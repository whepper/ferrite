# 03 — FDK Reference Analysis (Algorithmic Authority)

Date: 2026-11-16. Every translation task cites this document. Upstream
citations use the `cpp/…` tree at git `3878c45a` (= `4c69425~1`), recoverable
via `git show 3878c45a:cpp/…`. Ferrite citations use HEAD `a2c1d53`.

## 1. Upstream version and provenance (verified)

| Fact | Evidence |
|---|---|
| Upstream product | Fraunhofer FDK AAC Codec Library for Android, **version 3.0.0** (AOSP `platform/external/fdk-aac` versioning; includes xHE-AAC library, stripped in Ferrite history) | `git show 3878c45a:ChangeLog` (top entry `3.0.0`); `METADATA` (`license_type: BY_EXCEPTION_ONLY`) |
| Encoder code base | FDKv2 encoder (`libAACenc`), fixed-point `FIXP_DBL` = Q31 | `cpp/libAACenc/src/*` |
| Pinned reference commit | `f14ad40a574a4132df947d15e9fb14bee91cc8a5` (parent `da00af1c…`), private reference repo, **not published**, no submodule/remote | `docs/reference/fdk-aac-reference-revision.txt` |
| Local availability | `cpp/` removed from Ferrite at `4c69425`; full tree at `3878c45a`. Local reference checkout emptied by temp cleanup; re-provision required | git history; `01` §1 |
| Tree equivalence | *Assumption A-3*: pinned `f14ad40a` `cpp/` ≡ `3878c45a:cpp/`. The provenance inventory resolves all Ferrite `cpp:` anchors against `3878c45a` and treats it as the legacy reference tree. Confirm by diff at M0. | `docs/aac-enc-provenance-inventory.md` §A |
| License | FDK-AAC (Fraunhofer), no patent license; see `12` | `cpp/…/psy_const.h` header; `rust/LICENSE-FDK-AAC` |

**Version caution (assignment §4):** this is FDK-AAC **3.0.0**, not the
2.0.x used by some other Rust ports. Do not import constants/line-numbers
from other projects. All anchors below were resolved against `3878c45a`.

## 2. Structural mapping: C `psyMain` vs Ferrite factoring

Upstream `FDKaacEnc_psyMain` (`psy_main.cpp:407-1298`) is monolithic: it does
block switching, transform, rescale, tonality, TNS, threshold advance, MS,
grouping, IS/MS stereo, PNS, and output build in one function. Ferrite factors
it across modules; the **algorithm and order are preserved**:

| C stage (`psy_main.cpp`) | Ferrite owner | Note |
|---|---|---|
| Block switching `476-514` | `block_switch.rs` (called in `frontend`) | factored out; f32 |
| Transform `552-629` | `transform.rs` + `common/mdct.rs` | factored out; f32→f64 |
| Lowpass/zeroSpec `584-614` | `psy.rs:398-418` | inside psy |
| Rescale `631-749` | `psy.rs:636-797` + `energy.rs` | inside psy |
| Tonality `756-764` | **absent** (PNS off) | `usePns` gate |
| TNS `766-920` | `psy.rs:799-949` + `tns.rs` | TNS internals separate |
| Threshold advance `922-1019` | `psy.rs:1242-1277` + `thresholds.rs` | inside psy |
| MS energies `1021-1037` | `psy.rs:994-1043` + `energy.rs:321` | inside psy |
| Grouping + Ld `1039-1130` | `psy.rs:1285-1537` | inside psy |
| IS/MS stereo `1161-1227` | `intensity.rs` + MS mirror in `stream_driver.rs` | factored out (downstream) |
| PNS `1140-1246` | **absent** (PNS off) | `NO_NOISE_PNS` |
| Output build `1251-1295` | `psy.rs:1420-1513` + `LiveChannel` | inside psy + driver |

## 3. Function-level mapping table

