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
//! Scope: harness and ownership tests for the R2.1-A layer (above). This
//! module does **not** classify IS/MS, PE, S2, S4, S6 or frame outcomes —
//! those belong to later stages. Group C (R2.2 OP-1…OP-6 matrix, below)
//! replays the actual production S1/S2/S4/S6 entries through the reviewed
//! Group A replay functions and records per-OP coverage; outcome
//! classification stays in Group D and is absent here.

use crate::aac_enc::block_switch::{BlockSwitch, WindowSequence};
use crate::aac_enc::config::{EncoderConfig, PsyEngineKind, TransportType, VbrMode, FRAME_LENGTH};
use crate::aac_enc::fpe::{FpeChannel, FpeScratch};
use crate::aac_enc::pe::{vbr_pe_offset, AtsElement};
use crate::aac_enc::psy::{
    self, ChannelInput, ChannelOutput, MsOutput, PsyChannel, PsyScratch, PsyTables,
};
use crate::aac_enc::psy_config::load_cfg_for_profile;
use crate::aac_enc::stream_driver::{FrontendOut, R22Capture, StreamEncoder};
use crate::aac_enc::testdata;
use crate::aac_enc::threshold_vbr::{AH_ACTIVE, AH_INACTIVE, NO_AH};
use crate::aac_enc::tns::TnsChannel;
use crate::aac_enc::transform::ChannelTransform;

use super::is_ms::{probe_narrow, probe_sine};
use super::r22_capture::{
    check_acct_state, check_snapshot_completeness, diff_captures, replay_frame,
};

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
// ===========================================================================
// Group C: OP-1..OP-6 operating-point matrix (R2.2, test-only)
//
// Implements the approved operating-point matrix (design §§4, 6/AC-1/AC-2/
// AC-5, 7 Group C, 10.2/10.3/10.6): for every OP and every required material
// the Q31 and FPE encoders are initialized with the exact approved
// configuration, stepped through the ACTUAL production encoder path
// (`StreamEncoder::step_frame`: frontend -> IS/MS -> S1/S2/S4/S6 -> ADTS),
// and every frame's captured stages are replayed within each arm with the
// reviewed Group A functions (`replay_frame`: S1, S2, S4, S6 bit-exact;
// `check_acct_state`: accounting + frame-state consistency;
// `check_snapshot_completeness`: capture geometry). No second algorithm
// implementation and no simplified oracle exist here: replay calls the
// production decision functions on owned snapshot clones.
//
// Within-arm replay equality is the hard gate (exact `==`/`to_bits()` inside
// the Group A functions). Cross-arm Q31-vs-FPE outcomes are RECORDED per
// frame (`diff_captures`, well-formedness asserted) and never
// blanket-asserted; classification (R/A/S/P/F) belongs to Group D and is
// absent here. No evidence files are written (Group E owns `docs/evidence/`,
// which Group C must not create). No recurrence analysis (Group F).
//
// Operating-point matrix (design §4, constraints §10):
//
// | OP  | VBR  | rate  | ch | material                                     |
// |-----|------|-------|----|----------------------------------------------|
// | OP-1| Vbr5 | 48k*  | 2  | 13 synthetic probes (existing corpus)        |
// | OP-2| Vbr5 | 48k   | 2  | m7-music-stereo-48k (471 f)                  |
// | OP-3| Vbr5 | 48k   | 2  | 3 WAVs x 471 f (pauses/clean/eroica)         |
// | OP-4| Vbr1 | 48k   | 2  | 4-WAV chain, 4x471 = 1884 f, no reset        |
// | OP-5| Vbr3 | 44.1k | 2  | sine/onset-stereo-44100 fixtures             |
// | OP-6| Vbr5 | 48k   | 1  | click-mono + impulse-mono fixtures           |
//
// * OP-1's 13th probe (`probe-panned-44100`) is natively 44.1 kHz PCM (the
//   R2.1-B corpus holds 12x48k + 3x44.1k entries; OP-5 takes the two 44.1k
//   fixtures, leaving 13 entries for OP-1). It runs under OP-1's Vbr5 at
//   its native rate; the rate is attributed per material row rather than
//   resampled (resampling would rebuild the corpus, which is forbidden).
//
// Vbr2/Vbr4 are deliberately excluded (OD-R22-6: bracketing profiles cover
// the code paths; only constants drift). A single 44.1 kHz operating point
// (OP-5) is deliberate. OP-4's chain steps 4x471 material blocks on ONE
// persistent encoder pair (frame counter runs 0..1884 across blocks, no
// reset). Each material is encoded independently at the sample-input level:
// frames slice one 1024-sample block from the current material only, and
// each block contributes its own zero-padded tail frames. Encoder state
// (block switch, psy histories, ATS, reservoir, padding) remains continuous
// across the block boundaries at 471, 942 and 1413; the persistent encoder
// pair is never reset.
// ===========================================================================

/// One Group C material: canonical identity, native PCM, native rate, and
/// the exact frame count the harness steps (content frames + two
/// zero-padded tail frames, the `DiffStream::run_stream` accounting).
struct OpMaterial {
    /// Canonical identity for rows/report (fixture key, probe display name,
    /// or WAV canonical id per design §10.3).
    id: String,
    pcm: Vec<Vec<i16>>,
    sample_rate: u32,
    frames: u32,
}

/// Resolve a synthetic fixture by its existing `fixture_samples` key. No
/// audio is rebuilt: the committed in-binary resynthesis is consumed as-is.
fn op_fixture_material(id: &str, key: &str, cap_frames: Option<u32>) -> OpMaterial {
    let pcm = testdata::fixture_samples(key);
    let sample_rate = testdata::fixture_sample_rate(key);
    assert!(!pcm.is_empty(), "Group C: empty PCM for {id}");
    assert!(pcm.len() <= 2, "Group C: unexpected channels for {id}");
    let full = pcm[0].len().div_ceil(FRAME_LENGTH) as u32 + 2;
    let frames = cap_frames.map_or(full, |c| full.min(c));
    assert!(frames > 0, "Group C: empty frame range for {id}");
    OpMaterial {
        id: id.to_string(),
        pcm,
        sample_rate,
        frames,
    }
}

/// Resolve an R2.1-B probe (`probe_sine`/`probe_narrow` with the exact
/// recorded arguments — never substituted). Frames follow the same
/// content-plus-two-tail rule.
fn op_probe_material(id: &str, pcm: Vec<Vec<i16>>, sample_rate: u32) -> OpMaterial {
    assert!(!pcm.is_empty(), "Group C: empty probe PCM for {id}");
    let frames = pcm[0].len().div_ceil(FRAME_LENGTH) as u32 + 2;
    OpMaterial {
        id: id.to_string(),
        pcm,
        sample_rate,
        frames,
    }
}

/// Resolve one committed WAV fixture by canonical id (design §10.3),
/// verifying the Group B pins (stereo, 48 kHz, 480 000 samples/ch =
/// 471 AAC frames). A missing or mismatching fixture panics loudly as
/// `MaterialUnavailable`: never substituted, never synthesized.
fn op_wav_material(canonical_id: &str) -> OpMaterial {
    let fix = testdata::R22_WAV_FIXTURES
        .iter()
        .find(|f| f.canonical_id == canonical_id)
        .unwrap_or_else(|| panic!("Group C: unknown WAV canonical id {canonical_id}"));
    let pcm = testdata::fixture_samples(fix.fixture_key);
    assert_eq!(pcm.len(), 2, "Group C: {canonical_id} must be stereo");
    assert_eq!(
        pcm[0].len(),
        480_000,
        "Group C: {canonical_id} sample count drift"
    );
    assert_eq!(
        pcm[1].len(),
        480_000,
        "Group C: {canonical_id} sample count drift ch1"
    );
    assert_eq!(
        testdata::fixture_sample_rate(fix.fixture_key),
        48000,
        "Group C: {canonical_id} rate drift"
    );
    let frames = pcm[0].len().div_ceil(FRAME_LENGTH) as u32 + 2;
    assert_eq!(frames, 471, "Group C: {canonical_id} frame count drift");
    OpMaterial {
        id: canonical_id.to_string(),
        pcm,
        sample_rate: 48000,
        frames,
    }
}

/// OP-1: the 13 synthetic probes of the existing corpus (R2.1-B decision
/// corpus minus the two 44.1 kHz fixtures owned by OP-5). Probe arguments
/// are byte-identical to the recorded R2.1-B corpus.
fn op1_materials() -> Vec<OpMaterial> {
    let out = vec![
        op_fixture_material("sine-stereo", "sine-stereo", None),
        op_fixture_material("onset-stereo", "onset-stereo", None),
        op_fixture_material("stepdown-stereo", "stepdown-stereo", None),
        op_fixture_material("multitone-stereo", "multitone-stereo", None),
        op_fixture_material("tonal-noise-stereo", "tonal-noise-stereo", None),
        op_fixture_material("quiet-sine-stereo", "quiet-sine-stereo", None),
        // silence-stereo is stationary: 8 frames carry the fresh-start
        // settle plus steady LONGs; further frames add no new window,
        // transition, or carrier-path combination.
        op_fixture_material("silence-stereo", "silence-stereo", Some(8)),
        op_probe_material(
            "probe-panned-48k",
            probe_sine(12, 48000.0, 1000.0, 10_000.0, 0.25, 0.0, Some(4)),
            48000,
        ),
        op_probe_material(
            "probe-outphase-48k",
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
        ),
        op_probe_material(
            "probe-centred-48k",
            probe_sine(12, 48000.0, 1000.0, 10_000.0, 1.0, 0.0, Some(4)),
            48000,
        ),
        op_probe_material("probe-narrow-48k", probe_narrow(12, 48000.0, 0.25), 48000),
        op_probe_material(
            "probe-quiet-48k",
            probe_sine(12, 48000.0, 1000.0, 50.0, 0.25, 0.0, Some(4)),
            48000,
        ),
        // The 13th probe is natively 44.1 kHz (see matrix note above). It
        // runs under OP-1's Vbr5 at its native rate; the rate is attributed
        // per material row, and the 44.1 kHz geometry parity (below) covers
        // its keyed constants.
        op_probe_material(
            "probe-panned-44100",
            probe_sine(12, 44100.0, 1000.0, 10_000.0, 0.25, 0.0, Some(4)),
            44100,
        ),
    ];
    assert_eq!(out.len(), 13, "OP-1: thirteen synthetic probes");
    out
}

