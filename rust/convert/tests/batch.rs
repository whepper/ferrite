//! Sequential directory batch conversion tests (M8.2).
//!
//! Covers the M8.1 §9 acceptance matrix with deterministic tests: traversal,
//! mapping, preflight, dry-run, exit codes, collisions, cleanup, structure,
//! and platform-gated symlink/permission behavior. Existing single-file
//! suites are left untouched and must keep passing.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aac_convert::batch::{execute_plan, plan_batch, BatchError, BatchFileOutcome};
use aac_convert::OutputFormat;
use common::{fixture, mp4_itunsmpb, mp4_mdhd, parse_adts, parse_mp4, TempDir};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_aac-convert")
}

/// Copy a committed fixture into a tree, creating parent directories.
fn seed_file(root: &Path, relative: &str, fixture_name: &str) -> PathBuf {
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::copy(fixture(fixture_name), &destination).unwrap();
    destination
}

fn write_text(root: &Path, relative: &str, contents: &[u8]) -> PathBuf {
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&destination, contents).unwrap();
    destination
}

/// No `.aac-convert-*.tmp` file anywhere under `directory` (recursive).
fn assert_no_temp_files_anywhere(directory: &Path) {
    let mut stack = vec![directory.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            assert!(
                !name.starts_with(".aac-convert-"),
                "temporary file left behind: {}",
                path.display()
            );
            if path.is_dir() && !path.is_symlink() {
                stack.push(path);
            }
        }
    }
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

// ---------------------------------------------------------------------------
// Library: traversal, mapping, ordering (T-flat-1, T-nested-1, T-nonrec-1)
// ---------------------------------------------------------------------------

#[test]
fn flat_directory_converts_flac_and_ignores_the_rest() {
    let workspace = TempDir::new("batch-flat");
    let input = workspace.join("in");
    fs::create_dir(&input).unwrap();
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    seed_file(&input, "b.flac", "mono-441k-16.flac");
    write_text(&input, "note.txt", b"not audio");
    write_text(&input, "extensionless", b"no extension");

    let output = workspace.join("out");
    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(plan.entries.len(), 2);
    assert_eq!(plan.ignored.len(), 2);

    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(
        outcome.summary,
        aac_convert::batch::BatchSummary {
            converted: 2,
            failed: 0,
            existing: 0,
            ignored: 2,
        }
    );
    assert!(output.join("a.m4a").is_file());
    assert!(output.join("b.m4a").is_file());
    assert_no_temp_files_anywhere(&workspace.path().join("out"));
    assert_no_temp_files_anywhere(&input);
}

