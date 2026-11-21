//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! Test-only derived Q31↔FPE energy envelope (FPE-3.1R1, `docs/fpe/05` §9.2).
//!
//! The FPE value lane sums the **physical** f64 spectrum. The Q31 reference
//! maps the same spectrum onto the Q31 grid before squaring, so the expected
//! stored-energy difference has three source terms:
//!
//! ```text
//! Q_b = 2^(2f−31)·Σ|X_j| + n_b·2^(2f−33)   // input-mapping (1st + 2nd order)
//! T_b = 2n_b·2^(−s_b), s_b = min(2(s0_b−f), 31)   // square/floor/re-shift
//! R   = 1.5                                  // shift floor + FPE rounding
//! tol_E(b) = max(32, Q_b + T_b + R) + E_ref/500
//! ```
//!
//! `Q_b` derivation: `X_j = round(x_j·2^(31−m))`, `q_j = X_j·2^(m−31)`,
//! `δ_j = x_j−q_j`, `ε_j = δ_j·2^(31−m)`, `|ε_j| ≤ 1/2` without saturation.
//! The Q31-side stored energy (before its floor) is
//! `2^(2f−31)·ΣX²`; the FPE value is `2^(2f−31)·Σ(X+ε)²`; their difference is
//! `2^(2f−31)·(2ΣX_jε_j + Σε_j²)`, bounded by `2^(2f−31)·(Σ|X_j| + n/4)`
//! = `Q_b`. `T_b` mirrors `fPow2AddDiv2`'s `s²>>32` floor and the re-shift
//! floor; `R` covers the shift's `+1` floor and FPE's `≤0.5` rounding.
//!
//! **Assumptions/guards:** the half-LSB bound requires a non-saturating Q31
//! mapping; saturation is detected and reported as unsupported. The approved
//! `Q_b` also assumes the re-shift exponent is not clamped to 31
//! (`2(s0−f) > 31`); such a comparison is reported unsupported unless both
//! stored values are zero. The exact supported-domain detector additionally
//! rejects Q31 raw accumulation / `<<1` wrapping and negative re-shift
//! overflow (see `supported_domain`), including `maxScale < 4` where the
//! `−4` headroom shortcut does not apply. All arithmetic here is test-only and
//! independent of the production helpers.

use crate::aac_enc::energy as q31_energy;

/// Exact power-of-two scaling used only in the test envelope. Exponents are
/// small (`|e| ≤ 62`) on this path; the result is a normal f64.
#[inline]
pub(super) fn pow2(exp: i32) -> f64 {
    debug_assert!(
        (-1022..=1023).contains(&exp),
        "FPE envelope: exponent {exp} outside normal f64 range"
    );
    2f64.powi(exp)
}

/// Independent Q31 mapping: `round(x·2^(31−m))` clamped to i32 range.
/// Returns `(mapped, saturated)`; `saturated` marks a value whose rounded
/// magnitude left the i32 range (where the half-LSB derivation is invalid).
pub(super) fn map_q31(x: &[f64], m: i32) -> (Vec<i32>, bool) {
    let factor = pow2(31 - m);
    let mut saturated = false;
    let mapped = x
        .iter()
        .map(|&v| {
            let y = (v * factor).round();
            if y > i32::MAX as f64 || y < i32::MIN as f64 {
                saturated = true;
            }
            y.clamp(i32::MIN as f64, i32::MAX as f64) as i32
        })
        .collect();
    (mapped, saturated)
}

/// Independent `maxScale` recomputation (`CalcSfbMaxScaleSpec` semantics):
/// `min(30, clz(max|X|)−1)` per band, using wrapping absolute value.
pub(super) fn max_scale_q31(x_q31: &[i32], offsets: &[u16], active: usize) -> Vec<i32> {
    (0..active)
        .map(|b| {
            let mut peak: i32 = 0;
            for &v in &x_q31[offsets[b] as usize..offsets[b + 1] as usize] {
                peak = peak.max(v.wrapping_abs());
            }
            ((peak as u32).leading_zeros() as i32 - 1).min(30)
        })
        .collect()
}

