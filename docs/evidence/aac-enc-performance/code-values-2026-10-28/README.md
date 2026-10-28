# `code_values` / bit-emission investigation — 2026-10-28

Investigation question: after the retained `calc_sfb_dist` and
`scale_up_spectrum` changes, is there a defensible, measurable optimization in
`bitenc::code_values`? The answer is **yes**, but not in the Huffman lookup
itself: the measured cost is the inlined `BitWriter::write_bits` buffer
update. The retained change replaces that per-byte merge loop with one
8-byte window merge per codeword.

## Environment

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 (26A434) / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- Release test binary `target/release/deps/ferrite_aac-549f414e79686bc8`,
  starting SHA-256
  `1016eeabef40d7da86d8a86c6588fac7c4e882096f78500d60549ed64b885b4e`
  (both retained optimizations present, `bitcount.rs` at HEAD).
- Default `aac_enc` feature; release profile; macOS `sample` at 1 ms.
- Starting HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index;
  `rust/src/aac_enc/bitcount.rs` has no diff; `quant.rs` and `tns.rs` retain
  their prior changes. See [`starting-state.txt`](starting-state.txt).

## Commands

From `rust/`:

```sh
# Current baseline, 19 fixtures + 40 s derived workload
AAC_PERF_BENCH=1 AAC_PERF_REPS=15 AAC_PERF_LONG=1 AAC_PERF_QUIET=1 \
  target/release/deps/ferrite_aac-549f414e79686bc8 \
  --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench

# Profile (example: corpus, 80 reps over all fixtures, 10 s sample window)
AAC_PERF_BENCH=1 AAC_PERF_REPS=80 AAC_PERF_MODES=production AAC_PERF_QUIET=1 \
  target/release/deps/ferrite_aac-549f414e79686bc8 \
  --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench &
PID=$!; sleep 1; sample "$PID" 10 1 -file corpus-before.sample.txt; wait $PID

# Correctness
cargo test --release --locked --features aac_enc --lib \
  aac_enc::production_baseline::production_f32_corpus_baseline_is_unchanged
cargo test --release --locked --features aac_enc --lib aac_enc::bitenc

# Production example (non-`cfg(test)`), 4 committed WAVs + 40 s workload
cargo build --release --locked --features aac_enc --example aac_bench_wav
AAC_WAV_REPS=15 target/release/examples/aac_bench_wav
```

## Starting baseline

Three independent 15-rep runs over all 19 fixtures plus the derived 40 s
workload:

| Run | Production total | Realtime × | RTF |
|---|---:|---:|---:|
| 1 | 436.632 ms | 209.40 | 0.004775 |
| 2 | 446.684 ms | 204.70 | 0.004884 |
| 3 | 447.323 ms | 204.40 | 0.004891 |
| **Median** | **446.684 ms** | **204.70** | **0.004884** |

This session runs slightly slower than the 435.320 ms median recorded in
§14; the machine drifted during the session. All A/B results below are paired
within-session, and the old/new binaries were alternated.

## Profile: the cost is the inlined writer, not Huffman lookup

`sample` self time, normalised to worker samples (idle and harness frames
excluded). The before files are `*-before.sample.txt`, the after files are
`*-after.sample.txt`.

| Workload | `code_values` before | Share | `code_values` after | Share | Drop |
|---|---:|---:|---:|---:|---:|
| corpus | 341 | 4.18% | 161 | 2.19% | 1.99 pt |
| onset | 35 | 0.55% | 33 | 0.58% | noise |
| music | 327 | 4.97% | 185 | 3.13% | 1.85 pt |
| speech | 187 | 2.87% | 110 | 1.87% | 1.00 pt |

In the before profile, **~98% of `code_values` self samples** on corpus and
music land in two instructions of the inlined `write_bits` loop in the
books 7–10 arm (`+0x59c` loop head, `+0x5e4` byte merge). Speech is dominated
by book 11 (`+0x73c` writer, `+0x748` escape handling); onset is dominated by
`bit_count` and `code_values` is only 0.55%.

The call graph shows `frame::encode_channel_spectral` as the caller.
`BitWriter::write_bits` has no separate call from `code_values` in the
optimized binary (it is inlined), so its bit-emission cost is attributed to
`code_values`. That is why the profile’s largest `code_values` cost is not a
Huffman-table lookup.

