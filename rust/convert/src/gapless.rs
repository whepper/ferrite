//! Gapless-signalling support for M4A output.
//!
//! Default M4A output carries Apple `iTunSMPB` metadata (M7.13 product
//! decision); an experimental edit list remains opt-in for research. This
//! module only *computes and serializes* gapless metadata; it never touches
//! the encoder, the AAC payloads, or the ADTS path. All values derive from
//! counts the converter observes itself:
//!
//! - `S`: source sample-frames, counted from successfully decoded PCM.
//! - `N`: emitted AAC access units, counted as frames are written.
//! - `L`: AAC frame length from [`EncoderConfig::frame_length`](aac::aac_enc::config::EncoderConfig::frame_length).
//! - `D`: encoder delay from [`EncoderConfig::encoder_delay`](aac::aac_enc::config::EncoderConfig::encoder_delay).
//! - `P = N * L - S - D`: tail padding.
//!
//! No gapless playback is claimed. See the crate README for the validation
//! status of the experimental output.

use shiguredo_mp4::boxes::{EdtsBox, ElstBox, ElstEntry, UnknownBox};
use shiguredo_mp4::{BoxSize, BoxType, FixedPointNumber};

/// Validated gapless accounting for one conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GaplessAccounting {
    source_frames: u64,
    encoded_frames: u64,
    frame_length: u64,
    delay: u64,
    padding: u64,
}

/// Why gapless accounting was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AccountingError {
    /// `N * L < S + D`: the emitted stream cannot contain the source plus the
    /// encoder delay, so no trim point exists.
    InconsistentCounts,
    /// `N * L` or `S + D` overflowed `u64`, or a box size exceeded `u32`.
    Overflow,
    /// A zero frame length or an unrepresentable edit-list media time.
    InvalidParameter,
}

impl AccountingError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::InconsistentCounts => {
                "encoded frames do not cover source frames plus encoder delay"
            }
            Self::Overflow => "gapless accounting overflowed",
            Self::InvalidParameter => "invalid gapless accounting parameter",
        }
    }
}

impl GaplessAccounting {
    /// Validate counts and derive the tail padding with checked arithmetic.
    ///
    /// Every operand is a caller-observed count; nothing is inferred from the
    /// encoder beyond the `frame_length`/`delay` accessors it already exposes.
    /// `S = 0` is arithmetically valid (an empty source still carries the
    /// delay frames the flush emits); whether the converter accepts empty
    /// input is a separate, unchanged behavior.
    pub(crate) fn compute(
        source_frames: u64,
        encoded_frames: u64,
        frame_length: u64,
        delay: u64,
    ) -> Result<Self, AccountingError> {
        if frame_length == 0 {
            return Err(AccountingError::InvalidParameter);
        }
        let total = encoded_frames
            .checked_mul(frame_length)
            .ok_or(AccountingError::Overflow)?;
        let needed = source_frames
            .checked_add(delay)
            .ok_or(AccountingError::Overflow)?;
        if total < needed {
            return Err(AccountingError::InconsistentCounts);
        }
        Ok(Self {
            source_frames,
            encoded_frames,
            frame_length,
            delay,
            padding: total - needed,
        })
    }

    /// Single edit-list entry trimming `D` priming samples and `P` tail
    /// samples: `media_time = D`, `edit_duration = S`, normal rate.
    ///
    /// The caller must ensure the movie timescale equals the media timescale
    /// (true for this application's single-track files); the entry itself
    /// carries no timescale.
    pub(crate) fn edit_list(self) -> Result<EdtsBox, AccountingError> {
        let media_time =
            i64::try_from(self.delay).map_err(|_| AccountingError::InvalidParameter)?;
        Ok(EdtsBox {
            elst_box: Some(ElstBox {
                entries: vec![ElstEntry {
                    edit_duration: self.source_frames,
                    media_time,
                    media_rate: FixedPointNumber::new(1i16, 0i16),
                }],
            }),
            unknown_boxes: Vec::new(),
        })
    }

