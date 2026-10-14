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

  INT_PCM fresh[2][FRAME_LEN];
  int isShort[2], nWin[2], wLen[2], zeroSpecCh[2];
  int tnsActive[TRANS_FAC], nrgScaling[2], tnsSpecShift;

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
  }

  std::fclose(out);
  return 0;
}
