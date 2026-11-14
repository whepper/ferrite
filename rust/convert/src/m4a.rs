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
//!
//! Fast-start layout (`moov` before `mdat`) is produced by default via a
//! guaranteed second pass that relocates a finalized non-fast-start file
//! using the existing MP4 library (no new dependency). The muxer is created
//! with an explicit zero `reserved_moov_box_size`: predicting the final
//! `moov` size upfront would require the sample count (unknown until encoding
//! completes) plus the final `udta` size (text tags, loudness atoms, and cover
//! art up to 16 MiB, only known after decoding and tagging). A fixed large
//! reservation would write megabytes of zero-filled head for every file and
//! still cannot guarantee fit for arbitrarily long inputs, while the
//! post-finalize `udta` growth (appended after [`Mp4FileMuxer::finalize`])
//! can overflow any reservation and the muxer then silently falls back to a
//! trailing `moov` (detectable via `is_faststart_enabled`, but not preventable
//! upfront). The second pass therefore always ensures `moov` precedes `mdat`
//! and patches `stco`/`co64` chunk offsets, handling any artwork size without
//! upfront cost. Audio payloads, timestamps, and metadata are preserved; only
//! box order and chunk offsets change.

use std::num::NonZeroU16;
use std::num::NonZeroU32;
use std::path::Path;

use ferrite_aac::aac_enc::config::EncoderConfig;
use ferrite_aac::aac_enc::encoder::{AdtsError, AdtsFrame};
use shiguredo_mp4::boxes::{AudioSampleEntryFields, EsdsBox, MoovBox, Mp4aBox, SampleEntry};
use shiguredo_mp4::descriptors::{
    DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
};
use shiguredo_mp4::mux::{Mp4FileMuxer, Mp4FileMuxerOptions, MuxError, Sample};
use shiguredo_mp4::{Decode, Either, Encode, FixedPointNumber, TrackKind, Uint};

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
        // Explicit zero reservation (no upfront zero-filled head): the final
        // `moov` size depends on the sample count plus the post-finalize
        // `udta` (tags, loudness, cover up to 16 MiB) and cannot be sized
        // reliably before streaming. The muxer therefore finalizes to a
        // trailing `moov`, and `finish` relocates it before `mdat` to guarantee
        // fast-start for any artwork size. `is_faststart_enabled` is expected
        // to be false here; the final box-order check after relocation is the
        // guarantee, not the reservation.
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size: 0,
            ..Default::default()
        };
        let muxer = Mp4FileMuxer::with_options(options).map_err(ConvertError::Muxer)?;
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
    /// atoms, starting with the caller's encoder identification) appended
    /// after the `iTunSMPB` item; an empty slice writes gapless signalling
    /// only. The rewritten `moov` must be the final box so growth only
    /// extends the file; anything else fails without publishing.
    ///
    /// The file is then relocated to fast-start layout (`ftyp`, `moov`,
    /// `mdat`) by moving `moov` before `mdat` and patching `stco`/`co64`
    /// chunk offsets with the existing MP4 library. The returned size is the
    /// post-relocation file size.
    pub(crate) fn finish(
        &mut self,
        accounting: &GaplessAccounting,
        edit_list: bool,
        extra_ilst: &[Vec<u8>],
    ) -> Result<u64, ConvertError> {
        let finalized = self.muxer.finalize().map_err(ConvertError::Muxer)?;
        // Zero reservation must finalize to a trailing `moov`. If the muxer
        // ever fast-starts here (only possible for a tiny `moov` that fits
        // the 100-byte head), the trailing-`moov` scan below would misread
        // the head box, so fail with an explicit error instead of publishing
        // an unverified layout.
        if finalized.is_faststart_enabled() {
            return Err(ConvertError::GaplessSignalling(
                "unexpected fast-start from muxer with zero reservation",
            ));
        }
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

        // With an explicit zero reservation the muxer finalizes to a trailing
        // `moov` (non-fast-start). Relocate it before `mdat` to guarantee
        // progressive-download/streaming layout. The muxer's silent fallback
        // is therefore the expected path here; relocation detects the actual
        // box order by parsing the file (and treats an already fast-start
        // file as a verified no-op), so the guarantee does not depend on the
        // reservation.
        relocate_moov_before_mdat(&mut self.output)?;

        self.output.seek(std::io::SeekFrom::End(0))
    }

    pub(crate) fn publish(self, output: &Path) -> Result<(), ConvertError> {
        self.output.publish(output)
    }
}

