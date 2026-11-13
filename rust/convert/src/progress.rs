//! Renderer-agnostic conversion and preparation progress events.
//!
//! The batch pipeline reports structured [`ProgressEvent`] values through a
//! caller-supplied observer. The core never performs terminal I/O and never
//! depends on a progress or TUI crate, so the same event stream can drive the
//! CLI renderer, a future desktop GUI (Tauri/React, Slint), or a test harness
//! without duplicating any conversion logic.
//!
//! Events are delivered on the thread that called the batch executor. With
//! `jobs > 1`, preparation and encoding events may arrive in completion order
//! rather than plan order. Absolute counters carried by the events, and the
//! monotonic counters in [`ProgressState`], stay correct regardless of arrival
//! order: the pipeline increments them on the observing thread before
//! forwarding each event.

use std::path::PathBuf;

use crate::TagDiagnostics;

/// Position and paths of one work item in the deterministic batch plan.
///
/// This type is re-exported from [`crate::batch`] for compatibility with the
/// original batch progress API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchProgress {
    /// 1-based position among all work items (planned files plus discovery
    /// failures), in deterministic plan order. Concurrency may change event
    /// arrival order, never this identity.
    pub index: usize,
    /// Total number of work items in this execution.
    pub total: usize,
    /// Input path relative to the input root.
    pub relative_input: PathBuf,
    /// Destination relative to the output root (`None` for discovery
    /// failures).
    pub relative_output: Option<PathBuf>,
}

/// Outcome of one file's loudness analysis, for preparation accounting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LoudnessOutcome {
    /// The file was analyzed and produced a finite integrated loudness value.
    Analyzed,
    /// The file was readable but had no measurable signal (silence or below
    /// the gating threshold). Loudness atoms are omitted with a warning;
    /// conversion still proceeds.
    #[default]
    Unmeasurable,
    /// The source could not be opened or decoded for analysis. Loudness atoms
    /// are omitted with a warning; conversion proceeds and may fail on its
    /// own.
    Failed,
}

/// Where an album's selected artwork came from.
///
/// `Embedded` means the album's embedded pictures were selected for external
/// publication (the `--cover-art external`/`both` fallback path). It does not
/// count per-track embedding in `embedded`/`both` modes without an existing
/// album image, where each track keeps its own picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkSource {
    /// An existing image file in the source album directory was reused.
    ExistingFile,
    /// Artwork was selected from the source FLAC picture metadata for
    /// external publication.
    Embedded,
    /// No usable artwork was found for the album.
    None,
}

