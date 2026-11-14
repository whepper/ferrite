/* Ferrite: this file is part of a Third-Party Modified Version of the
Fraunhofer FDK AAC Codec Library for Android. Modifications and Rust
implementation copyright © 2025–2026 Jeroen Steenvoorde. Modification
notice date: 2026-10-26. See rust/NOTICE for the full license text. */
/* -----------------------------------------------------------------------------
Software License for The Fraunhofer FDK AAC Codec Library for Android

© Copyright 2025 Fraunhofer-Gesellschaft zur Förderung der angewandten Forschung
e.V. All rights reserved.

 1.    INTRODUCTION
The Fraunhofer FDK AAC Codec Library for Android ("FDK AAC Codec") is software
that implements the MPEG Advanced Audio Coding ("AAC") encoding and decoding
scheme for digital audio. This FDK AAC Codec software is intended to be used on
a wide variety of Android devices.

AAC's HE-AAC and HE-AAC v2 versions are regarded as today's most efficient
general perceptual audio codecs. AAC-ELD is considered the best-performing
full-bandwidth communications codec by independent studies and is widely
deployed. AAC has been standardized by ISO and IEC as part of the MPEG
specifications.

Patent licenses for necessary patent claims for the FDK AAC Codec (including
those of Fraunhofer) may be obtained through Via Licensing
(www.vialicensing.com) or through the respective patent owners individually for
the purpose of encoding or decoding bit streams in products that are compliant
with the ISO/IEC MPEG audio standards. Please note that most manufacturers of
Android devices already license these patent claims through Via Licensing or
directly from the patent owners, and therefore FDK AAC Codec software may
already be covered under those patent licenses when it is used for those
licensed purposes only.

Commercially-licensed AAC software libraries, including floating-point versions
with enhanced sound quality, are also available from Fraunhofer. Users are
encouraged to check the Fraunhofer website for additional applications
information and documentation.

 2.    COPYRIGHT LICENSE

Redistribution and use in source and binary forms, with or without modification,
are permitted without payment of copyright license fees provided that you
satisfy the following conditions:

You must retain the complete text of this software license in redistributions of
the FDK AAC Codec or your modifications thereto in source code form.

You must retain the complete text of this software license in the documentation
and/or other materials provided with redistributions of the FDK AAC Codec or
your modifications thereto in binary form. You must make available free of
charge copies of the complete source code of the FDK AAC Codec and your
modifications thereto to recipients of copies in binary form.

The name of Fraunhofer may not be used to endorse or promote products derived
from this library without prior written permission.

You may not charge copyright license fees for anyone to use, copy or distribute
the FDK AAC Codec software or your modifications thereto.

Your modified versions of the FDK AAC Codec must carry prominent notices stating
that you changed the software and the date of any change. For modified versions
of the FDK AAC Codec, the term "Fraunhofer FDK AAC Codec Library for Android"
must be replaced by the term "Third-Party Modified Version of the Fraunhofer FDK
AAC Codec Library for Android."

 3.    NO PATENT LICENSE

NO EXPRESS OR IMPLIED LICENSES TO ANY PATENT CLAIMS, including without
limitation the patents of Fraunhofer, ARE GRANTED BY THIS SOFTWARE LICENSE.
Fraunhofer provides no warranty of patent non-infringement with respect to this
software.

You may use this FDK AAC Codec software or modifications thereto only for
purposes that are authorized by appropriate patent licenses.

 4.    DISCLAIMER

This FDK AAC Codec software is provided by Fraunhofer on behalf of the copyright
holders and contributors "AS IS" and WITHOUT ANY EXPRESS OR IMPLIED WARRANTIES,
including but not limited to the implied warranties of merchantability and
fitness for a particular purpose. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
CONTRIBUTORS BE LIABLE for any direct, indirect, incidental, special, exemplary,
or consequential damages, including but not limited to procurement of substitute
goods or services; loss of use, data, or profits, or business interruption,
however caused and on any theory of liability, whether in contract, strict
liability, or tort (including negligence), arising in any way out of the use of
this software, even if advised of the possibility of such damage.

 5.    CONTACT INFORMATION

Fraunhofer Institute for Integrated Circuits IIS
Attention: Audio and Multimedia Departments - FDK AAC LL
Am Wolfsmantel 33
91058 Erlangen, Germany

www.iis.fraunhofer.de/amm
amm-info@iis.fraunhofer.de
----------------------------------------------------------------------------- */
//! M4-S3 scalar quantizer primitives: exact integer mirror of
//! `cpp/libAACenc/src/quantize.cpp` (at base commit `1785529`).
//!
//! ## Reference audit
//!
//! Ported functions and source locations:
//!
//! - `quantize_line` = `FDKaacEnc_quantizeLines` (`quantize.cpp:116-169`).
//! - `inv_quantize_line` = `FDKaacEnc_invQuantizeLines` (`:180-267`).
//! - `quantize_spectrum` = `FDKaacEnc_QuantizeSpectrum` (`:278-301`).
//! - `calc_sfb_dist` = `FDKaacEnc_calcSfbDist` (`:312-349`).
//! - `calc_sfb_quant_energy_and_dist` = `FDKaacEnc_calcSfbQuantEnergyAndDist`
//!   (`:361-401`).
//! - `MAX_QUANT = 8191` (`quantize.h:110`); dead-zone adds `QUANT_K0/K1`
//!   (`quantize.cpp:125-128`, C-probed literals in [`crate::quant_tables`]).
//!
//! ## Input/output types, units, numeric representation
//!
//! - Spectral input: Q31 `i32` (`FIXP_DBL`; `LONG` is 32-bit on every FDK
//!   target, `machine_type.h:176-186`).
//! - Quantized output: `i16` (`SHORT`), one per spectral line.
//! - `gain` (QSS) is a plain integer exponent offset, **not** Q31:
//!   `quantize_spectrum` passes `globalGain - scf` per band
//!   (`quantize.cpp:296`), i.e. `spec^(3/4) * 2^(-3/16*QSS) * k`-style
//!   scaling is folded into the `quantTableQ/E` + shift pipeline, never
//!   evaluated as a float.
//! - Distortion/energy outputs: Ld-domain Q31 `i32` (`log2*64`,
//!   `fixpoint_math.h:113-114`); the `-1.0` sentinel (`i32::MIN`) marks
//!   zero accumulated distortion/energy, exactly as `CalcLdData` does.
//!
//! ## Scalefactor / step-size / quantized-value relationship
//!
//! Per band, `QSS = globalGain - scalefactor[band]`; the quantizer line
//! computes `q = round vendorspec(|spec|^(3/4) * 2^(-3/16*QSS)) + k)`
//! with the `2^(-3/16*QSS)` factor split into `quantTableQ[(-QSS)&3]`
//! (fractional part, `2^(0.25q)/2`) plus an integer `quantizer_shift`
//! (`((-QSS)>>2)+1`), combined with the `x^0.75` mantissa table
//! `MTAB_3_4` and `quantTableE` (`2^(0.75e)/8`). Negative C shifts/ands
//! use two's-complement wrapping (`wrapping_neg`, arithmetic `>>`),
//! identical to the reference on every target.
//!
//! ## Rounding and clipping
//!
//! - Rounding is truncation toward zero of `(k + accu) >> 15`: the dead
//!   zone `k` (`QUANT_K0 = 13284` for LC, i.e. `dZoneQuantEnable = 0`)
//!   shifts the decision boundary; magnitudes below the zone quantize to
//!   0. There is no `+0.5`-style round-half-up anywhere.
//! - Sign handling: the negative branch mirrors the magnitude pipeline on
//!   `|accu|` and negates the truncated result, so `q(-x) == -q(x)` where
//!   the first-stage `fixmuldiv2_DS` truncation is sign-symmetric
//!   (structurally: spectra divisible by 2^16; pinned by
//!   `quantize_sign_symmetry`). Other magnitudes can differ by the DS
//!   floor LSB — identically in C — and are pinned bit-exactly by the
//!   probe transcript instead.
//! - `quantize_line` never clips: out-of-range magnitudes wrap through
//!   the `as i16` narrowing exactly like the C `(SHORT)` conversion.
//!   Range enforcement is the caller's job (`calcMaxValueInSfb`, S6).
//! - The distortion functions early-out `(0, 0)` / `0.0` when any
//!   `|q| > MAX_QUANT` is observed (`:327-329,373-377`).
//!
//! ## Tables and initialization
//!
//! All tables live in [`crate::quant_tables`], script-extracted from the
//! reference build and cross-checked against the ROM source text; no
//! runtime initialization is required. `MTAB_3_4`/`QUANT_TABLE_Q/E` are
//! 16-bit (`FIXP_QTD == FIXP_SGL`) because this target — like every
//! 32x16 arch (arm64 and x86, `FDK_archdef.h:182-223`) — defines
//! `ARCH_PREFER_MULT_32x16`; the probe asserts `qtd_bytes=2`.
//!
//! ## Scalar vs vectorized vs architecture-specific
//!
//! The reference is a scalar per-line loop. It IS architecture-sensitive
//! in two places, both pinned to the arm64/x86-64 build observed here:
//!
//! - `fMultDiv2(spec:DBL, quantizer:SGL)` = `fixmuldiv2_DS`, identical on
//!   all arches: `((q<<16)*spec)>>32` (`fixmul.h:183-195`).
//! - `fMultDiv2(mtab:SGL, tableE:SGL)` = `fixmuldiv2_SS`: a plain 32-bit
//!   product with NO shift (`fixmul.h:157-162`).
//! - `fMult(s, t)` (inverse path) uses the ARM64 `smull; asr #31` floor
//!   variant shared with `q31::fmult` (`fixmul_arm.h:174-193`); the x86
//!   `shl edx,1` variant drops the LSB and would disagree by 1 in rare
//!   cases, so fixtures generated here pin the ARM64 behavior.
//! - `fPow2` (distortion path) = `fixmuldiv2_DD(a,a)<<1` on ALL arches
//!   (no arch override exists): `(((a*a)>>32)<<1)`, which clears bit 0
//!   unlike `fMult`. A dedicated [`fpow2`] helper keeps the two apart.
//!
//! ## S3 vs S4 boundary
//!
//! S3 owns the five primitives above plus tables. Scalefactor search
//! (`improveScf`, assimilations, global-gain inversion, dead-band
//! zeroing in `sf_estim.cpp`) is S4 and calls into S3 via `calc_sfb_dist`
//! (`sf_estim.cpp:390-1226`) and `calc_sfb_quant_energy_and_dist`
//! (`:1010`). The QC gain loop (`QuantizeSpectrum` + `calcMaxValueInSfb`
//! per `qc_main.cpp:951-967`) and all bit counting are S5/S6.
//!
//! ## Downstream dependence
//!
//! S4's NMR comparisons consume `calc_sfb_dist` Ld outputs directly, and
//! S6's retry logic consumes the wrapped `i16` spectrum via
//! `calcMaxValueInSfb`. Both require bit-exact integers here; float
//! "equivalents" are not acceptable.
//!
//! ## Verified facts vs inferences
//!
//! - [Fact] Every mapping above was read from the cited source lines.
//! - [Fact] All table literals are C-probed (`quant_probe`, `QTARCH`
//!   `qtd_bytes=2`) and independently recomputed from `aacEnc_rom.cpp`.
//! - [Fact] `dZoneQuantEnable = 0` for LC (`qc_main.cpp:399-411` with
//!   `isLowDelay = 0`); the `=1` path is ported (shared `k` selection)
//!   but only probe-covered, never corpus-covered.
//! - [Fixed] `q == -32768` slipped every reference `|q| > MAX_QUANT` guard
//!   (`fixabs_S` wraps) and then read out of bounds in `invQuantizeLines`
//!   (`specExp = 16 >= 14`). The port widens to `i32` before `abs`, so the
//!   sentinel takes the documented distortion-function early-out instead of
//!   reaching `inv_quantize_line`. Committed fixtures are scanned to exclude
//!   the value; S6's `calcMaxValueInSfb` gate asserts loudly on it (see
//!   `rate.rs`), unchanged here.
//!
//! ## Valid input envelope
//!
//! Committed fixtures satisfy, per line (the probe aborts otherwise, so
//! this holds by construction, on x86 too where the asserts compile in):
//!
//! - quantize path: recomputed `totalShift = 12 - 3*(T>>2) >= 0` with
//!   `T = quantizer_shift - accu_shift + 1` (the `MAX_QUANT_VIOLATION`
//!   assert, `quantize.cpp:146,162`);
//! - inverse path: every line that reaches `invQuantizeLines` satisfies
//!   `|q| <= MAX_QUANT` (the `specExp < 14` assert, `:204,240`);
//!   larger magnitudes take the distortion-function early-out first and
//!   never arrive here, including the former slipped `-32768` (see quirk
//!   above), which committed fixtures additionally exclude by scan.

