# FPE-3.2 Owner Decision — Float TNS Energy Recompute Acceptance

**Authority:** Project owner (explicit decision issued 27 November 2026).
**Recorded by:** Muse Spark 1.3 Contributor (OpenCode), 27 November 2026.
**Disposition: ACCEPT WITH DOCUMENTED LIMITATIONS — the FPE-3.2
implementation covering float TNS energy recomputation for long and short
windows, including integration into the FPE orchestration path, is accepted
on the strength of the independent Mistral 4 review (PASS WITH FINDINGS,
no correctness defect identified). No production, numerical-contract,
or unrelated-governance change.**
**Baseline:** `189e6a79b78a84dcc5be88eaca5ffaba24097acc`
(= HEAD `189e6a7 feat(fpe): add R2.2 capture and replay infrastructure`).
**Scope:** Governance only — acceptance of the FPE-3.2 implementation
correctness and its reviewed integration requirements (assessment
implementation-roadmap Phase 2, FPE-3.2 item; findings-register context
A-12/A-14/A-20/A-21).
Documentation-only implementation: this record plus the dated assessment
clarifications listed in §7. No production source, test, harness, manifest,
lockfile, dependency, CI workflow, ADR, numerical threshold, tolerance,
acceptance envelope, R2.2 evidence/schema, failure pin, or M4-authorization
change.

**Subject patch (documentation only; pre-existing working-tree work
preserved untouched, see §8):**

- New: `docs/fpe/assessment/fpe-3.2-owner-decision.md` (this record).
- Dated clarifications (history-preserving appends, §7):
  `docs/fpe/assessment/findings-register.md`,
  `docs/fpe/assessment/validation-roadmap.md`,
  `docs/fpe/assessment/implementation-roadmap.md`.

This is the formal record of the owner's decision. It is not a new technical
review and not an implementation report. Technical evidence lives in the
implementation, tests, and independent review cited below; nothing in this
record re-adjudicates that evidence, upgrades any conditional acceptance,
closes any unrelated finding, or changes any numerical contract.

Preserved sources (unchanged by this decision):

