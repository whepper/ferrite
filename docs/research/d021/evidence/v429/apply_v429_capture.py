#!/usr/bin/env python3
"""V4.29 isolated scratch C capture (S4/S5/S6/TNS/static bit attribution).

Applies ONLY to the scratch copy at v429-scratch (never the main worktree).
Guarded by -DV429_BITATTR; baseline build omits the flag for byte-identity check.
"""
import sys

SCRATCH = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v429-scratch"

AACENC_HELPER = r'''
#if defined(V429_BITATTR)
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int v429_frame_counter = 0;
extern int g_v429_iterations;
extern int g_v429_crashed;

static void v429_capture_bitattr(
    const char *fixture, int frame, QC_OUT *qcOut, QC_OUT_ELEMENT *qcEl,
    PSY_OUT_ELEMENT *psyEl, PSY_OUT_CHANNEL *pc0, PSY_OUT_CHANNEL *pc1,
    int nChannels, int globHdrBits, int bitResTot, int iterations, int crashed,
    ELEMENT_BITS *elBits) {
  const char *outdir = getenv("V429_OUTDIR");
  const char *rev = getenv("V429_SOURCE_REV");
  char path[1024];
  FILE *fp;
  int i, ch, w, f, s;
  if (!outdir) outdir = "/tmp/v429-out";
  if (!rev) rev = "unknown";
  snprintf(path, sizeof(path), "%s/%s_f%d_bitattr.txt", outdir, fixture, frame);
  fp = fopen(path, "w");
  if (!fp) return;
  fprintf(fp, "V429_BITATTR fixture=%s frame_1based=%d frame_zero_based=%d stage=post_finalize\n",
          fixture, frame + 1, frame);
  fprintf(fp, "SOURCE_REV %s\n", rev);
  fprintf(fp, "QC_OUT grantedDyn=%d maxDyn=%d totFill=%d elExt=%d globalExt=%d static=%d usedDyn=%d align=%d total=%d globHdr=%d bitResTot=%d\n",
          qcOut->grantedDynBits, qcOut->maxDynBits, qcOut->totFillBits,
          qcOut->elementExtBits, qcOut->globalExtBits, qcOut->staticBits,
          qcOut->usedDynBits, qcOut->alignBits, qcOut->totalBits,
          globHdrBits, bitResTot);
  fprintf(fp, "QC_GRANTS totalNoRedPe=%d totalGrantedPeCorr=%d\n",
          qcOut->totalNoRedPe, qcOut->totalGrantedPeCorr);
  if (elBits) {
    fprintf(fp, "ELBITS chBitrateEl=%d maxBitsEl=%d bitResLevelEl=%d maxBitResBitsEl=%d relativeBitsEl=%lld\n",
            elBits->chBitrateEl, elBits->maxBitsEl, elBits->bitResLevelEl,
            elBits->maxBitResBitsEl, (long long)elBits->relativeBitsEl);
  }
  fprintf(fp, "EL0 staticUsed=%d dynUsed=%d extUsed=%d grantedDyn=%d grantedPe=%d grantedPeCorr=%d\n",
          qcEl->staticBitsUsed, qcEl->dynBitsUsed, qcEl->extBitsUsed,
          qcEl->grantedDynBits, qcEl->grantedPe, qcEl->grantedPeCorr);
  fprintf(fp, "EL0_PE pe=%d constPart=%d nActive=%d offset=%d ch0pe=%d ch1pe=%d\n",
          qcEl->peData.pe, qcEl->peData.constPart, qcEl->peData.nActiveLines,
          qcEl->peData.offset, qcEl->peData.peChannelData[0].pe,
          nChannels > 1 ? qcEl->peData.peChannelData[1].pe : -1);
  fprintf(fp, "LOOP iterations=%d crashed=%d\n", iterations, crashed);
  fprintf(fp, "MSDIGEST %d\n", psyEl->toolsInfo.msDigest);
  fprintf(fp, "MSMASK");
  for (i = 0; i < 56; i++) fprintf(fp, " %d", psyEl->toolsInfo.msMask[i]);
  fprintf(fp, "\n");
  for (ch = 0; ch < nChannels; ch++) {
    QC_OUT_CHANNEL *qc = qcEl->qcOutChannel[ch];
    PSY_OUT_CHANNEL *pc = (ch == 0) ? pc0 : pc1;
    fprintf(fp, "CH%d gain=%d nSfb=%d\n", ch, qc->globalGain, pc->sfbCnt);
    fprintf(fp, "CH%d_SCF", ch);
    for (i = 0; i < pc->sfbCnt; i++) fprintf(fp, " %d", qc->scf[i]);
    fprintf(fp, "\n");
    fprintf(fp, "CH%d_MAXV", ch);
    for (i = 0; i < pc->sfbCnt; i++) fprintf(fp, " %u", qc->maxValueInSfb[i]);
    fprintf(fp, "\n");
    fprintf(fp, "CH%d_SECT n=%d side=%d huff=%d scf=%d noise=%d firstScf=%d\n",
            ch, qc->sectionData.noOfSections, qc->sectionData.sideInfoBits,
            qc->sectionData.huffmanBits, qc->sectionData.scalefacBits,
            qc->sectionData.noiseNrgBits, qc->sectionData.firstScf);
    for (s = 0; s < qc->sectionData.noOfSections; s++) {
      fprintf(fp, "CH%d_SEC%d book=%d start=%d cnt=%d bits=%d\n", ch, s,
              qc->sectionData.huffsection[s].codeBook,
              qc->sectionData.huffsection[s].sfbStart,
              qc->sectionData.huffsection[s].sfbCnt,
              qc->sectionData.huffsection[s].sectionBits);
    }
    fprintf(fp, "CH%d_QUANT", ch);
    for (i = 0; i < 1024; i++) fprintf(fp, " %d", (int)qc->quantSpec[i]);
    fprintf(fp, "\n");
    fprintf(fp, "CH%d_TNS_NUMFILT", ch);
    for (w = 0; w < 8; w++) fprintf(fp, " %d", pc->tnsInfo.numOfFilters[w]);
    fprintf(fp, "\n");
    fprintf(fp, "CH%d_TNS_COEFRES", ch);
    for (w = 0; w < 8; w++) fprintf(fp, " %d", pc->tnsInfo.coefRes[w]);
    fprintf(fp, "\n");
    for (w = 0; w < 8; w++) {
      for (f = 0; f < 2; f++) {
        fprintf(fp, "CH%d_TNS_W%d_F%d len=%d order=%d dir=%d cc=%d coef",
                ch, w, f, pc->tnsInfo.length[w][f], pc->tnsInfo.order[w][f],
                pc->tnsInfo.direction[w][f], pc->tnsInfo.coefCompress[w][f]);
        for (i = 0; i < 12; i++) fprintf(fp, " %d", pc->tnsInfo.coef[w][f][i]);
        fprintf(fp, "\n");
      }
    }
  }
  fclose(fp);
}
#endif
'''

