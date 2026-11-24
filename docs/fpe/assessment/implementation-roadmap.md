# FPE Implementation Roadmap — Post-Checkpoint Reassessment

**Assessment date:** 21 November 2026
**Checkpoint:** `eb2896923ec788727975268aca0beb27cba51760`
**Authority:** Independent assessment / recommendation. Proposes phases for
owner approval; does **not** authorize implementation or change any ADR.

Companion: [findings-register.md](findings-register.md) (A-/OD- IDs),
[validation-roadmap.md](validation-roadmap.md) (N-/D-/P-/E- test IDs),
[next-phase-recommendation.md](next-phase-recommendation.md).

---

## 1. Reassessment of the existing phase structure

The `09` M0–M9 structure is **sound as a skeleton** but no longer matches
reality and should be re-baselined:

- **M3 is partially done.** FPE-3.1 (rescale energy) + R1 + R2.0/A/B + G-1 are
  implemented and verified; FPE-3.2 (TNS recompute) and FPE-3.3 (A-4) are not.
  The `09` overview table still shows M3 as unstarted (A-15).
- **R2 decision-safety grew beyond the original M3 sketch.** IS/MS is done;
  PE/ATS/S2/S4/S6 (R2.2) is a substantial, separately-reviewable effort that the
  original roadmap folded into "FPE-3.1R2". It deserves its own phase.
- **Evidence-integrity work is a prerequisite, not an afterthought.** The
  missing analytical report, G-1c record, byte baseline and unretained operands
  (OD-4/A-04/A-05/OD-6/A-03) undermine the acceptance basis. This is cheap and
  must come first.
- **Perceptual validation is absent from M0–M9** (the charter keeps metrics
  supplementary). It must be added as an explicit phase to close the conditional
  acceptances and to support any future default decision.
- **The seam-granularity decision (A-13)** must be made before M4, not discovered
  mid-implementation.

**Recommendation:** keep the M-numbering for continuity but insert explicit
sub-phases and two new tracks (evidence integrity; perceptual). Prefer small,
independently reviewable milestones. Do **not** assume "next = M3"; the next
work is evidence hardening + R2.2, then M3 completion.

---

## 2. Proposed phases

Each phase: objective / scope / dependencies / source changes / tests & evidence
/ acceptance / exit / risks / implementation model. "Strong" = strong reasoning
model; "Fast" = faster implementation model; "Owner" = owner decision/action.

### Phase 0 — Evidence integrity & acceptance-baseline hardening
- **Objective:** make the acceptance basis trustworthy and reproducible.
- **Scope:** recover/link the R2.1-B analytical report (`B_closed` derivation,
  OD-4) and G-1c record (A-04); commit a byte-level baseline for the decision
  corpus (OD-6/A-21); hard-assert R2.1-B aggregate counts (A-03); retain
  per-finding IS operands + Class B line vectors (A-05/OD-7/OD-8); recover M2
  green-run + M1/M2 review records or re-review (A-01/A-02); surgical doc
  corrections + status index (A-15/A-22).
- **Dependencies:** none (owner actions + test-only).
- **Source changes:** **test-only** pinning/retention + docs. **No production
  numerical change.** Any `testdata.rs`/harness edit is additive and approved.
- **Tests/evidence:** pinned aggregates; retained operand rows; committed
  baseline; recovered records linked.
- **Acceptance:** every acceptance-cited artifact is committed or permanently
  linked; aggregates asserted; baseline committed.
- **Exit:** owner confirms provenance chain complete.
- **Risks:** records unrecoverable → must record permanent location or
  reconstruct with provenance (not a silent substitute).
- **Model:** Owner (recovery) + Fast (pinning/baseline/docs) + Strong (review of
  reconstructed derivation if needed).

### Phase 1 — R2.2 downstream decision-safety (PE/ATS/S2/S4/S6)
- **Objective:** complete the decision-safety leg required before FPE-3.1
  acceptance; characterize downstream consequences of the float energy.
- **Scope:** production-replay harness for PE/ATS/S2/S4/S6/frame accounting
  (D-1…D-5, D-7); per-finding PE/ATS attribution for F-20/22/26 + P cascades;
  one-LSB sensitivity sweep (N-7); operating-point coverage (D-8); P-cascade
  termination on long/real material (D-6/OD-3); quiet-short fixtures (D-10/A-17).
