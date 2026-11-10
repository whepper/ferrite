# aac-convert — FLAC to AAC (M4A or ADTS), single file or directory batch

`aac-convert` is a separate application crate around the existing `ferrite-aac` encoder.
It converts 16-bit FLAC files (44.1 kHz or 48 kHz, mono or stereo) to AAC-LC;
24-bit FLAC is accepted with a per-file precision warning (reduced to 16-bit
PCM before encoding):

- **M4A** — audio-only MP4 container, the intended user-facing output.
- **ADTS** — engineering/test output, retained for encoder validation.

The native Rust encoder runs in its existing VBR-5 ADTS mode. For M4A, each
frame's validated raw access-unit payload (`AdtsFrame::raw_payload`) and the
configuration's AudioSpecificConfig (`EncoderConfig::audio_specific_config`)
are handed to the muxer. The encoded audio is identical for both containers.

The application-facing contract (input precision, metadata, errors, atomic
publication, gapless signaling) is documented in
[`docs/aac-convert-integration-contract.md`](../docs/aac-convert-integration-contract.md).

## Supported input

- Native FLAC files: one file, or a directory tree of files (batch mode).
- 44.1 kHz or 48 kHz.
- Mono or stereo.
- 16-bit samples, or 24-bit samples (reduced to 16-bit PCM with a warning;
  see below).

24-bit FLAC input is accepted with an explicit per-file warning: the decoder
reduces each sample to its top 16 bits (truncation, no rounding or dithering)
before AAC encoding, so a maximum error of one 16-bit LSB per sample applies
and low-order source precision is discarded. Sample rates, channel counts,
and bit depths outside the supported sets are rejected; there is no
resampling or downmixing.

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

