# M6.1 — Production Encoder API Implementation Report

Implementation milestone. No codec tuning, no threshold changes, no corpus expansion.

**Status: `M6.1 COMPLETE — PUBLIC API IMPLEMENTED AND COMPATIBILITY GATES PASS`**

## 1. Initial state (Phase 0)

- Branch `main`, HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (verified before and after; unchanged).
- Initial `git status --short`: 41 lines (11 modified implementation files + M5.0–M5.j reports + test-gated harness files + `cfg_44100_112000.txt` + testdata WAVs/provenance + `tools/c-enc-bench/`, `tools/enc-oracle/__pycache__/`). Index empty.
- Contract `docs/aac-enc-m6-api-contract.md` verified present and read; deliverable verified absent before writing.
- Nothing staged, committed, stashed, reset, cleaned, restored, amended, or pushed at any point.

## 2. Files changed and why

| File | Change | Purpose |
|---|---|---|
| `rust/src/aac_enc.rs` | Removed `#[cfg(test)]` from `mod stream_driver` (+ rationale comment); added `pub mod encoder;`, `pub mod psy_config;` | Expose the pipeline to production; register new modules |
| NEW `rust/src/aac_enc/psy_config.rs` | Verbatim move of `testdata_path`, `parse_kv_list`, `CfgSnap`, `TnsCfgSnap`, `load_cfg` from `testdata.rs` | Production needs init-time config loading (`StreamEncoder::new`); keeps 7.7 MB of embedded test WAVs out of production builds |
| `rust/src/aac_enc/testdata.rs` | Removed moved items; added `pub(crate) use super::psy_config::{...}` re-export | Zero test call-site changes (all 15+ `testdata::load_cfg` users keep working) |
| `rust/src/aac_enc/stream_driver.rs` | `use super::testdata` gated `#[cfg(test)]`; `load_cfg` call → `psy_config::`; `#[cfg(test)]` on `M55_FIXTURES`, `CensusRow`; new `StepError` enum + `snapshot.step_error` field + 4 typed capture sites (pure-additive diagnostics); `allow(dead_code)` on 3 capture structs + `b2m`/`b2e`; `#[cfg(test)]` on `init_summary` | Ungate with test-only surface kept test-only; typed error plumbing for the API mapping |
| NEW `rust/src/aac_enc/encoder.rs` | `Encoder`, `AdtsFrame`, `EncodeError`, `EncodeFailure`, `map_outcome`, lifecycle, 7 inline unit tests | The public API (contract §2–5) |
| NEW `rust/src/aac_enc/m6_migration.rs` | 19+1-stream equality harness (driver vs API, byte-compare) | Migration gate (equality, not replacement — driver observability untouched) |
| This report | New deliverable | — |

No production algorithm, threshold, fixture, baseline, oracle, or D-021 content touched. The only behavioral delta vs the test driver is the contract-mandated crash-discard (§6).

## 3. Implemented public surface (matches the approved contract)

`Encoder::new(config) -> Result<Self, EncodeError>` (matrix gate 44.1/48k × mono/stereo × ADTS, else `UnsupportedConfiguration`); `push(&[i16]) -> Result<Vec<AdtsFrame>, EncodeError>` (empty-ok, unaligned-rejected-before-buffering, poison-on-failure with in-call frames discarded and counted); `flush() -> Result<Vec<AdtsFrame>, EncodeError>` (exactly-once); `is_finished()` (finished OR poisoned). `AdtsFrame { bytes, frame_index }`. `EncodeError::{UnsupportedConfiguration, Config, UnalignedInput (recoverable), AlreadyFinished, FrameFailed{frame_index, completed_before, kind}, Poisoned}`; `EncodeFailure::{SkippedFrame{notes}, CrashedFrame{notes}, QcFailed(QcError), FrameWrite(FrameError)}` — with one documented refinement: the contract's `BitEnc`/`Adts` variants are consolidated into `FrameWrite`, because both convert into `FrameError` at the S8 boundary (`frame.rs:385,400` `From` impls; `build_adts_frame` returns `Result<_, FrameError>`) and can never reach the driver as their own types. No silent skips; no handle-and-continue; no threading claims.

## 4. Lifecycle behavior (contract §3, all implemented + tested)

Valid push → frames in order; split input byte-identical to single push (tested); empty push → `Ok(vec![])`; immediate flush emits delay frames (tested: 2 all-zero frames for empty mono input); partial final input padded on flush (tested: 2000 samples → 1 + 3 frames); repeated flush / post-finish push → `AlreadyFinished`, no state change (tested); unaligned push → `UnalignedInput`, nothing buffered, retry succeeds (tested); failure → `FrameFailed` + poison, subsequent calls → `Poisoned` (mapping tested for all 6 outcome shapes; live triggering has no test path by design — no live trigger exists); frame indices sequential 0..n (tested).

## 5. Crash-marked frames (contract §3/§5)

