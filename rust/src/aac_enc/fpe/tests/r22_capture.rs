//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-24.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! R2.2 Groups A and B (test-only).
//!
//! Group A provides the test-only capture/replay infrastructure and the two
//! distinct correctness layers required by design §10.2:
//!
//! * **Within-arm replay correctness (hard gate):** for each arm
//!   independently (Q31, FPE), replay the captured production decision
//!   function (`pe_calculation`, `adapt_thresholds_vbr`,
//!   `estimate_scale_factors_channel_reuse`, `qc_main_frame`) from the
//!   owned snapshot of that arm's operands and mutable state. The replay
//!   must reproduce the arm's production return value and all compared
//!   post-call state exactly. This validates capture/replay fidelity.
//! * **Cross-arm differential capture (R2.2 subject):** Q31-vs-FPE
//!   production outcomes are compared and every difference is *recorded*
//!   (`R22Diff`). Cross-arm bit identity is NOT asserted here: a
//!   difference is neither hidden nor automatically accepted. Full
//!   R/A/S/P/F classification and margin derivation belong to Group D and
//!   are intentionally absent from this module.
//!
//! Side-effect canaries prove the capture/replay instrumentation does not
//! alter the production path or the emitted output, and production source
//! stays read-only: every `R22*` item is `#[cfg(test)]`.
//!
//! Group B verifies the four committed WAV fixtures (existence, SHA-256,
//! format, rate, channels, samples, frame count) and pins their canonical
//! identities. No audio bytes are copied, regenerated, or modified.

use crate::aac_enc::adts::ADTS_HEADER_BITS;
use crate::aac_enc::config::{EncoderConfig, PsyEngineKind, TransportType, VbrMode, FRAME_LENGTH};
use crate::aac_enc::pe::{pe_calculation, AtsElement, PeChannelInput};
use crate::aac_enc::rate::{
    qc_main_frame, BitResMode, DistributeState, QcChannelInput, QcFrameInput, QcState, MAXVAL_DBL,
    MAX_ITERATIONS,
};
use crate::aac_enc::scf::{
    estimate_scale_factors_channel_reuse, ScfChannelInput, ScfGeometry, ScfWorkspace,
};
use crate::aac_enc::static_bits::MsDigest;
use crate::aac_enc::stream_driver::{
    FrameSnapshot, R22Capture, R22S1Cap, R22S2Cap, R22S4Cap, R22S4Channel, R22S6Cap, StreamEncoder,
};
use crate::aac_enc::testdata;
use crate::aac_enc::threshold_vbr::{adapt_thresholds_vbr, S2ChannelInput, SfbGeometry};
use crate::aac_enc::tns::TnsChannel;

// ---------------------------------------------------------------------------
// Comparators
// ---------------------------------------------------------------------------

/// Field-by-field `AtsElement` equivalence (no `PartialEq` derive exists;
/// compare every field explicitly rather than weakening the check).
fn assert_ats_eq(a: &AtsElement, b: &AtsElement, what: &str) {
    assert_eq!(a.pe_offset, b.pe_offset, "{what} pe_offset");
    assert_eq!(
        a.chaos_measure_en_fac, b.chaos_measure_en_fac,
        "{what} chaos_measure_en_fac"
    );
    assert_eq!(
        a.last_enfac_patch, b.last_enfac_patch,
        "{what} last_enfac_patch"
    );
    assert_eq!(
        a.chaos_measure_old, b.chaos_measure_old,
        "{what} chaos_measure_old"
    );
    assert_eq!(
        a.vbr_qual_factor, b.vbr_qual_factor,
        "{what} vbr_qual_factor"
    );
    assert_eq!(
        a.ah_modify_min_snr, b.ah_modify_min_snr,
        "{what} ah_modify_min_snr"
    );
    assert_eq!(a.ah_start_sfb_l, b.ah_start_sfb_l, "{what} ah_start_sfb_l");
    assert_eq!(a.ah_start_sfb_s, b.ah_start_sfb_s, "{what} ah_start_sfb_s");
    assert_eq!(a.msa_max_red, b.msa_max_red, "{what} msa_max_red");
    assert_eq!(
        a.msa_start_ratio, b.msa_start_ratio,
        "{what} msa_start_ratio"
    );
    assert_eq!(
        a.msa_red_ratio_fac, b.msa_red_ratio_fac,
        "{what} msa_red_ratio_fac"
    );
    assert_eq!(a.msa_red_offs, b.msa_red_offs, "{what} msa_red_offs");
}

fn ats_equal(a: &AtsElement, b: &AtsElement) -> bool {
    a.pe_offset == b.pe_offset
        && a.chaos_measure_en_fac == b.chaos_measure_en_fac
        && a.last_enfac_patch == b.last_enfac_patch
        && a.chaos_measure_old == b.chaos_measure_old
        && a.vbr_qual_factor == b.vbr_qual_factor
        && a.ah_modify_min_snr == b.ah_modify_min_snr
        && a.ah_start_sfb_l == b.ah_start_sfb_l
        && a.ah_start_sfb_s == b.ah_start_sfb_s
        && a.msa_max_red == b.msa_max_red
        && a.msa_start_ratio == b.msa_start_ratio
        && a.msa_red_ratio_fac == b.msa_red_ratio_fac
        && a.msa_red_offs == b.msa_red_offs
}

/// Field-by-field `TnsChannel` equivalence (`TnsChannel` has no
/// `PartialEq`; compare every leaf explicitly).
fn tns_equal(a: &TnsChannel, b: &TnsChannel) -> bool {
    a.num_filters == b.num_filters
        && a.coef_res == b.coef_res
        && a.length == b.length
        && a.order == b.order
        && a.direction == b.direction
        && a.coef == b.coef
        && a.active == b.active
        && a.gain == b.gain
        && a.merged == b.merged
}

/// Observable frame-surviving carrier state (canary comparison).
fn assert_observable_eq(
    a: &(AtsElement, DistributeState, i32, i32, i32),
    b: &(AtsElement, DistributeState, i32, i32, i32),
    what: &str,
) {
    assert_ats_eq(&a.0, &b.0, &format!("{what} ats"));
    assert_eq!(a.1, b.1, "{what} dist");
    assert_eq!(a.2, b.2, "{what} bit_res_tot");
    assert_eq!(a.3, b.3, "{what} bit_res_tot_max");
    assert_eq!(a.4, b.4, "{what} padding_rest");
}

// ---------------------------------------------------------------------------
// Within-arm replay (production functions only, on owned clones)
// ---------------------------------------------------------------------------

/// Replay S1 from the owned snapshot and assert bit-exact reproduction of
/// the production return value and post-call ATS state.
fn replay_s1(cap: &R22S1Cap, tag: &str) {
    let inputs: Vec<PeChannelInput> = cap
        .ch
        .iter()
        .map(|c| PeChannelInput {
            energy_ld: &c.energy_ld,
            threshold_ld: &c.threshold_ld,
            energy_linear: &c.energy_lin,
            form_factor_ld: &c.ff,
            offsets: &c.offsets,
            sfb_cnt: c.cnt,
            sfb_per_group: c.per,
            max_sfb_per_group: c.max,
            is_book: &c.is_book,
            is_scale: &c.is_scale,
            short_block: c.short,
        })
        .collect();
    let mut ats = cap.ats_pre.clone();
    let out = pe_calculation(&inputs, &cap.ms_mask, &mut ats);
    assert_eq!(out.data.pe, cap.pe, "{tag} s1 pe");
    assert_eq!(out.data.const_part, cap.const_part, "{tag} s1 const_part");
    assert_eq!(out.data.n_active_lines, cap.nact, "{tag} s1 nact");
    assert_eq!(out.data.offset, cap.offset, "{tag} s1 offset");
    assert_eq!(
        out.data.channels.len(),
        cap.pe_ch.len(),
        "{tag} s1 channel count"
    );
    for (i, (got, want)) in out.data.channels.iter().zip(cap.pe_ch.iter()).enumerate() {
        assert_eq!(got.sfb_n_lines, want.sfb_n_lines, "{tag} s1 ch{i} n_lines");
        assert_eq!(got.sfb_pe, want.sfb_pe, "{tag} s1 ch{i} sfb_pe");
        assert_eq!(
            got.sfb_const_part, want.sfb_const_part,
            "{tag} s1 ch{i} const"
        );
        assert_eq!(
            got.sfb_n_active_lines, want.sfb_n_active_lines,
            "{tag} s1 ch{i} nact"
        );
        assert_eq!(got.pe, want.pe, "{tag} s1 ch{i} pe");
        assert_eq!(got.const_part, want.const_part, "{tag} s1 ch{i} cpart");
        assert_eq!(
            got.n_active_lines, want.n_active_lines,
            "{tag} s1 ch{i} nlines"
        );
    }
    assert_eq!(out.weighted_energy_ld, cap.weighted, "{tag} s1 weighted");
    assert_eq!(out.threshold_ld, cap.thresholds, "{tag} s1 thresholds");
    assert_eq!(out.enfac_ld, cap.enfac, "{tag} s1 enfac");
    assert_ats_eq(&ats, &cap.ats_post, &format!("{tag} s1 ats_post"));
}

