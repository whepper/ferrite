//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! Production f32 transform/corpus regression pins.
//!
//! This test is deliberately independent of the removed f64 experiment. It
//! encodes the full in-repo corpus through the default `StreamEncoder` (the
//! same path used by the production `Encoder`) and pins both the exact ADTS
//! byte length and an FNV-1a 64 hash of every stream against the established
//! V3 baseline. Any change to the default transform, psychoacoustic decisions,
//! rate control, or frame assembly that moves output must be an explicit test
//! update, not an unnoticed regression.
//!
//! Expected values are from the retained V3 baseline
//! (`tools/aac-enc-metrics/fixtures-v3.json` and the M9 evidence set). The
//! V3 run also verified that the default build reproduces the historical
//! baseline byte-for-byte on all 19 fixtures on the recorded reference
//! platform (Apple Silicon macOS). Byte lengths are pinned on every platform;
//! exact hashes are enforced only on that reference platform because the
//! float encoder's least-significant decisions are not a portable
//! cross-architecture bit-pattern guarantee.

use super::config::{EncoderConfig, TransportType, VbrMode, FRAME_LENGTH};
use super::pcm::PcmFeeder;
use super::stream_driver::StreamEncoder;
use super::testdata;

/// (fixture key, C/ADTS stem, sample rate, channels, expected bytes, FNV-1a 64)
pub(crate) const BASELINE: [(&str, &str, u32, usize, usize, u64); 19] = [
    (
        "silence-stereo",
        "m2-silence-stereo-48k",
        48000,
        2,
        735,
        0x7713_057f_645e_a480,
    ),
    (
        "sine-stereo",
        "m2-sine-stereo-48k",
        48000,
        2,
        3147,
        0x8921_b76c_2eb4_297e,
    ),
    (
        "onset-stereo",
        "m2-onset-stereo-48k",
        48000,
        2,
        2359,
        0x2a6c_a107_a2f0_ba5d,
    ),
    (
        "stepdown-stereo",
        "m2-stepdown-stereo-48k",
        48000,
        2,
        2236,
        0xc5e3_cf81_d7c9_051d,
    ),
    (
        "click-mono",
        "m2-click-mono-48k",
        48000,
        1,
        2680,
        0x2c5e_9650_bd12_691c,
    ),
    (
        "impulse-mono",
        "m2-impulse-mono-48k",
        48000,
        1,
        306,
        0x0fef_4f3c_59ab_dbec,
    ),
    (
        "edge-mono",
        "m2-edge-mono-48k",
        48000,
        1,
        415,
        0x8d1d_7ff2_a90b_e28c,
    ),
    (
        "multitone-stereo",
        "m3-multitone-stereo-48k",
        48000,
        2,
        4819,
        0x2777_d738_0edf_b4e1,
    ),
    (
        "sweep-stereo",
        "m3-sweep-stereo-48k",
        48000,
        2,
        3988,
        0x264a_1b8e_77b4_69b7,
    ),
    (
        "noise-mono",
        "m3-noise-mono-48k",
        48000,
        1,
        10603,
        0x1d40_2183_d5f6_ea47,
    ),
    (
        "tonal-noise-stereo",
        "m3-tonal-noise-stereo-48k",
        48000,
        2,
        10630,
        0xdcc1_5ccc_2963_940c,
    ),
    (
        "quiet-sine-stereo",
        "m3-quiet-sine-stereo-48k",
        48000,
        2,
        4200,
        0xa1b6_c369_cc1b_8450,
    ),
    (
        "sine-stereo-44100",
        "m3-sine-stereo-441k",
        44100,
        2,
        2998,
        0x1ba2_1b60_517c_9291,
    ),
    (
        "onset-stereo-44100",
        "m3-onset-stereo-441k",
        44100,
        2,
        2250,
        0x7b3c_e9ae_a13d_7c47,
    ),
    (
        "stress-stereo",
        "m6-stress-stereo-48k",
        48000,
        2,
        10728,
        0x57fd_0bbb_44c9_60d8,
    ),
    (
        "music-stereo",
        "m7-music-stereo-48k",
        48000,
        2,
        250926,
        0x151e_37b9_67d5_2678,
    ),
    (
        "speech-pauses-stereo",
        "m7-speech-pauses-stereo-48k",
        48000,
        2,
        125818,
        0xe647_71cc_5687_e56b,
    ),
    (
        "speech-clean-stereo",
        "m7-speech-clean-stereo-48k",
        48000,
        2,
        127900,
        0xea31_83de_57cb_f39f,
    ),
    (
        "music-eroica-stereo",
        "m8-music-eroica-stereo-48k",
        48000,
        2,
        234229,
        0x3c40_aaae_9307_8019,
    ),
];

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn interleave(pcm: &[Vec<i16>]) -> Vec<i16> {
    let s = pcm[0].len();
    let ch = pcm.len();
    let mut out = Vec::with_capacity(s * ch);
    for i in 0..s {
        for c in pcm.iter().take(ch) {
            out.push(c[i]);
        }
    }
    out
}

