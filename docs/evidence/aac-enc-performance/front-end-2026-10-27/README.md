# Front-end profiling evidence — 2026-10-27

Diagnostic pass supporting
[`docs/aac-enc-performance-baseline.md`](../../../aac-enc-performance-baseline.md) §12.
No production optimization was implemented for this pass.

## Environment

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- Release test binary `target/release/deps/ferrite_aac-549f414e79686bc8`,
  SHA-256 `c58116db0d877e23a42f7a2246202bd572fa735f75ffc912b3e3c4a7f170bfca`
  (includes the retained `calc_sfb_dist` optimization).
- Default `aac_enc` feature; release profile (`opt-level=3`,
  debug-assertions off, LTO off, codegen-units 16, strip=debuginfo).
- Profiler: macOS `sample`, 1 ms interval.

## Baseline (current binary)

Three independent 15-rep runs over all 19 committed fixtures + the derived
40 s music workload:

| Run | Production total | Realtime × | RTF |
|---|---:|---:|---:|
| 1 | 452.411 ms | 202.1 | 0.004947 |
| 2 | 460.548 ms | 198.6 | 0.005036 |
| 3 | 470.381 ms | 194.4 | 0.005144 |
| **Median** | **460.548 ms** | **198.6** | **0.005036** |

A separate direct 19-fixture aggregate (15 reps, no derived workload) was
**220.353 ms / 233.5×** (RTF 0.004283). The production-baseline test passed
and all benchmark runs validated the committed fixture hashes. The prior
baseline/experiment evidence remains in the parent directory; this is the
post-`calc_sfb_dist` baseline.

## Commands

From `rust/`, with `BIN=target/release/deps/ferrite_aac-549f414e79686bc8`:

```sh
# Current baseline (three runs; 19 fixtures + 40 s derived workload)
AAC_PERF_BENCH=1 AAC_PERF_REPS=15 AAC_PERF_LONG=1 AAC_PERF_QUIET=1 \
  "$BIN" --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench

# Current 19-fixture direct aggregate
AAC_PERF_BENCH=1 AAC_PERF_REPS=15 AAC_PERF_QUIET=1 \
  "$BIN" --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench

# Profile loop; run sample against the test PID, e.g. for music
AAC_PERF_BENCH=1 AAC_PERF_FIXTURES=music-stereo AAC_PERF_REPS=300 \
  AAC_PERF_MODES=production AAC_PERF_QUIET=1 \
  "$BIN" --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench &
PID=$!
sleep 1
sample "$PID" 8 1 -file music.sample.txt
wait "$PID"
```

Onset used 6,000 reps; speech used 300; corpus used 80 reps over all 19
fixtures with a 10 s sample window.

## Profiles

| Workload | Fixture | Reps | Sample window | Worker samples | Front-end self |
|---|---|---:|---:|---:|---:|
| `corpus` | all 19 fixtures | 80 | 10 s | 8,231 | 26.70% |
| `onset` | `m2-onset-stereo-48k` | 6,000 | 8 s | 6,310 | 51.25% |
| `music` | `m7-music-stereo-48k` | 300 | 8 s | 6,606 | 21.50% |
| `speech` | `m7-speech-clean-stereo-48k` | 300 | 8 s | 6,561 | 29.57% |

Self-time shares are normalised to worker samples (idle/test-harness frames
and sampling overhead excluded). Cumulative shares come from the `sample`
call graph and are not additive; functions nest.

## Files

