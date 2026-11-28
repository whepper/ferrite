# FPE M3 — Exit Assessment

**Assessment date:** 28 November 2026
**Baseline:** `main` at HEAD `189e6a79b78a84dcc5be88eaca5ffaba24097acc`; index
empty at assessment start; the working tree carries the pre-existing
M3/R2.2 modifications and untracked evidence described in §8.
**Scope:** Assessment of the complete M3 exit boundary, plus documentation-only
status reconciliation. **This record does not declare M3 complete, does not
authorise M4, and does not change production code, tests, numerical behaviour,
thresholds, tolerances, envelopes, baseline pins, schemas, or evidence
bundles.**
**Recommendation:** **READY FOR M3 OWNER ACCEPTANCE** (§10).

> **Post-decision note (28 November 2026):** the owner has since accepted M3
> with documented limitations — see
> [`M3-owner-acceptance.md`](M3-owner-acceptance.md) and the dated addendum at
> the end of this document. The assessment body below is preserved as the
> pre-decision record; its matrix, limitations and deferred work remain the
> evidence basis.

Governing sources read for this assessment:

- [`../09-implementation-roadmap.md`](../09-implementation-roadmap.md) §M3
  (normative tasks, tests, acceptance, review).
- [`../07-validation-strategy.md`](../07-validation-strategy.md) §5.1–§5.2,
  §8 (gates and downstream decision-safety requirements).
- [`findings-register.md`](findings-register.md) §13 (M3 boundary set) and
  the dated updates §14–§20.
- [`R2.2-group-E2-E3-owner-decision.md`](R2.2-group-E2-E3-owner-decision.md)
  §1 (M3 boundary and scope-conflict resolution), §3 (FpeError/A-20), §5
  (gate confirmations, Tier-B/N-2/A-5/A-13 allocations).
- The component owner decisions:
  [`a4-owner-decision.md`](a4-owner-decision.md),
  [`fpe-3.2-owner-decision.md`](fpe-3.2-owner-decision.md),
  [`f6-owner-decision.md`](f6-owner-decision.md),
  [`a5-n2-owner-decision.md`](a5-n2-owner-decision.md).
- The R2.2 records:
  [`R2.2-downstream-decision-safety-design.md`](R2.2-downstream-decision-safety-design.md)
  §6 (AC-1…AC-10),
  [`R2.2-group-C-owner-acceptance.md`](R2.2-group-C-owner-acceptance.md),
  [`R2.2-group-D-owner-decision.md`](R2.2-group-D-owner-decision.md),
  [`R2.2-AC-7-divergence-characterization.md`](R2.2-AC-7-divergence-characterization.md)
  §12, [`R2.2-F20-F22-F26-numerical-owner-acceptance.md`](R2.2-F20-F22-F26-numerical-owner-acceptance.md),
  [`R2.2-AC-9-M3-scope-matrix.md`](R2.2-AC-9-M3-scope-matrix.md),
  [`R2.2-AC-9-margin-derivation-report.md`](R2.2-AC-9-margin-derivation-report.md),
  [`R2.2-AC-10-independent-review.md`](R2.2-AC-10-independent-review.md), and
  [`R2.2-AC-9-owner-acceptance.md`](R2.2-AC-9-owner-acceptance.md).
- [`R2.2-findings-register.md`](R2.2-findings-register.md) R22-AC9-13…15.

---

## 1. Executive status

**All technical and evidence requirements of the authoritative M3 boundary
are satisfied. No genuine M3 exit blocker was found. The only remaining step
between the current state and formal M3 completion is the owner's declaration
of M3 completion.**

The substantive position:

1. **FPE-3.1** (float rescale band energy) is implemented and its
   energy-envelope evidence is accepted (R2.1-B G-1, PASS WITH FINDINGS); its
   downstream decision-safety leg is now owner-accepted through AC-9.
2. **FPE-3.2** (float TNS energy recompute) is owner-accepted with documented
   limitations F1–F6 after an independent Mistral 4 review (PASS WITH FINDINGS,
   no correctness defect).
3. **FPE-3.3 / A-4** (f32-vs-f64 experiment) is owner-accepted with documented
   limitations; f64 remains the approved Stage-1 implementation.
4. **F-6** is closed on documented structural unreachability plus retained
   watches.
5. **R2.2 downstream decision-safety** is owner-accepted for the M3 boundary:
   AC-9 is ACCEPT WITH DOCUMENTED LIMITATIONS, AC-10 is PASS WITH FINDINGS with
   no M3 acceptance blocker, and Group C, Group D, AC-7 and the F-20/F-22/F-26
   numerical evidence are separately accepted.
6. **The FpeError/A-20 M3 decision** is recorded: the decision is required for
   M3; graceful frame-level implementation is **not** required for M3 and is
   deferred to FPE-default readiness or separately authorised scope. The
   release-active `assert!` stands.
7. **Engineering checks for M3** are green on the current tree: Q31 production
   baseline 2/2, AC9SCHEMA 1 hashes 5/5, Group E hashes 36/36, licensing OK,
   `git diff --check` clean, Q31 remains the default. The full library suite is
   exactly the documented state: 671 passed / 18 failed / 2 ignored, where the
   18 are the 7 preserved FPE pins plus the 11 Tier-B/environmental blockers.