use super::q31::{calc_ld_data, count_leading_bits, count_leading_zeros, fmult, shift_int};
use super::quant_tables::{
    MTAB_3_4, MTAB_4_3, QUANT_K0, QUANT_K1, QUANT_TABLE_E, QUANT_TABLE_Q, SPEC_EXP_MANT,
    SPEC_EXP_SHIFT,
};

/// Largest representable quantized magnitude (`quantize.h:110`).
/// Anything larger fails the S6 `calcMaxValueInSfb` gate and early-outs
/// the distortion functions.
pub const MAX_QUANT: i32 = 8191;

/// `fixmuldiv2_SS`: 16x16-bit fixed-point multiply-div2 is a plain
/// 32-bit product with no shift (`fixmul.h:157-162`).
fn fixmuldiv2_ss(a: i16, b: i16) -> i32 {
    a as i32 * b as i32
}

/// `fixmuldiv2_DS` as executed on this target: `fixmuldiv2_SD(b, a)` =
/// `fixmuldiv2_DD(b<<16, a)` (`fixmul.h:183-195` with the ARM
/// `fixmuldiv2_SD` override, `fixmul_arm.h:156-162`). The generic
/// fallback computes the identical value.
fn fixmuldiv2_ds(a: i32, b: i16) -> i32 {
    ((((b as i32) << 16) as i64 * a as i64) >> 32) as i32
}

