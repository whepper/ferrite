//! Audio-only MP4/M4A muxing for the converter.
//!
//! The native encoder still produces ADTS frames; this module consumes each
//! frame's validated raw access-unit payload (`AdtsFrame::raw_payload`) and
//! writes it through `shiguredo_mp4`'s streaming `Mp4FileMuxer`. The muxer
//! keeps only per-sample metadata, so the complete AAC payload is never held
//! in memory.
//!
//! The stream carries `iTunSMPB` gapless signalling with the Apple-style
//! `M4A` brand; see the README for the measured timing behavior and the
//! compatibility evidence. An experimental opt-in ([`M4aWriter::finish`]
//! `edit_list` switch) can additionally append `edts`/`elst` without touching
//! the AAC payloads; it makes no gapless playback claim.

use std::num::NonZeroU16;
use std::num::NonZeroU32;
use std::path::Path;

use aac::aac_enc::config::EncoderConfig;
use aac::aac_enc::encoder::{AdtsError, AdtsFrame};
use shiguredo_mp4::boxes::{AudioSampleEntryFields, EsdsBox, Mp4aBox, SampleEntry};
use shiguredo_mp4::descriptors::{
    DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
};
use shiguredo_mp4::mux::{Mp4FileMuxer, MuxError, Sample};
use shiguredo_mp4::{Encode, FixedPointNumber, TrackKind, Uint};

use crate::gapless::GaplessAccounting;
use crate::gapless::{DEFAULT_FTYP_BYTES, EXPERIMENTAL_FTYP_BYTES};
use crate::output::TempOutput;
use crate::{AudioProperties, ConvertError};

/// Streaming M4A writer over one temporary output file.
pub(crate) struct M4aWriter {
    output: TempOutput,
    muxer: Mp4FileMuxer,
    /// Byte position where the next sample payload must be written.
    offset: u64,
    timescale: NonZeroU32,
    frame_length: u32,
    sample_entry: SampleEntry,
    sample_count: u64,
}

impl M4aWriter {
    pub(crate) fn create(
        output_path: &Path,
        properties: AudioProperties,
        config: EncoderConfig,
    ) -> Result<Self, ConvertError> {
        let asc = config
            .audio_specific_config()
            .map_err(ConvertError::AudioSpecificConfig)?;
        let sample_entry = mp4a_sample_entry(properties, asc);
        let muxer = Mp4FileMuxer::new().map_err(ConvertError::Muxer)?;
        let mut output = TempOutput::create(output_path)?;
        output.write_all(muxer.initial_boxes_bytes())?;
        let offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale =
            NonZeroU32::new(properties.sample_rate).expect("validated sample rate is nonzero");

        Ok(Self {
            output,
            muxer,
            offset,
            timescale,
            frame_length: config.frame_length() as u32,
            sample_entry,
            sample_count: 0,
        })
    }

    /// Append one validated AAC access unit.
    pub(crate) fn write_frame(
        &mut self,
        frame: &AdtsFrame,
        expected_index: u64,
    ) -> Result<(), ConvertError> {
        if u64::from(frame.frame_index) != expected_index {
            return Err(ConvertError::AdtsInvalid("non-sequential frame index"));
        }
        let payload = match frame.raw_payload() {
            Ok(payload) => payload,
            Err(AdtsError::MalformedFrame(reason)) => {
                return Err(ConvertError::AdtsInvalid(reason));
            }
            Err(_) => return Err(ConvertError::AdtsInvalid("unusable ADTS payload")),
        };

        self.output.write_all(payload)?;
        self.muxer
            .append_sample(&Sample {
                track_kind: TrackKind::Audio,
                // Only the first sample needs the entry; later ones inherit it.
                sample_entry: (self.sample_count == 0).then(|| self.sample_entry.clone()),
                keyframe: true,
                timescale: self.timescale,
                duration: self.frame_length,
                composition_time_offset: None,
                data_offset: self.offset,
                data_size: payload.len(),
            })
            .map_err(ConvertError::Muxer)?;
        self.offset += payload.len() as u64;
        self.sample_count += 1;
        Ok(())
    }

