# A-5 / N-2 Owner Decision — Milestone-Allocation Resolution

**Authority:** Project owner (explicit decision issued 26 November 2026).
**Recorded by:** Muse Spark 1.3 Contributor (OpenCode), 26 November 2026.
**Disposition: DECIDED — allocation resolved as below; no production,
numerical, or unrelated-governance change.**
**Baseline:** `189e6a79b78a84dcc5be88eaca5ffaba24097acc`
(= HEAD `189e6a7 feat(fpe): add R2.2 capture and replay infrastructure`).
**Scope:** Governance only — the A-5 / N-2 milestone-allocation ambiguity
(see §1). Documentation-only implementation: this record plus the
explicitly scoped dated clarifications in the three assessment documents
listed in §7. No production source, test, harness, manifest, lockfile,
dependency, CI workflow, ADR, numerical threshold, tolerance, acceptance
envelope, R2.2 evidence/schema, or M4-authorization change.

**Subject patch:** none. No code patch is recorded under this decision; the
working tree at recording time carries only pre-existing modifications and
untracked files, preserved untouched (see §8).

This is the formal record of the owner's decision. It is not a new technical
review and not an implementation report. Technical evidence lives in the
harness and reports cited below; nothing in this record re-adjudicates that
evidence, upgrades any acceptance, closes any unrelated finding, or changes
any numerical contract.

Preserved sources (unchanged by this decision):

- Normative package: `docs/fpe/01-current-state-audit.md` (§9 A-5),
  `docs/fpe/05-floating-point-translation-spec.md` (§2, §9.7),
  `docs/fpe/09-implementation-roadmap.md` (M3/M8/M9), `docs/fpe/m0-verification.md`
  (M0.5 A-5), `docs/fpe/08-performance-strategy.md` (§3.1),
  `docs/fpe/10-risk-register.md` (R-18), `docs/fpe/adr/0004-differential-validation.md`.
- Cross-architecture harness (test-only, pre-existing untracked work):
  `rust/src/aac_enc/fpe/tests/xarch_log2.rs` (Layers A/B/C, coverage matrix,
  environment scoping).
- Assessment package: `docs/fpe/assessment/validation-roadmap.md` (N-2),
  `docs/fpe/assessment/findings-register.md` (A-11, R-18, ASM-A5),
  `docs/fpe/assessment/implementation-roadmap.md` (Phase 2).

---

## 1. Ambiguity being resolved

A-5 ("x86-64 performance/denormal behaviour", `01` §9; "x86-64 performance /
denormal behaviour", `m0-verification.md` M0.5; cross-arch `log2`/Ld
measurement, `05` §9.7) couples two distinguishable concerns that the
assessment package allocates inconsistently:

- the **numerical** cross-architecture `log2`/Ld comparison (validation-roadmap
  **N-2**: "max `|ld_ARM64 − ld_x86-64|` over corpus … Is the platform `log2`
  difference inside the envelope … | M8 cross-platform"), and
- the **performance/denormal-behaviour** investigation on x86 hardware or CI
  (`01` §9: "M9 on available x86 hardware or CI"; `m0-verification.md` M0.5:
  "Deferred to M9"; `08` §3.1: x86-64 reported separately, never pooled).

N-2 itself is allocated inconsistently: validation-roadmap N-2 names the
**M8 cross-platform** gate, while the findings-register blocker map and the
Phase 2 (M3 completion) scope place "A-4 + A-5 … ─► M3 completion"
(findings-register §7; implementation-roadmap Phase 2: "A-5 cross-arch Ld
measurement (N-2)" inside M3 completion with acceptance "A-4/A-5 recorded").
This decision adopts the split proposed in the independent adversarial
governance review, with the clarifications below, and resolves the ambiguity
without changing any numerical contract.

## 2. Decision — A-5 split

The original A-5 scope is split into two distinguishable concerns:

1. **Numerical `log2`/Ld comparison (→ N-2).** The retained macOS ARM64 versus
   macOS x86-64-under-Rosetta evidence (see §4) is accepted as **bounded
   evidence for the numerical `log2`/Ld comparison covered by N-2**, within
   its documented scope only (§4–§5).
2. **Performance/denormal-behaviour investigation (→ M9).** The original A-5
   performance/denormal-behaviour investigation **remains allocated to M9**,
   unchanged (`01` §9; `m0-verification.md` M0.5; `08` §3.1). Nothing in this
   decision advances, narrows, or closes that M9 work.

This decision does **not** imply that the retained evidence establishes
native x86-64 hardware behaviour, Linux/glibc behaviour, or universal
cross-platform equivalence (see §5).

## 3. Decision — N-2 disposition

- **N-2 is partially complete.** The currently retained macOS evidence (§4)
  is accepted within its documented scope.
- **Linux x86-64 CI measurement remains open** (see §6 for the completion
  protocol).
- **N-2 is not an M3 completion blocker.** The Phase 2 / blocker-map lines
  placing A-5/N-2 on the M3-completion path are clarified accordingly (see
  §7); M3 completion does not require the Linux measurement.
- **N-2 becomes an additional M8 cross-platform milestone requirement,
  established explicitly by this owner decision.** For the avoidance of
  doubt: this M8 requirement is an **explicit owner-established addition**
  made by this record. It is **not** claimed to have already been an
  unambiguous requirement in the original normative M8 roadmap (`09` M8
  lists G1–G5, G7 full differential validation; the cross-platform bound is
  added here as an M8 requirement, not read back into `09` as pre-existing).
- **N-2 remains open** until the Linux x86-64 comparison has been executed,
  reviewed, and its evidence retained per §6.

## 4. Existing evidence and its limitations (as recorded)

The retained macOS ARM64 versus macOS x86-64-under-Rosetta comparison, from
the `xarch_log2` harness revision present in the working tree, is recorded
with the following results:

- **Layer A:** 352/352 comparable outputs bit-identical (identical-input
  `f64::log2` / `ld_data` comparison across the eight production call sites;
  NaN-input and NaN-output rows separated per the harness NaN rule and
  excluded from the comparable count).
- **Layer B:** 658/658 observations identical (corpus capture across the
  documented fixtures/frames, joined on the stable observation key, including
  the retained TNS-long recompute, MS-Ld, and grouped short-block production
  diagnostics).
- **Layer C:** 49/49 boundary probes identical (deterministic probes around
  the exact source-defined TNS-guard, `shiftBits`, `max_nrg_for_shift`,
  sentinel, `q31_of_ld`-mirror, threshold-floor, and grouped-Ld expressions).
- **The maximum observed absolute Ld difference was zero** across the
  retained comparable observations in the documented corpus and boundary
  probes.

The last statement is an **observation about the tested material**, not a
universal mathematical or platform-wide bound. No tolerance is inferred from
the observed zero difference (see §6 of this record and `05` §9 / ADR-0004).

The following limitations are preserved and travel with the accepted
evidence; the evidence is **not** presented as general cross-platform
equivalence:

- x86-64 was executed **under Rosetta 2 on the same Apple Silicon host**,
  not on native x86-64 hardware.
- **Native x86-64 hardware has not been tested.**
- **Linux/glibc has not been tested.**
- **MXCSR/FPCR state was not directly recorded** (the harness records what
  it can observe and names the rest as limitations; subnormal inputs produced
  finite non-zero outputs on both tested environments, which is recorded as
  an observation, not a register inspection).
- **TNS-long diagnostics use unfiltered-spectrum invocations** (read-only
  production kernels on corpus-derived inputs; post-TNS-filtered spectra are
  unobserved — a stated harness limitation).
- **Float FPE MS energies remain outstanding** (integer-lane MS-Ld only;
  FPE-7.1) **and some complete production threshold paths remain
  unobserved** (grouped threshold Ld uses init-only thresholds as a labelled
  simplified-threshold diagnostic).

## 5. Linux x86-64 completion protocol

The following defines what remains necessary to close N-2. A Linux CI run is
**not** sufficient merely because the existing harness executes
successfully. The comparison must:

1. Use the **same harness revision** and **identical inputs** as the retained
   ARM64 reference capture.
2. **Preserve the ARM64 reference capture** and make the comparison
   **reproducible** (reference capture retained byte-identically; join keys
   and corpus pins as defined by the harness).
3. Execute the corresponding measurement on the **existing Linux x86-64 CI
   environment**.
4. **Retain relevant environment and runtime metadata** (architecture, OS,
   toolchain/build flags, library/runtime versions, and the harness-declared
   limitations for anything not directly observable).
5. Compare **identical-input `log2` results separately** from upstream energy
   or input differences (upstream-input differences mask downstream ones and
   must be classified as such, never reported as pure-`log2` variation).
6. **Report Ld differences and relevant guard/sentinel changes separately**
   (absolute Ld differences reported; guard/sentinel branch outcomes reported
   as their own class).
7. **Attribute observed differences to their relevant numerical stages where
   possible** (primitive `log2` vs Ld guard/sentinel vs integer conversion vs
   extended diagnostics, in pipeline order).
8. **Retain machine-readable results and a concise human-readable report**
   (captures plus comparison summary, committed or permanently linked per the
   assessment evidence-retention policy).

This protocol describes a comparison **between the tested ARM64 and Linux
x86-64 environments**. It does **not** claim to isolate CPU-architecture
effects from operating-system, runtime, or library effects.

The existing harness and evidence format are to be used where practical. No
new production dependency is introduced and no production implementation
scope is expanded by this protocol.

## 6. Numerical contract and ADR-0004 (unchanged)

This decision introduces **no new universal Ld threshold or ULP acceptance
criterion**. The existing numerical contracts govern:

- Existing energy-domain interpretation and applicable per-band envelopes
  (`05` §9; ADR-0004 amendment FPE-3.1R1) remain authoritative.
- Existing stored-Q31 and energy-domain contracts remain unchanged
  (`q31_tol` for stored integers; energy-domain envelope where applicable).
- **Ld absolute differences must be reported separately** (never folded into
  an integer tolerance).
- **Guard/sentinel changes must be investigated separately** (branch-outcome
  class, not a magnitude statistic).
- **No new tolerance may be inferred from the observed zero difference**
  (§4 reports a measurement; it establishes no bound).

> No existing numerical validation envelope, tolerance, or G0–G9 gate is
> changed. This decision explicitly establishes N-2 as an additional M8
> cross-platform milestone requirement.

No ADR-0004 amendment is required: the change is limited to milestone
allocation and does not change any existing validation envelope, tolerance,
or acceptance criterion. **ADR-0004 is not modified by this decision**
(verified: `docs/fpe/adr/0004-differential-validation.md` untouched).

## 7. Existing milestones and governance (preserved)

This decision explicitly preserves, and does not silently accept, close,
waive, or reclassify, any of the following:

- All existing **G0–G9 definitions and their current status** (including G0
  historical/non-rerunnable per A-01; Tier-B G1/G5 blocked per A-10/OD-1;
  G8 satisfied for M2 only).
- All other **M3 dependencies and their existing owner-approved status**
  (FPE-3.2, FPE-3.3/A-4, F-6, FpeError decision; R2.2 decision safety per
  A-08/OD-2; perceptual conditions per A-09).
- Existing **R2.2 decisions and limitations** (Group C accepted with
  documented limitations; Group D accepted with findings D-R1…D-R4; E1 schema
  freeze OD-3 with F-A…F-C; Groups E2/E3, Group F, and M4 entry remain
  unauthorized except under their own explicit instructions).
- **Tier-B C parity status and its existing blockers** (blocked, not waived;
  A-10/OD-1; A-3 residual gap unchanged).
- Existing **M4 authorization restrictions** (M4 entry only under its own
  authorized instruction; A-13 seam decision still required at M4 entry).
- The original **A-5 performance/denormal allocation to M9** (§2).

Assessment-document clarifications appended under this decision (dated,
history-preserving, normative documents untouched):

- Assessment validation roadmap (`docs/fpe/assessment/validation-roadmap.md`):
  dated clarification referring to this record — N-2 partially complete and
  open pending Linux x86-64 evidence; not an M3 blocker; additional M8
  requirement; A-5 performance/denormal work stays at M9.
- Assessment findings register (`docs/fpe/assessment/findings-register.md`):
  dated update referring to this record — A-11/R-18/ASM-A5 status notes and
  the §7 blocker-map line placing A-5 on the M3 path, clarified without
  rewriting history.
- Assessment implementation roadmap
  (`docs/fpe/assessment/implementation-roadmap.md`): dated clarification
  referring to this record — Phase 2 scope note clarifying N-2 is not an M3
  blocker and is carried as an additional M8 requirement.

The original normative documents (`01`, `05`, `09`, `m0-verification`) are
**not modified** in this increment. The owner-decision record and the dated
assessment clarifications are the mechanism resolving the allocation
ambiguity.

## 8. Recording integrity

- No production source, test, harness, manifest, lockfile, dependency, CI
  workflow, ADR, threshold, tolerance, envelope, R2.2 evidence/schema, or M4
  authorization file was modified by this recording action; the only new file
  is this record, and the only modifications are the three dated assessment
  clarifications in §7 (verified via `git status` / `git diff` before
  reporting).
- All pre-existing working-tree modifications and untracked files were
  preserved byte-identically; no file was overwritten, reverted, reformatted,
  renamed, or deleted.
- No `git add`, commit, push, amend, reset, rebase, cherry-pick, stash, or
  clean was performed.

---

## History

- 26 November 2026: owner decision **A-5 / N-2 allocation resolved** as
  recorded in §§2–7 (N-2 partially complete; macOS Rosetta-bounded evidence
  accepted within scope; Linux x86-64 measurement open; N-2 not an M3
  blocker; N-2 an additional M8 requirement established by this record; A-5
  performance/denormal work remains at M9). No envelope, tolerance, gate, or
  unrelated milestone/finding altered.

**STOP — A-5/N-2 allocation is resolved as above. Assessment clarifications
are appended under this record only. Linux x86-64 evidence, M4 entry, Group
E2/E3, Group F, and any production change remain unauthorized by this record.
Awaiting separately scoped owner direction.**
