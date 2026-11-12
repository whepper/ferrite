# Ferrite integration contract

Status: **controlled personal integration**. This document defines what a
consumer application may rely on today. It is not a promise of universal
player compatibility, bitstream equivalence with FDK, or perceptual quality.

## 1. Integration options

- **`ferrite-aac` library:** feed interleaved `i16` PCM, receive ADTS frames.
- **`aac-convert` reference consumer:** feed FLAC files, receive ADTS or M4A.
  It owns FLAC decoding, metadata, cover art, loudness analysis, M4A muxing,
  and atomic publication.

A consumer that already owns a decoder and container layer should use the
library. A consumer that wants the tested file workflow should use or copy the
`aac-convert` pattern. MusicPack integration is not part of this task.

## 2. Input contract

### `ferrite-aac` encoder

| Item | Contract |
|---|---|
| PCM | Interleaved `i16`, native endian in memory |
| Layout | `[L, R, L, R, …]` stereo; `[M, …]` mono |
| Alignment | Every push length must be a multiple of the channel count |
| Sample rates | 44,100 Hz or 48,000 Hz |
| Channels | mono (1) or stereo (2) |
| Frame length | 1024 samples per channel |
| Transport | ADTS only in the production `Encoder` |
| Mode | AAC-LC, VBR-5, afterburner on, PNS off |

Out-of-matrix configurations are rejected at construction. The encoder does
not resample, downmix, normalize, dither, or apply gain.

### `aac-convert` file input

| Item | Contract |
|---|---|
| Container | Native FLAC |
| Sample rates | 44,100 Hz or 48,000 Hz |
| Channels | mono or stereo |
| Bit depth | 16-bit, or 24-bit reduced to 16-bit |
| Integrity | FLAC MD5 verified when present and nonzero |
| Traversal | One file, or recursive directory batch |

FLAC MD5 absence is accepted after successful decoding; the converter does not
claim verification when no checksum exists.

## 3. PCM precision policy

The supported input contract is **`i16` PCM**. This is a deliberate boundary,
not an oversight.

- 24-bit FLAC is accepted by `aac-convert` and reduced to the top 16 bits by
  arithmetic right shift (`>> 8`). This is truncation toward negative
  infinity, not rounding and not dithering. The result is always within the
  `i16` range, including negative full scale.
- The reduction happens before the encoder sees the samples. Ferrite’s
  internal MDCT, quantization, and rate control are unchanged and continue to
  operate on the existing `i16`-derived Q31 spectrum.
- A per-file warning discloses the reduction. The warning is emitted for ADTS
  and M4A, and is not suppressed by `--no-tags`.
- Applications that need a different 24→16 policy (rounding, dithering,
  noise shaping, or an explicit 24-bit pipeline) must implement it before
  calling the encoder. They must not assume `aac-convert` applies dithering.

### Why not add `i32`/`f32` input now

A wider input path would require a new public API, clipping/scaling rules,
dithering policy, feeder changes, new regression vectors, and revalidation of
the production byte-identity pins. No current integration requirement
demonstrates that this expansion is needed: the reference consumer already
owns the decode-and-convert step, and the encoder’s validated matrix is
16-bit. A future 24-bit/float API should be designed and validated separately;
it must not silently change the existing `i16` contract.

## 4. Metadata preservation policy

