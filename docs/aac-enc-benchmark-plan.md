# Native Rust AAC-LC VBR-5 Encoder — Apple Silicon Benchmark Plan (M5.7, unexecuted)

Date: 2026-10-16. HEAD `e4258d2` **plus uncommitted worktree changes** (see §0);
HEAD alone does not represent the tested code. This plan is approved for a
separate execution milestone. No benchmarks were run to produce this document,
and it prescribes no performance targets.

## 0. Code under test (record at execution time)

- Rust: HEAD `e4258d2` + uncommitted changes, verified via
  `git status --porcelain` / `git diff --stat` before each run. At plan time:
  tracked `M rust/src/aac_enc.rs` (+4 test-only mods),
  `M rust/src/aac_enc/psy.rs` (short-block M/S Ld fix + tests),
  `M rust/src/aac_enc/testdata.rs` (stress fixture + WAV loader + M5.5 arms);
  untracked `rust/src/aac_enc/phase1b.rs`, `rust/src/aac_enc/stream_driver.rs`
  (parity/streaming/M5.2/M5.3B/M55 harness), `rust/src/aac_enc/testdata/m7-*.wav`
  + `m55-PROVENANCE.txt`, `tools/enc-oracle/__pycache__/`.
- C reference: Phase-0 `bin/aac-enc` (`in.wav out.aac` VBR-5 ADTS builds in
  `phase0-20261015/aac/`). Record its build identity at execution time
  (source commit, CMake flags, `md5 bin/aac-enc`); keep it a reference, not a
  dependency. If rebuilt, record the exact command.
- Toolchains: `rustc --version`, `cargo --version`, build profile
  (`--release` mandatory for timing), `sw_vers`, `uname -m`, Xcode CLT version.

## 1. Fixture set (18 at M5.7 plan time; **19** from the V3 addition — omit none without pre-declared justification)

- 15 synthetic (`rust/src/aac_enc/testdata.rs::fixture_samples`): silence,
  sine, onset, stepdown (stereo 48 kHz) + click/impulse/edge (mono 48 kHz) +
  multitone/sweep/noise/tonal-noise/quiet-sine (48 kHz) + sine/onset-44100 +
  stress-PRNG stereo. Durations 9.6–48 ksamples (see M5.2 report).
- 2 LibriSpeech `test-clean` speech (CC BY 4.0, attribution in
  `rust/src/aac_enc/testdata/m55-PROVENANCE.txt`): `m7-speech-pauses-…`,
  `m7-speech-clean-…` (10.0 s, 48 kHz dual-mono stereo).
- 1 CC0 music: `m7-music-stereo-48k` (Thoughtness, Kian Moretz, 10.0 s).
- **V3 addition (2026-10-17/18, additive):** 1 lossless real-music reference,
  `m8-music-eroica-stereo-48k` — Beethoven, Symphony No. 3 in E♭ major, Op. 55
  ("Eroica"), I. Allegro con brio; Czech National Symphony Orchestra (credited
  as "Musopen Symphony"), Musopen Kickstarter Project 2012; public domain. 10.0 s,
  48 kHz true stereo, 16-bit by arithmetic right shift of the 24-bit source with
  no dither and no resampling. Provenance, licence, exact conversion command, C
  reference identity and the qualification report's limitations are in
  `rust/src/aac_enc/testdata/m8-PROVENANCE.txt`; qualification report
  `…/T/opencode/v3-music-20261017/V3-MUSIC-FIXTURE-QUALIFICATION.md`
  (SHA-256 `22010c83…`). From this addition the corpus is **19** fixtures; the
  previously recorded 18-fixture identities (`201b292d…`, `27befab0…`,
  `0977ec70…`) remain valid descriptions of the pre-V3 corpus and are
  **historical**. The M5.5 `m7-*` assets, their C references and all
  M5.25/M5.26 evidence are unchanged. The corpus stays **provisional** until the
  updated provenance package passes independent review.
- Record per fixture: SHA-256 of the WAV, sample count, rate, channels
  (hashes in `m55-PROVENANCE.txt` for M5.5 assets, `m8-PROVENANCE.txt` for the
  V3 reference; FNV hashes in parity logs for synthetic).

## 2. Harness (existing; no new encoder code)

- Rust: test-only `StreamEncoder::step_frame` via `m52_write_adts_stream`
  (`rust/src/aac_enc/stream_driver.rs`), which already enforces
  stop-at-first-skip loudly. Drive it from a release-mode example or
  `#[bench]`-style loop added by the execution milestone (test-only).
- C: `time <bin>/aac-enc <in.wav> <out.aac>` per fixture (end-to-end incl.
  file I/O; C offers no separable encoder-only entry).

## 3. Measurement methodology

- Modes (report separately, never mix): (1) **encoder-only** Rust —
  in-memory PCM frames → `step_frame` loop (excludes WAV read and ADTS file
  write); (2) **end-to-end** — WAV read → feeder → encode → ADTS bytes to
  file, for Rust and C alike. Cross-implementation comparison uses mode 2
  primarily; mode 1 is Rust-internal scaling info.
