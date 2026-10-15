/* M4-S3 quantizer probe (test-only).
 *
 * Calls the UNMODIFIED reference quantizer (cpp/libAACenc/src/quantize.cpp)
 * through its exported entry points and prints deterministic transcripts:
 *  - the exact table/constant integers the build uses (proves the 16-bit
 *    FIXP_QTD selection on this target and feeds scripted extraction), and
 *  - synthetic single/multi-band quantize + distortion probes used as
 *    Rust regression anchors.
 *
 * Only exported functions are used (FDKaacEnc_quantizeLines /
 * FDKaacEnc_invQuantizeLines are static): FDKaacEnc_QuantizeSpectrum,
 * FDKaacEnc_calcSfbDist, FDKaacEnc_calcSfbQuantEnergyAndDist.
 *
 * Valid-envelope gate: every probe input is scanned BEFORE the real call
 * with the reference's own assert conditions replicated (FDK_ASSERT is a
 * no-op on ARM, so the probe enforces them explicitly and aborts loudly
 * instead of executing undefined shifts / out-of-bounds table reads):
 *  - quantize path: recomputed totalShift = 12 - 3*(T>>2) >= 0
 *    (quantize.cpp:146,162 MAX_QUANT_VIOLATION assert) for every line,
 *  - inverse path: |q| <= MAX_QUANT (8191) for every line
 *    (quantize.cpp:204,240 specExp < 14 assert).
 * Committed fixtures therefore stay valid on every architecture,
 * including x86 where the asserts are compiled in.
 *
 * Build (repo root):
 *   BLD=/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/fdk-aac-build
 *   c++ -std=c++11 -O2 -I cpp/libAACenc/src -I cpp/libFDK/include \
 *     -I cpp/libSYS/include -I cpp/libMpegTPEnc/include -I . \
 *     tools/enc-oracle/quant_probe.cpp -L "$BLD" -lfdk-aac \
 *     -Wl,-rpath,"$BLD" -o "$BLD/quant_probe"
 *   "$BLD/quant_probe" > "$BLD/qprobe.txt"
 */

#include <cstdio>
#include <cstdlib>

#include "aacEnc_rom.h"
#include "common_fix.h"
#include "fixpoint_math.h"
#include "quantize.h"

/* Replicated reference assert: totalShift as recomputed in quantizeLines
 * (quantize.cpp:145,161) must stay >= 0. Returns it for the scan. */
static int probe_total_shift(int gain, FIXP_DBL spec) {
  int qs = ((-gain) >> 2) + 1;
  /* First-stage accu exactly as fixmuldiv2_DS on this target:
   * fixmuldiv2_SD(q, spec) = fixmuldiv2_DD(q<<16, spec). */
  SHORT qq = FDKaacEnc_quantTableQ[(-gain) & 3];
  INT64 accu64 = ((INT64)((INT)((int)qq << 16))) * ((INT64)spec);
  INT accu = (INT)(accu64 >> 32);
  if (accu == 0) return 0;
  if (accu < 0) accu = -accu;
  int shift = CntLeadingZeros(accu) - 1;
  int t = qs - shift + 1;
  return (16 - 4) - (3 * (t >> 2));
}

static void require_quant_ok(int gain, const FIXP_DBL *spec, int n,
                             const char *what) {
  for (int i = 0; i < n; i++) {
    if (probe_total_shift(gain, spec[i]) < 0) {
      std::fprintf(stderr, "ENVELOPE VIOLATION (totalShift<0) in %s line %d\n",
                   what, i);
      std::exit(1);
    }
  }
}

/* Inverse-path gate: lines with |q| > MAX_QUANT take the reference's
 * early-out (quantize.cpp:327,373) and never reach invQuantizeLines — the
 * only value that slips the guard is SHORT_MIN (fixabs_S wraps to
 * itself), which would then read out of bounds (specExp = 16 >= 14).
 * The probe refuses exactly that value; everything else is safe. */
static void require_inv_ok(const SHORT *q, int n, const char *what) {
  for (int i = 0; i < n; i++) {
    if ((int)q[i] == -32768) {
      std::fprintf(stderr, "ENVELOPE VIOLATION (q==SHORT_MIN) in %s line %d\n",
                   what, i);
      std::exit(1);
    }
  }
}

/* Single-band QuantizeSpectrum probe: QSS = gain - scf. */
static void probe_ql(int gain, int dz, const FIXP_DBL *spec, int n) {
  SHORT q[2048];
  INT off[2] = {0, n};
  INT scf[1] = {0};
  require_quant_ok(gain, spec, n, "QL");
  FDKaacEnc_QuantizeSpectrum(1, 1, 1, off, spec, gain, scf, q, dz);
  require_inv_ok(q, n, "QL-out");
  std::fprintf(stdout, "QL g=%d dz=%d n=%d spec", gain, dz, n);
  for (int i = 0; i < n; i++) std::fprintf(stdout, " %d", (int)spec[i]);
  std::fprintf(stdout, " q");
  for (int i = 0; i < n; i++) std::fprintf(stdout, " %d", (int)q[i]);
  std::fprintf(stdout, "\n");
}

