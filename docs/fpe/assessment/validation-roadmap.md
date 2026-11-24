# FPE Validation Roadmap — Post-Checkpoint

**Assessment date:** 21 November 2026
**Checkpoint:** `eb2896923ec788727975268aca0beb27cba51760`
**Authority:** Independent assessment. Defines *required* evidence and gates;
does not authorize implementation or change any envelope/acceptance criterion.

This roadmap states what evidence is required before FPE can progress beyond
its current stage, **which test answers which technical question**, and which
are necessary gates versus supplementary. It respects the charter: Stage-1
acceptance is **faithful reproduction of the reference model**; objective
quality metrics supplement, never replace, fidelity gates (`00` §6, `07` §8).
Perceptual evidence is the gate for **closing conditional acceptances** and for
any future default change — not a Stage-1 numerical-fidelity gate.

Companion: [findings-register.md](findings-register.md) (finding IDs),
[implementation-roadmap.md](implementation-roadmap.md) (phase placement).

---

## 0. Principles (binding)

1. **Numerical equivalence ≠ perceptual equivalence** (constraint 5). Every
   gate below labels which axis it proves.
2. **No silent success** (`07` §7.1): exact frame accounting; unsupported ≠
   pass; Tier-B SKIP ≠ PASS; coverage matrix fully executed.
3. **Envelopes inherited, not widened** (ADR-0004). Any change requires the
   amendment mechanism (derived, source-verified, data-dependent, owner-approved,
   independently reviewed). No tolerance is invented to make a test pass.
4. **Conditional acceptances stay conditional** until their conditions are met
   (constraint 6). F-07/09/10/15/17/18 (termination) and F-20/22/26 (PE/ATS +
   perceptual) are not closed by numerical evidence alone.
5. **Q31 remains the reference baseline** (constraint 3); FPE and Q31
   independently identifiable/testable (constraint 4).
6. **Evidence retention:** any artifact cited in an acceptance record must be
   committed or permanently linked (addresses A-01/A-04/A-05/OD-4/OD-7/OD-8).

---

## 1. Numerical validation

### 1.1 Already established (verified at checkpoint)

| Evidence | Question answered | Gate |
|---|---|---|
| R1 full-corpus audit (`differential.rs:fpe31r1_full_corpus_energy_audit`, 2662 frames, 0 fail/unsupported) | Is float rescale energy within the derived per-band envelope across the corpus? | G2 (energy) |
| Three identity gates + decomposition (`audit.rs`) | Is the Q31 reference itself correct (independent integer recompute)? Is FPE arithmetic correct (independent recompute)? Does the delta split into mapping/truncation/boundary terms within bounds? | arithmetic integrity |
| Supported-domain detector (`envelope.rs`) | Are saturation/wrap/overflow/clamp cases rejected rather than silently bounded? | domain safety |
| G-1 probe adjudication (`probe_envelope.rs`, 84 frames, 56 rows) | Are the 28 probe-finding energy deltas envelope-conformant Cause-B residue? | G-1 (energy scope) |
| Class A/B sentinel classification | Are low-energy divergences explained by stored state (mapping-to-zero / truncation-vs-rounding)? | sentinel taxonomy |
| Representation-lane exactness (`m2_stages.rs`, per-frame `assert_channel_output_eq`) | Are maxScale/mdctScale/finalShift/tnsShift bit-exact (ADR-0005)? | repr lane |

### 1.2 Required next (necessary gates)

| ID | Test / evidence | Question | Gate | Phase |
|---|---|---|---|---|
| N-1 | **A-4 f32-vs-f64 energy-sum experiment** (FPE-3.3): measure f32 accumulation error on the corpus; record envelope | Is f64 empirically justified (not just analytically)? | M3 completion | C |
| N-2 | **A-5 cross-arch Ld measurement**: max `|ld_ARM64 − ld_x86-64|` over corpus for `log2`, energy sums, multiply/add chains; record bound within Ld envelope | Is the platform `log2` difference inside the envelope (no correct-rounding assumption)? | M8 cross-platform | C/H |
| N-3 | **F-6 closure**: documented structural-unreachability argument (`psy.rs:1190-1213` long-only `tnsSpecShift`; D-022) + retained watch; optional test-only synthetic | Is short-TNS `tnsSpecShift≠0` impossible (or handled)? | R2 closure | C |
| N-4 | **Per-kernel envelopes for M4–M7**: derived, source-verified bounds for threshold init/clip/spreading (M4), PCM floor (M5), pre-echo history (M6), MS energies (M7); propagated-bound rule (`05` §9.3) | Does each newly floated kernel stay within a *derived* (not widened) envelope? | G2 per kernel | D/E/F/G |
| N-5 | **Threshold propagation bound**: verify `C_RATIO·tol_E + 1` propagation never weaker than `q31_tol`; report widenings explicitly (audit already tracks; must stay 0 or justified) | Do float-energy errors propagate into thresholds within the retained envelope? | G2 (thresholds) | D |
| N-6 | **Cross-frame numerical stability**: long multi-frame runs (music/speech, ≥471 frames) asserting per-frame envelope + state; onset/stepdown decay | Does error accumulate or stay bounded across frames (pre-echo/MS history)? | G2 temporal | F/G |
| N-7 | **One-LSB decision sensitivity characterization**: systematic sweep of band energy near decision boundaries (I2 0.7, is_scale 8.0 edge, direction threshold) measuring flip frequency vs delta | How often does a sub-LSB energy delta flip a decision, and is it bounded? | R2.2 input | B |
| N-8 | **Accumulation precision / rounding boundary unit tests** for each new kernel (hand-computed, silence, full-scale, power-of-two, negative re-shift) | Are rounding/saturation conventions correct per kernel? | unit | D/E/F/G |

