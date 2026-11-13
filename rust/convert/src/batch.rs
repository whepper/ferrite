//! Directory batch conversion (M8.2), with bounded-concurrency execution.
//!
//! This module layers directory semantics on top of the unchanged single-file
//! pipeline ([`convert_file_with_format`](crate::convert_file_with_format)).
//! It performs discovery, deterministic planning, pre-flight validation, and
//! bounded-concurrency execution with per-file failure isolation.
//!
//! Design authority: `docs/aac-convert-m8.1-batch-design.md` for the
//! sequential planning and publication rules; the parallel execution rules
//! are an additive extension documented here.
//!
//! Concurrency is stage-bounded, never nested. Preparation resolves album
//! artwork once per source directory and then runs the loudness analysis; each
//! sub-stage uses at most [`BatchOptions::jobs`] workers and finishes before the
//! next begins. Only then does conversion run, again with at most the same
//! number of workers. The stages never overlap, so the global worker limit is
//! never exceeded even though every stage reuses it. Per-file results are
//! always collected in plan order; only progress events may arrive in
//! completion order.
//!
//! - Traversal is recursive by default; directory symlinks are never followed;
//!   symlinked files whose target is a regular file are accepted; hidden files
//!   and directories are traversed without special-casing.
//! - Only regular files (or symlinks to regular files) whose file name has a
//!   `.flac` extension under ASCII case-insensitive comparison are eligible.
//!   Everything else is tallied as ignored, never a failure.
//! - The plan (and all reporting) is sorted by the byte representation of the
//!   input-relative path, so runs are deterministic across platforms.
//! - Output mapping preserves the relative directory structure and the source
//!   stem byte-for-byte and replaces the extension with lowercase `.m4a`
//!   (M4A) or `.aac` (ADTS). Batch mode never emits `.mp4` or `.adts`.
//! - Duplicate destinations, including ASCII case-fold collisions, abort the
//!   run before anything is converted.
//! - Existing destinations are never overwritten; by default they count as
//!   failures for the exit-code contract and are tallied separately as
//!   `existing`. With `skip_existing`, pre-existing destinations are skipped
//!   instead: never converted, never a failure, tallied as `skipped`. The
//!   conversion pass does not decode a skipped source (though the M4A
//!   loudness pre-pass and album-artwork discovery may still read it;
//!   see [`execute_plan_with_options_and_progress`]). Publication-time
//!   collisions (a destination appearing after planning) are still enforced
//!   by the atomic hard link and surface as a skip under `skip_existing`, as
//!   a failure otherwise.
//! - Each file converts through the existing temporary-file and atomic
//!   hard-link publication path, so no partial output can remain and a
//!   concurrent winner is never replaced.
//! - Independent files are prepared and encoded by a bounded worker pool of
//!   [`BatchOptions::jobs`] threads (default: available logical CPUs; `1`
//!   restores strictly sequential execution). Every worker owns its decoder,
//!   encoder, metadata state, and temporary file, so no mutable encoder state
//!   is shared. Memory remains bounded per worker by the same streaming
//!   profile as single-file conversion. Artwork adds at most one image (up
//!   to the 16 MiB cover limit) per in-flight worker for embedding, plus
//!   small per-album metadata; no per-album image bytes are retained in RAM.
//!   Embedded fallback covers are spilled to a temp staging directory (one
//!   file per album, on disk, removed on completion) and reloaded on demand.
//! - M4A runs with tags add one streaming loudness pre-pass over source PCM
//!   (`ebur128`, one packet at a time, no whole-file buffering) before any
//!   publication. Albums are independent and may run concurrently; members of
//!   one album stay sequential because they share one ordered album analyzer.
//! - External-artwork discovery looks in the source album directory first
//!   (`cover`/`folder`/`front`/`album` with `.jpg`, `.jpeg`, `.png`, or
//!   `.webp`, case-insensitive, in that priority order). An existing image is
//!   reused for embedding and/or external publication; embedded pictures are
//!   inspected only when no suitable album file exists. Resolution happens
//!   once per album; the selected embedded cover is spilled to a temp staging
//!   directory (cleaned up on completion) and reloaded on demand for
//!   publication rather than retained in RAM. Embedded conflict detection
//!   remains for albums without an existing image, and conflict/publication
//!   problems never fail audio. Files are opened and re-validated as regular
//!   files with bounded reads, so a concurrently replaced path cannot cause
//!   an unbounded allocation (a residual FIFO-swap blocking risk on open is
//!   accepted and documented at the discovery site).
//! - Traversal itself uses only `std::fs` with an explicit stack (no
//!   recursion).
//! - [`execute_plan_with_options_and_progress`] exposes the legacy
//!   [`BatchEvent`] callback, while
//!   [`execute_plan_with_options_and_events`] exposes renderer-agnostic
//!   [`ProgressEvent`] values (plan counts,
//!   per-album artwork discovery, per-file loudness completion, preparation
//!   tallies, and encoding outcomes). Events are emitted on the calling
//!   thread; with `jobs > 1` their order follows completion, while returned
//!   results and stdout reports remain in plan order. External cover-art
//!   planning groups eligible artwork by destination directory, reports and
//!   skips conflicting images for that directory, and publishes one identical
//!   image without overwriting an existing `cover.jpg`/`cover.png`/
//!   `cover.webp`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::artwork::{self, ArtworkFormat};
use crate::loudness;
use crate::parallel;
use crate::progress::{ArtworkSource, LoudnessOutcome, ProgressEvent};
use crate::{
    convert_file_with_format_tags_and_loudness, AudioProperties, ConvertError, CoverArtMode,
    OutputFormat, ResolvedCover, TagDiagnostics,
};
use ferrite_aac::aac_enc::config::VbrMode;

pub use crate::progress::BatchProgress;

/// Reason recorded for an ignored directory symlink (verbose reporting).
pub const SYMLINKED_DIR_REASON: &str = "symlinked dir, not followed";

/// Reason recorded for an entry whose file name is not `.flac`
/// (case-insensitive).
pub const UNSUPPORTED_EXTENSION_REASON: &str = "unsupported extension";

/// Reason recorded for an entry that is not a regular file or a symlink to
/// one (FIFO, socket, device, ...).
pub const NON_REGULAR_FILE_REASON: &str = "not a regular file";

/// Reason recorded for an eligible FLAC below the top level when
/// `--no-recursive` is active.
pub const NON_RECURSIVE_REASON: &str = "excluded by --no-recursive";

/// Canonical batch output extension for a format.
///
/// Batch mode emits exactly one extension per format: lowercase `.m4a` for
/// M4A and `.aac` for ADTS. It never emits `.mp4` or `.adts`; single-file
/// extension inference is untouched.
pub fn batch_extension(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::M4a => "m4a",
        OutputFormat::Adts => "aac",
    }
}

/// Stable per-file failure category for a [`ConvertError`].
///
/// The category reuses the variant name so user-facing output stays stable
/// and greppable without exposing unstable internals.
pub fn category_for(error: &ConvertError) -> &'static str {
    match error {
        ConvertError::InputOpen(..) => "InputOpen",
        ConvertError::InputNotFile(..) => "InputNotFile",
        ConvertError::InputNotFlac(..) => "InputNotFlac",
        ConvertError::MissingAudioTrack => "MissingAudioTrack",
        ConvertError::MissingAudioProperties => "MissingAudioProperties",
        ConvertError::UnsupportedSampleRate(..) => "UnsupportedSampleRate",
        ConvertError::UnsupportedChannels(..) => "UnsupportedChannels",
        ConvertError::UnsupportedBitDepth(..) => "UnsupportedBitDepth",
        ConvertError::DecodeFailed => "DecodeFailed",
        ConvertError::SamePath(..) => "SamePath",
        ConvertError::OutputExists(..) => "OutputExists",
        ConvertError::OutputDirectoryMissing(..) => "OutputDirectoryMissing",
        ConvertError::OutputInspect(..) => "OutputInspect",
        ConvertError::TempCreate(..) => "TempCreate",
        ConvertError::TempWrite(..) => "TempWrite",
        ConvertError::EncoderConfig(..) => "EncoderConfig",
        ConvertError::Encoder(..) => "Encoder",
        ConvertError::AdtsInvalid(..) => "AdtsInvalid",
        ConvertError::AudioSpecificConfig(..) => "AudioSpecificConfig",
        ConvertError::Muxer(..) => "Muxer",
        ConvertError::GaplessSignalling(..) => "GaplessSignalling",
        ConvertError::Metadata(..) => "Metadata",
        ConvertError::Finalize(..) => "Finalize",
        ConvertError::Publish(..) => "Publish",
    }
}

/// Fatal batch setup error: nothing was converted.
#[derive(Debug)]
pub enum BatchError {
    /// The input path does not exist (including a broken input symlink).
    MissingInput(PathBuf),
    /// The input exists but is neither a regular file nor a directory.
    InputNotFileOrDir(PathBuf),
    /// The input root directory could not be listed.
    InputUnreadable(PathBuf, io::Error),
    /// The input directory yielded no convertible input.
    NoEligibleFiles {
        /// Canonical input root.
        root: PathBuf,
        /// True when the directory had no entries at all.
        empty: bool,
    },
    /// Two planned inputs map to the same destination.
    DuplicateDestination {
        /// First input, relative to the input root.
        first: PathBuf,
        /// Second input, relative to the input root.
        second: PathBuf,
        /// Conflicting destination, relative to the output root.
        destination: PathBuf,
    },
    /// The output root could not be created.
    OutputRootCreate(PathBuf, io::Error),
    /// Canonical input and output roots are the same directory.
    SameRoot(PathBuf),
}

impl fmt::Display for BatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInput(path) => {
                write!(f, "input path does not exist: {}", path.display())
            }
            Self::InputNotFileOrDir(path) => write!(
                f,
                "input is neither a regular file nor a directory: {}",
                path.display()
            ),
            Self::InputUnreadable(path, _) => {
                write!(f, "could not read input directory: {}", path.display())
            }
            Self::NoEligibleFiles { root, empty: true } => {
                write!(f, "input directory is empty: {}", root.display())
            }
            Self::NoEligibleFiles { root, empty: false } => write!(
                f,
                "no FLAC files found in input directory: {}",
                root.display()
            ),
            Self::DuplicateDestination {
                first,
                second,
                destination,
            } => write!(
                f,
                "duplicate output destination: {} (from {} and {}); \
                 refusing to convert anything",
                destination.display(),
                first.display(),
                second.display()
            ),
            Self::OutputRootCreate(path, _) => {
                write!(f, "could not create output directory: {}", path.display())
            }
            Self::SameRoot(path) => write!(
                f,
                "input and output directories are the same: {}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for BatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InputUnreadable(_, err) | Self::OutputRootCreate(_, err) => Some(err),
            _ => None,
        }
    }
}

