//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! R2.1-A independent downstream harness (test-only).
//!
//! Two complementary layers, both engine-tagged with disjoint-storage
//! proof:
//!
//! * **Frontend layer (primary, F1).** Two `StreamEncoder` instances — one
//!   Q31 (default), one FPE — are driven through the **production**
//!   `StreamEncoder::frontend` path on identical PCM frames. The returned
//!   `FrontendOut` values (`LiveChannel` working sets, `MsOutput`, canonical
//!   grouped MS) are the actual downstream-consumed boundary: no conversion
//!   is copied or reimplemented. Each `LiveHarness` owns its two encoders,
//!   which persist across that harness's `step_pair` calls, so block-switch,
//!   transform, and psy histories are live **across the calls made on one
//!   harness**. A `TaggedLiveInputs` value itself holds only one frame's
//!   working sets — persistent state lives in the harness encoders, not in
//!   the tagged value — and a harness stepped at an isolated frame starts
//!   from fresh (zero-history) encoder state. Frame accounting uses an
//!   expected-frame ledger (`FrameLedger`), not a counter.
//! * **Psy-output layer (supporting).** Direct `psy::process_pair_reuse` /
//!   `fpe::process_pair_reuse` calls feed re-derived frontend spectra to each
//!   engine. Block-switch/transform context is replayed from frame 0 to reach
//!   the target frame, but engine channel state (`PsyChannel`/`FpeChannel`:
//!   pre-echo, TNS, MS-Ld histories) is freshly built per call, so the
//!   resulting psy histories do **not** match a real continuous run at an
//!   arbitrary target frame. This layer exists only for buffers the
//!   production frontend consumes internally and therefore does not return
//!   (`maxScale`, `nm1`, per-window vectors, grouped internals): it proves
//!   those buffers are also independently owned.
//!
//! Scope: harness and ownership tests only. This module does **not** invoke
//! or compare the IS/MS, PE, S2, S4, S6 or frame algorithms — those belong
//! to later stages. `AtsElement` travels through the stage skeletons only
//! as a representative cloneable state token; real downstream state lives
//! in `TaggedLiveInputs` and is compared by later stages.

use crate::aac_enc::block_switch::{BlockSwitch, WindowSequence};
use crate::aac_enc::config::{EncoderConfig, PsyEngineKind, TransportType, VbrMode, FRAME_LENGTH};
use crate::aac_enc::fpe::{FpeChannel, FpeScratch};
use crate::aac_enc::pe::AtsElement;
use crate::aac_enc::psy::{
    self, ChannelInput, ChannelOutput, MsOutput, PsyChannel, PsyScratch, PsyTables,
};
use crate::aac_enc::stream_driver::{FrontendOut, StreamEncoder};
use crate::aac_enc::testdata;
use crate::aac_enc::tns::TnsChannel;
use crate::aac_enc::transform::ChannelTransform;

/// Test-only engine origin tag. Every independently constructed input set
/// carries one; downstream helpers assert it at entry so a Q31-produced
/// value can never silently flow into an FPE comparison arm.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum EngineTag {
    Q31,
    Fpe,
}

/// Assert disjoint storage for two same-length slices (F2).
///
/// Lengths are asserted equal first; empty slices carry no storage (every
/// `Vec::new()` shares the dangling pointer), so the pointer check is
/// vacuous exactly when both are empty.
fn assert_disjoint<T>(a: &[T], b: &[T], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: length");
    if a.is_empty() && b.is_empty() {
        return;
    }
    assert!(
        !std::ptr::eq(a.as_ptr(), b.as_ptr()),
        "{what}: shared storage"
    );
}

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

/// Field-by-field `TnsChannel` equivalence (all fields `pub`; exact
/// integer history must match between the reused-TNS arms).
fn assert_tns_eq(a: &TnsChannel, b: &TnsChannel, what: &str) {
    assert_eq!(a.num_filters, b.num_filters, "{what} num_filters");
    assert_eq!(a.coef_res, b.coef_res, "{what} coef_res");
    assert_eq!(a.length, b.length, "{what} length");
    assert_eq!(a.order, b.order, "{what} order");
    assert_eq!(a.direction, b.direction, "{what} direction");
    assert_eq!(a.coef, b.coef, "{what} coef");
    assert_eq!(a.active, b.active, "{what} active");
    assert_eq!(a.gain, b.gain, "{what} gain");
    assert_eq!(a.merged, b.merged, "{what} merged");
}

// ---------------------------------------------------------------------------
// Frontend layer (F1): production LiveChannel boundary
// ---------------------------------------------------------------------------

/// Independently owned production frontend outputs for one engine arm.
///
/// `fe` is the owned `FrontendOut` returned by that arm's `StreamEncoder`:
/// real `LiveChannel` working sets, `MsOutput`, and canonical grouped MS
/// for a single frame. Ownership is by value; pointer checks run through
/// the `#[cfg(test)]` `LiveChannel` accessors. Persistent histories live in
/// the owning harness's encoders, not in this value.
pub(super) struct TaggedLiveInputs {
    pub tag: EngineTag,
    pub fe: FrontendOut,
    pub frame: u32,
}

/// Expected-frame ledger (F3): every expected frame must be processed
/// exactly once; duplicates, unexpected frames, and skips fail loudly.
/// Completion is validated explicitly — a counter alone is insufficient.
#[derive(Default)]
pub(super) struct FrameLedger {
    expected: Vec<u32>,
    processed: Vec<u32>,
    skipped: Vec<u32>,
}

impl FrameLedger {
    pub(super) fn new(expected: Vec<u32>) -> Self {
        Self {
            expected,
            processed: Vec::new(),
            skipped: Vec::new(),
        }
    }

    /// Record a successfully processed frame. FAILS on unexpected or
    /// duplicate frames.
    pub(super) fn record(&mut self, frame: u32) {
        assert!(
            self.expected.contains(&frame),
            "ledger: unexpected frame {frame} (expected {:?})",
            self.expected
        );
        assert!(
            !self.processed.contains(&frame),
            "ledger: duplicate frame {frame}"
        );
        assert!(
            !self.skipped.contains(&frame),
            "ledger: frame {frame} already recorded as skipped"
        );
        self.processed.push(frame);
    }

    /// Record a frame the encoder skipped with a note (loud finding, never
    /// silent). Skips fail completion — they are findings, not passes.
    pub(super) fn record_skip(&mut self, frame: u32) {
        assert!(
            self.expected.contains(&frame),
            "ledger: unexpected skipped frame {frame}"
        );
        self.skipped.push(frame);
    }

