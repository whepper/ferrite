# Ferrite SIMD Quantization — Performance Benchmark Design & Audit (Phase 2.3)

**Status: design and audit only. No implementation, no measurements, no acceptance claim.**

Correctness validation for the SIMD quantization kernels is complete (NEON on
ARM64; AVX2 on native x86-64 hardware, run
[43643928322](https://github.com/whepper/ferrite/actions/runs/43643928322);
production AVX2 dispatch confirmed in debug and release; differential tests,
VBR-5/VBR-6 goldens and the full suite passed). Performance acceptance has
**not** been established. This document audits the existing benchmark
infrastructure, identifies what can already be measured, and proposes a concrete
benchmark and acceptance methodology for independent review **before** any
benchmark implementation begins.

**How to read this document.** Each section is labeled:

- **[Fact]** — verified by repository inspection (file:line references). HEAD
  `f063ac6`, worktree clean at audit start.
- **[Recommendation]** — proposed approach, open to review.
- **[Assumption]** — requires validation before it can be relied on.

---

## 1. Executive summary

**[Fact]** The repository already contains three benchmark mechanisms, all
opt-in and all currently unmeasured for acceptance purposes:

1. **Kernel microbenchmarks** (`quant_avx2.rs:1409`, `quant_neon.rs:892`):
   scalar-vs-SIMD timing of the three kernels on synthetic inputs.
2. **In-crate end-to-end harness** (`stream_driver.rs:7613`
   `aac_perf_bench`, `#[ignore]` + `AAC_PERF_BENCH=1`): VBR-5/VBR-6,
   production/feed/core modes, 19 fixtures + derived long workload, hash-gated,
   median + aggregate reporting with machine-readable `AACPERF_*` lines.
3. **Production-build example** (`rust/examples/aac_bench_wav.rs`): VBR-6
   default profile on 4 committed WAVs, hash-gated, median reporting with
   `WAVBENCH_*` lines. Exists to check `cfg(test)` build bias.

**[Fact]** No benchmark runs in CI today (`ci.yml` has no perf job;
`avx2-native-validation.yml` is correctness-only by design). No benchmark
result is recorded as an acceptance baseline. The only quantitative acceptance
sketch in the repo is §32.7 (≥1% median end-to-end VBR-5 improvement, ≥6/8
pairs positive, bit-exact, no unacceptable size/memory change — never executed).

**[Recommendation]** Do not build a new framework. The acceptance campaign
should combine, in order: **(a)** repaired/extended in-crate kernel
microbenchmarks using real distributions with correctness assertions and real
statistics; **(b)** paired worktree A/B end-to-end runs (baseline commit vs
candidate) on the **same** runner, reusing the existing harnesses, with
**formal acceptance resting on production-optimized builds** (the in-crate
`cfg(test)` harness remains the broad diagnostic workhorse but must not be the
sole acceptance basis — see §6.1); **(c)** a separate, opt-in CI performance
workflow that records machine-readable artifacts for trend monitoring, with
formal acceptance reserved for a stable dedicated runner. The paired effect is
defined on the log scale (`d_i < 0` = faster) with a calibrated noise floor
(MDE from an A/A pilot plus stability assessment), a 95% confidence interval
that must wholly clear the MDE for acceptance, and a hierarchy that
distinguishes inner repetitions from the `K` paired analysis units (§7). No production dispatch changes and no
benchmark-only public API are required: kernel comparison can use direct calls
to the already `pub(crate)` scalar/SIMD entry points, and end-to-end comparison
needs no backend override at all (worktree A/B). A test-only backend override
is analyzed (§4.4) and **not recommended** at this stage.

**[Assumption]** The quantization kernels are a large enough share of encode
time for SIMD to move end-to-end throughput. The historical profile (§5 of the
baseline doc) predates the S3/S4 work and attributes cost at stage granularity;
quantization's exact current share is unmeasured. Note: the `core − feed` mode
difference must **not** be read as quantization's share — `core` includes
psychoacoustics, TNS, bit counting and Huffman alongside quantization (see
§6.1). The share must be estimated by sampling profiling (diagnostic only,
§6.1) before acceptance, or acceptance must stand on end-to-end results alone
without an attribution claim.

---

## 2. Existing benchmark infrastructure inventory

### 2.1 Kernel microbenchmarks (scalar vs SIMD, same machine, same inputs)

**[Fact]**

| Item | AVX2 (`rust/src/aac_enc/quant_avx2.rs:1404–1487`) | NEON (`rust/src/aac_enc/quant_neon.rs:887–965`) |
|---|---|---|
| Guard | `#[test]` + `#[ignore]`, skips cleanly without AVX2 | `#[test]` + `#[ignore]`, ARM64-only module |
| Cases | `(n, gain)` = `(14, 0)`, `(14, −32)`, `(64, 0)`, `(64, 32)` | identical |
| Spectrum | synthetic LCG `(i·1103515245 + 12345) >> 4` | identical |
| Quant input (energy path) | `(i·7919) % 8191`, uniform | identical |
| Warm-up | 1000 mixed passes | identical |
| Repetitions | fixed 50,000 iterations, single mean | identical |
| Anti-DCE | `black_box` on outputs (not inputs) | identical |
| Metric | prints `ns/line` + ratio per kernel (dist / energy / quant) | identical |
| Correctness check | **none in the body** (see §3.2) | **none in the body** (see §3.2) |

**[Fact]** Scalar and SIMD arms receive **identical arguments** in both
microbenchmarks, so the *relative* comparison is valid as far as it goes. The
`quantize_lines_avx2` arm calls the real production kernel directly
(`quant_avx2.rs:1475`); the dist/energy arms call the real production dispatch
(`calc_sfb_dist`, `calc_sfb_quant_energy_and_dist`), which on AVX2 hardware
takes the AVX2 branch. The NEON arms are symmetric (`quant_neon.rs:921, 955`).

**[Fact]** Gain→(quantizer, shift) mapping: the AVX2 microbench matches
production (`quant_avx2.rs:1421–1423`: `qs = gain.wrapping_neg()`,
`Q[(qs & 3)]`, `shift = (qs >> 2) + 1`). The NEON microbench uses a
double-negated form (`quant_neon.rs:899–901`:
`Q[(qs.wrapping_neg() & 3)]`, `shift = (qs.wrapping_neg() >> 2) + 1` with
`qs = gain.wrapping_neg()`), i.e. effectively `(gain & 3)` / `(gain >> 2) + 1`.
For the microbench's actual gains (all multiples of 4: `0, ±32`) both forms
coincide, and both arms use the same values, so the comparison stands — but the
formula does **not** match production for general gains and would mislead if the
gain grid is extended without fixing it.

### 2.2 End-to-end harness (`stream_driver.rs:7117–7785`, `perf_tests`)

**[Fact]** Opt-in via `#[ignore]` **and** `AAC_PERF_BENCH=1`
(`stream_driver.rs:7612–7617`). Defaults: 9 reps, 1 warmup, VBR-5, modes
`production,feed,core`. Env controls: `AAC_PERF_VBR` (`5`/`6` only),
`AAC_PERF_MODES`, `AAC_PERF_REPS`, `AAC_PERF_WARMUPS`, `AAC_PERF_FIXTURES`,
`AAC_PERF_LONG=1`, `AAC_PERF_QUIET=1` (`stream_driver.rs:7207–7276,
7346–7374`).

**[Fact]** Coverage: all 19 `M58_FIXTURES` (`stream_driver.rs:6032–6062`) —
mono (`click`, `impulse`, `edge`, `noise`), stereo, 44.1 kHz (`sine-44100`,
`onset-44100`), synthetic signals, stress, plus 3 licensed speech/music WAVs
(~10 s stereo 48 kHz: `m7-music`, `m7-speech-clean`, `m7-speech-pauses`) and the
Eroica excerpt (`m8-music-eroica`, ~10 s), plus an opt-in derived 40 s music-x4
workload. **No multichannel beyond stereo.** Chunking (8,000 sample-frames)
matches `aac-convert`. Timed boundary excludes fixture synthesis, validation,
hashing and I/O; `Encoder::new` reported separately (`stream_driver.rs:7135–7146`).

**[Fact]** Correctness is independently enforced at benchmark time
(`stream_driver.rs:7144–7146, 7554–7594`): ADTS framing + frame law always;
VBR-5 full-corpus byte+hash pins; VBR-6 music pin only, other VBR-6 fixtures
framing + cross-repetition determinism. **No fixture is modified or regenerated
by benchmarking.**

**[Fact]** Statistics: per-fixture median over reps; aggregate = median of
complete-corpus per-rep sums (not sum of medians); RTF / ×realtime / samples/s;
machine-readable `AACPERF_CONFIG` / `AACPERF_ROW` / `AACPERF_MEDIAN` /
`AACPERF_AGG` / `AACPERF_DONE` lines (`stream_driver.rs:7632–7639, 7665–7783`).

### 2.3 Production-build example (`rust/examples/aac_bench_wav.rs`)

**[Fact]** Non-test binary through the public `Encoder` API (VBR-6 default
profile): 4 committed stereo-48 kHz WAVs + derived 40 s music-x4, default 15
reps + 1 warmup, median + RTF, hash-gated, `WAVBENCH_*` lines
(`aac_bench_wav.rs:1–16, 226–354`). Its documented purpose is measuring the
`cfg(test)` build bias for the in-crate harness (observed 0.25–0.86% advantage
for the production build; baseline §9.2).

### 2.4 CI and workflows

**[Fact]** `ci.yml` has **no** performance job (grep for perf/bench/criterion:
empty). `avx2-native-validation.yml` is explicitly correctness-only, runs on
`ubuntu-latest` with pinned Rust 1.99.1, `timeout-minutes: 30`, uploads
correctness logs as artifacts, and contains **no** benchmark step. Neither
workflow records or compares timing data.

### 2.5 What is currently recorded

**[Fact]** No benchmark result is recorded as an acceptance baseline anywhere.
The baseline document's quantitative history (§4, §11–§29) is pre-SIMD ARM64
work on a different machine; §32.8 states "None measured; none claimed" for
AVX2, and §36 records native AVX2 **correctness** only.

---

## 3. Identified gaps and limitations

### 3.1 Kernel microbenchmarks are not acceptance-grade (six defects)

1. **[Fact] No correctness assertion despite the doc comment.** Both
   microbenchmark doc comments state "correctness is asserted, not just
   timed" (`quant_avx2.rs:1405–1406`, `quant_neon.rs:887–888`), but the bodies
   contain **zero** `assert`/`==` comparisons (verified by inspection of
   `quant_avx2.rs:1409–1487` and `quant_neon.rs:892–965`) — they time and print
   only. (Correctness itself is covered by the differential tests, but the
   microbenchmark does not verify what it times.)
2. **[Fact] Synthetic-only inputs.** The LCG spectrum
   `(i·1103515245 + 12345) >> 4` is dominated by large magnitudes and the
   energy-path quant `(i·7919) % 8191` is uniform — neither matches the
   production distribution (mostly small magnitudes, structured gains). A
   speedup on this distribution need not transfer.
3. **[Fact] Only two sizes (14, 64).** No sub-group (1–7), group-boundary
   (8–9), tail-heavy (15–17), or full-band coverage; tail and early-out paths
   are never timed.
4. **[Fact] No statistics.** Single mean over 50,000 iterations; no median, no
   dispersion, no outlier policy, no alternating order between arms (scalar
   always first — systematic thermal/frequency bias).
5. **[Fact] NEON gain-mapping inconsistency** (§2.1): harmless for the current
   multiple-of-4 grid, wrong in general; must be fixed before extending the
   grid.
6. **[Fact] `inv_quantize` has no standalone timing** (it is folded into the
   dist/energy paths). Acceptable — it has no independent production call
   path — but the matrix should state this explicitly rather than leave it
   ambiguous.

### 3.2 Comparison-mechanism gap (the central design question)

**[Fact]** Production dispatch pins the backend per machine (§4.1): on AVX2
hardware the public entry points always take AVX2; on ARM64 they always take
NEON. There is **no** existing mechanism to time scalar through the public API
on SIMD hardware, and none to time SIMD on hardware lacking it.

**[Fact]** The observable entry points needed for direct comparison already
exist with `pub(crate)` visibility and identical signatures on both sides:
scalar `quantize_lines_scalar` (`quant.rs:481`), `calc_sfb_dist_scalar`
(`quant.rs:582`), `calc_sfb_quant_energy_and_dist_scalar` (`quant.rs:665`)
vs unsafe SIMD `quantize_lines_avx2` / `calc_sfb_dist_sum_avx2` /
`calc_sfb_quant_energy_and_dist_sums_avx2` (and the `quantize_lines_neon` /
`*_sums_neon` counterparts). No public API change is needed for an in-crate
harness. (External `examples/` / `benches/` targets cannot see `pub(crate)`
items — kernel benches must live in-crate, i.e. `#[cfg(test)]` as today.)

### 3.3 End-to-end harness gaps for SIMD acceptance

- **[Fact]** The harness cannot compare scalar vs SIMD *implementations* by
  itself — backend is determined by the build machine. The missing piece is
  procedural (paired worktree A/B, §4.2), not code.
- **[Fact]** Default 9 reps × 1 warmup is thinner than the baseline doc's own
  15 reps × 4 independent runs (§3.3); §9.2 documents ~1.5% aggregate
  run-to-run spread and up to 3.6% on short fixtures — comparable to the
  sketched ≥1% acceptance bar (§32.7), i.e. the current defaults cannot resolve
  the proposed threshold without more repetitions/pairs.
- **[Fact]** VBR-6 has no full-corpus golden set (music pin only); benchmarking
  the VBR-6 corpus beyond music relies on framing + determinism, not golden
  pins. Hash-gating is still enforced, just not against committed goldens.
- **[Fact]** `cfg(test)` build bias (0.25–0.86%, §9.2) applies equally to both
  arms of a paired comparison, so it cancels to first order — but it means
  absolute numbers from the harness understate production-build throughput.
- **[Fact]** No multichannel (>2ch) coverage; no input longer than 40 s except
  by repetition; the derived long workload is repeated music, not new content.

### 3.4 CI / reproducibility gaps

**[Fact]** GitHub-hosted runners are shared and variable (frequency scaling,
noisy neighbors, no affinity control); the existing harness pins nothing and
records no CPU/frequency metadata. Nothing consumes the machine-readable
`AACPERF_*`/`WAVBENCH_*` lines today. Artifacts exist only for correctness
logs.

---

## 4. Recommended scalar-versus-SIMD comparison strategy

### 4.1 Backend selection as it stands (verified facts)

- **AArch64:** compile-time `#[cfg(target_arch = "aarch64")]` selects NEON
  unconditionally (`quant.rs:421, 539, 619`) — NEON is part of the ARM64
  baseline, no runtime detection. Scalar is reachable only via direct calls to
  the `*_scalar` functions.
- **x86-64:** runtime `is_x86_feature_detected!("avx2")` at each of the three
  dispatch sites (`quant.rs:441, 561, 643`); `#[target_feature] +
  #[inline(never)]` entry points keep AVX2 out of the baseline path. On AVX2
  hardware production always takes AVX2; scalar only via direct `*_scalar`
  calls (or a non-AVX2 machine).
- **Other targets:** scalar (`quant.rs:464` etc.).

### 4.2 End-to-end: paired worktree A/B (primary recommendation)

**[Recommendation]** Compare two trees on the **same** physical runner, same
toolchain (1.99.1), same flags (default release, no `target-cpu=native`), same
inputs and config: baseline worktree at the pre-SIMD commit vs the SIMD tree,
alternating baseline/candidate pairs (a fixed pre-declared `K = 36` pairs per
the §8.3 power analysis; the older ≥8-pair floor from §32.7 is superseded),
using the existing
`aac_perf_bench` / `aac_bench_wav` harnesses unchanged. This measures exactly
what acceptance cares about (production dispatch, real gate behavior, tails,
early-outs) with **zero** code changes, zero dispatch semantics changes, and
zero risk of measuring a test-only path. This is already the shape sketched in
§32.7; this plan confirms it as the primary mechanism.

**[Assumption]** The SIMD under test is reachable from the end-to-end path on
the runner (true by construction on AVX2/ARM64 hardware; the dispatch gate in
the correctness workflow proves branch entry).

**Architecture-specific baselines [Recommendation]:** the pre-SIMD side of
each worktree A/B must isolate exactly the change being measured:

| Architecture | Baseline commit | Change being measured |
|---|---|---|
| ARM64 / NEON | `1c825c6` (parent of the NEON implementation commit `c530054`) | NEON implementation |
| x86-64 / AVX2 | `7adf935` (pre-AVX2 checkpoint, retains the earlier NEON work) | AVX2 implementation |

The x86-64 baseline must retain the earlier NEON work so the comparison
isolates AVX2 rather than combining unrelated changes. These are the intended
comparison baselines from the existing project history, presented as such and
subject to confirmation when the campaign is prepared — no campaign using them
has been run. The candidate SHA is recorded for each campaign as required by
§7.12.

### 4.3 Kernel-level: direct-call comparison in-crate (primary recommendation)

**[Recommendation]** Extend the existing ignored microbenchmarks in place:
call the `pub(crate)` scalar refs and the `pub(crate) unsafe` SIMD entry points
directly with identical inputs (§2.1 already does this). No dispatch is
involved, so no override is needed; what is timed is exactly what production
calls (same functions, same tail/exceptional handling — the kernels themselves
contain the scalar fallback paths). Keep the harness `#[cfg(test)]` +
`#[ignore]` so normal builds and suites never pay for it.

### 4.4 Test-only backend override: analyzed, NOT recommended now

If a future need arises to time *scalar through the public dispatch path* on
AVX2 hardware (e.g. attributing dispatch overhead itself), the only safe shape
is:

- A `#[cfg(test)]`-only override (e.g. env-var `FERRITE_FORCE_SCALAR`, read
  once per process, consulted in the three x86-64 dispatch sites alongside the
  feature check), compiled out of production builds entirely.
- **Design/isolation:** `cfg(test)` gating (not a runtime flag in production);
  read-once caching to avoid per-band overhead; documented as test-only in the
  dispatch comments; CI must run the override harness separately from
  correctness gates so a stuck override cannot mask a dispatch regression.
- **Risks:** (1) override leaking into production measurement (mitigated by
  `cfg(test)` + absence check in release asm); (2) masking real dispatch bugs
  if the override gate is ever mis-set; (3) measuring a path users never
  execute (scalar-on-AVX2-hardware), which answers an attribution question,
  not an acceptance question; (4) maintenance coupling to all three sites.
- **[Recommendation]** Do **not** implement this. Direct-call comparison (§4.3)
  already isolates kernel arithmetic, and worktree A/B (§4.2) already measures
  the production path end to end. An override adds dispatch-touching risk for
  no acceptance-relevant information: the residual it could measure (one
  cached feature-check per band) is negligible next to band work and is
  identical in both worktree arms anyway.

### 4.5 Constraints honored

- No public API solely for benchmarking (kernel benches stay in-crate;
  end-to-end uses the existing public `Encoder` API).
- No weakening of runtime feature detection; no production dispatch semantics
  change (the plan touches **no** production code).
- Never compare different CPUs or compiler configurations as a "SIMD speedup":
  §4.2 requires same-runner paired runs; cross-machine numbers (e.g. ARM64
  NEON vs x86-64 AVX2) are reported as independent throughputs, never as a
  speedup.
- Cross-compilation and Rosetta are explicitly **not** performance evidence
  (Rosetta exposes no AVX2; emulated timing is meaningless).

---

## 5. Kernel benchmark matrix (proposed)

**[Recommendation]** One matrix, run per kernel and per backend pair
(scalar-vs-AVX2 on x86-64; scalar-vs-NEON on ARM64). All cells assert
bit-identical output (scalar == SIMD) in addition to timing — closing gap 1.

| Kernel (production entry) | Sizes (lines) | Distributions | Gains / dz |
|---|---|---|---|
| `quantize_spectrum` → `quantize_lines_*` | 1, 4, 7, **8**, 9, **14** (frequently observed; distribution to be measured, §5.1), 16, 17, 32, 33, 64 | measured production distribution (§5.1) with frequency weighting; synthetic edge as a separate labeled class: all-zero, `INT_MIN`/`INT_MAX` lanes, loud (`0x70000000`), ±1/±65536 | residue-complete gains `{−72..−69, −32, −8, 0, 32, 64}` × `dz ∈ {false, true}` |
| `calc_sfb_dist` → `*_dist_sum_*` | same sizes | same spectra; early-out cases (`\|q\| > MAX_QUANT` at each group position) timed separately from clean cases | same grid; record early-out vs clean separately |
| `calc_sfb_quant_energy_and_dist` → `*_sums_*` | same sizes | caller quant: measured production lines with frequency weighting + edge magnitudes (`8191/8192/−8192/MIN/0`) as a separate class | gains `{−72..−69, 0..3, 32, 64}` (all four mantissa rows) |

- **Sizes rationale:** sub-group (1–7), exact group (8), boundary ±1 (7/9),
  a frequently observed mid size (14 — hitherto labeled "S4 average" in the
  microbench comments without a sourced measurement; treat as provisional until
  §5.1 measures the real distribution), group+tail (16/17), long band (32/33,
  64). Tails and early-out prefixes must appear as explicit cells, not
  incidentally.
- **Production distributions (§5.1) [Recommendation]:** do not assume the
  current synthetic inputs or any single size is representative. Collect the
  joint distribution of (band line-count, gain and quantizer residue, `dz`,
  vectorized-vs-scalar-fallback execution, early-out frequency and position)
  by **instrumentation or test-only collection at the existing production call
  sites** (`quantize_spectrum`, `calc_sfb_dist`,
  `calc_sfb_quant_energy_and_dist` in `quant.rs`) — e.g. a `#[cfg(test)]`-only
  histogram tap that records shapes without changing behavior, exercised by
  running the committed corpus through the existing Correctness-gated harness.
  Use the collected frequencies to **weight** benchmark cells: the headline
  kernel summary is frequency-weighted over production cells; synthetic
  edge-case cells are reported separately for robustness and must not dominate
  the summary.
- **Frequency-weighted estimator (LOW-2) [Recommendation]:** "frequency-weighted"
  means the following concrete aggregate, computed per kernel from the §5.1
  joint distribution — not an average of per-cell `ns/line`. Let each
  production cell `c` (a distinct combination of line-count, gain/residue,
  `dz`, fallback/early-out behavior) have observed call count `N_c` (number of
  production calls landing in that cell) and measured per-call costs
  `t_c^scalar`, `t_c^SIMD` (median wall time per call from the benchmark
  matrix). The aggregate costs are `T^scalar = Σ_c N_c · t_c^scalar` and
  `T^SIMD = Σ_c N_c · t_c^SIMD`; the headline kernel effect is the aggregate
  log-ratio `ln(T^SIMD / T^scalar)` (same sign convention as §7.3: negative =
  faster), reported alongside the aggregate percentage. Both factors matter: a
  frequent cheap cell and a rare expensive cell contribute by their product,
  and the joint distribution must be retained as collected — do **not** weight
  dimensions independently (e.g. average size-mix × average gain-mix), which
  destroys their correlations (gain and line-count co-vary across bands).
  Synthetic edge-case cells are excluded from `T` and reported separately.
  Kernel estimates remain supporting diagnostics, never the formal acceptance
  metric (§8 criterion 4).
- **Provenance and limits of S1/S3 vectors:** the committed `qc1_*`/`qc3_*`
  vectors are oracle-stage captures for specific frames, gains and dead-zone
  settings — production inputs by construction, but a sparse, non-random
  sample, not a measured production distribution. They are suitable seed
  material and reviewable fixtures, **not** a substitute for the §5.1
  distribution measurement; do not weight cells by their composition.
- **Synthetic inputs retained:** LCG/edge cases remain as a second input
  class, always labeled synthetic, for correctness and robustness coverage.
- **Per-cell procedure:** warm-up ≥1,000 mixed passes; measured reps with
  **alternating** scalar/SIMD order per rep (mitigates linear thermal/frequency
  drift within the cell; same limits as §7 item 8 — not a proof of
  stationarity);
  `black_box` on **inputs and outputs**; ns/line + ratio; correctness assert
  every rep (cheap relative to 50k-iteration timing loops — or assert on a
  shorter verification pass per cell).
- **Overhead isolation:** report per-cell `ns/line`; keep the existing
  fixed-iteration structure but add median + IQR over reps (see §7) instead of
  a single mean.
- **What is deliberately excluded:** standalone `fmult`/`fpow2` timing (no
  independent production call path; covered inside the three kernels).

**[Assumption — narrowed]** Once the §5.1 distribution is measured, weighting
kernel cells by observed production frequencies is valid for the corpus and
configuration measured. The S1/S3 committed vectors alone do **not** establish
representativeness (sparse, non-random sample; see provenance note above).

---

## 6. End-to-end benchmark matrix (proposed)

### 6.0 Formal acceptance requires production-optimized builds (MEDIUM-2)

**[Recommendation]** The in-crate `cfg(test)` harness (`aac_perf_bench`) is
the broad diagnostic workhorse, but it **must not be the sole basis for formal
acceptance**, because absolute numbers built under `cfg(test)` understate
production throughput (observed 0.25–0.86%, §9.2) and the test harness is not
what ships. Formal acceptance requires paired baseline/candidate measurements
using **production-optimized builds** — same compiler version (1.99.1),
optimization settings (default release, no `target-cpu=native`), target
features, runner, input data and VBR/config for both arms — over a
representative production workload matrix including at least the music
workloads. Existing correctness gates (hash/framing, §2.2) remain mandatory in
both arms; any mismatch fails the campaign before timing is read. Diagnostic
(`cfg(test)`) results and formal (production-build) results must be labeled as
such and never mixed into one headline.

**[Fact]** The current production-build example (`aac_bench_wav.rs`) is
VBR-6-only and covers only the 4 stereo-48 kHz WAVs, so as it stands it cannot
carry VBR-5 formal acceptance. The VBR-5 byte+hash pins for exactly those 4
WAVs already exist in the committed baseline (`production_baseline.rs:31–182`:
music-stereo, speech-pauses, speech-clean, music-eroica), so the smallest
sufficient **example-only** extension (no production code, no new fixtures) is:
VBR-mode selection plus VBR-5 pins for the same 4 WAVs, reusing the example's
existing file-reading and hash-gating pattern. Covering the remaining 15
(synthetic, mono, 44.1 kHz) fixtures in a production build would require
replicating the `testdata` PCM generators outside `cfg(test)`
(`testdata` is `pub(crate)`, `aac_enc.rs:160`, so external targets cannot use
it) — i.e. duplicated synthesis with drift risk. **[Recommendation]** Scope
formal production-build acceptance to the WAV-based matrix (music workloads
included); use the `cfg(test)` harness for the full 19-fixture breadth as
diagnostics. If VBR-5-on-WAVs and full-corpus `cfg(test)` results ever
disagree in direction, investigate before accepting.

**Intended measurement separation (this resolves the formal-vs-diagnostic
workload scope; §8 criterion 1 is written against it):**

| Measurement | Workload | Role |
|---|---|---|
| Production-optimized VBR-5 | Four committed WAV fixtures, including music | Formal performance acceptance |
| `cfg(test)` VBR-5 | Full 19-fixture corpus | Diagnostic coverage and regression investigation |
| Production-optimized VBR-6 | Four WAV fixtures, if supported by the example extension | Informative, non-gating |

Formal acceptance is claimed only over workloads the production harness can
execute; the 19-fixture corpus is required diagnostic review, not a substitute
for production-build results. If production (4-WAV) and diagnostic
(19-fixture) results disagree in direction, **investigate before acceptance**
— do not average them together and do not let either one silently override the
other.

**[Recommendation]** Reuse `aac_perf_bench` unchanged (plus `aac_bench_wav`
for the production-build cross-check). Matrix:

| Dimension | Values | Notes |
|---|---|---|
| Profile | VBR-5 (primary, full goldens), VBR-6 (music pin + determinism) | matches existing harness + goldens |
| Content | all 19 M58 fixtures (tonal: sine/multitone/sweep/quiet-sine; transient: onset/click/impulse/stress; noise: noise/tonal-noise; speech ×2; music ×2 incl. Eroica; silence/edge) | diagnostic breadth; formal production acceptance uses the 4-WAV subset per the §6.0 table; multichannel: see §6.2 |
| Channels / rate | mono (4 fixtures), stereo (15), 44.1 kHz (2) | committed fixtures; no new recordings needed |
| Duration | 0.2–10 s fixtures + derived 40 s music-x4 (`AAC_PERF_LONG=1`) | steady-state + startup; longer diverse inputs unavailable — noted risk |
| Modes | `production` (headline), `core` (encoder-core processing estimate), `feed` (overhead floor) | §6.1: `core − feed` is an approximate core-processing estimate, **not** a quantization measurement |
| Build | **formal:** production-optimized builds, both arms (§6.0); **diagnostic:** `cfg(test)` harness + `aac_bench_wav` spot check | controls the 0.25–0.86% test-build bias; never mix labels |

- **Identical inputs/config** for both arms (same PCM, same chunking, same
  VBR/mode flags); fresh encoder per rep (existing behavior).
- **Metrics:** wall time, RTF (encode/audio), ×realtime, samples/s — per
  fixture median + aggregate median-of-per-rep-sums (existing definitions, §3.4
  of the baseline doc). Paired analysis per §7.3 (log-scale pair deltas), not
  ratio-of-medians.
- **Correctness:** existing hash/framing gates stay on; **no fixture
  regeneration or golden change**; any hash mismatch fails the campaign before
  any timing is read.
- **Quantization contribution:** see §6.1 — report `core`/`feed` only as
  context, never as "quantization speedup".

### 6.1 Attribution: what `core − feed` does and does not measure (MEDIUM-3)

**[Fact]** `run_core` executes `StreamEncoder::step_frame` over prebuilt
feeder frames; `run_feed` executes only `PcmFeeder::push_interleaved`/`finish`.
Hence `core − feed` ≈ encoder-core processing **excluding the feeder**, which
includes block switching, psychoacoustics, TNS, quantization/scalefactor
search, bit counting, Huffman coding and frame assembly — it is an
**approximate encoder-core processing estimate, not a measurement of
quantization's individual contribution**, and must never be presented as one.

**[Recommendation]** To estimate quantization's actual share (diagnostic
only, to inform whether end-to-end acceptance is plausible):
run a **sampling-profiler** exercise on Linux x86-64 (`perf stat` /
`perf record` + flame graph on the release binary, same workload matrix
subset) and, for ARM64, the platform equivalent (`perf` on Linux ARM where
available; macOS `sample`/Instruments as already used in baseline §5). Keep
profiling runs **separate** from primary timing runs (profiling perturbs what
it measures); never present profiler sample percentages as exact wall-clock
attribution — report them as approximate hot-spot shares with the profiler,
interval and run conditions attached.

