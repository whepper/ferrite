//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! FPE float tuning constants — Q31-fraction domain (`docs/fpe/05` §3).
//!
//! Every constant is derived **exactly** from the Q31 tuning literal
//! (`/2^31` is exact for 32-bit patterns in f64); the literals themselves
//! are consumed from the existing licensed Q31 modules, never re-typed
//! (provenance: `docs/fpe/12` §4, risk R-05). These are psychoacoustic
//! tuning, not numerical scaffolding (`docs/fpe/03` §5). Per-band tables
//! (PCM floors, clip, spreading factors) come from the `SfbConfig` f64
//! accessors, which are already fraction-domain; FPE consumes the
//! committed tabulated values and never recomputes spreading factors or
//! minSnr in float (Stage 2 only).
//!
//! **M1 status (FPE-1.1):** scaffolding — no float kernel consumes these
//! yet (the value lane delegates to Q31). The unit tests below gate the
//! exact derivation now; the float kernels consume them from M3 onward.
//! Reference anchors verified at M0 (`docs/fpe/m0-verification.md` M0.2,
//! A-2 resolved): `psy_configuration.h:113/114/117-118`,
//! `psy_configuration.cpp:329/591-596`.

// M1 scaffolding: consumed by the float kernels from M3 onward (roadmap
// `09` FPE-1.1); exactness is gated by the unit tests in this module now.
#![allow(dead_code)]

use crate::aac_enc::sfb::{CLIP_ENERGY_Q31, PCM_QUANT_NOISE_Q31};
use crate::aac_enc::thresholds::{C_RATIO_Q31, MAX_INCREASE, MAX_Q31, MIN_REMAINING_Q15};

/// Q31 fraction denominator: `v_f = v_st / 2^31` (`docs/fpe/05` §1.3).
const Q31_DENOM: f64 = 2_147_483_648.0;

/// `C_RATIO_f` (`05` §3): threshold-init ratio `E_f · C_RATIO_f`.
/// `0x02940a10/2^31 ≈ 0.020143 = 16·10^(−29/10)` (the ×16 is the
/// `THR_SHIFTBITS` headroom removed by the PCM-floor stage).
/// Upstream: `psy_configuration.h:117-118`; Q31: `thresholds::C_RATIO_Q31`.
pub(crate) const C_RATIO_F: f64 = C_RATIO_Q31 as f64 / Q31_DENOM;

/// `CLIP_f_long` (`05` §3): long-block clip energy fraction
/// (`SfbConfig::clip_energy()` long). Upstream: `psy_configuration.cpp:595`;
/// Q31: `sfb::CLIP_ENERGY_Q31`.
pub(crate) const CLIP_F_LONG: f64 = CLIP_ENERGY_Q31 as f64 / Q31_DENOM;

/// `CLIP_f_short` (`05` §3): short-block clip energy fraction — C shifts
/// the stored integer (`clipEnergy >>= 6`, `psy_configuration.cpp:607`)
/// before the fraction conversion, mirrored exactly.
pub(crate) const CLIP_F_SHORT: f64 = (CLIP_ENERGY_Q31 >> 6) as f64 / Q31_DENOM;

/// `PCM_NOISE_f` (`05` §3): per-band PCM quantization floor factor,
/// `pcm_f[b] = width_b · PCM_NOISE_f` (the −96 dB / 1-bit absolute floor;
/// FDK's LC model has no ATH table, `docs/fpe/03` §4.5). Upstream:
/// `psy_configuration.cpp:329`; Q31: `sfb::PCM_QUANT_NOISE_Q31`.
pub(crate) const PCM_NOISE_F: f64 = PCM_QUANT_NOISE_Q31 as f64 / Q31_DENOM;

