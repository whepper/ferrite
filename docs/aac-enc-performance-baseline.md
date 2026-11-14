# Ferrite AAC-LC VBR-5 encoder — performance baseline and profiling audit

Date: 2026-10-28. Status: **baseline complete; `calc_sfb_dist` optimization
retained (§11); front-end profiling recommendation recorded (§12);
`tns::scale_up_spectrum` write-loop optimization retained (§13); bit-path
re-profile completed and the `bit_count` candidate rejected/reverted (§14);
`code_values` `write_bits` 8-byte-window optimization retained (§15);
scalefactor-search investigation completed with no retained change (§16);
`quantize_spectrum` shared quantizer-branch optimization retained (§17);
`calc_sfb_quant_energy_and_dist` unroll candidate rejected/reverted (§18);
`pe::form_factor` investigated with no justified change (§19);
`tns::autocorr_quarter_lags` loop transpose rejected/reverted (§20);
optimized DWARF source-line profiling established and next targets ranked
(§21); first `shift_int` call-site specialization rejected/reverted with the
§21 attribution corrected (§22); `find_max_merge` compacted-candidate rewrite
rejected/reverted at +0.846% median (§23); `calc_sfb_dist` `quant.rs:0`
region resolved to line-table gaps over 26 instructions (§24); scalefactor
search audited with 13–18% exact intra-pass `(band, gain)` repeats and a
per-band cache hypothesis (§25); per-band LRU-8 cache implemented, proven
bit-exact, and rejected/reverted at −0.66% median (§26); rejected candidates
reassessed and `find_max_merge` retained under a new prospective Tier 2
small-win policy (§27); SCF trial-array reuse re-tested under Tier 2 and
rejected/reverted at +0.246% median (§28); aggregate checkpoint measured the
retained set at **+13.93%** versus a reconstructed pre-optimization baseline
(§29); no commit or push.**

Scope: the validated single-threaded AAC-LC VBR-5 path, mono/stereo,
44.1/48 kHz, ADTS, using the public production `Encoder` API
(`rust/src/aac_enc/encoder.rs`). Evidence and raw profiler output are in
[`docs/evidence/aac-enc-performance/`](evidence/aac-enc-performance/README.md).

Sections 1–10 preserve the original pre-optimization baseline as measured.
Section 11 records the dated `calc_sfb_dist` follow-up experiment, its retained
source change, and its before/after measurements; the baseline tables are
intentionally not rewritten. Section 12 records the 2026-10-27 front-end
profiling pass and its recommendation. Section 13 records the 2026-10-28
`tns::scale_up_spectrum` write-loop experiment and its retained change.
Section 14 records the updated bit-path profile and the rejected/reverted
`bit_count` experiment. Section 15 records the `code_values` investigation,
the retained 8-byte-window writer change, and its paired benchmarks.
Section 16 records the scalefactor-search investigation, the rejected
per-candidate trial-copy hoist, and the final no-change disposition.
Section 17 records the `quantize_spectrum` investigation and the retained
shared-helper sign-path unification. Section 18 records the rejected
`calc_sfb_quant_energy_and_dist` two-line unroll and the no-change
disposition. Section 19 records the `pe::form_factor` investigation and its
no-change disposition. Section 20 records the rejected
`tns::autocorr_quarter_lags` loop transpose. Section 21 records the
optimized DWARF source-line profiling method and the ranked targets.
Sections 22–28 record the subsequent rejected or reverted candidates, the
retained `find_max_merge` decision, and the reverted SCF trial-array
experiment. Section 29 is the aggregate performance checkpoint for the
complete retained set.

## 1. Executive summary

**Verdict: PASS WITH FINDINGS.**

A reproducible release-build baseline and a function-level hotspot map now
exist. The findings are performance findings, not correctness defects:

- **Throughput.** On the 19 committed fixtures (51.45 s of audio), the median
  of four independent runs is **225.0 ms**, or **228.6× realtime** (RTF
  **0.00437** encoding-seconds per audio-second). Including the derived 40 s
  music workload, the median is **470.6 ms** for 91.45 s, or **194.3×
  realtime** (RTF **0.00515**). The slowest measured workload is music:
  **60.9–62.1 ms** for 10 s (median 61.26 ms, **163.3×**); the 40 s music
  workload is **244.5 ms** (**163.6×**). The fastest fixtures are short or
  silent; their high realtime factors include a relatively larger fixed setup
  share.
- **Test-harness bias is bounded.** A separate non-`cfg(test)` example on the
  four committed WAVs was **0.25–0.86% faster** than the test-build benchmark
  with identical bytes/hashes. The full 19-fixture numbers are therefore
  conservative by roughly that fraction, not a test-harness artifact of
  unknown size.
- **Dominant bottleneck.** The S4 scalefactor/quantization search accounts for
  **49.9%** of encoder self time on the 19-fixture aggregate and **58.2%** on
  10 s music. `quant::calc_sfb_dist` alone is **24.4%** of aggregate self time
  and **29.0%** on music. The hot loop is scalar: the ARM64 disassembly shows
  per-line bounds/panic paths and no NEON arithmetic.
- **Second bottleneck.** The psychoacoustic/transform front-end (TNS, block
  switching, FFT/MDCT/DCT, PE/form-factor, energy, thresholds, intensity) is
  **25.8%** aggregate, **28.4%** on speech, and **51.8%** on the 1 s onset
  fixture. Transient-heavy content is front-end dominated.
- **Third bottleneck.** Bit counting plus Huffman encoding is **10.3%**
  aggregate and **11.0%** on music. Memory/allocator self time is **7.4%**
  aggregate and **13.4%** on onset. Rate/QC bookkeeping is **2.1%** self time;
  the VBR-5 path remains a single-pass accounting wrapper in this respect.
- **Highest-value first experiment.** Clean up the `quant::calc_sfb_dist`
  line loop (single length assertion plus `zip` iteration, and provably-safe
  table indexing) so LLVM can drop the per-line bounds/panic branches without
  changing one arithmetic operation. This is exact by construction, isolated,
  and directly targets the largest measured self-time symbol. Realistic
  expectation: **1–4% overall**, not a large algorithmic win.
- **SIMD/configuration note.** The inspected tree has no explicit SIMD feature:
  `rust/Cargo.toml` defines only `default = ["aac_enc"]` and `aac_enc = []`;
  `.cargo/config.toml` contains only aliases; `build.rs` only sets rustversion
  cfgs. The release rustc invocation has no `target-cpu` or LTO override.
  aarch64 baseline NEON is available to LLVM, but the profiled hot loop did not
  vectorise. This is recorded as a configuration fact, not asserted as a
  defect or as the missing optimization.

The baseline measurements above were taken before any optimization. The
follow-up §11 changes only the `calc_sfb_dist` line loop and two equivalent
table-index masks; it is retained. No build profile, public behavior, or
expected output changed.

## 2. Hardware, compiler, build profile, features

| Item | Value |
|---|---|
| Host | Apple M5, arm64 (`aarch64-apple-darwin`), 10 logical CPUs, 16 GiB RAM |
| OS | Darwin 27.0.0 / macOS 27.0.1 (26A434) |
| Rust | rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1 |
| Cargo | 1.99.1 (5f94df478 2026-08-27) |
| Build | `cargo test --release --locked --features aac_enc --lib` |
| Profile | release: `opt-level=3`, `debug-assertions=off`, `overflow-checks=off`, LTO off, `codegen-units=16`, `strip=debuginfo` |
| Features | `default = ["aac_enc"]`; `aac_enc` only; no SIMD feature, no target-cpu override |
| Threading | single encoder thread; no CPU affinity/pinning |
| Profiler | macOS `sample`, 1 ms interval, 5–8 s windows |

The release profile is the Cargo default plus the workspace's separate
`dev-release` definition (not used here). The observed rustc command is
recorded in
[`build-flags.txt`](evidence/aac-enc-performance/build-flags.txt). There is no
`RUSTFLAGS`, no `-C target-cpu`, and no LTO flag. The benchmark therefore
measures a generic `aarch64-apple-darwin` build, not a `target-cpu=native`
build.

**SIMD check.** The project context mentions SIMD enabled by default through
configured feature flags. In this worktree the only features are `aac_enc` and
`default`; no `simd`, `std::simd`, `target_feature`, or intrinsic module is
declared. The aarch64 baseline does include NEON for LLVM auto-vectorisation,
but the `quant::calc_sfb_dist` disassembly excerpt contains no `fmla`, `fmul`,
Q-register loads/stores, or `addv`, while it does contain scalar `smull`/`clz`
and four `panic_bounds_check` call sites
([`calc-sfb-dist-disassembly.txt`](evidence/aac-enc-performance/calc-sfb-dist-disassembly.txt)).
This audit does not assume SIMD is the missing optimization; it records what
the build and profile actually show.

## 3. Workloads and benchmark methodology

### 3.1 Workloads

The 19 committed `M58_FIXTURES` are used unchanged: 15 synthetic signals, three
licensed speech/music recordings, and the Eroica excerpt. One derived
benchmark-only workload, `m7-music-stereo-48k-x4`, concatenates the committed
music PCM four times in memory (40.0 s stereo); it is not a new reference
artifact.

Representative coverage:

| Category | Fixtures |
|---|---|
| Speech | `m7-speech-clean-stereo-48k`, `m7-speech-pauses-stereo-48k` (10 s stereo 48 kHz) |
| Music / tonal | `m7-music-stereo-48k`, `m8-music-eroica-stereo-48k` (10 s stereo 48 kHz), `m3-multitone-stereo-48k`, `m2-sine-stereo-48k`, `m3-sweep-stereo-48k`, `m3-quiet-sine-stereo-48k` |
| Transient / percussive | `m2-onset-stereo-48k`, `m2-click-mono-48k`, `m2-impulse-mono-48k`, `m3-onset-stereo-441k`, `m6-stress-stereo-48k` |
| Mono | `m2-click-mono-48k`, `m2-impulse-mono-48k`, `m2-edge-mono-48k`, `m3-noise-mono-48k` |
| 44.1 kHz | `m3-sine-stereo-441k`, `m3-onset-stereo-441k` |
| Long steady state | `m7-music-stereo-48k-x4` (40 s stereo, derived) |

Sample counts per channel: 10 s fixtures 480,000; 1 s fixtures 48,000 (44,100
at 44.1 kHz); 0.5 s fixtures 24,000; 0.25 s 12,000; 0.2 s 9,600; long workload
1,920,000. All are 16-bit PCM. The frame law is
`ceil((samples + 1600) / 1024)`, matching the production feeder delay.

### 3.2 Timed boundary

- **Timed:** `Encoder::new` (reported separately) and all `Encoder::push` /
  `Encoder::flush` calls. This includes the production `PcmFeeder`,
  deinterleave, per-frame channel clones, block switching, psychoacoustics,
  TNS, quantization/scalefactor search, bit counting, Huffman encoding, and
  ADTS frame assembly.
- **Not timed:** fixture synthesis/decoding, interleaving, output validation,
  hashing, and disk I/O. `aac-convert`'s M4A mux and file write are outside;
  these are encoder-throughput numbers, not full conversion throughput.
- **Chunking:** 8,000 sample frames per channel, matching
  `aac-convert::CHUNK_FRAMES`. This is the production chunk target; it also
  bounds the feeder's front-drain behaviour.
- **Modes:** `production` is the public `Encoder` API. `feed` is
  `PcmFeeder::push_interleaved`/`finish` only. `core` is prebuilt feeder frames
  through the same per-frame clone and `StreamEncoder::step_frame` loop used by
  `Encoder::emit_frames`, excluding the feeder. `production` is the headline;
  the other modes decompose overhead.
- **Output buffer:** the benchmark preallocates the output byte vector outside
  the timed region; copying each emitted ADTS frame into it is inside. This
  models a caller with a pre-sized output buffer and avoids measuring
  `Vec` growth. Encoder-internal allocations (per-frame channel clones,
  `PcmFeeder` frames, `Vec<AdtsFrame>`) remain inside the timed region.
- **No C comparison.** `tools/c-enc-bench` exists but requires building the
  external pinned C reference. It was not run, and no absolute C-vs-Rust
  throughput claim is made. The historical M5.21 coarse ratio is not treated
  as an equivalent benchmark.

### 3.3 Repetition and statistics

For each fixture and mode: one untimed warm-up pass, then **15 measured
repetitions** with a fresh encoder per repetition. The complete benchmark was
run **four times independently**. The detailed table uses the final run; the
aggregate and spread use all four. The aggregate is a median of complete-set
per-repetition sums, not a sum of per-fixture minima.

### 3.4 Definitions

- **RTF** (as requested) = encoding wall time / audio duration. **Lower is
  faster.**
- **Realtime ×** = audio duration / encoding wall time. **Higher is faster.**
- **Samples/s** = input samples per channel / encoding wall time.
- **Total** includes `Encoder::new`; **encode** excludes it. Init is reported
  separately and is typically 15–25 µs.
- **Frames** and **bytes** are the ADTS output; bytes include the 7-byte
  header per frame.

## 4. Baseline results

### 4.1 Per-workload medians (final run, 15 repetitions)

| Fixture | sr / ch | Duration s | Frames | Bytes | Encode ms | Total ms | RTF | Realtime × | Samples/s |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `m7-music-stereo-48k` | 48000 / 2 | 10.0 | 471 | 250,926 | 61.211 | 61.229 | 0.006123 | 163.3 | 7,839,364 |
| `m7-speech-clean-stereo-48k` | 48000 / 2 | 10.0 | 471 | 127,900 | 39.572 | 39.591 | 0.003959 | 252.6 | 12,123,980 |
| `m7-speech-pauses-stereo-48k` | 48000 / 2 | 10.0 | 471 | 125,818 | 39.330 | 39.352 | 0.003935 | 254.1 | 12,197,511 |
| `m8-music-eroica-stereo-48k` | 48000 / 2 | 10.0 | 471 | 234,229 | 56.942 | 56.967 | 0.005697 | 175.5 | 8,425,956 |
| `m7-music-stereo-48k-x4` | 48000 / 2 | 40.0 | 1,877 | 997,957 | 244.367 | 244.389 | 0.006110 | 163.7 | 7,856,322 |
| `m2-onset-stereo-48k` | 48000 / 2 | 1.0 | 49 | 2,359 | 1.909 | 1.926 | 0.001926 | 519.1 | 24,917,280 |
| `m3-onset-stereo-441k` | 44100 / 2 | 1.0 | 45 | 2,250 | 1.905 | 1.925 | 0.001925 | 519.4 | 22,905,117 |
| `m2-click-mono-48k` | 48000 / 1 | 0.5 | 25 | 2,680 | 0.954 | 0.969 | 0.001939 | 515.8 | 24,757,148 |
| `m3-noise-mono-48k` | 48000 / 1 | 0.5 | 25 | 10,603 | 1.345 | 1.363 | 0.002727 | 366.8 | 17,604,446 |
| `m3-multitone-stereo-48k` | 48000 / 2 | 1.0 | 49 | 4,819 | 2.962 | 2.977 | 0.002977 | 335.9 | 16,121,584 |
| `m3-sine-stereo-441k` | 44100 / 2 | 1.0 | 45 | 2,998 | 2.675 | 2.693 | 0.002694 | 371.3 | 16,372,749 |
| `m2-silence-stereo-48k` | 48000 / 2 | 1.0 | 49 | 735 | 1.160 | 1.184 | 0.001184 | 844.4 | 40,531,982 |
| `m2-edge-mono-48k` | 48000 / 1 | 0.25 | 14 | 415 | 0.203 | 0.218 | 0.000873 | 1,145.3 | 54,972,491 |
| `m6-stress-stereo-48k` | 48000 / 2 | 0.5 | 25 | 10,728 | 1.949 | 1.967 | 0.003935 | 254.2 | 12,199,517 |

The complete 20-row table, including all synthetic/44.1 kHz/mono fixtures, is
[`benchmark-medians.tsv`](evidence/aac-enc-performance/benchmark-medians.tsv).
Every row's bytes/hash matched the committed production baseline.

### 4.2 Aggregate throughput

| Set | Audio s | Production total | RTF | Realtime × |
|---|---:|---:|---:|---:|
| 19 committed fixtures (four-run estimator) | 51.45 | 225.0 ms (median of four run sums) | 0.00437 | **228.6×** |
| 19 committed fixtures (direct 15-rep aggregate) | 51.45 | 222.5 ms | 0.00433 | **231.2×** |
| Derived 40 s music | 40.00 | 244.5 ms (median of four runs) | 0.00611 | **163.6×** |
| All 20 workloads | 91.45 | 470.6 ms (median of four aggregate runs) | 0.00515 | **194.3×** |

The four independent production aggregate totals were **468.2, 481.9, 471.8,
and 469.3 ms** (189.8×–195.3×). The median is 470.6 ms / 194.3×; the final run
in the detailed table is 469.3 ms / 194.9×. The 19-fixture subset per run was
223.2–225.1 ms (228.5×–230.5×), median 225.0 ms / 228.6×. A separate direct
15-rep aggregate over the 19 fixtures (no derived workload) measured
**222.5 ms / 231.2×**; the difference from the 225.0 ms four-run estimator is
an estimator effect (per-rep aggregate vs sum of per-fixture medians), not a
contradiction. The direct run is retained as
[`benchmark-19-direct.log`](evidence/aac-enc-performance/benchmark-19-direct.log).

Per-fixture run-to-run spread is
[`benchmark-fixture-spread.tsv`](evidence/aac-enc-performance/benchmark-fixture-spread.tsv).
The largest spread is 3.61% on the 0.5 s tonal-noise fixture; all 10 s fixtures
are ≤1.90%, and the 40 s workload is 0.93%. The aggregate spread is roughly −1.1%/+1.8%
around the median. This supports the medians as stable for this machine/build.

### 4.3 Mode decomposition

| Mode | What it times | Final-run total | Share of production |
|---|---|---:|---:|
| `production` | `Encoder::new` + `push`/`flush` | 469.3 ms | 100% |
| `core` | prebuilt frames + per-frame clone + `step_frame`, no feeder | 459.7 ms | 97.9% |
| `feed` | `PcmFeeder` only | 8.1 ms | 1.7% |

The feeder is not a material aggregate bottleneck, but it is **4.5%** of the
1 s onset profile and **2.1%** of the 0.5 s noise profile. `Encoder::new` is
15–25 µs per stream (<0.03% for 10 s, ~1% for the 0.2 s impulse fixture).
The feeder-only measurement is stable at ~8 ms across runs. The separately
timed `production - core` difference varies (3.9–9.7 ms across the four runs),
so the two decompositions agree only approximately; per-frame orchestration
and run noise prevent an exact subtraction.

Historical sanity check: the earlier M5.10/M5.21 work recorded aggregate
223.8× and worst-case music 159×; this audit measures 228.6× for the 19-fixture
set and 161–164× for 10 s/40 s music. The agreement is close but not a
controlled comparison: compiler patch level, harness, fixture set, and
statistics differ.

### 4.4 Production-build check without `cfg(test)`

The full 19-fixture inventory is driven from the test-gated `testdata` fixture
synthesizer, so its numbers include the `cfg(test)` build (notably the
test-only failure-injection branch in `Encoder::emit_frames`). To bound that
bias, `rust/examples/aac_bench_wav.rs` encodes the four committed WAV fixtures
and the derived 40 s workload through the same public `Encoder` API in a normal
`cargo run --release --example` build, with no test harness. It validates the
same committed byte totals and hashes.

