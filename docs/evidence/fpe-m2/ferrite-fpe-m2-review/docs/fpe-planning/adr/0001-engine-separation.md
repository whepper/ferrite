# ADR-0001 — Engine separation and dispatch

Status: Proposed. Date: 2026-11-16.

## Context

Ferrite's Q31 psychoacoustic engine is a set of flat modules
(`psy.rs`/`energy.rs`/`thresholds.rs`/`q31.rs`). FPE must coexist with it,
keep it intact and default, be independently selectable, and avoid duplicating
downstream functionality. There is no existing algorithm-selection mechanism
(no Cargo features; the only dispatch precedent is the arch-based quantizer
SIMD backend, `quant.rs:396-489`).

## Decision

1. Add FPE as a **new self-contained module tree** `rust/src/aac_enc/fpe/`
   beside the flat Q31 modules. Do **not** reorganize Q31 into a `psy/`
   directory (that would refactor the Q31 engine — forbidden).
2. Select via a small internal enum `PsyEngineKind { Q31, Fpe }` (default Q31)
   at a **single dispatch point** in `StreamEncoder::frontend`
   (`stream_driver.rs:847/857`). Both arms return the identical output tuple.
3. **No trait hierarchy, no generic numeric tower, no dyn dispatch, no Cargo
   feature, no plugin registry.** Two implementations share data types and
   representation-lane helpers, not arithmetic code.
4. Runtime enum (not compile-time feature) so both engines live in one binary
   for three-way differential testing.
5. Encoder-owned reusable `FpeScratch`; no hot-path kernel allocations; no
   global mutable state.

## Consequences

- Q31 diff is additive-only (seam + visibility bumps); byte-identical default
  output is verifiable.
- FPE is reviewable in isolation under `fpe/`.
- Same-binary Q31↔FPE comparison is cheap and C-independent.
- Slight code duplication of orchestration structure (acceptable: the two
  engines' arithmetic differs; sharing a framework would couple them).
- **Licensing unchanged**: FPE lives in the `ferrite-aac` crate under the
  **existing FDK-AAC licence** (no separate licence, no SPDX/NOTICE/licence-file
  change); new `fpe/` files carry the TPMV notice per `12` §0/§4.

## Alternatives rejected

- **Trait `PsyEngine` + dyn/generic dispatch**: premature abstraction for two
  impls; adds complexity and monomorphization/dyn overhead; no third engine
  planned.
- **Cargo feature**: compile-time selection prevents same-binary differential
  testing; no features exist today.
- **Reorganize into `psy/{q31,fpe}`**: refactors the Q31 engine (forbidden).
- **Replace Q31 in place**: loses the reference/fallback and the differential
  baseline.