    /// PASS iff every expected frame was processed exactly once with no
    /// skips. Missing, duplicate (already rejected at record), skipped, or
    /// unexpected frames FAIL here.
    pub(super) fn validate_complete(&self) {
        assert!(
            self.skipped.is_empty(),
            "ledger FAIL: skipped frames {:?}",
            self.skipped
        );
        let mut expected = self.expected.clone();
        let mut processed = self.processed.clone();
        expected.sort_unstable();
        processed.sort_unstable();
        assert_eq!(
            processed, expected,
            "ledger FAIL: missing frames (processed {processed:?}, expected {expected:?})"
        );
    }
}

/// Paired production-frontend harness with live encoder histories.
///
/// Both `StreamEncoder`s persist across `step_pair` calls, so block-switch,
/// transform, pre-echo, TNS, and MS-Ld histories evolve exactly as in
/// production. Operating point matches the differential harness (VBR-5,
/// per-channel bitrates) on both arms.
pub(super) struct LiveHarness {
    q_enc: StreamEncoder,
    f_enc: StreamEncoder,
    pcm: Vec<Vec<i16>>,
    channels: usize,
    ledger: FrameLedger,
}

impl LiveHarness {
    pub(super) fn new(fixture: &str, expected: Vec<u32>) -> Self {
        let pcm = testdata::fixture_samples(fixture);
        let sr = testdata::fixture_sample_rate(fixture);
        let channels = pcm.len();
        // Q31 arm: default constructor (default engine is Q31).
        let q_enc = StreamEncoder::with_mode(sr, channels, VbrMode::Vbr5);
        // FPE arm: identical config except the engine knob.
        let cfg = EncoderConfig::new(sr, channels as u32, FRAME_LENGTH, TransportType::Adts)
            .expect("cfg")
            .with_vbr_mode(VbrMode::Vbr5)
            .with_psy_engine(PsyEngineKind::Fpe);
        let f_enc = StreamEncoder::with_config(cfg);
        Self {
            q_enc,
            f_enc,
            pcm,
            channels,
            ledger: FrameLedger::new(expected),
        }
    }

    fn fresh1024(&self, frame: u32) -> Vec<[i16; 1024]> {
        (0..self.channels)
            .map(|c| {
                let mut block = [0i16; 1024];
                for (i, v) in block.iter_mut().enumerate() {
                    let t = frame as usize * 1024 + i;
                    *v = if t < self.pcm[c].len() {
                        self.pcm[c][t]
                    } else {
                        0
                    };
                }
                block
            })
            .collect()
    }

    /// Step both production frontends on the same PCM frame and wrap the
    /// owned outputs. A frontend structural note panics loudly (a skipped
    /// production frame is never silently counted).
    pub(super) fn step_pair(&mut self, frame: u32) -> (TaggedLiveInputs, TaggedLiveInputs) {
        let fresh = self.fresh1024(frame);
        let q_fe = self
            .q_enc
            .frontend(frame, fresh.clone())
            .unwrap_or_else(|note| panic!("Q31 frontend f{frame}: {note}"));
        let f_fe = self
            .f_enc
            .frontend(frame, fresh)
            .unwrap_or_else(|note| panic!("FPE frontend f{frame}: {note}"));
        let q = TaggedLiveInputs {
            tag: EngineTag::Q31,
            fe: q_fe,
            frame,
        };
        let f = TaggedLiveInputs {
            tag: EngineTag::Fpe,
            fe: f_fe,
            frame,
        };
        assert_live_independent(&q, &f);
        assert_live_geometry(&q.fe, &f.fe, frame);
        self.ledger.record(frame);
        (q, f)
    }

    pub(super) fn validate_complete(&self) {
        self.ledger.validate_complete();
    }
}

/// Assert the two frontend arms are genuinely independent: opposite tags,
/// equal channel/sequence geometry, and disjoint storage for every mutable
/// buffer the downstream working set retains (F2).
fn assert_live_independent(q: &TaggedLiveInputs, f: &TaggedLiveInputs) {
    let frame = q.frame;
    assert_eq!(q.tag, EngineTag::Q31, "f{frame}: Q31 arm tag");
    assert_eq!(f.tag, EngineTag::Fpe, "f{frame}: FPE arm tag");
    assert_eq!(q.frame, f.frame, "f{frame}: frame identity");
    assert_eq!(q.fe.live.len(), f.fe.live.len(), "f{frame}: channel count");
    assert_eq!(q.fe.seqs, f.fe.seqs, "f{frame}: sequences agree");
    assert_eq!(q.fe.groupings, f.fe.groupings, "f{frame}: groupings agree");
    for ch in 0..q.fe.live.len() {
        let (ql, fl) = (&q.fe.live[ch], &f.fe.live[ch]);
        assert_disjoint(
            ql.spec_q31(),
            fl.spec_q31(),
            &format!("f{frame} ch{ch} spec_q31"),
        );
        assert_disjoint(
            ql.e_stored(),
            fl.e_stored(),
            &format!("f{frame} ch{ch} e_stored"),
        );
        assert_disjoint(
            ql.t_stored(),
            fl.t_stored(),
            &format!("f{frame} ch{ch} t_stored"),
        );
        assert_disjoint(ql.spread(), fl.spread(), &format!("f{frame} ch{ch} spread"));
        assert_disjoint(ql.e_ld(), fl.e_ld(), &format!("f{frame} ch{ch} e_ld"));
        assert_disjoint(ql.t_ld(), fl.t_ld(), &format!("f{frame} ch{ch} t_ld"));
        assert_disjoint(
            ql.offsets(),
            fl.offsets(),
            &format!("f{frame} ch{ch} offsets"),
        );
        assert_disjoint(ql.ff(), fl.ff(), &format!("f{frame} ch{ch} ff"));
    }
    match (&q.fe.ms, &f.fe.ms) {
        (Some(qm), Some(fm)) => {
            for w in 0..qm.mid.len() {
                assert_disjoint(&qm.mid[w], &fm.mid[w], &format!("f{frame} ms.mid w{w}"));
                assert_disjoint(&qm.side[w], &fm.side[w], &format!("f{frame} ms.side w{w}"));
                assert_disjoint(
                    &qm.mid_ld[w],
                    &fm.mid_ld[w],
                    &format!("f{frame} ms.mid_ld w{w}"),
                );
                assert_disjoint(
                    &qm.side_ld[w],
                    &fm.side_ld[w],
                    &format!("f{frame} ms.side_ld w{w}"),
                );
            }
        }
        (None, None) => {}
        _ => panic!("f{frame}: MS presence differs between arms"),
    }
    for (side, qg, fg) in [
        ("l", &q.fe.ms_grouped_l, &f.fe.ms_grouped_l),
        ("r", &q.fe.ms_grouped_r, &f.fe.ms_grouped_r),
    ] {
        match (qg, fg) {
            (Some(qg), Some(fg)) => {
                assert_disjoint(
                    &qg.energies,
                    &fg.energies,
                    &format!("f{frame} grouped_{side} energies"),
                );
                assert_disjoint(&qg.ld, &fg.ld, &format!("f{frame} grouped_{side} ld"));
            }
            (None, None) => {}
            _ => panic!("f{frame}: grouped_{side} presence differs between arms"),
        }
    }
}

