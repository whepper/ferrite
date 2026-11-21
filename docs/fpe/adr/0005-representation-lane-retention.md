# ADR-0005 — Retain the integer representation lane

Status: Proposed. Date: 2026-11-16.

## Context

FDK's block-floating-point machinery computes per-band headroom (`maxScale`),
`minSpecShift`, `nrgShift`, `finalShift`, TNS `shiftBits`/`tnsSpecShift`, and
the `mdctScale` trajectory. These exist to keep C's fixed-point arithmetic in
range. In a float engine they are numerically unnecessary (f64 has huge dynamic
range). **However, they are observable downstream**: `mdctScale` is part of the
output contract and drives the Q31 spectrum mapping used by quantization;
`finalShift`/`tnsShift`/`maxScale` are compared **exactly** in existing tests;
pre-echo aligns thresholds by `mdctScale`.

## Decision

FPE **retains the integer representation lane bit-exactly** in Stage 1: it
reuses the existing Q31 helpers (`map_spectrum_to_q31_into`,
`max_scale_spec_into`, integer raw sums + `max_nrg_for_shift`, `rescale_shifts`,
the integer `shiftBits` decision) to produce `mdctScale`/`finalShift`/
`tnsShift`/`nrgScaling`/`maxScale` identically to the Q31 engine. Only the
**value arithmetic** (energy sums, thresholds, spreading, PCM floor, pre-echo,
MS) becomes float in the **Q31-fraction domain** (`v_f = v_st/2^31`, `05` §1).
The value lane never feeds back into representation decisions (one-way
dependency).

## Consequences

- `mdctScale`/shifts/headroom match Q31 (and C) **bit-exactly**, so the existing
  exact-match assertions and downstream quantization behaviour are preserved.
- FPE differences are confined to value rounding ⇒ the tightest, cleanest
  differential surface; any deviation is attributable to float-vs-fixed-point
  value arithmetic alone.
- Some Q31 "scaffolding" remains (headroom/shift analysis). This is a
  deliberate fidelity choice, not an incomplete translation: the scaffolding
  produces observable representation decisions, while the psychoacoustic
  **model arithmetic** is fully float.
- **Authority and independence (FPE-3.1R1).** The integer representation lane
  remains authoritative for every representation decision (`maxScale`,
  `mdctScale`, `finalShift`, `tnsShift`, `nrgScaling`, `shiftBits`). Physical
  f64 energy is independent of Q31 input-mapping quantization: the value lane
  consumes `x_f = x·2^(−m)`, not the dequantized `X/2^31`. These are not
  required to be numerically identical; their difference is bounded by the
  derived Q31↔FPE energy envelope (`05` §9.2) and is never fed back into the
  lane. The lane's exactness requirements are unchanged.
- The integer raw-energy sums are computed for the maxNrg/shift decision; the
  float energy sums are computed for values. This is a small, bounded
  duplication (energy sums are cheap relative to TNS/quantization).

## Alternatives rejected

- **Pure-float headroom (ilog2/frexp), drop finalShift**: would let `mdctScale`
  drift by ±1 at power-of-two knife edges, changing the Q31 spectrum rounding
  and failing exact-match assertions; enlarges the validation surface to
  real-value equivalence everywhere. Defer to Stage 2 as a simplification
  research item with its own charter.
- **Skip representation decisions entirely (finalShift=0)**: same drift risk;
  breaks the observable contract.

## Stage-2 note

A pure-float representation lane (removing the integer headroom duplication)
is a legitimate future simplification, but it changes observable representation
decisions and requires re-validating downstream equivalence at the real-value
level. It is explicitly **out of Stage 1**.