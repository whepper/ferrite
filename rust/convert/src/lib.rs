//! Single-file FLAC to AAC conversion (ADTS or M4A).
//!
//! This crate is deliberately a thin application around the existing
//! `aac::aac_enc` encoder. It accepts one 16-bit FLAC file at 44.1 kHz or
//! 48 kHz, mono or stereo, and writes either an ADTS stream or an audio-only
//! M4A container.
//!
//! M4A is the intended user-facing output; ADTS remains available as an
//! engineering/test format. M4A uses `shiguredo_mp4`'s streaming muxer with
//! the encoder's raw access-unit payload and AudioSpecificConfig, plus
//! `iTunSMPB` gapless signalling with the Apple-style brand so honoring
//! decoders present exactly the source audio. No edit list is written and the
//! duration fields keep the encoded-stream timeline; see the README.
//!
//! The encoder's `i16` PCM interface is unchanged; 24-bit input is rejected
//! until a conversion and dithering policy is approved.

mod adts;
mod flac;
mod gapless;
mod m4a;
mod output;

use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use aac::aac_enc::config::{ConfigError, EncoderConfig, TransportType, FRAME_LENGTH};
use aac::aac_enc::encoder::{AdtsError, AdtsFrame, EncodeError, Encoder};
use shiguredo_mp4::mux::MuxError;

/// Target number of sample frames per encoder push.
///
/// The encoder feeder drains from the front of its per-channel buffers, so
/// whole-file pushes can become quadratic. This keeps every encoder push
/// bounded. Decoded FLAC packets are staged separately and are bounded by the
/// decoder's FLAC block size, which may exceed this value; the whole file is
/// never buffered.
pub const CHUNK_FRAMES: usize = 8_000;

/// Output container written by the converter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// ADTS AAC stream (engineering/test output).
    Adts,
    /// Audio-only MP4/M4A container.
    M4a,
}

impl OutputFormat {
    /// Infer the format from the output path's extension.
    ///
    /// `.m4a` and `.mp4` select M4A. Everything else, including `.aac`,
    /// `.adts`, unknown extensions, and no extension, keeps the historical
    /// ADTS default. An explicit CLI `--format` overrides this inference.
    pub fn from_output_path(output: &Path) -> Self {
        match output
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("m4a" | "mp4") => Self::M4a,
            _ => Self::Adts,
        }
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Adts => "ADTS",
            Self::M4a => "M4A",
        })
    }
}

/// Properties of the supported FLAC input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioProperties {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u32,
    /// Source bits per sample.
    pub bits_per_sample: u32,
}

/// Result of a successful single-file conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversionSummary {
    /// Input properties used for encoding.
    pub properties: AudioProperties,
    /// Number of AAC frames written.
    pub encoded_frames: u64,
    /// Number of bytes written to the output file.
    pub output_bytes: u64,
}