### 6.2 Multichannel support vs fixture coverage (LOW-4)

**[Fact]** The encoder supports channel counts 1–8 (`ChannelMode`:
Mono/Stereo/3/4/5/5.1/7.1/6.1, `channel_map.rs:109–129`;
`EncoderConfig::new_with_mode` rejects only counts with no `ChannelMode`,
`config.rs:498`). The benchmark fixtures cover only mono and stereo — i.e.
multichannel is **supported by the encoder but absent from the benchmark
fixtures**, not unsupported. **[Recommendation]** Do **not** expand
acceptance scope to multichannel now: it would require new multichannel
fixtures **and** new golden pins plus validation of the multichannel path,
which is unjustified for a quantization-SIMD campaign (quantization operates
per band/line regardless of channel layout; channel handling is orthogonal).
Record the gap as a limitation. Expanding later is an implementation
prerequisite (fixtures + pins + path validation), not a plan edit.

---

## 7. Statistical methodology (proposed)

**[Recommendation]**

1. **Warm-up:** ≥1 full untimed pass per fixture/mode (existing default);
   kernels ≥1,000 mixed passes (existing). No change needed.
2. **Repetitions:** end-to-end ≥15 measured reps per fixture/mode (raise the
   harness default from 9 to match the baseline doc's own practice); kernels
   keep high iteration counts but report over ≥15 outer reps with alternating
   arm order.
3. **Paired measurement — definitions.** For pair *i* (same runner, same job,
   back-to-back runs) let `B_i`, `C_i` be the baseline and candidate wall times
   for the same fixture/mode (or the same aggregate). Define the paired effect
   on the log scale (symmetric: a 2× speedup and a 2× slowdown have equal
   magnitude, opposite sign):
   `d_i = ln(C_i / B_i) = ln(C_i) − ln(B_i)`.
   Sign convention: **`d_i < 0` means the candidate is faster** (less time).
   Headline: the **median paired effect** `d_med = median(d_i)`, reported with
   the percentage-throughput conversion `speedup% = (exp(−d_med) − 1) × 100`
   (e.g. `d_med = −0.01005` ↔ ≈ +1.0% throughput). Never report the ratio of
   separately computed medians as the headline — it discards the pairing that
   cancels runner drift.
   **Measurement hierarchy (LOW-1):** three levels, not to be conflated.
   *Inner repetitions* are the repeated timed measurements within one
   build/run and workload configuration (e.g. the harness's ≥15 reps per
   fixture/mode); they are **aggregated before forming any pair** — the value
   entering `d_i` for a fixture (or aggregate) is the pair-level summary
   (median of that pass's inner reps; for the corpus aggregate, the per-pass
   wall-time sum computed exactly like the existing `AACPERF_AGG` definition).
   A *pair* is one baseline measurement set plus one candidate measurement set,
   matched by workload and configuration, run back-to-back with AB/BA ordering
   (item 8); A/A pairs use the identical structure with both arms on the same
   commit. A *campaign* is the `K` paired observations (`d_1 … d_K`) entering
   the paired-effect analysis. The 15 inner repetitions are **not** 15
   independent pairs — treating them as such would be pseudo-replication
   (within-pass reps share a process, caches and thermal state); only the `K`
   back-to-back pair deltas are the analysis units. This hierarchy is
   compatible with the existing harness (reps already exist; pairing is a
   run-ordering and aggregation discipline) and with the proposed
   production-build example (same rep structure).
4. **A/A calibration: pilot vs sufficient.** Calibration proceeds in two
   stages. (a) **A/A pilot calibration:** an initial repeated
   baseline-vs-baseline measurement (same commit both arms) with a provisional
   minimum of `K ≥ 8` pairs, interleaved AB/BA (see item 8), computing the
   paired differences `a_i = ln(B'_i / B_i)`. The `K ≥ 8` floor is a
   **provisional starting point, not proof of adequate calibration** — it buys
   the first noise estimate, nothing more. This floor governs calibration
   observations only; it is **not** the A/B campaign sample size, which is
   fixed at `K = 36` (§8.3). The noise distribution is the
   `a_i` themselves. Summarize noise with the **median absolute deviation**
   `MAD = median(|a_i − median(a)|)` (scaled by 1.4826 for a normal-consistent
   `σ̂` if a σ-like scale is wanted) as the proposed robust scale estimator;
   report the 90th percentile of `|a_i|` (`P90`) as a **descriptive**
   tail check (it does not enter the MDE unless a future review explicitly
   justifies that role). Do not use an undefined "spread": the calibration
   record must name the statistic, `K`, ordering, and the full `a_i` list
   (preserved raw). (b) **Sufficient calibration for formal acceptance:** the
   noise estimate must be demonstrably stable before any MDE is frozen — with
   the explicit limitation that at pilot scale this assessment is coarse, not
   a proof of stability. Splitting eight observations into halves of four
   **cannot establish a stable noise estimate by itself**: half-sample MADs at
   n = 4 are extremely noisy, and agreement between the halves may be luck.
   Treat the split-half comparison strictly as a **practical diagnostic
   heuristic**, not a statistically validated guarantee. Procedure: split the
   collected `a_i` into first/second halves (in collection order) and compare
   the two half-sample `σ̂` estimates against the pre-declared tolerance
   (proposed: within a factor of 1.5×). A pass does **not** certify the noise
   floor — it only fails to contradict it. Define a material shift
   reproducibly: the pooled estimate is deemed materially shifted versus the
   first-half-only estimate iff pooled `σ̂` lies outside
   [`σ̂`_first/1.5, `σ̂`_first×1.5] (equivalently, the implied `MDE_log`
   moves by more than ~50%); otherwise the shift is immaterial. **Near-zero
   MAD:** a zero or near-zero MAD at small `K` must **not** be read as proof
   of negligible runner noise — it most plausibly reflects identical repeated
   timings at the clock resolution or an atypically quiet window. If the
   pooled MAD is ≈ 0 (or the `P90` is likewise ≈ 0), do not freeze a
   near-zero MDE; instead run the one pre-declared escalation (double `K`)
   and re-assess, or classify the runner as unsuitable. (The `max(0.01, …)`
   floor in the MDE already prevents a zero noise estimate from producing a
   zero bar, but the stability assessment itself must then be recorded as
   *inconclusive*, never as *passed*.) If the estimate remains unstable,
   collect the one pre-declared escalation batch or classify the runner as
   unsuitable for formal acceptance — do not collect unboundedly and do not
   re-tune the threshold after seeing A/B results. The final frozen record
   states the total `K`, the per-half estimates, the stability outcome
   (pass/inconclusive/fail, with the numbers), and the frozen `MDE_log`;
   freezing happens **before** the A/B campaign begins. No larger fixed
   minimum `K` is imposed: sufficiency is judged from these observations and
   their stability, not from a sample-size rule.
5. **Minimum detectable effect (MDE).** The acceptance bar must clear the
   calibrated noise. Pre-declare on the log scale:
   `MDE_log = max(0.01, 2 × σ̂)` (≈1% throughput-equivalent floor vs twice the
   MAD-based noise scale) where `σ̂` is from the same runner's **frozen** A/A
   calibration. Compare `|d_med|` against `MDE_log` (equivalently, compare the
   throughput conversion against `exp(MDE_log) − 1`). Two distinct
   uncertainties are involved here and must not be conflated: (a) uncertainty
   in the *calibrated noise scale itself* (finite A/A sample — addressed only
   by the §7.4 calibration procedure, whose escalation has its own
   pre-declared limit and which is frozen before any A/B result is examined);
   (b) uncertainty in the *measured paired effect* (finite A/B sample —
   narrowed by collecting the pre-declared A/B pairs and quantified by the
   CI, item 6). **Collecting additional A/B pairs cannot lower a frozen MDE**:
   the MDE is a function of the frozen A/A calibration, not of the A/B sample
   size — more A/B pairs sharpen `d_med` and narrow its CI but leave
   `MDE_log` unchanged. Additional A/A observations may improve confidence
   in the calibration estimate, but only by following the pre-declared §7.4
   calibration procedure and its single-escalation limit, and only *before*
   any A/B results are examined — never as a reaction to them. If the frozen
   calibrated MDE exceeds the target practical improvement, the runner is
   **unsuitable for detecting that target**: do not imply that collecting
   more A/B pairs solves this. More A/B pairs cannot detect an effect below a
   frozen noise floor; they can only estimate a below-floor effect more
   precisely. Keep the MDE frozen before A/B results are examined, and keep
   the practical effect size (is ≥1% worth retaining?) distinct from the
   uncertainty of the measured effect (is the measured gain distinguishable
   from noise?). Exceeding the MDE is necessary but **not** statistical
   significance — see item 6.
6. **Uncertainty of the headline.** Report a **95% confidence interval for the
   median paired effect** (e.g. the order-statistic interval: for `n` pairs,
   the interval between the `j`-th and `(n−j+1)`-th sorted `d_i` with
   `P(Bin(n, 0.5) ≤ j−1) ≤ 0.025`; no normality assumption), alongside the MAD
   of the `d_i`. Denote the interval `[CI_lower, CI_upper]` on the log scale
   (more negative = faster). The nominal 95% coverage interpretation of this
   ordinary order-statistic interval **assumes the pair-level deltas are
   independent observations drawn from a sufficiently stable, common
   distribution during the campaign** — i.e. no material serial dependence
   and no non-stationarity that would invalidate those assumptions. Fixed
   sample size and a single final analysis (item 9) address optional stopping
   only; they do **not** by themselves establish independence or
   stationarity. If the CI includes zero, the campaign has not
   distinguished the effect from noise regardless of the median's sign; §8
   criterion 2 additionally requires the whole interval to clear the MDE
   (item 5) — excluding zero alone is insufficient for an effect-beyond-MDE
   claim. **Response to dependence or non-stationarity:** if the campaign
   shows substantial temporal patterns, serial dependence, or evidence of
   non-stationarity (e.g. pair deltas drifting or clustering in collection
   order rather than scattering around a stable level), do **not** claim
   nominal 95% coverage from the ordinary order-statistic interval without
   qualification — treat the campaign as **inconclusive for formal
   acceptance**, investigate and correct the measurement procedure, then run
   a new pre-declared fixed-maximum campaign. Do not selectively discard
   inconvenient observations and do not extend the existing campaign after
   inspecting its results. No numerical autocorrelation threshold is imposed
   and no visual check is claimed to prove independence; this is an honest
   decision rule, not a time-series analysis.
7. **On "≥6/8 pairs positive".** A vote-count has low power and an arbitrary
   threshold: under the null with symmetric noise, 6/8 positives occurs with
   probability ≈ 14.5% (two-sided sign-test p ≈ 0.29) — i.e. it is **not**
   evidence of an effect. **[Recommendation]** Reclassify it explicitly as a
   **consistency check** stated without vote-counting: all pair deltas
   negative = consistent improvement direction (negative means faster); any
   positive (slower) pair, or mixed signs generally = investigate, do not
   average away. Never as proof. The statistical claim rests on items 5–6.
8. **Ordering and builds.** Alternate arm order across pairs (AB, BA, AB, …)
   to mitigate systematic run-order and approximately linear drift bias;
   randomize the starting arm per campaign and record it. AB/BA ordering
   mitigates the linear-drift component under balanced alternating order, but
   does not guarantee its cancellation. It is not proof of independence,
   absence of serial correlation, or stationarity (see item 6).
   **Build both binaries before any timed measurement begins** (separate
   build directories per worktree so compilation never overlaps timing).
   Preserve **all raw observations in actual chronological collection order**
   (every rep time, every pair delta, with pair index and arm order) in the
   artifact record — summaries must be recomputable and the time course
   inspectable.
9. **Fixed-maximum A/B sample (no interim looks).** Pre-declare an initial
   `K_init`, a maximum `K_max` (with `K_max ≥ K_init`), and — before any data
   is collected — whether the campaign collects `K_init` or the full `K_max`.
   The preferred choice is the full `K_max` from the outset. For the
   fixture-level family (§8.3) the pre-declared values are **`K_init =
   K_max = 36`** — the power-justified size — so there is no escalation path
   and the same `K` is used for all 23 hypotheses. Once collection
   starts, perform **no interim analysis**: do not compute or inspect a
   confidence interval (or any acceptance statistic) to decide whether to
   collect more pairs. A conventional 95% CI inspected at a data-dependent
   second look does not retain nominal 95% coverage, and pre-declaring that a
   look *may* happen does not restore it. Calculate the final median,
   confidence interval and acceptance result **once**, after the complete
   pre-declared sample is collected. Because the sample size is fixed before
   any data is examined and the interval is computed exactly once, the
   reported 95% interval retains its nominal coverage interpretation,
   subject to the independence-and-stationarity assumptions stated in item 6
   (no normality needed; no stopping-rule distortion — but also no guarantee
   against serial dependence or drift, which item 6 handles separately). If
   the single final analysis is inconclusive under §8 criterion 2, report
   "effect below detection on this runner" (or "unresolved") — do not continue
   collecting observations until a favourable result appears (that is
   p-hacking by repetition). Preserve
   AB/BA alternation and all raw observations. The A/A calibration escalation
   (§7.4) is a **separate** procedure with its own pre-declared limit: it
   refines the noise estimate *before* the MDE is frozen and must never be
   confused with, extended by, or substituted for A/B campaign sampling.
10. **Load/frequency:** no pinning or frequency control is practical on
    GitHub-hosted runners — **do not attempt it**; rely on alternation +
    repetition + medians. Document runner SKU/kernel in every record. For formal
    acceptance, use a stable dedicated runner (§9) where pinning/frequency
    policy can be fixed and stated.
11. **Cross-runner comparison:** forbidden for acceptance. Historical or
    cross-arch numbers are reported as independent absolute throughputs with
    full environment labels, never as speedups.
12. **Reproducibility record (LOW-1).** Every A/B result records: exact
    baseline commit SHA; exact candidate commit SHA; rustc version; target
    triple **and** target features/`RUSTFLAGS`; build profile and relevant
    compiler flags; CPU model and architecture; OS, kernel and runner image;
    benchmark harness version or source revision (commit SHA of the harness
    code); workload and configuration identifiers (fixture set, VBR, modes,
    reps, warmups, chunking, ordering seed). Both binaries are built before
    timing; raw observations are preserved (item 8).

---

## 8. Proposed performance acceptance criteria

**[Recommendation — proposal for review, not a decision.]** All conditions
must hold on the **same dedicated runner**, same toolchain/flags, paired as in
§7. Any failure → report honestly, do not retain (per §32.7's existing
disposition rule):

**Relationship to baseline §32.7:** §32.7 sketches an older proposal (≥1%
median, ≥6/8 pairs positive). That sketch is project history and is **not**
modified here. For the upcoming campaign, this plan's statistical methodology
(§7–§8) **supersedes** §32.7 — in particular the ≥6/8 vote-count is demoted to
a consistency check (§7 item 7) and the bar is the pre-declared MDE plus the
whole-CI rule (criterion 2) — subject to final approval of this plan. If this
plan is approved, §32.7 should be annotated as superseded in a follow-up docs
change (out of scope here).

1. **End-to-end (VBR-5, primary):** median paired effect `d_med` (§7.3)
   satisfies `d_med ≤ −0.01`, i.e. at least +1.0% throughput by the
   `speedup% = (exp(−d_med) − 1) × 100` conversion (−0.01 on the log scale is
   +1.005% throughput; the 0.005 pp rounding difference is negligible next to
   any calibrated MDE and is documented here rather than hidden), measured as
   the paired aggregate over the **production-build four-WAV VBR-5 matrix**
   (formal acceptance row of the §6.0 separation table: per-pair aggregate
   wall-time sum across the four WAVs, then the §7.3 paired analysis). This
   1% floor is a **proposal** carried over from §32.7 for continuity — a
   practical-relevance bar, not a statistically derived one; the statistical
   bar is criterion 2, and both must hold. The full 19-fixture `cfg(test)`
   VBR-5 run is a **required diagnostic review** (same paired analysis, per
   fixture, at the same prescribed `K = 36`): it does not substitute for the
   production-build result, and any
   direction disagreement between the two blocks acceptance pending
   investigation.
2. **Signal-over-noise (derives from A/A calibration, §7.4–§7.5):**
   a conservative two-part rule on the log scale. Let `MDE_log` be the frozen
   minimum detectable effect and `[CI_lower, CI_upper]` the two-sided 95% CI
   for the median (§7.6). Formal acceptance requires **both**
   `d_med ≤ −MDE_log` (the point estimate clears the practical threshold)
   **and** `CI_upper < −MDE_log` (the whole interval clears it — the effect
   is distinguishable from noise *at* the MDE, not merely from zero).
   Excluding zero alone is insufficient: a CI of, e.g., `[−0.008, −0.001]`
   excludes zero yet lies entirely inside a `MDE_log = 0.01` band, i.e. it is
   consistent with a negligible effect. A median above the MDE without a
   supporting CI is likewise not a significance claim.
   This directly addresses the documented ~1.5% scheduler spread vs the 1% bar.
3. **No statistically established workload regression:** sign convention —
   `d_i = ln(C_i/B_i)`: `d_i < 0` = candidate faster, `d_i = 0` = no observed
   difference, `d_i > 0` = candidate slower. Throughput change
   `t% = (exp(−d) − 1) × 100`, so **positive `t` = faster**
   and **negative `t` = slower** (equivalently, `d > 0` = regression). The
   percentage-to-log mapping is exact: `t = −1%` ⟺ `d = −ln(0.99) ≈ +0.01005`.
   Decision procedure (reproducible; no vote-counting). Fixture-level blocking
   decisions use the Holm-adjusted test defined below; nominal per-fixture 95%
   intervals may still be reported descriptively but are **not** sufficient
   evidence for the family-wise regression gate.

   **Fixture-level hypothesis family (frozen before the A/B campaign).** One
   combined family contains every fixture-level regression hypothesis that can
   independently block final acceptance: all formal production-build fixture
   tests **and** all diagnostic test-build fixture tests that can block
   acceptance — one family, so there is no ambiguity about separate
   per-matrix families. Each hypothesis is identified by fixture and
   measurement/build mode: the gated comparison is VBR-5 production-mode wall
   time per fixture. Default membership (campaigns without opt-in extras): the
   4 formal production-build WAV cells plus the 19 diagnostic `cfg(test)`
   cells — the 19-fixture corpus contains the same four source WAVs
   (`music-stereo`, `speech-pauses-stereo`, `speech-clean-stereo`,
   `music-eroica-stereo`), but production-build and test-build measurements
   are distinct measurement paths and therefore **distinct hypotheses**;
   nothing is double-counted. Default family size is **m = 23**. Feed/core
   modes, VBR-6 runs, and the opt-in derived long workload are excluded
   unless explicitly added and frozen (each added gated fixture cell
   increments m). No fixture may be added, dropped, or moved after results
   are observed; the frozen family (membership list + m) is part of the
   campaign record.

   **Test and Holm–Bonferroni rule (FWER α = 0.05).** This is a
   **conservative one-sided binomial sign test**. For each family member, let
   `d_1 … d_K` be that fixture's full predeclared sequence of paired
   log-deltas, with `K` the fixed campaign sample size (§7.9). Define
   `S+ = #{i : d_i > 0}`: strictly positive deltas are slowdowns; zero and
   negative deltas count as non-positive observations and are **neither
   removed nor allowed to reduce `K`**. Paired observations are assumed to be
   independent draws from a stable common distribution (§7.6). Under the null
   hypothesis that the candidate is not slower in median,
   `q = P(d_i > 0) ≤ ½`; consequently `S+ ~ Bin(K, q)` with `q ≤ ½`. The test
   evaluates the p-value at the least-favourable null boundary `q = ½`:
   `p = P(Bin(K, ½) ≥ S+)`. This upper-tail probability is **conservative for
   all `q ≤ ½`**, exact at the boundary `q = ½`, and potentially more
   conservative when `q < ½`, including when ties occur. With no ties (zero
   deltas) it is the ordinary exact one-sided binomial sign test at the
   boundary; when zeros occur, counting them as non-positive keeps `q` at or
   below ½ and makes the test **conservative** — it
   does not make tied observations disappear and it does not reduce `K`. A
   zero is at most "no observed difference"; assigning it to the slowdown
   side would inflate `S+` and overstate evidence of a regression, whereas
   keeping it on the null side can only enlarge the p-value, never shrink it,
   so exact ties cannot manufacture significance. All-zero outcomes give
   `S+ = 0` and `p = 1`, which is **not** evidence of no regression (see the
   all-zero rule below). The test is the inversion of the plan's
   order-statistic CI (§7.6) and inherits its dependence caveat; no normality
   is assumed.
   Sort the family p-values `p_(1) ≤ … ≤ p_(m)`; let `k*` be the smallest
   `k` with `p_(k) > α/(m−k+1)` (no such `k` ⇒ all m rejected); reject
   `H_(1) … H_(k*−1)`. A rejected fixture hypothesis is a statistically
   established regression. Tied p-values are ordered arbitrarily (e.g. by
   fixture name, recorded); the step-down rule controls FWER regardless of
   tie order.

   **All-zero fixture rule (inconclusive and blocking).** If all 36 paired
   deltas for a fixture are exactly zero, that fixture is **uninformative**:
   its sign-test p-value is 1, but this is **not** evidence that the
   candidate has no performance regression, and the fixture **must not be
   recorded as a pass**. It is classified **inconclusive**, and an unresolved
   all-zero fixture **blocks final performance acceptance**. The cause must
   be investigated — chiefly timer resolution and measurement granularity
   (e.g. clock quantization coarser than the effect, or a workload too short
   to resolve differences) — and acceptance requires either a documented
   resolution supported by evidence, or a new campaign designed and declared
   before its measurements are collected. All-zero results count toward the
   Holm family for multiplicity purposes (they are simply never rejected);
   they are not dropped from `m` and not dropped from `K`.

   **Minimum sample size for possible rejection (a bound, not a
   recommendation).** Holm's first threshold is `α/m = 0.05/23 = 0.0021739…`.
   The smallest p-value the sign test can produce with `K` observations is
   `2^-K` (every pair slower). `2^-8 = 0.0039063 > 0.0021739`, so `K = 8` can
   never reject; `2^-9 = 0.0019531 ≤ 0.0021739`, so `K = 9` can — but only if
   all nine pairs are slower. **K_min = 9** is the mathematical minimum for
   the *possibility* of a first-step rejection. It is not a power statement
   and is far too small to be a useful safeguard.

   **Prospective power (design calculation; no measurements).** Model the
   sign test under independent pairs with `q = P(d_i > 0)`, null boundary
   `q = 0.5`, slowdown alternative `q > 0.5`. For fixed `K`, the critical
   value at the first Holm threshold is the smallest `s*(K)` with
   `P(Bin(K, ½) ≥ s*) ≤ α/m`; per-fixture power is `P(Bin(K, q) ≥ s*)`.
   Exact binomial values:

   | K | s*(α/m) | power q=0.6 | q=0.7 | q=0.8 | q=0.9 | s*(α=0.05) | best-case q=0.7 | q=0.8 |
   |---|---|---|---|---|---|---|---|---|
   | 9 | 9 | 0.010 | 0.040 | 0.134 | 0.387 | 8 | 0.196 | 0.436 |
   | 15 | 14 | 0.005 | 0.035 | 0.167 | 0.549 | 12 | 0.297 | 0.648 |
   | 24 | 20 | 0.013 | 0.111 | 0.460 | 0.915 | 17 | 0.565 | 0.911 |
   | 30 | 24 | 0.017 | 0.160 | 0.607 | 0.974 | 20 | 0.730 | 0.974 |
   | 36 | 27 | 0.045 | 0.325 | 0.832 | 0.998 | 24 | 0.737 | 0.982 |

   The first block is conservative per-fixture marginal power (worst case:
   the other 22 hypotheses provide no help). The "best-case" block is the
   opposite extreme — the fixture evaluated at the last step (`α/1 = 0.05`)
   because the other hypotheses were at least as significant; it is
   conditional and illustrative, **not** the marginal power of Holm under
   arbitrary configurations. Under the complete null, Holm controls FWER at
   `α = 0.05` for any `K` — a false-positive guarantee, not a power claim.
   Read the table against these boundaries: even `K = 36` gives ~33%
   first-step power against a 70/30 slowdown (`q = 0.7`, ≈74% best case) and
   ~83% against 80/20 (`q = 0.8`, ≈98% best case); 60/40 drift (`q = 0.6`)
   is effectively undetectable at any practical `K` (~5%).

   **Prescribed fixed sample size: `K = 36` (mandatory)** for all 23
   hypotheses. It is `4 × K_min`, gives high power against strong consistent
   slowdowns (~83% first-step, ~98% best case at `q = 0.8`; ~99.8% at
   `q = 0.9`) and stays well below the cost of `K = 50`, which lifts
   `q = 0.7` first-step power only from ~33% to ~45%. Cost is linear in `K`:
   one pair is one baseline plus one candidate pass over the production
   matrix **and** over the diagnostic corpus, so `K = 36` means 72 passes of
   each (not 23 × 72). There is **no discretionary reduction and no
   alternative `K`**: the formal fixture-level campaign uses exactly 36 paired
   observations per hypothesis, `K_init = K_max = 36` (§7.9). The
   pre-campaign feasibility check exists only to confirm the runner can
   complete the prescribed campaign — if it cannot, that runner is
   **unsuitable for formal acceptance testing**, and the campaign is not run
   there; the sample size is never reduced to accommodate a runner.
   **Honest limitation:** this fixture gate is a high-specificity safeguard
   against consistent, near-systematic regressions; it has **low sensitivity**
   to subtle drift, which the aggregate MDE-based criterion (§8.2) and the 1%
   investigation trigger remain responsible for catching.

   (a) **Statistically established regression — acceptance failure.** If a
   fixture's hypothesis is rejected under the Holm procedure above, the
   fixture is a regression and performance acceptance **fails** — regardless
   of whether the estimated slowdown is below or above 1%. There is no
   practical-significance exemption for a statistically established
   regression.
   (b) **Greater than 1% median slowdown — mandatory investigation trigger.**
   The 1% threshold is retained as an investigation trigger, not as the
   definition of a regression and not as the only condition that can block
   acceptance. Investigation-blocking rule (applies identically to the formal
   production matrix and the diagnostic matrix):

   | Result | Interpretation | Acceptance consequence |
   |---|---|---|
   | Holm-adjusted test establishes a slowdown (rejected at FWER α = 0.05) | Statistically established regression | Blocks acceptance, regardless of magnitude |
   | Median slowdown greater than 1%, but adjusted test does not establish a regression | Investigation trigger, not a statistically established regression | Investigate; if unresolved, blocks acceptance |
   | Median slowdown at or below 1%, and adjusted test does not establish a regression | No established regression; no 1% investigation trigger | No additional fixture-level block, subject to the aggregate and other acceptance rules |
   | All 36 paired deltas exactly zero | Uninformative (p = 1); **not** a pass, **not** evidence of no regression | Inconclusive; unresolved all-zero fixture blocks acceptance |

   For either matrix, an investigation is resolved only when its outcome and
   supporting evidence are documented. An unresolved investigation must not be
   silently waived. In particular: any fixture median worse than `t = −1%`
   (`d_med > +0.01005`) always triggers at least investigation, even when its
   CI straddles zero or is otherwise inconclusive. A median slowdown with a CI
   crossing zero remains **inconclusive, not a statistically established
   regression** — apply the existing investigation and follow-up rules.
   (c) Do **not** treat the A/B pair-delta dispersion as a clean
   measurement-noise estimate for this decision: it mixes any real effect
   with measurement variability. Where fixture-specific noise is needed for
   a formal decision, prefer an independent per-fixture A/A calibration
   (same order-statistic CI machinery on baseline-vs-baseline pairs for that
   fixture); if none is available, keep the item as an **investigation**,
   never as a statistically established noise floor. Investigations are
   resolved by corroboration from the diagnostic corpus, a per-fixture A/A
   run, or a new pre-declared fixed-maximum follow-up campaign (one analysis
   per §7 item 9, on fresh pairs — never by extending the current sample
   after seeing its CI); an unresolved investigation on a formal-matrix
   fixture **blocks acceptance**. The same (a)–(c) procedure applies to the
   diagnostic 19-fixture matrix without exception, with the roles kept
   distinct: the 19-fixture matrix remains diagnostic and does not replace
   the formal production matrix, but the same Holm-adjusted regression rule
   applies — any Holm-established diagnostic regression blocks acceptance
   pending explanation, regardless of its practical magnitude, and an
   unresolved investigation triggered by a greater-than-1% median slowdown
   likewise blocks acceptance (per (b); not only statistically established
   regressions can block it). A small but statistically established
   diagnostic slowdown is not exempt (the formal 4-WAV matrix may have missed
   the pathology, so explanation — not silence — is required). Aggregate
   gains must never hide a consistently regressing fixture.
4. **Kernel support (not acceptance; LOW-2):** kernel matrix cells are split
   into **representative production cells** (frequency-weighted per §5.1) and
   **synthetic edge-case cells** (labeled synthetic). Require: improvement or
   neutrality on representative cells **and investigation of any material
   regression there** (a noisy isolated synthetic-cell result never fails the
   implementation on its own; a material representative-cell regression blocks
   acceptance until explained). Here "material" means practically large enough
   to matter (same role as the 1% investigation trigger in criterion 3, not a
   statistical failure rule): kernel cells sit outside the workload Holm
   family, so per-cell CIs are descriptive only and never constitute
   family-wise acceptance decisions. A CI entirely on the slower side, or
   equivalently a statistically established kernel-cell slowdown, is recorded
   as evidence of a kernel regression (supporting diagnostic only);
   otherwise a large median slowdown is an investigation trigger. Such
   kernel-cell evidence must be recorded and investigated, but kernel-cell
   results alone do not establish end-to-end workload performance and do not
   replace the workload acceptance criteria (criteria 1–3). Kernel numbers are
   supporting evidence only — a kernel win alone never passes acceptance, and
   the end-to-end fixture policy is not extended to kernel cells beyond the
   investigation-and-explanation rule stated here.
5. **Correctness:** bit-exact goldens (VBR-5 full corpus, VBR-6 music pin),
   full self-contained suite green, zero skips on SIMD-capable runners.
6. **Size/memory:** record `__text` delta (current SIMD cost ≈ +1.58% text on
   x86-64, §32.9); no new heap allocation, no threading change; any further
   growth must be justified in the acceptance record. No invented byte budget —
   reviewer judges proportionality to the measured gain.
7. **VBR-6:** reported separately (music pin + determinism); informative,
   non-gating until a full VBR-6 golden set exists.

**Trade-off discussion (as required).** A kernel-level speedup (e.g. 2–4× on
`ns/line`) overstates end-to-end impact because quantization is one stage
among psychoacoustics/TNS/bit-counting/Huffman; the `core`/`feed` decomposition
exists precisely to keep this honest. Against a 1%-scale end-to-end bar stands
real complexity cost: two hand-written SIMD backends (`quant_neon.rs`,
`quant_avx2.rs`, ~1.5 kLoC combined) with `unsafe`, arch-specific arithmetic
audit burden (the Phase 2 interleave defect is the cautionary exhibit), and
permanent dual-path maintenance. The acceptance question is therefore not "is
SIMD faster per line" but "does the measured end-to-end gain, with noise
margin, justify the retained complexity" — which is why criteria 1–4 gate on
end-to-end with kernel data as support, and why criterion 2 ties the bar to
measured noise rather than a fixed number alone.

---

## 9. CI and artifact strategy

**[Recommendation]**

- **Keep correctness and performance separate.** The existing
  `avx2-native-validation` workflow stays a correctness gate (no timing).
  Performance gets its own opt-in workflow (`workflow_dispatch` + label-gated
  on PRs, never blocking merge), so noisy timing can never fail correctness
  and correctness stays hermetic.
- **What the perf workflow runs:** the paired worktree A/B end-to-end
  procedure (§4.2) and, separately, the extended kernel matrix (§5) — both
  existing-harness invocations with env flags, no new test logic in CI beyond
  command lines.
- **Artifacts (machine-readable):** upload the raw `AACPERF_*` / `WAVBENCH_*`
  stdout logs **plus** a small JSON summary per run (runner metadata: CPU
  model, kernel, rustc version, commit SHAs of both arms, per-fixture medians,
  pair deltas, aggregate). The stdout lines are already machine-readable; the
  JSON wrapper is the only new artifact format proposed (to be specified at
  implementation time). Every perf record carries the full §7.12
  reproducibility record; raw observations are preserved.
- **Retention:** 14 days like the correctness logs (existing precedent),
  longer for acceptance records (committed to the baseline doc as tables, as
  §36 does for correctness).
- **GitHub-hosted runners: trend monitoring only.** Shared-runner variability
  (§3.4, §7.6) disqualifies them for acceptance; they are suitable for
  regression *trends* (is today's commit slower than last week's on similar
  SKUs?) with wide tolerance bands, never for the §8 gate.
- **Formal acceptance requires a stable dedicated runner** (self-hosted or
  pinned-SKU cloud instance, fixed frequency policy, documented environment,
  exclusive during runs). The A/A calibration (§7.4) must be published from
  that runner before any acceptance run, and every result carries the full
  reproducibility record (§7.12).

---

## 10. Proposed implementation steps (dependency order)

**[Recommendation]**

1. **Harness repair (no production change):** add correctness asserts to both
   microbenchmarks; fix the NEON gain-mapping formula to the production
   mapping (`quant_neon.rs:899–901`); document that `inv_quantize` is covered
   via dist/energy paths. Prerequisite for trusting any kernel number.
2. **Kernel matrix + statistics (test-only):** real-distribution input
   capture (S1/S3 vectors or a checked-in dump), extended size/gain grid (§5),
   alternating arm order, median + IQR reporting, per-cell asserts.
   Depends on (1).
3. **A/A calibration on the intended runner:** run the §7.3–§7.4 pairing
   procedure baseline-vs-baseline with pre-declared `K`; publish the noise
   distribution (`a_i`), MAD/`P90`, and the instantiated MDE (§7.5).
   Depends on the runner existing (§9); no code.
4. **Production-build acceptance matrix (§6.0):** extend the example for VBR-5
   on the committed WAVs (example-only, pins already committed); define the
   WAV-based formal matrix and the `cfg(test)` diagnostic matrix; verify
   determinism gates. Depends on (3) for interpretation.
5. **Artifact/JSON summary + opt-in perf workflow:** wire steps 2–4 into CI
   as manual/label-gated jobs uploading logs + JSON, including the §7.12
   reproducibility record and the §7.3–§7.6 statistics (pair deltas, MDE, CI).
   Depends on (2), (4).
6. **Dedicated acceptance runner:** provision, document, calibrate (3), and
   run the pre-campaign feasibility check for the prescribed `K = 36`
   campaign (§8.3) once, before collection; freeze `K = 36` in the campaign
   record. A runner that cannot complete the prescribed campaign is
   unsuitable for formal acceptance testing; the sample size is not reduced
   to accommodate it. Depends on infra availability, not on code.
7. **Acceptance campaign:** execute §8 on the dedicated runner at the
   prescribed `K = 36`; evaluate the complete 23-hypothesis Holm family in
   one final analysis after the fixed sample is collected (no interim looks,
   §7.9); record results in the baseline doc (new section, same evidence
   standard as §36); retain or reject per the pre-declared criteria.

---

## 11. Risks and open questions

1. **[Assumption → verify]** Quantization's share of encode time is large
   enough to matter. Mitigation: sampling-profile first (§6.1); if the
   quant-attributable share is small, revisit whether acceptance is
   plausible before investing in (2). Do not use `core − feed` as the share.
2. **[Open]** Exact SFB/band-size distribution in production (needed to weight
   kernel cells). The microbench comments' "S4 average 14" is unsourced —
   treat as provisional; measure the real distribution in step (2) per §5.1.
3. **[Open]** NEON acceptance needs an ARM64 runner (GH ARM runners or Apple
   Silicon self-hosted). x86-64 acceptance does not transfer to ARM64.
4. **[Open]** VBR-6 full-corpus golden set does not exist; VBR-6 acceptance
   stays informative until it does (or scope acceptance to VBR-5 explicitly).
5. **[Risk]** Shared-runner noise may swamp a 1%-scale effect even with
   pairing — this is why §8 criterion 2 ties the bar to calibrated noise (MDE + CI) and
   §9 reserves acceptance for a dedicated runner.
6. **[Risk]** Test-build bias (0.25–0.86%) is of the same order as the
   sketched bar; the paired design cancels it to first order, formal
   acceptance additionally requires production builds (§6.0), and
   `aac_bench_wav` provides the production-build cross-check — keep all three.
7. **[Risk]** Scope creep into `vpgatherdd`/lookup-strategy changes (§32.7
   leaves this open): any such change restarts correctness validation for the
   touched kernels and needs its own A/B; it is out of scope for acceptance of
   the current implementation.
8. **[Assumption → verify]** `ubuntu-latest` migration and runner-image drift
   (noted in §36.5) do not invalidate trend data as long as every record
   carries runner metadata and acceptance never uses shared runners.
9. **[Risk — statistical power]** The fixture-level Holm gate (§8.3) is
   deliberately high-specificity: even at `K = 36` it has only ~33%
   first-step power against a 70/30 slowdown and ~83% against 80/20, and
   essentially none against 60/40 drift. A passing fixture gate therefore
   does **not** prove the absence of subtle per-workload regressions; the
   aggregate MDE/CI criteria and the 1% investigation trigger carry that
   load. Raising `K` far beyond 36 buys little for moderate effects; do not
   present the fixture gate as a power guarantee.
10. **[Risk]** Exact-zero paired deltas are possible at timer resolution.
    The precise rule is §8.3's all-zero rule: an all-zero fixture is
    **uninformative, inconclusive and blocking** (p = 1 is not a pass and not
    evidence of no regression). Partial ties are handled conservatively by
    the sign test and do not reduce `K`. A campaign with frequent zeros
    indicates insufficient measurement resolution; that is grounds for
    declaring the campaign inconclusive and redesigning it — never for
    recording "no effect".

---

## Appendix A. Files inspected (audit trail)

**[Fact]** HEAD `f063ac6` (`docs(aac-enc): record native AVX2 validation
results`); worktree clean at start. Inspected:

- `docs/aac-enc-performance-baseline.md` — §2–§4 (hardware/profile, workloads,
  methodology, baselines), §8–§9 (validation, reproduction, uncertainty incl.
  test-build bias and scheduler spread), §29 (aggregate checkpoint pattern),
  §32.7–§32.9 (SIMD benchmark plan/acceptance sketch/size impact), §34/§36
  (native AVX2 CI correctness workflow + run 43643928322 evidence).
- `rust/src/aac_enc/quant.rs` — three dispatch sites (`:441, :561, :643`),
  scalar refs (`:481, :582, :665`), test-only observe module (no production
  effect).
- `rust/src/aac_enc/quant_avx2.rs:1404–1487` and
  `rust/src/aac_enc/quant_neon.rs:887–965` — kernel microbenchmarks (full
  bodies; assert absence and gain mapping verified).
- `rust/src/aac_enc/stream_driver.rs:7117–7785` (`perf_tests`), `:6032–6062`
  (`M58_FIXTURES`), `rust/src/aac_enc/production_baseline.rs:1–288`
  (VBR-5 19-fixture + VBR-6 music pins), `rust/src/aac_enc/testdata.rs`
  (fixture coverage), `rust/examples/aac_bench_wav.rs` (production-build
  harness).
- `rust/Cargo.toml` (no bench targets, no criterion/divan dependency),
  `.github/workflows/avx2-native-validation.yml` (correctness-only, no
  benchmarks), `.github/workflows/ci.yml` (no perf jobs).
- `rust/src/aac_enc/encoder.rs:283–355` (public `Encoder::new/push/flush`
  used by both end-to-end harnesses).

## Appendix B. What was NOT done (scope compliance)

No Rust source, test, fixture, manifest, lockfile, dependency, or workflow was
modified, added, or deleted. No benchmark was run; no timing presented here is
a measurement (all numbers cited are pre-existing records quoted with sources).
No VBR, psychoacoustic, quantization, dispatch, or API change was made or
proposed as implementation — §10 proposes steps for review, not changes.
No commit or push occurred.

---

*End of Phase 2.3 design & audit report. Awaiting independent review before any
benchmark implementation begins.*
