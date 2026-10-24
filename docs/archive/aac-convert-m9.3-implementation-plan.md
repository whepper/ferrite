# M9.3 — AAC Converter Integration Implementation Plan

- Date: 2026-10-24 (UTC)
- Baseline: `main` at `4908778` (`test(aac-convert): harden batch resilience coverage`),
  12 commits ahead of `origin/main`, tracked tree clean at plan time apart from
  pre-existing untracked `docs/aac-convert-m8.1-batch-design.md`,
  `docs/archive/aac-convert-m8.5-release-readiness.md`,
  `docs/archive/aac-convert-m9.1-feature-parity-audit.md`, and
  `docs/archive/aac-convert-m9.2-integration-design.md`.
- Status: **read-only plan. No implementation authorized. No code changes made.**
- Authority: `docs/archive/aac-convert-m9.1-feature-parity-audit.md` (M9.1) and
  `docs/archive/aac-convert-m9.2-integration-design.md` (M9.2). M9.2 is a **proposal
  under owner review, not an approval**. Nothing in M9.2 is treated as decided
  behavior in this plan; every M9.2 proposal that needs owner sign-off is
  gated explicitly below.
- Constraints carried forward: M7.1 D1–D10, M8.1 batch semantics and exit-code
  contract, M8.5 release boundaries, M9.2 provisional preferences. No FFmpeg or
  other subprocess in production, no VBR 1–4 work, no psychoacoustic or
  bitstream changes, no universal gapless claim, no licensing or
  distribution-clearance claim.

## 1. Summary and scope

This plan turns the M9.2 design proposals into a dependency-ordered,
independently reviewable milestone sequence for the approved direction only.

Recommended sequence:

| Order | Milestone | Type | Owner gate before work |
|---|---|---|---|
| G0 | Decision gate: approve core-9 table, behavior rules, and spike charters | Decision | **Yes — blocks everything below** |
| M1 | Core-9 M4A text metadata (initial slice) | Implementation | **Yes — G0** |
| S1 | Cover-art `covr` spike (time-boxed investigation) | Spike, no product code | **Yes — G0 charter** |
| S2 | 24-bit FLAC spike (time-boxed investigation) | Spike, no product code | **Yes — G0 charter** |
| D | Deferred candidates (recorded, unscheduled) | Decision record only | Each revival needs its own approval |

M1 is the smallest useful first implementation: ten mapped Vorbis keys written
as M4A text atoms through the existing `moov/udta/meta/ilst` path, ADTS
unchanged, gapless signalling unchanged, no new runtime dependency, no CLI
surface change unless the owner demands an opt-out at G0.

S1 and S2 are parallelizable after G0 and produce recommendations, not product
behavior. Section D records deferred items as explicitly **not** prerequisites
for M1.

## 2. Verified current architecture and data flow

Verified at the baseline (paths relative to repository root; no behavior
inferred):

- **Crate.** `rust/convert` is application crate `aac-convert` 0.1.0, edition
  2021, MSRV 1.99.1; one-way dependency on workspace `aac` (`aac_enc` feature),
  `symphonia` 0.6.1 (`flac` only), pinned `shiguredo_mp4 =2026.5.0`
  (`rust/convert/Cargo.toml:10-14`). No `clap`/`toml`/image/loudness dependency.
- **Single-file pipeline** (`rust/convert/src/lib.rs:336-416`): `output::resolve_paths`
  → `flac::FlacSource::open` → `properties.validate` → `EncoderConfig::new`
  → bounded `encode_chunks` (`CHUNK_FRAMES = 8_000`) → `source.verify()` (MD5)
  → encoder flush → `sink.finish` → `sink.publish`. No whole-file PCM/AAC buffering.
- **Input contract** (`src/flac.rs:43-132`, `src/lib.rs:251-266`): Symphonia probe
  (`hint "flac"`, `verify = true`); accept 16-bit, 44.1/48 kHz, mono/stereo only.
  `UnsupportedBitDepth/SampleRate/Channels` reject before decode; mid-stream spec
  change → `DecodeFailed`; present-but-wrong MD5 → `DecodeFailed`, no publication;
  absent/zero MD5 accepted after successful decode. Only PCM path is
  `copy_to_vec_interleaved::<i16>` (`flac.rs:118`).
