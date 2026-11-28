# F-6 Owner Decision — Short-Window TNS / tnsSpecShift Structural-Unreachability Closure

**Authority:** Project owner (explicit decision issued 27 November 2026).
**Recorded by:** Muse Spark 1.3 Contributor (OpenCode), 27 November 2026.
**Disposition: ACCEPT WITH DOCUMENTED LIMITATIONS — F-6 is structurally
unreachable within the current production architecture. F-6 (A-14 / N-3)
is closed on the basis of the documented structural-unreachability
argument plus retained monitoring watches. No production source or test
changes authorized or required.**
**Baseline:** `189e6a79b78a84dcc5be88eaca5ffaba24097acc`
(= HEAD `189e6a7 feat(fpe): add R2.2 capture and replay infrastructure`).
**Finding:** F-6 / assessment A-14. **Validation roadmap item:** N-3.
**Scope:** Governance only — closure of F-6 (A-14 / N-3) on the documented
structural-unreachability argument and retained watches, with explicit
resolution of the `docs/fpe/07-validation-strategy.md` §5.2 construction
requirement.
Documentation-only implementation: this record plus the explicitly scoped
clarifications listed in §7. No production source, test, harness, manifest,
lockfile, dependency, CI workflow, ADR, numerical threshold, tolerance,
acceptance envelope, R2.2 evidence/schema, failure pin, fixture, or
M4-authorization change.

**Subject patch (documentation only; pre-existing working-tree work
preserved untouched, see §8):**

- New: `docs/fpe/assessment/f6-owner-decision.md` (this record).
- Clarified in place (surgical, history-preserving, §7):
  `docs/fpe/assessment/findings-register.md` (A-14 status),
  `docs/fpe/assessment/validation-roadmap.md` (N-3 dated clarification),
  `docs/fpe/07-validation-strategy.md` (F-6 bullet reachability-conditional
  clarification),
  `docs/fpe/assessment/implementation-roadmap.md` (dated clarification
  recording F-6 closure).

This is the formal record of the owner's decision. It is not a new technical
review and not an implementation report. Technical evidence lives in the
production implementation and the independent review cited below; nothing in
this record re-adjudicates that evidence beyond adopting the review verdict,
upgrades any conditional acceptance, closes any unrelated finding, or changes
any numerical contract.

Preserved sources (unchanged by this decision):

