# Phase 0 — Finalization Preparation Report (RA-2/RA-3/RA-4)

**Package:** Owner-authorized documentation corrections RA-2/RA-3/RA-4 plus
Phase 0 commit inventory for owner review.
**Implementer:** Muse Spark 1.3 Contributor (OpenCode), implementation agent.
**Date:** 23 November 2026.
**Repository:** `whepper/ferrite`, branch `main`.
**Audited checkpoint:** `eb2896923ec788727975268aca0beb27cba51760`.
**Authority:** Project-owner mission "Ferrite FPE — Phase 0 Finalization
Preparation (RA-2/RA-3/RA-4)" (23 November 2026). Documentation and
preparation only. No commit, push, staging, or history operation authorized
or performed.
**Authoritative inputs:** `P0-C-remediation-independent-audit.md`,
`P0-C-remediation-report.md`, Phase 0 `README.md`,
`R2.1-B-Bclosed-derivation-reconstruction.md`, `R2.1-B-owner-decision.md`,
`P0-A-owner-disposition-record.md`, `docs/evidence/fpe-r21b-provenance.md`.

---

## 1. Initial repository state and checkpoint verification

Verified **before** any edit (repo root, `git`):

| Check | Result |
|---|---|
| `git rev-parse HEAD` | `eb2896923ec788727975268aca0beb27cba51760` — **identical to audited checkpoint** |
| `git rev-list --left-right --count HEAD...origin/main` | `0 0` — HEAD = `origin/main`, nothing ahead/behind |
| `git stash list` | empty |
| `git diff --cached --name-only` | empty — **nothing staged** |
| Tracked modifications (2) | `M docs/fpe/R2.1-B-owner-decision.md` (46/0, pre-existing P0-A D1–D3 append-only amendment); `M rust/src/aac_enc/fpe/tests/is_ms.rs` (1400/11, P0-B 1230/9 + N3/N4/N6 test-only additions) |
| Untracked entries | **163** (full list in §6; matches the independent audit §3 authorized Phase 0 set plus the audit report itself) |
| Total `git status -uall` lines | **165** (2 tracked + 163 untracked) |
| Manifest/lockfile diff (`rust/Cargo.toml`, `rust/Cargo.lock`, `rust/convert/Cargo.toml`) | empty |
| `git diff --check` (before edits) | clean (exit 0) |