# M4A without user metadata
cargo run -p aac-convert -- --no-tags input.flac output.m4a
```

`--format` accepts `m4a` or `adts`. Without it, `.m4a` and `.mp4` select M4A;
every other extension, including `.aac`, `.adts`, unknown extensions, and no
extension, keeps the historical ADTS default. The success message names the
selected format.

`--no-tags` omits user metadata from M4A output (see Metadata); it is accepted
for ADTS output as a no-op.

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

# Batch M4A output without user metadata
cargo run -p aac-convert -- --no-tags album/ /tmp/out/
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
  `--verbose` adds per-file audio detail,
  per-file `tags <in>: <n> mapped, <m> unmapped` lines and
  `unmapped <in>: <key>` lines (see Metadata),
  and `ignore <path>` lines; `--quiet` prints only the summary on stdout while
  keeping failure diagnostics on stderr. Tag warnings are printed as
  `warning: <in>: <message>` on stderr in every mode.
- **Failure detail.** Single-file failures use the same
  `error: <input>: <message>` form as batch failures. Encoder failures include
  the failing frame index, the underlying failure category/reason, and an
  explicit statement that the temporary output was discarded and no output
  file was published.
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

## Metadata

M4A output carries the G0-approved core Vorbis-comment text tags read from
the source FLAC file:

| Vorbis key | M4A atom |
|---|---|
| `title` | `©nam` |
| `artist` | `©ART` |
| `album` | `©alb` |
| `tracknumber` | `trkn` (track number and total) |
| `discnumber` | `disk` (disc number and total) |
| `albumartist` | `aART` |
| `date`, then `year` | `©day` |
| `genre` | `©gen` |
| `composer` | `©wrt` |
| `comment` | `©cmt` |

Rules (G0 §2, authoritative):

- Keys match ASCII case-insensitively; values are trimmed; the stored value
  is the trimmed value. Empty or whitespace-only values are treated as absent.
- Single-valued atoms use the first non-empty value. Missing values produce
  a per-file `warning:` line on stderr; conversion still succeeds. When a
  recognized text key has more than one distinct non-empty value, the first
  is kept and one per-file warning names the reduced keys:
  `multi-valued tags reduced to first value: artist, composer`. Identical
  repeated values do not warn.
- The M4A core atoms are deliberately written once, as single-valued items.
  This avoids duplicate or conflicting atoms and keeps output deterministic
  across players that ignore or flatten repeated values. Preserving every
  value would require multiple `data` boxes under one item (mutagen and
  FFmpeg can read them, but player support is not uniform); that is a
  separate metadata model, not part of this contract.
- `tracknumber`/`discnumber` accept `N` or `N/M` (whitespace around `/` is
  tolerated); totals fall back to `tracktotal`/`totaltracks` and
  `disctotal`/`totaldiscs`, else `0` (unknown). Unparseable numbers skip the
  atom with a warning, never a failure.
- `date` wins over `year` for `©day`; the atom is written at most once and
  stored verbatim (no reformatting).
- All text atoms use M4A data type UTF-8. No integer or boolean text atoms
  are written (loudness freeform atoms are documented under Loudness below).
- Deferred or unknown keys (BPM, compilation, sort fields, grouping, lyrics,
  MusicBrainz IDs, ISRC, label, catalogue number, barcode, `encoder`, and
  anything else outside the table) are ignored for output, counted in the
  verbose-only `unmapped` diagnostic, and never an error.
- There is no stable, universally read core M4A atom for ISRC, barcode,
  publisher/label, catalogue number, or copyright in the supported tag
  subset. They are therefore not guessed into freeform atoms. A future
  explicit freeform mapping (`----:com.apple.iTunes:...`) would need its own
  compatibility tests and a documented conflict policy; until then, keep
  those identifiers in the source FLAC and read them from there.

`--no-tags` omits user metadata from M4A output (gapless `iTunSMPB` signalling
and `M4A ` brands stay); it is accepted for ADTS output as a no-op. ADTS
output never carries tags. Tags never alter the AAC audio: M4A payloads are
byte-identical with or without tags, and gapless signalling is unchanged.

## Cover art

M4A output embeds at most one JPEG or PNG image as a `covr` atom (appended
after the text atoms), selected deterministically from the source FLAC
pictures:

1. Front-cover pictures first.
2. Other typed pictures (e.g. back cover, including `Other`) second.
3. Untyped pictures (legacy `COVERART` comments, unknown picture types) last.
4. Encounter order within each tier.

Only JPEG (MP4 data type 13) and PNG (type 14) are embedded, detected by
magic bytes; the declared MIME type is never trusted and images are never
decoded. The per-image limit is 16 MiB. Invalid, unsupported, oversized, or
absent artwork is reported with a per-file `warning:` line and never fails
an otherwise valid conversion; oversized candidates fall through to the
next eligible picture. Cover outcomes use the same warning mechanism as
text tags, ordered after text-tag warnings (and after the 24-bit precision
notice, if present). `--no-tags` omits cover art and its warnings; ADTS
output never carries cover art and is byte-identical with or without
pictures.

## Loudness

M4A output carries native ReplayGain 2.0 and iTunes Sound Check loudness
metadata, analyzed from the source FLAC PCM with the `ebur128` R128
implementation (HISTOGRAM mode) at a fixed **−18.0 LUFS** reference:

- `REPLAYGAIN_TRACK_GAIN` / `REPLAYGAIN_TRACK_PEAK` (`"±X.XX dB"`, `"%.6f"`),
  `REPLAYGAIN_ALBUM_GAIN` / `REPLAYGAIN_ALBUM_PEAK`, and `iTunNORM`,
  all as UTF-8 freeform (`----`) atoms with `mean com.apple.iTunes`.
- The `iTunNORM` value derives from the clipping-adjusted track gain with
  round-half-even integer conversion, matching the original workflow.
- Albums are source-parent directories. Track and album gains use
  Easy-mode-equivalent positive-only clipping protection, applied
  independently and never below `+0.00 dB`; measured peaks are written
  verbatim and AAC samples are never modified.
- Silence (or otherwise unmeasurable input) omits loudness atoms with a
  warning; an album with any unanalyzable member omits album-level atoms
  with an album-level warning, never a subset result presented as complete.
- Loudness atoms are appended after text tags and cover art; loudness
  warnings follow tag and cover warnings (after the 24-bit precision
  notice, if present). `--no-tags`, ADTS, and dry-run skip analysis
  entirely; analysis failures warn and the audio conversion still succeeds.

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

- Core Vorbis-comment text tags and single JPEG/PNG cover art are supported
  for M4A (see Metadata). Core text fields are single-valued: extra distinct
  values are reduced to the first and reported in a per-file warning. ISRC,
  barcode, publisher/label, catalogue number, copyright, and MusicBrainz IDs
  are not mapped. Loudness tagging (ReplayGain 2.0 and
  `iTunNORM`) is implemented and shipped for M4A (see Loudness). Still out
  of scope: multi-value metadata, multi-image galleries, resampling,
  downmixing, VBR 1–4, and parallel conversion.
- No edit list on the default path and no universal gapless-playback claim.
  Default M4A carries `iTunSMPB` signalling, exact on the verified FFmpeg and
  CoreAudio decode paths; VLC and GUI players are unverified or approximate
  (see above).
- ADTS remains the engineering/test format.

## Distribution

There is no standalone `cargo package` for `aac-convert`: its path
dependency on the workspace-local, unpublished `ferrite-aac` crate cannot
resolve against the public registry (the package is unpublished, so no
version satisfies the required `^0.1.0`), and publishing `ferrite-aac` is
out of scope. The supported distribution
model is a **workspace source bundle built with the pinned toolchain**:

```sh
cd rust
cargo build --locked --release -p aac-convert   # binary: target/release/aac-convert
cargo test --locked -p aac-convert              # app suite incl. M4A/ADTS checks
```

Requirements: Rust 1.99.1 (`rustup install 1.99.1`), dependency sources
resolved from crates.io against the committed `rust/Cargo.lock`. No
vendoring is needed. Do not publish either crate to a registry.

Before distributing a binary, include the complete source bundle and
`rust/NOTICE` (including the prominent modified-version notice dated
2026-10-26), retain all dependency notices, and complete the review in
`THIRD-PARTY-NOTICES.md` (FDK-AAC conditions, MPL-2.0 source availability,
Apache/MIT/BSD attributions, and separate AAC patent authorization). Do not
charge a copyright license fee for the FDK-derived codec, and do not imply
Fraunhofer endorsement.

## Release follow-ups

- Standalone `cargo package`/`cargo publish` for `aac-convert` remains
  unsupported while `ferrite-aac` is an unpublished path dependency; use the
  workspace-bundle model above.
- The `THIRD-PARTY-NOTICES.md` inventory still needs human/legal sign-off
  before distributing binaries.
- FDK AAC patent/licensing considerations remain as documented by the codec
  license.
