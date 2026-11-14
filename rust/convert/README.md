# aac-convert — FLAC to AAC (M4A or ADTS), single file or directory batch

`aac-convert` is a separate application crate around the existing `ferrite-aac` encoder.
It converts 16-bit FLAC files (44.1 kHz or 48 kHz, mono or stereo) to AAC-LC;
24-bit FLAC is accepted with a per-file precision warning (reduced to 16-bit
PCM before encoding):

- **M4A** — audio-only MP4 container, the intended user-facing output.
- **ADTS** — engineering/test output, retained for encoder validation.

The native Rust encoder runs in VBR ADTS mode (default VBR-6; select
VBR-1 through VBR-6 with `--vbr`). For M4A, each
frame's validated raw access-unit payload (`AdtsFrame::raw_payload`) and the
configuration's AudioSpecificConfig (`EncoderConfig::audio_specific_config`)
are handed to the muxer. The encoded audio is identical for both containers.
See [`../docs/vbr-profiles.md`](../docs/vbr-profiles.md) for the authoritative
profile reference.

The application-facing contract (input precision, metadata, errors, atomic
publication, gapless signaling) is documented in
[`docs/aac-convert-integration-contract.md`](../docs/aac-convert-integration-contract.md).

For an end-user walkthrough covering installation, single-file conversion,
batch jobs, metadata, safe reruns, exit codes and troubleshooting, see the
[Ferrite User Manual](../docs/user-manual.md).

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
# M4A by extension (default VBR-6)
cargo run -p aac-convert -- input.flac output.m4a

# ADTS by extension
cargo run -p aac-convert -- input.flac output.aac

# Explicit override (wins over the extension)
cargo run -p aac-convert -- --format m4a input.flac output.aac
cargo run -p aac-convert -- --format adts input.flac output.m4a

# Explicit VBR profile (default 6)
cargo run -p aac-convert -- --vbr 5 input.flac output.m4a
cargo run -p aac-convert -- --vbr 1 input.flac output.aac

# M4A without user metadata
cargo run -p aac-convert -- --no-tags input.flac output.m4a

# Write cover.jpg/cover.png beside the output instead of embedding it
cargo run -p aac-convert -- --cover-art external input.flac output.m4a

# Embed the artwork and also write the external file
cargo run -p aac-convert -- --cover-art both input.flac output.m4a
```

`--format` accepts `m4a` or `adts`. Without it, `.m4a` and `.mp4` select M4A;
every other extension, including `.aac`, `.adts`, unknown extensions, and no
extension, keeps the historical ADTS default. The success message names the
selected format and VBR profile:

```text
converted input.flac -> output.m4a (14 AAC frames, 1961 bytes, M4A, VBR-6, nominal 192000 bit/s, 20000 Hz bandwidth)
```

`--vbr` accepts `1` through `6` (also `VBR-1`..`VBR-6`, `vbr1`..). Default is
`6`. VBR-1..VBR-5 are the upstream FDK AAC profiles; VBR-6 is the Ferrite
default (VBR-5 with 20 kHz bandwidth). Invalid values fail with exit 2.
See [`../docs/vbr-profiles.md`](../docs/vbr-profiles.md) for the full table.

`--no-tags` omits user metadata from M4A output (see Metadata); it is accepted
for ADTS output as a no-op.

`--cover-art` accepts `embedded` (the default), `external`, or `both`:
`embedded` writes the selected image into the M4A `covr` atom, `external` does
not embed it and writes `cover.jpg` or `cover.png` beside the audio, and `both`
does both. External files are never overwritten. Cover-art modes have no effect
on ADTS output or when `--no-tags` is active; see Cover art.

`--jobs N` limits directory batch preparation and encoding to `N` concurrent
files. It defaults to the number of available logical CPUs; `--jobs 1` is
strictly sequential. It is a batch-only option and is a usage error for a
single-file input.

`--help`, `--version`, and `--` are supported. `--help` prints the
application name and version in its header. `--version` prints
`aac-convert <version>` to stderr, and normal execution prints the same
application/version banner to stderr (`--quiet` and `--dry-run` suppress it).
Stdout stays reserved for script-friendly conversion results. Both identities
use the Cargo package version (`CARGO_PKG_VERSION`) as their only source.
Exit codes: `0` success, `1` conversion failure, `2` command-line usage error.
Directory batch mode adds exit code `3` for fatal setup errors (see below).

## Batch conversion (directory input)

A directory input converts every discovered FLAC file into an output root,
mirroring the input structure. Independent files are processed by a bounded
worker pool (`--jobs`, default: available logical CPUs):

```sh
# Whole tree, M4A by default
cargo run -p aac-convert -- album/ /tmp/out/

