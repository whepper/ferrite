# M9.1 — FLAC-to-AAC Feature Parity Audit

- Date: 2026-10-24 (UTC)
- Repository: `whepper/fdk-aac`, branch `main`
- HEAD at audit: `4908778` (`test(aac-convert): harden batch resilience coverage`)
- Tracking: `main...origin/main [ahead 12]`, no staged changes, no modified tracked files
- Pre-existing untracked files (not created by this audit, left untouched):
  `docs/aac-convert-m8.1-batch-design.md`, `docs/archive/aac-convert-m8.5-release-readiness.md`
- This document is the sole permitted change for M9.1. No production code, test,
  manifest, lockfile, or existing documentation was modified.

## 1. Executive summary

`flac2aac` (Python, FFmpeg + libfdk_aac + rsgain + mutagen) is a full-library
manager: parallel album-oriented conversion with full metadata preservation,
cover-art handling, EBU R128 / ReplayGain 2.0 + iTunes SoundCheck tagging,
`config.toml`, four existing-file policies, persistent retry state, preflight,
GUI, and an optional RAM-disk working directory.

`aac-convert` (Rust, `rust/convert/`) is a deliberately narrow, dependency-light
converter: native FLAC decode (Symphonia) → native AAC-LC VBR-5 encoder →
M4A (default, `iTunSMPB`-only) or ADTS (engineering format), single file or
sequential directory batch, with atomic no-replace publication and deterministic
reporting. It intentionally omits metadata, cover art, loudness, parallelism,
configuration files, GUI, and working-directory indirection per
`docs/aac-convert-m7.1-architecture-decisions.md` (D3–D7, §4).

Parity conclusion:

- **Already at parity (keep as-is):** recursive mirrored traversal, deterministic
  ordering, VBR-5 highest-quality AAC-LC, M4A container validity, structural
  post-encode verification, dry-run, no-overwrite `skip` semantics including a
  safe-rerun (`--skip-existing`) variant, per-file failure isolation, bounded
  sequential execution, temporary-file hygiene on ordinary error paths.
- **Intentional non-parity (do not copy):** FFmpeg / rsgain subprocesses, VBR 1–4
  selection, RAM-disk `work_dir`, `ThreadPoolExecutor` parallelism, GUI,
  `shutil.move` cross-filesystem staging, Pillow/mutagen processing chains.
  These are implementation details superseded by the native Rust architecture.
- **Genuine product gaps (need decisions before M9.2+):** core Vorbis→M4A tag
  mapping (M7.1 U4, MVP-required), standalone/embedded cover-art policy,
  loudness-tag policy (ReplayGain 2.0 / iTunNORM / none), `newer` / `retag` /
  `rebuild` existing-output semantics, TOML vs CLI-only configuration, and
  operability additions (preflight, persistent retry state, machine-readable
  summary). 24-bit input policy (M7.1 U5) remains unresolved and orthogonal.
- **No work is proposed for VBR 1–4, FFmpeg production use, or RAM-disk
  adoption.** The evidence shows ordinary destination-filesystem temporary
  storage is sufficient for the current pipeline (see §8).

## 2. Audit scope and method

In scope (read-only):

1. Current repo status, branch, HEAD, recent commits, untracked files.
2. `flac2aac` source, README, CLI, configuration, tests, documented behavior
   via network (no local checkout; no guessing where evidence was unavailable).
3. `rust/convert/` README, CLI (`src/main.rs`), library (`src/lib.rs`,
   `src/batch.rs`, `src/flac.rs`, `src/output.rs`, `src/adts.rs`, `src/m4a.rs`,
   `src/gapless.rs`), tests, fixtures, manifests, and notices.
4. Architecture / migration records relevant to the converter.

Out of scope for M9.1 (not implemented, not decided here):

- Any encoder change, VBR 1–4 work, new dependencies, packaging, CI changes.
- Any `flac2aac` behavior requiring a product decision is recorded as
  `needs a decision`, not adopted implicitly.

Method: `git status --porcelain=v1 -b`, `git log --oneline -15`,
`git rev-parse HEAD`, `read` of every file cited in §3, `webfetch` of
`https://github.com/whepper/flac2aac` and
`https://raw.githubusercontent.com/whepper/flac2aac/main/<file>` on
2026-10-24. `zread` repo-structure access for `whepper/flac2aac` returned
`target not found`; that path was not used. No commits, stages, or pushes.

## 3. Evidence inventory

### 3.1 Local repository state

- `git log --oneline -15` top: `4908778`, `320720f`, `43dcd02`, `51b7055`,
  `63179a9`, `3748779`, `e2b5731`, `20c252d`, `6cc14d7`, `1b4ffc7`, `33c3ccc`,
  `f462e41`, `46c1c31`, `0b3b1f9`, `c5ab7d1`. Full 20-line log also verified
  `e51f35f` (MSRV 1.99.1) and earlier encoder docs.
- `git status --short --branch`: `## main...origin/main [ahead 12]`,
  `?? docs/aac-convert-m8.1-batch-design.md`,
  `?? docs/archive/aac-convert-m8.5-release-readiness.md`. No staged or modified files.
