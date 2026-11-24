# 08 — Performance and Memory Strategy

Date: 2026-11-16. Principle: **correctness first, then measure.** No
premature optimization; no sacrificing correctness/maintainability for
speculative gains. Baseline before change; measure after; budget-gated.

## 1. Baseline (verified, `01` §8)

Apple M5 arm64, release, production `Encoder` API
(`docs/aac-enc-performance-baseline.md` §12):

- Throughput: 19 fixtures + 40 s ≈ **460.5 ms median, ~198.6× realtime**
  (RTF 0.005036); 19-fixture aggregate 220.4 ms / 233.5×.
- Front-end self time: 26.70% corpus / 51.25% onset / 21.50% music /
  29.57% speech.
- **`psy::process_pair_reuse` ≈ 2.38% corpus self time** (5.28% onset).
  TNS stage 6.66% corpus; transform ~3.91%; block switch 2.93%.
- Quantization dominates (49.9% aggregate; `calc_sfb_dist` 24.4%).
- Retained optimization set +13.93% vs reconstructed baseline (§29).

**Implication**: FPE touches the psy value kernels, a subset of the ~2.4%
orchestration self time. Even a 2× slowdown of that subset is ~2% overall —
likely within noise. The dominant costs (quantization, TNS autocorr,
transform) are **unchanged** by FPE. Performance risk is low but must be
measured, not assumed.

## 2. What to measure

| Metric | Method | Tool |
|---|---|---|
| Total encoding throughput | production `Encoder` push/flush over corpus, median of N reps | existing bench harness (`quant_bench.rs`, `production_baseline.rs`, `examples/aac_bench_wav.rs`) |
| Psy processing time | per-stage `sample`/DWARF source-line profiling (method in baseline §21) | `sample` (macOS), perf (Linux) |
| Per-frame time | frame-loop timing, distribution (p50/p99) | bench harness |
| Memory allocations | allocation count/bytes per frame | instrumented allocator or `dhat`-style (test-only) |
| Scratch requirements | static sizeof `FpeScratch`/`FpeChannel` + peak heap | compile-time + test |
| Cache behaviour | where practical, perf counters | optional |
| SIMD opportunities | identify only; do not implement in Stage 1 | profiling |

## 3. Measurement plan (rev 4: fixed-sample, paired, interleaved, uncertainty-quantified)

The baseline run-to-run spread is ~±1.9%, so a naive "FPE median vs Q31 median
within ±2%" comparison cannot distinguish a real effect from noise. The
protocol below controls drift and quantifies uncertainty with a **fixed-sample
confirmatory design** (no optional stopping; §3.2), so the reported 95% CI is
valid for the sampling and stopping procedure used. **Total encoder throughput
and psychoacoustic-stage time are measured separately** (different instruments,
different acceptance interpretation).

### 3.1 Controls (identical conditions for both engines)

- **Same binary**: both engines compiled in one release build (runtime
  `PsyEngineKind` selection, ADR-0001); identical build profile, rustc, flags,
  LTO/codegen settings. No rebuild between arms.
- **Same corpus**: the 19 fixtures + derived 40 s workload; fixture hashes
  validated before and after (no input drift).
- **Same machine conditions**: fixed CPU governor/performance mode where
  available, machine idle, thermal state stabilized (warm-up runs discarded),
  no concurrent load; record machine/OS/thermal state. ARM64 primary; x86-64
  reported separately (A-5), never pooled.
- **Randomized, interleaved arms**: each repetition runs **both** Q31 and FPE
  back-to-back in a **randomized order** (coin flip per pair), so slow drift
  (thermal, background) affects both arms equally and cancels in the paired
  difference. Do not run all Q31 reps then all FPE reps.

### 3.2 Total encoder throughput (primary G6 metric) — fixed-sample design

**Design choice (rev 4): a fixed-sample confirmatory experiment is the Stage-1
default.** The earlier "start at 15 pairs, add pairs until the CI is decisive"
rule is **optional stopping**: repeatedly inspecting ordinary 95% CIs and
stopping at a favourable one inflates the false-decision rate and invalidates
the nominal 95% coverage. It is **removed**. The reported interval must be
valid for the sampling *and* stopping procedure actually used; a fixed-sample
design with a single pre-planned analysis guarantees this.

**Procedure (in order):**

1. **Pilot (variability estimation only).** Collect a small set of paired runs
   (e.g. 10–15 pairs) under the §3.1 controls. Use them **only** to estimate the
   paired-difference variability (SD `s_d`, and a robust scale such as the IQR
   for skew/heavy tails) and to assess serial dependence (§3.4). **No G6
   decision and no hypothesis test is made on the pilot.**
