//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-18.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! R2.1-B IS/MS decision-safety harness (test-only).
//!
//! Executable counterpart of the accepted FPE R2.1-B analytical report.
//! It establishes, using the actual production encoder paths, whether
//! differences between the physical-f64 FPE energy engine and the Q31
//! engine explain any Intensity Stereo (IS) or Mid/Side (MS) decision
//! changes. It does not change any production psychoacoustic calculation,
//! engine default, quantizer, rate control, or bitstream path.
//!
//! ## Architecture
//!
//! Two independent, persistent [`StreamEncoder`] pairs are stepped in
//! lockstep on identical PCM:
//!
//! * **Full pair** (Q31-default + FPE-selected): the real production
//!   [`StreamEncoder::step_frame`] path, yielding ADTS bytes and the
//!   [`FrameSnapshot`] (including the [`MsDecisionCapture`] and the
//!   R2.1-B `is_book_values` / `is_mask_values` diagnostics).
//! * **Frontend pair** (Q31 + FPE): the real production
//!   [`StreamEncoder::frontend`] path, yielding the pre-IS working sets.
//!
//! Both pairs start from fresh state and observe identical PCM, so the
//! frontend pair's pre-IS operands are the same values the full pair's
//! `step_frame` consumed internally. This lockstep assumption is not
//! trusted: every frame replays the production [`intensity_stereo`]
//! function on cloned frontend operands and asserts the replayed outputs
//! equal the production snapshot bit-exactly (gate 1). A mismatch is a
//! `ReplayMismatch` validation failure, never silently trusted.
//!
//! MS replays the production [`ms_stereo_mirror`] function on cloned
//! [`MsDecisionCapture`] operands and asserts mask/digest equality, then
//! recomputes the first-pass margin with exact integer semantics and
//! validates `M_ms` against `B_closed` (gates 2/5).
//!
//! ## Trace semantics and limitations
//!
//! The IS trace is test-side reconstruction over the actual production
//! decision functions ([`prepare_decisions`], [`finalize_decisions`],
//! [`sfb_correlation`], [`intensity_stereo`]): no production
//! instrumentation, no duplicated IS algorithm. The authoritative
//! decision is always the production function's output; the trace walks
//! the bands in production update order recomputing the source-derived
//! I1–I10 margins to explain each inclusion, rejection, direction drop,
//! and block clear, including stale-carry origin and cumulative
//! extension-error state. After the first Q31↔FPE divergence, later bands
//! in the same block are attributed as propagated (`OkPropagated`); no
//! local margin bound is claimed once its structural assumptions no
//! longer hold.
//!
//! Limitation: pre-IS operands reach the trace through the lockstep
//! frontend pair, not through in-`step_frame` clones (per §2.4 no
//! permanent production IS instrumentation is added). The per-frame
//! replay-equality check above is the explicit validator for this
//! indirection; see `validate_lockstep_replay`.
//!
//! ## Classification axes (independent)
//!
//! * **Cause**: A (input-mapping/representation), B (numerical
//!   energy/psychoacoustic, incl. out-of-envelope), C (established
//!   structural/implementation inconsistency only).
//! * **Validation status**: pass / out-of-envelope / bound-failure /
//!   domain-unsupported / replay-mismatch / ok-propagated.
//! * **Consequence**: N < L < P < G < F (record all applicable, keep the
//!   maximum).

use crate::aac_enc::config::{EncoderConfig, PsyEngineKind, TransportType, VbrMode, FRAME_LENGTH};
use crate::aac_enc::intensity::{
    finalize_decisions, intensity_stereo, prepare_decisions, sfb_correlation,
};
use crate::aac_enc::psy::PsyTables;
use crate::aac_enc::q31::{fmult, fmult_div2};
use crate::aac_enc::stream_driver::{
    ms_stereo_mirror, FrameSnapshot, MsDecisionCapture, StreamEncoder,
};
use crate::aac_enc::testdata;

// ---------------------------------------------------------------------------
// Engine tagging and ownership
// ---------------------------------------------------------------------------

/// Test-only engine origin tag. Every independently constructed operand set
/// carries one; replay helpers assert it at entry so a Q31-produced value
/// can never silently flow into an FPE replay.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EngineTag {
    Q31,
    Fpe,
}

/// Assert disjoint backing storage for two same-length slices.
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

// ---------------------------------------------------------------------------
// Source constants (mirror `intensity.rs`; algorithm stays in production)
// ---------------------------------------------------------------------------

/// `FL2FXCONST_DBL(0.95f)` exact rounded value (`intensity.rs:29`).
const CORR_THRESH: i32 = 0x7999_9980;
/// `FL2FXCONST_DBL(0.04f)` (`intensity.rs:30`).
const TOTAL_ERROR_THRESH: i32 = 0x051E_B850;
/// `FL2FXCONST_DBL(0.01f)` (`intensity.rs:31`).
const LOCAL_ERROR_THRESH: i32 = 0x0147_AE14;
/// `0.5f` (`intensity.rs:32`).
const DIRECTION_DEVIATION_THRESH: i32 = 0x4000_0000;
/// `FL2FXCONST_DBL(0.1f)` (`intensity.rs:33`).
const REGION_MIN_LOUDNESS: i32 = 0x0CCC_CCD0;
/// `FL2FXCONST_DBL(0.7f)` (`intensity.rs:34`).
const LEFT_RIGHT_RATIO_THRESH: i32 = 0x5999_9980;
/// `60/128` clamp rails (`intensity.rs:35-36`).
const REAL_SCALE_MAX: i32 = 0x3C00_0000;
const REAL_SCALE_MIN: i32 = -0x3C00_0000;
/// `FL2FXCONST_DBL(1.0f/1.5f)` (`intensity.rs:37`).
const ONE_POINT_FIVE_RECIP: i32 = 0x5555_5580;
/// `0.25f` / `0.125f` (`intensity.rs:38-39`).
const QUARTER: i32 = 0x2000_0000;
const EIGHTH: i32 = 0x1000_0000;
/// `0.5f/256` rounding half-LSB (`intensity.rs:40`).
const IS_SCALE_HALF_LSB: i32 = 0x0040_0000;
/// Grouping shifts (`intensity.rs:45-48`).
const MIN_IS_SFBS: i32 = 6;
const MAX_SFB_PER_GROUP_SF: i32 = 6;
const REAL_SCALE_SF: i32 = 1;
const LD_DATA_SHIFT: i32 = 6;
const DFRACT_BITS: i32 = 32;

/// Derived integer decision thresholds, exactly as production computes them.
const fn local_error_lim() -> i32 {
    LOCAL_ERROR_THRESH >> 3
}
const fn total_error_lim() -> i32 {
    TOTAL_ERROR_THRESH >> MAX_SFB_PER_GROUP_SF
}
const fn region_min_lim() -> i32 {
    REGION_MIN_LOUDNESS >> MAX_SFB_PER_GROUP_SF
}
const fn direction_lim() -> i32 {
    DIRECTION_DEVIATION_THRESH >> (REAL_SCALE_SF + LD_DATA_SHIFT - 2)
}

// ---------------------------------------------------------------------------
// Classification axes
// ---------------------------------------------------------------------------

/// Root-cause axis. `C` requires established structural evidence; a failed
/// bound or an invalid frame alone never implies `C`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cause {
    /// Documented input-mapping or representation effect.
    A,
    /// Numerical energy/psychoacoustic difference (incl. out-of-envelope).
    B,
    /// Established structural / state-management / ownership / replay /
    /// implementation inconsistency.
    C,
    /// Not yet investigated.
    Unclassified,
}

/// Validation-status axis (independent of cause and consequence).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ValStatus {
    Pass,
    OutOfEnvelope,
    BoundFailure,
    DomainUnsupported,
    ReplayMismatch,
    OkPropagated,
}

/// Consequence axis with severity ordering `N < L < P < G < F`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Consequence {
    /// No decision difference.
    N,
    /// Local per-band difference with documented margin and evidence.
    L,
    /// Persistent multi-frame cascade (requires bounded cascade + review).
    P,
    /// Grouped/short-block stereo difference (requires valid output + review).
    G,
    /// Frame-level but valid/decodable difference (requires trace + review).
    F,
}

/// Acceptance routing for one classified divergence. Returns `true` iff the
/// frame may pass with the recorded evidence (`P`/`G`/`F` additionally
/// require explicit owner review, tracked in the R2.1-B note; invalid
/// frames and unclassified divergences always fail here).
fn route(cause: Cause, status: ValStatus, cons: Consequence, frame_valid: bool) -> bool {
    if !frame_valid {
        return false;
    }
    match status {
        ValStatus::ReplayMismatch | ValStatus::BoundFailure => return false,
        // DomainUnsupported bands are reported explicitly (never counted as
        // passing bound validation); divergences there route as documented
        // findings with cause B (or A) and reviewable consequences.
        ValStatus::DomainUnsupported
        | ValStatus::Pass
        | ValStatus::OutOfEnvelope
        | ValStatus::OkPropagated => {}
    }
    match cause {
        Cause::C => false,
        Cause::Unclassified => false,
        Cause::A => true,
        Cause::B => match cons {
            Consequence::N | Consequence::L => true,
            // P/G/F are review-gated: routable only with evidence review.
            // The harness records them; the gate test asserts validity and
            // routes them to the findings table rather than failing here.
            Consequence::P | Consequence::G | Consequence::F => true,
        },
    }
}

/// One classified divergence with full evidence.
#[derive(Debug, Clone)]
struct Finding {
    material: &'static str,
    frame: u32,
    band: i32,
    cause: Cause,
    status: ValStatus,
    consequence: Consequence,
    detail: String,
}

// ---------------------------------------------------------------------------
// P0-B retained per-finding evidence (test-only, no decision behaviour)
// ---------------------------------------------------------------------------
//
// B3/B4: for every applicable finding retain IS operands, decision margins,
// trace chains and Class B line vectors using deterministic serialization
// and ordering. This module only *records* values already computed by the
// harness (`analyze_frame` locals); it never feeds a decision, bound,
// envelope or routing predicate. Stateless IS margins (I1/I2/I3/I8/I10,
// `real_scale`/`is_scale` exactness) are recomputed with the same helpers
// the trace uses. Stateful accumulator margins (I4/I6/I7, I5 carry) are
// reported via the retained trace events that carry them, never
// re-derived out of context. Genuinely unavailable values (digest
// findings with `band == -1`; unvisited/strided bands) are recorded
// explicitly as `None`/sentinel strings, not invented.

/// Stable per-finding evidence record. All integers render decimally;
/// integer vectors render as comma-separated decimals in `[...]`;
/// trace chains render as `Debug` event strings in production-walk order.
/// No `f64` enters the canonical form (spectra are `i32` Q31 lines), so no
/// decimal-float ambiguity exists. Ordering across findings is by
/// `(material_idx, frame, band, detail)` — never by hash-map order.
#[derive(Debug, Clone)]
struct FindingEvidence {
    material: &'static str,
    material_idx: usize,
    frame: u32,
    band: i32,
    cause: &'static str,
    status: &'static str,
    consequence: &'static str,
    detail: String,
    review_gated: bool,
    unsupported: bool,
    // IS operands (per band, both arms; `None` for digest findings band -1).
    q_e_l: Option<i32>,
    q_e_r: Option<i32>,
    f_e_l: Option<i32>,
    f_e_r: Option<i32>,
    q_eld_l: Option<i32>,
    q_eld_r: Option<i32>,
    f_eld_l: Option<i32>,
    f_eld_r: Option<i32>,
    q_t_l: Option<i32>,
    q_t_r: Option<i32>,
    f_t_l: Option<i32>,
    f_t_r: Option<i32>,
    q_real: Option<i32>,
    f_real: Option<i32>,
    q_scale: Option<i32>,
    f_scale: Option<i32>,
    q_book: Option<i32>,
    f_book: Option<i32>,
    q_mid: Option<i32>,
    f_mid: Option<i32>,
    q_pre: Option<i32>,
    f_pre: Option<i32>,
    // Stateless IS margins (exact recomputation; `None` where unavailable).
    q_i1: Option<i64>,
    f_i1: Option<i64>,
    q_i2a: Option<i64>,
    q_i2b: Option<i64>,
    f_i2a: Option<i64>,
    f_i2b: Option<i64>,
    q_i3: Option<i64>,
    f_i3: Option<i64>,
    q_i8a: Option<i64>,
    q_i8b: Option<i64>,
    f_i8a: Option<i64>,
    f_i8b: Option<i64>,
    q_i10a: Option<i64>,
    q_i10b: Option<i64>,
    f_i10a: Option<i64>,
    f_i10b: Option<i64>,
    // MS operands and first-pass margins (per band; `None` for digest
    // findings or bands outside the capture span).
    ms_supported: Option<bool>,
    q_m: Option<i64>,
    f_m: Option<i64>,
    b_closed: Option<i64>,
    use_q: Option<i32>,
    use_f: Option<i32>,
    d_el: Option<i64>,
    d_er: Option<i64>,
    d_em: Option<i64>,
    d_es: Option<i64>,
    d_tl: Option<i64>,
    d_tr: Option<i64>,
    // Trace chains: full frame event lists (both arms, walk order) plus the
    // filtered view for this finding's band.
    q_trace: Vec<String>,
    f_trace: Vec<String>,
    // Class B line vectors: Q31 spectral lines for the band window
    // `[spec_off..spec_end)` on both channels and both arms. `None` for
    // digest findings; empty vectors mean a zero-width window (recorded,
    // not omitted).
    spec_off: Option<usize>,
    spec_end: Option<usize>,
    q_spec_l: Option<Vec<i32>>,
    q_spec_r: Option<Vec<i32>>,
    f_spec_l: Option<Vec<i32>>,
    f_spec_r: Option<Vec<i32>>,
}

fn cause_str(c: Cause) -> &'static str {
    match c {
        Cause::A => "A",
        Cause::B => "B",
        Cause::C => "C",
        Cause::Unclassified => "U",
    }
}

fn status_str(s: ValStatus) -> &'static str {
    match s {
        ValStatus::Pass => "Pass",
        ValStatus::OutOfEnvelope => "OutOfEnvelope",
        ValStatus::BoundFailure => "BoundFailure",
        ValStatus::DomainUnsupported => "DomainUnsupported",
        ValStatus::ReplayMismatch => "ReplayMismatch",
        ValStatus::OkPropagated => "OkPropagated",
    }
}

fn consequence_str(c: Consequence) -> &'static str {
    match c {
        Consequence::N => "N",
        Consequence::L => "L",
        Consequence::P => "P",
        Consequence::G => "G",
        Consequence::F => "F",
    }
}

fn opt_i32(v: Option<i32>) -> String {
    match v {
        Some(x) => x.to_string(),
        None => "n/a".to_string(),
    }
}

fn opt_i64(v: Option<i64>) -> String {
    match v {
        Some(x) => x.to_string(),
        None => "n/a".to_string(),
    }
}

fn opt_vec(v: &Option<Vec<i32>>) -> String {
    match v {
        Some(xs) => {
            let mut s = String::from("[");
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&x.to_string());
            }
            s.push(']');
            s
        }
        None => "n/a".to_string(),
    }
}

impl FindingEvidence {
    fn stable_id(&self) -> String {
        format!("{}:f{}:b{}", self.material, self.frame, self.band)
    }

    /// Deterministic unique identifier for one evidence record.
    ///
    /// The original finding label (`stable_id`, `{material}:f{frame}:b{band}`)
    /// is preserved unchanged for provenance (it appears in the canonical
    /// string and in all historical prints). Because F-01/F-02 share the same
    /// band/frame (`quiet-sine-stereo:f16:b13`), the stable label alone is not
    /// unique; the unique id appends the 1-based occurrence ordinal of that
    /// stable label in deterministic sorted order
    /// (`sorted_evidences`: `(material_idx, frame, band, detail)`), e.g.
    /// `quiet-sine-stereo:f16:b13#1` and `...#2`. The ordinal derives only
    /// from the sorted position and the retained `detail` tie-break, so
    /// repeated executions produce identical identifiers. No classification
    /// or decision semantic is affected.
    fn unique_id(&self, ordinal: usize) -> String {
        format!("{}#{}", self.stable_id(), ordinal)
    }

    /// Deterministic canonical serialization. Field order is fixed;
    /// integers are decimal; vectors are bracketed decimal lists; traces
    /// are `|`-joined `Debug` strings in walk order. No map iteration,
    /// no pointer formatting, no float formatting enters this string.
    fn canonical_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "id={} cause={} status={} cons={} gated={} unsupported={} detail={}\n",
            self.stable_id(),
            self.cause,
            self.status,
            self.consequence,
            if self.review_gated { 1 } else { 0 },
            if self.unsupported { 1 } else { 0 },
            self.detail,
        ));
        s.push_str(&format!(
            "is q_e={}/{} f_e={}/{} q_eld={}/{} f_eld={}/{} q_t={}/{} f_t={}/{} q_real={} f_real={} q_scale={} f_scale={} q_book={} f_book={} q_mid={} f_mid={} q_pre={} f_pre={}\n",
            opt_i32(self.q_e_l),
            opt_i32(self.q_e_r),
            opt_i32(self.f_e_l),
            opt_i32(self.f_e_r),
            opt_i32(self.q_eld_l),
            opt_i32(self.q_eld_r),
            opt_i32(self.f_eld_l),
            opt_i32(self.f_eld_r),
            opt_i32(self.q_t_l),
            opt_i32(self.q_t_r),
            opt_i32(self.f_t_l),
            opt_i32(self.f_t_r),
            opt_i32(self.q_real),
            opt_i32(self.f_real),
            opt_i32(self.q_scale),
            opt_i32(self.f_scale),
            opt_i32(self.q_book),
            opt_i32(self.f_book),
            opt_i32(self.q_mid),
            opt_i32(self.f_mid),
            opt_i32(self.q_pre),
            opt_i32(self.f_pre),
        ));
        s.push_str(&format!(
            "margins q_i1={} f_i1={} q_i2={}/{} f_i2={}/{} q_i3={} f_i3={} q_i8={}/{} f_i8={}/{} q_i10={}/{} f_i10={}/{}\n",
            opt_i64(self.q_i1),
            opt_i64(self.f_i1),
            opt_i64(self.q_i2a),
            opt_i64(self.q_i2b),
            opt_i64(self.f_i2a),
            opt_i64(self.f_i2b),
            opt_i64(self.q_i3),
            opt_i64(self.f_i3),
            opt_i64(self.q_i8a),
            opt_i64(self.q_i8b),
            opt_i64(self.f_i8a),
            opt_i64(self.f_i8b),
            opt_i64(self.q_i10a),
            opt_i64(self.q_i10b),
            opt_i64(self.f_i10a),
            opt_i64(self.f_i10b),
        ));
        s.push_str(&format!(
            "ms supported={} q_m={} f_m={} B={} use={}/{} d=el:{} er:{} em:{} es:{} tl:{} tr:{}\n",
            match self.ms_supported {
                Some(true) => "1",
                Some(false) => "0",
                None => "n/a",
            },
            opt_i64(self.q_m),
            opt_i64(self.f_m),
            opt_i64(self.b_closed),
            opt_i32(self.use_q),
            opt_i32(self.use_f),
            opt_i64(self.d_el),
            opt_i64(self.d_er),
            opt_i64(self.d_em),
            opt_i64(self.d_es),
            opt_i64(self.d_tl),
            opt_i64(self.d_tr),
        ));
        s.push_str(&format!(
            "spec win={}..{} q_l={} q_r={} f_l={} f_r={}\n",
            self.spec_off.map_or("n/a".to_string(), |v| v.to_string()),
            self.spec_end.map_or("n/a".to_string(), |v| v.to_string()),
            opt_vec(&self.q_spec_l),
            opt_vec(&self.q_spec_r),
            opt_vec(&self.f_spec_l),
            opt_vec(&self.f_spec_r),
        ));
        s.push_str(&format!("trace_q={}\n", self.q_trace.join("|")));
        s.push_str(&format!("trace_f={}\n", self.f_trace.join("|")));
        s
    }
}