- Inspected local files:
  - `rust/convert/README.md`, `rust/convert/Cargo.toml`,
    `rust/convert/THIRD-PARTY-NOTICES.md`, `rust/convert/spike-m4a/README.md`
  - `rust/convert/src/main.rs`, `src/lib.rs`, `src/batch.rs`, `src/flac.rs`,
    `src/output.rs`, `src/adts.rs`, `src/m4a.rs`, `src/gapless.rs`
  - `rust/convert/tests/cli.rs`, `tests/conversion.rs`, `tests/batch.rs`,
    `tests/independent_decode.rs`, `tests/common/mod.rs`,
    `tests/fixtures/README.md`, `tests/fixtures/generate.py`
  - `rust/Cargo.toml` (workspace members `convert`, `framework`, `ffi`),
    `rust/src/aac_enc/encoder.rs` (`AdtsFrame::raw_payload`),
    `rust/src/aac_enc/config.rs` (`EncoderConfig::audio_specific_config`)
  - `docs/aac-convert-m7.1-architecture-decisions.md`,
    `docs/aac-convert-m8.1-batch-design.md`,
    `docs/archive/aac-convert-m8.5-release-readiness.md`,
    `docs/aac-enc-migration-spec.md`, `docs/aac-enc-vbr5-profile.md`,
    `docs/aac-enc-m6-api-contract.md` (§10 M7.6 accessors)

### 3.2 `flac2aac` evidence (remote, `main` branch, 2026-10-24)

Primary: rendered `https://github.com/whepper/flac2aac` README (features,
requirements, configuration reference, CLI flags, exit codes, pipeline
overview, GUI, EXAMPLES/RAM-disk sizing, patent note) plus raw files:

- `config.toml` — `[paths]`, `[encoding]`, `[metadata.cover_file]`,
  `[loudness]`, `[processing]` defaults
- `main.py` — `build_parser`, `apply_cli_overrides`, `cmd_encode`,
  `EXIT_OK/RUNTIME/CONFIG/SIGINT` (= 0/1/2/130)
- `config.py` — `PathsConfig`, `EncodingConfig`, `CoverFileConfig`,
  `MetadataConfig`, `LoudnessConfig`, `ProcessingConfig`, `load_config`,
  `VALID_EXISTING_MODES`, bounds `VBR_QUALITY_MIN/MAX`, `JPEG_QUALITY_MIN/MAX`
- `scanner.py` — `Scanner.scan/scan_all/should_process/_get_destination_path`,
  `rglob("*")`, `suffix.lower() == ".flac"`, `casefold` sort, dedup by resolve
- `encoder.py` — `Encoder.encode` (`ffmpeg -vn -c:a libfdk_aac -profile:a aac_low
  -vbr N -y`, `encode_timeout`), `verify_output` (mutagen duration/channels/
  rate), `verify_ffmpeg` (`-encoders` regex `libfdk_aac`)
- `metadata.py` — `TAG_MAPPING` (20 text atoms), `TAG_INTEGER_MAPPING`
  (`bpm→tmpo`), `TAG_BOOL_MAPPING` (`compilation→cpil`),
  `TAG_FREEFORM_MAPPING` (MusicBrainz/ISRC/label/catalog/barcode/…),
  `MetadataHandler.copy_metadata/validate_album`, `CoverManager`
  (`search_names`, `fallback_name`, `max_size`, `jpeg_quality`, Pillow)
- `pipeline.py` — `Pipeline.run`, `_group_by_album` (by `source.parent`),
  `_process_album`, `_encode_album` (`ThreadPoolExecutor(workers)`),
  `_move_album_to_output` (`replace` + `shutil.move` fallback),
  `_validate_paths`, `ProgressEvent`/`PHASE_*` (8 phases),
  `_validate_unique_destinations` (casefold), `_print_dry_run_report`
- `loudness.py` — `LoudnessProcessor.process_album`, `rsgain easy`,
  `verify_rsgain`, `reuse_existing_replaygain`, `_replaygain_to_soundcheck`
  (`reference_loudness`, 10-field `iTunNORM`), `_REPLAYGAIN_KEYS`
- `preflight.py` — `run_preflight` (input check, `validate_album` warnings,
  `shutil.disk_usage`, tool checks, work/output details)
- `run_state.py` — `RunState` (`.flac2aac-state.json`, atomic
  `.json.tmp→replace`, `failed_sources`, `update`)
- `gui.py` — `App`, `_PROFILES` (Maximum Quality/Apple Music/Portable/
  Retag Existing), saved `settings.json`, RAM-disk create/eject
  (`hdiutil/diskutil`), `PHASE_*` progress bar + 5000-line log + album table,
  pause/cancel/reveal-output/retry-failed
- `pyproject.toml`, `requirements.txt` (`mutagen>=1.47`, `Pillow>=10`,
  `tomli` on <3.11), `EXAMPLES.md` (quick-start, RAM-disk sizing, NAS workflow,
  quality profiles, cron/launchd), `tests/` listing via
  `api.github.com/repos/whepper/flac2aac/contents/tests` (10 files:
  `test_cli/config/encoder/loudness/loudness_rsgain/metadata/pipeline/
  preflight/run_state/scanner` + `conftest.py`). Test bodies were not fetched;
  no claims are made about their assertions beyond file names.

Unresolved remote gaps (explicitly not guessed): exact `flac2aac` commit SHA,
per-test assertions, GUI spec beyond the fetched `gui.py`, and any behavior not
present in the files above.

