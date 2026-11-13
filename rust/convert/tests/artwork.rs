//! Album artwork discovery, reuse, priority, and fallback integration tests.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use aac_convert::batch::{execute_plan_with_options, plan_batch, BatchOptions};
use aac_convert::{
    convert_file_with_format_tags_and_cover_art, CoverArtAction, CoverArtMode, OutputFormat,
};
use common::{fixture, mp4_ilst_cover, parse_mp4, TempDir};

const JPEG_A: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, b'A', b'R', b'T', b'I', b'S', b'T', 0x00, 0x01,
];
const JPEG_B: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, b'B', b'A', b'N', b'N', b'E', b'R', 0x00, 0x02,
];
const PNG_A: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, b'A', b'R', b'T', b'W', b'O', b'R', b'K',
];

fn seed_file(root: &Path, relative: &str, fixture_name: &str) -> PathBuf {
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::copy(fixture(fixture_name), &destination).unwrap();
    destination
}

fn write_artwork(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&destination, bytes).unwrap();
    destination
}

fn embedded_cover(bytes: &[u8]) -> Option<(u32, Vec<u8>)> {
    let root = parse_mp4(bytes);
    let list = root.require("udta").require("meta").require("ilst");
    mp4_ilst_cover(bytes, list)
}

fn execute(
    input: &Path,
    output: &Path,
    cover_art: CoverArtMode,
) -> aac_convert::batch::BatchExecution {
    let plan = plan_batch(input, output, OutputFormat::M4a, true).unwrap();
    execute_plan_with_options(
        &plan,
        OutputFormat::M4a,
        &BatchOptions {
            cover_art,
            jobs: 1,
            ..BatchOptions::default()
        },
    )
    .unwrap()
}

fn first_embedded_cover_named(fixture_name: &str) -> Vec<u8> {
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let path = fixture(fixture_name);
    let file = fs::File::open(&path).unwrap();
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
    let mut metadata = format.metadata();
    let revision = metadata.skip_to_latest().unwrap();
    revision.media.visuals[0].data.to_vec()
}

fn first_embedded_cover() -> Vec<u8> {
    first_embedded_cover_named("tagged-cover-jpeg-48k-16.flac")
}

#[test]
fn existing_album_artwork_is_reused_for_external_publication() {
    let workspace = TempDir::new("artwork-reuse-external");
    let input = workspace.join("in");
    seed_file(&input, "album/one.flac", "stereo-48k-16.flac");
    seed_file(&input, "album/two.flac", "mono-441k-16.flac");
    // Case-insensitive match, and a source-only name that maps to the normal
    // external output name `cover.jpg`.
    write_artwork(&input, "album/FOLDER.JPG", JPEG_A);
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::External);
    assert_eq!(execution.outcome.summary.converted, 2);
    assert_eq!(execution.cover_art.len(), 1, "{:?}", execution.cover_art);
    assert_eq!(execution.cover_art[0].directory, PathBuf::from("album"));
    assert_eq!(
        execution.cover_art[0].file_name.as_deref(),
        Some("cover.jpg")
    );
    assert_eq!(execution.cover_art[0].action, CoverArtAction::Written);
    assert_eq!(fs::read(output.join("album/cover.jpg")).unwrap(), JPEG_A);
    assert_eq!(
        fs::read(input.join("album/FOLDER.JPG")).unwrap(),
        JPEG_A,
        "source artwork must never be modified"
    );
    // External mode does not embed.
    for name in ["one.m4a", "two.m4a"] {
        assert_eq!(
            embedded_cover(&fs::read(output.join("album").join(name)).unwrap()),
            None
        );
    }
}

