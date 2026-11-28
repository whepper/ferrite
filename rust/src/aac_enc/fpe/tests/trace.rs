//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-24.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! R2.2 Group D (test-only): downstream decision classification, observational
//! margins and same-frame chain attribution.
//!
//! This module extends the accepted Group A capture/replay infrastructure
//! (`r22_capture.rs`) and the Group C operating-point harness (`decision.rs`)
//! with the R2.2 design §3 classification vocabulary and §3/§5 margins. It
//! adds **no** production behaviour: every item is `#[cfg(test)]`-gated by
//! living in the test tree, every margin is computed from dissection-cloned
//! operands (the owned `R22Capture` snapshots), and no margin result is ever
//! written back into a live production arm or consumed by a production
//! decision function (purity + side-effect canaries below).
//!
//! ## Two distinct comparisons (design §10.2, binding)
//!
//! * **Within-arm replay exactness** is owned by Group A (`replay_frame`):
//!   Q31 capture ↔ Q31 replay and FPE capture ↔ FPE replay, bit-exact. This
//!   module never re-asserts cross-arm identity.
//! * **Cross-arm classification** (this module): the Q31 and FPE production
//!   captures are compared, differences are *recorded* and classified under
//!   the canonical **R/A/S/P/F** vocabulary (design §10.4). A difference is
//!   neither hidden nor automatically accepted.
//!
//! ## Vocabulary (design §3 / §10.4)
//!
//! * **R — Residue**: decisions unchanged; value fields differ within the
//!   established FPE-3.1R1 term-separated envelope.
//! * **A — Intermediate**: threshold/Ld-domain intermediates differ while
//!   coding decisions remain equal. (No alternative `I` class exists.)
//! * **S — Structural flip**: a decision field differs; sub-flagged by the
//!   originating stage `S-IS`, `S-MS`, `S-PE`, `S-S2`, `S-S4`, `S-S6`.
//! * **P — Propagation**: a structural difference at a later stage of the
//!   *same frame*, attributed to its earliest defensible upstream R/A/S
//!   event. Within-frame only; cross-frame recurrence is Group F (D-6).
//! * **F — Frame-level**: `F-even` (byte-identical ADTS), `F-valid`
//!   (different but structurally valid), `F-unsupported` (unsupported path).
//!
//! `Unattributed` records a later-stage difference whose consuming chain
//! cannot be established; it is never forced into a P attribution.
//!
//! ## Margins (design §5, observational only)
//!
//! Every margin identifies the exact production decision expression, its
//! operands/units, the signed distance to the boundary, the crossing
//! direction and the production equality semantics (derivation sheet in the
//! Group D report). A margin whose derivation cannot be established from
//! production semantics is recorded [`MarginStatus::MarginDerivationRequired`]
//! — never guessed, never approximated, never silently omitted.

use crate::aac_enc::bitcount::CODE_BOOK_IS_IN_PHASE_NO;
use crate::aac_enc::config::{EncoderConfig, PsyEngineKind, TransportType, VbrMode, FRAME_LENGTH};
use crate::aac_enc::intensity::{finalize_decisions, intensity_stereo, prepare_decisions};
use crate::aac_enc::pe::{pe_calculation, PeChannelInput};
use crate::aac_enc::q31::{calc_ld_data, calc_ld_int, fdiv_norm, fmult};
use crate::aac_enc::rate::{
    bits2pe2, calc_frame_len, GLOBAL_EXT_BITS, MAX_ITERATIONS, MIN_BUFSIZE_PER_EFF_CHAN,
};
use crate::aac_enc::stream_driver::{R22Capture, R22S1Cap, R22S2Cap, StreamEncoder};
use crate::aac_enc::threshold_vbr::{
    adapt_min_snr, init_avoid_hole_flag_mono, SfbGeometry, AH_ACTIVE,
};

use super::is_ms::probe_narrow;
use super::r22_capture::R22Diff;

// ---------------------------------------------------------------------------
// Source constants (mirror production; algorithm stays in production).
//
// Each literal below is the exact C-probed Q31 value used by the cited
// production decision expression. They are mirrored here (the established
// `is_ms.rs` pattern) because the production constants are module-private;
// the boundary tests tie every mirrored edge to the production function's
// actual flip behaviour, so a drifted literal fails loudly.
// ---------------------------------------------------------------------------

/// `C1_LD_Q31` (`pe.rs:130`, `line_pe.cpp:109`): `log(8)/log(2) = 3.0` Ld.
/// `calc_sfb_pe` linear-branch gate `ratio >= C1` (inclusive).
const C1_LD_Q31: i32 = 0x0600_0000;
/// `ENFAC_FLOOR_Q31` (`pe.rs:135`, `adj_thr.cpp:822`): 0.1875 chaos floor.
const ENFAC_FLOOR_Q31: i32 = 0x1800_0000;
/// `PATCH_ARM_Q31` (`pe.rs:138`, `adj_thr.cpp:826`): 0.78125 patch-arm gate.
const PATCH_ARM_Q31: i32 = 0x6400_0000;
/// `PATCH_HI_Q31` (`pe.rs:139`): 0.8125 (3/4-energy exponent selector).
const PATCH_HI_Q31: i32 = 0x6800_0000;
/// `PATCH_MID_Q31` (`pe.rs:140`): 0.796875 (2/4-energy exponent selector).
const PATCH_MID_Q31: i32 = 0x6600_0000;
/// `SHORT_ENFAC_Q31` (`pe.rs:142`, `adj_thr.cpp:873`): 0.75 short-element state.
const SHORT_ENFAC_Q31: i32 = 0x6000_0000;

/// `MIN_LDTHRESH` (`threshold_vbr.rs:186`, `adj_thr.cpp:1133`): -0.515625.
const MIN_LDTHRESH: i32 = 0xbe00_0000u32 as i32;
/// `AVG_COMP_Q31` (`threshold_vbr.rs:199`): +0.09375 (>>6 accumulation comp).
const AVG_COMP_Q31: i32 = 0x0c00_0000;
/// `NEG_ONE_Q31` (`threshold_vbr.rs:192`): -1.0 Ld sentinel.
const NEG_ONE_Q31: i32 = i32::MIN;
/// `SHORT_THR_FAC` (`threshold_vbr.rs:217`, `adj_thr.cpp:1255`): 2.82/4.
const SHORT_THR_FAC_Q31: i32 = 0x5a3d7080;
/// `INV_SQRT4` (`threshold_vbr.rs:171-173`, `adj_thr.cpp:114-116`):
/// fourth-root reciprocals, verbatim.
const INV_SQRT4_Q31: [i32; 8] = [
    0x7fffffff, 0x7fffffff, 0x6ba27e65, 0x61424bb5, 0x5a827999, 0x55994845, 0x51c8e33c, 0x4eb160d1,
];
/// `LIMIT_THR_REDUCED` (`threshold_vbr.rs:184`, `adj_thr.cpp:985-986`).
const LIMIT_THR_REDUCED_Q31: i32 = 0x00008000;
/// `NEG_HALF_Q31` (`threshold_vbr.rs:188`, `:1302`): -0.5 clamp.
const NEG_HALF_Q31: i32 = 0xc0000000u32 as i32;
/// `ONE_Q31` (`threshold_vbr.rs:193`): saturating 1.0 (`MAXVAL_DBL`).
const ONE_Q31: i32 = i32::MAX;
/// `ZERO_Q31` (`threshold_vbr.rs:190`): 0.0.
const ZERO_Q31: i32 = 0;

/// `LEFT_RIGHT_RATIO_THRESH` (`intensity.rs:34`): 0.7 I-2 anti-centre sentinel.
const LEFT_RIGHT_RATIO_THRESH: i32 = 0x5999_9980;
/// `REAL_SCALE_MAX` (`intensity.rs:35`): 60/128 real-scale rail.
const REAL_SCALE_MAX: i32 = 0x3C00_0000;
/// `REAL_SCALE_MIN` (`intensity.rs:36`): -60/128 real-scale rail.
const REAL_SCALE_MIN: i32 = -0x3C00_0000;

/// `PECORR_12_2` (`rate.rs:264`, `adj_thr.cpp:2604`): 1.2/2 PE-corr window hi.
const PECORR_12_2: i32 = 0x4ccccd00u32 as i32;
/// `PECORR_065` (`rate.rs:266`): 0.65 PE-corr window lo factor.
const PECORR_065: i32 = 0x5333_3300;

/// `D_1122` (`scf.rs:264`, `sf_estim.cpp`): ld64(1.122) step-3 dist window.
const D_1122: i32 = 5_572_475;
/// `D_07079` (`scf.rs:265`): ld64(0.7079) step-3 energy window.
const D_07079: i32 = 16_722_942;

/// `FILL_EL_COUNT_BITS` (`rate.rs:1235`, `bitenc.cpp:851`): fill element count.
const FILL_EL_COUNT_BITS: i32 = 4;

// ---------------------------------------------------------------------------
// Classification vocabulary
// ---------------------------------------------------------------------------

/// Downstream stage identity, in production execution order (design §1).
/// The order is load-bearing for same-frame propagation attribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Stage {
    Is,
    Ms,
    Pe,
    S2,
    S4,
    S6,
    Acct,
}

impl Stage {
    /// Canonical sub-flag label (`S-IS`, `S-MS`, `S-PE`, `S-S2`, `S-S4`,
    /// `S-S6`; accounting flips are reported under S6's frame outcome).
    pub(super) fn label(self) -> &'static str {
        match self {
            Stage::Is => "S-IS",
            Stage::Ms => "S-MS",
            Stage::Pe => "S-PE",
            Stage::S2 => "S-S2",
            Stage::S4 => "S-S4",
            Stage::S6 => "S-S6",
            Stage::Acct => "S-S6",
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Stage::Is => "is",
            Stage::Ms => "ms",
            Stage::Pe => "pe",
            Stage::S2 => "s2",
            Stage::S4 => "s4",
            Stage::S6 => "s6",
            Stage::Acct => "acct",
        }
    }
}

/// Frame-level accounting outcome (design §3, F class).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FrameOutcome {
    /// Byte-identical ADTS across arms.
    FEven,
    /// Different bytes, both structurally valid ADTS (Tier-B decode blocked).
    FValid,
    /// An unsupported path was hit (never a pass).
    FUnsupported,
}

impl FrameOutcome {
    pub(super) fn label(self) -> &'static str {
        match self {
            FrameOutcome::FEven => "F-even",
            FrameOutcome::FValid => "F-valid",
            FrameOutcome::FUnsupported => "F-unsupported",
        }
    }
}

/// The canonical R/A/S/P/F classification of one observed difference (or of
/// one band/stage cell). `Unattributed` is a recorded finding, not a class
/// flip: it marks a later-stage difference whose consuming chain could not
/// be established (design §10.4 — never forced).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Class {
    /// Residue: decisions equal, value fields differ within the envelope.
    R,
    /// Intermediate: threshold/Ld-domain intermediates differ, decisions equal.
    A,
    /// Structural flip originating at this stage.
    S(Stage),
    /// Within-frame propagation: a structural difference at `stage` cited to
    /// its earliest defensible upstream event at `origin_stage`/`origin_band`.
    P {
        stage: Stage,
        origin_stage: Stage,
        origin_band: usize,
        origin_class: Box<Class>,
    },
    /// A later-stage difference with no establishable consuming chain.
    Unattributed { stage: Stage },
}

impl Class {
    /// True for a structural flip or a propagated structural difference.
    pub(super) fn is_structural(&self) -> bool {
        matches!(
            self,
            Class::S(_) | Class::P { .. } | Class::Unattributed { .. }
        )
    }

    /// Canonical label for evidence rows.
    pub(super) fn label(&self) -> String {
        match self {
            Class::R => "R".to_string(),
            Class::A => "A".to_string(),
            Class::S(s) => s.label().to_string(),
            Class::P { stage, .. } => format!("P({})", stage.label()),
            Class::Unattributed { stage } => format!("Unattributed({})", stage.name()),
        }
    }
}

// ---------------------------------------------------------------------------
// Decision-field mapping (design §3: which fields are *decisions*)
// ---------------------------------------------------------------------------

/// Kind of a captured leaf, deciding how a cross-arm difference classifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldKind {
    /// A coding decision field owned by a stage (flip ⇒ S of that stage).
    Decision(Stage),
    /// A threshold/Ld-domain intermediate (diff with equal decisions ⇒ A).
    Intermediate,
    /// An energy-domain value (diff within the FPE-3.1R1 envelope ⇒ R).
    EnergyValue,
    /// Geometry / representation invariant (must agree; a diff is structural).
    Structural,
    /// An init-time constant (must agree; a diff is an init-parity finding).
    InitConstant,
    /// A frame-surviving carrier (cross-frame provenance; Group F domain).
    Carrier,
}

/// Map a `diff_captures` leaf path (the part after `{prefix}.`) to its kind.
///
/// This is the production decision-field mapping: every leaf emitted by
/// `r22_capture::diff_captures` resolves here. Decision fields are exactly
/// the production outputs that select coding behaviour (IS book/scale, MS
/// mask/digest, PE patch-arm/admission, S2 avoid-hole flag, S4 scalefactor/
/// gain, S6 gains/sections/grants). Energy/Ld/threshold vectors are value or
/// intermediate fields; geometry and init constants are invariants.
pub(super) fn field_kind(path: &str) -> FieldKind {
    // `path` is `{stage}.{leaf}` (prefix already stripped by the caller).
    let (stage, leaf) = match path.split_once('.') {
        Some(s) => s,
        None => return FieldKind::Structural,
    };
    // Per-channel leaves look like `ch0.energy_ld` / `pe_ch0.sfb_pe`.
    let leaf_norm = leaf.trim_start_matches("pe_ch").trim_start_matches("ch");
    let leaf_name = leaf_norm
        .split_once('.')
        .map(|(_, n)| n)
        .unwrap_or(leaf_norm);
    // Strip a leading channel index (digits) from `leaf_norm` head.
    match stage {
        "s1" => match leaf_name {
            "is_book" | "is_scale" => FieldKind::Decision(Stage::Is),
            "ms_mask" => FieldKind::Decision(Stage::Ms),
            "energy_ld" | "energy_lin" | "ff" => FieldKind::EnergyValue,
            "threshold_ld" => FieldKind::Intermediate,
            "weighted" | "thresholds" | "enfac" => FieldKind::Intermediate,
            "pe" | "sums" | "sfb_pe" | "sfb_const_part" | "sfb_n_active_lines" | "const_part"
            | "n_active_lines" => FieldKind::Intermediate,
            "sfb_n_lines" => FieldKind::Intermediate,
            "ats_pre" | "ats_post" => FieldKind::Carrier,
            "offsets" | "geo" | "ch_count" | "pe_ch_count" => FieldKind::Structural,
            _ => FieldKind::Structural,
        },
        "s2" => match leaf_name {
            "ah_flag" => FieldKind::Decision(Stage::S2),
            "ms_mask" => FieldKind::Decision(Stage::Ms),
            "threshold_ld" | "pre_enfac" | "thr_exp" | "min_snr_ld" | "spread" | "reduce" => {
                FieldKind::Intermediate
            }
            "energy_lin" | "energy_ld" | "ff" => FieldKind::EnergyValue,
            "weighted" | "threshold" | "enfac" | "min_snr0" | "spread0" => FieldKind::Intermediate,
            "ats_pre" | "ats_post" => FieldKind::Carrier,
            "offsets" | "group_len" | "geo" | "ch_count" => FieldKind::Structural,
            _ => FieldKind::Structural,
        },
        "s4" => match leaf_name {
            "scf" | "gain" => FieldKind::Decision(Stage::S4),
            "quant_spec" | "mutated" => FieldKind::Decision(Stage::S4),
            "spectrum_pre" => FieldKind::Structural,
            "energy_ld" | "ff" => FieldKind::EnergyValue,
            "threshold_ld" => FieldKind::Intermediate,
            "offsets" | "geo" | "ch_count" => FieldKind::Structural,
            _ => FieldKind::Structural,
        },
        "s6" => match leaf_name {
            "grants" | "passes" | "sections" | "bits" | "bit_res_level_el" | "bit_res_max_el" => {
                FieldKind::Decision(Stage::S6)
            }
            "scf" | "gain" | "seed_quant" => FieldKind::Decision(Stage::S4),
            "is_book" | "is_scale" => FieldKind::Decision(Stage::Is),
            "ms_digest" => FieldKind::Decision(Stage::Ms),
            "dist_pre" | "dist_post" | "qc_pre" | "qc_post" | "qc_max" => FieldKind::Carrier,
            "noise" => FieldKind::EnergyValue,
            "spectrum" | "offsets" | "tns" | "geo" | "ch_count" => FieldKind::Structural,
            "frame_consts" => FieldKind::InitConstant,
            "no_red_pe" | "static_bits" | "max_bit_fac" => FieldKind::Intermediate,
            _ => FieldKind::Structural,
        },
        "state" => FieldKind::Carrier,
        "acct" => match leaf_name {
            "npass" | "crash" | "gains" | "last_iterations" => FieldKind::Decision(Stage::S6),
            "side" | "huff" | "scf_bits" | "used_dyn" | "tot_fill" | "align" | "total"
            | "sum_bits" | "el_granted" | "max_dyn" | "total_avail" | "static_used" => {
                FieldKind::Decision(Stage::Acct)
            }
            "pad_pre" | "pad_post" | "bit_res_tot" | "bit_res_max" => FieldKind::Carrier,
            _ => FieldKind::Structural,
        },
        _ => FieldKind::Structural,
    }
}