## 4. Feature-by-feature comparison

Classification: `supported` = already provided by `aac-convert`;
`partial` = partly provided or different semantics; `missing` = absent and
user-visible; `unnecessary` = `flac2aac` implementation detail superseded in
Rust, do not copy; `decision` = needs an explicit product decision first.

| # | `flac2aac` capability (source) | `aac-convert` status (evidence) | Class |
|---|---|---|---|
| F1 | Parallel FLAC→AAC encode, FFmpeg + libfdk_aac, VBR 1–5 selectable (`encoder.py:encode`, `config.toml vbr_quality`, README features) | Native Rust AAC-LC VBR-5 only; no FFmpeg; no quality selector (`lib.rs: EncoderConfig::new(…, FRAME_LENGTH, Adts)`, `flac.rs:validate`, M7.1 D3/D4) | `supported` for VBR-5 quality target; VBR 1–4 explicitly out of scope per task |
| F2 | FFmpeg binary dependency (`ffmpeg_bin`, `verify_ffmpeg`) | Not present by design; Symphonia FLAC + native encoder only (`README.md:Dependencies`, M7.1 D4) | `unnecessary` (production dep forbidden) |
| F3 | `rsgain` binary for ReplayGain 2.0 (`loudness.py`, `verify_rsgain`) | Not present; no loudness analysis (`README.md:Scope limits`) | `unnecessary` as subprocess; loudness *tags* themselves are `decision` (see F6) |
| F4 | Full Vorbis→M4A tag map: 20 text atoms + sort fields (`metadata.py:TAG_MAPPING`) | No tag writing; M4A carries only `iTunSMPB` + brands (`m4a.rs`, `gapless.rs`, `conversion.rs:assert_m4a_structure`) | `missing` (M7.1 §3.1 MVP-required; mapping review U4 outstanding) |
| F5 | Extended pass-through: BPM→`tmpo`, compilation→`cpil`, grouping, MusicBrainz IDs, ISRC, label/catalog/barcode, sort fields (`TAG_INTEGER/BOOL/FREEFORM_MAPPING`) | None of these atoms written | `decision` (subset of F4; full pass-through vs documented core set) |
| F6 | EBU R128 track+album gain + iTunes SoundCheck (`iTunNORM`, `reference_loudness -18.0`) (`loudness.py`) | Only gapless `iTunSMPB`; no `REPLAYGAIN_*`, no `iTunNORM` loudness tag | `decision` (M7.1 §4 out of scope for initial release; do not conflate gapless `iTunSMPB` with loudness `iTunNORM`) |
| F7 | `reuse_existing_replaygain` fast path (copy FLAC RG tags, skip rsgain) | No RG tags to reuse; MD5 fast path is integrity-only (`flac.rs:verify`) | `decision` (depends on F6) |
| F8 | Embedded artwork copy, front-cover preference, JPEG/PNG (`_copy_cover_art`) + `copy_artwork=false` default | No `covr` atom written | `decision` (M7.1 §4: cover art out of scope for initial release) |
| F9 | Standalone cover per album: 16-name search, fallback extract, resize/LANCZOS, JPEG quality, verbatim-copy fast path (`CoverManager`) | No cover files copied or extracted | `decision` (same as F8; needs size/format/ownership policy) |
| F10 | Working directory / RAM disk staging + move (`pipeline.py:use_work_dir`, `_move_album_to_output`, EXAMPLES sizing ~500 MB/album) | Same-directory hidden temp + hard-link publish, no staging dir (`output.rs:TempOutput`, `README.md:Output safety/Temporary files`) | `unnecessary` (see §8; ordinary temp sufficient) |
| F11 | Recursive scan + mirrored output structure (`scanner.py:scan_all`, `_get_destination_path`) | Recursive batch, mirrored structure, canonical `.m4a`/`.aac` (`batch.rs:discover/map_relative_output`, `batch.rs` tests T-flat/T-nested) | `supported` |
| F12 | Case-insensitive `.flac` match, sorted discovery (`suffix.lower()`, `casefold` sort) | ASCII case-insensitive `.flac`, byte-sorted plan/report (`batch.rs:is_flac_name/sort_key`, tests `flac_name_matching_is_case_insensitive`) | `supported` (sort-key domain differs: casefold vs byte order; both deterministic) |
| F13 | Single `config.toml` (`config.py:load_config`, 5 sections, unknown-key error) | CLI flags only; no config file (`main.rs:parse_cli`, no TOML dep in `Cargo.toml`) | `decision` (M7.1 U7, M8.1 §10 deferred; no TOML proposed here) |
| F14 | CLI overrides `--input/--output/--workers/--log-level/--existing-mode/--no-verify-output/--retry-failed` + `--config/--dry-run/--version` (`main.py`) | `--format/--recursive/--no-recursive/--dry-run/--skip-existing/-v/--quiet/--help/--version/--` (`main.rs:parse_cli`, `tests/cli.rs`) | `partial` (different surfaces; workers/log-level map to no-ops in sequential CLI) |
| F15 | Existing policies `skip/newer/retag/rebuild` (`scanner.should_process`, `_encode_file retag_existing`) | `skip` only: existing = per-file failure by default; `--skip-existing` turns it into a no-decode skip (`batch.rs:execute_plan`, `main.rs:run_single_file/run_batch`) | `partial` (`skip` = `supported`; `newer/retag/rebuild` = `missing`, deferred per M8.1 §10 Q7) |
| F16 | Post-encode verify duration/channels/rate via mutagen (`encoder.verify_output`, `verify_output=true`) | Structural ADTS/M4A checks + independent FFmpeg/CoreAudio decode evidence (`adts.rs:validate`, `m4a.rs`, `conversion.rs`, `independent_decode.rs` ignored) | `partial` (Rust checks strictly more about container integrity; duration-tolerance policy differs) |
| F17 | Persistent retry state `.flac2aac-state.json` + `--retry-failed` (`run_state.py`, `Pipeline.retry_failed_only`) | No state file; reruns re-plan from filesystem; `--skip-existing` covers idempotent reruns | `decision` (M7.1 §4 retry-state out of scope; state-file format/location needs design) |
| F18 | Per-album grouping + full-album loudness on final outputs incl. retained tracks (`_group_by_album`, `process_album(final_dest_files)`) | No album concept; files independent; no loudness pass | `decision` (album grouping only matters if F6/F9 adopted) |
| F19 | Metadata validation warnings (missing title/album, dup disc/track, inconsistent album) (`validate_album`) | No metadata warnings; input validation is codec-level (rate/channels/bit-depth/MD5) (`flac.rs`, `lib.rs:validate`) | `decision` (depends on F4) |
| F20 | Progress: monotonic file counter, 8 `PHASE_*` events, per-album table, 5000-line GUI log (`pipeline.py`, `gui.py`) | One stdout line per file + stderr diagnostics + final summary; `-v`/`-q` (`main.rs:run_batch`, `batch.rs` tests) | `partial` (CLI parity adequate; GUI/phase events = `unnecessary` for CLI) |
| F21 | Logging levels DEBUG/INFO/WARNING/ERROR (`log_level`, `setup_logging`) | No log levels; fixed stdout/stderr contract (`main.rs`, README batch section) | `decision` (verbosity pair already covers CI need; full levels not proposed) |
| F22 | Preflight: source count/size, album warnings, free-space, tool checks (`preflight.py`) | Fatal-setup checks only (missing/unreadable/empty/no-FLAC/duplicate/same-root/output-root) (`batch.rs:plan_batch`, `main.rs:dispatch_input`) | `partial` (safety subset = `supported`; capacity/tool/album-warning layer = `decision`) |
| F23 | Temp hygiene: unlink partial dest on failure, work-dir cleanup on album failure (`_encode_file`, `_process_album`) | Temp removed on ordinary errors via `Drop`; destination untouched; orphans only on fatal signals (`output.rs:Drop`, README temp section) | `supported` (Rust guarantee strictly stronger: never replaces existing) |
| F24 | Pause-between-albums, cancel, reveal-output, saved settings, profiles (`gui.py`) | No GUI, no pause/cancel (Ctrl-C = OS default), no settings (`README.md:Scope limits`, M7.1 D7) | `unnecessary` |
| F25 | Duplicate-destination guard incl. casefold (`_validate_unique_destinations`) | Exact + ASCII-fold duplicate abort before any conversion (`batch.rs:find_duplicate`, tests) | `supported` |
| F26 | Symlink/cycle handling: `resolved in seen` dedup; work/output inside-tree permitted | Dir symlinks never descended; file symlinks converted at link path; nested output root permitted (`batch.rs:discover`, `batch.rs` symlink tests) | `partial` (same safety outcome; mechanism differs; `flac2aac` hardlink-alias dedup has no Rust equivalent — two links convert independently by design) |
| F27 | Hidden files: no exclusion (`rglob("*")` traverses all) | Hidden traversed without special-casing (`batch.rs:discover`, test `hidden_files_and_directories_are_traversed`) | `supported` |
| F28 | Non-FLAC entries ignored implicitly (only `.flac` collected) | Tallied `ignored`, never failures; verbose `ignore <path>` (`batch.rs`, `main.rs`) | `supported` (Rust makes the tally explicit) |
| F29 | Output naming: stem + `output_format` (`m4a`/`mp4`) | Single-file: infer `.m4a`/`.mp4`→M4A else ADTS; batch: canonical `.m4a` (M4A) / `.aac` (ADTS), never `.mp4`/`.adts` (`lib.rs:from_output_path`, `batch.rs:batch_extension/map_relative_output`) | `partial` ( Covered: stem preservation, multi-dot stems, case preservation; Differ: batch `.mp4`/`.adts` never emitted by design) |
| F30 | Exit codes 0/1/2/130 (`main.py:EXIT_*`) | 0/1/2/3 (`main.rs:CliError`, `batch.rs:BatchError`); 3 = fatal setup, no 130 mapping | `partial` (0/1/2 agree in meaning; 3 is additive per M8.1 §7.2; SIGINT deferred per M8.1 §6.4) |
| F31 | Encode timeout 1800 s kill (`encode_timeout`, `TimeoutExpired`) | No timeout; corrupt input fails fast on decode/MD5 (`flac.rs`, `output.rs` cleanup tests) | `decision` (hang class differs: subprocess vs in-process decode; needs a bounded-decode argument, not a ported timeout) |
| F32 | Input breadth: any rate/channels/bit-depth FFmpeg accepts (resample/downmix by FFmpeg flags where configured) | 16-bit, 44.1/48 kHz, mono/stereo only; 24-bit/32 kHz/3ch rejected, no resample/downmix (`AudioProperties::validate`, `flac.rs` tests) | `supported` at documented scope; broader breadth = `decision` (M7.1 U5 24-bit policy; resample/downmix out of scope) |
| F33 | Integrity: no checksum enforcement beyond FFmpeg exit | StreamInfo MD5 enforced when present; absent/zero MD5 accepted without verified claim (`flac.rs:verify`, `conversion.rs` MD5 tests) | `supported` (Rust strictly stronger where checksums exist) |
| F34 | Tests: `tests/test_*.py` (10 files, names only verified) | `tests/cli/conversion/batch/independent_decode` + unit tests in every `src/*.rs` + `common/mod.rs` parsers | `partial` (different harnesses; Rust lacks metadata/cover/loudness/album tests because features absent) |
| F35 | Distribution: PyInstaller `.app` with vendored FFmpeg/rsgain; MIT license | Workspace source bundle, pinned toolchain 1.99.1, `cargo build --locked`; FDK-AAC + MPL-2.0 + Apache/MIT/BSD notices; no registry package (`README.md:Distribution`, `THIRD-PARTY-NOTICES.md`, M8.5 blockers) | `partial` (both source-buildable; legal gates differ; no parity claim) |