## Candidates considered

| # | Candidate | Evidence | Decision |
|---|---|---|---|
| 1 | Replace the per-byte `write_bits` merge with one 8-byte window | 98% of `code_values` self time in the inlined byte loop; 2–3 byte RMWs per codeword | **Selected** |
| 2 | Batch two books 7–10 pairs into one `write_bits` call | Halves call/setup overhead, but byte work is unchanged; estimated well below 1% | Not selected |
| 3 | Replace book 11 escape-length loop with `u32::ilog2` | Removes 8-instruction loop per escaped value, but speech-only and small aggregate weight | Not selected |

A write of at most 32 bits starting at any bit offset touches at most five
bytes, so an 8-byte window always covers the field and contains the current
partial byte. The writer is append-only, so bytes outside the written range
are read back unchanged and storing the window back is exact. This turns each
2–3 byte sequence of `ldrb`/`orr`/`strb` (plus per-byte bounds checks and a
`bits` store) into one `ldr x` / `orr` / `str x` plus the final `bits` store.

## Source change

Final diff: [`source-diff.patch`](source-diff.patch) (34 insertions, 17
deletions in `rust/src/aac_enc/bitenc.rs`). Summary:

- `write_bits` keeps its validation and error behavior unchanged.
- The emission loop becomes:
  - if `byte_idx + 8 <= buf.len()`, read the 8-byte big-endian window, OR the
    value at `64 - bit_offset - need`, and write the window back;
  - otherwise (final <8 buffer bytes), keep the original byte-at-a-time
    placement.
- `self.bits` is updated once, after the field is placed.
- `#[inline]` keeps the rewritten method inlined into the `code_values` arms;
  without it the compiler outlined the callee and added ~7 calls per
  `code_values` invocation.

No Huffman table, codeword, code length, escape value, sign handling, bit
order, scalefactor decision, quantization step, or frame-assembly rule
changed. No unsafe code or new dependency was added.

Interim note: two preliminary loop shapes were prototyped before this one.
A scalar split loop and a bounded four-byte loop both improved the isolated
writer (1.35× / 1.36×) but either did not inline or lost the gain in a
two-pair smoke test (`−0.93%`, `+0.05%`). They were replaced by the 8-byte
window before the formal benchmark; their smoke logs are
`interim-for4-*.log` and are not part of the retained A/B.

## Generated code

Hot block, books 7–10 arm:

Before (per-byte loop, 25 instructions visible in
[`asm-code_values.txt`](asm-code_values.txt)):

```
cmp/csel/subs/... loop control
ldrb w28, [x13, x10]
orr  w26, w28, w26
strb w26, [x13, x10]
str  x17, [x3, #0x18]
b.ne loop
```

After ([`asm-code_values-after.txt`](asm-code_values-after.txt)):

```
and  x20, x15, #0x7
ldr  x21, [x13, x9]      ; 8-byte window
neg/sub/lsl/rev/orr      ; place value, big-endian
str  x7,  [x13, x9]      ; write window back
str  x15, [x4, #0x18]    ; bits += need
```

Static sizes (whole functions, including cold panic/fallback paths):

| Function | Before | After |
|---|---:|---:|
| `code_values` | 0xcc0 (3,264 B), 816 instructions | 0xe40 (3,648 B), 912 instructions |
| `write_bits` | 0x13c (316 B), 79 instructions | 0x160 (352 B), 88 instructions |

The static count grew because the tail fallback is inlined into every arm;
the dynamically executed path shrank from a 2–3-iteration byte loop to one
window load/store. The profile drop above is the dynamic confirmation.

Isolated diagnostic (`writebits-window-microbench.rs`, 50 M writes of 7–14
bits at random bit offsets): **5.21 ns → 1.67 ns per write (3.13×)**,
identical output checksum ([output](writebits-window-microbench.txt)). This is
diagnostic only and is not used as the acceptance metric.

## End-to-end results

### Test build: eight alternating pairs