| Algorithm | Upstream file:function:lines | Ferrite mirror | FPE class |
|---|---|---|---|
| Per-band headroom | `band_nrg.cpp:FDKaacEnc_CalcSfbMaxScaleSpec:111-132` | `energy.rs:130 max_scale_spec_into` | B (repr lane) |
| Rescale energy + maxNrg | `band_nrg.cpp:FDKaacEnc_CheckBandEnergyOptim:140-184` | `energy.rs:161 check_band_energy_into`, `:190 max_nrg_for_shift` | C (value) + B (decision) |
| Rescale shifts | `psy_main.cpp:631-687` | `energy.rs:440 rescale_shifts` | B (repr lane) |
| Energy/Ld re-shift + thr init | `psy_main.cpp:689-708` | `psy.rs:752-771`, `thresholds.rs:130 init_threshold` | C |
| TNS long energy recompute | `band_nrg.cpp:FDKaacEnc_CalcBandEnergyOptimLong:192-256` | `energy.rs:249 band_energy_long_into` | C + B(shiftBits) |
| TNS short energy recompute | `band_nrg.cpp:FDKaacEnc_CalcBandEnergyOptimShort:264-288` | `energy.rs:215 band_energy_short_into` | C |
| TNS overflow rescale | `psy_main.cpp:893-912` | `psy.rs:1206 overflow_rescale` | C(value)+B(shift) |
| Clip cap | `psy_main.cpp:926-939` | `thresholds.rs:137 clip_cap` | C |
| Spreading | `spreading.cpp:FDKaacEnc_SpreadingMax:105-125` | `thresholds.rs:149 spreading_max` | C |
| PCM floor | `psy_main.cpp:955-971` | `thresholds.rs:167 apply_pcm_floor` | C |
| Pre-echo init | `pre_echo_control.cpp:FDKaacEnc_InitPreEchoControl:106-115` | `thresholds.rs:194 PreEchoState::new` | C |
| Pre-echo control | `pre_echo_control.cpp:FDKaacEnc_PreEchoControl:117-176` | `thresholds.rs:218 PreEchoState::control` | C |
| STOP/START resets | `psy_main.cpp:975-985, 996-1006` | `thresholds.rs:207 reset_for_transition`, `psy.rs:1264-1272` | C |
| Spread energy | `psy_main.cpp:1009-1017` | `psy.rs:1274-1275` | C |
| MS energies | `band_nrg.cpp:FDKaacEnc_CalcBandNrgMSOpt:296-361` | `energy.rs:321 calc_ms_energies` | C |
| Grouping | `grp_data.cpp:FDKaacEnc_groupShortData:118-264` (`nrgAddSaturate:114`) | `psy.rs:1285 group_short_channel` | B/C |
| Output Ld + floor | `psy_main.cpp:1058-1076, 1120-1128` (`LdDataVector`) | `psy.rs:1518-1537`, `sfb.rs:184 ld_data` | C |
| maxSfbPerGroup | `psy_main.cpp:1089-1101`, `grp_data.cpp:144-158` | `psy.rs:1308-1328, 1491-1509` | B |
| SFB table/offsets | `psy_configuration.cpp:FDKaacEnc_initSfbTable:191-259` | `sfb.rs:255-268` | A |
| Bark values | `psy_configuration.cpp:FDKaacEnc_BarcLineValue:270-317`, `initBarcValues:407-419` | `sfb.rs:bark_of_line` | A (data only) |
| Spreading factors | `psy_configuration.cpp:getMaskFactor:336-352`, `initSpreading:354-405` | `sfb_tables.rs SPREAD_*` (tabulated) | B (consume) |
| PCM quant threshold | `psy_configuration.cpp:FDKaacEnc_InitMinPCMResolution:325-334` | `sfb.rs:298-304` | A |
| minSnr | `psy_configuration.cpp:FDKaacEnc_initMinSnr:421-532` | `cfg_*.txt MINSNR` (tabulated) | B (consume) |
| Psy config init | `psy_configuration.cpp:FDKaacEnc_InitPsyConfiguration:534-627` | `sfb.rs:236 SfbConfig::new` | A |
| Block switching | `block_switch.cpp:229-346`, `CalcWindowEnergy:359-409`, `SyncBlockSwitching:426-560` | `block_switch.rs:341/456/409` | A |
| TNS detect/sync/encode | `aacenc_tns.cpp:FDKaacEnc_TnsDetect/TnsSync/TnsEncode`, `FDK_lpc.cpp` | `tns.rs` | A |
| Tonality (PNS) | `tonality.cpp:FDKaacEnc_CalculateFullTonality:121-162`, `CalcSfbTonality:175-219`; `chaosmeasure.cpp` | absent | A (absent) |
| fLog2 / LdData | `fixpoint_math.h:807-885` (Taylor), `LdDataVector` | `q31.rs:380 calc_ld_data`; float `sfb.rs:184` | C (float log2) |
| InvLdData | `fixpoint_math.cpp:208-240`, `fixpoint_math.h:228-256` | `q31.rs:439 calc_inv_ld_data` | A (downstream) |
| Fixed-point primitives | `fixmul_arm.h:174-193` (ARM64 `fixmul_DD`), `fixmul.h`, `fixmadd.h`, `scale.h:153-199`, `clz.h:152-200` | `q31.rs` | A (repr/boundary) |