/// One planned conversion: absolute input with its mapped destination.
#[derive(Debug, Clone)]
pub struct BatchPlanEntry {
    /// Absolute canonical input file.
    pub input: PathBuf,
    /// Input path relative to the canonical input root.
    pub relative_input: PathBuf,
    /// Destination path relative to the output root.
    pub relative_output: PathBuf,
    /// Absolute destination (output root joined with `relative_output`).
    pub output: PathBuf,
}

/// A skipped tree entry that is tallied, never a failure.
#[derive(Debug, Clone)]
pub struct IgnoredEntry {
    /// Path relative to the canonical input root.
    pub relative: PathBuf,
    /// Verbose-only reason (e.g. [`SYMLINKED_DIR_REASON`]), if any.
    pub reason: Option<&'static str>,
}

/// A discovery-time problem reported as a per-file failure without a
/// conversion attempt (e.g. an unreadable subdirectory).
#[derive(Debug, Clone)]
pub struct DiscoveryFailure {
    /// Path relative to the canonical input root.
    pub relative: PathBuf,
    /// Stable failure category (`UnreadableEntry`, `InputOpen`, ...).
    pub category: &'static str,
    /// User-facing detail for stderr.
    pub message: String,
}

/// Deterministic, sorted batch plan. Planning performs no writes: it reads
/// the input tree and validates mapping only.
#[derive(Debug, Clone)]
pub struct BatchPlan {
    /// Canonical input root.
    pub input_root: PathBuf,
    /// Absolute output root (lexical; may not exist yet for dry runs).
    pub output_root: PathBuf,
    /// Planned conversions in byte-sorted relative-path order.
    pub entries: Vec<BatchPlanEntry>,
    /// Skipped entries in byte-sorted relative-path order.
    pub ignored: Vec<IgnoredEntry>,
    /// Discovery failures in byte-sorted relative-path order.
    pub discovery_failures: Vec<DiscoveryFailure>,
}

/// Outcome of one work item.
#[derive(Debug, Clone)]
pub enum BatchFileOutcome {
    /// Converted and published.
    Converted {
        /// AAC frames written.
        frames: u64,
        /// Output bytes written.
        bytes: u64,
        /// Input audio properties.
        properties: AudioProperties,
        /// User-metadata diagnostics for the converted file.
        tags: TagDiagnostics,
    },
    /// Dry run: the destination is free and the file would be converted.
    Planned,
    /// Skipped under `skip_existing`: the destination already existed when
    /// checked, so the conversion pass does not decode the source and
    /// nothing is written. Never a conversion, never a failure.
    ///
    /// The M4A loudness pre-pass and album-artwork discovery may still
    /// read a skipped source; see
    /// [`execute_plan_with_options_and_progress`].
    Skipped,
    /// The file was not converted.
    Failed {
        /// Stable category (see [`category_for`], `UnreadableEntry`,
        /// `OutputDirCreate`).
        category: &'static str,
        /// User-facing detail for stderr.
        message: String,
    },
}

/// Ordered per-item batch result.
#[derive(Debug, Clone)]
pub struct BatchFileResult {
    /// Input path relative to the input root.
    pub relative_input: PathBuf,
    /// Destination relative to the output root (`None` for discovery failures).
    pub relative_output: Option<PathBuf>,
    /// What happened.
    pub outcome: BatchFileOutcome,
}

impl BatchFileResult {
    /// True when this item counts as `existing` (a no-overwrite refusal).
    pub fn is_existing(&self) -> bool {
        matches!(
            &self.outcome,
            BatchFileOutcome::Failed {
                category: "OutputExists",
                ..
            }
        )
    }

    /// True when this item was skipped under `skip_existing`.
    pub fn is_skipped(&self) -> bool {
        matches!(&self.outcome, BatchFileOutcome::Skipped)
    }
}

/// Final batch tallies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchSummary {
    /// Files converted (real run) or plannable (dry run).
    pub converted: u64,
    /// Files not converted, including `existing` refusals. Skips are never
    /// counted here.
    pub failed: u64,
    /// Subset of `failed`: destinations that already existed (only when
    /// `skip_existing` is off; with it on, such files count as `skipped`).
    pub existing: u64,
    /// Skipped non-eligible entries.
    pub ignored: u64,
    /// Files skipped under `skip_existing` because the destination already
    /// existed. Never a conversion, never a failure.
    pub skipped: u64,
}

/// Ordered results plus tallies.
#[derive(Debug, Clone)]
pub struct BatchOutcome {
    /// One item per work unit, in deterministic input order.
    pub results: Vec<BatchFileResult>,
    /// Final tallies.
    pub summary: BatchSummary,
}

/// Default batch worker count: the number of available logical CPUs.
///
/// Falls back to one worker when the platform cannot report a parallelism
/// value, so batch execution always has a usable limit.
pub fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, |count| count.get())
}

/// Options for one batch execution.
///
/// The field defaults preserve the historical behavior except for `jobs`,
/// which defaults to [`default_jobs`] (available logical CPUs): no dry run,
/// no skip-existing, tags enabled, embedded cover art, and VBR-6 (the Ferrite
/// default profile).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchOptions {
    /// Plan only: create nothing, decode nothing, read no metadata.
    pub dry_run: bool,
    /// Treat existing destinations as skips instead of failures.
    pub skip_existing: bool,
    /// Write user metadata atoms for M4A output (`--no-tags` sets `false`).
    pub write_tags: bool,
    /// Cover-art handling for M4A output with tags enabled.
    pub cover_art: CoverArtMode,
    /// Global worker limit for preparation and encoding. `1` is strictly
    /// sequential. Values below one are treated as one.
    pub jobs: usize,
    /// VBR profile for encoding (VBR-1 through VBR-6; default VBR-6).
    pub vbr_mode: VbrMode,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            skip_existing: false,
            write_tags: true,
            cover_art: CoverArtMode::Embedded,
            jobs: default_jobs(),
            vbr_mode: VbrMode::Vbr6,
        }
    }
}

/// Legacy progress event emitted synchronously during batch execution.
///
/// This is the historical surface retained for compatibility. New callers that
/// want preparation detail (per-album artwork discovery, per-file loudness
/// completion, reused/extracted tallies) should use
/// [`ProgressEvent`] through
/// [`execute_plan_with_options_and_events`].
///
/// Every work item emits exactly one terminal event (`Converted`, `Skipped`,
/// `Failed`, or `Planned`). Real conversion attempts also emit `Started`
/// before their terminal event; an item already known to be skipped under
/// `skip_existing` emits only `Skipped`. M4A runs with tags emit one optional
/// `Preparing` event before the first item when the loudness (and artwork)
/// pre-pass runs. Dry runs never emit `Started` or `Preparing`: nothing is
/// encoded or analyzed.
///
/// Events are emitted on the calling thread, so a single observer sees no
/// interleaved or reordered writes from workers. With `jobs == 1` events are
/// in plan order. With `jobs > 1` they arrive in completion order (which may
/// differ from plan order), while `BatchProgress::index` and `total` always
/// identify the item by its deterministic plan position. Each item's
/// `Started` event still precedes its own terminal event. Events never
/// replace or duplicate conversion logic; they are observations of the same
/// per-file pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchEvent {
    /// Optional real-run phase event: the album loudness (and, when external
    /// artwork is requested, cover-art) pre-pass is about to start. No
    /// per-file work has begun yet.
    Preparing {
        /// Total work items in this execution.
        total: usize,
        /// True when the pre-pass also scans artwork for external extraction.
        cover_art: bool,
    },
    /// A real-run item is about to be checked and converted.
    Started(BatchProgress),
    /// The file converted and published successfully.
    Converted {
        /// Position and paths.
        item: BatchProgress,
        /// Metadata diagnostics for the converted file.
        tags: TagDiagnostics,
    },
    /// The destination existed and `skip_existing` skipped the file.
    Skipped(BatchProgress),
    /// The file was not converted.
    Failed {
        /// Position and paths.
        item: BatchProgress,
        /// Stable failure category.
        category: &'static str,
        /// User-facing detail for stderr.
        message: String,
    },
    /// Dry run: the destination is free and the file would be converted.
    Planned(BatchProgress),
}

/// Outcome of one batch execution, including external cover-art reports.
#[derive(Debug, Clone)]
pub struct BatchExecution {
    /// Per-file results and tallies.
    pub outcome: BatchOutcome,
    /// External cover-art outcomes in output-directory order. Empty when
    /// nothing is extracted (embedded-only, ADTS, `--no-tags`, dry runs, or no
    /// eligible artwork).
    pub cover_art: Vec<crate::CoverArtReport>,
}

/// Discover, map, and validate a directory batch without writing anything.
///
/// Returns a sorted plan or a fatal [`BatchError`]. The output root is
/// absolutized lexically but never created here; creation happens in
/// [`execute_plan`] for real runs, so dry runs create nothing.
pub fn plan_batch(
    input: &Path,
    output_root: &Path,
    format: OutputFormat,
    recursive: bool,
) -> Result<BatchPlan, BatchError> {
    let input_root = input
        .canonicalize()
        .map_err(|_| BatchError::MissingInput(input.to_path_buf()))?;
    if !input_root.is_dir() {
        return Err(BatchError::InputNotFileOrDir(input.to_path_buf()));
    }
    let output_abs = absolutize(output_root);

    let (candidates, ignored, discovery_failures) = discover(&input_root, recursive)
        .map_err(|(root, err)| BatchError::InputUnreadable(root, err))?;
    if candidates.is_empty() && discovery_failures.is_empty() {
        return Err(BatchError::NoEligibleFiles {
            root: input_root,
            empty: is_empty_dir(&candidates, &ignored),
        });
    }

    // Best-effort identity check on the lexical path. The authoritative
    // canonical check runs in `execute_plan` after the root is created.
    if output_abs == input_root {
        return Err(BatchError::SameRoot(input_root));
    }
    if let Ok(canonical_output) = output_abs.canonicalize() {
        if canonical_output == input_root {
            return Err(BatchError::SameRoot(input_root));
        }
    }

    let mut entries = Vec::with_capacity(candidates.len());
    for relative_input in &candidates {
        let relative_output = map_relative_output(relative_input, format);
        let output = output_abs.join(&relative_output);
        debug_assert!(
            output.starts_with(&output_abs),
            "mapped destination escapes the output root"
        );
        entries.push(BatchPlanEntry {
            input: input_root.join(relative_input),
            relative_input: relative_input.clone(),
            relative_output,
            output,
        });
    }
    // Entries arrive sorted from discovery, but re-sort defensively: order is
    // the user-visible contract.
    entries.sort_by_key(|a| sort_key(&a.relative_input));

    find_duplicate(&entries)?;

    Ok(BatchPlan {
        input_root,
        output_root: output_abs,
        entries,
        ignored,
        discovery_failures,
    })
}

