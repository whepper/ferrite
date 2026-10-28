# AAC-LC VBR-5 encoder performance evidence

Diagnostic evidence for
[`docs/aac-enc-performance-baseline.md`](../../aac-enc-performance-baseline.md).
The baseline measurements in this directory were taken before the 2026-10-27
follow-up experiment. The follow-up changed only `quant::calc_sfb_dist` and
two equivalent table-index masks; its paired logs, assembly, and profile are
in [`calc-sfb-dist-2026-10-27/`](calc-sfb-dist-2026-10-27/). The
[`front-end-2026-10-27/`](front-end-2026-10-27/) directory is a diagnostic
profiling pass; its recommended experiment is recorded in
[`tns-scale-up-spectrum-2026-10-27/`](tns-scale-up-spectrum-2026-10-27/) and
was retained. [`code-values-2026-10-28/`](code-values-2026-10-28/) records the
later retained `BitWriter::write_bits` 8-byte-window change. No C oracle was
built and no reference artifact was regenerated.

## Method

1. Measured the public production `Encoder` API (`Encoder::new` + bounded
   `Encoder::push`/`flush`) in a normal `--release` build with the default
   `aac_enc` feature. Chunking is 8,000 sample frames per channel, matching
   `aac-convert::CHUNK_FRAMES`.
2. Ran one untimed warm-up pass, then 15 measured repetitions per fixture and
   mode. Each repetition constructs a fresh encoder and processes the fixture
   from the beginning. Fixture order is fixed; repetitions are separated by
   the other fixtures in the selected set.
3. Repeated the complete benchmark four times as independent runs to estimate
   run-to-run variance. The detailed table uses the final run; the variation
   table reports all four.
4. Collected output bytes inside the timed region (the normal caller
   behaviour), then validated ADTS framing and the committed
   `production_baseline` byte total and FNV-1a hash outside the timed region.
   The derived long workload is checked for byte/hash determinism across
   repetitions.
5. Repeated the four committed WAV fixtures and the derived 40 s workload in
   a separate `cargo run --release --example aac_bench_wav` build without
   `cfg(test)`, to bound test-harness overhead. The example uses the same
   public `Encoder` API and the same byte/hash pins.
6. Profiled separate opt-in runs with macOS `sample` at a 1 ms interval. The
   profiled binary is the release test binary; no source-line information is
   present because the release profile strips debuginfo. Attribution is
   function-level only.

## Workloads

The 19 committed `M58_FIXTURES` (15 synthetic + 3 licensed speech/music + the
Eroica excerpt) are used unchanged. One derived long workload,
`m7-music-stereo-48k-x4`, concatenates the committed music PCM four times in
memory (40.0 s stereo); it is benchmark-only and has no reference artifact.

Representative coverage:

| Category | Fixtures |
|---|---|
| Speech | `m7-speech-clean-stereo-48k`, `m7-speech-pauses-stereo-48k` (10 s stereo 48 kHz) |
| Music / tonal | `m7-music-stereo-48k`, `m8-music-eroica-stereo-48k` (10 s stereo 48 kHz), `m3-multitone-stereo-48k`, `m2-sine-stereo-48k`, `m3-sweep-stereo-48k`, `m3-quiet-sine-stereo-48k` |
| Transient / percussive | `m2-onset-stereo-48k`, `m2-click-mono-48k`, `m2-impulse-mono-48k`, `m3-onset-stereo-441k`, `m6-stress-stereo-48k` |
| Mono | `m2-click-mono-48k`, `m2-impulse-mono-48k`, `m2-edge-mono-48k`, `m3-noise-mono-48k` |
| 44.1 kHz | `m3-sine-stereo-441k`, `m3-onset-stereo-441k` |
| Long steady state | `m7-music-stereo-48k-x4` (40 s stereo, derived) |

## Files

