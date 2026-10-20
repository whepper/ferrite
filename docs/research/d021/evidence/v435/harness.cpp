/* V4.35 isolated C harness: runs the REAL mdct_block (fixed-point) on the
 * committed ring, steady-SHORT state. Scratch only; never in the repo. */
#include <stdio.h>
#include <string.h>
#include "mdct.h"
#include "FDK_tools_rom.h"

int main(int argc, char **argv) {
  static INT_PCM pcm[2048];
  static FIXP_DBL out[1024];
  static SHORT exps[8];
  FILE *f;
  int i, n;
  mdct_t st;

  if (argc < 2) return 1;
  f = fopen(argv[1], "r");
  if (!f) return 2;
  for (i = 0; i < 2048; i++) {
    if (fscanf(f, "%hd", &pcm[i]) != 1) return 3;
  }
  fclose(f);

  memset(&st, 0, sizeof(st));
  st.prev_wrs = FDKgetWindowSlope(128, 0);
  st.prev_fr = 128;
  st.prev_tl = 128;

  n = mdct_block(&st, pcm, 1024, out, 8, 128, FDKgetWindowSlope(128, 0), 128, exps);
  printf("NSPEC %d\n", n);
  printf("EXPS");
  for (i = 0; i < 8; i++) printf(" %d", (int)exps[i]);
  printf("\nOUT");
  for (i = 0; i < 1024; i++) printf(" %d", (int)out[i]);
  printf("\n");
  return 0;
}