/// Execute a plan with per-file failure isolation.
///
/// - Real runs create the output root (fatal [`BatchError`] on failure),
///   then prepare and convert entries through
///   [`convert_file_with_format_and_tags`](crate::convert_file_with_format_and_tags)
///   with user metadata writing enabled and at most [`default_jobs`] workers.
/// - Dry runs create nothing and decode nothing (and never read tags); they
///   report `Planned` for free destinations and `Failed` (`OutputExists`)
///   for taken ones. Dry runs do not start worker threads.
/// - Discovery failures are reported as failures in both modes.
/// - With `skip_existing`, a destination that already exists is skipped: the
///   conversion pass never decodes the source, no temporary file is created,
///   and the item counts as `skipped`, never as converted or failed. (The
///   M4A loudness pre-pass and album-artwork discovery may still read
///   a skipped source.) The pre-execution existence check is advisory only;
///   enforcement stays with the atomic hard-link publication, so a
///   destination appearing after planning still cannot be overwritten: a
///   publication-time `OutputExists` becomes a skip under `skip_existing`
///   and a failure otherwise. Every other error is a failure in both modes.
///
/// Results are returned in deterministic input order with final tallies,
/// regardless of worker completion order.
///
/// This wrapper writes user metadata for M4A output. Use
/// [`execute_plan_with_tags`] to omit user metadata atoms (`--no-tags`), and
/// [`execute_plan_with_options_and_progress`] for external cover-art control,
/// an explicit worker limit, or progress events.
pub fn execute_plan(
    plan: &BatchPlan,
    format: OutputFormat,
    dry_run: bool,
    skip_existing: bool,
) -> Result<BatchOutcome, BatchError> {
    execute_plan_with_tags(plan, format, dry_run, skip_existing, true)
}

/// Execute a plan with explicit user-metadata control.
///
/// Behaves like [`execute_plan`], except `write_tags` selects whether M4A
/// output carries user metadata atoms (`true`) or gapless signalling only
/// (`false`, the `--no-tags` CLI behavior). The flag is a no-op for ADTS
/// output, which never carries tags, and for dry runs, which never read tags.
///
/// M4A runs with tags first run the loudness pre-pass: every planned source
/// (converted or retained under `skip_existing`) is analyzed through the
/// album-grouped R128 pipeline, and each conversion carries only its own
/// precomputed loudness atoms. The pre-pass reads sources only — no outputs
/// are published until the conversion pass, so single-publication and
/// no-replace behavior are unchanged. Cover art remains in the default
/// embedded mode; use [`execute_plan_with_options`] for external extraction.
pub fn execute_plan_with_tags(
    plan: &BatchPlan,
    format: OutputFormat,
    dry_run: bool,
    skip_existing: bool,
    write_tags: bool,
) -> Result<BatchOutcome, BatchError> {
    execute_plan_with_options(
        plan,
        format,
        &BatchOptions {
            dry_run,
            skip_existing,
            write_tags,
            cover_art: CoverArtMode::Embedded,
            jobs: default_jobs(),
            vbr_mode: VbrMode::Vbr6,
        },
    )
    .map(|execution| execution.outcome)
}

/// Execute a plan with explicit batch options and no progress observer.
///
/// Behaves like [`execute_plan_with_tags`], with additive control over
/// external cover art, the worker limit, and access to
/// [`CoverArtReport`](crate::CoverArtReport) records. Output, metadata, and
/// result ordering semantics are identical regardless of the worker limit.
pub fn execute_plan_with_options(
    plan: &BatchPlan,
    format: OutputFormat,
    options: &BatchOptions,
) -> Result<BatchExecution, BatchError> {
    execute_plan_with_options_and_progress(plan, format, options, |_| {})
}

/// Execute a plan with explicit options and a synchronous progress observer.
///
/// The observer runs only on the calling thread, so events never interleave
/// with worker output. M4A runs with tags first emit one `Preparing` event for
/// the loudness and artwork discovery pre-pass. Every work item then
/// emits exactly one terminal [`BatchEvent`] (`Converted`, `Skipped`,
/// `Failed`, or `Planned`); real conversion attempts also emit `Started`
/// first, while items already known to be skipped under `skip_existing` emit
/// only `Skipped`. Dry runs emit neither `Preparing` nor `Started`.
/// Conversion logic is not duplicated: events observe the same single-file
/// pipeline.
///
/// With `options.jobs > 1`, preparation and conversion use bounded worker
/// pools of that size, and events arrive in completion order. `index` and
/// `total` still identify each item by plan position, and each item's
/// `Started` event precedes its terminal event. With `jobs == 1` (and for dry
/// runs, which never start workers) events remain in plan order.
///
/// External cover-art publication happens after the conversion stage and is
/// always sequential. Artwork conflicts and publication problems produce
/// reports and never fail or roll back otherwise valid audio conversions.
///
/// The observer must not panic: it executes synchronously on the calling
/// thread, and a panic unwinds through the executor, aborting the remaining
/// batch. Files already published before the panic are not rolled back.
pub fn execute_plan_with_options_and_progress<F>(
    plan: &BatchPlan,
    format: OutputFormat,
    options: &BatchOptions,
    mut progress: F,
) -> Result<BatchExecution, BatchError>
where
    F: FnMut(BatchEvent),
{
    let preparing = matches!(format, OutputFormat::M4a) && !options.dry_run && options.write_tags;
    let work_total = plan.entries.len() + plan.discovery_failures.len();
    execute_plan_with_options_and_events(plan, format, options, |event| {
        if preparing && matches!(event, ProgressEvent::BatchStarted { .. }) {
            progress(BatchEvent::Preparing {
                total: work_total,
                cover_art: options.cover_art.extracts(),
            });
        }
        if let Some(event) = legacy_event(event) {
            progress(event);
        }
    })
}

