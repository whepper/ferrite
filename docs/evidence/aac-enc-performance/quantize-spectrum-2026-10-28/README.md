# `quantize_spectrum` / shared quantizer-helper investigation — 2026-10-28

Question: can `quant::quantize_spectrum` be made faster without changing any
quantized value? Yes. The retained change unifies the positive and negative
magnitude paths in `quantize_line_body`, the helper inlined into both
`quantize_spectrum` and `calc_sfb_dist`. The end-to-end effect is larger than
`quantize_spectrum`’s own share because `calc_sfb_dist` uses the same helper.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 (26A434) / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- Release test binary `target/release/deps/ferrite_aac-549f414e79686bc8`,
  SHA-256
  `f26afee82ba4e6907dbd8a120e0fed3c2060d6dd079c387c64b43cfb279ae6e1`
  (post-`bitenc` tree; `quant::calc_sfb_dist`, `tns::scale_up_spectrum`, and
  the 8-byte-window `write_bits` present).
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index; `scf.rs`
  clean. Full starting status: [`starting-state.txt`](starting-state.txt).

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
| 1 | 427.305 ms | 214.0 | 0.004673 |
| 2 | 425.550 ms | 214.9 | 0.004653 |
| 3 | 428.917 ms | 213.2 | 0.004690 |
| **Median** | **427.305 ms** | **214.0** | **0.004673** |

The production-baseline test passed; all fixture and long-workload hashes
were pinned by the benchmark.

## Profile before the change

`sample` self time, worker-normalised:

| Workload | `quantize_spectrum` | Share | `calc_sfb_dist` | Share | `calc_sfb_quant_energy` | Share |
|---|---:|---:|---:|---:|---:|---:|
| corpus | 375 | 4.62% | 1,997 | 24.62% | 275 | 3.39% |
| onset | 97 | 1.53% | 377 | 5.97% | — | — |
| music | 309 | 4.73% | 1,956 | 29.92% | 289 | 4.42% |
| speech | 264 | 4.02% | 1,422 | 21.66% | 262 | 3.99% |

`quantize_spectrum` has no separate callees in the profile: its self time is
the inlined `quantize_line_body`. Hot-PC attribution:

| Workload | Hot offset | Samples | PC meaning |
|---|---|---:|---|
| corpus | `+0x10c` | 375 | zero/positive boundary after the quantizer multiply |
| music | `+0x16c` | 307 | negative-magnitude path (`tbnz` target) |
| speech | `+0x10c` | 264 | zero/positive boundary |
| onset | `+0xf4` | 97 | zero-width/loop-bound check |

The cost was concentrated at the data-dependent sign and zero branches
around the multiply, not at the table lookup itself.

## Candidates

| # | Candidate | Evidence | Decision |
|---|---|---|---|
| 1 | Share one magnitude path for both signs and negate only the result | Hot PCs at the sign branch and duplicated negative body | **Selected** |
| 2 | Remove the zero early-out and compute a masked result | Zero lines are common (silence/high bands); computing the full path for them adds work | Not selected |
| 3 | Replace the table/normalization pipeline with reciprocal multiplication | Would change fixed-point constants/rounding; not behavior-preserving | Rejected on invariants |

## Selected change and equivalence

`quantize_line_body` previously had two nearly identical bodies:

- `accu < 0`: negate, normalize, table-lookup, shift, negate the result;
- `accu > 0`: normalize, table-lookup, shift, return;
- `accu == 0`: return 0.

The change keeps the zero early-out, records `neg = accu < 0`, applies
`wrapping_neg` once to obtain the magnitude, runs one shared magnitude body,
and conditionally negates the final `i16`:

```rust
let accu = fixmuldiv2_ds(spec, quantizer);
if accu == 0 { return 0; }
let neg = accu < 0;
let mut accu = if neg { accu.wrapping_neg() } else { accu };
// ... same clz / table / shift pipeline ...
let q = (k.wrapping_add(accu) >> (32 - 1 - 16)) as i16;
if neg { q.wrapping_neg() } else { q }
```

For `accu < 0` this is exactly the old arithmetic: the old negative body did
`wrapping_neg`, the same magnitude pipeline, and `wrapping_neg` on the result.
For `accu > 0` it is the old positive body. For `accu == 0` it returns 0 at
the same point. No constant, table index, shift, comparison, rounding, or
traversal order changed. The conditional result negation compiles to `cneg`,
not a branch.

Exact diff: [`source-diff.patch`](source-diff.patch).

## Generated code

| Function | Before | After |
|---|---:|---:|
| `quantize_spectrum` | 0x2c0 (704 B), 176 instr, 13 branches | 0x240 (576 B), 144 instr, 11 branches |
| `calc_sfb_dist` | 0x310 (784 B), 196 instr, 5 branches | 0x2c0 (704 B), 176 instr, 4 branches |

`quantize_spectrum` now has `cbz` for the zero case, `cneg` for the sign
magnitude, one shared body, and a final `cneg`:

```
cbz  x28, zero
cmp  x27, #0
cneg w28, w28, mi
...
cneg w27, w15, mi
```

The standalone out-of-line `quantize_line` copy also disappeared from the
binary after the helper shrank. Full before/after disassembly:
[`asm-quantize_spectrum.txt`](asm-quantize_spectrum.txt),
[`asm-quantize_spectrum-after.txt`](asm-quantize_spectrum-after.txt),
[`asm-calc_sfb_dist-before.txt`](asm-calc_sfb_dist-before.txt),
[`asm-calc_sfb_dist-after.txt`](asm-calc_sfb_dist-after.txt); excerpt:
[`codegen-hot-block.txt`](codegen-hot-block.txt).

## End-to-end results

### Test build: eight alternating pairs