| Fixture | Test-build total | Production-example total | Test-build overhead | Test-build × | Production-example × |
|---|---:|---:|---:|---:|---:|
| `m7-music-stereo-48k` | 61.229 ms | 60.947 ms | +0.46% | 163.3 | 164.1 |
| `m7-speech-clean-stereo-48k` | 39.591 ms | 39.253 ms | +0.86% | 252.6 | 254.8 |
| `m7-speech-pauses-stereo-48k` | 39.352 ms | 39.088 ms | +0.68% | 254.1 | 255.8 |
| `m8-music-eroica-stereo-48k` | 56.967 ms | 56.816 ms | +0.27% | 175.5 | 176.0 |
| `m7-music-stereo-48k-x4` | 244.389 ms | 243.773 ms | +0.25% | 163.7 | 164.1 |

The production example is **0.25–0.86% faster** on the same fixtures and
produces identical bytes/hashes. The full 19-fixture test-build aggregate is
therefore conservative by roughly that fraction; it is not claimed as a
test-free production number for the synthetic fixtures. The example cannot
drive the 15 synthetic fixtures because their PCM is generated in test-gated
code; extending it would require a production-visible fixture source, which is
outside this audit.

## 5. Profiler results

### 5.1 Tool and method

`sample` was run at a 1 ms interval against release test binaries:

| Profile | Workload | Repetitions | Sample window |
|---|---|---|---:|---:|
| `corpus-19` | all 19 committed fixtures | 50 | 8 s |
| `music-stereo` | `m7-music-stereo-48k` | 300 | 8 s |
| `speech-clean-stereo` | `m7-speech-clean-stereo-48k` | 300 | 6 s |
| `onset-stereo` | `m2-onset-stereo-48k` | 6,000 | 6 s |
| `noise-mono` | `m3-noise-mono-48k` | 8,000 | 5 s |

`sample` reports leaf self time and a call graph. Because the release profile
strips debuginfo, there is no source-line attribution; symbols may include
inlined work. Shares below are normalised to **worker samples** after
excluding the idle main thread, test-harness frames, and sampling overhead.
`xctrace` was unavailable (full Xcode not installed), `dtrace` was blocked by
System Integrity Protection, and `perf` was absent; `sample` was the only
usable sampling profiler.

### 5.2 Self-time by stage

| Category | Corpus 19 | Music | Speech | Onset | Noise |
|---|---:|---:|---:|---:|---:|
| S4 scalefactor/quantization (`scf` + `quant` + `q31`) | **49.9%** | **58.2%** | **47.8%** | **15.3%** | **53.7%** |
| Front-end (TNS, transform/MDCT/FFT/DCT, PE, block switch, psy, energy, thresholds, intensity) | **25.8%** | 20.4% | **28.4%** | **51.8%** | 21.5% |
| Bit counting + Huffman encoding | **10.3%** | **11.0%** | 9.0% | 6.4% | **10.7%** |
| Memory copies + allocator | 7.4% | 5.3% | 7.1% | **13.4%** | 6.9% |
| Rate / QC bookkeeping | 2.1% | 1.8% | 2.5% | 3.5% | 2.4% |
| Feeder | 1.6% | 1.3% | 1.9% | **4.5%** | 2.1% |
| Other | 1.7% | 1.0% | 1.7% | 3.0% | 2.1% |

The onset profile is deliberately a transient-path view: the 1 s fixture has
only 49 frames, so setup/feeder and short-window work are over-weighted
relative to a long stream. The corpus aggregate and the 10 s/40 s workloads are
the steady-state references.

### 5.3 Top self-time symbols

Corpus aggregate (worker share):

| Rank | Symbol | Self | Share |
|---:|---|---:|---:|
| 1 | `quant::calc_sfb_dist` | 1,595 | **24.4%** |
| 2 | `scf::estimate_channel_with_inv_ws` | 573 | 8.8% |
| 3 | `quant::quantize_spectrum` | 275 | 4.2% |
| 4 | `tns::autocorr_quarter_lags` | 237 | 3.6% |
| 5 | `bitenc::code_values` | 222 | 3.4% |
| 6 | `bitcount::dyn_bit_count_reuse` | 215 | 3.3% |
| 7 | `quant::calc_sfb_quant_energy_and_dist` | 206 | 3.2% |
| 8 | `q31::calc_ld_data` | 204 | 3.1% |
| 9 | `block_switch::BlockSwitch::process` | 197 | 3.0% |
| 10 | `scf::assimilate_single_scf` | 184 | 2.8% |
| 11 | `bitcount::bit_count` | 179 | 2.7% |
| 12 | `pe::form_factor` | 179 | 2.7% |
| 13 | `common::fft::dit_fft` | 175 | 2.7% |
| 14 | `tns::scale_up_spectrum` | 162 | 2.5% |
| 15 | `platform_memmove` | 156 | 2.4% |

Music is more concentrated: `calc_sfb_dist` **29.0%**, estimate **8.4%**,
`quantize_spectrum` **5.1%**, `code_values` **5.0%**, `assimilate_single_scf`
**4.2%**. Noise is similar: `calc_sfb_dist` **30.2%**. Speech is **23.1%**
`calc_sfb_dist` and **3.9%** TNS autocorrelation. Onset is flat and
front-end-heavy: FFT **6.9%**, block switch **6.7%**, TNS `scale_up_spectrum`
**6.4%**, `psy::process_pair_reuse` **5.5%**, `calc_sfb_dist` **5.2%**,
`memmove` **4.5%**, `pe::form_factor` **4.5%**, feeder **4.5%**.

Full top-20 lists are in
[`profile-top-self.tsv`](evidence/aac-enc-performance/profile-top-self.tsv).

### 5.4 Call-graph context

Inclusive call-graph counts (not additive; functions nest):

- `StreamEncoder::step_frame`: 6,510 / 6,530 worker samples (99.7%).
- `scf::estimate_channel_with_inv_ws`: **46.4%** inclusive on the corpus,
  **53.8%** on music, **44.1%** on speech, **49.7%** on noise, **13.5%** on
  onset.
- `StreamEncoder::frontend`: **23.4%** inclusive on the corpus, **50.5%** on
  onset, **16.2%** on music.
- `psy::process_pair_reuse`: 13.2% corpus, 26.0% onset.
- `qc_main_frame`: 8.7% corpus. This is **not** a pure rate-control measure:
  it includes quantization and bit counting inside the QC call. Rate-control
  bookkeeping itself remains small (2.1% self), consistent with the VBR-5
  single-pass audit.
- `dyn_bit_count_reuse`: 6.8% inclusive; `channel_element_write`: 4.2%;
  `quantize_spectrum`: 4.2%; `code_values`: 3.4%.

### 5.5 What the profile establishes and what it does not

**Establishes:** the S4 scalefactor/quantization search is the largest
single-stage cost on music, speech, and noise; the front-end is the largest
stage on transient-heavy content; the bit path and memory/allocator costs are
secondary; rate-control bookkeeping is not a dominant cost in this VBR-5 path.

**Does not establish:** source-line attribution, exact per-call counts,
exclusive vs shared cache effects, or a causal speedup from any proposed
change. `sample` cannot distinguish, for example, how much of `calc_sfb_dist`
is branch stalls versus table-load latency. The disassembly shows the loop
shape and bounds checks, not a cycle-level breakdown. Any optimization must be
validated by a new benchmark run, not by the profile alone.

## 6. Ranked optimization shortlist

Ranking uses measured impact, plausible speedup, complexity, correctness risk,
and ease of isolation. No candidate was implemented at baseline time;
Candidate 1 was subsequently implemented and measured in §11.

### 6.1 Candidate 1 — remove per-line bounds/panic overhead in `quant::calc_sfb_dist`

- **Measured bottleneck:** `calc_sfb_dist` is 24.4% of aggregate self time,
  29.0% on music, 30.2% on noise, 23.1% on speech. The ARM64 excerpt is a
  scalar loop with per-line `quant[i]` bounds checks and four
  `panic_bounds_check` paths; it contains no NEON arithmetic.
- **Minimal experiment:** assert `spec.len() == quant.len()` once per call,
  then iterate `spec.iter().zip(quant.iter_mut())`. For table indices that are
  already masked to the table length, express the mask so LLVM can prove the
  bound (or use a narrowly scoped `get_unchecked` with a `debug_assert`).
  Do not change arithmetic, accumulation order, early-out, or output writes.
- **Why it should help:** removes a branch and a panic path per line, reduces
  register pressure, and gives LLVM a loop shape it can schedule or
  auto-vectorise. It cannot change results if the index proofs are correct.
- **Correctness tests:** `quant` probe/unit tests; `scf` corpus tests;
  `production_baseline` exact bytes/hash; full suite; benchmark hash gate.
- **Success:** median production throughput improves **≥1%** in at least two
  of three independent runs, with all 19 baseline hashes and the derived long
  hash unchanged.
- **Reject:** any hash change, any panic on a valid vector, <1% median gain, or
  a code-size/regression penalty that outweighs the gain.
- **Complexity/risk/isolation:** low / low / high.

### 6.2 Candidate 2 — fuse the final band energy/distortion pass

- **Measured bottleneck:** `calc_sfb_quant_energy_and_dist` 3.2% and
  `quantize_spectrum` 4.2% aggregate; these re-quantize/inverse-quantize bands
  after the scalefactor search. `calc_sfb_dist` already performs the same
  per-line inverse quantization and distortion accumulation during the search.
- **Minimal experiment:** for the winning candidate only, accumulate
  `fpow2(inv)` energy alongside distortion in the same loop (or return it from
  a variant used only after the winner is known), then consume it in
  `calc_sfb_quant_energy_and_dist` instead of a second full pass. Preserve the
  exact `fpow2`/`accumulate_dist` order, the `MAX_QUANT` early-out, and the Ld
  conversion.
- **Why it should help:** removes one per-line pass per band; upper bound is
  roughly the 3–4% currently spent in the final energy/dist pass, minus any
  extra work for losing candidates.
- **Correctness tests:** S4/S5 vectors; `production_baseline`; full suite.
- **Success:** ≥1% median gain with identical hashes; reject on any hash or
  quantized-line change.
- **Complexity/risk/isolation:** medium / medium / medium.

### 6.3 Candidate 3 — reuse code words/lengths between bit counting and encoding

- **Measured bottleneck:** bit counting + Huffman encoding is 10.3% aggregate,
  11.0% music; `code_values` 3.4%, `dyn_bit_count_reuse` 3.3%, `bit_count`
  2.7%.
- **Minimal experiment:** add a test-only census of per-section quad counts to
  confirm overlap between the count and encode traversals. If they traverse the
  same quantized quads, produce a scratch array of `(code word, length)` during
  counting and consume it in `code_values`, or fuse count+write for the
  single-pass VBR path. Preserve bit order and `write_bits` semantics.
- **Why it should help:** removes one table-lookup/traversal per quad; plausible
  1–3% overall.
- **Correctness tests:** S5/S7/S8 bit-exact vectors, `frame` hex tests,
  `production_baseline`, full suite.
- **Success:** ≥1% median gain with byte-identical output; reject on any
  bitstream change.
- **Complexity/risk/isolation:** medium-high / medium-high / medium.

### 6.4 Candidate 4 — reduce per-frame allocation/copy overhead

- **Measured bottleneck:** memory/allocator 7.4% aggregate, 13.4% onset;
  `PcmFeeder` 1.6% aggregate / 4.5% onset; `Encoder::emit_frames` clones each
  channel into a fresh `Vec<i16>` per frame, and the feeder drains per-channel
  `Vec`s one frame at a time.
- **Minimal experiment:** reuse the M5.19/M5.20 allocation-counting approach
  (or a benchmark-only counting allocator) to identify the per-frame
  allocations. Then replace the `to_vec` clones with reusable per-channel
  scratch buffers, or pass `&[i16; 1024]` slices into an internal
  borrow-based `step_frame` path while keeping the public API unchanged.
- **Why it should help:** reduces allocator/copy work, especially on short or
  transient streams. The prior campaign found allocation reductions without
  runtime wins, so the timing gate must be strict.
- **Correctness tests:** full suite; `production_baseline`; chunking
  equivalence tests.
- **Success:** ≥1% median gain with identical bytes; reject if no median gain
  even when allocation counts fall.
- **Complexity/risk/isolation:** medium / low-medium / medium.

### 6.5 Candidate 5 — specialise the transient front-end (TNS/block switch/FFT)

- **Measured bottleneck:** onset front-end 51.8%; FFT 6.9%, block switch 6.7%,
  TNS `scale_up_spectrum` 6.4%, `psy::process_pair_reuse` 5.5%, TNS
  autocorrelation 3.8%. Corpus front-end 25.8%.
- **Minimal experiment:** isolate the short-window path with the onset fixture;
  fuse the TNS autocorrelation/scale-up loops, or avoid redundant spectrum
  scaling where the short-window order is already known. Preserve exact
  integer order and TNS activation decisions.
- **Why it should help:** 1–3% corpus, potentially ~5% on transient-heavy
  content; workload-specific.
- **Correctness tests:** TNS D-012/D-013 vectors, block-switch oracle,
  `production_baseline`, full suite.
- **Success:** ≥1% corpus or ≥3% onset median gain with identical hashes;
  reject on any TNS/block decision or hash change.
- **Complexity/risk/isolation:** medium-high / medium-high / medium.

### 6.6 Not shortlisted

- **LTO / `target-cpu=native` / build-flag tuning.** These may move the
  numbers but change the deployment/portability contract and are not a code
  fix. They were not evaluated and are not recommended as the first step.
- **Precision changes (`f32`→`f64` or reduced precision).** Prohibited by the
  correctness guardrails and already investigated elsewhere; no performance
  case is made here.
- **Algorithmic rate-control changes.** VBR-5 rate control is single-pass and
  only 2.1% self time; there is no measured rate-control bottleneck to fix.

## 7. Recommended first optimization experiment

**Status: executed 2026-10-27; retained (§11).** The pre-registered plan below
was followed; results and disposition are in §11.

**Run Candidate 1 exactly, with a pre-registered gate.**

1. Record the current benchmark: four runs × 15 reps, all 19 hashes and the
   long-workload hash (already available in
   `docs/evidence/aac-enc-performance/`).
2. Change only the line loop in `quant.rs`:
   - one `assert_eq!(spec.len(), quant.len())` per call;
   - `spec.iter().zip(quant.iter_mut())` instead of index-based writes;
   - prove/express the existing table masks so the per-line table bounds checks
     disappear;
   - do not touch arithmetic, rounding, accumulation order, or the
     `MAX_QUANT` early-out.
3. Run `production_baseline` in release, the full release suite with artifacts,
   and the benchmark four times.
4. Accept only if all 19 baseline hashes and the long hash are unchanged and
   the median production total improves by **≥1%** in at least two of three
   independent comparisons. Otherwise revert.
5. If accepted, re-profile to confirm the `calc_sfb_dist` share fell; then
   consider Candidate 2.

The expected gain is modest: the function is 24% of self time, but removing
branches is unlikely to halve it. A **1–4% overall** improvement is the honest
planning range. A result below 1% is a reject, not a reason to widen the change
or relax the hash gate.

## 8. Correctness validation

### 8.1 Benchmark-time validation

Every production and core run asserted:

- ADTS sync and self-consistent frame lengths;
- the frame law `ceil((samples + 1600) / 1024)`;
- the committed `production_baseline` byte total and FNV-1a ADTS hash for all
  19 baseline fixtures (exact hash on macOS aarch64);
- the same committed bytes/hash in both the production and core paths;
- deterministic bytes/hash for the derived long workload.

Across four runs × 15 measured repetitions, **2,280 baseline hash comparisons**
(19 fixtures × 15 reps × 2 byte-producing modes × 4 runs) passed, and the
derived long workload was deterministic. No output byte, frame count, or frame
length changed. The separate non-`cfg(test)` example also matched the same four
10 s byte totals/hashes and the long-workload hash.

### 8.2 Test runs

| Command | Result |
|---|---|
| `cargo test --release --locked --features aac_enc --lib aac_enc::production_baseline::production_f32_corpus_baseline_is_unchanged` | **1 passed, 0 failed** |
| Full release suite with reference artifacts | **336 passed, 0 failed, 5 ignored** (4 debug-only ignores + the opt-in benchmark) |
| Full debug suite with reference artifacts | **340 passed, 0 failed, 1 ignored** (the benchmark) |
| Full release suite without artifacts | **326 passed, 10 failed, 5 ignored**; the 10 are the documented artifact-dependent set |
| Full debug suite without artifacts | **330 passed, 10 failed, 1 ignored**; same documented set |
| `cargo test --locked --features aac_enc --doc` | **19 passed, 0 failed** |
| `rust/tools/check-licensing.sh` | pass; 67 `rust/src` files carry the notice |
| `rustfmt --edition 2021 --check src/aac_enc/production_baseline.rs` | clean |
| `rustfmt --edition 2021 --check src/aac_enc/stream_driver.rs` | only the two pre-existing diffs at lines 5427 and 6062; the new `perf_tests` module is clean |

The artifact-dependent failures are not counted as self-contained passes.
`m55_streams_match_c_reference` remains fallback-dependent; it was not used as
a performance correctness claim beyond its existing role.

## 9. Reproduction and remaining uncertainty

### 9.1 Reproduction

From `rust/`:

```sh
# Production benchmark: 19 fixtures + derived 40 s music, 15 reps.
AAC_PERF_BENCH=1 AAC_PERF_REPS=15 AAC_PERF_LONG=1 \
  cargo test --release --locked --features aac_enc --lib \
  aac_enc::stream_driver::perf_tests::aac_perf_bench -- --ignored --nocapture

# Single-fixture profiling loop (300 reps; run sample against the test PID).
AAC_PERF_BENCH=1 AAC_PERF_FIXTURES=music-stereo AAC_PERF_REPS=300 \
  AAC_PERF_MODES=production AAC_PERF_QUIET=1 \
  cargo test --release --locked --features aac_enc --lib \
  aac_enc::stream_driver::perf_tests::aac_perf_bench -- --ignored --nocapture

# Production-build check on the four committed WAVs + derived 40 s music
# (no cfg(test) harness; 15 reps per fixture).
cargo run --release --locked --features aac_enc --example aac_bench_wav
```

The benchmark is `#[ignore]`d and additionally requires `AAC_PERF_BENCH=1`, so
normal test suites never pay for it. Fixture selection:
`AAC_PERF_FIXTURES=comma,separated,short,names`; modes:
`AAC_PERF_MODES=production,feed,core`; repetitions: `AAC_PERF_REPS=N`;
derived long workload: `AAC_PERF_LONG=1`; quiet per-rep output:
`AAC_PERF_QUIET=1`.

### 9.2 Remaining uncertainty

- **Single machine/build.** All numbers are for Apple M5, rustc 1.99.1,
  generic aarch64 release. No cross-machine, cross-compiler, or
  `target-cpu=native` comparison is claimed.
- **Scheduler variance.** No core pinning; P/E-core placement and background
  load dominate the ~1.5% aggregate run-to-run spread. The 0.5 s fixtures show
  up to 3.6% spread; their medians are less precise than the 10 s/40 s values.
- **Test-build bias.** The full 19-fixture benchmark runs in a `cfg(test)`
  build. The production example measures a 0.25–0.86% advantage on the four
  committed WAVs, so the 19-fixture aggregate is conservative by roughly that
  fraction. No production-build number is claimed for the 15 synthetic
  fixtures because their PCM is test-gated.
- **Function-level only.** `sample` has no source lines here; inlining can
  merge small functions into callers. The stage categories are a mapping of
  symbol names, not a compiler-authored attribution.
- **Benchmark boundary.** Output is collected in memory; `aac-convert`'s M4A
  muxing, disk I/O, and FLAC decode are excluded. Full conversion throughput
  will be lower and may have a different bottleneck.
- **Derived workload.** The 40 s workload is repeated committed music, not a
  new signal. It confirms steady-state throughput but does not represent other
  content.