    /// Apple `iTunSMPB` value string for `(D, P, S)`.
    ///
    /// Layout (conventional, not normative — there is no ISO definition of
    /// `iTunSMPB`): a leading space, then `priming` (8 hex digits), `remainder`
    /// (8 digits), total source samples (16 digits), then eight zero words,
    /// space-separated, uppercase. The consumer contract is FFmpeg's
    /// `ff_itunes_parse_smpb` (`libavformat/demux_utils.c`: four
    /// whitespace-separated hex fields `reserved priming remainder samples`,
    /// each at most 16 digits) plus CoreAudio's observed behavior on
    /// Apple-produced files; the producer template is `afconvert` output
    /// (`meta`/`hdlr`/`ilst`/`----`/`mean`/`name`/`data` structure mirrored
    /// byte-for-byte below except for these three values).
    ///
    /// Delay and padding must fit the 8-digit fields; anything wider would
    /// break the canonical 116-character layout, so larger values are
    /// rejected. The 16-digit source field accepts the full `u64` range.
    pub(crate) fn itunsmpb_value(self) -> Result<String, AccountingError> {
        if self.delay > u64::from(u32::MAX) || self.padding > u64::from(u32::MAX) {
            return Err(AccountingError::InvalidParameter);
        }
        Ok(format!(
            " {:08X} {:08X} {:08X} {:016X} \
             {:08X} {:08X} {:08X} {:08X} \
             {:08X} {:08X} {:08X} {:08X}",
            0, self.delay, self.padding, self.source_frames, 0, 0, 0, 0, 0, 0, 0, 0,
        ))
    }