- **M4A path** (`src/m4a.rs:46-197`, `src/gapless.rs:110-225`): ADTS-mode encoder
  frames → validated `AdtsFrame::raw_payload` + `EncoderConfig::audio_specific_config`
  → `shiguredo_mp4::Muxer::Mp4FileMuxer` (per-sample metadata only). `finish`
  rewrites only `moov`: appends `udta > meta > ilst > ----(iTunSMPB)` via
  `GaplessAccounting::udta_box`, splices 28-byte `M4A ` ftyp, enforces
  `moov`-is-final. `mdhd`/`mvhd` stay on the encoded timeline; no edit list.
- **Safety/batch** (`src/output.rs:17-190`, `src/batch.rs:314-617`,
  `src/main.rs:253-471`): canonicalized `resolve_paths`, `create_new` hidden
  `.aac-convert-<pid>-<seq>.tmp`, flush + `sync_all`, `fs::hard_link` publish
  (`AlreadyExists` → `OutputExists`, no rename fallback), `Drop` cleanup.
  Byte-sorted discovery, recursive default, dir-symlink never descended, file
  symlinks at link path, hidden entries traversed, non-FLAC tallied `ignored`.
  Exact + ASCII-fold duplicate preflight abort; `skip` = per-file failure,
  `--skip-existing` = no-decode skip; sequential execution; per-file stdout +
  stderr diagnostics + final `summary:`/`dry-run:`; exits `0/1/2/3`.
- **Validation setup (actual).** Workspace root is `rust/` (`.github/workflows/ci.yml`
  `defaults.run.working-directory: rust`). Converter job: `cargo build --locked
  -p aac-convert`, `cargo test --locked -p aac-convert` (Ubuntu, Rust 1.99.1).
  Encoder Tier A job with 15 documented skips, doctests, `cargo package -p aac`
  check; format/clippy jobs are informational (`continue-on-error: true`).
  Local README commands: `cargo build --locked --release -p aac-convert`,
  `cargo test --locked -p aac-convert`, ignored
  `cargo test -p aac-convert --test independent_decode -- --ignored --nocapture`.

## 3. Goals and non-goals

### Goals

1. Deliver the smallest metadata increment that preserves every §2 guarantee.
2. Keep each milestone independently reviewable with observable acceptance.
3. Resolve M9.2 open behavior through explicit owner gates, not inference.
4. Prove ADTS and gapless signalling are unaffected by metadata work.
5. Time-box investigations (S1/S2) so they return recommendations, not scope creep.

### Non-goals (not authorized by this plan)

- VBR 1–4, encoder/psychoacoustic/bitstream/transport/delay/flush changes.
- Production FFmpeg, rsgain, or any subprocess pipeline.
- RAM-disk / working-directory support; parallelism; GUI; TOML; retry state;
  `newer`/`retag`/`rebuild`; JSON summary; signal-handling changes.
- Loudness scanning, normalization, gain changes, or loudness tags.
- Cover-art product support (spike only).
- 24-bit conversion product support (spike only).
- Universal gapless claims; edit-list default output.
- Licensing or public-distribution clearance.

## 4. Milestone roadmap

Each milestone below carries the ten required elements: (1) objective/scope,
(2) non-goals, (3) files to inspect / likely to modify, (4) dependencies,
(5) acceptance outcomes, (6) tests, (7) validation commands, (8) error handling
and compatibility, (9) documentation, (10) risks, rollback, owner decisions.

### G0 — Decision gate (no code)

1. **Objective/scope.** Approve or amend: (a) the core-9 table and every
   behavior rule in §5 (case-insensitivity, first-non-empty-wins, track/disc
   tuple grammar, `date`-wins, whitespace rule, multi-value order, missing-key
   warning-only, unknown-key ignore + verbose count, opt-out question);
   (b) S1 and S2 charters and time boxes in §§6–7; (c) explicit deferral of §8.
   Output is a dated owner note (comment, review line, or addendum reference).
2. **Non-goals.** No code, tests, fixtures, manifests, or CI changes.
3. **Files to inspect.** M9.1 §§4/9/11, M9.2 §§4–5, `src/m4a.rs:118-192`,
   `src/gapless.rs:139-225`, `src/lib.rs:336-416`, `README.md:Scope limits`.
4. **Dependencies.** None (entry gate for M1/S1/S2).
5. **Acceptance.** Written approval naming the approved table version and any
   amendments; unresolved M9.2 O1/O2 (opt-out flag, multi-value order) answered
   yes/no.
6. **Tests.** None (decision only).
7. **Validation.** `git status --porcelain=v1 -b` shows no tracked modifications.
8. **Error handling/compat.** N/A.
9. **Documentation.** This plan + the approval note; no README change at G0.
10. **Risks/rollback/owner.** Risk: silent inference of approval from M9.2's
    existence — mitigated by requiring an explicit note. Rollback: N/A (no code).
    **Owner decision: REQUIRED — blocks M1/S1/S2.**

