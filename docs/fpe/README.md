# Ferrite Float Psychoacoustic Engine (FPE) — Planning Package

Date: 2026-11-16 (rev 4). Status: **architecture research and implementation
plan; planning only — no production code changed, no engine implemented,
nothing committed.** M0 source/reference verification executed and recorded in
[m0-verification.md](m0-verification.md) (verification only; no implementation).

> **Revision 2 (independent-review remediation).** This revision corrects the
> review findings without changing the approved architecture or scope:
> - **P1 (blocking):** `05` now declares explicit **physical / stored-Q31 /
>   FPE-fraction** domains and derives every formula from source in the single
>   FPE fraction domain (`v_f = v_st/2^31`), with a per-kernel domain table.
>   The rev-1 mix of stored-integer-as-float and fraction domains is removed.
> - **P2:** M2 is an unambiguous **bit-identical pass-through** (delegates to
>   Q31, no float round-trip/rounding; boundary conversion only from M3).
>   Performance validation uses **paired, interleaved runs with a 95%
>   confidence interval** and explicit noise handling (`08`); the ±2% budget
>   is unchanged.
> - **Licensing:** FPE **retains the existing FDK-AAC licence**; no SPDX /
>   licence-file / NOTICE change; categorical legal conclusions softened to the
>   project's documented compliance position (`12` §0).
> - **Precision:** no correct-rounding / 1-ULP claim for `log2`; measurable,
>   platform-tested Ld criteria (`05` §2).
> - **Validation integrity:** skipped frames / incomplete runs cannot silently
>   pass (`07` §7.1); new gates G0 (M2) and risks R-19/R-20.
> All five ADRs reviewed for consistency. See the final remediation report.
>
> **Revision 3 (final-review clarifications).** Two clarifications, no
> architecture/scope change:
> - **M2 persistent-state equivalence:** the M2 bit-identical gate now also
>   compares **all frame-surviving state after every frame** of **multi-frame
>   sequences** (`PreEchoState`, TNS `tns_state`/`tns_info`, MS-Ld history),
>   via `#[cfg(test)]` helpers (no new production API); covers long/short,
>   START/STOP, pre-echo enable/reset, stale-on-short MS-Ld (`07` §3.1, `09` M2,
>   `04` §4, ADR-0003/0004; risk R-21).
> - **G6 confidence rule:** `08` specifies a **95% two-sided CI** for the median
>   paired % difference (distribution-free order-statistic CI primary; bootstrap
>   ≥10 000 resamples cross-check; sign test preferred). **G6 PASS iff CI
>   upper bound ≤ +2%**; an interval **entirely below −2% is a measured
>   improvement and satisfies G6**; **INCONCLUSIVE** when the CI straddles +2%.
>   The ±2% regression budget is preserved (`08` §3–§4, ADR-0004).
>
> **Revision 4 (statistical correction).** The performance protocol is now a
> **fixed-sample confirmatory design** (`08` §3.2): pilot estimates variability;
> the final sample size `N`, rationale and analysis plan are **predeclared
> before** confirmatory collection; the primary analysis runs **exactly once**
> at `N`. The earlier "add pairs until the CI is decisive" optional-stopping
> rule is **removed** (it invalidated nominal 95% coverage). An inconclusive
> result is **reported as inconclusive**; any follow-up is a **separate
> confirmatory experiment** with its own predeclared `N`/plan. A sequential
> design is allowed only via a valid confidence sequence or alpha-spending
> procedure with documented error control. Also documented: percentile-bootstrap
> small-sample limitations, sign-test preference (Wilcoxon symmetry assumption),
> and serial-dependence assessment with a block-based/redesign remedy. The ±2%
> budget, G6 boundaries, M2 state equivalence, architecture/roadmap and
> FDK-AAC licensing are unchanged.
>
> **Revision 4 — final corrections.** (a) **Benchmark tie handling** (`08`
> §3.6): order-statistic CI coverage is exact only for continuous data; ties
> make it conservative (no exact-95% claim), the sign test drops zero
> differences (`N′ = N − #zeros`), tie/zero counts are reported, and frequent
> ties require an explicit qualification or a discrete-valid alternative.
> (b) **M0 executed** (source/reference verification, not implementation):
> evidence and A-2…A-7 disposition in [m0-verification.md](m0-verification.md).
> A-2 resolved; A-3 verified at the reference **parent** commit (`da00af1c:cpp`
> ≡ `3878c45a:cpp`) with a documented residual gap (the pinned `f14ad40a`
> object is unavailable locally). No architecture/scope/licensing change.

