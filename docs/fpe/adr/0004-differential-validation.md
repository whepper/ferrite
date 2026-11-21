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
   Any widening needs evidence + owner approval. Derived, source-verified
   envelope amendments follow the mechanism below (FPE-3.1R1 is the first
   application). New decision flips are triaged (residue vs structural), never
   forced to pass.
5. **Execution integrity**: skipped frames / missing artifacts / incomplete
   runs never count as success; coverage matrix fully executed; Tier-B SKIP ≠
   PASS (`07` §7.1).
6. **Acceptance gates G0–G9** (`07` §8, incl. G0 M2 pass-through); objective
   metrics supplementary, never the gate. **G6** uses a **fixed-sample 95%
   two-sided CI** of the median paired throughput difference (predeclared `N`,
   single analysis, no optional stopping; `08` §3.2): PASS iff CI upper bound ≤
   +2% (entirely below −2% = measured improvement, passes); INCONCLUSIVE if it
   straddles +2% (`08` §3–§4). Independent validation review at M8.

## Amendment mechanism (validation envelope changes)

A validation envelope may be changed only when **all** of the following hold:

1. The difference is **mathematically derived**, not chosen empirically to make
   tests pass.
2. The relevant error sources are **independently identified** and separated.
3. The derivation is **verified against the actual source semantics** (integer
   shifts, rounding, clamping, saturation, accumulation).
4. The envelope is **data-dependent where appropriate** (per band/vector from
   the actual operands, not a fixed global constant).
5. The **owner explicitly approves** the change.
6. **Independent review** confirms the implementation and the evidence.

An amendment is recorded as a dated ADR note with its derivation, source
cross-check, measured results, and approval. It is not described as "tolerance
widening"; arbitrary loosening remains prohibited.

## Amendment — FPE-3.1R1 (owner-approved Option A)

**Context.** FPE-3.1 uses the physical f64 spectrum for energy, while Q31
sums its Q31-mapped integer spectrum. The existing `q31_tol` was calibrated for
C↔Q31, where both sides use the quantized spectrum; it does not by itself cover
the Q31 input-mapping term.

**Approved decision.** Interpretation A — FPE energy retains the physical f64
spectrum. Q31 input-mapping/truncation differences are expected numerical
divergences, not implementation defects. Bit-level agreement is not the
definition of FPE correctness.

**Approved envelope** (`05` §9.2), per band from the actual spectrum and
representation state:

```
s0_b = max(0, maxScale_b − 4)
s_b  = min(2(s0_b − f), 31)
Q_b  = 2^(2f−31)·Σ|X_j| + n_b·2^(2f−33)
T_b  = 2n_b·2^(−s_b)          // signed s_b; negative preserves amplification
R    = 1.5
tol_E(b) = max(32, Q_b + T_b + 1.5) + E_ref/500
```

The old `q31_tol` remains for thresholds, `nm1`, grouped thresholds, and
reused Q31-kernel outputs where its assumptions hold. Ld is compared by
energy-domain inversion with the derived envelope; absolute Ld is measured
separately. Spread energy uses the propagated energy bound; grouped energy uses
the sum of per-window envelopes. Low-energy sentinel divergences are classified
by stored state as Class A (mapping-to-zero) and Class B (post-shift truncation
vs rounding), not by Ld crossing `−1.0`.

**Approval / review.** Owner-approved (Option A). The R1 implementation and
numerical evidence are subject to independent review before the gate is
signed. Downstream decision safety remains outstanding in FPE-3.1R2; R1 alone
does not accept FPE-3.1.

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