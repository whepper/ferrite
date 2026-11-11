# Ferrite User Manual

**Version:** 0.1.0  
**Release:** 11 November 2026  
**Audience:** Users converting FLAC music files with `aac-convert`

Ferrite is a native Rust AAC-LC encoder. Its companion command-line application, **`aac-convert`**, converts FLAC files into AAC audio in an M4A container or an ADTS stream.

This manual explains how to build and use the standalone converter. It focuses on everyday music conversion; it is not a developer guide to the encoder internals.

> **At a glance:** use M4A for normal music-library use. Use ADTS when you specifically need a raw AAC transport stream. The encoder uses a fixed AAC-LC VBR-5 profile; this release does not provide a quality or bitrate selector.

## 1. What you need

Ferrite 0.1.0 is distributed as source. Build it locally with:

- Rust and Cargo **1.99.1**.
- The Ferrite 0.1.0 source tree, including its workspace files and committed `Cargo.lock`.
- Network access to resolve crates.io dependencies on the first build (unless they are already cached).

FFmpeg is **not** required to build or run `aac-convert`. It is useful for independently checking converted files, but is not a runtime dependency.

### Get the source

Download or clone the repository, then select the release tag:

```sh
git clone https://github.com/whepper/ferrite.git
cd ferrite
git checkout v0.1.0
```

Install the required Rust toolchain with rustup if necessary:

```sh
rustup install 1.99.1
```

### Build the converter

From the repository's `rust/` directory:

```sh
cd rust
cargo build --locked --release -p aac-convert
```

The executable is `target/release/aac-convert`.

Check it:

```sh
./target/release/aac-convert --version
./target/release/aac-convert --help
```

The manual uses `aac-convert` below to refer to that executable. Either run it by its full path or place it somewhere on your `PATH`.

## 2. What files can be converted?

The input must be a native FLAC file or a directory containing FLAC files.

| Property | Supported input |
|---|---|
| Container | FLAC |
| Sample rate | 44,100 Hz or 48,000 Hz |
| Channels | Mono or stereo |
| Bit depth | 16-bit, or 24-bit with truncation to 16-bit |

Other sample rates, channel counts and bit depths are rejected. Ferrite does not resample, downmix or change channel layouts.

### Important: 24-bit FLAC

24-bit FLAC is accepted, but it is **not** passed to the encoder at its original precision. Each decoded sample is truncated to its top 16 bits using an arithmetic right shift. There is no rounding, dithering or noise shaping. A warning is printed for each converted 24-bit file, and the low-order source precision is discarded.

16-bit FLAC is the straightforward path when you want to avoid this conversion.

### FLAC integrity

When a FLAC file contains a nonzero StreamInfo MD5 checksum, the converter verifies it. A checksum mismatch fails conversion and no output is published. FLAC files with an absent or zero MD5 are accepted after decoding, but the converter cannot claim that their audio checksum was verified.

## 3. Convert one file

The basic command takes exactly two paths:

```sh
aac-convert <input.flac> <output>
```

For example:

```sh
aac-convert "track.flac" "track.m4a"
```

The output format is inferred from the output filename unless you specify `--format`:

- `.m4a` or `.mp4` selects M4A.
- Every other extension selects ADTS, including `.aac`, `.adts`, unknown extensions, or no extension.

M4A is the intended user-facing format. For example:

```sh
aac-convert "track.flac" "track.m4a"
```

To explicitly select a format:

```sh
aac-convert --format m4a "track.flac" "track.anything"
aac-convert --format adts "track.flac" "track.aac"
```

The explicit `--format` option wins over the filename extension. Accepted values are `m4a` and `adts`.

A successful single-file conversion prints a line similar to:

```text
converted track.flac -> track.m4a (N AAC frames, N bytes, M4A)
```

Warnings, such as the 24-bit precision notice or metadata warnings, are printed to standard error.

### Single-file options

| Option | Meaning |
|---|---|
| `--format m4a` | Write audio-only M4A. |
| `--format adts` | Write an ADTS AAC stream. |
| `--no-tags` | Omit user metadata from M4A. Accepted for ADTS as a no-op. |
| `-h`, `--help` | Display command help. |
| `-V`, `--version` | Display the application version. |
| `--` | End option parsing; subsequent arguments are paths. |

Existing output files are **never overwritten**. There is no force or overwrite option in 0.1.0. Choose a new output path when the destination already exists.

Single-file mode does not accept batch-only options such as `--recursive`, `--dry-run`, `--skip-existing`, `--verbose` or `--quiet`.

