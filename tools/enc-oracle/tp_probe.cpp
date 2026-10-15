/* Test-only transport probe (M4-S6 audit evidence): verify ADTS
 * globHdrBits via the REAL transportEnc. Result: 56 bits for mono/stereo
 * ADTS regardless of auBits (PCE absent for SCE/CPE, protection_absent=1).
 * The S6 oracle and Rust port take globHdrBits as a transport-owned input;
 * this probe is the provenance of the constant, not a build dependency.
 * Build: c++ -std=c++11 -O2 -I cpp/libAACenc/src -I cpp/libFDK/include
 *   -I cpp/libSYS/include -I cpp/libMpegTPEnc/include -I .
 *   tools/enc-oracle/tp_probe.cpp -L "$BLD" -lfdk-aac
 *   -Wl,-rpath,"$BLD" -o "$BLD/tp_probe" && "$BLD/tp_probe"
 */
#include <cstdio>
#include <cstring>
#include "tpenc_lib.h"
#include "FDK_audio.h"

static int check(CHANNEL_MODE mode, int sr, int ch, int br) {
  HANDLE_TRANSPORTENC hTp = nullptr;
  if (transportEnc_Open(&hTp) != TRANSPORTENC_OK) {
    std::printf("open failed\n");
    return 1;
  }
  CODER_CONFIG cc;
  std::memset(&cc, 0, sizeof(cc));
  cc.channelMode = mode;
  cc.aot = AOT_AAC_LC;
  cc.channelConfigZero = 0;
  cc.samplingRate = sr;
  cc.extSamplingRate = sr;
  cc.bitRate = br;
  cc.samplesPerFrame = 1024;
  cc.noChannels = ch;
  cc.nSubFrames = 1;
  cc.flags = 0;
  cc.matrixMixdownA = 0;
  cc.headerPeriod = 10; /* DEFAULT_HEADER_PERIOD_REPETITION_RATE-ish (>0) */
  static unsigned char buf[65536];
  if (transportEnc_Init(hTp, buf, sizeof(buf), TT_MP4_ADTS, &cc, 0) !=
      TRANSPORTENC_OK) {
    std::printf("init failed\n");
    return 1;
  }
  int b1 = transportEnc_GetStaticBits(hTp, 4096);
  int b2 = transportEnc_GetStaticBits(hTp, 12288);
  std::printf("mode=%d sr=%d ch=%d br=%d staticBits(4096)=%d staticBits(12288)=%d\n",
              (int)mode, sr, ch, br, b1, b2);
  transportEnc_Close(&hTp);
  return 0;
}

int main() {
  check(MODE_1, 48000, 1, 112000);
  check(MODE_2, 48000, 2, 192000);
  check(MODE_2, 44100, 2, 192000);
  return 0;
}
