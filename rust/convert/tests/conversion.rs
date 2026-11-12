mod common;

use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use aac_convert::{
    convert_file, convert_file_m4a_experiment, convert_file_with_format,
    convert_file_with_format_and_tags, AudioProperties, ConvertError, GaplessExperiment,
    OutputFormat,
};
use common::{
    fixture, mp4_audio_specific_config, mp4_chunk_offsets, mp4_elst, mp4_itunsmpb, mp4_mdhd,
    mp4_mvhd, mp4_sample_sizes, mp4_stts, parse_adts, parse_mp4, temp_output_files, TempDir,
};

fn assert_supported_summary(
    input: &str,
    output_name: &str,
    properties: AudioProperties,
    sample_rate_index: u8,
    channel_configuration: u8,
    expected_frames: u64,
) {
    let directory = TempDir::new("supported");
    let output = directory.join(output_name);
    let summary = convert_file(&fixture(input), &output).unwrap();
    assert_eq!(summary.properties, properties);
    assert_eq!(summary.encoded_frames, expected_frames);

    let bytes = fs::read(&output).unwrap();
    assert_eq!(summary.output_bytes, bytes.len() as u64);
    let frames = parse_adts(&bytes);
    assert_eq!(frames.len() as u64, summary.encoded_frames);
    assert_eq!(frames.last().unwrap().end, bytes.len());
    for frame in &frames {
        assert_eq!(frame.profile, 1, "AAC-LC profile");
        assert_eq!(frame.sample_rate_index, sample_rate_index);
        assert_eq!(frame.channel_configuration, channel_configuration);
        assert_eq!(frame.raw_blocks, 0);
    }
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn converts_supported_stereo_48k_fixture() {
    assert_supported_summary(
        "stereo-48k-16.flac",
        "stereo.aac",
        AudioProperties {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 16,
        },
        3,
        2,
        14,
    );
}

#[test]
fn converts_supported_mono_441k_fixture() {
    assert_supported_summary(
        "mono-441k-16.flac",
        "mono.aac",
        AudioProperties {
            sample_rate: 44_100,
            channels: 1,
            bits_per_sample: 16,
        },
        4,
        1,
        5,
    );
}

#[test]
fn accepts_24_bit_input_with_truncation_warning() {
    let directory = TempDir::new("accept-24");
    let output = directory.join("out.aac");
    let (summary, diagnostics) = convert_file_with_format_and_tags(
        &fixture("stereo-48k-24.flac"),
        &output,
        OutputFormat::Adts,
        true,
    )
    .unwrap();
    assert_eq!(summary.properties.bits_per_sample, 24);
    assert_eq!(summary.encoded_frames, 3);
    // Exactly one warning, up front: the precision notice, not tag noise
    // (ADTS carries no tags).
    assert_eq!(
        diagnostics.warnings,
        [
            "24-bit FLAC input is truncated to 16-bit PCM (arithmetic right shift, no dithering) \
             before AAC encoding; low-order source precision is discarded"
        ]
    );
    assert!(output.is_file());
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn accepts_24_bit_m4a_with_warning_and_gapless() {
    let directory = TempDir::new("accept-24-m4a");
    let output = directory.join("out.m4a");
    let (summary, diagnostics) = convert_file_with_format_and_tags(
        &fixture("stereo-48k-24.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(summary.properties.bits_per_sample, 24);
    assert_eq!(summary.encoded_frames, 3);

    // The precision notice leads; absent optional tags are silent, leaving
    // only loudness warnings for the short fixture.
    assert_eq!(
        diagnostics.warnings[0],
        "24-bit FLAC input is truncated to 16-bit PCM (arithmetic right shift, no dithering) \
         before AAC encoding; low-order source precision is discarded"
    );
    assert!(
        !diagnostics.warnings.iter().any(|w| w.contains("missing")),
        "absence of optional tags must be silent: {:?}",
        diagnostics.warnings
    );

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    assert_eq!(&bytes[8..12], b"M4A ");
    assert_eq!(mp4_mdhd(&bytes, root.require("mdhd")), (48_000, 3 * 1024));
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn rejects_corrupt_24_bit_md5_without_publishing_output() {
    // Same STREAMINFO-first layout as the 16-bit MD5 fixtures: `fLaC` magic,
    // 4-byte header, MD5 at byte 26. Flipping one bit must fail verification.
    let directory = TempDir::new("md5-24");
    let input = directory.join("input.flac");
    let mut bytes = fs::read(fixture("stereo-48k-24.flac")).unwrap();
    assert_eq!(&bytes[0..4], b"fLaC");
    bytes[26] ^= 0xFF;
    fs::write(&input, &bytes).unwrap();

    let output = directory.join("out.aac");
    let err = convert_file(&input, &output).unwrap_err();
    assert!(matches!(err, ConvertError::DecodeFailed));
    assert!(!output.exists());
    assert!(
        temp_output_files(directory.path()).is_empty(),
        "temporary output was not cleaned up"
    );
}

#[test]
fn rejects_unsupported_sample_rate() {
    let directory = TempDir::new("reject-rate");
    let output = directory.join("out.aac");
    let err = convert_file(&fixture("mono-32k-16.flac"), &output).unwrap_err();
    assert!(matches!(err, ConvertError::UnsupportedSampleRate(32_000)));
    assert!(!output.exists());
}

#[test]
fn rejects_unsupported_channel_count() {
    let directory = TempDir::new("reject-channels");
    let output = directory.join("out.aac");
    let err = convert_file(&fixture("three-channel-48k-16.flac"), &output).unwrap_err();
    assert!(matches!(err, ConvertError::UnsupportedChannels(3)));
    assert!(!output.exists());
}

#[test]
fn rejects_input_output_collision() {
    let input = fixture("stereo-48k-16.flac");
    let err = convert_file(&input, &input).unwrap_err();
    assert!(matches!(err, ConvertError::SamePath(_)));
}

#[test]
fn rejects_existing_directory_output() {
    let directory = TempDir::new("existing-directory");
    let output = directory.join("out.aac");
    fs::create_dir(&output).unwrap();
    let err = convert_file(&fixture("stereo-48k-16.flac"), &output).unwrap_err();
    assert!(matches!(err, ConvertError::OutputExists(_)));
    assert!(output.is_dir());
    assert!(temp_output_files(directory.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn rejects_symlink_to_input_as_collision() {
    use std::os::unix::fs::symlink;

    let directory = TempDir::new("symlink-input");
    let input = fixture("stereo-48k-16.flac");
    let output = directory.join("out.aac");
    symlink(&input, &output).unwrap();
    let err = convert_file(&input, &output).unwrap_err();
    assert!(matches!(err, ConvertError::SamePath(_)));
    assert!(fs::symlink_metadata(&output)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn protects_existing_output() {
    let directory = TempDir::new("existing");
    let output = directory.join("out.aac");
    fs::write(&output, b"sentinel").unwrap();

    let err = convert_file(&fixture("stereo-48k-16.flac"), &output).unwrap_err();
    assert!(matches!(err, ConvertError::OutputExists(_)));
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn rejects_missing_output_directory() {
    let directory = TempDir::new("missing-dir");
    let output = directory.join("missing").join("out.aac");
    let err = convert_file(&fixture("stereo-48k-16.flac"), &output).unwrap_err();
    assert!(matches!(err, ConvertError::OutputDirectoryMissing(_)));
    assert!(!output.exists());
}

#[test]
fn removes_temporary_file_after_truncated_input() {
    let directory = TempDir::new("truncated");
    let output = directory.join("out.aac");
    let err = convert_file(&fixture("truncated-stereo-48k-16.flac"), &output).unwrap_err();
    assert!(matches!(err, ConvertError::DecodeFailed));
    assert!(!output.exists());
    assert!(
        temp_output_files(directory.path()).is_empty(),
        "temporary output was not cleaned up"
    );
}

#[test]
fn rejects_md5_mismatch_without_publishing_output() {
    let directory = TempDir::new("md5-mismatch");
    let output = directory.join("out.aac");
    let err = convert_file(&fixture("md5-mismatch-stereo-48k-16.flac"), &output).unwrap_err();
    assert!(matches!(err, ConvertError::DecodeFailed));
    assert!(!output.exists());
    assert!(
        temp_output_files(directory.path()).is_empty(),
        "temporary output was not cleaned up"
    );
}

#[test]
fn accepts_valid_flac_with_absent_md5() {
    let directory = TempDir::new("no-md5");
    let no_md5_output = directory.join("no-md5.aac");
    let reference_output = directory.join("reference.aac");

    let summary = convert_file(&fixture("no-md5-stereo-48k-16.flac"), &no_md5_output).unwrap();
    assert_eq!(summary.properties.sample_rate, 48_000);
    assert_eq!(summary.properties.channels, 2);
    assert_eq!(summary.encoded_frames, 14);

    convert_file(&fixture("stereo-48k-16.flac"), &reference_output).unwrap();
    assert_eq!(
        fs::read(&no_md5_output).unwrap(),
        fs::read(&reference_output).unwrap(),
        "absent-MD5 input must not change the encoded audio"
    );
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn rejects_non_flac_input() {
    let directory = TempDir::new("not-flac");
    let input = directory.join("input.flac");
    let output = directory.join("out.aac");
    fs::write(&input, b"not a flac stream").unwrap();
    let err = convert_file(&input, &output).unwrap_err();
    assert!(matches!(err, ConvertError::InputNotFlac(_)));
    assert!(!output.exists());
}

#[test]
fn rejects_empty_input() {
    let directory = TempDir::new("empty");
    let input = directory.join("empty.flac");
    let output = directory.join("out.aac");
    fs::write(&input, b"").unwrap();
    let err = convert_file(&input, &output).unwrap_err();
    assert!(matches!(err, ConvertError::InputNotFlac(_)));
    assert!(!output.exists());
    assert!(temp_output_files(directory.path()).is_empty());
}

fn assert_m4a_structure(
    input: &str,
    output_name: &str,
    properties: AudioProperties,
    expected_frames: u64,
    expected_asc: [u8; 2],
    expected_smpb: &str,
) {
    let directory = TempDir::new("m4a-supported");
    let output = directory.join(output_name);
    let summary = convert_file_with_format(&fixture(input), &output, OutputFormat::M4a).unwrap();
    assert_eq!(summary.properties, properties);
    assert_eq!(summary.encoded_frames, expected_frames);

    let bytes = fs::read(&output).unwrap();
    assert_eq!(summary.output_bytes, bytes.len() as u64);
    let root = parse_mp4(&bytes);
    assert_eq!(root.children.first().unwrap().kind, "ftyp");
    let moov = root.require("moov");

    let (timescale, duration) = mp4_mdhd(&bytes, moov.require("mdhd"));
    assert_eq!(timescale, properties.sample_rate);
    assert_eq!(duration, expected_frames * 1024);

    assert_eq!(
        mp4_stts(&bytes, moov.require("stts")),
        vec![(expected_frames as u32, 1024)]
    );

    let sizes = mp4_sample_sizes(&bytes, moov.require("stsz"));
    assert_eq!(sizes.len() as u64, expected_frames);
    let mdat = root.require("mdat");
    let payload_bytes: u64 = sizes.iter().map(|size| u64::from(*size)).sum();
    assert_eq!(
        payload_bytes,
        (mdat.end - mdat.payload_start()) as u64,
        "sample sizes must exactly cover the mdat payload"
    );

    let offset_box = moov
        .find("stco")
        .or_else(|| moov.find("co64"))
        .expect("chunk offset table");
    assert_eq!(
        mp4_chunk_offsets(&bytes, offset_box),
        vec![mdat.payload_start() as u64],
        "one chunk starting at the first sample payload"
    );

    let mp4a = moov.require("mp4a");
    let entry = &bytes[mp4a.payload_start()..];
    let channels = u16::from_be_bytes(entry[16..18].try_into().unwrap());
    let sample_size = u16::from_be_bytes(entry[18..20].try_into().unwrap());
    let sample_rate = u32::from_be_bytes(entry[24..28].try_into().unwrap()) >> 16;
    assert_eq!(u32::from(channels), properties.channels);
    assert_eq!(sample_size, 16);
    assert_eq!(sample_rate, properties.sample_rate);

    let asc = mp4_audio_specific_config(&bytes, moov.require("esds"));
    assert_eq!(asc, expected_asc);

    // Default M4A carries iTunSMPB gapless signalling with the Apple-style
    // brand and no edit list. The movie timescale equals the media timescale
    // for this single-track file.
    assert_eq!(&bytes[8..12], b"M4A ");
    assert_eq!(
        &bytes[16..28],
        b"M4A mp42isom",
        "compatible brands mirror the Apple reference file"
    );
    assert!(
        moov.require("trak").find("edts").is_none(),
        "default output must not contain an edit list"
    );
    assert_eq!(
        mp4_itunsmpb(&bytes, &root),
        expected_smpb,
        "iTunSMPB must carry (D, P, S) for this fixture"
    );

    assert!(
        temp_output_files(directory.path()).is_empty(),
        "temporary output was not cleaned up"
    );
}

#[test]
fn converts_supported_stereo_48k_fixture_to_m4a() {
    // S = 12000, N = 14, D = 1600, P = 736.
    assert_m4a_structure(
        "stereo-48k-16.flac",
        "stereo.m4a",
        AudioProperties {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 16,
        },
        14,
        [0x11, 0x90],
        " 00000000 00000640 000002E0 0000000000002EE0 \
         00000000 00000000 00000000 00000000 \
         00000000 00000000 00000000 00000000",
    );
}

#[test]
fn converts_supported_mono_441k_fixture_to_m4a() {
    // S = 3000, N = 5, D = 1600, P = 520.
    assert_m4a_structure(
        "mono-441k-16.flac",
        "mono.m4a",
        AudioProperties {
            sample_rate: 44_100,
            channels: 1,
            bits_per_sample: 16,
        },
        5,
        [0x12, 0x08],
        " 00000000 00000640 00000208 0000000000000BB8 \
         00000000 00000000 00000000 00000000 \
         00000000 00000000 00000000 00000000",
    );
}

#[test]
fn m4a_payload_matches_adts_access_units() {
    let directory = TempDir::new("m4a-payload");
    let adts_output = directory.join("reference.aac");
    let m4a_output = directory.join("output.m4a");

    convert_file(&fixture("stereo-48k-16.flac"), &adts_output).unwrap();
    convert_file_with_format(
        &fixture("stereo-48k-16.flac"),
        &m4a_output,
        OutputFormat::M4a,
    )
    .unwrap();

    let adts_bytes = fs::read(&adts_output).unwrap();
    let frames = parse_adts(&adts_bytes);
    assert_eq!(frames.len(), 14);
    let mut expected_payload = Vec::new();
    for frame in &frames {
        expected_payload.extend_from_slice(&adts_bytes[frame.start + 7..frame.end]);
    }

    let m4a_bytes = fs::read(&m4a_output).unwrap();
    let root = parse_mp4(&m4a_bytes);
    let mdat = root.require("mdat");
    assert_eq!(
        &m4a_bytes[mdat.payload_start()..mdat.end],
        expected_payload.as_slice(),
        "M4A mdat must contain exactly the ADTS access units, with no missing final frame"
    );
}

#[test]
fn m4a_rejects_truncated_input_without_publishing() {
    let directory = TempDir::new("m4a-truncated");
    let output = directory.join("out.m4a");
    let err = convert_file_with_format(
        &fixture("truncated-stereo-48k-16.flac"),
        &output,
        OutputFormat::M4a,
    )
    .unwrap_err();
    assert!(matches!(err, ConvertError::DecodeFailed));
    assert!(!output.exists());
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn m4a_rejects_md5_mismatch_without_publishing() {
    let directory = TempDir::new("m4a-md5-mismatch");
    let output = directory.join("out.m4a");
    let err = convert_file_with_format(
        &fixture("md5-mismatch-stereo-48k-16.flac"),
        &output,
        OutputFormat::M4a,
    )
    .unwrap_err();
    assert!(matches!(err, ConvertError::DecodeFailed));
    assert!(!output.exists());
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn m4a_accepts_absent_md5() {
    let directory = TempDir::new("m4a-no-md5");
    let output = directory.join("out.m4a");
    let summary = convert_file_with_format(
        &fixture("no-md5-stereo-48k-16.flac"),
        &output,
        OutputFormat::M4a,
    )
    .unwrap();
    assert_eq!(summary.encoded_frames, 14);
    parse_mp4(&fs::read(&output).unwrap());
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn m4a_rejects_existing_output_without_touching_it() {
    let directory = TempDir::new("m4a-existing");
    let output = directory.join("out.m4a");
    fs::write(&output, b"existing").unwrap();
    let err = convert_file_with_format(&fixture("stereo-48k-16.flac"), &output, OutputFormat::M4a)
        .unwrap_err();
    assert!(matches!(err, ConvertError::OutputExists(_)));
    assert_eq!(fs::read(&output).unwrap(), b"existing");
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn concurrent_m4a_publication_never_replaces_the_winner() {
    let directory = TempDir::new("m4a-race");
    let output = directory.join("out.m4a");
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let input = fixture("stereo-48k-16.flac");
        let output = output.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            convert_file_with_format(&input, &output, OutputFormat::M4a)
        }));
    }
    barrier.wait();

    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "exactly one publication must win: {results:?}"
    );
    assert!(
        results
            .iter()
            .any(|result| matches!(result, Err(ConvertError::OutputExists(_)))),
        "the loser must be rejected as an existing output: {results:?}"
    );

    let bytes = fs::read(&output).unwrap();
    parse_mp4(&bytes);
    assert!(temp_output_files(directory.path()).is_empty());
}

fn with_edit_list() -> GaplessExperiment {
    GaplessExperiment { edit_list: true }
}

/// Default M4A output carries `iTunSMPB` gapless signalling with the
/// Apple-style brand and no edit list, whatever the fixture.
#[test]
fn m4a_default_output_carries_smpb_without_edit_list() {
    for (input, output_name, expected_smpb) in [
        (
            "stereo-48k-16.flac",
            "stereo.m4a",
            " 00000000 00000640 000002E0 0000000000002EE0 \
             00000000 00000000 00000000 00000000 \
             00000000 00000000 00000000 00000000",
        ),
        (
            "mono-441k-16.flac",
            "mono.m4a",
            " 00000000 00000640 00000208 0000000000000BB8 \
             00000000 00000000 00000000 00000000 \
             00000000 00000000 00000000 00000000",
        ),
    ] {
        let directory = TempDir::new("m4a-default-gapless");
        let output = directory.join(output_name);
        convert_file_with_format(&fixture(input), &output, OutputFormat::M4a).unwrap();
        let bytes = fs::read(&output).unwrap();
        let root = parse_mp4(&bytes);
        let moov = root.require("moov");
        assert_eq!(&bytes[8..12], b"M4A ");
        assert!(
            moov.require("trak").find("edts").is_none(),
            "{input}: default output must not contain an edit list"
        );
        assert_eq!(mp4_itunsmpb(&bytes, &root), expected_smpb);
        assert!(temp_output_files(directory.path()).is_empty());
    }
}

/// Experimental edit list for the 12,000-frame stereo fixture, added on top
/// of the default `iTunSMPB` signalling: `S = 12000`, `N = 14`, `D = 1600`,
/// `P = 736`.
#[test]
fn m4a_experiment_combined_matches_accounting() {
    let directory = TempDir::new("m4a-experiment-combined");
    let output = directory.join("stereo.m4a");
    let summary =
        convert_file_m4a_experiment(&fixture("stereo-48k-16.flac"), &output, with_edit_list())
            .unwrap();
    assert_eq!(summary.encoded_frames, 14);

    let bytes = fs::read(&output).unwrap();
    assert_eq!(summary.output_bytes, bytes.len() as u64);
    let root = parse_mp4(&bytes);
    let moov = root.require("moov");

    // `iTunSMPB` output carries the Apple-style brand CoreAudio requires.
    assert_eq!(&bytes[8..12], b"M4A ");
    assert_eq!(
        &bytes[16..28],
        b"M4A mp42isom",
        "compatible brands mirror the Apple reference file"
    );

    // The movie timescale equals the media timescale for this single-track
    // file, so the edit durations below are source-frame counts directly.
    assert_eq!(mp4_mvhd(&bytes, moov.require("mvhd")), (48_000, 14_336));
    // Sample tables and media duration are untouched by the signalling.
    assert_eq!(mp4_mdhd(&bytes, moov.require("mdhd")), (48_000, 14_336));
    assert_eq!(mp4_stts(&bytes, moov.require("stts")), vec![(14, 1024)]);
    let sizes = mp4_sample_sizes(&bytes, moov.require("stsz"));
    assert_eq!(sizes.len(), 14);

    assert_eq!(
        mp4_elst(&bytes, moov.require("elst")),
        vec![(12_000, 1600, 0x0001_0000)],
        "single edit trimming D priming samples with duration S at rate 1.0"
    );
    assert_eq!(
        mp4_itunsmpb(&bytes, &root),
        " 00000000 00000640 000002E0 0000000000002EE0 \
         00000000 00000000 00000000 00000000 \
         00000000 00000000 00000000 00000000",
        "iTunSMPB must carry (D, P, S) = (1600, 736, 12000)"
    );

    // The AAC payloads are byte-identical to the default ADTS path.
    let adts_output = directory.join("reference.aac");
    convert_file(&fixture("stereo-48k-16.flac"), &adts_output).unwrap();
    let adts_bytes = fs::read(&adts_output).unwrap();
    let mut expected_payload = Vec::new();
    for frame in parse_adts(&adts_bytes).iter() {
        expected_payload.extend_from_slice(&adts_bytes[frame.start + 7..frame.end]);
    }
    let mdat = root.require("mdat");
    assert_eq!(
        &bytes[mdat.payload_start()..mdat.end],
        expected_payload.as_slice(),
        "signalling must not alter the AAC access units"
    );
    let payload_bytes: u64 = sizes.iter().map(|size| u64::from(*size)).sum();
    assert_eq!(payload_bytes, (mdat.end - mdat.payload_start()) as u64);

    assert!(temp_output_files(directory.path()).is_empty());
}

/// Experimental edit list for the 3,000-frame mono fixture (`P = 520`),
/// added on top of the default `iTunSMPB` signalling.
#[test]
fn m4a_experiment_edit_list_adds_elst_to_default_signalling() {
    let directory = TempDir::new("m4a-experiment-elst");
    let output = directory.join("mono.m4a");
    let summary =
        convert_file_m4a_experiment(&fixture("mono-441k-16.flac"), &output, with_edit_list())
            .unwrap();
    assert_eq!(summary.encoded_frames, 5);

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let moov = root.require("moov");
    assert_eq!(mp4_mvhd(&bytes, moov.require("mvhd")), (44_100, 5_120));
    assert_eq!(
        mp4_elst(&bytes, moov.require("elst")),
        vec![(3000, 1600, 0x0001_0000)]
    );
    // The default signalling is still present underneath the experiment.
    assert_eq!(&bytes[8..12], b"M4A ");
    assert_eq!(
        mp4_itunsmpb(&bytes, &root),
        " 00000000 00000640 00000208 0000000000000BB8 \
         00000000 00000000 00000000 00000000 \
         00000000 00000000 00000000 00000000"
    );
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn m4a_experiment_rejects_existing_output_without_touching_it() {
    let directory = TempDir::new("m4a-experiment-existing");
    let output = directory.join("out.m4a");
    fs::write(&output, b"existing").unwrap();
    let err = convert_file_m4a_experiment(
        &fixture("stereo-48k-16.flac"),
        &output,
        GaplessExperiment { edit_list: false },
    )
    .unwrap_err();
    assert!(matches!(err, ConvertError::OutputExists(_)));
    assert_eq!(fs::read(&output).unwrap(), b"existing");
    assert!(temp_output_files(directory.path()).is_empty());
}
#[test]
fn m4a_experiment_rejects_truncated_input_without_publishing() {
    let directory = TempDir::new("m4a-experiment-truncated");
    let output = directory.join("out.m4a");
    let err = convert_file_m4a_experiment(
        &fixture("truncated-stereo-48k-16.flac"),
        &output,
        with_edit_list(),
    )
    .unwrap_err();
    assert!(matches!(err, ConvertError::DecodeFailed));
    assert!(!output.exists());
    assert!(temp_output_files(directory.path()).is_empty());
}
