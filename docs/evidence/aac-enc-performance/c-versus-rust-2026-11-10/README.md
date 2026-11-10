# C versus Rust AAC-LC VBR-5 encoder benchmark — 2026-11-10

Status: **measurement and analysis complete; no production change;
documentation/evidence record. Published as a standalone
documentation/evidence commit; not pushed.** This is a benchmark evidence
record, not normative guidance.

Question: how does the optimized native Rust AAC-LC VBR-5 encoder in Ferrite
perform relative to the original FDK-AAC C encoder on identical audio
workloads?

**Verdict: the comparison is fair within the stated limits, and the Rust
encoder is competitive but not at parity.** On the 20-workload, 91.45 s
corpus, the optimized Rust production API is **5.80% slower** from the two
arm medians (ratio Rust/C 1.0580); the paired median is **+6.25%** and
**8/8** alternating pairs are slower. The gap is concentrated in speech
(+12.7%) and short fixtures (+21% to +72%); the heavy categories that dominate
the aggregate are much closer: music +3.37% and the 40 s long workload
+3.37%. A portable `generic`-CPU rebuild reproduces the result (paired median
+6.15%, 8/8 pairs).

---

## 1. Executive summary

| Metric | C reference | Rust | Rust vs C |
|---|---:|---:|---:|
| Aggregate, median of complete-corpus per-repetition sums | **374.617 ms** | **396.337 ms** | ratio **1.0580** |
| Rust slowdown `(Rust/C − 1)`, arm medians | — | — | **+5.80%** |
| Rust speed advantage `(C/Rust − 1)`, arm medians | — | — | **−5.48%** (negative = Rust slower) |
| Realtime multiple | **244.1×** | **230.7×** | — |
| RTF (encode time / audio time) | **0.004096** | **0.004334** | — |
| Paired slowdown, median of 8 pairs | — | — | **+6.25%** |
| Paired slowdown range / IQR | — | — | **+5.34% to +10.49% / 1.38 pp** |
| Pairs where Rust is slower | — | — | **8 / 8** |
| Bootstrap 95% CI of paired median | — | — | **+5.45% to +6.87%** |

- **Corpus:** 20 workloads, 91.450 s of audio, 15 measured repetitions per
  workload, one untimed warm-up, eight alternating C/R pairs, plus three
  standalone and two post-series runs per encoder.
- **Output validity:** all 40 final streams (20 C + 20 Rust) parse as
  complete ADTS; all 40 decode with the reference `aac-dec` at exit 0 with
  empty stderr; decoded sample counts are identical between C and Rust on
  every workload.
- **Reference identity:** the rebuilt C encoder reproduces all 19 prior
  reference C ADTS artifacts **byte-for-byte**; the Rust harness reproduces
  all 20 published byte totals and FNV-1a hashes.
- **Portable check:** C 383.420 ms vs Rust 407.514 ms, ratio 1.0628, paired
  median +6.15%, 8/8 pairs slower. The relative result is not an
  `apple-m1`-specific codegen artifact.
- **Caveat:** this is a throughput and output-size comparison, not a
  perceptual-quality or ABX result. C and Rust streams are not expected to be
  bit-identical; they are not, except for silence and the edge fixture.

**Recommendation:** yes, sufficiently competitive for a production choice if
the requirement is "within roughly 10% aggregate and within roughly 5% on the
dominant music/long workloads." It is not at parity, and speech is the
weakest category (+12.7%). If further work is desired, target the Rust bit
path, memory movement/allocation, and rate-control paths; do not start a new
optimization campaign in this task. See §10.

---

## 2. What was compared

### 2.1 C reference

The C side is the original FDK-AAC encoder used by the prior oracle and
quality evidence: the stripped AAC-LC VBR-5, afterburner-on, ADTS build. The
current published tree contains only the Rust workspace; the C encoder is
preserved in a local backup of the earlier full FDK-AAC checkout at
`~/backups/fdk-aac-m5.23-worktree-20261016T193606Z/worktree` (backup commit
`e4258d2`, pinned reference
`f14ad40a574a4132df947d15e9fb14bee91cc8a5`). That source is byte-identical to
the reference baseline captured in the backup's
`evidence/m522/base/` directory (the published repository evidence was not
modified), and the rebuilt binary reproduces the retained C artifacts. The
`cpp/` tree and CLI files are byte-identical to that baseline and the rebuilt
encoder reproduces the 19 retained C ADTS artifacts exactly. Details and
hashes: `build-config.txt`, `identity.txt`,
`raw/validation/c-artifact-comparison.txt`.

