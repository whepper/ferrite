# M7.5 — M4A feasibility spike

Status: **spike only, not production code.** This directory is deliberately not
a member of the `rust/` workspace and nothing in the production converter
depends on it.

Goal: answer whether the existing native Rust AAC-LC VBR-5 encoder can produce
a valid audio-only `.m4a` with a Rust muxer, without FFmpeg in the production
path, and what gapless signaling would cost.

## How to run

```sh
cd rust/convert/spike-m4a
CARGO_TARGET_DIR=$TMPDIR/m75-target cargo run --release -- \
  ../../convert/tests/fixtures/stereo-48k-16.flac /tmp/out.m4a
```

Add `--elst` to add a single edit-list entry that trims encoder priming and
tail padding. The spike has its own `Cargo.lock`; it does not modify
`rust/Cargo.lock`.

## Candidate muxers

Evidence below comes from the published crate sources
(`mp4 0.14.0`, `muxide 0.5.0`, `shiguredo_mp4 2026.5.0`), not from READMEs.

| | `shiguredo_mp4` 2026.5.0 | `mp4` 0.14.0 | `muxide` 0.5.0 |
|---|---|---|---|
| Writes MP4/M4A | yes (`Mp4FileMuxer`) | yes (`Mp4Writer`) | yes (`MuxerBuilder`) |
| AAC entry + ASC | generic `Mp4aBox` + `EsdsBox`/descriptors; caller builds ASC | `AacConfig` → `Mp4aBox` + `EsdsBox`, writes 2-byte ASC | accepts ADTS frames, strips header, writes 2-byte ASC |
| Streaming output | yes: caller appends sample bytes, muxer keeps metadata only | yes: writes `mdat` as it goes, 1 s chunk buffer, metadata only | **no**: `SampleInfo { data: Vec<u8> }` keeps every sample in memory |
| Gapless/edit list | none in muxer (`edts_box: None`); docs explicitly say gap boxes are unsupported because player support varies | `TrakBox` has `edts`, but the writer never sets it and `Mp4TrackWriter` is private | no `elst`/`edts` code at all |
| License | Apache-2.0 | MIT | MIT OR Apache-2.0 |
| MSRV | 1.93, edition 2024 | unstated; released 2023-08 | 1.86 |
| Dependencies | **none** | `byteorder`, `bytes`, `num-rational`, `serde`, `serde_json`, `thiserror` | `anyhow`, `serde`, `serde_json`, `thiserror`; default `cli` feature adds `clap` + `indicatif` |
| Maintenance | active (2026.6 canary) | last release 2023-08 | 0.5.0 published 2026-10-19 (42 downloads); 0.2.5 is the more-used line |

`muxide` is rejected for the production path because it buffers the complete
output. `mp4` works and is MIT, but is stale, has a larger dependency
footprint, and its writer exposes no edit list. `shiguredo_mp4` was chosen for
the prototype: zero dependencies, true streaming, Apache-2.0, maintained.

Other crates screened from crates.io and sources:

- `mp4e 1.0.5` — MIT, zero dependencies, writes AAC-LC to a generic writer and
  keeps only sample metadata. This is a credible minimal alternative; it has no
  edit-list support either, and is a single-author crate first published in
  late 2025. Not prototyped.
- `mp4box 0.14.0` — parser/editor only; no muxing API for new media.
- `mp4forge 0.8.0` — remux/inspection-oriented CLI/library (async Tokio,
  encryption features) with raw-video input only; not a focused raw-AAC muxer.
- `mp4-core`, `mp4-track`, `mp4io`, `oxideav-mp4` — all first published in
  2026 with low adoption; some pull container frameworks or encryption
  dependencies. Not evaluated to prototype depth.

## Encoder API findings

All container inputs are obtainable, but not all directly:

- **Raw AAC payload**: available only by stripping the 7-byte ADTS header from
  `AdtsFrame::bytes`. This is structurally safe: `frame::build_adts_frame`
  prepends `adts::encode_header` to the raw block, and the ADTS length field
  counts header + payload. The spike re-validates syncword, length, and
  single-raw-block fields before slicing. There is no raw-frame accessor.
- **AudioSpecificConfig**: not exposed anywhere in the Rust encoder (the C++
  `tpenc_asc` path is not ported). The spike builds the two-byte AAC-LC ASC
  from `AOT_AAC_LC`, the sampling-frequency index, and
  `ChannelMode::channel_config()`. It has to spell out the SFI because
  `aac_enc::adts` is private: 48 kHz → 3, 44.1 kHz → 4. Outputs: `11 90`
  (48 kHz stereo) and `12 08` (44.1 kHz mono), both accepted by ffprobe.
- **Sample rate / channels**: public via `EncoderConfig`.
- **Priming**: `EncoderConfig::encoder_delay()` is public and returns 1600.
- **Tail padding**: not exposed by the encoder, but derivable from
  `frames * 1024 - source_frames - 1600` because the encoder emits
  `ceil((S + 1600) / 1024)` frames.
