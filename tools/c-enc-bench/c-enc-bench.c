/* M5.10 fair encoder-only benchmark driver (benchmark-only, isolated).
 *
 * Reuses the production FDK AAC encoder (cpp/libAACenc) through the same
 * public API and parameter sequence as aac-enc.c (AAC-LC VBR-5
 * afterburner-on ADTS). No encoder algorithm is duplicated here.
 *
 * Timing boundary (comparable to Rust M58Mode::EncoderOnly):
 *   TIMED   = aacEncOpen + all SetParam + Encode(NULL) init + EncInfo +
 *             all PCM submission + frame processing + end-of-input flush.
 *   UNTIMED = WAV load/parse, process startup, output file write, stdout
 *             report, decoder validation, metrics. Output completeness is
 *             checked after timing; failures exit non-zero.
 *
 * M5.11: `C_BENCH_REPEAT=N` repeats the TIMED region N times with fresh
 * encoder state per iteration (open/close per iter, same in-memory PCM)
 * for profiler sample collection. Default (unset) behavior is unchanged.
 *
 * Usage: c-enc-bench in.wav out.aac
 * stdout: BENCH wall_ns=<n> frames=<f> bytes=<b> sr=<s> ch=<c> iters=<n>
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "libAACenc/include/aacenc_lib.h"
#include "wavreader.h"

static uint64_t now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000u + (uint64_t)ts.tv_nsec;
}

int main(int argc, char *argv[]) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s in.wav out.aac\n", argv[0]);
        return 2;
    }
    const char *infile = argv[1], *outfile = argv[2];

    /* --- UNTIMED: load WAV fully into memory --- */
    void *wav = wav_read_open(infile);
    if (!wav) { fprintf(stderr, "Unable to open wav %s\n", infile); return 1; }
    int format = 0, channels = 0, sample_rate = 0, bits = 0;
    if (!wav_get_header(wav, &format, &channels, &sample_rate, &bits, NULL)) {
        fprintf(stderr, "Bad wav %s\n", infile); return 1;
    }
    if (format != 1 || bits != 16) {
        fprintf(stderr, "Unsupported WAV format=%d bits=%d\n", format, bits);
        return 1;
    }
    CHANNEL_MODE mode;
    switch (channels) {
    case 1: mode = MODE_1; break;
    case 2: mode = MODE_2; break;
    default: fprintf(stderr, "Unsupported channels %d\n", channels); return 1;
    }
    size_t cap = 1 << 20, len = 0;
    uint8_t *raw = malloc(cap);
    if (!raw) return 1;
    for (;;) {
        if (len + 65536 > cap) {
            cap *= 2;
            uint8_t *n = realloc(raw, cap);
            if (!n) return 1;
            raw = n;
        }
        int r = wav_read_data(wav, raw + len, 65536);
        if (r <= 0) break;
        len += (size_t)r;
    }
    wav_read_close(wav);
    size_t nsamples_total = len / 2; /* int16 samples incl. channels */
    int16_t *pcm = malloc(nsamples_total * 2 + 16);
    if (!pcm) return 1;
    for (size_t i = 0; i < nsamples_total; i++)
        pcm[i] = (int16_t)(raw[2 * i] | (raw[2 * i + 1] << 8));
    free(raw);

    long repeat = 1;
    {
        const char *e = getenv("C_BENCH_REPEAT");
        if (e) {
            repeat = atol(e);
            if (repeat < 1) repeat = 1;
        }
    }

    /* --- TIMED: fresh encoder init + all frame processing + flush --- */
    uint64_t t0 = now_ns();
    size_t frames = 0, out_len = 0;
    uint8_t *out = NULL;
    uint64_t check = 0;
    for (long iter = 0; iter < repeat; iter++) {
        HANDLE_AACENCODER handle;
        if (aacEncOpen(&handle, 0, channels) != AACENC_OK) {
            fprintf(stderr, "Unable to open encoder\n"); return 1;
        }
        if (aacEncoder_SetParam(handle, AACENC_SAMPLERATE, sample_rate) != AACENC_OK ||
            aacEncoder_SetParam(handle, AACENC_CHANNELMODE, mode) != AACENC_OK ||
            aacEncoder_SetParam(handle, AACENC_CHANNELORDER, 1) != AACENC_OK ||
            aacEncoder_SetParam(handle, AACENC_AOT, 2) != AACENC_OK ||
            aacEncoder_SetParam(handle, AACENC_BITRATEMODE, 5) != AACENC_OK ||
            aacEncoder_SetParam(handle, AACENC_TRANSMUX, TT_MP4_ADTS) != AACENC_OK ||
            aacEncoder_SetParam(handle, AACENC_AFTERBURNER, 1) != AACENC_OK) {
            fprintf(stderr, "Unable to set encoder params\n"); return 1;
        }
        if (aacEncEncode(handle, NULL, NULL, NULL, NULL) != AACENC_OK) {
            fprintf(stderr, "Unable to initialize the encoder\n"); return 1;
        }
        AACENC_InfoStruct info = {0};
        if (aacEncInfo(handle, &info) != AACENC_OK) {
            fprintf(stderr, "Unable to get encoder info\n"); return 1;
        }
        size_t frame_samples = (size_t)info.frameLength * (size_t)channels;
        size_t out_cap = 1 << 20, ilen = 0, iframes = 0;
        uint8_t *ibuf = malloc(out_cap);
        if (!ibuf) return 1;
        size_t pos = 0; /* int16 index into pcm */
        int flushing = 0, done = 0;
        while (!done) {
            AACENC_BufDesc in_buf = {0}, out_buf = {0};
            AACENC_InArgs in_args = {0};
            AACENC_OutArgs out_args = {0};
            int in_id = IN_AUDIO_DATA, out_id = OUT_BITSTREAM_DATA;
            int in_size, in_elem_size = 2, out_size, out_elem_size = 1;
            void *in_ptr, *out_ptr;
            uint8_t outbuf[20480];
            int16_t *chunk;
            size_t avail = (pos < nsamples_total) ? (nsamples_total - pos) : 0;
            size_t take;
            if (!flushing && avail > 0) {
                take = avail < frame_samples ? avail : frame_samples;
                chunk = pcm + pos;
                pos += take;
                /* NOTE: numInSamples counts ALL int16 samples across
                 * channels (aac-enc.c passes read/2 where read is bytes). */
                in_args.numInSamples = (int)take;
                if (pos >= nsamples_total) flushing = 1;
            } else {
                chunk = NULL;
                take = 0;
                in_args.numInSamples = -1;
            }
            in_ptr = chunk;
            in_size = (int)(take * 2);
            in_buf.numBufs = 1;
            in_buf.bufs = &in_ptr;
            in_buf.bufferIdentifiers = &in_id;
            in_buf.bufSizes = &in_size;
            in_buf.bufElSizes = &in_elem_size;
            out_ptr = outbuf;
            out_size = (int)sizeof(outbuf);
            out_buf.numBufs = 1;
            out_buf.bufs = &out_ptr;
            out_buf.bufferIdentifiers = &out_id;
            out_buf.bufSizes = &out_size;
            out_buf.bufElSizes = &out_elem_size;
            AACENC_ERROR err = aacEncEncode(handle, &in_buf, &out_buf, &in_args, &out_args);
            if (err == AACENC_ENCODE_EOF) break;
            if (err != AACENC_OK) {
                fprintf(stderr, "Encoding failed\n"); return 1;
            }
            if (out_args.numOutBytes > 0) {
                if (ilen + (size_t)out_args.numOutBytes > out_cap) {
                    while (ilen + (size_t)out_args.numOutBytes > out_cap) out_cap *= 2;
                    uint8_t *n = realloc(ibuf, out_cap);
                    if (!n) return 1;
                    ibuf = n;
                }
                memcpy(ibuf + ilen, outbuf, (size_t)out_args.numOutBytes);
                ilen += (size_t)out_args.numOutBytes;
                iframes++;
            }
        }
        aacEncClose(&handle);
        check += ilen + iframes;
        if (iter + 1 < repeat) {
            free(ibuf);
        } else {
            out = ibuf;
            out_len = ilen;
            frames = iframes;
        }
    }
    uint64_t t1 = now_ns();
    if (check == 0) { fprintf(stderr, "no output produced\n"); return 1; }

    /* --- UNTIMED: completeness check + file write + report --- */
    if (frames == 0 || out_len < 7) {
        fprintf(stderr, "Incomplete output: frames=%zu bytes=%zu\n", frames, out_len);
        return 1;
    }
    /* ADTS sync/length walk (same check as Rust harness post-timing). */
    {
        size_t off = 0, n = 0;
        while (off + 7 <= out_len) {
            if (out[off] != 0xFF || (out[off + 1] & 0xF0) != 0xF0) {
                fprintf(stderr, "ADTS sync lost @ %zu\n", off); return 1;
            }
            size_t fl = ((size_t)(out[off + 3] & 0x03) << 11) |
                        ((size_t)out[off + 4] << 3) | ((size_t)out[off + 5] >> 5);
            if (fl < 7) { fprintf(stderr, "bad frame len\n"); return 1; }
            off += fl; n++;
        }
        if (off != out_len || n != frames) {
            fprintf(stderr, "frame accounting mismatch (%zu/%zu)\n", n, frames);
            return 1;
        }
    }
    FILE *f = fopen(outfile, "wb");
    if (!f) { perror(outfile); return 1; }
    fwrite(out, 1, out_len, f);
    fclose(f);
    printf("BENCH wall_ns=%llu frames=%zu bytes=%zu sr=%d ch=%d iters=%ld\n",
           (unsigned long long)(t1 - t0), frames, out_len, sample_rate, channels, repeat);
    free(out);
    free(pcm);
    return 0;
}
