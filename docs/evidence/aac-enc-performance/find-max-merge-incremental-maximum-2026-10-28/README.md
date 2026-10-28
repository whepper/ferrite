# `bitcount::find_max_merge` compacted-candidate experiment — 2026-10-28

Question: can the repeated section rescan in `gm_stage2` be replaced by an
incremental maximum without changing a single bitstream? **The rewrite is
exactly equivalent and consistently faster, but it misses the 1% gate**
(median **+0.846%**, 4/4 pairs positive), so it was reverted. No production
change is retained.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 (26A434) / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1, host
  `aarch64-apple-darwin`
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index; retained
  `bitenc.rs`/`quant.rs`/`tns.rs` diffs untouched. Full status:
  [`starting-state.txt`](starting-state.txt).
- Production test binary `target/release/deps/ferrite_aac-549f414e79686bc8`,
  SHA-256 `61126376e5672d86ca9de8d23cbcfa4f8e8d52d2684f7386612b19ed67c61756`,
  10,930,480 B.

Commands (from `rust/`):

```sh
# Baseline / paired runs, 19 fixtures + 40 s derived workload
AAC_PERF_BENCH=1 AAC_PERF_REPS=15 AAC_PERF_LONG=1 \
  target/release/deps/ferrite_aac-549f414e79686bc8 \
  --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench

# Correctness
cargo test --release --locked --features aac_enc --lib aac_enc::bitcount
cargo test --release --locked --features aac_enc --lib \
  aac_enc::production_baseline::production_f32_corpus_baseline_is_unchanged
```

## What the algorithm actually costs

A temporary instrumented build (removed; patch in
[`instrumentation-attempted.patch`](instrumentation-attempted.patch))
counted `gm_stage2` work for one second of the stereo music fixture:

```
calls=2244 candidates=53771 merges=37832 scan_steps=725165
per call: 24.0 candidates, 16.9 merges, 323 scan steps
per merge: 19.2 scan steps
```

Raw: [`stage2-topology.txt`](stage2-topology.txt),
[`stage2-topology-raw.log`](stage2-topology-raw.log). Candidate counts are
bimodal: a low mode around 4-9 and a broad mode around 24-35 (mean 24.0).

This rules out the tournament-tree idea: with ~19 scan steps per merge, a
64-leaf tree needs up to three 6-level point updates (≈18 dependent node
visits) per merge, the same order as the scan. The scan was therefore kept
linear but compacted.

## Semantics and correctness argument

Full derivation: [`correctness-argument.md`](correctness-argument.md).

- Candidates are the section starts with a successor, visited in ascending
  order; strict `>` keeps the **first** maximum; a best gain of 0 stops the
  loop.
- A merge changes only three candidates: the merged section, its
  predecessor, and the absorbed successor. Everything else is untouched.
- The candidate list is therefore maintainable as ordered
  `(gain, start)` pairs: two point updates plus one ordered removal per
  merge, with the same scan order and tie rule.
- Boundary cases: empty/single-section groups, all-non-positive gains,
  `p == 0`, and removal of the last section all reduce to the original
  outcomes.

Verification: 20,000 randomized stage-1-shaped states compared against a
verbatim copy of the original `gmStage2`/`findMaxMerge` — final `sections`
and `bit_lookup` were bit-identical in all cases (13,930 with real merges).
Mutation checks caught both a `>=` tie rule (case 59) and a wrong
last-section removal.

## Candidate

[`candidate-attempted.patch`](candidate-attempted.patch) (reverted). Shape:

- `merge_gain: Vec<i32>` becomes `merge_cand: Vec<(i32, u32)>`
  (`(gain, section start)`).
- `gm_stage2` builds the list once with the existing walk, then after each
  merge updates `cand[p-1]`, `cand[p]`, and removes the absorbed successor
  in order (`copy_within`), or truncates with `n = p` when the merged
  section becomes last.
- The scan no longer chases `sections[i].sfb_cnt`; it reads independent
  8-byte pairs.

## Generated code

| Metric | Before | After |
|---|---|---:|
| `dyn_bit_count_reuse` | 5,104 B | 4,492 B |
| `gm_stage2` | inlined (no symbol) | 904 B out-of-line |
| `__text` (test build) | 1,834,748 B | 1,837,692 B |
| Binary | 10,930,480 B | 10,936,768 B |

Before (12 instructions per scan step, loop-carried through
`sections[i].sfb_cnt`):

```
ldrsw x9, [x9, #8]
add   x9, x28, x9
cmp   x9, x23
b.hs  exit
ldr   w10, [x20, x28, lsl #2]
cmp   w10, w8
csel  w8, w10, w8, gt
csel  x25, x28, x25, gt
mov   x28, x9
cmp   x9, x21
b.lo  loop
```