2. **Predetermine the final sample size `N`.** Choose a target 95% CI
   **half-width `h`** for the median paired % difference, predeclared **strictly
   below the budget margin** so a near-null effect is decidable within budget
   (recommended `h = 1%`, giving margin to the +2% boundary). Using the pilot
   `s_d`, the asymptotic median standard error is
   `SE_median ≈ √(π/2)·s_d/√N ≈ 1.2533·s_d/√N`, so a planning estimate is
   `N ≥ (z₀.₉₇₅·√(π/2)·s_d / h)² ≈ (2.4565·s_d / h)²`; round up and add a margin
   (e.g. ~20%) for non-normality and uncertainty in `s_d`. **Verify** the choice
   by simulating the order-statistic CI width at candidate `N` from the pilot
   distribution (the planning formula assumes approximate normality; the
   *reported* CI is the exact distribution-free one below, whose width depends
   on the empirical density at the median). If heavy tails make the target
   width unattainable at a feasible `N`, predeclare a larger `h`/`N` or accept
   that the result may be inconclusive — **do not** plan to "top up" later.
3. **Pre-register the analysis plan** (in the benchmark record / evidence log)
   **before** collecting confirmatory data: `N`, the pilot variability estimate,
   target `h`, the CI method(s), the test, the outlier rule, the
   serial-dependence check and remedy, and the G6 decision rule (§4).
4. **Collect confirmatory data**: exactly `N` analyzed pairs under §3.1
   controls (randomized interleaved Q31/FPE). The derived 40 s workload is
   preferred per run to shrink per-run variance. A small collection buffer is
   allowed so that, after the **predeclared** outlier exclusions (§3.4), the
   analyzed set equals the predeclared `N`; exclusions are logged and applied
   blind to the engine label where possible.
5. **Primary analysis — exactly once, at `N`.** Compute the median paired %
   difference and its **95% two-sided CI**, apply the §4 G6 rule **once**, and
   report. **If inconclusive, report INCONCLUSIVE** (§3.4); do not add pairs and
   recompute ordinary CIs as if coverage were preserved.

**Statistic and CI methods.** Per pair `d_i = 100·(t_FPE,i − t_Q31,i)/t_Q31,i`
(%). Report the **median paired percentage difference** with a **95% two-sided
CI for the median**:

- **Primary CI — distribution-free order-statistic (sign-based) interval.**
  Under iid continuous `d_i`, `[d_(k), d_(N−k+1)]` (order statistics) covers the
  median with probability `1 − 2·P(Bin(N, 0.5) ≤ k−1)`; choose the largest `k`
  with that tail ≤ 0.025 ⇒ **≥95% coverage, exact, no symmetry or normality
  assumption**. This is the decision interval. (Preserved from rev 2/3, now
  elevated to primary because it is exact at the predeclared `N`.)
- **Cross-check — bootstrap percentile CI** of the median, **≥10 000
  resamples** (resample pairs with replacement, recompute the median, take
  2.5th/97.5th percentiles). **Small-sample limitation:** the percentile
  bootstrap can have poor/uneven coverage for the **non-smooth median**
  statistic at small `N` (skewness, bias, discreteness); it is a cross-check,
  **not** the decision interval. BCa may be reported but does not remove the
  small-sample caveat; reliability improves at the larger fixed `N`. If serial
  dependence is present, an iid bootstrap is invalid — use the block-based
  remedy (§3.4).
- **Test — prefer the sign test** for "median difference ≠ 0"
  (`P(d>0)=0.5`), which assumes **no symmetry**. If a **Wilcoxon signed-rank**
  test is also reported, **document its additional assumption that the
  differences are symmetrically distributed** about the median; do not present
  it as assumption-free. The test is supplementary; the **CI is the G6 decision
  instrument**.

**Recorded quantities (mandatory).** Design (fixed-sample), predeclared `N` and
its rationale (pilot `s_d`, target `h`), the pre-registered analysis plan
reference, CI method(s) and which is primary, confidence level (95%, two-sided),
bootstrap resampling count, achieved analyzed pair count, number/reason of
excluded pairs, serial-dependence assessment result, tie/zero-difference counts
and effective sign-test `N′` (§3.6), median paired % difference, CI lower/upper
bounds, and the sign-test (and any Wilcoxon) p-value. Do not rely on a mean ± std
alone (timings are skewed).

**Acceptance interpretation** (§4): decide on the **95% CI relative to the +2%
regression budget**, not on a point estimate.

**Follow-up experiments.** If the confirmatory result is inconclusive and a
decision is still required, run a **separate confirmatory experiment** with its
**own predeclared `N` and analysis plan** (re-estimated from an updated pilot if
needed); its result stands on its own predeclared design. Do **not** treat a
follow-up as a continuation that preserves the original interval's coverage.

