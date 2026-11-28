// SPDX-License-Identifier: FDK-AAC
//
// Part of Ferrite, a Third-Party Modified Version of the
// Fraunhofer FDK AAC Codec Library for Android.
// See rust/NOTICE and rust/LICENSE-FDK-AAC for the complete license text,
// modification notice, and date.
//! R2.2 Group D S4 step-3 operand and decision attribution (test-only).
//!
//! Resolves the S4 evidence blocker with narrowly scoped `#[cfg(test)]`
//! instrumentation inside `assimilate_multiple_scf2` (`scf.rs` step 3,
//! `scf.rs:1047-1108` region). The probe records the actual intermediate
//! operands and branch outcomes at their execution points; it never feeds
//! production or test encoder decisions and introduces no second S4
//! implementation.
//!
//! ## Exact S4 step-3 source (authoritative)
//!
//! Production path (`inv=2` only):
//! `estimate_scale_factors_channel_reuse` (`scf.rs`) →
//! `estimate_channel_with_inv_ws` → `assimilate_multiple_scf2`.
//!
//! ```text
//! gate: smax3 - smin3 <= MAX_SCF_DELTA - 3            // MAX = 60
//! for up to 3 iterations k = 0,1,2:
//!   nw = trial[start] - 1
//!   trial[b] = nw for b in [start,stop)
//!   d_scf = count_scf_bits_diff(scf,trial,cnt,start,stop)
//!   delta_new = delta_pe + d_scf                      // no d_spec
//!   PE gate: delta_new <= 0                            // sole inclusive
//!   if PE passes:
//!     for b in [start,stop), trial[b] != NO_SCF:
//!       (en_q, dn) = calc_sfb_quant_energy_and_dist(
//!                      spectrum[o..o+w], quant[o..o+w], nw)
//!       old_sum += calc_inv_ld_data(dist[b]) >> 3
//!       new_sum += calc_inv_ld_data(dn) >> 3
//!       reject if dn > dist[b] + D_1122
//!               or en_q < energy_ld[b] - D_07079
//!     accept iff new_sum < old_sum && ok
//!       => delta_pe = delta_new; scf[b] = nw; dist[b] = new_d[b]
//! deferred re-quantize of every moved band (decisions untouched)
//! ```
//!
//! Comparison semantics: dist/energy windows reject on strict `>`/`<`
//! (equality passes); PE gate `<= 0` passes (equality passes); sum gate
//! `<` accepts (equality rejects).
//!
//! ## Instrumentation
//!
//! `scf::step3_probe` (all `#[cfg(test)]`, production builds contain no
//! probe behaviour):
//! - `begin_call` at `estimate_channel_with_inv_ws` entry (per channel).
//! - Per region: entry `scf`/`dist`/region-`quant`/`delta_pe`, `smin3`,
//!   `smax3`, gate outcome (both pass and fail paths recorded).
//! - Per iteration: `nw`, post-set `trial`, `d_scf`, `delta_new`, PE margin
//!   and predicate; per evaluated band `en_q`/`dn`, previous `dist` and
//!   `energy_ld`, both limits and signed margins, both predicates, both
//!   partial-sum contributions and running sums; `ok`, sums, sum predicate,
//!   accept/reject, `delta_pe` before/after.
//! - Step 1 (`Step1Rec`): gate terms, `dmax`, `dn`, signed margin, reject and
//!   terminal predicates, PE gate, trial acceptance, delta chain.
//! - Step 2 (`Step2Rec`): gate conjuncts, minimum-scalefactor violations,
//!   PE gate, per-band threshold rejects/non-evaluated bands, sum limit and
//!   acceptance, delta chain.
//! - Final clamp (`ClampRec`): `gmin`, per-band limit, `scf` before, live and
//!   clamped predicates, kept distinct from steps 1/2/3.
//! - The pre-existing net-move witness (`Move`) is unchanged.
//!
//! The detailed records are opt-in (`set_detailed`) so the full-corpus
//! harnesses pay no capture cost. Entry snapshots use wrapping arithmetic
//! identical to production; nothing recorded is ever written back.
//!
//! ## Affected corpus
//!
//! The 3 digest-changing + 24 coding-changing frames are recovered from the
//! existing R2.1-B corpus evidence via `is_ms::r21b_affected_frames` (same
//! `R21BHarness` + `analyze_frame` derivation as the S1/S2 attribution
//! tests). No identity is invented; the supplemental 34-frame R22 set is
//! retained as supplemental coverage, not a substitute.

use std::collections::BTreeMap;

use crate::aac_enc::config::{EncoderConfig, PsyEngineKind, TransportType, VbrMode, FRAME_LENGTH};
use crate::aac_enc::fpe::tests::is_ms::{probe_narrow, probe_sine, r21b_affected_frames};
use crate::aac_enc::fpe::tests::r22_capture::{diff_captures, replay_frame};
use crate::aac_enc::fpe::tests::trace::{
    s4_stage_margins, step3_dist_margin, step3_energy_margin, step3_pe_gate_margin,
    step3_sum_margin,
};
use crate::aac_enc::q31::{calc_inv_ld_data, fmult};
use crate::aac_enc::quant::{calc_sfb_dist, calc_sfb_quant_energy_and_dist};
use crate::aac_enc::scf::step3_probe;
use crate::aac_enc::stream_driver::{R22Capture, StreamEncoder};
use crate::aac_enc::testdata;

// ---------------------------------------------------------------------------
// S4 step-3 classification (task §5 vocabulary)
// ---------------------------------------------------------------------------

/// One S4 step-3 observation class for a single (frame, channel, band) cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum S4Step3Class {
    /// Q31 and FPE took different intermediate step-3 branches.
    IntermediateFlip,
    /// Intermediate values differ but remain on the same side of every
    /// step-3 boundary.
    SameSideChange,
    /// Intermediate operands and branch outcomes agree exactly.
    IntermediateEquality,
    /// Region/band cannot execute step 3 by production topology or branch
    /// (dead band, gate `smax3-smin3 > 57` failed, no live trial band).
    Inapplicable,
    /// Domain the harness does not support (unsupported path hit).
    Unsupported,
    /// Decision-point operands not evidenced.
    Unavailable,
}

impl S4Step3Class {
    pub(super) fn label(self) -> &'static str {
        match self {
            S4Step3Class::IntermediateFlip => "IntermediateFlip",
            S4Step3Class::SameSideChange => "SameSideChange",
            S4Step3Class::IntermediateEquality => "IntermediateEquality",
            S4Step3Class::Inapplicable => "Inapplicable",
            S4Step3Class::Unsupported => "Unsupported",
            S4Step3Class::Unavailable => "Unavailable",
        }
    }
}

/// One attributed (frame, channel, band) observation from final S4 outputs.
#[derive(Debug, Clone)]
pub(super) struct S4Step3Obs {
    pub material: &'static str,
    pub frame: u32,
    pub channel: usize,
    pub band: usize,
    pub q_scf: i32,
    pub f_scf: i32,
    pub q_gain: i32,
    pub f_gain: i32,
    pub final_equal: bool,
    pub class: S4Step3Class,
    pub reason: &'static str,
}