The temptingly “open” items — the seven pins, AC10-F1…F8, N-2, Tier-B,
A-09/perceptual work, D-6/P-termination, OP-4, OD-3 five-frame, SG-2/E6 — are
either explicitly deferred outside M3 by owner decision or documented
non-blocking limitations. None is an M3 exit requirement (§5).

**M3 is not declared complete by this document. M4 is not authorised.**

### At-a-glance

| Area | Status for M3 |
|---|---|
| FPE-3.1 energy kernel | Complete; energy-envelope and downstream legs accepted with documented limitations |
| FPE-3.2 TNS recompute | Complete; owner-accepted with F1–F6 limitations |
| FPE-3.3 / A-4 | Complete; owner-accepted with documented limitations |
| F-6 | Closed |
| R2.2 decision safety | Complete for M3; AC-9/AC-10/Group C/D/AC-7 accepted |
| F-20/F-22/F-26 numerical evidence | Accepted; perceptual conditions remain later-phase |
| FpeError/A-20 decision | Recorded; implementation deferred by owner decision |
| M3 numerical review gate | Complete via independent component reviews + AC-10 |
| Engineering checks (G2/G3/G7/G9 subset) | Green; documented pin/Tier-B exceptions only |
| Formal M3 completion | **Outstanding — owner declaration only** |

---

## 2. Authoritative M3 exit criteria

### 2.1 Precedence and scope resolution

Three layers govern M3:

1. **Normative milestone definition** — `09` §M3 defines FPE-3.1, FPE-3.2,
   FPE-3.3, their tests, acceptance (“G2 for energy with the R1 envelope;
   envelope and absolute Ld differences recorded”) and the numerical review
   gate.
2. **Validation gates** — `07` §5.2 requires downstream decision safety before
   FPE-3.1 acceptance; §8 defines G0–G9. For M3 the applicable subset is G2
   (Q31↔FPE baseline for the floated kernels), G3 (no regression), G7
   (licensing/hygiene), G8 (review) and G9 (default unchanged). G1/G5
   (Tier-B C parity/decoder), G4 (fuzz at M8) and G6 (performance at M9) are
   later gates.
3. **Controlling M3 boundary** — [`findings-register.md`](findings-register.md)
   §13 and [`R2.2-group-E2-E3-owner-decision.md`](R2.2-group-E2-E3-owner-decision.md)
   §1 define what M3 requires now and resolve the scope conflict with the
   broader R2.2 design language. The owner decision is explicit: **M3 is the
   energy-kernel + decision-safety + FpeError-decision milestone; full R2.1-B
   conditional closure (termination, boundedness, perceptual) is a later,
   separately authorised matter.**

The owner decisions therefore control over any broader reading of the R2.2
design’s AC-1…AC-10 or the original `09` test list where those documents
describe later work.

### 2.2 Exact M3 exit requirements

| ID | Requirement | Source |
|---|---|---|
| M3-C1 | Float rescale band energy (long + short), fraction domain: `E_real=Σspec_f64²`, `E_f=E_real·2^(−2m')`, boundary `E_st=round(E_f·2^31)`, rescale Ld with §6.8 conventions. | `09` §M3 FPE-3.1; `05` §6.1/§6.8 |
| M3-C2 | Float TNS energy recompute: long with integer `shiftBits` from the representation lane; short no-Ld quirk; overflow rescale in the value lane. | `09` §M3 FPE-3.2; `05` §6.2 |
| M3-C3 | A-4 f32-vs-f64 experiment: measured energy-sum error on the corpus, recorded envelope, f64 choice confirmed. | `09` §M3 FPE-3.3; validation roadmap N-1; findings register A-12 |
| M3-C4 | G2 for energy within the owner-approved FPE-3.1R1 derived per-band envelope; envelope and absolute Ld differences recorded. | `09` §M3 acceptance; `05` §9.2; `07` §8 G2 |
| M3-C5 | Unit energy sums (hand-computed, silence, full-scale); `q31_tol` retained for thresholds, `nm1`, grouped thresholds and reused Q31-kernel outputs; `shifts`/`maxScale` exact. | `09` §M3 tests; `07` §5 |
| M3-C6 | F-6 short-TNS `tnsSpecShift != 0` construction requirement resolved (closed by documented structural unreachability + retained watches, per owner decision). | `07` §5.2; findings register A-14; [`f6-owner-decision.md`](f6-owner-decision.md) |
| M3-C7 | R2.2 downstream decision-safety acceptance: D-1 (PE), D-2 (ATS), D-3 (S2), D-4 (S4), D-5 (S6/accounting), D-7 (margin classification), D-8 (operating-point coverage), with explicit records for unobserved/unavailable paths. | findings register §13; E2/E3 decision §1; design §6 AC-1…AC-9; validation roadmap §2.2 |
| M3-C8 | F-20/F-22/F-26 per-finding PE/ATS evidence required for FPE-3.1 acceptance. | findings register §13; E2/E3 decision §1; [`R2.2-F20-F22-F26-numerical-owner-acceptance.md`](R2.2-F20-F22-F26-numerical-owner-acceptance.md) |
| M3-C9 | Independent review and owner acceptance of the applicable R2.2 evidence. | findings register §13; E2/E3 decision §1; design §6 AC-10 |
| M3-C10 | FpeError/A-20 M3 decision. | findings register §13; E2/E3 decision §3 |
| M3-C11 | Existing FPE-3.1/3.2/3.3 milestone acceptance requirements. | findings register §13; E2/E3 decision §1; `09` §M3 |
| M3-C12 | Numerical review of the milestone (independent of the kernel author where practical). | `09` §M3 review; `11` §5 |
| M3-C13 | Applicable engineering gates: no new regression, Q31 baseline unchanged, licensing/hygiene clean, default remains Q31. | `07` §8 G3/G7/G9 |
| M3-C14 | Formal M3 completion declaration by the owner. | `09` milestone closure; AC-9 owner acceptance §3/§5 |