// ---------------------------------------------------------------------------
// MS first-pass margin, B_closed bound, supported domain
// ---------------------------------------------------------------------------

/// Exact-integer first-pass MS margin transcription of
/// `ms_stereo.cpp:185-201` (halved-shift form, strict `>` becomes
/// `margin >= 1`). `M_ms = pnms - pnlr - 1`; `use_ms = M_ms >= 0`.
/// Returns `(margins, mask)` over the `(0..cnt).step_by(per)` loop with the
/// `max`-bounded inner walk; IS bands and the ALL-upgrade path are handled
/// by the caller (this is the per-band margin, not the final mask).
#[allow(clippy::too_many_arguments)]
fn ms_first_pass_margins(
    eld_l: &[i32],
    eld_r: &[i32],
    tld_l: &[i32],
    tld_r: &[i32],
    em_ld: &[i32],
    es_ld: &[i32],
    cnt: usize,
    per: usize,
    max: usize,
    allow_ms: bool,
) -> (Vec<i64>, Vec<i32>) {
    let mut margins = vec![0i64; cnt];
    let mut mask = vec![0i32; cnt];
    for sfb in (0..cnt).step_by(per) {
        for off in 0..max {
            let b = sfb + off;
            if b >= cnt {
                break;
            }
            let min_tld = tld_l[b].min(tld_r[b]) as i64;
            let tmp = (eld_l[b] as i64).max(tld_l[b] as i64);
            let mut pnlr = (tld_l[b] as i64 >> 1) - (tmp >> 1);
            pnlr += tld_r[b] as i64 >> 1;
            let tmp = (eld_r[b] as i64).max(tld_r[b] as i64);
            pnlr -= tmp >> 1;
            let tmp = (em_ld[b] as i64).max(min_tld);
            let mut pnms = min_tld - (tmp >> 1);
            let tmp = (es_ld[b] as i64).max(min_tld);
            pnms -= tmp >> 1;
            let m = pnms - pnlr - 1;
            margins[b] = m;
            // Strict `>`: `pnms > pnlr <=> M_ms = pnms - pnlr - 1 >= 0`.
            mask[b] = i32::from(allow_ms && m >= 0);
        }
    }
    (margins, mask)
}

/// Candidate `B_closed` bound as reconstructed in
/// `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md` (owner-accepted D1
/// replacement for the missing original R2.1-B analytical report; see P0-A
/// report §3.1 and reconstruction §1).
///
/// ```text
/// M_ms = pnms - pnlr - 1
/// B_closed = ceil((dEL + dER + dEm + dEs) / 2)
///     + dTL + dTR + 2 * max(dTL, dTR) + 3
/// ```
///
/// All `d*` are per-band absolute deltas of the Q31 log-domain operands
/// between the arms: `dEL = |eld_l_q - eld_l_f|` (and likewise `dER` for
/// right, `dEm`/`dEs` for mid/side Ld), `dTL = |tld_l_q - tld_l_f|`,
/// `dTR = |tld_r_q - tld_r_f|`. Linearity in the deltas holds because
/// `pnms`/`pnlr` combine Ld operands with `>> 1` halvings and `min`/`max`
/// selections: each halved term shifts by at most `ceil(d/2)` (all deltas
/// are non-negative, so `(d + 1) / 2` is the exact ceiling), each threshold
/// selection propagates into both `pnms` and `pnlr`, and the trailing `+3`
/// is exactly 1 (min_tld parity) + 2 (four-way ceil grouping); the `-1` in
/// `M` cancels in the delta (see reconstruction §3.2 Steps 1–4).
fn b_closed(d_el: i64, d_er: i64, d_em: i64, d_es: i64, d_tl: i64, d_tr: i64) -> i64 {
    let half_sum = d_el + d_er + d_em + d_es;
    debug_assert!(half_sum >= 0, "B_closed deltas are absolute values");
    let half_ceil = (half_sum + 1) / 2;
    half_ceil + d_tl + d_tr + 2 * d_tl.max(d_tr) + 3
}

/// Supported operand domain for `B_closed` validation.
///
/// The bound reasons about finite Ld shifts, so bands where either arm
/// carries the zero-energy sentinel (`i32::MIN` from `calc_ld_data`) in any
/// of the four energy-Ld operands are domain-unsupported: the sentinel is
/// not a log value and its delta (up to ~2^31) is not a physical shift.
/// IS bands (`is_book != 0`) skip the MS first pass by C design and are
/// excluded from margin validation (their mask semantics are checked by
/// the IS half instead). Stride pads are still visited by the production
/// loop, so they remain in-domain when their operands are finite.
fn ms_band_supported(b: usize, cap_q: &MsDecisionCapture, cap_f: &MsDecisionCapture) -> bool {
    if cap_q.is_book.get(b).copied().unwrap_or(0) != 0 {
        return false;
    }
    if cap_f.is_book.get(b).copied().unwrap_or(0) != 0 {
        return false;
    }
    for v in [
        cap_q.eld_l[b],
        cap_q.eld_r[b],
        cap_q.em_ld[b],
        cap_q.es_ld[b],
        cap_f.eld_l[b],
        cap_f.eld_r[b],
        cap_f.em_ld[b],
        cap_f.es_ld[b],
    ] {
        if v == i32::MIN {
            return false;
        }
    }
    true
}

/// Outcome of validating one band's cross-arm MS margin against `B_closed`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MsBoundOutcome {
    /// Masks agree (no decision difference).
    Agree,
    /// Masks differ on a non-robust band: legitimate local difference that
    /// must be recorded with margin evidence (consequence L or higher).
    /// (A flip on a robust band is unreachable inside the bound: robustness
    /// `|M_q| > B` forces the other arm to the same side.)
    CloseCall,
    /// The measured margin shift exceeds the bound: failing validation
    /// status, triggers investigation (not automatically Cause C).
    BoundViolated,
}

/// Validate one band: recompute deltas, the bound, and both margins.
fn validate_ms_band(
    b: usize,
    cap_q: &MsDecisionCapture,
    cap_f: &MsDecisionCapture,
    m_q: i64,
    m_f: i64,
    use_q: i32,
    use_f: i32,
) -> (MsBoundOutcome, ValStatus, i64) {
    let d_el = (cap_q.eld_l[b] as i64 - cap_f.eld_l[b] as i64).abs();
    let d_er = (cap_q.eld_r[b] as i64 - cap_f.eld_r[b] as i64).abs();
    let d_em = (cap_q.em_ld[b] as i64 - cap_f.em_ld[b] as i64).abs();
    let d_es = (cap_q.es_ld[b] as i64 - cap_f.es_ld[b] as i64).abs();
    let d_tl = (cap_q.tld_l[b] as i64 - cap_f.tld_l[b] as i64).abs();
    let d_tr = (cap_q.tld_r[b] as i64 - cap_f.tld_r[b] as i64).abs();
    let bound = b_closed(d_el, d_er, d_em, d_es, d_tl, d_tr);
    let shift = (m_q - m_f).abs();
    if shift > bound {
        return (
            MsBoundOutcome::BoundViolated,
            ValStatus::BoundFailure,
            bound,
        );
    }
    if use_q == use_f {
        return (MsBoundOutcome::Agree, ValStatus::Pass, bound);
    }
    // Masks differ with a valid bound. A flip on a robust band (`|M_q| > B`)
    // is unreachable: robustness forces the other arm to the same side
    // (`M_q >= B+1` gives `M_f >= M_q - B >= 1`; symmetrically for use 0).
    // Hence every in-domain flip is a close call within B of the boundary,
    // numerically explained by the operand deltas. The robustness check is
    // retained as a debug assertion of that claim.
    let robust = if use_q == 1 {
        m_q > bound
    } else {
        m_q < -bound
    };
    debug_assert!(
        !robust,
        "robust band flipped inside the bound (bound unsound?)"
    );
    (MsBoundOutcome::CloseCall, ValStatus::OutOfEnvelope, bound)
}

#[cfg(test)]
mod ms_tests {
    use super::*;

    #[test]
    fn b_closed_matches_closed_form() {
        // ceil((1+2+3+4)/2)=5, +dTL(6)+dTR(7)+2*max(6,7)(14)+3 = 35.
        assert_eq!(b_closed(1, 2, 3, 4, 6, 7), 5 + 6 + 7 + 14 + 3);
        // Odd half-sum rounds up: ceil(3/2) = 2, plus the +3 slack.
        assert_eq!(b_closed(1, 1, 1, 0, 0, 0), 2 + 3);
        // Zero deltas leave only the +3 slack.
        assert_eq!(b_closed(0, 0, 0, 0, 0, 0), 3);
    }

    #[test]
    fn routing_policy_matches_acceptance_table() {
        // A with no decision difference: pass.
        assert!(route(Cause::A, ValStatus::Pass, Consequence::N, true));
        // A documented local difference: record and review (routable).
        assert!(route(
            Cause::A,
            ValStatus::OutOfEnvelope,
            Consequence::L,
            true
        ));
        // B local: accept with documented margin and evidence.
        assert!(route(Cause::B, ValStatus::Pass, Consequence::L, true));
        // B persistent/grouped/frame-level: routable, review-gated.
        assert!(route(Cause::B, ValStatus::Pass, Consequence::P, true));
        assert!(route(Cause::B, ValStatus::Pass, Consequence::G, true));
        assert!(route(Cause::B, ValStatus::Pass, Consequence::F, true));
        // Invalid frames always fail, even for Cause A.
        assert!(!route(Cause::A, ValStatus::Pass, Consequence::N, false));
        assert!(!route(Cause::B, ValStatus::Pass, Consequence::L, false));
        // C fails.
        assert!(!route(Cause::C, ValStatus::Pass, Consequence::L, true));
        // Unexplained fails pending investigation.
        assert!(!route(
            Cause::Unclassified,
            ValStatus::Pass,
            Consequence::L,
            true
        ));
        // Bound violations and replay mismatches fail their gates.
        assert!(!route(
            Cause::B,
            ValStatus::BoundFailure,
            Consequence::L,
            true
        ));
        assert!(!route(
            Cause::B,
            ValStatus::ReplayMismatch,
            Consequence::L,
            true
        ));
        // DomainUnsupported divergences are reported explicitly and route as
        // documented findings (never counted as bound-validated).
        assert!(route(
            Cause::B,
            ValStatus::DomainUnsupported,
            Consequence::L,
            true
        ));
    }

    #[test]
    fn ms_margin_boundary_strict_gt() {
        // Strict `>`: M_ms >= 0 uses MS, M_ms <= -1 does not.
        let mk = |m: i64| i32::from(m >= 0);
        assert_eq!(mk(1), 1);
        assert_eq!(mk(0), 1);
        assert_eq!(mk(-1), 0);
        assert_eq!(mk(-2), 0);
    }

    #[test]
    fn ms_supported_domain_excludes_sentinel_and_is_bands() {
        let mut q = MsDecisionCapture::default();
        let mut f = MsDecisionCapture::default();
        for cap in [&mut q, &mut f] {
            cap.eld_l = vec![100, 0, 100];
            cap.eld_r = vec![100, 100, 100];
            cap.em_ld = vec![100, 100, 100];
            cap.es_ld = vec![100, 100, 100];
            cap.is_book = vec![0, 0, 1];
            cap.cnt = 3;
        }
        // Band 0: finite operands, no IS -> supported.
        assert!(ms_band_supported(0, &q, &f));
        // Band 1: zero-energy sentinel on Q arm -> unsupported.
        q.eld_l[1] = i32::MIN;
        assert!(!ms_band_supported(1, &q, &f));
        // Band 2: IS band -> excluded from MS margin validation.
        assert!(!ms_band_supported(2, &q, &f));
    }

    #[test]
    fn ms_bound_outcome_matrix() {
        let mut q = MsDecisionCapture::default();
        let mut f = MsDecisionCapture::default();
        for cap in [&mut q, &mut f] {
            cap.eld_l = vec![100];
            cap.eld_r = vec![100];
            cap.em_ld = vec![100];
            cap.es_ld = vec![100];
            cap.tld_l = vec![50];
            cap.tld_r = vec![50];
            cap.is_book = vec![0];
        }
        // Identical operands: bound is 3, margins agree.
        let (o, s, b) = validate_ms_band(0, &q, &f, 10, 10, 1, 1);
        assert_eq!((o, s), (MsBoundOutcome::Agree, ValStatus::Pass));
        assert_eq!(b, 3);
        // Robust margins cannot flip inside the bound: M_q=10 with B=3
        // forces M_f >= 7 (use stays 1). A fabricated flip with shift 11
        // exceeds the bound and is a violation, never an explained flip.
        let (o, s, _) = validate_ms_band(0, &q, &f, 10, -1, 1, 0);
        assert_eq!(
            (o, s),
            (MsBoundOutcome::BoundViolated, ValStatus::BoundFailure)
        );
        // Close call: M_q=0 flips to -1 within the slack.
        let (o, s, _) = validate_ms_band(0, &q, &f, 0, -1, 1, 0);
        assert_eq!(
            (o, s),
            (MsBoundOutcome::CloseCall, ValStatus::OutOfEnvelope)
        );
    }
}

// ---------------------------------------------------------------------------
// IS margins I1–I10 (source-derived from `intensity.rs`)
// ---------------------------------------------------------------------------

/// I1 correlation margin: `|corr| - CORR_THRESH` (inclusive `>=` admits).
/// `corr` is the production [`sfb_correlation`] value for the band.
fn i1_margin(corr: i32) -> i64 {
    (corr as i64).abs() - CORR_THRESH as i64
}

/// I2 anti-centre (0.7) dual margin. Production sets the `1/8` sentinel iff
/// `e_l >= fmult(R,e_r) && fmult(R,e_l) <= e_r` with `R = 0.7f`.
/// Returns `(m_a, m_b)`; sentinel iff `m_a >= 0 && m_b <= 0`.
fn i2_margins(e_l: i32, e_r: i32) -> (i64, i64) {
    let m_a = e_l as i64 - fmult(LEFT_RIGHT_RATIO_THRESH, e_r) as i64;
    let m_b = fmult(LEFT_RIGHT_RATIO_THRESH, e_l) as i64 - e_r as i64;
    (m_a, m_b)
}

/// I3 local extension-error margin: `local_error - hrr_err[b]` (strict `<`
/// extends). Positive favors extension.
fn i3_margin(hrr_err: i32) -> i64 {
    local_error_lim() as i64 - hrr_err as i64
}

/// I4 cumulative extension-error margin: `total_error - overall` evaluated
/// with the production update order (the candidate band's `hrr_err >> 3`
/// contribution is added *before* the comparison). `overall_before` is the
/// accumulator entering the band; the margin uses the post-add value.
fn i4_margin(hrr_err: i32, overall_before: i32) -> i64 {
    let overall_after = overall_before.wrapping_add(hrr_err >> 3);
    total_error_lim() as i64 - overall_after as i64
}

/// I5 direction/stale-carry margin: `direction_thresh - |last - real|`
/// (strict `<` retains). `last` is the carried scale, which persists across
/// groups within the frame until a block clear resets it (carry origin is
/// tracked by the trace). Exact wrapping semantics: the production
/// `q31_abs` keeps `i32::MIN` negative, so an exact-`MIN` delta retains;
/// this margin reproduces that corner (margin stays positive there).
fn i5_margin(is_scale_last: i32, real_scale: i32) -> i64 {
    let d = is_scale_last.wrapping_sub(real_scale);
    let ad = (if d < 0 { d.wrapping_neg() } else { d }) as i64;
    direction_lim() as i64 - ad
}

/// I6 retention-count margin: `count - MIN_IS_SFBS` at the block boundary.
/// Non-negative retains (subject to I7).
fn i6_margin(count: i32) -> i64 {
    count as i64 - MIN_IS_SFBS as i64
}

/// I7 region-loudness margin: `region - region_min` with the production
/// update order (the band's `norm_loudness >> 6` is added before the block
/// check). `region_before` is the accumulator entering the band.
fn i7_margin(norm_loudness: i32, region_before: i32) -> i64 {
    let region_after = region_before.wrapping_add(norm_loudness >> 6);
    region_after as i64 - region_min_lim() as i64
}

/// I8 threshold-guard dual margin (processing stage, `intensity.rs:657`).
/// Production skips the band iff `e_l < thr_l && fmult(RECIP, e_r) > thr_r`.
/// Returns `(m_a, m_b)`; skip iff `m_a < 0 && m_b > 0`.
fn i8_margins(e_l: i32, e_r: i32, thr_l: i32, thr_r: i32) -> (i64, i64) {
    let m_a = e_l as i64 - thr_l as i64;
    let m_b = fmult(ONE_POINT_FIVE_RECIP, e_r) as i64 - thr_r as i64;
    (m_a, m_b)
}

/// I9 residual branch: sign of `lr` selects out-of-phase (`lr < 0`, `ed`)
/// vs in-phase (`lr >= 0`, `es`); the selected energy must be `> 0`.
/// Returns `(lr, selected_energy)`.
fn i9_branch(lr: i32, ed: i32, es: i32) -> (i32, i32) {
    if lr < 0 {
        (lr, ed)
    } else {
        (lr, es)
    }
}

/// I10 real-scale clamp dual margin: distance of the raw Ld difference to
/// the `±0x3C000000` rails. Returns `(to_max, from_min)`; clamping applies
/// when either is negative.
fn i10_margins(raw: i32) -> (i64, i64) {
    (
        REAL_SCALE_MAX as i64 - raw as i64,
        raw as i64 - REAL_SCALE_MIN as i64,
    )
}