/* calcSfbDist probe (re-quantizes internally, then inverse + Ld). */
static void probe_qd(int gain, int dz, const FIXP_DBL *spec, int n) {
  SHORT q[2048];
  require_quant_ok(gain, spec, n, "QD");
  FIXP_DBL d = FDKaacEnc_calcSfbDist(spec, q, n, gain, dz);
  std::fprintf(stdout, "QD g=%d dz=%d n=%d spec", gain, dz, n);
  for (int i = 0; i < n; i++) std::fprintf(stdout, " %d", (int)spec[i]);
  std::fprintf(stdout, " q");
  for (int i = 0; i < n; i++) std::fprintf(stdout, " %d", (int)q[i]);
  std::fprintf(stdout, " dist %d\n", (int)d);
}

/* calcSfbQuantEnergyAndDist probe (quant input fully caller-chosen). */
static void probe_qi(int gain, const FIXP_DBL *spec, const SHORT *q, int n) {
  require_inv_ok(q, n, "QI-in");
  SHORT qq[2048];
  for (int i = 0; i < n; i++) qq[i] = q[i];
  FIXP_DBL en = 0, dist = 0;
  FDKaacEnc_calcSfbQuantEnergyAndDist((FIXP_DBL *)spec, qq, n, gain, &en,
                                      &dist);
  std::fprintf(stdout, "QI g=%d n=%d spec", gain, n);
  for (int i = 0; i < n; i++) std::fprintf(stdout, " %d", (int)spec[i]);
  std::fprintf(stdout, " q");
  for (int i = 0; i < n; i++) std::fprintf(stdout, " %d", (int)q[i]);
  std::fprintf(stdout, " en %d dist %d\n", (int)en, (int)dist);
}

