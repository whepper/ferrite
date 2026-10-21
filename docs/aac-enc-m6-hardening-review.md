# M6.2 — Public API Hardening and Integration Review

Integration and lifecycle milestone. No codec algorithm, tuning, bitstream, fixture, baseline, or oracle-output changes.

**Verdict: `PASS WITH FINDINGS`**

Two API-boundary defects were found and fixed (both classifier/state-machine level, zero output change); one contract-premise inaccuracy and several integration risks are documented rather than silently changed. All compatibility gates reproduce byte-identically, and the nine M6.1 "oracle-environment" failures pass 19/19 once the documented oracle procedure is run.

## 1. Worktree protection (initial state)

| Item | Value |
|---|---|
| HEAD | `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` |
| Branch | `main` |
| `git status --short` | 45 lines (pre-existing work, incl. M6.1 files) |
| Staged | empty (`git diff --cached` = 0 lines) |
| Tracked diff | 11 files, 1604 insertions(+), 398 deletions(-) |

`docs/aac-enc-m6-hardening-review.md` did not exist. Pre-edit hashes of the files M6.2 was allowed to touch were recorded:

```
161317d8…  rust/src/aac_enc/encoder.rs
80ddd5f0…  rust/src/aac_enc/m6_migration.rs
e3667155…  docs/aac-enc-m6-implementation.md
```

Both source files are `??` (created in M6.1, untracked), so M6.2 edits inside them are separable from all tracked user work. No tracked file was modified.

## 2. Contract-to-implementation matrix

Reference: `docs/aac-enc-m6-api-contract.md` §2–§5, §3 state table. Line numbers refer to the current `rust/src/aac_enc/encoder.rs` unless noted.

| # | Contract requirement | Implementation evidence | Test evidence | Verdict |
|---|---|---|---|---|
| 1 | `AdtsFrame { bytes, frame_index }`, 0-based order | `:20-25`; index incremented once per attempted frame, `:267-288` | `frame_indices_are_sequential`, `split_input_matches_single_push`, migration harness | Verified |
| 2 | `Encoder::new` accepts only mono/stereo × 44.1/48k × ADTS; else `UnsupportedConfiguration` before `EncoderConfig::new` | `:183-205` (matrix screen precedes reconstruction) | `config_scope_gates` (4 accepted cells; 32k/6ch/8k/ Raw refused) | Verified |
| 3 | `push` borrowed interleaved `i16`, one frame per 1024-sample block, in order | `:211-235`; feeder copies into per-channel buffers (`pcm.rs:241-255`) | `empty_and_partial_flush_behaves`, `split_input_matches_single_push`, 20/20 migration | Verified |
| 4 | Empty `push` is a successful no-op | `:217-219` (lifecycle check first, then empty) | `empty_and_partial_flush_behaves` (`push(&[])` → empty) | Verified |
| 5 | Split input is byte-identical to single push | contiguous feeder state; no per-call framing state | `split_input_matches_single_push` (333-sample stereo chunks vs one push, byte compare) | Verified |
| 6 | Unaligned input rejected before buffering; recoverable; no state change | `pcm.rs:242-250` checks before mutation; mapping `:222-224` | `unaligned_input_rejected_and_recoverable` (rejection, then stream byte-equal to a fresh encoder) | Verified |
| 7 | Partial final input buffered; `flush` pads per frame law | feeder `finish` (`pcm.rs:261-283`) | `empty_and_partial_flush_behaves` (2000 samples → 1 + 3 frames) | Verified |
| 8 | `flush` is exactly-once; empty input flush is a valid stream | `:237-257`; state set `Finished` only on success | `empty_input_flush_emits_valid_stream` (2 delay frames, ADTS header/length checked; separately decoded, §5) | Verified |
| 9 | Repeated flush / input after completion → `AlreadyFinished`, no state change | `:212-216`, `:239-243` | `double_flush_and_post_finish_push_fail` (non-empty and empty post-finish pushes) | Verified |
| 10 | `is_finished` true after successful flush OR terminal error | `:259-261` (`state != Open`) | `double_flush…`, `poisoned_encoder_rejects_all_operations` | Verified |
| 11 | Frame failure → `FrameFailed{frame_index, completed_before, kind}`, in-call frames discarded, encoder poisoned | `:267-288` (single failure arm sets `Poisoned`, reports `out.len()`) | `outcome_mapping_table` (all kinds); poison state exercised directly (gap: no live trigger, §8) | Verified + gap |
| 12 | Any use after terminal error → `Poisoned`, no output | `:212-216`, `:239-243` | `poisoned_encoder_rejects_all_operations` | Verified |
| 13 | No silent skips: `None`/notes → typed failure | `map_outcome` `:296-317` (default arm is `SkippedFrame`, never `Ok(None)` for a batch entry) | `outcome_mapping_table` | Verified |
| 14 | Crash-marked frames discarded, typed as `CrashedFrame` | `:304-306` now classifies on the snapshot's `crash` marker (fix D1) | `outcome_mapping_table` (crash with and without hypothetical bytes) | Fixed |
| 15 | Fail-closed: bytes + error or bytes + notes never emit | arm order `:298-316`: typed error > crash marker > notes/bytes discard > clean emit | `outcome_mapping_table` incl. bytes+error and notes+bytes cases | Verified |
| 16 | Context preserved: index, count, typed source, notes | `EncodeError::FrameFailed` `:50-54`; `EncodeFailure` `:65-77`; `Display`/`Error` impls `:79-128` | `outcome_mapping_table`; compile-level use in tests | Verified (notes on typed variants are carried by the typed error itself; see D4) |
| 17 | Owned, single-threaded; no `Send`/`Sync`/async promise | `Encoder` fields all owned (`:166-171`); no globals/locks | `cargo build`; docs state non-promise | Verified by inspection |
| 18 | Test driver keeps observability; migration is an equality harness, not a replacement | `stream_driver` unchanged as a path; `m6_migration.rs` compares both | 20/20 streams api-identical; census/snapshot tests still pass | Verified |
| 19 | Public surface is feature-gated and production-only | `lib.rs:101-102` gates `aac_enc`; `aac_enc.rs` exposes `pub mod encoder` | Build with/without feature; docs; rlib symbol scan; out-of-crate consumer (§6) | Verified |
| 20 | Crash policy is an API-boundary delta, not a numeric change | classification only; no driver/rate change | 4 hash gates + 20/20 migration unchanged | Verified |

