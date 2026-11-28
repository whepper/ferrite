//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! FPE float band-energy kernels.
//!
//! **M3 (FPE-3.1): rescale band energy, long and short windows.** The
//! rescale-stage value arithmetic is native f64 in the Q31-fraction domain
//! (`docs/fpe/05` §1.3/§6.1); the boundary quantization that produces the
//! stored `i32` energies lives in [`super::output`] (`05` §7). The
//! representation lane (integer `maxScale`/`mdctScale`/`finalShift`/
//! `maxNrg`) is untouched and remains the Q31 engine's ([`crate::aac_enc::energy`],
//! ADR-0005).
//!
//! # Derivation (cross-checked against the Q31 mirror)
//!
//! C `FDKaacEnc_CheckBandEnergyOptim` (`band_nrg.cpp:140-184`) and the Q31
//! mirror [`crate::aac_enc::energy::check_band_energy_into`] accumulate
//! `fPow2AddDiv2` (`t += s²/2^32`) and finish with `<< 1`, so the raw sum is
//!
//! ```text
//! e_raw[b] = Σ_j (X[j]·2^scale0)² / 2^31,   scale0 = max(0, maxScale[b] − 4)
//! X[j]     = round(spec_f64[j]·2^(31−m))            (Q31 integer lane)
//! ```
//!
//! `psy_main.cpp:679-691` then re-shifts by `2(scale0 − finalShift)`:
//!
//! ```text
//! E_st[b] ≈ Σ_j X[j]² · 2^(2·finalShift) / 2^31
//!         ≈ E_real[b] · 2^(31 − 2m′),   m′ = m − finalShift
//! E_f[b]  = E_real[b] · 2^(−2m′) = E_st[b]/2^31
//! ```
//!
//! The per-band `scale0` scaffolding cancels in the net value; short
//! windows take the same rescale path (C uses `CheckBandEnergyOptim` for
//! both window types, and `psy_main.cpp:686-691` applies the same Ld
//! shift). The short-specific `CalcBandEnergyOptimShort` kernel
//! (`band_nrg.cpp:264-288`) belongs to the **TNS energy recompute** and is
//! FPE-3.2, not this task. Its net fraction energy reduces to the same
//! `E_real·2^(−2m)` (`05` §6.1), with the ±31 saturation handled by the
//! boundary clamp.
//!
//! # Ld conventions (`05` §6.8)
//!
//! [`crate::aac_enc::energy::rescale_ld_into`] computes, from the raw stored
//! int, `log2(e_raw/2^31)/64 − scale0·2/64` with the `e_raw <= 0 → −1.0`
//! sentinel, then adds `finalShift·2/64` **unconditionally** (lifting the
//! silent-band sentinel to `−1 + finalShift·2/64`). The float mirror uses the
//! pre-finalShift fraction energy `E_raw_f = E_real·2^(−2m+2scale0)` for the
//! base and preserves the guard and the unconditional correction exactly.
//!
//! Planned content still outstanding: **FPE-7.1** float MS energies
//! (`05` §6.6).
//!
//! **M3 (FPE-3.2): TNS energy recompute, long and short windows**
//! (`docs/fpe/05` §6.2). After the reused TNS filter mutates the physical
//! f64 spectrum in place, the per-band energy is recomputed as
//! `E_real[b] = Σ spec_f64_filtered[j]²` with `E_f = E_real·2^(−2m)` where
//! `m` is the current block exponent (post long pre-halving `m += 1`,
//! pre-overflow; the overflow `m += tnsSpecShift` is applied by the
//! unchanged integer [`crate::aac_enc::psy::overflow_rescale`]). Long
//! windows carry the integer-exact `shiftBits` (`nrgScaling`) from the
//! representation lane: the stored value is the `05` §7 boundary image of
//! `E_real·2^(−2(m+shiftBits))` and the Ld follows the guarded
//! `band_energy_long_into` convention. Short windows are energy-only (C
//! takes no Ld array; short Ld stays at rescale values).
//!
//! Upstream: `band_nrg.cpp:140-184/192-256/264-288/296-361`. Q31 mirrors
//! (read-only): `energy.rs:161/249/215/321`.

use super::output::fraction_to_stored;

/// Exact power-of-two scaling for the small exponents of the fraction
/// domain. Every value produced here is a normal f64 power of two, so the
/// multiplication in the kernels below is exact (`05` §1.4).
#[inline]
fn pow2(exp: i32) -> f64 {
    debug_assert!(
        (-1022..=1023).contains(&exp),
        "FPE energy: power-of-two exponent {exp} outside normal f64 range"
    );
    2f64.powi(exp)
}

/// Rescale-stage band energy in the FPE fraction domain (`docs/fpe/05` §6.1).
///
/// Fills, for every active band `b`:
///
/// ```text
/// E_real[b]  = Σ_j spec_f64[j]²
/// E_f[b]     = E_real[b] · 2^(−2(m − final_shift))
/// E_raw_f[b] = E_real[b] · 2^(−2m + 2·scale0[b])
/// ld[b]      = (E_raw_f > 0 ? log2(E_raw_f)/64 − scale0·2/64 : −1.0)
///              + final_shift·2/64
/// ```
///
/// `energies_f` receives `E_f` (f64, full precision, never quantized here);
/// the caller converts it at the output boundary (`05` §7). `ld` receives
/// the rescale energy Ld with the Q31 guard/correction semantics of
/// [`crate::aac_enc::energy::rescale_ld_into`] (`05` §6.8): the `scale0`
/// correction is skipped for the silent sentinel, and the `finalShift`
/// correction is applied unconditionally.
///
/// `spectrum` is one window of the physical f64 spectrum (already
/// lowpass-zeroed), `offsets` the channel's SFB offsets, and `max_scale`
/// the representation-lane per-band `maxScale` (`mdct_scale = m` is the
/// pre-rescale block exponent, `final_shift` the joint rescale shift).
#[allow(clippy::too_many_arguments)]
pub(super) fn rescale_band_energy_into(
    spectrum: &[f64],
    offsets: &[u16],
    active: usize,
    mdct_scale: i32,
    final_shift: i32,
    max_scale: &[i32],
    energies_f: &mut Vec<f64>,
    ld: &mut Vec<f64>,
) {
    debug_assert!(active < offsets.len(), "FPE energy: active bands");
    debug_assert!(max_scale.len() >= active, "FPE energy: maxScale length");

    // m′ = m − finalShift: the exponent of the post-rescale fraction energy.
    // The scaling is an exact power of two; the f64 sum keeps full precision.
    let energy_scale = pow2(-2 * (mdct_scale - final_shift));

    energies_f.clear();
    energies_f.reserve(active);
    ld.clear();
    ld.reserve(active);
    for b in 0..active {
        let mut e_real = 0.0f64;
        for &x in &spectrum[offsets[b] as usize..offsets[b + 1] as usize] {
            e_real += x * x;
        }
        let e_f = e_real * energy_scale;

        // Pre-finalShift fraction energy for the Ld base: the float image
        // of the Q31 raw sum (`scale0` scaffolding restored). Derived from
        // `E_f` by an exact power of two, so no precision is lost.
        let scale0 = (max_scale[b] - 4).max(0);
        let e_raw_f = e_f * pow2(2 * (scale0 - final_shift));

        let mut v = if e_raw_f <= 0.0 {
            // fLog2 sentinel for non-positive input (`rescale_ld_into`).
            -1.0
        } else {
            e_raw_f.log2() / 64.0 - scale0 as f64 * 2.0 / 64.0
        };
        // Unconditional `ldShift`; also lifts the silent-band sentinel
        // (verified on the Q31 mirror, `energy.rs:419-435`).
        v += final_shift as f64 * 2.0 / 64.0;

        energies_f.push(e_f);
        ld.push(v);
    }
}

