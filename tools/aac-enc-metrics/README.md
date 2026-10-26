# AAC-LC VBR-5 objective metrics

Measurement-only tooling for the C-vs-Rust AAC-LC VBR-5 corpus. This is the
maintained successor to the scratch V3 `stage1_metrics.py`.

It does **not**:

- claim perceptual equivalence, transparency, or audibility;
- run ABX or listening tests;
- apply dB/quality pass/fail thresholds;
- implement per-frame or per-band localization (reserved for F04-B2);
- change encoder or decoder code.

## Files

| File | Purpose |
|---|---|
| `vbr5_metrics.py` | Parameterized metrics runner, alignment policy, report and JSON writer |
| `vbr5_localize.py` | F04-B2 diagnostic error localization: per-window/region time localization, optional spectral band energies, tail diagnostics (consumes B1 outputs; changes no B1 definition) |
| `fixtures-v3.json` | Complete 19-fixture V3 corpus manifest plus the separate mono/44.1 kHz probe |
| `test_vbr5_metrics.py` | Targeted alignment, delay-accounting, alias, parser, provenance, baseline, and manifest tests |
| `test_vbr5_localize.py` | Focused B2 tests: windowing, regions, spectral accounting, tails, determinism |

## Prerequisites

- Python 3.11+ with the standard library only (no numpy/audioop).
- The repository's `tools/enc-oracle/make_fixtures.py` for the 15 synthetic WAVs.
- The exact C reference binaries and FDK library recorded below, or a
  pre-generated C ADTS directory plus a retained V3 artifact set for a
  verified/gated run. Without a retained per-file match, pre-generated ADTS is
  exploratory.
- The Rust test harness; the runner invokes
  `m58_corpus_validation` unless `--rust-adts-dir` is supplied.

Recorded V3 reference identity:

| Item | Value |
|---|---|
| C source commit | `f14ad40a574a4132df947d15e9fb14bee91cc8a5` |
| C source parent | `da00af1ca9624a213fb53c9ac52f6fec55e9b2af` |
| C build | Release, `-DBUILD_PROGRAMS=ON -DCMAKE_BUILD_TYPE=Release` |
| C encoder SHA-256 | `013e8acf2571486a008c0ece538489e36d7e796b84a6f927418e66fb2d15e5f5` |
| C decoder SHA-256 | `f361ba89d372cb1b1a9175e673e6969ac6fc632428202cab02a31b3e3a21c0b8` |
| FDK library SHA-256 | `a94ede263c3994abb2a6f0582d11b0346b8f76704219deed9f04bf0fb7d51a8e` |
| Rust baseline commit | `3878c45a8b9d85956bf1b62b17897f94c41b2d6a` |

The tool checks the C encoder/decoder hashes against these values by default.
If a sibling `libfdk-aac.2.0.4.dylib`, a resolved `@rpath` library, or `--c-lib`
is available, it checks that hash too. It also checks `--c-ref-dir` at `HEAD`
against the recorded C source commit and records whether tracked source files
are modified. A mismatch fails with an actionable message unless
`--allow-different-reference` is passed. That option is an explicit override:
it forces every result to remain exploratory even when the supplied reference
happens to match the recorded hashes.

If a supplied C binary does not match the recorded identity, the tool fails
rather than silently using another build. It does not fetch secrets or private
repositories.

## One-command reproduction

From the repository root, using the retained V3 reference build:

```sh
V3="${TMPDIR}/v3-full-rvehNNkF" && \
python3 tools/aac-enc-metrics/vbr5_metrics.py \
  --manifest tools/aac-enc-metrics/fixtures-v3.json \
  --work-dir "${TMPDIR}/f04b1-baseline" \
  --c-enc-bin "$V3/build/ref-build/aac-enc" \
  --c-dec-bin "$V3/build/ref-build/aac-dec" \
  --c-ref-dir "$V3/build/ref" \
  --v3-reference-dir "$V3"
```

Outputs:

- `${TMPDIR}/f04b1-baseline/report.md`
- `${TMPDIR}/f04b1-baseline/metrics.json`

`--v3-reference-dir` is optional. When supplied, the tool compares input,
ADTS and decoded-WAV hashes against the retained V3 artifacts. Missing
historical files are reported as unavailable; they are not recreated.
`baseline_reproduced` is set to `true` only when all 19 corpus fixtures are
selected and all five artifact classes match 19/19. A subset run reports
`baseline_reproduced=null` and `baseline_reproduced_for_selection` instead,
so a one-fixture comparison is never labelled full-baseline reproduction.

If the retained V3 binaries are gone, build the exact reference revision and
point `--c-enc-bin`, `--c-dec-bin`, and `--c-ref-dir` at that build. The
recorded source/toolchain is in `fixtures-v3.json` and `README` above.

