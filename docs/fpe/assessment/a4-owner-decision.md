# A-4 / FPE-3.3 Owner Decision — Numerical Precision Experiment Acceptance

**Authority:** Project owner (explicit decision issued 26 November 2026).
**Recorded by:** Muse Spark 1.3 Contributor (OpenCode), 26 November 2026.
**Disposition: ACCEPT WITH DOCUMENTED LIMITATIONS — the corrected A-4
experiment is accepted as a reproducible numerical precision experiment; no
production, numerical-contract, or unrelated-governance change.**
**Baseline:** `189e6a79b78a84dcc5be88eaca5ffaba24097acc`
(= HEAD `189e6a7 feat(fpe): add R2.2 capture and replay infrastructure`).
**Scope:** Governance only — acceptance of the corrected A-4 experiment
covering FPE-3.1 rescale-energy accumulation and FPE-3.2 TNS-recompute
energy accumulation (validation-roadmap N-1; findings-register A-12).
Documentation-only implementation: this record plus the explicitly scoped
report-accuracy corrections (§8 of the decision) and the dated assessment
clarifications listed in §7. No production source, test, harness, manifest,
lockfile, dependency, CI workflow, ADR, numerical threshold, tolerance,
acceptance envelope, R2.2 evidence/schema, or M4-authorization change.

**Subject patch (documentation only; pre-existing working-tree work
preserved untouched, see §9):**

- New: `docs/fpe/assessment/a4-owner-decision.md` (this record).
- Corrected (informational accuracy only, §8): `docs/fpe/FPE-3.3-A-4-experiment-report.md`
  §8 — clippy statement and exact failing-test names.
- Dated clarifications (history-preserving appends, §7):
  `docs/fpe/assessment/findings-register.md`,
  `docs/fpe/assessment/validation-roadmap.md`,
  `docs/fpe/assessment/implementation-roadmap.md`.

This is the formal record of the owner's decision. It is not a new technical
review and not an implementation report. Technical evidence lives in the
experiment report and harness cited below; nothing in this record
re-adjudicates that evidence, upgrades any conditional acceptance, closes any
unrelated finding, or changes any numerical contract.

Preserved sources (unchanged by this decision):

- Normative package: `docs/fpe/01-current-state-audit.md` (§9 A-4),
  `docs/fpe/05-floating-point-translation-spec.md` (§2, §9.7),
  `docs/fpe/07-validation-strategy.md` (§6), `docs/fpe/09-implementation-roadmap.md`
  (M3/FPE-3.3), `docs/fpe/m0-verification.md` (M0.5 A-4),
  `docs/fpe/10-risk-register.md` (R-18 context),
  `docs/fpe/adr/0002-floating-point-representation.md` (f64 decision, unchanged).
- Assessment package: `docs/fpe/assessment/validation-roadmap.md` (N-1),
  `docs/fpe/assessment/findings-register.md` (A-12; A-11/R-18/ASM-A5 unchanged),
  `docs/fpe/assessment/implementation-roadmap.md` (Phase 2).
- Experiment record: `docs/fpe/FPE-3.3-A-4-experiment-report.md` (corrected A-4
  run, F1–F7 remediation recorded in its §14).
- Experiment harness (test-only, pre-existing untracked work):
  `rust/src/aac_enc/fpe/tests/precision.rs` (plus its one-line
  `mod precision;` wiring in `rust/src/aac_enc/fpe/tests/mod.rs`).

---

## 1. Decision — A-4 accepted with documented limitations

**ACCEPT WITH DOCUMENTED LIMITATIONS.**

The corrected A-4 experiment is accepted as a reproducible numerical
precision experiment covering:

- FPE-3.1 rescale-energy accumulation.
- FPE-3.2 TNS-recompute energy accumulation.

The evidence supports retaining f64 as the approved Stage-1 implementation
(ADR-0002 unchanged, §4).

The f32 results remain diagnostic evidence for possible Stage-2 optimization
only. They do not authorize f32 production kernels or an ADR-0002 amendment
(§6).

