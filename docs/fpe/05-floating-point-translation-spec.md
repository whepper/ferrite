# 05 — Floating-Point Translation Specification

Date: 2026-11-16 (rev 2: numerical-domain remediation). This is the numerical
contract for FPE. Algorithm purpose and upstream provenance are in `03`; this
document specifies the **domains**, **float arithmetic**, scaling, rounding,
invariants, and tolerances. The Q31 mirrors (`energy.rs`, `thresholds.rs`,
`q31.rs`) are the line-by-line ground truth for edge-case ordering; every FPE
kernel is cross-checked against them.

> **Rev 2 note (P1 remediation).** Rev 1 mixed two incompatible float domains
> (a stored-integer-as-float domain for energies and a Q31-fraction domain for
> constants/boundary). This revision declares **one** FPE internal domain — the
> **Q31-fraction domain** — derives every formula from source in that domain,
> and adds a per-kernel domain table (§6.0). The choice is source-grounded
> (§1.3), not assumed.

## 1. Domains (authoritative definitions)

Three domains are used. **Every equation, constant, table and example in this
package is labelled with one of them.**

### 1.1 Physical domain

Actual mathematical amplitude/energy, independent of any block exponent.

- Amplitude `x` = the real spectral value. Ferrite's transform output **is**
  physical amplitude: ring input is `sample/32768.0` (`transform.rs:218,251`)
  and the MDCT parity contract is "Rust output = reference Q31 output read as
  `int·2^(E−31)`, per-bin ratio 1.0" (`transform.rs:120-125`). The frontend
  widens this `f32` to `f64` once (`stream_driver.rs:824-828`); the value
  `ChannelInput.spectrum` carries is `x`.
- Energy `E_real = Σ x²` over a band's lines (units: amplitude²).
- `m` (`mdctScale`) is the C block exponent tracked as metadata; physical
  values do **not** change when `m` changes (representation shifts leave real
  values untouched, `psy.rs:772-773`).

### 1.2 Stored Q31 domain

The existing **integer** representation (`i32`) used by the Q31 engine and by
the output contract / all downstream stages.

- Spectrum `X = round(x·2^(31−m))` (`energy.rs:395-404
  map_spectrum_to_q31_into`, factor `2^(31−m)`, round-to-nearest, clamp).
- Energy `E_st` ≈ `E_real·2^(31−2m)` (C truncation/shifts apply; §6.1).
- Threshold `T_st`, PCM floor `pcm_st`, clip `clip_st`, spreading factors
  `mask_st`, pre-echo history `prev_st` — all `i32`.
- LdData integer `ld_q31` = Q31 representation of `log2(stored/2^31)/64`
  (`q31.rs:380 calc_ld_data`), sentinel `i32::MIN` for non-positive.

### 1.3 FPE floating-point domain = Q31-fraction domain (the one internal domain)

**Definition: `v_f = v_st / 2^31`** (the stored integer read as a Q31 fraction).
Spectrum fractions lie in `[−1,1)`; energies/thresholds in `[0,~1)`.

| Quantity | FPE fraction value | Source grounding |
|---|---|---|
| Spectrum | `x_f = x·2^(−m)` (physical f64 fraction) | transform output, §1.1 |
| Energy | `E_f = E_real·2^(−2m)` (boundary: `E_st/2^31`, §6.1) | derivation §6.1 |
| Threshold | `T_f = T_st/2^31` | §6.3 |
| PCM floor | `pcm_f = pcm_st/2^31` | **`SfbConfig::pcm_quant()` is already `/2^31`** (`sfb.rs:301-304,432`) |
| Clip | `clip_f = clip_st/2^31` | **`SfbConfig::clip_energy()` already `/2^31`** (`sfb.rs:297`) |
| Spreading factors | `mask_f = mask_st/2^31` | **`mask_factors()`/`mask_factors_spr()` already `/2^31`** (`sfb.rs:330-342,447,457`) |
| Ld | `ld_f = log2(E_f)/64` (sentinel −1.0) | **existing float lane is `log2(stored/2^31)/64`** (`energy.rs:415-417 log2frac`, `psy.rs:1518-1529 grouped_ld`, `sfb.rs:184-190 ld_data`) |

**Spectrum domain (owner decision, FPE-3.1R1).** FPE psychoacoustic energy
consumes the **physical f64 spectrum** `x_f = x·2^(−m)`; it does **not**
quantize the spectrum before the energy sum. The Q31 representation is produced
by the integer representation lane:

```
X      = round(x · 2^(31−m))                    (Q31 mapping, clamp on overflow)
X/2^31 = x_f + ε_f
|ε_f|  ≤ 2^−32                                   (half an LSB, no saturation)
```

