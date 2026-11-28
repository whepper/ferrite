//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! In-binary Q31↔FPE differential harness (Tier-A, `docs/fpe/07` §3/§3.1).
//!
//! Self-contained: identical inputs are fed to both engines
//! ([`crate::aac_enc::psy::process_pair_reuse`] and
//! [`crate::aac_enc::fpe::process_pair_reuse`]) with independently
//! initialized but equivalent state, and the results are compared
//! field-by-field. Runs **multi-frame sequences** and compares the
//! complete observable output **and all frame-surviving persistent state
//! after every frame** (`PreEchoState`, TNS `tns_state`/`tns_info`, MS-Ld
//! history including stale-on-short contents), so a divergence is
//! localized to the frame that introduced it (risk R-21). No C artifacts
//! are needed; this is the primary development gate and runs in default
//! CI. Private state is inspected through `#[cfg(test)]` helpers only —
//! no production API is added for testing.
//!
//! **Comparison strictness (FPE-3.1R1):** the rescale band energies and their
//! Ld vectors are float-derived. They are compared with the owner-approved
//! **per-band derived envelope** (`docs/fpe/05` §9.2, `envelope.rs`), not a
//! fixed global tolerance. The audit (`audit.rs`) collects all failures,
//! term-separates Q31 input-mapping / truncation / boundary contributions,
//! measures absolute Ld deltas, classifies low-energy sentinel divergences by
//! stored state, and runs three independent identity gates. `q31_tol` is
//! retained for thresholds, `nm1`, grouped thresholds and reused Q31-kernel
//! outputs where its assumptions hold. Representation fields, TNS state, MS
//! energies/Ld, grouped layout/offsets/maxSfb, pre-echo flags and the mutated
//! spectrum remain **bit-identical** (`==` / `to_bits()`).
//!
//! Execution integrity (`docs/fpe/07` §7.1): every run asserts its exact
//! expected frame count (`frames_compared`); a skipped or missing frame
//! is a loud failure, never a silent omission. Scenario tests also assert
//! that the intended paths (long/short transitions, START/STOP, pre-echo
//! disable/reset, stale-on-short MS-Ld) were actually exercised.

mod audit;
mod ac9;
mod ac9_artifact;
mod decision;
mod differential;
mod envelope;
mod evidence;
mod f6_tns;
mod is_ms;
mod m2_stages;
mod precision;
mod probe_envelope;
mod r22_capture;
mod s4_step3;
mod trace;
mod xarch_log2;

use audit::{ChannelEnergyCtx, EnergyAudit};

use crate::aac_enc::block_switch::{BlockSwitch, WindowSequence};
use crate::aac_enc::fpe::{FpeChannel, FpeScratch};
use crate::aac_enc::psy::{
    self, ChannelInput, ChannelOutput, GroupedOutput, MsOutput, PsyChannel, PsyScratch, PsyTables,
};
use crate::aac_enc::testdata;
use crate::aac_enc::tns::TnsChannel;
use crate::aac_enc::transform::ChannelTransform;

// ---------------------------------------------------------------------------
// Exact-field comparators; value fields are handled by `audit::EnergyAudit`
// ---------------------------------------------------------------------------

/// `q31_tol = 32 + |ref|/500`, retained for the state/vector fields whose
/// input assumptions remain valid (`docs/fpe/05` §9.3, `07` §5). Energy,
/// spread and grouped-energy vectors use the derived per-band envelope.
fn q31_tol(want: i32) -> i64 {
    32 + (want as i64).abs() / 500
}

fn assert_i32_close(got: &[i32], want: &[i32], tag: &str) {
    assert_eq!(got.len(), want.len(), "{tag}: length");
    for (i, (&g, &w)) in got.iter().zip(want.iter()).enumerate() {
        let d = (g as i64 - w as i64).abs();
        let tol = q31_tol(w);
        assert!(d <= tol, "{tag}[{i}]: got {g} want {w} (tol {tol})");
    }
}