## 5. CLI behavior and configuration differences

`flac2aac` (`main.py:build_parser`, `config.toml`):

```text
python main.py [--config PATH] [--dry-run] [--input DIR] [--output DIR]
  [--workers N] [--log-level LEVEL] [--existing-mode MODE]
  [--no-verify-output] [--retry-failed] [--version]
```

- Configuration-first: `config.toml` holds `[paths]` (incl. `ffmpeg_bin`,
  `rsgain_bin`, optional `work_dir`), `[encoding]` (`vbr_quality`,
  `output_format m4a|mp4`, `encode_timeout`), `[metadata.cover_file]`,
  `[loudness]`, `[processing]` (`workers`, `existing_mode`, `verify_output`,
  `retry_failed_only`, `log_level`). CLI overrides patch loaded config
  (`apply_cli_overrides`); `workers<1` rejected; `overwrite_existing` derived
  from `rebuild`. Unknown TOML keys are errors (`config.py:_build_section`).
- Album-oriented: input/output are directories; files grouped by parent
  directory for encode + loudness + cover stages.

`aac-convert` (`src/main.rs:USAGE`, `parse_cli`, `run_single_file/run_batch`):

```text
aac-convert [--format adts|m4a] <input.flac> <output>
aac-convert [--format m4a|adts] [--recursive|--no-recursive] [--dry-run]
  [--skip-existing] [-v|--verbose] [-q|--quiet] <input-dir> <output-root>
```