## 4. Component analyses (assignment §7 items 1–13)

Notation: `FIXP_DBL` = Q31 signed 32-bit fraction, real value `int × 2^-31`.
`DFRACT_BITS = 32`. ARM64 `fMult(a,b) = floor(a·b / 2^31)` narrowed to 32 bits
(`fixmul_arm.h:174-193`, `smull; asr #31`); generic C fallback is
`((a·b)>>32)<<1` (floor-to-even) — **Ferrite targets ARM64 semantics**
(`q31.rs:104-107`). `fPow2AddDiv2(t,s) = t + ((s·s)>>32)` (all archs identical).
`scaleValue(v,s)`: arithmetic `>>` for s<0, wrapping `<<` for s≥0
(`scale.h:153-160`). LdData format: `log2(x)/64` in Q31 (`LD_DATA_SHIFT=6`),
sentinel `−1.0` (`i32::MIN`) for x≤0.

### 4.1 Spectral energy / band energy accumulation

- **Source**: `band_nrg.cpp:140-184` (rescale), `:192-256` (TNS long),
  `:264-288` (TNS short).
- **Purpose**: per-scalefactor-band energy = sum of squared spectral lines,
  scaled to maximize fixed-point precision; drives thresholds, PE, MS.
- **Formulation (rescale, long)**: for band b, `scale = max(0, maxScale[b]−4)`;
  `tmp = Σ_j fPow2AddDiv2(tmp, spec[j] << scale)`; `E[b] = tmp << 1`.
  Real value ≈ `Σ (x_j·2^scale)² × 2^-31` (stored domain). The `−4` and `<<1`
  give 4 bits of accumulation headroom (max band width 96 < 2^7, squared 2^14,
  /2 → 2^13, hence 4-bit margin rationale at `band_nrg.cpp:204-205`).
- **Q format / scaling**: input Q31 spectrum with block exponent `mdctScale`;
  energy stored domain has implicit exponent `2·mdctScale`. Per-band left
  shift `scale` recovers precision for quiet bands.
- **Multiply/accumulate**: `fPow2AddDiv2` = halved square `(s·s)>>32` added to
  32-bit accumulator (wrapping on overflow; headroom design prevents it).
- **Rounding/saturation**: each square **truncated** to 32 bits (`>>32`);
  accumulation wraps (unreachable by design); final `<<1`. Short path:
  `maxScale−3`, **no final `<<1`**, output rescale
  `scaleValueSaturate(E, −(2(maxScale−3)−1))` clamped ±31 (saturating).
- **Tables**: none (arithmetic).
- **Tuning constants**: headroom offsets `−4` (long) / `−3` (short) are
  **numerical scaffolding**, not psychoacoustic tuning.
- **Float equivalent**: true sum `E_real = Σ x_j²` (f64, physical domain).
  Stored Q31 value `E_st = E_real·2^(31−2m)`; **FPE fraction-domain** value
  `E_f = E_st/2^31 = E_real·2^(−2m)` (per-band shifts cancel in float).
  Domains and full derivation: `05` §1/§6.1.
- **Numerical differences**: C truncates each square (≤2^-32 relative per
  line) and shifts; float keeps full precision. Difference bounded by C's
  truncation noise; measured envelope `05` §9.
- **Tolerance / tests**: stored `32 + |w|/500`; unit + differential vs oracle
  `NRG`/`NRGLD` (`07`).

### 4.2 Tonality estimation

- **Source**: `tonality.cpp:121-219`; `chaosmeasure.cpp`.
- **Finding (verified)**: `FDKaacEnc_CalculateFullTonality` is **entirely
  gated by `usePns`** (`tonality.cpp:129`). Its output `sfbTonality` is
  consumed only by `FDKaacEnc_PnsDetect` (`psy_main.cpp:1151`). Ferrite runs
  AAC-LC with **PNS off** → tonality is dead code and is **correctly absent**.
