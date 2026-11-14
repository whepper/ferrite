# M5.a — Rust AAC Decode-and-Check Report

Validation only. No perceptual-quality claim, no production API change.

**Verdict: `M5.a PARTIAL — THREE CASES PASS, MONO-44.1k BLOCKED BY MISSING TEST INFRASTRUCTURE`**

## 1. Decoder provenance and verified invocation

- Source: `aac-dec.c` (repo root, Apache-2.0 header, Martin Storsjö). Built with `wavwriter.c`, linked against the in-repo `libfdk-aac`. No modification; `aac-dec.c` was not touched.
- Build (outside the repo, nothing written to the worktree):
  `cmake -S . -B /tmp/m5a-build -DBUILD_PROGRAMS=ON -DCMAKE_BUILD_TYPE=Release && cmake --build /tmp/m5a-build --target aac-dec -j 8`
  (build flags per `CMakeLists.txt:493-522`, `Makefile.am:41-50`). Toolchain: Apple clang 21.0.0 (`/usr/bin/cc` per CMakeCache), cmake 4.4.4, rustc 1.99.0. Binary SHA-256: `c5d5ff8dd0b8b91eadb8e0e5df90082bde447a262b92b2b31f2793abdf83efd8`.
- Verified CLI contract from source (`aac-dec.c:26-47,58-139`): usage `aac-dec in.aac out.wav`; exit 1 on wrong args, unreadable input, or missing stream info; `.loas` suffix selects LOAS transport, otherwise ADTS with per-frame header parse. Output: 16-bit PCM WAV, skipping `outputDelay × channels` samples at stream start (`aac-dec.c:117-129`).
- **Exit-code caveat (load-bearing):** `aac-dec` returns 0 even when it hits a mid-stream `Fill failed` / `Decode failed` / `Partial packet` condition — it prints to stderr and `break`s out of the loop (`aac-dec.c:77-103,131-136`). A clean exit status alone is therefore NOT success. The harness requires exit 0 **and** empty stderr **and** structural WAV validation. Licensing: invocation only; no redistribution.
- The C++ encoder oracle was not used as a decoder or as ground truth anywhere in this milestone.

## 2. Test matrix and fixtures

Harness: `rust/src/aac_enc/m5a_decode.rs` (new, `#[cfg(test)]`-only; registered by one test-gated line in `rust/src/aac_enc.rs`), tests `m5a_decode_check` and `m5a_mono44100_blocked_by_missing_cfg`. Generation reuses the existing paths: `stream_driver::m52_write_adts_stream` (`stream_driver.rs:4029`) → `PcmFeeder` + `StreamEncoder::step_frame`, with the loud skip/note policy (`m52_emit_or_fail`). Output dir: `$TMPDIR/m5a-decode/` (outside the repo; on this machine `/var/folders/*/T/m5a-decode/`).

| Case | Fixture source | SR/Ch | ADTS identity |
|---|---|---|---|
| mono-48k | `click-mono` resynthesis (`testdata.rs:303`) via existing path, stem `m2-click-mono-48k` | 48000/1 | 25 frames, 2680 B, SHA-256 `a9d7b395…ae2bb3` |
| stereo-48k | `onset-stereo` resynthesis via existing path, stem `m2-onset-stereo-48k` | 48000/2 | 49 frames, 2359 B, SHA-256 `702c8717…f06eb1` |
| stereo-44.1k | `sine-stereo-44100` resynthesis via existing path, stem `m3-sine-stereo-441k` | 44100/2 | 45 frames, 2998 B, SHA-256 `aa906b52…0ca6a52` |
| mono-44.1k | **BLOCKED — no run.** No committed mono-44.1k fixture key exists (`testdata.rs` has only `sine/onset-stereo-44100`), and `StreamEncoder::new(44100, 1)` cannot build tables: it calls `load_cfg(44100, 112000)` (`stream_driver.rs:516`) but only `cfg_44100_96000.txt` is committed, so the `expect` at `testdata.rs:491` panics. Deriving left-channel mono PCM is possible (recipe preserved as `emit_pcm_as_adts` in the harness), but the missing config snapshot blocks the existing path. | 44100/1 | n/a |

## 3. Per-case results (runs actually executed)

Command: `AAC_DEC_BIN=/tmp/m5a-build/aac-dec cargo test --features aac_enc m5a_ -- --nocapture` (run from `rust/`; only the 2 `m5a_` tests ran — no broad suite). Result: `2 passed; 0 failed`.

| Case | Encode | ADTS framing | Decode exit / stderr (run 1, run 2) | WAV ch/sr/bits | Samples/ch (granted → decoded) | Non-silent | Determinism (dec1 vs dec2 SHA-256) |
|---|---|---|---|---|---|---|---|
| mono-48k | ok, 25 frames | 25 headers, lens sum 2680 | 0 / empty, 0 / empty | 1 / 48000 / 16 | 25600 → 23856 | yes (nonzero bytes) | identical, `6a359695…b1a263e` |
| stereo-48k | ok, 49 frames | 49 headers, lens sum 2359 | 0 / empty, 0 / empty | 2 / 48000 / 16 | 50176 → 48432 | yes | identical, `4d729444…ff1325` |
| stereo-44.1k | ok, 45 frames | 45 headers, lens sum 2998 | 0 / empty, 0 / empty | 2 / 44100 / 16 | 46080 → 44395 | yes | identical, `aecb7b97…6fc8fdd` |
| mono-44.1k | blocked (see §2) | — | — | — | — | — | — |

