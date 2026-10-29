# Ferrite AAC encoder — consolidated numerical-divergence triage

Date: 2026-10-27. Status: **diagnostic and prioritization only; no production
change; no commit or push.**

Scope: consolidate the existing Rust-versus-FDK-AAC investigations for the
native AAC-LC VBR-5 encoder, separate verified causes from unresolved work,
and name the next bounded investigation. This report does not replace the
detailed reports; it references their evidence and records where they have
been superseded.

## 1. Executive summary

**Verdict: PASS WITH FINDINGS.**

No **verified porting defect** was found in the inspected numerical paths.
Every decision-changing divergence with a traced cause is either:

1. a **confirmed numerical divergence** caused by the documented float-native
   front end versus FDK's fixed-point reference, or
2. **upstream propagation** from such a difference into a later stage whose
   own rule is exact on equal inputs.

The three findings that matter most for prioritization:

- **Only one cross-frame state divergence has a demonstrated coded-output
  consequence:** the speech-clean block-switch case. FDK's f106
  `accWindowNrg` is 450 versus Rust's 497, and the f107 attack margin is
  +11.1 versus −32.5, producing START/SHORT/STOP versus LONG. ATS/S2
  cross-frame drift is real but was output-neutral under C-state injection
  over the tested horizons.
- **The TNS D-013 prediction-gain residual is narrower than the M5.25 audit
  states.** FDK uses `predictionGain` for the TNS activation test and for PNS
  detection (PNS is disabled in this profile); `FDKaacEnc_TnsEncode` filters
  with LPC coefficients, not with the prediction gain. Rust reads
  `TnsChannel.gain` only in tests. D-013 is therefore an activation-margin
  question, not a spectral-scale question. No activation flip exists in the
  committed `psy_*`/`CASES` window set.
- **D-021 is closed by its dedicated archive as a harness/reference
  discrepancy**, not a production encoder defect. The M5.26 report still lists
  it as open because M5.26 predates the closure record. The current TNS
  C-exact-RXX test independently corroborates the TNS part of that closure.

The pre-census highest-priority unresolved item was the **D-013
activation-margin census**: a same-input arithmetic residual sits on a
discrete TNS activation boundary, and the committed `rxx_*`/`psy_*` vectors
make the check cheap and self-contained. It was more bounded than a
rate-control trace and more decision-relevant as an open question than the
already-accounted MS band-16 residue.

**Post-census status (2026-10-27).** The D-013 census is complete: 124 HI and
4 LO committed windows, maximum observed residual 11, maximum
decision-relevant residual 1, minimum HI margin 54, minimum LO lower/upper
margins 343/78,843, and zero literal, policy-gate, exact-crossing, or actual
activation flips. **D-013 is closed for the committed corpus.** See
[`aac-enc-tns-d013-activation-margin-census.md`](aac-enc-tns-d013-activation-margin-census.md).
The pre-census ranking below is retained as investigation history.

**Post-audit rate-control status (2026-10-27).** The follow-up audit closes
the rate-control/bit-reservoir candidate for the VBR-5 path. `bitResMode` is
`FULL`, `bitResTot` is pinned to its maximum every frame, the grant derives
from `maxBitsPerFrame`, and the persistent `DistributeState` products are not
consumed by VBR threshold adaptation. All 2,442 committed-corpus frames are
single-pass with zero gain adjustments and positive margins; no local
rate-control defect or output-changing cross-frame propagation was found. See
[`aac-enc-rate-control-divergence-audit.md`](aac-enc-rate-control-divergence-audit.md).
No new C QC capture is justified for this configuration. The remaining
known output-changing cross-frame divergence is the already-attributed
block-switch case, not rate control.

## 2. Scope and repository baseline

### 2.1 Repository state

| Item | Value |
|---|---|
| HEAD | `c4d1262` (`refactor(aac-enc): remove redundant encoder bookkeeping`) |
| Branch | `main` |
| Worktree | Dirty: prior investigation reports/evidence/tests plus unrelated pre-existing edits; nothing staged, committed, or pushed |
| Encoder profile | AAC-LC, VBR-5, afterburner on, PNS off, ADTS, 1024-sample frames |
| Validation platform | Apple Silicon macOS, `rustc`/`cargo` 1.99.1 |

The prior investigation artifacts are present and were read in place. The
uncommitted worktree is a mix of those investigation artifacts (reports,
evidence, and focused tests) and unrelated pre-existing edits in the same
files; this triage did not attempt to attribute every hunk. Pre-existing
unrelated changes include `docs/aac-enc-m5b-objective-quality-baseline.md`,
`docs/aac-enc-quality-divergence-audit-m5.25.md`,
`tools/aac-enc-metrics/challenge/listening-review-worksheet.md`, and parts of
`rust/src/aac_enc.rs`, `block_switch.rs`, `psy.rs`, `stream_driver.rs`,
`tns.rs`, and `production_baseline.rs`. They were preserved untouched.