15 reps, 19 fixtures + derived 40 s workload:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 427.795 ms | 403.861 ms | **5.595%** |
| 2 | new → old | 441.306 ms | 405.367 ms | **8.144%** |
| 3 | old → new | 442.158 ms | 411.074 ms | **7.030%** |
| 4 | new → old | 449.054 ms | 411.658 ms | **8.328%** |
| 5 | old → new | 442.171 ms | 409.265 ms | **7.442%** |
| 6 | new → old | 444.153 ms | 409.818 ms | **7.731%** |
| 7 | old → new | 446.885 ms | 414.459 ms | **7.256%** |
| 8 | new → old | 445.663 ms | 413.044 ms | **7.319%** |
| **Median** | | | | **7.380%** |

8/8 positive and 8/8 ≥1%; range 5.595–8.328%. Logs:
`testbuild-{before,after}-{1..8}.log`; summary
[`optimization-testbuild-ab.tsv`](optimization-testbuild-ab.tsv).

Per-workload medians improved on all 20 workloads: music +7.918%, speech
+6.637%/+6.623%, Eróica +8.027%, long +7.954%, noise +8.585%, stress +5.336%,
multitone +1.047%, sweep +1.800%, silence +0.172%. Full table:
[`optimization-workloads.tsv`](optimization-workloads.tsv).

### Production example (non-`cfg(test)`)

Four alternating pairs, `AAC_WAV_REPS=15`:

| Pair | Order | Before total | After total | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 412.206 ms | 384.766 ms | **6.657%** |
| 2 | new → old | 415.384 ms | 383.337 ms | **7.715%** |
| 3 | old → new | 416.015 ms | 382.543 ms | **8.046%** |
| 4 | new → old | 414.897 ms | 383.955 ms | **7.458%** |
| **Median** | | | | **7.586%** |

Per fixture: music +7.895%, speech-clean +6.444%, speech-pauses +6.586%,
Eróica +8.134%, long +7.788%. Logs `wav-{before,after}-{1..4}.log`; summary
[`optimization-wav-example-ab.tsv`](optimization-wav-example-ab.tsv).

### Why the effect exceeds `quantize_spectrum`’s share

`quantize_line_body` is also inlined into `calc_sfb_dist`, which is 24.62%
corpus / 29.92% music self time. Re-profiling the candidate shows both paths
got cheaper:

| Corpus symbol | Before | After |
|---|---:|---:|
| `quantize_spectrum` | 375 (4.62%) | 152 (2.06%) |
| `calc_sfb_dist` | 1,997 (24.62%) | 1,637 (22.18%) |

`quantize_spectrum` self time fell ~59%; `calc_sfb_dist` absolute samples fell
~18%. That shared-helper effect accounts for the ~7.4% end-to-end gain;
`quantize_spectrum` alone could not.

## Correctness

The change is retained, so the attempted tree and final tree are the same.

- `aac_enc::quant`: **10 passed** (including `qprobe_transcript_exact`,
  `s3_corpus_exact`, distortion gates and inverse-quantize envelope tests).
- `aac_enc::scf`: **6**; `bitcount`: **11**; `frame`: **9**; `psy`: **14**.
- Production-baseline regression: **1 passed**.
- 60-row hash verification: all 19 fixture hashes unchanged; long-workload
  hash `889c3be4ccd53dd7` unchanged
  ([`hash-verification-attempted.log`](hash-verification-attempted.log),
  [`hash-verification-final.log`](hash-verification-final.log)).
- Full release suite with reference artifacts: **336 passed, 0 failed,
  5 ignored** ([`full-suite-with-artifacts.log`](full-suite-with-artifacts.log)).
- Full release suite without artifacts: **326 passed, 10 documented
  artifact-dependent failures, 5 ignored**
  ([`full-suite-without-artifacts.log`](full-suite-without-artifacts.log)).
- Licensing hygiene OK; `git diff --check` clean.

## Decision

**Retained.** Median test-build improvement **7.380%** (8/8 pairs ≥1%),
production-example median **7.586%** (4/4 ≥1%), all hashes unchanged, all
focused tests and the full suite with artifacts pass, and every workload
improved. No quantization decision, table, constant, rounding rule, or
bitstream value changed.

## Limitations

- One machine/session; the machine drifted upward during the old-binary runs.
  Alternating pairs and within-pair medians are the primary evidence.
- `sample` is function-level only; attribution to `quantize_spectrum` vs its
  inlined helper is inferred from the source/codegen and confirmed by the
  post-change profile drop in both `quantize_spectrum` and `calc_sfb_dist`.
- The large effect is a shared-helper effect; it is not a claim that the
  `quantize_spectrum` entry point alone was 7% of runtime.
- Byte-identical hashes establish bitstream identity on the tested corpus,
  not perceptual quality.
- No C oracle or reference artifacts were changed.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state, toolchain, host, binary hash. |
| `baseline-{1,2,3}.log`, `baseline-summary.tsv` | Session baseline. |
| `corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt` | Baseline profiles. |
| `corpus-after.sample.txt`, `corpus-after-bench.log` | Candidate profile. |
| `profile-summary.tsv`, `profile-corpus-before-after.tsv` | Symbol shares. |
| `asm-quantize_spectrum*.txt`, `asm-calc_sfb_dist-*.txt`, `codegen-hot-block.txt` | Disassembly and hot-block excerpts. |
| `source-diff.patch` | Retained production diff. |
| `testbuild-{before,after}-{1..8}.log`, `optimization-testbuild-ab.tsv`, `optimization-workloads.tsv` | Eight paired test-build runs and summaries. |
| `wav-{before,after}-{1..4}.log`, `optimization-wav-example-ab.tsv` | Production-example arm. |
| `hash-verification-{attempted,final}.log`, `full-suite-*.log` | Hash and suite verification. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**
