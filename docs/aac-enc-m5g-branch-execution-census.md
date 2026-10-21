# M5.g — Test-Only Branch-Execution Census

Instrumentation + validation. No encoder changes, no tuning, no API work.

**Status: `M5.g COMPLETE — CENSUS CAPTURED AND NON-INTERFERENCE VERIFIED`**

## 1. Initial baseline (Phase 0)

- Branch `main`, HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (verified before and after; unchanged).
- Initial `git status --short`: 35 lines (11 modified implementation files + M5.0–M5.f reports + `phase1b.rs`, `stream_driver.rs`, `m5a_decode.rs`, `m5b_measure.rs`, `cfg_44100_112000.txt`, 5 testdata WAV/provenance files, `tools/c-enc-bench/`, `tools/enc-oracle/__pycache__/`). Index empty; nothing staged, committed, or otherwise manipulated at any point.
- Deliverable verified absent before writing. M5.f §7 followed as authoritative; no conflicts with the brief arose (where the brief restates §7, §7 governs).

## 2. 19-stream inventory and provenance

15 M52 resynthesis fixtures (`stream_driver.rs:3982`, `M52_FIXTURES`: silence/sine/onset/stepdown/click/impulse/edge/multitone/sweep/noise/tonal-noise/quiet-sine stereo+mono 48k, sine/onset stereo 44.1k, stress) + 4 M55 licensed fixtures (`M55_FIXTURES`: music/speech-pauses/speech-clean 48k stereo, eroica 48k stereo; committed WAVs + provenance files in-repo). Inventory shared exactly, not duplicated: `M55_FIXTURES` was hoisted from `mod m52_tests` to top level (same content; `m52_tests` still resolves it via `use super::*` — verified by the passing m55 run, §6).

## 3. Observation-point mapping (Phase 1)

All values are direct reads of existing state at the existing per-frame boundary; no duplicate state, no decision-path changes (whole `stream_driver` module is `#[cfg(test)]`).

| Field | Source | Read point | Scope / lifetime | Direct? | Limits |
|---|---|---|---|---|---|
| `npass` | `qc_out.passes.len()` → `snap.npass` (`stream_driver.rs:1219+`) | S6 exit, per `step_frame` call | frame | Direct (S6 pass count) | Only meaningful where S6 ran (skipped frames emit no row — but skips fail the run loudly, so absence is itself evidence) |
| `crash` | `qc_out` max pass crash → `snap.crash` | S6 exit, per frame | frame (`-1` = none) | Direct (gain-loop state) | Records the decision input, not a crash event mid-flight (panics abort instead — none occurred) |
| `tot_fill` | `qc_out.tot_fill_bits` → `snap.tot_fill` → `RawBlockInput` → `write_fill_extension` (`frame.rs:1057`) | S6 count → S8 write, per frame | frame, bits | Direct counter; execution inferred via the in-code `fill_emitted == counted` gate (error otherwise — passed on all frames) | Counter ≠ mechanism trace; the fixed-7-bit pattern (§5) is reported, not explained |
| `is_book_any` | OR over the `zero_book` slices passed at all 5 S5/S1 sites (`:946`; sites `:968,1140,1170,1298,1339` all pass `&zero_book` — verified by grep) | S5/S1 input construction, per frame | frame | Direct observation of counting inputs (always the zero vec by construction) | Pins the invariant at the counting boundary; the suppression decision lives upstream (config + psy) and is not re-validated here |
| `noise_any_non_pns` | OR over `noise` vs `NO_NOISE_PNS` at all 4 S5 sites (`:1116`) | S5 input construction, per frame | frame | Direct | Same scope note as books |
| `total` / `adts_len` | `qc_out.total_bits` / emitted ADTS length | S6 count / S8 emit, per frame | frame | Direct | Identity `adts_len*8 == 56+total` asserted (M5.27 precedent); subsumes fill accounting |

## 4. Census data and per-stream summary (Phase 3)

Artifact: `$TMPDIR/m5g-census/census.json` (19 streams, 2442 rows: `{f, seq, npass, crash, tot_fill, total, adts_len, is_book, noise}` per frame) + 19 ADTS files. Command: `cargo test --features aac_enc m5g_branch_census -- --nocapture` (from `rust/`), exit ok, 1 passed.

| Stream(s) | Frames | Bytes | tot_fill (sum/max/nonzero) |
|---|---|---|---|
| silence-48k | 49 | 735 | 0/0/0 |
| sine-48k | 49 | 3147 | 7/7/1 |
| onset-48k | 49 | 2359 | 7/7/1 |
| stepdown-48k | 49 | 2236 | 0/0/0 |
| click-mono-48k | 25 | 2680 | 21/7/3 |
| impulse-mono-48k | 11 | 306 | 70/7/10 |
| edge-mono-48k | 14 | 415 | 84/7/12 |
| multitone-48k | 49 | 4819 | 49/7/7 |
| sweep-48k | 49 | 3988 | 21/7/3 |
| noise-mono-48k | 25 | 10603 | 14/7/2 |
| tonal-noise-48k | 25 | 10630 | 21/7/3 |
| quiet-sine-48k | 49 | 4200 | 63/7/9 |
| sine-441k | 45 | 2998 | 35/7/5 |
| onset-441k | 45 | 2250 | 14/7/2 |
| stress-48k | 25 | 10728 | 21/7/3 |
| music / speech-pauses / speech-clean / eroica-48k | 471 ×4 | 250926 / 125818 / 127900 / 234229 | 427/7/61, 392/7/56, 427/7/61, 336/7/48 |