/// Slice one 1024-sample frame per channel with zero padding past the end
/// of the material (matches the `fresh_frame` production-harness rule).
fn op_frame_slice(pcm: &[Vec<i16>], frame: u32) -> Vec<Vec<i16>> {
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

/// Q31/FPE encoder pair for one operating point at one native rate, plus
/// the resolved configs. Panics on any rejected configuration.
struct OpPair {
    q: StreamEncoder,
    f: StreamEncoder,
}

fn op_encoder_pair(
    op: &str,
    mat: &str,
    vbr: VbrMode,
    sample_rate: u32,
    channels: usize,
) -> (OpPair, EncoderConfig, EncoderConfig) {
    let base = EncoderConfig::new_with_mode(
        sample_rate,
        channels as u32,
        FRAME_LENGTH,
        TransportType::Adts,
        vbr,
    )
    .unwrap_or_else(|e| panic!("{op} {mat}: config rejected ({e})"));
    let q_cfg = base.with_psy_engine(PsyEngineKind::Q31);
    let f_cfg = base.with_psy_engine(PsyEngineKind::Fpe);
    // The two arms receive the intended equivalent initialization: every
    // resolved field agrees except the engine knob itself.
    assert_eq!(q_cfg.sample_rate(), sample_rate, "{op} {mat}: Q31 rate");
    assert_eq!(f_cfg.sample_rate(), sample_rate, "{op} {mat}: FPE rate");
    assert_eq!(q_cfg.channels(), channels as u32, "{op} {mat}: Q31 ch");
    assert_eq!(f_cfg.channels(), channels as u32, "{op} {mat}: FPE ch");
    assert_eq!(q_cfg.frame_length(), FRAME_LENGTH, "{op} {mat}: Q31 fl");
    assert_eq!(f_cfg.frame_length(), FRAME_LENGTH, "{op} {mat}: FPE fl");
    assert_eq!(q_cfg.transport(), TransportType::Adts, "{op} {mat}: Q31 tr");
    assert_eq!(f_cfg.transport(), TransportType::Adts, "{op} {mat}: FPE tr");
    assert_eq!(q_cfg.vbr_mode(), vbr, "{op} {mat}: Q31 mode");
    assert_eq!(f_cfg.vbr_mode(), vbr, "{op} {mat}: FPE mode");
    assert_eq!(
        q_cfg.psy_engine(),
        PsyEngineKind::Q31,
        "{op} {mat}: Q31 eng"
    );
    assert_eq!(
        f_cfg.psy_engine(),
        PsyEngineKind::Fpe,
        "{op} {mat}: FPE eng"
    );
    // Resolved derivations agree (profile table, not invented constants).
    let profile = vbr.profile();
    for (arm, cfg) in [("Q31", q_cfg), ("FPE", f_cfg)] {
        assert_eq!(
            cfg.vbr_qual_factor(),
            profile.qual_factor(),
            "{op} {mat} {arm}: qual_factor"
        );
        assert_eq!(
            cfg.vbr_qual_q31(),
            profile.qual_q31(),
            "{op} {mat} {arm}: qual_q31"
        );
        assert_eq!(
            cfg.configured_bandwidth_hz(),
            profile.bandwidth_hz(),
            "{op} {mat} {arm}: configured bw"
        );
        assert_eq!(
            cfg.effective_bandwidth_hz(),
            profile.bandwidth_hz().min(sample_rate / 2),
            "{op} {mat} {arm}: effective bw"
        );
    }
    let q = StreamEncoder::with_config(q_cfg);
    let f = StreamEncoder::with_config(f_cfg);
    // Encoder-owned init derivations agree across arms (average/max bits,
    // bits-to-PE factor, PE min/max): keyed-constant drift would show here.
    assert_eq!(
        q.init_summary(),
        f.init_summary(),
        "{op} {mat}: init_summary parity"
    );
    println!(
        "OP-INIT {op} mat={mat} vbr={vbr} sr={sample_rate} ch={channels} \
         qual_q31={:#010x} nom_br={} eff_bw={} init_sum={:?}",
        profile.qual_q31(),
        q_cfg.nominal_bitrate(),
        q_cfg.effective_bandwidth_hz(),
        q.init_summary(),
    );
    (OpPair { q, f }, q_cfg, f_cfg)
}

/// Frame-0 ATS parity: `peOffset` and `vbrQualFactor` are the only ATS
/// fields that vary by operating point (production `AdjThrInit` rule); both
/// arms must carry the exact derived values. Expectations come from the
/// production `vbr_pe_offset` function and the committed profile table —
/// never hard-coded.
fn op_assert_frame0_ats(
    op: &str,
    mat: &str,
    vbr: VbrMode,
    q_cap0: &R22Capture,
    f_cap0: &R22Capture,
) {
    let is_mono = q_cap0.s1.ch.len() == 1;
    assert_eq!(
        f_cap0.s1.ch.len(),
        q_cap0.s1.ch.len(),
        "{op} {mat}: ch count"
    );
    let profile = vbr.profile();
    let expect_off = vbr_pe_offset(profile.channel_bitrate(is_mono));
    for (arm, cap) in [("Q31", q_cap0), ("FPE", f_cap0)] {
        assert_eq!(
            cap.state.ats_pre.pe_offset, expect_off,
            "{op} {mat} {arm}: peOffset"
        );
        assert_eq!(
            cap.state.ats_pre.vbr_qual_factor,
            profile.qual_q31(),
            "{op} {mat} {arm}: vbrQualFactor"
        );
    }
    println!(
        "OP-ATS {op} mat={mat} pe_offset={expect_off} qual_q31={:#010x} mono={is_mono}",
        profile.qual_q31(),
    );
}

/// Keyed-constant geometry parity on the first LONG frame of an encoder
/// run: captured SFB geometry must equal the committed `cfg_*` snapshot for
/// the operating point's (rate, per-channel-bitrate) key, on both arms.
/// 44.1 kHz runs additionally report the twin-rate drift (visible, not
/// normalized): whether the 44.1 kHz snapshot differs from the 48 kHz twin
/// for the same key.
fn op_assert_long_geometry(
    op: &str,
    pin_mat: &str,
    pin_frame: u32,
    vbr: VbrMode,
    sample_rate: u32,
    channels: usize,
    q_long: &R22Capture,
    f_long: &R22Capture,
) {
    let is_mono = channels == 1;
    let profile = vbr.profile();
    let ((lcfg, _), _) = load_cfg_for_profile(sample_rate, profile, is_mono);
    assert!(lcfg.sfb_cnt > 0, "{op} {pin_mat}: long snapshot empty");
    assert!(
        lcfg.sfb_active <= lcfg.sfb_cnt,
        "{op} {pin_mat}: active exceeds total"
    );
    for (arm, cap) in [("Q31", q_long), ("FPE", f_long)] {
        for (i, c) in cap.s1.ch.iter().enumerate() {
            assert!(!c.short, "{op} {pin_mat} {arm} ch{i}: pin needs LONG");
            // LONG working sets span the lowpass-limited active bands
            // (`long_ac = tables.long_cfg.sfb_active()`); the band-coded
            // offsets are the matching active prefix of the snapshot.
            assert_eq!(
                c.cnt, lcfg.sfb_active,
                "{op} {pin_mat} {arm} ch{i}: active sfb_cnt"
            );
            assert_eq!(
                c.offsets,
                &lcfg.offsets[..c.cnt + 1],
                "{op} {pin_mat} {arm} ch{i}: band-coded offsets"
            );
        }
        for (i, c) in cap.s2.ch.iter().enumerate() {
            assert_eq!(
                c.min_snr0,
                &lcfg.min_snr_ld[..c.cnt],
                "{op} {pin_mat} {arm} ch{i}: keyed min_snr"
            );
        }
    }
    if sample_rate == 44100 {
        let ((lcfg48, _), _) = load_cfg_for_profile(48000, profile, is_mono);
        let same_offsets = lcfg.offsets == lcfg48.offsets;
        let same_cnt = lcfg.sfb_cnt == lcfg48.sfb_cnt;
        let same_active = lcfg.sfb_active == lcfg48.sfb_active;
        let same_minsnr = lcfg.min_snr_ld == lcfg48.min_snr_ld;
        println!(
            "OP-CFG44100 {op} mat={pin_mat} f={pin_frame} vs48k offsets_same={same_offsets} \
             cnt_same={same_cnt} active_same={same_active} minsnr_same={same_minsnr} \
             active={} active48={}",
            lcfg.sfb_active, lcfg48.sfb_active,
        );
    }
    println!(
        "OP-GEOM {op} pin={pin_mat} f={pin_frame} active_cnt={} ch={channels} sr={sample_rate} \
         pch={} both_arms_ok",
        lcfg.sfb_active,
        profile.channel_bitrate(is_mono),
    );
}

/// Avoid-hole coverage predicates (Group C finalization; production
/// meaning traced from `threshold_vbr.rs` / `adj_thr.cpp`).
///
/// Production states (`threshold_vbr.rs:152-156`, `adj_thr.cpp:307`):
/// `NO_AH = 0`, `AH_INACTIVE = 1`, `AH_ACTIVE = 2`.
///
/// - `ah_is_classified(v)`: `v != NO_AH`. True for both `AH_INACTIVE` and
///   `AH_ACTIVE`. `AH_INACTIVE` is the initial classification assigned by
///   `init_avoid_hole_flag_mono` (`threshold_vbr.rs:412-427`): a band is
///   marked `AH_INACTIVE` when spread does NOT hide its energy AND minSnr
///   allows coding below the threshold; otherwise `NO_AH`. The flag
///   survives into the `R22S2Cap::ah_flag` snapshot unless the later
///   reduction step clamps it.
/// - `ah_is_clamped(v)`: `v == AH_ACTIVE`. True only when the actual
///   avoid-hole threshold clamp engaged in `reduce_thresholds_vbr`
///   (`threshold_vbr.rs:847-876`, `adj_thr.cpp:1108-1312`): the band had
///   `thr_ld >= MIN_LDTHRESH`, `en_ld > thr_ld`, was not already
///   `AH_ACTIVE`, the chaos-gated reduced threshold exceeded
///   `en_ld + minSnr`, and the band was classified (`a[s] != NO_AH`);
///   the flag then flips to `AH_ACTIVE` and the threshold is clamped.
///   Classification alone (`AH_INACTIVE`) does NOT prove this clamp ran.
///
/// Frame-level helpers scan the owned production-replay capture
/// (`R22S2Cap::ah_flag: Vec<Vec<u8>>`, cloned verbatim from the live
/// `adapt_thresholds_vbr` output in `stream_driver.rs`) and report whether
/// ANY band on ANY channel fired. Both arms are evaluated independently;
/// no simulator or oracle-injected value is consulted here.
fn ah_is_classified(v: u8) -> bool {
    // Authoritative classification set is {AH_INACTIVE, AH_ACTIVE};
    // NO_AH is the only non-classified value. The constant is referenced
    // here so the predicate stays tied to the production definition.
    debug_assert!(AH_INACTIVE != NO_AH && AH_ACTIVE != NO_AH && AH_INACTIVE != AH_ACTIVE);
    v != NO_AH
}

fn ah_is_clamped(v: u8) -> bool {
    v == AH_ACTIVE
}

fn ah_frame_classified(ah_flag: &[Vec<u8>]) -> bool {
    ah_flag
        .iter()
        .any(|b| b.iter().any(|&v| ah_is_classified(v)))
}

fn ah_frame_clamped(ah_flag: &[Vec<u8>]) -> bool {
    ah_flag.iter().any(|b| b.iter().any(|&v| ah_is_clamped(v)))
}

/// First-example witness for a carrier regime on each arm (`None` after a
/// full scan = verified absence across every frame, never an assumption).
#[derive(Default)]
struct RegimeFirst {
    q: Option<(String, u32)>,
    f: Option<(String, u32)>,
}

impl RegimeFirst {
    fn note(&mut self, q_arm: bool, mat: &str, frame: u32, fired: bool) {
        if !fired {
            return;
        }
        let slot = if q_arm { &mut self.q } else { &mut self.f };
        if slot.is_none() {
            *slot = Some((format!("{mat} f{frame}"), frame));
        }
    }

    fn fmt(&self) -> String {
        let one = |o: &Option<(String, u32)>| {
            o.as_ref()
                .map_or("absent(all-frames-verified)".to_string(), |(w, _)| {
                    format!("observed@{w}")
                })
        };
        format!("q={} f={}", one(&self.q), one(&self.f))
    }
}

/// Per-OP coverage account: window types, (prev -> cur) transitions
/// (index 4 = INIT, the first frame of an encoder run), MS execution, MS
/// outcome distribution, content-dependent carrier regimes (first example
/// per arm), reservoir/padding extremes, and the recorded cross-arm
/// differential counts. No aggregate-count-only proof: every claim carries
/// a material+frame witness.
///
/// Avoid-hole coverage is split in two independently tracked dimensions
/// (Group C finalization): `ah_classified` (`v != NO_AH`: an avoid-hole
/// classification was assigned, `AH_INACTIVE` or `AH_ACTIVE`) and
/// `ah_clamped` (`v == AH_ACTIVE`: the actual avoid-hole threshold clamp
/// engaged). The two are never combined into a single avoid-hole status.
struct OpCov {
    op: &'static str,
    channels: usize,
    materials: Vec<(String, u32, u32)>,
    frames: u32,
    window_first: [Option<(String, u32)>; 4],
    window_count: [u32; 4],
    trans_first: [[Option<(String, u32)>; 4]; 5],
    ms_exec_q: u32,
    ms_exec_f: u32,
    digest_seen: [bool; 3],
    ms_some: RegimeFirst,
    is_active: RegimeFirst,
    ah_classified: RegimeFirst,
    ah_clamped: RegimeFirst,
    multipass: RegimeFirst,
    crash: RegimeFirst,
    bitres_min: i32,
    bitres_max: i32,
    pad_changed: bool,
    diff_n: [u32; 6],
    diff_first: [Option<(String, u32)>; 6],
}

impl OpCov {
    fn new(op: &'static str, channels: usize) -> Self {
        Self {
            op,
            channels,
            materials: Vec::new(),
            frames: 0,
            window_first: [None, None, None, None],
            window_count: [0; 4],
            trans_first: [
                [None, None, None, None],
                [None, None, None, None],
                [None, None, None, None],
                [None, None, None, None],
                [None, None, None, None],
            ],
            ms_exec_q: 0,
            ms_exec_f: 0,
            digest_seen: [false; 3],
            ms_some: RegimeFirst::default(),
            is_active: RegimeFirst::default(),
            ah_classified: RegimeFirst::default(),
            ah_clamped: RegimeFirst::default(),
            multipass: RegimeFirst::default(),
            crash: RegimeFirst::default(),
            bitres_min: i32::MAX,
            bitres_max: i32::MIN,
            pad_changed: false,
            diff_n: [0; 6],
            diff_first: [None, None, None, None, None, None],
        }
    }
}

/// Mutable per-encoder-run state (fresh encoders reset this; the OP-4 chain
/// carries one across all four WAV blocks).
struct RunState {
    prev_seq: Option<i32>,
    frame0_done: bool,
    long_pin: Option<(String, u32, R22Capture, R22Capture)>,
}

impl RunState {
    fn fresh() -> Self {
        Self {
            prev_seq: None,
            frame0_done: false,
            long_pin: None,
        }
    }
}

fn op_window_name(seq: i32) -> &'static str {
    match seq {
        0 => "LONG",
        1 => "START",
        2 => "SHORT",
        3 => "STOP",
        _ => panic!("window out of range: {seq}"),
    }
}