#[test]
fn nested_recursive_run_mirrors_structure_in_sorted_order() {
    let workspace = TempDir::new("batch-nested");
    let input = workspace.join("in");
    seed_file(&input, "top.flac", "stereo-48k-16.flac");
    seed_file(&input, "a/sib.FLAC", "mono-441k-16.flac");
    seed_file(&input, "a/b/c.flac", "stereo-48k-16.flac");

    let output = workspace.join("out");
    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let relative: Vec<PathBuf> = plan
        .entries
        .iter()
        .map(|entry| entry.relative_input.clone())
        .collect();
    assert_eq!(
        relative,
        vec![
            PathBuf::from("a/b/c.flac"),
            PathBuf::from("a/sib.FLAC"),
            PathBuf::from("top.flac"),
        ],
        "plan must be byte-sorted"
    );
    assert_eq!(
        plan.entries[0].relative_output,
        PathBuf::from("a/b/c.m4a"),
        "structure and stem preserved, extension replaced"
    );

    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(outcome.summary.converted, 3);
    assert_eq!(outcome.summary.failed, 0);
    // Reporting order equals processing order equals plan order.
    let reported: Vec<PathBuf> = outcome
        .results
        .iter()
        .map(|result| result.relative_input.clone())
        .collect();
    assert_eq!(reported, relative);
    assert!(output.join("a/b/c.m4a").is_file());
    assert!(output.join("a/sib.m4a").is_file());
    assert!(output.join("top.m4a").is_file());
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn no_recursive_converts_only_top_level_but_tallies_nested() {
    let workspace = TempDir::new("batch-nonrec");
    let input = workspace.join("in");
    seed_file(&input, "top.flac", "stereo-48k-16.flac");
    seed_file(&input, "sub/nested.flac", "mono-441k-16.flac");

    let output = workspace.join("out");
    let plan = plan_batch(&input, &output, OutputFormat::M4a, false).unwrap();
    assert_eq!(plan.entries.len(), 1);
    assert_eq!(plan.entries[0].relative_input, PathBuf::from("top.flac"));

    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(outcome.summary.converted, 1);
    assert_eq!(
        outcome.summary.ignored, 1,
        "nested FLAC is ignored, not failed"
    );
    assert!(output.join("top.m4a").is_file());
    assert!(
        !output.join("sub/nested.m4a").exists(),
        "no nested output may be created"
    );
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn extension_case_variants_map_to_lowercase_batch_extensions() {
    let workspace = TempDir::new("batch-ext");
    let input = workspace.join("in");
    seed_file(&input, "UPPER.FLAC", "stereo-48k-16.flac");
    seed_file(&input, "mixed.FlAc", "mono-441k-16.flac");
    seed_file(&input, "double.tar.flac", "stereo-48k-16.flac");
    seed_file(&input, ".hidden.flac", "mono-441k-16.flac");
    write_text(&input, "track.flac.bak", b"decoy");
    write_text(&input, "track.m4a", b"decoy");
    write_text(&input, "trackflac", b"decoy");

    let output = workspace.join("out");
    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let mut mapped: Vec<PathBuf> = plan
        .entries
        .iter()
        .map(|entry| entry.relative_output.clone())
        .collect();
    mapped.sort();
    assert_eq!(
        mapped,
        vec![
            PathBuf::from(".hidden.m4a"),
            PathBuf::from("UPPER.m4a"),
            PathBuf::from("double.tar.m4a"),
            PathBuf::from("mixed.m4a"),
        ]
    );
    assert_eq!(plan.ignored.len(), 3);

    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(outcome.summary.converted, 4);
    assert_eq!(outcome.summary.failed, 0);
    for expected in &mapped {
        assert!(output.join(expected).is_file(), "{expected:?} missing");
        let extension = output
            .join(expected)
            .extension()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            matches!(extension.as_str(), "m4a" | "aac"),
            "non-canonical batch extension {extension}"
        );
    }
}

#[test]
fn hidden_files_and_directories_are_traversed() {
    let workspace = TempDir::new("batch-hidden");
    let input = workspace.join("in");
    seed_file(&input, ".hidden.flac", "stereo-48k-16.flac");
    seed_file(&input, ".hiddendir/nested.flac", "mono-441k-16.flac");

    let output = workspace.join("out");
    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(plan.entries.len(), 2);
    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(outcome.summary.converted, 2);
    assert!(output.join(".hidden.m4a").is_file());
    assert!(output.join(".hiddendir/nested.m4a").is_file());
}

// ---------------------------------------------------------------------------
// Library: fatal setup (T-empty-1, T-noflac-1, T-sameroot-1)
// ---------------------------------------------------------------------------

#[test]
fn empty_directory_is_fatal_setup() {
    let workspace = TempDir::new("batch-empty");
    let input = workspace.join("in");
    fs::create_dir(&input).unwrap();
    let output = workspace.join("out");
    let err = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap_err();
    assert!(
        matches!(err, BatchError::NoEligibleFiles { empty: true, .. }),
        "{err:?}"
    );
    assert!(!output.exists(), "planning must not create the output root");
}

#[test]
fn directory_without_flac_is_fatal_setup() {
    let workspace = TempDir::new("batch-noflac");
    let input = workspace.join("in");
    fs::create_dir(&input).unwrap();
    write_text(&input, "a.txt", b"text");
    write_text(&input, "b.mp3", b"fake");
    let output = workspace.join("out");
    let err = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap_err();
    assert!(
        matches!(err, BatchError::NoEligibleFiles { empty: false, .. }),
        "{err:?}"
    );
}

#[test]
fn identical_input_and_output_roots_are_fatal() {
    let workspace = TempDir::new("batch-sameroot");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let err = plan_batch(&input, &input, OutputFormat::M4a, true).unwrap_err();
    assert!(matches!(err, BatchError::SameRoot(_)), "{err:?}");
}

// ---------------------------------------------------------------------------
// Library: collisions and failure isolation (T-dup-1, T-exist-1, T-cleanup-1)
// ---------------------------------------------------------------------------

#[test]
fn duplicate_destinations_abort_before_any_conversion() {
    let workspace = TempDir::new("batch-dup");
    let input = workspace.join("in");
    fs::create_dir(&input).unwrap();
    seed_file(&input, "A.flac", "stereo-48k-16.flac");
    // On a case-insensitive filesystem both spellings cannot coexist; that
    // platform behavior is covered by T-casefold-1 instead.
    if fs::copy(fixture("mono-441k-16.flac"), input.join("a.flac")).is_err()
        || !input.join("A.flac").exists()
        || !input.join("a.flac").exists()
        || same_file_hard_check(&input.join("A.flac"), &input.join("a.flac"))
    {
        eprintln!("case-insensitive filesystem: duplicate-source test not applicable");
        return;
    }
    let output = workspace.join("out");
    let err = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap_err();
    match err {
        BatchError::DuplicateDestination {
            first,
            second,
            destination,
        } => {
            assert_eq!(destination, PathBuf::from("a.m4a"));
            assert_ne!(first, second);
        }
        other => panic!("expected DuplicateDestination, got {other:?}"),
    }
    assert!(
        !output.exists(),
        "duplicate preflight must convert nothing and create nothing"
    );
}

/// True when both paths address the same file (case-folded filesystem).
fn same_file_hard_check(a: &Path, b: &Path) -> bool {
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(x), Ok(y)) => {
            use std::os::unix::fs::MetadataExt;
            x.dev() == y.dev() && x.ino() == y.ino()
        }
        _ => false,
    }
}