## 4. Convert an album or directory tree

Give a directory as the input and a destination directory as the output root:

```sh
aac-convert "album/" "converted/"
```

The converter discovers FLAC files and mirrors their relative directory structure under the output root. It preserves each source filename (including its case) and changes the extension:

- M4A output uses `.m4a`.
- ADTS output uses `.aac`.

For example:

```text
music/
├── Artist A/
│   ├── Album One/
│   │   ├── 01 - Intro.flac
│   │   └── 02 - Song.flac
│   └── Album Two/
│       └── 01 - Another Song.flac
└── Artist B/
    └── Live/
        └── 01 - Track.flac
```

Running:

```sh
aac-convert "music/" "converted/"
```

produces:

```text
converted/
├── Artist A/
│   ├── Album One/
│   │   ├── 01 - Intro.m4a
│   │   └── 02 - Song.m4a
│   └── Album Two/
│       └── 01 - Another Song.m4a
└── Artist B/
    └── Live/
        └── 01 - Track.m4a
```

Batch mode defaults to M4A, regardless of the output-root directory name. To select ADTS:

```sh
aac-convert --format adts "music/" "converted-adts/"
```

### Batch options

| Option | Meaning |
|---|---|
| `--format m4a` | M4A output (the default). |
| `--format adts` | ADTS output, using `.aac` filenames. |
| `--recursive` | Search subdirectories (the default). |
| `--no-recursive` | Convert only files directly inside the input directory. |
| `--dry-run` | Preview the planned conversions without writing files or decoding audio. |
| `--skip-existing` | Skip destinations that already exist, enabling safe reruns. |
| `--no-tags` | Omit user metadata, cover art and loudness tags from M4A. |
| `-v`, `--verbose` | Show extra per-file audio and metadata details. |
| `-q`, `--quiet` | Print only the final summary on standard output. Failure diagnostics remain on standard error. |
| `-h`, `--help` | Display command help. |
| `-V`, `--version` | Display the application version. |

If both `--recursive` and `--no-recursive` are present, the last one wins. `--verbose` and `--quiet` conflict and cannot be used together.

Examples:

```sh
# Convert the entire tree recursively (M4A).
aac-convert "music/" "m4a/"

# Convert only the top-level files.
aac-convert --no-recursive "music/" "m4a/"

# Preview the plan first.
aac-convert --dry-run "music/" "m4a/"

# Convert to ADTS instead.
aac-convert --format adts "music/" "aac/"

# Show sample rate, channels, bit depth, and tag diagnostics.
aac-convert --verbose "music/" "m4a/"

# Keep the final summary but suppress normal per-file progress.
aac-convert --quiet "music/" "m4a/"
```

### What gets discovered?

- Files with a `.flac` extension are discovered case-insensitively, including `.FLAC`.
- Other entries are ignored and counted: for example, files with other extensions and extensionless files.
- Hidden files and directories are included.
- Directory symlinks are not followed. Symlinks to regular FLAC files are converted at their link path.
- Processing order is deterministic: paths are byte-sorted by relative path.

Conversion is sequential; this release does not process multiple files in parallel.

## 5. Safe previews and reruns

### Preview a batch

Use `--dry-run` to inspect the plan before conversion:

```sh
aac-convert --dry-run "music/" "converted/"
```

A dry run performs discovery, output mapping and destination-collision checks, but it does not create files or directories and does not decode audio. Its final line begins with `dry-run:`.

### Resume a partial batch

By default, if a destination already exists, that input is reported as a failure. The converter never replaces it.

Use `--skip-existing` to skip existing destinations:

```sh
aac-convert --skip-existing "music/" "converted/"
```

This is intended for rerunning a completed or partially completed batch. Existing outputs are left byte-for-byte unchanged and are not decoded or encoded again. Missing outputs are converted normally.

You can combine the options:

```sh
aac-convert --skip-existing --dry-run "music/" "converted/"
```

Important details:

- `--skip-existing` applies only to directory mode.
- It never enables overwriting.
- A destination that appears after planning is still protected by the atomic publication step.
- Skipping existing files does not hide failures on other inputs.
- A batch where every file is skipped and nothing fails exits successfully.

### Preflight protections

Before converting anything, batch mode maps all destinations and checks for collisions. If two input paths would map to the same output (including ASCII case-fold collisions such as `A.flac` and `a.flac`), the run is aborted before conversion. Identical input and output directories are refused.

An output root inside the input tree is allowed because generated `.m4a` and `.aac` files are not rediscovered as FLAC inputs.