### 2.2 Rust side

The Rust side is the published production `Encoder` API at HEAD
`1bd724fa8786ef4003a1513b410400674bd88f52`, including the five retained
performance optimizations from the 2026-10-28 checkpoint. The measured
binary is an example built from a temporary `git checkout-index` copy of
HEAD; its production source is byte-identical to HEAD, with only the
benchmark example and test-only fixture dumper added. No `cfg(test)` code is
in the measured binary.

### 2.3 Settings equivalence

| Setting | C reference | Rust production API | Equivalent? |
|---|---|---|---|
| Object type | AAC-LC (`AOT=2`) | AAC-LC (config-enforced) | yes |
| Rate mode | VBR mode 5 | VBR-5 (config-enforced) | yes |
| Transport | ADTS | ADTS | yes |
| Sample rates | 44.1 / 48 kHz | 44.1 / 48 kHz | yes |
| Channels | mono / stereo | mono / stereo | yes |
| Frame length | 1024 | 1024 | yes |
| Afterburner | on | always on | yes |
| Channel order | WAV order (`AACENC_CHANNELORDER=1`) | WAV interleaved input | yes |
| Input PCM | 16-bit WAV, preloaded | 16-bit WAV, preloaded | yes |
| Encoder state | fresh per repetition | fresh per repetition | yes |

The C and Rust APIs do not expose identical control planes, but the
quality-affecting configuration is the same. The user bitrate is ignored in
both VBR-5 paths. Remaining differences are implementation/API differences,
not settings differences:

- C uses fixed-point arithmetic; Rust is float-native. Outputs are expected
  to differ and do.
- C's `aacEncEncode` writes into a caller-provided buffer; Rust's `push`
  returns `Vec<AdtsFrame>` (one `Vec<u8>` per frame), which the harness copies
  into a preallocated output buffer. Both include one output copy in the
  timed region; Rust additionally allocates each frame's byte vector inside
  the timed region. This is production API behavior.
- C's `aacEncInfo` is called inside the timed initialization; Rust has no
  equivalent call. The cost is negligible (microseconds).
- The two encoders run in separate processes; process startup is outside the
  timed region. A single binary linking both was not built; the timing
  boundaries are matched explicitly (below).

---

## 3. Method

### 3.1 Corpus

The established 20-workload checkpoint corpus is reused unchanged:

- 15 deterministic synthetic fixtures (silence, sine, onset, stepdown, click,
  impulse, edge, multitone, sweep, noise, tonal-noise, quiet-sine, 44.1 kHz
  sine/onset, stress).
- Four committed WAV fixtures (`m7-music`, `m7-speech-clean`,
  `m7-speech-pauses`, `m8-music-eroica`).
- One derived 40 s workload, `m7-music-stereo-48k-x4`, formed by concatenating
  the committed music PCM four times.

Total audio: **91.450 s**. Canonical PCM was materialized from the published
crate's own `testdata::fixture_samples` generator; the four committed WAVs
are read by that same function. Every WAV SHA-256 and PCM FNV-1a is in
`corpus-manifest.tsv`. The Rust harness asserts the published byte total,
frame count, and ADTS hash for all 20 workloads on every repetition, so a
wrong corpus or production drift fails loudly.

### 3.2 Timing boundary

| Region | C harness | Rust harness | Included? |
|---|---|---|---|
| WAV load / parse | before timing | before timing | no |
| Encoder configuration | `aacEncoder_SetParam` calls | `EncoderConfig::new` | yes |
| Encoder construction/init | `aacEncOpen` + `NULL` init + `aacEncInfo` | `Encoder::new` | yes |
| PCM submission + frame processing | `aacEncEncode` per 1024-frame chunk | `push` per 1024-frame chunk | yes |
| Final flush | `aacEncEncode(..., -1)` loop | `flush()` | yes |
| Output assembly / copy | stack buffer + `memcpy` to output | `Vec<AdtsFrame>` + `extend_from_slice` | yes |
| Encoder close / drop | `aacEncClose` | `drop(enc)` | no |
| ADTS validation / hashing / file write | after timing | after timing | no |
| Process startup | before timing | before timing | no |

Both harnesses feed exactly 1,024 sample frames per channel per call (the
last call may be partial), matching the C `frameLength` and the Rust
`FRAME_LENGTH`. Both construct a fresh encoder per repetition and process the
fixture from the beginning. Output buffers are allocated outside the timed
region in both harnesses. The C harness uses a 20 KB stack output buffer per
`aacEncEncode` call; the Rust API's per-frame allocation is part of the
measured production path.