### M1 — Core-9 M4A text metadata (initial slice)

1. **Objective/scope.** Implement exactly the G0-approved core table (§5):
   read Vorbis comments after `source.verify()`, encode approved atoms as M4A
   UTF-8 text (+ `trkn`/`disk` tuples), append alongside the existing `----`
   `iTunSMPB` item in the same `ilst`, publish through the unchanged temp +
   hard-link path. ADTS path writes no tags.
2. **Non-goals.** Everything in §8; no freeform/BPM/compilation/sort atoms; no
   `covr`; no loudness; no `newer`/`retag`/`rebuild`; no TOML/retry/RAM-disk/
   parallelism/JSON/signal work; no encoder change; no CLI change unless G0
   mandates an opt-out flag (default: no flag change).
3. **Files to inspect.** `src/flac.rs` (Symphonia metadata access shape),
   `src/gapless.rs:139-225` (`boxed()`/`udta_box`), `src/m4a.rs:118-197`
   (`finish` signature, `moov`-is-final), `src/lib.rs:106-155,336-416`
   (`ConvertError`, `convert_inner`), `src/batch.rs:62-92,401-617`
   (`category_for`, tallies), `src/main.rs:313-471` (reporting),
   `src/output.rs:149-190` (publish), `tests/common/mod.rs` (parsers),
   `tests/fixtures/*` (do not edit committed fixtures).
   **Likely to modify (at implementation time, not now):** new `src/meta.rs`
   (`pub(crate)` read + `ilst` children), `src/m4a.rs` (`finish` gains
   `tags: Option<&TagSet>`), `src/lib.rs` (thread-through + new
   `ConvertError::Metadata`-family variant + `Display`/`source`), `src/batch.rs`
   (`category_for` entry), `tests/` (new `tests/metadata.rs` + `common`
   `mp4_ilst` helper), `README.md` + crate docs (tag list + limits). No
   `Cargo.toml`/`Cargo.lock`/CI change expected; any new dependency forces
   stop-and-rescope.
4. **Dependencies.** G0 approval. No dependency on S1/S2/D.
5. **Acceptance (observable).**
   - Tagged M4A decodes in FFmpeg with zero-stderr full decode; `ffprobe` shows
     approved atoms; `mdat` bytes equal the untagged conversion of the same audio.
   - `edts` absent, `M4A ` brands present, `mdhd` = `N*1024`, `iTunSMPB` value
     unchanged for the same audio.
   - Unknown/deferred keys absent from output; verbose `unmapped` lines present;
     summary shape unchanged.
   - Corrupt comment block → per-file `Failed` with the new stable category,
     exit 1, no destination, no orphan.
   - ADTS outputs byte-identical to pre-metadata behavior (no tags).
6. **Tests required.**
   - Unit: tuple parser (`N`, `N/M`, zero-padded, fallback keys, unparseable →
     skip-with-warning); `date`-wins; case-insensitivity; whitespace = absent;
     multi-value order; UTF-8 edge handling per G0 rule.
   - Integration (`tests/metadata.rs`, proposed): structure test on both
     committed audio fixtures via generated tagged FLAC copies (never edit
     committed fixtures); round-trip ignored FFmpeg test; negative test
     (unknown keys absent; corrupt block → `Metadata` failure); verbose-output test.
   - Regression: full existing suite unmodified in behavior — `tests/cli.rs`,
     `tests/conversion.rs`, `tests/batch.rs`, unit tests in `src/*`, Tier A
     encoder gates per M8.5 §9. ADTS-invariance test (tagged source → ADTS has
     no `ilst` and matches untagged ADTS bytes).
7. **Validation commands (actual setup, run from `rust/` unless noted).**
   - `cargo build --locked -p aac-convert`
   - `cargo test --locked -p aac-convert`
   - `cargo test --locked -p aac-convert --test independent_decode -- --ignored --nocapture` (requires `ffmpeg`/`ffprobe` on PATH; advisory only)
   - Encoder gates unchanged: Tier A lib command with the 15 documented
     `--skip …` filters, `--doc` suites, and `cargo package --locked -p aac`
     per `ci.yml` (run when the implementation branch is ready; not run in planning).
   - `git status --porcelain=v1 -b` + `git diff --stat` review: only expected
     files touched.