**Explicitly outside M3 by owner decision:** D-6 recurrence and P-termination,
the OP-4 recurrence campaign, the retained five-frame OD-3 boundary evidence,
A-09 perceptual evidence, the nine R2.1-B conditional acceptances, N-2 Linux
x86-64 measurement (M8), A-5 performance/denormal work (M9), Tier-B C parity
and decoder (M8/Stage-1), A-13 seam decision (M4 entry), and M4 itself.

---

## 3. Complete exit matrix

Status vocabulary: **Complete** / **Partially complete** / **Outstanding** /
**Deferred**. Classification: **M3 blocker** / **Non-blocking limitation** /
**Later phase**.

| ID | Requirement (source) | Required outcome | Status | Evidence | Remaining work | Classification |
|---|---|---|---|---|---|---|
| M3-01 | FPE-3.1 float rescale energy (M3-C1) | Long+short fraction-domain energy, boundary rounding, §6.8 Ld | **Complete** | `rust/src/aac_enc/fpe/energy.rs` `rescale_band_energy_into`; `energy::tests::rescale_*`; G-1 report; R2.1-B owner decision §2.1; AC-10 current-tree audit zero failures | None for M3 | Non-blocking limitation: F-01/F-02 sentinel domain and the nine conditional findings remain as accepted |
| M3-02 | FPE-3.2 float TNS recompute (M3-C2) | Long `shiftBits`, short no-Ld quirk, overflow rescale | **Complete** | `tns_recompute_energy_into`; `energy::tests::tns_*`; Mistral 4 review PASS WITH FINDINGS; [`fpe-3.2-owner-decision.md`](fpe-3.2-owner-decision.md) | None for M3 | Non-blocking limitations F1–F6 (corpus nonzero-shift unobserved, N-2 M8, `audit.rs` labels, unrebaselined pins, FpeError deferred, clean-state packaging) |
| M3-03 | FPE-3.3 / A-4 experiment (M3-C3) | Measured f32-vs-f64 error, envelope recorded, f64 confirmed | **Complete** | [`FPE-3.3-A-4-experiment-report.md`](../FPE-3.3-A-4-experiment-report.md); [`a4-owner-decision.md`](a4-owner-decision.md); `precision::tests::a4_precision_corpus` | None for M3 | Non-blocking limitations (ARM64 only, `shiftBits` 0-only, f32 diagnostic only) |
| M3-04 | G2 energy envelope + absolute Ld recorded (M3-C4) | Zero over-envelope; maxima recorded | **Complete** | G-1 validation report; current-tree audit §8.2: zero energy/value/Ld/identity failures; maxima and utilisation recorded | None for M3 | — |
| M3-05 | Unit energy sums; exact shifts/`maxScale`; `q31_tol` retention (M3-C5) | Hand-computed/silence/full-scale; exact representation fields | **Complete** | `energy::tests::rescale_energy_*`, `rescale_ld_*`, `tns_*`; `audit.rs` exact-field comparators; `tests/mod.rs` `q31_tol` policy | None for M3 | — |
| M3-06 | F-6 closure (M3-C6) | Structural unreachability + retained watches | **Complete (closed)** | [`f6-owner-decision.md`](f6-owner-decision.md); `f6_tns` 2/2; audit F-6 observations 0 | None | Non-blocking residuals F6-R2…F6-R6 retained |
| M3-07 | R2.2 decision safety D-1…D-5/D-7/D-8 (M3-C7) | Production-replay evidence for PE/ATS/S2/S4/S6/accounting; honest statuses | **Complete for M3** | Group C (AC-1/AC-2/AC-3/AC-5, OP-1…OP-6); Group D (AC-9 margins; F-20/22/26 replay); E2/E3 (AC-6); AC-7 (D-7); AC-9 scope matrix + artifact (D-1/D-3/D-4/D-5); AC-10 PASS WITH FINDINGS | None for M3; D-6/AC-4 deferred | Non-blocking limitations: unobserved arms, later-phase sub-branches (AC10-F2…F7) |
| M3-08 | F-20/F-22/F-26 PE/ATS evidence (M3-C8) | Per-finding admission/direction, book/scale, ATS, PE, Ld/threshold, replay equality | **Complete numerically** | Six Group E replay rows; [`R2.2-F20-F22-F26-numerical-owner-acceptance.md`](R2.2-F20-F22-F26-numerical-owner-acceptance.md) | Perceptual conditions later (not M3) | Later phase for conditional closure; numerical leg accepted |
| M3-09 | Independent review + owner acceptance of R2.2 evidence (M3-C9) | Independent sign-off; owner disposition | **Complete** | Qwen4 Max (Group C), GLM-5.3-Flash (Group D), Mistral 4 (E2/E3), AC-7 owner decision, AC-10 PASS WITH FINDINGS, AC-9 owner acceptance | None | — |
| M3-10 | FpeError/A-20 decision (M3-C10) | Decision recorded for M3 | **Complete (decision)** | E2/E3 decision §3: required for M3; implementation not required; deferred to FPE-default readiness; `assert!` stands | Implementation later (outside M3) | Non-blocking; implementation deferred |
| M3-11 | FPE-3.1/3.2/3.3 milestone acceptance (M3-C11) | Component acceptance requirements met | **Complete (technical/evidence legs)** — FPE-3.2/3.3 owner-accepted; FPE-3.1 energy + downstream legs accepted; formal FPE-3.1 component acceptance is subsumed by the pending M3 declaration | FPE-3.2/FPE-3.3 owner decisions; FPE-3.1 energy envelope accepted via R2.1-B G-1 and its decision-safety leg via AC-9 | Formal M3 declaration encompasses FPE-3.1 | — |
| M3-12 | Numerical review gate (M3-C12) | Independent numerical review | **Complete** | Qwen G-1; DeepSeek R2.1-B numerical review; Mistral 4 FPE-3.2; Mistral 4 A-4 re-review; AC-10 | None | — |
| M3-13 | Engineering gates G3/G7/G9 (M3-C13) | No new regression; licensing; default Q31 | **Complete for M3** | `production_baseline` 2/2; full suite 671/18/2 with the documented exceptions; `check-licensing.sh` OK; `git diff --check` clean; `PsyEngineKind::default() == Q31` test | Clean-state packaging/CI at commit (FPE-3.2 F6) | Non-blocking: 7 pins + 11 Tier-B/environmental blockers; no rebaseline authorised |
| M3-15 | C↔FPE NRG/NRGLD Tier-B (`09` §M3 tests) | Compare against the pinned C reference | **Deferred** | Tier-B blocked (A-10/OD-1; no pinned C reference/oracle/decoder) | Reference provisioning and C-parity run at M8/Stage-1 | Later phase (M8/Stage-1; blocked, not waived) |
| M3-14 | Formal M3 completion declaration (M3-C14) | Owner declaration | **Outstanding (owner action)** | AC-9 owner acceptance §3/§5 defers this to the M3 exit assessment; this assessment finds readiness | Owner declares M3 complete; no technical work | **M3 blocker — owner action only (no technical work)** |