#[test]
fn existing_album_artwork_is_reused_for_embedding() {
    let workspace = TempDir::new("artwork-reuse-embedded");
    let input = workspace.join("in");
    // These fixtures carry no embedded artwork, so the only possible source is
    // the album directory file.
    seed_file(&input, "album/one.flac", "stereo-48k-16.flac");
    seed_file(&input, "album/two.flac", "mono-441k-16.flac");
    write_artwork(&input, "album/cover.png", PNG_A);
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::Embedded);
    assert_eq!(execution.outcome.summary.converted, 2);
    assert!(
        execution.cover_art.is_empty(),
        "embedded mode writes no external cover: {:?}",
        execution.cover_art
    );
    for name in ["one.m4a", "two.m4a"] {
        let bytes = fs::read(output.join("album").join(name)).unwrap();
        let (data_type, payload) = embedded_cover(&bytes).expect("album artwork must be embedded");
        assert_eq!(data_type, 14, "PNG cover data type");
        assert_eq!(payload, PNG_A);
    }
    assert!(!output.join("album/cover.png").exists());

    // `both` embeds the same selected album image and publishes it once.
    let both_output = workspace.join("both");
    let execution = execute(&input, &both_output, CoverArtMode::Both);
    assert_eq!(execution.outcome.summary.converted, 2);
    assert_eq!(execution.cover_art.len(), 1, "{:?}", execution.cover_art);
    assert_eq!(
        execution.cover_art[0].file_name.as_deref(),
        Some("cover.png")
    );
    assert_eq!(
        fs::read(both_output.join("album/cover.png")).unwrap(),
        PNG_A
    );
    for name in ["one.m4a", "two.m4a"] {
        let bytes = fs::read(both_output.join("album").join(name)).unwrap();
        let (data_type, payload) = embedded_cover(&bytes).expect("both must embed");
        assert_eq!(data_type, 14);
        assert_eq!(payload, PNG_A);
    }
}

#[test]
fn deterministic_priority_prefers_cover_jpg_over_other_candidates() {
    let workspace = TempDir::new("artwork-priority");
    let input = workspace.join("in");
    seed_file(&input, "album/one.flac", "stereo-48k-16.flac");
    write_artwork(&input, "album/album.jpeg", JPEG_B);
    write_artwork(&input, "album/folder.png", PNG_A);
    write_artwork(&input, "album/cover.jpg", JPEG_A);
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::External);
    assert_eq!(execution.cover_art.len(), 1, "{:?}", execution.cover_art);
    assert_eq!(
        execution.cover_art[0].file_name.as_deref(),
        Some("cover.jpg")
    );
    assert_eq!(fs::read(output.join("album/cover.jpg")).unwrap(), JPEG_A);
}

#[test]
fn multiple_albums_keep_their_own_artwork() {
    let workspace = TempDir::new("artwork-multiple-albums");
    let input = workspace.join("in");
    seed_file(&input, "a/one.flac", "stereo-48k-16.flac");
    seed_file(&input, "b/one.flac", "mono-441k-16.flac");
    write_artwork(&input, "a/cover.jpg", JPEG_A);
    write_artwork(&input, "b/front.png", PNG_A);
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::External);
    assert_eq!(execution.outcome.summary.converted, 2);
    assert_eq!(execution.cover_art.len(), 2, "{:?}", execution.cover_art);
    assert_eq!(
        execution.cover_art[0].file_name.as_deref(),
        Some("cover.jpg")
    );
    assert_eq!(
        execution.cover_art[1].file_name.as_deref(),
        Some("cover.png")
    );
    assert_eq!(fs::read(output.join("a/cover.jpg")).unwrap(), JPEG_A);
    assert_eq!(fs::read(output.join("b/cover.png")).unwrap(), PNG_A);
}

