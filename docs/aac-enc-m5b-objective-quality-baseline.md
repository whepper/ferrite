# M5.b — Objective Quality Baseline

Measurement only. No encoder tuning, no perceptual claim, no ABX/listening tests.

**Status: `M5.b COMPLETE — BASELINE MEASUREMENTS REPRODUCED`**

## 1. Provenance and commands

- Repo HEAD `53481f3a`, branch `main`. Input fixtures are deterministic in-repo resynthesis (`rust/src/aac_enc/testdata.rs`: `click-mono`, `onset-stereo`, `sine-stereo-44100`).
- Original PCM WAVs written by new test-gated `m5b_dump_inputs` (`rust/src/aac_enc/m5b_measure.rs`) to `$TMPDIR/m5b-quality/inputs/`; 16-bit interleaved, plain 44-byte header:
  - `click-mono.orig.wav` SHA-256 `a5041d60…879404` (24000 samples/ch, mono, 48 kHz, peak 30000)
  - `onset-stereo.orig.wav` SHA-256 `a8f58ee1…73b36e3` (48000 samples/ch, stereo, 48 kHz, peak 10000)
  - `sine-stereo-44100.orig.wav` SHA-256 `5577677e…b634dd17` (44100 samples/ch, stereo, 44.1 kHz, peak 9999)
- Rust ADTS: regenerated for this milestone by re-running the M5.a harness (`AAC_DEC_BIN=… cargo test --features aac_enc m5a_`, from `rust/`); hashes identical to the M5.a report (§7).
- C reference: repo `aac-enc.c` (`in.wav out.aac`, fixed AAC-LC VBR-5 afterburner-on ADTS; `-r` accepted-but-ignored), built with `cmake --build /tmp/m5a-build --target aac-enc -j 8` (same tree/flags as the M5.a decoder build; Apple clang 21.0.0, cmake 4.4.4). C++ oracle not used anywhere.
- Decode: `/tmp/m5a-build/aac-dec` (SHA-256 `c5d5ff8d…83efd8`, see M5.a report); every invocation required exit 0 **and** empty stderr (M5.a §1 caveat); all 9 decodes clean.
- Metrics: `/tmp/m5b-metrics.py` (stdlib only, no numpy), full JSON at `/tmp/m5b-metrics.json`; method §5. Toolchain: rustc 1.99.0.

## 2. Per-case results

Notation: `orig` = fixture PCM; `rust` = Rust-ADTS-decoded; `c` = C-ADTS-decoded. Overlap at nominal lag (§5).

### click-mono (48 kHz, mono)

| Signal | Samples/ch | Peak | Clipped (≥32767) | RMS | All-zero |
|---|---|---|---|---|---|
| orig | 24000 | 30000 | 0 | 5659.806 | no |
| rust-dec | 23856 | 23068 | 0 | 5463.356 | no |
| c-dec | 23856 | 23747 | 0 | 5463.481 | no |

- rust-vs-orig: lag −1600, overlap 22256, err RMS 91.31, max abs err 6932, SNR **35.84 dB**.
- c-vs-orig: lag −1600, overlap 22256, err RMS 90.45, max abs err 6253, SNR **35.93 dB**.
- rust-vs-c: lag 0, overlap 23856, err RMS 11.88, max abs err 679, SNR **53.25 dB**.
- Bitrate (over original duration 0.5 s): rust 2680 B / 25 frames / 42.88 kbps (payload 2505 B, hdr 6.53%); c 2739 B / 25 frames / 43.82 kbps (payload 2564 B, hdr 6.39%).

### onset-stereo (48 kHz, stereo; both channels identical vectors — ch0 tabulated)

| Signal | Samples/ch | Peak | Clipped | RMS | All-zero |
|---|---|---|---|---|---|
| orig | 48000 | 10000 | 0 | 4999.738 | no |
| rust-dec | 48432 | 10079 | 0 | 4853.886 | no |
| c-dec | 48432 | 10078 | 0 | 4852.908 | no |