- No config file, no `--workers/--log-level/--existing-mode/--retry-failed`
  equivalents. `--format` accepts `m4a|adts` plus `=` form; last of
  `--recursive/--no-recursive` wins; `-v`+`-q` conflict is a usage error.
- Mode dispatch by input file type, never output (`dispatch_input`):
  file→single-file pipeline unchanged; directory→batch. Symlinked inputs
  canonicalized first. Single-file mode rejects batch-only flags as usage
  errors (keeps `converted … (N frames, B bytes, FORMAT)` line stable).
- Batch defaults: `--format` defaults to `m4a`; traversal recursive; outputs
  `.m4a`/`.aac` only; `--dry-run` plans without creating directories or
  decoding; `--skip-existing` skips without decoding (never overwrites).
- Streams: per-file `ok/skip/plan/fail` on stdout, `error: …` diagnostics on
  stderr, always a final `summary:`/`dry-run:` line; `--verbose` adds audio
  detail + `ignore` lines, `--quiet` leaves only the summary on stdout.
  Exit codes `0` success / `1` per-file failure / `2` usage / `3` fatal setup.
  Single-file keeps `0/1/2` only (missing parent stays `OutputDirectoryMissing`
  → 1, not 3).

Consequence: there is no flag-to-flag mapping for `workers`, `log_level`,
`existing-mode=newer/retag/rebuild`, `no-verify-output`, `retry-failed`,
`--config`, or `work_dir`. Adopting any of them is a product decision with new
semantics, not a rename.

## 6. Input restrictions, formats, metadata, naming, traversal, overwrite, errors

- **Input restrictions.** Rust accepts native FLAC, 16-bit, 44.1/48 kHz,
  mono/stereo (`flac.rs:FlacSource::open/properties/validate`,
  `lib.rs:AudioProperties::validate`). 24-bit, other rates, 3+ channels rejected
  with named variants (`UnsupportedBitDepth/SampleRate/Channels`); mid-stream
  spec changes → `DecodeFailed`; truncated/undecodable → `DecodeFailed` with no
  publication; absent/zero MD5 accepted after successful decode
  (`flac.rs:verify`, `conversion.rs:accepts_valid_flac_with_absent_md5`).
  `flac2aac` accepts whatever FFmpeg decodes (plus timeout kill). The Rust
  restriction is scope, not a defect; widening needs M7.1 U5 (24-bit dither) and
  resample/downmix decisions, all currently out of scope.