Universal per-frame results (2442/2442 rows, asserted in-test, re-verified from the artifact): `npass==1`, `crash==-1`, `is_book==false`, `noise==false`, `adts_len*8 == 56+total` with zero violations. Zero skips/notes (loud gate); zero panics.

## 5. Findings

- **Retry/crash:** `NOT OBSERVED IN THE 19-STREAM CORPUS` (2442 frames incl. 1884 licensed real-content frames, zero occurrences). Mechanism remains unit-forced only (`rate.rs:2296,2316,2506`); the abort-loud policy is still never exercised end-to-end. Correctness gap, unchanged in kind but now bounded by a defined search.
- **Fill:** `OBSERVED — POLICY CONSISTENT` (new direct evidence; resolves M5.d F1's unobservability). 287 frames carry nonzero `tot_fill`, always exactly 7 bits, on 17/19 streams; silence + stepdown never do. The in-code `fill_emitted == counted` gate (`frame.rs:1057-1061`) passed everywhere, so every counted fill bit was written — 287 confirmed fill-branch executions. The fixed-7-bit uniformity (all sums exact multiples of 7) is consistent with a minimum/granularity rule; the exact mechanism was not traced and is not claimed.
- **Book invariants:** hold end-to-end on all 19 streams — first live-output pinning (previously construction + S5-corpus-unit-test only).
- **Non-interference:** all 4 known ADTS hashes byte-identical (`a9d7b395…`, `702c8717…`, `aa906b52…`, `c3b8b5fd…`); all 15 M52 byte counts match the M5.d run exactly; all 4 M55 byte counts match the M5.d run exactly; all 4 M5.b input WAVs unchanged. Instrumentation changed zero output bytes.
- **No unexplained events.** No failures, no blockers, nothing unobservable except the noted mechanism-trace limits.

## 6. Commands, tests, exit statuses (Phase 4E + census)

- `cargo test --features aac_enc m5g_branch_census -- --nocapture` → ok (1 passed). Census artifact + 19 ADTS in `$TMPDIR/m5g-census/`.
- `cargo test --features aac_enc m52_all_fixtures_write_complete` → ok. `m5a_decode_check` (with `AAC_DEC_BIN`) → ok (4 cases). `m5b_dump_inputs` → ok (inputs unchanged). `M55_C_DIR=$TMPDIR/m5d-c55 cargo test --features aac_enc m55_streams_match_c_reference` → ok (C dir pre-existing from M5.d, read-only).
- No broad suite launched (M5.f did not require it); the touched paths (`step_frame` reads, writer additive field, const hoist) are covered by the above plus the census itself.

## 7. Remaining gaps

- Retry/crash live firing still unobserved; forcing it needs adversarial/bitrate-pressured inputs (bounded follow-up proposed in §8).
- Fill 7-bit mechanism untraced (accounting verified, mechanism not).
- TNS `merged` flag / low-overlap shapes still unobserved live (census did not track them — M5.f §7 scope; remains gap #4).
- Decoder internals out of scope; no perceptual claims; other configs out of scope.

## 8. Recommended next step

**Retry/crash trigger-threshold mapping with the census as detector (bounded, no encoder changes).** Use the existing forced unit-test hooks (`rate.rs:2296` tiny-budget, `:2316` starvation) to document exact trigger thresholds and the resulting (discarded) stream behavior; extend the M5.g census assertions as the detector over bitrate-pressured synthetic runs (existing fixtures only, reduced `-MaxBitrate`-equivalent pressure via the existing test config surface — no new corpus, no tuning); then record the handle-vs-abort policy decision per class for the API milestone. Pass criteria: thresholds documented per class, census detects every forced event (no silent trigger), ADTS of the 19 baseline streams unchanged, policy recorded with reasons. Out of scope: fixing/retuning the loop, PNS/IS enabling, perceptual work, CI. Escalation: any forced event the census cannot see, or any baseline hash movement. Why this: the census proved the common path never needs these branches — the only remaining correctness question is the exceptional path, and the detector to observe it now exists.

## 9. Final worktree and change confirmation

- HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91`, branch `main`; index empty (0 staged) — nothing staged, committed, stashed, reset, cleaned, restored, amended, or pushed.
- Final `git status --short`: 36 lines = 35-line baseline + this report; all other lines verified identical to `/tmp/m5g-status-initial.txt`.
- Files changed, all test-gated and in scope: (1) `rust/src/aac_enc/stream_driver.rs` — 2 census snapshot fields, 2 pure-read observer lines, `CensusRow` + `M52WriteReport.census` (additive), `M55_FIXTURES` hoist (identical content); (2) `rust/src/aac_enc/m5g_census.rs` — new census test; (3) `rust/src/aac_enc.rs` — one registration line; (4) this report. No production behavior, existing fixtures, M5.b baselines, oracle, or pre-existing work modified. Generated artifacts live only in `$TMPDIR/m5g-census/` (+ pre-existing `$TMPDIR/m5*-` dirs, read-only).