- **FPE decision**: **do not implement tonality** in Stage 1. Adding it would
  be a model change (non-goal). The separate S2 "chaos measure"
  (`adj_thr.cpp:1054 calcChaosMeasure`, an energy-based frame flatness measure)
  is downstream rate-control state already in `threshold_vbr.rs`/`pe.rs` —
  also out of FPE scope.
- **Tests**: none needed; assert absence (no `sfbTonality` symbol).

### 4.3 Spectral spreading

- **Source**: `spreading.cpp:105-125 SpreadingMax`; factors from
  `psy_configuration.cpp:354-405 initSpreading`.
- **Purpose**: spread masking threshold / energy across neighbouring bands
  (simultaneous masking skirts).
- **Formulation**: two-pass max-envelope. Forward (high slope):
  `d = x[0]; for i in 1..n: d = max(x[i], fMult(maskHi[i], d)); x[i]=d`.
  Backward (low slope): `d = x[n−1]; for i in (0..n−1).rev:
  d = max(x[i], fMult(maskLo[i], d)); x[i]=d`.
- **Q format**: factors Q31 fractions `<1`; `fMult` floor semantics.
- **Rounding**: `fMult` truncates toward −∞ (ARM64); envelope is contractive
  (factors<1) so per-step error does not amplify.
- **Tables / tuning**: factors `maskHi/Lo` (threshold) and `maskHiSprEn/LoSprEn`
  (spread energy). Derived from Bark-width slopes:
  `factor = 10^(−slope·Δbark/10)` via `getMaskFactor` (`fPow`). Slopes
  (`psy_configuration.cpp:365-372`): MASKHIGH **1.5**, MASKLOW **3.0**
  (threshold, dB/Bark); spread-energy long hi **2.0** (low-bitrate ≤20000:
  **1.5**), lo **3.0**; short hi **1.5**, lo **2.0**. Edge factors 0
  (`initSpreading:399-402`). **These slopes are psychoacoustic tuning.**
- **Float equivalent** (FPE fraction domain, `05` §1/§6.4):
  `d = max(x_f[i], mask_hi_f[i]·d)` with `mask_f = mask_st/2^31` (the
  `SfbConfig::mask_factors()` f64 accessors). **Consume the tabulated factors**
  (`sfb_tables.rs SPREAD_*`), do not recompute via float atan/pow: C's
  `fixp_atan`/`fPow` approximations deviate up to **13%** from ideal float
  (`sfb.rs:111-123`); recomputing would change the model.
- **Differences**: ≤1 LSB per `fMult` vs float multiply; contractive.
- **Tolerance/tests**: stored envelope; unit hand-computed envelope
  (`thresholds.rs:295-308` pattern) + differential `THR`/`SPREAD`.

### 4.4 Masking threshold calculation (init + clip)

- **Source**: init `psy_main.cpp:702-703`; clip `psy_main.cpp:926-939`.
- **Purpose**: initial masking threshold = energy × ratio; cap to avoid
  clipping artifacts.
- **Formulation**: `T = fMult(E, C_RATIO)`. Clip:
  `clipNrgShift = 2·mdctScale − THR_SHIFTBITS`; headroom = 6 (short) / 0 (long);
  if `clipNrgShift ≥ 0`: `clip = clipEnergy >> clipNrgShift`;
  elif `≥ −headroom`: `clip = clipEnergy << −clipNrgShift`; else `MAXVAL`.
  `T = min(T, clip)`.
- **Tuning constants**: `C_RATIO = 0x02940a10` (Q31). Real ratio
  `C_RATIO/2^31 = 16 × 10^(−29/10) ≈ 0.020143` — the ×16 is `THR_SHIFTBITS=4`
  headroom; the PCM-floor stage's `>>4` yields the perceptual ratio
  `10^(−29/10) ≈ 0.001259` (≈ −29 dB). `clipEnergy = 0x773593ff`
  (= `1e9 × NORM_PCM_ENERGY`, Q31), short `>>= 6`
  (`psy_configuration.cpp:595-607`). `THR_SHIFTBITS = 4`
  (`psy_configuration.h`, A-2). **C_RATIO and clipEnergy are tuning.**
- **Float equivalent** (fraction domain, `05` §6.3): `T_f = E_f·C_RATIO_f`
  (`C_RATIO_f = 0x02940a10/2^31`); clip value computed with the same branch
  logic as an exact power-of-two scaling.