**Matrix conclusion:** every technical requirement applicable to M3
(M3-01…M3-13) is complete for M3, with documented limitations that the owner
has already accepted or explicitly deferred. M3-15 is deferred to M8/Stage-1
by owner decision. Only M3-14 remains, and it is an authority action rather
than implementation work.

---

## 4. Accepted work and evidence

### 4.1 FPE-3.1 — float rescale band energy

- **Implementation:** `rust/src/aac_enc/fpe/energy.rs`
  (`rescale_band_energy_into`) and the orchestration call sites in
  `fpe/orchestrate.rs`; the representation lane remains integer-exact.
- **Numerical envelope:** the owner-approved FPE-3.1R1 derived per-band
  envelope (`05` §9.2) replaced the original blanket `q31_tol` for rescale,
  spread and grouped energies. `q31_tol` remains for thresholds, `nm1`,
  grouped thresholds and reused Q31-kernel outputs.
- **Acceptance basis:** R2.1-B owner decision §2.1 accepts G-1 as PASS WITH
  FINDINGS for F-03…F-30 (zero energy/value/Ld/identity failures under the
  unchanged envelope); the nine conditional findings remain conditional. The
  downstream decision-safety leg required before FPE-3.1 acceptance is now
  satisfied by AC-9/AC-10 within the M3 boundary.
- **Current-tree evidence:** `probe_envelope::g1_probe_corpus_aggregate`
  reports zero energy/value/Ld/identity failures (see §8.2).

### 4.2 FPE-3.2 — float TNS energy recompute

- **Implementation:** `tns_recompute_energy_into` (long: float values +
  integer `shiftBits` + guarded Ld with `−shiftBits·2/64` correction; short:
  float values, no Ld array — the C quirk preserved); mirror-then-replace
  integration in `fpe/orchestrate.rs`; overflow rescale remains the reused
  Q31 path.
- **Independent review:** Mistral 4, 26 November 2026, PASS WITH FINDINGS;
  no correctness defect.
- **Owner disposition:** ACCEPT WITH DOCUMENTED LIMITATIONS
  ([`fpe-3.2-owner-decision.md`](fpe-3.2-owner-decision.md)). Findings F1–F6
  travel with the acceptance; none is an M3 blocker.

### 4.3 FPE-3.3 / A-4 — f32-vs-f64 experiment

- **Evidence:** 27 fixtures, 2,757 frames, 233,910 unique window-band cells,
  467,820 combined kernel observations; 100% supported; envelope utilisation
  ≤ 0.031; zero over-envelope, guard/sentinel or Class B flips.
- **Owner disposition:** ACCEPT WITH DOCUMENTED LIMITATIONS
  ([`a4-owner-decision.md`](a4-owner-decision.md)); f64 retained (ADR-0002
  unchanged), f32 diagnostic only for Stage 2.

### 4.4 F-6 — short-TNS unreachability

