# Optimized source-line profiling — 2026-10-28

Question: can the optimized encoder be profiled with useful Rust source-line
and inline attribution without materially distorting the workload? **Yes**,
using an optimized build with `debuginfo=2` and the default macOS dSYM split.
The investigation changed no production code and selected one clear next
target.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM
- macOS 27.0.1 (26A434) / Darwin 27.0.0
- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1, host
  `aarch64-apple-darwin`
- Tools: `/usr/bin/sample`, `/usr/bin/atos`, `/usr/bin/dwarfdump`,
  `/Library/Developer/CommandLineTools/usr/bin/llvm-dwarfdump`. No
  `llvm-addr2line`/`addr2line` on this host.
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index; `scf.rs`,
  `pe.rs`, and `block_switch.rs` had no edits from this investigation. All
  retained optimizations present. Full status:
  [`starting-state.txt`](starting-state.txt).

## Production-like baseline

Three independent 15-rep runs over 19 fixtures + 40 s derived workload:

| Run | Production total | Realtime × | RTF |
|---|---:|---:|---:|
| 1 | 393.374 ms | 232.5 | 0.004302 |
| 2 | 399.934 ms | 228.7 | 0.004373 |
| 3 | 402.381 ms | 227.3 | 0.004400 |
| **Median** | **399.934 ms** | **228.7** | **0.004373** |

The production-baseline test passed; every benchmark run validated all 19
fixture hashes and long-workload determinism.

## Profiling configurations

### Production-like (reference)

`cargo test --release --locked --features aac_enc --lib --no-run`
(default release, no debug info, default strip). Binary 10,930,480 B,
`__text` 1,834,748 B, no DWARF/dSYM. Median **399.934 ms**.

### Optimized + source-line DWARF

```sh
CARGO_TARGET_DIR=/private/.../srcline-dbg2 \
CARGO_PROFILE_RELEASE_STRIP=none \
RUSTFLAGS="-C debuginfo=2 -C strip=none" \
cargo test --release --locked --features aac_enc --lib --no-run
```

Important: the default macOS **dSYM split** must be kept. Adding
`-C split-debuginfo=off` produced no dSYM and no embedded DWARF on this
toolchain, so line resolution failed. Cargo otherwise passes
`-C strip=debuginfo`; `CARGO_PROFILE_RELEASE_STRIP=none` removes that and the
RUSTFLAGS above produce the dSYM.

- Binary: 11,149,232 B; `__text` 1,833,148 B; DWARF file 32,040,983 B
  (31 MB dSYM bundle).
- Line tables and inline frames resolve through `atos -i -o <binary>
  -l 0x100000000 <pc>`.
- Median runtime **404.660 ms** (+1.18% vs production); all hashes unchanged.
- `dbg-production-baseline.log`: the same pinned corpus test passes.

**Codegen comparison.** Most inspected functions have identical sizes:
`form_factor` 1600/1600, `calc_sfb_dist` 704/704, `quantize_spectrum`
576/576, `code_values` 3648/3648, `dyn_bit_count_reuse` 5104/5104.
`autocorr_quarter_lags` is 1184 vs 1212 B and `BlockSwitch7process` is
2296 vs 2292 B; `__text` is 1,600 B smaller overall (0.09%). The debug build
is therefore representative for source attribution but **not byte-identical**
and not suitable as acceptance-benchmark evidence.

Config table: [`config-comparison.tsv`](config-comparison.tsv).

## Source-line quality

`atos -i` resolves file:line and inline stacks for the sampled PCs. Examples:

```
block_switch.rs:462  calc_window_energy  <- process        (block_switch PC)
q31.rs:219           shift_int           <- accumulate_dist <- calc_sfb_dist
q31.rs:699           inv_sqrt_norm2      <- sqrt_fixp      <- form_factor
```

Limitations observed:

