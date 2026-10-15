# M4 Planning Audit — AAC-LC Quantization and Rate Control (VBR-5, Afterburner-On)

Status: planning only. No production DSP or bitstream output is implemented in this task.
Starting point: commit `1785529` (M1–M3 checkpoint, working tree clean).
Scope: psychoacoustic output → quantization → scalefactor decisions → bit-count
estimation → VBR-5 rate control → afterburner, up to (but excluding) bitstream writes.

Conventions used in this document:

- **[Fact]** — verified by reading the cited source at the stated path/line.
- **[Inference]** — best reading of the code; flagged where a re-check is warranted.
- **[Open]** — unresolved question or risk carried into implementation.

Constraints (from the task, restated for traceability):

1. Preserve the existing Rust decoder and its behavior.
2. Keep new encoder code behind the existing `aac_enc` feature.
3. Keep the stripped C++ encoder as the reference; do not modify production C++ to suit Rust.
4. Do not change the committed M1–M3 implementation or broaden existing numerical tolerances.
5. Preserve the documented expected-divergence policy (`KNOWN_*` pins) and reference fixtures.
6. Do not claim complete AAC encoding until valid streams can be produced and
   independently decoded (that requires a later bitstream milestone, not M4).

## 1. Starting point at `1785529`

**[Fact]** `git log --oneline` top is `1785529 Native Rust AAC-LC encoder foundation +
psychoacoustics (M1-M3 checkpoint)`; `git status` is clean at audit time.

**[Fact]** The migration spec is `docs/aac-enc-migration-spec.md` (§§0–11.5, through the
M3 closeout). The M4–M8 milestones there are still the operative roadmap; this document
is the detailed M4 (quantization/rate-control) audit the spec defers.

**[Fact]** Rust encoder surface at `1785529` (all behind `aac_enc`, re-exported from
`rust/src/aac_enc.rs:108-122`):

- `config.rs` — stripped VBR-5 parameter plane (`VBR_5_QUAL_FACTOR = 0.070`,
  `VBR_5_BANDWIDTH_HZ = 19293`, nominal bitrates, delay 1600).
- `channel_map.rs`, `adts.rs` — modes/elements, 7-byte ADTS header writer.
- `pcm.rs`, `block_switch.rs`, `transform.rs` — M2 frontend.
- `sfb.rs`, `sfb_tables.rs`, `energy.rs`, `thresholds.rs`, `tns.rs`, `psy.rs`,
  `q31.rs`, `testdata.rs` — M3 psychoacoustics (SFB tables, integer-lane energies,
  rescale shifts, exact TNS merge + float Schur, thresholds/spreading/pre-echo,
  MS energies, short grouping).
- `tools/enc-oracle/` — test-only instrumented reference driver (`enc_oracle.cpp`,
  `make_fixtures.py`, `extract_testdata.py`, `extract_tables.py`, `README.md`);
  production C++ is unmodified. Committed vectors live in `rust/src/aac_enc/testdata/`.

**[Fact]** What M4 does NOT have yet: no `quantize`, `scf/sf_estim`, `pe`, `vbr_threshold`,
`bitcount`, `qc/rate` modules; no `SECTION_DATA`, `QC_STATE`, `ATS_ELEMENT` equivalents;
no afterburner search; no granted/used bit accounting. The crate cannot produce AAC yet,
and this plan does not change that.

## 2. Reference call graph (VBR-5, per frame)

Read top-down from `FDKaacEnc_EncodeFrame`. All line numbers are at `1785529`.

### 2.1 Frame driver — `cpp/libAACenc/src/aacenc.cpp:769-998`

**[Fact]** `FDKaacEnc_EncodeFrame` (`aacenc.cpp:769`):

1. Per audio element (`:790-862`): alias QC buffers into psy (`psyOutChan->mdctSpectrum
   = qcOutChan->mdctSpectrum`, etc., `:798-808`), then:
   - `FDKaacEnc_psyMain` (`:811`, `psy_main.cpp:407`) — M1–M3 territory.
   - `FDKaacEnc_QCMainPrepare` (`:820`, `qc_main.cpp:435`) — form factor, PE,
     static-bit estimate. Accumulates `staticBits`, `elementExtBits` (`:856-857`) and
     `totalNoRedPe` (`:860`).
2. Global extensions: `globalExtBits = 0 + EL_ID_BITS (3)` when not scalable/ER (`:864-916`).
3. `FDKaacEnc_AdjustBitrate` (`:929`, `qc_main.cpp:469`): padding + `calcFrameLen` →
   `avgTotalBits`; scaled by `nSubFrames (=1)` (`:933`).
4. `globHdrBits = transportEnc_GetStaticBits(hTpEnc, avgTotalBits + bitResTot)` (`:940`).
5. `FDKaacEnc_QCMain` (`:947`, `qc_main.cpp:788`) — §2.3. The whole quantization /
   rate-control search lives here.
6. `FDKaacEnc_updateFillBits` (`:955`, `qc_main.cpp:1168`) — VBR fill/align pre-pass.
7. `FDKaacEnc_FinalizeBitConsumption` (`:960`, `qc_main.cpp:1283`) — totals, header
   reconciliation, fill/align split, max/min frame check.
8. `FDKaacEnc_updateBitres` (`:968`, `qc_main.cpp:1249`) — VBR branch pins reservoir full.
9. `transportEnc_WriteAccessUnit` + `FDKaacEnc_WriteBitstream` + `transportEnc_GetFrame`
   (`:974-992`, `bitenc.cpp:1182`) — bitstream assembly. **Out of M4 scope** except as a
   consumer interface (see §6.5).

### 2.2 Prepare — `FDKaacEnc_QCMainPrepare` (`qc_main.cpp:435-459`)

**[Fact]** Per element, in order:

1. `FDKaacEnc_CalcFormFactor(qcOutChannel, psyOutChannel, nChannels)` (`:446`,
   `sf_estim.cpp:166`) → per-channel `FDKaacEnc_CalcFormFactorChannel` (`:132`):
   `formFactor = sum(sqrt(|spec|)) >> FORM_FAC_SHIFT`, then `CalcLdData` → 
   `sfbFormFactorLdData`. Zero-spec bands above `maxSfbPerGroup` get `-1.0` sentinel.
2. `FDKaacEnc_peCalculation(&qcElement->peData, ...)` (`:450`, `adj_thr.cpp:908`):
   `preparePe` (`adj_thr.cpp:719` → `prepareSfbPe`, `line_pe.cpp:116`) →
   `calcWeighting` (`adj_thr.cpp:755`, enFac patch, `usePatchTool=1`) → in-place
   weighting (`sfbWeightedEnergy = sfbEnergy - sfbEnFac`,
   `sfbThreshold -= sfbEnFac`, `:925-939`) → `calcPe` (`:885` → `calcSfbPe`,
   `line_pe.cpp:161`). Inputs are the **post-TNS, post-grouping** energies/thresholds;
   outputs are `PE_DATA { pe, constPart, nActiveLines, offset }` per element.
3. `FDKaacEnc_ChannelElementWrite(NULL, ...)` (`:454`, `bitenc.cpp:918` with
   `hTpEnc=NULL`): counts `staticBitsUsed` without writing. **[Inference]** Static here
   means element header + ICS + MS + TNS + global-gain/side-info scaffolding; dynamic
   (section/Huffman/scf payload) is counted later by `dynBitCount`. The exact static
   partition must be confirmed line-by-line in `bitenc.cpp:997-1180` during S1 (see §11,
   Open Q6).

### 2.3 Main — `FDKaacEnc_QCMain` (`qc_main.cpp:788-1117`)

**[Fact]** With `nSubFrames = 1` in this build and `isCBRAdjustment = 0` for VBR-5 with a
full reservoir (`qc_main.cpp:798-801`; bitResMode derivation `aacenc.cpp:644-651` —
see Open Q1):

1. `FDKaacEnc_BitResRedistribution` (`:805`, `qc_main.cpp:738`): per-element
   `bitResLevelEl`/`maxBitResBitsEl` from `relativeBitsEl`. For VBR the driver passes
   `maxBitsPerFrame` rather than `avgTotalBits`.
2. `FDKaacEnc_prepareBitDistribution` (`:836`, `qc_main.cpp:615`):
   - `grantedDynBits = (min(maxBits, avgTotal) - globHdrBits) & ~7 - globalExt -
     static - elementExt` (`:621-624`); `maxDynBits` likewise from `maxBitsPerFrame`.
   - `FDKaacEnc_distributeElementDynBits` (`:638`, `:504`): `grantedDynBits` split by
     `relativeBitsEl` (`fMultI`), with min/max-element rounding compensation (`:524-547`).
   - Per element `FDKaacEnc_DistributeBits` (`:655`, `adj_thr.cpp:2723`): PE-domain grant
     `grantedPe` via `bits2pe` + `bitresCalcBitFac` (FULL-reservoir branch), then
     `grantedPeCorr` via `peCorrectionFactor` (`:2790-2793`); stashes `peLast`,
     resets `dynBitsLast = -1` (`:2796-2797`).
3. `FDKaacEnc_AdjustThresholds` (`qc_main.cpp:848`, `adj_thr.cpp:2804`): with
   `CBRbitrateMode == 0` takes the **VBR branch** (`adj_thr.cpp:2869-2883`) →
   per-element `FDKaacEnc_AdaptThresholdsVBR` (`:2160`): `calcThreshExp` (`:443`) +
   `adaptMinSnr` (`:466`) + `initAvoidHoleFlag` (`:539`) +
   `reduceThresholdsVBR` (`:1108`, chaos + `redVal` from `vbrQualFactor`) — then the
   shared enFac add `sfbThresholdLdData += sfbEnFacLd` (`:2884-2899`).
4. `FDKaacEnc_EstimateScaleFactors` (`qc_main.cpp:870`, `sf_estim.cpp:1279`) per
   element/channel with `invQuant = 2`, `dZoneQuantEnable` (LC: 0 — see §3):
   initial formula scf → `improveScf` (`:378`, true quant/invQuant/dist search) →
   `assimilateSingleScf` (`:458`) → `assimilateMultipleScf` (`:629`) →
   `assimilateMultipleScf2` (`:758`) → min/max clamp (`MAX_SCF_DELTA = 60`) →
   globalGain inversion (`maxSf - scf`; all-`FDK_INT_MIN` → gain 0 + zeroed spectrum,
   `sf_estim.cpp:1202-1273`).
5. Quantization loop (`qc_main.cpp:901-1107`, `do…while(!quantizationDone)`):
   - `FDKaacEnc_reduceBitConsumption` (`:919`, `:1119`) on retry: `globalGain +=
     gainAdjustment` per failing channel, or crash recovery at `maxIterations (=5`
     for non-LD, `aacenc.cpp:727-731`).
   - `FDKaacEnc_QuantizeSpectrum` (`:951`, `quantize.cpp:278`) per channel with
     `QSS = globalGain - scf`.
   - `FDKaacEnc_calcMaxValueInSfb` (`:958`, `:1219`): any band max `> MAX_QUANT
     (8191)` fails the channel and forces `decreaseBitConsumption = 1` (`:962-967`).
   - `FDKaacEnc_dynBitCount` (`:994`, `dyn_bits.cpp:617`): per-channel sectioning +
     Huffman/scf/noise counts → `SECTION_DATA`; sums to `dynBitsUsed`.
   - Element cap: `dynBitsUsed > nChannels*MIN_BUFSIZE_PER_EFF_CHAN - static - ext`
     fails the element (`:1013-1018`; `MIN_BUFSIZE_PER_EFF_CHAN = 6144`,
     `aacenc.h:111`).
   - Frame exit logic (`:1030-1107`): `getTotalConsumedDynBits/Bits`, `quantizationDone`,
     `decreaseBitConsumption` direction, `maxDynBits` overshoot, `avgTotalDynBits`
     (0 in this path — `prepareBitDistribution` sets `*avgTotalDynBits = 0`, `:641`),
     `checkMinFrameBitsDemand` (stub returning 1, `:571-575`).
6. `FDKaacEnc_crashRecovery` (`:1148`, `:1395`): brute-force HF zeroing from
   `maxSfbPerGroup-1` down, section `sfbCnt--` + `bitsPerScf` savings via
   `countValues`, side-info reclaim (`sideInfoTabLong/Short`), `maxSfbPerGroup`
   lowering, TNS/tools clear at zero, static/grant re-balance via a fresh
   `ChannelElementWrite(NULL)` (`:1484`).

### 2.4 Post — fill, finalize, reservoir (`qc_main.cpp:1168-1383,1249-1274`)

**[Fact]**

- `FDKaacEnc_updateFillBits` VBR branch (`:1178-1191`): `totFillBits =
  (granted - used) & 7` (byte alignment) + `(max(0, minBits - total) + 7) & ~7`
  (min-frame pad; `minBits = 0` for VBR-5, `aacenc.cpp:590-592`).
- `FDKaacEnc_FinalizeBitConsumption` (`:1283`): `totalBits = static + usedDyn +
  totFill + elementExt + globalExt`; the CBR-only transport-correction block
  (`:1295-1341`) is skipped for VBR; `globHdrBits` refreshed (`:1344`); fill payload
  sized through `writeExtensionData(NULL)` (`:1357`); `alignBits = 7 - (static +
  usedDyn + elementExt + totFill + globalExt - 1) % 8` (`:1361-1364`); 8-bit dedup
  (`:1367-1369`); final range check `totalBits <= maxBitsPerFrame` else
  `AAC_ENC_QUANT_ERROR` (`:1375-1378`).
- `FDKaacEnc_updateBitres` VBR branch (`:1251-1260`): `bitResTot = min(maxBitsPerFrame,
  bitResTotMax)` — pinned full, no carry. (Contrast CBR `:1265-1271`.)

## 3. VBR-5-specific vs shared AAC-LC logic

**[Fact]** Stripped-build VBR-5 constants (enforced `aacenc_lib.cpp:557-682`,
`aacenc.cpp:716`): AOT LC only, `BITRATEMODE == VBR_5`, afterburner forced on
(`invQuant = 2`), `usePns = 0` forced (`:665-666`), frame length 1024.

| Item | VBR-5-specific | Shared LC | Reference |
|---|---|---|---|
| Quality factor `0.070` (smallest = highest quality) | x | | `qc_main.cpp:122-133,389-397` → `AdjThrInit :2552` |
| Nominal rate mono 112k / stereo 96k per ch (192k stereo, 480k 5.1) | x | | `aacenc.cpp:199-246` |
| Bandwidth 19293 Hz mono+stereo | x | | `bandwidth.cpp:152-162` |
| `grantedDynBits` from `min(maxBits,avgTotal)` + `&~7` | x (formula) | byte-align idiom shared | `qc_main.cpp:621-627` |
| No reservoir carry; fill = align + min-pad only | x | | `qc_main.cpp:1178-1191,1251-1260` |
| `AdaptThresholdsVBR` + `reduceThresholdsVBR` + chaos path | x | (CBR uses `adaptThresholdsToPe` instead) | `adj_thr.cpp:2160,1108,2804-2883` |
| `invQuant == 2` afterburner path (improveScf + 3 assimilations) | x (forced) | routines shared with other invQuant levels | `sf_estim.cpp:1046-1299`, `aacenc.cpp:716` |
| `qualityIdx = 1` column in `bits2Pe` tables | x (setting) | table + interpolation shared | `adj_thr.cpp:354,180-304` |
| `dZoneQuantEnable = 0` for LC (only set when `isLowDelay`, `qc_main.cpp:399-411`) | x (value) | flag plumbing shared | `qc_main.cpp:399-411`, `aacenc.cpp:640` (`isLowDelay=0`) |
| `maxIterations = 5` (non-LD) | (setting, shared code) | x | `aacenc.cpp:727-731` |
| `MAX_QUANT=8191`, `MAX_SCF_DELTA=60`, `|scfDPCM|≤60` | | x | `quantize.h:110`, `sf_estim.cpp:115`, `bit_cnt.h:192-198` |
| Huffman/codebook selection, sectioning, scf DPCM tables | | x | `dyn_bits.cpp`, `bit_cnt.cpp`, `aacEnc_rom.h:122-140` |
| Quantizer formula `spec^(3/4)·2^(−3/16·(QSS−4·scf))` | | x | `quantize.cpp:286-289` |
| `MIN_BUFSIZE_PER_EFF_CHAN=6144`, `INVALID_BITCOUNT`, `&~7` aligns | | x | `aacenc.h:111`, `bit_cnt.h:110` |
| `ChannelElementWrite(NULL)` static counting, `WriteBitstream` element order | | x | `bitenc.cpp:918,1182` |
| MS/IS/TNS machinery (gated, mostly inactive at VBR-5 — §10) | (operating point) | x | `psy_main.cpp:1161-1227` |