- **Closure basis:** the short branch of `tns_encode_channel` cannot write the
  element-global `tnsSpecShift`; per-element-per-frame zero initialisation;
  D-022 synchronised stereo; release-active enforcement. Retained watches:
  `audit.rs` unsupported-marking, T1 scan, corpus zero-asserts; all 0 on the
  current run.
- **Owner disposition:** closed with residuals F6-R2…F6-R6 retained
  ([`f6-owner-decision.md`](f6-owner-decision.md)).

### 4.5 R2.2 decision safety, AC-9 and AC-10

- **Group C** (AC-1/AC-2/AC-3/AC-5): OP-1…OP-6 matrix, production replay,
  state capture, coverage accounting; accepted with documented limitations
  (unobserved `s6_multipass`, `s6_crash`, stereo `is_active` arms remain
  explicit).
- **Group D** (AC-9 margins; F-20/F-22/F-26 replay): classification,
  observational margins, attribution, bundled-PE replay; accepted with
  D-R1…D-R4 (D-R2 subsequently accepted as documented; the AC-9 margins were
  completed later in the separate AC-9 evidence).
- **E2/E3** (AC-6): deterministic bundle generation, manifest/SHA-256,
  environment gating, durability and ordering; accepted with F-1…F-5
  limitations (SG-2 clean-checkout remains an E6 obligation).
- **AC-7** (D-7): seven pins, S1/S2/S4 flips characterized; owner accepted the
  characterization with documented limitations; no rebaseline; 187 S4 flips
  remain unjudged.
- **AC-9** (D-1/D-3/D-4/D-5): M3 scope matrix classifies every remaining
  family; M3-mandatory remainder implemented as test-only evidence;
  `AC9SCHEMA 1` artifact at **134,617 rows**, **1,176 Group E links**, 5/5
  hashes; owner accepted with documented limitations.
- **AC-10:** independent acceptance review, **PASS WITH FINDINGS**, no M3
  acceptance blocker; independently reproduced 5/5 AC-9 hashes, 36/36 Group E
  hashes, row counts, link integrity, and the focused tests.
- **F-20/F-22/F-26:** numerical per-finding evidence accepted for M3; the nine
  R2.1-B conditionals remain conditional pending perceptual work.

### 4.6 FpeError/A-20

The M3-required decision is recorded in
[`R2.2-group-E2-E3-owner-decision.md`](R2.2-group-E2-E3-owner-decision.md) §3:

- the decision is required for M3;
- graceful frame-level error-path implementation is **not** required for M3;
- actual implementation is deferred to FPE-default readiness or a separately
  authorised scope;
- `fraction_to_stored`, its release-active assertion, the public API and
  frontend error propagation are unchanged.

The finding therefore changes from “open decision” to “decided for M3;
implementation deferred”. The residual implementation is outside M3.

### 4.7 Findings reconciliation

| Finding | M3 relevance | Current status after this assessment |
|---|---|---|
| A-08 (no PE/ATS/downstream validation) | Direct M3 gate (D-1…D-5/D-7/D-8) | **Resolved for the M3 boundary**: evidence accepted via Group C/D, AC-7, AC-9/AC-10. Remains open only for the explicitly deferred conditional-closure legs (D-6/P-termination, OP-4, OD-3, A-09). |
| A-12 (A-4 outstanding) | Direct M3 gate | Resolved; A-4 accepted with documented limitations. |
| A-14 / F-6 | Direct M3 gate | Closed 27 November 2026. |
| A-20 (FpeError) | Direct M3 decision gate | Decision recorded; implementation deferred. |
| A-11 / N-2 | Not M3 | Open for the Linux x86-64 leg; M8 requirement. |
| A-09 (perceptual) | Not M3 | Open; later-phase conditional closure. |
| A-10 (Tier-B) | Not M3 | Blocked, not waived; M8/Stage-1. |
| A-13 (seam) | Not M3 | Open; M4-entry decision. |
| A-21 / OD-6 (R2.1-B byte baseline) | Not M3 | Open; R2.1-B change-comparison hygiene; AC-9/E2/E3 determinism is separately evidenced. |
| A-22 / A-15 (doc precision/status index) | Not M3 | Open; documentation precision. |
| F-20/F-22/F-26 conditionals | Numerical leg is M3; perceptual leg is not | Numerical evidence accepted; conditionals remain conditional pending P-2/P-3. |
| Nine R2.1-B conditionals | Not M3 (deferred) | Conditional; no change. |
| AC10-F1…F8 | Non-blocking / later-phase by AC-10 | Unchanged; none closed, waived or reclassified. |
| D-R2 (`s2.ah_clamp_short`) | M3 evidence leg | Accepted with documented limitation; MDEF v2 `Required` marker retained. |
| R22-E23-08 (SG-2 clean-checkout) | Not M3 | Outstanding E6 obligation; not waived. |

R22-register reconciliation: R22-C-03 (applicable-but-unobserved arms) remains
an accepted limitation; R22-D-03 (F-20/F-22/F-26 exact-value pinning) is a
deferred follow-up, not an M3 gate; R22-E23-08 (SG-2) remains an E6
obligation; R22-AC9-06's “not ready for final owner acceptance” wording is
historical and superseded by R22-AC9-13/14. None changes the M3 conclusion.

---

