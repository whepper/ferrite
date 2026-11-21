//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! Q31↔FPE differential scenarios (roadmap M1 FPE-1.3, `docs/fpe/07`
//! §3.1/§5/§6): multi-frame sequences with per-frame complete-output and
//! persistent-state bit-identity, covering long/short transitions,
//! START/STOP, pre-echo enable/reset, and stale-on-short MS-Ld history,
//! plus the M1 edge-case fixtures and committed-vector single frames.
//!
//! Every scenario asserts exact frame accounting and that its intended
//! coverage paths actually executed (`docs/fpe/07` §7.1 — no silent
//! success). From M3 (FPE-3.1) exact fields remain bit-identical while the
//! float-energy value fields are checked by the FPE-3.1R1 term-separated
//! audit (`tests/audit.rs`): derived per-band envelope, collect-all failures,
//! energy-domain Ld comparison, sentinel classes and independent identity
//! gates.
//!
//! **Scope:** this is energy-value validation only. Downstream decision
//! safety (IS/MS/PE/ATS/S2/S4/S6/frame accounting) is **not** implemented
//! here; it remains required before FPE-3.1 acceptance and is FPE-3.1R2.

use super::*;

#[test]
fn fresh_fpe_state_init_matches_q31() {
    // R-08 init gate: FpeChannel::new mirrors PsyChannel::new exactly
    // (pre-echo seed/scale/enabled, TNS zeroed, MS-Ld zeros) before any
    // frame, at both validated rates.
    for sr in [48000u32, 44100] {
        let ((lcfg, _), _) = testdata::load_cfg(sr, 96000);
        let pcm = lcfg.pcm_quant.clone();
        let q31 = PsyChannel::new(&pcm);
        let fpe = FpeChannel::new(&pcm);
        assert_psy_state_eq(&q31, &fpe, &format!("init {sr}"));
        // Explicit init values (thresholds.rs PreEchoState::new).
        assert_eq!(q31.preecho_state(), (8, true), "{sr} prev_scale/enabled");
        assert_eq!(q31.preecho_prev()[..pcm.len()], pcm[..], "{sr} pcm seed");
    }
}

#[test]
fn onset_stereo_long_short_transitions_and_stale_ms_ld() {
    // Onset: silence -> full sine at 0.5 s. Exercises START (f23),
    // EIGHT_SHORT (f24, grouping [3,1,3,1]), STOP (f25), the pre-echo
    // STOP-before/START-after resets (disabled/memorize-only frames), and
    // the stale-on-short MS-Ld readout on f24.
    let s = DiffStream::run_checked("onset-stereo");
    assert!(s.seen(WindowSequence::Long), "onset: long frames");
    assert!(s.seen(WindowSequence::Start), "onset: START transition");
    assert!(s.seen(WindowSequence::Short), "onset: short frames");
    assert!(s.seen(WindowSequence::Stop), "onset: STOP transition");
    assert!(s.short_frames >= 1, "onset: short coverage");
    assert!(
        s.preecho_disabled_frames >= 1,
        "onset: pre-echo disable/reset coverage"
    );
    assert!(
        s.stale_ms_reads >= 1,
        "onset: stale-on-short MS-Ld coverage"
    );
}

#[test]
fn stepdown_stereo_offset_transition() {
    // Stepdown: full sine -> silence at 0.5 s. START f0, SHORT f1
    // (grouping [1,3,3,1]), STOP f2, then long decay into silence.
    let s = DiffStream::run_checked("stepdown-stereo");
    assert!(s.seen(WindowSequence::Start), "stepdown: START");
    assert!(s.seen(WindowSequence::Short), "stepdown: SHORT");
    assert!(s.seen(WindowSequence::Stop), "stepdown: STOP");
    assert!(s.stale_ms_reads >= 1, "stepdown: stale-on-short MS-Ld");
}

#[test]
fn mono_transients_click_impulse_edge() {
    // Click (full-scale click on sine), impulse (isolated full-scale
    // impulse), edge (late-frame impulse exercising the cross-border
    // attack repeat). Mono element path: no right channel, no MS output.
    for name in ["click-mono", "impulse-mono", "edge-mono"] {
        let s = DiffStream::run_checked(name);
        assert!(s.seen(WindowSequence::Long), "{name}: long frames");
    }
    // The click and edge fixtures are transient-rich: short frames must
    // occur (committed seq vectors: click f1, edge f3/f4).
    for name in ["click-mono", "edge-mono"] {
        let s = DiffStream::run_checked(name);
        assert!(s.short_frames >= 1, "{name}: short coverage");
    }
}

