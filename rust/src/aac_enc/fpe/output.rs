//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! FPE output boundary and shared-output assembly.
//!
//! **M3 (FPE-3.1 + FPE-3.2):** the boundary quantization of `docs/fpe/05` §7
//! is now **active for the rescale band energies and the TNS recompute
//! energies** — [`fraction_to_stored`] converts the f64 fraction energy
//! `E_f` to the stored `i32` `E_st = clamp(round(E_f·2^31))` at the
//! orchestration boundary (rescale) and inside the TNS recompute kernel
//! (which writes stored ints directly). All other value kernels (thresholds, spreading, PCM floor,
//! pre-echo, MS) still delegate to Q31 and pass their results through
//! unchanged. [`assemble_channel`] itself remains a pass-through of the
//! stored ints / f64 Ld it is handed (the energies it receives are already
//! boundary-quantized by the orchestration).
//!
//! **M2 (FPE-2.3, pass-through):** [`assemble_channel`] constructs the shared
//! [`crate::aac_enc::psy::ChannelOutput`] /
//! [`crate::aac_enc::psy::GroupedOutput`] types by calling the Q31 output
//! assembler ([`crate::aac_enc::psy::finish_channel`]) on the Q31-produced
//! stored ints / f64 Ld, **unchanged**. There is no boundary quantization in
//! M2 for the value lane: the value lane delegates to Q31, so the results
//! already satisfy the output contract and pass through bit-identically
//! (roadmap `09` M2 pass-through contract; risk R-19). The integer
//! representation fields
//! (`mdctScale`/`finalShift`/`tnsShift`/`maxScale`) are produced by the reused
//! integer-exact Q31 helpers and pass through exactly (ADR-0005).
//!
//! **M2 invariant guards (FPE-2.4, `docs/fpe/05` §8 / `04` §7):**
//! [`guard_channel_output`] checks the representation-lane and output-layout
//! invariants that are observable in M2 — non-negative `finalShift`, the
//! grouped stride layout contract (lengths == `groups·sfb_cnt`, zero stride
//! padding), the threshold-Ld floor (−33/64), and finite Ld vectors. These
//! are `debug_assert!` guards: they catch bugs during development and change
//! no release behaviour (the M2 value lane is Q31-produced and satisfies them
//! by construction). The D-022 structural preconditions are asserted at
//! orchestration entry with the same release-active `assert!` as Q31
//! (`super::orchestrate`), since they are caller-contract violations, not
//! numerical noise (`docs/fpe/06` §6).
//!
//! **M3+ (`docs/fpe/05` §7, active per-kernel only when a float kernel
//! lands):** boundary quantization `stored_i32 =
//! clamp(round(v_f·2^31), i32::MIN, i32::MAX)` (round-to-nearest, the same
//! convention as `energy::map_spectrum_to_q31_into`), Ld vectors per the
//! `05` §6.8 guard/floor conventions, grouped vectors via integer regroup of
//! quantized per-window values (`05` §6.7), and the f64 spectrum (mutated by
//! the reused TNS) passed through unchanged. The float-value guards (finite
//! energies/thresholds/history, non-negative values, saturating clamps,
//! fraction-range) activate with the float value lane. In **FPE-3.1R1** a
//! non-finite fraction at the energy boundary is a **release-active
//! assertion** in [`fraction_to_stored`], never a silent cast. Graceful
//! `FpeError` / frame-level skip-with-note handling is intended for
//! **FPE-3.1R2** but is **not implemented yet** (`docs/fpe/04` §7, `06` §6,
//! `10` R-16). Boundary rounding/rescaling is confined to the `05` §7
//! conversion for kernels already floated; no additional quantization is
//! introduced.
//!
//! Upstream: `psy_main.cpp:1058-1076/1089-1101/1120-1128/1251-1295`
//! (`LdDataVector`, output build). Q31 mirrors (read-only):
//! `psy.rs:1299-1551` (`group_short_channel`, `finish_channel`,
//! `grouped_ld`, `grouped_ld_clamped`).