int main() {
  /* ---- exact build integers ---- */
  std::fprintf(stdout, "QPROBE tables\n");
  std::fprintf(stdout, "QTARCH qtd_bytes=%d dbl_bytes=%d mant_digits=%d\n",
               (int)sizeof(FIXP_QTD), (int)sizeof(FIXP_DBL), MANT_DIGITS);
  std::fprintf(stdout, "QTABQ");
  for (int i = 0; i < 4; i++)
    std::fprintf(stdout, " %d", (int)FDKaacEnc_quantTableQ[i]);
  std::fprintf(stdout, "\nQTABE");
  for (int i = 0; i < 4; i++)
    std::fprintf(stdout, " %d", (int)FDKaacEnc_quantTableE[i]);
  std::fprintf(stdout, "\nMTAB34");
  for (int i = 0; i < MANT_SIZE; i++)
    std::fprintf(stdout, " %d", (int)FDKaacEnc_mTab_3_4[i]);
  std::fprintf(stdout, "\nMTAB43");
  for (int i = 0; i < 512; i++)
    std::fprintf(stdout, " %d", (int)FDKaacEnc_mTab_4_3Elc[i]);
  std::fprintf(stdout, "\nEXPM");
  for (int m = 0; m < 4; m++)
    for (int e = 0; e < 14; e++)
      std::fprintf(stdout, " %d",
                   (int)FDKaacEnc_specExpMantTableCombElc[m][e]);
  std::fprintf(stdout, "\nEXPS");
  for (int m = 0; m < 4; m++)
    for (int e = 0; e < 14; e++)
      std::fprintf(stdout, " %d",
                   (int)FDKaacEnc_specExpTableComb[m][e]);
  std::fprintf(stdout, "\nKZ %d KO %d\n",
               (int)(FL2FXCONST_DBL(-0.0946f + 0.5f) >> 16),
               (int)(FL2FXCONST_DBL(0.23f) >> 16));

  /* ---- single-line magnitude sweep (sign pairs + edges) ---- */
  std::fprintf(stdout, "QPROBE lines\n");
  static const INT mags[] = {
      0,         1,         2,         3,         100,       1000,
      32767,     32768,     1000000,   0x10000000, 0x20000000, 0x3fffffff,
      0x40000000, 0x7fffffff};
  static const int gains[] = {-64, -48, -32, -16, 0, 32, 64, 128, 192};
  for (unsigned gi = 0; gi < sizeof(gains) / sizeof(gains[0]); gi++) {
    for (unsigned mi = 0; mi < sizeof(mags) / sizeof(mags[0]); mi++) {
      FIXP_DBL pos[1] = {(FIXP_DBL)mags[mi]};
      FIXP_DBL neg[1] = {(FIXP_DBL)(mags[mi] == 0 ? 0 : -mags[mi])};
      probe_ql(gains[gi], 0, pos, 1);
      if (mags[mi] != 0) probe_ql(gains[gi], 0, neg, 1);
    }
  }
  /* MINVAL (only representable negative extreme) at moderate gains. */
  {
    FIXP_DBL mn[1] = {(FIXP_DBL)0x80000000};
    probe_ql(-16, 0, mn, 1);
    probe_ql(0, 0, mn, 1);
    probe_ql(64, 0, mn, 1);
  }
  /* Dead-zone path (dZoneQuantEnable=1, ELD-only in this build). */
  {
    FIXP_DBL dzspec[9] = {0, 1, 100, 1000000, 0x10000000, 0x40000000,
                          0x7fffffff, -1000, -0x40000000};
    for (int g = -32; g <= 64; g += 32) probe_ql(g, 1, dzspec, 9);
  }
  /* Multi-band geometry: 2 groups x 2 bands with an scf ramp, so each
   * band sees a different QSS = globalGain - scf. */
  {
    FIXP_DBL spec[16] = {0,
                         1,
                         -1,
                         0x10000000,
                         -0x10000000,
                         0x40000000,
                         1000,
                         -1000,
                         5000000,
                         -5000000,
                         0x20000000,
                         7,
                         -7,
                         0x08000000,
                         -0x08000000,
                         0x7fffffff};
    INT off[5] = {0, 3, 7, 11, 16};
    INT scf[4] = {5, 15, 25, 35};
    SHORT q[16];
    require_quant_ok(100, spec, 16, "QBANDS");
    FDKaacEnc_QuantizeSpectrum(4, 2, 2, off, spec, 100, scf, q, 0);
    require_inv_ok(q, 16, "QBANDS-out");
    std::fprintf(stdout, "QBANDS gg=100 dz=0 spec");
    for (int i = 0; i < 16; i++) std::fprintf(stdout, " %d", (int)spec[i]);
    std::fprintf(stdout, " q");
    for (int i = 0; i < 16; i++) std::fprintf(stdout, " %d", (int)q[i]);
    std::fprintf(stdout, "\n");
  }
  /* ---- distortion probes ---- */
  {
    FIXP_DBL z[8] = {0, 0, 0, 0, 0, 0, 0, 0};
    FIXP_DBL small[8] = {1, -1, 100, -100, 1000, -1000, 5000, -5000};
    FIXP_DBL loud[8] = {0x10000000, -0x10000000, 0x20000000, -0x20000000,
                        0x40000000, -0x40000000, 0x08000000, -0x08000000};
    FIXP_DBL mixed[8] = {0, 0x40000000, -7, 1000000,
                         -0x20000000, 3, -0x08000000, 12345};
    int dg[] = {-48, 0, 64, 160};
    for (unsigned gi = 0; gi < 4; gi++) {
      probe_qd(dg[gi], 0, z, 8);
      probe_qd(dg[gi], 0, small, 8);
      probe_qd(dg[gi], 0, loud, 8);
      probe_qd(dg[gi], 0, mixed, 8);
    }
    probe_qd(0, 1, mixed, 8);
    /* Clipping sweep at fixed gain: dist must snap to the 0.0
     * early-out once any |q| exceeds MAX_QUANT. */
    for (int k = 0; k < 8; k++) {
      FIXP_DBL s[4] = {0x10000000 + k * 0x02000000, 0, 0, 0};
      probe_qd(-48, 0, s, 4);
    }
  }
  /* ---- inverse-path probes (caller-chosen q) ---- */
  {
    FIXP_DBL spec[8] = {0x10000000, -0x10000000, 0x20000000, -0x20000000,
                        0x40000000, -0x40000000, 0x08000000, -0x08000000};
    SHORT qz[8] = {0, 0, 0, 0, 0, 0, 0, 0};
    SHORT q1[8] = {1, -1, 2, -2, 3, -3, 0, 1};
    SHORT qbig[8] = {8191, -8191, 8190, -8190, 4096, -4096, 100, -100};
    SHORT qclip[8] = {8192, -8192, 9000, -9000, 20000, -20000, 8191, 0};
    SHORT qmid[8] = {100, -100, 50, -50, 8192, 60, -70, 80};
    probe_qi(0, spec, qz, 8);
    probe_qi(0, spec, q1, 8);
    probe_qi(64, spec, q1, 8);
    probe_qi(0, spec, qbig, 8);
    probe_qi(64, spec, qbig, 8);
    probe_qi(-32, spec, qbig, 8);
    probe_qi(0, spec, qclip, 8);
    probe_qi(64, spec, qclip, 8);
    probe_qi(0, spec, qmid, 8);
  }
  std::fprintf(stdout, "QPROBE end\n");
  return 0;
}
