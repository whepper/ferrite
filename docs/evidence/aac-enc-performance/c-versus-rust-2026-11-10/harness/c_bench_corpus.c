/* Fair C-side corpus benchmark for the FDK AAC-LC VBR-5 encoder.
 *
 * Benchmark-only; no encoder algorithm is duplicated here. Uses the public
 * libAACenc API with the same parameter sequence as the stripped `aac-enc.c`
 * (AAC-LC, VBR mode 5, ADTS, afterburner on, WAV channel order).
 *
 * Timing boundary (matches the Rust `bench_corpus` harness):
 *   TIMED   = aacEncOpen + all SetParam + Encode(NULL) init + EncInfo +
 *             all PCM submission + frame processing + end-of-input flush.
 *   UNTIMED = WAV load/parse, process startup, output buffer allocation,
 *             aacEncClose, ADTS validation, hashing, output file write.
 *
 * Each measured repetition constructs a fresh encoder and processes the
 * fixture from the beginning. Rows are emitted per repetition so the caller
 * can form complete-corpus per-repetition sums.
 *
 * Usage:
 *   c_bench_corpus manifest.tsv outdir reps warmup
 *
 * Manifest: one `stem<TAB>wav_path` per line. WAV must be 16-bit PCM,
 * mono or stereo. Output rows:
 *   CFG  stem sr ch samples frameLength encoderDelay bitRate maxOutBufBytes
 *   WARM stem rep init_ns encode_ns close_ns frames bytes hash
 *   ROW  stem rep init_ns encode_ns close_ns frames bytes hash
 *   VAL  stem ok frames bytes hash
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "libAACenc/include/aacenc_lib.h"
#include "wavreader.h"

#define MAX_WORKLOADS 32
#define MAX_STEM 128
#define MAX_PATH 1024

typedef struct {
    char stem[MAX_STEM];
    char path[MAX_PATH];
    int16_t *pcm;
    size_t nsamples_total; /* int16 samples including channels */
    int sample_rate;
    int channels;
    uint8_t *out;
    size_t out_cap;
    size_t out_len;
} Workload;

static uint64_t now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000u + (uint64_t)ts.tv_nsec;
}

static uint64_t fnv1a64(const uint8_t *bytes, size_t len) {
    uint64_t h = 0xcbf29ce484222325ull;
    for (size_t i = 0; i < len; i++) {
        h ^= (uint64_t)bytes[i];
        h *= 0x00000100000001b3ull;
    }
    return h;
}

static int load_wav(Workload *w) {
    void *wav = wav_read_open(w->path);
    if (!wav) {
        fprintf(stderr, "Unable to open wav %s\n", w->path);
        return 1;
    }
    int format = 0, channels = 0, sample_rate = 0, bits = 0;
    if (!wav_get_header(wav, &format, &channels, &sample_rate, &bits, NULL)) {
        fprintf(stderr, "Bad wav %s\n", w->path);
        wav_read_close(wav);
        return 1;
    }
    if (format != 1 || bits != 16 || (channels != 1 && channels != 2)) {
        fprintf(stderr, "Unsupported WAV %s format=%d bits=%d ch=%d\n",
                w->path, format, bits, channels);
        wav_read_close(wav);
        return 1;
    }
    size_t cap = 1 << 20, len = 0;
    uint8_t *raw = malloc(cap);
    if (!raw) {
        wav_read_close(wav);
        return 1;
    }
    for (;;) {
        if (len + 65536 > cap) {
            cap *= 2;
            uint8_t *n = realloc(raw, cap);
            if (!n) {
                free(raw);
                wav_read_close(wav);
                return 1;
            }
            raw = n;
        }
        int r = wav_read_data(wav, raw + len, 65536);
        if (r <= 0) break;
        len += (size_t)r;
    }
    wav_read_close(wav);
    if (len == 0 || (len & 1)) {
        fprintf(stderr, "Bad PCM length %zu in %s\n", len, w->path);
        free(raw);
        return 1;
    }
    size_t n = len / 2;
    int16_t *pcm = malloc(n * sizeof(int16_t) + 16);
    if (!pcm) {
        free(raw);
        return 1;
    }
    for (size_t i = 0; i < n; i++)
        pcm[i] = (int16_t)(raw[2 * i] | (raw[2 * i + 1] << 8));
    free(raw);
    w->pcm = pcm;
    w->nsamples_total = n;
    w->sample_rate = sample_rate;
    w->channels = channels;
    w->out_cap = n * 2 + (1 << 20);
    w->out = malloc(w->out_cap);
    if (!w->out) {
        free(pcm);
        w->pcm = NULL;
        return 1;
    }
    return 0;
}

