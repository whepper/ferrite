# Scalefactor search performance investigation — 2026-10-28

Question: is there a behavior-preserving, measurable optimization in
`scf::estimate_channel_with_inv_ws` / `scf::assimilate_single_scf`? One
candidate was implemented and rejected; no production change is retained.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 (26A434) / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- Release test binary `target/release/deps/ferrite_aac-549f414e79686bc8`,
  SHA-256
  `f26afee82ba4e6907dbd8a120e0fed3c2060d6dd079c387c64b43cfb279ae6e1`
  (post-`bitenc` tree: `quant::calc_sfb_dist`, `tns::scale_up_spectrum`, and
  the 8-byte-window `write_bits` present).
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index;
  `rust/src/aac_enc/scf.rs` clean; `bitcount.rs` clean. Starting state and
  full worktree status: [`starting-state.txt`](starting-state.txt).

Reproduction commands (from `rust/`):

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

# Correctness
cargo test --release --locked --features aac_enc --lib aac_enc::scf
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
| 1 | 425.978 ms | 214.70 | 0.004658 |
| 2 | 428.682 ms | 213.30 | 0.004688 |
| 3 | 432.495 ms | 211.40 | 0.004729 |
| **Median** | **428.682 ms** | **213.30** | **0.004688** |

The production-baseline test passed; all 19 fixture and long-workload hashes
were already pinned by the benchmark.

## Profile

`sample` self time, worker-normalised. Full per-workload summaries:
[`profile-summary.tsv`](profile-summary.tsv).

| Workload | `estimate_channel` self | Share | `assimilate_single` self | Share | `calc_sfb_dist` self | Share |
|---|---:|---:|---:|---:|---:|---:|
| corpus | 673 | 8.26% | 268 | 3.29% | 1,985 | 24.35% |
| onset | 279 | 4.39% | 40 | 0.63% | 343 | 5.39% |
| music | 596 | 9.12% | 277 | 4.24% | 1,954 | 29.91% |
| speech | 585 | 8.98% | 159 | 2.44% | 1,390 | 21.34% |

Call-graph inclusive counts (not additive; children overlap):
`estimate_channel` ~3,843 corpus / 933 onset / 3,637 music / 2,800 speech;
`assimilate_single` ~550 / 119 / 550 / 318; `calc_sfb_dist` ~1,985 / 343 /
1,954 / 1,390. Also notable on corpus: `count_scf_bits_diff` 139 (1.71%),
`calc_spec_pe_diff` 118 (1.45%).

Single-PC attribution inside `estimate_channel` was extremely concentrated:

| Workload | Hot offset | Samples | Share |
|---|---|---:|---:|
| corpus | `+0x19ac` | 666 | 8.17% |
| music | `+0x19ac` | 591 | 9.05% |
| speech | `+0x1764` | 584 | 8.96% |
| onset | `+0x2798` | 224 | 3.52% |

`+0x19ac` is the store of a 60-element stack-array initialization:
`trial[i] = if i < cnt { scf[i] } else { 0 }` inside the coarser-candidate
loop of `assimilate_multiple_scf2` (step 1). `assimilate_single_scf` is
similarly concentrated: corpus `+0x6e8` (266), music `+0x168` (277), speech
`+0x684` (159), onset `+0x19c` (34). No call counters were added; the harness
does not expose call frequency.

## Candidate 1 — hoist the per-candidate trial copy (tested, rejected)

Source: `scf.rs`, `assimilate_multiple_scf2` step 1. The code copied all 60
entries of `scf` into `trial` before every coarser candidate `nw`:

```rust
let mut nw = scf[start];
loop {
    nw += 1;
    if !(nw <= hi) { break; }
    let mut trial = [0i32; 60];
    for i in 0..60 {
        trial[i] = if i < cnt { scf[i] } else { 0 };
    }
    for b in start..stop { if trial[b] != NO_SCF { trial[b] = nw; } }
    ...
}
```

The proposed change moved the array and baseline copy outside the `nw` loop;
each candidate then only rewrites `start..stop`.

**Equivalence argument.** `trial` differs from `scf` only in `start..stop`.
If a candidate is accepted, the commit writes `scf[b] = nw` for exactly the
same region entries that `trial` already holds, so `trial` still matches the
updated `scf`. If rejected, `scf` is unchanged. Entries outside the region
are never written by the candidate loop and are read by
`count_scf_bits_diff` / `calc_spec_pe_diff` only at indices `< cnt`, so the
one-time zero fill beyond `cnt` is never observed. Every trial sees the same
values as the original code; no arithmetic, ordering, comparison, or
tie-breaking changes.

Exact diff: [`source-change-attempted.patch`](source-change-attempted.patch).

### Generated code

`estimate_channel_with_inv_ws` shrank from 0x3748 (14,152 B) to 0x3690
(13,968 B). The 60-store initialization is gone from the per-candidate path;
the candidate loop now begins directly with the region update
([`codegen-hot-block.txt`](codegen-hot-block.txt)). Before/after full
disassembly: [`asm-estimate_channel.txt`](asm-estimate_channel.txt),
[`asm-estimate_channel-after.txt`](asm-estimate_channel-after.txt).

Re-profiling the candidate showed the self time did **not** disappear: corpus
`estimate_channel` moved from 673 samples (8.26%) to 580 (7.83%), and the hot
offset moved from the removed copy (`+0x19ac`, 666) to the adjacent live
min/max scan (`+0x17b8`, 575). The loop’s sampled share was therefore not
fully avoidable; end-to-end confirmed a small effect.

### Paired result — failed gate