/// f64 bit-identity (distinguishes ±0.0 and catches NaN drift; reused
/// Q31-produced values must match exactly, not merely closely).
fn assert_f64_bits_eq(got: &[f64], want: &[f64], tag: &str) {
    assert_eq!(got.len(), want.len(), "{tag}: length");
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(
            g.to_bits(),
            w.to_bits(),
            "{tag}[{i}]: got {g} want {w} (bits {:#x} vs {:#x})",
            g.to_bits(),
            w.to_bits()
        );
    }
}

fn assert_i32_vec_eq(got: &[i32], want: &[i32], tag: &str) {
    assert_eq!(got, want, "{tag}");
}

fn assert_i32_windows_eq(got: &[Vec<i32>], want: &[Vec<i32>], tag: &str) {
    assert_eq!(got.len(), want.len(), "{tag}: window count");
    for (w, (g, v)) in got.iter().zip(want.iter()).enumerate() {
        assert_i32_vec_eq(g, v, &format!("{tag} w{w}"));
    }
}

fn assert_f64_windows_eq(got: &[Vec<f64>], want: &[Vec<f64>], tag: &str) {
    assert_eq!(got.len(), want.len(), "{tag}: window count");
    for (w, (g, v)) in got.iter().zip(want.iter()).enumerate() {
        assert_f64_bits_eq(g, v, &format!("{tag} w{w}"));
    }
}

/// Every **exact** field of [`GroupedOutput`]: layout/offsets/minSnr/MS
/// fields and the regrouped spectrum (reused Q31 lane). Energies,
/// thresholds, spread and Ld vectors are checked by the audit.
fn assert_grouped_output_eq(got: &GroupedOutput, want: &GroupedOutput, tag: &str) {
    assert_eq!(
        got.energies.len(),
        want.energies.len(),
        "{tag} grouped.energies len"
    );
    assert_eq!(
        got.thresholds.len(),
        want.thresholds.len(),
        "{tag} grouped.thresholds len"
    );
    assert_eq!(
        got.spread.len(),
        want.spread.len(),
        "{tag} grouped.spread len"
    );
    assert_eq!(
        got.energy_ld.len(),
        want.energy_ld.len(),
        "{tag} grouped.energy_ld len"
    );
    assert_eq!(
        got.threshold_ld.len(),
        want.threshold_ld.len(),
        "{tag} grouped.threshold_ld len"
    );
    assert_i32_vec_eq(
        &got.offsets,
        &want.offsets,
        &format!("{tag} grouped.offsets"),
    );
    assert_i32_vec_eq(
        &got.min_snr,
        &want.min_snr,
        &format!("{tag} grouped.min_snr"),
    );
    assert_f64_bits_eq(
        &got.spectrum,
        &want.spectrum,
        &format!("{tag} grouped.spectrum"),
    );
    assert_eq!(
        got.max_sfb_per_group, want.max_sfb_per_group,
        "{tag} grouped.max_sfb_per_group"
    );
    assert_i32_vec_eq(
        &got.ms_energies,
        &want.ms_energies,
        &format!("{tag} grouped.ms_energies"),
    );
    assert_i32_vec_eq(&got.ms_ld, &want.ms_ld, &format!("{tag} grouped.ms_ld"));
}