#[test]
fn existing_destinations_fail_without_touching_sentinels() {
    let workspace = TempDir::new("batch-exist");
    let input = workspace.join("in");
    seed_file(&input, "keep.flac", "stereo-48k-16.flac");
    seed_file(&input, "fresh.flac", "mono-441k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("keep.m4a"), b"sentinel").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(outcome.summary.converted, 1);
    assert_eq!(outcome.summary.failed, 1);
    assert_eq!(outcome.summary.existing, 1);
    assert_eq!(fs::read(output.join("keep.m4a")).unwrap(), b"sentinel");
    assert!(output.join("fresh.m4a").is_file());
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn mixed_valid_and_broken_inputs_convert_what_can_convert() {
    let workspace = TempDir::new("batch-mixed");
    let input = workspace.join("in");
    // Sorted order: bad-bitdepth, broken, good — failures first exercise
    // isolation before the later valid file converts.
    seed_file(&input, "a-bitdepth.flac", "stereo-48k-24.flac");
    seed_file(&input, "b-truncated.flac", "truncated-stereo-48k-16.flac");
    seed_file(&input, "c-good.flac", "stereo-48k-16.flac");

    let output = workspace.join("out");
    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(plan.entries.len(), 3);
    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(outcome.summary.converted, 1);
    assert_eq!(outcome.summary.failed, 2);
    assert_eq!(outcome.summary.existing, 0);

    let mut categories: Vec<&str> = Vec::new();
    for result in &outcome.results {
        if let BatchFileOutcome::Failed { category, .. } = &result.outcome {
            categories.push(category);
        }
    }
    assert_eq!(categories.len(), 2);
    assert!(
        categories.contains(&"UnsupportedBitDepth"),
        "{categories:?}"
    );
    assert!(categories.contains(&"DecodeFailed"), "{categories:?}");

    assert!(output.join("c-good.m4a").is_file());
    assert!(!output.join("a-bitdepth.m4a").exists());
    assert!(!output.join("b-truncated.m4a").exists());
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn nested_output_root_is_safe_and_repeat_runs_report_existing() {
    let workspace = TempDir::new("batch-nestedroot");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    seed_file(&input, "sub/b.flac", "mono-441k-16.flac");
    let output = input.join("conv");

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let first = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(first.summary.converted, 2);
    assert_eq!(first.summary.failed, 0);
    assert!(output.join("a.m4a").is_file());
    assert!(output.join("sub/b.m4a").is_file());
    // Generated outputs are not rediscovered as inputs.
    let replan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(replan.entries.len(), 2);
    let second = execute_plan(&replan, OutputFormat::M4a, false).unwrap();
    assert_eq!(second.summary.converted, 0);
    assert_eq!(second.summary.failed, 2);
    assert_eq!(second.summary.existing, 2);
    assert_no_temp_files_anywhere(&input);
}

// ---------------------------------------------------------------------------
// Library: dry-run (T-dryrun-1, T-dryrun-2)
// ---------------------------------------------------------------------------

#[test]
fn dry_run_plans_without_creating_decoding_or_publishing() {
    let workspace = TempDir::new("batch-dryrun");
    let input = workspace.join("in");
    seed_file(&input, "good.flac", "stereo-48k-16.flac");
    // Corrupt content is still plannable: dry runs never decode.
    seed_file(&input, "broken.flac", "truncated-stereo-48k-16.flac");
    let output = workspace.join("out");

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(plan.entries.len(), 2);
    let outcome = execute_plan(&plan, OutputFormat::M4a, true).unwrap();
    assert_eq!(outcome.summary.converted, 2);
    assert_eq!(outcome.summary.failed, 0);
    assert!(
        outcome
            .results
            .iter()
            .all(|result| matches!(result.outcome, BatchFileOutcome::Planned)),
        "dry run must only plan, never convert"
    );
    assert!(!output.exists(), "dry run must not create the output root");
    assert_no_temp_files_anywhere(&input);

    // The real run then fails on the corrupt file while converting the good one.
    let real = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(real.summary.converted, 1);
    assert_eq!(real.summary.failed, 1);
}

#[test]
fn dry_run_reports_existing_destinations_without_writing() {
    let workspace = TempDir::new("batch-dryrun-exist");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("a.m4a"), b"sentinel").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan(&plan, OutputFormat::M4a, true).unwrap();
    assert_eq!(outcome.summary.converted, 0);
    assert_eq!(outcome.summary.failed, 1);
    assert_eq!(outcome.summary.existing, 1);
    assert_eq!(fs::read(output.join("a.m4a")).unwrap(), b"sentinel");
    assert_eq!(
        fs::read_dir(&output).unwrap().count(),
        1,
        "dry run must write nothing"
    );
}

// ---------------------------------------------------------------------------
// Library: container structure (T-m4a-1, T-adts-1) and pipeline identity
// ---------------------------------------------------------------------------

#[test]
fn batch_m4a_outputs_carry_smpb_without_edit_list() {
    let workspace = TempDir::new("batch-m4a");
    let input = workspace.join("in");
    seed_file(&input, "stereo.flac", "stereo-48k-16.flac");
    seed_file(&input, "mono.flac", "mono-441k-16.flac");
    let output = workspace.join("out");

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(outcome.summary.converted, 2);

    // D = 1600, P = 736, S = 12000 for the stereo fixture.
    let stereo = fs::read(output.join("stereo.m4a")).unwrap();
    let root = parse_mp4(&stereo);
    let moov = root.require("moov");
    assert_eq!(&stereo[8..12], b"M4A ");
    assert!(moov.require("trak").find("edts").is_none());
    assert_eq!(mp4_mdhd(&stereo, moov.require("mdhd")), (48_000, 14 * 1024));
    assert_eq!(
        mp4_itunsmpb(&stereo, &root),
        " 00000000 00000640 000002E0 0000000000002EE0 \
         00000000 00000000 00000000 00000000 \
         00000000 00000000 00000000 00000000"
    );

    // D = 1600, P = 520, S = 3000 for the mono fixture.
    let mono = fs::read(output.join("mono.m4a")).unwrap();
    let root = parse_mp4(&mono);
    let moov = root.require("moov");
    assert_eq!(mp4_mdhd(&mono, moov.require("mdhd")), (44_100, 5 * 1024));
    assert_eq!(
        mp4_itunsmpb(&mono, &root),
        " 00000000 00000640 00000208 0000000000000BB8 \
         00000000 00000000 00000000 00000000 \
         00000000 00000000 00000000 00000000"
    );
}

#[test]
fn batch_adts_outputs_are_structurally_valid() {
    let workspace = TempDir::new("batch-adts");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let plan = plan_batch(&input, &output, OutputFormat::Adts, true).unwrap();
    assert_eq!(
        plan.entries[0].relative_output,
        PathBuf::from("a.aac"),
        "ADTS batch extension is canonical .aac"
    );
    let outcome = execute_plan(&plan, OutputFormat::Adts, false).unwrap();
    assert_eq!(outcome.summary.converted, 1);

    let bytes = fs::read(output.join("a.aac")).unwrap();
    let frames = parse_adts(&bytes);
    assert_eq!(frames.len(), 14);
    for frame in &frames {
        assert_eq!(frame.profile, 1);
        assert_eq!(frame.sample_rate_index, 3);
        assert_eq!(frame.channel_configuration, 2);
        assert_eq!(frame.raw_blocks, 0);
    }
}

#[test]
fn batch_output_is_byte_identical_to_single_file_output() {
    // Deterministic bounded-memory equivalence check: batch execution must
    // reuse the exact single-file pipeline (same chunking, same publication),
    // so outputs are byte-identical to one-file conversions.
    let workspace = TempDir::new("batch-identity");
    let input = workspace.join("in");
    seed_file(&input, "x.flac", "stereo-48k-16.flac");
    seed_file(&input, "y.flac", "mono-441k-16.flac");

    let batch_out = workspace.join("batch");
    let plan = plan_batch(&input, &batch_out, OutputFormat::M4a, true).unwrap();
    // Convert the same files sequentially through one shared output tree to
    // also prove peak resource use never exceeds one file at a time: each
    // iteration leaves no temp files behind.
    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(outcome.summary.converted, 2);

    // Direct byte comparison against fresh single-file conversions proves
    // pipeline identity (same chunking, same publication).
    // The loop above already proved the shared tree converts cleanly with no
    // temp files left behind; the assertions below prove pipeline identity.
    let fresh = TempDir::new("batch-identity-fresh");
    aac_convert::convert_file_with_format(
        &input.join("x.flac"),
        &fresh.join("x.m4a"),
        OutputFormat::M4a,
    )
    .unwrap();
    assert_eq!(
        fs::read(batch_out.join("x.m4a")).unwrap(),
        fs::read(fresh.join("x.m4a")).unwrap(),
        "batch M4A must equal single-file M4A byte for byte"
    );
    let fresh_adts = TempDir::new("batch-identity-fresh-adts");
    let batch_adts_out = workspace.join("batch-adts");
    let plan = plan_batch(&input, &batch_adts_out, OutputFormat::Adts, true).unwrap();
    execute_plan(&plan, OutputFormat::Adts, false).unwrap();
    aac_convert::convert_file_with_format(
        &input.join("y.flac"),
        &fresh_adts.join("y.aac"),
        OutputFormat::Adts,
    )
    .unwrap();
    assert_eq!(
        fs::read(batch_adts_out.join("y.aac")).unwrap(),
        fs::read(fresh_adts.join("y.aac")).unwrap(),
        "batch ADTS must equal single-file ADTS byte for byte"
    );
    assert_no_temp_files_anywhere(workspace.path());
}

// ---------------------------------------------------------------------------
// Library: concurrent publication race (T-race-1)
// ---------------------------------------------------------------------------

#[test]
fn concurrent_batch_runs_never_replace_the_winner() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let workspace = TempDir::new("batch-race");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let plan = Arc::new(plan_batch(&input, &output, OutputFormat::M4a, true).unwrap());
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let plan = Arc::clone(&plan);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            execute_plan(&plan, OutputFormat::M4a, false).unwrap()
        }));
    }
    barrier.wait();
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let converted: u64 = outcomes.iter().map(|o| o.summary.converted).sum();
    let failed: u64 = outcomes.iter().map(|o| o.summary.failed).sum();
    assert_eq!(converted, 1, "exactly one racer must win");
    assert_eq!(failed, 1, "exactly one racer must lose");
    assert!(
        outcomes
            .iter()
            .flat_map(|o| &o.results)
            .any(|r| r.is_existing()),
        "the loser must report OutputExists"
    );
    parse_mp4(&fs::read(output.join("a.m4a")).unwrap());
    assert_no_temp_files_anywhere(&output);
}

