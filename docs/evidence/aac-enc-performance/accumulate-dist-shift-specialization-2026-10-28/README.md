# `accumulate_dist` first-shift specialization — 2026-10-28

Question: does replacing the first `shift_int(diff, scale)` in
`quant::accumulate_dist` with an equivalent unsigned wrapping left shift
remove enough work to clear the 1% end-to-end gate? **No.** The change is
exactly equivalent, but LLVM already emitted that form; the candidate is
instruction-identical to the baseline, measured a −0.279% median paired
change, and was reverted. The investigation also corrected a source-line
attribution error from §21.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 (26A434) / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1, host
  `aarch64-apple-darwin`
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index. All four
  retained optimizations present; rejected experiments absent. Full status:
  [`starting-state.txt`](starting-state.txt).
- Production test binary
  `rust/target/release/deps/ferrite_aac-549f414e79686bc8`, SHA-256
  `61126376e5672d86ca9de8d23cbcfa4f8e8d52d2684f7386612b19ed67c61756`,
  10,930,480 B. `cargo test --release --locked --features aac_enc --lib
  --no-run` reported it fresh.

Commands (from `rust/`):

```sh
# Baseline, 19 fixtures + 40 s derived workload
AAC_PERF_BENCH=1 AAC_PERF_REPS=15 AAC_PERF_LONG=1 AAC_PERF_QUIET=1 \
  target/release/deps/ferrite_aac-549f414e79686bc8 \
  --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench

# Focused correctness
cargo test --release --locked --features aac_enc --lib aac_enc::quant
cargo test --release --locked --features aac_enc --lib aac_enc::production_baseline

# Full suite with reference artifacts
PHASE1_ORACLE_DIR=$W/oracle PHASE1_C_DIR=$W/c-adts PHASE1_OUT=$W/phase1-report \
  AAC_DEC_BIN=$T/v3-full-rvehNNkF/build/ref-build/aac-dec \
  M55_C_DIR=$T/opencode/m55/aac \
  cargo test --release --locked --features aac_enc --lib
```

## Baseline

Three independent 15-rep runs over the 19 fixtures plus the derived 40 s
workload:

| Run | Production total | Realtime × | RTF |
|---|---:|---:|---:|
| 1 | 393.544 ms | 232.4 | 0.004303 |
| 2 | 396.690 ms | 230.5 | 0.004338 |
| 3 | 408.351 ms | 223.9 | 0.004465 |
| **Median** | **396.690 ms** | **230.5** | **0.004338** |

All runs revalidated the 19 fixture hashes and the long-workload hash.

## Equivalence proof (verified before editing)

Full derivation: [`equivalence-proof.md`](equivalence-proof.md).

- `count_leading_bits(v)` returns `0..=31` for every `i32` input, so
  `scale >= 0` at the first call; the `else` arm of `shift_int` is dead
  there.
- The taken arm is exactly
  `(v as u32).wrapping_shl(s as u32) as i32`; with `0 <= s <= 31` the cast
  and implicit mask (`rhs & 31`) are identities, so the 32-bit pattern is
  preserved for zero, positive, negative (including `i32::MIN`), and
  boundary values.
- The second `shift_int(diff, -scale2)` can receive either sign and was not
  touched.

## Attempted change (reverted)

```diff
--- a/rust/src/aac_enc/quant.rs
+++ b/rust/src/aac_enc/quant.rs
@@ -432,7 +432,9 @@
 fn accumulate_dist(acc: i32, inv: i32, spec: i32) -> i32 {
     let mut diff = (inv.wrapping_abs().wrapping_sub((spec >> 1).wrapping_abs())).wrapping_abs();
     let scale = count_leading_bits(diff);
-    diff = shift_int(diff, scale);
+    // `scale` is in 0..=31, so this is exactly `shift_int(diff, scale)`
+    // with the right-shift arm dead; keep the generic second shift below.
+    diff = (diff as u32).wrapping_shl(scale as u32) as i32;
     diff = fpow2(diff);
     let scale2 = (2 * (scale - 1)).min(32 - 1);
     diff = shift_int(diff, -scale2);
```

Patch file: [`source-change-attempted.patch`](source-change-attempted.patch).
No other source, test, fixture, or artifact was modified.

## Generated code

`accumulate_dist` is fully inlined into `calc_sfb_dist`. The baseline
already contained the specialized form:

```
clz   w22, w21            ; count_leading_bits(diff)
sub   w22, w22, #0x1
cmp   w20, w19            ; diff == 0 (and INT_MIN) guard
ccmp  w21, #0x0, #0x8, ne
csel  w19, wzr, w22, mi   ; scale = 0 for diff <= 0
lsl   w20, w21, w19       ; first shift: already the specialized form
```

The `csel` belongs to `count_leading_bits`, not to `shift_int`. After the
candidate build, the normalized instruction stream of `calc_sfb_dist` was
identical (0 differences over 176 instructions); the only binary-level
changes were link-time addresses and 20 test/auxiliary/drop-glue symbol
sizes.

| Metric | Before | After |
|---|---|---:|
| `calc_sfb_dist` | 704 B, 176 insns | 704 B, 176 insns |
| Normalized diff | — | 0 |
| `__text` | 0x1bfefc | 0x1bfefc |
| Binary | 10,930,480 B | 10,930,480 B |
| Text symbols | 2,868 | 2,868 |