### 3.3 Run protocol

- One untimed warm-up pass per process, then 15 measured repetitions per
  workload, repetition-major / workload-minor.
- Eight alternating C/R pairs: odd pairs C→R, even pairs R→C.
- Three standalone C and three standalone Rust runs before the pairs; two
  post-series C and two post-series Rust runs for drift checks.
- Each run is a fresh process; the timed region excludes startup and I/O.
- No CPU pinning (not available on macOS); the scheduler and background load
  are uncontrolled. Thermal state was checked before/after and showed no
  warnings.
- No run was discarded or repeated. The elevated pair-8/post-series C times
  are retained as session drift; the alternating pairs and medians are the
  documented control. No anomaly-triggered repeat rule was invoked because no
  run failed validation or produced an implausible rate/frame count.

### 3.4 Aggregation

- Per-workload time is the median of the 15 measured repetitions in a run.
- Per-run aggregate is the **median of the complete-corpus per-repetition
  sums**: for each repetition, sum the 20 workload totals, then take the
  median of the 15 sums. This is the established checkpoint method, not an
  average of per-workload percentages.
- Pair slowdown is `(Rust aggregate / C aggregate − 1) × 100`; pair speed
  advantage is `(C aggregate / Rust aggregate − 1) × 100`. The headline uses
  the median of the eight pair slowdowns; the arm medians are also reported.
- Per-workload medians pool the eight paired runs (8 × 15 = 120 samples per
  encoder per workload); variability is the IQR of those samples.

---

## 4. Build configuration

Full flags and hashes are in `build-config.txt` and `identity.txt`.

| Item | Native | Portable |
|---|---|---|
| C compiler | Apple clang 21.0.0, target `arm64-apple-darwin27.0.0` | same |
| C flags | `-O3 -DNDEBUG -arch arm64 -fPIC -fno-exceptions -fno-rtti` | add `-mcpu=generic` |
| C default CPU | `apple-m1` | `generic` |
| C LTO | off | off |
| FDK ARM assembly/intrinsic files | none compiled | none compiled |
| Rust compiler | rustc 1.99.1 (58cf8f9c3), LLVM 23.1.1 | same |
| Rust target | `aarch64-apple-darwin` | same |
| Rust default CPU | `apple-m1` | `generic` (`RUSTFLAGS=-C target-cpu=generic`) |
| Rust profile | release, `opt-level=3`, LTO off, `codegen-units=16`, `strip=debuginfo` | same |
| Rust features | `aac_enc` | `aac_enc` |
| RUSTFLAGS | none | `-C target-cpu=generic` |
| SIMD | no explicit `core::arch`/NEON intrinsics; LLVM auto-vectorization | same |
| Host | Apple M5, 10 CPUs, 16 GiB, macOS 27.0.1 / Darwin 27.0.0 | same |

Neither build uses `-mcpu=native` or `-C target-cpu=native`; both use the
production compiler default (`apple-m1`). The portable comparison lowers both
to `generic` to test sensitivity to architecture tuning. The C build does not
compile FDK's `libFDK/src/arm` files; the only architecture-specific effect is
compiler auto-vectorization/scheduling. The Rust source contains no explicit
NEON/SIMD intrinsics in `rust/src/aac_enc` or `rust/src/common`; LLVM
auto-vectorization is present in the release assembly.

---

## 5. Native results

### 5.1 Aggregate

| Pair | Order | C aggregate ms | Rust aggregate ms | Ratio R/C | Rust slowdown | Rust speed advantage |
|---:|---|---:|---:|---:|---:|---:|
| 1 | C→R | 371.174 | 393.292 | 1.0596 | +5.96% | −5.62% |
| 2 | R→C | 372.403 | 392.936 | 1.0551 | +5.51% | −5.23% |
| 3 | C→R | 370.920 | 396.089 | 1.0679 | +6.79% | −6.35% |
| 4 | R→C | 375.886 | 396.390 | 1.0545 | +5.45% | −5.17% |
| 5 | C→R | 376.191 | 396.285 | 1.0534 | +5.34% | −5.07% |
| 6 | R→C | 373.348 | 412.501 | 1.1049 | +10.49% | −9.49% |
| 7 | C→R | 376.218 | 402.046 | 1.0687 | +6.87% | −6.42% |
| 8 | R→C | 378.646 | 403.428 | 1.0655 | +6.55% | −6.14% |
| **Median** | | **374.617** | **396.337** | **1.0580** | **+6.25%** | **−5.88%** |