8. **Error handling and compatibility.**
   - New `ConvertError` variant (proposed name `Metadata(&'static str)`) maps to
     a new stable `category_for` string (proposed `"Metadata"`); counted as
     `failed` (never `existing`/`ignored`/`skipped`); siblings continue;
     dry-run never reads tags; exit-code contract frozen (`0/1/2/3`).
   - Missing/unknown/whitespace values are warnings (verbose-only), never
     failures. Tag errors never resurrect failed decodes (read after `verify()`).
   - Batch `ok`/`summary` shapes unchanged (no new counter); verbose additions
     only, mirroring existing `ignore` lines.
9. **Documentation updates (at implementation time).** `rust/convert/README.md`
   (mapped table + deferred list + ADTS-unchanged + gapless-unchanged notes),
   crate doc in `src/lib.rs`, `THIRD-PARTY-NOTICES.md` only if a dependency is
   (unexpectedly) added. No distribution or clearance claims.
10. **Risks, rollback, owner decisions.**
    - Risks: scope creep into deferred atoms; `moov` growth breaking
      `moov`-is-final (mitigated by reusing checked `boxed()` sizing + `set_len`
      + tiling test); warning/failure misclassification (mitigated by §10 tests).
    - Rollback: revert the M1 commit(s); tagged outputs remain decodable, so no
      data migration. Smallest rollback unit is the whole M1 slice (tags are one
      feature, not per-atom flags).
    - **Owner decisions: G0 table approval required; O1 (opt-out flag) and O2
      (multi-value order) answered at G0. No implementation without G0.**

### S1 — Cover-art `covr` spike (time-boxed, no product code)

1. **Objective/scope.** Answer: can `shiguredo_mp4 =2026.5.0` carry `covr`
   without breaking §2 playback/duration/memory evidence? Build throwaway JPEG
   and PNG `covr` prototypes via `UnknownBox` (or explicit `covr` if the API
   models it — establish which), one image each, against both committed audio
   fixtures. No product module, no CLI, no filesystem cover-copy behavior.
2. **Non-goals.** No standalone cover-file copy, no search-order/fallback/resize
   policy, no `covr` product support, no dependency addition, no README promise.
3. **Files to inspect.** `src/m4a.rs:118-197`, `src/gapless.rs:139-225`,
   `src/output.rs:107-147` (seek/set_len), `Cargo.lock` (`shiguredo_mp4`
   2026.5.0 tree), `tests/common/mod.rs` (MP4 parsers), README M4A/timing section.
4. **Dependencies.** G0 charter (time box + success bar). Independent of M1/S2.
5. **Acceptance (spike report, prose + commands).**
   - States explicitly whether `shiguredo_mp4` models `covr` or only
     `UnknownBox`, accepted image types/sizes observed, multi-image behavior if tested.
   - Records `ffprobe` codec output, full `ffmpeg -v error -i … -f null -`
     result (zero stderr required for pass), `afinfo`/`afconvert`-equivalent trim
     check (delay 1600 honored, duration fields untouched), and peak-RSS delta
     vs untagged conversion for a large (≥5 MB) image.
   - Ends with one of: (a) proceed to a standalone-copy proposal first, (b)
     proceed to embedded-`covr` proposal, or (c) stay with no artwork — with reasons.
   - Failure to decode or trim correctly is a valid negative result.
6. **Tests.** None committed (spike artifacts are throwaway). If the spike uses
   throwaway Rust harnesses, they live outside `rust/convert/tests/` and are not
   committed unless the follow-up proposal says otherwise.
7. **Validation commands.** Same build/test commands as M1 to prove no tree
   regression (spike must not modify the tree; if it does, discard changes):
   `cargo build --locked -p aac-convert`, `cargo test --locked -p aac-convert`;
   out-of-tree `ffprobe`/`ffmpeg`/`afinfo` checks recorded verbatim.
8. **Error handling/compat.** N/A (no product path). Must not change `finish`
   failure modes.
9. **Documentation.** Spike note only (appended to the follow-up proposal or a
   dated research note); no README change.
10. **Risks/rollback/owner.** Risk: spike drift into product code — mitigated by
    time box and no-commit rule. Rollback: delete throwaway artifacts.
    **Owner decision: REQUIRED — charter at G0; proceed/defer verdict after the report.**

### S2 — 24-bit FLAC spike (time-boxed, no product code)

1. **Objective/scope.** Establish from verified capabilities: what Symphonia
   0.6.1 yields for the committed `stereo-48k-24.flac` in this configuration
   (sample type/range), the exact scale-to-`i16` law, a dither candidate (e.g.,
   TPDF with stated width/shaping, or justified no-dither rounding), and the
   no-clipping/overflow argument with deterministic behavior. Compare conversion
   vs explicit rejection on precision, clipping, and test coverage. No encoder
   API change, no production policy change.