- rust-vs-orig: lag −1600, overlap 46832, err RMS 6.14, max abs err 135, SNR **58.10 dB**.
- c-vs-orig: lag −1600, overlap 46832, err RMS 6.49, max abs err 162, SNR **57.62 dB**.
- rust-vs-c: lag 0, overlap 48432, err RMS 3.26, max abs err 50, SNR **63.46 dB**.
- Bitrate (over 1.0 s): rust 2359 B / 49 frames / 18.87 kbps (payload 2016 B, hdr 14.54%); c 2318 B / 49 frames / 18.54 kbps (payload 1975 B, hdr 14.80%).

### sine-stereo-44100 (44.1 kHz, stereo; ch0 tabulated, ch1 identical)

| Signal | Samples/ch | Peak | Clipped | RMS | All-zero |
|---|---|---|---|---|---|
| orig | 44100 | 9999 | 0 | 7070.605 | no |
| rust-dec | 44395 | 10124 | 0 | 6939.406 | no |
| c-dec | 44395 | 10074 | 0 | 6938.826 | no |

- rust-vs-orig: lag −1600, overlap 42795, err RMS 12.37, max abs err 292, SNR **55.14 dB**.
- c-vs-orig: lag −1600, overlap 42795, err RMS 12.30, max abs err 278, SNR **55.19 dB**.
- rust-vs-c: lag 0, overlap 44395, err RMS 5.08, max abs err 169, SNR **62.71 dB**.
- Bitrate (over 1.0 s): rust 2998 B / 45 frames / 23.98 kbps (payload 2683 B, hdr 10.51%); c 3014 B / 45 frames / 24.11 kbps (payload 2699 B, hdr 10.45%).

No non-finite values are possible in the i16 working format (not applicable, verified by construction). No clipping indicators anywhere. No all-zero decoded output. (sine rust bitrate corrected: 2998 B × 8 / 1.0 s = 23.98 kbps.)

## 3. Interpretation (explicitly not quality proof)

- Codec error dominates everywhere: rust-vs-orig and c-vs-orig SNRs agree within 0.5 dB on all three cases (35.84 vs 35.93; 58.10 vs 57.62; 55.14 vs 55.19). The Rust path is, by these blunt metrics, indistinguishable from the real C encoder.
- Inter-implementation residue sits 17–40 dB below codec error (rust-vs-c SNR 53–63 dB vs codec SNR 24–58 dB), consistent with the D-021 finite-precision account; max abs rust-vs-c errors (679/50/169 LSB) are two orders below signal peaks.
- Higher SNR here must NOT be read as "better perceptual quality": SNR weights all error equally, ignores masking, and the click case (35.9 dB, transient smearing with peak 30000 → ~23100) shows how a single number hides the audible-relevant behavior. These numbers are a regression baseline, not a quality verdict.
- Bitrates agree within ±2.2% with identical frame counts (25/49/45 both encoders); header overhead 6–15% is pure ADTS framing, separated from payload above.

## 4. Repeatability

- Rust encoder across runs/days: re-running `m5a_decode_check` for this milestone reproduced all three M5.a ADTS hashes bit-for-bit (`a9d7b395…`, `702c8717…`, `aa906b52…`). Deterministic.
- C encoder: two consecutive `aac-enc` runs per input → byte-identical ADTS (`cmp` clean). Deterministic under this build.
- Decoder across runs/days: fresh `aac-dec` runs on the regenerated Rust ADTS reproduced all three M5.a decoded hashes bit-for-bit (`6a359695…`, `4d729444…`, `aecb7b97…`). Deterministic under this build.
- Metrics are pure functions of the files above; re-running `/tmp/m5b-metrics.py` reproduces `/tmp/m5b-metrics.json` exactly (inputs pinned by SHA-256 in §1).

## 5. Alignment and delay methodology (read before trusting §2)