### 1.3 Supplementary (not gates)

- f32 SIMD width trade-off study (Stage 2; psy ≈2.4% self time makes it
  immaterial for Stage 1).
- Alternative energy formulations (Stage 2 charter).

---

## 2. Decision-safety validation

### 2.1 Already established (IS/MS, with limits)

| Evidence | Question | Limit |
|---|---|---|
| IS/MS capture/replay parity (`is_ms.rs` gates 1–2) | Does the harness reproduce production decisions exactly (no second oracle)? | stereo-only, VBR-5 |
| I1–I10 margins + boundary unit tests (gate 3) | Are decision boundaries source-derived and pinned at ∓1/±0? | annotation-only |
| Stateful trace + carry origin (gate 4) | Is the IS carry/clear/rescan control flow mirrored and group-end verified? | per-finding chains not retained (A-05) |
| `B_closed` MS bound (gate 5) | Do in-domain MS margin shifts stay within the closed-form bound? | **current operand set only** (A-18); sentinel domain excluded (F-01) |
| Per-band coding state + digest classification (gates 7–8) | Are book/phase/scale/MS compared per band (not just coarse digest)? | digest equality ≠ per-band equality |
| Multi-frame live validation (gate 10) | Do persistent histories evolve across 381 frames with LONG/SHORT/START/STOP? | 12-frame probes; no real music |

### 2.2 Required next — R2.2 (necessary gates before FPE-3.1 acceptance)

| ID | Test / evidence | Question | Gate |
|---|---|---|---|
| D-1 | **PE evolution differential** (production-replay): per-finding PE attribution for F-20/22/26 and the P cascades; compare PE trajectory Q31 vs FPE | Does a one-LSB energy/IS difference shift perceptual entropy, and by how much? | FPE-3.1 acceptance; F-20/22/26 condition |
| D-2 | **ATS evolution differential**: ATS state trajectory across frames | Does temporal smoothing diverge structurally? | FPE-3.1 acceptance |
| D-3 | **S2 VBR adaptation / avoid-hole decisions** | Do threshold changes alter VBR adaptation/avoid-hole? | R2.2 |
| D-4 | **S4 scalefactor decisions** | Do threshold/energy changes alter scalefactors beyond the propagated envelope? | R2.2 |
| D-5 | **S6 bit allocation + final frame accounting** | Does allocation differ; are frames valid/decodable; exact accounting? | R2.2 |
| D-6 | **P-cascade termination** (OD-3): long/real-material runs (music, ≥hundreds of frames) proving termination or bounded persistence; preserve the 5-frame observation boundary | Does the I2-escape cascade terminate on real material? | P conditional closure |
| D-7 | **Margin-based residue-vs-structural flip classification** extended to PE/ATS/S2/S4/S6 (mirror `phase1e` methodology) | Is every new flip residue-explained or structural? | R2.2 |
| D-8 | **Operating-point coverage**: add VBR modes beyond VBR-5 (different TNS bitrate/minSnr → different TNS activity) and mono-context sanity | Is decision safety robust across the validated matrix? | R2.2 |
| D-9 | **MS bound re-derivation at M7** (A-18): when MS energies float, re-derive/re-validate `B_closed` for the new operand set | Does the MS bound still hold with float MS operands? | M7 |
| D-10 | **Quiet short-window sentinel/PE test** (`07` §5.2): resynthesize `fpe-short-burst-quiet`/`fpe-tns-active-short` (A-17) and exercise | Are quiet-short sentinel/PE paths safe? | R2 closure |

**Methodology (binding):** extend the *production-replay* approach (call real
`pe`/`ats`/`s2`/`s4`/`s6` functions on cloned operands; assert replay equality;
margins annotation-only). Do **not** re-implement downstream as a second oracle.
Retain per-finding operands (fixes A-05). Pin aggregate counts (fixes A-03).