Thus `X/2^31` and `x_f` are numerically identical only when the physical
spectrum is representable on the Q31 grid. For a general spectrum they differ
by the input-mapping error `ε_f`. This difference is **expected** and is a
separate term in the numerical error budget (§9); it is not an implementation
defect. If the mapping saturates (`|round(x·2^(31−m))| > i32` range), the
half-LSB bound does not apply and the comparison is reported as
unsupported/invalid rather than silently accepting it.

**Why this domain (derived, not assumed):** the existing Ferrite float Ld lane
already converts stored ints to fractions (`/2^31`) before `log2/64`, and the
`SfbConfig` f64 accessors already expose `/2^31` fractions. Choosing the
fraction domain makes FPE consistent with both, and it **eliminates the Q-format
scaffolding** (the point of the float engine):

| Q31 stored-integer op | FPE fraction equivalent |
|---|---|
| `fMult(a_st,b_st) = floor(a_st·b_st/2^31)` | `a_f·b_f` |
| `fPow2AddDiv2(t_st,s_st) = t_st + s_st²/2^32` | `t_f + s_f²/2` |
| energy `<<1` after `fPow2AddDiv2` | cancels the `/2` ⇒ `E_f = Σ s_f²` |
| `shift_int(v_st,s) = v_st·2^s` | `v_f·2^s` (exact power of two) |
| `fmult_sgl(0x0148,c_st) = floor(328·c_st/2^15)` | `c_f·(328/32768)` |

### 1.4 Conversions between domains (the only boundaries)

| From → To | Formula | Rounding |
|---|---|---|
| Physical → FPE fraction (spectrum) | `x_f = x·2^(−m)` | exact (power of two) |
| Physical → FPE fraction (energy) | `E_f = E_real·2^(−2m)` | exact scaling; `E_real` sum carries f64 rounding |
| FPE fraction → Stored Q31 | `v_st = clamp(round(v_f·2^31), i32::MIN, i32::MAX)` | **round-to-nearest** (matches `map_spectrum_to_q31_into`); saturate at `2^31−1` where C saturates |
| Stored Q31 → FPE fraction | `v_f = v_st/2^31` | exact in f64 |
| FPE fraction → Ld | `ld_f = if v_f≤0 {−1.0} else {log2(v_f)/64}` | `log2` platform accuracy (§2); per-vector guard/floor §6.8 |
| Ld → Q31 (driver, **unchanged**) | `q31_of_ld = round(ld·2^31)`, sentinel `i32::MIN` (`stream_driver.rs:215-221`) | round-to-nearest |

**Saturation levels:** `MAXVAL_f = (2^31−1)/2^31 ≈ 1.0` is the **fraction-domain**
saturation level (the float image of stored `MAXVAL_DBL = 2^31−1`). It clamps
fraction values; the **separate** boundary clamp to `i32` range (§1.4) handles
the integer output. These are two distinct clamps in two distinct domains — do
not conflate them.

## 2. Precision choice: f64 (ADR-0002)

**f64 for all FPE value arithmetic.** f32 remains only where already used
(block switching, MDCT) — FPE does not touch those. No mixed precision inside
FPE kernels. Rationale (criteria from assignment §8):

| Criterion | f32 (24-bit mantissa) | f64 (53-bit mantissa) | Verdict |
|---|---|---|---|
| Precision vs C Q31 (32-bit) | accumulation rounds ~2^-24, **coarser** than C's 2^-32 square truncation → new noise beyond C | ~2^-53 ≪ C's ~2^-32 noise → deviation dominated by C truncation | **f64** |
| Accumulation error (≤96 lines) | ~96·2^-24 ≈ 6e-6 rel | ~96·2^-53 ≈ 1e-14 rel | f64 |
| Underflow / denormals | denormals < ~1e-38 reachable by quiet scaled bands (FTZ risk) | denormals < ~1e-308; never reached (silence = exact 0) | f64 |
| Compatibility | spectrum already f64; f32 needs narrow+re-widen | natural fit | f64 |
| SIMD width | 4/8-wide | 2/4-wide | f32 faster, but psy ≈2.4% self time; Stage 1 scalar |

**`log2` accuracy — corrected claim (rev 2).** Rev 1 stated std `log2` is
"correctly-rounded within 1 ulp". **That is not guaranteed** by Rust or IEEE-754
across platforms; `f64::log2` accuracy is implementation-defined. This package
makes **no** correct-rounding/1-ULP claim. Instead:

- The Ld acceptance criterion is **measured**, not assumed (§9, `07` §5):
  the energy-domain envelope `32 + E/500` and an absolute Ld bound established
  empirically from (a) C's `fLog2` Taylor error (~1e-9, documented
  `energy.rs:412-414`) and (b) the measured platform `log2` error.
- **Platforms to test:** ARM64 (primary validated target, Apple Silicon) and
  x86-64 (secondary, A-5). **Operations to test:** `log2` (Ld), the energy
  square-accumulation, and the multiply/add chains (spreading, pre-echo).
