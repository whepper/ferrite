# 10 — Risk Register

Date: 2026-11-16. Probability/impact: L/M/H. Each risk has mitigation and
verification. IDs are referenced by the roadmap (`09`). Update at M9 with
measured outcomes.

| ID | Risk | Prob | Impact | Mitigation | Verification |
|---|---|---|---|---|---|
| R-01 | Accidental change to existing Q31 engine | M | H | FPE imports Q31 read-only; only additive `pub(crate)` visibility bumps permitted; seam additive only; no Q31 refactor | M2/M9 diff review: zero Q31 behaviour change; Q31-default output byte-identical to baseline hashes (G3) |
| R-02 | Orchestration order drift vs `psyMain` | M | H | M2 skeleton mirrors `psy.rs:495-1116` order exactly; bit-identical-to-Q31 gate before float kernels | M2 Q31↔FPE bit-exact on all committed vectors |
| R-03 | Lost scaling/normalization; misread fixed-point; **mixed numerical domains** (P1) | M | H | **rev 2:** single FPE fraction domain `v_f=v_st/2^31` derived from source (`05` §1, per-kernel domain table §6.0); representation lane integer-exact (ADR-0005); per-kernel cross-check vs Q31 mirror; shifts cancel in float proven | shifts/maxScale/mdctScale **exact**; energy within `q31_tol`; unit domain-conversion tests; no domain mixing in review |
| R-04 | Ld guard/floor/sentinel convention error | M | M | `05` §6.8 per-vector convention table; grouped MS-Ld stays exact `calc_ld_data`; sentinel-lift quirk preserved | differential `NRGLD/GELD/GTLD/TLD/MSMLD`; unit convention tests |
| R-05 | Psychoacoustic tuning drift (recomputed factors, wrong constants) | M | H | consume tabulated SPREAD/minSnr (no float recompute); constants derived exactly `q31/2^31`; tuning vs scaffolding separated (`03` §5) | constants unit-exact; factors byte-match tables; C↔FPE threshold envelope |
| R-06 | Long/short inconsistency; grouped stride layout error; per-window PCM accumulation | M | H | preserve short no-Ld quirk, stale-on-short MS-Ld, grouped stride contract (`psy.rs:216-224`), energyShift +16/win cap 31 | grouped layout asserts; short/long differential; edge fixtures (mixed pair, all-zero) |
| R-07 | Temporal state drift (pre-echo history, memorize/clamp order, resets) | M | H | float history with identical transitions; memorize-pre-clamp order; STOP-before/START-after; cross-frame fixtures | `NM1/NM1STATE` differential on onset/stepdown; history within envelope; transitions exact |
| R-08 | State init mismatch (pre-echo seed, prev_scale, TNS zero) | L | M | `FpeChannel::new` mirrors `PsyChannel::new`; init unit tests | M2 bit-exact gate includes first-frame **and per-frame persistent state** (multi-frame) |
| R-09 | Channel-pair regression (joint shifts, TNS sync, MS, LFE) | M | H | element-global decisions preserved; TNS reused; D-022 asserts; LFE early-outs | stereo + LFE + mixed-pair differential; MS digest exact on corpus |
| R-10 | Incorrect quantization integration; grouped MS-Ld not exact | L | H | boundary emits exact stored contract; downstream untouched; grouped MS-Ld exact integer | S4/S6/S8 differential within propagated envelope; short MS decision operands exact |
| R-11 | Inadequate reference testing (final-files-only, missing stages) | M | H | three-way intermediate-stage harness (`07` §4); full corpus incl. edge cases; decision-exact gates; independent validation review | G1–G5 pass; coverage matrix (`07` §6) complete |
| R-12 | Reference checkout unavailable / wrong revision | M | M | re-provision pinned `f14ad40a`; `check-reference.sh`; fallback `3878c45a:cpp/` build with documented assumption; Q31↔FPE in-binary harness unblocks dev | M0 checkout validation; A-3 diff recorded |
| R-13 | Performance regression | L | M | correctness first; **fixed-sample paired interleaved A/B, 95% two-sided CI** (predeclared N, single analysis, no optional stopping; `08` §3); ±2% budget unchanged; no premature SIMD; attribution profiling | G6 PASS iff 95% CI upper bound ≤ +2% (below −2% = improvement, passes); straddles +2% = INCONCLUSIVE ≠ pass (`08` §4) |
| R-14 | Memory regression (per-frame allocs, doubled state) | L | L | `FpeScratch` reused; allocate-selected-only; no kernel heap allocs; no globals | allocation audit (`08` §3.5); sizeof review |
| R-15 | Licensing / provenance (translated code obligations, notices) | M | H | FPE retains existing FDK-AAC licence; treated as translated/derived code under the project compliance position → TPMV notices + package license unchanged; provenance recorded (`12` §0); no upstream code copied ad hoc; no table regen | `check-licensing.sh` passes; `git diff --check`; no SPDX/NOTICE/license-file change; provenance doc reviewed |
| R-16 | Numerical instability (NaN/Inf/invalid threshold reaching encoder) | L | H | f64 (no denormal range issues); invariant guards; non-finite → loud **release-active boundary assertion** in R1, never silent; graceful frame-level `FpeError` deferred to R2; boundary clamps | G4 property/fuzz test over random spectra; edge fixtures; no NaN/Inf in corpus |
| R-17 | New decision flips beyond classified set (knife edges) | M | M | inherit classified exceptions only; triage any new flip (residue vs structural); pin + owner approval | decision-exact gates; phase1e-style causality triage (`07` §5) |
| R-18 | Cross-arch numerical difference (x86 vs ARM log2/FTZ) | L | L | ARM64 reference; no FTZ dependence (f64 range); **no correct-rounding claim** — Ld bound measured (`05` §2); document | x86 supplementary run (A-5); measured cross-platform Ld bound within envelope |
| R-19 | Accidental float round-trip / rounding in M2 pass-through (P2) | M | H | M2 delegates to Q31 and passes stored-int/f64-Ld through unchanged; boundary quantization inactive until M3+ (`09` M2, `05` §7, ADR-0004) | M2 gate: bit-identical **complete observable output + persistent state per frame** (`==` every field + spectrum + state); no float conversion in M2 diff |
| R-21 | State divergence hidden by single-frame / output-only tests | M | H | M2 harness runs **multi-frame sequences** and compares **all frame-surviving state after every frame** (PreEchoState/TNS/ms_ld) via test-only helpers; covers long/short, START/STOP, pre-echo reset, stale-on-short MS-Ld (`07` §3.1, `09` M2) | per-frame state bit-identity over transition sequences; divergence localized to introducing frame |
| R-20 | Misleading validation success from skipped frames / incomplete runs | M | H | frame accounting; Tier-B SKIP≠PASS (INCOMPLETE exit 2); coverage matrix fully executed; loud failure on missing artifacts (`07` §7.1) | validation report lists every coverage cell + result; no silent skips; default-regression guard independent of FPE |

## Top risks (owner attention)

1. **R-03 / R-05** (scaling/tuning fidelity): the core technical risk.
   Mitigated by the retained representation lane, tabulated factors, exact
   constants, and the bit-exact-then-envelope incremental strategy.
2. **R-01** (Q31 intact): mitigated by additive-only seams and byte-identical
   default regression.
3. **R-11 / R-12** (validation adequacy + reference availability): mitigated
   by the in-binary harness (works without C) plus re-provisioned Tier-B.

## Assumptions carried as risks

- A-3 (pinned tree ≡ `3878c45a:cpp/`): if false, C↔FPE ground truth shifts;
  resolve at M0 before M8.
- A-4 (f64 envelope): confirmed empirically at M3; if f32 were competitive it
  remains a Stage-2 question, not a Stage-1 change.