/// Top-level box location for relocation parsing.
struct TopBox {
    kind: [u8; 4],
    start: u64,
    end: u64,
    header_len: u64,
}

/// Parse top-level boxes, verifying they tile the file exactly.
fn parse_top_boxes(output: &mut TempOutput, file_len: u64) -> Result<Vec<TopBox>, ConvertError> {
    let mut boxes = Vec::new();
    let mut offset = 0u64;
    while offset < file_len {
        output.seek(std::io::SeekFrom::Start(offset))?;
        let mut header = [0u8; 8];
        output.read_exact(&mut header)?;
        let size32 = u32::from_be_bytes(header[0..4].try_into().expect("slice len 4"));
        let mut kind = [0u8; 4];
        kind.copy_from_slice(&header[4..8]);
        let (size, header_len) = if size32 == 1 {
            let mut large = [0u8; 8];
            output.read_exact(&mut large)?;
            let size = u64::from_be_bytes(large);
            if size < 16 {
                return Err(ConvertError::GaplessSignalling(
                    "invalid large MP4 box size",
                ));
            }
            (size, 16u64)
        } else if size32 == 0 {
            (file_len - offset, 8u64)
        } else {
            let size = u64::from(size32);
            if size < 8 {
                return Err(ConvertError::GaplessSignalling("invalid MP4 box size"));
            }
            (size, 8u64)
        };
        let end = offset
            .checked_add(size)
            .ok_or(ConvertError::GaplessSignalling("MP4 box size overflowed"))?;
        if end > file_len {
            return Err(ConvertError::GaplessSignalling("MP4 box overruns file"));
        }
        boxes.push(TopBox {
            kind,
            start: offset,
            end,
            header_len,
        });
        offset = end;
    }
    if offset != file_len {
        return Err(ConvertError::GaplessSignalling(
            "MP4 boxes do not tile the file",
        ));
    }
    Ok(boxes)
}

/// Add `delta` to every chunk offset in `moov`, promoting any `stco` table
/// that would overflow `u32` to `co64`.
///
/// Returns `true` when at least one table was promoted. All tracks are
/// visited; every entry in each table is shifted, not just the first.
fn apply_chunk_offset_delta(moov: &mut MoovBox, delta: u64) -> Result<bool, ConvertError> {
    let mut promoted_any = false;
    for trak in &mut moov.trak_boxes {
        let stbl = &mut trak.mdia_box.minf_box.stbl_box;
        promoted_any |= shift_chunk_offset_table(&mut stbl.stco_or_co64_box, delta)?;
    }
    Ok(promoted_any)
}

/// Shift one `stco`/`co64` table by `delta`, promoting `stco` to `co64` when
/// any shifted offset would exceed `u32::MAX`. Returns `true` on promotion.
fn shift_chunk_offset_table(
    table: &mut Either<shiguredo_mp4::boxes::StcoBox, shiguredo_mp4::boxes::Co64Box>,
    delta: u64,
) -> Result<bool, ConvertError> {
    match table {
        Either::A(stco) => {
            let mut promoted: Vec<u64> = Vec::with_capacity(stco.chunk_offsets.len());
            let mut needs_promotion = false;
            for offset in &stco.chunk_offsets {
                let updated = u64::from(*offset)
                    .checked_add(delta)
                    .ok_or(ConvertError::GaplessSignalling("chunk offset overflowed"))?;
                if updated > u64::from(u32::MAX) {
                    needs_promotion = true;
                }
                promoted.push(updated);
            }
            if needs_promotion {
                *table = Either::B(shiguredo_mp4::boxes::Co64Box {
                    chunk_offsets: promoted,
                });
                Ok(true)
            } else {
                for (slot, updated) in stco.chunk_offsets.iter_mut().zip(promoted.iter()) {
                    *slot = *updated as u32;
                }
                Ok(false)
            }
        }
        Either::B(co64) => {
            for offset in &mut co64.chunk_offsets {
                *offset = offset
                    .checked_add(delta)
                    .ok_or(ConvertError::GaplessSignalling("chunk offset overflowed"))?;
            }
            Ok(false)
        }
    }
}