- **Cross-platform criterion:** max `|ld_f(ARM64) − ld_f(x86-64)|` over the
  corpus must sit inside the Ld envelope; record the measured bound at M3/M8.
  No claim is made that the two are bit-identical.
- A measured f32-vs-f64 energy-sum error experiment (A-4) at M3 confirms the
  envelope. f64 stays the approved design unless source-based evidence gives a
  concrete reason to change; this is **not** a general f32/f64 redesign.

**Determinism controls:** plain `*`/`+` (no FMA contraction — Rust does not
contract without explicit `mul_add`/target features); no `fast-math`; fixed
operation order mirroring the Q31 loops. Per-arch bit-reproducibility holds for
`*`/`+`/`·2^k`; `log2` may differ in the last ulp across platforms (covered by
the measured Ld envelope, not by a correct-rounding assumption).

## 3. Float tuning constants (`fpe/constants.rs`) — FPE fraction domain

Derived **exactly** from the Q31 tuning literals (`/2^31` is exact for 32-bit
patterns in f64). Tuning, not scaffolding (`03` §5). All are fraction-domain
unless noted:

```
C_RATIO_f        = 0x02940a10 as f64 / 2^31   // ≈0.020143 = 16·10^(−29/10)
CLIP_f_long      = 0x773593ff as f64 / 2^31   // = SfbConfig::clip_energy() long
CLIP_f_short     = (0x773593ff >> 6) as f64 / 2^31
PCM_NOISE_f      = 0x00547062 as f64 / 2^31   // pcm_f[b] = width_b · PCM_NOISE_f
MIN_REMAINING    = 328.0 / 32768.0            // 0x0148 Q15 ≈0.01001 (dimensionless)
MAX_INCREASE     = 2.0                         // dimensionless integer factor
MAXVAL_f         = (2^31−1) as f64 / 2^31     // fraction saturation ≈1.0
LD_ZERO          = −1.0                        // Ld-domain sentinel
THR_LD_FLOOR     = −33.0/64.0 = −0.515625      // Ld-domain floor
THR_SHIFTBITS    = 4                           // ⇒ /16 in fraction domain
PCM_THR_SCALE    = 16                          // energyShift increment (shift count)
```

Per-band tables come from `SfbConfig` **f64 accessors** (already fraction
domain): `pcm_quant()`, `mask_factors()`, `mask_factors_spr()`,
`clip_energy()`. minSnr Ld ints convert `/2^31` where consumed (Ld domain).
**Do not recompute spreading factors or minSnr in float; do not regenerate the
committed tables** (`sfb_tables.rs`, `cfg_*.txt`).

## 4. Rounding and saturation replacement

| C operation (stored domain) | Semantics | FPE fraction replacement |
|---|---|---|
| `fMult(a,b)` | floor(a·b/2^31) | `a_f·b_f`; difference <1 stored LSB |
| `fPow2AddDiv2(t,s)` | t + (s²>>32), wrap | `t_f + s_f·s_f/2`; no wrap (range safe) |
| `scaleValue(v,s)` | arith >> / wrap << | `v_f·2^s` (exact) |
| `scaleValueSaturate` | saturating shift | `v_f·2^s`, clamp at `MAXVAL_f` |
| `>>k` truncation | floor toward −∞ | `/2^k` keeps fraction; truncation only at boundary |
| `nrgAddSaturate` | sat add at MAXVAL_st | integer regroup on stored ints (§6.7) |
| `fLog2` Taylor | ~1e-9 error | `log2` (platform accuracy, §2); existing float-lane convention |

**Principle:** float kernels keep full precision; **all rounding is deferred to
the output boundary** (§7), round-to-nearest (the same convention as
`map_spectrum_to_q31_into`). FPE is strictly more precise than C internally;
deviation from C is C's own truncation noise, bounded and measured (§9).

## 5. Representation lane (retained integer-exact, ADR-0005)

FPE reuses these Q31 helpers **unchanged** to produce the observable shift
decisions, so `mdctScale`/`finalShift`/`tnsShift`/`nrgScaling`/`maxScale` match
the Q31 engine bit-exactly: `map_spectrum_to_q31_into` (`energy.rs:395`),
`max_scale_spec_into` (`:130`), `check_band_energy_into` (`:161`) +
`max_nrg_for_shift` (`:190`) (integer raw sums **for the maxNrg/shift decision
only**), `rescale_shifts` (`:440`), and the integer `shiftBits` decision
(`band_energy_long_into`, `:292-302`). The value lane never feeds back into
these decisions (one-way dependency). Pure-float headroom is Stage 2.

## 6. Per-kernel float formulas (FPE fraction domain)

### 6.0 Domain table (input / internal / output / conversion / rounding)

