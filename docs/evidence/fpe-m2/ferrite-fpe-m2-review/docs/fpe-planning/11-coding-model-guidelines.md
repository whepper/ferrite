# 11 — Coding Model Delegation Guidelines

Date: 2026-11-16. Ferrite uses multiple coding models. This package is written
so a model can implement an approved task **without inventing algorithms,
changing FDK's psychoacoustic design, or making major architectural
decisions**. Models implement approved specifications; they do not redesign.

## 1. Ground rules (every task)

1. **Scope discipline**: implement exactly the task spec. No unrelated
   refactoring, no "improvements", no Stage-2 items (pure-float headroom,
   float-recomputed factors, SIMD, model changes).
2. **Q31 is read-only**: do not modify Q31 behaviour, signatures, or layout.
   The only permitted Q31-side edit is an additive `pub(crate)` visibility bump
   explicitly named in the task. If you believe a Q31 change is needed, STOP
   and escalate.
3. **Default unchanged**: FPE is selectable, never default. Existing
   constructors and behaviour unchanged.
4. **No architectural decisions**: module layout, dispatch, precision,
   interface, and validation approach are fixed by `04`/`05`/`06`/`07` and the
   ADRs. If a task seems to require a new decision, STOP and escalate with the
   specific question and options — do not choose silently.
5. **No commits/pushes** without owner instruction. Produce a focused diff.
6. **Numerical authority**: `03` (reference) + `05` (translation) + the Q31
   mirror are the ground truth. Do not derive psychoacoustic behaviour from
   general knowledge or other codecs.
7. **Licensing**: FPE retains the **existing FDK-AAC licence** — no separate
   licence, no change to the SPDX identifier, licence files, or NOTICE
   (`12` §0). New FPE files carry the TPMV notice per `12`; do not regenerate
   committed tables or copy upstream code ad hoc. Run
   `rust/tools/check-licensing.sh` and `git diff --check` before reporting.
8. **Tests required**: every task adds/updates the tests named in its spec;
   all existing Tier-A tests must still pass.

## 2. Task specification template

Each delegated task (IDs `FPE-<milestone>.<n>` from `09`) is issued with:

```
TASK FPE-x.y — <title>
MILESTONE: Mx
SCOPE: <exact behaviour to implement, one kernel group / seam>
FILES TO CHANGE: <explicit paths; NEW vs edit>
FILES READ-ONLY: <Q31 + downstream files that must not change>
REFERENCE:
  - upstream: <cpp file:function:lines from 03>
  - Q31 mirror: <rust file:lines>
  - spec: <05 §x.y formula>
EXPECTED CHANGE: <precise description; inputs/outputs; domains>
NON-GOALS: <explicit exclusions for this task>
REQUIRED TESTS: <unit + differential; named test fns; corpus>
ACCEPTANCE: <measurable gates; envelopes; exact fields>
INVARIANTS: <05 §8 checks relevant here>
REPORT FORMAT: <see §4>
ESCALATE IF: <conditions requiring a stop>
```

## 3. Escalation conditions (STOP, do not proceed)

- A Q31 behaviour change appears necessary.
- An architectural decision not covered by the ADRs is required.
- A measured deviation exceeds the stated envelope, or a new decision flip
  appears (do not widen tolerances or force branches to make tests pass).
- The reference checkout is unavailable for a Tier-B task (fall back to the
  in-binary harness; report the gap).
- A licensing/provenance question arises (do not copy upstream code ad hoc).
- The task conflicts with another task or the charter.

Escalation report: the specific question, the evidence, the options considered,
and a recommendation — but **no unilateral decision**.

## 4. Required report format (every task)

```
TASK FPE-x.y — REPORT
STATUS: DONE | BLOCKED | ESCALATE
DIFF SUMMARY: files changed, +/− lines, NEW files
WHAT CHANGED: behaviour implemented (cite spec sections)
REFERENCE CROSS-CHECK: upstream fn ↔ Q31 mirror ↔ FPE fn (table)
TESTS: added/updated test names; results (pass/fail counts)
DIFFERENTIAL RESULTS:
  - Q31↔FPE: max delta per field vs envelope; exact fields confirmed
  - C↔FPE (if Tier-B): per-stage pass/fail; any findings
INVARIANTS: checks run; results
LICENSING: check-licensing.sh result; git diff --check result
Q31 INTACT: confirmation no Q31 behaviour changed (diff evidence)
DEFAULT: confirmation default still Q31
DEVIATIONS/FINDINGS: any out-of-envelope or new flip (triage classification)
ASSUMPTIONS: anything assumed; evidence gap named
ESCALATIONS: questions for owner/reviewer (if any)
```

## 5. Review requirements

- **Focused diff**: one task = one reviewable change set. Reviewer checks
  scope, Q31-intactness, reference cross-check, tests, envelopes, licensing.
- **Independent review**: M8 validation review is performed by a model/person
  **other than the implementer**. M2/M7 numerical reviews independent of the
  kernel author where practical.
- **No self-approval of gates**: acceptance gates (`07` §8) are signed by the
  reviewer/owner, not asserted by the implementer alone.

## 6. Worked example (illustrative task issuance)

```
TASK FPE-4.2 — Float spectral spreading
MILESTONE: M4
SCOPE: two-pass max-envelope spreading for thresholds and spread-energy, f64.
FILES TO CHANGE: rust/src/aac_enc/fpe/thresholds.rs (add spreading fn),
  fpe/orchestrate.rs (call it at advance_thresholds + spread-energy sites).
FILES READ-ONLY: thresholds.rs (Q31), psy.rs, energy.rs, sfb.rs, sfb_tables.rs.
REFERENCE:
  - upstream: spreading.cpp:FDKaacEnc_SpreadingMax:105-125
  - Q31 mirror: thresholds.rs:149 spreading_max
  - spec: 05 §6.4; factors from SfbConfig f64 accessors (03 §4.3)
EXPECTED CHANGE: d=max(x[i], mask_hi[i]*d) forward; mask_lo backward; in place.
  Use tabulated f64 factors; DO NOT recompute via atan/pow.
NON-GOALS: no factor re-derivation; no threshold-init/clip (FPE-4.1); no PCM
  floor (M5); no Q31 edits.
REQUIRED TESTS: unit hand-computed envelope (mirror thresholds.rs:295-308,
  float version); edge factors 0 keep end bands; Q31↔FPE THR/SPREAD within
  q31_tol on committed psy_* vectors; C↔FPE THR/SPREAD (Tier-B).
ACCEPTANCE: THR/SPREAD within 32+|w|/500; no new decision flips; factors
  byte-match tables; all Tier-A pass.
INVARIANTS: finite outputs; thresholds ≥ 0.
REPORT FORMAT: §4.
ESCALATE IF: envelope exceeded; factor source unclear; Q31 edit seems needed.
```

## 7. Model capability notes

- Tasks are sized for a competent coding model with repo access: each cites
  exact files, reference functions, and formulas. No task requires reading the
  whole encoder.
- The incremental strategy (`09`) means a model never faces "implement the
  whole float engine at once"; each task replaces one kernel group against a
  bit-exact-or-envelope gate.
- Provide the model: this package (`docs/fpe/`), the Q31 mirror files, the
  upstream tree (`git show 3878c45a:cpp/…`), and the task spec. The committed
  vectors + in-binary harness give immediate feedback without C provisioning.