/// TNS energy recompute in the FPE fraction domain (`docs/fpe/05` §6.2).
///
/// Recomputes the per-band energy over the **post-TNS filtered** physical
/// f64 spectrum (the reused TNS filter has already mutated it in place).
/// Must be called **after** the Q31 [`crate::aac_enc::psy::tns_encode_channel`]
/// for the same window, so that `max_scale` is the recomputed TNS
/// headroom (representation lane, ADR-0005) and `mdct_scale` is the current
/// block exponent (post long pre-halving `m += 1`, pre-overflow). The
/// element-global overflow `m += tnsSpecShift` is applied afterwards by the
/// unchanged [`crate::aac_enc::psy::overflow_rescale`], which shifts the
/// stored ints produced here by the exact integer power of two.
///
/// ```text
/// E_real[b]       = Σ spec_f64_filtered[j]²            (f64 accumulation)
/// E_f[b]          = E_real[b] · 2^(−2m)                (short; §6.2 net)
/// E_shifted_f[b]  = E_real[b] · 2^(−2(m+shiftBits))    (long pre-overflow)
/// E_st[b]         = clamp(round(E_shifted_f·2^31))      (long; §7 boundary)
/// E_st[b]         = clamp(round(E_f·2^31))              (short; §7 boundary)
/// ```
///
/// `shift_bits` is the integer-exact `nrgScaling` (`shiftBits`) decision
/// from the Q31 representation lane (`band_energy_long_into` return); it is
/// **reused verbatim**, never recomputed in float. For short windows it is
/// ignored (energy-only path, no Ld array — the C quirk preserved:
/// short Ld stays at rescale values).
///
/// Long-window Ld (`05` §6.8) preserves the Q31 guarded convention with
/// identical thresholds, operating on the float image of the pre-shift
/// accumulation:
///
/// ```text
/// leading    = maxScale[b] − 4                        (signed)
/// scaleDiff  = leading·2/64
/// E_raw_f[b] = E_real[b] · 2^(−2m+2·leading)
/// base       = E_raw_f ≤ 0 ? −1.0 : log2(E_raw_f)/64
/// v          = base ≥ (−0.5 + scaleDiff/2) ? base − scaleDiff : −1.0
/// ld[b]      = v − shiftBits·2/64
/// ```
///
/// The final `−shiftBits·2/64` correction is unconditional (it also applies
/// to the `−1.0` sentinel path, mirroring `energy.rs:303-311`). Short
/// windows leave `ld` untouched.
///
/// `energies_stored` must have length `active` (overwritten in place by
/// band index); `ld` must have length `active` for long windows (overwritten
/// in place) and is not read or written for short windows.
#[allow(clippy::too_many_arguments)]
pub(super) fn tns_recompute_energy_into(
    spectrum: &[f64],
    offsets: &[u16],
    active: usize,
    mdct_scale: i32,
    max_scale: &[i32],
    shift_bits: i32,
    short: bool,
    energies_stored: &mut Vec<i32>,
    ld: &mut Vec<f64>,
) {
    debug_assert!(active < offsets.len(), "FPE TNS energy: active bands");
    debug_assert!(max_scale.len() >= active, "FPE TNS energy: maxScale length");
    debug_assert!(
        energies_stored.len() >= active,
        "FPE TNS energy: energies length"
    );
    if !short {
        debug_assert!(ld.len() >= active, "FPE TNS energy: Ld length");
    }
    // End of the recomputed window must lie inside the spectrum slice.
    debug_assert!(
        (offsets.get(active).copied().unwrap_or(0) as usize) <= spectrum.len(),
        "FPE TNS energy: offsets inside spectrum"
    );

    if short {
        let energy_scale = pow2(-2 * mdct_scale);
        for b in 0..active {
            let mut e_real = 0.0f64;
            for &x in &spectrum[offsets[b] as usize..offsets[b + 1] as usize] {
                e_real += x * x;
            }
            let e_f = e_real * energy_scale;
            energies_stored[b] = fraction_to_stored(e_f);
        }
        return;
    }

    let shifted_scale = pow2(-2 * (mdct_scale + shift_bits));
    let shift_corr = shift_bits as f64 * 2.0 / 64.0;
    for b in 0..active {
        let mut e_real = 0.0f64;
        for &x in &spectrum[offsets[b] as usize..offsets[b + 1] as usize] {
            e_real += x * x;
        }
        let leading = max_scale[b] - 4;
        let scale_diff = leading as f64 * 2.0 / 64.0;
        // Float image of the Q31 pre-shift accumulation
        // (`E_real·2^(−2m+2·leading)`); exact power-of-two scaling.
        let e_raw_f = e_real * pow2(-2 * mdct_scale + 2 * leading);
        let base = if e_raw_f <= 0.0 {
            -1.0
        } else {
            e_raw_f.log2() / 64.0
        };
        // Guarded Ld with identical thresholds to `band_energy_long_into`
        // (`energy.rs:282-289`): `base >= (−1>>1)+(scaleDiff>>1)` in the
        // integer formulation is `−0.5 + scaleDiff/2` in float.
        let mut v = if base >= (-0.5 + scale_diff / 2.0) {
            base - scale_diff
        } else {
            -1.0
        };
        // Unconditional `−shiftBits·2/64` (also applies to the sentinel
        // path, mirroring `energy.rs:308-310`).
        v -= shift_corr;

        let e_shifted_f = e_real * shifted_scale;
        energies_stored[b] = fraction_to_stored(e_shifted_f);
        ld[b] = v;
    }
}