- **No C comparison.** No equivalent C benchmark was run under identical
  conditions; no absolute C-vs-Rust speed ratio is asserted.
- **Optimization scope.** Candidate 1 is implemented and measured in §11;
  Candidates 2–5 remain untested profile hypotheses. No speedup is claimed
  for them.
- **SIMD configuration discrepancy.** The worktree has no explicit SIMD
  feature or target-cpu override despite the project context mentioning SIMD.
  This is recorded as an inspection result; it is not proof that a SIMD build
  was intended or that adding one would help.
- **No perceptual claim.** Throughput, hashes, frame sizes, and SNR are not
  perceptual evidence. No listening or ABX test was run.

## 10. Files added or modified

| File | Change |
|---|---|
| `rust/src/aac_enc/stream_driver.rs` | Added the test-gated `perf_tests` benchmark module (`#[cfg(test)]`, `#[ignore]`). No production path changed. |
| `rust/src/aac_enc/production_baseline.rs` | `const BASELINE` → `pub(crate) const BASELINE` so the benchmark can reuse the committed byte/hash pins. Test-only module; no behavior change. |
| `rust/examples/aac_bench_wav.rs` | Standalone release benchmark example for the four committed WAVs + derived 40 s workload; no `cfg(test)` harness. |
| `rust/src/aac_enc/quant.rs` | §11 follow-up: paired-slice assertion, `zip` line loop, equivalent 9-bit table masks. |
| `docs/aac-enc-performance-baseline.md` | This report. |
| `docs/evidence/aac-enc-performance/README.md` | Method, field definitions, reproduction, limitations. |
| `docs/evidence/aac-enc-performance/benchmark-medians.tsv` | Final-run per-fixture medians. |
| `docs/evidence/aac-enc-performance/benchmark-runs.tsv` | Four independent production run aggregates. |
| `docs/evidence/aac-enc-performance/benchmark-final-modes.tsv` | Final-run production/feed/core decomposition. |
| `docs/evidence/aac-enc-performance/benchmark-19-direct.log` | Direct 15-rep 19-fixture aggregate run (raw). |
| `docs/evidence/aac-enc-performance/benchmark-wav-production.tsv` | Production-example medians on the four committed WAVs + derived long workload. |
| `docs/evidence/aac-enc-performance/wav-bench-1.log`, `wav-bench-2.log` | Raw production-example runs. |
| `docs/evidence/aac-enc-performance/benchmark-fixture-spread.tsv` | Per-fixture four-run spread. |
| `docs/evidence/aac-enc-performance/benchmark-raw.log` | Raw final-run benchmark rows. |
| `docs/evidence/aac-enc-performance/profile-totals.tsv` | Profiler sample totals. |
| `docs/evidence/aac-enc-performance/profile-categories.tsv` | Self-time category shares. |
| `docs/evidence/aac-enc-performance/profile-top-self.tsv` | Top self-time symbols per workload. |
| `docs/evidence/aac-enc-performance/{corpus,music,speech,onset,noise}.sample.txt` | Raw `sample` output. |
| `docs/evidence/aac-enc-performance/calc-sfb-dist-disassembly.txt` | ARM64 excerpt for the hot loop. |
| `docs/evidence/aac-enc-performance/build-flags.txt` | Host/toolchain/profile facts. |
| `docs/README.md` | Index entry for this audit. |

Sections 1–10 describe the pre-experiment state. Section 11 records the
retained follow-up change; no public behavior, expected output, reference
artifact, or build profile was changed. No C oracle was built and no reference
fixture was regenerated.

## 11. 2026-10-27 follow-up — `calc_sfb_dist` bounds-check elimination

### 11.1 Change made

Production change: `rust/src/aac_enc/quant.rs` only.

1. `calc_sfb_dist` now asserts the paired-slice contract once per call:
   `spec.len() == quant.len()`. All in-tree callers pass equal band slices.
2. The line loop uses `spec.iter().zip(quant.iter_mut())` instead of
   `spec.iter().enumerate()` with `quant[i]`.
3. In `quantize_line_body` (both sign branches) and `inv_quantize_line`, the
   existing mantissa-table mask `& !512` is written as `& 0x1FF`.

No arithmetic, accumulation order, early-out position, table value, table
selection, output write, or float behavior changed. No unsafe code,
unchecked indexing, intrinsics, architecture-specific code, or dependency was
added.

**Why the mask is equivalent.** `quantize_line_body` normalizes a nonzero
magnitude so its leading one is at bit 30, leaving `accu >> 21` in
`0x200..=0x3FF`; `i32::MIN` wraps to zero and also maps to index 0.
`inv_quantize_line` does the same after `count_leading_bits` and its shift.
For every reachable value, `(x & !512)` and `(x & 0x1FF)` are therefore the
same index, and both are in `0..512`. The mask now proves the table bound to
LLVM instead of leaving a release panic path. The existing
`debug_assert!(tab_index < 512)` remains.

**Contract note.** The equality assertion makes the existing paired-slice
contract explicit. A longer `quant` slice was previously accepted and would
now be rejected; no in-tree caller does this. A shorter slice already panicked
at the first out-of-range write. This is not a public API change: `quant` is a
crate-internal module.

### 11.2 Optimized assembly

Disassembly of the same `calc_sfb_dist` symbol from the pre-change and
post-change release test binaries (same compiler, target, and profile):

| Metric | Before | After |
|---|---:|---:|
| Function bytes | 0x350 | 0x310 |
| Instructions | 212 | 196 |
| `panic_bounds_check` call sites | 4 | **0** |
| Conditional branches | 10 | 5 |
| `cmp xN, xN` bounds-style compares | 3 | 1 |
| `smull` | 3 | 3 |
| `clz` | 4 | 4 |
| `panic_fmt` call sites | 1 | 1 |
| `assert_failed` call sites | 0 | 1 |

The four removed bounds/panic paths are the per-line `quant[i]` write check
and the three 512-entry table-index checks. The remaining `panic_fmt` is the
pre-existing `specExp < 14` assertion in `inv_quantize_line`; the new
`assert_failed` is the once-per-call length assertion, outside the line loop.
Raw before/after disassembly is in
[`calc-sfb-dist-2026-10-27/assembly-before.txt`](evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/assembly-before.txt)
and
[`assembly-after.txt`](evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/assembly-after.txt).

### 11.3 Benchmark method and results

The test-build A/B used the established release benchmark over all 19 fixtures
plus the derived 40 s workload; the production-example A/B used the four
committed WAVs plus the derived long workload. Both used 15 measured
repetitions per fixture, one warm-up pass, and the same 8,000-frame chunking.
The old and new binaries were run alternately (old→new, new→old, old→new,
new→old) to control machine drift. Correctness checks and hashing stayed
outside the timed region.

**Test-build A/B (20 workloads, 91.45 s audio):**

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | before → after | 478.558 ms | 464.406 ms | **2.96%** |
| 2 | after → before | 483.770 ms | 461.358 ms | **4.63%** |
| 3 | before → after | 484.322 ms | 477.319 ms | **1.45%** |
| 4 | after → before | 484.044 ms | 471.057 ms | **2.68%** |
| **Median** | | | | **2.82%** |

All four pairs exceed the 1% gate. The `core` mode (feeder excluded) improved
1.73–5.26% (median 2.83%), confirming the gain is in encoder work rather than
the feeder. Feed-only time remained ~8 ms. A separate non-interleaved
before/after set gave 0.56%, 1.49%, and 2.38% (median 1.49%); the paired
alternating result is the primary estimate because it controls the drift that
made the later pre-change runs slower.

**Non-`cfg(test)` production example (four committed WAVs + 40 s derived,
sum of five fixture medians):**

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | before → after | 445.199 ms | 435.730 ms | **2.13%** |
| 2 | after → before | 448.467 ms | 434.163 ms | **3.19%** |
| 3 | before → after | 445.311 ms | 432.681 ms | **2.84%** |
| 4 | after → before | 449.835 ms | 433.559 ms | **3.62%** |
| **Median** | | | | **3.01%** |

Per-fixture production-example medians:

| Fixture | Before | After | Improvement | Hash |
|---|---:|---:|---:|---|
| `m7-music-stereo-48k` | 62.008 ms | 60.108 ms | 3.06% | unchanged |
| `m7-speech-clean-stereo-48k` | 39.950 ms | 38.920 ms | 2.58% | unchanged |
| `m7-speech-pauses-stereo-48k` | 39.690 ms | 38.686 ms | 2.53% | unchanged |
| `m8-music-eroica-stereo-48k` | 57.864 ms | 56.060 ms | 3.12% | unchanged |
| `m7-music-stereo-48k-x4` | 247.436 ms | 240.066 ms | 2.98% | unchanged |

**Profile attribution.** A matching 50-rep/8 s corpus sample reduced
`calc_sfb_dist`'s self-time share from **24.43%** to **23.38%** of worker
samples (quant category 31.79% → 30.97%). The shift is directionally
consistent with the end-to-end result but is within sampling noise; the
benchmark is the primary evidence.

### 11.4 Correctness and hashes

| Check | Result |
|---|---|
| `aac_enc::quant::tests` (release) | **10 passed, 0 failed** |
| `production_f32_corpus_baseline_is_unchanged` (release) | **1 passed** |
| Full release suite with reference artifacts | **336 passed, 0 failed, 5 ignored** |
| Full debug suite with reference artifacts | **340 passed, 0 failed, 1 ignored** |
| Full release suite without artifacts | **326 passed, 10 documented failures, 5 ignored** |
| Full debug suite without artifacts | **330 passed, 10 documented failures, 1 ignored** |
| Doctests | **19 passed, 0 failed** |
| Licensing check | pass; 67 `rust/src` files carry the notice |
| Fixture hashes | all 19 baseline fixtures plus the derived long workload unchanged |
| Production-example hashes | four committed WAVs plus long unchanged |

No expected output or test expectation was changed. The 10 artifact-dependent
failures are the same documented set as before this experiment. `rustfmt`
reports only a pre-existing test-formatting difference in `quant.rs` (line 715
after the added lines; the same diff exists at line 701 in the pre-change
copy) and the two pre-existing `stream_driver.rs` differences.

### 11.5 Disposition

**Retained.** The paired test-build median improvement is **2.82%** and the
paired non-`cfg(test)` production-example median is **3.01%**; all four pairs
in each arm exceed the 1% gate, all fixture and long-workload hashes are
unchanged, and the relevant/full suites pass. The assembly confirms the
intended checks were removed rather than merely rearranged.

Limitations: one machine/build; paired A/B controls drift but absolute ms
values are not comparable to the earlier baseline session; the profile share
shift is directional; and the production example covers the four committed
WAVs, while the synthetic fixtures remain test-build only.

### 11.6 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/optimization-testbuild-ab.tsv` | Four paired test-build comparisons. |
| `docs/evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/optimization-production-ab.tsv` | Four paired production-example comparisons. |
| `docs/evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/optimization-production-fixture.tsv` | Per-fixture production medians and hashes. |
| `docs/evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/optimization-assembly.tsv` | Assembly before/after counts. |
| `docs/evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/optimization-profile-share.tsv` | Profile share before/after. |
| `docs/evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/source-diff.txt` | Exact source diff. |
| `docs/evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/*.log` | Raw paired benchmark/example logs. |
| `docs/evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/assembly-{before,after}.txt` | Raw ARM64 disassembly. |
| `docs/evidence/aac-enc-performance/calc-sfb-dist-2026-10-27/profile-{before,after}-corpus.sample.txt` | Raw `sample` output. |

Only `rust/src/aac_enc/quant.rs` changed in production code. **No staging,
commit, push, amend, reset, rebase, cherry-pick, or history rewrite
occurred.**

## 12. 2026-10-27 front-end profiling pass and recommendation

**Status: diagnostic only.** No front-end production optimization was
implemented. The retained `calc_sfb_dist` change remains in place.

### 12.1 Current baseline and environment

This pass uses the post-`calc_sfb_dist` release binary:

- rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- Apple M5, arm64, macOS 27.0.1, 10 logical CPUs, 16 GiB RAM
- release profile with default `aac_enc` feature
- test binary SHA-256
  `c58116db0d877e23a42f7a2246202bd572fa735f75ffc912b3e3c4a7f170bfca`

Three independent 15-rep baseline runs over all 19 fixtures plus the derived
40 s workload:

| Run | Production total | Realtime × | RTF |
|---|---:|---:|---:|
| 1 | 452.411 ms | 202.1 | 0.004947 |
| 2 | 460.548 ms | 198.6 | 0.005036 |
| 3 | 470.381 ms | 194.4 | 0.005144 |
| **Median** | **460.548 ms** | **198.6** | **0.005036** |

A separate direct 19-fixture aggregate (15 reps, no derived workload) was
**220.353 ms / 233.5×** (RTF 0.004283). The committed `production_baseline`
test passed and every benchmark run validated all 19 fixture hashes plus the
derived long hash. The original pre-optimization baseline in §4 remains the
reference for the earlier session; this is the current post-optimization
baseline.

### 12.2 Profiling method

`sample` was used at a 1 ms interval against the release test binary, with the
same quiet benchmark loop used in §5. No production instrumentation or
per-frame diagnostics were added.

| Profile | Workload | Reps | Window | Worker samples | Front-end self |
|---|---|---:|---:|---:|---:|
| corpus | all 19 fixtures | 80 | 10 s | 8,231 | **26.70%** |
| onset | `m2-onset-stereo-48k` | 6,000 | 8 s | 6,310 | **51.25%** |
| music | `m7-music-stereo-48k` | 300 | 8 s | 6,606 | **21.50%** |
| speech | `m7-speech-clean-stereo-48k` | 300 | 8 s | 6,561 | **29.57%** |

Self-time shares are normalised to worker samples after excluding the idle
main thread, test harness, and sampling overhead. Cumulative values come from
the `sample` call graph; they are not additive because functions nest.

### 12.3 Measured front-end hotspots

Self time, as percentage of worker samples:

| Symbol | Corpus | Onset | Music | Speech | Cost character |
|---|---:|---:|---:|---:|---|
| `tns::autocorr_quarter_lags` | **3.39%** | 4.06% | 2.59% | **3.90%** | integer multiply-add loop |
| `pe::form_factor` | **3.18%** | 4.06% | 2.48% | 2.91% | `sqrt_fixp`/table arithmetic |
| `BlockSwitch::process` | **2.93%** | **8.02%** | 2.04% | 3.44% | f32 IIR/energy arithmetic |
| `fft::dit_fft` | 2.49% | **6.66%** | 2.13% | 3.26% | complex butterfly arithmetic |
| `tns::scale_up_spectrum` | 2.48% | **5.50%** | 1.97% | 2.64% | peak scan + shift/copy |
| `intensity_stereo` | 2.41% | 2.93% | 2.04% | 2.88% | sqrt/table-based stereo analysis |
| `psy::process_pair_reuse` | 2.38% | 5.28% | 1.92% | 2.88% | mixed analysis/orchestration |
| `dctiv` | 0.96% | 2.23% | 0.65% | 0.75% | transform arithmetic |
| `energy::map_spectrum_to_q31` | 0.70% | 1.77% | 0.41% | 0.96% | vectorised map/shift |
| `Mdct::mdct` | 0.36% | 1.28% | 0.32% | 0.50% | transform arithmetic |
| `tns::merged_autocorr_fixed` | 0.39% | 0.81% | 0.26% | 0.59% | orchestration around autocorr |

The TNS **stage** (autocorr + scale-up + merge/analyze/detect) is the largest
coherent front-end stage at **6.66% corpus / 10.92% onset**. The transform
components (MDCT/DCT/FFT) total about **3.91% corpus / 10.38% onset**, and
block switching is **2.93% corpus / 8.02% onset**. `pe::form_factor` is the
largest single non-TNS symbol at 3.18% corpus.

Selected inclusive call-graph counts:

| Stage | Corpus | Onset | Music | Speech |
|---|---:|---:|---:|---:|
| `StreamEncoder::frontend` | 22.21% | 50.36% | 16.95% | 25.56% |
| `psy::process_pair_reuse` | 12.22% | 24.29% | 9.60% | 14.07% |
| `tns::detect` | 6.89% | 11.44% | 5.24% | 7.80% |
| `transform::process` | 4.30% | 11.17% | 3.41% | 4.95% |
| `Mdct::mdct` | 3.84% | 10.17% | 3.10% | 4.51% |
| `dctiv` | 3.46% | 8.87% | 2.80% | 4.01% |
| `BlockSwitch::process` | 2.93% | 8.02% | 2.04% | 3.44% |
| `pe::form_factor` | 3.51% | 4.52% | 2.71% | 3.16% |
| `intensity_stereo` | 2.83% | 3.55% | 2.29% | 3.26% |

`StreamEncoder::frontend` inclusive (22.21%) is lower than the 26.70%
self-category total because some front-end work is attributed to call sites
outside that symbol or inlined into other frames. The two views are not
summable and should not be compared as if they were the same denominator.

### 12.4 Source and generated-code inspection

Assembly counts for the inspected symbols (same release binary):

| Symbol | Instr | `panic_bounds` | Branches | Relevant finding |
|---|---:|---:|---:|---|
| `tns::scale_up_spectrum` | 188 | 0 | 10 | peak scan vectorised; write loop scalar |
| `tns::autocorr_quarter_lags` | 303 | 16 | 39 | unrolled, 12 register accumulators; panic paths at tail |
| `BlockSwitch::process` | 562 | 1 | 19 | 90 `fmul`; f32 IIR/energy; one check |
| `fft::dit_fft` | 722 | 20 | 46 | all 20 `panic_bounds` sites at function tail |
| `pe::form_factor` | 416 | 1 | 29 | `sqrt_fixp` inlined; 8 `smull`, 2 `clz` |
| `psy::process_pair_reuse` | 4,944 | 63 | 392 | large mixed function; checks not localized |
| `dctiv` | 249 | 7 | 12 | 26 `fmul`; transform arithmetic |
| `intensity_stereo` | 2,524 | 2 | 134 | 95 `smull`, 18 `clz`; sqrt/table work |

The bounds-check counts were checked against the disassembly rather than
source appearance. The `autocorr_quarter_lags` and `dit_fft` panic paths are
at the end of their functions (tail writes/index copies), not in the hot
inner loops. A bounds-check cleanup in either would not be supported by this
codegen evidence.

**The one concrete source-level issue is `tns::scale_up_spectrum`.** The peak
scan is vectorised (`abs.4s`, `ushll.2d`, `cmhi.2d`), but the write loop:

```rust
for (d, s) in dest.iter_mut().zip(src.iter()).take(stop).skip(start) {
    *d = (*s as i64) << scale;
}
```

compiles to a scalar `ldrsw`/`lsl`/`str` loop. A direct-slice equivalent:

```rust
for (d, s) in dest[start..stop].iter_mut().zip(&src[start..stop]) {
    *d = (*s as i64) << scale;
}
```

In the isolated direct-slice build it compiles to vector shifts
(`sshll.2d`/`ushl.2d`). An isolated function-shape microbenchmark measured
four 256-element quarters at **1697.5 ns/frame**
(adapter form) versus **216.4 ns/frame** (direct slices), about **7.8×**; the
write-only loop measured **1636.8 ns** versus **67.3 ns**. These are isolated
loop measurements, not end-to-end encoder results. The production function is
2.48% of corpus self time and 5.50% of onset, so a 1% end-to-end gate is
plausible but must be measured.

### 12.5 Recommended experiment

**Status: executed 2026-10-28 and retained (§13).**

**`tns::scale_up_spectrum` write-loop slicing — single narrow change.**

1. **Measured hotspot:** `tns::scale_up_spectrum` 2.48% corpus / 5.50% onset
   / 1.97% music / 2.64% speech; TNS stage 6.66% corpus / 10.92% onset.
