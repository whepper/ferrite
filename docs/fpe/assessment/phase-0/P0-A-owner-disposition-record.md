# Phase 0 — P0-A Owner Disposition Record

**Authority:** Project owner (explicit decisions issued after review of the
P0-A evidence recovery report).
**Recorded by:** Qwen4 Max Preview (OpenCode), 21 November 2026.
**Repository:** `whepper/ferrite`, branch `main`.
**Checkpoint verified at recording time:** HEAD =
`eb2896923ec788727975268aca0beb27cba51760` (identical to the published
checkpoint and to `origin/main`).

This record durably captures the owner's dispositions on the P0-A report
(`docs/fpe/assessment/phase-0/P0-A-evidence-recovery-report.md`, preserved
unmodified) and the actions taken to implement them. **P0-B is not
authorized and has not been started.**

---

## 1. Owner decisions (as issued)

### D1 — `B_closed` analytical report (P0-A request D1 / OD-4)

**AUTHORIZED:** Create a provenance-labelled reconstruction as the accepted
replacement for the missing original analytical report.

Requirements (owner):
- Explicitly state that the original report was not recovered.
- Label the new document as reconstructed and independently verified.
- Preserve the mathematical derivation, assumptions and production margin
  semantics.
- Include the shift lemma, min/max Lipschitz and parity-accounting
  arguments.
- Record the 28,409,561 operand-pair verification with zero failures.
- Record the Qwen arithmetic-checker result.
- Compare the reconstructed derivation with the previously implemented
  formula.
- Do not claim historical provenance for the reconstruction.

### D2 — G-1c and FR-list (P0-A request D2 / A-04 / P0A-O1)

**AUTHORIZED:** Create a provenance-preserving absence amendment and
attribution correction.

Requirements (owner):
- Record G-1b as the last recovered reconciliation.
- Record G-1c and FR-1 through FR-4 as unrecovered.
- Correct the G-1 review attribution: the retained review is DeepSeek's.
- Record the referenced Qwen final acceptance review as missing.
- Do not invent FR definitions or infer the missing review's conclusions.
- Preserve the distinction between reproducible G-1 evidence and the
  incomplete historical acceptance chain.

### D3 — M1/M2 evidence preservation (P0-A request D3 / A-01 / A-02)

**AUTHORIZED:**
- Preserve the recovered original M1/M2 evidence bundles.
- Retain SHA-256 verification information.
- Preserve the M1 review report and remediation record.
- Preserve the M2 green-run record, execution logs and source/diff context.
- Distinguish confirmed M1 review occurrence from unconfirmed M2 review
  occurrence.
- Record that a fresh independent M2 review and sign-off is authorized.
- Do not manufacture or backdate historical sign-off documents.
- G0 remains historical and non-rerunnable. A-01 remains open.

### Not issued

- **D4 (P0-B authorization): NOT ISSUED.** P0-B (byte-level baseline,
  aggregate assertions, per-finding retention, B4 options) remains
  unauthorized and unstarted.
- **D5 (observations P0A-O3/O4/O5): NOT ISSUED.** The observations remain
  noted in the P0-A report; no action is authorized or taken.

## 2. Implementation record (actions taken under D1–D3)