/// Every **exact** field of [`ChannelOutput`]: representation fields,
/// maxScale, pre-echo flags, layout and MS fields. Value vectors
/// (energies/Ld/thresholds/spread/nm1/grouped values) are checked by the
/// audit with the derived energy envelope or retained `q31_tol`.
fn assert_channel_output_eq(got: &ChannelOutput, want: &ChannelOutput, tag: &str) {
    assert_eq!(got.mdct_scale, want.mdct_scale, "{tag} mdct_scale");
    assert_eq!(got.final_shift, want.final_shift, "{tag} final_shift");
    assert_eq!(got.tns_shift, want.tns_shift, "{tag} tns_shift");
    assert_eq!(got.nrg_scaling, want.nrg_scaling, "{tag} nrg_scaling");
    assert_i32_windows_eq(&got.maxscale, &want.maxscale, &format!("{tag} maxscale"));
    assert_eq!(
        got.energies.len(),
        want.energies.len(),
        "{tag} energy windows"
    );
    assert_eq!(
        got.energy_ld.len(),
        want.energy_ld.len(),
        "{tag} energy_ld windows"
    );
    assert_eq!(
        got.thresholds.len(),
        want.thresholds.len(),
        "{tag} threshold windows"
    );
    assert_eq!(got.spread.len(), want.spread.len(), "{tag} spread windows");
    for w in 0..got.energies.len() {
        assert_eq!(
            got.energies[w].len(),
            want.energies[w].len(),
            "{tag} energy len w{w}"
        );
        assert_eq!(
            got.energy_ld[w].len(),
            want.energy_ld[w].len(),
            "{tag} energy_ld len w{w}"
        );
        assert_eq!(
            got.thresholds[w].len(),
            want.thresholds[w].len(),
            "{tag} threshold len w{w}"
        );
        assert_eq!(
            got.spread[w].len(),
            want.spread[w].len(),
            "{tag} spread len w{w}"
        );
    }
    assert_eq!(got.m_nm1, want.m_nm1, "{tag} m_nm1");
    assert_eq!(got.calc_pre_echo, want.calc_pre_echo, "{tag} calc_pre_echo");
    match (&got.grouped, &want.grouped) {
        (Some(g), Some(w)) => assert_grouped_output_eq(g, w, tag),
        (None, None) => {}
        _ => panic!("{tag}: grouped presence differs"),
    }
    match (&got.threshold_ld, &want.threshold_ld) {
        (Some(_), Some(_)) => {}
        (None, None) => {}
        _ => panic!("{tag}: threshold_ld presence differs"),
    }
    assert_eq!(
        got.max_sfb_per_group, want.max_sfb_per_group,
        "{tag} max_sfb_per_group"
    );
}

/// Every field of [`MsOutput`], bit-identical (includes the stale-on-short
/// pre-grouping Ld readout).
fn assert_ms_output_eq(got: &MsOutput, want: &MsOutput, tag: &str) {
    assert_i32_windows_eq(&got.mid, &want.mid, &format!("{tag} ms.mid"));
    assert_i32_windows_eq(&got.side, &want.side, &format!("{tag} ms.side"));
    assert_f64_windows_eq(&got.mid_ld, &want.mid_ld, &format!("{tag} ms.mid_ld"));
    assert_f64_windows_eq(&got.side_ld, &want.side_ld, &format!("{tag} ms.side_ld"));
}

/// Field-by-field [`TnsChannel`] comparison (all fields are `pub`;
/// `docs/fpe/07` §3.1 test-only compare, no production API).
fn assert_tns_channel_eq(got: &TnsChannel, want: &TnsChannel, tag: &str) {
    assert_eq!(got.num_filters, want.num_filters, "{tag} num_filters");
    assert_eq!(got.coef_res, want.coef_res, "{tag} coef_res");
    assert_eq!(got.length, want.length, "{tag} length");
    assert_eq!(got.order, want.order, "{tag} order");
    assert_eq!(got.direction, want.direction, "{tag} direction");
    assert_eq!(got.coef, want.coef, "{tag} coef");
    assert_eq!(got.active, want.active, "{tag} active");
    assert_eq!(got.gain, want.gain, "{tag} gain");
    assert_eq!(got.merged, want.merged, "{tag} merged");
}