/// Failure of a single-file conversion.
#[derive(Debug)]
pub enum ConvertError {
    /// The input could not be opened or inspected.
    InputOpen(PathBuf, io::Error),
    /// The input exists but is not a regular file.
    InputNotFile(PathBuf),
    /// The input was not recognized as a readable FLAC stream.
    InputNotFlac(PathBuf),
    /// No audio track was present.
    MissingAudioTrack,
    /// Sample rate, channels, or bit depth was absent from the stream.
    MissingAudioProperties,
    /// The sample rate is outside this milestone's supported set.
    UnsupportedSampleRate(u32),
    /// The channel count is outside this milestone's supported set.
    UnsupportedChannels(u32),
    /// The bit depth is not supported by the current `i16` encoder interface.
    UnsupportedBitDepth(u32),
    /// Decoding or source-integrity verification failed.
    DecodeFailed,
    /// The input and output paths resolve to the same file.
    SamePath(PathBuf),
    /// The output path already exists.
    OutputExists(PathBuf),
    /// The output directory does not exist.
    OutputDirectoryMissing(PathBuf),
    /// The output path could not be inspected.
    OutputInspect(PathBuf, io::Error),
    /// A temporary output file could not be created.
    TempCreate(PathBuf, io::Error),
    /// A temporary output file could not be written.
    TempWrite(PathBuf, io::Error),
    /// The encoder configuration was rejected.
    EncoderConfig(ConfigError),
    /// The encoder failed while processing a frame.
    Encoder(EncodeError),
    /// The encoder produced a frame that is not valid ADTS.
    AdtsInvalid(&'static str),
    /// The AAC decoder configuration could not be derived.
    AudioSpecificConfig(AdtsError),
    /// The MP4 muxer rejected a sample or could not finalize the container.
    Muxer(MuxError),
    /// M4A gapless signalling failed.
    GaplessSignalling(&'static str),
    /// The temporary file could not be flushed or synced.
    Finalize(PathBuf, io::Error),
    /// The destination could not be published without replacing an existing file.
    Publish(PathBuf, io::Error),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputOpen(path, _) => {
                write!(f, "could not open input file: {}", path.display())
            }
            Self::InputNotFile(path) => {
                write!(f, "input is not a regular file: {}", path.display())
            }
            Self::InputNotFlac(path) => {
                write!(f, "input is not a supported FLAC file: {}", path.display())
            }
            Self::MissingAudioTrack => write!(f, "input contains no audio track"),
            Self::MissingAudioProperties => {
                write!(f, "could not determine FLAC sample rate, channels, or bit depth")
            }
            Self::UnsupportedSampleRate(rate) => write!(
                f,
                "unsupported sample rate {rate} Hz; supported rates are 44100 and 48000 Hz"
            ),
            Self::UnsupportedChannels(channels) => write!(
                f,
                "unsupported channel count {channels}; only mono and stereo are supported"
            ),
            Self::UnsupportedBitDepth(bits) => write!(
                f,
                "unsupported bit depth {bits}; only 16-bit FLAC input is supported in this milestone \
                 (24-bit conversion is deferred)"
            ),
            Self::DecodeFailed => write!(
                f,
                "could not decode the FLAC input; the file may be corrupt or use unsupported features"
            ),
            Self::SamePath(path) => {
                write!(f, "input and output refer to the same file: {}", path.display())
            }
            Self::OutputExists(path) => write!(
                f,
                "output file already exists: {} (overwriting is not supported)",
                path.display()
            ),
            Self::OutputDirectoryMissing(path) => {
                write!(f, "output directory does not exist: {}", path.display())
            }
            Self::OutputInspect(path, _) => {
                write!(f, "could not inspect output path: {}", path.display())
            }
            Self::TempCreate(_, _) => {
                write!(f, "could not create a temporary output file in the destination directory")
            }
            Self::TempWrite(_, _) => write!(f, "could not write the temporary output file"),
            Self::EncoderConfig(_) => {
                write!(f, "the input format is not supported by the AAC encoder")
            }
            Self::Encoder(_) => write!(f, "AAC encoding failed"),
            Self::AdtsInvalid(reason) => {
                write!(f, "the encoder produced an invalid ADTS stream ({reason})")
            }
            Self::AudioSpecificConfig(_) => write!(
                f,
                "could not derive the AAC decoder configuration for this input"
            ),
            Self::Muxer(_) => write!(f, "could not write the M4A container"),
            Self::GaplessSignalling(reason) => {
                write!(f, "M4A gapless signalling failed ({reason})")
            }
            Self::Finalize(_, _) => write!(f, "could not finalize the output file"),
            Self::Publish(_, _) => write!(
                f,
                "could not publish the output file without replacing an existing file; \
                 the destination filesystem may not support hard links"
            ),
        }
    }
}