/// Verify every chunk offset points inside the relocated `mdat` payload.
fn verify_chunk_offsets_within_mdat(
    moov: &MoovBox,
    payload_start: u64,
    payload_len: u64,
) -> Result<(), ConvertError> {
    let payload_end =
        payload_start
            .checked_add(payload_len)
            .ok_or(ConvertError::GaplessSignalling(
                "relocated layout overflowed",
            ))?;
    for trak in &moov.trak_boxes {
        let stbl = &trak.mdia_box.minf_box.stbl_box;
        let offsets: Vec<u64> = match &stbl.stco_or_co64_box {
            Either::A(stco) => stco.chunk_offsets.iter().map(|o| u64::from(*o)).collect(),
            Either::B(co64) => co64.chunk_offsets.clone(),
        };
        for offset in offsets {
            if offset < payload_start || offset > payload_end {
                return Err(ConvertError::GaplessSignalling(
                    "relocated chunk offset points outside the mdat payload",
                ));
            }
        }
    }
    Ok(())
}

/// Relocate `moov` before `mdat` for fast-start streaming.
///
/// After [`M4aWriter::finish`] writes gapless signalling and tags, the file is
/// a valid non-fast-start container (`ftyp`, `free`, `mdat`, `moov`). This
/// rewrites it in place to (`ftyp`, `moov`, `mdat`), patching every
/// `stco`/`co64` chunk offset by the payload shift using the existing MP4
/// library (`MoovBox::decode`/`encode`). Audio payloads are moved with a
/// bounded 64 KiB backward copy, so peak memory is the `moov` box (already
/// held for the `udta` rewrite) plus one chunk, never the audio payload.
/// Already fast-start files are a verified no-op.
fn relocate_moov_before_mdat(output: &mut TempOutput) -> Result<(), ConvertError> {
    let file_len = output.seek(std::io::SeekFrom::End(0))?;
    let boxes = parse_top_boxes(output, file_len)?;
    let ftyp = boxes.first().ok_or(ConvertError::GaplessSignalling(
        "finalized container has no boxes",
    ))?;
    if &ftyp.kind != b"ftyp" {
        return Err(ConvertError::GaplessSignalling(
            "finalized container does not start with ftyp",
        ));
    }
    let mdat = boxes
        .iter()
        .find(|b| &b.kind == b"mdat")
        .ok_or(ConvertError::GaplessSignalling(
            "finalized container has no mdat box",
        ))?;
    let moov = boxes
        .iter()
        .find(|b| &b.kind == b"moov")
        .ok_or(ConvertError::GaplessSignalling(
            "finalized container has no moov box",
        ))?;
    if moov.start < mdat.start {
        // Already fast-start: verify tiling already checked above; nothing to move.
        return Ok(());
    }
    // Expected non-fast-start layout: mdat payload immediately followed by moov at end.
    if mdat.end != moov.start || moov.end != file_len {
        return Err(ConvertError::GaplessSignalling(
            "unexpected box order for fast-start relocation",
        ));
    }
    // Only `free` padding may precede `mdat` besides the leading `ftyp`.
    // `free` is discardable padding; any other top-level box would be silently
    // dropped by the rewrite below, so reject it instead.
    let mdat_index = boxes
        .iter()
        .position(|b| &b.kind == b"mdat")
        .expect("mdat found above");
    for pre in &boxes[..mdat_index] {
        if &pre.kind != b"ftyp" && &pre.kind != b"free" {
            return Err(ConvertError::GaplessSignalling(
                "unsupported top-level box before mdat for fast-start relocation",
            ));
        }
    }
    if boxes.get(mdat_index + 1).map(|b| &b.kind) != Some(b"moov") || boxes.len() != mdat_index + 2
    {
        return Err(ConvertError::GaplessSignalling(
            "unexpected box order for fast-start relocation",
        ));
    }

    let ftyp_len = ftyp.end - ftyp.start;
    let old_header_len = mdat.header_len;
    if old_header_len != 8 && old_header_len != 16 {
        return Err(ConvertError::GaplessSignalling(
            "unsupported mdat header size",
        ));
    }
    let old_payload_start = mdat
        .start
        .checked_add(old_header_len)
        .ok_or(ConvertError::GaplessSignalling("mdat header overflowed"))?;
    if old_payload_start > moov.start {
        return Err(ConvertError::GaplessSignalling("mdat payload is empty"));
    }
    let payload_len = moov.start - old_payload_start;
    let moov_len = moov.end - moov.start;
    let moov_len_usize = usize::try_from(moov_len)
        .map_err(|_| ConvertError::GaplessSignalling("moov box too large"))?;

    // Old mdat header bytes are reused at the new location: payload size is
    // unchanged, so the size field stays valid.
    output.seek(std::io::SeekFrom::Start(mdat.start))?;
    let old_header_len_usize = usize::try_from(old_header_len)
        .map_err(|_| ConvertError::GaplessSignalling("mdat header too large"))?;
    let mut header_bytes = vec![0u8; old_header_len_usize];
    output.read_exact(&mut header_bytes)?;

    output.seek(std::io::SeekFrom::Start(moov.start))?;
    let mut moov_bytes = vec![0u8; moov_len_usize];
    output.read_exact(&mut moov_bytes)?;
    let (original_moov, consumed) = MoovBox::decode(&moov_bytes)
        .map_err(|_| ConvertError::GaplessSignalling("could not decode moov box"))?;
    if consumed != moov_bytes.len() {
        return Err(ConvertError::GaplessSignalling(
            "moov box did not decode exactly",
        ));
    }

    // New payload start depends on the re-encoded moov size, which can grow
    // by 4 bytes per chunk if an `stco` must promote to `co64`. Iterate to a
    // fixed point (at most a few rounds; typically one).
    let mut new_moov_len = moov_len;
    let mut new_moov_bytes = Vec::new();
    let mut converged = false;
    for _ in 0..4 {
        let new_payload_start = ftyp_len
            .checked_add(new_moov_len)
            .and_then(|v| v.checked_add(old_header_len))
            .ok_or(ConvertError::GaplessSignalling(
                "relocated layout overflowed",
            ))?;
        let delta = (new_payload_start as i64)
            .checked_sub(old_payload_start as i64)
            .ok_or(ConvertError::GaplessSignalling(
                "relocated layout overflowed",
            ))?;
        if delta < 0 {
            return Err(ConvertError::GaplessSignalling(
                "relocated moov does not fit before mdat",
            ));
        }
        let delta = delta as u64;
        let mut patched = original_moov.clone();
        apply_chunk_offset_delta(&mut patched, delta)?;
        verify_chunk_offsets_within_mdat(&patched, new_payload_start, payload_len)?;
        let encoded = patched
            .encode_to_vec()
            .map_err(MuxError::EncodeError)
            .map_err(ConvertError::Muxer)?;
        if encoded.len() as u64 == new_moov_len {
            new_moov_bytes = encoded;
            converged = true;
            break;
        }
        new_moov_len = encoded.len() as u64;
        new_moov_bytes = encoded;
    }
    if !converged {
        return Err(ConvertError::GaplessSignalling(
            "relocated moov size did not converge",
        ));
    }
    let new_payload_start = ftyp_len
        .checked_add(new_moov_len)
        .and_then(|v| v.checked_add(old_header_len))
        .ok_or(ConvertError::GaplessSignalling(
            "relocated layout overflowed",
        ))?;
    let new_file_len =
        new_payload_start
            .checked_add(payload_len)
            .ok_or(ConvertError::GaplessSignalling(
                "relocated layout overflowed",
            ))?;

    // Move payload forward with a bounded backward copy (destination is after
    // source, so backwards is overlap-safe). The new payload end lies within
    // the old file (the dropped `free` box shrinks the file), so no extension
    // is needed before copying.
    const COPY_CHUNK: u64 = 64 * 1024;
    let mut chunk_buf = vec![0u8; COPY_CHUNK as usize];
    let mut remaining = payload_len;
    while remaining > 0 {
        let chunk = remaining.min(COPY_CHUNK);
        let chunk_usize = usize::try_from(chunk)
            .map_err(|_| ConvertError::GaplessSignalling("copy chunk too large"))?;
        let src = old_payload_start + remaining - chunk;
        let dst = new_payload_start + remaining - chunk;
        output.seek(std::io::SeekFrom::Start(src))?;
        output.read_exact(&mut chunk_buf[..chunk_usize])?;
        output.seek(std::io::SeekFrom::Start(dst))?;
        output.write_all(&chunk_buf[..chunk_usize])?;
        remaining -= chunk;
    }

    output.seek(std::io::SeekFrom::Start(ftyp_len))?;
    output.write_all(&new_moov_bytes)?;
    output.seek(std::io::SeekFrom::Start(ftyp_len + new_moov_len))?;
    output.write_all(&header_bytes)?;
    output.set_len(new_file_len)?;

    // Verify the rewritten layout tiles and is fast-start.
    let verify_len = output.seek(std::io::SeekFrom::End(0))?;
    if verify_len != new_file_len {
        return Err(ConvertError::GaplessSignalling(
            "relocated file size mismatch",
        ));
    }
    let verify_boxes = parse_top_boxes(output, verify_len)?;
    let verify_mdat =
        verify_boxes
            .iter()
            .find(|b| &b.kind == b"mdat")
            .ok_or(ConvertError::GaplessSignalling(
                "relocated container has no mdat box",
            ))?;
    let verify_moov =
        verify_boxes
            .iter()
            .find(|b| &b.kind == b"moov")
            .ok_or(ConvertError::GaplessSignalling(
                "relocated container has no moov box",
            ))?;
    if !(verify_moov.start < verify_mdat.start) {
        return Err(ConvertError::GaplessSignalling(
            "relocation did not produce fast-start layout",
        ));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    use shiguredo_mp4::boxes::{Co64Box, StcoBox};

    fn stco(offsets: &[u32]) -> Either<StcoBox, Co64Box> {
        Either::A(StcoBox {
            chunk_offsets: offsets.to_vec(),
        })
    }

    fn co64(offsets: &[u64]) -> Either<StcoBox, Co64Box> {
        Either::B(Co64Box {
            chunk_offsets: offsets.to_vec(),
        })
    }

    #[test]
    fn stco_shifts_every_entry_without_promotion() {
        let mut table = stco(&[100, 200, 300]);
        assert!(!shift_chunk_offset_table(&mut table, 800).unwrap());
        match table {
            Either::A(stco) => assert_eq!(stco.chunk_offsets, vec![900, 1000, 1100]),
            Either::B(_) => panic!("must stay stco below u32::MAX"),
        }
    }

    #[test]
    fn stco_promotes_when_any_entry_overflows_u32() {
        // Only the middle entry overflows: promotion must still shift every
        // entry, not just the first, and must switch the whole table to co64.
        let max = u32::MAX;
        let mut table = stco(&[100, max - 10, 300]);
        assert!(shift_chunk_offset_table(&mut table, 20).unwrap());
        match table {
            Either::B(co64) => assert_eq!(
                co64.chunk_offsets,
                vec![120, u64::from(max) + 10, 320],
                "every entry must shift, overflowing entry included"
            ),
            Either::A(_) => panic!("must promote to co64 on overflow"),
        }
    }

    #[test]
    fn stco_at_exact_u32_max_boundary() {
        // Landing exactly on u32::MAX stays stco; one more promotes.
        let mut table = stco(&[100]);
        assert!(!shift_chunk_offset_table(&mut table, u64::from(u32::MAX) - 100).unwrap());
        match &table {
            Either::A(stco) => assert_eq!(stco.chunk_offsets, vec![u32::MAX]),
            Either::B(_) => panic!("exact u32::MAX must stay stco"),
        }
        assert!(shift_chunk_offset_table(&mut table, 1).unwrap());
        match table {
            Either::B(co64) => assert_eq!(co64.chunk_offsets, vec![u64::from(u32::MAX) + 1]),
            Either::A(_) => panic!("u32::MAX + 1 must promote"),
        }
    }

    #[test]
    fn co64_shifts_every_entry() {
        let base = u64::from(u32::MAX) + 100;
        let mut table = co64(&[base, 5000]);
        assert!(!shift_chunk_offset_table(&mut table, 800).unwrap());
        match table {
            Either::B(co64) => assert_eq!(co64.chunk_offsets, vec![base + 800, 5800]),
            Either::A(_) => panic!("co64 must stay co64"),
        }
    }

    #[test]
    fn chunk_offset_overflow_fails_closed() {
        let mut table = co64(&[u64::MAX - 5]);
        assert!(shift_chunk_offset_table(&mut table, 10).is_err());
        let mut table = stco(&[1]);
        assert!(shift_chunk_offset_table(&mut table, u64::MAX).is_err());
    }

    #[test]
    fn empty_table_shifts_without_promotion() {
        let mut table = stco(&[]);
        assert!(!shift_chunk_offset_table(&mut table, 800).unwrap());
        match table {
            Either::A(stco) => assert!(stco.chunk_offsets.is_empty()),
            Either::B(_) => panic!("empty stco must stay stco"),
        }
    }

    #[test]
    fn promoted_co64_round_trips_through_encode_decode() {
        let max = u32::MAX;
        let mut table = stco(&[100, max - 10]);
        assert!(shift_chunk_offset_table(&mut table, 20).unwrap());
        let co64 = match table {
            Either::B(co64) => co64,
            Either::A(_) => panic!("must promote"),
        };
        let bytes = co64.encode_to_vec().unwrap();
        let (decoded, consumed) = Co64Box::decode(&bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.chunk_offsets, vec![120, u64::from(max) + 10]);
    }
}