#[cfg(test)]
mod is_margin_tests {
    use super::*;

    #[test]
    fn i1_correlation_boundary_inclusive() {
        assert!(i1_margin(CORR_THRESH) >= 0);
        assert!(i1_margin(CORR_THRESH - 1) < 0);
        assert!(i1_margin(CORR_THRESH + 1) > 0);
        // Sign-insensitive (production compares `fAbs(corr)`).
        assert!(i1_margin(-CORR_THRESH) >= 0);
        assert_eq!(i1_margin(0), -(CORR_THRESH as i64));
    }

    #[test]
    fn i2_anticentre_boundary_both_sides() {
        // Exactly dual-mono: sentinel set.
        let (a, b) = i2_margins(1000, 1000);
        assert!(a >= 0 && b <= 0, "dual-mono sentinel ({a},{b})");
        // Strongly panned: no sentinel (at least one side violates).
        let (a, b) = i2_margins(100, 10_000);
        assert!(a < 0 || b > 0, "panned escapes ({a},{b})");
        // fmult floor goes through the exact production helper: probe the
        // boundary value directly rather than assuming real arithmetic.
        let r = LEFT_RIGHT_RATIO_THRESH;
        let e = 1_000_000i32;
        let q = fmult(r, e);
        assert_eq!(i2_margins(q, e).0, 0, "m_a exact zero at fmult value");
        assert_eq!(i2_margins(e, q).1, 0, "m_b exact zero at fmult value");
    }

    #[test]
    fn i3_local_error_strict_boundary() {
        assert!(i3_margin(local_error_lim() - 1) > 0);
        assert_eq!(i3_margin(local_error_lim()), 0, "strict <: zero rejects");
        assert!(i3_margin(local_error_lim() + 1) < 0);
    }

    #[test]
    fn i4_cumulative_shift_aware_boundary() {
        // Contribution added BEFORE the comparison: overall_before=0 with
        // hrr = 8<<3 lands exactly on total_error - 8... verify order.
        let h = 8i32 << 3;
        let before = total_error_lim() - (h >> 3) - 1;
        assert!(i4_margin(h, before) > 0);
        let before_eq = total_error_lim() - (h >> 3);
        assert_eq!(i4_margin(h, before_eq), 0, "post-add equality rejects");
        // Arithmetic shift of a negative contribution (production uses `>>`
        // on possibly-negative hrr_err, not a logical shift).
        let neg = -8i32;
        assert_eq!(neg >> 3, -1);
        assert_eq!(i4_margin(neg, 0), total_error_lim() as i64 + 1);
    }

    #[test]
    fn i5_direction_strict_and_stale_carry() {
        assert!(i5_margin(100, 100 + direction_lim() - 1) > 0);
        assert_eq!(i5_margin(100, 100 + direction_lim()), 0);
        assert!(i5_margin(100, 100 + direction_lim() + 1) < 0);
        // Negative deltas behave symmetrically (absolute value).
        assert_eq!(i5_margin(100, 100 - direction_lim()), 0, "abs deviation");
    }

    #[test]
    fn i6_retention_count_boundary() {
        assert!(i6_margin(MIN_IS_SFBS - 1) < 0, "5 clears");
        assert_eq!(i6_margin(MIN_IS_SFBS), 0, "6 retains");
        assert!(i6_margin(MIN_IS_SFBS + 1) > 0);
    }

    #[test]
    fn i7_region_loudness_shift_aware_boundary() {
        let add = 64i32 << 6;
        let before = region_min_lim() - (add >> 6) - 1;
        let _ = before;
        // Post-add accounting: land exactly on the minimum.
        let before_eq = region_min_lim() - (add >> 6);
        assert_eq!(i7_margin(add, before_eq), 0);
        assert!(i7_margin(add, before_eq - 1) < 0);
        assert!(i7_margin(add, before_eq + 1) > 0);
    }

    #[test]
    fn i8_threshold_guard_strict_both_sides() {
        // Skip requires m_a < 0 AND m_b > 0.
        let (a, b) = i8_margins(10, 1_000_000, 100, 100);
        assert!(a < 0 && b > 0, "guard skips ({a},{b})");
        // Equality on either side proceeds.
        let (a, _) = i8_margins(100, 1_000_000, 100, 100);
        assert_eq!(a, 0, "m_a == 0 proceeds");
        let recip = fmult(ONE_POINT_FIVE_RECIP, 1_000_000);
        let (_, b) = i8_margins(10, 1_000_000, 100, recip);
        assert_eq!(b, 0, "m_b == 0 proceeds");
    }

    #[test]
    fn i9_branch_sign_and_positivity() {
        assert_eq!(i9_branch(-5, 10, 20), (-5, 10));
        assert_eq!(i9_branch(0, 10, 20), (0, 20), "lr==0 takes in-phase");
        assert_eq!(i9_branch(5, 10, 20), (5, 20));
        // Non-positive residual energy rejects in both branches.
        assert!(i9_branch(-5, 0, 20).1 <= 0);
        assert!(i9_branch(5, 10, 0).1 <= 0);
    }

    #[test]
    fn i10_clamp_rail_boundaries() {
        assert_eq!(i10_margins(REAL_SCALE_MAX), (0, _i10_span()));
        assert_eq!(i10_margins(REAL_SCALE_MIN).1, 0);
        assert!(i10_margins(REAL_SCALE_MAX + 1).0 < 0);
        assert!(i10_margins(REAL_SCALE_MIN - 1).1 < 0);
        assert!(i10_margins(0).0 > 0 && i10_margins(0).1 > 0);
    }

    fn _i10_span() -> i64 {
        REAL_SCALE_MAX as i64 - REAL_SCALE_MIN as i64
    }

    #[test]
    fn production_correlation_degenerate_and_deterministic() {
        // Width-0 and all-zero bands read 0 (production guards).
        assert_eq!(sfb_correlation(&[0, 0], &[0, 0], 0, 0), 0);
        assert_eq!(sfb_correlation(&[5, 5, 5, 5], &[5, 5, 5, 5], 0, 4), 0);
        // Deterministic on real production spectra.
        let spec = vec![1000i32, -2000, 3000, -4000, 5000, -6000, 7000, -8000];
        let c1 = sfb_correlation(&spec, &spec, 0, 8);
        let c2 = sfb_correlation(&spec, &spec, 0, 8);
        assert_eq!(c1, c2);
        // Margin identities hold on the production value.
        assert_eq!(i1_margin(c1), (c1 as i64).abs() - CORR_THRESH as i64);
    }
}

// ---------------------------------------------------------------------------
// IS capture, production replay, and stateful trace
// ---------------------------------------------------------------------------

/// Cloned pre-IS operands for one engine arm, captured from the lockstep
/// production frontend. The tag enforces engine ownership at every replay.
struct IsOperands {
    tag: EngineTag,
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
    allow_is: bool,
}

/// Production [`intensity_stereo`] outputs plus the mutated vectors, for
/// lockstep validation against the full-encoder snapshot.
struct IsReplayOut {
    is_book: Vec<i32>,
    is_scale: Vec<i32>,
    is_mask: Vec<i32>,
    is_digest: i32,
    e_r: Vec<i32>,
    t_r: Vec<i32>,
    t_ld_r: Vec<i32>,
    spread_r: Vec<i32>,
    e_ld_r: Vec<i32>,
    spec_l: Vec<i32>,
    spec_r: Vec<i32>,
}

/// Replay the production `intensity_stereo` on cloned operands. The
/// caller's tag proves which engine's operands these are; cross-engine
/// reuse panics here, not downstream.
fn replay_intensity(op: &IsOperands, expect: EngineTag) -> IsReplayOut {
    assert_eq!(op.tag, expect, "IS replay: engine tag");
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
        op.allow_is,
    );
    IsReplayOut {
        is_book: res.is_book,
        is_scale: res.is_scale,
        is_mask: res.ms_mask,
        is_digest: res.ms_digest,
        e_r,
        t_r,
        t_ld_r,
        spread_r,
        e_ld_r,
        spec_l,
        spec_r,
    }
}

/// Production prepare-stage vectors (authoritative, from the real
/// [`prepare_decisions`] on cloned operands).
struct IsStage {
    hrr_err: Vec<i32>,
    is_mask_pre: Vec<i32>,
    real_scale: Vec<i32>,
    norm_loudness: Vec<i32>,
}

fn replay_prepare(op: &IsOperands, expect: EngineTag) -> IsStage {
    assert_eq!(op.tag, expect, "IS prepare replay: engine tag");
    let mut hrr_err = vec![0i32; op.cnt];
    let mut is_mask = vec![0i32; op.cnt];
    let mut real_scale = vec![0i32; op.cnt];
    let mut norm_loudness = vec![0i32; op.cnt];
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
        &mut hrr_err,
        &mut is_mask,
        &mut real_scale,
        &mut norm_loudness,
    );
    IsStage {
        hrr_err,
        is_mask_pre: is_mask,
        real_scale,
        norm_loudness,
    }
}

/// Replay the production [`finalize_decisions`] on cloned prepare outputs.
fn replay_finalize(stage: &IsStage, op: &IsOperands, expect: EngineTag) -> Vec<i32> {
    assert_eq!(op.tag, expect, "IS finalize replay: engine tag");
    let mut is_mask = stage.is_mask_pre.clone();
    finalize_decisions(
        &stage.hrr_err,
        &mut is_mask,
        &stage.real_scale,
        &stage.norm_loudness,
        op.cnt,
        op.per,
        op.max,
    );
    is_mask
}

/// Exact `is_scale` recomputation for a coded band (`intensity.rs:567`).
fn expected_is_scale(real_scale: i32) -> i32 {
    if real_scale < 0 {
        ((real_scale >> 1).wrapping_sub(IS_SCALE_HALF_LSB)
            >> (DFRACT_BITS - 1 - REAL_SCALE_SF - LD_DATA_SHIFT - 1))
            .wrapping_add(1)
    } else {
        (real_scale >> 1).wrapping_add(IS_SCALE_HALF_LSB)
            >> (DFRACT_BITS - 1 - REAL_SCALE_SF - LD_DATA_SHIFT - 1)
    }
}

/// Exact `real_scale` recomputation (`intensity.rs:218-219`).
fn expected_real_scale(e_ld_l: i32, e_ld_r: i32) -> i32 {
    let is_value = e_ld_l.wrapping_sub(e_ld_r);
    is_value.clamp(REAL_SCALE_MIN, REAL_SCALE_MAX)
}

/// Trace event kinds. The trace annotates production decisions; it never
/// decides.
///
/// Fields are retained trace evidence: every event carries the band, the
/// margins, and the carry state needed to explain the decision. They are
/// read through the `Debug` rendering during investigation (failure
/// messages, `--nocapture` runs, and the R2.1-B note), not through
/// production code paths.
#[allow(dead_code)]
#[derive(Debug, Clone)]
enum TraceEvent {
    /// Band admitted by the correlation path (I1, or sentinel-bypassed).
    Admit { band: usize, corr: i32, i1: i64 },
    /// Band admitted by extension into an open block (I3 + I4).
    Extend { band: usize, i3: i64, i4: i64 },
    /// Band rejected while its block stayed open (I3/I4 fail).
    Reject { band: usize, i3: i64, i4: i64 },
    /// Band cleared by the direction check (I5), with carry origin.
    DirectionDrop {
        band: usize,
        i5: i64,
        last: i32,
        origin: Option<(usize, usize)>,
    },
    /// Carry update on retention (I5 pass), recording the new origin.
    CarryUpdate { band: usize, i5: i64, value: i32 },
    /// Block cleared at its boundary (I6 and/or I7 fail), with range.
    BlockClear {
        start: usize,
        end: usize,
        i6: i64,
        i7: i64,
    },
    /// Pre-mask band dropped by the processing stage (geometry / I8 / I9).
    ProcessingDrop { band: usize, kind: &'static str },
    /// First Q31↔FPE divergence and its margins on both arms.
    FirstDivergence { band: usize, q_mid: i32, f_mid: i32 },
    /// Post-divergence band inside an affected block (no local bound).
    Propagated { band: usize },
}

/// Annotation cursor mirroring the production update order
/// (`finalize_decisions`, `intensity.rs:301-374`) for margin attribution.
/// Read-only w.r.t. decisions: every mask it observes comes from
/// production replays; any inconsistency fails loudly.
struct TraceCursor {
    is_scale_last: i32,
    found: bool,
    origin: Option<(usize, usize)>,
}

impl TraceCursor {
    fn new() -> Self {
        Self {
            is_scale_last: 0,
            found: false,
            origin: None,
        }
    }
}

/// Build the explanatory trace for one arm and validate it against the
/// production replay outputs. Returns the event list. Panics on any
/// trace/production inconsistency (trace bug fails loudly, never silently
/// reinterpreted).
///
/// The band walks are indexed (`needless_range_loop` allowed) because they
/// mirror the production `(0..cnt).step_by(per)` / `(0..max)` walk structure
/// band-for-band; iterator rewrites would obscure the order equivalence.
#[allow(clippy::needless_range_loop)]
fn build_trace(
    frame: u32,
    tag: EngineTag,
    op: &IsOperands,
    stage: &IsStage,
    mid_mask: &[i32],
    replay: &IsReplayOut,
) -> Vec<TraceEvent> {
    let what = format!("f{frame} {tag:?}");
    let grp_index = |b: usize| (b / op.per.max(1), b);
    let mut events = Vec::new();
    let mut cursor = TraceCursor::new();
    // real_scale exactness (I10 clamp path) on visited bands; unvisited
    // tail bands keep the zero init by C design (prepare never reaches them).
    for b in 0..op.cnt {
        let is_visited = (b % op.per.max(1)) < op.max;
        if is_visited {
            assert_eq!(
                stage.real_scale[b],
                expected_real_scale(op.e_ld_l[b], op.e_ld_r[b]),
                "{what} trace: real_scale exact band {b}"
            );
        } else {
            assert_eq!(
                stage.real_scale[b], 0,
                "{what} trace: unvisited band {b} keeps zero real_scale"
            );
        }
    }
    // hrr exactness (prepare second pass, `intensity.rs:275-281`): sentinel
    // bands keep EIGHTH, unvisited bands keep zero init, all other visited
    // bands carry `fmult_div2(QUARTER - corr>>2, loudness)` with the
    // production correlation replayed on the cloned spectra.
    for b in 0..op.cnt {
        let is_visited = (b % op.per.max(1)) < op.max;
        if !is_visited {
            assert_eq!(
                stage.hrr_err[b], 0,
                "{what} trace: unvisited band {b} keeps zero hrr"
            );
            continue;
        }
        let (m_a, m_b) = i2_margins(op.e_l[b], op.e_r[b]);
        if m_a >= 0 && m_b <= 0 {
            assert_eq!(
                stage.hrr_err[b], EIGHTH,
                "{what} trace: sentinel band {b} keeps hrr"
            );
            continue;
        }
        let corr = sfb_correlation(
            &op.spec_l,
            &op.spec_r,
            op.offsets[b] as usize,
            op.offsets[b + 1] as usize,
        );
        let expected = fmult_div2(QUARTER.wrapping_sub(corr >> 2), stage.norm_loudness[b]);
        assert_eq!(
            stage.hrr_err[b], expected,
            "{what} trace: hrr exact band {b} (corr {corr})"
        );
    }
    // is_scale exactness on every coded band.
    for b in 0..op.cnt {
        if replay.is_book[b] != 0 {
            assert_eq!(
                replay.is_scale[b],
                expected_is_scale(stage.real_scale[b]),
                "{what} trace: is_scale exact band {b}"
            );
            assert_ne!(mid_mask[b], 0, "{what} trace: coded band {b} had mid mask");
        }
    }
    for grp in (0..op.cnt).step_by(op.per) {
        let mut start_is_sfb = 0usize;
        let mut in_is_block = false;
        let mut count = 0i32;
        let mut overall = 0i32;
        let mut region = 0i32;
        // Per-band fate inside this group, verified against the production
        // mid mask at group end (a band's fate is final only once later
        // direction drops and block clears in the same group are known).
        let mut admitted: Vec<(usize, bool)> = Vec::new();
        let mut dir_dropped: Vec<bool> = Vec::new();
        let mut clears: Vec<(usize, usize)> = Vec::new();
        for off in 0..op.max {
            let b = grp + off;
            if b >= op.cnt {
                break;
            }
            let pre = stage.is_mask_pre[b];
            let mut was_admitted = false;
            // --- Admission / extension (mirrors finalize order) ---
            if pre == 1 {
                if count == 0 {
                    start_is_sfb = b;
                }
                if !cursor.found {
                    cursor.is_scale_last = stage.real_scale[b];
                    cursor.found = true;
                    cursor.origin = Some(grp_index(b));
                }
                in_is_block = true;
                count += 1;
                overall = overall.wrapping_add(stage.hrr_err[b] >> 3);
                region = region.wrapping_add(stage.norm_loudness[b] >> 6);
                let corr = sfb_correlation(
                    &op.spec_l,
                    &op.spec_r,
                    op.offsets[b] as usize,
                    op.offsets[b + 1] as usize,
                );
                events.push(TraceEvent::Admit {
                    band: b,
                    corr,
                    i1: i1_margin(corr),
                });
                assert!(
                    i1_margin(corr) >= 0,
                    "{what} trace: pre-admitted band {b} violates I1 (corr {corr})"
                );
                was_admitted = true;
            } else if in_is_block {
                overall = overall.wrapping_add(stage.hrr_err[b] >> 3);
                region = region.wrapping_add(stage.norm_loudness[b] >> 6);
                let i3 = i3_margin(stage.hrr_err[b]);
                // I4 uses the post-add accumulator (production order).
                let i4 = total_error_lim() as i64 - overall as i64;
                if stage.hrr_err[b] < local_error_lim() && overall < total_error_lim() {
                    count += 1;
                    events.push(TraceEvent::Extend { band: b, i3, i4 });
                    was_admitted = true;
                } else {
                    in_is_block = false;
                    events.push(TraceEvent::Reject { band: b, i3, i4 });
                }
            }
            // --- Direction check (applies whenever the block is open,
            // including freshly admitted/extended bands: production sets the
            // mask then drops it in the same iteration) ---
            let mut dropped = false;
            if in_is_block {
                let i5 = i5_margin(cursor.is_scale_last, stage.real_scale[b]);
                if i5 > 0 {
                    cursor.is_scale_last = stage.real_scale[b];
                    cursor.origin = Some(grp_index(b));
                    events.push(TraceEvent::CarryUpdate {
                        band: b,
                        i5,
                        value: stage.real_scale[b],
                    });
                } else {
                    // A direction drop requires an open block, which requires
                    // a prior admission that always establishes the carry
                    // origin (admit sets found+origin together; clears reset
                    // both together). No anonymous drops.
                    assert!(
                        cursor.origin.is_some(),
                        "{what} trace: direction drop at band {b} without carry origin"
                    );
                    events.push(TraceEvent::DirectionDrop {
                        band: b,
                        i5,
                        last: cursor.is_scale_last,
                        origin: cursor.origin,
                    });
                    in_is_block = false;
                    count -= 1;
                    dropped = true;
                }
            }
            admitted.push((b, was_admitted));
            dir_dropped.push(dropped);
            // --- Block-boundary check ---
            if count > 0 && (!in_is_block || off + 1 == op.max) {
                let i6 = i6_margin(count);
                let i7 = region as i64 - region_min_lim() as i64;
                if count < MIN_IS_SFBS || region < region_min_lim() {
                    events.push(TraceEvent::BlockClear {
                        start: start_is_sfb,
                        end: b,
                        i6,
                        i7,
                    });
                    clears.push((start_is_sfb, b));
                    cursor.is_scale_last = 0;
                    cursor.found = false;
                    cursor.origin = None;
                    for j in 0..start_is_sfb {
                        if mid_mask[j] != 0 {
                            cursor.is_scale_last = stage.real_scale[j];
                            cursor.found = true;
                            cursor.origin = Some(grp_index(j));
                        }
                    }
                }
                count = 0;
                overall = 0;
                region = 0;
            }
            // --- Processing-stage attribution (mid -> book) ---
            if mid_mask[b] != 0 && replay.is_book[b] == 0 {
                let start = op.offsets[b] as usize;
                let end = op.offsets[b + 1] as usize;
                let kind = if end <= start {
                    "geometry"
                } else {
                    let (m_a, m_b) = i8_margins(op.e_l[b], op.e_r[b], op.t_l[b], op.t_r[b]);
                    if m_a < 0 && m_b > 0 {
                        "I8-threshold"
                    } else {
                        "I9-residual"
                    }
                };
                events.push(TraceEvent::ProcessingDrop { band: b, kind });
            }
        }
        // --- Group-end verification: every band's fate is final now that
        // same-group direction drops and block clears are known. Expected
        // mid = admitted && !direction-dropped && !block-cleared.
        for (i, (b, was_admitted)) in admitted.iter().enumerate() {
            let cleared = clears.iter().any(|&(s, e)| *b >= s && *b <= e);
            let expected = i32::from(*was_admitted && !dir_dropped[i] && !cleared);
            assert_eq!(
                mid_mask[*b], expected,
                "{what} trace: group-end mid mismatch band {b} \
                 (admitted={was_admitted} dropped={} cleared={cleared})",
                dir_dropped[i]
            );
        }
    }
    events
}

// ---------------------------------------------------------------------------
// Live dual-engine harness (gates 1/2/6/10)
// ---------------------------------------------------------------------------

/// Rebuild the production `PsyTables` long-config `allow_is` for the exact
/// operating point the harness encoders use (VBR-5 per-channel bitrates,
/// config bandwidth). No production accessor is added for this: the tables
/// are a pure function of `(sample_rate, ch_br, bandwidth)` and the
/// committed snapshots, so the test rebuild is bit-exact by construction.
fn tables_allow_is(sample_rate: u32, channels: usize) -> bool {
    let (ch_br, tns_br) = if channels == 1 {
        (112_000u32, 112_000u32)
    } else {
        (96_000u32, 192_000u32)
    };
    let cfg = EncoderConfig::new(
        sample_rate,
        channels as u32,
        FRAME_LENGTH,
        TransportType::Adts,
    )
    .expect("harness cfg")
    .with_vbr_mode(VbrMode::Vbr5);
    let bw = cfg.effective_bandwidth_hz();
    let ((lcfg, _), (scfg, _)) = testdata::load_cfg(sample_rate, ch_br);
    let tables = PsyTables::for_rate(
        sample_rate,
        ch_br,
        tns_br,
        bw,
        lcfg.min_snr_ld,
        scfg.min_snr_ld,
    )
    .expect("harness tables");
    tables.long_cfg.allow_is()
}

/// One frame stepped on all four production encoders.
struct FramePair {
    frame: u32,
    q_bytes: Option<Vec<u8>>,
    f_bytes: Option<Vec<u8>>,
    q_snap: FrameSnapshot,
    f_snap: FrameSnapshot,
    q_op: IsOperands,
    f_op: IsOperands,
}

/// Two independent persistent `StreamEncoder` pairs (full production path +
/// frontend lockstep), one per engine, stepped on identical PCM.
struct R21BHarness {
    q_full: StreamEncoder,
    f_full: StreamEncoder,
    q_fe: StreamEncoder,
    f_fe: StreamEncoder,
    pcm: Vec<Vec<i16>>,
    allow_is: bool,
}

impl R21BHarness {
    fn new(pcm: Vec<Vec<i16>>, sample_rate: u32) -> Self {
        let channels = pcm.len();
        assert!(channels == 2, "R2.1-B harness is stereo-only");
        let q_cfg = EncoderConfig::new(
            sample_rate,
            channels as u32,
            FRAME_LENGTH,
            TransportType::Adts,
        )
        .expect("harness cfg")
        .with_vbr_mode(VbrMode::Vbr5);
        let f_cfg = q_cfg.with_psy_engine(PsyEngineKind::Fpe);
        let allow_is = tables_allow_is(sample_rate, channels);
        Self {
            q_full: StreamEncoder::with_config(q_cfg),
            f_full: StreamEncoder::with_config(f_cfg),
            q_fe: StreamEncoder::with_config(q_cfg),
            f_fe: StreamEncoder::with_config(f_cfg),
            pcm,
            allow_is,
        }
    }

