# M8.1 — Batch Conversion Architecture and CLI Design

> Maintained architecture: this document describes the shipped batch workflow and output-safety invariants. The original implementation-status banner is historical.

Design-only specification. No implementation is authorized by this document.
No Rust source, manifest, lockfile, fixture, test, or CI change is authorized.

- Date: 2026-10-23
- Baseline: `main` at `51b7055` (`research(aac-convert): preserve M4A muxer feasibility spike`),
  9 commits ahead of `origin/main`, working tree clean at audit start.
  The five M7 converter commits sit on top of four pre-existing local commits
  (`20c252d`, `6cc14d7`, `1b4ffc7`, `33c3ccc` above the `f462e41`/`46c1c31` base;
  see `git log --oneline -15` for the exact stack).
- Authority: this document specifies batch behaviour for M8.2 onward. It does
  not change the M7.1 decision record (`docs/aac-convert-m7.1-architecture-decisions.md`);
  no M7.1 amendment was needed (see §11, Q10).
- Constraints carried forward: sequential execution (M7.1 D6), M4A-first product
  direction with ADTS retained as engineering format (M7.1 D1), no FFmpeg in
  production (M7.1 D4), no RAM-disk handling (M7.1 D5), bounded memory with
  destination-filesystem temp files (M7.1 §5.4), no GUI (M7.1 D7).

## 1. Goals and explicit non-goals

### Goals

1. Specify exact, implementation-ready directory/batch semantics layered on the
   existing single-file `convert_file_with_format` pipeline without changing
   its per-file guarantees.
2. Keep the CLI surface minimal: two positionals plus `--format`, one
   recursion switch pair, `--dry-run`, and a bounded verbosity pair.
3. Preserve every existing safety property: no-overwrite, no partial outputs,
   atomic no-replace publication, bounded per-file memory, deterministic
   ordering.
4. Define a failure matrix and an exit-code model that distinguishes total
   success, partial batch failure, invalid invocation, and fatal setup failure.
5. Define a test plan with measurable acceptance criteria separable into
   deterministic automated tests, platform-specific tests, and manual checks.

### Non-goals (explicitly out of scope for the first batch release, M8.2)

- Parallel workers, async runtime, rayon/thread pools, or any concurrency
  beyond the already-tested concurrent-*publication race* (which is a safety
  property of `TempOutput::publish`, not an execution model).
- Overwrite/skip/newer/rebuild policies, `--force`, `retag`-only mode,
  retry-state/resume files, TOML configuration.
- Metadata/tag mapping, cover art, ReplayGain/loudness, resampling,
  downmixing, channel upmixing, VBR 1–4, new encoder algorithms, `i32`/float
  PCM API expansion, 24-bit acceptance (rejection stands).
- New output containers (raw AAC, LATM/LOAS, ADIF, fragmented MP4, MP4 video).
- Edit-list output on any default path (stays behind `GaplessExperiment`).
- Multiple explicit input paths in one invocation (deferred; see §11, Q6).
- Graceful signal handling (deferred; see §6.5).
- New runtime dependencies of any kind (see §6.6).
- Any universal-gapless-playback claim; the README's VLC/GUI limitations stand.

## 2. Current-state summary

Evidence from the audit (all paths relative to the repository root).
Quoted behaviour was read from source, not inferred from the milestone brief.

### 2.1 Crate shape and I/O model

- `rust/convert/` is a separate application crate (`aac-convert` 0.1.0,
  edition 2021, MSRV 1.99.1) depending one-way on `aac` (`aac_enc` feature),
  `symphonia` 0.6.1 (`flac` only), and pinned `shiguredo_mp4 =2026.5.0`
  (`rust/convert/Cargo.toml`, `rust/convert/README.md`).
- Single-file pipeline (`rust/convert/src/lib.rs`, `convert_inner`):
  `output::resolve_paths` → `flac::FlacSource::open` → `properties.validate`
  → `EncoderConfig::new(rate, channels, FRAME_LENGTH, Adts)` → bounded
  `encode_chunks` (cap `CHUNK_FRAMES = 8_000` sample frames per channel per
  encoder push) → `source.verify()` (FLAC MD5) → encoder flush → `sink.finish`
  → `sink.publish`. Whole-file PCM and whole-file AAC are never buffered;
  the interleaved packet staging vector is bounded by the decoder's FLAC
  block size, which may exceed `CHUNK_FRAMES` (documented in `lib.rs`).
- Format selection (`OutputFormat::from_output_path`): `.m4a`/`.mp4`
  (ASCII case-insensitive) select M4A; every other extension, unknown
  extensions, and no extension select ADTS. Explicit `--format` overrides
  inference (`src/main.rs`, `src/lib.rs`, unit test
  `output_format_inference_is_documented`).
