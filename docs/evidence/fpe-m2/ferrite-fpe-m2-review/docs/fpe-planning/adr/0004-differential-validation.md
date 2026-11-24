# ADR-0004 — Differential validation approach

Status: Proposed. Date: 2026-11-16.

## Context

Stage-1 acceptance is faithful reproduction of the reference model, proven at
intermediate stages — not by final AAC files alone. Ferrite already has a
validated C↔Q31 differential infrastructure (`enc_oracle`, committed vectors,
Tier-B driver, tolerance policy). FPE must reuse it, not reinvent it, and must
not add a production dependency on the C reference.

## Decision

1. **Three-way comparison**: C (oracle ground truth) ↔ Q31 (validated
   baseline) ↔ FPE (under test), at every psy intermediate stage (`07` §4).
2. **Two harnesses**:
   - **Q31↔FPE in-binary** (Tier-A, self-contained, default CI): identical
     inputs to both engines, field-by-field comparison. Primary development
     gate; works without C provisioning.
   - **C↔FPE** (Tier-B, opt-in): compare FPE outputs to `enc_oracle` dumps via
     the existing `PHASE1_ORACLE_DIR` path. No new C instrumentation needed for
     psy stages (the oracle already dumps them).
3. **Incremental bit-exact-then-envelope strategy** (`09`): M2 FPE delegates
   value kernels to Q31 and passes results through **unchanged** (no float
   round-trip, no rounding) ⇒ **bit-identical complete observable output AND
   all frame-surviving persistent state, after every frame of multi-frame
   sequences** (`PreEchoState`, TNS state, MS-Ld history; test-only helpers, no
   new production API; `07` §3.1) proves wiring; M3–M6 float-ify one kernel
   group at a time (introducing the fraction value lane + boundary per kernel),
   each gated by the differential.
4. **Tolerances inherited, not widened**: stored `32+|w|/500`, energy-domain
   equivalent; Ld bound **measured** (C fLog2 ~1e-9 + platform `log2` error,
   `05` §2 — no correct-rounding claim); pure-integer/decision fields **exact**.
   Any widening needs evidence + owner approval. New decision flips are triaged
   (residue vs structural), never forced to pass.
5. **Execution integrity**: skipped frames / missing artifacts / incomplete
   runs never count as success; coverage matrix fully executed; Tier-B SKIP ≠
   PASS (`07` §7.1).
6. **Acceptance gates G0–G9** (`07` §8, incl. G0 M2 pass-through); objective
   metrics supplementary, never the gate. **G6** uses a **fixed-sample 95%
   two-sided CI** of the median paired throughput difference (predeclared `N`,
   single analysis, no optional stopping; `08` §3.2): PASS iff CI upper bound ≤
   +2% (entirely below −2% = measured improvement, passes); INCONCLUSIVE if it
   straddles +2% (`08` §3–§4). Independent validation review at M8.

## Consequences

- Development is unblocked even if the C reference is unavailable (R-12).
- Defects isolate to the single kernel group just changed.
- M2 state equivalence over multi-frame sequences catches temporal-state
  divergence that output-only/single-frame tests would miss (R-21).
- No production dependency on C; ordinary build/test/package unaffected.
- Oracle nondeterminism (GSPEC/SPEC) handled by comparing stable fields only;
  config-only byte-gate; 3-run stability for new vectors.

## Alternatives rejected

- **Final-bitstream-only comparison**: hides intermediate divergence; cannot
  localize defects; assignment explicitly forbids relying on it.
- **New C instrumentation in production**: the oracle is test-only and already
  exposes the needed intermediates; production must not depend on C.
- **Choose tolerances to make FPE pass**: masks defects; envelopes are
  inherited from the validated Q31 policy.