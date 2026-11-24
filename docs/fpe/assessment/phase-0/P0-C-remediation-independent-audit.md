# Phase 0 — P0-C Remediation Independent Audit (N1–N7)

**Package:** Independent audit of the P0-C remediation of findings N1–N7.
**Auditor:** Qwen4 Max Preview (OpenCode), acting as independent auditor — not
the remediation implementer (Muse Spark 1.3).
**Date:** 23 November 2026.
**Repository:** `whepper/ferrite`, branch `main`.
**Checkpoint / HEAD audited:** `eb2896923ec788727975268aca0beb27cba51760`
(= `origin/main`, 0 ahead / 0 behind — re-verified at audit start and end).
**Authority:** Project owner mission "Ferrite FPE — Independent Audit of P0-C
Remediation" (23 November 2026). This audit verifies only. It does not
implement fixes, modify source, stage, commit, push, clean, or begin R2.2.
The only repository addition is this report.

**Classification vocabulary (per finding):** PASS / PASS WITH FINDINGS / FAIL /
NOT VERIFIED.

---

## 1. Executive assessment

The remediation of P0-C findings N1–N7 is **technically sound, correctly
scoped, and accurately represented**. I independently verified — not merely
reviewed — the following:

- **N1:** The Phase 0 README now records P0-A complete, P0-B complete, P0-C
  audit complete, N1–N7 remediated, overall PASS WITH FINDINGS pending owner
  disposition and finalization, FR-5 as an accepted limitation, and FR-6
  awaiting commit. One minor status-cell wording ambiguity is recorded (§4.1).
- **N2:** The +1230/−9 P0-B accounting is **cryptographically confirmed**: the
  audit-time diff preserved in session temp reconstructs the exact P0-B blob
  (`e22a0a82…`) from the HEAD blob (`a38ee4ad…`); the current working tree
  (`a937963c…`) differs from P0-B only by the N3/N4/N6 additions. The
  `some=242 / none=109` values reproduce from the retained gates and match the
  committed implementation note ("242 MS_SOME frames"); their meaning (Q31-arm
  MS digest-state frame counts) is verified in code. Corrections are explicitly
  annotated; no historical result or conclusion was silently changed.
- **N3:** The retained emission test prints all **30 complete canonical rows**;
  every row carries all required evidence fields; the unsupported F-01 row is
  retained explicitly; emission is byte-identical across processes and across
  debug/release; and an **auditor-written independent FNV-1a 64 re-hash** of the
  captured rows reproduces the pinned digest `0x0544_1ee2_9087_55aa` in all four
  captures. The documented capture command works. The corpus depends on no
  volatile session files. **Durability residual:** the emitted stream is
  reproducible but not checked in as a durable artifact; the emitting code
  itself is uncommitted (FR-6/N7). Classified and scoped in §4.3.
- **N4:** All 30 unique identifiers are distinct; F-01/F-02 are disambiguated
  (`#1`/`#2`); original `stable_id` labels are byte-preserved and remain the
  canonical `id=` field; ordinals are deterministic (in-test double corpus run +
  cross-process identity); no classification, decision, or numerical behaviour
  changed. The ordinal scheme's corpus-version relativity is documented as a
  limitation (§4.4).
- **N5:** R1 and R5 are preserved **byte-for-byte** (`cmp` against the original
  temp copies), SHA-256 pins match P0-A §3.2 exactly, the R1 SHA-1 hash chain
  matches, `SHA256SUMS` 2/2 OK. R2–R4 and R6–R8 are confirmed absent from temp
  and explicitly recorded unavailable with pins retained; nothing was
  reconstructed. M1 (64/64, 66 files) and M2 (77/77, 78 files) bundles are
  intact, supplemental pins match.
- **N6:** The `b_closed` doc comment now cites the owner-accepted D1
  reconstruction document with the exact `+3` accounting; the formula text and
  the implementation body are **byte-identical** to HEAD (verified by direct
  extraction and diff).
- **N7:** Every reported count reproduces exactly: FPE **132/132** debug and
  release; Q31 baseline **2/2** debug and release; Tier-A **500 passed / 0
  failed / 2 ignored / 15 filtered**; API **6/6**; doctests **19/19**; full
  library **504/11/2** with the 11 failures **exactly** the established
  artifact-gated Tier-B set; `cargo fmt --check` and `git diff --check` clean;
  cross-process output byte-identical; clippy **185 lib-test warnings, zero
  `is_ms` attribution**; licensing check OK (86 files).
- **Repository integrity preserved:** HEAD = checkpoint = `origin/main`; no
  commits, pushes, stages, stashes, or history operations; tracked diff is
  exactly the two authorized files; no manifest/lockfile/dependency change; no
  deletions; no production numerical change (test-only `#[cfg(test)]` file).

**No blocking defects were found.** Residual items are owner closure actions
(commit authorization, FR-5 decision) and minor documentation-precision nits,
plus the N3 durability gap classified in §4.3.

**Overall recommendation: PASS WITH FINDINGS — OWNER DISPOSITION REQUIRED** (§9).

---

## 2. Independent verification methodology

Independence was maintained by re-executing, re-deriving, and re-hashing rather
than trusting the remediation report:

1. **Full read of required inputs:** P0-C independent audit, P0-C remediation
   report, Phase 0 README, P0-A owner disposition record (incl. D4), P0-B
   reproducibility report, the `B_closed` reconstruction, the R2.1-B owner
   decision (committed text + working-tree amendment), the R2.1-B provenance
   record, and the preserved R1/R5 artifacts and checksum manifests.
2. **Actual diff inspection:** complete working-tree diff captured and read
   line-by-line; all 11 deleted lines accounted for; the remediation delta
   isolated by reconstructing the P0-B state from the audit-time diff.
3. **Blob-hash chain verification:** `git hash-object` of the reconstructed
   P0-B file matched the audit-time diff's target blob exactly, making the
   +1230/−9 accounting a cryptographic fact, not a report claim.