This triage made **no production-code change**. It added this report and one
`docs/README.md` entry only.

### 2.2 Reports and evidence reviewed

| Investigation | Report | Evidence | Current disposition |
|---|---|---|---|
| MDCT f32 vs f64 | `docs/aac-enc-mdct-f64-precision-experiment.md`; `docs/aac-enc-mdct-f64-closeout.md` | `docs/evidence/aac-enc-mdct-f64/` | Closed: retain f32; experimental code removed |
| Block switch, speech-clean f107–f109 | `docs/aac-enc-block-switch-oracle-investigation.md` | `docs/evidence/aac-enc-block-switch/` | Closed as numerical; material sequence effect; monitor |
| TNS D-012 / D-013 | `docs/aac-enc-tns-oracle-investigation.md`; `docs/aac-enc-tns-d013-activation-margin-census.md` | `docs/evidence/aac-enc-tns/`; `docs/evidence/aac-enc-tns-d013/` | D-012 closed as upstream numerical; D-013 closed by census (no crossing) |
| MINSNR1 / S2 | `docs/aac-enc-minsnr1-oracle-investigation.md` | `docs/evidence/aac-enc-minsnr1/` | Closed as upstream; no MINSNR1-only output effect demonstrated |
| Cross-frame state | `docs/aac-enc-cross-frame-state-drift-investigation.md` | `docs/evidence/aac-enc-cross-frame-state-drift/` | Closed for tested horizons; ATS/S2 output-neutral |
| D-021 short-frame replay | `docs/research/d021/D021-CLOSURE-RECORD.md`; `docs/research/d021/README.md` | `docs/research/d021/` archive | Closed as harness/reference discrepancy |
| Earlier audits | `docs/aac-enc-quality-divergence-audit-m5.25.md`; `docs/aac-enc-first-divergence-m5.26.md` | In-report tables | Historical, partially superseded |
| Quality/bitrate baseline | `docs/aac-enc-m5b-objective-quality-baseline.md` §9 | Temporary V3 metrics (summarized in report) | Current output is pinned to this baseline by `production_baseline.rs` |

The earlier audits were written at revision `e4258d2a` (2026-10-16/17). The
current production-baseline test reproduces the V3 byte lengths and hashes, so
the V3 quality numbers still describe the current default bytes on the
reference platform. They are **not** re-measured here and are not a perceptual
claim.

### 2.3 Baseline validation

| Check | Actual result |
|---|---|
| Full library suite with reference artifacts | **340 passed, 0 failed** |
| Full library suite without environment variables | **330 passed, 10 failed** (documented artifact-dependent set; see §8) |
| Focused regressions | all pass: cross-frame, MINSNR1, TNS C-exact, block-switch, production baseline, MS band-16 |
| Doctests | 19 passed |
| `aac-convert` | all executed suites pass; existing ignored tests unchanged |
| Licensing | pass, 67 `rust/src` files carry the modified-version notice |
| D-021 manifest | 89 checked, 0 failures |

## 3. Consolidated triage table

Classification uses the task's vocabulary. "Confirmed" means a direct
measurement plus an exact replay, counterfactual, or bit accounting;
"strongly supported" means measured and source-consistent without a
counterfactual; "unresolved" means the causal chain is not isolated.