Four alternating pairs, 15 reps, same workloads:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 426.014 ms | 428.401 ms | **−0.560%** |
| 2 | new → old | 438.853 ms | 434.782 ms | **+0.928%** |
| 3 | old → new | 441.589 ms | 437.379 ms | **+0.953%** |
| 4 | new → old | 443.153 ms | 441.929 ms | **+0.276%** |
| **Median** | | | | **+0.602%** |

3/4 pairs positive, but **0/4 reached 1%**; the median is below 1%. Raw logs
`testbuild-{before,after}-{1..4}.log`; summary
[`optimization-testbuild-ab.tsv`](optimization-testbuild-ab.tsv).

Per-workload medians were mixed: music +0.804%, long +0.818%, speech-clean
+0.957%, speech-pauses +1.160%, Eróica +0.515%; regressions on short fixtures
(silence −1.775%, onset −1.382%, quiet-sine −1.237%, edge −1.232%) and stress
−0.189%. The music/speech direction agrees with the profile, but the
aggregate effect is below the gate.

## Candidate 2 — branchless/vectorized min/max scan (not implemented)

The min/max scans in `assimilate_multiple_scf2` (steps 2 and 3) are already
branchless `csel` loops. A rewrite that maps `NO_SCF` to `i32::MAX` for the
min and applies `max` unconditionally is exactly equivalent, but an isolated
49-element benchmark measured **22.38 ns (current) vs 22.31 ns (branchless),
1.003×** — no headroom. Source and output:
[`scan-microbench.rs`](scan-microbench.rs),
[`scan-microbench.txt`](scan-microbench.txt). Not implemented.

## Candidate 3 — hoist the per-`act` `scf_tmp` copy in `assimilate_multiple_scf` (not selected)

`assimilate_multiple_scf` has the same 60-entry `scf_tmp` initialization per
`act`. It is a smaller/onset-weighted share than the step-1 copy, and the
step-1 hoist (the larger instance) was already sub-threshold, so this was not
implemented.

## Correctness on the attempted tree

- `aac_enc::scf`: **6 passed** (including `s4_corpus_exact`,
  `s4_unit_probe_exact`, `s4_quantizer_envelope_holds`).
- Production-baseline regression: **1 passed**.
- Hash verification: 60 rows (19 fixtures × production/feed/core + long);
  all fixture hashes unchanged, long-workload hash `889c3be4ccd53dd7`
  unchanged ([`hash-verification-attempted.log`](hash-verification-attempted.log)).
- The attempted tree was **not** full-suite-tested: it was rejected on the
  performance gate before that step. The final reverted tree was full-suite
  tested below.

## Final tree and validation

`rust/src/aac_enc/scf.rs` is back at HEAD (`git checkout --` of the only
experimental edit); the rebuilt test binary SHA is again
`f26afee82ba4e6907dbd8a120e0fed3c2060d6dd079c387c64b43cfb279ae6e1`.

- Final hash verification: 60 rows, all fixture and long hashes unchanged
  ([`hash-verification-final.log`](hash-verification-final.log)).
- Full release suite with reference artifacts: **336 passed, 0 failed,
  5 ignored** ([`full-suite-with-artifacts.log`](full-suite-with-artifacts.log)).
- Full release suite without artifacts: **326 passed, 10 documented
  artifact-dependent failures, 5 ignored**
  ([`full-suite-without-artifacts.log`](full-suite-without-artifacts.log)).
  The 10 are the known `m5a_decode_check`, `phase1_*`, and `phase1i_*`
  tests.

## Decision

**Rejected and reverted. No scalefactor-search production change is
retained.** The only tested candidate was a real but sub-threshold
improvement (+0.602% median, no pair ≥1%), and the next candidate had no
measurable headroom. The three retained optimizations (`quant::calc_sfb_dist`,
`tns::scale_up_spectrum`, `bitenc::write_bits`) are untouched.

## Limitations

- `sample` is function-level only (release strips debuginfo); single-PC
  attribution can overstate avoidable work, as the shifted hotspot showed.
  Inclusive counts from the call graph overlap and are not additive.
- No call counters were added; candidate frequency is inferred, not measured.
- One machine/session; short fixtures (0.17–2.8 ms) are noisy at 15 reps.
- Byte-identical output establishes bitstream identity on the tested corpus,
  not perceptual quality.
- The attempted tree received focused tests, production-baseline, and hash
  verification, but not the full suite; the final tree received the full
  suite.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state, toolchain, host, binary hash. |
| `baseline-{1,2,3}.log`, `baseline-summary.tsv`, `baseline-raw.tsv` | Session baseline. |
| `corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt` | Baseline profiles. |
| `corpus-after.sample.txt`, `corpus-after-bench.log` | Candidate profile (copy hoisted). |
| `profile-summary.tsv` | Self shares and counts. |
| `asm-estimate_channel.txt`, `asm-estimate_channel-after.txt`, `asm-assimilate_single.txt` | Disassembly. |
| `codegen-hot-block.txt` | Before/after per-candidate excerpt. |
| `source-change-attempted.patch` | Exact attempted diff (reverted). |
| `scan-microbench.rs`, `scan-microbench.txt` | Min/max scan diagnostic. |
| `testbuild-{before,after}-{1..4}.log`, `optimization-testbuild-ab.tsv`, `optimization-workloads.tsv` | Four paired runs and summaries. |
| `hash-verification-attempted.log`, `hash-verification-final.log` | Hash checks. |
| `full-suite-with-artifacts.log`, `full-suite-without-artifacts.log` | Final-tree suite runs. |

No C oracle was built, no reference artifact was regenerated, and no
perceptual test was used. **No staging, commit, push, amend, reset, rebase,
cherry-pick, or history rewrite occurred.**