/// Q31-fraction log2 of a positive fraction energy (`log2(v_f)/64`), the
/// float-lane convention consumed by the §6.8 Ld vectors. Kept here for the
/// FPE energy unit tests and future kernels; the exact Q31 `calc_ld_data`
/// path stays in the Q31 module.
#[cfg(test)]
fn log2frac_f(e_f: f64) -> f64 {
    e_f.log2() / 64.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::energy as q31_energy;
    use crate::aac_enc::fpe::output::fraction_to_stored;

    /// Convenience wrapper for the pure kernel (test-only allocation).
    fn energy(
        spectrum: &[f64],
        offsets: &[u16],
        active: usize,
        m: i32,
        f: i32,
        max_scale: &[i32],
    ) -> (Vec<f64>, Vec<f64>) {
        let mut e = Vec::new();
        let mut ld = Vec::new();
        rescale_band_energy_into(spectrum, offsets, active, m, f, max_scale, &mut e, &mut ld);
        (e, ld)
    }

    fn assert_close(got: f64, want: f64, tol: f64, what: &str) {
        assert!(
            (got - want).abs() <= tol,
            "{what}: got {got}, want {want} (tol {tol})"
        );
    }

    #[test]
    fn rescale_energy_hand_computed_sums() {
        // Two bands, four lines: [1,2]/16 and [3,4]/16. scale0 = 0, m′ = 0.
        let spec = [0.0625, 0.125, 0.1875, 0.25];
        let (e, ld) = energy(&spec, &[0, 2, 4], 2, 0, 0, &[4, 4]);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0], 5.0 / 256.0);
        assert_eq!(e[1], 25.0 / 256.0);
        assert_close(ld[0], log2frac_f(5.0 / 256.0), 1e-15, "ld band 0");
        assert_close(ld[1], log2frac_f(25.0 / 256.0), 1e-15, "ld band 1");
        // Boundary: round-to-nearest of E_f·2^31 (exact here).
        assert_eq!(fraction_to_stored(e[0]), 5 << 23);
        assert_eq!(fraction_to_stored(e[1]), 25 << 23);
    }

    #[test]
    fn rescale_energy_silence_is_zero_and_sentinel() {
        // All-zero band: E_f = 0 and the Ld sentinel is lifted by
        // finalShift·2/64 only (no scale0 correction on the sentinel path).
        let spec = [0.0; 8];
        let (e, ld) = energy(&spec, &[0, 4, 8], 2, 12, 3, &[20, 7]);
        assert_eq!(e, vec![0.0, 0.0]);
        let sentinel = -1.0 + 3.0 * 2.0 / 64.0;
        assert_eq!(ld, vec![sentinel, sentinel]);
    }

    #[test]
    fn rescale_energy_full_scale_saturates_at_boundary() {
        // Full-scale lines at zero exponent headroom: E_f = 4, so
        // E_f·2^31 = 2^33 saturates at the non-negative boundary maximum.
        let spec = [1.0; 4];
        let (e, _ld) = energy(&spec, &[0, 4], 1, 12, 12, &[0]);
        assert_eq!(e[0], 4.0);
        assert_eq!(fraction_to_stored(e[0]), i32::MAX);
    }

    #[test]
    fn rescale_energy_long_window_1024() {
        // Long window (1024 lines, m = 12), deterministic ramp; four
        // hand-computed bands. f = 12 gives m′ = 0, so E_f = E_real.
        let mut spec = [0.0f64; 1024];
        for (j, v) in spec.iter_mut().enumerate() {
            *v = ((j % 7) as f64) - 3.0;
        }
        let offsets = [0u16, 3, 64, 500, 1024];
        let (e, _ld) = energy(&spec, &offsets, 4, 12, 12, &[8, 8, 8, 8]);
        for b in 0..4 {
            let want: f64 = spec[offsets[b] as usize..offsets[b + 1] as usize]
                .iter()
                .map(|&x| x * x)
                .sum();
            assert_close(e[b], want, 1e-12, &format!("long band {b}"));
        }
        // f = 0 (m′ = 12) scales the same net energy by 2^(−24).
        let (e_unshifted, _) = energy(&spec, &offsets, 4, 12, 0, &[8, 8, 8, 8]);
        for b in 0..4 {
            assert_close(
                e_unshifted[b],
                e[b] * pow2(-24),
                1e-18,
                &format!("long f0 band {b}"),
            );
        }
    }

    #[test]
    fn rescale_energy_short_window_128() {
        // Short windows (8 × 128 lines, m = 9): per-window fraction energy
        // follows the same net formula. Use a window-varying signal and a
        // hand-computed two-band split.
        let mut spec = [0.0f64; 1024];
        for w in 0..8 {
            for j in 0..128 {
                spec[w * 128 + j] = if j < 2 {
                    (w + 1) as f64
                } else if j < 4 {
                    -1.0
                } else {
                    0.0
                };
            }
        }
        let offsets = [0u16, 2, 4];
        for w in 0..8 {
            let win = &spec[w * 128..(w + 1) * 128];
            let (e, ld) = energy(win, &offsets, 2, 9, 9, &[9, 9]);
            let a = (w + 1) as f64;
            assert_eq!(e[0], 2.0 * a * a, "short w{w} band 0");
            assert_eq!(e[1], 2.0, "short w{w} band 1");
            assert_close(ld[0], log2frac_f(2.0 * a * a), 1e-15, "short ld0");
            assert_close(ld[1], log2frac_f(2.0), 1e-15, "short ld1");
        }
    }

    #[test]
    fn rescale_energy_different_final_shifts_are_powers_of_two() {
        // Same spectrum/maxScale, increasing finalShift: E_f scales by
        // 2^(2f) exactly (the representation lane would choose the shift;
        // the value lane must follow it exactly).
        let spec = [0.25, -0.5, 0.75, 1.0];
        let mut prev: Option<(i32, Vec<f64>)> = None;
        for f in [0i32, 1, 2, 4] {
            let (e, ld) = energy(&spec, &[0, 2, 4], 2, 12, f, &[10, 10]);
            if let Some((pf, pe)) = &prev {
                for b in 0..2 {
                    assert_close(
                        e[b] / pe[b],
                        pow2(2 * (f - *pf)),
                        1e-15,
                        &format!("ratio f{f} b{b}"),
                    );
                }
            }
            // Active-band Ld is scale0-independent and equals log2(E_f)/64.
            assert_close(ld[0], log2frac_f(e[0]), 1e-15, &format!("ld f{f}"));
            assert_close(ld[1], log2frac_f(e[1]), 1e-15, &format!("ld b1 f{f}"));
            prev = Some((f, e));
        }
    }

    #[test]
    fn rescale_energy_pow2_input_scaling() {
        // x → 2^k·x gives E_f → 2^(2k)·E_f and Ld → Ld + 2k/64.
        let spec = [0.125, -0.25, 0.5, 0.75];
        let (e0, ld0) = energy(&spec, &[0, 2, 4], 2, 7, 2, &[6, 8]);
        for k in [-3i32, 1, 5] {
            let mut scaled = spec;
            for v in scaled.iter_mut() {
                *v *= pow2(k);
            }
            let (e, ld) = energy(&scaled, &[0, 2, 4], 2, 7, 2, &[6, 8]);
            for b in 0..2 {
                assert_close(e[b], e0[b] * pow2(2 * k), 1e-18, &format!("E k{k} b{b}"));
                assert_close(
                    ld[b],
                    ld0[b] + 2.0 * k as f64 / 64.0,
                    1e-15,
                    &format!("Ld k{k} b{b}"),
                );
            }
        }
    }

    #[test]
    fn rescale_ld_scale0_correction_and_unconditional_final_shift() {
        // Active bands: the scale0 correction cancels the per-band headroom,
        // so Ld is exactly log2(E_f)/64 regardless of maxScale. Silent
        // bands: the sentinel skips the scale0 correction but the finalShift
        // lift is unconditional, including finalShift = 0 (−1.0 exactly).
        // NOTE: these maxScale values are synthetic convention checks, not a
        // claim that they match the true spectrum; reachable production-state
        // cases live in `fpe31r1_reachable_*` (tests/differential.rs).
        let spec = [1.0, 2.0, 0.0, 0.0];
        let offsets = [0u16, 2, 4];
        for (f, sentinel) in [
            (0i32, -1.0f64),
            (2, -1.0 + 2.0 * 2.0 / 64.0),
            (7, -1.0 + 7.0 * 2.0 / 64.0),
        ] {
            for max_scale in [[4i32, 4], [10, 20], [30, 5]] {
                let (e, ld) = energy(&spec, &offsets, 2, 12, f, &max_scale);
                assert_close(
                    ld[0],
                    log2frac_f(e[0]),
                    1e-15,
                    &format!("active f{f} ms{max_scale:?}"),
                );
                assert_eq!(ld[1], sentinel, "silent band f{f} ms{max_scale:?}");
            }
        }
    }

    #[test]
    fn rescale_ld_matches_q31_mirror_on_clean_bands() {
        // Cross-check the float Ld/conversion against the Q31 kernel on a
        // deterministic integer-mappable spectrum with Q31 raw sums safely
        // above the truncation floor (the tiny-band sentinel divergence is
        // pinned separately below).
        let spec: [f64; 16] = [
            0.0, 0.25, -0.5, 0.75, 1.0, 0.0, 0.0, 0.0, -0.5, 0.25, 0.125, 0.0, 0.0, 0.0, 0.0, 0.0,
        ];
        let offsets = [0u16, 4, 8, 16];
        let m = 12;
        let f = 3;
        let max_scale = [6, 8, 12];
        let q = q31_energy::map_spectrum_to_q31(&spec, m);
        let mut e_raw = Vec::new();
        q31_energy::check_band_energy_into(&q, &max_scale, &offsets, 3, &mut e_raw);
        let mut ld_q31 = Vec::new();
        q31_energy::rescale_ld_into(&e_raw, &max_scale, f, &mut ld_q31);
        let (e_f, ld_f) = energy(&spec, &offsets, 3, m, f, &max_scale);
        for b in 0..3 {
            // Q31 stored energy after the C re-shift (floor, not round).
            let scale0 = (max_scale[b] - 4).max(0);
            let scale = ((scale0 - f) << 1).min(31);
            let q31_stored = if scale >= 0 {
                e_raw[b] >> scale
            } else {
                e_raw[b] << (-scale)
            };
            let float_stored = fraction_to_stored(e_f[b]);
            assert!(
                (float_stored - q31_stored).abs() <= 32 + q31_stored.abs() / 500,
                "band {b}: float {float_stored} vs q31 {q31_stored}"
            );
            assert!(
                (ld_f[b] - ld_q31[b]).abs() < 1e-6,
                "band {b} Ld: float {} vs q31 {}",
                ld_f[b],
                ld_q31[b]
            );
        }
    }

    #[test]
    fn rescale_ld_pins_q31_truncation_sentinel_divergence() {
        // Finding (reported, within the energy-domain envelope): when the
        // Q31 raw sum truncates to zero but the physical band energy is
        // non-zero, Q31 yields the lifted sentinel `−1 + finalShift·2/64`,
        // while the more precise float lane yields its true (very negative)
        // Ld. Both invert to ≈0 energy, so the §5 energy-domain gate
        // accepts the difference; the absolute Ld delta is measured and
        // reported, not masked by changing the float formula.
        let spec = [0.0, 1.0 / 1024.0, -2.0 / 1024.0, 3.0 / 1024.0];
        let offsets = [0u16, 4];
        let m = 12;
        let f = 3;
        let max_scale = [6];
        let q = q31_energy::map_spectrum_to_q31(&spec, m);
        let mut e_raw = Vec::new();
        q31_energy::check_band_energy_into(&q, &max_scale, &offsets, 1, &mut e_raw);
        assert_eq!(e_raw[0], 0, "test setup: Q31 raw sum must truncate to zero");
        let mut ld_q31 = Vec::new();
        q31_energy::rescale_ld_into(&e_raw, &max_scale, f, &mut ld_q31);
        let (e_f, ld_f) = energy(&spec, &offsets, 1, m, f, &max_scale);
        assert_eq!(ld_q31[0], -1.0 + f as f64 * 2.0 / 64.0, "Q31 sentinel");
        assert!(e_f[0] > 0.0, "float energy is resolved");
        assert!(ld_f[0] > ld_q31[0], "float Ld resolves above the sentinel");
        // Energy-domain equivalence (docs/fpe/07 §5): both are far below
        // the stored-energy tolerance floor.
        let e_got = 2f64.powf(ld_f[0] * 64.0 + 31.0);
        assert!(e_got <= 32.0, "inverted float energy {e_got} within floor");
        assert_eq!(fraction_to_stored(e_f[0]), 0, "band stores as zero");
    }

    // -----------------------------------------------------------------------
    // FPE-3.2 TNS energy recompute (docs/fpe/05 §6.2, §6.8, §7).
    // -----------------------------------------------------------------------

    /// Convenience wrapper for the long path (test-only allocation).
    fn tns_long(
        spectrum: &[f64],
        offsets: &[u16],
        active: usize,
        m: i32,
        max_scale: &[i32],
        shift_bits: i32,
    ) -> (Vec<i32>, Vec<f64>) {
        let mut e = vec![0i32; active];
        let mut ld = vec![-1.0f64; active];
        tns_recompute_energy_into(
            spectrum, offsets, active, m, max_scale, shift_bits, false, &mut e, &mut ld,
        );
        (e, ld)
    }

    /// Convenience wrapper for the short path (test-only allocation).
    /// `ld_init` is the caller-owned Ld vector; it must be untouched.
    fn tns_short(
        spectrum: &[f64],
        offsets: &[u16],
        active: usize,
        m: i32,
        max_scale: &[i32],
        ld_init: &mut Vec<f64>,
    ) -> Vec<i32> {
        let mut e = vec![0i32; active];
        tns_recompute_energy_into(
            spectrum, offsets, active, m, max_scale, 0, true, &mut e, ld_init,
        );
        e
    }

    #[test]
    fn tns_long_hand_computed_sums() {
        // Two bands, four lines: [1,2]/16 and [3,4]/16. m = 12, leading = 0,
        // shiftBits = 0. E_real = 5/256 and 25/256; E_shifted = E_real·2^-24.
        let spec = [0.0625, 0.125, 0.1875, 0.25];
        let (e, ld) = tns_long(&spec, &[0, 2, 4], 2, 12, &[4, 4], 0);
        // E_st = round(E_real·2^(31-24)) = round(E_real·128).
        assert_eq!(e[0], 3, "band 0 stores round(2.5)");
        assert_eq!(e[1], 13, "band 1 stores round(12.5)");
        // Ld: E_raw_f = E_real·2^-24, base = log2/64, scaleDiff = 0, guard
        // passes (base ≈ −0.46 ≥ −0.5), v = base.
        let e_raw0 = 5.0 / 256.0 * pow2(-24);
        let e_raw1 = 25.0 / 256.0 * pow2(-24);
        assert_close(ld[0], e_raw0.log2() / 64.0, 1e-15, "long ld0");
        assert_close(ld[1], e_raw1.log2() / 64.0, 1e-15, "long ld1");
    }

    #[test]
    fn tns_short_hand_computed_sums() {
        // Same spectrum as the long hand-computed case; short uses the same
        // net fraction E_f = E_real·2^(−2m) with no Ld array.
        let spec = [0.0625, 0.125, 0.1875, 0.25];
        let mut ld = vec![-1.0, -1.0];
        let e = tns_short(&spec, &[0, 2, 4], 2, 12, &[4, 4], &mut ld);
        assert_eq!(e, vec![3, 13], "short stores match long shiftBits=0");
        assert_eq!(ld, vec![-1.0, -1.0], "short leaves Ld untouched");
    }

    #[test]
    fn tns_silence_and_sentinel_cases() {
        // All-zero band: stored 0; long Ld is the guarded sentinel minus
        // the unconditional shiftBits correction.
        let spec = [0.0; 8];
        for shift in [0i32, 2, 5] {
            let (e, ld) = tns_long(&spec, &[0, 4, 8], 2, 12, &[4, 4], shift);
            assert_eq!(e, vec![0, 0], "silent stores shift={shift}");
            let want = -1.0 - shift as f64 * 2.0 / 64.0;
            assert_eq!(ld, vec![want, want], "silent Ld shift={shift}");
        }
        // Short silence: stored 0, Ld untouched (rescale sentinel preserved).
        let e = tns_short(
            &spec,
            &[0, 4, 8],
            2,
            12,
            &[30, 30],
            &mut vec![-1.0 + 3.0 * 2.0 / 64.0, -1.0],
        );
        assert_eq!(e, vec![0, 0], "short silent stores");
        // Pass a distinctive sentinel and check it is preserved bit-exactly.
        let mut ld_short = vec![0.12345, -0.98765];
        let e_short = tns_short(&spec, &[0, 4, 8], 2, 12, &[30, 30], &mut ld_short);
        assert_eq!(e_short, vec![0, 0]);
        assert_eq!(ld_short, vec![0.12345, -0.98765], "short Ld untouched");
    }

    #[test]
    fn tns_full_scale_and_saturation_boundaries() {
        // Full-scale lines at zero headroom (m = 0): E_real = 4, E_f = 4,
        // so E_st saturates at the non-negative boundary maximum.
        let spec = [1.0; 4];
        let (e_long, _) = tns_long(&spec, &[0, 4], 1, 0, &[4], 0);
        assert_eq!(e_long[0], i32::MAX, "long full-scale saturates");
        let mut ld = vec![-1.0];
        let e_short = tns_short(&spec, &[0, 4], 1, 0, &[4], &mut ld);
        assert_eq!(e_short[0], i32::MAX, "short full-scale saturates");
        // Same spectrum with shiftBits = 2 pulls the value back into range:
        // E_shifted = 4·2^(−4) = 0.25 → 0.25·2^31 = 2^29.
        let (e_shifted, ld_shifted) = tns_long(&spec, &[0, 4], 1, 0, &[4], 2);
        assert_eq!(e_shifted[0], 1 << 29, "shiftBits desaturates");
        // Ld for the saturated case is finite (E_raw_f > 0); the shifted Ld
        // is exactly 4/64 below the unshifted one (2·shiftBits/64).
        let (_, ld_unshifted) = tns_long(&spec, &[0, 4], 1, 0, &[4], 0);
        assert_close(
            ld_shifted[0],
            ld_unshifted[0] - 2.0 * 2.0 / 64.0,
            1e-15,
            "shiftBits Ld correction",
        );
    }

    #[test]
    fn tns_pre_halving_exponent_cases() {
        // Long pre-halving is m += 1 with the same filtered spectrum: the
        // value lane must scale by exactly 2^(−2) and the Ld by −2/64.
        let spec = [0.25, -0.5, 0.75, 1.0, 0.5, -0.25, 0.125, 0.0];
        let offsets = [0u16, 4, 8];
        let max_scale = [8, 8];
        for shift in [0i32, 1, 3] {
            let (e0, ld0) = tns_long(&spec, &offsets, 2, 12, &max_scale, shift);
            let (e1, ld1) = tns_long(&spec, &offsets, 2, 13, &max_scale, shift);
            for b in 0..2 {
                // Stored values scale by 1/4 up to the boundary rounding
                // (≤1 LSB for these magnitudes).
                let want = (e0[b] as f64 / 4.0).round() as i32;
                assert!(
                    (e1[b] - want).abs() <= 1,
                    "pre-halving stores m12→m13 b{b} shift={shift}: {} vs ~{}",
                    e1[b],
                    want
                );
                assert_close(
                    ld1[b],
                    ld0[b] - 2.0 / 64.0,
                    1e-15,
                    &format!("pre-halving Ld b{b} shift={shift}"),
                );
            }
        }
        // Short path: same 2^(−2m) scaling, no Ld.
        let mut ld0 = vec![9.0, 9.0];
        let mut ld1 = vec![9.0, 9.0];
        let e0 = tns_short(&spec, &offsets, 2, 12, &max_scale, &mut ld0);
        let e1 = tns_short(&spec, &offsets, 2, 13, &max_scale, &mut ld1);
        for b in 0..2 {
            let want = (e0[b] as f64 / 4.0).round() as i32;
            assert!((e1[b] - want).abs() <= 1, "short pre-halving b{b}");
        }
        assert_eq!(ld0, vec![9.0, 9.0]);
        assert_eq!(ld1, vec![9.0, 9.0]);
    }

    #[test]
    fn tns_overflow_rescale_exponent_cases() {
        // The unchanged integer overflow rescale is an exact power-of-two
        // shift: stored >>= 2·(shift−nrgScaling), Ld −= 2·(shift−nrgScaling)/64,
        // m += shift. Verified directly on the Q31 helper with FPE-produced
        // values (representation lane, ADR-0005).
        use crate::aac_enc::psy::overflow_rescale;
        let mut spectrum = vec![0.0f64; 8];
        let mut q = vec![1i32 << 20, 1 << 20, 1 << 20, 1 << 20, 0, 0, 0, 0];
        let mut e = vec![vec![1_000_000i32, 2_000_000]];
        let mut eld = vec![vec![0.25f64, -0.5]];
        let mut thr = vec![vec![400i32, 800]];
        let mut m = 12i32;
        overflow_rescale(
            &mut spectrum,
            &mut q,
            &mut e,
            &mut eld,
            &mut thr,
            &mut m,
            4,
            2,
            2,
            0,
        );
        // scale = (2−0)<<1 = 4: stored >>= 4, thr >>= 4, Ld −= 4/64, m += 2.
        assert_eq!(e[0], vec![62_500, 125_000]);
        assert_eq!(thr[0], vec![25, 50]);
        assert_close(eld[0][0], 0.25 - 4.0 / 64.0, 1e-18, "overflow ld0");
        assert_close(eld[0][1], -0.5 - 4.0 / 64.0, 1e-18, "overflow ld1");
        assert_eq!(m, 14);
        // Zero-shift path is a no-op for the value lane except m (unchanged).
        let mut e2 = vec![vec![123i32]];
        let mut eld2 = vec![vec![0.1f64]];
        let mut thr2 = vec![vec![456i32]];
        let mut q2 = vec![7i32];
        let mut m2 = 9i32;
        let mut spec2 = vec![0.0f64; 1];
        overflow_rescale(
            &mut spec2, &mut q2, &mut e2, &mut eld2, &mut thr2, &mut m2, 1, 1, 0, 0,
        );
        assert_eq!(e2[0], vec![123]);
        assert_eq!(eld2[0], vec![0.1]);
        assert_eq!(m2, 9);
    }

    #[test]
    fn tns_nonzero_shiftbits_ld_cases() {
        // Non-zero shiftBits: stored is the boundary image of
        // E_real·2^(−2(m+shiftBits)); Ld is the guarded value minus
        // shiftBits·2/64 (unconditional, including the sentinel path).
        let spec = [0.5, -0.5, 0.25, 0.25];
        let offsets = [0u16, 2, 4];
        let max_scale = [6, 6];
        let m = 12;
        // Band 0 active, band 1 active; shiftBits = 3.
        let (e, ld) = tns_long(&spec, &offsets, 2, m, &max_scale, 3);
        for b in 0..2 {
            let e_real: f64 = spec[offsets[b] as usize..offsets[b + 1] as usize]
                .iter()
                .map(|&x| x * x)
                .sum();
            let want_stored = fraction_to_stored(e_real * pow2(-2 * (m + 3)));
            assert_eq!(e[b], want_stored, "stored b{b} shift=3");
            // Independent guarded Ld recomputation (same formula, written
            // out to pin the threshold and the unconditional correction).
            let leading = max_scale[b] - 4;
            let e_raw = e_real * pow2(-2 * m + 2 * leading);
            let base = e_raw.log2() / 64.0;
            let scale_diff = leading as f64 * 2.0 / 64.0;
            let guarded = if base >= (-0.5 + scale_diff / 2.0) {
                base - scale_diff
            } else {
                -1.0
            };
            let want_ld = guarded - 3.0 * 2.0 / 64.0;
            assert_close(ld[b], want_ld, 1e-15, &format!("ld b{b} shift=3"));
        }
        // Sentinel path with shiftBits: silent band still subtracts.
        let (e_sil, ld_sil) = tns_long(&[0.0, 0.0], &[0, 2], 1, m, &[10], 4);
        assert_eq!(e_sil[0], 0);
        assert_eq!(ld_sil[0], -1.0 - 4.0 * 2.0 / 64.0);
    }

    #[test]
    fn tns_short_window_no_ld_behaviour() {
        // Short recompute must never create an Ld vector: the caller's Ld
        // (rescale values) is preserved bit-exactly, including NaN-free
        // sentinel patterns and distinct per-band values.
        let spec = [0.125, 0.25, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0];
        let offsets = [0u16, 4, 8];
        let before = vec![-1.0 + 1.0 * 2.0 / 64.0, 0.3333333333333333];
        let mut ld = before.clone();
        let e = tns_short(&spec, &offsets, 2, 9, &[9, 9], &mut ld);
        assert_eq!(ld, before, "short Ld bit-identical");
        assert!(e[0] > 0 && e[1] == 0, "short stores recomputed");
        // Long path with identical inputs DOES write Ld (control).
        let (_, ld_long) = tns_long(&spec, &offsets, 2, 9, &[9, 9], 0);
        assert_ne!(ld_long, before, "long writes Ld (control)");
    }

    #[test]
    fn tns_long_window_1024_and_short_window_128() {
        // Long window (1024 lines, m = 12): four hand-summed bands; the
        // float sum must equal the direct f64 accumulation to 1e-12 and the
        // stored image must be the §7 boundary of the shifted fraction.
        let mut spec = [0.0f64; 1024];
        for (j, v) in spec.iter_mut().enumerate() {
            *v = ((j % 7) as f64) - 3.0;
        }
        let offsets = [0u16, 3, 64, 500, 1024];
        let max_scale = [8, 8, 8, 8];
        let (e, ld) = tns_long(&spec, &offsets, 4, 12, &max_scale, 0);
        for b in 0..4 {
            let want_real: f64 = spec[offsets[b] as usize..offsets[b + 1] as usize]
                .iter()
                .map(|&x| x * x)
                .sum();
            let want_stored = fraction_to_stored(want_real * pow2(-24));
            assert_eq!(e[b], want_stored, "long1024 stored b{b}");
            // Ld net equals log2(E_f)/64 (leading cancels) when the guard
            // passes; silent bands are absent here (all bands active).
            let want_ld = (want_real * pow2(-24)).log2() / 64.0;
            assert_close(ld[b], want_ld, 1e-12, &format!("long1024 ld b{b}"));
        }
        // Short windows (8 × 128, m = 9): per-window hand check on a
        // window-varying signal; Ld untouched.
        let mut sspec = [0.0f64; 1024];
        for w in 0..8 {
            for j in 0..128 {
                sspec[w * 128 + j] = if j < 2 {
                    (w + 1) as f64
                } else if j < 4 {
                    -1.0
                } else {
                    0.0
                };
            }
        }
        let soff = [0u16, 2, 4];
        for w in 0..8 {
            let win = &sspec[w * 128..(w + 1) * 128];
            let mut ld_win = vec![-7.0, -8.0];
            let e_win = tns_short(win, &soff, 2, 9, &[9, 9], &mut ld_win);
            let a = (w + 1) as f64;
            let want0 = fraction_to_stored(2.0 * a * a * pow2(-18));
            let want1 = fraction_to_stored(2.0 * pow2(-18));
            assert_eq!(e_win[0], want0, "short128 w{w} b0");
            assert_eq!(e_win[1], want1, "short128 w{w} b1");
            assert_eq!(ld_win, vec![-7.0, -8.0], "short128 w{w} Ld");
        }
    }

    // --- Q31/FPE differential + N-4 envelope + Class A/B + absolute Ld -----

    /// TNS per-band derived tolerance (N-4, `05` §9.2 adapted to §6.2).
    ///
    /// Mirrors the rescale envelope with the TNS effective shift: short uses
    /// `f = 0` with the short re-shift `s_b = clamp(2·(maxScale−3)−1)`;
    /// long uses `f = −shiftBits` with `s_b = min(2·(leading+shiftBits),31)`.
    /// Returns `(tol, q, t, r, s_b, sum_abs_x)`; `tol` already includes the
    /// `E_ref/500` relative term. Callers must still check the
    /// supported-domain preconditions and report unsupported instead of
    /// passing with an under-estimated bound.
    fn tns_tol(
        x_q31: &[i32],
        offsets: &[u16],
        band: usize,
        max_scale: i32,
        shift_bits: i32,
        short: bool,
        e_ref: i32,
    ) -> (f64, f64, f64, f64, i32, f64) {
        let n = (offsets[band + 1] - offsets[band]) as f64;
        let sum_abs: f64 = x_q31[offsets[band] as usize..offsets[band + 1] as usize]
            .iter()
            .map(|&v| (v as f64).abs())
            .sum();
        let (q, s_b) = if short {
            // Short re-shift exponent (Q31 `band_energy_short_into`).
            let s = (2 * (max_scale - 3) - 1).clamp(-31, 31);
            // Q_b with f = 0: 2^(−31)·Σ|X| + n·2^(−33).
            let q = sum_abs * pow2(-31) + n * pow2(-33);
            (q, s)
        } else {
            let leading = max_scale - 4;
            let s = (2 * (leading + shift_bits)).min(31);
            // Q_b with f = −shiftBits: 2^(−31−2·shiftBits)·Σ|X| + ….
            let q = sum_abs * pow2(-31 - 2 * shift_bits) + n * pow2(-33 - 2 * shift_bits);
            (q, s)
        };
        // T_b mirrors the square/floor/re-shift floor (negative s_b
        // amplifies: left-shift of the truncated accumulation).
        let t = 2.0 * n * pow2(-s_b);
        let r = 1.5;
        let tol = (32.0f64).max(q + t + r) + (e_ref as f64).abs() / 500.0;
        (tol, q, t, r, s_b, sum_abs)
    }

    fn ld_to_e(ld: f64) -> f64 {
        if ld <= -1.0 {
            0.0
        } else {
            2f64.powf(ld * 64.0 + 31.0)
        }
    }

    #[test]
    fn tns_q31_fpe_differential_long_with_envelope_and_ld() {
        // Deterministic integer-mappable spectrum (no saturation, raw sums
        // above the truncation floor). Compares the float recompute against
        // the Q31 `band_energy_long_into` with the per-band N-4 envelope;
        // absolute Ld deltas are measured and reported per band, never
        // hidden behind the energy aggregate.
        let spec: [f64; 16] = [
            0.0, 0.25, -0.5, 0.75, 1.0, 0.0, 0.0, 0.0, -0.5, 0.25, 0.125, 0.0, 0.0, 0.0, 0.0, 0.0,
        ];
        let offsets = [0u16, 4, 8, 16];
        let m = 12;
        let active = 3;
        // Q31 representation lane (read-only reference).
        let q = q31_energy::map_spectrum_to_q31(&spec, m);
        let mut max_scale = Vec::new();
        q31_energy::max_scale_spec_into(&q, &offsets, active, &mut max_scale);
        let mut e_q31 = Vec::new();
        let mut ld_q31 = Vec::new();
        let mut pre = Vec::new();
        let shift = q31_energy::band_energy_long_into(
            &q,
            &max_scale,
            &offsets,
            active,
            &mut pre,
            &mut e_q31,
            &mut ld_q31,
        );
        // FPE recompute with the identical representation state.
        let (e_fpe, ld_fpe) = tns_long(&spec, &offsets, active, m, &max_scale, shift);
        let mut max_abs_ld = 0.0f64;
        let mut max_util = 0.0f64;
        for b in 0..active {
            let delta = (e_fpe[b] as i64 - e_q31[b] as i64).abs();
            let (tol, q_term, t_term, r_term, s_b, sum_abs) =
                tns_tol(&q, &offsets, b, max_scale[b], shift, false, e_q31[b]);
            let util = delta as f64 / tol;
            max_util = max_util.max(util);
            // Absolute Ld delta reported per band (not aggregated away).
            let abs_ld = (ld_fpe[b] - ld_q31[b]).abs();
            max_abs_ld = max_abs_ld.max(abs_ld);
            println!(
                "TNS-long diff b{b}: q31={} fpe={} delta={} tol={:.3} (Q={:.3} T={:.3} R={:.3} s_b={s_b} sum_abs={sum_abs:.0}) Ld q31={:.6} fpe={:.6} |ΔLd|={:.3e}",
                e_q31[b], e_fpe[b], delta, tol, q_term, t_term, r_term, ld_q31[b], ld_fpe[b], abs_ld
            );
            assert!(
                delta as f64 <= tol,
                "band {b}: delta {delta} exceeds N-4 tol {tol:.3} (q31 {} fpe {})",
                e_q31[b],
                e_fpe[b]
            );
            // Energy-domain Ld equivalence + tight absolute Ld on clean bands.
            let e_inv_q = ld_to_e(ld_q31[b]);
            let e_inv_f = ld_to_e(ld_fpe[b]);
            let ld_tol = tol.max(32.0);
            assert!(
                (e_inv_f - e_inv_q).abs() <= ld_tol,
                "band {b}: Ld energy-domain divergence"
            );
            assert!(
                abs_ld < 1e-6,
                "band {b}: absolute Ld {abs_ld:.3e} exceeds clean-band bound"
            );
        }
        println!("TNS-long N-4 max_util={max_util:.6} max_abs_Ld={max_abs_ld:.3e} shift={shift}");
        assert!(max_util < 1.0, "envelope must not be vacuous");
    }

    #[test]
    fn tns_q31_fpe_differential_short_with_envelope() {
        // Short differential: stored-only comparison with the N-4 envelope;
        // Ld is asserted bit-identical (untouched) rather than compared.
        let spec: [f64; 8] = [0.5, -0.25, 0.125, 0.75, 1.0, -1.0, 0.5, 0.25];
        let offsets = [0u16, 4, 8];
        let m = 9;
        let active = 2;
        let q = q31_energy::map_spectrum_to_q31(&spec, m);
        let mut max_scale = Vec::new();
        q31_energy::max_scale_spec_into(&q, &offsets, active, &mut max_scale);
        let mut e_q31 = Vec::new();
        q31_energy::band_energy_short_into(&q, &max_scale, &offsets, active, &mut e_q31);
        let mut ld = vec![0.111, -0.222];
        let ld_before = ld.clone();
        let mut e_fpe = vec![0i32; active];
        tns_recompute_energy_into(
            &spec, &offsets, active, m, &max_scale, 0, true, &mut e_fpe, &mut ld,
        );
        assert_eq!(ld, ld_before, "short Ld untouched in differential");
        let mut max_util = 0.0f64;
        for b in 0..active {
            let delta = (e_fpe[b] as i64 - e_q31[b] as i64).abs();
            let (tol, q_term, t_term, r_term, s_b, sum_abs) =
                tns_tol(&q, &offsets, b, max_scale[b], 0, true, e_q31[b]);
            let util = delta as f64 / tol;
            max_util = max_util.max(util);
            println!(
                "TNS-short diff b{b}: q31={} fpe={} delta={} tol={:.3} (Q={:.3} T={:.3} R={:.3} s_b={s_b} sum_abs={sum_abs:.0})",
                e_q31[b], e_fpe[b], delta, tol, q_term, t_term, r_term
            );
            assert!(
                delta as f64 <= tol,
                "short band {b}: delta {delta} exceeds tol {tol:.3}"
            );
        }
        println!("TNS-short N-4 max_util={max_util:.6}");
        assert!(max_util < 1.0);
    }

    #[test]
    fn tns_class_a_b_sentinel_accounting() {
        // Class A (mapping-to-zero): physical energy > 0, every mapped Q31
        // line is zero, Q31 stores 0, FPE may store positive.
        // Class B (post-shift truncation vs rounding): mapped lines nonzero,
        // Q31 rescale shift/floor yields 0, FPE rounds to positive.
        // Both are expected divergences; FPE must not reproduce Q31
        // truncation. Counted and reported per vector, never aggregated away.
        let m = 12;
        // Class A setup: sub-LSB spectrum (maps to all-zero). Half an LSB at
        // m = 12 is 2^(12−32) = 2^−20 ≈ 9.5e−07; use 1/2^22 so every line
        // rounds to zero.
        let spec_a = [
            1.0 / 4_194_304.0,
            -1.0 / 4_194_304.0,
            0.5 / 4_194_304.0,
            0.0,
        ];
        let offsets = [0u16, 4];
        let q_a = q31_energy::map_spectrum_to_q31(&spec_a, m);
        assert!(
            q_a.iter().all(|&v| v == 0),
            "Class A setup: all Q31 lines zero"
        );
        let mut max_a = Vec::new();
        q31_energy::max_scale_spec_into(&q_a, &offsets, 1, &mut max_a);
        let mut e_q31_a = Vec::new();
        let mut ld_q31_a = Vec::new();
        let mut pre_a = Vec::new();
        let shift_a = q31_energy::band_energy_long_into(
            &q_a,
            &max_a,
            &offsets,
            1,
            &mut pre_a,
            &mut e_q31_a,
            &mut ld_q31_a,
        );
        let (e_fpe_a, ld_fpe_a) = tns_long(&spec_a, &offsets, 1, m, &max_a, shift_a);
        let e_real_a: f64 = spec_a.iter().map(|&x| x * x).sum();
        assert!(e_real_a > 0.0, "Class A physical energy positive");
        assert_eq!(e_q31_a[0], 0, "Class A Q31 stores zero");
        println!(
            "Class A: e_real={:.3e} q31={} fpe={} ld_q31={} ld_fpe={} shift={shift_a}",
            e_real_a, e_q31_a[0], e_fpe_a[0], ld_q31_a[0], ld_fpe_a[0]
        );
        // FPE may store 0 or positive depending on magnitude; either way the
        // classification is by stored state (mapping-to-zero), not by Ld.
        assert_eq!(ld_q31_a[0], -1.0 - shift_a as f64 * 2.0 / 64.0);

        // Class B setup: mapped lines nonzero but Q31 long shift yields 0
        // while FPE rounds to positive. Search a deterministic sweep for the
        // first such band (low-energy truncation vs rounding).
        let mut found_b = None;
        for scale in 1i32..=120 {
            let amp = scale as f64 / 1024.0;
            let spec = [amp, -amp, amp / 2.0, 0.0];
            let q = q31_energy::map_spectrum_to_q31(&spec, m);
            if q.iter().all(|&v| v == 0) {
                continue;
            }
            let mut max = Vec::new();
            q31_energy::max_scale_spec_into(&q, &offsets, 1, &mut max);
            let mut e_q = Vec::new();
            let mut ld_q = Vec::new();
            let mut pre = Vec::new();
            let sh = q31_energy::band_energy_long_into(
                &q, &max, &offsets, 1, &mut pre, &mut e_q, &mut ld_q,
            );
            let (e_f, ld_f) = tns_long(&spec, &offsets, 1, m, &max, sh);
            if e_q[0] == 0 && e_f[0] > 0 {
                found_b = Some((spec, q, max, sh, e_q, ld_q, e_f, ld_f));
                break;
            }
        }
        let (spec_b, q_b, max_b, sh_b, e_q_b, ld_q_b, e_f_b, ld_f_b) =
            found_b.expect("Class B example must exist in sweep");
        let sum_abs_b: i64 = q_b.iter().map(|&v| (v as i64).abs()).sum();
        assert!(sum_abs_b > 0, "Class B mapped lines nonzero");
        println!(
            "Class B: spec={spec_b:?} sum_abs={sum_abs_b} maxScale={max_b:?} shift={sh_b} q31={} fpe={} ld_q={} ld_f={} |ΔLd|={:.3e}",
            e_q_b[0],
            e_f_b[0],
            ld_q_b[0],
            ld_f_b[0],
            (ld_f_b[0] - ld_q_b[0]).abs()
        );
        // Classification is by stored state (Q31 0 vs FPE positive with
        // nonzero mapped lines), not by Ld crossing −1.0. Here the guarded
        // Ld is finite on both sides (pre-shift accumulation resolved) and
        // matches to 1e-12; only the post-shift stored truncation differs by
        // one LSB, which the N-4 envelope accepts (tol ≥ 32).
        assert_eq!(e_q_b[0], 0, "Class B Q31 stores zero");
        assert!(e_f_b[0] > 0, "Class B FPE stores positive");
        let abs_ld_b = (ld_f_b[0] - ld_q_b[0]).abs();
        println!("Class B |ΔLd| = {abs_ld_b:.3e}");
        assert!(
            abs_ld_b < 1e-12,
            "Class B Ld must match (both finite from pre-shift)"
        );
        let (tol_b, _, _, _, _, _) = tns_tol(&q_b, &offsets, 0, max_b[0], sh_b, false, e_q_b[0]);
        assert!(
            (e_f_b[0] as i64 - e_q_b[0] as i64).abs() as f64 <= tol_b,
            "Class B delta within N-4 envelope"
        );
        assert_eq!(
            fraction_to_stored(e_f_b[0] as f64 / 2_147_483_648.0),
            e_f_b[0]
        );
    }

    #[test]
    fn tns_absolute_ld_differences_reported_not_aggregated() {
        // Sweep a deterministic ramp across bands; report the per-band
        // absolute Ld deltas and the maximum, and gate the clean bands
        // tightly. Aggregate energy tolerance alone would hide an Ld-only
        // regression, so this test fails on any unexplained Ld shift.
        let mut spec = [0.0f64; 32];
        for (j, v) in spec.iter_mut().enumerate() {
            *v = ((j % 5) as f64 - 2.0) * 0.125;
        }
        let offsets = [0u16, 8, 16, 24, 32];
        let m = 12;
        let active = 4;
        let q = q31_energy::map_spectrum_to_q31(&spec, m);
        let mut max_scale = Vec::new();
        q31_energy::max_scale_spec_into(&q, &offsets, active, &mut max_scale);
        let mut e_q31 = Vec::new();
        let mut ld_q31 = Vec::new();
        let mut pre = Vec::new();
        let shift = q31_energy::band_energy_long_into(
            &q,
            &max_scale,
            &offsets,
            active,
            &mut pre,
            &mut e_q31,
            &mut ld_q31,
        );
        let (e_fpe, ld_fpe) = tns_long(&spec, &offsets, active, m, &max_scale, shift);
        let mut max_abs = 0.0f64;
        for b in 0..active {
            let abs_ld = (ld_fpe[b] - ld_q31[b]).abs();
            max_abs = max_abs.max(abs_ld);
            let delta = (e_fpe[b] as i64 - e_q31[b] as i64).abs();
            println!(
                "Ld-report b{b}: q31_ld={:.9} fpe_ld={:.9} |ΔLd|={:.3e} e_q31={} e_fpe={} ΔE={}",
                ld_q31[b], ld_fpe[b], abs_ld, e_q31[b], e_fpe[b], delta
            );
            // Clean bands (both stored nonzero) must agree tightly; sentinel
            // bands are classified by stored state elsewhere and invert to ≈0.
            if e_q31[b] != 0 && e_fpe[b] != 0 {
                assert!(
                    abs_ld < 1e-6,
                    "clean band {b}: |ΔLd| {abs_ld:.3e} too large"
                );
            } else {
                let e_inv_f = ld_to_e(ld_fpe[b]);
                let e_inv_q = ld_to_e(ld_q31[b]);
                assert!(
                    (e_inv_f - e_inv_q).abs() <= 32.0,
                    "sentinel band {b}: energy-domain divergence"
                );
            }
        }
        println!("Ld-report max |ΔLd| = {max_abs:.3e} (shift={shift})");
    }

    #[test]
    fn tns_a17_fixture_exercises_short_window_tns_recompute() {
        // A-17 narrow scope: the resynthesized `fpe-tns-active-short-stereo`
        // fixture must genuinely drive short windows with TNS recompute
        // (element-global tns_active true on short windows). Additive check:
        // runs the Q31 reference engine over the fixture and asserts short
        // frames and TNS-active short windows are observed.
        use crate::aac_enc::block_switch::{BlockSwitch, WindowSequence};
        use crate::aac_enc::psy::{ChannelInput, PsyChannel, PsyScratch, PsyTables};
        use crate::aac_enc::testdata;
        use crate::aac_enc::transform::ChannelTransform;

        let name = "fpe-tns-active-short-stereo";
        let fixture = testdata::fixture_samples(name);
        assert_eq!(fixture.len(), 2, "stereo fixture");
        assert_eq!(fixture[0].len(), 48000, "1.0 s @48k");
        assert_eq!(fixture[1].len(), 48000);
        // Determinism probe: first sample is exact zero (gate open but the
        // 8 kHz sine starts at phase 0); the gate boundary at 4800 is
        // silence; a mid-gate off-zero-crossing sample carries the tone.
        assert_eq!(fixture[0][0], 0);
        assert_eq!(fixture[0][4800], 0, "gate boundary is silence");
        assert_ne!(fixture[0][1201], 0, "tone present inside gate");

        let channels = 2;
        let sr = testdata::fixture_sample_rate(name);
        assert_eq!(sr, 48000);
        let (ch_br, tns_br) = (96000, 192000);
        let ((lcfg, _), (scfg, _)) = testdata::load_cfg(sr, ch_br);
        let tables =
            PsyTables::for_rate(sr, ch_br, tns_br, 19293, lcfg.min_snr_ld, scfg.min_snr_ld)
                .expect("tables build");
        let mut switches: Vec<BlockSwitch> = (0..channels).map(|_| BlockSwitch::new()).collect();
        let mut transforms: Vec<ChannelTransform> =
            (0..channels).map(|_| ChannelTransform::new()).collect();
        let mut ch_state: Vec<PsyChannel> = (0..channels)
            .map(|_| PsyChannel::new(tables.long_cfg.pcm_quant_q31()))
            .collect();
        let mut ws = PsyScratch::default();
        let n = fixture[0].len().div_ceil(1024) + 2;
        let mut short_frames = 0usize;
        let mut tns_recompute_windows = 0usize;
        let mut tns_shift_nonzero = 0usize;
        for f in 0..n {
            let fresh: Vec<[i16; 1024]> = (0..channels)
                .map(|c| {
                    let mut block = [0i16; 1024];
                    for (i, v) in block.iter_mut().enumerate() {
                        let t = f * 1024 + i;
                        *v = if t < fixture[c].len() {
                            fixture[c][t]
                        } else {
                            0
                        };
                    }
                    block
                })
                .collect();
            for (sw, fr) in switches.iter_mut().zip(fresh.iter()) {
                sw.process(fr, false);
            }
            {
                let (l, r) = switches.split_at_mut(1);
                BlockSwitch::synchronize_pair(&mut l[0], &mut r[0]);
            }
            let mut specs: Vec<[f64; 1024]> = Vec::with_capacity(channels);
            let mut seqs = Vec::with_capacity(channels);
            let mut groupings = Vec::with_capacity(channels);
            for (ch, (tr, sw)) in transforms.iter_mut().zip(switches.iter()).enumerate() {
                let o = tr
                    .process(&fresh[ch], sw.sequence(), sw.shape())
                    .expect("mdct");
                let mut spec = [0.0f64; 1024];
                for (d, s) in spec.iter_mut().zip(o.iter()) {
                    *d = *s as f64;
                }
                specs.push(spec);
                seqs.push(sw.sequence());
                groupings.push(sw.grouping());
            }
            let (head, tail) = specs.split_at_mut(1);
            let lin = ChannelInput {
                spectrum: &mut head[0],
                seq: seqs[0],
                grouping: groupings[0],
                is_lfe: false,
            };
            let rin = ChannelInput {
                spectrum: &mut tail[0],
                seq: seqs[1],
                grouping: groupings[1],
                is_lfe: false,
            };
            let (pl, pr) = ch_state.split_at_mut(1);
            let (ol, _, _) = crate::aac_enc::psy::process_pair_reuse(
                &mut pl[0],
                Some(&mut pr[0]),
                lin,
                Some(rin),
                &tables,
                &mut ws,
            );
            if seqs[0] == WindowSequence::Short {
                short_frames += 1;
                // TNS recompute runs per TNS-active window; the Q31 output
                // exposes it via per-channel TNS state and nrgScaling.
                let active_any = pl[0]
                    .tns_state()
                    .active
                    .iter()
                    .chain(pr[0].tns_state().active.iter())
                    .any(|a| a[0] || a[1]);
                if active_any {
                    tns_recompute_windows += 1;
                }
            }
            if ol.tns_shift != 0 {
                tns_shift_nonzero += 1;
            }
        }
        println!(
            "A-17 {name}: frames={n} short={short_frames} tns_active_short_windows={tns_recompute_windows} tns_shift_nonzero={tns_shift_nonzero}"
        );
        assert!(
            short_frames > 0,
            "A-17: fixture must produce short windows (got 0)"
        );
        assert!(
            tns_recompute_windows > 0,
            "A-17: fixture must exercise short-window TNS recompute (got 0 TNS-active short windows)"
        );
        // Production reachability note (A-14/F-6): short elements never carry
        // a nonzero element-global tnsSpecShift; the TNS recompute still runs
        // per active window with shiftBits = 0.
        assert_eq!(
            tns_shift_nonzero, 0,
            "A-17: short-element tns_shift must stay 0 (structural)"
        );
    }
}