/// Assert exact representation geometry between the arms (F1): the integer
/// lane is authoritative and shared, so counts, grouping, offsets,
/// `mdct_scale`, and the common max must match bit-exactly. Stored
/// value vectors are NOT compared here — numerical equivalence is owned by
/// the R1 envelope and later downstream stages.
fn assert_live_geometry(q: &FrontendOut, f: &FrontendOut, frame: u32) {
    assert_eq!(q.live.len(), f.live.len(), "f{frame}: channel count");
    assert_eq!(q.max_common, f.max_common, "f{frame}: max_common");
    for ch in 0..q.live.len() {
        let (ql, fl) = (&q.live[ch], &f.live[ch]);
        assert_eq!(ql.cnt(), fl.cnt(), "f{frame} ch{ch}: cnt");
        assert_eq!(ql.per(), fl.per(), "f{frame} ch{ch}: per");
        assert_eq!(ql.short(), fl.short(), "f{frame} ch{ch}: short");
        assert_eq!(
            ql.mdct_scale(),
            fl.mdct_scale(),
            "f{frame} ch{ch}: mdct_scale"
        );
        assert_eq!(ql.offsets(), fl.offsets(), "f{frame} ch{ch}: offsets");
        assert_eq!(ql.spec_q31().len(), 1024, "f{frame} ch{ch}: spec span");
        for (name, v) in [
            ("e_stored", ql.e_stored()),
            ("t_stored", ql.t_stored()),
            ("spread", ql.spread()),
            ("e_ld", ql.e_ld()),
            ("t_ld", ql.t_ld()),
        ] {
            assert_eq!(v.len(), ql.cnt(), "f{frame} ch{ch}: {name} span");
            assert_eq!(
                fl_len(name, fl),
                ql.cnt(),
                "f{frame} ch{ch}: FPE {name} span"
            );
        }
        assert_eq!(
            ql.offsets().len(),
            ql.cnt() + 1,
            "f{frame} ch{ch}: offsets span"
        );
        // Grouped MS spans track the live band span (short only).
        if ql.short() {
            for (side, g) in [("l", &q.ms_grouped_l), ("r", &q.ms_grouped_r)] {
                if let Some(g) = g {
                    assert_eq!(
                        g.energies.len(),
                        ql.cnt(),
                        "f{frame} ch{ch}: grouped_{side} span"
                    );
                    assert_eq!(
                        g.ld.len(),
                        ql.cnt(),
                        "f{frame} ch{ch}: grouped_{side} ld span"
                    );
                }
            }
        }
    }
}

/// Length of a named `LiveChannel` vector on one arm (span cross-check helper).
fn fl_len(name: &str, fl: &crate::aac_enc::stream_driver::LiveChannel) -> usize {
    match name {
        "e_stored" => fl.e_stored().len(),
        "t_stored" => fl.t_stored().len(),
        "spread" => fl.spread().len(),
        "e_ld" => fl.e_ld().len(),
        "t_ld" => fl.t_ld().len(),
        _ => unreachable!(),
    }
}

/// Per-arm structural validation of real production output (F1): spans, the
/// empty-`ff` stage invariant (form factors are filled later at S1), the
/// threshold-Ld floor, the zero-energy Ld sentinel, and MS/grouped
/// presence rules. No Q31-vs-FPE value comparison — that is later-stage work.
fn validate_live_arm(fe: &FrontendOut, frame: u32, what: &str) {
    /// Threshold-Ld floor `-33/64` in Q31 (`stream_driver.rs`
    /// `THRESHOLD_LD_FLOOR_Q31`).
    const TLD_FLOOR: i32 = -33 * 33_554_432;
    for (ch, l) in fe.live.iter().enumerate() {
        let tag = format!("{what} f{frame} ch{ch}");
        assert_eq!(l.spec_q31().len(), 1024, "{tag}: spec span");
        assert_eq!(l.e_stored().len(), l.cnt(), "{tag}: e_stored span");
        assert_eq!(l.t_stored().len(), l.cnt(), "{tag}: t_stored span");
        assert_eq!(l.spread().len(), l.cnt(), "{tag}: spread span");
        assert_eq!(l.e_ld().len(), l.cnt(), "{tag}: e_ld span");
        assert_eq!(l.t_ld().len(), l.cnt(), "{tag}: t_ld span");
        assert_eq!(l.offsets().len(), l.cnt() + 1, "{tag}: offsets span");
        assert!(
            l.ff().is_empty(),
            "{tag}: ff filled before S1 (frontend leaves it empty)"
        );
        for b in 0..l.cnt() {
            assert!(
                l.t_ld()[b] >= TLD_FLOOR,
                "{tag} b{b}: threshold Ld floor (got {})",
                l.t_ld()[b]
            );
            // Zero-energy Ld sentinel, short windows only: short `e_ld` is
            // exactly `ld_of(&e_stored)`, so a zero stored band must read
            // `i32::MIN`. Long `e_ld` is NOT a pure function of `e_stored` —
            // it is the float-lane rescale Ld (raw-energy guard plus the
            // unconditional `finalShift` lift, `energy.rs:419-435` (Q31) /
            // `fpe/energy.rs:122-136` (FPE), called from `psy.rs:767,777` /
            // `fpe/orchestrate.rs:342,361`, plus the zero-guard-free TNS
            // overflow correction, `psy.rs:1241-1245`) — so a zero stored
            // band may resolve a non-`MIN` Ld there (e.g. observed
            // `-1087434918` for a zero band). Long-lane values are
            // later-stage value territory; no sentinel is asserted for them.
            if l.short() && l.e_stored()[b] == 0 {
                assert_eq!(l.e_ld()[b], i32::MIN, "{tag} b{b}: zero-energy Ld sentinel");
            }
        }
    }
    if fe.live.iter().any(|l| l.short()) {
        assert!(
            fe.ms_grouped_l.is_some() || fe.ms_grouped_r.is_some(),
            "{what} f{frame}: short frame without grouped MS"
        );
    }
}