impl StdError for ConvertError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::InputOpen(_, err)
            | Self::OutputInspect(_, err)
            | Self::TempCreate(_, err)
            | Self::TempWrite(_, err)
            | Self::Finalize(_, err)
            | Self::Publish(_, err) => Some(err),
            Self::EncoderConfig(err) => Some(err),
            Self::Encoder(err) => Some(err),
            Self::AudioSpecificConfig(err) => Some(err),
            Self::Muxer(err) => Some(err),
            _ => None,
        }
    }
}

impl AudioProperties {
    fn validate(self) -> Result<(), ConvertError> {
        if !matches!(self.sample_rate, 44_100 | 48_000) {
            return Err(ConvertError::UnsupportedSampleRate(self.sample_rate));
        }
        if !matches!(self.channels, 1 | 2) {
            return Err(ConvertError::UnsupportedChannels(self.channels));
        }
        // The public encoder interface is `i16`. A 24-bit conversion/dithering
        // policy is not approved yet, so reject rather than silently truncate.
        if self.bits_per_sample != 16 {
            return Err(ConvertError::UnsupportedBitDepth(self.bits_per_sample));
        }
        Ok(())
    }
}

/// Convert one FLAC file to one ADTS file.
///
/// This keeps the original M7.3 behavior. Use
/// [`convert_file_with_format`] to select M4A.
///
/// The destination is written through a temporary file in its parent
/// directory and published by an atomic hard link only after decoding, encoder
/// flush, output validation, file flush, and sync have succeeded. The hard link
/// fails if the destination already exists, so an existing output is never
/// replaced. Filesystems that do not support hard links fail the conversion;
/// there is no check-then-rename fallback.
pub fn convert_file(input: &Path, output: &Path) -> Result<ConversionSummary, ConvertError> {
    convert_file_with_format(input, output, OutputFormat::Adts)
}

/// Convert one FLAC file to ADTS or M4A.
///
/// M4A writes an audio-only ISO BMFF file with one AAC-LC `mp4a` track. The
/// encoder still runs in its existing ADTS mode; each frame's validated raw
/// access-unit payload and the configuration's AudioSpecificConfig are passed
/// to the muxer. M4A output carries `iTunSMPB` gapless signalling (encoder
/// delay, tail padding, source-frame count) with the Apple-style `M4A` brand,
/// so decoders that honor it present exactly the source audio; no edit list
/// is written and the `mdhd`/`tkhd`/`mvhd` durations keep the full
/// encoded-stream timeline (see the crate README for the evidence and the
/// duration-display consequence).
///
/// Output publication is identical for both formats: a same-directory
/// temporary file is flushed, synced, and hard-linked to the destination,
/// which fails atomically if the destination already exists.
pub fn convert_file_with_format(
    input: &Path,
    output: &Path,
    format: OutputFormat,
) -> Result<ConversionSummary, ConvertError> {
    convert_inner(input, output, format, None).map(|(summary, _)| summary)
}

/// Experimental M4A signalling switch (M7.9 prototype, retained for research).
///
/// Default M4A output always carries `iTunSMPB` gapless signalling with the
/// Apple-style brand; this switch only adds an experimental `edts`/`elst`
/// entry on top. The edit list makes **no claim of gapless playback**: it is
/// honored by FFmpeg (except for very short streams, which decode empty) and
/// ignored by CoreAudio. See the crate README.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaplessExperiment {
    /// Write a single `edts`/`elst` entry trimming the encoder delay and the
    /// tail padding (`media_time = D`, `edit_duration = S`) in addition to
    /// the default `iTunSMPB` metadata.
    pub edit_list: bool,
}

/// Convert one FLAC file to M4A with an experimental edit list on top of the
/// default `iTunSMPB` signalling.
///
/// Behaves like [`convert_file_with_format`] with [`OutputFormat::M4a`],
/// except the output additionally carries one `edts`/`elst` entry. Extension
/// dispatch, [`convert_file`], and [`convert_file_with_format`] semantics are
/// unchanged.
pub fn convert_file_m4a_experiment(
    input: &Path,
    output: &Path,
    experiment: GaplessExperiment,
) -> Result<ConversionSummary, ConvertError> {
    convert_inner(input, output, OutputFormat::M4a, Some(experiment)).map(|(summary, _)| summary)
}