/// Step one frame on both arms: production encode, capture checks, and
/// within-arm replay of S1/S2/S4/S6 (hard gate), then coverage recording
/// plus the recorded (never asserted-equal) cross-arm differential.
#[allow(clippy::too_many_arguments)]
fn op_step_frame(
    op: &'static str,
    vbr: VbrMode,
    channels: usize,
    cov: &mut OpCov,
    mat: &str,
    frame_no: u32,
    pair: &mut OpPair,
    fresh: &[Vec<i16>],
    st: &mut RunState,
) {
    let tag = format!("{op} {mat} f{frame_no}");
    let (qb, qs) = pair.q.step_frame(frame_no, fresh);
    let (fb, fs) = pair.f.step_frame(frame_no, fresh);
    // A missing, skipped, or noted frame is a loud finding, never a pass:
    // every frame must emit cleanly with a complete capture on both arms.
    assert!(
        qs.notes.is_empty(),
        "{tag} Q31 notes (frame finding): {:?}",
        qs.notes
    );
    assert!(
        fs.notes.is_empty(),
        "{tag} FPE notes (frame finding): {:?}",
        fs.notes
    );
    let qb = qb.unwrap_or_else(|| panic!("{tag} Q31 must emit ADTS"));
    let fb = fb.unwrap_or_else(|| panic!("{tag} FPE must emit ADTS"));
    let _ = (qb, fb);
    // Engine-independent frontend agreement (integer lane + block switch on
    // identical PCM): the window sequence must agree across arms. Value
    // fields are never asserted equal here.
    assert_eq!(qs.seq, fs.seq, "{tag} window sequence agreement");
    let seq = qs.seq;
    assert!((0..=3).contains(&seq), "{tag} seq range");
    // MS path execution follows channel mode, every frame (OP-6 verifies
    // the mono absence explicitly below).
    if channels == 2 {
        assert!(
            qs.ms_cap.is_some() && fs.ms_cap.is_some(),
            "{tag} stereo MS decision must execute"
        );
        cov.ms_exec_q += 1;
        cov.ms_exec_f += 1;
        for (arm_q, snap_d, snap_m) in [
            (true, qs.digest, qs.mask_ones),
            (false, fs.digest, fs.mask_ones),
        ] {
            assert!((0..=2).contains(&snap_d), "{tag} digest range");
            if arm_q {
                cov.digest_seen[snap_d as usize] = true;
            }
            cov.ms_some
                .note(arm_q, mat, frame_no, snap_d != 0 || snap_m > 0);
        }
    } else {
        assert!(
            qs.ms_cap.is_none() && fs.ms_cap.is_none(),
            "{tag} mono must not run the MS decision"
        );
        assert_eq!(qs.digest, 0, "{tag} mono Q31 digest");
        assert_eq!(fs.digest, 0, "{tag} mono FPE digest");
        assert_eq!(qs.mask_ones, 0, "{tag} mono Q31 mask");
        assert_eq!(fs.mask_ones, 0, "{tag} mono FPE mask");
    }
    let cap_q = qs
        .r22_cap
        .as_ref()
        .unwrap_or_else(|| panic!("{tag} Q31 missing R22 capture"));
    let cap_f = fs
        .r22_cap
        .as_ref()
        .unwrap_or_else(|| panic!("{tag} FPE missing R22 capture"));
    // Accounting + frame-state consistency, capture geometry: both arms.
    check_acct_state(cap_q, &qs, &format!("{tag} Q31"));
    check_acct_state(cap_f, &fs, &format!("{tag} FPE"));
    check_snapshot_completeness(cap_q, channels, &format!("{tag} Q31"));
    check_snapshot_completeness(cap_f, channels, &format!("{tag} FPE"));
    // Within-arm replay: production functions on owned clones, bit-exact.
    replay_frame(cap_q, &format!("{tag} Q31"));
    replay_frame(cap_f, &format!("{tag} FPE"));
    if channels == 1 {
        // OP-6 mono path: no stereo MS operands anywhere in the captures,
        // and no IS coding (a stereo-only tool) on either arm.
        assert!(
            cap_q.s1.ms_mask.is_empty() && cap_f.s1.ms_mask.is_empty(),
            "{tag} mono S1 mask must stay empty"
        );
        assert!(
            cap_q.s2.ms_mask.is_none() && cap_f.s2.ms_mask.is_none(),
            "{tag} mono S2 mask must stay absent"
        );
        assert_eq!(qs.is_bands, 0, "{tag} mono Q31 IS bands");
        assert_eq!(fs.is_bands, 0, "{tag} mono FPE IS bands");
    }
    // Frame-0 ATS parity on the first frame of each encoder run.
    if !st.frame0_done {
        op_assert_frame0_ats(op, mat, vbr, cap_q, cap_f);
        st.frame0_done = true;
    }
    // Retain the first LONG frame's captures for keyed-geometry parity.
    if seq == 0 && st.long_pin.is_none() {
        st.long_pin = Some((mat.to_string(), frame_no, cap_q.clone(), cap_f.clone()));
    }
    // Coverage: window type and (prev -> cur) transition with witnesses.
    let w = seq as usize;
    cov.window_count[w] += 1;
    if cov.window_first[w].is_none() {
        cov.window_first[w] = Some((format!("{mat} f{frame_no}"), frame_no));
    }
    let p = st.prev_seq.map_or(4, |s| s as usize);
    assert!(p < 5 && w < 4, "{tag} transition index range");
    if cov.trans_first[p][w].is_none() {
        cov.trans_first[p][w] = Some((format!("{mat} f{frame_no}"), frame_no));
    }
    st.prev_seq = Some(seq);
    // Content-dependent carrier regimes (first example per arm; absence
    // after the full scan is a verified absence, reported as such).
    // Avoid-hole is split: classification (`v != NO_AH`, i.e. AH_INACTIVE
    // or AH_ACTIVE assigned) vs actual clamp (`v == AH_ACTIVE` engaged in
    // `reduce_thresholds_vbr`). Each arm is evaluated independently from
    // its own production-replay capture (`R22S2Cap::ah_flag`); no
    // simulator or oracle-injected value is consulted here.
    cov.is_active.note(true, mat, frame_no, qs.is_bands > 0);
    cov.is_active.note(false, mat, frame_no, fs.is_bands > 0);
    let ah_class_q = ah_frame_classified(&cap_q.s2.ah_flag);
    let ah_class_f = ah_frame_classified(&cap_f.s2.ah_flag);
    cov.ah_classified.note(true, mat, frame_no, ah_class_q);
    cov.ah_classified.note(false, mat, frame_no, ah_class_f);
    let ah_clamp_q = ah_frame_clamped(&cap_q.s2.ah_flag);
    let ah_clamp_f = ah_frame_clamped(&cap_f.s2.ah_flag);
    cov.ah_clamped.note(true, mat, frame_no, ah_clamp_q);
    cov.ah_clamped.note(false, mat, frame_no, ah_clamp_f);
    cov.multipass
        .note(true, mat, frame_no, cap_q.acct.npass > 1);
    cov.multipass
        .note(false, mat, frame_no, cap_f.acct.npass > 1);
    cov.crash.note(true, mat, frame_no, cap_q.acct.crash >= 0);
    cov.crash.note(false, mat, frame_no, cap_f.acct.crash >= 0);
    // Reservoir extremes (Q31 arm; FPE tracked by the diff record) and the
    // content-independent padding evolution.
    cov.bitres_min = cov.bitres_min.min(cap_q.state.qc_post);
    cov.bitres_max = cov.bitres_max.max(cap_q.state.qc_post);
    cov.pad_changed |=
        cap_q.state.pad_pre != cap_q.state.pad_post || cap_f.state.pad_pre != cap_f.state.pad_post;
    // Cross-arm differential: recorded, never blanket-asserted (Group D
    // owns classification). Well-formedness IS asserted: stage flags must
    // be consistent with the differing-fields list.
    let d = diff_captures(cap_q, cap_f, &tag);
    let flagged =
        !d.s1_equal || !d.s2_equal || !d.s4_equal || !d.s6_equal || !d.state_equal || !d.acct_equal;
    assert_eq!(
        flagged,
        !d.differing_fields.is_empty(),
        "{tag} diff record well-formed"
    );
    let stages = [
        (!d.s1_equal, "s1"),
        (!d.s2_equal, "s2"),
        (!d.s4_equal, "s4"),
        (!d.s6_equal, "s6"),
        (!d.state_equal, "state"),
        (!d.acct_equal, "acct"),
    ];
    for (i, (hit, _)) in stages.iter().enumerate() {
        if *hit {
            cov.diff_n[i] += 1;
            if cov.diff_first[i].is_none() {
                cov.diff_first[i] = Some((format!("{mat} f{frame_no}"), frame_no));
            }
        }
    }
    cov.frames += 1;
}

/// Required transition classes per OP (witnessed (prev -> cur) pairs):
/// INIT (first frame), steady LONG, attack onset (-> START), START->SHORT,
/// SHORT release (->STOP or sustained SHORT), STOP recovery (->LONG).
fn op_assert_transition_classes(cov: &OpCov) {
    let op = cov.op;
    let has = |pred: &dyn Fn(usize, usize) -> bool| {
        cov.trans_first
            .iter()
            .enumerate()
            .flat_map(|(p, row)| {
                row.iter()
                    .enumerate()
                    .filter_map(move |(c, o)| o.as_ref().filter(|_| pred(p, c)).map(|w| (p, c, w)))
            })
            .next()
    };
    let show = |p: usize, c: usize, w: &(String, u32)| {
        let ps = if p == 4 {
            "INIT".to_string()
        } else {
            op_window_name(p as i32).to_string()
        };
        format!("{}->{} @{}", ps, op_window_name(c as i32), w.0)
    };
    let require = |name: &str, pred: &dyn Fn(usize, usize) -> bool| match has(pred) {
        Some((p, c, w)) => println!("OP-TRANS {op} class={name} {}", show(p, c, w)),
        None => {
            let mut table = String::new();
            for (p, row) in cov.trans_first.iter().enumerate() {
                for (c, o) in row.iter().enumerate() {
                    let ps = if p == 4 {
                        "INIT"
                    } else {
                        op_window_name(p as i32)
                    };
                    table.push_str(&format!(
                        "\n    {}->{}: {}",
                        ps,
                        op_window_name(c as i32),
                        o.as_ref().map_or("NotApplicable".to_string(), |(w, _)| {
                            format!("observed@{w}")
                        }),
                    ));
                }
            }
            panic!("{op}: transition class {name} never exercised; table:{table}");
        }
    };
    require("init", &|p, _| p == 4);
    require("steady_long", &|p, c| p == 0 && c == 0);
    require("onset", &|_, c| c == 1);
    require("attack", &|p, c| p == 1 && c == 2);
    require("release", &|p, c| p == 2 && (c == 3 || c == 2));
    require("recover", &|p, c| p == 3 && c == 0);
}