| Divergence | Earliest cause | Classification | Coding-decision impact | Output impact demonstrated | Confidence | Next action |
|---|---|---|---|---|---|---|
| MDCT f32 vs f64 (closed experiment) | f64 transform versus f32; shared f32 coefficient tables dominate the residual (`aac-enc-mdct-f64-precision-experiment.md` §5–§6) | No material consequence established within tested scope | f64 changed boundary decisions (42 PE frames, 17 totals, 3 TNS, 2 MS, 2 gains) but with no consistent direction | 9/18 decoded streams differ at 66.8–101.5 dB; full-band SNR Δ ≤0.003 dB; 14–15% slower | High | **Close** |
| Block switch, `speech-clean` f107–f109 | f106 `accWindowNrg` 450 vs 497; f106 `windowNrgF[7]` 992 vs 1048; f107 window-0 margin +11.1 vs −32.5 (`aac-enc-block-switch-oracle-investigation.md` §3) | Confirmed numerical divergence | **Yes:** C START/SHORT/STOP vs Rust LONG | **Yes:** sequence and grouping; f107–f110 276/267, 339/340, 268/304, 268/264 B; full stream −71 B (−0.055%); local SNR mixed | High | **Monitor** |
| TNS D-012 order/coefficient (40 pinned windows) | f32 MDCT/Q31 spectrum → RXX2 difference; C-exact RXX reproduces C order/coefs/activation (`aac-enc-tns-oracle-investigation.md` §5–§6) | Confirmed numerical divergence / upstream propagation | **Yes** on active-filter cases; activation unchanged in the compared `psy_*` windows | **Yes:** onset f24 +28 B (static +18); f25/f26 sizes equal; local SNR mixed | High | **Monitor** |
| TNS D-013 prediction-gain residual | Float Schur versus fixed `CLpc_AutoToParcor` on C-exact RXX; ≤11 at gain 22,154 (5×10⁻⁴) (`aac-enc-tns-oracle-investigation.md` §1, §5; `aac-enc-tns-d013-activation-margin-census.md`) | Confirmed numerical divergence | **Activation only** (`predictionGain > threshOn`; PNS off); census: no activation flip, minimum margin 54 vs max decision residual 1; not a spectral scale | None demonstrated; gain gate is `8 + \|want\|/1000` | High for the committed corpus | **Close** (census: no crossing) |
| MS band-16 (sine f5; onset f26) | Stored operands bit-identical; Ld lane within envelope; margin shifts exactly −2,539,383 to 0 (`phase1e_sine_f5_band16_residue`; `aac-enc-minsnr1-oracle-investigation.md` §5) | Upstream propagation | **Yes:** one MS mask band | **Yes:** sine f5 payload 52 vs 51 B; onset f26 payload byte 4; no perceptual claim | High | **Monitor** |
| MINSNR1 / S2 values | Upstream `SENL`/`PENLD` plus the MS band-16 mask; S2 exact on C inputs (`aac-enc-minsnr1-oracle-investigation.md` §1, §5) | Upstream propagation | Values/`THRV` remained in envelope; no coded effect | None demonstrated; `quiet-sine f5` byte-identical; ±1/2 B changes confounded | High | **Monitor** |
| ATS/S2 cross-frame drift (onset f25/f26; sweep f3–f10) | f25/f3 S1 energy/form-factor differences; PE exact on C inputs (`aac-enc-cross-frame-state-drift-investigation.md` §3–§5) | Upstream propagation / no material consequence within tested scope | None observed | **None:** C-state injection leaves f26–f28 and f4–f10 bytes and coded fields unchanged | High for tested scope | **Close** |
| Pre-echo and transform overlap state | `sfbThresholdnm1`/`mdctScalenm1`/`calcPreEcho`; ring/MDCT overlap (`aac-enc-cross-frame-state-drift-investigation.md` §2–§3) | No material consequence established within tested scope | None | **None:** pre-echo exact on onset f22–f26; ≤25 Q31 elsewhere; ring schedule exact; MDCT tolerance-gated | High | **Close** |
| Maxscale ±1 flips (D-011) | clz boundary / Q31 headroom; `finalShift` equal in studied cases (`aac-enc-quality-divergence-audit-m5.25.md` D-011) | No material consequence established within tested scope | None demonstrated | None; 21 pinned flips, representative frame 7 up/2 down, stored energies algebraically invariant | High for studied frames | **Close** |
| D-021 oracle short-frame replay blips | Oracle replay versus real `aac-enc` on SHORT frames; stale MS/S6 staging; MDCT residue ~1×10⁻⁴ (`docs/research/d021/D021-CLOSURE-RECORD.md`) | Harness/reference discrepancy | None in production; limits oracle attribution | No production divergence demonstrated; four cases reconciled (+224/+56/−8/0 payload bits) | High (archive); provenance caveats | **Close** |
| Aggregate payload divergence (D-051) | Sum of the above front-end/boundary decisions; 16/18 fixtures differ (`aac-enc-quality-divergence-audit-m5.25.md` §3.7) | Confirmed numerical divergence (aggregate) | **Yes,** by construction | **Yes:** payload sizes; V3 codec SNR within 1.3 dB of C (17/19 ≤0.5 dB), bitrate −2.5%/+2.0% | High | **Monitor** |
| Rate-control / bit-reservoir propagation | Pinned `FULL` reservoir; grants from `maxBitsPerFrame`; `DistributeState` products unused by VBR `AdjustThresholds` (`aac-enc-rate-control-divergence-audit.md` §2) | No material consequence established / closed for VBR-5 | None attributable; 2,442 frames single-pass, zero gain adjustments, positive margins | None established; frame-size differences start at/after payload differences | High for the tested VBR-5 corpus | **Close** (no C QC capture needed) |

### 3.1 Causal tests and regression coverage