## 4. Quantization / rate-control data flow

```text
M3 outputs per channel/window (post-TNS, grouped, rescaled Q31 spectrum + stored
energies/thresholds/spread/MS/grouped vectors; TNS info; window/grouping; SFB offsets)
        │
        ▼
QCMainPrepare (per element) — qc_main.cpp:435
  CalcFormFactor ──► sfbFormFactorLdData[60]            (sf_estim.cpp:166)
  peCalculation ──► PE_DATA {pe,constPart,nActiveLines} (adj_thr.cpp:908; line_pe.cpp)
  ChannelElementWrite(NULL) ──► staticBitsUsed          (bitenc.cpp:918)
        │
        ▼
AdjustBitrate + GetStaticBits ──► avgTotalBits, globHdrBits   (qc_main.cpp:469)
        │
        ▼
QCMain (qc_main.cpp:788)
  BitResRedistribution ──► elementBits[].bitResLevelEl/maxBitResBitsEl
  prepareBitDistribution ──► grantedDynBits/maxDynBits (frame+element),
                             grantedPe/grantedPeCorr per element (adj_thr.cpp:2723)
  AdjustThresholds (VBR) ──► mutated sfbThresholdLdData (+enFac add)
                             + ATS chaosMeasureOld/peMin/peMax/correction state
  EstimateScaleFactors (invQuant=2) ──► scf[60], globalGain, quantSpec[1024]
                             (mutates mdctSpectrum: dead bands zeroed),
                             + sfbDist/sfbConstPePart scratch
  gain loop: QuantizeSpectrum ──► quantSpec
             calcMaxValueInSfb ──► maxValueInSfb[60] (MAX_QUANT gate)
             dynBitCount ──► SECTION_DATA + dynBitsUsed (per ch → element → frame)
             reduceBitConsumption / crashRecovery (globalGain moves, spectrum cuts)
        │
        ▼
updateFillBits (VBR) ──► totFillBits            (qc_main.cpp:1183)
FinalizeBitConsumption ──► totalBits/alignBits   (qc_main.cpp:1283)
updateBitres (VBR: pinned) ──► bitResTot         (qc_main.cpp:1258)
        │
        ▼ (interface to later bitstream milestone; NOT implemented in M4)
WriteBitstream (bitenc.cpp:1182) consumes SECTION_DATA + scf/globalGain/quantSpec
  + TNS/MS/tools + static/dynamic counts; must agree bit-exactly with dynBitCount.
```

**[Fact]** Buffer aliasing: `psyOutChan->mdctSpectrum` points at `qcOutChan->mdctSpectrum`
(`aacenc.cpp:803`); scalefactor estimation **zeroes dead bands in place**
(`sf_estim.cpp:1250-1253,1267-1270`). The Rust port must model this ownership explicitly
(copy-on-write or explicit in/out) rather than hidden aliasing.

**[Fact]** `SECTION_DATA` (`dyn_bits.h:126-139`) is the count→encode contract: produced by
`dynBitCount`, consumed by `bitenc`, with `WRITE_SEC/SCAL/SPEC_ERROR` agreement checks.
M4 must define this struct once and share it with the future bitstream slice.

## 5. Required tables and state

### 5.1 Tables/constants to port (exact values, Q31→float mapping documented per constant)

**[Fact]** All live in `cpp/libAACenc/src/` unless noted:

- Quantizer: `FDKaacEnc_quantTableQ[4]`, `FDKaacEnc_quantTableE[4]`,
  `FDKaacEnc_mTab_3_4[MANT_SIZE]` (encode, `quantize.cpp:121-122,143`);
  `FDKaacEnc_mTab_4_3Elc[512]`, `FDKaacEnc_specExpMantTableCombElc[4][14]`,
  `FDKaacEnc_specExpTableComb[4][14]` (inverse, `:160-162`); dead-zone `k`: `0.23`
  vs `−0.0946+0.5` (`quantize.cpp:125-128`); `MAX_QUANT = 8191` (`quantize.h:110`).
- Scalefactor: `MAX_SCF_DELTA = 60` (`sf_estim.cpp:115`); `FORM_FAC_SHIFT = 6`
  (`sf_estim.h:112`); PE triad `PE_C1/C2/C3` (`:117-121`); `AS_PE_FAC_SHIFT = 7`,
  `DIST_FAC_SHIFT = 3` (`:111-113`); scf bit LUT `FDKaacEnc_huff_ltabscf[121]`
  + `CODE_BOOK_SCF_LAV = 60` (`bit_cnt.h:177,192-198`).
- Noiseless coding: Huffman length tables `huff_ltab1_2/3_4/5_6/7_8/9_10/11`
  and code tables `huff_ctab1..11 + ctabscf` (`aacEnc_rom.h:122-140`); codebook
  numbers/LAVs (`bit_cnt.h:116-179`); section constants `SECT_ESC_VAL_LONG=31`,
  `SECT_ESC_VAL_SHORT=7`, `CODE_BOOK_BITS=4`, `SECT_BITS_LONG/SHORT=5/3`
  (`dyn_bits.h:112-117`); side-info `sideInfoTabLong/Short` (`aacEnc_rom.h:167-168`);
  `INVALID_BITCOUNT = FDK_INT_MAX/4` (`bit_cnt.h:110`).
- Rate/threshold: `tableVbrQualFactor` (`qc_main.cpp:122-133`);
  `bits2PeConfigTab` + six `S_Bits2PeTab{16000…48000}` (`adj_thr.cpp:180-304`);
  `BRES_PARAM` long/short clips/saves/spends (`AdjThrInit :2488-2520`);
  `invInt[8]` (`:108-109`), `invSqrt4` (short redVal path, `:1251-1257`);
  chaos bounds/curve (`calcChaosMeasure :1054`, `reduceThresholdsVBR :1173-1190`,
  `MIN_LDTHRESH = -0.515625`); `Q_AVGBITS = 17` (`:312`);
  `PE_CONSTPART_SHIFT = FRACT_BITS` (`line_pe.h:110`); `LD_DATA_SCALING = 64.0`,
  `LD_DATA_SHIFT = 6` (`fixpoint_math.h:113-114`); `DFRACT_BITS = 32`
  (`common_fix.h:113`).
- Frame accounting: `MIN_BUFSIZE_PER_EFF_CHAN = 6144` (`aacenc.h:111`);
  `EL_ID_BITS = 3` (`bitenc.h:126`); `configTabVBR` (`aacenc.cpp:199-205`);
  `meanPe` formula (`:673-679`); `maxBitFac` 24-bit scaling (`:681-684`);
  `TRANS_FAC = 8`, `MAX_GROUPED_SFB = 60` (`psy_const.h:109,151-154`).
- Already ported in M1–M3 (reuse, do not reduplicate): SFB offsets/active/lowpass,
  mask factors, minSNR, TNS coeff/border tables (see `sfb_tables.rs`, `tns.rs`).

### 5.2 Persistent + per-frame state to model

**[Fact]** Struct definitions: `qc_data.h:114-297`, `adj_thr_data.h:119-173`,
`dyn_bits.h:119-144`, `interface.h:112-162`, `line_pe.h:110-130`.

- `QC_STATE`: `globHdrBits`, `maxBitsPerFrame (=6144·nChEff, VBR-5)`,
  `minBitsPerFrame (=0)`, `bitResTot TotMax` (pinned), `bitrateMode (VBR_5)`,
  `bitResMode`, `invQuant (2)`, `maxIterations (5)`, `vbrQualFactor (0.070)`,
  `maxBitFac`, `dZoneQuantEnable (0)`, `paddingRest`, `elementBits[]`, `hAdjThr`,
  `hBitCounter`. Init: `QCInit` (`qc_main.cpp:358`) from `QC_INIT` built in
  `aacenc.cpp:579-738`.
- `ATS_ELEMENT` (per element, evolves across frames): `peMin/peMax`,
  `bits2PeFactor_{m,e}`, `peCorrectionFactor_{m,e}`, `peLast`, `dynBitsLast`,
  `vbrQualFactor`, `chaosMeasureOld (init 0.3)`, `chaosMeasureEnFac/lastEnFacPatch`,
  `ahParam {modifyMinSnr, startSfbL/S (15/3 at VBR-5 rates)}`, `minSnrAdaptParam`,
  `peOffset` (0 at VBR-5 rates; `chBitrate ≥ 32000` skips the `<32000` branch,
  `adj_thr.cpp:2553`). `ADJ_THR_STATE`: `bresParamLong/Short`,
  `bitDistributionMode` (mono/stereo → INTRA per `aacenc.cpp:653-671`),
  `maxIter2ndGuess` (1 for ≤2ch non-LD; CBR-only use).
- `QC_OUT_CHANNEL[1024/60-arrays]`: `mdctSpectrum`, `quantSpec`, `maxValueInSfb`,
  `scf`, `globalGain`, `sectionData`, `sfbFormFactorLdData`, `sfbThreshold/Energy/
  MinSnr/WeightedEnergy/EnFac/SpreadEnergy (+Ld variants)`. `QC_OUT_ELEMENT`:
  `staticBitsUsed/dynBitsUsed/extBitsUsed`, `grantedDynBits`, `grantedPe/PeCorr`,
  `peData`, channels, `dynMem_*` scratch. `QC_OUT`: `granted/maxDyn/usedDyn`,
  `staticBits`, `totFillBits/alignBits/totalBits`, `totalGrantedPeCorr/totalNoRedPe`.
- `PE_DATA/PE_CHANNEL_DATA` (`line_pe.h`, `adj_thr_data.h`): `sfbNLines/sfbPe/
  sfbConstPart/sfbNActiveLines`, element `pe/constPart/nActiveLines/offset`.
- `SECTION_DATA/SECTION_INFO` + `BITCNTR_STATE` (`dyn_bits.h`): section codebooks,
  starts/counts/bits, `sideInfoBits/huffmanBits/scalefacBits/noiseNrgBits`,
  `firstScf`, block/group geometry.

## 6. Proposed Rust module boundaries

**[Inference]** All new modules live in `rust/src/aac_enc/`, gated `#[cfg(feature =
"aac_enc")]`, following the M1–M3 pattern. No decoder or `common/` edits (reuse by call).

| Proposed module | Ports (reference) | Owns |
|---|---|---|
| `pe.rs` | `prepareSfbPe`/`calcSfbPe` (`line_pe.cpp:116,161`), `preparePe/calcWeighting/calcPe/peCalculation` (`adj_thr.cpp:719,755,885,908`) | `PeData`, form-factor→PE inputs, enFac weighting; pure functions over M3 vectors |
| `threshold_vbr.rs` | `calcThreshExp/adaptMinSnr/initAvoidHoleFlag` (`:443,466,539`), `calcChaosMeasure/reduceThresholdsVBR/AdaptThresholdsVBR` (`:1054,1108,2160`), enFac add (`AdjustThresholds :2884-2899`) | VBR threshold mutation + `chaosMeasureOld` evolution; ATS slice |
| `quant.rs` | `quantizeLines/invQuantizeLines/QuantizeSpectrum/calcSfbDist/calcSfbQuantEnergyAndDist` (`quantize.cpp:116,180,278,312,361`) + tables | Scalar quantizer + inverse + distortion; table ports from `aacEnc_rom` |
| `scf.rs` | `CalcFormFactor*` + `EstimateScaleFactors*` + `improveScf` + 3 assimilations (`sf_estim.cpp:132,166,378,458,629,758,1046,1279`) | Afterburner search (`invQuant==2` only); scf/globalGain/quantSpec/dist scratch |
| `bitcount.rs` | `dynBitCount/noiselessCounter/scfCount/noiseCount` (`dyn_bits.cpp:617,343,467,589`) + `bitCount/countValues` (`bit_cnt.cpp:543,560`) + codebook/LAV consts | `SectionData/SectionInfo`, exact integer counts; `codeValues` *counting* half only (encode half deferred) |
| `rate.rs` (`qc.rs`) | `QCInit/AdjustBitrate/BitResRedistribution/prepareBitDistribution/DistributeBits/bitresCalcBitFac/peCorrections` + QCMain loop + `reduceBitConsumption/crashRecovery/calcMaxValueInSfb` + `updateFillBits/FinalizeBitConsumption/updateBitres` | `QcState/QcOut/...`, ATS/bit-counter handles, granted/used/fill/align accounting; owns the §2.3 orchestration |

Supporting changes:

- `config.rs` (extend, not rewrite): derive `averageBits`, `maxBitsPerFrame`,
  `bitResTotMax`, `meanPe`, `maxBitFac`, `bitResMode`, `bitDistributionMode` per
  `aacenc.cpp:579-738` so S6 has exact init vectors. **[Fact]** Current `config.rs`
  already pins nominal bitrate/qual/bandwidth/delay; the QC-init derivations are the
  missing interface.
