# Third-party notices — `aac-convert` production dependency graph

Inventory for release review. This file records license evidence; it is not
legal advice. Versions are pinned by `rust/Cargo.lock`.

Production graph = `cargo tree --locked -p aac-convert --edges normal`
(unique crates below; build-time-only tooling such as `rustversion`, `syn`,
and `autocfg` is listed separately). The converter links all of the
following into the `aac-convert` binary except where noted.

## First-party code

| Package | Version | License field | Evidence |
|---|---|---|---|
| `ferrite-aac` (codec, path dependency `rust/`) | 0.1.0 | `FDK-AAC` (custom, non-SPDX) | `rust/Cargo.toml`; full Fraunhofer text in file headers and `rust/NOTICE` |
| `aac-convert` (this application) | 0.1.0 | `FDK-AAC` (custom, non-SPDX) | `rust/convert/Cargo.toml` |

The FDK-AAC license text (see `NOTICE` at the repository root and
`rust/NOTICE`) conditions binary redistribution, among other things, on:
retaining the complete license text in documentation/materials; making the
complete source available free of charge; charging no copyright license fees;
no Fraunhofer endorsement; prominent modification notices in modified
versions. It grants **no patent license**; patent authorization must come
through Via Licensing or the patent owners. Whether desktop distribution is
already covered in any given case is an open question for review.

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

`rustversion`, `syn`, `quote`, `proc-macro2`, `unicode-ident`, `autocfg`,
`zerocopy-derive`, `hound` (dev/workspace scope). Recorded for completeness;
confirm with legal review whether their texts must also ship.

## Required actions before distributing a binary (for review)

1. Include the FDK-AAC license text and satisfy its binary-redistribution
   conditions (text retention, source availability, modification notices).
2. Satisfy MPL-2.0 obligations for the statically linked Symphonia crates
   (notices, source availability for the covered files).
3. Include Apache-2.0/MIT/BSD/Zlib attributions for `shiguredo_mp4` and the
   transitive set.
4. Resolve patent authorization for AAC encoding/decoding on the target
   platform (FDK grants none itself).
5. Clarify the non-SPDX `FDK-AAC` identifier before any registry publication.

## Out of scope for this inventory

Sibling workspace members (`framework`, `ffi`) and dev-dependencies are not
part of the `aac-convert` release build and are not assessed here.