| Kernel | Input domain | Internal domain | Output domain | Conversion | Rounding/clamp |
|---|---|---|---|---|---|
| Band energy (§6.1) | physical `x` (spec_f64) + `m` | fraction `E_f` | stored `E_st` (i32) + Ld `ld_f` | `E_f=E_real·2^(−2m)`; boundary `·2^31` | f64 sum; boundary round; sat `MAXVAL_f` |
| TNS recompute (§6.2) | physical filtered `x` + `m` | fraction `E_f` | stored `E_st` + Ld | same | shiftBits integer (repr lane) |
| Threshold init (§6.3) | fraction `E_f` | fraction `T_f` | stored `T_st` | `T_f=E_f·C_RATIO_f`; boundary | boundary round |
| Clip (§6.3) | fraction `T_f`, `m` | fraction | fraction | `clip_f=CLIP_f·2^(4−2m)` | branch + `MAXVAL_f` |
| Spreading (§6.4) | fraction `T_f`/`E_f`, `mask_f` | fraction | fraction | none | none (contractive) |
| PCM floor (§6.4) | fraction `T_f`, `pcm_f`, energyShift | fraction | fraction | `floor_f=pcm_f·2^(∓shift)`; `T_f/16` | none |
| Pre-echo (§6.5) | fraction `T_f`, `prev_f`, `m` | fraction | fraction + history | scale align `·2^(∓scaling)` | none |
| MS energies (§6.6) | physical `x_l,x_r` + `m` | fraction `mid_f/side_f` | stored + Ld | `mid_f=M_real·2^(−2m)`; boundary | sat `MAXVAL_f`; grouped MS-Ld exact |
| Grouping (§6.7) | stored per-window ints | **stored ints** | stored grouped ints | quantize first, then integer regroup | `nrgAddSaturate` exact |
| Ld vectors (§6.8) | fraction `v_f` | Ld | `ld_f` (f64) | `log2(v_f)/64` | guard/floor per vector |
| Boundary (§7) | fraction `v_f` | — | stored i32 | `round(v_f·2^31)` | round-to-nearest + i32 clamp |

### 6.1 Band energy (rescale, long & short)

Derivation from `band_nrg.cpp:140-184` + `psy.rs:752-771`: raw
`E_raw_st = Σ(X·2^scale0)²/2^31`, re-shift `>>2(scale0−finalShift)` ⇒
`E_st = Σ X²·2^(2finalShift)/2^31 = E_real·2^(31−2m')`, `m' = m−finalShift`.
Divide by 2^31 for the fraction domain:

```
E_real[b] = Σ_{j∈band b} spec_f64[j]²          // physical, f64 true sum
E_f[b]    = E_real[b] · 2^(−2m')               // FPE fraction
          = Σ (spec_f64[j]·2^(−m'))²           // equivalent: Σ x_f²
E_st[b]   = clamp(round(E_f[b]·2^31))          // boundary (§7)
```

`E_f = E_st/2^31` holds only up to the boundary conversion: the stored value is
round-to-nearest (saturating) at `2^31` scaling, not an exact real-valued
equality. The boundary rounding/saturation term is part of the numerical error
budget in §9. The Q31 reference is additionally computed from the mapped
integer spectrum `X`, so `E_real` and the Q31 stored energy differ by the
input-mapping term and by Q31 square/shift truncation (§9); FPE does not
reproduce that lost sub-LSB information.

The per-band `scale0 = max(0,maxScale−4)` scaffolding **cancels** in the net
fraction value. **Both long and short rescale energies use this formula.**
C's rescale stage calls `CheckBandEnergyOptim` (`band_nrg.cpp:140-184`) for
both window types and applies the same re-shift/Ld correction
(`psy_main.cpp:679-691`). The short-specific
`CalcBandEnergyOptimShort` (`band_nrg.cpp:264-288`,
`maxScale−3`/no `<<1`/saturating rescale) is the **TNS energy recompute**
path (FPE-3.2), not the rescale-energy implementation; its net fraction energy
also reduces to the same `E_real·2^(−2m)` (`Σ X²·2^(2(maxScale−3))/2^32 ·
2^(−(2(maxScale−3)−1)) = Σ X²/2^31`), with the ±31 saturation handled by the
boundary clamp. Ld: §6.8.

### 6.2 TNS energy recompute

After `tns::encode` filters `spec_f64` in place (reused), recompute
`E_real[b] = Σ spec_f64_filtered[j]²` at the same points C recomputes
(`psy.rs:1169-1200`); `E_f = E_real·2^(−2m)` with current `m` (after long
pre-halving `m+=1` and overflow `m+=tnsSpecShift`). Long: `shiftBits` from the
integer repr lane (§5); the `band_energy_long_into` Ld guard
(`ld ≥ (−1>>1)+(scaleDiff>>1) ? ld−scaleDiff : −1`, `energy.rs:280-291`) is
preserved as a float comparison with identical thresholds. Short: no Ld array
(C quirk; short Ld stays at rescale values, `psy.rs:1185-1186`).

