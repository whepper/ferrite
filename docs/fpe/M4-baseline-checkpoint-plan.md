# FPE M4 Baseline Checkpoint Plan — Accepted M3/R2.2 State

**Date:** 28 November 2026.
**Status:** plan — **the baseline is NOT yet established.** This document
specifies the checkpoint procedure required by owner decision D3
([`assessment/M4.0-owner-decision.md`](assessment/M4.0-owner-decision.md) §3).
**No Git operation described here has been executed.** Every operation in
§4 requires explicit owner approval before execution (D3); none was
performed while preparing this plan. No production source, test, fixture,
evidence bundle, schema, pin, ADR, threshold, tolerance or envelope was
modified; all pre-existing working-tree changes are preserved.

---

## 1. Current-state investigation (Task B record, 28 November 2026)

### 1.1 Branch, HEAD, index

| Item | Verified state |
|---|---|
| Branch | `main` |
| HEAD | `189e6a79b78a84dcc5be88eaca5ffaba24097acc` (= `189e6a7 feat(fpe): add R2.2 capture and replay infrastructure`) — **matches the historical checkpoint** of the M3 records |
| Index | empty (no staged changes) |
| Stash | empty |
| Remote | `origin` = `https://github.com/wheeper/ferrite.git`; `origin/main` = `a6bab67` → **`main` is ahead 1 of `origin/main`**: even the last local commit (`189e6a7`) has no off-machine copy |
| Ignored paths | `rust/target/`, `__pycache__/` only; all relevant content is tracked or untracked-visible |

### 1.2 Exact working-tree inventory

**19 modified files** (all part of the accepted M3/R2.2 set):

```
docs/fpe/07-validation-strategy.md
docs/fpe/assessment/R2.2-downstream-decision-safety-design.md
docs/fpe/assessment/findings-register.md
docs/fpe/assessment/implementation-roadmap.md
docs/fpe/assessment/validation-roadmap.md
rust/src/aac_enc/fpe/energy.rs
rust/src/aac_enc/fpe/mod.rs
rust/src/aac_enc/fpe/orchestrate.rs
rust/src/aac_enc/fpe/output.rs
rust/src/aac_enc/fpe/tests/decision.rs
rust/src/aac_enc/fpe/tests/is_ms.rs
rust/src/aac_enc/fpe/tests/mod.rs
rust/src/aac_enc/fpe/tests/r22_capture.rs
rust/src/aac_enc/intensity.rs
rust/src/aac_enc/rate.rs
rust/src/aac_enc/scf.rs
rust/src/aac_enc/stream_driver.rs
rust/src/aac_enc/testdata.rs
rust/src/aac_enc/threshold_vbr.rs
```

**32 untracked entries:**

- 2 evidence-bundle directories: `docs/evidence/fpe-r22/` (36 pinned files,
  ~78 MB) and `docs/evidence/fpe-r22-ac9/` (5 pinned files, ~120 MB)
  — 43 files total, committed-evidence is the established repo pattern
  (`2e21630`, `745e0f1`, `d0a85ca`).
- 21 assessment records under `docs/fpe/assessment/` (M3 exit/acceptance,
  the R2.2 group/AC records, the component owner decisions) plus
  `docs/fpe/FPE-3.3-A-4-experiment-report.md`.
- 7 test files under `rust/src/aac_enc/fpe/tests/`:
  `ac9.rs`, `ac9_artifact.rs`, `evidence.rs`, `precision.rs`,
  `s4_step3.rs`, `trace.rs`, `xarch_log2.rs`.
- 1 M4.0 governance document: `docs/fpe/M4-architecture-and-implementation-plan.md`
  (this task's set adds the decision record, this plan, and the four
  register/roadmap dated notes — see §3.3).

### 1.3 Classification

1. **Accepted M3/R2.2 baseline content:** all 19 modified files plus 31 of
   the 32 untracked entries. The inventory matches the recorded
   acceptance-time inventories ([`assessment/M3-owner-acceptance.md`](assessment/M3-owner-acceptance.md)
   §6: "19 modified + 30 untracked entries (the pre-existing M3/R2.2 set
   plus the M3 exit assessment)", extended by the M4.0 documentation set).
2. **Unrelated or pre-existing work outside that set:** **none found.**
   The only working-tree content beyond the accepted M3/R2.2 set is the
   M4.0 governance documentation (blueprint, this task's records, the
   append-only register notes) — which is M4.0 content by definition, not
   unrelated legacy work.
3. **Single-copy risk:** the accepted M3/R2.2 state exists **only** in this
   working directory. See §1.4.