    /// Complete `udta` box (`udta > meta > ilst > ---- > mean/name/data`)
    /// carrying the [`itunsmpb_value`](Self::itunsmpb_value), ready to append
    /// to `moov.unknown_boxes`.
    ///
    /// Fixed skeleton bytes replicate Apple `afconvert` output: `meta`
    /// version/flags 0, `hdlr` handler `mdir`, `mean` prefix plus
    /// `com.apple.iTunes`, `name` prefix plus `iTunSMPB`, `data` type 1
    /// (UTF-8) and locale 0. Only the `data` payload varies per file.
    pub(crate) fn udta_box(self) -> Result<UnknownBox, AccountingError> {
        /// `meta` child `hdlr` payload, verbatim from Apple output: FullBox
        /// version/flags 0, pre_defined 0, handler `mdir`, then Apple's fixed
        /// trailing bytes. Carries no per-file values.
        const META_HDLR_PAYLOAD: [u8; 26] = [
            0x00, 0x00, 0x00, 0x00, // version/flags
            0x00, 0x00, 0x00, 0x00, // pre_defined
            0x6d, 0x64, 0x69, 0x72, // handler_type 'mdir'
            0x61, 0x70, 0x70, 0x6c, // Apple's fixed 'appl' word
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        const MEAN_PREFIX: [u8; 4] = [0x00, 0x00, 0x00, 0x00];
        const MEAN_NAME: &[u8] = b"com.apple.iTunes";
        const ITEM_NAME: &[u8] = b"iTunSMPB";

        let value = self.itunsmpb_value()?;
        let value_bytes = value.as_bytes();
        let mut data_payload = Vec::with_capacity(
            8usize
                .checked_add(value_bytes.len())
                .ok_or(AccountingError::Overflow)?,
        );
        data_payload.extend_from_slice(&1u32.to_be_bytes()); // type: UTF-8
        data_payload.extend_from_slice(&0u32.to_be_bytes()); // locale
        data_payload.extend_from_slice(value_bytes);
        let data = boxed(b"data", &data_payload)?;

        let mut mean_payload = Vec::with_capacity(
            MEAN_PREFIX
                .len()
                .checked_add(MEAN_NAME.len())
                .ok_or(AccountingError::Overflow)?,
        );
        mean_payload.extend_from_slice(&MEAN_PREFIX);
        mean_payload.extend_from_slice(MEAN_NAME);
        let mean = boxed(b"mean", &mean_payload)?;

        let mut name_payload = Vec::with_capacity(
            MEAN_PREFIX
                .len()
                .checked_add(ITEM_NAME.len())
                .ok_or(AccountingError::Overflow)?,
        );
        name_payload.extend_from_slice(&MEAN_PREFIX);
        name_payload.extend_from_slice(ITEM_NAME);
        let name = boxed(b"name", &name_payload)?;

        let mut freeform = Vec::with_capacity(
            mean.len()
                .checked_add(name.len())
                .and_then(|len| len.checked_add(data.len()))
                .ok_or(AccountingError::Overflow)?,
        );
        freeform.extend_from_slice(&mean);
        freeform.extend_from_slice(&name);
        freeform.extend_from_slice(&data);
        let freeform = boxed(b"----", &freeform)?;

        let ilst = boxed(b"ilst", &freeform)?;

        let mut meta_payload = Vec::with_capacity(
            4usize
                .checked_add(META_HDLR_PAYLOAD.len())
                .and_then(|len| len.checked_add(ilst.len()))
                .ok_or(AccountingError::Overflow)?,
        );
        meta_payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // version/flags
        meta_payload.extend_from_slice(&boxed(b"hdlr", &META_HDLR_PAYLOAD)?);
        meta_payload.extend_from_slice(&ilst);
        let meta = boxed(b"meta", &meta_payload)?;

        let size = u32::try_from(meta.len().checked_add(8).ok_or(AccountingError::Overflow)?)
            .map_err(|_| AccountingError::Overflow)?;
        Ok(UnknownBox {
            box_type: BoxType::Normal(*b"udta"),
            box_size: BoxSize::U32(size),
            payload: meta,
        })
    }
}

/// Serialize one complete box (8-byte header plus payload) with checked sizes.
fn boxed(tag: &[u8; 4], payload: &[u8]) -> Result<Vec<u8>, AccountingError> {
    let size = u32::try_from(
        payload
            .len()
            .checked_add(8)
            .ok_or(AccountingError::Overflow)?,
    )
    .map_err(|_| AccountingError::Overflow)?;
    let mut bytes = Vec::with_capacity(size as usize);
    bytes.extend_from_slice(&size.to_be_bytes());
    bytes.extend_from_slice(tag);
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

/// The exact 28-byte `ftyp` box the pinned muxer writes: major `isom`,
/// minor 0, compatible `isom`, `iso2`, `mp41`.
///
/// [`EXPERIMENTAL_FTYP_BYTES`] may only replace these exact bytes; anything
/// else fails the experiment instead of splicing an unknown layout.
pub(crate) const DEFAULT_FTYP_BYTES: [u8; 28] = [
    0x00, 0x00, 0x00, 0x1C, b'f', b't', b'y', b'p', //
    b'i', b's', b'o', b'm', 0x00, 0x00, 0x00, 0x00, //
    b'i', b's', b'o', b'm', b'i', b's', b'o', b'2', //
    b'm', b'p', b'4', b'1',
];

/// Apple-style `ftyp` for experimental `iTunSMPB` output: major `M4A`,
/// minor 0, compatible `M4A`, `mp42`, `isom`.
///
/// Verified necessity, not decoration: with the default brands above,
/// macOS CoreAudio (`afinfo`/`afconvert`) reports `2112 priming, 0 remainder`
/// and ignores the `iTunSMPB` box entirely; with `M4A ` in the major or a
/// compatible brand it reports the signalled priming/remainder and trims.
/// FFmpeg parses the metadata under either brand set. Box order (`moov`
/// after `mdat`) was tested and is irrelevant. Same 28-byte size, so file
/// offsets are unaffected.
pub(crate) const EXPERIMENTAL_FTYP_BYTES: [u8; 28] = [
    0x00, 0x00, 0x00, 0x1C, b'f', b't', b'y', b'p', //
    b'M', b'4', b'A', b' ', 0x00, 0x00, 0x00, 0x00, //
    b'M', b'4', b'A', b' ', b'm', b'p', b'4', b'2', //
    b'i', b's', b'o', b'm',
];

#[cfg(test)]
mod tests {
    use super::*;

    use shiguredo_mp4::Encode;

    const FRAME_LENGTH: u64 = 1024;
    const DELAY: u64 = 1600;

    fn expected_frames(source: u64) -> u64 {
        source.checked_add(DELAY).unwrap().div_ceil(FRAME_LENGTH)
    }

    #[test]
    fn accounting_matches_observed_frame_counts() {
        // (S, N, P) with L = 1024 and D = 1600 throughout.
        let cases = [
            (0, 2, 448),
            (1, 2, 447),
            (100, 2, 348),
            (448, 2, 0),
            (449, 3, 1023),
            (500, 3, 972),
            (1023, 3, 449),
            (1024, 3, 448),
            (1025, 3, 447),
            (2048, 4, 448),
            (3000, 5, 520),
            (12_000, 14, 736),
        ];
        for (source, frames, padding) in cases {
            assert_eq!(
                expected_frames(source),
                frames,
                "frame count for S={source}"
            );
            assert_eq!(
                GaplessAccounting::compute(source, frames, FRAME_LENGTH, DELAY).unwrap(),
                GaplessAccounting {
                    source_frames: source,
                    encoded_frames: frames,
                    frame_length: FRAME_LENGTH,
                    delay: DELAY,
                    padding,
                },
                "accounting for S={source}"
            );
        }
    }

    #[test]
    fn accounting_rejects_inconsistent_counts() {
        // One frame short of covering source plus delay in each case.
        for (source, frames) in [
            (0, 0),
            (0, 1),
            (1, 1),
            (100, 1),
            (448, 1),
            (449, 2),
            (12_000, 13),
        ] {
            assert_eq!(
                GaplessAccounting::compute(source, frames, FRAME_LENGTH, DELAY),
                Err(AccountingError::InconsistentCounts),
                "S={source} N={frames}"
            );
        }
    }

    #[test]
    fn accounting_rejects_overflow_and_invalid_parameters() {
        assert_eq!(
            GaplessAccounting::compute(0, u64::MAX, FRAME_LENGTH, DELAY),
            Err(AccountingError::Overflow),
            "N * L overflows"
        );
        assert_eq!(
            GaplessAccounting::compute(u64::MAX, u64::MAX, FRAME_LENGTH, DELAY),
            Err(AccountingError::Overflow),
            "S + D overflows"
        );
        assert_eq!(
            GaplessAccounting::compute(1, u64::MAX, FRAME_LENGTH, u64::MAX),
            Err(AccountingError::Overflow),
            "S + D overflows with maximal delay"
        );
        assert_eq!(
            GaplessAccounting::compute(100, 2, 0, DELAY),
            Err(AccountingError::InvalidParameter),
            "zero frame length"
        );
    }

    #[test]
    fn real_encoder_frame_counts_match_accounting_inputs() {
        // The table above must reflect the encoder, not just the formula: drive
        // the public `Encoder` with exact sample counts and check the emitted
        // frame count, then check the accounting accepts the observed pair.
        use aac::aac_enc::config::{EncoderConfig, TransportType};
        use aac::aac_enc::encoder::Encoder;

        for source in [
            0, 1, 100, 448, 449, 500, 1023, 1024, 1025, 2048, 3000, 12_000,
        ] {
            let config =
                EncoderConfig::new(48_000, 1, FRAME_LENGTH as usize, TransportType::Adts).unwrap();
            let delay = config.encoder_delay() as u64;
            assert_eq!(delay, DELAY);
            let mut encoder = Encoder::new(config).unwrap();
            let mut frames = encoder.push(&vec![0i16; source as usize]).unwrap();
            frames.extend(encoder.flush().unwrap());
            assert_eq!(
                frames.len() as u64,
                expected_frames(source),
                "encoder emitted frame count for S={source}"
            );
            let accounting =
                GaplessAccounting::compute(source, frames.len() as u64, FRAME_LENGTH, delay)
                    .unwrap();
            assert_eq!(
                accounting,
                GaplessAccounting {
                    source_frames: source,
                    encoded_frames: frames.len() as u64,
                    frame_length: FRAME_LENGTH,
                    delay,
                    padding: frames.len() as u64 * FRAME_LENGTH - source - delay,
                },
                "accounting for encoder-observed S={source}"
            );
        }
    }

    #[test]
    fn ftyp_brand_constants_are_same_size_replacements() {
        assert_eq!(DEFAULT_FTYP_BYTES.len(), 28);
        assert_eq!(EXPERIMENTAL_FTYP_BYTES.len(), 28);
        assert_eq!(&DEFAULT_FTYP_BYTES[4..8], b"ftyp");
        assert_eq!(&EXPERIMENTAL_FTYP_BYTES[4..8], b"ftyp");
        assert_eq!(&DEFAULT_FTYP_BYTES[8..12], b"isom");
        assert_eq!(&EXPERIMENTAL_FTYP_BYTES[8..12], b"M4A ");
        // Compatible brands keep `isom` so ISO-BMFF readers still accept the file.
        assert!(EXPERIMENTAL_FTYP_BYTES[16..28]
            .windows(4)
            .any(|w| w == b"isom"));
    }

    #[test]
    fn edit_list_entry_trims_delay_and_padding() {
        let accounting = GaplessAccounting::compute(12_000, 14, FRAME_LENGTH, DELAY).unwrap();
        let edts = accounting.edit_list().unwrap();
        let bytes = edts.encode_to_vec().unwrap();
        // edts(36): size + type + elst(28): size + type + v/f + count +
        // edit_duration(u32) + media_time(i32) + media_rate(u32).
        assert_eq!(
            bytes,
            [
                0x00, 0x00, 0x00, 0x24, b'e', b'd', b't', b's', // edts, 36
                0x00, 0x00, 0x00, 0x1C, b'e', b'l', b's', b't', // elst, 28
                0x00, 0x00, 0x00, 0x00, // version 0, flags 0
                0x00, 0x00, 0x00, 0x01, // one entry
                0x00, 0x00, 0x2E, 0xE0, // edit_duration = 12000
                0x00, 0x00, 0x06, 0x40, // media_time = 1600
                0x00, 0x01, 0x00, 0x00, // media_rate = 1.0
            ],
        );
    }

    #[test]
    fn itunsmpb_value_matches_apple_layout() {
        // D = 1600, P = 736, S = 12000: field-for-field comparison against the
        // Apple-produced reference structure (values substituted, layout kept).
        let accounting = GaplessAccounting::compute(12_000, 14, FRAME_LENGTH, DELAY).unwrap();
        assert_eq!(
            accounting.itunsmpb_value().unwrap(),
            " 00000000 00000640 000002E0 0000000000002EE0 \
             00000000 00000000 00000000 00000000 \
             00000000 00000000 00000000 00000000",
        );
        assert_eq!(accounting.itunsmpb_value().unwrap().len(), 116);
    }

    #[test]
    fn itunsmpb_value_rejects_fields_beyond_eight_digits() {
        // Delay and padding share the 8-digit field width with the Apple
        // layout; wider values would shift every field after them.
        let max_delay =
            GaplessAccounting::compute(0, 4_194_304, FRAME_LENGTH, u64::from(u32::MAX)).unwrap();
        assert_eq!(
            max_delay,
            GaplessAccounting {
                source_frames: 0,
                encoded_frames: 4_194_304,
                frame_length: FRAME_LENGTH,
                delay: u64::from(u32::MAX),
                padding: 1,
            }
        );
        assert_eq!(max_delay.itunsmpb_value().unwrap().len(), 116);

        let over_delay =
            GaplessAccounting::compute(0, 4_194_305, FRAME_LENGTH, u64::from(u32::MAX) + 1)
                .unwrap();
        assert_eq!(
            over_delay.itunsmpb_value(),
            Err(AccountingError::InvalidParameter)
        );
        assert_eq!(
            over_delay.udta_box(),
            Err(AccountingError::InvalidParameter),
            "box assembly must propagate the layout bound"
        );

        // The 16-digit source field accepts large counts; the layout holds.
        let big_source =
            GaplessAccounting::compute(u64::from(u32::MAX), 4_194_306, FRAME_LENGTH, DELAY)
                .unwrap();
        assert_eq!(
            big_source,
            GaplessAccounting {
                source_frames: u64::from(u32::MAX),
                encoded_frames: 4_194_306,
                frame_length: FRAME_LENGTH,
                delay: DELAY,
                padding: 449,
            }
        );
        assert_eq!(big_source.itunsmpb_value().unwrap().len(), 116);
    }

    #[test]
    fn udta_box_nests_mean_name_and_data() {
        let accounting = GaplessAccounting::compute(3000, 5, FRAME_LENGTH, DELAY).unwrap();
        let udta = accounting.udta_box().unwrap();
        assert_eq!(udta.box_type, BoxType::Normal(*b"udta"));
        // udta payload is the complete meta box; walk the nesting manually.
        let meta = &udta.payload;
        assert_eq!(&meta[4..8], b"meta");
        let meta_size = u32::from_be_bytes(meta[0..4].try_into().unwrap()) as usize;
        assert_eq!(meta_size, meta.len());
        assert_eq!(&meta[8..12], &[0x00, 0x00, 0x00, 0x00]); // version/flags
        assert_eq!(&meta[16..20], b"hdlr");
        let hdlr_size = u32::from_be_bytes(meta[12..16].try_into().unwrap()) as usize;
        assert_eq!(hdlr_size, 34);
        let ilst = &meta[12 + hdlr_size..];
        assert_eq!(&ilst[4..8], b"ilst");
        let freeform = &ilst[8..];
        assert_eq!(&freeform[4..8], b"----");
        let mut offset = 8;
        let mut seen = (false, false, false);
        while offset < freeform.len() {
            let size =
                u32::from_be_bytes(freeform[offset..offset + 4].try_into().unwrap()) as usize;
            let kind = &freeform[offset + 4..offset + 8];
            let payload = &freeform[offset + 8..offset + size];
            match kind {
                b"mean" => {
                    assert_eq!(payload, b"\x00\x00\x00\x00com.apple.iTunes");
                    seen.0 = true;
                }
                b"name" => {
                    assert_eq!(payload, b"\x00\x00\x00\x00iTunSMPB");
                    seen.1 = true;
                }
                b"data" => {
                    assert_eq!(&payload[0..4], &[0x00, 0x00, 0x00, 0x01]); // UTF-8
                    assert_eq!(&payload[4..8], &[0x00, 0x00, 0x00, 0x00]); // locale
                    assert_eq!(
                        &payload[8..],
                        accounting.itunsmpb_value().unwrap().as_bytes(),
                        "data value carries (D, P, S) = (1600, 520, 3000)"
                    );
                    seen.2 = true;
                }
                _ => panic!("unexpected ---- child {kind:?}"),
            }
            offset += size;
        }
        assert_eq!(seen, (true, true, true));
        assert_eq!(offset, freeform.len(), "---- children must tile exactly");
        // Declared UnknownBox size matches the encoded udta box.
        let size = match udta.box_size {
            BoxSize::U32(size) => size as usize,
            BoxSize::U64(_) => panic!("udta must use a 32-bit size"),
        };
        assert_eq!(size, 8 + udta.payload.len());
    }
}