### 6.3 Threshold init + clip

```
T_f[b] = E_f[b] · C_RATIO_f                     // fMult → fraction multiply
clipNrgShift = 2·m − THR_SHIFTBITS              // = 2m − 4
headroom = 6 if short else 0
clip_f = if clipNrgShift >= 0          { CLIP_f · 2^(−clipNrgShift) }
         else if clipNrgShift >= −headroom { CLIP_f · 2^(−clipNrgShift) }
         else                          { MAXVAL_f }
T_f[b] = min(T_f[b], clip_f)
```
`CLIP_f` uses the short `>>6` integer when short (`sfb.rs:292-297`). Both
shift branches are the same exact power-of-two scaling in fraction; the branch
only matters for C overflow / the uncapped `MAXVAL_f` case.

### 6.4 Spreading + PCM floor + spread energy

```
// spreading (threshold), two-pass max-envelope, fraction factors:
d = T_f[0]; for i in 1..n { d = max(T_f[i], mask_hi_f[i]·d); T_f[i]=d }
d = T_f[n−1]; for i in (0..n−1).rev { d = max(T_f[i], mask_lo_f[i]·d); T_f[i]=d }

// PCM floor (per window). energyShift starts 2·m, += 16 each window:
energyShift += PCM_THR_SCALE
if energyShift >= 0 { energyShift=min(31,energyShift);
   floor_f = pcm_f[b]·2^(−energyShift) }        // pcm_f = SfbConfig::pcm_quant()
else { energyShift=min(31,−energyShift); floor_f = pcm_f[b]·2^(energyShift) }
T_f[b] = max(T_f[b]/16.0, floor_f)

// spread energy: copy energies then spread with sprEn fraction factors:
S_f[b] = E_f[b]; two-pass envelope with mask_hi_spr_f/mask_lo_spr_f
```
**`pcm_f` is the fraction accessor (`pcm_quant()`, already `/2^31`), NOT
`pcm_q31`.** `T_f/16.0` keeps fractional bits (C `>>4` truncates); ≤1 LSB.

### 6.5 Pre-echo (float temporal masking)

State `FpePreEchoState { prev_f: Vec<f64> (fraction at prev_scale),
prev_scale: i32, enabled: bool }`. **Init:** `prev_f = pcm_f_long`
(`pcm_quant()` long, fraction — the float image of `PreEchoState::new`'s stored
PCM floors, `thresholds.rs:194-202`), `prev_scale = 8`, `enabled = true`.
Per window, current `m`:
```
if !enabled { prev_f[..]=T_f[..]; prev_scale=m; return }   // memorize only
if m > prev_scale {
   scaling = 2·(m−prev_scale)
   for b { cap_f   = 2.0·prev_f[b]·2^(−scaling)
           floor_f = T_f[b]·MIN_REMAINING
           tmp = T_f[b]; prev_f[b]=tmp          // memorize PRE-clamp
           T_f[b] = max(min(tmp,cap_f), floor_f) }
} else {
   scaling = 2·(prev_scale−m)
   for b { cap_f   = 1.0·prev_f[b]              // (MAX_INCREASE>>1)=1
           floor_f = T_f[b]·MIN_REMAINING
           prev_f[b]=T_f[b]
           if T_f[b]·2^(−(scaling+1)) > cap_f { T_f[b]=cap_f·2^(scaling+1) }
           T_f[b]=max(T_f[b],floor_f) }
}
prev_scale = m
```
STOP-reset-before / START-reset-after identical to `thresholds.rs:207-213`
(`prev_f = MAXVAL_f`, `prev_scale=0`, `enabled=false`). **Memorize/clamp/floor
order preserved exactly.** All values fraction domain; scale alignment is
exact power-of-two.

### 6.6 MS energies (fraction, /4 convention)

Net derivation (`band_nrg.cpp:296-361`, `03` §4.12): both C branches reduce to
`mid_st = Σ((X_L+X_R)/2)²/2^31`, i.e. mid real `M_real = Σ((x_l+x_r)/2)²`.
Fraction domain:
```
M_real[b] = Σ ((spec_l[j]+spec_r[j])/2)²        // physical
mid_f[b]  = M_real[b]·2^(−2m) = Σ ((x_f_l[j]+x_f_r[j])/2)²
S_real[b] = Σ ((spec_l[j]−spec_r[j])/2)²
side_f[b] = S_real[b]·2^(−2m)
```
Cap at `MAXVAL_f` mirrors `fMin(Nrg,MAX>>1)<<1` (unreachable for real signals;
boundary clamp guards). `calc_ld = (seq != Short)`: long writes fresh
`mid_ld/side_ld` to `FpeChannel.ms_ld` history (fraction Ld); short leaves
history **stale** and reads it out (C parity). Fresh grouped MS-Ld
post-grouping uses **exact `calc_ld_data`** on quantized grouped MS stored ints
(§6.7) — must stay exact Q31 (downstream short MS decision consumes it).