| Item | Causal test or counterfactual | Regression coverage |
|---|---|---|
| MDCT f32 vs f64 | Controlled f64 transform experiment; 19-fixture differential encode/decode (`aac-enc-mdct-f64-precision-experiment.md` §4–§7) | `production_baseline::production_f32_corpus_baseline_is_unchanged`; experiment code removed |
| Block switch f107–f109 | Focused C probe replaying `FDKaacEnc_BlockSwitching`; Rust state trace (`aac-enc-block-switch-oracle-investigation.md` §2–§3) | `block_switch::tests::speech_clean_f107_attack_boundary_is_float_limited` |
| TNS D-012 | C-exact RXX replayed through Rust analysis; order/coefs/activation comparison (`aac-enc-tns-oracle-investigation.md` §5) | `tns::tests15::c_exact_rxx_reproduces_c_tns_analysis`; `psy::tests::psy_end_to_end_matches_oracle` |
| TNS D-013 | Same-input float-Schur gain comparison; C-exact-RXX analysis; 128-row activation-margin census (`aac-enc-tns-d013-activation-margin-census.md`) | `tns::tests_gain_gate::gain_gate_policy`; `tns::tests_gain_gate::schur_gains_match_committed_within_gate`; `tns::tests_gain_gate::d013_activation_margin_census`; `psy_end_to_end` |
| MS band-16 | Exact margin transcription and operand counterfactual (`phase1e_sine_f5_band16_residue`) | `stream_driver::phase1_tests::phase1e_sine_f5_band16_residue`; `minsnr1_divergence_is_upstream_of_s2` |
| MINSNR1 / S2 | Replay on captured production inputs and on C `SENL`/`PENLD`/MS mask (`aac-enc-minsnr1-oracle-investigation.md` §5) | `threshold_vbr::tests::s2_full_matches_oracle_with_injected_chaos`; `minsnr1_divergence_is_upstream_of_s2` |
| ATS/S2 cross-frame | C f25/f3 ATS/chaos state injection before f26/f4; byte/field comparison (`aac-enc-cross-frame-state-drift-investigation.md` §6) | `stream_driver::phase1_tests::cross_frame_state_onset_f25_is_upstream_and_output_neutral` |
| Pre-echo / transform | Onset f22–f26 state comparison; ring schedule and MDCT parity | `transform::tests::ring_schedule_matches_oracle`; `transform::tests::mdct_parity_with_reference`; `psy_end_to_end` |
| Maxscale D-011 | Pinned clz-boundary flips and neutrality argument | `psy_end_to_end_matches_oracle` with `KNOWN_MAXSCALE` |
| D-021 | Archive's 17-link causal audit, four-case bit reconciliation, C `mdct_block` harness (`docs/research/d021/`) | Archive manifest (89/0); `phase1e_*`/`phase1f_*` real-C checks |
| Aggregate D-051 | Full-corpus differential encode; V3 quality baseline | `production_baseline`; V3 baseline report |
| Rate-control / bit-reservoir | C S6 exact replay plus live 2,442-frame census; source-level VBR call-graph audit (`aac-enc-rate-control-divergence-audit.md` §2–§5) | `rate::tests::s6_corpus_exact`; `s6_grant_state_evolution_onset_chain`; `s6_padding_chain_matches_oracle`; `m5g_branch_census`; `production_baseline` |

### 3.2 What the table does not say

- A state mismatch is not automatically a coding-decision mismatch. The
  `chaosMeasureEnFac` case is the clearest example: the value differed by
  ~0.6% at onset f25, but the field is overwritten by the next frame's
  `calcWeighting`; it is not a history input.
- A coding-decision mismatch is not automatically a quality regression. The
  block-switch and TNS cases change discrete decisions; the local SNR evidence
  is mixed and is not perceptual evidence.
- Output neutrality over a bounded injection horizon does not prove
  neutrality over arbitrary horizons. The ATS/S2 conclusion is explicitly
  limited to the tested frames.

## 4. Ranked investigation candidates

Ranking uses the task's criteria: reproducibility, proximity to a coding
decision, demonstrated downstream effect, breadth, cost of a causal
experiment, and destabilization risk. No numerical severity score is implied.
The two candidates that remained after the earlier audits (D-013 and
rate-control/bit-reservoir) are now closed for the tested corpus; the
pre-audit rationale is retained below as investigation history.

### 4.1 D-013 TNS activation-margin census — **Closed by census**

**Status (2026-10-27).** The census is complete and found no crossing:
124 HI + 4 LO windows, max observed residual 11, max decision-relevant
residual 1, minimum HI margin 54, no literal/gate/exact candidates, no
activation flips. D-013 is closed for the committed corpus. The pre-census
rationale below is retained as investigation history.

**Pre-census rationale (superseded).** It was the only *unresolved*
same-input arithmetic residual that sits on a discrete TNS decision boundary.
The residual is small
(≤11 gain units at 22,154, 5×10⁻⁴ relative) and the activation test is a
strict comparison (`gain > threshOn`) plus a coefficient-energy alternative.
Committed `rxx_*`/`psy_*` vectors make a margin census cheap and
self-contained; no new C oracle is needed. It is more decision-relevant as an
open question than the MS band-16 residue (already exactly accounted) and far
cheaper than a rate-control trace.

**What is already resolved.** Activation is exact for every window compared
by `psy_end_to_end_matches_oracle` (its `CASES` fixture/frame set); the five
representative C-exact-RXX cases reproduce C activation. No flip is known.
The open part is the **headroom**: whether any committed window lies close
enough to the threshold that the residual could flip activation, and whether
the gain gate is hiding such a case.