2. **Non-goals.** No `i32`/float encoder interface, no resampling/downmixing,
   no 24-bit acceptance, no fixture replacement.
3. **Files to inspect.** `src/flac.rs:43-132` (probe/validate/decode order),
   `src/lib.rs:251-266` (`AudioProperties::validate`), `rust/convert/tests/fixtures/`
   (`stereo-48k-24.flac`, `generate.py`), encoder `push(&[i16])` contract
   (`rust/src/aac_enc/encoder.rs`), M7.1 U5/D10.
4. **Dependencies.** G0 charter. Independent of M1/S1.
5. **Acceptance (spike report).**
   - Records observed Symphonia 24-bit yield (type, range, channel/rate spec).
   - Records prototype scale + dither law, objective distance vs truncated
     baseline, full-scale ±/silence/fade behavior, clipping analysis, and
     determinism (same input → same output bytes).
   - Lists fixtures that would be needed (new files, never edits of committed
     ones) and the unit/integration gates that would pin the policy.
   - Ends with keep-reject or convert recommendation + rationale. Either outcome
     is valid; silent truncation is never an option.
6. **Tests.** None committed. Throwaway harness only.
7. **Validation.** `cargo build --locked -p aac-convert` /
   `cargo test --locked -p aac-convert` to confirm the tree is untouched; spike
   measurements recorded with commands and versions.
8. **Error handling/compat.** Current `UnsupportedBitDepth(24)` behavior stands
   during and after the spike.
9. **Documentation.** Spike note only.
10. **Risks/rollback/owner.** Risk: assuming `copy_to_vec_interleaved::<i16>`
    handles 24-bit correctly — explicitly forbidden; the spike must observe
    first. Rollback: discard harness. **Owner decision: REQUIRED — charter at
    G0; conversion policy after the report (plus quality review).**

## 5. Workstream A — Core M4A text metadata

### A.1 Mapping (proposal for G0 approval — not decided)

Conventions (all require G0 sign-off; M9.2 §5 is the review draft):

- Vorbis keys matched case-insensitively; values are UTF-8 strings from FLAC
  comments. First non-empty value wins per atom.
- `tracknumber`/`discnumber` accept `N` or `N/M`; total falls back to
  `tracktotal`/`totaltracks` and `disctotal`/`totaldiscs` when the slash form is
  absent; unparseable values are skipped with a per-file warning, never a failure.
  Zero totals written as `0` (unknown).
- `date` takes precedence over `year` for `©day`; never duplicated.
- All text atoms written as M4A data type UTF-8. No integer/boolean/freeform
  encoding in M1.

| # | Vorbis key(s) (case-insensitive) | M4A atom | Type in M1 | Notes |
|---|---|---|---|---|
| 1 | `title` | `©nam` | text | Required floor (M7.1 §8.6). |
| 2 | `artist` | `©ART` | text | Required floor. |
| 3 | `album` | `©alb` | text | Required floor. |
| 4 | `tracknumber` (+ `tracktotal`, `totaltracks` fallback) | `trkn` | track tuple `(N, M)` | Required floor; tuple rule above. |
| 5 | `discnumber` (+ `disctotal`, `totaldiscs` fallback) | `disk` | disc tuple `(N, M)` | Multi-disc correctness. |
| 6 | `albumartist` | `aART` | text | Compilation grouping; absence is not failure. |
| 7 | `date`, then `year` | `©day` | text | `date` wins; `year` only if `date` absent. |
| 8 | `genre` | `©gen` | text | Free-text genre; no genre-ID mapping. |
| 9 | `composer` | `©wrt` | text | Order per G0 O2 ruling. |
| 10 | `comment` | `©cmt` | text | Verbatim modulo UTF-8 validity. |

### A.2 Edge behavior (proposal for G0 approval — not decided)

- **Missing core keys:** warning only; conversion succeeds.
- **Empty/whitespace-only values:** treated as absent.
- **Duplicate atoms (`date`+`year`):** written once (`date` wins).
- **Multi-value text:** order per G0 O2 (M9.2 proposes source order, no dedup).
- **Non-UTF-8 payloads:** skip the value with a warning, never fail the file
  (exact Symphonia string-error type pinned at implementation; no new dependency).
- **Unknown/unsupported comments:** ignored for output, counted in a verbose-only
  `unmapped` diagnostic, never an error. Includes every §8 deferred key.
