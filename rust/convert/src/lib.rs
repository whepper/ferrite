//! Single-file FLAC to AAC conversion (ADTS or M4A).
//!
//! This crate is deliberately a thin application around the existing
//! `ferrite_aac::aac_enc` encoder. It accepts one 16-bit FLAC file at 44.1 kHz or
//! 48 kHz, mono or stereo, and writes either an ADTS stream or an audio-only
//! M4A container.
//!
//! M4A is the intended user-facing output; ADTS remains available as an
//! engineering/test format. M4A uses `shiguredo_mp4`'s streaming muxer with
//! the encoder's raw access-unit payload and AudioSpecificConfig, plus
//! `iTunSMPB` gapless signalling with the Apple-style brand so honoring
//! decoders present exactly the source audio. M4A additionally carries the
//! G0-approved core Vorbis-comment text tags (title, artist, album, track and
//! disc numbers with totals, album artist, date/year, genre, composer,
//! comment) and at most one JPEG/PNG cover image. Cover art can be embedded in
//! the `covr` atom, extracted as `cover.jpg`/`cover.png`, or both; ADTS carries
//! no tags or cover art. No edit list is written and the duration fields keep
//! the encoded-stream timeline; see the README.
//!
//! The encoder's `i16` PCM interface is unchanged; 24-bit input is reduced
//! to 16-bit PCM by truncation in the decode path, with a per-file warning
//! disclosing the discarded low-order precision (see the README).
//!
//! Directory batch conversion is available under [`batch`]: recursive
//! discovery, deterministic planning, and bounded-concurrency execution
//! through the same per-file pipeline documented above. Each worker owns its
//! own decoder, encoder, and output publication path.

mod adts;
pub mod batch;
mod flac;
mod gapless;
pub(crate) mod loudness;
mod m4a;
pub(crate) mod meta;
mod output;
mod parallel;

use std::error::Error as StdError;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ferrite_aac::aac_enc::config::{
    ConfigError, EncoderConfig, TransportType, VbrMode, FRAME_LENGTH,
};
use ferrite_aac::aac_enc::encoder::{AdtsError, AdtsFrame, EncodeError, Encoder};
use shiguredo_mp4::mux::MuxError;

/// Target number of sample frames per encoder push.
///
/// This keeps every encoder push (and the number of returned frames held at
/// once) bounded, so conversion memory stays flat. Decoded FLAC packets are
/// staged separately and are bounded by the decoder's FLAC block size, which
/// may exceed this value; the whole file is never buffered.
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

/// Cover-art handling requested by the CLI or library caller.
///
/// The default is [`Embedded`](Self::Embedded), preserving the historical
/// behavior. Extraction is available only for M4A output with user metadata
/// enabled; ADTS and `--no-tags` conversions never process artwork.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoverArtMode {
    /// Embed selected artwork in the M4A `covr` atom (default).
    #[default]
    Embedded,
    /// Do not embed artwork; write `cover.jpg`/`cover.png` externally.
    External,
    /// Embed and write the same selected artwork externally.
    Both,
}

impl CoverArtMode {
    /// True when the selected image is embedded in M4A output.
    pub fn embeds(self) -> bool {
        matches!(self, Self::Embedded | Self::Both)
    }

    /// True when the selected image is written next to the audio output.
    pub fn extracts(self) -> bool {
        matches!(self, Self::External | Self::Both)
    }
}

impl fmt::Display for CoverArtMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Embedded => "embedded",
            Self::External => "external",
            Self::Both => "both",
        })
    }
}

impl std::str::FromStr for CoverArtMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "embedded" => Ok(Self::Embedded),
            "external" => Ok(Self::External),
            "both" => Ok(Self::Both),
            _ => Err(format!(
                "invalid cover-art mode: {value} (expected embedded, external, or both)"
            )),
        }
    }
}

/// What happened while publishing one external cover-art file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverArtAction {
    /// The image was published atomically under a new name.
    Written,
    /// A file already existed at the destination and was left untouched.
    Existing,
    /// Different tracks in one output directory selected different images;
    /// external publication was skipped for that directory.
    Conflicted,
    /// External artwork publication failed; audio conversion is unaffected.
    Failed {
        /// User-facing detail for stderr.
        message: String,
    },
}

/// One external cover-art outcome for an output directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverArtReport {
    /// Target directory (relative to the output root in batch mode, or the
    /// resolved output parent for a single-file conversion).
    pub directory: PathBuf,
    /// Intended file name (`cover.jpg` or `cover.png`); `None` for a conflict,
    /// where no image was selected for publication.
    pub file_name: Option<String>,
    /// What happened.
    pub action: CoverArtAction,
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