**Source-level correction.** The M5.25 audit's D-013 row says the filtered
spectrum is scaled by this gain. That is not what the reference does:
`FDKaacEnc_TnsEncode` applies `LpcCoeff` from `CLpc_ParcorToLpc` with its
fixed-point `lpcGainFactor` exponent, not the TNS `predictionGain` statistic;
`predictionGain` is read by the activation test and by `PnsDetect`
(`psy_main.cpp:1152`), and PNS is off (`NO_NOISE_PNS`). Rust's
`TnsChannel.gain` is test-only. This correction lowers D-013's severity; it
does not erase the activation-margin question.

### 4.2 Rate-control / bit-reservoir propagation — **Closed for VBR-5**

**Post-audit status (2026-10-27).** The audit found no conventional reservoir
in this path: `bitResMode = FULL`, `bitResTot` is pinned to `bitResTotMax`
every frame, and `BitResRedistribution` yields a zero element level. The VBR
grant is derived from `maxBitsPerFrame`, not from a carried balance. The
persistent `DistributeState` fields are updated but only feed `grantedPe`/
`grantedPeCorr`, which VBR `AdjustThresholds` does not consume. All 2,442
corpus frames are single-pass with zero gain adjustments, no crash, and
minimum margins of 986 bits (grant), 1,042 (maxDyn), 984 (frame), and 1,045
(cap). The exact S6 corpus and onset state chain reproduce C on equal inputs.
No C QC capture is justified for this configuration. See
[`aac-enc-rate-control-divergence-audit.md`](aac-enc-rate-control-divergence-audit.md).

**Pre-audit rationale (superseded).** Once frame sizes differ, `bitResTot`,
`paddingRest`, `peLast`, and `dynBitsLast` necessarily differ. These fields
feed later grants and could turn a local frame-size difference into a
longer-lived allocation difference. The cross-frame investigation inventoried
them but did not trace them, and the retained dumps inspected here contain no
QC state (`bitRes`/`paddingRest`). The smallest useful experiment was thought
to require a C-side QC capture for a fixture with a demonstrated frame-size
divergence (`speech-clean` f107 or `onset` f24). The audit showed that the
first premise is true only as a record of computed values and the second is
false for this VBR-5 path.

### 4.3 MS band-16 Ld provenance — **Monitor**

**Why it is not higher.** The sine f5 band-16 case is already explained
quantitatively: stored operands are bit-identical, the Ld deltas are inside
the committed energy envelope, and the measured shift of −2,539,383 is exactly
what flips the strict `>` margin from +2,539,383 to 0. The onset f26 case is
the same mask band, already captured by the MINSNR1 test. There is no missing
operand and no rule mismatch. Only the general provenance of the float Ld
residue is outside the committed vectors, which is the same class as every
other front-end boundary.

### 4.4 TNS D-012 active-filter quality impact — **Monitor**

**Why it is not higher.** The causal chain is closed: C-exact RXX reproduces
C order, coefficients, and activation; the production difference is upstream
RXX2. Active filters on both sides change the encoded spectrum and static
bits (onset f24 +28 B), so the output effect is real. What is missing is a
perceptual/controlled-quality comparison, not a correctness test. The f64
experiment already showed that more float precision alone does not close the
FDK gap; a TNS-only patch would compensate for upstream spectrum residue. The
project's acceptance target does not require bit-identical FDK output.

### 4.5 Sweep and onset-441 quality observations — **Monitor**

**Why they are not higher.** The sweep ATS drift is now traced to upstream
S1 energy/form-factor inputs and is output-neutral under injection; the
remaining sweep payload gap is distributed and is an instance of D-051. The
onset-441 f22 gap favors Rust in the V3 baseline (1.27 dB codec SNR) and has
no localized defect or block-structure difference; it is a single-fixture
quality observation, not a correctness finding.

### 4.6 D-021 oracle MS/S6 staleness — **Close**

**Why it is closed.** The dedicated archive's disposition is "expected
numerical difference, no independently demonstrated defect." The stale oracle
SHORT-frame MS/S6 staging was invalidated as ground truth and excluded from
later conclusions. The current tests use real-C captures where the oracle is
unfaithful. The archive's remaining uncertainties (sine 8-bit split,
RXX→order swap, within-FFT apportionment) are non-blocking and do not
establish a defect. This triage did not re-run the archive's probes; the
current TNS C-exact test corroborates the TNS portion.

## 5. Recommended next investigation (completed for D-013 and rate control)

**Status: completed 2026-10-27.** The D-013 census found no crossing: 124 HI
and 4 LO windows, maximum observed residual 11, maximum decision-relevant
residual 1, minimum HI margin 54, and zero literal/gate/exact candidates or
activation flips. See
[`aac-enc-tns-d013-activation-margin-census.md`](aac-enc-tns-d013-activation-margin-census.md).
The executed test also covered the LO upper gain bound and the two-sided LO
sum branch, which the pre-census single-threshold sketch omitted.

