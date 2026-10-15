/* M2/M3 oracle: drive the FDK AAC-LC frontend and dump intermediates.
 *
 * Test-only tooling for whepper/fdk-aac. It contains no codec code of its
 * own: it calls the unmodified reference implementation (cpp/libAACenc,
 * cpp/libFDK) and prints per-frame decisions, spectra, energies,
 * thresholds, and TNS data. Init uses the real InitPsyConfiguration /
 * InitTnsConfiguration / InitPreEchoControl; only the per-frame psy_main
 * orchestration is mirrored statement-for-statement (integer ops are exact).
 * Build and usage: see README.md.
 */

#include <cstdio>
#include <cstring>
#include <vector>

#include "FDK_tools_rom.h"
#include "band_nrg.h"
#include "block_switch.h"
#include "fixpoint_math.h"
#include "grp_data.h"
#include "mdct.h"
#include "pre_echo_control.h"
#include "psy_configuration.h"
#include "psy_const.h"
#include "psy_data.h"
#include "spreading.h"
#include "tns_func.h"
#include "transform.h"
/* S1 stage (test-only QCMainPrepare mirror): real C calls, no production
 * changes. */
#include "FDK_audio.h"
#include "aacenc_pns.h"
#include "adj_thr.h"
#include "adj_thr_data.h"
#include "bitenc.h"
#include "intensity.h"
#include "interface.h"
#include "ms_stereo.h"
#include "pns_func.h"
#include "aacEnc_rom.h"
#include "qc_data.h"
#include "qc_main.h"
#include "aacenc.h"
#include "quantize.h"
#include "sf_estim.h"
#include "dyn_bits.h"
#include "bit_cnt.h"

extern "C" {
#include "wavreader.h"
}

#define FRAME_LEN 1024
#define RING_LEN (2 * FRAME_LEN)
/* LC block-switching look-ahead, cf. psy_main.cpp. */
#define BS_OFFSET (FRAME_LEN + (9 * FRAME_LEN / (2 * TRANS_FAC)))
/* Extra all-zero frames appended after the input (flush tail). */
#define ZERO_FRAMES 3
/* VBR-5 effective bandwidth, cf. bandwidth.cpp. */
#define VBR5_BANDWIDTH 19293

static void print_ints(FILE *out, const char *tag, const int *v, int n) {
  std::fprintf(out, "%s", tag);
  for (int i = 0; i < n; i++) std::fprintf(out, " %d", v[i]);
  std::fprintf(out, "\n");
}

static void print_q31(FILE *out, const char *tag, const FIXP_DBL *v, int n) {
  std::fprintf(out, "%s", tag);
  for (int i = 0; i < n; i++) std::fprintf(out, " %d", (int)v[i]);
  std::fprintf(out, "\n");
}

static void dump_config(FILE *out, const char *which, int sr,
                        const PSY_CONFIGURATION *c) {
  std::fprintf(out, "CFG %s sr=%d cnt=%d active=%d activeLFE=%d lowpass=%d lowpassLFE=%d clipE=%d\n",
               which, sr, c->sfbCnt, c->sfbActive, c->sfbActiveLFE,
               c->lowpassLine, c->lowpassLineLFE, (int)c->clipEnergy);
  print_ints(out, "OFFSETS ", c->sfbOffset, c->sfbCnt + 1);
  print_q31(out, "PCMQ ", c->sfbPcmQuantThreshold, c->sfbCnt);
  print_q31(out, "MASKHI ", c->sfbMaskHighFactor, c->sfbCnt);
  print_q31(out, "MASKLO ", c->sfbMaskLowFactor, c->sfbCnt);
  print_q31(out, "MASKHIS ", c->sfbMaskHighFactorSprEn, c->sfbCnt);
  print_q31(out, "MASKLOS ", c->sfbMaskLowFactorSprEn, c->sfbCnt);
  print_q31(out, "MINSNR ", c->sfbMinSnrLdData, c->sfbCnt);
}

static void dump_tns_config(FILE *out, const char *which,
                            const TNS_CONFIG *t) {
  std::fprintf(
      out,
      "TNSCFG %s active=%d maxOrder=%d coefRes=%d startBand=%d,%d startLine=%d,%d stopBand=%d stopLine=%d threshOn=%d,%d limitOrder=%d,%d\n",
      which, (int)t->tnsActive, t->maxOrder, t->coefRes, t->lpcStartBand[0],
      t->lpcStartBand[1], t->lpcStartLine[0], t->lpcStartLine[1],
      t->lpcStopBand, t->lpcStopLine, (int)t->confTab.threshOn[0],
      (int)t->confTab.threshOn[1], t->confTab.tnsLimitOrder[0],
      t->confTab.tnsLimitOrder[1]);
}

/* Test-only replica of FDKaacEnc_MergedAutoCorrelation
 * (cpp/libAACenc/src/aacenc_tns.cpp:619-752) for dumping rxx1/rxx2.
 * Identical integer ops; production source untouched. Only the equal-
 * quarters path and the norm-factor helpers are replicated since the
 * VBR-5 LC target always configures acfSplit=-1 (verified in TNSCFG).
 * Dumps raw Q31 ints; divide by 2^31 for fractions. */
static inline INT OracleScaleUpSpectrum(FIXP_DBL *dest, const FIXP_DBL *src,
                                        const INT startLine,
                                        const INT stopLine) {
  INT i, scale;
  FIXP_DBL maxVal = FL2FXCONST_DBL(0.f);
  for (i = startLine; i < stopLine; i++) {
    maxVal = fixMax(maxVal, fixp_abs(src[i]));
  }
  scale = CountLeadingBits(maxVal);
  for (i = startLine; i < stopLine; i++) {
    dest[i] = src[i] << scale;
  }
  return scale;
}

static inline FIXP_DBL OracleCalcAutoCorrValue(const FIXP_DBL *spectrum,
                                               const INT startLine,
                                               const INT stopLine,
                                               const INT lag, const INT scale) {
  int i;
  FIXP_DBL result = FL2FXCONST_DBL(0.f);
  if (lag == 0) {
    for (i = startLine; i < stopLine; i++) {
      result += (fPow2(spectrum[i]) >> scale);
    }
  } else {
    for (i = startLine; i < (stopLine - lag); i++) {
      result += (fMult(spectrum[i], spectrum[i + lag]) >> scale);
    }
  }
  return result;
}

static inline FIXP_DBL OracleAutoCorrNormFac(const FIXP_DBL value,
                                              const INT scale, INT *sc) {
#define ORACLE_HLM_MIN_NRG 0.0000000037252902984619140625f /* 2^-28 */
#define ORACLE_MAX_INV_NRGFAC (1.f / ORACLE_HLM_MIN_NRG)
  FIXP_DBL retValue;
  FIXP_DBL A, B;
  if (scale >= 0) {
    A = value;
    B = FL2FXCONST_DBL(ORACLE_HLM_MIN_NRG) >> fixMin(DFRACT_BITS - 1, scale);
  } else {
    A = value >> fixMin(DFRACT_BITS - 1, (-scale));
    B = FL2FXCONST_DBL(ORACLE_HLM_MIN_NRG);
  }
  if (A > B) {
    int shift = 0;
    FIXP_DBL tmp = invSqrtNorm2(value, &shift);
    retValue = fMult(tmp, tmp);
    *sc += (2 * shift);
  } else {
    retValue = (FIXP_DBL)MAXVAL_DBL;
    *sc += scale + 28;
  }
  return retValue;
}

static void OracleDumpRxx(FILE *out, int ch, int w, const FIXP_DBL *spectrum,
                          const TNS_CONFIG *tC) {
  FIXP_DBL pSpectrum[1024];
  FIXP_DBL rxx1[TNS_MAX_ORDER + 1], rxx2[TNS_MAX_ORDER + 1];
  for (int i = 0; i <= TNS_MAX_ORDER; i++) rxx1[i] = rxx2[i] = 0;
  int idx0, idx1, idx2, idx3, idx4, lag;
  {
    int i = tC->lpcStopLine - tC->lpcStartLine[LOFILT];
    idx0 = tC->lpcStartLine[LOFILT];
    idx1 = idx0 + i / 4;
    idx2 = idx0 + i / 2;
    idx3 = idx0 + i * 3 / 4;
    idx4 = tC->lpcStopLine;
  }
  INT sc1 = OracleScaleUpSpectrum(pSpectrum, spectrum, idx0, idx1);
  INT sc2 = OracleScaleUpSpectrum(pSpectrum, spectrum, idx1, idx2);
  INT sc3 = OracleScaleUpSpectrum(pSpectrum, spectrum, idx2, idx3);
  INT sc4 = OracleScaleUpSpectrum(pSpectrum, spectrum, idx3, idx4);
  INT nsc1, nsc2, nsc3, nsc4;
  for (nsc1 = 1; (1 << nsc1) < (idx1 - idx0); nsc1++)
    ;
  for (nsc2 = 1; (1 << nsc2) < (idx2 - idx1); nsc2++)
    ;
  for (nsc3 = 1; (1 << nsc3) < (idx3 - idx2); nsc3++)
    ;
  for (nsc4 = 1; (1 << nsc4) < (idx4 - idx3); nsc4++)
    ;
  FIXP_DBL rxx1_0 =
      OracleCalcAutoCorrValue(pSpectrum, idx0, idx1, 0, nsc1);
  FIXP_DBL rxx2_0 =
      OracleCalcAutoCorrValue(pSpectrum, idx1, idx2, 0, nsc2);
  FIXP_DBL rxx3_0 =
      OracleCalcAutoCorrValue(pSpectrum, idx2, idx3, 0, nsc3);
  FIXP_DBL rxx4_0 =
      OracleCalcAutoCorrValue(pSpectrum, idx3, idx4, 0, nsc4);
  if (rxx1_0 != FL2FXCONST_DBL(0.f)) {
    INT sc_fac1 = -1;
    FIXP_DBL fac1 =
        OracleAutoCorrNormFac(rxx1_0, ((-2 * sc1) + nsc1), &sc_fac1);
    rxx1[0] = scaleValue(fMult(rxx1_0, fac1), sc_fac1);
    if (tC->isLowDelay) {
      for (lag = 1; lag <= tC->maxOrder; lag++) {
        FIXP_DBL x1 =
            OracleCalcAutoCorrValue(pSpectrum, idx0, idx1, lag, nsc1);
        rxx1[lag] =
            fMult(scaleValue(fMult(x1, fac1), sc_fac1),
                  tC->acfWindow[LOFILT][lag]);
      }
    } else {
      for (lag = 1; lag <= tC->maxOrder; lag++) {
        if ((3 * lag) <= tC->maxOrder + 3) {
          FIXP_DBL x1 =
              OracleCalcAutoCorrValue(pSpectrum, idx0, idx1, lag, nsc1);
          rxx1[lag] = fMult(scaleValue(fMult(x1, fac1), sc_fac1),
                            tC->acfWindow[LOFILT][3 * lag]);
        }
      }
    }
  }
  if (!((rxx2_0 == FL2FXCONST_DBL(0.f)) && (rxx3_0 == FL2FXCONST_DBL(0.f)) &&
        (rxx4_0 == FL2FXCONST_DBL(0.f)))) {
    FIXP_DBL fac2, fac3, fac4;
    fac2 = fac3 = fac4 = FL2FXCONST_DBL(0.f);
    INT sc_fac2, sc_fac3, sc_fac4;
    sc_fac2 = sc_fac3 = sc_fac4 = 0;
    if (rxx2_0 != FL2FXCONST_DBL(0.f)) {
      fac2 = OracleAutoCorrNormFac(rxx2_0, ((-2 * sc2) + nsc2), &sc_fac2);
      sc_fac2 -= 2;
    }
    if (rxx3_0 != FL2FXCONST_DBL(0.f)) {
      fac3 = OracleAutoCorrNormFac(rxx3_0, ((-2 * sc3) + nsc3), &sc_fac3);
      sc_fac3 -= 2;
    }
    if (rxx4_0 != FL2FXCONST_DBL(0.f)) {
      fac4 = OracleAutoCorrNormFac(rxx4_0, ((-2 * sc4) + nsc4), &sc_fac4);
      sc_fac4 -= 2;
    }
    rxx2[0] = scaleValue(fMult(rxx2_0, fac2), sc_fac2) +
              scaleValue(fMult(rxx3_0, fac3), sc_fac3) +
              scaleValue(fMult(rxx4_0, fac4), sc_fac4);
    for (lag = 1; lag <= tC->maxOrder; lag++) {
      FIXP_DBL x2 = scaleValue(fMult(OracleCalcAutoCorrValue(
                                        pSpectrum, idx1, idx2, lag, nsc2),
                                    fac2),
                              sc_fac2) +
                   scaleValue(fMult(OracleCalcAutoCorrValue(
                                        pSpectrum, idx2, idx3, lag, nsc3),
                                    fac3),
                              sc_fac3) +
                   scaleValue(fMult(OracleCalcAutoCorrValue(
                                        pSpectrum, idx3, idx4, lag, nsc4),
                                    fac4),
                              sc_fac4);
      rxx2[lag] = fMult(x2, tC->acfWindow[HIFILT][lag]);
    }
  }
  std::fprintf(out, "RXX1 ch=%d w=%d", ch, w);
  for (int i = 0; i <= TNS_MAX_ORDER; i++)
    std::fprintf(out, " %d", (int)rxx1[i]);
  std::fprintf(out, "\nRXX2 ch=%d w=%d", ch, w);
  for (int i = 0; i <= TNS_MAX_ORDER; i++)
    std::fprintf(out, " %d", (int)rxx2[i]);
  std::fprintf(out, "\n");
}

struct ChState {
  BLOCK_SWITCHING_CONTROL bs;
  mdct_t mdct;
  INT_PCM ring[RING_LEN];
  int last_shape;
  /* M3 state */
  FIXP_DBL spec[FRAME_LEN];
  SFB_THRESHOLD thrU;
  SFB_ENERGY enU, spreadU, msU;
  FIXP_DBL enLd[TRANS_FAC][MAX_GROUPED_SFB];
  FIXP_DBL msLd[TRANS_FAC][MAX_GROUPED_SFB];
  INT maxScale[TRANS_FAC][MAX_GROUPED_SFB];
  FIXP_DBL nm1[MAX_SFB];
  INT m_nm1;
  INT calcPre;
  INT mdctScale;
  INT sfbActive;
  INT lowpassLine;
  TNS_DATA tnsData;
  TNS_INFO tnsInfo;
  INT groupedOff[MAX_GROUPED_SFB + 1];
  INT maxSfbPerGroup;
  FIXP_DBL groupedMinSnr[MAX_GROUPED_SFB];
  FIXP_DBL groupedELd[MAX_GROUPED_SFB];
  FIXP_DBL groupedTLd[MAX_GROUPED_SFB];
  FIXP_DBL thrLd[MAX_GROUPED_SFB];
  INT finalShiftDbg;
};