4. **Independent test execution** of every required suite (§8), from `rust/`,
   `--locked`, toolchain rustc/cargo **1.99.1** (`58cf8f9c3` / `5f94df478`),
   `aarch64-apple-darwin` — the recorded reference platform.
5. **Independent digest re-hash:** an auditor-written Python FNV-1a 64
   implementation (not derived from the Rust code) parsed the captured emission
   output, validated row structure, sorted the canonical strings
   lexicographically, joined with `\n`, and re-hashed — reproducing the pinned
   digest in four separate captures.
6. **Cross-process and cross-profile determinism:** two separate process runs
   each of the byte-baseline and emission tests, plus debug-vs-release
   comparison; `diff` clean in all cases.
7. **Checksum verification:** `shasum -a 256 -c SHA256SUMS` in all three
   preserved bundles; `cmp` of R1/R5 against the original temp copies;
   supplemental-pin comparison against the provenance records; filesystem
   inventory of temp to confirm R2–R4/R6–R8 absence.
8. **Code-level semantic verification:** `some/none` counter semantics,
   recorder placement after replay-equality gates, `cfg(test)` gating,
   `b_closed` body extraction and diff, sort/ordinal logic read directly.
9. **Git forensics:** HEAD/origin parity, reflog, staged-diff, stash list,
   worktree list, full untracked inventory, manifest/lockfile diff — at audit
   start and after all test runs.

All scratch material (diffs, logs, checkers) lives **outside the repository** in
session temp `…/T/opencode/p0c-remediation-audit/`. No repository content was
modified, cleaned, staged, committed, or pushed; no test or expectation was
altered; no unavailable artifact was recovered or reconstructed.

---

## 3. Repository integrity — PASS

| Check | Result |
|---|---|
| `git rev-parse HEAD` | `eb2896923ec788727975268aca0beb27cba51760` — **identical to expected checkpoint** |
| `git rev-parse origin/main` | same; `git rev-list --left-right --count HEAD...origin/main` = **0 0** |
| HEAD reflog | last entry = baseline commit `eb28969`; **no commits after it** |
| Staged changes (`git diff --cached`) | **none** (0 files) |
| `git stash list` | empty |
| `git diff --check` | clean (exit 0) |
| Manifest/lockfile diff (`rust/Cargo.toml`, `rust/Cargo.lock`, `rust/convert/Cargo.toml`) | **empty** |
| Worktrees | two pre-FPE detached worktrees (`02158ff`, `1c825c6`), directories present, unchanged |
| Deletions | none (`git status` shows no `D` entries) |
| Non-docs working-tree changes | only `rust/src/aac_enc/fpe/tests/is_ms.rs` (test-only) |

**Exact tracked modifications (2), identical before and after all audit runs:**

```text
46      0       docs/fpe/R2.1-B-owner-decision.md      (append-only P0-A D1–D3 amendment; verified pure append at EOF)
1400    11      rust/src/aac_enc/fpe/tests/is_ms.rs    (P0-B 1230/9 + N3/N4/N6 test-only additions)
```

**Untracked inventory** matches the authorized Phase 0 set exactly: five
assessment docs; six phase-0 documents (README, P0-A report, disposition
record, P0-B report, P0-C audit, remediation report); reconstruction + absence
amendment; M1/M2 provenance + bundles (66 + 78 files); R2.1-B provenance +
bundle (3 files). After this report is written, the only addition is this file.

**No production numerical change:** the only code file touched is
`rust/src/aac_enc/fpe/tests/is_ms.rs`, compiled test-only (`#[cfg(test)] mod
tests;` at `fpe/mod.rs:44-45`, verified). Q31 baselines unchanged (2/2 debug and
release); the full-lib Tier-B failure set is identical to the established
blocked set (§8). No manifest, lockfile, dependency, API, licensing, or ADR
change. No staged files, commits, pushes, or history operations occurred during
this audit.

---

## 4. Per-finding verification

### 4.1 N1 — README status — PASS WITH FINDINGS

**Required items checked against `docs/fpe/assessment/phase-0/README.md`:**

| Required record | README evidence | Verdict |
|---|---|---|
| P0-A complete | table row "**Complete** (2026-11-21); preserved unmodified"; standing status "P0-A is **complete**" | ✓ |
| P0-B complete | table row "**Complete** (2026-11-21)"; standing status "P0-B is **complete** (… uncommitted)" | ✓ |
| P0-C audit complete | table row "**Complete** (2026-11-21)"; standing status "the P0-C independent audit is **complete**" | ✓ |
| Remediation complete | standing status "N1–N7 **remediated** in `P0-C-remediation-report.md`, uncommitted" | ✓ (wording nit below) |
| Overall pending final owner disposition | "**PASS WITH FINDINGS, pending owner disposition and finalization**"; `phase-0-final-report.md` "Not started — pending owner disposition and finalization" | ✓ |
| FR-5 accepted limitation | "FR-5 remains open (outside D4 scope; **accepted limitation**, P0-C audit §9.2/N7)" | ✓ |
| FR-6 awaiting commit | "FR-6 is implemented as working-tree test pins (P0-B B1) **pending owner-authorized commit** (P0C-N7); its remaining closure action is committing the Phase 0 artifacts" | ✓ |

**Stale-status scan of the Phase 0 index and assessment docs:** no live
contradictory status remains. The historical statements in preserved documents
(P0-A report line 19 "P0-B has not been started"; disposition record §1
"D4 — NOT ISSUED"; findings-register FR-6 "Open" / OD-6 "Not committed") are
either explicitly superseded by append-only amendment D4 (which the README
links) or **still accurate** (FR-6 remains uncommitted; FR-5 remains open).
This is the established amendment-not-rewrite policy, not stale live status.
The README's own D4 scope note, standing status, and table are mutually
consistent.