/// Replay S2 from the owned snapshot and assert bit-exact reproduction.
fn replay_s2(cap: &R22S2Cap, tag: &str) {
    let inputs: Vec<S2ChannelInput> = cap
        .ch
        .iter()
        .map(|c| S2ChannelInput {
            energy_lin: &c.energy_lin,
            energy_ld: &c.energy_ld,
            weighted_energy_ld: &c.weighted,
            threshold_ld: &c.threshold,
            form_factor_ld: &c.ff,
            enfac_ld: &c.enfac,
            min_snr_ld: &c.min_snr0,
            spread: &c.spread0,
            geo: SfbGeometry {
                offsets: &c.offsets,
                group_len: &c.group_len,
                sfb_cnt: c.cnt,
                sfb_per_group: c.per,
                max_sfb_per_group: c.max,
                seq: c.seq,
            },
        })
        .collect();
    // Own the mask so the borrow outlives the call in both arms.
    let ms_owned: Option<Vec<i32>> = cap.ms_mask.clone();
    let ms_ref: Option<&[i32]> = ms_owned.as_deref();
    let mut ats = cap.ats_pre.clone();
    let out = adapt_thresholds_vbr(&inputs, ms_ref, &mut ats);
    assert_eq!(out.threshold_ld, cap.threshold_ld, "{tag} s2 thrv");
    assert_eq!(
        out.threshold_pre_enfac, cap.threshold_pre_enfac,
        "{tag} s2 pre_enfac"
    );
    assert_eq!(out.thr_exp, cap.thr_exp, "{tag} s2 thr_exp");
    assert_eq!(out.min_snr_ld, cap.min_snr_ld, "{tag} s2 minsnr1");
    assert_eq!(out.spread, cap.spread, "{tag} s2 spread");
    assert_eq!(out.ah_flag, cap.ah_flag, "{tag} s2 ah_flag");
    assert_eq!(out.reduce.red_val, cap.red_val, "{tag} s2 red_val");
    assert_eq!(
        out.reduce.chaos_measure, cap.chaos_measure,
        "{tag} s2 chaos"
    );
    assert_ats_eq(&ats, &cap.ats_post, &format!("{tag} s2 ats_post"));
}

/// Replay one S4 channel from the owned snapshot into the caller-provided
/// workspace and assert bit-exact reproduction.
fn replay_s4_channel(c: &R22S4Channel, ws: &mut ScfWorkspace, tag: &str) {
    let mut spec = c.spectrum_pre.clone();
    let mut input = ScfChannelInput {
        spectrum: &mut spec,
        energy_ld: &c.energy_ld,
        threshold_ld: &c.threshold_ld,
        form_factor_ld: &c.ff,
        geo: ScfGeometry {
            offsets: &c.offsets,
            sfb_cnt: c.cnt,
            sfb_per_group: c.per,
            max_sfb_per_group: c.max,
        },
    };
    let out = estimate_scale_factors_channel_reuse(&mut input, ws);
    assert_eq!(out.scf, c.scf, "{tag} s4 scf");
    assert_eq!(out.global_gain, c.gain, "{tag} s4 gain");
    assert_eq!(out.quant_spec, c.quant_spec, "{tag} s4 quant");
    assert_eq!(out.mutated_spectrum, c.mutated, "{tag} s4 mutated");
}

/// Replay S4 with one fresh workspace shared across the frame's channels
/// (mirrors the production encoder-owned workspace reuse).
fn replay_s4(cap: &R22S4Cap, tag: &str) {
    let mut ws = ScfWorkspace::default();
    for (i, c) in cap.ch.iter().enumerate() {
        replay_s4_channel(c, &mut ws, &format!("{tag} s4 ch{i}"));
    }
}

/// Replay S6 from the owned snapshot with a fresh bit-count scratch and
/// assert bit-exact reproduction of the frame output and post-call state.
///
/// The fresh scratch is the point: production enters with stale scratch
/// from prior frames, so equality proves the M5.22 scratch-invariant (no
/// prior-frame scratch leaks into the output).
fn replay_s6(cap: &R22S6Cap, tag: &str) {
    // Transport-owned constants cross-check (not invented per frame).
    assert_eq!(cap.glob_hdr, ADTS_HEADER_BITS as i32, "{tag} s6 glob_hdr");
    assert_eq!(cap.relative, MAXVAL_DBL, "{tag} s6 relative");
    assert_eq!(cap.bit_res_mode, BitResMode::Full, "{tag} s6 bit_res_mode");
    let mut qc = QcState::new(cap.qc_max);
    qc.bit_res_tot = cap.qc_pre;
    let mut dist: DistributeState = cap.dist_pre;
    let channels: Vec<QcChannelInput> = cap
        .ch
        .iter()
        .map(|c| QcChannelInput {
            spectrum: &c.spectrum,
            scf: &c.scf,
            global_gain: c.gain,
            sfb_cnt: c.cnt,
            max_sfb_per_group: c.max,
            sfb_per_group: c.per,
            sfb_offsets: &c.offsets,
            block_type: c.block_type,
            noise_nrg: &cap.noise,
            is_book: &c.is_book,
            is_scale: &c.is_scale,
            tns: &c.tns,
            seed_quant: c.seed_quant.as_deref(),
        })
        .collect();
    let input = QcFrameInput {
        channels,
        no_red_pe: cap.no_red_pe,
        static_bits: cap.static_bits,
        ms_digest: MsDigest::from_i32(cap.ms_digest).expect("r22 ms digest range"),
        glob_hdr_bits: cap.glob_hdr,
        max_bits_per_frame: cap.max_bits_frame,
        max_bits_el: cap.max_bits_el,
        relative_bits_el: cap.relative,
        max_bit_fac: cap.max_bit_fac,
        bit_res_mode: cap.bit_res_mode,
    };
    let out =
        qc_main_frame(&mut qc, &mut dist, &input, MAX_ITERATIONS).expect("r22 s6 replay converges");
    let want = &cap.output;
    assert_eq!(
        out.granted_dyn_bits, want.granted_dyn_bits,
        "{tag} s6 granted_dyn"
    );
    assert_eq!(out.max_dyn_bits, want.max_dyn_bits, "{tag} s6 max_dyn");
    assert_eq!(out.total_avail, want.total_avail, "{tag} s6 total_avail");
    assert_eq!(out.el_granted, want.el_granted, "{tag} s6 el_granted");
    assert_eq!(out.granted_pe, want.granted_pe, "{tag} s6 granted_pe");
    assert_eq!(
        out.granted_pe_corr, want.granted_pe_corr,
        "{tag} s6 granted_pe_corr"
    );
    assert_eq!(
        out.bit_res_level_el, want.bit_res_level_el,
        "{tag} s6 bit_res_level"
    );
    assert_eq!(
        out.bit_res_max_el, want.bit_res_max_el,
        "{tag} s6 bit_res_max"
    );
    assert_eq!(out.passes, want.passes, "{tag} s6 passes");
    assert_eq!(out.final_sections, want.final_sections, "{tag} s6 sections");
    assert_eq!(out.final_gains, want.final_gains, "{tag} s6 gains");
    assert_eq!(out.final_max_sfb, want.final_max_sfb, "{tag} s6 max_sfb");
    assert_eq!(out.final_quants, want.final_quants, "{tag} s6 quants");
    assert_eq!(out.final_qmaxs, want.final_qmaxs, "{tag} s6 qmaxs");
    assert_eq!(
        out.static_bits_used, want.static_bits_used,
        "{tag} s6 static_used"
    );
    assert_eq!(out.used_dyn_bits, want.used_dyn_bits, "{tag} s6 used_dyn");
    assert_eq!(out.tot_fill_bits, want.tot_fill_bits, "{tag} s6 tot_fill");
    assert_eq!(out.align_bits, want.align_bits, "{tag} s6 align");
    assert_eq!(out.total_bits, want.total_bits, "{tag} s6 total");
    assert_eq!(out.bit_res_tot, want.bit_res_tot, "{tag} s6 bit_res_tot");
    assert_eq!(dist, cap.dist_post, "{tag} s6 dist_post");
    assert_eq!(qc.bit_res_tot, cap.qc_post, "{tag} s6 qc_post");
}

/// Replay every stage of one frame's capture (within-arm hard gate).
/// `pub(super)` for Group C reuse (`decision.rs` OP matrix): the same
/// reviewed function, not a second replay implementation.
pub(super) fn replay_frame(cap: &R22Capture, tag: &str) {
    replay_s1(&cap.s1, tag);
    replay_s2(&cap.s2, tag);
    replay_s4(&cap.s4, tag);
    replay_s6(&cap.s6, tag);
}

/// Accounting/state internal consistency: the `R22AcctCap`/`R22FrameState`
/// mirror the live `FrameSnapshot` and the stage snapshots (no second
/// oracle; pure cross-reads of the same frame). `pub(super)` for Group C
/// reuse (`decision.rs` OP matrix).
pub(super) fn check_acct_state(cap: &R22Capture, snap: &FrameSnapshot, tag: &str) {
    let a = &cap.acct;
    assert_eq!(a.side, snap.side_bits, "{tag} acct side");
    assert_eq!(a.huff, snap.huff_bits, "{tag} acct huff");
    assert_eq!(a.scf_bits, snap.scf_bits, "{tag} acct scf");
    assert_eq!(a.used_dyn, snap.used_dyn, "{tag} acct used_dyn");
    assert_eq!(a.tot_fill, snap.tot_fill, "{tag} acct tot_fill");
    assert_eq!(a.align, snap.align, "{tag} acct align");
    assert_eq!(a.total, snap.total, "{tag} acct total");
    assert_eq!(a.npass, snap.npass, "{tag} acct npass");
    assert_eq!(a.crash, snap.crash, "{tag} acct crash");
    assert_eq!(a.gains, snap.gains, "{tag} acct gains");
    assert_eq!(
        a.last_iterations, snap.last_iterations,
        "{tag} acct last_iterations"
    );
    assert_eq!(a.sum_bits, snap.sum_bits, "{tag} acct sum_bits");
    assert_eq!(a.el_granted, snap.el_granted, "{tag} acct el_granted");
    assert_eq!(a.max_dyn, snap.max_dyn, "{tag} acct max_dyn");
    assert_eq!(a.total_avail, snap.total_avail, "{tag} acct total_avail");
    assert_eq!(a.static_used, snap.static_used, "{tag} acct static_used");
    assert_eq!(a.bit_res_tot, cap.s6.qc_post, "{tag} acct/s6 bit_res");
    assert_eq!(a.bit_res_max, cap.s6.qc_max, "{tag} acct/s6 bit_res_max");
    assert_eq!(a.pad_pre, cap.state.pad_pre, "{tag} acct/state pad_pre");
    assert_eq!(a.pad_post, cap.state.pad_post, "{tag} acct/state pad_post");
    assert_ats_eq(
        &cap.state.ats_pre,
        &cap.s1.ats_pre,
        &format!("{tag} state/s1 ats_pre"),
    );
    assert_ats_eq(
        &cap.state.ats_post,
        &cap.s2.ats_post,
        &format!("{tag} state/s2 ats_post"),
    );
    assert_eq!(
        cap.state.dist_pre, cap.s6.dist_pre,
        "{tag} state/s6 dist_pre"
    );
    assert_eq!(
        cap.state.dist_post, cap.s6.dist_post,
        "{tag} state/s6 dist_post"
    );
    assert_eq!(cap.state.qc_pre, cap.s6.qc_pre, "{tag} state/s6 qc_pre");
    assert_eq!(cap.state.qc_post, cap.s6.qc_post, "{tag} state/s6 qc_post");
}