// ---------------------------------------------------------------------------
// Library: platform-gated symlink and permission tests (T-symlink-1, T-perm-1)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn symlinked_directories_are_not_descended_but_files_are() {
    use std::os::unix::fs::symlink;

    let workspace = TempDir::new("batch-symlink");
    let input = workspace.join("in");
    let real = input.join("real");
    fs::create_dir_all(&real).unwrap();
    seed_file(&real, "inner.flac", "stereo-48k-16.flac");
    seed_file(&input, "top.flac", "mono-441k-16.flac");
    symlink(&real, input.join("linked-dir")).unwrap();
    symlink(real.join("inner.flac"), input.join("linked-file.flac")).unwrap();
    symlink(input.join("missing.flac"), input.join("broken.flac")).unwrap();
    write_text(&input, "linked-dir-decoy-check.txt", b"x");

    let output = workspace.join("out");
    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    // top.flac + real/inner.flac + linked-file.flac are candidates; the
    // symlinked dir and the broken link are not.
    assert_eq!(plan.entries.len(), 3);
    assert!(
        plan.entries
            .iter()
            .any(|e| e.relative_input == *Path::new("linked-file.flac")),
        "symlinked files are accepted"
    );
    assert!(
        plan.ignored
            .iter()
            .any(|i| i.relative == *Path::new("linked-dir")),
        "symlinked dirs are ignored, never descended"
    );
    assert_eq!(plan.discovery_failures.len(), 1);
    assert_eq!(
        plan.discovery_failures[0].relative,
        PathBuf::from("broken.flac")
    );
    assert_eq!(plan.discovery_failures[0].category, "InputOpen");

    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(outcome.summary.converted, 3);
    assert_eq!(outcome.summary.failed, 1);
    assert!(output.join("linked-file.m4a").is_file());
    assert!(output.join("real/inner.m4a").is_file());
    // The linked tree was converted once, not twice through the symlink.
    assert!(
        !output.join("linked-dir").exists(),
        "symlinked dir must not be mirrored"
    );
    assert_no_temp_files_anywhere(&output);
}

