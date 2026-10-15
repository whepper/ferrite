/* Test-only S7 bitstream-encode probe (bit_cnt.cpp encode half).
 * Calls the REAL `FDKaacEnc_codeValues` / `FDKaacEnc_codeScalefactorDelta`
 * through a real FDK writer bitstream and dumps emitted bits as hex
 * (MSB-first bytes, zero-padded tail) plus exact bit counts.
 * Also prints every code table the encode half reads, for the
 * scripted table extraction (cross-checked against aacEnc_rom.cpp text).
 *
 * Build: c++ -std=c++11 -O2 -I cpp/libAACenc/src -I cpp/libFDK/include \
 *   -I cpp/libSYS/include -I cpp/libMpegTPEnc/include -I . \
 *   tools/enc-oracle/bitenc_probe.cpp -L "$BLD" -lfdk-aac \
 *   -Wl,-rpath,"$BLD" -o "$BLD/bitenc_probe"
 * Run: "$BLD/bitenc_probe" > "$BLD/s7probe.txt"   (deterministic)
 * Table extraction: tools/enc-oracle/extract_bitenc_tables.py
 */
#include <cstdio>
#include <cstring>

#include "FDK_bitstream.h"
#include "aacEnc_rom.h"
#include "bit_cnt.h"

static UCHAR s_buf[2048];
static HANDLE_FDK_BITSTREAM s_bs = NULL;

static void resetBs() {
  std::memset(s_buf, 0, sizeof(s_buf));
  if (s_bs == NULL) {
    s_bs = FDKcreateBitStream(s_buf, sizeof(s_buf), BS_WRITER);
  } else {
    FDKresetBitbuffer(s_bs, BS_WRITER);
    /* reset re-attaches nothing; re-init onto the zeroed buffer */
    FDKinitBitStream(s_bs, s_buf, sizeof(s_buf), 0, BS_WRITER);
  }
}

static void dumpHex(const char *tag) {
  FDKsyncCache(s_bs);
  unsigned valid = FDKgetValidBits(s_bs);
  unsigned nbytes = (valid + 7) / 8;
  std::printf("%s bits=%u hex=", tag, valid);
  for (unsigned i = 0; i < nbytes; i++) std::printf("%02x", s_buf[i]);
  std::printf("\n");
}

static void runCv(int book, const short *vals, int n) {
  resetBs();
  /* codeValues takes non-const SHORT*; copy to a scratch (never mutated
     for books 0..11, but the signature is non-const). */
  static short scratch[64];
  for (int i = 0; i < n; i++) scratch[i] = vals[i];
  FDKaacEnc_codeValues(scratch, n, book, s_bs);
  char tag[160];
  std::snprintf(tag, sizeof(tag), "CV book=%d n=%d vals=", book, n);
  /* tag is prefix-only; values printed by caller convention below */
  (void)tag;
  FDKsyncCache(s_bs);
  unsigned valid = FDKgetValidBits(s_bs);
  unsigned nbytes = (valid + 7) / 8;
  std::printf("CV book=%d n=%d vals=", book, n);
  for (int i = 0; i < n; i++)
    std::printf("%s%d", i ? "," : "", (int)vals[i]);
  std::printf(" bits=%u hex=", valid);
  for (unsigned i = 0; i < nbytes; i++) std::printf("%02x", s_buf[i]);
  std::printf("\n");
}

static void runCs(int delta) {
  resetBs();
  int rc = FDKaacEnc_codeScalefactorDelta(delta, s_bs);
  FDKsyncCache(s_bs);
  unsigned valid = FDKgetValidBits(s_bs);
  unsigned nbytes = (valid + 7) / 8;
  std::printf("CS delta=%d rc=%d bits=%u hex=", delta, rc, valid);
  for (unsigned i = 0; i < nbytes; i++) std::printf("%02x", s_buf[i]);
  std::printf("\n");
}