// ---------------------------------------------------------------------------
// Cross-arm differential capture (recorded, never blanket-asserted)
// ---------------------------------------------------------------------------

/// Per-stage cross-arm difference record. `differing_fields` carries the
/// exact leaf paths (e.g. `s1.ch0.energy_ld`); an empty list means the two
/// captures agree on every compared leaf. Group D classifies these under
/// R/A/S/P/F; this module only records them.
///
/// Comparator contract (F-1 remediation): every leaf present in the
/// `R22*` capture structures participates in at least one `push_diff`
/// below — S1 `pe_ch` per-channel detail, S6 `output.bit_res_level_el` /
/// `output.bit_res_max_el`, S6 per-channel `offsets` / `tns`, S6 `qc_max`,
/// and every `R22AcctCap` accounting leaf. Bundled leaves (e.g.
/// `s1.sums`, `s6.grants`) still compare each named field with `&&`; a
/// bundle differs if any member differs. The sole intentional exclusion is
/// the S6 bit-count scratch (`bc_scratch`): it is deliberately *not*
/// snapshotted (see `R22S6Cap` docs, M5.22 scratch-invariant) and therefore
/// has no captured leaf to compare; the replay proves the invariant with a
/// fresh scratch instead.
#[derive(Debug, Default)]
pub(super) struct R22Diff {
    pub(super) s1_equal: bool,
    pub(super) s2_equal: bool,
    pub(super) s4_equal: bool,
    pub(super) s6_equal: bool,
    pub(super) state_equal: bool,
    pub(super) acct_equal: bool,
    pub(super) differing_fields: Vec<String>,
}

/// Stage selector for [`push_diff`] (avoids double mutable borrows).
#[derive(Debug, Clone, Copy)]
enum DiffStage {
    S1,
    S2,
    S4,
    S6,
    State,
    Acct,
}

fn push_diff(d: &mut R22Diff, stage: DiffStage, path: &str, equal: bool) {
    if !equal {
        match stage {
            DiffStage::S1 => d.s1_equal = false,
            DiffStage::S2 => d.s2_equal = false,
            DiffStage::S4 => d.s4_equal = false,
            DiffStage::S6 => d.s6_equal = false,
            DiffStage::State => d.state_equal = false,
            DiffStage::Acct => d.acct_equal = false,
        }
        d.differing_fields.push(path.to_string());
    }
}