- Warm-up: one untimed full-corpus pass. Then N=5 timed passes over all corpus
  fixtures (single-threaded, fixed order); record wall time per fixture per
  pass with `std::time::Instant` (Rust) and `/usr/bin/time -p` (C).
- Summary per fixture: median wall seconds, min–max spread, throughput =
  input-audio seconds / median wall second (real-time factor ×), Rust/C
  speed ratio from medians. Aggregate: median of per-fixture ratios +
  total-audio/total-wall throughput. No targets; report variability as found.
- Conditions: Apple Silicon Mac, plugged in, no other workload, 5-minute
  idle cooldown before pass 1; record `sw_vers`, chip (`uname -m`), ambient
  note if thermally constrained, and whether any pass is an outlier
  (keep outliers in the data, flagged, never silently dropped).

## 4. Correctness guardrails (every benchmark run)

- Record encoded bytes + frame count per fixture (Rust and C); require
  successful ADTS re-parse of Rust output.
- Decode every Rust stream with FFmpeg (`-f null -`, exit 0 required) and
  record codec/profile/rate/channels vs the C reference.
- Carry the known correctness notes: impulse-f5 16-bit residual (M5.4),
  Tier-B float-lane divergences (Phase 1I), stale SHORT-dump references —
  byte-identity is NOT a pass condition where documented divergences exist,
  and a speedup never reclassifies them as harmless.

## 4a. Decoder alignment contract (48 kHz, measured)

- **Scope.** The constants and rules below were validated on the four measured
  48 kHz fixtures (`m2-onset-stereo-48k`, `m2-click-mono-48k`,
  `m2-impulse-mono-48k`, `m2-edge-mono-48k`). They must **not** be applied to
  the 44.1 kHz fixtures (see the gating note), nor assumed valid for any
  unmeasured stream.
- **Front trim vs residual lag (different quantities).** The pinned `aac-dec`
  removes `info->outputDelay` (1 744 samples at 48 kHz, observed via
  sample-count arithmetic; the CLI does not report the field) from the front
  of its output. The measured common input-to-output lag is 1 600 samples.
  Do **not** re-apply the 1 600-sample trim — it is already accounted for by
  the measured lag. Quality metrics use only the valid overlap.
- **Per-stream valid comparison interval.** Let `N` = input samples per channel,
  `D` = decoded samples per channel, `L` = measured common lag (1 600 at 48 kHz).
  Compare `input[i]` with `decoded[i + L]` only for
  `0 <= i < min(N, max(0, D - L))`. Record
  `tail_shortfall = N - overlap_length`. Use actual measured lengths and lag;
  do not hard-code a tail loss.
- **Fixture-specific tail shortfall.** For the verified 48 kHz path, encoder
  frame padding `P = F*1024 - (N + 1600)` (F = actual AAC frame count) and
  the observed relationship `tail_shortfall = 1744 - P`. The 1 040-sample
  shortfall applies only to the 480 000-sample / 10 s fixtures (`P = 704`);
  it is not a universal pipeline constant. Verify per stream.
- **Stream eligibility and lag validation.** The four synthetic 48 kHz
  fixtures establish the reference alignment result `L = 1600`; they do not
  prove every corpus stream's alignment. Before calculating quality metrics for
  each 48 kHz stream, perform the planned per-stream lag sanity check and
  confirm its measured lag agrees with 1 600 samples within ±1 sample. Use the
  actual decoded sample count `D` and the accepted per-stream lag `L` to
  calculate `overlap = min(N, max(0, D - L))`, comparing `input[i]` with
  `decoded[i + L]` only for `0 <= i < overlap`, and record `N - overlap` as
  the per-stream shortfall. Measuring `D` alone does not validate alignment.
  If the lag check fails, is ambiguous, or cannot be performed reliably,
  exclude that stream from quality metrics until the discrepancy is resolved.
- **44.1 kHz gating.** `sine-stereo-44100` and `onset-stereo-44100` are
  **ineligible for quality metrics** until their alignment is measured in a
  separate, narrowly scoped experiment. Predicted (not measured): limiter
  component 661 samples, total `outputDelay` 1 685 samples. Do not apply the
  48 kHz constants to them.
- **Unmeasured streams.** Any stream whose alignment has not been measured is
  ineligible for quality metrics.
- **Migration spec (verified, cited).** The migration spec's delay-compensation
  wording was corrected and verified against the current file
  (`docs/aac-enc-migration-spec.md`, SHA-256 `07fe0948...`, passage at lines
  242-246): it now distinguishes the encoder nominal delay (1 600), the decoder
  CLI's front trim (`info->outputDelay`, measured 1 744 at 48 kHz), and the
  end-to-end alignment measured at 1 600 for the four named synthetic 48 kHz
  fixtures (not yet established for every corpus stream; 44.1 kHz still
  unmeasured and gated). Cited from the verified file, not from a prior
  report's assertion.

## 5. Report format (one row per fixture + aggregate)

`fixture | input_s | rust_enc_s (med, spread) | rust_e2e_s | c_e2e_s |
rt_factor_rust | rust/c_ratio | rust_bytes | c_bytes | frames |
decode_ok | notes` plus a header block with §0 identities, exact commands,
and the variability/limitation notes. Distinguish encoder-only from
end-to-end columns explicitly.
