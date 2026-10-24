# L3 Closeout Audit — Native Loudness Integration

- Date: 2026-10-24 (UTC).
- Scope: read-only, evidence-based audit of the L3 implementation (native
  ReplayGain 2.0 + `iTunNORM` tagging) against L0, finalized L2, and the L3
  implementation report. No fixes, no follow-up work performed.
- Baseline: `main` @ `7ebf208d40d78adcd265e3736e93f8a8aa93e8d1`.

## 1. Baseline and working-tree comparison

Initial state (audit start): branch `main`, HEAD `7ebf208d40d78adcd265e3736e93f8a8aa93e8d1`,
0 staged, 9 tracked-modified files, 23 untracked files.

- The 9 tracked modifications match the implementation report §2 table
  (`Cargo.lock`, `convert/Cargo.toml`, `convert/README.md`, `src/batch.rs`,
  `src/lib.rs`, `tests/coverart.rs`, `tests/metadata.rs`,
  `fixtures/README.md`, `fixtures/generate.py`).
- The 23 untracked files are the 14 pre-existing planning documents (present,
  unmodified — content re-read for this audit), the L3 implementation report,
  `src/loudness.rs`, `tests/loudness.rs`, and the 6 `loud-*.flac` fixtures.
- The L3 file list in the implementation report §2 is complete and accurate;
  no other source, test, manifest, or doc file was changed (`main.rs`,
  `meta.rs`, `m4a.rs`, `gapless.rs`, `flac.rs`, `adts.rs`, `output.rs`,
  CLI, and all other tests are untouched).
- Final state (audit end): identical — 0 staged, 0 commits, 0 pushes. The
  only repository change made by this audit is the creation of this file.
  (`cargo test` / `cargo build` runs wrote only to ignored `target/` and
  OS temp dirs.)

## 2. Executive verdict

**PASS WITH FINDINGS.** No P0 or P1 issues. The implementation satisfies
L0 D1–D7 and the finalized L2 design as written: architecture, domain,
gains, clipping, `iTunNORM` pipeline, ordering, flag semantics, failure
behavior, and compatibility all check out against code, tests, and
re-executed evidence. Three P2 findings require attention before release
(two test-coverage gaps against explicit L2 acceptance criteria, one
implementation ruling that needs explicit owner acknowledgment), plus two
P3 notes. None blocks the correctness of what was built; F1–F3 should be
closed (tests added / ruling acknowledged) before release-readiness sign-off.

## 3. L0/L2 compliance matrix

