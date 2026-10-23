# aac-convert — FLAC to AAC (M4A or ADTS), single file or directory batch

`aac-convert` is a separate application crate around the existing `aac` encoder.
It converts 16-bit FLAC files (44.1 kHz or 48 kHz, mono or stereo) to AAC-LC:

- **M4A** — audio-only MP4 container, the intended user-facing output.
- **ADTS** — engineering/test output, retained for encoder validation.

The native Rust encoder runs in its existing VBR-5 ADTS mode. For M4A, each
frame's validated raw access-unit payload (`AdtsFrame::raw_payload`) and the
configuration's AudioSpecificConfig (`EncoderConfig::audio_specific_config`)
are handed to the muxer. The encoded audio is identical for both containers.

## Supported input

- Native FLAC files: one file, or a directory tree of files (batch mode).
- 44.1 kHz or 48 kHz.
- Mono or stereo.
- 16-bit samples.

24-bit FLAC is rejected with an explicit error. The encoder's public PCM
interface is `i16`; no conversion or dithering policy is approved yet, so
silently truncating 24-bit input would be incorrect. Other sample rates,
channel counts, and bit depths are also rejected; there is no resampling or
downmixing.

Integrity verification uses the FLAC StreamInfo MD5 when one is present. A
present-but-mismatched checksum fails the conversion without publishing output.
FLAC permits an absent or zero MD5; such files are accepted after successful
decoding, and the application does not claim that their audio checksum was
verified.

## Usage

```sh
# M4A by extension
cargo run -p aac-convert -- input.flac output.m4a

# ADTS by extension
cargo run -p aac-convert -- input.flac output.aac

# Explicit override (wins over the extension)
cargo run -p aac-convert -- --format m4a input.flac output.aac
cargo run -p aac-convert -- --format adts input.flac output.m4a
```

`--format` accepts `m4a` or `adts`. Without it, `.m4a` and `.mp4` select M4A;
every other extension, including `.aac`, `.adts`, unknown extensions, and no
extension, keeps the historical ADTS default. The success message names the
selected format.

`--help`, `--version`, and `--` are supported. Exit codes: `0` success,
`1` conversion failure, `2` command-line usage error. Directory batch mode
adds exit code `3` for fatal setup errors (see below).

## Batch conversion (directory input)

A directory input converts every discovered FLAC file sequentially into an
output root, mirroring the input structure:

```sh
# Whole tree, M4A by default
cargo run -p aac-convert -- album/ /tmp/out/

# Explicit ADTS output
cargo run -p aac-convert -- --format adts album/ /tmp/adts-out/

# Top level only (no recursion)
cargo run -p aac-convert -- --format adts --no-recursive album/ /tmp/adts-out/

# Preview the plan without writing anything
cargo run -p aac-convert -- --dry-run album/ /tmp/out/

# Rerun a completed or partial batch, skipping finished files
cargo run -p aac-convert -- --skip-existing album/ /tmp/out/

# Preview what a rerun would convert or skip
cargo run -p aac-convert -- --skip-existing --dry-run album/ /tmp/out/
```

- **Mode dispatch.** A regular file input converts one file (behavior above);
  a directory input activates batch mode. The input's file type selects the
  mode, never the output's.
- **Traversal.** Recursive by default. `--no-recursive` converts only
  top-level files (nested FLAC files are then tallied as ignored); if both
  flags are given, the last one wins. Directory symlinks are never followed.
  Symlinks to regular FLAC files are converted at the link's path. Hidden
  files and directories are traversed like any other entry.
- **Discovery.** File names with a `.flac` extension match case-insensitively
  (`.flac`, `.FLAC`, …). All other entries — other extensions, extensionless
  files, FIFOs, sockets, devices — are tallied as ignored, never failures.
  Discovery and reporting order is deterministic: byte-sorted relative paths.
- **Output mapping.** The input-relative directory structure is preserved,
  the source stem is kept byte-for-byte (including case), and the extension
  is replaced with lowercase `.m4a` (M4A, the batch default) or `.aac`
  (explicit `--format adts`). Batch mode never emits `.mp4` or `.adts`.
- **Preflight.** All destinations are mapped and checked before anything is
  converted. Two inputs mapping to the same destination — including ASCII
  case-fold collisions such as `A.flac`/`a.flac` — abort the run with nothing
  converted. Identical input and output directories are refused. An output
  root inside the input tree is permitted: outputs are `.m4a`/`.aac`, so they
  are never rediscovered as inputs. `--dry-run` performs this planning and
  the destination-collision checks while creating no files or directories and
  decoding no audio.
