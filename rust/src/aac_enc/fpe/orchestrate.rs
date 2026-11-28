//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! FPE per-pair frame orchestration — explicit mirror of the Q31 order.
//!
//! **M2 (FPE-2.2) and M3 (FPE-3.1):** this module is a stage-by-stage mirror
//! of `psy::process_pair_reuse` (`psy.rs:509-1130`). It owns the FPE
//! processing sequence — the stage ordering, the conditional behaviour, the
//! element-global (joint-over-the-pair) representation decisions, the
//! channel-pair semantics, and the scratch reset discipline — and operates
//! on FPE-owned state ([`super::FpeChannel`]) and scratch
//! ([`super::FpeScratch`]).
//!
//! **M3 value-lane status (FPE-3.1 + FPE-3.2):** the rescale-stage band
//! energies and their Ld vector are native f64
//! ([`super::energy::rescale_band_energy_into`]) in the Q31-fraction domain,
//! converted to stored `i32` at the `05` §7 boundary
//! ([`super::output::fraction_to_stored`]), and the TNS energy recompute is
//! likewise floated ([`super::energy::tns_recompute_energy_into`], `05`
//! §6.2) with the mirror-then-replace discipline below. Every other value
//! calculation still calls the existing Q31 kernels (threshold
//! init/clip/spreading/PCM floor, pre-echo, MS energies), whose stored-int /
//! f64-Ld results pass through unchanged. The integer raw energy sums are
//! still computed — they feed ONLY the maxNrg/finalShift representation
//! decision (ADR-0005).
//!
//! **M2 pass-through semantics** (still authoritative for every kernel not
//! yet floated): no fraction conversion, no rounding, no rescaling is
//! applied to Q31-produced results (`docs/fpe/09` M2 pass-through contract;
//! risk R-19). M3–M7 replace one kernel group at a time, gated by the
//! Q31↔FPE differential harness.
//!
//! The value lane never feeds back into representation decisions (one-way
//! dependency, ADR-0005): `mdctScale`/`finalShift`/`maxScale`/`tnsSpecShift`
//! are produced by the reused integer-exact Q31 helpers and are observable
//! and exact.
//!
//! ## Stage correspondence (FPE ↔ Q31 `psy.rs` ↔ upstream `psy_main.cpp`)
//!
//! | FPE stage (below) | Q31 counterpart | Upstream |
//! |---|---|---|
//! | D-022 preconditions | `psy.rs:521-546` | caller contract |
//! | Per-channel config selection | `psy.rs:548-626` | `psy_main.cpp:552-629` |
//! | Lowpass + zeroSpec + initial exponents | `psy.rs:628-642` | `psy_main.cpp:584-614` |
//! | Integer lane Q31 map | `psy.rs:644-648` | (stored ints) |
//! | Rescale: headroom + joint minSpecShift | `psy.rs:650-685` | `psy_main.cpp:631-668` |
//! | Rescale: raw energies + joint maxNrg | `psy.rs:687-733` | `band_nrg.cpp:140-184` |
//! | zero_elem shortcut / rescale_shifts / re-shift + thr init / repr shift | `psy.rs:734-811` | `psy_main.cpp:669-749` |
//! | TNS detect / sync / active-OR / encode+recompute / overflow rescale | `psy.rs:813-963` | `psy_main.cpp:766-920` |
//! | Threshold advance (clip/spread/PCM/pre-echo/spread-energy) | `psy.rs:965-1006` | `psy_main.cpp:922-1019` |
//! | MS energies + stale-on-short Ld history | `psy.rs:1008-1057` | `psy_main.cpp:1021-1037` |
//! | Grouping + output assembly | `psy.rs:1059-1129` | `psy_main.cpp:1039-1130` |
//!
//! Upstream counterpart: `FDKaacEnc_psyMain` (`psy_main.cpp:407-1298`);
//! Q31 mirror: `psy.rs:509-1130` (`docs/fpe/03` §2/§3).

use super::energy as fpe_energy;
use super::output;
use super::{FpeChannel, FpeScratch, FpeTables};
use crate::aac_enc::block_switch::WindowSequence;
use crate::aac_enc::energy;
use crate::aac_enc::psy::{
    self, ChannelInput, ChannelOutput, MsOutput, MDCT_SCALE_LONG, MDCT_SCALE_SHORT,
};
use crate::aac_enc::q31::shift_int;
use crate::aac_enc::thresholds::init_threshold;
use crate::aac_enc::tns;

