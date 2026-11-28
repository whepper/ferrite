//! SPDX-License-Identifier: FDK-AAC
//!
//! Part of Ferrite, a Third-Party Modified Version of the
//! Fraunhofer FDK AAC Codec Library for Android.
//! See rust/NOTICE and rust/LICENSE-FDK-AAC for the complete license text,
//! modification notice, and date.
//! R2.2 Group E, phases E1–E3 (test-only): evidence-bundle schema, parser,
//! and bundle writer.
//!
//! This module establishes the deterministic row structures, identifiers,
//! ordering, canonical TSV serialization, schema-version validation,
//! vocabulary validation and JSON serialization foundations for the R2.2
//! evidence bundle (`docs/evidence/fpe-r22/`).
//!
//! E1 (frozen, OD-3): schema v2 contract only — synthetic rows, no corpus,
//! no bundle files, no production values. Population from Group A capture
//! (E3), the bundle writer (E5) and recurrence analysis (Group F) were
//! explicitly out of scope for E1 and remain so for the frozen contract.
//!
//! E2/E3 (authorized under `R2.2-group-E2-E3-owner-decision.md` §2): populate
//! the frozen E1 schema v2 from the accepted Group A capture and Group C/D
//! evidence, and write the committed bundle (`r22-bundle.tsv`,
//! `r22-bundle.json` + `json/` OP×row-type partition, `MANIFEST.txt`,
//! `SHA256SUMS`, `resource-report.txt`, `README.md`). No schema, contract,
//! production, API, threshold, tolerance, or pin change. Group F recurrence
//! stays `NotGenerated` (SG-3).
//!
//! ## Row family (design §5 as refined by §11 / E-OD-2)
//!
//! * `R22-FRAME` — one row per (OP, material, frame): window sequence,
//!   frame outcome (`F-even`/`F-valid`/`F-unsupported`), frame class
//!   (canonical R/A/S/P/F vocabulary) and margin status.
//! * `R22-BAND` — one row per (OP, material, frame, band): producing stage,
//!   band class and margin status.
//! * `R22-COV` — one row per (OP, coverage dimension, scope): the AC-2 /
//!   §10.6 applicability-matrix record with an explicit status marker and
//!   observed/required counts.
//!
//! The complete E3 corpus field population (per-band vectors, ATS state,
//! grants, accounting, replay results) is NOT established here. E1 fixes
//! identity, ordering and serialization so E3 can add fields without
//! changing row identity or file discipline.
//!
//! ## Identity and ordering
//!
//! * Row IDs are stable and deterministic:
//!   `R22-FRAME/{op}/{material}/{frame:06}`,
//!   `R22-BAND/{op}/{material}/{frame:06}/{band:03}`,
//!   `R22-COV/{op}/{dimension}/{scope}`.
//! * Total order (load-bearing for byte-identical regeneration): OP index
//!   (OP-1 … OP-6, numeric) < row-type rank (FRAME < BAND < COV) <
//!   material/scope (byte-lexicographic on the canonical id) < frame
//!   (numeric) < band (numeric) < dimension (byte-lexicographic).
//! * `RowSet` stores rows in a `BTreeMap` keyed by that order (no `HashMap`
//!   iteration anywhere) and rejects duplicate IDs.
//!
//! ## Serialization
//!
//! * TSV: UTF-8, LF line endings, tab-separated fields, fixed field order,
//!   canonical decimal integers (no leading zeros, no `+`), canonical
//!   vectors (comma-joined decimals, no spaces; absent optional vectors
//!   render as `n/a`), explicit missing-evidence markers, no embedded
//!   tabs/newlines, no trailing whitespace. Every file begins with the
//!   explicit schema-version header `R22SCHEMA\t2`; unknown versions are
//!   rejected, never accepted.
//! * Missing-evidence distinction (AC-7 / E-OD-10): `Missing`, `Unwired`,
//!   `Unsupported`, `Unobserved`, `NotApplicable` and `NotGenerated` are
//!   distinct markers. `NotApplicable` is recorded only for structurally
//!   inapplicable combinations; missing/unwired/unsupported/unobserved
//!   evidence is never reclassified as `NotApplicable`. `NotGenerated`
//!   marks Group F recurrence/persistence placeholders (E-OD-6 / SG-3):
//!   Group E draws no recurrence conclusions.
//! * JSON: one object per row with fixed key order (TSV field order,
//!   prefixed by `type` and `id`), same IDs and semantic values as TSV,
//!   independently parseable by the hand-written parser below (no new
//!   dependencies; no manifest or lockfile change).
//!
//! ## Vocabulary reuse (E-OD-1)
//!
//! Class/outcome/margin tokens are validated exclusively through the
//! reviewed Group D validators in `super::is_ms`
//! (`r22_class_token_valid`, `r22_outcome_token_valid`,
//! `r22_margin_token_valid`); stage tokens resolve to the reviewed
//! `super::trace::Stage`. The superseded `I` classification is never
//! accepted. No Group D behaviour is modified.

use std::collections::BTreeMap;
use std::fmt;

use super::is_ms::{r22_class_token_valid, r22_margin_token_valid, r22_outcome_token_valid};
use super::trace::Stage;

// ---------------------------------------------------------------------------
// Schema version
// ---------------------------------------------------------------------------

/// E1 evidence schema version. Bumped only by an explicit owner decision;
///
/// rows written under one version never parse under another.
pub(super) const SCHEMA_VERSION: u32 = 2;

/// Versioned margin-definition (MDEF) registry version. Kept in lockstep
/// with the evidence schema for E1-R0.3: MDEF v2 corresponds to schema v2.
pub(super) const MDEF_VERSION: u32 = 2;

/// Largest chain ordinal representable in a row ID (zero-padded 4 digits).
pub(super) const MAX_ORDINAL: u32 = 9_999;

/// The exact first line of every machine-readable evidence file:
/// `R22SCHEMA\t1`. Comparison is byte-exact; `R22SCHEMA 1`, `R22SCHEMA\t01`
/// and any other version are rejected.
pub(super) const SCHEMA_HEADER: &str = "R22SCHEMA\t2";

/// Render for absent optional fields (detail notes, optional vectors).
pub(super) const NA_TOKEN: &str = "n/a";

/// Largest frame index representable in a row ID (`{:06}` zero-padding keeps
/// lexicographic order identical to numeric order). The R2.2 corpus peaks
/// near 1884 frames; the bound fails closed far above it.
pub(super) const MAX_FRAME: u32 = 999_999;

/// Largest band index representable in a row ID (`{:03}` zero-padding).
pub(super) const MAX_BAND: u32 = 999;

/// Largest length of an ID-component string (material, dimension, scope).
pub(super) const MAX_ID_COMPONENT_LEN: usize = 128;

// ---------------------------------------------------------------------------
// Errors (fail closed; every malformed input maps to a variant)
// ---------------------------------------------------------------------------

/// All E1 parse/validation failures. Malformed rows are never accepted and
/// never counted as passes; every failure carries enough context to locate
/// the offending line and field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RowError {
    EmptyFile,
    MissingHeader,
    UnknownSchemaVersion(String),
    CarriageReturn,
    MissingTrailingNewline,
    EmptyLine(usize),
    BadFieldCount {
        line: usize,
        got: usize,
    },
    UnknownRowType(String),
    BadToken {
        line: usize,
        field: &'static str,
        value: String,
    },
    OutOfRange {
        line: usize,
        field: &'static str,
        value: String,
    },
    DuplicateId(String),
    BadJson(String),
}

impl fmt::Display for RowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RowError::EmptyFile => write!(f, "empty evidence file"),
            RowError::MissingHeader => write!(f, "missing schema-version header"),
            RowError::UnknownSchemaVersion(v) => {
                write!(f, "unknown schema version: {v:?}")
            }
            RowError::CarriageReturn => write!(f, "carriage return (CRLF) rejected"),
            RowError::MissingTrailingNewline => write!(f, "missing trailing newline"),
            RowError::EmptyLine(n) => write!(f, "empty line {n}"),
            RowError::BadFieldCount { line, got } => {
                write!(f, "line {line}: bad field count {got}")
            }
            RowError::UnknownRowType(t) => write!(f, "unknown row type: {t:?}"),
            RowError::BadToken { line, field, value } => {
                write!(f, "line {line}: bad {field} token {value:?}")
            }
            RowError::OutOfRange { line, field, value } => {
                write!(f, "line {line}: {field} out of range: {value:?}")
            }
            RowError::DuplicateId(id) => write!(f, "duplicate row id: {id:?}"),
            RowError::BadJson(why) => write!(f, "malformed JSON: {why}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical primitives
// ---------------------------------------------------------------------------

/// Parse a canonical decimal `u32`: ASCII digits, no leading zeros (unless
/// the value itself is `0`), no `+`, no whitespace. Rejects `01`, `+1`,
/// ` 1`, hex and overflow by round-trip comparison.
fn parse_canonical_u32(tok: &str) -> Option<u32> {
    let v: u32 = tok.parse().ok()?;
    if v.to_string() == tok {
        Some(v)
    } else {
        None
    }
}

/// Parse a canonical decimal `i32` (optional single leading `-`).
fn parse_canonical_i32(tok: &str) -> Option<i32> {
    let v: i32 = tok.parse().ok()?;
    if v.to_string() == tok {
        Some(v)
    } else {
        None
    }
}

/// True iff `s` carries no embedded tab, newline or carriage return.
/// UTF-8 content is otherwise unrestricted.
fn token_chars_ok(s: &str) -> bool {
    !s.bytes().any(|b| b == b'\t' || b == b'\n' || b == b'\r')
}

/// Validate an ID-component string (material id, coverage dimension, scope
/// material): non-empty, bounded, no `/` (the ID separator), no embedded
/// tabs/newlines.
fn id_component_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_ID_COMPONENT_LEN && !s.contains('/') && token_chars_ok(s)
}

// ---------------------------------------------------------------------------
// Operating points (design §4; OP-1 … OP-6 numeric order is load-bearing)
// ---------------------------------------------------------------------------

/// R2.2 operating point. Declaration order is the canonical OP-1 … OP-6
/// order used by total ordering; derived `Ord` encodes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Op {
    Op1,
    Op2,
    Op3,
    Op4,
    Op5,
    Op6,
}

impl Op {
    /// Numeric index 1 … 6.
    pub(super) fn index(self) -> u8 {
        match self {
            Op::Op1 => 1,
            Op::Op2 => 2,
            Op::Op3 => 3,
            Op::Op4 => 4,
            Op::Op5 => 5,
            Op::Op6 => 6,
        }
    }

    /// Canonical label (`OP-1` … `OP-6`).
    pub(super) fn label(self) -> &'static str {
        match self {
            Op::Op1 => "OP-1",
            Op::Op2 => "OP-2",
            Op::Op3 => "OP-3",
            Op::Op4 => "OP-4",
            Op::Op5 => "OP-5",
            Op::Op6 => "OP-6",
        }
    }

    /// Parse a canonical OP label. Anything else (including `OP-0`,
    /// `OP-7`, lowercase) is rejected.
    pub(super) fn parse(tok: &str) -> Option<Op> {
        match tok {
            "OP-1" => Some(Op::Op1),
            "OP-2" => Some(Op::Op2),
            "OP-3" => Some(Op::Op3),
            "OP-4" => Some(Op::Op4),
            "OP-5" => Some(Op::Op5),
            "OP-6" => Some(Op::Op6),
            _ => None,
        }
    }

    /// All six operating points in canonical order.
    pub(super) fn all() -> [Op; 6] {
        [Op::Op1, Op::Op2, Op::Op3, Op::Op4, Op::Op5, Op::Op6]
    }
}

// ---------------------------------------------------------------------------
// Row types (explicit ordering: FRAME < BAND < CHAIN < LEAF < REPLAY < COV)
// ---------------------------------------------------------------------------

/// Evidence row family. Rank order is the within-OP grouping used by total
/// ordering and (in E5) by the OP x row-type JSON partition. E1-R0.3 extends
/// the E0 three-row family (FRAME/BAND/COV) with CHAIN/LEAF/REPLAY while
/// preserving FRAME/BAND/COV identities and their relative order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RowType {
    Frame,
    Band,
    Chain,
    Leaf,
    Replay,
    Cov,
}

impl RowType {
    /// Canonical row tag (the TSV first field and JSON `type` value).
    pub(super) fn tag(self) -> &'static str {
        match self {
            RowType::Frame => "R22-FRAME",
            RowType::Band => "R22-BAND",
            RowType::Chain => "R22-CHAIN",
            RowType::Leaf => "R22-LEAF",
            RowType::Replay => "R22-REPLAY",
            RowType::Cov => "R22-COV",
        }
    }

    /// Rank used by total ordering: FRAME < BAND < CHAIN < LEAF < REPLAY < COV.
    pub(super) fn rank(self) -> u8 {
        match self {
            RowType::Frame => 0,
            RowType::Band => 1,
            RowType::Chain => 2,
            RowType::Leaf => 3,
            RowType::Replay => 4,
            RowType::Cov => 5,
        }
    }

    /// Parse a row tag. Anything else is rejected.
    pub(super) fn parse(tok: &str) -> Option<RowType> {
        match tok {
            "R22-FRAME" => Some(RowType::Frame),
            "R22-BAND" => Some(RowType::Band),
            "R22-CHAIN" => Some(RowType::Chain),
            "R22-LEAF" => Some(RowType::Leaf),
            "R22-REPLAY" => Some(RowType::Replay),
            "R22-COV" => Some(RowType::Cov),
            _ => None,
        }
    }

    /// All six row types in canonical rank order.
    pub(super) fn all() -> [RowType; 6] {
        [
            RowType::Frame,
            RowType::Band,
            RowType::Chain,
            RowType::Leaf,
            RowType::Replay,
            RowType::Cov,
        ]
    }
}

// ---------------------------------------------------------------------------
// Missing-evidence markers (AC-7 / E-OD-10; SG-3 for `NotGenerated`)
// ---------------------------------------------------------------------------

/// Explicit missing-evidence reason. The six markers are mutually distinct:
/// a row recorded under one marker is never reclassified as another (in
/// particular, missing/unwired/unsupported/unobserved evidence is never
/// counted as `NotApplicable`, and nothing is silently treated as a pass).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MissingKind {
    /// Evidence expected but not present in the row.
    Missing,
    /// Combination not wired by the harness (separately scoped capture
    /// extensions stay out of Group E per E-OD-10).
    Unwired,
    /// An unsupported path was hit (never a pass).
    Unsupported,
    /// Applicable combination with no observation (a coverage gap, never
    /// coverage).
    Unobserved,
    /// Structurally inapplicable combination (production-topology reason
    /// recorded in the row detail). The only marker that excuses absence.
    NotApplicable,
    /// Group F recurrence/persistence placeholder (E-OD-6 / SG-3). Group E
    /// draws no recurrence conclusions.
    NotGenerated,
}

impl MissingKind {
    /// Canonical marker token.
    pub(super) fn token(self) -> &'static str {
        match self {
            MissingKind::Missing => "Missing",
            MissingKind::Unwired => "Unwired",
            MissingKind::Unsupported => "Unsupported",
            MissingKind::Unobserved => "Unobserved",
            MissingKind::NotApplicable => "NotApplicable",
            MissingKind::NotGenerated => "NotGenerated",
        }
    }

    /// Parse a marker token. Vocabulary tokens and anything else are
    /// rejected (the caller tries the vocabulary first).
    pub(super) fn parse(tok: &str) -> Option<MissingKind> {
        match tok {
            "Missing" => Some(MissingKind::Missing),
            "Unwired" => Some(MissingKind::Unwired),
            "Unsupported" => Some(MissingKind::Unsupported),
            "Unobserved" => Some(MissingKind::Unobserved),
            "NotApplicable" => Some(MissingKind::NotApplicable),
            "NotGenerated" => Some(MissingKind::NotGenerated),
            _ => None,
        }
    }

    /// All six markers in canonical order.
    pub(super) fn all() -> [MissingKind; 6] {
        [
            MissingKind::Missing,
            MissingKind::Unwired,
            MissingKind::Unsupported,
            MissingKind::Unobserved,
            MissingKind::NotApplicable,
            MissingKind::NotGenerated,
        ]
    }
}

// ---------------------------------------------------------------------------
// Vocabulary-or-marker fields (Group D validators + explicit markers)
// ---------------------------------------------------------------------------

/// Shared representation: either a reviewed-vocabulary token or an explicit
/// missing-evidence marker. Unknown tokens are rejected by construction
/// (parse returns `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
enum FieldValue {
    Present(String),
    Missing(MissingKind),
}

impl FieldValue {
    fn render(&self) -> String {
        match self {
            FieldValue::Present(t) => t.clone(),
            FieldValue::Missing(m) => m.token().to_string(),
        }
    }

    fn is_present(&self) -> bool {
        matches!(self, FieldValue::Present(_))
    }

    fn missing(&self) -> Option<MissingKind> {
        match self {
            FieldValue::Present(_) => None,
            FieldValue::Missing(m) => Some(*m),
        }
    }
}

/// Frame/band classification: a canonical R/A/S/P/F token (validated by
/// the reviewed `r22_class_token_valid`; the superseded `I` class is never
/// accepted) or an explicit missing-evidence marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ClassField(FieldValue);

impl ClassField {
    pub(super) fn parse(tok: &str) -> Option<ClassField> {
        if r22_class_token_valid(tok) {
            Some(ClassField(FieldValue::Present(tok.to_string())))
        } else {
            MissingKind::parse(tok).map(|m| ClassField(FieldValue::Missing(m)))
        }
    }

    pub(super) fn render(&self) -> String {
        self.0.render()
    }

    /// True only for a real classification (never for missing evidence).
    pub(super) fn is_classified(&self) -> bool {
        self.0.is_present()
    }

    pub(super) fn missing(&self) -> Option<MissingKind> {
        self.0.missing()
    }
}

/// Frame accounting outcome: `F-even`/`F-valid`/`F-unsupported` (reviewed
/// `r22_outcome_token_valid`) or an explicit missing-evidence marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OutcomeField(FieldValue);

impl OutcomeField {
    pub(super) fn parse(tok: &str) -> Option<OutcomeField> {
        if r22_outcome_token_valid(tok) {
            Some(OutcomeField(FieldValue::Present(tok.to_string())))
        } else {
            MissingKind::parse(tok).map(|m| OutcomeField(FieldValue::Missing(m)))
        }
    }

    pub(super) fn render(&self) -> String {
        self.0.render()
    }

    pub(super) fn is_classified(&self) -> bool {
        self.0.is_present()
    }

    pub(super) fn missing(&self) -> Option<MissingKind> {
        self.0.missing()
    }
}

/// Margin derivation status: `Derived`/`DerivedF32`/`MarginDerivationRequired`
/// (reviewed `r22_margin_token_valid`) or an explicit missing-evidence
/// marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MarginField(FieldValue);

impl MarginField {
    pub(super) fn parse(tok: &str) -> Option<MarginField> {
        if r22_margin_token_valid(tok) {
            Some(MarginField(FieldValue::Present(tok.to_string())))
        } else {
            MissingKind::parse(tok).map(|m| MarginField(FieldValue::Missing(m)))
        }
    }

    pub(super) fn render(&self) -> String {
        self.0.render()
    }

    pub(super) fn is_classified(&self) -> bool {
        self.0.is_present()
    }

    pub(super) fn missing(&self) -> Option<MissingKind> {
        self.0.missing()
    }
}

/// Coverage status: positively `Observed`, or an explicit missing-evidence
/// marker. Only `Missing(NotApplicable)` excuses absence; every other
/// marker (including `Missing(Unobserved)`) keeps the combination open as
/// a gap, never coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CoverageStatus {
    Observed,
    Missing(MissingKind),
}

impl CoverageStatus {
    pub(super) fn token(self) -> String {
        match self {
            CoverageStatus::Observed => "Observed".to_string(),
            CoverageStatus::Missing(m) => m.token().to_string(),
        }
    }

    pub(super) fn parse(tok: &str) -> Option<CoverageStatus> {
        if tok == "Observed" {
            Some(CoverageStatus::Observed)
        } else {
            MissingKind::parse(tok).map(CoverageStatus::Missing)
        }
    }

    /// True only when the combination was actually observed.
    pub(super) fn is_observed(self) -> bool {
        matches!(self, CoverageStatus::Observed)
    }

    /// True only for structurally inapplicable combinations recorded
    /// explicitly as such. Missing/unwired/unsupported/unobserved evidence
    /// is never applicable-by-default and never inapplicable-by-default.
    pub(super) fn is_inapplicable(self) -> bool {
        matches!(self, CoverageStatus::Missing(MissingKind::NotApplicable))
    }
}

/// Coverage scope: one canonical material id, or `*` for a whole-OP
/// aggregate row (e.g. the OP-4 chain totals).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Scope {
    Material(String),
    All,
}

impl Scope {
    pub(super) fn render(&self) -> String {
        match self {
            Scope::Material(m) => m.clone(),
            Scope::All => "*".to_string(),
        }
    }

    pub(super) fn parse(tok: &str) -> Option<Scope> {
        if tok == "*" {
            Some(Scope::All)
        } else if id_component_ok(tok) {
            Some(Scope::Material(tok.to_string()))
        } else {
            None
        }
    }
}

/// Resolve a band-row stage token to the reviewed Group D `Stage`. The
/// token set is exactly the `Stage::name` vocabulary (`is`, `ms`, `pe`,
/// `s2`, `s4`, `s6`, `acct`); accounting rows keep the distinct `acct`
/// stage rather than being folded into `s6` (D-R4 clarification carried at
/// schema level — class labels retain the Group D `S-S6` convention).
pub(super) fn parse_stage_token(tok: &str) -> Option<Stage> {
    match tok {
        "is" => Some(Stage::Is),
        "ms" => Some(Stage::Ms),
        "pe" => Some(Stage::Pe),
        "s2" => Some(Stage::S2),
        "s4" => Some(Stage::S4),
        "s6" => Some(Stage::S6),
        "acct" => Some(Stage::Acct),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Versioned MDEF registry (E1-R0.3, OD-2)
// ---------------------------------------------------------------------------

/// Margin-definition derivation mode. `Direct` computes the margin from the
/// captured operands via the primary edge function; `ReDerived` recomputes
/// it via an independent path (recomputed intermediate + edge function) and
/// must cross-check exactly against `Direct`. `Required` marks margins whose
/// derivation is not established: they stay `MarginDerivationRequired`
/// (OD-6 retains this for `s2.ah_clamp_short`; Q-1 stays owner-scoped with
/// no production change per OD-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MdefMode {
    Direct,
    ReDerived,
    Required,
}

impl MdefMode {
    pub(super) fn token(self) -> &'static str {
        match self {
            MdefMode::Direct => "Direct",
            MdefMode::ReDerived => "ReDerived",
            MdefMode::Required => "Required",
        }
    }

    pub(super) fn parse(tok: &str) -> Option<MdefMode> {
        match tok {
            "Direct" => Some(MdefMode::Direct),
            "ReDerived" => Some(MdefMode::ReDerived),
            "Required" => Some(MdefMode::Required),
            _ => None,
        }
    }
}