/// Classify one band from the captured final S4 outputs.
///
/// Final equality must never be read as intermediate equivalence: every live
/// band is `Unavailable` at the final-output level (steps 1/2, clamp and
/// gain inversion also write `scf`). Probe-level attribution below replaces
/// this with evidenced classes where the probe fired.
pub(super) fn classify_s4_band(
    material: &'static str,
    frame: u32,
    channel: usize,
    band: usize,
    q_scf: i32,
    f_scf: i32,
    q_gain: i32,
    f_gain: i32,
) -> S4Step3Obs {
    const NO_SCF: i32 = i32::MIN;
    let final_equal = q_scf == f_scf && q_gain == f_gain;
    let (class, reason) = if q_scf == NO_SCF && f_scf == NO_SCF {
        (
            S4Step3Class::Inapplicable,
            "dead band on both arms; step 3 never visits it",
        )
    } else if final_equal {
        (
            S4Step3Class::Unavailable,
            "final scf/gain equal but step-3 intermediates not evidenced at final-output level",
        )
    } else {
        (
            S4Step3Class::Unavailable,
            "final scf differs but step-3 share unattributable from finals alone",
        )
    };
    S4Step3Obs {
        material,
        frame,
        channel,
        band,
        q_scf,
        f_scf,
        q_gain,
        f_gain,
        final_equal,
        class,
        reason,
    }
}

// ---------------------------------------------------------------------------
// Deterministic paired harness over explicit targets
// ---------------------------------------------------------------------------