#[cfg(unix)]
#[test]
fn unreadable_subdirectory_fails_only_its_own_entry() {
    use std::os::unix::fs::PermissionsExt;

    let workspace = TempDir::new("batch-perm");
    let input = workspace.join("in");
    seed_file(&input, "good.flac", "stereo-48k-16.flac");
    let locked = input.join("locked");
    fs::create_dir(&locked).unwrap();
    seed_file(&locked, "hidden.flac", "mono-441k-16.flac");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    // Running as root (or on filesystems ignoring Unix permissions) makes
    // the directory readable anyway; that platform state cannot test this.
    let applicable = fs::read_dir(&locked).is_err();
    // Restore before any assertion so the TempDir cleanup always succeeds.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if !applicable {
        eprintln!("permissions are not enforced here; permission test not applicable");
        return;
    }
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let plan = plan_batch(&input, &workspace.join("out"), OutputFormat::M4a, true).unwrap();
    // Restore immediately; planning is done and conversion only reads files.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        plan.discovery_failures
            .iter()
            .any(|f| f.relative == *Path::new("locked")),
        "unreadable subdir must be a per-file failure: {:?}",
        plan.discovery_failures
    );
    let outcome = execute_plan(&plan, OutputFormat::M4a, false).unwrap();
    assert_eq!(outcome.summary.converted, 1);
    assert_eq!(outcome.summary.failed, 1);
}