/// Compare two captures leaf-by-leaf without asserting. Returns the
/// difference record; callers must not treat a non-empty record as a
/// failure (that decision belongs to Group D / owner disposition).
/// `pub(super)` for Group C reuse (`decision.rs` records the per-OP
/// cross-arm differential; classification stays in Group D).
pub(super) fn diff_captures(q: &R22Capture, f: &R22Capture, prefix: &str) -> R22Diff {
    let mut d = R22Diff {
        s1_equal: true,
        s2_equal: true,
        s4_equal: true,
        s6_equal: true,
        state_equal: true,
        acct_equal: true,
        differing_fields: Vec::new(),
    };
    // S1.
    {
        let (a, b) = (&q.s1, &f.s1);
        push_diff(
            &mut d,
            DiffStage::S1,
            &format!("{prefix}.s1.ms_mask"),
            a.ms_mask == b.ms_mask,
        );
        push_diff(
            &mut d,
            DiffStage::S1,
            &format!("{prefix}.s1.ats_pre"),
            ats_equal(&a.ats_pre, &b.ats_pre),
        );
        push_diff(
            &mut d,
            DiffStage::S1,
            &format!("{prefix}.s1.ats_post"),
            ats_equal(&a.ats_post, &b.ats_post),
        );
        push_diff(
            &mut d,
            DiffStage::S1,
            &format!("{prefix}.s1.pe"),
            a.pe == b.pe,
        );
        push_diff(
            &mut d,
            DiffStage::S1,
            &format!("{prefix}.s1.sums"),
            a.const_part == b.const_part && a.nact == b.nact && a.offset == b.offset,
        );
        push_diff(
            &mut d,
            DiffStage::S1,
            &format!("{prefix}.s1.weighted"),
            a.weighted == b.weighted,
        );
        push_diff(
            &mut d,
            DiffStage::S1,
            &format!("{prefix}.s1.thresholds"),
            a.thresholds == b.thresholds,
        );
        push_diff(
            &mut d,
            DiffStage::S1,
            &format!("{prefix}.s1.enfac"),
            a.enfac == b.enfac,
        );
        push_diff(
            &mut d,
            DiffStage::S1,
            &format!("{prefix}.s1.ch_count"),
            a.ch.len() == b.ch.len(),
        );
        push_diff(
            &mut d,
            DiffStage::S1,
            &format!("{prefix}.s1.pe_ch_count"),
            a.pe_ch.len() == b.pe_ch.len(),
        );
        for (i, (ca, cb)) in a.pe_ch.iter().zip(b.pe_ch.iter()).enumerate() {
            for (leaf, eq) in [
                ("sfb_n_lines", ca.sfb_n_lines == cb.sfb_n_lines),
                ("sfb_pe", ca.sfb_pe == cb.sfb_pe),
                ("sfb_const_part", ca.sfb_const_part == cb.sfb_const_part),
                (
                    "sfb_n_active_lines",
                    ca.sfb_n_active_lines == cb.sfb_n_active_lines,
                ),
                ("pe", ca.pe == cb.pe),
                ("const_part", ca.const_part == cb.const_part),
                ("n_active_lines", ca.n_active_lines == cb.n_active_lines),
            ] {
                push_diff(
                    &mut d,
                    DiffStage::S1,
                    &format!("{prefix}.s1.pe_ch{i}.{leaf}"),
                    eq,
                );
            }
        }
        for (i, (ca, cb)) in a.ch.iter().zip(b.ch.iter()).enumerate() {
            for (leaf, eq) in [
                ("energy_ld", ca.energy_ld == cb.energy_ld),
                ("threshold_ld", ca.threshold_ld == cb.threshold_ld),
                ("energy_lin", ca.energy_lin == cb.energy_lin),
                ("ff", ca.ff == cb.ff),
                ("offsets", ca.offsets == cb.offsets),
                ("is_book", ca.is_book == cb.is_book),
                ("is_scale", ca.is_scale == cb.is_scale),
                (
                    "geo",
                    ca.cnt == cb.cnt
                        && ca.per == cb.per
                        && ca.max == cb.max
                        && ca.short == cb.short,
                ),
            ] {
                push_diff(
                    &mut d,
                    DiffStage::S1,
                    &format!("{prefix}.s1.ch{i}.{leaf}"),
                    eq,
                );
            }
        }
    }
    // S2.
    {
        let (a, b) = (&q.s2, &f.s2);
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.ms_mask"),
            a.ms_mask == b.ms_mask,
        );
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.ats_pre"),
            ats_equal(&a.ats_pre, &b.ats_pre),
        );
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.ats_post"),
            ats_equal(&a.ats_post, &b.ats_post),
        );
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.threshold_ld"),
            a.threshold_ld == b.threshold_ld,
        );
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.pre_enfac"),
            a.threshold_pre_enfac == b.threshold_pre_enfac,
        );
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.thr_exp"),
            a.thr_exp == b.thr_exp,
        );
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.min_snr_ld"),
            a.min_snr_ld == b.min_snr_ld,
        );
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.spread"),
            a.spread == b.spread,
        );
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.ah_flag"),
            a.ah_flag == b.ah_flag,
        );
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.reduce"),
            a.red_val == b.red_val && a.chaos_measure == b.chaos_measure,
        );
        push_diff(
            &mut d,
            DiffStage::S2,
            &format!("{prefix}.s2.ch_count"),
            a.ch.len() == b.ch.len(),
        );
        for (i, (ca, cb)) in a.ch.iter().zip(b.ch.iter()).enumerate() {
            for (leaf, eq) in [
                ("energy_lin", ca.energy_lin == cb.energy_lin),
                ("energy_ld", ca.energy_ld == cb.energy_ld),
                ("weighted", ca.weighted == cb.weighted),
                ("threshold", ca.threshold == cb.threshold),
                ("ff", ca.ff == cb.ff),
                ("enfac", ca.enfac == cb.enfac),
                ("min_snr0", ca.min_snr0 == cb.min_snr0),
                ("spread0", ca.spread0 == cb.spread0),
                ("offsets", ca.offsets == cb.offsets),
                ("group_len", ca.group_len == cb.group_len),
                (
                    "geo",
                    ca.cnt == cb.cnt && ca.per == cb.per && ca.max == cb.max && ca.seq == cb.seq,
                ),
            ] {
                push_diff(
                    &mut d,
                    DiffStage::S2,
                    &format!("{prefix}.s2.ch{i}.{leaf}"),
                    eq,
                );
            }
        }
    }
    // S4.
    {
        let (a, b) = (&q.s4, &f.s4);
        push_diff(
            &mut d,
            DiffStage::S4,
            &format!("{prefix}.s4.ch_count"),
            a.ch.len() == b.ch.len(),
        );
        for (i, (ca, cb)) in a.ch.iter().zip(b.ch.iter()).enumerate() {
            for (leaf, eq) in [
                ("spectrum_pre", ca.spectrum_pre == cb.spectrum_pre),
                ("energy_ld", ca.energy_ld == cb.energy_ld),
                ("threshold_ld", ca.threshold_ld == cb.threshold_ld),
                ("ff", ca.ff == cb.ff),
                ("offsets", ca.offsets == cb.offsets),
                (
                    "geo",
                    ca.cnt == cb.cnt && ca.per == cb.per && ca.max == cb.max,
                ),
                ("scf", ca.scf == cb.scf),
                ("gain", ca.gain == cb.gain),
                ("quant_spec", ca.quant_spec == cb.quant_spec),
                ("mutated", ca.mutated == cb.mutated),
            ] {
                push_diff(
                    &mut d,
                    DiffStage::S4,
                    &format!("{prefix}.s4.ch{i}.{leaf}"),
                    eq,
                );
            }
        }
    }
    // S6 (scalar leaves + per-channel input leaves + output leaves).
    {
        let (a, b) = (&q.s6, &f.s6);
        for (leaf, eq) in [
            ("no_red_pe", a.no_red_pe == b.no_red_pe),
            ("static_bits", a.static_bits == b.static_bits),
            ("ms_digest", a.ms_digest == b.ms_digest),
            ("dist_pre", a.dist_pre == b.dist_pre),
            ("qc_pre", a.qc_pre == b.qc_pre),
            ("qc_max", a.qc_max == b.qc_max),
            ("dist_post", a.dist_post == b.dist_post),
            ("qc_post", a.qc_post == b.qc_post),
            ("noise", a.noise == b.noise),
            (
                "grants",
                a.output.granted_dyn_bits == b.output.granted_dyn_bits
                    && a.output.max_dyn_bits == b.output.max_dyn_bits
                    && a.output.total_avail == b.output.total_avail
                    && a.output.el_granted == b.output.el_granted
                    && a.output.granted_pe == b.output.granted_pe
                    && a.output.granted_pe_corr == b.output.granted_pe_corr,
            ),
            (
                "passes",
                a.output.passes == b.output.passes
                    && a.output.final_gains == b.output.final_gains
                    && a.output.final_max_sfb == b.output.final_max_sfb,
            ),
            (
                "sections",
                a.output.final_sections == b.output.final_sections
                    && a.output.final_quants == b.output.final_quants
                    && a.output.final_qmaxs == b.output.final_qmaxs,
            ),
            (
                "bits",
                a.output.static_bits_used == b.output.static_bits_used
                    && a.output.used_dyn_bits == b.output.used_dyn_bits
                    && a.output.tot_fill_bits == b.output.tot_fill_bits
                    && a.output.align_bits == b.output.align_bits
                    && a.output.total_bits == b.output.total_bits
                    && a.output.bit_res_tot == b.output.bit_res_tot,
            ),
            (
                "bit_res_level_el",
                a.output.bit_res_level_el == b.output.bit_res_level_el,
            ),
            (
                "bit_res_max_el",
                a.output.bit_res_max_el == b.output.bit_res_max_el,
            ),
            (
                "frame_consts",
                a.glob_hdr == b.glob_hdr
                    && a.max_bits_frame == b.max_bits_frame
                    && a.max_bits_el == b.max_bits_el
                    && a.relative == b.relative
                    && a.max_bit_fac == b.max_bit_fac
                    && a.bit_res_mode == b.bit_res_mode,
            ),
        ] {
            push_diff(&mut d, DiffStage::S6, &format!("{prefix}.s6.{leaf}"), eq);
        }
        push_diff(
            &mut d,
            DiffStage::S6,
            &format!("{prefix}.s6.ch_count"),
            a.ch.len() == b.ch.len(),
        );
        for (i, (ca, cb)) in a.ch.iter().zip(b.ch.iter()).enumerate() {
            for (leaf, eq) in [
                ("spectrum", ca.spectrum == cb.spectrum),
                ("scf", ca.scf == cb.scf),
                ("gain", ca.gain == cb.gain),
                ("offsets", ca.offsets == cb.offsets),
                ("tns", tns_equal(&ca.tns, &cb.tns)),
                ("is_book", ca.is_book == cb.is_book),
                ("is_scale", ca.is_scale == cb.is_scale),
                (
                    "geo",
                    ca.cnt == cb.cnt
                        && ca.max == cb.max
                        && ca.per == cb.per
                        && ca.block_type == cb.block_type,
                ),
                ("seed_quant", ca.seed_quant == cb.seed_quant),
            ] {
                push_diff(
                    &mut d,
                    DiffStage::S6,
                    &format!("{prefix}.s6.ch{i}.{leaf}"),
                    eq,
                );
            }
        }
    }
    // Frame carriers.
    {
        let (a, b) = (&q.state, &f.state);
        push_diff(
            &mut d,
            DiffStage::State,
            &format!("{prefix}.state.ats_pre"),
            ats_equal(&a.ats_pre, &b.ats_pre),
        );
        push_diff(
            &mut d,
            DiffStage::State,
            &format!("{prefix}.state.ats_post"),
            ats_equal(&a.ats_post, &b.ats_post),
        );
        push_diff(
            &mut d,
            DiffStage::State,
            &format!("{prefix}.state.dist"),
            a.dist_pre == b.dist_pre && a.dist_post == b.dist_post,
        );
        push_diff(
            &mut d,
            DiffStage::State,
            &format!("{prefix}.state.qc"),
            a.qc_pre == b.qc_pre && a.qc_post == b.qc_post && a.qc_max == b.qc_max,
        );
        push_diff(
            &mut d,
            DiffStage::State,
            &format!("{prefix}.state.pad"),
            a.pad_pre == b.pad_pre && a.pad_post == b.pad_post,
        );
    }
    // Frame accounting (`R22AcctCap` mirrors the `FrameSnapshot` coded
    // fields plus the reservoir/padding carriers; every leaf participates
    // so a downstream accounting divergence cannot hide here).
    {
        let (a, b) = (&q.acct, &f.acct);
        for (leaf, eq) in [
            ("side", a.side == b.side),
            ("huff", a.huff == b.huff),
            ("scf_bits", a.scf_bits == b.scf_bits),
            ("used_dyn", a.used_dyn == b.used_dyn),
            ("tot_fill", a.tot_fill == b.tot_fill),
            ("align", a.align == b.align),
            ("total", a.total == b.total),
            ("npass", a.npass == b.npass),
            ("crash", a.crash == b.crash),
            ("gains", a.gains == b.gains),
            ("last_iterations", a.last_iterations == b.last_iterations),
            ("sum_bits", a.sum_bits == b.sum_bits),
            ("el_granted", a.el_granted == b.el_granted),
            ("max_dyn", a.max_dyn == b.max_dyn),
            ("total_avail", a.total_avail == b.total_avail),
            ("static_used", a.static_used == b.static_used),
            ("pad_pre", a.pad_pre == b.pad_pre),
            ("pad_post", a.pad_post == b.pad_post),
            ("bit_res_tot", a.bit_res_tot == b.bit_res_tot),
            ("bit_res_max", a.bit_res_max == b.bit_res_max),
        ] {
            push_diff(
                &mut d,
                DiffStage::Acct,
                &format!("{prefix}.acct.{leaf}"),
                eq,
            );
        }
    }
    d
}

// ---------------------------------------------------------------------------
// Harness helpers
// ---------------------------------------------------------------------------

fn fresh_frame(pcm: &[Vec<i16>], frame: u32) -> Vec<Vec<i16>> {
    pcm.iter()
        .map(|ch| {
            let mut b = vec![0i16; FRAME_LENGTH];
            for (i, v) in b.iter_mut().enumerate() {
                let t = frame as usize * FRAME_LENGTH + i;
                if t < ch.len() {
                    *v = ch[t];
                }
            }
            b
        })
        .collect()
}

fn q31_encoder(sample_rate: u32, channels: usize) -> StreamEncoder {
    StreamEncoder::with_mode(sample_rate, channels, VbrMode::Vbr5)
}

fn fpe_encoder(sample_rate: u32, channels: usize) -> StreamEncoder {
    let cfg = EncoderConfig::new(
        sample_rate,
        channels as u32,
        FRAME_LENGTH,
        TransportType::Adts,
    )
    .expect("cfg")
    .with_vbr_mode(VbrMode::Vbr5)
    .with_psy_engine(PsyEngineKind::Fpe);
    StreamEncoder::with_config(cfg)
}

/// Step one encoder over `frames`, returning emitted bytes + snapshots.
/// Every frame must emit cleanly with a complete capture (loud otherwise).
fn step_clean(
    enc: &mut StreamEncoder,
    pcm: &[Vec<i16>],
    frames: std::ops::Range<u32>,
    tag: &str,
) -> Vec<(Vec<u8>, FrameSnapshot)> {
    let mut out = Vec::new();
    for f in frames {
        let fresh = fresh_frame(pcm, f);
        let (bytes, snap) = enc.step_frame(f, &fresh);
        assert!(
            snap.notes.is_empty(),
            "{tag} f={f}: unexpected notes {:?}",
            snap.notes
        );
        let bytes = bytes.unwrap_or_else(|| panic!("{tag} f={f}: must emit"));
        assert!(snap.r22_cap.is_some(), "{tag} f={f}: missing R22 capture");
        out.push((bytes, snap));
    }
    out
}

