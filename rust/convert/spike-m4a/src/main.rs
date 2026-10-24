//! M7.5 M4A feasibility spike.
//!
//! FLAC -> native AAC-LC VBR-5 encoder -> audio-only M4A with `shiguredo_mp4`.
//! This binary is a throwaway experiment, not production code. It reuses the
//! same decode and encode path as `aac-convert`; the only new part is container
//! writing.
//!
//! Usage: `m4a-spike <input.flac> <output.m4a> [--elst]`
//!
//! `--elst` post-processes the finalized `moov` to add an edit list that trims
//! the encoder priming (and the tail padding) instead of writing it raw.

use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::num::NonZeroU16;
use std::num::NonZeroU32;
use std::path::Path;

use ferrite_aac::aac_enc::config::{EncoderConfig, TransportType, AOT_AAC_LC};
use ferrite_aac::aac_enc::encoder::{AdtsFrame, Encoder};
use shiguredo_mp4::boxes::{
    AudioSampleEntryFields, EdtsBox, ElstBox, ElstEntry, EsdsBox, Mp4aBox, SampleEntry,
};
use shiguredo_mp4::descriptors::{
    DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
};
use shiguredo_mp4::mux::{Mp4FileMuxer, Sample};
use shiguredo_mp4::{Encode, FixedPointNumber, TrackKind, Uint};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// Encoder pushes are capped; decoded FLAC packets are staged separately.
const PUSH_FRAMES: usize = 8_000;
const ADTS_HEADER_LEN: usize = 7;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let input = args
        .next()
        .ok_or("usage: m4a-spike <in.flac> <out.m4a> [--elst]")?;
    let output = args
        .next()
        .ok_or("usage: m4a-spike <in.flac> <out.m4a> [--elst]")?;
    let write_elst = match args.next().as_deref() {
        None => false,
        Some("--elst") => true,
        Some(other) => return Err(format!("unknown option: {other}").into()),
    };

    let source = decode_source(Path::new(&input))?;
    let config = EncoderConfig::new(
        source.sample_rate,
        source.channels as u32,
        1024,
        TransportType::Adts,
    )?;
    let delay = config.encoder_delay() as u64;
    let mut encoder = Encoder::new(config)?;
    let mut writer = M4aWriter::create(
        Path::new(&output),
        source.sample_rate,
        source.channels as u16,
        config.channel_mode().channel_config() as u8,
    )?;

    let mut format = source.format;
    let mut decoder = source.decoder;
    let track_id = source.track_id;
    let mut pcm = Vec::new();
    let mut source_frames = 0u64;

    while let Some(packet) = format
        .next_packet()
        .map_err(|err| format!("FLAC packet error: {err}"))?
    {
        if packet.track_id != track_id {
            continue;
        }
        let decoded = decoder
            .decode(&packet)
            .map_err(|err| format!("FLAC decode error: {err}"))?;
        decoded.copy_to_vec_interleaved::<i16>(&mut pcm);
        let frames = pcm.len() / source.channels;
        source_frames += frames as u64;

        for chunk in pcm.chunks(PUSH_FRAMES * source.channels) {
            let encoded = encoder.push(chunk)?;
            writer.write_frames(&encoded)?;
        }
    }

    let tail = encoder.flush()?;
    writer.write_frames(&tail)?;
    if decoder.finalize().verify_ok == Some(false) {
        return Err("FLAC checksum mismatch".into());
    }

    let encoded_frames = writer.sample_count;
    let media_frames = encoded_frames * 1024;
    if media_frames < source_frames + delay {
        return Err(format!(
            "encoder produced {media_frames} frames, fewer than source {source_frames} + delay {delay}"
        )
        .into());
    }
    let trailing_padding = media_frames - source_frames - delay;

    let moov_offset = writer.finish(write_elst.then_some((source_frames, delay)))?;

    println!("input: {input}");
    println!("output: {output}");
    println!("sample_rate: {}", source.sample_rate);
    println!("channels: {}", source.channels);
    println!("source_frames: {source_frames}");
    println!("encoded_frames: {encoded_frames} ({media_frames} samples)");
    println!("encoder_delay: {delay}");
    println!("trailing_padding: {trailing_padding}");
    println!("m4a_bytes: {}", writer.bytes_written);
    println!(
        "audio_specific_config: {:02X?}",
        audio_specific_config(
            source.sample_rate,
            config.channel_mode().channel_config() as u8
        )
    );
    println!(
        "edit_list: {}",
        if write_elst {
            format!("media_time={delay}, edit_duration={source_frames}")
        } else {
            "none".to_string()
        }
    );
    println!("moov_offset: {moov_offset}");
    Ok(())
}