- Normative package: `docs/fpe/05-floating-point-translation-spec.md`
  (§6.1, §6.2, §6.8, §5, §9.2–§9.4), `docs/fpe/09-implementation-roadmap.md`
  (M3/FPE-3.2), `docs/fpe/07-validation-strategy.md` (§4–§5),
  `docs/fpe/02-reuse-and-translation-inventory.md` (§1 #19–#20),
  `docs/fpe/03-fdk-reference-analysis.md` (§§4.9–4.10),
  `docs/fpe/04-system-architecture.md` (§§4, 6),
  `docs/fpe/06-encoder-integration.md` (§§4–6),
  `docs/fpe/adr/0002-floating-point-representation.md` (f64 decision,
  unchanged), `docs/fpe/adr/0004-differential-validation.md`,
  `docs/fpe/adr/0005-representation-lane-retention.md`.
- Assessment package: `docs/fpe/assessment/implementation-roadmap.md`
  (Phase 2), `docs/fpe/assessment/validation-roadmap.md`
  (N-1/N-3/N-4/N-8), `docs/fpe/assessment/findings-register.md`
  (A-12/A-14/A-20/A-21 context),
  `docs/fpe/assessment/a4-owner-decision.md` (A-4, separately accepted).
- Implementation (working-tree, accepted as reviewed, not modified by this
  record): `rust/src/aac_enc/fpe/energy.rs`
  (`tns_recompute_energy_into`), `rust/src/aac_enc/fpe/orchestrate.rs`
  (mirror-then-replace call sites), `rust/src/aac_enc/fpe/mod.rs` and
  `rust/src/aac_enc/fpe/output.rs` (documentation),
  `rust/src/aac_enc/testdata.rs` (`fpe-tns-active-short-stereo` fixture).
- Independent review: Mistral 4, dated 26 November 2026, verdict
  **PASS WITH FINDINGS** (findings F1–F6, preserved in §4). No
  correctness defect was identified.

---

## 1. Decision — FPE-3.2 accepted with documented limitations

**ACCEPT WITH DOCUMENTED LIMITATIONS.**

The FPE-3.2 implementation covering float TNS energy recomputation for
long and short windows, including integration into the FPE orchestration
path, is accepted.

**Date and rationale.** Issued 27 November 2026, following the independent
Mistral 4 review dated 26 November 2026. The review verdict was PASS WITH
FINDINGS: no correctness defect was identified in the FPE-3.2
implementation or its reviewed integration. The six findings (F1–F6, §4)
are limitations, observations, and deferred items — none blocks acceptance
of FPE-3.2 implementation correctness — and each is preserved with its
disposition below. This decision is limited to FPE-3.2 implementation
correctness and its reviewed integration requirements; it does not accept
all M3 or Phase 2 gates (§§5–6).

## 2. Evidence basis (as recorded, not re-adjudicated)

- **Independent review:** Mistral 4, 26 November 2026; verdict PASS WITH
  FINDINGS (F1–F6); no correctness defect identified.
- **Implementation scope (working tree, unmodified by this record):**
  `fpe/energy.rs` TNS-recompute kernel (long: float values + integer
  `shiftBits` from the repr lane + guarded Ld with unconditional
  `−shiftBits·2/64` correction; short: float values, no Ld array — the C
  quirk preserved); `fpe/orchestrate.rs` mirror-then-replace call sites
  (Q31 `tns_encode_channel` unchanged; only recomputed values replaced;
  `overflow_rescale` unchanged); docs-only `fpe/mod.rs` / `fpe/output.rs`;
  `testdata.rs` A-17 short-TNS fixture.
- **Test state at recording (as reported in the FPE-3.2 review package):**
  `aac_enc::fpe::energy` 24/24 pass (direct-kernel, differential, and
  edge-case tests for the TNS recompute); `fpe::tests::precision` 8/8
  pass; `fpe::tests::f6_tns` 2/2; `fpe::tests::envelope` 15/15;
  `fpe::tests::m2_stages` 8/8; `cargo test --lib fpe` 276 passed /
  7 failed, where the 7 failures are the pre-existing baseline-pin
  failures documented in §4 (F4) — pins moved by the in-progress
  FPE-3.2/R2.2 changes, not rebaselined by this decision.
- **Prior leg:** A-4 (FPE-3.1 rescale + FPE-3.2 TNS-recompute accumulation)
  remains separately accepted with documented limitations
  ([a4-owner-decision](a4-owner-decision.md)); nothing here re-decides it.

## 3. What this decision covers

- The FPE-3.2 numerical implementation: long- and short-window float TNS
  energy recomputation per `05` §6.2 (values lane floated; `shiftBits` /
  `tnsSpecShift` / `maxScale` representation-lane decisions integer-exact
  per ADR-0005).
- The reviewed integration requirements: mirror-then-replace orchestration
  in `fpe/orchestrate.rs` (Q31 encode call unchanged; replacement limited
  to recomputed stored energies and long-window Ld; overflow rescale
  unchanged and applied after all channels); FPE-owned state and scratch
  usage; Q31 behaviour unchanged when Q31 is selected; dispatch
  correctness for both engine selections; no dependency on test-only code
  in the production path.
- The six independent-review findings with their limitations (§4) — the
  acceptance travels with them.

## 4. Independent-review findings — preserved with limitations

All six Mistral 4 findings are recorded in full. None identified a
correctness defect; none is closed, waived, or reclassified beyond what
is stated here.

- **F1 — Nonzero `shiftBits`/`tnsSpecShift` production behaviour
  unobserved on-corpus; synthetic coverage only.** The corpus exercises
  `shiftBits = 0` paths; nonzero-shift behaviour is covered by synthetic
  unit tests only (`tns_nonzero_shiftbits_ld_cases`,
  `tns_overflow_rescale_exponent_cases`). Limitation: no measured
  nonzero-shift production evidence is claimed. Disposition: accepted as
  a documented evidence gap; it does not block FPE-3.2 correctness
  acceptance. Corpus measurement, if ever required, belongs to a
  separately scoped follow-up — not to this decision.
- **F2 — Cross-architecture `log2`/Ld validation incomplete; A-5/N-2
  Linux leg open.** The FPE-3.2 kernel computes long-window Ld via f64
  `log2()` per band; platform `log2` differences are measured only to the
  extent recorded under A-5/N-2 (macOS ARM64 vs macOS x86-64-under-Rosetta
  retained evidence; Linux x86-64 CI measurement open). Disposition:
  carried under the existing A-5/N-2 tracking; not an FPE-3.2 correctness
  blocker and not closed by this decision.
- **F3 — Stale `audit.rs` documentation labels remain.** Informational
  documentation-accuracy observation in the FPE test harness (`audit.rs`
  labels). Disposition: to be corrected during appropriate future
  maintenance; no harness, test, or numerical change is made or
  authorized here.
- **F4 — Seven baseline-pin failures remain unrebaselined.** Six R2.1-B
  P0-B `is_ms.rs` evidence tests plus
  `probe_envelope::g1_probe_corpus_aggregate` fail on this tree (old-tree
  pins vs current-tree FPE-3.2 outputs). The observed IS/MS changes and
  downstream bitstream differences remain subject to R2.2 owner
  disposition. Disposition: no automatic rebaselining is performed or
  authorized by this decision; the failures stay open under their
  existing pins (cf. open item A-21) and their downstream meaning belongs
  to R2.2, not to FPE-3.2 correctness.
- **F5 — Frame-level `FpeError` taxonomy deferred.** The graceful
  `FpeError`/skip-with-note path (A-20, `06` §6) remains undecided and
  unimplemented; R1 behaviour (release-active `assert!` in
  `fraction_to_stored`) stands. Disposition: carried under the existing
  A-20 tracking; the `output.rs` seam, if ever approved, belongs to a
  separately scoped decision — not to this one.
- **F6 — Cargo packaging verification outstanding for a clean committed
  state; licensing verification passed.** The licensing check passes;
  full packaging verification (`cargo package` file listing or
  equivalent) cannot be completed against a clean committed state while
  the tree carries uncommitted work, and is therefore outstanding.
  Disposition: recorded as outstanding; no manifest, packaging, or
  licensing change is made or authorized here.

## 5. Explicit boundaries

- A-4 remains separately accepted with documented limitations
  ([a4-owner-decision](a4-owner-decision.md)); this decision neither
  re-decides nor extends it.
- f64 remains the approved Stage-1 implementation (ADR-0002 unchanged).
- No f32 production kernel or ADR-0002 amendment is authorized.
- Q31 remains the unchanged reference and the default engine.
- No perceptual-equivalence claim is made.
- R2.2 decision safety remains an independent gate; this acceptance
  establishes no downstream decision outcome (cf. a4-owner-decision §5).
- No automatic rebaselining of the seven failing tests (F4).
- F-6 closure (A-14/N-3), `FpeError` disposition (A-20), A-5/N-2
  (A-11, Linux leg), perceptual validation (A-09), E2/E3 and Group F
  remain separately tracked and are not closed, narrowed, or reclassified
  by this decision.
- M3 is not declared complete.
- M4 and subsequent implementation phases are not authorized.

## 6. Authorization boundary — explicit non-authorizations

This decision does **not**:

- Declare M3 completion. The FPE-3.2 implementation/acceptance leg is
  satisfied; F-6 closure, the `FpeError` decision, the N-2 Linux leg, R2.2
  decision safety (A-08/OD-2), and the perceptual conditions (A-09) remain
  as currently recorded.
- Authorize M4 or any subsequent implementation phase, any production
  change, or any public-API/manifest/dependency change.
- Alter, re-pin, waive, or accept the seven existing FPE-suite failures;
  they remain separately tracked and are not resolved by this decision.
- Amend or reinterpret any ADR (ADR-0002 f64 decision stands; ADR-0004 /
  ADR-0005 lane and envelope arrangements unchanged).
- Change any numerical threshold, tolerance, envelope, or acceptance
  criterion.
- Upgrade, condition, or re-open any other conditional acceptance
  (R2.1-B nine conditionals; F-20/F-22/F-26; Tier-B C parity stays
  blocked, not waived; G0 historical/non-rerunnable).
- Authorize any f32 production work or Stage-2 optimization.

## 7. Existing milestones and governance (preserved)

- All existing **G0–G9 definitions and their current status** (including
  Tier-B G1/G5 blocked per A-10/OD-1; G8 satisfied for M2 only).
- All other **M3 dependencies and their existing status** beyond the
  FPE-3.2 leg now accepted (F-6, `FpeError` decision; R2.2 decision safety
  per A-08/OD-2; perceptual conditions per A-09; N-2 Linux leg per the
  A-5/N-2 allocation — an M8 requirement, not an M3 blocker).
- Existing **R2.2 decisions and limitations** (Group C accepted with
  documented limitations; Group D accepted with findings D-R1…D-R4,
  reconciled with consolidated evidence; E1 schema v2 freeze OD-3;
  Groups E2/E3, Group F, and M4 entry remain unauthorized except under
  their own explicit instructions).
- The original **normative documents** (`01`, `02`, `03`, `04`, `05`,
  `06`, `07`, `09`, `m0-verification.md`, ADRs) are **not modified** in
  this increment. The owner-decision record and the dated assessment
  clarifications below are the mechanism recording this acceptance.

Assessment-document clarifications appended under this decision (dated,
history-preserving, normative documents untouched):

- Assessment findings register
  (`docs/fpe/assessment/findings-register.md`): dated update referring to
  this record — FPE-3.2 recorded and accepted with documented limitations
  (F1–F6); the §7 blocker-map is clarified on the FPE-3.2 leg only; F-6,
  `FpeError` (A-20), N-2 Linux leg, R2.2, perceptual conditions, and the
  seven pin failures (A-21) unchanged.
- Assessment validation roadmap
  (`docs/fpe/assessment/validation-roadmap.md`): dated clarification
  referring to this record — the FPE-3.2 implementation/acceptance leg
  (G2 energy recompute within derived envelope; shifts exact) is recorded
  as owner-accepted with limitations F1–F6; N-1/N-3/N-4/N-8 rows otherwise
  unchanged; M3 completion not declared.
- Assessment implementation roadmap
  (`docs/fpe/assessment/implementation-roadmap.md`): dated clarification
  referring to this record — Phase 2 scope item "FPE-3.2 float TNS energy
  recompute" with acceptance "G2 …; FPE-3.1 + R2 accepted by owner +
  independent review" is clarified: the FPE-3.2 leg is now recorded and
  accepted with documented limitations; no phase authorized; M3 completion
  not declared.

## 8. Recording integrity

- Branch at recording: `main`; HEAD `189e6a79b78a84dcc5be88eaca5ffaba24097acc`
  (`feat(fpe): add R2.2 capture and replay infrastructure`).
- Working-tree inventory at recording (before this decision's edits):
  15 modified + 13 untracked, all pre-existing and preserved
  byte-identically except for this decision's explicitly scoped
  documentation edits (§7):
  - Modified (15): `docs/fpe/assessment/R2.2-downstream-decision-safety-design.md`,
    `docs/fpe/assessment/{findings-register,implementation-roadmap,validation-roadmap}.md`,
    `rust/src/aac_enc/fpe/{energy,mod,orchestrate,output}.rs`,
    `rust/src/aac_enc/fpe/tests/{decision,is_ms,mod,r22_capture}.rs`,
    `rust/src/aac_enc/{rate,scf,testdata}.rs`.
  - Untracked (13): `docs/fpe/FPE-3.3-A-4-experiment-report.md`,
    `docs/fpe/assessment/{R2.2-group-C-owner-acceptance,R2.2-group-D-implementation-report,R2.2-group-D-owner-acceptance,R2.2-group-D-owner-decision,R2.2-group-E1-owner-decision,a4-owner-decision,a5-n2-owner-decision}.md`,
    `rust/src/aac_enc/fpe/tests/{evidence,precision,s4_step3,trace,xarch_log2}.rs`.
- New files under this decision: this record only (§7 appends are edits
  to existing files).
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

- 27 November 2026: owner decision **ACCEPT WITH DOCUMENTED LIMITATIONS**
  issued as recorded in §§1–6 (FPE-3.2 implementation and reviewed
  integration requirements accepted; independent review PASS WITH
  FINDINGS with no correctness defect; findings F1–F6 preserved with
  limitations; no perceptual, decision-safety, cross-architecture, ADR,
  production, pin-rebaseline, M4, or M3-completion change).
  Assessment clarifications (§7) appended under this record.

**STOP — FPE-3.2 recorded as owner-accepted with documented limitations
(F1–F6). Assessment clarifications are appended under this record only.
A-4 stays separately accepted; F-6, FpeError, A-5/N-2, R2.2 decision
safety, perceptual validation, E2/E3, Group F, M4 entry, and the seven
existing FPE-suite failures remain unauthorized, unclosed, and separately
tracked by this record. M3 is not declared complete. Awaiting separately
scoped owner direction.**