/// Assert two snapshots carry the same coded outcome (bytes compared
/// separately by the caller).
#[allow(clippy::too_many_arguments)]
fn assert_same_coded(a: &FrameSnapshot, b: &FrameSnapshot, tag: &str) {
    assert_eq!(a.seq, b.seq, "{tag} seq");
    assert_eq!(a.digest, b.digest, "{tag} digest");
    assert_eq!(a.mask_ones, b.mask_ones, "{tag} mask_ones");
    assert_eq!(a.static_bits, b.static_bits, "{tag} static_bits");
    assert_eq!(a.pe, b.pe, "{tag} pe");
    assert_eq!(a.gains, b.gains, "{tag} gains");
    assert_eq!(a.side_bits, b.side_bits, "{tag} side_bits");
    assert_eq!(a.huff_bits, b.huff_bits, "{tag} huff_bits");
    assert_eq!(a.scf_bits, b.scf_bits, "{tag} scf_bits");
    assert_eq!(a.used_dyn, b.used_dyn, "{tag} used_dyn");
    assert_eq!(a.tot_fill, b.tot_fill, "{tag} tot_fill");
    assert_eq!(a.align, b.align, "{tag} align");
    assert_eq!(a.total, b.total, "{tag} total");
    assert_eq!(a.npass, b.npass, "{tag} npass");
    assert_eq!(a.crash, b.crash, "{tag} crash");
    assert_eq!(a.adts_len, b.adts_len, "{tag} adts_len");
    assert_eq!(
        a.last_iterations, b.last_iterations,
        "{tag} last_iterations"
    );
    assert_eq!(a.sum_bits, b.sum_bits, "{tag} sum_bits");
    assert_eq!(a.el_granted, b.el_granted, "{tag} el_granted");
    assert_eq!(a.max_dyn, b.max_dyn, "{tag} max_dyn");
    assert_eq!(a.total_avail, b.total_avail, "{tag} total_avail");
    assert_eq!(a.static_used, b.static_used, "{tag} static_used");
    assert_eq!(a.notes, b.notes, "{tag} notes");
}

/// Group A coverage corpus: (fixture, first frame, frame count).
/// Empirically pinned window sequences with fresh-encoder harness
/// semantics (each case steps a fresh encoder from `first`; `snap.seq`:
/// 0 LONG, 1 START, 2 SHORT, 3 STOP; dumped 2026-11-24, Q31 and FPE arms
/// identical):
/// * `sine-stereo` 0..8: startup `[1,2,3,0,0,0,0,0]`.
/// * `onset-stereo` 18..28: `[0,0,0,0,0,1,2,3,0,0]` — five LONGs from the
///   fresh start, then the interior transient at sample 24000 (frame 23)
///   drives START(f23)→SHORT(f24)→STOP(f25)→LONG(f26).
/// * `click-mono` 8..16: startup `[1,2,3,0,0,0,0,0]` on the mono path
///   (no `ms_mask`); covers all four sequences but via the fresh-start
///   transient, not via the interior click at sample 12000.
/// * `impulse-mono` 2..10: `[0,0,1,2,3,0,0,0]` — two LONGs from the fresh
///   start, then the interior impulse at ~0.1 s (sample 4800, frame 4)
///   drives START(f4)→SHORT(f5)→STOP(f6)→LONG(f7). This is the
///   content-driven mono transition proof (PE-only element-global
///   decisions, no `ms_mask`).
const R22_COVERAGE_CASES: [(&str, u32, u32); 4] = [
    ("sine-stereo", 0, 8),
    ("onset-stereo", 18, 10),
    ("click-mono", 8, 8),
    ("impulse-mono", 2, 8),
];

/// Snapshot completeness: every stage captured, every vector at geometry
/// length, mask/flag presence per channel mode, no empty captures.
/// `pub(super)` for Group C reuse (`decision.rs` OP matrix).
pub(super) fn check_snapshot_completeness(cap: &R22Capture, channels: usize, tag: &str) {
    // S1.
    assert_eq!(cap.s1.ch.len(), channels, "{tag} s1 channels");
    for (i, c) in cap.s1.ch.iter().enumerate() {
        assert_eq!(c.energy_ld.len(), c.cnt, "{tag} s1 ch{i} energy_ld");
        assert_eq!(c.threshold_ld.len(), c.cnt, "{tag} s1 ch{i} threshold_ld");
        assert_eq!(c.energy_lin.len(), c.cnt, "{tag} s1 ch{i} energy_lin");
        assert_eq!(c.ff.len(), c.cnt, "{tag} s1 ch{i} ff");
        assert_eq!(c.offsets.len(), c.cnt + 1, "{tag} s1 ch{i} offsets");
        assert_eq!(c.is_book.len(), c.cnt, "{tag} s1 ch{i} is_book");
        assert_eq!(c.is_scale.len(), c.cnt, "{tag} s1 ch{i} is_scale");
        assert!(c.cnt > 0, "{tag} s1 ch{i} empty geometry");
    }
    if channels == 2 {
        assert_eq!(
            cap.s1.ms_mask.len(),
            cap.s1.ch[0].cnt,
            "{tag} s1 ms_mask stereo span"
        );
    } else {
        assert!(cap.s1.ms_mask.is_empty(), "{tag} s1 ms_mask mono empty");
    }
    assert_eq!(cap.s1.weighted.len(), channels, "{tag} s1 weighted arms");
    assert_eq!(
        cap.s1.thresholds.len(),
        channels,
        "{tag} s1 thresholds arms"
    );
    assert_eq!(cap.s1.enfac.len(), channels, "{tag} s1 enfac arms");
    assert_eq!(cap.s1.pe_ch.len(), channels, "{tag} s1 pe_ch arms");
    // S2.
    assert_eq!(cap.s2.ch.len(), channels, "{tag} s2 channels");
    if channels == 2 {
        assert!(cap.s2.ms_mask.is_some(), "{tag} s2 stereo mask present");
    } else {
        assert!(cap.s2.ms_mask.is_none(), "{tag} s2 mono mask absent");
    }
    for (i, c) in cap.s2.ch.iter().enumerate() {
        assert_eq!(c.energy_lin.len(), c.cnt, "{tag} s2 ch{i} energy_lin");
        assert_eq!(c.weighted.len(), c.cnt, "{tag} s2 ch{i} weighted");
        assert_eq!(c.threshold.len(), c.cnt, "{tag} s2 ch{i} threshold");
        assert_eq!(c.ff.len(), c.cnt, "{tag} s2 ch{i} ff");
        assert_eq!(c.enfac.len(), c.cnt, "{tag} s2 ch{i} enfac");
        assert_eq!(c.min_snr0.len(), c.cnt, "{tag} s2 ch{i} min_snr0");
        assert_eq!(c.spread0.len(), c.cnt, "{tag} s2 ch{i} spread0");
    }
    assert_eq!(cap.s2.threshold_ld.len(), channels, "{tag} s2 thrv arms");
    assert_eq!(cap.s2.thr_exp.len(), channels, "{tag} s2 thr_exp arms");
    assert_eq!(cap.s2.min_snr_ld.len(), channels, "{tag} s2 minsnr arms");
    assert_eq!(cap.s2.spread.len(), channels, "{tag} s2 spread arms");
    for (i, ah) in cap.s2.ah_flag.iter().enumerate() {
        assert_eq!(ah.len(), cap.s2.ch[i].cnt, "{tag} s2 ch{i} ah_flag");
    }
    // S4.
    assert_eq!(cap.s4.ch.len(), channels, "{tag} s4 channels");
    for (i, c) in cap.s4.ch.iter().enumerate() {
        assert_eq!(c.spectrum_pre.len(), 1024, "{tag} s4 ch{i} spectrum");
        assert_eq!(c.scf.len(), c.cnt, "{tag} s4 ch{i} scf");
        assert_eq!(c.quant_spec.len(), 1024, "{tag} s4 ch{i} quant");
        assert_eq!(c.mutated.len(), 1024, "{tag} s4 ch{i} mutated");
    }
    // S6.
    assert_eq!(cap.s6.ch.len(), channels, "{tag} s6 channels");
    assert_eq!(cap.s6.noise.len(), cap.s6.ch[0].cnt, "{tag} s6 noise span");
    for (i, c) in cap.s6.ch.iter().enumerate() {
        assert_eq!(c.spectrum.len(), 1024, "{tag} s6 ch{i} spectrum");
        assert_eq!(c.scf.len(), c.cnt, "{tag} s6 ch{i} scf");
        assert!(c.seed_quant.is_some(), "{tag} s6 ch{i} seed present");
        assert_eq!(
            c.seed_quant.as_ref().expect("seed").len(),
            1024,
            "{tag} s6 ch{i} seed span"
        );
        assert_eq!(c.offsets.len(), c.cnt + 1, "{tag} s6 ch{i} offsets");
    }
    assert!(!cap.s6.output.passes.is_empty(), "{tag} s6 passes nonempty");
    assert_eq!(
        cap.s6.output.final_quants.len(),
        channels,
        "{tag} s6 quants arms"
    );
    // Carriers: reservoir max pins the init; padding evolves content-free.
    assert!(cap.state.qc_max > 0, "{tag} state qc_max positive");
}

// ---------------------------------------------------------------------------
// Group A tests
// ---------------------------------------------------------------------------

#[test]
fn r22_snapshot_completeness() {
    for (name, first, count) in R22_COVERAGE_CASES {
        let pcm = testdata::fixture_samples(name);
        let sr = testdata::fixture_sample_rate(name);
        let channels = pcm.len();
        for (arm, mut enc) in [
            ("q31", q31_encoder(sr, channels)),
            ("fpe", fpe_encoder(sr, channels)),
        ] {
            let tag = format!("r22 completeness {name} {arm}");
            let steps = step_clean(&mut enc, &pcm, first..first + count, &tag);
            assert_eq!(steps.len() as u32, count, "{tag}: frame accounting");
            for (k, (_, snap)) in steps.iter().enumerate() {
                let cap = snap.r22_cap.as_ref().expect("r22 cap");
                check_snapshot_completeness(cap, channels, &format!("{tag} f{}", first + k as u32));
                check_acct_state(cap, snap, &format!("{tag} f{}", first + k as u32));
            }
        }
    }
}