- Some hot PCs resolve to `file:0` (no line entry): the largest is
  `calc_sfb_dist +0x1d0` at 7.86% corpus (adjacent code is the `q` abs /
  `MAX_QUANT` gate and inverse-quantize path). `bitenc.rs:0` accounts for
  0.25%.
- One source line can map to multiple machine regions (e.g. `quant.rs:478`
  and inlined `q31.rs:219` appear at several call sites). Line sums are
  attribution, not exclusive cost centres.
- The debug build’s small code-layout differences mean individual PC offsets
  are representative but not interchangeable with the production binary.

Raw line data: [`dbg-hot-lines.tsv`](dbg-hot-lines.tsv),
[`dbg-hot-line-summary.tsv`](dbg-hot-line-summary.tsv),
[`dbg-multi-lines.tsv`](dbg-multi-lines.tsv).

## What the lines show (corpus, worker-normalised)

| Symbol | Source line / operation | Samples | Share |
|---|---|---:|---:|
| `calc_sfb_dist` | `quant.rs:0` — unresolved per-line q/abs/MAX_QUANT/inv path | 635 | 7.86% |
| `calc_sfb_dist` | `q31.rs:219` — `shift_int` left-shift branch (inlined via `accumulate_dist`) | 388 | 4.80% |
| `calc_sfb_dist` | `quant.rs:478` — `quantize_line_body` call | 383 | 4.74% |
| `BlockSwitch::process` | `block_switch.rs:462` — f32 IIR update in `calc_window_energy` | 274 | 3.39% |
| `dyn_bit_count_reuse` | `bitcount.rs:817` — `find_max_merge` scan | 248 | 3.07% |
| `form_factor` | `q31.rs:699` — second-order `inv_sqrt_norm2` correction | 206 | 2.55% |
| `autocorr_quarter_lags` | `tns.rs:481` — per-lag `fmult` accumulation | 205 | 2.54% |
| `calc_sfb_dist` | `quant.rs:476` — line-loop header | 193 | 2.39% |
| `quantize_spectrum` | `quant.rs:421` — `quantize_line_body` call | 129 | 1.60% |
| `form_factor` | `pe.rs:288` — `sqrt_fixp` call site | 45 | 0.56% |
| `code_values` | `bitenc.rs:369` — fallback byte-emission loop | 44 | 0.54% |

Workload stability: `block_switch.rs:462`, `tns.rs:481`, `pe` sqrt lines,
`quant.rs:478`, and `bitcount.rs:817` remain the top lines on music and
onset; onset’s `block_switch` share rises to 7.68% and its autocorr share to
4.95%. Raw profiles: `dbg-corpus.sample.txt`, `dbg-music.sample.txt`,
`dbg-onset.sample.txt`; shares: [`dbg-target-shares.tsv`](dbg-target-shares.tsv).

## Ranked next targets

### 1. `quant::accumulate_dist` — specialize the first shift (recommended)

`quant.rs:438` calls `shift_int(diff, scale)` where
`scale = count_leading_bits(diff) >= 0` for every reachable `diff`
(`diff` is a non-negative absolute difference; zero yields `scale == 0`).
The generic `shift_int` therefore always takes its left-shift arm for this
call, but the inlined code still carries the `s >= 0` select at
`q31.rs:219`, the 4.80% corpus line. Replacing only this call with
`(diff as u32).wrapping_shl(scale as u32) as i32` is exactly equivalent for
every input and removes the dead signed-shift branch/select. The second
`shift_int(diff, -scale2)` must remain generic (its direction depends on
`scale`).

- Estimated share: up to ~4.8% is attributed to the `shift_int` line; the
  actual removable portion is smaller. A 0.5–1.5% end-to-end gain is
  plausible, so the paired gate is required.
- Risks: low; integer-only, same shift count, same wrapping semantics.
- Next experiment: one-call specialization, then quant/qprobe tests,
  production-baseline, 19+long hashes, and four paired benchmarks.