# Explicit ADTS output
cargo run -p aac-convert -- --format adts album/ /tmp/adts-out/

# Top level only (no recursion)
cargo run -p aac-convert -- --format adts --no-recursive album/ /tmp/adts-out/

# Preview the plan without writing anything
cargo run -p aac-convert -- --dry-run album/ /tmp/out/

# Use eight workers (default: available logical CPUs; 1 is sequential)
cargo run -p aac-convert -- --jobs 8 album/ /tmp/out/

# Rerun a completed or partial batch, skipping finished files
cargo run -p aac-convert -- --skip-existing album/ /tmp/out/

# Preview what a rerun would convert or skip
cargo run -p aac-convert -- --skip-existing --dry-run album/ /tmp/out/

# Batch M4A output without user metadata
cargo run -p aac-convert -- --no-tags album/ /tmp/out/

# Extract each album's cover art as cover.jpg/cover.png beside the audio
cargo run -p aac-convert -- --cover-art external album/ /tmp/out/

# Embed and extract the same selected artwork
cargo run -p aac-convert -- --cover-art both album/ /tmp/out/
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
  `--verbose` appends the planner's reason when it can establish one:
  `ignore <path> (unsupported extension)`, `(not a regular file)`,
  `(excluded by --no-recursive)`, or `(symlinked dir, not followed)`.
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
- **Parallel workers.** `--jobs N` is one global limit reused by two
  barrier-separated stages: preparation (album artwork discovery and album
  loudness) completes before encoding starts, and each stage uses at most `N`
  workers, so at most `N` files are ever in flight. Every worker owns its
  decoder, encoder, metadata state, and temporary file; no encoder state is
  shared. Loudness analysis parallelises across albums (members of one album
  remain sequential because they share ordered album state). Artwork discovery
  and embedded fallback reads are parallel across albums, but conflict
  resolution and external cover publication stay sequential after encoding.
  Dry runs start no workers.