/// All frame-surviving persistent psy state after a frame
/// (`docs/fpe/07` §3.1): `PreEchoState` (prev/prev_scale/enabled via the
/// existing pub accessors), TNS `tns_state` + `tns_info`, and the MS-Ld
/// history `[[f64; 64]; 8]` including stale-on-short contents (test-only
/// accessors).
///
/// M3: pre-echo history values are Q31-arithmetic downstream of the
/// replaced energies, so they use the stored envelope; the scale/enabled
/// transition flags, TNS state, and MS-Ld history remain exact.
fn assert_psy_state_eq(q31: &PsyChannel, fpe: &FpeChannel, tag: &str) {
    // M2: FpeChannel owns the individual state fields (preecho, tns_state,
    // tns_info, ms_ld) directly, exposed via test-only accessors that mirror
    // PsyChannel's. Compare field-by-field.
    assert_i32_close(
        q31.preecho_prev(),
        fpe.preecho_prev(),
        &format!("{tag} preecho.prev"),
    );
    assert_eq!(
        q31.preecho_state(),
        fpe.preecho_state(),
        "{tag} preecho.prev_scale/enabled"
    );
    assert_tns_channel_eq(
        q31.tns_state(),
        fpe.tns_state(),
        &format!("{tag} tns_state"),
    );
    assert_tns_channel_eq(q31.tns_info(), fpe.tns_info(), &format!("{tag} tns_info"));
    for w in 0..8 {
        assert_f64_bits_eq(
            &q31.ms_ld()[w],
            &fpe.ms_ld()[w],
            &format!("{tag} ms_ld w{w}"),
        );
    }
}

// ---------------------------------------------------------------------------
// Dual-engine multi-frame sequence runner
// ---------------------------------------------------------------------------

/// Drives the shared upstream front-end (block switch + MDCT, identical by
/// construction) and feeds **identical** spectra to both psy engines with
/// independently initialized, equivalent state; compares the complete
/// output, the mutated spectra, and all persistent state after **every**
/// frame. Mirrors the `psy.rs` test `Stream` helper, doubled for the
/// differential. Scratch is encoder-owned-style and reused across frames
/// on both sides (also gates the reset discipline).
struct DiffStream {
    name: String,
    switches: Vec<BlockSwitch>,
    transforms: Vec<ChannelTransform>,
    q31: Vec<PsyChannel>,
    fpe: Vec<FpeChannel>,
    q31_ws: PsyScratch,
    fpe_ws: FpeScratch,
    fixture: Vec<Vec<i16>>,
    tables: PsyTables,
    channels: usize,
    /// Execution integrity: exact count of frames processed AND compared.
    frames_compared: usize,
    /// Per-frame ch0 window sequence (CPE pairs agree post-sync).
    seqs_seen: Vec<WindowSequence>,
    /// Frames where any channel's pre-echo control was disabled
    /// (STOP-reset / memorize-only path exercised).
    preecho_disabled_frames: usize,
    short_frames: usize,
    /// Short frames whose MS-Ld history was left stale (unchanged from
    /// the pre-frame snapshot) — the C stale-on-short parity path.
    stale_ms_reads: usize,
    /// Per-frame ch0 representation-lane decisions `(mdct_scale,
    /// final_shift, tns_shift)`, captured to make the ADR-0005 invariant
    /// observable (M2). FPE≡Q31 on these is asserted every frame by
    /// `assert_channel_output_eq`; this records the Q31 trajectory.
    repr_seen: Vec<(i32, i32, i32)>,
    /// FPE-3.1R1 term-separated value audit (collect-all; asserted by the
    /// scenario/full-corpus tests, not inside `run`).
    audit: EnergyAudit,
}

impl DiffStream {
    fn new(name: &str) -> Self {
        let fixture = testdata::fixture_samples(name);
        let channels = fixture.len();
        let sr = testdata::fixture_sample_rate(name);
        Self::from_pcm(name, fixture, sr, channels)
    }