use crate::aac_enc::psy::{self, ChannelOutput};
use crate::aac_enc::sfb::{SfbConfig, THRESHOLD_LD_FLOOR};
use crate::aac_enc::thresholds::PreEchoState;

/// Assemble one channel's output from the stored ints / f64 Ld handed in,
/// then run the invariant guards.
///
/// Signature mirrors [`crate::aac_enc::psy::finish_channel`] (the Q31 output
/// assembler) exactly; the arguments are forwarded unchanged. Grouping
/// (short), the fresh grouped MS-Ld (`calc_ld_data`), the grouped/long Ld
/// vectors, and the `maxSfbPerGroup` rules are all performed by the reused
/// Q31 assembler. From M3 (FPE-3.1) the `en`/`eld_true` inputs may be
/// float-derived (boundary-quantized energies / §6.8 Ld); all other inputs
/// are Q31-produced and pass through unchanged.
#[allow(clippy::too_many_arguments)]
pub(super) fn assemble_channel(
    preecho: &PreEchoState,
    thr: &[Vec<i32>],
    en: &[Vec<i32>],
    eld_true: &[Vec<f64>],
    spread: &[Vec<i32>],
    maxsc: &[Vec<i32>],
    spectrum: &[f64],
    conf: &SfbConfig,
    short_ch: bool,
    active: usize,
    mdct_scale: i32,
    final_shift: i32,
    tns_shift: i32,
    grouping: (u8, [u8; 4]),
    ms: &[Vec<i32>],
    min_snr_in: &[i32],
) -> ChannelOutput {
    // No extra boundary rounding here: the energies were already converted
    // by the orchestration (`fraction_to_stored`); the Q31 assembler builds
    // the shared contract from the stored ints / f64 Ld handed in.
    let out = psy::finish_channel(
        preecho,
        thr,
        en,
        eld_true,
        spread,
        maxsc,
        spectrum,
        conf,
        short_ch,
        active,
        mdct_scale,
        final_shift,
        tns_shift,
        grouping,
        ms,
        min_snr_in,
    );
    guard_channel_output(&out, conf, short_ch, active, grouping);
    out
}

