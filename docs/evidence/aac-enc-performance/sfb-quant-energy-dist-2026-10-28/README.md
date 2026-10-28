# `calc_sfb_quant_energy_and_dist` investigation — 2026-10-28

Question: can `quant::calc_sfb_quant_energy_and_dist` be made faster without
changing any energy, distortion, or quantized value? One candidate was
implemented and rejected; no production change is retained.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 (26A434) / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- Release test binary `target/release/deps/ferrite_aac-549f414e79686bc8`,
  SHA-256
  `61126376e5672d86ca9de8d23cbcfa4f8e8d52d2684f7386612b19ed67c61756`
  (post-`quantize_spectrum` tree; all prior retained optimizations present).
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index; `scf.rs`
  clean. Full status: [`starting-state.txt`](starting-state.txt).

Commands (from `rust/`):

```sh
# Baseline, 19 fixtures + 40 s derived workload
AAC_PERF_BENCH=1 AAC_PERF_REPS=15 AAC_PERF_LONG=1 AAC_PERF_QUIET=1 \
  target/release/deps/ferrite_aac-549f414e79686bc8 \
  --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench

# Corpus profile (80 reps, 10 s sample window)
AAC_PERF_BENCH=1 AAC_PERF_REPS=80 AAC_PERF_MODES=production AAC_PERF_QUIET=1 \
  target/release/deps/ferrite_aac-549f414e79686bc8 \
  --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench &
PID=$!; sleep 1; sample "$PID" 10 1 -file corpus.sample.txt; wait $PID

# Focused correctness
cargo test --release --locked --features aac_enc --lib aac_enc::quant
cargo test --release --locked --features aac_enc --lib \
  aac_enc::production_baseline::production_f32_corpus_baseline_is_unchanged
```

Music/speech used `AAC_PERF_FIXTURES=music-stereo` / `speech-clean-stereo`
with 300 reps and 8 s; onset used `onset-stereo` with 6,000 reps and 8 s.

## Baseline

Three independent 15-rep runs over the 19 fixtures plus the derived 40 s
workload:

| Run | Production total | Realtime × | RTF |
|---|---:|---:|---:|
| 1 | 387.902 ms | 235.8 | 0.004242 |
| 2 | 392.909 ms | 232.8 | 0.004296 |
| 3 | 421.689 ms | 216.9 | 0.004611 |
| **Median** | **392.909 ms** | **232.8** | **0.004296** |

Run 3 is a clear thermal/load outlier (~8% above the first two); all later
comparisons are paired and alternating. The production-baseline test passed.

## Profile before the change

`sample` self time:

| Workload | `calc_sfb_quant_energy_and_dist` self | Share | Inclusive |
|---|---:|---:|---:|
| corpus | 319 | 3.89% | 368 |
| onset | 43 | 0.67% | 54 |
| music | 336 | 5.13% | 368 |
| speech | 246 | 3.75% | 272 |

`inv_quantize_line` and `fpow2` do not appear as separate symbols: both are
inlined. Hot-PC attribution was ~99% at one instruction:

| Workload | Hot offset | Samples | PC |
|---|---|---:|---|
| corpus | `+0xdc` | 316 | `csel` in the distortion shift (`shift_int`) |
| music | `+0xdc` | 332 | same |
| speech | `+0xdc` | 241 | same |
| onset | `+0xdc` | 42 | same |

The loop is scalar, one spectral line per iteration, with a long
inverse-quantize → `fpow2` → `accumulate_dist` dependency chain. The hot PC is
a stall point in that chain, not a table lookup or bounds check.

## Candidates

| # | Candidate | Decision |
|---|---|---|
| 1 | Two-line unroll with independent energy/distortion accumulators | **Selected and tested** |
| 2 | Replace per-line `quant[i]` indexing with a length assert + zip | Not selected; estimated <0.5% and changes invalid-length handling |
| 3 | Restructure the fixed-point inverse-quantize/distortion pipeline | Rejected; no behavior-preserving simplification found |

## Selected change and equivalence

The attempted change processed two lines per iteration with independent
`energy0/energy1` and `distortion0/distortion1` accumulators, then summed
the pairs at the end. Wrapping `i32` addition is associative modulo 2^32, so
the final energy and distortion are identical regardless of grouping; no
multiply, shift, `clz`, `fpow2`, or `accumulate_dist` operation changed. The
`|q| > MAX_QUANT` early-out still returns `(0, 0)` if either line is out of
range; there are no side effects, so checking both before processing is
equivalent to checking in order. Indexed access preserves the existing
panic/shape behavior.