/// `fPow2(a) = fixpow2_D(a)` (`fixmul.h:280-283`): `(((a*a)>>32)<<1)`.
/// No architecture overrides this path, so bit 0 is always cleared —
/// unlike [`fmult`], which keeps it (ARM64 `asr #31`). The two must
/// never be interchanged; the distortion accumulations depend on it.
pub(crate) fn fpow2(a: i32) -> i32 {
    (((a as i64 * a as i64) >> 32) << 1) as i32
}

/// `FDKaacEnc_quantizeLines` for a single line (`quantize.cpp:116-169`).
///
/// `gain` is the band QSS (`globalGain - scf`); `spec` is Q31;
/// `dzone_quant_enable` selects the dead-zone addend (`false` = LC).
/// The output wraps through `as i16` exactly like the C `(SHORT)` cast:
/// no clipping happens here.
///
/// M5.16: the band-invariant quantizer state (`quantizer`,
/// `quantizer_shift`, `k` — pure functions of `gain` and
/// `dzone_quant_enable`, computed once per band by the C reference at
/// `quantize.cpp:120-131`) now lives in [`quantize_line_body`]; this
/// wrapper computes it and delegates. Per-line arithmetic is unchanged.
#[cfg(test)]
pub fn quantize_line(gain: i32, spec: i32, dzone_quant_enable: bool) -> i16 {
    let quantizer = QUANT_TABLE_Q[(gain.wrapping_neg() & 3) as usize];
    let quantizer_shift = (gain.wrapping_neg() >> 2) + 1;
    let k = if dzone_quant_enable {
        QUANT_K1
    } else {
        QUANT_K0
    };
    quantize_line_body(quantizer, quantizer_shift, k, spec)
}

/// Per-line quantizer arithmetic over precomputed band-invariant state
/// (`quantizer`, `quantizer_shift`, `k` — see [`quantize_line`]). This is
/// the exact former body of [`quantize_line`]; every operation, order,
/// wrapping behavior, and the `as i16` narrowing are identical. The split
/// exists so [`calc_sfb_dist`] can hoist the invariant state out of its
/// line loop (mirroring `FDKaacEnc_quantizeLines`) without changing any
/// per-line computation.
///
/// `always`: the plain `#[inline]` hint was measured insufficient —
/// release LLVM's size cost model kept an out-of-line per-line call from
/// `calc_sfb_dist`'s loop (verified in M5.16 Gate D disassembly), which
/// is exactly the code shape this kernel exists to remove. Forcing the
/// inline is safe: the body is straight-line integer code with no
/// recursion, and Gate D verifies the resulting shape.
#[inline(always)]
pub(crate) fn quantize_line_body(quantizer: i16, quantizer_shift: i32, k: i32, spec: i32) -> i16 {
    let accu = fixmuldiv2_ds(spec, quantizer);
    if accu == 0 {
        return 0;
    }
    // One magnitude path for both signs; only the final result is negated.
    // This is the arithmetic the former two-branch body performed:
    // `wrapping_neg` for negative input, identity otherwise, and the same
    // final `wrapping_neg` on the emitted magnitude.
    let neg = accu < 0;
    let mut accu = if neg { accu.wrapping_neg() } else { accu };
    /* normalize: leading one lands on bit 30 ([0.5, 1.0)). */
    let accu_shift = count_leading_zeros(accu) - 1;
    accu = accu.wrapping_shl(accu_shift as u32);
    // Normalization puts the leading one at bit 30, so `accu >> 21`
    // is 0..=0x3FF; masking to nine bits is the same index as the
    // former `& !512` and bounds the 512-entry table for LLVM.
    let tab_index = ((accu >> (32 - 2 - 9)) & 0x1FF) as usize;
    debug_assert!(tab_index < 512);
    let mut total_shift = quantizer_shift - accu_shift + 1;
    accu = fixmuldiv2_ss(
        MTAB_3_4[tab_index],
        QUANT_TABLE_E[(total_shift & 3) as usize],
    );
    total_shift = (16 - 4) - (3 * (total_shift >> 2));
    debug_assert!(total_shift >= 0, "MAX_QUANT_VIOLATION envelope");
    accu >>= total_shift.min(31);
    let q = (k.wrapping_add(accu) >> (32 - 1 - 16)) as i16;
    if neg {
        q.wrapping_neg()
    } else {
        q
    }
}