### 2. `bitcount::find_max_merge` — repeated full scan after each merge

`bitcount.rs:817` accounts for 3.07% corpus / 3.39% music. The caller
(`dyn_bit_count_reuse`) repeatedly calls `find_max_merge` after every section
merge; each call rescans the current section list, giving an O(n²) pattern
over ~dozens of sections. An incremental maximum structure that preserves the
current first-maximum tie-break (`>` in section order) is the credible
optimization.

- Estimated share: ~3.1% corpus, ~3.4% music; a well-implemented incremental
  max could save 1–1.5% end-to-end.
- Risks: higher; must preserve exact merge order, strict `>` first-max
  tie-breaking, and section-list mutations exactly. Needs bitcount/Huffman,
  corpus, and hash tests.
- Next experiment only if target 1 does not clear the gate.

### 3. `BlockSwitch::calc_window_energy` — serial f32 IIR (not recommended)

`block_switch.rs:462` is the per-sample IIR update. It is 3.39% corpus /
7.68% onset, but the `s0`/`s1` state is initialized once before the eight
window loops and carried continuously across the full 1024 samples, so the
windows are not independent and cannot be interleaved without changing the
filter. This is necessary serial work; no exact local transformation is
evident. Do not pursue another unroll/transpose here.

## Correctness and final validation

No production code was changed. Final production-like tree:

- Production-baseline: **1 passed**.
- 60-row hash verification: all 19 fixture hashes unchanged; long hash
  `889c3be4ccd53dd7` unchanged
  ([`hash-verification-final.log`](hash-verification-final.log)).
- Full release suite with reference artifacts: **336 passed, 0 failed,
  5 ignored** ([`full-suite-with-artifacts.log`](full-suite-with-artifacts.log)).
- Full release suite without artifacts: **326 passed, 10 documented
  artifact-dependent failures, 5 ignored**
  ([`full-suite-without-artifacts.log`](full-suite-without-artifacts.log)).
- Licensing hygiene OK; `git diff --check` clean.

The debug-info build is profiling-only evidence; its output hashes also
passed, but its +1.18% runtime and small code-layout differences make it
unsuitable for acceptance benchmarking.

## Limitations

- One machine/session; production baseline noise is a few percent.
- `atos` line attribution is good but not complete: 7.86% of corpus samples
  in `calc_sfb_dist` resolve to `quant.rs:0`, and some inline frames are
  compiler-generated.
- Debug-info build is not byte-identical to production; line shares are
  representative, not interchangeable with the acceptance binary.
- Byte-identical hashes establish bitstream identity on the tested corpus,
  not perceptual equivalence for arbitrary inputs.
- No profiling build is proposed as a production change.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state, toolchain, host, binary hash. |
| `baseline-{1,2,3}.log`, `baseline-summary.tsv` | Production-like baseline. |
| `dbg-baseline-{1,2,3}.log`, `config-comparison.tsv` | Debug-info build runtime, size, and metadata. |
| `dbg-corpus.sample.txt`, `dbg-music.sample.txt`, `dbg-onset.sample.txt` | Raw debug-info profiles. |
| `dbg-corpus-bench.log`, `dbg-music-bench.log`, `dbg-onset-bench.log` | Profiling-run benchmark logs. |
| `dbg-target-shares.tsv` | Symbol self shares per workload. |
| `dbg-hot-offsets.tsv`, `dbg-hot-lines.tsv`, `dbg-hot-line-summary.tsv`, `dbg-multi-lines.tsv` | PC offsets, resolved lines, inline chains, and per-line sums. |
| `dbg-production-baseline.log` | Pinned corpus test on the debug binary. |
| `hash-verification-final.log`, `full-suite-*.log` | Final production-tree validation. |

The debug build and dSYM live outside the repository in the temporary
`srcline-dbg2` target directory. **No staging, commit, push, amend, reset,
rebase, cherry-pick, or history rewrite occurred.**