**Finding (minor, non-blocking):** the remediation-report table row's status
cell reads "**Pending** (2026-11-23; prepared for owner review, uncommitted)"
while the standing status says "N1–N7 remediated". The parenthetical clarifies
that what is pending is owner review/commit, not the remediation work — but the
cell could be read as "remediation incomplete". Suggested precision correction
(owner-authorized documentation pass): "Complete (2026-11-23; uncommitted,
pending owner review)". No substantive misstatement.

### 4.2 N2 — P0-B report accuracy — PASS

**+1230/−9 accounting — independently verified via blob-hash chain:**

| State | Blob (git hash-object) | Accounting vs HEAD |
|---|---|---|
| HEAD committed `is_ms.rs` | `a38ee4ad2aed9a570d7c7fba7e485a2c44e65a0b` | — |
| P0-B state (reconstructed from audit-time diff `…/p0c-audit/is_ms.diff`, preserved from the P0-C audit) | `e22a0a82a395aeb9080ffa7a3c6f8d4b97f56dd7` | **+1230 / −9** (`git apply --numstat`) |
| Current working tree | `a937963cae78adb04390375509b9d8481cb13278` | **+1400 / −11** (`git diff --numstat`) |

The audit-time diff's index line is `a38ee4a..e22a0a8`; applying it to the HEAD
content reproduces blob `e22a0a82…` **exactly** (no patch fuzz, no rejects).
The remediation delta (P0-B → current) is **+174 / −10**, of which 8 deletions
are rewrites of P0-B-added ordering-assertion lines and 2 are the N6 doc-comment
lines. Arithmetic closes exactly: insertions 1230 − 8 + 174 = **1400**;
deletions 9 + 2 = **11**. The full diff was read line-by-line; the delta
contains **only** N3 (emission test + ordering strengthening), N4 (`unique_id`,
`unique_evidence_ids`, identifier test), and N6 (two doc-comment edits).

**All 11 deleted lines accounted for:** 2 evidence-plumbing code lines
(`for m in materials()` → enumerate; `analyze_frame` call gaining
`material_idx`) + 7 Tier-B placeholder comment lines (behavior unchanged) +
2 `b_closed` doc-comment lines (N6). Matches the corrected P0-B report cell and
the remediation report §2.

**`some=242 / none=109` — source and meaning verified:**

- **Code semantics** (`run_corpus`, `is_ms.rs:3037-3041`): per frame, the Q31-arm
  MS digest state `pair.q_snap.digest` is counted — `0 → none_frames` (MS_NONE),
  `1 → some_frames` (MS_SOME); other states (MS_ALL) are not counted in either.
  Printed by `summarize()` as `some={} none={}`. 242 + 109 = 351 of 381 frames;
  the remaining 30 frames are MS_ALL/other — consistent with NONE/SOME/ALL
  semantics.
- **Authoritative sources:** committed `R2.1-B-implementation-note.md` line 193
  "242 MS_SOME frames"; P0-A report §6.1 reproduced gate output `some=242
  none=109`; P0-C audit §7 same.
- **Reproduced in this audit:** `R21B gate01: frames=381 agree=365 findings=30
  digest_diffs=0 coding_frames=16 short=12 start=true stop=true
  unsupported_frames=1 some=242 none=109` (fpe-debug log, §8).