/// `MIN_REMAINING` (`05` §3): pre-echo remaining-threshold factor,
/// dimensionless `0x0148` Q15 = 328/32768 ≈ 0.01001. Upstream:
/// `psy_configuration.cpp:592`; Q31: `thresholds::MIN_REMAINING_Q15`.
pub(crate) const MIN_REMAINING_F: f64 = MIN_REMAINING_Q15 as f64 / 32768.0;

/// `MAX_INCREASE` (`05` §3): pre-echo maximum increase factor,
/// dimensionless integer 2. Upstream: `psy_configuration.cpp:591`;
/// Q31: `thresholds::MAX_INCREASE`.
pub(crate) const MAX_INCREASE_F: f64 = MAX_INCREASE as f64;

/// `MAXVAL_f` (`05` §3): fraction-domain saturation level
/// `(2^31−1)/2^31 ≈ 1.0` — the float image of stored `MAXVAL_DBL`. Clamps
/// fraction values where C saturates; distinct from the boundary i32
/// clamp (`05` §1.4). Q31: `thresholds::MAX_Q31`.
pub(crate) const MAXVAL_F: f64 = MAX_Q31 as f64 / Q31_DENOM;

/// `LD_ZERO` (`05` §3): Ld-domain sentinel for non-positive energy
/// (the float image of the `i32::MIN` stored sentinel).
pub(crate) const LD_ZERO: f64 = -1.0;

/// `THR_LD_FLOOR` (`05` §3): threshold-Ld floor −33/64 = −0.515625
/// (`psy_main.cpp:1071-1075/1124-1128`, `fixMax(LdDataVector, -0.515625)`).
pub(crate) const THR_LD_FLOOR: f64 = -33.0 / 64.0;