| File | Contents |
|---|---|
| `benchmark-medians.tsv` | Final-run per-fixture medians (15 reps): inputs, duration, frames, bytes, hash, init, encode, total, RTF, realtime ×, samples/s. |
| `benchmark-runs.tsv` | Four independent production-run aggregates. |
| `benchmark-final-modes.tsv` | Final-run production/feed/core decomposition. |
| `benchmark-19-direct.log` | Direct 15-rep aggregate over the 19 committed fixtures (no derived workload); raw rows. |
| `benchmark-wav-production.tsv` | Production-example medians for the four committed WAVs + derived 40 s workload (two independent 15-rep runs). |
| `wav-bench-1.log`, `wav-bench-2.log` | Raw production-example runs. |
| `benchmark-fixture-spread.tsv` | Per-fixture production median total time in each of the four runs and the spread. |
| `benchmark-raw.log` | Raw final-run `AACPERF_*` rows, including every repetition. |
| `profile-totals.tsv` | `sample` leaf totals, idle/harness samples, and worker samples per workload. |
| `profile-categories.tsv` | Self-time category shares, normalised to worker samples. |
| `profile-top-self.tsv` | Top 20 self-time symbols per workload, normalised to worker samples. |
| `corpus.sample.txt` | Raw `sample` output for the 19-fixture aggregate profile. |
| `music.sample.txt` | Raw `sample` output for `m7-music-stereo-48k`. |
| `speech.sample.txt` | Raw `sample` output for `m7-speech-clean-stereo-48k`. |
| `onset.sample.txt` | Raw `sample` output for `m2-onset-stereo-48k`. |
| `noise.sample.txt` | Raw `sample` output for `m3-noise-mono-48k`. |
| `calc-sfb-dist-disassembly.txt` | Baseline ARM64 disassembly excerpt of `quant::calc_sfb_dist` (scalar loop; no NEON, four bounds-check panic paths). |
| `build-flags.txt` | Host, toolchain, profile and feature facts. |
| `calc-sfb-dist-2026-10-27/` | Follow-up optimization: source diff, paired A/B logs, production-example A/B, assembly before/after, profile before/after, summary TSVs. |
| `front-end-2026-10-27/` | Diagnostic front-end pass: current baseline, onset/music/speech/corpus profiles, symbol and assembly summaries, and the `scale_up_spectrum` microbenchmark; no production change. |
| `tns-scale-up-spectrum-2026-10-27/` | Retained `scale_up_spectrum` write-loop experiment: source diff, before/after assembly and profiles, eight paired test-build runs, eight paired production-example runs, hash/correctness checks. |
| `bit-path-2026-10-28/` | Current bit-path profile, `bit_count` cleanup attempt, codegen, four paired benchmarks, and the reverted/rejected disposition. |
| `code-values-2026-10-28/` | `code_values` profile and candidate analysis; retained 8-byte-window `write_bits` change, before/after codegen, eight paired test-build runs, four paired production-example runs, hash/suite verification. |
| `scalefactor-search-2026-10-28/` | Scalefactor-search profile, `assimilate_multiple_scf2` trial-copy hoist attempt, equivalence argument, codegen, four paired runs, scan follow-up, and the rejected/reverted disposition. |
| `quantize-spectrum-2026-10-28/` | `quantize_spectrum` profile, shared `quantize_line_body` sign-path unification, retained change, before/after codegen, eight paired test-build runs, four production-example runs, hash/suite verification. |
| `sfb-quant-energy-dist-2026-10-28/` | `calc_sfb_quant_energy_and_dist` profile, two-line-unroll attempt, equivalence argument, codegen, four paired runs, and the rejected/reverted disposition. |
| `form-factor-2026-10-28/` | `pe::form_factor` profile, sqrt/interpolation hot-path analysis, candidate rationale, and the no-change disposition. |
| `tns-autocorr-quarter-lags-2026-10-28/` | `tns::autocorr_quarter_lags` profile, lag-outer transpose attempt, equivalence argument, codegen, four paired runs, and the rejected/reverted disposition. |
| `source-line-profiling-2026-10-28/` | Optimized DWARF/dSYM workflow, production-vs-debug config comparison, source-line and inline attribution, ranked next targets. |
| `accumulate-dist-shift-specialization-2026-10-28/` | First `shift_int` call-site specialization attempt: equivalence proof, identical-codegen finding, corrected second-shift attribution, four paired runs, and the rejected/reverted disposition. |
| `find-max-merge-incremental-maximum-2026-10-28/` | `gm_stage2` candidate-list rewrite: measured section/merge topology, incremental-maximum proof, 20k-case differential test, codegen, four paired runs (+0.846% median); rejected at the time, then retained after the §27 eight-pair remeasurement (+0.594% median, 6/8 positive). |
| `calc-sfb-dist-attribution-2026-10-28/` | `quant.rs:0` resolution: three fresh captures, binary/dSYM identity, DWARF line-0 rows, per-region source mapping, path-frequency counters, and the per-PC attribution correction. |
| `scalefactor-search-call-path-2026-10-28/` | S4 call hierarchy, per-phase and per-workload counters, exact intra-pass duplicate evidence, LRU-size simulation, lookup-cost measurement, and the per-band cache hypothesis. |
| `calc-sfb-dist-lru8-cache-2026-10-28/` | Per-band LRU-8 cache candidate: key/lifetime proof, prefix-write handling, differential tests, measured 15–21% hit rates, 15 KB footprint, four paired runs (−0.66% median), and the rejected/reverted disposition. |
| `rejected-candidates-reassessment-2026-10-28/` | Seven-candidate inventory, prospective small-win decision framework, eight-pair `find_max_merge` remeasurement (+0.594% median, 6/8 positive), the retention decision, and the retained-tree validation logs. |
| `scf-trial-array-reuse-2026-10-28/` | §16 trial-array reuse re-tested on the retained-`find_max_merge` tree: exact patch, oracle/hash checks, eight paired runs (+0.246% median, 6/8 positive, −184 B), and the Tier 2 rejection. |
| `aggregate-performance-checkpoint-2026-10-28/` | Aggregate checkpoint of the complete retained set versus a reconstructed pre-optimization baseline: source definition, four reversed patches, three pre + eight paired + two post baseline runs, all 20 workload medians, 60-row hash comparison, and the +13.932% median result. |