**Sequential alternative (not the Stage-1 default).** A sequential design is
acceptable **only** with a statistically valid procedure whose error control is
explicitly documented — e.g. a **confidence sequence** (anytime-valid, e.g.
nonnegative-supermartingale/betting CS) or a **group-sequential alpha-spending**
design (predeclared looks and spent α, e.g. O'Brien–Fleming/Pocock, applied to a
sign-test statistic for the median). Ordinary CIs re-inspected at informal
stopping points are **not** acceptable. If a confidence sequence targets the
mean rather than the median, state the estimand change explicitly.

### 3.3 Psychoacoustic-stage time (secondary, attribution)

- Source-line profiling (`sample` macOS / `perf` Linux, DWARF method in
  baseline §21) on a fixed workload, Q31 vs FPE, ≥3 profiles each.
- Report `psy::process_pair_reuse` / `fpe::process_pair_reuse` self-time share
  and the value-kernel breakdown. This metric is **noisier** (sampling) and is
  used for **attribution**, not as the G6 gate. A rise here is expected
  (float vs integer) and is acceptable if total throughput stays in budget.

### 3.4 Uncertainty, noise, serial dependence, and inconclusive results

- **Noise is not a result**: a paired difference whose CI includes 0 is
  "no detected difference", **not** a confirmed improvement or regression.
- **Inconclusive (no optional stopping)**: the G6 regression boundary is
  **+2%** (the −2% mark is an improvement-reporting threshold, not a regression
  boundary). If the 95% CI **straddles +2%** (lower bound ≤ +2% < upper bound),
  the result is **INCONCLUSIVE** → **report it as inconclusive** with the CI.
  **Do not** repeatedly add measurements and reinterpret ordinary CIs as if the
  original 95% coverage were preserved (§3.2). If a decision is still required,
  run a **separate confirmatory experiment** with its own predeclared `N` and
  analysis plan (§3.2). Never classify noise as a confirmed
  regression/improvement, and never declare G6 passed on an inconclusive CI.
  (A CI straddling −2% but wholly ≤ +2% is still G6 PASS — both sides are within
  budget or better.)
- **Serial dependence assessment**: paired runs collected over time can be
  autocorrelated (thermal/background drift). Randomized interleaving (§3.1)
  cancels slow drift in the paired difference, but residual dependence may
  remain. **Assess** it on the `d_i` sequence: lag-1 autocorrelation and a
  Ljung–Box (or runs) test, reported with the result. The order-statistic CI and
  sign test assume **iid** `d_i`. **If material dependence is present**, either
  **redesign** the measurement (better thermal stabilization, longer inter-pair
  spacing, fuller randomization) and re-pilot, **or** use a **block-based**
  method (e.g. moving-block bootstrap with a predeclared block length chosen
  from the dependence); in that case the exact order-statistic coverage no
  longer applies and the block method's assumptions must be documented. Do not
  silently apply iid methods to dependent data.
- **Outliers (predeclared, preserves `N`)**: the outlier rule is fixed in the
  pre-registered plan **before** confirmatory collection (e.g. exclude a pair
  only when a run exceeds a thermal/background anomaly threshold logged at run
  time). Exclusions are logged with reasons and applied blind to the engine
  label where possible; a collection buffer keeps the **analyzed** set equal to
  the predeclared `N` (§3.2). Do not cherry-pick, and do not let post-hoc
  exclusion choices change the effective sample size or the analysis.
- **Attribution before verdict**: any apparent regression beyond budget is
  source-line-profiled to confirm it lies in FPE value kernels (not unchanged
  code or measurement artifact) before being called a regression.

### 3.5 Allocation audit

- Confirm FPE adds no per-frame heap allocation beyond the existing output-Vec
  pattern; `FpeScratch` reused; no new globals (instrumented allocator /
  `dhat`-style, test-only). Report allocation count/bytes per frame, Q31 vs FPE.

### 3.6 Ties and discrete data (coverage qualification)

Paired percentage differences can **tie** (equal `d_i`), most importantly
`d_i = 0` (FPE and Q31 produce identical corpus byte counts/timings within
measurement resolution). Ties make the data **discrete**, which affects the
coverage claims:

- **Order-statistic CI under ties.** The exact binomial coverage
  `1 − 2·P(Bin(N,0.5) ≤ k−1)` is derived for **iid continuous** data (no ties,
  unique median). With ties the sign-based order-statistic interval is generally
  **conservative** (actual coverage ≥ nominal) rather than exactly 95%, and the
  median may not be unique. **Do not claim exact 95% coverage when ties are
  present**; report the interval as distribution-free and conservative, and
  state the tie count.
- **Sign test and zero differences.** The sign test for "median difference ≠ 0"
  **discards zero differences** and conditions on the `N′ = N − #(d_i = 0)`
  non-zero pairs (binomial reference uses `N′`, not `N`). Under a continuous
  null, zeros have probability 0; with discrete/tied data zeros are expected and
  must be handled this way. Report `N′` and the number of zeros. (The CI remains
  the G6 decision instrument; the sign test is supplementary.)
