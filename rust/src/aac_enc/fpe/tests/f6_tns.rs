//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! R2.0 F-6 short-window TNS exponent tests (test-only).
//!
//! Two strictly separated objectives (R2-A2 §1):
//!
//! * **T1 production reachability.** Real short-window corpus frames must
//!   satisfy `tns_shift == 0`. This establishes the observed production
//!   invariant with the correctly derived production exponent
//!   (`m0 = mdct_scale + finalShift` for short; no `+1`, no `tnsSpecShift`
//!   — `psy.rs:1165-1172` pre-halve is long-only, overflow
//!   `psy_main.cpp:895` skips short). It proves nothing about synthetic
//!   nonzero-TNS arithmetic.
//! * **T2 synthetic arithmetic.** A fixed, explicitly non-production state
//!   verifies the exponent reconstruction math: the original pre-rescale
//!   spectrum `x = [0.25, −0.25]` is supplied to `map_window`, the correct
//!   `m0 = mdct_scale + finalShift − tns_shift` reproduces the expected
//!   stored energy / maxScale / Ld, and the old `m = mdct_scale +
//!   finalShift` fails by exactly `4^s` in energy and `2s/64` in Ld.
//!   Constructed metadata only — production TNS logic is never forced.

use super::audit::{band_energy, fpe_stored_energy, independent_ld};
use super::envelope::{map_window, q31_stored_energy};

/// Fixed R2-A2 §4.2 example. Band of two lines, `offsets = [0, 2]`.
///
/// `m0 = 9` (short initial), `f = 2`, synthetic `s = 1`,
/// synthetic post-overflow `m3 = m0 − f + s = 8`.
const SYN_X: [f64; 2] = [0.25, -0.25];
const SYN_OFFSETS: [u16; 2] = [0, 2];
const SYN_M0: i32 = 9;
const SYN_F: i32 = 2;
const SYN_S: i32 = 1;
const SYN_M3: i32 = 8; // = SYN_M0 - SYN_F + SYN_S
const SYN_MAX_SCALE: i32 = 10;
const SYN_SCALE0: i32 = 6;
const SYN_E: i32 = 16384;
const SYN_LD: f64 = -17.0 / 64.0;

#[test]
fn production_short_windows_have_zero_tns_shift() {
    // Real corpus coverage: fixtures that exercise short windows
    // (transients + speech/music runs). Every short frame's element-global
    // `tns_shift` must be zero — the short path never sets it
    // (`psy.rs:1191-1200` returns `()`, long-only `1202-1213`).
    let names = [
        "onset-stereo",
        "stepdown-stereo",
        "click-mono",
        "impulse-mono",
        "edge-mono",
        "stress-stereo",
        "sine-stereo",
        "speech-clean-stereo",
        "music-stereo",
    ];
    let mut short_frames = 0usize;
    let mut nonzero = 0usize;
    let mut frames = 0usize;
    for name in names {
        let run = ShortScan::run(name);
        frames += run.frames;
        short_frames += run.short_frames;
        nonzero += run.nonzero;
    }
    assert!(
        short_frames > 0,
        "F-6 T1: no short frames exercised (fixtures changed?)"
    );
    assert_eq!(
        nonzero, 0,
        "F-6 T1: production short-window tns_shift != 0 observed ({nonzero}); R2 must triage"
    );
    println!("F-6 T1: {frames} frames, {short_frames} short, {nonzero} nonzero tns_shift");
}

/// Minimal short-window scanner: drives the production Q31 engine over a
/// fixture and records per-frame `(seq, tns_shift)`.
struct ShortScan {
    frames: usize,
    short_frames: usize,
    nonzero: usize,
}