2. **Suspected avoidable cost:** the `take(stop).skip(start)` adapter chain
   leaves the shift/copy loop scalar; the peak scan is already vectorised.
3. **Smallest change:** replace only the write-loop iterators with
   `dest[start..stop].iter_mut().zip(&src[start..stop])`; keep the peak scan
   and every arithmetic operation unchanged. Add a debug-only range
   assertion for the existing valid-range contract.
4. **Why semantics are preserved:** same source elements, same destination
   indexes, same write order, same `i64 << scale` operation. All callers pass
   `idx0..idx1`, `idx1..idx2`, `idx2..idx3`, `idx3..idx4` within the 1024-line
   buffer. No arithmetic, state, table, or decision changes. The change
   tightens invalid-range handling from silent adapter truncation to a slice
   panic; that range is outside the private function's contract and no caller
   uses it.
5. **Expected benefit:** with the full-shape microbenchmark at 7.8×, the
   upper bound on this symbol is about 2.1% corpus / 4.8% onset; a realistic
   end-to-end range is **1–2% corpus / 2–4% onset**, with uncertainty from
   cache behavior, four separate quarter calls, and actual branch layout.
6. **Correctness risks:** low but not zero: the write loop is a pure shift
   and copy, so hash equality is the decisive gate. TNS D-012/D-013 tests,
   the TNS oracle corpus, `production_baseline`, and the full suite must pass.
7. **Acceptance gate:** at least **1% median end-to-end improvement in two of
   three independent paired comparisons**, all 19 fixture hashes and the
   derived long hash unchanged, all relevant tests pass, and no meaningful
   regression on music/speech/onset. Reject if the median gain is below 1% or
   any hash changes. Expected verification: paired old/new benchmark plus the
   non-test production example; confirm the scale-up write loop vectorises in
   the new release disassembly.

### 12.6 Hypotheses inspected and not selected

- **`tns::autocorr_quarter_lags` bounds checks.** 16 panic paths, but the
  generated inner loop is already fully unrolled with 12 register
  accumulators, and the panic sites are tail checks. Removing them would not
  address the measured arithmetic loop. A lag-outer rewrite would reread the
  spectrum and is not supported by this profile.
- **`fft::dit_fft` bounds checks.** 20 panic paths, all at the function tail;
  the hot butterfly loops are arithmetic. The FFT is a larger correctness
  surface and the expected end-to-end gain from tail checks is small.
- **`BlockSwitch::process`.** Largest onset self time (8.02%), but only one
  bounds check; the loop is an f32 IIR/energy recurrence. Reordering,
  reassociation, or FMA would change numerical behavior and is prohibited.
- **`pe::form_factor`.** 3.18% corpus / 4.06% onset, but only one bounds
  check; the cost is inlined `sqrt_fixp`/table arithmetic. No exact cheaper
  substitute was identified. A zero-line skip is exact in principle but its
  zero density was not measured, so it remains a lower-confidence follow-up.
- **`psy::process_pair_reuse` allocations/bounds.** 12.22% inclusive and 63
  bounds paths, but the function is 4,944 instructions of mixed orchestration
  and children. Several small `Vec`s escape into returned outputs, so they
  cannot simply move to scratch. No single bounded change is justified from
  this profile.

### 12.7 Correctness and evidence

- `production_baseline` release test: **1 passed**.
- Current baseline benchmark: all 19 fixture hashes and the derived long hash
  validated on every run.
- TNS tests (release): **5 passed**.
- Block-switch tests (release): **8 passed**.
- Psy tests (release): **14 passed**.
- Full release suite with reference artifacts: **336 passed, 0 failed, 5
  ignored**.
- No expected output or test expectation was changed; no production code was
  changed in this pass.

Raw profiles, symbol tables, assembly excerpts, baseline logs, and the
microbenchmark are in
[`front-end-2026-10-27/`](evidence/aac-enc-performance/front-end-2026-10-27/):
`README.md`, `baseline-*.log`, `*.sample.txt`, `frontend-*.tsv`,
`assembly-stats.tsv`, `asm-*.txt`, and the two `scale-up-*microbench*` files.

### 12.8 Limitations

- One machine/build; Apple M5 and generic aarch64 release. No cross-machine
  or `target-cpu=native` comparison.
- Profiles are function-level (`sample`; release strips debuginfo), with no
  source-line attribution or measured call counts. Inlining can merge small
  functions into their callers.
- The 19-fixture and onset profiles are test-build runs; the previously
  measured non-test advantage was 0.25–0.86%, so absolute throughput is
  slightly conservative. The production example does not cover the synthetic
  onset fixture.
- The microbenchmark isolates the loop shape and is not an end-to-end result.
  The 7.8× figure must not be presented as an achieved encoder speedup.
- No C oracle, reference artifact, perceptual test, or listening claim was
  used.
- At the time of the §12 pass, no front-end optimization had been
  implemented; the separate paired experiment in §13 executed and retained
  the `scale_up_spectrum` recommendation.

## 13. 2026-10-28 follow-up — `tns::scale_up_spectrum` write loop

### 13.1 Change made

Production change: `rust/src/aac_enc/tns.rs`, only the write loop of
`scale_up_spectrum`.

```rust
// before
for (d, s) in dest.iter_mut().zip(src.iter()).take(stop).skip(start) {
    *d = (*s as i64) << scale;
}

// after
let end = stop.min(dest.len()).min(src.len());
let start = start.min(end);
for (d, s) in dest[start..end].iter_mut().zip(&src[start..end]) {
    *d = (*s as i64) << scale;
}
```

The peak scan, shift, write order, TNS analysis, tables, thresholds,
coefficients, and state updates are unchanged. No unsafe code, intrinsics,
dependencies, or production instrumentation was added.

**Range equivalence.** The old form processed
`start..min(stop, dest.len(), src.len())`; `skip(start)` after a shorter
iterator produced an empty tail when `start` exceeded that end. The new
clamped range is exactly the same set for every range, including out-of-range
values, so the private function's existing non-panicking behavior is
preserved. All supported callers are in `merged_autocorr_fixed`: `pspec` is
1024 i64; `spectrum_q31` is 1024 i64 for long windows or 128 for short
windows; `start/stop` are the four `start_line[LO]..stop_line` quarters, with
`idx0 <= idx1 <= idx2 <= idx3 <= idx4`. Valid ranges are strictly inside both
slices.

### 13.2 Generated code

| Build | Standalone `scale_up_spectrum` | `merged_autocorr_fixed` | Evidence |
|---|---|---|---|
| Before | 188 instructions; write loop scalar | 0x1164 / 1,113 instructions | scalar `ldrsw`/`lsl`/`str` |
| After | no standalone symbol (inlined) | 0x1f70 / 2,012 instructions | 20 `sshll.2d`, 20 `sshll2.2d`, 56 `ushl.2d`, 16 `abs.4s`, `stp q` stores |

The intended vector codegen is present in the actual production path, not
just the isolated build. LLVM inlined `scale_up_spectrum` into
`merged_autocorr_fixed`; the merged symbol grew from 4,452 to 8,048 bytes.
Adding back the old 752-byte standalone function gives a combined
5,204 → 8,048 byte (+2,844 byte) footprint for this path. This code-size
increase is retained as a caveat; the paired end-to-end result remains
positive.

### 13.3 Paired end-to-end results

Eight alternating old/new pairs, 15 measured repetitions per fixture, all 19
fixtures plus the derived 40 s workload. The old and new release test binaries
share the same compiler, target, profile, and benchmark harness. A standalone
pre-change baseline was **461.126 ms / 198.3×**; the paired arms span roughly
445–487 ms because the machine drifted during the session, so paired ratios
are the valid comparison.

**Test build:**

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 445.217 ms | 450.765 ms | −1.25% |
| 2 | new → old | 455.471 ms | 446.502 ms | +1.97% |
| 3 | old → new | 456.800 ms | 451.879 ms | +1.08% |
| 4 | new → old | 469.850 ms | 462.750 ms | +1.51% |
| 5 | old → new | 486.687 ms | 467.082 ms | +4.03% |
| 6 | new → old | 483.853 ms | 474.567 ms | +1.92% |
| 7 | old → new | 477.970 ms | 463.590 ms | +3.01% |
| 8 | new → old | 471.164 ms | 461.979 ms | +1.95% |
| **Median** | | | | **+1.934%** |

Seven of eight pairs improved; pair 1 is the only negative. The median exceeds
the 1% gate.

**Non-`cfg(test)` production example** (four committed WAVs + 40 s derived
music, sum of per-fixture medians, 15 reps):

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 441.467 ms | 431.631 ms | +2.23% |
| 2 | new → old | 442.328 ms | 434.914 ms | +1.68% |
| 3 | old → new | 440.412 ms | 436.070 ms | +0.99% |
| 4 | new → old | 447.676 ms | 478.328 ms | −6.85%* |
| 5 | old → new | 445.120 ms | 432.548 ms | +2.82% |
| 6 | new → old | 440.816 ms | 429.555 ms | +2.55% |
| 7 | old → new | 436.620 ms | 429.802 ms | +1.56% |
| 8 | new → old | 472.982 ms | 442.576 ms | +6.43% |
| **Median** | | | | **+1.952%** |

\*Pair 4 was a machine-wide slow run: every fixture in
`production-after-4.log` is about 10% slower than the adjacent after runs
(for example music 67.1 vs 60.1/60.2 ms; long 264.4 vs 243.7/239.6 ms),
consistent with a thermal/scheduler event. It is retained in the table and
the median.

Per-fixture production-example medians across all eight runs:

| Fixture | Before | After | Improvement | Hash |
|---|---:|---:|---:|---|
| `m7-music-stereo-48k` | 61.281 ms | 60.121 ms | 1.89% | unchanged |
| `m7-speech-clean-stereo-48k` | 39.493 ms | 38.398 ms | 2.77% | unchanged |
| `m7-speech-pauses-stereo-48k` | 39.310 ms | 38.234 ms | 2.74% | unchanged |
| `m8-music-eroica-stereo-48k` | 56.875 ms | 55.761 ms | 1.96% | unchanged |
| `m7-music-stereo-48k-x4` | 245.131 ms | 240.681 ms | 1.82% | unchanged |

**Profile check.** Before/after `sample` profiles at the same workload
settings:

| Workload | TNS self before | TNS self after | Front-end before | Front-end after |
|---|---:|---:|---:|---:|
| corpus | 6.66% | 4.56% | 26.70% | 25.66% |
| onset | 10.92% | 6.93% | 51.25% | 47.78% |
| music | 5.03% | 3.39% | 21.50% | 19.87% |
| speech | 7.50% | 5.70% | 29.57% | 28.34% |

The TNS share fell by ~2.1 points on the corpus and ~4.0 points on onset;
no new front-end hotspot appeared. `scale_up_spectrum` is no longer a
standalone symbol after inlining, so its cost is measured through
`merged_autocorr_fixed`/the TNS category. Profile shifts are directional and
sampling-noisy; the paired benchmark is primary.

### 13.4 Correctness and hashes

| Check | Result |
|---|---|
| TNS tests (release) | **5 passed** |
| Block-switch tests (release) | **8 passed** |
| Psy tests (release) | **14 passed** |
| Quant tests (release) | **10 passed** |
| Production-baseline test (release) | **1 passed** |
| Full release suite with reference artifacts | **336 passed, 0 failed, 5 ignored** |
| Full debug suite with reference artifacts | **340 passed, 0 failed, 1 ignored** |
| Release suite without artifacts | **326 passed, 10 documented failures, 5 ignored** |
| Doctests | **19 passed** |
| Licensing check | pass; 67 `rust/src` files carry the notice |
| Fixture hashes | all 19 + derived long unchanged |
| Production-example hashes | four WAVs + long unchanged |

No expected output, test expectation, or reference artifact was changed.

### 13.5 Disposition

**Retained.** The paired medians are **1.934%** (test build) and **1.952%**
(production example); seven of eight pairs improved in each arm; all hashes
and correctness checks pass; the intended vector codegen is present in the
production path. The one negative test-build pair and the one thermal-outlier
production pair are recorded rather than hidden.

Limitations: one machine/build; the code-size increase from inlining is a
deployment risk not characterized on other targets; the non-test example
covers only four committed WAVs; the gain is ~1.9%, not the isolated loop
shape's 7.8×; no C comparison or perceptual claim is made.

### 13.6 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/tns-scale-up-spectrum-2026-10-27/source-diff.txt` | Exact source diff. |
| `.../assembly-summary.tsv` | Before/after instruction, bounds, branch, vector, and size counts. |
| `.../asm-scale-up-before.txt`, `.../asm-merged-before.txt`, `.../asm-merged-after.txt` | Raw disassembly. |
| `.../testbuild-before-*.log`, `.../testbuild-after-*.log` | Eight paired test-build runs. |
| `.../production-before-*.log`, `.../production-after-*.log` | Eight paired production-example runs. |
| `.../optimization-testbuild-ab.tsv`, `.../optimization-production-ab.tsv`, `.../optimization-production-fixture.tsv` | Paired summaries. |
| `.../profile-summary.tsv`, `.../{corpus,onset,music,speech}-{before,after}.sample.txt` | Profile comparison. |
| `.../hash-verification.log` | Non-quiet 1-rep run with all 19 fixture hashes and the derived long hash. |
| `.../README.md` | Method, range argument, results, caveats. |

Only `rust/src/aac_enc/tns.rs` changed in production code for the retained
§13 experiment. **No staging, commit, push, amend, reset, rebase,
cherry-pick, or history rewrite occurred.**

## 14. 2026-10-28 bit-path re-profile and rejected `bit_count` experiment

### 14.1 Current baseline and updated profile

After both retained optimizations, three independent 15-rep runs over the 19
fixtures plus the derived 40 s workload measured **436.628, 435.320, and
434.154 ms** (median **435.320 ms / 210.1×**, RTF 0.004760). The
production-baseline test passed and every benchmark run validated all fixture
hashes plus the long workload.

Current `sample` profiles (same post-optimization binary; 1 ms interval):

| Workload | Bit-path self | `code_values` | `dyn_bit_count_reuse` | `bit_count` |
|---|---:|---:|---:|---:|
| corpus | 10.37% | 3.76% | 2.98% | 2.77% |
| onset | 7.98% | 0.91% | 2.11% | **4.12%** |
| music | 11.62% | **5.15%** | 2.87% | 2.37% |
| speech | 9.59% | 3.35% | 2.19% | 3.26% |

The three largest bit-path self symbols are therefore `code_values` (3.76%
corpus / 5.15% music), `dyn_bit_count_reuse` (2.98% corpus / 2.87% music), and
`bit_count` (2.77% corpus / 4.12% onset). No single symbol dominates, and the
bit path is only 7.98–11.62% of self time depending on workload.

### 14.2 Candidate selection and attempted change

`bit_count` was selected for a bounded experiment because it had **38
`panic_bounds_check` sites** in 881 instructions and is the largest bit-path
symbol on onset (4.12%). Source inspection showed that most hot guards are
**Huffman-table range guards**, which are an intentional fail-closed contract;
only the stride-4 value loads can be made bounds-safe without changing the
table/error contract.

Attempted change (reverted; not in the current tree): replace the
`while i < values.len()` / `values[i]` kernels in `count_1_to_11`,
`count_3_to_11`, `count_5_to_11`, `count_7_to_11`, `count_9_to_11`,
`count_11_only`, and `count_esc` with `chunks_exact(4)` / `chunks_exact(2)`,
plus a release width assertion. No Huffman table, codeword, bit-order,
scaling, threshold, or emitted-bit expression changed. The table guards were
retained. Exact patterns:
[`source-change-attempted.txt`](evidence/aac-enc-performance/bit-path-2026-10-28/source-change-attempted.txt).

Generated code confirmed a partial effect: `bit_count` shrank from **881 to
700 instructions**, `panic_bounds_check` from **38 to 19**, and branches from
**57 to 32**; code size fell from **0xdc4 to 0xaf0**. The remaining 19 table
guards are intentional.

### 14.3 Paired end-to-end result — rejected

Four alternating old/new test-build pairs, 15 reps, all 19 fixtures plus the
derived workload:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 426.267 ms | 437.847 ms | **−2.72%** |
| 2 | new → old | 447.935 ms | 443.307 ms | **+1.03%** |
| 3 | old → new | 449.355 ms | 448.621 ms | **+0.16%** |
| 4 | new → old | 450.803 ms | 451.275 ms | **−0.11%** |
| **Median** | | | | **+0.029%** |

Only one of four pairs reached 1%; the median is far below the gate.
Per-workload medians were mixed and mostly below 1% (click +1.15%, edge
+0.91%, stress +0.72%, music +0.41%, speech-clean −0.61%, long −0.01%).
The partial code-size reduction did not translate into end-to-end throughput.

### 14.4 Correctness and disposition

- `aac_enc::bitcount`: **11 passed** during the attempted change.
- Production-baseline: **1 passed**; all hashes unchanged.
- Attempted change passed the relevant bit-counting tests and hash gate.
- After reverting only `bitcount.rs`: full release suite with artifacts
  **336 passed, 0 failed, 5 ignored**; release without artifacts **326
  passed, 10 documented failures, 5 ignored**.
- The retained `quant::calc_sfb_dist` and `tns::scale_up_spectrum` changes
  were not touched.
- No expected output, reference artifact, or test expectation changed.
- The full release suite was run on the reverted tree. The attempted tree
  was covered by the bit-counting, production-baseline, and hash checks but
  not a separate full-suite run, because it was rejected on the performance
  gate and reverted.

**Disposition: rejected and reverted.** `bitcount.rs` is back at HEAD; no
bit-path optimization is retained. The current tree is the two-optimization
state with the 435.320 ms / 210.1× baseline above. This is a conclusive
negative result for this candidate, not a claim that no bit-path work could
ever help.

### 14.5 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/bit-path-2026-10-28/README.md` | Method, profile, attempt, result, limitations. |
| `.../baseline-*.log`, `.../baseline-summary.tsv` | Current baseline. |
| `.../corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt` | Current profiles. |
| `.../bitself.tsv`, `.../bit-assembly-stats.tsv` | Symbol and codegen summaries. |
| `.../asm-bit_count-before.txt`, `.../asm-bit_count-after.txt` | Attempted before/after disassembly. |
| `.../testbuild-{before,after}-*.log` | Four paired benchmarks. |
| `.../optimization-testbuild-ab.tsv`, `.../optimization-workloads.tsv` | Paired/workload summaries. |
| `.../source-change-attempted.txt` | Reverted change description. |
| `.../hash-verification-final.log` | Post-revert hash verification. |

No bit-path production code was retained. **No staging, commit, push, amend,
reset, rebase, cherry-pick, or history rewrite occurred.**

## 15. 2026-10-28 `code_values` / bit-emission optimization (retained)

### 15.1 Profile: the cost is the inlined writer

The `code_values` investigation started from the post-§14 tree (HEAD
`c4d1262`, `bitcount.rs` clean, `quant.rs`/`tns.rs` retained). A fresh
session baseline measured **436.632, 446.684, and 447.323 ms** (median
**446.684 ms / 204.70×**, RTF 0.004884); the session ran slightly slower than
the 435.320 ms §14 figure, so all comparisons below are paired within the
session.

`sample` self time before and after the change (worker-normalised):

| Workload | `code_values` before | Share | After | Share |
|---|---:|---:|---:|---:|
| corpus | 341 | 4.18% | 161 | 2.19% |
| onset | 35 | 0.55% | 33 | 0.58% |
| music | 327 | 4.97% | 185 | 3.13% |
| speech | 187 | 2.87% | 110 | 1.87% |