    /// Build a differential stream over caller-supplied PCM (G-1 probe
    /// corpus). Same operating point as [`DiffStream::new`] (VBR-5
    /// per-channel bitrates, 19293 bandwidth — the effective VBR-5
    /// bandwidth at both 44100 and 48000 Hz, `min(19293, sr/2)`), same
    /// state initialization, same audit machinery. The `display` name
    /// tags every audit row (`"{display} f{frame}"`), matching the R2.1-B
    /// material names for cross-referencing findings.
    fn from_pcm(display: &str, pcm: Vec<Vec<i16>>, sample_rate: u32, channels: usize) -> Self {
        assert_eq!(pcm.len(), channels, "from_pcm: channel count");
        assert!(!pcm.is_empty(), "from_pcm: stereo or mono PCM required");
        // Same operating point as the psy.rs vector harness and the
        // committed vectors (VBR-5 per-channel bitrates, 19293 bandwidth).
        let (ch_br, tns_br) = if channels == 1 {
            (112000, 112000)
        } else {
            (96000, 192000)
        };
        let ((lcfg, _), (scfg, _)) = testdata::load_cfg(sample_rate, ch_br);
        let tables = PsyTables::for_rate(
            sample_rate,
            ch_br,
            tns_br,
            19293,
            lcfg.min_snr_ld,
            scfg.min_snr_ld,
        )
        .expect("tables build");
        let switches = (0..channels).map(|_| BlockSwitch::new()).collect();
        let transforms = (0..channels).map(|_| ChannelTransform::new()).collect();
        let q31 = (0..channels)
            .map(|_| PsyChannel::new(tables.long_cfg.pcm_quant_q31()))
            .collect();
        let fpe = (0..channels)
            .map(|_| FpeChannel::new(tables.long_cfg.pcm_quant_q31()))
            .collect();
        Self {
            name: display.to_string(),
            switches,
            transforms,
            q31,
            fpe,
            q31_ws: PsyScratch::default(),
            fpe_ws: FpeScratch::default(),
            fixture: pcm,
            tables,
            channels,
            frames_compared: 0,
            seqs_seen: Vec::new(),
            preecho_disabled_frames: 0,
            short_frames: 0,
            stale_ms_reads: 0,
            repr_seen: Vec::new(),
            audit: EnergyAudit::default(),
        }
    }

    fn fresh_at(&self, frame: u32) -> Vec<[i16; 1024]> {
        (0..self.channels)
            .map(|c| {
                let mut block = [0i16; 1024];
                for (i, v) in block.iter_mut().enumerate() {
                    let t = frame as usize * 1024 + i;
                    *v = if t < self.fixture[c].len() {
                        self.fixture[c][t]
                    } else {
                        0
                    };
                }
                block
            })
            .collect()
    }

    /// Run the shared front-end for one frame and compare both engines.
    fn step(&mut self, frame: u32) {
        let fresh = self.fresh_at(frame);
        for (sw, fr) in self.switches.iter_mut().zip(fresh.iter()) {
            sw.process(fr, false);
        }
        if self.channels == 2 {
            let (l, r) = self.switches.split_at_mut(1);
            BlockSwitch::synchronize_pair(&mut l[0], &mut r[0]);
        } else {
            self.switches[0].synchronize_mono();
        }
        let mut specs: Vec<[f64; 1024]> = Vec::with_capacity(self.channels);
        let mut seqs = Vec::with_capacity(self.channels);
        let mut groupings = Vec::with_capacity(self.channels);
        for (ch, (tr, sw)) in self
            .transforms
            .iter_mut()
            .zip(self.switches.iter())
            .enumerate()
        {
            let out = tr
                .process(&fresh[ch], sw.sequence(), sw.shape())
                .expect("mdct");
            let mut spec = [0.0f64; 1024];
            for (d, s) in spec.iter_mut().zip(out.iter()) {
                *d = *s as f64;
            }
            specs.push(spec);
            seqs.push(sw.sequence());
            groupings.push(sw.grouping());
        }
        self.step_inputs(specs, seqs, groupings);
    }