- Normative package: `docs/fpe/07-validation-strategy.md` (§5.2),
  `docs/fpe/05-floating-point-translation-spec.md` (§6.1, §6.2),
  `docs/fpe/09-implementation-roadmap.md` (M3),
  `docs/fpe/02-reuse-and-translation-inventory.md` (§1 #19–#20),
  `docs/fpe/03-fdk-reference-analysis.md` (§§4.9–4.10),
  `docs/fpe/04-system-architecture.md` (§§4, 6),
  `docs/fpe/06-encoder-integration.md` (§§4–6).
- Assessment package: `docs/fpe/assessment/findings-register.md` (A-14;
  A-12/A-11/A-20/A-21 context), `docs/fpe/assessment/validation-roadmap.md`
  (N-3), `docs/fpe/assessment/implementation-roadmap.md` (Phase 2),
  `docs/fpe/assessment/a4-owner-decision.md`,
  `docs/fpe/assessment/fpe-3.2-owner-decision.md` (FPE-3.2 finding F1
  context).
- Implementation (unchanged by this decision): `rust/src/aac_enc/psy.rs`
  (`tns_encode_channel`, `overflow_rescale`, D-022 preconditions),
  `rust/src/aac_enc/energy.rs` (`band_energy_short_into`,
  `band_energy_long_into`), `rust/src/aac_enc/tns.rs`,
  `rust/src/aac_enc/fpe/orchestrate.rs`, `rust/src/aac_enc/fpe/energy.rs`.
- Existing evidence (unchanged by this decision):
  `rust/src/aac_enc/fpe/tests/f6_tns.rs` (T1 production scan, T2 synthetic
  arithmetic), `rust/src/aac_enc/fpe/tests/audit.rs`
  (`short_tns_spec_shift_seen` watch),
  `rust/src/aac_enc/fpe/tests/differential.rs`,
  `rust/src/aac_enc/fpe/tests/probe_envelope.rs`,
  `rust/src/aac_enc/fpe/tests/precision.rs` (A-4 corpus shift coverage).
- Independent review: Mistral 4 adversarial review dated 27 November 2026, verdict
  **PASS WITH FINDINGS** (residual findings F6-R1…F6-R6, preserved in §4).
  No durable review artifact is retained in the repository; the review is
  cited by reviewer and date.

---

## 1. Decision — F-6 closed with documented limitations

**ACCEPT WITH DOCUMENTED LIMITATIONS.**

F-6 (short-window TNS with nonzero element-global `tnsSpecShift`) is
structurally unreachable within the current production architecture. F-6
(A-14 / N-3) is closed on the closure basis in §2, with the residual
findings in §4.

**Date and rationale.** Issued 27 November 2026, adopting the Mistral 4 independent
adversarial review dated 27 November 2026 (PASS WITH FINDINGS). The review
established the contribution-side impossibility as a code-path fact (the
short branch of `tns_encode_channel` contains no assignment to
`nrg_scaling_out` or `tns_spec_shift_global`; only the long branch can
contribute to the element-global shift), the per-element-per-frame zero
initialization of the global, stereo sequence agreement through block-switch
synchronization, and release-active D-022 rejection of mismatched CPE
sequences and LFE-with-Short configurations at both engine boundaries.
Consequently a short channel cannot receive a nonzero shift contributed by
a long partner within the current production architecture. Retained watches
detect and reject any future observation of the supposedly unreachable
condition. No production source or test changes are needed.

## 2. Closure basis (structural unreachability + retained watches)

1. The short branch of `psy::tns_encode_channel` (`psy.rs:1191-1200`)
   cannot write `nrg_scaling_out` or `tns_spec_shift_global`. Only the
   long branch (`psy.rs:1201-1213`) contributes to the element-global
   shift (`psy.rs:1211-1212`).
2. The global shift is initialized to zero per element and frame
   (`psy.rs:816-817`; FPE mirror `orchestrate.rs:414`).
3. Stereo block-switch synchronization assigns the same synchronized
   sequence to both channels (`block_switch.rs:409-443`).
4. Release-active D-022 assertions reject mismatched CPE sequences and
   LFE-with-Short configurations at both the Q31
   (`psy.rs:526-546`) and FPE (`orchestrate.rs:113-119`) engine
   boundaries.
5. Consequently, a short channel cannot receive a nonzero shift
   contributed by a long partner within the current production
   architecture (mono: single channel; stereo: sequences agree by
   synchronization and assertion; LFE: forced Long, TNS block skipped).
6. Retained watches detect and reject any future observation of the
   supposedly unreachable condition: `audit.rs`
   `short_tns_spec_shift_seen` (marks short-nonzero Ld unsupported, never
   passing), T1 production scan, `differential.rs` /
   `probe_envelope.rs` hard zero-asserts — all currently reporting 0.
7. T1/T2, the precision tests, D-022 rejection tests in debug and
   release, and the full-corpus differential audit were run successfully
   in the independent review.
8. The proof covers the current production mono/stereo architecture.
   Future multichannel orchestration requires re-verification (§4, F6-R3).
9. Upstream C `SyncBlockSwitching` source parity remains unverified, but
   the Rust proof relies on local synchronization and release-active
   enforcement rather than an upstream guarantee (§4, F6-R4).

## 3. Explicit resolution of the §5.2 construction requirement

`docs/fpe/07-validation-strategy.md` §5.2 literally requires constructing
a short element with nonzero element-global `tnsSpecShift`, while N-3
(`validation-roadmap.md:59`) already describes a synthetic test as
optional. This decision resolves the conflict explicitly; neither text is
silently waived or ignored:

- **The §5.2 construction requirement is reachability-conditional.** Where
  source-level structural proof establishes that the condition cannot
  occur within the supported production architecture, documented
  unreachability plus retained watches satisfies the requirement's
  intent. A separate synthetic short-element test is **not required**
  for F-6 closure.
- **The original validation intent is preserved** (see the §5.2
  clarification applied under this decision): if the condition is ever
  observed, it must be treated as unsupported/failed and investigated;
  the retained watches must continue to detect it. Reachability-conditional
  does not mean waived — it means the construction obligation is
  discharged by proof rather than by fabrication of an unreachable state.
- **T2's role is fixed:** the T2 synthetic reconstruction-arithmetic test
  (`f6_tns.rs:205-256`) is supporting evidence for the exponent bookkeeping
  (`m0 = m3+f−s` vs old `m3+f`, factor `4^s` / `2s/64`). It exercises audit
  helpers on explicitly non-production metadata — no `tns::detect`/`encode`,
  no `band_energy_long_into`, no `overflow_rescale` — and therefore is
  **not itself a production reachability test**. It is retained as
  arithmetic evidence; it is not cited as occurrence evidence.

## 4. Residual findings F6-R1…F6-R6 (severity-ranked)

- **F6-R1 — §5.2 governance conflict (RESOLVED by this decision).**
  The literal-construction reading of §5.2 conflicted with N-3's
  optional-synthetic framing. Resolved: construction is
  reachability-conditional (§3); the §5.2 text is clarified accordingly,
  not waived. No further action.
- **F6-R2 — Retained watches must continue (LOW, retained).** The closure
  depends in part on detection: `audit.rs` unsupported-marking, T1 scan,
  and corpus zero-asserts must remain in place. Any future observation of
  short-element nonzero shift is a defect-grade event (unsupported/failed,
  investigate). No watch may be removed without a superseding owner
  decision.
- **F6-R3 — Scope limited to current mono/stereo architecture (LOW,
  retained).** The exposure-side guarantee rests on per-element shift
  locals plus sequence agreement. Future multichannel orchestration must
  re-verify the invariant before any mixed-element processing is admitted.
  This decision does not pre-authorize multichannel reuse of the argument.
- **F6-R4 — Upstream C `SyncBlockSwitching` parity unverified
  (INFORMATIONAL, retained).** Not a premise of this Rust proof, which
  relies on local synchronization plus release-active enforcement.
  Recorded so no future reader mistakes the closure for a C-parity claim.
- **F6-R5 — T1 coverage limits (INFORMATIONAL, retained).** T1 exercises
  9 fixtures at one operating point, left-channel triggering, with no
  TNS-active-short floor. Adequate as a retained watch in combination
  with the structural proof; not expanded as a closure condition.
- **F6-R6 — Long-path nonzero shift constructible in principle,
  unobserved (INFORMATIONAL, retained).** The long-branch trigger
  (`pre[b] > 2^(2·maxScale[b]+23)`, requiring `maxScale ≤ 3`) is
  numerically reachable but has never fired on-corpus (FPE-3.2 finding F1
  context). Unrelated to short-side closure; tracked under existing
  synthetic-only coverage, not reopened here.

## 5. Explicit boundaries

- A-14 / N-3 are closed by this decision. No other finding is closed,
  narrowed, or reclassified.
- This decision does not close FPE-3.2's separately documented
  limitations (FPE-3.2 findings F1–F6), R2.2 decision safety, the
  `FpeError` taxonomy (A-20), perceptual validation, A-5/N-2, E2/E3,
  Group F, or any other open work.
- **M3 is not declared complete.** F-6 closure satisfies one M3-completion
  leg; the `FpeError` decision, R2.2 decision safety, perceptual
  conditions, and any other remaining M3 items stand as recorded.
- The N-2 Linux cross-architecture Ld leg remains an **M8 requirement**,
  not an M3 blocker, per the [A-5/N-2 owner decision](a5-n2-owner-decision.md),
  which explicitly establishes N-2 as an M8 requirement (reiterated in the
  FPE-3.2 owner decision). Nothing in this change reclassifies it.
- M4 and subsequent implementation phases are not authorized.

## 6. Authorization boundary — explicit non-authorizations

This decision does **not**:

- Authorize or require any production source change.
- Authorize or require any test, harness, fixture, or test-data change
  (including no new synthetic short-element test).
- Amend or reinterpret any ADR.
- Change any numerical threshold, tolerance, envelope, acceptance
  criterion, pin, or baseline.
- Upgrade, condition, or re-open any conditional acceptance.
- Declare M3 complete or authorize M4+ work.

## 7. Existing milestones and governance (preserved)

- All existing **G0–G9 definitions and their current status** are
  unchanged.
- All other **M3 dependencies and their existing status** beyond the F-6
  leg now closed (`FpeError` decision; R2.2 decision safety per A-08/OD-2;
  perceptual conditions per A-09; N-2 Linux leg as an M8 requirement) are
  unchanged.
- Existing **R2.2 decisions and limitations** are unchanged.
- The original **normative documents** are not rewritten in this
  increment, except for the surgical F-6 clarification of `07` §5.2
  described below (clarification only; no requirement weakened).

Assessment and normative clarifications applied under this decision
(surgical, history-preserving):

- Findings register (`docs/fpe/assessment/findings-register.md`): A-14
  status updated to Closed with reference to this record; historical
  description and rationale preserved; §8 owner-decision queue item 5
  marked decided; §7 blocker-map F-6 leg annotated as closed.
- Validation roadmap (`docs/fpe/assessment/validation-roadmap.md`):
  dated clarification that N-3 is closed by documented structural
  unreachability plus retained watches; the isolated synthetic test is
  optional and was not required for closure; link to this record.
- Normative strategy (`docs/fpe/07-validation-strategy.md`): F-6 bullet
  clarified as reachability-conditional (§3); observation intent
  (unsupported/failed on occurrence; watches continue) preserved.
- Implementation roadmap
  (`docs/fpe/assessment/implementation-roadmap.md`): dated clarification
  recording F-6 closure; Phase 2 text otherwise untouched; M3 completion
  not declared.

## 8. Recording integrity

- Branch at recording: `main`; HEAD `189e6a79b78a84dcc5be88eaca5ffaba24097acc`
  (`feat(fpe): add R2.2 capture and replay infrastructure`).
- Working-tree inventory at recording (before this decision's edits):
  15 modified + 14 untracked, all pre-existing and preserved
  byte-identically except for this decision's explicitly scoped
  documentation edits (§7):
  - Modified (15): `docs/fpe/assessment/R2.2-downstream-decision-safety-design.md`,
    `docs/fpe/assessment/{findings-register,implementation-roadmap,validation-roadmap}.md`,
    `rust/src/aac_enc/fpe/{energy,mod,orchestrate,output}.rs`,
    `rust/src/aac_enc/fpe/tests/{decision,is_ms,mod,r22_capture}.rs`,
    `rust/src/aac_enc/{rate,scf,testdata}.rs`.
  - Untracked (14): `docs/fpe/FPE-3.3-A-4-experiment-report.md`,
    `docs/fpe/assessment/{R2.2-group-C-owner-acceptance,R2.2-group-D-implementation-report,R2.2-group-D-owner-acceptance,R2.2-group-D-owner-decision,R2.2-group-E1-owner-decision,a4-owner-decision,a5-n2-owner-decision,fpe-3.2-owner-decision}.md`,
    `rust/src/aac_enc/fpe/tests/{evidence,precision,s4_step3,trace,xarch_log2}.rs`.
- New files under this decision: this record only (§7 items are edits to
  existing files).
- No Rust source, test, harness, ADR, threshold, tolerance, envelope, pin,
  baseline, fixture, or test-data file was modified by this recording
  action.
- All pre-existing working-tree modifications and untracked files were
  preserved without loss or reversion, except for the explicitly scoped §7
  documentation additions; no file was reverted, reformatted, renamed, or
  deleted.
- No `git add`, commit, push, amend, reset, rebase, cherry-pick, stash, or
  clean was performed.

---

## History

- 27 November 2026: owner decision **ACCEPT WITH DOCUMENTED LIMITATIONS**
  issued as recorded in §§1–6 (F-6 closed: structurally unreachable in
  current production mono/stereo architecture; closure basis structural
  argument + retained watches; §5.2 construction requirement resolved as
  reachability-conditional; residual findings F6-R1 resolved, F6-R2…F6-R6
  retained; no source/test changes; M3 not declared complete).
  Clarifications (§7) appended under this record.

**STOP — F-6 (A-14 / N-3) recorded as closed with documented limitations
(F6-R1 resolved; F6-R2…F6-R6 retained). Assessment and §5.2 clarifications
are appended under this record only. M3 completion, M4 entry, FPE-3.2
limitations, R2.2 decision safety, FpeError, A-5/N-2 (M8), perceptual
validation, E2/E3, Group F, and the seven existing FPE-suite failures
remain unauthorized, unclosed, and separately tracked by this record.
Awaiting separately scoped owner direction.**