- **Dependencies:** Phase 0 (retention/pinning discipline); existing R2.1-B
  harness.
- **Source changes:** **test-only** harness extension; additive `testdata.rs`
  resynthesis; `#[cfg(test)]` accessors only if needed (no production numerical
  change).
- **Tests/evidence:** D-1…D-10, N-7; retained per-finding downstream operands;
  pinned aggregates.
- **Acceptance:** every downstream flip residue-explained or structural;
  per-finding PE/ATS quantified; P termination evidenced or bounded; no
  structural/state inconsistency.
- **Exit:** independent numerical + decision review; owner disposition of any
  new findings.
- **Risks:** new downstream flips may be perceptually relevant → route to Phase 5
  (do not force-pass); PE/ATS may reveal larger consequences than IS/MS.
- **Model:** Strong (harness design, margin derivation, flip triage) + Fast
  (mechanical replay wiring, fixture resynthesis).

### Phase 2 — M3 completion (FPE-3.2 / FPE-3.3 / A-5 / F-6 / FpeError decision)
- **Objective:** finish the float energy kernel group and close M3.
- **Scope:** FPE-3.2 float TNS energy recompute (long integer shiftBits from repr
  lane; short no-Ld quirk; overflow rescale value lane, `05` §6.2); FPE-3.3 A-4
  experiment (N-1); A-5 cross-arch Ld measurement (N-2); F-6 closure (N-3/A-14);
  `FpeError` graceful skip-with-note decision + implementation (A-20, `06` §6).
- **Dependencies:** Phase 1 (decision safety for the floated energy); Phase 0.
- **Source changes:** `fpe/energy.rs` (TNS recompute), `fpe/orchestrate.rs`
  (call sites), `fpe/output.rs` (FpeError seam if approved); **no Q31 numerical
  change**.
- **Tests/evidence:** N-1…N-3, N-8; per-kernel envelope (N-4 energy recompute);
  TNS-recompute differential; FpeError path test.
- **Acceptance:** G2 for energy recompute within derived envelope; shifts exact;
  A-4/A-5 recorded; F-6 closed; FpeError decided; **FPE-3.1 + R2 accepted by
  owner + independent review**.
- **Exit:** M3 marked complete; R2.1-B milestone closure *if* perceptual
  conditions also met (else remain conditional — Phase 5).
- **Risks:** TNS recompute interacts with overflow rescale (F-6 edge); FpeError
  changes dispatch signature semantics (must use existing `Err(note)` seam).
- **Model:** Strong (recompute derivation, FpeError design) + Fast (kernel
  translation against `05` §6.2, tests).

### Phase 3 — M4 float threshold init / clip / spreading
- **Objective:** float the threshold front-end.
- **Scope:** FPE-4.1 init (`T_f=E_f·C_RATIO_f`) + clip; FPE-4.2 two-pass spreading
  (tabulated f64 factors, never recomputed); FPE-4.3 assert tonality absent.
- **Dependencies:** Phase 2; **A-13 seam decision (Owner)** — FPE-side composite
  mirror vs additive baseline-gated Q31 kernel extraction.
- **Source changes:** `fpe/thresholds.rs` (new kernels), `fpe/orchestrate.rs`
  (replace `init_threshold`/`advance_thresholds` threshold sites per seam
  decision).
- **Tests/evidence:** N-4/N-5/N-8 (threshold envelope + propagation); unit
  init/clip/spreading; THR/SPREAD differential; tonality-absence assert.
- **Acceptance:** G2 thresholds/spread within derived envelope; factors
  byte-match tables; no new unclassified flips; Tier-A green.
- **Exit:** numerical review.
- **Risks:** R-05 tuning drift (must use tables); composite duplication (A-13);
  propagation bound widening (N-5).
- **Model:** Strong (seam execution, propagation bound) + Fast (spreading
  translation, unit tests).

### Phase 4 — M5 float PCM floor
- **Objective:** float the absolute floor.
- **Scope:** FPE-5.1 PCM floor with accumulating `energyShift` (start 2m, +16/win,
  cap 31, sign branches), `max(T_f/16, pcm_f·2^(∓shift))`.
- **Dependencies:** Phase 3.
- **Source changes:** `fpe/thresholds.rs`, `fpe/orchestrate.rs`.
- **Tests/evidence:** N-4/N-8 (floor accumulation per-window shift/cap/sign);
  THR differential; document no ATH table.