- **Output formats.** Rust M4A = streaming `shiguredo_mp4` muxer over validated
  raw payloads + ASC (`m4a.rs:M4aWriter::create/write_frame/finish`,
  `encoder.rs:raw_payload`, `config.rs:audio_specific_config`); ADTS = raw
  validated frames (`adts.rs:validate`). Both containers share identical AAC
  payloads (`conversion.rs:m4a_payload_matches_adts_access_units`). `flac2aac`
  emits `m4a`/`mp4` via FFmpeg `libfdk_aac -vbr -profile:a aac_low`; no ADTS
  product path. Batch canonical extensions (`.m4a`/`.aac`) vs `flac2aac`
  `m4a`/`mp4` is an intentional simplification (M8.1 Q2/Q3).
- **Metadata handling.** Rust writes no user metadata (only gapless `iTunSMPB`
  + brands; `gapless.rs:itunsmpb_value/udta_box`, `m4a.rs:finish`). `flac2aac`
  writes ~30 mappings (F4–F7) plus cover and loudness. Gap: entire tag layer.
  Do not treat `iTunSMPB` (gapless) as loudness `iTunNORM` (SoundCheck); the
  names resemble each other but the atoms, writers, and purposes are disjoint
  (`gapless.rs` vs `loudness.py:_replaygain_to_soundcheck`).
- **Output naming.** Both preserve relative structure and stem byte-for-byte.
  Rust batch replaces the suffix with lowercase canonical extension and aborts
  on exact + ASCII-fold collisions (`batch.rs:map_relative_output`,
  `find_duplicate`). `flac2aac` replaces suffix with configured format and
  aborts on `casefold` collisions (`scanner._get_destination_path`,
  `pipeline._validate_unique_destinations`). Equivalent safety; Rust additionally
  pins `.mp4`/`.adts` out of batch outputs.
- **Directory traversal.** Both recursive by default, mirrored, hidden-inclusive.
  Rust: byte-sorted plan/report, dir symlinks never followed, file symlinks at
  link path, FIFOs/sockets/devices ignored, unreadable subtrees become
  per-file `UnreadableEntry` (`batch.rs:discover`, `sort_key`). `flac2aac`:
  `casefold`-sorted, dedups resolved inodes, no explicit symlink-dir rule in
  fetched code. Rust rule is stricter and tested (`batch.rs` symlink/perm tests).
- **Overwrite behavior.** Rust: never overwrite; pre-existing destination =
  failure by default, skip under `--skip-existing`; atomic hard-link is the
  enforcement point, including publication races (`output.rs:publish`,
  `batch.rs:execute_plan`, race tests). `flac2aac`: `skip` (default), `newer`
  (mtime compare), `retag` (metadata/loudness only), `rebuild` (re-encode);
  moves use `replace` with `shutil.move` fallback. Only `skip` has a Rust
  counterpart; the other three need timestamp/hash/retag semantics designed.
- **Error reporting.** Rust: typed `ConvertError` (22 variants) + stable
  `category_for` strings + `BatchError` fatals + exit-code contract
  (`lib.rs:ConvertError`, `batch.rs:category_for/BatchError`,
  `main.rs:CliError`). `flac2aac`: log-level messages + `ProcessingStats`
  (total/successful/failed/skipped/albums) + `PreflightReport.errors/warnings`
  + `RunState` per-track status. Rust favors greppable stdout + stderr detail;
  `flac2aac` favors human logs + GUI table.

## 7. Operational features

- **Progress reporting.** Rust batch prints `batch: …` header, per-file lines in
  plan order, and a stable summary line parsed by tests
  (`main.rs:run_batch`, `batch.rs` CLI tests `cli_batch_*`). `flac2aac` emits
  `ProgressEvent(phase, album_index/total, track_index/total, files_done/total)`
  driving a monotonic GUI bar plus per-album table (`pipeline.py:PHASE_*`,
  `gui.py:_handle_progress`). CLI parity is adequate; GUI phases are out of
  scope per M7.1 D7.
- **Logging.** `flac2aac` log levels + module loggers + GUI 5000-line capped
  view. Rust has no levels; verbosity is the `-v/--quiet` pair plus stderr
  diagnostics. No change proposed: levels add surface without a demonstrated
  consumer.
- **Batch processing.** Both group work deterministically and isolate per-file
  failures (Rust continues to end of plan; `flac2aac` continues to end of album
  list, counting `albums_failed`). Rust is strictly sequential with bounded
  memory (`CHUNK_FRAMES=8000`, streaming muxer keeps only per-sample metadata;
  `lib.rs:CHUNK_FRAMES/encode_chunks`, `m4a.rs` docs). `flac2aac` uses
  `ThreadPoolExecutor(workers=4)` per album plus whole-album loudness/cover
  passes. Parallelism is explicitly deferred (M7.1 D6, M8.1 §10); no worker
  pool is proposed.
- **Temporary storage.** See §8.
- **Platform-specific behavior.** Rust gated tests cover Unix symlinks and
  permission-denied subtrees; Windows/FAT/exFAT/network-FS/disk-full/forced-kill
  remain documented-outstanding (M8.5 §5). `flac2aac` handles cross-filesystem
  moves via `shutil.move` fallback and documents RAM-disk sizing per OS
  (EXAMPLES.md). Rust deliberately fails safe on non-hard-link filesystems
  (`Publish` error, no rename fallback) — a documented limitation, not a gap to
  close without a decision.