/// Execute a plan with explicit options and a structured progress observer.
///
/// This is the renderer-agnostic entry point behind
/// [`execute_plan_with_options_and_progress`]. It emits [`ProgressEvent`]
/// values for planning, preparation (album artwork discovery, loudness
/// analysis, reuse/extraction tallies), and encoding. The event stream carries
/// no terminal formatting, timing, or GUI assumptions; see [`crate::progress`].
///
/// Events are delivered on the calling thread. With `options.jobs > 1`,
/// preparation and encoding events arrive in completion order while each
/// event's absolute counters and plan-position identity stay deterministic.
/// Dry runs emit `BatchStarted`, terminal `FilePlanned` events, and no
/// preparation or start events.
///
/// Artwork is resolved once per album directory before encoding: an existing
/// `cover.*`/`folder.*`/`front.*`/`album.*` image is reused when present, and
/// embedded artwork is selected only as a fallback. Conflicting embedded
/// images still veto external publication for their directory, matching the
/// existing cover-art contract. Publication happens after conversion and never
/// fails or rolls back otherwise valid audio conversions.
///
/// Memory policy: no per-album image bytes are retained in RAM. Existing-file
/// albums retain only paths and formats; embedded fallback covers are spilled
/// to a temp staging directory inside the output root (one file per album,
/// up to the 16 MiB cover limit each, on disk) and reloaded on demand. Peak
/// artwork RAM therefore scales with `--jobs` (at most one image per
/// in-flight worker for embedding, plus one album at a time for sequential
/// publication), not with the number of albums. The staging directory is
/// removed when execution completes (including panic unwinds while the guard
/// is alive); abrupt termination can leave an orphan
/// `.aac-convert-artwork-*.tmp.d` directory, which is never a valid output,
/// never blocks reruns, and is safe to delete manually once no conversion is
/// running.
///
/// The observer must not panic: it executes synchronously on the calling
/// thread, and a panic unwinds through the executor, aborting the remaining
/// batch. Files already published before the panic are not rolled back.
pub fn execute_plan_with_options_and_events<F>(
    plan: &BatchPlan,
    format: OutputFormat,
    options: &BatchOptions,
    mut progress: F,
) -> Result<BatchExecution, BatchError>
where
    F: FnMut(ProgressEvent),
{
    let output_root = if options.dry_run {
        plan.output_root.clone()
    } else {
        fs::create_dir_all(&plan.output_root)
            .map_err(|err| BatchError::OutputRootCreate(plan.output_root.clone(), err))?;
        let canonical = plan
            .output_root
            .canonicalize()
            .map_err(|err| BatchError::OutputRootCreate(plan.output_root.clone(), err))?;
        if canonical == plan.input_root {
            return Err(BatchError::SameRoot(plan.input_root.clone()));
        }
        canonical
    };

    // Merge planned entries and discovery failures into one deterministic
    // order keyed by relative input path.
    let mut work: Vec<(Vec<u8>, WorkItem<'_>)> = Vec::new();
    for entry in &plan.entries {
        work.push((sort_key(&entry.relative_input), WorkItem::Entry(entry)));
    }
    for failure in &plan.discovery_failures {
        work.push((sort_key(&failure.relative), WorkItem::Discovery(failure)));
    }
    work.sort_by(|a, b| a.0.cmp(&b.0));
    let total = work.len();

    // Dry runs are metadata-only and deterministic; they neither start
    // workers nor analyze anything.
    let jobs = if options.dry_run { 1 } else { options.jobs };

    let albums = album_groups(plan);
    progress(ProgressEvent::BatchStarted {
        files: plan.entries.len(),
        albums: albums.len(),
    });

    // Preparation is M4A-only and real-run-only. Embedded artwork is relevant
    // even without external extraction because an existing album image is
    // reused for embedding as well.
    let preparation = matches!(format, OutputFormat::M4a) && !options.dry_run && options.write_tags;
    let embed_cover = options.write_tags && options.cover_art.embeds();
    let known_skip: HashMap<PathBuf, bool> = plan
        .entries
        .iter()
        .map(|entry| {
            (
                entry.relative_input.clone(),
                is_entry_known_skip(entry, &output_root, options),
            )
        })
        .collect();

    let mut prepared = 0usize;
    let mut preparation_failed = 0usize;
    let mut preparation_skipped = 0usize;
    let mut loudness_completed = 0usize;
    let mut loudness_total = 0usize;

    // Artwork spill staging lives for the whole execution so spill files
    // survive from preparation through publication, then are removed. The
    // staging directory holds at most one spilled cover per album with
    // embedded fallback (on disk, not in RAM); existing-file albums need no
    // spill. If creation fails, embedded fallback is skipped but conversion
    // still proceeds.
    let staging = if preparation && options.cover_art.extracts() {
        ArtworkStaging::new(&output_root)
    } else {
        None
    };

    let artwork = if preparation {
        progress(ProgressEvent::PreparationStarted {
            files: plan.entries.len(),
            albums: albums.len(),
            loudness: true,
            artwork: true,
            external_artwork: options.cover_art.extracts(),
        });
        Some(prepare_album_artwork(
            plan,
            &albums,
            options,
            jobs,
            staging.as_ref(),
            &mut progress,
        ))
    } else {
        None
    };

    // Loudness pre-pass: analyze every planned source grouped into albums by
    // source-parent directory before any conversion publishes. Retained members
    // participate through their readable sources; analysis never modifies
    // retained outputs. Albums are independent workers; members of one album
    // remain sequential. Per-file events may arrive in completion order.
    let loudness = if preparation {
        let members = plan
            .entries
            .iter()
            .map(|entry| (entry.relative_input.clone(), entry.input.clone()))
            .collect::<Vec<_>>();
        loudness_total = members.len();
        let map =
            loudness::analyze_album_with_jobs_and_progress(&members, jobs, |relative, outcome| {
                loudness_completed += 1;
                let skipped = known_skip.get(relative).copied().unwrap_or(false);
                if skipped {
                    preparation_skipped += 1;
                } else if outcome == LoudnessOutcome::Failed {
                    preparation_failed += 1;
                } else {
                    prepared += 1;
                }
                progress(ProgressEvent::LoudnessFinished {
                    completed: loudness_completed,
                    total: loudness_total,
                    relative_input: relative.to_path_buf(),
                    outcome,
                });
            });
        Some(map)
    } else {
        None
    };

    if preparation {
        let totals = artwork
            .as_ref()
            .map(|(_, totals)| *totals)
            .unwrap_or_default();
        progress(ProgressEvent::PreparationFinished {
            prepared,
            failed: preparation_failed,
            skipped: preparation_skipped,
            loudness_completed,
            loudness_total,
            artwork_completed: totals.completed,
            artwork_total: albums.len(),
            artwork_reused: totals.reused,
            artwork_extracted: totals.extracted,
        });
    }

    if !options.dry_run {
        progress(ProgressEvent::EncodingStarted { total });
    }

    // Workers only borrow immutable plan/options/loudness/artwork state. Each
    // task owns its decoder, encoder, and temporary file, and results are
    // slotted back by plan index so completion order cannot reorder the report.
    let loudness = loudness.as_ref();
    let artwork_map = artwork.as_ref().map(|(map, _)| map);
    let results = parallel::map_indexed_with_events(
        work.len(),
        jobs,
        |index| !options.dry_run && !is_known_skip(&work[index].1, &output_root, options),
        |index| {
            execute_work_item(
                &work[index].1,
                &output_root,
                format,
                options,
                embed_cover,
                loudness,
                artwork_map,
            )
        },
        |event| match event {
            parallel::IndexEvent::Started(index) => {
                progress(ProgressEvent::FileStarted(
                    work[index].1.progress(index + 1, total),
                ));
            }
            parallel::IndexEvent::Finished(index, result) => {
                emit_terminal_progress(result, index + 1, total, &mut progress);
            }
        },
    );

    let summary = summarize(&results, plan.ignored.len() as u64);
    let cover_art = if matches!(format, OutputFormat::M4a)
        && !options.dry_run
        && options.write_tags
        && options.cover_art.extracts()
    {
        match artwork_map {
            Some(map) => publish_external_covers(&output_root, map, &results),
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };

    Ok(BatchExecution {
        outcome: BatchOutcome { results, summary },
        cover_art,
    })
}

/// Translate a rich progress event into the historical [`BatchEvent`] shape.
///
/// Preparation detail (per-album artwork and per-file loudness) has no legacy
/// equivalent and is intentionally dropped here; callers that need it use
/// [`execute_plan_with_options_and_events`].
fn legacy_event(event: ProgressEvent) -> Option<BatchEvent> {
    match event {
        ProgressEvent::FileStarted(item) => Some(BatchEvent::Started(item)),
        ProgressEvent::FileConverted { item, tags } => Some(BatchEvent::Converted { item, tags }),
        ProgressEvent::FileSkipped(item) => Some(BatchEvent::Skipped(item)),
        ProgressEvent::FileFailed {
            item,
            category,
            message,
        } => Some(BatchEvent::Failed {
            item,
            category,
            message,
        }),
        ProgressEvent::FilePlanned(item) => Some(BatchEvent::Planned(item)),
        ProgressEvent::BatchStarted { .. }
        | ProgressEvent::PreparationStarted { .. }
        | ProgressEvent::ArtworkStarted { .. }
        | ProgressEvent::ArtworkFinished { .. }
        | ProgressEvent::LoudnessFinished { .. }
        | ProgressEvent::PreparationFinished { .. }
        | ProgressEvent::EncodingStarted { .. } => None,
    }
}

/// One unit of execution in deterministic plan order.
enum WorkItem<'a> {
    Entry(&'a BatchPlanEntry),
    Discovery(&'a DiscoveryFailure),
}

impl WorkItem<'_> {
    /// Position and paths for progress reporting.
    fn progress(&self, index: usize, total: usize) -> BatchProgress {
        match self {
            Self::Entry(entry) => BatchProgress {
                index,
                total,
                relative_input: entry.relative_input.clone(),
                relative_output: Some(entry.relative_output.clone()),
            },
            Self::Discovery(failure) => BatchProgress {
                index,
                total,
                relative_input: failure.relative.clone(),
                relative_output: None,
            },
        }
    }
}

/// True when a planned destination already exists under `skip_existing`.
///
/// The check is advisory: atomic publication remains the enforcement point
/// for destinations that appear after planning. Kept separate so the caller
/// can suppress a misleading "encoding..." start line for known skips.
fn is_known_skip(item: &WorkItem<'_>, output_root: &Path, options: &BatchOptions) -> bool {
    match item {
        WorkItem::Entry(entry) => is_entry_known_skip(entry, output_root, options),
        WorkItem::Discovery(_) => false,
    }
}

/// Entry-specific form of [`is_known_skip`], shared with preparation
/// accounting so the preparation and encoding phases agree on skip status.
fn is_entry_known_skip(entry: &BatchPlanEntry, output_root: &Path, options: &BatchOptions) -> bool {
    !options.dry_run
        && options.skip_existing
        && fs::symlink_metadata(output_root.join(&entry.relative_output)).is_ok()
}

/// Execute one work item through the unchanged per-file pipeline.
fn execute_work_item(
    item: &WorkItem<'_>,
    output_root: &Path,
    format: OutputFormat,
    options: &BatchOptions,
    embed_cover: bool,
    loudness: Option<&HashMap<PathBuf, loudness::FileLoudness>>,
    artwork: Option<&BTreeMap<PathBuf, AlbumArtwork>>,
) -> BatchFileResult {
    match item {
        WorkItem::Discovery(failure) => BatchFileResult {
            relative_input: failure.relative.clone(),
            relative_output: None,
            outcome: BatchFileOutcome::Failed {
                category: failure.category,
                message: failure.message.clone(),
            },
        },
        WorkItem::Entry(entry) => {
            if is_known_skip(item, output_root, options) {
                BatchFileResult {
                    relative_input: entry.relative_input.clone(),
                    relative_output: Some(entry.relative_output.clone()),
                    outcome: BatchFileOutcome::Skipped,
                }
            } else {
                let resolved = artwork
                    .and_then(|map| {
                        let album_dir = entry
                            .relative_input
                            .parent()
                            .unwrap_or_else(|| Path::new(""));
                        map.get(album_dir)
                    })
                    .and_then(|album| {
                        resolved_cover_for(album, &entry.relative_input, options, embed_cover)
                    });
                execute_entry(
                    entry,
                    output_root,
                    format,
                    options,
                    embed_cover,
                    resolved,
                    loudness.and_then(|map| map.get(&entry.relative_input)),
                )
            }
        }
    }
}

/// Build the per-member resolved cover for one conversion.
///
/// Rules preserve both diagnostics and the historical embedding contract:
/// - Existing-file albums always suppress per-file embedded selection (the
///   external file is authoritative); embedding loads that file on demand.
/// - External-only fallback albums suppress re-reads and reuse the stored
///   per-member warnings (even when empty), so unsupported-cover diagnostics
///   survive without attaching one track's warnings to another file. Members
///   whose probe failed have no stored entry and fall back to per-file
///   selection, preserving error semantics.
/// - `Both`/`Embedded` without an existing file keep per-file selection so
///   each track embeds its own picture.
fn resolved_cover_for(
    album: &AlbumArtwork,
    member: &Path,
    options: &BatchOptions,
    embed_cover: bool,
) -> Option<ResolvedCover> {
    match album.source {
        ArtworkSource::ExistingFile => Some(ResolvedCover {
            cover: if embed_cover {
                album.load_existing_cover()
            } else {
                None
            },
            warnings: Vec::new(),
        }),
        ArtworkSource::Embedded => {
            if options.cover_art != CoverArtMode::External {
                return None;
            }
            album
                .member_warnings
                .get(member)
                .map(|warnings| ResolvedCover {
                    cover: None,
                    warnings: warnings.clone(),
                })
        }
        ArtworkSource::None => {
            if options.cover_art != CoverArtMode::External {
                return None;
            }
            match album.member_warnings.get(member) {
                Some(warnings) if !warnings.is_empty() => Some(ResolvedCover {
                    cover: None,
                    warnings: warnings.clone(),
                }),
                // No stored diagnostics: fall back to per-file selection so
                // warnings are still produced without an extra retained copy.
                _ => None,
            }
        }
    }
}

/// Execute one planned entry through the existing per-file pipeline.
fn execute_entry(
    entry: &BatchPlanEntry,
    output_root: &Path,
    format: OutputFormat,
    options: &BatchOptions,
    embed_cover: bool,
    resolved_cover: Option<ResolvedCover>,
    loudness: Option<&loudness::FileLoudness>,
) -> BatchFileResult {
    let destination = if options.dry_run {
        entry.output.clone()
    } else {
        let destination = output_root.join(&entry.relative_output);
        debug_assert!(
            destination.starts_with(output_root),
            "mapped destination escapes the output root"
        );
        destination
    };
    let relative_output = || Some(entry.relative_output.clone());

    if options.dry_run {
        return match fs::symlink_metadata(&destination) {
            Ok(_) => {
                if options.skip_existing {
                    BatchFileResult {
                        relative_input: entry.relative_input.clone(),
                        relative_output: relative_output(),
                        outcome: BatchFileOutcome::Skipped,
                    }
                } else {
                    BatchFileResult {
                        relative_input: entry.relative_input.clone(),
                        relative_output: relative_output(),
                        outcome: BatchFileOutcome::Failed {
                            category: "OutputExists",
                            message: format!(
                                "output file already exists: {} \
                                 (overwriting is not supported)",
                                entry.relative_output.display()
                            ),
                        },
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => BatchFileResult {
                relative_input: entry.relative_input.clone(),
                relative_output: relative_output(),
                outcome: BatchFileOutcome::Planned,
            },
            Err(_) => BatchFileResult {
                relative_input: entry.relative_input.clone(),
                relative_output: relative_output(),
                outcome: BatchFileOutcome::Failed {
                    category: "OutputInspect",
                    message: format!(
                        "could not inspect output path: {}",
                        entry.relative_output.display()
                    ),
                },
            },
        };
    }

    // With `skip_existing`, a destination known to exist is skipped before
    // any conversion-pass decode, encode, or temp-file work. This check is
    // advisory: the atomic hard-link publication remains the enforcement
    // point, so a destination appearing afterwards still cannot be
    // overwritten.
    if options.skip_existing && fs::symlink_metadata(&destination).is_ok() {
        return BatchFileResult {
            relative_input: entry.relative_input.clone(),
            relative_output: relative_output(),
            outcome: BatchFileOutcome::Skipped,
        };
    }
    if let Some(parent) = destination.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            // A concurrent batch may have created the directory first; only
            // real failures count.
            if err.kind() != io::ErrorKind::AlreadyExists {
                return BatchFileResult {
                    relative_input: entry.relative_input.clone(),
                    relative_output: relative_output(),
                    outcome: BatchFileOutcome::Failed {
                        category: "OutputDirCreate",
                        message: format!(
                            "could not create output subdirectory for {}",
                            entry.relative_input.display()
                        ),
                    },
                };
            }
        }
    }
    match convert_file_with_format_tags_and_loudness(
        &entry.input,
        &destination,
        format,
        options.write_tags,
        embed_cover,
        resolved_cover,
        loudness,
        options.vbr_mode,
    ) {
        Ok((summary, tags)) => BatchFileResult {
            relative_input: entry.relative_input.clone(),
            relative_output: relative_output(),
            outcome: BatchFileOutcome::Converted {
                frames: summary.encoded_frames,
                bytes: summary.output_bytes,
                properties: summary.properties,
                tags,
            },
        },
        Err(err) => {
            let category = category_for(&err);
            // A publication-time collision means the destination appeared
            // after planning (a concurrent batch or an external write). The
            // hard link already refused the overwrite; under `skip_existing`
            // that outcome is a skip, never a failure. Any other publication
            // error stays a failure: races are never inferred from generic
            // I/O errors.
            if category == "OutputExists" && options.skip_existing {
                BatchFileResult {
                    relative_input: entry.relative_input.clone(),
                    relative_output: relative_output(),
                    outcome: BatchFileOutcome::Skipped,
                }
            } else {
                BatchFileResult {
                    relative_input: entry.relative_input.clone(),
                    relative_output: relative_output(),
                    outcome: BatchFileOutcome::Failed {
                        category,
                        message: err.to_string(),
                    },
                }
            }
        }
    }
}

/// Emit exactly one terminal progress event for a finished work item.
fn emit_terminal_progress<F>(result: &BatchFileResult, index: usize, total: usize, progress: &mut F)
where
    F: FnMut(ProgressEvent),
{
    let item = BatchProgress {
        index,
        total,
        relative_input: result.relative_input.clone(),
        relative_output: result.relative_output.clone(),
    };
    match &result.outcome {
        BatchFileOutcome::Converted { tags, .. } => progress(ProgressEvent::FileConverted {
            item,
            tags: tags.clone(),
        }),
        BatchFileOutcome::Planned => progress(ProgressEvent::FilePlanned(item)),
        BatchFileOutcome::Skipped => progress(ProgressEvent::FileSkipped(item)),
        BatchFileOutcome::Failed { category, message } => progress(ProgressEvent::FileFailed {
            item,
            category,
            message: message.clone(),
        }),
    }
}

/// Derive the unchanged summary counts from the ordered results.
fn summarize(results: &[BatchFileResult], ignored: u64) -> BatchSummary {
    let mut summary = BatchSummary {
        converted: 0,
        failed: 0,
        existing: 0,
        ignored,
        skipped: 0,
    };
    for result in results {
        match &result.outcome {
            BatchFileOutcome::Converted { .. } | BatchFileOutcome::Planned => {
                summary.converted += 1;
            }
            BatchFileOutcome::Skipped => summary.skipped += 1,
            BatchFileOutcome::Failed { category, .. } => {
                summary.failed += 1;
                if *category == "OutputExists" {
                    summary.existing += 1;
                }
            }
        }
    }
    summary
}

/// Content fingerprint for embedded cover-art conflict detection.
///
/// Hashing the format and complete image bytes compares actual content, not
/// names or metadata descriptions. Length is included explicitly and the
/// hash covers each byte, so distinct images are not conflated by filename.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoverFingerprint {
    format: crate::meta::CoverFormat,
    len: usize,
    hash: u64,
}

fn cover_fingerprint(cover: &crate::meta::CoverArt) -> CoverFingerprint {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cover.format.hash(&mut hasher);
    cover.data.hash(&mut hasher);
    CoverFingerprint {
        format: cover.format,
        len: cover.data.len(),
        hash: hasher.finish(),
    }
}

/// Aggregate artwork discovery counters for progress reporting.
///
/// `extracted` counts albums whose embedded artwork was selected for
/// external publication (the `--cover-art external`/`both` fallback path).
/// It does not count per-track embedding in `embedded`/`both` modes without
/// an existing album image, where each track keeps its own picture through
/// the historical per-file selection.
#[derive(Debug, Clone, Copy, Default)]
struct ArtworkTotals {
    completed: usize,
    reused: usize,
    extracted: usize,
}

/// Temporary staging for spilled embedded artwork bytes.
///
/// One directory per batch execution holds at most one spilled cover file
/// per album that needs embedded fallback publication. Image bytes live on
/// disk, not in RAM: the in-memory map retains only paths, formats, and
/// small warning strings. The directory is removed on drop, so spill files
/// are cleaned up on success, failure, and panic paths while the guard is
/// alive.
struct ArtworkStaging {
    dir: PathBuf,
}

impl ArtworkStaging {
    /// Create a uniquely named staging directory inside `parent`.
    ///
    /// Returns `None` when creation fails; callers then skip the embedded
    /// spill (existing-file reuse still works) so artwork temp issues never
    /// fail audio conversion.
    fn new(parent: &Path) -> Option<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        for _ in 0..64 {
            let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = parent.join(format!(
                ".aac-convert-artwork-{}-{sequence}.tmp.d",
                std::process::id()
            ));
            match fs::create_dir(&dir) {
                Ok(()) => return Some(Self { dir }),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(_) => return None,
            }
        }
        None
    }

    /// Spill one album's selected cover bytes; returns the spill path.
    ///
    /// Failures are returned to the caller, which degrades to no external
    /// artwork for that album rather than failing the batch.
    fn spill(&self, album_index: usize, bytes: &[u8]) -> io::Result<PathBuf> {
        let path = self.dir.join(format!("album-{album_index}.cover"));
        fs::write(&path, bytes)?;
        Ok(path)
    }
}

impl Drop for ArtworkStaging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Artwork resolved once for one source album directory.
///
/// Memory policy: no image bytes are retained for any album. Existing-file
/// albums retain only the source path and format; embedded-fallback albums
/// retain only a spill path (temp file), the format, conflict status, and
/// small per-member warning strings. Conversions and publication load the
/// bytes they need on demand (one album at a time for publication, one file
/// at a time per worker for embedding), so peak artwork RAM scales with
/// `--jobs`, not with the number of albums.
#[derive(Debug, Clone)]
struct AlbumArtwork {
    /// Where the selected artwork came from.
    source: ArtworkSource,
    /// Existing file that was reused, when applicable.
    source_path: Option<PathBuf>,
    /// Format of the existing file, when applicable.
    existing_format: Option<ArtworkFormat>,
    /// Format of the spilled embedded cover, when applicable.
    embedded_format: Option<crate::meta::CoverFormat>,
    /// Temp file holding the selected embedded cover bytes for publication.
    spill_path: Option<PathBuf>,
    /// Per-member warnings from the album-level embedded scan, keyed by
    /// input-relative path. Only members whose `read_cover` succeeded have
    /// entries; members whose probe failed have none, so conversion falls
    /// back to its own per-file selection and preserves error semantics.
    member_warnings: HashMap<PathBuf, Vec<String>>,
    /// True when embedded images in this album disagree.
    conflicted: bool,
}

impl AlbumArtwork {
    /// Load the existing-file cover for embedding, when one applies.
    ///
    /// Reads the source file on demand instead of retaining its bytes, so
    /// peak RAM stays bounded. Returns `None` when there is no embeddable
    /// existing cover or the file can no longer be read (TOCTOU); callers
    /// treat that as no cover, never as a conversion failure.
    fn load_existing_cover(&self) -> Option<crate::meta::CoverArt> {
        let path = self.source_path.as_ref()?;
        let format = self.existing_format?;
        let cover_format = match format {
            ArtworkFormat::Jpeg => crate::meta::CoverFormat::Jpeg,
            ArtworkFormat::Png => crate::meta::CoverFormat::Png,
            ArtworkFormat::Webp => return None,
        };
        // Bounded read: the file was validated during discovery, but it may
        // have grown since; reject over-limit reads instead of allocating.
        let data = read_bounded_cover(path)?;
        if data.is_empty() || data.len() > crate::meta::COVER_MAX_BYTES {
            return None;
        }
        // Re-validate magic bytes: the file may have been replaced since
        // discovery. A format change means the discovery result is stale;
        // treat it as no cover rather than embedding unexpected bytes.
        let sniffed = match (data[0], data.get(1).copied().unwrap_or(0)) {
            (0xFF, 0xD8) => ArtworkFormat::Jpeg,
            _ if data.len() >= 8
                && data[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] =>
            {
                ArtworkFormat::Png
            }
            _ => return None,
        };
        if sniffed != format {
            return None;
        }
        Some(crate::meta::CoverArt {
            format: cover_format,
            data,
        })
    }

    /// External file name and bytes to publish, loading on demand.
    ///
    /// Returns owned bytes (one album at a time; publication is sequential).
    /// Returns `None` when there is nothing to publish or the backing file
    /// can no longer be read; callers skip publication without failing audio.
    fn external_file(&self) -> Option<(String, Vec<u8>)> {
        match self.source {
            ArtworkSource::ExistingFile => {
                let path = self.source_path.as_ref()?;
                let format = self.existing_format?;
                let data = read_bounded_cover(path)?;
                if data.is_empty() || data.len() > crate::meta::COVER_MAX_BYTES {
                    return None;
                }
                Some((format.external_file_name().to_string(), data))
            }
            ArtworkSource::Embedded => {
                let spill = self.spill_path.as_ref()?;
                let format = self.embedded_format?;
                let data = fs::read(spill).ok()?;
                if data.is_empty() || data.len() > crate::meta::COVER_MAX_BYTES {
                    return None;
                }
                let file_name = match format {
                    crate::meta::CoverFormat::Jpeg => "cover.jpg".to_string(),
                    crate::meta::CoverFormat::Png => "cover.png".to_string(),
                };
                Some((file_name, data))
            }
            ArtworkSource::None => None,
        }
    }
}

/// Bounded cover read shared with [`artwork`] discovery.
///
/// Reads at most [`COVER_MAX_BYTES`](crate::meta::COVER_MAX_BYTES) + 1 bytes
/// so a concurrently grown file cannot cause an unbounded allocation.
fn read_bounded_cover(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read as _;

    let file = fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bounded = file.take(crate::meta::COVER_MAX_BYTES as u64 + 1);
    let mut data = Vec::new();
    bounded.read_to_end(&mut data).ok()?;
    Some(data)
}

/// Group planned entries by source album directory, in deterministic byte
/// order.
fn album_groups(plan: &BatchPlan) -> Vec<(PathBuf, Vec<usize>)> {
    let mut groups: BTreeMap<PathBuf, Vec<usize>> = BTreeMap::new();
    for (index, entry) in plan.entries.iter().enumerate() {
        let directory = entry
            .relative_input
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        groups.entry(directory).or_default().push(index);
    }
    groups.into_iter().collect()
}

/// Resolve artwork once for every album directory.
///
/// Album resolutions run on at most `jobs` workers. The returned map and
/// counters are in album order regardless of completion order. Existing
/// external artwork wins; embedded artwork is inspected only when no suitable
/// external image exists and external extraction was requested.
///
/// Within one (potentially large) album, embedded member reads stay
/// sequential. This is intentional: members already stay sequential for the
/// loudness aggregation, and parallelism is preserved across albums, where
/// the batch scales. No per-member work is duplicated across stages beyond
/// the documented conflict-detection scan.
///
/// Memory policy: no image bytes are retained. Embedded fallback covers are
/// spilled to `staging` (when available) and reloaded on demand for
/// publication; existing-file covers are reloaded from their source files.
/// If `staging` is `None` (temp creation failed), embedded fallback is
/// skipped but existing-file reuse still works, so temp issues never fail
/// audio conversion.
fn prepare_album_artwork<F>(
    plan: &BatchPlan,
    albums: &[(PathBuf, Vec<usize>)],
    options: &BatchOptions,
    jobs: usize,
    staging: Option<&ArtworkStaging>,
    progress: &mut F,
) -> (BTreeMap<PathBuf, AlbumArtwork>, ArtworkTotals)
where
    F: FnMut(ProgressEvent),
{
    let mut totals = ArtworkTotals::default();
    let resolved = parallel::map_indexed_with_events(
        albums.len(),
        jobs,
        |_| true,
        |index| resolve_album_artwork(plan, &albums[index].1, options, staging, index),
        |event| match event {
            parallel::IndexEvent::Started(index) => {
                progress(ProgressEvent::ArtworkStarted {
                    album: index + 1,
                    albums: albums.len(),
                    directory: albums[index].0.clone(),
                });
            }
            parallel::IndexEvent::Finished(index, artwork) => {
                totals.completed += 1;
                match artwork.source {
                    ArtworkSource::ExistingFile => totals.reused += 1,
                    ArtworkSource::Embedded => totals.extracted += 1,
                    ArtworkSource::None => {}
                }
                progress(ProgressEvent::ArtworkFinished {
                    album: index + 1,
                    albums: albums.len(),
                    directory: albums[index].0.clone(),
                    source: artwork.source,
                    source_path: artwork.source_path.clone(),
                    reused_albums: totals.reused,
                    extracted_albums: totals.extracted,
                    completed_albums: totals.completed,
                });
            }
        },
    );
    let map = albums
        .iter()
        .map(|(directory, _)| directory.clone())
        .zip(resolved)
        .collect();
    (map, totals)
}

/// Resolve artwork for one album directory.
///
/// The existing-file search reads the directory once and is the only source of
/// truth for reuse. When no suitable external image exists and external
/// extraction is requested, each member's embedded artwork is read once to
/// detect conflicts, collect per-member diagnostics, and select the first
/// cover for publication; that selected cover is spilled to `staging` and
/// never retained in RAM, so extraction happens at most once per album.
fn resolve_album_artwork(
    plan: &BatchPlan,
    indices: &[usize],
    options: &BatchOptions,
    staging: Option<&ArtworkStaging>,
    album_index: usize,
) -> AlbumArtwork {
    resolve_album_artwork_with(
        plan,
        indices,
        options,
        staging,
        album_index,
        crate::meta::read_cover,
    )
}

fn resolve_album_artwork_with<F>(
    plan: &BatchPlan,
    indices: &[usize],
    options: &BatchOptions,
    staging: Option<&ArtworkStaging>,
    album_index: usize,
    read_cover: F,
) -> AlbumArtwork
where
    F: Fn(&Path) -> Result<(Option<crate::meta::CoverArt>, Vec<String>), ConvertError>,
{
    let relative_dir = plan.entries[indices[0]]
        .relative_input
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    let directory = plan.input_root.join(&relative_dir);
    let mut album = AlbumArtwork {
        source: ArtworkSource::None,
        source_path: None,
        existing_format: None,
        embedded_format: None,
        spill_path: None,
        member_warnings: HashMap::new(),
        conflicted: false,
    };

    if let Some(existing) = artwork::discover(&directory) {
        match existing.format {
            ArtworkFormat::Jpeg | ArtworkFormat::Png => {
                album.source = ArtworkSource::ExistingFile;
                album.source_path = Some(existing.path);
                album.existing_format = Some(existing.format);
                return album;
            }
            ArtworkFormat::Webp => {
                // WebP cannot be embedded. With external extraction requested,
                // reuse it as `cover.webp`; otherwise fall back to the normal
                // per-file embedded selection.
                if options.cover_art.extracts() {
                    album.source = ArtworkSource::ExistingFile;
                    album.source_path = Some(existing.path);
                    album.existing_format = Some(existing.format);
                    return album;
                }
            }
        }
    }

    if !options.cover_art.extracts() {
        return album;
    }

    let mut fingerprint: Option<CoverFingerprint> = None;
    let mut selected: Option<crate::meta::CoverArt> = None;
    for &index in indices {
        let entry = &plan.entries[index];
        let Ok((cover, warnings)) = read_cover(&entry.input) else {
            // Probe failures have no entry: conversion falls back to its own
            // per-file selection and preserves error semantics instead of
            // silently reusing another member's warnings.
            continue;
        };
        // Store every successful member's warnings (even when no cover was
        // selected) so conversion can reuse them without re-reading and
        // without attaching one track's warnings to another file.
        album
            .member_warnings
            .insert(entry.relative_input.clone(), warnings);
        if let Some(cover) = cover {
            if selected.is_none() {
                selected = Some(cover.clone());
            }
            let current = cover_fingerprint(&cover);
            match fingerprint {
                None => fingerprint = Some(current),
                Some(existing) if existing != current => album.conflicted = true,
                _ => {}
            }
        }
    }
    if let Some(cover) = selected {
        // Spill the publication bytes to disk instead of retaining them.
        // If staging or the write fails, keep the per-member warnings for
        // diagnostics but report no artwork for publication.
        if let Some(staging) = staging {
            match staging.spill(album_index, &cover.data) {
                Ok(spill_path) => {
                    album.source = ArtworkSource::Embedded;
                    album.embedded_format = Some(cover.format);
                    album.spill_path = Some(spill_path);
                }
                Err(_) => {
                    album.source = ArtworkSource::None;
                }
            }
        } else {
            album.source = ArtworkSource::None;
        }
    }
    album
}

/// Publish one external image per non-conflicting output directory.
///
/// Only directories with at least one converted or retained-skipped audio
/// item receive a file. Conflicts are reported and skipped; one directory's
/// artwork problem never changes audio outcomes or the final summary. Cover
/// bytes are loaded on demand from the source album file or the preparation
/// spill file (one album at a time; publication itself is sequential), so no
/// image bytes are retained in RAM across the batch.
fn publish_external_covers(
    output_root: &Path,
    artwork: &BTreeMap<PathBuf, AlbumArtwork>,
    results: &[BatchFileResult],
) -> Vec<crate::CoverArtReport> {
    let produced: BTreeSet<PathBuf> = results
        .iter()
        .filter(|result| {
            matches!(
                &result.outcome,
                BatchFileOutcome::Converted { .. } | BatchFileOutcome::Skipped
            )
        })
        .filter_map(|result| result.relative_output.as_ref())
        .map(|relative| {
            relative
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .to_path_buf()
        })
        .collect();

    let mut reports = Vec::new();
    for (relative_dir, album) in artwork {
        let display_directory = if relative_dir.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            relative_dir.clone()
        };
        if album.conflicted {
            reports.push(crate::CoverArtReport {
                directory: display_directory,
                file_name: None,
                action: crate::CoverArtAction::Conflicted,
            });
            continue;
        }
        if !produced.contains(relative_dir) {
            continue;
        }
        let Some((file_name, bytes)) = album.external_file() else {
            continue;
        };
        let destination = output_root.join(relative_dir).join(&file_name);
        let action = crate::publish_cover_file(&destination, &bytes);
        reports.push(crate::CoverArtReport {
            directory: display_directory,
            file_name: Some(file_name),
            action,
        });
    }
    reports
}