| Requirement (source) | Implementation (file:lines) | Test(s) | Verdict |
|---|---|---|---|
| Both RG 2.0 + `iTunNORM` (L0 D1) | `loudness.rs:42-47,325-379` | `loudness.rs:112` (`tone_…_complete_loudness_set`) | PASS |
| Fixed −18.0 LUFS, no configurability (L0 D2) | `loudness.rs:34,40,303,490,509` (two named consts, no flag/plumbing) | unit vectors + golden `0.75 dB` etc. | PASS |
| Native R128, `ebur128` 0.1.10 (L0 D3, L2 C) | `Cargo.toml:14` (`=0.1.10`); `loudness.rs:27,49-52` (`I\|SAMPLE_PEAK\|HISTOGRAM`) | Gate-2 suite (prep); `loudness.rs` golden tests | PASS |
| Source-parent album grouping (L0 D4) | `loudness.rs:388-405` (`relative.parent()`, root → `""`) | `batch_groups_albums_by_parent_directory` | PASS |
| Retained members analyzed via readable sources (L0 D5, L2 §5) | `batch.rs:484-493` (pre-pass over all `plan.entries` before any skip/conversion) | `retained_…_byte_identical…`, `missing_retained_source_…` | PASS |
| No retag / single publication (L0 D6) | atoms threaded pre-`finish` (`lib.rs:553-562`); `finish→publish` unchanged | race/sentinel suites green; §7 conclusions | PASS |
| No new flag; `--no-tags`/ADTS/dry-run skip; recoverable=warn; genuine=fail; exits+summary frozen (L0 D7, L2 F/§7) | `batch.rs:484` gate; `lib.rs:372,553-556`; `main.rs` untouched | `no_tags_adts_and_dry_run_skip_analysis`, `genuine_decode_failures…`, cli suite green | PASS |
| Source-FLAC PCM domain (L2 D1) | `loudness.rs:77-89,182-205` (`FlacSource`, packet feed; no AAC decode anywhere) | golden values + report §7 rsgain parity | PASS |
| Gains = −18 − integrated; peaks verbatim; Easy-mode positive-only clip, track/album independent, floor +0.00, audio untouched (L2 D2) | `loudness.rs:259-268,488-520` | T-clip unit tests; `clip_spike_…` (`8.71→0.92 dB`, peak `0.899994` verbatim) | PASS |
| Four RG fields + formula; order `iTunSMPB→text→covr→loudness`; TG/TP/AG/AP/iTunNORM (L2 D3, §4) | `loudness.rs:358-379`; `lib.rs:548-562`; `gapless.rs` `udta_box_with_tags` (unchanged) | `tone_…`, `tagged_cover_coexists_…` (exact 9-child order) | PASS |
| Banker's rounding; `+30.00→sc_2500==2`; clamps; exact layout (L2 §4.3) | `loudness.rs:276-311` (never `f64::round`) | `soundcheck_half_way_pin…`, `…_clamps…`, `…_layout_is_exact` | PASS |
| iTunNORM from adjusted track gain, 2-decimal quantized input (L2 §5) | `loudness.rs:363-377` (parse-back of rendered gain) | `track_atoms_follow_l2_order_and_keys`; byte check (report §7) | PASS |
| Incomplete album: track atoms kept, album omitted, naming warning (L2 D4) | `loudness.rs:423-554` | `incomplete_album_…`, `missing_retained_source_…`, `mixed_layout_…` | PASS |
| Warning order precision→tags→cover→loudness; per-file + one album notice; `--no-tags` suppresses all (L2 §7) | `lib.rs:548-575`; `batch.rs` docs | `cover_24bit_unsupported…` (exact 14-warning vector), `metadata.rs` vectors | PASS |
| Private thread-through; public API unchanged (L2 §2/§7) | `lib.rs:389-402` `pub(crate)`; no new `pub` item | full suite green | PASS |
| No FFmpeg/`rsgain` production dep (L0 constraints) | `cargo tree`: `ebur128→bitflags1/dasp/smallvec` only; `cc` absent | build graph | PASS |

## 4. Findings

### F1 (P2) — No absolute R128 calibration test in CI (L2 §8 T-cal)

- Requirement: L2 §8 T-cal — 1 kHz −23 LUFS stereo (±0.1 LUFS), silence gate,
  44.1/48 kHz × mono/stereo lattice, failing on nondeterminism.
- Evidence: `tests/loudness.rs` golden values (`0.75 dB`, `12.35 dB`,
  `3.75 dB`, …) were measured with the implementation under test and then
  pinned. They are self-consistent: a systematic wiring error (wrong channel
  map, double-counted blocks, rate mixup) would shift measurement and goldens
  together and CI would still pass. The only absolute anchor is the one-off
  manual `rsgain` parity check (implementation report §7: `+0.75` exact,
  quiet `+12.34` vs `+12.35`), which lives outside CI.
- Test gap: no test feeds a known-level sine and asserts integrated loudness
  against theory, and no repeat-run determinism test exists.
- Blocks release readiness: no (calibration is evidenced; CI is merely not
  guarding it). Fix before sign-off: add a CI test asserting a 1 kHz sine
  fixture lands within ±0.1 LUFS of its theoretical integrated value (or
  within ±0.05 dB of `rsgain`-agreed gains), plus a repeat-run equality
  assertion.
- Recommendation: add the T-cal test named above; do not adjust goldens.

### F2 (P2) — No partial-rerun tag-identity test (L2 §8 T-album)

- Requirement: L2 §8 T-album — "full run vs. partial rerun (one retained) →
  identical track + album tags".
- Evidence: `retained_outputs_stay_byte_identical_and_complete_the_album`
  (`tests/loudness.rs:422-443`) covers only the fully-retained rerun
  (2 skipped, byte-identity of untouched outputs). The observable
  consequence of retained-member analysis — a *converted* member of a
  partially-retained album carrying byte-identical tags to the full run —
  is asserted nowhere. The implementation does the right thing (pre-pass
  covers all entries, `batch.rs:484-493`), but the L2 acceptance criterion
  has no regression test.
