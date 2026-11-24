# Ferrite FPE M1 — Independent Review Bundle

Base revision: `b188415e3c16dbf00706984858c6a568c9b705ff` (branch `main`).
The M1 changes are **working-tree changes only** — nothing was committed,
pushed, amended, rebased, reset, or stashed. This bundle captures the complete
M1 implementation for independent review, plus the validation evidence.

Scope reviewed: FPE M1 (architecture scaffolding + selection seam +
Q31-delegating pass-through skeleton + in-binary Q31↔FPE differential harness
+ edge-case fixtures), per `docs/fpe/09-implementation-roadmap.md` M1 and the
delegated task. **No floating-point psychoacoustic kernels exist in M1**; the
Q31 engine remains authoritative and the default.

## Bundle layout

| Path | Contents |
|---|---|
| `REMEDIATION.md` | **Review-finding remediation record** (Finding 1 header dates, Finding 2 test counts) — read first for the post-review state |
| `diff/m1-complete.diff` | **Single reviewable diff**: all tracked modifications + all new M1 files (git-format, `git apply`-able against the base HEAD) |
| `diff/m1-tracked.diff` | Tracked modifications only (5 files, `git diff b188415 -- …`) |
| `diff/new-*.diff` | Per-new-file diffs (`git diff --no-index /dev/null …`) |
| `new-files/…` | Complete contents of every new M1 source file, at repo-relative paths (9 `fpe/` files + `make_fpe_fixtures.py`) |
| `modified-files/…` | Complete current contents of the 5 modified tracked files (for whole-file context) |
| `context/…` | Verbatim, line-ranged excerpts of the surrounding source the reviewer needs (frontend dispatch, `psy_tns_info`, `PsyEngineKind` + config methods, state allocation/init, the full Q31 `process_pair_reuse`, state accessors, `PreEchoState`, `TnsChannel`). Each file carries a provenance header (source path + line range). |
| `validation/COMMANDS.md` | Exact commands + results for every suite |
| `validation/pre-change/` | Baseline logs captured on the pristine tree BEFORE M1 edits |
| `validation/post-change/` | Post-implementation logs, incl. `fresh-verification.log` (consolidated re-run at bundle creation, every command echoed with exit code) |
| `validation/fixture-stability.txt` | 3-run byte-stability SHA-256 table for the 6 new fixtures |
| `validation/fixture-crosscheck.md` | Python↔Rust resynthesis cross-check: method, exact command, verbatim output, and the temporary harness code (removed after verification) |
| `validation/fixtures-run1/` | The 6 generated fixture WAVs (run1; byte-identical across 3 runs) |
| `validation/git-status.txt` | Git state snapshot (HEAD, branch, porcelain status) |
| `report/M1-REPORT.md` | The implementation report (files, architecture, pass-through, harness, results, blockers, planning-compliance notes) |
| `MANIFEST.md` | Every bundle file with category and repo status (modified / newly added) |
| `SHA256SUMS` | SHA-256 of every bundle file (verify with `shasum -a 256 -c SHA256SUMS`) |

Excluded as pre-existing unrelated untracked material: `docs/fpe/` (the
planning package itself), `.DS_Store`, `docs/.DS_Store`.

## Suggested review order

1. `report/M1-REPORT.md` — what was implemented and why (§11 lists the four
   resolved planning tensions, notably the engine-aware TNS-info source).
2. `diff/m1-complete.diff` — the focused diff. Check: additive-only Q31 seam;
   Q31 dispatch arm byte-identical; default Q31; no float kernels; no
   rounding/rescaling; no manifest/licensing change.
3. `new-files/rust/src/aac_enc/fpe/` — the isolated module tree. `orchestrate.rs`
   is the whole delegation; `energy/thresholds/preecho/output.rs` are doc-only
   skeletons; `constants.rs` constants are consumed by no kernel yet.
4. `context/psy-process_pair_reuse.rs.txt` — the delegation target (unchanged
   Q31 engine), to verify the pass-through inherits D-022, element-global
   shifts, TNS sync, and stale-on-short MS-Ld exactly.
5. `new-files/rust/src/aac_enc/fpe/tests/` — the differential harness:
   bit-identical comparators (every output field + mutated spectrum + all
   persistent state, per frame), multi-frame coverage, frame accounting.
6. `validation/` — regression evidence (baseline hashes, bench pins, Tier-A
   counts) and the explicit blocked-Tier-B accounting.

## Key review gates (from docs/fpe/)

- **R-01 / G3 / G9:** Q31 intact, default unchanged, byte-identical baseline.
- **G0 precursor (M2 gate enforced early):** FPE≡Q31 bit-identical complete
  output AND all frame-surviving state, per frame, multi-frame.
- **R-19:** no accidental float round-trip/rounding in the pass-through.
- **R-21 / 07 §7.1:** no state divergence hidden by single-frame tests; no
  silent skips (exact frame accounting + coverage assertions).
- **07 §3.1:** private state inspected via `#[cfg(test)]` helpers only; no
  production API added for testing.
- **R-12 / A-3:** reference-dependent work explicitly blocked, never claimed
  resolved or passing.

## Integrity

Verify the archive after extraction:

```sh
cd ferrite-fpe-m1-review
shasum -a 256 -c SHA256SUMS
```

All source excerpts and copies were taken from the working tree at base HEAD
`b188415` + M1 changes; no repository file was modified to produce this bundle.