struct FlacSource {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    sample_rate: u32,
    channels: usize,
}

/// Minimal copy of the converter's Symphonia setup (the production module is
/// private to `aac-convert`).
fn decode_source(path: &Path) -> Result<FlacSource, Box<dyn Error>> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("flac");
    let format = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;

    let (track_id, sample_rate, channels, decoder) = {
        let track = format
            .default_track(TrackType::Audio)
            .ok_or("no audio track")?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .ok_or("no audio parameters")?;
        let sample_rate = params.sample_rate.ok_or("no sample rate")?;
        let channels = params
            .channels
            .as_ref()
            .map(|channels| channels.count())
            .ok_or("no channel layout")?;
        if params.bits_per_sample != Some(16) {
            return Err("spike only accepts 16-bit FLAC".into());
        }
        let mut options = AudioDecoderOptions::default();
        options.verify = true;
        let decoder = symphonia::default::get_codecs().make_audio_decoder(params, &options)?;
        (track.id, sample_rate, channels, decoder)
    };

    Ok(FlacSource {
        format,
        decoder,
        track_id,
        sample_rate,
        channels,
    })
}

/// Two-byte AudioSpecificConfig for AAC-LC with no explicit frame length and
/// no core coder / extension flags.
fn audio_specific_config(sample_rate: u32, channel_config: u8) -> [u8; 2] {
    // The encoder's sampling-frequency index table is private, so the spike
    // spells out the two rates the encoder currently accepts. A production
    // implementation should expose this index (or the finished ASC) instead
    // of duplicating it.
    let sfi = match sample_rate {
        48_000 => 3,
        44_100 => 4,
        other => panic!("unsupported encoder rate: {other}"),
    };
    let aot = AOT_AAC_LC as u8;
    [
        (aot << 3) | (sfi >> 1),
        ((sfi & 1) << 7) | (channel_config << 3),
    ]
}

fn mp4a_sample_entry(sample_rate: u32, channels: u16, asc: [u8; 2]) -> SampleEntry {
    let dec_specific = DecoderSpecificInfo {
        payload: asc.to_vec(),
    };
    let dec_config = DecoderConfigDescriptor {
        object_type_indication:
            DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3,
        stream_type: DecoderConfigDescriptor::STREAM_TYPE_AUDIO,
        up_stream: DecoderConfigDescriptor::UP_STREAM_FALSE,
        buffer_size_db: Uint::<u32, 24>::new(0),
        max_bitrate: 0,
        avg_bitrate: 0,
        dec_specific_info: Some(dec_specific),
    };
    let es = EsDescriptor {
        es_id: EsDescriptor::MIN_ES_ID,
        stream_priority: EsDescriptor::LOWEST_STREAM_PRIORITY,
        depends_on_es_id: None,
        url_string: None,
        ocr_es_id: None,
        dec_config_descr: dec_config,
        sl_config_descr: SlConfigDescriptor,
    };

    SampleEntry::Mp4a(Mp4aBox {
        audio: AudioSampleEntryFields {
            data_reference_index: NonZeroU16::MIN,
            channelcount: channels,
            samplesize: 16,
            samplerate: FixedPointNumber::new(sample_rate as u16, 0u16),
        },
        esds_box: EsdsBox { es },
        unknown_boxes: Vec::new(),
    })
}

struct M4aWriter {
    file: File,
    muxer: Mp4FileMuxer,
    offset: u64,
    timescale: NonZeroU32,
    sample_entry: SampleEntry,
    sample_count: u64,
    bytes_written: u64,
}

