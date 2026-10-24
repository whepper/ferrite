//! S1.3 cover-art integration tests.
//!
//! Cover art rides the existing raw `ilst` path: one `covr` child appended
//! after the text atoms, selected deterministically by picture-usage tier.
//! These tests exercise the real `meta.rs → extra_ilst → finish` path end to
//! end (byte-surgery is not accepted as evidence here).

mod common;

use std::fs;
use std::process::Command;

use aac_convert::batch::{execute_plan, plan_batch, BatchFileOutcome};
use aac_convert::{convert_file_with_format_and_tags, OutputFormat};
use common::{
    fixture, mp4_ilst_children, mp4_ilst_cover, mp4_ilst_text, mp4_itunsmpb, mp4_mdhd, parse_adts,
    parse_mp4, temp_output_files, TempDir,
};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_aac-convert")
}

fn run_cli(args: &[&str]) -> std::process::Output {
    Command::new(binary()).args(args).output().unwrap()
}

/// Independent picture read-back through Symphonia (not the converter):
/// `(usage debug, raw image bytes)` in emission order.
fn flac_visuals(path: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let file = fs::File::open(path).unwrap();
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("flac");
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    // Narrow scope so the returned data is owned, not borrowed from the probe.
    let visuals = {
        let mut metadata = format.metadata();
        let revision = metadata.skip_to_latest().expect("metadata revision");
        revision
            .media
            .visuals
            .iter()
            .map(|v| (format!("{:?}", v.usage), v.data.to_vec()))
            .collect::<Vec<_>>()
    };
    visuals
}

/// Copy a committed fixture into a tree, creating parent directories.
fn seed_file(root: &std::path::Path, name: &str, fixture_name: &str) -> std::path::PathBuf {
    let destination = root.join(name);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::copy(fixture(fixture_name), &destination).unwrap();
    destination
}

/// `ilst` child types in file order, as raw 4-byte tags.
fn ilst_types(bytes: &[u8]) -> Vec<[u8; 4]> {
    let root = parse_mp4(bytes);
    let ilst = root.require("udta").require("meta").require("ilst");
    mp4_ilst_children(bytes, ilst)
        .into_iter()
        .map(|(kind, _)| kind)
        .collect()
}