The ratio of the two arm medians is 1.0580 (+5.80% slowdown); the median of
the eight pair ratios is +6.25%. Both are reported because they answer
slightly different questions. The sign is identical: **Rust is slower in
every pair.** The pair slowdown IQR is 1.38 pp; the bootstrap 95% CI of the
paired median is +5.45% to +6.87%. The C→R pairs have a median +6.37%; the
R→C pairs +6.03%; the order effect is about 0.34 pp, small relative to the
gap. Pair 6 is the only clear tail (+10.49%); no run was excluded.

Drift check (not the headline):

| Series | C aggregate ms | Rust aggregate ms |
|---|---:|---:|
| Standalone 1–3 | 360.530, 364.142, 367.715 | 394.087, 389.566, 394.673 |
| Post-series 1–2 | 379.244, 379.687 | 399.844, 403.088 |
| Five-run median | 367.715 | 394.673 |

C drifted about 4–5% slower by the post-series runs; Rust drifted about
2–3%. The alternating pairs and the pair medians are the primary estimate.
The standalone/post values are a drift check, not a second headline.

### 5.2 Per-workload results

Medians pool the eight paired runs (120 samples per encoder per workload).
C and Rust frame counts are identical for all 20 workloads. `Rust speed
advantage` is `1/(1+slowdown) − 1`; it is negative for every workload because
Rust is slower.

| Workload | Category | C median ms | Rust median ms | Ratio R/C | Rust slowdown | C IQR ms | Rust IQR ms | C / Rust bytes |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| `m2-click-mono-48k` | short_diagnostic | 0.721 | 0.873 | 1.2110 | +21.10% | 0.022 | 0.024 | 2739 / 2680 |
| `m2-edge-mono-48k` | short_diagnostic | 0.123 | 0.195 | 1.5855 | +58.55% | 0.007 | 0.008 | 415 / 415 |
| `m2-impulse-mono-48k` | short_diagnostic | 0.106 | 0.166 | 1.5579 | +55.79% | 0.005 | 0.009 | 308 / 306 |
| `m2-onset-stereo-48k` | short_diagnostic | 1.200 | 1.762 | 1.4674 | +46.74% | 0.041 | 0.054 | 2318 / 2359 |
| `m2-silence-stereo-48k` | short_diagnostic | 0.605 | 1.042 | 1.7223 | +72.23% | 0.015 | 0.045 | 735 / 735 |
| `m2-sine-stereo-48k` | short_diagnostic | 1.742 | 2.377 | 1.3647 | +36.47% | 0.051 | 0.083 | 3104 / 3147 |
| `m2-stepdown-stereo-48k` | short_diagnostic | 1.196 | 1.734 | 1.4497 | +44.97% | 0.031 | 0.045 | 2209 / 2236 |
| `m3-multitone-stereo-48k` | tonal_noise | 2.095 | 2.722 | 1.2997 | +29.97% | 0.058 | 0.081 | 4726 / 4819 |
| `m3-noise-mono-48k` | tonal_noise | 1.047 | 1.118 | 1.0676 | +6.76% | 0.042 | 0.036 | 10601 / 10603 |
| `m3-onset-stereo-441k` | transient | 1.270 | 1.737 | 1.3681 | +36.81% | 0.039 | 0.062 | 2247 / 2250 |
| `m3-quiet-sine-stereo-48k` | tonal_noise | 2.006 | 2.413 | 1.2030 | +20.30% | 0.050 | 0.069 | 4189 / 4200 |
| `m3-sine-stereo-441k` | tonal_noise | 1.932 | 2.432 | 1.2588 | +25.88% | 0.043 | 0.076 | 3014 / 2998 |
| `m3-sweep-stereo-48k` | tonal_noise | 1.970 | 2.478 | 1.2577 | +25.77% | 0.062 | 0.059 | 4088 / 3988 |
| `m3-tonal-noise-stereo-48k` | tonal_noise | 1.393 | 1.611 | 1.1565 | +15.65% | 0.043 | 0.042 | 10628 / 10630 |
| `m6-stress-stereo-48k` | transient | 1.462 | 1.672 | 1.1433 | +14.33% | 0.043 | 0.038 | 10728 / 10728 |
| `m7-music-stereo-48k` | music | 49.958 | 51.639 | 1.0337 | +3.37% | 0.646 | 1.003 | 251279 / 250926 |
| `m7-music-stereo-48k-x4` | long | 199.097 | 205.808 | 1.0337 | +3.37% | 2.598 | 4.310 | 999312 / 997957 |
| `m7-speech-clean-stereo-48k` | speech | 30.125 | 33.941 | 1.1267 | +12.67% | 0.352 | 0.515 | 127971 / 127900 |
| `m7-speech-pauses-stereo-48k` | speech | 30.037 | 33.848 | 1.1269 | +12.69% | 0.352 | 0.769 | 125996 / 125818 |
| `m8-music-eroica-stereo-48k` | music | 46.485 | 47.505 | 1.0219 | +2.19% | 0.649 | 0.983 | 235444 / 234229 |

