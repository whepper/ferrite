# Ferrite FPE M2 — Independent-Review Bundle

Prepared for independent architecture + numerical review of the completed
**FPE M2 (Explicit Orchestration Mirror)** implementation. This bundle is
**read-only with respect to the repository**: no source, index, or git state was
modified to produce it (no commit/push/amend/rebase/reset/cherry-pick/stash).

- Repository HEAD: `b188415e3c16dbf00706984858c6a568c9b705ff` (branch `main`).
- Working tree: HEAD + uncommitted **M1** (seam/harness/fixtures/planning) + **M2**
  (explicit orchestration mirror, FPE-owned state, invariant guards, M2 tests).
- Host/toolchain: `aarch64-apple-darwin` (Apple Silicon — the `production_baseline`
  exact-hash reference platform), rustc/cargo 1.99.1.
- Milestone authority: `docs/fpe/09` M2 (FPE-2.1…2.5), `04` §4/§5/§6, `05` §7/§8,
  `07` §3.1/§5, ADR-0001/0003/0004/0005.

## Headline result

M2 replaces M1's whole-function delegation with an **explicit FPE processing
sequence** over **independently owned state/scratch**, calling the **existing Q31
kernels for every value calculation**. FPE is **bit-identical to Q31** on the
complete observable output **and all frame-surviving persistent state, after every
frame** of multi-frame sequences. **No floating-point psychoacoustic kernel,
fraction conversion, rounding, normalization, or rescaling** was introduced. Q31
remains the default; production hashes/byte totals unchanged.

## Contents map

| Path | Content |
|---|---|
| `00-README.md` | This file. |
| `MANIFEST.txt` | Every bundled file + byte size. |
| `SHA256SUMS` | SHA-256 of every bundled file (except itself). |
| `git/head-and-branch.txt` | HEAD commit + branch. |
| `git/status-porcelain.txt`, `git/status-full.txt` | Working-tree status. |
| `git/diff-tracked.patch` | `git diff HEAD` — the 5 modified tracked files. |
| `git/diff-new-files.patch` | Add-patches for the 32 new files (read-only `--no-index`). |
| `git/diff-complete.patch` | **Complete working-tree diff vs HEAD** (tracked + new; 37 entries). |
| `source/fpe/**` | Complete current FPE modules + tests (10 files). |
| `source/modified-rust/**` | Complete current contents of the 5 modified Rust files. |
| `q31-reference/psy-orchestration-and-helpers.rs` | Verbatim extract of `psy::process_pair_reuse` + all 10 helpers the mirror calls (line ranges noted). |
| `q31-reference/{energy,thresholds,q31,tns,sfb}.rs` | Complete Q31 kernel dependencies (read-only). |
| `docs/stage-correspondence.md` | **Complete M2 orchestration stage-correspondence** (FPE ↔ `psy.rs` ↔ upstream ↔ kernel). |
| `docs/guidance/AGENTS.md` | Project guidance — **modification-date policy** + licensing rules. |
| `docs/guidance/12-licensing-provenance.md` | `docs/fpe/12` — licensing/provenance (incl. §4.2 date policy). |
| `docs/fpe-planning/**` | Full FPE planning package (binding authority; 21 docs incl. 5 ADRs + M0). |
| `tests/list-*.txt` | Exact `cargo test -- --list` outputs (lib, FPE, ignored debug/release). |
| `validation/*.log` | Complete validation logs (see `validation/commands.txt`). |
| `report/m2-implementation-report.md` | The M2 implementation report. |
| `report/m1-context.md` | What was pre-existing M1 vs the M2-only delta. |

## How to review

1. Read `report/m2-implementation-report.md` + `report/m1-context.md`.
2. Read `docs/stage-correspondence.md`, then diff `source/fpe/orchestrate.rs`
   against `q31-reference/psy-orchestration-and-helpers.rs` (`process_pair_reuse`).
   Confirm stage order, conditionals, element-global decisions, state transitions,
   and scratch resets match; confirm every value calculation calls a Q31 kernel.
3. Confirm the only `psy.rs` change is the 5 additive `pub(crate)` bumps
   (`git/diff-tracked.patch`); bodies byte-identical to HEAD.
4. Confirm FPE state/scratch are independently owned (`source/fpe/mod.rs`) and no
   mutable Q31 state is shared.
5. Check the invariant guards (`source/fpe/output.rs`) and D-022 entry asserts
   (`source/fpe/orchestrate.rs`); confirm they are `debug_assert!` (no release
   behaviour change) except the release-active D-022 `assert!`.
6. Review the differential + M2 stage tests (`source/fpe/tests/`) and the logs
   (`validation/`): per-frame output+state bit-identity, Tier-A accounting,
   production baseline hashes, Clippy.
7. Verify integrity: `sha256sum -c SHA256SUMS` (from the bundle root).

## Validation summary (full logs in validation/)

- FPE differential + M2 tests: **24/24** (debug **and** release).
- Tier-A (CI skip list): **388 pass / 0 fail / 6 ignored** (release);
  **392 / 0 / 2 ignored** (debug); 15 filtered.
- Production baseline **2/2** (exact FNV-1a + byte totals, 19 fixtures + VBR-6 pin);
  production API **6/6**; doctests **19/19**; FPE seam end-to-end **3/3**.
- `cargo fmt --check` clean; Clippy **0 errors**; default + no-default-features
  builds OK; `check-licensing.sh` OK; `git diff --check` clean; package includes
  NOTICE/LICENSE-FDK-AAC + all FPE files.
- **BLOCKED (not passing):** 11 Tier-B tests fail loudly on missing C-reference /
  decoder artifacts (`PHASE1_ORACLE_DIR`/`PHASE1_C_DIR`/`M55_C_DIR`/`AAC_DEC_BIN`);
  identical to the M0/M1 blocked set. **A-3 remains unresolved** (pinned `f14ad40a`
  not re-provisioned). See `validation/full-lib-release-no-skip.log`.

## Findings for the owner / reviewer

1. **Modification-date policy (M1-inherited):** M1's uncommitted diff bumped the
   header modification-notice date `2026-10-26 → 2026-11-16` in 5 existing files,
   conflicting with `AGENTS.md` and `docs/fpe/12 §4.2` (preserve `2026-10-26`; do not
   rewrite existing headers). M2 preserved that state and did not change
   attribution. Recommend restoring `2026-10-26` (or explicit owner approval) before
   commit. See `docs/guidance/`.
2. **Clippy (informational, non-gating):** 2 notes in `fpe/orchestrate.rs:420/655`
   are identical to pre-existing Q31 baseline patterns (`psy.rs:865/1099`); the
   mirror was kept token-faithful to the binding Q31 implementation.
3. **Tier-B / A-3:** C↔FPE ground truth remains blocked on private-reference
   re-provisioning (R-12); the Q31↔FPE in-binary harness is the active M2 gate.

## Scope exclusions

Pre-existing unrelated untracked files (macOS `.DS_Store` artifacts) are excluded
from the diff and the bundle. No repository file was modified; all bundle content
was copied/read into a temp staging area and archived outside the repository.