#[test]
fn sustained_tonal_and_noise() {
    // Long-window dominated corpus: sine, multitone, sweep, noise,
    // tonal-noise. (Initial-frame SHORT blips are shared front-end
    // decisions, identical for both engines by construction.)
    for name in [
        "sine-stereo",
        "multitone-stereo",
        "sweep-stereo",
        "noise-mono",
        "tonal-noise-stereo",
    ] {
        let s = DiffStream::run_checked(name);
        assert!(s.seen(WindowSequence::Long), "{name}: long coverage");
    }
}

#[test]
fn silence_and_near_silence() {
    // All-zero elements (zeroSpec shortcut, -1.0 Ld placeholders) and
    // PCM-floor-dominated low-level content.
    for name in ["silence-stereo", "fpe-allzero-stereo"] {
        let s = DiffStream::run_checked(name);
        assert!(s.seen(WindowSequence::Long), "{name}: long");
        assert!(!s.seen(WindowSequence::Short), "{name}: no attacks");
        assert_eq!(s.short_frames, 0, "{name}: all-long");
    }
    for name in ["quiet-sine-stereo", "fpe-near-silence-ramp-stereo"] {
        let s = DiffStream::run_checked(name);
        assert!(s.seen(WindowSequence::Long), "{name}: long coverage");
    }
}

#[test]
fn stress_and_full_scale_edges() {
    // High-level / max-headroom content: PRNG+tone stress, full-scale
    // square (polarity-inverted pair: zero mid, full side), and the
    // impulse-train + HF-tone max-headroom fixture. Pressures clip caps,
    // saturation, overflow rescale, and joint pair shift decisions.
    for name in [
        "stress-stereo",
        "fpe-fullscale-square-stereo",
        "fpe-max-headroom-stereo",
    ] {
        let s = DiffStream::run_checked(name);
        assert!(s.short_frames >= 1, "{name}: transient/short coverage");
    }
}

#[test]
fn pow2_boundary_energies() {
    // Power-of-two amplitude segments (2^0..2^14): band energies on/near
    // power-of-two boundaries pressure the integer clz headroom analysis
    // and rescale shift knife edges (representation lane must stay exact).
    let s = DiffStream::run_checked("fpe-pow2-boundary-stereo");
    assert!(s.seen(WindowSequence::Long), "pow2: long coverage");
}

#[test]
fn mixed_silent_active_pair() {
    // Channel 0 onset at 0.5 s, channel 1 exactly silent throughout:
    // joint minSpecShift/finalShift/tnsSpecShift with an asymmetric pair,
    // one-sided MS energies, and transitions against a zero partner.
    let s = DiffStream::run_checked("fpe-mixed-pair-stereo");
    assert!(s.seen(WindowSequence::Start), "mixed pair: START");
    assert!(s.seen(WindowSequence::Short), "mixed pair: SHORT");
    assert!(s.seen(WindowSequence::Stop), "mixed pair: STOP");
    assert!(s.stale_ms_reads >= 1, "mixed pair: stale-on-short MS-Ld");
}

#[test]
fn rate_44100_sine_and_onset() {
    // The second validated sample rate (different SFB widths).
    let s = DiffStream::run_checked("sine-stereo-44100");
    assert!(s.seen(WindowSequence::Long), "sine-44100: long");
    let s = DiffStream::run_checked("onset-stereo-44100");
    assert!(s.seen(WindowSequence::Start), "onset-44100: START");
    assert!(s.seen(WindowSequence::Short), "onset-44100: SHORT");
    assert!(s.seen(WindowSequence::Stop), "onset-44100: STOP");
    assert!(s.stale_ms_reads >= 1, "onset-44100: stale-on-short MS-Ld");
}

#[test]
fn real_music_and_speech() {
    // Licensed real recordings (10 s each, ~471 frames): long multi-frame
    // runs with live decaying histories, including the pinned
    // speech-clean f107 float-native block-switch knife edge (shared
    // front-end decision — identical for both engines by construction).
    for name in [
        "music-stereo",
        "speech-clean-stereo",
        "speech-pauses-stereo",
        "music-eroica-stereo",
    ] {
        let s = DiffStream::run_checked(name);
        assert!(s.seen(WindowSequence::Long), "{name}: long coverage");
    }
}

