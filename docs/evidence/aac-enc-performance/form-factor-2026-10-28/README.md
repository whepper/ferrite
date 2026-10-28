# `pe::form_factor` investigation — 2026-10-28

Question: does `pe::form_factor` contain avoidable work that can produce a
repeatable ≥1% end-to-end gain without changing psychoacoustic decisions? The
investigation found no credible candidate. **No production change was made.**

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 (26A434) / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- Release test binary `target/release/deps/ferrite_aac-549f414e79686bc8`,
  SHA-256
  `61126376e5672d86ca9de8d23cbcfa4f8e8d52d2684f7386612b19ed67c61756`
  (post-§18 tree; all retained optimizations present).
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index; `scf.rs`
  clean; no rejected quantization unroll. Full status:
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

# Correctness / hash gate
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
| 1 | 397.007 ms | 230.3 | 0.004341 |
| 2 | 401.120 ms | 228.0 | 0.004386 |
| 3 | 404.528 ms | 226.1 | 0.004423 |
| **Median** | **401.120 ms** | **228.0** | **0.004386** |

The production-baseline test passed; the benchmark validated all fixture and
long-workload hashes.

## Profile

`sample` self time:

| Workload | `form_factor` self | Share | Inclusive | `calc_ld_data` self |
|---|---:|---:|---:|---:|
| corpus | 290 | 3.56% | 292 | 248 |
| onset | 293 | 4.63% | 295 | 86 |
| music | 220 | 3.36% | 220 | 233 |
| speech | 192 | 2.93% | 192 | 208 |

`sqrt_fixp` and `inv_sqrt_norm2` are inlined and do not appear as separate
symbols; `calc_ld_data` is partly inlined and partly a separate symbol.
Hot-PC attribution:

| Workload | Hot offset | Samples | PC |
|---|---|---:|---|
| corpus | `+0x25c` | 290 | final `fmult_div2` of the inlined `sqrt_fixp` |
| music | `+0x25c` | 220 | same |
| speech | `+0x25c` | 192 | same |
| onset | `+0x1e0` | 258 | per-line loop head / first sqrt stage |

`form_factor` is called once per channel per frame from
`stream_driver.rs:1101`; its result feeds `prepare_sfb_pe` and the
psychoacoustic model. The hot loop is the per-line
`sqrt_fixp(q.wrapping_abs()) >> FORM_FAC_SHIFT` sum, followed by one
`calc_ld_data` per band. The sampled cost is a long fixed-point square-root
dependency chain, not a table lookup, bounds check, or allocation.

## Candidates considered

| # | Candidate | Exact operation | Why not selected |
|---|---|---|---|
| 1 | Always execute the second-order interpolation correction in `inv_sqrt_norm2` | Remove the `if fract != 0` branch; when `fract == 0` the correction is provably a no-op (`fmult_div2(0, …) == 0`, `fmult_add_div2(reg1, 0, diff) == reg1`) | For real spectra `fract != 0` is the common case, so the branch is predictable; always executing the correction adds work on zero-fraction lines. Estimated ≪0.1%. |
| 2 | Reuse the normalized `val` inside `inv_sqrt_norm2` to avoid re-normalizing `op` in `sqrt_fixp` | `sqrt_fixp` calls `shift_int(op, tmp_exp - 1)` after `inv_sqrt_norm2` already computed `val = op << (clz-1)`; `val >> ((clz-1)>>1)` is the same value, saving one variable shift | Saves one shift per line out of a ~40-instruction sqrt pipeline; estimated <0.1%. |
| 3 | Two-line unroll of the per-line sqrt sum with independent accumulators | Same latency-hiding idea as the rejected §18 unroll | The analogous `calc_sfb_quant_energy_and_dist` unroll grew code 127% and measured **−0.411%** median; `form_factor` is already 0x640 bytes and this would duplicate the full sqrt/interpolation body. No evidence of removable work. |

No candidate removes a provably redundant block of work or has a credible
path to a 1% end-to-end improvement. The dominant cost is the required
fixed-point `sqrtFixp`/`invSqrtNorm2` evaluation for every spectral line.

## Final validation

No code was changed; the attempted-tree/final-tree distinction is empty.

- Final 60-row hash verification: all 19 fixture hashes unchanged; long
  hash `889c3be4ccd53dd7`
  ([`hash-verification-final.log`](hash-verification-final.log)).
- Full release suite with reference artifacts: **336 passed, 0 failed,
  5 ignored** ([`full-suite-with-artifacts.log`](full-suite-with-artifacts.log)).
- Full release suite without artifacts: **326 passed, 10 documented
  artifact-dependent failures, 5 ignored**
  ([`full-suite-without-artifacts.log`](full-suite-without-artifacts.log)).
- Licensing hygiene OK; `git diff --check` clean.

## Decision

**No production change. No experiment was retained or attempted.** The
function’s sampled cost is necessary fixed-point square-root/interpolation
work; the three plausible micro-transformations are each far below the 1%
gate and would add risk for no measurable gain.

## Limitations

- One machine/session; function-level sampling only. The single hot PC is a
  dependency-chain stall, so its share should not be read as removable work.
- No call counters were added; invocation frequency is inferred from the
  single production call site.
- Byte-identical hashes establish bitstream identity on the tested corpus,
  not perceptual equivalence for arbitrary inputs.
- No microbenchmark was treated as acceptance evidence because no candidate
  reached the implementation stage.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state, toolchain, host, binary hash. |
| `baseline-{1,2,3}.log`, `baseline-summary.tsv` | Session baseline. |
| `corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt` | Raw profiles. |
| `profile-summary.tsv` | Self/inclusive shares. |
| `asm-form_factor.txt`, `codegen-hot-block.txt` | Disassembly and per-line hot block. |
| `hash-verification-final.log`, `full-suite-*.log` | Hash and suite verification. |

No C oracle was built, no reference artifact was regenerated, and no
perceptual test was used. **No staging, commit, push, amend, reset, rebase,
cherry-pick, or history rewrite occurred.**