def patch_aacenc(path):
    src = open(path).read()
    assert "V429_BITATTR" not in src, "already patched"
    anchor = "AAC_ENCODER_ERROR FDKaacEnc_EncodeFrame("
    assert anchor in src
    src = src.replace(anchor, AACENC_HELPER + "\n" + anchor, 1)
    old_counter = "  FDKmemclear(extPayloadUsed, MAX_TOTAL_EXT_PAYLOADS * sizeof(UCHAR));"
    assert old_counter in src
    src = src.replace(
        old_counter,
        old_counter + "\n#if defined(V429_BITATTR)\n  v429_frame_counter++;\n#endif",
        1,
    )
    old_fin = """    /*-------------------------------------------- */
    FDKaacEnc_updateBitres(cm, hAacEnc->qcKernel, hAacEnc->qcOut);"""
    assert old_fin in src
    hook = old_fin + """

#if defined(V429_BITATTR)
    {
      const char *v429_fixture = getenv("V429_FIXTURE");
      const char *v429_outdir = getenv("V429_OUTDIR");
      if (!v429_fixture) v429_fixture = "unknown";
      if (!v429_outdir) v429_outdir = "/tmp/v429-out";
      (void)v429_outdir;
      {
        int v429_frame = v429_frame_counter - 1;
        int v429_nch = cm->elInfo[0].nChannelsInEl;
        QC_OUT *v429_qcOut = qcOut;
        QC_OUT_ELEMENT *v429_qcEl = qcOut->qcElement[0];
        PSY_OUT_ELEMENT *v429_psyEl = psyOut->psyOutElement[0];
        PSY_OUT_CHANNEL *v429_pc0 = v429_psyEl->psyOutChannel[0];
        PSY_OUT_CHANNEL *v429_pc1 = (v429_nch > 1) ? v429_psyEl->psyOutChannel[1] : v429_pc0;
        ELEMENT_BITS *v429_elBits = hAacEnc->qcKernel->elementBits[0];
        v429_capture_bitattr(v429_fixture, v429_frame, v429_qcOut, v429_qcEl,
                             v429_psyEl, v429_pc0, v429_pc1, v429_nch,
                             hAacEnc->qcKernel->globHdrBits,
                             hAacEnc->qcKernel->bitResTot,
                             g_v429_iterations, g_v429_crashed, v429_elBits);
      }
    }
#endif"""
    src = src.replace(old_fin, hook, 1)
    open(path, "w").write(src)
    print("patched aacenc.cpp")

