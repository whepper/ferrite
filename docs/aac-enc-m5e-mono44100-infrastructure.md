# M5.e — Mono/44.1 kHz Infrastructure Restore and Validation

Infrastructure restoration + validation. No encoder tuning, no API work, no listening tests.

**Status: `M5.e COMPLETE — MONO/44.1 kHz CASE VALIDATED`**

## 1. Baseline (Phase 0)

- Branch `main`, HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (verified before and after; unchanged).
- Initial `git status --short`: 32 lines (11 modified implementation files + M5.0/M5.a/M5.b/M5.c/M5.d reports + `phase1b.rs`, `stream_driver.rs`, `m5a_decode.rs`, `m5b_measure.rs`, 5 testdata WAV/provenance files, `tools/c-enc-bench/`, `tools/enc-oracle/__pycache__/`). Index empty; nothing staged, committed, or otherwise manipulated at any point.
- Deliverable `docs/aac-enc-m5e-mono44100-infrastructure.md` verified absent before writing.
- M5.d findings impact: **F1** (no per-frame fill gate) — no interaction, no fill logic touched, new case emits with zero notes under the loud policy. **F2** (mono-44100 blocked) — resolved by this milestone. **F3** (M55 default C-ref dir holds 1/4) — no interaction; the M55 suite was not rerun, and the C reference for the new case was generated fresh via the established `aac-enc` procedure (M5.b §1).

## 2. Configuration restoration (Phase 1)

**Established path used end to end** (`tools/enc-oracle/README.md` "Build" + "Fixtures and runs" + `extract_testdata.py`):

1. Reference library already built at `/tmp/m5a-build` (`libfdk-aac.dylib` present from the M5.a/M5.b builds).
2. Oracle built with the documented command verbatim: `c++ -std=c++11 -O2 -I cpp/libAACenc/src -I cpp/libFDK/include -I cpp/libSYS/include -I cpp/libMpegTPEnc/include -I . tools/enc-oracle/enc_oracle.cpp wavreader.c -L /tmp/m5a-build -lfdk-aac -Wl,-rpath,/tmp/m5a-build -o /tmp/m5a-build/enc_oracle` (exit 0). No oracle source touched.
3. Input WAV is the M5.b-provenance dump itself (`$TMPDIR/m5b-quality/inputs/onset-mono-44100.orig.wav`, SHA-256 `df25ddc7…9a80c6`, written by the test-gated `m5b_dump_inputs` from the new fixture key — byte-identical by construction to what the Rust path consumes). Oracle run exit 0; dump header: `sr=44100 ch=1 … chBitrate=112000` — exactly the missing operating point (mono → totalBr 112000 → chBitrate 112000 per `enc_oracle.cpp:363-370`).
4. Extraction via a scratch copy `/tmp/m5e-extract-cfg.py` differing from the repo script in exactly two documented places: `CFG_SOURCES` restricted to the new stem, and the per-frame committed-vector loop stubbed (`{}.items()`) so existing vectors are never rewritten. The `parse()` + config-write code path is byte-identical (verified by diff).
5. Output `cfg_44100_112000.txt` (4281 B, 18 lines, SHA-256 `ba2a2ee8…0745`): all 18 structural tags present; diff vs the committed `cfg_44100_96000.txt` shows **only** the two MINSNR lines differing (LONG + SHORT) — precisely the expected 112000-vs-96000 operating-point difference. Nothing hand-authored or guessed.
6. Installed by copy to `rust/src/aac_enc/testdata/cfg_44100_112000.txt` (hash identical after copy).

**Fixture key** (`rust/src/aac_enc/testdata.rs`, test-gated module): new `onset-mono-44100` arm mirroring the `onset-stereo-44100` closure over `(0..1)` — mono twin of the M5.a recipe's source formula. `fixture_sample_rate` resolves 44100 via the existing `-44100` suffix convention; no convention changes.

**CASES promotion** (`rust/src/aac_enc/m5a_decode.rs`): 4th entry `(short "onset-mono-44100", stem "m3-onset-mono-441k", 44100, 1)` running the normal `m52_write_adts_stream` path. Obsolete blocker-only treatment removed as the brief authorizes: the `m5a_mono44100_blocked_by_missing_cfg` test and the `emit_pcm_as_adts` recipe (plus its now-unused imports and the `derived_mono` field/branch) are deleted — the recipe's purpose is fulfilled, not merely hidden.

## 3. Validation results (Phase 2)

