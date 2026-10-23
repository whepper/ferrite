use aac::aac_enc::encoder::AdtsFrame;

use crate::{AudioProperties, ConvertError};

const ADTS_HEADER_LEN: usize = 7;

/// Check the fixed ADTS fields and frame length emitted by the public encoder.
pub(crate) fn validate(
    frame: &AdtsFrame,
    properties: AudioProperties,
    expected_index: u64,
) -> Result<(), ConvertError> {
    let bytes = &frame.bytes;
    if expected_index > u32::MAX as u64 || u64::from(frame.frame_index) != expected_index {
        return Err(ConvertError::AdtsInvalid("non-sequential frame index"));
    }
    if bytes.len() < ADTS_HEADER_LEN {
        return Err(ConvertError::AdtsInvalid("truncated header"));
    }
    if bytes[0] != 0xFF || bytes[1] & 0xF0 != 0xF0 {
        return Err(ConvertError::AdtsInvalid("bad syncword"));
    }
    if bytes[1] & 0x08 != 0 {
        return Err(ConvertError::AdtsInvalid("not MPEG-4 ADTS"));
    }
    if bytes[1] & 0x06 != 0 {
        return Err(ConvertError::AdtsInvalid("nonzero ADTS layer"));
    }
    if bytes[1] & 0x01 != 1 {
        return Err(ConvertError::AdtsInvalid("unexpected ADTS CRC"));
    }
    if (bytes[2] >> 6) & 0x03 != 1 {
        return Err(ConvertError::AdtsInvalid("not AAC-LC profile"));
    }

    let sample_rate_index = match properties.sample_rate {
        44_100 => 4,
        48_000 => 3,
        _ => return Err(ConvertError::AdtsInvalid("unsupported sample-rate index")),
    };
    if (bytes[2] >> 2) & 0x0F != sample_rate_index {
        return Err(ConvertError::AdtsInvalid("wrong ADTS sample rate"));
    }

    let channel_configuration = ((bytes[2] & 0x01) << 2) | ((bytes[3] >> 6) & 0x03);
    if u32::from(channel_configuration) != properties.channels {
        return Err(ConvertError::AdtsInvalid(
            "wrong ADTS channel configuration",
        ));
    }

    let frame_length = ((usize::from(bytes[3] & 0x03)) << 11)
        | (usize::from(bytes[4]) << 3)
        | (usize::from(bytes[5]) >> 5);
    if frame_length != bytes.len() {
        return Err(ConvertError::AdtsInvalid("ADTS frame length mismatch"));
    }
    if bytes.len() == ADTS_HEADER_LEN {
        return Err(ConvertError::AdtsInvalid("empty AAC payload"));
    }
    if bytes[6] & 0x03 != 0 {
        return Err(ConvertError::AdtsInvalid("multiple raw data blocks"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_frame(
        len: usize,
        profile: u8,
        sample_rate_index: u8,
        channel_configuration: u8,
        raw_blocks: u8,
    ) -> AdtsFrame {
        let mut bytes = vec![0u8; len];
        bytes[0] = 0xFF;
        bytes[1] = 0xF1;
        bytes[2] = (profile << 6)
            | ((sample_rate_index & 0x0F) << 2)
            | ((channel_configuration >> 2) & 0x01);
        bytes[3] = (channel_configuration & 0x03) << 6 | ((len >> 11) & 0x03) as u8;
        bytes[4] = (len >> 3) as u8;
        bytes[5] = ((len & 0x07) as u8) << 5 | 0x1F;
        bytes[6] = 0xFC | (raw_blocks & 0x03);
        AdtsFrame {
            bytes,
            frame_index: 0,
        }
    }

    fn properties() -> AudioProperties {
        AudioProperties {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 16,
        }
    }

    #[test]
    fn accepts_valid_stereo_frame() {
        let frame = test_frame(32, 1, 3, 2, 0);
        validate(&frame, properties(), 0).unwrap();
    }

    #[test]
    fn rejects_bad_syncword() {
        let mut frame = test_frame(32, 1, 3, 2, 0);
        frame.bytes[0] = 0;
        assert!(matches!(
            validate(&frame, properties(), 0),
            Err(ConvertError::AdtsInvalid("bad syncword"))
        ));
    }

    #[test]
    fn rejects_non_mpeg4_id() {
        let mut frame = test_frame(32, 1, 3, 2, 0);
        frame.bytes[1] |= 0x08;
        assert!(matches!(
            validate(&frame, properties(), 0),
            Err(ConvertError::AdtsInvalid("not MPEG-4 ADTS"))
        ));
    }

    #[test]
    fn rejects_bad_profile() {
        let mut frame = test_frame(32, 0, 3, 2, 0);
        assert!(matches!(
            validate(&frame, properties(), 0),
            Err(ConvertError::AdtsInvalid("not AAC-LC profile"))
        ));
        frame.bytes[2] = 0xF0;
        assert!(matches!(
            validate(&frame, properties(), 0),
            Err(ConvertError::AdtsInvalid("not AAC-LC profile"))
        ));
    }

    #[test]
    fn rejects_wrong_sample_rate_and_channels() {
        let frame = test_frame(32, 1, 4, 2, 0);
        assert!(matches!(
            validate(&frame, properties(), 0),
            Err(ConvertError::AdtsInvalid("wrong ADTS sample rate"))
        ));

        let frame = test_frame(32, 1, 3, 1, 0);
        assert!(matches!(
            validate(&frame, properties(), 0),
            Err(ConvertError::AdtsInvalid(
                "wrong ADTS channel configuration"
            ))
        ));
    }

    #[test]
    fn rejects_length_and_payload_errors() {
        let frame = test_frame(32, 1, 3, 2, 0);
        let truncated = AdtsFrame {
            bytes: frame.bytes[..16].to_vec(),
            frame_index: 0,
        };
        assert!(matches!(
            validate(&truncated, properties(), 0),
            Err(ConvertError::AdtsInvalid("ADTS frame length mismatch"))
        ));

        let empty = test_frame(7, 1, 3, 2, 0);
        assert!(matches!(
            validate(&empty, properties(), 0),
            Err(ConvertError::AdtsInvalid("empty AAC payload"))
        ));
    }

    #[test]
    fn rejects_multiple_raw_blocks_and_wrong_index() {
        let frame = test_frame(32, 1, 3, 2, 1);
        assert!(matches!(
            validate(&frame, properties(), 0),
            Err(ConvertError::AdtsInvalid("multiple raw data blocks"))
        ));

        let frame = test_frame(32, 1, 3, 2, 0);
        assert!(matches!(
            validate(&frame, properties(), 1),
            Err(ConvertError::AdtsInvalid("non-sequential frame index"))
        ));
    }
}
