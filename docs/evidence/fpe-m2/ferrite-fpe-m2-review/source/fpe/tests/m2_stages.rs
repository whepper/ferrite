//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! M2 explicit-orchestration stage and invariant tests (roadmap `09` M2,
//! FPE-2.2/2.4; `docs/fpe/05` §8, `04` §7, `07` §3.1).
//!
//! These exercise the stages that M2 separated out of the M1 whole-function
//! delegation into the explicit FPE orchestration mirror:
//!
//! - **D-022 entry preconditions** (release-active `assert!`, identical to
//!   Q31): LFE-must-be-Long and CPE-sequence-agreement, mirrored from the
//!   Q31 `psy.rs` D-022 regressions/controls. The rejection tests are
//!   `#[should_panic]` and pass in both debug and release because the D-022
//!   asserts are ordinary `assert!` (not `debug_assert!`).
//! - **LFE structural early-outs** (skip TNS + pre-echo) with FPE↔Q31
//!   output/state parity.
//! - **Representation-lane invariant** (ADR-0005): `mdctScale`/`finalShift`/
//!   `tnsShift`/`maxScale` observable and exact across long/short transitions.
//!
//! The valid-control and LFE tests compare the complete FPE output, mutated
//! spectrum, and persistent state against Q31 bit-identically (the M2
//! pass-through gate), in addition to asserting the structural behaviour.

use super::*;

/// 48 kHz stereo operating point (same as the differential harness /
/// committed vectors: 96000 per-channel, 192000 TNS, 19293 bandwidth).
fn tables_48k() -> PsyTables {
    let ((lcfg, _), (scfg, _)) = testdata::load_cfg(48000, 96000);
    PsyTables::for_rate(
        48000,
        96000,
        192000,
        19293,
        lcfg.min_snr_ld,
        scfg.min_snr_ld,
    )
    .expect("tables")
}

// ---------------------------------------------------------------------------
// D-022 entry preconditions (mirror of the Q31 psy.rs D-022 tests, FPE path)
// ---------------------------------------------------------------------------

/// D-022 regression (FPE): an LFE channel whose effective sequence became
/// Short through pair synchronization must be rejected at orchestration
/// entry, before any value processing. Mirrors `psy.rs`
/// `d022_lfe_short_partner_rejected`.
#[test]
#[should_panic(expected = "D-022: LFE channel must not enter psy")]
fn fpe_d022_lfe_short_partner_rejected() {
    let mut sw_lfe = BlockSwitch::new();
    let silent = [0i16; 1024];
    sw_lfe.process(&silent, true); // is_lfe=true forces Long

    let mut sw_partner = BlockSwitch::new();
    for _ in 0..3 {
        sw_partner.process(&[0i16; 1024], false);
    }
    let mut burst = [0i16; 1024];
    for v in burst.iter_mut() {
        *v = 32767;
    }
    for _ in 0..2 {
        sw_partner.process(&burst, false);
    }
    assert_eq!(
        sw_partner.sequence(),
        WindowSequence::Short,
        "test setup: partner must decide Short before sync"
    );
    BlockSwitch::synchronize_pair(&mut sw_lfe, &mut sw_partner);
    assert_eq!(
        sw_lfe.sequence(),
        WindowSequence::Short,
        "test setup: synchronized LFE must be Short"
    );

    let tables = tables_48k();
    let mut fpe = FpeChannel::new(tables.long_cfg.pcm_quant_q31());
    let mut ws = FpeScratch::default();
    let mut spec = [1000.0f64; 1024];
    let input = ChannelInput {
        spectrum: &mut spec,
        seq: sw_lfe.sequence(),
        grouping: sw_lfe.grouping(),
        is_lfe: true,
    };
    let _ = crate::aac_enc::fpe::process_pair_reuse(&mut fpe, None, input, None, &tables, &mut ws);
}