## 3. Defects found, fixes, and regression evidence

### D1 (fixed) — `CrashedFrame` was unreachable; crash frames were misclassified as `SkippedFrame`

M6.0 asserted the test driver "emits-then-fails" on `crash != -1`. The current source does not: the crash note is pushed at `stream_driver.rs:1304-1309` and the frame returns `(None, snap)` at `:1350-1352`. The driver's own invariant (`phase1_full_stream_parity`) asserts `snap.notes.is_empty() == bytes.is_some()`, and `step_frame` has exactly one `Some` return (its final line). Consequently M6.1's `map_outcome` classified crash frames by "bytes + notes", which can never co-occur, so `EncodeFailure::CrashedFrame` was dead and crash-marked frames surfaced as `SkippedFrame`.

Fix (API boundary only, `encoder.rs:274,296-317`): pass the snapshot's existing `crash: i32` into `map_outcome` and classify `crash != -1` as `CrashedFrame`, before the notes-only skip arm. Bytes, if any ever accompany a crash marker, are still discarded. The doc comment now states the actual driver behavior instead of the stale "emit-then-fail" premise.

Regression evidence: 20/20 migration streams byte-identical; four ADTS hashes unchanged; `crash == -1` on all 2442 census rows, so no validated output can change. The synthetic mapping test covers crash with and without bytes.

### D2 (fixed, defensive) — feeder-desync arms returned `Poisoned` without poisoning

`push`/`flush` mapped an impossible `FeederError::AlreadyFinished` (state `Open` implies the feeder is unfinished) to `EncodeError::Poisoned` but left `state == Open`, so `is_finished()` would contradict the returned terminal error if the invariant were ever broken. Both arms now set `Poisoned` before returning (`encoder.rs:227-231`, `:246-252`). Unreachable by construction; covered by inspection, not induction (see gaps).