- **Existing outputs and safe reruns.** Existing destinations are never
  overwritten, truncated, or replaced. Without `--skip-existing` they are
  reported as failures (and tallied separately as `existing`), so rerunning a
  batch after fixing real errors exits non-zero until every file is converted.
  With `--skip-existing`, destinations that already exist are skipped instead:
  the source is never decoded, no temporary file is created, the destination
  is left byte-for-byte unchanged, and the file counts as `skipped` — neither
  a conversion nor a failure. Missing destinations convert normally. A batch
  in which every file is skipped (and nothing fails) exits `0`. The flag never
  enables overwriting: a destination appearing after planning is still refused
  by the atomic publication, and surfaces as a skip (flag on) or a failure
  (flag off). Duplicate destination collisions remain fatal preflight errors
  either way. `--skip-existing` applies to directory batch mode only; with a
  single-file input it is a usage error. There are no overwrite or force
  options in this release.
- **Progress and summary.** One line per file in processing order on stdout
  (`ok <in> -> <out> (<frames> frames, <bytes> bytes)`,
  `skip <in> -> <out> (exists)` with `--skip-existing`, or
  `fail <in>: <category>`), failure details as `error: <in>: <message>` on
  stderr, and always a final summary line:
  `summary: <converted> converted, <failed> failed, <existing> existing, <ignored> ignored (<elapsed>s)`
  (`dry-run: …` for dry runs). With `--skip-existing` the summary gains a
  trailing skipped count (`…, <ignored> ignored, <skipped> skipped (…)`).
  `--verbose` adds per-file audio detail and
  `ignore <path>` lines; `--quiet` prints only the summary on stdout while
  keeping failure diagnostics on stderr.
- **Exit codes.** `0`: at least one file converted (or a clean dry-run plan),
  zero failures — including a valid batch in which every file was skipped via
  `--skip-existing`. `1`: batch completed with one or more per-file failures
  (existing destinations count as failures unless `--skip-existing` skips
  them; skips never hide unrelated failures). `2`: CLI usage error. `3`:
  fatal setup — missing input, input that is neither file nor directory,
  unreadable input root, empty directory, no eligible FLAC files, duplicate
  destinations, or output-root creation failure.
- **Sequential, bounded memory.** Files convert one at a time through the
  same per-file pipeline as single-file conversion (8,000-frame encoder
  pushes, streaming muxer); batch overhead is the sorted path list only.
  There is no parallel conversion.
- **Temporary files.** Each file converts through a hidden
  `.aac-convert-<pid>-<seq>.tmp` file in its destination directory, removed
  on ordinary error paths and atomically hard-linked to the destination on
  success. No partial output remains after an ordinary (returned) error.
  Abrupt termination — fatal signals, forced process kills, machine shutdown,
  or power loss — runs no cleanup, since there is no signal handler: it can
  leave an orphan temporary file behind. Orphans keep the recognizable
  `.aac-convert-*.tmp` prefix, are never valid outputs (discovery only
  collects `.flac` inputs; outputs are `.m4a`/`.aac`), never block or corrupt
  a later run, and are safe to delete manually once no conversion is running.
  They are deliberately never deleted automatically. Destination filesystems
  must support hard links (FAT/exFAT fail safely per file; there is no rename
  fallback).

## M4A structure and timing

The muxer writes one audio track:

- `ftyp`/`moov`/`mdat` with a single `trak` (`soun`), `mp4a` sample entry and
  `esds` descriptor carrying the two-byte AAC-LC AudioSpecificConfig
  (`11 90` for 48 kHz stereo, `12 08` for 44.1 kHz mono).
- Media timescale equals the sample rate; every sample has a duration of 1024.
  Sample sizes, offsets, `stts`, `stsz`, `stsc`, and `stco` are written from the
  actual encoded frames.
- `mdhd` duration is `frame_count * 1024`. The duration fields are left at the
  encoded-stream timeline: adjusting them to the source duration breaks
  FFmpeg tail trimming, so they are intentionally not rewritten.
- `moov.udta.meta.ilst` carries Apple `iTunSMPB` gapless metadata with the
  encoder delay (1600), the tail padding, and the source-frame count, and
  `ftyp` uses the Apple-style `M4A` brands that CoreAudio requires before it
  honors that metadata.
- No edit list is written.

Decoders that honor `iTunSMPB` (verified: FFmpeg 9.0.2, macOS CoreAudio on
macOS 27.0.1) therefore present exactly the source audio: no leading encoder
delay, no trailing padding. Reported container duration is still
`frame_count * 1024 / sample_rate`, not the FLAC source duration; duration
displays follow the container fields, not the trimmed audio timeline.

Measured behavior on the committed fixtures (default output):

| Fixture | Frames | Container duration | FFmpeg decoded | CoreAudio decoded |
|---|---|---|---|---|
| stereo 48 kHz, 12,000 source frames | 14 | 0.298667 s (14,336 samples) | 12,000 samples | 12,000 samples |
| mono 44.1 kHz, 3,000 source frames | 5 | 0.116100 s (5,120 samples) | 3,000 samples | 3,000 samples |