fn convert_inner(
    input: &Path,
    output: &Path,
    format: OutputFormat,
    experiment: Option<GaplessExperiment>,
) -> Result<(ConversionSummary, u64), ConvertError> {
    let paths = output::resolve_paths(input, output)?;
    let mut source = flac::FlacSource::open(&paths.input)?;
    let properties = source.properties();
    properties.validate()?;

    let config = EncoderConfig::new(
        properties.sample_rate,
        properties.channels,
        FRAME_LENGTH,
        TransportType::Adts,
    )
    .map_err(ConvertError::EncoderConfig)?;
    let mut encoder = Encoder::new(config).map_err(ConvertError::Encoder)?;
    let mut sink = Sink::create(format, &paths.output, properties, config)?;

    let mut pcm = Vec::new();
    let mut encoded_frames = 0u64;
    // Source frames are counted for M4A output (default and experimental
    // alike) so gapless accounting can be validated before publication. The
    // ADTS path keeps its exact behavior: one predictable branch, no new
    // failure modes when ADTS is requested.
    let is_m4a = matches!(format, OutputFormat::M4a);
    let mut source_frames = 0u64;

    while source.next_pcm(&mut pcm)? {
        if is_m4a {
            let channels = u64::from(properties.channels);
            if (pcm.len() as u64) % channels != 0 {
                return Err(ConvertError::GaplessSignalling(
                    "decoded PCM is not channel-aligned",
                ));
            }
            source_frames = source_frames
                .checked_add((pcm.len() as u64) / channels)
                .ok_or(ConvertError::GaplessSignalling(
                    "source-frame count overflowed",
                ))?;
        }
        encode_chunks(
            &mut encoder,
            &pcm,
            properties,
            &mut sink,
            &mut encoded_frames,
        )?;
    }
    source.verify()?;

    let tail = encoder.flush().map_err(ConvertError::Encoder)?;
    sink.write_frames(&tail, properties, &mut encoded_frames)?;

    let gapless = match (is_m4a, experiment) {
        (false, _) => None,
        (true, experiment) => Some((
            gapless::GaplessAccounting::compute(
                source_frames,
                encoded_frames,
                FRAME_LENGTH as u64,
                config.encoder_delay() as u64,
            )
            .map_err(|err| ConvertError::GaplessSignalling(err.message()))?,
            experiment.map_or(false, |experiment| experiment.edit_list),
        )),
    };
    let output_bytes = sink.finish(gapless)?;
    sink.publish(&paths.output)?;
    Ok((
        ConversionSummary {
            properties,
            encoded_frames,
            output_bytes,
        },
        source_frames,
    ))
}

fn encode_chunks(
    encoder: &mut Encoder,
    pcm: &[i16],
    properties: AudioProperties,
    sink: &mut Sink,
    encoded_frames: &mut u64,
) -> Result<(), ConvertError> {
    let samples_per_chunk = CHUNK_FRAMES * properties.channels as usize;
    for chunk in pcm.chunks(samples_per_chunk) {
        let frames = encoder.push(chunk).map_err(ConvertError::Encoder)?;
        sink.write_frames(&frames, properties, encoded_frames)?;
    }
    Ok(())
}

/// One of the two output containers, with its own frame-writing rules.
enum Sink {
    Adts {
        output: output::TempOutput,
        written: u64,
    },
    M4a(m4a::M4aWriter),
}

impl Sink {
    fn create(
        format: OutputFormat,
        output: &Path,
        properties: AudioProperties,
        config: EncoderConfig,
    ) -> Result<Self, ConvertError> {
        match format {
            OutputFormat::Adts => Ok(Self::Adts {
                output: output::TempOutput::create(output)?,
                written: 0,
            }),
            OutputFormat::M4a => Ok(Self::M4a(m4a::M4aWriter::create(
                output, properties, config,
            )?)),
        }
    }