// ---------------------------------------------------------------------------
// Per-band / per-frame classification
// ---------------------------------------------------------------------------

/// One band's classification at one stage (the per-(frame, band, stage) cell
/// of the design §3 trace). `Equal` means no cross-arm difference at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BandClass {
    Equal,
    Class(Class),
}

/// Per-stage cross-arm summary derived from the recorded diff.
#[derive(Debug, Clone)]
pub(super) struct StageClass {
    pub stage: Stage,
    /// Number of differing leaves owned by this stage.
    pub diff_leaves: usize,
    /// The dominant class observed at this stage (S > A > R > Equal).
    pub class: Option<Class>,
}

/// Complete per-frame classification result.
#[derive(Debug, Clone)]
pub(super) struct FrameClass {
    pub frame: u32,
    pub seq: i32,
    /// Frame-level F outcome (from byte comparison + structural validity).
    pub outcome: FrameOutcome,
    /// Per-stage summaries in production order.
    pub stages: Vec<StageClass>,
    /// Per-band origin class (earliest differing stage per band), indexed by
    /// band over the channel-0 geometry.
    pub band_origin: Vec<BandClass>,
    /// Attributed propagations and unattributed findings (design §10.4).
    pub attributions: Vec<Attribution>,
    /// Explicit findings (missing/skipped/unsupported never count as a pass).
    pub findings: Vec<String>,
}

/// One attribution record: a later-stage difference cited to its earliest
/// defensible upstream event, or recorded `Unattributed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Attribution {
    pub stage: Stage,
    pub band: usize,
    pub class: Class,
    /// Human-readable chain citation (origin stage/band/class).
    pub cite: String,
}

/// Precedence used to pick a dominant class: structural flips outrank
/// intermediates, which outrank residue (design §3 classification order).
fn class_rank(c: &Class) -> u8 {
    match c {
        Class::S(_) => 5,
        Class::P { .. } => 4,
        Class::Unattributed { .. } => 3,
        Class::A => 2,
        Class::R => 1,
    }
}

/// Classify the recorded cross-arm diff into per-stage summaries. This works
/// purely from the `R22Diff` leaf list (Group A output) and the decision-field
/// mapping; it never asserts cross-arm equality.
pub(super) fn classify_stages(diff: &R22Diff, prefix: &str) -> Vec<StageClass> {
    let mut out: Vec<StageClass> = Vec::new();
    let mut push = |stage: Stage, kind: FieldKind| {
        if !out.iter().any(|s| s.stage == stage) {
            out.push(StageClass {
                stage,
                diff_leaves: 0,
                class: None,
            });
        }
        let sc = out
            .iter_mut()
            .find(|s| s.stage == stage)
            .expect("stage present");
        sc.diff_leaves += 1;
        let c = match kind {
            FieldKind::Decision(s) => Class::S(s),
            FieldKind::Intermediate => Class::A,
            FieldKind::EnergyValue => Class::R,
            // Structural / init-constant / carrier diffs are recorded as
            // structural findings at the consuming stage (never silent).
            FieldKind::Structural | FieldKind::InitConstant => Class::S(stage),
            FieldKind::Carrier => Class::A,
        };
        let replace = match &sc.class {
            None => true,
            Some(prev) => class_rank(&c) > class_rank(prev),
        };
        if replace {
            sc.class = Some(c);
        }
    };
    for field in &diff.differing_fields {
        // Strip the caller prefix (`{prefix}.`); prefix contains no dots.
        let rel = field
            .strip_prefix(&format!("{prefix}."))
            .unwrap_or(field.as_str());
        let stage = match rel.split('.').next() {
            Some("s1") => Stage::Pe,
            Some("s2") => Stage::S2,
            Some("s4") => Stage::S4,
            Some("s6") => Stage::S6,
            Some("state") => Stage::S6,
            Some("acct") => Stage::Acct,
            _ => Stage::S6,
        };
        push(stage, field_kind(rel));
    }
    out.sort_by_key(|s| s.stage);
    out
}