/// Map an input-relative path to its output-relative destination.
///
/// Preserves parent directories and the file stem byte-for-byte (including
/// case) and replaces the final extension with the canonical lowercase
/// batch extension for `format`.
pub(crate) fn map_relative_output(relative_input: &Path, format: OutputFormat) -> PathBuf {
    let extension = batch_extension(format);
    let file_name = relative_input
        .file_name()
        .expect("planned inputs always have a file name");
    let stem = Path::new(file_name)
        .file_stem()
        .expect("a .flac file name always has a stem");
    let mut name = OsString::from(stem);
    name.push(".");
    name.push(extension);
    match relative_input.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
        _ => PathBuf::from(name),
    }
}

/// ASCII case-insensitive `.flac` file-name check (byte-based, so non-UTF-8
/// names work).
pub(crate) fn is_flac_name(file_name: &OsStr) -> bool {
    Path::new(file_name)
        .extension()
        .is_some_and(|extension| extension.as_encoded_bytes().eq_ignore_ascii_case(b"flac"))
}

/// Byte sort key for deterministic, locale-independent ordering.
fn sort_key(path: &Path) -> Vec<u8> {
    path.as_os_str().as_encoded_bytes().to_vec()
}

/// ASCII-lowercased fold for cross-platform duplicate detection.
fn ascii_fold(path: &Path) -> Vec<u8> {
    path.as_os_str()
        .as_encoded_bytes()
        .iter()
        .map(|byte| byte.to_ascii_lowercase())
        .collect()
}