### 1.4 Reproducibility assessment

**The current tree is NOT reproducible from existing commits and artifacts:**

- None of the 19 modified-file contents and none of the 32 untracked
  entries exist in any commit; HEAD `189e6a7` predates all of them.
- The evidence bundles are deterministically regenerable from source
  (asserted by the E2/E3 double-encode + hash-pin tests), but the source
  they are regenerated **from** is itself uncommitted — regeneration is
  circular and does not provide an independent reproduction path.
- `main` is ahead 1 of `origin/main`; no off-machine copy of any of the
  M3/R2.2 work exists.

**Conclusion:** establishing the baseline requires Git operations (scoped
commits). A checkpoint without commits would leave the accepted M3 state a
single unversioned copy.

### 1.5 Re-verified validation state (current tree, 28 November 2026)

All checks were re-run read-only during this investigation; the current
tree matches the accepted M3 state exactly:

| Check | Result |
|---|---|
| `cargo test -p ferrite-aac --lib production_baseline` (from `rust/`) | **2/2 passed** |
| `cargo test -p ferrite-aac --lib fpe::` | **289 passed / 7 failed / 0 ignored** — the 7 are exactly the preserved pins: `r21b_p0b_{aggregates_are_pinned, byte_baseline_is_pinned, canonical_evidence_is_emitted, per_finding_evidence_is_complete_and_traceable, stable_identifiers_are_unique_and_deterministic, unsupported_frame_is_explicit}` + `probe_envelope::g1_probe_corpus_aggregate` |
| `cargo test -p ferrite-aac --lib` (full) | **671 passed / 18 failed / 2 ignored** — the 18 are exactly the documented set: the 7 pins + the 11 Tier-B/environmental blockers (`m5a_decode_check`, `m55_streams_match_c_reference`, `phase1_frontend_agreement_sine_f5`, `phase1_full_stream_parity`, `phase1e_onset_f24_bands_causality`, `phase1e_sine_f5_band16_residue`, `phase1f_multitone_f1_no_live_divergence`, `phase1f_stepdown_f1_no_live_divergence`, `phase1g_long_flips_residue`, `phase1i_multitone_f5_tier_b`, `phase1i_sine44_f5_tier_b`) |
| Evidence bundle integrity | `docs/evidence/fpe-r22`: **36/36 OK**; `docs/evidence/fpe-r22-ac9`: **5/5 OK** (`shasum -a 256 -c SHA256SUMS`) |
| `bash rust/tools/check-licensing.sh` | OK (NOTICE + LICENSE present; 94 `rust/src` notices; converter clean) |
| `git diff --check` | Clean |
| Default engine | `PsyEngineKind::default() == Q31` (passing in the suite) |
| `cargo fmt --check` | 98 hunks in 8 files — the documented pre-existing drift (`fpe/tests/xarch_log2.rs` 44, `fpe/tests/decision.rs` 31, `fpe/tests/ac9.rs` 13, `fpe/tests/s4_step3.rs` 6, `fpe/tests/ac9_artifact.rs` 1, `fpe/tests/mod.rs` 1, `threshold_vbr.rs` 1, `scf.rs` 1) — see §4.4 |

## 2. Required baseline content and validation results

The baseline checkpoint, once established, must contain and reproduce:

1. **Source revision:** branch tip descending from `189e6a79b78a84dcc5be88eaca5ffaba24097acc`
   with exactly the working-tree content of §1.2 (byte-identical, per the
   §5 manifest gate) — no unrelated content, nothing dropped.
2. **Accepted M3 content:** the M3 implementation (FPE-3.1/3.2 kernels,
   orchestration, boundary), the R2.2/AC-9/Group C/D/E decision-safety
   test infrastructure, the A-4 precision experiment tests, the complete
   assessment record set (M3 exit/acceptance, R2.2 records, component
   owner decisions), and both evidence bundles with their SHA-256 pins
   intact.
3. **Required validation results** (the baseline is only reproducible if a
   fresh tree at the checkpoint reproduces these exactly): every row of
   §1.5 — production baseline 2/2; `fpe::` 289/7 with the exact seven pin
   names; full suite 671/18/2 with the exact eighteen names; evidence
   36/36 + 5/5; licensing OK; `git diff --check` clean; default Q31.
   The known formatting drift (§1.5 last row) is reproduced as-is at the
   checkpoint (see §4.4 for its disposition).