#[test]
fn fpe31r1_full_corpus_energy_audit() {
    // FPE-3.1R1 full-corpus evidence run: every differential fixture, with
    // the derived per-band envelope, collect-all failures, independent
    // Q31/FPE integrity checks, decomposition term bounds and sentinel
    // classification. Individual scenario tests enforce the same gate; this
    // test aggregates and reports the corpus-wide maxima, so the previously
    // hidden quiet bands are included.
    let mut total = EnergyAudit::default();
    let mut frames = 0usize;
    for name in [
        "silence-stereo",
        "fpe-allzero-stereo",
        "quiet-sine-stereo",
        "fpe-near-silence-ramp-stereo",
        "sine-stereo",
        "multitone-stereo",
        "sweep-stereo",
        "noise-mono",
        "tonal-noise-stereo",
        "onset-stereo",
        "stepdown-stereo",
        "click-mono",
        "impulse-mono",
        "edge-mono",
        "stress-stereo",
        "fpe-fullscale-square-stereo",
        "fpe-max-headroom-stereo",
        "fpe-pow2-boundary-stereo",
        "fpe-mixed-pair-stereo",
        "sine-stereo-44100",
        "onset-stereo-44100",
        "music-stereo",
        "speech-clean-stereo",
        "speech-pauses-stereo",
        "music-eroica-stereo",
    ] {
        let s = DiffStream::run(name);
        frames += s.frames_compared;
        total.merge(&s.audit);
    }
    assert_eq!(
        frames, 2662,
        "corpus: exact frame accounting (no silent omissions)"
    );
    println!("FPE-3.1R1 corpus frames compared: {frames}");
    total.print_summary("full differential corpus");
    total.assert_clean("full differential corpus");

    // Coverage floors: every vector class and every identity gate must have
    // been exercised, and the expected sentinel classes must be present.
    assert!(total.long_rescale.count > 0, "long rescale coverage");
    assert!(total.short_rescale.count > 0, "short rescale coverage");
    assert!(total.long_spread.count > 0, "long spread coverage");
    assert!(total.short_spread.count > 0, "short spread coverage");
    assert!(total.grouped.count > 0, "grouped coverage");
    assert!(total.identity.q31.checked > 0, "Q31 identity coverage");
    assert!(total.identity.fpe.checked > 0, "FPE identity coverage");
    assert_eq!(
        total.identity.q31.checked, 426_648,
        "Q31 integrity count (two distinct checks per band)"
    );
    assert_eq!(
        total.identity.fpe.checked, 213_324,
        "FPE integrity unique count (one independent energy recomputation per band)"
    );
    assert_eq!(
        total.short_tns_spec_shift_seen, 0,
        "F-6: corpus does not exercise short-TNS tnsSpecShift != 0; R2 must"
    );
    assert!(
        total.identity.fpe_ld.checked > 0,
        "FPE Ld identity coverage"
    );
    assert!(total.identity.decomp.checked > 0, "decomposition coverage");
    assert!(
        total.sentinels.long_a + total.sentinels.short_a > 0,
        "Class A exercised on the corpus"
    );
    assert!(
        total.sentinels.long_b + total.sentinels.short_b + total.sentinels.grouped_b > 0,
        "Class B exercised on the corpus"
    );
    // The known grouped sentinel divergence is reproduced by the reachable
    // 3-window short case (~0.540390); the full-corpus maximum must include
    // it rather than hide it in an aggregate.
    assert!(
        total.ld.grouped_energy.max_abs > 0.5,
        "grouped Class B Ld divergence present (max {})",
        total.ld.grouped_energy.max_abs
    );
}

/// Run one synthetic mono frame through both engines and the audit.
#[allow(clippy::needless_range_loop)]
fn run_mono_audited(
    spec: [f64; 1024],
    seq: WindowSequence,
    grouping: (u8, [u8; 4]),
) -> (ChannelOutput, ChannelOutput, EnergyAudit) {
    let ((lcfg, _), (scfg, _)) = testdata::load_cfg(48000, 112000);
    let tables = PsyTables::for_rate(
        48000,
        112000,
        112000,
        19293,
        lcfg.min_snr_ld,
        scfg.min_snr_ld,
    )
    .expect("mono tables");
    let mut q31 = PsyChannel::new(tables.long_cfg.pcm_quant_q31());
    let mut fpe = FpeChannel::new(tables.long_cfg.pcm_quant_q31());
    let mut ws_q = PsyScratch::default();
    let mut ws_f = FpeScratch::default();
    let mut spec_q = spec;
    let mut spec_f = spec;
    let (out_q, _, _) = psy::process_pair_reuse(
        &mut q31,
        None,
        ChannelInput {
            spectrum: &mut spec_q,
            seq,
            grouping,
            is_lfe: false,
        },
        None,
        &tables,
        &mut ws_q,
    );
    let (out_f, _, _) = crate::aac_enc::fpe::process_pair_reuse(
        &mut fpe,
        None,
        ChannelInput {
            spectrum: &mut spec_f,
            seq,
            grouping,
            is_lfe: false,
        },
        None,
        &tables,
        &mut ws_f,
    );
    let short = seq == WindowSequence::Short;
    let nw = if short { 8 } else { 1 };
    let mut tns_active = [false; 8];
    for w in 0..nw {
        tns_active[w] = q31.tns_state().active[w][0] || q31.tns_state().active[w][1];
    }
    let conf = if short {
        &tables.short_cfg
    } else {
        &tables.long_cfg
    };
    let mut audit = EnergyAudit::default();
    let ctx = ChannelEnergyCtx {
        tag: "synthetic".to_string(),
        ch: 0,
        short,
        active: conf.sfb_active(),
        grouping,
        x: &spec,
        tns_active,
        conf,
    };
    audit.compare_channel(&ctx, &out_f, &out_q);
    (out_q, out_f, audit)
}