- `psy.rs` output contract (extend): M4 needs PosTNS grouped `mdctSpectrum` +
  `sfbEnergy/Threshold/MinSnr/Spread +Ld`, `isBook/isScale/noiseNrg` (all-zero at
  VBR-5/PNS-off — assert, don't assume), `msDigest/msMask`, `commonWindow`,
  `maxSfbPerGroup/sfbCnt/sfbPerGroup/offsets`, `mdctScale`. **[Fact]** Most exist as
  `ChannelOutput/GroupedOutput/MsOutput` or oracle vectors; `isBook/isScale/noiseNrg`
  need explicit zero-plumbing with tests (see Open Q5).
- Bitstream seam (§6.5): define `SectionData` + a `StaticBitsEstimator` trait in
  `bitcount.rs` whose NULL-mode implementation mirrors `ChannelElementWrite(NULL)`
  bit-for-bit; the real writer arrives in a later milestone and must consume the same
  struct without conversion.

### 6.5 Bitstream-assembly interface (dependency, not implementation)

**[Fact]** `WriteBitstream` (`bitenc.cpp:1182`) and `ChannelElementWrite` (`:918`) consume
`SECTION_DATA`, `scf`, `globalGain`, `quantSpec`, `maxValueInSfb`, TNS/MS/tools, and the
static/dynamic counts. M4 must therefore export those values in C-compatible order and
assert `dynBitCount`↔future-writer agreement hooks (count functions shared, not forked).
No `FDKwriteBits`/transport calls in M4; syntax-validity claims wait for that milestone
(per task constraint).

## 7. Oracle and fixture strategy

### 7.1 Oracle extensions (test-only, production C++ untouched)

Extend `tools/enc-oracle/enc_oracle.cpp` with a `--qc` stage that reuses the already-linked
objects to dump, per frame/element/channel, in parseable `TAG …` lines:

1. `FF <formFactorLd[active]>` after `CalcFormFactor` (validates S-scoped port).
2. `PE <pe constPart nActive offset> + PECH <per-ch pe>` after `peCalculation`.
3. `GRANT <grantedDynBits maxDynBits grantedPe grantedPeCorr totalGrantedPeCorr>` after
   `prepareBitDistribution`/`DistributeBits`.
4. `THRV <thresholdLd[active] post-VBR>` + `CHAOS <chaos measureOld>` after
   `AdaptThresholdsVBR` (+ enFac add), so S2 pins decisions before scf search.
5. `SCF <globalGain> <scf[active]>` + `QMAX <maxValueInSfb[active]>` +
   `QSP <quantSpec checksum + sparse dump>` after `EstimateScaleFactors` and after each
   gain-loop iteration (iteration index tagged).
6. `SECT <noOfSections {codeBook,sfbStart,sfbCnt,sectionBits}… sideInfo/huffman/
   scalefac/noiseNrg/firstScf>` + `DYNU <dynBitsUsed>` after `dynBitCount`.
7. `FRAMEBITS <static usedDyn totFill align total granted>` +
   `BITRES <bitResTot bitResTotMax>` after finalize/update.

**[Fact]** Existing oracle precedent: deterministic dumps with header/GSPEC exclusions,
`--configs` init vectors, `extract_testdata.py` harvesting into
`rust/src/aac_enc/testdata/`. Follow it: one new extractor pass, committed small vectors
(checksums + sparse arrays, never full 1024-line dumps except 2–3 anchor frames).

Determinism rules (carry over from M2/M3): oracle re-runs must be byte-identical over
all committed lines modulo the documented header/heap exclusions; any nondeterminism is
an oracle bug, not tolerance.

### 7.2 Fixture matrix (small, targeted; reuse M2/M3 wavs where possible)

Prefer the committed M2/M3 stems (silence, sine, impulse, onset, stepdown, click, edge,
sweep, multitone, noise, tonal-noise, quiet-sine, 44100 variants) plus 2–3 new 0.2–1 s
synthetic wavs only where a boundary is otherwise uncovered:

| # | Fixture (16-bit WAV, hashed, in-repo) | Exposes |
|---|---|---|
| F1 | silence stereo (reuse) | all-zero path: `FDK_INT_MIN` scf → gain 0, zeroed spectrum, `SECT` empty, fill=min-pad |
| F2 | 1 kHz sine stereo (reuse) | few active bands; initial-formula scf + `improveScf` NMR≈1.25 branch both sides |
| F3 | impulse/click mono (reuse) | short-block VBR `redVal[group]` path, chaos pinned 0.5, grouped scf geometry |
| F4 | onset transient stereo (reuse) | long→short chaos transition, `chaosMeasureOld` evolution across frames |
| F5 | stereo MS-correlated pair (reuse multitone) | `calcWeighting` MS-coupled patch + `msDigest/msMask` plumbing into PE |
| F6 | full-scale synthetic stress (new, 0.5 s, deterministic PRNG + tone stack) | `MAX_QUANT` overflow → gain retry; `MIN_BUFSIZE` element cap; `maxIterations=5` → `crashRecovery` HF cut + `maxSfbPerGroup` drop |
| F7 | scf-delta stress (new, sparse multi-tone at SFB edges) | `MAX_SCF_DELTA=60` clamp + `|Δscf|≤60` DPCM gate; assimilation region merges |

Frame selection: pin F1–F5 on the same frames the M3 survey already covers (onset f22–f26,
sine f5, click f11, stepdown f0–f2, sweep f10, silence f5) so M3 `KNOWN_*` pins stay
interpretable; add F6–F7 frames that trip each gate at least once (proved by oracle dump,
not by assertion-text).

### 7.3 Comparison policy (no tolerance broadening)

- Integer decisions exact: `scf`, `globalGain`, section `codeBook/sfbStart/sfbCnt`,
  `maxSfbPerGroup`, `firstScf`, `totFillBits/alignBits`, bit counts, `granted/used`
  bits. Any mismatch fails loudly.
- Values in Ld/Q31 domains use the existing M3 gates only (e.g. TNS gain gate
  `8+gain/1000`); **no new float tolerance without a same-input residual study** of the
  kind in spec §11.2. New `KNOWN_*` pins require both sides recorded + cause tag, and
  the tamper-verification the M3 closeout used.
- `quantSpec` compared exactly (i16 vectors); `quantSpec` checksums never substitute for
  full comparison on anchor frames.

## 8. Numeric-representation differences that could affect parity

**[Fact]** Rust is float-native (`f32/f64`, zero `FIXP` hits under `rust/src`); C++ is
Q31 (`FIXP_DBL`, `common_fix.h`, `fixpoint_math.h`). Preserve mathematics and decisions,
not bit patterns (spec §5). Concretely for M4:

1. **Constant mapping.** Every `FL2FXCONST_DBL(x)` must be ported as the Q31-rounded
   value (`x·2³¹` rounded, cf. dead-zone `0.23` / `0.4054`, `vbrQualFactor 0.070`,
   `PE_C1/C2/C3`, chaos curve constants). Document the Q31 integer alongside the float.
2. **Log domains.** `CalcLdData/CalcInvLdData` are `log2·64` / inverse with fixed-point
   rounding; `LD_DATA` floor sentinels (`-1.0`, `MIN_LDTHRESH -0.515625`) are exact
   decision inputs. Port as `log2/pow2` in `f64` with explicit sentinel handling; never
   let `log(0)` or denormal paths diverge silently.
3. **Quantizer fixed-point chain.** `quantizeLines` (`quantize.cpp:116-169`) is a
   normalize→`mTab_3_4`→`quantTableE`→shift→`k`-add pipeline with `CntLeadingZeros`
   normalization and `fMultDiv2` truncation. The float port must reproduce the
   **integer `quantSpec`** (including sign handling and the `k` dead zone), validated by
   exhaustive table-driven tests (§9, S3), not by SNR alone.
4. **Inverse/distortion asymmetry.** `calcSfbDist` compares `|invQuant| − |spec>>1|`
   (`quantize.cpp:334`) — note the `>>1` — then squares in a renormalized domain before
   `CalcLdData`. The `>>1` and the `MAX_QUANT→0.0` early-out (`:327-329`) are
   load-bearing; port them exactly.
5. **Threshold/chaos fixed-point.** `redVal` shifts (`<< (…>>2)`), `schur_div`,
   `fDivNorm`, `CntLeadingZeros(frameEnergy)` normalization, `invInt/invSqrt4` lookups,
   and the chaos averaging (`0.25·new + 0.75·old`, then `min`, then characteristic
   curve) all quantize the VBR decision. Same-input residual studies must precede any
   gate wider than exact.
6. **PE/counting integers.** `grantedDynBits &~7`, `alignBits` modulo, `INVALID_BITCOUNT`
   saturation, `|Δscf|≤60` assert, `MAX_QUANT`/`MAX_SCF_DELTA` gates, and
   `countValues` Huffman lengths are all exact integers — port bit-exactly; float never
   touches them.
7. **Known M3 lesson.** Upstream f32-MDCT noise already flips discrete TNS decisions at
   Q15 borders (spec §11–11.5). Expect the same at scf/codebook boundaries; the policy
   is pin-and-tag (`KNOWN_*`), never silent tolerance growth.

## 9. Incremental implementation slices

Each slice lists scope, reference comparisons, tests, and acceptance. Slices are
sequential; no slice writes bitstream output.

### S1 — PE + form factor + static-bit estimator (no decisions yet)

- Scope: `pe.rs` (`prepareSfbPe/calcSfbPe/preparePe/calcWeighting/calcPe`) +
  `CalcFormFactor(Channel)` + NULL-mode `StaticBitsEstimator` skeleton over
  `bitenc.cpp:918-997` helpers used in the counting path. Reuse M3 vectors as inputs.
- Compares: oracle `FF`, `PE/PECH`, `staticBitsUsed` per element on F1–F5 (exact ints;
  PE `pe/constPart` exact — they are integer-shifted fixed-point accumulations).
- Tests: unit (single-SFB PE formula both `ldRatio ≷ C1` branches; scf-DPCM cost
  helper), corpus (all F1–F5 framesNW exact).
- Accepts: PE/form-factor/static agree exactly on F1–F5; `isBook/isScale/noiseNrg`
  zero-plumbing asserted (not assumed); open Q5/Q6 either closed or carried explicitly.

### S2 — VBR threshold adaptation (deterministic given S1)

- Scope: `threshold_vbr.rs` (`calcThreshExp/adaptMinSnr/initAvoidHoleFlag/
  reduceThresholdsVBR/AdaptThresholdsVBR` + enFac add). Per-element ATS slice with
  `chaosMeasureOld` persistence; `InitBits2PeFactor qualityIdx=1` values as init
  vectors (no `DistributeBits` iteration yet — grants arrive as oracle inputs).
- Compares: `THRV`, `CHAOS` per frame/element on F1–F5, long + short (redVal long vs
  per-group), chaos-evolution frames (F4 onset) across 3+ consecutive frames.
- Tests: chaos unit (synthetic `sfbEnergy/FormFactor` with hand-computed measure);
  redVal long/short vectors; corpus exact `THRV`.
- Accepts: thresholds post-VBR exact on F1–F5; `chaosMeasureOld` trajectory exact;
  any float residual gets a §11.2-style study before any gate — default is exact.

### S3 — Scalar quantizer primitives (table-accurate integers)

- Scope: `quant.rs`: port `quantTableQ/E`, `mTab_3_4`, `mTab_4_3Elc`,
  `specExpMantTableCombElc/specExpTableComb` from `aacEnc_rom`; implement
  `quantizeLines/invQuantizeLines/QuantizeSpectrum/calcSfbDist/
  calcSfbQuantEnergyAndDist` with dead-zone `k` selection by `dZoneQuantEnable (=0)`.
- Compares: exhaustive `quantSpec` vectors over gain × amplitude grid against oracle
  `QSP` probes (exact i16); `calcSfbDist` Ld values within the pre-registered gate
  (exact preferred; any gate needs residual evidence); round-trip
  `inverse_quantize_band` property vs existing `common/quantize.rs` tests.
- Tests: boundary (`|q| = 8191/8192` → `MAX_QUANT` early-out `0.0`); sign symmetry;
  `spec>>1` asymmetry pinned; zero-line fast path.
- Accepts: bit-exact `quantSpec` on the grid + F2/F6 anchor frames; distortion gate
  justified by measurement, not assumed.

### S4 — Afterburner scalefactor estimation (`invQuant == 2` only)

- Scope: `scf.rs`: initial-formula scf (`sf_estim.cpp:1093-1167`, incl.
  `minSfMaxQuant` via `CalcLdData(maxSpec)` + `c1Const`), `improveScf` (both NMR
  branches, `UPCOUNT_LIMIT=1`), `assimilateSingleScf` (restart-on-success),
  `assimilateMultipleScf`, `assimilateMultipleScf2` (all three sub-steps), clamp +
  globalGain inversion + dead-band zeroing. Only the `invQuant>1` path; `invQuant==0`
  deleted, `==1` explicitly rejected (unreachable in this build).
- Compares: oracle `SCF/QMAX/QSP` after estimation on F1–F5 + F7 (exact `scf`,
  `globalGain`, `quantSpec`); per-assimilation checkpoints on 2 anchor frames to
  localize divergences (temporary probe dumps, not committed).
- Tests: `FDK_INT_MIN` band handling; `MAX_SCF_DELTA` clamp; all-zero → gain-0 case
  (F1); `minSfMaxQuant` ceiling unit tests; assimilation PE-gate
  `10.0/2^14 = 0.0006103515625` pinned.
- Accepts: exact scf/gain/quant agreement on F1–F5 + F7; F6 stress frames either exact
  or `KNOWN_*`-pinned with cause tags (no silent passes).

### S5 — Noiseless bit counting + sectioning (exact integers)

- Scope: `bitcount.rs`: `bitCount/countValues` + all `count*` kernels,
  `noiselessCounter` (greedy merge stages `gmStage0/1/2`, merge-gain LUT),
  `scfCount` (DPCM + `huff_ltabscf`), `noiseCount` (no-op at PNS-off — asserted),
  `dynBitCount` assembling `SectionData`. Port Huffman length tables
  (`huff_ltab*`) exactly; code tables (`huff_ctab*`) deferred to bitstream milestone
  except for shared-definition review.
- Compares: oracle `SECT/DYNU` on all fixtures (exact section geometry + counts);
  `INVALID_BITCOUNT` saturation case covered by F6; `firstScf` + side-info bits vs
  `sideInfoTabLong/Short`.
- Tests: per-codebook unit vectors (0, ±1, ESC, zero-run); merge-gain greedy
  regression on crafted spectra; scf-DPCM `±60` boundary.
- Accepts: bit-exact counts and sectioning on F1–F7; count↔future-encode sharing
  (single table source) reviewed and recorded.

### S6 — QC loop + VBR rate-control closure (no bitstream writes)

- Scope: `rate.rs`: `QCInit` derivations (`config.rs` extension), `AdjustBitrate`,
  `BitResRedistribution`, `prepareBitDistribution` + `DistributeBits` (FULL-reservoir
  branch + both pe-correction paths, selected by `bitResMode`), gain loop with
  `reduceBitConsumption`/`crashRecovery`/`calcMaxValueInSfb`, `updateFillBits` (VBR),
  `FinalizeBitConsumption` (VBR path), `updateBitres` (VBR pin). Terminates with
  `totalBits/alignBits/granted/used` + all per-element artifacts; hands
  `SectionData/scf/gain/quant/TNS/MS` to the future writer interface.
- Compares: oracle `GRANT/FRAMEBITS/BITRES` + iteration trace (`SCF/QMAX` per gain
  iteration, crash-recovery `maxSfbPerGroup` drops) on F1–F7.
- Tests: convergence (F6 trips retry + recovery at least once); `MIN_BUFSIZE` cap;
  `maxDynBits` overshoot forcing `decreaseBitConsumption=1`; `quantizationDone` exit
  matrix (under/over/zero-consumption).
- Accepts (M4 exit — see §10).

## 10. Acceptance criteria and known risks

### M4 exit criteria (all must hold; none implies a complete encoder)

1. S1–S6 accepted in order; `cargo test --features aac_enc` green except the two
   pre-existing/accepted failures/pins: decoder `tp_dec::pce::tests::read` and the
   M3 `KNOWN_*` entries (unchanged, unbroadened).
2. Oracle `--qc` re-runs byte-identical (modulo documented exclusions); extractor
   outputs committed; any `KNOWN_*` additions follow the M3 tamper-verified policy.
3. Frame-accounting exactness: `grantedDynBits/usedDynBits/staticBits/totFillBits/
   alignBits/totalBits/bitResTot` match the oracle integer-for-integer on F1–F7.
4. Decision exactness: `scf/globalGain/SECTION_DATA/maxSfbPerGroup/firstScf` match;
   any deviation is a `KNOWN_*` entry with both sides + cause tag, or a hard failure.
5. No bitstream bytes are produced or claimed; validity/decodability claims wait for
   the bitstream milestone (FFmpeg + C + Rust decode of the full corpus, SNR/SDR +
   bitrate-distribution gates per spec §6).
6. Docs updated: this plan + spec §7 checkboxes for the completed slices; ADTS/M3
   vectors untouched.

### Known risks (carried, not resolved here)

- R1 — Float-vs-fixed compounding through threshold→scf→quant→count feedback (§8).
  Mitigation: S1–S5 exact-integer gates before S6 closes the loop; full-chain quality
  gates deferred to M7 per spec.
- R2 — VBR quality-factor non-monotonicity (0.070 vs 0.176) is replicated, never
  "fixed" (spec §8.2).
- R3 — Afterburner search-order sensitivity: assimilations mutate shared
  `quantSpec/sfbDist/sfbConstPePart` in place with restart-on-success (single) vs
  single-pass region sweeps (multiple). Port order verbatim; any "equivalent"
  reordering needs oracle proof.
- R4 — Crash-recovery rarity at VBR-5 may leave it under-covered; F6 exists to force
  it, but real-music coverage waits for M7.
- R5 — Legal: FDK license + AAC patent notes (spec §0/§8.6) ride along unchanged.

## 11. Unresolved questions

- Q1 — `bitResMode` at VBR-5. **[Fact]** `aacenc.cpp:644-651` derives FULL/REDUCED/
  DISABLED from `maxBits − averageBits`; **[Inference]** VBR-5's
  `maxBits = 6144·nChEff` vs `averageBits ≈ nominal` leaves a large surplus → FULL,
  hence `isCBRAdjustment = 0` and the `DistributeBits` FULL branch. **[Open]** Confirm
  numerically for mono/stereo/5.1 during S6 and record which `peCorrection` path
  executes.
- Q2 — Afterburner + `bits2Pe qualityIdx=1` interaction. **[Fact]** `InitBits2PeFactor`
  selects column 1 when `invQuant != 0` (`adj_thr.cpp:354`); the interpolated table
  values feed `DistributeBits` grants that the afterburner then spends. **[Open]** No
  festivity: port both verbatim, but record measured grant deltas idx0-vs-idx1 on F2
  as a sanity probe in S6.
- Q3 — Which spectrum does scf estimation consume? **[Fact]** Post-TNS, post-grouping,
  rescaled `mdctSpectrum` via the `aacenc.cpp:803` alias; TNS-active frames carry
  filtered spectra + `tnsShift` rescaling, short frames the grouped spectrum
  (`psy_main.cpp` grouping + oracle `GSPEC`). **[Open]** Re-verify the exact handoff
  (grouped vs ungrouped pointer, `maxSfbPerGroup` used) against `psy_main.cpp:1251-1295`
  when wiring S4 inputs — the M3 oracle already dumps both, so this is a wiring check,
  not research.
- Q4 — MS/IS at VBR-5 rates. **[Fact]** `allowIS = useIS && (bitrate/bandwidth < 5)`
  (`psy_configuration.cpp:565`); `usePns = 0` forced; MS gated by `allowMS`
  (`psy_main.cpp:1190-1225`). **[Open]** Instrument `msDigest/msMask/isBook/isScale`
  on the corpus in S1 to prove the operating point (expect MS occasionally active, IS
  at/below boundary) before deciding whether IS ports now or stays stubbed-zero.
- Q5 — `noiseNrg` path with PNS off. **[Fact]** `PnsDetect` early-outs
  (`aacenc_pns.cpp:192-194`); M3 oracle leaves MS buffers zero for mono. **[Open]**
  Prove `noiseNrg == NO_NOISE_PNS` and `isBook/isScale == 0` on every F1–F7 frame in S1
  (assert in code), so `dynBitCount`'s PNS branches stay dead by proof, not hope.
- Q6 — Static/dynamic partition in `ChannelElementWrite(NULL)`. **[Fact]** Docstring
  (`bitenc.h:128-156`) defines the three modes (count static / count minimum / write).
  **[Open]** Enumerate exactly which `encode*` helpers run in NULL mode and which bits
  they contribute (ICS/MS/TNS/global-gain scaffolding vs section payload) during S1;
  the S5/S6 agreement checks depend on this partition.
- Q7 — VBR "targeting" without a target. **[Fact]** VBR-5 sets grants from
  `min(maxBits, avgTotal)` with reservoir pinned full — quality comes from
  `vbrQualFactor` + chaos reduction, not from hitting `avgTotal`. **[Open]** Frame-size
  histogram comparison (C vs Rust) in S6 validates parity, but perceptual quality
  claims still wait for M7; do not conflate the two.
- Q8 — Syntax validity without a writer. **[Fact]** M4 ends at `totalBits/alignBits`
  accounting; `WRITE_SEC/SCAL/SPEC_ERROR` agreement checks live in the writer.
  **[Open]** S6 must still enforce the writer's preconditions (`|q|≤8191`,
  `|Δscf|≤60`, `totalBits≤maxBitsPerFrame`, section geometry within
  `SECT_ESC_VAL` limits) so the later milestone inherits valid inputs.