- M4A structure (`src/m4a.rs`, `src/gapless.rs`, README §"M4A structure and
  timing"): one `mp4a` track, timescale = sample rate, 1024 samples/frame,
  `mdhd` duration = `N * 1024` (encoded timeline deliberately kept; rewriting
  it breaks FFmpeg tail trimming), `stts`/`stsz`/`stco` from actual frames,
  `esds` carrying the 2-byte ASC (`11 90` stereo-48k, `12 08` mono-44.1k),
  `moov.udta.meta.ilst` `iTunSMPB` with (D=1600, P, S), Apple-style `M4A`
  brands, no `edts`/`elst` on the default path. ADTS payload bytes and M4A
  `mdat` payload bytes are identical (test `m4a_payload_matches_adts_access_units`).

### 2.2 Input contract

- Accepted: native FLAC, 16-bit, 44.1/48 kHz, mono/stereo (`flac.rs`,
  `AudioProperties::validate`). 24-bit, other rates, other channel counts are
  rejected with `UnsupportedBitDepth/SampleRate/Channels` — no silent
  truncation, no resampling, no downmixing.
- Integrity: Symphonia decoder opened with `verify = true`; present-but-wrong
  StreamInfo MD5 fails closed as `DecodeFailed` with no output published;
  absent/zero MD5 is accepted after successful decode with no verified-audio
  claim (README, tests `rejects_md5_mismatch_without_publishing_output`,
  `accepts_valid_flac_with_absent_md5`, truncated-input equivalents).
- Non-FLAC bytes and empty files surface as `InputNotFlac`; mid-stream spec
  changes surface as `DecodeFailed`.

### 2.3 Output safety (the invariants the batch design must not weaken)

`src/output.rs`, `src/m4a.rs` (`M4aWriter` owns a `TempOutput`), README
§"Output safety":

1. **Pre-check, no clobber.** `resolve_paths` canonicalizes the input;
   `symlink_metadata` on the output: anything existing (file, dir, symlink —
   including a symlink whose target is the input, tested on Unix) → either
   `SamePath` (resolves to the input) or `OutputExists`. Missing parent →
   `OutputDirectoryMissing` (parent is canonicalized; bare filenames resolve
   against `.`). The joined canonical-parent + file-name is re-compared to the
   input for `SamePath`.
2. **Same-filesystem temp file.** Hidden `.aac-convert-{pid}-{seq}.tmp`
   created with `create_new(true)` in the destination directory (64 name
   retries). Flush + `sync_all` before publication; write/seek/set_len/read
   failures map to `TempWrite`; flush/sync failures map to `Finalize`.
3. **Atomic no-replace publication.** `fs::hard_link(tmp, dest)`: success
   publishes; `AlreadyExists` (including a concurrent winner) maps to
   `OutputExists`; anything else maps to `Publish`. There is deliberately **no
   check-then-rename fallback**; filesystems without hard-link support
   (FAT/exFAT, some network FS) fail safely with `Publish`.
4. **Failure cleanup.** `TempOutput::drop` removes the temp file unless
   publication completed. Tests assert `temp_output_files(dir).is_empty()`
   after every success and failure path, and `concurrent_publication_never_replaces_the_winner`
   / `concurrent_m4a_publication_never_replaces_the_winner` prove exactly one
   racer wins and the loser observes `OutputExists` with the winner's bytes intact.

### 2.4 CLI as built

`src/main.rs`, `tests/cli.rs` (9 tests):

```text
Usage: aac-convert [--format adts|m4a] <input.flac> <output>
```

- `--format adts|m4a` (also `--format=...`), `-h/--help`, `-V/--version`,
  `--` end-of-options. Unknown dash-options are usage errors.
- Exactly two positionals; anything else is a usage error.
- Exit codes today: `0` success (help/version also `0`), `1` conversion
  failure with an actionable `error: ...` on stderr, `2` usage error plus
  usage text. Success prints
  `converted <in> -> <out> (<frames> AAC frames, <bytes> bytes, <FORMAT>)`.
- No directory mode, no recursion, no dry-run, no verbosity flags exist.

### 2.5 Test and distribution posture

- `cargo test -p aac-convert` covers ADTS structure, M4A box/sample-table
  structure, payload equivalence, integrity failures, collisions, concurrent
  publication, gapless accounting, and CLI behaviour. `tests/independent_decode.rs`
  holds two `#[ignore]`d FFmpeg/ffprobe smoke tests (ADTS + M4A).
- Fixtures (`tests/fixtures/`): `stereo-48k-16`, `mono-441k-16` (supported);
  `stereo-48k-24`, `mono-32k-16`, `three-channel-48k-16` (rejections);
  `truncated-stereo-48k-16`, `md5-mismatch-stereo-48k-16`, `no-md5-stereo-48k-16`.
- Distribution is a workspace source bundle with the pinned toolchain
  (`cargo build --locked --release -p aac-convert`); no `cargo package`.
  `THIRD-PARTY-NOTICES.md` and AAC patent authorization still need human/legal
  sign-off. Player-level gapless behaviour is not universally verified
  (VLC approximates, GUI players unverified).

## 3. Proposed CLI syntax with realistic examples

### 3.1 Design principles

- Two positionals stay positional: `<input> <output>`. The **input's file
  type selects the mode**, never the output's: a file input means
  single-file mode (byte-for-byte current behaviour); a directory input means
  batch mode. This avoids ambiguous "is this output a file or a root?" inference.
- Flags are explicit and few. NoDWIM: the tool never guesses recursion,
  format, or overwrite intent beyond the documented defaults below.
- Single-file invocations keep exit codes `0/1/2` exactly. Directory mode
  adds exit `3` for fatal setup (see §7.2). No existing invocation changes
  meaning.

### 3.2 Syntax

```text
Usage:
  aac-convert [--format m4a|adts] <input.flac> <output>
  aac-convert [--format m4a|adts] [--recursive|--no-recursive] [--dry-run]
              [-v|--verbose] [-q|--quiet] <input-dir> <output-root>

Single-file mode (unchanged):
  --format m4a|adts   Explicit container. Without it, .m4a/.mp4 imply M4A,
                      everything else implies ADTS. Wins over the extension.

Batch (directory) mode (new):
  --format m4a|adts   Default `m4a`. Selects the output container AND the
                      output extension (.m4a / .aac). Extension inference
                      from existing files does not apply in batch mode.
  --recursive         Descend into subdirectories (default: on).
  --no-recursive      Convert only top-level *.flac files. Last of the two
                      flags wins.
  --dry-run           Discover, map, and validate; print the plan; write
                      nothing (no files, no directories). Same exit-code
                      model as a real run.
  -v, --verbose       Per-file details (properties, frames, bytes).
  -q, --quiet         Only the final summary on stdout; errors still go to
                      stderr. Conflicts with --verbose: usage error (exit 2).
  -h, --help          Print usage and exit 0.
  -V, --version       Print version and exit 0.
  --                  End of options.
```

Flag placement is free (before, between, or after positionals, as today).
`--format=value` and `--format value` are both accepted, as today.

### 3.3 Examples

```sh
# Single file, unchanged behaviour: M4A inferred from the extension.
aac-convert album/track01.flac /tmp/out/track01.m4a

# Single file, explicit ADTS into a nonstandard extension.
aac-convert --format adts album/track01.flac /tmp/out/track01.bin

# Batch, whole tree (recursive is the default), M4A by default.
aac-convert album/ /tmp/out/

# Batch, explicit format + top level only.
aac-convert --format adts --no-recursive album/ /tmp/adts-out/

# Batch, preview what WOULD happen (writes nothing, creates no directories).
aac-convert --dry-run album/ /tmp/out/

# Batch, quiet CI run: exit code + summary line only.
aac-convert -q album/ /tmp/out/; echo "exit=$?"

# Filenames starting with a dash remain usable.
aac-convert -- --weird.flac ./out.m4a
```

### 3.4 Progress and summary reporting (recommended)

- **Streams.** Per-file outcomes go to **stdout**; diagnostics for failures go
  to **stderr** with the same `error: <rel>: <message>` shape as today's
  single-file `error: ...`. Rationale: stdout stays a stable, greppable
  record; stderr carries human detail without polluting it.
- **Normal batch line (one per converted file, in processing order):**
  `ok <relative-input> -> <relative-output> (<frames> frames, <bytes> bytes)`.
  Failure line on stdout: `fail <relative-input>: <category>`; the full
  message is on stderr. Ignored non-FLAC files print nothing in normal mode
  (they are tallied in the summary), and print `ignore <rel>` lines under
  `--verbose`.
- **Summary (always the last stdout line, including `--quiet` and
  `--dry-run`):**
  `summary: <converted> converted, <failed> failed, <existed> existing, <ignored> ignored (<elapsed>)`.
  `existing` counts destinations skipped by the no-overwrite rule; it is a
  subset of failure accounting for exit-code purposes (see §7.2).
- **Single-file output is unchanged**, including the `converted ...` line and
  the `M4A`/`ADTS` format token. Batch `ok` lines intentionally omit the
  format token (it is constant within a run and stated in the header line
  `batch: <n> files, format <M4A|ADTS>, <recursive|top-level>` printed first
  in normal/verbose mode).
- No JSON, no progress bars, no percentages in M8.2. A machine-readable
  format is deferred (see §11, Q8).

## 4. Input traversal rules

### 4.1 Mode dispatch

1. `stat` the input path. Missing input → fatal setup (§7, `MissingInput`).
2. Input is a regular file → single-file mode: call the existing
   `convert_file_with_format` path unchanged (including its
   `OutputDirectoryMissing`-if-parent-absent strictness).
3. Input is a directory → batch mode per this section.
4. Input is anything else (symlink handled below, FIFO, socket, device) →
   fatal setup `InputNotFileOrDir`.
5. **Symlinked input paths.** A symlink given directly as `<input>` is
   resolved as today: if it canonicalizes to a regular file, single-file
   mode proceeds; if to a directory, batch mode proceeds **on the canonical
   target** (so `strip_prefix` for relative paths is well-defined). A broken
   symlink or unresolvable path → fatal setup `MissingInput`. Rationale:
   preserves the tested single-file symlink behaviour
   (`rejects_symlink_to_input_as_collision`) and gives directory symlinks one
   consistent rule.

### 4.2 Eligibility and discovery (batch mode)

- **Extension match:** file-name extension equal to `.flac` under ASCII
  case-insensitive comparison (`.flac`, `.FLAC`, `.Flac` all eligible).
  Files with no extension, other extensions, or names like `track.flac.bak`
  are **ignored** (not failures): tallied as `ignored` in the summary.
- **File type:** only regular files are eligible. Directories, FIFOs,
  sockets, devices encountered during traversal are ignored (tallied), never
  followed or opened.
- **Directory symlinks: never followed.** Neither the input root (handled in
  §4.1 by canonicalizing once) nor any nested symlink-to-directory is
  descended. A nested symlink-to-directory is tallied as `ignored` (verbose
  logs `ignore <rel> (symlinked dir, not followed)`). Rationale: avoids
  cycles, escapes from the input tree, and double-processing the same subtree
  through two names; `std::fs` only, no cycle-detection dependency needed.
- **Symlinked files: accepted.** A symlink (or hardlink) whose final target
  is a regular file with a `.flac` name is eligible and converts like any
  other input; the mapping key is the link's relative path, not the target's.
  Two links to the same target convert independently (documented, tested).
  A broken symlink or a link whose target is not a regular file is a
  per-file failure (`InputOpen`), processing continues. Rationale: consistent
  with single-file canonicalization behaviour; links are user intent, while
  directory descent is where the danger lies.
- **Hidden files and directories: no special-casing.** Dotfiles are eligible
  and dot-directories are traversed. Rationale: music libraries legitimately
  contain dot-prefixed content; silently skipping it would be data loss by
  omission, and deterministic completeness is more predictable than
  dotfile heuristics.
- **Unreadable entries:** `read_dir` failure on a subdirectory, permission
  denial on a file, or any per-entry I/O error is a **per-file failure**
  (`UnreadableEntry`), processing continues with siblings. Only failure to
  read the **input root itself** is fatal setup.
- **Recursion:** `--recursive` (default on) descends the full tree;
  `--no-recursive` processes only the root's direct children. Last flag wins;
  specifying both is not an error. Depth-first vs breadth-first is
  unobservable because the candidate list is sorted before processing (§4.3).

### 4.3 Deterministic order

1. Collect the full candidate list (eligible `.flac` regular files/symlinked
   files) with paths relative to the canonical input root.
2. Sort by the relative path's byte representation (lexicographic over the
   `OsStr` bytes on Unix; on Windows, over the WTF-8/UTF-16-preserving
   lossless form — in practice: sort by `relative.as_os_str().as_encoded_bytes()`
   or the `to_string_lossy` fallback where the API is unavailable, documented
   in code). Ties are impossible (paths are unique); sort stability is
   therefore irrelevant.