    /// Feed identical prepared inputs to both engines and compare output,
    /// mutated spectra, and persistent state. Also used by the
    /// committed-vector single-frame tests.
    #[allow(clippy::needless_range_loop)]
    fn step_inputs(
        &mut self,
        mut specs: Vec<[f64; 1024]>,
        seqs: Vec<WindowSequence>,
        groupings: Vec<(u8, [u8; 4])>,
    ) {
        let tag = format!("{} f{}", self.name, self.frames_compared);
        assert_eq!(specs.len(), self.channels, "{tag}: input channels");
        // Keep the pre-TNS spectrum for the envelope/identity gates. Apply
        // the same lowpass zeroing the engines perform so the saved copy is
        // the actual rescale-stage input (the last active band may straddle
        // the lowpass line).
        let short = seqs[0] == WindowSequence::Short;
        let n_win = if short { 8 } else { 1 };
        let wlen = if short { 128 } else { 1024 };
        let conf = if short {
            &self.tables.short_cfg
        } else {
            &self.tables.long_cfg
        };
        let mut specs_input = specs.clone();
        for spec in specs_input.iter_mut() {
            psy::lowpass_and_zero_check(spec, n_win, wlen, conf.lowpass_line());
        }
        // Pre-frame MS-Ld snapshot for stale-on-short coverage accounting.
        let ms_before: Vec<[[f64; 64]; 8]> = self.q31.iter().map(|c| *c.ms_ld()).collect();
        // Each engine mutates its own copy of the identical input spectrum.
        let mut specs_fpe = specs.clone();

        // Q31 arm (authoritative baseline).
        let (head, tail) = specs.split_at_mut(1);
        let lin = ChannelInput {
            spectrum: &mut head[0],
            seq: seqs[0],
            grouping: groupings[0],
            is_lfe: false,
        };
        let (ol_q, or_q, ms_q) = if self.channels == 2 {
            let rin = ChannelInput {
                spectrum: &mut tail[0],
                seq: seqs[1],
                grouping: groupings[1],
                is_lfe: false,
            };
            let (pl, pr) = self.q31.split_at_mut(1);
            psy::process_pair_reuse(
                &mut pl[0],
                Some(&mut pr[0]),
                lin,
                Some(rin),
                &self.tables,
                &mut self.q31_ws,
            )
        } else {
            let (ol, _, _) = psy::process_pair_reuse(
                &mut self.q31[0],
                None,
                lin,
                None,
                &self.tables,
                &mut self.q31_ws,
            );
            (ol, None, None)
        };

        // FPE arm (implementation under test), identical inputs.
        let (head, tail) = specs_fpe.split_at_mut(1);
        let lin = ChannelInput {
            spectrum: &mut head[0],
            seq: seqs[0],
            grouping: groupings[0],
            is_lfe: false,
        };
        let (ol_f, or_f, ms_f) = if self.channels == 2 {
            let rin = ChannelInput {
                spectrum: &mut tail[0],
                seq: seqs[1],
                grouping: groupings[1],
                is_lfe: false,
            };
            let (pl, pr) = self.fpe.split_at_mut(1);
            crate::aac_enc::fpe::process_pair_reuse(
                &mut pl[0],
                Some(&mut pr[0]),
                lin,
                Some(rin),
                &self.tables,
                &mut self.fpe_ws,
            )
        } else {
            let (ol, _, _) = crate::aac_enc::fpe::process_pair_reuse(
                &mut self.fpe[0],
                None,
                lin,
                None,
                &self.tables,
                &mut self.fpe_ws,
            );
            (ol, None, None)
        };

        // Complete observable output: exact representation/reused fields are
        // asserted directly; value fields go through the collect-all audit.
        assert_channel_output_eq(&ol_f, &ol_q, &format!("{tag} ch0"));
        match (&or_f, &or_q) {
            (Some(g), Some(w)) => {
                assert_channel_output_eq(g, w, &format!("{tag} ch1"));
            }
            (None, None) => {}
            _ => panic!("{tag}: right-output presence differs"),
        }
        match (&ms_f, &ms_q) {
            (Some(g), Some(w)) => assert_ms_output_eq(g, w, &format!("{tag} ms")),
            (None, None) => {}
            _ => panic!("{tag}: MS-output presence differs"),
        }

        // FPE-3.1R1 value audit (derived energy envelope + retained q31_tol).
        let mut tns_active = [false; 8];
        for w in 0..n_win {
            tns_active[w] = self
                .q31
                .iter()
                .any(|c| c.tns_state().active[w][0] || c.tns_state().active[w][1]);
        }
        {
            let ctx = ChannelEnergyCtx {
                tag: tag.clone(),
                ch: 0,
                short,
                active: conf.sfb_active(),
                grouping: groupings[0],
                x: &specs_input[0],
                tns_active,
                conf,
            };
            self.audit.compare_channel(&ctx, &ol_f, &ol_q);
        }
        if let (Some(g), Some(w)) = (&or_f, &or_q) {
            let ctx = ChannelEnergyCtx {
                tag: tag.clone(),
                ch: 1,
                short,
                active: conf.sfb_active(),
                grouping: groupings[1],
                x: &specs_input[1],
                tns_active,
                conf,
            };
            self.audit.compare_channel(&ctx, g, w);
        }

        // Mutated spectra (TNS filtering + exact scalings), bit-identical.
        for ch in 0..self.channels {
            assert_f64_bits_eq(
                &specs_fpe[ch],
                &specs[ch],
                &format!("{tag} ch{ch} mutated spectrum"),
            );
        }
        // All frame-surviving persistent state, after this frame.
        for ch in 0..self.channels {
            assert_psy_state_eq(&self.q31[ch], &self.fpe[ch], &format!("{tag} ch{ch} state"));
        }

        // Coverage accounting (docs/fpe/07 §7.1: no silent omissions).
        self.frames_compared += 1;
        self.seqs_seen.push(seqs[0]);
        self.repr_seen
            .push((ol_q.mdct_scale, ol_q.final_shift, ol_q.tns_shift));
        if short {
            self.short_frames += 1;
        }
        if self.q31.iter().any(|c| !c.preecho_state().1) {
            self.preecho_disabled_frames += 1;
        }
        if short
            && self
                .q31
                .iter()
                .enumerate()
                .all(|(ch, c)| c.ms_ld() == &ms_before[ch])
        {
            self.stale_ms_reads += 1;
        }
    }