impl ShortScan {
    fn run(name: &str) -> Self {
        use crate::aac_enc::block_switch::{BlockSwitch, WindowSequence};
        use crate::aac_enc::psy::{ChannelInput, PsyChannel, PsyScratch, PsyTables};
        use crate::aac_enc::testdata;
        use crate::aac_enc::transform::ChannelTransform;

        let fixture = testdata::fixture_samples(name);
        let channels = fixture.len();
        let sr = testdata::fixture_sample_rate(name);
        let (ch_br, tns_br) = if channels == 1 {
            (112000, 112000)
        } else {
            (96000, 192000)
        };
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
        let mut out = Self {
            frames: 0,
            short_frames: 0,
            nonzero: 0,
        };
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
            if channels == 2 {
                let (l, r) = switches.split_at_mut(1);
                BlockSwitch::synchronize_pair(&mut l[0], &mut r[0]);
            } else {
                switches[0].synchronize_mono();
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
            let (ol, _, _) = if channels == 2 {
                let rin = ChannelInput {
                    spectrum: &mut tail[0],
                    seq: seqs[1],
                    grouping: groupings[1],
                    is_lfe: false,
                };
                let (pl, pr) = ch_state.split_at_mut(1);
                crate::aac_enc::psy::process_pair_reuse(
                    &mut pl[0],
                    Some(&mut pr[0]),
                    lin,
                    Some(rin),
                    &tables,
                    &mut ws,
                )
            } else {
                let (ol, or_, ms) = crate::aac_enc::psy::process_pair_reuse(
                    &mut ch_state[0],
                    None,
                    lin,
                    None,
                    &tables,
                    &mut ws,
                );
                (ol, or_, ms)
            };
            out.frames += 1;
            if seqs[0] == WindowSequence::Short {
                out.short_frames += 1;
                if ol.tns_shift != 0 {
                    out.nonzero += 1;
                }
            }
        }
        assert_eq!(out.frames, n, "{name}: frame accounting");
        out
    }
}

#[test]
fn synthetic_exponent_old_fails_new_passes() {
    // Spectrum representation: the ORIGINAL pre-rescale spectrum is
    // supplied (as `ChannelEnergyCtx.x` is in the audit). Correct exponent
    // for it is the pre-rescale m0 = m3 + f − s = 8 + 2 − 1 = 9.
    let phys = band_energy(&SYN_X, &SYN_OFFSETS, 0);
    assert_eq!(phys, 0.125, "physical Σx²");

    // Corrected reconstruction.
    let mw = map_window(&SYN_X, &SYN_OFFSETS, 1, SYN_M3 + SYN_F - SYN_S);
    assert_eq!(mw.max_scale, vec![SYN_MAX_SCALE], "corrected maxScale");
    assert!(!mw.mapping_saturated, "no saturation");
    let e_new = q31_stored_energy(&mw.x_q31, &SYN_OFFSETS, 0, mw.max_scale[0], SYN_F);
    assert_eq!(e_new, SYN_E, "corrected stored energy");
    let ld_new = independent_ld(phys, SYN_M0, SYN_SCALE0, SYN_F);
    assert!(
        (ld_new - SYN_LD).abs() < 1e-15,
        "corrected Ld {ld_new} != expected {SYN_LD}"
    );
    let fpe_new = fpe_stored_energy(phys, SYN_M0, SYN_F);
    assert_eq!(fpe_new, SYN_E, "corrected FPE energy");

    // Old reconstruction m = m3 + f = 10 (missing −s).
    let m_old = SYN_M3 + SYN_F;
    let mw_old = map_window(&SYN_X, &SYN_OFFSETS, 1, m_old);
    let e_old = q31_stored_energy(&mw_old.x_q31, &SYN_OFFSETS, 0, mw_old.max_scale[0], SYN_F);
    assert_eq!(
        e_old,
        SYN_E / 4,
        "old reconstruction must be off by 4^s (4096 vs 16384)"
    );
    assert_eq!(
        SYN_E - e_old,
        12288,
        "old reconstruction factor-of-four discrepancy"
    );
    assert_eq!(
        mw_old.max_scale,
        vec![SYN_MAX_SCALE + 1],
        "old reconstruction shifts maxScale by s"
    );
    let ld_old = independent_ld(phys, m_old, (mw_old.max_scale[0] - 4).max(0), SYN_F);
    assert!(
        (ld_old - SYN_LD + 2.0 / 64.0).abs() < 1e-15,
        "old Ld must be off by −2s/64 (got {ld_old}, want {})",
        SYN_LD - 2.0 / 64.0
    );

    // Identity-style checks: old fails, new passes.
    assert_ne!(e_old, SYN_E, "old stored-energy identity must fail");
    assert_eq!(e_new, SYN_E, "new stored-energy identity must pass");
}