- **Acceptance:** G2 PCM floor; short-window per-window accumulation correct.
- **Exit:** numerical review.
- **Risks:** R-03 shift accumulation; R-06 short-window accumulation.
- **Model:** Fast (translation) + Strong (review).

### Phase 5 — Perceptual validation & conditional-acceptance closure
- **Objective:** establish perceptual evidence; close F-20/22/26 and P
  conditions (or record residual risk).
- **Scope:** P-1 objective metrics; P-2 ABX on flip material; P-3 real-material
  listening; P-4 Q31 regression; P-5 decoder interop (Tier-B). Design early;
  execute after decoder artifact + R2.2 attribution exist.
- **Dependencies:** Phase 1 (flip localization), Tier-B decoder (A-10), Phase 2
  (decodable FPE streams).
- **Source changes:** test/tooling only (metrics harness, stimulus rendering);
  **no production change**.
- **Tests/evidence:** P-1…P-5; pre-registered analysis; retained results.
- **Acceptance:** owner disposition of conditional acceptances **on perceptual
  evidence** (close, or retain conditional with recorded risk). Not a numerical
  gate.
- **Exit:** owner acceptance record updated; conditions explicitly resolved or
  retained.
- **Risks:** audible differences → may require model-level reconsideration
  (Stage 2) or acceptance of bounded divergence; do not manufacture a pass.
- **Model:** Strong (methodology design, statistical plan, interpretation) +
  Fast (metrics/rendering tooling) + Owner (listening panel, disposition).

### Phase 6 — M6 float pre-echo (temporal masking)
- **Objective:** float the temporal state.
- **Scope:** FPE-6.1 fraction `FpePreEchoState` replacing delegated Q31 state;
  FPE-6.2 STOP-before/START-after resets; FPE-6.3 nm1/m_nm1/calc_pre_echo readout.
- **Dependencies:** Phase 4; Phase 1 decision-safety harness (temporal).
- **Source changes:** `fpe/preecho.rs` (new state), `fpe/mod.rs` (FpeChannel),
  `fpe/orchestrate.rs`, `fpe/output.rs`.
- **Tests/evidence:** N-4/N-6/N-8 (history envelope; transitions exact);
  cross-frame onset/stepdown NM1/NM1STATE; per-frame state comparison.
- **Acceptance:** G2 temporal; no new decision flips; transitions exact.
- **Exit:** numerical review.
- **Risks:** **R-07 temporal state drift (highest)**; memorize-pre-clamp order;
  scale alignment.
- **Model:** Strong (state-transition design) + Fast (mirror implementation).

### Phase 7 — M7 MS energies / grouping / TNS-LFE integration
- **Objective:** complete the orchestration value lane.
- **Scope:** FPE-7.1 float MS energies (/4 convention) + stale-on-short Ld;
  FPE-7.2 grouping (quantize→integer regroup; grouped MS-Ld exact); FPE-7.3 TNS
  orchestration + LFE early-outs + D-022; FPE-7.4 long/short output assembly.
- **Dependencies:** Phase 6; **D-9 MS bound re-derivation (A-18)**.
- **Source changes:** `fpe/energy.rs` (MS), `fpe/orchestrate.rs`, `fpe/output.rs`.
- **Tests/evidence:** N-4/N-8 (MS /4; grouped layout); full-output differential;
  grouped MS-Ld exact; LFE/mixed-pair edges; **re-validate `B_closed` for float MS
  operands (D-9)** + decision-safety re-run.
- **Acceptance:** full orchestration; G2 MS/grouping; grouped MS-Ld exact; MS
  decision safety re-established.
- **Exit:** numerical + integration review.
- **Risks:** R-09 channel-pair; R-10 grouped MS-Ld exactness; **new MS divergence
  source (A-18)** re-opens decision safety.
- **Model:** Strong (MS bound re-derivation, integration) + Fast (grouping
  mirror).

### Phase 8 — M8 full three-way differential + public API
- **Objective:** prove G1–G5, G7 on the full corpus; independent validation.
- **Scope:** FPE-8.1 C↔FPE Tier-B every stage; FPE-8.2 triage/pin exceptions;
  FPE-8.3 FPE-selected encode + decode (expand A-06 coverage); FPE-8.4 G4
  fuzz/property; FPE-8.5 licensing; FPE-8.6 public `with_psy_engine`/`PsyEngineKind`.