    fn seen(&self, seq: WindowSequence) -> bool {
        self.seqs_seen.contains(&seq)
    }

    /// Run the full fixture plus two zero-padded tail frames (decaying
    /// histories / all-zero path), asserting exact frame accounting.
    fn run(name: &str) -> Self {
        Self::run_stream(Self::new(name))
    }

    /// [`run`] plus the FPE-3.1R1 value-audit gate for one scenario.
    fn run_checked(name: &str) -> Self {
        let s = Self::run(name);
        s.audit.assert_clean(name);
        s
    }

    /// [`run`] over caller-supplied PCM (G-1 probe corpus): the probe
    /// frames plus two zero-padded tail frames, same accounting
    /// discipline. `display` names the scenario for frame accounting
    /// and every audit row.
    fn run_pcm(display: &str, pcm: Vec<Vec<i16>>, sample_rate: u32) -> Self {
        let channels = pcm.len();
        Self::run_stream(Self::from_pcm(display, pcm, sample_rate, channels))
    }

    /// [`run_pcm`] plus the FPE-3.1R1 value-audit gate for one probe.
    fn run_checked_pcm(display: &str, pcm: Vec<Vec<i16>>, sample_rate: u32) -> Self {
        let label = display.to_string();
        let s = Self::run_pcm(display, pcm, sample_rate);
        s.audit.assert_clean(&label);
        s
    }

    /// Shared run body: step every frame plus two zero-padded tail
    /// frames, asserting exact frame accounting (`07` §7.1).
    fn run_stream(mut s: Self) -> Self {
        let name = s.name.clone();
        let n = s.fixture[0].len().div_ceil(1024) + 2;
        for f in 0..n {
            s.step(f as u32);
        }
        assert_eq!(s.frames_compared, n, "{name}: frame accounting");
        s
    }
}
