# Ferrite MDCT precision experiment: f32 vs f64

Date: 2026-10-27. Status: **closed out; report and evidence retained; the
experimental f64 implementation was removed; production default unchanged.**
Recommendation: **retain f32**.

This is a controlled precision experiment on the forward MDCT only. It did
not propose a production refactor. The f64 path was feature-gated, test-only,
and never selected by the production constructor. After the experiment it was
removed rather than retained: its conclusions are fully recorded here, and the
implementation added maintenance cost without diagnostic value beyond this
one measured result. See
[`aac-enc-mdct-f64-closeout.md`](aac-enc-mdct-f64-closeout.md).

## 1. Scope and environment

| Item | Value |
|---|---|
| Repository HEAD at final validation | `e2ab63026e9dd5feb96c61ac908e18f9e85e7520` |
| Base commit at experiment start | `0abf11b` (`main`) |
| Rust | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1` |
| Hardware / OS | Apple M5, arm64, macOS 27.0.1 (26A434) |
| Reference encoder | FDK `aac-enc` SHA-256 `013e8acf…15e5f5` (exact pinned reference) |
| Reference decoder | FDK `aac-dec` SHA-256 `f361ba89…a21c0b8` (exact pinned reference) |
| Reference source | pinned `f14ad40a…1cc8a5`, Release, `-DBUILD_PROGRAMS=ON` |
| Encoder profile | AAC-LC, VBR-5, afterburner on, PNS off, ADTS, 1024 frames |
| Corpus | 19 fixtures: 15 synthetic, 2 speech, 2 music; mono/stereo; 44.1/48 kHz |

The worktree also contained concurrent, unrelated in-progress changes in
`frame.rs`, `pe.rs`, `phase1b.rs`, `psy.rs`, `rate.rs`, `sfb.rs`, `tns.rs`, and
`common/hybrid.rs`. They were not authored or modified by this experiment.
The f32 default path was re-verified against the retained baseline after they
appeared: all 19 f32 ADTS streams are byte-identical to the retained V3
baseline, so the differential f32/f64 comparison is valid on that coherent
tree (see §9 for the transient build state during final assembly).

## 2. Baseline architecture and numerical path

### 2.1 Complete path

1. **PCM**: interleaved `i16` → per-channel 1024-sample frames by `PcmFeeder`.
   No scaling at this stage.
2. **Block switching**: time-domain HP-energy attack detection on the PCM
   (`block_switch.rs`). It does not use the spectrum, so transform precision
   cannot change its decisions.
3. **Transform staging**: `ChannelTransform::process` (`transform.rs`) copies
   fresh samples 0..448 into ring positions 1600..2048, transforms, rotates
   the ring, and copies samples 448..1024 into 1024..1600. Constant delay:
   **1600 samples**.
4. **MDCT**: `common/mdct.rs::Mdct::mdct`, a line-for-line float port of FDK
   `mdct_block`: fold `(-D-Cr, A-Br)`, left slope from persistent previous
   frame, right slope explicit, `MDCT_FAC_FL = 2.0`, lengths 1024 (long/
   start/stop) or 8 × 128 (short), 448-sample short-block input offset.
5. **DCT-IV / FFT**: `common/dct.rs::dctiv` and `common/fft.rs`, both f32.
6. **Spectral output**: `[f32; 1024]`, widened to f64 once in
   `stream_driver.rs` before psychoacoustics.
7. **Psychoacoustics**: `psy.rs` is f64: lowpass zeroing, headroom/rescale,
   TNS detect/sync/encode, thresholds, PCM floor, pre-echo, spreading, M/S,
   short grouping, PE.
8. **Float → Q31**: `energy::map_spectrum_to_q31`:
   `round(v * 2^(31 - mdct_scale))`, clamp to `i32`. `mdct_scale` is the
   measured constant 12 (1024-line) or 9 (short), plus TNS shifts.
9. **Q31 backend** (`q31.rs`, `energy.rs`, `pe.rs`, `scf.rs`, `quant.rs`,
   `rate.rs`, `bitcount.rs`, `frame.rs`): log-domain energy/thresholds,
   scalefactors, quantization, bit counting, rate control, ADTS assembly.
   Unchanged by this experiment.

### 2.2 Types, scaling, and rounding

- Ring: `[f32; 2048]`; input `i16` maps to `x / 32768.0`.
- Window slopes: `Complex<f32>` tables (`window_tables.rs`), sine and KBD;
  512 pairs for 1024, 64 pairs for 128.
- DCT-IV pre/post twiddles: existing f32 window/sine tables.
- Output scale: fold gain 2.0, then unscaled DCT-IV.
- Q31 rounding: `f64::round` (half away from zero), saturation clamp.
- Audio output: no f32→i16 conversion in the encoder; output is a bitstream.

### 2.3 Was f32 deliberate?

There is no documented precision study selecting f32. The migration spec
(`docs/aac-enc-migration-spec.md` §4-5) records a deliberate **float-native**
decision versus FDK's Q31, but not an f32-versus-f64 choice. The f32 types are
inherited from the reused MDCT/DCT/FFT kernels (`transform.rs` explicitly
states that the shared kernel was reused rather than replaced). f32 is
therefore best described as an **intermediate implementation choice**, not an
AAC-LC requirement.

## 3. Experimental changes (historical; removed at closeout)

These changes existed during the experiment and were removed at closeout. They
are recorded so the measurements can be reproduced from this report/evidence,
not because the code is retained.

| File | Change | Disposition |
|---|---|---|
| `rust/Cargo.toml` | Non-default feature `transform-f64-experiment = ["aac_enc"]`. | Removed. |
| `rust/src/aac_enc.rs` | Feature-gated `transform64` and M9 test modules. | Removed. |
| `rust/src/aac_enc/transform64.rs` | Isolated f64 ring, MDCT fold, DCT-IV, radix-2 FFT. | Removed. |
| `rust/src/aac_enc/stream_driver.rs` | Compile-time transform backend selector (default f32); feature-gated diagnostic capture. | Removed; default path restored. |
| `rust/src/aac_enc/m9_f64_experiment.rs` | Full-corpus comparison, oracle, and performance tests. | Removed; key outputs retained in `docs/evidence/aac-enc-mdct-f64/`. |

Design:

- The production `transform.rs`, `common/mdct.rs`, `common/dct.rs`,
  `common/fft.rs`, window tables, psychoacoustics, Q31 backend, quantization,
  rate control, and bitstream code are **untouched**.
- `TransformState` contains only the f32 backend in default builds. The f64
  variant exists only under the feature and is selected only by
  `StreamEncoder::new_experimental`, used by the M9 tests.
- f64 is used for the PCM ring, window multiplication, folding, accumulation,
  DCT-IV, FFT, and spectral output.
- **Coefficient policy:** AAC window slopes and DCT-IV pre/post twiddles are
  the repository's existing f32 tables, widened exactly. Their values are
  unchanged, so the experiment isolates arithmetic precision.
- The internal FFT uses f64 twiddles from the same DFT angles. That is an
  internal implementation change, not a change to the AAC transform
  definition. It is the only coefficient-level difference.
- No Q31, psychoacoustic, TNS, M/S, quantization, rate-control, ADTS, or
  flush semantics were changed.

## 4. Commands and regression results

Baseline before changes, with the regenerated oracle and retained C artifacts:

```sh
cd rust
PHASE1_ORACLE_DIR=<regenerated dumps> \
PHASE1_C_DIR=<retained C ADTS> \
AAC_DEC_BIN=<pinned aac-dec> \
M55_C_DIR=<retained m55 ADTS> \
cargo test --locked --features aac_enc --lib
# 334 passed, 0 failed
```

Without external artifacts, the same suite reports 324 passed / 10 failed:
the 10 are the documented reference-artifact tests, not code failures.

During the experiment (historical):

| Command | Result |
|---|---|
| `cargo test --locked --features aac_enc --lib` (with artifacts) | **334/334 pass** |
| `cargo test --locked --features aac_enc,transform-f64-experiment --lib` (with artifacts) | **345/345 pass** |
| `cargo test --locked -p aac-convert` | pass |
| `cargo test --locked --features aac_enc --doc` | 19/19 pass |
| `cargo build --locked --no-default-features --features aac_enc` | pass |
| `cargo build --locked --no-default-features --features transform-f64-experiment` | pass |
| `rust/tools/check-licensing.sh` | pass |
| `git diff --check` (scoped to experiment files) | clean |
| debug vs release f64 ADTS | 19/19 identical |

After closeout (current tree, f64 code removed, production-baseline test
added): `cargo test --locked --features aac_enc --lib` with reference
artifacts gives **335/335 pass**. Without external artifacts, the same suite
gives 325 pass / 10 documented reference-artifact failures. See the
[closeout report](aac-enc-mdct-f64-closeout.md).

Experiment-specific runs (historical; the feature and M9 tests no longer
exist after closeout):

```sh
# These commands were used during the experiment and are retained for the
# record. They are not runnable on the closed-out tree.
F64_EXPERIMENT_OUT=<work> F64_BASELINE_ADTS=<baseline-adts> \
  cargo test --locked --features aac_enc,transform-f64-experiment --lib \
  m9_f64_encode_comparison -- --nocapture

