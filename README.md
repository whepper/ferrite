# Ferrite — Native audio engineering in Rust

Ferrite is the working identity for this project's native Rust audio work:
a **native Rust AAC-LC encoder derived from the FDK AAC codebase**,
accompanied by the **`aac-convert`** conversion application. (Ferrite is a
working name; trademark, package-name, and domain availability have not
been cleared. The selected encoder package/library name is
`ferrite-aac` / `ferrite_aac`;
`aac-convert` is unchanged. The repository rename to `whepper/ferrite` is
complete.)

## What it is

- `rust/` — Cargo workspace with the native Rust implementation:
  - `ferrite-aac` (`ferrite_aac`) — AAC-LC encoder library (**VBR-5
    profile**) plus shared DSP/transport modules.
  - `aac-convert` — FLAC-to-AAC application: single-file and batch
    conversion to audio-only M4A or ADTS, with core Vorbis-comment tags,
    cover art, Apple `iTunSMPB` gapless signalling, and native ReplayGain
    2.0 + `iTunNORM` loudness metadata (fixed −18.0 LUFS reference).
  - The encoder has no FFI surface; there is no decoder crate in this
    repository.
- The upstream Fraunhofer FDK AAC C/C++ implementation is **not
  vendored here**: it lives only in the private reference repository
  (`whepper/fdk-aac-reference`, pinned revision in
  `docs/reference/fdk-aac-reference-revision.txt`) and is used **solely as
  a reference oracle** — it is not compiled, linked, or executed by any
  production Rust build (see "Production vs reference tooling" below).

See `rust/README.md` for the crate layout and `rust/convert/README.md`
for `aac-convert` usage, options, and metadata behavior.

Repository layout: `rust/` is the production workspace; `docs/` is indexed
in `docs/README.md`; `tools/` holds oracle/benchmark provenance tooling;
reference validation builds and runs the external pinned checkout
(`rust/tools/run-reference-validation.sh`, manual workflow only); CI is
defined in `.github/workflows/ci.yml`.

## Relationship to FDK AAC

The Rust encoder is a **float-native port derived from FDK AAC, not a 1:1
or bit-exact port**: control logic, tables, and design concepts are ported
and cited per file (`cpp:` references), while the DSP is re-implemented
floating-point and the architecture (chunk loop, state representation,
error taxonomy) is its own. The migration specification states this
explicitly (`docs/aac-enc-migration-spec.md` §5: mathematics and decisions
preserved, not bit patterns). There is no official affiliation with or
endorsement by Fraunhofer. Per-file FDK license headers are retained as
provenance (see "Licensing" below).

## Supported encoder scope

- AAC-LC, fixed VBR-5 profile (afterburner-on behavior), 1024-sample
  frames, encoder delay of 1600 samples.
- 44.1 kHz and 48 kHz, mono and stereo, 16-bit PCM input. (`aac-convert`
  accepts 24-bit FLAC by truncating to 16-bit with a per-file warning;
  other rates, channel counts, and bit depths are rejected.)
- ADTS transport internally; `aac-convert` muxes audio-only M4A.
- No target configurability, no metadata-only retag, no true-peak analysis
  in v1 (per `docs/aac-convert-l0-loudness-decisions.md`).

## Build and test

All commands run from `rust/` (the Cargo workspace root); from the
repository root, run `cd rust` first. Minimum
supported Rust version: **1.99.1**.

```sh
cargo build --locked -p aac-convert
cargo test --locked -p aac-convert
cargo build --locked --features aac_enc
cargo test --locked --features aac_enc --lib
cargo fmt --all -- --check
```

CI (`.github/workflows/ci.yml`) runs these plus doctests, `cargo package`,
and informational clippy checks — no C/C++ compilation and no external
audio tools required. Reference validation
(`rust/tools/run-reference-validation.sh`, manual workflow
`reference-validation.yml`) builds and runs the external pinned reference
checkout, and is never required for normal Rust validation or CI.

## Production vs reference tooling

- **Production:** everything `cargo` builds in `rust/`. The production
  dependency closure is pure Rust (notably no FFmpeg, no `rsgain`, no C
  compiler on the default build path).
- **Reference/oracle (manual use only, never production):** the external
  pinned checkout (`whepper/fdk-aac-reference`) plus the in-repo
  `tools/enc-oracle/` provenance scripts and the `rust/tools/` validation
  driver — used to validate Rust behavior against the original
  implementation. System `ffmpeg`, `ffprobe`, and `mutagen` are used only
  offline (fixture generation; `#[ignore]`d advisory tests skip cleanly
  without them).

## Licensing, attribution, NOTICE guidance

- FDK-derived code (the Rust sources carrying FDK license headers; the
  upstream C tree now lives only in the private reference repository) is
  subject to the **Fraunhofer FDK AAC Codec Library for Android** license
  terms, including the patent-licensing notice pointing to Via Licensing.
  **Do not remove FDK license headers or NOTICE files.**
- The root `NOTICE` preserves the upstream license text. `rust/NOTICE`
  carries the complete license text plus the prominent modified-version
  notice dated 2026-10-26; retain both.
- The codec crate manifest declares the standard SPDX identifier
  `license = "FDK-AAC"` and `license-file = "LICENSE-FDK-AAC"`. The
  dedicated file contains the complete FDK-AAC license for Cargo source
  packages. The converter manifest also uses
  `FDK-AAC`. Distribution bundles containing binaries must include the
  complete corresponding source and the license/notice documentation.
- **Modified-version identification:** Ferrite is a “Third-Party Modified
  Version of the Fraunhofer FDK AAC Codec Library for Android.” The Rust
  implementation/modifications are copyright © 2025–2026 Jeroen Steenvoorde.
  Ferrite is not affiliated with or endorsed by Fraunhofer.
- Separately, small C reference/tooling helpers in the external reference
  checkout carry their own regimes (`aac-enc.c` is Apache-2.0
  (Martin Storsjo); the SHA-1 helpers are BSD (Paul E. Jones)). This does
  not alter the FDK-derived licensing obligations above.
- The FDK-AAC license grants no patent license. Any applicable AAC patent
  authorization must be assessed separately before distribution.
- This documentation is not a legal opinion; the inventory and distribution
  model require review before a public or commercial release.

## Limitations and deferred work (not implemented)

- Encoder scope is fixed as stated above; additional VBR modes, SIMD
  optimization, and true-peak analysis are deferred to their own
  milestones — not started here.
- Loudness tagging follows the finalized L2 design; the L3 closeout audit
  (`docs/archive/aac-convert-l3-closeout-audit.md`, PASS WITH FINDINGS) lists two
  test-coverage follow-ups (absolute calibration test, partial-rerun
  identity test) and one ruling awaiting owner acknowledgment
  (mixed-layout album handling).
- No perceptual-equivalence claims beyond the evidence in the M5
  quality/divergence reports.

## Key documents

Start from `docs/README.md` for the maintained-vs-historical index.

- `docs/aac-enc-migration-spec.md` — encoder migration specification and
  float-native thesis (§5).
- `docs/aac-enc-m6-api-contract.md` — production encoder API contract.
- `rust/convert/README.md` — `aac-convert` usage and metadata reference.
- `docs/aac-convert-l2-loudness-integration-design.md` — finalized
  loudness design; `docs/archive/aac-convert-l3-*.md` — evidence and audit trail.
- `docs/archive/repository-modernization-audit.md` — modernization roadmap and
  naming evaluation (source of the Ferrite working identity).
- `docs/research/d021/` — read-only divergence evidence archive.