#[test]
fn production_f32_corpus_baseline_is_unchanged() {
    for (short, stem, sr, ch, want_bytes, want_hash) in BASELINE {
        let pcm = testdata::fixture_samples(short);
        assert_eq!(pcm.len(), ch, "{stem}: channel count");
        let cfg =
            EncoderConfig::new(sr, ch as u32, FRAME_LENGTH, TransportType::Adts).expect("cfg");
        let mut feeder = PcmFeeder::new(cfg);
        let mut frames = feeder
            .push_interleaved(&interleave(&pcm))
            .expect("feeder push");
        frames.extend(feeder.finish().expect("feeder finish"));
        let want_frames = (pcm[0].len() + 1600).div_ceil(FRAME_LENGTH);
        assert_eq!(frames.len(), want_frames, "{stem}: feeder frame law");

        let mut enc = StreamEncoder::with_mode(sr, ch, VbrMode::Vbr5);
        let mut adts = Vec::with_capacity(want_bytes);
        for (i, fr) in frames.iter().enumerate() {
            let fresh: Vec<Vec<i16>> = fr.channels().iter().map(|b| b.to_vec()).collect();
            let (bytes, snap) = enc.step_frame(i as u32, &fresh);
            assert!(snap.notes.is_empty(), "{stem} f{i}: notes {:?}", snap.notes);
            assert_eq!(snap.crash, -1, "{stem} f{i}: crash marker");
            let bytes = bytes.unwrap_or_else(|| panic!("{stem} f{i}: no bytes"));
            adts.extend_from_slice(&bytes);
        }
        assert_eq!(adts.len(), want_bytes, "{stem}: baseline byte length");
        // The V3 baseline was recorded on Apple Silicon macOS. Pin exact
        // content there; elsewhere keep the platform-neutral length/frame
        // checks and report that the exact hash was not enforced.
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(fnv1a64(&adts), want_hash, "{stem}: baseline ADTS hash");
        } else {
            eprintln!(
                "{stem}: exact ADTS hash not enforced on this target; \
                 byte length and frame count pinned"
            );
        }
    }
}

/// Default-profile (VBR-6) output pin for the production music fixture,
/// mirroring the `aac_bench_wav` example expectations. The VBR-5 corpus
/// baseline above pins the upstream-compatible profile through an explicit
/// `with_mode` selection; this pins what the *default* constructor produces,
/// so an accidental default-profile change is an explicit test update.
/// Recorded with the M7 audit sweep (2026-11-13, same reference platform as
/// the V3 baseline).
#[test]
fn production_default_profile_vbr6_music_baseline_is_unchanged() {
    let pcm = testdata::fixture_samples("music-stereo");
    assert_eq!(pcm.len(), 2, "music-stereo: channel count");
    let (sr, want_bytes, want_hash) = (48_000u32, 256_763, 0xd6fc_9d2c_59cf_5c36);
    let mut enc = StreamEncoder::new(sr, pcm.len());
    assert_eq!(enc.vbr_mode(), VbrMode::Vbr6, "default profile is VBR-6");
    let mut feeder = PcmFeeder::new(
        EncoderConfig::new(sr, pcm.len() as u32, FRAME_LENGTH, TransportType::Adts).expect("cfg"),
    );
    let frames = feeder
        .push_interleaved(&interleave(&pcm))
        .expect("feeder push");
    let mut frames = frames;
    frames.extend(feeder.finish().expect("feeder finish"));
    let want_frames = (pcm[0].len() + 1600).div_ceil(FRAME_LENGTH);
    assert_eq!(frames.len(), want_frames, "music-stereo: feeder frame law");
    let mut adts = Vec::with_capacity(want_bytes);
    for (i, fr) in frames.iter().enumerate() {
        let fresh: Vec<Vec<i16>> = fr.channels().iter().map(|b| b.to_vec()).collect();
        let (bytes, snap) = enc.step_frame(i as u32, &fresh);
        assert!(snap.notes.is_empty(), "f{i}: notes {:?}", snap.notes);
        assert_eq!(snap.crash, -1, "f{i}: crash marker");
        let bytes = bytes.unwrap_or_else(|| panic!("f{i}: no bytes"));
        adts.extend_from_slice(&bytes);
    }
    assert_eq!(adts.len(), want_bytes, "VBR-6 default byte length");
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert_eq!(fnv1a64(&adts), want_hash, "VBR-6 default ADTS hash");
    } else {
        eprintln!(
            "music-stereo: exact VBR-6 hash not enforced on this target; \
             byte length and frame count pinned"
        );
    }
}