**No silent historical change:** the two corrected sites carry explicit
correction annotations ("previously misreported as insertions"; "corrected from
`s200 SOME`" with source cross-references). All historical P0-B results
(130/130, 498 Tier-A, 2/2, 6/6, 19/19, 185 clippy, five aggregates, digest pin)
remain as written — accurate as history. The P0-C audit's quoted original
wording ("+1239 / −9", "s200 SOME / 109 NONE") matches exactly the two sites
that were corrected; nothing else in the report differs from the audit's
description.

### 4.3 N3 — Canonical evidence emission — PASS WITH FINDINGS

**Implementation inspected** (`r21b_p0b_canonical_evidence_is_emitted`,
`is_ms.rs:3899-3958`; `canonical_string()`, `is_ms.rs:433-520`;
`p0b_fnv1a64`, `is_ms.rs:3636-3643`; `sorted_evidences`, `is_ms.rs:2461-2472`):

- Each row is `FindingEvidence::canonical_string()` — fixed field order,
  decimal integers, bracketed decimal line vectors, `|`-joined `Debug` trace
  chains, `n/a` for absent optionals; no map iteration, pointer, or float
  formatting. Seven lines per row: identity; IS operands both arms; stateless
  margins (I1/I2 dual/I3/I8 dual/I10 dual); MS operands/margins/`B_closed`/use
  + six absolute Ld deltas; spec window + four Q31 line vectors; `trace_q`;
  `trace_f`.
- Provenance: the recorder `build_frame_evidences` is called at
  `is_ms.rs:2853`, **after** the replay-equality gates
  (`validate_lockstep_replay`/`validate_ms_replay`, lines 2534-2535) and the
  routing assertions; it records production-validated state and never feeds a
  predicate. Digest findings (band −1) record explicit `None`s (path documented
  but unexercised: 0 digest diffs).
- Emission: one `R21B-CANONICAL <unique-id>` header + full canonical row per
  finding, in `sorted_evidences` order `(material_idx, frame, band, detail)`.
- Digest procedure identical to `r21b_p0b_byte_baseline_is_pinned`
  (lines 3877-3884 vs 3940-3943): canonical strings → lexicographic `sort()` →
  `join("\n")` → FNV-1a 64 (offset basis `0xcbf29ce484222325`, prime
  `0x100000001b3`). Hard-asserted `0x0544_1ee2_9087_55aa` on
  `macos/aarch64`; informational `eprintln!` elsewhere (production_baseline
  convention). The `id=` field inside the canonical string remains the
  preserved `stable_id` — the canonical serialization is byte-unchanged from
  P0-B, hence the digest is preserved.
- Ordering assertion strengthened (`is_ms.rs:3394-3404`): full-tuple
  monotonicity `key >= *p` over `(material_idx, frame, band, detail)` —
  addresses the P0C-N3 tie-break gap.

**Seven verification items — all reproduced:**

| # | Item | Result |
|---|---|---|
| 1 | All 30 canonical rows emitted | **30 headers + 30 complete rows** in every capture (parser-verified) |
| 2 | Each row contains all required fields | **7-line structure with required prefixes asserted for all 30 rows** by the auditor's parser; F-01/F-02 rows manually inspected (IS operands, margins, MS operands/bound/use/deltas, spec vectors, both traces present) |
| 3 | Unsupported evidence retained | F-01 row: `unsupported=1`, `status=DomainUnsupported`, `ms supported=0`, `q_m=-6670 f_m=121350 B=128022 use=0/1` — matches the Qwen-checker values exactly; never a pass |
| 4 | Deterministic emission | two separate process runs **byte-identical** (`diff` clean); debug vs release **byte-identical**; in-test determinism gate green ("30 evidence rows identical") |
| 5 | Re-hashing reproduces pinned digest | **auditor-written Python FNV-1a 64** over parsed rows (lexicographic sort, `join("\n")`) = `0x05441ee2908755aa` in **all four captures** (full-suite debug log, dedicated debug ×2, release) — **MATCH** |
| 6 | Documented capture command | `cargo test --locked --lib r21b_p0b_canonical_evidence_is_emitted -- --nocapture` from `rust/` — used successfully; documented in the test comment and remediation report §3 |
| 7 | Reproducible without volatile session files | corpus = in-repo procedural fixtures (`testdata.rs::fixture_samples`) + in-file probe generators (`probe_sine`/`probe_narrow`); **no external/volatile file dependency**. (The test code itself is working-tree-only pending commit — FR-6/N7, §4.7.) |

Reproduced output lines: `R21B-CANONICAL-DIGEST rows=30
fnv1a64=0x05441ee2908755aa` (debug and release); `R21B-EVIDENCE-DIGEST rows=30
fnv1a64=0x05441ee2908755aa` (baseline, unchanged).

**Durability assessment (explicit determination, per mission):**

A distinction must be drawn between (a) a **reproducible test-generated
evidence stream** and (b) a **durable checked-in evidence artifact**.

- The implemented approach is (a): a retained test emits the complete 30-row
  stream deterministically, integrity-pinned by the digest, capturable via a
  documented command. This **matches the cited G-1 `BandRow` emission model**
  (validation-roadmap §6 item 2), which is itself test-output-based: the
  committed G-1 validation report cites the retained test names and embeds
  representative row values, while the full row stream lives in test output.
  The original P0C-N3 requirement — literal full-row emission plus the ordering
  tie-break assertion — is **satisfied**.
- However, the approach is **not** (b): **no checked-in snapshot of the 30-row
  stream exists** anywhere in the repository. The R2.1-B evidence bundle
  contains only R1/R5 + SHA256SUMS; the remediation report describes the rows
  but does not embed them; inspection requires running the test (reference
  platform + toolchain for exact-hash enforcement). Moreover, the emitting test
  and all pins are **uncommitted** (working-tree-only), so even the
  reproducibility basis is not yet durable — this is the known FR-6/N7 commit
  item, and validation-roadmap §6 items 1/4 require **committed** artifacts for
  acceptance citations.

**Determination:** the emission requirement is met; the durable-artifact aspect
is **not fully met**. **Residual gap classification: non-blocking.** The
reproducibility + digest pin is arguably stronger than a static snapshot for
drift detection, and the gap is of the same durability class as the reference
BandRow model; the commit dependency is the already-recorded owner action N7.

**Smallest appropriate remediation (described, NOT implemented):** after the
owner authorizes the commit of the test code, capture the documented command's
output once on the reference platform and check it in under
`docs/evidence/fpe-r21b/` (e.g. `canonical-evidence-rows-eb28969.txt`) with a
SHA-256 pin and a short provenance note (command, HEAD, toolchain, platform,
date); optionally embed representative rows (F-01/F-02 + one probe finding) in
the Phase 0 final report, mirroring the G-1 report pattern. No code change is
needed. Owner authorization required.

### 4.4 N4 — Stable identifiers — PASS

**Implementation inspected** (`stable_id` `is_ms.rs:408-410`; `unique_id`
425-427; `unique_evidence_ids` 2482-2495; test
`r21b_p0b_stable_identifiers_are_unique_and_deterministic` 3961-4020):

| Requirement | Verification | Verdict |
|---|---|---|
| All 30 identifiers unique | test asserts `BTreeSet` size 30; auditor's parser confirms **30 distinct headers** in every capture | ✓ |
| F-01/F-02 disambiguated | `quiet-sine-stereo:f16:b13#1` (F-01, DomainUnsupported) vs `#2` (F-02, OutOfEnvelope); test pins exactly one colliding stable label with count 2 and two distinct unique ids; reproduced in captures | ✓ |
| Original labels traceable | `stable_id()` **byte-identical to P0-B** (remediation delta does not touch it); remains the canonical `id=` field and the key of all historical prints; test asserts every unique id preserves its stable label as prefix | ✓ |
| Ordinal assignment deterministic | ordinal = 1-based per-label occurrence count in `sorted_evidences` order `(material_idx, frame, band, detail)`; linear scan, no map iteration; test runs `run_corpus()` twice and compares id vectors, also vs the cached corpus; cross-process captures byte-identical | ✓ |
| Repeated corpus generation identical | in-test double-run assertion green (debug and release); `R21B-UNIQUE-IDS rows=30 unique=30` reproduced | ✓ |
| No classification/decision/numerical change | delta adds only identifier helpers/tests + comment edits; `b_closed` body identical; Q31 2/2; Tier-B set identical; canonical string unchanged (digest preserved) | ✓ |

**Documented limitations of the ordinal scheme (per mission):**

1. **Corpus-version relativity.** Ordinals are occurrence counts within a
   sorted snapshot. If a future corpus adds/removes a finding sharing a stable
   label, or changes a `detail` string (the sort tie-break), existing ordinals
   can renumber (e.g. a third finding at `quiet-sine-stereo:f16:b13` sorting
   between F-01 and F-02 would make F-02 `#3`). Unique ids are therefore stable
   **within a pinned corpus version**, not permanent historical identifiers
   across corpus changes.
2. **F-numbers not embedded.** The historical F-01…F-30 ledger labels are not
   part of the unique id; mapping to F-numbers relies on the owner-review
   ledger + `detail` + sort position. Traceability is preserved (the canonical
   `id=` keeps the stable label and `detail` carries the semantics), but `#n`
   itself carries no finding-number semantics.
3. **Mitigation in place:** any content change that could renumber ordinals
   fails the pinned digest and aggregate gates loudly (30-row population,
   single-collision pin), so silent renumbering cannot pass retained gates; a
   corpus change requires explicit owner-authorized re-pinning.
4. Cosmetic observations (non-defects): `unique_evidence_ids` is O(n²) (trivial
   at 30 rows); the `R21B-UNIQUE-IDS` print uses `uids.len()` for both fields
   (uniqueness is hard-asserted immediately before, so the print is accurate).

These limitations are inherent to the authorized design ("per-label ordinal in
sorted order") and do not defeat the original N4 requirement (uniqueness +
determinism + traceability within the acceptance corpus). **Classification:
PASS**, limitations recorded for the owner.

### 4.5 N5 — R1–R8 preservation — PASS

**Preserved bundle** `docs/evidence/fpe-r21b/ferrite-fpe-r21b-review/`
(3 files: R1, R5, SHA256SUMS):

| Artifact | SHA-256 (P0-A §3.2 pin) | Verified | Copy fidelity |
|---|---|---|---|
| R1 `is_ms.rs.orig` (99,960 B, mtime 2026-11-18 20:47 preserved) | `3e5ed6b6e43fa91c8e64bb05549b99bf2bb4b54e6e82dfc59b065a37df7f1f74` | `SHA256SUMS: OK`; full hash recomputed = pin | `cmp` vs original temp copy: **byte-identical** |
| R5 `r21b-review-checks.rs` (6,546 B, mtime 2026-11-18 21:12 preserved) | `20cd68a497031d510407dcd46a69ee608f09b4fbad630377cc5d8e40ade78591` | `SHA256SUMS: OK`; full hash recomputed = pin | `cmp` vs original temp copy: **byte-identical** |

Supplemental hash chain: R1 SHA-1 = `16459f94d7ab1ec86c3b01fa82e0829cf85b2b8a`
— matches owner review / Qwen §3.1 / DeepSeek §7.2 (recomputed in this audit).
Original temp copies remain present and untouched.

**Unavailable artifacts — independently confirmed absent, pins retained:**

| # | Artifact | Pin (retained in provenance register) | Temp inventory 2026-11-23 |
|---|---|---|---|
| R2 | `r21b_verify/ms_bound_verify.rs` | `0922e0d6…5a2a28` | `T/r21b_verify/` **empty** |
| R3 | `r21b_verify/is_margin_verify.rs` | `1e58e8f6…aa954a4` | same |
| R4 | `r21b_verify/final_math_verify.rs` | `7a3ce183…c5313` | same |
| R6 | `r21b_env_audit.log` | `7115b305…8378845b` | no `*env_audit*` match in temp |
| R7 | `r21b_env_audit2.log` | `c3298901…b5aee52f8` | same |
| R8 | `r21b_gates.log` | `5640321e…73dd4d5` | absent |

All six are explicitly marked **Unavailable** in both the remediation report §5
and `docs/evidence/fpe-r21b-provenance.md` §3, with the loss mechanism (volatile
temp cleanup after the P0-C audit, which verified all eight pins intact) and
"pin retained; not reconstructed" recorded. The bundle contains **only** the
two surviving originals + SHA256SUMS — **no unavailable artifact was
reconstructed** (verified by directory inventory). This audit did not attempt
recovery or reconstruction.

**M1/M2 bundles intact (re-verified at the preserved locations):**

| Bundle | `shasum -a 256 -c SHA256SUMS` | Files | Supplemental pins |
|---|---|---|---|
| M1 | **64/64 OK, 0 failures** | 66 | MANIFEST.md `549ad37d…` ✓, SHA256SUMS `89332c96…` ✓, M1-REPORT.md `d720a84b…` ✓, REMEDIATION.md `81e97292…` ✓ |
| M2 | **77/77 OK, 0 failures** | 78 | SHA256SUMS `785ea4b1…` ✓, m2-implementation-report.md `7b6ce78d…` ✓ |

All supplemental pins match `docs/evidence/fpe-m1-m2-provenance.md` exactly.

**Carried-forward limitation (not a remediation defect):** with R2–R4/R6–R8
lost, the 28.4M-pair scaffold verification is no longer re-runnable from
original source; the empirical leg rests on R1+R5, the identities embedded in
the reconstruction (§4–§5) and committed Qwen review §3.3, and the retained
per-run bound re-validation gates (green in this audit, `gate05`). This was
disclosed in the remediation report and matches the P0-C audit's residual-risk
assessment (P0C-N5). The provenance record's handling rules (§4) correctly
forbid reconstruction and reserve supersession to the owner.

### 4.6 N6 — B_closed citation — PASS

**Current doc comment** (`is_ms.rs:573-593`) verified:

- Cites "`docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md` (owner-accepted
  D1 replacement for the missing original R2.1-B analytical report; see P0-A
  report §3.1 and reconstruction §1)". The target document exists, is labelled
  **RECONSTRUCTED + DERIVED, owner-accepted replacement (D1)**, and prominently
  states the original was not recovered — the citation is accurate.