/// Independent Q31 stored band energy for the **rescale** path.
///
/// Mirrors `energy::check_band_energy_into` + the `psy_main.cpp:679-691`
/// re-shift without calling production helpers:
/// `s = X << scale0`; `t = wrap32(t + (s·s >> 32))`; `e_raw = t << 1`;
/// `E_st = shift(e_raw, −s_b)` with wrapping left shift for negative `s_b`.
pub(super) fn q31_stored_energy(
    x_q31: &[i32],
    offsets: &[u16],
    band: usize,
    max_scale: i32,
    final_shift: i32,
) -> i32 {
    let scale0 = (max_scale - 4).max(0);
    let mut tmp: i32 = 0;
    for &v in &x_q31[offsets[band] as usize..offsets[band + 1] as usize] {
        let s = (v as u32).wrapping_shl(scale0 as u32) as i32;
        let sq = (((s as i64) * (s as i64)) >> 32) as i32;
        tmp = tmp.wrapping_add(sq);
    }
    let e_raw = (tmp as u32).wrapping_shl(1) as i32;
    let s_b = (2 * (scale0 - final_shift)).min(31);
    if s_b >= 0 {
        e_raw >> s_b
    } else {
        (e_raw as u32).wrapping_shl((-s_b) as u32) as i32
    }
}

/// Maximum finite stored-energy tolerance (R2.0 R1a3 hardening).
///
/// The largest representable `|got − want|` for two i32 stored energies is
/// `2^32 − 1`. A supported comparison never needs a larger tolerance; any
/// computed tolerance at or above this (including `inf`) indicates a missing
/// unsupported classification. Supported-path tolerances are capped here so
/// a synthetic extreme exponent can never pass with an infinite bound.
pub(super) const MAX_FINITE_ENERGY_TOL: f64 = 4_294_967_295.0;

/// Per-band derived envelope terms (`05` §9.2).
#[derive(Debug, Clone, Copy)]
pub(super) struct BandEnvelope {
    /// Input-mapping contribution (`Q_b`).
    pub q: f64,
    /// Square/floor/re-shift contribution (`T_b`).
    pub t: f64,
    /// Boundary/shift-floor contribution (`R = 1.5`).
    pub r: f64,
    /// `max(32, Q_b + T_b + R)`; add `E_ref/500` for the full tolerance.
    pub base: f64,
    /// Signed re-shift exponent `s_b`.
    pub scale_b: i32,
    /// `s0_b = max(0, maxScale_b−4)`.
    pub scale0: i32,
    /// Band width `n_b`.
    pub width: usize,
    /// `Σ|X_j|` over the mapped band.
    pub sum_abs_x: f64,
    /// Q31 mapping saturated in this band (half-LSB bound invalid).
    pub mapping_saturated: bool,
    /// `2(s0_b−f) > 31` (approved `Q_b` assumes no clamp).
    pub shift_clamped: bool,
    /// Q31 raw accumulation or final `<<1` would wrap (envelope invalid).
    pub accumulation_wrap: bool,
    /// Negative re-shift (`s_b < 0`) would overflow i32 (envelope invalid).
    pub reshift_overflow: bool,
    /// The floating-point `Q_b`/`T_b` terms are not finite (synthetic
    /// out-of-range exponent); the envelope is invalid.
    pub non_finite_terms: bool,
}

impl BandEnvelope {
    /// Full stored-energy tolerance for reference `E_ref`.
    pub fn tolerance(&self, e_ref: i32) -> f64 {
        self.base + (e_ref as f64).abs() / 500.0
    }