/// FPE fraction → stored `i32` boundary quantization (`docs/fpe/05` §7):
///
/// ```text
/// stored_i32 = clamp(round(v_f · 2^31), i32::MIN, i32::MAX)
/// ```
///
/// Round-to-nearest is the same convention as the Q31 integer lane's single
/// approximation point
/// ([`crate::aac_enc::energy::map_spectrum_to_q31_into`]). Energy callers
/// pass non-negative fraction values, so the upper saturation is
/// `2^31 − 1` (`MAXVAL_f`, C's `MAXVAL_DBL`); the fraction-domain
/// `MAXVAL_f` clamp and this stored-domain clamp produce the same stored
/// value (`05` §1.4 keeps them separate).
///
/// **M3 (FPE-3.1 + FPE-3.2):** active for the rescale band energies (at the
/// orchestration boundary) and the TNS recompute energies (inside the
/// recompute kernel via the same §7 function); every other value kernel
/// still passes Q31 results through unchanged.
pub(super) fn fraction_to_stored(v_f: f64) -> i32 {
    // S-4 (FPE-3.1R1): this is deliberately an ordinary `assert!`, not a
    // `debug_assert!`. A non-finite fraction is unreachable for finite
    // MDCT/PCM input (sums of squares cannot overflow f64 here), but if it
    // ever occurs it must fail loudly in release rather than silently
    // casting to zero. A frame-level error path is deferred to FPE-3.1R2;
    // the existing FPE dispatch signature returns values, not `Result`.
    assert!(
        v_f.is_finite(),
        "FPE boundary: non-finite fraction value {v_f}"
    );
    (v_f * 2_147_483_648.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

/// Representation-lane / output-layout invariant guards (`docs/fpe/05`
/// §8, `04` §7). `debug_assert!` only — no release behaviour change. From
/// M3 the rescale energies are float-derived (sums of squares, boundary
/// clamp) and the Ld vectors follow the §6.8 conventions, so these hold by
/// construction; the guards localize any divergence introduced by a float
/// kernel.
fn guard_channel_output(
    out: &ChannelOutput,
    conf: &SfbConfig,
    short_ch: bool,
    active: usize,
    grouping: (u8, [u8; 4]),
) {
    // Representation lane (ADR-0005): finalShift is non-negative (C asserts
    // this; `energy::rescale_shifts` debug_asserts it). mdctScale/finalShift/
    // tnsShift/maxScale are Q31-produced and pass through exactly.
    debug_assert!(out.final_shift >= 0, "FPE repr lane: finalShift >= 0");

    // Stored thresholds are non-negative (init/clip/spread/PCM/pre-echo all
    // preserve non-negativity). Energies are sums of squares; guard them too.
    for (w, thr) in out.thresholds.iter().enumerate() {
        for (b, &t) in thr.iter().enumerate() {
            debug_assert!(t >= 0, "FPE invariant: threshold w{w} b{b} >= 0 (got {t})");
        }
    }
    for (w, en) in out.energies.iter().enumerate() {
        for (b, &e) in en.iter().enumerate() {
            debug_assert!(e >= 0, "FPE invariant: energy w{w} b{b} >= 0 (got {e})");
        }
    }

    // Per-window energy Ld finite (no NaN/Inf propagation, `05` §8).
    for (w, ld) in out.energy_ld.iter().enumerate() {
        for (b, &v) in ld.iter().enumerate() {
            debug_assert!(
                v.is_finite(),
                "FPE invariant: energy_ld w{w} b{b} finite (got {v})"
            );
        }
    }

    if short_ch {
        if let Some(g) = &out.grouped {
            let sfb_cnt = conf.sfb_count();
            let no_groups = grouping.0 as usize;
            let no_sfb = no_groups * sfb_cnt;
            // Grouped stride layout contract (`psy.rs` GroupedOutput docs):
            // flat `no_of_groups * sfb_cnt`, active band `b` of group `g` at
            // `g*sfb_cnt + b`, stride padding zeros.
            debug_assert_eq!(g.energies.len(), no_sfb, "FPE grouped energies stride");
            debug_assert_eq!(g.thresholds.len(), no_sfb, "FPE grouped thresholds stride");
            debug_assert_eq!(g.spread.len(), no_sfb, "FPE grouped spread stride");
            debug_assert_eq!(g.energy_ld.len(), no_sfb, "FPE grouped energy_ld stride");
            debug_assert_eq!(
                g.threshold_ld.len(),
                no_sfb,
                "FPE grouped threshold_ld stride"
            );
            debug_assert_eq!(g.min_snr.len(), no_sfb, "FPE grouped min_snr stride");
            debug_assert_eq!(
                g.ms_energies.len(),
                no_sfb,
                "FPE grouped ms_energies stride"
            );
            debug_assert_eq!(g.ms_ld.len(), no_sfb, "FPE grouped ms_ld stride");
            debug_assert_eq!(g.offsets.len(), no_sfb + 1, "FPE grouped offsets stride");
            debug_assert_eq!(g.spectrum.len(), 1024, "FPE grouped spectrum length");
            // Stride padding bands (active..sfb_cnt) are zero, never consumed.
            // (ms_ld/energy_ld padding hold the Ld sentinel, not zero.)
            for grp in 0..no_groups {
                for b in active..sfb_cnt {
                    let i = grp * sfb_cnt + b;
                    debug_assert_eq!(g.energies[i], 0, "FPE grouped energies padding {i}");
                    debug_assert_eq!(g.thresholds[i], 0, "FPE grouped thresholds padding {i}");
                    debug_assert_eq!(g.spread[i], 0, "FPE grouped spread padding {i}");
                    debug_assert_eq!(g.ms_energies[i], 0, "FPE grouped ms_energies padding {i}");
                }
            }
            // Threshold-Ld floor −33/64 (`05` §6.8) and finite Ld.
            for (i, &v) in g.threshold_ld.iter().enumerate() {
                debug_assert!(
                    v >= THRESHOLD_LD_FLOOR,
                    "FPE grouped threshold_ld[{i}] floor (got {v})"
                );
                debug_assert!(
                    v.is_finite(),
                    "FPE grouped threshold_ld[{i}] finite (got {v})"
                );
            }
            for (i, &v) in g.energy_ld.iter().enumerate() {
                debug_assert!(v.is_finite(), "FPE grouped energy_ld[{i}] finite (got {v})");
            }
        }
    } else if let Some(tld) = &out.threshold_ld {
        // Long threshold-Ld floor −33/64 and finite.
        for (i, &v) in tld.iter().enumerate() {
            debug_assert!(
                v >= THRESHOLD_LD_FLOOR,
                "FPE long threshold_ld[{i}] floor (got {v})"
            );
            debug_assert!(v.is_finite(), "FPE long threshold_ld[{i}] finite (got {v})");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::constants::MAXVAL_F;
    use super::*;

    const Q31: f64 = 2_147_483_648.0;
    #[test]
    fn fraction_to_stored_rounds_to_nearest() {
        // Exact half-LSB rounds away from zero (same as round-to-nearest
        // for non-negative energies); just below stays down.
        assert_eq!(fraction_to_stored(0.0), 0);
        assert_eq!(fraction_to_stored(0.5 / Q31), 1);
        assert_eq!(fraction_to_stored(1.5 / Q31), 2);
        assert_eq!(fraction_to_stored(2.5 / Q31), 3);
        assert_eq!(fraction_to_stored(0.499_999 / Q31), 0);
        assert_eq!(fraction_to_stored(1.49 / Q31), 1);
        assert_eq!(fraction_to_stored(1.51 / Q31), 2);
        // Exact integer LSBs are preserved.
        assert_eq!(fraction_to_stored(7.0 / Q31), 7);
        assert_eq!(fraction_to_stored(123_456_789.0 / Q31), 123_456_789);
    }

    #[test]
    fn fraction_to_stored_saturates_nonnegative_energies() {
        // MAXVAL_f is stored MAXVAL_DBL (2^31−1); values at/above 1.0
        // saturate at the non-negative maximum, never wrap negative.
        assert_eq!(fraction_to_stored(MAXVAL_F), i32::MAX);
        assert_eq!(fraction_to_stored(1.0), i32::MAX);
        assert_eq!(fraction_to_stored(2.0), i32::MAX);
        assert_eq!(fraction_to_stored(1.0e9), i32::MAX);
        // Sub-LSB energies round to zero.
        assert_eq!(fraction_to_stored(0.49 / Q31), 0);
    }

    #[test]
    fn fraction_to_stored_clamps_full_i32_range() {
        // General §7 contract (not used by non-negative energies): the
        // boundary clamps both ends of the i32 range.
        assert_eq!(fraction_to_stored(-1.0 / Q31), -1);
        assert_eq!(fraction_to_stored(-2.0), i32::MIN);
        assert_eq!(fraction_to_stored(i32::MIN as f64 / Q31), i32::MIN);
    }

    #[test]
    #[should_panic(expected = "non-finite fraction")]
    fn fraction_to_stored_rejects_nan() {
        // S-4 release-mode guard: ordinary assert!, active in release too.
        let _ = fraction_to_stored(f64::NAN);
    }

    #[test]
    #[should_panic(expected = "non-finite fraction")]
    fn fraction_to_stored_rejects_infinity() {
        let _ = fraction_to_stored(f64::INFINITY);
    }
}