- The formula text block is unchanged: `B_closed = ceil((dEL+dER+dEm+dEs)/2) +
  dTL + dTR + 2*max(dTL,dTR) + 3`.
- The `+3` attribution is now exact: "1 (min_tld parity) + 2 (four-way ceil
  grouping); the `-1` in `M` cancels in the delta (see reconstruction §3.2
  Steps 1–4)" — matches the reconstruction's derivation (§3.2, verified by
  read) and replaces the previous loose "strict-`>` unit plus halving floors"
  wording.
- **Implementation body byte-identical to HEAD:** extracted `fn b_closed … }`
  from both versions and diffed — identical (`half_ceil + d_tl + d_tr +
  2*d_tl.max(d_tr) + 3`). The remediation delta touches only the two comment
  lines; no derivation or implementation change.

**Documentation residuals (non-blocking, outside the N6 code-comment target):**
the reconstruction document itself still (a) describes the *old* loose doc
comment in its §3.2 parenthetical (lines 190-192), now superseded by the N6
correction; (b) states in §6 that the scaffolds "currently reside in volatile
session temp", superseded by the N5 preservation/absence record; (c) carries
pre-remediation line cites (§4.1 `is_ms.rs:309-314`; `b_closed` is now at line
594). Item (c) is disclosed in the remediation report §7; items (a)/(b) are
additional precision residuals noted by this audit. All are amendment-pass
items for the owner; formulas and semantics are unaffected.