Observations:

- The largest percentages are on sub-3 ms fixtures, where fixed per-stream
  and per-frame overhead dominates. Their IQRs are 0.005–0.083 ms.
- The heavy music workloads are close: +3.37% (music), +3.37% (40 s long),
  +2.19% (Eroica). These three are 79% of C corpus time.
- Speech is the largest weighted gap: +12.67% and +12.69%, with 16.1% of C
  corpus time.
- `m3-noise-mono` is the closest workload (+6.76%); silence is the farthest
  (+72.23%), but both are short and low-weight.

### 5.3 Category analysis

Category totals are medians of per-repetition category sums across the eight
pairs. Shares are of each encoder's total corpus time.

| Category | Fixtures | C ms | Rust ms | Ratio R/C | Rust slowdown | C share | Rust share |
|---|---:|---:|---:|---:|---:|---:|---:|
| Short diagnostic | 7 | 5.718 | 8.160 | 1.4272 | +42.72% | 1.5% | 2.1% |
| Tonal / noise | 6 | 10.422 | 12.744 | 1.2228 | +22.28% | 2.8% | 3.2% |
| Transient | 2 | 2.734 | 3.404 | 1.2449 | +24.49% | 0.7% | 0.9% |
| Music | 2 | 96.323 | 99.198 | 1.0298 | +2.98% | 25.7% | 25.0% |
| Speech | 2 | 60.219 | 67.665 | 1.1236 | +12.36% | 16.1% | 17.1% |
| Long (40 s) | 1 | 198.938 | 205.476 | 1.0329 | +3.29% | 53.1% | 51.8% |

Contribution to the +5.95% category-total aggregate (percentage points; the
paired median is +6.25%):

| Category | Share of C time | Contribution to Rust slowdown |
|---|---:|---:|
| Long | 53.1% | +1.75 pp |
| Speech | 16.1% | +1.99 pp |
| Music | 25.7% | +0.77 pp |
| Short diagnostic | 1.5% | +0.65 pp |
| Tonal / noise | 2.8% | +0.62 pp |
| Transient | 0.7% | +0.18 pp |
| **Total** | | **+5.95 pp** |

Columns may not sum exactly because of rounding.

This is the key result: the aggregate gap is not driven by a single outlier.
It is mostly the combination of the dominant long/music workloads being ~3.3%
slower and speech being ~12.4% slower. The short/tonal categories look
dramatic in percentage terms but contribute only ~1.45 pp because they are a
small share of total time.

---

## 6. Portable-build sensitivity

The portable builds use `-mcpu=generic` (C) and `-C target-cpu=generic`
(Rust), with the same source, flags, corpus, API boundaries, 10 repetitions,
and eight alternating pairs. Both portable builds produce **byte-identical**
outputs to the native/reference outputs (C 20/20 native outputs and 19/19
prior artifacts; Rust 20/20 published pins).

| Metric | Native | Portable |
|---|---:|---:|
| C aggregate median | 374.617 ms | 383.420 ms |
| Rust aggregate median | 396.337 ms | 407.514 ms |
| Ratio Rust/C | 1.0580 | 1.0628 |
| Rust slowdown from arm medians | +5.80% | +6.28% |
| Paired slowdown median | +6.25% | +6.15% |
| Paired slowdown range | +5.34% to +10.49% | +4.97% to +7.51% |
| Pairs Rust slower | 8/8 | 8/8 |

Representative per-workload portable ratios: music +3.77%, 40 s long +3.79%,
Eroica +2.99%, speech-clean +12.36%, speech-pauses +12.85%, short fixtures
+18.0% to +71.5%. The full table is `portable-per-workload.tsv`.

The portable central estimate and every-workload pattern match the native
result; its pair range is narrower (4.97–7.51%) than the native range, whose
tail is the single +10.49% pair. The conclusion is not an `apple-m1`-specific
scheduling or tuning artifact.

---

## 7. Correctness and output characteristics

