//! Structured progress event tests for the renderer-agnostic pipeline API.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use aac_convert::batch::{execute_plan_with_options_and_events, plan_batch, BatchOptions};
use aac_convert::progress::{ArtworkSource, LoudnessOutcome, ProgressEvent, ProgressState};
use aac_convert::{CoverArtMode, OutputFormat};
use common::{fixture, TempDir};

fn seed_file(root: &Path, relative: &str, fixture_name: &str) -> PathBuf {
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::copy(fixture(fixture_name), &destination).unwrap();
    destination
}

fn collect(
    input: &Path,
    output: &Path,
    options: &BatchOptions,
) -> (aac_convert::batch::BatchExecution, Vec<ProgressEvent>) {
    let plan = plan_batch(input, output, OutputFormat::M4a, true).unwrap();
    let mut events = Vec::new();
    let execution =
        execute_plan_with_options_and_events(&plan, OutputFormat::M4a, options, |event| {
            events.push(event);
        })
        .unwrap();
    (execution, events)
}

fn preparation_finished(events: &[ProgressEvent]) -> &ProgressEvent {
    events
        .iter()
        .find(|event| matches!(event, ProgressEvent::PreparationFinished { .. }))
        .expect("preparation must finish")
}

fn count<F>(events: &[ProgressEvent], predicate: F) -> usize
where
    F: Fn(&ProgressEvent) -> bool,
{
    events.iter().filter(|event| predicate(event)).count()
}

