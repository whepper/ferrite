# `tns::autocorr_quarter_lags` investigation — 2026-10-28

Question: can `tns::autocorr_quarter_lags` be made faster without changing
any TNS autocorrelation value? One exactly-equivalent loop transpose was
tested and rejected; no production change is retained.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 (26A434) / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- Release test binary `target/release/deps/ferrite_aac-549f414e79686bc8`,
  SHA-256
  `61126376e5672d86ca9de8d23cbcfa4f8e8d52d2684f7386612b19ed67c61756`
  (post-§19 tree; all retained optimizations present).
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index; `scf.rs`
  clean; no rejected scalefactor/unroll edits. Full status:
  [`starting-state.txt`](starting-state.txt).

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

# Focused correctness / hash gate
cargo test --release --locked --features aac_enc --lib aac_enc::tns
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
| 1 | 397.899 ms | 229.8 | 0.004351 |
| 2 | 397.914 ms | 229.8 | 0.004351 |
| 3 | 440.146 ms | 207.8 | 0.004813 |
| **Median** | **397.914 ms** | **229.8** | **0.004351** |

Run 3 is a clear thermal/load outlier; later comparisons are paired.

## Profile

`sample` self time:

| Workload | `autocorr_quarter_lags` self | Share | Inclusive |
|---|---:|---:|---:|
| corpus | 340 | 4.15% | 340 |
| onset | 323 | 5.05% | 323 |
| music | 200 | 3.05% | 200 |
| speech | 255 | 3.90% | 255 |

The function is fully inlined into `merged_autocorr_fixed`’s callers; the
`fmult` helper is inlined. Hot-PC attribution is spread across the unrolled
lag cascade: corpus `+0x2ac` (74), `+0x188` (65), `+0x19c` (58), `+0x248`
(53), `+0x178` (47), `+0x224` (40); onset is dominated by `+0x188` (138)
with the loop head at `+0x170`. The function is 0x4bc bytes, 303
instructions, 39 branches, and keeps 13 `i64` lag accumulators in registers.
It is called four times per channel/frame from `rxx1`/`rxx2` quarter
traversals.

## Candidate tested — lag-outer transpose

The original is i-outer:
per `i`, `lim = min(max_lag, stop-i-1)`, then `lag in 1..=lim`; each lag’s
terms are added in increasing `i`. The candidate transposes to lag-outer:
for each `lag`, add terms for `i in [start, stop-lag)` in increasing `i`.
The set of terms and the i-ascending accumulation order are identical, so
every `out[lag]` is bit-identical; `out[0]` remains untouched. The per-lag
inner loop was expressed with two unit-stride slices and one `i64`
accumulator (`source-change-attempted.patch`).

### Generated code

| Metric | Before | After |
|---|---:|---:|
| Size | 0x4bc (1,212 B) | 0x2fc (764 B) |
| Instructions | 303 | 191 |
| Branches | 39 | 13 |
| Panic paths | 16 | 2 |
| `mul` | 12 | 9 |

LLVM vectorized the transposed inner loop with NEON (8-wide loads/shifts),
but the transposition also introduced two spectral loads per term instead of
reusing one `a` per sample, and the vectorized form still reduces through
scalar `i64` multiplies.

### Paired result — rejected

Four alternating pairs, 15 reps, 19 fixtures + 40 s workload:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 407.896 ms | 417.558 ms | **−2.369%** |
| 2 | new → old | 405.934 ms | 413.108 ms | **−1.767%** |
| 3 | old → new | 404.392 ms | 409.884 ms | **−1.358%** |
| 4 | new → old | 406.357 ms | 411.122 ms | **−1.173%** |
| **Median** | | | | **−1.563%** |

0/4 pairs positive. Per-workload medians regressed almost everywhere
(short fixtures −1.3% to −4.8%; music −1.400%, long −1.336%, speech −1.755%/
−1.816%). Full table: [`optimization-workloads.tsv`](optimization-workloads.tsv).

## Follow-up assessment — bounds hoist

The original cascade also carries per-lag spectral bounds checks. A
fast-path slice/assert variant could remove roughly one branch out of a
~7-instruction term, but it would change invalid-length panic timing and was
estimated below 0.6% end-to-end. Not implemented. A partial unroll was
rejected on the §18 evidence that a similar unroll regressed −0.411% and grew
code 127%.

## Correctness on the attempted tree

- `aac_enc::tns`: **5 passed**, including `fixed_merge_matches_rxx_bit_exact`
  and the D-013 activation census.
- Production-baseline regression: **1 passed**.
- 60-row hash verification: all 19 fixture hashes unchanged; long hash
  `889c3be4ccd53dd7` unchanged
  ([`hash-verification-attempted.log`](hash-verification-attempted.log)).
- Not full-suite-tested; it failed the performance gate first.

## Final tree and validation

The experimental edit was reverted; the test binary SHA is restored to
`611263…`.

- Final hash verification: 60 rows, all fixture and long hashes unchanged
  ([`hash-verification-final.log`](hash-verification-final.log)).
- Full release suite with reference artifacts: **336 passed, 0 failed,
  5 ignored** ([`full-suite-with-artifacts.log`](full-suite-with-artifacts.log)).
- Full release suite without artifacts: **326 passed, 10 documented
  artifact-dependent failures, 5 ignored**
  ([`full-suite-without-artifacts.log`](full-suite-without-artifacts.log)).
- Licensing hygiene OK; `git diff --check` clean.

## Decision

**Rejected and reverted. No production change is retained.** The existing
i-outer cascade reuses one spectral sample across all lags and is faster than
the smaller, branch-reduced transpose.

## Limitations

- One machine/session; baseline run 3 was a thermal/load outlier.
- `sample` is function-level only; hot-PC attribution is spread across the
  unrolled cascade, so no single instruction is the whole cost.
- No call counters were added; invocation frequency is inferred from the
  four quarter calls per channel/frame.
- Byte-identical hashes establish bitstream identity on the tested corpus,
  not perceptual quality.
- The attempted tree received focused TNS tests, production-baseline, and
  hash verification, but not the full suite; the final tree received the full
  suite.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state, toolchain, host, binary hash. |
| `baseline-{1,2,3}.log`, `baseline-summary.tsv` | Session baseline. |
| `corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt` | Raw profiles. |
| `profile-summary.tsv` | Self/inclusive shares. |
| `asm-autocorr_quarter_lags.txt`, `asm-autocorr_quarter_lags-after.txt` | Before/after disassembly. |
| `source-change-attempted.patch` | Exact transposed attempt (reverted). |
| `testbuild-{before,after}-{1..4}.log`, `optimization-testbuild-ab.tsv`, `optimization-workloads.tsv` | Four paired runs and summaries. |
| `hash-verification-{attempted,final}.log`, `full-suite-*.log` | Hash and suite verification. |

No C oracle was built, no reference artifact was regenerated, and no
perceptual test was used. **No staging, commit, push, amend, reset, rebase,
cherry-pick, or history rewrite occurred.**