// ---------------------------------------------------------------------------
// Psy-output layer (supporting): buffers consumed by frontend construction
// ---------------------------------------------------------------------------

/// Independently owned psy-output input set for one engine arm.
///
/// Built with **fresh per-call engine channel state**: each call constructs
/// new `PsyChannel`/`FpeChannel` values. Block-switch/transform context is
/// replayed from frame 0 to the target frame, but the fresh channel state
/// means pre-echo, TNS, and MS-Ld histories do **not** match a real
/// continuous run at an arbitrary target frame. No state is shared between
/// the arms or carried across calls. Used only for buffers the production
/// frontend consumes internally (`maxScale`, `nm1`, per-window vectors,
/// grouped internals) — the live-history boundary is the frontend layer
/// above.
pub(super) struct TaggedPsyInputs {
    pub tag: EngineTag,
    /// Post-engine mutated spectra (engine-owned copies, one per channel).
    pub spectra: Vec<[f64; 1024]>,
    /// Per-channel psy outputs (owned, one per channel).
    pub outputs: Vec<ChannelOutput>,
    /// Element MS output (stereo only).
    pub ms: Option<MsOutput>,
    /// Window sequences and groupings fed to the engines.
    pub seqs: Vec<WindowSequence>,
    /// Groupings fed to the engines (retained for later stage wiring).
    #[allow(dead_code)]
    pub groupings: Vec<(u8, [u8; 4])>,
    /// Fixture and frame identity (frame accounting).
    #[allow(dead_code)]
    pub fixture: String,
    #[allow(dead_code)]
    pub frame: u32,
}

/// Paired psy-output comparison: exact count of frames whose both arms were
/// built and checked. A skipped or missing frame never counts. (The
/// multi-frame ledger lives on `LiveHarness`; this counter serves the
/// single-frame supporting checks.)
#[derive(Default)]
pub(super) struct DecisionHarness {
    pub frames_compared: usize,
}

impl DecisionHarness {
    /// Build both arms for (`fixture`, `frame`) from two **fully independent**
    /// frontends (no shared mutable state between arms) and record the frame.
    pub(super) fn compare_pair(
        &mut self,
        fixture: &str,
        frame: u32,
    ) -> (TaggedPsyInputs, TaggedPsyInputs) {
        let q = build_tagged_inputs(EngineTag::Q31, fixture, frame);
        let f = build_tagged_inputs(EngineTag::Fpe, fixture, frame);
        assert_inputs_independent(&q, &f, fixture, frame);
        self.frames_compared += 1;
        (q, f)
    }
}