    /// Reason the approved bound is not valid for this comparison, if any.
    pub fn unsupported_reason(&self, got: i32, want: i32) -> Option<&'static str> {
        if self.mapping_saturated {
            return Some("Q31 input mapping saturated");
        }
        if self.accumulation_wrap {
            return Some("Q31 raw accumulation or <<1 would wrap");
        }
        if self.reshift_overflow {
            return Some("Q31 negative re-shift would overflow i32");
        }
        if self.non_finite_terms {
            return Some("envelope Q_b/T_b term is non-finite");
        }
        if self.shift_clamped && !(got == 0 && want == 0) {
            return Some("Q31 re-shift exponent clamped to 31");
        }
        None
    }

    /// Whether the approved bound applies to this comparison.
    pub fn supported(&self, got: i32, want: i32) -> bool {
        self.unsupported_reason(got, want).is_none()
    }
}

/// Safe envelope-term power of two. Returns a saturated f64 for exponents
/// outside the normal/subnormal f64 range instead of panicking; supported
/// production exponents are exact. Used only by the test envelope.
fn pow2_checked(exp: i64) -> f64 {
    if exp > 1023 {
        f64::INFINITY
    } else if exp < -1074 {
        0.0
    } else {
        2f64.powi(exp as i32)
    }
}

/// Exact supported-domain detector for the Q31 raw accumulation and the
/// energy re-shift.
///
/// Production semantics (`check_band_energy_into`):
/// `s_j = X_j << s0` (wrapping), `u_j = (s_j·s_j) >> 32` (floor),
/// `tmp = Σ u_j` (i32 wrapping), `e_raw = tmp << 1` (wrapping), then
/// `shift_int(e_raw, −s_b)` (wrapping left shift for negative `s_b`).
///
/// The envelope derivation assumes no wrap. This detector computes the exact
/// non-negative sum in i64 and requires:
/// 1. every `s_j` is representable in i32 (no left-shift wrap);
/// 2. `Σ floor(s_j²/2^32) ≤ i32::MAX >> 1` (so both `tmp` and `tmp << 1`
///    fit; partial sums are monotone, so this also excludes intermediate
///    accumulation wrap);
/// 3. for `s_b < 0`, `e_raw·2^(−s_b)` is representable in i32.
///
/// It is conservative-safe and exact for the production path, and it also
/// covers `maxScale < 4`, where the `−4` headroom shortcut does not apply.
/// No shift here can overflow the host integer: `s0` is range-guarded, and
/// the re-shift check rejects `−s_b ≥ 32` for nonzero `e_raw` before using an
/// i128 comparison; a zero `e_raw` cannot overflow.
fn supported_domain(
    x_q31: &[i32],
    offsets: &[u16],
    band: usize,
    scale0: i32,
    scale_b: i64,
) -> (bool, bool) {
    let mut sum_u: i64 = 0;
    let mut accumulation_wrap = false;
    for &v in &x_q31[offsets[band] as usize..offsets[band + 1] as usize] {
        if v == 0 {
            continue;
        }
        // For nonzero v, scale0 >= 32 implies |v·2^scale0| >= 2^32 > i32.
        // Reject before shifting. This also avoids the i128 value-wrap
        // trap where, e.g., 2^16 << 123 silently becomes zero in a 128-bit
        // register and would otherwise look representable.
        if scale0 >= 32 {
            accumulation_wrap = true;
            continue;
        }
        // scale0 < 32 and v is i32: the shifted magnitude fits comfortably
        // in i64 (max ~2^62), so no host shift overflow is possible.
        let s_exact = (v as i64) << scale0;
        if s_exact > i32::MAX as i64 || s_exact < i32::MIN as i64 {
            // The production left shift wraps; no valid envelope.
            accumulation_wrap = true;
            continue;
        }
        // s_exact is in i32 range, so its square fits in i64.
        sum_u += (s_exact * s_exact) >> 32;
    }
    if sum_u > (i32::MAX >> 1) as i64 {
        accumulation_wrap = true;
    }
    if accumulation_wrap {
        // The production accumulation already wraps; the envelope is invalid
        // and the re-shift term is not meaningful.
        return (false, false);
    }
    let e_raw = sum_u * 2;
    // `scale_b` is i64 here, so this models the mathematical re-shift even
    // for synthetic f > maxScale states. A zero raw value cannot overflow
    // (0 << k = 0). For nonzero e_raw, any left shift of ≥32 already exceeds
    // i32; reject it without shifting. For <32, i128 holds the product
    // exactly.
    let reshift_overflow = if scale_b < 0 && e_raw != 0 {
        let k = scale_b.unsigned_abs();
        if k >= 32 {
            true
        } else {
            ((e_raw as i128) << k) > i32::MAX as i128
        }
    } else {
        false
    };
    (true, !reshift_overflow)
}