- Signals compared as i16 integers, no normalization, no resampling, no windowing. Channel order native (interleaved L/R preserved end to end; stereo fixtures are dual-mono so ch0 ≡ ch1 — reported once, verified both).
- Nominal end-to-end lag −1600 (decoded[1600:] ≈ orig[0:]) applied to all orig comparisons. Justification: (a) matches the documented 1600-sample end-to-end alignment (M5.0 audit §4, spec edit); (b) exact correlation argmax on click-mono and sine-44100; (c) decoder trim arithmetic (M5.a implied trims 1744/1744/1685 vs 1600-sample frontend delay).
- Onset alias trap (why the raw argmax was overruled): unconstrained correlation on onset-stereo peaks at lag −1504 (SNR 23.78 dB, max err 10021 = full-scale edge misplacement), not −1600. The 1 kHz tone period is 48 samples; lags ≡ −1600 (mod 48) are near-tied over the 24000-sample silent half, and priming-transient correlation tips the peak to an alias two periods off. Edge alignment decides: at −1600 the onset-edge error collapses 10021 → 135 and SNR rises to 58.10 dB. A dropped guard here would have manufactured a 34 dB phantom "divergence" — the Phase-3 alignment-first rule in action. Rust-vs-c lag is 0 (argmax, all cases): both decoded streams share identical delay structure, so that comparison is alignment-trivial.
- Overlap = maximal at the stated lag (tail samples excluded, never zero-padded into the comparison). Front priming (1600 samples) and trailing (1744/1744/1685-sample equivalent) regions are excluded from error metrics — a documented limitation: errors in trimmed regions are unmeasured.
- Bitrate duration = original fixture duration S/sr (0.5 s / 1.0 s / 1.0 s). Encoder delay (1600), decoder trim, and flush frames are NOT added to the duration; ADTS-header bytes are separated from payload (§2). Alternative conventions would shift kbps by ~±7%; the convention is stated so future comparisons stay commensurable.

## 6. Limitations and unavailable references

