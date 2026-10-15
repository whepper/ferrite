/* Test-only constant probe for S4 (sf_estim.cpp literals).
 * Prints FL2FXCONST_DBL values exactly as the reference build computes them.
 * Build: c++ -std=c++11 -O2 -I cpp/libFDK/include -I cpp/libSYS/include -I . tools/enc-oracle/scf_probe.cpp -o /tmp/scf_probe && /tmp/scf_probe
 */
#include <cstdio>
#include "common_fix.h"

#define P(tag, expr) std::printf("%s %d\n", tag, (int)(FIXP_DBL)(expr))

int main() {
  P("PE_C1", FL2FXCONST_DBL(3.0f / (float)(1 << 7)));
  P("PE_C2", FL2FXCONST_DBL(1.3219281f / (float)(1 << 7)));
  P("PE_C3", FL2FXCONST_DBL(0.5593573f));
  P("ASPEFAC", FL2FXCONST_DBL(0.109375));
  P("DISTFAC", FL2FXCONST_DBL(-0.0050301265));
  P("PEGATE", FL2FXCONST_DBL(0.0006103515625f));
  P("THRESHCONST", FL2FXCONST_DBL(0.04304511722f));
  P("CONVCONST", FL2FXCONST_DBL(0.30102999566f));
  P("C1CONST", FL2FXCONST_DBL(-0.27083183594f));
  P("NEG1", FL2FXCONST_DBL(-1.f));
  P("F009375", FL2FXCONST_DBL(0.09375f));
  P("F0021522", FL2FXCONST_DBL(0.02152255861f));
  P("F0375", FL2FXCONST_DBL(0.375f));
  P("F07", FL2FXCONST_DBL(0.7f));
  P("F08585", FL2FXCONST_DBL(0.8585f));
  P("THIRD", FL2FXCONST_DBL(1.0f / 3.0f));
  P("E3NEG", FL2FXCONST_DBL(0.15571537944));
  P("F08", FL2FXCONST_DBL(0.8f));
  P("D2594", FL2FXCONST_DBL(0.00259488556167f));
  P("D7787", FL2FXCONST_DBL(0.00778722686652f));
  P("ZERO", FL2FXCONST_DBL(0.0f));
  P("ONE", FL2FXCONST_DBL(1.0f));
  return 0;
}