Exact attempted diff:
[`source-change-attempted.patch`](source-change-attempted.patch).

### Generated code

| Metric | Before | After |
|---|---:|---:|
| Function size | 0x200 (512 B) | 0x48c (1,164 B) |
| Instructions | 128 | 291 |
| Branches | 3 | 9 |
| Panic paths | 1 | 3 |
| `clz` | 2 | 6 |
| `smull` | 3 | 9 |

The compiler did interleave the two line pipelines, but it duplicated the
whole body and tail, tripling the cold/bounds paths and growing the
function by 127%.

## Paired result — failed gate

Four alternating pairs, 15 reps, same workloads:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 395.260 ms | 399.315 ms | **−1.026%** |
| 2 | new → old | 404.136 ms | 402.723 ms | **+0.350%** |
| 3 | old → new | 405.044 ms | 406.621 ms | **−0.389%** |
| 4 | new → old | 404.499 ms | 406.245 ms | **−0.432%** |
| **Median** | | | | **−0.411%** |

Only 1/4 pairs was positive, 0/4 reached 1%, and the median is negative.
Per-workload medians were mostly small regressions (short fixtures −0.2% to
−2.0%; music +0.146%, long −0.072%, speech-pauses +0.004%). The extra code
and register pressure outweighed the ILP. Full table:
[`optimization-workloads.tsv`](optimization-workloads.tsv); logs
`testbuild-{before,after}-{1..4}.log`.

## Correctness on the attempted tree

- `aac_enc::quant`: **10 passed** (including `qprobe_transcript_exact`,
  `s3_corpus_exact`, inverse-quantize envelope).
- Production-baseline regression: **1 passed**.
- 60-row hash verification: all 19 fixture hashes unchanged; long-workload
  hash `889c3be4ccd53dd7` unchanged
  ([`hash-verification-attempted.log`](hash-verification-attempted.log)).
- The attempted tree was not full-suite-tested; it was rejected on the
  performance gate first.

## Final tree and validation

The only experimental edit was reverted (`quant.rs` is back to the retained
post-§17 state; test binary SHA restored to `611263…`).

- Final hash verification: 60 rows, all fixture and long hashes unchanged
  ([`hash-verification-final.log`](hash-verification-final.log)).
- Full release suite with reference artifacts: **336 passed, 0 failed,
  5 ignored** ([`full-suite-with-artifacts.log`](full-suite-with-artifacts.log)).
- Full release suite without artifacts: **326 passed, 10 documented
  artifact-dependent failures, 5 ignored**
  ([`full-suite-without-artifacts.log`](full-suite-without-artifacts.log)).
- Licensing hygiene OK; `git diff --check` clean.

## Decision

**Rejected and reverted. No production change is retained.** The target is
already compiled as a tight scalar pipeline; a two-line unroll increases code
size and cold paths enough to lose ~0.4% median. This is a negative result
for unrolling, not evidence that the function cannot be improved.

## Limitations

- One machine/session; baseline run 3 was an ~8% thermal/load outlier.
- `sample` is function-level only; the single hot PC is a stall in a long
  dependency chain, so its share should not be read as removable work.
- No call counters were added; invocation frequency is inferred from callers.
- Byte-identical hashes establish bitstream identity on the tested corpus,
  not perceptual quality.
- The attempted tree received focused tests, production-baseline, and hash
  verification, but not the full suite; the final tree received the full
  suite.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state, toolchain, host, binary hash. |
| `baseline-{1,2,3}.log`, `baseline-summary.tsv` | Session baseline. |
| `corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt` | Baseline profiles. |
| `profile-summary.tsv` | Symbol self/inclusive shares. |
| `asm-eqd.txt`, `asm-eqd-after.txt` | Before/after disassembly. |
| `source-change-attempted.patch` | Exact attempted diff (reverted). |
| `testbuild-{before,after}-{1..4}.log`, `optimization-testbuild-ab.tsv`, `optimization-workloads.tsv` | Four paired runs and summaries. |
| `hash-verification-{attempted,final}.log`, `full-suite-*.log` | Hash and suite verification. |

No C oracle was built, no reference artifact was regenerated, and no
perceptual test was used. **No staging, commit, push, amend, reset, rebase,
cherry-pick, or history rewrite occurred.**