/// Per-file user-metadata diagnostics for an M4A conversion.
///
/// Counts what the G0 core mapping produced for one input: mapped atoms
/// actually written, unmapped (deferred or unknown) keys seen, and warnings
/// for malformed, invalid, unsupported, or genuinely lossy values. Missing
/// optional fields (including absent artwork) are silent. Warnings never fail
/// the conversion. ADTS conversions and `--no-tags` M4A conversions report no
/// mapped atoms; `--no-tags` additionally suppresses warnings while unmapped
/// keys are still counted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagDiagnostics {
    /// Number of M4A metadata atoms written (`0`–`10`).
    pub mapped: usize,
    /// Distinct unmapped Vorbis keys seen, lowercased and sorted.
    pub unmapped: Vec<String>,
    /// Per-file warnings for invalid or unsupported values, lossy numeric
    /// reductions, and invalid/unsupported artwork; plus the 24-bit
    /// precision-reduction notice when the source exceeds 16 bits.
    pub warnings: Vec<String>,
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
    /// M4A user metadata could not be read or serialized.
    ///
    /// Tag values that are merely missing, empty, or unparseable are warnings,
    /// never this error. This variant covers phase failures such as an
    /// unreadable comment block or an unserializable tag set.
    Metadata(&'static str),
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
                "unsupported bit depth {bits}; only 16-bit and 24-bit FLAC input is supported"
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
            Self::EncoderConfig(err) => {
                write!(f, "encoder configuration rejected: {err}")
            }
            Self::Encoder(err) => write!(
                f,
                "AAC encoding failed: {err}; the temporary output was discarded and no output file was published"
            ),
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
            Self::Metadata(reason) => {
                write!(f, "could not write M4A metadata ({reason})")
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
        // The public encoder interface is `i16`. 24-bit input is accepted
        // through the existing decode path, which truncates left-justified
        // samples to their top 16 bits (see `FlacSource::next_pcm`); a
        // per-file warning discloses the precision reduction at conversion
        // time. Anything else is rejected rather than silently truncated.
        if !matches!(self.bits_per_sample, 16 | 24) {
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
/// Per-file warnings (tag issues, the 24-bit precision notice, cover-art
/// outcomes) are not returned by this function; use
/// [`convert_file_with_format_and_tags`] to inspect them. Only the precision
/// notice can arise here, as ADTS output never carries tags or cover art.
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

/// Convert one FLAC file to ADTS or M4A, writing user metadata by default.
///
/// M4A writes an audio-only ISO BMFF file with one AAC-LC `mp4a` track. The
/// encoder still runs in its existing ADTS mode; each frame's validated raw
/// access-unit payload and the configuration's AudioSpecificConfig are passed
/// to the muxer. M4A output carries `iTunSMPB` gapless signalling (encoder
/// delay, tail padding, source-frame count) with the Apple-style `M4A` brand,
/// so decoders that honor it present exactly the source audio; no edit list
/// is written and the `mdhd`/`tkhd`/`mvhd` durations keep the full
/// encoded-stream timeline (see the crate README for the evidence and the
/// duration-display consequence). M4A additionally carries the G0-approved
/// core Vorbis-comment text tags and the `©too` encoder identification
/// (`Ferrite <version>`); ADTS carries no tags.
///
/// Output publication is identical for both formats: a same-directory
/// temporary file is flushed, synced, and hard-linked to the destination,
/// which fails atomically if the destination already exists.
///
/// Use [`convert_file_with_format_and_tags`] to omit user metadata or to
/// inspect per-file tag diagnostics.
///
/// Per-file warnings (tag issues, the 24-bit precision notice, cover-art
/// outcomes) are not returned by this function; use
/// [`convert_file_with_format_and_tags`] to inspect them.
pub fn convert_file_with_format(
    input: &Path,
    output: &Path,
    format: OutputFormat,
) -> Result<ConversionSummary, ConvertError> {
    convert_file_with_format_vbr(input, output, format, VbrMode::Vbr6)
}

/// Convert one FLAC file to ADTS or M4A with an explicit VBR profile.
///
/// Behaves like [`convert_file_with_format`], except `vbr_mode` selects
/// VBR-1 through VBR-6 (default [`VbrMode::Vbr6`] in the wrapper above).
pub fn convert_file_with_format_vbr(
    input: &Path,
    output: &Path,
    format: OutputFormat,
    vbr_mode: VbrMode,
) -> Result<ConversionSummary, ConvertError> {
    convert_inner(input, output, format, None, true, true, None, vbr_mode)
        .map(|(summary, _, _)| summary)
}

/// Convert one FLAC file to ADTS or M4A with explicit metadata control.
///
/// Behaves like [`convert_file_with_format`], except `write_tags` selects
/// whether M4A output carries user metadata atoms (`true`) or gapless
/// signalling plus the `©too` encoder identification only (`false`, the
/// `--no-tags` CLI behavior). The flag is a no-op for ADTS output, which
/// never carries metadata. The returned [`TagDiagnostics`] report mapped
/// atoms, unmapped keys, and warnings for the converted file; warnings never
/// change the success outcome.
///
/// Single-file M4A conversions with tags run the loudness pre-pass for the
/// degenerate one-member album before converting (no second publication).
pub fn convert_file_with_format_and_tags(
    input: &Path,
    output: &Path,
    format: OutputFormat,
    write_tags: bool,
) -> Result<(ConversionSummary, TagDiagnostics), ConvertError> {
    convert_file_with_format_and_tags_vbr(input, output, format, write_tags, VbrMode::Vbr6)
}

/// Convert one FLAC file with explicit metadata control and VBR profile.
///
/// Behaves like [`convert_file_with_format_and_tags`], except `vbr_mode`
/// selects VBR-1 through VBR-6.
pub fn convert_file_with_format_and_tags_vbr(
    input: &Path,
    output: &Path,
    format: OutputFormat,
    write_tags: bool,
    vbr_mode: VbrMode,
) -> Result<(ConversionSummary, TagDiagnostics), ConvertError> {
    let loudness = if matches!(format, OutputFormat::M4a) && write_tags {
        Some(loudness::analyze_single(input))
    } else {
        None
    };
    convert_inner(
        input,
        output,
        format,
        None,
        write_tags,
        true,
        loudness.as_ref(),
        vbr_mode,
    )
    .map(|(summary, _, diagnostics)| (summary, diagnostics))
}

/// Convert one FLAC file with explicit user-metadata and cover-art control.
///
/// Behaves like [`convert_file_with_format_and_tags`], except
/// [`CoverArtMode`] selects whether selected artwork is embedded, written
/// externally as `cover.jpg`/`cover.png`, or both. External publication uses
/// the same atomic no-overwrite path as audio output; an existing cover file
/// is left untouched. Artwork publication problems are returned as reports
/// and never turn a successful audio conversion into an error.
///
/// Cover-art modes apply only to M4A output with tags enabled. ADTS output and
/// `write_tags == false` (`--no-tags`) never embed or extract artwork.
pub fn convert_file_with_format_tags_and_cover_art(
    input: &Path,
    output: &Path,
    format: OutputFormat,
    write_tags: bool,
    cover_art: CoverArtMode,
) -> Result<(ConversionSummary, TagDiagnostics, Vec<CoverArtReport>), ConvertError> {
    convert_file_with_format_tags_and_cover_art_vbr(
        input,
        output,
        format,
        write_tags,
        cover_art,
        VbrMode::Vbr6,
    )
}

/// Convert one FLAC file with explicit cover-art control and VBR profile.
///
/// Behaves like [`convert_file_with_format_tags_and_cover_art`], except
/// `vbr_mode` selects VBR-1 through VBR-6.
#[allow(clippy::too_many_arguments)]
pub fn convert_file_with_format_tags_and_cover_art_vbr(
    input: &Path,
    output: &Path,
    format: OutputFormat,
    write_tags: bool,
    cover_art: CoverArtMode,
    vbr_mode: VbrMode,
) -> Result<(ConversionSummary, TagDiagnostics, Vec<CoverArtReport>), ConvertError> {
    let interesting = matches!(format, OutputFormat::M4a) && write_tags;
    let extract = interesting && cover_art.extracts();
    // Pre-read only when extraction is requested. Text diagnostics still come
    // from the conversion's own metadata pass, so cover warnings are not
    // duplicated.
    let selected = if extract {
        meta::read_cover(input).ok().and_then(|(cover, _)| cover)
    } else {
        None
    };
    let loudness = if interesting {
        Some(loudness::analyze_single(input))
    } else {
        None
    };
    let (summary, _, diagnostics) = convert_inner(
        input,
        output,
        format,
        None,
        write_tags,
        interesting && cover_art.embeds(),
        loudness.as_ref(),
        vbr_mode,
    )?;
    let reports = match selected {
        Some(cover) => vec![publish_single_external_cover(output, &cover)],
        None => Vec::new(),
    };
    Ok((summary, diagnostics, reports))
}

/// Convert one FLAC file with explicit metadata control plus precomputed
/// loudness atoms (batch loudness pre-pass).
///
/// Behaves like [`convert_file_with_format_and_tags`], except the loudness
/// atoms and warnings are supplied by the caller (album pre-pass) instead of
/// being analyzed for a degenerate one-member album. `None` writes no
/// loudness atoms; M4A conversions with tags and `None` behave exactly like
/// the pre-loudness converter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn convert_file_with_format_tags_and_loudness(
    input: &Path,
    output: &Path,
    format: OutputFormat,
    write_tags: bool,
    embed_cover: bool,
    loudness: Option<&loudness::FileLoudness>,
    vbr_mode: VbrMode,
) -> Result<(ConversionSummary, TagDiagnostics), ConvertError> {
    if matches!(format, OutputFormat::M4a) && write_tags {
        debug_assert!(
            loudness.is_some(),
            "batch M4A conversions with tags always carry pre-pass loudness"
        );
    }
    convert_inner(
        input,
        output,
        format,
        None,
        write_tags,
        embed_cover,
        loudness,
        vbr_mode,
    )
    .map(|(summary, _, diagnostics)| (summary, diagnostics))
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
///
/// Per-file warnings are not returned by this function; use
/// [`convert_file_with_format_and_tags`] to inspect them.
pub fn convert_file_m4a_experiment(
    input: &Path,
    output: &Path,
    experiment: GaplessExperiment,
) -> Result<ConversionSummary, ConvertError> {
    let loudness = Some(loudness::analyze_single(input));
    convert_inner(
        input,
        output,
        OutputFormat::M4a,
        Some(experiment),
        true,
        true,
        loudness.as_ref(),
        VbrMode::Vbr6,
    )
    .map(|(summary, _, _)| summary)
}

#[allow(clippy::too_many_arguments)]
fn convert_inner(
    input: &Path,
    output: &Path,
    format: OutputFormat,
    experiment: Option<GaplessExperiment>,
    write_tags: bool,
    embed_cover: bool,
    loudness: Option<&loudness::FileLoudness>,
    vbr_mode: VbrMode,
) -> Result<(ConversionSummary, u64, TagDiagnostics), ConvertError> {
    let paths = output::resolve_paths(input, output)?;
    let mut source = flac::FlacSource::open(&paths.input)?;
    let properties = source.properties();
    properties.validate()?;

    let config = EncoderConfig::new_with_mode(
        properties.sample_rate,
        properties.channels,
        FRAME_LENGTH,
        TransportType::Adts,
        vbr_mode,
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
    // User metadata is read after integrity verification so corrupt or
    // tagless inputs keep today's failure semantics, and tag errors can
    // never resurrect a failed decode. ADTS never reads tags.
    let (tag_ilst, mut diagnostics) = match (is_m4a, write_tags) {
        (false, _) => (Vec::new(), TagDiagnostics::default()),
        (true, true) => {
            let mut set = meta::read_tags(&paths.input, true)?;
            let diagnostics = TagDiagnostics {
                mapped: set.mapped_count(),
                unmapped: set.unmapped.clone(),
                warnings: set.warnings.clone(),
            };
            // External-only mode still selects artwork so invalid/unsupported
            // candidates warn, but the selected image is not serialized into
            // the M4A.
            if !embed_cover {
                set.cover = None;
            }
            (set.ilst_children()?, diagnostics)
        }
        (true, false) => {
            // `--no-tags`: report what was seen but write nothing and warn
            // about nothing. Cover art is not selected either.
            let set = meta::read_tags(&paths.input, false).unwrap_or_default();
            (
                Vec::new(),
                TagDiagnostics {
                    mapped: 0,
                    unmapped: set.unmapped,
                    warnings: Vec::new(),
                },
            )
        }
    };
    // Precomputed loudness atoms (batch album pre-pass or the degenerate
    // one-member single-file pass) extend the tag list after text atoms and
    // cover art (L2 ilst order). Loudness warnings follow tag/cover warnings.
    // ADTS and `--no-tags` conversions never carry loudness.
    let mut tag_ilst = tag_ilst;
    // Every M4A file identifies its encoder. This is deliberately independent
    // of `write_tags`: `--no-tags` omits user metadata, not the identification
    // of the generated file. It is inserted before user atoms so it cannot be
    // displaced by any source-derived metadata, and ADTS writes none.
    if is_m4a {
        tag_ilst.insert(0, meta::encoder_identification_atom()?);
    }
    if let Some(precomputed) = loudness {
        debug_assert!(
            is_m4a && write_tags,
            "loudness atoms are M4A-with-tags only"
        );
        tag_ilst.extend(precomputed.atoms.iter().cloned());
        diagnostics
            .warnings
            .extend(precomputed.warnings.iter().cloned());
    }
    // 24-bit sources reach the encoder as 16-bit PCM via the decode path's
    // truncation. One notice per converted file, up front, in single-file
    // and batch runs alike (`convert_inner` runs once per file, never per
    // PCM chunk); dry runs never reach here. This is not a tag warning, so
    // `--no-tags` does not suppress it, and ADTS output carries it too.
    if properties.bits_per_sample == 24 {
        diagnostics.warnings.insert(
            0,
            "24-bit FLAC input is truncated to 16-bit PCM (arithmetic right shift, no \
             dithering) before AAC encoding; low-order source precision is discarded"
                .to_string(),
        );
    }
    let output_bytes = sink.finish(gapless, &tag_ilst)?;
    sink.publish(&paths.output)?;
    Ok((
        ConversionSummary {
            properties,
            encoded_frames,
            output_bytes,
        },
        source_frames,
        diagnostics,
    ))
}

/// Publish one selected cover image next to an already-published audio file.
///
/// Publication is non-fatal by design: the audio output is already complete,
/// so an existing or unwritable cover file becomes a report, not an error.
fn publish_single_external_cover(output: &Path, cover: &meta::CoverArt) -> CoverArtReport {
    let resolved = output
        .canonicalize()
        .unwrap_or_else(|_| output.to_path_buf());
    let directory = resolved
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let file_name = cover.file_name();
    let action = publish_cover_file(&directory.join(&file_name), &cover.data);
    CoverArtReport {
        directory,
        file_name: Some(file_name),
        action,
    }
}

/// Atomically publish a cover file without ever replacing an existing path.
pub(crate) fn publish_cover_file(destination: &Path, bytes: &[u8]) -> CoverArtAction {
    match fs::symlink_metadata(destination) {
        Ok(_) => CoverArtAction::Existing,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            match output::publish_bytes(destination, bytes) {
                Ok(()) => CoverArtAction::Written,
                // A concurrent writer won between the check and the link.
                Err(ConvertError::OutputExists(_)) => CoverArtAction::Existing,
                Err(err) => CoverArtAction::Failed {
                    message: err.to_string(),
                },
            }
        }
        Err(err) => CoverArtAction::Failed {
            message: format!("could not inspect cover destination: {err}"),
        },
    }
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
    /// `tag_ilst` holds complete `ilst` child boxes appended after the
    /// `iTunSMPB` item; it is empty for ADTS, untagged, and `--no-tags`
    /// conversions. M4A output without accounting is a broken internal
    /// invariant and fails closed instead of writing unsignalled output.
    fn finish(
        &mut self,
        gapless: Option<(gapless::GaplessAccounting, bool)>,
        tag_ilst: &[Vec<u8>],
    ) -> Result<u64, ConvertError> {
        match self {
            Self::Adts { written, .. } => Ok(*written),
            Self::M4a(writer) => match gapless {
                Some((accounting, edit_list)) => writer.finish(&accounting, edit_list, tag_ilst),
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
    fn encoder_error_display_includes_frame_and_reason() {
        use ferrite_aac::aac_enc::encoder::EncodeFailure;
        use std::error::Error as _;

        let error = ConvertError::Encoder(EncodeError::FrameFailed {
            frame_index: 1496,
            completed_before: 0,
            kind: EncodeFailure::SkippedFrame {
                notes: vec!["frontend mismatch".to_string()],
            },
        });
        let text = error.to_string();
        assert!(text.contains("AAC encoding failed"), "{text}");
        assert!(text.contains("frame 1496"), "{text}");
        assert!(text.contains("frame skipped"), "{text}");
        assert!(text.contains("frontend mismatch"), "{text}");
        assert!(text.contains("no output file was published"), "{text}");
        assert!(
            error
                .source()
                .expect("encoder error source")
                .to_string()
                .contains("frame 1496"),
            "source chain must preserve the frame error"
        );
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

    #[test]
    fn validate_accepts_only_16_and_24_bit() {
        for bits in [16u32, 24] {
            AudioProperties {
                sample_rate: 48_000,
                channels: 2,
                bits_per_sample: bits,
            }
            .validate()
            .unwrap();
        }
        // Focused rejection coverage without binary fixtures: 8/12/20/32-bit
        // FLAC reaches the same gate and must keep failing closed.
        for bits in [8u32, 12, 20, 32] {
            match (AudioProperties {
                sample_rate: 48_000,
                channels: 2,
                bits_per_sample: bits,
            })
            .validate()
            {
                Err(ConvertError::UnsupportedBitDepth(got)) => assert_eq!(got, bits),
                other => panic!("bits={bits}: expected UnsupportedBitDepth, got {other:?}"),
            }
        }
    }
}
