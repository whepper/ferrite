# Ferrite

**A native Rust AAC-LC encoder and audio conversion application.**

Ferrite is an audio encoding project built around a native Rust implementation of an AAC-LC encoder derived from the Fraunhofer FDK AAC codebase. It combines the reusable **`ferrite-aac`** encoder library with **`aac-convert`**, a command-line application for converting FLAC music libraries to AAC in M4A or ADTS format.

The project is designed as a complete encoding pipeline: from PCM input and AAC-LC encoding through metadata, cover-art handling, loudness analysis, and safe single-file or parallel batch conversion.

> **Current interface:** Ferrite is currently used through its command-line application and Rust library. A desktop GUI is a possible future interface, not part of the current release.

## What Ferrite provides

### AAC-LC encoder

The `ferrite-aac` Rust library provides the AAC-LC encoding engine:

- VBR-1 through VBR-6 encoding profiles, with VBR-6 as the default.
- Mono and stereo encoding at 44.1 kHz and 48 kHz.
- Streaming PCM input and ADTS access-unit output through a native Rust API.
- No FFI layer and no production dependency on the upstream C/C++ encoder.
- Reusable as a library by other Rust applications.

See [encoder integration](rust/docs/encoder-integration.md) and the [VBR profile reference](rust/docs/vbr-profiles.md).

### Audio conversion application

`aac-convert` is the user-facing conversion application built on the Ferrite encoder. It converts FLAC files or directory trees to audio-only M4A or ADTS AAC.

Highlights:

- Single-file and recursive directory-batch conversion.
- Parallel processing with a configurable worker limit.
- Live preparation and encoding progress, including album loudness analysis and artwork discovery.
- M4A metadata mapping from FLAC Vorbis comments, preserving repeated text values.
- Embedded and external album artwork, reusing existing album-folder artwork where available.
- ReplayGain 2.0 and Apple `iTunNORM` loudness metadata.
- Gapless playback signalling through Apple `iTunSMPB`.
- Safe reruns with `--skip-existing`; existing outputs are never overwritten.
- Dry-run planning, deterministic output mapping, and machine-readable per-file results.

The conversion pipeline is implemented in Rust. FFmpeg is not required to build or run it.

See the [Ferrite user manual](rust/docs/user-manual.md) for installation and usage, and the [aac-convert reference](rust/convert/README.md) for the full CLI and metadata behavior.

## Supported format and encoder scope

| Area | Current support |
|---|---|
| Codec | AAC-LC |
| Profiles | VBR-1 through VBR-6 (VBR-6 default) |
| Sample rates | 44.1 kHz and 48 kHz |
| Channels | Mono and stereo |
| PCM input | 16-bit signed PCM |
| FLAC input | 16-bit; 24-bit accepted with truncation to 16-bit and a warning |
| Output transports | ADTS AAC and audio-only M4A |
| Conversion input | FLAC file or directory tree |

Ferrite does not currently resample, downmix, or change channel layouts. Other sample rates, channel layouts, and input bit depths are rejected. The encoder uses 1024-sample AAC frames and has an encoder delay of 1600 samples.

VBR-1 through VBR-5 follow the upstream FDK profile parameter plane; VBR-6 is an additional Ferrite profile. See the [profile documentation](rust/docs/vbr-profiles.md) for the authoritative settings and distinctions.

### Supported platforms

Validated on macOS (ARM64) and Linux (x86_64, via CI). Other platforms, including Windows, are expected to build from source but are not validated.

## Get started

Ferrite is currently distributed as source. You need Rust and Cargo **1.99.1** — the toolchain declared by the workspace (`rust-version`) and pinned in CI.

Clone the repository and build the converter:

```sh
git clone https://github.com/whepper/ferrite.git
cd ferrite/rust
cargo build --locked --release -p aac-convert
```

The executable is created at `target/release/aac-convert`.

Check the version and available options:

```sh
./target/release/aac-convert --version
./target/release/aac-convert --help
```

### Convert a track

```sh
./target/release/aac-convert "track.flac" "track.m4a"
```

M4A is selected by the `.m4a` extension. To write an ADTS AAC stream:

```sh
./target/release/aac-convert --format adts "track.flac" "track.aac"
```

### Convert a music library

```sh
./target/release/aac-convert "music/" "converted/"
```

Directory mode recursively discovers FLAC files and mirrors their directory structure in the output directory. M4A is the default batch format.

Useful options:

```sh
# Select an encoder profile
aac-convert --vbr 5 "track.flac" "track.m4a"

# Limit parallel workers
aac-convert --jobs 4 "music/" "converted/"

# Preview the conversion plan without writing output
aac-convert --dry-run "music/" "converted/"

# Safely resume a partially completed batch
aac-convert --skip-existing "music/" "converted/"

# Extract album artwork alongside the converted tracks
aac-convert --cover-art external "music/" "converted/"
```