/// Per-pair orchestration entry (called by [`super::process_pair_reuse`]).
///
/// Explicit FPE processing sequence mirroring `psy::process_pair_reuse`
/// stage-for-stage over FPE-owned state and scratch. From M3 (FPE-3.1 +
/// FPE-3.2) the rescale band energies with their Ld vector and the TNS
/// recomputed energies (long stored + Ld, short stored only) are
/// float-derived; every other value kernel still delegates to Q31 and passes
/// its results through unchanged, so representation fields and mutated
/// spectra remain exact and the remaining value fields move only through
/// the replaced energy values.
pub(super) fn process_pair(
    left: &mut FpeChannel,
    right: Option<&mut FpeChannel>,
    lin: ChannelInput<'_>,
    rin: Option<ChannelInput<'_>>,
    tables: &FpeTables,
    ws: &mut FpeScratch,
) -> (ChannelOutput, Option<ChannelOutput>, Option<MsOutput>) {
    let mut right = right;
    let stereo = right.is_some();
    assert_eq!(stereo, rin.is_some(), "channel/state pairing");

    // --- D-022 robustness preconditions (ordinary assert!, active in release
    // builds), identical to `psy.rs:521-546`. Structural caller-contract
    // violations, not numerical noise (`docs/fpe/06` §6). ---
    // 1. LFE window invariant: every channel marked LFE must have an
    //    effective Long window sequence. Block switching forces LFE to
    //    Long, but pair synchronization can merge an LFE Long with a
    //    partner Short into Short; reject that for both left and right.
    assert!(
        !(lin.is_lfe && lin.seq == WindowSequence::Short),
        "D-022: LFE channel must not enter psy with an effective Short window \
         sequence (block-switch forced Long merged with partner Short)"
    );
    if let Some(rin) = rin.as_ref() {
        assert!(
            !(rin.is_lfe && rin.seq == WindowSequence::Short),
            "D-022: LFE channel must not enter psy with an effective Short window \
             sequence (block-switch forced Long merged with partner Short)"
        );
    }
    // 2. CPE sequence invariant: left and right effective window sequences
    //    must match before MS-energy cross-indexing and regrouping.
    if let Some(rin) = rin.as_ref() {
        assert_eq!(
            lin.seq, rin.seq,
            "D-022: CPE left and right window sequences must match before \
             MS-energy and regrouping"
        );
    }

    // --- Per-channel static selection (`psy.rs:548-626`). ---
    let short_l = lin.seq == WindowSequence::Short;
    let short_r = rin
        .as_ref()
        .map(|r| r.seq == WindowSequence::Short)
        .unwrap_or(false);
    // C uses isShortWindow[0] element-globally for nrgShift (psy_main:676);
    // CPE pairs always agree post-sync.
    let short_elem = short_l;
    let conf_l = if short_l {
        &tables.short_cfg
    } else {
        &tables.long_cfg
    };
    let conf_r = if short_r {
        &tables.short_cfg
    } else {
        &tables.long_cfg
    };
    let tconf_l = if short_l {
        &tables.tns_short
    } else {
        &tables.tns_long
    };
    let tconf_r = if short_r {
        &tables.tns_short
    } else {
        &tables.tns_long
    };
    let n_win_l = if short_l { 8 } else { 1 };
    // Mono elements have no right channel: zero windows keeps every
    // right-side loop (maxscale, energies, thresholds, MS) a no-op.
    let n_win_r = if stereo {
        if short_r {
            8
        } else {
            1
        }
    } else {
        0
    };
    let wlen_l = if short_l { 128 } else { 1024 };
    let wlen_r = if short_r { 128 } else { 1024 };
    let active_l = if lin.is_lfe {
        conf_l.sfb_active_lfe()
    } else {
        conf_l.sfb_active()
    };
    let lowpass_l = if lin.is_lfe {
        conf_l.lowpass_line_lfe()
    } else {
        conf_l.lowpass_line()
    };
    let (active_r, lowpass_r) = match &rin {
        Some(r) => (
            if r.is_lfe {
                conf_r.sfb_active_lfe()
            } else {
                conf_r.sfb_active()
            },
            if r.is_lfe {
                conf_r.lowpass_line_lfe()
            } else {
                conf_r.lowpass_line()
            },
        ),
        None => (0, 0),
    };
    // Copy Probe flags first (Copy types, no borrow retained), then split
    // the spectrum borrows.
    let (rseq, rlfe, rgroup) = match &rin {
        Some(r) => (r.seq, r.is_lfe, r.grouping),
        None => (WindowSequence::Long, false, (1, [1, 0, 0, 0])),
    };
    let spec_l: &mut [f64] = &mut lin.spectrum[..];
    let mut rin_opt = rin;
    let mut spec_r_opt: Option<&mut [f64]> = rin_opt.as_mut().map(|r| &mut r.spectrum[..]);

    // --- Lowpass + zeroSpec + initial exponents (`psy.rs:628-642`). ---
    let zero_l = psy::lowpass_and_zero_check(spec_l, n_win_l, wlen_l, lowpass_l);
    let zero_r = spec_r_opt
        .as_deref_mut()
        .map(|s| psy::lowpass_and_zero_check(s, n_win_r, wlen_r, lowpass_r));
    let mut m_l = if short_l {
        MDCT_SCALE_SHORT
    } else {
        MDCT_SCALE_LONG
    };
    let mut m_r = if short_r {
        MDCT_SCALE_SHORT
    } else {
        MDCT_SCALE_LONG
    };

    // --- Integer lane: Q31-mapped spectra (`psy.rs:644-648`). Round-to-
    // nearest, the single approximation point; all later integer ops exact.
    // Written into FPE-owned reused scratch (`clear` + `push` reproduces the
    // former fresh `Vec` exactly). ---
    energy::map_spectrum_to_q31_into(spec_l, m_l, &mut ws.q_l);

    // --- Rescale stage, joint over the element (`psy.rs:650-685`). ---
    // NOTE: maxScale/energies run unconditionally (zero spectra yield 30s
    // and zeros naturally); the all-zero shortcut below applies per C only
    // when the whole element is silent (mixed elements take the full path,
    // including the Ld offset on zero bands).
    ws.maxsc_l.resize(n_win_l, Vec::new());
    let mut min_spec_shift = i32::MAX;
    for w in 0..n_win_l {
        energy::max_scale_spec_into(
            &ws.q_l[w * wlen_l..(w + 1) * wlen_l],
            conf_l.offsets(),
            active_l,
            &mut ws.maxsc_l[w],
        );
        for &m in ws.maxsc_l[w].iter() {
            min_spec_shift = min_spec_shift.min(m);
        }
    }
    // Right channel contributes to the global minimum (when present).
    ws.q_r.clear();
    ws.maxsc_r.resize(n_win_r, Vec::new());
    if let (Some(spec_r), Some(zero_r)) = (spec_r_opt.as_deref(), zero_r) {
        let _ = zero_r;
        energy::map_spectrum_to_q31_into(spec_r, m_r, &mut ws.q_r);
        for w in 0..n_win_r {
            energy::max_scale_spec_into(
                &ws.q_r[w * wlen_r..(w + 1) * wlen_r],
                conf_r.offsets(),
                active_r,
                &mut ws.maxsc_r[w],
            );
            for &m in ws.maxsc_r[w].iter() {
                min_spec_shift = min_spec_shift.min(m);
            }
        }
    }

    // --- Energies (stored ints) + joint maxNrg (`psy.rs:687-733`). Log
    // energies derive from the stored values after the re-shift below. The
    // `e_*` are fully overwritten by the helpers; the `eld_*` placeholders
    // (-1.0) are read on the all-zero path, so both are reset to the exact
    // former `vec![...]` contents; inner lengths stay exactly `active_*`. ---
    ws.e_l.resize(n_win_l, Vec::new());
    ws.eld_l.resize(n_win_l, Vec::new());
    for w in 0..n_win_l {
        ws.e_l[w].clear();
        ws.e_l[w].resize(active_l, 0);
        ws.eld_l[w].clear();
        ws.eld_l[w].resize(active_l, -1.0);
    }
    // FPE-3.1 float rescale energies (fully overwritten per window in the
    // non-zero path; never read on the all-zero shortcut).
    ws.ef_l.resize(n_win_l, Vec::new());
    ws.e_r.resize(n_win_r, Vec::new());
    ws.eld_r.resize(n_win_r, Vec::new());
    for w in 0..n_win_r {
        ws.e_r[w].clear();
        ws.e_r[w].resize(active_r, 0);
        ws.eld_r[w].clear();
        ws.eld_r[w].resize(active_r, -1.0);
    }
    ws.ef_r.resize(n_win_r, Vec::new());
    let mut max_nrg = 0i32;
    for w in 0..n_win_l {
        energy::check_band_energy_into(
            &ws.q_l[w * wlen_l..(w + 1) * wlen_l],
            &ws.maxsc_l[w],
            conf_l.offsets(),
            active_l,
            &mut ws.e_l[w],
        );
        let m = energy::max_nrg_for_shift(&ws.e_l[w], &ws.maxsc_l[w], min_spec_shift);
        max_nrg = max_nrg.max(m);
    }
    if spec_r_opt.as_deref().is_some() {
        for w in 0..n_win_r {
            energy::check_band_energy_into(
                &ws.q_r[w * wlen_r..(w + 1) * wlen_r],
                &ws.maxsc_r[w],
                conf_r.offsets(),
                active_r,
                &mut ws.e_r[w],
            );
            let m = energy::max_nrg_for_shift(&ws.e_r[w], &ws.maxsc_r[w], min_spec_shift);
            max_nrg = max_nrg.max(m);
        }
    }
    // All-zero shortcut is element-global (psy_main:732): blanks and
    // mdctScale 0 only when every channel is silent; otherwise the full
    // path runs (zero channels get natural zeros plus the Ld offset).
    let zero_elem = zero_l && (!stereo || zero_r == Some(true));
    // `t_*` initial zeros are observable on the all-zero path (they are
    // the `thresholds` outputs there), so reset to exact length+zeros.
    ws.t_l.resize(n_win_l, Vec::new());
    for w in 0..n_win_l {
        ws.t_l[w].clear();
        ws.t_l[w].resize(active_l, 0);
    }
    ws.t_r.resize(n_win_r, Vec::new());
    for w in 0..n_win_r {
        ws.t_r[w].clear();
        ws.t_r[w].resize(active_r, 0);
    }
    let final_shift = if zero_elem {
        for w in 0..n_win_l {
            ws.maxsc_l[w].clear();
            ws.maxsc_l[w].resize(active_l, 0);
        }
        for w in 0..n_win_r {
            ws.maxsc_r[w].clear();
            ws.maxsc_r[w].resize(active_r, 0);
        }
        m_l = 0;
        m_r = 0;
        0
    } else {
        let (_, f) = energy::rescale_shifts(max_nrg, min_spec_shift, short_elem, m_l);
        // Value lane (FPE-3.1): native f64 rescale band energy in the
        // fraction domain (`docs/fpe/05` §6.1). The integer raw sums above
        // feed ONLY the joint maxNrg/finalShift decision (ADR-0005); the
        // stored energy written here is the `05` §7 boundary image of the
        // float sum, and the Ld vector follows the `05` §6.8 rescale
        // conventions. The C re-shift scaffolding (`scale0` and the
        // `2(scale0−f)` right shift) cancels in the net fraction value, so
        // the float form needs only `m′ = mdctScale − finalShift`.
        for w in 0..n_win_l {
            fpe_energy::rescale_band_energy_into(
                &spec_l[w * wlen_l..(w + 1) * wlen_l],
                conf_l.offsets(),
                active_l,
                m_l,
                f,
                &ws.maxsc_l[w],
                &mut ws.ef_l[w],
                &mut ws.eld_l[w],
            );
            for b in 0..active_l {
                ws.e_l[w][b] = output::fraction_to_stored(ws.ef_l[w][b]);
                ws.t_l[w][b] = init_threshold(ws.e_l[w][b]);
            }
        }
        if stereo {
            // Copy Probe flags / spectrum ends before the TNS block reborrows.
            let spec_r = spec_r_opt.as_deref().expect("stereo spectrum");
            for w in 0..n_win_r {
                fpe_energy::rescale_band_energy_into(
                    &spec_r[w * wlen_r..(w + 1) * wlen_r],
                    conf_r.offsets(),
                    active_r,
                    m_r,
                    f,
                    &ws.maxsc_r[w],
                    &mut ws.ef_r[w],
                    &mut ws.eld_r[w],
                );
                for b in 0..active_r {
                    ws.e_r[w][b] = output::fraction_to_stored(ws.ef_r[w][b]);
                    ws.t_r[w][b] = init_threshold(ws.e_r[w][b]);
                }
            }
        }
        // Representation shift (C shifts stored ints, tracked by
        // mdctScale; real values unchanged, so the float lane is untouched).
        if f != 0 {
            for v in ws.q_l.iter_mut() {
                *v = shift_int(*v, f);
            }
            for w in 0..n_win_l {
                for m in ws.maxsc_l[w].iter_mut() {
                    *m -= f;
                }
            }
            m_l -= f;
            if spec_r_opt.is_some() {
                for v in ws.q_r.iter_mut() {
                    *v = shift_int(*v, f);
                }
                for w in 0..n_win_r {
                    for m in ws.maxsc_r[w].iter_mut() {
                        *m -= f;
                    }
                }
                m_r -= f;
            }
        }
        f
    };

    // --- TNS block (skipped for LFE elements, mirroring isLFE[0])
    // (`psy.rs:813-963`). ---
    let tns_on = tconf_l.active || tconf_r.active;
    let mut tns_active = [false; 8];
    let mut nrg_scaling = [0i32; 2];
    let mut tns_spec_shift = 0i32;
    if !lin.is_lfe && tns_on {
        // Detect per channel/window on the Q31 stored-domain spectrum
        // (exact integer merge; ratios feed float Schur).
        for w in 0..n_win_l {
            let base = w * wlen_l;
            tns::detect(
                &mut left.tns_state,
                &mut left.tns_info,
                tconf_l,
                &ws.q_l[base..base + wlen_l],
                w,
                short_l,
                conf_l.sfb_count(),
            );
        }
        if stereo {
            let (rstate, rinfo) = right
                .as_deref_mut()
                .map(|r| (&mut r.tns_state, &mut r.tns_info))
                .expect("stereo state");
            let spec_r = spec_r_opt.as_deref().expect("stereo spectrum");
            let _ = spec_r;
            for w in 0..n_win_r {
                let base = w * wlen_r;
                tns::detect(
                    rstate,
                    rinfo,
                    tconf_r,
                    &ws.q_r[base..base + wlen_r],
                    w,
                    short_r,
                    conf_r.sfb_count(),
                );
            }
            // Sync ch1 <- ch0 with ch1's config (psy_main:783-791).
            tns::synchronize_pair(
                rstate,
                rinfo,
                &left.tns_state,
                &left.tns_info,
                tconf_r,
                short_r,
                short_l,
            );
        }
        // Active flags OR over the element (psy_main:793-816).
        let nw = if short_l { 8 } else { 1 };
        for w in 0..nw {
            let l = left.tns_state.active[w];
            let r = right
                .as_deref()
                .map(|r| r.tns_state.active[w])
                .unwrap_or([false, false]);
            // Long windows read subblock 0; short read per-window.
            tns_active[w] = l[0] || l[1] || r[0] || r[1];
        }
        // Pre-encode scaling + encode + energy recompute, per channel.
        // (Two loops mirror C: encode/energy for all channels first so the
        // element-global tnsSpecShift is final before the overflow rescale.)
        // Left channel.
        // FPE-3.2 mirror-then-replace: the Q31 call below is unchanged and
        // preserves every representation and state effect (Q remap, maxScale
        // recompute, TNS filtering of `spec_l`, pre-halving `m_l += 1`,
        // threshold quartering, `nrgScaling`/global `tnsSpecShift`). Only
        // the designated recomputed values are then replaced with the float
        // results (long: stored energies + Ld; short: stored energies only,
        // Ld stays at rescale values). `overflow_rescale` below stays
        // unchanged and shifts the replaced stored ints by the exact
        // integer power of two.
        psy::tns_encode_channel(
            &mut left.tns_state,
            &mut left.tns_info,
            tconf_l,
            conf_l,
            spec_l,
            &mut ws.q_l,
            &mut ws.maxsc_l,
            &mut ws.e_l,
            &mut ws.eld_l,
            &mut ws.t_l,
            &tns_active,
            short_l,
            active_l,
            &mut m_l,
            &mut nrg_scaling[0],
            &mut tns_spec_shift,
            &mut ws.qwin,
            &mut ws.tns_pre,
        );
        for w in 0..n_win_l {
            if tns_active[w] {
                fpe_energy::tns_recompute_energy_into(
                    &spec_l[w * wlen_l..(w + 1) * wlen_l],
                    conf_l.offsets(),
                    active_l,
                    m_l,
                    &ws.maxsc_l[w],
                    nrg_scaling[0],
                    short_l,
                    &mut ws.e_l[w],
                    &mut ws.eld_l[w],
                );
            }
        }
        // Right channel.
        if stereo {
            let (rstate, rinfo) = right
                .as_deref_mut()
                .map(|r| (&mut r.tns_state, &mut r.tns_info))
                .expect("stereo state");
            let spec_r = spec_r_opt.as_deref_mut().expect("stereo spectrum");
            psy::tns_encode_channel(
                rstate,
                rinfo,
                tconf_r,
                conf_r,
                spec_r,
                &mut ws.q_r,
                &mut ws.maxsc_r,
                &mut ws.e_r,
                &mut ws.eld_r,
                &mut ws.t_r,
                &tns_active,
                short_r,
                active_r,
                &mut m_r,
                &mut nrg_scaling[1],
                &mut tns_spec_shift,
                &mut ws.qwin,
                &mut ws.tns_pre,
            );
            let spec_r = spec_r_opt.as_deref().expect("stereo spectrum");
            for w in 0..n_win_r {
                if tns_active[w] {
                    fpe_energy::tns_recompute_energy_into(
                        &spec_r[w * wlen_r..(w + 1) * wlen_r],
                        conf_r.offsets(),
                        active_r,
                        m_r,
                        &ws.maxsc_r[w],
                        nrg_scaling[1],
                        short_r,
                        &mut ws.e_r[w],
                        &mut ws.eld_r[w],
                    );
                }
            }
        }
    } else {
        // TNS globally inactive: clear runtime decisions (psy_main:915-919).
        // tnsInfo keeps detect-written fields (none ran); info stays as-is.
        left.tns_state.clear_runtime();
        if let Some(r) = right.as_deref_mut() {
            r.tns_state.clear_runtime();
        }
    }
    // Overflow rescale AFTER all channels (element-global tnsSpecShift).
    if tns_spec_shift != 0 {
        psy::overflow_rescale(
            spec_l,
            &mut ws.q_l,
            &mut ws.e_l,
            &mut ws.eld_l,
            &mut ws.t_l,
            &mut m_l,
            lowpass_l,
            active_l,
            tns_spec_shift,
            nrg_scaling[0],
        );
        if stereo {
            let spec_r = spec_r_opt.as_deref_mut().expect("stereo spectrum");
            psy::overflow_rescale(
                spec_r,
                &mut ws.q_r,
                &mut ws.e_r,
                &mut ws.eld_r,
                &mut ws.t_r,
                &mut m_r,
                lowpass_r,
                active_r,
                tns_spec_shift,
                nrg_scaling[1],
            );
        }
    }

    // --- Threshold advance per channel (`psy.rs:965-1006`,
    // psy_main:922-1019 minus MS). `spread_*` placeholders are fully
    // overwritten per band by `advance_thresholds`, reset here to the exact
    // former zeros and exact length. ---
    ws.spread_l.resize(n_win_l, Vec::new());
    for w in 0..n_win_l {
        ws.spread_l[w].clear();
        ws.spread_l[w].resize(active_l, 0);
    }
    psy::advance_thresholds(
        &mut left.preecho,
        &mut ws.t_l,
        &mut ws.spread_l,
        &ws.e_l,
        conf_l,
        active_l,
        m_l,
        short_l,
        lin.seq,
        lin.is_lfe,
    );
    ws.spread_r.clear();
    if stereo {
        ws.spread_r.resize(n_win_r, Vec::new());
        for w in 0..n_win_r {
            ws.spread_r[w].clear();
            ws.spread_r[w].resize(active_r, 0);
        }
        let rstate = right.as_deref_mut().expect("stereo state");
        psy::advance_thresholds(
            &mut rstate.preecho,
            &mut ws.t_r,
            &mut ws.spread_r,
            &ws.e_r,
            conf_r,
            active_r,
            m_r,
            short_r,
            rseq,
            rlfe,
        );
    }

    // --- MS energies (stereo element-level, `psy.rs:1008-1057`,
    // psy_main:1021-1037). ---
    let ms_out = if stereo {
        // Re-map Q with current exponents (post everything). Reused scratch
        // (full overwrite by the mapper).
        energy::map_spectrum_to_q31_into(spec_l, m_l, &mut ws.ql_full);
        energy::map_spectrum_to_q31_into(
            spec_r_opt.as_deref().expect("stereo spectrum"),
            m_r,
            &mut ws.qr_full,
        );
        let calc_ld = rseq != WindowSequence::Short;
        let mut mid = Vec::with_capacity(n_win_r);
        let mut side = Vec::with_capacity(n_win_r);
        for w in 0..n_win_r {
            let (mi, si, mil, sil) = energy::calc_ms_energies(
                &ws.ql_full[w * wlen_r..(w + 1) * wlen_r],
                &ws.qr_full[w * wlen_r..(w + 1) * wlen_r],
                &ws.maxsc_l[w],
                &ws.maxsc_r[w],
                conf_r.offsets(),
                active_l,
                calc_ld,
            );
            mid.push(mi);
            side.push(si);
            // C leaves msLd STALE unless calcLd: store on write, read the
            // persistent arrays for output (both short-stale and init-zero
            // mirror C exactly). FPE-owned history.
            if calc_ld {
                left.ms_ld[w][..active_l].copy_from_slice(&mil);
                let rstate = right.as_deref_mut().expect("stereo state");
                rstate.ms_ld[w][..active_l].copy_from_slice(&sil);
            }
        }
        let rstate = right.as_deref().expect("stereo state");
        let mut mid_ld = Vec::with_capacity(n_win_r);
        let mut side_ld = Vec::with_capacity(n_win_r);
        for w in 0..n_win_r {
            mid_ld.push(left.ms_ld[w][..active_l].to_vec());
            side_ld.push(rstate.ms_ld[w][..active_l].to_vec());
        }
        Some(MsOutput {
            mid,
            side,
            mid_ld,
            side_ld,
        })
    } else {
        None
    };

    // --- Grouping + outputs per channel (`psy.rs:1059-1129`). ---
    // MS vectors per channel for short-block regrouping (mid ch0/side ch1).
    // Mono MS buffers stay zero (never written in C either).
    ws.ms_zeros_l.resize(n_win_l, Vec::new());
    for w in 0..n_win_l {
        ws.ms_zeros_l[w].clear();
        ws.ms_zeros_l[w].resize(active_l, 0);
    }
    ws.ms_zeros_r.resize(n_win_r, Vec::new());
    for w in 0..n_win_r {
        ws.ms_zeros_r[w].clear();
        ws.ms_zeros_r[w].resize(active_r, 0);
    }
    let (ms_l, ms_r): (&[Vec<i32>], &[Vec<i32>]) = match &ms_out {
        Some(ms) => (&ms.mid, &ms.side),
        None => (&ws.ms_zeros_l, &ws.ms_zeros_r),
    };
    let mut left_out = output::assemble_channel(
        &left.preecho,
        &ws.t_l,
        &ws.e_l,
        &ws.eld_l,
        &ws.spread_l,
        &ws.maxsc_l,
        spec_l,
        conf_l,
        short_l,
        active_l,
        m_l,
        final_shift,
        tns_spec_shift,
        lin.grouping,
        ms_l,
        if short_l {
            &tables.min_snr_short
        } else {
            &tables.min_snr_long
        },
    );
    let mut right_out = if stereo {
        let rstate = right.as_deref_mut().expect("stereo state");
        Some(output::assemble_channel(
            &rstate.preecho,
            &ws.t_r,
            &ws.e_r,
            &ws.eld_r,
            &ws.spread_r,
            &ws.maxsc_r,
            spec_r_opt.as_deref().expect("stereo spectrum"),
            conf_r,
            short_r,
            active_r,
            m_r,
            final_shift,
            tns_spec_shift,
            rgroup,
            ms_r,
            if short_r {
                &tables.min_snr_short
            } else {
                &tables.min_snr_long
            },
        ))
    } else {
        None
    };
    left_out.nrg_scaling = nrg_scaling[0];
    if let Some(ref mut r) = right_out {
        r.nrg_scaling = nrg_scaling[1];
    }
    (left_out, right_out, ms_out)
}