`aac-convert` M4A output preserves the core tags listed in
[`convert/README.md`](../convert/README.md#metadata):

- title, artist, album, track number/total, disc number/total,
  album artist, date/year, genre, composer, comment;
- one embedded JPEG or PNG cover image;
- generated ReplayGain 2.0 and `iTunNORM` atoms;
- `iTunSMPB` gapless signalling;
- a `©too` encoder identification with the value `Ferrite <version>`, built
  from Cargo package metadata and written to every M4A output.

Behavior:

- Keys match ASCII case-insensitively; values are trimmed.
- Core atoms are single-valued and written once. If a recognized text key has
  multiple distinct values, the first non-empty value is kept and a per-file
  warning names the reduced keys.
- Missing or invalid tag values warn; they never fail an otherwise valid
  conversion.
- Unmapped keys are counted in the `TagDiagnostics` and shown by the CLI in
  verbose mode. They are not guessed into non-standard atoms.
- Artwork selection is deterministic: front cover, then other typed pictures,
  then untyped pictures, in source order. JPEG/PNG only, maximum 16 MiB.
- `--no-tags` omits user tags, cover art, and loudness atoms; it does not
  change audio payloads, `iTunSMPB`, or the `©too` encoder identification.
- Tags never alter the AAC payload. The encoded AAC access units are
  byte-identical with and without tags.

Deliberately not preserved today: multi-valued text fields beyond the first
value, ISRC, barcode, publisher/label, catalogue number, MusicBrainz IDs,
copyright, and other identifiers. There is no stable core M4A atom for these
in the supported subset. Preserving them would require a separate, explicit
freeform metadata model and compatibility tests.

## 5. Error handling and recovery

- `EncodeError::UnalignedInput` is recoverable: no state changes; retry with
  aligned PCM.
- `EncodeError::FrameFailed` is terminal: the encoder is poisoned. The error
  carries the failing frame index, how many frames completed earlier in the
  call, and the underlying failure category/reason.
- `aac-convert::ConvertError::Encoder` displays the encoder error, including
  frame index and cause, and states that the temporary output was discarded
  and no output file was published. Its `source()` chain is preserved.
- Batch mode continues after per-file failures, reports each affected input
  file and failure, and exits nonzero if any file failed.
- Batch preparation and encoding use a bounded worker pool
  (`BatchOptions::jobs`, default `aac_convert::batch::default_jobs()`). Each
  worker owns its decoder, encoder, metadata state, and temporary file; no
  mutable encoder state is shared. Preparation and encoding are
  barrier-separated stages that reuse the same limit, so concurrency never
  exceeds `jobs`. Per-file results remain in plan order; progress callbacks
  may arrive in completion order.
- `--skip-existing` skips known destinations before decode/encode work and
  still enforces no-overwrite publication.

## 6. Temporary output and atomic publication

Every file is written to a hidden temporary file in the destination
directory, flushed, synced, and published by a hard link only after decoding,
encoding, container finalization, and validation succeed. The hard link fails
if the destination exists, so an existing file is never replaced. On failure
the temporary file is removed. There is no check-then-rename fallback; a
filesystem without hard-link support fails safely with a publication error.

A consumer must treat the destination as all-or-nothing and must not publish
partial output from a failed conversion.

## 7. M4A gapless signalling

M4A output carries `iTunSMPB` with:

- encoder priming = 1600 samples;
- original source sample count;
- remaining/tail padding from the encoded frame count.

Decoders that honor `iTunSMPB` present exactly the source sample count.
FFmpeg and CoreAudio are the verified decode paths. No edit list is written by
default, and universal gapless playback is not claimed. ADTS carries no
gapless metadata; consumers must trim using the known encoder delay.

## 8. Current limitations

- 44.1/48 kHz, mono/stereo only; no resampling or downmixing.
- `i16` PCM only; 24-bit input is truncated before encoding, with no dither.
- Core tags only; multi-values are first-value, identifiers are not mapped.
- One cover image only; JPEG/PNG only; 16 MiB limit.
- Bounded parallel batch conversion; no intra-file or frame-level
  parallelism.
- No perceptual quality, transparency, or ABX claim.
- No stable crates.io release contract yet; use a pinned source dependency.
- Licensing is FDK-AAC; patent authorization remains the integrator’s
  responsibility.

## 9. Integration acceptance checklist

- Construct `EncoderConfig` and `Encoder` only for the supported matrix.
- Convert source PCM to interleaved `i16`; apply any dithering/rounding policy
  before the encoder.
- Write frames only through a temporary file and publish atomically.
- Preserve the encoder delay (1600 samples) and honor `iTunSMPB`.
- Treat `FrameFailed` as terminal and discard that track’s output.
- Accept first-value metadata behavior or read extra values from the source.
- Verify decoded sample counts and M4A gapless fields in the host application.