static int walk_adts(const uint8_t *bytes, size_t len, size_t *frames_out) {
    size_t off = 0, frames = 0;
    while (off + 7 <= len) {
        if (bytes[off] != 0xFF || (bytes[off + 1] & 0xF0) != 0xF0) {
            fprintf(stderr, "ADTS sync lost @ %zu\n", off);
            return 1;
        }
        size_t fl = ((size_t)(bytes[off + 3] & 0x03) << 11) |
                    ((size_t)bytes[off + 4] << 3) |
                    ((size_t)bytes[off + 5] >> 5);
        if (fl < 7 || off + fl > len) {
            fprintf(stderr, "bad frame len %zu @ %zu (total %zu)\n", fl, off, len);
            return 1;
        }
        off += fl;
        frames++;
    }
    if (off != len) {
        fprintf(stderr, "trailing bytes: off=%zu len=%zu\n", off, len);
        return 1;
    }
    *frames_out = frames;
    return 0;
}

static int encode_once(Workload *w, int rep, int measured) {
    size_t ilen = 0, frames = 0;
    uint64_t t0 = now_ns();

    HANDLE_AACENCODER handle;
    if (aacEncOpen(&handle, 0, (UINT)w->channels) != AACENC_OK) {
        fprintf(stderr, "Unable to open encoder\n");
        return 1;
    }
    CHANNEL_MODE mode = (w->channels == 1) ? MODE_1 : MODE_2;
    if (aacEncoder_SetParam(handle, AACENC_SAMPLERATE, (UINT)w->sample_rate) != AACENC_OK ||
        aacEncoder_SetParam(handle, AACENC_CHANNELMODE, mode) != AACENC_OK ||
        aacEncoder_SetParam(handle, AACENC_CHANNELORDER, 1) != AACENC_OK ||
        aacEncoder_SetParam(handle, AACENC_AOT, 2) != AACENC_OK ||
        aacEncoder_SetParam(handle, AACENC_BITRATEMODE, 5) != AACENC_OK ||
        aacEncoder_SetParam(handle, AACENC_TRANSMUX, TT_MP4_ADTS) != AACENC_OK ||
        aacEncoder_SetParam(handle, AACENC_AFTERBURNER, 1) != AACENC_OK) {
        fprintf(stderr, "Unable to set encoder params\n");
        aacEncClose(&handle);
        return 1;
    }
    if (aacEncEncode(handle, NULL, NULL, NULL, NULL) != AACENC_OK) {
        fprintf(stderr, "Unable to initialize the encoder\n");
        aacEncClose(&handle);
        return 1;
    }
    AACENC_InfoStruct info = {0};
    if (aacEncInfo(handle, &info) != AACENC_OK) {
        fprintf(stderr, "Unable to get encoder info\n");
        aacEncClose(&handle);
        return 1;
    }
    uint64_t t_init = now_ns();

    size_t frame_samples = (size_t)info.frameLength * (size_t)w->channels;
    size_t pos = 0;
    int flushing = 0, done = 0;
    while (!done) {
        AACENC_BufDesc in_buf = {0}, out_buf = {0};
        AACENC_InArgs in_args = {0};
        AACENC_OutArgs out_args = {0};
        int in_id = IN_AUDIO_DATA, out_id = OUT_BITSTREAM_DATA;
        int in_size, in_elem_size = 2, out_size, out_elem_size = 1;
        void *in_ptr, *out_ptr;
        uint8_t outbuf[20480];
        size_t avail = (pos < w->nsamples_total) ? (w->nsamples_total - pos) : 0;
        size_t take;
        if (!flushing && avail > 0) {
            take = avail < frame_samples ? avail : frame_samples;
            in_ptr = w->pcm + pos;
            pos += take;
            in_args.numInSamples = (INT)take;
            if (pos >= w->nsamples_total) flushing = 1;
        } else {
            in_ptr = NULL;
            take = 0;
            in_args.numInSamples = -1;
            flushing = 1;
        }
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
            fprintf(stderr, "Encoding failed: %d\n", (int)err);
            aacEncClose(&handle);
            return 1;
        }
        if (out_args.numOutBytes > 0) {
            size_t n = (size_t)out_args.numOutBytes;
            if (ilen + n > w->out_cap) {
                fprintf(stderr, "output overflow (%zu + %zu > %zu)\n", ilen, n, w->out_cap);
                aacEncClose(&handle);
                return 1;
            }
            memcpy(w->out + ilen, outbuf, n);
            ilen += n;
            frames++;
        }
        if (in_args.numInSamples == -1 && out_args.numOutBytes == 0) done = 1;
    }
    uint64_t t_enc = now_ns();
    aacEncClose(&handle);
    uint64_t t_close = now_ns();

    size_t adts_frames = 0;
    if (walk_adts(w->out, ilen, &adts_frames) != 0) {
        fprintf(stderr, "ADTS validation failed for %s\n", w->stem);
        return 1;
    }
    if (adts_frames != frames) {
        fprintf(stderr, "frame accounting mismatch %zu/%zu for %s\n",
                adts_frames, frames, w->stem);
        return 1;
    }
    uint64_t out_hash = fnv1a64(w->out, ilen);
    uint64_t in_hash = fnv1a64((const uint8_t *)w->pcm,
                               w->nsamples_total * sizeof(int16_t));

    printf("%s\t%s\t%d\t%llu\t%llu\t%llu\t%zu\t%zu\t%016llx\n",
           measured ? "ROW" : "WARM", w->stem, rep,
           (unsigned long long)(t_init - t0),
           (unsigned long long)(t_enc - t_init),
           (unsigned long long)(t_close - t_enc),
           frames, ilen, (unsigned long long)out_hash);
    if (measured && rep == 0) {
        printf("CFG\t%s\t%d\t%d\t%zu\t%d\t%d\t%d\t%d\n",
               w->stem, w->sample_rate, w->channels,
               w->nsamples_total / (size_t)w->channels,
               (int)info.frameLength, (int)info.nDelay,
               (int)info.maxOutBufBytes, (int)info.confSize);
        printf("IN\t%s\t%016llx\n", w->stem, (unsigned long long)in_hash);
    }
    w->out_len = ilen;
    return 0;
}