- **Opt-out:** no `--no-tags` (or similar) unless G0 O1 mandates it. Default
  proposal: tags are the documented M4A behavior once M1 lands.

### A.3 Write path (proposal)

- New small module (proposed `src/meta.rs`, `pub(crate)`): `read_flac_comments`
  → `TagSet`; `ilst_children(&TagSet)` → per-atom child bytes via the existing
  `gapless.rs:boxed()` helper and checked-size arithmetic.
- `M4aWriter::finish` gains `tags: Option<&TagSet>`; M4A passes `Some`, ADTS
  path is untouched. Tags are read after `source.verify()` and before
  `sink.finish`. New `ConvertError::Metadata`-family variant + `category_for`
  entry; per-file `Failed`, siblings continue, no destination/orphan left behind.
- Diagnostics mirror existing lines: `ok` lines unchanged; verbose-only
  `tags`/`unmapped` lines; summary shape unchanged (no new counter).
- **ADTS and gapless invariants (must-hold):** ADTS outputs contain no tags and
  are byte-identical to untagged behavior; M4A `mdat` equals the ADTS access-unit
  payloads for the same audio; `edts` absent; `M4A ` brands, `mdhd = N*1024`,
  and `iTunSMPB` value unchanged for the same audio.

### A.4 What M1 must NOT invent

Tuple grammar beyond `N`/`N/M` + documented fallbacks; integer/boolean/freeform
encoders; genre-ID tables; sort-field semantics; `encoder`-identity claims; any
behavior for deferred keys beyond ignore-and-count. Anything not in the G0 note
is out.

## 6. Workstream B — Cover-art spike (S1 detail)

Time-boxed investigation only. Evidence required before any product decision:

1. `shiguredo_mp4` 2026.5.0 `covr` modeling: explicit type vs `UnknownBox`
   (inspect the pinned source in the local registry, not the README).
2. Accepted image types/sizes, multi-image behavior (observed, not assumed).
3. FFmpeg acceptance: `ffprobe` codec + `ffmpeg -v error -i … -f null -`
   zero-stderr on JPEG and PNG prototypes for both committed audio fixtures.
4. CoreAudio trim invariance: delay-1600 honored, duration fields untouched
   (same bar as §2).
5. Peak-RSS delta with a large (≥5 MB) image vs untagged conversion
   (bounded-memory story in `README.md:Memory`).

Testing strategy: throwaway harness only; no committed tests. Verdict options:
standalone-copy-first, embedded-second, or stay-with-no-artwork. **Do not
implement artwork support as part of M1; S1 needs its own approval after the
report even if the spike passes.**

## 7. Workstream C — 24-bit FLAC spike (S2 detail)

Time-boxed investigation only. Evidence required:

1. Observed Symphonia yield for 24-bit FLAC in this configuration (sample type,
   range, spec) using the committed `stereo-48k-24.flac` — observation before law.
2. Candidate scale-to-`i16` law + dither candidate (e.g., TPDF width/shaping) or
   justified no-dither rounding; deterministic byte behavior.
3. Precision/clipping analysis: full-scale ±, silence, fade; no-clipping/overflow
   proof; objective distance vs truncated baseline.
4. Fixture/test plan that would be needed (new files only): full-scale, silence,
   fade, multi-block, determinism (same bytes twice).

**Do not change the current 24-bit reject policy or implement conversion without
owner + quality-review approval.** M7.1 D10 (no encoder PCM API expansion) stands.

## 8. Workstream D — Deferred candidates (recorded, unscheduled)

Recorded as deferred unless the owner later approves them. **None is a
prerequisite for M1.** Each needs its own scoping if revived:

- Freeform tags, BPM, compilation, sort metadata (needs value-semantics + player review).
- Loudness scanning, normalization, gain metadata (needs algorithm/library/licensing
  review; `iTunSMPB ≠ iTunNORM` must stay documented).
- `newer`/`retag`/`rebuild` policies (`retag` meaningless pre-tags; `rebuild`
  contradicts no-replace without a new atomic-replace design).
- TOML configuration (trigger: demonstrated recurring multi-profile need).
- Persistent retry state (filesystem remains source of truth).
- RAM-disk support (trigger: measured multi-rewrite bottleneck only).
- Parallelism (needs a new architecture decision overturning M7.1 D6).
- JSON output and signal-handling changes (each independently scoped, additive only).

## 9. CLI/configuration behavior and compatibility