/// Reachable Class A: a long 8-line band below the Q31 mapping grid with
/// `maxScale = 30`. All mapped lines are zero, Q31 stores zero, FPE resolves
/// the physical energy and Ld.
#[test]
fn fpe31r1_reachable_class_a_mapping_to_zero() {
    let conf_sr = 48000u32;
    let ((lcfg, _), (scfg, _)) = testdata::load_cfg(conf_sr, 112000);
    let tables = PsyTables::for_rate(
        conf_sr,
        112000,
        112000,
        19293,
        lcfg.min_snr_ld,
        scfg.min_snr_ld,
    )
    .expect("tables");
    let offsets = tables.long_cfg.offsets();
    // Long SFB 10 is 8 lines wide. Choose x just below the mapping grid:
    // x·2^19 rounds to 0, while 8 lines give a half-LSB FPE stored value.
    let b = 10usize;
    let x = 2f64.powi(-20) * (1.0 - 1e-6);
    let mut spec = [0.0f64; 1024];
    for v in spec[offsets[b] as usize..offsets[b + 1] as usize].iter_mut() {
        *v = x;
    }
    let (out_q, out_f, audit) = run_mono_audited(spec, WindowSequence::Long, (1, [1, 0, 0, 0]));
    audit.print_summary("reachable Class A");
    assert_eq!(out_q.energies[0][b], 0, "Q31 mapping-to-zero stored energy");
    assert!(
        out_f.energies[0][b] > 0,
        "FPE resolves the physical energy (got {})",
        out_f.energies[0][b]
    );
    assert!(audit.sentinels.long_a >= 1, "Class A counted");
    assert!(
        out_f.energy_ld[0][b].is_finite(),
        "FPE Ld resolves physical energy"
    );
    let f = out_q.final_shift;
    assert!(
        (out_q.energy_ld[0][b] - (-1.0 + f as f64 * 2.0 / 64.0)).abs() < 1e-15,
        "Q31 lifted sentinel"
    );
    audit.assert_clean("reachable Class A");
}