## Appendix A — Source reference index (all at `1785529`)

Frame/QC: `aacenc.cpp:199-246,295-308,428-760 (init),769-998 (frame)`;
`qc_main.cpp:122-133,358-426 (QCInit),435-459 (prepare),469-489 (AdjustBitrate),
504-550 (distributeElement),588-611 (minStatic),615-674 (prepareBitDist),
738-786 (bitResRedist),788-1117 (QCMain),1119-1166 (reduceBitCons),
1168-1209 (updateFillBits),1219-1240 (calcMaxValue),1249-1274 (updateBitres),
1283-1383 (finalize),1395-1498 (crashRecovery)`;
`qc_data.h:114-297`; `aacenc.h:111`.

Threshold/PE: `adj_thr.cpp:180-304 (bits2Pe tabs),318-427 (InitBits2Pe),
433-437 (bits2pe2),443,466,539,719,755,885,908-944,951,988,1054,1108-1312 (VBR reduce),
1320-1890 (correctThresh…adaptToPe),2160-2194 (AdaptVBR),2218-2431 (bitSave/Spend/
adjustPeMinMax/bitresCalcBitFac),2437-2598 (New/Init),2604-2798 (peCorrections/
DistributeBits),2804-2900 (AdjustThresholds)`; `adj_thr_data.h:119-173`;
`adj_thr.h:116-164`; `line_pe.cpp:109-234`; `line_pe.h:110-130`.

Scalefactor/quant: `sf_estim.cpp:111-121,132-174,183-369,378-452,458-1044,1046-1292`;
`sf_estim.h:112-124`; `quantize.cpp:116-169,180-267,278-301,312-349,361-401`;
`quantize.h:110-127`.

Counting/bitstream: `dyn_bits.cpp:112-663`; `dyn_bits.h:112-160`;
`bit_cnt.cpp:121-950`; `bit_cnt.h:110-200`; `bitenc.cpp:127-918,918-1180,1182-1322`;
`bitenc.h:126-183`; `aacEnc_rom.h:122-215`; `grp_data.cpp:118`;
`interface.h:112-162`; `psy_main.cpp:1125-1298`; `psy_configuration.cpp:565-566`;
`fixpoint_math.h:113-114`; `common_fix.h:112-147`; `psy_const.h:109-154`.

Rust/M3: `rust/src/aac_enc.rs`, `rust/src/aac_enc/{config,channel_map,adts,pcm,
block_switch,transform,energy,sfb,sfb_tables,psy,thresholds,tns,q31,testdata}.rs`,
`rust/src/aac_enc/testdata/*`, `tools/enc-oracle/{enc_oracle.cpp:250-914,
make_fixtures.py,extract_testdata.py,extract_tables.py,README.md}`.

## 12. S1 implementation record (2026-10-14)

Planning §§1–11 stand; this section records what S1 built, measured, and
left open. No S2–S6 code was added; no bitstream bytes are produced.

### 12.1 What S1 implemented

**[Fact]** New/changed files (all encoder-side or test-only tooling):

- `rust/src/aac_enc/pe.rs` — `form_factor` (`sf_estim.cpp:132-174`),
  `prepare_sfb_pe` (`line_pe.cpp:116-150`), `calc_sfb_pe`
  (`line_pe.cpp:161-234`), `calc_weighting` (`adj_thr.cpp:755-879`),
  `pe_calculation` (`adj_thr.cpp:908-944`, with `preparePe`/`calcPe`
  folded in: offset seeding, per-channel prepare, weighting-apply,
  summation), `AtsElement` (S1 ATS subset), `bit_count_scf_delta`
  (`bit_cnt.h:192-198` + `HUFF_LTABSCF` from `aacEnc_rom.cpp:281-292`,
  script-extracted), `PeChannelData`/`PeData`/`PeResult`.
- `rust/src/aac_enc/static_bits.rs` — NULL-mode counting subset of
  `FDKaacEnc_ChannelElementWrite` (`bitenc.cpp:918-1178`): element
  header/tag, `common_window`, `encodeIcsInfo` count
  (`bitenc.cpp:180-227`), `encodeMSInfo` count (`:380-423`),
  `encodeGlobalGain` (8), `encodePulseData`/`encodeGainControlData`/
  `encodeTnsDataPresent` (1 each), `encodeTnsData` NULL branch
  (`:535-574`). Section/scalefactor/spectral contribute 0 by
  construction (`:279-281,366-368,1109-1122`); LFE shares the SCE list.
- `rust/src/aac_enc/q31.rs` (extended) — `calc_ld_data` (Taylor `fLog2`
  with the arch-selected 16-bit `LD_COEFF`), `calc_inv_ld_data` (+
  `EXP2_TAB/W/X`), `calc_ld_int` (+ 193-entry `LD_INT_COEFF`),
  `sqrt_fixp`/`inv_sqrt_norm2` (reuse `sfb_tables::INV_SQRT_TAB`),
  `fmult_norm`, `fmult_i`, `fdiv_norm`/`fdiv_norm_e`, `schur_div`,
  `fixnorm_d`, narrowed `fmult_div2`/`fmult_add_div2`.
- `rust/src/aac_enc/testdata.rs` (extended, test-only) — strict
  `load_s1` for `qc1_<name>_f<F>.txt` (+ `S1Channel`/`S1Frame`).
- `rust/src/aac_enc.rs` — `pub mod pe; pub mod static_bits;`.
- `rust/src/aac_enc/testdata/qc1_*.txt` (16 files, ~270 KB) — committed
  S1 vectors (§12.2).
- `tools/enc-oracle/enc_oracle.cpp` (test-only) — per-frame S1 stage on
  copies (§12.2); existing dumps byte-identical modulo the documented
  pre-existing GSPEC scratch wobble (§12.3).
- `tools/enc-oracle/extract_s1.py` (new), `validate_s1.py` (new),
  `README.md` (S1 format + commands).

Out of scope (unchanged): VBR threshold adaptation, quantization,
scalefactor search/afterburner, sectioning, rate-control orchestration,
bitstream writing, MS/IS *decision* port (their outputs are explicit
S1 inputs), runtime pipeline wiring.

### 12.2 Oracle and fixture coverage

**[Fact]** The S1 stage mirrors `psy_main.cpp:1140-1246` (PNS/IS/MS
decisions) with real C calls on per-frame copies
(`PnsDetect`/`PreProcessPnsChannelPair` early-out at `usePns = 0`;
`IntensityStereoProcessing` with `allowIS` from config;
`MsStereoProcessing` with `allowMS`; `CodePnsChannel` fills
`NO_NOISE_PNS`), then the real `CalcFormFactor`, `peCalculation`
(persistent zeroed `ATS_ELEMENT`, `peOffset = 0` asserted via
`chBitrate >= 32000`) and `ChannelElementWrite(NULL, …, 0, AOT_AAC_LC,
-1, …, 0)`. Copies keep every M2/M3 dump byte-identical (verified by
stripped-diff against the pre-S1 binary on all fixtures).

**[Fact]** Committed S1 set (16 element-frames; `extract_s1.py:S1_PLAN`):
silence-stereo f5; sine-stereo f1/f5; onset-stereo f22/f24/f26;
stepdown-stereo f2; click-mono f11; impulse-mono f5; edge-mono f4;
multitone-stereo f1; sweep-stereo f10; noise-mono f5;
tonal-noise-stereo f5; sine-44100 f5; onset-44100 f22. Covers long/
short/start/stop, mono/stereo, 48/44.1 kHz, MS NONE/SOME/ALL, TNS
absent/present (long order-10 at stepdown f2, short multi-window at
onset f24), EnFac patch armed (noise, tonal-noise, click) vs unarmed,
all-zero PE (silence, onset f22) vs loudest (stepdown f24 PE 5233).

**[Fact]** Branch notes: IS never activates on any fixture frame
(`ISB` all zero corpus-wide) — the `isBook` path is covered by
synthetic dead-store tests instead. `NOISE` is uniformly
`NO_NOISE_PNS` (closes plan Q5 for S1 inputs). `msDigest` takes all
three values on corpus (closes plan Q4 measurement half; the decision
port itself is S2 work). Static/dynamic partition confirmed
line-by-line (§6.5/Q6): NULL mode counts exactly header + tag +
common-window + ICS + MS + global-gain + pulse + TNS-present/data +
gain-control.

**[Fact]** 3-run validation (`validate_s1.py`, clean-room `v0/v1/v2`
runs): every committed S1 output line byte-identical across runs.
`SPEC` wobble (5–10 ints, e.g. positions 854/858/862/1010/1020 on
onset f24) lies entirely inside FF-inactive bands (band 13 of each
short group, `maxSfbPerGroup = 13`) on every affected frame; the
validator asserts this per frame against grouped/long offsets.
`ATS pre` differs from fresh zeros even on silence (`0.1875`
from frames 0–4), confirming state injection over history replay.

### 12.3 Numeric-parity findings (all resolved, no tolerances added)

**[Fact]** Every S1 comparison is bit-exact (integers) — no float gates
exist in S1. Five transcription traps were found by failing tests and
fixed with C-probed evidence:

1. `LDCOEFF_16BIT`: on `__arm__ && __ARM_ARCH_8__`,
   `FDK_archdef.h:178-184` selects 16-bit Taylor coefficients
   (`FL2FXCONST_SGL` widened, low 16 bits zero), not the 32-bit
   `FL2FXCONST_DBL` values. Symptom: `calc_ld_data(0x40000000)` off
   by 72.
2. Mixed-width `fMultDiv2(DBL, SGL)` in `CalcInvLdData`
   (`fixpoint_math.h:252`): generic `fixmuldiv2_DS(a,b) =
   fixmuldiv2_DD(a, b<<16)` — the 10-bit fraction is widened before
   the multiply. Symptom: `calc_inv_ld_data(-1)` off by 64.
3. `line_pe.cpp:113` uses DOUBLE `0.5593573` for `C3LdData`
   (`0x4799051F`), unlike `sf_estim.cpp:121`'s float `PE_C3`
   (`0x47990500`, S4's constant). Symptom: sine f1 ch0 band 50 SPCO
   off by one. Pinned by `s1_pe_constants_match_reference_literals`.
4. `calcSfbPe` intensity branch is a dead store
   (`line_pe.cpp:209-220` overwrites the IS provision with zeros):
   ported as observable zeros with the citation; IS costs live in S5
   `scfCount`. Proven by varying-`isScale` tests.
5. Dual buffer roles in `peCalculation` (`adj_thr.cpp:925-939`):
   `sfbWeightedEnergyLdData` is write-only (zero-init + active
   writes) while `sfbThresholdLdData` mutates in place (full clone +
   active subtracts). Symptom: silence WENLD/WTHLD patterns.
6. `HUFF_LTABSCF` is asymmetric around 0 (index 59 `0x03` vs index 61
   `0x04`); table is script-extracted and length/content asserted.

**[Inference]** `FIXP_DBL`/`LONG` are 32-bit on this target
(`sizeof` probe: 4/4/4), so the i32 wrapping model is faithful;
values stay in 32-bit range by headroom design wherever C relies on
it (same argument as the M3 TNS merge).

Known limitations: S1 functions take Q31 vectors — the float→Q31
mapping stays M2/M3's approximation point (no S1 end-to-end encode
claim); `common_window = 0` and LFE counts are covered by hand-count
unit tests only (no fixtures); short-TNS multi-filter counting is
covered via onset f24 + hand-counted window-3 unit.

### 12.4 Acceptance results

**[Fact]** `cargo test --features aac_enc --lib`: 331 passed
(310 M1–M3 + 21 new S1), 1 failed — the pre-existing decoder
`tp_dec::pce::tests::read` (also the only failure on default
features: 254 passed + same 1). No M1–M3 file was modified; no
tolerance was broadened; no `KNOWN_*` entry was added or altered.
`cargo fmt --check` clean; clippy reports no warnings in S1 files
(remaining warnings are pre-existing in M2/M3 files).
`cargo test` (default) shows decoder behavior untouched.

**[Fact, amended 2026-10-15 for S2]** The S1 corpus grew from 16 to 19
frames (`extract_s1.py:S1_PLAN` + onset f23/f25 + quiet-sine f5; S2
needs their S1 inputs). All three re-validated S1-STABLE under the
unchanged validator; S1 `S1_CASES` in `pe.rs`/`static_bits.rs` now
iterate all 19. Nothing else in §12.1–12.3 changed.

### 12.5 Open questions for S2 (from §11)