4. **Evidence and artifact integrity:** the two bundle `SHA256SUMS` files
   and `MANIFEST.txt` verify from the committed tree; no bundle file is
   regenerated, re-ordered, or edited; the frozen E1 schema v2, MDEF v2,
   the seven preserved pins, the 187 unjudged S4 flips and every pinned
   aggregate are **untouched** (D4).

## 3. Unrelated-change preservation

- The **only** content beyond the accepted M3/R2.2 set is the M4.0
  governance documentation: `docs/fpe/M4-architecture-and-implementation-plan.md`,
  `docs/fpe/assessment/M4.0-owner-decision.md`, this plan, and the four
  append-only register/roadmap notes (findings-register §23 + A-13 status
  line, implementation-roadmap and validation-roadmap dated
  clarifications, and the M4 blueprint's dated addendum). It is preserved
  by committing it as its own governance commit (§4.2, C3) — never mixed
  into the M3/R2.2 commits.
- Three files (`findings-register.md`, `implementation-roadmap.md`,
  `validation-roadmap.md`) carry **both** M3-era reconciliation updates
  and M4.0 dated additions. The preferred procedure stages these files
  hunk-by-hunk (`git add -p`): the M3-era hunks in C2, the M4.0 hunks in
  C3. The M4.0 hunks are cleanly separable (status-line edit + EOF
  appends, distinct from the earlier §21/§22-era changes). **Fallback**
  (if hunk separation proves unreliable during execution): commit the
  three files once in C2 and record in both commit messages that the
  M4.0 dated sections are included; the M4.0 record file itself still
  lands in C3. Either way nothing is dropped or silently incorporated.
- Build artifacts (`rust/target/`, `__pycache__/`) are gitignored and
  remain untouched; they are not part of the baseline.

## 4. Proposed Git operations (NOT executed; each requires explicit owner approval)

### 4.1 Principles

- `main` stays untouched at `189e6a7` (already ahead 1 of `origin/main`);
  all checkpoint commits land on a **dedicated branch**, so the entire
  procedure is reversible by deleting the branch while `main` and the
  working tree remain intact.
- Every commit is enumerated with exact paths before execution; nothing
  is staged that is not listed here (no `git add -A`).
- The working tree is never reset, cleaned or stashed at any point; the
  commits are additive capture of existing content.

### 4.2 Proposed sequence

```
# 0. Preconditions (before any mutation)
#    - owner approval of this plan's operations (D3)
#    - §1.5 validation re-verified on the current tree (already done, §1.5)
#    - pre-execution manifest written OUTSIDE the repository:
#      shasum -a 256 of every file in the §1.2 inventory (plus this task's
#      M4.0 files) -> /tmp/m4-baseline-manifest-<date>.sha256

# 1. Create the checkpoint branch (working tree and index are preserved
#    by construction; main stays at 189e6a7)
git switch -c fpe/m3-baseline

# 2. C1 — accepted M3/R2.2 implementation and test infrastructure
#    (compile- and suite-coherent as one unit: the modified production
#    files, the modified test wiring, and the new test modules interlock)
git add rust/src/aac_enc/fpe/energy.rs rust/src/aac_enc/fpe/mod.rs \
        rust/src/aac_enc/fpe/orchestrate.rs rust/src/aac_enc/fpe/output.rs \
        rust/src/aac_enc/fpe/tests/decision.rs rust/src/aac_enc/fpe/tests/is_ms.rs \
        rust/src/aac_enc/fpe/tests/mod.rs rust/src/aac_enc/fpe/tests/r22_capture.rs \
        rust/src/aac_enc/fpe/tests/ac9.rs rust/src/aac_enc/fpe/tests/ac9_artifact.rs \
        rust/src/aac_enc/fpe/tests/evidence.rs rust/src/aac_enc/fpe/tests/precision.rs \
        rust/src/aac_enc/fpe/tests/s4_step3.rs rust/src/aac_enc/fpe/tests/trace.rs \
        rust/src/aac_enc/fpe/tests/xarch_log2.rs \
        rust/src/aac_enc/intensity.rs rust/src/aac_enc/rate.rs \
        rust/src/aac_enc/scf.rs rust/src/aac_enc/stream_driver.rs \
        rust/src/aac_enc/testdata.rs rust/src/aac_enc/threshold_vbr.rs
git commit   # "feat(fpe): M3 float energy kernels and R2.2 decision-safety
             #  infrastructure" + the standard Mistral Vibe trailer
# GATE C1: cargo test -p ferrite-aac --lib  == 671/18/2 (documented set);
#          git status shows only the docs/evidence + M4.0 entries remaining

# 3. C2 — accepted M3/R2.2 assessment records and evidence bundles
git add docs/fpe/07-validation-strategy.md \
        docs/fpe/assessment/R2.2-downstream-decision-safety-design.md \
        docs/fpe/FPE-3.3-A-4-experiment-report.md \
        docs/fpe/assessment/M3-exit-assessment.md \
        docs/fpe/assessment/M3-owner-acceptance.md \
        docs/fpe/assessment/R2.2-AC-10-independent-review.md \
        docs/fpe/assessment/R2.2-AC-7-divergence-characterization.md \
        docs/fpe/assessment/R2.2-AC-9-D-R2-owner-decision.md \
        docs/fpe/assessment/R2.2-AC-9-M3-scope-matrix.md \
        docs/fpe/assessment/R2.2-AC-9-margin-derivation-report.md \
        docs/fpe/assessment/R2.2-AC-9-owner-acceptance.md \
        docs/fpe/assessment/R2.2-AC-9-partial-owner-acceptance.md \
        docs/fpe/assessment/R2.2-F20-F22-F26-numerical-owner-acceptance.md \
        docs/fpe/assessment/R2.2-findings-register.md \
        docs/fpe/assessment/R2.2-group-C-owner-acceptance.md \
        docs/fpe/assessment/R2.2-group-D-implementation-report.md \
        docs/fpe/assessment/R2.2-group-D-owner-acceptance.md \
        docs/fpe/assessment/R2.2-group-D-owner-decision.md \
        docs/fpe/assessment/R2.2-group-E1-owner-decision.md \
        docs/fpe/assessment/R2.2-group-E2-E3-owner-decision.md \
        docs/fpe/assessment/a4-owner-decision.md \
        docs/fpe/assessment/a5-n2-owner-decision.md \
        docs/fpe/assessment/f6-owner-decision.md \
        docs/fpe/assessment/fpe-3.2-owner-decision.md \
        docs/evidence/fpe-r22/ docs/evidence/fpe-r22-ac9/
# plus, via `git add -p`, the M3-era hunks of the three shared files:
#   docs/fpe/assessment/findings-register.md
#   docs/fpe/assessment/implementation-roadmap.md
#   docs/fpe/assessment/validation-roadmap.md
#   (fallback per §3 if hunk staging is unreliable: add the three files whole
#    and note the included M4.0 sections in both commit messages)
git commit   # "docs(fpe): M3/R2.2 acceptance records and evidence bundles"
# GATE C2: shasum -c on both committed bundles == 36/36 and 5/5;
#          no evidence file modified (manifest comparison)

# 4. C3 — M4.0 governance documentation
git add docs/fpe/M4-architecture-and-implementation-plan.md \
        docs/fpe/assessment/M4.0-owner-decision.md \
        docs/fpe/M4-baseline-checkpoint-plan.md
# plus the remaining M4.0 hunks of the three shared files (or the whole
# files under the §3 fallback)
git commit   # "docs(fpe): M4.0 owner decisions and baseline checkpoint plan"

# 5. GATE FINAL (§5): working tree clean; manifest re-verified; fresh
#    worktree reproduction; then tag:
git tag -a fpe-m3-baseline -m "FPE accepted M3/R2.2 baseline checkpoint
  (M4.0 owner decision D3; docs/fpe/assessment/M4.0-owner-decision.md)"

# 6. OPTIONAL — off-machine redundancy (separate authorization, see §4.3):
git push -u origin fpe/m3-baseline
#    and/or, to sync the pre-existing local-only commit:
git push origin main            # advances origin/main a6bab67 -> 189e6a7
```

### 4.3 Optional operations (each requires its own explicit approval)

| Operation | Purpose | Blast radius |
|---|---|---|
| `git push -u origin fpe/m3-baseline` | the **only** way to obtain off-machine redundancy for the accepted M3 state (currently a single copy); strongly recommended given §1.4 | creates a new remote branch; no existing remote branch is modified |
| `git push origin main` | syncs the already-local commit `189e6a7` to `origin/main` (main is already ahead 1; the checkpoint does not add commits to main) | advances `origin/main` by the one existing commit; no force |
| §4.4 formatting cleanup | clears the documented `cargo fmt --check` drift for CI | see below |

### 4.4 Formatting cleanup (optional, separate authorization)

The 98-hunk `cargo fmt --check` drift (§1.5) is entirely in the
uncommitted test instrumentation plus two single hunks in
`threshold_vbr.rs`/`scf.rs`. **Ordering rule:** the checkpoint commits
(C1–C3) must capture the tree **as validated** (§1.5), so the cleanup is
proposed **after** the checkpoint as its own commit:

```
git switch -c fpe/m4-format-cleanup   # or continue on the checkpoint branch
cd rust && cargo fmt                  # the 8 listed files only
git add <the 8 files> && git commit   # "style: cargo fmt on M3/R2.2 test instrumentation"
# GATE: full re-validation (671/18/2 documented set) + updated manifest
```

A cleanup **before** the checkpoint would make the committed baseline
differ from the validated state and is not proposed.

### 4.5 Isolated worktrees (considered, not required for the checkpoint)

An isolated worktree cannot serve as the checkpoint itself: the content to
checkpoint exists only in this working directory, and a fresh worktree
from `189e6a7` would not contain it. After the checkpoint exists,
M4.1 implementation may proceed on the checkpoint branch — either in
place or in a dedicated worktree of `fpe/m3-baseline` (recommended if the
owner wants the main checkout left quiet during M4.1). Direct commits on
`main` (skipping the branch) is a simpler alternative the owner may
prefer; it forfeits the "main untouched until reviewed" property but
matches the repo's trunk-based history. The branch-first sequence above
is the recommendation.

## 5. Objective criteria for declaring the baseline reproducible

The baseline is established **only** when all of the following hold, each
verified and recorded:

1. **Content capture:** `git status --porcelain` on the checkpoint branch is
   **empty** (every working-tree byte of §1.2 plus the M4.0 set is
   committed; nothing lost, nothing extra).
2. **Byte-exactness:** the pre-execution SHA-256 manifest re-verifies
   against the committed tree — every captured file's hash is unchanged
   (proves the commits captured the validated content without staging
   mistakes or fmt interference).
3. **Evidence integrity:** `shasum -a 256 -c SHA256SUMS` passes **36/36**
   (`docs/evidence/fpe-r22`) and **5/5** (`docs/evidence/fpe-r22-ac9`)
   from the committed tree; no bundle file was regenerated or modified
   (manifest comparison).
4. **Validation reproduction (fresh tree):** a fresh `git worktree add` of
   `fpe/m3-baseline` (independent of this working directory) builds and
   reproduces: production baseline **2/2**; `fpe::` suite **289/7** with
   the exact seven pin names; full suite **671/18/2** with the exact
   eighteen names; `check-licensing.sh` OK; default engine Q31.
5. **Hygiene:** `git diff --check` clean at the branch tip.
6. **Checkpoint identity:** the annotated tag `fpe-m3-baseline` exists at
   the branch tip, and the branch descends from `189e6a7` with exactly the
   enumerated commits (C1–C3, plus optional C4).
7. **Record:** the completion is recorded as a dated addendum to the M4.0
   owner decision (baseline established, gate results, tag/commit ids),
   and M4.1's D3 block is lifted by that record.

If off-machine redundancy is approved (§4.3), criterion 7 additionally
records the remote branch state, and a `git clone` from `origin` is the
final reproduction proof.

## 6. Rollback and recovery procedure

- **Before any push:** every step is additive and `main` is untouched. If
  a gate fails or a commit is defective, **stop**; the recovery options
  are (a) fix-forward: `git revert` or corrective commits on the branch;
  (b) branch reset: `git reset --keep <sha>` on the checkpoint branch only
  (destructive only to commits that themselves hold the only copy — never
  do this without first re-verifying the working-tree manifest); (c)
  delete the branch entirely (`git branch -D fpe/m3-baseline`) — safe
  **only** while the working tree still holds the original content, which
  the manifest confirms. The working tree itself is never reset, cleaned
  or stashed by the checkpoint procedure.
- **After a push:** fix-forward only (revert/corrective commits); history
  rewrite on a pushed branch requires its own explicit authorization.
- **Working-tree disturbance at any point:** the SHA-256 manifest detects
  it; content recovery is possible from the checkpoint commits. Any
  working-tree restore operation (`git checkout --`, `git restore`,
  `git reset --hard`) is destructive to uncommitted state and requires
  explicit owner approval before use; while the pre-checkpoint manifest
  and the current inventory agree, the working tree remains the primary
  copy and no restore is needed.
- **Failure of the fresh-worktree reproduction (§5.4):** the baseline is
  **not** declared established; investigate (environment vs content),
  report, and re-run. No partial pass is claimed.

## 7. Status

**The baseline is NOT established.** The required Git operations are
specified in §4 and await explicit owner approval (D3). M4.1 implementation
remains blocked until §5's criteria are met and recorded.