/// Integer shift counts (`05` §3), re-exported from the Q31 module (not
/// float constants): `THR_SHIFTBITS = 4` (⇒ /16 in the fraction domain)
/// and `PCM_THR_SCALE = 16` (per-window `energyShift` increment).
/// Upstream anchors: `psy_configuration.h:113/114` (A-2, verified M0).
// M1 scaffolding: consumed by the float kernels from M3 onward.
#[allow(unused_imports)]
pub(crate) use crate::aac_enc::thresholds::{
    PCM_QUANT_THR_SCALE as PCM_THR_SCALE, THR_SHIFTBITS as THR_SHIFTBITS_I32,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::sfb::{BlockKind, SfbConfig};

    /// Exact bit equality (the derivations are exact power-of-two scalings
    /// of 32-bit patterns, so `==` on the bit patterns is the gate).
    fn assert_bits(got: f64, want: f64, what: &str) {
        assert_eq!(got.to_bits(), want.to_bits(), "{what}: {got} vs {want}");
    }

    fn cfg_48k(kind: BlockKind) -> SfbConfig {
        SfbConfig::new(48000, kind, 96000, 19293, true, true).expect("sfb config")
    }

    #[test]
    fn float_constants_equal_q31_over_2_31_exactly() {
        // FPE-1.1 gate: each float constant equals its Q31 literal / 2^31
        // exactly (docs/fpe/05 §3; risk R-05 tuning-drift gate).
        assert_bits(C_RATIO_F, 0x0294_0a10u32 as f64 / Q31_DENOM, "C_RATIO_F");
        assert_bits(
            CLIP_F_LONG,
            0x7735_93ffu32 as f64 / Q31_DENOM,
            "CLIP_F_LONG",
        );
        assert_bits(
            CLIP_F_SHORT,
            (0x7735_93ffu32 >> 6) as f64 / Q31_DENOM,
            "CLIP_F_SHORT",
        );
        assert_bits(
            PCM_NOISE_F,
            0x0054_7062u32 as f64 / Q31_DENOM,
            "PCM_NOISE_F",
        );
        assert_bits(MIN_REMAINING_F, 328.0 / 32768.0, "MIN_REMAINING_F");
        assert_bits(MAX_INCREASE_F, 2.0, "MAX_INCREASE_F");
        assert_bits(MAXVAL_F, i32::MAX as f64 / Q31_DENOM, "MAXVAL_F");
        assert_bits(LD_ZERO, -1.0, "LD_ZERO");
        assert_bits(THR_LD_FLOOR, -0.515625, "THR_LD_FLOOR");
        assert_eq!(THR_SHIFTBITS_I32, 4, "THR_SHIFTBITS (A-2 anchor)");
        assert_eq!(PCM_THR_SCALE, 16, "PCM_THR_SCALE (A-2 anchor)");
        // Sanity against the reference derivation comment
        // (psy_configuration.h:118: 16·10^(−29/10)); the stored literal is
        // the authority, the physical value is a loose cross-check.
        assert!((C_RATIO_F - 16.0 * 10f64.powf(-29.0 / 10.0)).abs() < 1e-6);
    }

    #[test]
    fn clip_constants_match_sfb_config_accessors() {
        // The SfbConfig f64 accessors are already fraction-domain
        // (docs/fpe/05 §1.3); FPE's constants must agree exactly.
        let long = cfg_48k(BlockKind::Long);
        let short = cfg_48k(BlockKind::Short);
        assert_bits(CLIP_F_LONG, long.clip_energy(), "CLIP_F_LONG vs long cfg");
        assert_bits(
            CLIP_F_SHORT,
            short.clip_energy(),
            "CLIP_F_SHORT vs short cfg",
        );
    }

    #[test]
    fn pcm_noise_constant_reproduces_per_band_floors_exactly() {
        // pcm_f[b] = width_b · PCM_NOISE_f, exact in f64 (the integer
        // product width·PCM_QUANT_NOISE is < 2^31, both scalings exact).
        for kind in [BlockKind::Long, BlockKind::Short] {
            let cfg = cfg_48k(kind);
            let offsets = cfg.offsets();
            let pcm_f = cfg.pcm_quant();
            let pcm_st = cfg.pcm_quant_q31();
            assert_eq!(pcm_f.len(), cfg.sfb_count());
            for b in 0..cfg.sfb_count() {
                let width = (offsets[b + 1] - offsets[b]) as f64;
                assert_bits(
                    pcm_f[b],
                    width * PCM_NOISE_F,
                    &format!("{kind:?} pcm_f band {b}"),
                );
                assert_bits(
                    pcm_f[b],
                    pcm_st[b] as f64 / Q31_DENOM,
                    &format!("{kind:?} pcm_f vs stored band {b}"),
                );
            }
        }
    }

    #[test]
    fn tabulated_mask_factors_are_fractions_and_match_stored() {
        // FPE consumes the committed tabulated factors (never recomputes
        // them in float; docs/fpe/03 §4.3, Stage-2 boundary). The f64
        // accessors must be the exact /2^31 image of the stored ints.
        for kind in [BlockKind::Long, BlockKind::Short] {
            let cfg = cfg_48k(kind);
            let (hi, lo) = cfg.mask_factors();
            let (hi_spr, lo_spr) = cfg.mask_factors_spr();
            let (hi_st, lo_st) = cfg.mask_factors_q31();
            let (hi_spr_st, lo_spr_st) = cfg.mask_factors_spr_q31();
            for (name, f, st) in [
                ("mask_hi", hi, hi_st),
                ("mask_lo", lo, lo_st),
                ("mask_hi_spr", hi_spr, hi_spr_st),
                ("mask_lo_spr", lo_spr, lo_spr_st),
            ] {
                assert_eq!(f.len(), st.len(), "{name} len");
                for (i, (&v, &q)) in f.iter().zip(st.iter()).enumerate() {
                    assert_bits(v, q as f64 / Q31_DENOM, &format!("{name}[{i}]"));
                    assert!(
                        (0.0..1.0).contains(&v),
                        "{name}[{i}] = {v} outside [0,1) (contractive envelope)"
                    );
                }
            }
        }
    }
}