#[test]
fn preparation_and_encoding_events_cover_every_item() {
    let workspace = TempDir::new("progress-complete");
    let input = workspace.join("in");
    seed_file(&input, "album-a/one.flac", "stereo-48k-16.flac");
    seed_file(&input, "album-a/two.flac", "mono-441k-16.flac");
    seed_file(&input, "album-b/one.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let (execution, events) = collect(
        &input,
        &output,
        &BatchOptions {
            jobs: 1,
            ..BatchOptions::default()
        },
    );
    assert_eq!(execution.outcome.summary.converted, 3);

    match events.first() {
        Some(ProgressEvent::BatchStarted { files, albums }) => {
            assert_eq!((*files, *albums), (3, 2));
        }
        other => panic!("expected BatchStarted first: {other:?}"),
    }
    assert!(matches!(
        events.get(1),
        Some(ProgressEvent::PreparationStarted {
            files: 3,
            albums: 2,
            loudness: true,
            artwork: true,
            ..
        })
    ));

    let artwork_starts = count(&events, |event| {
        matches!(event, ProgressEvent::ArtworkStarted { .. })
    });
    let artwork_finishes: Vec<&ProgressEvent> = events
        .iter()
        .filter(|event| matches!(event, ProgressEvent::ArtworkFinished { .. }))
        .collect();
    assert_eq!(artwork_starts, 2);
    assert_eq!(artwork_finishes.len(), 2);
    for (completed, event) in artwork_finishes.iter().enumerate() {
        match event {
            ProgressEvent::ArtworkFinished {
                completed_albums,
                albums,
                ..
            } => {
                assert_eq!(*completed_albums, completed + 1);
                assert_eq!(*albums, 2);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    let loudness: Vec<(usize, PathBuf, LoudnessOutcome)> = events
        .iter()
        .filter_map(|event| match event {
            ProgressEvent::LoudnessFinished {
                completed,
                relative_input,
                outcome,
                ..
            } => Some((*completed, relative_input.clone(), *outcome)),
            _ => None,
        })
        .collect();
    assert_eq!(loudness.len(), 3, "{events:?}");
    assert_eq!(
        loudness
            .iter()
            .map(|(completed, _, _)| *completed)
            .collect::<Vec<_>>(),
        vec![1, 2, 3],
        "counters must be monotonic even when album order differs"
    );
    for relative in ["album-a/one.flac", "album-a/two.flac", "album-b/one.flac"] {
        assert_eq!(
            loudness
                .iter()
                .filter(|(_, path, _)| path == Path::new(relative))
                .count(),
            1,
            "loudness must be reported once per file: {relative}"
        );
    }

    match preparation_finished(&events) {
        ProgressEvent::PreparationFinished {
            prepared,
            failed,
            skipped,
            loudness_completed,
            loudness_total,
            artwork_completed,
            artwork_total,
            ..
        } => {
            assert_eq!((*prepared, *failed, *skipped), (3, 0, 0));
            assert_eq!((*loudness_completed, *loudness_total), (3, 3));
            assert_eq!((*artwork_completed, *artwork_total), (2, 2));
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let finished_position = events
        .iter()
        .position(|event| matches!(event, ProgressEvent::PreparationFinished { .. }))
        .expect("preparation must finish");
    for (position, event) in events.iter().enumerate() {
        if matches!(
            event,
            ProgressEvent::LoudnessFinished { .. } | ProgressEvent::ArtworkFinished { .. }
        ) {
            assert!(
                position < finished_position,
                "preparation detail must precede PreparationFinished: {event:?}"
            );
        }
    }
    assert!(
        events
            .iter()
            .position(|event| matches!(event, ProgressEvent::EncodingStarted { .. }))
            .is_some_and(|position| position > finished_position),
        "encoding must start after preparation finishes: {events:?}"
    );

    assert!(matches!(
        events
            .iter()
            .find(|event| matches!(event, ProgressEvent::EncodingStarted { .. })),
        Some(ProgressEvent::EncodingStarted { total: 3 })
    ));
    assert_eq!(
        count(&events, |event| matches!(
            event,
            ProgressEvent::FileStarted(_)
        )),
        3
    );
    assert_eq!(
        count(&events, |event| {
            matches!(
                event,
                ProgressEvent::FileConverted { .. }
                    | ProgressEvent::FileSkipped(_)
                    | ProgressEvent::FileFailed { .. }
                    | ProgressEvent::FilePlanned(_)
            )
        }),
        3
    );

    let mut state = ProgressState::default();
    for event in &events {
        state.apply(event);
    }
    assert_eq!(state.files_discovered, 3);
    assert_eq!(state.albums_total, 2);
    assert_eq!(state.loudness_completed, 3);
    assert_eq!(state.artwork_completed, 2);
    assert_eq!(state.prepared, 3);
    assert_eq!(state.converted, 3);
    assert_eq!(state.encoding_completed(), 3);
}

#[test]
fn preparation_reports_failures_and_known_skips() {
    let workspace = TempDir::new("progress-failures");
    let input = workspace.join("in");
    seed_file(&input, "good.flac", "stereo-48k-16.flac");
    seed_file(&input, "bad.flac", "truncated-stereo-48k-16.flac");
    seed_file(&input, "skip.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("skip.m4a"), b"retained").unwrap();

    let (execution, events) = collect(
        &input,
        &output,
        &BatchOptions {
            skip_existing: true,
            jobs: 2,
            ..BatchOptions::default()
        },
    );
    assert_eq!(execution.outcome.summary.converted, 1);
    assert_eq!(execution.outcome.summary.failed, 1);
    assert_eq!(execution.outcome.summary.skipped, 1);
    assert!(
        events.iter().any(|event| matches!(
            event,
            ProgressEvent::LoudnessFinished {
                outcome: LoudnessOutcome::Failed,
                ..
            }
        )),
        "the corrupt file must be reported as an analysis failure: {events:?}"
    );
    match preparation_finished(&events) {
        ProgressEvent::PreparationFinished {
            prepared,
            failed,
            skipped,
            loudness_completed,
            ..
        } => {
            assert_eq!(
                (*prepared, *failed, *skipped),
                (1, 1, 1),
                "prepared + failed + skipped must equal the planned files"
            );
            assert_eq!(*loudness_completed, 3);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let mut state = ProgressState::default();
    for event in &events {
        state.apply(event);
    }
    assert_eq!(state.prepared, 1);
    assert_eq!(state.preparation_failed, 1);
    assert_eq!(state.preparation_skipped, 1);
    assert_eq!(state.converted, 1);
    assert_eq!(state.encoding_failed, 1);
    assert_eq!(state.encoding_skipped, 1);
}

#[test]
fn artwork_reuse_and_extraction_are_reported_per_album() {
    let workspace = TempDir::new("progress-artwork");
    let input = workspace.join("in");
    seed_file(&input, "reused/one.flac", "stereo-48k-16.flac");
    seed_file(&input, "reused/two.flac", "mono-441k-16.flac");
    seed_file(
        &input,
        "extracted/one.flac",
        "tagged-cover-jpeg-48k-16.flac",
    );
    fs::write(input.join("reused/cover.jpg"), JPEG).unwrap();
    let output = workspace.join("out");

    let (_execution, events) = collect(
        &input,
        &output,
        &BatchOptions {
            cover_art: CoverArtMode::External,
            jobs: 3,
            ..BatchOptions::default()
        },
    );

    let reusable: Vec<&ProgressEvent> = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ProgressEvent::ArtworkFinished {
                    source: ArtworkSource::ExistingFile,
                    ..
                }
            )
        })
        .collect();
    let extracted: Vec<&ProgressEvent> = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ProgressEvent::ArtworkFinished {
                    source: ArtworkSource::Embedded,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(reusable.len(), 1, "{events:?}");
    assert_eq!(extracted.len(), 1, "{events:?}");
    match preparation_finished(&events) {
        ProgressEvent::PreparationFinished {
            artwork_reused,
            artwork_extracted,
            ..
        } => {
            assert_eq!((*artwork_reused, *artwork_extracted), (1, 1));
        }
        other => panic!("unexpected event: {other:?}"),
    }
    assert_eq!(
        count(&events, |event| matches!(
            event,
            ProgressEvent::ArtworkFinished { .. }
        )),
        2,
        "artwork is resolved once per album, not once per file"
    );
}

#[test]
fn dry_run_emits_planning_events_without_preparation_or_start() {
    let workspace = TempDir::new("progress-dry-run");
    let input = workspace.join("in");
    seed_file(&input, "a.flac", "stereo-48k-16.flac");
    seed_file(&input, "b.flac", "mono-441k-16.flac");
    let output = workspace.join("out");

    let (_execution, events) = collect(
        &input,
        &output,
        &BatchOptions {
            dry_run: true,
            ..BatchOptions::default()
        },
    );
    assert!(matches!(
        events.first(),
        Some(ProgressEvent::BatchStarted {
            files: 2,
            albums: 1
        })
    ));
    assert_eq!(
        count(&events, |event| matches!(
            event,
            ProgressEvent::FilePlanned(_)
        )),
        2
    );
    for event in &events {
        assert!(
            !matches!(
                event,
                ProgressEvent::PreparationStarted { .. }
                    | ProgressEvent::ArtworkStarted { .. }
                    | ProgressEvent::ArtworkFinished { .. }
                    | ProgressEvent::LoudnessFinished { .. }
                    | ProgressEvent::PreparationFinished { .. }
                    | ProgressEvent::EncodingStarted { .. }
                    | ProgressEvent::FileStarted(_)
            ),
            "dry run must not emit work events: {event:?}"
        );
    }
    assert!(!output.exists(), "dry run must create nothing");
}

#[test]
fn single_album_with_multiple_jobs_still_reports_every_file() {
    // A single album forces the single-worker progress path even with
    // multiple configured jobs; loudness events must still arrive once per
    // file with monotonic counters.
    let workspace = TempDir::new("progress-single-album-jobs");
    let input = workspace.join("in");
    seed_file(&input, "album/one.flac", "stereo-48k-16.flac");
    seed_file(&input, "album/two.flac", "mono-441k-16.flac");
    seed_file(&input, "album/three.flac", "stereo-48k-16.flac");
    let output = workspace.join("out");

    let (execution, events) = collect(
        &input,
        &output,
        &BatchOptions {
            jobs: 4,
            ..BatchOptions::default()
        },
    );
    assert_eq!(execution.outcome.summary.converted, 3);
    let loudness = count(&events, |event| {
        matches!(event, ProgressEvent::LoudnessFinished { .. })
    });
    assert_eq!(loudness, 3, "{events:?}");
    let mut state = ProgressState::default();
    for event in &events {
        state.apply(event);
    }
    assert_eq!(state.loudness_completed, 3);
    assert_eq!(state.prepared, 3);
}

#[test]
fn progress_state_resets_cleanly_for_a_new_batch() {
    let mut state = ProgressState::default();
    state.apply(&ProgressEvent::BatchStarted {
        files: 2,
        albums: 1,
    });
    state.apply(&ProgressEvent::LoudnessFinished {
        completed: 2,
        total: 2,
        relative_input: PathBuf::from("a.flac"),
        outcome: LoudnessOutcome::Analyzed,
    });
    state.apply(&ProgressEvent::PreparationFinished {
        prepared: 2,
        failed: 0,
        skipped: 0,
        loudness_completed: 2,
        loudness_total: 2,
        artwork_completed: 1,
        artwork_total: 1,
        artwork_reused: 0,
        artwork_extracted: 0,
    });
    state.apply(&ProgressEvent::EncodingStarted { total: 2 });
    assert_eq!(state.prepared, 2);
    assert_eq!(state.loudness_completed, 2);

    // A new BatchStarted must reset all counters so a reused state cannot
    // leak the previous batch into GUI tallies.
    state.apply(&ProgressEvent::BatchStarted {
        files: 5,
        albums: 3,
    });
    assert_eq!(state.files_discovered, 5);
    assert_eq!(state.albums_total, 3);
    assert_eq!(state.loudness_completed, 0);
    assert_eq!(state.artwork_completed, 0);
    assert_eq!(state.prepared, 0);
    assert_eq!(state.preparation_failed, 0);
    assert_eq!(state.preparation_skipped, 0);
    assert_eq!(state.encoding_total, 0);
    assert_eq!(state.converted, 0);
    assert_eq!(state.encoding_completed(), 0);
    assert_eq!(state.current_file, None);
    assert_eq!(state.current_album, None);
}

const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, b'D', b'I', b'S', b'K', 0x00];