/// Structured progress event emitted by the batch pipeline.
///
/// See the [module documentation](self) for delivery and ordering rules.
/// `BatchStarted` is always the first event of an execution. A real M4A run
/// with tags follows it with one `PreparationStarted` event when the
/// preparation pre-pass is relevant, then artwork discovery events, loudness
/// analysis events, and exactly one `PreparationFinished`. Encoding then emits
/// `EncodingStarted`, one `FileStarted` per real attempt, and exactly one
/// terminal file event per work item. Dry runs emit planning-only events and
/// never emit `FileStarted`.
///
/// This enum is [`non_exhaustive`](https://doc.rust-lang.org/reference/attributes/type_system.html)
/// so future pipeline detail (for example live publication events) can be
/// added without breaking downstream `match` statements. Consumers outside
/// this crate must include a wildcard arm.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    /// The deterministic plan is ready.
    BatchStarted {
        /// Planned convertible files (discovery failures excluded).
        files: usize,
        /// Distinct source-parent directories among the planned files.
        albums: usize,
    },
    /// The preparation pre-pass is about to start (real M4A runs with tags).
    PreparationStarted {
        /// Planned convertible files.
        files: usize,
        /// Distinct album directories.
        albums: usize,
        /// Loudness analysis will run for every planned file.
        loudness: bool,
        /// Existing album artwork will be discovered.
        artwork: bool,
        /// External artwork extraction was requested by `--cover-art`.
        external_artwork: bool,
    },
    /// Album artwork discovery started for one album directory.
    ArtworkStarted {
        /// 1-based album position in deterministic order.
        album: usize,
        /// Total albums in this execution.
        albums: usize,
        /// Album directory relative to the input root (empty for the root).
        directory: PathBuf,
    },
    /// Album artwork discovery finished for one album directory.
    ArtworkFinished {
        /// 1-based album position in deterministic order.
        album: usize,
        /// Total albums in this execution.
        albums: usize,
        /// Album directory relative to the input root (empty for the root).
        directory: PathBuf,
        /// How this album's artwork was selected.
        source: ArtworkSource,
        /// Existing file that was reused, when `source` is
        /// [`ArtworkSource::ExistingFile`].
        source_path: Option<PathBuf>,
        /// Albums whose existing external artwork has been reused so far.
        reused_albums: usize,
        /// Albums whose embedded artwork has been selected so far.
        extracted_albums: usize,
        /// Albums whose artwork discovery has completed so far.
        completed_albums: usize,
    },
    /// Loudness analysis finished for one planned file.
    LoudnessFinished {
        /// Files analyzed so far (1-based monotonic count).
        completed: usize,
        /// Planned files in this execution.
        total: usize,
        /// Input path relative to the input root.
        relative_input: PathBuf,
        /// Analysis outcome for this file.
        outcome: LoudnessOutcome,
    },
    /// The preparation pre-pass finished.
    PreparationFinished {
        /// Files ready for encoding.
        prepared: usize,
        /// Files whose preparation analysis failed.
        failed: usize,
        /// Files already known to be skipped by `--skip-existing`.
        skipped: usize,
        /// Planned files whose loudness analysis completed.
        loudness_completed: usize,
        /// Planned files considered for loudness analysis.
        loudness_total: usize,
        /// Albums whose artwork discovery completed.
        artwork_completed: usize,
        /// Albums considered for artwork discovery.
        artwork_total: usize,
        /// Albums whose existing external artwork was reused.
        artwork_reused: usize,
        /// Albums whose embedded artwork was selected for extraction.
        artwork_extracted: usize,
    },
    /// Encoding is about to start (real runs only).
    EncodingStarted {
        /// Work items in this execution (including discovery failures).
        total: usize,
    },
    /// A real conversion attempt for one work item is about to start.
    FileStarted(BatchProgress),
    /// The file converted and published successfully.
    FileConverted {
        /// Position and paths.
        item: BatchProgress,
        /// Metadata diagnostics for the converted file.
        tags: TagDiagnostics,
    },
    /// The destination existed and `--skip-existing` skipped the file.
    FileSkipped(BatchProgress),
    /// The file was not converted.
    FileFailed {
        /// Position and paths.
        item: BatchProgress,
        /// Stable failure category.
        category: &'static str,
        /// User-facing detail for stderr.
        message: String,
    },
    /// Dry run: the destination is free and the file would be converted.
    FilePlanned(BatchProgress),
}