3. Process and report strictly in that order. Retries, if any future
   milestone adds them, must not reorder.
4. Rationale: byte-order sorting is locale-independent, needs no `icu`/`unicode`
   dependency, and makes run-to-run and machine-to-machine output diffable.

### 4.4 Multiple explicit inputs

**Not supported in M8.2.** Exactly two positionals, as today. Passing three
or more positionals is a usage error (exit 2), even if all exist. Rationale:
keeps discovery, mapping, and exit codes to one root→root function; shell
globbing and `find -exec` cover multi-root needs; revisit with a concrete
proposal after batch MVP (see §11, Q6).

## 5. Output mapping and collision policy

### 5.1 Mapping function

For each eligible input with root-relative path `rel`:

1. Take `rel`'s parent (possibly empty) and file stem; **preserve the stem
   byte-for-byte, including case** (`Track_01.FLAC` → stem `Track_01`).
2. Replace the extension with the run's format extension, always lowercase:
   `m4a` mode → `.m4a`; `adts` mode → `.aac`. (Batch mode never emits `.mp4`
   or `.adts`; single-file inference is untouched.)
3. Destination = `<output-root>/<parent>/<stem>.<new-ext>`.
4. Create parent directories under the output root with `create_dir_all`
   lazily per file (tolerating `AlreadyExists` from a concurrent batch; any
   other creation failure is a per-file `OutputDirCreate` failure, processing
   continues — except failure to create the **output root itself**, which is
   fatal setup; see §7).

