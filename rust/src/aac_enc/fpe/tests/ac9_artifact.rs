// SPDX-License-Identifier: FDK-AAC
//
// Part of Ferrite, a Third-Party Modified Version of the
// Fraunhofer FDK AAC Codec Library for Android.
// See rust/NOTICE and rust/LICENSE-FDK-AAC for the complete license text,
// modification notice, and date.
//! AC-9 supplemental evidence artifact (separate from frozen E1 schema v2).
//!
//! Generation is explicitly gated by `FPE_AC9_GENERATE=1`. The default test is
//! read-only verification. Reuses Group E's SHA-256, durable-write, manifest,
//! and checksum primitives; does not alter `R22SCHEMA`, MDEF v2, or Group E.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use super::evidence::{self as ev};

pub(super) const SCHEMA_VERSION: u32 = 1;
pub(super) const SCHEMA_HEADER: &str = "AC9SCHEMA\t1";
pub(super) const ENV_GATE: &str = "FPE_AC9_GENERATE";
pub(super) const TSV_NAME: &str = "ac9-margins.tsv";
pub(super) const JSON_NAME: &str = "ac9-margins.json";
pub(super) const LINKS_NAME: &str = "group-e-links.tsv";
pub(super) const MANIFEST_NAME: &str = "MANIFEST.txt";
pub(super) const SHA_NAME: &str = "SHA256SUMS";
pub(super) const README_NAME: &str = "README.md";

pub(super) fn artifact_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/evidence/fpe-r22-ac9")
}