int main(int argc, char *argv[]) {
  if (argc == 2 && std::strcmp(argv[1], "--configs") == 0) {
    /* Init-time dump for every LC sample rate (no audio needed), at both
       VBR-5 operating points (96000 stereo / 112000 mono bit/s per
       channel; allowIS differs). Shared stdout format with the per-run
       CFG block; sections separated by "# pch=" markers. */
    const int rates[] = {8000, 11025, 12000, 16000, 22050, 24000, 32000,
                         44100, 48000, 64000, 88200, 96000};
    const int pch[] = {96000, 112000};
    for (unsigned b = 0; b < sizeof(pch) / sizeof(pch[0]); b++) {
      std::printf("# pch=%d\n", pch[b]);
      for (unsigned r = 0; r < sizeof(rates) / sizeof(rates[0]); r++) {
        int sr = rates[r];
        PSY_CONFIGURATION cL, cS;
        int bw = VBR5_BANDWIDTH < sr / 2 ? VBR5_BANDWIDTH : sr / 2;
        if (FDKaacEnc_InitPsyConfiguration(pch[b], sr, bw,
                                           LONG_WINDOW, FRAME_LEN, 1, 1, &cL,
                                           FB_LC) != AAC_ENC_OK ||
            FDKaacEnc_InitPsyConfiguration(pch[b], sr, bw,
                                           SHORT_WINDOW, FRAME_LEN, 1, 1, &cS,
                                           FB_LC) != AAC_ENC_OK) {
          std::fprintf(stderr, "config init failed for %d\n", sr);
          return 1;
        }
        TNS_CONFIG tL, tS;
        int tnsCh = (pch[b] == 112000) ? 1 : 2;
        int tnsBr = (pch[b] == 112000) ? 112000 : 192000;
        if (FDKaacEnc_InitTnsConfiguration(tnsBr, sr, tnsCh, LONG_WINDOW,
                                           FRAME_LEN, 0, 0, &tL, &cL, 2,
                                           8) != AAC_ENC_OK ||
            FDKaacEnc_InitTnsConfiguration(tnsBr, sr, tnsCh, SHORT_WINDOW,
                                           FRAME_LEN, 0, 0, &tS, &cS, 1,
                                           4) != AAC_ENC_OK) {
          std::fprintf(stderr, "tns init failed for %d\n", sr);
          return 1;
        }
        FILE *out = stdout;
        dump_config(out, "LONG", sr, &cL);
        dump_config(out, "SHORT", sr, &cS);
        dump_tns_config(out, "LONG", &tL);
        dump_tns_config(out, "SHORT", &tS);
      }
    }
    return 0;
  }
  if (argc < 3) {
    std::fprintf(stderr, "usage: %s in.wav out.txt | %s --configs\n", argv[0],
                 argv[0]);
    return 1;
  }

  void *wav = wav_read_open(argv[1]);
  if (!wav) {
    std::fprintf(stderr, "cannot open %s\n", argv[1]);
    return 1;
  }
  int format = 0, channels = 0, sample_rate = 0, bits_per_sample = 0;
  if (!wav_get_header(wav, &format, &channels, &sample_rate,
                      &bits_per_sample, NULL) ||
      format != 1 || bits_per_sample != 16 || channels < 1 ||
      channels > 2) {
    std::fprintf(stderr, "unsupported wav (need 16-bit PCM mono/stereo)\n");
    return 1;
  }

  /* Nominal VBR-5 bitrates, cf. aacenc.cpp configTabVBR. */
  int totalBr = (channels == 1) ? 112000 : 96000 * channels;
  int bandwidth = VBR5_BANDWIDTH < sample_rate / 2 ? VBR5_BANDWIDTH
                                                   : sample_rate / 2;
  int nEff = channels; /* no LFE in fixtures */
  int chBitrate = totalBr / nEff;
  int tnsCh = channels;
  int tnsBitrate = (totalBr * tnsCh) / nEff;

  /* Real init. */
  PSY_CONFIGURATION confLong, confShort;
  if (FDKaacEnc_InitPsyConfiguration(chBitrate, sample_rate, bandwidth,
                                     LONG_WINDOW, FRAME_LEN, 1, 1, &confLong,
                                     FB_LC) != AAC_ENC_OK) {
    std::fprintf(stderr, "InitPsyConfiguration(long) failed\n");
    return 1;
  }
  if (FDKaacEnc_InitPsyConfiguration(chBitrate, sample_rate, bandwidth,
                                     SHORT_WINDOW, FRAME_LEN, 1, 1, &confShort,
                                     FB_LC) != AAC_ENC_OK) {
    std::fprintf(stderr, "InitPsyConfiguration(short) failed\n");
    return 1;
  }
  TNS_CONFIG tnsConfLong, tnsConfShort;
  if (FDKaacEnc_InitTnsConfiguration(tnsBitrate, sample_rate, tnsCh,
                                     LONG_WINDOW, FRAME_LEN, 0, 0,
                                     &tnsConfLong, &confLong, 2,
                                     8) != AAC_ENC_OK) {
    std::fprintf(stderr, "InitTnsConfiguration(long) failed\n");
    return 1;
  }
  if (FDKaacEnc_InitTnsConfiguration(tnsBitrate, sample_rate, tnsCh,
                                     SHORT_WINDOW, FRAME_LEN, 0, 0,
                                     &tnsConfShort, &confShort, 1,
                                     4) != AAC_ENC_OK) {
    std::fprintf(stderr, "InitTnsConfiguration(short) failed\n");
    return 1;
  }

  /* Read all samples. */
  std::vector<short> interleaved;
  unsigned char buf[65536];
  int nread;
  while ((nread = wav_read_data(wav, buf, sizeof(buf))) > 0) {
    for (int i = 0; i + 1 < nread; i += 2)
      interleaved.push_back((short)(buf[i] | (buf[i + 1] << 8)));
  }
  wav_read_close(wav);

  long total_per_ch = (long)interleaved.size() / channels;
  long n_frames = (total_per_ch + FRAME_LEN - 1) / FRAME_LEN + ZERO_FRAMES;

  std::vector<std::vector<short> > pcm(
      channels, std::vector<short>(n_frames * FRAME_LEN, 0));
  for (long i = 0; i < total_per_ch; i++)
    for (int c = 0; c < channels; c++)
      pcm[c][i] = interleaved[(size_t)i * channels + c];

  std::vector<ChState> st(channels);
  for (int c = 0; c < channels; c++) {
    std::memset(&st[c], 0, sizeof(st[c]));
    FDKaacEnc_InitBlockSwitching(&st[c].bs, 0); /* LC */
    mdct_init(&st[c].mdct, NULL, 0);
    st[c].last_shape = 0;
    FDKaacEnc_InitPreEchoControl(st[c].nm1, &st[c].calcPre, confLong.sfbCnt,
                                 confLong.sfbPcmQuantThreshold, &st[c].m_nm1);
    st[c].mdctScale = 0;
  }

  /* S6 persistent QC state (one element; fixtures are mono/stereo).
   * Initialized through the REAL `FDKaacEnc_QCInit` (which calls the real
   * `InitElementBits` + `AdjThrInit`), with `QC_INIT` built per the
   * `aacenc.cpp:582-684` VBR path:
   * averageBits/minBits (VBR: bitRes=maxBits=6144*nChEff, minBits=0),
   * bitResMode=FULL (maxBits-averageBits >> 300*nCh), bitDistributionMode=1
   * (mono/stereo per aacenc.cpp:667-670), meanPe/maxBitFac via the real
   * `fDivNorm`+`scaleValue` (aacenc.cpp:675-684), staticBits=56 (ADTS
   * header, transport-probed constant: `adtsWrite_GetHeaderBits`=56 with
   * protection_absent=1 and PCE=-1 for SCE/CPE per
   * `getPceRepetitionRate`; auBits-independent, verified by /tmp/tp_probe
   * against the real `transportEnc_GetStaticBits`: 56/56/56).
   * peOffset=0 holds for VBR-5 nominal rates (chBitrate>=32000 skips the
   * <32000 branch); chaosMeasureEnFac/lastEnFacPatch start zeroed like
   * encoder RAM (AdjThrInit leaves them untouched); chaosMeasureOld starts
   * at 0.3 and evolves in the S2 stage. */
  ATS_ELEMENT s1ats;
  std::memset(&s1ats, 0, sizeof(s1ats));
  ADJ_THR_STATE s2hAdjThr;
  std::memset(&s2hAdjThr, 0, sizeof(s2hAdjThr));
  CHANNEL_MAPPING s2cm;
  std::memset(&s2cm, 0, sizeof(s2cm));
  s2cm.encMode = (channels == 2) ? MODE_2 : MODE_1;
  s2cm.nElements = 1;
  s2cm.nChannels = channels;
  s2cm.nChannelsEff = nEff;
  s2cm.elInfo[0].elType = (channels == 2) ? ID_CPE : ID_SCE;
  s2cm.elInfo[0].nChannelsInEl = channels;
  s2cm.elInfo[0].ChannelIndex[0] = 0;
  s2cm.elInfo[0].ChannelIndex[1] = 1;
  s2cm.elInfo[0].relativeBits = (FIXP_DBL)MAXVAL_DBL;
  s2hAdjThr.adjThrStateElem[0] = &s1ats;
  QC_STATE s6qc;
  std::memset(&s6qc, 0, sizeof(s6qc));
  ELEMENT_BITS s6elBits;
  std::memset(&s6elBits, 0, sizeof(s6elBits));
  s6qc.elementBits[0] = &s6elBits;
  s6qc.hAdjThr = &s2hAdjThr;
  INT s6avgBitsFrame =
      FDKaacEnc_CalcBitsPerFrame(totalBr, FRAME_LEN, sample_rate);
  INT s6averageBits = (s6avgBitsFrame + 7) & ~7;
  INT s6bitRes = 6144 * nEff;
  INT s6maxBits = 6144 * nEff;
  s6maxBits = fixMax(s6maxBits, (s6avgBitsFrame + 7) & ~7);
  INT s6minBits = fixMin(0, s6avgBitsFrame & ~7);
  AACENC_BITRES_MODE s6bitResMode = AACENC_BR_MODE_FULL;
  if (!(s6maxBits - s6averageBits >= 300 * channels)) {
    std::fprintf(stderr, "S6 bitResMode assumption violated\n");
    return 1;
  }
  INT s6meanPe, s6maxBitFac_m;
  {
    INT qbw = 0;
    FIXP_DBL bw_ratio =
        fDivNorm((FIXP_DBL)(10 * FRAME_LEN * bandwidth), (FIXP_DBL)sample_rate,
                 &qbw);
    s6meanPe = fixMax(
        (INT)scaleValue(bw_ratio, qbw + 1 - (DFRACT_BITS - 1)), 1);
    INT mbfac_e = 0;
    FIXP_DBL mbfac = fDivNorm(s6maxBits, s6averageBits, &mbfac_e);
    s6maxBitFac_m = scaleValue(mbfac, -(DFRACT_BITS - 1 - 24 - mbfac_e));
  }
  {
    QC_INIT s6init;
    std::memset(&s6init, 0, sizeof(s6init));
    s6init.channelMapping = &s2cm;
    s6init.sceCpe = 0;
    s6init.maxBits = s6maxBits;
    s6init.averageBits = s6averageBits;
    s6init.bitRes = s6bitRes;
    s6init.sampleRate = sample_rate;
    s6init.isLowDelay = 0;
    s6init.staticBits = 56;
    s6init.bitrateMode = QCDATA_BR_MODE_VBR_5;
    s6init.meanPe = s6meanPe;
    s6init.chBitrate = chBitrate;
    s6init.invQuant = 2;
    s6init.maxIterations = 5;
    s6init.maxBitFac = s6maxBitFac_m;
    s6init.bitrate = totalBr;
    s6init.nSubFrames = 1;
    s6init.minBits = s6minBits;
    s6init.bitResMode = s6bitResMode;
    s6init.bitDistributionMode = 1;
    s6init.padding.paddingRest = sample_rate;
    if (FDKaacEnc_QCInit(&s6qc, &s6init, 1) != AAC_ENC_OK) {
      std::fprintf(stderr, "S6 QCInit failed\n");
      return 1;
    }
  }
  if (chBitrate < 32000 || s1ats.peOffset != 0) {
    std::fprintf(stderr, "S1 ATS assumption violated: chBitrate %d peOffset %d\n",
                 chBitrate, (int)s1ats.peOffset);
    return 1;
  }

  FILE *out = std::fopen(argv[2], "w");
  if (!out) {
    std::perror(argv[2]);
    return 1;
  }
  std::fprintf(out, "# sr=%d ch=%d frames=%ld src=%s chBitrate=%d\n",
               sample_rate, channels, n_frames, argv[1], chBitrate);
  dump_config(out, "LONG", sample_rate, &confLong);
  dump_config(out, "SHORT", sample_rate, &confShort);
  dump_tns_config(out, "LONG", &tnsConfLong);
  dump_tns_config(out, "SHORT", &tnsConfShort);
  /* S6 init vectors (all from the real QCInit path above). */
  std::fprintf(out,
               "S6INIT avg=%d max=%d min=%d bitres=%d mode=%d distmode=%d "
               "meanpe=%d maxbitfac=%d chbitrateEl=%d maxbitsEl=%d "
               "relbitsEl=%d bits2pe=%d,%d pemin=%d pemax=%d\n",
               (int)s6averageBits, (int)s6maxBits, (int)s6minBits,
               (int)s6bitRes, (int)s6bitResMode, 1, (int)s6meanPe,
               (int)s6maxBitFac_m, (int)s6elBits.chBitrateEl,
               (int)s6elBits.maxBitsEl, (int)s6elBits.relativeBitsEl,
               (int)s1ats.bits2PeFactor_m, (int)s1ats.bits2PeFactor_e,
               (int)s1ats.peMin, (int)s1ats.peMax);

  INT_PCM fresh[2][FRAME_LEN];
  int isShort[2], nWin[2], wLen[2], zeroSpecCh[2];
  int tnsActive[TRANS_FAC], nrgScaling[2], tnsSpecShift;
  /* S6 per-frame saves (assigned in S1/S4/S5 stages, consumed by the S6
     stage after S5; frame-loop scope outlives the stage blocks). */
  INT s6peSaved = 0, s6staticSaved = 0;
  INT s6gain[2] = {0, 0};
  SHORT s6qspec[2][FRAME_LEN];
  INT s6scf[2][MAX_GROUPED_SFB];
  UINT s6qmax[2][MAX_GROUPED_SFB];
  FIXP_DBL s6mdct[2][FRAME_LEN];
  SECTION_DATA s6sec[2];
  INT s6dyn[2] = {0, 0};
  TOOLSINFO s6toolsSaved;
  INT s6off[2][MAX_GROUPED_SFB + 1];
  INT s6cnt[2] = {0, 0}, s6per[2] = {0, 0}, s6maxSfb0[2] = {0, 0};
  INT s6seq[2] = {0, 0};
  INT s6noise[2][MAX_GROUPED_SFB], s6isBook[2][MAX_GROUPED_SFB],
      s6isScale[2][MAX_GROUPED_SFB];
  /* S8 per-frame saves (assigned in S1/S6 stages, consumed by the S8 stage
     after S6; frame-loop scope outlives the stage blocks). Side info for
     element assembly (windowShape/groupingMask/mdctScale from s1poc) plus
     the S6 fill/finalize totals. First-pass s6* doubles as the final state
     on the corpus (all 20 frames single-pass, npass == 1). */
  INT s8wshape[2] = {0, 0}, s8gmask[2] = {0, 0}, s8mdct[2] = {0, 0};
  INT s8totfill = 0, s8align = 0, s8total = 0, s8static = 0, s8useddyn = 0;

  for (long f = 0; f < n_frames; f++) {
    for (int c = 0; c < channels; c++) {
      std::memcpy(fresh[c], &pcm[c][(size_t)f * FRAME_LEN],
                  FRAME_LEN * sizeof(INT_PCM));
      FDKaacEnc_BlockSwitching(&st[c].bs, FRAME_LEN, 0, fresh[c]);
    }
    if (channels == 2) {
      if (FDKaacEnc_SyncBlockSwitching(&st[0].bs, &st[1].bs, 2, 1) != 0) {
        std::fprintf(stderr, "sync failed at frame %ld\n", f);
        return 1;
      }
    } else {
      FDKaacEnc_SyncBlockSwitching(&st[0].bs, NULL, 1, 1);
    }

    for (int c = 0; c < channels; c++) {
      std::memcpy(st[c].ring + BS_OFFSET, fresh[c],
                  (RING_LEN - BS_OFFSET) * sizeof(INT_PCM));

      int seq = st[c].bs.lastWindowSequence;
      int shape = st[c].bs.windowShape;
      int fr = (seq == START_WINDOW || seq == SHORT_WINDOW) ? FRAME_LEN >> 3
                                                            : FRAME_LEN;
      const FIXP_WTP *slope = FDKgetWindowSlope(fr, shape);

      INT e = 0;
      if (FDKaacEnc_Transform_Real(st[c].ring, st[c].spec, seq, shape,
                                   &st[c].last_shape, &st[c].mdct, FRAME_LEN,
                                   &e, 0) != 0) {
        std::fprintf(stderr, "transform failed at frame %ld ch %d\n", f, c);
        return 1;
      }
      st[c].mdctScale = e;

      std::fprintf(out, "FRAME %ld CH %d SEQ %d SHAPE %d GROUPS %d %d %d %d %d\n",
                   f, c, seq, shape, st[c].bs.noOfGroups,
                   st[c].bs.groupLen[0], st[c].bs.groupLen[1],
                   st[c].bs.groupLen[2], st[c].bs.groupLen[3]);
      std::fprintf(out, "RING");
      for (int i = 0; i < RING_LEN; i++)
        std::fprintf(out, " %d", (int)st[c].ring[i]);
      std::fprintf(out, "\nMDCT_E %d\nMDCT", (int)e);
      for (int i = 0; i < FRAME_LEN; i++)
        std::fprintf(out, " %d", (int)st[c].spec[i]);
      std::fprintf(out, "\n");

      /* ---- psy stage mirror ---- */
      PSY_CONFIGURATION *conf;
      isShort[c] = (seq == SHORT_WINDOW);
      if (isShort[c]) {
        conf = &confShort;
        nWin[c] = TRANS_FAC;
        wLen[c] = FRAME_LEN / TRANS_FAC;
      } else {
        conf = &confLong;
        nWin[c] = 1;
        wLen[c] = FRAME_LEN;
      }
      st[c].sfbActive = conf->sfbActive;
      st[c].lowpassLine = conf->lowpassLine;

      /* lowpass zero + zeroSpec (per channel; global decided below) */
      int zeroCh = 1;
      for (int w = 0; w < nWin[c]; w++) {
        for (int i = w * wLen[c] + st[c].lowpassLine; i < (w + 1) * wLen[c]; i++)
          st[c].spec[i] = 0;
        for (int i = w * wLen[c]; i < w * wLen[c] + st[c].lowpassLine; i++) {
          if (st[c].spec[i] != 0) {
            zeroCh = 0;
            break;
          }
        }
      }
      zeroSpecCh[c] = zeroCh;
    } /* ch: transform + lowpass */

    /* ---- element-global rescale (mirrors psy_main.cpp:631-730) ----
       Real C computes one minSpecShift/maxNrg/finalShift over all channels;
       per-channel rescale with an order-dependent clamp (as here before)
       diverges for quiet stereo (ch1 read ch0's already-shifted mdctScale).
       Verified: quiet-sine f5 ch0/ch1 identical inputs gave 11/4. */
    int finalShift = 0;
    int allZero = 1;
    for (int c = 0; c < channels; c++) allZero = allZero && zeroSpecCh[c];
    if (allZero) {
      for (int c = 0; c < channels; c++) {
        st[c].mdctScale = 0;
        for (int w = 0; w < nWin[c]; w++) {
          for (int sfb = 0; sfb < st[c].sfbActive; sfb++) {
            st[c].maxScale[w][sfb] = 0;
            (isShort[c] ? st[c].enU.Short[w][sfb] : st[c].enU.Long[sfb]) = 0;
            (isShort[c] ? st[c].enLd[w][sfb] : st[c].enLd[0][sfb]) =
                FL2FXCONST_DBL(-1.0f);
            (isShort[c] ? st[c].thrU.Short[w][sfb] : st[c].thrU.Long[sfb]) = 0;
          }
        }
        st[c].finalShiftDbg = 0;
      }
    } else {
      /* joint maxScale + minSpecShift over the element */
      int minSpecShift = MAX_SHIFT_DBL;
      for (int c = 0; c < channels; c++) {
        PSY_CONFIGURATION *conf =
            isShort[c] ? &confShort : &confLong;
        for (int w = 0; w < nWin[c]; w++) {
          FDKaacEnc_CalcSfbMaxScaleSpec(
              st[c].spec + w * wLen[c], conf->sfbOffset,
              st[c].maxScale[w], st[c].sfbActive);
          for (int sfb = 0; sfb < st[c].sfbActive; sfb++)
            minSpecShift = fixMin(minSpecShift, st[c].maxScale[w][sfb]);
        }
      }
      /* joint energies + maxNrg */
      FIXP_DBL maxNrg = 0;
      for (int c = 0; c < channels; c++) {
        PSY_CONFIGURATION *conf =
            isShort[c] ? &confShort : &confLong;
        for (int w = 0; w < nWin[c]; w++) {
          FIXP_DBL *E =
              isShort[c] ? st[c].enU.Short[w] : st[c].enU.Long;
          FIXP_DBL *Ld = st[c].enLd[w];
          FIXP_DBL cur = FDKaacEnc_CheckBandEnergyOptim(
              st[c].spec + w * wLen[c], st[c].maxScale[w], conf->sfbOffset,
              st[c].sfbActive, E, Ld, minSpecShift - 4);
          maxNrg = fixMax(maxNrg, cur);
        }
      }
      int nrgShift = MAX_SHIFT_DBL;
      if (maxNrg != 0) {
        nrgShift =
            (CountLeadingBits(maxNrg) >> 1) + (minSpecShift - 4);
      }
      /* C uses isShortWindow[0] for the whole element (psy_main:676). */
      if (isShort[0]) nrgShift--;
      finalShift = fixMin(minSpecShift, nrgShift);
      /* clamp reads UNMODIFIED mdctScale (joint section, before apply) */
      if (finalShift > st[0].mdctScale + 3) finalShift = st[0].mdctScale + 3;
      /* per-channel apply */
      FIXP_DBL ldShift = finalShift * FL2FXCONST_DBL(2.0 / 64);
      for (int c = 0; c < channels; c++) {
        for (int w = 0; w < nWin[c]; w++) {
          FIXP_DBL *E = isShort[c] ? st[c].enU.Short[w] : st[c].enU.Long;
          FIXP_DBL *Ld = st[c].enLd[w];
          FIXP_DBL *T = isShort[c] ? st[c].thrU.Short[w] : st[c].thrU.Long;
          for (int sfb = 0; sfb < st[c].sfbActive; sfb++) {
            int scale = fixMax(0, st[c].maxScale[w][sfb] - 4);
            scale = fixMin((scale - finalShift) << 1, DFRACT_BITS - 1);
            if (scale >= 0)
              E[sfb] >>= scale;
            else
              E[sfb] <<= (-scale);
            T[sfb] = fMult(E[sfb], C_RATIO);
            Ld[sfb] += ldShift;
          }
        }
        if (finalShift != 0) {
          for (int w = 0; w < nWin[c]; w++) {
            for (int i = 0; i < st[c].lowpassLine; i++)
              st[c].spec[w * wLen[c] + i] <<= finalShift;
            for (int sfb = 0; sfb < st[c].sfbActive; sfb++)
              st[c].maxScale[w][sfb] -= finalShift;
          }
          st[c].mdctScale -= finalShift;
        }
        st[c].finalShiftDbg = finalShift;
      }
    }

    for (int c = 0; c < channels; c++) {
      PSY_CONFIGURATION *conf = isShort[c] ? &confShort : &confLong;
      TNS_CONFIG *tconf = isShort[c] ? &tnsConfShort : &tnsConfLong;
      int seq = st[c].bs.lastWindowSequence;

      /* TNS detect (global gate mirrors psy_main:766) */
      if (tnsConfLong.tnsActive || tnsConfShort.tnsActive) {
        for (int w = 0; w < nWin[c]; w++) {
          FDKaacEnc_TnsDetect(&st[c].tnsData, tconf, &st[c].tnsInfo,
                              conf->sfbCnt, st[c].spec + w * wLen[c], w,
                              seq);
        }
        /* Ground-truth autocorrelation dump (same input as detect). */
        for (int w = 0; w < nWin[c]; w++) {
          OracleDumpRxx(out, c, w, st[c].spec + w * wLen[c], tconf);
        }
      }
    } /* ch: TNS detect second pass */

    /* C declares these per-EncodeFrame (psy_main:767-769); they must be
       reset every frame, not just when TNS fires. */
    tnsSpecShift = 0;
    nrgScaling[0] = nrgScaling[1] = 0;
    if (channels == 2) {
      FDKaacEnc_TnsSync(&st[1].tnsData, &st[0].tnsData, &st[1].tnsInfo,
                        &st[0].tnsInfo, st[1].bs.lastWindowSequence,
                        st[0].bs.lastWindowSequence, isShort[1] ? &tnsConfShort : &tnsConfLong);
    }
    for (int w = 0; w < TRANS_FAC; w++) tnsActive[w] = 0;
    if (tnsConfLong.tnsActive || tnsConfShort.tnsActive) {
      int nw = isShort[0] ? TRANS_FAC : 1;
      for (int w = 0; w < nw; w++) {
        if (isShort[0]) {
          tnsActive[w] =
              st[0].tnsData.dataRaw.Short.subBlockInfo[w].tnsActive[HIFILT] ||
              st[0].tnsData.dataRaw.Short.subBlockInfo[w].tnsActive[LOFILT] ||
              st[channels - 1]
                  .tnsData.dataRaw.Short.subBlockInfo[w]
                  .tnsActive[HIFILT] ||
              st[channels - 1]
                  .tnsData.dataRaw.Short.subBlockInfo[w]
                  .tnsActive[LOFILT];
        } else {
          tnsActive[w] =
              st[0].tnsData.dataRaw.Long.subBlockInfo.tnsActive[HIFILT] ||
              st[0].tnsData.dataRaw.Long.subBlockInfo.tnsActive[LOFILT] ||
              st[channels - 1]
                  .tnsData.dataRaw.Long.subBlockInfo.tnsActive[HIFILT] ||
              st[channels - 1]
                  .tnsData.dataRaw.Long.subBlockInfo.tnsActive[LOFILT];
        }
      }
    }
    for (int c = 0; c < channels; c++) {
      PSY_CONFIGURATION *conf = isShort[c] ? &confShort : &confLong;
      if (tnsActive[0] && !isShort[c]) {
        for (int i = 0; i < st[c].lowpassLine; i++)
          st[c].spec[i] >>= 1;
        FIXP_DBL *T = st[c].thrU.Long;
        for (int sfb = 0; sfb < st[c].sfbActive; sfb++) T[sfb] >>= 2;
        st[c].mdctScale += 1;
      }
      for (int w = 0; w < nWin[c]; w++) {
        FDKaacEnc_TnsEncode(&st[c].tnsInfo, &st[c].tnsData, conf->sfbCnt,
                            isShort[c] ? &tnsConfShort : &tnsConfLong,
                            conf->sfbOffset[st[c].sfbActive],
                            st[c].spec + w * wLen[c], w,
                            st[c].bs.lastWindowSequence);
        if (tnsActive[w]) {
          FDKaacEnc_CalcSfbMaxScaleSpec(st[c].spec + w * wLen[c],
                                        conf->sfbOffset, st[c].maxScale[w],
                                        st[c].sfbActive);
        }
      }
      for (int w = 0; w < nWin[c]; w++) {
        if (tnsActive[w]) {
          if (isShort[c]) {
            FDKaacEnc_CalcBandEnergyOptimShort(
                st[c].spec + w * wLen[c], st[c].maxScale[w], conf->sfbOffset,
                st[c].sfbActive, st[c].enU.Short[w]);
          } else {
            nrgScaling[c] = FDKaacEnc_CalcBandEnergyOptimLong(
                st[c].spec, st[c].maxScale[w], conf->sfbOffset,
                st[c].sfbActive, st[c].enU.Long, st[c].enLd[w]);
            tnsSpecShift = fixMax(tnsSpecShift, nrgScaling[c]);
          }
        }
      }
    }
    /* overflow rescale AFTER all channels (tnsSpecShift is element-global) */
    for (int c = 0; c < channels; c++) {
      if ((tnsSpecShift != 0) && !isShort[c]) {
        for (int i = 0; i < st[c].lowpassLine; i++)
          st[c].spec[i] >>= tnsSpecShift;
        int scale = (tnsSpecShift - nrgScaling[c]) << 1;
        for (int sfb = 0; sfb < st[c].sfbActive; sfb++) {
          st[c].enLd[0][sfb] -= scale * FL2FXCONST_DBL(1.0 / LD_DATA_SCALING);
          st[c].enU.Long[sfb] >>= scale;
          st[c].thrU.Long[sfb] >>= (tnsSpecShift << 1);
        }
        st[c].mdctScale += tnsSpecShift;
      }
      if (!(tnsConfLong.tnsActive || tnsConfShort.tnsActive)) {
        std::memset(&st[c].tnsData, 0, sizeof(st[c].tnsData));
      }
    }

    /* threshold advance + MS + grouping + dumps */
    for (int c = 0; c < channels; c++) {
      PSY_CONFIGURATION *conf = isShort[c] ? &confShort : &confLong;
      int energyShift = st[c].mdctScale * 2;
      int clipNrgShift = energyShift - THR_SHIFTBITS;
      FIXP_DBL clipEnergy;
      int headroom = isShort[c] ? 6 : 0;
      if (clipNrgShift >= 0)
        clipEnergy = conf->clipEnergy >> clipNrgShift;
      else if (clipNrgShift >= -headroom)
        clipEnergy = conf->clipEnergy << -clipNrgShift;
      else
        clipEnergy = (FIXP_DBL)MAXVAL_DBL;
      for (int w = 0; w < nWin[c]; w++) {
        FIXP_DBL *T = isShort[c] ? st[c].thrU.Short[w] : st[c].thrU.Long;
        FIXP_DBL *E = isShort[c] ? st[c].enU.Short[w] : st[c].enU.Long;
        FIXP_DBL *S = isShort[c] ? st[c].spreadU.Short[w] : st[c].spreadU.Long;
        for (int i = 0; i < st[c].sfbActive; i++)
          T[i] = fixMin(T[i], clipEnergy);
        FDKaacEnc_SpreadingMax(st[c].sfbActive, conf->sfbMaskLowFactor,
                               conf->sfbMaskHighFactor, T);
        energyShift += PCM_QUANT_THR_SCALE;
        if (energyShift >= 0) {
          energyShift = fixMin(DFRACT_BITS - 1, energyShift);
          for (int i = 0; i < st[c].sfbActive; i++)
            T[i] = fixMax(T[i] >> THR_SHIFTBITS,
                          (conf->sfbPcmQuantThreshold[i] >> energyShift));
        } else {
          energyShift = fixMin(DFRACT_BITS - 1, -energyShift);
          for (int i = 0; i < st[c].sfbActive; i++)
            T[i] = fixMax(T[i] >> THR_SHIFTBITS,
                          (conf->sfbPcmQuantThreshold[i] << energyShift));
        }
        if (st[c].bs.lastWindowSequence == STOP_WINDOW) {
          for (int i = 0; i < st[c].sfbActive; i++)
            st[c].nm1[i] = (FIXP_DBL)MAXVAL_DBL;
          st[c].m_nm1 = 0;
          st[c].calcPre = 0;
        }
        FDKaacEnc_PreEchoControl(
            st[c].nm1, st[c].calcPre, st[c].sfbActive,
            conf->maxAllowedIncreaseFactor, conf->minRemainingThresholdFactor,
            T, st[c].mdctScale, &st[c].m_nm1);
        st[c].calcPre = 1;
        if (st[c].bs.lastWindowSequence == START_WINDOW) {
          for (int i = 0; i < st[c].sfbActive; i++)
            st[c].nm1[i] = (FIXP_DBL)MAXVAL_DBL;
          st[c].m_nm1 = 0;
          st[c].calcPre = 0;
        }
        std::memcpy(S, E, st[c].sfbActive * sizeof(FIXP_DBL));
        FDKaacEnc_SpreadingMax(st[c].sfbActive,
                               conf->sfbMaskLowFactorSprEn,
                               conf->sfbMaskHighFactorSprEn, S);
      }
    }

    /* MS energies (stereo only, element-level like psy_main:1021-1037) */
    if (channels == 2) {
      PSY_CONFIGURATION *conf1 = isShort[1] ? &confShort : &confLong;
      for (int w = 0; w < nWin[1]; w++) {
        FDKaacEnc_CalcBandNrgMSOpt(
            st[0].spec + w * wLen[1], st[1].spec + w * wLen[1],
            st[0].maxScale[w], st[1].maxScale[w], conf1->sfbOffset,
            st[0].sfbActive, st[0].msU.Short[w], st[1].msU.Short[w],
            (st[1].bs.lastWindowSequence != SHORT_WINDOW), st[0].msLd[w],
            st[1].msLd[w]);
      }
      for (int w = 0; w < nWin[1]; w++) {
        char tag[32];
        std::snprintf(tag, sizeof(tag), "MSM%d ", w);
        print_q31(out, tag, st[0].msU.Short[w], st[0].sfbActive);
        std::snprintf(tag, sizeof(tag), "MSS%d ", w);
        print_q31(out, tag, st[1].msU.Short[w], st[1].sfbActive);
        std::snprintf(tag, sizeof(tag), "MSMLD%d ", w);
        print_q31(out, tag, st[0].msLd[w], st[0].sfbActive);
        std::snprintf(tag, sizeof(tag), "MSSLD%d ", w);
        print_q31(out, tag, st[1].msLd[w], st[1].sfbActive);
      }
    } else {
      for (int c = 0; c < channels; c++) {
        /* mono: MS buffers stay zero (never written in C either) */
        std::memset(&st[c].msU, 0, sizeof(st[c].msU));
      }
    }

    /* grouping + output Ld + dumps */
    for (int c = 0; c < channels; c++) {
      PSY_CONFIGURATION *conf = isShort[c] ? &confShort : &confLong;
      std::fprintf(out, "PSY mdctScale=%d finalShift=%d tnsShift=%d nrgScaling=%d\n",
                   (int)st[c].mdctScale, (int)st[c].finalShiftDbg, tnsSpecShift,
                   nrgScaling[c]);
      print_ints(out, "MAXSCALE ", st[c].maxScale[0], st[c].sfbActive);
      if (!isShort[c]) {
        print_q31(out, "NRG ", st[c].enU.Long, st[c].sfbActive);
        print_q31(out, "NRGLD ", st[c].enLd[0], st[c].sfbActive);
        print_q31(out, "THR ", st[c].thrU.Long, st[c].sfbActive);
        print_q31(out, "SPREAD ", st[c].spreadU.Long, st[c].sfbActive);
      } else {
        for (int w = 0; w < nWin[c]; w++) {
          char tag[32];
          std::snprintf(tag, sizeof(tag), "NRG%d ", w);
          print_q31(out, tag, st[c].enU.Short[w], st[c].sfbActive);
          std::snprintf(tag, sizeof(tag), "NRGLD%d ", w);
          print_q31(out, tag, st[c].enLd[w], st[c].sfbActive);
          std::snprintf(tag, sizeof(tag), "THR%d ", w);
          print_q31(out, tag, st[c].thrU.Short[w], st[c].sfbActive);
          std::snprintf(tag, sizeof(tag), "SPREAD%d ", w);
          print_q31(out, tag, st[c].spreadU.Short[w], st[c].sfbActive);
        }
      }
      /* TNS dump */
      int nw = isShort[c] ? TRANS_FAC : 1;
      for (int w = 0; w < nw; w++) {
        std::fprintf(out, "TNSW w=%d nf=%d ord=%d,%d len=%d,%d dir=%d,%d coefs=",
                     w, st[c].tnsInfo.numOfFilters[w],
                     st[c].tnsInfo.order[w][0], st[c].tnsInfo.order[w][1],
                     st[c].tnsInfo.length[w][0], st[c].tnsInfo.length[w][1],
                     st[c].tnsInfo.direction[w][0],
                     st[c].tnsInfo.direction[w][1]);
        for (int k = 0; k < TNS_MAX_ORDER; k++)
          std::fprintf(out, "%s%d", k ? "," : "",
                       st[c].tnsInfo.coef[w][0][k]);
        std::fprintf(out, " | ");
        for (int k = 0; k < TNS_MAX_ORDER; k++)
          std::fprintf(out, "%s%d", k ? "," : "",
                       st[c].tnsInfo.coef[w][1][k]);
        std::fprintf(out, "\n");
        if (isShort[c]) {
          std::fprintf(out, "TNSG w=%d active=%d,%d gain=%d,%d merged=%d\n", w,
                       st[c].tnsData.dataRaw.Short.subBlockInfo[w].tnsActive[0],
                       st[c].tnsData.dataRaw.Short.subBlockInfo[w].tnsActive[1],
                       st[c].tnsData.dataRaw.Short.subBlockInfo[w].predictionGain[0],
                       st[c].tnsData.dataRaw.Short.subBlockInfo[w].predictionGain[1],
                       st[c].tnsData.filtersMerged);
        } else if (w == 0) {
          std::fprintf(out, "TNSG w=0 active=%d,%d gain=%d,%d merged=%d\n",
                       st[c].tnsData.dataRaw.Long.subBlockInfo.tnsActive[0],
                       st[c].tnsData.dataRaw.Long.subBlockInfo.tnsActive[1],
                       st[c].tnsData.dataRaw.Long.subBlockInfo.predictionGain[0],
                       st[c].tnsData.dataRaw.Long.subBlockInfo.predictionGain[1],
                       st[c].tnsData.filtersMerged);
        }
      }
      print_q31(out, "NM1 ", st[c].nm1, st[c].sfbActive);
      std::fprintf(out, "NM1STATE m1=%d calc=%d\n", (int)st[c].m_nm1,
                   (int)st[c].calcPre);

      if (isShort[c]) {
        int noSfb = st[c].bs.noOfGroups * confShort.sfbCnt;
        FDKaacEnc_groupShortData(
            st[c].spec, &st[c].thrU, &st[c].enU, &st[c].msU, &st[c].spreadU,
            confShort.sfbCnt, st[c].sfbActive, confShort.sfbOffset,
            confShort.sfbMinSnrLdData, st[c].groupedOff,
            &st[c].maxSfbPerGroup, st[c].groupedMinSnr,
            st[c].bs.noOfGroups, st[c].bs.groupLen, FRAME_LEN);
        /* grouped Ld (mirror psy_main:1060-1084) */
        for (int g = 0; g < noSfb; g += confShort.sfbCnt) {
          LdDataVector(&st[c].enU.Long[g], &st[c].groupedELd[g],
                       st[c].sfbActive);
          LdDataVector(&st[c].thrU.Long[g], &st[c].groupedTLd[g],
                       st[c].sfbActive);
          for (int sfb = 0; sfb < st[c].sfbActive; sfb++) {
            if (st[c].groupedTLd[g + sfb] <
                FL2FXCONST_DBL(-0.515625f))
              st[c].groupedTLd[g + sfb] = FL2FXCONST_DBL(-0.515625f);
          }
        }
        std::fprintf(out, "GROUPED maxSfb=%d noSfb=%d\n", st[c].maxSfbPerGroup,
                     noSfb);
        print_q31(out, "GE ", st[c].enU.Long, noSfb);
        print_q31(out, "GT ", st[c].thrU.Long, noSfb);
        print_q31(out, "GS ", st[c].spreadU.Long, noSfb);
        print_q31(out, "GELD ", st[c].groupedELd, noSfb);
        print_q31(out, "GTLD ", st[c].groupedTLd, noSfb);
        print_ints(out, "GOFF ", st[c].groupedOff, noSfb + 1);
        print_q31(out, "GMSNR ", st[c].groupedMinSnr, noSfb);
        print_q31(out, "GSPEC ", st[c].spec, FRAME_LEN);
      } else {
        /* long maxSfb (mirror psy_main:1092-1101) */
        int sfb, line;
        for (sfb = st[c].sfbActive - 1; sfb >= 0; sfb--) {
          for (line = confLong.sfbOffset[sfb + 1] - 1;
               line >= confLong.sfbOffset[sfb]; line--) {
            if (st[c].spec[line] != 0) break;
          }
          if (line > confLong.sfbOffset[sfb]) break;
        }
        st[c].maxSfbPerGroup = sfb + 1;
        if (fixMin(5, st[c].sfbActive) > st[c].maxSfbPerGroup)
          st[c].maxSfbPerGroup = fixMin(5, st[c].sfbActive);
        LdDataVector(st[c].thrU.Long, st[c].thrLd, st[c].sfbActive);
        for (int i = 0; i < st[c].sfbActive; i++) {
          if (st[c].thrLd[i] < FL2FXCONST_DBL(-0.515625f))
            st[c].thrLd[i] = FL2FXCONST_DBL(-0.515625f);
        }
        std::fprintf(out, "LONGOUT maxSfb=%d\n", st[c].maxSfbPerGroup);
        print_q31(out, "TLD ", st[c].thrLd, st[c].sfbActive);
      }

      /* rotate ring (mirror psy_main:618-627) */
      std::memmove(st[c].ring, st[c].ring + FRAME_LEN,
                   FRAME_LEN * sizeof(INT_PCM));
      std::memcpy(st[c].ring + FRAME_LEN, fresh[c] + (RING_LEN - BS_OFFSET),
                  (BS_OFFSET - FRAME_LEN) * sizeof(INT_PCM));
    }

    /* ---- S1 stage: stereo decisions (psy_main:1140-1246) + QCMainPrepare
       (qc_main:435-459) on COPIES via the real C functions. Existing dumps
       above are unaffected. ---- */
    {
      FIXP_DBL s1spec[2][FRAME_LEN];
      FIXP_DBL s1en[2][MAX_GROUPED_SFB], s1thr[2][MAX_GROUPED_SFB];
      FIXP_DBL s1enLd[2][MAX_GROUPED_SFB], s1thrLd[2][MAX_GROUPED_SFB];
      FIXP_DBL s1spread[2][MAX_GROUPED_SFB];
      FIXP_DBL s1msLin[2][MAX_GROUPED_SFB], s1msLd[2][MAX_GROUPED_SFB];
      INT s1isBook[2][MAX_GROUPED_SFB], s1isScale[2][MAX_GROUPED_SFB];
      INT s1noise[2][MAX_GROUPED_SFB];
      INT s1msMask[MAX_GROUPED_SFB];
      INT s1msDigest = MS_NONE;
      FIXP_SGL s1tonality[MAX_GROUPED_SFB];
      int s1cnt[2], s1perGroup[2];
      int s1maxSfb = 0;

      std::memset(s1msMask, 0, sizeof(s1msMask));
      std::memset(s1tonality, 0, sizeof(s1tonality));
      for (int c = 0; c < channels; c++) {
        if (isShort[c]) {
          s1cnt[c] = st[c].bs.noOfGroups * confShort.sfbCnt;
          s1perGroup[c] = confShort.sfbCnt;
          std::memcpy(s1en[c], st[c].enU.Long,
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s1thr[c], st[c].thrU.Long,
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s1enLd[c], st[c].groupedELd,
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s1thrLd[c], st[c].groupedTLd,
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s1spread[c], st[c].spreadU.Long,
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s1msLin[c], st[c].msU.Long,
                      s1cnt[c] * sizeof(FIXP_DBL));
        } else {
          s1cnt[c] = confLong.sfbActive;
          s1perGroup[c] = confLong.sfbActive;
          std::memcpy(s1en[c], st[c].enU.Long,
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s1thr[c], st[c].thrU.Long,
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s1enLd[c], st[c].enLd[0],
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s1thrLd[c], st[c].thrLd,
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s1spread[c], st[c].spreadU.Long,
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s1msLin[c], st[c].msU.Short[0],
                      s1cnt[c] * sizeof(FIXP_DBL));
        }
        /* MS Ld: single persistent array in real PSY_DATA (last long-frame
           values or zeros); oracle msLd[0] mirrors it (w=0 is the only
           window written for long blocks). */
        std::memcpy(s1msLd[c], st[c].msLd[0],
                    s1cnt[c] * sizeof(FIXP_DBL));
        std::memcpy(s1spec[c], st[c].spec, sizeof(s1spec[c]));
        s1maxSfb = fixMax(s1maxSfb, st[c].maxSfbPerGroup);
        std::memset(s1isBook[c], 0, sizeof(s1isBook[c]));
        std::memset(s1isScale[c], 0, sizeof(s1isScale[c]));
      }

      PNS_CONFIG s1pnsConf;
      PNS_DATA s1pns[2];
      PNS_DATA *s1pnsPtr[2] = {&s1pns[0], &s1pns[1]};
      std::memset(&s1pnsConf, 0, sizeof(s1pnsConf)); /* usePns = 0 */
      std::memset(s1pns, 0, sizeof(s1pns));

      /* PNS detect (real call; usePns=0 clears flags + NO_NOISE and returns
         before reading tonality/TNS args). */
      for (int c = 0; c < channels; c++) {
        PSY_CONFIGURATION *pconf = isShort[c] ? &confShort : &confLong;
        FDKaacEnc_PnsDetect(
            &s1pnsConf, &s1pns[c], st[c].bs.lastWindowSequence,
            st[c].sfbActive, st[c].maxSfbPerGroup, s1thrLd[c],
            pconf->sfbOffset, s1spec[c], st[c].maxScale[0], s1tonality,
            st[c].tnsInfo.order[0][0],
            st[c].tnsData.dataRaw.Long.subBlockInfo.predictionGain[HIFILT],
            st[c].tnsData.dataRaw.Long.subBlockInfo.tnsActive[HIFILT],
            s1enLd[c], s1noise[c]);
      }

      if (channels == 2) {
        /* commonWindow is unconditionally 1 (psy_main:413). */
        s1maxSfb = fixMax(st[0].maxSfbPerGroup, st[1].maxSfbPerGroup);
        PSY_DATA s1psyData[2];
        PSY_OUT_CHANNEL s1psyOut[2];
        std::memset(s1psyData, 0, sizeof(s1psyData));
        std::memset(s1psyOut, 0, sizeof(s1psyOut));
        for (int c = 0; c < 2; c++) {
          s1psyData[c].mdctSpectrum = s1spec[c];
          s1psyData[c].sfbActive = st[c].sfbActive;
          s1psyData[c].mdctScale = st[c].mdctScale;
          for (int i = 0; i < s1cnt[c]; i++) {
            s1psyData[c].sfbEnergy.Long[i] = s1en[c][i];
            s1psyData[c].sfbThreshold.Long[i] = s1thr[c][i];
            s1psyData[c].sfbEnergyLdData.Long[i] = s1enLd[c][i];
            s1psyData[c].sfbSpreadEnergy.Long[i] = s1spread[c][i];
            s1psyData[c].sfbEnergyMS.Long[i] = s1msLin[c][i];
          }
          for (int i = 0; i < s1cnt[c]; i++)
            s1psyData[c].sfbEnergyMSLdData[i] = s1msLd[c][i];
          s1psyOut[c].sfbCnt = s1cnt[c];
          s1psyOut[c].sfbPerGroup = s1perGroup[c];
          s1psyOut[c].maxSfbPerGroup = s1maxSfb;
          s1psyOut[c].sfbEnergyLdData = s1enLd[c];
          s1psyOut[c].sfbThresholdLdData = s1thrLd[c];
        }
        PSY_DATA *s1pd[2] = {&s1psyData[0], &s1psyData[1]};
        PSY_OUT_CHANNEL *s1po[2] = {&s1psyOut[0], &s1psyOut[1]};

        if (st[0].bs.lastWindowSequence != SHORT_WINDOW) {
          /* long-block stereo path (psy_main:1167-1199). */
          FDKaacEnc_PreProcessPnsChannelPair(
              st[0].sfbActive, s1psyData[0].sfbEnergy.Long,
              s1psyData[1].sfbEnergy.Long, s1enLd[0], s1enLd[1], s1msLin[0],
              &s1pnsConf, &s1pns[0], &s1pns[1]);
          FDKaacEnc_IntensityStereoProcessing(
              s1psyData[0].sfbEnergy.Long, s1psyData[1].sfbEnergy.Long,
              s1spec[0], s1spec[1], s1psyData[0].sfbThreshold.Long,
              s1psyData[1].sfbThreshold.Long, s1thrLd[1],
              s1psyData[0].sfbSpreadEnergy.Long,
              s1psyData[1].sfbSpreadEnergy.Long, s1enLd[0], s1enLd[1],
              &s1msDigest, s1msMask, confLong.sfbCnt, confLong.sfbCnt, s1maxSfb,
              confLong.sfbOffset, confLong.allowIS, s1isBook[1],
              s1isScale[1], s1pnsPtr);
          FDKaacEnc_MsStereoProcessing(
              s1pd, s1po, s1isBook[1], &s1msDigest, s1msMask, confLong.allowMS,
              st[0].sfbActive, st[0].sfbActive, s1maxSfb,
              confLong.sfbOffset);
          FDKaacEnc_PostProcessPnsChannelPair(
              st[0].sfbActive, &s1pnsConf, &s1pns[0], &s1pns[1], s1msMask,
              &s1msDigest);
        } else {
          /* short-block stereo path (psy_main:1201-1226, grouped arrays). */
          int noSfb = st[0].bs.noOfGroups * confShort.sfbCnt;
          FDKaacEnc_IntensityStereoProcessing(
              s1psyData[0].sfbEnergy.Long, s1psyData[1].sfbEnergy.Long,
              s1spec[0], s1spec[1], s1psyData[0].sfbThreshold.Long,
              s1psyData[1].sfbThreshold.Long, s1thrLd[1],
              s1psyData[0].sfbSpreadEnergy.Long,
              s1psyData[1].sfbSpreadEnergy.Long, s1enLd[0], s1enLd[1],
              &s1msDigest, s1msMask, noSfb, confShort.sfbCnt, s1maxSfb,
              st[0].groupedOff, confLong.allowIS, s1isBook[1],
              s1isScale[1], s1pnsPtr);
          FDKaacEnc_MsStereoProcessing(
              s1pd, s1po, s1isBook[1], &s1msDigest, s1msMask, confShort.allowMS,
              noSfb, confShort.sfbCnt, s1maxSfb, st[0].groupedOff);
        }
        /* Sync back MS/IS-modified linear energies into the S1 copies
           (spectra and Ld vectors were modified in place already). Spread
           is synced too: MsStereoProcessing rewrites spread for MS bands
           and S2 consumes post-MS spread. */
        for (int c = 0; c < 2; c++) {
          for (int i = 0; i < s1cnt[c]; i++) {
            s1en[c][i] = s1psyData[c].sfbEnergy.Long[i];
            s1thr[c][i] = s1psyData[c].sfbThreshold.Long[i];
            s1spread[c][i] = s1psyData[c].sfbSpreadEnergy.Long[i];
          }
        }
      }

      /* PNS coding (real call; usePns=0 fills NO_NOISE and returns). */
      for (int c = 0; c < channels; c++) {
        FDKaacEnc_CodePnsChannel(st[c].sfbActive, &s1pnsConf,
                                 s1pns[c].pnsFlag, s1enLd[c], s1noise[c],
                                 s1thrLd[c]);
      }

      /* QCMainPrepare working state: inputs copied into QC arrays (value
         flow mirrors the aliased C buffers; mutation stays in s1qoc). */
      PSY_OUT_CHANNEL s1poc[2];
      QC_OUT_CHANNEL s1qoc[2];
      std::memset(s1poc, 0, sizeof(s1poc));
      std::memset(s1qoc, 0, sizeof(s1qoc));
      for (int c = 0; c < channels; c++) {
        s1poc[c].sfbCnt = s1cnt[c];
        s1poc[c].sfbPerGroup = s1perGroup[c];
        s1poc[c].maxSfbPerGroup =
            (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup;
        s1poc[c].lastWindowSequence = st[c].bs.lastWindowSequence;
        s1poc[c].windowShape = st[c].bs.windowShape;
        s1poc[c].mdctScale = st[c].mdctScale;
        s1poc[c].mdctSpectrum = s1spec[c];
        s1poc[c].sfbEnergyLdData = s1enLd[c];
        s1poc[c].sfbThresholdLdData = s1thrLd[c];
        s1poc[c].sfbEnergy = s1en[c];
        if (isShort[c]) {
          /* grouping mask (psy_main:1273-1282). */
          int mask = 0;
          for (int g = 0; g < st[c].bs.noOfGroups; g++) {
            mask <<= 1;
            for (int j = 1; j < st[c].bs.groupLen[g]; j++)
              mask = (mask << 1) | 1;
          }
          s1poc[c].groupingMask = mask;
          std::memcpy(s1poc[c].sfbOffsets, st[c].groupedOff,
                      (s1cnt[c] + 1) * sizeof(INT));
        } else {
          s1poc[c].groupingMask = 0;
          std::memcpy(s1poc[c].sfbOffsets, confLong.sfbOffset,
                      (s1cnt[c] + 1) * sizeof(INT));
        }
        for (int i = 0; i < s1cnt[c]; i++) {
          s1poc[c].isBook[i] = s1isBook[c][i];
          s1poc[c].isScale[i] = s1isScale[c][i];
          s1poc[c].noiseNrg[i] = s1noise[c][i];
        }
        s1poc[c].tnsInfo = st[c].tnsInfo;
        std::memcpy(s1qoc[c].mdctSpectrum, s1spec[c], sizeof(s1spec[c]));
        std::memcpy(s1qoc[c].sfbEnergyLdData, s1enLd[c],
                    s1cnt[c] * sizeof(FIXP_DBL));
        std::memcpy(s1qoc[c].sfbThresholdLdData, s1thrLd[c],
                    s1cnt[c] * sizeof(FIXP_DBL));
        std::memcpy(s1qoc[c].sfbEnergy, s1en[c],
                    s1cnt[c] * sizeof(FIXP_DBL));
      }
      PSY_OUT_CHANNEL *s1pocPtr[2] = {&s1poc[0], &s1poc[1]};
      QC_OUT_CHANNEL *s1qocPtr[2] = {&s1qoc[0], &s1qoc[1]};

      /* Form factor (real call, qc_main:446). */
      FDKaacEnc_CalcFormFactor(s1qocPtr, s1pocPtr, channels);

      /* PE (real call, qc_main:450). ATS persists across frames. */
      PE_DATA s1pe;
      std::memset(&s1pe, 0, sizeof(s1pe));
      TOOLSINFO s1tools;
      std::memset(&s1tools, 0, sizeof(s1tools));
      s1tools.msDigest = s1msDigest;
      if (channels == 2)
        std::memcpy(s1tools.msMask, s1msMask, sizeof(s1msMask));
      FIXP_DBL s1ats_pre_enfac[2] = {s1ats.chaosMeasureEnFac[0],
                                     s1ats.chaosMeasureEnFac[1]};
      INT s1ats_pre_patch[2] = {s1ats.lastEnFacPatch[0],
                                s1ats.lastEnFacPatch[1]};
      FDKaacEnc_peCalculation(&s1pe, s1pocPtr, s1qocPtr, &s1tools, &s1ats,
                             channels);

      /* Static bits (real call, qc_main:454). */
      ELEMENT_INFO s1el;
      std::memset(&s1el, 0, sizeof(s1el));
      s1el.elType = (channels == 2) ? ID_CPE : ID_SCE;
      s1el.instanceTag = 0;
      s1el.nChannelsInEl = channels;
      PSY_OUT_ELEMENT s1elem;
      std::memset(&s1elem, 0, sizeof(s1elem));
      s1elem.psyOutChannel[0] = &s1poc[0];
      s1elem.psyOutChannel[1] = &s1poc[1];
      s1elem.commonWindow = 1;
      s1elem.toolsInfo = s1tools;
      INT s1staticBits = 0;
      if (FDKaacEnc_ChannelElementWrite(NULL, &s1el, NULL, &s1elem, s1pocPtr,
                                        0, AOT_AAC_LC, -1, &s1staticBits,
                                        0) != AAC_ENC_OK) {
        std::fprintf(stderr, "S1 static count failed at frame %ld\n", f);
        return 1;
      }

      /* Dumps. Input snapshots (SPEC/PENLD/PTHLD) are the exact Q31
         vectors the real C S1 functions consumed; outputs follow.
         ATS dumps the pre-frame weighting state (injected by Rust tests)
         so no cross-frame history replay is needed. */
      std::fprintf(out, "S1FRAME f=%ld\n", f);
      std::fprintf(out, "ATS pre=%d,%d,%d,%d\n", (int)s1ats_pre_enfac[0],
                   (int)s1ats_pre_enfac[1], (int)s1ats_pre_patch[0],
                   (int)s1ats_pre_patch[1]);
      std::fprintf(out, "ATS post=%d,%d,%d,%d\n",
                   (int)s1ats.chaosMeasureEnFac[0],
                   (int)s1ats.chaosMeasureEnFac[1],
                   (int)s1ats.lastEnFacPatch[0],
                   (int)s1ats.lastEnFacPatch[1]);
      for (int c = 0; c < channels; c++) {
        std::fprintf(out, "S1GEO ch=%d cnt=%d per=%d max=%d seq=%d\n", c,
                     s1cnt[c], s1perGroup[c],
                     (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup,
                     (int)st[c].bs.lastWindowSequence);
        /* Exact SFB offsets consumed below (grouped for short). */
        std::fprintf(out, "SOFF");
        const INT *s1off = isShort[c] ? st[c].groupedOff : confLong.sfbOffset;
        for (int i = 0; i <= s1cnt[c]; i++)
          std::fprintf(out, " %d", (int)s1off[i]);
        std::fprintf(out, "\n");
        /* TNS decisions consumed by static-bit counting (same layout as
           the M3 TNSW lines, plus per-window coefRes). */
        int s1nwin = isShort[c] ? TRANS_FAC : 1;
        for (int w = 0; w < s1nwin; w++) {
          std::fprintf(out,
                       "STNSW w=%d nf=%d ord=%d,%d len=%d,%d dir=%d,%d coefs=",
                       w, st[c].tnsInfo.numOfFilters[w],
                       st[c].tnsInfo.order[w][0], st[c].tnsInfo.order[w][1],
                       st[c].tnsInfo.length[w][0], st[c].tnsInfo.length[w][1],
                       st[c].tnsInfo.direction[w][0],
                       st[c].tnsInfo.direction[w][1]);
          for (int k = 0; k < TNS_MAX_ORDER; k++)
            std::fprintf(out, "%s%d", k ? "," : "",
                         st[c].tnsInfo.coef[w][0][k]);
          std::fprintf(out, " | ");
          for (int k = 0; k < TNS_MAX_ORDER; k++)
            std::fprintf(out, "%s%d", k ? "," : "",
                         st[c].tnsInfo.coef[w][1][k]);
          std::fprintf(out, "\n");
        }
        std::fprintf(out, "STNSR");
        for (int w = 0; w < s1nwin; w++)
          std::fprintf(out, " %d", (int)st[c].tnsInfo.coefRes[w]);
        std::fprintf(out, "\n");
        std::fprintf(out, "SPEC ch=%d", c);
        for (int i = 0; i < FRAME_LEN; i++)
          std::fprintf(out, " %d", (int)s1spec[c][i]);
        std::fprintf(out, "\n");
        print_q31(out, "PENLD ", s1enLd[c], s1cnt[c]);
        print_q31(out, "PTHLD ", s1thrLd[c], s1cnt[c]);
        print_q31(out, "SENL ", s1en[c], s1cnt[c]);
        print_q31(out, "FF ", s1qoc[c].sfbFormFactorLdData, s1cnt[c]);
        print_q31(out, "ENFAC ", s1qoc[c].sfbEnFacLd, s1cnt[c]);
        print_q31(out, "WENLD ", s1qoc[c].sfbWeightedEnergyLdData, s1cnt[c]);
        print_q31(out, "WTHLD ", s1qoc[c].sfbThresholdLdData, s1cnt[c]);
        /* Per-band PE details (branch diagnosability: C1≷C2/C3, IS-cost). */
        std::fprintf(out, "SPNL");
        for (int i = 0; i < s1cnt[c]; i++)
          std::fprintf(out, " %d", (int)s1pe.peChannelData[c].sfbNLines[i]);
        std::fprintf(out, "\nSPECH");
        for (int i = 0; i < s1cnt[c]; i++)
          std::fprintf(out, " %d", (int)s1pe.peChannelData[c].sfbPe[i]);
        std::fprintf(out, "\nSPCO");
        for (int i = 0; i < s1cnt[c]; i++)
          std::fprintf(out, " %d", (int)s1pe.peChannelData[c].sfbConstPart[i]);
        std::fprintf(out, "\nSPNA");
        for (int i = 0; i < s1cnt[c]; i++)
          std::fprintf(out, " %d", (int)s1pe.peChannelData[c].sfbNActiveLines[i]);
        std::fprintf(out, "\n");
        std::fprintf(out, "PECH ch=%d pe=%d const=%d nact=%d\n", c,
                     s1pe.peChannelData[c].pe, s1pe.peChannelData[c].constPart,
                     s1pe.peChannelData[c].nActiveLines);
        print_ints(out, "ISB ", s1isBook[c], s1cnt[c]);
        print_ints(out, "ISS ", s1isScale[c], s1cnt[c]);
        print_ints(out, "NOISE ", s1noise[c], st[c].sfbActive);
      }
      if (channels == 2) {
        std::fprintf(out, "MSD %d", (int)s1msDigest);
        for (int i = 0; i < s1cnt[0]; i++)
          std::fprintf(out, " %d", (int)s1msMask[i]);
        std::fprintf(out, "\n");
      }
      std::fprintf(out, "PE %d %d %d %d\n", (int)s1pe.pe,
                   (int)s1pe.constPart, (int)s1pe.nActiveLines,
                   (int)s1pe.offset);
      std::fprintf(out, "STATIC %s %d\n", channels == 2 ? "CPE" : "SCE",
                   (int)s1staticBits);
      /* S6 saves: element PE + static bits (S1 scope ends here). */
      s6peSaved = s1pe.pe;
      s6staticSaved = s1staticBits;
      s6toolsSaved = s1tools;
      /* S8 saves: element-assembly side info (s1poc psy outputs). */
      for (int c = 0; c < channels; c++) {
        s8wshape[c] = s1poc[c].windowShape;
        s8gmask[c] = s1poc[c].groupingMask;
        s8mdct[c] = s1poc[c].mdctScale;
      }

      /* ---- S2 stage: VBR threshold adjustment (AdjustThresholds VBR
         branch, adj_thr.cpp:2869-2899) via the real exported call.
         Buffers mirror the aliased encoder flow (aacenc.cpp:803-808):
         psyOut reads and qcOut writes start from identical post-S1
         values (the VBR path never writes psy arrays, so separate
         copies are numerically identical to the aliased buffers).
         Inputs: weighted energies/thresholds (WENLD/WTHLD), original
         linear/Ld energies (SENL/PENLD), form factors, EnFac, minSnr
         init (grouped config for short, static config for long),
         post-MS spread. s1ats carries the persistent chaosMeasureOld
         (S1 enFac evolution and S2 chaos evolution share the one
         ATS_ELEMENT exactly like the encoder). ---- */
      {
        FIXP_DBL s2en[2][MAX_GROUPED_SFB], s2enLd[2][MAX_GROUPED_SFB];
        FIXP_DBL s2wEn[2][MAX_GROUPED_SFB], s2thr[2][MAX_GROUPED_SFB];
        FIXP_DBL s2ff[2][MAX_GROUPED_SFB], s2enfac[2][MAX_GROUPED_SFB];
        FIXP_DBL s2minsnr[2][MAX_GROUPED_SFB], s2spread[2][MAX_GROUPED_SFB];
        FIXP_DBL s2thrExp[2][MAX_GROUPED_SFB];
        PSY_OUT_CHANNEL s2poc[2];
        QC_OUT_CHANNEL s2qoc[2];
        std::memset(s2poc, 0, sizeof(s2poc));
        std::memset(s2qoc, 0, sizeof(s2qoc));
        std::memset(s2thrExp, 0, sizeof(s2thrExp));
        for (int c = 0; c < channels; c++) {
          for (int i = 0; i < s1cnt[c]; i++) {
            s2en[c][i] = s1en[c][i];
            s2enLd[c][i] = s1enLd[c][i];
            s2wEn[c][i] = s1qoc[c].sfbWeightedEnergyLdData[i];
            s2thr[c][i] = s1qoc[c].sfbThresholdLdData[i];
            s2ff[c][i] = s1qoc[c].sfbFormFactorLdData[i];
            s2enfac[c][i] = s1qoc[c].sfbEnFacLd[i];
            s2spread[c][i] = s1spread[c][i];
            if (isShort[c])
              s2minsnr[c][i] = st[c].groupedMinSnr[i];
            else
              s2minsnr[c][i] = confLong.sfbMinSnrLdData[i];
          }
          s2poc[c].sfbCnt = s1cnt[c];
          s2poc[c].sfbPerGroup = s1perGroup[c];
          s2poc[c].maxSfbPerGroup =
              (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup;
          s2poc[c].lastWindowSequence = st[c].bs.lastWindowSequence;
          s2poc[c].sfbEnergy = s2en[c];
          s2poc[c].sfbEnergyLdData = s2enLd[c];
          s2poc[c].sfbThresholdLdData = s2thr[c];
          s2poc[c].sfbMinSnrLdData = s2minsnr[c];
          s2poc[c].sfbSpreadEnergy = s2spread[c];
          std::memcpy(s2poc[c].sfbOffsets, s1poc[c].sfbOffsets,
                      (s1cnt[c] + 1) * sizeof(INT));
          std::memcpy(s2poc[c].groupLen, st[c].bs.groupLen,
                      MAX_NO_OF_GROUPS * sizeof(INT));
          std::memcpy(s2qoc[c].sfbEnergy, s2en[c],
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s2qoc[c].sfbEnergyLdData, s2enLd[c],
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s2qoc[c].sfbWeightedEnergyLdData, s2wEn[c],
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s2qoc[c].sfbThresholdLdData, s2thr[c],
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s2qoc[c].sfbFormFactorLdData, s2ff[c],
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s2qoc[c].sfbEnFacLd, s2enfac[c],
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s2qoc[c].sfbMinSnrLdData, s2minsnr[c],
                      s1cnt[c] * sizeof(FIXP_DBL));
          std::memcpy(s2qoc[c].sfbSpreadEnergy, s2spread[c],
                      s1cnt[c] * sizeof(FIXP_DBL));
          /* thrExp input probe via the real inline (calcThreshExp
             adj_thr.cpp:454 reads the weighted psy thresholds). */
          for (int g = 0; g < s1cnt[c]; g += s1perGroup[c])
            for (int b = 0; b < ((channels == 2) ? s1maxSfb
                                                : st[c].maxSfbPerGroup); b++)
              s2thrExp[c][g + b] = CalcInvLdData(s2thr[c][g + b] >> 2);
        }
        PSY_OUT_CHANNEL *s2pocPtr[2] = {&s2poc[0], &s2poc[1]};
        QC_OUT_CHANNEL *s2qocPtr[2] = {&s2qoc[0], &s2qoc[1]};
        PSY_OUT_ELEMENT s2pelem;
        std::memset(&s2pelem, 0, sizeof(s2pelem));
        s2pelem.psyOutChannel[0] = &s2poc[0];
        s2pelem.psyOutChannel[1] = &s2poc[1];
        s2pelem.commonWindow = 1;
        s2pelem.toolsInfo = s1tools;
        const PSY_OUT_ELEMENT *s2pe[((8))] = {&s2pelem, 0, 0, 0, 0, 0, 0, 0};
        QC_OUT_ELEMENT s2qel;
        std::memset(&s2qel, 0, sizeof(s2qel));
        s2qel.qcOutChannel[0] = &s2qoc[0];
        s2qel.qcOutChannel[1] = &s2qoc[1];
        QC_OUT_ELEMENT *s2qe[((8))] = {&s2qel, 0, 0, 0, 0, 0, 0, 0};
        QC_OUT s2qo;
        std::memset(&s2qo, 0, sizeof(s2qo));
        FIXP_DBL s2chaos_pre = s1ats.chaosMeasureOld;
        FDKaacEnc_AdjustThresholds(&s2hAdjThr, s2qe, &s2qo, s2pe, 0, &s2cm);

        std::fprintf(out, "S2FRAME f=%ld\n", f);
        std::fprintf(out, "S2CHAOS pre=%d post=%d\n", (int)s2chaos_pre,
                     (int)s1ats.chaosMeasureOld);
        for (int c = 0; c < channels; c++) {
          std::fprintf(out, "S2GEO ch=%d cnt=%d per=%d max=%d seq=%d\n", c,
                       s1cnt[c], s1perGroup[c],
                       (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup,
                       (int)st[c].bs.lastWindowSequence);
          std::fprintf(out, "GROUPLEN");
          for (int g = 0; g < MAX_NO_OF_GROUPS; g++)
            std::fprintf(out, " %d", (int)st[c].bs.groupLen[g]);
          std::fprintf(out, "\n");
          print_q31(out, "THREXP ", s2thrExp[c], s1cnt[c]);
          print_q31(out, "MINSNR0 ", s2minsnr[c], s1cnt[c]);
          print_q31(out, "SPREAD0 ", s1spread[c], s1cnt[c]);
          print_q31(out, "THRV ", s2qoc[c].sfbThresholdLdData, s1cnt[c]);
          print_q31(out, "MINSNR1 ", s2qoc[c].sfbMinSnrLdData, s1cnt[c]);
          print_q31(out, "SPREAD1 ", s2qoc[c].sfbSpreadEnergy, s1cnt[c]);
        }
        std::fprintf(out, "S2END\n");

        /* ---- S4 stage: scalefactor estimation + afterburner
           (sf_estim.cpp) via the real exported `FDKaacEnc_EstimateScaleFactors`.
           Inputs mirror the aliased encoder (`aacenc.cpp:803-808`):
           spectrum = S1 post-MS/IS spectrum with inactive-band lines zeroed
           (S3 input contract — unread downstream, every reference loop
           bounded by maxSfbPerGroup); energies = S1 unweighted Ld
           (`s1enLd`, i.e. PENLD); thresholds = S2 post-VBR (`THRV`);
           form factors = S1 (`FF`). psy/qc pointers alias the same working
           spectrum buffer, exactly like the encoder.
           Three invQuant levels are dumped on identical inputs: 0 gives the
           initial-formula scf (no search), 1 adds `improveScf` + single
           assimilation, 2 is the full afterburner (S4). Fresh spectrum
           copies per level (the call zeroes dead bands in place).
           Valid-envelope gate like S3: any `SHORT_MIN` line or
           `totalShift < 0` recomputation aborts loudly. ---- */
        {
          FIXP_DBL s4base[2][FRAME_LEN];
          for (int c = 0; c < channels; c++) {
            std::memcpy(s4base[c], s1spec[c], sizeof(s4base[c]));
            int maxSfb = (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup;
            for (int o = 0; o < s1cnt[c]; o += s1perGroup[c])
              for (int b = maxSfb; b < s1perGroup[c]; b++)
                for (int line = s1poc[c].sfbOffsets[o + b];
                     line < s1poc[c].sfbOffsets[o + b + 1]; line++)
                  s4base[c][line] = 0;
          }
          std::fprintf(out, "S4FRAME f=%ld\n", f);
          for (int c = 0; c < channels; c++) {
            std::fprintf(out, "S4GEO ch=%d cnt=%d per=%d max=%d seq=%d\n", c,
                         s1cnt[c], s1perGroup[c],
                         (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup,
                         (int)st[c].bs.lastWindowSequence);
            std::fprintf(out, "S4OFF");
            for (int i = 0; i <= s1cnt[c]; i++)
              std::fprintf(out, " %d", (int)s1poc[c].sfbOffsets[i]);
            std::fprintf(out, "\n");
          }
          static const int S4_INVS[] = {0, 1, 2};
          for (unsigned vi = 0;
               vi < sizeof(S4_INVS) / sizeof(S4_INVS[0]); vi++) {
            int inv = S4_INVS[vi];
            PSY_OUT_CHANNEL s4poc[2];
            QC_OUT_CHANNEL s4qoc[2];
            FIXP_DBL s4spec[2][FRAME_LEN];
            std::memset(s4poc, 0, sizeof(s4poc));
            std::memset(s4qoc, 0, sizeof(s4qoc));
            for (int c = 0; c < channels; c++) {
              std::memcpy(s4spec[c], s4base[c], sizeof(s4spec[c]));
              s4poc[c].sfbCnt = s1cnt[c];
              s4poc[c].sfbPerGroup = s1perGroup[c];
              s4poc[c].maxSfbPerGroup =
                  (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup;
              std::memcpy(s4poc[c].sfbOffsets, s1poc[c].sfbOffsets,
                          (s1cnt[c] + 1) * sizeof(INT));
              s4poc[c].mdctSpectrum = s4spec[c];
              s4poc[c].sfbEnergyLdData = s1enLd[c];
              s4poc[c].sfbThresholdLdData = s2qoc[c].sfbThresholdLdData;
              std::memcpy(s4qoc[c].mdctSpectrum, s4spec[c],
                          sizeof(s4spec[c]));
              std::memcpy(s4qoc[c].sfbEnergyLdData, s1enLd[c],
                          s1cnt[c] * sizeof(FIXP_DBL));
              std::memcpy(s4qoc[c].sfbThresholdLdData,
                          s2qoc[c].sfbThresholdLdData,
                          s1cnt[c] * sizeof(FIXP_DBL));
              std::memcpy(s4qoc[c].sfbFormFactorLdData,
                          s1qoc[c].sfbFormFactorLdData,
                          s1cnt[c] * sizeof(FIXP_DBL));
            }
            PSY_OUT_CHANNEL *s4pocPtr[2] = {&s4poc[0], &s4poc[1]};
            QC_OUT_CHANNEL *s4qocPtr[2] = {&s4qoc[0], &s4qoc[1]};
            FDKaacEnc_EstimateScaleFactors(s4pocPtr, s4qocPtr, inv, 0,
                                           channels);
            // `mdctSpectrum` is an array in QC_OUT (copied in); sync the
            // working buffer back so dumps/envelope read the mutated
            // spectrum (dead bands zeroed), mirroring the aliased encoder.
            for (int c = 0; c < channels; c++) {
              std::memcpy(s4spec[c], s4qoc[c].mdctSpectrum,
                          sizeof(s4spec[c]));
            }
            for (int c = 0; c < channels; c++) {
              for (int i = 0; i < FRAME_LEN; i++) {
                if ((int)s4qoc[c].quantSpec[i] == -32768) {
                  std::fprintf(stderr,
                               "S4 ENVELOPE VIOLATION (q==SHORT_MIN) f=%ld ch=%d inv=%d\n",
                               f, c, inv);
                  return 1;
                }
              }
              // totalShift gate per live line (S3 probe logic).
              int maxSfb =
                  (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup;
              for (int o = 0; o < s1cnt[c]; o += s1perGroup[c])
                for (int b = 0; b < maxSfb; b++) {
                  int qss = s4qoc[c].globalGain - s4qoc[c].scf[o + b];
                  // Dead bands (scf==0 after inversion with zeroed
                  // spectrum) quantize zeros; still gate them.
                  int qs = ((-qss) >> 2) + 1;
                  for (int line = s1poc[c].sfbOffsets[o + b];
                       line < s1poc[c].sfbOffsets[o + b + 1]; line++) {
                    SHORT qtab = FDKaacEnc_quantTableQ[(-qss) & 3];
                    INT64 acc64 = ((INT64)((INT)((int)qtab << 16))) *
                                  ((INT64)s4spec[c][line]);
                    INT a = (INT)(acc64 >> 32);
                    if (a != 0) {
                      if (a < 0) a = -a;
                      int sh = CntLeadingZeros(a) - 1;
                      int t = qs - sh + 1;
                      if ((16 - 4) - (3 * (t >> 2)) < 0) {
                        std::fprintf(stderr,
                                     "S4 ENVELOPE VIOLATION (totalShift<0) f=%ld ch=%d inv=%d\n",
                                     f, c, inv);
                        return 1;
                      }
                    }
                  }
                }
            }
            for (int c = 0; c < channels; c++) {
              std::fprintf(out, "S4SCF inv=%d ch=%d", inv, c);
              for (int i = 0; i < s1cnt[c]; i++)
                std::fprintf(out, " %d", (int)s4qoc[c].scf[i]);
              std::fprintf(out, "\n");
              std::fprintf(out, "S4GAIN inv=%d ch=%d %d\n", inv, c,
                           (int)s4qoc[c].globalGain);
              std::fprintf(out, "S4Q inv=%d ch=%d", inv, c);
              for (int i = 0; i < FRAME_LEN; i++)
                std::fprintf(out, " %d", (int)s4qoc[c].quantSpec[i]);
              std::fprintf(out, "\n");
              std::fprintf(out, "S4SPEC inv=%d ch=%d", inv, c);
              for (int i = 0; i < FRAME_LEN; i++)
                std::fprintf(out, " %d", (int)s4spec[c][i]);
              std::fprintf(out, "\n");
              // Recomputed per-band distortion/energy with the final QSS
              // (diagnostic, mirrors S3DIST/S3EN; internal sfbDist scratch
              // is not exposed by the exported call).
              std::fprintf(out, "S4DIST inv=%d ch=%d", inv, c);
              for (int b = 0; b < s1cnt[c]; b++) {
                int start = s1poc[c].sfbOffsets[b];
                int width = s1poc[c].sfbOffsets[b + 1] - start;
                int qss =
                    s4qoc[c].globalGain - s4qoc[c].scf[b];
                SHORT scratch[1024];
                FIXP_DBL d = FDKaacEnc_calcSfbDist(s4spec[c] + start,
                                                  scratch, width, qss, 0);
                std::fprintf(out, " %d", (int)d);
              }
              std::fprintf(out, "\n");
              std::fprintf(out, "S4EN inv=%d ch=%d en", inv, c);
              for (int b = 0; b < s1cnt[c]; b++) {
                int start = s1poc[c].sfbOffsets[b];
                int width = s1poc[c].sfbOffsets[b + 1] - start;
                int qss = s4qoc[c].globalGain - s4qoc[c].scf[b];
                FIXP_DBL en = 0, ds = 0;
                FDKaacEnc_calcSfbQuantEnergyAndDist(
                    s4spec[c] + start, s4qoc[c].quantSpec + start, width,
                    qss, &en, &ds);
                std::fprintf(out, " %d", (int)en);
                (void)ds;
              }
              // Second half: dist values (separate loop keeps line parseable).
              std::fprintf(out, " dist");
              for (int b = 0; b < s1cnt[c]; b++) {
                int start = s1poc[c].sfbOffsets[b];
                int width = s1poc[c].sfbOffsets[b + 1] - start;
                int qss = s4qoc[c].globalGain - s4qoc[c].scf[b];
                FIXP_DBL en = 0, ds = 0;
                FDKaacEnc_calcSfbQuantEnergyAndDist(
                    s4spec[c] + start, s4qoc[c].quantSpec + start, width,
                    qss, &en, &ds);
                std::fprintf(out, " %d", (int)ds);
              }
              std::fprintf(out, "\n");
            }
          }
          std::fprintf(out, "S4END\n");
        }

      /* ---- S5 stage: noiseless bit counting + sectioning (dyn_bits.cpp,
         bit_cnt.cpp) via the real exported `FDKaacEnc_dynBitCount`.
         Inputs mirror the QCMain gain loop (qc_main.cpp:994-999): per
         channel, quantSpectrum = S4 inv=2 final lines, maxValueInSfb =
         per-band max abs (inline replica of the S6-owned static
         `FDKaacEnc_calcMaxValueInSfb`, qc_main.cpp:1219 — same loop,
         zero-init for determinism; unvisited bands are never read by
         sectioning/scfCount), scalefac = S4 inv=2 post-inversion scf,
         blockType = lastWindowSequence, geometry/noiseNrg/isBook/isScale
         from the S1 psy outputs, syntaxFlags = 0 (encoder default,
         aacenc.cpp:358, so useVCB11 = 0). BITCNTR_STATE is wired manually
         (plain struct of two scratch arrays; no behavior change).
         S4 inv=2 is recomputed into fresh S5 buffers (deterministic per
         S4-STABLE) to keep stages independent. Envelope gates: any
         SHORT_MIN line aborts loudly (S3/S4 input contract). ---- */
      {
        std::fprintf(out, "S5FRAME f=%ld\n", f);
        for (int c = 0; c < channels; c++) {
          int cnt = s1cnt[c];
          int per = s1perGroup[c];
          int maxSfb = (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup;
          int bt = (int)st[c].bs.lastWindowSequence;
          std::fprintf(out, "S5GEO ch=%d cnt=%d per=%d max=%d seq=%d\n", c,
                       cnt, per, maxSfb, bt);
          std::fprintf(out, "S5OFF");
          for (int i = 0; i <= cnt; i++)
            std::fprintf(out, " %d", (int)s1poc[c].sfbOffsets[i]);
          std::fprintf(out, "\n");
        }
        /* Element S4 inv=2 run (both channels, mirrors EstimateScaleFactors
           element loop for stereo coupling-free determinism). */
        {
          PSY_OUT_CHANNEL s5pocE[2];
          QC_OUT_CHANNEL s5qocE[2];
          FIXP_DBL s5specE[2][FRAME_LEN];
          std::memset(s5pocE, 0, sizeof(s5pocE));
          std::memset(s5qocE, 0, sizeof(s5qocE));
          for (int c = 0; c < channels; c++) {
            int cnt = s1cnt[c];
            int per = s1perGroup[c];
            int maxSfb = (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup;
            for (int i = 0; i < FRAME_LEN; i++) s5specE[c][i] = s1spec[c][i];
            for (int o = 0; o < cnt; o += per)
              for (int b = maxSfb; b < per; b++)
                for (int line = s1poc[c].sfbOffsets[o + b];
                     line < s1poc[c].sfbOffsets[o + b + 1]; line++)
                  s5specE[c][line] = 0;
            s5pocE[c].sfbCnt = cnt;
            s5pocE[c].sfbPerGroup = per;
            s5pocE[c].maxSfbPerGroup = maxSfb;
            std::memcpy(s5pocE[c].sfbOffsets, s1poc[c].sfbOffsets,
                        (cnt + 1) * sizeof(INT));
            s5pocE[c].mdctSpectrum = s5specE[c];
            s5pocE[c].sfbEnergyLdData = s1enLd[c];
            s5pocE[c].sfbThresholdLdData = s2qoc[c].sfbThresholdLdData;
            std::memcpy(s5qocE[c].mdctSpectrum, s5specE[c],
                        sizeof(s5specE[c]));
            std::memcpy(s5qocE[c].sfbEnergyLdData, s1enLd[c],
                        cnt * sizeof(FIXP_DBL));
            std::memcpy(s5qocE[c].sfbThresholdLdData,
                        s2qoc[c].sfbThresholdLdData, cnt * sizeof(FIXP_DBL));
            std::memcpy(s5qocE[c].sfbFormFactorLdData,
                        s1qoc[c].sfbFormFactorLdData, cnt * sizeof(FIXP_DBL));
          }
          PSY_OUT_CHANNEL *ppE[2] = {&s5pocE[0], &s5pocE[1]};
          QC_OUT_CHANNEL *qpE[2] = {&s5qocE[0], &s5qocE[1]};
          FDKaacEnc_EstimateScaleFactors(ppE, qpE, 2, 0, channels);
          for (int c = 0; c < channels; c++)
            std::memcpy(s5specE[c], s5qocE[c].mdctSpectrum,
                        sizeof(s5specE[c]));
          /* S6 saves: first-pass S4 outputs (gain/quantSpec/scf/mutated
             spectrum) for the gain-loop iteration 0. */
          for (int c = 0; c < channels; c++) {
            s6gain[c] = s5qocE[c].globalGain;
            std::memcpy(s6qspec[c], s5qocE[c].quantSpec, sizeof(s6qspec[c]));
            std::memcpy(s6scf[c], s5qocE[c].scf, sizeof(s6scf[c]));
            std::memcpy(s6mdct[c], s5qocE[c].mdctSpectrum, sizeof(s6mdct[c]));
          }
          /* Per-channel QMAX (inline replica of static calcMaxValueInSfb)
             + real dynBitCount + dumps. */
          for (int c = 0; c < channels; c++) {
            int cnt = s1cnt[c];
            int per = s1perGroup[c];
            int maxSfb = (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup;
            int bt = (int)st[c].bs.lastWindowSequence;
            for (int i = 0; i < FRAME_LEN; i++) {
              if ((int)s5qocE[c].quantSpec[i] == -32768) {
                std::fprintf(stderr,
                             "S5 ENVELOPE VIOLATION (q==SHORT_MIN) f=%ld ch=%d\n",
                             f, c);
                return 1;
              }
            }
            UINT qmax[MAX_GROUPED_SFB];
            for (int i = 0; i < MAX_GROUPED_SFB; i++) qmax[i] = 0;
            for (int o = 0; o < cnt; o += per)
              for (int b = 0; b < maxSfb; b++) {
                INT m = 0;
                for (int line = s1poc[c].sfbOffsets[o + b];
                     line < s1poc[c].sfbOffsets[o + b + 1]; line++) {
                  INT t = s5qocE[c].quantSpec[line];
                  if (t < 0) t = -t;
                  if (t > m) m = t;
                }
                qmax[o + b] = (UINT)m;
              }
            std::fprintf(out, "S5QMAX ch=%d", c);
            for (int i = 0; i < cnt; i++)
              std::fprintf(out, " %u", qmax[i]);
            std::fprintf(out, "\n");
            std::fprintf(out, "S5ISB ch=%d", c);
            for (int i = 0; i < cnt; i++)
              std::fprintf(out, " %d", (int)s1poc[c].isBook[i]);
            std::fprintf(out, "\n");
            std::fprintf(out, "S5ISS ch=%d", c);
            for (int i = 0; i < cnt; i++)
              std::fprintf(out, " %d", (int)s1poc[c].isScale[i]);
            std::fprintf(out, "\n");
            std::fprintf(out, "S5NOISE ch=%d", c);
            for (int i = 0; i < cnt; i++)
              std::fprintf(out, " %d", (int)s1poc[c].noiseNrg[i]);
            std::fprintf(out, "\n");
            INT bitLookUp[MAX_SFB_LONG][CODE_BOOK_ESC_NDX + 1];
            INT mergeGain[MAX_SFB_LONG];
            BITCNTR_STATE bc;
            bc.bitLookUp = (INT *)bitLookUp;
            bc.mergeGainLookUp = mergeGain;
            SECTION_DATA sd;
            std::memset(&sd, 0, sizeof(sd));
            INT dyn = FDKaacEnc_dynBitCount(
                &bc, s5qocE[c].quantSpec, qmax, s5qocE[c].scf, bt, cnt,
                maxSfb, per, s1poc[c].sfbOffsets, &sd, s1poc[c].noiseNrg,
                s1poc[c].isBook, s1poc[c].isScale, 0);
            /* S6 saves: per-channel QMAX + first-pass sectioning/dyn. */
            std::memcpy(s6qmax[c], qmax, sizeof(s6qmax[c]));
            s6sec[c] = sd;
            s6dyn[c] = dyn;
            /* S6 saves: geometry + PNS/IS preconditions for retry passes. */
            s6cnt[c] = cnt;
            s6per[c] = per;
            s6maxSfb0[c] = maxSfb;
            s6seq[c] = bt;
            std::memcpy(s6off[c], s1poc[c].sfbOffsets, sizeof(s6off[c]));
            std::memcpy(s6noise[c], s1poc[c].noiseNrg, sizeof(s6noise[c]));
            std::memcpy(s6isBook[c], s1poc[c].isBook, sizeof(s6isBook[c]));
            std::memcpy(s6isScale[c], s1poc[c].isScale, sizeof(s6isScale[c]));
            std::fprintf(out, "S5SECT ch=%d nsec=%d\n", c,
                         (int)sd.noOfSections);
            for (int i = 0; i < (int)sd.noOfSections; i++)
              std::fprintf(out, "S5SEC ch=%d i=%d book=%d start=%d cnt=%d bits=%d\n",
                           c, i, (int)sd.huffsection[i].codeBook,
                           (int)sd.huffsection[i].sfbStart,
                           (int)sd.huffsection[i].sfbCnt,
                           (int)sd.huffsection[i].sectionBits);
            std::fprintf(out,
                         "S5BITS ch=%d block=%d nsec=%d side=%d huff=%d scf=%d noise=%d first=%d dyn=%d\n",
                         c, (int)sd.blockType, (int)sd.noOfSections,
                         (int)sd.sideInfoBits, (int)sd.huffmanBits,
                         (int)sd.scalefacBits, (int)sd.noiseNrgBits,
                         (int)sd.firstScf, (int)dyn);
          }
        }
        std::fprintf(out, "S5END\n");
      }

      /* ---- S7 stage: dynamic-payload bitstream encoding (bit_cnt.cpp
         encode half + bitenc.cpp section writers) via the REAL
         `FDKaacEnc_codeValues`, `FDKaacEnc_codeScalefactorDelta` and
         `FDKwriteBits` through a real writer bitstream. The file-static
         `encodeSpectralData` (bitenc.cpp:127-147), `encodeScaleFactorData`
         (:292-369) and `encodeSectionData` (:239-281) are not exported, so
         the stage replicates their traversal verbatim; the replica is
         self-checked bit-exactly against the S5 counts (the production
         WRITE_SEC/SCAL/SPEC_ERROR agreement, bitenc.cpp:1049-1122) and
         aborts loudly on any deviation. Inputs are the saved S5 first
         pass (s6qspec/s6scf/s6gain/s6qmax/s6sec + geometry/PNS/IS).
         Envelope gates: SHORT_MIN abort (S3/S4 input contract). ---- */
      {
        std::fprintf(out, "S7FRAME f=%ld\n", f);
        for (int c = 0; c < channels; c++) {
          SECTION_DATA *sd = &s6sec[c];
          UCHAR s7buf[2048];
          HANDLE_FDK_BITSTREAM s7bs = NULL;
          /* Spectral payload (encodeSpectralData :134-145). */
          std::memset(s7buf, 0, sizeof(s7buf));
          s7bs = FDKcreateBitStream(s7buf, sizeof(s7buf), BS_WRITER);
          for (int i = 0; i < (int)sd->noOfSections; i++) {
            if (sd->huffsection[i].codeBook == CODE_BOOK_PNS_NO) continue;
            int tmp = sd->huffsection[i].sfbStart + sd->huffsection[i].sfbCnt;
            for (int sfb = sd->huffsection[i].sfbStart; sfb < tmp; sfb++) {
              FDKaacEnc_codeValues(s6qspec[c] + s6off[c][sfb],
                                   s6off[c][sfb + 1] - s6off[c][sfb],
                                   sd->huffsection[i].codeBook, s7bs);
            }
          }
          FDKsyncCache(s7bs);
          {
            unsigned specBits = FDKgetValidBits(s7bs);
            if ((int)specBits != sd->huffmanBits) {
              std::fprintf(stderr,
                           "S7 SPEC MISMATCH f=%ld ch=%d emit=%u count=%d\n",
                           f, c, specBits, (int)sd->huffmanBits);
              return 1;
            }
            std::fprintf(out, "S7SPEC ch=%d bits=%u hex=", c, specBits);
            for (unsigned k = 0; k < (specBits + 7) / 8; k++)
              std::fprintf(out, "%02x", s7buf[k]);
            std::fprintf(out, "\n");
          }
          /* Scalefactor payload (encodeScaleFactorData :307-363). */
          std::memset(s7buf, 0, sizeof(s7buf));
          FDKresetBitbuffer(s7bs, BS_WRITER);
          FDKinitBitStream(s7bs, s7buf, sizeof(s7buf), 0, BS_WRITER);
          {
            INT lastValScf = s6scf[c][sd->firstScf];
            INT lastValPns = s6gain[c] - s6scf[c][sd->firstScf] + 100 -
                             4 * (-15) - 90;
            INT lastValIs = 0;
            INT noisePCMFlag = TRUE;
            for (int i = 0; i < (int)sd->noOfSections; i++) {
              if (sd->huffsection[i].codeBook == CODE_BOOK_ZERO_NO) continue;
              if (sd->huffsection[i].codeBook ==
                      CODE_BOOK_IS_OUT_OF_PHASE_NO ||
                  sd->huffsection[i].codeBook == CODE_BOOK_IS_IN_PHASE_NO) {
                int sfbStart = sd->huffsection[i].sfbStart;
                int tmp = sfbStart + sd->huffsection[i].sfbCnt;
                for (int j = sfbStart; j < tmp; j++) {
                  INT deltaIs = s6isScale[c][j] - lastValIs;
                  lastValIs = s6isScale[c][j];
                  if (FDKaacEnc_codeScalefactorDelta(deltaIs, s7bs)) {
                    std::fprintf(stderr, "S7 IS delta OOB f=%ld ch=%d\n", f,
                                 c);
                    return 1;
                  }
                }
              } else if (sd->huffsection[i].codeBook == CODE_BOOK_PNS_NO) {
                int sfbStart = sd->huffsection[i].sfbStart;
                int tmp = sfbStart + sd->huffsection[i].sfbCnt;
                for (int j = sfbStart; j < tmp; j++) {
                  INT deltaPns = s6noise[c][j] - lastValPns;
                  lastValPns = s6noise[c][j];
                  if (noisePCMFlag) {
                    FDKwriteBits(s7bs, deltaPns + (1 << (PNS_PCM_BITS - 1)),
                                 PNS_PCM_BITS);
                    noisePCMFlag = FALSE;
                  } else {
                    if (FDKaacEnc_codeScalefactorDelta(deltaPns, s7bs)) {
                      std::fprintf(stderr, "S7 PNS delta OOB f=%ld ch=%d\n",
                                   f, c);
                      return 1;
                    }
                  }
                }
              } else {
                int tmp = sd->huffsection[i].sfbStart +
                          sd->huffsection[i].sfbCnt;
                for (int j = sd->huffsection[i].sfbStart; j < tmp; j++) {
                  INT deltaScf;
                  if (s6qmax[c][j] == 0)
                    deltaScf = 0;
                  else {
                    deltaScf = -(s6scf[c][j] - lastValScf);
                    lastValScf = s6scf[c][j];
                  }
                  if (FDKaacEnc_codeScalefactorDelta(deltaScf, s7bs)) {
                    std::fprintf(stderr, "S7 SCF delta OOB f=%ld ch=%d\n", f,
                                 c);
                    return 1;
                  }
                }
              }
            }
          }
          FDKsyncCache(s7bs);
          {
            unsigned scfBits = FDKgetValidBits(s7bs);
            if ((int)scfBits != sd->scalefacBits + sd->noiseNrgBits) {
              std::fprintf(stderr,
                           "S7 SCF MISMATCH f=%ld ch=%d emit=%u count=%d+%d\n",
                           f, c, scfBits, (int)sd->scalefacBits,
                           (int)sd->noiseNrgBits);
              return 1;
            }
            std::fprintf(out, "S7SCF ch=%d bits=%u hex=", c, scfBits);
            for (unsigned k = 0; k < (scfBits + 7) / 8; k++)
              std::fprintf(out, "%02x", s7buf[k]);
            std::fprintf(out, "\n");
          }
          /* Section headers (encodeSectionData :242-277). */
          std::memset(s7buf, 0, sizeof(s7buf));
          FDKresetBitbuffer(s7bs, BS_WRITER);
          FDKinitBitStream(s7bs, s7buf, sizeof(s7buf), 0, BS_WRITER);
          {
            int esc = 0, escBits = 0;
            switch (sd->blockType) {
              case LONG_WINDOW:
              case START_WINDOW:
              case STOP_WINDOW:
              default:
                esc = SECT_ESC_VAL_LONG;
                escBits = SECT_BITS_LONG;
                break;
              case SHORT_WINDOW:
                esc = SECT_ESC_VAL_SHORT;
                escBits = SECT_BITS_SHORT;
                break;
            }
            for (int i = 0; i < (int)sd->noOfSections; i++) {
              int book = sd->huffsection[i].codeBook;
              int len = sd->huffsection[i].sfbCnt;
              FDKwriteBits(s7bs, book, 4);
              while (len >= esc) {
                FDKwriteBits(s7bs, esc, escBits);
                len -= esc;
              }
              FDKwriteBits(s7bs, len, escBits);
            }
          }
          FDKsyncCache(s7bs);
          {
            unsigned secBits = FDKgetValidBits(s7bs);
            if ((int)secBits != sd->sideInfoBits) {
              std::fprintf(stderr,
                           "S7 SEC MISMATCH f=%ld ch=%d emit=%u count=%d\n",
                           f, c, secBits, (int)sd->sideInfoBits);
              return 1;
            }
            std::fprintf(out, "S7SECH ch=%d bits=%u hex=", c, secBits);
            for (unsigned k = 0; k < (secBits + 7) / 8; k++)
              std::fprintf(out, "%02x", s7buf[k]);
            std::fprintf(out, "\n");
          }
          FDKdeleteBitStream(s7bs);
        }
        std::fprintf(out, "S7END\n");
      }
      }

      /* ---- S6 stage: QC gain loop + rate-control closure (qc_main.cpp)
         via real exported calls wherever the reference exposes them, with
         documented inline replicas of the file-static helpers
         (`BitResRedistribution` :738, `prepareBitDistribution` :615 +
         `distributeElementDynBits` :504, `calcMaxValueInSfb` :1219,
         `reduceBitConsumption` :1119, `crashRecovery` :1395, the
         consumed-bits accumulators :677-736, the QCMain exit matrix
         :1030-1107 and the VBR `FinalizeBitConsumption` path :1283-1383).
         Real calls: `AdjustBitrate`, `DistributeBits`, `QuantizeSpectrum`,
         `dynBitCount`, `ChannelElementWrite` (static + crash recounts),
         `writeExtensionData` (fill sizing), `updateFillBits`,
         `updateBitres`. Persistent state (`s6qc` bitResTot/paddingRest,
         `s1ats` peLast/dynBitsLast/peCorrection/peMin/peMax/chaos) evolves
         exactly like the encoder across frames. Iteration 0 reuses the
         saved S4/S5 first pass; retries re-quantize the saved mutated
         spectrum with bumped gains. Envelope gates: SHORT_MIN abort,
         QUANT_ERROR abort, 25-pass runaway abort; the BITRES_TOO_LOW
         branch is unreachable at VBR-5 (granted>0, bitResTot>=0 pinned)
         and aborts loudly if ever taken. ---- */
      {
        INT avgTotal = 0;
        if (FDKaacEnc_AdjustBitrate(&s6qc, &s2cm, &avgTotal, totalBr,
                                    sample_rate, FRAME_LEN) != AAC_ENC_OK) {
          std::fprintf(stderr, "S6 AdjustBitrate failed f=%ld\n", f);
          return 1;
        }
        const INT globHdr = 56; /* transport-probed ADTS constant */
        s6qc.globHdrBits = globHdr;
        /* BitResRedistribution replica (VBR passes maxBitsPerFrame). */
        if (s6qc.bitResTot < 0) {
          std::fprintf(stderr, "S6 BITRES_TOO_LOW f=%ld\n", f);
          return 1;
        }
        if (s6qc.bitResTot > s6qc.bitResTotMax) {
          std::fprintf(stderr, "S6 BITRES_TOO_HIGH f=%ld\n", f);
          return 1;
        }
        {
          INT totalBits = 0, totalBits_max = 0;
          INT totalBitreservoir =
              fixMin(s6qc.bitResTot,
                     (s6qc.maxBitsPerFrame - s6qc.maxBitsPerFrame));
          INT totalBitreservoirMax = fixMin(
              s6qc.bitResTotMax, (s6qc.maxBitsPerFrame - s6qc.maxBitsPerFrame));
          s6elBits.bitResLevelEl = fMultI(s6elBits.relativeBitsEl,
                                          totalBitreservoir);
          totalBits += s6elBits.bitResLevelEl;
          s6elBits.maxBitResBitsEl = fMultI(s6elBits.relativeBitsEl,
                                            totalBitreservoirMax);
          totalBits_max += s6elBits.maxBitResBitsEl;
          {
            int deltaBits = fixMax(totalBitreservoir - totalBits,
                                   -s6elBits.bitResLevelEl);
            s6elBits.bitResLevelEl += deltaBits;
            totalBits += deltaBits;
            deltaBits = fixMax(totalBitreservoirMax - totalBits_max,
                               -s6elBits.maxBitResBitsEl);
            s6elBits.maxBitResBitsEl += deltaBits;
            totalBits_max += deltaBits;
          }
        }
        /* prepareBitDistribution replica (single element; general formula).
           VBR passes maxBitsPerFrame (not the frame avgTotal) as the
           avgTotalBits argument (QCMain :836-839, isCBRAdjustment==0), so
           both the grant base and totalAvail derive from maxBits.
           globalExtBits = EL_ID_BITS (3, aacenc.cpp:913-916), elementExt = 0
           (no ext payload in fixtures). */
        INT grantedDynBits =
            (fixMin(s6qc.maxBitsPerFrame, s6qc.maxBitsPerFrame) - globHdr) &
            ~7;
        grantedDynBits -= (3 + s6staticSaved + 0);
        INT maxDynBits =
            (s6qc.maxBitsPerFrame & ~7) - (3 + s6staticSaved + 0);
        if ((grantedDynBits + s6qc.bitResTot) < 0) {
          std::fprintf(stderr, "S6 prepare BITRES_TOO_LOW f=%ld\n", f);
          return 1;
        }
        /* distributeElementDynBits replica (1 element; loop ported). */
        INT elGranted =
            fixMax(0, fMultI(s6elBits.relativeBitsEl, grantedDynBits));
        {
          INT totalDist = elGranted;
          if (grantedDynBits != totalDist) {
            /* Single element is both max and min: compensation lands here. */
            elGranted += grantedDynBits - totalDist;
          }
        }
        INT totalAvail = s6qc.maxBitsPerFrame + s6elBits.bitResLevelEl;
        totalAvail = fixMin(s6qc.maxBitsPerFrame, totalAvail);
        /* Real DistributeBits (mutates s1ats grant state). */
        PE_DATA s6peData;
        std::memset(&s6peData, 0, sizeof(s6peData));
        s6peData.pe = s6peSaved;
        PSY_OUT_CHANNEL s6dpsy[2];
        std::memset(&s6dpsy, 0, sizeof(s6dpsy));
        for (int c = 0; c < channels; c++)
          s6dpsy[c].lastWindowSequence = s6seq[c];
        PSY_OUT_CHANNEL *s6dpsyPtr[2] = {&s6dpsy[0], &s6dpsy[1]};
        INT grantedPe = 0, grantedPeCorr = 0;
        /* S6 grant-state pre-save (dumped as S6GSTATE after S6GRANT, so
           Rust can forge the exact pre-frame ATS grant state). */
        INT gstatePre[6] = {(int)s1ats.peLast, (int)s1ats.dynBitsLast,
                            (int)s1ats.peCorrectionFactor_m,
                            (int)s1ats.peCorrectionFactor_e, (int)s1ats.peMin,
                            (int)s1ats.peMax};
        FDKaacEnc_DistributeBits(&s2hAdjThr, &s1ats, s6dpsyPtr, &s6peData,
                                 &grantedPe, &grantedPeCorr, channels, 1,
                                 elGranted, s6elBits.bitResLevelEl,
                                 s6elBits.maxBitResBitsEl, s6qc.maxBitFac,
                                 s6qc.bitResMode);
        /* Gain loop (QCMain :901-1107, nSubFrames=1, single element). */
        INT iterations = 0;
        INT constraintsFulfilled = 1;
        INT chConstraints[2] = {1, 1};
        INT calculateQuant[2] = {0, 0};
        INT decrease = -1, quantizationDone = 0;
        INT wGain[2] = {s6gain[0], s6gain[1]};
        SHORT wQ[2][FRAME_LEN];
        UINT wMax[2][MAX_GROUPED_SFB];
        SECTION_DATA wSec[2];
        INT wDyn[2] = {s6dyn[0], s6dyn[1]};
        INT wMaxSfb[2] = {s6maxSfb0[0], s6maxSfb0[1]};
        for (int c = 0; c < channels; c++) {
          std::memcpy(wQ[c], s6qspec[c], sizeof(wQ[c]));
          std::memcpy(wMax[c], s6qmax[c], sizeof(wMax[c]));
          wSec[c] = s6sec[c];
        }
        INT elDynBitsUsed = s6dyn[0] + ((channels == 2) ? s6dyn[1] : 0);
        INT elStaticBitsUsed = s6staticSaved;
        INT frameGranted = grantedDynBits, frameMaxDyn = maxDynBits;
        INT crashDrops = -1, nPasses = 0;
        std::fprintf(out, "S6FRAME f=%ld\n", f);
        std::fprintf(out, "S6BITR avgTotal=%d globHdr=%d padRest=%d\n",
                     (int)avgTotal, (int)globHdr,
                     (int)s6qc.padding.paddingRest);
        std::fprintf(out,
                     "S6GRANT granted=%d maxdyn=%d avail=%d elgrant=%d gpe=%d "
                     "gpecorr=%d reslevel=%d resmax=%d\n",
                     (int)grantedDynBits, (int)maxDynBits, (int)totalAvail,
                     (int)elGranted, (int)grantedPe, (int)grantedPeCorr,
                     (int)s6elBits.bitResLevelEl,
                     (int)s6elBits.maxBitResBitsEl);
        std::fprintf(out, "S6GSTATE %d %d %d %d %d %d\n", (int)gstatePre[0],
                     (int)gstatePre[1], (int)gstatePre[2], (int)gstatePre[3],
                     (int)gstatePre[4], (int)gstatePre[5]);
        INT outerGuard = 0;
        do {
          quantizationDone = 0;
          do { /* element-cap loop */
            do { /* MAX_QUANT loop */
              if (!constraintsFulfilled) {
                /* reduceBitConsumption replica (:1119-1166). */
                if (iterations < 5) {
                  for (int ch = 0; ch < channels; ch++)
                    if (!chConstraints[ch]) {
                      wGain[ch] += (decrease ? 1 : -1);
                      calculateQuant[ch] = 1;
                    }
                } else if (iterations == 5) {
                  if (elDynBitsUsed == 0) {
                    std::fprintf(stderr, "S6 QUANT_ERROR (empty) f=%ld\n", f);
                    return 1;
                  }
                  INT bitsToSave = fixMax(
                      (elDynBitsUsed + 8) -
                          (s6elBits.bitResLevelEl + elGranted),
                      (elDynBitsUsed + elStaticBitsUsed + 8) -
                          (s6elBits.maxBitsEl));
                  if (bitsToSave > 0) {
                    /* crashRecovery replica (:1395-1498). */
                    INT bitsPerScf[2][MAX_GROUPED_SFB];
                    INT sectionToScf[2][MAX_GROUPED_SFB];
                    for (int ch = 0; ch < channels; ch++) {
                      for (int sect = 0; sect < wSec[ch].noOfSections;
                           sect++) {
                        INT cb = wSec[ch].huffsection[sect].codeBook;
                        for (int sfb = wSec[ch].huffsection[sect].sfbStart;
                             sfb < wSec[ch].huffsection[sect].sfbStart +
                                       wSec[ch].huffsection[sect].sfbCnt;
                             sfb++) {
                          bitsPerScf[ch][sfb] = 0;
                          if (cb != CODE_BOOK_PNS_NO) {
                            INT sl = s6off[ch][sfb];
                            bitsPerScf[ch][sfb] = FDKaacEnc_countValues(
                                &wQ[ch][sl], s6off[ch][sfb + 1] - sl, cb);
                          }
                          sectionToScf[ch][sfb] = sect;
                        }
                      }
                    }
                    INT savedBits = 0, sfbStop = 0;
                    int sfb;
                    for (sfb = wMaxSfb[0] - 1; sfb >= 0; sfb--) {
                      for (int grp = 0; grp < s6cnt[0]; grp += s6per[0]) {
                        for (int ch = 0; ch < channels; ch++) {
                          int sect = sectionToScf[ch][grp + sfb];
                          wSec[ch].huffsection[sect].sfbCnt--;
                          savedBits += bitsPerScf[ch][grp + sfb];
                          if (wSec[ch].huffsection[sect].sfbCnt == 0) {
                            savedBits +=
                                (s6seq[ch] != SHORT_WINDOW)
                                    ? FDKaacEnc_sideInfoTabLong[0]
                                    : FDKaacEnc_sideInfoTabShort[0];
                          }
                        }
                      }
                      if (savedBits >= bitsToSave) break;
                    }
                    if (sfb == -1) sfb = 0;
                    sfbStop = sfb;
                    for (int ch = 0; ch < channels; ch++) {
                      wMaxSfb[ch] = sfbStop;
                    }
                    /* Static recount with cleared tools/TNS on wipeout. */
                    {
                      ELEMENT_INFO elInfo;
                      std::memset(&elInfo, 0, sizeof(elInfo));
                      elInfo.nChannelsInEl = channels;
                      elInfo.elType =
                          (channels == 2) ? ID_CPE : ID_SCE;
                      PSY_OUT_CHANNEL rpsy[2];
                      std::memset(&rpsy, 0, sizeof(rpsy));
                      for (int ch = 0; ch < channels; ch++) {
                        rpsy[ch] = s6dpsy[ch];
                        rpsy[ch].tnsInfo = st[ch].tnsInfo;
                        rpsy[ch].lastWindowSequence = s6seq[ch];
                        if (sfbStop == 0)
                          std::memset(&rpsy[ch].tnsInfo, 0,
                                      sizeof(rpsy[ch].tnsInfo));
                      }
                      PSY_OUT_CHANNEL *rpsyPtr[2] = {&rpsy[0], &rpsy[1]};
                      PSY_OUT_ELEMENT relem;
                      std::memset(&relem, 0, sizeof(relem));
                      relem.psyOutChannel[0] = &rpsy[0];
                      relem.psyOutChannel[1] = &rpsy[1];
                      relem.commonWindow = 1;
                      relem.toolsInfo = s6toolsSaved;
                      if (sfbStop == 0)
                        std::memset(&relem.toolsInfo, 0,
                                    sizeof(relem.toolsInfo));
                      INT statBitsNew = 0;
                      if (FDKaacEnc_ChannelElementWrite(
                              NULL, &elInfo, NULL, &relem, rpsyPtr, 0,
                              AOT_AAC_LC, -1, &statBitsNew, 0) != AAC_ENC_OK) {
                        std::fprintf(stderr, "S6 crash recount failed f=%ld\n",
                                     f);
                        return 1;
                      }
                      savedBits = elStaticBitsUsed - statBitsNew;
                      elStaticBitsUsed -= savedBits;
                      elGranted += savedBits;
                      frameGranted += savedBits;
                      frameMaxDyn += savedBits;
                    }
                    crashDrops = sfbStop;
                    for (int ch = 0; ch < channels; ch++)
                      calculateQuant[ch] = 1;
                  } else {
                    for (int ch = 0; ch < channels; ch++) wGain[ch] += 1;
                    for (int ch = 0; ch < channels; ch++)
                      calculateQuant[ch] = 1;
                  }
                } else {
                  std::fprintf(stderr, "S6 QUANT_ERROR (iter) f=%ld\n", f);
                  return 1;
                }
                iterations++;
              }
              constraintsFulfilled = 1;
              for (int ch = 0; ch < channels; ch++) {
                chConstraints[ch] = 1;
                if (calculateQuant[ch]) {
                  calculateQuant[ch] = 0;
                  FDKaacEnc_QuantizeSpectrum(
                      s6cnt[ch], wMaxSfb[ch], s6per[ch], s6off[ch], s6mdct[ch],
                      wGain[ch], s6scf[ch], wQ[ch], 0);
                  for (int i = 0; i < FRAME_LEN; i++)
                    if ((int)wQ[ch][i] == -32768) {
                      std::fprintf(stderr,
                                   "S6 ENVELOPE VIOLATION f=%ld ch=%d\n", f,
                                   ch);
                      return 1;
                    }
                  /* calcMaxValueInSfb replica (:1219-1240). */
                  INT maxAll = 0;
                  for (int o = 0; o < s6cnt[ch]; o += s6per[ch])
                    for (int b = 0; b < wMaxSfb[ch]; b++) {
                      INT m = 0;
                      for (int line = s6off[ch][o + b];
                           line < s6off[ch][o + b + 1]; line++) {
                        INT t = wQ[ch][line];
                        if (t < 0) t = -t;
                        if (t > m) m = t;
                      }
                      wMax[ch][o + b] = (UINT)m;
                      if (m > maxAll) maxAll = m;
                    }
                  if (maxAll > 8191) {
                    chConstraints[ch] = 0;
                    constraintsFulfilled = 0;
                    decrease = 1;
                  }
                }
              }
            } while (!constraintsFulfilled);
            elDynBitsUsed = 0;
            for (int ch = 0; ch < channels; ch++) {
              INT bitLookUp[MAX_SFB_LONG][CODE_BOOK_ESC_NDX + 1];
              INT mergeGain[MAX_SFB_LONG];
              BITCNTR_STATE bc;
              bc.bitLookUp = (INT *)bitLookUp;
              bc.mergeGainLookUp = mergeGain;
              wDyn[ch] = FDKaacEnc_dynBitCount(
                  &bc, wQ[ch], wMax[ch], s6scf[ch], s6seq[ch], s6cnt[ch],
                  wMaxSfb[ch], s6per[ch], s6off[ch], &wSec[ch], s6noise[ch],
                  s6isBook[ch], s6isScale[ch], 0);
              elDynBitsUsed += wDyn[ch];
            }
            if (s1ats.dynBitsLast == -1) s1ats.dynBitsLast = elDynBitsUsed;
            if (elDynBitsUsed >
                ((channels * 6144) - elStaticBitsUsed - 0)) {
              constraintsFulfilled = 0;
            }
          } while (!constraintsFulfilled);
          INT usedDynFrame = elDynBitsUsed; /* updateUsedDynBits replica */
          INT dataBits = elDynBitsUsed + elStaticBitsUsed + 0 + 3;
          INT sumBits =
              (8 - dataBits % 8) % 8 + dataBits + globHdr;
          {
            INT maxqDump[2] = {0, 0};
            for (int ch = 0; ch < channels; ch++) {
              INT m = 0;
              for (int i = 0; i < s6cnt[ch]; i++)
                if ((INT)wMax[ch][i] > m) m = (INT)wMax[ch][i];
              maxqDump[ch] = m;
            }
            std::fprintf(out,
                         "S6IT pass=%d iter=%d dec=%d dyn0=%d dyn1=%d gain0=%d "
                         "gain1=%d maxq0=%d maxq1=%d eldyn=%d elstat=%d "
                         "crash=%d\n",
                         (int)nPasses, (int)iterations, (int)decrease,
                         (int)wDyn[0], (int)wDyn[1], (int)wGain[0],
                         (int)wGain[1], (int)maxqDump[0], (int)maxqDump[1],
                         (int)elDynBitsUsed, (int)elStaticBitsUsed,
                         (int)crashDrops);
            nPasses++;
          }
          /* QCMain exit matrix replica (:1030-1107). */
          {
            INT sumDyn = usedDynFrame;
            if (sumDyn != -1) {
              if (((sumBits < totalAvail) || sumDyn == 0) &&
                  (decrease == 1)) {
                quantizationDone = 1;
              }
              if (sumBits > totalAvail && (decrease == 0)) {
                quantizationDone = 0;
              }
            } else {
              quantizationDone = 0;
            }
            INT emergency = (iterations < 5) ? 0 : 1;
            INT overshoot = (usedDynFrame > maxDynBits) ? 1 : 0;
            if (quantizationDone == 0 || overshoot) {
              if ((sumDyn >= 0) || (sumDyn == 0)) quantizationDone = 1;
              if (emergency && (sumBits < totalAvail)) quantizationDone = 1;
              if ((sumBits > totalAvail)) quantizationDone = 0;
              if ((sumBits < totalAvail)) {
                decrease = 0;
              } else {
                decrease = 1;
              }
              if (overshoot) {
                quantizationDone = 0;
                decrease = 1;
              }
              constraintsFulfilled = 0;
              chConstraints[0] = 0;
              chConstraints[1] = 0;
            }
          }
          if (++outerGuard > 25) {
            std::fprintf(stderr, "S6 loop runaway f=%ld\n", f);
            return 1;
          }
        } while (!quantizationDone);
        /* Real updateFillBits on shells. */
        QC_OUT s6qo;
        std::memset(&s6qo, 0, sizeof(s6qo));
        s6qo.grantedDynBits = frameGranted;
        s6qo.maxDynBits = frameMaxDyn;
        s6qo.staticBits = elStaticBitsUsed;
        s6qo.elementExtBits = 0;
        s6qo.globalExtBits = 3;
        s6qo.usedDynBits = elDynBitsUsed;
        ELEMENT_BITS *s6elbArr[((8))] = {&s6elBits, 0, 0, 0, 0, 0, 0, 0};
        QC_OUT *s6qoArr[(1)] = {&s6qo};
        if (FDKaacEnc_updateFillBits(&s2cm, &s6qc, s6elbArr, s6qoArr) !=
            AAC_ENC_OK) {
          std::fprintf(stderr, "S6 updateFillBits failed f=%ld\n", f);
          return 1;
        }
        /* FinalizeBitConsumption VBR-path replica (:1283-1383; CBR block
           skipped) with a REAL writeExtensionData(NULL) fill sizing. */
        s6qo.totalBits = s6qo.staticBits + s6qo.usedDynBits +
                         s6qo.totFillBits + s6qo.elementExtBits +
                         s6qo.globalExtBits;
        s6qc.globHdrBits = 56; /* GetStaticBits replica (probed constant) */
        {
          INT totFill = s6qo.totFillBits;
          QC_OUT_EXTENSION fillExt;
          std::memset(&fillExt, 0, sizeof(fillExt));
          fillExt.type = EXT_FILL_DATA;
          fillExt.nPayloadBits = totFill;
          s6qo.totFillBits = FDKaacEnc_writeExtensionData(
              NULL, &fillExt, 0, 0, 0, AOT_AAC_LC, -1);
          INT alignBits =
              7 - (s6qo.staticBits + s6qo.usedDynBits +
                   s6qo.elementExtBits + s6qo.totFillBits +
                   s6qo.globalExtBits - 1) %
                      8;
          if (((alignBits + s6qo.totFillBits - totFill) == 8) &&
              (s6qo.totFillBits > 8))
            s6qo.totFillBits -= 8;
          s6qo.totalBits = s6qo.staticBits + s6qo.usedDynBits +
                           s6qo.totFillBits + alignBits +
                           s6qo.elementExtBits + s6qo.globalExtBits;
          if ((s6qo.totalBits > s6qc.maxBitsPerFrame) ||
              (s6qo.totalBits < s6qc.minBitsPerFrame)) {
            std::fprintf(stderr, "S6 QUANT_ERROR (final range) f=%ld\n", f);
            return 1;
          }
          s6qo.alignBits = alignBits;
        }
        FDKaacEnc_updateBitres(&s2cm, &s6qc, s6qoArr);
        std::fprintf(out,
                     "S6FINAL static=%d useddyn=%d totfill=%d align=%d "
                     "total=%d granted=%d bitres=%d crash=%d npass=%d\n",
                     (int)s6qo.staticBits, (int)s6qo.usedDynBits,
                     (int)s6qo.totFillBits, (int)s6qo.alignBits,
                     (int)s6qo.totalBits, (int)s6qo.grantedDynBits,
                     (int)s6qc.bitResTot, (int)crashDrops, (int)nPasses);
        /* S6 grant-state post-dump (evolution proof: PRE(f+1)==POST(f)). */
        std::fprintf(out, "S6GPOST %d %d %d %d %d %d\n",
                     (int)s1ats.peLast, (int)s1ats.dynBitsLast,
                     (int)s1ats.peCorrectionFactor_m,
                     (int)s1ats.peCorrectionFactor_e, (int)s1ats.peMin,
                     (int)s1ats.peMax);
        std::fprintf(out, "S6END\n");
        /* S8 saves: fill/finalize totals for the raw-block tail. */
        s8totfill = s6qo.totFillBits;
        s8align = s6qo.alignBits;
        s8total = s6qo.totalBits;
        s8static = s6qo.staticBits;
        s8useddyn = s6qo.usedDynBits;
      }

      /* ---- S8 stage: channel-element assembly + raw-data-block output
         (bitenc.cpp write-mode ChannelElementWrite :918-1178 +
         WriteBitstream tail :1182-1322, stripped single-element path) via
         the REAL leaf calls plus verbatim traversals on the saved S5
         first pass (s6qspec/s6scf/s6gain/s6qmax/s6sec + geometry/PNS/IS,
         which is the final state on the single-pass corpus) and the saved
         S1 side info (windowShape/groupingMask/mdctScale/TNS/MS) with the
         saved S6 fill/align/total. Static scaffolding (ICS/MS/gain/pulse/
         TNS) uses direct FDKwriteBits (values are fixed syntax, counts
         proven by S1); section/scale/spectral use the REAL codeValues/
         codeScalefactorDelta + S7-verified traversals, self-checked
         bit-exactly against the S5 counts (the production WRITE_SEC/SCAL/
         SPEC_ERROR agreement). Envelope gates: SHORT_MIN abort. ---- */
      {
        std::fprintf(out, "S8FRAME f=%ld\n", f);
        for (int c = 0; c < channels; c++) {
          std::fprintf(out, "S8SIDE ch=%d wshape=%d gmask=%d mdct=%d\n", c,
                       (int)s8wshape[c], (int)s8gmask[c], (int)s8mdct[c]);
        }
        {
          UCHAR s8buf[8192];
          std::memset(s8buf, 0, sizeof(s8buf));
          HANDLE_FDK_BITSTREAM s8bs =
              FDKcreateBitStream(s8buf, sizeof(s8buf), BS_WRITER);
          if (s8bs == NULL) {
            std::fprintf(stderr, "S8 bitstream alloc failed f=%ld\n", f);
            return 1;
          }
          /* Element header: EL_ID (3) + instance tag (4). */
          int elType = (channels == 2) ? ID_CPE : ID_SCE;
          FDKwriteBits(s8bs, elType, 3);
          unsigned elemStart = FDKgetValidBits(s8bs);
          (void)elemStart;
          if (channels == 2) {
            FDKwriteBits(s8bs, 0, 4); /* instanceTag */
            FDKwriteBits(s8bs, 1, 1); /* common_window = 1 */
            /* Shared ICS on ch0 (encodeIcsInfo :180-227). */
            {
              int bt = s6seq[0];
              int wshape = (s8wshape[0] == 2) ? 1 : s8wshape[0];
              FDKwriteBits(s8bs, 0, 1);
              FDKwriteBits(s8bs, bt, 2);
              FDKwriteBits(s8bs, wshape, 1);
              if (bt == 2) {
                FDKwriteBits(s8bs, s6maxSfb0[0], 4);
                FDKwriteBits(s8bs, s8gmask[0], 7);
              } else {
                FDKwriteBits(s8bs, s6maxSfb0[0], 6);
                FDKwriteBits(s8bs, 0, 1);
              }
            }
            /* MS (encodeMSInfo :380-423) on ch0 geometry. */
            {
              int digest = s6toolsSaved.msDigest;
              FDKwriteBits(s8bs, digest, 2);
              if (digest == 1) {
                for (int off = 0; off < s6cnt[0]; off += s6per[0])
                  for (int b = 0; b < s6maxSfb0[0]; b++) {
                    int v = s6toolsSaved.msMask[off + b];
                    FDKwriteBits(s8bs, (v & 1) ? 1 : 0, 1);
                  }
              }
            }
            for (int c = 0; c < channels; c++) {
              SECTION_DATA *sd = &s6sec[c];
              /* Global gain (:158-168). */
              {
                int scfFirst = s6scf[c][sd->firstScf];
                int gv = s6gain[c] - scfFirst + 100 - 4 * (-15 - s8mdct[c]);
                FDKwriteBits(s8bs, gv, 8);
              }
              /* Section headers (:239-281). */
              {
                int esc = 0, escBits = 0;
                if (sd->blockType == 2) {
                  esc = 7;
                  escBits = 3;
                } else {
                  esc = 31;
                  escBits = 5;
                }
                for (int i = 0; i < (int)sd->noOfSections; i++) {
                  int book = sd->huffsection[i].codeBook;
                  int len = sd->huffsection[i].sfbCnt;
                  FDKwriteBits(s8bs, book, 4);
                  while (len >= esc) {
                    FDKwriteBits(s8bs, esc, escBits);
                    len -= esc;
                  }
                  FDKwriteBits(s8bs, len, escBits);
                }
              }
              /* Scalefactors (:292-369). */
              {
                int lastScf = s6scf[c][sd->firstScf];
                int lastPns = s6gain[c] - s6scf[c][sd->firstScf] + 100 -
                              4 * (-15) - 90;
                int lastIs = 0, pcmFlag = 1;
                for (int i = 0; i < (int)sd->noOfSections; i++) {
                  int cb = sd->huffsection[i].codeBook;
                  if (cb == 0) continue;
                  if (cb == 14 || cb == 15) {
                    for (int j = sd->huffsection[i].sfbStart;
                         j < sd->huffsection[i].sfbStart +
                                 sd->huffsection[i].sfbCnt;
                         j++) {
                      int d = s6isScale[c][j] - lastIs;
                      lastIs = s6isScale[c][j];
                      if (FDKaacEnc_codeScalefactorDelta(d, s8bs)) {
                        std::fprintf(stderr, "S8 IS OOB f=%ld ch=%d\n", f, c);
                        return 1;
                      }
                    }
                  } else if (cb == 13) {
                    for (int j = sd->huffsection[i].sfbStart;
                         j < sd->huffsection[i].sfbStart +
                                 sd->huffsection[i].sfbCnt;
                         j++) {
                      int d = s6noise[c][j] - lastPns;
                      lastPns = s6noise[c][j];
                      if (pcmFlag) {
                        FDKwriteBits(s8bs, d + 256, 9);
                        pcmFlag = 0;
                      } else {
                        if (FDKaacEnc_codeScalefactorDelta(d, s8bs)) {
                          std::fprintf(stderr, "S8 PNS OOB f=%ld ch=%d\n", f,
                                       c);
                          return 1;
                        }
                      }
                    }
                  } else {
                    for (int j = sd->huffsection[i].sfbStart;
                         j < sd->huffsection[i].sfbStart +
                                 sd->huffsection[i].sfbCnt;
                         j++) {
                      int d;
                      if (s6qmax[c][j] == 0)
                        d = 0;
                      else {
                        d = -(s6scf[c][j] - lastScf);
                        lastScf = s6scf[c][j];
                      }
                      if (FDKaacEnc_codeScalefactorDelta(d, s8bs)) {
                        std::fprintf(stderr, "S8 SCF OOB f=%ld ch=%d\n", f, c);
                        return 1;
                      }
                    }
                  }
                }
              }
              /* Pulse (1 bit 0), TNS present + data, gain-control (1 bit 0). */
              FDKwriteBits(s8bs, 0, 1);
              {
                /* TNS present (:434-454). */
                int wins = (s6seq[c] == 2) ? 8 : 1;
                int present = 0;
                /* TNS decisions live in s1poc-equivalent st[].tnsInfo;
                   re-read via the saved S1 TNS is not stored, so use the
                   live st[] (same frame, unmutated by S2-S6 on this path). */
                for (int wI = 0; wI < wins; wI++)
                  if (st[c].tnsInfo.numOfFilters[wI] != 0) present = 1;
                FDKwriteBits(s8bs, present, 1);
                if (present) {
                  for (int wI = 0; wI < wins; wI++) {
                    int nf = st[c].tnsInfo.numOfFilters[wI];
                    FDKwriteBits(s8bs, nf, (s6seq[c] == 2) ? 1 : 2);
                    if (nf) {
                      int cr = st[c].tnsInfo.coefRes[wI];
                      FDKwriteBits(s8bs, (cr == 4) ? 1 : 0, 1);
                      for (int jj = 0; jj < nf; jj++) {
                        int len = st[c].tnsInfo.length[wI][jj];
                        int ord = st[c].tnsInfo.order[wI][jj];
                        FDKwriteBits(s8bs, len, (s6seq[c] == 2) ? 4 : 6);
                        FDKwriteBits(s8bs, ord, (s6seq[c] == 2) ? 3 : 5);
                        if (ord) {
                          int dir = st[c].tnsInfo.direction[wI][jj];
                          FDKwriteBits(s8bs, dir, 1);
                          int cbits = 0;
                          if (cr == 4) {
                            cbits = 3;
                            for (int k = 0; k < ord; k++)
                              if (st[c].tnsInfo.coef[wI][jj][k] > 3 ||
                                  st[c].tnsInfo.coef[wI][jj][k] < -4) {
                                cbits = 4;
                                break;
                              }
                          } else {
                            cbits = 2;
                            for (int k = 0; k < ord; k++)
                              if (st[c].tnsInfo.coef[wI][jj][k] > 1 ||
                                  st[c].tnsInfo.coef[wI][jj][k] < -2) {
                                cbits = 3;
                                break;
                              }
                          }
                          FDKwriteBits(s8bs, -(cbits - cr), 1);
                          static const int rmask[] = {0, 1, 3, 7, 15};
                          for (int k = 0; k < ord; k++)
                            FDKwriteBits(
                                s8bs,
                                st[c].tnsInfo.coef[wI][jj][k] & rmask[cbits],
                                cbits);
                        }
                      }
                    }
                  }
                }
              }
              FDKwriteBits(s8bs, 0, 1);
              /* Spectral (:127-147). */
              for (int i = 0; i < (int)sd->noOfSections; i++) {
                if (sd->huffsection[i].codeBook == 13) continue;
                int tmp = sd->huffsection[i].sfbStart +
                          sd->huffsection[i].sfbCnt;
                for (int sfb = sd->huffsection[i].sfbStart; sfb < tmp; sfb++) {
                  FDKaacEnc_codeValues(
                      s6qspec[c] + s6off[c][sfb],
                      s6off[c][sfb + 1] - s6off[c][sfb],
                      sd->huffsection[i].codeBook, s8bs);
                }
              }
            }
          } else {
            /* SCE. */
            FDKwriteBits(s8bs, 0, 4); /* instanceTag */
            {
              SECTION_DATA *sd = &s6sec[0];
              int scfFirst = s6scf[0][sd->firstScf];
              int gv = s6gain[0] - scfFirst + 100 - 4 * (-15 - s8mdct[0]);
              FDKwriteBits(s8bs, gv, 8);
              int bt = s6seq[0];
              int wshape = (s8wshape[0] == 2) ? 1 : s8wshape[0];
              FDKwriteBits(s8bs, 0, 1);
              FDKwriteBits(s8bs, bt, 2);
              FDKwriteBits(s8bs, wshape, 1);
              if (bt == 2) {
                FDKwriteBits(s8bs, s6maxSfb0[0], 4);
                FDKwriteBits(s8bs, s8gmask[0], 7);
              } else {
                FDKwriteBits(s8bs, s6maxSfb0[0], 6);
                FDKwriteBits(s8bs, 0, 1);
              }
              int esc = (bt == 2) ? 7 : 31;
              int escBits = (bt == 2) ? 3 : 5;
              for (int i = 0; i < (int)sd->noOfSections; i++) {
                int book = sd->huffsection[i].codeBook;
                int len = sd->huffsection[i].sfbCnt;
                FDKwriteBits(s8bs, book, 4);
                while (len >= esc) {
                  FDKwriteBits(s8bs, esc, escBits);
                  len -= esc;
                }
                FDKwriteBits(s8bs, len, escBits);
              }
              int lastScf = s6scf[0][sd->firstScf];
              int lastPns =
                  s6gain[0] - s6scf[0][sd->firstScf] + 100 - 4 * (-15) - 90;
              int lastIs = 0, pcmFlag = 1;
              for (int i = 0; i < (int)sd->noOfSections; i++) {
                int cb = sd->huffsection[i].codeBook;
                if (cb == 0) continue;
                if (cb == 14 || cb == 15) {
                  for (int j = sd->huffsection[i].sfbStart;
                       j < sd->huffsection[i].sfbStart +
                               sd->huffsection[i].sfbCnt;
                       j++) {
                    int d = s6isScale[0][j] - lastIs;
                    lastIs = s6isScale[0][j];
                    if (FDKaacEnc_codeScalefactorDelta(d, s8bs)) return 1;
                  }
                } else if (cb == 13) {
                  for (int j = sd->huffsection[i].sfbStart;
                       j < sd->huffsection[i].sfbStart +
                               sd->huffsection[i].sfbCnt;
                       j++) {
                    int d = s6noise[0][j] - lastPns;
                    lastPns = s6noise[0][j];
                    if (pcmFlag) {
                      FDKwriteBits(s8bs, d + 256, 9);
                      pcmFlag = 0;
                    } else {
                      if (FDKaacEnc_codeScalefactorDelta(d, s8bs)) return 1;
                    }
                  }
                } else {
                  for (int j = sd->huffsection[i].sfbStart;
                       j < sd->huffsection[i].sfbStart +
                               sd->huffsection[i].sfbCnt;
                       j++) {
                    int d;
                    if (s6qmax[0][j] == 0)
                      d = 0;
                    else {
                      d = -(s6scf[0][j] - lastScf);
                      lastScf = s6scf[0][j];
                    }
                    if (FDKaacEnc_codeScalefactorDelta(d, s8bs)) return 1;
                  }
                }
              }
              FDKwriteBits(s8bs, 0, 1);
              {
                int wins = (s6seq[0] == 2) ? 8 : 1;
                int present = 0;
                for (int wI = 0; wI < wins; wI++)
                  if (st[0].tnsInfo.numOfFilters[wI] != 0) present = 1;
                FDKwriteBits(s8bs, present, 1);
                if (present) {
                  for (int wI = 0; wI < wins; wI++) {
                    int nf = st[0].tnsInfo.numOfFilters[wI];
                    FDKwriteBits(s8bs, nf, (s6seq[0] == 2) ? 1 : 2);
                    if (nf) {
                      int cr = st[0].tnsInfo.coefRes[wI];
                      FDKwriteBits(s8bs, (cr == 4) ? 1 : 0, 1);
                      for (int jj = 0; jj < nf; jj++) {
                        int len = st[0].tnsInfo.length[wI][jj];
                        int ord = st[0].tnsInfo.order[wI][jj];
                        FDKwriteBits(s8bs, len, (s6seq[0] == 2) ? 4 : 6);
                        FDKwriteBits(s8bs, ord, (s6seq[0] == 2) ? 3 : 5);
                        if (ord) {
                          FDKwriteBits(s8bs, st[0].tnsInfo.direction[wI][jj],
                                       1);
                          int cbits = 0;
                          if (cr == 4) {
                            cbits = 3;
                            for (int k = 0; k < ord; k++)
                              if (st[0].tnsInfo.coef[wI][jj][k] > 3 ||
                                  st[0].tnsInfo.coef[wI][jj][k] < -4) {
                                cbits = 4;
                                break;
                              }
                          } else {
                            cbits = 2;
                            for (int k = 0; k < ord; k++)
                              if (st[0].tnsInfo.coef[wI][jj][k] > 1 ||
                                  st[0].tnsInfo.coef[wI][jj][k] < -2) {
                                cbits = 3;
                                break;
                              }
                          }
                          FDKwriteBits(s8bs, -(cbits - cr), 1);
                          static const int rmask[] = {0, 1, 3, 7, 15};
                          for (int k = 0; k < ord; k++)
                            FDKwriteBits(
                                s8bs,
                                st[0].tnsInfo.coef[wI][jj][k] & rmask[cbits],
                                cbits);
                        }
                      }
                    }
                  }
                }
              }
              FDKwriteBits(s8bs, 0, 1);
              for (int i = 0; i < (int)sd->noOfSections; i++) {
                if (sd->huffsection[i].codeBook == 13) continue;
                int tmp = sd->huffsection[i].sfbStart +
                          sd->huffsection[i].sfbCnt;
                for (int sfb = sd->huffsection[i].sfbStart; sfb < tmp; sfb++)
                  FDKaacEnc_codeValues(
                      s6qspec[0] + s6off[0][sfb],
                      s6off[0][sfb + 1] - s6off[0][sfb],
                      sd->huffsection[i].codeBook, s8bs);
              }
            }
          }
          unsigned elemBits = FDKgetValidBits(s8bs);
          /* S8ELEM dump before the fill tail overwrites the pad bits:
             the last element byte carries zero padding here (like the Rust
             BitWriter tail), while after fill/END it would carry fill/END
             bits in those positions. */
          std::fprintf(out, "S8ELEM bits=%u hex=", elemBits);
          for (unsigned k = 0; k < (elemBits + 7) / 8; k++)
            std::fprintf(out, "%02x", s8buf[k]);
          std::fprintf(out, "\n");
          /* Fill + END + align (WriteBitstream tail :1282-1310). */
          {
            QC_OUT_EXTENSION fillExt;
            std::memset(&fillExt, 0, sizeof(fillExt));
            fillExt.type = EXT_FILL_DATA;
            fillExt.nPayloadBits = s8totfill;
            /* Real fill sizing already proven by S6; emit via the real
               writeExtensionData with the transport handle NULL? That path
               needs hTpEnc for writes. Use manual replica of the FIL loop
               with direct writes (values are fixed: ID_FIL + cnt + type +
               nibble + 0xA5 bytes), self-checked against s8totfill. */
            int payload = s8totfill;
            unsigned fillStart = FDKgetValidBits(s8bs);
            (void)fillExt;
            while (payload >= 7) {
              int cnt = 0, esc = -1;
              payload -= 7;
              if (payload >= 120) {
                payload -= 8;
                esc = 0;
              }
              cnt = (payload >> 3) < 269 ? (payload >> 3) : 269;
              if (cnt >= 15) esc = cnt - 15 + 1;
              FDKwriteBits(s8bs, 6, 3);
              if (esc >= 0) {
                FDKwriteBits(s8bs, 15, 4);
                FDKwriteBits(s8bs, esc, 8);
              } else {
                FDKwriteBits(s8bs, cnt, 4);
              }
              int bits = (cnt * 8) < payload ? (cnt * 8) : payload;
              if (bits >= 4) {
                FDKwriteBits(s8bs, 1, 4);
                FDKwriteBits(s8bs, 0, 4);
                int wb = bits - 8;
                while (wb >= 8) {
                  FDKwriteBits(s8bs, 0xA5, 8);
                  wb -= 8;
                }
              }
              payload -= bits;
            }
            unsigned fillBits = FDKgetValidBits(s8bs) - fillStart;
            if ((int)fillBits != s8totfill) {
              std::fprintf(stderr,
                           "S8 FILL MISMATCH f=%ld emit=%u count=%d\n", f,
                           fillBits, (int)s8totfill);
              return 1;
            }
          }
          FDKwriteBits(s8bs, 7, 3); /* ID_END */
          FDKwriteBits(s8bs, 0, s8align);
          FDKsyncCache(s8bs);
          unsigned rawBits = FDKgetValidBits(s8bs);
          if ((int)rawBits != s8total) {
            std::fprintf(stderr, "S8 RAW MISMATCH f=%ld emit=%u count=%d\n",
                         f, rawBits, (int)s8total);
            return 1;
          }
          std::fprintf(out, "S8RAW bits=%u hex=", rawBits);
          for (unsigned k = 0; k < (rawBits + 7) / 8; k++)
            std::fprintf(out, "%02x", s8buf[k]);
          std::fprintf(out, "\n");
          std::fprintf(out, "S8FINAL static=%d useddyn=%d totfill=%d align=%d "
                            "total=%d\n",
                       (int)s8static, (int)s8useddyn, (int)s8totfill,
                       (int)s8align, (int)s8total);
          FDKdeleteBitStream(s8bs);
        }
        std::fprintf(out, "S8END\n");
      }

      /* ---- S3 stage: scalar quantizer primitives (quantize.cpp) via the
         real exported calls. Inputs are the post-MS/IS S1 spectra (s1spec,
         identical to s1qoc mdctSpectrum), which is exactly what
         QuantizeSpectrum consumes in the aliased encoder (aacenc.cpp:803).
         Uniform QSS per grid gain (scf = 0, globalGain = gain);
         dZoneQuantEnable = 0 (LC, qc_main.cpp:399-411).
         Valid-envelope gate (see quant_probe.cpp): every line is scanned
         for totalShift >= 0 and every S3Q vector for SHORT_MIN before any
         inverse-path call; violation aborts loudly instead of executing
         undefined shifts / out-of-bounds reads. ---- */
      {
        static const int S3_GAINS[] = {-72, -32, -8, 64};
        std::fprintf(out, "S3FRAME f=%ld\n", f);
        for (int c = 0; c < channels; c++) {
          int cnt = s1cnt[c];
          int per = s1perGroup[c];
          int maxSfb = (channels == 2) ? s1maxSfb : st[c].maxSfbPerGroup;
          /* S3 input contract: active bands carry the S1 spectrum;
           * inactive-band lines (per-band index >= maxSfbPerGroup) are
           * zeroed. Their C contents are proven scratch (validate_s1.py
           * confines all run-to-run wobble there), and no downstream
           * consumer (S4 scf search, S5/S6 counting) ever reads them —
           * every reference loop is bounded by maxSfbPerGroup. Zeroing
           * keeps S3DIST/S3EN deterministic without touching any real
           * decision. */
          FIXP_DBL s3spec[FRAME_LEN];
          std::memcpy(s3spec, s1spec[c], sizeof(s3spec));
          for (int sfbOffs = 0; sfbOffs < cnt; sfbOffs += per)
            for (int sfb = maxSfb; sfb < per; sfb++)
              for (int line = s1poc[c].sfbOffsets[sfbOffs + sfb];
                   line < s1poc[c].sfbOffsets[sfbOffs + sfb + 1]; line++)
                s3spec[line] = 0;
          std::fprintf(out, "S3GEO ch=%d cnt=%d per=%d max=%d seq=%d\n", c,
                       cnt, per, maxSfb, (int)st[c].bs.lastWindowSequence);
          std::fprintf(out, "S3OFF");
          for (int i = 0; i <= cnt; i++)
            std::fprintf(out, " %d", (int)s1poc[c].sfbOffsets[i]);
          std::fprintf(out, "\n");
          for (unsigned gi = 0; gi < sizeof(S3_GAINS) / sizeof(S3_GAINS[0]); gi++) {
            int g = S3_GAINS[gi];
            SHORT s3q[FRAME_LEN];
            /* Bands past maxSfbPerGroup are never visited below (same as
             * the reference loop bounds); zero them so the dump holds no
             * stack garbage. The real encoder's sectioning likewise only
             * reads visited bands. */
            std::memset(s3q, 0, sizeof(s3q));
            INT scf0[MAX_GROUPED_SFB];
            std::memset(scf0, 0, sizeof(scf0));
            FDKaacEnc_QuantizeSpectrum(cnt, maxSfb, per, s1poc[c].sfbOffsets,
                                       s3spec, g, scf0, s3q, 0);
            /* Envelope scan (replicated reference asserts). */
            for (int i = 0; i < FRAME_LEN; i++) {
              if ((int)s3q[i] == -32768) {
                std::fprintf(stderr,
                             "S3 ENVELOPE VIOLATION (q==SHORT_MIN) f=%ld ch=%d g=%d\n",
                             f, c, g);
                return 1;
              }
            }
            {
              int qs = ((-g) >> 2) + 1;
              for (int sfbOffs = 0; sfbOffs < cnt; sfbOffs += per)
                for (int sfb = 0; sfb < maxSfb; sfb++) {
                  int start = s1poc[c].sfbOffsets[sfbOffs + sfb];
                  int stop = s1poc[c].sfbOffsets[sfbOffs + sfb + 1];
                  for (int line = start; line < stop; line++) {
                    SHORT qtab = FDKaacEnc_quantTableQ[(-g) & 3];
                    INT64 acc64 =
                        ((INT64)((INT)((int)qtab << 16))) * ((INT64)s3spec[line]);
                    INT a = (INT)(acc64 >> 32);
                    if (a != 0) {
                      if (a < 0) a = -a;
                      int sh = CntLeadingZeros(a) - 1;
                      int t = qs - sh + 1;
                      if ((16 - 4) - (3 * (t >> 2)) < 0) {
                        std::fprintf(stderr,
                                     "S3 ENVELOPE VIOLATION (totalShift<0) f=%ld ch=%d g=%d\n",
                                     f, c, g);
                        return 1;
                      }
                    }
                  }
                }
            }
            std::fprintf(out, "S3Q g=%d ch=%d", g, c);
            for (int i = 0; i < FRAME_LEN; i++)
              std::fprintf(out, " %d", (int)s3q[i]);
            std::fprintf(out, "\n");
            std::fprintf(out, "S3DIST g=%d ch=%d", g, c);
            for (int b = 0; b < cnt; b++) {
              int start = s1poc[c].sfbOffsets[b];
              int width = s1poc[c].sfbOffsets[b + 1] - start;
              SHORT scratch[1024];
              FIXP_DBL d = FDKaacEnc_calcSfbDist(s3spec + start, scratch,
                                                width, g, 0);
              std::fprintf(out, " %d", (int)d);
            }
            std::fprintf(out, "\n");
            std::fprintf(out, "S3EN g=%d ch=%d en", g, c);
            FIXP_DBL s3e[MAX_GROUPED_SFB], s3d[MAX_GROUPED_SFB];
            for (int b = 0; b < cnt; b++) {
              int start = s1poc[c].sfbOffsets[b];
              int width = s1poc[c].sfbOffsets[b + 1] - start;
              FIXP_DBL en = 0, dist = 0;
              FDKaacEnc_calcSfbQuantEnergyAndDist(s3spec + start, s3q + start,
                                                 width, g, &en, &dist);
              s3e[b] = en;
              s3d[b] = dist;
            }
            for (int b = 0; b < cnt; b++) std::fprintf(out, " %d", (int)s3e[b]);
            std::fprintf(out, " dist");
            for (int b = 0; b < cnt; b++) std::fprintf(out, " %d", (int)s3d[b]);
            std::fprintf(out, "\n");
          }
        }
        std::fprintf(out, "S3END\n");
      }
    }
  }

  std::fclose(out);
  return 0;
}