FPE is a proposed native-Rust **floating-point implementation of the existing
FDK-AAC psychoacoustic model** already ported in Ferrite. It is a numerical
modernization, not a new psychoacoustic model:

> **Change the numerical representation, not the psychoacoustic model.**

## Executive summary

Ferrite's encoder (`rust/src/aac_enc/`) is a validated port of the Fraunhofer
FDK-AAC 3.0.0 AAC-LC encoder. Its psychoacoustic stage today is a **hybrid**:
the MDCT spectrum is floating-point (`f32` kernel, carried as `f64`), but the
psychoacoustic kernels — band energies, threshold initialization, spectral
spreading, PCM floor, pre-echo temporal control, mid/side energies — run as
**bit-faithful Q31 integer mirrors** of the C fixed-point code
(`psy.rs`, `energy.rs`, `thresholds.rs`, `q31.rs`). Block switching is already
float-native (`f32`); TNS is a separate hybrid subsystem (exact integer
autocorrelation + float Schur); everything downstream of psy (PE, VBR threshold
adaptation, scalefactor estimation, quantization, rate control, bitstream) is
Q31 integer arithmetic.

FPE replaces the **Q31 value arithmetic inside the psychoacoustic kernels**
with `f64` floating-point arithmetic while preserving:

- every FDK algorithm, state transition, and tuning constant;
- the frequency partitions and scalefactor-band relationships;
- the block-floating-point **representation decisions** (`mdctScale`,
  `finalShift`, `tnsSpecShift`, per-band `maxScale`) bit-exactly, so the
  integer-domain interface consumed by the unchanged downstream encoder is
  preserved;
- long/short window behaviour, channel-pair semantics, and temporal state.

The existing Q31 engine remains intact, selectable, and the **default**. FPE
is independently selectable through an additive configuration knob and is
validated by a **three-way differential harness** (upstream FDK C ↔ Ferrite
Q31 ↔ FPE) reusing the existing `enc-oracle` infrastructure and committed
reference vectors.

Key verified facts that shape the design (details in the linked documents):

1. The upstream reference is **FDK-AAC 3.0.0** (AOSP `external/fdk-aac`),
   pinned at private-reference commit `f14ad40a…`; the identical `cpp/` tree
   is recoverable from Ferrite git history at `3878c45a` (`4c69425~1`).
   The local reference checkout has been emptied by temp cleanup and must be
   re-provisioned before Tier-B validation (see `03`, risk R-12).
2. **Tonality estimation exists upstream but is PNS-gated**
   (`tonality.cpp:121-162`, `usePns` guard). Ferrite runs AAC-LC with PNS off,
   so tonality is correctly absent and stays absent in FPE. There is **no
   absolute-hearing-threshold table** in FDK's LC model; the absolute floor is
   the PCM quantization threshold (`sfbPcmQuantThreshold`).
3. Spreading factors and minSnr are **committed oracle-extracted data** in
   Ferrite (`sfb_tables.rs`, `cfg_*.txt`), because C's `fixp_atan`/`fPow`
   approximations do not reproduce in float within useful tolerance (measured
   worst factor deviation 13%). FPE consumes the same tabulated values
   converted to float; recomputing them in float would change the model and
   is deferred to Stage 2.
4. All downstream consumers take **Q31 stored-domain integers and Q31 Ld
   vectors** (`ScfChannelInput`, `QcChannelInput`, `S2ChannelInput`,
   `pe.rs`). FPE computes in the **Q31-fraction domain** (`v_f=v_st/2^31`) and
   quantizes back to the existing output contract at the psy boundary (M3+;
   M2 passes Q31 results through unchanged); no second quantizer/rate-controller
   is created.