- **Differences**: `fMult` floor vs float multiply ≤1 LSB.
- **Tests**: unit (`thresholds.rs:274-292` pattern) + differential `THR`.

### 4.5 Absolute hearing threshold

- **Finding (verified)**: FDK's LC model has **no ATH table**. Grep for
  "hearing"/"ATH" in `libAACenc` returns nothing. The absolute floor is the
  **PCM quantization threshold** `sfbPcmQuantThreshold`
  (`psy_configuration.cpp:325-334`): per-band
  `width × PCM_QUANT_NOISE`, `PCM_QUANT_NOISE = 0x00547062`
  (= `10^(−20/10) × ABS_LOW × NORM_PCM_ENERGY × 2^16`), modelling 1-bit
  (−96 dB) quantization noise. Applied in the PCM-floor stage (§4.6).
- **FPE decision**: translate the PCM floor; **do not invent an ATH model**
  (non-goal: no Bark/ERB/ATH where FDK has none).

### 4.6 PCM quantization floor (threshold adaptation)

- **Source**: `psy_main.cpp:955-971`.
- **Formulation**: per window, `energyShift += PCM_QUANT_THR_SCALE(16)`
  (starts `2·mdctScale`); if `≥0`: `energyShift=min(31,energyShift)`,
  `T = max(T >> THR_SHIFTBITS(4), pcm[b] >> energyShift)`; else
  `energyShift=min(31,−energyShift)`, `T = max(T >> 4, pcm[b] << energyShift)`.
- **Q format**: `T >> 4` removes the C_RATIO ×16 headroom; pcm floor shifted
  by the accumulating per-window `energyShift` (short windows accumulate
  16/window, capped 31).
- **Rounding**: arithmetic `>>` truncation.
- **Tuning**: `PCM_QUANT_THR_SCALE=16`, `THR_SHIFTBITS=4`, `PCM_QUANT_NOISE`
  (the −96 dB floor is tuning; the shift mechanics are scaffolding).
- **Float equivalent** (fraction domain, `05` §6.4):
  `T_f = max(T_f/16, pcm_f·2^(∓energyShift))` keeping fractional bits, where
  `pcm_f = SfbConfig::pcm_quant()` (already `/2^31`); identical `energyShift`
  accumulation/branch logic.
- **Differences**: `>>4` truncation vs `/16` (≤1 LSB); floor shift exact
  (power of two).
- **Tests**: unit (`thresholds.rs:311-325`) + differential `THR`.

### 4.7 Temporal masking / pre-echo control

- **Source**: `pre_echo_control.cpp:106-176`; resets `psy_main.cpp:975-1006`.
- **Purpose**: limit threshold increase vs previous frame (pre-echo), keep a
  fraction of current threshold; temporal masking across frames.
- **State**: `sfbThresholdnm1[MAX_SFB]` (prev thresholds), `mdctScalenm1`,
  `calcPreEcho`. Init: prev = long PCM floors, `mdctScalenm1 = 8`, enabled.
- **Formulation**: if disabled → memorize only. If `mdctScale > prevScale`:
  `scaling = 2(mdctScale−prevScale)`;
  `cap = maxAllowedIncreaseFactor × (prev >> scaling)`;
  `floor = fMult(minRemainingThresholdFactor, T)`; memorize pre-clamp T;
  `T = min(T, cap)` then `max(T, floor)`. Else branch:
  `scaling = 2(prevScale−mdctScale)`; `cap = (factor>>1)×prev`;
  if `(T >> (scaling+1)) > cap`: `T = cap << (scaling+1)`; `T = max(T, floor)`.
- **Tuning**: `maxAllowedIncreaseFactor = 2` (integer),
  `minRemainingThresholdFactor = 0x0148` (Q15 = 328/32768 ≈ 0.01001, ≈0.01)
  (`psy_configuration.cpp:591-592`). **These are tuning.**
- **Resets**: STOP-window → reset **before** control; START-window → reset
  **after** (prev=MAXVAL, scale=0, disabled) so transitions don't compare
  long vs short thresholds.
- **Rounding**: `fMult` floor for the 0.01 factor (`fmult_sgl`,
  `q31.rs:137`); shifts truncate.
- **Float equivalent** (fraction domain, `05` §6.5): float history `prev_f`
  (= `prev_st/2^31`), identical scale-aligned clamp/floor/memorize and reset
  ordering. `floor_f = T_f·(328/32768)`.