#[test]
fn r22_mono_window_transitions() {
    // F-2: mono LONG/START/SHORT/STOP coverage on `impulse-mono` 2..10.
    // Source evidence (dumped 2026-11-24 with fresh-encoder harness
    // semantics, Q31 and FPE arms identical): impulse-mono 2..10 =
    // [0,0,1,2,3,0,0,0]. Two LONGs from the fresh start, then the interior
    // impulse at ~0.1 s (sample 4800, frame 4) drives
    // START(f4)→SHORT(f5)→STOP(f6)→LONG(f7). This test asserts that exact
    // sequence is exercised *and* captured (snapshot `seq` plus the S2/S6
    // window leaves in `R22Capture`). The pre-existing `click-mono` 8..16
    // window ([1,2,3,0,0,0,0,0]) covers all four via the fresh-start
    // transient; the impulse window proves content-driven mono switching
    // after settled LONGs.
    let name = "impulse-mono";
    let (first, count) = (2u32, 8u32);
    let expect_seqs = [0i32, 0, 1, 2, 3, 0, 0, 0];
    let pcm = testdata::fixture_samples(name);
    let sr = testdata::fixture_sample_rate(name);
    assert_eq!(pcm.len(), 1, "impulse-mono is mono");
    for (arm, mut enc) in [("q31", q31_encoder(sr, 1)), ("fpe", fpe_encoder(sr, 1))] {
        let tag = format!("r22 mono windows {name} {arm}");
        let steps = step_clean(&mut enc, &pcm, first..first + count, &tag);
        assert_eq!(steps.len(), expect_seqs.len(), "{tag}: frame accounting");
        let mut seen = [false; 4];
        for (k, (_, snap)) in steps.iter().enumerate() {
            let f = first + k as u32;
            let cap = snap.r22_cap.as_ref().expect("r22 cap");
            // The intended sequence is actually exercised: exact per-frame
            // `snap.seq`, not just a set-membership check.
            assert_eq!(snap.seq, expect_seqs[k], "{tag} f{f}: window sequence");
            seen[snap.seq as usize] = true;
            // The same window is captured in the R22 leaves: S2 geometry
            // `seq` and S6 `block_type` agree with the snapshot.
            assert_eq!(
                cap.s2.ch[0].seq as i32, snap.seq,
                "{tag} f{f}: s2 seq captured"
            );
            assert_eq!(
                cap.s6.ch[0].block_type, snap.seq,
                "{tag} f{f}: s6 block_type captured"
            );
            // Mono path: no MS masks on either stage.
            assert!(cap.s1.ms_mask.is_empty(), "{tag} f{f}: s1 mono mask");
            assert!(cap.s2.ms_mask.is_none(), "{tag} f{f}: s2 mono mask");
        }
        assert!(
            seen.iter().all(|&s| s),
            "{tag}: must exercise LONG/START/SHORT/STOP, seen {seen:?}"
        );
    }
    // Contrast anchor: `click-mono` 8..16 exercises [1,2,3,0,0,0,0,0] via
    // the fresh-start transient (not via its interior click at sample
    // 12000). Asserted here so the two mono windows are not conflated.
    let pcm_c = testdata::fixture_samples("click-mono");
    let sr_c = testdata::fixture_sample_rate("click-mono");
    let mut enc_c = q31_encoder(sr_c, 1);
    let steps_c = step_clean(&mut enc_c, &pcm_c, 8..16, "r22 mono contrast click");
    let expect_click = [1i32, 2, 3, 0, 0, 0, 0, 0];
    for (k, (_, snap)) in steps_c.iter().enumerate() {
        assert_eq!(
            snap.seq,
            expect_click[k],
            "click-mono f{}: fresh-start sequence",
            8 + k as u32
        );
    }
}

#[test]
fn r22_within_arm_replay_q31() {
    for (name, first, count) in R22_COVERAGE_CASES {
        let pcm = testdata::fixture_samples(name);
        let sr = testdata::fixture_sample_rate(name);
        let mut enc = q31_encoder(sr, pcm.len());
        let tag = format!("r22 replay q31 {name}");
        let steps = step_clean(&mut enc, &pcm, first..first + count, &tag);
        for (k, (_, snap)) in steps.iter().enumerate() {
            let ftag = format!("{tag} f{}", first + k as u32);
            replay_frame(snap.r22_cap.as_ref().expect("r22 cap"), &ftag);
        }
    }
}

#[test]
fn r22_within_arm_replay_fpe() {
    for (name, first, count) in R22_COVERAGE_CASES {
        let pcm = testdata::fixture_samples(name);
        let sr = testdata::fixture_sample_rate(name);
        let mut enc = fpe_encoder(sr, pcm.len());
        let tag = format!("r22 replay fpe {name}");
        let steps = step_clean(&mut enc, &pcm, first..first + count, &tag);
        for (k, (_, snap)) in steps.iter().enumerate() {
            let ftag = format!("{tag} f{}", first + k as u32);
            replay_frame(snap.r22_cap.as_ref().expect("r22 cap"), &ftag);
        }
    }
}

#[test]
fn r22_s4_workspace_invariance() {
    // The production frame shares one encoder-owned workspace across its
    // channels; replaying each channel with a *fresh* workspace must agree
    // too. Equality proves the workspace is pure scratch: no channel reads
    // state left by an earlier channel or frame.
    for (name, first, count) in R22_COVERAGE_CASES {
        let pcm = testdata::fixture_samples(name);
        let sr = testdata::fixture_sample_rate(name);
        for (arm, mut enc) in [
            ("q31", q31_encoder(sr, pcm.len())),
            ("fpe", fpe_encoder(sr, pcm.len())),
        ] {
            let tag = format!("r22 s4ws {name} {arm}");
            let steps = step_clean(&mut enc, &pcm, first..first + count, &tag);
            for (k, (_, snap)) in steps.iter().enumerate() {
                let cap = snap.r22_cap.as_ref().expect("r22 cap");
                for (i, c) in cap.s4.ch.iter().enumerate() {
                    let mut ws = ScfWorkspace::default();
                    replay_s4_channel(c, &mut ws, &format!("{tag} f{} ch{i}", first + k as u32));
                }
            }
        }
    }
}

#[test]
fn r22_capture_canary_enabled_vs_disabled() {
    // Capture gate canary: two identically seeded encoders over the same
    // PCM, one with the R22 snapshot enabled (and fully replayed per
    // frame), one with it disabled. Emitted bytes and every coded field
    // must agree exactly: the instrumentation is observational only.
    let name = "onset-stereo";
    let pcm = testdata::fixture_samples(name);
    let sr = testdata::fixture_sample_rate(name);
    let (first, count) = (18u32, 10u32);
    let mut enc_on = q31_encoder(sr, pcm.len());
    let mut enc_off = q31_encoder(sr, pcm.len());
    enc_off.set_r22_capture(false);
    for f in first..first + count {
        let fresh = fresh_frame(&pcm, f);
        let (bytes_on, snap_on) = enc_on.step_frame(f, &fresh);
        let (bytes_off, snap_off) = enc_off.step_frame(f, &fresh);
        let tag = format!("r22 canary on/off {name} f{f}");
        assert!(snap_on.notes.is_empty(), "{tag} on notes");
        assert!(snap_off.notes.is_empty(), "{tag} off notes");
        assert_eq!(bytes_on, bytes_off, "{tag} emitted bytes");
        assert_same_coded(&snap_on, &snap_off, &tag);
        // The gate itself works: enabled captures, disabled does not.
        assert!(snap_on.r22_cap.is_some(), "{tag} on captures");
        assert!(snap_off.r22_cap.is_none(), "{tag} off skips");
        // Replay on the enabled arm (still must not disturb anything: the
        // next frame below steps both encoders identically).
        replay_frame(snap_on.r22_cap.as_ref().expect("r22 cap"), &tag);
    }
}

#[test]
fn r22_replay_does_not_mutate_live_state() {
    // Replay side-effect canary: snapshot the encoder's observable
    // frame-surviving state, run every stage replay, and prove nothing
    // moved. A parallel never-replayed encoder must then emit identical
    // bytes on the following frame.
    let name = "sine-stereo";
    let pcm = testdata::fixture_samples(name);
    let sr = testdata::fixture_sample_rate(name);
    let (first, count) = (0u32, 8u32);
    let mut enc_a = q31_encoder(sr, pcm.len());
    let mut enc_b = q31_encoder(sr, pcm.len());
    for f in first..first + count {
        let fresh = fresh_frame(&pcm, f);
        let (bytes_a, snap_a) = enc_a.step_frame(f, &fresh);
        let (bytes_b, snap_b) = enc_b.step_frame(f, &fresh);
        let tag = format!("r22 canary replay-pure {name} f{f}");
        assert_eq!(bytes_a, bytes_b, "{tag} lockstep bytes");
        assert_same_coded(&snap_a, &snap_b, &tag);
        let cap = snap_a.r22_cap.as_ref().expect("r22 cap");
        let before = enc_a.r22_observable_state();
        replay_frame(cap, &tag);
        let after = enc_a.r22_observable_state();
        assert_observable_eq(&before, &after, &tag);
    }
    // Histories still agree after all that replaying: one more lockstep frame.
    let f = first + count;
    let fresh = fresh_frame(&pcm, f);
    let (bytes_a, snap_a) = enc_a.step_frame(f, &fresh);
    let (bytes_b, snap_b) = enc_b.step_frame(f, &fresh);
    assert_eq!(bytes_a, bytes_b, "r22 canary post-replay bytes f{f}");
    assert_same_coded(&snap_a, &snap_b, &format!("r22 canary post-replay f{f}"));
}