/// `FDKaacEnc_invQuantizeLines` for a single line (`quantize.cpp:180-267`).
///
/// Caller precondition (mirroring the reference asserts): `|q| <=
/// MAX_QUANT`. Larger magnitudes take the distortion-function early-out
/// before ever reaching this function; feeding them here panics loudly
/// instead of performing C's out-of-bounds table read (see module docs).
pub fn inv_quantize_line(gain: i32, q: i16) -> i32 {
    if q == 0 {
        return 0;
    }
    let neg = q < 0;
    /* The two C branches differ: negative negates after promotion to INT,
     * positive passes through unchanged (quantize.cpp:198 vs 233). */
    let mut accu = if neg { -(q as i32) } else { q as i32 };
    let ex = count_leading_bits(accu);
    accu = accu.wrapping_shl(ex as u32);
    let spec_exp = (32 - 1) - ex;
    assert!((spec_exp as u32) < 14, "specExp envelope: |q| <= MAX_QUANT");
    // `count_leading_bits` + shift leaves the leading one at bit 30, so
    // `accu >> 21` is 0..=0x3FF and the nine-bit mask is the same index
    // as the former `& !512` while bounding the 512-entry table.
    let tab_index = ((accu >> (32 - 2 - 9)) & 0x1FF) as usize;
    debug_assert!(tab_index < 512);
    let m = (gain & 3) as usize;
    let s = MTAB_4_3[tab_index];
    let t = SPEC_EXP_MANT[m][spec_exp as usize];
    accu = fmult(s, t);
    /* -1 avoids overflows in accu (quantize.cpp:220,255). */
    let se = SPEC_EXP_SHIFT[m][spec_exp as usize] as i32 - 1;
    let ishift = gain >> 2;
    let c = ishift.wrapping_neg().wrapping_sub(se);
    if c < 0 {
        /* Hardware shift counts mask (ARM LSLV / x86 sal); wrapping_shl
         * matches on both. Counts beyond the mask are unreachable
         * inside the valid gain envelope. */
        accu = accu.wrapping_shl(c.wrapping_neg() as u32);
    } else {
        accu = accu.wrapping_shr(c as u32);
    }
    if neg {
        accu.wrapping_neg()
    } else {
        accu
    }
}

/// `FDKaacEnc_QuantizeSpectrum` (`quantize.cpp:278-301`): per-band
/// `QSS = global_gain - scf[band]`, then the per-line quantizer over the
/// band.
///
/// M5.16: the band-invariant quantizer state for each band's `QSS` is
/// computed once per band — the exact structure of the C reference
/// (`QuantizeSpectrum` calls `FDKaacEnc_quantizeLines` per band, which
/// computes `quantizer`/`quantizershift`/`k` before its line loop,
/// `quantize.cpp:283-297`). The per-line call now goes through
/// [`quantize_line_body`] with identical arguments, so every emitted
/// value matches `quantize_line(qss, spec[line], dzone_quant_enable)`
/// bit-for-bit.
#[allow(clippy::too_many_arguments)]
pub fn quantize_spectrum(
    cnt: usize,
    max_per: usize,
    per: usize,
    off: &[i32],
    spec: &[i32],
    global_gain: i32,
    scf: &[i32],
    out: &mut [i16],
    dzone_quant_enable: bool,
) {
    let mut sfb_offs = 0;
    while sfb_offs < cnt {
        for sfb in 0..max_per {
            let qss = global_gain.wrapping_sub(scf[sfb_offs + sfb]);
            #[cfg_attr(target_arch = "aarch64", allow(unused_variables))]
            let quantizer = QUANT_TABLE_Q[(qss.wrapping_neg() & 3) as usize];
            let quantizer_shift = (qss.wrapping_neg() >> 2) + 1;
            let k = if dzone_quant_enable {
                QUANT_K1
            } else {
                QUANT_K0
            };
            let start = off[sfb_offs + sfb] as usize;
            let end = off[sfb_offs + sfb + 1] as usize;
            #[cfg(target_arch = "aarch64")]
            {
                // SAFETY: NEON is part of the AArch64 baseline; the kernel
                // reads/writes exactly `end - start` lines and falls back to
                // `quantize_line_body` for tails.
                unsafe {
                    super::quant_neon::quantize_lines_neon(
                        &spec[start..end],
                        &mut out[start..end],
                        qss,
                        quantizer_shift,
                        k,
                    );
                }
            }
            #[cfg(target_arch = "x86_64")]
            {
                // Runtime dispatch: AVX2 kernels only after the feature check;
                // the `#[target_feature]` + `#[inline(never)]` entry points keep
                // AVX2 instructions out of the baseline path (see quant_avx2).
                if std::arch::is_x86_feature_detected!("avx2") {
                    #[cfg(test)]
                    avx2_dispatch_observe::record(avx2_dispatch_observe::QUANTIZE);
                    // SAFETY: feature-checked above; the kernel reads/writes
                    // exactly `end - start` lines and falls back to
                    // `quantize_line_body` for tails.
                    unsafe {
                        super::quant_avx2::quantize_lines_avx2(
                            &spec[start..end],
                            &mut out[start..end],
                            qss,
                            quantizer_shift,
                            k,
                        );
                    }
                } else {
                    quantize_lines_scalar(
                        &spec[start..end],
                        &mut out[start..end],
                        quantizer,
                        quantizer_shift,
                        k,
                    );
                }
            }
            #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
            {
                quantize_lines_scalar(
                    &spec[start..end],
                    &mut out[start..end],
                    quantizer,
                    quantizer_shift,
                    k,
                );
            }
        }
        sfb_offs += per;
    }
}

pub(crate) fn quantize_lines_scalar(
    spec: &[i32],
    quant: &mut [i16],
    quantizer: i16,
    quantizer_shift: i32,
    k: i32,
) {
    for (s, q_out) in spec.iter().zip(quant.iter_mut()) {
        *q_out = quantize_line_body(quantizer, quantizer_shift, k, *s);
    }
}

/// Squared-error distortion of one band in the `>>1` comparison domain
/// (`quantize.cpp:334-344`): `diff = ||inv| - |spec>>1||`, squared via
/// [`fpow2`] after `CountLeadingBits` renormalization, accumulated with
/// wrap. Shared by both distortion entry points.
pub(crate) fn accumulate_dist(acc: i32, inv: i32, spec: i32) -> i32 {
    let mut diff = (inv.wrapping_abs().wrapping_sub((spec >> 1).wrapping_abs())).wrapping_abs();
    let scale = count_leading_bits(diff);
    diff = shift_int(diff, scale);
    diff = fpow2(diff);
    let scale2 = (2 * (scale - 1)).min(32 - 1);
    diff = shift_int(diff, -scale2);
    acc.wrapping_add(diff)
}