## 5. Genuine outstanding M3 blockers

**None technical.**

The candidates that could be mistaken for M3 blockers, and why they are not:

1. **Seven preserved FPE pin failures** (six `r21b_p0b_*` plus
   `probe_envelope::g1_probe_corpus_aggregate`). They are the deterministic
   consequence of the accepted FPE-3.2 energy change, fully characterized in
   AC-7; the owner explicitly preserved them without rebaseline (E2/E3 decision
   §4; AC-7 §12). They are visible failures, not hidden regressions; no M3 exit
   criterion requires these specific pins to be green.
2. **AC10-F1…F8.** AC-10 explicitly classifies all eight as minor and
   non-blocking or later-phase; the AC-9 owner acceptance preserves them. None
   is an M3 requirement.
3. **N-2 Linux x86-64 measurement.** The owner decision establishes N-2 as an
   **M8** requirement, expressly not an M3 blocker.
4. **Tier-B C parity / decoder (A-10).** Blocked, not waived; allocated to
   M8/Stage-1 by the E2/E3 owner decision §5. The `09` M3 test list’s
   “C↔FPE NRG/NRGLD (Tier-B)” entry is read as deferred for M3 by that
   allocation, not as an unmet M3 blocker.
5. **A-09 perceptual evidence.** Explicitly not an M3 prerequisite; it closes
   the R2.1-B conditionals and supports any future default change.
6. **D-6/P-termination, OP-4, OD-3 five-frame, MS/M7 downstream.** Explicitly
   deferred by the M3 boundary.
7. **SG-2 clean-checkout / E6.** An E6 obligation under design §11.2, not an
   M3 gate.
8. **Clean-state packaging (FPE-3.2 F6) and `cargo fmt --check` drift.** Real
   repository-hygiene items for the eventual commit/CI pass, but not M3
   acceptance criteria; the formatting drift is entirely in pre-existing
   uncommitted test instrumentation and was not modified here (see §8.4).
9. **No standalone “FPE-3.1 accepted” record.** The FPE-3.1 energy envelope is
   accepted through R2.1-B G-1 and its downstream leg through AC-9; the formal
   M3 declaration is the natural umbrella acceptance. A separate record is
   optional, not a technical blocker.

### 5.1 The one remaining M3 exit action — owner declaration

| Field | Record |
|---|---|
| Requirement | M3-C14; `09` milestone closure; AC-9 owner acceptance §3/§5 reserves the completion declaration to the owner. |
| Required outcome | The owner formally declares M3 complete (and thereby settles the formal FPE-3.1 component acceptance). |
| Current status | **Outstanding — authority action only.** All technical/evidence legs are complete. |
| Evidence | This assessment; the component owner decisions; AC-9/AC-10; Group C/D/AC-7; F-20/F-22/F-26; F-6; A-4; FPE-3.2; validation results §8. |
| Remaining work | None technical. The owner issues the M3 completion record/declaration. |
| Validation | The declaration should cite this assessment and the evidence above; no new test or evidence run is required. |
| Owner authorisation | **Required and sufficient.** No implementer action can substitute for it. |

---

## 6. Explicitly deferred work

| Item | Authority | Destination |
|---|---|---|
| D-6 recurrence / P-cascade termination | M3 boundary; E2/E3 decision §1 | Later R2.1-B conditional closure |
| OP-4 recurrence campaign | M3 boundary | Later conditional closure |
| Retained five-frame OD-3 boundary evidence | M3 boundary | Later conditional closure |
| A-09 perceptual validation (P-1…P-5) | M3 boundary; E2/E3 decision §5 | Later; required to close F-20/22/26 + nine conditionals and before any default change |
| D-10 quiet short-window sentinel/PE test | Validation roadmap (gate: R2 closure) | Later R2 conditional closure; `fpe-tns-active-short` is Rust-resident, `fpe-short-burst-quiet` remains Python-only by design |
| Nine R2.1-B conditional acceptances | M3 boundary | Conditional; perceptual/termination evidence later |
| N-2 Linux x86-64 measurement | A-5/N-2 owner decision | M8 |
| A-5 performance/denormal investigation | A-5/N-2 owner decision | M9 |
| Tier-B C parity + decoder (A-10/OD-1) | E2/E3 decision §5 | M8/Stage-1; blocked, not waived |
| A-13 composite seam decision | E2/E3 decision §5 | M4 entry (owner decision) |
| M4 implementation | E2/E3 decision §5 | Not authorised |
| SG-2 clean-checkout proof | E2/E3 SG-2 clarification | E6 obligation |
| FpeError production implementation | E2/E3 decision §3 | FPE-default readiness or separately authorised scope |
| AC10-F1…F8 later-phase recommendations | AC-10; AC-9 owner acceptance | Later phase; no M3 expansion |
| A-21/OD-6 byte baseline; A-22/A-15 doc precision | Earlier assessments | Non-M3 hygiene |
| MS/M7 downstream and D-9 | M3 scope matrix §3 | M7 |

---

## 7. Implementation work performed

**No production or test implementation was required by the remaining M3
scope, and none was performed.**

The M3-mandatory implementation had already been completed and accepted before
this assessment: FPE-3.1/FPE-3.2/FPE-3.3, F-6, the R2.2 harness/evidence, the
AC-9 M3-remainder families, and the F-20/F-22/F-26 numerical slice. Writing
duplicate kernels or tests now would add no required evidence and would risk
changing accepted behaviour or duplicating an existing gate.