- **Progress and summary.** Batch progress is live on stderr and reports both
  preparation and encoding. M4A runs with tags print
  `Preparing <n> files across <m> albums`, then counters for loudness analysis
  (`Loudness analysis <done>/<total> <pct>%`) and per-album artwork discovery
  (`Artwork discovery <done>/<total> <pct>%`), followed by reused/extracted
  album tallies (`Artwork reused` counts albums with an existing image file;
  `Artwork extracted` counts albums whose embedded pictures were selected
  for external publication, not per-track embedding) and prepared, failed
  and skipped file counts. Encoding then
  reports its own live counter. On a terminal the status line updates in place
  and includes the current file or album plus an ETA once enough work has
  completed. Redirected output or a non-interactive terminal receives the same
  information as plain newline-delimited milestone lines (ten-percent steps and
  completion), never carriage-return updates, so logs stay readable and
  machine-friendly. `--verbose` adds one `encoding`/`done`/`skipped`/`failed`
  line per file. Errors and warnings always print, including under `--quiet`.
  The batch header reports the effective worker count. The final
  machine-readable result lines remain on stdout in plan
  order, regardless of completion order
  (`ok <in> -> <out> (<frames> frames, <bytes> bytes)`,
  `skip <in> -> <out> (exists)` with `--skip-existing`, or
  `fail <in>: <category>`), and failures add `error: <in>: <message>` on
  stderr. A final summary line is always printed:
  `summary: <converted> converted, <failed> failed, <existing> existing, <ignored> ignored (<elapsed>s)`
  (`dry-run: …` for dry runs). With `--skip-existing` the summary gains a
  trailing skipped count (`…, <ignored> ignored, <skipped> skipped (…)`).
  `--verbose` adds per-file audio detail,
  per-file `tags <in>: <n> mapped, <m> unmapped` lines,
  `unmapped <in>: <key>` lines (see Metadata), and
  `ignore <path> (<reason>)` lines. `--quiet` suppresses live progress and
  per-file result lines, leaving the final summary on stdout; failures and
  warnings remain on stderr. Dry runs print the stdout plan only and emit no
  fake `encoding...` progress. The same preparation and encoding information is
  available to library callers as structured `ProgressEvent` values through
  `batch::execute_plan_with_options_and_events`; terminal rendering lives only
  in the CLI, so a future GUI can consume the event stream without duplicating
  conversion logic.
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
- **Bounded memory.** Each worker streams one file at a time through the same
  per-file pipeline as single-file conversion (8,000-frame encoder pushes,
  streaming muxer). Peak batch memory scales with `--jobs` (one in-flight
  file per worker), not with the size of the library. Artwork follows the
  same bound: no per-album image bytes are retained in RAM. Existing-file
  albums retain only paths and formats; embedded fallback covers are spilled
  to a temp staging directory inside the output root (one file per album, up
  to the 16 MiB cover limit each, on disk) and reloaded on demand — one album
  at a time for sequential publication, one image per in-flight worker for
  embedding. The staging directory (`.aac-convert-artwork-*.tmp.d`) is
  removed on completion; abrupt termination can leave an orphan, which is
  never a valid output, never blocks reruns, and is safe to delete manually
  once no conversion is running. Remaining batch overhead is the sorted path
  list plus small per-album metadata and per-member warning strings.
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

M4A output carries a standard `©too` (Tool) atom identifying the generating
encoder, plus the G0-approved core Vorbis-comment text tags read from the
source FLAC file.

| Encoder metadata | M4A atom | Value |
|---|---|---|
| Application identity | `©too` | `Ferrite <version>` |

`<version>` comes from Cargo package metadata (`CARGO_PKG_VERSION`); there is
no separately maintained version constant. This atom is technical output
metadata, not a copy of source tags: it is written for every M4A file,
including with `--no-tags`, and no source Vorbis comment is mapped to `©too`,
so a source `encoder` or `tool` tag can never overwrite or duplicate it. ADTS
has no metadata container and never carries it.

The source Vorbis-comment mapping is:

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

Rules (revised metadata policy):

- Keys match ASCII case-insensitively; values are trimmed; the stored value is
  the trimmed value. Empty or whitespace-only values are treated as absent.
- **Missing optional fields are silent.** None of the mapped text slots is
  required, so an untagged file, a missing `COMMENT`, or absent artwork
  produces no metadata warning. Warnings are reserved for malformed, invalid,
  unsupported, or genuinely lossy metadata.
- **Multiple values in one text atom.** Vorbis comments may repeat a key. For
  fields mapped to one M4A text atom (`©nam`, `©ART`, `©alb`, `aART`, `©day`,
  `©gen`, `©wrt`, `©cmt`), distinct non-empty values are preserved in source
  order and joined with `; ` (semicolon and one space). Exact duplicate values
  are removed while retaining their first occurrence; empty values are
  ignored. Example: `ARTIST=Artist A` plus `ARTIST=Artist B` becomes
  `Artist A; Artist B`. This is a pragmatic, readable single-atom mapping, not
  a lossless representation of the original Vorbis comment list.
- `date` still wins over `year` for `©day`; the two keys are never joined with
  each other. Multiple values of the selected key follow the same `; ` rule,
  and the atom is stored verbatim (no reformatting).