## 6. Metadata, cover art and loudness

M4A output can preserve selected metadata from FLAC Vorbis comments. ADTS output carries no tags.

### Text tags

The following source tags are mapped:

| FLAC Vorbis comment | M4A field |
|---|---|
| `TITLE` | Title |
| `ARTIST` | Artist |
| `ALBUM` | Album |
| `TRACKNUMBER` | Track number and total |
| `DISCNUMBER` | Disc number and total |
| `ALBUMARTIST` | Album artist |
| `DATE`, then `YEAR` | Date |
| `GENRE` | Genre |
| `COMPOSER` | Composer |
| `COMMENT` | Comment |

Keys are matched case-insensitively and values are trimmed. Core M4A atoms are single-valued: the first non-empty value is kept. When a recognized text key has multiple distinct non-empty values, the converter emits one warning listing the affected keys. Identical repeated values do not cause a warning.

Unmapped tags (for example, ISRC, barcode, publisher/label, catalogue number, copyright, MusicBrainz IDs, BPM, lyrics and sort fields) are not written. With `--verbose`, the converter reports the number of mapped and unmapped tags and lists unmapped keys.

Missing or invalid tag values produce warnings rather than failing an otherwise valid conversion.

### Cover art

M4A output embeds at most one JPEG or PNG image:

- Front cover is preferred.
- Other typed pictures are considered next.
- Untyped pictures are considered last.
- Original order is retained within each group.
- Maximum image size is 16 MiB.

Image type is identified from its file signature, not just its declared MIME type. Unsupported, invalid, oversized or missing images produce warnings and do not fail audio conversion. If one candidate is not eligible, the next candidate can be considered.

`--no-tags` also omits cover art.

### ReplayGain and iTunes Sound Check

For M4A with tags enabled, the converter analyzes the decoded source audio and writes ReplayGain 2.0 track/album information and iTunes `iTunNORM` metadata. The reference level is fixed at −18.0 LUFS. It does not change the AAC samples to apply gain.

For batch conversion, an album is defined by the source parent directory. Album loudness is written only when the album's members can all be analyzed. If a member is silent or otherwise unanalyzable, album-level loudness atoms are omitted with a warning rather than publishing a partial album result. Mixed sample-rate/channel layouts receive track-level loudness only; album-level atoms are omitted with a warning.

Silence or unmeasurable audio omits loudness atoms with a warning. `--no-tags`, ADTS output and dry-run do not perform loudness analysis.

### What `--no-tags` means

For M4A, `--no-tags` omits user text tags, cover art and loudness atoms. It does **not** remove the M4A gapless signalling described below. The AAC audio payload is byte-identical with and without user tags.

## 7. Gapless playback and timing

M4A output includes Apple `iTunSMPB` gapless metadata. It records:

- 1600 samples of encoder priming;
- the original source sample count;
- the tail padding needed to account for the encoded frames.

The M4A uses Apple-style `M4A ` brands and does not write an edit list by default. Decoders that honor `iTunSMPB` can present exactly the original number of source samples. This was verified on the release fixtures with FFmpeg 9.0.2 and macOS CoreAudio (`afinfo`).

This is **not** a universal gapless-playback guarantee. Other players may ignore or interpret the metadata differently. Also, the container duration describes the encoded AAC timeline and can be longer than the trimmed source audio.

ADTS has no gapless metadata. Applications using ADTS must account for the 1600-sample encoder delay themselves.

## 8. Output safety and filesystem requirements

The converter never overwrites an existing destination.

For each output, it writes to a hidden temporary file in the destination directory. Only after decoding, encoding, container finalization and validation succeed does it publish the completed file using an atomic hard link. If another process creates the destination first, publication fails rather than replacing that file.

On ordinary conversion errors, temporary output is removed and no partial destination is published.

The destination filesystem must support hard links. FAT/exFAT and some network filesystems may not. On filesystems without the required support, publication fails safely; there is no rename-based fallback.

An abrupt process termination, machine shutdown or power loss can leave a hidden temporary file whose name begins with `.aac-convert-` and ends with `.tmp`. Such files are not valid outputs and are not removed automatically. Once no conversion is running, they can be deleted manually.

## 9. Progress, warnings and exit codes

### Batch progress

Normal batch mode prints a header, per-file results and a final summary. The per-file forms include:

```text
ok <input> -> <output> (<frames> frames, <bytes> bytes)
skip <input> -> <output> (exists)
fail <input>: <category>
```