/// Reachable Class B: a short 16-line band with |X| = 2 (SFB 11). Q31's
/// re-shift floors to zero, FPE rounds to one; grouped with three windows it
/// reproduces the known ~0.540390 grouped Ld divergence.
#[test]
fn fpe31r1_reachable_class_b_grouped_and_isolated_16line() {
    let ((lcfg, _), (scfg, _)) = testdata::load_cfg(48000, 112000);
    let tables = PsyTables::for_rate(
        48000,
        112000,
        112000,
        19293,
        lcfg.min_snr_ld,
        scfg.min_snr_ld,
    )
    .expect("tables");
    let offsets = tables.short_cfg.offsets();
    // Short SFB 11 is 16 lines wide. X = 2 exactly; the rescale shift makes
    // Q31 store 0 while FPE's round-to-nearest stores 1.
    let b = 11usize;
    let x = 2.0 * 2f64.powi(9 - 31);
    // Flat but non-tonal sign pattern: same |X| energy, low TNS prediction
    // gain so the short TNS detector stays inactive and the FPE rescale
    // energy is actually observable.
    let signs = [
        1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0,
    ];
    let mut spec = [0.0f64; 1024];
    for w in 0..3 {
        for (j, v) in spec[w * 128 + offsets[b] as usize..w * 128 + offsets[b + 1] as usize]
            .iter_mut()
            .enumerate()
        {
            *v = x * signs[j];
        }
    }
    let (out_q, out_f, audit) = run_mono_audited(spec, WindowSequence::Short, (1, [8, 0, 0, 0]));
    audit.print_summary("reachable Class B");
    for w in 0..3 {
        assert_eq!(out_q.energies[w][b], 0, "Q31 post-shift truncation w{w}");
        assert_eq!(out_f.energies[w][b], 1, "FPE rounded stored energy w{w}");
    }
    assert_eq!(audit.sentinels.short_b, 3, "three per-window Class B");
    assert_eq!(audit.sentinels.grouped_b, 1, "one grouped Class B");
    let q31_g = out_q.grouped.as_ref().expect("grouped").energies[b];
    let fpe_g = out_f.grouped.as_ref().expect("grouped").energies[b];
    assert_eq!(q31_g, 0, "grouped Q31 truncates to zero");
    assert_eq!(fpe_g, 3, "grouped FPE sums three rounded ones");
    let q31_ld = out_q.grouped.as_ref().expect("grouped").energy_ld[b];
    let fpe_ld = out_f.grouped.as_ref().expect("grouped").energy_ld[b];
    let want_ld = (3f64.log2() - 31.0) / 64.0; // log2(3/2^31)/64
    assert!(
        (q31_ld - (-1.0)).abs() < 1e-15,
        "grouped Q31 sentinel is -1"
    );
    assert!(
        (fpe_ld - want_ld).abs() < 1e-12,
        "grouped FPE Ld {fpe_ld} want {want_ld}"
    );
    assert!(
        ((fpe_ld - q31_ld).abs() - 0.5403900390737681).abs() < 1e-9,
        "known grouped Class B Ld divergence reproduced (got {})",
        (fpe_ld - q31_ld).abs()
    );
    audit.assert_clean("reachable Class B");
}

/// The previously failing near-silence and power-of-two boundary fixtures
/// now pass the derived envelope and report their sentinel classes.
#[test]
fn fpe31r1_near_silence_and_pow2_boundary_cases() {
    let s = DiffStream::run_checked("fpe-near-silence-ramp-stereo");
    assert!(
        s.audit.sentinels.long_b + s.audit.sentinels.grouped_b > 0,
        "near-silence sentinel classes reported"
    );
    let s = DiffStream::run_checked("fpe-pow2-boundary-stereo");
    assert!(
        s.audit.long_rescale.count + s.audit.short_rescale.count > 0,
        "pow2 rescale coverage"
    );
}

#[test]
fn committed_mdct_vectors_single_frame() {
    // Feed the committed oracle MDCT vectors (exact C transform outputs,
    // read as int·2^(E−31)) directly to both engines with fresh state:
    // long stereo (sweep f10, both channels committed), short stereo
    // (onset f24, grouping [3,1,3,1]), and mono (click f11). Complements
    // the live multi-frame runs with the exact committed-frame spectra
    // (`docs/fpe/07` §3).
    let cases: [(&str, u32, usize); 3] = [
        ("sweep-stereo", 10, 2),
        ("onset-stereo", 24, 2),
        ("click-mono", 11, 1),
    ];
    for (name, frame, channels) in cases {
        let rows = testdata::load_seq(name);
        let row_of = |ch: u32| {
            rows.iter()
                .find(|r| r.frame == frame && r.channel == ch)
                .unwrap_or_else(|| panic!("{name} f{frame} ch{ch}: seq row"))
        };
        let seq_of = |v: u8| match v {
            0 => WindowSequence::Long,
            1 => WindowSequence::Start,
            2 => WindowSequence::Short,
            3 => WindowSequence::Stop,
            _ => panic!("unknown seq discriminant {v}"),
        };
        let mut specs = Vec::with_capacity(channels);
        let mut seqs = Vec::with_capacity(channels);
        let mut groupings = Vec::with_capacity(channels);
        for ch in 0..channels as u32 {
            let row = row_of(ch);
            let (e, coeffs) = testdata::load_mdct(name, frame, ch);
            let mut spec = [0.0f64; 1024];
            for (d, &q) in spec.iter_mut().zip(coeffs.iter()) {
                *d = testdata::q31_to_f64(q, e);
            }
            specs.push(spec);
            seqs.push(seq_of(row.seq));
            groupings.push((row.groups, row.group_len));
        }
        let mut s = DiffStream::new(name);
        s.step_inputs(specs, seqs, groupings);
        s.audit.assert_clean(&format!("{name} f{frame}"));
        assert_eq!(s.frames_compared, 1, "{name} f{frame}: frame accounting");
    }
}