fn stdout_lines(output: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn jpeg_cover_round_trip() {
    // The fixture carries a front-cover JPEG plus title/artist tags.
    let expectations = flac_visuals(&fixture("tagged-cover-jpeg-48k-16.flac"));
    assert_eq!(expectations.len(), 1);
    assert!(expectations[0].0.contains("FrontCover"), "{expectations:?}");
    let expected_bytes = &expectations[0].1;

    let directory = TempDir::new("cover-jpeg");
    let output = directory.join("cover.m4a");
    let (summary, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-cover-jpeg-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(summary.encoded_frames, 14);
    // `mapped` stays a text-tag count: cover art is excluded by design.
    assert_eq!(diagnostics.mapped, 2);
    assert!(
        diagnostics.unmapped.is_empty(),
        "{:?}",
        diagnostics.unmapped
    );
    // Valid cover: no cover warnings at all (loudness warnings, which name
    // the fixture file, are out of scope for this assertion).
    assert!(
        !diagnostics
            .warnings
            .iter()
            .filter(|w| !w.starts_with("loudness:"))
            .any(|w| w.contains("cover")),
        "{:?}",
        diagnostics.warnings
    );

    let bytes = fs::read(&output).unwrap();
    assert_eq!(summary.output_bytes, bytes.len() as u64);
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");

    // `covr` is last, after the text atoms.
    let types = ilst_types(&bytes);
    assert_eq!(types.last(), Some(b"covr"), "{types:?}");
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9nam"), vec!["Cover Title"]);
    let (data_type, payload) = mp4_ilst_cover(&bytes, list).expect("covr must be present");
    assert_eq!(data_type, 13, "JPEG data type");
    assert_eq!(&payload, expected_bytes, "original image bytes unchanged");

    // Container contract unchanged: brands, no edit list, encoded timeline.
    assert_eq!(&bytes[8..12], b"M4A ");
    assert!(root.require("trak").find("edts").is_none());
    assert_eq!(mp4_mdhd(&bytes, root.require("mdhd")), (48_000, 14 * 1024));

    // AAC payloads identical to the ADTS access units for the same audio.
    let adts_dir = TempDir::new("cover-jpeg-adts");
    let adts_out = adts_dir.join("cover.aac");
    convert_file_with_format_and_tags(
        &fixture("tagged-cover-jpeg-48k-16.flac"),
        &adts_out,
        OutputFormat::Adts,
        true,
    )
    .unwrap();
    let adts = fs::read(&adts_out).unwrap();
    let mut expected_payload = Vec::new();
    for frame in parse_adts(&adts) {
        expected_payload.extend_from_slice(&adts[frame.start + 7..frame.end]);
    }
    let mdat = root.require("mdat");
    assert_eq!(
        &bytes[mdat.payload_start()..mdat.end],
        expected_payload.as_slice()
    );
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn png_cover_round_trip() {
    let expectations = flac_visuals(&fixture("tagged-cover-png-48k-16.flac"));
    assert_eq!(expectations.len(), 1);
    let expected_bytes = &expectations[0].1;

    let directory = TempDir::new("cover-png");
    let output = directory.join("cover.m4a");
    let (_, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-cover-png-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(diagnostics.mapped, 1);

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    let (data_type, payload) = mp4_ilst_cover(&bytes, list).expect("covr must be present");
    assert_eq!(data_type, 14, "PNG data type");
    assert_eq!(&payload, expected_bytes, "original image bytes unchanged");
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn unsupported_format_skipped_with_warning() {
    let expectations = flac_visuals(&fixture("tagged-cover-unsupported-48k-16.flac"));
    assert_eq!(expectations.len(), 1);

    let directory = TempDir::new("cover-unsupported");
    let output = directory.join("cover.m4a");
    let (summary, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-cover-unsupported-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    // Audio converts fine; the GIF is skipped, then reported missing.
    assert_eq!(summary.encoded_frames, 14);
    assert_eq!(diagnostics.mapped, 1);
    assert!(
        diagnostics
            .warnings
            .iter()
            .any(|w| w.contains("unsupported cover image format: skipped")),
        "{:?}",
        diagnostics.warnings
    );
    assert!(
        diagnostics.warnings.iter().any(|w| w == "missing cover"),
        "{:?}",
        diagnostics.warnings
    );

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    assert_eq!(mp4_ilst_cover(&bytes, list), None);
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9nam"), vec!["Cover Title"]);
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn legacy_coverart_comment_falls_back_to_covr() {
    // No top-level PICTURE block: only a legacy COVERART comment. Symphonia
    // surfaces it as an untyped visual; selection must still embed it, and
    // the raw comment key must not leak into the unmapped tag count.
    let expectations = flac_visuals(&fixture("tagged-cover-legacy-48k-16.flac"));
    assert_eq!(expectations.len(), 1, "{expectations:?}");
    assert!(expectations[0].0.contains("None"), "{expectations:?}");
    let expected_bytes = &expectations[0].1;

    let directory = TempDir::new("cover-legacy");
    let output = directory.join("cover.m4a");
    let (_, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-cover-legacy-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(diagnostics.mapped, 1);
    assert!(
        diagnostics.unmapped.is_empty(),
        "{:?}",
        diagnostics.unmapped
    );

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    let (data_type, payload) = mp4_ilst_cover(&bytes, list).expect("covr must be present");
    assert_eq!(data_type, 13, "COVERART payload is JPEG");
    assert_eq!(&payload, expected_bytes);
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn front_cover_beats_earlier_back_cover() {
    // Fixture order is [back JPEG, front PNG]: tier beats encounter order.
    let expectations = flac_visuals(&fixture("tagged-cover-multi-48k-16.flac"));
    assert_eq!(expectations.len(), 2, "{expectations:?}");
    let png_bytes = &expectations[1].1;

    let directory = TempDir::new("cover-multi");
    let output = directory.join("cover.m4a");
    let (_, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-cover-multi-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert!(
        !diagnostics
            .warnings
            .iter()
            .filter(|w| !w.starts_with("loudness:"))
            .any(|w| w.contains("cover")),
        "{:?}",
        diagnostics.warnings
    );

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    let (data_type, payload) = mp4_ilst_cover(&bytes, list).expect("covr must be present");
    assert_eq!(data_type, 14, "front PNG wins over earlier back JPEG");
    assert_eq!(&payload, png_bytes);
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn cli_no_tags_omits_covr_and_suppresses_cover_warnings() {
    let workspace = TempDir::new("cli-cover-notags");
    let output = workspace.join("out.m4a");
    let result = run_cli(&[
        "--no-tags",
        fixture("tagged-cover-jpeg-48k-16.flac").to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    // No warnings at all: tag warnings and cover warnings are both suppressed.
    assert!(
        String::from_utf8_lossy(&result.stderr).is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    assert_eq!(ilst_types(&bytes), vec![*b"----"]);

    // Gapless signalling is preserved exactly: the same conversion with tags
    // carries an identical iTunSMPB value, only with user atoms appended.
    let tagged = workspace.join("tagged.m4a");
    let tagged_result = run_cli(&[
        fixture("tagged-cover-jpeg-48k-16.flac").to_str().unwrap(),
        tagged.to_str().unwrap(),
    ]);
    assert!(tagged_result.status.success());
    let tagged_bytes = fs::read(&tagged).unwrap();
    assert_eq!(
        mp4_itunsmpb(&bytes, &parse_mp4(&bytes)),
        mp4_itunsmpb(&tagged_bytes, &parse_mp4(&tagged_bytes)),
        "iTunSMPB must be identical with and without --no-tags"
    );
    assert!(temp_output_files(workspace.path()).is_empty());
}

#[test]
fn adts_ignores_pictures_entirely() {
    let directory = TempDir::new("cover-adts");
    let tagged = directory.join("tagged.aac");
    let plain = directory.join("plain.aac");
    let (_, tagged_diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-cover-jpeg-48k-16.flac"),
        &tagged,
        OutputFormat::Adts,
        true,
    )
    .unwrap();
    aac_convert::convert_file(&fixture("stereo-48k-16.flac"), &plain).unwrap();

    // Same audio, byte-identical ADTS: pictures never reach the ADTS path,
    // and no warnings are produced for them either.
    assert_eq!(fs::read(&tagged).unwrap(), fs::read(&plain).unwrap());
    assert!(
        tagged_diagnostics.warnings.is_empty(),
        "{tagged_diagnostics:?}"
    );
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn batch_converts_mixed_picture_and_plain_inputs() {
    let workspace = TempDir::new("batch-cover");
    let input = workspace.join("in");
    seed_file(&input, "plain.flac", "stereo-48k-16.flac");
    seed_file(&input, "cover.flac", "tagged-cover-jpeg-48k-16.flac");
    let output = workspace.join("out");

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(plan.entries.len(), 2);
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
    assert_eq!(outcome.summary.converted, 2);
    assert_eq!(outcome.summary.failed, 0);

    for result in &outcome.results {
        let bytes = fs::read(output.join(format!(
            "{}.m4a",
            result.relative_input.file_stem().unwrap().to_str().unwrap()
        )))
        .unwrap();
        let root = parse_mp4(&bytes);
        let list = root.require("udta").require("meta").require("ilst");
        match &result.outcome {
            BatchFileOutcome::Converted { .. } => {
                if result.relative_input.ends_with("cover.flac") {
                    assert!(mp4_ilst_cover(&bytes, list).is_some());
                } else {
                    assert_eq!(mp4_ilst_cover(&bytes, list), None);
                }
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
    assert!(temp_output_files(&output).is_empty());
}

#[test]
fn cli_batch_cover_dry_run_plans_without_warnings() {
    let workspace = TempDir::new("cli-cover-dryrun");
    let input = workspace.join("in");
    seed_file(&input, "cover.flac", "tagged-cover-jpeg-48k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[
        "--dry-run",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    let lines = stdout_lines(&result);
    assert!(
        lines.iter().any(|l| l == "plan cover.flac -> cover.m4a"),
        "{lines:?}"
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).is_empty(),
        "dry runs convert nothing and warn about nothing: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!output.exists(), "dry run must create nothing");
}

#[test]
fn cover_24bit_combined_reports_precision_first() {
    // F2: a 24-bit input with cover art exercises both warnings.
    let expectations = flac_visuals(&fixture("cover-jpeg-48k-24.flac"));
    assert_eq!(expectations.len(), 1, "{expectations:?}");
    let expected_bytes = &expectations[0].1;

    let directory = TempDir::new("cover-24bit");
    let output = directory.join("cover.m4a");
    let (summary, diagnostics) = convert_file_with_format_and_tags(
        &fixture("cover-jpeg-48k-24.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(summary.properties.bits_per_sample, 24);
    assert_eq!(summary.encoded_frames, 3);

    // Required order: (1) precision notice, (2) text-tag warnings,
    // (3) cover-art warnings, (4) loudness warnings (L3: short fixtures are
    // unmeasurable, so loudness warns after cover warnings). The fixture has
    // a valid cover, so no cover warnings appear — but tag warnings do (only
    // `title` is set).
    assert!(
        diagnostics.warnings[0].starts_with("24-bit FLAC input is converted"),
        "{:?}",
        diagnostics.warnings
    );
    assert!(
        !diagnostics
            .warnings
            .iter()
            .filter(|w| !w.starts_with("loudness:"))
            .any(|w| w.contains("cover")),
        "valid cover must warn about nothing: {:?}",
        diagnostics.warnings
    );
    assert!(
        diagnostics.warnings.iter().any(|w| w.contains("artist")),
        "{:?}",
        diagnostics.warnings
    );

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    let (data_type, payload) = mp4_ilst_cover(&bytes, list).expect("covr must be present");
    assert_eq!(data_type, 13);
    assert_eq!(&payload, expected_bytes, "original image bytes unchanged");
    assert_eq!(&bytes[8..12], b"M4A ");
    assert_eq!(mp4_mdhd(&bytes, root.require("mdhd")), (48_000, 3 * 1024));
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn cover_24bit_unsupported_reports_full_warning_order() {
    // F1: all warning categories in one conversion, asserted in exact order:
    // (1) precision notice, (2) text-tag warnings in slot order,
    // (3) cover-art warnings in selection order, (4) loudness warnings (L3:
    // short fixtures are unmeasurable, so loudness warns last).
    let directory = TempDir::new("cover-24bit-order");
    let output = directory.join("cover.m4a");
    let (summary, diagnostics) = convert_file_with_format_and_tags(
        &fixture("cover-unsupported-48k-24.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(summary.properties.bits_per_sample, 24);
    assert_eq!(summary.encoded_frames, 3);
    assert_eq!(
        diagnostics.warnings,
        [
            "24-bit FLAC input is converted to 16-bit PCM before AAC encoding; low-order source \
             precision is discarded",
            "missing artist",
            "missing album",
            "missing tracknumber",
            "missing discnumber",
            "missing albumartist",
            "missing date/year",
            "missing genre",
            "missing composer",
            "missing comment",
            "unsupported cover image format: skipped",
            "missing cover",
            "loudness: no measurable signal: omitting loudness tags for this track",
            "loudness: album is incomplete (unanalyzable: cover-unsupported-48k-24.flac): \
             omitting album-level loudness tags",
        ]
    );
    // Audio converts successfully with no covr atom.
    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    assert_eq!(mp4_ilst_cover(&bytes, list), None);
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
#[ignore = "requires ffprobe on PATH"]
fn ffprobe_sees_cover_as_attached_pic() {
    if Command::new("ffprobe").arg("-version").output().is_err() {
        eprintln!("ffprobe is not available; cover read-back check skipped");
        return;
    }
    let directory = TempDir::new("cover-ffprobe");
    let output = directory.join("cover.m4a");
    convert_file_with_format_and_tags(
        &fixture("tagged-cover-jpeg-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();

    // The image surfaces as an attached-picture video stream beside audio.
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,codec_type",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(probe.status.success());
    let text = String::from_utf8_lossy(&probe.stdout);
    assert!(text.contains("codec_name=aac"), "{text}");
    assert!(text.contains("codec_name=mjpeg"), "{text}");

    let decode = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    assert!(
        decode.status.success(),
        "{}",
        String::from_utf8_lossy(&decode.stderr)
    );
}