// ---------------------------------------------------------------------------
// CLI: exit-code matrix and reporting (T-exit-1, T-summary-1, T-multi-1)
// ---------------------------------------------------------------------------

#[test]
fn cli_batch_reports_ok_lines_and_summary_with_exit_zero() {
    let workspace = TempDir::new("cli-batch-ok");
    let input = workspace.join("in");
    seed_file(&input, "b.flac", "mono-441k-16.flac");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let lines = stdout_lines(&result);
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert!(
        lines[0].starts_with("batch: 2 files, format M4A, recursive"),
        "{lines:?}"
    );
    assert_eq!(
        lines[1], "ok a.flac -> a.m4a (14 frames, 1922 bytes)",
        "{lines:?}"
    );
    assert!(
        lines[2].starts_with("ok b.flac -> b.m4a (5 frames, "),
        "{lines:?}"
    );
    assert!(
        lines[3].starts_with("summary: 2 converted, 0 failed, 0 existing, 0 ignored ("),
        "{lines:?}"
    );
}

#[test]
fn cli_batch_partial_failure_exits_one_with_fail_lines() {
    let workspace = TempDir::new("cli-batch-partial");
    let input = workspace.join("in");
    seed_file(&input, "good.flac", "stereo-48k-16.flac");
    seed_file(&input, "bad.flac", "stereo-48k-24.flac");
    let output = workspace.join("out");

    let result = run_cli(&[input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    let lines = stdout_lines(&result);
    assert!(
        lines
            .iter()
            .any(|l| l == "fail bad.flac: UnsupportedBitDepth"),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("ok good.flac -> good.m4a ")),
        "{lines:?}"
    );
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("summary: 1 converted, 1 failed, 0 existing, 0 ignored ("),
        "{lines:?}"
    );
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    assert!(stderr.contains("bad.flac"), "{stderr}");
    assert!(stderr.contains("unsupported bit depth 24"), "{stderr}");
    assert!(output.join("good.m4a").is_file());
}

