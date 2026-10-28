# `calc_sfb_dist` `quant.rs:0` attribution — 2026-10-28

Question: what is the `quant.rs:0` region that carried 7.86% of corpus
samples in §21? **It is a DWARF line-table gap over 26 instructions in four
ranges**, not one operation and not a missing function: band-invariant
quantizer setup, the `q == 0` early-return path, the zero store, and the
`|q|` negation inside inlined `inv_quantize_line`. The line share is
reproducible (7.45–8.20% across three captures), but §21's per-PC split was
not; only the line total is defensible. No production code was changed.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM; macOS 27.0.1
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1,
  `aarch64-apple-darwin`
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index; retained
  `bitenc.rs`/`quant.rs`/`tns.rs` diffs untouched; `bitcount.rs` clean.
  Full state: [`starting-state.txt`](starting-state.txt).
- Production test binary
  `rust/target/release/deps/ferrite_aac-549f414e79686bc8`, SHA-256
  `61126376e5672d86ca9de8d23cbcfa4f8e8d52d2684f7386612b19ed67c61756`.

## Reproduction commands

```sh
# Attribution build (current tree, separate target dir)
CARGO_TARGET_DIR=/private/.../srcline-attr \
CARGO_PROFILE_RELEASE_STRIP=none \
RUSTFLAGS="-C debuginfo=2 -C strip=none" \
cargo test --release --locked --features aac_enc --lib --no-run

# Profile: 80 corpus repetitions, 10 s at 1 ms
AAC_PERF_BENCH=1 AAC_PERF_REPS=80 AAC_PERF_MODES=production AAC_PERF_QUIET=1 \
  /private/.../srcline-attr/release/deps/ferrite_aac-1c6dc192d7062723 \
  --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench &
PID=$!; sleep 1; sample "$PID" 10 1 -file capture-1.sample.txt; wait $PID

# Symbolication (static addresses, no slide)
atos -i -o <same binary> -l 0x100000000 0x100134dd0

# Line table
xcrun llvm-dwarfdump --debug-line <dSYM>/Contents/Resources/DWARF/<binary>
```

## Binary / dSYM identity

- Rebuilt from the current tree; binary and dSYM both
  `UUID 52E58310-6CC2-3673-B174-94EB9D9DE468`.
- The §21 build (`srcline-dbg2`, built 2026-10-28 18:26) has the **same
  UUID**, i.e. the old evidence was produced from the same code and the
  current tree reproduces it exactly.
- `calc_sfb_dist` is 704 B / 176 instructions in both the production and
  dSYM builds; normalized instruction streams are **identical (0
  differences)**, so the line attribution transfers directly to production.

## How the 7.86% was produced, and what was wrong with the per-PC numbers

`sample` emits one line per (stack, symbol, resolved line) and **merges
several leaf PCs** into it:

```
178 calc_sfb_dist + 464,180,...  [0x...dd0,0x...cb4,...]  quant.rs:0
```

The line-level total is valid: every PC in those groups resolves to
`quant.rs:0` (verified per PC with `atos`). The §21 parser took the *first*
offset in the list and assigned the whole group count to it, which produced
`+0x1d0 = 532 (6.58%)` and `+0xb4 = 103 (1.27%)`. Those per-PC numbers are
**not** measurements; only their sum (635, 7.86% of the §21 worker total) is.

Re-analysis with child-subtraction self time reproduces the line totals:

| Capture | Worker samples | `calc_sfb_dist` self | `quant.rs:0` self | Share |
|---|---:|---:|---:|---:|
| capture 1 | 8,310 | 1,745 | 619 | 7.45% |
| capture 2 | 8,344 | 1,753 | 647 | 7.75% |
| capture 3 | 8,261 | 1,848 | 677 | 8.20% |
| §21 corpus | 8,177 | 1,774 | 635 | 7.77% |
| §21 music | 6,677 | 1,801 | 688 | 10.30% |
| §21 onset | 6,576 | 374 | 28 | 0.43% |

Per-capture PCs and group counts:
[`capture-summary.tsv`](capture-summary.tsv),
[`q0-groups.tsv`](q0-groups.tsv). The three fresh captures reproduce §21.

## What the region contains

The DWARF line table has explicit line-0 rows inside `calc_sfb_dist`
(26 of 176 instructions, four ranges). `atos` returns `quant.rs:0` for
exactly those PCs; this is a genuine line-table gap, not a symbolication
error.

| Range | Bytes | Insns | Source operation | Frequency | Sampled |
|---|---:|---:|---|---|---|
| `0x038..0x03c` | 4 | 1 | `xfsf = 0` (quant.rs:475) | once per call | no |
| `0x05c..0x0b4` | 88 | 22 | band-invariant quantizer/table setup hoisted from the line loop (gain&3 table rows, `quantizer_shift`, dzone constant, QUANT/MTAB base pointers; quant.rs:468-474 + inlined `quantize_line_body` tables) | once per call | `0x94` rarely (1 sample in capture 3) |
| `0x0b4..0x0b8` | 4 | 1 | `mov w20,#0`: `inv_quantize_line`'s `if q == 0 { return 0; }` (quant.rs:344-346) feeding `accumulate_dist` with `inv = 0` | ~50% of all lines | `0xb4` in almost every group |
| `0x1ac..0x1b0` | 4 | 1 | zero store for the `q == 0` path (quant.rs:477-478) | ~50% of all lines | no |
| `0x1d0..0x1d4` | 4 | 1 | `neg`: `accu = if neg { -(q as i32) } else { q as i32 }` in inlined `inv_quantize_line` (quant.rs:350) | ~48% of all lines | `0x1d0` in almost every group |