Raw: [`asm-before-calc_sfb_dist.txt`](asm-before-calc_sfb_dist.txt),
[`asm-after-calc_sfb_dist.txt`](asm-after-calc_sfb_dist.txt),
[`asm-normalized-comparison.txt`](asm-normalized-comparison.txt),
[`codegen-comparison.tsv`](codegen-comparison.tsv).

## Corrected attribution from §21

The §21 profile attributed 4.80% of corpus samples to `q31.rs:219` via
`shift_int <- accumulate_dist (quant.rs:438)`. `quant.rs:438` is the
**second** shift, not the first. In both builds the sampled PCs
(`calc_sfb_dist+0x108` = `asr`, `+0x10c` = `cmp w19, #0x2`) are the
branch-free select between the left and right shift for
`shift_int(diff, -scale2)`. That direction genuinely depends on `scale`:

```
lsl   w21, w20, w21       ; fpow2(diff) << -scale2
asr   w20, w20, w22       ; fpow2(diff) >> scale2
cmp   w19, #0x2
csel  w19, w21, w20, lo   ; pick by scale
```

So the 4.80% line is necessary work on the second shift's critical path,
not a removable dead branch. The first shift had already been optimized by
LLVM; the source rewrite changed nothing.

## Paired result — gate not met

Four alternating pairs, 15 reps, 19 fixtures + 40 s workload, same binaries
copied aside (`bin-before`, `bin-after`):

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 400.041 ms | 408.757 ms | **−2.179%** |
| 2 | new → old | 413.744 ms | 410.739 ms | **+0.726%** |
| 3 | old → new | 410.269 ms | 413.362 ms | **−0.754%** |
| 4 | new → old | 412.488 ms | 411.682 ms | **+0.195%** |
| **Median** | | | | **−0.279%** |

2/4 pairs positive, 0/4 at or above 1%, median below the 1% gate. With
identical code this is layout and thermal noise; the gate is not met.

Per-workload medians are flat: long workload +0.138%, music +0.262%,
speech-clean +0.279%, speech-pauses +0.407%, Eroica +0.629%; the largest
short-fixture moves are `m2-click` −1.328% and `m2-edge` −1.033%.
Full table: [`paired-workloads.tsv`](paired-workloads.tsv); per-pair
summary: [`paired-summary.tsv`](paired-summary.tsv); raw logs
`pair-{1..4}-{old,new}.log`.

## Correctness

Candidate tree (before revert):

- `aac_enc::quant` 10 passed, `aac_enc::scf` 6, `aac_enc::bitcount` 11,
  `aac_enc::bitenc` 9, `common::quantize` 1, `aac_enc::production_baseline`
  1 — all passed ([`focused-tests-candidate.log`](focused-tests-candidate.log)).
- All eight paired benchmark runs asserted every fixture hash and passed.

Final tree (after revert):

- Production binary rebuilt to the original SHA-256
  `61126376e5672d86ca9de8d23cbcfa4f8e8d52d2684f7386612b19ed67c61756`.
- Production baseline **1 passed**.
- 60-row hash verification: all 19 fixture hashes unchanged; long hash
  `889c3be4ccd53dd7` unchanged
  ([`hash-verification-final.log`](hash-verification-final.log)).
- Full release suite with artifacts: **336 passed, 0 failed, 5 ignored**
  ([`full-suite-with-artifacts.log`](full-suite-with-artifacts.log)).
- Without artifacts: **326 passed, 10 documented artifact-dependent
  failures, 5 ignored** ([`full-suite-without-artifacts.log`](full-suite-without-artifacts.log)).
- Licensing hygiene OK; `git diff --check` clean for the touched file.

## Decision

**Rejected and reverted.** The specialization is exactly equivalent but
produces identical machine code, so it cannot be faster; the measured
−0.279% median confirms no gain. The working tree is back to the starting
production diff (`bitenc.rs` 51, `quant.rs` 80, `tns.rs` 303 changed lines).

## Limitations

- One machine/session; the session warmed over the run (baseline 393–408 ms,
  pairs 400–414 ms), so only paired deltas are meaningful.
- The binary layout shifted between builds even though function code was
  identical, so a small layout-driven spread is expected and indistinguishable
  from noise here.
- Instruction-level profile attribution cannot separate a stall from the
  instruction it is reported against; the `asr`/`cmp` samples may reflect the
  dependency chain rather than two instructions of work.
- Byte-identical hashes establish bitstream identity on this corpus, not
  perceptual equivalence.

## Recommended next target

`bitcount::find_max_merge` (`bitcount.rs:817`, 3.07% corpus / 3.39% music)
remains the best-supported target: `dyn_bit_count_reuse` rescans the section
list after every merge (O(n²)), and an incremental maximum can preserve the
strict `>` first-max tie-break. Higher correctness risk; needs bitcount,
corpus and hash gates.

The second shift in `accumulate_dist` is a plausible residual micro-site —
for `scale < 2` the shift is either zero (`diff == 0`) or a no-op
(`scale2 == 0`), so `fpow2(diff) >> scale2.max(0)` would be exact and drop
the redundant left-shift plus one select — but that is 2–3 instructions in a
loop whose critical path is the `fpow2` multiply, well under the 1% gate.
Do not start there.
