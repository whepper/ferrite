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
//! success). At M1/M2 all comparisons are bit-identical (pass-through).

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
    let s = DiffStream::run("onset-stereo");
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
    let s = DiffStream::run("stepdown-stereo");
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
        let s = DiffStream::run(name);
        assert!(s.seen(WindowSequence::Long), "{name}: long frames");
    }
    // The click and edge fixtures are transient-rich: short frames must
    // occur (committed seq vectors: click f1, edge f3/f4).
    for name in ["click-mono", "edge-mono"] {
        let s = DiffStream::run(name);
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
        let s = DiffStream::run(name);
        assert!(s.seen(WindowSequence::Long), "{name}: long coverage");
    }
}

#[test]
fn silence_and_near_silence() {
    // All-zero elements (zeroSpec shortcut, -1.0 Ld placeholders) and
    // PCM-floor-dominated low-level content.
    for name in ["silence-stereo", "fpe-allzero-stereo"] {
        let s = DiffStream::run(name);
        assert!(s.seen(WindowSequence::Long), "{name}: long");
        assert!(!s.seen(WindowSequence::Short), "{name}: no attacks");
        assert_eq!(s.short_frames, 0, "{name}: all-long");
    }
    for name in ["quiet-sine-stereo", "fpe-near-silence-ramp-stereo"] {
        let s = DiffStream::run(name);
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
        let s = DiffStream::run(name);
        assert!(s.short_frames >= 1, "{name}: transient/short coverage");
    }
}

#[test]
fn pow2_boundary_energies() {
    // Power-of-two amplitude segments (2^0..2^14): band energies on/near
    // power-of-two boundaries pressure the integer clz headroom analysis
    // and rescale shift knife edges (representation lane must stay exact).
    let s = DiffStream::run("fpe-pow2-boundary-stereo");
    assert!(s.seen(WindowSequence::Long), "pow2: long coverage");
}

#[test]
fn mixed_silent_active_pair() {
    // Channel 0 onset at 0.5 s, channel 1 exactly silent throughout:
    // joint minSpecShift/finalShift/tnsSpecShift with an asymmetric pair,
    // one-sided MS energies, and transitions against a zero partner.
    let s = DiffStream::run("fpe-mixed-pair-stereo");
    assert!(s.seen(WindowSequence::Start), "mixed pair: START");
    assert!(s.seen(WindowSequence::Short), "mixed pair: SHORT");
    assert!(s.seen(WindowSequence::Stop), "mixed pair: STOP");
    assert!(s.stale_ms_reads >= 1, "mixed pair: stale-on-short MS-Ld");
}

#[test]
fn rate_44100_sine_and_onset() {
    // The second validated sample rate (different SFB widths).
    let s = DiffStream::run("sine-stereo-44100");
    assert!(s.seen(WindowSequence::Long), "sine-44100: long");
    let s = DiffStream::run("onset-stereo-44100");
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
        let s = DiffStream::run(name);
        assert!(s.seen(WindowSequence::Long), "{name}: long coverage");
    }
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
        assert_eq!(s.frames_compared, 1, "{name} f{frame}: frame accounting");
    }
}