### 4.7 N7 — Validation — PASS

All commands re-run independently from `rust/`, `--locked`, toolchain 1.99.1
(`58cf8f9c3` / `5f94df478`), `aarch64-apple-darwin`. Logs in
`…/T/opencode/p0c-remediation-audit/logs/`. Exact results:

| # | Command | Result | Reported | Match |
|---|---|---|---|---|
| 1 | `cargo test --locked --lib 'fpe::' -- --nocapture` | **132 passed, 0 failed**, 385 filtered | 132/132 | ✓ |
| 2 | `cargo test --locked --release --lib 'fpe::'` | **132 passed, 0 failed** | 132/132 | ✓ |
| 3 | `cargo test --locked --lib production_baseline -- --nocapture` | **2 passed, 0 failed** | 2/2 | ✓ |
| 4 | `cargo test --locked --release --lib production_baseline` | **2 passed, 0 failed** | 2/2 | ✓ |
| 5 | `cargo test --locked --lib -- --skip …` (15 CI Tier-A skips, verified against `.github/workflows/ci.yml:52-66`) | **500 passed, 0 failed, 2 ignored, 15 filtered** | 500/0 | ✓ |
| 6 | `cargo test --locked --test production_api` | **6 passed, 0 failed** | 6/6 | ✓ |
| 7 | `cargo test --locked --doc -p ferrite-aac` | **19 passed, 0 failed** | 19/19 | ✓ |
| 8 | `cargo fmt -- --check` | clean (exit 0) | clean | ✓ |
| 9 | `git diff --check` (repo root) | clean (exit 0) | clean | ✓ |
| 10 | `cargo test --locked --lib` (full, no skips) | **504 passed, 11 failed, 2 ignored** | 504/11/2 | ✓ |
| 11 | `cargo test --locked --lib r21b_p0b_byte_baseline_is_pinned -- --nocapture` ×2 | both pass; `R21B-BASELINE` rows + `R21B-EVIDENCE-DIGEST` **byte-identical** (`diff` clean) | identical | ✓ |
| 12 | `cargo test --locked --lib r21b_p0b_canonical_evidence_is_emitted -- --nocapture` ×2 + release | pass; `R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x05441ee2908755aa`; cross-process and debug/release **byte-identical**; independent re-hash **MATCH** (§4.3) | preserved baseline | ✓ |
| 13 | `cargo test --locked --lib r21b_p0b_stable_identifiers -- --nocapture` (+ release) | pass; `R21B-UNIQUE-IDS rows=30 unique=30` | 30/30 | ✓ |
| 14 | `bash rust/tools/check-licensing.sh` | **OK** — NOTICE + LICENSE-FDK-AAC present, manifest declares FDK-AAC, **86** rust/src files carry TPMV notice; converter boundary intact | OK / 86 | ✓ |
| 15 | `cargo clippy --locked --lib --tests` | **185 lib-test warnings** (63 duplicates); lib 85; manifest 1 (pre-existing dual declaration); **0 mentions of `is_ms`** in the full log | 185 / zero | ✓ |
| 16 | `cargo test --locked --lib r21b_p0b` | **7 passed, 0 failed** (5 pre-existing + 2 new) | 7/7 | ✓ |

**Reproduced gate output (identical to P0-A §6.1 / P0-C §7 / remediation §6):**

```text
R21B gate01: frames=381 agree=365 findings=30 digest_diffs=0 coding_frames=16
  short=12 start=true stop=true unsupported_frames=1 some=242 none=109
R21B gate11: review-gated (P/G/F) findings=14
R21B gate05: supported-domain bands all satisfy |shift| <= B_closed; unsupported-band frames=1
R21B-EVIDENCE-DIGEST rows=30 fnv1a64=0x05441ee2908755aa
R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x05441ee2908755aa
R21B-P0B determinism: 30 evidence rows identical
R21B-UNIQUE-IDS rows=30 unique=30
R21B-UNSUPPORTED quiet-sine-stereo:f16:b13 status=DomainUnsupported ms_supported=0 peers_in_frame=2
30 R21B-FINDING rows; 30 R21B-CANONICAL rows; 56 G1-BAND rows
```

Test breakdown verified: `is_ms` **38** (36 + 2 new), `probe_envelope` **13**.
Five aggregate assertions green in debug and release (30/16/0/14/1). The 2
Tier-A ignored are the opt-in benchmarks. Full-lib growth 502 → 504 = exactly
the 2 new tests.