#[test]
fn missing_artwork_converts_without_cover_files() {
    let workspace = TempDir::new("artwork-missing");
    let input = workspace.join("in");
    seed_file(&input, "album/one.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::External);
    assert_eq!(execution.outcome.summary.converted, 1);
    assert_eq!(execution.outcome.summary.failed, 0);
    assert!(execution.cover_art.is_empty(), "{:?}", execution.cover_art);
    assert!(!output.join("album/cover.jpg").exists());
    assert!(!output.join("album/cover.png").exists());
    assert!(!output.join("album/cover.webp").exists());
}

#[cfg(unix)]
#[test]
fn unreadable_artwork_falls_back_to_embedded_artwork() {
    use std::os::unix::fs::PermissionsExt;

    let expected = first_embedded_cover();
    let workspace = TempDir::new("artwork-unreadable");
    let input = workspace.join("in");
    seed_file(&input, "album/one.flac", "tagged-cover-jpeg-48k-16.flac");
    let locked = write_artwork(&input, "album/cover.jpg", JPEG_A);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&locked).is_ok() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
        eprintln!("permissions are not enforced here; unreadable-artwork test not applicable");
        return;
    }
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::External);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(execution.outcome.summary.converted, 1);
    assert_eq!(execution.outcome.summary.failed, 0);
    assert_eq!(execution.cover_art.len(), 1, "{:?}", execution.cover_art);
    assert_eq!(
        execution.cover_art[0].action,
        CoverArtAction::Written,
        "unreadable external artwork must fall back to the embedded image"
    );
    assert_eq!(
        fs::read(output.join("album/cover.jpg")).unwrap(),
        expected,
        "fallback must publish the embedded JPEG, not the unreadable file"
    );
}

#[test]
fn webp_artwork_is_reused_for_external_publication() {
    let webp = {
        let mut data = b"RIFF".to_vec();
        data.extend_from_slice(&12u32.to_le_bytes());
        data.extend_from_slice(b"WEBPVP8 ");
        data.extend_from_slice(b"payload");
        data
    };
    let workspace = TempDir::new("artwork-webp");
    let input = workspace.join("in");
    seed_file(&input, "album/one.flac", "stereo-48k-16.flac");
    write_artwork(&input, "album/album.webp", &webp);
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::External);
    assert_eq!(execution.outcome.summary.converted, 1);
    assert_eq!(execution.cover_art.len(), 1, "{:?}", execution.cover_art);
    assert_eq!(
        execution.cover_art[0].file_name.as_deref(),
        Some("cover.webp")
    );
    assert_eq!(fs::read(output.join("album/cover.webp")).unwrap(), webp);
}

#[test]
fn existing_external_artwork_wins_over_embedded_conflicts() {
    let workspace = TempDir::new("artwork-external-wins");
    let input = workspace.join("in");
    // Different embedded images would normally conflict and veto external
    // publication. The directory image is authoritative and avoids the
    // embedded scan entirely.
    seed_file(&input, "album/jpeg.flac", "tagged-cover-jpeg-48k-16.flac");
    seed_file(&input, "album/png.flac", "tagged-cover-png-48k-16.flac");
    write_artwork(&input, "album/front.jpg", JPEG_A);
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::External);
    assert_eq!(execution.outcome.summary.converted, 2);
    assert_eq!(execution.cover_art.len(), 1, "{:?}", execution.cover_art);
    assert_eq!(execution.cover_art[0].action, CoverArtAction::Written);
    assert_eq!(fs::read(output.join("album/cover.jpg")).unwrap(), JPEG_A);
}

#[test]
fn conflicting_embedded_artwork_still_vetoes_publication_without_external_file() {
    let workspace = TempDir::new("artwork-conflict");
    let input = workspace.join("in");
    seed_file(&input, "album/jpeg.flac", "tagged-cover-jpeg-48k-16.flac");
    seed_file(&input, "album/png.flac", "tagged-cover-png-48k-16.flac");
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::External);
    assert_eq!(execution.outcome.summary.converted, 2);
    assert_eq!(execution.cover_art.len(), 1, "{:?}", execution.cover_art);
    assert_eq!(execution.cover_art[0].action, CoverArtAction::Conflicted);
    assert!(!output.join("album/cover.jpg").exists());
    assert!(!output.join("album/cover.png").exists());
}