- **Dependencies:** Phase 7; **Tier-B unblocked (A-10/OD-1: pinned C + dumps +
  decoder)**; A-3 resolved.
- **Source changes:** Tier-B tests; public API exposure (additive); no downstream
  change.
- **Tests/evidence:** G1–G5, G7; coverage matrix fully executed; no silent skips.
- **Acceptance:** G1–G5, G7 pass; independent validation review (not implementer).
- **Exit:** validation sign-off.
- **Risks:** R-11/R-12 reference adequacy/availability; new C↔FPE exceptions.
- **Model:** Strong (triage, review) + Fast (Tier-B wiring) + Owner (reference
  provisioning).

### Phase 9 — M9 performance + final audit + owner acceptance
- **Objective:** G6, G8, G9; finalize.
- **Scope:** FPE-9.1 A/B performance (E-4, fixed-sample CI); FPE-9.2 final audit
  (Q31 unchanged, default Q31, no downstream/manifest change); FPE-9.3 risk
  register update; FPE-9.4 independent sign-off + owner acceptance.
- **Dependencies:** Phase 8; Phase 5 (perceptual disposition).
- **Source changes:** docs; production only if a budget finding is approved.
- **Tests/evidence:** G6 (CI upper ≤ +2%), G8, G9; allocation audit (E-8).
- **Acceptance:** G6 within budget (or owner-approved finding); G8/G9; owner
  acceptance.
- **Exit:** Stage-1 complete.
- **Risks:** R-13/R-14 performance/memory.
- **Model:** Strong (audit, review) + Fast (benchmark harness) + Owner
  (acceptance).

### Stage 2 — genuinely float-native engine (separate charter, out of Stage 1)
- **Objective:** remove integer scaffolding and the Q31-anchored boundary.
- **Scope (each its own charter):** pure-float representation lane (drop
  finalShift duplication, ADR-0005 Stage-2 note); physical-domain energies /
  float-native thresholds (remove `·2^31` boundary); float-recomputed spreading
  factors/minSnr; float-specific SIMD; psychoacoustic improvements.
- **Dependencies:** Stage-1 acceptance + independent audit.
- **Constraint:** changes observable representation decisions → re-validate
  downstream at real-value level; never smuggled into Stage 1.
- **Model:** Strong (architecture/research) + Owner (charter).

---

## 3. Dependencies & priorities

```
Phase 0 (evidence) ─► Phase 1 (R2.2) ─► Phase 2 (M3 complete) ─► Phase 3 (M4) ─► Phase 4 (M5) ─► Phase 6 (M6) ─► Phase 7 (M7) ─► Phase 8 (M8) ─► Phase 9 (M9)
                                          │                                                                                          ▲
                                          └─► Phase 5 (perceptual; needs Tier-B decoder + Phase 1 attribution) ──────────────────────┘
Tier-B provisioning (A-10) ─► Phase 5 (P-5) and Phase 8 (G1/G5)
A-13 seam decision (Owner) ─► Phase 3 entry
```

**Priority order:** 0 → 1 → 2 → (3 → 4 → 6 → 7) → 8 → 9, with 5 running as soon
as its dependencies allow (design early, execute after decoder + attribution).
Phase 0 is small and unblocks trust; Phase 1 is the largest technical gap; do
not skip to M4 before R2.2 + M3 acceptance.

## 4. Implementation priorities & model delegation

| Work | Model | Rationale |
|---|---|---|
| Architecture/seam decisions (A-13), envelope derivations (N-4/D-9), MS bound re-derivation, FpeError design, perceptual methodology, flip triage, independent reviews | **Strong reasoning** | novel derivation, judgment, adversarial verification |
| Test pinning (A-03), byte baseline (A-21), fixture resynthesis (A-17), spec-exact kernel translation (M4–M7 against `05`), Tier-B wiring, doc formatting | **Fast implementation** | mechanical, spec-bound, immediately gated by existing harness |
| Record recovery (OD-4/A-04), reference provisioning (A-10), listening panels, acceptance dispositions, seam-strategy approval | **Owner** | authority/external artifacts |

**Review discipline (`11` §5):** every phase ends with its review gate; no
self-approval; M8 validation review by a non-implementer; focused diffs; Q31
read-only; default unchanged; no commits without owner instruction.

*Assessment-only recommendation; no phase is authorized by this document.*