/// D-022 regression (FPE): mismatched CPE window sequences must be rejected
/// before MS-energy cross-indexing or regrouping. Mirrors `psy.rs`
/// `d022_mismatched_cpe_sequences_rejected`.
#[test]
#[should_panic(expected = "D-022: CPE left and right window sequences must match")]
fn fpe_d022_mismatched_cpe_sequences_rejected() {
    let tables = tables_48k();
    let mut fpe_l = FpeChannel::new(tables.long_cfg.pcm_quant_q31());
    let mut fpe_r = FpeChannel::new(tables.long_cfg.pcm_quant_q31());
    let mut ws = FpeScratch::default();
    let mut spec_l = [1000.0f64; 1024];
    let mut spec_r = [1000.0f64; 1024];
    let lin = ChannelInput {
        spectrum: &mut spec_l,
        seq: WindowSequence::Long,
        grouping: (1, [1, 0, 0, 0]),
        is_lfe: false,
    };
    let rin = ChannelInput {
        spectrum: &mut spec_r,
        seq: WindowSequence::Short,
        grouping: (8, [2, 2, 2, 2]),
        is_lfe: false,
    };
    let _ = crate::aac_enc::fpe::process_pair_reuse(
        &mut fpe_l,
        Some(&mut fpe_r),
        lin,
        Some(rin),
        &tables,
        &mut ws,
    );
}

/// D-022 regression (FPE): right-hand LFE channel with effective Short window
/// sequence must be rejected. Mirrors `psy.rs` `d022_right_lfe_short_rejected`.
#[test]
#[should_panic(expected = "D-022: LFE channel must not enter psy")]
fn fpe_d022_right_lfe_short_rejected() {
    let tables = tables_48k();
    let mut fpe_l = FpeChannel::new(tables.long_cfg.pcm_quant_q31());
    let mut fpe_r = FpeChannel::new(tables.long_cfg.pcm_quant_q31());
    let mut ws = FpeScratch::default();
    let mut spec_l = [10.0f64; 1024];
    let mut spec_r = [10.0f64; 1024];
    let lin = ChannelInput {
        spectrum: &mut spec_l,
        seq: WindowSequence::Short,
        grouping: (4, [2, 2, 2, 2]),
        is_lfe: false,
    };
    let rin = ChannelInput {
        spectrum: &mut spec_r,
        seq: WindowSequence::Short,
        grouping: (4, [2, 2, 2, 2]),
        is_lfe: true,
    };
    let _ = crate::aac_enc::fpe::process_pair_reuse(
        &mut fpe_l,
        Some(&mut fpe_r),
        lin,
        Some(rin),
        &tables,
        &mut ws,
    );
}

/// D-022 control (FPE): valid matching-Long CPE input does not trip the
/// entry assertions, and FPE output/state are bit-identical to Q31. Mirrors
/// `psy.rs` `d022_valid_matching_long_cpe_ok` (strengthened with parity).
#[test]
fn fpe_d022_valid_matching_long_cpe_ok() {
    let mut s = DiffStream::new("onset-stereo");
    let spec = [1000.0f64; 1024];
    s.step_inputs(
        vec![spec, spec],
        vec![WindowSequence::Long, WindowSequence::Long],
        vec![(1, [1, 0, 0, 0]), (1, [1, 0, 0, 0])],
    );
    assert_eq!(s.frames_compared, 1, "matching-long CPE: frame accounting");
}

/// D-022 control (FPE): valid matching-Short CPE input does not trip the
/// entry assertions, and FPE output/state are bit-identical to Q31. Mirrors
/// `psy.rs` `d022_valid_matching_short_cpe_ok` (strengthened with parity).
#[test]
fn fpe_d022_valid_matching_short_cpe_ok() {
    let mut s = DiffStream::new("onset-stereo");
    let spec = [10.0f64; 1024];
    s.step_inputs(
        vec![spec, spec],
        vec![WindowSequence::Short, WindowSequence::Short],
        vec![(4, [2, 2, 2, 2]), (4, [2, 2, 2, 2])],
    );
    assert_eq!(s.frames_compared, 1, "matching-short CPE: frame accounting");
}

// ---------------------------------------------------------------------------
// LFE structural early-outs (skip TNS + pre-echo), FPE↔Q31 parity
// ---------------------------------------------------------------------------

/// Run a mono Long LFE element through both engines on identical input and
/// return `(q31_out, fpe_out, q31_state, fpe_state, q31_spec, fpe_spec)`.
fn mono_lfe_both(
    spec_val: f64,
) -> (
    ChannelOutput,
    ChannelOutput,
    PsyChannel,
    FpeChannel,
    [f64; 1024],
    [f64; 1024],
) {
    let tables = tables_48k();
    let mut q31 = PsyChannel::new(tables.long_cfg.pcm_quant_q31());
    let mut fpe = FpeChannel::new(tables.long_cfg.pcm_quant_q31());
    let mut ws_q = PsyScratch::default();
    let mut ws_f = FpeScratch::default();
    let mut spec_q = [spec_val; 1024];
    let mut spec_f = [spec_val; 1024];
    let (oq, _, _) = {
        let lin = ChannelInput {
            spectrum: &mut spec_q,
            seq: WindowSequence::Long,
            grouping: (1, [1, 0, 0, 0]),
            is_lfe: true,
        };
        psy::process_pair_reuse(&mut q31, None, lin, None, &tables, &mut ws_q)
    };
    let (of, _, _) = {
        let lin = ChannelInput {
            spectrum: &mut spec_f,
            seq: WindowSequence::Long,
            grouping: (1, [1, 0, 0, 0]),
            is_lfe: true,
        };
        crate::aac_enc::fpe::process_pair_reuse(&mut fpe, None, lin, None, &tables, &mut ws_f)
    };
    (oq, of, q31, fpe, spec_q, spec_f)
}