/// One versioned margin definition. The registry is the single source of
/// truth for which per-band margins exist, which stage owns them, and which
/// derivation mode they require. Unknown names are rejected, never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Mdef {
    pub name: &'static str,
    pub stage: Stage,
    pub mode: MdefMode,
    /// Production source reference (file + line anchor, observational only).
    pub source: &'static str,
}

/// E1-R0.3 MDEF registry, version 2. FRAME/BAND/COV margin names are a
/// subset; CHAIN/LEAF/REPLAY rows reference the same registry for their
/// margin fields. `s2.ah_clamp_short` stays `Required` (OD-6, D-R2).
pub(super) const MDEF_REGISTRY: &[Mdef] = &[
    Mdef {
        name: "pe.prepare_admission",
        stage: Stage::Pe,
        mode: MdefMode::Direct,
        source: "pe.rs:353",
    },
    Mdef {
        name: "pe.calc_admission",
        stage: Stage::Pe,
        mode: MdefMode::Direct,
        source: "pe.rs:406",
    },
    Mdef {
        name: "pe.c1_branch",
        stage: Stage::Pe,
        mode: MdefMode::Direct,
        source: "pe.rs:411",
    },
    Mdef {
        name: "pe.chaos_floor",
        stage: Stage::Pe,
        mode: MdefMode::Direct,
        source: "pe.rs:515",
    },
    Mdef {
        name: "pe.chaos_patch_arm",
        stage: Stage::Pe,
        mode: MdefMode::Direct,
        source: "pe.rs:516",
    },
    Mdef {
        name: "pe.chaos_patch_hi",
        stage: Stage::Pe,
        mode: MdefMode::Direct,
        source: "pe.rs:532",
    },
    Mdef {
        name: "pe.chaos_patch_mid",
        stage: Stage::Pe,
        mode: MdefMode::Direct,
        source: "pe.rs:537",
    },
    Mdef {
        name: "pe.enfac_energy_gate",
        stage: Stage::Pe,
        mode: MdefMode::Direct,
        source: "pe.rs:531",
    },
    Mdef {
        name: "i2.sentinel_a",
        stage: Stage::Is,
        mode: MdefMode::Direct,
        source: "intensity.rs:227",
    },
    Mdef {
        name: "i2.sentinel_b",
        stage: Stage::Is,
        mode: MdefMode::Direct,
        source: "intensity.rs:228",
    },
    Mdef {
        name: "s2.minsnr_drop",
        stage: Stage::S2,
        mode: MdefMode::Direct,
        source: "threshold_vbr.rs:308",
    },
    Mdef {
        name: "s2.ah_classify_spread",
        stage: Stage::S2,
        mode: MdefMode::Direct,
        source: "threshold_vbr.rs:419",
    },
    Mdef {
        name: "s2.ah_classify_minsnr",
        stage: Stage::S2,
        mode: MdefMode::Direct,
        source: "threshold_vbr.rs:419",
    },
    Mdef {
        name: "s2.ah_clamp",
        stage: Stage::S2,
        mode: MdefMode::Direct,
        source: "threshold_vbr.rs:870",
    },
    Mdef {
        name: "s2.ah_button",
        stage: Stage::S2,
        mode: MdefMode::Direct,
        source: "threshold_vbr.rs:871",
    },
    Mdef {
        name: "s2.short_negone",
        stage: Stage::S2,
        mode: MdefMode::Direct,
        source: "threshold_vbr.rs:854",
    },
    Mdef {
        name: "s2.short_zero",
        stage: Stage::S2,
        mode: MdefMode::Direct,
        source: "threshold_vbr.rs:856",
    },
    Mdef {
        name: "s2.long_zero",
        stage: Stage::S2,
        mode: MdefMode::Direct,
        source: "threshold_vbr.rs:865",
    },
    Mdef {
        name: "s2.ah_clamp_short",
        stage: Stage::S2,
        mode: MdefMode::Required,
        source: "threshold_vbr.rs:850-853 OD-6 D-R2",
    },
    Mdef {
        name: "s4.step3_dist",
        stage: Stage::S4,
        mode: MdefMode::Direct,
        source: "scf.rs:1090",
    },
    Mdef {
        name: "s4.step3_energy",
        stage: Stage::S4,
        mode: MdefMode::Direct,
        source: "scf.rs:1091",
    },
    Mdef {
        name: "s4.step3_pe_gate",
        stage: Stage::S4,
        mode: MdefMode::Direct,
        source: "scf.rs:1071",
    },
    Mdef {
        name: "s4.step3_sum",
        stage: Stage::S4,
        mode: MdefMode::Direct,
        source: "scf.rs:1098",
    },
    Mdef {
        name: "s6.grant_headroom",
        stage: Stage::S6,
        mode: MdefMode::Direct,
        source: "rate.rs:953",
    },
    Mdef {
        name: "s6.element_cap",
        stage: Stage::S6,
        mode: MdefMode::Direct,
        source: "rate.rs:1745",
    },
    Mdef {
        name: "s6.exit_headroom",
        stage: Stage::S6,
        mode: MdefMode::Direct,
        source: "rate.rs:1173",
    },
    Mdef {
        name: "s6.multipass",
        stage: Stage::S6,
        mode: MdefMode::Direct,
        source: "rate.rs:1023",
    },
    Mdef {
        name: "s6.pe_corr_hi",
        stage: Stage::S6,
        mode: MdefMode::Direct,
        source: "rate.rs:724",
    },
    Mdef {
        name: "s6.pe_corr_lo",
        stage: Stage::S6,
        mode: MdefMode::Direct,
        source: "rate.rs:728",
    },
    Mdef {
        name: "acct.fill_floor",
        stage: Stage::Acct,
        mode: MdefMode::Direct,
        source: "rate.rs:1224",
    },
    Mdef {
        name: "acct.fill_ext_gate",
        stage: Stage::Acct,
        mode: MdefMode::Direct,
        source: "rate.rs:1239",
    },
    Mdef {
        name: "acct.bitres_max",
        stage: Stage::Acct,
        mode: MdefMode::Direct,
        source: "rate.rs:912",
    },
    Mdef {
        name: "acct.bitres_floor",
        stage: Stage::Acct,
        mode: MdefMode::Direct,
        source: "rate.rs:909",
    },
    Mdef {
        name: "acct.padding_onset",
        stage: Stage::Acct,
        mode: MdefMode::Direct,
        source: "rate.rs:383",
    },
];

/// Look up a margin definition by name. Unknown names return `None`
/// (fail-closed, never guessed).
pub(super) fn mdef_get(name: &str) -> Option<&'static Mdef> {
    MDEF_REGISTRY.iter().find(|m| m.name == name)
}

/// True iff `name` is a registered margin definition.
pub(super) fn mdef_contains(name: &str) -> bool {
    mdef_get(name).is_some()
}

/// The registry version (E1-R0.3: 2, in lockstep with the schema).
pub(super) fn mdef_version() -> u32 {
    MDEF_VERSION
}

/// Q-1 remains an unverified production concern routed to owner-scoped
/// review (OD-5). No production change is authorized to resolve it here;
/// this marker makes the deferral explicit and test-assertable.
pub(super) const Q1_STATUS: &str = "OwnerReview";

// ---------------------------------------------------------------------------
// Row structures
// ---------------------------------------------------------------------------

/// R22-FRAME: one row per (OP, material, frame). Frame-level outcome,
/// classification and margin status; per-band detail lives in `BandRow`.
/// E3 capture fields (vectors, ATS state, accounting) attach to this
/// identity later without changing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FrameRow {
    pub op: Op,
    pub material: String,
    pub frame: u32,
    pub seq: i32,
    pub outcome: OutcomeField,
    pub class: ClassField,
    pub margin: MarginField,
}

impl FrameRow {
    /// Stable deterministic ID: `R22-FRAME/{op}/{material}/{frame:06}`.
    pub(super) fn id(&self) -> String {
        format!(
            "R22-FRAME/{}/{}/{:06}",
            self.op.label(),
            self.material,
            self.frame
        )
    }

    fn validate(&self, line: usize) -> Result<(), RowError> {
        if !id_component_ok(&self.material) {
            return Err(RowError::BadToken {
                line,
                field: "material",
                value: self.material.clone(),
            });
        }
        if self.frame > MAX_FRAME {
            return Err(RowError::OutOfRange {
                line,
                field: "frame",
                value: self.frame.to_string(),
            });
        }
        Ok(())
    }

    /// Canonical TSV body (without the row tag): fixed field order
    /// `op material frame seq outcome class margin`.
    pub(super) fn render_body(&self) -> String {
        self.validate(0)
            .expect("FrameRow::render_body: invalid row");
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.op.label(),
            self.material,
            self.frame,
            self.seq,
            self.outcome.render(),
            self.class.render(),
            self.margin.render()
        )
    }

    pub(super) fn render_tsv(&self) -> String {
        format!("{}\t{}", RowType::Frame.tag(), self.render_body())
    }

    pub(super) fn parse_body(f: &[&str], line: usize) -> Result<FrameRow, RowError> {
        debug_assert_eq!(f.len(), 7);
        let bad = |field: &'static str, value: &str| RowError::BadToken {
            line,
            field,
            value: value.to_string(),
        };
        let op = Op::parse(f[0]).ok_or_else(|| bad("op", f[0]))?;
        if !id_component_ok(f[1]) {
            return Err(bad("material", f[1]));
        }
        let frame = parse_canonical_u32(f[2]).ok_or_else(|| bad("frame", f[2]))?;
        if frame > MAX_FRAME {
            return Err(RowError::OutOfRange {
                line,
                field: "frame",
                value: f[2].to_string(),
            });
        }
        let seq = parse_canonical_i32(f[3]).ok_or_else(|| bad("seq", f[3]))?;
        let outcome = OutcomeField::parse(f[4]).ok_or_else(|| bad("outcome", f[4]))?;
        let class = ClassField::parse(f[5]).ok_or_else(|| bad("class", f[5]))?;
        let margin = MarginField::parse(f[6]).ok_or_else(|| bad("margin", f[6]))?;
        Ok(FrameRow {
            op,
            material: f[1].to_string(),
            frame,
            seq,
            outcome,
            class,
            margin,
        })
    }
}

/// R22-BAND: one row per (OP, material, frame, band). Producing stage,
/// band class and margin status for that band cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BandRow {
    pub op: Op,
    pub material: String,
    pub frame: u32,
    pub band: u32,
    pub stage: Stage,
    pub class: ClassField,
    pub margin: MarginField,
}

impl BandRow {
    /// Stable deterministic ID:
    /// `R22-BAND/{op}/{material}/{frame:06}/{band:03}`.
    pub(super) fn id(&self) -> String {
        format!(
            "R22-BAND/{}/{}/{:06}/{:03}",
            self.op.label(),
            self.material,
            self.frame,
            self.band
        )
    }

    fn validate(&self, line: usize) -> Result<(), RowError> {
        if !id_component_ok(&self.material) {
            return Err(RowError::BadToken {
                line,
                field: "material",
                value: self.material.clone(),
            });
        }
        if self.frame > MAX_FRAME {
            return Err(RowError::OutOfRange {
                line,
                field: "frame",
                value: self.frame.to_string(),
            });
        }
        if self.band > MAX_BAND {
            return Err(RowError::OutOfRange {
                line,
                field: "band",
                value: self.band.to_string(),
            });
        }
        Ok(())
    }

    /// Canonical TSV body (without the row tag): fixed field order
    /// `op material frame band stage class margin`.
    pub(super) fn render_body(&self) -> String {
        self.validate(0).expect("BandRow::render_body: invalid row");
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.op.label(),
            self.material,
            self.frame,
            self.band,
            self.stage.name(),
            self.class.render(),
            self.margin.render()
        )
    }

    pub(super) fn render_tsv(&self) -> String {
        format!("{}\t{}", RowType::Band.tag(), self.render_body())
    }

    pub(super) fn parse_body(f: &[&str], line: usize) -> Result<BandRow, RowError> {
        debug_assert_eq!(f.len(), 7);
        let bad = |field: &'static str, value: &str| RowError::BadToken {
            line,
            field,
            value: value.to_string(),
        };
        let op = Op::parse(f[0]).ok_or_else(|| bad("op", f[0]))?;
        if !id_component_ok(f[1]) {
            return Err(bad("material", f[1]));
        }
        let frame = parse_canonical_u32(f[2]).ok_or_else(|| bad("frame", f[2]))?;
        if frame > MAX_FRAME {
            return Err(RowError::OutOfRange {
                line,
                field: "frame",
                value: f[2].to_string(),
            });
        }
        let band = parse_canonical_u32(f[3]).ok_or_else(|| bad("band", f[3]))?;
        if band > MAX_BAND {
            return Err(RowError::OutOfRange {
                line,
                field: "band",
                value: f[3].to_string(),
            });
        }
        let stage = parse_stage_token(f[4]).ok_or_else(|| bad("stage", f[4]))?;
        let class = ClassField::parse(f[5]).ok_or_else(|| bad("class", f[5]))?;
        let margin = MarginField::parse(f[6]).ok_or_else(|| bad("margin", f[6]))?;
        Ok(BandRow {
            op,
            material: f[1].to_string(),
            frame,
            band,
            stage,
            class,
            margin,
        })
    }
}

/// R22-COV: one row per (OP, coverage dimension, scope) — the AC-2 / §10.6
/// applicability-matrix record. `dimension` names the requested combination
/// (e.g. `window:LONG`, `transition:START`, `path:s4-step3`); `scope` is one
/// material or `*` for a whole-OP aggregate. `observed`/`required` are exact
/// counts; `detail` carries the `NotApplicable` reason or `n/a` when absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CovRow {
    pub op: Op,
    pub dimension: String,
    pub scope: Scope,
    pub status: CoverageStatus,
    pub observed: u32,
    pub required: u32,
    pub detail: Option<String>,
}

impl CovRow {
    /// Stable deterministic ID: `R22-COV/{op}/{dimension}/{scope}`.
    pub(super) fn id(&self) -> String {
        format!(
            "R22-COV/{}/{}/{}",
            self.op.label(),
            self.dimension,
            self.scope.render()
        )
    }

    fn validate(&self, line: usize) -> Result<(), RowError> {
        if !id_component_ok(&self.dimension) {
            return Err(RowError::BadToken {
                line,
                field: "dimension",
                value: self.dimension.clone(),
            });
        }
        match &self.scope {
            Scope::Material(m) if !id_component_ok(m) => {
                return Err(RowError::BadToken {
                    line,
                    field: "scope",
                    value: m.clone(),
                });
            }
            _ => {}
        }
        if self.observed > self.required {
            return Err(RowError::OutOfRange {
                line,
                field: "observed",
                value: format!("{}>{}", self.observed, self.required),
            });
        }
        match &self.detail {
            Some(d) if d.is_empty() || d == NA_TOKEN || !token_chars_ok(d) => {
                return Err(RowError::BadToken {
                    line,
                    field: "detail",
                    value: d.clone(),
                });
            }
            _ => {}
        }
        Ok(())
    }

    /// Canonical TSV body (without the row tag): fixed field order
    /// `op dimension scope status observed required detail`.
    pub(super) fn render_body(&self) -> String {
        self.validate(0).expect("CovRow::render_body: invalid row");
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.op.label(),
            self.dimension,
            self.scope.render(),
            self.status.token(),
            self.observed,
            self.required,
            self.detail.as_deref().unwrap_or(NA_TOKEN)
        )
    }

    pub(super) fn render_tsv(&self) -> String {
        format!("{}\t{}", RowType::Cov.tag(), self.render_body())
    }

    pub(super) fn parse_body(f: &[&str], line: usize) -> Result<CovRow, RowError> {
        debug_assert_eq!(f.len(), 7);
        let bad = |field: &'static str, value: &str| RowError::BadToken {
            line,
            field,
            value: value.to_string(),
        };
        let op = Op::parse(f[0]).ok_or_else(|| bad("op", f[0]))?;
        if !id_component_ok(f[1]) {
            return Err(bad("dimension", f[1]));
        }
        let scope = Scope::parse(f[2]).ok_or_else(|| bad("scope", f[2]))?;
        let status = CoverageStatus::parse(f[3]).ok_or_else(|| bad("status", f[3]))?;
        let observed = parse_canonical_u32(f[4]).ok_or_else(|| bad("observed", f[4]))?;
        let required = parse_canonical_u32(f[5]).ok_or_else(|| bad("required", f[5]))?;
        if observed > required {
            return Err(RowError::OutOfRange {
                line,
                field: "observed",
                value: format!("{observed}>{required}"),
            });
        }
        let detail = if f[6] == NA_TOKEN {
            None
        } else if f[6].is_empty() || !token_chars_ok(f[6]) {
            return Err(bad("detail", f[6]));
        } else {
            Some(f[6].to_string())
        };
        Ok(CovRow {
            op,
            dimension: f[1].to_string(),
            scope,
            status,
            observed,
            required,
            detail,
        })
    }
}

// ---------------------------------------------------------------------------
// R22-CHAIN: one row per propagation link (E1-R0.3, OD-2)
// ---------------------------------------------------------------------------

/// Chain-link origin: the earliest attributable upstream step (`Link`), the
/// chain head (`Root`), or an unattributable later-stage difference
/// (`Unattributed`, never forced).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ChainOrigin {
    Root,
    Unattributed,
    Link { stage: Stage, band: u32 },
}

impl ChainOrigin {
    pub(super) fn render(&self) -> String {
        match self {
            ChainOrigin::Root => "root".to_string(),
            ChainOrigin::Unattributed => "Unattributed".to_string(),
            ChainOrigin::Link { stage, band } => format!("{}:{band}", stage.name()),
        }
    }

    pub(super) fn parse(tok: &str) -> Option<ChainOrigin> {
        if tok == "root" {
            return Some(ChainOrigin::Root);
        }
        if tok == "Unattributed" {
            return Some(ChainOrigin::Unattributed);
        }
        let (s, b) = tok.split_once(':')?;
        let stage = parse_stage_token(s)?;
        let band = parse_canonical_u32(b)?;
        if band > MAX_BAND {
            return None;
        }
        Some(ChainOrigin::Link { stage, band })
    }
}

/// R22-CHAIN: one row per (OP, material, frame, chain, ordinal). The chain
/// groups the ordered propagation links of one flipped frame; `ordinal` is
/// the dense link index (0-based). Provenance is carried by `origin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChainRow {
    pub op: Op,
    pub material: String,
    pub frame: u32,
    pub chain: String,
    pub ordinal: u32,
    pub stage: Stage,
    pub band: u32,
    pub origin: ChainOrigin,
}

impl ChainRow {
    /// Stable deterministic ID:
    /// `R22-CHAIN/{op}/{material}/{frame:06}/{chain}/{ordinal:04}`.
    pub(super) fn id(&self) -> String {
        format!(
            "R22-CHAIN/{}/{}/{:06}/{}/{:04}",
            self.op.label(),
            self.material,
            self.frame,
            self.chain,
            self.ordinal
        )
    }

    fn validate(&self, line: usize) -> Result<(), RowError> {
        if !id_component_ok(&self.material) {
            return Err(RowError::BadToken {
                line,
                field: "material",
                value: self.material.clone(),
            });
        }
        if !id_component_ok(&self.chain) {
            return Err(RowError::BadToken {
                line,
                field: "chain",
                value: self.chain.clone(),
            });
        }
        if self.frame > MAX_FRAME {
            return Err(RowError::OutOfRange {
                line,
                field: "frame",
                value: self.frame.to_string(),
            });
        }
        if self.ordinal > MAX_ORDINAL {
            return Err(RowError::OutOfRange {
                line,
                field: "ordinal",
                value: self.ordinal.to_string(),
            });
        }
        if self.band > MAX_BAND {
            return Err(RowError::OutOfRange {
                line,
                field: "band",
                value: self.band.to_string(),
            });
        }
        Ok(())
    }

    /// Canonical TSV body: `op material frame chain ordinal stage band origin`.
    pub(super) fn render_body(&self) -> String {
        self.validate(0)
            .expect("ChainRow::render_body: invalid row");
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.op.label(),
            self.material,
            self.frame,
            self.chain,
            self.ordinal,
            self.stage.name(),
            self.band,
            self.origin.render()
        )
    }

    pub(super) fn render_tsv(&self) -> String {
        format!("{}\t{}", RowType::Chain.tag(), self.render_body())
    }

    pub(super) fn parse_body(f: &[&str], line: usize) -> Result<ChainRow, RowError> {
        debug_assert_eq!(f.len(), 8);
        let bad = |field: &'static str, value: &str| RowError::BadToken {
            line,
            field,
            value: value.to_string(),
        };
        let op = Op::parse(f[0]).ok_or_else(|| bad("op", f[0]))?;
        if !id_component_ok(f[1]) {
            return Err(bad("material", f[1]));
        }
        let frame = parse_canonical_u32(f[2]).ok_or_else(|| bad("frame", f[2]))?;
        if frame > MAX_FRAME {
            return Err(RowError::OutOfRange {
                line,
                field: "frame",
                value: f[2].to_string(),
            });
        }
        if !id_component_ok(f[3]) {
            return Err(bad("chain", f[3]));
        }
        let ordinal = parse_canonical_u32(f[4]).ok_or_else(|| bad("ordinal", f[4]))?;
        if ordinal > MAX_ORDINAL {
            return Err(RowError::OutOfRange {
                line,
                field: "ordinal",
                value: f[4].to_string(),
            });
        }
        let stage = parse_stage_token(f[5]).ok_or_else(|| bad("stage", f[5]))?;
        let band = parse_canonical_u32(f[6]).ok_or_else(|| bad("band", f[6]))?;
        if band > MAX_BAND {
            return Err(RowError::OutOfRange {
                line,
                field: "band",
                value: f[6].to_string(),
            });
        }
        let origin = ChainOrigin::parse(f[7]).ok_or_else(|| bad("origin", f[7]))?;
        Ok(ChainRow {
            op,
            material: f[1].to_string(),
            frame,
            chain: f[3].to_string(),
            ordinal,
            stage,
            band,
            origin,
        })
    }
}