Both containers decode successfully. They do **not** demonstrate universal
gapless playback: VLC 3.0.20 command-line transcoding skips one whole frame
and ignores `iTunSMPB` under every tested variant, and no GUI player playback
(Apple Music, QuickTime, Safari, VLC interface) has been verified. An edit
list was prototyped and remains available behind the experimental API below,
but it is not written by default: FFmpeg honors it except for very short
streams (which decode empty), and CoreAudio ignores it. Do not claim
universal gapless playback.

## Experimental edit list (opt-in research)

The library exposes `convert_file_m4a_experiment` with a `GaplessExperiment`
switch that adds a single `edts`/`elst` entry (`media_time = 1600`,
`edit_duration = S`, rate 1.0) on top of the default `iTunSMPB` signalling.
There is no CLI flag: the default CLI and `convert_file`/
`convert_file_with_format` outputs carry `iTunSMPB` only.

The edit list exists for player research, not production use: FFmpeg honors
it except for very short streams (N = 2 AAC frames or fewer decode to zero
samples), and CoreAudio ignores it under every tested brand set. Do not
enable it by default and do not present it as gapless playback.

## Dependencies

- [Symphonia](https://github.com/pdeljanov/Symphonia) 0.6.1 with only the
  `flac` feature: pure-Rust FLAC decode, MPL-2.0, no C or FFmpeg.
- [`shiguredo_mp4`](https://github.com/shiguredo/mp4-rs) 2026.5.0, pinned with
  `=`: Apache-2.0, zero dependencies, MSRV 1.93. Its `Mp4FileMuxer` is a real
  streaming muxer — sample bytes are written to the file as they are produced
  and only per-sample metadata is retained. It was selected in the M7.5 spike
  after comparing `mp4`, `muxide`, `mp4e`, and parser-only crates; `muxide`
  buffers every sample in memory, and `mp4` is stale and exposes no edit-list
  writer.

FFmpeg is not a build or runtime dependency. It is used only for out-of-tree
verification.

## Output safety

Existing output files are never replaced. Conversion writes to a hidden
temporary file in the destination directory, flushes and syncs it, then
publishes it by hard-linking the temporary name to the destination. The link
atomically fails if the destination was created meanwhile, including between
two concurrent conversions. This requires a destination filesystem with
hard-link support; FAT/exFAT and some network filesystems fail safely with a
`Publish` error, and there is no check-then-rename fallback. On failure the
temporary file is removed and an existing destination is left untouched.

## Memory

Audio is decoded one FLAC packet at a time. Encoder pushes are capped at 8,000
sample frames per channel; the intermediate interleaved packet buffer is
separate and bounded by the decoder's FLAC block size, which may be larger. The
M4A muxer stores only per-sample metadata, so neither the complete input nor
the complete AAC output is buffered.

## Tests

```sh
cargo test -p aac-convert
```

The app tests use committed synthetic FLAC fixtures; no external encoder is
needed. They verify ADTS output, M4A box structure and sample tables, payload
equivalence between the two containers, integrity failures, output collisions,
concurrent publication, and sequential directory batch conversion (traversal,
mapping, preflight, dry-run, exit codes, and cleanup). Ignored independent-decoder smoke tests for both
ADTS and M4A can be run when `ffmpeg` is available:

```sh
cargo test -p aac-convert --test independent_decode -- --ignored --nocapture
```

## Scope limits

- No metadata, cover art, ReplayGain, resampling, downmixing, VBR 1–4, or
  parallel conversion.
- No edit list on the default path and no universal gapless-playback claim.
  Default M4A carries `iTunSMPB` signalling, exact on the verified FFmpeg and
  CoreAudio decode paths; VLC and GUI players are unverified or approximate
  (see above).
- ADTS remains the engineering/test format.

## Distribution

There is no standalone `cargo package` for `aac-convert`: its path
dependency on the workspace-local, unpublished `aac` crate cannot resolve
against the public registry (crates.io offers only 0.0.0, which does not
satisfy the required `^0.1.0`), and publishing `aac` is out of scope. The supported distribution
model is a **workspace source bundle built with the pinned toolchain**:

```sh
cd rust
cargo build --locked --release -p aac-convert   # binary: target/release/aac-convert
cargo test --locked -p aac-convert              # app suite incl. M4A/ADTS checks
```

Requirements: Rust 1.99.1 (`rustup install 1.99.1`), dependency sources
resolved from crates.io against the committed `rust/Cargo.lock`. No
vendoring is needed. Do not publish either crate to a registry.

Before distributing a binary, complete the notice/license review in
`THIRD-PARTY-NOTICES.md` (FDK-AAC conditions, MPL-2.0 source availability,
Apache/MIT/BSD attributions, AAC patent authorization).

## Release follow-ups

- Standalone `cargo package`/`cargo publish` for `aac-convert` remains
  unsupported while `aac` is an unpublished path dependency; use the
  workspace-bundle model above.
- The `THIRD-PARTY-NOTICES.md` inventory still needs human/legal sign-off
  before distributing binaries.
- FDK AAC patent/licensing considerations remain as documented by the codec
  license.