Commands (from `rust/`, existing harness only): `AAC_DEC_BIN=/tmp/m5a-build/aac-dec cargo test --features aac_enc m5a_ -- --nocapture` (run twice); `cargo test --features aac_enc m52_all_fixtures_write_complete`; C encode + decodes via `/tmp/m5a-build/aac-enc` / `aac-dec` with exit+stderr gates; metrics via `/tmp/m5b-metrics.py` (case list extended outside the repo). One inline-shell quoting bug during C-encode/decode produced crossed filenames; the size check caught it immediately, files were deleted, and the run was redone via the auditable `/tmp/m5d-run.sh`-style explicit script — only the clean run is reported.

- **New case gates (M5.a criteria, all executed):** encode ok, 45 frames; ADTS 2012 B, SHA-256 `c3b8b5fd…048868`; framing parses exactly; decode exit 0 + empty stderr on both runs; WAV ch=1/sr=44100/16-bit, N=44395 (D′=1685, same as stereo-44100); dec1==dec2 bytes in-test; nonzero output. **PASS.**
- **Reconciliation (M5.d rule `tail = (1600+D) − Z`):** S=44100 → F=ceil(45700/1024)=45 ✓; Z=1980 (68 pending + 956 + full-zero frame); G=46080; N=44395; tail=44100−42795=1305 = 3285−1980 ✓. No unexplained loss/duplication. **PASS.**
- **Regression:** existing three ADTS hashes unchanged (`a9d7b395…`, `702c8717…`, `aa906b52…`); existing three input WAVs unchanged; m52 suite 15/15 **ok** (zero notes/skips — no trigger introduced); M5.b metrics JSON for the three old cases unchanged (verified by equality, §4). **PASS.**
- **Determinism:** new ADTS hash identical across both runs; new decoded WAV identical across runs (in-test both runs + cross-run `cmp` clean, SHA-256 `b8b33083…fad13`). **PASS.**
- **M5.b integration (additive only):** existing three metric sets byte-identical; new case — rust 2012 B / 45 frames / 16.10 kbps (payload 1697 B, hdr 15.66%) vs C 2018 B / 45 frames / 16.14 kbps (payload 1703 B, hdr 15.61%, C SHA-256 `24f9b6bb…ff83`); integrity peak 9999→10053/10028, RMS 4999.7→4831.5/4830.3, zero clips; rust-vs-orig SNR **57.98 dB** (err 6.22, max 80), c-vs-orig **57.09 dB** (err 6.88, max 74) at nominal lag −1600 / overlap 42795; rust-vs-c lag 0, SNR **57.51 dB** (err 6.44, max 89). Observation, not a defect signal: here the inter-implementation residue sits at the codec-error level (unlike the 17–40 dB margin on the M5.b trio), but absolute errors remain minute (≤89 LSB on a peak-10000 signal, the smallest of all four cases) with lag-0 alignment and no systematic sign. Recorded raw; no tuning undertaken and none recommended on this evidence.

## 4. Gaps, warnings, failures

- None blocking. The M5.a report's mono-44100 `BLOCKED` classification and the M5.c gap #3 are closed; the M5.d §3 inventory note for the 44.1k-mono cell is superseded (cell now measured).
- Standing limitations carried forward unchanged: F1 (fill gate), perceptual metrics (`QUALITY UNCERTAINTY`), real-content signal numbers, >2ch/other rates, release criteria/CI.
- `tools/enc-oracle/extract_testdata.py` was deliberately **not** modified (no `CFG_SOURCES` change in-repo); a one-line follow-up adding `m3-onset-mono-441k` to `CFG_SOURCES` would make regeneration native. Left undone to keep this milestone strictly additive — recommended as a trivial follow-up, not a gap.

## 5. Final worktree and change confirmation

- HEAD `53481f3a`, branch `main`; index empty (0 staged). Final `git status --short`: 33 lines = 32-line baseline + this report; pre-existing lines verified unchanged (only additions).
- Files changed, all in scope: (1) `rust/src/aac_enc/testdata/cfg_44100_112000.txt` — new, generated via the established path; (2) `rust/src/aac_enc/testdata.rs` — new fixture arm only; (3) `rust/src/aac_enc/m5a_decode.rs` — CASES promotion + authorized removal of obsolete blocker code; (4) `rust/src/aac_enc/m5b_measure.rs` — INPUTS +1 entry (existing three dumps untouched); (5) this report — new.
- No production code, existing fixtures, M5.b baselines/hashes/thresholds, oracle behavior, or pre-existing work modified, overwritten, staged, or committed. Generated audio and build artifacts live only in `$TMPDIR/m5b-quality/`, `$TMPDIR/m5a-decode/`, `$TMPDIR/m5d/`, `$TMPDIR/m5e-oracle/`, `$TMPDIR/m5e-testdata/`, `/tmp/m5a-build/`; scripts in `/tmp/m5e-extract-cfg.py`, `/tmp/m5b-metrics.py`. D-021 untouched.
