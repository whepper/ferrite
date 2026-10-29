// Focused FDK block-switch diagnostic probe.
// Builds against the pinned reference tree. Prints pre-sync per-channel state
// and post-sync sequence/grouping for frames 100..115 of a 48 kHz stereo WAV.
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

#include "block_switch.h"

extern "C" {
#include "wavreader.h"
}

static const int FRAME_LEN = 1024;

static void print_q31_array(const char* tag, int ch, const FIXP_DBL* a, int n) {
  std::printf("%s ch=%d", tag, ch);
  for (int i = 0; i < n; i++) std::printf(" %d", (int)a[i]);
  std::printf("\n");
}

int main(int argc, char** argv) {
  if (argc != 2) {
    std::fprintf(stderr, "usage: %s in.wav\n", argv[0]);
    return 2;
  }
  void* wav = wav_read_open(argv[1]);
  if (!wav) {
    std::fprintf(stderr, "cannot open %s\n", argv[1]);
    return 2;
  }
  int format = 0, channels = 0, sample_rate = 0, bits = 0;
  unsigned int data_length = 0;
  int hr = wav_get_header(wav, &format, &channels, &sample_rate, &bits,
                          &data_length);
  std::fprintf(stderr, "header rc=%d format=%d ch=%d sr=%d bits=%d data=%u\n",
               hr, format, channels, sample_rate, bits, data_length);
  if (hr == 0 || channels != 2 || bits != 16) {
    std::fprintf(stderr, "unsupported WAV\n");
    return 2;
  }
  std::vector<short> interleaved;
  unsigned char buf[4096];
  int nread;
  while ((nread = wav_read_data(wav, buf, sizeof(buf))) > 0) {
    for (int i = 0; i + 1 < nread; i += 2)
      interleaved.push_back((short)(buf[i] | (buf[i + 1] << 8)));
  }
  wav_read_close(wav);

  const long total_per_ch = (long)interleaved.size() / channels;
  const long n_frames = (total_per_ch + FRAME_LEN - 1) / FRAME_LEN + 3;
  std::vector<std::vector<short> > pcm(
      channels, std::vector<short>(n_frames * FRAME_LEN, 0));
  for (long i = 0; i < total_per_ch; i++)
    for (int c = 0; c < channels; c++)
      pcm[c][i] = interleaved[(size_t)i * channels + c];

  BLOCK_SWITCHING_CONTROL bs[2];
  for (int c = 0; c < channels; c++) FDKaacEnc_InitBlockSwitching(&bs[c], 0);

  INT_PCM fresh[2][FRAME_LEN];
  for (long f = 0; f < n_frames; f++) {
    for (int c = 0; c < channels; c++) {
      std::memcpy(fresh[c], &pcm[c][(size_t)f * FRAME_LEN],
                  FRAME_LEN * sizeof(INT_PCM));
      FDKaacEnc_BlockSwitching(&bs[c], FRAME_LEN, 0, fresh[c]);
    }
    if (f <= 115) {
      for (int c = 0; c < channels; c++) {
        std::printf(
            "PRE f=%ld ch=%d seq=%d shape=%d attack=%d lastattack=%d "
            "aidx=%d laidx=%d groups=%d acc=%d max=%d iir0=%d iir1=%d\n",
            f, c, (int)bs[c].lastWindowSequence, (int)bs[c].windowShape,
            (int)bs[c].attack, (int)bs[c].lastattack, (int)bs[c].attackIndex,
            (int)bs[c].lastAttackIndex, (int)bs[c].noOfGroups,
            (int)bs[c].accWindowNrg, (int)bs[c].maxWindowNrg,
            (int)bs[c].iirStates[0], (int)bs[c].iirStates[1]);
        print_q31_array("NRG", c, bs[c].windowNrg[1], 8);
        print_q31_array("NRGF", c, bs[c].windowNrgF[1], 8);
        print_q31_array("NRG0", c, bs[c].windowNrg[0], 8);
        print_q31_array("NRGF0", c, bs[c].windowNrgF[0], 8);
      }
      if (FDKaacEnc_SyncBlockSwitching(&bs[0], &bs[1], 2, 1) != 0) {
        std::fprintf(stderr, "sync failed at %ld\n", f);
        return 1;
      }
      for (int c = 0; c < channels; c++) {
        std::printf(
            "POST f=%ld ch=%d seq=%d shape=%d groups=%d gl=%d,%d,%d,%d\n",
            f, c, (int)bs[c].lastWindowSequence, (int)bs[c].windowShape,
            (int)bs[c].noOfGroups, (int)bs[c].groupLen[0],
            (int)bs[c].groupLen[1], (int)bs[c].groupLen[2],
            (int)bs[c].groupLen[3]);
      }
    } else {
      FDKaacEnc_SyncBlockSwitching(&bs[0], &bs[1], 2, 1);
    }
  }
  return 0;
}
