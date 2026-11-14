//! Fast-start (progressive-download) M4A layout tests.
//!
//! Validates actual MP4 box order, not configuration: every M4A produced by
//! the normal conversion path must place `moov` before `mdat` so players can
//! access container metadata without downloading the entire file. Covers
//! plain output, embedded artwork, large artwork (1 MiB sibling `cover.jpg`
//! exercising the relocation path with a sizable `moov`), tagged output, and
//! the experimental edit-list path. ADTS output must remain unchanged.

mod common;

use std::fs;

use aac_convert::{
    convert_file, convert_file_m4a_experiment, convert_file_with_format,
    convert_file_with_format_and_tags, convert_file_with_format_tags_and_cover_art, CoverArtMode,
    GaplessExperiment, OutputFormat,
};
use common::{
    fixture, mp4_audio_specific_config, mp4_chunk_offsets, mp4_ilst_cover, mp4_itunsmpb, mp4_mdhd,
    mp4_sample_sizes, mp4_stts, parse_adts, parse_mp4, temp_output_files, TempDir,
};

/// Top-level box kinds in file order.
fn top_box_kinds(bytes: &[u8]) -> Vec<String> {
    parse_mp4(bytes)
        .children
        .iter()
        .map(|b| b.kind.clone())
        .collect()
}

/// Assert fast-start order and return the parsed root for further checks.
fn assert_faststart(bytes: &[u8]) -> common::Mp4BoxInfo {
    let root = parse_mp4(bytes);
    let kinds = top_box_kinds(bytes);
    assert_eq!(
        kinds.first().map(String::as_str),
        Some("ftyp"),
        "first box must be ftyp: {kinds:?}"
    );
    let moov_pos = kinds.iter().position(|k| k == "moov").expect("no moov box");
    let mdat_pos = kinds.iter().position(|k| k == "mdat").expect("no mdat box");
    assert!(
        moov_pos < mdat_pos,
        "moov must precede mdat for fast-start: {kinds:?}"
    );
    // Relocation drops the muxer's placeholder `free` box: fast-start files
    // are exactly ftyp + moov + mdat.
    assert_eq!(
        kinds,
        vec!["ftyp".to_string(), "moov".to_string(), "mdat".to_string()],
        "fast-start layout must be ftyp/moov/mdat with no free box: {kinds:?}"
    );
    root
}