Extension-case examples (format `m4a`):

| Input relative path | Output relative path |
|---|---|
| `track01.flac` | `track01.m4a` |
| `Disc 1/Track 01.FLAC` | `Disc 1/Track 01.m4a` |
| `live.take-2.FlAc` | `live.take-2.m4a` |
| `archive.tar.flac` | `archive.tar.m4a` (only the final suffix is replaced) |
| `.hidden.flac` | `.hidden.m4a` |

### 5.2 Pre-flight validation (before any conversion or directory creation beyond the root)

1. Canonicalize the input root. Create-or-canonicalize the output root
   (`create_dir_all`, then canonicalize). Failure → fatal setup
   `OutputRootCreate`.
2. **Identity check:** if canonical output root == canonical input root →
   fatal setup `SameRoot`. (Same extensions differ, so this is about refusing
   a meaningless self-mirror, not about overwrite mechanics.)
3. **Nested-root rule (recommended: allow, with documentation).** An output
   root inside the input tree (or vice versa) is **permitted**: outputs are
   `.m4a`/`.aac` and the scanner only collects `.flac`, so re-scanning cannot
   loop, and per-file `SamePath`/`OutputExists` still protect. The design
   considered forbidding nesting but rejected it: users legitimately convert
   `album/` into `album/m4a/`, and a ban would need an `--allow-nested`
   escape hatch that enlarges the surface for no safety gain. The rule is
   revisited if a future scanner ever collects non-`.flac` files.
4. **Duplicate-destination detection (fail closed, before any work).** Build
   the full destination set first. If two distinct inputs map to the same
   destination — exact duplicates (only possible via symlink aliases with
   identical rel paths, i.e. never for distinct rel paths) **or
   case-folded duplicates** (`A.flac` + `a.flac` → `a.m4a` on case-insensitive
   filesystems) — the run aborts as fatal setup `DuplicateDestination` with
   both paths named, converting **nothing**. Case-folding uses ASCII
   lowercasing of the mapped relative path on all platforms (conservative:
   protects macOS/Windows default filesystems without platform sniffing).
   Rationale: converting either file first would poison the other's
   no-overwrite check; detecting up front avoids half-done batches.
5. **Per-file collision re-check (TOCTOU-safe).** Existence is still enforced
   at publication time by `hard_link` semantics, exactly as today: a
   destination created after the pre-flight (by the user or a concurrent
   batch) surfaces as per-file `OutputExists`, never as a replace.

### 5.3 Existing outputs and traversal safety

- **Default is no-overwrite, with no override flag in M8.2.** A
  pre-existing destination (file, directory, symlink, anything
  `symlink_metadata` finds) is a per-file `OutputExists` failure: counted in
  both `failed` and `existing`, processing continues, the existing path is
  never modified, replaced, truncated, or unlinked. There is no `--force`,
  `--skip`, `--newer`, or `--rebuild` in M8.2 (deferred to M8.3+; M7.1 §3.2's
  `skip/newer/rebuild` proposal is noted but not adopted here — see §11, Q7).
- **No path traversal.** Relative paths come from `strip_prefix` on the
  canonical root, so `..` components cannot occur; the implementation must
  still `debug_assert!` that every mapped destination starts with the
  canonical output root and fail the file closed (`TraversalRejected`) if the
  invariant ever breaks (defence in depth against future refactoring).
- **Source protection.** The per-file `SamePath` check (canonical input ==
  canonical destination) is retained inside `convert_file_with_format`; batch
  mapping additionally guarantees stem+extension replacement, so a source
  `.flac` can never be the publication target of its own conversion.
- **Partial outputs never remain.** Each file converts through its own
  `TempOutput` in the destination directory with `Drop`-cleanup, identical to
  today. A failed file leaves neither the destination (untouched if it
  pre-existed, absent otherwise) nor a `.aac-convert-*.tmp` orphan on the
  normal path. Orphans are possible only under `SIGKILL`/power loss and are
  documented as safe to delete (recognizable prefix, hidden, never a valid
  output name).