#[test]
fn cli_batch_usage_errors_exit_two() {
    let workspace = TempDir::new("cli-batch-usage");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    // Unknown flag.
    let result = run_cli(&["--bogus", input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(2));
    // Three positionals.
    let result = run_cli(&[input.to_str().unwrap(), output.to_str().unwrap(), "extra"]);
    assert_eq!(result.status.code(), Some(2));
    // Verbose + quiet conflict.
    let result = run_cli(&[
        "-v",
        "-q",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(2));
    // Invalid format value.
    let result = run_cli(&[
        "--format",
        "wav",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(2));
    // Batch-only flag on a file input.
    let file = fixture("stereo-48k-16.flac");
    let result = run_cli(&[
        "--dry-run",
        file.to_str().unwrap(),
        output.join("x.m4a").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn cli_batch_setup_failures_exit_three() {
    let workspace = TempDir::new("cli-batch-setup");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");

    // Missing input.
    let result = run_cli(&[
        workspace.join("missing").to_str().unwrap(),
        workspace.join("o1").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(3), "missing input");
    // Empty directory.
    let empty = workspace.join("empty");
    fs::create_dir(&empty).unwrap();
    let result = run_cli(&[
        empty.to_str().unwrap(),
        workspace.join("o2").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(3), "empty dir");
    // No eligible FLAC.
    let noflac = workspace.join("noflac");
    fs::create_dir(&noflac).unwrap();
    write_text(&noflac, "a.txt", b"x");
    let result = run_cli(&[
        noflac.to_str().unwrap(),
        workspace.join("o3").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(3), "no FLAC");
    // Same root.
    let result = run_cli(&[input.to_str().unwrap(), input.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(3), "same root");
    // Output root under a regular file.
    let blocker = workspace.join("blocker");
    fs::write(&blocker, b"x").unwrap();
    let result = run_cli(&[
        input.to_str().unwrap(),
        blocker.join("sub").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(3), "uncreatable root");
    assert!(!blocker.join("sub").exists());
}

#[test]
fn cli_batch_quiet_prints_only_the_summary() {
    let workspace = TempDir::new("cli-batch-quiet");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    write_text(&input, "skip.txt", b"x");
    let output = workspace.join("out");

    let result = run_cli(&["-q", input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(0));
    let lines = stdout_lines(&result);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("summary: 1 converted, 0 failed, 0 existing, 1 ignored ("),
        "{lines:?}"
    );

    // Quiet mode still reports failures on stderr and exits 1.
    seed_file(&input, "bad.flac", "stereo-48k-24.flac");
    let output2 = workspace.join("out2");
    let result = run_cli(&[
        "--quiet",
        input.to_str().unwrap(),
        output2.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    let lines = stdout_lines(&result);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("summary: 1 converted, 1 failed, 0 existing, 1 ignored ("),
        "{lines:?}"
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("bad.flac"));
}

#[test]
fn cli_batch_verbose_reports_ignores_and_details() {
    let workspace = TempDir::new("cli-batch-verbose");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    write_text(&input, "skip.txt", b"x");
    let output = workspace.join("out");

    let result = run_cli(&[
        "--verbose",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    let lines = stdout_lines(&result);
    assert!(
        lines[0].starts_with("batch: 1 files, format M4A, recursive"),
        "{lines:?}"
    );
    assert!(lines.iter().any(|l| l == "ignore skip.txt"), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("ok a.flac -> a.m4a (14 frames, 1922 bytes, 48000 Hz, stereo")),
        "{lines:?}"
    );
}

#[test]
fn cli_batch_dry_run_writes_nothing() {
    let workspace = TempDir::new("cli-batch-dryrun");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    seed_file(&input, "sub/b.flac", "mono-441k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[
        "--dry-run",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    let lines = stdout_lines(&result);
    assert!(lines[0].contains("dry-run"), "{lines:?}");
    assert!(
        lines.iter().any(|l| l == "plan a.flac -> a.m4a"),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "plan sub/b.flac -> sub/b.m4a"),
        "{lines:?}"
    );
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("dry-run: 2 converted, 0 failed, 0 existing, 0 ignored ("),
        "{lines:?}"
    );
    assert!(!output.exists(), "dry run must create nothing");
}

#[test]
fn cli_batch_flag_precedence_and_format_forms() {
    let workspace = TempDir::new("cli-batch-flags");
    let input = workspace.join("in");
    seed_file(&input, "top.flac", "stereo-48k-16.flac");
    seed_file(&input, "sub/nested.flac", "mono-441k-16.flac");

    // Last of --recursive/--no-recursive wins: --no-recursive last.
    let out1 = workspace.join("o1");
    let result = run_cli(&[
        "--recursive",
        "--no-recursive",
        input.to_str().unwrap(),
        out1.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    assert!(out1.join("top.m4a").is_file());
    assert!(!out1.join("sub/nested.m4a").exists());

    // Reverse order: recursive wins.
    let out2 = workspace.join("o2");
    let result = run_cli(&[
        "--no-recursive",
        "--recursive",
        input.to_str().unwrap(),
        out2.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    assert!(out2.join("sub/nested.m4a").is_file());

    // --format= form selects ADTS with canonical .aac outputs.
    let out3 = workspace.join("o3");
    let result = run_cli(&[
        "--format=adts",
        "--no-recursive",
        input.to_str().unwrap(),
        out3.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    let bytes = fs::read(out3.join("top.aac")).unwrap();
    assert_eq!(bytes[0], 0xFF);
    assert!(!out3.join("top.adts").exists());
    assert!(!out3.join("top.m4a").exists());
}

#[test]
fn cli_batch_default_format_is_m4a_and_adts_is_explicit() {
    let workspace = TempDir::new("cli-batch-format");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");

    let out_m4a = workspace.join("default");
    let result = run_cli(&[input.to_str().unwrap(), out_m4a.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(0));
    assert!(out_m4a.join("a.m4a").is_file());
    assert_eq!(&fs::read(out_m4a.join("a.m4a")).unwrap()[4..8], b"ftyp");

    let out_adts = workspace.join("explicit");
    let result = run_cli(&[
        "--format",
        "adts",
        input.to_str().unwrap(),
        out_adts.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    assert!(out_adts.join("a.aac").is_file());
    assert!(!out_adts.join("a.m4a").exists());
}
