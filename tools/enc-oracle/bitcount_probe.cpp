/* Test-only bitcount table probe for S5 (dyn_bits.cpp / bit_cnt.cpp).
 * Prints the exact Huffman length tables and side-info tables as the
 * reference build sees them (all plain integer data, no arch variants —
 * the probe still guards against silent table swaps).
 * Build: c++ -std=c++11 -O2 -I cpp/libAACenc/src -I cpp/libFDK/include -I cpp/libSYS/include -I cpp/libMpegTPEnc/include -I . tools/enc-oracle/bitcount_probe.cpp -L "$BLD" -lfdk-aac -Wl,-rpath,"$BLD" -o "$BLD/bitcount_probe" && "$BLD/bitcount_probe" > "$BLD/bctables.txt"
 * Extraction + cross-check vs aacEnc_rom.cpp source: tools/enc-oracle/extract_bitcount_tables.py
 */
#include <cstdio>

#include "aacEnc_rom.h"

int main() {
  std::printf("BCTABLES\n");
  std::printf("LTAB12");
  for (int a = 0; a < 3; a++)
    for (int b = 0; b < 3; b++)
      for (int c = 0; c < 3; c++)
        for (int d = 0; d < 3; d++)
          std::printf(" %u", (unsigned)FDKaacEnc_huff_ltab1_2[a][b][c][d]);
  std::printf("\nLTAB34");
  for (int a = 0; a < 3; a++)
    for (int b = 0; b < 3; b++)
      for (int c = 0; c < 3; c++)
        for (int d = 0; d < 3; d++)
          std::printf(" %u", (unsigned)FDKaacEnc_huff_ltab3_4[a][b][c][d]);
  std::printf("\nLTAB56");
  for (int a = 0; a < 9; a++)
    for (int b = 0; b < 9; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ltab5_6[a][b]);
  std::printf("\nLTAB78");
  for (int a = 0; a < 8; a++)
    for (int b = 0; b < 8; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ltab7_8[a][b]);
  std::printf("\nLTAB910");
  for (int a = 0; a < 13; a++)
    for (int b = 0; b < 13; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ltab9_10[a][b]);
  std::printf("\nLTAB11");
  for (int a = 0; a < 17; a++)
    for (int b = 0; b < 17; b++)
      std::printf(" %u", (unsigned)FDKaacEnc_huff_ltab11[a][b]);
  std::printf("\nSITABLONG");
  for (int i = 0; i < 52; i++)
    std::printf(" %d", (int)FDKaacEnc_sideInfoTabLong[i]);
  std::printf("\nSITABSHORT");
  for (int i = 0; i < 16; i++)
    std::printf(" %d", (int)FDKaacEnc_sideInfoTabShort[i]);
  std::printf("\nBCTABLES end\n");
  return 0;
}