- **Differences**: ≤1 LSB per op; history accumulates float vs integer
  rounding — bounded, measured (Q31 engine already shows ≤25 Q31 on some
  frames vs C, `01` §7).
- **Tests**: unit (`thresholds.rs:328-370`) + differential `NM1`/`NM1STATE`
  + cross-frame onset/stepdown fixtures.

### 4.8 Attack detection / block switching

- **Source**: `block_switch.cpp:121-560`.
- **Finding**: **already float-native f32 in Ferrite** (`block_switch.rs`),
  upstream of psy. FPE **reuses unchanged** (class A). Documented for
  completeness; the known float-limited knife-edge case
  (`m7-speech-clean` f107) is inherited, not re-litigated.
- **Tuning**: HP coeffs −0.5095/0.7548, acc weights 0.3/0.7, invAttackRatio
  0.1, minAttackNrg 15625 Q31, border rule `/16 > 0.625×`, sequence/grouping
  tables (`block_switch.rs:165-268`).

### 4.9 Long-window processing

- Rescale energy `CheckBandEnergyOptim` (§4.1), TNS long recompute
  `CalcBandEnergyOptimLong` with overflow `shiftBits` decision
  (`band_nrg.cpp:237-255`): `maxNrgLd` starts 0.0; if `≤0` → shiftBits 0;
  else `while maxNrgLd>0: maxNrgLd −= 2/64; shiftBits++`. Ld guard
  (`band_nrg.cpp:229-232`): subtract scaleDiff only if
  `ld ≥ (−1.0>>1)+(scaleDiff>>1)`, else sentinel. FPE preserves the guard and
  the integer shiftBits decision (representation), floats the values
  (`05` §6.2).
- Long output: maxSfb from top non-zero line, floor `min(5,active)`
  (`psy_main.cpp:1092-1101`); threshold Ld floor −33/64.

### 4.10 Short-window processing

- 8×128 windows; TNS short recompute `CalcBandEnergyOptimShort` (§4.1, no Ld
  array — C quirk, short Ld stays at rescale values, `psy.rs:1185-1186`).
- Grouping `groupShortData` (§4.11); per-window PCM floor accumulation;
  pre-echo per window; MS-Ld stale-on-short.

### 4.11 Short-block grouping

- **Source**: `grp_data.cpp:118-264`.
- **Purpose**: regroup 8 short windows into `noOfGroups` groups (from block
  switching), summing energies/thresholds/spread/MS per group, interleaving
  spectrum, computing grouped offsets and maxSfbPerGroup.
- **Formulation**: `nrgAddSaturate(a,b) = (a ≥ MAXVAL−b) ? MAXVAL : a+b`
  (`grp_data.cpp:114-116`); per-group window sums over active bands; stride
  `sfbCnt` with padding; offsets `offset + sfbOffset[sfb]×groupLen`;
  maxSfb from top non-zero band across windows.
- **FPE decision**: offsets/maxSfb/spectrum interleave are data movement
  (reuse). Window sums: **recommended** quantize per-window float values to
  stored ints first, then reuse integer `nrgAddSaturate` regroup (keeps the
  grouped contract exact; the sum is representation-level). Alternative
  float-sum-then-saturate is equivalent within rounding (`05` §6.7).
- **Tests**: differential `GROUPED GE/GT/GS/GELD/GTLD/GOFF/GMSNR`.

### 4.12 Channel-specific / channel-pair processing

- **Per-channel**: energies, thresholds, TNS, pre-echo, MS-Ld history.
- **Element-global (joint)**: `minSpecShift` (min over pair), `finalShift`,
  zeroSpec shortcut, TNS active OR + `tnsSpecShift` (max), MS energies.
- **MS energies** `CalcBandNrgMSOpt` (`band_nrg.cpp:296-361`):
  `minScale = max(0, min(maxScL,maxScR)−4)`;
  `specm = (X_L<<(minScale−1)) + (X_R<<(minScale−1))` (or `>>1` if minScale 0);
  `NrgMid = Σ fPow2AddDiv2(NrgMid, specm)`; `mid = fMin(NrgMid, MAXVAL>>1)<<1`;
  Ld (if calcLd) with scale correction `−2(minScale0−4)/64` guarded by `!=−1`;
  stored `mid >>= max(0,2(minScale0−4))` capped 31. Net real convention:
  mid/side energies carry a **/4** factor relative to `(L±R)²`
  (`energy.rs:315-317`). FPE preserves the net convention (`05` §6.6).
