/* Test-only S6 literal probe (M4-S6): print exact Q31 integers for every
 * FL2FXCONST_DBL expression in the S6 port paths (adj_thr.cpp
 * DistributeBits family + bitres helpers). Expressions are evaluated
 * exactly as C evaluates them (float division first, then conversion).
 * Every literal in rust/src/aac_enc/rate.rs cites this probe.
 * Build: c++ -std=c++11 -O2 -I cpp/libAACenc/src -I cpp/libFDK/include
 *   -I cpp/libSYS/include -I cpp/libMpegTPEnc/include -I .
 *   tools/enc-oracle/s6lit_probe.cpp -L "$BLD" -lfdk-aac
 *   -Wl,-rpath,"$BLD" -o "$BLD/s6lit_probe" && "$BLD/s6lit_probe"
 */
#include <cstdio>
#include "fixpoint_math.h"

#define P(expr) std::printf("%-22s %d 0x%08x\n", #expr, (int)(expr), (unsigned)(expr))

int main() {
  P(FL2FXCONST_DBL(1.18f / (1 << 1)));
  P(FL2FXCONST_DBL(0.6f));
  P(FL2FXCONST_DBL(0.4f));
  P(FL2FXCONST_DBL(0.3f));
  P(FL2FXCONST_DBL(0.0f));
  P(FL2FXCONST_DBL(0.466666666f));
  P(FL2FXCONST_DBL(0.666666666f));
  P(FL2FXCONST_DBL(1.2f / 2.f));
  P(FL2FXCONST_DBL(0.65f));
  P(FL2FXCONST_DBL(1.1f / 2.f));
  P(FL2FXCONST_DBL(1.f / 2.f));
  P(FL2FXCONST_DBL(0.85f / 2.f));
  P(FL2FXCONST_DBL(0.9f / 2.f));
  P(FL2FXCONST_DBL(1.15f / 2.f));
  P(FL2FXCONST_DBL(1.f / 2.f));
  P(FL2FXCONST_DBL(0.85f));
  P(FL2FXCONST_DBL(0.15f));
  P(FL2FXCONST_DBL(0.7f));
  P(FL2FXCONST_DBL(0.3f));
  P(FL2FXCONST_DBL(0.75f / 2.f));
  P(FL2FXCONST_DBL(0.75f));
  P(FL2FXCONST_DBL(1.0f / 2.f));
  P(FL2FXCONST_DBL(0.005f));
  P(FL2FXCONST_DBL(0.25f));
  P(FL2FXCONST_DBL(0.3f));
  P(FL2FXCONST_DBL(0.14f));
  P(FL2FXCONST_DBL(0.07f));
  P(FL2FXCONST_DBL(0.1666666667f));
  P(FL2FXCONST_DBL(3.f / 4.f));
  P(FL2FXCONST_DBL(0.35f));
  P(FL2FXCONST_DBL(0.8f));
  P(FL2FXCONST_DBL(0.6f));
  P(FL2FXCONST_DBL(0.070f));
  P(FL2FXCONST_DBL(0.5f));
  return 0;
}