15 reps, all 19 fixtures + derived 40 s workload. Every pair is a fresh
process; order alternates.

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 449.940 ms | 441.165 ms | **1.950%** |
| 2 | new → old | 456.633 ms | 442.227 ms | **3.155%** |
| 3 | old → new | 455.468 ms | 445.193 ms | **2.256%** |
| 4 | new → old | 457.943 ms | 446.215 ms | **2.561%** |
| 5 | old → new | 446.609 ms | 440.887 ms | **1.281%** |
| 6 | new → old | 457.029 ms | 450.640 ms | **1.398%** |
| 7 | old → new | 458.411 ms | 450.708 ms | **1.680%** |
| 8 | new → old | 457.981 ms | 447.058 ms | **2.385%** |
| **Median** | | **456.831 ms** | **445.704 ms** | **2.103%** |

8/8 pairs positive; 8/8 at or above 1%; min 1.281%, max 3.155%. Raw logs:
`testbuild-{before,after}-{1..8}.log`; summary
[`optimization-testbuild-ab.tsv`](optimization-testbuild-ab.tsv).

### Per-workload medians (eight runs each)

| Fixture | Before | After | Improvement |
|---|---:|---:|---:|
| m2-silence-stereo-48k | 1.121 ms | 1.110 ms | 1.009% |
| m2-sine-stereo-48k | 2.489 ms | 2.465 ms | 0.967% |
| m2-onset-stereo-48k | 1.839 ms | 1.825 ms | 0.762% |
| m2-stepdown-stereo-48k | 1.818 ms | 1.809 ms | 0.502% |
| m2-click-mono-48k | 0.940 ms | 0.924 ms | 1.691% |
| m2-impulse-mono-48k | 0.173 ms | 0.171 ms | 1.001% |
| m2-edge-mono-48k | 0.201 ms | 0.202 ms | −0.612% |
| m3-multitone-stereo-48k | 2.849 ms | 2.830 ms | 0.678% |
| m3-sweep-stereo-48k | 2.612 ms | 2.582 ms | 1.152% |
| m3-noise-mono-48k | 1.303 ms | 1.254 ms | 3.773% |
| m3-tonal-noise-stereo-48k | 1.795 ms | 1.743 ms | 2.908% |
| m3-quiet-sine-stereo-48k | 2.624 ms | 2.609 ms | 0.557% |
| m3-sine-stereo-441k | 2.596 ms | 2.571 ms | 0.948% |
| m3-onset-stereo-441k | 1.829 ms | 1.810 ms | 1.022% |
| m6-stress-stereo-48k | 1.880 ms | 1.825 ms | 2.955% |
| m7-music-stereo-48k | 59.763 ms | 58.131 ms | 2.731% |
| m7-speech-pauses-stereo-48k | 38.249 ms | 37.514 ms | 1.922% |
| m7-speech-clean-stereo-48k | 38.253 ms | 37.600 ms | 1.706% |
| m8-music-eroica-stereo-48k | 55.600 ms | 54.245 ms | 2.438% |
| m7-music-stereo-48k-x4 (40 s) | 238.197 ms | 232.120 ms | 2.551% |

19 of 20 workloads improved. The only negative is the 0.2 ms synthetic
`m2-edge-mono-48k` at −0.61%, which is below the run-to-run variation for a
fixture that short and is not a meaningful workload regression. Speech,
music, Eróica, noise/stress, and the 40 s workload all improved.

### Production example (non-`cfg(test)`, four WAVs + 40 s derived)

Four alternating pairs, `AAC_WAV_REPS=15`:

| Pair | Order | Before total | After total | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 421.659 ms | 414.110 ms | **1.790%** |
| 2 | new → old | 427.304 ms | 417.836 ms | **2.216%** |
| 3 | old → new | 427.591 ms | 419.405 ms | **1.914%** |
| 4 | new → old | 429.452 ms | 417.371 ms | **2.813%** |
| **Median** | | | | **2.065%** |

All four pairs ≥1%. Per-fixture medians: music +2.267%, speech-clean
+1.565%, speech-pauses +1.690%, Eróica +2.296%, long +2.532%. Logs
`wav-{before,after}-{1..4}.log`; summary
[`optimization-wav-example-ab.tsv`](optimization-wav-example-ab.tsv).

## Correctness

- `aac_enc::bitenc`: **9 passed** (writer packing, 32-bit write, zero-length,
  invalid length/value/overflow, C-exact probe transcript, corpus payloads).
- `aac_enc::bitcount`: **11 passed**; `frame`: **9**; `quant`: **10**;
  `tns`: **5**; `block_switch`: **8**; `psy`: **14** — all passed.