| Decision | Action | Artifact |
|---|---|---|
| D1 | Reconstruction created with all owner-required elements (original-not-recovered statement; reconstructed/independently-verified labels; derivation + assumptions + production margin semantics; L1 shift lemma, L2 min/max Lipschitz, L3 parity accounting; 28,409,561-pair zero-failure verification; Qwen checker results; formula comparison — identical; no historical provenance claimed) | `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md` (new) |
| D2 | Absence amendment + attribution correction created (G-1b last recovered reconciliation; G-1c/FR-1…FR-4 unrecovered; retained G-1 review is DeepSeek's; Qwen final acceptance review missing; no invented definitions or inferred conclusions; reproducible-evidence vs incomplete-chain distinction) | `docs/fpe/R2.1-B-G1c-absence-amendment.md` (new) |
| D2 | Append-only amendment pointer added to the historical owner decision (original text unchanged): OD-4 disposition, G-1c/FR absence, attribution correction, M1/M2 preservation note | `docs/fpe/R2.1-B-owner-decision.md` (appended section only) |
| D3 | M1 bundle preserved byte-for-byte (66 files; 64/64 SHA-256 OK; `diff -rq` vs original byte-identical) | `docs/evidence/fpe-m1/ferrite-fpe-m1-review/` (new) |
| D3 | M2 bundle preserved byte-for-byte (78 files; 77/77 SHA-256 OK; `diff -rq` vs original byte-identical) | `docs/evidence/fpe-m2/ferrite-fpe-m2-review/` (new) |
| D3 | Provenance/verification record: original temp locations, base HEAD `b188415`, checksum results + supplemental pins, M1-review-confirmed vs M2-review-unconfirmed distinction, fresh M2 review authorized (not performed), no manufactured/backdated sign-offs, G0 historical / A-01 open | `docs/evidence/fpe-m1-m2-provenance.md` (new) |
| — | Phase 0 package index/status | `docs/fpe/assessment/phase-0/README.md` (new) |

The P0-A report itself was **preserved unmodified** (instruction 3); this
record supersedes its §9 "awaiting owner" status explicitly, without
rewriting it.

## 3. Evidence preserved and its location

- **M1 independent-review bundle** (original, 2026-11-17, base `b188415`):
  `docs/evidence/fpe-m1/ferrite-fpe-m1-review/` — implementation report,
  REMEDIATION.md (independent-review occurrence evidence), complete diffs,
  source copies, context excerpts, pre/post validation logs, fixture
  stability + cross-check, git snapshot, MANIFEST, SHA256SUMS.
- **M2 independent-review bundle** (original, 2026-11-17, base `b188415`):
  `docs/evidence/fpe-m2/ferrite-fpe-m2-review/` — M2 green-run record
  (24/24 bit-identity debug+release), execution logs, complete source +
  Q31-reference extracts, stage correspondence, complete working-tree diff,
  test lists, MANIFEST, SHA256SUMS.
- Integrity: M1 64/64 and M2 77/77 SHA-256 checks pass **at the preserved
  location**; copies byte-identical to the recovered originals; supplemental
  pins for checksum-excluded files recorded in
  `docs/evidence/fpe-m1-m2-provenance.md` §2.
- **Not preserved (outside D3 scope):** the R2.1-B session-temp artifacts
  (P0-A report §3.2: `is_ms.rs.orig`, `ms_bound_verify.rs`,
  `is_margin_verify.rs`, `final_math_verify.rs`, `r21b-review-checks.rs`,
  `r21b_env_audit*.log`, `r21b_gates.log`) remain in volatile session temp,
  pinned by SHA-256 in the P0-A report. Their durable preservation was not
  authorized by D1–D3 and is flagged as a **follow-up owner decision**
  (residual risk: temp cleanup would leave the reconstruction's empirical
  leg reported-only, though the essential identities are embedded in the
  reconstruction document and the committed Qwen review §3.3, and the
  retained gates re-validate the bound on every run).

## 4. Discrepancies between owner decisions and the P0-A report

**No substantive discrepancies.** The decisions match the P0-A proposals
(§3.5 option b, §4.4 option 2, §9 D3) with these recorded nuances:

1. **M1 fresh review not ordered.** P0-A §5.4 suggested the M1 sign-off
   receive "same treatment" as M2 (produce record or re-review). Owner D3
   confirms M1 review **occurrence** (via REMEDIATION.md) and authorizes a
   fresh independent review **for M2 only**. Recorded as decided: the M1
   sign-off document remains unretained (A-02 residual); no fresh M1 review
   is authorized by D3. Whether the confirmed occurrence suffices for G8
   review completeness is a later governance question, not resolved here.
2. **R2.1-B artifact preservation not authorized.** P0-A §3.2 recovered
   R2.1-B scaffolds/logs; D3 covers M1/M2 only. The artifacts stay in temp
   (§3 above); follow-up decision flagged.
3. **D4/D5 not issued.** P0-A §9 listed five requests; the owner disposed
   D1–D3 only. P0-B remains gated; observations P0A-O3/O4/O5 remain noted
   without action.
4. **Filename convention:** the reconstruction uses the exact path proposed
   in P0-A §3.5 (`R2.1-B-Bclosed-derivation-reconstruction.md`).

## 5. Status of findings after these dispositions

| Item | Status |
|---|---|
| OD-4 (analytical report) | **Disposed (D1):** original missing; reconstruction created and owner-accepted as replacement; P0-C independent audit pending. Historical references amended, not rewritten |
| A-04 (G-1c / FR-1…FR-4) | **Disposed as absence (D2):** formally recorded unrecovered; "addressed by G-1c" claim marked unverifiable; recovery by owner would supersede via further amendment |
| P0A-O1 (attribution) | **Corrected (D2):** retained G-1 review is DeepSeek's; Qwen final acceptance review recorded missing; no conclusions inferred |
| A-01 (G0) | **Open (unchanged, per D3):** historical evidence now durably preserved; G0 non-rerunnable on current tree |
| A-02 (M1/M2 reviews) | **Partially resolved:** records preserved; M1 occurrence confirmed; M2 occurrence unconfirmed; **fresh independent M2 review + sign-off authorized, not yet performed** |
| FR-5 / FR-6 (OD-6/OD-10) | **Open (unchanged):** routed to future authorized P0-B scope |
| Nine conditional acceptances | **Conditional (unchanged):** no promotion |
| Tier-B C parity | **Blocked (unchanged):** not waived |
| A-03/A-05/A-21 (aggregates, retention, baseline) | **Open:** P0-B scope, unauthorized |
| P0A-O3/O4/O5 | Noted; D5 not issued; no action |

## 6. Execution constraints honored

- No production source, test, manifest, lockfile, or dependency change
  (the preserved bundle source copies are evidence artifacts under
  `docs/evidence/`, not production source; `rust/` untouched).
- No commit, push, stage, amend, rebase, reset, cherry-pick, or clean; no
  untracked file deleted. The only tracked-file modification is the
  **append-only** amendment section of `R2.1-B-owner-decision.md`.
- Historical records corrected by explicit amendment only; the P0-A report
  and all five assessment documents preserved unmodified.
- No sign-off manufactured or backdated; no FR definitions invented; no
  missing-review conclusions inferred; no perceptual claim; no conditional
  acceptance promoted.
- **P0-B not started.**

## 7. Git integrity (at recording time)

- HEAD: `eb2896923ec788727975268aca0beb27cba51760` (unchanged; = checkpoint
  = `origin/main`).
- Tracked modifications: `docs/fpe/R2.1-B-owner-decision.md` (appended
  amendment section only; `git diff --check` clean).
- New untracked files: this record; the reconstruction; the absence
  amendment; the evidence provenance record; the two preserved bundles;
  the phase-0 index — in addition to the previously untracked five
  assessment documents and the P0-A report.
- Exact post-action `git status` is recorded in the final execution summary
  of this recording session.

**STOP — dispositions D1–D3 recorded and implemented. Awaiting owner
review. P0-B remains unauthorized and unstarted.**

---

## Amendment D4 — 21 November 2026 (P0-B authorization)

*Appended by explicit owner instruction. The historical recording above is
preserved unchanged; this amendment supersedes the §1 "D4 — NOT ISSUED"
entry and the historical "P0-B unauthorized/unstarted" statements **as of
this date only** — they remain accurate as history. D1, D2 and D3 are
untouched. D5 remains not issued.*

**Decision (owner, subsequent conversation):** The project owner explicitly
authorizes **Muse Spark 1.3** to execute **P0-B** (Reproducibility and
acceptance-baseline hardening).

**Authorized scope (exactly):**

1. R2.1-B byte-level baseline, following the established
   `production_baseline.rs` pattern.
2. Hard assertions for all five authoritative aggregate counts:
   **30 findings; 16 coding frames; 0 digest differences; 14 review-gated
   findings; 1 unsupported frame.**
3. Retention of per-finding IS operands, decision margins and trace chains.
4. Retention of Class B line vectors.
5. Deterministic serialization and ordering.
6. Explicit unsupported-frame evidence handling.
7. Creation of `P0-B-reproducibility-report.md`
   (`docs/fpe/assessment/phase-0/P0-B-reproducibility-report.md`).

**Explicit exclusion:**

- **G0 pass-through comparison mode is NOT authorized.**
- G0 remains historical and non-rerunnable.
- **A-01 remains open.**

**Technical boundaries (unchanged from the Phase 0 mission):** P0-B remains
**test-only and documentation-only**. No production numerical changes; no
Q31 behaviour changes; no R1 envelope or tolerance changes; no `B_closed`
decision-behaviour changes; no MS supported-domain changes; no IS/MS
decision-definition changes; no API, manifest, lockfile or dependency
changes; no R2.2, M3, M4 or later implementation; no G-2/G-3 implementation;
no conditional-acceptance promotion; no FPE default changes. No commits or
pushes without separate owner authorization.

**Scope notes (accuracy, no expansion):**

- Mapping to the mission's P0-B spec: item 1 = B1; item 2 = B2; items 3–4 =
  B3 (OD-7/OD-8); items 5–6 = the authorized B4 subset; item 7 = B5. The
  B4 option "G0 pass-through comparison mode" is excluded as stated above.
- FR-6 (no committed byte baseline ≡ OD-6/A-21) is addressed by scope
  item 1. **FR-5 (`sum_abs_x=37` hard assertion) is not explicitly included
  in the D4 scope and remains open** unless separately directed.
- The nine conditional acceptances remain conditional; Tier-B remains
  blocked, not waived.
- This amendment records authorization only. **P0-B implementation has not
  been started by this recording action**; execution belongs to Muse Spark
  1.3 under a separately scoped instruction, followed by P0-C independent
  audit.

---

## Amendment M2 acceptance — 24 November 2026 (formal M2 owner acceptance)

*Appended by explicit owner instruction. The historical recording above
(D1–D4) is preserved unchanged; this amendment records the owner's formal
M2 decision **as of this date only**. It does not rewrite any historical
review-occurrence statement, finding disposition or status.*

**Decision (owner):** The project owner formally **accepts M2: Explicit
Orchestration Mirror**.

1. The owner accepts M2.
2. The acceptance relies on GLM 5.3 Flash's **fresh independent review**
   dated **24 November 2026** (`docs/fpe/assessment/m2-independent-review-glm-2026-11-24.md`,
   verdict **PASS WITH FINDINGS**), covering the M2 evidence bundle
   (77/77 checksum), M2-era tree reconstruction, byte-for-byte
   reconstruction of 15 source files, independent reproduction of 24/24
   FPE tests in debug and release, architecture/output/state
   equivalence, twelve multi-frame scenarios and all ten M2 acceptance
   criteria.
3. The independent review is newly performed and does **not** recover or
   backdate historical M2 sign-off. The original M2 review occurrence
   remains **unconfirmed as history** (provenance record §4); the GLM
   review is a **separate event**.
4. All ten M2 acceptance criteria (AC-01…AC-10 in the review §7) are
   accepted.
5. Findings M2 F-1 through M2 F-4 remain **non-blocking and retained**
   with the limitations stated in the review §8 (F-1 Tier-B blocked;
   F-2 provenance/reconstruction; F-3 LFE M3+ coverage gap; F-4
   informational Clippy).
6. The acceptance is limited to the **Q31-equivalent explicit
   orchestration foundation** (review §10). It is not a float-native
   engine, not perceptual evidence, not C-reference Tier-B parity, not
   authorization to switch FPE to default, and not authorization to begin
   R2.2.

**Standing limitations unchanged by this amendment:** G0 remains
historical and non-rerunnable (A-01 open); the nine conditional R2.1-B
acceptances remain conditional; Tier-B C parity remains blocked, not
waived; G0/A-01 and all other assessment limitations are unchanged.
D5 remains not issued.