Decoded WAV sizes: 47756 / 193772 / 177624 B. All WAVs: `RIFF/WAVE/fmt /data`, `data_len == file_len − 44`, data length frame-aligned (`% (2·ch) == 0`), channel count and sample rate match the encode configuration. i16 PCM has no non-finite representation; not applicable. No decoder warnings, truncation, or structural anomalies in any executed case.

## 4. Acceptance criteria and classification

Criterion (fixed before interpretation): pass requires successful encode, nonempty correctly-framed ADTS, decoder exit 0 **with empty stderr**, nonempty WAV with expected ch/sr, plausible sample counts under documented trim semantics, byte-identical repeat decode, no unexplained diagnostics.

- mono-48k: **PASS** (all criteria met).
- stereo-48k: **PASS** (all criteria met).
- stereo-44.1k: **PASS** (all criteria met).
- mono-44.1k: **BLOCKED** (infrastructure missing; no encode or decode attempted — not a decode failure).

No case required decoded PCM to match source PCM; AAC is lossy and no such comparison was performed.

## 5. Known decoder delay/padding behavior

Implied front-trim `D' = frames·1024 − decoded_samples/ch`: **1744** samples/ch on both 48 kHz cases, **1685** on the 44.1 kHz case. The 48 kHz value reproduces the prior independent measurement (`outputDelay` 1744, per the M5.0 audit §4) exactly on two unrelated fixtures — a corroboration, not a new claim. The 44.1 kHz value (1685) is recorded as observed; its interpretation (outputDelay vs padding interaction) is unresolved and no exact expected count was invented. The harness bounds `0 < D' < 4096` rather than asserting an exact figure. Trailing behavior: `aac-dec` stops at short final read with no diagnostic; no end-trim anomaly was observed (all counts within one frame of granted).

## 6. Reproduction

Prerequisites: repo at HEAD `53481f3a`, `AAC_DEC_BIN` pointing at the build below, `cargo` with `--features aac_enc`.

```sh
cmake -S . -B /tmp/m5a-build -DBUILD_PROGRAMS=ON -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/m5a-build --target aac-dec -j 8
cd rust
AAC_DEC_BIN=/tmp/m5a-build/aac-dec cargo test --features aac_enc m5a_ -- --nocapture
```

Generated streams, decoded WAVs, and logs live only in `$TMPDIR/m5a-decode/` (9 files: 3 ADTS + 6 WAV). Nothing generated is stored in the repo. The harness is repeatable and yields an unambiguous per-case verdict (`ok` / loud panic naming stem, frame, and cause). Implemented harness code (`m5a_decode.rs`) is distinct from the runs above; only the runs in §3 are claimed as executed — the mono-44.1k recipe (`emit_pcm_as_adts`) is implemented but unexecuted, and is claimed only as a recipe.

## 7. Limitations and recommended next milestone

- Coverage is 3 streams, not a corpus; no quality metric (SNR/SDR), no bitrate histogram, no clipping check — all deferred to M5.b by design.
- Untested paths from the M5.0 audit remain untested (retry/crash/fill, PNS/IS, LO-TNS, >2ch, other rates, flush-payload ground truth).
- Decoder build is unpinned beyond the versions in §1; `verify` of decode determinism holds only "under the same decoder build and settings."
- `aac-dec`'s silent-nonzero-exit behavior (§1) means any future consumer of this decoder must keep the stderr-empty check; it is asserted in `run_decode`, not just documented here.
- Recommended next: **M5.b delay-compensated PCM comparison baseline** on the three passing streams (plus licensed in-repo WAVs once aligned): record per-fixture SNR/SDR, avg bitrate, and frame-size histograms for Rust and C side-by-side, metrics only, no pass/fail thresholds. Unblocking mono-44.1k (commit `cfg_44100_112000.txt` + fixture key via the oracle path, then promote the case into `CASES`) is a separable small task and explicitly not folded into M5.b.

## 8. Worktree confirmation

- Branch `main`, HEAD `53481f3a` (unchanged). No staging, commits, stashes, resets, cleans, restores, or amends.
- Only intended changes: new `rust/src/aac_enc/m5a_decode.rs`, one test-gated registration line in the already-modified `rust/src/aac_enc.rs`, and this report. All 25 pre-existing status lines plus the M5.0 audit file remain untouched; final `git status --short` shows 26 pre-existing lines + the 2 new M5.a files, `git diff --stat` still 11 files +1567/−280 for the pre-existing implementation work.
- Temporary files created only under `/tmp/m5a-build/` and `$TMPDIR/m5a-decode/`; no repo artifacts removed or modified. `aac-dec.c` unmodified.