## 8. Temporary storage: RAM disk vs ordinary temp files

`flac2aac` `work_dir` exists to (a) avoid slow NAS/HDD round-trips during the
multi-rewrite album cycle (encode → mutagen tag → Pillow cover → rsgain →
mutagen iTunNORM) and (b) keep half-finished albums out of `output_dir`
(EXAMPLES.md NAS workflow, `pipeline._move_album_to_output`).

Neither motive applies to `aac-convert` today:

- Single-pass pipeline: decode → bounded encode pushes → streaming muxer write
  → one metadata append (`iTunSMPB`/`ftyp` splice) → flush/sync → hard-link
  publish (`lib.rs:convert_inner`, `m4a.rs:finish`, `output.rs:publish`).
  No tag-rewrite or second-pass analysis touches the file after publication.
- Same-directory temp files already keep incomplete outputs out of the tree and
  preserve atomicity: `create_new` hidden `.aac-convert-<pid>-<seq>.tmp`,
  `sync_all`, `hard_link` (fails if destination appeared), `Drop` cleanup
  (`output.rs:TempOutput`, README `Temporary files`). Orphans occur only on
  fatal signals/power loss, are never valid outputs (`.flac`-only discovery),
  never block reruns, and are safe to delete manually — all asserted by
  `temp_output_files(...).is_empty()` checks and concurrent-publication tests.
- Measured streaming profile (`spike-m4a/README.md:Memory`): 120 s stereo
  input → ~4.2 MB peak RSS while payload grows ~564 KB; complete output never
  buffered. Batch overhead is the sorted path list only.

Decision: **do not add a working directory or assume a RAM disk.** Ordinary
filesystem-backed temp files in the destination directory are sufficient and
preserve the same-filesystem hard-link guarantee that a RAM disk would break
(cross-filesystem staging would force a copy fallback and weaken atomicity).
Revisit only if a future multi-rewrite feature (metadata + loudness tagging)
demonstrates a measured NAS/SSD bottleneck — with benchmarks, not by default.

## 9. Missing tests and important edge cases

Rust coverage is strong for its scope (`batch.rs` T-matrix, `conversion.rs`
structure/payload/integrity, `cli.rs` exit codes, `common/mod.rs` MP4/ADTS
parsers, ignored `independent_decode` FFmpeg smokes). Gaps fall into two
classes:

Already-scoped but untested (consider regardless of parity):

- Empty (0-frame) FLAC: `GaplessAccounting::compute` admits `S=0` arithmetically
  (`gapless.rs`), but converter acceptance of empty input is untested.
- Very long single file (30-min equivalent RSS-flatness, M8.1 T-bounded-1):
  specified but no automated bound test committed.
- Non-UTF8 / adversarial file names through plan→report→output mapping
  (byte-key design claims support; no explicit test).
- Unreadable input *file* (vs tested unreadable *subdirectory*), output root
  under a file, destination parent permission denial at batch depth.
- `SIGINT`/`SIGTERM` mid-batch and power-loss orphan recognition (documented
  only; M8.5 P2).
- FAT/exFAT/network-FS `Publish` path and true disk-full at temp-write vs
  output-root-creation boundary (M8.5 P1).

Out-of-scope features with no tests because the features are absent (expected;
add tests only with the features):

- No Vorbis→M4A mapping tests (cf. `flac2aac` `test_metadata.py`), no
  sort-field/freeform/BPM/compilation coverage, no `validate_album` warning
  tests, no unknown-tag documentation test.
- No cover tests (search order, fallback extract, resize/quality, transparency
  flatten, verbatim-copy fast path; cf. Pillow branches in `CoverManager`).
- No loudness tests (rsgain invocation, full-album-on-partial-rerun, `iTunNORM`
  vector incl. `reference_loudness` shifts, `reuse_existing_replaygain`; cf.
  `test_loudness*.py`).
- No album-group tests (group-by-parent, per-album failure isolation counting
  `albums_failed`, cover/loudness applied to final outputs).
- No config-file tests (unknown-key errors, bounds `vbr_quality/jpeg_quality/
  reference_loudness`, `overwrite_existing→existing_mode` migration; cf.
  `test_config.py`).
- No retry-state tests (atomic `.flac2aac-state.json`, `failed_sources`
  filtering; cf. `test_run_state.py` + `test_pipeline.py` retry paths).
- No preflight tests (free-space, tool presence, album warnings; cf.
  `test_preflight.py`).
- No scanner-policy tests for `newer` (mtime), `retag` (no re-encode), `rebuild`
  (replace) — the Rust `skip`/`skip-existing` matrix is the only policy tested.

## 10. Prioritized proposed work (M9.2+, rationale, dependencies)

Constraints honored: no VBR 1–4, no FFmpeg production dependency, no RAM disk,
AAC-LC VBR-5 only. Ordering favors MVP obligations (M7.1 §3) over post-MVP
convenience, and decisions before code.