5. A rigorous differential infrastructure already exists: `enc_oracle`
   dumps every psy intermediate (energies, thresholds, spread, pre-echo
   history, TNS, grouped vectors, MS vectors), 47 committed `psy_*` frame
   vectors, 18 config snapshots, 15 synthetic fixtures + 4 music/speech WAVs,
   and an established tolerance policy (`32 + |w|/500` stored-domain,
   energy-domain equivalent; the C `fLog2` Taylor error is ~1e-9, and the FPE
   `log2` acceptance bound is **measured**, not assumed — `05` §2).

Stage 1 acceptance is **faithful reproduction of the reference model** within
the established numerical envelopes, with decision-level equivalence where
the Q31 engine is decision-exact, all existing tests passing, and independent
review. Objective quality metrics are supplementary, never the primary gate.

## Document map

| Doc | Content |
|---|---|
| [00-project-charter.md](00-project-charter.md) | Goals, non-goals, principles, scope boundary, Stage-1 acceptance summary |
| [01-current-state-audit.md](01-current-state-audit.md) | Verified Ferrite architecture, data flow, state ownership, numerical census |
| [02-reuse-and-translation-inventory.md](02-reuse-and-translation-inventory.md) | A/B/C/D component classification with evidence |
| [03-fdk-reference-analysis.md](03-fdk-reference-analysis.md) | Upstream version/provenance, function-level mapping, per-component algorithm analysis |
| [04-system-architecture.md](04-system-architecture.md) | Module layout, dispatch, shared types, state, scratch, diagrams |
| [05-floating-point-translation-spec.md](05-floating-point-translation-spec.md) | Domains (physical/stored-Q31/FPE-fraction), f32/f64 analysis, per-kernel domain table + translation formulas, rounding, invariants, tolerances |
| [06-encoder-integration.md](06-encoder-integration.md) | Selection mechanism, config, lifecycle, frame sequence, error handling |
| [07-validation-strategy.md](07-validation-strategy.md) | Three-way differential harness, corpus, tolerances, test tiers, acceptance gates |
| [08-performance-strategy.md](08-performance-strategy.md) | Baseline numbers, measurement plan, budgets, SIMD posture |
| [09-implementation-roadmap.md](09-implementation-roadmap.md) | M0–M9 milestones, tasks, files, tests, acceptance, review gates |
| [10-risk-register.md](10-risk-register.md) | Probability/impact/mitigation/verification per risk |
| [11-coding-model-guidelines.md](11-coding-model-guidelines.md) | Delegation rules, task template, report format |
| [12-licensing-provenance.md](12-licensing-provenance.md) | License obligations, provenance strategy for translated code |
| [adr/](adr/) | Architecture decision records (engine separation, precision, interface, validation, representation lane) |
| [m0-verification.md](m0-verification.md) | M0 source/reference verification evidence: tree hashes, A-2 anchors, A-3 equivalence conclusion, Q31 baseline, A-2…A-7 disposition |

## Reading order for implementers

1. `00` (scope) → `01` (what exists) → `04` (what you will build) →
   `05` (exact arithmetic) → `06` (where it plugs in) → `07` (how it is
   proven) → `09` (your milestone) → `11` (how to work and report).
2. `03` is the algorithmic authority: every translation task cites it.
3. `02`, `08`, `10`, `12`, and the ADRs are reference material consulted
   when a task touches their concerns.

## Evidence conventions

- **Verified**: read directly from source at the cited path/line during this
  audit (Ferrite HEAD `a2c1d53`; upstream tree `3878c45a`).
- **Assumption**: explicitly labelled, with the investigation that would
  confirm it.
- **Recommendation**: a design decision proposed here, recorded as an ADR
  where architectural.

Line numbers refer to Ferrite HEAD `a2c1d53` and upstream tree `3878c45a`
unless stated otherwise. Upstream citations use the `cpp/…` paths of that
tree (recoverable via `git show 3878c45a:cpp/…`).