The follow-up rate-control audit found no conventional reservoir or
output-changing QC feedback in VBR-5. The pre-audit “needs C QC capture”
candidate is closed for this path; see
[`aac-enc-rate-control-divergence-audit.md`](aac-enc-rate-control-divergence-audit.md).
The D-013 method below is retained as the executed record. No further
numerical investigation is justified by the current evidence; a future C
capture would be scoped to CBR/reduced-reservoir or multi-element paths, not
this VBR-5 corpus.

### 5.1 Hypothesis

On the committed TNS corpus, the float-Schur prediction-gain residual cannot
flip a TNS activation decision, because every activation decision has more
margin than the measured residual; the only unresolved part is the size of
that margin.

### 5.2 Smallest useful experiment

Extend the existing `tns::tests_gain_gate` corpus test (or add a sibling
test) to compute, for every committed `rxx_*` window:

```text
d_gain = |gain_rust - thresh_on|
d_sum  = |sum_sqr_rust - (limit_order/2 + 2)|
r_gain = |gain_rust - gain_committed|
```

The activation rule is:

```text
active = (gain > thresh_on) || (sum_sqr > limit_order/2 + 2)
```

Use the committed `TNSG` gain as the C reference (integer ×1000; allow a +1
rounding guard). Report the minimum `d_gain` and `d_sum` and every window
where `d_gain <= r_gain + 1` and the sum branch is false.

### 5.3 Falsifiable observations

- **No window is within the residual envelope.** Then D-013 cannot flip
  activation on the committed corpus; record the margin bound, keep the
  existing gate, and close D-013 for the available evidence. Remaining
  uncertainty is out-of-corpus only.
- **A window is within the envelope.** Replay `detect` with the C-exact gain
  (and C-exact coefficients) and compare `active`, `num_filters`, static bits,
  and the filtered spectrum. If activation flips, that is a boundary
  numerical divergence; pin it and recompute the D-012/D-013 impact. If it
  does not flip, the tolerance still has headroom on that window.
- **The sum branch already activates the filter.** The gain residual is
  irrelevant for that window; exclude it from the census.

### 5.4 Cost and stop condition

The experiment needs only committed fixtures and vectors, adds no production
path, and should run in seconds. Stop after the census plus at most one
C-gain replay per candidate window. Do not broaden to a new C oracle or a
full TNS quality study; those are separate, more expensive questions.

## 6. Items explicitly closed or deferred

### 6.1 Closed

| Item | Disposition |
|---|---|
| f64 MDCT experiment | Retain f32; no measured quality/agreement benefit; ~14–15% cost; experimental code removed and production baseline green. |
| Block-switch f107–f109 cause | Float-native Q31 truncation versus f32 accumulation; no branch/table/state defect; keep boundary test. |
| TNS D-012 cause | Upstream RXX2; TNS arithmetic faithful on C-exact RXX; keep C-exact test. |
| TNS D-013 cause | Activation-margin census: 124 HI + 4 LO windows, max decision-relevant residual 1, min HI margin 54, zero candidates/flips; closed for the committed corpus. |
| Rate-control / bit-reservoir | Closed for VBR-5: pinned FULL reservoir, cap-derived grants, inert QC grant state; 2,442 frames single-pass with zero gain adjustments; no C QC capture justified. |
| MINSNR1 / S2 cause | Upstream `SENL`/`PENLD` plus MS band-16; S2 exact on C inputs; keep attribution test. |
| ATS/S2 cross-frame cause | Upstream S1 inputs; PE/S2 exact on C inputs; C-state injection output-neutral on tested horizons. |
| Pre-echo / transform state | Exact on onset transition; ≤25 Q31 residue elsewhere; ring schedule exact; no decision effect found. |
| Maxscale ±1 (D-011) | Representational; `finalShift` equal in studied cases; no decision effect. |
| D-021 blips | Harness/reference discrepancy; closure record; stale oracle MS/S6 excluded. |
| Aggregate D-051 cause class | Expected consequence of the above; V3 quality baseline bounds codec-SNR/bitrate deltas; no new root cause. |

### 6.2 Deferred (not numerical-divergence work)

| Item | Why deferred |
|---|---|
| PNS / intensity-stereo live paths (D-014) | Not activated on the committed corpus; C-vs-Rust behavior unvalidated. A coverage gap, not a demonstrated divergence. |
| Retry / crash recovery (D-040) | No corpus retry/crash; the Rust API may drop a frame rather than encode it. Out of this triage's numerical scope. |
| Out-of-contract panics and 960-granule/multi-element paths (D-022/D-080/D-081/D-084) | Production hardening / API scope, not numerical parity. |
| Other rates/bitrates and general `MINSNR0` derivation | Rust uses committed snapshots at the validated points; generalization needs new snapshots. |
| Perceptual validation | No PEAQ/ViSQOL/listening test exists; byte equality, frame sizes, and SNR are not perceptual evidence and are not treated as such. |
| S1/MS test-capture in the production path | Pre-existing test-observability capture; behavior-neutral but clones vectors every frame. Cleanup is a performance task, not a numerical divergence. |
| `m55_streams_match_c_reference` fallback | Artifact-dependent test passes without env only because its hard-coded `M55_C_DIR` fallback exists; test-hygiene issue, not encoder behavior. |
| Rate-control / bit-reservoir trace | Completed for VBR-5; see §4.2 and the audit report. A capture remains future work only for CBR/reduced-reservoir or multi-element paths. |