- **Numeric fields are not joined.** `tracknumber`/`discnumber` map to numeric
  tuples; multiple distinct values keep the first and emit a warning because
  the numeric destination cannot represent the rest. They accept `N` or `N/M`
  (whitespace around `/` is tolerated); totals fall back to
  `tracktotal`/`totaltracks` and `disctotal`/`totaldiscs`, else `0` (unknown).
  Unparseable numbers skip the atom with a warning, never a failure. A
  multi-valued numeric total likewise keeps the first parseable value and
  warns.
- Unsupported opaque values warn. If a recognized text key has both usable
  text and an opaque value, the text is kept and the opaque value warns.
- All text atoms use M4A data type UTF-8. No integer or boolean text atoms
  are written (loudness freeform atoms are documented under Loudness below).
- Deferred or unknown keys (BPM, compilation, sort fields, grouping, lyrics,
  MusicBrainz IDs, ISRC, label, catalogue number, barcode, `encoder`, and
  anything else outside the table) are ignored for output, counted in the
  verbose-only `unmapped` diagnostic, and never an error. The always-present
  `©too` encoder identification is generated independently and is never
  derived from these keys.
- There is no stable, universally read core M4A atom for ISRC, barcode,
  publisher/label, catalogue number, or copyright in the supported tag
  subset. They are therefore not guessed into freeform atoms. A future
  explicit freeform mapping (`----:com.apple.iTunes:...`) would need its own
  compatibility tests and a documented conflict policy; until then, keep
  those identifiers in the source FLAC and read them from there.

`--no-tags` omits user metadata from M4A output (gapless `iTunSMPB` signalling
and the `©too` encoder identification stay); it is accepted for ADTS output as
a no-op. ADTS output never carries tags or the encoder atom. Tags never alter
the AAC audio: M4A payloads are byte-identical with or without tags, and
gapless signalling is unchanged.

## Cover art

M4A output can embed at most one JPEG or PNG image as a `covr` atom (appended
after the text atoms). Artwork is selected deterministically from a
conventional album image file when one exists, otherwise from the source FLAC
pictures:

1. Front-cover pictures first.
2. Other typed pictures (e.g. back cover, including `Other`) second.
3. Untyped pictures (legacy `COVERART` comments, unknown picture types) last.
4. Encounter order within each tier.

Only JPEG (MP4 data type 13) and PNG (type 14) are accepted, detected by magic
bytes; the declared MIME type is never trusted and images are never decoded.
The per-image limit is 16 MiB. Empty, invalid, unsupported, or oversized
candidates are skipped with a per-file `warning:` line and never fail an
otherwise valid conversion; oversized or invalid candidates fall through to
the next eligible picture. Absent artwork is silent. Cover outcomes use the
same warning mechanism as text tags, ordered after text-tag warnings (and
after the 24-bit precision notice, if present).

### Existing album artwork

Before reading embedded pictures, the converter looks for a conventional image
file in each source album directory (the parent directory of the FLAC files;
for a single-file conversion, the input file's parent is its one-member album
directory):

1. `cover.jpg`, `folder.jpg`, `front.jpg`, `album.jpg`
2. the same four stems with `.jpeg`
3. the same four stems with `.png`
4. the same four stems with `.webp`

Matching is ASCII case-insensitive. The first readable, supported, in-limit
file in that order wins, regardless of directory enumeration order. Missing,
unreadable, empty, invalid, or oversized candidates are skipped gracefully and
the search continues; if no suitable file exists, embedded artwork is used as
before. Existing artwork is resolved once per album directory, never once per
track, and is reused for both embedding (`embedded`/`both`) and external
publication (`external`/`both`). JPEG and PNG files are embeddable and
publishable; WebP is supported for external reuse only and is written as
`cover.webp` (the M4A `covr` atom supports JPEG and PNG only). A WebP-only
album therefore falls back to embedded artwork for `embedded`/`both` while
`external` still reuses the WebP file.