**Tier-B failure set (full-lib no-skip): identical to the established blocked
set — not waived, not reclassified.** The 11 failures are exactly:
`m5a_decode_check` (AAC_DEC_BIN NotPresent), `m55_streams_match_c_reference`,
`phase1_frontend_agreement_sine_f5`, `phase1_full_stream_parity`,
`phase1e_onset_f24_bands_causality`, `phase1e_sine_f5_band16_residue`,
`phase1f_multitone_f1_no_live_divergence`,
`phase1f_stepdown_f1_no_live_divergence`, `phase1g_long_flips_residue`,
`phase1i_multitone_f5_tier_b`, `phase1i_sine44_f5_tier_b` (reference validation
artifacts missing). Each fails loudly on its missing artifact. The Tier-B
placeholder `r21b_tier_b_pinned_c_parity_blocked` still prints `TIERB_SKIP …
BLOCKED` (cargo-counted as passed, **not** a substantive pass). No failure was
suppressed or reclassified.

**Minor command-precision nit (non-blocking):** remediation report §6 row 14
labels the licensing command "`bash tools/check-licensing.sh` (repo root)";
the script actually lives at `rust/tools/check-licensing.sh` (the repo-root
`tools/` directory has no such script). The check itself passes with the
correct path; only the report's path annotation is imprecise.

**No discrepancy with the remediation report on any count or outcome.**

---

## 5. Evidence and checksum verification summary

| Evidence | Verification performed | Result |
|---|---|---|
| R1 `is_ms.rs.orig` | SHA-256 vs P0-A pin; SHA-1 hash chain; `cmp` vs original temp; size/mtime | **intact, byte-identical** |
| R5 `r21b-review-checks.rs` | SHA-256 vs P0-A pin; `cmp` vs original temp; size/mtime | **intact, byte-identical** |
| R2–R4, R6–R8 | temp filesystem inventory; provenance register | **unavailable, pins retained, not reconstructed** |
| M1 bundle | `shasum -a 256 -c` 64/64; file count 66; 4 supplemental pins | **intact** |
| M2 bundle | `shasum -a 256 -c` 77/77; file count 78; 2 supplemental pins | **intact** |
| Canonical evidence digest | auditor-written Python FNV-1a 64 re-hash of 4 captured emissions | **0x05441ee2908755aa reproduced** |
| P0-B line accounting | blob-hash chain reconstruction (`a38ee4a → e22a0a8 → a937963`) | **+1230/−9 exact; delta N3/N4/N6 only** |
| `b_closed` body | extraction + diff HEAD vs working tree | **byte-identical** |
| Owner-decision amendment | full diff read | **pure append (+46/−0), historical text unchanged** |

---

## 6. Outstanding limitations and residual items

**Owner closure actions (carried forward, not remediation defects):**

1. **Commit authorization (FR-6 ≡ OD-6 ≡ A-21 ≡ P0C-N7).** The entire Phase 0
   artifact set — including the byte baseline, emission test, identifier test,
   and all evidence/provenance documents — is working-tree-only. Phase 0's exit
   criterion (committed artifacts / owner-confirmed provenance chain) is
   **pending owner action**. The remediation report §8 inventories the exact
   commit set; this audit confirms the inventory matches `git status -uall`.
2. **FR-5 accepted limitation.** The `sum_abs_x=37` Class B example pin remains
   printed-not-hard-asserted; no implementation was authorized or performed.
   Owner decision outstanding.
3. **Fresh independent M2 review + sign-off (A-02).** Authorized (D3), not
   performed; correctly not misrepresented as performed.
4. **R2–R4/R6–R8 empirical leg.** Reported-only since the temp cleanup; pins
   retained; surviving leg (R1+R5 + embedded identities + per-run gates)
   verified green. Only the owner can supersede the unavailable status.
5. **Nine conditional acceptances** remain conditional; **Tier-B C parity**
   remains blocked (not waived); **G0** remains historical/non-rerunnable
   (A-01 open; no pass-through mode implemented — verified no
   `pass_through|pass-through|passthrough` implementation in `is_ms.rs`).

**New non-blocking findings from this audit (owner disposition suggested):**

| ID | Finding | Suggested disposition |
|---|---|---|
| RA-1 | N3 durability: no durable checked-in snapshot of the 30-row canonical stream; emitting code uncommitted (intersects item 1). Emission model itself conforms to the BandRow reference. | After commit authorization, capture the documented command output once and check it in under `docs/evidence/fpe-r21b/` with SHA-256 pin + provenance note (§4.3). Optional, owner-authorized. |
| RA-2 | README remediation-report status cell reads "Pending" while standing status says "N1–N7 remediated" (§4.1). | Precision correction in the documentation pass: "Complete (uncommitted; pending owner review)". |
| RA-3 | Reconstruction document residuals: §3.2 parenthetical still describes the pre-N6 loose doc comment; §6 "scaffolds reside in volatile temp" superseded by N5 record; pre-remediation line cites (§4.1 `is_ms.rs:309-314` etc.; `b_closed` now line 594). Line cites disclosed in remediation §7; the other two noted here. | Append-only amendment / documentation pass; no code or formula change. |
| RA-4 | Remediation report §6 licensing command path annotation "(repo root) `tools/check-licensing.sh`" — actual path `rust/tools/check-licensing.sh` (§4.7). | Precision correction in the documentation pass. |
| RA-5 | N4 ordinal scheme is corpus-version-relative; F-numbers not embedded in unique ids (§4.4 limitations 1–2). Mitigated by digest/aggregate pins. | Accept as documented; revisit only if the corpus changes under a future authorized package. |

**Disclosed technical limitations (unchanged, confirmed):** band-−1 digest
path documented but unexercised (0 digest diffs); exact hashes pin on the
reference platform per the production_baseline convention; stateful margins
(I4/I6/I7, I5 carry) travel in trace events, not standalone scalars; G-1 energy
rows referenced, not duplicated; no perceptual evidence claimed.

---

## 7. Scope compliance

- No production numerical, Q31, decision-definition, envelope, `B_closed`,
  supported-domain, API, manifest, lockfile, dependency, licensing, or ADR
  change (verified: tracked diff = 2 files; test-only gating; Q31 2/2; Tier-B
  set identical; manifests empty diff).
- No G0 pass-through, R2.2, M3/M4, G-2/G-3, or float-kernel work begun.
- No conditional acceptance promoted; no Tier-B waiver; no failure suppressed
  or reclassified.