pub(super) fn generation_enabled() -> bool {
    std::env::var(ENV_GATE).map(|v| v == "1").unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvidenceClass {
    Derived,
    Observed,
    Unavailable,
    Inapplicable,
    Unobserved,
    NotGenerated,
}

impl EvidenceClass {
    pub(super) fn token(self) -> &'static str {
        match self {
            Self::Derived => "Derived",
            Self::Observed => "Observed",
            Self::Unavailable => "Unavailable",
            Self::Inapplicable => "Inapplicable",
            Self::Unobserved => "Unobserved",
            Self::NotGenerated => "NotGenerated",
        }
    }

    fn parse(tok: &str) -> Option<Self> {
        match tok {
            "Derived" => Some(Self::Derived),
            "Observed" => Some(Self::Observed),
            "Unavailable" => Some(Self::Unavailable),
            "Inapplicable" => Some(Self::Inapplicable),
            "Unobserved" => Some(Self::Unobserved),
            "NotGenerated" => Some(Self::NotGenerated),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MarginRow {
    pub id: String,
    pub stage: String,
    pub material: String,
    pub frame: u32,
    pub(super) channel: Option<usize>,
    pub band: usize,
    pub arm: String,
    pub probe: String,
    pub classification: EvidenceClass,
    pub operands: String,
    pub equation: String,
    pub threshold: String,
    pub value: Option<i64>,
    pub decision: String,
    pub equality: String,
    pub source: String,
}

impl MarginRow {
    pub(super) fn body(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.stage.clone(),
            self.material.clone(),
            self.frame.to_string(),
            self.channel
                .map(|c| c.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            self.band.to_string(),
            self.arm.clone(),
            self.probe.clone(),
            self.classification.token().to_string(),
            self.operands.clone(),
            self.equation.clone(),
            self.threshold.clone(),
            self.value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            self.decision.clone(),
            self.equality.clone(),
            self.source.clone(),
        ]
    }

    pub(super) fn render_tsv(&self) -> String {
        let mut fields = vec!["AC9-MARGIN".to_string()];
        fields.extend(self.body());
        fields.join("\t")
    }

    pub(super) fn render_json(&self) -> String {
        let keys = [
            "type",
            "id",
            "stage",
            "material",
            "frame",
            "channel",
            "band",
            "arm",
            "probe",
            "classification",
            "operands",
            "equation",
            "threshold",
            "value",
            "decision",
            "equality",
            "source",
        ];
        let mut fields = vec!["AC9-MARGIN".to_string()];
        fields.extend(self.body());
        let mut out = String::from("{");
        for (i, (key, value)) in keys.iter().zip(fields.iter()).enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{key}\":"));
            if matches!(*key, "frame" | "channel" | "band") {
                if value == "n/a" {
                    out.push_str("null");
                } else {
                    out.push_str(value);
                }
            } else if *key == "value" {
                if value == "n/a" {
                    out.push_str("null");
                } else {
                    out.push_str(value);
                }
            } else {
                out.push_str(&ev::json_str(value));
            }
        }
        out.push('}');
        out
    }

    pub(super) fn parse_tsv(fields: &[&str]) -> Result<Self, String> {
        if fields.len() != 17 || fields[0] != "AC9-MARGIN" {
            return Err("bad AC9-MARGIN field count/type".to_string());
        }
        let number = |tok: &str, name: &str| -> Result<u64, String> {
            let v: u64 = tok.parse().map_err(|_| format!("bad {name}"))?;
            if v.to_string() != tok {
                return Err(format!("non-canonical {name}"));
            }
            Ok(v)
        };
        let frame = number(fields[4], "frame")?;
        let channel = if fields[5] == "n/a" {
            None
        } else {
            Some(number(fields[5], "channel")? as usize)
        };
        let band = number(fields[6], "band")?;
        if frame > u32::MAX as u64 || band > usize::MAX as u64 {
            return Err("numeric field overflow".to_string());
        }
        let classification = EvidenceClass::parse(fields[9]).ok_or("bad classification")?;
        let value = if fields[13] == "n/a" {
            None
        } else {
            let v: i64 = fields[13].parse().map_err(|_| "bad value")?;
            if v.to_string() != fields[13] {
                return Err("non-canonical value".to_string());
            }
            Some(v)
        };
        if classification == EvidenceClass::Derived && value.is_none() {
            return Err("Derived row without value".to_string());
        }
        if !matches!(
            classification,
            EvidenceClass::Derived | EvidenceClass::Observed
        ) && value.is_some()
        {
            return Err("non-numeric class carries value".to_string());
        }
        if fields[5] == "n/a" && !matches!(fields[2], "is" | "ms") {
            return Err("n/a channel outside pair-level stage".to_string());
        }
        for (idx, tok) in fields[1..].iter().enumerate() {
            if *tok == "n/a"
                && !matches!(
                    classification,
                    EvidenceClass::Unavailable
                        | EvidenceClass::Inapplicable
                        | EvidenceClass::Unobserved
                        | EvidenceClass::NotGenerated
                )
                && idx != 4
                && idx != 13
            {
                return Err("unexpected n/a token".to_string());
            }
        }
        Ok(Self {
            id: fields[1].to_string(),
            stage: fields[2].to_string(),
            material: fields[3].to_string(),
            frame: frame as u32,
            channel,
            band: band as usize,
            arm: fields[7].to_string(),
            probe: fields[8].to_string(),
            classification,
            operands: fields[10].to_string(),
            equation: fields[11].to_string(),
            threshold: fields[12].to_string(),
            value,
            decision: fields[14].to_string(),
            equality: fields[15].to_string(),
            source: fields[16].to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GroupELink {
    pub material: String,
    pub frame: u32,
    pub band: Option<usize>,
    pub op: String,
    pub row_id: String,
    pub row_sha: String,
    pub bundle_tsv_sha: String,
    pub bundle_json_sha: String,
    pub schema: u32,
}

impl GroupELink {
    fn render_tsv(&self) -> String {
        format!(
            "AC9-LINK\t{}\tf{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.material,
            self.frame,
            self.band
                .map(|b| b.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            self.op,
            self.row_id,
            self.row_sha,
            self.bundle_tsv_sha,
            self.bundle_json_sha,
            self.schema,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Artifact {
    pub rows: Vec<MarginRow>,
    pub links: Vec<GroupELink>,
    pub group_e_tsv_sha: String,
    pub group_e_json_sha: String,
    pub group_e_shas_sha: String,
    pub head: String,
    pub platform: String,
}

impl Artifact {
    fn sorted(&mut self) {
        self.rows.sort_by(|a, b| {
            (
                &a.id,
                &a.stage,
                &a.material,
                a.frame,
                a.channel,
                a.band,
                &a.arm,
                &a.probe,
            )
                .cmp(&(
                    &b.id,
                    &b.stage,
                    &b.material,
                    b.frame,
                    b.channel,
                    b.band,
                    &b.arm,
                    &b.probe,
                ))
        });
        self.links.sort_by(|a, b| {
            (&a.material, a.frame, a.band, &a.op, &a.row_id).cmp(&(
                &b.material,
                b.frame,
                b.band,
                &b.op,
                &b.row_id,
            ))
        });
        for i in 1..self.rows.len() {
            assert_ne!(self.rows[i - 1].id, self.rows[i].id, "duplicate AC9 row id");
        }
    }

    pub(super) fn tsv(&self) -> String {
        let mut out = String::from("AC9SCHEMA\t1\n");
        for row in &self.rows {
            out.push_str(&row.render_tsv());
            out.push('\n');
        }
        out
    }
    pub(super) fn json(&self) -> String {
        let mut out = String::from("{\"schema\":1,\"rows\":[");
        for (i, row) in self.rows.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&row.render_json());
        }
        out.push_str("]}\n");
        out
    }

    pub(super) fn links_tsv(&self) -> String {
        let mut out = String::from("AC9LINKSCHEMA\t1\n");
        for link in &self.links {
            out.push_str(&link.render_tsv());
            out.push('\n');
        }
        out
    }

    pub(super) fn manifest(&self, files: &[(&str, usize, &str)]) -> String {
        let mut out = format!(
            "AC9MANIFEST\t1\nschema: 1\nhead: {}\nplatform: {}\ncommand: FPE_AC9_GENERATE=1 AC9_TEST_WRITE=1 cargo test -p ferrite-aac --lib ac9_supplement_generate_and_verify\nrows: {}\nlinks: {}\ngroup_e_tsv_sha: {}\ngroup_e_json_sha: {}\ngroup_e_shas_sha: {}\nfiles:\n",
            self.head,
            self.platform,
            self.rows.len(),
            self.links.len(),
            self.group_e_tsv_sha,
            self.group_e_json_sha,
            self.group_e_shas_sha,
        );
        for (name, bytes, sha) in files {
            out.push_str(&format!("{sha}  {name}  {bytes}\n"));
        }
        out
    }

    pub(super) fn readme(&self) -> String {
        format!(
            "# R2.2 AC-9 Supplemental Margin Evidence\n\n\
             Separate artifact schema `AC9SCHEMA 1`; the frozen E1 `R22SCHEMA 2` and MDEF v2 are unchanged.\n\
             Generated by `FPE_AC9_GENERATE=1 AC9_TEST_WRITE=1 cargo test -p ferrite-aac --lib ac9_supplement_generate_and_verify`.\n\
             `AC9_TEST_WRITE=1` is the separate write gate; `FPE_AC9_GENERATE=1` alone only collects and checks determinism. Default verification is read-only.\n\n\
             - Rows: {} margin records; {} Group E identity links. `Derived`/`Observed` rows carry exact values; `Unavailable`/`Inapplicable`/`Unobserved`/`NotGenerated` rows carry `n/a`.\n\
             - Group E TSV SHA-256: `{}`.\n\
             - Group E JSON SHA-256: `{}`.\n\
             - Group E SHA256SUMS SHA-256: `{}`.\n\
             - Classes: Derived / Observed / Unavailable / Inapplicable / Unobserved / NotGenerated. `Inapplicable` is the AC9SCHEMA 1 token for structural not-applicability (the report's `NotApplicable`); it is distinct from `Unobserved` (applicable but not reached) and `NotGenerated` (not emitted by the probed path).\n\
             - Numeric values are exact i64 Q31/bits/counts or exact f32 bits rendered as i64 where applicable; `n/a` is never a numeric zero.\n\
             - S4 evidence distinguishes step-1 coarser trials, step-2 finer trials, step-3 bit-saves, the final `gmin+60` clamp, and the all-dead/gain-inversion branch; PE-fail and gate-off cases are `Inapplicable`, and step-2 bands skipped by an earlier rejection are `NotGenerated`.\n\
             - S1 evidence includes explicit patch execution, channel-1 MS-coupled inheritance, and the per-band patch-write gate; unexecuted patch writes are `Inapplicable`, never zero observations.\n\
             - S2 evidence includes avoid-hole peak/valley minSnr modification and MS minSnr/spread cross-tweaks; ATS-disabled cases are `Inapplicable`.\n\
             - I9 evidence carries exact `lr` phase residual and the selected `ed`/`es` energy residual; bands that do not reach I9 are `Inapplicable` with a reason.\n\
             - S6 evidence carries the synthetic crash chains plus the real-corpus per-pass sweep (`S6-PROBE-CORPUS`) with pass-level element cap, MAX_QUANT, exit headroom, multipass entry, reduce/crash-error branches, the full exit-predicate set, exit branches, crash/fallback applicability, and an explicit `Unobserved` next pass. Accounting rows cover fill floor/extension and pre-S6 reservoir guards. S6 probe rows are not linked to OP-1 Group E bands because the S6 corpus is a separate operating point.\n\
             - `group-e-links.tsv` points to existing R2.2 row IDs and records both bundle hashes. Synthetic S6 and S6 corpus probe evidence have no Group E row and use explicit probe identities.\n\
             - No Group E file is regenerated or modified by this artifact.\n",
            self.rows.len(),
            self.links.len(),
            self.group_e_tsv_sha,
            self.group_e_json_sha,
            self.group_e_shas_sha,
        )
    }
}

#[derive(Debug)]
pub(super) enum ArtifactError {
    Io(String),
    Format(String),
    Hash(String),
    Link(String),
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(s) | Self::Format(s) | Self::Hash(s) | Self::Link(s) => write!(f, "{s}"),
        }
    }
}

fn read(dir: &Path, name: &str) -> Result<Vec<u8>, ArtifactError> {
    std::fs::read(dir.join(name)).map_err(|e| ArtifactError::Io(format!("read {name}: {e}")))
}

fn parse_tsv(text: &str) -> Result<Vec<MarginRow>, ArtifactError> {
    if !text.starts_with("AC9SCHEMA\t1\n") {
        return Err(ArtifactError::Format("bad AC9 schema header".to_string()));
    }
    if !text.ends_with('\n') || text.contains('\r') {
        return Err(ArtifactError::Format(
            "AC9 TSV must be LF with trailing newline".to_string(),
        ));
    }
    let mut rows = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for (i, line) in text.lines().skip(1).enumerate() {
        if line.is_empty() {
            return Err(ArtifactError::Format(format!("empty line {}", i + 2)));
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let row = MarginRow::parse_tsv(&fields)
            .map_err(|e| ArtifactError::Format(format!("line {}: {e}", i + 2)))?;
        if !ids.insert(row.id.clone()) {
            return Err(ArtifactError::Format("duplicate AC9 id".to_string()));
        }
        rows.push(row);
    }
    Ok(rows)
}

fn parse_json(text: &str) -> Result<Vec<MarginRow>, ArtifactError> {
    const KEYS: [&str; 17] = [
        "type",
        "id",
        "stage",
        "material",
        "frame",
        "channel",
        "band",
        "arm",
        "probe",
        "classification",
        "operands",
        "equation",
        "threshold",
        "value",
        "decision",
        "equality",
        "source",
    ];
    let value =
        ev::parse_json_toplevel(text).map_err(|e| ArtifactError::Format(format!("json: {e:?}")))?;
    let envelope = match value {
        ev::JsonVal::Obj(fields) => fields,
        _ => return Err(ArtifactError::Format("bad AC9 JSON envelope".to_string())),
    };
    if envelope.len() != 2
        || envelope[0].0 != "schema"
        || envelope[1].0 != "rows"
        || !matches!(&envelope[0].1, ev::JsonVal::Num(n) if n == "1")
    {
        return Err(ArtifactError::Format("bad AC9 JSON envelope".to_string()));
    }
    let items = match &envelope[1].1 {
        ev::JsonVal::Arr(items) => items,
        _ => return Err(ArtifactError::Format("bad AC9 JSON rows".to_string())),
    };
    let mut rows = Vec::with_capacity(items.len());
    for item in items {
        let object = match item {
            ev::JsonVal::Obj(fields) => fields,
            _ => return Err(ArtifactError::Format("bad AC9 JSON row".to_string())),
        };
        if object.len() != KEYS.len()
            || object
                .iter()
                .zip(KEYS.iter())
                .any(|((key, _), want)| key != want)
        {
            return Err(ArtifactError::Format(
                "bad AC9 JSON row keys/order".to_string(),
            ));
        }
        let text = |i: usize| -> Result<String, ArtifactError> {
            match &object[i].1 {
                ev::JsonVal::Str(s) => Ok(s.clone()),
                _ => Err(ArtifactError::Format(format!(
                    "json {} must be a string",
                    KEYS[i]
                ))),
            }
        };
        let number = |i: usize| -> Result<String, ArtifactError> {
            match &object[i].1 {
                ev::JsonVal::Num(n) => Ok(n.clone()),
                _ => Err(ArtifactError::Format(format!(
                    "json {} must be an integer",
                    KEYS[i]
                ))),
            }
        };
        let optional = |i: usize| -> Result<String, ArtifactError> {
            match &object[i].1 {
                ev::JsonVal::Num(n) => Ok(n.clone()),
                ev::JsonVal::Null => Ok("n/a".to_string()),
                _ => Err(ArtifactError::Format(format!(
                    "json {} must be an integer or null",
                    KEYS[i]
                ))),
            }
        };
        let tokens = [
            text(0)?,
            text(1)?,
            text(2)?,
            text(3)?,
            number(4)?,
            optional(5)?,
            number(6)?,
            text(7)?,
            text(8)?,
            text(9)?,
            text(10)?,
            text(11)?,
            text(12)?,
            optional(13)?,
            text(14)?,
            text(15)?,
            text(16)?,
        ];
        let fields: Vec<&str> = tokens.iter().map(String::as_str).collect();
        let row = MarginRow::parse_tsv(&fields)
            .map_err(|e| ArtifactError::Format(format!("json row: {e}")))?;
        rows.push(row);
    }
    for i in 1..rows.len() {
        if rows[i - 1].id == rows[i].id {
            return Err(ArtifactError::Format("duplicate AC9 id".to_string()));
        }
    }
    Ok(rows)
}

fn parse_links(text: &str) -> Result<Vec<GroupELink>, ArtifactError> {
    if !text.starts_with("AC9LINKSCHEMA\t1\n") || !text.ends_with('\n') || text.contains('\r') {
        return Err(ArtifactError::Format(
            "bad links header/newlines".to_string(),
        ));
    }
    let mut links = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 10 || f[0] != "AC9-LINK" {
            return Err(ArtifactError::Format("bad link row".to_string()));
        }
        let frame = f[2]
            .strip_prefix("f")
            .and_then(|v| v.parse::<u32>().ok())
            .ok_or_else(|| ArtifactError::Format("link frame".to_string()))?;
        let band = if f[3] == "n/a" {
            None
        } else {
            Some(
                f[3].parse::<usize>()
                    .map_err(|_| ArtifactError::Format("link band".to_string()))?,
            )
        };
        let schema = f[9]
            .parse::<u32>()
            .map_err(|_| ArtifactError::Format("link schema".to_string()))?;
        if schema != 2 {
            return Err(ArtifactError::Format(
                "link must target frozen schema 2".to_string(),
            ));
        }
        links.push(GroupELink {
            material: f[1].to_string(),
            frame,
            band,
            op: f[4].to_string(),
            row_id: f[5].to_string(),
            row_sha: f[6].to_string(),
            bundle_tsv_sha: f[7].to_string(),
            bundle_json_sha: f[8].to_string(),
            schema,
        });
    }
    Ok(links)
}

fn verify_hashes(dir: &Path, manifest: &str, shas: &str) -> Result<Vec<String>, ArtifactError> {
    let mut names = Vec::new();
    for line in shas.lines() {
        let mut it = line.splitn(2, "  ");
        let sha = it
            .next()
            .ok_or_else(|| ArtifactError::Hash("shas line".to_string()))?;
        let name = it
            .next()
            .ok_or_else(|| ArtifactError::Hash("shas line".to_string()))?;
        if name == SHA_NAME {
            return Err(ArtifactError::Hash("SHA256SUMS lists itself".to_string()));
        }
        let bytes = read(dir, name)?;
        if ev::sha256_of(&bytes) != sha {
            return Err(ArtifactError::Hash(format!("sha mismatch {name}")));
        }
        if name != MANIFEST_NAME {
            let needle = format!("{sha}  {name}  {}", bytes.len());
            if !manifest.contains(&needle) {
                return Err(ArtifactError::Hash(format!("manifest missing {name}")));
            }
        }
        names.push(name.to_string());
    }
    for required in [TSV_NAME, JSON_NAME, LINKS_NAME, MANIFEST_NAME, README_NAME] {
        if !names.iter().any(|n| n == required) {
            return Err(ArtifactError::Hash(format!("missing {required}")));
        }
    }
    Ok(names)
}

pub(super) fn verify(dir: &Path, group_e: &Path) -> Result<usize, ArtifactError> {
    let tsv_text = String::from_utf8(read(dir, TSV_NAME)?)
        .map_err(|e| ArtifactError::Format(e.to_string()))?;
    let json_text = String::from_utf8(read(dir, JSON_NAME)?)
        .map_err(|e| ArtifactError::Format(e.to_string()))?;
    let links_text = String::from_utf8(read(dir, LINKS_NAME)?)
        .map_err(|e| ArtifactError::Format(e.to_string()))?;
    let manifest = String::from_utf8(read(dir, MANIFEST_NAME)?)
        .map_err(|e| ArtifactError::Format(e.to_string()))?;
    let shas = String::from_utf8(read(dir, SHA_NAME)?)
        .map_err(|e| ArtifactError::Format(e.to_string()))?;
    let via_tsv = parse_tsv(&tsv_text)?;
    let via_json = parse_json(&json_text)?;
    if via_tsv != via_json {
        return Err(ArtifactError::Format("TSV/JSON mismatch".to_string()));
    }
    let links = parse_links(&links_text)?;
    if links.len() > via_tsv.len() {
        return Err(ArtifactError::Link(
            "more links than margin rows".to_string(),
        ));
    }
    verify_hashes(dir, &manifest, &shas)?;
    let ge_tsv = ev::sha256_of(&read(group_e, ev::BUNDLE_TSV_NAME)?);
    let ge_json = ev::sha256_of(&read(group_e, ev::BUNDLE_JSON_NAME)?);
    let ge_shas = ev::sha256_of(&read(group_e, ev::SHA_NAME)?);
    for expected in [&ge_tsv, &ge_json, &ge_shas] {
        if !manifest.contains(expected) {
            return Err(ArtifactError::Link(
                "Group E hash not in manifest".to_string(),
            ));
        }
    }
    let bundle_text = String::from_utf8(read(group_e, ev::BUNDLE_TSV_NAME)?)
        .map_err(|e| ArtifactError::Io(e.to_string()))?;
    let bundle_set = ev::RowSet::parse_tsv_file(&bundle_text)
        .map_err(|e| ArtifactError::Link(format!("Group E parse: {e}")))?;
    let row_ids: std::collections::BTreeSet<String> =
        bundle_set.rows_in_order().iter().map(|r| r.id()).collect();
    let mut row_hashes = std::collections::BTreeMap::new();
    for row in bundle_text.lines() {
        row_hashes.insert(ev::sha256_of(row.as_bytes()), row);
    }
    for link in &links {
        if link.bundle_tsv_sha != ge_tsv || link.bundle_json_sha != ge_json || link.schema != 2 {
            return Err(ArtifactError::Link(format!(
                "bad Group E link {}",
                link.row_id
            )));
        }
        let expected_row_id = if link.band.is_some() {
            format!(
                "R22-BAND/{}/{}/{:06}/{:03}",
                link.op,
                link.material,
                link.frame,
                link.band.unwrap()
            )
        } else {
            format!("R22-FRAME/{}/{}/{:06}", link.op, link.material, link.frame)
        };
        if expected_row_id != link.row_id || !row_ids.contains(&link.row_id) {
            return Err(ArtifactError::Link("row id mismatch/not found".to_string()));
        }
        let row = row_hashes
            .get(&link.row_sha)
            .ok_or_else(|| ArtifactError::Link(format!("row hash not found {}", link.row_id)))?;
        let f: Vec<&str> = row.split('\t').collect();
        let identity_ok = if link.band.is_some() {
            f.len() >= 8
                && f[0] == "R22-BAND"
                && f[1] == link.op
                && f[2] == link.material
                && f[3] == link.frame.to_string()
                && f[4] == link.band.unwrap().to_string()
        } else {
            f.len() >= 8
                && f[0] == "R22-FRAME"
                && f[1] == link.op
                && f[2] == link.material
                && f[3] == link.frame.to_string()
        };
        if !identity_ok {
            return Err(ArtifactError::Link(format!(
                "row identity mismatch {}",
                link.row_id
            )));
        }
    }
    ev::verify_bundle_dir(group_e).map_err(ArtifactError::Link)?;
    Ok(via_tsv.len())
}

pub(super) fn write(
    dir: &Path,
    mut artifact: Artifact,
    group_e: &Path,
) -> Result<usize, ArtifactError> {
    artifact.sorted();
    let tsv = artifact.tsv();
    let json = artifact.json();
    let links = artifact.links_tsv();
    let readme = artifact.readme();
    let mut content = vec![
        (TSV_NAME.to_string(), tsv.into_bytes()),
        (JSON_NAME.to_string(), json.into_bytes()),
        (LINKS_NAME.to_string(), links.into_bytes()),
        (README_NAME.to_string(), readme.into_bytes()),
    ];
    // Reuse Group E's durable file writer. It permits top-level and json/
    // names, all of which are used here.
    let refs: Vec<(&str, usize, String)> = content
        .iter()
        .map(|(n, b)| (n.as_str(), b.len(), ev::sha256_of(b)))
        .collect();
    let borrowed: Vec<(&str, usize, &str)> =
        refs.iter().map(|(n, l, s)| (*n, *l, s.as_str())).collect();
    let manifest = artifact.manifest(&borrowed);
    let manifest_bytes = manifest.into_bytes();
    let mut sha_inputs: Vec<(&str, String)> =
        refs.iter().map(|(n, _, s)| (*n, s.clone())).collect();
    sha_inputs.push((MANIFEST_NAME, ev::sha256_of(&manifest_bytes)));
    let shas = ev::shasums_text(&sha_inputs);
    content.push((MANIFEST_NAME.to_string(), manifest_bytes));
    content.push((SHA_NAME.to_string(), shas.into_bytes()));
    for (name, bytes) in &content {
        ev::write_file_durable(dir, name, bytes).map_err(|e| ArtifactError::Io(e.to_string()))?;
    }
    let n = verify(dir, group_e)?;
    Ok(n)
}

pub(super) fn head() -> Result<String, ArtifactError> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
        .output()
        .map_err(|e| ArtifactError::Io(format!("git rev-parse: {e}")))?;
    if !out.status.success() {
        return Err(ArtifactError::Io("git rev-parse failed".to_string()));
    }
    Ok(String::from_utf8(out.stdout)
        .map_err(|e| ArtifactError::Format(e.to_string()))?
        .trim()
        .to_string())
}

pub(super) fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

pub(super) fn group_e_hashes(group_e: &Path) -> Result<(String, String, String), ArtifactError> {
    Ok((
        ev::sha256_of(&read(group_e, ev::BUNDLE_TSV_NAME)?),
        ev::sha256_of(&read(group_e, ev::BUNDLE_JSON_NAME)?),
        ev::sha256_of(&read(group_e, ev::SHA_NAME)?),
    ))
}

pub(super) fn artifact_from_rows(
    rows: Vec<MarginRow>,
    group_e: &Path,
) -> Result<Artifact, ArtifactError> {
    let (group_e_tsv_sha, group_e_json_sha, group_e_shas_sha) = group_e_hashes(group_e)?;
    let links = build_links(&rows, group_e)?;
    Ok(Artifact {
        rows,
        links,
        group_e_tsv_sha,
        group_e_json_sha,
        group_e_shas_sha,
        head: head()?,
        platform: platform(),
    })
}

pub(super) fn build_links(
    rows: &[MarginRow],
    group_e: &Path,
) -> Result<Vec<GroupELink>, ArtifactError> {
    let (tsv_sha, json_sha, _) = group_e_hashes(group_e)?;
    let bundle_text = String::from_utf8(read(group_e, ev::BUNDLE_TSV_NAME)?)
        .map_err(|e| ArtifactError::Io(e.to_string()))?;
    ev::RowSet::parse_tsv_file(&bundle_text)
        .map_err(|e| ArtifactError::Link(format!("Group E parse: {e}")))?;
    let bundle = bundle_text;
    let mut candidates_by_identity: BTreeMap<(String, String, u32, usize), Vec<&str>> =
        BTreeMap::new();
    for line in bundle.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 8 && f[0] == "R22-BAND" {
            if let (Some(op), Some(frame), Some(band)) = (
                ev::Op::parse(f[1]),
                f[3].parse::<u32>().ok(),
                f[4].parse::<usize>().ok(),
            ) {
                candidates_by_identity
                    .entry((op.label().to_string(), f[2].to_string(), frame, band))
                    .or_default()
                    .push(line);
            }
        }
    }
    let mut links = Vec::new();
    let mut seen = BTreeMap::new();
    for row in rows {
        if row.material == "noise-mono" && row.probe == "S6-PROBE-SYNTH" {
            continue;
        }
        // S6 corpus pass rows are real-corpus probe evidence, but they are
        // not OP-1 Group E band identities (the S6 corpus is a separate
        // operating point). They are intentionally unlinked rather than
        // mis-attributed to an unrelated Group E band row.
        if row.probe == "S6-PROBE-CORPUS" {
            continue;
        }
        // Accounting rows are frame-aggregate evidence, not a single Group E
        // band identity. They are intentionally unlinked.
        if row.probe == "R22-ACCT-CAPTURE" {
            continue;
        }
        let candidates: &Vec<&str> = candidates_by_identity
            .iter()
            .find(|((_, material, frame, band), _)| {
                material == &row.material && *frame == row.frame && *band == row.band
            })
            .map(|(_, v)| v)
            .ok_or_else(|| {
                ArtifactError::Link(format!(
                    "no Group E band link for {} {} f{} b{}",
                    row.id, row.material, row.frame, row.band
                ))
            })?;
        let exact = candidates
            .iter()
            .find(|line| line.split('\t').nth(5) == Some(row.stage.as_str()));
        let line = if row.stage == "is" || row.stage == "ms" {
            exact.or_else(|| candidates.first()).copied()
        } else {
            exact.copied()
        }
        .or_else(|| {
            if row.stage == "pe" || row.stage == "s2" || row.stage == "s4" {
                candidates.first().copied()
            } else {
                None
            }
        })
        .ok_or_else(|| {
            ArtifactError::Link(format!(
                "no Group E band link for {} {} f{} b{}",
                row.id, row.material, row.frame, row.band
            ))
        })?;
        let f: Vec<&str> = line.split('\t').collect();
        let key = (
            f[1].to_string(),
            f[2].to_string(),
            f[3].to_string(),
            f[4].to_string(),
        );
        if seen.contains_key(&key) {
            continue;
        }
        seen.insert(key, ());
        links.push(GroupELink {
            material: f[2].to_string(),
            frame: row.frame,
            band: Some(row.band),
            op: f[1].to_string(),
            row_id: format!(
                "R22-BAND/{}/{}/{:06}/{:03}",
                f[1], f[2], row.frame, row.band
            ),
            row_sha: ev::sha256_of(line.as_bytes()),
            bundle_tsv_sha: tsv_sha.clone(),
            bundle_json_sha: json_sha.clone(),
            schema: 2,
        });
    }
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> MarginRow {
        MarginRow {
            id: "AC9-MARGIN/test/synthetic/000000/chNA/b000/Q31/summary".to_string(),
            stage: "s6".to_string(),
            material: "synthetic".to_string(),
            frame: 0,
            channel: Some(0),
            band: 0,
            arm: "Q31".to_string(),
            probe: "TEST".to_string(),
            classification: EvidenceClass::Derived,
            operands: "a=1".to_string(),
            equation: "a".to_string(),
            threshold: "0".to_string(),
            value: Some(1),
            decision: "test".to_string(),
            equality: "test".to_string(),
            source: "test".to_string(),
        }
    }

    #[test]
    fn ac9_supplement_generate_and_verify() {
        let group_e = ev::bundle_dir();
        if !generation_enabled() {
            let n = verify(&artifact_dir(), &group_e).expect("default read-only AC-9 verify");
            assert!(n > 0);
            return;
        }
        let (rows, s1s2_frames, s4_frames) = super::super::ac9::collect_ac9_evidence();
        assert_eq!(s1s2_frames, 24);
        assert_eq!(s4_frames, 24);
        assert!(rows
            .iter()
            .any(|r| r.name == "s6.crash_stop_distance" && r.value == Some(34)));
        assert_eq!(
            rows.iter()
                .filter(|r| r.name == "s6.crash_step" && r.value == Some(0))
                .count(),
            16
        );
        assert!(rows
            .iter()
            .filter(|r| r.name == "s4.step3_sum")
            .all(|r| r.evidence_class == "Derived"));
        assert!(
            rows.iter().any(|r| r.name == "s4.step1_dmax"),
            "step-1 evidence present"
        );
        assert!(
            rows.iter().any(|r| r.name == "s4.step2_sum"),
            "step-2 evidence present"
        );
        assert!(
            rows.iter().any(|r| r.name == "s4.final_clamp"),
            "final-clamp evidence present"
        );
        assert!(
            rows.iter().any(|r| r.name == "is.i9_phase" && r.value.is_some()),
            "exact I9 lr evidence present"
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r.name == "s6.element_cap_pass" && r.probe == "S6-PROBE-CORPUS")
                .count(),
            20,
            "S6 per-pass corpus sweep present"
        );
        let artifact_rows: Vec<MarginRow> = rows
            .iter()
            .map(|r| MarginRow {
                id: format!(
                    "AC9-MARGIN/{}/{}/{:06}/ch{}/b{:03}/{}/{}/{}",
                    r.name,
                    r.material,
                    r.frame,
                    r.channel
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "NA".to_string()),
                    r.band,
                    r.arm,
                    r.probe,
                    r.occurrence
                ),
                stage: r.stage.to_string(),
                material: r.material.clone(),
                frame: r.frame,
                channel: r.channel,
                band: r.band,
                arm: r.arm.to_string(),
                probe: r.probe.to_string(),
                classification: EvidenceClass::parse(r.evidence_class).expect("class"),
                operands: r.operands.clone(),
                equation: r.equation.to_string(),
                threshold: r.threshold.to_string(),
                value: r.value,
                decision: r.decision.to_string(),
                equality: r.equality.to_string(),
                source: r.source.to_string(),
            })
            .collect();
        assert!(
            artifact_rows
                .iter()
                .all(|r| r.material != "noise-mono" || r.probe.starts_with("S6-PROBE")),
            "noise-mono is synthetic only for S6 probe evidence"
        );
        let mut artifact = artifact_from_rows(artifact_rows, &group_e).expect("artifact links");
        artifact.sorted();
        let rendered_once = (artifact.tsv(), artifact.json(), artifact.links_tsv());
        let mut artifact_second = artifact.clone();
        artifact_second.sorted();
        assert_eq!(
            rendered_once,
            (
                artifact_second.tsv(),
                artifact_second.json(),
                artifact_second.links_tsv()
            ),
            "artifact render deterministic"
        );
        if std::env::var_os("AC9_TEST_WRITE").is_none() {
            return;
        }
        // Rendering is pure and sorted over the collected rows.
        let tmp = std::env::temp_dir().join(format!("ac9-supplement-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let n = write(&tmp, artifact.clone(), &group_e).expect("temp write/verify");
        assert_eq!(n, artifact.rows.len());
        let tsv_path = tmp.join(TSV_NAME);
        let bytes = std::fs::read(&tsv_path).unwrap();
        std::fs::write(&tsv_path, &bytes[..bytes.len() / 2]).unwrap();
        assert!(
            verify(&tmp, &group_e).is_err(),
            "torn supplement fails closed"
        );
        let _ = std::fs::remove_dir_all(&tmp);
        let n2 = write(&artifact_dir(), artifact, &group_e).expect("committed supplement");
        assert_eq!(n, n2);
        assert_eq!(ev::SCHEMA_VERSION, 2);
        assert_eq!(ev::SCHEMA_HEADER, "R22SCHEMA\t2");
        assert_eq!(ev::MDEF_VERSION, 2);
    }

    #[test]
    fn ac9_schema_header_and_round_trip() {
        let mut artifact = Artifact {
            rows: vec![sample_row()],
            links: Vec::new(),
            group_e_tsv_sha: "0".repeat(64),
            group_e_json_sha: "1".repeat(64),
            group_e_shas_sha: "2".repeat(64),
            head: "0".repeat(40),
            platform: "test".to_string(),
        };
        artifact.sorted();
        assert_eq!(parse_tsv(&artifact.tsv()).unwrap(), artifact.rows);
        assert_eq!(parse_json(&artifact.json()).unwrap(), artifact.rows);
        assert!(parse_tsv("R22SCHEMA\t2\n").is_err());
        assert!(parse_tsv("AC9SCHEMA\t01\n").is_err());
    }

    #[test]
    fn ac9_unavailable_has_no_numeric_value() {
        let mut row = sample_row();
        row.classification = EvidenceClass::Unavailable;
        row.value = None;
        assert!(MarginRow::parse_tsv(&[
            "AC9-MARGIN",
            &row.id,
            "s2",
            "synthetic",
            "0",
            "0",
            "0",
            "Q31",
            "TEST",
            "Unavailable",
            "n/a",
            "n/a",
            "n/a",
            "n/a",
            "n/a",
            "n/a",
            "n/a"
        ])
        .is_ok());
        row.value = Some(0);
        let fields = row.body();
        let refs: Vec<&str> = fields.iter().map(|s| s.as_str()).collect();
        let mut all = vec!["AC9-MARGIN"];
        all.extend(refs);
        assert!(MarginRow::parse_tsv(&all).is_err());
    }
}
