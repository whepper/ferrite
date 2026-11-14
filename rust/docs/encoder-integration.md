# Using Ferrite from an application

This guide is for an application developer who wants to encode PCM to AAC-LC
with `ferrite-aac`. It covers the supported production matrix, the public API,
streaming and finalization rules, error handling, and the known limits. It does
not describe the psychoacoustic or quantization internals.

## Dependency

The crate is `ferrite-aac` (library name `ferrite_aac`, version `0.3.2`). The
supported path today is a source dependency; this repository does not yet
publish a stable crates.io release contract.

```toml
[dependencies]
ferrite-aac = { path = "../fdk-aac/rust" }
```

MSRV is Rust 1.99.1. No C toolchain, FFmpeg, or external library is needed at
build time or at run time.

## Supported production matrix

| Item | Supported |
|---|---|
| Container | ADTS (MPEG-4, no CRC), 7-byte header per frame |
| Profile | AAC-LC (AOT 2) |
| Sample rates | 44,100 Hz and 48,000 Hz |
| Channels | mono (1) and stereo (2), interleaved |
| Input samples | `i16`, native endian in memory |
| Frame length | 1024 samples per channel |
| Rate control | VBR-1 through VBR-6, afterburner on; default VBR-6 (nominal 112 kbit/s mono, 192 kbit/s stereo total). See [VBR profiles](vbr-profiles.md) |
| Encoder delay | 1600 samples per channel |
| Threading | one owned encoder per stream; no internal synchronization |

`EncoderConfig::new` validates the wider FDK parameter plane (12 sample rates,
1–8 channels, RAW or ADTS transport), but the production `Encoder` deliberately
accepts only the four cells above. An out-of-matrix configuration passes
`EncoderConfig::new` and is then rejected by `Encoder::new` with
`EncodeError::UnsupportedConfiguration`. Check that result at startup; it is
not a mid-stream error.

Other sample rates, more than two channels, floating-point PCM, and 24-bit PCM
are not accepted. The application must resample, downmix, and convert to
16-bit before calling the encoder.

## Minimal production path

```rust
use std::io::Write;

use ferrite_aac::aac_enc::config::{EncoderConfig, TransportType, FRAME_LENGTH};
use ferrite_aac::aac_enc::encoder::Encoder;

fn encode<W: Write>(pcm: &[i16], channels: u32, out: &mut W) -> Result<(), Box<dyn std::error::Error>> {
    let config = EncoderConfig::new(48_000, channels, FRAME_LENGTH, TransportType::Adts)?;
    let mut encoder = Encoder::new(config)?;

    // Feed bounded, channel-aligned chunks; write frames as they arrive.
    let samples_per_chunk = 8_000 * channels as usize;
    for chunk in pcm.chunks(samples_per_chunk) {
        for frame in encoder.push(chunk)? {
            out.write_all(&frame.bytes)?;
        }
    }

    // Flush exactly once and write the delayed tail.
    for frame in encoder.flush()? {
        out.write_all(&frame.bytes)?;
    }
    assert!(encoder.is_finished());
    Ok(())
}
```

`EncoderConfig::stereo_48k_adts()` is a shortcut for the common 48 kHz stereo
cell (VBR-6 default). The full constructor is used above to keep the rate and
channel count explicit; it also defaults to VBR-6. Use
`EncoderConfig::new_with_mode(..., VbrMode::Vbr5)` or
`.with_vbr_mode(VbrMode::Vbr1)` for explicit profiles. See
[VBR profiles](vbr-profiles.md) for the full table and examples.

A runnable version that writes ADTS to stdout is
[`examples/aac_encode_adts.rs`](../examples/aac_encode_adts.rs):

```sh
cargo run --release --example aac_encode_adts > example.aac
```

## PCM input

- Samples are interleaved per frame: stereo is `[L, R, L, R, ...]`, mono is
  `[M, M, ...]`.
- Every `push` slice must contain a whole number of multi-channel samples.
  A slice whose length is not a multiple of the channel count is rejected with
  `EncodeError::UnalignedInput`; nothing is buffered and the call can be
  retried with aligned data.
- An empty slice is a successful no-op. It is still rejected after the encoder
  has finished.
- An encoder that receives no PCM at all still flushes to a short silent
  stream (two 1024-sample frames at the 1024 frame length). Reject empty source
  files at the application level if a silent stream is not the desired result.
- Chunk sizes are otherwise arbitrary. A target of about 8,000 sample frames
  per channel is a good default; it matches the reference `aac-convert`
  application.
- Bounded chunks are still recommended for peak memory. The feeder copies
  each push into per-channel buffers; a whole-track push therefore holds the
  source, the pending copy, and every returned frame at once. Buffer work is
  linear for any push size. In a local release-mode check, a 5-minute stereo
  48 kHz buffer took about 0.70 s as one push and about 0.70 s in
  8,000-frame chunks, with identical bytes; peak RSS (including the 57.6 MB
  source buffer) was about 178 MB for the whole push versus about 61 MB for
  8,000-frame chunks.
- The encoder does not resample, downmix, normalize, or apply gain. Convert
  the source to the supported rate, channel count, and 16-bit format first.

## Output

Each completed frame is returned as an `AdtsFrame`:

- `frame.bytes` is the complete ADTS frame, header included, ready to write.
- `frame.frame_index` is the 0-based stream position and is contiguous across
  `push` and `flush`.