### 6.7 Grouping (short)

Recommended: quantize per-window fraction energies/thresholds/spread/MS to
stored i32 (§7), then **reuse the integer regroup** (`nrgAddSaturate` window
sums, grouped offsets, maxSfb, spectrum interleave) exactly as
`psy.rs:1285-1414`. Keeps the grouped contract bit-identical given the same
per-window stored ints. Float-sum-then-saturate is equivalent within rounding
but risks grouped MS-Ld exactness — rejected for Stage 1. Grouped MS-Ld =
`calc_ld_data(grouped_ms_stored)` (exact, `psy.rs:1469`).

### 6.8 Ld vector conventions (preserve per-vector guard/floor)

Bases computed from fraction values via `log2(v_f)/64` (the existing float-lane
convention). Corrections are multiples of 1/64 (exact in f64). **Preserve the
guard/sentinel behaviour exactly** — do not collapse to the net formula for
silent bands:

| Vector | Base (fraction) | Guard/correction | Floor |
|---|---|---|---|
| Rescale energy Ld (long) | `log2(E_raw_f)/64 − scale0·2/64` | correction only if `≠−1`; then **unconditional** `+finalShift·2/64` (lifts sentinel: silent → `−1+finalShift·2/64`) | none |
| TNS long energy Ld | `log2(E_raw_f)/64` | `≥(−1>>1)+(scaleDiff>>1) ? −scaleDiff : −1`; then `−shiftBits·2/64` | none |
| Grouped energy Ld | `log2(E_f)/64` | `≤0 → −1` | none |
| Threshold Ld (grouped/long) | `log2(T_f)/64` | `≤0 → −1` | **−33/64** |
| MS Ld (long, fresh) | `log2(mid_f)/64` | `≠−1 ? −scale/64` (no sentinel lift) | none |
| Grouped MS Ld (short) | **exact `calc_ld_data`** of grouped stored | sentinel `i32::MIN` | none |

`E_raw_f` = pre-finalShift fraction energy (`E_real·2^(−2m_pre+2scale0)`); the
rescale guard operates on it exactly as `energy.rs:423-435 rescale_ld_into`
operates on the raw stored int. These conventions are observable in oracle
`NRGLD/GELD/GTLD/TLD/MSMLD` and committed vectors; preserving them is mandatory.

## 7. Output boundary (quantization) — active only for float kernels

`fpe/output.rs` converts fraction values to the shared output contract:
```
stored_i32 = clamp(round(v_f·2^31), i32::MIN, i32::MAX)   // round-to-nearest
```
- Energies/thresholds/spread: `round(E_f·2^31)` etc. (non-negative; saturate at
  `2^31−1` for C saturation parity).
- `energy_ld`/`threshold_ld`: f64 Ld per §6.8 (no quantization).
- `mdct_scale`/`final_shift`/`tns_shift`/`nrg_scaling`/`maxscale`: integer repr
  lane (exact).
- `nm1`: quantize fraction pre-echo history to stored i32 for readout.
- Grouped vectors: integer regroup of quantized per-window values (§6.7).
- `MsOutput.mid/side`: quantized stored; `mid_ld/side_ld`: fraction Ld readout.
- Spectrum: the f64 spectrum (mutated by TNS) passes through unchanged; the
  driver re-maps to Q31 via `map_spectrum_to_q31` as today
  (`stream_driver.rs:929`).

**M2 pass-through contract (P2, rev 2).** In M2 the value lane **delegates to
the Q31 kernels**, which already produce stored `i32` and f64 Ld. M2 passes
those results through **unchanged**: **no** fraction conversion, **no**
`round(v_f·2^31)`, **no** normalization or rescaling is applied. The boundary
quantization above activates **only** in M3+ when a specific kernel is replaced
by float arithmetic. The M2 gate additionally requires **bit-identical
frame-surviving persistent state after every frame** of multi-frame sequences
(`PreEchoState`, `tns_state`/`tns_info`, `ms_ld`), compared via `#[cfg(test)]`
helpers (no new production API). See `09` M2, `04` §4/§6, `06` §5, `07` §3.1/§5,
ADR-0004.

## 8. Invariants and defensive checks