| Check | Result |
|---|---|
| C reference identity | 19/19 prior C ADTS artifacts reproduced **byte-for-byte**; the derived x4 has no prior artifact but is deterministic across all repetitions/runs |
| Production baseline regression | `production_f32_corpus_baseline_is_unchanged`: **1 passed, 0 failed** (release, exact HEAD source) |
| Rust output identity | 20/20 published byte totals, frame counts, and FNV-1a ADTS hashes reproduced on every repetition |
| ADTS framing | 20/20 C and 20/20 Rust streams parse with no sync loss, no truncated frame, no trailing bytes; frame accounting matches |
| Frame counts | C == Rust for all 20 workloads (25/14/11/49/49/49/49/49/25/45/49/45/49/25/25/471/1877/471/471/471) |
| Decode validation | 40/40 `aac-dec` invocations exit 0 with empty stderr and a parseable WAV |
| Decoded length | C and Rust decoded sample counts identical on all 20 workloads; decoded duration = input duration plus the codec delay/priming (6–12 ms depending on rate/frame count) |
| Output hashes | C and Rust identical only for `m2-silence` and `m2-edge`; all other streams differ, as expected for fixed-point C vs float-native Rust |
| Output size / bitrate | Heavy workloads within ~0.1–0.5%; short fixtures up to ~2.5%; no truncation or implausible rate |
| Determinism | One hash per workload across all repetitions/runs for C and for Rust (`determinism.json` has no failures) |

The C/R output-size differences are small and consistent with the same VBR-5
operating point. This is a bitrate/output-size observation, not a perceptual
quality claim. No ABX or listening test was run. The reference decoder is the
same FDK build used for the C encoder; decoded lengths agree, but this is not
an independent codec conformance test.

Full per-workload output hashes, sizes, frames, and bitrates:
`output-validation.tsv`; decode details: `decode-validation.tsv`.

---

## 8. Technical analysis

### 8.1 Profiling method

`sample` (macOS, 1 ms interval, 2 s) was attached to separate 100–120
repetition runs of the music and speech-clean workloads. Both binaries retain
symbols; Rust names are v0-mangled but readable. The stage buckets are an
approximate mapping of top-of-stack self samples; they are diagnostic, not a
source-line profile. Shares are normalized by the sampled main-thread root
samples (1,655–1,709 per capture). Raw captures: `raw/profiles/`;
machine-readable rows: `profile-symbols.tsv`, `profile-stages.tsv`.

### 8.2 Stage shares

| Stage | C music | Rust music | C speech | Rust speech |
|---|---:|---:|---:|---:|
| S4 scalefactor/quantization | 63.3% | 55.0% | 51.8% | 42.4% |
| Front-end / psy / TNS / transform | 20.0% | 18.9% | 29.2% | 25.4% |
| Bit counting / bit encoding | 5.5% | 10.2% | 4.0% | 10.6% |
| Memory / feeder | 1.4% | 4.5% | 1.2% | 6.0% |
| Rate control | 0.9% | 3.1% | 1.1% | 4.9% |
| Driver / API / output copy | 3.9% | 2.5% | 5.7% | 4.4% |
| Other | 2.1% | 1.7% | 4.4% | 2.7% |

Top self-time symbols are consistent with the stage table:

- Both: `calcSfbDist` / `quant::calc_sfb_dist` and
  `EstimateScaleFactors` / `scf::estimate_scale_factors_channel_reuse` are
  the two largest symbols. The S4 search is the dominant cost in both
  encoders.
- C also has large `CalcFormFactor`, `calcSfbQuantEnergyAndDist`,
  `QuantizeSpectrum`, `TnsDetect`, `dit_fft`, `BlockSwitching`, and
  `dynBitCount` shares.
- Rust also has large `calc_sfb_quant_energy_and_dist`, `dyn_bit_count_reuse`,
  `encode_spectral_data`, `calc_ld_data`, `form_factor`,
  `autocorr_quarter_lags`, `block_switch::process`, `quantize_spectrum`,
  `intensity_stereo`, `adapt_thresholds_vbr`, and `calc_max_value_in_sfb`
  shares.

### 8.3 Interpretation

The profile pattern explains the category result:

- **Music / long:** both encoders are dominated by the same S4 search. Rust's
  S4 and front-end shares are somewhat lower, but its bit path, memory, and
  rate-control shares are higher. The net is only ~2–4% slower. This is the
  bulk of the corpus time.
- **Speech:** the S4 search is a smaller share, and the bit path, memory, and
  rate-control costs are larger. Rust's higher shares in those stages align
  with the ~12.7% speech gap.