### 2.3 Perceptual acceptability is a separate axis

Understanding a numerical behaviour (e.g. the one-LSB I2 escape) **does not**
demonstrate its perceptual acceptability. D-1…D-5 quantify downstream *numerical*
consequences; §3 establishes *perceptual* consequences. Both are required to
close F-20/22/26; neither alone suffices.

---

## 3. Perceptual validation

**Status: none exists (A-09).** This is the gate for closing the G-finding
conditional acceptances and for any future FPE-default decision. It is **not** a
Stage-1 numerical-fidelity gate (charter keeps objective metrics supplementary).

### 3.1 Required evidence

| ID | Test / evidence | Question | Method | Gate |
|---|---|---|---|---|
| P-1 | **Objective quality metrics** on FPE vs Q31 streams: SNR/segmental-SNR vs C decode, ebur128 loudness (`tools/aac-enc-metrics/`) | Is there a measurable objective difference? | report-only, supplementary | supports P-2 |
| P-2 | **ABX listening test** on decision-flip material: stimuli rendered from F-20/22/26 bands and P-cascade probes (panned/out-of-phase/narrow), FPE vs Q31 vs C anchor | Can listeners distinguish FPE from Q31 on the material where decisions flip? | double-blind ABX, ≥20 trials/listener, trained + naive panels, binomial significance | **F-20/22/26 closure** |
| P-3 | **Listening on real music/speech** (m7/m8 corpus): FPE vs Q31, preference + transparency | Is FPE perceptually competitive on natural material? | MUSHRA-style or paired preference; report CI | pre-default |
| P-4 | **Regression against Q31 engine**: confirm no perceptual degradation on the existing validated corpus | Does FPE regress the baseline perceptually? | paired comparison | pre-default |
| P-5 | **Decoder interoperability** (Tier-B, G5): FPE corpus streams decode cleanly (no errors, finite PCM) via `aac-dec` | Are FPE streams valid/decodable? | requires decoder artifact (A-10) | G5 |

### 3.2 Methodology notes (independent design)

- **Stimulus selection must target the flips**, not generic material: render the
  exact bands/frames where F-20/22/26 and the P cascades change IS admission/
  direction/scale, plus matched controls where decisions agree. A null result on
  generic material would not close the conditional acceptances.
- **ABX is meaningful only where a coding difference exists** (16 coding-diff
  frames identified). Use the retained per-band evidence to localize stimuli.
- **Controls:** Q31 as reference, C as anchor; loudness-match; avoid clipping;
  randomize; counterbalance order. Pre-register the analysis plan (avoid optional
  stopping, mirroring the `08` rev-4 statistical discipline).
- **Objective metrics never replace listening** for the perceptual axis; they
  screen and corroborate. A metric difference is not claimed as a perceptual
  improvement (charter).
- **Sample size / power:** predeclare panel size and trials; report confidence
  intervals; an inconclusive result is reported as inconclusive, not as "pass".

### 3.3 Sequencing

Perceptual work depends on decodable FPE streams (P-5/Tier-B) and on localized
flip stimuli (D-1…D-5). It can be *designed* early (strong reasoning model) but
*executed* only after the decoder artifact and R2.2 attribution exist. Do not
claim perceptual neutrality before P-2/P-3 are run.

---

## 4. Engineering validation

| ID | Test / evidence | Question | Status / gate |
|---|---|---|---|
| E-1 | **Determinism**: no FMA contraction (`mul_add`/`fast_math` absent, verified), fixed order; per-arch bit-reproducibility of `*`/`+`/`·2^k` | Is FPE deterministic per platform? | controls present; assert in CI |
| E-2 | **Cross-platform consistency**: x86-64 CI Tier-A (implicit) + measured A-5 bound (N-2) | Do ARM64/x86-64 agree within envelope? | A-5 open (A-11) |
| E-3 | **Test coverage**: corpus matrix (`07` §6) fully executed; coverage floors asserted; FPE-selected integration expanded (A-06) | Is every category × fixture executed? | partial; expand |
| E-4 | **Performance (G6)**: fixed-sample paired interleaved A/B, predeclared N, single analysis, 95% two-sided CI of median paired % diff; PASS iff CI upper ≤ +2% (below −2% = improvement); INCONCLUSIVE if straddles | Does FPE regress throughput beyond budget? | M9; Q31 baseline captured |
| E-5 | **State integrity**: per-frame persistent-state comparison (PreEchoState/TNS/ms_ld) over multi-frame sequences; canary/ownership tests | Does state diverge or alias across engines? | active; keep through M6/M7 |
| E-6 | **Numerical safety (G4)**: property/fuzz over random spectra; boundary guards; no NaN/Inf/invalid threshold downstream | Are invariants robust to arbitrary input? | M8; release-active assert present; FpeError pending (A-20) |
| E-7 | **SIMD compatibility**: Stage-1 scalar f64 (no SIMD); Stage-2 vectorization separate | Is SIMD deferred correctly? | deferred (psy ≈2.4%) |
| E-8 | **Allocation audit** (`08` §3.5): no per-frame kernel heap allocs beyond output Vec; allocate-selected-only | Memory regression? | M9 |
| E-9 | **Licensing/provenance (G7)**: `check-licensing.sh` + `git diff --check`; TPMV notices; provenance records committed | Licensing hygiene + evidence provenance? | hygiene green; provenance gaps A-04/OD-4 |
| E-10 | **Default unchanged (G9)** + Q31 byte-identical baseline (G3) | Is Q31 still default and unchanged? | green (2/2 baseline) |