### D3 (fixed, docs) — stale contract premise

The `CrashedFrame` rustdoc claimed an "emit-then-fail" driver path that does not exist. Corrected in code. The reviewed contract (`docs/aac-enc-m6-api-contract.md:77`) still contains that premise; it is a factual annotation, not a behavior requirement, and was left untouched as a reviewed artifact. Recommend correcting it in the next docs pass.

### D4 (documented, no change) — unreachable/edge semantics kept intentionally

- `EncodeError::Config`: an `EncoderConfig` can only exist in validated form (private fields, validating constructors), so the inner `EncoderConfig::new` cannot fail. The variant is kept because the contract specifies it; it is a type-system-unreachable defensive arm.
- Notes on typed failures: `QcFailed(QcError)` / `FrameWrite(FrameError)` carry the typed source error but not the snapshot notes, which are just `format!("…: {e:?}")` duplicates on those paths. Skip/crash carry notes because they have no typed source. This preserves all non-redundant context without changing the approved variant shapes.

## 4. Lifecycle coverage vs Scope B

| Scope B case | Coverage |
|---|---|
| 1. Fresh encoder accepts input | `empty_and_partial_flush_behaves` |
| 2. Split vs unsplit byte equality | `split_input_matches_single_push` |
| 3. Unaligned rejected pre-buffer, next call correct | `unaligned_input_rejected_and_recoverable` (byte-equal to fresh encoder) |
| 4. Successful flush finishes; no replay | `double_flush_and_post_finish_push_fail` (second flush errors; state unchanged) |
| 5. Repeated flush / post-finish push | same, incl. `push(&[])` after finish |
| 6. Frame failure poisons; no resume | `outcome_mapping_table` + `poisoned_encoder_rejects_all_operations` (state entered directly; no live trigger) |
| 7. Flush-time failure leaves unusable | shares `emit_frames` failure arm with push; flush-after-poison asserted. Live flush failure not inducible — gap |
| 8. Ordered, stable indices across pushes/flush | `frame_indices_are_sequential`, chunked-order assertions in `split_input_matches_single_push`, migration harness |
| 9. Error paths leak no frames | type-level (`Err` carries no frames) + poisoned test asserts no output |
| 10. Empty input | `push(&[])` no-op; `empty_input_flush_emits_valid_stream` (2 frames, ADTS structure) |

Tests added/changed in `encoder.rs`: +`empty_input_flush_emits_valid_stream`, +`poisoned_encoder_rejects_all_operations`, strengthened unaligned/split/double-flush assertions, updated mapping table, ADTS structural helper. Total API unit tests: 9.

## 5. Commands and results (all from `rust/` unless noted)

| Command | Result |
|---|---|
| `cargo test --features aac_enc encoder::` | **9 passed**, 0 failed |
| `cargo test --features aac_enc --doc` | **82 passed** (includes `encoder::Encoder` doctest) |
| `cargo test --features aac_enc m6_api_matches_driver_bytes` | **1 passed**, 20/20 streams `api-identical` |
| `cargo test --features aac_enc m52_all_fixtures_write_complete` | 1 passed |
| `AAC_DEC_BIN=/tmp/m5a-build/aac-dec cargo test --features aac_enc m5a_decode_check` | 1 passed |
| `cargo test --features aac_enc m5b_dump_inputs` | 1 passed |
| `M55_C_DIR=$TMPDIR/m5d-c55 cargo test --features aac_enc m55_streams_match_c_reference` | 1 passed |
| `cargo test --features aac_enc m5g_branch_census` | 1 passed |
| `cargo test --features aac_enc frame::` / `quant::` / `rate::` | 8 / 9 / 20 passed |
| `PHASE1_ORACLE_DIR=… PHASE1_C_DIR=… cargo test --features aac_enc phase1` | **19 passed**, 0 failed |
| Full suite: `… cargo test --features aac_enc` (all env vars set) | **463 passed, 1 failed** — `tp_dec::pce::tests::read` (known pre-existing failure, D-083; reproduces with `--no-default-features --features aac_dec`, file untouched since v3.0.0) |
| `cargo build --features aac_enc` | OK, 9 pre-existing warnings, **0 in M6.2 files** |
| `cargo build` (default `aac_dec`) | OK |
| `cargo build --no-default-features --features aac_enc` | OK (encoder is standalone) |
| `cargo doc --no-deps --features aac_enc` | OK; `target/doc/aac/aac_enc/encoder/index.html` generated |
| `rustfmt --check --edition 2021 src/aac_enc/encoder.rs src/aac_enc/m6_migration.rs` | 0 diffs (M6.2 files formatted) |
| `cargo clippy --features aac_enc --all-targets` | No errors; 0 findings in M6.2 files (2 `needless_range_loop` findings fixed). Crate keeps its pre-existing warning baseline. |