- **Short fixtures:** the absolute times are 0.1–2.4 ms. Encoder
  initialization is only ~12–20 µs for both, so startup is not the cause; the
  gap is per-frame steady-state work (and fixed first-frame/cache effects).
  Their high percentages have little effect on the 91.45 s aggregate.
- **Output path:** Rust's `Encoder::push` returns per-frame `Vec<u8>` values;
  the harness's `extend_from_slice` and the per-frame allocations are part of
  the measured production path. C's public API writes into a caller buffer.
  This is a real API cost difference, not a harness asymmetry, but a
  core-only comparison (bypassing frame assembly) could show a smaller gap.

The stage mapping is approximate and the profilers ran on separate processes;
do not over-interpret small share differences. The robust statement is: both
are S4-dominated; Rust's remaining overhead is disproportionately in the bit
path, memory/allocation, and rate control, which is consistent with the
speech gap.

---

## 9. Limitations and confidence

**Confidence.** The sign is robust: 8/8 native pairs and 8/8 portable pairs
show Rust slower; the native paired median is +6.25% with bootstrap 95% CI
+5.45% to +6.87%; the portable median is +6.15%. The exact aggregate is
sensitive to session drift at about ±1 pp, and one native pair is a +10.49%
tail; the category and workload pattern is stable.

Limitations:

- One machine, one OS, one toolchain, one session. Background desktop load
  was present; no CPU pinning or frequency control is available on macOS.
  P/E-core placement and thermal drift remain the largest variability source.
  Alternating pairs and medians control the drift but do not eliminate it.
- Separate processes rather than one binary linking both encoders. Process
  startup and WAV I/O are outside the timed region in both harnesses; the
  boundaries are matched, but a single-process design would remove the
  remaining process-level difference.
- C and Rust output paths differ (caller buffer vs per-frame `Vec`). Both
  include one copy; Rust includes per-frame allocation. This is production
  behavior, but it is an API-level difference, not a pure DSP comparison.
- C is fixed-point and Rust is float-native. Streams are not bit-identical
  and are not expected to be; no perceptual-quality or ABX claim is made.
- The C source is a backup of the pinned reference tree, not a fresh network
  fetch. It is byte-identical to the captured reference baseline and
  reproduces 19/19 prior C artifacts, which is stronger evidence than a
  version label alone.
- Short fixtures are sub-3 ms and dominated by fixed per-stream/per-frame
  overhead. Their percentages are recorded but are not the primary result.
- Profiling uses `sample` self time with an approximate stage map; inline
  attribution and separate profiling runs limit precision.
- Portable builds still use the AArch64 baseline/NEON auto-vectorization;
  "portable" here means no `apple-m1` scheduling/tuning, not scalar-only
  code.
- No independent decoder conformance test; `aac-dec` is the same FDK
  reference build. Decoded sample counts and clean exits are the validation.
- The 40 output streams and the 20 corpus WAVs are retained in the temporary
  working directory, not in this evidence directory (size and audio-content
  licensing). Their hashes, sizes, and validation rows are retained;
  `corpus-manifest.tsv` plus the published generator reproduces the corpus.

---

## 10. Verdict and recommendation

**Comparison validity:** fair within the limits above. Both sides use their
production APIs, the same PCM, the same AAC-LC VBR-5 ADTS settings, a fresh
encoder per repetition, and matched timed boundaries. The C reference is
bit-exact to prior reference artifacts; the Rust side is bit-exact to the
published baseline; the portable rebuild reproduces the result.

**Performance verdict:** the optimized Rust encoder is **competitive but not
at parity**. Aggregate +5.80% slower (paired median +6.25%), with the dominant
music/long workloads at ~3.4% and speech at ~12.7%. All pairs are slower, so
this is not noise; one native pair is a +10.49% tail, but the median and the
seven other pairs are +5.3% to +6.9%.

**Recommendation:** if the production bar is "within ~10% aggregate and
within ~5% on the dominant music/long workloads," the Rust encoder meets it.
If the bar is workload-by-workload parity, it does not, and speech is the
first target. The existing 2026-10-28 checkpoint already recommended stopping
incremental optimization; this benchmark supports using the current Rust tree
as the C-comparison reference rather than starting another micro-optimization
sweep. If further work is explicitly funded, focus on:

1. Rust bit counting/bit encoding (`dyn_bit_count`, `encode_spectral_data`,
   `calc_ld_data`) — higher share than C, especially on speech.
2. Memory movement/allocation in the frame path (`memmove`, `memset`,
   per-frame `Vec` allocation).
