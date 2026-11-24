//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! FPE output boundary and shared-output assembly.
//!
//! **M2 (FPE-2.3, pass-through):** [`assemble_channel`] constructs the shared
//! [`crate::aac_enc::psy::ChannelOutput`] /
//! [`crate::aac_enc::psy::GroupedOutput`] types by calling the Q31 output
//! assembler ([`crate::aac_enc::psy::finish_channel`]) on the Q31-produced
//! stored ints / f64 Ld, **unchanged**. There is no boundary quantization in
//! M2: the value lane delegates to Q31, so the results already satisfy the
//! output contract and pass through bit-identically (roadmap `09` M2
//! pass-through contract; risk R-19). The integer representation fields
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
//! fraction-range) activate with the float value lane; a non-finite value is
//! a loud frame-level finding via the existing skip-with-note path, never
//! silent. Introducing rounding or rescaling now would violate the M2
//! pass-through contract.
//!
//! Upstream: `psy_main.cpp:1058-1076/1089-1101/1120-1128/1251-1295`
//! (`LdDataVector`, output build). Q31 mirrors (read-only):
//! `psy.rs:1299-1551` (`group_short_channel`, `finish_channel`,
//! `grouped_ld`, `grouped_ld_clamped`).

use crate::aac_enc::psy::{self, ChannelOutput};
use crate::aac_enc::sfb::{SfbConfig, THRESHOLD_LD_FLOOR};
use crate::aac_enc::thresholds::PreEchoState;

/// Assemble one channel's output as an M2 **pass-through** of the Q31
/// results into the shared output types, then run the M2 invariant guards.
///
/// Signature mirrors [`crate::aac_enc::psy::finish_channel`] (the Q31 output
/// assembler) exactly; the arguments are forwarded unchanged. Grouping
/// (short), the fresh grouped MS-Ld (`calc_ld_data`), the grouped/long Ld
/// vectors, and the `maxSfbPerGroup` rules are all performed by the reused
/// Q31 assembler, so the assembled output is bit-identical to Q31.
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
    // M2 pass-through: no fraction conversion, no boundary rounding, no
    // rescaling. The Q31 assembler builds the shared contract unchanged.
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

/// M2 representation-lane / output-layout invariant guards (`docs/fpe/05`
/// §8, `04` §7). `debug_assert!` only — no release behaviour change. The
/// value lane is Q31-produced in M2, so these hold by construction; the
/// guards localize any future divergence introduced when a float kernel
/// lands (M3+).
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