| Invariant | Check | When |
|---|---|---|
| All float values finite | `is_finite()` on energies/thresholds/history | debug assert; release boundary guard |
| Energies ≥ 0, thresholds ≥ 0 | clamp at boundary; debug assert | boundary |
| Stored ints in i32 range | saturating clamp in quantization | boundary |
| Fraction values within `[−MAXVAL_f, MAXVAL_f]` where C saturates | clamp | kernels/boundary |
| Ld sentinel for non-positive | `≤0 → −1.0` | Ld construction |
| Threshold-Ld floor | `max(ld, −33/64)` | threshold Ld |
| No NaN/Inf propagation | non-finite → loud release-active boundary assertion (`fraction_to_stored`); frame-level `FpeError` skip-with-note path deferred to FPE-3.1R2 | boundary/orchestration |
| Grouped stride layout | lengths == `groups·sfb_cnt`, padding zeros | output assembly |
| D-022 preconditions | LFE Long, CPE seq agreement | entry (same asserts as Q31) |

A numerical-invariant violation at the FPE energy boundary is a loud
release-active assertion (`fraction_to_stored`); the frame-level
skip-with-note/`FpeError` taxonomy remains deferred to FPE-3.1R2
(`06` §6). It never silently corrupts output.

## 9. Numerical differences and error budget (measurable)

> **Owner decision (FPE-3.1R1, Option A).** Ferrite FPE uses physical f64
> spectrum values for psychoacoustic energy calculations. Q31 input-mapping
> and truncation differences are expected numerical divergences, not
> implementation errors. Differential validation must account for these
> differences using mathematically derived, per-band error bounds. Q31 remains
> the reference engine, but bit-level agreement is not the definition of FPE
> correctness.

### 9.1 Error sources (kept distinct)

Observed Q31↔FPE differences are **not** all "floating-point error". They are
separated into:

| Source | Domain | Nature / bound |
|---|---|---|
| Physical spectrum information | physical f64 `x` | FPE's intended input; not an error |
| Q31 input mapping | `X = round(x·2^(31−m))`, `q = X·2^(m−31)` | `|x−q| ≤ 2^(m−32)` per line (half an LSB), if no clamp |
| Q31 per-square truncation | `fPow2AddDiv2` floor (`s²>>32`) | `0 ≤ ρ_j < 1` per line in the shifted-square domain |
| Q31 rescale shift/floor | `e_raw >> s_b` | floor toward −∞; amplified for negative `s_b` |
| FPE boundary rounding | `round(E_f·2^31)` | `|ρ| ≤ 0.5` stored LSB, plus saturation |
| f64 arithmetic | sums/multiplies | ~`n_b·2^-53` relative; below the terms above |
| Genuine implementation defect | any | not bounded by this policy; must fail loudly |

Q31 is the **reference engine**, but bit-level agreement is not the definition
of FPE correctness: FPE deliberately retains physical spectrum information
that Q31 mapping discards.

### 9.2 Approved Q31↔FPE energy envelope (owner-approved, FPE-3.1R1)

For a band `b` of width `n_b`, with `s0_b = max(0, maxScale_b−4)` and
`s_b = min(2(s0_b − finalShift), 31)` (signed; negative values preserve the
left-shift amplification), define:

```
Q_b = 2^(2·finalShift−31) · Σ_j |X_j|
      + n_b · 2^(2·finalShift−33)
T_b = 2·n_b · 2^(−s_b)
R   = 1.5

tol_E(b) = max(32, Q_b + T_b + 1.5) + E_ref/500
```

- `Q_b` is the first- and second-order **input-mapping** contribution:
  `Σ(2X_jε_j + ε_j²)` with `|ε_j| ≤ 1/2` in Q31 units, scaled to stored
  energy.
- `T_b` is the **per-square truncation and re-shift floor** contribution,
  including the negative-`s_b` amplification.
- `R = 1.5` covers the re-shift floor and the FPE round-to-nearest boundary.
- The existing absolute floor and relative component are preserved.
- The bound is computed **per band from the actual mapped/physical spectrum
  and representation state**; it is not a global tolerance.

**Assumptions and guards.**
- The half-LSB mapping bound assumes `X` did not saturate. The implementation
  detects mapping saturation (`|round(x·2^(31−m))|` outside `i32` range) and
  reports such comparisons as unsupported/invalid instead of applying the
  bound.
- The approved `Q_b` uses the unclamped re-shift exponent. When
  `2(s0_b−finalShift) > 31`, the implementation treats a non-trivial energy
  comparison as unsupported (the reachable corpus case has both stored values
  zero) rather than silently applying an under-estimated bound.