QCMAINC_GLOBALS = r'''
#if defined(V429_BITATTR)
int g_v429_iterations = 0;
int g_v429_crashed = 0;
#endif
'''

def patch_qcmain(path):
    src = open(path).read()
    assert "V429_BITATTR" not in src, "already patched"
    anchor = "static void FDKaacEnc_crashRecovery(INT nChannels,"
    assert anchor in src
    src = src.replace(anchor, QCMAINC_GLOBALS + "\n" + anchor, 1)
    old_crash = "static void FDKaacEnc_crashRecovery(INT nChannels,\n"
    # set crashed flag at crashRecovery entry: insert after opening brace of body.
    # Find function definition end: first '{' after anchor occurrence.
    idx = src.index("g_v429_crashed = 0;\n#endif\n\nstatic void FDKaacEnc_crashRecovery")
    # locate body start
    bstart = src.index("{", src.index("FDKaacEnc_crashRecovery(INT nChannels,", idx))
    src = src[: bstart + 1] + "\n#if defined(V429_BITATTR)\n  g_v429_crashed = 1;\n#endif" + src[bstart + 1 :]
    # record iterations at end of QCMain: after the do-while loop close, before return
    old_tail = "  } while (!quantizationDone);"
    assert old_tail in src
    src = src.replace(
        old_tail,
        old_tail
        + "\n#if defined(V429_BITATTR)\n  g_v429_iterations = iterations[0][0];\n#endif",
        1,
    )
    # reset crashed flag at QCMain entry
    old_qcmain = "AAC_ENCODER_ERROR FDKaacEnc_QCMain("
    assert old_qcmain in src
    # insert reset right after first '{' following QCMain signature: find end of signature meio
    qidx = src.index(old_qcmain)
    qb = src.index("{", qidx)
    src = src[: qb + 1] + "\n#if defined(V429_BITATTR)\n  g_v429_crashed = 0;\n  g_v429_iterations = 0;\n#endif" + src[qb + 1 :]
    open(path, "w").write(src)
    print("patched qc_main.cpp")

if __name__ == "__main__":
    patch_aacenc(SCRATCH + "/cpp/libAACenc/src/aacenc.cpp")
    patch_qcmain(SCRATCH + "/cpp/libAACenc/src/qc_main.cpp")