Repository changes made by this pass are documentation-only:

1. **New:** this assessment,
   `docs/fpe/assessment/M3-exit-assessment.md`.
2. **Updated:** `docs/fpe/assessment/findings-register.md` — A-08 and A-20
   status lines reconciled with the accepted evidence/decision, plus a dated
   §21 update recording this assessment and the reconciliation.

No production file, test file, fixture, manifest, lockfile, dependency, ADR,
threshold, tolerance, envelope, baseline pin, schema, MDEF entry, frozen E1
bundle, or AC9SCHEMA 1 artifact was modified. No evidence was regenerated. No
file was staged, committed, pushed, reset, rebased, stashed or cleaned.

---

## 8. Validation results

### 8.1 Repository state

- HEAD: `189e6a79b78a84dcc5be88eaca5ffaba24097acc` (unchanged).
- Branch: `main`.
- Index: empty at start and after this pass.
- Working tree: 19 modified entries (the pre-existing M3/R2.2 set, including
  this pass’s edit to `findings-register.md`) plus 30 untracked entries (the
  pre-existing evidence/test set plus this new assessment). No pre-existing
  entry was reverted, renamed, deleted or reformatted.
- `git diff --check -- docs rust`: clean.
- No evidence bundle was regenerated or written.

### 8.2 Current-tree numerical audit (FPE-3.1/FPE-3.2)

From `cargo test -p ferrite-aac --lib fpe::` (28 November 2026), the retained
G-1 probe-corpus audit reports **zero energy/value/Ld/identity/unsupported**
failures. The test’s separate grouped-B sentinel-count assertion is one of the
seven preserved pins listed in §8.3; it is not an envelope failure.

| Metric | Result |
|---|---|
| Long rescale (n=4,892) | max\|Δ\| 2,777; max tolerance 3,930,673.336; max envelope utilisation 0.1182; fail 0 |
| Short rescale (n=1,040) | max\|Δ\| 99; max tolerance 571,867.072; max utilisation 0.0312; fail 0 |
| Grouped energy (n=520) | max\|Δ\| 265; max tolerance 1,442,954.048; max utilisation 0.0312; fail 0 |
| Ld window-energy (n=5,932) | max\|ΔLd\| 0.073607554 (probe-quiet-48k f8 ch1 w0 b40) |
| Ld grouped-energy (n=520) | max\|ΔLd\| 0.53125 (probe-panned-48k f5 ch1 g2 b9) |
| Ld threshold (n=7,682) | max\|ΔLd\| 0.000010395 |
| Independent identity gates | Q31 6,676 / FPE 3,338 / FPE Ld 4,300 checked; all 0 failures |
| Decomposition term checks | 2,816 checked; 0 failures |
| Unsupported comparisons | 0 |
| F-6 `tnsSpecShift != 0` observations | 0 |

The maxima are recorded here to satisfy the “envelope and absolute Ld
differences recorded” acceptance element on the current tree; the governing
envelope itself is unchanged.

### 8.3 Test and artefact verification

| Check | Result |
|---|---|
| `cargo test -p ferrite-aac --lib fpe::` | **289 passed; 7 failed; 0 ignored** (395 filtered). The 7 are exactly the preserved pins listed in §5. |
| `cargo test -p ferrite-aac --lib` | **671 passed; 18 failed; 2 ignored** — exactly the documented set: 7 pins + 11 Tier-B/environmental blockers. |
| `cargo test -p ferrite-aac --lib production_baseline` | **2/2 passed**. |
| AC9SCHEMA 1 `shasum -a 256 -c SHA256SUMS` | **5/5 OK**. |
| Group E `shasum -a 256 -c SHA256SUMS` | **36/36 OK**. |
| `bash rust/tools/check-licensing.sh` | OK (NOTICE + LICENSE present; 94 `rust/src` notices; converter MIT/Apache clean). |
| `git diff --check -- docs rust` | Clean. |
| Default engine | `PsyEngineKind::default() == Q31`; constructor tests pass. |
| `cargo package --locked -p ferrite-aac --list` | Not completed: the working tree is dirty (pre-existing uncommitted work), exactly the FPE-3.2 F6 clean-state limitation. Not an M3 gate. |

### 8.4 Pre-existing hygiene observations (non-blocking for M3)

- `cargo fmt --check` reports formatting drift in pre-existing uncommitted test
  instrumentation (`fpe/tests/decision.rs`, `fpe/tests/xarch_log2.rs`,
  `fpe/tests/s4_step3.rs`, `scf.rs`, `threshold_vbr.rs`, and a few related
  files). CI includes a rustfmt job. This drift predates this pass and is not
  an M3 numerical or evidence defect; a formatting-only cleanup should be
  authorised before the eventual commit/CI run. No reformatting was performed
  here, to preserve the pre-existing working tree.
- FPE-3.2 F3 (`audit.rs` stale documentation labels) remains as recorded; it is
  documentation-accuracy only.
- The 11 Tier-B/environmental failures require external C artifacts/decoder or
  environment variables and remain blocked, not waived.

### 8.5 Behaviour-preservation evidence

- No production numerical file was touched by this pass; the only code paths
  exercised are the unchanged existing tests.