/// `FDKaacEnc_calcSfbDist` (`quantize.cpp:312-349`): re-quantizes every
/// line with `gain`, early-outs `0.0` (Q31 0, NOT the Ld sentinel) when
/// any fresh `|q| > MAX_QUANT`, else inverse-quantizes and returns the
/// Ld of the accumulated distortion.
///
/// The guard widens to `i32` before `abs`, so the slipped reference
/// value (`-32768`, which `fixabs_S`/`wrapping_abs` keeps negative) takes
/// the documented early-out instead of reaching `inv_quantize_line`.
/// `quant` receives the fresh quantized lines (C quantizes in place).
///
/// M5.16: the band-invariant quantizer state (`quantizer`,
/// `quantizer_shift`, `k`) is computed once per band here — exactly like
/// the C reference's `FDKaacEnc_quantizeLines` prologue (`quantize.cpp:120-131`)
/// — and the per-line work runs through [`quantize_line_body`]. The
/// values are identical to the former per-line computation (pure
/// functions of `gain`/`dzone_quant_enable`), so every emitted `q`,
/// the early-out, the inverse quantization, and the distortion
/// accumulation are unchanged.
pub fn calc_sfb_dist(spec: &[i32], gain: i32, dzone_quant_enable: bool, quant: &mut [i16]) -> i32 {
    // All callers pass paired band slices. State the contract once so the
    // line loop can carry no per-line `quant[i]` bounds check.
    assert_eq!(
        spec.len(),
        quant.len(),
        "calc_sfb_dist: spec and quant lengths differ"
    );
    let quantizer_shift = (gain.wrapping_neg() >> 2) + 1;
    let k = if dzone_quant_enable {
        QUANT_K1
    } else {
        QUANT_K0
    };
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is part of the AArch64 baseline. The kernel writes
        // exactly the scalar prefix on early-out and is bit-identical
        // otherwise; tails and exceptional groups use the scalar path.
        let sum = unsafe {
            super::quant_neon::calc_sfb_dist_sum_neon(spec, quant, gain, quantizer_shift, k)
        };
        match sum {
            Some(xfsf) => calc_ld_data(xfsf),
            None => 0,
        }
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        calc_sfb_dist_scalar(spec, gain, dzone_quant_enable, quant)
    }
    #[cfg(target_arch = "x86_64")]
    {
        // Runtime dispatch: AVX2 kernels only after the feature check; the
        // `#[target_feature]` + `#[inline(never)]` entry points keep AVX2
        // instructions out of the baseline path (see quant_avx2).
        if std::arch::is_x86_feature_detected!("avx2") {
            #[cfg(test)]
            avx2_dispatch_observe::record(avx2_dispatch_observe::DIST);
            // SAFETY: feature-checked above. The kernel writes exactly the
            // scalar prefix on early-out and is bit-identical otherwise;
            // tails and exceptional groups use the scalar path.
            let sum = unsafe {
                super::quant_avx2::calc_sfb_dist_sum_avx2(spec, quant, gain, quantizer_shift, k)
            };
            match sum {
                Some(xfsf) => calc_ld_data(xfsf),
                None => 0,
            }
        } else {
            calc_sfb_dist_scalar(spec, gain, dzone_quant_enable, quant)
        }
    }
}

/// Scalar reference for [`calc_sfb_dist`]: always available for differential
/// testing, including where the public entry uses SIMD (NEON/AVX2).
pub(crate) fn calc_sfb_dist_scalar(
    spec: &[i32],
    gain: i32,
    dzone_quant_enable: bool,
    quant: &mut [i16],
) -> i32 {
    let quantizer = QUANT_TABLE_Q[(gain.wrapping_neg() & 3) as usize];
    let quantizer_shift = (gain.wrapping_neg() >> 2) + 1;
    let k = if dzone_quant_enable {
        QUANT_K1
    } else {
        QUANT_K0
    };
    let mut xfsf: i32 = 0;
    for (&s, q_out) in spec.iter().zip(quant.iter_mut()) {
        let q = quantize_line_body(quantizer, quantizer_shift, k, s);
        *q_out = q;
        // Widen before abs: `wrapping_abs` keeps `i16::MIN` negative and
        // would let it slip past `> MAX_QUANT` into `inv_quantize_line`
        // (whose `specExp < 14` assert then panics). The reference
        // `fixabs_S` has the same blind spot; here the sentinel takes the
        // documented early-out instead.
        if (q as i32).abs() > MAX_QUANT {
            return 0;
        }
        let inv = inv_quantize_line(gain, q);
        xfsf = accumulate_dist(xfsf, inv, s);
    }
    calc_ld_data(xfsf)
}

/// `FDKaacEnc_calcSfbQuantEnergyAndDist` (`quantize.cpp:361-401`):
/// energy (`fPow2` of the inverse values) and distortion of a band with
/// caller-supplied quantized lines. Same `MAX_QUANT` early-out, yielding
/// `(0, 0)`; otherwise `(CalcLdData(energy) + 0.03125,
/// CalcLdData(distortion))`.
pub fn calc_sfb_quant_energy_and_dist(spec: &[i32], quant: &[i16], gain: i32) -> (i32, i32) {
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: NEON is part of the AArch64 baseline. Bit-identical sums;
        // exceptional groups and tails use the scalar path.
        let sums = unsafe {
            super::quant_neon::calc_sfb_quant_energy_and_dist_sums_neon(spec, quant, gain)
        };
        match sums {
            Some((energy, distortion)) => (
                calc_ld_data(energy).wrapping_add(0x0400_0000),
                calc_ld_data(distortion),
            ),
            None => (0, 0),
        }
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        calc_sfb_quant_energy_and_dist_scalar(spec, quant, gain)
    }
    #[cfg(target_arch = "x86_64")]
    {
        // Runtime dispatch: AVX2 kernels only after the feature check; the
        // `#[target_feature]` + `#[inline(never)]` entry points keep AVX2
        // instructions out of the baseline path (see quant_avx2).
        if std::arch::is_x86_feature_detected!("avx2") {
            #[cfg(test)]
            avx2_dispatch_observe::record(avx2_dispatch_observe::ENERGY);
            // SAFETY: feature-checked above. Bit-identical sums;
            // exceptional groups and tails use the scalar path.
            let sums = unsafe {
                super::quant_avx2::calc_sfb_quant_energy_and_dist_sums_avx2(spec, quant, gain)
            };
            match sums {
                Some((energy, distortion)) => (
                    calc_ld_data(energy).wrapping_add(0x0400_0000),
                    calc_ld_data(distortion),
                ),
                None => (0, 0),
            }
        } else {
            calc_sfb_quant_energy_and_dist_scalar(spec, quant, gain)
        }
    }
}