- Blocks release readiness: no (behavior verified by design + full-retention
  test). Fix before sign-off: extend the retained test — delete one output,
  rerun with `skip_existing`, assert the converted file is byte-identical to
  the full-run output and carries identical album atoms.
- Recommendation: add the partial-rerun identity test; no code change.

### F3 (P2) — Mixed-layout album poisoning needs explicit owner acknowledgment

- Requirement: L2 lists incompleteness triggers as corrupt/undecodable
  source, silence/undefined integrated value, invalid measurement, or
  missing/unreadable retained source (L2 §§5,7) — mixed rate/channel layouts
  within one directory are not addressed.
- Evidence: `loudness.rs:91-107,146-180,429-464` — a member whose layout
  differs from the seeded album analyzer poisons the `AlbumState`
  (`Poisoned`), album atoms are omitted, the causer is named
  (`CausedPoison`), and post-poison members are measured track-only without
  being named. This is D4-consistent (never subset-as-complete),
  conservative, disclosed in the implementation report §9, and pinned by
  `mixed_layout_album_keeps_track_tags_and_omits_album_tags`
  (`tests/loudness.rs:487-532`).
- This is filed for review per the audit brief, not as a defect: the ruling
  is safe and tested, but it is an implementation decision the approved
  design does not spell out (alternative: reject mixed layouts earlier, or
  group by layout within a directory).
- Blocks release readiness: only in the procedural sense — record owner
  acceptance of the ruling as-is (no code change), or amend L2 first.

### F4 (P3) — 24-bit inputs are analyzed post-truncation; undocumented, untested

- Observation: `FlacSource::next_pcm` truncates 24-bit PCM to i16 before
  both encoding and analysis (`flac.rs` decode path; `loudness.rs:182-205`
  consumes the same packets). L2 §5 says values are "measured on the
  lossless source", which for 24-bit input strictly means pre-truncation
  PCM. Analyzing the truncated stream is self-consistent with what gets
  encoded, and the impact is negligible (sub-LSB integrated differences;
  peak error ≤ 1.5e-5 linear) — but the choice is nowhere stated and no
  test exercises loudness on a 24-bit input.
- Recommendation: one sentence in the README/implementation report plus,
  optionally, a 24-bit loud-fixture golden test. No behavior change proposed.

### F5 (P3) — `debug_assert!` guards the M4A+tags loudness invariant

- Observation: `lib.rs:389-402` asserts (debug-only) that batch M4A+tags
  conversions always carry `Some` loudness. The invariant holds by
  construction (the pre-pass map contains every entry, `batch.rs:484-493`,
  `607-615`), so a release-mode violation would silently write no atoms
  rather than fail. Unreachable via the current call graph; noted for
  maintenance awareness only.
- Recommendation: none required; optionally convert to a code comment.

## 5. Test evidence, commands, and results

- `cargo test -p aac-convert --locked --offline` (rustc 1.99.1 = crate MSRV;
  `--locked` forbids lockfile mutation, `--offline` forbids network; runs
  write only to ignored `target/` and OS temp dirs):
  lib 68 (incl. 13 new `loudness::tests`) + batch 47 + cli 10 + conversion
  30 + coverart 11 + loudness 15 (new) + metadata 19 = **200 passed,
  0 failed**; 4 pre-existing ignores (ffprobe-gated, independent-decode,
  metadata-ignored). Headline confirmed — the "200" counts real executed
  assertions, not file existence.
- `cargo build --workspace --locked --offline`: succeeds.
- `cargo fmt -p aac-convert -- --check`: clean (workspace-wide `--all`
  shows only pre-existing `aac`-crate diffs from a newer rustfmt; untouched).
- `cargo clippy -p aac-convert --all-targets`: no warnings in new/changed
  code (verified file-by-file; remaining convert warnings are pre-existing
  lints from the newer toolchain, e.g. `lib.rs:483,517`, untouched lines).
- `cargo tree -p aac-convert --edges normal --locked --offline`: `ebur128
  0.1.10 → bitflags 1.3.2 / dasp_frame 0.11+DASM / smallvec`; no `cc`, no
  FFmpeg/`rsgain`; lockfile consistent (`--locked` enforced) with checksum
  `e227cc62…` matching the vetted crate.