## 6. Public API and feature-gate findings

- Derived public path: `aac::aac_enc::encoder::{Encoder, AdtsFrame, EncodeError, EncodeFailure}` with `features = ["aac_enc"]`. Crate name is `aac` (`rust/Cargo.toml:2`); `aac_enc` is opt-in (default is `aac_dec`). No ambiguity in the current layout.
- Out-of-crate consumer proof: a standalone program using only those paths was compiled against `target/debug/libaac.rlib` (`rustc -L dependency=target/debug/deps --extern aac=…`), ran, and emitted a decodable stream. No test-only module was needed.
- Production asset exclusion: `strings target/debug/libaac.rlib` contains 0 occurrences of `fixture_samples`, `wav_stereo_s16`, or fixture WAV names; `Encoder` symbols are present. The 7.7 MB embedded test WAVs remain test-only.
- Runtime install dependency: table init reads `src/aac_enc/testdata/cfg_{rate}_{per_channel_bitrate}.txt` relative to `CARGO_MANIFEST_DIR` (`psy_config.rs`). This is documented on `Encoder::new` under `# Panics`; embedding or shipping these files is a pre-release packaging task.
- **Risk:** `cfg_44100_112000.txt` (mono 44.1 kHz) is still **untracked** (M5.e addition). A clean checkout would lack it and `Encoder::new(44100, 1, …)` would panic. It must be committed with the M6 line before release.
- No `Send`/`Sync`, callback, async, or threading promise is made; `Encoder` is a plain owned single-threaded value.

## 7. Oracle test-environment gap (Scope D) — resolved

The nine M6.1 failures were environmental, not encoder failures. The required data is fully regenerable with committed tooling (`tools/enc-oracle/`, documented in `tools/enc-oracle/README.md`). The only prerequisite is a reference build with `libfdk-aac` (already present from M5.a at `/tmp/m5a-build`; build command is in the README §Build).

Exact preparation used (scratch only; no repo writes):

```sh
S="$TMPDIR/opencode/m62-oracle"
mkdir -p "$S/wav" "$S/oracle" "$S/c"
python3 tools/enc-oracle/make_fixtures.py "$S/wav"
for w in "$S"/wav/*.wav; do
  stem=$(basename "$w" .wav)
  /tmp/m5a-build/enc_oracle "$w" "$S/oracle/$stem.s1.txt"   # S1 dump = 2nd arg
  /tmp/m5a-build/aac-enc    "$w" "$S/c/$stem.aac"           # C reference ADTS
done
PHASE1_ORACLE_DIR="$S/oracle" PHASE1_C_DIR="$S/c" \
  cargo test --features aac_enc phase1                     # 19 passed, 0 failed
```

Notes for a maintainer:
- `enc_oracle` writes the S1 blocks into the file named by its second argument; naming it `<stem>.s1.txt` is what the tests expect. There is no separate sibling dump.
- `phase1_full_stream_parity` also needs `PHASE1_C_DIR` (C reference `.aac`), which M6.1's report did not mention.
- `PHASE1_OUT` is optional (report output directory).
- No oracle output, expected hash, or fixture was modified; everything was generated into `$TMPDIR`.

Result: **19/19 phase1 tests pass** with the regenerated scratch data. They are classified as *passing with the documented oracle prerequisite*, separately from tests that need no external data.

