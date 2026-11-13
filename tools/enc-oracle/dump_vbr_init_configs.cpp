/* VBR-1..5 init-time reference dump (test-only oracle tooling, not production).
 *
 * Drives the UNMODIFIED FDK-AAC v3.0.0 implementation through its real init
 * path and dumps the effective per-profile values consumed by Ferrite's
 * `cfg_<rate>_<pch>.txt` snapshots:
 *   - `FDKaacEnc_GetVBRBitrate` (nominal bitrate table check),
 *   - `FDKaacEnc_InitPsyConfiguration` for LONG and SHORT windows
 *     (SFB geometry, spreading factors, minSnr),
 *   - `FDKaacEnc_InitTnsConfiguration` for LONG and SHORT windows,
 *   - `FDKaacEnc_QCInit` (which calls the real `InitElementBits` +
 *     `FDKaacEnc_AdjThrInit`: peOffset, ahParam, vbrQualFactor, meanPe,
 *     frame-bit derivations).
 * Follows tools/enc-oracle/enc_oracle.cpp wiring (single-element mono/stereo,
 * ADTS static bits 56, invQuant=2, FULL bit reservoir, VBR branch).
 *
 * Build against the pinned reference checkout (see
 * docs/reference/fdk-aac-reference-revision.txt; the init paths used here
 * are byte-identical between the public v3.0.0 tag and the stripped
 * checkout). With REF set to the reference checkout and BLD to a build
 * directory (same convention as this directory's README):
 *
 *   cmake -S "$REF" -B "$BLD" -DCMAKE_BUILD_TYPE=Release -DBUILD_PROGRAMS=OFF
 *   cmake --build "$BLD" --target fdk-aac -j10
 *   c++ -std=c++11 -O2 \
 *     -I "$REF/cpp/libAACenc/src" \
 *     -I "$REF/cpp/libFDK/include" \
 *     -I "$REF/cpp/libSYS/include" \
 *     -I "$REF/cpp/libMpegTPEnc/include" \
 *     -I "$REF/cpp/libSBRenc/include" \
 *     -I "$REF/cpp/libMpegTPDec/include" \
 *     -I "$REF/cpp/libSBRdec/include" \
 *     -I "$REF/cpp/libAACdec/include" \
 *     tools/enc-oracle/dump_vbr_init_configs.cpp \
 *     -L"$BLD" -lfdk-aac -Wl,-rpath,"$BLD" \
 *     -o "$BLD/dump_vbr_init_configs"
 *   "$BLD/dump_vbr_init_configs" > "$BLD/vbr_init_reference.txt"
 *
 * FDK revision verified during development: tag v3.0.0, commit
 * a68a51aa45d86a43c7b228e3d45624af09dcdd8d (mstorsjo/fdk-aac).
 * Confirm before regenerating:
 *   git -C "$REF" rev-parse HEAD; git -C "$REF" describe --tags
 *
 * The committed `rust/src/aac_enc/testdata/cfg_*.txt` snapshots are
 * transcribed from this dump's CFG/TNSCFG sections (same file layout as
 * `enc_oracle --configs` output, minus the mode suffix in labels, plus no
 * `allowIS/allowMS` suffix on CFG heads to match the established format).
 * Transcription is self-checked by re-emitting the four pre-existing VBR-5
 * files byte-identically before trusting new files.
 */

#include <cstdio>
#include <cstring>

#include "FDK_tools_rom.h"
#include "adj_thr.h"
#include "adj_thr_data.h"
#include "aacenc.h"
#include "channel_map.h"
#include "fixpoint_math.h"
#include "psy_configuration.h"
#include "psy_const.h"
#include "qc_data.h"
#include "qc_main.h"
#include "tns_func.h"

#define FRAME_LEN 1024

static void print_q31(FILE *out, const char *tag, const FIXP_DBL *v, int n) {
  std::fprintf(out, "%s", tag);
  for (int i = 0; i < n; i++) std::fprintf(out, "%d ", (int)v[i]);
  std::fprintf(out, "\n");
}

static void dump_psy(const char *label, int sr, const PSY_CONFIGURATION *c) {
  std::printf("CFG %s sr=%d cnt=%d active=%d activeLFE=%d lowpass=%d lowpassLFE=%d clipE=%d allowIS=%d allowMS=%d\n",
              label, sr, c->sfbCnt, c->sfbActive, c->sfbActiveLFE,
              c->lowpassLine, c->lowpassLineLFE, (int)c->clipEnergy,
              (int)c->allowIS, (int)c->allowMS);
  std::printf("OFFSETS");
  for (int i = 0; i <= c->sfbCnt; i++) std::printf(" %d", c->sfbOffset[i]);
  std::printf("\n");
  print_q31(stdout, "PCMQ ", c->sfbPcmQuantThreshold, c->sfbCnt);
  print_q31(stdout, "MASKHI ", c->sfbMaskHighFactor, c->sfbCnt);
  print_q31(stdout, "MASKLO ", c->sfbMaskLowFactor, c->sfbCnt);
  print_q31(stdout, "MASKHIS ", c->sfbMaskHighFactorSprEn, c->sfbCnt);
  print_q31(stdout, "MASKLOS ", c->sfbMaskLowFactorSprEn, c->sfbCnt);
  print_q31(stdout, "MINSNR ", c->sfbMinSnrLdData, c->sfbCnt);
}

