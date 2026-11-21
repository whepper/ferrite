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
//! Planned content still outstanding: **FPE-3.2** float TNS energy recompute
//! (`05` §6.2) and **FPE-7.1** float MS energies (`05` §6.6).
//!
//! Upstream: `band_nrg.cpp:140-184/192-256/264-288/296-361`. Q31 mirrors
//! (read-only): `energy.rs:161/249/215/321`.

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
}