- Q4 remainder: port `MsStereoProcessing` + `IntensityStereoProcessing`
  decisions so runtime PE inputs (msDigest/msMask/isBook/isScale) stop
  being oracle-supplied. S1 proves the consumer side exactly.
- Q5 closed for S1 (`noiseNrg` uniformly `NO_NOISE_PNS`); the S5
  `noiseCount` no-op inherits the same proof obligation per frame.
- Q6 closed for the NULL path (exact partition above); the
  count↔encode agreement checks (`WRITE_SEC/SCAL/SPEC_ERROR`) belong
  to the bitstream milestone.
- Q1–Q3, Q7–Q8 untouched by S1 (S6/S7 territory).

## 13. S2 implementation record (2026-10-15)

Planning §§1–12 stand; this section records what S2 built, measured, and
left open. No S3–S6 code was added; no bitstream bytes are produced.

### 13.1 What S2 implemented

**[Fact]** New/changed files (all encoder-side or test-only tooling):

- `rust/src/aac_enc/threshold_vbr.rs` (new) — `calc_thresh_exp`
  (`adj_thr.cpp:443-459`), `adapt_min_snr` (`:466-533`),
  `init_avoid_hole_flag_mono` + `init_avoid_hole_flag_ms`
  (`:539-701`, split for borrow-checking; identical band order),
  `calc_chaos_measure` (`:1054-1105`), `reduce_thresholds_vbr`
  (`:1108-1312`), `adapt_thresholds_vbr` (orchestration mirroring
  `AdaptThresholdsVBR :2160-2194` plus the shared enFac add
  `:2884-2899`); `SfbGeometry`, `S2ChannelInput`, `StereoWork`,
  `AdaptResult`/`ReduceResult`; `INV_INT`/`INV_SQRT4` verbatim tables;
  all Q31 literals C-probed (table in module docs). 19 tests.
- `rust/src/aac_enc/pe.rs` (extended) — `AtsElement` carries the S2
  `ATS_ELEMENT` slice (`chaosMeasureOld`, `vbrQualFactor`, `ahParam`,
  `minSnrAdaptParam`) initialized in `::new` to the exact
  `AdjThrInit` VBR-5 values; S1 behavior unchanged (::new signature
  unchanged, S1 functions never read the new fields).
- `rust/src/aac_enc.rs` — `pub mod threshold_vbr;` + doc line.
- `rust/src/aac_enc/testdata.rs` (extended, test-only) — strict
  `load_s2` for `qc2_<name>_f<F>.txt` (+ `S2Channel`/`S2Frame`).
- `rust/src/aac_enc/testdata/qc2_*.txt` (19 files) — committed S2
  vectors (§13.2); plus 3 new `qc1_*` files (§12.4 amendment).
- `tools/enc-oracle/enc_oracle.cpp` (test-only) — S2 stage calling the
  real `FDKaacEnc_AdjustThresholds` (VBR branch) on identically-
  initialized buffer copies with the persistent `ATS_ELEMENT` now
  fully initialized by the real `FDKaacEnc_AdjThrInit` (VBR-5 point);
  post-MS spread sync-back fix (§13.3, finding 1).
- `tools/enc-oracle/extract_s2.py`, `validate_s2.py` (new);
  `extract_testdata.py` (+S2 skip for M3 compat); `README.md` (S2
  format + commands).

Out of scope (unchanged): CBR path (`adaptThresholdsToPe` etc.),
`DistributeBits`/grants (S6), quantization/scf/sectioning/rate loop
(S3–S6), MS/IS decisions (S1 `MSD` consumed as explicit inputs).

### 13.2 Oracle and fixture coverage

**[Fact]** The S2 stage wires `PSY_OUT_CHANNEL`/`QC_OUT_CHANNEL`
copies from S1 post-state (weighted `WTHLD` thresholds as the psy
threshold input — matching the aliased encoder where `peCalculation`
mutated the shared buffer; `SENL`/`PENLD` unweighted energies;
`MINSNR0` from grouped config (short) or static config (long);
post-MS `SPREAD0`; `groupLen` from block switching) and calls the
real `FDKaacEnc_AdjustThresholds(..., CBRbitrateMode=0, ...)`.
`meanPe` is computed with the real `aacenc.cpp:675-679` formula (it
only feeds CBR `peMin/peMax`, unused on the S2 path — documented in
code). The VBR path never writes psy arrays (verified by reading
every sub-function), so separate identically-initialized copies are
numerically identical to the aliased buffers.

**[Fact]** Committed S2 set (19 element-frames; `extract_s2.py:S2_PLAN`):
all 19 S1 frames. Covers long/short/start/stop (onset f22 LONG, f23
START, f24 SHORT, f25 STOP, f26 LONG), mono/stereo, 48/44.1 kHz, MS
NONE/SOME/ALL masks into the avoid-hole MS section, silence (S2
no-op: THRV==WTHLD, MINSNR/SPREAD unchanged) through stepdown f2 (23
changed bands), chaos pre-states from 156M to 2.1B. The onset f22–f26
consecutive run proves chaos-trajectory continuity (each committed
post == next committed pre; single carried ATS reproduces all five).

**[Fact]** 3-run validation (`validate_s2.py`, same `w0/w1/w2` runs):
**S2-STABLE** — every committed S2 line byte-identical (no wobble
anywhere; S2 inputs derive from S1 vectors + deterministic config).
`validate_s1.py` re-run on the same runs: **S1-STABLE** (S2 stage does
not disturb M2/M3/S1 lines); M3 `extract_testdata.py` output
byte-identical on spot-checked vectors.

**[Fact]** Branch notes: zero-guards (long `redVal >= 1-thrExp`,
short `thrExp <= limit-redVal`) fire on **zero** corpus bands
(verified by scan); they are covered by forced synthetic tests with
condition+outcome proofs instead. 241 active strict AH-skip bands
were first misread as mechanism evidence — corrected: bands past
`maxSfbPerGroup` are never visited by any C loop (scratch, unread),
so the scan was restricted to active bands, leaving 1 ambiguous band
(quiet-sine f5 ch0 b7, gate-open yet unchanged — consistent with an
AH clamp but a single sample). AH mechanism is therefore proven by
the exact skip/clamp pair tests, AH-init classification tests, and
the end-to-end exact match (which is sensitive to every AH decision
on 19 frames). The short `groupNumber = sfb/sfbPerGroup` quirk
(`:1249`, always 0) is ported verbatim and proven by the onset-f24
4-group exact match plus a synthetic identical-inputs-across-groups
test. IS/MS decisions remain oracle inputs (S2 consumes `MSD` only
in the avoid-hole MS section).

### 13.3 Numeric-parity findings (all resolved, no tolerances added)

**[Fact]** Every S2 comparison is bit-exact (integers). Three findings:

1. Post-MS spread sync gap (oracle bug, not a port bug): the S1 MS
   block mutated `s1psyData` spread copies that died with the block
   scope; S1 never dumped spread so nothing caught it. Fixed by
   syncing spread back alongside energies (S1 lines byte-identical
   before/after — verified by S1-STABLE re-run).
2. `CalcInvLdData(...) << 1` parenthesization
   (`calcChaosMeasure :1088-1098`): the `<<1` sits INSIDE the call.
   The port first applied it outside (sine-f5 chaos off by ~6x, caught
   by the corpus test, bisected with a C probe using the real inline
   primitives, fixed, pinned by C-probe anchors
   `s2_chaos_measure_matches_c_probe_anchors`).
3. `FL2FXCONST_DBL(1e-10f)` underflows Q31 to exactly 0 (probe), so
   `frameEnergy` starts at 0 — ported as the literal with the citation.
   `FL2FXCONST_DBL(1.f)` saturates to `MAXVAL` (used for the
   empty-chaos and `1.0 - thrExp` expressions with wrapping
   arithmetic).

**[Inference]** The short `-1.0` guard is effectively dead for all
realizable inputs (minimum `scaled` ≈ 56828 > `limit` 32768 — even
with `thrExp = 0`, which `calc_thresh_exp` can never produce since
`thr>>2` bottoms out above the `CalcInvLdData` zero clamp). Ported
verbatim; proven by construction + corpus exact match (a wrongful
firing would surface as `-1.0`+correction bands).

Known limitations: `thrExp`/`ahFlag` scratch past active bands reads
0 in Rust (garbage and unread in C); `common_window = 0` has no
fixture (S2 stereo sections assume channel-0 geometry like C);
`peMin/peMax`/`bits2Pe` init paths are S6 territory (values computed
but unused on the VBR path).

### 13.4 Acceptance results

**[Fact]** `cargo test --features aac_enc --lib`: 350 passed
(310 M1–M3 + 21 S1 + 19 new S2), 1 failed — the pre-existing decoder
`tp_dec::pce::tests::read` (also the only failure on default
features: 254 passed + same 1). No M1–M3 file was modified (S1
`pe.rs` gained struct fields with unchanged `::new` signature;
S1 `S1_CASES` grew 16→19 with no policy change); no tolerance was
broadened; no `KNOWN_*` entry was added or altered.
`cargo fmt --check` clean; clippy reports no warnings in S2 files
(remaining warnings are pre-existing in M2/M3 files).
`cargo test` (default) shows decoder behavior untouched.

### 13.5 Open questions for S3+ (from §11, updated)

- Q4 remainder (unchanged): port `MsStereoProcessing` +
  `IntensityStereoProcessing` decisions so runtime PE/S2 inputs stop
  being oracle-supplied. S1+S2 prove both consumer sides exactly.
- Q5 unchanged (`NO_NOISE_PNS` throughout; S5 `noiseCount` proof
  obligation stands).
- Q6 unchanged (NULL partition closed in S1; count↔encode agreement
  waits for the writer).
- Q1–Q3, Q7–Q8 untouched by S2 (S6/S7 territory). S2 additionally
  notes: `DistributeBits` grant plumbing (`bits2Pe`, `peCorrection`,
  `bitresCalcBitFac`) is entirely S6 — S2 never reads grants.

## 14. S3 implementation record (2026-10-15)

Planning §§1–13 stand; this section records what S3 built, measured, and
left open. No S4–S6 code was added; no bitstream bytes are produced.

### 14.1 What S3 implemented

**[Fact]** New/changed files (all encoder-side or test-only tooling):

- `rust/src/aac_enc/quant.rs` (new) — `quantize_line`
  (`quantize.cpp:116-169`), `inv_quantize_line` (`:180-267`),
  `quantize_spectrum` (`:278-301`), `calc_sfb_dist` (`:312-349`),
  `calc_sfb_quant_energy_and_dist` (`:361-401`), `MAX_QUANT`,
  private `fixmuldiv2_ss/ds` + `fpow2` width helpers. 9 tests.
- `rust/src/aac_enc/quant_tables.rs` (new, generated) — `QUANT_TABLE_Q/E`
  (16-bit), `MTAB_3_4` (16-bit, 512), `MTAB_4_3` (Q31, 512),
  `SPEC_EXP_MANT`/`SPEC_EXP_SHIFT` (4x14), `QUANT_K0/K1`.
- `rust/src/aac_enc.rs` — `pub mod quant;` + `pub(crate) mod
  quant_tables;` + doc lines.
- `rust/src/aac_enc/testdata.rs` (extended, test-only) — strict
  `load_qprobe` + `load_s3` (`S3Channel`/`S3Frame`).
- `rust/src/aac_enc/testdata/qprobe_quant.txt` (committed probe
  transcript, ~22 KB) + `qc3_*.txt` (19 files, ~570 KB).
- `tools/enc-oracle/quant_probe.cpp` (new, test-only) — synthetic
  probes through the exported entry points only, with the
  valid-envelope gate enforced (aborts loudly on `totalShift < 0` or
  `SHORT_MIN` instead of executing UB).
- `tools/enc-oracle/extract_quant_tables.py` (new) — table extraction
  with independent source-text cross-check (aborts on any mismatch).
- `tools/enc-oracle/enc_oracle.cpp` (test-only) — S3 stage (real
  `QuantizeSpectrum` over gains [-72,-32,-8,64] + per-band
  `calcSfbDist`/`calcSfbQuantEnergyAndDist`, inactive-band spectrum
  zeroed, envelope-gated, `s3q` zero-initialized).
- `tools/enc-oracle/extract_s3.py`, `validate_s3.py` (new);
  `README.md` (S3 + probe format/commands).

Out of scope (unchanged): scalefactor search/afterburner (S4),
sectioning/counting (S5), QC loop/rate control (S6), MS/IS decisions
(still oracle inputs via S1).

### 14.2 Reference audit outcome (S3 scope)

**[Fact]** Ported: `quantizeLines`/`invQuantizeLines` (static, reached
via the exported wrappers)/`QuantizeSpectrum`/`calcSfbDist`/
`calcSfbQuantEnergyAndDist` + six tables + two dead-zone constants +
`MAX_QUANT`. `dZoneQuantEnable = 0` for LC (`qc_main.cpp:399-411`,
`isLowDelay = 0`); the `=1` path shares the `k` selection and is
probe-covered only.