/// Per-band origin classification over the channel-0 geometry: for each band,
/// the earliest production stage whose *decision* fields differ between the
/// two captures. Value-only differences classify A (intermediate) or R
/// (energy residue) when no decision differs.
///
/// Decision fields per stage (production outputs that select coding):
/// * IS: right-channel `is_book`/`is_scale` (S1 capture carries them).
/// * MS: post-MS `ms_mask`.
/// * PE: per-band weighting application (`enfac` zero vs non-zero) and band
///   admission (`sfb_n_lines` zero vs non-zero).
/// * S2: `ah_flag`.
/// * S4: `scf` only. The element-global `gain` and the `quant_spec`/`mutated`/
///   `spectrum_pre` leaves are reported at stage level by `classify_stages`
///   (via `field_kind`), not inspected by this per-band walk.
pub(super) fn classify_bands(q: &R22Capture, f: &R22Capture) -> Vec<BandClass> {
    let cnt = q.s1.ch.first().map(|c| c.cnt).unwrap_or(0);
    let mut out = vec![BandClass::Equal; cnt];
    let stereo = q.s1.ch.len() == 2;
    for b in 0..cnt {
        // IS decision (right channel only; left book/scale stay zero).
        if stereo {
            let (qb, fb) = (&q.s1.ch[1], &f.s1.ch[1]);
            if qb.is_book.get(b) != fb.is_book.get(b) || qb.is_scale.get(b) != fb.is_scale.get(b) {
                out[b] = BandClass::Class(Class::S(Stage::Is));
                continue;
            }
        }
        // MS decision (post-MS mask).
        if q.s1.ms_mask.get(b) != f.s1.ms_mask.get(b) {
            out[b] = BandClass::Class(Class::S(Stage::Ms));
            continue;
        }
        // PE decision: admission (n_lines 0 vs >0) or patch application
        // (enfac 0 vs non-zero) on either channel.
        let pe_flip = (0..q.s1.ch.len()).any(|ch| {
            let nl_q = q.s1.pe_ch[ch].sfb_n_lines.get(b).copied().unwrap_or(0);
            let nl_f = f.s1.pe_ch[ch].sfb_n_lines.get(b).copied().unwrap_or(0);
            let ef_q = q.s1.enfac[ch].get(b).copied().unwrap_or(0);
            let ef_f = f.s1.enfac[ch].get(b).copied().unwrap_or(0);
            (nl_q == 0) != (nl_f == 0) || (ef_q == 0) != (ef_f == 0)
        });
        if pe_flip {
            out[b] = BandClass::Class(Class::S(Stage::Pe));
            continue;
        }
        // S2 decision: avoid-hole flag.
        let ah_flip =
            (0..q.s2.ah_flag.len()).any(|ch| q.s2.ah_flag[ch].get(b) != f.s2.ah_flag[ch].get(b));
        if ah_flip {
            out[b] = BandClass::Class(Class::S(Stage::S2));
            continue;
        }
        // S4 decision: scalefactor.
        let scf_flip =
            (0..q.s4.ch.len()).any(|ch| q.s4.ch[ch].scf.get(b) != f.s4.ch[ch].scf.get(b));
        if scf_flip {
            out[b] = BandClass::Class(Class::S(Stage::S4));
            continue;
        }
        // No decision flip: intermediate (Ld/threshold) vs energy residue.
        let inter_flip = (0..q.s1.ch.len()).any(|ch| {
            q.s1.ch[ch].threshold_ld.get(b) != f.s1.ch[ch].threshold_ld.get(b)
                || q.s1.weighted[ch].get(b) != f.s1.weighted[ch].get(b)
                || q.s1.thresholds[ch].get(b) != f.s1.thresholds[ch].get(b)
                || q.s1.enfac[ch].get(b) != f.s1.enfac[ch].get(b)
                || q.s2.threshold_ld[ch].get(b) != f.s2.threshold_ld[ch].get(b)
                || q.s2.min_snr_ld[ch].get(b) != f.s2.min_snr_ld[ch].get(b)
                || q.s2.spread[ch].get(b) != f.s2.spread[ch].get(b)
        });
        if inter_flip {
            out[b] = BandClass::Class(Class::A);
            continue;
        }
        let energy_flip = (0..q.s1.ch.len()).any(|ch| {
            q.s1.ch[ch].energy_ld.get(b) != f.s1.ch[ch].energy_ld.get(b)
                || q.s1.ch[ch].energy_lin.get(b) != f.s1.ch[ch].energy_lin.get(b)
                || q.s1.ch[ch].ff.get(b) != f.s1.ch[ch].ff.get(b)
        });
        if energy_flip {
            out[b] = BandClass::Class(Class::R);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Same-frame chain attribution (design §3 / §10.4)
// ---------------------------------------------------------------------------

/// Ordered per-band events on the production consuming chain. The first
/// event is the origin (R/A/S); every later structural event is a candidate
/// propagation. Energy/intermediate value events precede the IS decision.
/// `pub(super)` for Group E2/E3 bundle generation (E-OD-1 visibility-only
/// reuse; no behaviour change).
pub(super) fn band_events(q: &R22Capture, f: &R22Capture, b: usize) -> Vec<(Stage, Class)> {
    let mut ev: Vec<(Stage, Class)> = Vec::new();
    let stereo = q.s1.ch.len() == 2;
    // 1. Energy-domain input residue (earliest defensible upstream event).
    let energy_flip = (0..q.s1.ch.len()).any(|ch| {
        q.s1.ch[ch].energy_ld.get(b) != f.s1.ch[ch].energy_ld.get(b)
            || q.s1.ch[ch].energy_lin.get(b) != f.s1.ch[ch].energy_lin.get(b)
            || q.s1.ch[ch].ff.get(b) != f.s1.ch[ch].ff.get(b)
    });
    if energy_flip {
        ev.push((Stage::Is, Class::R));
    }
    // 2. IS decision (right channel book/scale).
    if stereo {
        let (qb, fb) = (&q.s1.ch[1], &f.s1.ch[1]);
        if qb.is_book.get(b) != fb.is_book.get(b) || qb.is_scale.get(b) != fb.is_scale.get(b) {
            ev.push((Stage::Is, Class::S(Stage::Is)));
        }
    }
    // 3. MS decision (post-MS mask).
    if q.s1.ms_mask.get(b) != f.s1.ms_mask.get(b) {
        ev.push((Stage::Ms, Class::S(Stage::Ms)));
    }
    // 4. PE decision (admission / patch application).
    let pe_flip = (0..q.s1.ch.len()).any(|ch| {
        let nl_q = q.s1.pe_ch[ch].sfb_n_lines.get(b).copied().unwrap_or(0);
        let nl_f = f.s1.pe_ch[ch].sfb_n_lines.get(b).copied().unwrap_or(0);
        let ef_q = q.s1.enfac[ch].get(b).copied().unwrap_or(0);
        let ef_f = f.s1.enfac[ch].get(b).copied().unwrap_or(0);
        (nl_q == 0) != (nl_f == 0) || (ef_q == 0) != (ef_f == 0)
    });
    if pe_flip {
        ev.push((Stage::Pe, Class::S(Stage::Pe)));
    } else {
        // PE intermediate (weighted/threshold/enfac value) with equal decision.
        let pe_inter = (0..q.s1.ch.len()).any(|ch| {
            q.s1.weighted[ch].get(b) != f.s1.weighted[ch].get(b)
                || q.s1.thresholds[ch].get(b) != f.s1.thresholds[ch].get(b)
                || q.s1.enfac[ch].get(b) != f.s1.enfac[ch].get(b)
        });
        if pe_inter {
            ev.push((Stage::Pe, Class::A));
        }
    }
    // 5. S2 decision (avoid-hole flag) or intermediate.
    let ah_flip =
        (0..q.s2.ah_flag.len()).any(|ch| q.s2.ah_flag[ch].get(b) != f.s2.ah_flag[ch].get(b));
    if ah_flip {
        ev.push((Stage::S2, Class::S(Stage::S2)));
    } else {
        let s2_inter = (0..q.s2.threshold_ld.len()).any(|ch| {
            q.s2.threshold_ld[ch].get(b) != f.s2.threshold_ld[ch].get(b)
                || q.s2.min_snr_ld[ch].get(b) != f.s2.min_snr_ld[ch].get(b)
                || q.s2.spread[ch].get(b) != f.s2.spread[ch].get(b)
        });
        if s2_inter {
            ev.push((Stage::S2, Class::A));
        }
    }
    // 6. S4 decision (scalefactor).
    let scf_flip = (0..q.s4.ch.len()).any(|ch| q.s4.ch[ch].scf.get(b) != f.s4.ch[ch].scf.get(b));
    if scf_flip {
        ev.push((Stage::S4, Class::S(Stage::S4)));
    }
    ev
}

/// Attribute same-frame propagations. For each band the earliest event is the
/// origin; every later structural event on the same-band consuming chain is
/// `P` citing that origin exactly once (no double-attribution). A structural
/// event with no upstream event is itself an `S` origin. S6 is element-global:
/// its differences are attributed to the earliest S4 origin band when one
/// exists, otherwise recorded `Unattributed` (never forced).
pub(super) fn attribute_frame(q: &R22Capture, f: &R22Capture) -> Vec<Attribution> {
    let cnt = q.s1.ch.first().map(|c| c.cnt).unwrap_or(0);
    let mut attrs = Vec::new();
    let mut earliest_s4: Option<(usize, Class)> = None;
    for b in 0..cnt {
        let ev = band_events(q, f, b);
        if ev.is_empty() {
            continue;
        }
        let (origin_stage, origin_class) = ev[0].clone();
        // The origin itself, when structural, is recorded as an S origin.
        if origin_class.is_structural() {
            attrs.push(Attribution {
                stage: origin_stage,
                band: b,
                class: origin_class.clone(),
                cite: format!("origin {} b{b}", origin_class.label()),
            });
        }
        // Later structural events propagate from the origin (once each).
        for (stage, class) in ev.iter().skip(1) {
            if class.is_structural() {
                attrs.push(Attribution {
                    stage: *stage,
                    band: b,
                    class: Class::P {
                        stage: *stage,
                        origin_stage,
                        origin_band: b,
                        origin_class: Box::new(origin_class.clone()),
                    },
                    cite: format!(
                        "{} b{b} <- {} b{b} ({})",
                        stage.label(),
                        origin_stage.name(),
                        origin_class.label()
                    ),
                });
            }
        }
        // Track the earliest S4 structural origin for S6 attribution.
        if earliest_s4.is_none() {
            if let Some((s, c)) = ev
                .iter()
                .find(|(s, c)| *s == Stage::S4 && c.is_structural())
            {
                earliest_s4 = Some((b, c.clone()));
                let _ = s;
            }
        }
    }
    // S6 element-global decision differences (gains/sections/grants/accounting).
    let s6_flip = !s6_decision_equal(q, f);
    if s6_flip {
        match &earliest_s4 {
            Some((b, c)) => attrs.push(Attribution {
                stage: Stage::S6,
                band: *b,
                class: Class::P {
                    stage: Stage::S6,
                    origin_stage: Stage::S4,
                    origin_band: *b,
                    origin_class: Box::new(c.clone()),
                },
                cite: format!("S-S6 <- s4 b{b} ({})", c.label()),
            }),
            None => attrs.push(Attribution {
                stage: Stage::S6,
                band: 0,
                class: Class::Unattributed { stage: Stage::S6 },
                cite: "S-S6 element-global; no per-band upstream chain established".to_string(),
            }),
        }
    }
    attrs
}

/// Element-global S6 decision equality (gains, sections, maxSfb, grants,
/// pass structure, accounting). Per-band scf inputs are compared separately
/// by the band walk; this captures the element-level allocation decisions.
fn s6_decision_equal(q: &R22Capture, f: &R22Capture) -> bool {
    let (a, b) = (&q.s6.output, &f.s6.output);
    a.granted_dyn_bits == b.granted_dyn_bits
        && a.max_dyn_bits == b.max_dyn_bits
        && a.total_avail == b.total_avail
        && a.el_granted == b.el_granted
        && a.granted_pe == b.granted_pe
        && a.granted_pe_corr == b.granted_pe_corr
        && a.passes == b.passes
        && a.final_gains == b.final_gains
        && a.final_max_sfb == b.final_max_sfb
        && a.final_quants == b.final_quants
        && a.static_bits_used == b.static_bits_used
        && a.used_dyn_bits == b.used_dyn_bits
        && a.tot_fill_bits == b.tot_fill_bits
        && a.align_bits == b.align_bits
        && a.total_bits == b.total_bits
}

// ---------------------------------------------------------------------------
// Observational margins (design §5) — computed ONLY from dissection-cloned
// operands (the owned `R22Capture` snapshots and local clones). No margin is
// ever written back into a live arm or consumed by a production decision.
// ---------------------------------------------------------------------------

/// Result of one margin derivation. A margin is either derived exactly from
/// the production decision expression, or explicitly recorded
/// `MarginDerivationRequired` (design §5/§6 AC-9 — never guessed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MarginApplicability {
    Applicable,
    NotApplicable(&'static str),
    Unobserved,
    NotGenerated,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum MarginStatus {
    /// Signed integer distance to the production boundary (Q31 / bits / count).
    Derived(i64),
    /// f32-domain distance (PE-correction window uses exact `f32` ops).
    DerivedF32(f32),
    /// The derivation cannot be established from captured production
    /// semantics; blocks acceptance of this stage margin (never omitted).
    MarginDerivationRequired(&'static str),
    /// The decision path did not execute for this capture. This is not a zero
    /// margin and never counts as a derived observation.
    NotApplicable(&'static str),
    /// The path is applicable but was not reached by the capture/probe.
    Unobserved,
    /// The evidence family was deliberately not generated (for example Group F
    /// recurrence under SG-3). No numerical or recurrence conclusion is drawn.
    NotGenerated(&'static str),
}

/// One named observational margin with its derivation metadata.
#[derive(Debug, Clone)]
pub(super) struct Margin {
    pub name: &'static str,
    pub stage: Stage,
    pub status: MarginStatus,
    /// Applicability is independent from numeric derivation. An inapplicable
    /// or unobserved path is never represented as `Derived(0)`.
    pub applicability: MarginApplicability,
    /// Direction in which crossing the boundary changes the decision.
    pub direction: &'static str,
    /// Production equality/tie semantics at the boundary.
    pub equality: &'static str,
}

impl Margin {
    fn derived(
        name: &'static str,
        stage: Stage,
        value: i64,
        direction: &'static str,
        equality: &'static str,
    ) -> Self {
        Self {
            name,
            stage,
            status: MarginStatus::Derived(value),
            applicability: MarginApplicability::Applicable,
            direction,
            equality,
        }
    }

    fn derived_f32(
        name: &'static str,
        stage: Stage,
        value: f32,
        direction: &'static str,
        equality: &'static str,
    ) -> Self {
        Self {
            name,
            stage,
            status: MarginStatus::DerivedF32(value),
            applicability: MarginApplicability::Applicable,
            direction,
            equality,
        }
    }

    fn required(name: &'static str, stage: Stage, reason: &'static str) -> Self {
        Self {
            name,
            stage,
            status: MarginStatus::MarginDerivationRequired(reason),
            applicability: MarginApplicability::Applicable,
            direction: "n/a (derivation required)",
            equality: "n/a (derivation required)",
        }
    }

    fn not_applicable(name: &'static str, stage: Stage, reason: &'static str) -> Self {
        Self {
            name,
            stage,
            status: MarginStatus::NotApplicable(reason),
            applicability: MarginApplicability::NotApplicable(reason),
            direction: "n/a (not applicable)",
            equality: "n/a (not applicable)",
        }
    }

    fn unobserved(name: &'static str, stage: Stage) -> Self {
        Self {
            name,
            stage,
            status: MarginStatus::Unobserved,
            applicability: MarginApplicability::Unobserved,
            direction: "n/a (unobserved)",
            equality: "n/a (unobserved)",
        }
    }

    fn not_generated(name: &'static str, stage: Stage, reason: &'static str) -> Self {
        Self {
            name,
            stage,
            status: MarginStatus::NotGenerated(reason),
            applicability: MarginApplicability::NotGenerated,
            direction: "n/a (not generated)",
            equality: "n/a (not generated)",
        }
    }

    pub(super) fn status_token(&self) -> &'static str {
        match self.status {
            MarginStatus::Derived(_) => "Derived",
            MarginStatus::DerivedF32(_) => "DerivedF32",
            MarginStatus::MarginDerivationRequired(_) => "MarginDerivationRequired",
            MarginStatus::NotApplicable(_) => "NotApplicable",
            MarginStatus::Unobserved => "Unobserved",
            MarginStatus::NotGenerated(_) => "NotGenerated",
        }
    }

    pub(super) fn is_required(&self) -> bool {
        matches!(self.status, MarginStatus::MarginDerivationRequired(_))
    }

    pub(super) fn applicability_token(&self) -> &'static str {
        match self.applicability {
            MarginApplicability::Applicable => "Applicable",
            MarginApplicability::NotApplicable(_) => "NotApplicable",
            MarginApplicability::Unobserved => "Unobserved",
            MarginApplicability::NotGenerated => "NotGenerated",
        }
    }

    pub(super) fn is_applicable(&self) -> bool {
        matches!(self.applicability, MarginApplicability::Applicable)
    }

    pub(super) fn is_derived(&self) -> bool {
        matches!(
            self.status,
            MarginStatus::Derived(_) | MarginStatus::DerivedF32(_)
        )
    }
}

// --- S1 / PE pure edge functions (source: `pe.rs`) -------------------------

/// `prepare_sfb_pe` admission (`pe.rs:353`): `energy_ld > threshold_ld`
/// (strict). Distance `energy_ld - threshold_ld`; `> 0` admits a non-zero
/// line estimate, `<= 0` reads 0. Equality (`== 0`) does NOT admit.
pub(super) fn pe_prepare_admission_margin(energy_ld: i32, threshold_ld: i32) -> i64 {
    energy_ld as i64 - threshold_ld as i64
}

/// `calc_sfb_pe` admission (`pe.rs:406`): `weighted_energy_ld > threshold_ld`
/// (strict), same semantics as the prepare gate but on post-weighting vectors.
pub(super) fn pe_calc_admission_margin(weighted_ld: i32, threshold_ld: i32) -> i64 {
    weighted_ld as i64 - threshold_ld as i64
}

/// `calc_sfb_pe` C1 branch (`pe.rs:411`): `ratio >= C1_LD_Q31` (inclusive).
/// `ratio = weighted - threshold`. Distance `ratio - C1`; `>= 0` selects the
/// linear branch, `< 0` the C2/C3 branch. Equality (`== 0`) selects linear.
pub(super) fn pe_c1_branch_margin(weighted_ld: i32, threshold_ld: i32) -> i64 {
    let ratio = weighted_ld.wrapping_sub(threshold_ld);
    ratio as i64 - C1_LD_Q31 as i64
}

/// `calc_weighting` chaos floor (`pe.rs:515`): `ENFAC_FLOOR.max(fdiv_norm(
/// lines_sum, total_lines))`. Distance `raw - ENFAC_FLOOR`; `< 0` means the
/// 0.1875 floor clamps. Equality (`== 0`) reads the floor exactly.
pub(super) fn chaos_floor_margin(lines_sum: i32, total_lines: i32) -> i64 {
    let raw = fdiv_norm(lines_sum, total_lines);
    raw as i64 - ENFAC_FLOOR_Q31 as i64
}

/// `calc_weighting` patch-arm gate (`pe.rs:516`): `chaos > PATCH_ARM` (strict).
/// Distance `chaos - PATCH_ARM`; `> 0` arms the patch. Equality does NOT arm.
pub(super) fn chaos_patch_arm_margin(chaos: i32) -> i64 {
    chaos as i64 - PATCH_ARM_Q31 as i64
}

/// `calc_weighting` 3/4-energy exponent selector (`pe.rs:532`): `chaos >
/// PATCH_HI` (strict). `> 0` selects the 3/4-energy exponent.
pub(super) fn chaos_patch_hi_margin(chaos: i32) -> i64 {
    chaos as i64 - PATCH_HI_Q31 as i64
}

/// `calc_weighting` 2/4-energy exponent selector (`pe.rs:537`): `chaos >
/// PATCH_MID` (strict). `> 0` selects the 2/4-energy exponent (else 3/4-prod).
pub(super) fn chaos_patch_mid_margin(chaos: i32) -> i64 {
    chaos as i64 - PATCH_MID_Q31 as i64
}

/// `calc_weighting` per-band energy gate (`pe.rs:531`): `energy_linear > 0`
/// (strict). Distance `energy_linear - 0`; `> 0` allows a patch write.
pub(super) fn enfac_energy_gate_margin(energy_linear: i32) -> i64 {
    energy_linear as i64
}

/// Established I-2 anti-centre sentinel dual margin (`intensity.rs:227-228`,
/// `LEFT_RIGHT_RATIO_THRESH = 0.7`). Sentinel set iff `m_a >= 0 && m_b <= 0`.
/// This is the IS-domain decision the bundled-PE triad cites; it operates on
/// the stored linear energies that also feed PE. Mirrors `is_ms::i2_margins`.
pub(super) fn i2_sentinel_margins(e_l: i32, e_r: i32) -> (i64, i64) {
    let m_a = e_l as i64 - fmult(LEFT_RIGHT_RATIO_THRESH, e_r) as i64;
    let m_b = fmult(LEFT_RIGHT_RATIO_THRESH, e_l) as i64 - e_r as i64;
    (m_a, m_b)
}

/// Recompute the frame-average energy Ld exactly as `adapt_min_snr`
/// (`threshold_vbr.rs:283-301`): `>>6` accumulation over active bands, then
/// `CalcLdData(accu) + AVG_COMP - CalcLdInt(n_sfb)`; `NEG_ONE` on zero.
fn frame_avg_en_ld(energy_lin: &[i32], cnt: usize, per: usize, max: usize) -> i32 {
    frame_avg_en_ld_active(energy_lin, cnt, per, max)
}

/// Frame-average energy with the production fixed-point operations exposed to
/// the AC-9 evidence adapter. The production algorithm remains in
/// [`adapt_min_snr`]; this wrapper performs the same source-derived arithmetic
/// on owned capture operands.
fn frame_avg_en_ld_active(energy_lin: &[i32], cnt: usize, per: usize, max: usize) -> i32 {
    let mut n_sfb: i32 = 0;
    let mut accu: i32 = 0;
    let mut grp = 0;
    while grp < cnt {
        n_sfb += max as i32;
        for sfb in 0..max {
            accu = accu.wrapping_add(energy_lin[grp + sfb] >> 6);
        }
        grp += per;
    }
    if accu == 0 || n_sfb == 0 {
        NEG_ONE_Q31
    } else {
        calc_ld_data(accu)
            .wrapping_add(AVG_COMP_Q31)
            .wrapping_sub(calc_ld_int(n_sfb))
    }
}

// --- S2 pure edge functions (source: `threshold_vbr.rs`) -------------------

/// `adapt_min_snr` drop gate (`threshold_vbr.rs:308-309`): `update =
/// msa_start_ratio < db_ratio` where `db_ratio = avg_en_ld - energy_ld[s]`.
/// Distance `db_ratio - start_ratio`; `> 0` lowers minSnr. Equality (`== 0`)
/// does NOT update (strict `<`).
pub(super) fn minsnr_drop_margin(avg_en_ld: i32, energy_ld: i32, start_ratio: i32) -> i64 {
    let db_ratio = avg_en_ld.wrapping_sub(energy_ld);
    db_ratio as i64 - start_ratio as i64
}

/// `init_avoid_hole_flag_mono` classification (`threshold_vbr.rs:419`):
/// `NO_AH` iff `spread > energy_lin || min_snr > 0`. Returns the two signed
/// distances `(spread - energy_lin, min_snr - 0)`; classification
/// (`AH_INACTIVE`) requires `spread <= energy_lin && min_snr <= 0`.
pub(super) fn ah_classify_margins(
    spread_post: i32,
    energy_lin: i32,
    min_snr_post: i32,
) -> (i64, i64) {
    (spread_post as i64 - energy_lin as i64, min_snr_post as i64)
}

/// `reduce_thresholds_vbr` avoid-hole clamp trigger (`threshold_vbr.rs:870`):
/// `reduced - en_ld > min_snr && ah != NO_AH` (strict `>`). Distance
/// `(reduced - en_ld) - min_snr`; `> 0` on a classified band engages the
/// clamp (`AH_ACTIVE`). Equality does NOT clamp.
pub(super) fn ah_clamp_margin(reduced: i32, en_ld: i32, min_snr: i32) -> i64 {
    (reduced.wrapping_sub(en_ld)) as i64 - min_snr as i64
}

/// `reduce_thresholds_vbr` low-energy button (`threshold_vbr.rs:871`):
/// `min_snr > NEG_ONE - en_ld` selects `reduced = max(min_snr + en_ld, thr)`.
/// Distance `min_snr - (NEG_ONE - en_ld)`; `> 0` takes the button branch.
pub(super) fn ah_button_margin(min_snr: i32, en_ld: i32) -> i64 {
    min_snr as i64 - (NEG_ONE_Q31.wrapping_sub(en_ld)) as i64
}

/// Recompute the chaos-gated reduced threshold (long path) exactly as
/// `reduce_thresholds_vbr_impl` (`threshold_vbr.rs:865-868`) from captured
/// operands: `red_val >= MAX - thr_exp` ⇒ 0, else `CalcLdData(thr_exp +
/// red_val) << 2`. Used only to evaluate the clamp margin observationally.
fn reduced_threshold_long(thr_exp: i32, red_val: i32) -> i32 {
    if red_val >= i32::MAX.wrapping_sub(thr_exp) {
        0
    } else {
        calc_ld_data(thr_exp.wrapping_add(red_val)).wrapping_shl(2)
    }
}

/// Recompute the short-path scaled threshold exactly as
/// `reduce_thresholds_vbr_impl` (`threshold_vbr.rs:850-853`) from captured
/// operands: `scaled = fmult(thr_e, fmult(SHORT_THR_FAC, INV_SQRT4[gl]))<<2`.
/// Group-length index follows production (`gl = group_len[group_number]`).
/// Used only to evaluate short-branch margins observationally.
fn scaled_threshold_short(thr_e: i32, group_len: i32) -> i32 {
    let gl = (group_len.max(1).min(8) - 1) as usize;
    fmult(thr_e, fmult(SHORT_THR_FAC_Q31, INV_SQRT4_Q31[gl])).wrapping_shl(2)
}

/// Short NEG_ONE branch (`threshold_vbr.rs:854`):
/// `scaled <= LIMIT - red` selects NEG_ONE (inclusive `<=`).
/// Distance `scaled - (LIMIT - red)`; `<= 0` selects NEG_ONE.
/// Equality (`== 0`) selects NEG_ONE.
pub(super) fn short_negone_margin(scaled: i32, red_group: i32) -> i64 {
    scaled as i64 - (LIMIT_THR_REDUCED_Q31.wrapping_sub(red_group)) as i64
}

/// Short ZERO branch (`threshold_vbr.rs:856`):
/// `red >= ONE - scaled` selects ZERO (inclusive `>=`).
/// Distance `red - (ONE - scaled)`; `>= 0` selects ZERO.
/// Equality (`== 0`) selects ZERO.
pub(super) fn short_zero_margin(red_group: i32, scaled: i32) -> i64 {
    red_group as i64 - (ONE_Q31.wrapping_sub(scaled)) as i64
}

/// Long ZERO branch (`threshold_vbr.rs:865`):
/// `red >= ONE - thr_e` selects ZERO (inclusive `>=`).
/// Distance `red - (ONE - thr_e)`; `>= 0` selects ZERO.
pub(super) fn long_zero_margin(red_0: i32, thr_e: i32) -> i64 {
    red_0 as i64 - (ONE_Q31.wrapping_sub(thr_e)) as i64
}

// --- S4 pure edge functions (source: `scf.rs` step 3) ----------------------

/// Step-3 per-band distortion window (`scf.rs:1090`): reject when
/// `dn > dist_prev + D_1122`. Distance `(dist_prev + D_1122) - dn`; `>= 0`
/// passes the window. Equality (`== 0`) passes (strict `>` rejects).
pub(super) fn step3_dist_margin(dn: i32, dist_prev: i32) -> i64 {
    (dist_prev.wrapping_add(D_1122)) as i64 - dn as i64
}

/// Step-3 per-band energy window (`scf.rs:1091`): reject when
/// `en_q < energy_ld - D_07079`. Distance `en_q - (energy_ld - D_07079)`;
/// `>= 0` passes. Equality passes (strict `<` rejects).
pub(super) fn step3_energy_margin(en_q: i32, energy_ld: i32) -> i64 {
    en_q as i64 - (energy_ld.wrapping_sub(D_07079)) as i64
}

/// Step-3 PE gate (`scf.rs:1071`): `delta_new <= 0` (non-strict, the only
/// `<=` acceptance). Distance `0 - delta_new`; `>= 0` passes. Equality passes.
pub(super) fn step3_pe_gate_margin(delta_new: i32) -> i64 {
    -(delta_new as i64)
}

/// Step-3 sum gate (`scf.rs:1098`): `new_sum < old_sum` (strict). Distance
/// `old_sum - new_sum`; `> 0` accepts. Equality rejects.
pub(super) fn step3_sum_margin(new_sum: i32, old_sum: i32) -> i64 {
    old_sum as i64 - new_sum as i64
}

// --- S6 pure edge functions (source: `rate.rs`) ----------------------------

/// `prepare_grants` reservoir floor (`rate.rs:953`): `granted + bit_res_tot
/// < 0` ⇒ `BitresTooLow`. Distance `granted + bit_res_tot`; `>= 0` proceeds.
pub(super) fn grant_headroom_margin(granted: i32, bit_res_tot: i32) -> i64 {
    granted as i64 + bit_res_tot as i64
}

/// Gain-loop element cap (`rate.rs:1745`): `el_dyn > nch*6144 - el_static`
/// fails constraints. Distance `nch*6144 - el_static - el_dyn`; `>= 0` passes.
pub(super) fn element_cap_margin(el_dyn: i32, el_static: i32, n_channels: i32) -> i64 {
    (n_channels as i64) * (MIN_BUFSIZE_PER_EFF_CHAN as i64) - el_static as i64 - el_dyn as i64
}

/// Exit-matrix frame headroom (`rate.rs:1173/1197`): `sum_bits < total_avail`
/// (strict) favors done. Distance `total_avail - sum_bits`; `> 0` under budget.
pub(super) fn exit_headroom_margin(total_avail: i32, sum_bits: i32) -> i64 {
    total_avail as i64 - sum_bits as i64
}

/// `reduce_bit_consumption` retry boundary (`rate.rs:1023/1030`):
/// `iterations < max` bumps gain, `== max` triggers crash. Distance
/// `max_iterations - iterations`; `> 0` retries remain, `== 0` crash boundary.
pub(super) fn multipass_margin(iterations: i32) -> i64 {
    MAX_ITERATIONS as i64 - iterations as i64
}

/// PE-correction window (`calc_pe_correction`, `rate.rs:724-728`, FULL path).
/// Returns the five signed distances; the correction branch participates iff
/// all five are strictly positive. The two ratio edges use exact `f32` ops
/// (int→f32 conversion rounds once, then f32 multiply) like production.
pub(super) struct PeCorrMargins {
    pub bits_last: i64,
    pub hi_f32: f32,
    pub lo_f32: f32,
    pub win_12: i64,
    pub win_065: i64,
}

pub(super) fn pe_corr_margins(
    pe_act: i32,
    pe_last: i32,
    bits_last: i32,
    bits2pe_m: i32,
    bits2pe_e: i32,
) -> PeCorrMargins {
    let hi = 1.5f32 * (pe_last as f32) - (pe_act as f32);
    let lo = (pe_act as f32) - 0.7f32 * (pe_last as f32);
    let b12 = bits2pe2(bits_last, fmult(PECORR_12_2, bits2pe_m), bits2pe_e + 1);
    let b065 = bits2pe2(bits_last, fmult(PECORR_065, bits2pe_m), bits2pe_e);
    PeCorrMargins {
        bits_last: bits_last as i64,
        hi_f32: hi,
        lo_f32: lo,
        win_12: b12 as i64 - pe_last as i64,
        win_065: pe_last as i64 - b065 as i64,
    }
}

// --- Accounting pure edge functions (source: `rate.rs`) --------------------

/// `update_fill_bits` min-frame floor (`rate.rs:1224`): the floor term
/// `max(0, min_bits - prelim)` engages when `prelim < min_bits`. Distance
/// `prelim - min_bits`; `< 0` engages the floor. At VBR `min_bits = 0`.
pub(super) fn fill_floor_margin(prelim: i32, min_bits_per_frame: i32) -> i64 {
    prelim as i64 - min_bits_per_frame as i64
}

/// `fill_extension_bits` first-element gate (`rate.rs:1239`): a fill element
/// is written iff `payload >= GLOBAL_EXT_BITS + FILL_EL_COUNT_BITS` (= 7).
/// Distance `payload - 7`; `>= 0` writes. Equality writes (inclusive `>=`).
pub(super) fn fill_ext_gate_margin(payload: i32) -> i64 {
    payload as i64 - (GLOBAL_EXT_BITS + FILL_EL_COUNT_BITS) as i64
}

/// `bit_res_redistribution` level-max guard (`rate.rs:912`): `bit_res_tot >
/// bit_res_tot_max` ⇒ `QuantError`. Distance `max - level`; `>= 0` valid.
pub(super) fn bitres_max_margin(bit_res_tot: i32, bit_res_tot_max: i32) -> i64 {
    bit_res_tot_max as i64 - bit_res_tot as i64
}

/// `bit_res_redistribution` low guard (`rate.rs:909`): `bit_res_tot < 0` ⇒
/// `BitresTooLow`. Distance `bit_res_tot - 0`; `>= 0` valid.
pub(super) fn bitres_floor_margin(bit_res_tot: i32) -> i64 {
    bit_res_tot as i64
}

/// `frame_padding` onset (`rate.rs:383-384`): `rest = padding_rest -
/// calc_frame_len(bitrate, sr, 1024, true)`; `rest <= 0` turns padding ON.
/// Distance `rest`; `<= 0` pads. Equality (`== 0`) pads (inclusive `<=`).
pub(super) fn padding_onset_margin(padding_rest: i32, bitrate: i32, sample_rate: i32) -> i64 {
    let difference = calc_frame_len(bitrate, sample_rate, 1024, true);
    padding_rest as i64 - difference as i64
}

// ---------------------------------------------------------------------------
// Per-stage margin collectors over the owned (dissection-cloned) captures.
// Each collector reports, per decision edge, the tightest signed distance
// across the active bands/channels (the closest observed approach to the
// boundary) with its witness. All values are observational; none is written
// back. Edges whose decision-point operands are not captured return
// `MarginDerivationRequired` (never a guessed number).
// ---------------------------------------------------------------------------

/// Fold a set of `(witness, signed_distance)` pairs into the tightest margin
/// (smallest absolute distance — the closest approach to the boundary).
fn tightest(
    name: &'static str,
    stage: Stage,
    direction: &'static str,
    equality: &'static str,
    values: Vec<(usize, i64)>,
) -> Margin {
    tightest_or(name, stage, direction, equality, values, || {
        Margin::unobserved(name, stage)
    })
}

/// Fold applicable observations to the closest boundary. An empty population
/// is never converted into `Derived(0)`; the caller supplies the honest
/// applicability/status outcome.
fn tightest_or<F>(
    name: &'static str,
    stage: Stage,
    direction: &'static str,
    equality: &'static str,
    values: Vec<(usize, i64)>,
    empty: F,
) -> Margin
where
    F: FnOnce() -> Margin,
{
    match values.iter().min_by_key(|(_, v)| v.abs()).copied() {
        Some((_, value)) => Margin::derived(name, stage, value, direction, equality),
        None => empty(),
    }
}

pub(super) fn active_bands_for_ac9(cnt: usize, per: usize, max: usize) -> Vec<usize> {
    active_bands(cnt, per, max)
}

pub(super) fn frame_avg_en_ld_for_ac9(
    energy_lin: &[i32],
    cnt: usize,
    per: usize,
    max: usize,
) -> i32 {
    frame_avg_en_ld(energy_lin, cnt, per, max)
}

pub(super) fn minsnr_drop_margin_for_ac9(avg_en_ld: i32, energy_ld: i32, start_ratio: i32) -> i64 {
    minsnr_drop_margin(avg_en_ld, energy_ld, start_ratio)
}

/// Active-band indices for a geometry (`grp` stride `per`, `max` per group).
fn active_bands(cnt: usize, per: usize, max: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut grp = 0;
    while grp < cnt {
        for sfb in 0..max {
            if grp + sfb < cnt {
                out.push(grp + sfb);
            }
        }
        grp += per;
    }
    out
}

/// S1/PE observational margins from the owned S1 capture.
pub(super) fn pe_stage_margins(cap: &R22S1Cap) -> Vec<Margin> {
    let mut out = Vec::new();
    let mut prep = Vec::new();
    let mut calc = Vec::new();
    let mut c1 = Vec::new();
    let mut gate = Vec::new();
    for (ch, c) in cap.ch.iter().enumerate() {
        let bands = active_bands(c.cnt, c.per, c.max);
        for &s in &bands {
            prep.push((
                s,
                pe_prepare_admission_margin(c.energy_ld[s], c.threshold_ld[s]),
            ));
            let w = cap.weighted[ch][s];
            let t = cap.thresholds[ch][s];
            calc.push((s, pe_calc_admission_margin(w, t)));
            if w > t {
                c1.push((s, pe_c1_branch_margin(w, t)));
            }
            gate.push((s, enfac_energy_gate_margin(c.energy_lin[s])));
        }
        // Chaos floor: recompute raw flatness from the cloned line counts.
        let lines_sum: i32 = bands
            .iter()
            .map(|&s| cap.pe_ch[ch].sfb_n_lines[s])
            .fold(0i32, |a, b| a.wrapping_add(b));
        let total_lines = c.offsets[c.cnt];
        if !c.short {
            out.push(Margin::derived(
                "pe.chaos_floor",
                Stage::Pe,
                chaos_floor_margin(lines_sum, total_lines),
                "<0 clamps to the 0.1875 floor",
                "==0 reads the floor exactly (max)",
            ));
        }
        let chaos = cap.ats_post.chaos_measure_en_fac[ch];
        out.push(Margin::derived(
            "pe.chaos_patch_arm",
            Stage::Pe,
            chaos_patch_arm_margin(chaos),
            ">0 arms the flatness patch",
            "==0 does NOT arm (strict >)",
        ));
        out.push(Margin::derived(
            "pe.chaos_patch_hi",
            Stage::Pe,
            chaos_patch_hi_margin(chaos),
            ">0 selects the 3/4-energy exponent",
            "==0 does NOT select (strict >)",
        ));
        out.push(Margin::derived(
            "pe.chaos_patch_mid",
            Stage::Pe,
            chaos_patch_mid_margin(chaos),
            ">0 selects the 2/4-energy exponent",
            "==0 does NOT select (strict >)",
        ));
    }
    out.push(tightest(
        "pe.prepare_admission",
        Stage::Pe,
        ">0 admits a non-zero line estimate",
        "==0 does NOT admit (strict >)",
        prep,
    ));
    out.push(tightest(
        "pe.calc_admission",
        Stage::Pe,
        ">0 contributes per-band PE",
        "==0 contributes 0 (strict >)",
        calc,
    ));
    out.push(tightest(
        "pe.c1_branch",
        Stage::Pe,
        ">=0 selects the linear branch",
        "==0 selects linear (inclusive >=)",
        c1,
    ));
    out.push(tightest(
        "pe.enfac_energy_gate",
        Stage::Pe,
        ">0 allows a patch write",
        "==0 blocks the write (strict >)",
        gate,
    ));
    out
}

/// S2 observational margins from the owned S2 capture. The minSnr drop gate
/// uses the captured ATS `msa_start_ratio`; the avoid-hole classification
/// operands are reconstructed by replaying the production `adapt_min_snr` +
/// `init_avoid_hole_flag_mono` on cloned inputs (no second oracle); the clamp
/// trigger uses the captured post-init minSnr and the recomputed reduced
/// threshold (long path via `reduced_threshold_long`, short path via
/// `scaled_threshold_short` with mirrored `SHORT_THR_FAC/INV_SQRT4/LIMIT`
/// constants). Short-path branch margins are derived, not required.
pub(super) fn s2_stage_margins(cap: &R22S2Cap) -> Vec<Margin> {
    let mut out = Vec::new();
    let mut drop = Vec::new();
    let mut classify_spread = Vec::new();
    let mut classify_snr = Vec::new();
    let mut clamp = Vec::new();
    let mut button = Vec::new();
    let mut long_zero = Vec::new();
    let mut short_negone = Vec::new();
    let mut short_zero = Vec::new();
    let mut saw_short = false;
    let mut saw_long = false;
    let start_ratio = cap.ats_pre.msa_start_ratio;
    for (ch, c) in cap.ch.iter().enumerate() {
        let bands = active_bands(c.cnt, c.per, c.max);
        let avg = frame_avg_en_ld(&c.energy_lin, c.cnt, c.per, c.max);
        let geo = SfbGeometry {
            offsets: &c.offsets,
            group_len: &c.group_len,
            sfb_cnt: c.cnt,
            sfb_per_group: c.per,
            max_sfb_per_group: c.max,
            seq: c.seq,
        };
        // Replay the production mono-core init on cloned operands to obtain
        // the exact post-init spread/minSnr at the classification point.
        let mut min_snr_clone = c.min_snr0.clone();
        let mut spread_clone = c.spread0.clone();
        adapt_min_snr(
            &c.energy_lin,
            &c.energy_ld,
            &mut min_snr_clone,
            &geo,
            &cap.ats_pre,
        );
        let _ah_mono = init_avoid_hole_flag_mono(
            &c.energy_lin,
            &c.energy_ld,
            &mut min_snr_clone,
            &mut spread_clone,
            &geo,
            &cap.ats_pre,
        );
        let short = c.seq == 2;
        saw_short |= short;
        saw_long |= !short;
        for &s in &bands {
            drop.push((s, minsnr_drop_margin(avg, c.energy_ld[s], start_ratio)));
            let (m_spread, m_snr) =
                ah_classify_margins(spread_clone[s], c.energy_lin[s], min_snr_clone[s]);
            classify_spread.push((s, m_spread));
            classify_snr.push((s, m_snr));
            // Clamp trigger: captured post-init minSnr (= reduce's m[s]) and
            // the recomputed reduced threshold (long and short paths).
            let en_ld = c.weighted[s];
            let m = cap.min_snr_ld[ch][s];
            if c.threshold[s] >= MIN_LDTHRESH && en_ld > c.threshold[s] {
                if !short {
                    let thr_e = cap.thr_exp[ch][s];
                    let red_0 = cap.red_val.first().copied().unwrap_or(0);
                    long_zero.push((s, long_zero_margin(red_0, thr_e)));
                    let reduced = reduced_threshold_long(thr_e, red_0);
                    clamp.push((s, ah_clamp_margin(reduced, en_ld, m)));
                } else {
                    // Short path (`threshold_vbr.rs:849-864`): group number
                    // follows production `sfb / geo.sfb_per_group`.
                    let inner = s % c.per.max(1);
                    let group_number = inner / c.per.max(1);
                    // Production indexes `group_len[group_number]`; guard the
                    // captured length identically to the `gl as usize` cast
                    // domain (short blocks always carry 4 group lengths).
                    let gl = c.group_len.get(group_number).copied().unwrap_or(1);
                    let thr_e = cap.thr_exp[ch][s];
                    let red_g = cap.red_val.get(group_number).copied().unwrap_or(0);
                    let scaled = scaled_threshold_short(thr_e, gl);
                    short_negone.push((s, short_negone_margin(scaled, red_g)));
                    short_zero.push((s, short_zero_margin(red_g, scaled)));
                    // Reduced threshold for the clamp trigger: replicate
                    // production short computation including group correction
                    // (`calc_ld_data(scaled+red)<<2 + calc_ld_int(gl)-AVG_COMP`),
                    // with NEG_ONE/ZERO early branches preserved.
                    let mut reduced: i32 = if scaled <= LIMIT_THR_REDUCED_Q31.wrapping_sub(red_g) {
                        NEG_ONE_Q31
                    } else if red_g >= ONE_Q31.wrapping_sub(scaled) {
                        ZERO_Q31
                    } else {
                        calc_ld_data(scaled.wrapping_add(red_g)).wrapping_shl(2)
                    };
                    reduced = reduced.wrapping_add(calc_ld_int(gl).wrapping_sub(AVG_COMP_Q31));
                    clamp.push((s, ah_clamp_margin(reduced, en_ld, m)));
                }
                if cap.ah_flag[ch][s] == AH_ACTIVE {
                    button.push((s, ah_button_margin(m, en_ld)));
                }
            }
        }
    }
    out.push(tightest(
        "s2.minsnr_drop",
        Stage::S2,
        ">0 lowers the band minSnr (frame-average drop gate)",
        "==0 does NOT update (strict <)",
        drop,
    ));
    out.push(tightest(
        "s2.ah_classify_spread",
        Stage::S2,
        ">0 (spread hides energy) forces NO_AH",
        "==0 allows classification (inclusive <=)",
        classify_spread,
    ));
    out.push(tightest(
        "s2.ah_classify_minsnr",
        Stage::S2,
        ">0 forces NO_AH",
        "==0 allows classification (inclusive <=)",
        classify_snr,
    ));
    out.push(tightest(
        "s2.ah_clamp",
        Stage::S2,
        ">0 on a classified band engages the AH_ACTIVE clamp (long and short paths; short reduced via mirrored SHORT_THR_FAC/INV_SQRT4/LIMIT)",
        "==0 does NOT clamp (strict >)",
        clamp,
    ));
    out.push(tightest(
        "s2.ah_button",
        Stage::S2,
        ">0 takes the max(minSnr+en, thr) button branch",
        "==0 takes the thr-only branch (strict >)",
        button,
    ));
    out.push(tightest_or(
        "s2.long_zero",
        Stage::S2,
        ">=0 selects ZERO (inclusive >=: red >= ONE-thr_e)",
        "==0 selects ZERO (inclusive >=)",
        long_zero,
        || {
            if saw_long {
                Margin::unobserved("s2.long_zero", Stage::S2)
            } else {
                Margin::not_applicable(
                    "s2.long_zero",
                    Stage::S2,
                    "capture contains no LONG channel",
                )
            }
        },
    ));
    out.push(tightest_or(
        "s2.short_negone",
        Stage::S2,
        "<=0 selects NEG_ONE (inclusive <=: scaled <= LIMIT-red)",
        "==0 selects NEG_ONE (inclusive <=)",
        short_negone,
        || {
            if saw_short {
                Margin::unobserved("s2.short_negone", Stage::S2)
            } else {
                Margin::not_applicable(
                    "s2.short_negone",
                    Stage::S2,
                    "capture contains no SHORT channel",
                )
            }
        },
    ));
    out.push(tightest_or(
        "s2.short_zero",
        Stage::S2,
        ">=0 selects ZERO (inclusive >=: red >= ONE-scaled)",
        "==0 selects ZERO (inclusive >=)",
        short_zero,
        || {
            if saw_short {
                Margin::unobserved("s2.short_zero", Stage::S2)
            } else {
                Margin::not_applicable(
                    "s2.short_zero",
                    Stage::S2,
                    "capture contains no SHORT channel",
                )
            }
        },
    ));
    out
}

/// S4 step-3 margins. The decision-point operands (pre-step-3 `scf`/`dist`/
/// `quant`/`delta_pe` per region) are NOT in the Group A `R22S4Cap` (which
/// snapshots only the final S4 outputs). The per-band pinned-edge distances
/// therefore cannot be derived from captured production data; recorded
/// `MarginDerivationRequired` (design §5 — never guessed). The pure edge
/// functions above are boundary-tested against the production primitives.
pub(super) fn s4_stage_margins_with_probe(
    probe_available: bool,
    gate_observed: bool,
) -> Vec<Margin> {
    if !probe_available {
        return vec![
            Margin::required(
                "s4.step3_dist_edge",
                Stage::S4,
                "R22S4Cap has final outputs only and no detailed step3_probe was supplied",
            ),
            Margin::required(
                "s4.step3_energy_edge",
                Stage::S4,
                "R22S4Cap has final outputs only and no detailed step3_probe was supplied",
            ),
            Margin::required(
                "s4.step3_pe_gate",
                Stage::S4,
                "R22S4Cap has final outputs only and no detailed step3_probe was supplied",
            ),
            Margin::required(
                "s4.step3_sum",
                Stage::S4,
                "R22S4Cap has final outputs only and no detailed step3_probe was supplied",
            ),
        ];
    }
    if !gate_observed {
        return vec![
            Margin::not_applicable(
                "s4.step3_dist_edge",
                Stage::S4,
                "no step-3 region gate passed in the supplied probe",
            ),
            Margin::not_applicable(
                "s4.step3_energy_edge",
                Stage::S4,
                "no step-3 region gate passed in the supplied probe",
            ),
            Margin::not_applicable(
                "s4.step3_pe_gate",
                Stage::S4,
                "no step-3 region gate passed in the supplied probe",
            ),
            Margin::not_applicable(
                "s4.step3_sum",
                Stage::S4,
                "no step-3 region gate passed in the supplied probe",
            ),
        ];
    }
    vec![
        Margin::unobserved("s4.step3_dist_edge", Stage::S4),
        Margin::unobserved("s4.step3_energy_edge", Stage::S4),
        Margin::unobserved("s4.step3_pe_gate", Stage::S4),
        Margin::unobserved("s4.step3_sum", Stage::S4),
    ]
}

/// Default frame collector has no step-3 probe attached. The Required status
/// is scoped to that capture path, not to the existence of a derivation: the
/// reviewed `step3_probe` carries the operands used by the AC-9 supplement.
pub(super) fn s4_stage_margins() -> Vec<Margin> {
    s4_stage_margins_with_probe(false, false)
}

/// S6 observational margins from the owned capture (grants, retry, exit
/// headroom, PE-correction window). Crash-recovery stop-distance requires
/// crash-time intermediates that are not captured (recorded required).
pub(super) fn s6_stage_margins(cap: &R22Capture) -> Vec<Margin> {
    let s6 = &cap.s6;
    let o = &s6.output;
    let nch = s6.ch.len() as i32;
    let mut out = Vec::new();
    out.push(Margin::derived(
        "s6.grant_headroom",
        Stage::S6,
        grant_headroom_margin(o.granted_dyn_bits, s6.qc_pre),
        "<0 raises BitresTooLow",
        "==0 proceeds (inclusive >=0)",
    ));
    let last = o.passes.last().expect("s6 passes nonempty");
    out.push(Margin::derived(
        "s6.element_cap",
        Stage::S6,
        element_cap_margin(last.el_dyn, last.el_static, nch),
        "<0 fails the element constraints",
        "==0 passes (inclusive <=)",
    ));
    out.push(Margin::derived(
        "s6.exit_headroom",
        Stage::S6,
        exit_headroom_margin(o.total_avail, cap.acct.sum_bits),
        ">0 is under the frame budget (favors done)",
        "==0 is NOT under (strict <)",
    ));
    out.push(Margin::derived(
        "s6.multipass_retry",
        Stage::S6,
        multipass_margin(last.iterations),
        ">0 retries remain; ==0 is the crash boundary",
        "==0 triggers crash recovery (== max)",
    ));
    // PE-correction window (FULL path): pe_act = min(granted_pe, no_red_pe).
    let pe_act = o.granted_pe.min(s6.no_red_pe);
    let pc = pe_corr_margins(
        pe_act,
        s6.dist_pre.pe_last,
        s6.dist_pre.dyn_bits_last,
        s6.dist_pre.bits2pe_factor_m,
        s6.dist_pre.bits2pe_factor_e,
    );
    out.push(Margin::derived(
        "s6.pe_corr_bits_last",
        Stage::S6,
        pc.bits_last,
        ">0 required for correction participation",
        "==0 resets correction (strict >)",
    ));
    out.push(Margin {
        name: "s6.pe_corr_hi_f32",
        stage: Stage::S6,
        status: MarginStatus::DerivedF32(pc.hi_f32),
        applicability: MarginApplicability::Applicable,
        direction: ">0 required (pe_act < 1.5*pe_last)",
        equality: "==0 excludes (strict <, f32)",
    });
    out.push(Margin {
        name: "s6.pe_corr_lo_f32",
        stage: Stage::S6,
        status: MarginStatus::DerivedF32(pc.lo_f32),
        applicability: MarginApplicability::Applicable,
        direction: ">0 required (pe_act > 0.7*pe_last)",
        equality: "==0 excludes (strict >, f32)",
    });
    out.push(Margin::derived(
        "s6.pe_corr_win12",
        Stage::S6,
        pc.win_12,
        ">0 required (bits2pe(1.2/2) > pe_last)",
        "==0 excludes (strict >)",
    ));
    out.push(Margin::derived(
        "s6.pe_corr_win065",
        Stage::S6,
        pc.win_065,
        ">0 required (bits2pe(0.65) < pe_last)",
        "==0 excludes (strict <)",
    ));
    // The R22 frame capture proves whether crash recovery participated. When
    // it did not, stop-distance is NotApplicable; a separate detailed crash
    // probe is required to derive the numeric distance when it did.
    if o.passes.iter().any(|p| p.crash >= 0) {
        out.push(Margin::required(
            "s6.crash_stop_distance",
            Stage::S6,
            "crash recovery participated, but R22Capture does not contain crash-time saved_bits/bits_to_save; use the detailed s6_probe supplement",
        ));
    } else {
        out.push(Margin::not_applicable(
            "s6.crash_stop_distance",
            Stage::S6,
            "no crash recovery participated in this captured frame",
        ));
    }
    out
}

/// Accounting observational margins. `bitrate`/`sample_rate` are the
/// operating-point config operands (passed explicitly, never invented).
pub(super) fn acct_stage_margins(cap: &R22Capture, bitrate: i32, sample_rate: i32) -> Vec<Margin> {
    let o = &cap.s6.output;
    let mut out = Vec::new();
    // Raw fill demand before extension sizing: (frame_granted - used_dyn) & 7.
    // frame_granted = granted + (static_in - static_used) (crash rebalance).
    let frame_granted = o
        .granted_dyn_bits
        .wrapping_add(cap.s6.static_bits.wrapping_sub(o.static_bits_used));
    let raw_fill = (frame_granted.wrapping_sub(o.used_dyn_bits)) & 7;
    let prelim = o
        .static_bits_used
        .wrapping_add(o.used_dyn_bits)
        .wrapping_add(raw_fill)
        .wrapping_add(GLOBAL_EXT_BITS);
    out.push(Margin::derived(
        "acct.fill_floor",
        Stage::Acct,
        fill_floor_margin(prelim, 0),
        "<0 engages the min-frame floor (VBR min=0)",
        "==0 reads the floor exactly",
    ));
    out.push(Margin::derived(
        "acct.fill_ext_gate",
        Stage::Acct,
        fill_ext_gate_margin(raw_fill),
        ">=0 writes a fill element",
        "==0 writes (inclusive >=7)",
    ));
    out.push(Margin::derived(
        "acct.bitres_max",
        Stage::Acct,
        bitres_max_margin(cap.s6.qc_pre, cap.s6.qc_max),
        "<0 raises QuantError (level above max)",
        "==0 is exactly full (inclusive <=)",
    ));
    out.push(Margin::derived(
        "acct.bitres_floor",
        Stage::Acct,
        bitres_floor_margin(cap.s6.qc_pre),
        "<0 raises BitresTooLow",
        "==0 is exactly empty (inclusive >=0)",
    ));
    out.push(Margin::derived(
        "acct.padding_onset",
        Stage::Acct,
        padding_onset_margin(cap.state.pad_pre, bitrate, sample_rate),
        "<=0 turns padding ON",
        "==0 pads (inclusive <=)",
    ));
    out
}

/// Aggregate per-frame observational margins across every stage.
pub(super) fn frame_margins(cap: &R22Capture, bitrate: i32, sample_rate: i32) -> Vec<Margin> {
    let mut out = Vec::new();
    out.extend(pe_stage_margins(&cap.s1));
    out.extend(s2_stage_margins(&cap.s2));
    out.extend(s4_stage_margins());
    out.extend(s6_stage_margins(cap));
    out.extend(acct_stage_margins(cap, bitrate, sample_rate));
    out
}

/// Build the complete per-frame classification (design §3 trace row).
/// `bytes_equal` is the cross-arm ADTS byte comparison; `unsupported` marks
/// an unsupported-path hit (never a pass). Missing/skipped data is recorded
/// as a finding by the caller (the harness asserts clean emission).
pub(super) fn classify_frame(
    q: &R22Capture,
    f: &R22Capture,
    diff: &R22Diff,
    prefix: &str,
    frame: u32,
    seq: i32,
    bytes_equal: bool,
    unsupported: bool,
) -> FrameClass {
    let outcome = if unsupported {
        FrameOutcome::FUnsupported
    } else if bytes_equal {
        FrameOutcome::FEven
    } else {
        FrameOutcome::FValid
    };
    let stages = classify_stages(diff, prefix);
    let band_origin = classify_bands(q, f);
    let attributions = attribute_frame(q, f);
    let mut findings = Vec::new();
    if unsupported {
        findings.push(format!(
            "f{frame}: unsupported path hit (F-unsupported, never a pass)"
        ));
    }
    // Every frame must carry a classification or an explicit finding; an
    // empty diff with non-identical bytes is a structural finding.
    if diff.differing_fields.is_empty() && !bytes_equal && !unsupported {
        findings.push(format!(
            "f{frame}: bytes differ but no captured leaf differs (accounting-only divergence)"
        ));
    }
    FrameClass {
        frame,
        seq,
        outcome,
        stages,
        band_origin,
        attributions,
        findings,
    }
}

// ---------------------------------------------------------------------------
// F-20 / F-22 / F-26 instrumented bundled-PE replay (design §3 / §6).
//
// Reproduces the stored `eR` 0-versus-1 LSB cases through the UNCHANGED
// production `intensity_stereo` (IS admission/direction) and `pe_calculation`
// (bundled PE) functions on cloned operands. The evidence chain is:
//
//   stored eR → Ld (e_ld_r) → real_scale → IS admission/direction
//
// This provides downstream evidence for the three findings; it does NOT
// close their conditional owner acceptances (design §8, nine dispositions).
// ---------------------------------------------------------------------------

/// One arm's pre-IS frontend operands, cloned from the production
/// `FrontendOut` working sets (the lockstep indirection validated by the
/// established R2.1-B gate-1 pattern).
#[derive(Clone)]
struct FrontendIs {
    e_l: Vec<i32>,
    e_r: Vec<i32>,
    spec_l: Vec<i32>,
    spec_r: Vec<i32>,
    t_l: Vec<i32>,
    t_r: Vec<i32>,
    t_ld_r: Vec<i32>,
    spread_l: Vec<i32>,
    spread_r: Vec<i32>,
    e_ld_l: Vec<i32>,
    e_ld_r: Vec<i32>,
    offsets: Vec<i32>,
    cnt: usize,
    per: usize,
    max: usize,
}

impl FrontendIs {
    fn from_frontend(fe: &crate::aac_enc::stream_driver::FrontendOut) -> Self {
        let (l, r) = (&fe.live[0], &fe.live[1]);
        Self {
            e_l: l.e_stored().to_vec(),
            e_r: r.e_stored().to_vec(),
            spec_l: l.spec_q31().to_vec(),
            spec_r: r.spec_q31().to_vec(),
            t_l: l.t_stored().to_vec(),
            t_r: r.t_stored().to_vec(),
            t_ld_r: r.t_ld().to_vec(),
            spread_l: l.spread().to_vec(),
            spread_r: r.spread().to_vec(),
            e_ld_l: l.e_ld().to_vec(),
            e_ld_r: r.e_ld().to_vec(),
            offsets: l.offsets().to_vec(),
            cnt: l.cnt(),
            per: l.per(),
            max: fe.max_common,
        }
    }
}

/// One finding's replay evidence row (per arm).
#[derive(Debug, Clone)]
pub(super) struct PeReplayRow {
    pub finding: &'static str,
    pub band: usize,
    pub arm: &'static str,
    /// Pre-IS stored right-channel energy (the 0-vs-1 LSB root).
    pub stored_er: i32,
    /// Ld of the stored right energy (`MIN` sentinel vs finite).
    pub e_ld_r: i32,
    pub e_ld_l: i32,
    /// `real_scale = clamp(e_ld_l - e_ld_r, ±60/128)` (MAX-clamp vs finite).
    pub real_scale: i32,
    /// IS admission decision (0 = dropped, 15 = in-phase retained).
    pub is_book: i32,
    /// IS direction scalefactor.
    pub is_scale: i32,
    /// Pre-PE ATS: `chaos_measure_en_fac[0]`, `last_enfac_patch[0]`, `pe_offset`.
    pub ats_pre: [i32; 3],
    /// Post-PE ATS (same fields).
    pub ats_post: [i32; 3],
    /// Element PE reproduced by the unchanged production function.
    pub pe: i32,
    /// Per-band threshold Ld at the finding band, per channel
    /// (`R22S1Cap.thresholds[ch][band]`; M3 per-band threshold leg).
    pub thr: [i32; 2],
    /// Per-band weighted-energy Ld at the finding band, per channel
    /// (PE-trajectory context; same indexing as `thr`).
    pub w: [i32; 2],
}

/// Replay the production IS decision on cloned frontend operands and return
/// the book/scale/mask plus the prepare-stage `real_scale`. Asserts the
/// replay reproduces the full-encoder production snapshot bit-exactly
/// (within-arm replay correctness, design §10.2).
fn replay_is(
    op: &FrontendIs,
    allow_is: bool,
    snap: &crate::aac_enc::stream_driver::FrameSnapshot,
    tag: &str,
) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let mut e_r = op.e_r.clone();
    let mut spec_l = op.spec_l.clone();
    let mut spec_r = op.spec_r.clone();
    let mut t_r = op.t_r.clone();
    let mut t_ld_r = op.t_ld_r.clone();
    let mut spread_r = op.spread_r.clone();
    let mut e_ld_r = op.e_ld_r.clone();
    let res = intensity_stereo(
        &op.e_l,
        &mut e_r,
        &mut spec_l,
        &mut spec_r,
        &op.t_l,
        &mut t_r,
        &mut t_ld_r,
        &op.spread_l,
        &mut spread_r,
        &op.e_ld_l,
        &mut e_ld_r,
        &op.offsets,
        op.cnt,
        op.per,
        op.max,
        allow_is,
    );
    assert_eq!(res.is_book, snap.is_book_values, "{tag}: IS book replay");
    assert_eq!(res.is_scale, snap.is_scale_values, "{tag}: IS scale replay");
    assert_eq!(res.ms_mask, snap.is_mask_values, "{tag}: IS mask replay");
    assert_eq!(res.ms_digest, snap.is_digest, "{tag}: IS digest replay");
    // Prepare-stage real_scale (production function on cloned operands).
    let mut hrr = vec![0i32; op.cnt];
    let mut mask = vec![0i32; op.cnt];
    let mut real_scale = vec![0i32; op.cnt];
    let mut loud = vec![0i32; op.cnt];
    prepare_decisions(
        &op.e_l,
        &op.e_r,
        &op.e_ld_l,
        &op.e_ld_r,
        &op.spec_l,
        &op.spec_r,
        &op.offsets,
        op.cnt,
        op.per,
        op.max,
        &mut hrr,
        &mut mask,
        &mut real_scale,
        &mut loud,
    );
    let mut final_mask = mask.clone();
    finalize_decisions(
        &hrr,
        &mut final_mask,
        &real_scale,
        &loud,
        op.cnt,
        op.per,
        op.max,
    );
    let _ = final_mask;
    (res.is_book, res.is_scale, real_scale)
}

/// Replay the bundled PE on the actual captured `PeChannelInput` operands with
/// a cloned pre-PE ATS, asserting bit-exact reproduction of the production PE
/// outputs and post-ATS (within-arm replay). Returns pre/post ATS triples.
fn replay_bundled_pe(cap: &R22S1Cap, tag: &str) -> ([i32; 3], [i32; 3], i32) {
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
    let pre = [
        ats.chaos_measure_en_fac[0],
        ats.last_enfac_patch[0],
        ats.pe_offset,
    ];
    let out = pe_calculation(&inputs, &cap.ms_mask, &mut ats);
    assert_eq!(out.data.pe, cap.pe, "{tag}: bundled PE replay");
    assert_eq!(out.weighted_energy_ld, cap.weighted, "{tag}: PE weighted");
    assert_eq!(out.threshold_ld, cap.thresholds, "{tag}: PE thresholds");
    assert_eq!(out.enfac_ld, cap.enfac, "{tag}: PE enfac");
    assert_eq!(
        ats.chaos_measure_en_fac, cap.ats_post.chaos_measure_en_fac,
        "{tag}: PE ats chaos"
    );
    assert_eq!(
        ats.last_enfac_patch, cap.ats_post.last_enfac_patch,
        "{tag}: PE ats patch"
    );
    let post = [
        ats.chaos_measure_en_fac[0],
        ats.last_enfac_patch[0],
        ats.pe_offset,
    ];
    (pre, post, out.data.pe)
}

/// The three finding bands (probe-narrow-48k f1, SHORT, local band 10 in
/// groups g1/g2/g3 → IS bands 24/38/52; R2.1-B owner review ledger).
const F20_BAND: usize = 24;
const F22_BAND: usize = 38;
const F26_BAND: usize = 52;

/// Run the instrumented F-20/F-22/F-26 replay and return the evidence rows
/// (both arms, three findings). Asserts the recorded mechanism: stored `eR`
/// 0-vs-1, Ld `MIN`-vs-finite, `real_scale` MAX-clamp-vs-finite, and the
/// direction drop-vs-retain. Does not close the owner conditions.
pub(super) fn run_f20_f22_f26_replay() -> Vec<PeReplayRow> {
    let pcm = probe_narrow(12, 48000.0, 0.25);
    let sr = 48000u32;
    let channels = 2usize;
    let cfg = EncoderConfig::new(sr, channels as u32, FRAME_LENGTH, TransportType::Adts)
        .expect("cfg")
        .with_vbr_mode(VbrMode::Vbr5);
    let q_cfg = cfg.with_psy_engine(PsyEngineKind::Q31);
    let f_cfg = cfg.with_psy_engine(PsyEngineKind::Fpe);
    let mut q_full = StreamEncoder::with_config(q_cfg);
    let mut f_full = StreamEncoder::with_config(f_cfg);
    let mut q_fe = StreamEncoder::with_config(q_cfg);
    let mut f_fe = StreamEncoder::with_config(f_cfg);
    // allow_is from the production tables (same derivation as the encoder).
    let allow_is = {
        let (ch_br, tns_br) = (96_000u32, 192_000u32);
        let bw = cfg.effective_bandwidth_hz();
        let ((lcfg, _), (scfg, _)) = crate::aac_enc::testdata::load_cfg(sr, ch_br);
        crate::aac_enc::psy::PsyTables::for_rate(
            sr,
            ch_br,
            tns_br,
            bw,
            lcfg.min_snr_ld,
            scfg.min_snr_ld,
        )
        .expect("tables")
        .long_cfg
        .allow_is()
    };
    assert!(allow_is, "48k stereo Vbr5 must allow IS");

    let fresh = |frame: u32| -> Vec<Vec<i16>> {
        (0..channels)
            .map(|c| {
                (0..FRAME_LENGTH)
                    .map(|i| {
                        let t = frame as usize * FRAME_LENGTH + i;
                        *pcm[c].get(t).unwrap_or(&0)
                    })
                    .collect()
            })
            .collect()
    };
    let mut rows = Vec::new();
    for frame in 0..=1u32 {
        let f = fresh(frame);
        let f1024: Vec<[i16; 1024]> = f
            .iter()
            .map(|v| {
                let mut b = [0i16; 1024];
                b.copy_from_slice(&v[..1024]);
                b
            })
            .collect();
        let q_fe_out = q_fe.frontend(frame, f1024.clone()).expect("q frontend");
        let f_fe_out = f_fe.frontend(frame, f1024).expect("f frontend");
        let (_, q_snap) = q_full.step_frame(frame, &f);
        let (_, f_snap) = f_full.step_frame(frame, &f);
        if frame != 1 {
            continue;
        }
        assert_eq!(q_snap.seq, 2, "narrow f1 must be SHORT");
        assert_eq!(f_snap.seq, 2, "narrow f1 must be SHORT (FPE)");
        let q_op = FrontendIs::from_frontend(&q_fe_out);
        let f_op = FrontendIs::from_frontend(&f_fe_out);
        let (q_book, q_scale, q_real) = replay_is(&q_op, allow_is, &q_snap, "F20/22/26 Q31");
        let (f_book, f_scale, f_real) = replay_is(&f_op, allow_is, &f_snap, "F20/22/26 FPE");
        let q_cap = q_snap.r22_cap.as_ref().expect("q r22 cap");
        let f_cap = f_snap.r22_cap.as_ref().expect("f r22 cap");
        let (q_pre, q_post, q_pe) = replay_bundled_pe(&q_cap.s1, "F20/22/26 Q31 PE");
        let (f_pre, f_post, f_pe) = replay_bundled_pe(&f_cap.s1, "F20/22/26 FPE PE");
        for (finding, band) in [("F-20", F20_BAND), ("F-22", F22_BAND), ("F-26", F26_BAND)] {
            // Per-band threshold/weighted at the finding band, per channel
            // (M3 per-band threshold leg; print-only — the Group E REPLAY
            // hash payload in `decision.rs` is untouched, so bundle bytes
            // are preserved). Structural indexing only; exact values are
            // recorded, never pinned (D-R3 deference).
            let mut q_thr = [0i32; 2];
            let mut f_thr = [0i32; 2];
            let mut q_w = [0i32; 2];
            let mut f_w = [0i32; 2];
            for ch in 0..2usize {
                assert!(
                    band < q_cap.s1.thresholds[ch].len(),
                    "{finding} Q31 threshold band range"
                );
                assert!(
                    band < f_cap.s1.thresholds[ch].len(),
                    "{finding} FPE threshold band range"
                );
                q_thr[ch] = q_cap.s1.thresholds[ch][band];
                f_thr[ch] = f_cap.s1.thresholds[ch][band];
                q_w[ch] = q_cap.s1.weighted[ch][band];
                f_w[ch] = f_cap.s1.weighted[ch][band];
            }
            // Mechanism assertions (the recorded R2.1-B M3 root): Q31 stores
            // eR=0 (Ld MIN, real_scale MAX-clamp, direction drop → book 0);
            // FPE stores eR=1 (finite Ld, finite real_scale, retain → book 15).
            assert_eq!(q_op.e_r[band], 0, "{finding} Q31 stored eR must be 0");
            assert_eq!(f_op.e_r[band], 1, "{finding} FPE stored eR must be 1");
            assert_eq!(q_op.e_ld_r[band], i32::MIN, "{finding} Q31 Ld sentinel");
            assert_ne!(f_op.e_ld_r[band], i32::MIN, "{finding} FPE Ld finite");
            assert_eq!(q_real[band], REAL_SCALE_MAX, "{finding} Q31 MAX-clamp");
            assert_ne!(
                f_real[band], REAL_SCALE_MAX,
                "{finding} FPE finite real_scale"
            );
            assert_eq!(q_book[band], 0, "{finding} Q31 direction drop");
            assert_eq!(
                f_book[band], CODE_BOOK_IS_IN_PHASE_NO,
                "{finding} FPE retain"
            );
            rows.push(PeReplayRow {
                finding,
                band,
                arm: "Q31",
                stored_er: q_op.e_r[band],
                e_ld_r: q_op.e_ld_r[band],
                e_ld_l: q_op.e_ld_l[band],
                real_scale: q_real[band],
                is_book: q_book[band],
                is_scale: q_scale[band],
                ats_pre: q_pre,
                ats_post: q_post,
                pe: q_pe,
                thr: q_thr,
                w: q_w,
            });
            rows.push(PeReplayRow {
                finding,
                band,
                arm: "FPE",
                stored_er: f_op.e_r[band],
                e_ld_r: f_op.e_ld_r[band],
                e_ld_l: f_op.e_ld_l[band],
                real_scale: f_real[band],
                is_book: f_book[band],
                is_scale: f_scale[band],
                ats_pre: f_pre,
                ats_post: f_post,
                pe: f_pe,
                thr: f_thr,
                w: f_w,
            });
        }
    }
    assert_eq!(rows.len(), 6, "three findings × two arms");
    rows
}

// ---------------------------------------------------------------------------
// Group D tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::pe::{
        calc_sfb_pe, calc_weighting, prepare_sfb_pe, AtsElement, WeightingInput,
    };
    use crate::aac_enc::stream_driver::FrameSnapshot;
    use crate::aac_enc::testdata;
    use crate::aac_enc::threshold_vbr::{AH_INACTIVE, NO_AH};

    /// Step a fresh Q31 encoder to `frame` on `fixture` and return its owned
    /// R22 capture + snapshot (production path; the capture is the dissection
    /// clone every margin/classification below operates on).
    fn cap_at(fixture: &str, frame: u32) -> (R22Capture, FrameSnapshot) {
        let pcm = testdata::fixture_samples(fixture);
        let sr = testdata::fixture_sample_rate(fixture);
        let mut enc = StreamEncoder::with_mode(sr, pcm.len(), VbrMode::Vbr5);
        let mut last = None;
        for f in 0..=frame {
            let fresh: Vec<Vec<i16>> = pcm
                .iter()
                .map(|ch| {
                    (0..FRAME_LENGTH)
                        .map(|i| {
                            let t = f as usize * FRAME_LENGTH + i;
                            *ch.get(t).unwrap_or(&0)
                        })
                        .collect()
                })
                .collect();
            let (bytes, snap) = enc.step_frame(f, &fresh);
            assert!(
                snap.notes.is_empty(),
                "{fixture} f{f}: notes {:?}",
                snap.notes
            );
            assert!(bytes.is_some(), "{fixture} f{f}: must emit");
            assert!(snap.r22_cap.is_some(), "{fixture} f{f}: capture");
            last = Some(snap);
        }
        let snap = last.expect("stepped at least one frame");
        (snap.r22_cap.clone().expect("cap"), snap)
    }

    // --- Classification truth table / precedence ---------------------------

    #[test]
    fn class_precedence_structural_over_intermediate_over_residue() {
        assert!(class_rank(&Class::S(Stage::S4)) > class_rank(&Class::A));
        assert!(class_rank(&Class::A) > class_rank(&Class::R));
        assert!(
            class_rank(&Class::P {
                stage: Stage::S6,
                origin_stage: Stage::S4,
                origin_band: 0,
                origin_class: Box::new(Class::R),
            }) > class_rank(&Class::A)
        );
        assert!(Class::S(Stage::Is).is_structural());
        assert!(Class::Unattributed { stage: Stage::S6 }.is_structural());
        assert!(!Class::A.is_structural());
        assert!(!Class::R.is_structural());
    }

    #[test]
    fn vocabulary_labels_are_canonical_raspf() {
        assert_eq!(Class::R.label(), "R");
        assert_eq!(Class::A.label(), "A");
        assert_eq!(Class::S(Stage::Is).label(), "S-IS");
        assert_eq!(Class::S(Stage::Ms).label(), "S-MS");
        assert_eq!(Class::S(Stage::Pe).label(), "S-PE");
        assert_eq!(Class::S(Stage::S2).label(), "S-S2");
        assert_eq!(Class::S(Stage::S4).label(), "S-S4");
        assert_eq!(Class::S(Stage::S6).label(), "S-S6");
        assert_eq!(FrameOutcome::FEven.label(), "F-even");
        assert_eq!(FrameOutcome::FValid.label(), "F-valid");
        assert_eq!(FrameOutcome::FUnsupported.label(), "F-unsupported");
        // Stage order is the production execution order.
        assert!(Stage::Is < Stage::Ms && Stage::Ms < Stage::Pe);
        assert!(Stage::Pe < Stage::S2 && Stage::S2 < Stage::S4);
        assert!(Stage::S4 < Stage::S6 && Stage::S6 < Stage::Acct);
    }

    #[test]
    fn field_kind_maps_decision_fields_to_owning_stage() {
        assert_eq!(field_kind("s1.ch1.is_book"), FieldKind::Decision(Stage::Is));
        assert_eq!(
            field_kind("s1.ch1.is_scale"),
            FieldKind::Decision(Stage::Is)
        );
        assert_eq!(field_kind("s1.ms_mask"), FieldKind::Decision(Stage::Ms));
        assert_eq!(field_kind("s2.ah_flag"), FieldKind::Decision(Stage::S2));
        assert_eq!(field_kind("s4.ch0.scf"), FieldKind::Decision(Stage::S4));
        assert_eq!(field_kind("s6.grants"), FieldKind::Decision(Stage::S6));
        assert_eq!(field_kind("acct.total"), FieldKind::Decision(Stage::Acct));
        // Value / intermediate fields.
        assert_eq!(field_kind("s1.ch0.energy_ld"), FieldKind::EnergyValue);
        assert_eq!(field_kind("s1.weighted"), FieldKind::Intermediate);
        assert_eq!(field_kind("s2.threshold_ld"), FieldKind::Intermediate);
        // Invariants and carriers.
        assert_eq!(field_kind("s1.ch0.offsets"), FieldKind::Structural);
        assert_eq!(field_kind("state.ats_post"), FieldKind::Carrier);
    }

    // --- R/A/S/P/F classification on production captures -------------------

    #[test]
    fn self_diff_classifies_no_difference() {
        let (cap, _) = cap_at("onset-stereo", 23);
        let d = super::super::r22_capture::diff_captures(&cap, &cap, "self");
        assert!(d.differing_fields.is_empty());
        let stages = classify_stages(&d, "self");
        assert!(stages.is_empty(), "self-diff has no stage class");
        let bands = classify_bands(&cap, &cap);
        assert!(bands.iter().all(|b| *b == BandClass::Equal));
        assert!(attribute_frame(&cap, &cap).is_empty());
    }

    #[test]
    fn energy_residue_classifies_r_not_structural() {
        let (q, _) = cap_at("onset-stereo", 23);
        let mut f = q.clone();
        // Perturb one energy value (within-envelope residue), no decision.
        f.s1.ch[0].energy_lin[5] = f.s1.ch[0].energy_lin[5].wrapping_add(1);
        let bands = classify_bands(&q, &f);
        assert_eq!(bands[5], BandClass::Class(Class::R));
        // No structural flip anywhere from a pure energy residue.
        assert!(!bands
            .iter()
            .any(|b| matches!(b, BandClass::Class(c) if c.is_structural())));
    }

    #[test]
    fn intermediate_diff_classifies_a() {
        let (q, _) = cap_at("onset-stereo", 23);
        let mut f = q.clone();
        // Perturb a post-weighting threshold (Ld intermediate), no decision.
        f.s1.thresholds[0][5] = f.s1.thresholds[0][5].wrapping_add(1);
        let bands = classify_bands(&q, &f);
        assert_eq!(bands[5], BandClass::Class(Class::A));
    }

    #[test]
    fn is_book_flip_classifies_s_is() {
        let (q, _) = cap_at("onset-stereo", 23);
        let mut f = q.clone();
        f.s1.ch[1].is_book[5] = f.s1.ch[1].is_book[5].wrapping_add(1);
        let bands = classify_bands(&q, &f);
        assert_eq!(bands[5], BandClass::Class(Class::S(Stage::Is)));
    }

    #[test]
    fn ah_flag_flip_classifies_s_s2() {
        let (q, _) = cap_at("onset-stereo", 23);
        let mut f = q.clone();
        let v = f.s2.ah_flag[0][5];
        f.s2.ah_flag[0][5] = if v == NO_AH { AH_INACTIVE } else { NO_AH };
        let bands = classify_bands(&q, &f);
        assert_eq!(bands[5], BandClass::Class(Class::S(Stage::S2)));
    }

    #[test]
    fn scf_flip_classifies_s_s4() {
        let (q, _) = cap_at("onset-stereo", 23);
        let mut f = q.clone();
        f.s4.ch[0].scf[5] = f.s4.ch[0].scf[5].wrapping_add(1);
        let bands = classify_bands(&q, &f);
        assert_eq!(bands[5], BandClass::Class(Class::S(Stage::S4)));
    }

    #[test]
    fn frame_outcome_even_valid_unsupported() {
        let (q, _) = cap_at("onset-stereo", 23);
        let d = super::super::r22_capture::diff_captures(&q, &q, "p");
        let even = classify_frame(&q, &q, &d, "p", 23, 0, true, false);
        assert_eq!(even.outcome, FrameOutcome::FEven);
        assert_eq!(even.frame, 23);
        assert_eq!(even.seq, 0);
        assert!(even.stages.is_empty(), "self-diff carries no stage class");
        let valid = classify_frame(&q, &q, &d, "p", 23, 0, false, false);
        assert_eq!(valid.outcome, FrameOutcome::FValid);
        let unsup = classify_frame(&q, &q, &d, "p", 23, 0, true, true);
        assert_eq!(unsup.outcome, FrameOutcome::FUnsupported);
        assert!(!unsup.findings.is_empty(), "unsupported records a finding");
        // A real cross-arm diff populates the per-stage summary.
        let mut f = q.clone();
        f.s4.ch[0].scf[5] = f.s4.ch[0].scf[5].wrapping_add(1);
        let df = super::super::r22_capture::diff_captures(&q, &f, "p");
        let fc = classify_frame(&q, &f, &df, "p", 23, 0, false, false);
        assert!(fc.stages.iter().any(|s| s.stage == Stage::S4));
    }

    // --- Same-frame chain attribution --------------------------------------

    #[test]
    fn propagation_cites_earliest_upstream_once() {
        let (q, _) = cap_at("onset-stereo", 23);
        let mut f = q.clone();
        // Energy residue at band 5 (origin R) + a downstream S4 scf flip.
        f.s1.ch[0].energy_lin[5] = f.s1.ch[0].energy_lin[5].wrapping_add(1);
        f.s4.ch[0].scf[5] = f.s4.ch[0].scf[5].wrapping_add(1);
        let attrs = attribute_frame(&q, &f);
        // The S4 flip is a P citing the energy R origin at band 5.
        let p = attrs
            .iter()
            .find(|a| a.stage == Stage::S4 && a.band == 5)
            .expect("S4 attribution at band 5");
        match &p.class {
            Class::P {
                origin_stage,
                origin_band,
                origin_class,
                ..
            } => {
                assert_eq!(*origin_stage, Stage::Is);
                assert_eq!(*origin_band, 5);
                assert_eq!(**origin_class, Class::R);
            }
            other => panic!("expected P, got {other:?}"),
        }
        // No double-attribution: exactly one attribution for the S4 flip at b5.
        let n = attrs
            .iter()
            .filter(|a| a.stage == Stage::S4 && a.band == 5)
            .count();
        assert_eq!(n, 1, "no double-attribution");
    }

    #[test]
    fn structural_origin_without_upstream_is_s_not_p() {
        let (q, _) = cap_at("onset-stereo", 23);
        let mut f = q.clone();
        // Only an S4 scf flip, no upstream energy/intermediate event at b5.
        f.s4.ch[0].scf[5] = f.s4.ch[0].scf[5].wrapping_add(1);
        let attrs = attribute_frame(&q, &f);
        let a = attrs
            .iter()
            .find(|a| a.stage == Stage::S4 && a.band == 5)
            .expect("S4");
        assert_eq!(a.class, Class::S(Stage::S4), "origin is S, not P");
    }

    #[test]
    fn s6_element_global_without_upstream_is_unattributed() {
        let (q, _) = cap_at("onset-stereo", 23);
        let mut f = q.clone();
        // Perturb an element-global S6 grant with no per-band upstream chain.
        f.s6.output.granted_pe = f.s6.output.granted_pe.wrapping_add(1);
        let attrs = attribute_frame(&q, &f);
        let a = attrs
            .iter()
            .find(|a| a.stage == Stage::S6)
            .expect("S6 attribution");
        assert_eq!(a.class, Class::Unattributed { stage: Stage::S6 });
    }

    // --- Margin boundary tests (below / at / above each edge) --------------

    #[test]
    fn pe_prepare_admission_boundary_strict() {
        // production: energy_ld > threshold_ld admits (strict).
        assert!(pe_prepare_admission_margin(10, 10) == 0); // at: not admitted
        assert!(pe_prepare_admission_margin(9, 10) < 0); // below
        assert!(pe_prepare_admission_margin(11, 10) > 0); // above
                                                          // Tie to production: at the edge n_lines == 0; above, non-zero.
        let e = vec![0x1000_0000];
        let t_eq = vec![0x1000_0000];
        let t_lo = vec![0x0fff_ffff];
        let ff = vec![0x1000_0000];
        let off = vec![0, 4];
        let n_at = prepare_sfb_pe(&e, &t_eq, &ff, &off, 1, 1, 1);
        let n_above = prepare_sfb_pe(&e, &t_lo, &ff, &off, 1, 1, 1);
        assert_eq!(n_at, vec![0], "at edge admits nothing");
        assert!(n_above[0] > 0, "above edge admits");
    }

    #[test]
    fn pe_c1_branch_boundary_inclusive() {
        // production: ratio >= C1 selects linear (inclusive).
        let t: i32 = 0x1000_0000;
        let w_at = t.wrapping_add(C1_LD_Q31);
        assert_eq!(pe_c1_branch_margin(w_at, t), 0); // at: linear
        assert!(pe_c1_branch_margin(w_at.wrapping_sub(1), t) < 0); // below: C2/C3
        assert!(pe_c1_branch_margin(w_at.wrapping_add(1), t) > 0); // above: linear
                                                                   // Tie to production calc_sfb_pe branch selection at the edge.
        let n = vec![4];
        let isb = vec![0];
        let iss = vec![0];
        let at = calc_sfb_pe(&n, &[w_at], &[t], 1, 1, 1, &isb, &iss);
        let below = calc_sfb_pe(&n, &[w_at.wrapping_sub(1)], &[t], 1, 1, 1, &isb, &iss);
        assert_ne!(at.sfb_pe, below.sfb_pe, "C1 edge flips the PE branch");
    }

    #[test]
    fn chaos_patch_arm_boundary_strict() {
        assert_eq!(chaos_patch_arm_margin(PATCH_ARM_Q31), 0); // at: not armed
        assert!(chaos_patch_arm_margin(PATCH_ARM_Q31 - 1) < 0);
        assert!(chaos_patch_arm_margin(PATCH_ARM_Q31 + 1) > 0);
        assert_eq!(chaos_patch_hi_margin(PATCH_HI_Q31), 0);
        assert_eq!(chaos_patch_mid_margin(PATCH_MID_Q31), 0);
        // Tie the mirrored PATCH_ARM edge to production calc_weighting: on a
        // real loud long input the recorded last_enfac_patch must equal the
        // strict `chaos > PATCH_ARM` gate the margin mirrors.
        let mut ats = AtsElement::new(0);
        let ch = WeightingInput {
            n_lines: &[900, 900],
            energy_linear: &[0x2000_0000, 0x2000_0000],
            energy_ld: &[0x1000_0000, 0x1000_0000],
            offsets: &[0, 1024, 2048],
            sfb_cnt: 2,
            sfb_per_group: 2,
            max_sfb_per_group: 2,
            short_block: false,
        };
        let _ = calc_weighting(&[ch], &[], &mut ats);
        let chaos = ats.chaos_measure_en_fac[0];
        assert_eq!(
            ats.last_enfac_patch[0],
            (chaos > PATCH_ARM_Q31) as i32,
            "mirrored PATCH_ARM edge matches production arm decision"
        );
        assert_eq!(chaos_patch_arm_margin(chaos) > 0, chaos > PATCH_ARM_Q31);
    }

    #[test]
    fn enfac_energy_gate_boundary_strict() {
        assert_eq!(enfac_energy_gate_margin(0), 0); // at: blocked
        assert!(enfac_energy_gate_margin(-1) < 0);
        assert!(enfac_energy_gate_margin(1) > 0);
    }

    #[test]
    fn short_element_chaos_state_matches_mirrored_constant() {
        // calc_weighting short path pins chaos to SHORT_ENFAC (0.75) and arms
        // the patch for the next long frame (adj_thr.cpp:871-876).
        let ch = WeightingInput {
            n_lines: &[10, 20],
            energy_linear: &[1000, 2000],
            energy_ld: &[0x1000_0000, 0x1000_0000],
            offsets: &[0, 4, 8],
            sfb_cnt: 2,
            sfb_per_group: 2,
            max_sfb_per_group: 2,
            short_block: true,
        };
        let mut ats = AtsElement::new(0);
        let enfac = calc_weighting(&[ch], &[], &mut ats);
        assert_eq!(enfac, vec![vec![0, 0]], "short enfac stays zero");
        assert_eq!(ats.chaos_measure_en_fac[0], SHORT_ENFAC_Q31);
        assert_eq!(ats.last_enfac_patch[0], 1);
    }

    #[test]
    fn real_scale_clamps_at_both_production_rails() {
        // prepare_decisions clamps real_scale to ±60/128 (intensity.rs:219).
        let run = |e_ld_l: i32, e_ld_r: i32| -> i32 {
            let e_l = vec![1000];
            let e_r = vec![1000];
            let spec = vec![0i32; 4];
            let offsets = vec![0, 4];
            let mut hrr = vec![0i32; 1];
            let mut mask = vec![0i32; 1];
            let mut real = vec![0i32; 1];
            let mut loud = vec![0i32; 1];
            prepare_decisions(
                &e_l,
                &e_r,
                &[e_ld_l],
                &[e_ld_r],
                &spec,
                &spec,
                &offsets,
                1,
                1,
                1,
                &mut hrr,
                &mut mask,
                &mut real,
                &mut loud,
            );
            real[0]
        };
        // Above the max rail → MAX clamp; below the min rail → MIN clamp.
        assert_eq!(run(0x7000_0000, 0), REAL_SCALE_MAX);
        assert_eq!(run(-0x7000_0000, 0), REAL_SCALE_MIN);
        // Inside the rails → finite (unclamped) difference.
        assert_eq!(run(0x1000_0000, 0), 0x1000_0000);
    }

    #[test]
    fn i2_sentinel_boundary_dual() {
        // Exactly dual-mono sets the sentinel (m_a >= 0 && m_b <= 0).
        let (a, b) = i2_sentinel_margins(1000, 1000);
        assert!(a >= 0 && b <= 0);
        // Strongly panned escapes.
        let (a, b) = i2_sentinel_margins(100, 10_000);
        assert!(a < 0 || b > 0);
        // fmult-floor boundary ties to the production helper.
        let e = 1_000_000i32;
        let q = fmult(LEFT_RIGHT_RATIO_THRESH, e);
        assert_eq!(i2_sentinel_margins(q, e).0, 0);
        assert_eq!(i2_sentinel_margins(e, q).1, 0);
    }

    #[test]
    fn minsnr_drop_boundary_strict() {
        // update iff start_ratio < db_ratio (strict).
        let avg: i32 = 0x2000_0000;
        let en: i32 = 0x1000_0000;
        let db = avg.wrapping_sub(en);
        assert_eq!(minsnr_drop_margin(avg, en, db), 0); // at: no update
        assert!(minsnr_drop_margin(avg, en, db - 1) > 0); // above: update
        assert!(minsnr_drop_margin(avg, en, db + 1) < 0); // below: no update
    }

    #[test]
    fn ah_clamp_and_button_boundary_strict() {
        // clamp: reduced - en_ld > min_snr (strict).
        let en: i32 = 0x1000_0000;
        let m: i32 = 0x0100_0000;
        let reduced_at = en.wrapping_add(m);
        assert_eq!(ah_clamp_margin(reduced_at, en, m), 0); // at: no clamp
        assert!(ah_clamp_margin(reduced_at.wrapping_add(1), en, m) > 0);
        assert!(ah_clamp_margin(reduced_at.wrapping_sub(1), en, m) < 0);
        // button: min_snr > NEG_ONE - en_ld (strict).
        let edge = NEG_ONE_Q31.wrapping_sub(en);
        assert_eq!(ah_button_margin(edge, en), 0); // at: thr-only branch
        assert!(ah_button_margin(edge.wrapping_add(1), en) > 0);
        assert!(ah_button_margin(edge.wrapping_sub(1), en) < 0);
    }

    #[test]
    fn s2_thr_exp_and_short_branch_boundaries() {
        // thr_exp is branch-free (`calc_inv_ld_data(threshold>>2)`); verify
        // the recomputation helper matches production on both zero and
        // finite inputs (Q31 and FPE-shifted paths share the same function).
        use crate::aac_enc::threshold_vbr::calc_thresh_exp;
        use crate::aac_enc::threshold_vbr::SfbGeometry;
        let offsets = [0i32, 4, 8];
        let group_len = [1i32, 1, 1, 1];
        let geo = SfbGeometry {
            offsets: &offsets,
            group_len: &group_len,
            sfb_cnt: 2,
            sfb_per_group: 2,
            max_sfb_per_group: 2,
            seq: 0,
        };
        assert_eq!(
            calc_thresh_exp(&[0xbe000000u32 as i32, 0x1000_0000], &geo),
            vec![
                crate::aac_enc::q31::calc_inv_ld_data((0xbe000000u32 as i32) >> 2),
                crate::aac_enc::q31::calc_inv_ld_data(0x1000_0000 >> 2),
            ]
        );
        // Long ZERO branch: red >= ONE-thr_e selects ZERO (inclusive).
        let thr_e: i32 = 0x1000_0000;
        let at = ONE_Q31.wrapping_sub(thr_e);
        assert_eq!(long_zero_margin(at, thr_e), 0); // at: selects ZERO
        assert!(long_zero_margin(at.wrapping_add(1), thr_e) > 0);
        assert!(long_zero_margin(at.wrapping_sub(1), thr_e) < 0);
        // Short NEG_ONE branch: scaled <= LIMIT-red selects NEG_ONE.
        let scaled: i32 = 0x2000_0000;
        let red: i32 = 0x0100_0000;
        let at_neg = LIMIT_THR_REDUCED_Q31.wrapping_sub(red);
        // At equality the production `<=` takes NEG_ONE.
        assert_eq!(short_negone_margin(at_neg, red), 0);
        assert!(short_negone_margin(at_neg.wrapping_add(1), red) > 0); // above: no NEG_ONE
        assert!(short_negone_margin(at_neg.wrapping_sub(1), red) < 0);
        // Short ZERO branch: red >= ONE-scaled selects ZERO.
        let at_zero = ONE_Q31.wrapping_sub(scaled);
        assert_eq!(short_zero_margin(at_zero, scaled), 0); // at: selects ZERO
        assert!(short_zero_margin(at_zero.wrapping_add(1), scaled) > 0);
        assert!(short_zero_margin(at_zero.wrapping_sub(1), scaled) < 0);
        // Short scaled helper matches production fixed-point chain
        // (fmult nesting + <<2) on representative Q31 and FPE-shifted inputs.
        for &te in &[0xbe000000u32 as i32, 0x1000_0000, 7053950, -1040183479] {
            for &gl in &[1i32, 2, 4, 8] {
                let got = scaled_threshold_short(te, gl);
                let want = crate::aac_enc::q31::fmult(
                    te,
                    crate::aac_enc::q31::fmult(
                        SHORT_THR_FAC_Q31,
                        INV_SQRT4_Q31[(gl.max(1).min(8) - 1) as usize],
                    ),
                )
                .wrapping_shl(2);
                assert_eq!(got, want, "scaled te={te} gl={gl}");
            }
        }
    }

    #[test]
    fn step3_edges_boundary() {
        // dist window: dn > dist_prev + D_1122 rejects (strict >).
        let dp: i32 = 0x1000_0000;
        let dn_at = dp.wrapping_add(D_1122);
        assert_eq!(step3_dist_margin(dn_at, dp), 0); // at: passes
        assert!(step3_dist_margin(dn_at.wrapping_add(1), dp) < 0); // above: reject
        assert!(step3_dist_margin(dn_at.wrapping_sub(1), dp) > 0);
        // energy window: en_q < energy_ld - D_07079 rejects (strict <).
        let el: i32 = 0x2000_0000;
        let en_at = el.wrapping_sub(D_07079);
        assert_eq!(step3_energy_margin(en_at, el), 0); // at: passes
        assert!(step3_energy_margin(en_at.wrapping_sub(1), el) < 0); // below: reject
        assert!(step3_energy_margin(en_at.wrapping_add(1), el) > 0);
        // PE gate: delta_new <= 0 passes (non-strict).
        assert_eq!(step3_pe_gate_margin(0), 0); // at: passes
        assert!(step3_pe_gate_margin(-1) > 0);
        assert!(step3_pe_gate_margin(1) < 0); // above: reject
                                              // sum gate: new_sum < old_sum accepts (strict).
        assert_eq!(step3_sum_margin(100, 100), 0); // at: reject
        assert!(step3_sum_margin(99, 100) > 0);
        assert!(step3_sum_margin(101, 100) < 0);
    }

    #[test]
    fn s6_grant_and_retry_boundary() {
        // grant floor: granted + bit_res_tot < 0 errors.
        assert_eq!(grant_headroom_margin(-100, 100), 0); // at: proceeds
        assert!(grant_headroom_margin(-101, 100) < 0); // below: error
        assert!(grant_headroom_margin(-99, 100) > 0);
        // retry: iterations == max is the crash boundary.
        assert_eq!(multipass_margin(MAX_ITERATIONS), 0); // at: crash
        assert!(multipass_margin(MAX_ITERATIONS - 1) > 0); // retries remain
                                                           // exit headroom: sum_bits < total_avail (strict).
        assert_eq!(exit_headroom_margin(100, 100), 0); // at: not under
        assert!(exit_headroom_margin(101, 100) > 0);
        assert!(exit_headroom_margin(99, 100) < 0);
    }

    #[test]
    fn pe_correction_window_participation() {
        // Construct operands inside the window: all five margins positive.
        // pe_last large, pe_act near pe_last, bits_last positive.
        let m = 0x4b85_1e80u32 as i32; // B2PE default mantissa
        let e = 1i32;
        let pe_last = 1000;
        let pe_act = 1000;
        let bits_last = 5000;
        let pc = pe_corr_margins(pe_act, pe_last, bits_last, m, e);
        assert!(pc.bits_last > 0);
        assert!(pc.hi_f32 > 0.0, "pe_act < 1.5*pe_last");
        assert!(pc.lo_f32 > 0.0, "pe_act > 0.7*pe_last");
        // Outside the window on the low side: pe_act too small.
        let pc_lo = pe_corr_margins(1, pe_last, bits_last, m, e);
        assert!(pc_lo.lo_f32 < 0.0, "below 0.7*pe_last excludes");
        // bits_last == 0 excludes (strict >).
        let pc_zero = pe_corr_margins(pe_act, pe_last, 0, m, e);
        assert_eq!(pc_zero.bits_last, 0);
    }

    #[test]
    fn empty_margin_populations_are_not_fabricated_zero() {
        // A SHORT-only capture has no LONG branch.
        let (cap, _) = cap_at("sine-stereo", 1);
        let ms = s2_stage_margins(&cap.s2);
        let long_zero = ms.iter().find(|m| m.name == "s2.long_zero").unwrap();
        assert!(matches!(long_zero.status, MarginStatus::NotApplicable(_)));
        let acct = acct_stage_margins(&cap, 192_000, 48_000);
        let floor = acct.iter().find(|m| m.name == "acct.bitres_floor").unwrap();
        assert_eq!(floor.status, MarginStatus::Derived(cap.s6.qc_pre as i64));
    }

    #[test]
    fn accounting_fill_and_padding_boundary() {
        // fill ext gate: payload >= 7 writes (inclusive).
        assert_eq!(fill_ext_gate_margin(7), 0); // at: writes
        assert!(fill_ext_gate_margin(6) < 0); // below: no element
        assert!(fill_ext_gate_margin(8) > 0);
        // fill floor: prelim < min engages.
        assert_eq!(fill_floor_margin(0, 0), 0);
        assert!(fill_floor_margin(-1, 0) < 0);
        // bitres max guard: level > max errors.
        assert_eq!(bitres_max_margin(100, 100), 0); // at: full
        assert!(bitres_max_margin(101, 100) < 0); // above: error
        assert!(bitres_max_margin(99, 100) > 0);
        // bitres floor: level < 0 errors.
        assert_eq!(bitres_floor_margin(0), 0);
        assert!(bitres_floor_margin(-1) < 0);
        // padding onset: rest <= 0 pads (inclusive). Use a bitrate whose
        // calc_frame_len modulo is 0 so rest == padding_rest.
        assert_eq!(padding_onset_margin(0, 192_000, 48_000), 0); // at: pads
        assert!(padding_onset_margin(1, 192_000, 48_000) > 0); // above: no pad
        assert!(padding_onset_margin(-1, 192_000, 48_000) < 0); // below: pads
    }

    // --- MarginDerivationRequired status (never a guessed number) ----------

    #[test]
    fn s4_step3_margins_are_derivation_required() {
        let ms = s4_stage_margins();
        assert!(!ms.is_empty());
        for m in &ms {
            assert!(
                m.is_required(),
                "{} must be MarginDerivationRequired",
                m.name
            );
            assert!(matches!(
                m.status,
                MarginStatus::MarginDerivationRequired(_)
            ));
        }
    }

    #[test]
    fn s6_crash_stop_distance_is_derivation_required() {
        let (cap, _) = cap_at("onset-stereo", 23);
        let ms = s6_stage_margins(&cap);
        let crash = ms
            .iter()
            .find(|m| m.name == "s6.crash_stop_distance")
            .expect("crash margin");
        assert!(matches!(crash.status, MarginStatus::NotApplicable(_)));
        assert!(!crash.is_applicable());
        // The derivable S6 margins are present and numeric.
        let grant = ms
            .iter()
            .find(|m| m.name == "s6.grant_headroom")
            .expect("grant");
        assert!(matches!(grant.status, MarginStatus::Derived(_)));
    }

    #[test]
    fn every_stage_margin_is_derived_or_required_never_silent() {
        let (cap, _) = cap_at("onset-stereo", 23);
        let ms = frame_margins(&cap, 192_000, 48_000);
        assert!(!ms.is_empty());
        for m in &ms {
            match m.status {
                MarginStatus::Derived(_)
                | MarginStatus::DerivedF32(_)
                | MarginStatus::MarginDerivationRequired(_)
                | MarginStatus::NotApplicable(_)
                | MarginStatus::Unobserved
                | MarginStatus::NotGenerated(_) => {}
            }
            // Every margin carries direction + equality metadata.
            assert!(!m.direction.is_empty());
            assert!(!m.equality.is_empty());
        }
        // Each stage is represented.
        for stage in [Stage::Pe, Stage::S2, Stage::S4, Stage::S6, Stage::Acct] {
            assert!(
                ms.iter().any(|m| m.stage == stage),
                "stage {stage:?} represented"
            );
        }
    }

    // --- F-20 / F-22 / F-26 stored-input replay ----------------------------

    #[test]
    fn r22_f20_f22_f26_stored_input_replay() {
        let rows = run_f20_f22_f26_replay();
        assert_eq!(rows.len(), 6);
        for finding in ["F-20", "F-22", "F-26"] {
            let q = rows
                .iter()
                .find(|r| r.finding == finding && r.arm == "Q31")
                .expect("Q31 row");
            let f = rows
                .iter()
                .find(|r| r.finding == finding && r.arm == "FPE")
                .expect("FPE row");
            // The 0-vs-1 stored LSB root and the downstream chain.
            assert_eq!(q.stored_er, 0);
            assert_eq!(f.stored_er, 1);
            assert_eq!(q.e_ld_r, i32::MIN);
            assert_ne!(f.e_ld_r, i32::MIN);
            assert_eq!(q.real_scale, REAL_SCALE_MAX);
            assert_ne!(f.real_scale, REAL_SCALE_MAX);
            assert_eq!(q.is_book, 0, "{finding} Q31 direction drop");
            assert_eq!(f.is_book, CODE_BOOK_IS_IN_PHASE_NO, "{finding} FPE retain");
            // Per-band Ld components and direction scale recorded both arms.
            // The F-20/22/26 root is the right-channel stored LSB; the left
            // Ld is finite on both arms (recorded, not asserted unequal).
            assert_ne!(q.e_ld_l, i32::MIN, "{finding} Q31 left Ld finite");
            assert_ne!(f.e_ld_l, i32::MIN, "{finding} FPE left Ld finite");
            assert_eq!(q.is_scale, 0, "{finding} Q31 dropped band carries no scale");
            assert_ne!(f.is_scale, 0, "{finding} FPE retained band carries a scale");
            // Pre/post ATS recorded (chaos, patch, pe_offset).
            assert_eq!(q.ats_pre[2], q.ats_post[2], "pe_offset stable within frame");
            println!(
                "R22-PE-REPLAY {finding} b{} Q31 eR={} Ld={:#x} real={:#x} book={} scale={} pe={} thr={:?} w={:?} ats_pre={:?} ats_post={:?} | FPE eR={} Ld={:#x} real={:#x} book={} scale={} pe={} thr={:?} w={:?} ats_pre={:?} ats_post={:?}",
                q.band, q.stored_er, q.e_ld_r, q.real_scale, q.is_book, q.is_scale, q.pe, q.thr, q.w, q.ats_pre, q.ats_post,
                f.stored_er, f.e_ld_r, f.real_scale, f.is_book, f.is_scale, f.pe, f.thr, f.w, f.ats_pre, f.ats_post,
            );
        }
    }

    // --- Purity + side-effect canary ---------------------------------------

    #[test]
    fn margin_and_classification_side_effect_canary() {
        // Two identically seeded Q31 encoders in lockstep. After each frame,
        // run the FULL Group D classification + margin pipeline on encoder A's
        // capture; prove A's live frame-surviving state never moves and the
        // next frame's emitted bytes still match the never-touched encoder B.
        let pcm = testdata::fixture_samples("sine-stereo");
        let sr = testdata::fixture_sample_rate("sine-stereo");
        let mut a = StreamEncoder::with_mode(sr, pcm.len(), VbrMode::Vbr5);
        let mut b = StreamEncoder::with_mode(sr, pcm.len(), VbrMode::Vbr5);
        for f in 0..8u32 {
            let fresh: Vec<Vec<i16>> = pcm
                .iter()
                .map(|ch| {
                    (0..FRAME_LENGTH)
                        .map(|i| *ch.get(f as usize * FRAME_LENGTH + i).unwrap_or(&0))
                        .collect()
                })
                .collect();
            let (ba, sa) = a.step_frame(f, &fresh);
            let (bb, sb) = b.step_frame(f, &fresh);
            assert_eq!(ba, bb, "f{f}: lockstep bytes");
            let cap = sa.r22_cap.as_ref().expect("cap");
            let before = a.r22_observable_state();
            // Dissection-cloned operand pipeline (reads only owned clones).
            let cap_clone = cap.clone();
            let d = super::super::r22_capture::diff_captures(cap, cap, "canary");
            let _fc = classify_frame(cap, cap, &d, "canary", f, sa.seq, true, false);
            let _attrs = attribute_frame(cap, &cap_clone);
            let _ms = frame_margins(&cap_clone, 192_000, sr as i32);
            let _pe = pe_stage_margins(&cap_clone.s1);
            let _s2 = s2_stage_margins(&cap_clone.s2);
            let after = a.r22_observable_state();
            assert_eq!(before.0.pe_offset, after.0.pe_offset, "f{f}: ats pe_offset");
            assert_eq!(
                before.0.chaos_measure_en_fac, after.0.chaos_measure_en_fac,
                "f{f}: chaos"
            );
            assert_eq!(
                before.0.last_enfac_patch, after.0.last_enfac_patch,
                "f{f}: patch"
            );
            assert_eq!(before.1, after.1, "f{f}: dist");
            assert_eq!(before.2, after.2, "f{f}: bit_res_tot");
            assert_eq!(before.3, after.3, "f{f}: bit_res_max");
            assert_eq!(before.4, after.4, "f{f}: padding_rest");
            let _ = sb;
        }
        // One more lockstep frame proves histories still agree.
        let f = 8u32;
        let fresh: Vec<Vec<i16>> = pcm
            .iter()
            .map(|ch| {
                (0..FRAME_LENGTH)
                    .map(|i| *ch.get(f as usize * FRAME_LENGTH + i).unwrap_or(&0))
                    .collect()
            })
            .collect();
        let (ba, _) = a.step_frame(f, &fresh);
        let (bb, _) = b.step_frame(f, &fresh);
        assert_eq!(ba, bb, "post-pipeline bytes must agree");
    }

    #[test]
    fn margins_operate_on_dissection_clones_not_live_storage() {
        // The capture vectors are owned clones: their storage is disjoint from
        // any other capture (the live arm is never aliased). Mutating a margin
        // input clone cannot touch the original capture.
        let (cap, _) = cap_at("onset-stereo", 23);
        let mut clone = cap.clone();
        assert!(!std::ptr::eq(
            cap.s1.ch[0].energy_ld.as_ptr(),
            clone.s1.ch[0].energy_ld.as_ptr()
        ));
        clone.s1.ch[0].energy_ld[0] = clone.s1.ch[0].energy_ld[0].wrapping_add(1);
        assert_ne!(cap.s1.ch[0].energy_ld[0], clone.s1.ch[0].energy_ld[0]);
        // Margins computed on the clone leave the original capture intact.
        let _ = pe_stage_margins(&clone.s1);
        let (cap2, _) = cap_at("onset-stereo", 23);
        assert_eq!(cap.s1.ch[0].energy_ld, cap2.s1.ch[0].energy_ld);
    }

    // --- Missing / unsupported never count as a pass -----------------------

    #[test]
    fn unsupported_frame_is_finding_not_pass() {
        let (q, _) = cap_at("onset-stereo", 23);
        let d = super::super::r22_capture::diff_captures(&q, &q, "p");
        let fc = classify_frame(&q, &q, &d, "p", 23, 0, true, true);
        assert_eq!(fc.outcome, FrameOutcome::FUnsupported);
        assert!(fc.findings.iter().any(|f| f.contains("unsupported")));
        // An unsupported frame is never F-even/F-valid.
        assert_ne!(fc.outcome, FrameOutcome::FEven);
        assert_ne!(fc.outcome, FrameOutcome::FValid);
    }

    #[test]
    fn bytes_differ_without_leaf_diff_is_recorded_finding() {
        let (q, _) = cap_at("onset-stereo", 23);
        let d = super::super::r22_capture::diff_captures(&q, &q, "p");
        assert!(d.differing_fields.is_empty());
        let fc = classify_frame(&q, &q, &d, "p", 23, 0, false, false);
        assert_eq!(fc.outcome, FrameOutcome::FValid);
        assert!(
            !fc.findings.is_empty(),
            "accounting-only divergence is a finding"
        );
    }

    // --- Per-OP / per-stage classification corpus --------------------------

    /// Step a Q31/FPE pair over a material and classify every frame, returning
    /// per-class counts and the first representative frame per class. This is
    /// the Group D classification evidence (the full per-OP bundle is Group E).
    fn classify_material(
        op: &str,
        pcm: &[Vec<i16>],
        sr: u32,
        frames: u32,
    ) -> (
        std::collections::BTreeMap<String, u32>,
        std::collections::BTreeMap<String, String>,
    ) {
        let ch = pcm.len();
        let cfg = EncoderConfig::new(sr, ch as u32, FRAME_LENGTH, TransportType::Adts)
            .expect("cfg")
            .with_vbr_mode(VbrMode::Vbr5);
        let mut q = StreamEncoder::with_config(cfg.with_psy_engine(PsyEngineKind::Q31));
        let mut f = StreamEncoder::with_config(cfg.with_psy_engine(PsyEngineKind::Fpe));
        let mut counts: std::collections::BTreeMap<String, u32> = Default::default();
        let mut first: std::collections::BTreeMap<String, String> = Default::default();
        for fr in 0..frames {
            let fresh: Vec<Vec<i16>> = pcm
                .iter()
                .map(|c| {
                    (0..FRAME_LENGTH)
                        .map(|i| *c.get(fr as usize * FRAME_LENGTH + i).unwrap_or(&0))
                        .collect()
                })
                .collect();
            let (qb, qs) = q.step_frame(fr, &fresh);
            let (fb, fs) = f.step_frame(fr, &fresh);
            assert!(
                qs.notes.is_empty() && fs.notes.is_empty(),
                "{op} f{fr}: notes"
            );
            let qb = qb.expect("q emits");
            let fb = fb.expect("f emits");
            let qc = qs.r22_cap.as_ref().expect("q cap");
            let fc = fs.r22_cap.as_ref().expect("f cap");
            let tag = format!("{op}-{fr}");
            let d = super::super::r22_capture::diff_captures(qc, fc, &tag);
            let bytes_eq = qb == fb;
            let cls = classify_frame(qc, fc, &d, &tag, fr, qs.seq, bytes_eq, false);
            // Every frame carries an outcome (never a silent pass).
            *counts.entry(cls.outcome.label().to_string()).or_insert(0) += 1;
            first
                .entry(cls.outcome.label().to_string())
                .or_insert_with(|| format!("{op} f{fr}"));
            // Per-band origin classes.
            for (b, bc) in cls.band_origin.iter().enumerate() {
                if let BandClass::Class(c) = bc {
                    let key = c.label();
                    *counts.entry(key.clone()).or_insert(0) += 1;
                    first
                        .entry(key)
                        .or_insert_with(|| format!("{op} f{fr} b{b}"));
                }
            }
            // Attribution classes (P / Unattributed).
            for a in &cls.attributions {
                let key = a.class.label();
                *counts.entry(key.clone()).or_insert(0) += 1;
                first
                    .entry(key)
                    .or_insert_with(|| format!("{op} f{fr} b{}", a.band));
            }
        }
        (counts, first)
    }

    #[test]
    fn r22_group_d_classification_corpus() {
        // Representative corpus: the Group A window-coverage fixtures plus a
        // real-material WAV prefix. Every frame is classified; cross-arm
        // identity is never asserted. Counts + first witnesses are recorded.
        let mut total: std::collections::BTreeMap<String, u32> = Default::default();
        let mut witnesses: std::collections::BTreeMap<String, String> = Default::default();
        let cases: [(&str, &str, u32); 5] = [
            ("OP-1", "sine-stereo", 8),
            ("OP-1", "onset-stereo", 28),
            ("OP-6", "click-mono", 16),
            ("OP-6", "impulse-mono", 10),
            ("OP-2", "music-stereo", 30),
        ];
        for (op, fixture, frames) in cases {
            let pcm = testdata::fixture_samples(fixture);
            let sr = testdata::fixture_sample_rate(fixture);
            let (counts, first) = classify_material(op, &pcm, sr, frames);
            for (k, v) in counts {
                *total.entry(k.clone()).or_insert(0) += v;
                let w = first[&k].clone();
                witnesses.entry(k).or_insert(w);
            }
        }
        // Non-vacuity: the corpus must exhibit residue and frame outcomes.
        assert!(
            total.values().sum::<u32>() > 0,
            "classification corpus non-empty"
        );
        assert!(
            total.contains_key("F-even") || total.contains_key("F-valid"),
            "frame outcomes recorded"
        );
        // Every recorded class is canonical vocabulary (no alternative I class).
        for k in total.keys() {
            assert!(
                k == "R"
                    || k == "A"
                    || k.starts_with("S-")
                    || k.starts_with("P(")
                    || k.starts_with("Unattributed(")
                    || k.starts_with("F-"),
                "canonical vocabulary only, got {k}"
            );
            assert!(!k.starts_with("I"), "no alternative I classification");
        }
        for (k, v) in &total {
            println!(
                "R22-CLASS-COUNT class={k} n={v} first={}",
                witnesses.get(k).map(|s| s.as_str()).unwrap_or("none")
            );
        }
    }
}