No assumption of an unchanged tree was made: the state above was read
directly and matches the audit's recorded state (§3: same HEAD, same 46/0 +
1400/11 tracked diff, same untracked composition) with the single expected
addition of the audit report file itself
(`P0-C-remediation-independent-audit.md`, the audit's only addition).

---

## 2. Exact files modified for RA-2, RA-3, RA-4

Only **three untracked working-tree documentation files** were edited. No
tracked file, test file, production source, manifest, lockfile, or audit
record was modified by this task.

| ID | File | Kind |
|---|---|---|
| RA-2 | `docs/fpe/assessment/phase-0/README.md` | 1 table-cell wording correction |
| RA-3 | `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md` | 8 surgical replacements (1 parenthetical + 1 disposition paragraph + 6 line-cite replacements) |
| RA-4 | `docs/fpe/assessment/phase-0/P0-C-remediation-report.md` | 1 command-path annotation correction (§6 row 14) |

Explicitly **not** modified: `P0-C-remediation-independent-audit.md`
(independent historical audit record — read only); `is_ms.rs` (FR-5/FR-6
test implementation already authorized and complete — untouched);
`R2.1-B-owner-decision.md` (tracked P0-A amendment — untouched);
all evidence bundles and provenance records (read/inspected only).

---

## 3. Before/after descriptions of each correction

### 3.1 RA-2 — README remediation-report status cell

**Location:** `docs/fpe/assessment/phase-0/README.md`, §"Package contents"
table, `P0-C-remediation-report.md` row, Status column.

- **Before:** `Pending (2026-11-23; prepared for owner review, uncommitted)`
- **After:** `Complete (2026-11-23; uncommitted, pending owner review)`

**Rationale (per audit §4.1):** the standing status (§"Standing status")
already records "N1–N7 **remediated** in `P0-C-remediation-report.md`,
uncommitted" — the remediation work is complete; what is pending is owner
review and the owner-authorized commit. The old cell could be read as
"remediation incomplete". The new wording preserves the completed-work /
pending-review-and-commit distinction and is mutually consistent with the
standing status and the D4 scope note. No historical statement was
rewritten.

### 3.2 RA-3 — reconstruction document residuals

File: `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md`.
Provenance header (§1: reconstructed + derived, owner-accepted D1
replacement; original not recovered) and the explicit statement that the
original analytical report was not recovered are **unchanged**. Formula,
derivation, assumptions, conclusions, and accepted-reconstruction status
are **unchanged**.

**Residual 1 — §3.2 `+3` parenthetical (superseded pre-N6 description).**

- **Before:** `(The implementation's doc comment attributes '+3' to "the
  strict-'>' unit ('−1') plus halving floors" — a loose description: the
  '−1' cancels in the delta. No numerical consequence.)`
- **After:** `(The implementation's 'b_closed' doc comment, corrected under
  P0-C N6, now records this exact accounting — '1' ('min_tld' parity) +
  '2' (four-way ceil grouping) — and notes the '−1' in 'M' cancels in the
  delta; the pre-N6 "strict-'>' unit plus halving floors" wording is
  superseded. No numerical consequence.)`
- **Effect:** describes the current exact accounting (1 from `min_tld`
  parity; 2 from four-way ceiling grouping; the `−1` in `M` cancels in the
  delta) as required. Derivation text and formula untouched.

**Residual 2 — §6 scaffold currency (superseded N5 disposition).**

- **Before:** code comment `# Recovered original scaffolds (outside the
  repository; session temp)` plus paragraph `The scaffolds currently reside
  in volatile session temp (P0-A report §3.2 pins their SHA-256); their
  preservation into durable storage was 'not' part of owner decision D3 and
  is flagged as a follow-up … so the verification is reconstructible even
  if the temp artifacts are lost.`
- **After:** code comment `# Recovered original scaffolds (historical P0-A
  re-run record; session temp paths below are no longer all available —
  see N5 disposition note)` plus paragraph: `P0-C N5 disposition (see
  'docs/evidence/fpe-r21b-provenance.md'): the surviving originals R1
  ('is_ms.rs.orig') and R5 ('r21b-review-checks.rs') are preserved
  byte-for-byte in 'docs/evidence/fpe-r21b/ferrite-fpe-r21b-review/'
  (SHA-256 verified); R2–R4 and R6–R8 are unavailable (volatile
  session-temp cleanup after the P0-C audit), their SHA-256 pins are
  retained, and unavailable artifacts have not been reconstructed. The
  scaffold commands above are therefore a historical re-run record: the
  28.4-million-pair verification (§5.1) cannot now be rerun from the
  unavailable R2–R4/R6–R8 originals. The essential identities are embedded
  above and in 'R2.1-B-independent-review-qwen.md' §3.3, and the bound
  remains re-validated on every run by the retained corpus gates, so the
  derivation is checkable even though the temp artifacts are lost.`
- **Effect:** reflects the actual N5 disposition (R1/R5 preserved
  byte-for-byte; R2–R4/R6–R8 unavailable with pins retained; nothing
  reconstructed) and no longer implies the original scaffold can be rerun
  from unavailable artifacts. Retained-gate reproduction commands
  unchanged.

**Residual 3 — stale pre-remediation line cites (all replaced by stable
function references; every replacement verified against current source
before writing).**

| Site | Before | After (verified) |
|---|---|---|
| §2.1 operand-domain reading | `` (`is_ms.rs:300-308`) `` | `` (`fn b_closed` doc comment and `fn validate_ms_band` delta computation in `rust/src/aac_enc/fpe/tests/is_ms.rs`) `` — deltas documented at `b_closed` doc comment (current lines 584–593) and computed as absolutes in `validate_ms_band` (current lines 660–665) |
| §2.2 transcription | `` (`rust/src/aac_enc/fpe/tests/is_ms.rs:253-290`) `` | `` (`fn ms_first_pass_margins` in `rust/src/aac_enc/fpe/tests/is_ms.rs`) `` — function verified at current lines 534–571 |
| §2.3 supported domain | `` (`is_ms.rs:326-348`) `` | `` (`fn ms_band_supported` in `rust/src/aac_enc/fpe/tests/is_ms.rs`) `` — function verified at current lines 611–633 |
| §3.3 robustness assertion | `` at `is_ms.rs:399-407` `` | `` in `fn validate_ms_band` (`rust/src/aac_enc/fpe/tests/is_ms.rs`) `` — robustness `debug_assert!` verified at current lines 689–692 inside `validate_ms_band` (651–694) |
| §4.1 retained implementation | `` `is_ms.rs:309-314` `` | `` `fn b_closed` in `rust/src/aac_enc/fpe/tests/is_ms.rs` `` — function verified at current lines 594–599; code block unchanged |
| §7 references | `` `rust/src/aac_enc/fpe/tests/is_ms.rs:246-409` (margin transcription, `b_closed`, supported domain, validation, robustness assertion) `` | `` `rust/src/aac_enc/fpe/tests/is_ms.rs` (`fn ms_first_pass_margins` margin transcription, `fn b_closed`, `fn ms_band_supported` supported domain, `fn validate_ms_band` validation and robustness assertion) `` |

Post-edit scan: `grep -n "is_ms.rs:[0-9]"` on the document returns **no
matches**. No formula, bound value, domain rule, tightness observation, or
conclusion was altered.

### 3.3 RA-4 — licensing command path

**Location:** `P0-C-remediation-report.md`, §6 row 14, Command column.

- **Before:** `` `bash tools/check-licensing.sh` (repo root) ``
- **After:** `` `bash rust/tools/check-licensing.sh` (from repo root) ``

**Verification:** `ls rust/tools/check-licensing.sh` exists (executable);
`ls tools/check-licensing.sh` confirms no such file (repo-root `tools/`
contains only `aac-enc-metrics`, `c-enc-bench`, `enc-oracle`). Reported
result and validation outcome (`OK`, 86 files) **unchanged** — only the
path annotation was imprecise.

---

## 4. Consistency review results

- **README internal consistency:** table (P0-A complete 2026-11-21; P0-B
  complete 2026-11-21; P0-C audit complete 2026-11-21; remediation
  **Complete** 2026-11-23 uncommitted pending review; final report not
  started), standing status (P0-A/B complete, P0-C complete, overall PASS
  WITH FINDINGS pending disposition; N1–N7 remediated uncommitted;
  conditionals conditional; Tier-B blocked; G0 historical A-01 open; A-02
  partially resolved; FR-5 open accepted limitation; FR-6 working-tree pins
  pending commit), and D4 scope note (B1–B4 authorized; G0 excluded; FR-5
  outside scope and open; FR-6 addressed by working-tree pins) are mutually
  consistent. The RA-2 wording removes the only flagged ambiguity.
- **Historical preservation:** no preserved report or record was
  rewritten. P0-A report, P0-C audit, disposition record (incl. D4
  amendment), owner-decision history (amended append-only), and the
  independent remediation audit were left byte-untouched; superseded
  statements are annotated in place (RA-3 parenthetical/disposition note),
  not rewritten.
- **Unchanged by design:** FR-5 limitation (printed, not hard-asserted —
  no assertion added); D1–D4 owner decisions; all validation results,
  digests (`0x0544_1ee2_9087_55aa`), aggregate pins (30/16/0/14/1,
  `some=242 none=109`), evidence pins, SHA-256/SHA-1 pins, and audit
  conclusions. The independent audit report was not altered.
- **No contradictory live status** remains in the Phase 0 README or the
  edited reports beyond the explicitly historical statements covered by
  the append-only amendment policy (same determination as audit §4.1).

---

## 5. Validation commands and exact results

All from `rust/` except `git diff --check` (repo root). Toolchain as
reported by the remediation/audit: rustc/cargo 1.99.1,
`aarch64-apple-darwin`.

| # | Command | Exact result |
|---|---|---|
| 1 | `git diff --check` | clean (exit 0) |
| 2 | `cargo fmt -- --check` | clean (exit 0) |
| 3 | `cargo test --locked --lib 'fpe::'` | **132 passed, 0 failed**, 385 filtered |
| 4 | `cargo test --locked --release --lib 'fpe::'` | **132 passed, 0 failed** |
| 5 | `cargo test --locked --lib production_baseline` | **2 passed, 0 failed** |
| 6 | `cargo test --locked --release --lib production_baseline` | **2 passed, 0 failed** |

No test or expectation was modified to make any suite pass; on failure the
instruction was to report the exact failure — there were no failures.
Full-lib / Tier-A / API / doctest / clippy suites from audit §4.7 were not
re-run in this task (out of the six mandated commands); their last
independently verified values (500/0 Tier-A, 6/6 API, 19/19 doctests,
504/11/2 full-lib with the established Tier-B blocked set, 185 clippy
warnings with zero `is_ms` attribution) are carried forward unchanged and
are not claimed as re-verified here.

**Change confinement (verified):**
tracked diff remains exactly the two authorized files (46/0 owner-decision
amendment + 1400/11 `is_ms.rs`); no manifest/lockfile/dependency diff
(`rust/Cargo.toml`, `rust/Cargo.lock`, `rust/convert/Cargo.toml` empty);
the only code file touched remains `rust/src/aac_enc/fpe/tests/is_ms.rs`,
compiled test-only (`#[cfg(test)] mod tests;` at `fpe/mod.rs:44`);
no production numerical, Q31, decision-definition, envelope, `B_closed`,
supported-domain, API, licensing, or ADR change.

---

## 6. Final tracked and untracked working-tree inventory

- HEAD: `eb2896923ec788727975268aca0beb27cba51760` (= `origin/main`, 0/0).
- `git diff --cached --name-only`: empty. `git stash list`: empty.
- Tracked modifications (2): `docs/fpe/R2.1-B-owner-decision.md` (46/0);
  `rust/src/aac_enc/fpe/tests/is_ms.rs` (1400/11).
- Untracked (164 after this report is written: 163 pre-existing + this
  report). The authoritative machine-readable inventory is
  `git status --porcelain -uall` (166 lines total: 2 modified + 164
  untracked; verified counts: M1 bundle 66, M2 bundle 78, R2.1-B bundle 3,
  phase-0 docs 7 + this report, assessment context docs 5, D1/D2 docs 2,
  provenance records 2, tracked modifications 2).
  Every exact path is enumerated individually in the proposed commit groups
  in §7 (16 + 1 + 149 = 166 working-tree entries; each §6 path appears
  exactly once across §7.1–§7.3).

---

## 7. Proposed logical commit groups (no staging performed)

All paths verified to exist in the working tree via `git status
--porcelain -uall` (§6 method). Suggested application order: (1) → (2) →
(3); (4) deferred until after (2) is committed and a separately
authorized capture is performed. Staging order within each group is the
listed order.

### 7.1 `docs(fpe): finalize phase 0 evidence and dispositions`

Documentation corrections (N1/N2/N6 + RA-2/RA-3/RA-4), owner dispositions,
assessment reports, and the independent audit. Purpose per path in
parentheses.

**Tracked (1):**

- `docs/fpe/R2.1-B-owner-decision.md` (pre-existing P0-A D1–D3 append-only
  amendment, 46/0; historical text preserved, amendment appended; untouched
  by this task)

**Untracked — D1/D2 reconstruction and amendment (2):**

- `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md` (D1
  owner-accepted reconstruction + RA-3 precision corrections from this
  task; formula and conclusions unchanged)
- `docs/fpe/R2.1-B-G1c-absence-amendment.md` (D2 absence amendment +
  attribution correction; preserved)

**Untracked — Phase 0 package (8, incl. this report):**

- `docs/fpe/assessment/phase-0/README.md` (package index; N1-corrected +
  RA-2 status precision from this task)
- `docs/fpe/assessment/phase-0/P0-A-evidence-recovery-report.md`
  (preserved evidence-recovery record)
- `docs/fpe/assessment/phase-0/P0-A-owner-disposition-record.md`
  (preserved D1–D4 dispositions incl. D4 amendment)
- `docs/fpe/assessment/phase-0/P0-B-reproducibility-report.md`
  (N2-corrected byte-baseline/aggregate report)
- `docs/fpe/assessment/phase-0/P0-C-independent-audit.md` (preserved P0-C
  audit)
- `docs/fpe/assessment/phase-0/P0-C-remediation-report.md` (N1–N7
  remediation record + RA-4 command-path correction from this task)
- `docs/fpe/assessment/phase-0/P0-C-remediation-independent-audit.md`
  (independent audit of the remediation; historical record — included for
  completeness, never edited)
- `docs/fpe/assessment/phase-0/phase-0-finalization-preparation-report.md`
  (**this report**; preparation record + commit inventory)

**Untracked — assessment context docs (5, pre-existing working-tree
context inventoried by the audits):**

- `docs/fpe/assessment/findings-register.md` (findings ledger context)
- `docs/fpe/assessment/implementation-roadmap.md` (roadmap context)
- `docs/fpe/assessment/next-phase-recommendation.md` (planning context)
- `docs/fpe/assessment/post-checkpoint-assessment.md` (assessment context)
- `docs/fpe/assessment/validation-roadmap.md` (validation context,
  cites the BandRow emission model referenced by N3)

**Group 1 totals:** 1 tracked modification + 15 untracked files = 16
paths. No code, no evidence bytes.

### 7.2 `test(fpe): pin R2.1-B evidence and identifiers`

Test-only implementation changes (P0-B B1–B4 + P0-C N3/N4/N6). No
production, manifest, lockfile, dependency, API, or numerical change.

**Tracked (1):**

- `rust/src/aac_enc/fpe/tests/is_ms.rs` (1400/11 vs HEAD: P0-B byte
  baseline + 5 aggregate assertions + per-finding evidence + ordering +
  unsupported handling; N3 canonical-emission test + full-tuple ordering
  strengthening; N4 `unique_id`/`unique_evidence_ids` + identifier test;
  N6 `b_closed` doc-comment citation/accounting; `#[cfg(test)]`-gated)

**Group 2 totals:** 1 tracked file. Depends on nothing else to review,
but Group 1 documents describe exactly what this file implements (§3–§4
of the remediation report); commit **after** Group 1 if atomic review is
preferred, or together — owner choice. The deferred Group 4 snapshot can
only be captured after this file is committed (it is the emitting code).

### 7.3 `docs(fpe): preserve phase 0 evidence bundles`

Byte-preserved evidence and provenance/checksum records (D3 + N5). All
files read-only evidence; nothing reconstructed.

**Provenance records (2):**

- `docs/evidence/fpe-m1-m2-provenance.md` (D3 M1/M2 provenance + checksum
  pins)
- `docs/evidence/fpe-r21b-provenance.md` (N5 R1–R8 preservation register:
  R1/R5 preserved, R2–R4/R6–R8 unavailable with pins retained)

**R2.1-B preserved bundle (3):**

- `docs/evidence/fpe-r21b/ferrite-fpe-r21b-review/SHA256SUMS` (2/2 OK
  manifest)
- `docs/evidence/fpe-r21b/ferrite-fpe-r21b-review/is_ms.rs.orig` (R1,
  99,960 B original)
- `docs/evidence/fpe-r21b/ferrite-fpe-r21b-review/r21b-review-checks.rs`
  (R5, 6,546 B original)

**M1 preserved bundle (66 files):**

- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/MANIFEST.md`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/README.md`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/REMEDIATION.md`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/SHA256SUMS`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/context/config-PsyEngineKind-and-methods.rs.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/context/config-seam-tests.rs.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/context/psy-PsyChannel-and-accessors.rs.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/context/psy-PsyScratch.rs.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/context/psy-process_pair_reuse.rs.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/context/stream_driver-frontend.rs.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/context/stream_driver-psy_tns_info.rs.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/context/stream_driver-with_config.rs.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/context/thresholds-PreEchoState.rs.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/context/tns-TnsChannel.rs.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/m1-complete.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/m1-tracked.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/new-fpe-constants.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/new-fpe-energy.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/new-fpe-mod.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/new-fpe-orchestrate.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/new-fpe-output.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/new-fpe-preecho.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/new-fpe-tests-differential.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/new-fpe-tests-mod.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/new-fpe-thresholds.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/diff/new-make_fpe_fixtures.diff`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/modified-files/rust/src/aac_enc.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/modified-files/rust/src/aac_enc/config.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/modified-files/rust/src/aac_enc/psy.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/modified-files/rust/src/aac_enc/stream_driver.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/modified-files/rust/src/aac_enc/testdata.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/new-files/rust/src/aac_enc/fpe/constants.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/new-files/rust/src/aac_enc/fpe/energy.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/new-files/rust/src/aac_enc/fpe/mod.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/new-files/rust/src/aac_enc/fpe/orchestrate.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/new-files/rust/src/aac_enc/fpe/output.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/new-files/rust/src/aac_enc/fpe/preecho.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/new-files/rust/src/aac_enc/fpe/tests/differential.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/new-files/rust/src/aac_enc/fpe/tests/mod.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/new-files/rust/src/aac_enc/fpe/thresholds.rs`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/new-files/tools/enc-oracle/make_fpe_fixtures.py`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/report/M1-REPORT.md`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/COMMANDS.md`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/fixture-crosscheck.md`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/fixture-stability.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/fixtures-run1/fpe-allzero-stereo-48k.wav`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/fixtures-run1/fpe-fullscale-square-stereo-48k.wav`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/fixtures-run1/fpe-max-headroom-stereo-48k.wav`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/fixtures-run1/fpe-mixed-pair-stereo-48k.wav`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/fixtures-run1/fpe-near-silence-ramp-stereo-48k.wav`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/fixtures-run1/fpe-pow2-boundary-stereo-48k.wav`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/git-status.txt`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/fresh-verification.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/post-bench.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/post-clippy.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/post-doctest.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/post-fmt.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/post-fpe-rerun.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/post-full-lib.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/post-integration.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/post-nodefault.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/post-package.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/post-tierA.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/post-change/remediation-validation.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/pre-change/fpe-m1-pre-baseline-release.log`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/validation/pre-change/fpe-m1-pre-tierA-debug.log`

**M2 preserved bundle (78 files):**

- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/00-README.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/MANIFEST.txt`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/SHA256SUMS`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/00-project-charter.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/01-current-state-audit.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/02-reuse-and-translation-inventory.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/03-fdk-reference-analysis.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/04-system-architecture.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/05-floating-point-translation-spec.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/06-encoder-integration.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/07-validation-strategy.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/08-performance-strategy.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/09-implementation-roadmap.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/10-risk-register.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/11-coding-model-guidelines.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/12-licensing-provenance.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/README.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/adr/0001-engine-separation.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/adr/0002-floating-point-representation.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/adr/0003-psychoacoustic-interface.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/adr/0004-differential-validation.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/adr/0005-representation-lane-retention.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/adr/README.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/fpe-planning/m0-verification.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/guidance/12-licensing-provenance.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/guidance/AGENTS.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/docs/stage-correspondence.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/git/diff-complete.patch`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/git/diff-new-files.patch`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/git/diff-tracked.patch`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/git/head-and-branch.txt`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/git/status-full.txt`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/git/status-porcelain.txt`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/q31-reference/energy.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/q31-reference/psy-orchestration-and-helpers.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/q31-reference/q31.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/q31-reference/sfb.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/q31-reference/thresholds.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/q31-reference/tns.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/report/m1-context.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/report/m2-implementation-report.md`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/fpe/constants.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/fpe/energy.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/fpe/mod.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/fpe/orchestrate.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/fpe/output.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/fpe/preecho.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/fpe/tests/differential.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/fpe/tests/m2_stages.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/fpe/tests/mod.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/fpe/thresholds.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/modified-rust/aac_enc.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/modified-rust/config.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/modified-rust/psy.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/modified-rust/stream_driver.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/source/modified-rust/testdata.rs`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/tests/list-fpe-release.txt`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/tests/list-ignored-debug.txt`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/tests/list-ignored-release.txt`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/tests/list-lib-release.txt`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/build-default.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/build-no-default-features.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/clippy.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/commands.txt`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/doctests.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/fmt-check.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/fpe-debug.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/fpe-release.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/fpe-seam-debug.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/fpe-seam-release.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/full-lib-release-no-skip.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/git-diff-check.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/licensing.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/package-list.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/production-api.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/production-baseline.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/tier-a-debug.log`
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/validation/tier-a-release.log`

**Group 3 totals:** 2 provenance records + 3 R2.1-B bundle files + 66 M1
files + 78 M2 files = **149 paths**. Independent of Group 2 code; cited by
Group 1 provenance records (R1/R5 pins, M1/M2 checksums).

### 7.4 `docs(fpe): preserve canonical R2.1-B evidence` — DEFERRED

No files created in this task. After Group 2 is committed under separate
owner authorization, capture once on the reference platform
(`aarch64-apple-darwin`, toolchain rustc/cargo 1.99.1) via the documented
command `cargo test --locked --lib
r21b_p0b_canonical_evidence_is_emitted -- --nocapture` (from `rust/`) and
check in under `docs/evidence/fpe-r21b/` (exact filenames to be fixed at
capture time; expected shape per audit §4.3):

- `docs/evidence/fpe-r21b/canonical-evidence-rows-<checkpoint>.txt`
  (expected: 30 `R21B-CANONICAL` headers + 30 complete canonical rows +
  `R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x05441ee2908755aa`)
- SHA-256 pin record for the snapshot (e.g. appended `SHA256SUMS` entry or
  adjacent provenance note — to be fixed at capture time)
- Short provenance note (command, HEAD, toolchain, platform, date)

**Dependency:** requires the Group 2 test code to be committed first so the
snapshot has a durable basis; the Group 1 remediation report §3 already
documents the generation procedure. This task only records the plan.

### 7.5 Overlap and dependency summary

- Groups 1–3 are disjoint path sets (16 + 1 + 149 = 166 working-tree
  entries: every §6 path appears exactly once).
- Group 1 (docs) describes Groups 2 (code) and 3 (bundles) but contains
  neither; reviewing Group 1 first gives the acceptance basis for 2–3.
- Group 2 (test) is the executable basis for the Group 4 snapshot and for
  every digest/aggregate cited in Group 1 — it must be committed before
  Group 4 is captured.
- Group 3 (bundles) is self-contained evidence; its pins are cited by
  Group 1 docs and the reconstruction.
- Group 4 is empty in this task by explicit exclusion (RA-1 deferred).

*Inventory method note: the authoritative machine-readable inventory is
`git status --porcelain -uall` itself (165 lines before this report, 166
after — verified in-task). The §7.1–§7.3 per-group lists were verified
file-by-file against that output (M1 66, M2 78, R2.1-B bundle 3, phase-0
docs 7 + this report, assessment context 5, D1/D2 docs 2, provenance
records 2, tracked modifications 2) and are the commit-authoritative
enumeration; `git status --porcelain -uall` governs in case of any
discrepancy.*

---

## 8. Explicit confirmation — RA-1 snapshot capture remains deferred

RA-1 (durable checked-in snapshot of the 30-row canonical evidence stream)
is **accepted as a carry-forward item but NOT performed in this task**, per
the owner disposition (capture only after the test implementation is
committed in a separately authorized operation). No snapshot file
(`canonical-evidence-rows-*.txt`), checksum entry, or provenance note for
such a snapshot was created, and §7.4 contains only the deferred plan. The
emission model itself (retained test + digest pin) is unchanged from the
P0-C remediation.

## 9. Explicit confirmation — FR-5 remains an accepted limitation

`sum_abs_x=37` remains **printed but not individually hard-asserted**. No
additional assertion, test, or implementation for FR-5 was added in this
task (no `is_ms.rs` change of any kind was made here). FR-5 is carried
forward exactly as the accepted documented limitation recorded in the
remediation report §7, the README standing status, and the independent
audit §4.7/§6.

## 10. Explicit confirmation — no staging, commits, pushes, or history operations

- `git add` / staging: **none** (`git diff --cached` empty before and
  after; §1 and §5/§6).
- Commits / pushes: **none** (HEAD unchanged
  `eb2896923ec788727975268aca0beb27cba51760` throughout; `origin/main`
  0 ahead/behind).
- Amend / rebase / reset / cherry-pick / history rewriting: **none**
  (reflog entry remains the baseline commit; no such command run).
- `git clean` / deletion of untracked files: **none** (all 163
  pre-existing untracked paths still present plus this report).
- All changes left **unstaged** in the working tree for owner review.

## 11. Explicit confirmation — R2.2 and subsequent phases were not started

No G0 pass-through implementation, R2.2, M3/M4, G-2/G-3, float-kernel,
production numerical, Q31, envelope, decision-definition, API, manifest,
lockfile, dependency, ADR, Tier-B waiver/reclassification, conditional-
acceptance promotion, or R2–R4/R6–R8 reconstruction work was begun or
performed. The nine conditional acceptances remain conditional; Tier-B C
parity remains blocked; G0 remains historical/non-rerunnable. Validation
in §5 touched only the six mandated read-only test/check commands.

---

**STOP — documentation corrections (RA-2/RA-3/RA-4), consistency review,
validation, and commit inventory complete. All changes unstaged. RA-1
canonical snapshot not captured. R2.2 not begun. Awaiting project-owner
review and separate authorization for any Git operations.**