/// Join a possibly-relative path onto the current directory without touching
/// the filesystem (unlike `canonicalize`, works for not-yet-created roots).
fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

/// The no-eligible-files case is an "empty directory" only when discovery saw
/// nothing at all (not even ignored entries or unreadable ones, which cannot
/// occur here because discovery failures are non-empty-checked by callers).
fn is_empty_dir(candidates: &[PathBuf], ignored: &[IgnoredEntry]) -> bool {
    candidates.is_empty() && ignored.is_empty()
}

/// Fail closed when two entries map to the same destination, comparing both
/// exactly and under ASCII case-folding (protects case-insensitive
/// filesystems without platform sniffing).
fn find_duplicate(entries: &[BatchPlanEntry]) -> Result<(), BatchError> {
    use std::collections::HashMap;

    let mut exact_keys: HashMap<Vec<u8>, &BatchPlanEntry> = HashMap::new();
    let mut folded: HashMap<Vec<u8>, &BatchPlanEntry> = HashMap::new();
    for entry in entries {
        let key = sort_key(&entry.relative_output);
        if let Some(prior) = exact_keys.get(&key) {
            return Err(BatchError::DuplicateDestination {
                first: prior.relative_input.clone(),
                second: entry.relative_input.clone(),
                destination: entry.relative_output.clone(),
            });
        }
        exact_keys.insert(key, entry);
        let fold_key = ascii_fold(&entry.relative_output);
        if let Some(prior) = folded.get(&fold_key) {
            // An exact duplicate is reported above; anything reaching here
            // differs textually but folds together.
            if prior.relative_output != entry.relative_output {
                return Err(BatchError::DuplicateDestination {
                    first: prior.relative_input.clone(),
                    second: entry.relative_input.clone(),
                    destination: entry.relative_output.clone(),
                });
            }
        } else {
            folded.insert(fold_key, entry);
        }
    }
    Ok(())
}