In the before profile, ~98% of the corpus/music `code_values` samples land in
the **inlined `BitWriter::write_bits` byte loop** in the books 7–10 arm
(`+0x59c` loop head, `+0x5e4` byte merge). Speech is dominated by book 11
(`+0x73c` writer, `+0x748` escape handling); onset is dominated by
`bit_count` and `code_values` is only 0.55%. The call graph shows
`frame::encode_channel_spectral` as the caller; `write_bits` is inlined, so
its emission cost is attributed to `code_values`. The measured cost is
therefore not Huffman-table lookup.

Three candidates were considered:

1. **Selected:** replace the per-byte `write_bits` merge with one 8-byte
   window merge per codeword.
2. Batch two books 7–10 pairs into one `write_bits` call (byte work
   unchanged; estimated well below 1%).
3. Replace the book 11 escape-length loop with `u32::ilog2` (speech-only,
   small aggregate weight).

### 15.2 Selected change

`rust/src/aac_enc/bitenc.rs` only. `write_bits` keeps its validation and
error behavior. A write of at most 32 bits starting at any bit offset touches
at most five bytes, so when `byte_idx + 8 <= buf.len()` the method reads the
8-byte big-endian window, ORs the field at `64 - bit_offset - need`, and
writes the window back; the final <8 buffer bytes keep the original
byte-at-a-time placement. `self.bits` is updated once. `#[inline]` keeps the
method inlined into the `code_values` arms (without it the compiler outlined
the callee and added calls per invocation).

No Huffman table, codeword, code length, escape value, sign handling, bit
order, scalefactor decision, quantization step, or frame-assembly rule
changed; no unsafe code or dependency was added. Exact diff:
[`source-diff.patch`](evidence/aac-enc-performance/code-values-2026-10-28/source-diff.patch).

### 15.3 Generated code

The books 7–10 hot block changes from a 25-instruction per-byte loop
(`ldrb`/`orr`/`strb` plus per-byte bounds checks and a `bits` store) to:

```
and  x20, x15, #0x7
ldr  x21, [x13, x9]      ; 8-byte window
neg/sub/lsl/rev/orr      ; place value, big-endian
str  x7,  [x13, x9]      ; write window back
str  x15, [x4, #0x18]    ; bits += need
```

Static sizes grew slightly because the tail fallback is inlined into every
arm: `code_values` 0xcc0 → 0xe40 (816 → 912 instructions), `write_bits`
0x13c → 0x160. The dynamically executed path shrank; the post-change profile
drop above is the confirmation. An isolated diagnostic (50 M writes of 7–14
bits at random offsets) measured **5.21 → 1.67 ns per write (3.13×)** with an
identical output checksum; that microbenchmark is diagnostic only.

### 15.4 Paired end-to-end result — retained

Eight alternating old/new test-build pairs, 15 reps, all 19 fixtures plus the
derived 40 s workload:

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

All 8 pairs are positive and all 8 are ≥1% (min 1.281%, max 3.155%). A
production-example arm (non-`cfg(test)`, four WAVs + 40 s derived) gave four
positive pairs of **1.790%, 2.216%, 1.914%, 2.813%** (median **2.065%**,
4/4 ≥1%). Per-workload medians improved on 19 of 20 workloads: music
+2.731%, speech-clean +1.706%, speech-pauses +1.922%, Eróica +2.438%, long
+2.551%, noise +3.773%, stress +2.955%. The only negative was the 0.2 ms
`m2-edge-mono-48k` at −0.61%, below the run-to-run variation for that fixture
and not a meaningful workload regression.

### 15.5 Correctness and disposition

- `aac_enc::bitenc` **9 passed**; `bitcount` **11**; `frame` **9**;
  `quant` **10**; `tns` **5**; `block_switch` **8**; `psy` **14**.
- Production-baseline **1 passed**; all 19 fixture byte totals and FNV-1a
  hashes plus the derived long-workload hash (`889c3be4ccd53dd7`) unchanged
  in the 60-row benchmark verification.
- Full release suite with artifacts: **336 passed, 0 failed, 5 ignored**.
  Without artifacts: **326 passed, 10 documented artifact-dependent failures,
  5 ignored**.
- No expected output, reference artifact, or test expectation changed;
  licensing hygiene and `git diff --check` clean.

**Disposition: retained.** This is a bitstream-preserving writer
implementation change, not a Huffman/quantization/rate-control change. It
clears the gate on both arms and the targeted profile share fell as expected.

### 15.6 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/code-values-2026-10-28/README.md` | Method, profiles, candidates, codegen, paired results, limitations. |
| `.../starting-state.txt`, `.../baseline-summary.tsv` | Git state and session baseline. |
| `.../*-before.sample.txt`, `.../*-after.sample.txt` | Before/after profiles. |
| `.../code-values-self*.tsv` | Symbol self-sample summaries. |
| `.../asm-code_values*.txt`, `.../asm-write_bits-*.txt`, `.../codegen-hot-block.txt` | Before/after disassembly. |
| `.../source-diff.patch` | Retained production diff. |
| `.../testbuild-{before,after}-{1..8}.log`, `.../optimization-testbuild-ab.tsv`, `.../optimization-workloads.tsv` | Eight paired test-build runs and summaries. |
| `.../wav-{before,after}-{1..4}.log`, `.../optimization-wav-example-ab.tsv` | Production-example arm. |
| `.../hash-verification-attempted.log`, `.../full-suite-*.log` | Hash and suite verification. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 16. 2026-10-28 scalefactor-search investigation (no retained change)

### 16.1 Profile

Session baseline (post-§15 tree, `scf.rs` clean): **425.978, 428.682, and
432.495 ms**, median **428.682 ms / 213.30×** (RTF 0.004688). `sample` self
time, worker-normalised:

| Workload | `estimate_channel` self | Share | `assimilate_single` self | Share | `calc_sfb_dist` self | Share |
|---|---:|---:|---:|---:|---:|---:|
| corpus | 673 | 8.26% | 268 | 3.29% | 1,985 | 24.35% |
| onset | 279 | 4.39% | 40 | 0.63% | 343 | 5.39% |
| music | 596 | 9.12% | 277 | 4.24% | 1,954 | 29.91% |
| speech | 585 | 8.98% | 159 | 2.44% | 1,390 | 21.34% |

Call-graph inclusive counts (overlapping, not additive) put
`estimate_channel` at ~3,843 corpus / 933 onset / 3,637 music / 2,800
speech. Inside `estimate_channel`, 98% of corpus/music self samples landed on
one store in a 60-element stack-array initialization
(`trial[i] = if i < cnt { scf[i] } else { 0 }`) in
`assimilate_multiple_scf2`'s coarser-candidate loop; speech's hot offset was
the adjacent live min/max scan.

### 16.2 Candidate and equivalence

The candidate hoisted the `trial` array and its baseline copy out of the
`nw` candidate loop; each candidate then only rewrites `start..stop`. This is
exactly equivalent: `trial` differs from `scf` only in the region; accepted
candidates write the same region values to `scf`, rejected candidates leave
`scf` unchanged; entries outside the region are never written and are read
only at indices `< cnt`, so the one-time zero fill beyond `cnt` is never
observed. No arithmetic, ordering, comparison, or tie-breaking changed.
Exact attempted diff:
[`source-change-attempted.patch`](evidence/aac-enc-performance/scalefactor-search-2026-10-28/source-change-attempted.patch).

Generated code removed the 60-store loop from the per-candidate path and
shrank `estimate_channel_with_inv_ws` from 0x3748 to 0x3690 bytes. Re-profiling
showed the sampled cost did not disappear: corpus `estimate_channel` went
from 673 samples (8.26%) to 580 (7.83%), with the hotspot moving to the
adjacent min/max scan (+0x17b8, 575). The loop’s profile share was larger than
the avoidable work.

### 16.3 Paired result — rejected

Four alternating test-build pairs, 15 reps, 19 fixtures + 40 s workload:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 426.014 ms | 428.401 ms | **−0.560%** |
| 2 | new → old | 438.853 ms | 434.782 ms | **+0.928%** |
| 3 | old → new | 441.589 ms | 437.379 ms | **+0.953%** |
| 4 | new → old | 443.153 ms | 441.929 ms | **+0.276%** |
| **Median** | | | | **+0.602%** |

3/4 pairs were positive, but **none reached 1%**; the median is below the
gate. Music (+0.804%), long (+0.818%), speech-clean (+0.957%), and
speech-pauses (+1.160%) moved in the expected direction; short synthetic
fixtures were mixed and noisy (silence −1.775%, onset −1.382%, quiet-sine
−1.237%, edge −1.232%).

### 16.4 Follow-up candidates

The next-hot test was the branchless min/max scan. It is already emitted as
a `csel` loop, and an isolated 49-element benchmark measured **22.38 ns
(current) vs 22.31 ns (branchless, 1.003×)** — no measurable headroom, so it
was not implemented. The same 60-entry copy in `assimilate_multiple_scf`
(per `act`) is smaller and onset-weighted; with the larger step-1 copy
already sub-threshold, it was not selected.

### 16.5 Correctness and disposition

- Attempted tree: `aac_enc::scf` **6 passed**; production-baseline
  **1 passed**; 60-row hash verification unchanged (long hash
  `889c3be4ccd53dd7`). The attempted tree was not full-suite-tested because
  it was rejected before that step.
- Final tree: `scf.rs` reverted to HEAD; test binary SHA restored to
  `f26afee…`; final hash verification unchanged; full release suite with
  artifacts **336 passed, 0 failed, 5 ignored**, without artifacts **326
  passed, 10 documented artifact-dependent failures, 5 ignored**.
- The retained `quant`, `tns`, and `bitenc` changes were not touched.

**Disposition: rejected and reverted. No production change is retained.**
This is a negative result for this candidate, not evidence that scalefactor
search cannot be optimized; it does show that the single-PC profile share of
the 60-element copy overstates the avoidable cost.

### 16.6 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/scalefactor-search-2026-10-28/README.md` | Method, profile, candidate, equivalence, results, limitations. |
| `.../starting-state.txt`, `.../baseline-summary.tsv` | State and session baseline. |
| `.../corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt`, `corpus-after.sample.txt` | Profiles. |
| `.../profile-summary.tsv`, `.../asm-estimate_channel*.txt`, `.../asm-assimilate_single.txt`, `.../codegen-hot-block.txt` | Symbol/codegen evidence. |
| `.../source-change-attempted.patch` | Reverted attempt. |
| `.../scan-microbench.rs`, `.../scan-microbench.txt` | Min/max follow-up diagnostic. |
| `.../testbuild-{before,after}-{1..4}.log`, `.../optimization-testbuild-ab.tsv`, `.../optimization-workloads.tsv` | Paired results. |
| `.../hash-verification-{attempted,final}.log`, `.../full-suite-*.log` | Hash and suite verification. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 17. 2026-10-28 `quantize_spectrum` shared-helper optimization (retained)

### 17.1 Profile

Fresh session baseline (post-§16 tree): **427.305, 425.550, and 428.917 ms**,
median **427.305 ms / 214.0×** (RTF 0.004673). `sample` self time:

| Workload | `quantize_spectrum` | Share | `calc_sfb_dist` | Share |
|---|---:|---:|---:|---:|
| corpus | 375 | 4.62% | 1,997 | 24.62% |
| onset | 97 | 1.53% | 377 | 5.97% |
| music | 309 | 4.73% | 1,956 | 29.92% |
| speech | 264 | 4.02% | 1,422 | 21.66% |

`quantize_spectrum` has no separate callees in the profile: its self time is
the inlined `quantize_line_body`. Hot PCs sat on the zero/positive boundary
(corpus `+0x10c`, 375; onset `+0xf4`, 97), the negative-magnitude path (music
`+0x16c`, 307), and the same boundary on speech (`+0x10c`, 264) — not on the
table lookup.

### 17.2 Candidate and equivalence

The selected change unifies the duplicated positive and negative bodies in
`quantize_line_body`: keep the `accu == 0` early-out, record
`neg = accu < 0`, apply `wrapping_neg` once to get the magnitude, run one
shared clz/table/shift pipeline, and conditionally negate the final `i16`.
For negative input this is exactly the old negate → magnitude → negate
sequence; for positive input it is the old positive body; zero still returns
0 immediately. No constant, table index, shift, comparison, rounding rule, or
traversal order changed. Exact diff:
[`source-diff.patch`](evidence/aac-enc-performance/quantize-spectrum-2026-10-28/source-diff.patch).

The helper is shared with `calc_sfb_dist`, which is why the effect exceeds
`quantize_spectrum`’s own share. Re-profiling the corpus: `quantize_spectrum`
375 (4.62%) → 152 (2.06%), `calc_sfb_dist` 1,997 (24.62%) → 1,637 (22.18%).

### 17.3 Generated code

| Function | Before | After |
|---|---:|---:|
| `quantize_spectrum` | 0x2c0 (704 B), 176 instr, 13 branches | 0x240 (576 B), 144 instr, 11 branches |
| `calc_sfb_dist` | 0x310 (784 B), 196 instr, 5 branches | 0x2c0 (704 B), 176 instr, 4 branches |

The zero case is a `cbz`; the sign magnitude and final result both use
`cneg`; the duplicated negative body is gone. The standalone out-of-line
`quantize_line` symbol also disappeared from the binary.

### 17.4 Paired end-to-end result — retained

Eight alternating test-build pairs, 15 reps, 19 fixtures + 40 s workload:

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

8/8 positive and ≥1% (range 5.595–8.328%). The production-example arm
(non-`cfg(test)`, four WAVs + 40 s derived) gave four positive pairs of
**6.657%, 7.715%, 8.046%, 7.458%** (median **7.586%**). All 20 workloads
improved: music +7.918%, speech +6.637%/+6.623%, Eróica +8.027%, long
+7.954%, noise +8.585%, stress +5.336%, silence +0.172%.

### 17.5 Correctness and disposition

- `aac_enc::quant` **10 passed** (including `qprobe_transcript_exact`,
  `s3_corpus_exact`, distortion gates, inverse-quantize envelope).
  `scf` **6**, `bitcount` **11**, `frame` **9**, `psy` **14**.
- Production-baseline **1 passed**; all 19 fixture hashes and long hash
  `889c3be4ccd53dd7` unchanged in the 60-row verification.
- Full release suite with artifacts: **336 passed, 0 failed, 5 ignored**.
  Without artifacts: **326 passed, 10 documented artifact-dependent failures,
  5 ignored**.
- Licensing hygiene OK; `git diff --check` clean.

**Disposition: retained.** This is a fixed-point-equivalent control-flow
change in one helper; it changes no quantized value, table, or emitted bit.
It clears the gate on both arms and improves every workload.

### 17.6 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/quantize-spectrum-2026-10-28/README.md` | Method, profiles, candidate, equivalence, codegen, results, limitations. |
| `.../starting-state.txt`, `.../baseline-summary.tsv` | State and session baseline. |
| `.../corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt`, `corpus-after.sample.txt` | Profiles. |
| `.../profile-summary.tsv`, `.../profile-corpus-before-after.tsv` | Symbol shares. |
| `.../asm-quantize_spectrum*.txt`, `.../asm-calc_sfb_dist-*.txt`, `.../codegen-hot-block.txt` | Before/after disassembly. |
| `.../source-diff.patch` | Retained production diff. |
| `.../testbuild-{before,after}-{1..8}.log`, `.../optimization-testbuild-ab.tsv`, `.../optimization-workloads.tsv` | Eight paired runs and summaries. |
| `.../wav-{before,after}-{1..4}.log`, `.../optimization-wav-example-ab.tsv` | Production-example arm. |
| `.../hash-verification-{attempted,final}.log`, `.../full-suite-*.log` | Hash and suite verification. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 18. 2026-10-28 `calc_sfb_quant_energy_and_dist` investigation (no retained change)

### 18.1 Profile

Fresh session baseline (post-§17 tree): **387.902, 392.909, and 421.689 ms**,
median **392.909 ms / 232.8×** (RTF 0.004296). The third run was an ~8%
thermal/load outlier; later comparisons are paired.

| Workload | `calc_sfb_quant_energy_and_dist` self | Share | Inclusive |
|---|---:|---:|---:|
| corpus | 319 | 3.89% | 368 |
| onset | 43 | 0.67% | 54 |
| music | 336 | 5.13% | 368 |
| speech | 246 | 3.75% | 272 |

`inv_quantize_line` and `fpow2` are fully inlined. Hot-PC attribution was
~99% at `+0xdc` (corpus 316, music 332, speech 241, onset 42) — a `csel` in
the distortion shift (`shift_int`). The loop is a scalar, one-line-per-
iteration inverse-quantize → `fpow2` → `accumulate_dist` dependency chain;
the hot PC is a stall point, not a bounds check or table lookup.

### 18.2 Candidate and equivalence

The attempted change processed two lines per iteration with independent
`energy`/`distortion` accumulator pairs and summed them at the end. Wrapping
`i32` addition is associative modulo 2^32, so the final energy and distortion
are exactly the sequential value; no multiply, shift, `clz`, `fpow2`, or
distortion operation changed. The `|q| > MAX_QUANT` early-out still returns
`(0, 0)` if either line is out of range; there are no side effects, so
checking both before processing is equivalent. Exact diff:
[`source-change-attempted.patch`](evidence/aac-enc-performance/sfb-quant-energy-dist-2026-10-28/source-change-attempted.patch).

Generated code: function 0x200 (512 B) → 0x48c (1,164 B), instructions
128 → 291, branches 3 → 9, panic paths 1 → 3, `clz` 2 → 6, `smull` 3 → 9.
LLVM interleaved the two chains, but duplicated the body/tail and cold paths.

### 18.3 Paired result — rejected

Four alternating test-build pairs, 15 reps, 19 fixtures + 40 s workload:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 395.260 ms | 399.315 ms | **−1.026%** |
| 2 | new → old | 404.136 ms | 402.723 ms | **+0.350%** |
| 3 | old → new | 405.044 ms | 406.621 ms | **−0.389%** |
| 4 | new → old | 404.499 ms | 406.245 ms | **−0.432%** |
| **Median** | | | | **−0.411%** |

Only 1/4 pairs positive; 0/4 ≥1%; the median is negative. Per-workload
medians were mostly small regressions (short fixtures −0.2% to −2.0%;
music +0.146%, long −0.072%). The code-size/register-pressure cost outweighed
the ILP.

### 18.4 Correctness and disposition

- Attempted tree: `aac_enc::quant` **10 passed**; production-baseline
  **1 passed**; 60-row hash verification unchanged (long hash
  `889c3be4ccd53dd7`). It was not full-suite-tested because it failed the
  performance gate first.
- Final tree: experimental edit reverted, test binary SHA restored to
  `611263…`; final hash verification unchanged; full release suite with
  artifacts **336 passed, 0 failed, 5 ignored**, without artifacts **326
  passed, 10 documented artifact-dependent failures, 5 ignored**.
- The retained `quant`, `tns`, and `bitenc` changes were untouched.

**Disposition: rejected and reverted. No production change is retained.**
The target is already a tight scalar pipeline; a two-line unroll is not a
viable optimization here.

### 18.5 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/sfb-quant-energy-dist-2026-10-28/README.md` | Method, profile, candidate, equivalence, codegen, results, limitations. |
| `.../starting-state.txt`, `.../baseline-summary.tsv` | State and session baseline. |
| `.../corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt` | Profiles. |
| `.../profile-summary.tsv` | Self/inclusive shares. |
| `.../asm-eqd.txt`, `.../asm-eqd-after.txt` | Before/after disassembly. |
| `.../source-change-attempted.patch` | Reverted attempt. |
| `.../testbuild-{before,after}-{1..4}.log`, `.../optimization-testbuild-ab.tsv`, `.../optimization-workloads.tsv` | Paired results. |
| `.../hash-verification-{attempted,final}.log`, `.../full-suite-*.log` | Hash and suite verification. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 19. 2026-10-28 `pe::form_factor` investigation (no justified change)