#[test]
fn plain_m4a_is_faststart_with_valid_structure() {
    let directory = TempDir::new("faststart-plain");
    let output = directory.join("plain.m4a");
    let summary =
        convert_file_with_format(&fixture("stereo-48k-16.flac"), &output, OutputFormat::M4a)
            .unwrap();
    assert_eq!(summary.encoded_frames, 14);

    let bytes = fs::read(&output).unwrap();
    assert_eq!(summary.output_bytes, bytes.len() as u64);
    let root = assert_faststart(&bytes);
    let moov = root.require("moov");

    // Container contract unchanged: brands, timeline, sample tables.
    assert_eq!(&bytes[8..12], b"M4A ");
    assert_eq!(mp4_mdhd(&bytes, moov.require("mdhd")), (48_000, 14 * 1024));
    assert_eq!(
        mp4_stts(&bytes, moov.require("stts")),
        vec![(14u32, 1024u32)]
    );
    let sizes = mp4_sample_sizes(&bytes, moov.require("stsz"));
    assert_eq!(sizes.len(), 14);
    let mdat = root.require("mdat");
    let payload_bytes: u64 = sizes.iter().map(|s| u64::from(*s)).sum();
    assert_eq!(
        payload_bytes,
        (mdat.end - mdat.payload_start()) as u64,
        "sample sizes must cover the mdat payload"
    );
    assert_eq!(
        mp4_chunk_offsets(&bytes, moov.require("stco")),
        vec![mdat.payload_start() as u64],
        "single chunk must start at the mdat payload"
    );
    assert_eq!(
        mp4_audio_specific_config(&bytes, moov.require("esds")),
        vec![0x11, 0x90]
    );
    assert!(!mp4_itunsmpb(&bytes, &root).is_empty());
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn mdat_payload_matches_adts_access_units() {
    let directory = TempDir::new("faststart-payload");
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
    let mut expected = Vec::new();
    for frame in parse_adts(&adts_bytes) {
        expected.extend_from_slice(&adts_bytes[frame.start + 7..frame.end]);
    }
    let m4a_bytes = fs::read(&m4a_output).unwrap();
    let root = assert_faststart(&m4a_bytes);
    let mdat = root.require("mdat");
    assert_eq!(
        &m4a_bytes[mdat.payload_start()..mdat.end],
        expected.as_slice(),
        "relocation must preserve AAC packets byte-for-byte"
    );
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn embedded_artwork_m4a_is_faststart() {
    let directory = TempDir::new("faststart-cover");
    let output = directory.join("cover.m4a");
    let (summary, _) = convert_file_with_format_and_tags(
        &fixture("tagged-cover-jpeg-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(summary.encoded_frames, 14);

    let bytes = fs::read(&output).unwrap();
    let root = assert_faststart(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    let (data_type, payload) = mp4_ilst_cover(&bytes, list).expect("covr must survive relocation");
    assert_eq!(data_type, 13);
    assert!(!payload.is_empty());
    // Audio still intact after moving moov front.
    let moov = root.require("moov");
    assert_eq!(mp4_mdhd(&bytes, moov.require("mdhd")), (48_000, 14 * 1024));
    assert_eq!(mp4_sample_sizes(&bytes, moov.require("stsz")).len(), 14);
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn large_sibling_artwork_m4a_is_faststart() {
    // 1 MiB JPEG exercises the relocation path with a sizable moov: the
    // artwork is discovered as a sibling file and embedded, so no FLAC
    // rewriting is needed and the test stays within the 16 MiB cover cap.
    let workspace = TempDir::new("faststart-large-cover");
    let input_dir = workspace.join("in");
    std::fs::create_dir_all(&input_dir).unwrap();
    let input = input_dir.join("track.flac");
    fs::copy(fixture("stereo-48k-16.flac"), &input).unwrap();
    let mut cover = vec![0xFF, 0xD8, 0xFF, 0xE0];
    cover.resize(1024 * 1024, 0x00);
    fs::write(input_dir.join("cover.jpg"), &cover).unwrap();
    let output = workspace.join("out.m4a");

    let (summary, _, _) = convert_file_with_format_tags_and_cover_art(
        &input,
        &output,
        OutputFormat::M4a,
        true,
        CoverArtMode::Embedded,
    )
    .unwrap();
    assert_eq!(summary.encoded_frames, 14);

    let bytes = fs::read(&output).unwrap();
    let root = assert_faststart(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    let (data_type, payload) = mp4_ilst_cover(&bytes, list).expect("large covr must be present");
    assert_eq!(data_type, 13);
    assert_eq!(payload.len(), cover.len());
    assert_eq!(payload, cover);
    // Moov must actually be large (cover + tables), proving the test
    // exercised a sizable relocation, not just the small-fixture path.
    let moov = root.require("moov");
    assert!(
        moov.end - moov.start > 512 * 1024,
        "moov must carry the 1 MiB cover: {} bytes",
        moov.end - moov.start
    );
    // Offsets and audio preserved.
    let mdat = root.require("mdat");
    assert_eq!(
        mp4_chunk_offsets(&bytes, moov.require("stco")),
        vec![mdat.payload_start() as u64]
    );
    assert_eq!(mp4_sample_sizes(&bytes, moov.require("stsz")).len(), 14);
    assert!(temp_output_files(workspace.path()).is_empty());
}

#[test]
fn tagged_m4a_is_faststart() {
    let directory = TempDir::new("faststart-tagged");
    let output = directory.join("tagged.m4a");
    let (_, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-core-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(diagnostics.mapped, 10);

    let bytes = fs::read(&output).unwrap();
    let root = assert_faststart(&bytes);
    assert!(!mp4_itunsmpb(&bytes, &root).is_empty());
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn experiment_edit_list_m4a_is_faststart() {
    let directory = TempDir::new("faststart-experiment");
    let output = directory.join("experiment.m4a");
    convert_file_m4a_experiment(
        &fixture("stereo-48k-16.flac"),
        &output,
        GaplessExperiment { edit_list: true },
    )
    .unwrap();

    let bytes = fs::read(&output).unwrap();
    let root = assert_faststart(&bytes);
    assert!(
        root.require("trak").find("edts").is_some(),
        "experimental edit list must survive relocation"
    );
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn adts_output_is_unchanged() {
    let directory = TempDir::new("faststart-adts");
    let output = directory.join("out.aac");
    let summary = convert_file(&fixture("stereo-48k-16.flac"), &output).unwrap();
    assert_eq!(summary.encoded_frames, 14);

    let bytes = fs::read(&output).unwrap();
    let frames = parse_adts(&bytes);
    assert_eq!(frames.len(), 14);
    assert_eq!(frames.last().unwrap().end, bytes.len());
    // No MP4 container in ADTS output.
    assert!(
        !bytes.windows(4).any(|w| w == b"moov"),
        "ADTS must not contain a moov box"
    );
    assert!(
        !bytes.windows(4).any(|w| w == b"mdat"),
        "ADTS must not contain an mdat box"
    );
    assert!(temp_output_files(directory.path()).is_empty());
}