static void dump_tns(const char *label, const TNS_CONFIG *t) {
  std::printf("TNSCFG %s active=%d maxOrder=%d coefRes=%d startBand=%d,%d startLine=%d,%d stopBand=%d stopLine=%d threshOn=%d,%d limitOrder=%d,%d\n",
              label, (int)t->tnsActive, t->maxOrder, t->coefRes,
              t->lpcStartBand[0], t->lpcStartBand[1],
              t->lpcStartLine[0], t->lpcStartLine[1],
              t->lpcStopBand, t->lpcStopLine,
              (int)t->confTab.threshOn[0], (int)t->confTab.threshOn[1],
              t->confTab.tnsLimitOrder[0], t->confTab.tnsLimitOrder[1]);
}

int main() {
  std::printf("# FDK-AAC v3.0.0 VBR-1..5 reference dump\n");
  std::printf("# commit a68a51aa45d86a43c7b228e3d45624af09dcdd8d tag v3.0.0\n");

  /* 1. Verify bitrate table via real GetVBRBitrate. */
  for (int mode = 1; mode <= 5; mode++) {
    int mono = FDKaacEnc_GetVBRBitrate((AACENC_BITRATE_MODE)mode, MODE_1);
    int ster = FDKaacEnc_GetVBRBitrate((AACENC_BITRATE_MODE)mode, MODE_2);
    std::printf("VBRBITRATE mode=%d mono_total=%d stereo_total=%d stereo_per_ch=%d\n",
                mode, mono, ster, ster / 2);
  }

  const int bw_tab[6] = {0, 13000, 13000, 15750, 16500, 19293};
  const int rates[12] = {8000, 11025, 12000, 16000, 22050, 24000, 32000,
                         44100, 48000, 64000, 88200, 96000};

  for (int mode = 1; mode <= 5; mode++) {
    QCDATA_BR_MODE qcmode;
    switch (mode) {
      case 1: qcmode = QCDATA_BR_MODE_VBR_1; break;
      case 2: qcmode = QCDATA_BR_MODE_VBR_2; break;
      case 3: qcmode = QCDATA_BR_MODE_VBR_3; break;
      case 4: qcmode = QCDATA_BR_MODE_VBR_4; break;
      default: qcmode = QCDATA_BR_MODE_VBR_5; break;
    }
    for (int ri = 0; ri < 12; ri++) {
      int sr = rates[ri];
      for (int ch = 1; ch <= 2; ch++) {
        int totalBr = FDKaacEnc_GetVBRBitrate((AACENC_BITRATE_MODE)mode, (ch == 1) ? MODE_1 : MODE_2);
        int chBitrate = totalBr / ch;
        int nEff = ch;
        int bw = bw_tab[mode] < sr / 2 ? bw_tab[mode] : sr / 2;

        /* --- PsyConfiguration (real calls) --- */
        PSY_CONFIGURATION cL, cS;
        if (FDKaacEnc_InitPsyConfiguration(chBitrate, sr, bw, LONG_WINDOW,
                                           FRAME_LEN, 1, 1, &cL, FB_LC) !=
                AAC_ENC_OK ||
            FDKaacEnc_InitPsyConfiguration(chBitrate, sr, bw, SHORT_WINDOW,
                                           FRAME_LEN, 1, 1, &cS, FB_LC) !=
                AAC_ENC_OK) {
          std::fprintf(stderr, "psy init failed mode=%d sr=%d ch=%d\n", mode,
                       sr, ch);
          return 1;
        }
        std::printf("# profile=VBR-%d sr=%d ch=%d pch=%d total=%d bw=%d\n",
                    mode, sr, ch, chBitrate, totalBr, bw);
        char lab[64];
        std::snprintf(lab, sizeof(lab), "LONG m%d", mode);
        dump_psy(lab, sr, &cL);
        std::snprintf(lab, sizeof(lab), "SHORT m%d", mode);
        dump_psy(lab, sr, &cS);

        /* --- TNS (real calls, oracle mask convention) --- */
        {
          TNS_CONFIG tL, tS;
          int tnsCh = ch;
          if (FDKaacEnc_InitTnsConfiguration(totalBr, sr, tnsCh, LONG_WINDOW,
                                             FRAME_LEN, 0, 0, &tL, &cL, 2,
                                             8) != AAC_ENC_OK ||
              FDKaacEnc_InitTnsConfiguration(totalBr, sr, tnsCh, SHORT_WINDOW,
                                             FRAME_LEN, 0, 0, &tS, &cS, 1,
                                             4) != AAC_ENC_OK) {
            std::fprintf(stderr, "tns init failed mode=%d sr=%d ch=%d\n",
                         mode, sr, ch);
            return 1;
          }
          char tl[64], ts[64];
          std::snprintf(tl, sizeof(tl), "LONG m%d", mode);
          std::snprintf(ts, sizeof(ts), "SHORT m%d", mode);
          dump_tns(tl, &tL);
          dump_tns(ts, &tS);
        }

        /* --- QCInit -> AdjThrInit (real calls, oracle wiring) --- */
        ATS_ELEMENT s1ats;
        std::memset(&s1ats, 0, sizeof(s1ats));
        ADJ_THR_STATE hAdjThr;
        std::memset(&hAdjThr, 0, sizeof(hAdjThr));
        CHANNEL_MAPPING cm;
        std::memset(&cm, 0, sizeof(cm));
        cm.encMode = (ch == 2) ? MODE_2 : MODE_1;
        cm.nElements = 1;
        cm.nChannels = ch;
        cm.nChannelsEff = nEff;
        cm.elInfo[0].elType = (ch == 2) ? ID_CPE : ID_SCE;
        cm.elInfo[0].nChannelsInEl = ch;
        cm.elInfo[0].ChannelIndex[0] = 0;
        cm.elInfo[0].ChannelIndex[1] = 1;
        cm.elInfo[0].relativeBits = (FIXP_DBL)MAXVAL_DBL;
        hAdjThr.adjThrStateElem[0] = &s1ats;
        QC_STATE qc;
        std::memset(&qc, 0, sizeof(qc));
        ELEMENT_BITS elBits;
        std::memset(&elBits, 0, sizeof(elBits));
        qc.elementBits[0] = &elBits;
        qc.hAdjThr = &hAdjThr;

        INT avgBitsFrame =
            FDKaacEnc_CalcBitsPerFrame(totalBr, FRAME_LEN, sr);
        INT averageBits = (avgBitsFrame + 7) & ~7;
        INT bitRes = 6144 * nEff;
        INT maxBits = 6144 * nEff;
        maxBits = fixMax(maxBits, (avgBitsFrame + 7) & ~7);
        INT minBits = fixMin(0, avgBitsFrame & ~7);
        INT qbw = 0;
        FIXP_DBL bw_ratio = fDivNorm((FIXP_DBL)(10 * FRAME_LEN * bw),
                                     (FIXP_DBL)sr, &qbw);
        INT meanPe = fixMax(
            (INT)scaleValue(bw_ratio, qbw + 1 - (DFRACT_BITS - 1)), 1);
        INT mbfac_e = 0;
        FIXP_DBL mbfac = fDivNorm(maxBits, averageBits, &mbfac_e);
        INT maxBitFac =
            scaleValue(mbfac, -(DFRACT_BITS - 1 - 24 - mbfac_e));

        QC_INIT init;
        std::memset(&init, 0, sizeof(init));
        init.channelMapping = &cm;
        init.sceCpe = 0;
        init.maxBits = maxBits;
        init.averageBits = averageBits;
        init.bitRes = bitRes;
        init.sampleRate = sr;
        init.isLowDelay = 0;
        init.staticBits = 56;
        init.bitrateMode = qcmode;
        init.meanPe = meanPe;
        init.invQuant = 2;
        init.maxIterations = 5;
        init.maxBitFac = maxBitFac;
        init.bitrate = totalBr;
        init.nSubFrames = 1;
        init.minBits = minBits;
        init.bitResMode = AACENC_BR_MODE_FULL;
        init.bitDistributionMode = 1;
        init.padding.paddingRest = sr;
        if (FDKaacEnc_QCInit(&qc, &init, 1) != AAC_ENC_OK) {
          std::fprintf(stderr, "QCInit failed mode=%d sr=%d ch=%d\n", mode,
                       sr, ch);
          return 1;
        }
        std::printf("ATS mode=%d sr=%d ch=%d chBitrate=%d peOffset=%d ahMod=%d ahL=%d ahS=%d vbrQ=%d chaosOld=%d msaMax=%d msaStart=%d msaFac=%d msaOff=%d b2m=%d b2e=%d peMin=%d peMax=%d meanPe=%d avgBits=%d maxBits=%d minBits=%d bitRes=%d maxBitFac=%d dZone=%d invQ=%d\n",
                    mode, sr, ch, chBitrate, (int)s1ats.peOffset,
                    (int)s1ats.ahParam.modifyMinSnr,
                    (int)s1ats.ahParam.startSfbL,
                    (int)s1ats.ahParam.startSfbS, (int)s1ats.vbrQualFactor,
                    (int)s1ats.chaosMeasureOld,
                    (int)s1ats.minSnrAdaptParam.maxRed,
                    (int)s1ats.minSnrAdaptParam.startRatio,
                    (int)s1ats.minSnrAdaptParam.redRatioFac,
                    (int)s1ats.minSnrAdaptParam.redOffs,
                    (int)s1ats.bits2PeFactor_m, (int)s1ats.bits2PeFactor_e,
                    (int)s1ats.peMin, (int)s1ats.peMax, (int)meanPe,
                    (int)averageBits, (int)maxBits, (int)minBits, (int)bitRes,
                    (int)maxBitFac, (int)qc.dZoneQuantEnable,
                    (int)qc.invQuant);
      }
    }
  }
  return 0;
}
