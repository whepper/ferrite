# Third-party notices — `aac-convert` production dependency graph

Inventory for release review. This file records license evidence; it is not
legal advice. Versions are pinned by `rust/Cargo.lock`.

Production graph = `cargo tree --locked -p aac-convert --edges normal`
(unique crates below; build-time-only tooling such as `syn` and `autocfg`
is listed separately). The converter links all of the
following into the `aac-convert` binary except where noted.

## Licensing boundary

`aac-convert` itself is independently developed converter code, licensed
`MIT OR Apache-2.0` (`rust/convert/Cargo.toml`; complete texts in
`rust/convert/LICENSE-MIT` and `rust/convert/LICENSE-APACHE`). Depending on
`ferrite-aac` does not place the converter's own source under the FDK-AAC
license.

The `ferrite-aac` dependency is separately licensed FDK-AAC
(`rust/Cargo.toml`; complete text in `rust/LICENSE-FDK-AAC` and
`rust/NOTICE`). Distributions that include the encoder — including
`aac-convert` binaries, which link it — remain subject to the FDK-AAC
conditions for that dependency (see below). No FDK-AAC license text is
duplicated in this package; the encoder's complete text ships in the
`ferrite-aac` package.

## First-party code

| Package | Version | License field | Evidence |
|---|---|---|---|
| `ferrite-aac` (codec dependency, path `rust/`) | 0.3.2 | `FDK-AAC` | `rust/Cargo.toml`; complete text in `rust/LICENSE-FDK-AAC` and `rust/NOTICE` |
| `aac-convert` (this application) | 0.3.3 | `MIT OR Apache-2.0` | `rust/convert/Cargo.toml`; complete texts in `rust/convert/LICENSE-MIT` and `rust/convert/LICENSE-APACHE` |

The FDK-AAC SPDX identifier is `FDK-AAC`. The codec crate manifest declares
`license = "FDK-AAC"` (the standard SPDX identifier) and also sets
`license-file = "LICENSE-FDK-AAC"` so Cargo source packages carry the
complete license text. The full text is retained in the root `NOTICE`,
`rust/NOTICE`, and `rust/LICENSE-FDK-AAC`.

Ferrite is a **Third-Party Modified Version of the Fraunhofer FDK AAC Codec
Library for Android**. The modified-version notice in `rust/NOTICE` is dated
2026-10-26 and identifies the Ferrite/Rust modifications. The notice does not
claim Fraunhofer affiliation or endorsement.

The FDK-AAC license conditions binary redistribution, among other things,
on retaining the complete license text in documentation/materials; making
the complete source available free of charge; charging no copyright license
fees; not implying Fraunhofer endorsement; and providing prominent
modification notices. It grants **no patent license**; patent authorization
must be assessed separately through Via Licensing or the patent owners.

## Direct third-party dependencies (linked)

| Package | Version | License field | Bundled text |
|---|---|---|---|
| `symphonia` (+ `symphonia-bundle-flac`, `symphonia-common`, `symphonia-core`, `symphonia-metadata`; FLAC features only) | 0.6.1 | `MPL-2.0` | `LICENSE` in each crate |
| `shiguredo_mp4` | 2026.5.0 | `Apache-2.0` | `LICENSE` |
| `ebur128` (direct; R128 loudness analysis) | 0.1.10 | `MIT` | `LICENSE` |

## Transitive dependencies (linked)

| Package | Version | License field |
|---|---|---|
| `arrayvec` | 0.7.8 | `MIT OR Apache-2.0` |
| `bitflags` | 2.13.2 | `MIT OR Apache-2.0` |
| `bitflags` | 1.3.2 (via `ebur128`) | `MIT/Apache-2.0` (texts: `LICENSE-MIT`, `LICENSE-APACHE`) |
| `bytemuck` | 1.25.2 | `Zlib OR Apache-2.0 OR MIT` (texts: `LICENSE-ZLIB`, `LICENSE-APACHE`, `LICENSE-MIT`) |
| `dasp_frame` (via `ebur128`) | 0.11.0 | `MIT OR Apache-2.0` |
| `dasp_sample` (via `ebur128`) | 0.11.0 | `MIT OR Apache-2.0` |
| `either` | 1.19.0 | `MIT OR Apache-2.0` |
| `itertools` | 0.10.5 | `MIT/Apache-2.0` |
| `lazy_static` | 1.5.1 | `MIT OR Apache-2.0` |
| `log` | 0.4.34 | `MIT OR Apache-2.0` |
| `num-complex` | 0.4.6 | `MIT OR Apache-2.0` |
| `num-traits` | 0.2.19 | `MIT OR Apache-2.0` |
| `paste` (proc-macro) | 1.0.15 | `MIT OR Apache-2.0` |
| `regex-lite` | 0.1.9 | `MIT OR Apache-2.0` |
| `smallvec` | 1.16.3 | `MIT OR Apache-2.0` |
| `zerocopy` | 0.8.63 | `BSD-2-Clause OR Apache-2.0 OR MIT` |

All fields above were read from the crates' own `Cargo.toml` manifests in
the local registry; bundled texts were confirmed present where noted.

## Build-time-only dependencies (not linked into the binary)

`syn`, `quote`, `proc-macro2`, `unicode-ident`, `autocfg`,
`zerocopy-derive`, `hound` (dev/workspace scope). Recorded for completeness;
confirm with legal review whether their texts must also ship.

## Required actions before distributing a binary (for review)

1. Include the converter's MIT/Apache-2.0 license texts and, for the linked
   `ferrite-aac` dependency, the FDK-AAC license text while satisfying its
   binary-redistribution conditions (text retention, source availability,
   modification notices).
2. Satisfy MPL-2.0 obligations for the statically linked Symphonia crates
   (notices, source availability for the covered files).
3. Include Apache-2.0/MIT/BSD/Zlib attributions for `shiguredo_mp4` and the
   transitive set.
4. Resolve patent authorization for AAC encoding/decoding on the target
   platform (FDK grants none itself).
5. Do not publish crates to a registry; standalone `cargo package`/`cargo
   publish` for `aac-convert` remains unsupported (see `README.md`
   Distribution) while `ferrite-aac` is an unpublished path dependency the
   public registry cannot resolve. The converter's own license is now
   `MIT OR Apache-2.0`; the `FDK-AAC` value remains the standard SPDX
   identifier of the `ferrite-aac` dependency.
6. Distribute `aac-convert` as the workspace source bundle described in
   `README.md` Distribution: the corresponding `ferrite-aac` source plus
   `rust/LICENSE-FDK-AAC`, `rust/NOTICE`, the converter's
   `rust/convert/LICENSE-MIT` and `rust/convert/LICENSE-APACHE`, and the
   applicable third-party notices. The standalone `aac-convert` Cargo
   source archive is not the supported complete distribution artifact; do
   not redistribute that archive as though it independently contains the
   FDK-AAC licensing materials of the encoder dependency. This states
   project distribution policy and is not a legal conclusion about any
   standalone archive.

## Out of scope for this inventory

Dev-dependencies are not part of the `aac-convert` release build and are not
assessed here.