/// Slice one 1024-sample frame per channel with zero padding past the end.
fn frame_slice(pcm: &[Vec<i16>], frame: u32) -> Vec<Vec<i16>> {
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

/// Explicit deterministic supplemental S4 target set (Group A coverage
/// windows plus Group D representative prefix). Supplement only: the
/// affected-corpus probe below is the prioritized evidence.
pub(super) const S4_STEP3_TARGETS: &[(&str, u32, u32)] = &[
    ("sine-stereo", 0, 8),
    ("onset-stereo", 18, 10),
    ("click-mono", 8, 8),
    ("impulse-mono", 2, 8),
];

/// Step one material on a persistent Q31/FPE encoder pair and classify every
/// band of every frame from the captured final S4 outputs.
pub(super) fn collect_s4_step3_obs(
    material: &'static str,
    first: u32,
    count: u32,
) -> Vec<S4Step3Obs> {
    let pcm = testdata::fixture_samples(material);
    let sr = testdata::fixture_sample_rate(material);
    let mut q = q31_encoder(sr, pcm.len());
    let mut f = fpe_encoder(sr, pcm.len());
    let mut out = Vec::new();
    for k in 0..count {
        let frame = first + k;
        let fresh = frame_slice(&pcm, frame);
        let (q_bytes, q_snap) = q.step_frame(frame, &fresh);
        let (f_bytes, f_snap) = f.step_frame(frame, &fresh);
        assert!(
            q_snap.notes.is_empty(),
            "{material} f{frame} Q31 notes {:?}",
            q_snap.notes
        );
        assert!(
            f_snap.notes.is_empty(),
            "{material} f{frame} FPE notes {:?}",
            f_snap.notes
        );
        assert!(
            q_bytes.is_some() && f_bytes.is_some(),
            "{material} f{frame}: both arms must emit"
        );
        let q_cap: &R22Capture = q_snap.r22_cap.as_ref().expect("Q31 R22 capture");
        let f_cap: &R22Capture = f_snap.r22_cap.as_ref().expect("FPE R22 capture");
        assert_eq!(
            q_cap.s4.ch.len(),
            f_cap.s4.ch.len(),
            "{material} f{frame}: channel pairing"
        );
        replay_frame(q_cap, &format!("s4-step3 Q31 {material} f{frame}"));
        replay_frame(f_cap, &format!("s4-step3 FPE {material} f{frame}"));
        let _ = diff_captures(q_cap, f_cap, &format!("s4-step3 {material} f{frame}"));
        for (ch, (qc, fc)) in q_cap.s4.ch.iter().zip(f_cap.s4.ch.iter()).enumerate() {
            assert_eq!(qc.cnt, fc.cnt, "{material} f{frame} ch{ch}: band count");
            for b in 0..qc.cnt {
                out.push(classify_s4_band(
                    material, frame, ch, b, qc.scf[b], fc.scf[b], qc.gain, fc.gain,
                ));
            }
        }
    }
    out
}

/// Minimum absolute final-`scf` delta among bands whose finals differ.
pub(super) fn min_abs_scf_delta(obs: &[S4Step3Obs]) -> Option<i64> {
    obs.iter()
        .filter(|o| o.q_scf != o.f_scf)
        .map(|o| (o.q_scf as i64 - o.f_scf as i64).abs())
        .min()
}

// ---------------------------------------------------------------------------
// R2.1-B affected-corpus PCM (mirrors is_ms::materials exactly)
// ---------------------------------------------------------------------------

/// Rebuild one R2.1-B corpus material's PCM by name. Formulae and caps are
/// identical to `is_ms::materials` (fixture resynthesis, silence run,
/// probe arguments); unknown names fail loudly rather than substituting.
fn r21b_pcm(name: &str) -> (Vec<Vec<i16>>, u32, u32) {
    let fixture_capped = |key: &str, cap: u32| {
        let pcm = testdata::fixture_samples(key);
        let sr = testdata::fixture_sample_rate(key);
        let full = (pcm[0].len().div_ceil(FRAME_LENGTH) + 2) as u32;
        (pcm, sr, full.min(cap))
    };
    match name {
        "sine-stereo" => fixture_capped("sine-stereo", 49),
        "onset-stereo" => fixture_capped("onset-stereo", 49),
        "stepdown-stereo" => fixture_capped("stepdown-stereo", 49),
        "multitone-stereo" => fixture_capped("multitone-stereo", 32),
        "tonal-noise-stereo" => fixture_capped("tonal-noise-stereo", 26),
        "quiet-sine-stereo" => fixture_capped("quiet-sine-stereo", 32),
        "sine-stereo-44100" => fixture_capped("sine-stereo-44100", 32),
        "onset-stereo-44100" => fixture_capped("onset-stereo-44100", 32),
        "silence-stereo" => {
            let pcm = testdata::fixture_samples("silence-stereo");
            (pcm, 48000, 8)
        }
        "probe-panned-48k" => (
            probe_sine(12, 48000.0, 1000.0, 10_000.0, 0.25, 0.0, Some(4)),
            48000,
            12,
        ),
        "probe-outphase-48k" => (
            probe_sine(
                12,
                48000.0,
                1000.0,
                10_000.0,
                0.5,
                std::f64::consts::PI,
                Some(4),
            ),
            48000,
            12,
        ),
        "probe-centred-48k" => (
            probe_sine(12, 48000.0, 1000.0, 10_000.0, 1.0, 0.0, Some(4)),
            48000,
            12,
        ),
        "probe-narrow-48k" => (probe_narrow(12, 48000.0, 0.25), 48000, 12),
        "probe-quiet-48k" => (
            probe_sine(12, 48000.0, 1000.0, 50.0, 0.25, 0.0, Some(4)),
            48000,
            12,
        ),
        "probe-panned-44100" => (
            probe_sine(12, 44100.0, 1000.0, 10_000.0, 0.25, 0.0, Some(4)),
            44100,
            12,
        ),
        _ => panic!("unknown R2.1-B material {name}"),
    }
}

// ---------------------------------------------------------------------------
// Step-3 probe harness over the affected corpus
// ---------------------------------------------------------------------------

/// One frame's paired step-3 probe evidence (Q31 + FPE, channel ordered).
pub(super) struct ProbeFrame {
    pub(super) material: String,
    pub(super) frame: u32,
    pub(super) q_calls: Vec<step3_probe::CallRec>,
    pub(super) f_calls: Vec<step3_probe::CallRec>,
    pub(super) q_cap: R22Capture,
    pub(super) f_cap: R22Capture,
}

/// Step every affected material with persistent Q31/FPE encoders from frame
/// 0 (production state evolution preserved) and capture the detailed step-3
/// probe plus the R22 snapshot at each target frame. Targets are grouped by
/// material in byte-lexicographic reporting order; encoder stepping is per
/// material from zero so histories match a continuous run.
pub(super) fn collect_probe_frames(targets: &[(String, u32)]) -> Vec<ProbeFrame> {
    let mut by_mat: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for (m, f) in targets {
        by_mat.entry(m.clone()).or_default().push(*f);
    }
    for v in by_mat.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    let was_detailed = step3_probe::is_detailed();
    let was_enabled = step3_probe::is_enabled();
    step3_probe::set_enabled(true);
    step3_probe::set_detailed(true);
    let mut out = Vec::new();
    for (material, frames) in &by_mat {
        let (pcm, sr, _) = r21b_pcm(material);
        let max = *frames.iter().max().expect("nonempty targets");
        let mut q = q31_encoder(sr, pcm.len());
        let mut f = fpe_encoder(sr, pcm.len());
        for frame in 0..=max {
            let fresh = frame_slice(&pcm, frame);
            let is_target = frames.contains(&frame);
            if !is_target {
                step3_probe::set_detailed(false);
            }
            step3_probe::clear();
            let (q_bytes, q_snap) = q.step_frame(frame, &fresh);
            let q_calls = step3_probe::take_calls();
            let _ = step3_probe::take();
            step3_probe::clear();
            let (f_bytes, f_snap) = f.step_frame(frame, &fresh);
            let f_calls = step3_probe::take_calls();
            let _ = step3_probe::take();
            if !is_target {
                step3_probe::set_detailed(true);
                continue;
            }
            assert!(
                q_snap.notes.is_empty(),
                "{material} f{frame} Q31 notes {:?}",
                q_snap.notes
            );
            assert!(
                f_snap.notes.is_empty(),
                "{material} f{frame} FPE notes {:?}",
                f_snap.notes
            );
            assert!(
                q_bytes.is_some() && f_bytes.is_some(),
                "{material} f{frame}: both arms must emit"
            );
            let q_cap = q_snap.r22_cap.clone().expect("Q31 R22 capture");
            let f_cap = f_snap.r22_cap.clone().expect("FPE R22 capture");
            assert_eq!(
                q_cap.s4.ch.len(),
                f_cap.s4.ch.len(),
                "{material} f{frame}: channel pairing"
            );
            assert_eq!(
                q_calls.len(),
                q_cap.s4.ch.len(),
                "{material} f{frame}: Q31 probe call count"
            );
            assert_eq!(
                f_calls.len(),
                f_cap.s4.ch.len(),
                "{material} f{frame}: FPE probe call count"
            );
            replay_frame(&q_cap, &format!("s4-probe Q31 {material} f{frame}"));
            replay_frame(&f_cap, &format!("s4-probe FPE {material} f{frame}"));
            out.push(ProbeFrame {
                material: material.clone(),
                frame,
                q_calls,
                f_calls,
                q_cap,
                f_cap,
            });
        }
    }
    step3_probe::set_detailed(was_detailed);
    step3_probe::set_enabled(was_enabled);
    out.sort_by(|a, b| (a.material.clone(), a.frame).cmp(&(b.material.clone(), b.frame)));
    out
}

/// Independently verify one frame's recorded step-3 calculations.
///
/// Checks, from recorded operands only (no second S4 implementation):
/// call/region/iteration counts and delta chains; `trial_after`
/// reconstruction; `delta_new` wrapping sum; PE margin and predicate;
/// per-band limits, margins (cross-checked with the `trace` pure edge
/// functions), predicates, partial-sum contributions (recomputed with the
/// production `calc_inv_ld_data`) and running-sum chains; `en_q`/`dn`
/// independent replay with the production `calc_sfb_quant_energy_and_dist`
/// on the R22 spectrum and entry-quant slices; `ok`, sum predicate and
/// acceptance consistency; region `scf_after`/`dist_after` reconstruction.
pub(super) fn verify_probe_frame(pf: &ProbeFrame) {
    assert_eq!(pf.q_calls.len(), pf.q_cap.s4.ch.len());
    assert_eq!(pf.f_calls.len(), pf.f_cap.s4.ch.len());
    for (arm, calls, cap) in [
        ("Q31", &pf.q_calls, &pf.q_cap),
        ("FPE", &pf.f_calls, &pf.f_cap),
    ] {
        for (ch, (call, s4ch)) in calls.iter().zip(cap.s4.ch.iter()).enumerate() {
            let tag = format!("{} {} f{} ch{ch}", arm, pf.material, pf.frame);
            assert_eq!(call.inv_quant, 2, "{tag}: production inv path");
            assert_eq!(call.cnt, s4ch.cnt, "{tag}: band count");
            for region in &call.regions {
                assert!(region.start < region.stop, "{tag}: region span");
                assert!(region.stop <= call.cnt, "{tag}: region bound");
                assert_eq!(region.entry_scf.len(), call.cnt, "{tag}: entry scf span");
                assert_eq!(region.entry_dist.len(), call.cnt, "{tag}: entry dist span");
                let gate_expect = region.smax3.wrapping_sub(region.smin3) <= 60 - 3;
                assert_eq!(region.gate_pass, gate_expect, "{tag}: gate predicate");
                if !region.gate_pass {
                    assert!(
                        region.iterations.is_empty(),
                        "{tag}: gated region runs no iteration"
                    );
                    assert_eq!(region.delta_pe_after, region.delta_pe_before);
                    continue;
                }
                assert_eq!(region.iterations.len(), 3, "{tag}: three iterations");
                for (ki, iter) in region.iterations.iter().enumerate() {
                    assert_eq!(iter.k, ki, "{tag}: iteration order");
                    assert_eq!(
                        iter.delta_new,
                        iter.delta_pe_before.wrapping_add(iter.d_scf),
                        "{tag}: delta_new wrapping sum"
                    );
                    assert_eq!(iter.pe_margin, -(iter.delta_new as i64), "{tag}: PE margin");
                    assert_eq!(
                        iter.pe_margin,
                        step3_pe_gate_margin(iter.delta_new),
                        "{tag}: PE margin matches pure edge"
                    );
                    assert_eq!(iter.pe_pass, iter.delta_new <= 0, "{tag}: PE predicate");
                    // Trial reconstruction: live bands read nw, dead bands
                    // ride along from the previous trial (entry for k=0).
                    let prev_trial: Vec<i32> = if ki == 0 {
                        region.entry_scf.clone()
                    } else {
                        region.iterations[ki - 1].trial_after.clone()
                    };
                    assert_eq!(iter.trial_after.len(), call.cnt);
                    for b in 0..call.cnt {
                        let in_region = b >= region.start && b < region.stop;
                        let live_trial = in_region && prev_trial[b] != i32::MIN;
                        let expect = if live_trial { iter.nw } else { prev_trial[b] };
                        assert_eq!(
                            iter.trial_after[b], expect,
                            "{tag}: trial reconstruction b{b}"
                        );
                    }
                    if !iter.pe_pass {
                        assert!(iter.bands.is_empty());
                        assert!(iter.ok);
                        assert!(!iter.sum_accept && !iter.accepted);
                        assert_eq!(iter.delta_pe_after, iter.delta_pe_before);
                        continue;
                    }
                    // Band-level verification against production primitives.
                    let mut running_old = 0i32;
                    let mut running_new = 0i32;
                    let mut saw_reject = false;
                    // Live dist threading: earlier accepted iterations in this
                    // region already rewrote dist for these bands.
                    let mut live_dist = region.entry_dist.clone();
                    for prev in region.iterations.iter().take(ki) {
                        if prev.accepted {
                            for be_prev in &prev.bands {
                                live_dist[be_prev.band] = be_prev.dn;
                            }
                        }
                    }
                    for be in &iter.bands {
                        assert!(
                            be.band >= region.start && be.band < region.stop,
                            "{tag}: band span"
                        );
                        assert_eq!(be.nw, iter.nw, "{tag}: band nw");
                        let w = (s4ch.offsets[be.band + 1] - s4ch.offsets[be.band]) as usize;
                        let o = s4ch.offsets[be.band] as usize;
                        let qb = region.quant_base;
                        let base_o = o - qb;
                        let qslice = &region.entry_quant_region[base_o..base_o + w];
                        let spec = &s4ch.spectrum_pre[o..o + w];
                        let spec_i: Vec<i32> = spec.to_vec();
                        let (re_en, re_dn) = calc_sfb_quant_energy_and_dist(&spec_i, qslice, be.nw);
                        assert_eq!(re_en, be.en_q, "{tag}: en_q replay b{}", be.band);
                        assert_eq!(re_dn, be.dn, "{tag}: dn replay b{}", be.band);
                        assert_eq!(
                            be.dist_prev, live_dist[be.band],
                            "{tag}: dist threading b{}",
                            be.band
                        );
                        let dist_limit = be.dist_prev.wrapping_add(5572475);
                        let energy_limit = be.energy_ld.wrapping_sub(16722942);
                        assert_eq!(dist_limit, be.dist_limit, "{tag}: dist limit");
                        assert_eq!(energy_limit, be.energy_limit, "{tag}: energy limit");
                        assert_eq!(
                            be.dist_margin,
                            step3_dist_margin(be.dn, be.dist_prev),
                            "{tag}: dist margin pure edge"
                        );
                        assert_eq!(
                            be.energy_margin,
                            step3_energy_margin(be.en_q, be.energy_ld),
                            "{tag}: energy margin pure edge"
                        );
                        assert_eq!(be.dist_reject, be.dn > be.dist_limit);
                        assert_eq!(be.energy_reject, be.en_q < be.energy_limit);
                        let old_c = calc_inv_ld_data(be.dist_prev) >> 3;
                        let new_c = calc_inv_ld_data(be.dn) >> 3;
                        assert_eq!(old_c, be.old_contrib, "{tag}: old contrib");
                        assert_eq!(new_c, be.new_contrib, "{tag}: new contrib");
                        running_old = running_old.wrapping_add(old_c);
                        running_new = running_new.wrapping_add(new_c);
                        assert_eq!(running_old, be.old_sum_after, "{tag}: old chain");
                        assert_eq!(running_new, be.new_sum_after, "{tag}: new chain");
                        if be.dist_reject || be.energy_reject {
                            saw_reject = true;
                        }
                    }
                    assert_eq!(iter.ok, !saw_reject, "{tag}: ok predicate");
                    assert_eq!(iter.old_sum, running_old, "{tag}: old total");
                    assert_eq!(iter.new_sum, running_new, "{tag}: new total");
                    assert_eq!(
                        step3_sum_margin(iter.new_sum, iter.old_sum),
                        iter.old_sum as i64 - iter.new_sum as i64
                    );
                    assert_eq!(
                        iter.sum_accept,
                        iter.new_sum < iter.old_sum,
                        "{tag}: sum predicate"
                    );
                    assert_eq!(
                        iter.accepted,
                        iter.pe_pass && iter.ok && iter.sum_accept,
                        "{tag}: acceptance"
                    );
                    if iter.accepted {
                        assert_eq!(iter.delta_pe_after, iter.delta_new);
                    } else {
                        assert_eq!(iter.delta_pe_after, iter.delta_pe_before);
                    }
                }
                // Delta chain across iterations and region exit state.
                let mut expect_delta = region.delta_pe_before;
                for iter in &region.iterations {
                    assert_eq!(iter.delta_pe_before, expect_delta);
                    expect_delta = iter.delta_pe_after;
                }
                assert_eq!(region.delta_pe_after, expect_delta);
                let mut expect_scf = region.entry_scf.clone();
                let mut expect_dist = region.entry_dist.clone();
                for iter in &region.iterations {
                    if iter.accepted {
                        for b in region.start..region.stop {
                            if expect_scf[b] != i32::MIN {
                                expect_scf[b] = iter.nw;
                                let be = iter
                                    .bands
                                    .iter()
                                    .find(|e| e.band == b)
                                    .expect("accepted band evaluated");
                                expect_dist[b] = be.dn;
                            }
                        }
                    }
                }
                assert_eq!(region.scf_after, expect_scf, "{tag}: scf exit");
                assert_eq!(region.dist_after, expect_dist, "{tag}: dist exit");
            }
            // Step 1: exact delta/margin and trial-dn replay.
            for rec in &call.step1 {
                assert_eq!(
                    rec.delta_new,
                    rec.delta_pe_before.wrapping_add(rec.d_scf).wrapping_add(rec.d_spec),
                    "{tag}: step1 delta_new"
                );
                assert_eq!(rec.pe_pass, rec.delta_new < 0, "{tag}: step1 PE gate");
                if rec.pe_pass {
                    let mut all_ok = true;
                    for be in &rec.bands {
                        assert!(
                            be.band >= rec.start && be.band < rec.stop,
                            "{tag}: step1 band span"
                        );
                        let offs = s4ch.offsets[be.band] as usize;
                        let width = (s4ch.offsets[be.band + 1] - s4ch.offsets[be.band]) as usize;
                        let mut tmp = vec![0i16; width];
                        let replay_dn = calc_sfb_dist(
                            &s4ch.spectrum_pre[offs..offs + width],
                            rec.nw,
                            false,
                            &mut tmp,
                        );
                        assert_eq!(replay_dn, be.dn, "{tag}: step1 dn replay b{}", be.band);
                        assert_eq!(
                            be.dmax_threshold_term,
                            fmult(715_827_904, be.threshold_ld),
                            "{tag}: step1 dmax threshold term b{}",
                            be.band
                        );
                        assert_eq!(
                            be.dmax_dist_terms,
                            fmult(715_827_904, be.dmax_dist_base)
                                .wrapping_add(fmult(715_827_904, be.dmax_dist_base)),
                            "{tag}: step1 dmax dist terms b{}",
                            be.band
                        );
                        assert_eq!(
                            be.dmax_energy_floor,
                            be.energy_ld.wrapping_sub(334_396_231),
                            "{tag}: step1 dmax energy floor b{}",
                            be.band
                        );
                        let expect_dmax = be
                            .dmax_threshold_term
                            .wrapping_add(be.dmax_dist_terms)
                            .max(be.dmax_energy_floor)
                            .min(be.threshold_ld);
                        assert_eq!(be.dmax, expect_dmax, "{tag}: step1 dmax b{}", be.band);
                        assert_eq!(
                            be.reject,
                            be.dn > be.dmax,
                            "{tag}: step1 reject b{}",
                            be.band
                        );
                        assert_eq!(
                            be.terminal,
                            be.reject && be.dn == be.energy_ld,
                            "{tag}: step1 terminal b{}",
                            be.band
                        );
                        all_ok &= !be.reject;
                    }
                    assert_eq!(rec.accepted, all_ok, "{tag}: step1 accepted");
                } else {
                    assert!(rec.bands.is_empty(), "{tag}: step1 PE fail no bands");
                    assert!(!rec.accepted, "{tag}: step1 PE fail not accepted");
                }
                let expect_after = if rec.accepted {
                    rec.delta_new
                } else {
                    rec.delta_pe_before
                };
                assert_eq!(
                    rec.delta_pe_after, expect_after,
                    "{tag}: step1 delta_pe chain"
                );
            }
            // Step 2: exact gate, minimum, PE, band and sum semantics.
            for rec in &call.step2 {
                assert_eq!(
                    rec.attempted,
                    rec.gate_act_eq && rec.gate_lo_lt_act && rec.spread_margin >= 0,
                    "{tag}: step2 gate conjunction"
                );
                if rec.attempted {
                    assert_eq!(
                        rec.min_scf_violation,
                        rec.bands.iter().any(|b| b.violation),
                        "{tag}: step2 violation aggregate"
                    );
                    for be in &rec.bands {
                        assert_eq!(
                            be.violation,
                            rec.nw < be.min_scf,
                            "{tag}: step2 min_scf b{}",
                            be.band
                        );
                    }
                    if !rec.min_scf_violation {
                        let delta_new = rec.delta_new.expect("non-violating delta");
                        assert_eq!(
                            delta_new,
                            rec.delta_pe_before.wrapping_add(rec.d_scf).wrapping_add(rec.d_spec),
                            "{tag}: step2 delta_new"
                        );
                        assert_eq!(rec.pe_pass, delta_new < 0, "{tag}: step2 PE gate");
                        if rec.pe_pass {
                            let mut replay_old = 0i32;
                            let mut replay_new = 0i32;
                            let mut broke = false;
                            for be in &rec.bands {
                                if !be.evaluated {
                                    continue;
                                }
                                let offs = s4ch.offsets[be.band] as usize;
                                let width =
                                    (s4ch.offsets[be.band + 1] - s4ch.offsets[be.band]) as usize;
                                let mut tmp = vec![0i16; width];
                                let replay_dn = calc_sfb_dist(
                                    &s4ch.spectrum_pre[offs..offs + width],
                                    rec.nw,
                                    false,
                                    &mut tmp,
                                );
                                assert_eq!(
                                    replay_dn, be.dn,
                                    "{tag}: step2 dn replay b{}",
                                    be.band
                                );
                                assert_eq!(
                                    be.threshold_reject,
                                    be.dn > be.threshold_ld,
                                    "{tag}: step2 threshold reject b{}",
                                    be.band
                                );
                                let old_c = calc_inv_ld_data(be.dist_before) >> 3;
                                assert_eq!(be.old_contrib, old_c, "{tag}: step2 old contrib");
                                if be.threshold_reject {
                                    replay_old = replay_old.wrapping_add(old_c);
                                    replay_new = replay_old.wrapping_shl(1);
                                    broke = true;
                                    break;
                                }
                                replay_old = replay_old.wrapping_add(old_c);
                                replay_new = replay_new.wrapping_add(
                                    calc_inv_ld_data(be.dn) >> 3,
                                );
                            }
                            assert_eq!(rec.old_sum, replay_old, "{tag}: step2 old sum");
                            assert_eq!(rec.new_sum, replay_new, "{tag}: step2 new sum");
                            assert_eq!(
                                rec.sum_limit,
                                fmult(1_717_986_944, rec.old_sum),
                                "{tag}: step2 sum limit"
                            );
                            assert_eq!(
                                rec.sum_accept,
                                !broke && rec.new_sum < rec.sum_limit,
                                "{tag}: step2 sum predicate"
                            );
                            assert_eq!(
                                rec.accepted, rec.sum_accept,
                                "{tag}: step2 acceptance"
                            );
                        } else {
                            assert!(rec.bands.iter().all(|b| !b.evaluated));
                            assert!(!rec.accepted);
                        }
                    } else {
                        assert!(rec.delta_new.is_none());
                        assert!(!rec.pe_pass && !rec.accepted);
                    }
                } else {
                    assert!(rec.bands.is_empty());
                    assert!(rec.delta_new.is_none());
                    assert!(!rec.pe_pass && !rec.accepted);
                }
                let expect_after = if rec.accepted {
                    rec.delta_new.expect("accepted step2 delta")
                } else {
                    rec.delta_pe_before
                };
                assert_eq!(
                    rec.delta_pe_after, expect_after,
                    "{tag}: step2 delta_pe chain"
                );
            }
            // Final clamp: exact limit, applicability and predicate.
            if let Some(clamp) = &call.clamp {
                for be in &clamp.bands {
                    assert_eq!(
                        be.limit,
                        clamp.gmin.wrapping_add(crate::aac_enc::scf::MAX_SCF_DELTA),
                        "{tag}: clamp limit b{}",
                        be.band
                    );
                    assert_eq!(
                        be.live,
                        be.scf_before != crate::aac_enc::scf::NO_SCF,
                        "{tag}: clamp live b{}",
                        be.band
                    );
                    assert_eq!(
                        be.clamped,
                        be.live && be.limit < be.scf_before,
                        "{tag}: clamp predicate b{}",
                        be.band
                    );
                }
            } else {
                panic!("{tag}: final clamp record missing");
            }
            // Final gain selection + inversion: exact branch and per-band
            // transformation.
            let gain = call
                .final_gain
                .as_ref()
                .expect("final gain record missing");
            assert_eq!(
                gain.all_dead,
                gain.gmax == crate::aac_enc::scf::NO_SCF,
                "{tag}: all-dead branch"
            );
            assert_eq!(
                gain.gain,
                if gain.all_dead { 0 } else { gain.gmax },
                "{tag}: gain selection"
            );
            for be in &gain.bands {
                assert_eq!(
                    be.dead,
                    be.scf_before == crate::aac_enc::scf::NO_SCF,
                    "{tag}: dead band b{}",
                    be.band
                );
                assert_eq!(
                    be.scf_after,
                    if be.dead {
                        0
                    } else {
                        gain.gmax.wrapping_sub(be.scf_before)
                    },
                    "{tag}: gain inversion b{}",
                    be.band
                );
            }
        }
    }
}

/// Per-band probe evaluation record for cross-arm comparison.
#[derive(Debug, Clone)]
struct BandEvalCtx {
    region_start: usize,
    region_stop: usize,
    k: usize,
    dist_margin: i64,
    energy_margin: i64,
    pe_margin: i64,
    sum_margin: i64,
    pe_pass: bool,
    dist_reject: bool,
    energy_reject: bool,
    ok: bool,
    sum_accept: bool,
    accepted: bool,
    en_q: i32,
    dn: i32,
    d_scf: i32,
    delta_new: i32,
    old_sum: i32,
    new_sum: i32,
}

/// Collect every evaluation of one band on one arm, in probe order.
fn band_evals(calls: &[step3_probe::CallRec], ch: usize, band: usize) -> Vec<BandEvalCtx> {
    let mut out = Vec::new();
    let call = &calls[ch];
    for region in &call.regions {
        if !region.gate_pass {
            continue;
        }
        for iter in &region.iterations {
            if !iter.pe_pass {
                // PE-fail still decides the band (no trial evaluation).
                if band >= region.start && band < region.stop {
                    out.push(BandEvalCtx {
                        region_start: region.start,
                        region_stop: region.stop,
                        k: iter.k,
                        dist_margin: 0,
                        energy_margin: 0,
                        pe_margin: iter.pe_margin,
                        sum_margin: 0,
                        pe_pass: false,
                        dist_reject: false,
                        energy_reject: false,
                        ok: true,
                        sum_accept: false,
                        accepted: false,
                        en_q: 0,
                        dn: 0,
                        d_scf: iter.d_scf,
                        delta_new: iter.delta_new,
                        old_sum: 0,
                        new_sum: 0,
                    });
                }
                continue;
            }
            for be in &iter.bands {
                if be.band != band {
                    continue;
                }
                out.push(BandEvalCtx {
                    region_start: region.start,
                    region_stop: region.stop,
                    k: iter.k,
                    dist_margin: be.dist_margin,
                    energy_margin: be.energy_margin,
                    pe_margin: iter.pe_margin,
                    sum_margin: iter.old_sum as i64 - iter.new_sum as i64,
                    pe_pass: true,
                    dist_reject: be.dist_reject,
                    energy_reject: be.energy_reject,
                    ok: iter.ok,
                    sum_accept: iter.sum_accept,
                    accepted: iter.accepted,
                    en_q: be.en_q,
                    dn: be.dn,
                    d_scf: iter.d_scf,
                    delta_new: iter.delta_new,
                    old_sum: iter.old_sum,
                    new_sum: iter.new_sum,
                });
            }
        }
    }
    out
}

/// Classify one band from its paired Q31/FPE probe evaluation sequences.
fn classify_probe_band(q: &[BandEvalCtx], f: &[BandEvalCtx]) -> S4Step3Class {
    if q.is_empty() && f.is_empty() {
        return S4Step3Class::Inapplicable;
    }
    if q.len() != f.len() {
        return S4Step3Class::IntermediateFlip;
    }
    let mut same_side = false;
    for (a, b) in q.iter().zip(f.iter()) {
        if a.region_start != b.region_start || a.region_stop != b.region_stop || a.k != b.k {
            return S4Step3Class::IntermediateFlip;
        }
        if a.pe_pass != b.pe_pass
            || a.dist_reject != b.dist_reject
            || a.energy_reject != b.energy_reject
            || a.ok != b.ok
            || a.sum_accept != b.sum_accept
            || a.accepted != b.accepted
        {
            return S4Step3Class::IntermediateFlip;
        }
        if a.en_q != b.en_q
            || a.dn != b.dn
            || a.d_scf != b.d_scf
            || a.delta_new != b.delta_new
            || a.old_sum != b.old_sum
            || a.new_sum != b.new_sum
            || a.dist_margin != b.dist_margin
            || a.energy_margin != b.energy_margin
            || a.pe_margin != b.pe_margin
        {
            same_side = true;
        }
    }
    if same_side {
        S4Step3Class::SameSideChange
    } else {
        S4Step3Class::IntermediateEquality
    }
}

/// Attributed probe observation for one band.
pub(super) struct ProbeObs {
    pub(super) material: String,
    pub(super) frame: u32,
    pub(super) channel: usize,
    pub(super) band: usize,
    pub(super) class: S4Step3Class,
    pub(super) detail: String,
}

/// Attribute every band of one probed frame from paired probe sequences.
pub(super) fn attribute_probe_frame(pf: &ProbeFrame) -> Vec<ProbeObs> {
    let nch = pf.q_cap.s4.ch.len();
    assert_eq!(pf.f_cap.s4.ch.len(), nch);
    let mut out = Vec::new();
    for ch in 0..nch {
        let cnt = pf.q_cap.s4.ch[ch].cnt;
        assert_eq!(pf.f_cap.s4.ch[ch].cnt, cnt);
        for band in 0..cnt {
            let q = band_evals(&pf.q_calls, ch, band);
            let f = band_evals(&pf.f_calls, ch, band);
            let class = classify_probe_band(&q, &f);
            let detail = format!(
                "q_evals={} f_evals={} q_acc={} f_acc={}",
                q.len(),
                f.len(),
                q.iter().any(|e| e.accepted),
                f.iter().any(|e| e.accepted),
            );
            out.push(ProbeObs {
                material: pf.material.clone(),
                frame: pf.frame,
                channel: ch,
                band,
                class,
                detail,
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests (additive; no production change; S1/S2/A6 preserved)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::fpe::tests::r22_capture::{check_acct_state, check_snapshot_completeness};

    #[test]
    fn s4_step3_source_formulas_are_pinned() {
        assert_eq!(crate::aac_enc::scf::MAX_SCF_DELTA, 60, "clamp/gate width");
        assert_eq!(crate::aac_enc::scf::NO_SCF, i32::MIN, "dead-band marker");
        assert_eq!(
            step3_dist_margin(10, 4),
            (4i32.wrapping_add(5_572_475)) as i64 - 10
        );
        assert_eq!(
            step3_energy_margin(100, 200),
            100i64 - (200i32.wrapping_sub(16_722_942)) as i64
        );
        assert_eq!(step3_pe_gate_margin(-3), 3);
        assert_eq!(step3_pe_gate_margin(0), 0, "PE gate equality passes");
        assert_eq!(step3_sum_margin(5, 9), 4);
        assert_eq!(step3_sum_margin(9, 9), 0, "sum equality rejects");
    }

    #[test]
    fn s4_step3_r22_margins_stay_derivation_required() {
        // R22 final-output capture alone still cannot derive the decision
        // distances; the probe below is the derivation path.
        let margins = s4_stage_margins();
        assert_eq!(margins.len(), 4);
        for m in &margins {
            assert!(m.is_required(), "{} must stay Required", m.name);
        }
        let with_probe = super::super::trace::s4_stage_margins_with_probe(true, false);
        assert!(with_probe.iter().all(|m| matches!(
            m.status,
            super::super::trace::MarginStatus::NotApplicable(_)
        )));
    }

    #[test]
    fn s4_step3_pure_edges_boundary() {
        assert!(step3_dist_margin(0, 0) > 0);
        assert_eq!(step3_dist_margin(5_572_475, 0), 0, "dist equality passes");
        assert!(step3_dist_margin(5_572_476, 0) < 0);
        assert!(step3_energy_margin(1, 16_722_942) > 0);
        assert_eq!(
            step3_energy_margin(0, 16_722_942),
            0,
            "energy equality passes"
        );
        assert!(step3_energy_margin(-1, 16_722_942) < 0);
        assert!(step3_pe_gate_margin(-1) > 0);
        assert_eq!(step3_pe_gate_margin(0), 0, "PE equality passes");
        assert!(step3_pe_gate_margin(1) < 0);
        assert!(step3_sum_margin(4, 9) > 0);
        assert_eq!(step3_sum_margin(9, 9), 0, "sum equality rejects");
        assert!(step3_sum_margin(10, 9) < 0);
    }

    #[test]
    fn s4_step3_probe_records_step3_decisions() {
        // Instrumentation smoke: one affected frame records channel-ordered
        // calls with region/iteration/band detail, and every record verifies
        // against production primitives (independent replay, §6).
        let (digest, coding) = r21b_affected_frames();
        assert!(!digest.is_empty() && !coding.is_empty());
        let mut targets: Vec<(String, u32)> = coding.clone();
        targets.sort_unstable();
        targets.dedup();
        let first = targets[0].clone();
        let frames = collect_probe_frames(std::slice::from_ref(&first));
        assert_eq!(frames.len(), 1, "one probed frame");
        let pf = &frames[0];
        assert_eq!(pf.q_calls.len(), 2, "stereo channel calls");
        assert_eq!(pf.f_calls.len(), 2);
        let q_regions: usize = pf.q_calls.iter().map(|c| c.regions.len()).sum();
        let f_regions: usize = pf.f_calls.iter().map(|c| c.regions.len()).sum();
        assert!(q_regions > 0 && f_regions > 0, "regions recorded");
        verify_probe_frame(pf);
        println!(
            "S4-PROBE first={} f{} q_regions={} f_regions={}",
            first.0, first.1, q_regions, f_regions
        );
    }

    #[test]
    fn s4_step3_affected_corpus_recovered() {
        // Identity recovery from existing R2.1-B evidence (no invention).
        let (digest, coding) = r21b_affected_frames();
        let (digest2, coding2) = r21b_affected_frames();
        assert_eq!(digest, digest2, "deterministic digest identities");
        assert_eq!(coding, coding2, "deterministic coding identities");
        assert_eq!(digest.len(), 3, "three digest-changing frames");
        assert_eq!(coding.len(), 24, "twenty-four coding-changing frames");
        for (m, f) in &digest {
            println!("S4-AFFECTED digest mat={m} f{f}");
        }
        for (m, f) in &coding {
            println!("S4-AFFECTED coding mat={m} f{f}");
        }
        // Digest frames are a subset of the coding union on this corpus.
        for d in &digest {
            assert!(
                coding.contains(d),
                "digest frame {d:?} outside coding union"
            );
        }
    }

    #[test]
    fn s4_step3_probe_decisions_attributed() {
        // Full affected-corpus probe: 27 frames, per-band classes, margin
        // distributions, determinism and stable ordering.
        let (digest, coding) = r21b_affected_frames();
        let mut targets = coding.clone();
        targets.sort_unstable();
        targets.dedup();
        assert_eq!(targets.len(), 24);
        let run_once = || {
            let frames = collect_probe_frames(&targets);
            let mut obs = Vec::new();
            for pf in &frames {
                verify_probe_frame(pf);
                obs.extend(attribute_probe_frame(pf));
            }
            obs.sort_by(|a: &ProbeObs, b: &ProbeObs| {
                (a.material.clone(), a.frame, a.channel, a.band).cmp(&(
                    b.material.clone(),
                    b.frame,
                    b.channel,
                    b.band,
                ))
            });
            (frames, obs)
        };
        let (frames_a, obs_a) = run_once();
        let (_frames_b, obs_b) = run_once();
        assert_eq!(obs_a.len(), obs_b.len(), "deterministic observation count");
        for (x, y) in obs_a.iter().zip(obs_b.iter()) {
            assert_eq!(x.material, y.material);
            assert_eq!(x.frame, y.frame);
            assert_eq!(x.channel, y.channel);
            assert_eq!(x.band, y.band);
            assert_eq!(x.class, y.class);
        }
        let mut counts = [0u32; 6];
        for o in &obs_a {
            match o.class {
                S4Step3Class::IntermediateFlip => counts[0] += 1,
                S4Step3Class::SameSideChange => counts[1] += 1,
                S4Step3Class::IntermediateEquality => counts[2] += 1,
                S4Step3Class::Inapplicable => counts[3] += 1,
                S4Step3Class::Unsupported => counts[4] += 1,
                S4Step3Class::Unavailable => counts[5] += 1,
            }
        }
        // Margin distributions from recorded operands (both arms).
        let mut min_dist = i64::MAX;
        let mut min_energy = i64::MAX;
        let mut min_pe = i64::MAX;
        let mut min_sum = i64::MAX;
        let mut exact_zero = [0u32; 4];
        let mut evaluated_bands = 0u32;
        for pf in &frames_a {
            for calls in [&pf.q_calls, &pf.f_calls] {
                for call in calls {
                    for region in &call.regions {
                        if !region.gate_pass {
                            continue;
                        }
                        for iter in &region.iterations {
                            min_pe = min_pe.min(iter.pe_margin.abs());
                            if iter.pe_margin == 0 {
                                exact_zero[2] += 1;
                            }
                            for be in &iter.bands {
                                evaluated_bands += 1;
                                min_dist = min_dist.min(be.dist_margin.abs());
                                min_energy = min_energy.min(be.energy_margin.abs());
                                if be.dist_margin == 0 {
                                    exact_zero[0] += 1;
                                }
                                if be.energy_margin == 0 {
                                    exact_zero[1] += 1;
                                }
                            }
                            min_sum =
                                min_sum.min((iter.old_sum as i64 - iter.new_sum as i64).abs());
                            if iter.old_sum == iter.new_sum {
                                exact_zero[3] += 1;
                            }
                        }
                    }
                }
            }
        }
        println!(
            "S4-PROBE obs={} flip={} sameside={} equal={} inapplicable={} unsupported={} unavailable={}",
            obs_a.len(), counts[0], counts[1], counts[2], counts[3], counts[4], counts[5],
        );
        println!(
            "S4-PROBE digest_targets={} coding_targets={} probed_frames={}",
            digest.len(),
            targets.len(),
            frames_a.len()
        );
        println!(
            "S4-PROBE evaluated_band_evals={} min_abs_dist={} min_abs_energy={} min_abs_pe={} min_abs_sum={} exact_zero={:?}",
            evaluated_bands, min_dist, min_energy, min_pe, min_sum, exact_zero,
        );
        for o in obs_a
            .iter()
            .filter(|o| o.class == S4Step3Class::IntermediateFlip)
            .take(8)
        {
            println!(
                "S4-FLIP {} f{} ch{} b{} {}",
                o.material, o.frame, o.channel, o.band, o.detail
            );
        }
        assert_eq!(counts[4], 0, "no unsupported domain on affected corpus");
        assert_eq!(counts[5], 0, "probe fired: no Unavailable on probed frames");
        assert!(evaluated_bands > 0, "probe evaluated bands");
        assert!(min_dist != i64::MAX && min_energy != i64::MAX && min_pe != i64::MAX);
    }

    #[test]
    fn s4_step3_probe_canary_enabled_vs_disabled() {
        // Capture on/off identity: detailed probe must not alter encoder
        // output or decisions (bytes, digests, gains, scf, quant, state).
        let targets = vec![("probe-narrow-48k".to_string(), 1u32)];
        let (pcm, sr, _) = r21b_pcm("probe-narrow-48k");
        let run = |detailed: bool| {
            step3_probe::set_enabled(true);
            step3_probe::set_detailed(detailed);
            let mut q = q31_encoder(sr, pcm.len());
            let mut f = fpe_encoder(sr, pcm.len());
            let mut out = Vec::new();
            for frame in 0..=4u32 {
                let fresh = frame_slice(&pcm, frame);
                step3_probe::clear();
                let (qb, qs) = q.step_frame(frame, &fresh);
                let (fb, fs) = f.step_frame(frame, &fresh);
                let _ = step3_probe::take_calls();
                let _ = step3_probe::take();
                out.push((qb, fb, qs, fs));
            }
            step3_probe::set_detailed(false);
            out
        };
        let on = run(true);
        let off = run(false);
        assert_eq!(on.len(), off.len());
        for (i, ((qb_on, fb_on, qs_on, fs_on), (qb_off, fb_off, qs_off, fs_off))) in
            on.into_iter().zip(off.into_iter()).enumerate()
        {
            assert_eq!(qb_on, qb_off, "frame {i}: Q31 bytes identical");
            assert_eq!(fb_on, fb_off, "frame {i}: FPE bytes identical");
            assert_eq!(qs_on.digest, qs_off.digest, "frame {i}: Q31 digest");
            assert_eq!(fs_on.digest, fs_off.digest, "frame {i}: FPE digest");
            assert_eq!(qs_on.gains, qs_off.gains, "frame {i}: Q31 gains");
            assert_eq!(fs_on.gains, fs_off.gains, "frame {i}: FPE gains");
            let qc_on = qs_on.r22_cap.as_ref().expect("cap");
            let qc_off = qs_off.r22_cap.as_ref().expect("cap");
            assert_eq!(qc_on.s4.ch.len(), qc_off.s4.ch.len());
            for (a, b) in qc_on.s4.ch.iter().zip(qc_off.s4.ch.iter()) {
                assert_eq!(a.scf, b.scf, "frame {i}: Q31 scf");
                assert_eq!(a.gain, b.gain, "frame {i}: Q31 gain");
                assert_eq!(a.quant_spec, b.quant_spec, "frame {i}: Q31 quant");
                assert_eq!(a.mutated, b.mutated, "frame {i}: Q31 spectrum");
            }
        }
        let _ = targets;
    }

    #[test]
    fn s4_step3_operand_pairing_and_replay() {
        let pcm = testdata::fixture_samples("sine-stereo");
        let sr = testdata::fixture_sample_rate("sine-stereo");
        let mut q = q31_encoder(sr, pcm.len());
        let mut f = fpe_encoder(sr, pcm.len());
        let fresh = frame_slice(&pcm, 1);
        let (_, q_snap) = q.step_frame(1, &fresh);
        let (_, f_snap) = f.step_frame(1, &fresh);
        let q_cap = q_snap.r22_cap.as_ref().expect("Q31 cap");
        let f_cap = f_snap.r22_cap.as_ref().expect("FPE cap");
        check_snapshot_completeness(q_cap, pcm.len(), "s4-step3 Q31 pairing");
        check_snapshot_completeness(f_cap, pcm.len(), "s4-step3 FPE pairing");
        check_acct_state(q_cap, &q_snap, "s4-step3 Q31 acct");
        check_acct_state(f_cap, &f_snap, "s4-step3 FPE acct");
        replay_frame(q_cap, "s4-step3 Q31 replay");
        replay_frame(f_cap, "s4-step3 FPE replay");
        assert_eq!(q_cap.s4.ch.len(), f_cap.s4.ch.len());
    }

    #[test]
    fn s4_step3_final_equality_is_not_intermediate_equivalence() {
        let obs = classify_s4_band("sine-stereo", 1, 0, 3, 7, 7, 42, 42);
        assert!(obs.final_equal);
        assert_eq!(obs.class, S4Step3Class::Unavailable);
        let obs2 = classify_s4_band("sine-stereo", 1, 0, 3, 7, 8, 42, 42);
        assert!(!obs2.final_equal);
        assert_eq!(obs2.class, S4Step3Class::Unavailable);
        let dead = classify_s4_band("sine-stereo", 1, 0, 3, i32::MIN, i32::MIN, 0, 0);
        assert_eq!(dead.class, S4Step3Class::Inapplicable);
    }

    #[test]
    fn s4_step3_capture_canary_enabled_vs_disabled() {
        let pcm = testdata::fixture_samples("onset-stereo");
        let sr = testdata::fixture_sample_rate("onset-stereo");
        let mut on = q31_encoder(sr, pcm.len());
        let mut off = q31_encoder(sr, pcm.len());
        off.set_r22_capture(false);
        for frame in 18..22 {
            let fresh = frame_slice(&pcm, frame);
            let (b_on, s_on) = on.step_frame(frame, &fresh);
            let (b_off, s_off) = off.step_frame(frame, &fresh);
            assert_eq!(b_on, b_off, "onset f{frame}: bytes unchanged by capture");
            assert_eq!(s_on.digest, s_off.digest, "onset f{frame}: digest");
            assert_eq!(s_on.gains, s_off.gains, "onset f{frame}: gains");
        }
    }

    #[test]
    fn s4_step3_corpus_deterministic_and_ordered() {
        let run_once = || -> Vec<S4Step3Obs> {
            let mut all = Vec::new();
            for (mat, first, count) in S4_STEP3_TARGETS {
                all.extend(collect_s4_step3_obs(mat, *first, *count));
            }
            all.sort_by(|x, y| {
                (x.material, x.frame, x.channel, x.band)
                    .cmp(&(y.material, y.frame, y.channel, y.band))
            });
            all
        };
        let a = run_once();
        let b = run_once();
        assert_eq!(a.len(), b.len(), "observation count deterministic");
        assert!(!a.is_empty());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.material, y.material);
            assert_eq!(x.frame, y.frame);
            assert_eq!(x.channel, y.channel);
            assert_eq!(x.band, y.band);
            assert_eq!(x.q_scf, y.q_scf);
            assert_eq!(x.f_scf, y.f_scf);
            assert_eq!(x.class, y.class);
        }
        let mut prev = (String::new(), 0u32, 0usize, 0usize);
        for o in &a {
            let key = (o.material.to_string(), o.frame, o.channel, o.band);
            assert!(key >= prev, "stable observation ordering");
            prev = key;
        }
        let mut counts = [0u32; 6];
        for o in &a {
            match o.class {
                S4Step3Class::IntermediateFlip => counts[0] += 1,
                S4Step3Class::SameSideChange => counts[1] += 1,
                S4Step3Class::IntermediateEquality => counts[2] += 1,
                S4Step3Class::Inapplicable => counts[3] += 1,
                S4Step3Class::Unsupported => counts[4] += 1,
                S4Step3Class::Unavailable => counts[5] += 1,
            }
        }
        println!(
            "S4-STEP3 obs={} flip={} sameside={} equal={} inapplicable={} unsupported={} unavailable={} min_abs_scf_delta={:?}",
            a.len(),
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            counts[4],
            counts[5],
            min_abs_scf_delta(&a),
        );
        assert_eq!(counts[0], 0);
        assert_eq!(counts[1], 0);
        assert_eq!(counts[2], 0);
        assert_eq!(counts[4], 0);
        assert!(counts[5] > 0);
        let finals_equal = a.iter().filter(|o| o.final_equal).count();
        println!("S4-STEP3 finals_equal={finals_equal} total={}", a.len());
    }
}