Targeted and pre-generated modes:

```sh
# Subset only (still runs the full 19-fixture Rust harness if no --rust-adts-dir)
python3 tools/aac-enc-metrics/vbr5_metrics.py ... --only m2-click-mono-48k,m2-onset-stereo-48k

# Reuse retained ADTS instead of re-encoding (useful for alignment/metric tests)
python3 tools/aac-enc-metrics/vbr5_metrics.py ... \
  --rust-adts-dir "$V3/rust-adts" --c-adts-dir "$V3/c-adts"
```

`--c-adts-dir` and `--rust-adts-dir` are accepted for targeted measurement,
but a supplied directory is not proof of provenance. A pre-generated stream is
gated only when its file matches the retained V3 artifact for that fixture
(and the C decoder is exact). Otherwise the affected fixture is exploratory,
even if an exact encoder binary was supplied but not run. The same rule applies
to a later Rust revision: it is exploratory unless its generated ADTS matches
the retained V3 artifact for the fixture. The `--v3-reference-dir` contents are
trusted by the caller; the tool verifies byte equality against them, not the
historical authenticity of that directory. `--allow-different-reference` is only
for a clearly labelled exploratory comparison against a rebuilt reference; it
never turns a non-recorded reference or an unverified stream into a gated
result.

## Manifest and eligibility

The manifest contains:

- 15 synthetic fixtures, including `m6-stress-stereo-48k`;
- 3 licensed WAVs with existing provenance (`m7-*`);
- the `m8` fixture marked `provisional`;
- one separate mono/44.1 kHz probe (`m3-onset-mono-441k`) marked
  `corpus_member: false`.

Every entry records full input SHA-256, category, provenance status/source,
license, sample rate, channel count, sample count, duration, input source,
alignment gate, and static eligibility.

Eligibility is an evidence scope, not a quality score:

| Scope | Meaning |
|---|---|
| `gated` | 48 kHz, approved/deterministic fixture provenance, verified Rust and C stream provenance, plausible decoded length, unambiguous per-channel alignment |
| `exploratory` | Measured, but outside the 44.1 kHz gate, provisional m8, unverified pre-generated ADTS, a later Rust revision without a retained match, an explicit reference override, or an unavailable decoded-length check |
| `excluded` | Structurally measured, but alignment is ambiguous/disagreeing or the decoded length is implausible, so no quality metrics are computed |
| `separate` | Manifest-identified probe not part of the 19-fixture V3 corpus; not measured by this command |

The 44.1 kHz fixtures remain **exploratory**. The benchmark plan's separate
44.1 kHz alignment gate is not silently declared satisfied. The `m8` fixture
is **exploratory** because its provenance is `PROVISIONAL`, not owner-approved.
The separate mono probe's declared hash is checked when its input is available
(via `${TMPDIR}/m5b-quality/inputs/onset-mono-44100.orig.wav` or
`--separate-probe-input`); an unavailable probe is reported as unavailable and
is never regenerated or substituted.

## Alignment policy

Positive lag means `decoded[lag:]` aligns with `original[0:]`; this is the
explicit convention used in the JSON. (V3 prose called the same alignment
“−1600”; this tool avoids the sign ambiguity.)

For each implementation and each channel independently:

1. Search integer lags in `[nominal − 32, nominal + 32]`, with
   `nominal = 1600`, using normalized cross-correlation.
2. Use stride 1 for signals ≤ 48,000 samples and stride 8 for longer signals.
   Refine the best coarse candidate at full rate over ±2 samples for short
   signals and ±4 samples for long signals; ±4 covers the whole stride-8 cell.
   A lag is scored only when its overlap has at least 1,000 samples (all corpus
   fixtures exceed this by a wide margin).
3. Record the per-channel estimate, best/second-best scores, score margin,
   coarse step, effective refine half-width, search range, and status.
4. For stereo, require the two channel estimates to agree within ±1 sample.
5. Select nominal 1600 when both estimates are within ±1 of nominal;
   otherwise select the joint best candidate. Never average conflicting
   estimates and never silently use channel 0.
6. Exclude from quality metrics on `no_signal`, `search_boundary`,
   `score_tie`, `channel_lag_disagreement`, or
   `outside_nominal_tolerance`.

The decoder's `outputDelay` (1744 at 48 kHz, 1685 at 44.1 kHz) is recorded
but **not** subtracted again. Only the selected alignment lag is applied to
the decoded WAV.

No AAC-frame-to-source-sample causal mapping is used.

## Measurements and units

Encoded/framing:

- `total_bytes`, `frame_count`, `header_bytes`, `payload_bytes`
- `frame_length_bytes` (`min_bytes`, `p50_bytes`, `p95_bytes`, `max_bytes`,
  `mean_bytes`)
- `bitrate_kbps_total`, `bitrate_kbps_payload` over original fixture duration

Decoded/full-band:

- `snr_db`
- `sdr_db` — **alias of `snr_db`**, with `sdr_db_is_alias: true`
- `snr_status` (`defined`, `identical`, `undefined_silence`)
- `rms_error_lsb`, `peak_error_lsb`
- `signal_rms_lsb`, `signal_peak_lsb`
- `overlap_samples_per_channel`, `overlap_samples_total`
- `reference_tail_shortfall_samples_per_channel`,
  `test_tail_shortfall_samples_per_channel`
- per-channel and aggregate error blocks

Decoded length plausibility:

- `length_check` per implementation compares decoded samples with
  `frame_count × 1024 − outputDelay` from the manifest. The retained FDK CLI
  satisfies this identity exactly for all 38 corpus streams. The check is exact
  for that decoder; an unknown rate is reported as `unavailable`, not given an
  invented tolerance. An implausible length excludes quality metrics.

Structural:

- decode exit code, empty-stderr flag, WAV sample rate/channels/bits, decoded
  samples, decoded WAV SHA-256, clipped-sample count.

The JSON deliberately has no per-frame/per-band fields. Frame-size
distribution is encoded-framing data, not decoded-audio quality.

## Tests

```sh
python3 -m unittest discover -s tools/aac-enc-metrics -p 'test_*.py' -v
python3 tools/aac-enc-metrics/vbr5_metrics.py \
  --validate-manifest \
  --manifest tools/aac-enc-metrics/fixtures-v3.json \
  --work-dir "${TMPDIR}/f04b1-manifest-check"
```

The alignment tests cover unique stereo alignment, unique mono, disagreeing
channels, silence/no-signal, lower and upper search-boundary ambiguity, score
ties, lag outside the nominal tolerance, and the long-signal stride-8
refinement interval. The metric tests cover single-lag delay accounting, the
SNR/SDR alias, tail shortfall, decoded-length plausibility, WAV/ADTS parsing,
and clipping counts. Provenance tests cover pre-generated C/Rust ADTS without
a trusted match, the retained-V3 match path, current-run encoder verification,
later Rust revisions, and the explicit override. Baseline tests distinguish a
one-fixture selection from full 19-fixture reproduction. Manifest tests check
the 19+1 split, 15 synthetic/3 licensed requirements, m8 provisional status,
44.1 kHz eligibility, committed WAV hashes, the refinement declaration, and
separate-probe hash validation.

## Remaining gaps after F04-B2

B2 (see below) covers per-window/per-region decoded-error localization,
diagnostic spectral band energies, and tail/termination diagnostics.

Still open:

- decoded-audio comparison against a second independent decoder;
- broader real/44.1 kHz/true-stereo coverage;
- any perceptual or listening evidence (outside this tool's scope).

## F04-B2 error localization (`vbr5_localize.py`)

Diagnostic-only follow-up to B1. It localizes Rust-vs-C decoded-audio
differences in time and (optionally) frequency. It applies no thresholds,
claims no perceptual equivalence or audibility, and computes no single
quality score. It changes no encoder/decoder code and no B1 definition,
gate, eligibility, or alignment policy: fixture selection, provenance,
alignment estimation, decoded-length checks, and error formulas are reused
verbatim from `vbr5_metrics.py`. A fixture B1 excludes (`quality is None`)
is passed through as skipped, never analyzed.

Command against existing artifacts (no re-encoding; decoding only happens
inside the referenced B1 run):

```sh
B1="${TMPDIR}/f04b2-b1-subset" && V3="${TMPDIR}/v3-full-rvehNNkF" && \
python3 tools/aac-enc-metrics/vbr5_localize.py \
  --b1-json "$B1/metrics.json" \
  --b1-work-dir "$B1" \
  --v3-reference-dir "$V3" \
  --out-dir "${TMPDIR}/f04b2-localize" \
  --only m2-sine-stereo-48k,m2-impulse-mono-48k,m2-edge-mono-48k,m7-music-stereo-48k
```

Outputs: `<out-dir>/localization.json`, `<out-dir>/report.md`. Add
`--enable-spectral` for the stdlib Hann-DFT band-energy mode.

Required files and provenance:

- `--b1-json`: a B1 `metrics.json` with eligibility, alignment, encoded
  frame counts, and quality blocks.
- Wave source: `--b1-work-dir` (`inputs/<stem>.wav`,
  `decoded/<stem>.{rust,c}.wav`) and/or `--v3-reference-dir` (`inputs/`,
  `rust-wav/`, `c-wav/`). With both, workdir WAVs are additionally verified
  byte-for-byte against the retained V3 files.
- For each analyzed fixture the tool verifies the original hash against the
  manifest and both decoded hashes against the B1 JSON, recomputes B1
  alignment with the same contract, and refuses to analyze when the lags
  differ from the B1 run.

Frame-to-time mapping and alignment semantics:

- Window `k` covers original samples `[k*W, min((k+1)*W, overlap))`, where
  `W` is `--window-samples` (default 1024, AAC frame-sized) and `overlap` is
  the joint B1-style overlap
  `min(rust_len - lag_r, c_len - lag_c, orig_len)`. Time is `sample / rate`.
- Both implementations are compared against the same original samples; the
  Rust-vs-C difference compares the decoded samples aligned to that same
  original interval. Lags are the B1 selected lags (1600/1600 for the whole
  retained corpus); the decoder `outputDelay` is never subtracted twice.
- A decoded analysis window is not an AAC encoder frame. Encoder delay,
  MDCT overlap-add, and decoder priming prevent any sample-to-frame causal
  mapping, as the B1 manifest states.

Windowing and partial-window treatment:

- The first window starts at original sample 0 and is full unless the whole
  overlap is shorter than one window. The last window is partial when
  `overlap % W != 0` (752 samples at 48 kHz, 811 at 44.1 kHz in the retained
  corpus) and is flagged `partial: true`; its metrics use only valid
  samples. Empty spans yield status `insufficient_overlap`.
- Regions (`--region-samples`, default 4096) group whole windows with the
  same raw-energy aggregation; the last region may be short.

Metric definitions, units, undefined cases:

- Per channel and window: `rms_error_lsb`, `peak_error_lsb`,
  `signal_rms_lsb`, `signal_peak_lsb`, `snr_db` for rust-vs-original and
  c-vs-original with B1-identical formulas; `rms_difference_lsb` and
  `peak_difference_lsb` (plus a B1-style diff SNR against the rust decode)
  for rust-vs-c; `valid_samples` and time bounds.
- `snr_status` mirrors B1: `defined`, `identical` (zero error, nonzero
  reference), `undefined_silence` (zero reference energy, including the
  all-silent case), `insufficient_overlap` (no samples).
- Reconciliation: per-fixture window aggregates are checked against a
  joint full-band computation over exactly the same samples
  (`reconciled: true` in the validated subset). B1's `rust_vs_c` full-band
  spans the whole decoded streams including priming, so its RMS differs
  slightly from the joint-overlap diff; both numbers are reported with the
  span difference stated, not hidden.

Spectral methodology and limitations (`--enable-spectral` only):

- Hann-windowed N-point DFT (stdlib radix-2 FFT, N = window size) of the
  error signals, per channel, full windows only (partial windows are
  `partial_skipped`). Bands in Hz: [0,1000), [1000,4000), [4000,8000),
  [8000,12000), [12000,Nyquist]; one-sided bin energies with interior bins
  doubled so bands sum to the Parseval total. Identical parameters for
  rust-err, c-err, and rust-vs-c diff.
- These are windowed error energies, not masking-weighted errors. Nothing
  here is called NMR and no masking model is implemented. Cost is seconds
  per fixture (stdlib FFT); spectral runs subset-first.

Tail/termination interpretation:

- Reports expected decoded length (`frame_count*1024 - outputDelay`,
  recomputed with B1's check), actual lengths, deltas, B1 tail shortfalls,
  last complete / partial window indices, final-region (last 4 windows)
  Rust-vs-C differences, and whether lengths are equal. In the retained
  corpus all Rust/C lengths are equal, so final-region differences persist
  within the overlapping samples rather than being length effects. A tail
  difference is never labelled a codec defect.

Tests and validation commands:

```sh
python3 -m unittest discover -s tools/aac-enc-metrics -p 'test_*.py' -v
```

`test_vbr5_localize.py` covers identical streams, single-window error
confinement, channel-specific differences, partial first/last windows,
silence/zero-error statuses, invalid rates/channels/lengths, B1-excluded
passthrough, lag/hash refusal, determinism (byte-identical reruns), FFT
correctness vs naive DFT, band tiling, and Parseval energy conservation.

What this evidence can and cannot establish:

- It can show which time windows/regions/bands hold the most Rust-vs-C
  decoded-difference energy for a given fixture and decoder, and whether a
  tail difference survives within the overlapping samples.
- It cannot establish audibility, perceptual equivalence, causality (a hot
  window does not name a codec stage), or tail/flush correctness beyond
  B1's exact length identity. All observations are conditional on the
  single FDK decoder; no second-decoder comparison exists.