## Definitions

- **Input samples** are per channel. Stereo fixtures contain two channels of
  the stated length.
- **Audio duration** = `input_samples_per_channel / sample_rate`.
- **RTF** (as requested) = encoding wall time / audio duration. Lower is
  faster.
- **Realtime ×** = audio duration / encoding wall time. Higher is faster.
- **Samples/s** = input samples per channel / encoding wall time.
- **Total** includes `Encoder::new`; **encode** is only `push`/`flush`.
  `Encoder::new` is reported separately and is ~15–25 µs per stream.
- **Production** is the public `Encoder` API. **Feed** is `PcmFeeder` only.
  **Core** is prebuilt feeder frames through the same per-frame clone and
  `StreamEncoder::step_frame` loop used by `Encoder::emit_frames`, excluding
  the feeder.

## Benchmark boundary and exclusions

- Timed: `Encoder::new` (reported separately) and `Encoder::push`/`flush`,
  including the production feeder, deinterleave, per-frame channel clones,
  QC/quantiser, bit counting, and ADTS frame assembly.
- Not timed: fixture synthesis/decoding, interleaving, output validation,
  hashing, disk I/O, and allocation of the result byte vector (pre-sized
  outside timing; per-frame byte copies are timed). `aac-convert`'s M4A
  mux/write is outside this benchmark; these numbers are encoder throughput,
  not full conversion throughput.
- No oracle comparison, no C encoder, and no reference decode run is in the
  timed path.
- The derived long workload is a concatenation of committed PCM. It is not a
  new fixture or reference artifact.
- The full 19-fixture benchmark runs in a `cfg(test)` build. The separate
  `aac_bench_wav` example (no `cfg(test)`) measured a **0.25–0.86%** advantage
  on the four committed WAVs, with identical bytes/hashes, so the 19-fixture
  aggregate is conservative by roughly that fraction. The synthetic fixtures
  have no test-free driver in this audit.

## Profiler limitations

- macOS `sample` provides leaf self-time and a call graph, but not source
  lines here (release `strip=debuginfo`). Some small functions may be inlined
  into callers, so a symbol's self time can include inlined work.
- `xctrace` is unavailable (full Xcode not installed); `dtrace` is blocked by
  System Integrity Protection; `perf` is not present. `sample` was the only
  usable sampling profiler.
- The test harness runs the encoder on a worker thread; the idle main thread
  and test-harness frames are excluded from the worker-normalised shares.
  `mach_absolute_time` sampling overhead is included in the harness bucket and
  excluded from worker shares.
- No CPU affinity is available on macOS. P/E-core placement and background
  load dominate run-to-run variance; the report uses medians, not fastest
  samples.
- Profiling runs use fresh encoder state per repetition and include
  `Encoder::new`/feeder overhead for short fixtures. The 19-fixture aggregate
  and the 10 s/40 s workloads are the steady-state references; the 1 s onset
  profile is a transient-path view and over-weights setup/feeder relative to a
  long stream.

## Correctness checks during benchmarking

The benchmark asserts, for every production and core run:

- ADTS sync and frame-length consistency;
- the expected frame count `ceil((samples + 1600) / 1024)`;
- the committed `production_baseline` byte total and FNV-1a ADTS hash for all
  19 baseline fixtures (exact hash enforced on macOS aarch64);
- the same committed bytes/hash in both the production and core paths;
- deterministic bytes/hash for the derived long workload across repetitions.

The committed `production_baseline` test is also run separately in release.
See the main report §8 for exact test commands and results.

## Reproduction

From `rust/`:

```sh
# Benchmark (opt-in; #[ignore] by default)
AAC_PERF_BENCH=1 AAC_PERF_REPS=15 AAC_PERF_LONG=1 \
  cargo test --release --locked --features aac_enc --lib \
  aac_enc::stream_driver::perf_tests::aac_perf_bench -- --ignored --nocapture

# Correctness gate
cargo test --release --locked --features aac_enc --lib \
  aac_enc::production_baseline::production_f32_corpus_baseline_is_unchanged

# Production-build check on the four committed WAVs + derived 40 s workload
cargo run --release --locked --features aac_enc --example aac_bench_wav
```

Profiling (single fixture, 300 repetitions, quiet rows):

```sh
BIN=$(cargo test --release --locked --features aac_enc --lib --no-run 2>&1 | \
  sed -n 's/.*Executable unittests src\/lib.rs (\(.*\))/\1/p')
AAC_PERF_BENCH=1 AAC_PERF_FIXTURES=music-stereo AAC_PERF_REPS=300 \
  AAC_PERF_MODES=production AAC_PERF_QUIET=1 \
  "$BIN" --ignored --nocapture aac_enc::stream_driver::perf_tests::aac_perf_bench &
PID=$!
sleep 1
sample "$PID" 8 1 -file music.sample.txt
wait "$PID"
```

The exact binary path embeds a Cargo hash and is machine/build-specific; use
the `--no-run` output rather than copying the path from this evidence set.