Implemented exactly as approved: bytes-with-notes → discard, `Err(FrameFailed{CrashedFrame})`, poison. Fail-closed ordering in `map_outcome` (any typed error outranks bytes; bytes+notes can never emit). Unit-covered synthetically for every arm including the hypothetical bytes+error shape. Live crash path never fired on any corpus (M5.g), so the discard-vs-emit delta is behavior-preserving on all validated content — proven by the byte-identity gates, not assumed.

## 6. Migration and observability (§3 gate)

The existing test driver is unchanged as a code path (same `m52_write_adts_stream`, snapshots, census fields, notes policy). `m6_api_matches_driver_bytes` drives 19 M5.d streams + the validated mono-44100 case through both paths with identical inputs and asserts byte-identical output plus frame-index order: **20/20 api-identical** (19 × 414-byte-exact + eroica/music streams). Census/test observability untouched (`FrameSnapshot`, `CensusRow`, all notes asserts intact and re-passing).

## 7. Commands, results, exit statuses (all from `rust/`, `cargo test --features aac_enc`)

- `encoder::` — 7 passed, 0 failed.
- `m6_api_matches_driver_bytes` — 1 passed (20 streams).
- `m52_all_fixtures_write_complete`, `m5a_decode_check` (with `AAC_DEC_BIN`), `m5b_dump_inputs`, `m55_streams_match_c_reference` (`M55_C_DIR=$TMPDIR/m5d-c55`), `m5g_branch_census` — all pass.
- `sfb::` (5, incl. moved-`load_cfg` callers), `rate::tests::s6` (20) — all pass.
- `phase1*` (19): 10 pass; **9 fail identically with `fs::read_to_string(...).expect("oracle dump readable")` on `PHASE1_ORACLE_DIR` (default `/private/.../T/opencode/phase0-20261015/oracle`), which does not exist in this environment.** Pre-existing environmental failures: the failing tests read external per-session scratch (Oct-15 oracle dumps), untouched by M6.1; failure sites are dump readers, and testdata-path loads (my moved code) succeed everywhere they are reached (all committed-vector tests pass). No stash/rollback was used to prove this (prohibited); the evidence is the failure location (external path) plus the absence of that path.
- `cargo build --features aac_enc` (non-test) — clean, exit 0. Remaining warnings are pre-existing/unrelated (`process_pair` dead import, `b2m/b2e` pre-existing dead fields, tns/scf/tp_dec helpers); new ungating-specific warnings were closed with scoped `allow(dead_code)`/gates, documented in code.

## 8. Compatibility gates (all pass)

- **Gate B (byte-identical output):** `a9d7b395…` (click), `702c8717…` (onset), `aa906b52…` (sine441), `c3b8b5fd…` (mono44100) — reproduced from both migration and m5a outputs.
- **Gate C (decode):** m5a suite in-test gates pass on all 4 configs; shell decodes of the 4 migration ADTS: exit 0 + empty stderr; WAV hashes match M5.a/M5.e records (`6a359695…`, `4d729444…`, `aecb7b97…`, `b8b33083…`).
- **Gate D (baselines):** metrics re-run JSON identical for all 3 M5.b fixtures (only addition is the M5.e 4th key); 4 input WAV hashes unchanged.
- **Gate E (census + margins):** m5g re-run: 2442 rows, all M5.g asserts pass, tot_fill distribution identical; all margin minima identical to the M5.i record (GLOBAL `max_iter=0`, grant/maxdyn/frame/cap minima 986/1042/984/1045 @ noise-mono f1).

## 9. Blockers, deviations, untested paths

- **Contract refinement (not a blocker):** `EncodeFailure::{BitEnc,Adts}` consolidated into `FrameWrite` with source evidence (§3). No semantics lost (conversion happens inside S8).
- **Crash discard-vs-emit:** contract-mandated, implemented, hash-gated (no validated output changes).
- **Untested by construction:** live skip/crash/QcError via the public API (no live trigger exists anywhere; mapping covered synthetically); Raw/multi-channel paths (rejected at construction); `Sync`/threading (not promised).
- **Not re-verified here (unchanged, out of scope):** D-021 (no new evidence), perceptual quality, broad configs.
- **Recommendation for next:** proceed to API review/use; the remaining validation work (live retry/crash policy evidence, CI pinning, real-content quality, broader configs) is pre-release work per M5.j §5, explicitly not gating correctness of this milestone.

## 10. Final state and non-interference confirmation

- HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91`, branch `main` — unchanged.
- Final `git status --short`: 42 lines = 41-line baseline + this report; all other lines verified identical to `/tmp/m61-status-initial.txt`; index empty (0 staged).
- No staging, commits, stashes, resets, cleans, restores, amends, or pushes. Production encoder math untouched; fixtures, baselines, hash references, oracle, and D-021 untouched. Generated audio stays in `$TMPDIR/m5*/`, `$TMPDIR/m6-migration/`; build trees in `/tmp/m5a-build/`.