| File | Contents |
|---|---|
| `baseline-run-{1,2,3}.log`, `baseline-19.log` | Raw current baseline runs. |
| `baseline-summary.tsv` | Three-run baseline summary. |
| `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt`, `corpus.sample.txt` | Raw `sample` output. |
| `onset-bench.log`, `music-bench.log`, `speech-bench.log`, `corpus-bench.log` | Benchmark loop logs for each profile run. |
| `profile-totals.tsv` | Total/idle/worker samples and front-end self share. |
| `frontend-summary.tsv` | Front-end self share per workload. |
| `frontend-self.tsv` | Per-symbol front-end self samples and worker share. |
| `frontend-cumulative.tsv` | Selected inclusive call-graph counts. |
| `assembly-stats.tsv` | Instruction, panic-bound, branch, call, and FP counts for inspected symbols. |
| `asm-tns-scale-up.txt`, `asm-tns-autocorr-quarter.txt`, `asm-block-switch-process.txt`, `asm-dit-fft.txt`, `asm-pe-form-factor.txt`, `asm-psy-process-pair.txt` | Raw symbol disassembly excerpts. |
| `scale-up-microbench.rs`, `scale-up-microbench.txt` | Isolated 1024-element write-loop benchmark. |
| `scale-up-function-microbench.rs`, `scale-up-function-microbench.txt` | Isolated full `scale_up_spectrum` shape benchmark (peak scan + write, four 256-element quarters). |
| `scale-up-codegen.txt` | Old scalar vs new vectorized loop excerpts from the isolated build. |

## Key measurements

Corpus front-end self symbols (worker share):

| Symbol | Corpus | Onset | Music | Speech |
|---|---:|---:|---:|---:|
| `tns::autocorr_quarter_lags` | 3.39% | 4.06% | 2.59% | 3.90% |
| `pe::form_factor` | 3.18% | 4.06% | 2.48% | 2.91% |
| `BlockSwitch::process` | 2.93% | 8.02% | 2.04% | 3.44% |
| `fft::dit_fft` | 2.49% | 6.66% | 2.13% | 3.26% |
| `tns::scale_up_spectrum` | 2.48% | 5.50% | 1.97% | 2.64% |
| `intensity_stereo` | 2.41% | 2.93% | 2.04% | 2.88% |
| `psy::process_pair_reuse` | 2.38% | 5.28% | 1.92% | 2.88% |

Selected inclusive counts: `StreamEncoder::frontend` 22.21% corpus / 50.36%
onset / 16.95% music / 25.56% speech; `psy::process_pair_reuse` 12.22% /
24.29% / 9.60% / 14.07%; `tns::detect` 6.89% / 11.44% / 5.24% / 7.80%;
`transform::process` 4.30% / 11.17% / 3.41% / 4.95%; `BlockSwitch::process`
2.93% / 8.02% / 2.04% / 3.44%.

## Recommendation

The only candidate with strong source and code-generation support is
**`tns::scale_up_spectrum`**: replace the adapter-chain write loop
(`take(stop).skip(start)`) with direct slice ranges. The peak scan is already
vectorized; the write loop is scalar in the production binary. The isolated
full-shape microbenchmark measured **1697.5 ns → 216.4 ns per 1024-element
frame-equivalent** (about 7.8×), and the write-only loop 1636.8 ns → 67.3 ns.
The production function is 2.48% of corpus self time and 5.50% of onset; a
1% end-to-end gate is plausible but must be measured, not assumed. This
recommendation was executed in
[`../tns-scale-up-spectrum-2026-10-27/`](../tns-scale-up-spectrum-2026-10-27/README.md)
and retained after paired benchmarks.

Rejected hypotheses from the same pass: `autocorr_quarter_lags` is already
unrolled/registerized and its 16 bounds checks are tail checks, not inner-loop
checks; `dit_fft`'s 20 bounds checks are all in the cold tail; block switching
is arithmetic-bound f32 IIR with only one bounds check; `form_factor` is
`sqrt_fixp`-bound with one bounds check. No production change is proposed for
these.

## Limitations

Function-level sampling only (release binary strips debuginfo); no source-line
attribution. One machine/build. The production example is not used for the
front-end profiles; the full 19-fixture and onset profiles are `cfg(test)`
builds, so absolute throughput is conservative by the previously measured
0.3–0.9%. The microbenchmarks isolate loop shapes, not full encoder behavior.
No C oracle, no reference regeneration, and no perceptual claim.