- **Accumulation/re-shift supported-domain precondition.** Production
  `fPow2AddDiv2` wraps the 32-bit accumulator and `e_raw = tmp << 1` wraps.
  With `s0_b = max(0, maxScale_b−4)`, the envelope is valid only when:
  1. every `s_j = X_j·2^(s0_b)` is representable in `i32` (no left-shift wrap);
  2. `A_b = Σ_j floor(s_j²/2^32) ≤ i32::MAX >> 1`, so both `tmp` and
     `tmp << 1` fit (partial sums are monotone, which also excludes
     intermediate accumulation wrap);
  3. for `s_b < 0`, `e_raw·2^(−s_b)` is representable in `i32` (negative-shift
     amplification does not overflow).
  The implementation computes the non-negative accumulation sum in `i64`,
  rejects any nonzero left shift with `s0_b ≥ 32` before shifting, and uses
  `i128` for the re-shift product comparison; it marks the band
  **unsupported** otherwise. The `s0_b = max(0, maxScale_b−4)` derivation uses
  a wide intermediate so the full declared `i32` `maxScale` domain is
  panic-free. Unsupported comparisons fail the audit and cannot count as
  passing. `maxScale_b ≥ 4` with band width `≤ 96` is a sufficient shortcut:
  `A < 2^(31−maxScale)` gives `|s_j| < 2^27`, `u_j < 2^22`, and
  `A_b < 2^29`. It is not necessary and is not used as a blind gate: the exact
  detector also covers `maxScale_b < 4` and the `<<1` boundary.
- Exponents are evaluated as exact powers of two in f64; `|s_b| ≤ 31` on the
  corpus, `n_b ≤ 96`, and `2^(2·finalShift−31) ≤ 2^-1`, so no overflow or
  underflow is reachable. Integer shifts mirror the Q31 source exactly
  (`wrapping_shl`, arithmetic right shift, wrapping accumulation).

### 9.3 Application

- The derived envelope applies to **long and short rescale energies**, to
  **spread energy** (propagated through the contractive spreading envelope),
  and to **grouped energy** (sum of the per-window envelopes; grouped
  saturation is detected and reported).
- `q31_tol = 32 + |reference|/500` is retained for thresholds, `nm1`,
  grouped thresholds, and reused Q31-kernel outputs (e.g. the TNS recompute
  before FPE-3.2). For fields downstream of the float energy, a propagated
  bound is used only when it is **no greater** than `q31_tol`; otherwise the
  larger derived bound is reported explicitly and must be justified.
- Ld vectors are compared by energy-domain inversion using the derived energy
  envelope; absolute Ld differences are measured and reported separately for
  per-window rescale Ld, grouped Ld and threshold Ld. `q31_tol` is not applied
  directly to Ld values. The §6.8 sentinel/correction conventions are
  preserved.

### 9.4 Sentinel/divergence classes

Expected Q31↔FPE differences at very low energy are classified by stored
state, not by Ld crossing `−1.0`:

- **Class A — mapping-to-zero:** physical band energy is positive, **every**
  mapped Q31 line is zero, Q31 stored energy is zero; FPE stored energy may be
  positive, and the FPE Ld resolves the physical energy.
- **Class B — post-shift truncation vs FPE rounding:** the mapped Q31 spectrum
  contains nonzero lines, Q31 rescale shifting/flooring produces zero stored
  energy, and FPE round-to-nearest produces positive stored energy. Grouped Ld
  can differ substantially because grouped stored values are summed before Ld.

These are expected divergences; FPE must **not** be forced to reproduce Q31's
truncation. Both classes are counted and reported per vector and per window
type.

### 9.5 C↔FPE policy (Tier-B)

When C reference artifacts are available, the triangle-inequality policy is:

```
tol_C_FPE = q31_tol + Q_b + T_b + 1.5
```

This accounts for the validated C↔Q31 difference plus the Q31↔FPE derived
terms. It is **policy documentation**; Tier-B remains BLOCKED/INCOMPLETE until
the reference checkout, oracle dumps and decoder artifacts are available, and
no Tier-B pass may be claimed without them.

### 9.6 Downstream decision safety

The derived energy envelope establishes **energy-value equivalence only**.
Downstream decision safety (IS books/scale/digest, MS mask/digest, PE/ATS
evolution, S2 VBR adaptation/avoid-hole, S4 scalefactors, S6 allocation, final
frame accounting, margin-based flip classification, quiet short-window
sentinel/PE) is **not** covered by this task. It is required before FPE-3.1
can be accepted and is implemented in **FPE-3.1R2**. The milestone remains
blocked until R2 and independent review complete.

### 9.7 Outstanding numerical experiments

- **A-4**: broader f32-vs-f64 energy-sum experiment — outstanding (FPE-3.3);
  production remains f64.
- **A-5**: cross-architecture `log2`/Ld measurement (x86-64 supplement) —
  outstanding; Apple Silicon results are not cross-architecture validation.

## 10. What is NOT translated (stays integer / reused)

- Representation-lane headroom/shift decisions (§5).
- TNS autocorrelation (exact integer, validated bit-exact; float cannot
  reproduce C, `tns.rs:103-111`).
- Grouped MS-Ld (`calc_ld_data`, exact Q31).
- All downstream stages (PE/S2/S4/S5/S6/S8) and IS/MS decisions.
- Spreading-factor / minSnr re-derivation (tables consumed, never regenerated).
- Tonality / PNS / ATH (absent, stay absent).