    /// 1024 fresh samples per channel for `frame`, zero-padded past the end.
    fn fresh_vecs(&self, frame: u32) -> Vec<Vec<i16>> {
        (0..self.pcm.len())
            .map(|c| {
                (0..FRAME_LENGTH)
                    .map(|i| {
                        let t = frame as usize * FRAME_LENGTH + i;
                        *self.pcm[c].get(t).unwrap_or(&0)
                    })
                    .collect()
            })
            .collect()
    }

    /// Step all four encoders on the same PCM frame. Frontend structural
    /// notes fail loudly (a skipped production frame is never silent).
    fn step(&mut self, frame: u32) -> FramePair {
        let fresh = self.fresh_vecs(frame);
        let fresh1024: Vec<[i16; 1024]> = fresh
            .iter()
            .map(|v| {
                let mut b = [0i16; 1024];
                b.copy_from_slice(&v[..1024]);
                b
            })
            .collect();
        let q_fe_out = self
            .q_fe
            .frontend(frame, fresh1024.clone())
            .unwrap_or_else(|note| panic!("Q31 frontend f{frame}: {note}"));
        let f_fe_out = self
            .f_fe
            .frontend(frame, fresh1024)
            .unwrap_or_else(|note| panic!("FPE frontend f{frame}: {note}"));
        assert_live_geometry(&q_fe_out, &f_fe_out, frame);
        let q_op = IsOperands::from_frontend(EngineTag::Q31, &q_fe_out, self.allow_is, frame);
        let f_op = IsOperands::from_frontend(EngineTag::Fpe, &f_fe_out, self.allow_is, frame);
        q_op.assert_disjoint_vs(&f_op, frame);
        let (q_bytes, q_snap) = self.q_full.step_frame(frame, &fresh);
        let (f_bytes, f_snap) = self.f_full.step_frame(frame, &fresh);
        FramePair {
            frame,
            q_bytes,
            f_bytes,
            q_snap,
            f_snap,
            q_op,
            f_op,
        }
    }
}

/// Bit-exact engine-invariant operand geometry (gate 6): the integer lane
/// is authoritative and shared, so counts, grouping, offsets, `mdct_scale`
/// and the common maximum must match bit-exactly between arms. Stored value
/// vectors are NOT compared here — numerical equivalence is owned by the
/// R1 energy envelope and the margin analysis below.
fn assert_live_geometry(
    q: &crate::aac_enc::stream_driver::FrontendOut,
    f: &crate::aac_enc::stream_driver::FrontendOut,
    frame: u32,
) {
    assert_eq!(q.live.len(), f.live.len(), "f{frame}: channel count");
    assert_eq!(q.seqs, f.seqs, "f{frame}: sequences agree");
    assert_eq!(q.groupings, f.groupings, "f{frame}: groupings agree");
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
        assert_eq!(ql.offsets().len(), ql.cnt() + 1, "f{frame}: offsets span");
    }
    match (&q.ms, &f.ms) {
        (Some(_), Some(_)) => {}
        (None, None) => {}
        _ => panic!("f{frame}: MS presence differs between arms"),
    }
}

impl IsOperands {
    fn from_frontend(
        tag: EngineTag,
        fe: &crate::aac_enc::stream_driver::FrontendOut,
        allow_is: bool,
        frame: u32,
    ) -> Self {
        assert_eq!(fe.live.len(), 2, "f{frame}: stereo operands");
        let (l, r) = (&fe.live[0], &fe.live[1]);
        assert_eq!(l.cnt(), r.cnt(), "f{frame}: stereo cnt agree");
        assert_eq!(l.per(), r.per(), "f{frame}: stereo per agree");
        Self {
            tag,
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
            allow_is,
        }
    }

    fn assert_disjoint_vs(&self, other: &Self, frame: u32) {
        assert_eq!(self.tag, EngineTag::Q31, "f{frame}: Q31 tag");
        assert_eq!(other.tag, EngineTag::Fpe, "f{frame}: FPE tag");
        assert_eq!(self.cnt, other.cnt, "f{frame}: cnt");
        assert_disjoint(&self.e_l, &other.e_l, &format!("f{frame} e_l"));
        assert_disjoint(&self.e_r, &other.e_r, &format!("f{frame} e_r"));
        assert_disjoint(&self.spec_l, &other.spec_l, &format!("f{frame} spec_l"));
        assert_disjoint(&self.spec_r, &other.spec_r, &format!("f{frame} spec_r"));
        assert_disjoint(&self.offsets, &other.offsets, &format!("f{frame} offsets"));
    }
}

/// Gate 1: replay the production `intensity_stereo` on cloned frontend
/// operands and require bit-exact agreement with the full-encoder
/// production snapshot (books, scales, masks, digest, and every mutated
/// right-channel vector vs the post-IS `MsDecisionCapture`).
fn validate_lockstep_replay(pair: &FramePair) {
    for (tag, op, snap) in [
        (EngineTag::Q31, &pair.q_op, &pair.q_snap),
        (EngineTag::Fpe, &pair.f_op, &pair.f_snap),
    ] {
        let replay = replay_intensity(op, tag);
        let cap = snap
            .ms_cap
            .as_ref()
            .unwrap_or_else(|| panic!("f{} {tag:?}: stereo frame without MS capture", pair.frame));
        assert_eq!(
            replay.is_book, snap.is_book_values,
            "f{} {tag:?}: IS book replay",
            pair.frame
        );
        assert_eq!(
            replay.is_scale, snap.is_scale_values,
            "f{} {tag:?}: IS scale replay",
            pair.frame
        );
        assert_eq!(
            replay.is_mask, snap.is_mask_values,
            "f{} {tag:?}: IS phase-mask replay",
            pair.frame
        );
        assert_eq!(
            replay.is_digest, snap.is_digest,
            "f{} {tag:?}: IS digest replay",
            pair.frame
        );
        // Post-IS right-channel vectors must equal the pre-MS capture.
        assert_eq!(replay.e_r, cap.e_r, "f{} {tag:?}: post-IS e_r", pair.frame);
        assert_eq!(replay.t_r, cap.t_r, "f{} {tag:?}: post-IS t_r", pair.frame);
        assert_eq!(
            replay.t_ld_r, cap.tld_r,
            "f{} {tag:?}: post-IS t_ld_r",
            pair.frame
        );
        assert_eq!(
            replay.spread_r, cap.spread_r,
            "f{} {tag:?}: post-IS spread_r",
            pair.frame
        );
        assert_eq!(
            replay.e_ld_r, cap.eld_r,
            "f{} {tag:?}: post-IS e_ld_r",
            pair.frame
        );
        // Post-IS spectra: production only rewrites coded bands (the IS
        // sum/difference substitution); uncoded bands must be bit-identical
        // to the cloned inputs. This also consumes the replayed spectra as
        // validated per-band outputs (§3.1 "all relevant per-band outputs").
        for b in 0..op.cnt {
            let start = op.offsets[b] as usize;
            let end = op.offsets[b + 1] as usize;
            if replay.is_book[b] == 0 {
                assert_eq!(
                    replay.spec_l[start..end],
                    op.spec_l[start..end],
                    "f{} {tag:?} band {b}: uncoded left spectra unchanged",
                    pair.frame
                );
                assert_eq!(
                    replay.spec_r[start..end],
                    op.spec_r[start..end],
                    "f{} {tag:?} band {b}: uncoded right spectra unchanged",
                    pair.frame
                );
            }
        }
    }
}