/// Scalar reference for [`calc_sfb_quant_energy_and_dist`].
pub(crate) fn calc_sfb_quant_energy_and_dist_scalar(
    spec: &[i32],
    quant: &[i16],
    gain: i32,
) -> (i32, i32) {
    let mut energy: i32 = 0;
    let mut distortion: i32 = 0;
    for (i, &s) in spec.iter().enumerate() {
        let q = quant[i];
        // Widen before abs: `wrapping_abs` keeps `i16::MIN` negative and
        // would let it slip past `> MAX_QUANT` into `inv_quantize_line`
        // (whose `specExp < 14` assert then panics). The reference
        // `fixabs_S` has the same blind spot; here the sentinel takes the
        // documented early-out instead.
        if (q as i32).abs() > MAX_QUANT {
            return (0, 0);
        }
        let inv = inv_quantize_line(gain, q);
        energy = energy.wrapping_add(fpow2(inv));
        distortion = accumulate_dist(distortion, inv, s);
    }
    (
        calc_ld_data(energy).wrapping_add(0x0400_0000),
        calc_ld_data(distortion),
    )
}

/// Which SIMD backend the production dispatch condition in this module
/// resolves to on the current target. Test-only: it mirrors the exact `cfg`
/// gates and the same `is_x86_feature_detected!("avx2")` macro used by
/// `quantize_spectrum`, `calc_sfb_dist` and `calc_sfb_quant_energy_and_dist`.
/// NOTE: this re-evaluates the dispatch *condition*; it does not by itself
/// prove the production branch executed. Actual branch entry is observed by
/// [`avx2_dispatch_observe`] (Phase 2.2.1), which the native diagnostic
/// asserts before confirming AVX2. No production atomics, counters, or
/// runtime overhead.
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum Backend {
    Neon,
    Avx2,
    Scalar,
}

#[cfg(test)]
pub(crate) fn selected_backend() -> Backend {
    #[cfg(target_arch = "aarch64")]
    {
        Backend::Neon
    }
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            Backend::Avx2
        } else {
            Backend::Scalar
        }
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        Backend::Scalar
    }
}

/// Test-only instrumentation recording *actual entry* into the production
/// AVX2 dispatch branches (Phase 2.2.1). Unlike [`selected_backend`] — which
/// only re-evaluates the feature-detection condition — these counters are
/// incremented inside the real `if is_x86_feature_detected!("avx2")` branches
/// of [`quantize_spectrum`], [`calc_sfb_dist`] and
/// [`calc_sfb_quant_energy_and_dist`], so a nonzero count proves the
/// production dispatcher actually called the AVX2 kernel (not merely that
/// AVX2 is available). Compiled only for x86-64 test builds; absent from
/// production builds (zero overhead, no public API change).
#[cfg(all(test, target_arch = "x86_64"))]
pub(crate) mod avx2_dispatch_observe {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    pub(crate) const QUANTIZE: usize = 0;
    pub(crate) const DIST: usize = 1;
    pub(crate) const ENERGY: usize = 2;

    /// Monotonic per-branch entry counts. Atomics make concurrent increments
    /// from parallel tests race-free.
    pub(crate) static COUNTS: [AtomicU64; 3] =
        [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];

    /// Serializes the reset→invoke→assert critical section of the observing
    /// test so its measurement window cannot be interleaved with another
    /// observation.
    pub(crate) static LOCK: Mutex<()> = Mutex::new(());