#[test]
fn batch_external_fallback_preserves_per_member_cover_diagnostics() {
    use aac_convert::batch::BatchFileOutcome;

    let workspace = TempDir::new("artwork-per-member-diagnostics");
    let input = workspace.join("in");
    // One member carries an unsupported picture (warns); the other is plain
    // (silent). Neither has a usable cover, so nothing is published.
    seed_file(
        &input,
        "album/bad.flac",
        "tagged-cover-unsupported-48k-16.flac",
    );
    seed_file(&input, "album/plain.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::External);
    assert_eq!(execution.outcome.summary.converted, 2);
    assert!(execution.cover_art.is_empty(), "{:?}", execution.cover_art);

    let mut bad_warnings = None;
    let mut plain_warnings = None;
    for result in &execution.outcome.results {
        match &result.outcome {
            BatchFileOutcome::Converted { tags, .. } => {
                if result.relative_input.ends_with("bad.flac") {
                    bad_warnings = Some(tags.warnings.clone());
                } else if result.relative_input.ends_with("plain.flac") {
                    plain_warnings = Some(tags.warnings.clone());
                }
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
    let bad_warnings = bad_warnings.expect("bad track must convert");
    let plain_warnings = plain_warnings.expect("plain track must convert");
    assert!(
        bad_warnings
            .iter()
            .any(|warning| warning.contains("unsupported cover image format")),
        "unsupported member must keep its own warning: {bad_warnings:?}"
    );
    assert!(
        !plain_warnings
            .iter()
            .any(|warning| warning.to_ascii_lowercase().contains("cover")),
        "plain member must not inherit another track's cover warning: {plain_warnings:?}"
    );
}

#[test]
fn batch_artwork_spill_files_are_cleaned_up_on_success() {
    let workspace = TempDir::new("artwork-spill-cleanup");
    let input = workspace.join("in");
    seed_file(&input, "a/one.flac", "tagged-cover-jpeg-48k-16.flac");
    seed_file(&input, "b/one.flac", "tagged-cover-jpeg-48k-16.flac");
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::External);
    assert_eq!(execution.outcome.summary.converted, 2);
    // No staging directory may remain inside the output root.
    let leftovers: Vec<_> = fs::read_dir(&output)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".aac-convert-artwork-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "artwork staging must be cleaned up: {leftovers:?}"
    );
}

#[test]
fn both_mode_keeps_per_track_embedded_artwork_without_external_file() {
    let jpeg = first_embedded_cover_named("tagged-cover-jpeg-48k-16.flac");
    let png = first_embedded_cover_named("tagged-cover-png-48k-16.flac");
    let workspace = TempDir::new("artwork-both-per-track");
    let input = workspace.join("in");
    seed_file(&input, "album/jpeg.flac", "tagged-cover-jpeg-48k-16.flac");
    seed_file(&input, "album/png.flac", "tagged-cover-png-48k-16.flac");
    let output = workspace.join("out");

    let execution = execute(&input, &output, CoverArtMode::Both);
    assert_eq!(execution.outcome.summary.converted, 2);
    // Conflicting embedded images still veto external publication.
    assert_eq!(execution.cover_art.len(), 1, "{:?}", execution.cover_art);
    assert_eq!(execution.cover_art[0].action, CoverArtAction::Conflicted);
    assert!(!output.join("album/cover.jpg").exists());
    assert!(!output.join("album/cover.png").exists());

    // Each track keeps its own embedded picture when no external album image
    // exists; `both` must not collapse them to the first image.
    let jpeg_bytes = fs::read(output.join("album/jpeg.m4a")).unwrap();
    let (data_type, payload) = embedded_cover(&jpeg_bytes).expect("jpeg must embed");
    assert_eq!(data_type, 13);
    assert_eq!(payload, jpeg);
    let png_bytes = fs::read(output.join("album/png.m4a")).unwrap();
    let (data_type, payload) = embedded_cover(&png_bytes).expect("png must embed");
    assert_eq!(data_type, 14);
    assert_eq!(payload, png);
}

#[test]
fn single_file_conversion_reuses_sibling_artwork() {
    let workspace = TempDir::new("artwork-single-file");
    let input = workspace.join("in/track.flac");
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::copy(fixture("stereo-48k-16.flac"), &input).unwrap();
    write_artwork(workspace.path(), "in/folder.jpg", JPEG_A);
    let output = workspace.join("out/track.m4a");
    fs::create_dir_all(output.parent().unwrap()).unwrap();

    let (_, _, reports) = convert_file_with_format_tags_and_cover_art(
        &input,
        &output,
        OutputFormat::M4a,
        true,
        CoverArtMode::Both,
    )
    .unwrap();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0].file_name.as_deref(), Some("cover.jpg"));
    assert_eq!(reports[0].action, CoverArtAction::Written);
    assert_eq!(fs::read(workspace.join("out/cover.jpg")).unwrap(), JPEG_A);
    let bytes = fs::read(&output).unwrap();
    let (data_type, payload) = embedded_cover(&bytes).expect("both must embed");
    assert_eq!(data_type, 13);
    assert_eq!(payload, JPEG_A);
}