#[test]
fn r22_cross_arm_differential_recorded() {
    // Cross-arm differential capture: Q31 and FPE encoders step in lockstep
    // and every frame's captures are diffed leaf-by-leaf. The record is
    // structural only — this test asserts the machinery is sound (self-diff
    // is empty; the cross record is well-formed) and NEVER asserts
    // cross-arm equality. Classification belongs to Group D.
    let name = "onset-stereo";
    let pcm = testdata::fixture_samples(name);
    let sr = testdata::fixture_sample_rate(name);
    let (first, count) = (18u32, 10u32);
    let mut enc_q = q31_encoder(sr, pcm.len());
    let mut enc_f = fpe_encoder(sr, pcm.len());
    // Non-vacuity: the window must contain at least one recorded
    // difference, proving the differential capture observes real
    // Q31-vs-FPE divergence (it does: the FPE energy residue surfaces in
    // S1 weighted vectors on transient frames). This asserts existence of
    // a record, never equality.
    let mut frames_with_diffs = 0u32;
    for f in first..first + count {
        let fresh = fresh_frame(&pcm, f);
        let (_, snap_q) = enc_q.step_frame(f, &fresh);
        let (_, snap_f) = enc_f.step_frame(f, &fresh);
        let tag = format!("r22 diff {name} f{f}");
        assert!(snap_q.notes.is_empty(), "{tag} q notes");
        assert!(snap_f.notes.is_empty(), "{tag} f notes");
        let q = snap_q.r22_cap.as_ref().expect("q r22 cap");
        let fp = snap_f.r22_cap.as_ref().expect("f r22 cap");
        // Self-diff is the soundness anchor: identical captures diff empty.
        let self_diff = diff_captures(q, q, &tag);
        assert!(
            self_diff.differing_fields.is_empty(),
            "{tag} self-diff must be empty"
        );
        assert!(self_diff.s1_equal, "{tag} self s1");
        assert!(self_diff.s2_equal, "{tag} self s2");
        assert!(self_diff.s4_equal, "{tag} self s4");
        assert!(self_diff.s6_equal, "{tag} self s6");
        assert!(self_diff.state_equal, "{tag} self state");
        assert!(self_diff.acct_equal, "{tag} self acct");
        // Cross-arm record: well-formed (flags consistent with the field
        // list), recorded — not asserted equal, not hidden.
        let cross = diff_captures(q, fp, &tag);
        let flagged = !cross.s1_equal
            || !cross.s2_equal
            || !cross.s4_equal
            || !cross.s6_equal
            || !cross.state_equal
            || !cross.acct_equal;
        assert_eq!(
            flagged,
            !cross.differing_fields.is_empty(),
            "{tag} record well-formed"
        );
        for field in &cross.differing_fields {
            // Path shape is `{prefix}.{stage}.{leaf}`; the prefix itself
            // contains no dots, so the stage is the second segment.
            let stage = field.split('.').nth(1).unwrap_or("?");
            match stage {
                "s1" => assert!(!cross.s1_equal, "{tag} {field} flag"),
                "s2" => assert!(!cross.s2_equal, "{tag} {field} flag"),
                "s4" => assert!(!cross.s4_equal, "{tag} {field} flag"),
                "s6" => assert!(!cross.s6_equal, "{tag} {field} flag"),
                "state" => assert!(!cross.state_equal, "{tag} {field} flag"),
                "acct" => assert!(!cross.acct_equal, "{tag} {field} flag"),
                _ => panic!("{tag} unexpected diff path {field}"),
            }
        }
        if !cross.differing_fields.is_empty() {
            frames_with_diffs += 1;
        }
    }
    assert!(
        frames_with_diffs > 0,
        "r22 diff {name}: expected at least one recorded cross-arm difference"
    );
}

/// Focused F-1 proofs: each newly compared leaf must flip the record when
/// mutated. Each test clones a real production capture, mutates exactly one
/// leaf, and asserts the exact `differing_fields` path appears with the
/// owning stage flag cleared — while the untouched self-diff stays empty.
/// No Q31/FPE identity is asserted here.
#[test]
fn r22_diff_detects_s1_pe_ch() {
    let name = "onset-stereo";
    let pcm = testdata::fixture_samples(name);
    let sr = testdata::fixture_sample_rate(name);
    let mut enc = q31_encoder(sr, pcm.len());
    let steps = step_clean(&mut enc, &pcm, 18..19, "r22 diff s1 pe_ch");
    let base = steps[0].1.r22_cap.as_ref().expect("r22 cap").clone();
    assert!(
        diff_captures(&base, &base, "self")
            .differing_fields
            .is_empty(),
        "self-diff empty"
    );
    let mut mutated = base.clone();
    let ch0 = mutated.s1.pe_ch.first_mut().expect("pe_ch nonempty");
    ch0.pe = ch0.pe.wrapping_add(1);
    let d = diff_captures(&base, &mutated, "probe");
    assert!(!d.s1_equal, "s1 flag must clear on pe_ch.pe");
    assert!(d.s2_equal && d.s4_equal && d.s6_equal && d.state_equal && d.acct_equal);
    assert!(
        d.differing_fields.iter().any(|p| p == "probe.s1.pe_ch0.pe"),
        "expected probe.s1.pe_ch0.pe, got {:?}",
        d.differing_fields
    );
    // A per-band leaf is equally covered.
    let mut mutated_band = base.clone();
    let ch0b = mutated_band.s1.pe_ch.first_mut().expect("pe_ch nonempty");
    ch0b.sfb_pe[0] = ch0b.sfb_pe[0].wrapping_add(1);
    let db = diff_captures(&base, &mutated_band, "probe");
    assert!(
        db.differing_fields
            .iter()
            .any(|p| p == "probe.s1.pe_ch0.sfb_pe"),
        "expected probe.s1.pe_ch0.sfb_pe, got {:?}",
        db.differing_fields
    );
}

#[test]
fn r22_diff_detects_s6_reservoir_levels() {
    let name = "onset-stereo";
    let pcm = testdata::fixture_samples(name);
    let sr = testdata::fixture_sample_rate(name);
    let mut enc = q31_encoder(sr, pcm.len());
    let steps = step_clean(&mut enc, &pcm, 23..24, "r22 diff s6 reslevel");
    let base = steps[0].1.r22_cap.as_ref().expect("r22 cap").clone();
    let mut mutated = base.clone();
    mutated.s6.output.bit_res_level_el = mutated.s6.output.bit_res_level_el.wrapping_add(1);
    let d = diff_captures(&base, &mutated, "probe");
    assert!(!d.s6_equal, "s6 flag must clear on bit_res_level_el");
    assert!(
        d.differing_fields
            .iter()
            .any(|p| p == "probe.s6.bit_res_level_el"),
        "expected probe.s6.bit_res_level_el, got {:?}",
        d.differing_fields
    );
    let mut mutated2 = base.clone();
    mutated2.s6.output.bit_res_max_el = mutated2.s6.output.bit_res_max_el.wrapping_add(1);
    let d2 = diff_captures(&base, &mutated2, "probe");
    assert!(!d2.s6_equal, "s6 flag must clear on bit_res_max_el");
    assert!(
        d2.differing_fields
            .iter()
            .any(|p| p == "probe.s6.bit_res_max_el"),
        "expected probe.s6.bit_res_max_el, got {:?}",
        d2.differing_fields
    );
    // Unmutated stages stay equal.
    assert!(d.s1_equal && d.s2_equal && d.s4_equal && d.state_equal && d.acct_equal);
}

#[test]
fn r22_diff_detects_s6_ch_offsets() {
    let name = "onset-stereo";
    let pcm = testdata::fixture_samples(name);
    let sr = testdata::fixture_sample_rate(name);
    let mut enc = q31_encoder(sr, pcm.len());
    let steps = step_clean(&mut enc, &pcm, 23..24, "r22 diff s6 offsets");
    let base = steps[0].1.r22_cap.as_ref().expect("r22 cap").clone();
    let mut mutated = base.clone();
    mutated.s6.ch[0].offsets[0] = mutated.s6.ch[0].offsets[0].wrapping_add(1);
    let d = diff_captures(&base, &mutated, "probe");
    assert!(!d.s6_equal, "s6 flag must clear on ch offsets");
    assert!(
        d.differing_fields
            .iter()
            .any(|p| p == "probe.s6.ch0.offsets"),
        "expected probe.s6.ch0.offsets, got {:?}",
        d.differing_fields
    );
    assert!(d.s1_equal && d.s2_equal && d.s4_equal && d.state_equal && d.acct_equal);
}

#[test]
fn r22_diff_detects_s6_ch_tns() {
    let name = "onset-stereo";
    let pcm = testdata::fixture_samples(name);
    let sr = testdata::fixture_sample_rate(name);
    let mut enc = q31_encoder(sr, pcm.len());
    let steps = step_clean(&mut enc, &pcm, 23..24, "r22 diff s6 tns");
    let base = steps[0].1.r22_cap.as_ref().expect("r22 cap").clone();
    let mut mutated = base.clone();
    mutated.s6.ch[0].tns.merged = !mutated.s6.ch[0].tns.merged;
    let d = diff_captures(&base, &mutated, "probe");
    assert!(!d.s6_equal, "s6 flag must clear on ch tns");
    assert!(
        d.differing_fields.iter().any(|p| p == "probe.s6.ch0.tns"),
        "expected probe.s6.ch0.tns, got {:?}",
        d.differing_fields
    );
    assert!(d.s1_equal && d.s2_equal && d.s4_equal && d.state_equal && d.acct_equal);
}

#[test]
fn r22_diff_detects_acct() {
    let name = "onset-stereo";
    let pcm = testdata::fixture_samples(name);
    let sr = testdata::fixture_sample_rate(name);
    let mut enc = q31_encoder(sr, pcm.len());
    let steps = step_clean(&mut enc, &pcm, 23..24, "r22 diff acct");
    let base = steps[0].1.r22_cap.as_ref().expect("r22 cap").clone();
    assert!(
        diff_captures(&base, &base, "self")
            .differing_fields
            .is_empty(),
        "self-diff empty"
    );
    let mut mutated = base.clone();
    mutated.acct.total = mutated.acct.total.wrapping_add(1);
    let d = diff_captures(&base, &mutated, "probe");
    assert!(!d.acct_equal, "acct flag must clear on acct.total");
    assert!(
        d.differing_fields.iter().any(|p| p == "probe.acct.total"),
        "expected probe.acct.total, got {:?}",
        d.differing_fields
    );
    assert!(d.s1_equal && d.s2_equal && d.s4_equal && d.s6_equal && d.state_equal);
    // A second accounting leaf (reservoir level) is independently covered.
    let mut mutated2 = base.clone();
    mutated2.acct.bit_res_tot = mutated2.acct.bit_res_tot.wrapping_add(1);
    let d2 = diff_captures(&base, &mutated2, "probe");
    assert!(
        d2.differing_fields
            .iter()
            .any(|p| p == "probe.acct.bit_res_tot"),
        "expected probe.acct.bit_res_tot, got {:?}",
        d2.differing_fields
    );
}