### 19.1 Profile

Fresh session baseline (post-§18 tree): **397.007, 401.120, and 404.528 ms**,
median **401.120 ms / 228.0×** (RTF 0.004386).

| Workload | `form_factor` self | Share | Inclusive | `calc_ld_data` self |
|---|---:|---:|---:|---:|
| corpus | 290 | 3.56% | 292 | 248 |
| onset | 293 | 4.63% | 295 | 86 |
| music | 220 | 3.36% | 220 | 233 |
| speech | 192 | 2.93% | 192 | 208 |

`sqrt_fixp`/`inv_sqrt_norm2` are inlined; `calc_ld_data` is partly inlined.
Hot-PC attribution was ~99% at `+0x25c` (corpus 290, music 220, speech 192;
onset 258 at the loop head `+0x1e0`) — the final `fmult_div2` of the inlined
fixed-point square root. `form_factor` is called once per channel per frame
from `stream_driver.rs:1101` and feeds `prepare_sfb_pe` and the
psychoacoustic model. The sampled cost is a long sqrt/interpolation
dependency chain per spectral line, not a bounds check or table lookup.

### 19.2 Candidates

| # | Candidate | Why not selected |
|---|---|---|
| 1 | Always execute the second-order `inv_sqrt_norm2` correction instead of branching on `fract != 0` | The correction is a no-op when `fract == 0`, but real spectra almost always have `fract != 0`; the branch is predictable and always running the correction adds work. ≪0.1% expected. |
| 2 | Reuse `inv_sqrt_norm2`’s normalized `val` to avoid re-normalizing `op` in `sqrt_fixp` | Saves one variable shift per line out of a ~40-instruction sqrt pipeline; <0.1% expected. |
| 3 | Two-line unroll of the per-line sqrt sum | The analogous §18 unroll grew code 127% and regressed **−0.411%** median; `form_factor` is already 0x640 bytes and this would duplicate the full sqrt/interpolation body. |

No candidate removes a provably redundant block of work or has a credible
path to a 1% end-to-end gain. No experiment was implemented.

### 19.3 Final validation and disposition

No code changed, so attempted-tree and final-tree validation are identical.
Final 60-row hash verification: all fixture hashes unchanged, long hash
`889c3be4ccd53dd7`. Full release suite with artifacts: **336 passed, 0 failed,
5 ignored**; without artifacts: **326 passed, 10 documented
artifact-dependent failures, 5 ignored**. Licensing hygiene OK;
`git diff --check` clean.

**Disposition: no production change.** The function’s cost is necessary
fixed-point square-root/interpolation work; the plausible micro-transforms
are far below the gate and would add risk for no measurable gain.

### 19.4 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/form-factor-2026-10-28/README.md` | Method, profile, candidates, no-change rationale, limitations. |
| `.../starting-state.txt`, `.../baseline-summary.tsv` | State and session baseline. |
| `.../corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt` | Profiles. |
| `.../profile-summary.tsv` | Self/inclusive shares. |
| `.../asm-form_factor.txt`, `.../codegen-hot-block.txt` | Disassembly and hot block. |
| `.../hash-verification-final.log`, `.../full-suite-*.log` | Hash and suite verification. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 20. 2026-10-28 `tns::autocorr_quarter_lags` investigation (no retained change)

### 20.1 Profile

Fresh session baseline (post-§19 tree): **397.899, 397.914, and 440.146 ms**,
median **397.914 ms / 229.8×** (RTF 0.004351). The third run was a thermal/
load outlier.

| Workload | `autocorr_quarter_lags` self | Share | Inclusive |
|---|---:|---:|---:|
| corpus | 340 | 4.15% | 340 |
| onset | 323 | 5.05% | 323 |
| music | 200 | 3.05% | 200 |
| speech | 255 | 3.90% | 255 |

The function (0x4bc, 303 instructions, 39 branches) is an i-outer,
fully-unrolled lag cascade keeping 13 `i64` accumulators. It reuses one
spectral `a` sample across all lags; each term is `fmult(a,b) >> scale`.
Hot PCs are spread through the cascade (`+0x2ac`, `+0x188`, `+0x19c`,
`+0x248`, `+0x178`, `+0x224` on corpus; onset concentrates at `+0x188`).
It is called four times per channel/frame from the `rxx1`/`rxx2` quarter
traversals.

### 20.2 Candidate and equivalence

The tested candidate transposed to lag-outer: for each `lag`, accumulate
`i in [start, stop-lag)` in ascending order. This is exactly the same term
set and per-lag accumulation order, so every `out[lag]` is bit-identical;
`out[0]` is untouched. The inner loop used two unit-stride slices and one
accumulator per lag. Exact attempted diff:
[`source-change-attempted.patch`](evidence/aac-enc-performance/tns-autocorr-quarter-lags-2026-10-28/source-change-attempted.patch).

Generated code shrank from 0x4bc/303 instructions/39 branches to
0x2fc/191/13, but the transpose introduced two spectral loads per term
instead of reusing `a`, and LLVM’s NEON form still reduces through scalar
`i64` multiplies.

### 20.3 Paired result — rejected

Four alternating test-build pairs, 15 reps, 19 fixtures + 40 s workload:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 407.896 ms | 417.558 ms | **−2.369%** |
| 2 | new → old | 405.934 ms | 413.108 ms | **−1.767%** |
| 3 | old → new | 404.392 ms | 409.884 ms | **−1.358%** |
| 4 | new → old | 406.357 ms | 411.122 ms | **−1.173%** |
| **Median** | | | | **−1.563%** |

0/4 pairs positive. Per-workload medians regressed almost everywhere
(short fixtures −1.3% to −4.8%; music −1.400%, long −1.336%, speech −1.755%/
−1.816%). A bounds-hoist fast path was assessed as a follow-up but would
remove only ~one instruction from a ~7-instruction term and changes
invalid-length panic timing; estimated <0.6% and not implemented.

### 20.4 Correctness and disposition

- Attempted tree: `aac_enc::tns` **5 passed** (including
  `fixed_merge_matches_rxx_bit_exact` and the D-013 census); production
  baseline **1 passed**; 60-row hash verification unchanged (long hash
  `889c3be4ccd53dd7`). It was not full-suite-tested because it failed the
  gate first.
- Final tree: experimental edit reverted, test binary SHA restored to
  `611263…`; final hash verification unchanged; full release suite with
  artifacts **336 passed, 0 failed, 5 ignored**, without artifacts **326
  passed, 10 documented artifact-dependent failures, 5 ignored**.
- The retained `quant`, `tns`, and `bitenc` changes were untouched.

**Disposition: rejected and reverted. No production change is retained.**
The existing i-outer cascade’s sample reuse is worth more than the
transpose’s smaller code and branch count.

### 20.5 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/tns-autocorr-quarter-lags-2026-10-28/README.md` | Method, profile, candidate, equivalence, codegen, results, limitations. |
| `.../starting-state.txt`, `.../baseline-summary.tsv` | State and session baseline. |
| `.../corpus.sample.txt`, `onset.sample.txt`, `music.sample.txt`, `speech.sample.txt` | Profiles. |
| `.../profile-summary.tsv` | Self/inclusive shares. |
| `.../asm-autocorr_quarter_lags*.txt` | Before/after disassembly. |
| `.../source-change-attempted.patch` | Reverted transpose. |
| `.../testbuild-{before,after}-{1..4}.log`, `.../optimization-testbuild-ab.tsv`, `.../optimization-workloads.tsv` | Paired results. |
| `.../hash-verification-{attempted,final}.log`, `.../full-suite-*.log` | Hash and suite verification. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 21. 2026-10-28 optimized source-line profiling (no production change)

### 21.1 Configurations and baseline

Production-like release baseline: **393.374, 399.934, 402.381 ms**, median
**399.934 ms / 228.7×** (RTF 0.004373), no DWARF sections.

Source-line build:

```sh
CARGO_PROFILE_RELEASE_STRIP=none RUSTFLAGS="-C debuginfo=2 -C strip=none" \
  CARGO_TARGET_DIR=/tmp/srcline-dbg2 \
  cargo test --release --locked --features aac_enc --lib --no-run
```

The default macOS dSYM split must be retained: `-C split-debuginfo=off`
produced no dSYM and no embedded DWARF on this toolchain. The working build
has an 11,149,232 B binary, a 32,040,983 B DWARF file, `__text` 1,833,148 B
(vs 1,834,748 production), median **404.660 ms** (+1.18%) and unchanged
hashes. Most inspected function sizes match exactly; `autocorr_quarter_lags`
(1184 vs 1212 B) and `BlockSwitch7process` (2296 vs 2292 B) differ slightly.
The debug build is suitable for source attribution, not acceptance
benchmarking.

`sample` + `atos -i` resolves file:line and inline stacks. The main
limitation is that some hot PCs resolve to `file:0`; the largest is the
`calc_sfb_dist` per-line path (7.86% corpus).

### 21.2 Source-line findings (corpus, worker-normalised)

| Symbol | Line / operation | Share |
|---|---|---:|
| `calc_sfb_dist` | `quant.rs:0` unresolved q/abs/MAX_QUANT/inv path | 7.86% |
| `calc_sfb_dist` | `q31.rs:219` inlined `shift_int` left-shift arm | 4.80% |
| `calc_sfb_dist` | `quant.rs:478` `quantize_line_body` call | 4.74% |
| `BlockSwitch::process` | `block_switch.rs:462` f32 IIR update | 3.39% |
| `dyn_bit_count_reuse` | `bitcount.rs:817` `find_max_merge` scan | 3.07% |
| `form_factor` | `q31.rs:699` `inv_sqrt_norm2` correction | 2.55% |
| `autocorr_quarter_lags` | `tns.rs:481` per-lag `fmult` accumulation | 2.54% |
| `quantize_spectrum` | `quant.rs:421` `quantize_line_body` call | 1.60% |

Inline attribution works: e.g. `q31.rs:219 -> accumulate_dist
(quant.rs:438) -> calc_sfb_dist (quant.rs:488)`, and `block_switch.rs:462 ->
BlockSwitch::process`. The same lines dominate on music and onset.

### 21.3 Ranked targets

1. **`quant::accumulate_dist` first shift (recommended).** The first
   `shift_int(diff, scale)` always has `scale = count_leading_bits(diff) >= 0`,
   yet the inlined code carries the `s >= 0` select at the 4.80% `q31.rs:219`
   line. Replacing only this call with
   `(diff as u32).wrapping_shl(scale as u32) as i32` is exact for every
   reachable input and removes the dead signed-shift branch. The second
   `shift_int(diff, -scale2)` must stay generic. Estimated removable portion
   is below the full 4.8% line; a 0.5–1.5% end-to-end gain is plausible and
   must be tested with the paired gate.
   **Superseded by §22:** the candidate is exact but LLVM already emitted the
   specialized `lsl`, and the 4.80% line belongs to the second shift
   (`quant.rs:438`), not this one.
2. **`bitcount::find_max_merge` scan.** `bitcount.rs:817` is 3.07% corpus /
   3.39% music. `dyn_bit_count_reuse` rescans the section list after every
   merge (O(n²)). An incremental maximum preserving the strict `>`
   first-max tie-break could save 1–1.5%, but has higher correctness risk and
   should follow target 1.
3. **`BlockSwitch::calc_window_energy` IIR (not recommended).**
   `block_switch.rs:462` is 3.39% corpus / 7.68% onset, but `s0`/`s1` carry
   across all eight windows and the full 1024 samples, so the windows are not
   independent. This is necessary serial f32 work; no exact local
   transformation is evident.

### 21.4 Validation and disposition

No production code changed. Final production-like tree: production-baseline
**1 passed**; 60-row hashes unchanged (long `889c3be4ccd53dd7`); full release
suite with artifacts **336 passed, 0 failed, 5 ignored**; without artifacts
**326 passed, 10 documented artifact-dependent failures, 5 ignored**;
licensing hygiene OK; `git diff --check` clean. Profiling itself made no
encoder faster; the output is a ranked target list.

### 21.5 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/source-line-profiling-2026-10-28/README.md` | Method, configs, line findings, ranked targets, limitations. |
| `.../starting-state.txt`, `.../baseline-summary.tsv`, `.../config-comparison.tsv` | State, production baseline, config metadata. |
| `.../dbg-corpus.sample.txt`, `dbg-music.sample.txt`, `dbg-onset.sample.txt` | Raw profiles. |
| `.../dbg-target-shares.tsv`, `dbg-hot-lines.tsv`, `dbg-hot-line-summary.tsv`, `dbg-multi-lines.tsv` | Shares, line hits, inline chains. |
| `.../dbg-baseline-*.log`, `dbg-*-bench.log`, `dbg-production-baseline.log` | Debug-build benchmarks and pinned test. |
| `.../hash-verification-final.log`, `.../full-suite-*.log` | Final production-tree validation. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 22. 2026-10-28 `accumulate_dist` first-shift specialization (rejected)

### 22.1 Question and equivalence

§21 ranked the first `shift_int(diff, scale)` in `quant::accumulate_dist`
(`quant.rs:435`) first, on the assumption that its `s >= 0` branch was dead
code at the 4.80% `q31.rs:219` line. The candidate replaced only that call
with `(diff as u32).wrapping_shl(scale as u32) as i32`, leaving the second
`shift_int(diff, -scale2)` generic.

The equivalence holds: `count_leading_bits` returns `0..=31` for every
`i32` input (including `0`, `i32::MIN` and `-1`), so `scale >= 0`; in that
arm `shift_int` is exactly `(v as u32).wrapping_shl(s as u32) as i32`, and
for `0 <= s <= 31` the cast and the implicit `rhs & 31` mask are identities.
Every boundary value is preserved. Full derivation:
`docs/evidence/aac-enc-performance/accumulate-dist-shift-specialization-2026-10-28/equivalence-proof.md`.

### 22.2 Generated code — already specialized

The baseline `calc_sfb_dist` (which inlines `accumulate_dist`) already
emitted `lsl w20, w21, w19` for the first shift, preceded by the
`clz`/`ccmp`/`csel` sequence that implements `count_leading_bits`'s
zero/`INT_MIN` guard. The candidate build produced an identical normalized
instruction stream: 704 B, 176 instructions, 0 differences; same `__text`
size (0x1bfefc) and binary size (10,930,480 B). Only link-time addresses
moved and 20 test/auxiliary/drop-glue symbols changed size.

### 22.3 Corrected §21 attribution

The 4.80% `q31.rs:219` share carries the inline chain `shift_int <-
accumulate_dist (quant.rs:438) <- calc_sfb_dist`; `quant.rs:438` is the
**second** shift. Its sampled PCs (`asr`, `cmp w19, #0x2`) are the
branch-free select between the left and right shift for
`shift_int(diff, -scale2)`, whose direction genuinely depends on `scale`.
The line is necessary work, not a dead branch. §21's ranking is corrected
accordingly.

### 22.4 Paired result — gate not met

Four alternating pairs, 15 reps, 19 fixtures + 40 s workload:

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 400.041 ms | 408.757 ms | −2.179% |
| 2 | new → old | 413.744 ms | 410.739 ms | +0.726% |
| 3 | old → new | 410.269 ms | 413.362 ms | −0.754% |
| 4 | new → old | 412.488 ms | 411.682 ms | +0.195% |
| **Median** | | | | **−0.279%** |

2/4 pairs positive, 0/4 ≥ 1%, median below the 1% gate. Per-workload
medians were flat (long +0.138%, music +0.262%, speech +0.279%/+0.407%),
consistent with identical code plus layout noise.

### 22.5 Validation and disposition

Candidate tree: focused `quant`/`scf`/`bitcount`/`bitenc`/`common::quantize`
and production-baseline tests passed; all eight paired benchmark runs
validated every fixture hash. The change was **reverted**; the rebuilt
production binary returned to its original SHA-256
`61126376e5672d86ca9de8d23cbcfa4f8e8d52d2684f7386612b19ed67c61756`. Final
tree: production-baseline **1 passed**; 60-row hashes unchanged (long
`889c3be4ccd53dd7`); full suite with artifacts **336 passed, 0 failed,
5 ignored**; without artifacts **326 passed, 10 documented
artifact-dependent failures, 5 ignored**; licensing hygiene OK.

### 22.6 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/accumulate-dist-shift-specialization-2026-10-28/README.md` | Method, proof summary, codegen finding, paired results, disposition. |
| `.../equivalence-proof.md`, `.../source-change-attempted.patch` | Dataflow proof and the reverted patch. |
| `.../asm-{before,after}-calc_sfb_dist.txt`, `.../asm-normalized-comparison.txt`, `.../codegen-comparison.tsv` | Instruction-level before/after evidence. |
| `.../baseline-{1,2,3}.log`, `.../pair-{1..4}-{old,new}.log`, `.../paired-summary.tsv`, `.../paired-workloads.tsv` | Benchmarks. |
| `.../focused-tests-candidate.log`, `.../hash-verification-{before,final}.log`, `.../full-suite-*.log` | Candidate and final-tree validation. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 23. 2026-10-28 `find_max_merge` compacted-candidate experiment (rejected)

### 23.1 Hypothesis and measured topology

§21/§22 left `bitcount::find_max_merge` as the next target
(`bitcount.rs:817`, 3.07% corpus / 3.39% music). `gm_stage2` rescans every
section start after each merge; the scan advances by
`sections[i].sfb_cnt`, so each step carries a dependent load.

A temporary instrumented build (reverted) measured one second of stereo
music: **2,244** stage-2 calls, **53,771** candidates (24.0/call), **37,832**
merges (16.9/call) and **725,165** scan steps (19.2 per merge, 323 per
call). This ruled out a tournament tree: ~19 scan steps vs ~18 dependent
node visits for three 6-level updates per merge.

### 23.2 Semantics and correctness

Candidates are section starts with a successor, visited in ascending order;
strict `>` keeps the first maximum, and a best gain of 0 stops the loop. A
merge changes exactly three candidates — the merged section, its
predecessor, and the absorbed successor — so an ordered list of
`(gain, start)` pairs can be maintained with two point updates and one
ordered removal per merge. The scan order and tie rule are unchanged.
Full proof: `docs/evidence/aac-enc-performance/find-max-merge-incremental-maximum-2026-10-28/correctness-argument.md`.

A 20,000-case randomized differential test against a verbatim copy of the
original `gmStage2`/`findMaxMerge` produced bit-identical final sections and
lookup rows in every case (13,930 with real merges). Mutation checks caught
a `>=` tie rule and a wrong last-section removal. All 19 fixture hashes and
the long hash were unchanged on the candidate.

### 23.3 Generated code

`dyn_bit_count_reuse` shrank 5,104 -> 4,492 B; `gm_stage2` became
out-of-line (904 B). The scan changed from a 12-instruction loop chasing
`sections[i].sfb_cnt` to a 7-instruction loop over independent 8-byte
`(gain, start)` loads. The ordered removal compiles to one `_memmove` call
per merge.

### 23.4 Paired result — gate not met

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old -> new | 415.208 ms | 411.639 ms | +0.860% |
| 2 | new -> old | 417.721 ms | 414.242 ms | +0.833% |
| 3 | old -> new | 418.192 ms | 416.396 ms | +0.429% |
| 4 | new -> old | 418.516 ms | 414.245 ms | +1.021% |
| **Median** | | | | **+0.846%** |

4/4 pairs positive but the median and 2/4-at-1% conditions fail. Heavy
workloads gained (music +0.974%, long +0.846%, Eroica +1.418%, stress
+0.783%); sub-3 ms fixtures were mixed and noise-dominated. Baseline: median
411.808 ms.

### 23.5 Disposition and validation