/// Gate 2: replay the production `ms_stereo_mirror` on cloned capture
/// operands and require exact mask/digest agreement, then recompute the
/// first-pass margin with exact integer semantics and require it to
/// reproduce the recorded pre-upgrade mask.
fn validate_ms_replay(pair: &FramePair) {
    for (tag, snap) in [
        (EngineTag::Q31, &pair.q_snap),
        (EngineTag::Fpe, &pair.f_snap),
    ] {
        let cap = snap
            .ms_cap
            .as_ref()
            .unwrap_or_else(|| panic!("f{} {tag:?}: stereo frame without MS capture", pair.frame));
        // Exact-integer first-pass transcription must reproduce the
        // recorded mask on every non-IS, non-upgraded band.
        let (margins, first) = ms_first_pass_margins(
            &cap.eld_l,
            &cap.eld_r,
            &cap.tld_l,
            &cap.tld_r,
            &cap.em_ld,
            &cap.es_ld,
            cap.cnt,
            cap.per,
            cap.max,
            cap.allow_ms,
        );
        assert_eq!(
            margins.len(),
            cap.cnt,
            "f{} {tag:?}: margin span",
            pair.frame
        );
        // Full-mirror replay with the recorded initial mask must be exact.
        let mut e_l2 = cap.e_l.clone();
        let mut e_r2 = cap.e_r.clone();
        let mut t_l2 = cap.t_l.clone();
        let mut t_r2 = cap.t_r.clone();
        let mut eld_l2 = cap.eld_l.clone();
        let mut eld_r2 = cap.eld_r.clone();
        let mut tld_l2 = cap.tld_l.clone();
        let mut tld_r2 = cap.tld_r.clone();
        let mut spread_l2 = cap.spread_l.clone();
        let mut spread_r2 = cap.spread_r.clone();
        let mut spec_l2 = vec![0i32; 1024];
        let mut spec_r2 = vec![0i32; 1024];
        // Recover the initial IS phase mask: the recorded final mask with
        // MS-applied bands cleared is not directly observable, so replay
        // from the snapshot IS diagnostics: phase mask == is_mask_values.
        let init = snap.is_mask_values.clone();
        assert_eq!(
            init.len(),
            cap.cnt,
            "f{} {tag:?}: init mask span",
            pair.frame
        );
        let (d2, m2) = ms_stereo_mirror(
            &mut e_l2,
            &mut e_r2,
            &mut t_l2,
            &mut t_r2,
            &mut eld_l2,
            &mut eld_r2,
            &mut tld_l2,
            &mut tld_r2,
            &cap.em,
            &cap.es,
            &cap.em_ld,
            &cap.es_ld,
            &mut spread_l2,
            &mut spread_r2,
            &mut spec_l2,
            &mut spec_r2,
            &cap.offsets,
            cap.cnt,
            cap.per,
            cap.max,
            cap.allow_ms,
            &cap.is_book,
            &init,
        );
        assert_eq!(d2, cap.digest, "f{} {tag:?}: MS digest replay", pair.frame);
        assert_eq!(m2, cap.mask, "f{} {tag:?}: MS mask replay", pair.frame);
        // First-pass transcription agrees with the replay on non-IS bands
        // before the ALL upgrade; where they differ, the ALL rule must be
        // the documented cause (digest ALL with IS-free geometry).
        for b in 0..cap.cnt {
            if cap.is_book.get(b).copied().unwrap_or(0) != 0 {
                continue;
            }
            // Unvisited tail bands keep zero on both paths (all loops share
            // the max-bounded walk).
            let visited_ms = (b % cap.per.max(1)) < cap.max;
            if !visited_ms {
                assert_eq!(
                    m2[b], 0,
                    "f{} {tag:?} band {b}: unvisited stays 0",
                    pair.frame
                );
                continue;
            }
            if d2 == 2 {
                // ALL upgrade converts every remaining non-IS band.
                assert_eq!(m2[b], 1, "f{} {tag:?} band {b}: ALL upgrade", pair.frame);
            } else {
                assert_eq!(
                    first[b], m2[b],
                    "f{} {tag:?} band {b}: first-pass transcription (no ALL)",
                    pair.frame
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Per-band stereo coding state, digest classification, ADTS validity
// ---------------------------------------------------------------------------

/// Ground-truth per-band stereo coding (separate from element digests and
/// encoded side info): IS book, IS phase bit, IS scale, and the MS mask for
/// non-IS bands. On IS bands the final mask preserves the IS phase bits by
/// C design, so the phase entry reads the final mask directly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct CodingBand {
    book: i32,
    phase: i32,
    scale: i32,
    ms: i32,
}

fn coding_state(book: &[i32], final_mask: &[i32], scale: &[i32]) -> Vec<CodingBand> {
    book.iter()
        .enumerate()
        .map(|(b, &bk)| {
            if bk != 0 {
                CodingBand {
                    book: bk,
                    phase: final_mask[b],
                    scale: scale[b],
                    ms: 0,
                }
            } else {
                CodingBand {
                    book: 0,
                    phase: 0,
                    scale: 0,
                    ms: final_mask[b],
                }
            }
        })
        .collect()
}

/// Digest classification preserving NONE/SOME/ALL semantics and the IS-band
/// rule (any IS band prohibits MS_ALL by C design: `num_false = 9`).
fn assert_digest_consistent(snap: &FrameSnapshot, cap: &MsDecisionCapture, what: &str) {
    let digest = snap.digest;
    assert!((0..=2).contains(&digest), "{what}: digest range");
    let any_is = snap.is_bands > 0;
    let ones = snap.mask_ones;
    match digest {
        0 => {
            assert_eq!(ones, 0, "{what}: NONE with mask ones");
            assert!(!any_is, "{what}: NONE with IS bands");
        }
        1 => {
            assert!(
                ones > 0 || any_is,
                "{what}: SOME with no evidence (mask {ones}, is {any_is})"
            );
        }
        2 => {
            assert!(
                !any_is,
                "{what}: ALL with IS bands (C prohibits MS_ALL there)"
            );
            assert!(ones > 0, "{what}: ALL with empty mask");
        }
        _ => unreachable!(),
    }
    assert_eq!(cap.digest, digest, "{what}: cap digest agrees");
    assert_eq!(
        cap.mask.iter().map(|&v| v as u32).sum::<u32>(),
        ones,
        "{what}: cap mask popcount agrees"
    );
}

/// Independent ADTS header validation of one emitted frame (structural
/// validity only, mirroring the production header contract).
fn assert_adts_valid(bytes: &[u8], sample_rate: u32, channels: usize, what: &str) {
    assert!(bytes.len() >= 7, "{what}: frame too short");
    let want_fi: u8 = match sample_rate {
        48000 => 3,
        44100 => 4,
        _ => panic!("{what}: unexpected rate {sample_rate}"),
    };
    assert_eq!(bytes[0], 0xFF, "{what}: syncword high");
    assert_eq!(bytes[1], 0xF1, "{what}: MPEG-4 / layer 0 / no CRC");
    assert_eq!((bytes[2] >> 6) & 0x3, 1, "{what}: AAC-LC profile");
    assert_eq!((bytes[2] >> 2) & 0xF, want_fi, "{what}: sampling freq idx");
    let cc = ((bytes[2] & 0x1) << 2) | ((bytes[3] >> 6) & 0x3);
    assert_eq!(cc, channels as u8, "{what}: channel config");
    let fl = ((bytes[3] as usize & 0x03) << 11)
        | ((bytes[4] as usize) << 3)
        | ((bytes[5] as usize) >> 5);
    assert_eq!(fl, bytes.len(), "{what}: frame_length counts header");
    let full = ((bytes[5] as u16 & 0x1F) << 6) | ((bytes[6] as u16) >> 2);
    assert_eq!(full, 0x7FF, "{what}: VBR fullness");
    assert_eq!(bytes[6] & 0x3, 0, "{what}: single raw block");
}

// ---------------------------------------------------------------------------
// Deterministic stereo probes (test-side, mirrors probe_pcm coverage goals)
// ---------------------------------------------------------------------------

/// Panned/out-of-phase/centred sine probe with an optional onset: silence
/// before `onset_frame`, then `amp * sin(2π f t)` on the left and
/// `gain_r * amp * sin(2π f t + phase_r)` on the right. Truncating `as`
/// cast, same convention as `testdata` resynthesis.
///
/// `pub(super)` for G-1 probe-corpus reuse (`probe_envelope`): the same
/// generator function feeds both harnesses, so the signals cannot diverge.
pub(super) fn probe_sine(
    frames: u32,
    sample_rate: f64,
    freq: f64,
    amp: f64,
    gain_r: f64,
    phase_r: f64,
    onset_frame: Option<u32>,
) -> Vec<Vec<i16>> {
    let n = frames as usize * FRAME_LENGTH;
    let mut l = Vec::with_capacity(n);
    let mut r = Vec::with_capacity(n);
    for t in 0..n {
        let on = onset_frame.is_none_or(|o| t >= o as usize * FRAME_LENGTH);
        let ph = 2.0 * std::f64::consts::PI * freq * t as f64 / sample_rate;
        let lv = if on { amp * ph.sin() } else { 0.0 };
        let rv = if on {
            gain_r * amp * (ph + phase_r).sin()
        } else {
            0.0
        };
        l.push(lv.clamp(-32768.0, 32767.0) as i16);
        r.push(rv.clamp(-32768.0, 32767.0) as i16);
    }
    vec![l, r]
}

/// Narrow three-tone stack probe (correlated, in-phase, panned).
///
/// `pub(super)` for G-1 probe-corpus reuse (see `probe_sine`).
pub(super) fn probe_narrow(frames: u32, sample_rate: f64, gain_r: f64) -> Vec<Vec<i16>> {
    let n = frames as usize * FRAME_LENGTH;
    let mut l = Vec::with_capacity(n);
    let mut r = Vec::with_capacity(n);
    for t in 0..n {
        let mut v = 0.0;
        for (i, f) in [5500.0, 6000.0, 6500.0].iter().enumerate() {
            let ph = 2.0 * std::f64::consts::PI * f * t as f64 / sample_rate + i as f64 * 0.371;
            v += ph.sin() / 3.0;
        }
        l.push((8000.0 * v).clamp(-32768.0, 32767.0) as i16);
        r.push((8000.0 * gain_r * v).clamp(-32768.0, 32767.0) as i16);
    }
    vec![l, r]
}

// ---------------------------------------------------------------------------
// Per-frame analysis, corpus runner, acceptance gates
// ---------------------------------------------------------------------------

use std::collections::{HashMap, HashSet};

/// Per-frame analysis outcome.
struct FrameAnalysis {
    coding_diffs: Vec<usize>,
    digest_diff: bool,
    findings: Vec<Finding>,
    evidences: Vec<FindingEvidence>,
}

/// Build the retained per-finding evidence for one frame's findings.
/// Test-only recording: reads `analyze_frame` locals, writes nothing to
/// production state and changes no predicate. Stateful accumulator
/// margins (I4/I6/I7, I5 carry) are carried by the retained trace events,
/// not re-derived here; stateless margins are recomputed exactly.
#[allow(clippy::too_many_arguments)]
fn build_frame_evidences(
    material: &'static str,
    material_idx: usize,
    pair: &FramePair,
    q_stage: &IsStage,
    f_stage: &IsStage,
    q_mid: &[i32],
    f_mid: &[i32],
    q_replay: &IsReplayOut,
    f_replay: &IsReplayOut,
    q_m: &[i64],
    f_m: &[i64],
    q_events: &[TraceEvent],
    f_events: &[TraceEvent],
    findings: &[Finding],
) -> Vec<FindingEvidence> {
    let q_cap_opt = pair.q_snap.ms_cap.as_ref();
    let f_cap_opt = pair.f_snap.ms_cap.as_ref();
    let q_trace: Vec<String> = q_events.iter().map(|e| format!("{e:?}")).collect();
    let f_trace: Vec<String> = f_events.iter().map(|e| format!("{e:?}")).collect();
    let mut out = Vec::with_capacity(findings.len());
    for finding in findings {
        if finding.band < 0 {
            // Digest-level finding: no per-band operands exist. Record the
            // absence explicitly rather than inventing band-0 values.
            out.push(FindingEvidence {
                material,
                material_idx,
                frame: finding.frame,
                band: -1,
                cause: cause_str(finding.cause),
                status: status_str(finding.status),
                consequence: consequence_str(finding.consequence),
                detail: finding.detail.clone(),
                review_gated: finding.consequence >= Consequence::P,
                unsupported: finding.status == ValStatus::DomainUnsupported,
                q_e_l: None,
                q_e_r: None,
                f_e_l: None,
                f_e_r: None,
                q_eld_l: None,
                q_eld_r: None,
                f_eld_l: None,
                f_eld_r: None,
                q_t_l: None,
                q_t_r: None,
                f_t_l: None,
                f_t_r: None,
                q_real: None,
                f_real: None,
                q_scale: None,
                f_scale: None,
                q_book: None,
                f_book: None,
                q_mid: None,
                f_mid: None,
                q_pre: None,
                f_pre: None,
                q_i1: None,
                f_i1: None,
                q_i2a: None,
                q_i2b: None,
                f_i2a: None,
                f_i2b: None,
                q_i3: None,
                f_i3: None,
                q_i8a: None,
                q_i8b: None,
                f_i8a: None,
                f_i8b: None,
                q_i10a: None,
                q_i10b: None,
                f_i10a: None,
                f_i10b: None,
                ms_supported: None,
                q_m: None,
                f_m: None,
                b_closed: None,
                use_q: None,
                use_f: None,
                d_el: None,
                d_er: None,
                d_em: None,
                d_es: None,
                d_tl: None,
                d_tr: None,
                q_trace: q_trace.clone(),
                f_trace: f_trace.clone(),
                spec_off: None,
                spec_end: None,
                q_spec_l: None,
                q_spec_r: None,
                f_spec_l: None,
                f_spec_r: None,
            });
            continue;
        }
        let b = finding.band as usize;
        let in_is_span = b < pair.q_op.cnt
            && b < pair.f_op.cnt
            && b < q_stage.hrr_err.len()
            && b < f_stage.hrr_err.len()
            && (b + 1) < pair.q_op.offsets.len()
            && (b + 1) < pair.f_op.offsets.len();
        // IS operands (None when the band is outside the captured span).
        let (q_e_l, q_e_r, f_e_l, f_e_r) = if in_is_span {
            (
                Some(pair.q_op.e_l[b]),
                Some(pair.q_op.e_r[b]),
                Some(pair.f_op.e_l[b]),
                Some(pair.f_op.e_r[b]),
            )
        } else {
            (None, None, None, None)
        };
        let (q_eld_l, q_eld_r, f_eld_l, f_eld_r) = if in_is_span {
            (
                Some(pair.q_op.e_ld_l[b]),
                Some(pair.q_op.e_ld_r[b]),
                Some(pair.f_op.e_ld_l[b]),
                Some(pair.f_op.e_ld_r[b]),
            )
        } else {
            (None, None, None, None)
        };
        let (q_t_l, q_t_r, f_t_l, f_t_r) = if in_is_span {
            (
                Some(pair.q_op.t_l[b]),
                Some(pair.q_op.t_r[b]),
                Some(pair.f_op.t_l[b]),
                Some(pair.f_op.t_r[b]),
            )
        } else {
            (None, None, None, None)
        };
        let (q_real, f_real) = if in_is_span {
            (Some(q_stage.real_scale[b]), Some(f_stage.real_scale[b]))
        } else {
            (None, None)
        };
        let (q_scale, f_scale) = if b < q_replay.is_scale.len() && b < f_replay.is_scale.len() {
            (Some(q_replay.is_scale[b]), Some(f_replay.is_scale[b]))
        } else {
            (None, None)
        };
        let (q_book, f_book) = if b < q_replay.is_book.len() && b < f_replay.is_book.len() {
            (Some(q_replay.is_book[b]), Some(f_replay.is_book[b]))
        } else {
            (None, None)
        };
        let (q_mid_v, f_mid_v) = if b < q_mid.len() && b < f_mid.len() {
            (Some(q_mid[b]), Some(f_mid[b]))
        } else {
            (None, None)
        };
        let (q_pre, f_pre) = if in_is_span {
            (Some(q_stage.is_mask_pre[b]), Some(f_stage.is_mask_pre[b]))
        } else {
            (None, None)
        };
        // Stateless IS margins (exact recomputation with trace helpers).
        let (q_i1, f_i1) = if in_is_span {
            let q_corr = sfb_correlation(
                &pair.q_op.spec_l,
                &pair.q_op.spec_r,
                pair.q_op.offsets[b] as usize,
                pair.q_op.offsets[b + 1] as usize,
            );
            let f_corr = sfb_correlation(
                &pair.f_op.spec_l,
                &pair.f_op.spec_r,
                pair.f_op.offsets[b] as usize,
                pair.f_op.offsets[b + 1] as usize,
            );
            (Some(i1_margin(q_corr)), Some(i1_margin(f_corr)))
        } else {
            (None, None)
        };
        let (q_i2a, q_i2b, f_i2a, f_i2b) = match (q_e_l, q_e_r, f_e_l, f_e_r) {
            (Some(ql), Some(qr), Some(fl), Some(fr)) => {
                let (a, c) = i2_margins(ql, qr);
                let (e, g) = i2_margins(fl, fr);
                (Some(a), Some(c), Some(e), Some(g))
            }
            _ => (None, None, None, None),
        };
        let (q_i3, f_i3) = if in_is_span {
            (
                Some(i3_margin(q_stage.hrr_err[b])),
                Some(i3_margin(f_stage.hrr_err[b])),
            )
        } else {
            (None, None)
        };
        let (q_i8a, q_i8b, f_i8a, f_i8b) =
            match (q_e_l, q_e_r, f_e_l, f_e_r, q_t_l, q_t_r, f_t_l, f_t_r) {
                (
                    Some(ql),
                    Some(qr),
                    Some(fl),
                    Some(fr),
                    Some(qtl),
                    Some(qtr),
                    Some(ftl),
                    Some(ftr),
                ) => {
                    let (a, c) = i8_margins(ql, qr, qtl, qtr);
                    let (e, g) = i8_margins(fl, fr, ftl, ftr);
                    (Some(a), Some(c), Some(e), Some(g))
                }
                _ => (None, None, None, None),
            };
        let (q_i10a, q_i10b, f_i10a, f_i10b) = match (q_eld_l, q_eld_r, f_eld_l, f_eld_r) {
            (Some(ql), Some(qr), Some(fl), Some(fr)) => {
                let (a, c) = i10_margins(ql.wrapping_sub(qr));
                let (e, g) = i10_margins(fl.wrapping_sub(fr));
                (Some(a), Some(c), Some(e), Some(g))
            }
            _ => (None, None, None, None),
        };
        // MS operands and first-pass margins.
        let (ms_supported, q_mv, f_mv, bnd, use_q, use_f, d_el, d_er, d_em, d_es, d_tl, d_tr) =
            match (q_cap_opt, f_cap_opt) {
                (Some(q_cap), Some(f_cap)) if b < q_cap.cnt && b < f_cap.cnt => {
                    let supported = ms_band_supported(b, q_cap, f_cap);
                    let d_el_v = (q_cap.eld_l[b] as i64 - f_cap.eld_l[b] as i64).abs();
                    let d_er_v = (q_cap.eld_r[b] as i64 - f_cap.eld_r[b] as i64).abs();
                    let d_em_v = (q_cap.em_ld[b] as i64 - f_cap.em_ld[b] as i64).abs();
                    let d_es_v = (q_cap.es_ld[b] as i64 - f_cap.es_ld[b] as i64).abs();
                    let d_tl_v = (q_cap.tld_l[b] as i64 - f_cap.tld_l[b] as i64).abs();
                    let d_tr_v = (q_cap.tld_r[b] as i64 - f_cap.tld_r[b] as i64).abs();
                    let bound = b_closed(d_el_v, d_er_v, d_em_v, d_es_v, d_tl_v, d_tr_v);
                    let qm = if b < q_m.len() { Some(q_m[b]) } else { None };
                    let fm = if b < f_m.len() { Some(f_m[b]) } else { None };
                    let (uq, uf) = match (qm, fm) {
                        (Some(qm_v), Some(fm_v)) => (
                            Some(i32::from(q_cap.allow_ms && qm_v >= 0)),
                            Some(i32::from(f_cap.allow_ms && fm_v >= 0)),
                        ),
                        _ => (None, None),
                    };
                    (
                        Some(supported),
                        qm,
                        fm,
                        Some(bound),
                        uq,
                        uf,
                        Some(d_el_v),
                        Some(d_er_v),
                        Some(d_em_v),
                        Some(d_es_v),
                        Some(d_tl_v),
                        Some(d_tr_v),
                    )
                }
                _ => (
                    None, None, None, None, None, None, None, None, None, None, None, None,
                ),
            };
        // Class B line vectors: Q31 spectral lines for the band window.
        let (spec_off, spec_end, q_spec_l, q_spec_r, f_spec_l, f_spec_r) = if in_is_span {
            let off = pair.q_op.offsets[b] as usize;
            let end = pair.q_op.offsets[b + 1] as usize;
            let f_off = pair.f_op.offsets[b] as usize;
            let f_end = pair.f_op.offsets[b + 1] as usize;
            // Both arms share geometry by gate 6; record the Q31 window and
            // each arm's lines separately. Out-of-range windows are recorded
            // as None (never sliced unsafely).
            if end <= pair.q_op.spec_l.len()
                && end <= pair.q_op.spec_r.len()
                && f_end <= pair.f_op.spec_l.len()
                && f_end <= pair.f_op.spec_r.len()
                && off <= end
                && f_off <= f_end
            {
                (
                    Some(off),
                    Some(end),
                    Some(pair.q_op.spec_l[off..end].to_vec()),
                    Some(pair.q_op.spec_r[off..end].to_vec()),
                    Some(pair.f_op.spec_l[f_off..f_end].to_vec()),
                    Some(pair.f_op.spec_r[f_off..f_end].to_vec()),
                )
            } else {
                (None, None, None, None, None, None)
            }
        } else {
            (None, None, None, None, None, None)
        };
        out.push(FindingEvidence {
            material,
            material_idx,
            frame: finding.frame,
            band: finding.band,
            cause: cause_str(finding.cause),
            status: status_str(finding.status),
            consequence: consequence_str(finding.consequence),
            detail: finding.detail.clone(),
            review_gated: finding.consequence >= Consequence::P,
            unsupported: finding.status == ValStatus::DomainUnsupported,
            q_e_l,
            q_e_r,
            f_e_l,
            f_e_r,
            q_eld_l,
            q_eld_r,
            f_eld_l,
            f_eld_r,
            q_t_l,
            q_t_r,
            f_t_l,
            f_t_r,
            q_real,
            f_real,
            q_scale,
            f_scale,
            q_book,
            f_book,
            q_mid: q_mid_v,
            f_mid: f_mid_v,
            q_pre,
            f_pre,
            q_i1,
            f_i1,
            q_i2a,
            q_i2b,
            f_i2a,
            f_i2b,
            q_i3,
            f_i3,
            q_i8a,
            q_i8b,
            f_i8a,
            f_i8b,
            q_i10a,
            q_i10b,
            f_i10a,
            f_i10b,
            ms_supported,
            q_m: q_mv,
            f_m: f_mv,
            b_closed: bnd,
            use_q,
            use_f,
            d_el,
            d_er,
            d_em,
            d_es,
            d_tl,
            d_tr,
            q_trace: q_trace.clone(),
            f_trace: f_trace.clone(),
            spec_off,
            spec_end,
            q_spec_l,
            q_spec_r,
            f_spec_l,
            f_spec_r,
        });
    }
    out
}

/// Deterministic ordering for retained evidence: by material corpus order,
/// then frame, then band, then detail. Never by hash-map order.
fn sorted_evidences(evidences: &[FindingEvidence]) -> Vec<&FindingEvidence> {
    let mut v: Vec<&FindingEvidence> = evidences.iter().collect();
    v.sort_by(|a, b| {
        (a.material_idx, a.frame, a.band, &a.detail).cmp(&(
            b.material_idx,
            b.frame,
            b.band,
            &b.detail,
        ))
    });
    v
}

/// Deterministic unique identifiers for a sorted evidence view.
///
/// Returns one `unique_id` per input in the same order: the preserved
/// `stable_id` plus `#n`, where `n` is the 1-based occurrence ordinal of that
/// stable label among the preceding rows of the same sorted view. Because the
/// view is sorted by `(material_idx, frame, band, detail)`, the ordinals — and
/// hence the identifiers — are identical on every execution with the same
/// inputs. No map iteration participates (linear scan over the sorted view).
fn unique_evidence_ids(sorted: &[&FindingEvidence]) -> Vec<String> {
    let mut ids = Vec::with_capacity(sorted.len());
    for (i, ev) in sorted.iter().enumerate() {
        let sid = ev.stable_id();
        let mut ordinal = 1usize;
        for prev in sorted[..i].iter() {
            if prev.stable_id() == sid {
                ordinal += 1;
            }
        }
        ids.push(ev.unique_id(ordinal));
    }
    ids
}

/// Compare one stepped frame pair: validity, replays, traces, margins,
/// coding state, and digests. Invalid frames panic (fail the gate loudly);
/// bound violations panic (fail + investigate); everything else is recorded
/// as classified findings.
fn analyze_frame(
    pair: &FramePair,
    material: &'static str,
    material_idx: usize,
    sample_rate: u32,
    persist: &HashMap<usize, u32>,
) -> FrameAnalysis {
    let frame = pair.frame;
    let what = format!("{material} f{frame}");
    // --- Validity (gate: invalid fails, never classified) ---
    for (tag, bytes, snap) in [
        ("Q31", &pair.q_bytes, &pair.q_snap),
        ("FPE", &pair.f_bytes, &pair.f_snap),
    ] {
        let b = bytes
            .as_ref()
            .unwrap_or_else(|| panic!("{what} {tag}: no bytes (notes {:?})", snap.notes));
        assert!(
            snap.notes.is_empty(),
            "{what} {tag}: notes {:?}",
            snap.notes
        );
        assert_eq!(snap.crash, -1, "{what} {tag}: crash marker");
        assert_adts_valid(b, sample_rate, 2, &format!("{what} {tag}"));
    }
    assert!(pair.q_snap.ms_ran && pair.f_snap.ms_ran, "{what}: MS ran");
    let short = pair.q_snap.seq == 2;
    assert_eq!(
        pair.q_snap.seq, pair.f_snap.seq,
        "{what}: window sequence agrees"
    );

    // --- Gates 1-2: production-callsite replay parity ---
    validate_lockstep_replay(pair);
    validate_ms_replay(pair);

    // --- Gates 3-4: IS margins, stateful trace, carry origin ---
    let q_stage = replay_prepare(&pair.q_op, EngineTag::Q31);
    let f_stage = replay_prepare(&pair.f_op, EngineTag::Fpe);
    let q_mid = replay_finalize(&q_stage, &pair.q_op, EngineTag::Q31);
    let f_mid = replay_finalize(&f_stage, &pair.f_op, EngineTag::Fpe);
    assert_eq!(q_mid.len(), pair.q_op.cnt, "{what}: Q31 mid-mask span");
    assert_eq!(f_mid.len(), pair.f_op.cnt, "{what}: FPE mid-mask span");
    let q_replay = replay_intensity(&pair.q_op, EngineTag::Q31);
    let f_replay = replay_intensity(&pair.f_op, EngineTag::Fpe);
    let mut q_events = build_trace(
        frame,
        EngineTag::Q31,
        &pair.q_op,
        &q_stage,
        &q_mid,
        &q_replay,
    );
    let mut f_events = build_trace(
        frame,
        EngineTag::Fpe,
        &pair.f_op,
        &f_stage,
        &f_mid,
        &f_replay,
    );

    // --- Sentinel consistency (I2): visited dual-mono bands keep hrr == 1/8.
    // Only bands visited by the prepare loop (`(b % per) < max`) carry a
    // decision; unvisited tail bands keep the zero init by C design.
    let visited = |b: usize| b < pair.q_op.cnt && (b % pair.q_op.per.max(1)) < pair.q_op.max;
    for (op, stage, tag) in [(&pair.q_op, &q_stage, "Q31"), (&pair.f_op, &f_stage, "FPE")] {
        for b in 0..op.cnt {
            if !visited(b) {
                assert_eq!(
                    stage.hrr_err[b], 0,
                    "{what} {tag} band {b}: unvisited bands keep zero init"
                );
                assert_eq!(
                    stage.is_mask_pre[b], 0,
                    "{what} {tag} band {b}: unvisited bands never pre-admit"
                );
                continue;
            }
            let (m_a, m_b) = i2_margins(op.e_l[b], op.e_r[b]);
            if m_a >= 0 && m_b <= 0 {
                assert_eq!(
                    stage.hrr_err[b], EIGHTH,
                    "{what} {tag} band {b}: anti-centre sentinel"
                );
                assert_eq!(
                    stage.is_mask_pre[b], 0,
                    "{what} {tag} band {b}: sentinel never pre-admits"
                );
            }
        }
    }

    // --- Gate 5: MS margin + B_closed validation (supported domain) ---
    let q_cap = pair.q_snap.ms_cap.as_ref().expect("q ms cap");
    let f_cap = pair.f_snap.ms_cap.as_ref().expect("f ms cap");
    assert_eq!(
        (q_cap.cnt, q_cap.per, q_cap.max),
        (f_cap.cnt, f_cap.per, f_cap.max)
    );
    assert_eq!(q_cap.offsets, f_cap.offsets, "{what}: MS offsets agree");
    let (q_m, _) = ms_first_pass_margins(
        &q_cap.eld_l,
        &q_cap.eld_r,
        &q_cap.tld_l,
        &q_cap.tld_r,
        &q_cap.em_ld,
        &q_cap.es_ld,
        q_cap.cnt,
        q_cap.per,
        q_cap.max,
        q_cap.allow_ms,
    );
    let (f_m, _) = ms_first_pass_margins(
        &f_cap.eld_l,
        &f_cap.eld_r,
        &f_cap.tld_l,
        &f_cap.tld_r,
        &f_cap.em_ld,
        &f_cap.es_ld,
        f_cap.cnt,
        f_cap.per,
        f_cap.max,
        f_cap.allow_ms,
    );
    let mut findings = Vec::new();
    let mut ms_diff_bands = Vec::new();
    for b in 0..q_cap.cnt {
        let use_q = i32::from(q_cap.allow_ms && q_m[b] >= 0);
        let use_f = i32::from(f_cap.allow_ms && f_m[b] >= 0);
        // Transcription checks apply to visited bands only: both the
        // production mirror and the transcription walk `(0..cnt).step_by(per)`
        // with the `max`-bounded inner walk, so unvisited tail bands keep the
        // zero init on both sides.
        let visited_ms = (b % q_cap.per.max(1)) < q_cap.max;
        if visited_ms && q_cap.is_book.get(b).copied().unwrap_or(0) == 0 && q_cap.digest != 2 {
            assert_eq!(
                use_q, q_cap.mask[b],
                "{what} band {b}: Q31 first-pass transcription"
            );
        }
        if visited_ms && f_cap.is_book.get(b).copied().unwrap_or(0) == 0 && f_cap.digest != 2 {
            assert_eq!(
                use_f, f_cap.mask[b],
                "{what} band {b}: FPE first-pass transcription"
            );
        }
        if !ms_band_supported(b, q_cap, f_cap) {
            // Out-of-domain bands are excluded from B_closed validation but
            // never silently dropped: a mask divergence there is recorded
            // with DomainUnsupported status (reported explicitly, never
            // counted as bound-validated).
            if use_q != use_f {
                findings.push(Finding {
                    material,
                    frame,
                    band: b as i32,
                    cause: Cause::B,
                    status: ValStatus::DomainUnsupported,
                    consequence: Consequence::L,
                    detail: format!(
                        "MS flip on unsupported band (sentinel/IS) use {use_q}->{use_f}"
                    ),
                });
            }
            continue;
        }
        let (outcome, status, bound) =
            validate_ms_band(b, q_cap, f_cap, q_m[b], f_m[b], use_q, use_f);
        match outcome {
            MsBoundOutcome::Agree => {}
            MsBoundOutcome::BoundViolated => {
                panic!(
                    "{what} band {b}: B_closed violated (M_q={} M_f={} shift={} > B={}): \
                     failing validation status, investigate underlying operands",
                    q_m[b],
                    f_m[b],
                    (q_m[b] - f_m[b]).abs(),
                    bound
                );
            }
            MsBoundOutcome::CloseCall => {
                ms_diff_bands.push(b);
                findings.push(Finding {
                    material,
                    frame,
                    band: b as i32,
                    cause: Cause::B,
                    status,
                    consequence: Consequence::L,
                    detail: format!(
                        "MS flip M_q={} M_f={} B={bound} use {use_q}->{use_f}",
                        q_m[b], f_m[b]
                    ),
                });
            }
        }
    }

    // --- Gate 7: complete per-band stereo coding-state comparison ---
    // Phase entries read the final mask (which preserves IS phase bits by C
    // design); the snapshot `is_mask_values` phase vector is validated
    // separately by the gate-1 replay and must agree with the final mask on
    // every IS band (asserted below).
    for b in 0..q_cap.cnt {
        if pair.q_snap.is_book_values[b] != 0 {
            assert_eq!(
                q_cap.mask[b], pair.q_snap.is_mask_values[b],
                "{what} band {b}: Q31 final mask preserves IS phase"
            );
        }
        if pair.f_snap.is_book_values[b] != 0 {
            assert_eq!(
                f_cap.mask[b], pair.f_snap.is_mask_values[b],
                "{what} band {b}: FPE final mask preserves IS phase"
            );
        }
    }
    let q_coding = coding_state(
        &pair.q_snap.is_book_values,
        &q_cap.mask,
        &pair.q_snap.is_scale_values,
    );
    let f_coding = coding_state(
        &pair.f_snap.is_book_values,
        &f_cap.mask,
        &pair.f_snap.is_scale_values,
    );
    assert_eq!(q_coding.len(), f_coding.len(), "{what}: coding span");
    let mut coding_diffs: Vec<usize> = q_coding
        .iter()
        .zip(f_coding.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(b, _)| b)
        .collect();
    coding_diffs.sort_unstable();
    // First-divergence attribution: the first band in each group where the
    // mid masks or the coding states differ is the divergence root; later
    // bands in an affected group are propagated (`OkPropagated`) — no local
    // margin bound is claimed once structural assumptions no longer hold.
    let per = pair.q_op.per.max(1);
    let mut div_first: HashMap<usize, usize> = HashMap::new();
    for b in 0..q_mid.len() {
        if q_mid[b] != f_mid[b] {
            div_first.entry(b / per).or_insert(b);
        }
    }
    for &b in &coding_diffs {
        div_first.entry(b / per).or_insert(b);
    }
    if let Some(&b0) = div_first.values().min() {
        let ev = TraceEvent::FirstDivergence {
            band: b0,
            q_mid: q_mid[b0],
            f_mid: f_mid[b0],
        };
        q_events.push(ev.clone());
        f_events.push(ev);
    }
    for &b in &coding_diffs {
        let first = div_first.get(&(b / per)).copied().unwrap_or(b);
        let propagated = first != b;
        if propagated {
            let ev = TraceEvent::Propagated { band: b };
            q_events.push(ev.clone());
            f_events.push(ev);
        }
        let status = if propagated {
            ValStatus::OkPropagated
        } else {
            ValStatus::OutOfEnvelope
        };
        findings.push(Finding {
            material,
            frame,
            band: b as i32,
            cause: Cause::B,
            status,
            consequence: Consequence::L,
            detail: format!(
                "coding {:?}->{:?}{}",
                q_coding[b],
                f_coding[b],
                if propagated { " propagated" } else { "" }
            ),
        });
    }

    // --- Gate 8: digest classification (kept separate from coding state) ---
    assert_digest_consistent(&pair.q_snap, q_cap, &format!("{what} Q31"));
    assert_digest_consistent(&pair.f_snap, f_cap, &format!("{what} FPE"));
    let digest_diff =
        pair.q_snap.digest != pair.f_snap.digest || pair.q_snap.is_digest != pair.f_snap.is_digest;
    if digest_diff {
        // A digest difference with identical per-band coding state and
        // identical masks is impossible (both digests are deterministic
        // functions of those vectors): fail loudly rather than misreport a
        // digest-only change as a coding change (or vice versa).
        assert!(
            !coding_diffs.is_empty() || q_cap.mask != f_cap.mask,
            "{what}: digest differs with identical coding state and masks"
        );
        findings.push(Finding {
            material,
            frame,
            band: -1,
            cause: Cause::B,
            status: ValStatus::OutOfEnvelope,
            consequence: Consequence::F,
            detail: format!(
                "digest Q({}/{}) vs F({}/{}) valid+decodable",
                pair.q_snap.is_digest,
                pair.q_snap.digest,
                pair.f_snap.is_digest,
                pair.f_snap.digest
            ),
        });
    }

    // --- Consequence escalation: persistence and grouped context ---
    for finding in findings.iter_mut() {
        if finding.band < 0 {
            continue;
        }
        let b = finding.band as usize;
        // Persistent cascade: the band already differed on the two preceding
        // frames (≥3-frame cascade including this frame).
        if persist.get(&b).copied().unwrap_or(0) >= 2 {
            finding.consequence = finding.consequence.max(Consequence::P);
        }
        if short {
            finding.consequence = finding.consequence.max(Consequence::G);
        }
        if digest_diff {
            finding.consequence = finding.consequence.max(Consequence::F);
        }
    }

    // --- Routing: every finding must be acceptable or review-gated ---
    for finding in &findings {
        assert!(
            route(finding.cause, finding.status, finding.consequence, true),
            "{what} band {}: unroutable {:?}/{:?}/{:?} ({})",
            finding.band,
            finding.cause,
            finding.status,
            finding.consequence,
            finding.detail
        );
    }
    let _ = ms_diff_bands.len();
    let evidences = build_frame_evidences(
        material,
        material_idx,
        pair,
        &q_stage,
        &f_stage,
        &q_mid,
        &f_mid,
        &q_replay,
        &f_replay,
        &q_m,
        &f_m,
        &q_events,
        &f_events,
        &findings,
    );
    FrameAnalysis {
        coding_diffs,
        digest_diff,
        findings,
        evidences,
    }
}

// ---------------------------------------------------------------------------
// Corpus runner (gates 1/2/5/6/7/8/10)
// ---------------------------------------------------------------------------

use std::sync::OnceLock;

struct Material {
    name: &'static str,
    pcm: Vec<Vec<i16>>,
    sample_rate: u32,
    frames: u32,
}

fn fixture_material(name: &'static str, cap_frames: u32) -> Material {
    let pcm = testdata::fixture_samples(name);
    let sample_rate = testdata::fixture_sample_rate(name);
    let full = (pcm[0].len().div_ceil(FRAME_LENGTH) + 2) as u32;
    Material {
        name,
        pcm,
        sample_rate,
        frames: full.min(cap_frames),
    }
}

fn probe_material(
    name: &'static str,
    pcm: Vec<Vec<i16>>,
    sample_rate: u32,
    frames: u32,
) -> Material {
    Material {
        name,
        pcm,
        sample_rate,
        frames,
    }
}

fn materials() -> Vec<Material> {
    let mut out = vec![
        fixture_material("sine-stereo", 49),
        fixture_material("onset-stereo", 49),
        fixture_material("stepdown-stereo", 49),
        fixture_material("multitone-stereo", 32),
        fixture_material("tonal-noise-stereo", 26),
        fixture_material("quiet-sine-stereo", 32),
        fixture_material("sine-stereo-44100", 32),
        fixture_material("onset-stereo-44100", 32),
    ];
    // Silence sentinel (short run: the frame is stationary).
    {
        let pcm = testdata::fixture_samples("silence-stereo");
        out.push(probe_material("silence-stereo", pcm, 48000, 8));
    }
    // Panned in-phase probe (IS correlation path).
    out.push(probe_material(
        "probe-panned-48k",
        probe_sine(12, 48000.0, 1000.0, 10_000.0, 0.25, 0.0, Some(4)),
        48000,
        12,
    ));
    // Out-of-phase probe (IS phase-bit path).
    out.push(probe_material(
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
        12,
    ));
    // Centred dual-mono probe (anti-centre sentinel path).
    out.push(probe_material(
        "probe-centred-48k",
        probe_sine(12, 48000.0, 1000.0, 10_000.0, 1.0, 0.0, Some(4)),
        48000,
        12,
    ));
    // Narrow correlated stack (retention-count path).
    out.push(probe_material(
        "probe-narrow-48k",
        probe_narrow(12, 48000.0, 0.25),
        48000,
        12,
    ));
    // Quiet probe (low-energy/sentinel path).
    out.push(probe_material(
        "probe-quiet-48k",
        probe_sine(12, 48000.0, 1000.0, 50.0, 0.25, 0.0, Some(4)),
        48000,
        12,
    ));
    // Second sample rate with transient (IS block clearing + MS_ALL).
    out.push(probe_material(
        "probe-panned-44100",
        probe_sine(12, 44100.0, 1000.0, 10_000.0, 0.25, 0.0, Some(4)),
        44100,
        12,
    ));
    out
}

#[derive(Debug, Clone, Default)]
struct CorpusResult {
    frames: usize,
    agree_frames: usize,
    findings: Vec<Finding>,
    evidences: Vec<FindingEvidence>,
    saw_short: bool,
    saw_long: bool,
    saw_start: bool,
    saw_stop: bool,
    short_frames: usize,
    digest_diff_frames: Vec<u32>,
    coding_diff_frames: Vec<u32>,
    unsupported_band_frames: usize,
    some_frames: usize,
    none_frames: usize,
}

fn run_corpus() -> CorpusResult {
    let mut res = CorpusResult::default();
    for (material_idx, m) in materials().into_iter().enumerate() {
        let mut h = R21BHarness::new(m.pcm, m.sample_rate);
        let mut persist: HashMap<usize, u32> = HashMap::new();
        let mut prev_geo = (0usize, 0usize);
        for f in 0..m.frames {
            let pair = h.step(f);
            match pair.q_snap.seq {
                0 => res.saw_long = true,
                1 => res.saw_start = true,
                2 => {
                    res.saw_short = true;
                    res.short_frames += 1;
                }
                3 => res.saw_stop = true,
                _ => {}
            }
            let analysis = analyze_frame(&pair, m.name, material_idx, m.sample_rate, &persist);
            if analysis.findings.is_empty() {
                res.agree_frames += 1;
            }
            if analysis.digest_diff {
                res.digest_diff_frames.push(f);
            }
            if !analysis.coding_diffs.is_empty() {
                res.coding_diff_frames.push(f);
            }
            for finding in &analysis.findings {
                if finding.status == ValStatus::DomainUnsupported {
                    res.unsupported_band_frames += 1;
                    break;
                }
            }
            match pair.q_snap.digest {
                0 => res.none_frames += 1,
                1 => res.some_frames += 1,
                _ => {}
            }
            res.findings.extend(analysis.findings);
            res.evidences.extend(analysis.evidences);
            res.frames += 1;
            // Persistence accounting for the next frame (reset on geometry
            // change: short/long cnt differs, band indices are incomparable).
            let geo = (pair.q_op.cnt, pair.q_op.per);
            if geo != prev_geo {
                persist.clear();
                prev_geo = geo;
            }
            let diffs: HashSet<usize> = analysis.coding_diffs.iter().copied().collect();
            persist.retain(|b, _| diffs.contains(b));
            for &b in &diffs {
                *persist.entry(b).or_insert(0) += 1;
            }
            // Every processed frame is recorded exactly once (ledger
            // discipline: no skips, no duplicates by construction of the
            // sequential loop; the count asserts completeness below).
        }
    }
    res
}

fn corpus() -> CorpusResult {
    static CORPUS: OnceLock<CorpusResult> = OnceLock::new();
    CORPUS.get_or_init(run_corpus).clone()
}

// ---------------------------------------------------------------------------
// Acceptance gates 1-12, ownership, Tier-B blocker
// ---------------------------------------------------------------------------

#[cfg(test)]
mod gate_tests {
    use super::*;

    fn summarize(c: &CorpusResult, gate: &str) {
        println!(
            "R21B {gate}: frames={} agree={} findings={} digest_diffs={} coding_frames={} \
             short={} start={} stop={} unsupported_frames={} some={} none={}",
            c.frames,
            c.agree_frames,
            c.findings.len(),
            c.digest_diff_frames.len(),
            c.coding_diff_frames.len(),
            c.short_frames,
            c.saw_start,
            c.saw_stop,
            c.unsupported_band_frames,
            c.some_frames,
            c.none_frames,
        );
    }

    #[test]
    fn gate01_is_capture_replay_parity() {
        // Production-callsite IS capture/replay parity on every frame of
        // every material (both engines). Any mismatch panics inside
        // `analyze_frame` with ReplayMismatch evidence.
        let c = corpus();
        assert!(c.frames > 40, "corpus too small: {}", c.frames);
        summarize(&c, "gate01");
    }

    #[test]
    fn gate02_ms_capture_replay_parity() {
        // Production-callsite MS capture/replay parity (mask + digest exact)
        // plus exact-integer first-pass transcription on every frame.
        let c = corpus();
        assert!(c.frames > 40, "corpus too small: {}", c.frames);
        summarize(&c, "gate02");
    }

    #[test]
    fn gate03_i_margins_validated_per_frame() {
        // I1–I10 margins recomputed per band per frame and validated against
        // production outputs (trace consistency asserts inside
        // `build_trace` + sentinel/real_scale/is_scale exactness).
        let c = corpus();
        assert!(c.findings.iter().all(|f| f.cause != Cause::Unclassified));
        summarize(&c, "gate03");
    }

    #[test]
    fn gate04_stateful_trace_and_carry_origin() {
        // The trace ran on every frame of both arms (any inconsistency
        // panics in `build_trace`). Additionally: every recorded direction
        // drop must carry an origin, and block clears must reset the carry.
        let c = corpus();
        assert!(c.frames > 40);
        // Conditional invariants over the corpus event stream are enforced
        // inside `build_trace` (origin Some on drops, rescan on clears).
        summarize(&c, "gate04");
    }

    #[test]
    fn gate05_ms_margin_and_b_closed() {
        // B_closed validated in its supported domain on every band of every
        // frame; violations panic in `analyze_frame`. Unsupported bands are
        // reported explicitly (never counted as validated).
        let c = corpus();
        assert!(
            !c.findings
                .iter()
                .any(|f| f.status == ValStatus::BoundFailure),
            "bound failures must fail the gate (see panic evidence)"
        );
        println!(
            "R21B gate05: supported-domain bands all satisfy |shift| <= B_closed; \
             unsupported-band frames={}",
            c.unsupported_band_frames
        );
        summarize(&c, "gate05");
    }

    #[test]
    fn gate06_engine_invariant_operands_bitexact() {
        // Representation geometry bit-exact between arms on every frame
        // (asserted in `R21BHarness::step`); offsets/cnt/per/max agree.
        let c = corpus();
        assert!(c.frames > 40);
        summarize(&c, "gate06");
    }

    #[test]
    fn gate07_complete_coding_state_comparison() {
        // Per-band coding state compared on every frame; digest-only changes
        // are never misreported as coding changes (impossibility asserted in
        // `analyze_frame`: a digest diff requires a coding or mask diff).
        let c = corpus();
        for &f in &c.digest_diff_frames {
            assert!(
                c.coding_diff_frames.contains(&f)
                    || c.findings.iter().any(|x| x.frame == f && x.band == -1),
                "digest frame {f} without classified evidence"
            );
        }
        summarize(&c, "gate07");
    }

    #[test]
    fn gate08_digest_classification() {
        // NONE/SOME/ALL rules hold on every frame of both arms; SOME occurs
        // in the corpus (MS actively decides), NONE occurs (silence).
        let c = corpus();
        assert!(c.some_frames > 0, "corpus never reaches MS_SOME");
        assert!(c.none_frames > 0, "corpus never reaches MS_NONE");
        summarize(&c, "gate08");
    }

    #[test]
    fn gate10_multiframe_live_validation() {
        // Multi-frame live runs with exact frame accounting and
        // short+long+start+stop window coverage.
        let c = corpus();
        let expected: usize = materials().iter().map(|m| m.frames as usize).sum();
        assert_eq!(c.frames, expected, "exact frame accounting");
        assert!(c.saw_short, "corpus must cover SHORT");
        assert!(c.saw_long, "corpus must cover LONG");
        assert!(c.saw_start, "corpus must cover START (onset material)");
        assert!(c.saw_stop, "corpus must cover STOP (onset material)");
        summarize(&c, "gate10");
    }

    #[test]
    fn gate11_consequence_routing_complete() {
        // Every divergence carries cause + status + consequence and routes;
        // cause C and unclassified never appear (they would fail in
        // `analyze_frame`); severity ordering holds by construction.
        let c = corpus();
        assert!(Consequence::N < Consequence::L);
        assert!(Consequence::L < Consequence::P);
        assert!(Consequence::P < Consequence::G);
        assert!(Consequence::G < Consequence::F);
        for f in &c.findings {
            assert_ne!(f.cause, Cause::C, "Cause C requires evidence");
            assert_ne!(f.cause, Cause::Unclassified, "unclassified divergence");
            assert!(
                route(f.cause, f.status, f.consequence, true),
                "unroutable finding {} f{} b{} ({:?}/{:?}/{:?})",
                f.material,
                f.frame,
                f.band,
                f.cause,
                f.status,
                f.consequence
            );
        }
        let review_gated = c
            .findings
            .iter()
            .filter(|f| f.consequence >= Consequence::P)
            .count();
        println!("R21B gate11: review-gated (P/G/F) findings={review_gated}");
        summarize(&c, "gate11");
    }

    #[test]
    fn gate12_q31_default_and_baseline_nonregression() {
        // Q31 remains the default engine on every constructor path, and the
        // Q31-only stream still emits valid frames (existing baseline hashes
        // are re-verified by the `production_baseline` suite in validation).
        for (sr, ch) in [(48000u32, 2usize), (48000, 1), (44100, 2)] {
            let enc = StreamEncoder::new(sr, ch);
            assert_eq!(enc.psy_engine(), PsyEngineKind::Q31, "new {sr}/{ch}");
            let enc = StreamEncoder::with_mode(sr, ch, VbrMode::Vbr5);
            assert_eq!(enc.psy_engine(), PsyEngineKind::Q31, "with_mode {sr}/{ch}");
        }
        let pcm = testdata::fixture_samples("sine-stereo");
        let mut enc = StreamEncoder::new(48000, 2);
        for f in 0..4u32 {
            let fresh: Vec<Vec<i16>> = pcm
                .iter()
                .map(|c| {
                    (0..FRAME_LENGTH)
                        .map(|i| *c.get(f as usize * FRAME_LENGTH + i).unwrap_or(&0))
                        .collect()
                })
                .collect();
            let (bytes, snap) = enc.step_frame(f, &fresh);
            let b = bytes.expect("Q31 frame emits");
            assert!(snap.notes.is_empty());
            assert_eq!(snap.crash, -1);
            assert_adts_valid(&b, 48000, 2, &format!("Q31 baseline f{f}"));
        }
        let c = corpus();
        summarize(&c, "gate12");
    }

    #[test]
    fn ownership_engine_selection_and_state_isolation() {
        // Separate Q31/FPE encoder instances and independent mutable buffers;
        // engine selection is exact on all four harness encoders.
        let pcm = probe_sine(4, 48000.0, 1000.0, 10_000.0, 0.25, 0.0, None);
        let mut h = R21BHarness::new(pcm, 48000);
        assert_eq!(h.q_full.psy_engine(), PsyEngineKind::Q31);
        assert_eq!(h.f_full.psy_engine(), PsyEngineKind::Fpe);
        assert_eq!(h.q_fe.psy_engine(), PsyEngineKind::Q31);
        assert_eq!(h.f_fe.psy_engine(), PsyEngineKind::Fpe);
        let pair = h.step(1);
        // Backing-allocation pointer inequality on the operand stores.
        pair.q_op.assert_disjoint_vs(&pair.f_op, pair.frame);
        let q_cap = pair.q_snap.ms_cap.as_ref().expect("q cap");
        let f_cap = pair.f_snap.ms_cap.as_ref().expect("f cap");
        assert_disjoint(&q_cap.e_l, &f_cap.e_l, "cap e_l");
        assert_disjoint(&q_cap.mask, &f_cap.mask, "cap mask");
        assert_disjoint(
            &pair.q_snap.is_book_values,
            &pair.f_snap.is_book_values,
            "is_book_values",
        );
    }

    #[test]
    fn ownership_canary_both_directions() {
        // Poison Q31 operands: FPE arm byte-identical to pre-poison snapshot.
        let pcm = probe_sine(4, 48000.0, 1000.0, 10_000.0, 0.25, 0.0, None);
        let mut h = R21BHarness::new(pcm, 48000);
        let mut pair = h.step(1);
        let f_snap: Vec<i32> = pair.f_op.e_l.clone();
        let q_snap: Vec<i32> = pair.q_op.e_l.clone();
        for v in pair.q_op.e_l.iter_mut() {
            *v = v.wrapping_add(0x3FFF_FFFF);
        }
        assert_ne!(pair.q_op.e_l, q_snap, "poison must take effect");
        assert_eq!(pair.f_op.e_l, f_snap, "FPE changed by Q31 poison");
        // Reverse direction.
        let mut pair = h.step(2);
        let q_snap2: Vec<i32> = pair.q_op.e_r.clone();
        let f_snap2: Vec<i32> = pair.f_op.e_r.clone();
        for v in pair.f_op.e_r.iter_mut() {
            *v = v.wrapping_add(0x3FFF_FFFF);
        }
        assert_ne!(pair.f_op.e_r, f_snap2, "poison must take effect");
        assert_eq!(pair.q_op.e_r, q_snap2, "Q31 changed by FPE poison");
    }

    #[test]
    fn ownership_crossed_inputs_rejected() {
        // Crossed engine tags must be rejected by the replay validators.
        let pcm = probe_sine(4, 48000.0, 1000.0, 10_000.0, 0.25, 0.0, None);
        let mut h = R21BHarness::new(pcm, 48000);
        let pair = h.step(1);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            replay_intensity(&pair.q_op, EngineTag::Fpe)
        }));
        assert!(r.is_err(), "crossed IS replay must fail");
        let stage = replay_prepare(&pair.q_op, EngineTag::Q31);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            replay_finalize(&stage, &pair.q_op, EngineTag::Fpe)
        }));
        assert!(r.is_err(), "crossed finalize replay must fail");
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pair.q_op.assert_disjoint_vs(&pair.q_op, pair.frame)
        }));
        assert!(r.is_err(), "self-pair must fail the tag check");
    }

    #[test]
    fn r21b_p0b_aggregates_are_pinned() {
        // P0-B B2: hard assertions for the five authoritative aggregate
        // counts. Sources (verified by P0-A §6.1 against gate output, the
        // owner-review ledger §1/§3, the implementation note §7, and the
        // owner decision §1/§5 register 5 accepted + 9 conditional = 14):
        // 30 findings / 16 coding frames / 0 digest diffs / 1 unsupported
        // frame = reproduced gate output + owner-review ledger + impl note;
        // 14 review-gated = reproduced gate11 output + owner decision §1/§5.
        // Corpus scope: materials() = 8 fixtures (49+49+49+32+26+32+32+32 =
        // 301) + silence (8) + 6 probes (12 each = 72) = 15 materials,
        // 381 frames (the "16 materials" wording in owner records is the
        // known A-22 error; 16 is the coding-difference frame count).
        let c = corpus();
        assert_eq!(c.frames, 381, "corpus frame accounting");
        assert_eq!(c.agree_frames, 365, "agreeing frames");
        assert_eq!(c.findings.len(), 30, "findings");
        assert_eq!(
            c.digest_diff_frames.len(),
            0,
            "digest differences (digest equality does not imply per-band equality)"
        );
        assert_eq!(c.coding_diff_frames.len(), 16, "coding-difference frames");
        let review_gated = c
            .findings
            .iter()
            .filter(|f| f.consequence >= Consequence::P)
            .count();
        assert_eq!(review_gated, 14, "review-gated (P/G/F) findings");
        assert_eq!(
            c.unsupported_band_frames, 1,
            "unsupported-domain frames (sentinel domain, never a pass)"
        );
        // Evidence parity: one retained record per finding.
        assert_eq!(
            c.evidences.len(),
            c.findings.len(),
            "one evidence record per finding"
        );
        println!(
            "R21B-P0B aggregates: findings=30 coding_frames=16 digest_diffs=0 review_gated=14 unsupported_frames=1"
        );
        summarize(&c, "p0b-aggregates");
    }

    #[test]
    fn r21b_p0b_per_finding_evidence_is_complete_and_traceable() {
        // P0-B B3/B4: every applicable finding retains IS operands, decision
        // margins, trace chains and Class B line vectors on the G-1 BandRow
        // pattern (retained rows, never re-derived decisions). A reviewer can
        // trace each finding from inputs through margins to the decision.
        let c = corpus();
        assert_eq!(c.evidences.len(), 30, "evidence population");
        let sorted = sorted_evidences(&c.evidences);
        // Deterministic ordering: sorted view must be monotonic in the full
        // (material_idx, frame, band, detail) tuple (detail is the documented
        // tie-break for F-01/F-02; P0C-N3 strengthening).
        let mut prev: Option<(usize, u32, i32, String)> = None;
        for ev in &sorted {
            let key = (ev.material_idx, ev.frame, ev.band, ev.detail.clone());
            if let Some(p) = &prev {
                assert!(key >= *p, "evidence ordering (material,frame,band,detail)");
            }
            prev = Some(key);
        }
        for ev in &sorted {
            // Identity: which finding, frame and channel context.
            assert!(!ev.material.is_empty(), "material present");
            assert!(ev.material_idx < 15, "material index in corpus");
            assert_eq!(
                ev.review_gated,
                ev.consequence == "P" || ev.consequence == "G" || ev.consequence == "F",
                "{}: review-gated flag",
                ev.stable_id()
            );
            assert_eq!(
                ev.unsupported,
                ev.status == "DomainUnsupported",
                "{}: unsupported flag",
                ev.stable_id()
            );
            if ev.band < 0 {
                // Digest-level findings carry no per-band operands; the
                // absence is explicit (this corpus has zero digest diffs, so
                // this path documents the representation without firing).
                assert!(
                    ev.q_e_l.is_none(),
                    "{}: digest has no IS operands",
                    ev.stable_id()
                );
                assert!(
                    ev.ms_supported.is_none(),
                    "{}: digest has no MS band",
                    ev.stable_id()
                );
                assert!(
                    ev.q_spec_l.is_none(),
                    "{}: digest has no line vectors",
                    ev.stable_id()
                );
                continue;
            }
            // IS operands present on both arms.
            assert!(
                ev.q_e_l.is_some() && ev.f_e_l.is_some(),
                "{}: IS e_l",
                ev.stable_id()
            );
            assert!(
                ev.q_e_r.is_some() && ev.f_e_r.is_some(),
                "{}: IS e_r",
                ev.stable_id()
            );
            assert!(
                ev.q_real.is_some() && ev.f_real.is_some(),
                "{}: real_scale",
                ev.stable_id()
            );
            assert!(
                ev.q_scale.is_some() && ev.f_scale.is_some(),
                "{}: is_scale",
                ev.stable_id()
            );
            assert!(
                ev.q_book.is_some() && ev.f_book.is_some(),
                "{}: is_book",
                ev.stable_id()
            );
            // Stateless decision margins recomputed exactly.
            assert!(
                ev.q_i1.is_some() && ev.f_i1.is_some(),
                "{}: I1",
                ev.stable_id()
            );
            assert!(
                ev.q_i2a.is_some()
                    && ev.q_i2b.is_some()
                    && ev.f_i2a.is_some()
                    && ev.f_i2b.is_some(),
                "{}: I2 dual",
                ev.stable_id()
            );
            assert!(
                ev.q_i3.is_some() && ev.f_i3.is_some(),
                "{}: I3",
                ev.stable_id()
            );
            assert!(
                ev.q_i8a.is_some()
                    && ev.q_i8b.is_some()
                    && ev.f_i8a.is_some()
                    && ev.f_i8b.is_some(),
                "{}: I8 dual",
                ev.stable_id()
            );
            assert!(
                ev.q_i10a.is_some()
                    && ev.q_i10b.is_some()
                    && ev.f_i10a.is_some()
                    && ev.f_i10b.is_some(),
                "{}: I10 dual",
                ev.stable_id()
            );
            // Stateful margins travel with the trace chains (I4/I6/I7, I5
            // carry): the trace must be retained and non-empty on both arms.
            assert!(!ev.q_trace.is_empty(), "{}: Q31 trace", ev.stable_id());
            assert!(!ev.f_trace.is_empty(), "{}: FPE trace", ev.stable_id());
            // MS operands and bound on in-span bands.
            assert!(
                ev.ms_supported.is_some(),
                "{}: MS domain flag",
                ev.stable_id()
            );
            assert!(
                ev.q_m.is_some() && ev.f_m.is_some(),
                "{}: MS margins",
                ev.stable_id()
            );
            assert!(ev.b_closed.is_some(), "{}: B_closed", ev.stable_id());
            assert!(
                ev.use_q.is_some() && ev.use_f.is_some(),
                "{}: MS use",
                ev.stable_id()
            );
            // Class B line vectors retained (mapped Q31 lines per band).
            assert!(
                ev.spec_off.is_some() && ev.spec_end.is_some(),
                "{}: spec window",
                ev.stable_id()
            );
            assert!(ev.q_spec_l.is_some(), "{}: Q31 left lines", ev.stable_id());
            assert!(ev.q_spec_r.is_some(), "{}: Q31 right lines", ev.stable_id());
            assert!(ev.f_spec_l.is_some(), "{}: FPE left lines", ev.stable_id());
            assert!(ev.f_spec_r.is_some(), "{}: FPE right lines", ev.stable_id());
            let (off, end) = (ev.spec_off.unwrap(), ev.spec_end.unwrap());
            assert!(off <= end, "{}: window order", ev.stable_id());
            assert_eq!(
                ev.q_spec_l.as_ref().unwrap().len(),
                end - off,
                "{}: Q31 left span",
                ev.stable_id()
            );
            assert_eq!(
                ev.f_spec_r.as_ref().unwrap().len(),
                end - off,
                "{}: FPE right span",
                ev.stable_id()
            );
            // Canonical serialization is stable (determinism check per row).
            let a = ev.canonical_string();
            let b = ev.canonical_string();
            assert_eq!(a, b, "{}: canonical determinism", ev.stable_id());
            println!(
                "R21B-FINDING {} cause={} status={} cons={} gated={} unsupported={} {}",
                ev.stable_id(),
                ev.cause,
                ev.status,
                ev.consequence,
                if ev.review_gated { 1 } else { 0 },
                if ev.unsupported { 1 } else { 0 },
                ev.detail,
            );
        }
        // No digest-level findings on this corpus (0 digest diffs); every
        // retained record is a per-band finding with line vectors.
        assert!(
            c.evidences.iter().all(|e| e.band >= 0),
            "all 30 findings are per-band (digest diffs are 0)"
        );
    }

    #[test]
    fn r21b_p0b_unsupported_frame_is_explicit() {
        // P0-B B4: the unsupported frame is explicit evidence, never a pass
        // and never silently omitted from aggregate accounting.
        let c = corpus();
        let unsupported: Vec<&FindingEvidence> = sorted_evidences(&c.evidences)
            .into_iter()
            .filter(|e| e.unsupported)
            .collect();
        assert_eq!(
            unsupported.len(),
            1,
            "exactly one unsupported finding (F-01)"
        );
        let ev = unsupported[0];
        assert_eq!(ev.material, "quiet-sine-stereo", "unsupported material");
        assert_eq!(ev.frame, 16, "unsupported frame");
        assert_eq!(ev.band, 13, "unsupported band");
        assert_eq!(ev.status, "DomainUnsupported");
        assert_eq!(ev.ms_supported, Some(false), "out of B_closed domain");
        // The sentinel lives in the MS energy-Ld operands (eld/em/es domain
        // rule, `ms_band_supported`): the IS stored energies retained above
        // are finite on this frame, while the MS arm carries the MIN
        // sentinel. The domain flag plus the retained MS deltas/bound carry
        // the evidence; no sentinel value is invented here.
        // The same frame carries the coding consequence (F-02): one frame
        // with two findings, counted once in unsupported frames but twice
        // in the finding population.
        let frame_peers = c
            .evidences
            .iter()
            .filter(|e| e.material == ev.material && e.frame == ev.frame)
            .count();
        assert_eq!(frame_peers, 2, "quiet f16 carries F-01 + F-02");
        assert_eq!(
            c.unsupported_band_frames, 1,
            "unsupported frames counted once"
        );
        println!(
            "R21B-UNSUPPORTED {} status={} ms_supported=0 peers_in_frame={frame_peers}",
            ev.stable_id(),
            ev.status,
        );
    }

    #[test]
    fn r21b_p0b_evidence_determinism_across_runs() {
        // P0-B §4.1: identical output across repeated executions on the same
        // inputs. Re-runs the full corpus (not the OnceLock cache) and
        // compares canonical serializations byte-for-byte.
        let a = run_corpus();
        let b = run_corpus();
        let sa: Vec<String> = sorted_evidences(&a.evidences)
            .into_iter()
            .map(|e| e.canonical_string())
            .collect();
        let sb: Vec<String> = sorted_evidences(&b.evidences)
            .into_iter()
            .map(|e| e.canonical_string())
            .collect();
        assert_eq!(sa.len(), sb.len(), "evidence population stable");
        assert_eq!(sa, sb, "canonical evidence identical across runs");
        println!("R21B-P0B determinism: {} evidence rows identical", sa.len());
    }

    fn p0b_fnv1a64(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    #[test]
    fn r21b_p0b_byte_baseline_is_pinned() {
        // P0-B B1: deterministic byte-level baseline for the R2.1-B decision
        // corpus, following the `production_baseline.rs` pattern (exact ADTS
        // byte length + FNV-1a 64 hash per material per engine, plus a
        // canonical-evidence digest). Scope: the 15 decision-corpus materials
        // (381 frames) through the production `step_frame` path on both
        // engines; the corpus definition (`materials()`) is the pinned
        // input. Unexpected changes fail loudly; values are pinned below and
        // never regenerated during normal test execution.
        struct Row {
            material: &'static str,
            sample_rate: u32,
            frames: u32,
            q_len: usize,
            q_hash: u64,
            f_len: usize,
            f_hash: u64,
        }
        let mut rows: Vec<Row> = Vec::new();
        for m in materials() {
            let mut h = R21BHarness::new(m.pcm, m.sample_rate);
            let mut q_adts: Vec<u8> = Vec::new();
            let mut f_adts: Vec<u8> = Vec::new();
            for f in 0..m.frames {
                let pair = h.step(f);
                let q_b = pair.q_bytes.as_ref().expect("Q31 emits");
                let f_b = pair.f_bytes.as_ref().expect("FPE emits");
                assert_adts_valid(q_b, m.sample_rate, 2, &format!("{} f{f} Q31", m.name));
                assert_adts_valid(f_b, m.sample_rate, 2, &format!("{} f{f} FPE", m.name));
                q_adts.extend_from_slice(q_b);
                f_adts.extend_from_slice(f_b);
            }
            let row = Row {
                material: m.name,
                sample_rate: m.sample_rate,
                frames: m.frames,
                q_len: q_adts.len(),
                q_hash: p0b_fnv1a64(&q_adts),
                f_len: f_adts.len(),
                f_hash: p0b_fnv1a64(&f_adts),
            };
            println!(
                "R21B-BASELINE {} sr={} frames={} q_len={} q_hash={:#018x} f_len={} f_hash={:#018x}",
                row.material,
                row.sample_rate,
                row.frames,
                row.q_len,
                row.q_hash,
                row.f_len,
                row.f_hash,
            );
            rows.push(row);
        }
        // Pinned expectations (recorded 2026-11-21, aarch64-apple-darwin,
        // toolchain 1.99.1; byte lengths are platform-neutral pins, hashes
        // pin exact ADTS content on the reference platform per the
        // production_baseline.rs convention).
        let expected: &[(&str, u32, u32, usize, u64, usize, u64)] = &[
            (
                "sine-stereo",
                48000,
                49,
                3147,
                0x8921_b76c_2eb4_297e,
                3147,
                0x8921_b76c_2eb4_297e,
            ),
            (
                "onset-stereo",
                48000,
                49,
                2359,
                0x2a6c_a107_a2f0_ba5d,
                2359,
                0x2a6c_a107_a2f0_ba5d,
            ),
            (
                "stepdown-stereo",
                48000,
                49,
                2236,
                0xc5e3_cf81_d7c9_051d,
                2236,
                0xc5e3_cf81_d7c9_051d,
            ),
            (
                "multitone-stereo",
                48000,
                32,
                2846,
                0x12b8_5b86_7b81_65e4,
                2846,
                0x12b8_5b86_7b81_65e4,
            ),
            (
                "tonal-noise-stereo",
                48000,
                26,
                10645,
                0x09c8_010f_bb56_535b,
                10645,
                0x09c8_010f_bb56_535b,
            ),
            (
                "quiet-sine-stereo",
                48000,
                32,
                2709,
                0xcaf0_4c85_7e5d_6686,
                2709,
                0x3aaf_db40_df87_98e6,
            ),
            (
                "sine-stereo-44100",
                44100,
                32,
                1968,
                0x3719_4169_f53b_1234,
                1968,
                0x3719_4169_f53b_1234,
            ),
            (
                "onset-stereo-44100",
                44100,
                32,
                1220,
                0x66dc_3820_6189_19ee,
                1220,
                0x66dc_3820_6189_19ee,
            ),
            (
                "silence-stereo",
                48000,
                8,
                120,
                0x73f8_cdbf_8e69_830d,
                120,
                0x73f8_cdbf_8e69_830d,
            ),
            (
                "probe-panned-48k",
                48000,
                12,
                686,
                0xc032_917a_b7b3_ad8f,
                691,
                0x6dbb_d0d6_0b26_ecfc,
            ),
            (
                "probe-outphase-48k",
                48000,
                12,
                672,
                0xa727_d13c_20dc_a705,
                682,
                0xa5bb_f197_1dde_9932,
            ),
            (
                "probe-centred-48k",
                48000,
                12,
                667,
                0x14d0_9c29_47d2_6449,
                667,
                0x14d0_9c29_47d2_6449,
            ),
            (
                "probe-narrow-48k",
                48000,
                12,
                1336,
                0xf9f5_6aed_5d54_7f12,
                1330,
                0x75b8_344e_ed9d_a453,
            ),
            (
                "probe-quiet-48k",
                48000,
                12,
                1060,
                0x5e44_589b_6ef2_dc74,
                1060,
                0x5e44_589b_6ef2_dc74,
            ),
            (
                "probe-panned-44100",
                44100,
                12,
                717,
                0xb809_7967_e420_9627,
                728,
                0xc22e_8852_3a89_bfbf,
            ),
        ];
        let total_frames: usize = rows.iter().map(|r| r.frames as usize).sum();
        assert_eq!(total_frames, 381, "decision corpus frame accounting");
        assert_eq!(rows.len(), 15, "decision corpus material count");
        assert_eq!(expected.len(), 15, "pinned baseline material count");
        for row in &rows {
            let (_, want_sr, want_frames, want_q_len, want_q_hash, want_f_len, want_f_hash) =
                expected
                    .iter()
                    .find(|(name, _, _, _, _, _, _)| *name == row.material)
                    .copied()
                    .unwrap_or_else(|| panic!("baseline: unexpected material {}", row.material));
            assert_eq!(row.sample_rate, want_sr, "{}: sample rate", row.material);
            assert_eq!(row.frames, want_frames, "{}: frame count", row.material);
            assert_eq!(
                row.q_len, want_q_len,
                "{}: Q31 ADTS byte length",
                row.material
            );
            assert_eq!(
                row.f_len, want_f_len,
                "{}: FPE ADTS byte length",
                row.material
            );
            // Exact ADTS content is pinned on the reference platform; other
            // targets keep the platform-neutral length/frame pins and report.
            if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                assert_eq!(row.q_hash, want_q_hash, "{}: Q31 ADTS hash", row.material);
                assert_eq!(row.f_hash, want_f_hash, "{}: FPE ADTS hash", row.material);
            } else {
                eprintln!(
                    "{}: exact ADTS hashes not enforced on this target; \
                     byte length and frame count pinned",
                    row.material
                );
            }
        }
        // Evidence digest: canonical serializations hashed deterministically.
        let c = corpus();
        let mut canon: Vec<String> = sorted_evidences(&c.evidences)
            .into_iter()
            .map(|e| e.canonical_string())
            .collect();
        canon.sort();
        let joined = canon.join("\n");
        let digest = p0b_fnv1a64(joined.as_bytes());
        println!(
            "R21B-EVIDENCE-DIGEST rows={} fnv1a64={:#018x}",
            canon.len(),
            digest
        );
        assert_eq!(canon.len(), 30, "evidence population");
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(digest, 0x0544_1ee2_9087_55aa, "canonical evidence digest");
        } else {
            eprintln!("canonical evidence digest not enforced on this target: {digest:#018x}");
        }
    }

    #[test]
    fn r21b_p0b_canonical_evidence_is_emitted() {
        // P0-C N3: durable, inspectable representation of the complete
        // canonical evidence rows (G-1 BandRow emission model).
        //
        // Provenance: each row is `FindingEvidence::canonical_string()` —
        // deterministic serialization (fixed field order, decimal integers,
        // bracketed decimal line vectors, `|`-joined `Debug` trace chains,
        // `n/a` for absent optionals, no map iteration / pointer / float
        // formatting) over values recorded from production-validated harness
        // state (`build_frame_evidences` reads `analyze_frame` locals after
        // the replay-equality gates; it never feeds a predicate). Includes
        // all required per-finding operands (IS both arms), stateless margins
        // (I1/I2/I3/I8/I10), MS operands/margins/`B_closed`/use, full frame
        // trace chains both arms, and Class B line vectors (`spec_off..spec_end`
        // Q31 lines both channels both arms). Unsupported-frame representation
        // is preserved (`unsupported=1`, `DomainUnsupported`,
        // `ms_supported=0` for F-01; never a pass, counted once in frames and
        // twice in findings). Digest findings (`band == -1`) would record
        // explicit `None`s (this corpus has 0 digest diffs, so the path is
        // documented but unexercised).
        //
        // Generation: `cargo test --locked --lib
        // r21b_p0b_canonical_evidence_is_emitted -- --nocapture` from `rust/`
        // prints one `R21B-CANONICAL <unique-id>` header plus the full
        // multi-line canonical row per finding in deterministic sorted order.
        // The test re-hashes the rows with the same procedure as
        // `r21b_p0b_byte_baseline_is_pinned` and pins the existing digest
        // baseline `0x0544_1ee2_9087_55aa` (reference platform). No decision,
        // bound, envelope, routing, or numerical behaviour is changed.
        let c = corpus();
        assert_eq!(c.evidences.len(), 30, "evidence population");
        let sorted = sorted_evidences(&c.evidences);
        let uids = unique_evidence_ids(&sorted);
        assert_eq!(uids.len(), sorted.len(), "one id per row");
        for (uid, ev) in uids.iter().zip(sorted.iter()) {
            // Header carries the deterministic unique id; the body is the
            // complete canonical row (all operands/margins/traces/vectors).
            println!("R21B-CANONICAL {uid}");
            println!("{}", ev.canonical_string());
        }
        // Reproduce the pinned digest baseline byte-for-byte.
        let mut canon: Vec<String> = sorted.iter().map(|e| e.canonical_string()).collect();
        canon.sort();
        let joined = canon.join("\n");
        let digest = p0b_fnv1a64(joined.as_bytes());
        println!(
            "R21B-CANONICAL-DIGEST rows={} fnv1a64={:#018x}",
            canon.len(),
            digest
        );
        assert_eq!(canon.len(), 30, "evidence population");
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(
                digest, 0x0544_1ee2_9087_55aa,
                "canonical evidence digest (preserved baseline)"
            );
        } else {
            eprintln!("canonical evidence digest not enforced on this target: {digest:#018x}");
        }
    }

    #[test]
    fn r21b_p0b_stable_identifiers_are_unique_and_deterministic() {
        // P0-C N4: deterministic identifiers uniquely identify each finding.
        // Original labels (`stable_id` = `{material}:f{frame}:b{band}`) are
        // preserved unchanged (they appear in the canonical string and in all
        // historical prints); uniqueness comes from the appended per-label
        // ordinal in sorted order (`unique_evidence_ids`). F-01/F-02 share
        // `quiet-sine-stereo:f16:b13` and are disambiguated by ordinal + the
        // retained `detail` tie-break. No classification or decision semantic
        // is changed.
        let c = corpus();
        assert_eq!(c.evidences.len(), 30, "evidence population");
        let sorted = sorted_evidences(&c.evidences);
        let uids = unique_evidence_ids(&sorted);
        assert_eq!(uids.len(), 30, "one id per finding");
        // Uniqueness across the evidence corpus.
        {
            use std::collections::BTreeSet;
            let set: BTreeSet<&String> = uids.iter().collect();
            assert_eq!(set.len(), 30, "unique identifiers are distinct");
        }
        // Original labels preserved as prefixes.
        for (uid, ev) in uids.iter().zip(sorted.iter()) {
            let sid = ev.stable_id();
            assert!(
                uid.starts_with(&format!("{sid}#")),
                "{uid}: preserves stable label {sid}"
            );
        }
        // The known collision is resolved: exactly one stable label appears
        // twice (F-01/F-02), with distinct unique ids.
        {
            use std::collections::BTreeMap;
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for ev in &sorted {
                *counts.entry(ev.stable_id()).or_insert(0) += 1;
            }
            let dupes: Vec<(&String, &usize)> = counts.iter().filter(|(_, &n)| n > 1).collect();
            assert_eq!(dupes.len(), 1, "exactly one colliding stable label");
            assert_eq!(
                dupes[0].0, "quiet-sine-stereo:f16:b13",
                "collision is F-01/F-02"
            );
            assert_eq!(*dupes[0].1, 2, "F-01/F-02 share one frame-band");
            let mut pair: Vec<&String> = uids
                .iter()
                .filter(|u| u.starts_with("quiet-sine-stereo:f16:b13#"))
                .collect();
            pair.sort();
            assert_eq!(pair.len(), 2, "two unique ids for F-01/F-02");
            assert_ne!(pair[0], pair[1], "F-01/F-02 unique ids differ");
        }
        // Determinism: repeated executions produce identical identifiers.
        let a = run_corpus();
        let b = run_corpus();
        let sa = unique_evidence_ids(&sorted_evidences(&a.evidences));
        let sb = unique_evidence_ids(&sorted_evidences(&b.evidences));
        assert_eq!(sa, sb, "unique identifiers identical across runs");
        assert_eq!(sa, uids, "identifiers stable vs cached corpus");
        println!("R21B-UNIQUE-IDS rows={} unique={}", uids.len(), uids.len());
    }

    #[test]
    fn r21b_tier_b_pinned_c_parity_blocked() {
        match std::env::var("PINNED_C_REF") {
            Err(_) => {
                println!(
                    "TIERB_SKIP r21b_tier_b_pinned_c_parity_blocked \
                     missing_env=PINNED_C_REF BLOCKED: pinned C reference unavailable; \
                     Tier-B C parity INCOMPLETE (needs pinned C ref + R2.1-B IS/MS oracle dumps)"
                );
            }
            Ok(dir) => {
                println!(
                    "TIERB_SKIP r21b_tier_b_pinned_c_parity_blocked \
                     present_env=PINNED_C_REF({dir}) BLOCKED: oracle dumps for R2.1-B \
                     IS/MS decisions unavailable; Tier-B C parity INCOMPLETE"
                );
            }
        }
    }
}