**Date and rationale.** Issued 26 November 2026, following the independent
Mistral 4 re-review of the corrected run. The re-review reproduced the
primary numerical results and raised exactly two informational report-accuracy
findings (clippy statement; abbreviated failing-test names); both are
corrected under this decision (§8) without touching numerical results,
experiment logic, tolerances, or thresholds. The corrected corpus numbers
(§3) stay ≥32× inside the approved per-band envelopes with zero
over-envelope, guard, sentinel, or Class B flip events, and the two
structural f64 arguments (exact i32 boundary; f64 immunity to the reachable
f32 denormal range on sentinel-sensitive quiet bands) hold independently of
the measured deltas.

## 2. Evidence basis (as recorded, not re-adjudicated)

- **Independent review:** Mistral 4 re-review of the F1–F7-remediated run;
  primary numerical results reproduced exactly; two informational
  report-accuracy findings, both corrected in §8 of this decision. No
  numerical finding remains open within the F1–F7 plus re-review scope.
- **Corrected run (ARM64):** 27 fixtures, 2757 frames (long 2662 / short 95;
  TNS-active frames 216, of which short-TNS-active 43); 233,910 unique
  window-band cells; 467,820 combined kernel observations (rescale +
  TNS recompute, one observation per kernel per cell); 100% supported,
  0 unsupported, 0 non-finite, 0 mapping-saturated.
- **Headline deltas (f32-variant vs f64):** rescale max 338 stored LSB at
  eroica f261 ch1 w0 b18; TNS-long-active max 55; TNS-short-active max 33;
  TNS-short-inactive max 53; ~79% of bands bit-exact; ~99.4% within 64 LSB;
  maximum relative fraction error 5.5e-7; maximum |ΔLd| 1.24e-8; envelope
  utilization ≤ 0.031 everywhere (≥32× margin); 0 over-envelope, 0
  guard/sentinel flips, 0 Class B stored-value flips across all 467,820
  combined kernel observations.
- **Regression state at recording:** 8/8 precision tests pass (debug and
  release); `aac_enc::fpe::energy` 24/24 pass; `cargo test --lib fpe`
  276 passed / 7 failed, where the 7 failures are the pre-existing
  working-tree issues documented in §5 (pins moved by in-progress
  R2.2/FPE-3.2 changes; pristine-HEAD baseline not runnable without
  stashing, attribution by file/change ownership).

## 3. What this decision covers

- The A-4 numerical question only (validation-roadmap N-1): is f64
  empirically justified for the floated energy accumulations (not just
  analytically)? Answer recorded: yes — retained f64, evidence §§1–2.
- Both accumulation surfaces named in the owner direction: FPE-3.1
  rescale-energy accumulation and FPE-3.2 TNS-recompute energy
  accumulation, each under its approved per-band envelope.
- The f32 arms as diagnostic evidence: the f32-realistic arm and the
  f32-accumulator diagnostic stay on record as Stage-2-optimization input
  only (§6).

## 4. Preserved limitations (unchanged, travel with the acceptance)

- Single-architecture (ARM64) measurement; A-5 cross-arch remains open
  (f32 FTZ/denormal behaviour on other targets unmeasured; the f32
  denormal range ~1e-38 is reachable by quiet bands per `05` §2).
- `shiftBits` coverage is 0-only everywhere on-corpus (active-long max
  observed `shiftBits` 0 over 15,240 bands; inactive rows cover
  `shiftBits = 0` by construction); nonzero-shift behaviour unmeasured.
- Real-corpus Class A mining yielded 0; deterministic Class A coverage is
  synthetic (`sub_lsb_class_a_detected` unit test + audited `energy.rs`
  Class A tests).
- No saturation events on-corpus; no per-band histogram persisted beyond
  the 6-bucket summary; no evidence-bundle generation; no performance
  measurement (Stage-1 ~2.4%-self-time context stands).
- Stereo TNS sync uses the agreed-window configuration (fixtures comply
  with D-022); mixed-window sync behaviour is not an A-4 surface.
- The downstream numerical propagation numbers are sensitivity
  observations from a deliberately non-production-mirror probe (rescale
  basis only; production threads post-TNS/overflow energies into
  thresholds), not safety evidence — R2.2 owns decision safety (§5).

## 5. A-4 numerical evidence versus R2.2 decision-safety evidence

For the avoidance of doubt, this acceptance draws and preserves the
following boundary:

- **A-4 (this decision):** numerical error magnitudes of the energy
  accumulations against the approved per-band envelopes. It establishes
  no downstream decision outcome.