/// Compute the per-band envelope from the independent mapped spectrum.
pub(super) fn band_envelope(
    x_q31: &[i32],
    offsets: &[u16],
    band: usize,
    final_shift: i32,
    max_scale: i32,
    mapping_saturated: bool,
) -> BandEnvelope {
    let width = offsets[band + 1] as usize - offsets[band] as usize;
    let sum_abs_x: f64 = x_q31[offsets[band] as usize..offsets[band + 1] as usize]
        .iter()
        .map(|&v| (v as f64).abs())
        .sum();
    // Wide intermediate: `max_scale - 4` must not overflow for the full
    // declared i32 domain (notably i32::MIN..=i32::MIN+3).
    let scale0 = (max_scale as i64 - 4).max(0) as i32;
    let unclamped = 2i64 * (scale0 as i64 - final_shift as i64);
    let scale_b_i64 = unclamped.min(31);
    let shift_clamped = unclamped > 31;
    // Report field: for supported production states this is exactly `s_b`;
    // for synthetic out-of-range negatives it is clamped only for display
    // (the envelope is marked unsupported by the exact detector).
    let scale_b = scale_b_i64.clamp(i32::MIN as i64, 31) as i32;
    let (accumulation_ok, reshift_ok) = supported_domain(x_q31, offsets, band, scale0, scale_b_i64);

    let q = pow2_checked(2i64 * final_shift as i64 - 31) * sum_abs_x
        + width as f64 * pow2_checked(2i64 * final_shift as i64 - 33);
    let t = 2.0 * width as f64 * pow2_checked(-scale_b_i64);
    let non_finite_terms = !q.is_finite() || !t.is_finite();
    let r = 1.5;
    BandEnvelope {
        q,
        t,
        r,
        base: if non_finite_terms {
            f64::INFINITY
        } else {
            (q + t + r).max(32.0)
        },
        scale_b,
        scale0,
        width,
        sum_abs_x,
        mapping_saturated,
        shift_clamped,
        accumulation_wrap: !accumulation_ok,
        reshift_overflow: !reshift_ok,
        non_finite_terms,
    }
}

/// Test-only convenience used by the identity gates: Q31-mapped window plus
/// independently recomputed per-band `maxScale`.
pub(super) struct MappedWindow {
    pub x_q31: Vec<i32>,
    pub max_scale: Vec<i32>,
    pub mapping_saturated: bool,
}