/// Monotonic, renderer-independent counters derived from [`ProgressEvent`]s.
///
/// A CLI, GUI, or test can feed every event into [`apply`](Self::apply) and
/// read the same counters. The struct is intentionally free of timing and
/// formatting concerns: elapsed time and ETA belong to the renderer, because
/// they depend on when observation started, not on the pipeline.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgressState {
    /// Planned convertible files.
    pub files_discovered: usize,
    /// Distinct album directories among the planned files.
    pub albums_total: usize,
    /// Files whose loudness analysis completed.
    pub loudness_completed: usize,
    /// Files considered for loudness analysis.
    pub loudness_total: usize,
    /// Albums whose artwork discovery completed.
    pub artwork_completed: usize,
    /// Albums considered for artwork discovery.
    pub artwork_total: usize,
    /// Albums whose existing external artwork was reused.
    pub artwork_reused: usize,
    /// Albums whose embedded artwork was selected for external publication
    /// (the `--cover-art external`/`both` fallback path). Per-track
    /// embedding in `embedded`/`both` modes without an existing album image
    /// is not counted here.
    pub artwork_extracted: usize,
    /// Files ready for encoding after preparation (authoritative at
    /// `PreparationFinished`; incrementally tracks non-failed loudness
    /// completions before then, so a GUI can show live progress).
    pub prepared: usize,
    /// Files whose preparation analysis failed (live on each
    /// `LoudnessFinished` with a failed outcome; authoritative at
    /// `PreparationFinished`).
    pub preparation_failed: usize,
    /// Files already known to be skipped during preparation (authoritative
    /// at `PreparationFinished` only; skips are reconciled there because
    /// per-file loudness events do not carry skip status).
    pub preparation_skipped: usize,
    /// Work items entering the encoding stage.
    pub encoding_total: usize,
    /// Real conversion attempts started.
    pub encoding_started: usize,
    /// Files converted and published.
    pub converted: usize,
    /// Files skipped during encoding (known skips or publication races).
    pub encoding_skipped: usize,
    /// Files that failed encoding.
    pub encoding_failed: usize,
    /// Dry-run items that would be converted.
    pub planned: usize,
    /// Most recently reported input path, for a status line.
    pub current_file: Option<PathBuf>,
    /// Most recently reported album directory, for a status line.
    pub current_album: Option<PathBuf>,
}

impl ProgressState {
    /// Apply one event to the counters.
    ///
    /// `BatchStarted` resets the state for a new execution so a single
    /// `ProgressState` can be reused across batches without leaking counts
    /// from the previous run.
    pub fn apply(&mut self, event: &ProgressEvent) {
        match event {
            ProgressEvent::BatchStarted { files, albums } => {
                *self = Self {
                    files_discovered: *files,
                    albums_total: *albums,
                    ..Self::default()
                };
            }
            ProgressEvent::PreparationStarted {
                files,
                albums,
                loudness,
                artwork,
                ..
            } => {
                self.files_discovered = *files;
                self.albums_total = *albums;
                if *loudness {
                    self.loudness_total = *files;
                }
                if *artwork {
                    self.artwork_total = *albums;
                }
            }
            ProgressEvent::ArtworkStarted { directory, .. } => {
                self.current_album = Some(directory.clone());
            }
            ProgressEvent::ArtworkFinished {
                directory,
                albums,
                reused_albums,
                extracted_albums,
                completed_albums,
                ..
            } => {
                self.current_album = Some(directory.clone());
                self.artwork_total = *albums;
                self.artwork_completed = *completed_albums;
                self.artwork_reused = *reused_albums;
                self.artwork_extracted = *extracted_albums;
            }
            ProgressEvent::LoudnessFinished {
                completed,
                total,
                relative_input,
                outcome,
            } => {
                self.loudness_completed = self.loudness_completed.max(*completed);
                self.loudness_total = *total;
                self.current_file = Some(relative_input.clone());
                // Incremental live counters; `PreparationFinished` later
                // overwrites them with authoritative totals that also
                // reconcile `--skip-existing` (which per-file loudness
                // events do not carry).
                if *outcome == LoudnessOutcome::Failed {
                    self.preparation_failed += 1;
                } else {
                    self.prepared += 1;
                }
            }
            ProgressEvent::PreparationFinished {
                prepared,
                failed,
                skipped,
                loudness_completed,
                loudness_total,
                artwork_completed,
                artwork_total,
                artwork_reused,
                artwork_extracted,
            } => {
                self.prepared = *prepared;
                self.preparation_failed = *failed;
                self.preparation_skipped = *skipped;
                self.loudness_completed = *loudness_completed;
                self.loudness_total = *loudness_total;
                self.artwork_completed = *artwork_completed;
                self.artwork_total = *artwork_total;
                self.artwork_reused = *artwork_reused;
                self.artwork_extracted = *artwork_extracted;
            }
            ProgressEvent::EncodingStarted { total } => {
                self.encoding_total = *total;
            }
            ProgressEvent::FileStarted(item) => {
                self.encoding_started += 1;
                self.current_file = Some(item.relative_input.clone());
            }
            ProgressEvent::FileConverted { item, .. } => {
                self.converted += 1;
                self.current_file = Some(item.relative_input.clone());
            }
            ProgressEvent::FileSkipped(item) => {
                self.encoding_skipped += 1;
                self.current_file = Some(item.relative_input.clone());
            }
            ProgressEvent::FileFailed { item, .. } => {
                self.encoding_failed += 1;
                self.current_file = Some(item.relative_input.clone());
            }
            ProgressEvent::FilePlanned(item) => {
                self.planned += 1;
                self.current_file = Some(item.relative_input.clone());
            }
        }
    }