impl M4aWriter {
    fn create(
        path: &Path,
        sample_rate: u32,
        channels: u16,
        channel_config: u8,
    ) -> Result<Self, Box<dyn Error>> {
        let muxer = Mp4FileMuxer::new()?;
        let mut file = File::create(path)?;
        file.write_all(muxer.initial_boxes_bytes())?;
        let offset = muxer.initial_boxes_bytes().len() as u64;

        Ok(Self {
            file,
            muxer,
            offset,
            timescale: NonZeroU32::new(sample_rate).ok_or("zero sample rate")?,
            sample_entry: mp4a_sample_entry(
                sample_rate,
                channels,
                audio_specific_config(sample_rate, channel_config),
            ),
            sample_count: 0,
            bytes_written: offset,
        })
    }

    fn write_frames(&mut self, frames: &[AdtsFrame]) -> Result<(), Box<dyn Error>> {
        for frame in frames {
            let raw = raw_payload(frame)?;
            self.file.write_all(raw)?;
            self.muxer.append_sample(&Sample {
                track_kind: TrackKind::Audio,
                // Only the first sample needs the entry; later ones inherit it.
                sample_entry: (self.sample_count == 0).then(|| self.sample_entry.clone()),
                keyframe: true,
                timescale: self.timescale,
                duration: 1024,
                composition_time_offset: None,
                data_offset: self.offset,
                data_size: raw.len(),
            })?;
            self.offset += raw.len() as u64;
            self.bytes_written += raw.len() as u64;
            self.sample_count += 1;
        }
        Ok(())
    }

    /// Write the finalized boxes and optionally rewrite `moov` with an edit
    /// list trimming the encoder priming and tail padding.
    fn finish(&mut self, elst: Option<(u64, u64)>) -> Result<u64, Box<dyn Error>> {
        let finalized = self.muxer.finalize()?;
        let mut moov_offset = None;
        for (offset, bytes) in finalized.offset_and_bytes_pairs() {
            if bytes.get(4..8) == Some(b"moov") {
                moov_offset = Some(offset);
            }
            self.file.seek(SeekFrom::Start(offset))?;
            self.file.write_all(bytes)?;
        }
        self.file.seek(SeekFrom::End(0))?;
        self.bytes_written = self.file.stream_position()?;

        if let Some((source_frames, delay)) = elst {
            let mut moov = finalized.moov_box().clone();
            let trak = moov.trak_boxes.first_mut().ok_or("no trak in moov")?;
            trak.edts_box = Some(EdtsBox {
                elst_box: Some(ElstBox {
                    entries: vec![ElstEntry {
                        edit_duration: source_frames,
                        media_time: delay as i64,
                        media_rate: FixedPointNumber::new(1i16, 0i16),
                    }],
                }),
                unknown_boxes: Vec::new(),
            });
            let bytes = moov.encode_to_vec()?;
            let offset = moov_offset.ok_or("moov written at offset zero is unsupported")?;
            self.file.seek(SeekFrom::Start(offset))?;
            self.file.write_all(&bytes)?;
            self.file.set_len(offset + bytes.len() as u64)?;
            self.bytes_written = offset + bytes.len() as u64;
        }

        self.file.sync_all()?;
        Ok(moov_offset.unwrap_or(0))
    }
}

/// Strip the 7-byte ADTS header after re-validating the fields the container
/// depends on. The encoder emits header bytes verbatim followed by the raw
/// block, so this is a structural split, not a guess.
fn raw_payload(frame: &AdtsFrame) -> Result<&[u8], Box<dyn Error>> {
    let bytes = &frame.bytes;
    if bytes.len() <= ADTS_HEADER_LEN {
        return Err("ADTS frame too short".into());
    }
    if bytes[0] != 0xFF || bytes[1] & 0xF0 != 0xF0 {
        return Err("bad ADTS syncword".into());
    }
    let declared = ((usize::from(bytes[3] & 0x03)) << 11)
        | (usize::from(bytes[4]) << 3)
        | (bytes[5] >> 5) as usize;
    if declared != bytes.len() {
        return Err("ADTS frame length mismatch".into());
    }
    if bytes[6] & 0x03 != 0 {
        return Err("multiple raw data blocks".into());
    }
    Ok(&bytes[ADTS_HEADER_LEN..])
}