See `aac-convert --help` and the [user manual](rust/docs/user-manual.md) for all options and behavior.

## Build and test

Run the following commands from the `rust/` Cargo workspace root:

```sh
cargo build --locked -p aac-convert
cargo test --locked -p aac-convert
cargo build --locked --features aac_enc
cargo test --locked --features aac_enc --lib
cargo fmt --all -- --check
```

CI runs the default and encoder-only configurations, encoder library tests, documentation tests, package validation, and a separate decoder-backed validation job. Formatting and Clippy are informational jobs.

See [CI configuration](.github/workflows/ci.yml) and [workspace documentation](rust/README.md).

## Architecture and relationship to FDK AAC

Ferrite is a **float-native Rust port derived from FDK AAC**, not a 1:1 or bit-exact translation. Encoder control logic, tables, and design concepts are ported and attributed in the relevant source files. DSP is re-implemented in floating point, while the Rust architecture, state representation, streaming interface, and error model are native to this project.

The upstream Fraunhofer FDK AAC C/C++ implementation is **not vendored in this repository**. It is held in a separate private reference repository, pinned by revision in `docs/reference/fdk-aac-reference-revision.txt`, and used solely as a reference oracle. It is not compiled, linked, or executed by production Rust builds.

There is no official affiliation with or endorsement by Fraunhofer.

## Production and reference tooling

- **Production:** the Cargo workspace under `rust/`. Its production dependency closure is Rust; the default build does not require FFmpeg, `rsgain`, or a C compiler.
- **Reference and research tooling:** `tools/enc-oracle/` and selected scripts under `rust/tools/` support offline comparisons and validation against the pinned reference checkout. These workflows are separate from normal builds and are manual-only unless explicitly described otherwise.
- **Independent inspection:** FFmpeg/ffprobe may be useful for examining generated files, but they are not runtime requirements of Ferrite.

## Current limitations and future work

The encoder and converter have a deliberately bounded initial scope:

- AAC-LC only; no HE-AAC, HE-AAC v2, AAC-ELD, or other object types.
- Mono/stereo at 44.1/48 kHz only; no resampling or downmixing.
- 16-bit PCM at the encoder API; 24-bit FLAC conversion truncates to 16-bit.
- No metadata-only retagging mode.
- No true-peak analysis.
- SIMD optimization is deferred.
- A desktop GUI is not yet implemented.

Perceptual equivalence claims are limited to the evidence documented in the project's quality and divergence reports. Ferrite does not claim bit-exact output equivalence with FDK AAC.

## Project documentation

Start with [`docs/README.md`](docs/README.md) for the maintained documentation index.

| Document | Purpose |
|---|---|
| [Ferrite user manual](rust/docs/user-manual.md) | Build, install, and use `aac-convert` |
| [Converter reference](rust/convert/README.md) | CLI options, conversion pipeline, metadata, artwork, and progress |
| [Encoder integration](rust/docs/encoder-integration.md) | Public Rust encoder API and integration guidance |
| [VBR profiles](rust/docs/vbr-profiles.md) | Authoritative VBR-1 through VBR-6 profile reference |
| [Encoder migration specification](docs/aac-enc-migration-spec.md) | Porting goals and float-native design |
| [Encoder API contract](docs/aac-enc-m6-api-contract.md) | Public encoder contract |
| [Provenance inventory](docs/aac-enc-provenance-inventory.md) | Technical source-comparison and attribution evidence |
| [Loudness design](docs/aac-convert-l2-loudness-integration-design.md) | Loudness analysis and tagging design |
| [Documentation index](docs/README.md) | Maintained documents and historical evidence |

## Licensing and patent notice

Ferrite-derived encoder sources retain the applicable FDK AAC license notices. The project identifies itself as a **“Third-Party Modified Version of the Fraunhofer FDK AAC Codec Library for Android.”** Modifications and Rust implementation are copyright © 2025–2026 Jeroen Steenvoorde. Ferrite is not affiliated with or endorsed by Fraunhofer.

The FDK-AAC license requires preservation of license text and prominent modified-version notices. The license grants **no patent license**. Any patent authorization required for AAC encoding or decoding must be assessed separately before use or distribution.

The repository includes the applicable license and notice materials. Review the root `NOTICE`, `rust/NOTICE`, `rust/LICENSE-FDK-AAC`, and `rust/convert/THIRD-PARTY-NOTICES.md` before redistributing source or binaries. This documentation is not legal advice, and no legal or patent clearance is claimed.

---

**Ferrite is a native Rust AAC-LC encoder and conversion application.** The CLI is the current application interface; the encoder library is the foundation for integrations and future interfaces.