- No mono-44.1k case: blocked test infrastructure (M5.a §2), not an encoder failure; no substitute fixture generated. This baseline covers 3 of the 4 supported-matrix cells.
- No perceptual metric (PE, NMR, masking model), no listening/ABX: out of scope by design.
- SNR/SDR here are blunt full-band error ratios (§3 limits apply); silence-heavy fixtures were NOT excised, so quiet-region behavior is folded into the single numbers — retained raw per instruction, not massaged.
- Trimmed regions (front 1600, tail ≈1744/1685) excluded from comparison; flush-payload correctness remains open (M5.0 risk #3).
- Untested paths from the M5.0 audit (retry/crash/fill, PNS/IS, LO-TNS, >2ch, other rates) remain untested; C reference built from the same revision with unpinned Apple-clang toolchain (versions §1).
- Only short synthetic fixtures; no speech/music content in this baseline (licensed WAVs exist in-repo but were out of the M5.b scope).

## 7. Recommendation

Next bounded milestone: **M5.c flush/termination + skip-policy validation** (M5.0 risk #3 — the one gap this baseline cannot close): per-frame payload ground truth for final frames on these three fixtures, enumerate skip triggers on real content, and decide handle-vs-abort per path with tests. Rationale: §2–§4 show the steady-state path is measured and reproducible; the unmeasured regions are exactly the trimmed/flush edges and the loud-skip paths. M5.b numbers become the regression floor it must not move (modulo documented alignment).

## 8. Worktree confirmation

- Branch `main`, HEAD `53481f3a` (unchanged). No staging, commits, stashes, resets, cleans, restores, or amends.
- Only intended additions: `rust/src/aac_enc/m5b_measure.rs`, one test-gated registration line in the already-modified `rust/src/aac_enc.rs`, and this report. All 28 pre-existing status lines verified untouched at start (26-count baseline + 2 M5.a files); final status shows only those plus the 2 new M5.b files.
- Production encoder code, existing fixtures, and existing tests unmodified (`m5a_decode.rs` untouched; M5.a artifacts reused read-only). `aac-dec.c`/`aac-enc.c` unmodified. Generated audio lives only in `$TMPDIR/m5a-decode/` and `$TMPDIR/m5b-quality/`; build trees only in `/tmp/m5a-build/`.

## 9. V3 full-corpus successor baseline (2026-10-24/25, HEAD `3878c45`)

This section supersedes §§1–7 as the current quality/performance baseline; the M5.b
3-fixture numbers above are retained as the method-validation record (the V3 pilot
reproduces them exactly: 35.84/35.93, 58.10/57.62, 55.14/55.19 dB). Method, conventions,
and limitations below are unchanged unless stated; the corpus is the 19-fixture V3 set
(`aac-enc-benchmark-plan.md` §1 documents provenance; committed resynthesis formulae in
`rust/src/aac_enc/testdata.rs`, licensed WAVs + `m55-PROVENANCE.txt`/`m8-PROVENANCE.txt`).

- **Scope:** all 19 `M58_FIXTURES` (15 synthetic + m6 stress + 3 licensed + m8 eroica).
  Mono-44.1k is absent from the corpus by design (unsupported per M5.a/M5.e), not a failure;
  `m8-music-eroica` is PROVISIONAL pending independent provenance review.
- **Builds:** pinned reference `f14ad40` (Release `aac-enc`/`aac-dec`, Apple clang 21.0.0,
  cmake 4.4.4); Rust `cargo test --features aac_enc` (dev profile for quality — deterministic
  bytes; `--release` for perf), rustc/cargo 1.99.1, no RUSTFLAGS.
- **Input identity:** Rust in-memory PCM vs C-side WAV bit-identical on 19/19 (FNV-1a64 ==
  harness `input_hash`); m5b hashes reproduce §1 exactly.
- **Validation:** 38/38 reference decodes (exit 0, empty stderr) + 38/38 FFmpeg `decode_ok`
  (9.0.2). Frame counts and decoded lengths rust==c on all 19. Zero clipped samples except
  stress (22/22 both — full-scale content, identical).
- **Quality (lag −1600 nominal, maximal overlap, both channels; rust-vs-c at lag 0):**
  bitrates within −2.5%/+2.0% of C (18/19 within the old descriptive ±2.2% band; sweep
  −2.45% is the sole outlier — the band is descriptive, not a gate); codec-error SNR
  agreement ≤1.3 dB (17/19 ≤0.5 dB; onset441 1.27 dB *favoring Rust*; sweep 0.47 dB);
  residue (rust-vs-c) 33–64 dB, i.e. 6–40 dB below codec error; silence/edge bit-identical,
  stress bytes identical. SNR/SDR remain blunt full-band ratios — no perceptual claim.
- **Performance (5 reps/config, medians, outliers kept):** Rust encoder-core median **1.34×**
  slower than C (range 1.13–1.75×; silence worst at 1.75×); aggregate 223.8× vs 293.7×
  realtime (51.5 s audio); worst case music 159× realtime Rust. C end-to-end timing is
  order-of-magnitude only (`/usr/bin/time -p`, 10 ms granularity); the ns-resolution
  `c-enc-bench` column carries the comparison. Timed outputs byte-identical to quality-run
  outputs (release==dev bytes).
- **Alignment discipline (extends §5):** coarse-to-fine correlation candidate (±2048, stride
  64 → refine ±96) with acceptance always by full-rate SNR evidence at nominal −1600.
  Raw candidates that lost on SNR evidence: onset48 544, multitone 640, quiet-sine 544,
  click −80/−128 (rust/c — raw is unstable across impls), edge/silence range-edge −2048.
  (The V3 pilot's exact full-rate scan gave onset48=1600; near-tied period-48 aliases differ
  by scan, the nominal decision does not.) 44.1 kHz raw agreed (−1600 both fixtures) on the
  weaker two-fixture basis. Two frame coordinates are distinguished where needed: causal
  (orig sample *i* encoded in output frame ⌊i/1024⌋) vs decoded-audio (frame *f*'s audio ↔
  orig [f·1024−1600−D, …), D=1744 @48 kHz / 1685 @44.1 kHz, ±1-frame MDCT slop).
- **Limitations carried forward:** trimmed regions excluded; flush-payload stage attribution
  open (M5.c gap, M5.d bounds only); 44.1 kHz basis thin; no perceptual evidence.
- **Scratch provenance:** full data/logs under `$TMPDIR/v3-full-rvehNNkF/` (metrics JSON,
  perf JSON, 38 ADTS+WAVs, build logs) and the pilot under `$TMPDIR/v3-XM50vmgW/` are
  **temporary** and may be deleted. Durable conclusions (the numbers above, the method,
  the limitations) survive deletion; re-measurement needs only the committed fixtures,
  the existing harnesses (`m5b_dump_inputs`, `m58_corpus_validation`, `m52` paths), and a
  fresh pin-clone build per the recorded commands.
- **Direction (evidence, not a gate):** throughput (≥142× realtime worst case) does not
  justify a performance/SIMD campaign; the measured 1.34× gap does not justify
  hand-vectorization risk. Next milestone: bounded correctness (sweep bitrate, onset441
  gap), then quality; see the V4 record appended to
  `aac-enc-quality-divergence-audit-m5.25.md` §12.