#[test]
fn single_file_external_mode_reuses_webp_album_artwork() {
    let webp = {
        let mut data = b"RIFF".to_vec();
        data.extend_from_slice(&12u32.to_le_bytes());
        data.extend_from_slice(b"WEBPVP8 ");
        data.extend_from_slice(b"single");
        data
    };
    let workspace = TempDir::new("artwork-single-webp");
    let input = workspace.join("in/track.flac");
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::copy(fixture("stereo-48k-16.flac"), &input).unwrap();
    write_artwork(workspace.path(), "in/cover.webp", &webp);
    let output = workspace.join("out/track.m4a");
    fs::create_dir_all(output.parent().unwrap()).unwrap();

    let (_, _, reports) = convert_file_with_format_tags_and_cover_art(
        &input,
        &output,
        OutputFormat::M4a,
        true,
        CoverArtMode::External,
    )
    .unwrap();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0].file_name.as_deref(), Some("cover.webp"));
    assert_eq!(fs::read(workspace.join("out/cover.webp")).unwrap(), webp);
    assert_eq!(embedded_cover(&fs::read(&output).unwrap()), None);
}

#[test]
fn single_file_external_webp_suppresses_ignored_embedded_artwork() {
    // Reviewer scenario (B2): the source carries an unsupported embedded
    // picture, but a valid sibling `cover.webp` is authoritative for
    // `--cover-art external`. No embedded read or warning may leak through.
    let webp = {
        let mut data = b"RIFF".to_vec();
        data.extend_from_slice(&12u32.to_le_bytes());
        data.extend_from_slice(b"WEBPVP8 ");
        data.extend_from_slice(b"review");
        data
    };
    let workspace = TempDir::new("artwork-single-webp-precedence");
    let input = workspace.join("in/track.flac");
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::copy(fixture("tagged-cover-unsupported-48k-16.flac"), &input).unwrap();
    write_artwork(workspace.path(), "in/cover.webp", &webp);
    let output = workspace.join("out/track.m4a");
    fs::create_dir_all(output.parent().unwrap()).unwrap();

    let (_, diagnostics, reports) = convert_file_with_format_tags_and_cover_art(
        &input,
        &output,
        OutputFormat::M4a,
        true,
        CoverArtMode::External,
    )
    .unwrap();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0].file_name.as_deref(), Some("cover.webp"));
    assert_eq!(reports[0].action, CoverArtAction::Written);
    assert_eq!(fs::read(workspace.join("out/cover.webp")).unwrap(), webp);
    assert_eq!(embedded_cover(&fs::read(&output).unwrap()), None);
    assert!(
        !diagnostics
            .warnings
            .iter()
            .any(|warning| warning.to_ascii_lowercase().contains("cover")),
        "ignored embedded artwork must not warn when external WebP wins: {:?}",
        diagnostics.warnings
    );
}