- M1 default proposal: **no CLI change**. Existing flags frozen (`--format`,
  `--recursive/--no-recursive` last-wins, `--dry-run`, `--skip-existing`,
  `-v/--verbose`, `-q/--quiet`, `--`, `-h/--help`, `-V/--version`); single-file
  batch-flag guards stand; exits `0/1/2/3` frozen.
- If G0 O1 mandates an opt-out, it must be specified then (name, default,
  single-file vs batch applicability, dry-run interaction, summary impact) and
  must default to current behavior for all pre-existing invocations.
- Future TOML (deferred): versioned schema, unknown-key errors, bounds, path
  canonicalization — not designed here.

## 10. Output collision, rebuild, and failure semantics

- Unchanged foundation: duplicate preflight (exact + ASCII-fold), `SameRoot`
  refusal, per-file `OutputExists` vs `--skip-existing` skip, advisory pre-check
  with atomic hard-link enforcement, `Drop` cleanup, sentinel preservation, race
  semantics (`src/output.rs:publish`, `src/batch.rs:execute_plan`).
- M1 tag errors: per-file `Failed` under the new stable category, `failed`
  tally, stderr detail, siblings continue, no destination, no orphan; dry-run
  never reads tags and keeps today's `OutputExists`/`Skipped` outcomes.
- No `--force`/rename fallback/destructive replace. Cover/loudness rewrites of
  published outputs are forbidden without a new safety case (not proposed).

## 11. Testing and acceptance criteria

Conventions: reuse `tests/common/mod.rs` (`TempDir`, `temp_output_files`,
`parse_adts`, `parse_mp4` + proposed `mp4_ilst` helper). Every test asserts empty
temp scans and sentinel preservation on collision paths.

### M1 acceptance (T-tags-1…T-tags-5)

- **T-tags-1 (unit).** Tuple grammar, fallback keys, unparseable → warning-skip,
  `date`-wins, case-insensitivity, whitespace-absent, G0 multi-value order, UTF-8 edges.
- **T-tags-2 (structure).** Tagged copies of both committed audio fixtures (test
  harness generates tagged FLAC; committed fixtures never edited): `ilst`
  contains exactly mapped children + `----/iTunSMPB`; `edts` absent; `ftyp`
  `M4A `; `mdhd = N*1024`; `mdat` byte-identical to untagged conversion.
- **T-tags-3 (round-trip, ignored).** `ffprobe` shows approved atoms; full FFmpeg
  decode clean, zero stderr (same ignored pattern as `independent_decode.rs`).
- **T-tags-4 (negative).** Deferred/unknown keys absent + verbose `unmapped`
  lines + exit 0 + unchanged summary; corrupt comment block → `Metadata` failure,
  exit 1, no destination/orphan.
- **T-tags-5 (regression).** Full suite green with unchanged behavior:
  `tests/cli.rs`, `tests/conversion.rs`, `tests/batch.rs`, `src/*` unit tests;
  ADTS-invariance (no tags, byte-identical ADTS); Tier A + doctests per M8.5 §9.

### S1/S2 acceptance

- **T-artspike-1.** JPEG + PNG prototypes: `ffprobe`, zero-stderr FFmpeg decode,
  trim invariance, RSS delta, written verdict (§6).
- **T-24spike-1.** Yield record, scale+dither prototype, no-clipping proof,
  distance vs truncation, fixture/test list, written verdict (§7).

### Explicitly not gated

No `newer`/`retag`/`rebuild`, TOML, retry-state, loudness, cover-product, JSON,
signal, or automated T-bounded-1 gates. Manual T-bounded-1 procedure (convert a
30-minute-equivalent synthetic FLAC; record peak RSS and wall time per file;
confirm flat growth and linear time) may be run informationally; automation
waits for one recorded baseline.

## 12. Phased implementation outline (dependency-ordered)

- **G0 — Decisions.** Owner approves/amends §5 table + behavior rules, O1/O2,
  S1/S2 charters + time boxes, §8 deferral. No code. **Blocks M1/S1/S2.**
- **M1 — Core-9 metadata.** Smallest useful implementation (§4 M1). Reviewable
  alone; shippable without S1/S2/D. **Requires G0.**
- **S1 — Cover-art spike.** Parallelizable with M1/S2 after G0. No product code.
  Verdict needs fresh approval before any cover proposal.
- **S2 — 24-bit spike.** Parallelizable with M1/S1 after G0. No product code.
  Verdict needs owner + quality-review approval before any conversion proposal.
- **D — Deferred bucket.** Unscheduled. Each revival is a new scoping exercise
  with the triggers in §8; none blocks M1.