- **Transport**: `Encoder::new` rejects anything except `TransportType::Adts`,
  even though `TransportType::Raw` exists as an enum variant.

Smallest justified extension (not implemented in this spike):

1. `AdtsFrame::raw_payload(&self) -> &[u8]` (or a `RawFrame` return type), and
2. `EncoderConfig::audio_specific_config() -> [u8; 2]` (or a public sampling
   frequency index), and
3. optionally `Encoder::tail_padding(source_frames)` for a single source of
   truth.

None of these would touch the encoding algorithm.

## Container layout produced

`ftyp` + `free` + `mdat` + `moov`; one `trak`:

- `stsd` → `mp4a` → `esds` → ES descriptor → decoder config
  (OTI `0x40`, audio stream type) → `DecoderSpecificInfo` (ASC) → SL config.
- `stts` = one entry `(14, 1024)`, `stsz` per-sample sizes, `stsc`, `stco`.
- optional `edts` → `elst` with `media_time = 1600`,
  `edit_duration = source_frames`, rate 1.0.

Independent box walk confirmed the boxes tile the file exactly and that the
`mdat` AAC payload is 960 bytes for the 14-frame fixture, i.e. exactly the
1058-byte ADTS stream minus 14 × 7 header bytes.

## Verification results

Fixture `stereo-48k-16.flac`: 12,000 source frames, 48 kHz stereo.

| Artifact | ffprobe | ffmpeg decode | afconvert (CoreAudio) |
|---|---|---|---|
| no edit list | AAC-LC, 48 kHz, 2 ch, duration 0.298667 s, 14 frames, ASC `1190` | 14,336 frames | 12,224 frames |
| with edit list | AAC-LC, 48 kHz, 2 ch, duration **0.250000 s**, 14 frames | **12,000 frames** exactly | 12,224 frames |

- ffmpeg-decoded edit-list output is byte-identical to the no-edit-list output
  after dropping the first 1,600 frames: `elst == plain[1600..13600]` → the
  trimming is exactly the encoder delay, and the tail is trimmed as well.
- A deliberately bogus edit list (`media_time = 0`, `edit_duration = 10000`)
  makes ffmpeg output 10,000 frames, proving ffmpeg really honors the box.
- CoreAudio's `afconvert` outputs 12,224 frames for *all three* files (plain,
  correct edit list, bogus edit list). It ignores the edit list, assumes its
  own 2,112-sample AAC priming, so playback starts 512 source frames late
  (`ca[0] ≈ src[512]`) and retains the 736-frame tail padding.
- Mono 44.1 kHz: ASC `1208`, 3,000 source frames, 5 AAC frames; ffmpeg +
  edit list → 3,000 frames; CoreAudio → 3,008 frames (same 2,112 assumption).
- `ffmpeg -v error -i out.m4a -f null -` reports no errors on any artifact.

Conclusion: the file is a valid, decodable M4A. It is **not** demonstrably
gapless across players. Edit lists are honored by ffmpeg and ignored by
CoreAudio; a production claim of gapless playback is therefore not justified
without a product decision on player support (e.g. also writing `iTunSMPB`,
which was not evaluated here and is metadata work).

## Memory

The muxer stores only per-sample metadata (`duration`, `size`, `offset`,
`keyframe`); sample bytes are written to the file immediately.

Measured with `/usr/bin/time -l` on synthetic stereo 48 kHz input, release
build, edit list enabled:

| Input | Peak RSS | Output | Encode time |
|---|---|---|---|
| 10 s (480,000 frames) | 3,850,240 B | 48,059 B | 0.21 s |
| 120 s (5,760,000 frames) | 4,243,456 B | 564,375 B | 0.55 s |

RSS grows by ~384 KiB while the AAC payload grows by ~516 KiB; the complete
output is not retained. The 120 s file reports duration exactly `120.000000`
via the edit list.

## Licensing

- `shiguredo_mp4`: Apache-2.0. Binary distribution requires retaining the
  license/notice and providing the Apache-2.0 text; no source-availability
  obligation for the rest of the app beyond Apache's notice terms.
- `mp4`: MIT (attribution in binary distribution).
- `muxide`: MIT OR Apache-2.0.
- The FDK-AAC-derived encoder remains under its own FDK license; this spike
  changes nothing there. Release-level MPL/notice review is still outstanding
  from M7.3.1.

## Recommendation

Proceed with `shiguredo_mp4` for M7.5/M4A production work, after a small,
non-behavioral encoder API extension for raw payload and ASC. Keep the ADTS
header-stripping validation as a defensive check.

Open product decision: gapless is **not** solved. Either accept priming/padding
(and document it), or add an edit list plus a release-level decision about
players that ignore it. Do not claim gapless until it is verified on the
players the product targets.