/// Validate one chain group: ordinals must be dense `0..n`, unique, ordered,
/// and bounded; the head link must carry `Root` provenance; every later
/// `Link` origin must cite an earlier link in the same chain (or be
/// `Unattributed`, never forced). Fails closed on any violation.
pub(super) fn validate_chain_set(rows: &[ChainRow]) -> Result<(), RowError> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<(String, String, u32, String), Vec<&ChainRow>> = BTreeMap::new();
    for r in rows {
        let key = (
            r.op.label().to_string(),
            r.material.clone(),
            r.frame,
            r.chain.clone(),
        );
        groups.entry(key).or_default().push(r);
    }
    for (_, mut links) in groups {
        links.sort_by_key(|r| r.ordinal);
        for (i, r) in links.iter().enumerate() {
            if r.ordinal as usize != i {
                return Err(RowError::OutOfRange {
                    line: 0,
                    field: "ordinal",
                    value: format!("gap: want {i} got {}", r.ordinal),
                });
            }
        }
        if links.is_empty() {
            return Err(RowError::BadToken {
                line: 0,
                field: "chain",
                value: "empty".to_string(),
            });
        }
        match &links[0].origin {
            ChainOrigin::Root => {}
            other => {
                return Err(RowError::BadToken {
                    line: 0,
                    field: "origin",
                    value: format!("head must be root, got {other:?}"),
                });
            }
        }
        for (i, r) in links.iter().enumerate().skip(1) {
            match &r.origin {
                ChainOrigin::Root => {
                    return Err(RowError::BadToken {
                        line: 0,
                        field: "origin",
                        value: "only head may be root".to_string(),
                    });
                }
                ChainOrigin::Unattributed => {}
                ChainOrigin::Link { stage, band } => {
                    let mut found = false;
                    for prev in links.iter().take(i) {
                        if &prev.stage == stage && &prev.band == band {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(RowError::BadToken {
                            line: 0,
                            field: "origin",
                            value: format!("unprovenanced link {}:{band}", stage.name()),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// R22-LEAF: one row per decomposed leaf (E1-R0.3, OD-2)
// ---------------------------------------------------------------------------

/// Suffix marking an explicitly incomplete decomposition path. Incomplete
/// paths must carry this suffix and an absent value; silently dropping a
/// truncated subtree is rejected.
pub(super) const INCOMPLETE_SUFFIX: &str = ".<incomplete>";

/// True iff `path` carries the explicit incomplete-path marker.
pub(super) fn is_incomplete_path(path: &str) -> bool {
    path.ends_with(INCOMPLETE_SUFFIX)
}

/// Validate a leaf path: non-empty, bounded, no `/` (ID separator), no
/// embedded tabs/newlines, dot/bracket vocabulary only.
pub(super) fn leaf_path_ok(path: &str) -> bool {
    if path.is_empty() || path.len() > MAX_ID_COMPONENT_LEN {
        return false;
    }
    if !token_chars_ok(path) || path.contains('/') {
        return false;
    }
    path.bytes().all(|b| {
        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'[' | b']' | b'<' | b'>')
    })
}

/// R22-LEAF: one row per (OP, material, frame, band, path). `value` is the
/// canonical leaf value or absent (`n/a`) for missing leaves; incomplete
/// paths must be explicitly marked and absent-valued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LeafRow {
    pub op: Op,
    pub material: String,
    pub frame: u32,
    pub band: u32,
    pub path: String,
    pub value: Option<i32>,
}

impl LeafRow {
    /// Stable deterministic ID:
    /// `R22-LEAF/{op}/{material}/{frame:06}/{band:03}/{path}`.
    pub(super) fn id(&self) -> String {
        format!(
            "R22-LEAF/{}/{}/{:06}/{:03}/{}",
            self.op.label(),
            self.material,
            self.frame,
            self.band,
            self.path
        )
    }

    fn validate(&self, line: usize) -> Result<(), RowError> {
        if !id_component_ok(&self.material) {
            return Err(RowError::BadToken {
                line,
                field: "material",
                value: self.material.clone(),
            });
        }
        if self.frame > MAX_FRAME {
            return Err(RowError::OutOfRange {
                line,
                field: "frame",
                value: self.frame.to_string(),
            });
        }
        if self.band > MAX_BAND {
            return Err(RowError::OutOfRange {
                line,
                field: "band",
                value: self.band.to_string(),
            });
        }
        if !leaf_path_ok(&self.path) {
            return Err(RowError::BadToken {
                line,
                field: "path",
                value: self.path.clone(),
            });
        }
        if is_incomplete_path(&self.path) && self.value.is_some() {
            return Err(RowError::BadToken {
                line,
                field: "value",
                value: "incomplete path must be absent".to_string(),
            });
        }
        Ok(())
    }

    /// Canonical TSV body: `op material frame band path value`.
    pub(super) fn render_body(&self) -> String {
        self.validate(0).expect("LeafRow::render_body: invalid row");
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            self.op.label(),
            self.material,
            self.frame,
            self.band,
            self.path,
            match self.value {
                Some(v) => v.to_string(),
                None => NA_TOKEN.to_string(),
            }
        )
    }

    pub(super) fn render_tsv(&self) -> String {
        format!("{}\t{}", RowType::Leaf.tag(), self.render_body())
    }

    pub(super) fn parse_body(f: &[&str], line: usize) -> Result<LeafRow, RowError> {
        debug_assert_eq!(f.len(), 6);
        let bad = |field: &'static str, value: &str| RowError::BadToken {
            line,
            field,
            value: value.to_string(),
        };
        let op = Op::parse(f[0]).ok_or_else(|| bad("op", f[0]))?;
        if !id_component_ok(f[1]) {
            return Err(bad("material", f[1]));
        }
        let frame = parse_canonical_u32(f[2]).ok_or_else(|| bad("frame", f[2]))?;
        if frame > MAX_FRAME {
            return Err(RowError::OutOfRange {
                line,
                field: "frame",
                value: f[2].to_string(),
            });
        }
        let band = parse_canonical_u32(f[3]).ok_or_else(|| bad("band", f[3]))?;
        if band > MAX_BAND {
            return Err(RowError::OutOfRange {
                line,
                field: "band",
                value: f[3].to_string(),
            });
        }
        if !leaf_path_ok(f[4]) {
            return Err(bad("path", f[4]));
        }
        let value = if f[5] == NA_TOKEN {
            None
        } else {
            Some(parse_canonical_i32(f[5]).ok_or_else(|| bad("value", f[5]))?)
        };
        if is_incomplete_path(f[4]) && value.is_some() {
            return Err(bad("value", f[5]));
        }
        Ok(LeafRow {
            op,
            material: f[1].to_string(),
            frame,
            band,
            path: f[4].to_string(),
            value,
        })
    }
}

/// Synthetic nested test value for LEAF decomposition exercises. No corpus,
/// no production values: purely synthetic structures covering unequal
/// lengths, absent values, nesting and explicit truncation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Nested {
    Leaf(i32),
    Absent,
    Seq(Vec<Nested>),
    Map(Vec<(String, Nested)>),
    Truncated(String),
}

/// Decompose `node` at `prefix` into `(path, value)` leaves. Sequences emit
/// `prefix[i]`; maps emit `prefix.key`; absent leaves emit `None`;
/// truncated subtrees emit `prefix.<incomplete>` with `None` (explicit
/// incomplete-path handling, never silent).
pub(super) fn decompose_leaves(prefix: &str, node: &Nested, out: &mut Vec<(String, Option<i32>)>) {
    match node {
        Nested::Leaf(v) => out.push((prefix.to_string(), Some(*v))),
        Nested::Absent => out.push((prefix.to_string(), None)),
        Nested::Seq(items) => {
            for (i, item) in items.iter().enumerate() {
                let p = format!("{prefix}[{i}]");
                decompose_leaves(&p, item, out);
            }
        }
        Nested::Map(entries) => {
            for (k, v) in entries {
                let p = format!("{prefix}.{k}");
                decompose_leaves(&p, v, out);
            }
        }
        Nested::Truncated(_) => {
            out.push((format!("{prefix}{INCOMPLETE_SUFFIX}"), None));
        }
    }
}

/// Bidirectional completeness: every expected leaf must appear in the
/// observed set with an equal value (including absent-equals-absent), and
/// every observed leaf must appear in the expected set. Unequal lengths,
/// absent/value mismatches, nested-path mismatches and missing or spurious
/// incomplete markers all fail closed.
pub(super) fn check_leaf_completeness(
    expected: &[(String, Option<i32>)],
    observed: &[(String, Option<i32>)],
) -> Result<(), RowError> {
    use std::collections::BTreeMap;
    let exp: BTreeMap<&str, &Option<i32>> = expected.iter().map(|(p, v)| (p.as_str(), v)).collect();
    let obs: BTreeMap<&str, &Option<i32>> = observed.iter().map(|(p, v)| (p.as_str(), v)).collect();
    for (p, v) in &exp {
        match obs.get(p) {
            Some(ov) if *ov == *v => {}
            Some(ov) => {
                return Err(RowError::BadToken {
                    line: 0,
                    field: "leaf-value",
                    value: format!("{p}: expected {v:?} got {ov:?}"),
                });
            }
            None => {
                return Err(RowError::BadToken {
                    line: 0,
                    field: "leaf-path",
                    value: format!("missing {p}"),
                });
            }
        }
    }
    for (p, _) in &obs {
        if !exp.contains_key(p) {
            return Err(RowError::BadToken {
                line: 0,
                field: "leaf-path",
                value: format!("spurious {p}"),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// R22-REPLAY: corpus/OP-independent replay records (E1-R0.3, OD-2 + OD-7)
// ---------------------------------------------------------------------------

/// Replay kinds (test-only). The set is closed; unknown kinds are rejected.
pub(super) const REPLAY_KINDS: &[&str] =
    &["is", "pe", "s2", "s4", "s6", "acct", "writeahead", "f20"];

/// Validate a replay kind token.
pub(super) fn replay_kind_ok(kind: &str) -> bool {
    REPLAY_KINDS.contains(&kind)
}

/// Replay status: `Match` (replay reproduced production exactly),
/// `Mismatch` (divergence, persisted before assertion per OD-7),
/// `Torn` (truncated record), `Unavailable` (missing record). Only `Match`
/// validates successfully; every other status fails closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplayStatus {
    Match,
    Mismatch,
    Torn,
    Unavailable,
}

impl ReplayStatus {
    pub(super) fn token(self) -> &'static str {
        match self {
            ReplayStatus::Match => "Match",
            ReplayStatus::Mismatch => "Mismatch",
            ReplayStatus::Torn => "Torn",
            ReplayStatus::Unavailable => "Unavailable",
        }
    }

    pub(super) fn parse(tok: &str) -> Option<ReplayStatus> {
        match tok {
            "Match" => Some(ReplayStatus::Match),
            "Mismatch" => Some(ReplayStatus::Mismatch),
            "Torn" => Some(ReplayStatus::Torn),
            "Unavailable" => Some(ReplayStatus::Unavailable),
            _ => None,
        }
    }
}

/// Validate a 64-char lowercase hex digest (sha256).
pub(super) fn digest_ok(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// R22-REPLAY: one row per replay tag. Deliberately corpus/OP-independent:
/// no OP, material or frame fields. Identity is `R22-REPLAY/{tag}` and the
/// total order sorts purely by tag, so REPLAY rows never shift with corpus
/// or OP selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReplayRow {
    pub tag: String,
    pub kind: String,
    pub digest: String,
    pub status: ReplayStatus,
}

impl ReplayRow {
    /// Stable deterministic ID: `R22-REPLAY/{tag}` (no OP/material/frame).
    pub(super) fn id(&self) -> String {
        format!("R22-REPLAY/{}", self.tag)
    }

    fn validate(&self, line: usize) -> Result<(), RowError> {
        if !id_component_ok(&self.tag) {
            return Err(RowError::BadToken {
                line,
                field: "tag",
                value: self.tag.clone(),
            });
        }
        if !replay_kind_ok(&self.kind) {
            return Err(RowError::BadToken {
                line,
                field: "kind",
                value: self.kind.clone(),
            });
        }
        if !digest_ok(&self.digest) {
            return Err(RowError::BadToken {
                line,
                field: "digest",
                value: self.digest.clone(),
            });
        }
        Ok(())
    }

    /// Canonical TSV body: `tag kind digest status`.
    pub(super) fn render_body(&self) -> String {
        self.validate(0)
            .expect("ReplayRow::render_body: invalid row");
        format!(
            "{}\t{}\t{}\t{}",
            self.tag,
            self.kind,
            self.digest,
            self.status.token()
        )
    }

    pub(super) fn render_tsv(&self) -> String {
        format!("{}\t{}", RowType::Replay.tag(), self.render_body())
    }

    pub(super) fn parse_body(f: &[&str], line: usize) -> Result<ReplayRow, RowError> {
        debug_assert_eq!(f.len(), 4);
        let bad = |field: &'static str, value: &str| RowError::BadToken {
            line,
            field,
            value: value.to_string(),
        };
        if !id_component_ok(f[0]) {
            return Err(bad("tag", f[0]));
        }
        if !replay_kind_ok(f[1]) {
            return Err(bad("kind", f[1]));
        }
        if !digest_ok(f[2]) {
            return Err(bad("digest", f[2]));
        }
        let status = ReplayStatus::parse(f[3]).ok_or_else(|| bad("status", f[3]))?;
        Ok(ReplayRow {
            tag: f[0].to_string(),
            kind: f[1].to_string(),
            digest: f[2].to_string(),
            status,
        })
    }
}

// ---------------------------------------------------------------------------
// Durable write-ahead replay staging (E1-R0.3, OD-7)
// ---------------------------------------------------------------------------

/// Write-ahead staging errors. All fail closed: a torn, mismatched or
/// unavailable record must prevent successful validation, never count as a
/// pass. A mismatch is persisted before assertion so the divergent bytes
/// remain inspectable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StagingError {
    Unavailable(String),
    Torn(String),
    Mismatch(String),
}

impl std::fmt::Display for StagingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StagingError::Unavailable(s) => write!(f, "replay unavailable: {s}"),
            StagingError::Torn(s) => write!(f, "replay torn: {s}"),
            StagingError::Mismatch(s) => write!(f, "replay mismatch: {s}"),
        }
    }
}

/// Stage `payload` durably under `dir/{tag}.replay`: write, flush, `fsync`
/// the file before returning. The caller asserts only after staging, so a
/// mismatch is persisted before assertion (OD-7).
pub(super) fn stage_replay(
    dir: &std::path::Path,
    tag: &str,
    payload: &[u8],
) -> Result<std::path::PathBuf, StagingError> {
    if !id_component_ok(tag) {
        return Err(StagingError::Unavailable(format!("bad tag {tag:?}")));
    }
    std::fs::create_dir_all(dir).map_err(|e| StagingError::Unavailable(format!("mkdir: {e}")))?;
    let path = dir.join(format!("{tag}.replay"));
    let mut file = std::fs::File::create(&path)
        .map_err(|e| StagingError::Unavailable(format!("create: {e}")))?;
    use std::io::Write;
    file.write_all(payload)
        .map_err(|e| StagingError::Unavailable(format!("write: {e}")))?;
    file.flush()
        .map_err(|e| StagingError::Unavailable(format!("flush: {e}")))?;
    file.sync_all()
        .map_err(|e| StagingError::Unavailable(format!("fsync: {e}")))?;
    Ok(path)
}

/// Validate a staged record against `expected`: missing files are
/// `Unavailable`, length/content truncation is `Torn`, any byte difference
/// is `Mismatch`. Only exact equality returns `Ok`. All failures keep the
/// staged bytes on disk for inspection.
pub(super) fn validate_staged(path: &std::path::Path, expected: &[u8]) -> Result<(), StagingError> {
    let got = std::fs::read(path)
        .map_err(|_| StagingError::Unavailable(format!("missing {}", path.display())))?;
    if got.len() != expected.len() {
        return Err(StagingError::Torn(format!(
            "length {} vs {}",
            got.len(),
            expected.len()
        )));
    }
    if got != expected {
        return Err(StagingError::Mismatch(format!(
            "{} bytes differ",
            got.len()
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-instance COV witness bijection + fail-closed aggregation (E1-R0.3)
// ---------------------------------------------------------------------------

/// One coverage witness: the concrete observed instance backing a COV count.
/// Witness IDs are canonical id-components (e.g. frame-qualified tags).
pub(super) fn witness_id_ok(id: &str) -> bool {
    id_component_ok(id)
}

/// Per-instance witness set for one COV row. Bijection: when the COV status
/// is `Observed` with `observed == n`, exactly `n` distinct witness IDs must
/// be present; any duplicate, missing or surplus witness fails closed. When
/// the status is any missing-evidence marker, the witness list must be empty
/// (no phantom witnesses for unobserved combinations).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CovWitnessSet {
    pub cov_id: String,
    pub observed: u32,
    pub is_observed: bool,
    pub witnesses: Vec<String>,
}

impl CovWitnessSet {
    pub(super) fn validate(&self) -> Result<(), RowError> {
        if self.is_observed {
            if self.witnesses.len() as u32 != self.observed {
                return Err(RowError::OutOfRange {
                    line: 0,
                    field: "witnesses",
                    value: format!("{} vs {}", self.witnesses.len(), self.observed),
                });
            }
            let mut seen = std::collections::BTreeSet::new();
            for w in &self.witnesses {
                if !witness_id_ok(w) {
                    return Err(RowError::BadToken {
                        line: 0,
                        field: "witness",
                        value: w.clone(),
                    });
                }
                if !seen.insert(w) {
                    return Err(RowError::DuplicateId(format!("duplicate witness {w:?}")));
                }
            }
        } else if !self.witnesses.is_empty() {
            return Err(RowError::BadToken {
                line: 0,
                field: "witnesses",
                value: "must be empty when unobserved".to_string(),
            });
        }
        Ok(())
    }
}

/// Fail-closed aggregate coverage: the whole-OP (`*`) observed count must
/// equal the sum of the per-material observed counts. Any mismatch fails
/// closed (never silently adjusted).
pub(super) fn validate_cov_aggregate(
    per_material_observed: &[u32],
    total_observed: u32,
) -> Result<(), RowError> {
    let sum: u32 = per_material_observed.iter().sum();
    if sum != total_observed {
        return Err(RowError::OutOfRange {
            line: 0,
            field: "observed",
            value: format!("aggregate {sum} vs total {total_observed}"),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Geometry serialization + stereo equality invariant (E1-R0.3)
// ---------------------------------------------------------------------------

/// Serializable SFB geometry (per-channel captured values preserved exactly:
/// `cnt`, `per`, `max`, `short`, channel count and the full offsets vector).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Geometry {
    pub cnt: u32,
    pub per: u32,
    pub max: u32,
    pub short: bool,
    pub offsets: Vec<i32>,
}

impl Geometry {
    /// Canonical serialization: `cnt=..;per=..;max=..;short=0|1;offsets=..`.
    pub(super) fn serialize(&self) -> String {
        format!(
            "cnt={};per={};max={};short={};offsets={}",
            self.cnt,
            self.per,
            self.max,
            if self.short { 1 } else { 0 },
            self.offsets
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    /// Parse the canonical form. Rejects non-canonical integers, bad flags,
    /// embedded separators and out-of-range counts. Fails closed.
    pub(super) fn parse(s: &str) -> Option<Geometry> {
        let parts: Vec<&str> = s.split(';').collect();
        if parts.len() != 5 {
            return None;
        }
        let cnt = parts[0].strip_prefix("cnt=")?;
        let per = parts[1].strip_prefix("per=")?;
        let max = parts[2].strip_prefix("max=")?;
        let short = parts[3].strip_prefix("short=")?;
        let offsets = parts[4].strip_prefix("offsets=")?;
        let cnt = parse_canonical_u32(cnt)?;
        let per = parse_canonical_u32(per)?;
        let max = parse_canonical_u32(max)?;
        if cnt == 0 || per == 0 || max == 0 || cnt > 512 || per > 64 || max > 64 {
            return None;
        }
        let short = match short {
            "0" => false,
            "1" => true,
            _ => return None,
        };
        let offsets = if offsets.is_empty() {
            Vec::new()
        } else {
            offsets
                .split(',')
                .map(parse_canonical_i32)
                .collect::<Option<Vec<_>>>()?
        };
        Some(Geometry {
            cnt,
            per,
            max,
            short,
            offsets,
        })
    }
}

/// Approved stereo equality invariant: a synchronized stereo pair must agree
/// on `cnt`/`per`/`max`/`short` and carry equal-length offset vectors whose
/// per-channel captured values are preserved (this function does not merge
/// or coerce them; it only checks equality). Mono (`None` right) always
/// validates. Any stereo mismatch fails closed.
pub(super) fn validate_stereo_geometry(
    left: &Geometry,
    right: Option<&Geometry>,
) -> Result<(), RowError> {
    let Some(r) = right else {
        return Ok(());
    };
    if left.cnt != r.cnt || left.per != r.per || left.max != r.max || left.short != r.short {
        return Err(RowError::BadToken {
            line: 0,
            field: "geometry",
            value: "stereo cnt/per/max/short mismatch".to_string(),
        });
    }
    if left.offsets.len() != r.offsets.len() {
        return Err(RowError::BadToken {
            line: 0,
            field: "geometry",
            value: "stereo offsets length mismatch".to_string(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Direct / ReDerived margin modes + mandatory cross-check (E1-R0.3, OD-4)
// ---------------------------------------------------------------------------

/// Margin derivation mode for one per-band computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MarginMode {
    Direct,
    ReDerived,
}

impl MarginMode {
    pub(super) fn token(self) -> &'static str {
        match self {
            MarginMode::Direct => "Direct",
            MarginMode::ReDerived => "ReDerived",
        }
    }
}

/// One per-band margin derivation carrying both modes. The cross-check is
/// mandatory: `Direct` and `ReDerived` must agree exactly (integer equality,
/// `to_bits()` for f32), otherwise validation fails closed with
/// `Mismatch`. `s2.ah_clamp_short` never derives: both arms stay
/// `MarginDerivationRequired` (OD-6, conservative).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PerBandMargin {
    pub name: &'static str,
    pub direct: super::trace::MarginStatus,
    pub rederived: super::trace::MarginStatus,
}

impl PerBandMargin {
    /// Mandatory cross-check. Fails closed on any Direct/ReDerived
    /// disagreement, including Required-vs-Derived confusion.
    pub(super) fn cross_check(&self) -> Result<(), RowError> {
        use super::trace::MarginStatus as MS;
        match (&self.direct, &self.rederived) {
            (MS::Derived(a), MS::Derived(b)) if a == b => Ok(()),
            (MS::DerivedF32(a), MS::DerivedF32(b)) if a.to_bits() == b.to_bits() => Ok(()),
            (MS::MarginDerivationRequired(_), MS::MarginDerivationRequired(_)) => Ok(()),
            _ => Err(RowError::BadToken {
                line: 0,
                field: "margin-cross-check",
                value: format!("{} Direct/ReDerived mismatch", self.name),
            }),
        }
    }

    /// Conservative constructor for `s2.ah_clamp_short`: both modes stay
    /// `MarginDerivationRequired`, never derived (OD-6).
    pub(super) fn ah_clamp_short_required(reason: &'static str) -> Self {
        use super::trace::MarginStatus as MS;
        PerBandMargin {
            name: "s2.ah_clamp_short",
            direct: MS::MarginDerivationRequired(reason),
            rederived: MS::MarginDerivationRequired(reason),
        }
    }
}

// ---------------------------------------------------------------------------
// Row enum + total ordering
// ---------------------------------------------------------------------------

/// One evidence row of any type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Row {
    Frame(FrameRow),
    Band(BandRow),
    Chain(ChainRow),
    Leaf(LeafRow),
    Replay(ReplayRow),
    Cov(CovRow),
}

impl Row {
    pub(super) fn row_type(&self) -> RowType {
        match self {
            Row::Frame(_) => RowType::Frame,
            Row::Band(_) => RowType::Band,
            Row::Chain(_) => RowType::Chain,
            Row::Leaf(_) => RowType::Leaf,
            Row::Replay(_) => RowType::Replay,
            Row::Cov(_) => RowType::Cov,
        }
    }

    /// Stable deterministic row ID (see each row type).
    pub(super) fn id(&self) -> String {
        match self {
            Row::Frame(r) => r.id(),
            Row::Band(r) => r.id(),
            Row::Chain(r) => r.id(),
            Row::Leaf(r) => r.id(),
            Row::Replay(r) => r.id(),
            Row::Cov(r) => r.id(),
        }
    }

    /// Total-order key: deterministic tagged global ordering. FRAME/BAND/
    /// CHAIN/LEAF sort by (OP index, rank, material, frame, band/ordinal,
    /// trailing key); COV sorts by (OP, rank, scope, dimension); REPLAY
    /// sorts purely by tag (OP/material/frame-independent per E1-R0.3, so
    /// REPLAY rows never shift with corpus or OP selection). Fully
    /// deterministic; no hashing, no platform dependence.
    fn sort_key(&self) -> (u8, u8, String, u32, u32, String) {
        match self {
            Row::Frame(r) => (
                r.op.index(),
                RowType::Frame.rank(),
                r.material.clone(),
                r.frame,
                0,
                String::new(),
            ),
            Row::Band(r) => (
                r.op.index(),
                RowType::Band.rank(),
                r.material.clone(),
                r.frame,
                r.band,
                String::new(),
            ),
            Row::Chain(r) => (
                r.op.index(),
                RowType::Chain.rank(),
                r.material.clone(),
                r.frame,
                r.ordinal,
                format!("{}\t{}\t{}", r.chain, r.stage.name(), r.band),
            ),
            Row::Leaf(r) => (
                r.op.index(),
                RowType::Leaf.rank(),
                r.material.clone(),
                r.frame,
                r.band,
                r.path.clone(),
            ),
            Row::Replay(r) => (
                0,
                RowType::Replay.rank(),
                String::new(),
                0,
                0,
                r.tag.clone(),
            ),
            Row::Cov(r) => (
                r.op.index(),
                RowType::Cov.rank(),
                r.scope.render(),
                0,
                0,
                r.dimension.clone(),
            ),
        }
    }

    /// Canonical TSV line (tag + body, no line ending).
    pub(super) fn render_tsv(&self) -> String {
        match self {
            Row::Frame(r) => r.render_tsv(),
            Row::Band(r) => r.render_tsv(),
            Row::Chain(r) => r.render_tsv(),
            Row::Leaf(r) => r.render_tsv(),
            Row::Replay(r) => r.render_tsv(),
            Row::Cov(r) => r.render_tsv(),
        }
    }

    /// Parse one TSV data line (header and line endings handled at file
    /// level). Fails closed on any malformed field. Field counts are
    /// per-row-type: FRAME/BAND/COV 8, CHAIN 9, LEAF 7, REPLAY 5.
    pub(super) fn parse_tsv_line(line: &str, line_no: usize) -> Result<Row, RowError> {
        let f: Vec<&str> = line.split('\t').collect();
        if f.is_empty() {
            return Err(RowError::BadFieldCount {
                line: line_no,
                got: 0,
            });
        }
        let tag = RowType::parse(f[0]).ok_or_else(|| RowError::UnknownRowType(f[0].to_string()))?;
        let body = &f[1..];
        let want = match tag {
            RowType::Frame => 7,
            RowType::Band => 7,
            RowType::Chain => 8,
            RowType::Leaf => 6,
            RowType::Replay => 4,
            RowType::Cov => 7,
        };
        if body.len() != want {
            return Err(RowError::BadFieldCount {
                line: line_no,
                got: f.len(),
            });
        }
        match tag {
            RowType::Frame => Ok(Row::Frame(FrameRow::parse_body(body, line_no)?)),
            RowType::Band => Ok(Row::Band(BandRow::parse_body(body, line_no)?)),
            RowType::Chain => Ok(Row::Chain(ChainRow::parse_body(body, line_no)?)),
            RowType::Leaf => Ok(Row::Leaf(LeafRow::parse_body(body, line_no)?)),
            RowType::Replay => Ok(Row::Replay(ReplayRow::parse_body(body, line_no)?)),
            RowType::Cov => Ok(Row::Cov(CovRow::parse_body(body, line_no)?)),
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical vector codec (foundation for E3 per-band vectors)
// ---------------------------------------------------------------------------

/// Render an `i32` slice canonically: comma-joined decimals, no spaces
/// (`1,-2,300`). An empty slice renders as the empty string.
pub(super) fn encode_vec_i32(v: &[i32]) -> String {
    v.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Parse a canonical `i32` vector (inverse of [`encode_vec_i32`]).
/// The empty string is the empty vector. Returns `None` on any malformed
/// element (non-canonical integers, empty elements, whitespace).
pub(super) fn parse_vec_i32(s: &str) -> Option<Vec<i32>> {
    if s.is_empty() {
        return Some(Vec::new());
    }
    if s.starts_with(',') || s.ends_with(',') || s.contains(",,") {
        return None;
    }
    s.split(',').map(parse_canonical_i32).collect()
}

/// Render an optional vector: absent renders as `n/a`.
pub(super) fn encode_vec_opt(v: Option<&[i32]>) -> String {
    match v {
        Some(slice) => encode_vec_i32(slice),
        None => NA_TOKEN.to_string(),
    }
}

/// Parse an optional vector: `n/a` is absent, anything else must be a
/// canonical vector. Outer `None` means malformed.
pub(super) fn parse_vec_opt(s: &str) -> Option<Option<Vec<i32>>> {
    if s == NA_TOKEN {
        Some(None)
    } else {
        parse_vec_i32(s).map(Some)
    }
}

// ---------------------------------------------------------------------------
// Minimal JSON emitter / parser (no new dependencies)
// ---------------------------------------------------------------------------

/// JSON-escape a string value: `"`, `\` and C0 controls escaped, all other
/// UTF-8 passed through raw.
fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

pub(super) fn json_str(s: &str) -> String {
    format!("\"{}\"", escape_json(s))
}

/// Minimal JSON value model for the flat row objects (and the
/// `{"schema":…,"rows":[…]}` envelope) emitted below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum JsonVal {
    Str(String),
    /// Raw canonical-integer text (`-?(0|[1-9][0-9]*)`).
    Num(String),
    Null,
    Arr(Vec<JsonVal>),
    /// Object preserving source key order; duplicate keys rejected.
    Obj(Vec<(String, JsonVal)>),
}

struct JsonParser<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn new(s: &'a str) -> JsonParser<'a> {
        JsonParser {
            b: s.as_bytes(),
            pos: 0,
        }
    }

    fn err(&self, why: &str) -> RowError {
        RowError::BadJson(format!("{why} at byte {}", self.pos))
    }

    fn skip_ws(&mut self) {
        while self.pos < self.b.len() && matches!(self.b[self.pos], b' ' | b'\t' | b'\n' | b'\r') {
            self.pos += 1;
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), RowError> {
        self.skip_ws();
        if self.b.get(self.pos) == Some(&c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected {c:?}")))
        }
    }

    fn parse_value(&mut self) -> Result<JsonVal, RowError> {
        self.skip_ws();
        match self.b.get(self.pos) {
            Some(b'"') => Ok(JsonVal::Str(self.parse_string()?)),
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(c) if *c == b'-' || c.is_ascii_digit() => self.parse_number(),
            _ if self.b[self.pos..].starts_with(b"null") => {
                self.pos += 4;
                Ok(JsonVal::Null)
            }
            _ => Err(self.err("unexpected value")),
        }
    }

    fn parse_string(&mut self) -> Result<String, RowError> {
        // Called with `pos` at the opening quote.
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = *self
                .b
                .get(self.pos)
                .ok_or_else(|| self.err("unterminated string"))?;
            match c {
                b'"' => {
                    self.pos += 1;
                    break;
                }
                b'\\' => {
                    self.pos += 1;
                    let e = *self
                        .b
                        .get(self.pos)
                        .ok_or_else(|| self.err("truncated escape"))?;
                    self.pos += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let hi = self.parse_hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                // High surrogate: require a low surrogate.
                                if self.b.get(self.pos) == Some(&b'\\')
                                    && self.b.get(self.pos + 1) == Some(&b'u')
                                {
                                    self.pos += 2;
                                    let lo = self.parse_hex4()?;
                                    if !(0xDC00..0xE000).contains(&lo) {
                                        return Err(self.err("lone high surrogate"));
                                    }
                                    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                } else {
                                    return Err(self.err("lone high surrogate"));
                                }
                            } else if (0xDC00..0xE000).contains(&hi) {
                                return Err(self.err("lone low surrogate"));
                            } else {
                                hi
                            };
                            let ch =
                                char::from_u32(cp).ok_or_else(|| self.err("invalid code point"))?;
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return Err(self.err("invalid escape")),
                    }
                }
                0x00..=0x1F => return Err(self.err("unescaped control in string")),
                _ => {
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
        String::from_utf8(out).map_err(|_| self.err("invalid UTF-8 in string"))
    }

    fn parse_hex4(&mut self) -> Result<u32, RowError> {
        if self.pos + 4 > self.b.len() {
            return Err(self.err("truncated \\u escape"));
        }
        let mut v: u32 = 0;
        for i in 0..4 {
            let d = self.b[self.pos + i];
            let h = match d {
                b'0'..=b'9' => (d - b'0') as u32,
                b'a'..=b'f' => (d - b'a') as u32 + 10,
                b'A'..=b'F' => (d - b'A') as u32 + 10,
                _ => return Err(self.err("invalid hex in \\u escape")),
            };
            v = v * 16 + h;
        }
        self.pos += 4;
        Ok(v)
    }

    fn parse_number(&mut self) -> Result<JsonVal, RowError> {
        let start = self.pos;
        if self.b.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        let digits = self.pos;
        while self.pos < self.b.len() && self.b[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos == digits {
            return Err(self.err("malformed number"));
        }
        let raw = std::str::from_utf8(&self.b[start..self.pos])
            .map_err(|_| self.err("invalid number"))?;
        // Canonical integers only: no leading zeros, no fractions, no
        // exponents; `-0` is not canonical.
        let canonical = raw != "-0"
            && (raw == "0"
                || (raw.starts_with('-') && {
                    let d = &raw[1..];
                    d.len() == 1 || !d.starts_with('0')
                })
                || (!raw.starts_with('-') && (raw.len() == 1 || !raw.starts_with('0'))));
        if !canonical {
            return Err(self.err("non-canonical number"));
        }
        Ok(JsonVal::Num(raw.to_string()))
    }

    fn parse_array(&mut self) -> Result<JsonVal, RowError> {
        // Called with `pos` at `[`.
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_ws();
        if self.b.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(JsonVal::Arr(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.b.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
        Ok(JsonVal::Arr(items))
    }

    fn parse_object(&mut self) -> Result<JsonVal, RowError> {
        // Called with `pos` at `{`.
        self.pos += 1;
        let mut keys: Vec<(String, JsonVal)> = Vec::new();
        self.skip_ws();
        if self.b.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(JsonVal::Obj(keys));
        }
        loop {
            self.skip_ws();
            if self.b.get(self.pos) != Some(&b'"') {
                return Err(self.err("expected string key"));
            }
            let key = self.parse_string()?;
            if keys.iter().any(|(k, _)| *k == key) {
                return Err(self.err("duplicate object key"));
            }
            self.expect(b':')?;
            let val = self.parse_value()?;
            keys.push((key, val));
            self.skip_ws();
            match self.b.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
        Ok(JsonVal::Obj(keys))
    }
}

pub(super) fn parse_json_toplevel(s: &str) -> Result<JsonVal, RowError> {
    let mut p = JsonParser::new(s);
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.b.len() {
        return Err(p.err("trailing content"));
    }
    Ok(v)
}

fn json_get_str(fields: &[(String, JsonVal)], key: &str) -> Result<String, RowError> {
    match fields.iter().find(|(k, _)| k == key) {
        Some((_, JsonVal::Str(s))) => Ok(s.clone()),
        Some(_) => Err(RowError::BadJson(format!("{key} must be a string"))),
        None => Err(RowError::BadJson(format!("missing key {key:?}"))),
    }
}

fn json_get_num(fields: &[(String, JsonVal)], key: &str) -> Result<String, RowError> {
    match fields.iter().find(|(k, _)| k == key) {
        Some((_, JsonVal::Num(n))) => Ok(n.clone()),
        Some(_) => Err(RowError::BadJson(format!("{key} must be a number"))),
        None => Err(RowError::BadJson(format!("missing key {key:?}"))),
    }
}

fn json_require_keys(fields: &[(String, JsonVal)], want: &[&str]) -> Result<(), RowError> {
    if fields.len() != want.len() || !want.iter().all(|k| fields.iter().any(|(f, _)| f == k)) {
        return Err(RowError::BadJson(format!("expected exactly keys {want:?}")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// JSON row rendering (fixed key order = TSV field order + type/id)
// ---------------------------------------------------------------------------

impl FrameRow {
    /// Canonical JSON object with fixed key order
    /// `type id op material frame seq outcome class margin` — the same
    /// semantic values as [`FrameRow::render_tsv`].
    pub(super) fn render_json(&self) -> String {
        self.validate(0)
            .expect("FrameRow::render_json: invalid row");
        format!(
            "{{\"type\":{},\"id\":{},\"op\":{},\"material\":{},\"frame\":{},\"seq\":{},\"outcome\":{},\"class\":{},\"margin\":{}}}",
            json_str(RowType::Frame.tag()),
            json_str(&self.id()),
            json_str(self.op.label()),
            json_str(&self.material),
            self.frame,
            self.seq,
            json_str(&self.outcome.render()),
            json_str(&self.class.render()),
            json_str(&self.margin.render())
        )
    }

    fn parse_json(fields: &[(String, JsonVal)]) -> Result<FrameRow, RowError> {
        json_require_keys(
            fields,
            &[
                "type", "id", "op", "material", "frame", "seq", "outcome", "class", "margin",
            ],
        )?;
        let id = json_get_str(fields, "id")?;
        let op = Op::parse(&json_get_str(fields, "op")?)
            .ok_or_else(|| RowError::BadJson("bad op".to_string()))?;
        let material = json_get_str(fields, "material")?;
        let frame: u32 = json_get_num(fields, "frame")?
            .parse()
            .map_err(|_| RowError::BadJson("bad frame".to_string()))?;
        let seq: i32 = json_get_num(fields, "seq")?
            .parse()
            .map_err(|_| RowError::BadJson("bad seq".to_string()))?;
        let outcome = OutcomeField::parse(&json_get_str(fields, "outcome")?)
            .ok_or_else(|| RowError::BadJson("bad outcome".to_string()))?;
        let class = ClassField::parse(&json_get_str(fields, "class")?)
            .ok_or_else(|| RowError::BadJson("bad class".to_string()))?;
        let margin = MarginField::parse(&json_get_str(fields, "margin")?)
            .ok_or_else(|| RowError::BadJson("bad margin".to_string()))?;
        let row = FrameRow {
            op,
            material,
            frame,
            seq,
            outcome,
            class,
            margin,
        };
        row.validate(0).map_err(|e| match e {
            RowError::BadToken { field, value, .. } => RowError::BadToken {
                line: 0,
                field,
                value,
            },
            RowError::OutOfRange { field, value, .. } => RowError::OutOfRange {
                line: 0,
                field,
                value,
            },
            other => RowError::BadJson(format!("invalid frame row: {other}")),
        })?;
        if row.id() != id {
            return Err(RowError::BadJson(format!(
                "id mismatch: {:?} vs {:?}",
                id,
                row.id()
            )));
        }
        Ok(row)
    }
}

impl BandRow {
    /// Canonical JSON object with fixed key order
    /// `type id op material frame band stage class margin`.
    pub(super) fn render_json(&self) -> String {
        self.validate(0).expect("BandRow::render_json: invalid row");
        format!(
            "{{\"type\":{},\"id\":{},\"op\":{},\"material\":{},\"frame\":{},\"band\":{},\"stage\":{},\"class\":{},\"margin\":{}}}",
            json_str(RowType::Band.tag()),
            json_str(&self.id()),
            json_str(self.op.label()),
            json_str(&self.material),
            self.frame,
            self.band,
            json_str(self.stage.name()),
            json_str(&self.class.render()),
            json_str(&self.margin.render())
        )
    }

    fn parse_json(fields: &[(String, JsonVal)]) -> Result<BandRow, RowError> {
        json_require_keys(
            fields,
            &[
                "type", "id", "op", "material", "frame", "band", "stage", "class", "margin",
            ],
        )?;
        let id = json_get_str(fields, "id")?;
        let op = Op::parse(&json_get_str(fields, "op")?)
            .ok_or_else(|| RowError::BadJson("bad op".to_string()))?;
        let material = json_get_str(fields, "material")?;
        let frame: u32 = json_get_num(fields, "frame")?
            .parse()
            .map_err(|_| RowError::BadJson("bad frame".to_string()))?;
        let band: u32 = json_get_num(fields, "band")?
            .parse()
            .map_err(|_| RowError::BadJson("bad band".to_string()))?;
        let stage = parse_stage_token(&json_get_str(fields, "stage")?)
            .ok_or_else(|| RowError::BadJson("bad stage".to_string()))?;
        let class = ClassField::parse(&json_get_str(fields, "class")?)
            .ok_or_else(|| RowError::BadJson("bad class".to_string()))?;
        let margin = MarginField::parse(&json_get_str(fields, "margin")?)
            .ok_or_else(|| RowError::BadJson("bad margin".to_string()))?;
        let row = BandRow {
            op,
            material,
            frame,
            band,
            stage,
            class,
            margin,
        };
        row.validate(0).map_err(|e| match e {
            RowError::BadToken { field, value, .. } => RowError::BadToken {
                line: 0,
                field,
                value,
            },
            RowError::OutOfRange { field, value, .. } => RowError::OutOfRange {
                line: 0,
                field,
                value,
            },
            other => RowError::BadJson(format!("invalid band row: {other}")),
        })?;
        if row.id() != id {
            return Err(RowError::BadJson(format!(
                "id mismatch: {:?} vs {:?}",
                id,
                row.id()
            )));
        }
        Ok(row)
    }
}

impl CovRow {
    /// Canonical JSON object with fixed key order
    /// `type id op dimension scope status observed required detail`
    /// (`detail: null` is not used; absent detail renders as `"n/a"`).
    pub(super) fn render_json(&self) -> String {
        self.validate(0).expect("CovRow::render_json: invalid row");
        format!(
            "{{\"type\":{},\"id\":{},\"op\":{},\"dimension\":{},\"scope\":{},\"status\":{},\"observed\":{},\"required\":{},\"detail\":{}}}",
            json_str(RowType::Cov.tag()),
            json_str(&self.id()),
            json_str(self.op.label()),
            json_str(&self.dimension),
            json_str(&self.scope.render()),
            json_str(&self.status.token()),
            self.observed,
            self.required,
            json_str(self.detail.as_deref().unwrap_or(NA_TOKEN))
        )
    }

    fn parse_json(fields: &[(String, JsonVal)]) -> Result<CovRow, RowError> {
        json_require_keys(
            fields,
            &[
                "type",
                "id",
                "op",
                "dimension",
                "scope",
                "status",
                "observed",
                "required",
                "detail",
            ],
        )?;
        let id = json_get_str(fields, "id")?;
        let op = Op::parse(&json_get_str(fields, "op")?)
            .ok_or_else(|| RowError::BadJson("bad op".to_string()))?;
        let dimension = json_get_str(fields, "dimension")?;
        let scope = Scope::parse(&json_get_str(fields, "scope")?)
            .ok_or_else(|| RowError::BadJson("bad scope".to_string()))?;
        let status = CoverageStatus::parse(&json_get_str(fields, "status")?)
            .ok_or_else(|| RowError::BadJson("bad status".to_string()))?;
        let observed: u32 = json_get_num(fields, "observed")?
            .parse()
            .map_err(|_| RowError::BadJson("bad observed".to_string()))?;
        let required: u32 = json_get_num(fields, "required")?
            .parse()
            .map_err(|_| RowError::BadJson("bad required".to_string()))?;
        let detail_raw = json_get_str(fields, "detail")?;
        let detail = if detail_raw == NA_TOKEN {
            None
        } else {
            Some(detail_raw)
        };
        let row = CovRow {
            op,
            dimension,
            scope,
            status,
            observed,
            required,
            detail,
        };
        row.validate(0).map_err(|e| match e {
            RowError::BadToken { field, value, .. } => RowError::BadToken {
                line: 0,
                field,
                value,
            },
            RowError::OutOfRange { field, value, .. } => RowError::OutOfRange {
                line: 0,
                field,
                value,
            },
            other => RowError::BadJson(format!("invalid coverage row: {other}")),
        })?;
        if row.id() != id {
            return Err(RowError::BadJson(format!(
                "id mismatch: {:?} vs {:?}",
                id,
                row.id()
            )));
        }
        Ok(row)
    }
}

impl ChainRow {
    /// Canonical JSON object with fixed key order
    /// `type id op material frame chain ordinal stage band origin`.
    pub(super) fn render_json(&self) -> String {
        self.validate(0)
            .expect("ChainRow::render_json: invalid row");
        format!(
            "{{\"type\":{},\"id\":{},\"op\":{},\"material\":{},\"frame\":{},\"chain\":{},\"ordinal\":{},\"stage\":{},\"band\":{},\"origin\":{}}}",
            json_str(RowType::Chain.tag()),
            json_str(&self.id()),
            json_str(self.op.label()),
            json_str(&self.material),
            self.frame,
            json_str(&self.chain),
            self.ordinal,
            json_str(self.stage.name()),
            self.band,
            json_str(&self.origin.render())
        )
    }

    fn parse_json(fields: &[(String, JsonVal)]) -> Result<ChainRow, RowError> {
        json_require_keys(
            fields,
            &[
                "type", "id", "op", "material", "frame", "chain", "ordinal", "stage", "band",
                "origin",
            ],
        )?;
        let id = json_get_str(fields, "id")?;
        let op = Op::parse(&json_get_str(fields, "op")?)
            .ok_or_else(|| RowError::BadJson("bad op".to_string()))?;
        let material = json_get_str(fields, "material")?;
        let frame: u32 = json_get_num(fields, "frame")?
            .parse()
            .map_err(|_| RowError::BadJson("bad frame".to_string()))?;
        let chain = json_get_str(fields, "chain")?;
        let ordinal: u32 = json_get_num(fields, "ordinal")?
            .parse()
            .map_err(|_| RowError::BadJson("bad ordinal".to_string()))?;
        let stage = parse_stage_token(&json_get_str(fields, "stage")?)
            .ok_or_else(|| RowError::BadJson("bad stage".to_string()))?;
        let band: u32 = json_get_num(fields, "band")?
            .parse()
            .map_err(|_| RowError::BadJson("bad band".to_string()))?;
        let origin = ChainOrigin::parse(&json_get_str(fields, "origin")?)
            .ok_or_else(|| RowError::BadJson("bad origin".to_string()))?;
        let row = ChainRow {
            op,
            material,
            frame,
            chain,
            ordinal,
            stage,
            band,
            origin,
        };
        row.validate(0).map_err(|e| match e {
            RowError::BadToken { field, value, .. } => RowError::BadToken {
                line: 0,
                field,
                value,
            },
            RowError::OutOfRange { field, value, .. } => RowError::OutOfRange {
                line: 0,
                field,
                value,
            },
            other => RowError::BadJson(format!("invalid chain row: {other}")),
        })?;
        if row.id() != id {
            return Err(RowError::BadJson(format!(
                "id mismatch: {id:?} vs {:?}",
                row.id()
            )));
        }
        Ok(row)
    }
}

impl LeafRow {
    /// Canonical JSON object with fixed key order
    /// `type id op material frame band path value` (`value` is a number or
    /// the string `"n/a"` for absent leaves).
    pub(super) fn render_json(&self) -> String {
        self.validate(0).expect("LeafRow::render_json: invalid row");
        let value = match self.value {
            Some(v) => v.to_string(),
            None => json_str(NA_TOKEN),
        };
        format!(
            "{{\"type\":{},\"id\":{},\"op\":{},\"material\":{},\"frame\":{},\"band\":{},\"path\":{},\"value\":{}}}",
            json_str(RowType::Leaf.tag()),
            json_str(&self.id()),
            json_str(self.op.label()),
            json_str(&self.material),
            self.frame,
            self.band,
            json_str(&self.path),
            value
        )
    }

    fn parse_json(fields: &[(String, JsonVal)]) -> Result<LeafRow, RowError> {
        json_require_keys(
            fields,
            &[
                "type", "id", "op", "material", "frame", "band", "path", "value",
            ],
        )?;
        let id = json_get_str(fields, "id")?;
        let op = Op::parse(&json_get_str(fields, "op")?)
            .ok_or_else(|| RowError::BadJson("bad op".to_string()))?;
        let material = json_get_str(fields, "material")?;
        let frame: u32 = json_get_num(fields, "frame")?
            .parse()
            .map_err(|_| RowError::BadJson("bad frame".to_string()))?;
        let band: u32 = json_get_num(fields, "band")?
            .parse()
            .map_err(|_| RowError::BadJson("bad band".to_string()))?;
        let path = json_get_str(fields, "path")?;
        let value = match fields.iter().find(|(k, _)| k == "value") {
            Some((_, JsonVal::Num(n))) => Some(
                n.parse::<i32>()
                    .map_err(|_| RowError::BadJson("bad value".to_string()))?,
            ),
            Some((_, JsonVal::Str(s))) if s == NA_TOKEN => None,
            _ => return Err(RowError::BadJson("bad value".to_string())),
        };
        let row = LeafRow {
            op,
            material,
            frame,
            band,
            path,
            value,
        };
        row.validate(0).map_err(|e| match e {
            RowError::BadToken { field, value, .. } => RowError::BadToken {
                line: 0,
                field,
                value,
            },
            RowError::OutOfRange { field, value, .. } => RowError::OutOfRange {
                line: 0,
                field,
                value,
            },
            other => RowError::BadJson(format!("invalid leaf row: {other}")),
        })?;
        if row.id() != id {
            return Err(RowError::BadJson(format!(
                "id mismatch: {id:?} vs {:?}",
                row.id()
            )));
        }
        Ok(row)
    }
}

impl ReplayRow {
    /// Canonical JSON object with fixed key order
    /// `type id tag kind digest status` (no OP/material/frame by design).
    pub(super) fn render_json(&self) -> String {
        self.validate(0)
            .expect("ReplayRow::render_json: invalid row");
        format!(
            "{{\"type\":{},\"id\":{},\"tag\":{},\"kind\":{},\"digest\":{},\"status\":{}}}",
            json_str(RowType::Replay.tag()),
            json_str(&self.id()),
            json_str(&self.tag),
            json_str(&self.kind),
            json_str(&self.digest),
            json_str(self.status.token())
        )
    }

    fn parse_json(fields: &[(String, JsonVal)]) -> Result<ReplayRow, RowError> {
        json_require_keys(fields, &["type", "id", "tag", "kind", "digest", "status"])?;
        let id = json_get_str(fields, "id")?;
        let tag = json_get_str(fields, "tag")?;
        let kind = json_get_str(fields, "kind")?;
        let digest = json_get_str(fields, "digest")?;
        let status = ReplayStatus::parse(&json_get_str(fields, "status")?)
            .ok_or_else(|| RowError::BadJson("bad status".to_string()))?;
        let row = ReplayRow {
            tag,
            kind,
            digest,
            status,
        };
        row.validate(0).map_err(|e| match e {
            RowError::BadToken { field, value, .. } => RowError::BadToken {
                line: 0,
                field,
                value,
            },
            RowError::OutOfRange { field, value, .. } => RowError::OutOfRange {
                line: 0,
                field,
                value,
            },
            other => RowError::BadJson(format!("invalid replay row: {other}")),
        })?;
        if row.id() != id {
            return Err(RowError::BadJson(format!(
                "id mismatch: {id:?} vs {:?}",
                row.id()
            )));
        }
        Ok(row)
    }
}

impl Row {
    pub(super) fn json_string(fields: &[(String, JsonVal)], key: &str) -> Result<String, RowError> {
        json_get_str(fields, key)
    }

    /// Canonical JSON object (dispatches on the row type).
    pub(super) fn render_json(&self) -> String {
        match self {
            Row::Frame(r) => r.render_json(),
            Row::Band(r) => r.render_json(),
            Row::Chain(r) => r.render_json(),
            Row::Leaf(r) => r.render_json(),
            Row::Replay(r) => r.render_json(),
            Row::Cov(r) => r.render_json(),
        }
    }

    /// Parse one JSON row object through the JSON path only (never via
    /// the TSV parser — the two formats are independently parseable).
    pub(super) fn parse_json_object(s: &str) -> Result<Row, RowError> {
        let v = parse_json_toplevel(s)?;
        let fields = match v {
            JsonVal::Obj(f) => f,
            _ => return Err(RowError::BadJson("row must be an object".to_string())),
        };
        let tag = json_get_str(&fields, "type")?;
        match tag.as_str() {
            "R22-FRAME" => Ok(Row::Frame(FrameRow::parse_json(&fields)?)),
            "R22-BAND" => Ok(Row::Band(BandRow::parse_json(&fields)?)),
            "R22-CHAIN" => Ok(Row::Chain(ChainRow::parse_json(&fields)?)),
            "R22-LEAF" => Ok(Row::Leaf(LeafRow::parse_json(&fields)?)),
            "R22-REPLAY" => Ok(Row::Replay(ReplayRow::parse_json(&fields)?)),
            "R22-COV" => Ok(Row::Cov(CovRow::parse_json(&fields)?)),
            _ => Err(RowError::UnknownRowType(tag)),
        }
    }
}

// ---------------------------------------------------------------------------
// RowSet: unique-ID enforcement + deterministic file render/parse
// ---------------------------------------------------------------------------

/// An ordered, duplicate-free set of evidence rows. Storage is a `BTreeMap`
/// keyed by the total-order key (no `HashMap` iteration anywhere); render
/// always emits total order, so the same set always renders byte-identically.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct RowSet {
    rows: BTreeMap<(u8, u8, String, u32, u32, String), Row>,
}

impl RowSet {
    pub(super) fn new() -> RowSet {
        RowSet {
            rows: BTreeMap::new(),
        }
    }

    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Insert a row; a duplicate ID is rejected (never silently merged).
    pub(super) fn insert(&mut self, row: Row) -> Result<(), RowError> {
        let id = row.id();
        if self.rows.contains_key(&row.sort_key()) {
            return Err(RowError::DuplicateId(id));
        }
        self.rows.insert(row.sort_key(), row);
        Ok(())
    }

    /// Rows in total order.
    pub(super) fn rows_in_order(&self) -> Vec<&Row> {
        self.rows.values().collect()
    }

    /// Canonical TSV file: schema-version header, then rows in total order,
    /// LF endings, exactly one trailing newline.
    pub(super) fn render_tsv_file(&self) -> String {
        let mut out = String::from(SCHEMA_HEADER);
        out.push('\n');
        for row in self.rows.values() {
            out.push_str(&row.render_tsv());
            out.push('\n');
        }
        out
    }

    /// Parse a canonical TSV file. Rejects: empty input, missing/wrong
    /// header, unknown schema version, any CR byte, a missing trailing
    /// newline, empty lines, malformed rows and duplicate IDs.
    pub(super) fn parse_tsv_file(text: &str) -> Result<RowSet, RowError> {
        if text.is_empty() {
            return Err(RowError::EmptyFile);
        }
        if text.contains('\r') {
            return Err(RowError::CarriageReturn);
        }
        if !text.ends_with('\n') {
            return Err(RowError::MissingTrailingNewline);
        }
        let mut lines: Vec<&str> = text.split('\n').collect();
        // `ends_with('\n')` guarantees the final split element is empty.
        lines.pop();
        if lines.is_empty() {
            return Err(RowError::MissingHeader);
        }
        let header = lines[0];
        if header == SCHEMA_HEADER {
            // Supported version.
        } else if header.starts_with("R22SCHEMA\t") {
            return Err(RowError::UnknownSchemaVersion(header.to_string()));
        } else {
            return Err(RowError::MissingHeader);
        }
        let mut set = RowSet::new();
        for (i, line) in lines.iter().enumerate().skip(1) {
            let line_no = i + 1; // 1-based file line number.
            if line.is_empty() {
                return Err(RowError::EmptyLine(line_no));
            }
            let row = Row::parse_tsv_line(line, line_no)?;
            set.insert(row)?;
        }
        Ok(set)
    }

    /// Canonical JSON envelope: `{"schema":2,"rows":[…]}` in total order,
    /// single line plus one trailing newline. (E5 partitions by OP ×
    /// row type; E1 establishes the row-level capability.)
    pub(super) fn render_json_file(&self) -> String {
        let mut out = String::from("{\"schema\":2,\"rows\":[");
        for (i, row) in self.rows.values().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&row.render_json());
        }
        out.push_str("]}\n");
        out
    }

    /// Parse a JSON envelope through the JSON path only (independent of
    /// the TSV parser). Requires `schema: 2`; anything else is an
    /// unknown-version rejection.
    pub(super) fn parse_json_file(text: &str) -> Result<RowSet, RowError> {
        let v = parse_json_toplevel(text)?;
        let fields = match v {
            JsonVal::Obj(f) => f,
            _ => return Err(RowError::BadJson("envelope must be an object".to_string())),
        };
        json_require_keys(&fields, &["schema", "rows"])?;
        let schema = json_get_num(&fields, "schema")?;
        if schema != SCHEMA_VERSION.to_string() {
            return Err(RowError::UnknownSchemaVersion(format!("schema {schema}")));
        }
        let rows = match fields.iter().find(|(k, _)| k == "rows") {
            Some((_, JsonVal::Arr(a))) => a,
            _ => return Err(RowError::BadJson("rows must be an array".to_string())),
        };
        let mut set = RowSet::new();
        for item in rows {
            let obj = match item {
                JsonVal::Obj(f) => f,
                _ => return Err(RowError::BadJson("row must be an object".to_string())),
            };
            let tag = json_get_str(obj, "type")?;
            let row = match tag.as_str() {
                "R22-FRAME" => Row::Frame(FrameRow::parse_json(obj)?),
                "R22-BAND" => Row::Band(BandRow::parse_json(obj)?),
                "R22-CHAIN" => Row::Chain(ChainRow::parse_json(obj)?),
                "R22-LEAF" => Row::Leaf(LeafRow::parse_json(obj)?),
                "R22-REPLAY" => Row::Replay(ReplayRow::parse_json(obj)?),
                "R22-COV" => Row::Cov(CovRow::parse_json(obj)?),
                _ => return Err(RowError::UnknownRowType(tag)),
            };
            set.insert(row)?;
        }
        Ok(set)
    }
}

// ---------------------------------------------------------------------------
// E2/E3 total-ordering specification (F-A obligation)
//
// The abbreviated module note above is superseded for E3 by this explicit
// specification. The frozen E1 contract is unchanged; this section documents
// it completely so E3 population cannot drift.
//
// Total order (load-bearing for byte-identical regeneration). Every row maps
// to a six-component key; comparison is lexicographic on the tuple:
//
//   1. OP index (u8): REPLAY rows use 0; FRAME/BAND/CHAIN/LEAF/COV use the
//      numeric OP index 1..=6 (`Op::index`). Numeric, never lexical (`OP-10`
//      hazards do not exist but numeric order is still load-bearing).
//   2. Row-type rank (u8): FRAME 0 < BAND 1 < CHAIN 2 < LEAF 3 < REPLAY 4 <
//      COV 5 (`RowType::rank`). Rank separates row families so corpus or OP
//      selection never interleaves types.
//   3. Material-or-scope (String, byte-lexicographic on the canonical id):
//      FRAME/BAND/CHAIN/LEAF use the material id; COV uses `Scope::render`
//      (`*` for whole-OP aggregates sorts before any material id since
//      `*` (0x2A) < alphanumerics); REPLAY uses the empty string (all REPLAY
//      rows share components 1–3, so they sort purely by component 6).
//   4. Frame (u32, numeric): FRAME/BAND/CHAIN/LEAF frame index; COV/REPLAY 0.
//      Zero-padded `:06` rendering keeps lexicographic file order identical
//      to numeric order, but the key itself is numeric (no string-sort risk
//      at 100000+ frames; bound fails closed at 999999).
//   5. Band-or-ordinal (u32, numeric): BAND/LEAF band `:03`; CHAIN ordinal
//      `:04`; others 0. Band 2 sorts before band 10 numerically.
//   6. Trailing key (String, byte-lexicographic): CHAIN
//      `chain \t stage \t band`; LEAF path; COV dimension; REPLAY tag;
//      otherwise empty. This is the only string-compared numeric content;
//      all embedded numbers here are names (chain ids, tags, dimensions),
//      never frame/band ordinals.
//
// Consequences: REPLAY rows sort purely by tag (corpus/OP-independent, so
// REPLAY rows never shift with corpus or OP selection); COV rows sort by
// (OP, scope, dimension); FRAME/BAND/CHAIN/LEAF sort by
// (OP, material, frame, band/ordinal, trailing). `RowSet` stores rows in a
// `BTreeMap` keyed by this tuple — no `HashMap` iteration anywhere — and
// rejects duplicate IDs. Render always emits total order; the same set
// always renders byte-identically (TSV and JSON share IDs, values, and
// order; JSON key order is fixed per row type).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// E2/E3 bundle writer (E-OD-4 env gating, SG-1..SG-3, F-B durability)
//
// Test-only. No schema, contract, production, API, threshold, tolerance,
// envelope, baseline, or pin change. Reuses the reviewed Group C/D functions
// through `pub(super)` visibility only (E-OD-1); no second replay/diff/
// completeness implementation. SHA-256 reuses the reviewed Group B
// `super::r22_capture::sha256_hex` (no new crypto, no new dependency).
// ---------------------------------------------------------------------------

/// Bundle file names (stable, documented in the bundle README).
pub(super) const BUNDLE_TSV_NAME: &str = "r22-bundle.tsv";
pub(super) const BUNDLE_JSON_NAME: &str = "r22-bundle.json";
pub(super) const MANIFEST_NAME: &str = "MANIFEST.txt";
pub(super) const SHA_NAME: &str = "SHA256SUMS";
pub(super) const RESOURCE_NAME: &str = "resource-report.txt";
pub(super) const BUNDLE_README_NAME: &str = "README.md";

/// Environment gate for generation (E-OD-4). Bundle files are written only
/// when `FPE_R22_GENERATE=1`; the default invocation is read-only
/// verification against the committed bundle. Accidental regeneration or
/// overwrite without the gate is not possible.
pub(super) const BUNDLE_ENV_GATE: &str = "FPE_R22_GENERATE";

/// Bundle directory: `docs/evidence/fpe-r22/` relative to the repository
/// root (resolved from `CARGO_MANIFEST_DIR`, which is `rust/` for this
/// crate). Tests run with CWD inside `rust/`; this path does not depend on
/// CWD.
pub(super) fn bundle_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/evidence/fpe-r22")
}

/// True only when the explicit generation gate is set (`=1`). Any other
/// value (including unset) means read-only verification mode.
pub(super) fn env_generate_enabled() -> bool {
    std::env::var(BUNDLE_ENV_GATE)
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// SHA-256 hex of bytes, via the reviewed Group B implementation (E-OD-1
/// reuse; no second hash implementation).
pub(super) fn sha256_of(bytes: &[u8]) -> String {
    super::r22_capture::sha256_hex(bytes)
}

/// `fsync` a directory (F-B durability: a crash between file `fsync` and
/// directory entry persistence must not lose the bundle). Unix: open the
/// directory and `sync_all`. Other platforms: best-effort Ok (documented in
/// the bundle README; the file-level `fsync` still holds).
pub(super) fn dir_fsync(dir: &std::path::Path) -> Result<(), StagingError> {
    #[cfg(unix)]
    {
        let f = std::fs::File::open(dir)
            .map_err(|e| StagingError::Unavailable(format!("open dir: {e}")))?;
        f.sync_all()
            .map_err(|e| StagingError::Unavailable(format!("dir fsync: {e}")))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

/// Durably write `bytes` to `dir/name`: create parents, write, flush,
/// file-`fsync`, then directory-`fsync` (F-B). `name` may contain a single
/// `json/` subdirectory prefix for the E-OD-8 sidecar; `..`, absolute
/// paths, empty components, and embedded tabs/newlines are rejected.
/// Returns the file path. No timestamp or process-id is embedded; the same
/// bytes always produce the same file.
pub(super) fn write_file_durable(
    dir: &std::path::Path,
    name: &str,
    bytes: &[u8],
) -> Result<std::path::PathBuf, StagingError> {
    if name.is_empty() || name.starts_with('/') || name.contains("..") {
        return Err(StagingError::Unavailable(format!("bad file name {name:?}")));
    }
    for comp in name.split('/') {
        if comp.is_empty() || !token_chars_ok(comp) {
            return Err(StagingError::Unavailable(format!("bad file name {name:?}")));
        }
    }
    if name.contains('/') && !name.starts_with("json/") {
        return Err(StagingError::Unavailable(format!("bad file name {name:?}")));
    }
    std::fs::create_dir_all(dir).map_err(|e| StagingError::Unavailable(format!("mkdir: {e}")))?;
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| StagingError::Unavailable(format!("mkdir parent: {e}")))?;
    }
    let mut file = std::fs::File::create(&path)
        .map_err(|e| StagingError::Unavailable(format!("create: {e}")))?;
    use std::io::Write;
    file.write_all(bytes)
        .map_err(|e| StagingError::Unavailable(format!("write: {e}")))?;
    file.flush()
        .map_err(|e| StagingError::Unavailable(format!("flush: {e}")))?;
    file.sync_all()
        .map_err(|e| StagingError::Unavailable(format!("fsync: {e}")))?;
    drop(file);
    // fsync the file's parent directory (the bundle dir itself for top-level
    // files, `json/` for partitioned sidecar files), then the bundle dir.
    if let Some(parent) = path.parent() {
        let _ = dir_fsync(parent);
    }
    dir_fsync(dir)?;
    Ok(path)
}

/// Deterministic MANIFEST (no timestamps, no PIDs, LF-only, trailing
/// newline). Records schema, HEAD at generation (provenance, not asserted on
/// verify), reference platform, generation command, per-file bytes + SHA-256,
/// and row counts. The manifest itself is checksummed in SHA256SUMS (as
/// `MANIFEST.txt`); verification re-hashes every listed file.
pub(super) fn manifest_text(
    head: &str,
    platform: &str,
    files: &[(&str, usize, &str)],
    tsv_rows: usize,
    json_rows: usize,
) -> String {
    let mut out = String::from("R22MANIFEST\t2\n");
    out.push_str(&format!("schema: 2\nhead: {head}\nplatform: {platform}\n"));
    out.push_str(
        "command: FPE_R22_GENERATE=1 cargo test -p ferrite-aac --lib fpe::tests::e2e3_bundle\n",
    );
    out.push_str(&format!(
        "tsv_rows: {tsv_rows}\njson_rows: {json_rows}\nfiles:\n"
    ));
    for (name, bytes, sha) in files {
        out.push_str(&format!("{sha}  {name}  {bytes}\n"));
    }
    out
}

/// Deterministic SHA256SUMS (sorted by file name, LF-only, trailing
/// newline; `<sha>  <name>` per line).
pub(super) fn shasums_text(files: &[(&str, String)]) -> String {
    let mut sorted: Vec<(&str, &String)> = files.iter().map(|(n, s)| (*n, s)).collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = String::new();
    for (name, sha) in sorted {
        out.push_str(&format!("{sha}  {name}\n"));
    }
    out
}

/// Partition a `RowSet` by OP × row type for the `json/` sidecar (E-OD-8).
/// Returns `(op_label_or_REPLAY, row_type_tag, rows_in_total_order)`.
/// REPLAY rows are corpus/OP-independent and form one partition keyed by
/// tag order. Within each partition rows keep total order.
pub(super) fn partition_op_rowtype(set: &RowSet) -> Vec<(String, String, Vec<Row>)> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<(String, String), Vec<Row>> = BTreeMap::new();
    for row in set.rows_in_order() {
        let (op_key, type_tag) = match row {
            Row::Frame(r) => (r.op.label().to_string(), "R22-FRAME".to_string()),
            Row::Band(r) => (r.op.label().to_string(), "R22-BAND".to_string()),
            Row::Chain(r) => (r.op.label().to_string(), "R22-CHAIN".to_string()),
            Row::Leaf(r) => (r.op.label().to_string(), "R22-LEAF".to_string()),
            Row::Replay(_) => ("REPLAY".to_string(), "R22-REPLAY".to_string()),
            Row::Cov(r) => (r.op.label().to_string(), "R22-COV".to_string()),
        };
        map.entry((op_key, type_tag))
            .or_default()
            .push((*row).clone());
    }
    map.into_iter()
        .map(|((op, ty), rows)| (op, ty, rows))
        .collect()
}

/// Render one JSON partition file (same envelope as the single-file JSON,
/// filtered rows in total order, single line + trailing newline).
pub(super) fn render_json_partition(rows: &[Row]) -> String {
    let mut out = String::from("{\"schema\":2,\"rows\":[");
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&row.render_json());
    }
    out.push_str("]}\n");
    out
}

/// Verify a bundle directory in read-only mode (E-OD-4 default): parse TSV
/// + JSON (both paths independently), assert TSV/JSON semantic equality,
/// parse every `json/` partition and assert the partition union equals the
/// TSV set, re-hash every file against SHA256SUMS, and assert the manifest
/// lists every content file (all SHA entries except MANIFEST itself) with
/// matching sizes + hashes. No file is written, created, or modified.
pub(super) fn verify_bundle_dir(dir: &std::path::Path) -> Result<usize, String> {
    let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| format!("read {name}: {e}"));
    let tsv_bytes = read(BUNDLE_TSV_NAME)?;
    let json_bytes = read(BUNDLE_JSON_NAME)?;
    let manifest_bytes = read(MANIFEST_NAME)?;
    let shas_bytes = read(SHA_NAME)?;
    let tsv_text = std::str::from_utf8(&tsv_bytes).map_err(|e| format!("tsv utf8: {e}"))?;
    let json_text = std::str::from_utf8(&json_bytes).map_err(|e| format!("json utf8: {e}"))?;
    let manifest =
        std::str::from_utf8(&manifest_bytes).map_err(|e| format!("manifest utf8: {e}"))?;
    let shas = std::str::from_utf8(&shas_bytes).map_err(|e| format!("shas utf8: {e}"))?;
    let via_tsv = RowSet::parse_tsv_file(tsv_text).map_err(|e| format!("tsv parse: {e}"))?;
    let via_json = RowSet::parse_json_file(json_text).map_err(|e| format!("json parse: {e}"))?;
    if via_tsv != via_json {
        return Err("tsv/json semantic mismatch".to_string());
    }
    // SHA256SUMS: every line `<sha>  <name>` must match the file on disk.
    // The manifest itself is covered (listed in SHA256SUMS); SHA256SUMS
    // never lists itself.
    let mut expected_files: Vec<String> = Vec::new();
    for (i, line) in shas.split('\n').enumerate() {
        if line.is_empty() {
            if i + 1 == shas.split('\n').count() {
                continue;
            }
            return Err(format!("shasums: empty line {}", i + 1));
        }
        let mut it = line.splitn(2, "  ");
        let sha = it
            .next()
            .ok_or_else(|| format!("shasums line {} malformed", i + 1))?;
        let name = it
            .next()
            .ok_or_else(|| format!("shasums line {} malformed", i + 1))?;
        if name == SHA_NAME {
            return Err("shasums must not list itself".to_string());
        }
        let bytes = read(name)?;
        let got = sha256_of(&bytes);
        if got != sha {
            return Err(format!("sha mismatch for {name}"));
        }
        expected_files.push(name.to_string());
    }
    if !expected_files.contains(&BUNDLE_TSV_NAME.to_string()) {
        return Err("shasums missing tsv".to_string());
    }
    if !expected_files.contains(&BUNDLE_JSON_NAME.to_string()) {
        return Err("shasums missing json".to_string());
    }
    if !expected_files.contains(&MANIFEST_NAME.to_string()) {
        return Err("shasums missing manifest".to_string());
    }
    // Manifest must list every content file (all SHA entries except the
    // manifest itself) with matching size + hash.
    if !manifest.starts_with("R22MANIFEST\t2\n") {
        return Err("manifest header".to_string());
    }
    for name in &expected_files {
        if name == MANIFEST_NAME {
            continue;
        }
        let bytes = read(name)?;
        let sha = sha256_of(&bytes);
        let needle = format!("{sha}  {name}  {}", bytes.len());
        if !manifest.contains(&needle) {
            return Err(format!("manifest missing entry for {name}"));
        }
    }
    // Every `json/` partition must parse independently and the union must
    // equal the TSV set (E-OD-8 sidecar consistency).
    let mut union = RowSet::new();
    let mut partitions = 0usize;
    for name in &expected_files {
        if !name.starts_with("json/") {
            continue;
        }
        let bytes = read(name)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| format!("{name} utf8: {e}"))?;
        let part = RowSet::parse_json_file(text).map_err(|e| format!("{name} parse: {e}"))?;
        for row in part.rows_in_order() {
            union
                .insert((*row).clone())
                .map_err(|e| format!("{name} dup: {e}"))?;
        }
        partitions += 1;
    }
    if partitions == 0 {
        return Err("no json partitions".to_string());
    }
    if union != via_tsv {
        return Err("json partition union mismatch".to_string());
    }
    Ok(via_tsv.len())
}

// ---------------------------------------------------------------------------
// Tests (synthetic rows only; no corpus execution, no bundle files)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod evidence_tests {
    use super::super::trace::{Class, FrameOutcome, MarginStatus, Stage};
    use super::*;

    fn frame_row(op: Op, material: &str, frame: u32) -> FrameRow {
        FrameRow {
            op,
            material: material.to_string(),
            frame,
            seq: 0,
            outcome: OutcomeField::parse("F-even").unwrap(),
            class: ClassField::parse("R").unwrap(),
            margin: MarginField::parse("Derived").unwrap(),
        }
    }

    fn band_row(op: Op, material: &str, frame: u32, band: u32) -> BandRow {
        BandRow {
            op,
            material: material.to_string(),
            frame,
            band,
            stage: Stage::S4,
            class: ClassField::parse("S-S4").unwrap(),
            margin: MarginField::parse("Derived").unwrap(),
        }
    }

    fn cov_row(op: Op, dimension: &str, scope: Scope) -> CovRow {
        CovRow {
            op,
            dimension: dimension.to_string(),
            scope,
            status: CoverageStatus::Observed,
            observed: 3,
            required: 3,
            detail: None,
        }
    }

    #[test]
    fn schema_header_is_versioned_and_exact() {
        assert_eq!(SCHEMA_VERSION, 2);
        assert_eq!(MDEF_VERSION, 2);
        assert_eq!(mdef_version(), 2);
        assert_eq!(SCHEMA_HEADER, "R22SCHEMA\t2");
        // Accepted (E1-R0.3: v2; v1 never parses under v2).
        let set = RowSet::parse_tsv_file("R22SCHEMA\t2\n").unwrap();
        assert!(set.is_empty());
        // Rejected: other versions, near-miss headers, missing header.
        assert!(matches!(
            RowSet::parse_tsv_file("R22SCHEMA\t1\n"),
            Err(RowError::UnknownSchemaVersion(_))
        ));
        assert!(matches!(
            RowSet::parse_tsv_file("R22SCHEMA\t0\n"),
            Err(RowError::UnknownSchemaVersion(_))
        ));
        assert!(matches!(
            RowSet::parse_tsv_file("R22SCHEMA 2\n"),
            Err(RowError::MissingHeader)
        ));
        assert!(matches!(
            RowSet::parse_tsv_file("R22SCHEMA\t02\n"),
            Err(RowError::UnknownSchemaVersion(_))
        ));
        assert!(matches!(
            RowSet::parse_tsv_file(""),
            Err(RowError::EmptyFile)
        ));
        assert!(matches!(
            RowSet::parse_tsv_file("R22-FRAME\tOP-1\tsine-stereo\t0\t0\tF-even\tR\tDerived\n"),
            Err(RowError::MissingHeader)
        ));
    }

    #[test]
    fn row_ids_are_stable_and_deterministic() {
        let a = frame_row(Op::Op2, "m7-music-stereo-48k", 471);
        let b = frame_row(Op::Op2, "m7-music-stereo-48k", 471);
        assert_eq!(a.id(), b.id());
        assert_eq!(a.id(), "R22-FRAME/OP-2/m7-music-stereo-48k/000471");
        let band = band_row(Op::Op1, "sine-stereo", 8, 3);
        assert_eq!(band.id(), "R22-BAND/OP-1/sine-stereo/000008/003");
        let cov = cov_row(Op::Op4, "window:LONG", Scope::All);
        assert_eq!(cov.id(), "R22-COV/OP-4/window:LONG/*");
        // Zero-padding keeps lexicographic order numeric.
        let f9 = frame_row(Op::Op1, "m", 9);
        let f10 = frame_row(Op::Op1, "m", 10);
        assert!(f9.id() < f10.id());
    }

    #[test]
    fn row_ids_are_unique_across_types_ops_materials() {
        let rows = [
            Row::Frame(frame_row(Op::Op1, "sine-stereo", 0)),
            Row::Frame(frame_row(Op::Op2, "sine-stereo", 0)),
            Row::Frame(frame_row(Op::Op1, "onset-stereo", 0)),
            Row::Frame(frame_row(Op::Op1, "sine-stereo", 1)),
            Row::Band(band_row(Op::Op1, "sine-stereo", 0, 0)),
            Row::Band(band_row(Op::Op1, "sine-stereo", 0, 1)),
            Row::Cov(cov_row(Op::Op1, "window:LONG", Scope::All)),
            Row::Cov(cov_row(
                Op::Op1,
                "window:LONG",
                Scope::Material("sine-stereo".to_string()),
            )),
        ];
        let mut ids: Vec<String> = rows.iter().map(|r| r.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), rows.len(), "every synthetic row has a unique id");
        assert_eq!(
            rows.iter().map(|r| r.row_type()).collect::<Vec<_>>(),
            vec![
                RowType::Frame,
                RowType::Frame,
                RowType::Frame,
                RowType::Frame,
                RowType::Band,
                RowType::Band,
                RowType::Cov,
                RowType::Cov,
            ]
        );
    }

    #[test]
    fn rowset_rejects_duplicate_ids() {
        let mut set = RowSet::new();
        set.insert(Row::Frame(frame_row(Op::Op1, "sine-stereo", 0)))
            .unwrap();
        let dup = set.insert(Row::Frame(frame_row(Op::Op1, "sine-stereo", 0)));
        assert!(matches!(dup, Err(RowError::DuplicateId(_))));
        assert_eq!(set.len(), 1);
        // File-level duplicates are rejected as well.
        let row = frame_row(Op::Op1, "sine-stereo", 0).render_tsv();
        let text = format!("{SCHEMA_HEADER}\n{row}\n{row}\n");
        assert!(matches!(
            RowSet::parse_tsv_file(&text),
            Err(RowError::DuplicateId(_))
        ));
    }

    #[test]
    fn total_ordering_is_deterministic() {
        let mut set = RowSet::new();
        // Insert in scrambled order: COV first, then BAND, then FRAME,
        // OP-6 before OP-1, high frames before low ones.
        set.insert(Row::Cov(cov_row(Op::Op6, "window:SHORT", Scope::All)))
            .unwrap();
        set.insert(Row::Band(band_row(Op::Op1, "sine-stereo", 2, 5)))
            .unwrap();
        set.insert(Row::Frame(frame_row(Op::Op6, "click-mono", 0)))
            .unwrap();
        set.insert(Row::Frame(frame_row(Op::Op1, "sine-stereo", 2)))
            .unwrap();
        set.insert(Row::Band(band_row(Op::Op1, "sine-stereo", 2, 1)))
            .unwrap();
        set.insert(Row::Frame(frame_row(Op::Op1, "onset-stereo", 0)))
            .unwrap();
        set.insert(Row::Frame(frame_row(Op::Op1, "sine-stereo", 10)))
            .unwrap();
        let ids: Vec<String> = set.rows_in_order().iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "R22-FRAME/OP-1/onset-stereo/000000",
                "R22-FRAME/OP-1/sine-stereo/000002",
                "R22-FRAME/OP-1/sine-stereo/000010",
                "R22-BAND/OP-1/sine-stereo/000002/001",
                "R22-BAND/OP-1/sine-stereo/000002/005",
                "R22-FRAME/OP-6/click-mono/000000",
                "R22-COV/OP-6/window:SHORT/*",
            ]
        );
        // Byte-identical re-render regardless of insertion order.
        let first = set.render_tsv_file();
        let ordered: Vec<Row> = set.rows_in_order().into_iter().cloned().collect();
        let mut shuffled = RowSet::new();
        for row in ordered.into_iter().rev() {
            shuffled.insert(row).unwrap();
        }
        assert_eq!(first, shuffled.render_tsv_file());
    }

    #[test]
    fn op_ordering_is_one_through_six() {
        let mut labels = Vec::new();
        for op in Op::all() {
            labels.push(op.label().to_string());
        }
        assert_eq!(labels, vec!["OP-1", "OP-2", "OP-3", "OP-4", "OP-5", "OP-6"]);
        for (i, op) in Op::all().iter().enumerate() {
            assert_eq!(op.index(), (i + 1) as u8);
        }
        assert!(Op::all().windows(2).all(|w| w[0] < w[1]));
        // Rejected labels fail closed.
        for bad in ["OP-0", "OP-7", "op-1", "OP1", "", "OP-1 "] {
            assert!(Op::parse(bad).is_none(), "{bad:?} must not parse");
        }
    }

    #[test]
    fn material_ordering_is_byte_lexicographic() {
        // Canonical WAV ids order deterministically (no corpus needed).
        let mut mats = vec![
            "m8-music-eroica-stereo-48k",
            "m7-speech-clean-stereo-48k",
            "m7-music-stereo-48k",
            "m7-speech-pauses-stereo-48k",
        ];
        mats.sort();
        assert_eq!(
            mats,
            vec![
                "m7-music-stereo-48k",
                "m7-speech-clean-stereo-48k",
                "m7-speech-pauses-stereo-48k",
                "m8-music-eroica-stereo-48k",
            ]
        );
        // And rows follow it: OP-4 chain materials sort by id.
        let mut set = RowSet::new();
        for m in [
            "m8-music-eroica-stereo-48k",
            "m7-music-stereo-48k",
            "m7-speech-clean-stereo-48k",
        ] {
            set.insert(Row::Frame(frame_row(Op::Op4, m, 0))).unwrap();
        }
        let ids: Vec<String> = set.rows_in_order().iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "R22-FRAME/OP-4/m7-music-stereo-48k/000000",
                "R22-FRAME/OP-4/m7-speech-clean-stereo-48k/000000",
                "R22-FRAME/OP-4/m8-music-eroica-stereo-48k/000000",
            ]
        );
    }

    #[test]
    fn tsv_render_is_deterministic_and_clean() {
        let mut set = RowSet::new();
        set.insert(Row::Frame(frame_row(Op::Op2, "m7-music-stereo-48k", 471)))
            .unwrap();
        set.insert(Row::Band(band_row(Op::Op2, "m7-music-stereo-48k", 471, 12)))
            .unwrap();
        set.insert(Row::Cov(cov_row(
            Op::Op2,
            "transition:START",
            Scope::Material("m7-music-stereo-48k".to_string()),
        )))
        .unwrap();
        let a = set.render_tsv_file();
        let b = set.render_tsv_file();
        assert_eq!(a, b, "render is deterministic");
        assert!(!a.contains('\r'), "LF line endings only");
        assert!(
            a.ends_with('\n') && !a.ends_with("\n\n"),
            "one trailing newline"
        );
        for (i, line) in a.split('\n').enumerate() {
            if line.is_empty() {
                continue;
            }
            assert!(
                !line.ends_with(' ') && !line.ends_with('\t'),
                "line {i}: no trailing whitespace"
            );
            if i == 0 {
                assert_eq!(line, SCHEMA_HEADER, "line 0: schema-version header");
            } else {
                assert_eq!(line.split('\t').count(), 8, "line {i}: 8 tab fields");
            }
        }
        assert_eq!(
            a,
            "R22SCHEMA\t2\n\
             R22-FRAME\tOP-2\tm7-music-stereo-48k\t471\t0\tF-even\tR\tDerived\n\
             R22-BAND\tOP-2\tm7-music-stereo-48k\t471\t12\ts4\tS-S4\tDerived\n\
             R22-COV\tOP-2\ttransition:START\tm7-music-stereo-48k\tObserved\t3\t3\tn/a\n"
        );
    }

    #[test]
    fn vocabulary_accepts_canonical_raspf() {
        // Classes: R/A/S-*/P(*)/Unattributed(*), via the Group D validators.
        for tok in [
            "R",
            "A",
            "S-IS",
            "S-MS",
            "S-PE",
            "S-S2",
            "S-S4",
            "S-S6",
            "P(S-IS)",
            "P(S-S6)",
            "Unattributed(s6)",
            "Unattributed(acct)",
        ] {
            assert!(ClassField::parse(tok).unwrap().is_classified(), "{tok}");
        }
        for tok in ["F-even", "F-valid", "F-unsupported"] {
            assert!(OutcomeField::parse(tok).unwrap().is_classified(), "{tok}");
        }
        for tok in ["Derived", "DerivedF32", "MarginDerivationRequired"] {
            assert!(MarginField::parse(tok).unwrap().is_classified(), "{tok}");
        }
        // Lockstep with the reviewed Group D labels (E-OD-1 reuse proof).
        assert!(ClassField::parse(&Class::R.label()).is_some());
        assert!(ClassField::parse(&Class::A.label()).is_some());
        assert!(ClassField::parse(&Class::S(Stage::Is).label()).is_some());
        assert!(ClassField::parse(&Class::S(Stage::S6).label()).is_some());
        assert!(ClassField::parse(
            &Class::P {
                stage: Stage::S6,
                origin_stage: Stage::S4,
                origin_band: 3,
                origin_class: Box::new(Class::R),
            }
            .label()
        )
        .is_some());
        assert!(ClassField::parse(&Class::Unattributed { stage: Stage::S6 }.label()).is_some());
        assert!(OutcomeField::parse(FrameOutcome::FEven.label()).is_some());
        assert!(OutcomeField::parse(FrameOutcome::FValid.label()).is_some());
        assert!(OutcomeField::parse(FrameOutcome::FUnsupported.label()).is_some());
        let _ = MarginStatus::Derived(0);
        let _ = MarginStatus::MarginDerivationRequired("e1");
    }

    #[test]
    fn vocabulary_rejects_superseded_i_class() {
        // The superseded `I` classification must fail closed everywhere.
        for tok in ["I", "S-I", "P(I)", "P(R)", "S-XX", "", "r", "s-s4"] {
            assert!(ClassField::parse(tok).is_none(), "{tok:?} must not parse");
        }
        for tok in ["F-pass", "F-even ", "even", ""] {
            assert!(OutcomeField::parse(tok).is_none(), "{tok:?} must not parse");
        }
        for tok in ["Guessed", "Approximate", "derived", ""] {
            assert!(MarginField::parse(tok).is_none(), "{tok:?} must not parse");
        }
    }

    #[test]
    fn invalid_tokens_rejected() {
        // Bad stage / op / row tag.
        assert!(parse_stage_token("s7").is_none());
        assert!(parse_stage_token("S4").is_none());
        assert!(parse_stage_token("").is_none());
        for tok in ["is", "ms", "pe", "s2", "s4", "s6", "acct"] {
            assert!(parse_stage_token(tok).is_some(), "{tok}");
        }
        assert!(RowType::parse("R22").is_none());
        assert!(RowType::parse("r22-frame").is_none());
        // Non-canonical integers fail closed.
        for bad in ["01", "+1", " 1", "1 ", "x", "-", "4294967296"] {
            let line = format!("R22-FRAME\tOP-1\tsine-stereo\t{bad}\t0\tF-even\tR\tDerived");
            assert!(
                Row::parse_tsv_line(&line, 2).is_err(),
                "{bad:?} frame must not parse"
            );
        }
        // Embedded tab splits fields (wrong count); embedded newline splits
        // lines. A material id containing `/` breaks ID structure.
        let evil = "R22-FRAME\tOP-1\ta/b\t0\t0\tF-even\tR\tDerived";
        assert!(Row::parse_tsv_line(evil, 2).is_err());
        let short = "R22-FRAME\tOP-1\tsine-stereo\t0\t0\tF-even\tR";
        assert!(matches!(
            Row::parse_tsv_line(short, 2),
            Err(RowError::BadFieldCount { got: 7, .. })
        ));
        // Out-of-range frame/band; observed exceeding required.
        let big = "R22-FRAME\tOP-1\tsine-stereo\t1000000\t0\tF-even\tR\tDerived";
        assert!(Row::parse_tsv_line(big, 2).is_err());
        let big_band = "R22-BAND\tOP-1\tsine-stereo\t0\t1000\ts4\tS-S4\tDerived";
        assert!(Row::parse_tsv_line(big_band, 2).is_err());
        let over = "R22-COV\tOP-1\twindow:LONG\t*\tObserved\t4\t3\tn/a";
        assert!(Row::parse_tsv_line(over, 2).is_err());
    }

    #[test]
    fn missing_evidence_tokens_preserved() {
        for marker in MissingKind::all() {
            let tok = marker.token();
            assert_eq!(ClassField::parse(tok).unwrap().missing(), Some(marker));
            assert_eq!(OutcomeField::parse(tok).unwrap().missing(), Some(marker));
            assert_eq!(MarginField::parse(tok).unwrap().missing(), Some(marker));
            assert_eq!(
                CoverageStatus::parse(tok),
                Some(CoverageStatus::Missing(marker))
            );
            // Round-trip through render: the exact marker is preserved.
            assert_eq!(ClassField::parse(tok).unwrap().render(), tok);
            assert_eq!(CoverageStatus::parse(tok).unwrap().token(), tok);
        }
        // A row carrying markers round-trips through the file layer.
        let mut row = frame_row(Op::Op3, "m7-speech-pauses-stereo-48k", 5);
        row.class = ClassField::parse("Unobserved").unwrap();
        row.outcome = OutcomeField::parse("Missing").unwrap();
        row.margin = MarginField::parse("MarginDerivationRequired").unwrap();
        let line = row.render_tsv();
        let back = Row::parse_tsv_line(&line, 2).unwrap();
        assert_eq!(back, Row::Frame(row));
    }

    #[test]
    fn no_accidental_classification_of_missing_as_not_applicable() {
        // Only an explicit NotApplicable excuses absence.
        assert!(CoverageStatus::Observed.is_observed());
        assert!(!CoverageStatus::Observed.is_inapplicable());
        for marker in MissingKind::all() {
            let s = CoverageStatus::Missing(marker);
            assert!(!s.is_observed(), "{marker:?} is not observed");
            assert_eq!(
                s.is_inapplicable(),
                marker == MissingKind::NotApplicable,
                "{marker:?} inapplicability must be explicit"
            );
        }
        // In particular: Unobserved/Unwired/Unsupported/Missing stay open.
        for marker in [
            MissingKind::Missing,
            MissingKind::Unwired,
            MissingKind::Unsupported,
            MissingKind::Unobserved,
            MissingKind::NotGenerated,
        ] {
            let row = CovRow {
                op: Op::Op2,
                dimension: "path:s6-crash".to_string(),
                scope: Scope::All,
                status: CoverageStatus::Missing(marker),
                observed: 0,
                required: 1,
                detail: None,
            };
            assert!(!row.status.is_observed());
            assert!(!row.status.is_inapplicable(), "{marker:?} must stay open");
            let back = Row::parse_tsv_line(&row.render_tsv(), 2).unwrap();
            assert_eq!(back, Row::Cov(row));
        }
    }

    #[test]
    fn tsv_render_parse_round_trip() {
        let mut set = RowSet::new();
        set.insert(Row::Frame(frame_row(Op::Op1, "sine-stereo", 0)))
            .unwrap();
        set.insert(Row::Band(band_row(Op::Op5, "sine-stereo-44100", 40, 21)))
            .unwrap();
        let mut cov = cov_row(
            Op::Op4,
            "chain:material-boundary",
            Scope::Material("m7-music-stereo-48k".to_string()),
        );
        cov.status = CoverageStatus::Missing(MissingKind::NotApplicable);
        cov.observed = 0;
        cov.required = 0;
        cov.detail = Some("single-material OPs have no chain boundary".to_string());
        set.insert(Row::Cov(cov)).unwrap();
        let text = set.render_tsv_file();
        let back = RowSet::parse_tsv_file(&text).unwrap();
        assert_eq!(back, set);
        assert_eq!(back.render_tsv_file(), text, "re-render is byte-identical");
    }

    #[test]
    fn json_render_is_deterministic_with_fixed_key_order() {
        let row = Row::Frame(frame_row(Op::Op2, "m7-music-stereo-48k", 471));
        let a = row.render_json();
        let b = row.render_json();
        assert_eq!(a, b);
        assert_eq!(
            a,
            "{\"type\":\"R22-FRAME\",\"id\":\"R22-FRAME/OP-2/m7-music-stereo-48k/000471\",\
             \"op\":\"OP-2\",\"material\":\"m7-music-stereo-48k\",\"frame\":471,\"seq\":0,\
             \"outcome\":\"F-even\",\"class\":\"R\",\"margin\":\"Derived\"}"
        );
        let band = Row::Band(band_row(Op::Op1, "sine-stereo", 8, 3));
        assert_eq!(
            band.render_json(),
            "{\"type\":\"R22-BAND\",\"id\":\"R22-BAND/OP-1/sine-stereo/000008/003\",\
             \"op\":\"OP-1\",\"material\":\"sine-stereo\",\"frame\":8,\"band\":3,\
             \"stage\":\"s4\",\"class\":\"S-S4\",\"margin\":\"Derived\"}"
        );
        let cov = Row::Cov(cov_row(Op::Op1, "window:LONG", Scope::All));
        assert_eq!(
            cov.render_json(),
            "{\"type\":\"R22-COV\",\"id\":\"R22-COV/OP-1/window:LONG/*\",\
             \"op\":\"OP-1\",\"dimension\":\"window:LONG\",\"scope\":\"*\",\
             \"status\":\"Observed\",\"observed\":3,\"required\":3,\"detail\":\"n/a\"}"
        );
    }

    #[test]
    fn json_round_trip() {
        let rows = [
            Row::Frame(frame_row(Op::Op3, "m7-speech-clean-stereo-48k", 470)),
            Row::Band(band_row(Op::Op6, "impulse-mono", 2, 0)),
            Row::Cov(cov_row(
                Op::Op6,
                "path:mono-no-ms",
                Scope::Material("click-mono".to_string()),
            )),
        ];
        for row in rows {
            let s = row.render_json();
            let back = Row::parse_json_object(&s).unwrap();
            assert_eq!(back, row);
            assert_eq!(back.render_json(), s, "JSON re-render is byte-identical");
        }
        // Envelope round-trip preserves total order.
        let mut set = RowSet::new();
        set.insert(Row::Band(band_row(Op::Op1, "sine-stereo", 2, 5)))
            .unwrap();
        set.insert(Row::Frame(frame_row(Op::Op1, "sine-stereo", 2)))
            .unwrap();
        let env = set.render_json_file();
        assert!(env.starts_with("{\"schema\":2,\"rows\":["));
        assert!(env.ends_with("]}\n"));
        let back = RowSet::parse_json_file(&env).unwrap();
        assert_eq!(back, set);
    }

    #[test]
    fn json_tsv_semantic_equivalence() {
        // Every row parses to the same value through both formats, with the
        // same ID and the same semantic tokens — including markers.
        let mut class = ClassField::parse("P(S-S4)").unwrap();
        assert!(class.is_classified());
        class = ClassField::parse("Unsupported").unwrap();
        let mut frame = frame_row(Op::Op4, "m7-music-stereo-48k", 471);
        frame.class = class;
        frame.margin = MarginField::parse("NotGenerated").unwrap();
        let rows = [
            Row::Frame(frame),
            Row::Band(band_row(Op::Op2, "m7-music-stereo-48k", 0, 48)),
            Row::Cov(cov_row(Op::Op3, "window:SHORT", Scope::All)),
        ];
        for row in rows {
            let from_tsv = Row::parse_tsv_line(&row.render_tsv(), 7).unwrap();
            let from_json = Row::parse_json_object(&row.render_json()).unwrap();
            assert_eq!(from_tsv, from_json);
            assert_eq!(from_tsv.id(), from_json.id());
            assert_eq!(from_tsv.id(), row.id(), "same row IDs as TSV");
        }
        // File level: TSV and JSON envelopes decode to equal sets.
        let mut set = RowSet::new();
        set.insert(Row::Frame(frame_row(Op::Op1, "sine-stereo", 1)))
            .unwrap();
        set.insert(Row::Band(band_row(Op::Op1, "sine-stereo", 1, 2)))
            .unwrap();
        let via_tsv = RowSet::parse_tsv_file(&set.render_tsv_file()).unwrap();
        let via_json = RowSet::parse_json_file(&set.render_json_file()).unwrap();
        assert_eq!(via_tsv, via_json);
    }

    #[test]
    fn malformed_rows_never_accepted() {
        // Unknown row type / bad tag.
        assert!(matches!(
            Row::parse_tsv_line("R22\tOP-1\tsine-stereo\t0\t0\tF-even\tR\tDerived", 2),
            Err(RowError::UnknownRowType(_))
        ));
        // File-level malformations.
        let good_row = frame_row(Op::Op1, "sine-stereo", 0).render_tsv();
        let crlf = format!("{SCHEMA_HEADER}\r\n{good_row}\n");
        assert!(matches!(
            RowSet::parse_tsv_file(&crlf),
            Err(RowError::CarriageReturn)
        ));
        let no_trail = format!("{SCHEMA_HEADER}\n{good_row}");
        assert!(matches!(
            RowSet::parse_tsv_file(&no_trail),
            Err(RowError::MissingTrailingNewline)
        ));
        let empty_line = format!("{SCHEMA_HEADER}\n\n{good_row}\n");
        assert!(matches!(
            RowSet::parse_tsv_file(&empty_line),
            Err(RowError::EmptyLine(2))
        ));
        // JSON malformations: truncated, wrong types, extra keys,
        // non-canonical numbers, id mismatch, unknown schema.
        assert!(Row::parse_json_object("{\"type\":\"R22-FRAME\"").is_err());
        assert!(Row::parse_json_object("{\"type\":7}").is_err());
        let mut extra = frame_row(Op::Op1, "sine-stereo", 0).render_json();
        extra = extra.replace("\"margin\":\"Derived\"}", "\"margin\":\"Derived\",\"x\":1}");
        assert!(Row::parse_json_object(&extra).is_err());
        let noncanon = frame_row(Op::Op1, "sine-stereo", 0)
            .render_json()
            .replace("\"frame\":0", "\"frame\":00");
        assert!(Row::parse_json_object(&noncanon).is_err());
        let quoted_num = frame_row(Op::Op1, "sine-stereo", 0)
            .render_json()
            .replace("\"frame\":0", "\"frame\":\"0\"");
        assert!(Row::parse_json_object(&quoted_num).is_err());
        let id_swap = frame_row(Op::Op1, "sine-stereo", 0)
            .render_json()
            .replace("000000", "000001");
        assert!(Row::parse_json_object(&id_swap).is_err());
        let bad_schema = "{\"schema\":1,\"rows\":[]}";
        assert!(matches!(
            RowSet::parse_json_file(bad_schema),
            Err(RowError::UnknownSchemaVersion(_))
        ));
        assert!(RowSet::parse_json_file("{\"schema\":2,\"rows\":[{}]}").is_err());
        assert!(RowSet::parse_json_file("not json").is_err());
        // A detail of `n/a` text collides with absence: the TSV field `n/a`
        // always parses as absent, so literal `n/a` text is unrepresentable
        // as present detail (constructing it fails loudly at render).
        let na_line = "R22-COV\tOP-1\twindow:LONG\t*\tObserved\t0\t0\tn/a";
        let parsed = Row::parse_tsv_line(na_line, 2).unwrap();
        let mut expect_na = cov_row(Op::Op1, "window:LONG", Scope::All);
        expect_na.observed = 0;
        expect_na.required = 0;
        assert_eq!(parsed, Row::Cov(expect_na));
        let mut cov = cov_row(Op::Op1, "window:LONG", Scope::All);
        cov.detail = Some(NA_TOKEN.to_string());
        assert!(std::panic::catch_unwind(|| cov.render_tsv()).is_err());
    }

    #[test]
    fn vector_codec_round_trip() {
        assert_eq!(encode_vec_i32(&[]), "");
        assert_eq!(encode_vec_i32(&[1, -2, 300]), "1,-2,300");
        assert_eq!(parse_vec_i32(""), Some(Vec::new()));
        assert_eq!(parse_vec_i32("1,-2,300"), Some(vec![1, -2, 300]));
        assert_eq!(encode_vec_opt(None), "n/a");
        assert_eq!(encode_vec_opt(Some(&[0, 5][..])), "0,5");
        assert_eq!(parse_vec_opt("n/a"), Some(None));
        assert_eq!(parse_vec_opt(""), Some(Some(Vec::new())));
        assert_eq!(parse_vec_opt("7"), Some(Some(vec![7])));
        // Malformed vectors fail closed.
        for bad in [
            "1,,2", ",1", "1,", "a", "1.0", "01", " 1", "1 ", "+1", "n/A",
        ] {
            assert!(parse_vec_i32(bad).is_none(), "{bad:?}");
            if bad != "n/A" {
                // "n/A" is not the absent token either.
                assert!(parse_vec_opt(bad).is_none(), "{bad:?}");
            }
        }
        assert!(parse_vec_opt("n/A").is_none());
    }

    #[test]
    fn scope_and_status_tokens() {
        assert_eq!(Scope::parse("*"), Some(Scope::All));
        assert_eq!(
            Scope::parse("sine-stereo"),
            Some(Scope::Material("sine-stereo".to_string()))
        );
        assert!(Scope::parse("").is_none());
        assert!(Scope::parse("a/b").is_none());
        assert_eq!(Scope::All.render(), "*");
        assert_eq!(CoverageStatus::Observed.token(), "Observed");
        // `Observed` is not a missing marker and markers are not `Observed`.
        assert!(CoverageStatus::parse("Observed").unwrap().is_observed());
        assert!(MissingKind::parse("Observed").is_none());
    }

    #[test]
    fn field_charset_rejects_separators() {
        // Tabs/newlines can never hide inside a field. A tab splits the
        // line into too many fields; a newline corrupts line structure;
        // both fail closed at parse (raw lines built by hand — rendering
        // an invalid row panics loudly by design instead).
        for evil in ["a\tb", "a\nb", "a\rb"] {
            let line = format!("R22-FRAME\tOP-1\t{evil}\t0\t0\tF-even\tR\tDerived");
            assert!(Row::parse_tsv_line(&line, 2).is_err(), "{evil:?}");
            let cov = format!("R22-COV\tOP-1\twindow:LONG\t*\tObserved\t0\t0\t{evil}");
            assert!(Row::parse_tsv_line(&cov, 2).is_err(), "{evil:?}");
        }
    }

    // -----------------------------------------------------------------------
    // E1-R0.3 verification (synthetic only; no corpus, no bundle files)
    // -----------------------------------------------------------------------

    fn chain_row(op: Op, material: &str, frame: u32, chain: &str, ordinal: u32) -> ChainRow {
        let origin = if ordinal == 0 {
            ChainOrigin::Root
        } else {
            ChainOrigin::Unattributed
        };
        ChainRow {
            op,
            material: material.to_string(),
            frame,
            chain: chain.to_string(),
            ordinal,
            stage: Stage::S4,
            band: 3,
            origin,
        }
    }

    fn leaf_row(
        op: Op,
        material: &str,
        frame: u32,
        band: u32,
        path: &str,
        value: Option<i32>,
    ) -> LeafRow {
        LeafRow {
            op,
            material: material.to_string(),
            frame,
            band,
            path: path.to_string(),
            value,
        }
    }

    fn replay_row(tag: &str) -> ReplayRow {
        ReplayRow {
            tag: tag.to_string(),
            kind: "is".to_string(),
            digest: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_string(),
            status: ReplayStatus::Match,
        }
    }

    #[test]
    fn six_row_schema_preserves_frame_band_cov_identities() {
        assert_eq!(RowType::Frame.tag(), "R22-FRAME");
        assert_eq!(RowType::Band.tag(), "R22-BAND");
        assert_eq!(RowType::Cov.tag(), "R22-COV");
        assert_eq!(RowType::Chain.tag(), "R22-CHAIN");
        assert_eq!(RowType::Leaf.tag(), "R22-LEAF");
        assert_eq!(RowType::Replay.tag(), "R22-REPLAY");
        assert!(RowType::Frame.rank() < RowType::Band.rank());
        assert!(RowType::Band.rank() < RowType::Chain.rank());
        assert!(RowType::Chain.rank() < RowType::Leaf.rank());
        assert!(RowType::Leaf.rank() < RowType::Replay.rank());
        assert!(RowType::Replay.rank() < RowType::Cov.rank());
        assert_eq!(
            RowType::all(),
            [
                RowType::Frame,
                RowType::Band,
                RowType::Chain,
                RowType::Leaf,
                RowType::Replay,
                RowType::Cov
            ]
        );
        let f = frame_row(Op::Op2, "m7-music-stereo-48k", 471);
        assert_eq!(f.id(), "R22-FRAME/OP-2/m7-music-stereo-48k/000471");
        let b = band_row(Op::Op1, "sine-stereo", 8, 3);
        assert_eq!(b.id(), "R22-BAND/OP-1/sine-stereo/000008/003");
        let c = cov_row(Op::Op4, "window:LONG", Scope::All);
        assert_eq!(c.id(), "R22-COV/OP-4/window:LONG/*");
    }

    #[test]
    fn mdef_registry_is_versioned_and_conservative() {
        assert_eq!(MDEF_VERSION, 2);
        assert_eq!(mdef_version(), 2);
        assert_eq!(SCHEMA_VERSION, 2);
        assert!(mdef_contains("pe.prepare_admission"));
        assert!(mdef_contains("s2.ah_clamp"));
        assert!(mdef_contains("s2.ah_clamp_short"));
        assert!(!mdef_contains("s2.no_such_margin"));
        assert!(!mdef_contains(""));
        assert!(mdef_get("nope").is_none());
        let short = mdef_get("s2.ah_clamp_short").expect("registered");
        assert_eq!(short.mode, MdefMode::Required);
        assert_eq!(short.stage, Stage::S2);
        let clamp = mdef_get("s2.ah_clamp").expect("registered");
        assert_eq!(clamp.mode, MdefMode::Direct);
        assert_eq!(MdefMode::parse("Direct"), Some(MdefMode::Direct));
        assert_eq!(MdefMode::parse("ReDerived"), Some(MdefMode::ReDerived));
        assert_eq!(MdefMode::parse("Required"), Some(MdefMode::Required));
        assert!(MdefMode::parse("Guessed").is_none());
        assert_eq!(Q1_STATUS, "OwnerReview");
    }

    #[test]
    fn deterministic_tagged_global_ordering_replay_independent() {
        let mut set = RowSet::new();
        set.insert(Row::Cov(cov_row(Op::Op6, "window:SHORT", Scope::All)))
            .unwrap();
        set.insert(Row::Replay(replay_row("z-tag"))).unwrap();
        set.insert(Row::Replay(replay_row("a-tag"))).unwrap();
        set.insert(Row::Leaf(leaf_row(
            Op::Op1,
            "sine-stereo",
            2,
            1,
            "s4.scf[0]",
            Some(3),
        )))
        .unwrap();
        set.insert(Row::Chain(chain_row(Op::Op1, "sine-stereo", 2, "c1", 0)))
            .unwrap();
        set.insert(Row::Chain(chain_row(Op::Op1, "sine-stereo", 2, "c1", 1)))
            .unwrap();
        set.insert(Row::Band(band_row(Op::Op1, "sine-stereo", 2, 5)))
            .unwrap();
        set.insert(Row::Frame(frame_row(Op::Op1, "sine-stereo", 2)))
            .unwrap();
        set.insert(Row::Frame(frame_row(Op::Op1, "sine-stereo", 10)))
            .unwrap();
        let ids: Vec<String> = set.rows_in_order().iter().map(|r| r.id()).collect();
        // Tagged global ordering: (OP, rank) with REPLAY pinned to OP 0 keeps
        // REPLAY corpus/OP-independent (always first block, tag-ordered).
        assert_eq!(
            ids,
            vec![
                "R22-REPLAY/a-tag",
                "R22-REPLAY/z-tag",
                "R22-FRAME/OP-1/sine-stereo/000002",
                "R22-FRAME/OP-1/sine-stereo/000010",
                "R22-BAND/OP-1/sine-stereo/000002/005",
                "R22-CHAIN/OP-1/sine-stereo/000002/c1/0000",
                "R22-CHAIN/OP-1/sine-stereo/000002/c1/0001",
                "R22-LEAF/OP-1/sine-stereo/000002/001/s4.scf[0]",
                "R22-COV/OP-6/window:SHORT/*",
            ]
        );
        // REPLAY independence: tag order decides, not OP/material/frame.
        let mut set2 = RowSet::new();
        set2.insert(Row::Replay(replay_row("m-tag"))).unwrap();
        set2.insert(Row::Replay(replay_row("a-tag"))).unwrap();
        let ids2: Vec<String> = set2.rows_in_order().iter().map(|r| r.id()).collect();
        assert_eq!(ids2, vec!["R22-REPLAY/a-tag", "R22-REPLAY/m-tag"]);
        // Byte-identical regeneration regardless of insertion order.
        let first = set.render_tsv_file();
        let mut rev = RowSet::new();
        for r in set.rows_in_order().into_iter().cloned().rev() {
            rev.insert(r).unwrap();
        }
        assert_eq!(first, rev.render_tsv_file());
        let back = RowSet::parse_tsv_file(&first).unwrap();
        assert_eq!(back, set);
        assert_eq!(back.render_tsv_file(), first);
        let j = set.render_json_file();
        let bj = RowSet::parse_json_file(&j).unwrap();
        assert_eq!(bj, set);
    }

    #[test]
    fn leaf_decomposition_bidirectional_completeness() {
        let mut out = Vec::new();
        decompose_leaves(
            "s4",
            &Nested::Seq(vec![Nested::Leaf(1), Nested::Absent, Nested::Leaf(-2)]),
            &mut out,
        );
        assert_eq!(
            out,
            vec![
                ("s4[0]".to_string(), Some(1)),
                ("s4[1]".to_string(), None),
                ("s4[2]".to_string(), Some(-2))
            ]
        );
        check_leaf_completeness(&out, &out).unwrap();
        // Unequal lengths fail closed (missing leaf).
        let short = &out[..2];
        assert!(check_leaf_completeness(&out, short).is_err());
        assert!(check_leaf_completeness(short, &out).is_err());
        // Absent-vs-value mismatch fails.
        let mut wrong = out.clone();
        wrong[1].1 = Some(0);
        assert!(check_leaf_completeness(&out, &wrong).is_err());
        // Nested structures.
        let mut nested = Vec::new();
        decompose_leaves(
            "s2",
            &Nested::Map(vec![
                ("thr_exp".to_string(), Nested::Leaf(7)),
                (
                    "sub".to_string(),
                    Nested::Map(vec![("x".to_string(), Nested::Leaf(1))]),
                ),
            ]),
            &mut nested,
        );
        assert!(nested.contains(&("s2.thr_exp".to_string(), Some(7))));
        assert!(nested.contains(&("s2.sub.x".to_string(), Some(1))));
        check_leaf_completeness(&nested, &nested).unwrap();
        // Incomplete-path handling: truncated subtree emits explicit marker, absent-valued.
        let mut trunc = Vec::new();
        decompose_leaves("s4", &Nested::Truncated("cut".to_string()), &mut trunc);
        assert_eq!(trunc.len(), 1);
        assert!(is_incomplete_path(&trunc[0].0));
        assert_eq!(trunc[0].1, None);
        check_leaf_completeness(&trunc, &trunc).unwrap();
        // Incomplete path with a value is rejected at row validation.
        let bad = leaf_row(Op::Op1, "m", 0, 0, "s4.<incomplete>", Some(1));
        assert!(std::panic::catch_unwind(|| bad.render_tsv()).is_err());
        // Spurious leaf fails.
        let mut extra = out.clone();
        extra.push(("s4[9]".to_string(), Some(0)));
        assert!(check_leaf_completeness(&out, &extra).is_err());
    }

    #[test]
    fn leaf_row_round_trip_and_rejections() {
        let r = leaf_row(Op::Op1, "sine-stereo", 2, 3, "s4.scf[12]", Some(-5));
        assert_eq!(r.id(), "R22-LEAF/OP-1/sine-stereo/000002/003/s4.scf[12]");
        let line = r.render_tsv();
        let back = Row::parse_tsv_line(&line, 2).unwrap();
        assert_eq!(back, Row::Leaf(r.clone()));
        let j = Row::Leaf(r.clone()).render_json();
        assert_eq!(Row::parse_json_object(&j).unwrap(), Row::Leaf(r));
        // Absent value round-trips as n/a.
        let na = leaf_row(Op::Op1, "m", 0, 0, "s2.thr_exp[1]", None);
        assert!(na.render_tsv().ends_with("\tn/a"));
        assert_eq!(
            Row::parse_tsv_line(&na.render_tsv(), 2).unwrap(),
            Row::Leaf(na)
        );
        // Rejections: bad path, value on incomplete path, out-of-range band.
        assert!(Row::parse_tsv_line("R22-LEAF\tOP-1\tm\t0\t0\tbad/path\t1", 2).is_err());
        assert!(Row::parse_tsv_line("R22-LEAF\tOP-1\tm\t0\t0\ts4.<incomplete>\t1", 2).is_err());
        assert!(Row::parse_tsv_line("R22-LEAF\tOP-1\tm\t0\t1000\ts4.x\t1", 2).is_err());
        assert!(Row::parse_tsv_line("R22-LEAF\tOP-1\tm\t0\t0\ts4.x\t01", 2).is_err());
    }

    #[test]
    fn chain_provenance_ordering_uniqueness_and_bounds() {
        let ok = vec![
            ChainRow {
                op: Op::Op1,
                material: "m".to_string(),
                frame: 0,
                chain: "c1".to_string(),
                ordinal: 0,
                stage: Stage::S4,
                band: 3,
                origin: ChainOrigin::Root,
            },
            ChainRow {
                op: Op::Op1,
                material: "m".to_string(),
                frame: 0,
                chain: "c1".to_string(),
                ordinal: 1,
                stage: Stage::S6,
                band: 3,
                origin: ChainOrigin::Link {
                    stage: Stage::S4,
                    band: 3,
                },
            },
            ChainRow {
                op: Op::Op1,
                material: "m".to_string(),
                frame: 0,
                chain: "c1".to_string(),
                ordinal: 2,
                stage: Stage::S6,
                band: 5,
                origin: ChainOrigin::Unattributed,
            },
        ];
        validate_chain_set(&ok).unwrap();
        // Ordinal gap fails.
        let gap = vec![ok[0].clone(), ok[2].clone()];
        assert!(validate_chain_set(&gap).is_err());
        // Head must be root.
        let bad_head = vec![
            ChainRow {
                origin: ChainOrigin::Unattributed,
                ..ok[0].clone()
            },
            ok[1].clone(),
        ];
        assert!(validate_chain_set(&bad_head).is_err());
        // Only head may be root.
        let two_roots = vec![
            ok[0].clone(),
            ChainRow {
                origin: ChainOrigin::Root,
                ordinal: 1,
                ..ok[1].clone()
            },
        ];
        assert!(validate_chain_set(&two_roots).is_err());
        // Unprovenanced link fails (cites a band with no earlier link).
        let bad_link = vec![
            ok[0].clone(),
            ChainRow {
                ordinal: 1,
                stage: Stage::S6,
                band: 9,
                origin: ChainOrigin::Link {
                    stage: Stage::S4,
                    band: 9,
                },
                ..ok[1].clone()
            },
        ];
        assert!(validate_chain_set(&bad_link).is_err());
        // Ordinal bound enforced at parse and validate.
        let big = format!("R22-CHAIN\tOP-1\tm\t0\tc1\t10000\ts4\t3\troot");
        assert!(Row::parse_tsv_line(&big, 2).is_err());
        // Uniqueness via RowSet duplicate IDs.
        let mut set = RowSet::new();
        set.insert(Row::Chain(ok[0].clone())).unwrap();
        assert!(set.insert(Row::Chain(ok[0].clone())).is_err());
        // Round-trips.
        for r in &ok {
            let line = r.render_tsv();
            assert_eq!(
                Row::parse_tsv_line(&line, 2).unwrap(),
                Row::Chain(r.clone())
            );
            let j = Row::Chain(r.clone()).render_json();
            assert_eq!(Row::parse_json_object(&j).unwrap(), Row::Chain(r.clone()));
        }
        // Origin parsing.
        assert_eq!(ChainOrigin::parse("root"), Some(ChainOrigin::Root));
        assert_eq!(
            ChainOrigin::parse("Unattributed"),
            Some(ChainOrigin::Unattributed)
        );
        assert_eq!(
            ChainOrigin::parse("s4:12"),
            Some(ChainOrigin::Link {
                stage: Stage::S4,
                band: 12
            })
        );
        assert!(ChainOrigin::parse("s9:1").is_none());
        assert!(ChainOrigin::parse("root:1").is_none());
    }

    #[test]
    fn replay_rows_are_corpus_independent_and_fail_closed() {
        let a = replay_row("f20-q31-b24");
        let b = replay_row("f20-fpe-b24");
        assert_eq!(a.id(), "R22-REPLAY/f20-q31-b24");
        assert!(!a.id().contains("OP-"));
        assert!(!a.id().contains("sine"));
        // Round-trips TSV + JSON with identical IDs.
        let la = a.render_tsv();
        assert_eq!(Row::parse_tsv_line(&la, 2).unwrap(), Row::Replay(a.clone()));
        let ja = Row::Replay(a.clone()).render_json();
        assert_eq!(Row::parse_json_object(&ja).unwrap(), Row::Replay(a.clone()));
        assert_eq!(
            Row::parse_tsv_line(&la, 2).unwrap().id(),
            Row::parse_json_object(&ja).unwrap().id()
        );
        // Rejections: bad kind, bad digest (uppercase, short), bad status.
        assert!(ReplayRow::parse_body(&["t", "bogus", &"a".repeat(64), "Match"], 2).is_err());
        assert!(ReplayRow::parse_body(&["t", "is", &"A".repeat(64), "Match"], 2).is_err());
        assert!(ReplayRow::parse_body(&["t", "is", "abc", "Match"], 2).is_err());
        assert!(ReplayRow::parse_body(&["t", "is", &"a".repeat(64), "Pass"], 2).is_err());
        assert!(!replay_kind_ok("bogus"));
        assert!(replay_kind_ok("writeahead"));
        assert!(replay_kind_ok("f20"));
        // Statuses other than Match never validate successfully (fail-closed at use).
        assert_eq!(ReplayStatus::parse("Match"), Some(ReplayStatus::Match));
        assert_eq!(
            ReplayStatus::parse("Mismatch"),
            Some(ReplayStatus::Mismatch)
        );
        assert_eq!(ReplayStatus::parse("Torn"), Some(ReplayStatus::Torn));
        assert_eq!(
            ReplayStatus::parse("Unavailable"),
            Some(ReplayStatus::Unavailable)
        );
        assert!(ReplayStatus::parse("Ok").is_none());
        let _ = b;
    }

    #[test]
    fn cov_witness_bijection_and_fail_closed_aggregation() {
        let ok = CovWitnessSet {
            cov_id: "R22-COV/OP-1/window:LONG/*".to_string(),
            observed: 2,
            is_observed: true,
            witnesses: vec!["f0".to_string(), "f1".to_string()],
        };
        ok.validate().unwrap();
        // Duplicate witness fails.
        let dup = CovWitnessSet {
            witnesses: vec!["f0".to_string(), "f0".to_string()],
            ..ok.clone()
        };
        assert!(dup.validate().is_err());
        // Count mismatch fails.
        let short = CovWitnessSet {
            witnesses: vec!["f0".to_string()],
            ..ok.clone()
        };
        assert!(short.validate().is_err());
        // Bad witness id fails.
        let bad = CovWitnessSet {
            witnesses: vec!["a/b".to_string(), "f1".to_string()],
            ..ok.clone()
        };
        assert!(bad.validate().is_err());
        // Unobserved must carry no witnesses.
        let unobs = CovWitnessSet {
            cov_id: "x".to_string(),
            observed: 0,
            is_observed: false,
            witnesses: vec![],
        };
        unobs.validate().unwrap();
        let phantom = CovWitnessSet {
            witnesses: vec!["f0".to_string()],
            ..unobs.clone()
        };
        assert!(phantom.validate().is_err());
        // Aggregate: per-material sum must equal the All total.
        validate_cov_aggregate(&[2, 3], 5).unwrap();
        assert!(validate_cov_aggregate(&[2, 3], 4).is_err());
        assert!(validate_cov_aggregate(&[], 1).is_err());
    }

    #[test]
    fn direct_rederived_margin_cross_checks_are_mandatory() {
        use super::super::trace::MarginStatus as MS;
        let ok = PerBandMargin {
            name: "s2.ah_clamp",
            direct: MS::Derived(5),
            rederived: MS::Derived(5),
        };
        ok.cross_check().unwrap();
        let bad = PerBandMargin {
            name: "s2.ah_clamp",
            direct: MS::Derived(5),
            rederived: MS::Derived(6),
        };
        assert!(bad.cross_check().is_err());
        let f_ok = PerBandMargin {
            name: "s6.pe_corr_hi",
            direct: MS::DerivedF32(1.5),
            rederived: MS::DerivedF32(1.5),
        };
        f_ok.cross_check().unwrap();
        let f_bad = PerBandMargin {
            name: "s6.pe_corr_hi",
            direct: MS::DerivedF32(1.5),
            rederived: MS::DerivedF32(1.5000001),
        };
        // Bit-level cross-check: distinct f32 bits fail even when close.
        if f_bad.direct != f_bad.rederived {
            assert!(f_bad.cross_check().is_err());
        }
        let mixed = PerBandMargin {
            name: "s4.step3_sum",
            direct: MS::Derived(0),
            rederived: MS::MarginDerivationRequired("x"),
        };
        assert!(mixed.cross_check().is_err());
        // OD-6: ah_clamp_short stays Required on both arms and cross-checks.
        let short = PerBandMargin::ah_clamp_short_required("D-R2 conservative");
        assert!(matches!(short.direct, MS::MarginDerivationRequired(_)));
        assert!(matches!(short.rederived, MS::MarginDerivationRequired(_)));
        short.cross_check().unwrap();
        assert_eq!(short.name, "s2.ah_clamp_short");
        // Margin modes token round-trip.
        assert_eq!(MarginMode::Direct.token(), "Direct");
        assert_eq!(MarginMode::ReDerived.token(), "ReDerived");
    }

    #[test]
    fn durable_write_ahead_staging_is_fail_closed() {
        let dir = std::env::temp_dir().join(format!("r22-replay-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let payload = b"synthetic replay bytes";
        let path = stage_replay(&dir, "w1", payload).expect("stage");
        // Mismatch is persisted before assertion: the file remains for inspection.
        validate_staged(&path, payload).unwrap();
        assert!(path.exists(), "staged bytes persist");
        assert!(validate_staged(&path, b"other bytes").is_err());
        assert!(path.exists(), "mismatch persists before assertion");
        // Torn record (truncated) fails closed.
        let torn = b"synthetic replay byte";
        assert!(validate_staged(&path, torn).is_err());
        std::fs::write(&path, b"short").unwrap();
        assert!(validate_staged(&path, payload).is_err());
        // Unavailable record fails closed.
        let missing = dir.join("nope.replay");
        assert!(validate_staged(&missing, payload).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        // Bad tags never stage.
        assert!(stage_replay(&dir, "a/b", payload).is_err());
    }

    #[test]
    fn e2e3_env_gate_defaults_to_read_only() {
        // E-OD-4: generation runs only under the explicit gate; the default
        // is read-only verification. This test never sets the gate.
        let saved = std::env::var(BUNDLE_ENV_GATE).ok();
        unsafe { std::env::remove_var(BUNDLE_ENV_GATE) };
        assert!(!env_generate_enabled(), "default is read-only");
        unsafe { std::env::set_var(BUNDLE_ENV_GATE, "0") };
        assert!(!env_generate_enabled(), "\"0\" is read-only");
        unsafe { std::env::set_var(BUNDLE_ENV_GATE, "yes") };
        assert!(!env_generate_enabled(), "only \"1\" enables");
        unsafe { std::env::set_var(BUNDLE_ENV_GATE, "1") };
        assert!(env_generate_enabled(), "\"1\" enables generation");
        unsafe { std::env::remove_var(BUNDLE_ENV_GATE) };
        if let Some(v) = saved {
            unsafe { std::env::set_var(BUNDLE_ENV_GATE, v) };
        }
    }

    #[test]
    fn e2e3_durable_write_with_dir_fsync_is_fail_closed() {
        // F-B: durable bundle write (file fsync + directory fsync). A torn
        // write (truncated file) fails closed on re-parse; traversal names
        // are rejected.
        let dir = std::env::temp_dir().join(format!("r22-bundle-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut set = RowSet::new();
        set.insert(Row::Frame(frame_row(Op::Op1, "mat", 3)))
            .unwrap();
        let tsv = set.render_tsv_file();
        let path = write_file_durable(&dir, "t.tsv", tsv.as_bytes()).expect("durable write");
        assert!(path.exists());
        let back = std::fs::read_to_string(&path).unwrap();
        assert_eq!(RowSet::parse_tsv_file(&back).unwrap(), set);
        // Kill-mid-write simulation: truncate to half length. The torn file
        // must fail closed (never parse as a smaller valid bundle).
        let torn_len = tsv.len() / 2;
        std::fs::write(&path, &tsv.as_bytes()[..torn_len]).unwrap();
        let torn = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            RowSet::parse_tsv_file(&torn).is_err(),
            "torn bundle fails closed"
        );
        // Traversal / absolute names rejected.
        assert!(write_file_durable(&dir, "../evil", b"x").is_err());
        assert!(write_file_durable(&dir, "/abs", b"x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn e2e3_manifest_shas_partition_round_trip() {
        // Manifest + SHA256SUMS + OP×row-type JSON partition (E-OD-8):
        // deterministic render, hash pin, and independent re-parse.
        let mut set = RowSet::new();
        set.insert(Row::Frame(frame_row(Op::Op1, "mat", 1)))
            .unwrap();
        set.insert(Row::Band(band_row(Op::Op1, "mat", 1, 0)))
            .unwrap();
        set.insert(Row::Replay(replay_row("r1"))).unwrap();
        set.insert(Row::Cov(cov_row(Op::Op1, "dim", Scope::All)))
            .unwrap();
        let tsv = set.render_tsv_file();
        let json = set.render_json_file();
        assert_eq!(tsv, set.render_tsv_file(), "tsv deterministic");
        assert_eq!(json, set.render_json_file(), "json deterministic");
        let files = vec![
            (BUNDLE_TSV_NAME, tsv.len(), sha256_of(tsv.as_bytes())),
            (BUNDLE_JSON_NAME, json.len(), sha256_of(json.as_bytes())),
        ];
        let file_refs: Vec<(&str, usize, &str)> =
            files.iter().map(|(n, b, s)| (*n, *b, s.as_str())).collect();
        let manifest = manifest_text("head", "test-platform", &file_refs, set.len(), set.len());
        assert!(manifest.starts_with("R22MANIFEST\t2\n"));
        assert!(manifest.ends_with('\n'));
        assert!(!manifest.contains("2026"), "no timestamps in manifest");
        let sha_refs: Vec<(&str, String)> = files.iter().map(|(n, _, s)| (*n, s.clone())).collect();
        let shas = shasums_text(&sha_refs);
        assert!(shas.contains(BUNDLE_TSV_NAME) && shas.contains(BUNDLE_JSON_NAME));
        // Partition keeps total order within each OP×type file and the
        // union re-parses to the same set.
        let parts = partition_op_rowtype(&set);
        assert!(!parts.is_empty());
        let mut union = RowSet::new();
        for (op, ty, rows) in &parts {
            assert!(!rows.is_empty());
            let body = render_json_partition(rows);
            assert!(body.starts_with("{\"schema\":2,\"rows\":[") && body.ends_with("]}\n"));
            let parsed = RowSet::parse_json_file(&body).unwrap();
            assert_eq!(parsed.len(), rows.len(), "partition {op} {ty} round-trips");
            for r in rows {
                union.insert(r.clone()).unwrap();
            }
        }
        assert_eq!(union, set, "partition union equals set");
    }

    #[test]
    fn geometry_serialization_preserves_channels_and_stereo_invariant() {
        let g = Geometry {
            cnt: 48,
            per: 8,
            max: 8,
            short: false,
            offsets: vec![0, 4, 8, 12],
        };
        let s = g.serialize();
        let back = Geometry::parse(&s).expect("round-trip");
        assert_eq!(back, g, "per-channel captured values preserved");
        // Stereo equality: identical geometries validate.
        validate_stereo_geometry(&g, Some(&g)).unwrap();
        // Mono always validates.
        validate_stereo_geometry(&g, None).unwrap();
        // Short mismatch fails.
        let mut short = g.clone();
        short.short = true;
        assert!(validate_stereo_geometry(&g, Some(&short)).is_err());
        // cnt/per/max mismatch fails.
        let mut cnt = g.clone();
        cnt.cnt += 1;
        assert!(validate_stereo_geometry(&g, Some(&cnt)).is_err());
        // Offsets length mismatch fails.
        let mut off = g.clone();
        off.offsets.push(16);
        assert!(validate_stereo_geometry(&g, Some(&off)).is_err());
        // Malformed inputs fail closed.
        assert!(Geometry::parse("cnt=0;per=8;max=8;short=0;offsets=0").is_none());
        assert!(Geometry::parse("cnt=48;per=8;max=8;short=2;offsets=0").is_none());
        assert!(Geometry::parse("cnt=01;per=8;max=8;short=0;offsets=0").is_none());
        assert!(Geometry::parse("cnt=48;per=8;max=8;short=0").is_none());
        assert!(Geometry::parse("cnt=48;per=8;max=8;short=0;offsets=a").is_none());
        assert!(Geometry::parse("").is_none());
    }

    #[test]
    fn new_row_types_reject_adversarial_inputs() {
        // Wrong field counts fail closed per row type.
        assert!(Row::parse_tsv_line("R22-CHAIN\tOP-1\tm\t0\tc1\t0\ts4", 2).is_err());
        assert!(Row::parse_tsv_line("R22-LEAF\tOP-1\tm\t0", 2).is_err());
        assert!(Row::parse_tsv_line("R22-REPLAY\tonly-one", 2).is_err());
        // Unknown row type, bad OP, bad stage, bad origin.
        assert!(matches!(
            Row::parse_tsv_line("R22-X\tOP-1\tm\t0\t0\tF-even\tR\tDerived", 2),
            Err(RowError::UnknownRowType(_))
        ));
        assert!(Row::parse_tsv_line("R22-CHAIN\tOP-9\tm\t0\tc1\t0\ts4\t3\troot", 2).is_err());
        assert!(Row::parse_tsv_line("R22-CHAIN\tOP-1\tm\t0\tc1\t0\ts9\t3\troot", 2).is_err());
        assert!(Row::parse_tsv_line("R22-CHAIN\tOP-1\tm\t0\tc1\t0\ts4\t3\tbogus", 2).is_err());
        // Embedded separators and id slashes fail.
        assert!(Row::parse_tsv_line("R22-CHAIN\tOP-1\ta/b\t0\tc1\t0\ts4\t3\troot", 2).is_err());
        assert!(Row::parse_tsv_line("R22-REPLAY\ta/b\tis\tAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\tMatch", 2).is_err());
        // Out-of-range ordinals/bands/frames.
        assert!(Row::parse_tsv_line("R22-CHAIN\tOP-1\tm\t1000000\tc1\t0\ts4\t3\troot", 2).is_err());
        assert!(Row::parse_tsv_line("R22-LEAF\tOP-1\tm\t0\t1000\ts4.x\t1", 2).is_err());
        // JSON: extra keys, id mismatch, unknown schema, non-canonical numbers.
        let mut extra = replay_row("k1").render_json();
        extra = extra.replace("\"status\":\"Match\"}", "\"status\":\"Match\",\"x\":1}");
        assert!(Row::parse_json_object(&extra).is_err());
        let mismatch = chain_row(Op::Op1, "m", 0, "c1", 0)
            .render_json()
            .replace("0000", "0001");
        assert!(Row::parse_json_object(&mismatch).is_err());
        assert!(matches!(
            RowSet::parse_json_file("{\"schema\":99,\"rows\":[]}"),
            Err(RowError::UnknownSchemaVersion(_))
        ));
        // Duplicate REPLAY tag rejected.
        let mut set = RowSet::new();
        set.insert(Row::Replay(replay_row("dup"))).unwrap();
        assert!(matches!(
            set.insert(Row::Replay(replay_row("dup"))),
            Err(RowError::DuplicateId(_))
        ));
    }

    #[test]
    fn all_six_row_types_round_trip_both_formats() {
        let mut set = RowSet::new();
        set.insert(Row::Frame(frame_row(Op::Op3, "mat", 7)))
            .unwrap();
        set.insert(Row::Band(band_row(Op::Op3, "mat", 7, 2)))
            .unwrap();
        set.insert(Row::Chain(chain_row(Op::Op3, "mat", 7, "k", 0)))
            .unwrap();
        set.insert(Row::Leaf(leaf_row(Op::Op3, "mat", 7, 2, "s4.x", Some(1))))
            .unwrap();
        set.insert(Row::Replay(replay_row("r1"))).unwrap();
        set.insert(Row::Cov(cov_row(Op::Op3, "dim", Scope::All)))
            .unwrap();
        let tsv = set.render_tsv_file();
        let back = RowSet::parse_tsv_file(&tsv).unwrap();
        assert_eq!(back, set);
        assert_eq!(back.render_tsv_file(), tsv);
        let json = set.render_json_file();
        let bj = RowSet::parse_json_file(&json).unwrap();
        assert_eq!(bj, set);
        let via_tsv = RowSet::parse_tsv_file(&set.render_tsv_file()).unwrap();
        let via_json = RowSet::parse_json_file(&set.render_json_file()).unwrap();
        assert_eq!(via_tsv, via_json);
    }
}
