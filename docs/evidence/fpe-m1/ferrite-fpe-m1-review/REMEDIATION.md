# FPE M1 — Review Finding Remediation Record

Date: 2026-11-17. Base HEAD: `b188415e3c16dbf00706984858c6a568c9b705ff`
(unchanged; working-tree changes only). This record documents the remediation
of the two minor findings from the independent M1 source review. **M2 was not
started; no float kernels; no Q31 calculation/behaviour change; no
dependency/manifest change; no commits/push/amend/rebase/reset/cherry-pick/stash.**

## Finding 1 — Modification dates in source headers (FIXED)

The five files M1 modified carried the baseline Ferrite modification-notice
date `2026-10-26`. Because these files were actually modified during M1
(2026-11-16), their per-file modification notices were updated to the M1
modification date, consistent with the FDK-AAC requirement to state "the date
of any change."

**Exact correction (date string only, one occurrence per file):**

| File | Header form | Change |
|---|---|---|
| `rust/src/aac_enc.rs` | block `/* … */` | `notice date: 2026-10-26.` → `notice date: 2026-11-16.` |
| `rust/src/aac_enc/config.rs` | block `/* … */` | `notice date: 2026-10-26.` → `notice date: 2026-11-16.` |
| `rust/src/aac_enc/psy.rs` | block `/* … */` | `notice date: 2026-10-26.` → `notice date: 2026-11-16.` |
| `rust/src/aac_enc/testdata.rs` | block `/* … */` | `notice date: 2026-10-26.` → `notice date: 2026-11-16.` |
| `rust/src/aac_enc/stream_driver.rs` | concise `//!` | `Modification notice date: 2026-10-26.` → `Modification notice date: 2026-11-16.` |

Each file follows its existing per-file header convention; only the date
changed. Verified: each file now contains `2026-11-16` exactly once and
`2026-10-26` zero times; the header diff is date-only (no other header text
changed).

**Preserved (unchanged):** the licensing decision (FDK-AAC), SPDX identifiers,
`rust/NOTICE` / `rust/LICENSE-FDK-AAC` / root `NOTICE` (the package-level
prominent notice keeps its baseline date `2026-10-26`), the upstream Fraunhofer
copyright (`© Copyright 2025 …`), the Ferrite attribution copyright year range
(`© 2025–2026 Jeroen Steenvoorde`), and every unmodified source file (all retain
`2026-10-26`). The new `fpe/` files already carried their own implementation
date `2026-11-16` and were not touched.

**Guidance note:** repository `AGENTS.md` and `docs/fpe/12` §4.2 advise
preserving the baseline notice date and avoiding bulk header rewrites. This
remediation is an explicit, owner-issued, targeted date correction on exactly
the five files M1 modified — not a bulk rewrite — and records the actual M1
modification date as the licence requires. No licensing file, SPDX identifier,
NOTICE, or attribution text was altered beyond the necessary per-file date.
`check-licensing.sh` continues to pass (it detects the "Third-Party Modified
Version" designation, which is unchanged; the package-level NOTICE date check
is unaffected).

## Finding 2 — Test-count reporting (FIXED, documentation only)

Clarified the M1 test inventory in `report/M1-REPORT.md` (§5 table + new §5a)
and `validation/COMMANDS.md` (§2):
- The `fpe` substring filter selects **20** tests (4 constants + 12
  differential + 3 seam + 1 config
  `with_psy_engine_builder_selects_fpe_and_preserves_other_fields`).
- `config::tests::psy_engine_defaults_to_q31_in_every_constructor` does not
  contain `fpe` in its name, so it is **not** selected by the filter; it runs
  in the broader Tier-A library suite.
- **Total new M1 tests = 21** (20 + 1).

The earlier draft of `COMMANDS.md` had mis-stated the filter as matching "2
config" tests; this is corrected to 1 config test in the filter (2 config
tests total). **No test behaviour was changed.** Verified empirically:
`cargo test --locked --lib fpe -- --list` → 20 tests;
`cargo test --locked --lib -- --list | grep psy_engine_defaults_to_q31…` →
present; the `fpe` filter list contains 0 occurrences of that test name.

## Validation (all green; see `validation/post-change/remediation-validation.log`)

| Command (from `rust/`) | Result |
|---|---|
| `cargo fmt --all -- --check` | clean (exit 0) |
| `cargo test --locked --lib fpe` | 20 passed, 0 failed, 381 filtered |
| `cargo test --locked --lib -- <15 CI skips>` (full Tier-A) | 384 passed, 0 failed, 2 ignored, 15 filtered |
| `cargo test --release --locked --lib production_baseline -- --nocapture` | 2 passed, 0 failed (Q31 hashes unchanged) |
| `cargo test --locked --lib psy_engine_defaults_to_q31_in_every_constructor` | 1 passed |
| `git diff --check` | clean (exit 0) |
| `bash tools/check-licensing.sh` | OK (79 src files carry the notice; converter boundary intact) |

**Q31 baseline hashes confirmed unchanged** (production_baseline release pins
pass on the aarch64-apple-darwin reference platform). The header date change is
comment-only and cannot affect codegen; the passing baseline hashes confirm no
behavioural drift.

## Scope confirmation

- M2 **not** started (no orchestration mirror, no individual FPE state fields,
  no float kernels, no boundary quantization).
- No Q31 calculation/behaviour change (comment-only header edits).
- No dependency or manifest change (`rust/Cargo.toml`, `rust/Cargo.lock`,
  `rust/convert/Cargo.toml` untouched).
- No unrelated changes; only the five header dates (repo) plus this
  documentation remediation (bundle).
- No commits/push/amend/rebase/reset/cherry-pick/stash; HEAD `b188415` unchanged.

## Bundle refresh

Because Finding 1 changed the five source files, this bundle was refreshed so
its diffs, modified-file copies, manifest, and checksums reflect the remediated
working tree (the review artifact must match the actual implementation). The
`context/` excerpts are unaffected (they start below the file headers). The
`new-files/` copies are unchanged (FPE files already dated 2026-11-16).