    fn write_frames(
        &mut self,
        frames: &[AdtsFrame],
        properties: AudioProperties,
        encoded_frames: &mut u64,
    ) -> Result<(), ConvertError> {
        for frame in frames {
            match self {
                Self::Adts { output, written } => {
                    adts::validate(frame, properties, *encoded_frames)?;
                    output.write_all(&frame.bytes)?;
                    *written += frame.bytes.len() as u64;
                }
                Self::M4a(writer) => writer.write_frame(frame, *encoded_frames)?,
            }
            *encoded_frames += 1;
        }
        Ok(())
    }

    /// Finalize the container and return its final size.
    ///
    /// `gapless` carries validated accounting plus the experimental edit-list
    /// switch for M4A output; `None` (ADTS) keeps the historical behavior.
    /// M4A output without accounting is a broken internal invariant and fails
    /// closed instead of writing unsignalled output.
    fn finish(
        &mut self,
        gapless: Option<(gapless::GaplessAccounting, bool)>,
    ) -> Result<u64, ConvertError> {
        match self {
            Self::Adts { written, .. } => Ok(*written),
            Self::M4a(writer) => match gapless {
                Some((accounting, edit_list)) => writer.finish(&accounting, edit_list),
                None => Err(ConvertError::GaplessSignalling(
                    "missing gapless accounting for M4A output",
                )),
            },
        }
    }

    fn publish(self, output: &Path) -> Result<(), ConvertError> {
        match self {
            Self::Adts { output: temp, .. } => temp.publish(output),
            Self::M4a(writer) => writer.publish(output),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(pcm: &[i16], chunked: bool) -> Vec<AdtsFrame> {
        let config = EncoderConfig::new(48_000, 2, FRAME_LENGTH, TransportType::Adts).unwrap();
        let mut encoder = Encoder::new(config).unwrap();
        let mut frames = Vec::new();
        if chunked {
            let samples_per_chunk = CHUNK_FRAMES * 2;
            for chunk in pcm.chunks(samples_per_chunk) {
                frames.extend(encoder.push(chunk).unwrap());
            }
        } else {
            frames.extend(encoder.push(pcm).unwrap());
        }
        frames.extend(encoder.flush().unwrap());
        frames
    }

    #[test]
    fn chunking_preserves_encoder_output_and_adt_sequence() {
        let frame_count = 12_000i32;
        let pcm: Vec<i16> = (0..frame_count)
            .flat_map(|i| {
                let left = (i * 2 - frame_count) as i16;
                let right = (frame_count - i * 2) as i16;
                [left, right]
            })
            .collect();

        let chunked = encode(&pcm, true);
        let direct = encode(&pcm, false);
        assert!(!chunked.is_empty());
        assert_eq!(chunked.len(), direct.len());
        for (chunked_frame, direct_frame) in chunked.iter().zip(direct.iter()) {
            assert_eq!(chunked_frame.bytes, direct_frame.bytes);
            assert_eq!(chunked_frame.frame_index, direct_frame.frame_index);
        }

        let properties = AudioProperties {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 16,
        };
        for (index, frame) in chunked.iter().enumerate() {
            adts::validate(frame, properties, index as u64).unwrap();
        }
    }

    #[test]
    fn output_format_inference_is_documented() {
        use std::path::Path;

        assert_eq!(
            OutputFormat::from_output_path(Path::new("song.m4a")),
            OutputFormat::M4a
        );
        assert_eq!(
            OutputFormat::from_output_path(Path::new("song.MP4")),
            OutputFormat::M4a
        );
        for adts in ["song.aac", "song.adts", "song", "song.bin"] {
            assert_eq!(
                OutputFormat::from_output_path(Path::new(adts)),
                OutputFormat::Adts,
                "{adts}"
            );
        }
        assert_eq!(OutputFormat::Adts.to_string(), "ADTS");
        assert_eq!(OutputFormat::M4a.to_string(), "M4A");
    }
}