- Production C-toolchain independence: not re-executed in this audit (would
  require a clean rebuild). The implementation report §3 records the shimmed
  `cc`/`gcc`/`clang` build (zero invocations, success); I corroborated the
  mechanism read-only — the vendored `ebur128-0.1.10/build.rs` invokes `cc`
  strictly under `cfg(feature = "c-tests")`, and none of `c-tests`, `capi`,
  `internal-tests`, `precision-true-peak`, `reference-tests` is a default
  feature, nor enabled by `aac-convert`. Mechanism supports the claim.
- Real-file byte evidence (report §7: 7/7 `iTunNORM` byte-match, `rsgain`
  parity `+0.75`/`+0.92` exact): historical report evidence generated by
  isolated scripts outside the repo; consistent with the CI goldens
  re-verified above (`0.75 dB`, `0.92 dB`, `0.899994`, `00000349…`,
  `000007E7…`). Not re-executed (requires disposable fixtures + system
  tools, no repo impact either way); no contradiction found.
- Coverage mapping: gains/clip (unit T-clip + `clip_spike_…`); round-half-even
  + `+30.00` pin + clamps + layout (4 unit tests); atom names/types/locales/
  UTF-8/order on real M4As (`tone_…`, `tagged_cover_…`); text/cover/`iTunSMPB`
  preservation (`tagged_cover_…`, `mapped` assertions unchanged);
  complete/incomplete albums, naming, silence, short-input pin, mono lattice,
  parent grouping, retained byte-identity, missing-source poisoning,
  mixed-layout poisoning, `--no-tags`/ADTS/dry-run, `DecodeFailed` with no
  output published, warning order, summary/exit stability (existing suites).

## 6. Explicit conclusions

- **Retained-output membership: holds.** The pre-pass (`batch.rs:484-493`)
  analyzes every planned source before any skip decision or publication;
  retained members contribute through readable sources without output
  modification (byte-identity test). Missing sources poison via the normal
  unanalyzable path (dedicated test).
- **Incomplete albums: holds.** Track atoms + `iTunNORM` for measurable
  members; album gain/peak omitted; per-file and album-level naming warnings
  emitted per converted file. Silence, missing-source, and poison triggers
  all covered by tests.
- **Metadata ordering: holds.** `iTunSMPB → text → covr → loudness`
  (TG/TP/AG/AP/iTunNORM) asserted exactly on real output bytes, including
  full 9-child `ilst` vectors.
- **No-replace / single publication: holds.** Loudness travels precomputed
  atoms into the existing `finish→publish` path; the TempOutput hard-link
  flow is untouched; race/sentinel suites pass; corrupt inputs publish
  nothing. No post-publish rewrite exists anywhere in the code.

## 7. Unverified claims and release-readiness questions

1. No-C-compiler production build: historical report evidence, mechanism
   corroborated (§5), not independently re-executed. Re-run the shimmed
   build once on the release machine if release policy requires first-hand
   evidence.
2. Absolute calibration rests on a manual `rsgain` comparison outside CI
   (see F1); EBU vectors validated the dependency, not the wired pipeline.
3. Performance: no wall-clock/RSS baseline recorded (L2 sets no thresholds;
   first measured baseline becomes the budget per L2 §8). Consider one
   timed batch run for the release note.
4. Exact-string goldens (`"12.35 dB"`, peak strings) assume libm last-ulp
   stability across release platforms; values sit within tolerance either
   way, but a different toolchain could flake an exact assertion. Watch CI
   on new platforms; prefer range assertions only if flakes appear.
5. The `+30.00 dB` pin and `+0.00 dB` cap paths are unit-tested, not
   reachable through real fixtures — theoretical by nature, adequately
   pinned.

## 8. Recommended next step

Owner: acknowledge F3 (mixed-layout poisoning ruling) and authorize closing
F1/F2 as test-only additions (absolute-calibration test, partial-rerun
identity test) plus the F4 documentation sentence. No production code change
is required for any finding. After that, L3 is release-ready pending the
normal release checklist (M8.5); do not start VBR 1–4 or encoder work under
this closeout.