- **R2.2 (separate, not decided here):** whether any energy delta flips a
  downstream decision (PE/ATS/S2/S4/S6/frame accounting) and with what
  consequence. All R2.2 evidence gaps, E-schema conditions, Group E2/E3
  and Group F work, and the downstream legs of AC-4/AC-6/AC-10 remain
  exactly as currently recorded; no R2.2 finding is closed, narrowed, or
  reclassified by this decision.
- Understanding a numerical behaviour (envelope utilization, flip counts
  of zero on-corpus) does **not** demonstrate its perceptual
  acceptability. No perceptual-equivalence claim follows from this
  acceptance (P-1…P-4 outstanding per A-09).

## 6. Authorization boundary — explicit non-authorizations

This decision does **not**:

- Authorize any f32 production kernel. The f32 results are diagnostic
  evidence for possible Stage-2 optimization only.
- Amend or reinterpret ADR-0002. The f64 decision stands unchanged
  (verified: `docs/fpe/adr/0002-floating-point-representation.md`
  untouched).
- Establish perceptual equivalence, decision safety, or universal
  cross-architecture numerical equivalence.
- Authorize M4 or any subsequent implementation work, any production
  change, or any public-API/manifest/dependency change.
- Alter, re-pin, waive, or accept the seven existing FPE-suite failures
  (six `is_ms.rs` R2.1-B P0-B evidence tests plus
  `probe_envelope::g1_probe_corpus_aggregate`, §2); they remain separately
  tracked under their existing pins (cf. open item A-21) and are not
  resolved or accepted by this decision.
- Declare M3 completion. The A-4 evidence/acceptance leg is satisfied;
  FPE-3.2 acceptance, F-6 closure, the `FpeError` decision, R2.2 decision
  safety (A-08/OD-2), and the perceptual conditions (A-09) remain as
  currently recorded.
- Upgrade, condition, or re-open any other conditional acceptance
  (R2.1-B nine conditionals; F-20/F-22/F-26; Tier-B C parity stays
  blocked, not waived; G0 historical/non-rerunnable).

## 7. Existing milestones and governance (preserved)

- All existing **G0–G9 definitions and their current status** (including
  Tier-B G1/G5 blocked per A-10/OD-1; G8 satisfied for M2 only).
- All other **M3 dependencies and their existing status** (FPE-3.2
  acceptance, F-6, `FpeError` decision; R2.2 decision safety per
  A-08/OD-2; perceptual conditions per A-09).
- Existing **R2.2 decisions and limitations** (Group C accepted with
  documented limitations; Group D accepted with findings D-R1…D-R4; E1
  schema v2 freeze OD-3 with F-A…F-C; Groups E2/E3, Group F, and M4 entry
  remain unauthorized except under their own explicit instructions).
- The **A-5 / N-2 allocation** (`a5-n2-owner-decision.md`): N-2 partially
  complete and open pending Linux x86-64 evidence (additional M8
  requirement, not an M3 blocker); A-5 performance/denormal work stays
  at M9.
- The original **normative documents** (`01`, `05`, `07`, `09`,
  `m0-verification.md`, ADR-0002/ADR-0004) are **not modified** in this
  increment. The owner-decision record and the dated assessment
  clarifications below are the mechanism recording this acceptance.

Assessment-document clarifications appended under this decision (dated,
history-preserving, normative documents untouched):

- Assessment validation roadmap
  (`docs/fpe/assessment/validation-roadmap.md`): dated clarification
  referring to this record — N-1 (A-4) recorded and accepted with
  documented limitations; no longer awaiting independent review or owner
  acceptance; numerical leg only (R2.2 decision safety separate).
- Assessment findings register
  (`docs/fpe/assessment/findings-register.md`): dated update referring to
  this record — A-12 recorded and accepted with documented limitations;
  the §7 blocker-map A-4 leg clarified (M3 completion itself not
  declared; F-6, N-2 Linux leg, R2.2, and perceptual conditions
  unchanged).
- Assessment implementation roadmap
  (`docs/fpe/assessment/implementation-roadmap.md`): dated clarification
  referring to this record — Phase 2 scope item "FPE-3.3 A-4 experiment
  (N-1)" with acceptance "A-4/A-5 recorded": the A-4 leg is now recorded
  and accepted; no phase authorized; M3 completion not declared.

