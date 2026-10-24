# Ferrite — Phase 1 Implementation Report

- Date: 2026-10-24 (UTC).
- Baseline: `main` @ `7ebf208d40d78adcd265e3736e93f8a8aa93e8d1`
  (L3 implementation present but uncommitted, exactly as audited).
- Authority: `docs/archive/repository-modernization-audit.md`,
  `docs/aac-convert-l0-loudness-decisions.md`.
- Scope: working identity (Ferrite), root entry-point README, provenance/
  licensing documentation, minimal packaging metadata. No behavior change.

## 1. Baseline and final Git status

- Baseline: branch `main`, HEAD `7ebf208...`, 0 staged, 9 tracked-modified
  (all L3), 25 untracked (14 planning docs + modernization audit + L3/L2/
  prep/closeout reports + L3 sources/fixtures).
- Final: identical plus Phase 1 changes — 0 staged, 0 commits, 0 pushes.
  New tracked modifications: `rust/Cargo.toml`, `rust/ffi/Cargo.toml`,
  `rust/framework/Cargo.toml` (see §3). New files: `README.md` (root),
  this report.
- The pre-existing `M rust/Cargo.lock` entry is the L3 dependency addition,
  not Phase 1: every Phase 1 cargo invocation used `--locked` (which fails
  rather than rewrite the lockfile) and no dependency changed, so the
  lockfile is untouched by this phase.

## 2. Files changed and rationale

| File | Change | Rationale |
|---|---|---|
| `README.md` (root, **new**) | Authoritative entry point: Ferrite identity + tagline, capabilities and fixed encoder scope, FDK relationship (§5 wording), architecture map, `aac-convert` link, verified build/test commands, production-vs-oracle boundary, licensing/NOTICE guidance, limitations, doc links | Required §3; no root README existed |
| `rust/Cargo.toml` | `description` → "Native Rust AAC codec library derived from Fraunhofer FDK AAC (Ferrite)"; removed dead `obsolete_bindings_enabled = []` | Accurate identity (was product-titled "Fraunhofer FDK AAC Codec Library"); feature had zero references (verified: only its definition + the audit mention) |
| `rust/convert/Cargo.toml` | `description` → "…converter for the native Ferrite Rust AAC encoder" | Was "native FDK AAC encoder" (implies the encoder *is* FDK) |
| `rust/ffi/Cargo.toml` | `description` → "Native Rust AAC decoder FFI derived from Fraunhofer FDK AAC (Ferrite)" | Same product-title fix, decoder-scoped |
| `rust/framework/Cargo.toml` | `description` → "Example decoder application for the native Rust Ferrite AAC library" | Same fix, example-scoped |
| `docs/archive/ferrite-phase-1-report.md` (**new**) | This report | Required §8 |

No packages, crates, binaries, CLI commands, or versions renamed or
bumped. No dependency changed. No encoder, test, fixture, C/C++, or
unrelated-doc file touched.

## 3. Final project description and provenance wording

- Name: **Ferrite**. Tagline: **Native audio engineering in Rust.**
  Working identity only (availability not cleared; nothing registered).
- Technical description: "A native Rust AAC-LC encoder derived from the
  FDK AAC codebase, accompanied by the `aac-convert` conversion
  application." README states: native Rust implementation; architecture
  and floating-point processing differ from FDK; no bit-exact equivalence,
  no 1:1-port claim, no perceptual claim beyond M5 evidence, no
  Fraunhofer affiliation.
- Provenance: FDK derivation explicit everywhere it matters (manifest
  descriptions, README relationship section, retained per-file headers).

## 4. Licensing and NOTICE decisions

- Inspected: root `NOTICE` (© 1995–2018, upstream C tree), `rust/NOTICE`
  (© 2025, carried with the Rust port), per-file FDK headers in
  `rust/src` (2025 variant), `MODULE_LICENSE_FRAUNHOFER`,
  `METADATA`, `OWNERS`, manifest `license = "FDK-AAC"` markers.
- Decision: **both NOTICE files retained, unmodified.** Root NOTICE covers
  the upstream C/C++ tree; `rust/NOTICE` covers the Rust port whose source
  files bear that header. They differ only in the copyright-year line
  (6-line diff); README documents which applies where. No merge/delete —
  no clear basis, and the headers reference the texts as-is.
- Open question (deferred, not guessed): manifests declare the custom
  `license = "FDK-AAC"` identifier. Adding `license-file` was **not**
  done: (a) the correct authoritative text per package needs confirmation
  (root vs `rust/NOTICE`); (b) `license-file` must live inside each package
  directory, so a shared pointer is not straightforward; (c) changing
  license metadata must not misrepresent the arrangement. Recorded for
  legal/owner review; `cargo package` works as before.
- Unchanged: keywords (kept `fdk` for discoverability; descriptions carry
  the accurate identity — documented choice, easy to revisit).

## 5. Packaging metadata changes and evidence

- `obsolete_bindings_enabled` removal: verified dead — sole definition at
  `rust/Cargo.toml:30`; zero references in manifests, sources, build
  files, CI (CI uses only `aac_enc`/`aac_dec`), and docs (only the audit's
  dead-code note). Pre-1.0 removal; no downstream compatibility surface in
  repo. `cargo build --workspace --locked --offline` succeeds after
  removal.
- Description edits: text-only manifest metadata; no version, dependency,
  feature (besides the dead one), target, or lockfile impact.
- Lockfile: untouched (see §1).

## 6. Validation commands and actual results

All from `rust/` (workspace root), matching CI's working directory:

- `cargo build --workspace --locked --offline` → `Finished dev profile`
  (success).
- `cargo test -p aac-convert --locked --offline` → **200 passed, 0 failed**
  (68+47+10+30+11+15+19; 4 pre-existing ignores), same as the audited
  headline. (Full-workspace tests not run: out of scope, unchanged code.)
- `cargo fmt -p aac-convert -- --check` → clean. (`cargo fmt --all`
  shows only the three pre-existing `aac`-crate test-file diffs from a
  newer rustfmt — untouched, out of scope.)
- `cargo tree -p aac-convert --edges normal --locked --offline` → pure-Rust
  closure; zero matches for ffmpeg/rsgain/cc/cmake: production constraints
  hold after the manifest edits.
- README commands verified against `.github/workflows/ci.yml` (same
  build/test invocations, same `working-directory: rust`, same 1.99.1
  toolchain) and `rust/README.md` (MSRV, `aac_decoder` package/bin names).
  Scope statements verified against `config.rs` constants (via prior
  audit evidence: FRAME_LENGTH 1024, delay 1600, VBR-5, 44.1/48 kHz,
  mono/stereo, ADTS) and `convert/README.md` (formats, loudness −18 LUFS,
  cover art, gapless).
- Final diff inspection: 12 tracked files = 9 pre-existing L3 + 3 Phase 1
  manifests (1–2 lines each); no encoder, test, fixture, lockfile, or
  unrelated-doc changes.

## 7. Deferred cleanup

- `license-file` pointer (see §4).
- `keywords = ["fdk"]` retention (see §4).
- `spike-m4a` relocation/marking, edition-2024 consideration, root-NOTICE
  explanation paragraph (all STRUCT items) — left for a docs/structure
  pass; explicitly not bundled here.
- L3 F1–F5 remain open per the closeout audit; untouched by this phase.

## 8. Stop point

Phase 1 complete. Nothing staged, committed, or pushed. Awaiting review
before any Phase 2 proposal (naming verification/adoption, deeper
cleanup). Do not start VBR, encoder, or refactoring work.