When a suitable album file exists, it is authoritative: embedded pictures are
not extracted, and differing embedded images in the same directory no longer
veto publication. Without a suitable album file, embedded selection and
conflict detection retain the behavior described below. The selected image is
read and retained once per album during preparation; external publication does
not re-read the source. The original image bytes are never modified.

`--cover-art` selects the handling:

| Mode | Embed in M4A | Write external file |
|---|---:|---:|
| `embedded` (default) | Yes | No |
| `external` | No | Yes |
| `both` | Yes | Yes |

External files are written beside the audio output using the converter's
normal output-directory mapping (the output root plus the mirrored relative
directory), named `cover.jpg` for JPEG, `cover.png` for PNG, and `cover.webp`
for reused WebP album files. The original image bytes are written unchanged;
there is no recompression or transformation. Publication uses the same
temporary-file, flush/sync, atomic-hard-link path as audio output, so an
existing `cover.jpg`/`cover.png`/`cover.webp` is never replaced. A cover file is
written once per output directory, never once per track. Directories with no
eligible artwork get no file. With `--skip-existing`, retained existing audio
outputs still participate: an album whose tracks are all skipped can receive
its external cover if no cover file exists yet.

Before writing, the converter resolves artwork once per source album directory.
An existing album image wins; otherwise it groups planned tracks by output
directory and compares the actual selected image content (format, length, and a
content hash over the complete bytes). If every eligible embedded image in a
directory is identical, one file is published. If different images are found,
it prints a warning naming the output directory and skips external artwork
publication for that directory; audio conversion continues normally. All
planned tracks take part in this comparison, including tracks that are skipped
under `--skip-existing` and tracks that later fail conversion, so a failed track
with different artwork can veto external publication for its output directory.
No arbitrary first-seen image is published, and no partially written cover
remains. Dry runs read no artwork and create no files.

`--no-tags` omits embedding and external extraction entirely, and suppresses
cover warnings. ADTS output never carries cover art and never writes external
cover files, regardless of `--cover-art`. M4A audio payloads are byte-identical
with or without cover art, and gapless signalling is unchanged.

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
concurrent publication, metadata mapping and missing/multi-value policy,
embedded and external cover-art modes (including reusable album artwork,
case-insensitive name priority, WebP external reuse, conflicts and
no-overwrite), structured preparation/encoding progress events, live batch
progress output (interactive and redirected), verbose ignored-entry reasons,
and parallel directory batch conversion (worker limits, concurrent preparation
and encoding, progress accounting under concurrent completion, byte-identical
sequential/parallel output, traversal, mapping, preflight, dry-run, exit
codes, and cleanup). Ignored independent-decoder smoke tests for both
ADTS and M4A can be run when `ffmpeg` is available:

```sh
cargo test -p aac-convert --test independent_decode -- --ignored --nocapture
```

## Scope limits

- Core Vorbis-comment text tags and single JPEG/PNG cover art are supported
  for M4A (see Metadata). Text fields are single M4A atoms: multiple distinct
  source values are joined with `; ` in source order, while numeric
  track/disc fields keep the first value and warn about the reduction. ISRC,
  barcode, publisher/label, catalogue number, copyright, and MusicBrainz IDs
  are not mapped. Loudness tagging (ReplayGain 2.0 and
  `iTunNORM`) is implemented and shipped for M4A (see Loudness). Still out
  of scope: preserving the original multi-entry Vorbis structure, multi-image
  galleries, resampling, downmixing, and intra-file or frame-level
  parallelism (only independent files are processed concurrently).
- No edit list on the default path and no universal gapless-playback claim.
  Default M4A carries `iTunSMPB` signalling, exact on the verified FFmpeg and
  CoreAudio decode paths; VLC and GUI players are unverified or approximate
  (see above).
- ADTS remains the engineering/test format.

## Distribution

There is no standalone `cargo package` for `aac-convert`: its path
dependency on the workspace-local, unpublished `ferrite-aac` crate cannot
resolve against the public registry (the package is unpublished, so no
version satisfies the required `^0.3.1`), and publishing `ferrite-aac` is
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