## 6. Batch processing and resource model

### 6.1 Sequential execution

- Files convert strictly one at a time, in the sorted order of §4.3, reusing
  the existing `convert_file_with_format` (same `CHUNK_FRAMES`, same temp +
  hard-link publication, same per-file memory ceiling). No threads, no
  async, no worker pool, no global audio buffer, no RAM disk.
- Peak memory for a batch run equals the peak for one single-file conversion
  plus the sorted path list (two `PathBuf`s per file — negligible against
  audio buffers). Long-file behaviour (no O(n²) feeder drain) is inherited
  from the chunked-push design; batching adds no new buffering.
- **No new runtime dependencies.** Traversal is `std::fs::read_dir` +
  explicit stack; sorting is `std::slice::sort`; no `walkdir`/`jwalk`,
  no `clap`/`lexopt` (the hand parser extends in place), no `tempfile`
  (existing `TempOutput` is reused), no signal-hook crates.

### 6.2 Per-file lifecycle

```text
for each (rel, dest) in sorted plan:
    ensure dest parent dirs exist (create_dir_all; failure → record OutputDirCreate, continue)
    convert_file_with_format(canonical_input_file, dest, run_format)
        → Ok(summary)    : print `ok …`, tally converted
        → Err(e)         : print `fail <rel>: <category>`, stderr detail,
                           tally failed (+existing if OutputExists),
                           assert no dest created and no tmp orphan
print summary; exit per §7.2
```

Directory creation happens lazily per file rather than all up front so that
`--dry-run` (which creates nothing) and real runs share the same planner,
and so that a `create_dir_all` failure is attributable to one file's subtree.

### 6.3 Dry-run

- Runs discovery, mapping, duplicate detection, and the per-file existence
  pre-checks; prints `plan <rel> -> <rel-out>` lines in processing order plus
  the same summary shape with a `dry-run:` prefix; creates **no files and no
  directories** (not even the output root); opens **no FLAC files for decode**
  (existence/type checks only). Exit codes mirror a real run (0/1/3, plus 2
  for bad flags), so CI can gate on `‑‑dry‑run` first.
- Rationale for not decoding during dry-run: decoding is the expensive,
  failure-prone half; a dry-run that decodes is a slow run that happens not
  to write. Eligibility (format support, corruption) remains a real-run
  discovery.

### 6.4 Cancellation and interruption

- **M8.2 scope: no graceful-interruption handling.** `Ctrl-C`/`SIGTERM`
  terminates the process between or mid-file per the OS default; the current
  file's `TempOutput::drop` cleans up on unwinding paths where destructors
  run, and any `SIGKILL`/power-loss orphan keeps the documented
  `.aac-convert-*.tmp` shape, safe to delete, never mistaken for output.
- Rationale: `std` alone cannot portably catch signals; adding `ctrlc`/
  `signal-hook` for a sequential loop buys little (at most one file's work is
  lost, nothing is half-published by construction). A future milestone may
  add "finish current file then stop with exit 3 + `interrupted` tally"
  without changing the per-file contract. Disk-full behaviour needs no new
  handling either: write/sync failures already map to `TempWrite`/`Finalize`
  and leave no destination.

### 6.5 Resource bounds (measurable)

- Per-file heap: one FLAC packet staging `Vec<i16>` (decoder block-size
  bounded) + ≤ `CHUNK_FRAMES × channels` `i16` per encoder push + muxer
  per-sample metadata (proportional to frame count, bytes not audio).
- Batch overhead: the sorted `(rel, dest)` plan only.
- Acceptance: converting a directory containing a 30-minute-equivalent FLAC
  must show flat resident memory across files (see §9, T-bounded-1).

## 7. Failure matrix and exit codes

### 7.1 Failure matrix

"Continues" = the batch proceeds to the next file. "Partial?" = whether any
user-visible partial output (destination or temp file) may remain on the
normal path (`SIGKILL`/power-loss orphans excepted). Categories reuse
`ConvertError` variant names where the per-file error is unchanged.

| # | Case | Continues? | Partial? | Reported category | Exit |
|---|---|---|---|---|---|
| F1 | Invalid CLI arguments (bad flag, bad `--format`, `-v`+`-q`, wrong positional count) | n/a (no scan) | No | `Usage` (stderr + usage) | 2 |
| F2 | Missing input path (incl. broken symlink) | n/a | No | `MissingInput` (fatal setup) | 3 |
| F3 | Input is not a file or directory (FIFO/socket/device) | n/a | No | `InputNotFileOrDir` (fatal setup) | 3 |
| F4 | Input root unreadable (permission/`read_dir` on root fails) | n/a | No | `InputUnreadable` (fatal setup) | 3 |
| F5 | Empty input directory (no entries at all) | n/a | No | `NoEligibleFiles: empty directory` (fatal setup) | 3 |
| F6 | No eligible FLAC files (entries exist, none match `.flac` regular-file rule) | n/a | No | `NoEligibleFiles` (fatal setup) | 3 |
| F7 | Unsupported FLAC format (24-bit / rate / channels, incl. mid-stream change) | Yes | No | `UnsupportedBitDepth/SampleRate/Channels` or `DecodeFailed` per file | 1 |
| F8 | Corrupt/truncated FLAC | Yes | No | `DecodeFailed` (or `InputNotFlac` if unrecognizable) | 1 |
| F9 | FLAC MD5 mismatch (present-but-wrong) | Yes | No | `DecodeFailed` | 1 |
| F10 | Valid FLAC with absent/zero MD5 | — (success path) | No | Accepted; no verified-checksum claim | 0 if all else ok |
| F11 | Missing/unreadable subdirectory or entry mid-tree | Yes | No | `UnreadableEntry` per file/dir; siblings continue | 1 |
| F12 | Destination already exists (any type) | Yes | No (pre-existing file untouched) | `OutputExists` per file; tallied in `failed` and `existing` | 1 |
| F13 | Duplicate mapped destination (incl. case-fold collision) | n/a (pre-flight abort, nothing converted) | No | `DuplicateDestination` naming both inputs (fatal setup) | 3 |
| F14 | Output root creation failure (incl. parent is a file, permission) | n/a | No | `OutputRootCreate` (fatal setup) | 3 |
| F15 | Per-file output subdir creation failure | Yes | No | `OutputDirCreate` per file | 1 |
| F16 | Temp-file creation/write/finalize failure (incl. disk full mid-write) | Yes | No (temp removed) | `TempCreate/TempWrite/Finalize` per file | 1 |
| F17 | No-replace publication failure (concurrent winner → `AlreadyExists`; FS without hard links → other I/O) | Yes | No (loser temp removed; winner intact) | `OutputExists` / `Publish` per file | 1 |
| F18 | Disk full (before/at publish) | Yes¹ | No | `TempWrite`/`Finalize`/`Publish` per file | 1 |
| F19 | Interruption (`SIGINT`/`SIGTERM`/`SIGKILL`) | n/a (deferred handling) | No² | Process terminates; no category printed by M8.2 | OS default |
| F20 | Mixed success + failure in one batch | Yes (to end) | No | Per-file categories as above + summary | 1 |
| F21 | Input file == mapped destination (`SamePath`) | Yes | No | `SamePath` per file (defence in depth; unreachable by construction in batch) | 1 |
| F22 | Non-FLAC/irregular files in tree | — (ignored, not errors) | No | Tallied `ignored`; verbose logs `ignore <rel>` | 0 if nothing else failed |