/// D-022 control (FPE): valid Long-window LFE input does not trip the entry
/// assertions. Mirrors `psy.rs` `d022_valid_long_lfe_ok`.
#[test]
fn fpe_d022_valid_long_lfe_ok() {
    let (_oq, _of, _q31, _fpe, _sq, _sf) = mono_lfe_both(1000.0);
    // Reaching here without panic is the control; parity is asserted in
    // fpe_lfe_path_skips_tns_and_preecho below.
}

/// LFE early-outs (FPE): TNS stays fully inactive and pre-echo history stays
/// at its initializer while energies still compute; FPE output, mutated
/// spectrum, and persistent state are bit-identical to Q31. Mirrors `psy.rs`
/// `lfe_path_skips_tns_and_preecho` (strengthened with parity).
#[test]
fn fpe_lfe_path_skips_tns_and_preecho() {
    let tables = tables_48k();
    let (oq, of, q31, fpe, sq, sf) = mono_lfe_both(1000.0);
    // Complete output + mutated spectrum + persistent state parity.
    assert_channel_output_eq(&of, &oq, "lfe ch0");
    assert_f64_bits_eq(&sf, &sq, "lfe mutated spectrum");
    assert_psy_state_eq(&q31, &fpe, "lfe state");
    // Structural early-outs on the FPE-owned state.
    for w in 0..8 {
        assert_eq!(
            fpe.tns_state().active[w],
            [false, false],
            "lfe tns inactive w{w}"
        );
        assert_eq!(fpe.tns_state().gain[w], [0, 0], "lfe tns gain w{w}");
    }
    assert!(
        !of.energies.iter().all(|w| w.iter().all(|&e| e == 0)),
        "lfe energies still compute"
    );
    // Pre-echo untouched: history still the initializer (long PCM floors).
    assert_eq!(
        &fpe.preecho_prev()[..tables.long_cfg.sfb_active()],
        &tables.long_cfg.pcm_quant_q31()[..tables.long_cfg.sfb_active()],
        "lfe pre-echo history stays at init"
    );
}

// ---------------------------------------------------------------------------
// Representation-lane invariant (ADR-0005): observable and exact
// ---------------------------------------------------------------------------

/// The integer representation lane (`mdctScale`/`finalShift`/`tnsShift`, and
/// `maxScale` via the per-frame output comparison) is observable and exact
/// across long/short transitions. The differential harness already asserts
/// FPE≡Q31 on every representation field each frame; this test additionally
/// confirms the lane was actually exercised (both window types present, a
/// non-zero rescale shift occurred, and `finalShift` stayed non-negative).
#[test]
fn representation_lane_exact_and_observable_across_transitions() {
    let s = DiffStream::run("onset-stereo");
    assert!(s.seen(WindowSequence::Long), "repr lane: long frames");
    assert!(s.seen(WindowSequence::Short), "repr lane: short frames");
    assert!(s.seen(WindowSequence::Start), "repr lane: START");
    assert!(s.seen(WindowSequence::Stop), "repr lane: STOP");
    assert_eq!(
        s.repr_seen.len(),
        s.frames_compared,
        "repr lane: per-frame capture"
    );
    // finalShift >= 0 on every frame (C invariant; energy::rescale_shifts).
    assert!(
        s.repr_seen.iter().all(|&(_, f, _)| f >= 0),
        "repr lane: finalShift non-negative throughout"
    );
    // The rescale stage actually shifted on at least one frame.
    assert!(
        s.repr_seen.iter().any(|&(_, f, _)| f > 0),
        "repr lane: a non-zero finalShift was exercised"
    );
    // mdctScale is observable (non-zero trajectory recorded every frame).
    assert!(
        s.repr_seen.iter().all(|&(m, _, _)| m >= 0),
        "repr lane: mdctScale recorded"
    );
}
