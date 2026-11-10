//! Public-API production integration tests for the `ferrite-aac` encoder.
//!
//! These tests deliberately use only the public surface an application sees:
//! `EncoderConfig`, `Encoder`, `AdtsFrame`, and `EncodeError`. They do not
//! inspect internal buffer state or test-only seams.

use std::f64::consts::PI;

use ferrite_aac::aac_enc::config::{ConfigError, EncoderConfig, TransportType, FRAME_LENGTH};
use ferrite_aac::aac_enc::encoder::{AdtsFrame, EncodeError, Encoder};

const ADTS_HEADER_BYTES: usize = 7;

fn adts_frame_length(bytes: &[u8]) -> usize {
    ((usize::from(bytes[3] & 0x03)) << 11)
        | (usize::from(bytes[4]) << 3)
        | (usize::from(bytes[5]) >> 5)
}

fn assert_adts(frame: &AdtsFrame, expected_index: u32, sample_rate: u32, channels: usize) {
    assert_eq!(frame.frame_index, expected_index);
    let bytes = &frame.bytes;

    assert!(
        bytes.len() > ADTS_HEADER_BYTES,
        "frame {expected_index} has no payload"
    );
    assert_eq!(bytes[0], 0xFF, "frame {expected_index} syncword high");
    assert_eq!(bytes[1] & 0xF0, 0xF0, "frame {expected_index} syncword low");
    assert_eq!(bytes[1] & 0x08, 0, "frame {expected_index} must be MPEG-4");
    assert_eq!(bytes[1] & 0x06, 0, "frame {expected_index} layer must be 0");
    assert_eq!(
        bytes[1] & 0x01,
        1,
        "frame {expected_index} must not use CRC"
    );
    assert_eq!(
        (bytes[2] >> 6) & 0x03,
        1,
        "frame {expected_index} must be AAC-LC"
    );

    let expected_rate_index = match sample_rate {
        44_100 => 4,
        48_000 => 3,
        other => panic!("test does not cover sample rate {other}"),
    };
    assert_eq!(
        (bytes[2] >> 2) & 0x0F,
        expected_rate_index,
        "frame {expected_index} sample-rate index"
    );

    let channel_configuration = ((bytes[2] & 0x01) << 2) | ((bytes[3] >> 6) & 0x03);
    assert_eq!(
        channel_configuration as usize, channels,
        "frame {expected_index} channel configuration"
    );
    assert_eq!(
        adts_frame_length(bytes),
        bytes.len(),
        "frame {expected_index} ADTS frame length"
    );
    assert_eq!(
        bytes[6] & 0x03,
        0,
        "frame {expected_index} must contain one raw data block"
    );
    assert!(
        frame.raw_payload().is_ok(),
        "frame {expected_index} payload must be valid"
    );
}

fn pcm_sine(
    sample_frames: usize,
    channels: usize,
    sample_rate: u32,
    start_frame: usize,
) -> Vec<i16> {
    let mut pcm = Vec::with_capacity(sample_frames * channels);
    for offset in 0..sample_frames {
        let t = (start_frame + offset) as f64;
        let phase = 2.0 * PI * 440.0 * t / sample_rate as f64;
        let left = (phase.sin() * 12_000.0) as i16;
        let right = (phase.sin() * 6_000.0) as i16;
        for channel in 0..channels {
            pcm.push(if channel == 0 { left } else { right });
        }
    }
    pcm
}

/// Encode `total_frames` per channel in chunks of `chunk_frames`, then flush.
fn encode_all(config: EncoderConfig, total_frames: usize, chunk_frames: usize) -> Vec<AdtsFrame> {
    assert!(chunk_frames > 0);
    let channels = config.channels() as usize;
    let sample_rate = config.sample_rate();
    let mut encoder = Encoder::new(config).expect("supported production configuration");
    let mut frames = Vec::new();
    let mut position = 0;
    while position < total_frames {
        let take = (total_frames - position).min(chunk_frames);
        let pcm = pcm_sine(take, channels, sample_rate, position);
        frames.extend(encoder.push(&pcm).expect("push succeeds"));
        position += take;
    }
    frames.extend(encoder.flush().expect("flush succeeds"));
    assert!(encoder.is_finished(), "flush must finish the encoder");
    frames
}

fn flatten(frames: &[AdtsFrame]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for frame in frames {
        bytes.extend_from_slice(&frame.bytes);
    }
    bytes
}

#[test]
fn all_supported_configurations_emit_valid_adts() {
    for (sample_rate, channels) in [(44_100, 1usize), (44_100, 2), (48_000, 1), (48_000, 2)] {
        let config = EncoderConfig::new(
            sample_rate,
            channels as u32,
            FRAME_LENGTH,
            TransportType::Adts,
        )
        .unwrap();
        let total_frames = sample_rate as usize + 123; // ~1 s plus a partial frame
        let frames = encode_all(config, total_frames, 8_000);
        let expected_frames = (total_frames + config.encoder_delay()).div_ceil(FRAME_LENGTH);

        assert_eq!(
            frames.len(),
            expected_frames,
            "{sample_rate} Hz / {channels} ch frame count"
        );
        for (index, frame) in frames.iter().enumerate() {
            assert_adts(frame, index as u32, sample_rate, channels);
        }
    }
}