## 7. Report and evidence discrepancies

These are recorded rather than silently reconciled.

| Observation | Current evidence | Disposition |
|---|---|---|
| MDCT experiment §9 reports **345/345** feature-build and **334/334** default-build results; the closeout reports **339/339** and **329/10**. | The 345/334 run was a different feature/default build configuration at an intermediate tree; the post-census current `--lib` run is 340/340 with artifacts and 330/10 without (pre-census 339/329). | Counts are revision-specific, not contradictory. Use the current post-census run for this triage. |
| M5.26 lists D-021 as unresolved; M5.25 treats it as a harness limitation. | `docs/research/d021/D021-CLOSURE-RECORD.md` (archived 2026-10-20 at `e4258d2a`) closes it as an expected numerical difference with no independently demonstrated defect. | D-021 is treated as closed here. The M5.26 ranking predates the archive; do not silently rewrite it. The archive was produced at `e4258d2a`, not current `c4d1262`, and was not re-run. |
| M5.25 D-013 says the filtered spectrum is scaled by the prediction gain. | `FDKaacEnc_TnsEncode` uses `CLpc_Analysis` with `LpcCoeff` from `CLpc_ParcorToLpc`; `predictionGain` is used by activation and `PnsDetect` (PNS off). Rust's gain is test-only. | Source correction recorded; D-013 severity reduced to the activation margin. |
| M5.26 says the sweep f10 ATS drift origin is inconclusive. | `aac-enc-cross-frame-state-drift-investigation.md` traces the first persistent ATS divergence to f3, reproduces it from C energy inputs, and shows C-state injection leaves f4–f10 unchanged. | Superseded for the ATS/chaos question; the residual quality observation remains monitored. |
| M5.26 hypothesizes onset f26 `TNS → PE → chaosMeasureEnFac → ATS pre`. | The cross-frame trace shows f25 ATS post is already upstream-driven by C energy/form-factor inputs; f26 S1 post reconverges. `lastEnFacPatch` is exact (0), and `chaosMeasureEnFac` is not a history input. | Hypothesis narrowed: the drift enters with f25 inputs, not an f26 state-update defect. |
| "Without external artifacts" is described as a clean no-artifact run. | On this machine `phase0-20261015/{oracle,aac}` and `m55/aac` exist; the phase0 dirs are incomplete, and `m55_streams_match_c_reference` passes via its hard-coded fallback. | The 330/10 count is environment-specific. It is not a clean-checkout guarantee. With env set, 340/340. |

## 8. Validation commands and actual results

### 8.1 Current suite

| Command | Result |
|---|---|
| `PHASE1_ORACLE_DIR=… PHASE1_C_DIR=… AAC_DEC_BIN=… M55_C_DIR=… cargo test --locked --features aac_enc --lib` | **340 passed, 0 failed, 0 ignored** |
| `cargo test --locked --features aac_enc --lib` (no env) | **330 passed, 10 failed** |
| `cargo test --locked --features aac_enc --doc` | **19 passed** |
| `cargo test --locked -p aac-convert` | all executed suites passed; existing ignored tests unchanged |
| `rust/tools/check-licensing.sh` | pass; 67 `rust/src` files carry the notice |
| `python3 docs/research/d021/verify_manifest.py` | **89 checked, 0 failures** |
| `git diff --check -- <changed paths>` | pass |
| `grep` for `#[ignore]` / `state_drift_` / diagnostic names in `src/aac_enc` | none |

The 10 failures without env are exactly:

```text
aac_enc::m5a_decode::m5a_tests::m5a_decode_check
aac_enc::stream_driver::phase1_tests::phase1_frontend_agreement_sine_f5
aac_enc::stream_driver::phase1_tests::phase1_full_stream_parity
aac_enc::stream_driver::phase1_tests::phase1e_onset_f24_bands_causality
aac_enc::stream_driver::phase1_tests::phase1e_sine_f5_band16_residue
aac_enc::stream_driver::phase1_tests::phase1f_multitone_f1_no_live_divergence
aac_enc::stream_driver::phase1_tests::phase1f_stepdown_f1_no_live_divergence
aac_enc::stream_driver::phase1_tests::phase1g_long_flips_residue
aac_enc::stream_driver::phase1_tests::phase1i_multitone_f5_tier_b
aac_enc::stream_driver::phase1_tests::phase1i_sine44_f5_tier_b
```