**Rejected and reverted.** The rewrite is exactly equivalent and removes
the loop-carried walk, but the residual cost is merge bookkeeping (two
`calc_merge_gain` calls plus the `memmove`), not the scan, so the 1% gate is
not reachable by further scan tuning. The production binary rebuilt to its
original SHA-256 `61126376…`; `bitcount.rs` has no diff. Final tree:
production-baseline **1 passed**; 60-row hashes unchanged (long
`889c3be4ccd53dd7`); full suite with artifacts **336 passed, 0 failed,
5 ignored**; without artifacts **326 passed, 10 documented
artifact-dependent failures, 5 ignored**; licensing hygiene OK.

**Status update (2026-10-28):** retained under the prospective Tier 2
policy in §27 after the eight-pair remeasurement (§27.2). The original
four-pair measurement and rejection verdict above remain the historical
record; §27.5 records the retained-tree validation.

### 23.6 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/find-max-merge-incremental-maximum-2026-10-28/README.md` | Method, topology, proof summary, paired results, disposition. |
| `.../correctness-argument.md`, `.../candidate-attempted.patch` | Invariant proof and the reverted candidate. |
| `.../stage2-topology.txt`, `.../stage2-topology-raw.log`, `.../instrumentation-attempted.patch` | Temporary counters and their patch. |
| `.../asm-*-dyn_bit_count_reuse.txt`, `.../asm-after-gm_stage2.txt`, `.../codegen-comparison.tsv` | Before/after codegen. |
| `.../baseline-{1,2,3}.log`, `.../pair-{1..4}-{old,new}.log`, `.../paired-summary.tsv`, `.../paired-workloads.tsv` | Benchmarks. |
| `.../focused-tests-candidate.log`, `.../hash-verification-{candidate,final}.log`, `.../full-suite-*.log` | Candidate and final-tree validation. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 24. 2026-10-28 `calc_sfb_dist` `quant.rs:0` attribution (no production change)

### 24.1 What the region is

§21 reported 7.86% of corpus samples in `calc_sfb_dist` resolving to
`quant.rs:0`. Three fresh captures reproduce the line share
(7.45%, 7.75%, 8.20%; §21 corpus 7.77%, music 10.30%, onset 0.43%), and the
rebuilt dSYM has the same UUID as the §21 build
(`52E58310-6CC2-3673-B174-94EB9D9DE468`). The production and dSYM builds
have identical `calc_sfb_dist` instruction streams (704 B, 176
instructions, 0 normalized differences).

`quant.rs:0` is a DWARF line-table gap covering 26 of 176 instructions in
four ranges:

| Range | Insns | Operation | Frequency |
|---|---:|---|---|
| `0x038` | 1 | `xfsf = 0` (475) | once per call |
| `0x05c..0x0b4` | 22 | hoisted quantizer/table setup (468-474 + inlined tables) | once per call |
| `0x0b4` | 1 | `inv_quantize_line` `q == 0` return (344-346) feeding `accumulate_dist` | ~50% of lines |
| `0x1ac` | 1 | zero store (477-478) | ~50% of lines |
| `0x1d0` | 1 | `accu = -(q as i32)` in inlined `inv_quantize_line` (350) | ~48% of lines |

A temporary counter build (reverted) measured 52.5% zero-quantized lines
for music (sine 50.4%, onset 45.2%, noise 9.0%), so both per-line line-0
paths are live; the once-per-call setup is not a sample contributor.

### 24.2 Attribution correction

`sample` merges several leaf PCs into one line. §21's per-PC numbers
(`+0x1d0` 6.58%, `+0xb4` 1.27%) assigned each merged group's count to the
first listed offset and are not valid; only the line total (635 samples,
7.86% of the §21 worker total) is. The groups almost always contain both
`+0xb4` and `+0x1d0`, so their split is not recoverable from these captures.

### 24.3 Disposition

No removable work was identified: the setup is hoisted and runs once per
call, the `q == 0` return and zero store are the algorithm's fast path, and
the `neg` is required by `inv_quantize_line`'s magnitude. The sampled PCs
are 1-cycle `mov`/`neg` operations at block entries, so their counts likely
reflect the following dependency chain (or the ~50/50 `q == 0` branch)
rather than those instructions. Stop optimization on this region. The only
structural observation is the call rate (2,229 `calc_sfb_dist` calls per
channel-frame at ~14.4 lines/call); that is a caller-level question with no
supporting evidence here.

### 24.4 Validation

No production change: `quant.rs`'s retained diff hash is unchanged,
`bitcount.rs` is clean, HEAD and the index are unchanged, and the retained
diffs remain 379/55. Production baseline **1 passed**; 60-row hashes
unchanged (long `889c3be4ccd53dd7`); `git diff --check` clean.

### 24.5 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/calc-sfb-dist-attribution-2026-10-28/README.md` | Method, identity checks, findings, limitations. |
| `.../capture-{1,2,3}.sample.txt`, `.../capture-summary.tsv`, `.../q0-groups.tsv` | Raw captures, line shares, group PCs. |
| `.../instruction-lines.tsv`, `.../dwarf-line-map-calc_sfb_dist.tsv`, `.../quant-rs0-pcs.tsv` | PC resolution and line-table rows. |
| `.../asm-calc_sfb_dist{,-production}.txt`, `.../attribution-table.tsv` | Disassembly and region mapping. |
| `.../quant-path-frequency.log`, `.../instrumentation-quant.patch` | Temporary counters and their removed patch. |
| `.../production-baseline.log`, `.../hash-verification-final.log`, `.../quant-diff-sha-before.txt` | Final validation. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 25. 2026-10-28 scalefactor-search call-path audit (no production change)

### 25.1 Call hierarchy

Every `calc_sfb_dist` call comes from the S4 estimator
(`estimate_channel_with_inv_ws`, `scf.rs:1142`): `improve_scf` (4 sites,
phase 0), `assimilate_single_scf` (593, phase 1),
`assimilate_multiple_scf` (735, phase 2),
`assimilate_multiple_scf2` steps 1/2 (895/985, phases 3/4), and the clamp
(1357, phase 5). Step 3 of `scf2` uses `calc_sfb_quant_energy_and_dist` and
does not re-quantize. The estimator runs **~10 times per audio
channel-frame** on music and speech, which turns §24's 2,229 calls per
channel-frame into 222 per S4 pass.

### 25.2 Measurements

Temporary counters (removed; patch in the evidence directory):

| Workload | Calls/pass | Lines/call | Dup calls | Dup lines |
|---|---:|---:|---:|---:|
| music | 222.4 | 14.4 | 18.3% | **17.8%** |
| speech | 115.6 | 14.2 | 15.6% | 13.2% |
| onset | 33.9 | 7.5 | 19.6% | 16.3% |
| sine | 63.5 | 6.7 | 20.3% | 17.2% |
| noise | 163.3 | 14.6 | 20.7% | 17.7% |

Phase shares for music: 38.9% initial, 24.3% single, 14.4% multiple, 18.6%
scf2-coarse, 3.8% scf2-fine, 0% clamp. Duplicates are mostly cross-phase
(improve_scf → later passes: 21,706 of 38,433; single → later: 15,292).
Within one pass the spectrum is immutable (`spec` cloned at 1169; the only
`fill(0)` is the post-search dead-band zeroing at 1384), so a repeated
`(band, gain, dzone)` has identical inputs; `dzone` is always false and
only `gain` varies.

A per-band LRU over gains captures the repeats: LRU-4 83–94%, LRU-8
99–100%. Consecutive S4 passes see different spectra (1/942 repeats for
music, 0/942 speech), so a cache must be cleared per pass.

### 25.3 Hypothesis and lookup cost

