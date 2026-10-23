# aac-convert — single-file FLAC to AAC (M4A or ADTS)

`aac-convert` is a separate application crate around the existing `aac` encoder.
It converts one FLAC file to one AAC-LC file:

- **M4A** — audio-only MP4 container, the intended user-facing output.
- **ADTS** — engineering/test output, retained for encoder validation.

The native Rust encoder runs in its existing VBR-5 ADTS mode. For M4A, each
frame's validated raw access-unit payload (`AdtsFrame::raw_payload`) and the
configuration's AudioSpecificConfig (`EncoderConfig::audio_specific_config`)
are handed to the muxer. The encoded audio is identical for both containers.

## Supported input

- Native FLAC file, single file only.
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
`1` conversion failure, `2` command-line usage error.

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
and concurrent publication. Ignored independent-decoder smoke tests for both
ADTS and M4A can be run when `ffmpeg` is available:

```sh
cargo test -p aac-convert --test independent_decode -- --ignored --nocapture
```

## Scope limits

- No directory traversal, batch conversion, metadata, cover art, ReplayGain,
  resampling, downmixing, VBR 1–4, or parallel conversion.
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