## 13. Risks, open questions, and owner decisions

| # | Risk / question | Impact | Mitigation in this plan | Owner decision |
|---|---|---|---|---|
| R1 | Scope creep (freeform/BPM/sort/cover bundled into "tags") | Unbounded review, container risk | Enforce §5/§8 split; T-tags-4 asserts absence | **Yes — G0 table sign-off (M7.1 U4).** |
| R2 | Artwork assumed shippable | Broken players, memory regression | Spike-first (S1); no M1 commitment | **Yes — charter now, verdict later.** |
| R3 | Loudness conflated with gapless | Wrong tags, level shifts | §8 + doc distinction (`iTunSMPB ≠ iTunNORM`) | **Yes — any loudness work.** |
| R4 | 24-bit "just scale it" | Clipping, noise, silent loss | S2 observation-first; reject stands | **Yes — charter now, policy later.** |
| R5 | `moov` growth breaks `moov`-is-final | Publish failure on tagged files | Reuse checked `boxed()` + `set_len`; T-tags-2 tiling | Engineering (test-gated). |
| R6 | Warning/failure misclassification | CI breakage or silent loss | Rule: unknown/missing = warning; undecodable block = per-file failure; frozen exits | Engineering (T-tags-4). |
| R7 | TOML/retry/`newer` revived by symmetry | Sprawl | Recurrence-evidence trigger only (§8) | **Yes — each revival.** |
| R8 | Encoder-adjacent drift | Release-gate breach | No encoder/transport/delay/flush change; M8.5 §9 re-run | Engineering (CI). |
| O1 | `--no-tags` opt-out? | Flag surface | Only if G0 mandates; default is no change (§9) | **Owner at G0.** |
| O2 | Multi-value order? | Display order | M9.2 proposes source order; G0 rules (§5) | **Owner at G0.** |
| O3 | Cover order/fallback if ever adopted? | User surprise | Deferred to post-S1 proposal | **Owner later.** |

Open behavior questions that stay open until G0: exact approved table version,
O1/O2 rulings, S1/S2 time boxes, and confirmation that §8 stays deferred.

## 14. Traceability

- M9.1 F4 (core tags, `missing`, MVP-required) → G0 + M1 + §5. M9.1 F5
  (freeform/BPM/compilation/sort, `decision`) → §8 deferred + T-tags-4 absence.
- M9.1 F6–F7 (loudness, `decision`) → §8 + no-tag rule; `iTunSMPB ≠ iTunNORM`.
- M9.1 F8–F9 (cover, `decision`) → S1 spike-first (§6); no M1 commitment.
- M9.1 F10 (work_dir, `unnecessary`) → not scheduled; benchmark trigger only.
- M9.1 F13–F17 (`decision`: TOML, `newer`/`retag`/`rebuild`, retry state, album
  grouping, warnings) → §8 deferred; filesystem stays source of truth.
- M9.1 F15–F16/F20–F22/F26/F29–F31 (`partial`) → preserved behavior (§§9–10);
  no port of `verify_output` mtime/timeout semantics.
- M9.1 F23/F25/F27/F28/F32–F33 (`supported`) → §2 constraints; ADTS/MD5/safety
  regression-gated in T-tags-5.
- M9.1 §8 temp storage → no working directory (M7.1 D5 upheld).
- M9.1 §9 missing tests → T-tags-1…5, T-artspike-1, T-24spike-1; non-tag gaps
  stay manual until baselined.
- M9.1 §10 P0–P2 → G0/M1/S1/S2; P3–P7 → §8 with triggers; VBR 1–4/FFmpeg/RAM-disk
  exclusions honored.
- M9.2 §5 (core-9 + write path) → §5/M1 (proposal, pending G0). M9.2 §6.1/§6.2
  (artwork spike / loudness none) → §§6/8 + S1. M9.2 §7 (16-bit-only) → §7 + S2.
  M9.2 §8–§9 (CLI-only, no-replace) → §§9–10. M9.2 §10/T-tags → §11. M9.2 §11
  phases → §12. M9.2 §12 R1–R8/O1–O3 → §13.

No universal gapless playback is claimed; default M4A carries `iTunSMPB`-only
signalling with Apple brands (verified FFmpeg 9.0.2 + CoreAudio paths; VLC/GUI
limits per crate README). No licensing or distribution clearance is claimed;
M8.5 P0 items remain with the legal reviewer.

---

*End of M9.3 plan. No implementation authorized. Next step is the G0 owner
decision; M1/S1/S2 must not start before it.*