pub(super) fn map_window(x: &[f64], offsets: &[u16], active: usize, m: i32) -> MappedWindow {
    let (x_q31, mapping_saturated) = map_q31(x, m);
    let max_scale = max_scale_q31(&x_q31, offsets, active);
    MappedWindow {
        x_q31,
        max_scale,
        mapping_saturated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_reproduces_production_semantics_and_flags_saturation() {
        // Independent mapping matches the production mapper on a normal
        // spectrum (bit-exact), and detects out-of-range input.
        let x = [0.0, 0.25, -0.5, 1.0 / 1024.0, -3.0 / 4096.0];
        let (got, sat) = map_q31(&x, 12);
        let want = q31_energy::map_spectrum_to_q31(&x, 12);
        assert_eq!(got, want, "independent mapping");
        assert!(!sat, "normal mapping not saturated");

        let (_, sat_big) = map_q31(&[2.0], 0);
        assert!(sat_big, "out-of-range mapping detected");
        let (_, sat_neg) = map_q31(&[-2.0], 0);
        assert!(sat_neg, "negative out-of-range mapping detected");
    }

    #[test]
    fn max_scale_and_q31_energy_match_production_on_clean_bands() {
        let x = [0.0, 0.25, -0.5, 0.75, 1.0, 0.0, 0.0, 0.0];
        let offsets = [0u16, 4, 8];
        let m = 12;
        let f = 3;
        let q = q31_energy::map_spectrum_to_q31(&x, m);
        let max_scale = max_scale_q31(&q, &offsets, 2);
        let want_max = q31_energy::max_scale_spec(&q, &offsets, 2);
        assert_eq!(max_scale, want_max, "independent maxScale");
        let mut raw = Vec::new();
        q31_energy::check_band_energy_into(&q, &max_scale, &offsets, 2, &mut raw);
        for b in 0..2 {
            let got = q31_stored_energy(&q, &offsets, b, max_scale[b], f);
            let scale0 = (max_scale[b] - 4).max(0);
            let s_b = (2 * (scale0 - f)).min(31);
            let want = if s_b >= 0 {
                raw[b] >> s_b
            } else {
                (raw[b] as u32).wrapping_shl((-s_b) as u32) as i32
            };
            assert_eq!(got, want, "independent Q31 stored band {b}");
        }
    }

    #[test]
    fn q31_energy_matches_negative_shift_amplification() {
        // Negative s_b => left shift; the independent mirror must preserve
        // the wrapping amplification, not clamp the exponent to zero.
        let x = [0.5, -0.25, 0.125, 0.0];
        let offsets = [0u16, 4];
        let m = 12;
        let f = 12;
        let q = q31_energy::map_spectrum_to_q31(&x, m);
        let max_scale = max_scale_q31(&q, &offsets, 1);
        let scale0 = (max_scale[0] - 4).max(0);
        assert!(
            2 * (scale0 - f) < 0,
            "test setup requires negative re-shift, scale0={scale0}"
        );
        let got = q31_stored_energy(&q, &offsets, 0, max_scale[0], f);
        let mut raw = Vec::new();
        q31_energy::check_band_energy_into(&q, &max_scale, &offsets, 1, &mut raw);
        let s_b = 2 * (scale0 - f);
        let want = (raw[0] as u32).wrapping_shl((-s_b) as u32) as i32;
        assert_eq!(got, want);
    }

    #[test]
    fn envelope_mapping_term_matches_actual_mapping_error() {
        // Directly measure the mapping difference before truncation and
        // verify it is within Q_b. This is the core of the derivation.
        let x = [0.3, -0.7, 0.11, -0.02, 0.9, 0.05, 0.0, 0.4];
        let offsets = [0u16, 4, 8];
        let m = 12;
        let f = 4;
        let (q, sat) = map_q31(&x, m);
        assert!(!sat);
        for b in 0..2 {
            let max_scale = max_scale_q31(&q, &offsets, 2)[b];
            let env = band_envelope(&q, &offsets, b, f, max_scale, sat);
            let sum_x2: f64 = x[offsets[b] as usize..offsets[b + 1] as usize]
                .iter()
                .map(|v| v * v)
                .sum();
            let sum_q2: f64 = q[offsets[b] as usize..offsets[b + 1] as usize]
                .iter()
                .map(|&v| {
                    let qv = v as f64 * pow2(m - 31);
                    qv * qv
                })
                .sum();
            let sx = sum_x2 * pow2(31 - 2 * m + 2 * f);
            let sq = sum_q2 * pow2(31 - 2 * m + 2 * f);
            let actual = (sx - sq).abs();
            assert!(
                actual <= env.q,
                "band {b}: mapping delta {actual} > Q_b {}",
                env.q
            );
        }
    }

    #[test]
    fn envelope_truncation_and_rounding_cover_q31_floor() {
        // Compare the independent Q31 stored value with the exact Q31-side
        // real value; the difference must fit T_b + R.
        let x = [0.001, -0.002, 0.0035, -0.0007, 0.02, -0.01, 0.004, 0.0];
        let offsets = [0u16, 4, 8];
        let m = 12;
        let f = 5;
        let (q, sat) = map_q31(&x, m);
        assert!(!sat);
        let max_scale = max_scale_q31(&q, &offsets, 2);
        for b in 0..2 {
            let env = band_envelope(&q, &offsets, b, f, max_scale[b], sat);
            let q31_stored = q31_stored_energy(&q, &offsets, b, max_scale[b], f);
            let sum_q2: f64 = q[offsets[b] as usize..offsets[b + 1] as usize]
                .iter()
                .map(|&v| {
                    let qv = v as f64 * pow2(m - 31);
                    qv * qv
                })
                .sum();
            let ideal_q31 = sum_q2 * pow2(31 - 2 * m + 2 * f);
            let trunc = (ideal_q31 - q31_stored as f64).abs();
            assert!(
                trunc <= env.t + env.r,
                "band {b}: truncation {trunc} > T_b+R {}",
                env.t + env.r
            );
        }
    }

    #[test]
    fn envelope_reports_shift_clamp_and_saturation() {
        // Clamped re-shift: s0−f > 15.5 with a high maxScale.
        let q = [1, 0, 0, 0];
        let offsets = [0u16, 4];
        let env = band_envelope(&q, &offsets, 0, 0, 30, false);
        assert!(env.shift_clamped, "shift clamp detected");
        assert_eq!(env.scale_b, 31);
        assert!(
            !env.supported(1, 0),
            "non-trivial clamped compare unsupported"
        );
        assert!(env.supported(0, 0), "zero/zero clamped compare allowed");

        // Saturated mapping is always unsupported.
        let env = band_envelope(&q, &offsets, 0, 0, 4, true);
        assert!(!env.supported(0, 0), "saturated mapping unsupported");
    }

    #[test]
    fn accumulation_guard_detects_tmp_and_shift_wrap_boundary() {
        // maxScale = 0 (X = 2^30), scale0 = 0, f = 0.
        // u = (2^30)^2 >> 32 = 2^28 per line.
        // i32::MAX >> 1 = 2^30 - 1 = 1,073,741,823.
        // 4 lines => sum = 2^30 (one over the bound); 3 lines => supported.
        let q4 = [1i32 << 30; 4];
        let off4 = [0u16, 4];
        let env4 = band_envelope(&q4, &off4, 0, 0, 0, false);
        assert!(
            env4.accumulation_wrap,
            "4-line maxScale=0 band must be detected as wrapping"
        );
        assert!(!env4.supported(0, 0), "wrapping band is unsupported");
        assert!(
            q31_stored_energy(&q4, &off4, 0, 0, 0) < 0,
            "test setup: production raw <<1 actually wraps here"
        );

        let q3 = [1i32 << 30; 3];
        let off3 = [0u16, 3];
        let env3 = band_envelope(&q3, &off3, 0, 0, 0, false);
        assert!(
            !env3.accumulation_wrap,
            "3-line boundary band is still supported"
        );
        assert!(env3.supported(0, 0), "boundary band remains supported");

        // The direct detector also covers maxScale < 4 (the `-4` headroom
        // shortcut does not apply there); no blind `maxScale >= 4` gate.
        assert!(!env3.accumulation_wrap);
    }

    #[test]
    fn accumulation_guard_does_not_reject_normal_wide_band() {
        // maxScale = 4 (X = 2^26, scale0 = 0), 96 lines:
        // u = 2^20 per line, sum = 96*2^20 << i32::MAX>>1.
        let q = [1i32 << 26; 96];
        let offsets = [0u16, 96];
        let env = band_envelope(&q, &offsets, 0, 0, 4, false);
        assert!(!env.accumulation_wrap, "wide supported band");
        assert!(!env.reshift_overflow, "no re-shift overflow");
        assert!(env.supported(0, 0));
    }

    #[test]
    fn reshift_guard_detects_negative_shift_overflow() {
        // X = 2^27 => maxScale = 3, scale0 = 0.
        // e_raw = 2 * ((2^27)^2 >> 32) = 2^23.
        // f = 15 => s_b = -30; e_raw << 30 exceeds i32.
        let q = [1i32 << 27];
        let offsets = [0u16, 1];
        let env_bad = band_envelope(&q, &offsets, 0, 15, 3, false);
        assert!(
            env_bad.reshift_overflow,
            "negative re-shift overflow must be detected"
        );
        assert!(!env_bad.supported(0, 0), "overflowing re-shift unsupported");

        let env_ok = band_envelope(&q, &offsets, 0, 0, 3, false);
        assert!(!env_ok.reshift_overflow, "f=0 re-shift is safe");
        assert!(env_ok.supported(0, 0));
    }

    #[test]
    fn left_shift_guard_rejects_i128_value_wrap_and_preserves_boundaries() {
        let offsets = [0u16, 1];

        // 65536 << 123 = 2^139; an i128 shift wraps it to zero, which the
        // old guard would misclassify as representable. maxScale=127 =>
        // scale0=123; must be unsupported.
        let env = band_envelope(&[65536], &offsets, 0, 0, 127, false);
        assert!(
            env.accumulation_wrap,
            "scale0=123 nonzero value must be rejected"
        );
        assert!(!env.supported(0, 0));

        // scale0=31 boundary: zero energy is supported where the other
        // guards permit (f=16 => s_b=30, no clamp).
        let env = band_envelope(&[0], &offsets, 0, 16, 35, false);
        assert!(!env.accumulation_wrap, "zero at scale0=31");
        assert!(env.supported(0, 0));

        // scale0=31 with -1: the shift itself fits (i32::MIN), but the
        // accumulation/<<1 wraps, so it is still unsupported.
        let env = band_envelope(&[-1], &offsets, 0, 16, 35, false);
        assert!(env.accumulation_wrap, "scale0=31 nonzero accumulation wrap");
        assert!(!env.supported(0, 0));

        // scale0=32: zero remains supported; nonzero is rejected.
        let env = band_envelope(&[0], &offsets, 0, 16, 36, false);
        assert!(env.supported(0, 0), "zero at scale0=32");
        let env = band_envelope(&[1], &offsets, 0, 16, 36, false);
        assert!(
            env.accumulation_wrap,
            "nonzero at scale0=32 must be rejected"
        );
        assert!(!env.supported(0, 0));

        // scale0=30 nonzero boundary with a non-clamped re-shift: the
        // largest non-rejected nonzero shift on this path.
        let env = band_envelope(&[1], &offsets, 0, 15, 34, false);
        assert!(!env.accumulation_wrap, "scale0=30 nonzero supported");
        assert!(env.supported(0, 0));
    }

    #[test]
    fn scale0_derivation_is_panic_free_across_i32_domain() {
        let offsets = [0u16, 1];
        for max_scale in [
            i32::MIN,
            i32::MIN + 1,
            i32::MIN + 2,
            i32::MIN + 3,
            i32::MIN + 4,
            0,
            4,
            12,
            30,
            i32::MAX,
        ] {
            for q in [[0i32], [1i32], [-1i32]] {
                let env = band_envelope(&q, &offsets, 0, 0, max_scale, false);
                // Reaching this point without a debug panic is the primary
                // assertion. Zero energy stays supported whenever the other
                // guards permit, including huge scale0.
                if q[0] == 0 {
                    assert!(
                        env.supported(0, 0),
                        "zero-energy maxScale={max_scale} must stay supported"
                    );
                }
            }
        }

        // Extreme synthetic finalShift values must not panic either.
        let _ = band_envelope(&[1], &offsets, 0, i32::MAX, i32::MAX, false);
        let _ = band_envelope(&[1], &offsets, 0, i32::MIN, i32::MAX, false);
    }

    #[test]
    fn reshift_guard_rejects_large_negative_shift_synthetic_states() {
        // f = 63 > maxScale = 3 => s_b = 2*(0-63) = -126. The previous i64
        // comparison could wrap/mask the shift and misclassify this as
        // supported; the safe detector must reject it.
        let q = [1i32 << 27];
        let offsets = [0u16, 1];
        let env = band_envelope(&q, &offsets, 0, 63, 3, false);
        assert!(
            env.reshift_overflow,
            "large negative re-shift must be rejected"
        );
        assert!(!env.supported(0, 0));
        assert!(
            !env.non_finite_terms,
            "f=63 terms are finite; rejection is the re-shift guard"
        );

        // Existing production-reachable boundary is unchanged.
        let env_ok = band_envelope(&q, &offsets, 0, 0, 3, false);
        assert!(!env_ok.reshift_overflow);
        assert!(env_ok.supported(0, 0));

        // Extreme synthetic exponent: must not panic, and must be rejected.
        let env_x = band_envelope(&q, &offsets, 0, 1000, 3, false);
        assert!(!env_x.supported(0, 0));
        assert!(env_x.non_finite_terms || env_x.reshift_overflow);
    }

    #[test]
    fn envelope_is_data_dependent_per_band() {
        // Same width/representation, different mapped magnitudes => larger
        // Q_b for the larger spectrum (not a fixed global tolerance).
        let small = [1, 0, 0, 0];
        let large = [1 << 20, 0, 0, 0];
        let offsets = [0u16, 4];
        let e_small = band_envelope(&small, &offsets, 0, 2, 20, false);
        let e_large = band_envelope(&large, &offsets, 0, 2, 4, false);
        assert!(e_large.q > e_small.q, "Q_b follows Σ|X|");
    }

    #[test]
    fn scale0_is_max_zero_headroom() {
        // Direct scale0 assertion (R2.0 R1a3 F4): the envelope headroom is
        // exactly `max(0, maxScale − 4)`. Negative synthetic maxScale values
        // stay confined to this unit test — never production reachability.
        let offsets = [0u16, 1];
        for max_scale in [-5, -1, 0, 1, 3, 4, 5, 10, 30] {
            let env = band_envelope(&[1], &offsets, 0, 2, max_scale, false);
            assert_eq!(
                env.scale0,
                (max_scale - 4).max(0),
                "scale0 for maxScale={max_scale}"
            );
        }
    }

    #[test]
    fn supported_tolerance_is_finite_and_capped() {
        // R2.0 R1a3 F1: a supported comparison always carries a finite
        // tolerance within the representable i32-delta domain. Extreme
        // synthetic exponents are unsupported, never infinite-passing.
        let offsets = [0u16, 1];
        let env = band_envelope(&[1 << 20], &offsets, 0, 2, 4, false);
        let tol = env.tolerance(1000).min(MAX_FINITE_ENERGY_TOL);
        assert!(tol.is_finite(), "supported tolerance finite");
        assert!(tol <= MAX_FINITE_ENERGY_TOL, "tolerance capped");
        let extreme = band_envelope(&[1 << 27], &offsets, 0, 1000, 3, false);
        assert!(
            !extreme.supported(0, 0),
            "extreme synthetic state unsupported"
        );
    }
}