¹ If the disk is full at output-root creation, that is F14 (exit 3) rather
than F18. ² Except `SIGKILL`/power-loss temp orphans per §5.3, safe to delete.

### 7.2 Exit-code model

| Exit | Meaning | When |
|---|---|---|
| `0` | Total success | Batch completed; ≥1 file converted; zero failures (ignored files allowed). Single-file mode unchanged. `--dry-run` with a clean plan. `--help`/`--version`. |
| `1` | Partial batch failure | Scan completed; ≥1 file failed for any per-file reason (F7–F9, F11–F12, F15–F18, F20–F21). Summary shows nonzero `failed`. |
| `2` | Invalid invocation | CLI usage error (F1). Prints `error: ...` + usage. No filesystem mutation. |
| `3` | Fatal setup failure **(new in directory mode)** | Nothing was converted because the run could not start or had nothing valid to do: F2–F6, F13–F14. Prints `error: ...`; creates nothing except possibly the output root attempt in F14 (which failed). |

Notes:

- Single-file mode never returns 3 (unchanged `0/1/2` contract; its
  missing-parent strictness stays `OutputDirectoryMissing` → exit 1, not 3).
- `existing` destinations are failures for exit purposes (they contribute to
  `failed` and force exit 1); the separate `existing` tally exists so users
  can distinguish "re-run after fixing real errors" from "outputs already
  there". No silent skip-success: reporting an unconverted file as success
  would lie to CI.
- `--dry-run` uses the same codes against the plan (e.g. duplicates → 3,
  existing destinations → 1) while writing nothing.

## 8. Security and filesystem safety considerations

1. **No overwrite, no delete, no rename-over.** The only filesystem mutations
   are: `create_dir_all` (output tree), `create_new` temp files, `hard_link`
   publication, temp-file unlinking. The tool never unlinks, renames over, or
   truncates a user path. `OutputExists`/`SamePath`/`Publish` are fail-closed.
2. **TOCTOU.** Pre-flight existence checks are advisory; the atomic
   `hard_link` is the enforcement point (proven by the existing race tests).
   Batch code must not add a "re-check then rename" path.
3. **Symlink discipline.** Directory symlinks are never descended (no cycles,
   no escapes, no double-processing). File symlinks are converted as files;
   `resolve_paths` canonicalization keeps `SamePath` sound. Publication never
   follows a pre-existing destination symlink (it fails with `OutputExists`
   at `symlink_metadata` time, before any temp file is made).
4. **Traversal confinement.** Destinations derive from `strip_prefix` on the
   canonical root; an assertion plus `TraversalRejected` guards the join.
   No user-controlled `..` or absolute path ever reaches the output join.
5. **Hard-link filesystems.** FAT/exFAT/some network FS fail safe with
   `Publish` per file rather than degrading to rename. Documented; no
   fallback without a separate decision.
6. **Permissions.** The tool creates output dirs/files with the process umask
   defaults; it never chmods, chowns, or preserves source modes/ACLs in M8.2
   (documented limitation; ownership preservation is a future decision).
7. **No secrets, no network.** FLAC audio + file names only; nothing is
   transmitted. File names are echoed to stdout/stderr (documented: do not
   run on adversarially named trees in a context where logs are sensitive).
8. **Resource exhaustion.** Disk-full and temp-exhaustion map to per-file
   failures with cleanup; the sorted plan bounds memory independent of tree
   depth (explicit stack, no recursion — deep trees cannot overflow the call
   stack).
9. **Orphan hygiene.** Only `SIGKILL`/power-loss can orphan
   `.aac-convert-*.tmp` files; they are hidden, uniquely prefixed, and safe
   to delete. A future `--clean-orphans` is deferred (see §11, Q9).

## 9. Test plan and measurable acceptance criteria

Conventions: reuse `tests/common/mod.rs` (`TempDir`, `temp_output_files`,
`parse_adts`, `parse_mp4` helpers). All automated tests assert
`temp_output_files(...).is_empty()` after every run and assert pre-existing
sentinel bytes are untouched on collision paths.

### 9.1 Deterministic automated tests (must pass on every platform, no FFmpeg)