## 8. Compatibility gates (unchanged)

- **ADTS hashes:** `a9d7b395…` (click-mono), `702c8717…` (onset-stereo), `aa906b52…` (sine-stereo-441k), `c3b8b5fd…` (onset-mono-441k) — all reproduced from the API path.
- **Decode gates:** `m5a_decode_check` passed with the reference decoder; decoded WAV hashes match the M5.a/M5.e records (`6a359695…`, `4d729444…`, `aecb7b97…`, `b8b33083…`); two decodes of the same stream are identical.
- **Empty-input stream (separate check):** 2 ADTS frames / 26 bytes, `aac-dec` exit 0 with empty stderr, mono 48 kHz WAV with 304 samples (`2×1024 − 1744` decoder trim), all-zero content, decode deterministic across two runs.
- **M5.b baseline:** `/tmp/m62-metrics.json` equals the recorded baseline for all three historical cases (`click-mono`, `onset-stereo`, `sine-stereo-44100`); only the additive M5.e key `onset-mono-44100` is new.
- **Census/margins:** 19 streams, 2442 rows, `max iter = 0`, minimum `avail − sum = 984` bits, fill max 7 bits, zero crash, zero `npass > 1`, `is_book`/`noise` false everywhere. Every per-stream `tot_fill (sum/max/nonzero)` matches the M5.g table exactly.
- **Documentation discrepancy (not a code change):** M5.g §5 says "231 frames carry nonzero `tot_fill`", but its own per-stream table sums to **287**, and the re-run artifact reproduces 287. The historical report was not edited; this is a documentation-only inconsistency.

## 9. Remaining gaps and risks

1. **Live failure induction:** no production input deterministically triggers `FrameFailed` (M5.g/M5.h: crash/retry never fire on the corpus). Classification is unit-covered for every arm; the poison state transition is exercised directly. `completed_before` counting and flush-time failure are inspected, not end-to-end tested, by design (no invasive production hook added).
2. **Missing install data panics:** `Encoder::new` panics rather than returning an error if a cfg snapshot is absent. Documented; packaging fix (embed/ship + typed error) is pre-release work.
3. **Untracked mono-44.1k config asset** (`cfg_44100_112000.txt`) must be committed before release (see §6).
4. **Known decoder failure** `tp_dec::pce::tests::read` (D-083) remains the only full-suite failure; unrelated to the encoder and reproducible without the `aac_enc` feature.
5. **Formatting/lint baseline:** pre-existing files still show rustfmt drift; only M6.1/M6.2-created files were formatted. Clippy baseline is not clean crate-wide.
6. **Contract text:** M6.0's crash "emit-then-fail" premise is stale (D3); behavior matches the contract's requirement, not its rationale.

**Recommended next step:** freeze the API surface and commit the M6 line (including `cfg_44100_112000.txt`), then do the pre-release packaging/embedding pass and add a CI job that provisions `PHASE1_ORACLE_DIR`/`PHASE1_C_DIR` via the commands in §7.

## 10. Final state and non-interference accounting

| Item | Value |
|---|---|
| HEAD | `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (unchanged) |
| Branch | `main` |
| Staged | empty (`git diff --cached` = 0) |
| Tracked diff | 11 files, 1604 insertions(+), 398 deletions(-) — identical to the initial baseline |
| `git status --short` | 45 pre-existing lines + this report = 46 |

Files changed by M6.2 (all untracked/new, i.e. no user-owned file touched):

1. `rust/src/aac_enc/encoder.rs` — D1 crash classification, D2 defensive poisoning, docs/doctest, 2 new tests + strengthened assertions, rustfmt.
2. `rust/src/aac_enc/m6_migration.rs` — 2 clippy `needless_range_loop` fixes, rustfmt.
3. `docs/aac-enc-m6-hardening-review.md` — this report.

No staged changes, commits, stashes, resets, restores, cleans, rebases, or pushes. No established hash, baseline, fixture, oracle output, or expected result was modified. Generated audio and oracle data live only under `$TMPDIR/opencode/m62-oracle/`, `$TMPDIR/m5*/`, and `/tmp/m62-*.json`.