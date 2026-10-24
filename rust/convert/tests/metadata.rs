//! M1 core-9 M4A text metadata tests (G0 decision record).
//!
//! Covers the agreed mapping and edge rules end to end: every approved atom,
//! missing/empty/duplicate/multi-value handling, track/disc tuples and
//! fallbacks, date/year precedence, UTF-8 behavior, unknown/deferred keys,
//! `--no-tags` semantics on M4A and ADTS, and regression that ADTS output,
//! gapless signalling, batch behavior, and no-overwrite publication are
//! unchanged.

mod common;

use std::fs;
use std::process::Command;

use aac_convert::batch::{execute_plan_with_tags, plan_batch, BatchFileOutcome};
use aac_convert::{
    convert_file, convert_file_with_format, convert_file_with_format_and_tags, OutputFormat,
};
use common::{
    fixture, mp4_ilst_children, mp4_ilst_pair, mp4_ilst_text, mp4_itunsmpb, mp4_mdhd, parse_adts,
    parse_mp4, temp_output_files, TempDir,
};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_aac-convert")
}

fn run_cli(args: &[&str]) -> std::process::Output {
    Command::new(binary()).args(args).output().unwrap()
}

fn stdout_lines(output: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
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

/// Children of `moov/udta/meta/ilst` as raw 4-byte types, in file order.
fn ilst_types(bytes: &[u8]) -> Vec<[u8; 4]> {
    let root = parse_mp4(bytes);
    let ilst = root.require("udta").require("meta").require("ilst");
    mp4_ilst_children(bytes, ilst)
        .into_iter()
        .map(|(kind, _)| kind)
        .collect()
}

#[test]
fn core_tags_round_trip_stereo() {
    let directory = TempDir::new("meta-core");
    let output = directory.join("tagged.m4a");
    let (summary, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-core-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(summary.encoded_frames, 14);
    assert_eq!(diagnostics.mapped, 10);
    assert!(
        diagnostics.unmapped.is_empty(),
        "{:?}",
        diagnostics.unmapped
    );
    // Fully tagged but imageless: the S1.3 cover slot reports `missing cover`.
    assert_eq!(diagnostics.warnings, ["missing cover"]);

    let bytes = fs::read(&output).unwrap();
    assert_eq!(summary.output_bytes, bytes.len() as u64);
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");

    assert_eq!(
        ilst_types(&bytes),
        vec![
            *b"----",
            *b"\xa9nam",
            *b"\xa9ART",
            *b"\xa9alb",
            *b"trkn",
            *b"disk",
            *b"aART",
            *b"\xa9day",
            *b"\xa9gen",
            *b"\xa9wrt",
            *b"\xa9cmt",
        ]
    );
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9nam"), vec!["Core Title"]);
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9ART"), vec!["Core Artist"]);
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9alb"), vec!["Core Album"]);
    assert_eq!(mp4_ilst_pair(&bytes, list, b"trkn"), Some((5, 12)));
    assert_eq!(mp4_ilst_pair(&bytes, list, b"disk"), Some((1, 2)));
    assert_eq!(
        mp4_ilst_text(&bytes, list, b"aART"),
        vec!["Core Album Artist"]
    );
    // `date` wins over `year`; the atom is written once.
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9day"), vec!["2024-03-07"]);
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9gen"), vec!["Test Genre"]);
    assert_eq!(
        mp4_ilst_text(&bytes, list, b"\xa9wrt"),
        vec!["Core Composer"]
    );
    assert_eq!(
        mp4_ilst_text(&bytes, list, b"\xa9cmt"),
        vec!["Core Comment"]
    );

    // Container contract unchanged: brands, no edit list, encoded timeline.
    assert_eq!(&bytes[8..12], b"M4A ");
    assert!(root.require("trak").find("edts").is_none());
    assert_eq!(mp4_mdhd(&bytes, root.require("mdhd")), (48_000, 14 * 1024));

    // Gapless signalling identical to the untagged conversion of same audio.
    let plain_dir = TempDir::new("meta-core-plain");
    let plain_out = plain_dir.join("plain.m4a");
    convert_file_with_format(
        &fixture("stereo-48k-16.flac"),
        &plain_out,
        OutputFormat::M4a,
    )
    .unwrap();
    let plain = fs::read(&plain_out).unwrap();
    assert_eq!(
        mp4_itunsmpb(&bytes, &root),
        mp4_itunsmpb(&plain, &parse_mp4(&plain))
    );

    // AAC payloads identical to the ADTS access units for the same audio.
    let adts_dir = TempDir::new("meta-core-adts");
    let adts_out = adts_dir.join("tagged.aac");
    convert_file_with_format(
        &fixture("tagged-core-48k-16.flac"),
        &adts_out,
        OutputFormat::Adts,
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
fn core_tags_round_trip_mono_441() {
    let directory = TempDir::new("meta-mono");
    let output = directory.join("mono.m4a");
    let (summary, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-core-441k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(summary.encoded_frames, 5);
    assert_eq!(diagnostics.mapped, 5);

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9nam"), vec!["Mono Title"]);
    assert_eq!(mp4_ilst_pair(&bytes, list, b"trkn"), Some((3, 9)));
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9day"), vec!["2023"]);
    assert_eq!(mp4_mdhd(&bytes, root.require("mdhd")), (44_100, 5 * 1024));
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn edge_year_fallback_tracktotal_and_unmapped() {
    let directory = TempDir::new("meta-edge");
    let output = directory.join("edge.m4a");
    let (_, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-edge-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();

    // title trimmed, first artist wins, year used, totals from fallbacks.
    assert_eq!(diagnostics.mapped, 7);
    assert_eq!(
        diagnostics.unmapped,
        vec![
            "bpm",
            "compilation",
            "grouping",
            "label",
            "musicbrainz_trackid"
        ]
    );
    assert!(
        diagnostics
            .warnings
            .iter()
            .any(|w| w.contains("albumartist")),
        "{:?}",
        diagnostics.warnings
    );
    assert!(
        diagnostics.warnings.iter().any(|w| w.contains("composer")),
        "{:?}",
        diagnostics.warnings
    );
    assert!(
        diagnostics.warnings.iter().any(|w| w.contains("comment")),
        "{:?}",
        diagnostics.warnings
    );

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    assert_eq!(
        mp4_ilst_text(&bytes, list, b"\xa9nam"),
        vec!["Padded Title"]
    );
    assert_eq!(
        mp4_ilst_text(&bytes, list, b"\xa9ART"),
        vec!["First Artist"]
    );
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9day"), vec!["2001"]);
    assert_eq!(mp4_ilst_pair(&bytes, list, b"trkn"), Some((7, 9)));
    assert_eq!(mp4_ilst_pair(&bytes, list, b"disk"), Some((2, 3)));
    // Deferred keys produce no atoms.
    for atom in [b"tmpo", b"cpil", b"\xa9grp"] {
        assert!(
            mp4_ilst_text(&bytes, list, atom).is_empty(),
            "{atom:?} must not be written"
        );
    }
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn invalid_numbers_skip_atom_with_warning() {
    let directory = TempDir::new("meta-invalid");
    let output = directory.join("invalid.m4a");
    let (summary, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-invalid-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(summary.encoded_frames, 14);
    assert_eq!(diagnostics.mapped, 3);

    assert!(
        diagnostics
            .warnings
            .iter()
            .any(|w| w.contains("tracknumber")),
        "{:?}",
        diagnostics.warnings
    );
    assert!(
        diagnostics
            .warnings
            .iter()
            .any(|w| w.contains("discnumber")),
        "{:?}",
        diagnostics.warnings
    );
    assert!(
        diagnostics.warnings.iter().any(|w| w.contains("comment")),
        "{:?}",
        diagnostics.warnings
    );

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    assert_eq!(mp4_ilst_pair(&bytes, list, b"trkn"), None);
    assert_eq!(mp4_ilst_pair(&bytes, list, b"disk"), None);
    assert_eq!(
        mp4_ilst_text(&bytes, list, b"\xa9nam"),
        vec!["Invalid Title"]
    );
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn unicode_tags_round_trip_trimmed() {
    let directory = TempDir::new("meta-unicode");
    let output = directory.join("unicode.m4a");
    let (_, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-unicode-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(diagnostics.mapped, 2);
    assert!(
        diagnostics.unmapped.is_empty(),
        "{:?}",
        diagnostics.unmapped
    );
    // Only the two written slots are silent; the rest still warn per G0.
    assert!(
        !diagnostics
            .warnings
            .iter()
            .any(|w| w.contains("title") || w.contains("comment")),
        "{:?}",
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
    assert_eq!(mp4_ilst_text(&bytes, list, b"\xa9nam"), vec!["café 🎵"]);
    assert_eq!(
        mp4_ilst_text(&bytes, list, b"\xa9cmt"),
        vec!["crème brûlée"]
    );
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn untagged_input_writes_gapless_only() {
    let directory = TempDir::new("meta-untagged");
    let output = directory.join("plain.m4a");
    let (_, diagnostics) = convert_file_with_format_and_tags(
        &fixture("stereo-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap();
    assert_eq!(diagnostics.mapped, 0);
    // Committed fixtures carry only an `encoder` comment (written by ffmpeg
    // at generation time); it is deferred, hence unmapped, never an error.
    assert_eq!(diagnostics.unmapped, vec!["encoder"]);
    assert!(
        diagnostics.warnings.iter().any(|w| w.contains("title")),
        "{:?}",
        diagnostics.warnings
    );

    let bytes = fs::read(&output).unwrap();
    assert_eq!(ilst_types(&bytes), vec![*b"----"]);
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn no_tags_flag_omits_atoms_but_counts_unmapped() {
    let directory = TempDir::new("meta-notags");
    let output = directory.join("notags.m4a");
    let (_, diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-core-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        false,
    )
    .unwrap();
    assert_eq!(diagnostics.mapped, 0);
    // Tagged-core has no unmapped keys; diagnostics still report zero mapped
    // and no warnings under the flag.
    assert!(
        diagnostics.unmapped.is_empty(),
        "{:?}",
        diagnostics.unmapped
    );
    assert!(
        diagnostics.warnings.is_empty(),
        "{:?}",
        diagnostics.warnings
    );

    let bytes = fs::read(&output).unwrap();
    assert_eq!(ilst_types(&bytes), vec![*b"----"]);

    // Unmapped counting continues under the flag (edge fixture has 5).
    let edge_out = directory.join("edge-notags.m4a");
    let (_, edge) = convert_file_with_format_and_tags(
        &fixture("tagged-edge-48k-16.flac"),
        &edge_out,
        OutputFormat::M4a,
        false,
    )
    .unwrap();
    assert_eq!(edge.mapped, 0);
    assert_eq!(edge.unmapped.len(), 5);
    assert!(edge.warnings.is_empty());
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn adts_never_carries_tags() {
    let directory = TempDir::new("meta-adts");
    let tagged = directory.join("tagged.aac");
    let plain = directory.join("plain.aac");
    let (tagged_summary, tagged_diagnostics) = convert_file_with_format_and_tags(
        &fixture("tagged-core-48k-16.flac"),
        &tagged,
        OutputFormat::Adts,
        true,
    )
    .unwrap();
    convert_file(&fixture("stereo-48k-16.flac"), &plain).unwrap();

    // Byte-identical to the untagged ADTS conversion; no tags read or written.
    assert_eq!(fs::read(&tagged).unwrap(), fs::read(&plain).unwrap());
    assert_eq!(tagged_diagnostics.mapped, 0);
    assert!(tagged_diagnostics.unmapped.is_empty());
    assert!(tagged_diagnostics.warnings.is_empty());
    assert_eq!(tagged_summary.encoded_frames, 14);
    assert!(temp_output_files(directory.path()).is_empty());
}

#[test]
fn tag_error_has_stable_category_and_message() {
    // The Metadata variant is only reachable when tag reading itself fails
    // (decode succeeds first); assert the taxonomy wiring directly. An
    // undecodable input still fails earlier with `InputNotFlac`, never with
    // a metadata error.
    let err = aac_convert::ConvertError::Metadata("could not read tags");
    assert_eq!(aac_convert::batch::category_for(&err), "Metadata");
    assert!(err.to_string().contains("could not read tags"), "{err}");

    let directory = TempDir::new("meta-notflac");
    let input = directory.join("input.flac");
    fs::write(&input, b"not a flac stream").unwrap();
    let output = directory.join("out.m4a");
    let err =
        convert_file_with_format_and_tags(&input, &output, OutputFormat::M4a, true).unwrap_err();
    assert!(
        matches!(err, aac_convert::ConvertError::InputNotFlac(_)),
        "{err:?}"
    );
    assert!(!output.exists());
}

// ---------------------------------------------------------------------------
// CLI behavior
// ---------------------------------------------------------------------------

#[test]
fn cli_single_file_tags_by_default_and_warns() {
    let workspace = TempDir::new("cli-meta-single");
    let output = workspace.join("out.m4a");
    let result = run_cli(&[
        fixture("tagged-invalid-48k-16.flac").to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("converted"), "{stdout}");
    assert!(stdout.contains("M4A"), "{stdout}");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("warning: "), "{stderr}");
    assert!(stderr.contains("tracknumber"), "{stderr}");

    let bytes = fs::read(&output).unwrap();
    let root = parse_mp4(&bytes);
    let list = root.require("udta").require("meta").require("ilst");
    assert_eq!(
        mp4_ilst_text(&bytes, list, b"\xa9nam"),
        vec!["Invalid Title"]
    );
    assert_eq!(mp4_ilst_pair(&bytes, list, b"trkn"), None);
}

#[test]
fn cli_no_tags_m4a_omits_atoms_and_warnings() {
    let workspace = TempDir::new("cli-meta-notags");
    let output = workspace.join("out.m4a");
    let result = run_cli(&[
        "--no-tags",
        fixture("tagged-core-48k-16.flac").to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).is_empty(),
        "no warnings under --no-tags: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    assert_eq!(ilst_types(&bytes), vec![*b"----"]);
}

#[test]
fn cli_no_tags_adts_is_a_noop() {
    let workspace = TempDir::new("cli-meta-notags-adts");
    let output = workspace.join("out.aac");
    let result = run_cli(&[
        "--no-tags",
        "--format",
        "adts",
        fixture("tagged-core-48k-16.flac").to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    // Same bytes as the untagged ADTS conversion: tags never touch ADTS.
    let reference = workspace.join("ref.aac");
    convert_file(&fixture("stereo-48k-16.flac"), &reference).unwrap();
    assert_eq!(fs::read(&output).unwrap(), fs::read(&reference).unwrap());
}

#[test]
fn cli_no_tags_invalid_spelling_is_a_usage_error() {
    let workspace = TempDir::new("cli-meta-badflag");
    let output = workspace.join("out.m4a");
    let result = run_cli(&[
        "--no-tag",
        fixture("tagged-core-48k-16.flac").to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("unknown option"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!output.exists());
}

#[test]
fn cli_dry_run_no_tags_plans_identically() {
    for extra in [false, true] {
        let workspace = TempDir::new("cli-meta-dryrun");
        let input = workspace.join("in");
        seed_file(&input, "a.flac", "tagged-core-48k-16.flac");
        let output = workspace.join("out");
        let mut args: Vec<&str> = vec!["--dry-run"];
        if extra {
            args.push("--no-tags");
        }
        let input_str = input.to_str().unwrap().to_string();
        let output_str = output.to_str().unwrap().to_string();
        args.push(&input_str);
        args.push(&output_str);
        let result = run_cli(&args);
        assert_eq!(result.status.code(), Some(0));
        let lines = stdout_lines(&result);
        assert!(
            lines.iter().any(|l| l == "plan a.flac -> a.m4a"),
            "{lines:?}"
        );
        assert!(!output.exists(), "dry run must create nothing");
    }
}

#[test]
fn cli_verbose_reports_tags_and_unmapped() {
    let workspace = TempDir::new("cli-meta-verbose");
    let input = workspace.join("in");
    seed_file(&input, "core.flac", "tagged-core-48k-16.flac");
    seed_file(&input, "edge.flac", "tagged-edge-48k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[
        "--verbose",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    let lines = stdout_lines(&result);
    assert!(
        lines
            .iter()
            .any(|l| l == "tags core.flac: 10 mapped, 0 unmapped"),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "tags edge.flac: 7 mapped, 5 unmapped"),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "unmapped edge.flac: bpm"),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "unmapped edge.flac: compilation"),
        "{lines:?}"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("warning: "), "{stderr}");
}

#[test]
fn cli_batch_summary_shape_unchanged_with_tags() {
    let workspace = TempDir::new("cli-meta-summary");
    let input = workspace.join("in");
    seed_file(&input, "core.flac", "tagged-core-48k-16.flac");
    seed_file(&input, "plain.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(0));
    let lines = stdout_lines(&result);
    // No new summary counter: the M8.2 four-count shape stands.
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("summary: 2 converted, 0 failed, 0 existing, 0 ignored ("),
        "{lines:?}"
    );
    assert!(output.join("core.m4a").is_file());
    assert!(output.join("plain.m4a").is_file());
}

#[test]
fn cli_batch_no_tags_real_run_omits_user_atoms() {
    let workspace = TempDir::new("cli-meta-batch-notags");
    let input = workspace.join("in");
    seed_file(&input, "core.flac", "tagged-core-48k-16.flac");
    seed_file(&input, "plain.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[
        "--no-tags",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    // No per-file warnings under --no-tags.
    assert!(
        String::from_utf8_lossy(&result.stderr).is_empty(),
        "no warnings under --no-tags: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    // Both outputs exist and carry gapless signalling only; brands stay.
    for name in ["core.m4a", "plain.m4a"] {
        let bytes = fs::read(output.join(name)).unwrap();
        assert_eq!(&bytes[8..12], b"M4A ", "{name}");
        assert_eq!(ilst_types(&bytes), vec![*b"----"], "{name}");
        let root = parse_mp4(&bytes);
        assert!(
            !mp4_itunsmpb(&bytes, &root).is_empty(),
            "iTunSMPB must survive --no-tags in {name}"
        );
    }

    // Summary shape unchanged: the M8.2 four-count line stands.
    let lines = stdout_lines(&result);
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("summary: 2 converted, 0 failed, 0 existing, 0 ignored ("),
        "{lines:?}"
    );
    assert!(temp_output_files(&output).is_empty());
}

// ---------------------------------------------------------------------------
// Batch library behavior
// ---------------------------------------------------------------------------

#[test]
fn batch_tags_flow_through_outcome() {
    let workspace = TempDir::new("batch-meta");
    let input = workspace.join("in");
    seed_file(&input, "core.flac", "tagged-core-48k-16.flac");
    seed_file(&input, "plain.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan_with_tags(&plan, OutputFormat::M4a, false, false, true).unwrap();
    assert_eq!(outcome.summary.converted, 2);
    assert_eq!(outcome.summary.failed, 0);

    for result in &outcome.results {
        match &result.outcome {
            BatchFileOutcome::Converted { tags, .. } => {
                if result.relative_input.ends_with("core.flac") {
                    assert_eq!(tags.mapped, 10, "{:?}", result.relative_input);
                    assert!(tags.unmapped.is_empty());
                    // Fully tagged but imageless (S1.3 cover slot).
                    assert_eq!(tags.warnings, ["missing cover"]);
                } else {
                    assert_eq!(tags.mapped, 0, "{:?}", result.relative_input);
                    assert!(tags.warnings.iter().any(|w| w.contains("title")));
                }
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
    assert!(temp_output_files(&output).is_empty());
}

#[test]
fn batch_no_tags_skips_writing_but_reports_unmapped() {
    let workspace = TempDir::new("batch-meta-notags");
    let input = workspace.join("in");
    seed_file(&input, "edge.flac", "tagged-edge-48k-16.flac");
    let output = workspace.join("out");

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan_with_tags(&plan, OutputFormat::M4a, false, false, false).unwrap();
    assert_eq!(outcome.summary.converted, 1);
    match &outcome.results[0].outcome {
        BatchFileOutcome::Converted { tags, .. } => {
            assert_eq!(tags.mapped, 0);
            assert_eq!(tags.unmapped.len(), 5);
            assert!(tags.warnings.is_empty());
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
    let bytes = fs::read(output.join("edge.m4a")).unwrap();
    assert_eq!(ilst_types(&bytes), vec![*b"----"]);
    assert!(temp_output_files(&output).is_empty());
}

#[test]
#[ignore = "requires ffprobe on PATH"]
fn ffprobe_reads_core_tags() {
    if Command::new("ffprobe").arg("-version").output().is_err() {
        eprintln!("ffprobe is not available; tag read-back check skipped");
        return;
    }
    let directory = TempDir::new("meta-ffprobe");
    let output = directory.join("tagged.m4a");
    convert_file_with_format(
        &fixture("tagged-core-48k-16.flac"),
        &output,
        OutputFormat::M4a,
    )
    .unwrap();

    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format_tags"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(probe.status.success());
    assert!(
        String::from_utf8_lossy(&probe.stderr).is_empty(),
        "{}",
        String::from_utf8_lossy(&probe.stderr)
    );
    let text = String::from_utf8_lossy(&probe.stdout);
    assert!(text.contains("Core Title"), "{text}");
    assert!(text.contains("Core Album"), "{text}");

    let decode = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "default=nw=1",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(decode.status.success());
    assert!(
        String::from_utf8_lossy(&decode.stdout).contains("codec_name=aac"),
        "{}",
        String::from_utf8_lossy(&decode.stdout)
    );
}