/// Run an independent frontend deterministically from frame 0 through
/// `frame` (fresh `BlockSwitch`/`ChannelTransform` per call, so the two arms
/// share nothing) and feed the resulting spectra to the selected engine with
/// freshly initialized, equivalent channel state. Only the block-switch /
/// transform context is replayed; engine histories (pre-echo, TNS, MS-Ld)
/// are fresh and do not match a continuous run.
fn build_tagged_inputs(tag: EngineTag, fixture: &str, frame: u32) -> TaggedPsyInputs {
    let channels = testdata::fixture_samples(fixture).len();
    let sr = testdata::fixture_sample_rate(fixture);
    let (ch_br, tns_br) = if channels == 1 {
        (112000, 112000)
    } else {
        (96000, 192000)
    };
    let ((lcfg, _), (scfg, _)) = testdata::load_cfg(sr, ch_br);
    let tables = PsyTables::for_rate(sr, ch_br, tns_br, 19293, lcfg.min_snr_ld, scfg.min_snr_ld)
        .expect("tables build");
    let fixture_data = testdata::fixture_samples(fixture);
    let mut switches: Vec<BlockSwitch> = (0..channels).map(|_| BlockSwitch::new()).collect();
    let mut transforms: Vec<ChannelTransform> =
        (0..channels).map(|_| ChannelTransform::new()).collect();
    let mut spectra: Vec<[f64; 1024]> = Vec::new();
    let mut seqs = Vec::new();
    let mut groupings = Vec::new();
    // Step 0..=frame so the block-switch/transform context matches a real
    // run at the target frame. Engine channel state below is still fresh
    // per call (see struct docs): psy histories do not match a continuous
    // run.
    for f in 0..=frame {
        let fresh: Vec<[i16; 1024]> = (0..channels)
            .map(|c| {
                let mut block = [0i16; 1024];
                for (i, v) in block.iter_mut().enumerate() {
                    let t = f as usize * 1024 + i;
                    *v = if (t) < fixture_data[c].len() {
                        fixture_data[c][t]
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
        spectra.clear();
        seqs.clear();
        groupings.clear();
        for (ch, (tr, sw)) in transforms.iter_mut().zip(switches.iter()).enumerate() {
            let out = tr
                .process(&fresh[ch], sw.sequence(), sw.shape())
                .expect("mdct");
            let mut spec = [0.0f64; 1024];
            for (d, s) in spec.iter_mut().zip(out.iter()) {
                *d = *s as f64;
            }
            spectra.push(spec);
            seqs.push(sw.sequence());
            groupings.push(sw.grouping());
        }
    }
    // Each arm mutates its own clone of the identical frontend spectra.
    let mut owned: Vec<[f64; 1024]> = spectra.clone();
    let (outputs, ms) = match tag {
        EngineTag::Q31 => {
            let mut ch_state: Vec<PsyChannel> = (0..channels)
                .map(|_| PsyChannel::new(tables.long_cfg.pcm_quant_q31()))
                .collect();
            // Fresh channels carry identical init histories by construction;
            // the single engine call below runs on the target-frame spectra.
            let mut ws = PsyScratch::default();
            let (head, tail) = owned.split_at_mut(1);
            let lin = ChannelInput {
                spectrum: &mut head[0],
                seq: seqs[0],
                grouping: groupings[0],
                is_lfe: false,
            };
            let (ol, or_, ms) = if channels == 2 {
                let rin = ChannelInput {
                    spectrum: &mut tail[0],
                    seq: seqs[1],
                    grouping: groupings[1],
                    is_lfe: false,
                };
                let (pl, pr) = ch_state.split_at_mut(1);
                psy::process_pair_reuse(
                    &mut pl[0],
                    Some(&mut pr[0]),
                    lin,
                    Some(rin),
                    &tables,
                    &mut ws,
                )
            } else {
                let (ol, _, _) =
                    psy::process_pair_reuse(&mut ch_state[0], None, lin, None, &tables, &mut ws);
                (ol, None, None)
            };
            let mut outs = vec![ol];
            if let Some(r) = or_ {
                outs.push(r);
            }
            (outs, ms)
        }
        EngineTag::Fpe => {
            let mut ch_state: Vec<FpeChannel> = (0..channels)
                .map(|_| FpeChannel::new(tables.long_cfg.pcm_quant_q31()))
                .collect();
            let mut ws = FpeScratch::default();
            let (head, tail) = owned.split_at_mut(1);
            let lin = ChannelInput {
                spectrum: &mut head[0],
                seq: seqs[0],
                grouping: groupings[0],
                is_lfe: false,
            };
            let (ol, or_, ms) = if channels == 2 {
                let rin = ChannelInput {
                    spectrum: &mut tail[0],
                    seq: seqs[1],
                    grouping: groupings[1],
                    is_lfe: false,
                };
                let (pl, pr) = ch_state.split_at_mut(1);
                crate::aac_enc::fpe::process_pair_reuse(
                    &mut pl[0],
                    Some(&mut pr[0]),
                    lin,
                    Some(rin),
                    &tables,
                    &mut ws,
                )
            } else {
                let (ol, _, _) = crate::aac_enc::fpe::process_pair_reuse(
                    &mut ch_state[0],
                    None,
                    lin,
                    None,
                    &tables,
                    &mut ws,
                );
                (ol, None, None)
            };
            let mut outs = vec![ol];
            if let Some(r) = or_ {
                outs.push(r);
            }
            (outs, ms)
        }
    };
    TaggedPsyInputs {
        tag,
        spectra: owned,
        outputs,
        ms,
        seqs,
        groupings,
        fixture: fixture.to_string(),
        frame,
    }
}

/// Assert the two psy-output arms are genuinely independent: opposite tags,
/// equal geometry, and disjoint storage for every mutable buffer (F2) —
/// spectra, per-window energies/thresholds/spread/Ld, `maxScale`, `nm1`,
/// grouped vectors including MS halves, and `MsOutput` vectors.
fn assert_inputs_independent(q: &TaggedPsyInputs, f: &TaggedPsyInputs, fixture: &str, frame: u32) {
    let tag = format!("{fixture} f{frame}");
    assert_eq!(q.tag, EngineTag::Q31, "{tag}: Q31 arm tag");
    assert_eq!(f.tag, EngineTag::Fpe, "{tag}: FPE arm tag");
    assert_eq!(q.spectra.len(), f.spectra.len(), "{tag}: channel count");
    assert_eq!(q.outputs.len(), f.outputs.len(), "{tag}: output count");
    assert_eq!(
        q.seqs, f.seqs,
        "{tag}: sequences agree (identical frontend)"
    );
    for ch in 0..q.spectra.len() {
        assert_disjoint(
            &q.spectra[ch],
            &f.spectra[ch],
            &format!("{tag} ch{ch} spectra"),
        );
        assert_psy_outputs_disjoint(&q.outputs[ch], &f.outputs[ch], &format!("{tag} ch{ch}"));
    }
    match (&q.ms, &f.ms) {
        (Some(qm), Some(fm)) => assert_ms_disjoint(qm, fm, &tag),
        (None, None) => {}
        _ => panic!("{tag}: MS presence differs between arms"),
    }
}

/// Disjoint storage over every mutable buffer of one channel's output.
fn assert_psy_outputs_disjoint(q: &ChannelOutput, f: &ChannelOutput, what: &str) {
    for w in 0..q.energies.len() {
        assert_disjoint(
            &q.energies[w],
            &f.energies[w],
            &format!("{what} energies w{w}"),
        );
        assert_disjoint(
            &q.thresholds[w],
            &f.thresholds[w],
            &format!("{what} thresholds w{w}"),
        );
        assert_disjoint(&q.spread[w], &f.spread[w], &format!("{what} spread w{w}"));
        assert_disjoint(
            &q.energy_ld[w],
            &f.energy_ld[w],
            &format!("{what} energy_ld w{w}"),
        );
        assert_disjoint(
            &q.maxscale[w],
            &f.maxscale[w],
            &format!("{what} maxscale w{w}"),
        );
    }
    assert_disjoint(&q.nm1, &f.nm1, &format!("{what} nm1"));
    match (&q.grouped, &f.grouped) {
        (Some(qg), Some(fg)) => {
            assert_disjoint(
                &qg.energies,
                &fg.energies,
                &format!("{what} grouped.energies"),
            );
            assert_disjoint(
                &qg.thresholds,
                &fg.thresholds,
                &format!("{what} grouped.thresholds"),
            );
            assert_disjoint(&qg.spread, &fg.spread, &format!("{what} grouped.spread"));
            assert_disjoint(
                &qg.energy_ld,
                &fg.energy_ld,
                &format!("{what} grouped.energy_ld"),
            );
            assert_disjoint(
                &qg.threshold_ld,
                &fg.threshold_ld,
                &format!("{what} grouped.threshold_ld"),
            );
            assert_disjoint(&qg.offsets, &fg.offsets, &format!("{what} grouped.offsets"));
            assert_disjoint(&qg.min_snr, &fg.min_snr, &format!("{what} grouped.min_snr"));
            assert_disjoint(
                &qg.spectrum,
                &fg.spectrum,
                &format!("{what} grouped.spectrum"),
            );
            assert_disjoint(
                &qg.ms_energies,
                &fg.ms_energies,
                &format!("{what} grouped.ms_energies"),
            );
            assert_disjoint(&qg.ms_ld, &fg.ms_ld, &format!("{what} grouped.ms_ld"));
        }
        (None, None) => {}
        _ => panic!("{what}: grouped presence differs between arms"),
    }
    match (&q.threshold_ld, &f.threshold_ld) {
        (Some(qt), Some(ft)) => assert_disjoint(qt, ft, &format!("{what} threshold_ld")),
        (None, None) => {}
        _ => panic!("{what}: threshold_ld presence differs between arms"),
    }
}

/// Disjoint storage over an element `MsOutput`.
fn assert_ms_disjoint(q: &MsOutput, f: &MsOutput, what: &str) {
    for w in 0..q.mid.len() {
        assert_disjoint(&q.mid[w], &f.mid[w], &format!("{what} ms.mid w{w}"));
        assert_disjoint(&q.side[w], &f.side[w], &format!("{what} ms.side w{w}"));
        assert_disjoint(
            &q.mid_ld[w],
            &f.mid_ld[w],
            &format!("{what} ms.mid_ld w{w}"),
        );
        assert_disjoint(
            &q.side_ld[w],
            &f.side_ld[w],
            &format!("{what} ms.side_ld w{w}"),
        );
    }
}

/// Isolated-stage harness entry point: take the two independent arms plus
/// independently cloned initial stage state, verify ownership/tags and
/// state equivalence, and hand them to a future stage closure. The closure
/// is a no-op ownership probe in this phase — no downstream algorithm runs
/// here.
pub(super) fn isolated_stage<R>(
    q: &TaggedPsyInputs,
    f: &TaggedPsyInputs,
    q_state: AtsElement,
    f_state: AtsElement,
    stage: impl Fn(&TaggedPsyInputs, &AtsElement) -> R,
) -> (R, R) {
    assert_eq!(q.tag, EngineTag::Q31, "isolated stage: Q31 tag");
    assert_eq!(f.tag, EngineTag::Fpe, "isolated stage: FPE tag");
    // The two initial states must start equivalent (field-by-field); they
    // are owned clones, so later mutation cannot cross arms.
    assert_ats_eq(&q_state, &f_state, "isolated stage initial state");
    (stage(q, &q_state), stage(f, &f_state))
}

/// Sequential-pipeline harness entry point: carry each arm's owned state
/// through an ordered list of future stage names. Snapshots are owned
/// clones, so mutation cannot cross arms; cross-arm equivalence of the
/// initial snapshots is asserted by the caller test. Stage bodies are
/// deferred to later phases.
pub(super) struct SequentialArm {
    pub tag: EngineTag,
    pub stages_seen: Vec<String>,
    /// Per-stage state snapshots (owned, cloned per stage).
    pub snapshots: Vec<AtsElement>,
}

impl SequentialArm {
    pub(super) fn new(tag: EngineTag, initial: AtsElement) -> Self {
        Self {
            tag,
            stages_seen: Vec::new(),
            snapshots: vec![initial],
        }
    }

    /// Record passing through `stage` with an owned clone of the current
    /// state (no mutation of the other arm possible by construction).
    pub(super) fn pass(&mut self, stage: &str) {
        let cur = self.snapshots.last().expect("initial state").clone();
        self.snapshots.push(cur);
        self.stages_seen.push(stage.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::config::VbrMode;

    fn test_ats() -> AtsElement {
        AtsElement::for_vbr_mode(VbrMode::Vbr6, 0)
    }

    // --- Frontend layer (F1/F2/F3) ---

    #[test]
    fn live_boundary_geometry_and_spans_agree() {
        // Real production boundary on both arms: exact representation
        // geometry, valid spans, sentinel/floor invariants per arm.
        let mut h = LiveHarness::new("sine-stereo", vec![1]);
        let (q, f) = h.step_pair(1);
        validate_live_arm(&q.fe, 1, "Q31");
        validate_live_arm(&f.fe, 1, "FPE");
        // Cross-arm TNS check for this frame (sine f1 case): the reused-TNS
        // integer history must match bit-exactly. Per-frame TNS state
        // evolution over sequences is covered frame-by-frame by the
        // differential harness (`mod.rs` `assert_psy_state_eq` checks
        // `tns_state`/`tns_info`/MS-Ld after every frame).
        for ch in 0..q.fe.live.len() {
            assert_tns_eq(
                q.fe.live[ch].tns(),
                f.fe.live[ch].tns(),
                &format!("live tns ch{ch}"),
            );
        }
        h.validate_complete();
    }

    #[test]
    fn live_boundary_short_window_uses_grouped_canonical_ms() {
        // Short-window frames: grouped MS halves travel with the arms with
        // exact grouped spans. One harness steps frames 0..40 with live
        // histories (a fresh encoder stepped at a single frame never sees
        // the attack history that triggers SHORT); the first short pair is
        // retained for the span assertions.
        let mut h = LiveHarness::new("onset-stereo", (0..40).collect());
        let mut short_pair: Option<(TaggedLiveInputs, TaggedLiveInputs)> = None;
        for frame in 0..40 {
            let (q, f) = h.step_pair(frame);
            validate_live_arm(&q.fe, frame, "Q31");
            validate_live_arm(&f.fe, frame, "FPE");
            if short_pair.is_none() && q.fe.live[0].short() && f.fe.live[0].short() {
                short_pair = Some((q, f));
            }
        }
        h.validate_complete();
        let (q, f) = short_pair.expect("onset-stereo must reach a short frame");
        let frame = q.frame;
        let (ql, fl) = (&q.fe.live[0], &f.fe.live[0]);
        assert_eq!(ql.cnt(), fl.cnt(), "short cnt");
        for (side, qg, fg) in [
            ("l", &q.fe.ms_grouped_l, &f.fe.ms_grouped_l),
            ("r", &q.fe.ms_grouped_r, &f.fe.ms_grouped_r),
        ] {
            match (qg, fg) {
                (Some(qg), Some(fg)) => {
                    assert_eq!(qg.energies.len(), ql.cnt(), "grouped_{side} span");
                    assert_eq!(fg.energies.len(), fl.cnt(), "FPE grouped_{side} span");
                }
                _ => panic!("short frame without grouped_{side} on both arms"),
            }
        }
        validate_live_arm(&q.fe, frame, "Q31");
        validate_live_arm(&f.fe, frame, "FPE");
        h.validate_complete();
    }

    #[test]
    fn canary_live_energy_q31_to_fpe() {
        // Poison Q31 live energy on the actual paired arm; the FPE arm's
        // stored data must remain byte-identical to its pre-poison snapshot.
        let mut h = LiveHarness::new("sine-stereo", vec![3]);
        let (mut q, f) = h.step_pair(3);
        let f_snap: Vec<i32> = f.fe.live[0].e_stored().to_vec();
        let q_snap: Vec<i32> = q.fe.live[0].e_stored().to_vec();
        for v in q.fe.live[0].e_stored_mut().iter_mut() {
            *v = v.wrapping_add(0x3FFF_FFFF);
        }
        assert_ne!(
            q.fe.live[0].e_stored(),
            q_snap.as_slice(),
            "poison must take effect on the Q31 arm"
        );
        assert_eq!(
            f.fe.live[0].e_stored(),
            f_snap.as_slice(),
            "FPE arm energy changed by Q31 poison"
        );
        h.validate_complete();
    }

    #[test]
    fn canary_live_energy_fpe_to_q31() {
        // Poison FPE live energy on the actual paired arm; the Q31 arm's
        // stored data must remain byte-identical to its pre-poison snapshot.
        let mut h = LiveHarness::new("sine-stereo", vec![3]);
        let (q, mut f) = h.step_pair(3);
        let q_snap: Vec<i32> = q.fe.live[0].e_stored().to_vec();
        let f_snap: Vec<i32> = f.fe.live[0].e_stored().to_vec();
        for v in f.fe.live[0].e_stored_mut().iter_mut() {
            *v = v.wrapping_add(0x3FFF_FFFF);
        }
        assert_ne!(
            f.fe.live[0].e_stored(),
            f_snap.as_slice(),
            "poison must take effect on the FPE arm"
        );
        assert_eq!(
            q.fe.live[0].e_stored(),
            q_snap.as_slice(),
            "Q31 arm energy changed by FPE poison"
        );
        h.validate_complete();
    }

    #[test]
    fn canary_live_spectrum_both_directions() {
        // Spectrum poisoning on the real boundary, both directions.
        let mut h = LiveHarness::new("sine-stereo", vec![5]);
        let (mut q, mut f) = h.step_pair(5);
        let q_snap: Vec<i32> = q.fe.live[0].spec_q31().to_vec();
        let f_snap: Vec<i32> = f.fe.live[0].spec_q31().to_vec();
        for v in q.fe.live[0].spec_q31_mut().iter_mut() {
            *v = v.wrapping_add(1);
        }
        assert_eq!(
            f.fe.live[0].spec_q31(),
            f_snap.as_slice(),
            "FPE spectrum changed by Q31 poison"
        );
        for v in f.fe.live[0].spec_q31_mut().iter_mut() {
            *v = v.wrapping_add(2);
        }
        assert_eq!(
            q.fe.live[0].spec_q31(),
            q_snap
                .iter()
                .map(|v| v.wrapping_add(1))
                .collect::<Vec<_>>()
                .as_slice(),
            "Q31 spectrum changed by FPE poison (beyond its own poison)"
        );
        h.validate_complete();
    }

    #[test]
    fn crossed_live_inputs_rejected_by_harness() {
        // Two Q31 arms presented as a (Q31, FPE) pair must be rejected by
        // the REAL harness validator, not by a test-local tag assert.
        let mut h1 = LiveHarness::new("sine-stereo", vec![7]);
        let mut h2 = LiveHarness::new("sine-stereo", vec![7]);
        let (q1, _) = h1.step_pair(7);
        let (q2, _) = h2.step_pair(7);
        assert_eq!(q1.tag, EngineTag::Q31);
        assert_eq!(q2.tag, EngineTag::Q31);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_live_independent(&q1, &q2);
        }));
        assert!(res.is_err(), "harness must reject crossed live inputs");
        h1.validate_complete();
        h2.validate_complete();
    }

    #[test]
    fn multiframe_live_histories_validate_every_frame() {
        // Genuine multi-frame run with live encoder histories (F3): both
        // production encoders persist across 31 frames of onset audio. Every
        // expected frame is recorded exactly once; completion is validated
        // explicitly. Window-sequence coverage asserts each transition
        // state independently — SHORT alone does not imply START/STOP.
        let expected: Vec<u32> = (0..=30).collect();
        let mut h = LiveHarness::new("onset-stereo", expected);
        let mut saw_short = false;
        let mut saw_long = false;
        let mut saw_start = false;
        let mut saw_stop = false;
        for frame in 0..=30 {
            let (q, f) = h.step_pair(frame);
            validate_live_arm(&q.fe, frame, "Q31");
            validate_live_arm(&f.fe, frame, "FPE");
            match q.fe.seqs[0] {
                WindowSequence::Short => saw_short = true,
                WindowSequence::Long => saw_long = true,
                WindowSequence::Start => saw_start = true,
                WindowSequence::Stop => saw_stop = true,
            }
            assert_eq!(
                q.fe.seqs, f.fe.seqs,
                "f{frame}: window sequences must agree across arms"
            );
        }
        h.validate_complete();
        assert!(saw_short, "onset 0..=30 must cover short windows");
        assert!(saw_long, "onset 0..=30 must cover long windows");
        assert!(saw_start, "onset 0..=30 must cover the START transition");
        assert!(saw_stop, "onset 0..=30 must cover the STOP transition");
    }

    #[test]
    fn ledger_rejects_duplicate_frame() {
        let mut l = FrameLedger::new(vec![0, 1, 2]);
        l.record(0);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            l.record(0);
        }));
        assert!(res.is_err(), "duplicate frame must fail");
    }

    #[test]
    fn ledger_rejects_missing_frame_at_completion() {
        let mut l = FrameLedger::new(vec![0, 1, 2]);
        l.record(0);
        l.record(1);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            l.validate_complete();
        }));
        assert!(res.is_err(), "missing frame must fail completion");
    }

    #[test]
    fn ledger_rejects_skipped_frame_at_completion() {
        let mut l = FrameLedger::new(vec![0, 1]);
        l.record(0);
        l.record_skip(1);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            l.validate_complete();
        }));
        assert!(res.is_err(), "skipped frame must fail completion");
    }

    #[test]
    fn ledger_rejects_unexpected_frame() {
        let mut l = FrameLedger::new(vec![0]);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            l.record(9);
        }));
        assert!(res.is_err(), "unexpected frame must fail");
    }

    // --- Psy-output layer (supporting) ---

    #[test]
    fn q31_vs_q31_self_check_is_exact() {
        // Harness self-check: two independently built Q31 arms on identical
        // inputs must agree exactly (representation fields + stored ints).
        let a = build_tagged_inputs(EngineTag::Q31, "sine-stereo", 1);
        let b = build_tagged_inputs(EngineTag::Q31, "sine-stereo", 1);
        assert_eq!(a.tag, EngineTag::Q31);
        assert_eq!(b.tag, EngineTag::Q31);
        assert_eq!(a.spectra.len(), b.spectra.len());
        for ch in 0..a.outputs.len() {
            assert_eq!(
                a.outputs[ch].mdct_scale, b.outputs[ch].mdct_scale,
                "self-check mdct_scale ch{ch}"
            );
            assert_eq!(
                a.outputs[ch].final_shift, b.outputs[ch].final_shift,
                "self-check final_shift ch{ch}"
            );
            assert_eq!(
                a.outputs[ch].tns_shift, b.outputs[ch].tns_shift,
                "self-check tns_shift ch{ch}"
            );
            assert_eq!(
                a.outputs[ch].energies, b.outputs[ch].energies,
                "self-check energies ch{ch}"
            );
        }
    }

    #[test]
    fn q31_fpe_inputs_are_independent() {
        let mut h = DecisionHarness::default();
        let (q, f) = h.compare_pair("sine-stereo", 1);
        assert_eq!(h.frames_compared, 1, "exact frame accounting");
        assert_eq!(q.outputs.len(), f.outputs.len());
        let _ = (&q, &f);
    }

    #[test]
    fn canary_q31_poison_leaves_fpe_untouched() {
        // Poison Q31 psy-output energy on one arm; the paired FPE arm's
        // stored data must remain byte-identical to its pre-poison snapshot.
        let mut h = DecisionHarness::default();
        let (mut q, f) = h.compare_pair("sine-stereo", 1);
        let f_snap = f.outputs[0].energies[0].clone();
        let q_snap = q.outputs[0].energies[0].clone();
        q.outputs[0].energies[0][0] = q.outputs[0].energies[0][0].wrapping_add(1_000_000);
        q.spectra[0][0] += 1.0e6;
        assert_ne!(
            q.outputs[0].energies[0], q_snap,
            "poison must take effect on the Q31 arm"
        );
        assert_eq!(
            f.outputs[0].energies[0], f_snap,
            "FPE arm energy changed by Q31 poison"
        );
        assert!(
            !std::ptr::eq(q.spectra.as_ptr(), f.spectra.as_ptr()),
            "arms share spectrum storage"
        );
    }

    #[test]
    fn canary_fpe_poison_leaves_q31_untouched() {
        // Poison FPE psy-output energy on one arm; the paired Q31 arm's
        // stored data must remain byte-identical to its pre-poison snapshot.
        let mut h = DecisionHarness::default();
        let (q, mut f) = h.compare_pair("sine-stereo", 1);
        let q_snap = q.outputs[0].energies[0].clone();
        let f_snap = f.outputs[0].energies[0].clone();
        f.outputs[0].energies[0][0] = f.outputs[0].energies[0][0].wrapping_add(1_000_000);
        f.spectra[0][0] += 1.0e6;
        assert_ne!(
            f.outputs[0].energies[0], f_snap,
            "poison must take effect on the FPE arm"
        );
        assert_eq!(
            q.outputs[0].energies[0], q_snap,
            "Q31 arm energy changed by FPE poison"
        );
        assert!(
            !std::ptr::eq(f.spectra.as_ptr(), q.spectra.as_ptr()),
            "arms share spectrum storage"
        );
    }

    #[test]
    fn canary_crossed_inputs_detected() {
        // Deliberately crossed arms (both Q31) must be rejected by the REAL
        // harness validator `assert_inputs_independent`.
        let q = build_tagged_inputs(EngineTag::Q31, "sine-stereo", 1);
        let q2 = build_tagged_inputs(EngineTag::Q31, "sine-stereo", 1);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_inputs_independent(&q, &q2, "sine-stereo", 1);
        }));
        assert!(res.is_err(), "crossed inputs must fail");
    }

    #[test]
    fn isolated_stage_carries_cloned_state() {
        let mut h = DecisionHarness::default();
        let (q, f) = h.compare_pair("onset-stereo", 2);
        let (rq, rf) = isolated_stage(&q, &f, test_ats(), test_ats(), |arm, _st| {
            (arm.tag, arm.outputs.len())
        });
        assert_eq!(rq, (EngineTag::Q31, q.outputs.len()));
        assert_eq!(rf, (EngineTag::Fpe, f.outputs.len()));
    }

    #[test]
    fn sequential_pipeline_preserves_arm_separation() {
        let mut h = DecisionHarness::default();
        let (q, f) = h.compare_pair("onset-stereo", 2);
        let stages = ["IS", "MS", "S1", "S2", "S4", "S6"];
        let mut aq = SequentialArm::new(q.tag, test_ats());
        let mut af = SequentialArm::new(f.tag, test_ats());
        // Initial snapshots start equivalent (owned clones on each arm).
        assert_ats_eq(
            &aq.snapshots[0],
            &af.snapshots[0],
            "sequential initial state",
        );
        for s in stages {
            aq.pass(s);
            af.pass(s);
        }
        assert_eq!(aq.stages_seen, stages);
        assert_eq!(af.stages_seen, stages);
        assert_eq!(aq.tag, EngineTag::Q31);
        assert_eq!(af.tag, EngineTag::Fpe);
        assert_eq!(aq.snapshots.len(), stages.len() + 1);
        // Snapshots stay per-arm owned snapshots of the equivalent initial
        // state (each equals the initial state it was cloned from).
        for snap in aq.snapshots.iter().skip(1) {
            assert_ats_eq(snap, &aq.snapshots[0], "sequential Q31 snapshot chain");
        }
        assert_eq!(h.frames_compared, 1, "exact frame accounting");
    }

    #[test]
    fn grouped_output_travels_with_arm() {
        // Short-window frame carries grouped output on both arms. Scan for
        // the first short frame rather than hardcoding an index (frontend
        // histories determine the exact START/SHORT boundary).
        let mut found = None;
        for frame in 0..40 {
            let q = build_tagged_inputs(EngineTag::Q31, "onset-stereo", frame);
            if q.seqs[0] == WindowSequence::Short {
                found = Some(frame);
                break;
            }
        }
        let frame = found.expect("onset-stereo must reach a short frame");
        let mut h = DecisionHarness::default();
        let (q, f) = h.compare_pair("onset-stereo", frame);
        assert_eq!(q.seqs[0], WindowSequence::Short, "test needs short frame");
        assert!(q.outputs[0].grouped.is_some(), "Q31 grouped present");
        assert!(f.outputs[0].grouped.is_some(), "FPE grouped present");
        let (gq, gf) = (
            q.outputs[0].grouped.as_ref().expect("q grouped"),
            f.outputs[0].grouped.as_ref().expect("f grouped"),
        );
        assert_disjoint(&gq.ms_energies, &gf.ms_energies, "grouped ms_energies");
        assert_disjoint(&gq.ms_ld, &gf.ms_ld, "grouped ms_ld");
    }
}