## 8. Report-accuracy corrections applied under this decision

Two informational inaccuracies in `docs/fpe/FPE-3.3-A-4-experiment-report.md`
§8, identified by the Mistral 4 re-review, are corrected here. Numerical
results, experiment logic, tolerances, thresholds, ADRs, and production
source are untouched:

- **Clippy:** the claim of no new clippy warnings from `precision.rs` is
  replaced with the precise statement that `cargo clippy` reports warnings
  in `precision.rs` (e.g. `needless_range_loop`, `unnecessary_cast`,
  `needless_lifetimes`) which are non-blocking and consistent with existing
  crate-wide lint patterns (cf. findings-register A-16; CI clippy is
  informational, non-gating).
- **Failing-test names:** the six `is_ms.rs` failures are now given by
  their exact test names
  (`r21b_p0b_aggregates_are_pinned`,
  `r21b_p0b_byte_baseline_is_pinned`,
  `r21b_p0b_canonical_evidence_is_emitted`,
  `r21b_p0b_per_finding_evidence_is_complete_and_traceable`,
  `r21b_p0b_stable_identifiers_are_unique_and_deterministic`,
  `r21b_p0b_unsupported_frame_is_explicit`;
  plus `probe_envelope::g1_probe_corpus_aggregate`), replacing the
  abbreviated forms.

## 9. Recording integrity

- Branch at recording: `main`; HEAD `189e6a79b78a84dcc5be88eaca5ffaba24097acc`
  (`feat(fpe): add R2.2 capture and replay infrastructure`).
- Working-tree inventory at recording (before this decision's edits):
  15 modified + 12 untracked, all pre-existing and preserved
  byte-identically except for this decision's explicitly scoped
  documentation edits (§§7–8):
  - Modified (15): `docs/fpe/assessment/R2.2-downstream-decision-safety-design.md`,
    `docs/fpe/assessment/{findings-register,implementation-roadmap,validation-roadmap}.md`,
    `rust/src/aac_enc/fpe/{energy,mod,orchestrate,output}.rs`,
    `rust/src/aac_enc/fpe/tests/{decision,is_ms,mod,r22_capture}.rs`,
    `rust/src/aac_enc/{rate,scf,testdata}.rs`.
  - Untracked (12): `docs/fpe/FPE-3.3-A-4-experiment-report.md`,
    `docs/fpe/assessment/{R2.2-group-C-owner-acceptance,R2.2-group-D-implementation-report,R2.2-group-D-owner-acceptance,R2.2-group-D-owner-decision,R2.2-group-E1-owner-decision,a5-n2-owner-decision}.md`,
    `rust/src/aac_enc/fpe/tests/{evidence,precision,s4_step3,trace,xarch_log2}.rs`.
- New files under this decision: this record only (§7 appends and §8
  corrections are edits to existing files).
- No production source, test, harness, manifest, lockfile, dependency, CI
  workflow, ADR, threshold, tolerance, envelope, R2.2 evidence/schema, pin,
  or M4-authorization file was modified by this recording action.
- All pre-existing working-tree modifications and untracked files were
  preserved byte-identically; no file was reverted, reformatted, renamed,
  or deleted.
- No `git add`, commit, push, amend, reset, rebase, cherry-pick, stash, or
  clean was performed.

---

## History

- 26 November 2026: owner decision **ACCEPT WITH DOCUMENTED LIMITATIONS**
  issued as recorded in §§1–6 (A-4 numerical precision evidence accepted;
  f64 retained as approved Stage-1 implementation; f32 diagnostic only;
  no perceptual, decision-safety, cross-architecture, ADR, production, M4,
  M3-completion, or failure-pin change). Report-accuracy corrections (§8)
  and assessment clarifications (§7) appended under this record.

**STOP — A-4 recorded as owner-accepted with documented limitations.
Assessment clarifications are appended under this record only. FPE-3.2
acceptance, F-6, FpeError, R2.2 decision safety, perceptual validation,
Group E2/E3, Group F, M4 entry, and the seven existing FPE-suite failures
remain unauthorized, unclosed, and separately tracked by this record.
Awaiting separately scoped owner direction.**