    /// Finalize the container with `iTunSMPB` gapless signalling plus optional
    /// user metadata, and return the final file size.
    ///
    /// The encoded AAC samples are preserved untouched: only the `moov`
    /// metadata is re-encoded, appending the `iTunSMPB` `udta` item (and an
    /// `edts`/`elst` entry when the experimental `edit_list` switch is set)
    /// and rewriting the `ftyp` brand to the Apple-style `M4A` set CoreAudio
    /// requires. `extra_ilst` holds complete `ilst` child boxes (M4A text
    /// atoms) appended after the `iTunSMPB` item; an empty slice writes
    /// gapless signalling only. The rewritten `moov` must be the final box
    /// so growth only extends the file; anything else fails without publishing.
    pub(crate) fn finish(
        &mut self,
        accounting: &GaplessAccounting,
        edit_list: bool,
        extra_ilst: &[Vec<u8>],
    ) -> Result<u64, ConvertError> {
        let finalized = self.muxer.finalize().map_err(ConvertError::Muxer)?;
        let mut moov_offset = None;
        let mut moov_len = 0u64;
        for (offset, bytes) in finalized.offset_and_bytes_pairs() {
            if bytes.get(4..8) == Some(b"moov") {
                moov_offset = Some(offset);
                moov_len = bytes.len() as u64;
            }
            self.output.seek(std::io::SeekFrom::Start(offset))?;
            self.output.write_all(bytes)?;
        }
        let moov_offset =
            moov_offset
                .filter(|_| moov_len > 0)
                .ok_or(ConvertError::GaplessSignalling(
                    "finalized container has no moov box",
                ))?;
        let end = self.output.seek(std::io::SeekFrom::End(0))?;
        if moov_offset.checked_add(moov_len) != Some(end) {
            return Err(ConvertError::GaplessSignalling(
                "moov box is not the final box",
            ));
        }

        let mut moov = finalized.moov_box().clone();
        if edit_list {
            let track = moov
                .trak_boxes
                .first_mut()
                .ok_or(ConvertError::GaplessSignalling(
                    "container has no audio track",
                ))?;
            track.edts_box = Some(
                accounting
                    .edit_list()
                    .map_err(|err| ConvertError::GaplessSignalling(err.message()))?,
            );
        }
        moov.unknown_boxes.push(
            accounting
                .udta_box_with_tags(extra_ilst)
                .map_err(|err| ConvertError::GaplessSignalling(err.message()))?,
        );
        let rewritten = moov
            .encode_to_vec()
            .map_err(MuxError::EncodeError)
            .map_err(ConvertError::Muxer)?;
        let new_end = moov_offset.checked_add(rewritten.len() as u64).ok_or(
            ConvertError::GaplessSignalling("rewritten moov box overflows the file size"),
        )?;
        self.output.seek(std::io::SeekFrom::Start(moov_offset))?;
        self.output.write_all(&rewritten)?;
        self.output.set_len(new_end)?;

        // `iTunSMPB` additionally requires an `M4A` brand: CoreAudio ignores
        // the metadata under the muxer's default `isom` brands. The
        // replacement is the same 28 bytes, so no offset changes.
        self.output.seek(std::io::SeekFrom::Start(0))?;
        let mut ftyp = [0u8; 28];
        self.output.read_exact(&mut ftyp)?;
        if ftyp != DEFAULT_FTYP_BYTES {
            return Err(ConvertError::GaplessSignalling(
                "unexpected ftyp layout for brand splice",
            ));
        }
        self.output.seek(std::io::SeekFrom::Start(0))?;
        self.output.write_all(&EXPERIMENTAL_FTYP_BYTES)?;

        self.output.seek(std::io::SeekFrom::End(0))
    }

    pub(crate) fn publish(self, output: &Path) -> Result<(), ConvertError> {
        self.output.publish(output)
    }
}

/// Build the `mp4a` sample entry and its `esds`/AudioSpecificConfig.
fn mp4a_sample_entry(properties: AudioProperties, asc: [u8; 2]) -> SampleEntry {
    let dec_specific = DecoderSpecificInfo {
        payload: asc.to_vec(),
    };
    let dec_config = DecoderConfigDescriptor {
        object_type_indication:
            DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3,
        stream_type: DecoderConfigDescriptor::STREAM_TYPE_AUDIO,
        up_stream: DecoderConfigDescriptor::UP_STREAM_FALSE,
        buffer_size_db: Uint::<u32, 24>::new(0),
        // VBR bitrates are not declared by the encoder API; 0 means unknown.
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
            channelcount: properties.channels as u16,
            samplesize: 16,
            samplerate: FixedPointNumber::new(properties.sample_rate as u16, 0u16),
        },
        esds_box: EsdsBox { es },
        unknown_boxes: Vec::new(),
    })
}