Evidence: [`attribution-table.tsv`](attribution-table.tsv),
[`instruction-lines.tsv`](instruction-lines.tsv) (all 26 line-0
instructions with `atos` output),
[`dwarf-line-map-calc_sfb_dist.tsv`](dwarf-line-map-calc_sfb_dist.tsv)
(85 line rows with spans), and the disassembly
[`asm-calc_sfb_dist.txt`](asm-calc_sfb_dist.txt).

Why line 0: these are instructions LLVM hoisted, scheduled into the
prologue, merged across statements, or folded into `mov`/`neg` forms. The
line table marks them with no statement instead of guessing one. The
92-byte setup row at `0x5c` even spans two different logical operations
(per-call setup and the `0xb4` early-return entry), so a single line number
would be wrong for part of it in any case.

## Which paths dominate, and what remains unresolved

A temporary counter build (reverted; patch in
[`instrumentation-quant.patch`](instrumentation-quant.patch)) measured path
frequency:

```
music-stereo: calls=209522 lines=3011032 zero_q=1579458 (52.5%) maxq_ret=0
sine-stereo:  calls=6221   lines=41952   zero_q=21128   (50.4%) maxq_ret=0
onset-stereo: calls=3324   lines=24812   zero_q=11215   (45.2%) maxq_ret=0
noise-mono:   calls=4082   lines=59628   zero_q=5373    ( 9.0%) maxq_ret=0
```

So the two per-line line-0 paths (`0xb4` zero-quantized and `0x1d0` `|q|`)
run about equally often in music, and **both** appear in almost every
sampled group. The once-per-call setup is not a sample contributor
(`0x94` appears once in three captures).

**Unresolved:** the exact time split between `0xb4` and `0x1d0`.
`sample` records only the group count, and its address lists are truncated,
so no per-PC attribution is recoverable from these captures. The observed
PCs are also 1-cycle `mov`/`neg` operations at basic-block entries; samples
landing there most plausibly reflect the following dependency chain (load
of `spec`, `clz`, `fpow2`, shifts) or the taken `q == 0` branch rather than
the instruction itself. That is an interpretation, not a measurement.

## Is there a credible optimization?

**No, and the recommendation is to stop on this region.**

- The setup range is once per call and already hoisted; at 209,522 calls
  per second of music its ~22 instructions are a small fraction of a ~54 ms
  encode.
- The `q == 0` return and the zero store are the algorithm's own fast path.
- The `neg` is the magnitude needed by `inv_quantize_line`'s normalize/
  table lookup; a compare-only gate would still need it.
- The `0x1d0`/`0xb4` samples sit on the per-line distortion path whose
  arithmetic is already attributed to `quant.rs:433-438`; no removable
  operation is visible in the generated code.

One unverified microarchitectural hypothesis: `q == 0` is a ~50/50 branch
for music, so the test in `inv_quantize_line` may mispredict often. The
branch PC itself does not appear among the hot PCs, and making it
branchless would require computing the inverse-quantize path for zero
lines, so there is no evidence-backed change to make.

The only structural observation is the call rate: 209,522 calls and
3.0 M lines per second of music means **2,229 calls per channel-frame at
~14.4 lines/call** (scalefactor search). That is a caller-level question,
not supported by this profile, and is not proposed as a change.

## Validation

No production code changed. `quant.rs` was restored to the exact starting
diff (SHA-256 of `git diff` unchanged); `bitcount.rs` is clean; HEAD and
the index are unchanged; retained diffs remain 379 insertions / 55
deletions.

- Production binary SHA-256 unchanged
  (`61126376…`), production baseline **1 passed**.
- 60-row hash check unchanged: 19 fixtures and long hash
  `889c3be4ccd53dd7` ([`hash-verification-final.log`](hash-verification-final.log)).
- `git diff --check` clean.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state, toolchain, production hash. |
| `capture-{1,2,3}.sample.txt`, `capture-{1,2,3}-bench.log` | Raw profiles and benchmark logs. |
| `capture-summary.tsv`, `q0-groups.tsv` | Line shares and group counts/PC sets. |
| `quant-rs0-pcs.tsv`, `instruction-lines.tsv` | PC-level `atos` resolution and all line-0 instructions. |
| `dwarf-line-map-calc_sfb_dist.tsv` | DWARF line rows with address spans. |
| `asm-calc_sfb_dist.txt`, `asm-calc_sfb_dist-production.txt` | dSYM and production disassembly. |
| `attribution-table.tsv` | Region → operation → frequency mapping. |
| `quant-path-frequency.log`, `instrumentation-quant.patch` | Temporary counter run and its removed patch. |
| `production-baseline.log`, `hash-verification-final.log` | Final validation. |
| `quant-diff-sha-before.txt` | Retained `quant.rs` diff hash before/after. |