- Q31 remains the default and the production baseline is unchanged (2/2).
- The seven pin failures are byte-for-byte the documented AC-7 set; no new
  failure appeared.
- The AC-9 and Group E artefacts verify unchanged (5/5 and 36/36).

---

## 9. Remaining owner decisions

1. **Declare M3 complete.** This is the only remaining M3 exit action. It
   encompasses the formal FPE-3.1 component acceptance (energy envelope +
   downstream decision safety). No technical precondition remains. The R2.2
   acceptance for M3 is the collective effect of the Group C/D, AC-7, E2/E3,
   F-20/F-22/F-26 and AC-9/AC-10 owner decisions; the M3 boundary as written
   does not require a separate consolidated R2.2 record.
2. **Optional:** if the owner prefers a standalone FPE-3.1 acceptance record
   in addition to the M3 declaration, issue one; it is a presentation choice,
   not a technical blocker, and is not required by the M3 boundary as written.
3. **Not part of M3 (do not bundle into the M3 decision):** M4 authorisation
   and the A-13 seam decision; the deferred D-6/P-termination/OP-4/OD-3/A-09
   programme; N-2 at M8; A-5 at M9; Tier-B provisioning at M8/Stage-1; any
   rebaseline of the seven pins or the 187 S4 flips; any perceptual-equivalence
   claim.
4. **Repository hygiene (optional, before commit/CI):** authorise a
   formatting-only pass to clear the pre-existing `cargo fmt --check` drift.
   This is not an M3 acceptance criterion.

---

## 10. Recommendation

### **READY FOR M3 OWNER ACCEPTANCE**

Every technical and evidence requirement of the authoritative M3 boundary is
satisfied, with the documented limitations the owner has already accepted or
explicitly deferred. The remaining distance to formal M3 completion is **one
owner declaration**; there is no missing implementation, test, review or
evidence item that this assessment can identify as an M3 exit blocker.

**Exact remaining gap:** the owner’s formal M3 completion declaration (which
also settles the formal FPE-3.1 component acceptance). Nothing else is
required for M3.

**Not authorised by this assessment:** M3 completion, M4, any production or
test change, any threshold/tolerance/envelope/pin/schema change, any evidence
regeneration, any rebaseline, any perceptual-equivalence claim, or any change
to the nine conditional R2.1-B acceptances.

---

## Appendix A — Evidence index

- Kernel implementation: `rust/src/aac_enc/fpe/energy.rs`,
  `rust/src/aac_enc/fpe/orchestrate.rs`, `rust/src/aac_enc/fpe/output.rs`.
- Kernel tests: `rust/src/aac_enc/fpe/energy.rs` unit tests;
  `rust/src/aac_enc/fpe/tests/{audit,envelope,m2_stages,precision,f6_tns,probe_envelope}.rs`.
- R2.2 evidence: `rust/src/aac_enc/fpe/tests/{decision,r22_capture,trace,s4_step3,ac9,ac9_artifact,evidence,is_ms}.rs`;
  `docs/evidence/fpe-r22/`; `docs/evidence/fpe-r22-ac9/`.
- Acceptance records: `R2.1-B-owner-decision.md`, `a4-owner-decision.md`,
  `fpe-3.2-owner-decision.md`, `f6-owner-decision.md`,
  `a5-n2-owner-decision.md`, `R2.2-group-C-owner-acceptance.md`,
  `R2.2-group-D-owner-decision.md`, `R2.2-AC-7-divergence-characterization.md`
  §12, `R2.2-F20-F22-F26-numerical-owner-acceptance.md`,
  `R2.2-AC-9-owner-acceptance.md`, `R2.2-AC-10-independent-review.md`.
- Scope/boundary: `findings-register.md` §13; `R2.2-group-E2-E3-owner-decision.md`
  §1/§3/§5; `R2.2-AC-9-M3-scope-matrix.md`.

## Appendix B — Deferred-vs-blocker decision rule

A recorded finding is treated as an **M3 blocker** only if an authoritative M3
criterion (M3-C1…M3-C14) explicitly requires it and no owner decision defers it.
It is a **non-blocking limitation** if the requirement is satisfied but the
evidence carries an accepted limitation. It is **later phase** if an owner
decision explicitly assigns it to M8/M9, M4 entry, E6, FPE-default readiness,
or later R2.1-B conditional closure. Applying that rule yields the matrix in §3
and the blocker conclusion in §5.

---

## Dated addendum — M3 owner acceptance (28 November 2026)

*Appended without rewriting §§1–10 or the appendices above (assessment history
preserved). Authority:
[formal owner decision](M3-owner-acceptance.md) (28 November 2026).*

**FPE M3 is ACCEPTED / COMPLETE WITH DOCUMENTED LIMITATIONS.** The owner acted
on this assessment’s recommendation (§10) and formally accepted FPE-3.1,
FPE-3.2, FPE-3.3/A-4 and the R2.2 decision-safety work within the M3 boundary.
All authoritative M3 exit criteria are satisfied; no technical M3 blocker
remains.

This addendum supersedes, prospectively only, the assessment’s pre-decision
statements that M3 was not yet declared complete. The matrix, accepted
limitations, test exceptions, packaging/formatting observations and deferred
work above remain exactly as recorded. M4 is not authorised.
