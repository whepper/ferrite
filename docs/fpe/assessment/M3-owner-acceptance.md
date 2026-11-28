# FPE M3 — Formal Owner Acceptance

**Authority:** Project owner — explicit decision issued 28 November 2026.
**Recorded by:** OpenCode (recording only; this is the owner's decision).
**Disposition: FPE M3 — ACCEPTED / COMPLETE WITH DOCUMENTED LIMITATIONS.**
**Baseline:** `main` at HEAD `189e6a79b78a84dcc5be88eaca5ffaba24097acc`;
index empty at recording start.
**Scope:** Governance only — formal M3 milestone acceptance, register
reconciliation and history-preserving links. No production source, test,
fixture, manifest, lockfile, dependency, ADR, threshold, tolerance, envelope,
baseline pin, schema, or evidence-bundle change.

**Basis:**

- [`M3-exit-assessment.md`](M3-exit-assessment.md), including its complete
  exit matrix (§3), blocker analysis (§5), validation results (§8) and
  recommendation (§10).
- The authoritative M3 criteria in
  [`../09-implementation-roadmap.md`](../09-implementation-roadmap.md) §M3 and
  [`../07-validation-strategy.md`](../07-validation-strategy.md) §5.2/§8.
- The controlling M3 boundary in
  [`findings-register.md`](findings-register.md) §13 and
  [`R2.2-group-E2-E3-owner-decision.md`](R2.2-group-E2-E3-owner-decision.md)
  §1/§3/§5.
- The previously recorded component owner decisions:
  [`a4-owner-decision.md`](a4-owner-decision.md),
  [`fpe-3.2-owner-decision.md`](fpe-3.2-owner-decision.md),
  [`f6-owner-decision.md`](f6-owner-decision.md), and the R2.2 acceptances.
- [`R2.2-AC-9-owner-acceptance.md`](R2.2-AC-9-owner-acceptance.md)
  (ACCEPT WITH DOCUMENTED LIMITATIONS) and
  [`R2.2-AC-10-independent-review.md`](R2.2-AC-10-independent-review.md)
  (PASS WITH FINDINGS; no M3 acceptance blocker).

---

## 1. Decision

**FPE M3 is ACCEPTED and COMPLETE WITH DOCUMENTED LIMITATIONS**, effective on
this record.

The owner formally accepts, as M3 deliverables:

1. **FPE-3.1 — float rescale band energy.** The fraction-domain long/short
   energy kernel, boundary rounding and §6.8 Ld conventions, with the
   owner-approved FPE-3.1R1 derived envelope. The G-1 energy-envelope
   adjudication (R2.1-B owner decision §2.1) and the downstream
   decision-safety leg (AC-9/AC-10) are accepted within the M3 boundary.
2. **FPE-3.2 — float TNS energy recompute.** Long-window `shiftBits`, the
   short no-Ld quirk, and the overflow rescale integration, accepted with
   documented limitations F1–F6 in
   [`fpe-3.2-owner-decision.md`](fpe-3.2-owner-decision.md).
3. **FPE-3.3 / A-4 — f32-vs-f64 experiment.** The corrected corpus run,
   recorded envelope, and retention of f64 as the approved Stage-1
   implementation, accepted with documented limitations in
   [`a4-owner-decision.md`](a4-owner-decision.md).
4. **R2.2 decision-safety work within the M3 boundary.** The D-1…D-5/D-7/D-8
   evidence, the AC-9 M3-mandatory families, the Group C/D operating-point,
   replay, margin and attribution work, the AC-7 divergence characterization,
   and the F-20/F-22/F-26 numerical per-finding evidence, accepted through the
   component owner decisions and the AC-9/AC-10 disposition.

In addition, the M3-required **F-6 closure** and **FpeError/A-20 decision** are
accepted as recorded in
[`f6-owner-decision.md`](f6-owner-decision.md) and
[`R2.2-group-E2-E3-owner-decision.md`](R2.2-group-E2-E3-owner-decision.md) §3.

**All authoritative M3 exit criteria are satisfied. No technical M3 blocker
remains.** The detailed requirement-by-requirement mapping is
[M3-exit-assessment.md](M3-exit-assessment.md) §3 (M3-01…M3-15).

This decision supersedes, **prospectively only**, the historical statements
that “M3 is not declared complete” in the assessment records. Those statements
remain accurate as history for their dates and are not rewritten. The M3 exit
assessment remains the evidence record; this document is the formal owner
acceptance.

---

## 2. Evidence accepted

| Area | Accepted evidence |
|---|---|
| FPE-3.1 energy kernel | `rust/src/aac_enc/fpe/energy.rs` `rescale_band_energy_into`; `energy::tests::rescale_*`; G-1 validation report; R2.1-B owner decision §2.1 |
| FPE-3.2 TNS recompute | `tns_recompute_energy_into`; `energy::tests::tns_*`; Mistral 4 review; FPE-3.2 owner decision |
| FPE-3.3 / A-4 | `docs/fpe/FPE-3.3-A-4-experiment-report.md`; `precision` tests; A-4 owner decision |
| F-6 | Structural-unreachability closure plus retained watches; F-6 owner decision; `f6_tns` 2/2; audit F-6 observations 0 |
| R2.2 decision safety | Group C/D acceptances; AC-7 owner decision; E2/E3 acceptance; AC-9 scope matrix + `AC9SCHEMA 1` artifact; AC-10 PASS WITH FINDINGS |
| F-20/F-22/F-26 | Six Group E replay rows; numerical owner acceptance |
| FpeError/A-20 | E2/E3 owner decision §3: M3 decision recorded; implementation deferred |
| Numerical review | Qwen G-1; DeepSeek R2.1-B numerical review; Mistral 4 FPE-3.2; Mistral 4 A-4 re-review; AC-10 |

The current-tree validation recorded in
[M3-exit-assessment.md](M3-exit-assessment.md) §8 remains the acceptance
evidence:

- full library suite **671 passed / 18 failed / 2 ignored**;
- Q31 production baseline **2/2**;
- AC9SCHEMA 1 hashes **5/5**; Group E hashes **36/36**;
- licensing OK; `git diff --check` clean;
- current-tree energy audit: zero energy/value/Ld/identity/unsupported
  failures; maxima and Ld differences recorded;
- Q31 remains the default engine.

---

## 3. Preserved limitations and open findings

This acceptance is **milestone-level** and does not close, waive, rebaseline or
reclassify any finding beyond what the component decisions already recorded.

### 3.1 Known test exceptions (not resolved by this acceptance)

The 18 library-test failures remain exactly the documented set. They are
accepted as known limitations of the M3 state, not as passes:

**Seven preserved FPE pins (no rebaseline authorised):**

1. `r21b_p0b_aggregates_are_pinned`
2. `r21b_p0b_byte_baseline_is_pinned`
3. `r21b_p0b_canonical_evidence_is_emitted`
4. `r21b_p0b_per_finding_evidence_is_complete_and_traceable`
5. `r21b_p0b_stable_identifiers_are_unique_and_deterministic`
6. `r21b_p0b_unsupported_frame_is_explicit`
7. `probe_envelope::g1_probe_corpus_aggregate`

These are the deterministic consequences of the accepted FPE-3.2 change,
characterized in AC-7 (agree 357/365; 129 findings; grouped-B 47/3; etc.), and
remain observed divergences under their existing pins. **No rebaseline or
replacement pin is authorised.**

**Eleven Tier-B/environmental blockers (external artifacts/decoder or opt-in
environment):**

1. `m5a_decode_check`
2. `m55_streams_match_c_reference`
3. `phase1_frontend_agreement_sine_f5`
4. `phase1_full_stream_parity`
5. `phase1e_onset_f24_bands_causality`
6. `phase1e_sine_f5_band16_residue`
7. `phase1f_multitone_f1_no_live_divergence`
8. `phase1f_stepdown_f1_no_live_divergence`
9. `phase1g_long_flips_residue`
10. `phase1i_multitone_f5_tier_b`
11. `phase1i_sine44_f5_tier_b`

These remain **blocked, not waived**. Tier-B C parity and the decoder remain
M8/Stage-1 work.

**Packaging and formatting limitations (not M3 acceptance criteria, not
resolved):**

- `cargo package --locked -p ferrite-aac --list` cannot complete on the
  uncommitted working tree — the known FPE-3.2 F6 clean-state packaging
  limitation. It is not treated as satisfied.
- `cargo fmt --check` reports pre-existing formatting drift in the
  uncommitted test instrumentation. CI includes a rustfmt job. A
  formatting-only cleanup remains outstanding for the eventual commit/CI
  pass; no reformatting was performed here.

### 3.2 Findings preserved

- **AC10-F1…F8** remain visible with their AC-10 severity and classification.
  None is closed, waived or reclassified.
- The **nine R2.1-B conditional acceptances** (F-07/09/10/15/17/18 and
  F-20/22/26) remain conditional. The F-20/F-22/F-26 **numerical** evidence is
  accepted; their **perceptual** conditions remain open.
- **A-08** remains “resolved for the M3 boundary” with the deferred
  conditional-closure legs (D-6/P-termination, OP-4, OD-3 five-frame, A-09)
  still open outside M3.
- **A-20** remains “M3 decision satisfied; implementation deferred” to
  FPE-default readiness or separately authorised scope.
- **A-09** (perceptual), **A-10** (Tier-B), **A-11/N-2** (M8), **A-13**
  (M4 entry), **A-21/OD-6**, **A-22/A-15** and **R22-E23-08** remain open
  exactly as recorded; **D-R2** remains accepted with its documented
  limitation.
- **187 S4 flips** remain unjudged; no harmlessness or perceptual-neutrality
  claim is made.
- Frozen **E1 schema v2**, **MDEF v2**, the accepted **Group E** bundle
  (36/36) and **AC9SCHEMA 1** (5/5) are unchanged. **D-R2** remains accepted
  with its documented limitation.

### 3.3 Deferred work (outside M3; unchanged)

D-6 recurrence / P-cascade termination; the OP-4 recurrence campaign; the
retained five-frame OD-3 boundary evidence; D-10 quiet short-window
sentinel/PE closure; A-09 perceptual validation; the nine R2.1-B conditionals;
N-2 Linux x86-64 measurement (M8); A-5 performance/denormal work (M9);
Tier-B C parity and decoder (M8/Stage-1); A-13 seam decision (M4 entry);
SG-2 clean-checkout / E6; FpeError production implementation; MS/M7 downstream
and D-9; AC10-F1…F8 later-phase recommendations; A-21/OD-6 and A-22/A-15.

---

## 4. Boundaries

This acceptance does **not**:

- Change production code, tests, fixtures, manifests, lockfiles, dependencies,
  ADRs, thresholds, tolerances, envelopes, baseline pins, schemas or evidence
  bundles.
- Rebaseline the seven preserved pin failures or the 187 unjudged S4 flips.
- Modify or regenerate the accepted Group E or AC9SCHEMA 1 artifacts.
- Close, waive or reclassify any deferred or conditional finding merely
  because M3 is complete.
- Establish perceptual equivalence or perceptual acceptability.
- Authorise M4 or any subsequent implementation phase. **M4 requires a
  separate authorisation and planning decision.**
- Authorise a default-engine change, Stage-2 work, or any f32 production
  kernel.

No file was staged, committed, pushed, reset, rebased, stashed or cleaned by
this recording action. All pre-existing working-tree changes are preserved.

---

## 5. Register updates recorded with this decision

History-preserving links/notes were appended to:

1. [`findings-register.md`](findings-register.md) §22 — M3 owner acceptance;
   A-08/A-20 remain at their reconciled statuses; deferred findings unchanged.
2. [`implementation-roadmap.md`](implementation-roadmap.md) — dated M3
   acceptance clarification; Phase 2/M3 completion recorded prospectively.
3. [`validation-roadmap.md`](validation-roadmap.md) — dated M3 acceptance
   clarification; the M3 decision-safety gate row is satisfied for M3.
4. [`R2.2-findings-register.md`](R2.2-findings-register.md) — History line
   recording M3 acceptance without closing any R22 finding.
5. [`M3-exit-assessment.md`](M3-exit-assessment.md) — dated addendum linking
   this owner decision.

No historical owner decision, AC-10 review or component acceptance record was
rewritten.

---

## 6. Recording integrity

- Branch: `main`. HEAD: `189e6a79b78a84dcc5be88eaca5ffaba24097acc`.
- Index: empty at recording start and after this recording.
- Working-tree inventory before this recording: 19 modified + 30 untracked
  entries (the pre-existing M3/R2.2 set plus the M3 exit assessment).
- This recording adds the new owner-acceptance file and appends the register
  notes listed in §5; no new production, test or evidence file is added or
  modified.
- `git diff --check` is clean.
- Group E `SHA256SUMS` 36/36 and AC9SCHEMA 1 `SHA256SUMS` 5/5 remain valid;
  neither bundle was touched.
- No commit, stage, push, reset, rebase, stash or clean was performed.

---

## 7. Effect

**FPE M3 is ACCEPTED / COMPLETE WITH DOCUMENTED LIMITATIONS.**

The milestone is complete for the M3 boundary. The deferred findings,
conditional acceptances, test exceptions, packaging/formatting limitations and
later-phase work listed in §3 remain exactly as documented. **M4 is not
authorised by this decision and must be handled as a separate authorisation
and planning decision.**

**STOP — M3 accepted; limitations preserved; M4 not authorised.**