Traversal and mapping:

- **T-flat-1.** Flat dir with 2 supported `.flac` + 1 `.txt` + 1 extensionless
  file → 2 converted, 2 ignored, exit 0, outputs `<stem>.m4a` at root.
- **T-nested-1.** 3-level tree (`a/b/c.flac`, `a/sib.FLAC`, `top.flac`) with
  `--recursive` (default) → mirrored tree, 3 converted, processing/reporting
  order equals byte-sorted relative order (assert via captured stdout lines).
- **T-nonrec-1.** Same tree with `--no-recursive` → only `top.flac`
  converted; summary `1 converted … ignored` counts nested `.flac` as ignored
  (documented) — assert no nested outputs created.
- **T-flags-1.** `--recursive --no-recursive` (last wins → top-level only)
  and reverse order; `--format`/`--format=` equivalence in batch mode.
- **T-ext-1.** `UPPER.FLAC`, `mixed.FlAc`, `double.tar.flac`, `.hidden.flac`
  → `UPPER.m4a`, `mixed.m4a`, `double.tar.m4a`, `.hidden.m4a` (stems
  byte-preserved, extensions lowercase). `track.flac.bak`, `track.m4a`
  (pre-seeded decoy), extensionless `trackflac` → ignored.
- **T-empty-1.** Empty directory → exit 3, `NoEligibleFiles`, nothing created
  (dry-run likewise creates nothing).
- **T-noflac-1.** Directory with only `.txt`/`.mp3`/empty files → exit 3.
- **T-dryrun-1.** Nontrivial tree with `--dry-run` → `plan …` lines in
  sorted order, correct summary, exit 0; assert output root was **not**
  created and no FLAC was decoded (e.g. seed a corrupt `.flac`: dry-run still
  exits 0 with a clean plan; the real run exits 1).
- **T-dryrun-2.** `--dry-run` with a pre-existing destination → exit 1 with
  `existing` tallied, still writes nothing.
- **T-multi-1.** Three positionals → exit 2 regardless of path validity.

Collisions and safety:

- **T-dup-1.** Two inputs mapping to one destination via case folding
  (`A.flac` + `a.flac` in one dir, format m4a) → exit 3
  `DuplicateDestination` naming both, **zero** files converted, no outputs.
- **T-exist-1.** Pre-seeded `out/track.m4a` sentinel + convertible input →
  exit 1, sentinel bytes intact, `existing=1`, no tmp orphans.
- **T-race-1.** Barrier-started concurrent batch runs over the same
  input→output mapping (mirroring `concurrent_m4a_publication_never_replaces_the_winner`)
  → exactly one winner per file, losers `OutputExists`, winner bytes parse
  as MP4, no orphans.
- **T-cleanup-1.** Tree mixing one truncated `.flac`, one 24-bit `.flac`,
  one valid `.flac` → exit 1, valid file converted, two `fail` lines with
  correct categories, no destinations and no tmps for the failures.
- **T-nestedroot-1.** Output root inside input root (`in/conv/`) → run
  succeeds, outputs land nested, second identical run exits 1 with all
  `existing` (no `.flac` outputs rescanned, no loop).
- **T-sameroot-1.** Input root == output root → exit 3 `SameRoot`, nothing written.

Exit codes and reporting:

- **T-exit-1.** All-ok run → 0; one bad file → 1; bad flag → 2; missing
  input → 3. Each asserted via CLI subprocess status (as `tests/cli.rs` does).
- **T-single-compat-1.** Every existing single-file CLI/API test passes
  unmodified (regression gate; see §9.4).
- **T-summary-1.** Exact summary-line regex including `--quiet` (only summary
  on stdout) and `-v`/`-q` conflict → exit 2.

Audio fidelity (no new fixtures needed):

- **T-m4a-1.** Batch M4A outputs carry `M4A` brands, `iTunSMPB` with correct
  (D, P, S), no `edts`, `mdhd` = `N*1024`, `mdat` == ADTS access units
  (reuse `parse_mp4`/`mp4_itunsmpb` assertions per file).
- **T-adts-1.** `--format adts` batch outputs are byte-valid ADTS
  (`parse_adts` tiles exactly, AAC-LC profile, correct rate/channel indices).

Resource bounds:

- **T-bounded-1.** Batch over N copies of the stereo fixture converts with
  flat peak RSS across files (CI assertion: peak minus first-file peak <
  25% — generous, catches whole-file buffering regressions without flakiness)
  and total wall time linear in N (documents sequentiality; not a benchmark).

### 9.2 Platform-specific tests (gated, not universal)

