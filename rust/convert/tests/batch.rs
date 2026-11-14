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

use aac_convert::batch::{
    default_jobs, execute_plan, execute_plan_with_options_and_progress, plan_batch, BatchError,
    BatchEvent, BatchFileOutcome, BatchOptions, BatchPlan, NON_RECURSIVE_REASON,
    NON_REGULAR_FILE_REASON, SYMLINKED_DIR_REASON, UNSUPPORTED_EXTENSION_REASON,
};
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

    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
    assert_eq!(
        outcome.summary,
        aac_convert::batch::BatchSummary {
            converted: 2,
            failed: 0,
            existing: 0,
            ignored: 2,
            skipped: 0,
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

    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
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
fn progress_events_follow_plan_order_with_one_terminal_event_each() {
    let workspace = TempDir::new("batch-progress-order");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    seed_file(&input, "b.flac", "three-channel-48k-16.flac");
    seed_file(&input, "c.flac", "mono-441k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("c.m4a"), b"sentinel").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(plan.entries.len(), 3);
    let mut events = Vec::new();
    let execution = execute_plan_with_options_and_progress(
        &plan,
        OutputFormat::M4a,
        &BatchOptions {
            skip_existing: true,
            // This test pins the sequential event contract; concurrency is
            // covered by the jobs-specific tests below.
            jobs: 1,
            ..BatchOptions::default()
        },
        |event| events.push(event),
    )
    .unwrap();

    let kinds: Vec<&str> = events
        .iter()
        .map(|event| match event {
            BatchEvent::Preparing { .. } => "preparing",
            BatchEvent::Started(_) => "started",
            BatchEvent::Converted { .. } => "converted",
            BatchEvent::Skipped(_) => "skipped",
            BatchEvent::Failed { .. } => "failed",
            BatchEvent::Planned(_) => "planned",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "preparing",
            "started",
            "converted",
            "started",
            "failed",
            "skipped"
        ],
        "{events:?}"
    );

    let inputs: Vec<PathBuf> = events
        .iter()
        .filter_map(|event| {
            let item = match event {
                BatchEvent::Preparing { .. } => return None,
                BatchEvent::Started(item)
                | BatchEvent::Skipped(item)
                | BatchEvent::Planned(item)
                | BatchEvent::Converted { item, .. }
                | BatchEvent::Failed { item, .. } => item,
            };
            assert_eq!(item.total, 3);
            Some(item.relative_input.clone())
        })
        .collect();
    assert_eq!(
        inputs,
        vec![
            PathBuf::from("a.flac"),
            PathBuf::from("a.flac"),
            PathBuf::from("b.flac"),
            PathBuf::from("b.flac"),
            PathBuf::from("c.flac"),
        ]
    );

    for index in 1..=3 {
        let terminal = events
            .iter()
            .filter(|event| {
                let item = match event {
                    BatchEvent::Preparing { .. } => return false,
                    BatchEvent::Started(item)
                    | BatchEvent::Skipped(item)
                    | BatchEvent::Planned(item)
                    | BatchEvent::Converted { item, .. }
                    | BatchEvent::Failed { item, .. } => item,
                };
                item.index == index && !matches!(event, BatchEvent::Started(_))
            })
            .count();
        assert_eq!(terminal, 1, "item {index} terminal count: {events:?}");
    }
    assert_eq!(execution.outcome.summary.converted, 1, "a.flac converts");
    assert_eq!(execution.outcome.summary.failed, 1, "b.flac fails");
    assert_eq!(execution.outcome.summary.skipped, 1, "c.m4a is retained");
    assert_eq!(fs::read(output.join("c.m4a")).unwrap(), b"sentinel");
}

#[test]
fn dry_run_progress_never_emits_started_or_encodes() {
    let workspace = TempDir::new("batch-progress-dry");
    let input = workspace.join("in");
    seed_file(&input, "free.flac", "stereo-48k-16.flac");
    seed_file(&input, "taken.flac", "mono-441k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("taken.m4a"), b"sentinel").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let mut events = Vec::new();
    let execution = execute_plan_with_options_and_progress(
        &plan,
        OutputFormat::M4a,
        &BatchOptions {
            dry_run: true,
            ..BatchOptions::default()
        },
        |event| events.push(event),
    )
    .unwrap();

    assert!(
        events
            .iter()
            .all(|event| !matches!(event, BatchEvent::Started(_) | BatchEvent::Preparing { .. })),
        "dry runs must not pretend to encode or analyze: {events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, BatchEvent::Planned(_)))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, BatchEvent::Failed { .. }))
            .count(),
        1
    );
    assert_eq!(execution.outcome.summary.converted, 1);
    assert_eq!(execution.outcome.summary.existing, 1);
    assert_eq!(fs::read(output.join("taken.m4a")).unwrap(), b"sentinel");
}

#[test]
fn empty_plan_emits_no_events_and_zero_summary() {
    let workspace = TempDir::new("batch-progress-empty");
    let plan = BatchPlan {
        input_root: workspace.join("in"),
        output_root: workspace.join("out"),
        entries: Vec::new(),
        ignored: Vec::new(),
        discovery_failures: Vec::new(),
    };
    let mut events = Vec::new();
    let execution = execute_plan_with_options_and_progress(
        &plan,
        OutputFormat::M4a,
        &BatchOptions {
            dry_run: true,
            ..BatchOptions::default()
        },
        |event| events.push(event),
    )
    .unwrap();
    assert!(events.is_empty(), "{events:?}");
    assert!(execution.outcome.results.is_empty());
    assert_eq!(
        execution.outcome.summary,
        aac_convert::batch::BatchSummary {
            converted: 0,
            failed: 0,
            existing: 0,
            ignored: 0,
            skipped: 0,
        }
    );
    assert!(execution.cover_art.is_empty());
    assert!(!workspace.join("out").exists());
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

    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
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

    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
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
fn ignored_entries_report_planner_reasons() {
    let workspace = TempDir::new("batch-ignore-reasons");
    let input = workspace.join("in");
    seed_file(&input, "top.flac", "stereo-48k-16.flac");
    seed_file(&input, "sub/nested.flac", "mono-441k-16.flac");
    write_text(&input, "note.txt", b"not audio");
    write_text(&input, "extensionless", b"no extension");

    let output = workspace.join("out");
    let top_only = plan_batch(&input, &output, OutputFormat::M4a, false).unwrap();
    for (path, reason) in [
        ("note.txt", UNSUPPORTED_EXTENSION_REASON),
        ("extensionless", UNSUPPORTED_EXTENSION_REASON),
        ("sub/nested.flac", NON_RECURSIVE_REASON),
    ] {
        assert!(
            top_only
                .ignored
                .iter()
                .any(|ignored| ignored.relative == *Path::new(path)
                    && ignored.reason == Some(reason)),
            "missing reason {reason} for {path}: {:?}",
            top_only.ignored
        );
    }

    let recursive = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(recursive.entries.len(), 2);
    for (path, reason) in [
        ("note.txt", UNSUPPORTED_EXTENSION_REASON),
        ("extensionless", UNSUPPORTED_EXTENSION_REASON),
    ] {
        assert!(
            recursive
                .ignored
                .iter()
                .any(|ignored| ignored.relative == *Path::new(path)
                    && ignored.reason == Some(reason)),
            "missing reason {reason} for {path}: {:?}",
            recursive.ignored
        );
    }
}

#[cfg(unix)]
#[test]
fn non_regular_entries_report_their_reason() {
    let workspace = TempDir::new("batch-ignore-fifo");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let fifo = input.join("pipe");
    let created = Command::new("mkfifo").arg(&fifo).status();
    if !created.map(|status| status.success()).unwrap_or(false) {
        eprintln!("mkfifo is not available; non-regular entry test skipped");
        return;
    }

    let plan = plan_batch(&input, &workspace.join("out"), OutputFormat::M4a, true).unwrap();
    assert_eq!(plan.entries.len(), 1);
    assert!(
        plan.ignored
            .iter()
            .any(|ignored| ignored.relative == *Path::new("pipe")
                && ignored.reason == Some(NON_REGULAR_FILE_REASON)),
        "{:?}",
        plan.ignored
    );
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
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
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
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
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
    // Sorted order: bad-channels, broken, good — failures first exercise
    // isolation before the later valid file converts.
    seed_file(&input, "a-channels.flac", "three-channel-48k-16.flac");
    seed_file(&input, "b-truncated.flac", "truncated-stereo-48k-16.flac");
    seed_file(&input, "c-good.flac", "stereo-48k-16.flac");

    let output = workspace.join("out");
    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(plan.entries.len(), 3);
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
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
        categories.contains(&"UnsupportedChannels"),
        "{categories:?}"
    );
    assert!(categories.contains(&"DecodeFailed"), "{categories:?}");

    assert!(output.join("c-good.m4a").is_file());
    assert!(!output.join("a-channels.m4a").exists());
    assert!(!output.join("b-truncated.m4a").exists());
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn mixed_16_and_24_bit_batch_converts_both_with_targeted_warning() {
    // The warning routes through per-file diagnostics identically for M4A
    // and ADTS output.
    for format in [OutputFormat::M4a, OutputFormat::Adts] {
        let workspace = TempDir::new("batch-mixed-depth");
        let input = workspace.join("in");
        seed_file(&input, "plain.flac", "stereo-48k-16.flac");
        seed_file(&input, "deep.flac", "stereo-48k-24.flac");
        let output = workspace.join("out");

        let plan = plan_batch(&input, &output, format, true).unwrap();
        assert_eq!(plan.entries.len(), 2);
        let outcome = execute_plan(&plan, format, false, false).unwrap();
        assert_eq!(outcome.summary.converted, 2);
        assert_eq!(outcome.summary.failed, 0);

        for result in &outcome.results {
            match &result.outcome {
                BatchFileOutcome::Converted {
                    properties, tags, ..
                } => {
                    let precision = tags
                        .warnings
                        .iter()
                        .filter(|w| w.contains("24-bit FLAC input"))
                        .count();
                    if properties.bits_per_sample == 24 {
                        assert!(result.relative_input.ends_with("deep.flac"));
                        assert_eq!(precision, 1, "{tags:?}");
                    } else {
                        assert_eq!(precision, 0, "{tags:?}");
                    }
                }
                other => panic!("unexpected outcome: {other:?}"),
            }
        }
        assert_no_temp_files_anywhere(&output);
    }
}

#[test]
fn nested_output_root_is_safe_and_repeat_runs_report_existing() {
    let workspace = TempDir::new("batch-nestedroot");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    seed_file(&input, "sub/b.flac", "mono-441k-16.flac");
    let output = input.join("conv");

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let first = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
    assert_eq!(first.summary.converted, 2);
    assert_eq!(first.summary.failed, 0);
    assert!(output.join("a.m4a").is_file());
    assert!(output.join("sub/b.m4a").is_file());
    // Generated outputs are not rediscovered as inputs.
    let replan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(replan.entries.len(), 2);
    let second = execute_plan(&replan, OutputFormat::M4a, false, false).unwrap();
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
    let outcome = execute_plan(&plan, OutputFormat::M4a, true, false).unwrap();
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
    let real = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
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
    let outcome = execute_plan(&plan, OutputFormat::M4a, true, false).unwrap();
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
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
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
    let outcome = execute_plan(&plan, OutputFormat::Adts, false, false).unwrap();
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
    // Convert the same files through one shared output tree. Each item
    // publishes through the same temporary-file path, so the tree converts
    // cleanly with no temp files left behind.
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
    assert_eq!(outcome.summary.converted, 2);

    // Direct byte comparison against fresh single-file conversions proves
    // pipeline identity (same chunking, same publication).
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
    execute_plan(&plan, OutputFormat::Adts, false, false).unwrap();
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
            execute_plan(&plan, OutputFormat::M4a, false, false).unwrap()
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
            .any(|i| i.relative == *Path::new("linked-dir")
                && i.reason == Some(SYMLINKED_DIR_REASON)),
        "symlinked dirs are ignored with a reason, never descended: {:?}",
        plan.ignored
    );
    assert_eq!(plan.discovery_failures.len(), 1);
    assert_eq!(
        plan.discovery_failures[0].relative,
        PathBuf::from("broken.flac")
    );
    assert_eq!(plan.discovery_failures[0].category, "InputOpen");

    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
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
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
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
        lines[0].starts_with("batch: 2 files, format M4A, VBR-6, recursive"),
        "{lines:?}"
    );
    assert_eq!(
        // VBR-6 default (20 kHz): +2 bytes over the VBR-5 1959-byte baseline
        // (+37 bytes over the pre-metadata baseline for `©too`), minus the
        // 72-byte `free` placeholder dropped by fast-start relocation
        // (ftyp/moov/mdat with no free box).
        lines[1],
        "ok a.flac -> a.m4a (14 frames, 1889 bytes)",
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
fn cli_batch_writes_live_progress_to_stderr_and_results_to_stdout() {
    let workspace = TempDir::new("cli-batch-progress");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    seed_file(&input, "sub/b.flac", "mono-441k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[
        "--jobs",
        "1",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("Preparing 2 files across 2 albums"),
        "preparation header must precede per-file progress: {stderr}"
    );
    assert!(stderr.contains("Preparation"), "{stderr}");
    assert!(
        stderr.contains("Artwork discovery    2/2  100%"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Loudness analysis    2/2  100%"),
        "{stderr}"
    );
    assert!(stderr.contains("Prepared             2 files"), "{stderr}");
    assert!(stderr.contains("Encoding"), "{stderr}");
    assert!(
        stderr.contains("Converted            2 converted, 0 skipped, 0 failed"),
        "{stderr}"
    );
    // Redirected output must be plain newline-delimited text, not a live
    // carriage-return display.
    assert!(
        !stderr.contains('\r'),
        "redirected progress must not use \\r"
    );
    assert!(
        !stderr.contains("encoding..."),
        "normal mode should not emit a line per file: {stderr}"
    );
    // Stdout keeps the scriptable result and summary shapes.
    let lines = stdout_lines(&result);
    assert!(
        lines.iter().any(|l| l.starts_with("ok a.flac -> a.m4a ")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("ok sub/b.flac -> sub/b.m4a ")),
        "{lines:?}"
    );
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("summary: 2 converted, 0 failed, 0 existing, 0 ignored ("),
        "{lines:?}"
    );
}

#[test]
fn cli_batch_progress_keeps_long_nested_paths_visible() {
    let workspace = TempDir::new("cli-batch-long-progress");
    let input = workspace.join("in");
    let long_name = format!("{}.flac", "very-long-track-name-".repeat(6));
    seed_file(
        &input,
        &format!("album/disc/{long_name}"),
        "stereo-48k-16.flac",
    );
    let output = workspace.join("out");

    let result = run_cli(&[
        "--verbose",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains(&format!("  encoding album/disc/{long_name}")),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("  done     album/disc/{long_name}")),
        "{stderr}"
    );
}

#[test]
fn cli_batch_quiet_suppresses_progress_but_keeps_findings() {
    let workspace = TempDir::new("cli-batch-quiet-progress");
    let input = workspace.join("in");
    seed_file(&input, "good.flac", "stereo-48k-16.flac");
    seed_file(&input, "bad.flac", "three-channel-48k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&["--quiet", input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.contains("encoding...") && !stderr.contains("done"),
        "quiet must suppress normal progress: {stderr}"
    );
    assert!(
        stderr.contains("error: bad.flac:"),
        "failures remain visible: {stderr}"
    );
    let lines = stdout_lines(&result);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("summary: "), "{lines:?}");
}

#[test]
fn cli_batch_dry_run_prints_plan_without_progress_lines() {
    let workspace = TempDir::new("cli-batch-dry-progress");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[
        "--dry-run",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.is_empty(),
        "dry runs must not pretend to encode: {stderr}"
    );
    let lines = stdout_lines(&result);
    assert!(
        lines.iter().any(|l| l == "plan a.flac -> a.m4a"),
        "{lines:?}"
    );
    assert!(!output.exists());
}

#[test]
fn cli_batch_warning_heavy_output_stays_throttled() {
    // Each 24-bit file emits a precision warning; diagnostics must not reset
    // the non-interactive progress throttle and produce a line per file.
    let workspace = TempDir::new("cli-batch-throttled");
    let input = workspace.join("in");
    for index in 0..20 {
        seed_file(
            &input,
            &format!("track-{index:02}.flac"),
            "stereo-48k-24.flac",
        );
    }
    let output = workspace.join("out");

    let result = run_cli(&[
        "--jobs",
        "1",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!stderr.contains('\r'), "redirected output must not use \\r");
    let file_lines: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains("Files") && line.contains('%'))
        .collect();
    assert!(
        file_lines.len() <= 12,
        "progress must stay throttled despite per-file warnings ({} lines): {stderr}",
        file_lines.len()
    );
    let warnings = stderr
        .lines()
        .filter(|line| line.starts_with("warning: "))
        .count();
    assert!(
        warnings >= 20,
        "every file must still warn (at least the 24-bit notice): {stderr}"
    );
}

#[test]
fn cli_batch_partial_failure_exits_one_with_fail_lines() {
    let workspace = TempDir::new("cli-batch-partial");
    let input = workspace.join("in");
    seed_file(&input, "good.flac", "stereo-48k-16.flac");
    seed_file(&input, "bad.flac", "three-channel-48k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
    let lines = stdout_lines(&result);
    assert!(
        lines
            .iter()
            .any(|l| l == "fail bad.flac: UnsupportedChannels"),
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
    assert!(stderr.contains("unsupported channel count 3"), "{stderr}");
    assert!(output.join("good.m4a").is_file());
}

#[test]
fn cli_batch_24_bit_warns_once_per_affected_file() {
    let workspace = TempDir::new("cli-batch-24");
    let input = workspace.join("in");
    seed_file(&input, "plain.flac", "stereo-48k-16.flac");
    seed_file(&input, "deep.flac", "stereo-48k-24.flac");
    let output = workspace.join("out");

    let result = run_cli(&[input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    // Exactly one precision notice naming the 24-bit file — not one per PCM
    // chunk, and none for the 16-bit file. (Tag warnings for both files may
    // also be present; they are out of scope for this assertion.)
    let precision: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains("24-bit FLAC input is truncated to 16-bit PCM"))
        .collect();
    assert_eq!(precision.len(), 1, "{stderr}");
    assert!(precision[0].contains("deep.flac"), "{stderr}");
    assert!(output.join("plain.m4a").is_file());
    assert!(output.join("deep.m4a").is_file());
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn cli_batch_24_bit_dry_run_emits_no_warning() {
    let workspace = TempDir::new("cli-batch-24-dryrun");
    let input = workspace.join("in");
    seed_file(&input, "deep.flac", "stereo-48k-24.flac");
    let output = workspace.join("out");

    let result = run_cli(&[
        "--dry-run",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    // Dry runs never decode, so the precision warning must not appear.
    assert!(
        String::from_utf8_lossy(&result.stderr).is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!output.exists(), "dry run must create nothing");
}

#[test]
fn cli_batch_24_bit_no_tags_still_warns_about_precision() {
    let workspace = TempDir::new("cli-batch-24-notags");
    let input = workspace.join("in");
    seed_file(&input, "deep.flac", "stereo-48k-24.flac");
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
    let stderr = String::from_utf8_lossy(&result.stderr);
    // The precision notice is not a tag warning: `--no-tags` suppresses tag
    // warnings but must preserve it.
    let precision: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains("24-bit FLAC input is truncated to 16-bit PCM"))
        .collect();
    assert_eq!(precision.len(), 1, "{stderr}");
    assert!(
        !stderr.contains("missing title"),
        "tag warnings must stay suppressed: {stderr}"
    );
    assert!(output.join("deep.m4a").is_file());
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
    seed_file(&input, "bad.flac", "three-channel-48k-16.flac");
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
        lines[0].starts_with("batch: 1 files, format M4A, VBR-6, recursive"),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "ignore skip.txt (unsupported extension)"),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("ok a.flac -> a.m4a (14 frames, 1889 bytes, 48000 Hz, stereo")),
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

// ---------------------------------------------------------------------------
// M8.3: --skip-existing (safe batch reruns)
// ---------------------------------------------------------------------------

#[test]
fn skip_existing_leaves_sentinels_untouched_without_decoding() {
    let workspace = TempDir::new("skip-sentinel");
    let input = workspace.join("in");
    seed_file(&input, "good.flac", "stereo-48k-16.flac");
    // A corrupt source behind a pre-existing destination proves skipped files
    // are never decoded: decoding it would fail the run.
    seed_file(&input, "broken.flac", "truncated-stereo-48k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("good.m4a"), b"sentinel-good").unwrap();
    fs::write(output.join("broken.m4a"), b"sentinel-broken").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, true).unwrap();
    assert_eq!(outcome.summary.converted, 0);
    assert_eq!(outcome.summary.failed, 0);
    assert_eq!(outcome.summary.existing, 0);
    assert_eq!(outcome.summary.skipped, 2);
    assert!(
        outcome.results.iter().all(|result| result.is_skipped()),
        "every item must be a skip: {:?}",
        outcome.results
    );
    assert_eq!(fs::read(output.join("good.m4a")).unwrap(), b"sentinel-good");
    assert_eq!(
        fs::read(output.join("broken.m4a")).unwrap(),
        b"sentinel-broken"
    );
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn skip_existing_mixes_skips_and_conversions() {
    let workspace = TempDir::new("skip-mixed");
    let input = workspace.join("in");
    seed_file(&input, "old.flac", "stereo-48k-16.flac");
    seed_file(&input, "new.flac", "mono-441k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("old.m4a"), b"sentinel").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, true).unwrap();
    assert_eq!(outcome.summary.converted, 1);
    assert_eq!(outcome.summary.failed, 0);
    assert_eq!(outcome.summary.skipped, 1);
    assert_eq!(fs::read(output.join("old.m4a")).unwrap(), b"sentinel");
    parse_mp4(&fs::read(output.join("new.m4a")).unwrap());
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn skip_existing_does_not_hide_unrelated_failures() {
    let workspace = TempDir::new("skip-failure");
    let input = workspace.join("in");
    seed_file(&input, "done.flac", "stereo-48k-16.flac");
    seed_file(&input, "bad.flac", "three-channel-48k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("done.m4a"), b"sentinel").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, true).unwrap();
    assert_eq!(outcome.summary.converted, 0);
    assert_eq!(outcome.summary.failed, 1);
    assert_eq!(outcome.summary.skipped, 1);
    assert!(
        outcome.results.iter().any(|result| matches!(
            &result.outcome,
            BatchFileOutcome::Failed {
                category: "UnsupportedChannels",
                ..
            }
        )),
        "the real failure must still be reported: {:?}",
        outcome.results
    );
}

#[test]
fn without_flag_existing_destinations_still_fail() {
    // Default behavior is unchanged by the flag's existence.
    let workspace = TempDir::new("skip-default");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("a.m4a"), b"sentinel").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
    assert_eq!(outcome.summary.converted, 0);
    assert_eq!(outcome.summary.failed, 1);
    assert_eq!(outcome.summary.existing, 1);
    assert_eq!(outcome.summary.skipped, 0);
}

#[test]
fn skip_existing_dry_run_plans_and_skips_without_writing() {
    let workspace = TempDir::new("skip-dryrun");
    let input = workspace.join("in");
    seed_file(&input, "done.flac", "stereo-48k-16.flac");
    seed_file(&input, "todo.flac", "mono-441k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("done.m4a"), b"sentinel").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan(&plan, OutputFormat::M4a, true, true).unwrap();
    assert_eq!(outcome.summary.converted, 1);
    assert_eq!(outcome.summary.failed, 0);
    assert_eq!(outcome.summary.skipped, 1);
    assert!(
        outcome
            .results
            .iter()
            .any(|result| matches!(result.outcome, BatchFileOutcome::Planned)),
        "free destinations must still be planned"
    );
    assert_eq!(fs::read(output.join("done.m4a")).unwrap(), b"sentinel");
    assert!(
        !output.join("todo.m4a").exists(),
        "dry run must write nothing"
    );
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn skip_existing_keeps_duplicate_detection_fatal() {
    let workspace = TempDir::new("skip-dup");
    let input = workspace.join("in");
    fs::create_dir(&input).unwrap();
    seed_file(&input, "A.flac", "stereo-48k-16.flac");
    if fs::copy(fixture("mono-441k-16.flac"), input.join("a.flac")).is_err()
        || same_file_hard_check(&input.join("A.flac"), &input.join("a.flac"))
    {
        eprintln!("case-insensitive filesystem: duplicate-source test not applicable");
        return;
    }
    let output = workspace.join("out");
    // Planning itself refuses the ambiguous plan, flag or not.
    let err = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap_err();
    assert!(
        matches!(err, BatchError::DuplicateDestination { .. }),
        "{err:?}"
    );
    assert!(!output.exists(), "nothing may be converted or created");
}

#[test]
fn skip_existing_turns_publication_races_into_skips() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let workspace = TempDir::new("skip-race");
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
            execute_plan(&plan, OutputFormat::M4a, false, true).unwrap()
        }));
    }
    barrier.wait();
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let converted: u64 = outcomes.iter().map(|o| o.summary.converted).sum();
    let skipped: u64 = outcomes.iter().map(|o| o.summary.skipped).sum();
    let failed: u64 = outcomes.iter().map(|o| o.summary.failed).sum();
    assert_eq!(converted, 1, "exactly one racer must win");
    assert_eq!(skipped, 1, "the loser must skip, not fail");
    assert_eq!(failed, 0, "no failures in a skip race");
    parse_mp4(&fs::read(output.join("a.m4a")).unwrap());
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn cli_skip_existing_rerun_succeeds_without_modifying_outputs() {
    let workspace = TempDir::new("cli-skip-rerun");
    let input = workspace.join("in");
    seed_file(&input, "b.flac", "mono-441k-16.flac");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let first = run_cli(&[input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(first.status.code(), Some(0));
    let before_a = fs::read(output.join("a.m4a")).unwrap();
    let before_b = fs::read(output.join("b.m4a")).unwrap();

    // Default rerun still fails on existing destinations (M8.2 behavior).
    let rerun = run_cli(&[input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(rerun.status.code(), Some(1));
    assert!(
        !String::from_utf8_lossy(&rerun.stdout).contains("skipped"),
        "default summary shape must not mention skips"
    );

    // Flagged rerun is a successful no-op with byte-identical outputs.
    let skipped = run_cli(&[
        "--skip-existing",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(
        skipped.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&skipped.stderr)
    );
    let lines = stdout_lines(&skipped);
    assert!(lines[0].contains("skip-existing"), "{lines:?}");
    assert!(
        lines.iter().any(|l| l == "skip a.flac -> a.m4a (exists)"),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "skip b.flac -> b.m4a (exists)"),
        "{lines:?}"
    );
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("summary: 0 converted, 0 failed, 0 existing, 0 ignored, 2 skipped ("),
        "{lines:?}"
    );
    assert!(
        !String::from_utf8_lossy(&skipped.stderr).contains("warning:"),
        "skip rerun must not warn: {}",
        String::from_utf8_lossy(&skipped.stderr)
    );
    assert_eq!(fs::read(output.join("a.m4a")).unwrap(), before_a);
    assert_eq!(fs::read(output.join("b.m4a")).unwrap(), before_b);
}

#[test]
fn cli_skip_existing_mixed_run_converts_missing_and_reports_failures() {
    let workspace = TempDir::new("cli-skip-mixed");
    let input = workspace.join("in");
    seed_file(&input, "done.flac", "stereo-48k-16.flac");
    seed_file(&input, "todo.flac", "mono-441k-16.flac");
    seed_file(&input, "bad.flac", "three-channel-48k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("done.m4a"), b"sentinel").unwrap();

    let result = run_cli(&[
        "--skip-existing",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    let lines = stdout_lines(&result);
    assert!(
        lines
            .iter()
            .any(|l| l == "skip done.flac -> done.m4a (exists)"),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("ok todo.flac -> todo.m4a ")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "fail bad.flac: UnsupportedChannels"),
        "{lines:?}"
    );
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("summary: 1 converted, 1 failed, 0 existing, 0 ignored, 1 skipped ("),
        "{lines:?}"
    );
    assert_eq!(fs::read(output.join("done.m4a")).unwrap(), b"sentinel");
}

#[test]
fn cli_skip_existing_dry_run_reports_plans_and_skips() {
    let workspace = TempDir::new("cli-skip-dryrun");
    let input = workspace.join("in");
    seed_file(&input, "done.flac", "stereo-48k-16.flac");
    seed_file(&input, "todo.flac", "mono-441k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("done.m4a"), b"sentinel").unwrap();

    let result = run_cli(&[
        "--skip-existing",
        "--dry-run",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0));
    let lines = stdout_lines(&result);
    assert!(
        lines.iter().any(|l| l == "plan todo.flac -> todo.m4a"),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "skip done.flac -> done.m4a (exists)"),
        "{lines:?}"
    );
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("dry-run: 1 converted, 0 failed, 0 existing, 0 ignored, 1 skipped ("),
        "{lines:?}"
    );
    assert!(
        !output.join("todo.m4a").exists(),
        "dry run must write nothing"
    );
}

#[test]
fn cli_skip_existing_with_single_file_input_is_a_usage_error() {
    let workspace = TempDir::new("cli-skip-single");
    let output = workspace.join("out.m4a");
    let result = run_cli(&[
        "--skip-existing",
        fixture("stereo-48k-16.flac").to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("--skip-existing requires a directory input"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!output.exists());
}

#[test]
fn cli_skip_existing_quiet_prints_only_the_summary() {
    let workspace = TempDir::new("cli-skip-quiet");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let first = run_cli(&["-q", input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(first.status.code(), Some(0));
    let rerun = run_cli(&[
        "--skip-existing",
        "-q",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(rerun.status.code(), Some(0));
    let lines = stdout_lines(&rerun);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("summary: 0 converted, 0 failed, 0 existing, 0 ignored, 1 skipped ("),
        "{lines:?}"
    );
}

#[test]
fn stale_temporary_files_are_never_confused_with_outputs() {
    // A crash orphan keeps the recognizable prefix but must neither block a
    // later run nor be mistaken for a completed output.
    let workspace = TempDir::new("batch-stale-tmp");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join(".aac-convert-99999-0.tmp"), b"partial").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(plan.entries.len(), 1);
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
    assert_eq!(outcome.summary.converted, 1);
    assert_eq!(outcome.summary.failed, 0);
    parse_mp4(&fs::read(output.join("a.m4a")).unwrap());
    // Cleanup only removes files the run created itself: the stale file is
    // left alone for manual removal.
    assert_eq!(
        fs::read(output.join(".aac-convert-99999-0.tmp")).unwrap(),
        b"partial"
    );
}

// ---------------------------------------------------------------------------
// Parallel batch execution (--jobs)
// ---------------------------------------------------------------------------

#[test]
fn default_worker_count_is_available_parallelism_and_reported() {
    let workspace = TempDir::new("jobs-default");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[input.to_str().unwrap(), output.to_str().unwrap()]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected = default_jobs();
    let label = if expected == 1 {
        "1 job".to_string()
    } else {
        format!("{expected} jobs")
    };
    let lines = stdout_lines(&result);
    assert!(
        lines[0].starts_with(&format!(
            "batch: 1 files, format M4A, VBR-6, recursive, {label}"
        )),
        "{lines:?}"
    );
}

#[test]
fn explicit_jobs_one_and_multiworker_outputs_are_byte_identical() {
    // Same plan, sequential and parallel runs: loudness preparation, external
    // artwork scanning, and AAC encoding must all preserve output bytes.
    let workspace = TempDir::new("jobs-identity");
    let input = workspace.join("in");
    seed_file(&input, "album-a/one.flac", "tagged-cover-jpeg-48k-16.flac");
    seed_file(&input, "album-a/two.flac", "tagged-cover-jpeg-48k-16.flac");
    seed_file(&input, "album-b/one.flac", "stereo-48k-16.flac");
    seed_file(&input, "album-b/two.flac", "mono-441k-16.flac");
    seed_file(&input, "root.flac", "stereo-48k-16.flac");

    let sequential = workspace.join("sequential");
    let parallel = workspace.join("parallel");
    for (directory, jobs, label) in [
        (&sequential, "--jobs=1", "1 job"),
        (&parallel, "--jobs=4", "4 jobs"),
    ] {
        let result = run_cli(&[
            jobs,
            "--cover-art",
            "external",
            input.to_str().unwrap(),
            directory.to_str().unwrap(),
        ]);
        assert_eq!(
            result.status.code(),
            Some(0),
            "{jobs}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let lines = stdout_lines(&result);
        assert!(lines[0].contains(label), "{lines:?}");
    }

    for relative in [
        "album-a/one.m4a",
        "album-a/two.m4a",
        "album-b/one.m4a",
        "album-b/two.m4a",
        "root.m4a",
    ] {
        let sequential_bytes = fs::read(sequential.join(relative)).unwrap();
        let parallel_bytes = fs::read(parallel.join(relative)).unwrap();
        assert_eq!(sequential_bytes, parallel_bytes, "{relative}");
        parse_mp4(&parallel_bytes);
    }
    assert_eq!(
        fs::read(sequential.join("album-a/cover.jpg")).unwrap(),
        fs::read(parallel.join("album-a/cover.jpg")).unwrap()
    );
    assert!(!parallel.join("album-b/cover.jpg").exists());
    assert!(!parallel.join("album-b/cover.png").exists());
    assert_no_temp_files_anywhere(&parallel);
}

#[test]
fn concurrent_progress_accounts_for_every_item_exactly_once() {
    let workspace = TempDir::new("jobs-progress");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    seed_file(&input, "b.flac", "three-channel-48k-16.flac");
    seed_file(&input, "c.flac", "mono-441k-16.flac");
    seed_file(&input, "d.flac", "truncated-stereo-48k-16.flac");
    seed_file(&input, "e.flac", "stereo-48k-24.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("e.m4a"), b"retained").unwrap();

    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    let total = plan.entries.len();
    assert_eq!(total, 5);

    let mut events = Vec::new();
    let execution = execute_plan_with_options_and_progress(
        &plan,
        OutputFormat::M4a,
        &BatchOptions {
            skip_existing: true,
            jobs: 4,
            ..BatchOptions::default()
        },
        |event| events.push(event),
    )
    .unwrap();

    assert!(
        matches!(
            events.first(),
            Some(BatchEvent::Preparing {
                total: reported, ..
            }) if *reported == total
        ),
        "{events:?}"
    );

    let mut starts = Vec::new();
    let mut terminals = std::collections::BTreeMap::new();
    for event in &events {
        let (index, kind) = match event {
            BatchEvent::Preparing { .. } => continue,
            BatchEvent::Started(item) => {
                assert_eq!(item.total, total);
                starts.push(item.index);
                continue;
            }
            BatchEvent::Converted { item, .. } => (item.index, "converted"),
            BatchEvent::Skipped(item) => (item.index, "skipped"),
            BatchEvent::Failed { item, .. } => (item.index, "failed"),
            BatchEvent::Planned(item) => (item.index, "planned"),
        };
        assert_eq!(item_total(event), total);
        assert!(
            terminals.insert(index, kind).is_none(),
            "duplicate terminal event for item {index}: {events:?}"
        );
    }
    assert_eq!(terminals.len(), total, "{events:?}");
    assert_eq!(
        terminals.keys().copied().collect::<Vec<_>>(),
        (1..=total).collect::<Vec<_>>()
    );
    assert_eq!(
        starts.len(),
        4,
        "e.flac is a known skip and must not emit a start line"
    );

    for index in &starts {
        let start = events
            .iter()
            .position(|event| matches!(event, BatchEvent::Started(item) if item.index == *index))
            .unwrap();
        let terminal = events
            .iter()
            .position(|event| match event {
                BatchEvent::Converted { item, .. }
                | BatchEvent::Skipped(item)
                | BatchEvent::Planned(item)
                | BatchEvent::Failed { item, .. } => item.index == *index,
                BatchEvent::Preparing { .. } | BatchEvent::Started(_) => false,
            })
            .unwrap();
        assert!(
            start < terminal,
            "item {index} started after its terminal event: {events:?}"
        );
    }

    let summary = execution.outcome.summary;
    assert_eq!(summary.converted, 2, "a.flac and c.flac");
    assert_eq!(summary.failed, 2, "b.flac and d.flac");
    assert_eq!(summary.skipped, 1, "e.m4a is retained");
    assert_eq!(summary.existing, 0);
    assert_eq!(fs::read(output.join("e.m4a")).unwrap(), b"retained");

    // Completion order may differ, but the result report keeps plan order.
    let result_order: Vec<PathBuf> = execution
        .outcome
        .results
        .iter()
        .map(|result| result.relative_input.clone())
        .collect();
    let plan_order: Vec<PathBuf> = plan
        .entries
        .iter()
        .map(|entry| entry.relative_input.clone())
        .collect();
    assert_eq!(result_order, plan_order);
    assert_no_temp_files_anywhere(&output);
}

fn item_total(event: &BatchEvent) -> usize {
    match event {
        BatchEvent::Preparing { total, .. } => *total,
        BatchEvent::Started(item)
        | BatchEvent::Skipped(item)
        | BatchEvent::Planned(item)
        | BatchEvent::Converted { item, .. }
        | BatchEvent::Failed { item, .. } => item.total,
    }
}

#[test]
fn cli_parallel_batch_propagates_per_file_failures() {
    let workspace = TempDir::new("jobs-failure");
    let input = workspace.join("in");
    seed_file(&input, "good.flac", "stereo-48k-16.flac");
    seed_file(&input, "bad.flac", "three-channel-48k-16.flac");
    let output = workspace.join("out");

    let result = run_cli(&[
        "--jobs",
        "4",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    let lines = stdout_lines(&result);
    assert!(lines[0].contains("4 jobs"), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("ok good.flac -> good.m4a ")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "fail bad.flac: UnsupportedChannels"),
        "{lines:?}"
    );
    assert!(
        lines
            .last()
            .unwrap()
            .starts_with("summary: 1 converted, 1 failed, 0 existing, 0 ignored ("),
        "{lines:?}"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("error: bad.flac:"), "{stderr}");
    assert!(output.join("good.m4a").is_file());
    assert!(!output.join("bad.m4a").exists());
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn cli_parallel_adts_batch_converts_all_outputs() {
    let workspace = TempDir::new("jobs-adts");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    seed_file(&input, "b.flac", "mono-441k-16.flac");
    seed_file(&input, "c.flac", "stereo-48k-24.flac");
    let output = workspace.join("out");

    let result = run_cli(&[
        "--format",
        "adts",
        "--jobs",
        "4",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let lines = stdout_lines(&result);
    assert!(lines[0].contains("4 jobs"), "{lines:?}");
    for name in ["a.aac", "b.aac", "c.aac"] {
        let bytes = fs::read(output.join(name)).unwrap();
        assert_eq!(bytes[0], 0xFF, "{name}");
        assert!(!parse_adts(&bytes).is_empty(), "{name}");
    }
    assert_no_temp_files_anywhere(&output);
}

#[test]
fn cli_jobs_rejects_zero_and_invalid_values() {
    let workspace = TempDir::new("jobs-invalid");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    for args in [
        vec!["--jobs", "0"],
        vec!["--jobs=0"],
        vec!["--jobs", "abc"],
        vec!["--jobs", "-1"],
        vec!["--jobs="],
        vec!["--jobs", "1.5"],
    ] {
        let mut command = args.clone();
        command.push(input.to_str().unwrap());
        command.push(output.to_str().unwrap());
        let result = run_cli(&command);
        assert_eq!(result.status.code(), Some(2), "args {args:?}");
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("--jobs"), "args {args:?}: {stderr}");
        assert!(!output.exists(), "args {args:?}");
    }

    // A trailing --jobs has no value at all.
    let result = run_cli(&[input.to_str().unwrap(), output.to_str().unwrap(), "--jobs"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("--jobs requires a positive integer"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    // Batch-only option on a file input.
    let file = fixture("stereo-48k-16.flac");
    let result = run_cli(&[
        "--jobs",
        "2",
        file.to_str().unwrap(),
        output.join("x.m4a").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("--jobs requires a directory input"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!output.exists());
}

#[cfg(unix)]
#[test]
fn read_only_output_subdirectory_fails_only_its_own_files() {
    // Deterministic proxy for temporary-file creation failure: the batch must
    // convert everything else, leave no destination or temp file behind for
    // the failed entry, and report the failure accurately.
    use std::os::unix::fs::PermissionsExt;

    let workspace = TempDir::new("batch-readonly-out");
    let input = workspace.join("in");
    seed_file(&input, "good.flac", "stereo-48k-16.flac");
    seed_file(&input, "sub/locked.flac", "mono-441k-16.flac");
    let output = workspace.join("out");
    let locked_dir = output.join("sub");
    fs::create_dir_all(&locked_dir).unwrap();
    fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let applicable = fs::write(locked_dir.join(".probe"), b"x").is_err();
    fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o755)).unwrap();
    if !applicable {
        eprintln!("permissions are not enforced here; read-only test not applicable");
        return;
    }
    fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let plan = plan_batch(&input, &output, OutputFormat::M4a, true).unwrap();
    assert_eq!(plan.entries.len(), 2);
    let outcome = execute_plan(&plan, OutputFormat::M4a, false, false).unwrap();
    fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(outcome.summary.converted, 1);
    assert_eq!(outcome.summary.failed, 1);
    assert!(
        outcome.results.iter().any(|result| matches!(
            &result.outcome,
            BatchFileOutcome::Failed {
                category: "TempCreate",
                ..
            }
        )),
        "creation failure must be classified, not masked: {:?}",
        outcome.results
    );
    assert!(output.join("good.m4a").is_file());
    assert!(
        !locked_dir.join("locked.m4a").exists(),
        "no partial destination may appear"
    );
    assert_no_temp_files_anywhere(&output);
}