/// Per-OP applicability matrix gate + machine-readable report rows.
/// Windows and transition classes are hard-required (every OP's corpus
/// exercises all four window types). Transitions and carrier regimes use a
/// three-state account: Observed (first-example witnesses per arm),
/// NotApplicable (structurally impossible per the production block-switch
/// topology), or Unobserved (structurally possible but absent in this OP's
/// corpus: an explicit limitation, never counted as coverage).
fn op_assert_matrix(vbr: VbrMode, cov: &OpCov) {
    let op = cov.op;
    // Exact frame accounting: every expected frame stepped exactly once.
    let expect: u32 = cov.materials.iter().map(|(_, _, n)| n).sum();
    assert_eq!(cov.frames, expect, "{op}: frame accounting");
    // Window types: all four exercised, with witnesses.
    for seq in 0..4 {
        match &cov.window_first[seq as usize] {
            Some((w, _)) => println!(
                "OP-COVERAGE {op} dim=window val={} first={w} count={}",
                op_window_name(seq),
                cov.window_count[seq as usize],
            ),
            None => panic!(
                "{op}: window {} never exercised (counts {:?})",
                op_window_name(seq),
                cov.window_count,
            ),
        }
    }
    op_assert_transition_classes(cov);
    // Full (prev -> cur) pair table: observed pairs carry witnesses;
    // structurally impossible pairs are NotApplicable with a
    // production-topology reason (NEXT_SEQUENCE / SYNCED_SEQUENCE in
    // `block_switch.rs`); structurally possible but absent pairs are
    // Unobserved limitations (verified absence, never counted as coverage).
    // Reachability per the production topology (pre-sync NEXT_SEQUENCE
    // last_attack/attack/last_sequence table, post-sync SYNCED_SEQUENCE
    // merge; stereo merge cannot introduce successors outside the per-arm
    // set, and mono follows the NEXT_SEQUENCE set directly):
    // LONG -> {LONG, START}; START -> {SHORT}; SHORT -> {SHORT, STOP};
    // STOP -> {LONG, START}; INIT (first frame from initial LONG) ->
    // {LONG, START}.
    let transition_possible = |p: usize, c: usize| match (p, c) {
        (4, 0) | (4, 1) => true,
        (0, 0) | (0, 1) => true,
        (1, 2) => true,
        (2, 2) | (2, 3) => true,
        (3, 0) | (3, 1) => true,
        _ => false,
    };
    let topology_reason = |p: usize, _c: usize| match p {
        4 => {
            "structurally impossible per production block-switch topology: \
             first frame from the initial LONG state takes only LONG (no attack) \
             or START (attack) via NEXT_SEQUENCE in block_switch.rs, merged by \
             SYNCED_SEQUENCE"
        }
        0 => {
            "structurally impossible per NEXT_SEQUENCE/SYNCED_SEQUENCE in \
             block_switch.rs: LONG advances only to LONG (no attack) or START \
             (attack)"
        }
        1 => {
            "structurally impossible per NEXT_SEQUENCE/SYNCED_SEQUENCE in \
             block_switch.rs: START advances to SHORT for every \
             (last_attack, attack) combination"
        }
        2 => {
            "structurally impossible per NEXT_SEQUENCE/SYNCED_SEQUENCE in \
             block_switch.rs: SHORT advances only to SHORT or STOP"
        }
        _ => {
            "structurally impossible per NEXT_SEQUENCE/SYNCED_SEQUENCE in \
             block_switch.rs: STOP advances only to LONG (no attack) or START \
             (attack)"
        }
    };
    for (p, row) in cov.trans_first.iter().enumerate() {
        for (c, o) in row.iter().enumerate() {
            let ps = if p == 4 {
                "INIT"
            } else {
                op_window_name(p as i32)
            };
            match o {
                Some((w, _)) => println!(
                    "OP-APPLIC {op} dim=transition key={ps}->{} status=Observed example={w}",
                    op_window_name(c as i32),
                ),
                None => {
                    if transition_possible(p, c) {
                        println!(
                            "OP-APPLIC {op} dim=transition key={ps}->{} status=Unobserved reason=\"applicable but unobserved: structurally possible per NEXT_SEQUENCE/SYNCED_SEQUENCE in block_switch.rs, verified absence across {} frames (limitation, not coverage)\"",
                            op_window_name(c as i32),
                            cov.frames,
                        );
                    } else {
                        println!(
                            "OP-APPLIC {op} dim=transition key={ps}->{} status=NotApplicable reason=\"{}\"",
                            op_window_name(c as i32),
                            topology_reason(p, c),
                        );
                    }
                }
            }
        }
    }
    // State-carrier paths.
    if cov.channels == 2 {
        assert_eq!(
            cov.ms_exec_q, cov.frames,
            "{op}: stereo MS decision must execute every frame (Q31)"
        );
        assert_eq!(
            cov.ms_exec_f, cov.frames,
            "{op}: stereo MS decision must execute every frame (FPE)"
        );
        println!(
            "OP-APPLIC {op} dim=carrier key=ms_decision status=Observed \
             frames={} digest_none/some/all={:?}",
            cov.frames, cov.digest_seen,
        );
        match &cov.ms_some.q {
            Some((w, _)) => println!(
                "OP-APPLIC {op} dim=carrier key=ms_active status=Observed example={w} ({})",
                cov.ms_some.fmt()
            ),
            None => println!(
                "OP-APPLIC {op} dim=carrier key=ms_active status=Unobserved \
                 reason=\"applicable but unobserved: MS decision executed every frame \
                 but selected NONE throughout (MS_NONE = active non-selection, \
                 verified across {} frames; limitation, not coverage)\"",
                cov.frames
            ),
        }
    } else {
        // OP-6 mono: the stereo MS path is structurally inapplicable, and
        // the driver asserted its absence (empty masks, zero digests, no
        // MS capture) on every frame of both arms.
        assert_eq!(cov.ms_exec_q, 0, "{op}: mono must not execute MS (Q31)");
        assert_eq!(cov.ms_exec_f, 0, "{op}: mono must not execute MS (FPE)");
        println!(
            "OP-APPLIC {op} dim=carrier key=ms_decision status=NotApplicable \
             reason=\"nch==1: no ms_mask, no MS capture, zero digests on all {} frames (both arms asserted)\"",
            cov.frames
        );
        println!(
            "OP-APPLIC {op} dim=carrier key=ms_active status=NotApplicable \
             reason=\"mono element-global PE path; stereo MS processing structurally absent (asserted)\""
        );
    }
    // Content-dependent regimes: observed with witnesses, else Unobserved
    // limitations (structurally possible but absent in this OP's corpus;
    // verified absence, never counted as coverage). The only structural
    // NotApplicable carriers are the mono MS/IS paths (handled above for
    // MS, below for IS).
    //
    // Avoid-hole correction (Group C finalization): the prior remediation
    // report stated `avoid_hole` absent/Unobserved in all six OPs. That was
    // incorrect. The classification dimension (`ah_classified`, `v != NO_AH`)
    // is Observed wherever the production-replay capture contains a genuine
    // witness; classification alone does NOT prove the actual threshold
    // clamp. Clamp coverage (`ah_clamped`, `v == AH_ACTIVE`) is reported
    // separately below. Simulator oracle-injected observations are
    // supporting analysis only, never production-replay witnesses.
    // AC-2 qualification is preserved: coverage accounting discipline is
    // satisfied, but full applicable-path execution is NOT achieved
    // (`s6_multipass`/`s6_crash` unobserved everywhere, `is_active`
    // unobserved in stereo OP-2/3/4/5, documented transition gaps remain
    // unobserved limitations); none of these are represented as coverage.
    let regime = |key: &str, r: &RegimeFirst, why_absent: &str| {
        if r.q.is_some() || r.f.is_some() {
            println!(
                "OP-APPLIC {op} dim=carrier key={key} status=Observed {}",
                r.fmt()
            );
        } else {
            println!(
                "OP-APPLIC {op} dim=carrier key={key} status=Unobserved \
                 reason=\"applicable but unobserved: {why_absent} (verified absence \
                 across {} frames; limitation, not coverage)\"",
                cov.frames
            );
        }
    };
    if cov.channels == 1 {
        println!(
            "OP-APPLIC {op} dim=carrier key=is_active status=NotApplicable \
             reason=\"mono: intensity stereo structurally absent (nch==1; is_bands==0 \
             asserted on all {} frames, both arms)\"",
            cov.frames
        );
    } else {
        regime(
            "is_active",
            &cov.is_active,
            "no right-channel band selected intensity coding in this OP's corpus",
        );
    }
    regime(
        "ah_classified",
        &cov.ah_classified,
        "no band met the avoid-hole classification trigger (AH_INACTIVE/AH_ACTIVE, v != NO_AH) in this OP's corpus",
    );
    regime(
        "ah_clamped",
        &cov.ah_clamped,
        "no band engaged the actual avoid-hole threshold clamp (AH_ACTIVE, v == AH_ACTIVE) in this OP's corpus; classification alone (AH_INACTIVE) does not prove clamping; simulator oracle-injected observations are supporting analysis only, not production-replay witnesses",
    );
    regime(
        "s6_multipass",
        &cov.multipass,
        "single S6 pass sufficed on every frame of this OP's corpus (npass==1 throughout)",
    );
    regime(
        "s6_crash",
        &cov.crash,
        "crash-recovery maxSfb reduction never engaged in this OP's corpus (crash==-1 throughout, production no-crash marker)",
    );
    println!(
        "OP-RESERVOIR {op} bitres_min={} bitres_max={} pad_changed={} vbr={vbr}",
        cov.bitres_min, cov.bitres_max, cov.pad_changed,
    );
    let stages = ["s1", "s2", "s4", "s6", "state", "acct"];
    for (i, s) in stages.iter().enumerate() {
        println!(
            "OP-DIFF {op} stage={s} frames_with_diffs={} first={}",
            cov.diff_n[i],
            cov.diff_first[i]
                .as_ref()
                .map_or("none".to_string(), |(w, _)| w.clone()),
        );
    }
    println!(
        "OP-RESULT {op} vbr={vbr} ch={} materials={} frames={} windows={:?}",
        cov.channels,
        cov.materials.len(),
        cov.frames,
        cov.window_count,
    );
    for (id, sr, n) in &cov.materials {
        println!("OP-MATERIAL {op} id={id} sr={sr} frames={n}");
    }
}

/// Run one operating point over standalone materials (fresh encoder pair
/// per material; histories persist within a material run per the
/// multi-frame capture rule, never across materials).
fn run_op_standalone(
    op: &'static str,
    vbr: VbrMode,
    channels: usize,
    materials: &[OpMaterial],
) -> OpCov {
    let mut cov = OpCov::new(op, channels);
    for mat in materials {
        assert_eq!(mat.pcm.len(), channels, "{op} {}: channel count", mat.id);
        let (mut pair, _, _) = op_encoder_pair(op, &mat.id, vbr, mat.sample_rate, channels);
        let mut st = RunState::fresh();
        for f in 0..mat.frames {
            let fresh = op_frame_slice(&mat.pcm, f);
            op_step_frame(
                op, vbr, channels, &mut cov, &mat.id, f, &mut pair, &fresh, &mut st,
            );
        }
        let (pin_mat, pin_frame, q_long, f_long) = st
            .long_pin
            .as_ref()
            .unwrap_or_else(|| panic!("{op} {}: no LONG frame for geometry pin", mat.id));
        op_assert_long_geometry(
            op,
            pin_mat,
            *pin_frame,
            vbr,
            mat.sample_rate,
            channels,
            q_long,
            f_long,
        );
        cov.materials
            .push((mat.id.clone(), mat.sample_rate, mat.frames));
    }
    op_assert_matrix(vbr, &cov);
    cov
}

/// OP-4: the four WAV materials as ONE continuous chain on one persistent
/// encoder pair (no reset between materials). Four blocks of 471 frames =
/// 1884 frames total; the frame counter runs across blocks so encoder
/// state (block switch, psy histories, ATS, reservoir, padding) evolves
/// continuously, and material boundaries are attributed per frame.
fn run_op4_chain(op: &'static str, wavs: &[OpMaterial]) -> OpCov {
    assert_eq!(wavs.len(), 4, "{op}: four WAV blocks");
    let channels = 2;
    let mut cov = OpCov::new(op, channels);
    let (mut pair, _, _) = op_encoder_pair(op, "chain", VbrMode::Vbr1, 48000, channels);
    let mut st = RunState::fresh();
    let mut global: u32 = 0;
    for (b, mat) in wavs.iter().enumerate() {
        assert_eq!(mat.frames, 471, "{op} {}: block length", mat.id);
        if b > 0 {
            println!(
                "OP-4-BOUNDARY {op} global_f={global} entering={} (encoder NOT reset)",
                mat.id
            );
        }
        for f in 0..mat.frames {
            let fresh = op_frame_slice(&mat.pcm, f);
            op_step_frame(
                op,
                VbrMode::Vbr1,
                channels,
                &mut cov,
                &mat.id,
                global,
                &mut pair,
                &fresh,
                &mut st,
            );
            global += 1;
        }
        cov.materials
            .push((mat.id.clone(), mat.sample_rate, mat.frames));
    }
    assert_eq!(global, 1884, "{op}: chained run must total 1884 frames");
    let (pin_mat, pin_frame, q_long, f_long) = st
        .long_pin
        .as_ref()
        .unwrap_or_else(|| panic!("{op}: no LONG frame for geometry pin"));
    op_assert_long_geometry(
        op,
        pin_mat,
        *pin_frame,
        VbrMode::Vbr1,
        48000,
        channels,
        q_long,
        f_long,
    );
    op_assert_matrix(VbrMode::Vbr1, &cov);
    cov
}

#[cfg(test)]
mod group_c_tests {
    use super::*;

    #[test]
    fn r22_op_init_parity() {
        // Fast gate: every OP configuration (plus the OP-1 44.1 kHz probe
        // rate) resolves to the intended equivalent initialization on both
        // arms, without stepping any audio.
        let cases = [
            ("OP-1/2/3", VbrMode::Vbr5, 48000u32, 2usize),
            ("OP-1-probe44100", VbrMode::Vbr5, 44100, 2),
            ("OP-4", VbrMode::Vbr1, 48000, 2),
            ("OP-5", VbrMode::Vbr3, 44100, 2),
            ("OP-6", VbrMode::Vbr5, 48000, 1),
        ];
        for (label, vbr, sr, ch) in cases {
            let (_pair, q_cfg, f_cfg) = op_encoder_pair(label, "init-probe", vbr, sr, ch);
            // Cross-check the ATS expectations the frame driver asserts:
            // peOffset via the production rule, qualFactor via the profile.
            let is_mono = ch == 1;
            let expect_off = vbr_pe_offset(vbr.profile().channel_bitrate(is_mono));
            let ats = crate::aac_enc::pe::AtsElement::for_vbr_mode(vbr, expect_off);
            assert_eq!(ats.pe_offset, expect_off, "{label}: peOffset");
            assert_eq!(
                ats.vbr_qual_factor,
                vbr.profile().qual_q31(),
                "{label}: qual"
            );
            // Committed snapshots resolve for every OP key (fail-closed on
            // unknown keys; Vbr2/Vbr4 deliberately have no OP here).
            let ((lcfg, _), (scfg, _)) = load_cfg_for_profile(sr, vbr.profile(), is_mono);
            assert!(lcfg.sfb_cnt > 0 && scfg.sfb_cnt > 0, "{label}: snapshots");
            let _ = (q_cfg, f_cfg);
        }
    }

    #[test]
    fn r22_op1_synthetic_probes() {
        let mats = op1_materials();
        // Matrix rim: 12 probe/fixtures at 48 kHz + the single native
        // 44.1 kHz probe (attributed, not resampled).
        assert_eq!(mats.len(), 13, "OP-1: thirteen materials");
        assert_eq!(
            mats.iter().filter(|m| m.sample_rate == 48000).count(),
            12,
            "OP-1: twelve 48 kHz materials"
        );
        assert_eq!(
            mats.iter().filter(|m| m.sample_rate == 44100).count(),
            1,
            "OP-1: one native 44.1 kHz probe"
        );
        let cov = run_op_standalone("OP-1", VbrMode::Vbr5, 2, &mats);
        assert_eq!(cov.materials.len(), 13, "OP-1: material accounting");
    }

    #[test]
    fn r22_op2_music() {
        let mats = [op_wav_material("m7-music-stereo-48k")];
        let cov = run_op_standalone("OP-2", VbrMode::Vbr5, 2, &mats);
        assert_eq!(cov.frames, 471, "OP-2: single-WAV frame count");
    }

    #[test]
    fn r22_op3_speech_music() {
        let mats = [
            op_wav_material("m7-speech-pauses-stereo-48k"),
            op_wav_material("m7-speech-clean-stereo-48k"),
            op_wav_material("m8-music-eroica-stereo-48k"),
        ];
        let cov = run_op_standalone("OP-3", VbrMode::Vbr5, 2, &mats);
        assert_eq!(cov.frames, 3 * 471, "OP-3: three-WAV frame count");
        assert_eq!(cov.materials.len(), 3, "OP-3: material accounting");
    }

    #[test]
    fn r22_op4_vbr1_chain() {
        // The structural opposite (peOffset 50, lowest bitrate): one
        // continuous 1884-frame chain, encoder never reset. Recurrence
        // analysis and D-6 conclusions belong to Group F, not here.
        let wavs = [
            op_wav_material("m7-music-stereo-48k"),
            op_wav_material("m7-speech-pauses-stereo-48k"),
            op_wav_material("m7-speech-clean-stereo-48k"),
            op_wav_material("m8-music-eroica-stereo-48k"),
        ];
        let cov = run_op4_chain("OP-4", &wavs);
        assert_eq!(cov.frames, 1884, "OP-4: chained frame count");
        // Vbr1 stereo takes the peOffset branch (20 kbit/s per channel):
        // the frame-0 ATS asserts inside the driver pin this at 50 via the
        // production rule; re-state the expectation here for the rim.
        assert_eq!(
            vbr_pe_offset(VbrMode::Vbr1.profile().channel_bitrate(false)),
            50,
            "OP-4: Vbr1 stereo peOffset branch"
        );
    }