    #[inline]
    pub(crate) fn record(branch: usize) {
        COUNTS[branch].fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phase 2.1 (MEDIUM-1): the dispatch-selection helper must resolve to the
    /// backend mandated by the target and runtime features. Runs on every
    /// architecture: AArch64 -> NEON, x86-64 -> AVX2 iff detected else scalar,
    /// other -> scalar. This is a positive assertion of the selection logic,
    /// not an inference from a kernel test passing.
    #[test]
    fn dispatch_backend_matches_arch_and_features() {
        let b = selected_backend();
        #[cfg(target_arch = "aarch64")]
        assert_eq!(b, Backend::Neon, "AArch64 must dispatch NEON");
        #[cfg(target_arch = "x86_64")]
        {
            let want = if std::arch::is_x86_feature_detected!("avx2") {
                Backend::Avx2
            } else {
                Backend::Scalar
            };
            assert_eq!(b, want, "x86-64 dispatch must follow AVX2 detection");
        }
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        assert_eq!(b, Backend::Scalar, "other targets must dispatch scalar");
    }

    #[test]
    fn probe_table_anchors_hold() {
        // C-probed literals (quant_probe QTAB dump): the scripted
        // extraction cross-checks every entry; these pin the modules
        // together so a stale quant_tables.rs fails loudly here.
        assert_eq!(MTAB_3_4.len(), 512);
        assert_eq!(MTAB_4_3.len(), 512);
        assert_eq!(QUANT_TABLE_Q, [16384, 19484, 23170, 27554]);
        assert_eq!(QUANT_TABLE_E, [4096, 6889, 11585, 19484]);
        assert_eq!(QUANT_K0, 13284);
        assert_eq!(QUANT_K1, 7536);
        assert_eq!(MTAB_3_4[0], 19484);
        assert_eq!(MTAB_4_3[0], 852229450);
        assert_eq!(SPEC_EXP_MANT[0][0], 1073741824);
        assert_eq!(SPEC_EXP_SHIFT[3][13], 19);
    }

    #[test]
    fn fixmuldiv_widths_match_reference() {
        // fixmuldiv2_SS is a plain product (no shift, fixmul.h:159-161).
        assert_eq!(fixmuldiv2_ss(16384, 4096), 67108864);
        assert_eq!(fixmuldiv2_ss(-32768, 32767), -1073709056);
        // fixmuldiv2_DS widens the SGL operand by 16 (fixmul.h:185-194).
        assert_eq!(fixmuldiv2_ds(0x40000000, 0x4000), 0x10000000);
        assert_eq!(fixmuldiv2_ds(0, 12345), 0);
        assert_eq!(fixmuldiv2_ds(-1, 1), -1);
        // fpow2 clears bit 0 where fmult keeps it (ARM asr#31 vs >>32<<1).
        assert_eq!(fpow2(0x40000000), 0x20000000);
        assert_eq!(fpow2(0), 0);
        assert_eq!(fpow2(1), 0);
        assert_eq!(fpow2(0x7FFFFFFF), 0x7FFFFFFE);
        assert_eq!(fpow2(0x40000001), 0x20000000);
        assert_eq!(fmult(0x40000001, 0x40000001), 0x20000001);
        assert_eq!(fmult(0x7FFFFFFF, 0x7FFFFFFF), 0x7FFFFFFE);
    }

    #[test]
    fn quantize_zero_fast_path() {
        // accu == 0 takes the zero branch (quantize.cpp:165-167): no
        // normalization, no table read, whatever the gain or dead zone.
        for gain in [-192, -64, -1, 0, 1, 100, 300] {
            assert_eq!(quantize_line(gain, 0, false), 0);
            assert_eq!(quantize_line(gain, 0, true), 0);
        }
    }

    #[test]
    fn quantize_sign_symmetry() {
        // The negative branch mirrors the magnitude pipeline and negates
        // the truncated result (quantize.cpp:133-149). Symmetry is exact
        // wherever the first-stage product truncation is sign-symmetric,
        // i.e. (quantizer<<16)*spec divisible by 2^32 — structurally
        // guaranteed for spectra that are multiples of 2^16 (and zero).
        // Other magnitudes can differ by the DS floor (shared with C;
        // the probe transcript pins those bit-exactly instead).
        let specs = [
            0,
            65536,
            -65536,
            0x1000000,
            0x10000000,
            -0x10000000,
            0x40000000,
            -0x40000000,
        ];
        for gain in [-64, -48, -16, 0, 32, 64, 128, 192] {
            for &s in &specs {
                for dz in [false, true] {
                    let p = quantize_line(gain, s, dz);
                    let n = quantize_line(gain, s.wrapping_neg(), dz);
                    assert_eq!(n, p.wrapping_neg(), "symmetry g={gain} s={s} dz={dz}");
                }
            }
        }
    }

    #[test]
    fn qprobe_transcript_exact() {
        // Every synthetic probe the reference executed (magnitudes x
        // gains x dead zones, multi-band scf ramp, distortion and
        // inverse-path bands) reproduces bit-exactly, including the
        // wrapping-overflow sentinel cases. Any divergence fails here
        // with the exact (gain, input) that first differs.
        let cases = super::super::testdata::load_qprobe();
        assert!(cases.len() > 200, "probe corpus size");
        let mut counts = [0usize; 4];
        for case in &cases {
            match case {
                super::super::testdata::QProbe::Quantize { gain, dz, spec, q } => {
                    counts[0] += 1;
                    let mut out = vec![0i16; spec.len()];
                    let off = [0, spec.len() as i32];
                    quantize_spectrum(1, 1, 1, &off, spec, *gain, &[0], &mut out, *dz);
                    assert_eq!(&out, q, "QL g={gain} dz={dz} spec={spec:?}");
                }
                super::super::testdata::QProbe::QuantizeBands { gain, spec, q } => {
                    counts[1] += 1;
                    let mut out = vec![0i16; 16];
                    quantize_spectrum(
                        4,
                        2,
                        2,
                        &super::super::testdata::QPROBE_BAND_OFF,
                        spec,
                        *gain,
                        &super::super::testdata::QPROBE_BAND_SCF,
                        &mut out,
                        false,
                    );
                    assert_eq!(&out, q, "QBANDS gg={gain}");
                }
                super::super::testdata::QProbe::Dist {
                    gain,
                    dz,
                    spec,
                    q,
                    dist,
                } => {
                    counts[2] += 1;
                    let mut quant = vec![0i16; spec.len()];
                    let got = calc_sfb_dist(spec, *gain, *dz, &mut quant);
                    assert_eq!(&quant, q, "QD re-quantized lines g={gain} dz={dz}");
                    assert_eq!(got, *dist, "QD dist g={gain} dz={dz} spec={spec:?}");
                }
                super::super::testdata::QProbe::InvDist {
                    gain,
                    spec,
                    q,
                    en,
                    dist,
                } => {
                    counts[3] += 1;
                    let (got_en, got_dist) = calc_sfb_quant_energy_and_dist(spec, q, *gain);
                    assert_eq!(got_en, *en, "QI en g={gain}");
                    assert_eq!(got_dist, *dist, "QI dist g={gain} q={q:?}");
                }
            }
        }
        assert!(
            counts[0] > 200 && counts[1] == 1 && counts[2] > 10 && counts[3] > 5,
            "probe mix: {counts:?}"
        );
    }

    #[test]
    fn dist_zero_band_reads_sentinel() {
        // All-zero spectra quantize to all-zero lines; the distortion sum
        // stays 0 and CalcLdData(0) is the -1.0 sentinel (i32::MIN).
        let spec = [0i32; 8];
        let mut quant = [0i16; 8];
        assert_eq!(calc_sfb_dist(&spec, 0, false, &mut quant), i32::MIN);
        assert_eq!(quant, [0; 8]);
        // Same through the energy/dist path: energy sentinel picks up the
        // +0.03125 add (0x80000000 + 0x04000000 wraps to 0x84000000).
        let (en, dist) = calc_sfb_quant_energy_and_dist(&spec, &[0; 8], 0);
        assert_eq!(en, 0x84000000u32 as i32);
        assert_eq!(dist, i32::MIN);
    }

    #[test]
    fn dist_max_quant_gate_exact() {
        // The guard is a strict `> MAX_QUANT` on the widened magnitude
        // (quantize.cpp:327,373): 8191 passes through, 8192 early-outs.
        let spec = [0x10000000i32; 4];
        let (en, dist) = calc_sfb_quant_energy_and_dist(&spec, &[8191; 4], 0);
        assert_ne!((en, dist), (0, 0));
        assert_eq!(
            calc_sfb_quant_energy_and_dist(&spec, &[8192, 0, 0, 0], 0),
            (0, 0)
        );
        assert_eq!(
            calc_sfb_quant_energy_and_dist(&spec, &[0, 0, 8192, 0], 0),
            (0, 0)
        );
    }

    #[test]
    fn dist_min_quant_takes_early_out() {
        // `i16::MIN` widened to `i32` has magnitude 32768, so it takes the
        // documented early-out instead of reaching `inv_quantize_line`
        // (whose `specExp < 14` assert the old wrapping guard let it hit).
        let spec = [0x10000000i32; 4];
        assert_eq!(
            calc_sfb_quant_energy_and_dist(&spec, &[i16::MIN, 0, 0, 0], 0),
            (0, 0)
        );
        assert_eq!(calc_sfb_quant_energy_and_dist(&[0], &[i16::MIN], 0), (0, 0));
    }

    #[test]
    #[should_panic(expected = "specExp envelope")]
    fn inv_quantize_out_of_envelope_panics_loudly() {
        // |q| > MAX_QUANT never reaches invQuantizeLines in the reference
        // flow (the distortion guards catch it, including the former
        // slipped value -32768, which would OOB-read in C). Direct calls
        // above the envelope still panic — loud failure by design.
        let _ = inv_quantize_line(0, 8192);
    }

    /// Committed S3 corpus: (testdata short name, frame). Same 19
    /// element-frames as S1/S2; S3 consumes the qc1 post-MS/IS spectra
    /// with inactive-band lines zeroed (the S3 input contract — the
    /// oracle zeroes the same lines; see `s3_input_spec`).
    const S3_CASES: [(&str, u32); 19] = [
        ("silence-stereo", 5),
        ("sine-stereo", 1),
        ("sine-stereo", 5),
        ("onset-stereo", 22),
        ("onset-stereo", 23),
        ("onset-stereo", 24),
        ("onset-stereo", 25),
        ("onset-stereo", 26),
        ("stepdown-stereo", 2),
        ("click-mono", 11),
        ("impulse-mono", 5),
        ("edge-mono", 4),
        ("multitone-stereo", 1),
        ("sweep-stereo", 10),
        ("noise-mono", 5),
        ("tonal-noise-stereo", 5),
        ("quiet-sine-stereo", 5),
        ("sine-stereo-44100", 5),
        ("onset-stereo-44100", 22),
    ];

    /// S3 input contract: copy of the qc1 spectrum with inactive-band
    /// lines (per-band index >= max, i.e. bands QuantizeSpectrum never
    /// visits) zeroed. Mirrors the oracle S3 stage exactly: those lines
    /// hold proven C scratch (see validate_s1.py), and no downstream
    /// consumer reads them.
    fn s3_input_spec(
        spec: &[i32],
        offsets: &[i32],
        cnt: usize,
        per: usize,
        max: usize,
    ) -> Vec<i32> {
        let mut v = spec.to_vec();
        let mut o = 0;
        while o < cnt {
            for b in max..per {
                let start = offsets[o + b] as usize;
                let end = offsets[o + b + 1] as usize;
                for line in start..end {
                    v[line] = 0;
                }
            }
            o += per;
        }
        v
    }

    #[test]
    fn s3_corpus_exact() {
        // End-to-end exactness over real spectra: for every channel and
        // grid gain, QuantizeSpectrum (uniform QSS), per-band calcSfbDist
        // and per-band calcSfbQuantEnergyAndDist (fed with the committed
        // lines) reproduce the oracle bit-exactly. Geometry cross-checks
        // the qc1 vectors, proving the S3 stage consumed the same bands.
        use super::super::testdata;
        for (name, f) in S3_CASES {
            let s1 = testdata::load_s1(name, f);
            let s3 = testdata::load_s3(name, f);
            assert_eq!(s3.frame, f);
            assert_eq!(s3.ch.len(), s1.ch.len(), "{name} f={f} channel count");
            for (ci, (c1, c3)) in s1.ch.iter().zip(s3.ch.iter()).enumerate() {
                assert_eq!(c3.geo_cnt, c1.geo_cnt, "{name} f={f} ch={ci} cnt");
                assert_eq!(c3.geo_per, c1.geo_per, "{name} f={f} ch={ci} per");
                assert_eq!(c3.geo_max, c1.geo_max, "{name} f={f} ch={ci} max");
                assert_eq!(c3.offsets, c1.offsets, "{name} f={f} ch={ci} offsets");
                assert_eq!(c3.gains, [-72, -32, -8, 64], "{name} f={f} gain grid");
                let spec = s3_input_spec(&c1.spec, &c3.offsets, c3.geo_cnt, c3.geo_per, c3.geo_max);
                for (gi, &gain) in c3.gains.iter().enumerate() {
                    let scf = vec![0i32; c3.geo_cnt];
                    let mut out = vec![0i16; 1024];
                    quantize_spectrum(
                        c3.geo_cnt,
                        c3.geo_max,
                        c3.geo_per,
                        &c3.offsets,
                        &spec,
                        gain,
                        &scf,
                        &mut out,
                        false,
                    );
                    assert_eq!(out, c3.qspec[gi], "{name} f={f} ch={ci} S3Q g={gain}");
                    for b in 0..c3.geo_cnt {
                        let start = c3.offsets[b] as usize;
                        let end = c3.offsets[b + 1] as usize;
                        let mut scratch = vec![0i16; end - start];
                        let dist = calc_sfb_dist(&spec[start..end], gain, false, &mut scratch);
                        assert_eq!(
                            dist, c3.dist[gi][b],
                            "{name} f={f} ch={ci} S3DIST g={gain} band={b}"
                        );
                        // QuantizeSpectrum only visits bands below
                        // maxSfbPerGroup per group (quantize.cpp:291-292);
                        // other bands stay zero in S3Q by construction.
                        // calcSfbDist stops writing at the first |q| >
                        // MAX_QUANT line (quantize.cpp:327-329), so only
                        // the prefix through that line is comparable.
                        if b % c3.geo_per < c3.geo_max {
                            let mut valid = end - start;
                            for (k, &qv) in c3.qspec[gi][start..end].iter().enumerate() {
                                // Wrapping abs mirrors the C guard exactly.
                                if (qv.wrapping_abs() as i32) > MAX_QUANT {
                                    valid = k + 1;
                                    break;
                                }
                            }
                            assert_eq!(
                                scratch[..valid],
                                c3.qspec[gi][start..start + valid],
                                "{name} f={f} ch={ci} re-quantized lines g={gain} band={b}"
                            );
                        }
                        let (en, ed) = calc_sfb_quant_energy_and_dist(
                            &spec[start..end],
                            &c3.qspec[gi][start..end],
                            gain,
                        );
                        assert_eq!(
                            en, c3.en[gi][b],
                            "{name} f={f} ch={ci} S3EN en g={gain} band={b}"
                        );
                        assert_eq!(
                            ed, c3.en_dist[gi][b],
                            "{name} f={f} ch={ci} S3EN dist g={gain} band={b}"
                        );
                    }
                }
            }
        }
    }
}