- No unavailable artifact recovered or reconstructed.
- No commit, push, stage, amend, rebase, reset, cherry-pick, clean, or
  untracked-file deletion. All audit scratch outside the repository.
- **This report is the only repository addition.**

---

## 8. Acceptance criteria assessment (remediation mission)

| # | Criterion | Classification | Basis |
|---|---|---|---|
| N1 | README status accurate; no contradictory stale index status | **PASS WITH FINDINGS** | §4.1; all seven items recorded; RA-2 wording nit |
| N2 | P0-B accounting and distribution accurate; history unchanged | **PASS** | §4.2; blob-hash chain; code semantics; annotated corrections |
| N3 | Canonical emission complete, deterministic, digest-reproducing, inspectable | **PASS WITH FINDINGS** | §4.3; items 1–7 verified; durability residual RA-1 |
| N4 | Identifiers unique, deterministic, traceable; no behaviour change | **PASS** | §4.4; limitations documented (RA-5) |
| N5 | R1/R5 byte-preserved; unavailable explicit; pins retained; M1/M2 intact | **PASS** | §4.5; checksums + cmp + inventory |
| N6 | Citation references accepted reconstruction; formula/body unchanged | **PASS** | §4.6; byte-identical body; RA-3 doc residuals |
| N7 | All validation counts reproduce; no suppression | **PASS** | §4.7; every count exact; RA-4 path nit |
| — | Repository integrity | **PASS** | §3 |

---

## 9. Overall recommendation

> ## PASS WITH FINDINGS — OWNER DISPOSITION REQUIRED

**Rationale.** All seven P0-C findings are remediated as authorized and were
independently verified by re-execution, re-hashing, and blob-level diff
analysis — not by report review. Every reported validation count reproduces
exactly; the canonical evidence digest reproduces under an auditor-written
independent hash implementation; the preserved artifacts are byte-identical to
their originals and pins; no production numerical behaviour changed; repository
integrity is preserved. **No blocking defect exists.**

The recommendation is "PASS WITH FINDINGS" rather than unconditional "PASS —
REMEDIATION ACCEPTED" because:

1. **Phase 0's exit criterion is not yet met:** all artifacts (including the
   remediation itself) are working-tree-only pending owner-authorized commit
   (FR-6/N7). The remediation cannot be "accepted" into the committed record
   until the owner authorizes the commit.
2. **N3 durability residual (RA-1):** the evidence stream is reproducible and
   integrity-pinned but not checked in as a durable artifact; the smallest
   remediation is described (§4.3) and requires owner authorization.
3. **FR-5** remains an open accepted limitation requiring owner decision.
4. **Minor documentation-precision items (RA-2/RA-3/RA-4)** warrant a surgical
   owner-authorized documentation pass.
5. Carried-forward owner items: fresh M2 review (A-02), R2–R4/R6–R8 unavailable
   status, nine conditionals, Tier-B blocked, G0 historical.

These are owner closure actions and non-blocking precision items, not technical
defects in the remediation.

**Required owner dispositions:**

1. Disposition RA-1…RA-5 (accept / schedule corrections / authorize snapshot).
2. Authorize the **commit** of the Phase 0 artifact set (remediation report §8
   inventory; suggested atomic separation: documentation corrections + report;
   test-only N3/N4/N6 implementation; N5 evidence bundle + provenance).
3. Decide FR-5.
4. Schedule the authorized fresh independent M2 review + sign-off (A-02).
5. Optionally authorize the post-commit canonical-snapshot capture (RA-1) and
   the documentation precision pass (RA-2/RA-3/RA-4).

---

## 10. Phase 0 readiness for final owner acceptance and commit

**Recommendation: Phase 0 is TECHNICALLY READY for final owner acceptance and
commit**, conditional on the owner dispositions above.

- All validation is green on the reference platform (debug and release); the
  Tier-B failure set is exactly the established artifact-gated blocked set; no
  regression of any kind was found.
- The evidence chain is intact and independently re-verified: M1/M2 bundles,
  R1/R5 originals, pins, provenance records, reconstruction, and amendments are
  consistent and correctly labelled.
- The reproducibility basis (byte baseline + aggregates + canonical emission +
  unique identifiers + determinism gates) is established, test-only, and
  digest-pinned.
- The commit set is fully inventoried and working-tree-ready; nothing in the
  audit blocks committing it. **Committing first is recommended** so that any
  subsequent work (including R2.2, when separately authorized) builds on a
  committed, pinned basis.
- Not ready / unchanged: M3/FPE-3.1 acceptance, full R2.1-B closure, Stage-1
  closure, and any FPE-default change remain blocked by outstanding evidence
  (R2.2 itself, perceptual evidence, Tier-B provisioning, conditional-acceptance
  conditions). These are not Phase 0 commit blockers.

**This audit does not authorize or begin R2.2.**

---

## 11. Stop-condition compliance

- This report is the **only** repository addition
  (`docs/fpe/assessment/phase-0/P0-C-remediation-independent-audit.md`,
  untracked).
- No source, test, fixture, manifest, lockfile, dependency, ADR, acceptance
  criterion, or numerical behaviour was modified. No fix implemented; no
  unavailable artifact reconstructed; no failure suppressed or reclassified.
- No commit, push, stage, amend, rebase, reset, cherry-pick, clean, or
  untracked-file deletion. Git history not rewritten.
- Working-tree state re-verified after all test runs and after writing this
  report: HEAD remains `eb2896923ec788727975268aca0beb27cba51760` =
  `origin/main`; tracked diff unchanged (46/0 + 1400/11); nothing staged.
- All audit scratch (diffs, logs, Python checker) resides outside the
  repository in session temp `…/T/opencode/p0c-remediation-audit/`.

**STOP — P0-C remediation independent audit complete. Awaiting project-owner
review and disposition. R2.2 and any subsequent implementation phase are not
begun.**