int main() {
  /* ---- code tables, C declaration order, flattened row-major ---- */
  std::printf("S7TABLES\n");
  std::printf("CTAB1");
  for (int a = 0; a < 3; a++)
    for (int b = 0; b < 3; b++)
      for (int c = 0; c < 3; c++)
        for (int d = 0; d < 3; d++)
          std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab1[a][b][c][d]);
  std::printf("\nCTAB2");
  for (int a = 0; a < 3; a++)
    for (int b = 0; b < 3; b++)
      for (int c = 0; c < 3; c++)
        for (int d = 0; d < 3; d++)
          std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab2[a][b][c][d]);
  std::printf("\nCTAB3");
  for (int a = 0; a < 3; a++)
    for (int b = 0; b < 3; b++)
      for (int c = 0; c < 3; c++)
        for (int d = 0; d < 3; d++)
          std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab3[a][b][c][d]);
  std::printf("\nCTAB4");
  for (int a = 0; a < 3; a++)
    for (int b = 0; b < 3; b++)
      for (int c = 0; c < 3; c++)
        for (int d = 0; d < 3; d++)
          std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab4[a][b][c][d]);
  std::printf("\nCTAB5");
  for (int a = 0; a < 9; a++)
    for (int b = 0; b < 9; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab5[a][b]);
  std::printf("\nCTAB6");
  for (int a = 0; a < 9; a++)
    for (int b = 0; b < 9; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab6[a][b]);
  std::printf("\nCTAB7");
  for (int a = 0; a < 8; a++)
    for (int b = 0; b < 8; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab7[a][b]);
  std::printf("\nCTAB8");
  for (int a = 0; a < 8; a++)
    for (int b = 0; b < 8; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab8[a][b]);
  std::printf("\nCTAB9");
  for (int a = 0; a < 13; a++)
    for (int b = 0; b < 13; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab9[a][b]);
  std::printf("\nCTAB10");
  for (int a = 0; a < 13; a++)
    for (int b = 0; b < 13; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab10[a][b]);
  /* ctab11 is [21][17]; the encoder only indexes rows/cols 0..16
     (t = min(|v|,16)); all 21 rows printed, source cross-checked. */
  std::printf("\nCTAB11");
  for (int a = 0; a < 21; a++)
    for (int b = 0; b < 17; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ctab11[a][b]);
  std::printf("\nCTABSCF");
  for (int i = 0; i < 121; i++)
    std::printf(" %u", (unsigned)FDKaacEnc_huff_ctabscf[i]);
  std::printf("\nS7TABLES end\n");

  /* ---- leaf vectors: every book, signs, widths, escape boundaries ---- */
  std::printf("S7VECTORS\n");
  { const short v[] = {0, 0, 0, 0}; runCv(0, v, 4); }
  /* books 1,2 (values -1..1, width 4) */
  { const short v[] = {0, 0, 0, 0}; runCv(1, v, 4); runCv(2, v, 4); }
  { const short v[] = {1, 1, 1, 1}; runCv(1, v, 4); runCv(2, v, 4); }
  { const short v[] = {-1, -1, -1, -1}; runCv(1, v, 4); runCv(2, v, 4); }
  { const short v[] = {1, -1, 0, 1}; runCv(1, v, 4); runCv(2, v, 4); }
  { const short v[] = {-1, 0, 1, 0}; runCv(1, v, 4); runCv(2, v, 4); }
  { const short v[] = {1, 1, 1, 1, -1, -1, -1, -1}; runCv(1, v, 8); runCv(2, v, 8); }
  /* books 3,4 (abs <= 2, sign bits) */
  { const short v[] = {0, 0, 0, 0}; runCv(3, v, 4); runCv(4, v, 4); }
  { const short v[] = {1, 0, 0, 0}; runCv(3, v, 4); runCv(4, v, 4); }
  { const short v[] = {0, 0, 0, -1}; runCv(3, v, 4); runCv(4, v, 4); }
  { const short v[] = {2, -2, 2, -2}; runCv(3, v, 4); runCv(4, v, 4); }
  { const short v[] = {-2, -1, 1, 2}; runCv(3, v, 4); runCv(4, v, 4); }
  { const short v[] = {2, 0, -1, 0, 0, 2, 0, -2}; runCv(3, v, 8); runCv(4, v, 8); }
  /* books 5,6 (values -4..4, paired codewords) */
  { const short v[] = {0, 0, 0, 0}; runCv(5, v, 4); runCv(6, v, 4); }
  { const short v[] = {4, 4, 4, 4}; runCv(5, v, 4); runCv(6, v, 4); }
  { const short v[] = {-4, -4, -4, -4}; runCv(5, v, 4); runCv(6, v, 4); }
  { const short v[] = {0, 4, 0, -4}; runCv(5, v, 4); runCv(6, v, 4); }
  { const short v[] = {1, 2, 3, -4}; runCv(5, v, 4); runCv(6, v, 4); }
  { const short v[] = {4, -3, 2, -1, 0, 1, -2, 3}; runCv(5, v, 8); runCv(6, v, 8); }
  /* books 7,8 (abs <= 7, pairs) */
  { const short v[] = {0, 0}; runCv(7, v, 2); runCv(8, v, 2); }
  { const short v[] = {7, -7}; runCv(7, v, 2); runCv(8, v, 2); }
  { const short v[] = {-7, 7}; runCv(7, v, 2); runCv(8, v, 2); }
  { const short v[] = {1, 0}; runCv(7, v, 2); runCv(8, v, 2); }
  { const short v[] = {0, -1}; runCv(7, v, 2); runCv(8, v, 2); }
  { const short v[] = {7, 7, -7, -7}; runCv(7, v, 4); runCv(8, v, 4); }
  { const short v[] = {3, -5, 0, 7}; runCv(7, v, 4); runCv(8, v, 4); }
  /* books 9,10 (abs <= 12, pairs) */
  { const short v[] = {0, 0}; runCv(9, v, 2); runCv(10, v, 2); }
  { const short v[] = {12, -12}; runCv(9, v, 2); runCv(10, v, 2); }
  { const short v[] = {-12, 12}; runCv(9, v, 2); runCv(10, v, 2); }
  { const short v[] = {5, 0}; runCv(9, v, 2); runCv(10, v, 2); }
  { const short v[] = {0, -5}; runCv(9, v, 2); runCv(10, v, 2); }
  { const short v[] = {12, 12, -12, -12}; runCv(9, v, 4); runCv(10, v, 4); }
  /* book 11 base (no escape) + escape boundaries */
  { const short v[] = {0, 0}; runCv(11, v, 2); }
  { const short v[] = {1, -1}; runCv(11, v, 2); }
  { const short v[] = {-1, 1}; runCv(11, v, 2); }
  { const short v[] = {15, 15}; runCv(11, v, 2); }
  { const short v[] = {-15, -15}; runCv(11, v, 2); }
  { const short v[] = {16, 0}; runCv(11, v, 2); }
  { const short v[] = {0, 16}; runCv(11, v, 2); }
  { const short v[] = {-16, 16}; runCv(11, v, 2); }
  { const short v[] = {16, 16}; runCv(11, v, 2); }
  { const short v[] = {17, 0}; runCv(11, v, 2); }
  { const short v[] = {31, 31}; runCv(11, v, 2); }
  { const short v[] = {32, -32}; runCv(11, v, 2); }
  { const short v[] = {100, -200}; runCv(11, v, 2); }
  { const short v[] = {300, -400}; runCv(11, v, 2); }
  { const short v[] = {1000, 1000}; runCv(11, v, 2); }
  { const short v[] = {8191, -8191}; runCv(11, v, 2); }
  { const short v[] = {32767, -32767}; runCv(11, v, 2); }
  { const short v[] = {-32768, 32767}; runCv(11, v, 2); }
  { const short v[] = {16, 16, 0, 0}; runCv(11, v, 4); }
  { const short v[] = {20, -20, 0, 0}; runCv(11, v, 4); }
  /* escape-length sweep: each boundary value in all pair positions */
  {
    const int sweep[] = {15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128,
                         255, 256, 511, 512, 1023, 1024, 2047, 2048, 4095,
                         4096, 8191, 8192, 16383, 16384, 32767};
    for (unsigned k = 0; k < sizeof(sweep) / sizeof(sweep[0]); k++) {
      short v[2];
      v[0] = (short)sweep[k]; v[1] = 0; runCv(11, v, 2);
      v[0] = 0; v[1] = (short)sweep[k]; runCv(11, v, 2);
      v[0] = (short)-sweep[k]; v[1] = (short)sweep[k]; runCv(11, v, 2);
    }
  }
  /* invalid books: C default-break emits nothing (documents the branch
     Rust refuses explicitly). */
  { const short v[] = {1, 2, 3, 4}; runCv(12, v, 4); }
  { const short v[] = {1, 2, 3, 4}; runCv(14, v, 4); }
  { const short v[] = {1, 2}; runCv(-1, v, 2); }
  /* scalefactor deltas: full legal sweep + both over-range aborts */
  for (int d = -61; d <= 61; d++) runCs(d);
  std::printf("S7VECTORS end\n");

  (void)dumpHex;
  FDKdeleteBitStream(s_bs);
  return 0;
}