    #[test]
    fn r22_op5_44100() {
        // The single 44.1 kHz operating point: second sample rate with
        // 44.1k long/short SFB widths + keyed-constant drift (reported by
        // the geometry parity as OP-CFG44100 rows).
        let mats = [
            op_fixture_material("sine-stereo-44100", "sine-stereo-44100", None),
            op_fixture_material("onset-stereo-44100", "onset-stereo-44100", None),
        ];
        for m in &mats {
            assert_eq!(m.sample_rate, 44100, "OP-5 {}: native rate", m.id);
        }
        let cov = run_op_standalone("OP-5", VbrMode::Vbr3, 2, &mats);
        assert_eq!(cov.materials.len(), 2, "OP-5: material accounting");
        // Vbr3 stereo stays on the zero-peOffset branch (48 kbit/s/ch).
        assert_eq!(
            vbr_pe_offset(VbrMode::Vbr3.profile().channel_bitrate(false)),
            0,
            "OP-5: Vbr3 stereo peOffset branch"
        );
    }

    #[test]
    fn r22_op6_mono() {
        // Mono branch (nch == 1): no ms_mask, PE-only element-global
        // decisions; the driver asserts the absence of stereo MS processing
        // on every frame of both arms.
        let mats = [
            op_fixture_material("click-mono", "click-mono", None),
            op_fixture_material("impulse-mono", "impulse-mono", None),
        ];
        for m in &mats {
            assert_eq!(m.sample_rate, 48000, "OP-6 {}: native rate", m.id);
        }
        let cov = run_op_standalone("OP-6", VbrMode::Vbr5, 1, &mats);
        assert_eq!(cov.materials.len(), 2, "OP-6: material accounting");
        assert_eq!(cov.ms_exec_q, 0, "OP-6: no MS execution (Q31)");
        assert_eq!(cov.ms_exec_f, 0, "OP-6: no MS execution (FPE)");
    }

    #[test]
    fn r22_ah_predicates_split() {
        // Authoritative production constants only: NO_AH = 0,
        // AH_INACTIVE = 1 (initial classification), AH_ACTIVE = 2
        // (actual threshold clamp engaged in `reduce_thresholds_vbr`).
        // AH_INACTIVE counts as classified but NOT clamped; AH_ACTIVE
        // counts as both; NO_AH counts as neither.
        assert_eq!(NO_AH, 0, "production NO_AH pin");
        assert_eq!(AH_INACTIVE, 1, "production AH_INACTIVE pin");
        assert_eq!(AH_ACTIVE, 2, "production AH_ACTIVE pin");
        assert!(!ah_is_classified(NO_AH), "NO_AH: neither");
        assert!(!ah_is_clamped(NO_AH), "NO_AH: neither");
        assert!(ah_is_classified(AH_INACTIVE), "AH_INACTIVE: classified");
        assert!(
            !ah_is_clamped(AH_INACTIVE),
            "AH_INACTIVE: not clamped (classification alone proves no clamp)"
        );
        assert!(ah_is_classified(AH_ACTIVE), "AH_ACTIVE: classified");
        assert!(ah_is_clamped(AH_ACTIVE), "AH_ACTIVE: clamped");
        // Frame-level helpers over owned capture-shaped flags.
        let none: Vec<Vec<u8>> = vec![vec![NO_AH, NO_AH], vec![NO_AH]];
        assert!(!ah_frame_classified(&none), "all-NO_AH: not classified");
        assert!(!ah_frame_clamped(&none), "all-NO_AH: not clamped");
        let inactive: Vec<Vec<u8>> = vec![vec![NO_AH, AH_INACTIVE], vec![NO_AH]];
        assert!(ah_frame_classified(&inactive), "AH_INACTIVE: classified");
        assert!(!ah_frame_clamped(&inactive), "AH_INACTIVE: not clamped");
        let active: Vec<Vec<u8>> = vec![vec![NO_AH, AH_ACTIVE], vec![NO_AH]];
        assert!(ah_frame_classified(&active), "AH_ACTIVE: classified");
        assert!(ah_frame_clamped(&active), "AH_ACTIVE: clamped");
        // Empty capture: neither (no band to witness).
        let empty: Vec<Vec<u8>> = vec![];
        assert!(!ah_frame_classified(&empty), "empty: not classified");
        assert!(!ah_frame_clamped(&empty), "empty: not clamped");
    }

    #[test]
    fn r22_ah_arms_independent() {
        // Q31 and FPE coverage are evaluated independently: a witness on
        // one arm must not leak into the other arm's slot, and a
        // classification witness must not imply a clamp witness.
        let mut classified = RegimeFirst::default();
        let mut clamped = RegimeFirst::default();
        // Q31 sees AH_INACTIVE only: classified-Q fires, clamped stays
        // empty on both arms.
        classified.note(true, "mat", 7, ah_is_classified(AH_INACTIVE));
        clamped.note(true, "mat", 7, ah_is_clamped(AH_INACTIVE));
        assert!(classified.q.is_some(), "Q classified fires");
        assert!(classified.f.is_none(), "F classified stays empty");
        assert!(clamped.q.is_none(), "Q clamped stays empty");
        assert!(clamped.f.is_none(), "F clamped stays empty");
        // FPE sees AH_ACTIVE: both classified-F and clamped-F fire, Q
        // clamp slot stays empty.
        classified.note(false, "mat", 9, ah_is_classified(AH_ACTIVE));
        clamped.note(false, "mat", 9, ah_is_clamped(AH_ACTIVE));
        assert!(classified.f.is_some(), "F classified fires");
        assert!(clamped.f.is_some(), "F clamped fires");
        assert!(clamped.q.is_none(), "Q clamped still empty");
        // First-witness stability: later frames do not overwrite.
        classified.note(true, "later", 99, true);
        clamped.note(false, "later", 99, true);
        assert_eq!(
            classified.q.as_ref().expect("q").0,
            "mat f7",
            "Q first witness stable"
        );
        assert_eq!(
            clamped.f.as_ref().expect("f").0,
            "mat f9",
            "F first witness stable"
        );
    }

    #[test]
    fn r22_ah_unobserved_clamp_not_coverage() {
        // Unobserved clamp coverage is never counted as Observed: with no
        // genuine production-replay witness both slots stay None and the
        // three-state report renders Unobserved (verified absence), never
        // Observed. Classification may be Observed at the same time without
        // implying clamp coverage.
        let mut classified = RegimeFirst::default();
        let mut clamped = RegimeFirst::default();
        classified.note(true, "m", 3, true);
        classified.note(false, "m", 3, true);
        // No clamp witness on either arm across the whole scan.
        assert!(clamped.q.is_none() && clamped.f.is_none(), "clamp absent");
        assert!(
            classified.q.is_some() && classified.f.is_some(),
            "classification Observed on both arms"
        );
        // Three-state semantics intact: None renders absent (Unobserved),
        // Some renders observed with the material+frame witness.
        assert!(
            clamped.fmt().contains("absent(all-frames-verified)"),
            "clamp fmt is absent, not observed"
        );
        assert!(
            classified.fmt().contains("observed@m f3"),
            "classified fmt carries witness"
        );
        // Zero/unclassified values never fire either dimension, even when
        // mixed with each other (no magic numbers: authoritative NO_AH).
        let mut c2 = RegimeFirst::default();
        let mut k2 = RegimeFirst::default();
        let flags: Vec<Vec<u8>> = vec![vec![NO_AH, NO_AH]];
        c2.note(true, "m", 0, ah_frame_classified(&flags));
        k2.note(true, "m", 0, ah_frame_clamped(&flags));
        assert!(c2.q.is_none() && k2.q.is_none(), "NO_AH fires neither");
    }
}