int main(int argc, char *argv[]) {
    if (argc != 5) {
        fprintf(stderr, "usage: %s manifest.tsv outdir reps warmup\n", argv[0]);
        return 2;
    }
    const char *manifest = argv[1];
    const char *outdir = argv[2];
    int reps = atoi(argv[3]);
    int warmup = atoi(argv[4]);
    if (reps < 1 || warmup < 0) {
        fprintf(stderr, "bad reps/warmup\n");
        return 2;
    }

    FILE *mf = fopen(manifest, "r");
    if (!mf) {
        perror(manifest);
        return 1;
    }
    Workload works[MAX_WORKLOADS];
    int nwork = 0;
    char line[2048];
    while (fgets(line, sizeof(line), mf)) {
        char *nl = strchr(line, '\n');
        if (nl) *nl = '\0';
        if (line[0] == '\0' || line[0] == '#') continue;
        char *tab = strchr(line, '\t');
        if (!tab) {
            fprintf(stderr, "bad manifest line: %s\n", line);
            return 1;
        }
        *tab = '\0';
        if (nwork >= MAX_WORKLOADS) {
            fprintf(stderr, "too many workloads\n");
            return 1;
        }
        Workload *w = &works[nwork++];
        memset(w, 0, sizeof(*w));
        snprintf(w->stem, sizeof(w->stem), "%s", line);
        snprintf(w->path, sizeof(w->path), "%s", tab + 1);
        char *path_tab = strchr(w->path, '\t');
        if (path_tab) *path_tab = '\0';
        if (load_wav(w) != 0) return 1;
    }
    fclose(mf);
    if (nwork == 0) {
        fprintf(stderr, "empty manifest\n");
        return 1;
    }

    /* Warm-up: one complete pass, one fresh encoder per workload. */
    for (int wi = 0; wi < nwork; wi++) {
        if (encode_once(&works[wi], 0, 0) != 0) return 1;
    }

    /* Measured: repetition-major, workload-minor, matching the Rust harness. */
    for (int rep = 0; rep < reps; rep++) {
        for (int wi = 0; wi < nwork; wi++) {
            if (encode_once(&works[wi], rep, 1) != 0) return 1;
        }
    }

    /* Untimed output write, after all repetitions, matching the Rust harness. */
    for (int wi = 0; wi < nwork; wi++) {
        char path[MAX_PATH + 32];
        snprintf(path, sizeof(path), "%s/%s.c.aac", outdir, works[wi].stem);
        FILE *f = fopen(path, "wb");
        if (!f) {
            perror(path);
            return 1;
        }
        if (fwrite(works[wi].out, 1, works[wi].out_len, f) != works[wi].out_len) {
            perror("fwrite");
            fclose(f);
            return 1;
        }
        fclose(f);
    }

    for (int wi = 0; wi < nwork; wi++) {
        free(works[wi].pcm);
        free(works[wi].out);
    }
    printf("DONE\t%d\n", nwork);
    return 0;
}