- **Reporting (mandatory).** Report the number of tied pairs, the number of
  zero differences, the effective sign-test `N′`, and the tie frequency
  (`#ties / N`). State explicitly whether the continuous-data coverage
  assumption is reasonable for the observed tie frequency.
- **When ties are frequent enough to invalidate the continuity assumption**
  (e.g. coarse timing resolution quantizes many differences to the same value,
  or a large mass at `d_i = 0`): the exact-coverage claim is **not** valid.
  Required qualification/alternative:
  1. **Qualify** the interval as conservative/approximate (no exact-95% claim);
     and/or
  2. use a method valid for discrete data — e.g. compute the **actual discrete
     coverage** from the observed tie structure, a **permutation test** respecting
     the discrete distribution, or a bootstrap calibrated for the discrete
     statistic (noting the percentile-bootstrap small-sample caveat, §3.2);
     and/or
  3. **reduce ties at the source** by increasing timing resolution / using the
     longer 40 s workload so differences are less quantized, then re-pilot and
     re-predeclare the analysis (§3.2).
  Whatever the remedy, **do not silently assert exact coverage under an
  assumption the data violate.**

## 4. Budget and gate (G6)

- **Budget (unchanged objective)**: FPE total encoder throughput within
  **±2%** of Q31 on the corpus aggregate. This is the desired performance
  objective; it is **not weakened** by the protocol refinement.
- **Decision rule (rev 4)**: using the median paired percentage difference and
  its **fixed-sample 95% two-sided CI** (§3.2). The regression budget boundary
  is **+2%**:
  - **G6 PASS** iff the CI lies **entirely at or below +2%** (upper bound ≤
    +2%). This covers both "within [−2%, +2%]" (no material regression) and
    "entirely below −2%" (improvement).
  - **Measured improvement**: CI **entirely below −2%** (upper bound < −2%) →
    a measured performance improvement; it **satisfies G6** and is reported as
    supplementary (not a Stage-1 goal; no SIMD claimed).
  - **Regression finding**: CI **entirely above +2%** (lower bound > +2%, FPE
    slower) → attribute (§3.4), report; owner decides accept/mitigate. Not an
    automatic failure of the whole project, but G6 is not passed without owner
    approval.
  - **INCONCLUSIVE**: CI **straddles +2%** (lower bound ≤ +2% < upper bound) →
    **report inconclusive**; G6 not declared passed. No optional stopping: a
    follow-up is a separate predeclared confirmatory experiment (§3.2/§3.4).
- **Psy-stage self time** (§3.3) may rise; it does not by itself fail G6 if
  total throughput is in budget. It is reported for attribution.
- Correctness gates (G0–G5) take precedence; a performance fix must not change
  numerics. **No SIMD in Stage 1** (non-goal); f64 scalar kernels mirror the
  Q31 loop structure for reviewability. Stage 2 may vectorize once correctness
  is locked.

## 5. Memory strategy

- `FpeChannel` ≈ `PsyChannel` footprint (pre-echo `Vec<f64>` 51 vs `Vec<i32>`
  51 → +204 B/ch; ms_ld same 4 KB; TNS same). Negligible.
- `FpeScratch` mirrors `PsyScratch` discipline: encoder-owned, reused,
  reset-to-exact-initial; float value buffers add ~2× the Q31 working set
  (f64 vs i32) — a few KB, allocated once per stream.
- Allocate-selected-only engine state recommended (`06` §3) to avoid carrying
  unused Q31+FPE state simultaneously.
- **No hot-path allocation** in kernels; output Vecs constructed as today.
- No global mutable state (`01` §4, verified zero).

## 6. Determinism and reproducibility

- Fixed operation order, no FMA contraction, no fast-math (`05` §2) ⇒ the
  `*`/`+`/`·2^k` arithmetic is bit-reproducible per arch. `log2` is **not**
  claimed bit-identical across platforms; its accuracy is measured (`05` §2).
  Benchmark comparisons use identical inputs and validated fixture hashes.
- Cross-arch performance reported separately (ARM64 reference; x86-64
  supplementary, A-5); never pooled into one statistic.
- **Baseline capture (M0/M1)**: before any FPE kernel, re-run the production
  Q31 baseline on current HEAD with the established method (fixture hashes
  validated), recording Q31 psy self-time and total throughput on the available
  hardware. This anchors the paired comparison and the default-regression
  guard.

## 7. Out of scope (Stage 2)

- Float-specific SIMD kernels.
- Pure-float headroom decisions (would remove integer repr-lane work).
- Any optimization that changes numerics or the model.
- Multithreading (no demonstrated requirement; non-goal).