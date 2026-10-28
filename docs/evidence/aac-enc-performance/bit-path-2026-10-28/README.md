# Bit-path profiling and `bit_count` experiment — 2026-10-28

Diagnostic pass supporting
[`docs/aac-enc-performance-baseline.md`](../../../aac-enc-performance-baseline.md)
§14. The attempted `bit_count` change was **reverted**; no bit-path
optimization is retained. The two prior optimizations remain in the tree.

## Environment

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- Release test binary at start:
  `target/release/deps/ferrite_aac-549f414e79686bc8`,
  SHA-256 `1016eeabef40d7da86d8a86c6588fac7c4e882096f78500d60549ed64b885b4e`
  (after the retained `calc_sfb_dist` and `scale_up_spectrum` changes).
- Default `aac_enc` feature; release profile; `sample` 1 ms.

## Current baseline

Three independent 15-rep runs over all 19 fixtures + 40 s derived workload:

| Run | Production total | Realtime × | RTF |
|---|---:|---:|---:|
| 1 | 436.628 ms | 209.4 | 0.004775 |
| 2 | 435.320 ms | 210.1 | 0.004760 |
| 3 | 434.154 ms | 210.6 | 0.004747 |
| **Median** | **435.320 ms** | **210.1** | **0.004760** |

The production-baseline test passed and all benchmark runs validated the 19
fixtures and the long workload.

## Current bit-path profile

The music/speech/onset/corpus profiles were collected from the same
post-`scale_up_spectrum` binary; raw outputs are in this directory. Worker
samples and self shares:

| Workload | Bit-path self | Top symbols |
|---|---:|---|
| corpus | 10.37% | `code_values` 3.76%, `dyn_bit_count_reuse` 2.98%, `bit_count` 2.77% |
| onset | 7.98% | `bit_count` 4.12%, `dyn_bit_count_reuse` 2.11%, `code_values` 0.91% |
| music | 11.62% | `code_values` 5.15%, `dyn_bit_count_reuse` 2.87%, `bit_count` 2.37% |
| speech | 9.59% | `code_values` 3.35%, `bit_count` 3.26%, `dyn_bit_count_reuse` 2.19% |

`dyn_bit_count_reuse` and `code_values` are the largest single bit-path
symbols on corpus/music; `bit_count` is largest on onset. All are
integer-table/loop code; there is no single dominant symbol.

## Candidate selected: `bit_count` quad-loop cleanup

`bit_count` had **38 `panic_bounds_check` sites**, 881 instructions, and
2.77% corpus / 4.12% onset self time. Most hot checks are table-range guards,
but the stride-4 loads also carry per-value index checks. The attempted
change replaced each `while i < values.len()` / `values[i]` kernel with
`chunks_exact(4)` (and `chunks_exact(2)` for `count_esc`), preserving every
table lookup, arithmetic operation, and emitted/count value. A release width
assertion kept invalid-width failure fail-closed. The table guards were kept
because removing them would require changing the invalid-input contract.

Exact attempted source description:
[`source-change-attempted.txt`](source-change-attempted.txt). This file was
reverted; it is not present in the current tree.

## Attempted code generation

| Metric | Before | After |
|---|---:|---:|
| `bit_count` instructions | 881 | 700 |
| `panic_bounds_check` | 38 | 19 |
| Branches | 57 | 32 |
| Code bytes | 0xdc4 | 0xaf0 |

The value-index checks were removed, but **19 table-guard panics remain**
because the function contract intentionally panics on out-of-contract values.
The function shrank about 20.5%; this is partial, not complete, bounds-check
removal.

## End-to-end result — failed gate

Four alternating old/new test-build pairs, 15 reps, 19 fixtures + 40 s
derived workload:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 426.267 ms | 437.847 ms | **−2.72%** |
| 2 | new → old | 447.935 ms | 443.307 ms | **+1.03%** |
| 3 | old → new | 449.355 ms | 448.621 ms | **+0.16%** |
| 4 | new → old | 450.803 ms | 451.275 ms | **−0.11%** |
| **Median** | | | | **+0.029%** |

Only one of four pairs reached 1%; the median is far below the gate. The
per-workload medians were also mixed and mostly <1% (click +1.15%, edge
+0.91%, stress +0.72%, music +0.41%, speech-clean −0.61%, long −0.01%).
This is consistent with `bit_count` being only 2.77% of corpus self time and
with the remaining table guards preserving much of the checked code.

## Correctness

- `aac_enc::bitcount`: **11 passed**.
- Production-baseline: **1 passed**.
- All benchmark fixture and long-workload hashes unchanged during the
  attempted change.
- After reverting only `bitcount.rs`: full release suite with reference
  artifacts **336 passed, 0 failed, 5 ignored**; release without artifacts
  **326 passed, 10 documented failures, 5 ignored**.
- The retained `quant::calc_sfb_dist` and `tns::scale_up_spectrum` changes
  were not touched.
- No expected output, reference artifact, or test expectation changed.

## Decision

**Rejected and reverted.** The safe loop cleanup did not clear the 1% median
gate; `bitcount.rs` is back at HEAD. No bit-path optimization is retained.
The current baseline remains the 435.320 ms / 210.1× run above.

## Files

| File | Contents |
|---|---|
| `baseline-*.log`, `baseline-summary.tsv` | Current baseline runs. |
| `corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt` | Current `sample` profiles. |
| `bitself.tsv` | Bit-path self samples/shares. |
| `bit-assembly-stats.tsv`, `asm-bit_count-before.txt`, `asm-bit_count-after.txt` | Before/after codegen. |
| `testbuild-before-*.log`, `testbuild-after-*.log` | Four paired benchmarks. |
| `optimization-testbuild-ab.tsv`, `optimization-workloads.tsv` | Paired and per-workload summaries. |
| `source-change-attempted.txt` | The reverted change, with exact patterns. |
| `hash-verification-final.log` | Current post-revert hash verification. |

No C oracle, reference regeneration, or perceptual test was used. **No
staging, commit, push, amend, reset, rebase, cherry-pick, or history rewrite
occurred.**
