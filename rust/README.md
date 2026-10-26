# Ferrite — Rust production workspace

This directory is the production Cargo workspace. Product identity,
provenance, and the production-vs-reference boundary are defined in the
root `../README.md`.

## Folder Structure

### `src`

This directory contains the Rust source files for the `ferrite-aac`
(`ferrite_aac`) encoder library, covering the AAC-LC encoder (`aac_enc`
feature) plus shared DSP/transport modules.

### `convert`

This directory contains the `aac-convert` application, supporting
single-file and directory-batch FLAC-to-M4A (or ADTS) conversion around
the existing encoder. See `convert/README.md` for supported input, usage,
and metadata behavior.

## Testing and build

All commands run from this directory (the Cargo workspace root).

See the root `../README.md` (“Build and test”) and
`../.github/workflows/ci.yml` for the normal validation workflow, and
`convert/README.md` for `aac-convert` usage and tests.

### Prerequisites

* Install Cargo:
  * `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
* Minimum supported Rust version: 1.99.1
  * `rustup install 1.99.1`
  * `rustup default 1.99.1`

### Example: conversion application

Build the package: `cargo build --package aac-convert`

To run the converter, use: `target/debug/aac-convert --help`

### How to Build the Documentation

* Generate documentation with: `cargo doc`

The C reference/oracle workflow is separate, manual-only, and never part
of normal Rust builds or CI; see the root `../README.md`
(“Production vs reference tooling”) and `../tools/enc-oracle/README.md`
for that workflow.