---

## 5. Acceptance gate map (which gates are necessary, and when)

| Gate (`07` §8) | Axis | Necessary for | Status at checkpoint |
|---|---|---|---|
| G0 M2 pass-through | wiring | foundation | historical; not re-runnable (A-01) — recover record |
| G2 Q31↔FPE baseline | numerical | each floated kernel | energy done; M4–M7 pending (N-4) |
| G1 C↔FPE fidelity | numerical | Stage-1 | **blocked** (Tier-B, A-10) |
| G3 no regression | engineering | every phase | green (Tier-A 493, baseline 2/2) |
| G4 numerical safety | engineering | M8 | partial (guards present; fuzz pending) |
| G5 decoder | engineering | Stage-1 | **blocked** (decoder, A-10) |
| G6 performance | engineering | M9 | not measured |
| G7 licensing | engineering | every phase | green (provenance gaps separate) |
| G8 independent review | governance | each milestone | R2.1-B/G-1 reviewed; M1/M2 records absent (A-02) |
| G9 default unchanged | engineering | always | green |
| **R2 decision safety** (`07` §5.2) | decision | **FPE-3.1 acceptance** | IS/MS done; PE/ATS/S2/S4/S6 pending (A-08) |
| **Perceptual closure** | perceptual | **F-20/22/26 + P conditions; pre-default** | none (A-09) |
| **Tier-B C parity** | numerical | applicable later gate | **blocked, not waived** (OD-1) |

**Critical path to FPE-3.1 acceptance:** R2.2 (D-1…D-5) + independent review +
A-4/A-5/F-6 closure. **Critical path to Stage-1 closure:** + Tier-B (G1/G5) +
M4–M7 + G4/G6/G8. **Critical path to closing conditional acceptances:** +
perceptual (P-2/P-3) + P termination (D-6). These are distinct; do not conflate
numerical acceptance with perceptual acceptance.

---

## 6. Evidence requirements (retention policy)

To prevent recurrence of A-01/A-04/A-05/OD-4/OD-7/OD-8:

1. Every acceptance record must cite **committed** artifacts (test names +
   retained output rows) or a **permanent external link** — never a removed
   temporary scaffold.
2. Temporary diagnostic harnesses must **emit retained rows before removal**
   (the G-1 `BandRow` pattern is the model; extend it to IS margins and PE/ATS).
3. Aggregate acceptance counts must be **hard-asserted** in retained tests
   (G-1a sentinel-count pattern; extend to R2.1-B aggregates, A-03).
4. Byte-level baselines for any corpus used in acceptance must be **committed**
   (OD-6/A-21).
5. Cross-platform measurements must be **recorded**, not inferred from "tests
   pass on both" (A-11).

*Assessment-only; no envelope, tolerance, test or acceptance criterion was
changed. All gates above are recommendations for owner approval.*

---

## Dated clarification — M2 acceptance (24 November 2026)

*Appended without rewriting §§0–6 (history preserved; this document is a
21 November 2026 assessment).*

- The §5 gate-map row “G8 independent review … M1/M2 records absent
  (A-02)” is supplemented for M2 by a separate event: the fresh
  independent M2 review by **GLM 5.3 Flash** on 24 November 2026
  (`docs/fpe/assessment/m2-independent-review-glm-2026-11-24.md`, PASS WITH
  FINDINGS) and formal owner acceptance of M2
  (`docs/fpe/assessment/phase-0/P0-A-owner-disposition-record.md`, Amendment M2
  acceptance). G8 is satisfied for M2 only; M1 sign-off remains
  unretained. G0 remains historical/non-rerunnable (A-01 open); Tier-B
  (G1/G5) remains blocked; the nine conditional R2.1-B acceptances remain
  conditional. No gate, envelope, tolerance or criterion is changed by
  this clarification.