Failures have a corresponding diagnostic on standard error:

```text
error: <input>: <message>
```

The final summary has this form:

```text
summary: <converted> converted, <failed> failed, <existing> existing, <ignored> ignored (<seconds>s)
```

With `--skip-existing`, the summary also includes a skipped count. A dry-run summary begins with `dry-run:`.

### Warnings and verbose output

Warnings are printed to standard error in every mode, including quiet mode. Examples include:

- 24-bit input was truncated to 16-bit PCM.
- Multiple distinct values were reduced to the first value.
- A tag value was missing or invalid.
- Cover art was unsupported or could not be embedded.
- Loudness could not be calculated.

`--verbose` adds sample rate, channel count and source bit depth to successful per-file lines, plus tag counts, unmapped keys and ignored paths.

`--quiet` suppresses normal progress on standard output and leaves the final summary. Error and warning diagnostics still appear on standard error.

### Exit codes

| Code | Meaning |
|---:|---|
| `0` | Success. Includes a clean dry-run and a batch where all files were skipped with no failures. |
| `1` | Conversion failure, or a batch completed with one or more per-file failures. Existing destinations count as failures unless skipped with `--skip-existing`. |
| `2` | Command-line usage error, such as missing/extra paths, an invalid format, conflicting verbosity flags, or a batch-only option used for a single file. |
| `3` | Fatal batch setup error, such as a missing input, invalid input root, empty directory, no eligible FLAC files, destination collision, or output-root setup failure. |

A fatal setup error occurs before file conversion begins. A per-file failure in a batch does not prevent other planned files from being attempted.

## 10. Troubleshooting

### “Unsupported sample rate” or “unsupported channel count”

The converter accepts only 44.1 or 48 kHz, mono or stereo FLAC. It does not resample or downmix. Convert the source to a supported format with a separate audio tool before running Ferrite.

### “24-bit FLAC input is truncated…”

This is an informational warning, not a conversion failure. Ferrite 0.1.0 encodes 16-bit PCM. The converter truncates 24-bit samples to their top 16 bits without dithering.

### “Output already exists” / publication failure

The converter intentionally refuses to replace an output. Choose another destination, or use batch-mode `--skip-existing` to skip existing files. There is no overwrite mode.

### A batch exits with code 1

One or more files failed, even if other files converted successfully. Read the `error:` lines on standard error and the final summary. Fix the affected source or destination issue, then rerun with `--skip-existing` so already-converted files are not processed again.

### A batch exits with code 3

This is a setup failure rather than an individual track failure. Check the input directory, output root, permissions, destination collisions and whether any eligible FLAC files were discovered.

### Output cannot be published on a filesystem

Publication requires hard-link support. FAT/exFAT and some network filesystems fail safely. Use a destination filesystem that supports hard links.

### The output duration looks longer than the source

M4A duration fields describe the encoded AAC stream. Gapless-aware decoders use `iTunSMPB` to trim the priming and padding. Not every player honors this metadata.

### The output is not gapless in a particular player

Ferrite does not promise universal player compatibility. FFmpeg and macOS CoreAudio were verified against the release fixtures. A player that ignores `iTunSMPB` may expose encoder delay or padding.

### An orphan temporary file remains

If the process was forcibly terminated or the machine shut down during conversion, cleanup may not have run. Wait until no conversion is active, then remove files matching `.aac-convert-*.tmp` from the relevant output directories.

## 11. Scope and limitations

Ferrite 0.1.0 is deliberately focused:

- AAC-LC VBR-5 only; no quality ladder or user bitrate setting.
- FLAC input only; M4A or ADTS output only.
- 44.1/48 kHz, mono/stereo.
- 16-bit encoder input; 24-bit FLAC is truncated with no dithering.
- No resampling or downmixing.
- Single-valued core metadata, one JPEG/PNG cover image; no identifier mapping.
- Sequential batch conversion; no parallel processing.
- Hard-link-capable output filesystem required.
- No universal gapless, perceptual transparency or bit-exact FDK equivalence claim.

## 12. More documentation

- [Project overview](../../README.md)
- [Rust workspace README](../README.md)
- [aac-convert detailed behavior and metadata reference](../convert/README.md)
- [Ferrite encoder API integration guide](encoder-integration.md)
- [aac-convert integration contract](aac-convert-integration-contract.md)
- [Ferrite 0.1.0 release notes](releases/0.1.0.md)

For encoder-library consumers, use the encoder integration guide. This user manual covers the standalone `aac-convert` application.