F64_EXPERIMENT_OUT=<work> PHASE1_ORACLE_DIR=<oracle-dir> \
  cargo test --locked --features aac_enc,transform-f64-experiment --lib \
  m9_f64_oracle_full_corpus -- --nocapture

F64_EXPERIMENT_OUT=<work> F64_PERF_MODE=f32 F64_PERF_REPS=5 \
  cargo test --release --locked --features aac_enc,transform-f64-experiment \
  --lib m9_f64_performance -- --exact --nocapture
```

The retained production regression is
`aac_enc::production_baseline::production_f32_corpus_baseline_is_unchanged`;
it runs in the ordinary `--features aac_enc` suite and pins all 19 corpus byte
lengths on every platform, plus FNV-1a hashes on the recorded reference
platform (Apple Silicon macOS).

`cargo fmt --all -- --check` still reports pre-existing formatting diffs in
unrelated files (`m5a_decode.rs`, `stream_driver.rs` test code, others). The
new files are rustfmt-clean.

**Baseline reproduction:** the current default f32 build reproduces the
retained V3 corpus **byte-for-byte** (19/19 inputs, ADTS, and decoded WAVs;
`baseline_reproduced=true`). This is the proof that the experiment did not
change the production default.

## 5. Transform-level numerical comparison

All 19 fixtures, every frame, f32 and f64 in lockstep: **4,786
frame-channel pairs**.

### 5.1 f32 versus f64 (same inputs, same tables)

| Metric | Value |
|---|---:|
| Mean per-frame spectral RMS / frame peak | **7.225e-9** |
| Maximum per-frame RMS / peak | **4.297e-8** |
| Global spectral RMS (stored scale) | 6.968e-7 on a global peak of 450 |
| Silence | exactly zero, both paths |
| NaN / infinity / overflow | none |
| Determinism | bit-identical on repeat (40-frame and full-stream tests) |

Error by frequency region (per-bin mean absolute / mean relative to signal):

| Region | Mean abs per bin | Mean rel |
|---|---:|---:|
| 0–6 kHz | 3.638e-7 | 1.707e-7 |
| 6–12 kHz | 3.181e-7 | 1.168e-6 |
| 12–18 kHz | 3.104e-7 | 1.751e-6 |
| 18–24 kHz | 3.211e-7 | 2.235e-6 |

By signal type (maximum frame RMS / peak):

| Type | Max frame RMS/peak |
|---|---:|
| Noise | 4.297e-8 |
| Speech | 2.137e-8 |
| Transient | 1.744e-8 |
| Music | 1.420e-8 |
| Full-scale (`m6-stress`) | 1.410e-8 |
| Tonal | 1.363e-8 |
| Quiet sine | 7.613e-9 |
| Silence | 0 |

Quiet-sine spectral RMS is 6.70e-9 absolute; stress-fixture RMS is 2.12e-6
absolute at peak 368.8, with no saturation.

### 5.2 Against an ideal f64 MDCT

Same input, ideal ISO MDCT reference:

| Path | Worst relative error |
|---|---:|
| Production f32 kernel | 3.757e-7 |
| Experimental f64 kernel | 1.199e-7 |

f64 is ~3.1× closer to the ideal arithmetic result. Both are far below the
f32 coefficient-table error against FDK (see below).

DCT-IV/FFT unit checks:

| Check | Result |
|---|---:|
| f64 FFT vs naive DFT (n=16/64/512) | max 1.97e-12 |
| f64 DCT-IV vs naive f64 DCT-IV | rel 3.33e-8 (table-limited) |
| f64 DCT-IV vs production f32 DCT-IV | max 4.63e-6 at peak 33.1 |

### 5.3 Against the FDK C oracle

Regenerated oracle dumps, all frames with matching block-switch decisions:
**4,521 frame-channel pairs / 4,629,504 bins**. Reference read as
`int * 2^(E-31)`; no fudge factor.

| Metric | f32 | f64 |
|---|---:|---:|
| Worst absolute spectral deviation | 1.034e-2 | 1.035e-2 |
| Worst relative deviation / frame peak | 2.488e-3 | 3.902e-5 |
| RMS absolute deviation (stored scale) | 1.674e-4 | 1.675e-4 |
| Frames over the production absolute bound (`5e-4 * max(peak,1)`) | **0** | **0** |
| Bins closer to oracle | 2,331,110 | 2,294,042 |
| Median strong-bin ratio | — | 1.0000072 |
| Min / max strong-bin ratio | — | 0.984760 / 1.014257 |

Interpretation:

- Both paths satisfy the production absolute oracle tolerance on every frame.
- f64 reduces the *relative* worst case but **not** the absolute worst case
  or RMS agreement. The f32 and f64 absolute worst values differ by 0.1%.
- The residual against FDK is therefore **dominated by the shared f32 window/
  twiddle tables and the Q31 oracle quantization**, not by f32 accumulation.
  The direct f32↔f64 spectral RMS (6.97e-7 stored units) is ~240× smaller
  than either path's RMS deviation from FDK (1.67e-4 stored units).
- f64 is not a meaningful accuracy upgrade against the reference for the
  current coefficient tables.

Window/overlap behavior: all f32/f64 sequence and shape decisions were
identical; silence is exact; zero-reference frames stay zero. Six pre-existing
block-switch sequence mismatches vs the oracle exist in `speech-clean`; they
are also present in the f32 baseline and are unrelated to transform precision.

## 6. Downstream encoder behavior

Full 19-fixture encode with identical configuration: **2,442 frames**,
**800,967 bytes f32 / 800,965 bytes f64** (−2 bytes, −0.0002%).

| Metric | Result |
|---|---:|
| Frame lengths exactly equal | 2,425 / 2,442 (99.30%) |
| Frame lengths differing | 17 |
| Maximum frame-length delta | 5 bytes |
| Total absolute frame-length delta | 26 bytes |
| Whole ADTS streams byte-identical | 10 / 19 |
| Encoder notes / crashes | 0 / 0 |
| Frame-count law (feeder/flush) | all pass |

### 6.1 Q31 and psychoacoustic operands

| Operand | Difference |
|---|---|
| Q31 bins differing | 3,417,011 / 4,924,416 (69.39%) |
| Mean absolute Q31 difference | 4.37 LSB |
| Maximum Q31 difference | 379 LSB |
| Bins > 1 / > 16 / > 256 LSB | 2,846,764 / 215,763 / 42 |
| Q31 saturation | 0 f32, 0 f64 |
| Non-finite spectra | 0 f32, 0 f64 |
| `mdct_scale` mismatches | 0 |
| SFB energy (`e_stored`) differing bands | 32,900 (max 3,584) |
| Threshold (`t_stored`) differing bands | 375 (max 8) |
| Spread energy differing bands | 33,058 (max 3,584) |
| Energy Ld differing values | 117,887 (max 2,012,319) |
| Threshold Ld differing values | 375 (max 1,698,730) |

### 6.2 Decision differences

| Decision field | Frames differing |
|---|---:|
| PE | 42 |
| `used_dyn` | 31 |
| `huff_bits` | 31 |
| bit `align` | 30 |
| frame `total` bits | 17 |
| `scf_bits` | 10 |
| `tot_fill` | 4 |
| `side_bits` | 3 |
| section codebooks | 3 |
| TNS info | 3 |
| MS mask ones | 2 |
| gains | 2 |

Named flips:

- `m3-sine-stereo-441k` f17: gains `[-55, 0]` → `[-58, 0]`.
- `m7-speech-pauses-stereo-48k` f423: gains `[-29, 0]` → `[-28, 0]`.
- `m3-sine-stereo-441k` and `m3-onset-stereo-441k`, f27: MS mask ones 26 → 25.
- `m7-speech-pauses-stereo-48k` f109, f253 and `m7-speech-clean-stereo-48k`
  f380: TNS decisions changed (prediction-gain boundary).
- Section codebook segmentation changed in 3 frames
  (`m7-music` f7, `m7-speech-pauses` f286/f423).

These are boundary flips caused by Q31 differences of a few LSB to a few
hundred LSB; the downstream logic is unchanged. They are expected precision
consequences, not structural defects: no new failures, no saturation, no
frame-count changes, and every stream decodes. Block-switch `seq` never
differed, and `mdct_scale`/`max_common` never differed.

## 7. Decoded quality

Decoded with the exact pinned FDK decoder; all outputs independently decode
with system `ffmpeg` as a second decoder.

| Check | Result |
|---|---|
| Decode failures | 0 f32, 0 f64 |
| Frame-count mismatches vs C | 0 f32, 0 f64 |
| Decoded lengths | equal to C on all 19 |
| Clipped samples | 22 f32 / 22 f64 (all `m6-stress`; unchanged) |
| ADTS validity | 19/19 both; `ffmpeg` and FDK decode |

Decoded f64 vs decoded f32:

- 9 / 18 measurable streams are **bit-identical**.
- The 9 differing streams have waveform SNR **66.82–101.46 dB**
  (RMS difference 0.04–1.63 LSB; max sample difference 1–126 LSB).
- 4,096-sample windowed SNR min/median is unchanged to two decimals on every
  fixture.

Full-band SNR versus the original (higher is better; descriptive only):

| Metric | f32 | f64 | f64 − f32 |
|---|---:|---:|---:|
| Gated aggregate (15 fixtures) | — | — | min −0.001, median 0.000, max +0.003 dB |
| All measurable (18 fixtures) | — | — | min −0.001, median 0.000, max +0.003 dB |
| f64 − C (all 18) | — | — | min −0.472, median −0.000, max +1.267 dB |

Selected fixtures:

| Fixture | SNR f32 | SNR f64 | f64−f32 | f64−C |
|---|---:|---:|---:|---:|
| m2-sine-stereo-48k | 56.911 | 56.911 | +0.000 | +0.322 |
| m2-onset-stereo-48k | 58.102 | 58.105 | +0.003 | +0.484 |
| m3-sweep-stereo-48k | 43.335 | 43.335 | −0.000 | −0.472 |
| m6-stress-stereo-48k | 10.316 | 10.316 | +0.000 | +0.001 |
| m7-music-stereo-48k | 37.530 | 37.530 | −0.000 | −0.185 |
| m7-speech-clean-stereo-48k | 41.126 | 41.126 | +0.000 | +0.029 |
| m8-music-eroica-stereo-48k | 30.930 | 30.929 | −0.000 | −0.053 |

Conclusion: **no measurable decoded-quality improvement** and no material
regression. The f64 change does not close the gap to the C encoder.

Limitations of this quality evidence:

- Only waveform SNR / windowed SNR and clipping are measured. No perceptual
  metric (PEAQ, ViSQOL, etc.) or listening test was available; no ABX claim
  is made.
- SNR is not a perceptual verdict, especially for noise-like or transient
  material.
- The C reference is fixed-point and its control decisions differ anyway; this
  is a bitstream-level comparison at the same VBR-5/afterburner settings, not
  a source-identical implementation comparison.
- 44.1 kHz and `m8` remain exploratory under the repository's established
  eligibility rules; `m2-silence` has no defined SNR.

## 8. Performance and memory

Final-tree release measurements, encoder-only, in-memory PCM, 19 fixtures
(51.29 s audio, 2,442 frames), warm-up + 5 timed passes:

| Mode | Total wall | RTF | Relative |
|---|---:|---:|---:|
| f32 | 0.2255 s (median of 2 processes) | 227.4× | baseline |
| f64 | 0.2572 s (median of 2 processes) | 199.4× | **+14.0%** |

Earlier independent 3 × 5-rep runs on the same binary measured **+14.9–15.0%**.
Treat the slowdown as **~14–15%** (1.14–1.15×), not a precise single figure.

Representative per-fixture slowdowns (earlier 7-rep run):

| Fixture | f32 | f64 | Δ |
|---|---:|---:|---:|
| mono click | 0.866 ms | 1.023 ms | +18.1% |
| mono noise | 1.263 ms | 1.347 ms | +6.7% |
| stereo stress | 1.774 ms | 2.139 ms | +20.6% |
| stereo 44.1k onset | 1.780 ms | 2.385 ms | +34.0% |
| stereo music | 60.46 ms | 66.07 ms | +9.3% |
| stereo speech pauses | 38.57 ms | 44.53 ms | +15.4% |
| stereo Eroica | 56.21 ms | 62.35 ms | +10.9% |
| stereo silence | 1.947 ms | 2.968 ms | +52.4% |

Memory:

- Process max RSS: f32 24.20–24.85 MB, f64 24.07–24.26 MB. The observed
  difference is within run-to-run noise; there is no material process RSS
  increase.
- Steady-state analytical delta: ring 2 × 2048 × 4 B = 16 KiB (f32) versus
  2 × 2048 × 8 B = 32 KiB (f64), i.e. **+16 KiB for stereo**.
- `dctiv64` uses a stack `ArrayVec<Complex<f64>, 512>`; it adds no per-frame
  heap allocation. Production heap behavior is unchanged.

## 9. Failures, limitations, and unresolved discrepancies

- No test failure, decode failure, panic, NaN/Inf, Q31 saturation, or frame
  count/length regression was introduced by the f64 path.
- Six pre-existing block-switch sequence mismatches vs the oracle in
  `speech-clean` are present in both paths; they are outside this experiment.
  The closeout promotes them to the highest-value follow-up: see
  [`aac-enc-mdct-f64-closeout.md`](aac-enc-mdct-f64-closeout.md) §6.
- The worktree contains unrelated concurrent changes (listed in §1). The
  final default f32 corpus reproduction is byte-identical to the retained
  baseline, so they do not invalidate the differential result. Concurrent
  edits continued during final assembly; `frame.rs` was transiently
  uncompilable at one point and was then repaired by that concurrent work.
  Final re-runs on the coherent tree passed **345/345** (feature build) and
  **334/334** (default build).
- The f64 path widens f32 window/twiddle tables. It does **not** test f64
  window tables, f64 KBD generation, 24-bit PCM, or other sample rates. Those
  are the natural next experiments if precision is to be pursued.
- The f64 path uses an iterative radix-2 FFT, not a type-generic copy of the
  f32 mixed-radix kernel. The measured f32↔f64 delta therefore includes FFT
  implementation/rounding differences in addition to precision. Both implement
  the same DFT and are separately validated against naive DFT/DCT-IV
  references; the oracle conclusion (coefficient-table residual dominates) is
  unaffected.
- No perceptual metric or listening test was available; no audibility claim is
  made.
- The f64 experiment's ADTS files are new, so the repository metrics tool
  labels 9 fixtures "exploratory" on stream-provenance grounds even though
  they decode cleanly. That label is provenance, not a quality result.

## 10. Recommendation

**Retain the existing f32 transform.**

Evidence:

1. Decoded quality is unchanged: 9/18 streams bit-identical; the rest differ
   at 66.8–101.5 dB waveform SNR; full-band SNR delta is at most ±0.003 dB.
2. Agreement with the FDK oracle is unchanged in absolute/RMS terms
   (1.035e-2 vs 1.034e-2 absolute worst; 1.675e-4 vs 1.674e-4 RMS absolute).
   The shared f32 coefficient tables, not f32 accumulation, dominate the
   residual.
3. f32 already meets the production absolute oracle bound on all 4,521
   compared frames.
4. f64 costs ~14–15% encode time for no measurable benefit.
5. The observed decision flips (2 gains, 2 MS masks, 3 TNS, 3 sectioning
   frames) are expected boundary effects, not improvements.

Revisit only if the project later adds higher-precision window/twiddle tables
or a 24-bit/high-resolution PCM path, where transform arithmetic could become
the limiting error term. Such a follow-up should include f64 coefficient
tables, perceptual metrics, and a broader corpus before any production change.

## 11. Production readiness

The experimental code was **not production-ready**:

- It was isolated behind a non-default Cargo feature and a test-only harness.
- It had no public API, no fuzzing/soak testing, and no cross-platform
  performance validation.
- It was functionally safe on this corpus and useful as a reference experiment,
  but it should not have been merged as the production default.

It was removed at closeout rather than retained. No commit or push was
performed.

## 12. Evidence locations

The key derived evidence is retained in the repository under
[`docs/evidence/aac-enc-mdct-f64/`](evidence/aac-enc-mdct-f64/README.md):
per-fixture downstream comparison, oracle comparison, decoded quality, and
performance. The full raw artifacts remain in
`/private/var/folders/*/T/opencode/f64-mdct-experiment/`:

| Artifact | Contents |
|---|---|
| `transform64-tests.log` | transform/DCT/FFT unit and corpus-delta logs |
| `oracle-full.log`, `oracle-full.tsv` | full-corpus FDK MDCT comparison |
| `summary.txt`, `comparison.tsv`, `decision-diffs.tsv` | downstream encode comparison |
| `f32-adts/`, `f64-adts/` | generated ADTS streams + SHA-256 lists |
| `metrics-f32/`, `metrics-f64/` | repository metrics reports and JSON |
| `quality-comparison.md`, `quality-comparison.json` | decoded f32/f64/C comparison |
| `perf-summary.txt`, `perf-f32.tsv`, `perf-f64.tsv` | timing and RSS |
| `oracle/` | regenerated `enc_oracle` dumps for all 19 fixtures |