| Pri | Proposal | Rationale | Dependencies / notes |
|---|---|---|---|
| P0 | **Record metadata-scope decision (U4):** exact Vorbis→M4A core table to implement, explicitly listing unsupported tags; cover and loudness stay out. | M7.1 §3.1/§3.3 requires core metadata for MVP success (`preserves the documented core metadata`); without the table no metadata code can start. | Owner/product sign-off; audit `metadata.py:TAG_*` as reference only, not as target. Pure-Rust tag library evaluation (license/MSRV) is part of the decision, not pre-decided here. |
| P1 | **Implement decided core-tag writing** (text atoms + track/disc tuples only; no freeform/BPM/compilation until P0 says so), with unknown-tag documentation test. | Smallest user-visible parity item; unblocks real-library use where titles/artists matter more than loudness. | P0 table; M4A incremental-metadata mechanism (write before/after `finish`); structural + FFmpeg-decode tests. No encoder change. |
| P2 | **Record 24-bit input policy (U5):** reject (status quo) vs defined TPDF/dither to `i16`; no silent truncation either way. | Orthogonal to metadata but blocks a large library subset; current `UnsupportedBitDepth` is correct but needs a deliberate keep-or-convert decision. | Listening/objective quality review; `flac.rs:validate` + fixture `stereo-48k-24.flac` already pin the reject path. No `i32`/float encoder API expansion in this phase (M7.1 D10). |
| P3 | **Decide `newer` semantics (mtime + hash?) or defer explicitly.** | `skip` + `--skip-existing` already covers clean reruns; `newer` is the next most requested `flac2aac` policy and the cheapest after `skip`. | Timestamp-resolution + clock-skew analysis; `scanner.should_process` mtime branch as reference. `retag`/`rebuild` stay deferred (see P5). |
| P4 | **Decide loudness-tag policy:** none vs ReplayGain 2.0 vs iTunNORM vs both; if adopted, decide full-album-on-partial-rerun and `reference_loudness` handling. | Loudness is explicitly post-MVP (M7.1 §4); adopting rsgain as a subprocess is forbidden, so a native analysis or tag-only path needs research first. Do not start with implementation. | Depends on album-grouping decision (F18); native R128 library evaluation or documented wont-do. |
| P5 | **Decide cover-art policy:** none vs standalone-copy vs embedded `covr`, with size/format/ownership rules. | Same post-MVP class as loudness; standalone copy has fewer container risks than embedded art. | Pillow-equivalent native image handling evaluation; per-album test fixtures. |
| P6 | **Decide configuration surface:** CLI-only (status quo) vs versioned TOML with unknown-key errors and bounds. | `flac2aac` TOML covers 5 sections; Rust CLI covers conversion-only flags. No evidence of a consumer need for TOML yet (M8.1 Q8 pattern). | If adopted: schema versioning, path canonicalization, and migration from CLI flags; no new runtime deps without review. |
| P7 | **Operability hardening (independent, any order):** machine-readable summary line (opt-in), persistent retry-state file design, preflight capacity/album-warning report, `SIGINT`-finish-current-file behavior. | Each is small, testable, and requested by `flac2aac` practice; none changes audio. | No dependencies between items; keep exit-code contract additive (M8.1 §7.2). |
| — | **Explicitly not proposed:** VBR 1–4 selector, FFmpeg/rsgain production use, RAM-disk `work_dir`, parallel workers, GUI, `mp4` batch extension, universal-gapless claims. | Forbidden by task, M7.1 D3–D7, or evaluated as unnecessary (§8). | Revisit parallelism only via a new architecture decision overturning M7.1 D6. |

## 11. Unresolved questions

1. Core tag set: which of `TAG_MAPPING`’s ~20 atoms plus sort fields constitute
   “core” for MVP? (`flac2aac` treats all as core; M7.1 requires a reviewed
   subset.)
2. Should any freeform/BPM/compilation atoms ship in MVP, or are they
   documented-unsupported?
3. Loudness: is any loudness tag required for the first user-facing release, or
   is gapless `iTunSMPB` alone sufficient?
4. Cover: standalone copy, embedded `covr`, both, or neither for MVP?
5. 24-bit: keep rejecting or specify conversion/dither?
6. `newer`: mtime-only or content-hash gated? What clock guarantees?
7. Config file: needed at all before multi-library use demonstrates it?
8. Retry state: file location/format (output-root JSON vs elsewhere) and
   concurrency story?
9. Preflight scope: capacity checks without a work-dir concept?
10. Long-file memory bound: what RSS ceiling gates T-bounded-1 automation?

## 12. Sources and traceability

- `flac2aac` file:line anchors are to `main` as fetched 2026-10-24 (see §3.2).
  Where line numbers were not captured via `webfetch` text (README, EXAMPLES),
  the section name is the anchor.
- Rust file:line anchors use the working tree at `4908778` (see §3.1).
- Architecture anchors: M7.1 D1–D10/§3–§5/§7–§8, M8.1 §§4–7/§9–§11, M8.5 §§4–8,
  migration-spec §§3/5, VBR-5 profile §§4–8, M6 contract §10.
- No `flac2aac` code was copied; no behavior was inferred beyond fetched files.
  Items marked `decision` have no implementation proposal in this milestone.

---

*End of M9.1 audit. No implementation authorized. Next step is a separately
scoped M9.2 proposal addressing P0–P2 only after product sign-off.*