// ---------------------------------------------------------------------------
// E2/E3 evidence-bundle generation (test-only, authorized under
// `R2.2-group-E2-E3-owner-decision.md` §2).
//
// Reuses the reviewed Group A replay/diff/completeness functions
// (`replay_frame`, `diff_captures`, `check_acct_state`,
// `check_snapshot_completeness` via `super::super::r22_capture`, E-OD-1),
// the accepted Group C OP harness helpers (`op_*`, `OpCov`, `OpMaterial`),
// and the reviewed Group D classifiers (`classify_frame`,
// `classify_bands`, `band_events`, `attribute_frame`,
// `run_f20_f22_f26_replay`). No second replay/diff implementation, no
// production change, no schema change. Group F recurrence stays
// `NotGenerated` (SG-3). Generation is env-gated (E-OD-4); default is
// read-only verification.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod e2e3_bundle {
    use super::super::evidence as ev;
    use super::super::r22_capture as rcap;
    use super::super::trace as tr;
    use super::*;

    fn ev_op(label: &str) -> ev::Op {
        match label {
            "OP-1" => ev::Op::Op1,
            "OP-2" => ev::Op::Op2,
            "OP-3" => ev::Op::Op3,
            "OP-4" => ev::Op::Op4,
            "OP-5" => ev::Op::Op5,
            "OP-6" => ev::Op::Op6,
            _ => panic!("bundle: unknown OP {label}"),
        }
    }

    fn class_field(label: String) -> ev::ClassField {
        ev::ClassField::parse(&label)
            .unwrap_or_else(|| panic!("bundle: bad class token {label:?}"))
    }

    fn outcome_field(label: &str) -> ev::OutcomeField {
        ev::OutcomeField::parse(label)
            .unwrap_or_else(|| panic!("bundle: bad outcome token {label:?}"))
    }

    fn margin_field(label: &str) -> ev::MarginField {
        ev::MarginField::parse(label)
            .unwrap_or_else(|| panic!("bundle: bad margin token {label:?}"))
    }

    /// One collected frame with owned captures for row building.
    struct CollectedFrame {
        mat: String,
        frame: u32,
        seq: i32,
        bytes_equal: bool,
        q: R22Capture,
        f: R22Capture,
    }

    /// Step one frame on a persistent encoder pair, reusing the reviewed
    /// Group A primitives (no second replay implementation). Mirrors the
    /// Group C driver checks bit-for-bit; additionally retains owned
    /// captures for bundle row building. Group C functions themselves are
    /// untouched.
    #[allow(clippy::too_many_arguments)]
    fn bundle_step(
        op: &'static str,
        vbr: VbrMode,
        channels: usize,
        cov: &mut OpCov,
        mat: &str,
        frame_no: u32,
        pair: &mut OpPair,
        fresh: &[Vec<i16>],
        st: &mut RunState,
        out: &mut Vec<CollectedFrame>,
    ) {
        let tag = format!("{op} {mat} f{frame_no}");
        let (qb, qs) = pair.q.step_frame(frame_no, fresh);
        let (fb, fs) = pair.f.step_frame(frame_no, fresh);
        assert!(
            qs.notes.is_empty(),
            "{tag} Q31 notes: {:?}",
            qs.notes
        );
        assert!(
            fs.notes.is_empty(),
            "{tag} FPE notes: {:?}",
            fs.notes
        );
        let qb = qb.unwrap_or_else(|| panic!("{tag} Q31 must emit ADTS"));
        let fb = fb.unwrap_or_else(|| panic!("{tag} FPE must emit ADTS"));
        let bytes_equal = qb == fb;
        assert_eq!(qs.seq, fs.seq, "{tag} window sequence agreement");
        let seq = qs.seq;
        assert!((0..=3).contains(&seq), "{tag} seq range");
        if channels == 2 {
            assert!(
                qs.ms_cap.is_some() && fs.ms_cap.is_some(),
                "{tag} stereo MS decision must execute"
            );
            cov.ms_exec_q += 1;
            cov.ms_exec_f += 1;
            for (arm_q, snap_d, snap_m) in [
                (true, qs.digest, qs.mask_ones),
                (false, fs.digest, fs.mask_ones),
            ] {
                assert!((0..=2).contains(&snap_d), "{tag} digest range");
                if arm_q {
                    cov.digest_seen[snap_d as usize] = true;
                }
                cov.ms_some
                    .note(arm_q, mat, frame_no, snap_d != 0 || snap_m > 0);
            }
        } else {
            assert!(
                qs.ms_cap.is_none() && fs.ms_cap.is_none(),
                "{tag} mono must not run MS"
            );
            assert_eq!(qs.digest, 0, "{tag} mono Q31 digest");
            assert_eq!(fs.digest, 0, "{tag} mono FPE digest");
        }
        let cap_q = qs
            .r22_cap
            .as_ref()
            .unwrap_or_else(|| panic!("{tag} Q31 missing R22 capture"))
            .clone();
        let cap_f = fs
            .r22_cap
            .as_ref()
            .unwrap_or_else(|| panic!("{tag} FPE missing R22 capture"))
            .clone();
        rcap::check_acct_state(&cap_q, &qs, &format!("{tag} Q31"));
        rcap::check_acct_state(&cap_f, &fs, &format!("{tag} FPE"));
        rcap::check_snapshot_completeness(&cap_q, channels, &format!("{tag} Q31"));
        rcap::check_snapshot_completeness(&cap_f, channels, &format!("{tag} FPE"));
        rcap::replay_frame(&cap_q, &format!("{tag} Q31"));
        rcap::replay_frame(&cap_f, &format!("{tag} FPE"));
        if channels == 1 {
            assert!(
                cap_q.s1.ms_mask.is_empty() && cap_f.s1.ms_mask.is_empty(),
                "{tag} mono S1 mask must stay empty"
            );
            assert!(
                cap_q.s2.ms_mask.is_none() && cap_f.s2.ms_mask.is_none(),
                "{tag} mono S2 mask must stay absent"
            );
            assert_eq!(qs.is_bands, 0, "{tag} mono Q31 IS bands");
            assert_eq!(fs.is_bands, 0, "{tag} mono FPE IS bands");
        }
        if !st.frame0_done {
            op_assert_frame0_ats(op, mat, vbr, &cap_q, &cap_f);
            st.frame0_done = true;
        }
        if seq == 0 && st.long_pin.is_none() {
            st.long_pin = Some((mat.to_string(), frame_no, cap_q.clone(), cap_f.clone()));
        }
        let w = seq as usize;
        cov.window_count[w] += 1;
        if cov.window_first[w].is_none() {
            cov.window_first[w] = Some((format!("{mat} f{frame_no}"), frame_no));
        }
        let p = st.prev_seq.map_or(4, |s| s as usize);
        if cov.trans_first[p][w].is_none() {
            cov.trans_first[p][w] = Some((format!("{mat} f{frame_no}"), frame_no));
        }
        st.prev_seq = Some(seq);
        cov.is_active.note(true, mat, frame_no, qs.is_bands > 0);
        cov.is_active.note(false, mat, frame_no, fs.is_bands > 0);
        cov.ah_classified.note(
            true,
            mat,
            frame_no,
            ah_frame_classified(&cap_q.s2.ah_flag),
        );
        cov.ah_classified.note(
            false,
            mat,
            frame_no,
            ah_frame_classified(&cap_f.s2.ah_flag),
        );
        cov.ah_clamped.note(
            true,
            mat,
            frame_no,
            ah_frame_clamped(&cap_q.s2.ah_flag),
        );
        cov.ah_clamped.note(
            false,
            mat,
            frame_no,
            ah_frame_clamped(&cap_f.s2.ah_flag),
        );
        cov.multipass
            .note(true, mat, frame_no, cap_q.acct.npass > 1);
        cov.multipass
            .note(false, mat, frame_no, cap_f.acct.npass > 1);
        cov.crash.note(true, mat, frame_no, cap_q.acct.crash >= 0);
        cov.crash.note(false, mat, frame_no, cap_f.acct.crash >= 0);
        cov.bitres_min = cov.bitres_min.min(cap_q.state.qc_post);
        cov.bitres_max = cov.bitres_max.max(cap_q.state.qc_post);
        cov.pad_changed |=
            cap_q.state.pad_pre != cap_q.state.pad_post || cap_f.state.pad_pre != cap_f.state.pad_post;
        let d = rcap::diff_captures(&cap_q, &cap_f, &tag);
        let flagged =
            !d.s1_equal || !d.s2_equal || !d.s4_equal || !d.s6_equal || !d.state_equal || !d.acct_equal;
        assert_eq!(
            flagged,
            !d.differing_fields.is_empty(),
            "{tag} diff record well-formed"
        );
        let stages = [
            !d.s1_equal,
            !d.s2_equal,
            !d.s4_equal,
            !d.s6_equal,
            !d.state_equal,
            !d.acct_equal,
        ];
        for (i, hit) in stages.iter().enumerate() {
            if *hit {
                cov.diff_n[i] += 1;
                if cov.diff_first[i].is_none() {
                    cov.diff_first[i] = Some((format!("{mat} f{frame_no}"), frame_no));
                }
            }
        }
        cov.frames += 1;
        out.push(CollectedFrame {
            mat: mat.to_string(),
            frame: frame_no,
            seq,
            bytes_equal,
            q: cap_q,
            f: cap_f,
        });
    }

    /// Run one OP over standalone materials, collecting frames + coverage.
    fn collect_standalone(
        op: &'static str,
        vbr: VbrMode,
        channels: usize,
        materials: &[OpMaterial],
    ) -> (OpCov, Vec<CollectedFrame>) {
        let mut cov = OpCov::new(op, channels);
        let mut out = Vec::new();
        for mat in materials {
            assert_eq!(mat.pcm.len(), channels, "{op} {}: channel count", mat.id);
            let (mut pair, _, _) = op_encoder_pair(op, &mat.id, vbr, mat.sample_rate, channels);
            let mut st = RunState::fresh();
            for f in 0..mat.frames {
                let fresh = op_frame_slice(&mat.pcm, f);
                bundle_step(op, vbr, channels, &mut cov, &mat.id, f, &mut pair, &fresh, &mut st, &mut out);
            }
            let (pin_mat, pin_frame, q_long, f_long) = st
                .long_pin
                .as_ref()
                .unwrap_or_else(|| panic!("{op} {}: no LONG frame", mat.id));
            op_assert_long_geometry(op, pin_mat, *pin_frame, vbr, mat.sample_rate, channels, q_long, f_long);
            cov.materials.push((mat.id.clone(), mat.sample_rate, mat.frames));
        }
        (cov, out)
    }

    /// Run OP-4 as one continuous 1884-frame chain, collecting frames.
    fn collect_op4_chain(op: &'static str, wavs: &[OpMaterial]) -> (OpCov, Vec<CollectedFrame>) {
        assert_eq!(wavs.len(), 4, "{op}: four WAV blocks");
        let mut cov = OpCov::new(op, 2);
        let mut out = Vec::new();
        let (mut pair, _, _) = op_encoder_pair(op, "chain", VbrMode::Vbr1, 48000, 2);
        let mut st = RunState::fresh();
        let mut global: u32 = 0;
        for mat in wavs {
            assert_eq!(mat.frames, 471, "{op} {}: block length", mat.id);
            for f in 0..mat.frames {
                let fresh = op_frame_slice(&mat.pcm, f);
                bundle_step(op, VbrMode::Vbr1, 2, &mut cov, &mat.id, global, &mut pair, &fresh, &mut st, &mut out);
                global += 1;
            }
            cov.materials.push((mat.id.clone(), mat.sample_rate, mat.frames));
        }
        assert_eq!(global, 1884, "{op}: chained run must total 1884 frames");
        let (pin_mat, pin_frame, q_long, f_long) = st
            .long_pin
            .as_ref()
            .unwrap_or_else(|| panic!("{op}: no LONG frame"));
        op_assert_long_geometry(op, pin_mat, *pin_frame, VbrMode::Vbr1, 48000, 2, q_long, f_long);
        (cov, out)
    }

    /// Dominant FRAME class from the reviewed Group D classification: the
    /// highest-rank stage summary (S/P/Unattributed > A > R; Equal = R).
    fn frame_class_token(fc: &tr::FrameClass) -> String {
        let mut best: Option<tr::Class> = None;
        let mut rank = |c: &tr::Class| -> u8 {
            match c {
                tr::Class::S(_) | tr::Class::P { .. } | tr::Class::Unattributed { .. } => 3,
                tr::Class::A => 2,
                tr::Class::R => 1,
            }
        };
        for sc in &fc.stages {
            if let Some(c) = &sc.class {
                let r = rank(c);
                let replace = match &best {
                    None => true,
                    Some(prev) => r > rank(prev),
                };
                if replace {
                    best = Some(c.clone());
                }
            }
        }
        match best {
            Some(c) => c.label(),
            None => "R".to_string(),
        }
    }

    /// FRAME margin: Required if any frame margin requires it. Frozen schema
    /// v2 has no mixed-status token, so any inapplicable/unobserved family is
    /// recorded as `Missing`; the AC-9 supplement carries exact statuses.
    fn frame_margin_token(q: &R22Capture, bitrate: i32, sr: i32) -> &'static str {
        let margins = tr::frame_margins(q, bitrate, sr);
        if margins.iter().any(|m| m.is_required()) {
            "MarginDerivationRequired"
        } else {
            // Frozen Group E labels are coarse and historical. Applicability
            // remediation is recorded in the AC-9 supplement, not by
            // rebaselining accepted bundle rows.
            "Derived"
        }
    }

    /// BAND margin rule (reuses Group D Required characterization, no new
    /// derivation): S2 short-window bands, all S4 bands (step-3 incomplete),
    /// and all S6 bands (crash stop-distance incomplete) are
    /// `MarginDerivationRequired`; everything else is `Derived`. Documents
    /// AC-9 exactly as currently recorded.
    fn band_margin_token(stage: tr::Stage, seq: i32) -> &'static str {
        // Frozen Group E labels are intentionally unchanged. The AC-9
        // supplement records per-margin applicability without rebaselining.
        match stage {
            tr::Stage::S2 if seq == 2 => "MarginDerivationRequired",
            tr::Stage::S4 => "MarginDerivationRequired",
            tr::Stage::S6 | tr::Stage::Acct => "MarginDerivationRequired",
            _ => "Derived",
        }
    }

    /// Build FRAME/BAND/CHAIN/LEAF rows for collected frames. CHAIN/LEAF
    /// cover flipped frames only (equal bands carry no decomposition);
    /// LEAF values are the Q31 baseline decision values for flipped bands
    /// (FPE divergence is recorded via class + CHAIN; full vectors remain
    /// in captures, provenance via rerun).
    fn build_frame_band_chain_leaf(
        set: &mut ev::RowSet,
        op_label: &'static str,
        bitrate: i32,
        sr_for_margin: i32,
        frames: &[CollectedFrame],
    ) {
        let op = ev_op(op_label);
        for cf in frames {
            let prefix = format!("{op_label} {} f{}", cf.mat, cf.frame);
            let d = rcap::diff_captures(&cf.q, &cf.f, &prefix);
            let fc = tr::classify_frame(&cf.q, &cf.f, &d, &prefix, cf.frame, cf.seq, cf.bytes_equal, false);
            let outcome = if cf.bytes_equal { "F-even" } else { "F-valid" };
            let class_tok = frame_class_token(&fc);
            // FRAME margin from the Q31 arm (baseline-anchored; FPE arm
            // margins classify identically for the Required/Derived split
            // used here).
            let margin_tok = frame_margin_token(&cf.q, bitrate, sr_for_margin);
            set.insert(ev::Row::Frame(ev::FrameRow {
                op,
                material: cf.mat.clone(),
                frame: cf.frame,
                seq: cf.seq,
                outcome: outcome_field(outcome),
                class: class_field(class_tok),
                margin: margin_field(margin_tok),
            }))
            .expect("frame insert");
            let band_origin = tr::classify_bands(&cf.q, &cf.f);
            let cnt = band_origin.len();
            for (b, bc) in band_origin.iter().enumerate() {
                let (stage, class_label) = match bc {
                    tr::BandClass::Equal => (tr::Stage::Is, "R".to_string()),
                    tr::BandClass::Class(c) => {
                        let stage = match c {
                            tr::Class::S(s) => *s,
                            tr::Class::P { stage, .. } => *stage,
                            tr::Class::Unattributed { stage } => *stage,
                            tr::Class::A | tr::Class::R => {
                                // Earliest A/R event stage from the reviewed
                                // per-band event chain (E-OD-1 reuse).
                                let evs = tr::band_events(&cf.q, &cf.f, b);
                                evs.into_iter()
                                    .find(|(_, ec)| !ec.is_structural())
                                    .map(|(s, _)| s)
                                    .unwrap_or(tr::Stage::Is)
                            }
                        };
                        (stage, c.label())
                    }
                };
                set.insert(ev::Row::Band(ev::BandRow {
                    op,
                    material: cf.mat.clone(),
                    frame: cf.frame,
                    band: b as u32,
                    stage,
                    class: class_field(class_label),
                    margin: margin_field(band_margin_token(stage, cf.seq)),
                }))
                .expect("band insert");
            }
            // CHAIN + LEAF for flipped frames only.
            if d.differing_fields.is_empty() {
                continue;
            }
            let attrs = tr::attribute_frame(&cf.q, &cf.f);
            for (ord, a) in attrs.iter().enumerate() {
                let origin = match &a.class {
                    tr::Class::S(_) => ev::ChainOrigin::Root,
                    tr::Class::P { origin_stage, origin_band, .. } => ev::ChainOrigin::Link {
                        stage: *origin_stage,
                        band: *origin_band as u32,
                    },
                    tr::Class::Unattributed { .. } => ev::ChainOrigin::Unattributed,
                    tr::Class::A | tr::Class::R => ev::ChainOrigin::Root,
                };
                set.insert(ev::Row::Chain(ev::ChainRow {
                    op,
                    material: cf.mat.clone(),
                    frame: cf.frame,
                    chain: "main".to_string(),
                    ordinal: ord as u32,
                    stage: a.stage,
                    band: a.band as u32,
                    origin,
                }))
                .expect("chain insert");
            }
            // LEAF rows for structural decision flips only (S/P/Unattributed).
            // A/R bands carry no decision leaf to decompose (their values
            // are recorded via BAND class/margin); Equal bands carry nothing.
            // LEAF values are the Q31 baseline decision values for the
            // flipped stage (FPE divergence via class + CHAIN; full vectors
            // remain in captures, provenance via rerun).
            for (b, bc) in band_origin.iter().enumerate() {
                let stage = match bc {
                    tr::BandClass::Equal => continue,
                    tr::BandClass::Class(tr::Class::A) | tr::BandClass::Class(tr::Class::R) => continue,
                    tr::BandClass::Class(tr::Class::S(s))
                    | tr::BandClass::Class(tr::Class::P { stage: s, .. })
                    | tr::BandClass::Class(tr::Class::Unattributed { stage: s }) => *s,
                };
                let (path, value) = leaf_for_stage(&cf.q, &cf.f, b, stage);
                // LEAF ids must be unique: one row per (frame, band, path).
                set.insert(ev::Row::Leaf(ev::LeafRow {
                    op,
                    material: cf.mat.clone(),
                    frame: cf.frame,
                    band: b as u32,
                    path,
                    value,
                }))
                .expect("leaf insert");
            }
            let _ = cnt;
        }
    }

    /// Q31 baseline decision leaf for a structurally flipped band at the
    /// given stage: path names the production decision leaf, value is the
    /// Q31 decision value (FPE divergence is recorded via BAND class and
    /// CHAIN attribution). Falls back to the stage name with an absent
    /// value when the stage's decision vector does not cover the band.
    fn leaf_for_stage(
        q: &R22Capture,
        f: &R22Capture,
        b: usize,
        stage: tr::Stage,
    ) -> (String, Option<i32>) {
        let nch = q.s1.ch.len();
        match stage {
            tr::Stage::Is if nch == 2 && q.s1.ch[1].cnt > b => {
                let v = q.s1.ch[1].is_book.get(b).copied().unwrap_or(0);
                let _ = f;
                ("is.book".to_string(), Some(v))
            }
            tr::Stage::Ms if nch == 2 => {
                let v = q.s1.ms_mask.get(b).copied().unwrap_or(0);
                let _ = f;
                ("ms.mask".to_string(), Some(v))
            }
            tr::Stage::Pe => ("pe.admission".to_string(), None),
            tr::Stage::S2 => {
                let v = q
                    .s2
                    .ah_flag
                    .first()
                    .and_then(|c| c.get(b).copied())
                    .unwrap_or(0) as i32;
                let _ = f;
                ("s2.ah_flag".to_string(), Some(v))
            }
            tr::Stage::S4 => {
                let v = q
                    .s4
                    .ch
                    .first()
                    .and_then(|c| c.scf.get(b).copied())
                    .unwrap_or(0);
                let _ = f;
                ("s4.scf".to_string(), Some(v))
            }
            _ => (stage.name().to_string(), None),
        }
    }

    /// Legacy helper retained for documentation (superseded by
    /// `leaf_for_stage`; not used by the bundle builder).
    #[allow(dead_code)]
    fn leaf_for_band(q: &R22Capture, b: usize) -> (String, Option<i32>) {
        let nch = q.s1.ch.len();
        // Right-channel IS book (stereo only).
        if nch == 2 {
            if let (Some(qb), Some(_)) = (q.s1.ch.get(1), q.s1.ch.get(0)) {
                if let (Some(v), true) = (qb.is_book.get(b).copied(), true) {
                    // Only emit IS leaves where the arm carries IS data.
                    if q.s1.ch[1].cnt > b {
                        return ("is.book".to_string(), Some(v));
                    }
                }
            }
        }
        // MS mask.
        if let Some(v) = q.s1.ms_mask.get(b).copied() {
            if nch == 2 {
                return ("ms.mask".to_string(), Some(v));
            }
        }
        // S2 avoid-hole flag (first channel).
        if let Some(ch0) = q.s2.ah_flag.first() {
            if let Some(v) = ch0.get(b).copied() {
                return ("s2.ah_flag".to_string(), Some(v as i32));
            }
        }
        // S4 scalefactor (first channel).
        if let Some(ch0) = q.s4.ch.first() {
            if let Some(v) = ch0.scf.get(b).copied() {
                return ("s4.scf".to_string(), Some(v));
            }
        }
        ("pe.admission".to_string(), None)
    }

    /// Build COV rows from an OP's coverage account (Observed /
    /// NotApplicable / Unobserved three-state, never counting gaps as
    /// coverage) plus the Group F `NotGenerated` placeholders (SG-3).
    fn build_cov(set: &mut ev::RowSet, cov: &OpCov) {
        let op = ev_op(cov.op);
        let all = ev::Scope::All;
        let req_frames = cov.frames.max(1);
        // Windows: all four exercised (hard-required by Group C asserts).
        for seq in 0..4 {
            let (observed, status) = match &cov.window_first[seq as usize] {
                Some(_) => (cov.window_count[seq as usize], ev::CoverageStatus::Observed),
                None => (0, ev::CoverageStatus::Missing(ev::MissingKind::Unobserved)),
            };
            set.insert(ev::Row::Cov(ev::CovRow {
                op,
                dimension: format!("window:{}", op_window_name(seq)),
                scope: all.clone(),
                status,
                observed,
                required: req_frames,
                detail: None,
            }))
            .expect("cov window");
        }
        // Transitions: 20 pairs with production-topology reasons.
        let possible = |p: usize, c: usize| -> bool {
            match (p, c) {
                (4, 0) | (4, 1) => true,
                (0, 0) | (0, 1) => true,
                (1, 2) => true,
                (2, 2) | (2, 3) => true,
                (3, 0) | (3, 1) => true,
                _ => false,
            }
        };
        for p in 0..5 {
            for c in 0..4 {
                let ps = if p == 4 { "INIT".to_string() } else { op_window_name(p as i32).to_string() };
                let dim = format!("transition:{ps}->{}", op_window_name(c as i32));
                let (status, observed, detail) = match &cov.trans_first[p][c] {
                    Some(_) => (ev::CoverageStatus::Observed, 1, None),
                    None if possible(p, c) => (
                        ev::CoverageStatus::Missing(ev::MissingKind::Unobserved),
                        0,
                        Some("applicable-but-unobserved-verified-absence".to_string()),
                    ),
                    None => (
                        ev::CoverageStatus::Missing(ev::MissingKind::NotApplicable),
                        0,
                        Some("structurally-impossible-per-block-switch-topology".to_string()),
                    ),
                };
                set.insert(ev::Row::Cov(ev::CovRow {
                    op,
                    dimension: dim,
                    scope: all.clone(),
                    status,
                    observed,
                    required: 1,
                    detail,
                }))
                .expect("cov transition");
            }
        }
        // Carriers.
        let carrier = |key: &str, q: &Option<(String, u32)>, f: &Option<(String, u32)>, na: Option<&str>| {
            if q.is_some() || f.is_some() {
                (ev::CoverageStatus::Observed, 1, None)
            } else if let Some(reason) = na {
                (ev::CoverageStatus::Missing(ev::MissingKind::NotApplicable), 0, Some(reason.to_string()))
            } else {
                (
                    ev::CoverageStatus::Missing(ev::MissingKind::Unobserved),
                    0,
                    Some(format!("{key}-applicable-but-unobserved")),
                )
            }
        };
        let mono = cov.channels == 1;
        // ms_decision: stereo executes every frame; mono structurally absent.
        if mono {
            set.insert(ev::Row::Cov(ev::CovRow {
                op,
                dimension: "carrier:ms_decision".to_string(),
                scope: all.clone(),
                status: ev::CoverageStatus::Missing(ev::MissingKind::NotApplicable),
                observed: 0,
                required: 1,
                detail: Some("mono-no-ms-mask-no-capture".to_string()),
            }))
            .expect("cov ms_decision");
        } else {
            set.insert(ev::Row::Cov(ev::CovRow {
                op,
                dimension: "carrier:ms_decision".to_string(),
                scope: all.clone(),
                status: ev::CoverageStatus::Observed,
                observed: cov.frames,
                required: req_frames,
                detail: None,
            }))
            .expect("cov ms_decision");
        }
        let (st, ob, dt) = carrier("carrier:ms_active", &cov.ms_some.q, &cov.ms_some.f, mono.then_some("mono-stereo-absent"));
        set.insert(ev::Row::Cov(ev::CovRow {
            op, dimension: "carrier:ms_active".to_string(), scope: all.clone(), status: st, observed: ob, required: 1, detail: dt,
        }))
        .expect("cov ms_active");
        if mono {
            set.insert(ev::Row::Cov(ev::CovRow {
                op, dimension: "carrier:is_active".to_string(), scope: all.clone(),
                status: ev::CoverageStatus::Missing(ev::MissingKind::NotApplicable),
                observed: 0, required: 1,
                detail: Some("mono-intensity-stereo-absent".to_string()),
            }))
            .expect("cov is_active");
        } else {
            let (st, ob, dt) = carrier("carrier:is_active", &cov.is_active.q, &cov.is_active.f, None);
            set.insert(ev::Row::Cov(ev::CovRow {
                op, dimension: "carrier:is_active".to_string(), scope: all.clone(), status: st, observed: ob, required: 1, detail: dt,
            }))
            .expect("cov is_active");
        }
        let (st, ob, dt) = carrier("carrier:ah_classified", &cov.ah_classified.q, &cov.ah_classified.f, None);
        set.insert(ev::Row::Cov(ev::CovRow {
            op, dimension: "carrier:ah_classified".to_string(), scope: all.clone(), status: st, observed: ob, required: 1, detail: dt,
        }))
        .expect("cov ah_classified");
        let (st, ob, dt) = carrier("carrier:ah_clamped", &cov.ah_clamped.q, &cov.ah_clamped.f, None);
        set.insert(ev::Row::Cov(ev::CovRow {
            op, dimension: "carrier:ah_clamped".to_string(), scope: all.clone(), status: st, observed: ob, required: 1, detail: dt,
        }))
        .expect("cov ah_clamped");
        let (st, ob, dt) = carrier("carrier:s6_multipass", &cov.multipass.q, &cov.multipass.f, None);
        set.insert(ev::Row::Cov(ev::CovRow {
            op, dimension: "carrier:s6_multipass".to_string(), scope: all.clone(), status: st, observed: ob, required: 1, detail: dt,
        }))
        .expect("cov s6_multipass");
        // s6_crash: the production no-crash marker is crash==-1; a RegimeFirst
        // witness fires only on crash>=0, so absence across the corpus is the
        // documented Unobserved limitation (Group C), never coverage.
        let (st, ob, dt) = carrier("carrier:s6_crash", &cov.crash.q, &cov.crash.f, None);
        set.insert(ev::Row::Cov(ev::CovRow {
            op, dimension: "carrier:s6_crash".to_string(), scope: all.clone(), status: st, observed: ob, required: 1, detail: dt,
        }))
        .expect("cov s6_crash");
        // Group F placeholders (SG-3): explicitly NotGenerated, no recurrence
        // conclusions from Group E outputs.
        for dim in [
            "recurrence:op4-chain-behaviour",
            "recurrence:five-frame-od3-boundary",
            "recurrence:p-termination-evidence",
        ] {
            set.insert(ev::Row::Cov(ev::CovRow {
                op,
                dimension: dim.to_string(),
                scope: all.clone(),
                status: ev::CoverageStatus::Missing(ev::MissingKind::NotGenerated),
                observed: 0,
                required: 0,
                detail: Some("Group-F-NotGenerated-per-SG-3".to_string()),
            }))
            .expect("cov group-f placeholder");
        }
    }

    /// Build REPLAY rows for the F-20/F-22/F-26 instrumented bundled-PE
    /// evidence (M3 narrow slice): per-finding per-arm mechanism digests
    /// through the unchanged production functions. Status is `Match` (the
    /// replay asserts bit-exactly; a mismatch would panic, never record
    /// `Match`).
    fn build_f20_replay(set: &mut ev::RowSet) {
        let rows = tr::run_f20_f22_f26_replay();
        assert_eq!(rows.len(), 6, "three findings x two arms");
        for r in &rows {
            let tag = format!("f20-{}-{}", r.finding, r.arm);
            let mut s = String::new();
            s.push_str(&format!(
                "{}:{}:{}:{}:{}:{}:{}:{}:{},{},{}:{},{},{}:{};",
                r.finding,
                r.band,
                r.stored_er,
                r.e_ld_r,
                r.e_ld_l,
                r.real_scale,
                r.is_book,
                r.is_scale,
                r.ats_pre[0],
                r.ats_pre[1],
                r.ats_pre[2],
                r.ats_post[0],
                r.ats_post[1],
                r.ats_post[2],
                r.pe,
            ));
            let digest = ev::sha256_of(s.as_bytes());
            set.insert(ev::Row::Replay(ev::ReplayRow {
                tag,
                kind: "f20".to_string(),
                digest,
                status: ev::ReplayStatus::Match,
            }))
            .expect("replay insert");
        }
    }

    /// Collect all six OPs (single deterministic pass) into one `RowSet`.
    fn collect_all() -> ev::RowSet {
        let mut set = ev::RowSet::new();
        // OP-1: 13 synthetic probes.
        let mats1 = op1_materials();
        let (cov1, frames1) = collect_standalone("OP-1", VbrMode::Vbr5, 2, &mats1);
        assert_eq!(cov1.materials.len(), 13, "OP-1 material accounting");
        build_frame_band_chain_leaf(&mut set, "OP-1", 96000, 48000, &frames1);
        build_cov(&mut set, &cov1);
        // OP-2: single WAV.
        let mats2 = [op_wav_material("m7-music-stereo-48k")];
        let (cov2, frames2) = collect_standalone("OP-2", VbrMode::Vbr5, 2, &mats2);
        assert_eq!(cov2.frames, 471, "OP-2 frame count");
        build_frame_band_chain_leaf(&mut set, "OP-2", 96000, 48000, &frames2);
        build_cov(&mut set, &cov2);
        // OP-3: three WAVs.
        let mats3 = [
            op_wav_material("m7-speech-pauses-stereo-48k"),
            op_wav_material("m7-speech-clean-stereo-48k"),
            op_wav_material("m8-music-eroica-stereo-48k"),
        ];
        let (cov3, frames3) = collect_standalone("OP-3", VbrMode::Vbr5, 2, &mats3);
        assert_eq!(cov3.frames, 3 * 471, "OP-3 frame count");
        build_frame_band_chain_leaf(&mut set, "OP-3", 96000, 48000, &frames3);
        build_cov(&mut set, &cov3);
        // OP-4: 1884-frame Vbr1 chain.
        let wavs4 = [
            op_wav_material("m7-music-stereo-48k"),
            op_wav_material("m7-speech-pauses-stereo-48k"),
            op_wav_material("m7-speech-clean-stereo-48k"),
            op_wav_material("m8-music-eroica-stereo-48k"),
        ];
        let (cov4, frames4) = collect_op4_chain("OP-4", &wavs4);
        assert_eq!(cov4.frames, 1884, "OP-4 frame count");
        build_frame_band_chain_leaf(&mut set, "OP-4", 20000, 48000, &frames4);
        build_cov(&mut set, &cov4);
        // OP-5: 44.1 kHz pair.
        let mats5 = [
            op_fixture_material("sine-stereo-44100", "sine-stereo-44100", None),
            op_fixture_material("onset-stereo-44100", "onset-stereo-44100", None),
        ];
        let (cov5, frames5) = collect_standalone("OP-5", VbrMode::Vbr3, 2, &mats5);
        assert_eq!(cov5.materials.len(), 2, "OP-5 material accounting");
        build_frame_band_chain_leaf(&mut set, "OP-5", 48000, 44100, &frames5);
        build_cov(&mut set, &cov5);
        // OP-6: mono pair.
        let mats6 = [
            op_fixture_material("click-mono", "click-mono", None),
            op_fixture_material("impulse-mono", "impulse-mono", None),
        ];
        let (cov6, frames6) = collect_standalone("OP-6", VbrMode::Vbr5, 1, &mats6);
        assert_eq!(cov6.materials.len(), 2, "OP-6 material accounting");
        build_frame_band_chain_leaf(&mut set, "OP-6", 112000, 48000, &frames6);
        build_cov(&mut set, &cov6);
        // F-20/F-22/F-26 per-finding replay (M3 narrow slice).
        build_f20_replay(&mut set);
        set
    }

    fn bundle_head() -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        match std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("rev-parse")
            .arg("HEAD")
            .output()
        {
            Ok(o) if o.status.success() => {
                String::from_utf8_lossy(&o.stdout).trim().to_string()
            }
            _ => "unknown-head".to_string(),
        }
    }

    fn bundle_platform() -> String {
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
    }

    fn bundle_readme(row_count: usize, tsv_bytes: usize, json_bytes: usize) -> String {
        format!(
            "# R2.2 Evidence Bundle (E2/E3, schema v2)\n\n\
            Generated under `docs/fpe/assessment/R2.2-group-E2-E3-owner-decision.md` §2 from the frozen E1 schema v2 (no redesign).\n\n\
            - Rows: {row_count} (FRAME/BAND/CHAIN/LEAF/REPLAY/COV, total order, `R22SCHEMA\\t2`).\n\
            - Files: `r22-bundle.tsv` ({tsv_bytes} bytes), `r22-bundle.json` ({json_bytes} bytes), `json/` OP×row-type partition (E-OD-8), `MANIFEST.txt`, `SHA256SUMS`.\n\
            - Provenance: manifest records HEAD, platform, per-file SHA-256 + bytes, row counts; SHA256SUMS pins every file; verification is read-only by default (E-OD-4: set `FPE_R22_GENERATE=1` to regenerate).\n\
            - Ordering: see `evidence.rs` E2/E3 total-ordering specification (F-A); REPLAY sorts purely by tag; COV sorts by (OP, scope, dimension).\n\
            - Durability: files are written with flush + file-fsync + directory-fsync; torn writes fail closed (F-B).\n\
            - Group F isolation (SG-3): recurrence dimensions are `NotGenerated`; no FixedPoint/Cycle/BoundedNotTerminated conclusions from Group E outputs.\n\
            - Register: `docs/fpe/assessment/R2.2-findings-register.md` (R22-* namespace, E-OD-5); this bundle is implementation output, not acceptance.\n"
        )
    }

    fn bundle_resource_report(
        elapsed_secs: f64,
        rows: usize,
        tsv_bytes: usize,
        json_bytes: usize,
        part_bytes: usize,
        total_bytes: usize,
    ) -> String {
        let rss = current_rss_kb();
        let rss_line = match rss {
            Some(kb) => format!("peak_rss_kb: {kb} (best-effort `ps` sample after generation; method noted, not a peak-hold counter)\n"),
            None => "peak_rss_kb: unavailable (ps sample failed; bundle bytes + row counts retained; never silently omitted)\n".to_string(),
        };
        format!(
            "SG-1 resource report (actual E2/E3 generation run, reference platform)\n\
            generation_runtime_secs: {elapsed_secs:.2}\n\
            rows: {rows}\n\
            tsv_bytes: {tsv_bytes}\n\
            json_bytes: {json_bytes}\n\
            json_partition_bytes: {part_bytes}\n\
            total_bundle_bytes: {total_bytes}\n\
            {rss_line}\
            ci_impact: bundle files total {total_bytes} bytes; generation runtime {elapsed_secs:.2}s in debug (release expected faster); \
            verification is read-only parsing + hashing (no encoder runs). No rows, materials, OPs, or determinism gates omitted for size/time pressure.\n"
        )
    }

    /// Best-effort current RSS in KiB via `ps` (method documented in the
    /// resource report; `None` is reported explicitly, never silently).
    fn current_rss_kb() -> Option<u64> {
        #[cfg(unix)]
        {
            let out = std::process::Command::new("ps")
                .arg("-o")
                .arg("rss=")
                .arg("-p")
                .arg(std::process::id().to_string())
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().ok()
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    /// Write a complete bundle directory (durable files + manifest + hashes).
    /// Returns `(row_count, total_bytes)`. Caller holds the generation gate.
    fn write_bundle(dir: &std::path::Path, set: &ev::RowSet, elapsed_secs: f64) -> (usize, usize) {
        let tsv = set.render_tsv_file();
        let json = set.render_json_file();
        let parts = ev::partition_op_rowtype(set);
        let mut part_bytes_total = 0usize;
        let mut part_files: Vec<(String, String)> = Vec::new();
        for (op, ty, rows) in &parts {
            let fname = format!("json/{op}_{ty}.json");
            let body = ev::render_json_partition(rows);
            part_bytes_total += body.len();
            part_files.push((fname, body));
        }
        let head = bundle_head();
        let platform = bundle_platform();
        // Content files first (manifest + shasums last).
        let readme = bundle_readme(set.len(), tsv.len(), json.len());
        // Resource report needs total bytes: compute after content known.
        // Two-phase: write content to memory, build resource + manifest, then
        // durable-write everything (single deterministic pass).
        let mut content: Vec<(String, Vec<u8>)> = Vec::new();
        content.push((ev::BUNDLE_TSV_NAME.to_string(), tsv.into_bytes()));
        content.push((ev::BUNDLE_JSON_NAME.to_string(), json.into_bytes()));
        for (name, body) in &part_files {
            content.push((name.clone(), body.clone().into_bytes()));
        }
        content.push((ev::BUNDLE_README_NAME.to_string(), readme.into_bytes()));
        // Placeholder resource (recomputed with final totals below).
        let total_content: usize = content.iter().map(|(_, b)| b.len()).sum::<usize>() + part_bytes_total;
        let _ = total_content;
        // First compute resource with content totals (manifest/shas not yet
        // included; resource reports content + partition bytes explicitly).
        let rows = set.len();
        let tsv_len = content[0].1.len();
        let json_len = content[1].1.len();
        let resource = bundle_resource_report(elapsed_secs, rows, tsv_len, json_len, part_bytes_total, tsv_len + json_len + part_bytes_total);
        content.push((ev::RESOURCE_NAME.to_string(), resource.into_bytes()));
        // Manifest over content files (all except MANIFEST + SHA).
        let file_refs: Vec<(&str, usize, String)> = content
            .iter()
            .map(|(n, b)| (n.as_str(), b.len(), ev::sha256_of(b)))
            .collect();
        let file_refs_borrowed: Vec<(&str, usize, &str)> = file_refs
            .iter()
            .map(|(n, l, s)| (*n, *l, s.as_str()))
            .collect();
        // Row counts: TSV and JSON carry the same set.
        let manifest = ev::manifest_text(&head, &platform, &file_refs_borrowed, rows, rows);
        let manifest_bytes = manifest.into_bytes();
        let mut sha_inputs: Vec<(&str, String)> = file_refs
            .iter()
            .map(|(n, _, s)| (*n, s.clone()))
            .collect();
        sha_inputs.push((ev::MANIFEST_NAME, ev::sha256_of(&manifest_bytes)));
        let shas = ev::shasums_text(&sha_inputs);
        // Durable writes (top-level + json/ + manifest + shas).
        for (name, bytes) in &content {
            ev::write_file_durable(dir, name, bytes).expect("bundle write");
        }
        ev::write_file_durable(dir, ev::MANIFEST_NAME, &manifest_bytes).expect("manifest write");
        ev::write_file_durable(dir, ev::SHA_NAME, shas.as_bytes()).expect("shas write");
        let total: usize = content.iter().map(|(_, b)| b.len()).sum::<usize>()
            + manifest_bytes.len()
            + shas.len();
        (rows, total)
    }

    #[test]
    fn e2e3_bundle_deterministic_generation_and_verify() {
        // E-OD-4: generation only under the gate; without it, verify the
        // committed bundle read-only. With it, regenerate deterministically
        // (double-generation byte-identical) and verify.
        if !ev::env_generate_enabled() {
            let dir = ev::bundle_dir();
            if !dir.join(ev::BUNDLE_TSV_NAME).exists() {
                eprintln!("SKIP: no committed bundle at {}; set FPE_R22_GENERATE=1 to generate", dir.display());
                return;
            }
            let n = ev::verify_bundle_dir(&dir).expect("read-only verify");
            assert!(n > 0, "bundle non-empty");
            return;
        }
        // Generation pass (timed for SG-1).
        let t0 = std::time::Instant::now();
        let set = collect_all();
        let elapsed = t0.elapsed().as_secs_f64();
        // Determinism: rebuild and compare byte-identically (same process,
        // no hidden state; second pass reruns the encoders).
        let set2 = collect_all();
        assert_eq!(set, set2, "double-generation set equality");
        assert_eq!(
            set.render_tsv_file(),
            set2.render_tsv_file(),
            "double-generation TSV byte-identical"
        );
        assert_eq!(
            set.render_json_file(),
            set2.render_json_file(),
            "double-generation JSON byte-identical"
        );
        // Schema v2 unchanged (E1 contract pins).
        assert_eq!(ev::SCHEMA_VERSION, 2, "schema v2 frozen");
        assert_eq!(ev::SCHEMA_HEADER, "R22SCHEMA\t2");
        assert_eq!(ev::MDEF_VERSION, 2);
        // Write to a temp dir first (isolation: no dependence on prior
        // bundle files), verify, then write the committed bundle dir.
        let tmp = std::env::temp_dir().join(format!("r22-bundle-gen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let (rows, _) = write_bundle(&tmp, &set, elapsed);
        let n = ev::verify_bundle_dir(&tmp).expect("temp verify");
        assert_eq!(n, rows, "temp verify row count");
        // Kill-mid-write on the temp copy fails closed (F-B).
        let tsv_path = tmp.join(ev::BUNDLE_TSV_NAME);
        let tsv_bytes = std::fs::read(&tsv_path).unwrap();
        std::fs::write(&tsv_path, &tsv_bytes[..tsv_bytes.len() / 2]).unwrap();
        assert!(
            ev::verify_bundle_dir(&tmp).is_err(),
            "torn bundle fails closed"
        );
        let _ = std::fs::remove_dir_all(&tmp);
        // Committed bundle dir (durable, env-gated).
        let dir = ev::bundle_dir();
        let (rows2, _) = write_bundle(&dir, &set, elapsed);
        assert_eq!(rows2, rows);
        let n2 = ev::verify_bundle_dir(&dir).expect("committed verify");
        assert_eq!(n2, rows, "committed verify row count");
        // F-20/22/26 slice present (M3 narrow authorization).
        let has_f20 = set.rows_in_order().iter().any(|r| r.id().starts_with("R22-REPLAY/f20-"));
        assert!(has_f20, "F-20/22/26 REPLAY rows present");
        // SG-3: Group F placeholders present, no recurrence conclusions.
        let has_notgenerated = set.rows_in_order().iter().any(|r| match r {
            ev::Row::Cov(c) => c.status == ev::CoverageStatus::Missing(ev::MissingKind::NotGenerated),
            _ => false,
        });
        assert!(has_notgenerated, "Group F NotGenerated placeholders present");
        for r in set.rows_in_order() {
            let id = r.id();
            assert!(
                !id.contains("FixedPoint") && !id.contains("CycleObserved") && !id.contains("BoundedNotTerminated"),
                "no recurrence conclusions in bundle"
            );
        }
    }

    #[test]
    fn e2e3_schema_v2_unchanged() {
        // E1 contract pins: any change requires a new owner decision.
        assert_eq!(ev::SCHEMA_VERSION, 2);
        assert_eq!(ev::SCHEMA_HEADER, "R22SCHEMA\t2");
        assert_eq!(ev::MDEF_VERSION, 2);
        // Spot-check frozen vocabulary + ordering through the reviewed
        // validators (E-OD-1 reuse, not reimplementation).
        assert!(ev::RowSet::parse_tsv_file("R22SCHEMA\t2\n").unwrap().is_empty());
        assert!(ev::RowSet::parse_tsv_file("R22SCHEMA\t1\n").is_err());
        assert!(ev::RowSet::parse_tsv_file("R22SCHEMA 2\n").is_err());
    }
}
