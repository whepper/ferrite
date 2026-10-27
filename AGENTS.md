# AGENTS.md — Ferrite repository guidance

Working directory: repo root. Production workspace is `rust/` (run `cd rust` first for all Cargo commands). See root `README.md` (“Build and test”, “Licensing, attribution, NOTICE guidance”) and `rust/README.md` for crate layout. CI is defined in `.github/workflows/ci.yml`.

## Licensing and Third-Party Notices

This section is an engineering and repository-maintenance rule, not a legal opinion. It preserves the existing licensing arrangement; it does not grant clearance. Open questions — how newly authored files relate to the FDK-AAC terms, how the license’s replacement-term requirement applies to each artifact, and what patent authorization any distribution needs — remain open until appropriately reviewed.

### Existing upstream-derived files

- Do not remove, shorten, rewrite, or reformat existing complete FDK-AAC license blocks (the `Software License for The Fraunhofer FDK AAC Codec Library for Android` text in per-file headers).
- Preserve upstream copyright notices and other provenance information verbatim.
- Preserve the existing prominent Ferrite modified-version notice and modification date (`2026-10-26`) wherever it appears.
- Do not perform bulk license-header rewrites without explicit approval. The existing per-file headers may be stricter than the minimum requirement; preserve them. In particular, do not touch the history of the 262-file licensing commit (`e3aeb7f`); do not amend or rewrite it.
- Do not change existing copyright years merely to make them consistent without establishing the correct provenance.
- Do not alter embedded upstream license text merely to change the product designation without explicit approval and qualified review.

### New Ferrite-authored files

- Do not copy the entire upstream FDK-AAC license block into every newly authored file.
- Existing header conventions in this repo (check before adding a header):
  - Upstream-derived `.rs` files use a block notice (`/* Ferrite: this file is part of a Third-Party Modified Version … See rust/NOTICE for the full license text. */`) followed by the complete license block.
  - Recently authored codec files (for example `rust/src/aac_enc/encoder.rs`) use a module-doc notice (`//! Ferrite: part of a Third-Party Modified Version … Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.`). They do not currently carry an `SPDX-License-Identifier` line.
  - Match the surrounding file style (`//!` for module docs, `//` for plain files, `/* … */` for block headers) rather than mixing styles.
- Candidate notice for new Ferrite-authored Rust codec files (adapt comment markers to the file):

```rust
// SPDX-License-Identifier: FDK-AAC
//
// Part of Ferrite, a Third-Party Modified Version of the
// Fraunhofer FDK AAC Codec Library for Android.
// See rust/NOTICE and rust/LICENSE-FDK-AAC for the complete license text,
// modification notice, and date.
```

- `SPDX-License-Identifier: FDK-AAC` in that candidate records the project’s declared packaging identifier (the same `FDK-AAC` value used in the Cargo manifests). It is not independent proof that the file is correctly licensed, and a package-level `license = "FDK-AAC"` declaration must not be treated as a verified per-file licensing basis.
- Do not apply FDK-AAC notices automatically to standalone utilities, application code, research tools, or unrelated files (for example `rust/convert/src/*`, `rust/build.rs`, `tools/*`, test fixtures). Establish the file’s relationship to the codec first: only files that are part of or integrated with the modified codec carry the codec notice. When in doubt, leave the file unmarked and ask rather than inventing licensing permissions.

### Package-level distribution

- Preserve the complete license text in `rust/NOTICE` and `rust/LICENSE-FDK-AAC`. Do not remove or consolidate them. The root `NOTICE` preserves the upstream text; retain it as well.
- `ferrite-aac` (`rust/Cargo.toml`) declares `license = "FDK-AAC"` with `license-file = "LICENSE-FDK-AAC"` so the complete text ships in the Cargo source package; `NOTICE` is also included. Verify with `cargo package --locked -p ferrite-aac --list` from `rust/`.
- `aac-convert` (`rust/convert/Cargo.toml`) declares `license = "FDK-AAC"` and ships `THIRD-PARTY-NOTICES.md`, which documents the codec dependency and the FDK-AAC terms. Its own Cargo package does not currently bundle `LICENSE-FDK-AAC`/`NOTICE`; do not assume its package metadata alone settles its distribution obligations. Report any proposed manifest change for approval instead of changing it unilaterally.
- Keep the root `README.md` distribution guidance consistent with the actual packaging arrangement: binary distributions must include the appropriate notices and license text in documentation/materials and make the complete corresponding source available free of charge, charge no copyright license fees, and grant no patent license. Repository presence alone does not guarantee compliance for any future distribution method.

### Naming and attribution

- When identifying the modified codec, use the required designation: “Third-Party Modified Version of the Fraunhofer FDK AAC Codec Library for Android” (short form “Ferrite” only with that qualification nearby, as in `rust/NOTICE`).
- Do not imply Fraunhofer affiliation, sponsorship, or endorsement.
- Modifications and Rust implementation: copyright © 2025–2026 Jeroen Steenvoorde; modification notice date: 2026-10-26 (per `rust/NOTICE`).

### Validation

- Run `rust/tools/check-licensing.sh` (offline, read-only; never rewrites files) and `git diff --check` before sending changes for review.
- The check verifies package-level files exist, the prominent modified-version notice is intact, and every `rust/src/**/*.rs` file carries a Ferrite/Third-Party-Modified-Version notice (accepting either the full-block or concise form). It deliberately does not enforce the new-file template on `rust/convert/`, `tools/`, build scripts, or tests, and a text match is only a hygiene signal — not proof of legal compliance.
