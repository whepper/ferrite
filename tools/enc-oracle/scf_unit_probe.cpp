/* Test-only S4 unit probe: calls static sf_estim.cpp functions directly
 * by including the implementation (production source untouched) and prints
 * deterministic transcripts for Rust comparison.
 * Build: c++ -std=c++11 -O2 -I cpp/libAACenc/src -I cpp/libFDK/include -I cpp/libSYS/include -I cpp/libMpegTPEnc/include -I . tools/enc-oracle/scf_unit_probe.cpp -L "$BLD" -lfdk-aac -Wl,-rpath,"$BLD" -o "$BLD/scf_unit_probe"
 * (links for tables; static functions come from the include).
 */
#include <cstdio>
#include <cstring>
#include "qc_data.h"
#include "interface.h"
// Pull static functions into this TU (test-only; production file untouched).
#include "sf_estim.cpp"

int main() {
  // countSingleScfBits over a grid (dense small deltas for corpus).
  std::printf("SCFUNIT bits\n");
  for (int scf = -70; scf <= 70; scf += 1) {
    for (int d = -12; d <= 12; d += 1) {
      FIXP_DBL v = FDKaacEnc_countSingleScfBits(scf, scf + d, scf - d);
      std::printf("CB scf=%d d=%d v=%d\n", scf, d, (int)v);
    }
  }
  // calcSingleSpecPe over dense corpus-like grid.
  std::printf("SCFUNIT specpe\n");
  FIXP_DBL cps[] = {(FIXP_DBL)-500000000, (FIXP_DBL)-232370909, (FIXP_DBL)-197662475,
                    (FIXP_DBL)-197000000, (FIXP_DBL)0, (FIXP_DBL)500000000};
  FIXP_DBL nls[] = {(FIXP_DBL)0, (FIXP_DBL)43874821, (FIXP_DBL)51160268,
                    (FIXP_DBL)111416461, (FIXP_DBL)1073741824};
  for (int scf = -70; scf <= 20; scf += 1) {
    for (unsigned i = 0; i < 5; i++)
      for (unsigned j = 0; j < 5; j++) {
        FIXP_DBL v = FDKaacEnc_calcSingleSpecPe(scf, cps[i], nls[j]);
        std::printf("SP scf=%d cp=%d nl=%d v=%d\n", scf, (int)cps[i],
                    (int)nls[j], (int)v);
      }
  }
  // calcSfbRelevantLines on synthetic vectors.
  std::printf("SCFUNIT relevant\n");
  {
    const int cnt = 4, per = 4, mx = 4;
    INT off[5] = {0, 4, 8, 12, 16};
    FIXP_DBL ff[4] = {(FIXP_DBL)-337460364, (FIXP_DBL)-318525798,
                      (FIXP_DBL)0, (FIXP_DBL)100000000};
    FIXP_DBL en[4] = {(FIXP_DBL)-693314277, (FIXP_DBL)-734730193,
                      (FIXP_DBL)0, (FIXP_DBL)-100000000};
    FIXP_DBL th[4] = {(FIXP_DBL)-704116391, (FIXP_DBL)-700000000,
                      (FIXP_DBL)0, (FIXP_DBL)-200000000};
    FIXP_DBL out[4] = {0, 0, 0, 0};
    FDKaacEnc_calcSfbRelevantLines(ff, en, th, off, cnt, per, mx, out);
    for (int i = 0; i < 4; i++)
      std::printf("RL i=%d v=%d\n", i, (int)out[i]);
  }
  // Full-channel probe with a dead band (tests MIN handling).
  std::printf("SCFUNIT channel\n");
  {
    const int cnt = 4, per = 4, mx = 4;
    INT off[5] = {0, 4, 8, 12, 16};
    // Band 1 dead (energy <= thresh).
    FIXP_DBL spec[1024] = {0};
    spec[0] = 1000000;
    spec[1] = -2000000;
    spec[2] = 3000000;
    spec[3] = -1000000;
    // band1 zeros (dead).
    spec[8] = 8000000;
    spec[9] = -8000000;
    spec[10] = 4000000;
    spec[11] = -4000000;
    spec[12] = 100000;
    spec[13] = -100000;
    spec[14] = 200000;
    spec[15] = -200000;
    FIXP_DBL en[4] = {(FIXP_DBL)-400000000, (FIXP_DBL)-600000000,
                      (FIXP_DBL)-400000000, (FIXP_DBL)-500000000};
    FIXP_DBL th[4] = {(FIXP_DBL)-500000000, (FIXP_DBL)-500000000,
                      (FIXP_DBL)-500000000, (FIXP_DBL)-600000000};
    FIXP_DBL ff[4] = {(FIXP_DBL)-300000000, (FIXP_DBL)-300000000,
                      (FIXP_DBL)-300000000, (FIXP_DBL)-300000000};
    for (int inv = 0; inv <= 2; inv++) {
      PSY_OUT_CHANNEL poc;
      QC_OUT_CHANNEL qoc;
      FIXP_DBL ws[1024];
      std::memset(&poc, 0, sizeof(poc));
      std::memset(&qoc, 0, sizeof(qoc));
      std::memcpy(ws, spec, sizeof(ws));
      poc.sfbCnt = cnt;
      poc.sfbPerGroup = per;
      poc.maxSfbPerGroup = mx;
      std::memcpy(poc.sfbOffsets, off, sizeof(off));
      poc.mdctSpectrum = ws;
      poc.sfbEnergyLdData = en;
      poc.sfbThresholdLdData = th;
      std::memcpy(qoc.mdctSpectrum, ws, sizeof(ws));
      for (int i = 0; i < 4; i++) qoc.sfbEnergyLdData[i] = en[i];
      for (int i = 0; i < 4; i++) qoc.sfbThresholdLdData[i] = th[i];
      for (int i = 0; i < 4; i++) qoc.sfbFormFactorLdData[i] = ff[i];
      PSY_OUT_CHANNEL *pp[2] = {&poc, nullptr};
      QC_OUT_CHANNEL *qp[2] = {&qoc, nullptr};
      FDKaacEnc_EstimateScaleFactors(pp, qp, inv, 0, 1);
      std::printf("CH inv=%d gain=%d scf=%d,%d,%d,%d\n", inv,
                  (int)qoc.globalGain, (int)qoc.scf[0], (int)qoc.scf[1],
                  (int)qoc.scf[2], (int)qoc.scf[3]);
      std::printf("CHQ inv=%d", inv);
      for (int i = 0; i < 16; i++) std::printf(" %d", (int)qoc.quantSpec[i]);
      std::printf("\n");
    }
  }
  std::printf("SCFUNIT improve\n");
  {
    FIXP_DBL spec[4] = {(FIXP_DBL)0, (FIXP_DBL)0, (FIXP_DBL)0, (FIXP_DBL)0};
    // Real band46 magnitudes unknown here; use the synthetic band-2 line
    // instead (same shape as the single probe) at two starting gains.
    FIXP_DBL bs[4] = {(FIXP_DBL)8000000, (FIXP_DBL)-8000000,
                      (FIXP_DBL)4000000, (FIXP_DBL)-4000000};
    for (int scf0 = -30; scf0 >= -38; scf0 -= 4) {
      SHORT q[4] = {0, 0, 0, 0};
      SHORT t[4] = {0, 0, 0, 0};
      FIXP_DBL d = 0;
      INT m = 0;
      INT best = FDKaacEnc_improveScf(bs, q, t, 4, (FIXP_DBL)-500000000,
                                      scf0, -111, &d, &m, 0);
      std::printf("IM scf0=%d best=%d dist=%d mincalc=%d q=%d,%d,%d,%d\n",
                  scf0, (int)best, (int)d, (int)m, (int)q[0], (int)q[1],
                  (int)q[2], (int)q[3]);
    }
  }
  std::printf("SCFUNIT single\n");
  {
    const int cnt = 4, per = 4, mx = 4;
    INT off[5] = {0, 4, 8, 12, 16};
    FIXP_DBL spec[16] = {
        (FIXP_DBL)1000000, (FIXP_DBL)-2000000, (FIXP_DBL)3000000, (FIXP_DBL)-1000000,
        (FIXP_DBL)5000000, (FIXP_DBL)-5000000, (FIXP_DBL)1000000, (FIXP_DBL)2000000,
        (FIXP_DBL)8000000, (FIXP_DBL)-8000000, (FIXP_DBL)4000000, (FIXP_DBL)-4000000,
        (FIXP_DBL)100000, (FIXP_DBL)-100000, (FIXP_DBL)200000, (FIXP_DBL)-200000};
    FIXP_DBL en[4] = {(FIXP_DBL)-400000000, (FIXP_DBL)-400000000,
                      (FIXP_DBL)-400000000, (FIXP_DBL)-500000000};
    FIXP_DBL th[4] = {(FIXP_DBL)-500000000, (FIXP_DBL)-500000000,
                      (FIXP_DBL)-500000000, (FIXP_DBL)-600000000};
    FIXP_DBL ff[4] = {(FIXP_DBL)-300000000, (FIXP_DBL)-300000000,
                      (FIXP_DBL)-300000000, (FIXP_DBL)-300000000};
    INT scf[4] = {-40, -36, -30, -20};
    INT minScf[4] = {-111, -111, -111, -111};
    FIXP_DBL dist[4] = {(FIXP_DBL)-400000000, (FIXP_DBL)-410000000,
                        (FIXP_DBL)-420000000, (FIXP_DBL)-430000000};
    FIXP_DBL cpe[4] = {(FIXP_DBL)(int)0x80000000, (FIXP_DBL)(int)0x80000000,
                       (FIXP_DBL)(int)0x80000000, (FIXP_DBL)(int)0x80000000};
    FIXP_DBL nrel[4];
    INT minCalc[4] = {-40, -36, -30, -20};
    SHORT qs[1024] = {0};
    SHORT qt[1024] = {0};
    FIXP_DBL bigSpec[1024] = {0};
    for (int i = 0; i < 16; i++) bigSpec[i] = spec[i];
    {
      FIXP_DBL out[4] = {0, 0, 0, 0};
      FDKaacEnc_calcSfbRelevantLines(ff, en, th, off, cnt, per, mx, out);
      for (int i = 0; i < 4; i++) nrel[i] = out[i];
      for (int i = 0; i < 4; i++)
        std::printf("SN nrel i=%d v=%d\n", i, (int)nrel[i]);
    }
    PSY_OUT_CHANNEL poc;
    QC_OUT_CHANNEL qoc;
    std::memset(&poc, 0, sizeof(poc));
    std::memset(&qoc, 0, sizeof(qoc));
    poc.sfbCnt = cnt;
    poc.sfbPerGroup = per;
    poc.maxSfbPerGroup = mx;
    std::memcpy(poc.sfbOffsets, off, sizeof(off));
    std::memcpy(qoc.mdctSpectrum, bigSpec, sizeof(bigSpec));
    for (int i = 0; i < 4; i++) qoc.sfbEnergyLdData[i] = en[i];
    FDKaacEnc_assimilateSingleScf(&poc, &qoc, qs, qt, 0, scf, minScf, dist,
                                  cpe, ff, nrel, minCalc, 1);
    for (int i = 0; i < 4; i++)
      std::printf("SN scf i=%d v=%d dist=%d\n", i, (int)scf[i], (int)dist[i]);
    std::printf("SN qs");
    for (int i = 0; i < 16; i++) std::printf(" %d", (int)qs[i]);
    std::printf("\n");
  }
  // Corpus 6-band single probe (sine f1 bands 45-50, isolated).
  std::printf("SCFUNIT sixband\n");
  {
    const int cnt = 6, per = 6, mx = 6;
    INT off[7] = {0, 4, 8, 16, 24, 32, 44};
    FIXP_DBL spec[44] = {
        (FIXP_DBL)3351240, (FIXP_DBL)33792, (FIXP_DBL)-2021824, (FIXP_DBL)49280,
        (FIXP_DBL)1356672, (FIXP_DBL)-97576, (FIXP_DBL)-958056, (FIXP_DBL)107568,
        (FIXP_DBL)714648, (FIXP_DBL)-106536, (FIXP_DBL)-552480, (FIXP_DBL)102280,
        (FIXP_DBL)439616, (FIXP_DBL)-96600, (FIXP_DBL)-366704, (FIXP_DBL)91328,
        (FIXP_DBL)295128, (FIXP_DBL)-82232, (FIXP_DBL)81992, (FIXP_DBL)-17668,
        (FIXP_DBL)59740, (FIXP_DBL)-20884, (FIXP_DBL)-53580, (FIXP_DBL)16892,
        (FIXP_DBL)42612, (FIXP_DBL)-26300, (FIXP_DBL)-42776, (FIXP_DBL)5804,
        (FIXP_DBL)36716, (FIXP_DBL)-19064, (FIXP_DBL)-30040, (FIXP_DBL)12388,
        (FIXP_DBL)27936, (FIXP_DBL)-12860, (FIXP_DBL)-26204, (FIXP_DBL)21248,
        (FIXP_DBL)21404, (FIXP_DBL)324, (FIXP_DBL)-22944, (FIXP_DBL)12320,
        (FIXP_DBL)20308, (FIXP_DBL)-11132, (FIXP_DBL)-14792, (FIXP_DBL)8424};
    // Band49/50 specs unknown (need dump); zeros placeholder (will fix).
    FIXP_DBL en[6] = {(FIXP_DBL)-610674038, (FIXP_DBL)-693314277,
                      (FIXP_DBL)-734730193, (FIXP_DBL)-850811009,
                      (FIXP_DBL)-987004883, (FIXP_DBL)-1040183479};
    FIXP_DBL th[6] = {(FIXP_DBL)-633321256, (FIXP_DBL)-704116391,
                      (FIXP_DBL)-752244433, (FIXP_DBL)-861613123,
                      (FIXP_DBL)-997806997, (FIXP_DBL)-1050985593};
    FIXP_DBL ff[6] = {(FIXP_DBL)-324236992, (FIXP_DBL)-337460364,
                      (FIXP_DBL)-318525798, (FIXP_DBL)-353478107,
                      (FIXP_DBL)-375609672, (FIXP_DBL)-368161603};
    INT scf[6] = {-34, -38, -44, -50, -58, -62};
    INT minScf[6] = {-106, -111, -115, -120, -131, -134};
    FIXP_DBL dist[6] = {(FIXP_DBL)-627841291, (FIXP_DBL)-693351700,
                        (FIXP_DBL)-759167659, (FIXP_DBL)-928721872,
                        (FIXP_DBL)(int)0x80000000, (FIXP_DBL)(int)0x80000000};
    FIXP_DBL cpe[6] = {(FIXP_DBL)(int)0x80000000, (FIXP_DBL)(int)0x80000000,
                       (FIXP_DBL)(int)0x80000000, (FIXP_DBL)(int)0x80000000,
                       (FIXP_DBL)(int)0x80000000, (FIXP_DBL)(int)0x80000000};
    FIXP_DBL nrel[6];
    INT minCalc[6] = {-34, -38, -44, -50, -58, -62};
    SHORT qs[1024] = {0};
    SHORT qt[1024] = {0};
    FIXP_DBL bigSpec[1024] = {0};
    for (int i = 0; i < 44; i++) bigSpec[i] = spec[i];
    FDKaacEnc_calcSfbRelevantLines(ff, en, th, off, cnt, per, mx, nrel);
    for (int i = 0; i < 6; i++)
      std::printf("SX nrel i=%d v=%d\n", i, (int)nrel[i]);
    // Requantize for initial lines.
    for (int b = 0; b < 6; b++) {
      int w = off[b + 1] - off[b];
      SHORT t[64] = {0};
      FDKaacEnc_calcSfbDist(bigSpec + off[b], t, w, scf[b], 0);
      for (int k = 0; k < w; k++) qs[off[b] + k] = t[k];
    }
    PSY_OUT_CHANNEL poc;
    QC_OUT_CHANNEL qoc;
    std::memset(&poc, 0, sizeof(poc));
    std::memset(&qoc, 0, sizeof(qoc));
    poc.sfbCnt = cnt;
    poc.sfbPerGroup = per;
    poc.maxSfbPerGroup = mx;
    std::memcpy(poc.sfbOffsets, off, sizeof(off));
    std::memcpy(qoc.mdctSpectrum, bigSpec, sizeof(bigSpec));
    for (int i = 0; i < 6; i++) qoc.sfbEnergyLdData[i] = en[i];
    FDKaacEnc_assimilateSingleScf(&poc, &qoc, qs, qt, 0, scf, minScf, dist,
                                  cpe, ff, nrel, minCalc, 1);
    for (int i = 0; i < 6; i++)
      std::printf("SX scf i=%d v=%d dist=%d\n", i, (int)scf[i], (int)dist[i]);
  }
  {
    const int cnt = 1, per = 1, mx = 1;
    INT off[2] = {912, 916};
    FIXP_DBL ff[1] = {(FIXP_DBL)-337460364};
    FIXP_DBL en[1] = {(FIXP_DBL)-693314277};
    FIXP_DBL th[1] = {(FIXP_DBL)-704116391};
    FIXP_DBL out[1] = {0};
    FDKaacEnc_calcSfbRelevantLines(ff, en, th, off, cnt, per, mx, out);
    std::printf("N46 v=%d\n", (int)out[0]);
  }
  // Corpus band45/46 quantize cross-check (mod-2 QSS).
  {
    FIXP_DBL b45[4] = {(FIXP_DBL)3351240, (FIXP_DBL)33792,
                       (FIXP_DBL)-2021824, (FIXP_DBL)49280};
    FIXP_DBL b46[4] = {(FIXP_DBL)1356672, (FIXP_DBL)-97576,
                       (FIXP_DBL)-958056, (FIXP_DBL)107568};
    SHORT q[4] = {0, 0, 0, 0};
    FIXP_DBL d45 =
        FDKaacEnc_calcSfbDist(b45, q, 4, -34, 0);
    std::printf("QB45 dist=%d q=%d,%d,%d,%d\n", (int)d45, (int)q[0],
                (int)q[1], (int)q[2], (int)q[3]);
    SHORT q2[4] = {0, 0, 0, 0};
    FIXP_DBL d46 =
        FDKaacEnc_calcSfbDist(b46, q2, 4, -38, 0);
    std::printf("QB46 dist=%d q=%d,%d,%d,%d\n", (int)d46, (int)q2[0],
                (int)q2[1], (int)q2[2], (int)q2[3]);
  }
  std::printf("SCFUNIT end\n");
  return 0;
}