- **calcLd = (seq != SHORT)**: long writes fresh MS-Ld; short leaves
  `sfbEnergyMSLdData` **stale** (`psy_main.cpp:1033-1035`); fresh grouped MS-Ld
  computed post-grouping (`psy_main.cpp:1078-1084`). FPE preserves staleness.
- **LFE**: forces Long, skips TNS + pre-echo (`psy_main.cpp:752-754, 973`);
  Ferrite D-022 asserts (`psy.rs:507-532`). FPE inherits.
- **IS/MS stereo decisions** are downstream (class A), not FPE.

### 4.13 Temporal state and history (summary)

| State | C location | FPE handling |
|---|---|---|
| Pre-echo prev thresholds/scale/enabled | `PSY_STATIC.sfbThresholdnm1/mdctScalenm1/calcPreEcho` (`psy_data.h:147-149`) | float history, identical transitions |
| MS-Ld history (stale-on-short) | `PSY_DATA.sfbEnergyMSLdData` (`psy_data.h:160`) | f64 history, identical write/read |
| TNS runtime/info | `TNS_DATA`/`tnsInfo` | reuse `TnsChannel` unchanged |
| Block-switch history | `BLOCK_SWITCHING_CONTROL` | reuse unchanged |
| MDCT overlap | `mdctPers`/`psyInputBuffer` | reuse unchanged |

## 5. Tuning constants vs numerical scaffolding (assignment §7 distinction)

**Psychoacoustic tuning (preserve exactly):** C_RATIO (−29 dB ratio),
clipEnergy (1e9·NORM_PCM_ENERGY), PCM_QUANT_NOISE (−96 dB floor), spreading
slopes (1.5/3.0/2.0 dB/Bark + low-bitrate branch), pre-echo factor 2 and
remaining 0x0148 (~0.01), minSnr params (BITS2PEFAC 1.18, PERS2P4 0.024,
MAX_SNR 0.8/−1 dB, MIN_SNR 0.003/−25 dB), block-switch coeffs/ratios,
Bark formula constants (13.3, 3.5, 0.00076, 4/3·1e-4), SFB widths, LFE
lowpass 12, threshold-Ld floor −33/64.

**Numerical scaffolding (eliminate/absorb in float, keep decisions):**
per-band headroom offsets −4/−3, `<<1`/`>>32` square truncation, THR_SHIFTBITS
×16/>>4 headroom, PCM_QUANT_THR_SCALE shift accumulation mechanics,
`fMult`/`fPow2AddDiv2` truncation, block exponent `mdctScale` bookkeeping,
`finalShift`/`nrgShift`/`tnsSpecShift` overflow prevention, `scaleValue`
wrapping. **Caveat (ADR-0005):** the *decisions* derived from scaffolding
(mdctScale trajectory, finalShift, tnsSpecShift, maxScale, shiftBits) are
**observable downstream** and are retained integer-exact in Stage 1; only the
*value arithmetic* becomes float.

## 6. Missing evidence / investigations

| ID | Item | Investigation |
|---|---|---|
| A-2 | Exact `psy_configuration.h` line anchors for THR_SHIFTBITS/PCM_QUANT_THR_SCALE/C_RATIO | **RESOLVED by M0** (`m0-verification.md` M0.2): anchors confirmed at `psy_configuration.h:113/114/117-118`; values match Ferrite (`thresholds.rs:115/117/119`, `sfb.rs:165/168`) |
| A-3 | Pinned tree ≡ `3878c45a:cpp/` | **M0: parent-tree verified, residual gap** (`m0-verification.md` M0.3): `da00af1c:cpp` ≡ `3878c45a:cpp` (tree `97225710…`, identical hashes); pinned `f14ad40a` object unavailable locally — confirm `f14ad40a:cpp` unchanged via private-reference re-provisioning before M8. **Not fully resolved** |
| A-6 | C `mdctSpectrum_e` derivation (why 12/9) | `transform.cpp`/`mdct.cpp` scaling; Ferrite uses measured constants (`psy.rs:127-131`); FPE reuses them, no re-derivation needed |
| A-7 | `fixp_atan`/`fPow` exact algorithms (for Stage-2 float recompute research only) | `FDK_trigFcts.cpp`, `fixpoint_math.cpp`; **not needed for Stage 1** (tables consumed) |