**[Fact]** I/O: Q31 `i32` spectra in, `i16` lines out, integer QSS
(`globalGain - scf`) per band, Ld-Q31 distortion/energy out. Rounding
is truncation `(k+accu)>>15` with dead zone `k`; no clipping in the
quantizer (wraps through the `SHORT` cast; range is S6's gate).

**[Fact]** Arch-sensitive (pinned to this arm64/x86-64 build):
`FIXP_QTD` is 16-bit (`qtd_bytes=2` asserted by the probe;
`QTC = FX_DBL2FXCONST_SGL`); `fixmuldiv2_SS` is an unshifted product;
`fixmuldiv2_DS` widens by 16; inverse-path `fMult` keeps bit 0 (ARM
`asr #31`, unlike x86 `shl edx,1`); distortion-path `fPow2` clears bit
0 on all arches (`>>32<<1`, no override exists). The port keeps
separate `fmult`/`fpow2` helpers with distinguishing unit anchors.

**[Fact]** S3/S4 boundary: S4 (`improveScf` et al.) calls
`calcSfbDist` (`sf_estim.cpp:390-1226`) and
`calcSfbQuantEnergyAndDist` (`:1010`); S3 exposes exactly those plus
the spectrum quantizers. `calcMaxValueInSfb` stays S6.

**[Open, documented quirk]** `q == -32768` slips every reference
`|q| > MAX_QUANT` guard (`fixabs_S` wraps) and then OOB-reads
(`specExp = 16`). The port replicates the guard branch bit-exactly
but panics on the table index instead of reading out of bounds.
Committed fixtures are scanned to exclude the value (extract-time
gate); S6 inherits the same reference blind spot in
`calcMaxValueInSfb` (recorded, not fixed here).

### 14.3 Oracle and fixture coverage

**[Fact]** Committed S3 set (19 element-frames = all S1/S2 frames):
per channel, S3Q full 1024-line spectra at 4 grid gains, S3DIST +
S3EN per band per gain. Grid spans clipping/wrap (-72: 22% nonzero,
3 wrapped vectors), dense (-32: 5%), sparse (-8: 0.7%) and the
all-zero dead-zone path (64: 0%). S3 inputs are the qc1 post-MS/IS
spectra (the exact `QuantizeSpectrum` input via the `aacenc.cpp:803`
alias) with inactive-band lines zeroed (S3 input contract, §14.4).

**[Fact]** Synthetic probe transcript (286 QL/QBANDS/QD/QI lines):
magnitudes {0, ±1, ±2, ±3, ±100, ±1000, ±32767/8, ±1e6, ±2^28..2^31-1,
MINVAL} x gains {-64..192} x dz 0/1, 2-group scf ramp, distortion
bands incl. the `MAX_QUANT` early-out sweep, caller-chosen inverse
lines incl. ±8191/±8192 and mid-band clipping. All inside the valid
envelope by probe-gate construction.

**[Fact]** 3-run validation (same `v0/v1/v2` runs): **S3-STABLE**
(strict byte identity, no band exemption — the zeroing contract
holds); **S1-STABLE** and **S2-STABLE** re-validated on the same runs
(S3 stage does not disturb earlier lines); M3 `extract_testdata.py`
needed an S3FRAME..S3END skip (same pattern as the S1/S2 skips —
without it, extraction aborts on the first S3 block) and is otherwise
byte-identical on 7 spot-checked vectors; the 8th
(`psy_onset-stereo_f24_ch0`) differs in 13 GSPEC positions, all proven
inside FF-inactive bands — the documented M3 scratch wobble, not S3
disturbance. Fresh `extract_s3` output byte-identical to all 19
committed qc3 files; fresh `quant_probe` output byte-identical to the
committed transcript; table re-extraction reproduces
`quant_tables.rs` values exactly (layout normalized by rustfmt, as
with `extract_tables.py`).

### 14.4 Numeric-parity findings (all resolved, no tolerances added)

**[Fact]** Every S3 comparison is bit-exact (integers). Five findings:

1. Missing sign-branch split (port bug): the positive C branch passes
   the line through UNnegated (`:233`); the port negated
   unconditionally. Caught by the first test run, fixed.
2. `fpow2` vs `fmult` LSB (expectation bug in the test, not the
   port): `fixpow2_D = fixmuldiv2_DD<<1` clears bit 0 on all arches;
   pinned with the `0x40000001` distinguisher both ways.
3. Sign symmetry is NOT total (shared with C): the first-stage
   `fixmuldiv2_DS` floor breaks it by 1 LSB for some magnitudes
   (e.g. ±2^31-1 at gain -64: -4096 vs -4093). Property test scoped
   to structurally-exact inputs (multiples of 2^16); the transcript
   pins the asymmetric cases bit-exactly.
4. `calcSfbDist` early-out stops writing `quantSpectrum` at the first
   `|q| > MAX_QUANT` line (`:327-329`): the re-quantize cross-check
   compares only the written prefix now.
5. Inactive-band C scratch leaks into per-band distortion (sine f5
   ch0 band 41: committed `1`s and stale magnitudes inside
   `maxSfbPerGroup`-excluded bands). Fix: the S3 stage zeroes
   inactive-band spectrum lines (S3 input contract — unread
   downstream, every reference loop bounded by `maxSfbPerGroup`);
   the Rust test zeroes the same lines. Unvisited `s3q` bands are
   zero-initialized (no stack garbage in dumps).

Known limitations: `common_window = 0` has no fixture (S3 takes
geometry as inputs, no windowing logic); `dZone = 1` is probe-only;
`q == -32768` panics loudly by design (see §14.2 quirk).

### 14.5 Acceptance results

**[Fact]** `cargo test --features aac_enc --lib`: 359 passed
(310 M1–M3 + 21 S1 + 19 S2 + 9 new S3), 1 failed — the pre-existing
decoder `tp_dec::pce::tests::read` (also the only failure on default
features: 254 passed + same 1). No M1–M3 file was modified; no
tolerance was broadened; no `KNOWN_*` entry was added or altered.
`cargo fmt --check` clean; clippy reports no warnings in S3 files
(`too_many_arguments` allowed on `quantize_spectrum` per M3/S1/S2
precedent for C-arity mirrors).
`cargo test` (default) shows decoder behavior untouched.

### 14.6 Open questions for S4+ (from §11, updated)

- Q4 remainder (unchanged): MS/IS decisions still oracle-supplied.
- Q5 unchanged (`NO_NOISE_PNS` throughout; S5 `noiseCount` proof
  obligation stands).
- Q6 unchanged (NULL partition closed in S1; count↔encode agreement
  waits for the writer).
- Q1–Q3, Q7–Q8 untouched by S3 (S6/S7 territory). S3 additionally
  notes: real S4 QSS values are unknown yet — the grid spans the
  plausible operating range (-72..64) but S4 must confirm its gains
  land inside the valid envelope (the probe gate gives a loud abort
  if they do not); the `-32768` blind spot rides along into S6's
  `calcMaxValueInSfb` gate.

## 15. S4 implementation record (2026-10-15)

Planning §§1–14 stand; this section records what S4 built, measured, and
left open. No S5–S6 code was added; no bitstream bytes are produced.

### 15.1 What S4 implemented

**[Fact]** New/changed files (all encoder-side or test-only tooling):

- `rust/src/aac_enc/scf.rs` (new) — `calc_sfb_relevant_lines`
  (`sf_estim.cpp:183-224`), `count_single_scf_bits` (`:233-243`),
  `calc_single_spec_pe` (`:250-268`), `count_scf_bits_diff`
  (`:275-313`), `calc_spec_pe_diff` (`:320-369`), `improve_scf`
  (`:378-452`), `assimilate_single_scf` (`:458-623`,
  `restartOnSuccess = 1`), `assimilate_multiple_scf` (`:629-752`),
  `assimilate_multiple_scf2` (`:758-1044`, all three sub-steps),
  `estimate_scale_factors_channel` / `estimate_channel_with_inv`
  (`:1046-1277`, `invQuant = 2` production path plus `0/1`
  checkpoints on the same code path). `MAX_SCF_DELTA`, `NO_SCF`,
  all `FL2FXCONST_DBL` literals C-probed. 6 tests.
- `rust/src/aac_enc.rs` — `pub mod scf;` + doc line.
- `rust/src/aac_enc/testdata.rs` (extended, test-only) — strict
  `load_s4` (`S4Level`/`S4Channel`/`S4Frame`); `testdata_path`
  widened to `pub(crate)` for the unit-probe loader.
- `rust/src/aac_enc/testdata/qc4_*.txt` (19 files, ~720 KB) +
  `scfunit.txt` (synthetic transcripts, ~110 KB).
- `tools/enc-oracle/enc_oracle.cpp` (test-only) — S4 stage calling the
  real `FDKaacEnc_EstimateScaleFactors` at `inv = 0/1/2` on identical
  inputs (S1 spectra with inactive-band lines zeroed, S1 `PENLD`
  energies, S2 `THRV` thresholds, S1 `FF` form factors; psy/qc alias
  the same working spectrum like `aacenc.cpp:803-808`),
  envelope-gated (`SHORT_MIN` / `totalShift < 0` abort loudly).
- `tools/enc-oracle/extract_s4.py`, `validate_s4.py` (new);
  `extract_testdata.py` (+S4FRAME skip); `scf_probe.cpp`,
  `scf_unit_probe.cpp` (new, test-only); `README.md` (S4 + probe docs).

Out of scope (unchanged): sectioning/counting (S5), QC loop/rate
control (S6), bitstream writing, MS/IS decisions (still oracle inputs
via S1).

### 15.2 Reference audit outcome (S4 scope)

**[Fact]** Ported: initial-formula scf (`:1085-1168`, incl.
`minSfMaxQuant` via `CalcLdData(maxSpec)` + `c1Const` with the
`c1Const > -1-tmp` guard), `improveScf` (both NMR branches,
`UPCOUNT_LIMIT = 1`), all three assimilations (single with restart,
multiple region sweeps, multiple2 steps 1–3), clamp (`min + 60` with
requant), global-gain inversion + dead-band zeroing. Only
`invQuant = 2` (afterburner forced on, `aacenc.cpp:716`);
`dZoneQuantEnable = false` (LC). Other modes rejected loudly.

**[Fact]** I/O: Q31 spectrum (mutated in place: dead bands zeroed),
Ld energies/thresholds/form-factors in, `scf` (post-inversion stored:
`maxSf - orig`, dead `0`, unvisited `MIN`), `globalGain`, 1024-line
`quantSpec`, mutated spectrum out. `QSS = gain - scf` reproduces the
searched QSS on live bands for the later `QuantizeSpectrum`.

**[Fact]** No S6 dependency: the search prices candidates with DPCM
Huffman lengths (`bit_count_scf_delta`, table proven exact against the
ROM) plus the PE model (`calcSingleSpecPe`/`calcSpecPeDiff`), never
with granted bits. S4 takes no bit-budget input; `grantedPe` /
`grantedDynBits` never reach this file.

**[Fact]** S4/S5 boundary: `calcMaxValueInSfb`, `dynBitCount` /
sectioning, gain loop, grants, fill/finalize/reservoir are S5/S6.

### 15.3 Oracle and fixture coverage

**[Fact]** Committed S4 set (19 element-frames = all S1/S2/S3 frames):
per channel geometry + per `inv` level (0 = initial formula, 1 =
+improve/single, 2 = full afterburner) the `S4SCF`/`S4GAIN`/`S4Q`/
`S4SPEC` plus recomputed `S4DIST`/`S4EN` diagnostics (same QSS, mirrors
S3DIST/S3EN — the internal `sfbDist` scratch is not exposed by the
exported call). The three levels share inputs (fresh spectrum copies
per level), so `0` vs `1` vs `2` localizes divergences to the initial
formula, single assimilation, or the multiple stages.

**[Fact]** Synthetic transcripts (`scfunit.txt`, ~5800 lines): `CB`
(`countSingleScfBits` dense scf×delta incl. small corpus-like deltas),
`SP` (`calcSingleSpecPe` dense scf×constPe×nLines incl. corpus
values), `RL` (relevant lines), `SN` (full single run), `CH` (full
channel at `inv=0/1/2` with a dead band), `IM` (`improveScf` trials),
`N46` (relevant lines on corpus band-46 inputs), `QB45`/`QB46`
(`calcSfbDist` on corpus bands at their S4 (mod-2) QSS values — the
mod-2 gains the S3 grid never covered), `SX` (6-band corpus-slice
single run with nrel + final scf).

**[Fact]** 3-run validation (fresh `v0/v1/v2` runs):
**S4-STABLE** (strict byte identity, no exemption — same zeroing
contract as S3); **S1-STABLE**, **S2-STABLE**, **S3-STABLE**
re-validated on the same runs (S4 stage disturbs nothing earlier);
M3 `extract_testdata.py` (with the new S4FRAME skip — without it,
extraction aborts on the first S4 block) byte-identical on 6
spot-checked vectors; the 7th (`psy_onset-stereo_f24_ch0`) differs in
13 GSPEC positions, all proven inside FF-inactive bands (the
documented M3 scratch wobble). Fresh `extract_s4` output
byte-identical to all 19 committed qc4 files; fresh `scf_unit_probe`
output byte-identical to the committed transcript.

### 15.4 Numeric-parity findings (all resolved, no tolerances added)

**[Fact]** Every S4 comparison is bit-exact (integers). Two findings:

1. Restart signedness (port bug): the single-assimilation walk ends
   with `while (sfbNext < sfbCnt)` on SIGNED ints (`sf_estim.cpp:622`),
   so `-1` (restart) continues. The port compared `(next as usize) <
   cnt`, which wraps `-1` to `usize::MAX` and exits after the first
   success — the walk never restarted. Symptom: isolated 6-band single
   stopped after band 1 (`[-40,-40,-30,-20]` vs `[-40,-40,-37,-21]`
   with the tail untouched). Fixed to a signed compare; pinned by the
   `SX` 6-band single test (which exercises restart across dead-band
   gaps on corpus values).
2. Edge-neighbour tracking (port bug, the load-bearing one): for the
   first/last relevant band the reference passes POINTERS to the local
   `scfAct` trial variable (`sf_estim.cpp:507` → `scfLast = &scfAct`;
   `:516` → `scfNext = &scfAct`), so the missing side TRACKS `cur` on
   every trial (0 DPCM cost on that side) and the `prevScf` save
   records the final `cur`. The port captured visit-start values
   instead (3-bit / 393216-unit PE difference on the first trial:
   C `peNew = 1283916` vs Rust `1677132` with `peOld = 1364659` on
   both sides). Bisected via paired C/Rust trial traces (`TR visit` /
   `TR trial` / `TR dist` lines) on the isolated 6-band corpus slice;
   fixed by threading `first_edge` / `last_edge` flags through the
   trial loop and the `prev` save. Pinned by the `SX` scf test plus the
   full corpus (which failed on 7/19 `inv=1` frames before the fix and
   passes everywhere after).

Known limitations: `common_window = 0` has no fixture (S4 takes
geometry as inputs); `q == -32768` panics loudly by design (inherited
S3 quirk — proven absent on all committed S4 outputs by the envelope
test); the trailing `start == cnt` scan mirrors C's branch outcome
without its out-of-bounds `scf[startSfb]` read (same skipped-region
result, documented in code).

### 15.5 Acceptance results

**[Fact]** `cargo test --features aac_enc --lib`: 365 passed
(310 M1–M3 + 21 S1 + 19 S2 + 9 S3 + 6 new S4), 1 failed — the
pre-existing decoder `tp_dec::pce::tests::read` (also the only failure
on default features: 254 passed + same 1). No M1–M3 file was modified;
no S1–S3 behavior was changed (S1/S2/S3 tests all still pass
unmodified); no tolerance was broadened; no `KNOWN_*` entry was added
or altered. `cargo fmt --check` clean; clippy reports no warnings in
S4 files (`too_many_arguments` / `needless_range_loop` allowed per
M3/S1/S2 precedent for C-arity/index mirrors; `manual_memcpy`
rewritten to `copy_from_slice`/`fill`). Default-feature tests show
decoder behavior untouched.

### 15.6 Quantizer-envelope proof

**[Fact]** Every committed S4Q vector (all 19 frames × channels ×
`inv` 0/1/2) was scanned by the oracle gate AND the extractor gate
AND the Rust `s4_quantizer_envelope_holds` test: no `-32768`
anywhere. Every live line was additionally gated on recomputed
`totalShift >= 0` in the oracle (same probe logic as S3), so all
S4-generated QSS values lie inside the supported S3 envelope on every
target (including x86 where the asserts compile in). Real S4 QSS
values observed span roughly `-72..+10` (e.g. onset f24 short,
gain `-61`), inside the S3-probed operating range.

### 15.7 Open questions for S5+ (from §11, updated)

- Q4 remainder (unchanged): MS/IS decisions still oracle-supplied.
- Q5 unchanged (`NO_NOISE_PNS` throughout; S5 `noiseCount` proof
  obligation stands).
- Q6 unchanged (NULL partition closed in S1; count↔encode agreement
  waits for the writer).
- Q1–Q3, Q7–Q8 untouched by S4 (S6/S7 territory). S4 additionally
  notes: S5 consumes post-inversion `scf`/`globalGain`/`quantSpec`/
  `maxValueInSfb` (via a later `calcMaxValueInSfb`, S6) with the
  section geometry from S1; the `-32768` blind spot rides along into
  S6's gate; S4 outputs are valid S5 inputs by the envelope proof.

## 16. S5 implementation record (2026-10-15)

Planning §§1–15 stand; this section records what S5 built, measured, and
left open. No S6 code was added; no bitstream bytes are produced.

### 16.1 What S5 implemented

**[Fact]** New/changed files (all encoder-side or test-only tooling):

- `rust/src/aac_enc/bitcount.rs` (new) — `bit_count` + six `count*`
  kernels (`bit_cnt.cpp:121-555`), `count_values` (`:560-723`),
  `build_bit_lookup`/`find_best_book`/`find_min_merge_bits`/
  `merge_bit_lookup`/`find_max_merge`/`calc_merge_gain`/
  `gm_stage0`/`gm_stage1`/`gm_stage2`/`noiseless_counter`
  (`dyn_bits.cpp:112-430`), `scf_count` (`:467-586`), `noise_count`
  (`:589-615`), `dyn_bit_count` (`:617-643`); `SectionInfo`/`SectionData`
  (C field order, `sections: Vec` compacted — count is `.len()` by
  construction); scf-DPCM lengths reuse `pe::bit_count_scf_delta` (no
  duplicate table). 10 tests.
- `rust/src/aac_enc/bitcount_tables.rs` (new, generated) — the eight
  integer tables (`huff_ltab1_2/3_4/5_6/7_8/9_10/11`, `sideInfoTabLong/`
  `Short`), probe-printed and source-cross-checked.
- `rust/src/aac_enc.rs` — `pub mod bitcount; pub(crate) mod
  bitcount_tables;` + doc line.
- `rust/src/aac_enc/testdata.rs` (extended, test-only) — strict
  `load_s5` (`S5Section`/`S5Channel`/`S5Frame`; QMAX range-checked
  0..32768, section order + `S5BITS nsec` agreement asserted).
- `rust/src/aac_enc/testdata/qc5_*.txt` (19 files, ~76 KB).
- `tools/enc-oracle/enc_oracle.cpp` (test-only) — S5 stage calling the
  real `FDKaacEnc_dynBitCount` per channel on fresh S4 inv=2 outputs
  (recomputed for stage independence, deterministic per S4-STABLE),
  with an inline QMAX replica of the S6-owned static
  `calcMaxValueInSfb`, a manually wired `BITCNTR_STATE`, `blockType =
  lastWindowSequence`, S1 geometry/noise/is vectors, `syntaxFlags = 0`;
  envelope-gated (`SHORT_MIN` abort; `FDK_ASSERT` on `|Δscf| > 60`
  stays armed — build has no `-DNDEBUG`).
- `tools/enc-oracle/bitcount_probe.cpp`,
  `extract_bitcount_tables.py`, `extract_s5.py`, `validate_s5.py`
  (new); `extract_testdata.py` (+S5FRAME skip); `README.md` (S5 format,
  table provenance, run commands).

Out of scope (unchanged): `codeValues`/`codeScalefactorDelta` encode
half (takes a bitstream handle — S7), `calcMaxValueInSfb` port (S6 owns
it; S5 consumes QMAX as input), gain loop/grants/fill/reservoir (S6),
bitstream writing (S7).

### 16.2 Reference audit outcome (S5 scope)

**[Fact]** Ported: the full `dynBitCount` path — per-group one-section-
per-band init, cheapest-book selection (strict `<`, first wins),
equal-book run merging + side info (with the `sfbStart` breadcrumb
write), greedy max-gain merging while gain > 0 (PNS/IS involvement
forces −1), compress pass (book re-resolution — stage 2 never updates
`codeBook` — huffman net of side info, group-shifted starts,
compaction), `scfCount` (first-nonzero scan, IS-DPCM branch, zero-
`maxValue` skip-forward with the `|Δ| ≤ 60` bit-saving rule, trailing
no-live-band `0`), `noiseCount` (9-bit PCM header + DPCM), and the
`huffman+sideInfo+scalefac+noiseNrg` return (`:641-642`).

**[Fact]** I/O: `quantSpec &[i16]`, `maxValueInSfb &[u32]`, `scf &[i32]`,
geometry + `blockType` (LONG=0/START=1/SHORT=2/STOP=3), explicit
`noiseNrg/isBook/isScale`, `use_vcb11` (accepted, always `false` in
this build — `syntaxFlags = 0`, `aacenc.cpp:358`; the two `find*`
helpers ignore it exactly like C). The C `scalefacGain == NULL` early
return is unreachable (QCMain always passes `qcOutCh->scf`) and is not
modeled — documented in code.

**[Fact]** No S6 dependency: S5 takes no bit-budget input and prices
candidates with table lengths only. `SECTION_DATA` is defined once here
(C field order) for the future writer to share.

### 16.3 Oracle and fixture coverage

**[Fact]** Committed S5 set (19 element-frames = all S1–S4 frames): per
channel geometry + QMAX/ISB/ISS/NOISE precondition lines + the
section list (book/start/cnt/bits per section, strict order) + the
`S5BITS` totals (block/nsec/side/huff/scf/noise/first/dyn). Coverage
survey over the 19 files: codebooks 0–11 all appear (incl. ESC book 11
on onset f24), section counts 1–16, dyn bits 9–3446; onset f24's
`first=45` exercises the scf skip-forward path end-to-end.

**[Fact]** 3-run validation (fresh `v0/v1/v2` runs with the S5-stage
binary): **S5-STABLE** (strict byte identity, no exemption);
**S1/S2/S3/S4-STABLE** re-validated on the same runs (S5 stage appends
blocks only — earlier content undisturbed). Fresh `extract_s5` output
byte-identical to all 19 committed qc5 files; fresh `bitcount_probe`
output byte-identical to the extracted transcript; M3
`extract_testdata.py` (with the new S5FRAME skip) byte-identical on 4
spot-checked vectors.

### 16.4 Numeric-parity findings (all resolved, no tolerances added)

**[Fact]** Every S5 comparison is bit-exact (integers). Three
implementation bugs were found by failing tests (all port bugs, all
fixed, none in the reference):

1. Group-based spectrum slicing (port bug): `noiseless_counter` passed
   `&quant_spec[group_base..]` while `build_bit_lookup` indexed it with
   absolute offsets — double-based reads (caught as an out-of-range
   panic on the first corpus run). Fixed to pass the full slice, as C
   passes the full `quantSpectrum` with shifted offsets.
2. Over-strict debug precondition (test bug): the `max_val`-bounds-
   values `debug_assert` fired on a hand-written `max_val = 16` input
   with `|v| = 20` — but the C dispatch clamps at 16 while the real
   `maxVal` is the true band maximum (20). Fixed the test input (same
   kernel selected); the assert correctly documents the real-path
   precondition (loud panic instead of C's OOB table read).
3. Two hand-derived literals miscounted pairs (test bugs): the ESC
   `[20,-20,0,0]` expectation dropped the zero pair's `ltab11[0][0] =
   4` base cost (17 → 21, confirmed against the ROM text); the
   `[100,-200,300,-400]` ESC expectation under-counted escape doublings
   (→ 60, recomputed stepwise). Both fixed; cross-kernel agreement
   (`countValues` vs `bitCount` row) held throughout, isolating the
   errors to the literals.

Plus one deliberate behavior confirmation: the crafted 2-band scf test
merged into ONE section in stage 2 (side-info saving beats joint
cost), so it pins the true intra-section skip-forward path (silent band
codes a delta-0 word, cost 1) rather than the firstScf-scan case as
first drafted — the test was rewritten to assert the merged outcome.

Known limitations: PNS/IS books never occur on fixtures (S1-proven
zeros); the branches are code-mirrored and covered by synthetic PNS/IS
single-section tests. `useVCB11`/`HCR` paths are dead in this build
(asserted `false` by all callers). The trailing `start == cnt`-style
scans have no OOB analogue (all loops range-checked; C reads stay in
bounds on valid inputs).

### 16.5 Acceptance results

**[Fact]** `cargo test --features aac_enc --lib`: 375 passed
(365 S1–S4/M1–M3 + 10 new S5), 1 failed — the pre-existing decoder
`tp_dec::pce::tests::read` (also the only failure on default
features: 254 passed + same 1). No M1–M3 file was modified; no S1–S4
behavior was changed (all S1–S4 tests still pass unmodified); no
tolerance was introduced; no `KNOWN_*` entry was added or altered.
`cargo fmt --check` clean; clippy reports no warnings in S5 files
(`too_many_arguments` / `needless_range_loop` allowed per M3/S1/S2/S4
precedent for C-arity/index mirrors; `manual_memcpy`-class loops
rewritten to `fill`/`copy` where trivial). Default-feature tests show
decoder behavior untouched.

### 16.6 Quantizer-envelope proof (extended to S5)

**[Fact]** Every S5 input line vector (all 19 frames × channels, from
S4 inv=2) was scanned by the oracle gate AND the Rust
`s5_quantizer_envelope_holds` test: no `-32768` anywhere (so the
`fixp_abs` widening in the count kernels, which needs `|q| ≤ 32768 <<
i32::MAX`, is safe on supported inputs); every QMAX fits 0..32768
(loader-enforced). The completed oracle run is additionally proof that
no corpus `|Δscf|` exceeds 60 (the armed `FDK_ASSERT` would have
aborted the run). S5 outputs are valid S6 inputs by construction
(`|q| ≤ 8191` gating itself stays S6's `calcMaxValueInSfb` gate —
fixtures may contain ESC-range maxima since the S5 oracle bypasses the
gain loop by design; documented, not a violation).

### 16.7 Open questions for S6 (from §11, updated)

- Q4 remainder (unchanged): MS/IS decisions still oracle-supplied.
- Q5 unchanged (`NO_NOISE_PNS` throughout; S5 `noiseCount` no-op
  inherits the same proof obligation per frame; synthetic PNS test
  proves the live branch).
- Q6 unchanged (NULL partition closed in S1; the count↔encode agreement
  checks (`WRITE_SEC/SCAL/SPEC_ERROR`) belong to the bitstream
  milestone — S5's single `SectionData` definition is the shared
  contract).
- Q1–Q3, Q7–Q8 untouched by S5 (S6/S7 territory). S5 additionally
  notes: S6 consumes `SectionData` + post-inversion `scf`/`globalGain`/
  `quantSpec` + oracle QMAX (to be replaced by the real
  `calcMaxValueInSfb` port, cross-checkable against these fixtures);
  the `-32768` blind spot rides along into S6's gate.

## 17. S6 implementation record (2026-10-15)

Planning §§1–16 stand; this section records what S6 built, measured, and
left open. No bitstream bytes are produced; the `codeValues`/
`codeScalefactorDelta` encode half and `WriteBitstream` stay S7.

### 17.1 What S6 implemented

**[Fact]** New/changed files (all encoder-side or test-only tooling):

- `rust/src/aac_enc/rate.rs` (new) — QC-init derivations
  (`calc_bits_per_frame`, `derive_qc_init`, `init_bits2pe_factor`,
  `distribute_state_init`), `adjust_bitrate`/`frame_padding`/
  `calc_frame_len`, `bit_res_redistribution`,
  `distribute_element_dyn_bits`, `prepare_grants`, `bits2pe2`,
  `bitres_calc_bit_fac` (+ save/spend/peMinMax helpers),
  `calc_pe_correction` (+ LowBitRes), `distribute_bits`,
  `calc_max_value_in_sfb`, `reduce_bit_consumption` (+ `ReduceOutcome`),
  `crash_recovery`, consumed-bits accumulators, `exit_matrix_step`,
  `update_fill_bits`, `fill_extension_bits`, `finalize_bit_consumption`,
  `update_bitres_vbr`, frame driver `qc_main_frame` (+ `QcState`,
  `DistributeState`, `QcFrameInput/Output`, `PassTrace`). 17 tests.
- `rust/src/aac_enc.rs` — `pub mod rate;` + doc lines.
- `rust/src/aac_enc/testdata.rs` (extended, test-only) — strict `load_s6`
  (`S6Pass`/`S6Frame` with pass-order + `npass` agreement, `S6GSTATE`/
  `S6GPOST` 6-tuples) and `load_s6init` (14 fields; `bits2pe` split).
- `rust/src/aac_enc/testdata/qc6_*.txt` (20 frames) + `qc6init_*.txt`
  (3 operating points) + `qc{1,2,4,5}_stress-stereo_f5.txt` (S6 inputs
  for the new F6 frame; S1/S2/S4/S5 PLANs untouched).
- `tools/enc-oracle/enc_oracle.cpp` (test-only) — S6INIT via the real
  `QCInit` path (replacing the manual `AdjThrInit` block; S1/S2 outputs
  verified byte-identical after the switch) + per-frame S6 stage (real
  `AdjustBitrate`/`DistributeBits`/`QuantizeSpectrum`/`dynBitCount`/
  `ChannelElementWrite`/`writeExtensionData`/`updateFillBits`/`updateBitres`
  with documented inline replicas of the file-statics), persistent
  QC/ATS state, envelope gates.
- `tools/enc-oracle/make_fixtures.py` — `m6-stress-stereo-48k.wav` (F6:
  full-scale splitmix32 + tone stack, deterministic).
- `tools/enc-oracle/extract_s6.py`, `validate_s6.py` (new);
  `extract_testdata.py` (+S6FRAME/S6INIT skip); `extract_s{1-5}.py`
  `short_name` gains `m6-` stripping (one line each; existing outputs
  unaffected); `README.md` (S6 format, run commands incl. `m6`).

Out of scope (unchanged): encode half, `WriteBitstream`/transport, CBR
family, multi-element arms, ELD `bits2pe` tables, `codeValues` counting
beyond S5's `count_values` reuse.

### 17.2 Reference audit outcome (S6 scope)

**[Fact]** Ported verbatim (integer-exact): the VBR grant/budget formulas,
`DistributeBits` FULL + REDUCED/DISABLED branches with both pe-correction
paths, the gain-loop retry structure (`calculateQuant` gating,
`MAX_QUANT` gate with `decrease = 1`, element cap, `dynBitsLast`
followup), `reduceBitConsumption` (gain bump/crash/QUANT_ERROR arms),
`crashRecovery` (top-down band drops, `countValues` repricing, side-info
reclaim by window, static/grant/maxDyn rebalance via recount, wipeout
tools/TNS clear), the exit matrix (including the first-pass memclear
asymmetry: DONE via the taken branch still clears constraints), VBR
fill/finalize (fill-element sizing with the `cnt >= 4` payload gate,
8-bit dedup, range check) and the reservoir pin.

**[Fact]** Three VBR-5 facts the port relies on (all read from source,
all oracle-verified): (1) QCMain passes `maxBitsPerFrame` — not the frame
`avgTotalBits` — into `BitResRedistribution`/`prepareBitDistribution`
(`:805-806,836-839`), so grants/budget derive from the buffer size and
`AdjustBitrate` only evolves `paddingRest`; (2) `bitResTot` is pinned full,
so redistribution yields level/max 0 and `totalAvail == maxBitsPerFrame`;
(3) `globHdrBits` is a transport-owned input (56, probed via a real
`transportEnc`: 56/56/56 independent of `auBits`; PCE `-1` for SCE/CPE).

**[Fact]** `DistributeState`/`QcState` split: S2-owned ATS fields
(`chaosMeasureOld`, enFac, `ahParam`, `minSnrAdaptParam`) stay in
`AtsElement` untouched; S6 owns only the grant subset (same C struct,
partitioned by use). No S1–S5 algorithm was altered.

### 17.3 Oracle and fixture coverage

**[Fact]** Committed S6 set: 20 element-frames (all 19 S1–S5 frames + F6
stress f5) with grants, per-pass iteration trace (strict order),
fill/finalize totals and pre/post grant-state 6-tuples; 3 init fixtures
(mono-48k, stereo-48k, stereo-44100) from the real `QCInit` path. All 20
corpus frames are single-pass (`npass = 1`, no crash) — correct at VBR-5,
where `totalAvail == maxBitsPerFrame` makes retry an emergency path
(plan R4). Retry/crash are covered by synthetic Rust tests with shrunken
buffers (2000-bit forces multi-pass convergence; 1200-bit forces crash
with `maxSfbPerGroup` drop and rebalance), plus hand-computed exit-matrix,
fill/finalize and error-arm vectors.

**[Fact]** 3-run validation with the S6-stage binary: **S1/S2/S3/S4/S5/
S6-STABLE** (strict byte identity). Fresh `extract_s6` output
byte-identical to all 23 committed files. M3 `extract_testdata.py` (with
the S6FRAME/S6INIT skip) byte-identical on 4 spot-checked vectors.
Grant-state chaining holds: PRE(f+1) == POST(f) on consecutive frames
(verified on onset f22–26, which the Rust chain test replays end to end).

### 17.4 Real-encoder cross-check (`aac-enc` ADTS frame sizes)

**[Fact]** `S6FINAL total + 56` reproduces the stripped `aac-enc`
binary's ADTS frame lengths bit-exactly on 5/15 files for every frame
(silence 49/49, impulse 11/11, edge 14/14, noise-mono 25/25, quiet-sine
49/49) and on all non-blip frames of the rest. This validates the whole
S6 chain (QCInit, grants, DistributeBits, loop exit, fill, finalize,
reservoir pin) end to end — it caught the `totalAvail` port bug below.

**[Fact]** 10 content frames diverge, ALL `SHORT_WINDOW` (f1 of sine,
stepdown, click, multitone, sweep, tonal-noise, sine-441k, stress;
onset f24/f22-441k transients): oracle S6 totals differ from `aac-enc`
(e.g. sine f1: 784 vs 2024; tonal f1: 872 vs 3208; click f1 differs by
only 8 bits). Every LONG/START/STOP frame matches. Root cause is upstream
of S6 (short-block inputs: M2/M3/S2/S4 short paths, pre-existing) — S6
formulas are window-independent and proven exact wherever inputs agree.
Fixtures pin oracle behavior including the blips (short-block coverage is
required); the boundary is documented here, not hidden. No S1–S5 code was
touched for the blips. Trailing index diffs (f47/48) are flush-tail
artifacts (oracle runs extra ZERO_FRAMES flush frames).

### 17.5 Numeric-parity findings (all resolved, no tolerances added)

**[Fact]** Every S6 comparison is bit-exact (integers; the two `f32`
window comparisons in `calc_pe_correction` replicate C float semantics
exactly). Four port bugs were found by failing tests/fixtures, all fixed:

1. `totalAvail`/`grantedDynBits` from frame `avgTotalBits` (wrong) instead
   of `maxBitsPerFrame` (VBR arg, `:805-806,836-839`): made loud mono
   frames spuriously loop (7 passes + crash on noise f1–7). The `aac-enc`
   cross-check exposed it (5160-bit frames vs 2224-bit oracle totals);
   after the fix the full 25-frame noise sequence matches bit-exactly.
2. `writeExtensionPayload` fill-nibble gate dropped (`cnt >= 4`): sized a
   7-bit fill as 3 instead of 7. Fixed with the `EXT_TYPE_BITS` gate cited.
3. `maxBitsEl` derivation used `maxBits` instead of the per-channel share
   (`2 * (maxBits / nChannelsEff)` for CPE, `channel_map.cpp:396`).
4. Three hand-transcribed Q31 literals were wrong (e.g. `1.18/2` is
   `0x4B851E80`, not `0x4B849000`); all S6 literals are now C-probed
   (`/tmp/s6lit_probe.cpp` values recorded in `s6_q31_literals_match_probe`).

Plus one structural correction: the `calc_max_value_in_sfb` no-write
contract for unvisited bands (S5 fixtures zero-init; C leaves stale) is
now pinned by sentinel assertions, and S5 `QMAX` cross-checks only visited
bands — the headline S5→S6 handoff proof (19 frames × channels exact).

Known limitations: PNS/IS books never occur on fixtures (S1-proven zeros);
`useVCB11`/HCR dead (`syntaxFlags = 0`); multi-element/CBR/ELD paths panic
explicitly; `checkMinFrameBitsDemand` stub returns 1 like C; crash
`sectionToScf` assumes sections tile bands exactly once (true by S5
construction; debug-asserted by indexing).

### 17.6 Acceptance results

**[Fact]** `cargo test --features aac_enc --lib`: 392 passed (375
S1–S5/M1–M3 + 17 new S6), 1 failed — the pre-existing decoder
`tp_dec::pce::tests::read` (also the only failure on default features:
254 passed + same 1). No M1–M3 or S1–S5 file was modified (only additive
`testdata.rs` loader + `aac_enc.rs` module line + oracle tooling); no
tolerance was introduced; no `KNOWN_*` entry was added or altered.
`cargo fmt --check` clean; clippy reports zero warnings in `rate.rs`
(`too_many_arguments`/`needless_range_loop` allowed per M3/S1–S5 precedent
for C-arity/index mirrors). Default-feature tests show decoder behavior
untouched.

### 17.7 Quantizer-envelope proof (extended to S6)

**[Fact]** Every S6 re-quantization scans all 1024 lines for `-32768`
(oracle gate + Rust `assert_ne!`, loud in all builds); none occurs on the
20 corpus frames (S4-proven). `calc_max_value_in_sfb` asserts the same
before `unsigned_abs`. The completed oracle run is additionally proof that
no corpus `|Δscf|` exceeds 60 (armed `FDK_ASSERT` in the scf-DPCM path
would abort). S6 outputs satisfy the writer preconditions of plan Q8
(`|q| ≤ 8191` gated, `|Δscf| ≤ 60`, `totalBits ≤ maxBitsPerFrame`,
section geometry within `SECT_ESC_VAL` limits) on all 20 frames.

### 17.8 Open questions for S7 (from §11, updated)

- Q6 (writer agreement): S6 hands `SectionData` + post-inversion
  `scf`/`globalGain`/`quantSpec` + counts to the future writer (single
  shared `SectionData` definition, S5 contract upheld). The
  `WRITE_SEC/SCAL/SPEC_ERROR` agreement checks belong to S7.
- Q8 (writer preconditions): enforced by S6 on all fixtures (see §17.7).
- Q1/Q2 (bitResMode FULL, `qualityIdx = 1`): confirmed numerically —
  `S6INIT mode=0` on all three operating points; `bits2pe = 1267015296,1`
  (LC default, tables never consulted).
- Q4 remainder (MS/IS decisions oracle-supplied), Q5 (`NO_NOISE_PNS`
  throughout), Q7 (VBR targeting: grants from `maxBits`, quality from
  thresholds — replicated, never "fixed"), R4 (crash rarity at VBR-5:
  corpus never retries; synthetic starvation tests cover the paths).
- New: the 10-frame short-block oracle-vs-encoder boundary (§17.4) is an
  S1–S5/M2–M3 fidelity item, not S6; S7 bitstream comparisons should
  expect it on SHORT frames until the upstream cause is found.

## 18. S7 implementation record (2026-10-15)

Planning §§1–17 stand; this section records what S7 built, measured, and
left open. No element/frame assembly, transport, or ADTS code was added;
S8 owns `ChannelElementWrite` write mode, `FDKaacEnc_WriteBitstream`,
transport, and ADTS.

### 18.1 S7 contract (ownership disambiguation)

**[Fact]** S7 owns the encode half plus the three dynamic-payload section
writers, with the boundary drawn at (not through) element assembly:

- Ported: `FDKaacEnc_codeValues` (`bit_cnt.cpp:725-939`, books 0–11) +
  `FDKaacEnc_codeScalefactorDelta` (`:941-950`) + the file-static payload
  writers `encodeSpectralData` (`bitenc.cpp:127-147`),
  `encodeSectionData` (`:239-281`), `encodeScaleFactorData` (`:292-369`).
- Helpers ported with them: `huff_ctab1..11` + `huff_ctabscf`
  (`aacEnc_rom.h:129-140`, `aacEnc_rom.cpp:294-569`), the MSB-first
  `FDKwriteBits` order (`FDK_bitstream.h:342-376`, big-endian `FDK_put`),
  `HI_LTAB/LO_LTAB` (`bit_cnt.cpp:107-108`), the book-11 escape formula
  (`:922-931`), and the writer constants `globalGainOffset = 100` /
  `noiseOffset = 90` (`bitenc.cpp:114-116`), `LOG_NORM_PCM = -15`
  (`psy_const.h:161`), `SECT_ESC_VAL_LONG/SHORT = 31/7`,
  `SECT_BITS_LONG/SHORT = 5/3`, `CODE_BOOK_BITS = 4`
  (`dyn_bits.h:112-117`).
- S8+: element assembly (element tag, ICS/MS/global-gain/pulse/TNS write
  paths), the runtime `WRITE_SEC/SCAL/SPEC_ERROR` checks
  (`bitenc.cpp:1049-1122`, which live in `ChannelElementWrite`),
  `WriteBitstream`, transport, ADTS. S7 proves the agreement those checks
  enforce (emitted == S5 counts, strict, on all fixtures); S8 hosts them.

**[Fact]** S7's only S1–S6 touch is a visibility widening: pe.rs
`HUFF_LTABSCF` became `pub(crate)` so the encode half shares the length
table single-source with the counters (no duplicate table, no algorithm
change). No S1–S6 algorithm, fixture, or tolerance was altered; the S6
PE-correction fixes are byte-untouched.

**[Fact]** Bit-buffer abstraction: no encoder-owned writer existed
(`common::bitstream::Bitstream` is decoder-owned and `common/` is
frozen); S7 adds a minimal bounded MSB-first `BitWriter` (capacity
errors explicit — C has no bounds check).

### 18.2 What S7 implemented

**[Fact]** New/changed files (all encoder-side or test-only tooling):

- `rust/src/aac_enc/bitenc.rs` (new) — `BitWriter` (bounded MSB-first
  writer, `FDKwriteBits` order + `FDKsyncCache`-compatible tail packing),
  `BitEncError` (explicit refusal for every C silent/UB case),
  `code_values` (books 0–11 verbatim incl. sign packing and escapes),
  `code_scalefactor_delta` (`|delta| > 60` refused; C returns 1),
  `encode_section_data` / `encode_scale_factor_data` /
  `encode_spectral_data` (section traversals verbatim, returning emitted
  bits for the S8 agreement checks). 9 tests.
- `rust/src/aac_enc/bitenc_tables.rs` (new, generated) — the twelve code
  tables, probe-printed and source-cross-checked (note `ctab11[21][17]`:
  rows 17..20 carried verbatim, never indexed by the encode path).
- `rust/src/aac_enc.rs` — `pub mod bitenc;` + `pub(crate) mod
  bitenc_tables;` + doc lines.
- `rust/src/aac_enc/testdata.rs` (extended, test-only) — strict `load_s7`
  (`S7Channel`/`S7Frame`) and `load_s7probe` (`S7Probe::CodeValues/
  CodeDelta`).
- `rust/src/aac_enc/testdata/qc7_*.txt` (20 frames) + `s7probe.txt`
  (287-vector C transcript, committed).
- `tools/enc-oracle/bitenc_probe.cpp` (new, test-only) — table printer +
  synthetic encode vectors through the real leaf functions.
- `tools/enc-oracle/extract_bitenc_tables.py` (new) — table extraction
  with independent source-text cross-check (aborts on any mismatch).
- `tools/enc-oracle/enc_oracle.cpp` (test-only) — S7 stage (real leaf
  functions + real `FDKwriteBits` on the saved S5 first pass; traversal
  replicas self-checked against S5 counts, loud abort on deviation).
- `tools/enc-oracle/extract_s7.py`, `validate_s7.py` (new);
  `extract_testdata.py` (+S7FRAME skip); `README.md` (S7 format, probe
  docs, run commands).

Out of scope (unchanged): element assembly, `WriteBitstream`/transport/
ADTS, CBR family, ELD tables, `common/` (frozen), decoder.

### 18.3 Reference audit outcome (S7 scope)

**[Fact]** Behavioural rules pinned by the probe before coding:
`codeValues` book 0 emits nothing; books 1/2 index `(v+1)` quads;
books 3/4 append sign bits trailing (`(cw << sl) | sign`); books 5/6
concatenate the second pair's codeword first (`(cw1 << len2) + cw2`);
books 7–10 seed `sign` from the first value's top bit; book 11 clamps
magnitudes at 16 and escapes each `|v| >= 16` with
`((((1 << (n-3)) - 2) << n) | (v - (1 << n)))`, `n + n - 3` bits
(`n = 4; while ((p >>= 1) >= 16) n++`); scf delta 0 is 1 bit (`0x00`),
`±61` return 1 with zero bits; invalid books emit nothing (C default
arm — Rust refuses explicitly instead).

**[Fact]** C divergences handled loudly: out-of-LAV values, non-stride
widths (C reads the neighbouring band; all 1734 corpus SFB widths are
multiples of 4, everything else is `InvalidWidth`), `|delta| > 60`,
the `codeValues` default arm (PNS/IS carry no spectral lines — the
traversal skips them explicitly with the citation, matching C's
observable zero bits), oversized write values (C masks), buffer
overflow (C unbounded).

**[Fact]** `encodeScaleFactorData` returns C's `1` sentinel as `Err`
(never a bit count). The `scfCount` skip-forward far case (nonzero
delta on a zero-`maxValue` band) provably never fires on valid inputs
(it needs a live-to-live `|Δscf| > 60`, which S6 forbids per plan Q8);
the encode side unconditionally emits delta 0 there, matching C.

### 18.4 Oracle and fixture coverage

**[Fact]** Committed S7 set: 20 element-frames (all S1–S6 frames incl. F6
stress f5) × channels with `S7SPEC`/`S7SCF`/`S7SECH` bit-exact payloads
(hex, MSB-first). Corpus spread: spectral 0–3323 bits (15 zero-payload
channels — the empty-input edge), scf 0–114, section headers 9–144;
codebooks 0–11 all exercised incl. ESC book 11 (onset f24).

**[Fact]** 3-run validation with the S7-stage binary: **S7-STABLE**
(strict byte identity) and **S1/S2/S3/S4/S5/S6-STABLE** re-validated on
the same runs (S1 shows only the documented FF-inactive SPEC wobble).
Fresh `extract_s7` output byte-identical to all 20 committed qc7 files;
fresh `bitenc_probe` output byte-identical to the committed transcript;
table re-extraction reproduces `bitenc_tables.rs` (modulo rustfmt
layout, as with `extract_tables.py`). M3 `extract_testdata.py` (with
the new S7FRAME skip) byte-identical on 3 spot-checked vectors; the
4th (`psy_onset-stereo_f24_ch0`) differs in 12 GSPEC positions, all
proven inside FF-inactive band 13 of short groups 2–3 (the documented
M3 scratch wobble, not S7 disturbance).

**[Fact]** PNS/IS walk limitation (documented, not hidden): corpus
PNS/IS inputs are uniformly off (S1-proven), so the PNS-PCM/DPCM and IS
DPCM walk branches are code-mirrored with primitive-verified lengths
(probe-pinned `codeScalefactorDelta` + 9-bit PCM rule) and synthetic
Rust structure tests — no corpus C payload exists for them. The
straight-line walks contain no skip logic (unlike `scfCount`'s normal
path, which IS corpus-proven); the first real PNS/IS frame in a future
corpus will exercise them end to end.

### 18.5 Acceptance results

**[Fact]** `cargo test --features aac_enc --lib`: 405 passed
(396 S1–S6/M1–M3 + 9 new S7), 1 failed — the pre-existing decoder
`tp_dec::pce::tests::read` (also the only failure on default
features: 254 passed + same 1). No M1–M3 file was modified; the single
S1–S6 touch is the `HUFF_LTABSCF` visibility widening (no behaviour
change — all S1–S6 tests pass unmodified); no tolerance was
introduced; no `KNOWN_*` entry was added or altered.
`cargo fmt --check` clean; clippy reports zero warnings in S7 files.
Default-feature tests show decoder behavior untouched.

### 18.6 Quantizer-envelope proof (extended to S7)

**[Fact]** Every S7 payload encodes committed S4Q lines already proven
free of `-32768` (S4/S5/S6 envelope proofs); the leaf widens `i16` to
`i32` before `unsigned_abs`, so even the probe's `-32768` vector is
defined behaviour both sides. The completed oracle run is additionally
proof that no corpus `|Δscf|` exceeds 60 on the encode path (the armed
`FDK_ASSERT` plus the checked `codeScalefactorDelta` return codes would
abort). S7 outputs satisfy the writer preconditions of plan Q8 on all
20 frames by the emitted==counted proofs.

### 18.7 Open questions for S8 (from §11, updated)

- Q6 (writer agreement): proven for all dynamic payloads (S7 tests);
  S8 hosts the runtime `WRITE_SEC/SCAL/SPEC_ERROR` checks in the
  `ChannelElementWrite` port and adds the static-scaffolding write
  paths (element tag, ICS, MS, global-gain, pulse, TNS).
- Q8 (writer preconditions): enforced and proven on all fixtures.
- Q1/Q2 (FULL, `qualityIdx = 1`): unchanged from S6.
- Q4 remainder (MS/IS decisions oracle-supplied), Q5 (`NO_NOISE_PNS`
  throughout — S7 PNS-walk limitation above), Q7 (VBR targeting
  untouched), R4 (unchanged).
- New: the 10-frame short-block oracle-vs-encoder boundary (§17.4) is
  unchanged by S7 (payload encoding is window-independent; S7 compares
  against oracle payloads, never against `aac-enc` frame sizes).