    /// Work items with a terminal encoding event so far.
    pub fn encoding_completed(&self) -> usize {
        self.converted + self.encoding_skipped + self.encoding_failed + self.planned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(index: usize, total: usize) -> BatchProgress {
        BatchProgress {
            index,
            total,
            relative_input: PathBuf::from(format!("track-{index}.flac")),
            relative_output: Some(PathBuf::from(format!("track-{index}.m4a"))),
        }
    }

    #[test]
    fn preparation_counts_are_monotonic_and_authoritative() {
        let mut state = ProgressState::default();
        state.apply(&ProgressEvent::BatchStarted {
            files: 3,
            albums: 2,
        });
        state.apply(&ProgressEvent::PreparationStarted {
            files: 3,
            albums: 2,
            loudness: true,
            artwork: true,
            external_artwork: true,
        });
        for completed in 1..=3 {
            state.apply(&ProgressEvent::LoudnessFinished {
                completed,
                total: 3,
                relative_input: PathBuf::from(format!("track-{completed}.flac")),
                outcome: LoudnessOutcome::Analyzed,
            });
        }
        state.apply(&ProgressEvent::ArtworkFinished {
            album: 2,
            albums: 2,
            directory: PathBuf::from("album-b"),
            source: ArtworkSource::ExistingFile,
            source_path: None,
            reused_albums: 1,
            extracted_albums: 1,
            completed_albums: 2,
        });
        state.apply(&ProgressEvent::PreparationFinished {
            prepared: 2,
            failed: 1,
            skipped: 0,
            loudness_completed: 3,
            loudness_total: 3,
            artwork_completed: 2,
            artwork_total: 2,
            artwork_reused: 1,
            artwork_extracted: 1,
        });
        assert_eq!(state.files_discovered, 3);
        assert_eq!(state.albums_total, 2);
        assert_eq!(state.loudness_completed, 3);
        assert_eq!(state.artwork_completed, 2);
        assert_eq!(state.artwork_reused, 1);
        assert_eq!(state.artwork_extracted, 1);
        assert_eq!(state.prepared, 2);
        assert_eq!(state.preparation_failed, 1);
        assert_eq!(state.preparation_skipped, 0);
    }

    #[test]
    fn encoding_terminal_counts_are_exactly_once() {
        let mut state = ProgressState::default();
        state.apply(&ProgressEvent::EncodingStarted { total: 3 });
        state.apply(&ProgressEvent::FileStarted(item(1, 3)));
        state.apply(&ProgressEvent::FileConverted {
            item: item(1, 3),
            tags: TagDiagnostics::default(),
        });
        state.apply(&ProgressEvent::FileSkipped(item(2, 3)));
        state.apply(&ProgressEvent::FileFailed {
            item: item(3, 3),
            category: "DecodeFailed",
            message: "bad".to_string(),
        });
        assert_eq!(state.encoding_started, 1);
        assert_eq!(state.converted, 1);
        assert_eq!(state.encoding_skipped, 1);
        assert_eq!(state.encoding_failed, 1);
        assert_eq!(state.encoding_completed(), 3);
    }
}