// ---------------------------------------------------------------------------
// Group B: WAV identity verification and wiring
// ---------------------------------------------------------------------------

/// Minimal self-contained SHA-256 (test-only Group B identity anchor; no
/// dependency or manifest change). Correctness is gated by
/// `r22_sha256_selftest` (FIPS vectors) before any fixture digest is trusted.
/// E1-R0.3 OD-1: `pub(super)` export for test-only reuse (visibility-only;
/// no production or Group D behaviour change).
pub(super) fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    assert_eq!(msg.len() % 64, 0, "sha256 padded length");
    for i in 0..msg.len() / 64 {
        let chunk = &msg[i * 64..(i + 1) * 64];
        let mut w = [0u32; 64];
        for (i, slot) in w.iter_mut().enumerate().take(16) {
            *slot = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|w| format!("{w:08x}")).collect()
}

#[test]
fn r22_sha256_selftest() {
    // FIPS 180-4 vectors: the fixture pins below are only trustworthy if
    // this local implementation reproduces them.
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

// ---------------------------------------------------------------------------
// E1-R0.3 OD-1: narrowly scoped test-only F-20/F-22/F-26 replay capture
// extension (synthetic only). No production encoder code or Group D decision
// behaviour is altered: this captures only the recorded M3 mechanism values
// (stored eR 0-vs-1, Ld MIN-vs-finite, real_scale MAX-clamp-vs-finite,
// book 0-vs-15) on forged synthetic operands and digests them for pinning.
// ---------------------------------------------------------------------------

/// One finding's synthetic mechanism values (forged, not corpus).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct F20SyntheticCapture {
    pub finding: &'static str,
    pub band: usize,
    pub stored_er_q: i32,
    pub stored_er_f: i32,
    pub ld_q_is_min: bool,
    pub ld_f_is_min: bool,
    pub real_q_is_max_clamp: bool,
    pub real_f_is_max_clamp: bool,
    pub book_q: i32,
    pub book_f: i32,
}

/// Test-only synthetic capture of the F-20/F-22/F-26 M3 root. Uses only forged
/// values matching the recorded mechanism (R2.1-B M3); runs no corpus, emits
/// no bundle, touches no production path.
pub(super) fn capture_f20_synthetic() -> Vec<F20SyntheticCapture> {
    vec![
        F20SyntheticCapture {
            finding: "F-20",
            band: 24,
            stored_er_q: 0,
            stored_er_f: 1,
            ld_q_is_min: true,
            ld_f_is_min: false,
            real_q_is_max_clamp: true,
            real_f_is_max_clamp: false,
            book_q: 0,
            book_f: 15,
        },
        F20SyntheticCapture {
            finding: "F-22",
            band: 38,
            stored_er_q: 0,
            stored_er_f: 1,
            ld_q_is_min: true,
            ld_f_is_min: false,
            real_q_is_max_clamp: true,
            real_f_is_max_clamp: false,
            book_q: 0,
            book_f: 15,
        },
        F20SyntheticCapture {
            finding: "F-26",
            band: 52,
            stored_er_q: 0,
            stored_er_f: 1,
            ld_q_is_min: true,
            ld_f_is_min: false,
            real_q_is_max_clamp: true,
            real_f_is_max_clamp: false,
            book_q: 0,
            book_f: 15,
        },
    ]
}

/// Digest of the synthetic F-20/F-22/F-26 capture (test-only pin, not a
/// regression pin per D-R3; exact-value regression pinning stays deferred).
pub(super) fn f20_synthetic_digest() -> String {
    let caps = capture_f20_synthetic();
    let mut s = String::new();
    for c in &caps {
        s.push_str(&format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}:{};",
            c.finding,
            c.band,
            c.stored_er_q,
            c.stored_er_f,
            c.ld_q_is_min,
            c.ld_f_is_min,
            c.real_q_is_max_clamp,
            c.real_f_is_max_clamp,
            c.book_q,
            c.book_f
        ));
    }
    sha256_hex(s.as_bytes())
}

#[test]
fn r22_f20_synthetic_capture_extension() {
    // Narrow scope: three findings, mechanism values only, synthetic.
    let caps = capture_f20_synthetic();
    assert_eq!(caps.len(), 3);
    for c in &caps {
        assert_eq!(c.stored_er_q, 0, "{} Q31 stored eR", c.finding);
        assert_eq!(c.stored_er_f, 1, "{} FPE stored eR", c.finding);
        assert!(c.ld_q_is_min, "{} Q31 Ld sentinel", c.finding);
        assert!(!c.ld_f_is_min, "{} FPE Ld finite", c.finding);
        assert!(c.real_q_is_max_clamp, "{} Q31 MAX-clamp", c.finding);
        assert!(!c.real_f_is_max_clamp, "{} FPE finite", c.finding);
        assert_eq!(c.book_q, 0, "{} Q31 drop", c.finding);
        assert_eq!(c.book_f, 15, "{} FPE retain", c.finding);
    }
    // Digest is deterministic and well-formed (64-char lowercase hex).
    let d1 = f20_synthetic_digest();
    let d2 = f20_synthetic_digest();
    assert_eq!(d1, d2, "digest deterministic");
    assert_eq!(d1.len(), 64);
    assert!(d1
        .bytes()
        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    // Production behaviour untouched: this extension runs no encoder.
}

#[test]
fn r22_wav_identity() {
    // Group B: the four committed 48 kHz stereo fixtures (design §10.3).
    // Verifies existence, SHA-256, WAV format, sample rate, channels,
    // sample count/duration, and the AAC frame count implied by the
    // driver's documented flush law; pins the canonical identities from
    // `testdata::R22_WAV_FIXTURES` against the actual repository bytes.
    assert_eq!(testdata::R22_WAV_FIXTURES.len(), 4, "four fixtures");
    for fix in testdata::R22_WAV_FIXTURES {
        // Short labels may appear in prose but must resolve unambiguously
        // to the canonical identities below (design §10.3).
        let tag = format!("r22 wav {} ({})", fix.canonical_id, fix.short_label);
        // Binding repo path resolves to the loaded file.
        assert!(
            fix.repo_path.ends_with(fix.file),
            "{tag}: repo path/file wiring"
        );
        let bytes = std::fs::read(testdata::testdata_path(fix.file))
            .unwrap_or_else(|e| panic!("{tag}: missing file {e}"));
        // Hash pin (provenance: m55/m8-PROVENANCE.txt).
        assert_eq!(sha256_hex(&bytes), fix.sha256, "{tag} sha256");
        // WAV structure: RIFF/WAVE, PCM s16le, 48 kHz, stereo.
        assert!(bytes.len() > 44, "{tag} too short");
        assert_eq!(&bytes[0..4], b"RIFF", "{tag} riff");
        assert_eq!(&bytes[8..12], b"WAVE", "{tag} wave");
        let mut off = 12usize;
        let (mut fmt, mut data) = (None, None);
        while off + 8 <= bytes.len() {
            let size = u32::from_le_bytes([
                bytes[off + 4],
                bytes[off + 5],
                bytes[off + 6],
                bytes[off + 7],
            ]) as usize;
            if &bytes[off..off + 4] == b"fmt " {
                fmt = Some(off + 8);
            } else if &bytes[off..off + 4] == b"data" {
                data = Some((off + 8, size));
                break;
            }
            off += 8 + size + (size % 2);
        }
        let fmt = fmt.expect("wav fmt chunk");
        let (dstart, dsize) = data.expect("wav data chunk");
        assert_eq!(
            u16::from_le_bytes([bytes[fmt], bytes[fmt + 1]]),
            1,
            "{tag} pcm"
        );
        assert_eq!(
            u16::from_le_bytes([bytes[fmt + 2], bytes[fmt + 3]]),
            2,
            "{tag} stereo"
        );
        assert_eq!(
            u32::from_le_bytes([
                bytes[fmt + 4],
                bytes[fmt + 5],
                bytes[fmt + 6],
                bytes[fmt + 7]
            ]),
            48000,
            "{tag} rate"
        );
        assert_eq!(
            u16::from_le_bytes([bytes[fmt + 14], bytes[fmt + 15]]),
            16,
            "{tag} s16"
        );
        assert_eq!(dstart + dsize, bytes.len(), "{tag} length");
        // 1 920 000 data bytes = 480 000 samples/ch = 10.0 s.
        assert_eq!(dsize, 1_920_000, "{tag} data bytes");
        let samples = dsize / 4;
        assert_eq!(samples, 480_000, "{tag} samples/ch");
        // AAC frame count from the driver's documented flush law
        // (ceil((S + 1600) / 1024)): the design corpus value.
        let frames = (samples + 1600).div_ceil(1024);
        assert_eq!(frames, 471, "{tag} aac frames");
        // Fixture wiring resolves to the same PCM the encoder consumes.
        let pcm = testdata::fixture_samples(fix.fixture_key);
        assert_eq!(pcm.len(), 2, "{tag} channels");
        assert_eq!(pcm[0].len(), 480_000, "{tag} pcm len ch0");
        assert_eq!(pcm[1].len(), 480_000, "{tag} pcm len ch1");
        // Prefix proof: the driver consumes the wired material cleanly
        // (full-corpus 471-frame flights belong to Group C/F, not Group B).
        let mut enc = q31_encoder(48000, 2);
        for f in 0..8u32 {
            let fresh = fresh_frame(&pcm, f);
            let (emitted, snap) = enc.step_frame(f, &fresh);
            assert!(snap.notes.is_empty(), "{tag} f={f} notes");
            assert!(emitted.is_some(), "{tag} f={f} emits");
        }
    }
}
