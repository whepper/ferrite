# ADR-0003 — Psychoacoustic interface contract

Status: Proposed. Date: 2026-11-16.

## Context

All downstream consumers (PE, S2 VBR adaptation, S4 scalefactors, S6 rate
control, IS/MS decisions) require **Q31 stored-domain integers and Q31 Ld
vectors** (`ScfChannelInput`, `QcChannelInput`, `S2ChannelInput`,
`PeChannelInput`; verified `01` §3.3). The spectrum enters psy as f64 but every
downstream stage is integer. Rewriting downstream is a non-goal.

## Decision

FPE **emits the existing output contract unchanged**: it constructs the shared
`ChannelOutput`/`GroupedOutput`/`MsOutput` types (`psy.rs:247/194/384`) with
stored-domain i32 energies/thresholds/spread, f64 Ld vectors, integer
mdctScale/shifts/maxScale, grouped vectors, and MS vectors. When a kernel is
float (M3+), FPE quantizes its fraction-domain results to this contract at the
boundary (`05` §7, `round(v_f·2^31)`, round-to-nearest, matching
`map_spectrum_to_q31_into`).

**M2 pass-through (P2):** in M2 the value lane delegates to the Q31 kernels and
their stored-int / f64-Ld results pass through **unchanged** — no fraction
conversion, no boundary rounding. The boundary quantization activates
per-kernel only in M3+ (`09` M2, `05` §7, ADR-0004). The bit-identical M2 gate
applies to the complete observable output **and all frame-surviving persistent
state after every frame** of multi-frame sequences (`PreEchoState`, TNS state,
MS-Ld history), compared via `#[cfg(test)]` helpers — no new production API
(`07` §3.1).

- `StreamEncoder::frontend`'s `LiveChannel` construction
  (`stream_driver.rs:872-966`) and all downstream stages are **untouched**.
- Grouped MS-Ld stays **exact Q31** (`calc_ld_data`), because the short-block
  MS decision consumes it directly.
- Long energy Ld passes through the existing `q31_of_ld_vec` float→Q31 path;
  short energy Ld via `ld_of(stored)` — both conventions preserved.
- No new downstream input structs; no second quantizer/rate-controller.

## Consequences

- A single downstream path serves both engines; VBR-1..6 behaviour preserved.
- FPE's float precision is confined to the psy kernels; the integer boundary
  is the only quantization point (no new approximation convention).
- The interface is representationally asymmetric by design (f64 in, Q31 out)
  — this is the existing Ferrite contract, not an FPE invention.

## Alternatives rejected

- **Float-native downstream (rewrite PE/S2/S4/S6 to f64)**: non-goal; would
  change quantization/rate-control and invalidate the validated baseline.
- **New float output struct + adapter per consumer**: duplicates the contract
  and risks divergence; the shared types already exist.
- **Emit only float and convert in each consumer**: scatters quantization
  conventions; the single boundary is cleaner and matches today's design.