After (7 instructions per candidate, independent sequential loads):

```
ldr   w12, [x11], #8
cmp   w12, w8
csel  w8, w12, w8, gt
csel  x26, x10, x26, gt
add   x10, x10, #1
subs  x9, x9, #8
b.ne  loop
```

Raw: [`asm-before-dyn_bit_count_reuse.txt`](asm-before-dyn_bit_count_reuse.txt),
[`asm-after-dyn_bit_count_reuse.txt`](asm-after-dyn_bit_count_reuse.txt),
[`asm-after-gm_stage2.txt`](asm-after-gm_stage2.txt),
[`codegen-comparison.tsv`](codegen-comparison.tsv). The ordered removal
compiles to an out-of-line `_memmove` call (one per merge).

## Paired result — gate not met

Baseline (old binary): 407.532 / 411.808 / 416.375 ms, median
**411.808 ms**. Four alternating pairs, 15 reps, 19 fixtures + 40 s:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 415.208 ms | 411.639 ms | **+0.860%** |
| 2 | new → old | 417.721 ms | 414.242 ms | **+0.833%** |
| 3 | old → new | 418.192 ms | 416.396 ms | **+0.429%** |
| 4 | new → old | 418.516 ms | 414.245 ms | **+1.021%** |
| **Median** | | | | **+0.846%** |

4/4 positive, 0 negative, but only 1/4 reaches 1% and the median is below
the gate. Per-workload medians: music +0.974%, long (+0.846%), Eroica
+1.418%, stress +0.783%, speech-clean +0.269%, speech-pauses +0.075%;
short fixtures are mixed (−1.053% to +1.065%) and sub-3 ms, i.e. noise
dominated. Full table: [`paired-workloads.tsv`](paired-workloads.tsv);
per-pair totals: [`paired-summary.tsv`](paired-summary.tsv); raw logs
`baseline-{1..3}.log`, `pair-{1..4}-{old,new}.log`.

The effect is consistent (every pair improved), so it is a real gain, not
an outlier artefact — but it is below the pre-registered 1% threshold.

## Correctness

Candidate tree:

- `aac_enc::bitcount` 13 passed (incl. 20,000-case differential test),
  `quant` 10, `scf` 6, `bitenc` 9, `encoder` 13, `common::quantize` 1,
  production-baseline 1 ([`focused-tests-candidate.log`](focused-tests-candidate.log)).
- All 19 fixture hashes and the long-workload hash unchanged
  ([`hash-verification-candidate.log`](hash-verification-candidate.log)).

Final tree (after revert):

- Production binary rebuilt to the original SHA-256 and size.
- Production baseline **1 passed**; 60-row hash check unchanged, long hash
  `889c3be4ccd53dd7`.
- Full suite with artifacts: **336 passed, 0 failed, 5 ignored**
  ([`full-suite-with-artifacts.log`](full-suite-with-artifacts.log)).
- Without artifacts: **326 passed, 10 documented artifact-dependent
  failures, 5 ignored** ([`full-suite-without-artifacts.log`](full-suite-without-artifacts.log)).
- Licensing hygiene OK; `bitcount.rs` returns to HEAD with no diff.

## Decision

**Rejected and reverted.** The candidate is exactly equivalent and removes
the loop-carried section walk, but the measured median (+0.846%) does not
meet the 1% gate. The residual cost is merge bookkeeping — two
`calc_merge_gain` calls and the ordered `memmove` per merge — not the scan,
so further tuning of the scan alone is unlikely to clear 1%.

## Limitations

- One machine/session; the machine warmed through the run (baseline
  407–416 ms), so only paired deltas are meaningful.
- The candidate test binary includes ~200 lines of new test code, which
  shifts code layout by ~6 KB; the measured delta is partly layout noise,
  though its consistency across four pairs argues the direction is real.
- The `_memmove` removal cost is proportional to the tail length and is not
  optimised here.
- Byte-identical hashes establish bitstream identity on this corpus, not
  perceptual equivalence.

## Next evidence-supported target

Stop work on `find_max_merge`. The largest unresolved item in the §21
profile is the `calc_sfb_dist` `quant.rs:0` region at 7.86% of corpus
samples (the per-line `q`/abs/`MAX_QUANT`/inverse-quantize path), which has
no line attribution at all. The next step should be to resolve that region
— e.g. a targeted dSYM/assembly annotation of `calc_sfb_dist`'s hot PCs —
before attempting another optimization. `BlockSwitch::calc_window_energy`'s
IIR remains serial and is not recommended.