/// Walk the canonical input root with an explicit stack (no recursion, so
/// deep trees cannot overflow the call stack) and never descend into
/// directory symlinks.
///
/// Returns `(candidates, ignored, discovery_failures)` with paths relative
/// to `input_root`. Only failure to list the root itself is an error; every
/// deeper problem becomes a [`DiscoveryFailure`] so siblings still convert.
#[allow(clippy::type_complexity)]
fn discover(
    input_root: &Path,
    recursive: bool,
) -> Result<(Vec<PathBuf>, Vec<IgnoredEntry>, Vec<DiscoveryFailure>), (PathBuf, io::Error)> {
    let mut candidates = Vec::new();
    let mut ignored = Vec::new();
    let mut failures = Vec::new();
    let mut stack = vec![PathBuf::new()];

    while let Some(relative_dir) = stack.pop() {
        let absolute_dir = input_root.join(&relative_dir);
        let listing = match fs::read_dir(&absolute_dir) {
            Ok(listing) => listing,
            Err(err) => {
                if relative_dir.as_os_str().is_empty() {
                    return Err((input_root.to_path_buf(), err));
                }
                failures.push(DiscoveryFailure {
                    relative: relative_dir,
                    category: "UnreadableEntry",
                    message: String::from("could not read subdirectory"),
                });
                continue;
            }
        };
        // Depth of the directory being listed (root is depth 0).
        let depth = relative_dir.components().count();
        for entry in listing {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    failures.push(DiscoveryFailure {
                        relative: relative_dir.clone(),
                        category: "UnreadableEntry",
                        message: String::from("could not read directory entry"),
                    });
                    continue;
                }
            };
            let relative = relative_dir.join(entry.file_name());
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => {
                    failures.push(DiscoveryFailure {
                        relative,
                        category: "UnreadableEntry",
                        message: String::from("could not inspect directory entry"),
                    });
                    continue;
                }
            };
            // Below the top level in non-recursive mode, eligible files are
            // tallied as ignored (documented M8.1 T-nonrec-1 behavior), but
            // the walk still descends so the tally is complete.
            let below_top = !recursive && depth > 0;
            if file_type.is_dir() {
                stack.push(relative);
            } else if file_type.is_symlink() {
                if !is_flac_name(&entry.file_name()) {
                    // The extension alone already excludes this entry. When
                    // the target is a directory, report the more specific
                    // "not followed" reason without changing the outcome.
                    let reason = match fs::metadata(entry.path()) {
                        Ok(target) if target.is_dir() => SYMLINKED_DIR_REASON,
                        _ => UNSUPPORTED_EXTENSION_REASON,
                    };
                    ignored.push(IgnoredEntry {
                        relative,
                        reason: Some(reason),
                    });
                    continue;
                }
                match fs::metadata(entry.path()) {
                    Ok(target) if target.is_dir() => {
                        // A symlink to a directory is never descended.
                        ignored.push(IgnoredEntry {
                            relative,
                            reason: Some(SYMLINKED_DIR_REASON),
                        });
                    }
                    Ok(target) if target.is_file() => {
                        if below_top {
                            ignored.push(IgnoredEntry {
                                relative,
                                reason: Some(NON_RECURSIVE_REASON),
                            });
                        } else {
                            candidates.push(relative);
                        }
                    }
                    Ok(_) => {
                        // A symlink to a non-file target can never convert.
                        failures.push(DiscoveryFailure {
                            relative,
                            category: "InputNotFile",
                            message: String::from("input is not a regular file"),
                        });
                    }
                    Err(_) => {
                        // A broken symlink can never be opened.
                        failures.push(DiscoveryFailure {
                            relative,
                            category: "InputOpen",
                            message: String::from("could not open input file"),
                        });
                    }
                }
            } else if file_type.is_file() {
                if !is_flac_name(&entry.file_name()) {
                    ignored.push(IgnoredEntry {
                        relative,
                        reason: Some(UNSUPPORTED_EXTENSION_REASON),
                    });
                } else if below_top {
                    ignored.push(IgnoredEntry {
                        relative,
                        reason: Some(NON_RECURSIVE_REASON),
                    });
                } else {
                    candidates.push(relative);
                }
            } else {
                // FIFOs, sockets, devices: ignored, never opened.
                ignored.push(IgnoredEntry {
                    relative,
                    reason: Some(NON_REGULAR_FILE_REASON),
                });
            }
        }
    }

    candidates.sort_by_key(|a| sort_key(a));
    ignored.sort_by_key(|a| sort_key(&a.relative));
    failures.sort_by_key(|a| sort_key(&a.relative));
    Ok((candidates, ignored, failures))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_jobs_matches_available_parallelism() {
        let expected = std::thread::available_parallelism().map_or(1, |count| count.get());
        assert!(expected >= 1);
        assert_eq!(super::default_jobs(), expected);
        assert_eq!(BatchOptions::default().jobs, expected);
    }

    #[test]
    fn batch_extensions_are_canonical() {
        assert_eq!(batch_extension(OutputFormat::M4a), "m4a");
        assert_eq!(batch_extension(OutputFormat::Adts), "aac");
    }

    #[test]
    fn flac_name_matching_is_case_insensitive() {
        for eligible in ["a.flac", "A.FLAC", "mix.FlAc", ".hidden.flac"] {
            assert!(is_flac_name(OsStr::new(eligible)), "{eligible}");
        }
        for ignored in ["a.txt", "a.m4a", "a.flac.bak", "aflac", "flac", ".flac-dir"] {
            assert!(!is_flac_name(OsStr::new(ignored)), "{ignored}");
        }
        // Extensionless and empty-adjacent names are ignored.
        assert!(!is_flac_name(OsStr::new("trackflac")));
        assert!(!is_flac_name(OsStr::new("")));
    }

    #[test]
    fn mapping_preserves_stem_and_structure() {
        let cases = [
            ("track01.flac", "track01.m4a", OutputFormat::M4a),
            (
                "Disc 1/Track 01.FLAC",
                "Disc 1/Track 01.m4a",
                OutputFormat::M4a,
            ),
            ("live.take-2.FlAc", "live.take-2.m4a", OutputFormat::M4a),
            ("archive.tar.flac", "archive.tar.m4a", OutputFormat::M4a),
            (".hidden.flac", ".hidden.m4a", OutputFormat::M4a),
            ("track01.flac", "track01.aac", OutputFormat::Adts),
            ("Sub/UPPER.FLAC", "Sub/UPPER.aac", OutputFormat::Adts),
        ];
        for (input, expected, format) in cases {
            assert_eq!(
                map_relative_output(Path::new(input), format),
                PathBuf::from(expected),
                "{input}"
            );
        }
        // Batch mode never emits .mp4 or .adts.
        for (_, format) in [("x", OutputFormat::M4a), ("x", OutputFormat::Adts)] {
            let mapped = map_relative_output(Path::new("s.flac"), format);
            let extension = mapped.extension().unwrap().to_str().unwrap();
            assert!(
                matches!(extension, "m4a" | "aac"),
                "unexpected batch extension {extension}"
            );
        }
    }

    #[test]
    fn duplicate_detection_catches_case_folds() {
        fn entry(relative_input: &str, relative_output: &str) -> BatchPlanEntry {
            BatchPlanEntry {
                input: PathBuf::from(relative_input),
                relative_input: PathBuf::from(relative_input),
                relative_output: PathBuf::from(relative_output),
                output: PathBuf::from(relative_output),
            }
        }
        // Exact duplicates abort.
        let duplicate = find_duplicate(&[entry("a.flac", "a.m4a"), entry("b.flac", "a.m4a")]);
        assert!(
            matches!(duplicate, Err(BatchError::DuplicateDestination { .. })),
            "{duplicate:?}"
        );
        // ASCII case-fold collisions abort even though the text differs.
        let folded = find_duplicate(&[entry("A.flac", "A.m4a"), entry("a.flac", "a.m4a")]);
        assert!(
            matches!(folded, Err(BatchError::DuplicateDestination { .. })),
            "{folded:?}"
        );
        // Distinct destinations pass.
        assert!(find_duplicate(&[entry("A.flac", "A.m4a"), entry("b.flac", "b.m4a")]).is_ok());
        assert!(find_duplicate(&[]).is_ok());
    }

    #[test]
    fn error_categories_reuse_convert_error_names() {
        let input = PathBuf::from("in.flac");
        let cases: Vec<(ConvertError, &'static str)> = vec![
            (
                ConvertError::InputOpen(input.clone(), io::Error::other("x")),
                "InputOpen",
            ),
            (ConvertError::InputNotFile(input.clone()), "InputNotFile"),
            (ConvertError::DecodeFailed, "DecodeFailed"),
            (ConvertError::UnsupportedBitDepth(24), "UnsupportedBitDepth"),
            (ConvertError::Metadata("could not read tags"), "Metadata"),
            (ConvertError::OutputExists(input.clone()), "OutputExists"),
            (ConvertError::SamePath(input.clone()), "SamePath"),
        ];
        for (error, category) in cases {
            assert_eq!(category_for(&error), category);
        }
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "aac-convert-batch-unit-{}-{label}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn cover(marker: u8) -> crate::meta::CoverArt {
        crate::meta::CoverArt {
            format: crate::meta::CoverFormat::Jpeg,
            data: vec![0xFF, 0xD8, 0xFF, 0xE0, marker],
        }
    }

    fn artwork_plan(root: &Path, relative: &[&str]) -> BatchPlan {
        let entries = relative
            .iter()
            .map(|relative| {
                let relative = PathBuf::from(relative);
                let output_relative = map_relative_output(&relative, OutputFormat::M4a);
                BatchPlanEntry {
                    input: root.join(&relative),
                    relative_input: relative,
                    output: root.join("out").join(&output_relative),
                    relative_output: output_relative,
                }
            })
            .collect();
        BatchPlan {
            input_root: root.to_path_buf(),
            output_root: root.join("out"),
            entries,
            ignored: Vec::new(),
            discovery_failures: Vec::new(),
        }
    }

    #[test]
    fn existing_external_artwork_avoids_the_embedded_reader() {
        let directory = TestDir::new("artwork-external");
        fs::create_dir_all(directory.path.join("album")).unwrap();
        fs::write(
            directory.path.join("album/cover.jpg"),
            [0xFF, 0xD8, 0xFF, 0xE0, 0x01],
        )
        .unwrap();
        let plan = artwork_plan(&directory.path, &["album/one.flac", "album/two.flac"]);
        let calls = std::cell::Cell::new(0usize);
        let album = resolve_album_artwork_with(
            &plan,
            &[0, 1],
            &BatchOptions::default(),
            None,
            0,
            |_: &Path| {
                calls.set(calls.get() + 1);
                Ok((Some(cover(0x02)), Vec::new()))
            },
        );
        assert_eq!(calls.get(), 0, "embedded extraction must be skipped");
        assert_eq!(album.source, ArtworkSource::ExistingFile);
        assert_eq!(
            album.existing_format,
            Some(ArtworkFormat::Jpeg),
            "format must be retained without image bytes"
        );
        assert!(album.spill_path.is_none());
        // No image bytes are retained: loading happens on demand.
        let loaded = album.load_existing_cover().expect("must load from source");
        assert_eq!(loaded.data, [0xFF, 0xD8, 0xFF, 0xE0, 0x01]);
    }

    #[test]
    fn embedded_selection_reads_each_member_once_and_spills_for_publication() {
        let directory = TestDir::new("artwork-embedded-once");
        let plan = artwork_plan(
            &directory.path,
            &["album/a.flac", "album/b.flac", "album/c.flac"],
        );
        let staging = ArtworkStaging::new(&directory.path).expect("staging must be created");
        let calls = std::cell::Cell::new(0usize);
        let album = resolve_album_artwork_with(
            &plan,
            &[0, 1, 2],
            &BatchOptions {
                cover_art: CoverArtMode::External,
                ..BatchOptions::default()
            },
            Some(&staging),
            0,
            |_: &Path| {
                calls.set(calls.get() + 1);
                Ok((Some(cover(0x03)), Vec::new()))
            },
        );
        // One read per member is required for conflict detection; the selected
        // cover is spilled to disk and never retained in RAM.
        assert_eq!(calls.get(), 3);
        assert_eq!(album.source, ArtworkSource::Embedded);
        assert!(!album.conflicted);
        let spill = album.spill_path.as_ref().expect("must spill");
        assert_eq!(fs::read(spill).unwrap(), cover(0x03).data);
        assert_eq!(album.embedded_format, Some(crate::meta::CoverFormat::Jpeg));
        // Publication loads from the spill file on demand.
        let (file_name, bytes) = album.external_file().expect("must publish");
        assert_eq!(file_name, "cover.jpg");
        assert_eq!(bytes, cover(0x03).data);
    }

    #[test]
    fn different_embedded_images_conflict() {
        let directory = TestDir::new("artwork-embedded-conflict");
        let plan = artwork_plan(&directory.path, &["album/a.flac", "album/b.flac"]);
        let staging = ArtworkStaging::new(&directory.path).expect("staging must be created");
        let album = resolve_album_artwork_with(
            &plan,
            &[0, 1],
            &BatchOptions {
                cover_art: CoverArtMode::External,
                ..BatchOptions::default()
            },
            Some(&staging),
            1,
            |path: &Path| {
                let marker = if path.ends_with("a.flac") { 0x04 } else { 0x05 };
                Ok((Some(cover(marker)), Vec::new()))
            },
        );
        assert!(album.conflicted);
    }

    #[test]
    fn external_artwork_is_not_used_for_embedded_only_webp() {
        let directory = TestDir::new("artwork-webp-embedded");
        fs::create_dir_all(directory.path.join("album")).unwrap();
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&12u32.to_le_bytes());
        webp.extend_from_slice(b"WEBPVP8 payload");
        fs::write(directory.path.join("album/cover.webp"), &webp).unwrap();
        let plan = artwork_plan(&directory.path, &["album/one.flac"]);
        let album = resolve_album_artwork_with(
            &plan,
            &[0],
            &BatchOptions::default(),
            None,
            0,
            |_: &Path| Ok((None, Vec::new())),
        );
        assert_eq!(
            album.source,
            ArtworkSource::None,
            "WebP cannot be embedded and embedded mode must fall back"
        );
        assert!(album.spill_path.is_none());
        assert!(album.source_path.is_none());
    }

    #[test]
    fn embedded_fallback_preserves_per_member_warnings() {
        let directory = TestDir::new("artwork-member-warnings");
        let plan = artwork_plan(&directory.path, &["album/a.flac", "album/b.flac"]);
        let staging = ArtworkStaging::new(&directory.path).expect("staging must be created");
        let album = resolve_album_artwork_with(
            &plan,
            &[0, 1],
            &BatchOptions {
                cover_art: CoverArtMode::External,
                ..BatchOptions::default()
            },
            Some(&staging),
            2,
            |path: &Path| {
                if path.ends_with("a.flac") {
                    Ok((
                        None,
                        vec!["unsupported cover image format: skipped".to_string()],
                    ))
                } else {
                    Ok((Some(cover(0x06)), vec!["first warning".to_string()]))
                }
            },
        );
        // Warnings are stored per member, not collapsed to the first cover.
        assert_eq!(
            album
                .member_warnings
                .get(&PathBuf::from("album/a.flac"))
                .cloned()
                .unwrap_or_default(),
            vec!["unsupported cover image format: skipped".to_string()]
        );
        assert_eq!(
            album
                .member_warnings
                .get(&PathBuf::from("album/b.flac"))
                .cloned()
                .unwrap_or_default(),
            vec!["first warning".to_string()]
        );
        // Each member resolves to its own warnings.
        let options = BatchOptions {
            cover_art: CoverArtMode::External,
            ..BatchOptions::default()
        };
        let resolved_a = resolved_cover_for(&album, Path::new("album/a.flac"), &options, false)
            .expect("member with warnings must resolve");
        assert!(resolved_a.cover.is_none());
        assert_eq!(
            resolved_a.warnings,
            vec!["unsupported cover image format: skipped".to_string()]
        );
        let resolved_b = resolved_cover_for(&album, Path::new("album/b.flac"), &options, false)
            .expect("member with warnings must resolve");
        assert_eq!(resolved_b.warnings, vec!["first warning".to_string()]);
    }

    #[test]
    fn probe_failures_fall_back_to_per_file_selection() {
        let directory = TestDir::new("artwork-probe-failure");
        let plan = artwork_plan(&directory.path, &["album/a.flac", "album/b.flac"]);
        let staging = ArtworkStaging::new(&directory.path).expect("staging must be created");
        let album = resolve_album_artwork_with(
            &plan,
            &[0, 1],
            &BatchOptions {
                cover_art: CoverArtMode::External,
                ..BatchOptions::default()
            },
            Some(&staging),
            3,
            |path: &Path| {
                if path.ends_with("a.flac") {
                    Err(ConvertError::Metadata("could not read tags"))
                } else {
                    Ok((Some(cover(0x07)), Vec::new()))
                }
            },
        );
        // The failed member has no stored entry, so conversion falls back to
        // its own per-file selection instead of reusing another member's
        // warnings.
        assert!(!album
            .member_warnings
            .contains_key(&PathBuf::from("album/a.flac")));
        let options = BatchOptions {
            cover_art: CoverArtMode::External,
            ..BatchOptions::default()
        };
        assert!(resolved_cover_for(&album, Path::new("album/a.flac"), &options, false).is_none());
        assert!(resolved_cover_for(&album, Path::new("album/b.flac"), &options, false).is_some());
    }
}