Per-pass, per-band LRU-8 cache of `(gain → dist, quant)`. The function is
pure and the cached quant bytes reproduce every output, including
`MAX_QUANT` early-out tails. Upper bound: 17.8% × 27.0% ≈ 4.8% on music,
~3% corpus. Measured lookup costs: `std::HashMap` 11.6–14.0 ns (≈ the
~69 ns average call's whole saving), a small open-addressed/per-band table
1.3–2 ns. A global hash map is therefore not viable; the per-band design
could net 2–4% on music and ~2–3% corpus. One isolated experiment is
supported only with that design; otherwise stopping is defensible.

### 25.4 Validation

No production change: `quant.rs` and `scf.rs` restored byte-for-byte
(`quant.rs` diff SHA-256 unchanged, `scf.rs` no diff), HEAD and index
unchanged, retained diffs 379/55, `bitcount.rs` clean. Production binary
`61126376…` unchanged; production baseline **1 passed**; 60-row hashes
unchanged (long `889c3be4ccd53dd7`); `git diff --check` clean.

### 25.5 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/scalefactor-search-call-path-2026-10-28/README.md` | Hierarchy, counters, duplicate matrix, LRU simulation, hypothesis. |
| `.../call-sites.tsv`, `.../workload-counts.tsv`, `.../call-path-counters.log` | Call sites and per-workload counters. |
| `.../instrumentation-call-path.patch` | Temporary counters (removed). |
| `.../lookup-costs.txt`, `.../lookup-bench.rs` | Lookup-cost measurement. |
| `.../production-baseline.log`, `.../hash-verification-final.log` | Final validation. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 26. 2026-10-28 `calc_sfb_dist` per-band LRU-8 cache (rejected)

### 26.1 Design and lifetime

A fixed-capacity, per-band LRU-8 cache scoped to one
`estimate_channel_with_inv_ws` pass. The key is
`(band, gain, dzone, slice length)`; the value is the returned distance plus
the exact prefix `calc_sfb_dist` wrote to `quant`. `reset(cnt)` clears the
band entries and the storage pool at pass start, so no entry can cross a
pass boundary. Inside a pass the spectrum is immutable (cloned at
`scf.rs:1169`; the only `fill(0)` is the post-search dead-band zeroing), so
the key fully determines both outputs.

`quant.rs` gained a private `calc_sfb_dist_written() -> (i32, usize)`
returning the written prefix length; the public `calc_sfb_dist` remains a
thin wrapper with identical arithmetic. A hit copies exactly the cached
prefix and leaves the tail untouched, matching the `MAX_QUANT` early-out
behaviour at non-pre-zeroed sites (`improve_scf`). All six §25 call-site
groups were converted; storage and lookup do not allocate.

### 26.2 Correctness

New tests: hit replay, early-out tail preservation (pinned negative-gain
case), LRU eviction after eight gains, reset/empty band, 50k randomized
cache-vs-direct cases with fixed per-band spectra, and a full-pass
cache-on vs cache-off differential over every committed S4 corpus case and
checkpoint (`scf`, `gain`, `quant_spec`, `mutated_spectrum`). The existing
S4 oracle tests also pass with the cache enabled, as do all 19 fixture
hashes and the long hash `889c3be4ccd53dd7` on the candidate tree.

### 26.3 Hit rate and footprint

Measured hit rates: music 18.2% (38,140/209,522), speech 15.4%, onset
19.6%, sine 20.3%, noise 20.7% — exactly the §25 LRU-8 simulation.
Footprint: 7,200 B entries (45 bands × 8 slots × 20 B) + 8,192 B pool
capacity = **15,392 B** per workspace; no per-lookup allocation.

### 26.4 Paired result — gate not met

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old -> new | 411.529 ms | 415.330 ms | −0.924% |
| 2 | new -> old | 412.817 ms | 413.035 ms | −0.053% |
| 3 | old -> new | 411.828 ms | 413.463 ms | −0.397% |
| 4 | new -> old | 411.638 ms | 416.130 ms | −1.091% |
| **Median** | | | | **−0.660%** |

0/4 positive. Heavy workloads regressed less (music −0.511%, long −0.397%)
than speech (−1.661%/−1.865%); sub-3 ms fixtures −0.1% to −6.1%. A
cache-bypassed build of the same refactor measured 424.6/427.7 ms in an
ad-hoc sequence (baseline 410.0–419.0, cache-on 416.7–419.6), indicating the
extra call layer and parameter threading cost about as much as the cache
saves.

### 26.5 Disposition and validation

**Rejected and reverted.** The cache is bit-exact and hits the modelled
rate, but per-call lookup/storage plus the refactor overhead exceed the
savings. `quant.rs` and `scf.rs` were restored byte-for-byte (`quant.rs`
diff SHA-256 unchanged, `scf.rs` no diff); the production binary rebuilt to
`61126376…`. Final tree: production baseline **1 passed**; 60-row hashes
unchanged; full suite with artifacts **336 passed, 0 failed, 5 ignored**;
without artifacts **326 passed, 10 documented artifact-dependent failures,
5 ignored**; licensing hygiene OK.

### 26.6 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/calc-sfb-dist-lru8-cache-2026-10-28/README.md` | Design, correctness, hit rates, footprint, benchmarks. |
| `.../candidate-attempted.patch` | Reverted candidate (quant + scf). |
| `.../hit-rate-instrumentation.patch`, `.../cache-hit-rate.log`, `.../cache-footprint.log` | Temporary diagnostics. |
| `.../baseline-{1,2,3}.log`, `.../pair-{1..4}-{old,new}.log`, `.../paired-summary.tsv`, `.../paired-workloads.tsv` | Benchmarks. |
| `.../diag-*.log`, `.../bypass-run.log` | Cache-on/off diagnostics. |
| `.../hash-verification-candidate.log`, `.../final-*.log` | Candidate and final validation. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 27. 2026-10-28 rejected-candidate reassessment (no production change)

### 27.1 Inventory and decision framework

Every rejected candidate from §14–§26 was re-read from its evidence
directory and classified: bit-exactness, paired result, codegen, complexity,
and rejection reason ([`inventory.tsv`](evidence/aac-enc-performance/rejected-candidates-reassessment-2026-10-28/inventory.tsv)).
The old 1% gate stays as the standard; a **prospective** framework adds an
explicit tier for small consistent wins:

- **Tier 1 (retain):** median ≥ +1.0%, ≥3/4 pairs positive, ≥2/4 ≥1%, no
  heavy-workload regression, hashes + suite pass.
- **Tier 2 (small but consistent, retain with an explicit note):** median
  ≥ +0.4% over ≥8 alternating pairs, ≥70% positive, no heavy workload
  regressing beyond noise, small differential-tested change.
- **Tier 3 (neutral for maintainability):** median within ±0.2% and a
  measurable code-size/complexity/hazard reduction; otherwise reject.
- **Tier 4 (reject):** negative or inconsistent results, or a heavy-workload
  regression.

The framework is prospective; no historical verdict is rewritten.

### 27.2 `find_max_merge` remeasurement

The exact §23 patch was re-applied and rebuilt to the same candidate SHA-256
`31a0269a…`; the 20k differential, mutation checks, 60 hashes and
production baseline passed again. Eight alternating pairs (15 reps, 19
fixtures + 40 s workload) gave **6/8 positive, median +0.594%**, 3/8 ≥1%.
All heavy workloads were positive: music +0.857%, long +0.783%, Eroica
+1.448%, speech +0.153%/+0.128%. Combined with §23 the candidate has
**10/12 positive pairs (83%)** across two sessions. Pair-level spread is
large on sub-3 ms fixtures, and both binaries are `cfg_test` builds with
different test-code layout.

Under Tier 2 the candidate qualifies: median +0.594% over 8 pairs, 75%
positive, all heavy workloads positive, ~30 production lines with a
verbatim-reference differential test.

### 27.3 Disposition and recommendation

The audit changed no production code: the patch was reversed, the binary
rebuilt to `61126376…`, `bitcount.rs`/`scf.rs` are clean, and the retained
diffs are unchanged. Final: production baseline **1 passed**; 60-row hashes
unchanged (long `889c3be4ccd53dd7`); `git diff --check` clean.

**Recommendation (carried out 2026-10-28):** `find_max_merge` was retained
by re-applying the existing patch; §27.5 records the retained-tree
validation. The next experiment is one separate 8-pair run for the §16
trial-array reuse (3/4 positive at +0.602%, also smaller code); do not
bundle it. Do not pursue `bit_count` `chunks_exact`, the `accumulate_dist`
shift, the LRU-8 cache, the energy/distortion unroll, or the TNS transpose:
they are neutral or negative with no plausible new mechanism.

### 27.4 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/rejected-candidates-reassessment-2026-10-28/README.md` | Inventory, framework, remeasurement, recommendation. |
| `.../inventory.tsv` | Seven candidates with exactness, results, codegen, disposition. |
| `.../pair-{1..8}-{old,new}.log`, `.../paired-summary.tsv`, `.../paired-workloads.tsv` | Remeasurement. |
| `.../candidate-*.log`, `.../final-*.log` | Candidate and final validation. |

### 27.5 Retention (2026-10-28)

The §23 patch was re-applied unchanged: the resulting diff is byte-identical
to
[`candidate-attempted.patch`](evidence/aac-enc-performance/find-max-merge-incremental-maximum-2026-10-28/candidate-attempted.patch),
and the rebuilt binary SHA-256 is
`31a0269a037742bc3be4ac12664f928b59bcfbb8d04f6a68cfd04a83e211e7ea` — the
same binary measured in §27.2. Retained-tree validation: `aac_enc::bitcount`
**13 passed** (including the 20,000-case differential and both mutation
checks), production baseline **1 passed**, all 60 hash rows unchanged
(long `889c3be4ccd53dd7`), full release suite with artifacts **338 passed,
0 failed, 5 ignored** (the two extra tests are the candidate's differential
tests), licensing hygiene OK, and `git diff --check` clean. The retained
diffs are `bitcount.rs` +248/−35 plus the unchanged pre-existing
`bitenc.rs`/`quant.rs`/`tns.rs` changes (379/55).

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 28. 2026-10-28 SCF trial-array reuse under Tier 2 (rejected)

### 28.1 Candidate

The §16 patch was re-applied unchanged (byte-identical diff): the
`[i32; 60]` `trial` array and its baseline copy move out of the
`assimilate_multiple_scf2` `nw` loop; each iteration rewrites only
`start..stop`. `trial` differs from `scf` only in that region, accepted
trials write the same values back, and rejected trials leave `scf`
unchanged, so the reuse is exactly equivalent. Measured on the current tree
with `find_max_merge` retained.

### 28.2 Correctness

`aac_enc::scf` **6 passed** (including both exact S4 oracle tests),
`aac_enc::bitcount` **13 passed**, production baseline **1 passed**, all 60
hash rows unchanged (long `889c3be4ccd53dd7`).

### 28.3 Performance — Tier 2 gate not met

Baseline: 409.129 / 410.330 / 411.859 ms, median **410.330 ms**.

| Pair | Order | Old | New | Improvement |
|---|---|---:|---:|---:|
| 1 | old -> new | 413.103 | 412.105 | +0.242% |
| 2 | new -> old | 416.253 | 416.571 | −0.076% |
| 3 | old -> new | 416.449 | 414.689 | +0.423% |
| 4 | new -> old | 414.839 | 413.804 | +0.250% |
| 5 | old -> new | 418.037 | 408.274 | +2.336% |
| 6 | new -> old | 414.118 | 420.995 | −1.661% |
| 7 | old -> new | 416.132 | 411.086 | +1.212% |
| 8 | new -> old | 415.050 | 414.509 | +0.130% |
| **Median** | | | | **+0.246%** |

6/8 positive (75%); heavy workloads all non-negative (music +0.425%, long
+0.505%, speech +0.246%/+0.755%, Eroica +0.013%). `estimate_channel_with_inv_ws`
shrank 0x3748 -> 0x3690 (−184 B; `__text` −192 B) as a diagnostic.

Tier 2 requires median ≥ +0.4%; the measured median is +0.246%, so the
candidate fails the pre-registered magnitude threshold despite the
consistent direction.

### 28.4 Disposition and validation

**Rejected and reverted.** `scf.rs` has no diff; the retained
`bitcount.rs` (+248/−35) and the `bitenc.rs`/`quant.rs`/`tns.rs` changes
(379/55) are unchanged; the binary rebuilt to `31a0269a…`. Final:
production baseline **1 passed**; 60-row hashes unchanged; full suite with
artifacts **338 passed, 0 failed, 5 ignored**; licensing hygiene OK.

### 28.5 Experiment files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/scf-trial-array-reuse-2026-10-28/README.md` | Hypothesis, equivalence, correctness, paired results, decision. |
| `.../candidate-attempted.patch` | Exact reverted candidate. |
| `.../baseline-{1,2,3}.log`, `.../pair-{1..8}-{old,new}.log`, `.../paired-summary.tsv`, `.../paired-workloads.tsv` | Benchmarks. |
| `.../codegen-size.txt` | Function and `__text` sizes. |
| `.../candidate-*.log`, `.../final-*.log` | Candidate and final validation. |

**No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**

## 29. 2026-10-28 aggregate performance checkpoint (valid)

### 29.1 Question and verdict

What is the combined end-to-end effect of the retained optimization set
relative to the pre-optimization source? **Valid paired comparison: +13.932%
median over eight alternating pairs**, 8/8 pairs positive, all 20 workloads
positive, every hash unchanged. The baseline was rebuilt from the current
source with the retained performance hunks reversed, and both arms were built
in the same isolated worktree with the same compiler, profile, features, and
harness.

| Metric (20 workloads, 91.45 s audio) | Baseline | Current | Change |
|---|---:|---:|---:|
| Production total, paired-run median | 481.191 ms | 415.247 ms | **+13.932%** |
| Realtime × | 190.0× | 220.2× | +15.9% |
| RTF (encoding s / audio s) | 0.005262 | 0.004541 | −13.7% |
| Median pair improvement | — | — | **+13.932%** (8/8) |

The 13.932% headline is the median of the eight pair improvements, matching
the established report convention; the ratio of the two arm medians is
13.70%. The five standalone baseline runs (three before the pairs, two after)
had a
median of **481.947 ms** (190.0×, RTF 0.005262) and a range of 474.308–482.161
ms. The original §4 four-run baseline range was 468.2–481.9 ms (median
470.6 ms); the fresh median lies inside that range and is 2.4% slower than the
historical median, consistent with the documented session drift. The fresh
paired baseline is the primary reference. A naive cross-session comparison of
the current 415.247 ms to the historical 470.6 ms would give 11.8%, but it
mixes sessions; the paired 13.93% is the checkpoint estimate.

### 29.2 Baseline reconstruction and comparability

HEAD is `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; the index was empty. The
retained optimizations are uncommitted worktree changes, so the baseline was
built in a detached worktree at HEAD with the current non-performance changes
copied in and the four retained performance patches excluded:

| File | Baseline source | Removed retained change |
|---|---|---|
| `quant.rs` | HEAD | §11 `calc_sfb_dist` cleanup; §17 `quantize_line_body` shared magnitude path and `inv_quantize_line` mask |
| `bitenc.rs` | HEAD | §15 `write_bits` 8-byte window |
| `bitcount.rs` | HEAD | §23/§27 ordered-candidate `find_max_merge` and tests |
| `tns.rs` | current minus the §13 hunk | §13 `scale_up_spectrum` direct-slice loop only; later TNS tests retained |

The non-performance worktree changes (test-only module registration,
`block_switch`/`psy` tests, `stream_driver` S1/S2 captures and benchmark
harness) are present in both arms. A recursive source diff between the two
built trees was clean after the four patches were reapplied.

Both arms used:

```sh
cargo test --release --locked --features aac_enc --lib --no-run
```

in the same worktree and target directory, on rustc 1.99.1 (58cf8f9c3
2026-10-13, LLVM 23.1.1), Cargo 1.99.1, `aarch64-apple-darwin`, release
Cargo defaults (`opt-level=3`, no LTO, `codegen-units=16`, `strip=debuginfo`),
features `default`/`aac_enc`, no `RUSTFLAGS` or `target-cpu` override.

| Arm | SHA-256 | Size | `__text` |
|---|---|---:|---:|
| Baseline | `54eb92771609c1d85e40eeaf0dd23ee200ae61bc22935840b951acad45deae8e` | 10,926,960 B | 1,831,036 B |
| Current, isolated rebuild | `261b747a8e289f960e3bfc596b518ac8a3401451eec979c406ecd113faeb55dd` | 10,936,768 B | 1,837,692 B |
| Current, repository build | `31a0269a037742bc3be4ac12664f928b59bcfbb8d04f6a68cfd04a83e211e7ea` | 10,936,768 B | 1,837,692 B |

The isolated current rebuild and the repository build are source-identical
with the same `__text` size; their SHA-256 differs because the test binary
embeds the absolute worktree path, shifting constant addresses. The
retained-optimization functions and 2,893 of 2,903 text symbols have identical
sizes; the 10 differences are test/auxiliary symbols and the encoder helper
`dyn_bit_count` (4–8 B each), consistent with path-dependent constant
materialization. The benchmark used the two same-path binaries. A two-pair
build-path control (repository vs isolated current) measured +1.168% and
−1.335% (median −0.084%), so there is no systematic build-path difference
beyond the ±1.3% run noise. Code size increased by 9,808 B total / 6,656 B
text; this is a diagnostic, not a performance claim.

### 29.3 Method

The established release test-build harness ran the public production
`Encoder` API: AAC-LC VBR-5, ADTS, 1024-sample frames, mono/stereo,
44.1/48 kHz, 8,000-sample chunks. The corpus is the 19 committed
`M58_FIXTURES` plus the derived 40 s `m7-music-stereo-48k-x4`; 91.45 s of
audio. One untimed warm-up pass preceded 15 measured repetitions per fixture
and mode, with a fresh encoder per repetition. Three standalone baseline runs
came before eight alternating old/new pairs (old→new on odd pairs, new→old on
even pairs), followed by two post-series baseline runs.

The headline uses the harness `AACPERF_AGG production` value: the median of
complete-corpus per-repetition sums, not a sum of per-fixture medians. Pair
improvement is `(old − new) / old × 100`. Timing covers `Encoder::new`
(reported separately) plus `push`/`flush`; fixture synthesis/decoding,
interleaving, ADTS validation, hashing, and disk I/O are outside, while copying
emitted bytes into a preallocated buffer is inside. Every run validates the
committed byte totals and FNV-1a hashes.

### 29.4 Results

| Pair | Order | Old (ms) | New (ms) | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 481.343 | 414.401 | +13.907% |
| 2 | new → old | 484.223 | 414.398 | +14.420% |
| 3 | old → new | 485.920 | 417.086 | +14.166% |
| 4 | new → old | 480.635 | 416.092 | +13.429% |
| 5 | old → new | 481.039 | 416.439 | +13.429% |
| 6 | new → old | 481.882 | 413.769 | +14.135% |
| 7 | old → new | 480.663 | 413.581 | +13.956% |
| 8 | new → old | 479.291 | 420.122 | +12.345% |
| **Median** | | **481.191** | **415.247** | **+13.932%** |

8/8 pairs positive; range 12.345–14.420%, interquartile range 13.429–14.151%.
Old-arm runs span 479.291–485.920 ms; new-arm runs 413.581–420.122 ms.

| Workload | Old median (ms) | New median (ms) | Improvement |
|---|---:|---:|---:|
| `m2-click-mono-48k` | 0.990 | 0.911 | +7.983% |
| `m2-edge-mono-48k` | 0.216 | 0.199 | +7.622% |
| `m2-impulse-mono-48k` | 0.184 | 0.172 | +6.868% |
| `m2-onset-stereo-48k` | 1.957 | 1.852 | +5.361% |
| `m2-silence-stereo-48k` | 1.213 | 1.130 | +6.854% |
| `m2-sine-stereo-48k` | 2.615 | 2.494 | +4.621% |
| `m2-stepdown-stereo-48k` | 1.933 | 1.823 | +5.694% |
| `m3-multitone-stereo-48k` | 2.988 | 2.819 | +5.635% |
| `m3-noise-mono-48k` | 1.373 | 1.159 | +15.557% |
| `m3-onset-stereo-441k` | 1.961 | 1.814 | +7.478% |
| `m3-quiet-sine-stereo-48k` | 2.757 | 2.515 | +8.798% |
| `m3-sine-stereo-441k` | 2.726 | 2.536 | +6.949% |
| `m3-sweep-stereo-48k` | 2.744 | 2.580 | +5.964% |
| `m3-tonal-noise-stereo-48k` | 1.908 | 1.667 | +12.626% |
| `m6-stress-stereo-48k` | 1.992 | 1.731 | +13.078% |
| `m7-music-stereo-48k` | 62.923 | 53.800 | +14.499% |
| `m7-music-stereo-48k-x4` | 250.954 | 215.155 | +14.265% |
| `m7-speech-clean-stereo-48k` | 40.401 | 35.415 | +12.341% |
| `m7-speech-pauses-stereo-48k` | 40.342 | 35.466 | +12.087% |
| `m8-music-eroica-stereo-48k` | 58.701 | 49.664 | +15.394% |

All 20 workloads improved. The five heavy workloads (>10 ms) have a median
gain of **+14.265%**; the 15 short fixtures have a median gain of **+6.949%**.
The short fixtures are 0.2–3.0 ms, so their pair-to-pair ranges are wider and
are not the primary evidence. Full per-pair ranges are in
[`paired-workloads.tsv`](evidence/aac-enc-performance/aggregate-performance-checkpoint-2026-10-28/paired-workloads.tsv).

The arithmetic sum of the individual medians is 2.82 + 1.93 + 2.10 + 7.38 +
0.594 ≈ 14.82%, versus the measured combined 13.93%. The individual
percentages are not additive; the difference is consistent with interaction,
code-layout, and session effects, and no sum was used as the checkpoint.

### 29.5 Correctness and validation

| Check | Baseline | Current |
|---|---|---|
| Production baseline test | 1 passed | 1 passed |
| Focused tests | `quant` 10, `bitenc` 9, `bitcount` 11, `tns` 5 | §28 retained tree: full suite 338 passed |
| 60 benchmark hash rows | 60/60 match | 60/60 match |
| Long-workload hash | `889c3be4ccd53dd7` | `889c3be4ccd53dd7` |
| Explicit row comparison | 60/60 identical (`hash-comparison.tsv`) | same |

The baseline has 11 `bitcount` tests because the retained candidate's two
differential tests are absent there; the current tree has 13. Full release
suite with artifacts on the current tree: **338 passed, 0 failed, 5 ignored**;
licensing hygiene OK. Matching hashes establish bit-exact output on this finite
corpus, not universal perceptual equivalence.

### 29.6 Limitations and recommendation

Limitations: one machine/toolchain/session; single-threaded process with no CPU
pinning, so core placement and thermal drift remain the main variability
source (the standalone baseline was 474.3–482.2 ms and stabilized around
482 ms); the established test-build harness is 0.25–0.86% conservative versus
the non-test production example; short fixtures are dominated by fixed
overhead; and the baseline is a deterministic source reconstruction, not the
original pre-optimization binary from the historical session. The exact
measured test-only harness (including the shared S1/S2 captures) is preserved
under `.../aggregate-performance-checkpoint-2026-10-28/harness/` rather than
committed as production source, because it is entangled with unrelated
quality-test work; applying that snapshot reproduces the measured test build.

**Recommendation: stop incremental performance work for now.** The retained
set delivers a large, consistent end-to-end gain with unchanged output. The
remaining dominant cost is the S4 scalefactor/quantization search, but recent
local candidates there and in the bit path were neutral or below the
pre-registered gates. Further work should require a new structural target and
the existing Tier 1/Tier 2 evidence rules, using this checkpoint as the
regression reference.

### 29.7 Checkpoint files

| File | Contents |
|---|---|
| `docs/evidence/aac-enc-performance/aggregate-performance-checkpoint-2026-10-28/README.md` | Baseline definition, method, all results, limitations. |
| `.../starting-state.txt`, `.../git-status-start.txt` | Initial Git state and source-diff inventory. |
| `.../build-config.txt` | Binaries, toolchain, build/benchmark commands, corpus hashes, timing rules. |
| `.../perf-{quant,bitenc,bitcount,tns}.patch` | Exact retained diffs reversed for the baseline. |
| `.../baseline-{1,2,3}.log`, `.../post-baseline-{1,2}.log`, `.../pair-{1..8}-{old,new}.log` | Raw benchmark runs. |
| `.../paired-summary.tsv`, `.../paired-workloads.tsv`, `.../workloads.tsv` | Aggregates, per-workload medians/ranges, corpus metadata. |
| `.../baseline-hash-verification.log`, `.../current-rebuild-hash-verification.log`, `.../hash-comparison.tsv` | 60-row hash verification and comparison. |
| `.../control-pair-{1,2}-{repo,isolated}.log`, `.../control-summary.tsv` | Repository-vs-isolated build control. |
| `.../baseline-*.log`, `.../current-rebuild-*.log`, `.../baseline-focused-tests.log` | Correctness checks. |
| `.../harness/` | Test-only measured harness snapshot and reproduction instructions. |

At measurement time no staging, commit, push, amend, reset, rebase,
cherry-pick, or history rewrite occurred; the final publication is a separate
commit recorded outside this report.

## 30. 2026-11-14 SIMD Phase 1: AArch64 NEON quantization kernels

### 30.1 Question and verdict

Can the S3/S4 quantization and distortion kernels be vectorized with
bit-identical output? **Yes: +15.69% median end-to-end over eight
alternating pairs** (all pairs positive, all heavy workloads positive, all
fixture hashes unchanged), measured before this milestone's audit. The audit
below confirms scalar/NEON equivalence, adds index-clamping for exceptional
lanes, upgrades the differential tests with trial accounting and explicit
boundary cases, and makes the benchmark VBR mode explicit (VBR-5 default for
historical comparisons, VBR-6 opt-in). No further optimization was undertaken.

### 30.2 Implementation

Production change: `rust/src/aac_enc/quant_neon.rs` (new) plus dispatch in
`rust/src/aac_enc/quant.rs` (`rust/src/aac_enc.rs` registers the module).
Public behavior, tables, psychoacoustics, VBR parameters, rate control, and
APIs are unchanged; the scalar code is the unmodified behavioral reference.

- Four-line NEON kernels for `quantize_lines`,
  `calc_sfb_dist` (quantize + inverse + distortion), and
  `calc_sfb_quant_energy_and_dist` (inverse + energy + distortion).
- `sqdmulh` for the fixed-point products (operand bounds prove no
  saturation: quantizer path `|2·x·q'| ≤ 3.9e18 < 2^63-1; inverse path
  `2·s·t ≤ 8.7e18 < 2^63-1`; distortion/energy squares `< 2^63`). The single
  saturation divergence, `fpow2(INT_MIN)`, is routed to the scalar fallback
  by an explicit `inv == INT_MIN` guard.
- `clz` + variable `sshl` normalization with 5-bit masked shifts, matching
  the scalar wrapping-shift semantics (verified `>> negative` masks mod 32
  in release; the valid input envelope of `quant.rs` ("Valid input
  envelope") guarantees non-negative shifts in production).
- Lane loads for the 512-entry tables (9-bit indices always in bounds);
  exponent-row indices clamped to 13 so exceptional `|q| > MAX_QUANT` lanes
  load in-bounds before the `|q|` guard discards them via scalar fallback.
  `fmult` keeps bit 0 (ARM64 floor); only `fpow2` clears it.
- Zero fast paths and sign restoration are selects (`bsl`/`bic`), not
  branches. Early-out and prefix-write semantics are preserved: a violating
  group falls back to the scalar loop, which writes the exact scalar prefix
  and returns `None`; tails always run scalar. Energy/distortion sums are
  wrapping `i32` adds, associative mod 2^32.
- Non-AArch64 targets use the portable scalar path. No AVX2 implementation
  (investigated; x86-64 keeps the scalar fallback). No new dependencies.

### 30.3 Correctness and tests

- `quant` 10, `scf` 6, `production_baseline` 2 (debug + release); focused
  encoder suites (`quant`, `scf`, `bitcount`, `bitenc`, `frame`, `rate`,
  `production_baseline`, `encoder`) 96 passed, 0 failed, in both
  configurations; doctests 19 passed; `aac-convert` suites pass (debug +
  release). Full library suites: debug 357 passed + 11 failed, release 353
  passed + 11 failed — all 11 are the documented missing-artifact set
  (`AAC_DEC_BIN`, `PHASE1_ORACLE_DIR`, `PHASE1_C_DIR` unset), unrelated to
  this change.
- NEON differentials (AArch64, debug + release, fixed seeds): 5,000
  randomized trials per kernel (0 rejections, 615/26/3,771 early-outs
  across dist/matrix/energy), explicit boundary lengths
  0,1,2,3,4,5,6,7,8,9,12,15,16,17, per-position `i16::MIN`/`MAX_QUANT` guard
  tests, per-position single-violation prefix/tail tests with sentinel
  fill, and zero/edge-gain cases. Trial counts print as
  `NEON_TEST <name>: generated=… accepted=… rejected=… early_out=…`; any
  envelope rejection fails the test.
- Generated assembly (release test binary) shows the intended vector
  codegen in the production path (`sqdmulh.4s`, `abs.4s`, `clz.4s`,
  `sshl.4s`, `shrn.4h`, lane `ld1.h`/`ld1.s` gathers).
- Binary impact: `__text` 1,890,108 → 1,904,508 B (+14,400 B, +0.76%);
  no heap allocation; stack-resident constants only.

### 30.4 Benchmark harness VBR selection

The default `EncoderConfig` moved to VBR-6 after the §29 checkpoint while
the harness's `production` mode still validated against the VBR-5
`production_baseline` pins, so a default full-corpus run failed its hash
gate. Corrected without changing historical semantics:

- `AAC_PERF_VBR` selects the profile for `production` and `core`
  (`"5"` default, `"6"` opt-in; parsed via `VbrMode::from_str`).
- VBR-5 (`core` and `production`) validates every fixture against the
  committed VBR-5 bytes/hash pins, as in all prior reports.
- VBR-6 validates the pinned music fixture against its VBR-6 pin
  (256,763 bytes / `d6fc9d2c59cf5c36`, from
  `production_default_profile_vbr6_music_baseline_is_unchanged`) and checks
  ADTS framing plus cross-repetition byte/hash determinism elsewhere.
- `AACPERF_CONFIG` now carries `vbr=5|6`; the per-fixture `expected` column
  shows the mode-applicable pin. `feed` is mode-independent.
- Verified: full 20-workload VBR-5 run passes both modes with VBR-5 pins;
  full VBR-6 run passes with the music pin and determinism gates; an
  invalid `AAC_PERF_VBR` value fails loudly.

### 30.5 Performance evidence status

The +15.69% campaign (eight alternating old/new pairs, 15 reps, 19
fixtures + derived 40 s music, `core`/VBR-5) predates the §30.2 clamp and
the `as_chunks` refactor; a two-pair re-verification on the final tree
measured +15.92% / +16.44%, consistent with the campaign median, and all
heavy workloads improved with no meaningful regression (largest short-fixture
delta: silence −0.49%, run noise). The campaign remains the result of
record; the harness correction does not alter its `core`/VBR-5 comparison.
No VBR-6 speedup is claimed.

### 30.6 Files

| File | Change |
|---|---|
| `rust/src/aac_enc/quant_neon.rs` | New NEON kernels + differential tests + ignored microbenchmark. |
| `rust/src/aac_enc/quant.rs` | NEON dispatch; scalar helpers as the testable reference. |
| `rust/src/aac_enc.rs` | Module registration (`cfg aarch64`). |
| `rust/src/aac_enc/stream_driver.rs` | `AAC_PERF_VBR`, mode-applicable pins, determinism gates. |

No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred while preparing this section.