- Production-baseline regression: **1 passed**.
- Hash verification run: 60 rows (19 fixtures × production/feed/core + long);
  the benchmark asserts the committed byte totals and FNV-1a hashes for all
  19 fixtures and long-workload determinism. All unchanged; the long-workload
  production/core hash is `889c3be4ccd53dd7` in both arms
  ([`hash-verification-attempted.log`](hash-verification-attempted.log)).
- Full release suite with reference artifacts: **336 passed, 0 failed,
  5 ignored** ([`full-suite-with-artifacts.log`](full-suite-with-artifacts.log)).
- Full release suite without reference artifacts (documented
  artifact-dependent failures): **326 passed, 10 failed, 5 ignored**
  ([`full-suite-without-artifacts.log`](full-suite-without-artifacts.log)).
  The 10 are the known `m5a_decode_check`, `phase1_*`, and `phase1i_*`
  artifact tests; they are not caused by this change.
- `rust/tools/check-licensing.sh`: hygiene OK (67 `rust/src` files carry the
  modified-version notice).
- `git diff --check` (scoped to `bitenc.rs`): clean.
- No expected output, reference artifact, or test expectation changed.

## Decision

**Retained.** The change clears the gate on both arms (test-build median
**2.103%**, 8/8 pairs ≥1%; production example median **2.065%**, 4/4 pairs
≥1%), with all fixture and long-workload hashes unchanged, all relevant tests
passing, and no meaningful workload regression. The post-change profile
confirms the targeted cost fell (`code_values` 4.18% → 2.19% corpus,
4.97% → 3.13% music, 2.87% → 1.87% speech).

## Limitations

- One machine and one session; the machine drifted upward during the
  session. Alternating pairs and within-pair medians are the primary
  evidence; absolute before/after medians differ from the session-start
  baseline.
- `sample` is function-level only (release `strip=debuginfo`); inlined
  `write_bits` cost is attributed to `code_values`. PC skid can move a sample
  by a few instructions.
- Static `code_values` grew ~12% because the tail fallback is inlined into
  every arm; the dynamic hot path shrank. I-cache effects were not isolated.
- No perceptual-quality claim is made. Byte/hash equality establishes
  bitstream identity on the tested corpus, not listening quality.
- The 8-byte window relies on the append-only, zero-initialized buffer; the
  fallback preserves correctness for the final <8 bytes and for tiny test
  writers.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state, toolchain, host facts at start. |
| `baseline-{1,2,3}.log`, `baseline-summary.tsv` | Session-start baseline. |
| `corpus-before.sample.txt`, `onset-before.sample.txt`, `music-before.sample.txt`, `speech-before.sample.txt` | Before profiles. |
| `corpus-after.sample.txt`, `onset-after.sample.txt`, `music-after.sample.txt`, `speech-after.sample.txt` | After profiles. |
| `code-values-self.tsv`, `code-values-self-after.tsv` | Symbol self-sample summaries. |
| `asm-code_values.txt`, `asm-code_values-after.txt` | Full before/after `code_values` disassembly. |
| `asm-write_bits-before.txt`, `asm-write_bits-after.txt` | Standalone writer disassembly. |
| `codegen-hot-block.txt` | Before/after hot-block excerpt. |
| `source-diff.patch` | Final production diff. |
| `writebits-window-microbench.rs`, `writebits-window-microbench.txt` | Isolated diagnostic writer comparison and output. |
| `testbuild-{before,after}-{1..8}.log` | Eight paired test-build runs. |
| `optimization-testbuild-ab.tsv`, `optimization-workloads.tsv` | Paired and per-workload summaries. |
| `wav-{before,after}-{1..4}.log` | Four paired production-example runs. |
| `optimization-wav-example-ab.tsv` | Example summary. |
| `hash-verification-attempted.log` | Post-change hash verification. |
| `full-suite-with-artifacts.log`, `full-suite-without-artifacts.log` | Full suite runs. |
| `interim-for4-*.log` | Two-pair smoke test of a replaced interim loop shape. |

No C oracle was built, no reference artifact was regenerated, and no
perceptual test was used. **No staging, commit, push, amend, reset, rebase,
cherry-pick, or history rewrite occurred.**