- **T-symlink-1 (Unix + Windows separately).** Nested dir symlink →
  not descended (target's `.flac` not converted, tallied ignored);
  file symlink → converted at the link's rel path; broken symlink →
  per-file `InputOpen`, exit 1 overall. Unix asserts POSIX symlink semantics;
  Windows asserts the same modulo privilege-gated symlink creation (skip with
  a clear message when creation is denied).
- **T-perm-1 (Unix).** `chmod 000` subdirectory/file → `UnreadableEntry`,
  siblings convert, exit 1. (Windows ACL equivalent deferred to manual check.)
- **T-casefold-1 (macOS/Windows CI).** `A.flac`/`a.flac` duplicate detection
  fires even where the filesystem would have rejected the second source file
  — construct via sequential create/rename where needed; assert exit 3.

### 9.3 Manual checks (no automation, no ABX, no GUI playback)

- **M-ffmpeg-1.** Un-ignore `independent_decode` on a batch-produced tree
  spot-check (ffprobe codec/profile/rate/channels + full ffmpeg decode,
  zero stderr) after any muxer-adjacent change.
- **M-diskfull-1.** Batch against a size-capped ramdisk/tmpfs or quota dir →
  per-file `TempWrite`/`Finalize`, no destinations, no orphans, exit 1
  (F14 vs F18 boundary observed).
- **M-interrupt-1.** `Ctrl-C` mid-batch → at most one `.aac-convert-*.tmp`
  orphan, no half-published destination, re-run completes.
- **M-nonlinkfs-1.** Batch with output root on FAT/exFAT (USB stick) →
  per-file `Publish` failures, exit 1, nothing published (documents the
  no-fallback rule on real media).

### 9.4 Acceptance criteria for M8.2

1. All §9.1 tests pass; §9.2 tests pass on their platforms; single-file
   CLI/API suites pass unmodified.
2. `cargo test --locked -p aac-convert` green; no new dependencies in
   `Cargo.toml`/`Cargo.lock`; MSRV 1.99.1 intact.
3. Exit-code matrix of §7.2 demonstrated end-to-end via CLI subprocess tests.
4. No `.aac-convert-*.tmp` remains after any automated test; sentinel files
   intact after every collision test.
5. README documents the batch CLI, defaults, and exit codes; this design
   document's §11 recommendations are either adopted or explicitly re-decided
   with rationale in the M8.2 closeout.

## 10. Phased implementation plan for M8.2 onward

- **M8.2 — Sequential batch MVP (the only milestone this document specifies
  in full).** Discovery (`read_dir` stack, no dir-symlink descent),
  byte-sorted plan, mapping per §5.1, pre-flight (§5.2), sequential
  `convert_file_with_format` reuse, reporting per §3.4, exit codes per §7.2,
  full §9.1 suite + applicable §9.2 tests, README batch section. No new
  dependencies. Effort: small (one new module ~300 lines + tests); risk:
  low (per-file path already proven).
- **M8.3 — Existing-output policies (separate decision).** Adopts or rejects
  M7.1 §3.2's `skip/newer/rebuild` naming with exact timestamp/hash semantics,
  default-unchanged (no-overwrite) guarantee, and per-policy tests. Not
  started until batch MVP is validated.
- **M8.4 — Operability follow-ups (separately scoped, in order).**
  Machine-readable summary (`--format=json` for the summary line only, if a
  consumer need is demonstrated); interruption handling (finish-current-file
  then exit 3); orphan cleanup documentation/tooling; performance spot-checks
  on long-file trees. Parallelism is **not** on this roadmap without a new
  architecture decision overturning M7.1 D6.
- Explicitly **not** scheduled here: metadata/tag mapping (needs the reviewed
  Vorbis→M4A table, M7.1 U4), 24-bit policy (M7.1 U5), muxer/decoder changes,
  packaging/registry work, patent/legal sign-off.

## 11. Open questions, with recommended defaults

Recommendations are normative for M8.2 unless the implementer re-decides with
recorded rationale. Genuinely product-level questions are flagged [PRODUCT].

1. **Recursive default?** Recommended: **on** (`--no-recursive` opts out).
   Mirroring is the point of directory mode; top-level-only as the default
   would silently miss nested albums. Both flags accepted, last wins.
2. **ADTS batch extension `.aac` vs `.adts`?** Recommended: **`.aac`**.
   Matches single-file convention (`out.aac` throughout the test suite) and
   keeps one extension per format in batch mode.
3. **Batch M4A extension `.m4a` only (no `.mp4`)?** Recommended: **yes**.
   `.mp4` inference stays a single-file convenience; batch output is
   canonically `.m4a`.
4. **Output-root auto-creation vs single-file strictness?** Recommended:
   **auto-create the root and parents** (`create_dir_all`) in batch mode
   while single-file keeps `OutputDirectoryMissing`. A batch tool that cannot
   create its own tree is unusable; per-file attribution (§6.2) keeps errors
   diagnosable.
5. **Nested output roots?** Recommended: **allow** (see §5.2.3). No loop is
   possible with a `.flac`-only scanner; document and test rather than forbid.
6. **Multiple explicit inputs?** Recommended: **defer** (usage error in
   M8.2). [PRODUCT] Confirm scripting (`find -exec`, shell loops) suffices
   for the MVP audience.
7. **Existing-output policies (`skip/newer/rebuild`)?** Recommended:
   **defer to M8.3**; M8.2 reports `existing` as failure (exit 1). [PRODUCT]
   Confirm that "re-run exits 1 until clean" is acceptable CI behaviour for
   the MVP, versus a `--skip-existing` convenience flag.
8. **Progress streams and machine readability?** Recommended: per-file lines
   on stdout + details on stderr + stable summary line (see §3.4); **no JSON
   in M8.2**. [PRODUCT] Confirm no automation consumer needs JSON yet.
9. **Orphan cleanup tooling?** Recommended: **document-only** (safe-to-delete
   prefix). A `--clean-orphans` flag is deferred for lack of a demonstrated need.
10. **M7.1 record amendment?** **None made.** M7.1 D1 (M4A-first), D4–D7
    (no FFmpeg/RAM-disk/sequential/no-GUI), and §3.2's batch sketch all still
    hold; this document refines rather than overturns them (notably: exit 3
    is new but additive — single-file codes are unchanged — and the
    `skip/newer/rebuild` sketch is deferred, not rejected). If a future
    milestone adopts overwrite policies or parallelism, *that* milestone
    amends M7.1 with a link back here.
11. **Verbosity pair (`-v`/`-q`)?** Recommended: **include** as specified;
    the surface cost is two flags and the conflict rule, and CI quietness is
    a real need. Dropping them would push users toward parsing normal output.
12. **Hidden-file traversal?** Recommended: **traverse, no special-casing**
    (§4.2). Flag if any supported platform's file chooser norms require
    otherwise — none found in audit.

---

*End of M8.1 design. Implementation begins only under a separately approved
M8.2 milestone; any deviation from the recommendations above must record its
rationale in the M8.2 closeout.*