#[test]
fn chunk_boundaries_do_not_change_the_encoded_stream() {
    let config = EncoderConfig::stereo_48k_adts();
    let total_frames = 48_000 + 777;
    let reference = flatten(&encode_all(config, total_frames, total_frames));

    for chunk_frames in [
        1,
        2,
        7,
        333,
        1_023,
        1_024,
        1_025,
        4_096,
        8_000,
        total_frames,
    ] {
        let candidate = flatten(&encode_all(config, total_frames, chunk_frames));
        assert_eq!(
            candidate, reference,
            "{chunk_frames}-frame chunks must produce a byte-identical stream"
        );
    }
}

#[test]
fn empty_and_short_input_finalize_predictably() {
    let mono_48k = EncoderConfig::new(48_000, 1, FRAME_LENGTH, TransportType::Adts).unwrap();
    let mut empty = Encoder::new(mono_48k).unwrap();
    assert!(empty.push(&[]).unwrap().is_empty());
    assert!(!empty.is_finished());
    let tail = empty.flush().unwrap();
    assert_eq!(tail.len(), 1600usize.div_ceil(FRAME_LENGTH));
    for (index, frame) in tail.iter().enumerate() {
        assert_adts(frame, index as u32, 48_000, 1);
    }
    assert!(empty.is_finished());
    assert!(matches!(empty.flush(), Err(EncodeError::AlreadyFinished)));
    assert!(matches!(empty.push(&[]), Err(EncodeError::AlreadyFinished)));

    let stereo_44k = EncoderConfig::new(44_100, 2, FRAME_LENGTH, TransportType::Adts).unwrap();
    let short = encode_all(stereo_44k, 3, 8_000);
    assert_eq!(
        short.len(),
        (3 + stereo_44k.encoder_delay()).div_ceil(FRAME_LENGTH)
    );
    for (index, frame) in short.iter().enumerate() {
        assert_adts(frame, index as u32, 44_100, 2);
    }
}

#[test]
fn unaligned_input_is_rejected_without_poisoning_the_encoder() {
    let config = EncoderConfig::stereo_48k_adts();
    let mut recovered = Encoder::new(config).unwrap();
    assert!(matches!(
        recovered.push(&[1, 2, 3]),
        Err(EncodeError::UnalignedInput {
            len: 3,
            channels: 2
        })
    ));
    assert!(!recovered.is_finished());

    let pcm = pcm_sine(5_000, 2, 48_000, 0);
    let mut got = flatten(&recovered.push(&pcm).unwrap());
    got.extend(flatten(&recovered.flush().unwrap()));

    let mut fresh = Encoder::new(config).unwrap();
    let mut want = flatten(&fresh.push(&pcm).unwrap());
    want.extend(flatten(&fresh.flush().unwrap()));

    assert_eq!(
        got, want,
        "a rejected unaligned push must leave no buffered state"
    );
}

#[test]
fn unsupported_configurations_fail_with_explicit_errors() {
    assert!(matches!(
        EncoderConfig::new(12_345, 2, FRAME_LENGTH, TransportType::Adts),
        Err(ConfigError::UnsupportedSampleRate(12_345))
    ));
    assert!(matches!(
        EncoderConfig::new(48_000, 0, FRAME_LENGTH, TransportType::Adts),
        Err(ConfigError::UnsupportedChannelCount(0))
    ));
    assert!(matches!(
        EncoderConfig::new(48_000, 2, 960, TransportType::Adts),
        Err(ConfigError::UnsupportedFrameLength(960))
    ));

    // `EncoderConfig` describes the FDK parameter plane; the production
    // `Encoder` deliberately supports only the four personal-music cells.
    for (sample_rate, channels, transport) in [
        (32_000, 2, TransportType::Adts),
        (96_000, 1, TransportType::Adts),
        (48_000, 6, TransportType::Adts),
        (48_000, 2, TransportType::Raw),
    ] {
        let config = EncoderConfig::new(sample_rate, channels, FRAME_LENGTH, transport).unwrap();
        match Encoder::new(config) {
            Err(EncodeError::UnsupportedConfiguration {
                sample_rate: rejected_rate,
                channels: rejected_channels,
                ..
            }) => {
                assert_eq!(rejected_rate, sample_rate);
                assert_eq!(rejected_channels, channels);
            }
            Ok(_) => panic!("{sample_rate} Hz / {channels} ch unexpectedly accepted"),
            Err(other) => panic!("unexpected error for {sample_rate} Hz / {channels} ch: {other}"),
        }
    }
}

#[test]
fn long_stream_completes_with_the_expected_frame_count() {
    let config = EncoderConfig::stereo_48k_adts();
    let total_frames = 48_000 * 120; // two minutes
    let frames = encode_all(config, total_frames, 8_000);
    let expected_frames = (total_frames + config.encoder_delay()).div_ceil(FRAME_LENGTH);

    assert_eq!(frames.len(), expected_frames);
    assert_eq!(
        frames.last().unwrap().frame_index as usize,
        expected_frames - 1,
        "frame indices must be contiguous and end at the final frame"
    );
    for frame in &frames {
        assert_eq!(adts_frame_length(&frame.bytes), frame.bytes.len());
    }
}