3. Rate-control/threshold work (`adapt_thresholds_vbr`,
   `calc_max_value_in_sfb`).
4. Speech-specific behavior, where the gap is largest among heavy workloads.

No production algorithm was changed by this task.

---

## 11. Reproduction

The full build and run commands are in `build-config.txt` and
`harness/README.md`. Summary:

```sh
# 1. Materialize the 20-workload corpus from the published generator
FIXTURE_OUT="$T/corpus" cargo test --release --locked --features aac_enc --lib \
  aac_enc::fixture_dump::fixture_dump_tests::dump_all_benchmark_fixtures -- --nocapture

# 2. Build the C reference and harness (see build-config.txt), then:
bash harness/run_measurements.sh          # native series
bash harness/run_portable.sh              # portable series

# 3. Recompute the tables
python3 analysis/assemble_identity.py
python3 analysis/analyze.py
python3 analysis/analyze_portable.py
python3 analysis/profile_summary.py
```

Where `$T` is a working directory outside the repository. The measured
commands used `$T=/private/var/folders/…/opencode/aac-c-vs-rust-2026-11-10`.

---

## 12. Evidence files

| File / directory | Contents |
|---|---|
| `README.md` | This report. |
| `build-config.txt` | Host, toolchains, flags, build/benchmark commands, timing rules, corpus description. |
| `identity.txt` | Source and binary SHA-256 records, HEAD tree, pinned reference, aggregate source hashes. |
| `corpus-manifest.tsv` | All 20 workloads: WAV path/SHA-256, PCM FNV-1a, sample rate, channels, samples, audio duration, frames, published Rust bytes/hash, C/R bytes and stream SHA-256. |
| `paired-summary.tsv` | Eight native pairs: C/R aggregates, ratio, slowdown, speed advantage. |
| `per-run.tsv` | All native runs (stand, pair, post) with aggregates and min/max fixture times. |
| `per-workload.tsv` | 120-sample-per-encoder medians, ratios, slowdown/speed advantage, min/max/IQR, bytes, hashes, frames, bitrates. |
| `category-summary.tsv` | Category totals, ratios, shares. |
| `summary.json`, `summary.md` | Machine-readable and quick human summary of the native result. |
| `output-validation.tsv` | Per-workload C/R bytes, SHA-256, FNV-1a, frame counts, durations, bitrates, prior-reference match, hash equality. |
| `decode-validation.tsv` | 40 decoder runs: exit code, stderr length, WAV bytes, sample rate/channels/bits, decoded samples/duration. |
| `determinism.json` | Hash set per workload across all native runs; no failures. |
| `profile-symbols.tsv` | Top-of-stack self samples by symbol, workload, impl, and approximate stage. |
| `profile-stages.tsv` | Stage shares for music/speech, C/R, normalized to root samples. |
| `portable-paired-summary.tsv`, `portable-per-workload.tsv`, `portable-summary.json` | Portable-build sensitivity results. |
| `harness/` | Exact C harness, Rust example, fixture dumper, native/portable run scripts, and harness instructions. |
| `analysis/` | Exact parsing/aggregation/profiling scripts. |
| `raw/native/` | 26 native raw logs (3 C + 3 Rust standalone, 8 C + 8 Rust pairs, 2 C + 2 Rust post-series). |
| `raw/portable/` | 16 portable raw logs (8 C + 8 R). |
| `raw/validation/` | One-repetition validation logs, the production baseline regression, smoke logs, and the 19/19 C artifact comparison. |
| `raw/profiles/` | Raw `sample` captures and profiling run logs. |
| `raw/environment/` | Session environment, CMake/Rust build logs, analysis console logs. |

No audio files are included. The evidence is text only.

---

## 13. Repository integrity

At the start and end of the measurement:

- Branch `main`; HEAD `1bd724fa8786ef4003a1513b410400674bd88f52`;
  `origin/main` equal to HEAD; index empty.
- The two pre-existing uncommitted benchmark items were preserved unchanged:
  - `rust/src/aac_enc/stream_driver.rs` — opt-in `perf_tests` module.
  - `rust/examples/aac_bench_wav.rs` — standalone benchmark example.
- The benchmark task itself added the evidence directory and a
  documentation-index entry; it did not stage, commit, amend, rebase,
  cherry-pick, reset, clean, stash, or push anything, and did not modify
  production encoder algorithms. The separate publication task commits this
  evidence and the index entry; it does not push.
- The already published performance checkpoint and its evidence were not
  modified.

The exact final `git status --short` is recorded in the handoff report, not in
this file, because the evidence directory itself is part of the status.