`m55_streams_match_c_reference` is **not** in that list because its hard-coded
fallback `M55_C_DIR` directory exists on this machine; it is still
artifact-dependent and would fail on a clean machine without the env var.
See §7 for the documentation discrepancy.

### 8.2 Focused regressions

| Test | Result |
|---|---|
| `aac_enc::stream_driver::phase1_tests::cross_frame_state_onset_f25_is_upstream_and_output_neutral` | pass |
| `aac_enc::stream_driver::phase1_tests::minsnr1_divergence_is_upstream_of_s2` | pass |
| `aac_enc::tns::tests15::c_exact_rxx_reproduces_c_tns_analysis` | pass |
| `aac_enc::block_switch::tests::speech_clean_f107_attack_boundary_is_float_limited` | pass |
| `aac_enc::production_baseline::production_f32_corpus_baseline_is_unchanged` | pass |
| `aac_enc::stream_driver::phase1_tests::phase1e_sine_f5_band16_residue` (with oracle env) | pass; reports "stored exact, Ld in-envelope, shift −2,539,383 to margin 0" |

### 8.3 Test/evidence integrity review

- **Production baseline determinism:** the test encodes all 19 fixtures
  through the default path, pins byte lengths on every platform, and pins
  FNV-1a hashes on macOS/aarch64. It passes on this reference platform. It
  does not depend on external artifacts.
- **Cross-frame injection:** the baseline `run(false)` asserts production
  ATS/chaos and pre-echo state against committed values; only the second
  `run(true)` injects C state. The test cannot pass by replacing production
  behavior with C values.
- **Narrow scoping:** the block-switch test pins the float-side f107 boundary
  and the recorded C-side margin; the TNS test feeds C-exact RXX; the MINSNR1
  test replays on captured inputs. None freezes every C/Rust mismatch.
- **Temporary instrumentation:** no ignored diagnostic tests or `state_drift_`
  probes remain. The new S2 capture is `#[cfg(test)]`-gated. The pre-existing
  S1/MS capture is **not** gated and is constructed in `step_frame`, which the
  production `Encoder` uses; it is behavior-neutral (M6 byte-equality test
  passes) but does per-frame vector cloning. Flagged for a separate
  performance cleanup, not changed here.
- **External artifacts:** the 340/340 result is the meaningful full-suite
  result. The 330/10 result is not a clean no-artifact baseline because of
  the fallback paths above; it must not be reported as "all tests pass
  without artifacts."

## 9. Limitations and unsupported conclusions

- **No perceptual evidence.** No PEAQ/ViSQOL/listening test exists. Byte
  equality, frame sizes, SNR, and internal-state agreement are not treated as
  perceptual evidence. The V3 quality numbers are descriptive waveform
  metrics only.
- **Bounded counterfactuals.** The ATS/S2 injection covers onset f26–f28 and
  sweep f3–f10 only. Output neutrality over those horizons does not prove
  neutrality for every future frame.
- **D-013 census coverage.** The census is complete for the committed corpus
  (124 HI + 4 LO windows), but it has no LO-active case and does not
  generalize to untested signals or RXX vectors. The current claim is that
  no activation flip is observed or reachable under the measured residual
  envelope for those committed windows.
- **Rate-control audit scope.** The audit covers the validated VBR-5 path
  only. There is no production frame-by-frame C QC capture, but the VBR
  state is pinned or inert by source and the exact S6 vectors cover the
  same-input arithmetic; no CBR/reduced-reservoir or multi-element
  generalization is claimed.
- **D-021 archive is older than HEAD.** Its manifest integrity was verified
  (89/0), but its probes were not re-run at `c4d1262`. The current TNS
  C-exact test independently corroborates the TNS portion; the other links
  rely on the archive.
- **Pre-echo scan breadth.** Exactness was checked on onset f22–f26 and the
  19 committed S1 cases; the ≤25 Q31 residue is not a proof for every
  possible signal.
- **MS band-16 breadth.** The exact accounting covers sine f5 and the onset
  f26 mask use. It is not a general proof about every MS boundary.
- **No new C oracle.** This triage did not build or regenerate reference
  artifacts. The existing oracle dumps are the evidence base.
- **Report counts are revision-specific.** The f64 report's 345/334 and the
  current post-census 340/330 are different trees; see §7.
- **Scope of "no verified porting defect."** It applies to the inspected
  numerical paths and evidence. It is not a proof of correctness for
  unexercised paths (PNS/IS, retry/crash, out-of-contract inputs, other
  rates/bitrates).

## 10. Files added or modified

| File | Change |
|---|---|
| `docs/aac-enc-numerical-divergence-triage.md` | This consolidated triage report. |
| `docs/README.md` | Index entry for this report. |

No production code, test expectation, fixture, oracle artifact, or prior
report was modified. The worktree's pre-existing investigation changes were
preserved. No staging, commit, push, amend, reset, rebase, or history rewrite
occurred.