- `frame.raw_payload()` borrows the AAC access unit without the 7-byte ADTS
  header, for an MP4/M4A muxer.
- `EncoderConfig::audio_specific_config()` returns the two-byte AAC-LC
  AudioSpecificConfig needed by an out-of-band (RAW) container.

The encoder does not retain completed frames. Write or drop each returned
frame before the next call; do not accumulate the whole stream in memory unless
the application actually needs it.

## Finalization

Call `flush` exactly once after the last `push`:

- It emits the zero-padded partial frame plus the encoder-delay frames. The
  total frame count is `ceil((samples_per_channel + 1600) / 1024)`.
- When `flush` returns `Ok`, all output has been produced. Write the returned
  frames, then the stream is complete.
- After a successful `flush`, `is_finished()` is true and further `push` or
  `flush` calls return `EncodeError::AlreadyFinished`.
- A second `flush` is never a way to obtain more output.
- `is_finished()` is also true after a terminal error. Treat `flush`
  returning `Ok` as the success condition, not `is_finished()` alone.
- ADTS carries no gapless metadata. The first 1600 decoded samples are
  filterbank/look-ahead priming. For gapless playback, use a container with
  delay/padding signalling (the reference `aac-convert` writes M4A
  `iTunSMPB`; its edit-list variant is experimental) or trim on playback using
  `EncoderConfig::encoder_delay()`.

## Error handling

The VBR gain loop may need more than one outer pass to fit a frame into the
bit budget (FDK's normal retry path). That is handled internally: the converged
final pass is emitted, so a successful multi-pass frame does not surface as an
error. `FrameFailed` is reserved for genuinely unrecoverable frame conditions.

| Error | Meaning | Recovery |
|---|---|---|
| `UnalignedInput` | Slice length is not a multiple of the channel count | Recoverable; no state changed |
| `UnsupportedConfiguration` | Not one of the four production cells | Fix the configuration before encoding |
| `Config` | Constructor-level validation failure | Fix the configuration |
| `AlreadyFinished` | `push`/`flush` after completion | Lifecycle bug; no state change |
| `FrameFailed { .. }` | A frame failed to encode | Terminal; encoder is poisoned |
| `Poisoned` | Any use after a terminal error | Terminal; create a new encoder |

Only `UnalignedInput` is recoverable within an open encoder. `AlreadyFinished`
is a lifecycle error with no state change. A `FrameFailed` result, or any use
of a poisoned encoder, is terminal: the encoder cannot resume. Frames
completed earlier in the same `push`/`flush` call are discarded (the error
reports how many via `completed_before`), so an application that has already
written a partial file should discard that output and re-encode from the start
rather than publish a truncated stream.

The reference application writes to a temporary file in the destination
directory, validates each ADTS frame, and publishes atomically only after a
successful `flush`; see `convert/src/lib.rs`. That is the recommended pattern
for file output.

## Memory and long streams

The encoder owns a fixed amount of per-stream state plus at most one partial
input frame and the delay padding. It does not accumulate the encoded stream.
Memory use is therefore flat across a long stream as long as the caller writes
or drops each returned frame and does not keep the whole input buffer alive.

The `frame_index` is a `u32` (about 25,000 hours, or 2.9 years, of 48 kHz audio
at 1024 samples per frame); the internal emitted-frame counter is `u64`. A
single encoder is intended for one stream on one thread. Create independent
encoders for independent streams.

## Known limitations

- Only 44.1/48 kHz, mono/stereo, AAC-LC, VBR-1 through VBR-6 (default VBR-6), ADTS.
- 16-bit interleaved PCM only; no float, 24-bit, resampling, downmix, or
  channel-layout conversion.
- Profile selection is via `VbrMode` (`--vbr` in `aac-convert`); no other
  object types or CBR modes.
- No RAW/LATM/LOAS/ADIF output from the production `Encoder` (the config plane
  accepts RAW, but `Encoder::new` rejects it).
- No metadata, ID3, or gapless container in the library; applications must
  handle delay/padding.
- Whole-buffer pushes are linear in buffer work but hold the input, feeder
  copy, and all returned frames at once; use bounded chunks to keep peak
  memory low.
- The crate is source-consumed and version `0.3.2`; the public API and MSRV are
  not yet a stable published contract.
- Licensing is the FDK-AAC license (see `rust/NOTICE` and
  `rust/LICENSE-FDK-AAC`); it is not a general-purpose permissive license, and
  patent authorization is the integrator's responsibility.
- Validation is structural and decoder-based. It is not a perceptual
  equivalence claim; listening review remains a separate step.

## Reference consumer

`rust/convert` (`aac-convert`) is the reference production consumer. It shows
the complete pattern:

- construct `EncoderConfig` and `Encoder`,
- feed FLAC-decoded PCM in 8,000-frame chunks (`encode_chunks`),
- validate every frame (`adts::validate`),
- write ADTS or mux the raw payload into M4A,
- flush once and write the tail,
- publish through a temporary file only after success,
- map `EncodeError` into application errors.

See `convert/src/lib.rs` and `convert/README.md`. The application-facing
contract, including PCM precision, metadata preservation, error reporting,
atomic publication, and gapless signalling, is in
[`aac-convert-integration-contract.md`](aac-convert-integration-contract.md).
