//! Sequential directory batch conversion (M8.2).
//!
//! This module layers directory semantics on top of the unchanged single-file
//! pipeline ([`convert_file_with_format`](crate::convert_file_with_format)).
//! It performs discovery, deterministic planning, pre-flight validation, and
//! sequential execution with per-file failure isolation.
//!
//! Design authority: `docs/aac-convert-m8.1-batch-design.md`. The rules below
//! summarize the normative behavior; the design document is the reference for
//! rationale.
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
//! - Existing destinations are never overwritten; they count as failures for
//!   the exit-code contract and are tallied separately as `existing`.
//! - Each file converts through the existing temporary-file and atomic
//!   hard-link publication path, so no partial output can remain and a
//!   concurrent winner is never replaced.
//! - Execution is strictly sequential with the same per-file bounded-memory
//!   profile as single-file conversion. No new dependencies are used
//!   (`std::fs` traversal with an explicit stack; no recursion).

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{convert_file_with_format, AudioProperties, ConvertError, OutputFormat};

/// Reason recorded for an ignored directory symlink (verbose reporting).
pub const SYMLINKED_DIR_REASON: &str = "symlinked dir, not followed";

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
    },
    /// Dry run: the destination is free and the file would be converted.
    Planned,
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
}

/// Final batch tallies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchSummary {
    /// Files converted (real run) or plannable (dry run).
    pub converted: u64,
    /// Files not converted, including `existing` refusals.
    pub failed: u64,
    /// Subset of `failed`: destinations that already existed.
    pub existing: u64,
    /// Skipped non-eligible entries.
    pub ignored: u64,
}

/// Ordered results plus tallies.
#[derive(Debug, Clone)]
pub struct BatchOutcome {
    /// One item per work unit, in deterministic input order.
    pub results: Vec<BatchFileResult>,
    /// Final tallies.
    pub summary: BatchSummary,
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

/// Execute a plan sequentially with per-file failure isolation.
///
/// - Real runs create the output root (fatal [`BatchError`] on failure),
///   then convert each entry in plan order through
///   [`convert_file_with_format`](crate::convert_file_with_format).
/// - Dry runs create nothing and decode nothing; they report `Planned` for
///   free destinations and `Failed` (`OutputExists`) for taken ones.
/// - Discovery failures are reported as failures in both modes.
///
/// Results are returned in deterministic input order with final tallies.
pub fn execute_plan(
    plan: &BatchPlan,
    format: OutputFormat,
    dry_run: bool,
) -> Result<BatchOutcome, BatchError> {
    let output_root = if dry_run {
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
    enum Work<'a> {
        Entry(&'a BatchPlanEntry),
        Discovery(&'a DiscoveryFailure),
    }
    let mut work: Vec<(Vec<u8>, Work<'_>)> = Vec::new();
    for entry in &plan.entries {
        work.push((sort_key(&entry.relative_input), Work::Entry(entry)));
    }
    for failure in &plan.discovery_failures {
        work.push((sort_key(&failure.relative), Work::Discovery(failure)));
    }
    work.sort_by(|a, b| a.0.cmp(&b.0));

    let mut results = Vec::with_capacity(work.len());
    let mut converted = 0u64;
    let mut failed = 0u64;
    let mut existing = 0u64;

    for (_, item) in work {
        let result = match item {
            Work::Discovery(failure) => {
                failed += 1;
                BatchFileResult {
                    relative_input: failure.relative.clone(),
                    relative_output: None,
                    outcome: BatchFileOutcome::Failed {
                        category: failure.category,
                        message: failure.message.clone(),
                    },
                }
            }
            Work::Entry(entry) => {
                let destination = if dry_run {
                    entry.output.clone()
                } else {
                    let destination = output_root.join(&entry.relative_output);
                    debug_assert!(
                        destination.starts_with(&output_root),
                        "mapped destination escapes the output root"
                    );
                    destination
                };
                if dry_run {
                    match fs::symlink_metadata(&destination) {
                        Ok(_) => {
                            failed += 1;
                            existing += 1;
                            BatchFileResult {
                                relative_input: entry.relative_input.clone(),
                                relative_output: Some(entry.relative_output.clone()),
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
                        Err(err) if err.kind() == io::ErrorKind::NotFound => {
                            converted += 1;
                            BatchFileResult {
                                relative_input: entry.relative_input.clone(),
                                relative_output: Some(entry.relative_output.clone()),
                                outcome: BatchFileOutcome::Planned,
                            }
                        }
                        Err(_) => {
                            failed += 1;
                            BatchFileResult {
                                relative_input: entry.relative_input.clone(),
                                relative_output: Some(entry.relative_output.clone()),
                                outcome: BatchFileOutcome::Failed {
                                    category: "OutputInspect",
                                    message: format!(
                                        "could not inspect output path: {}",
                                        entry.relative_output.display()
                                    ),
                                },
                            }
                        }
                    }
                } else {
                    if let Some(parent) = destination.parent() {
                        if let Err(err) = fs::create_dir_all(parent) {
                            // A concurrent batch may have created the
                            // directory first; only real failures count.
                            if err.kind() != io::ErrorKind::AlreadyExists {
                                failed += 1;
                                results.push(BatchFileResult {
                                    relative_input: entry.relative_input.clone(),
                                    relative_output: Some(entry.relative_output.clone()),
                                    outcome: BatchFileOutcome::Failed {
                                        category: "OutputDirCreate",
                                        message: format!(
                                            "could not create output subdirectory for {}",
                                            entry.relative_input.display()
                                        ),
                                    },
                                });
                                continue;
                            }
                        }
                    }
                    match convert_file_with_format(&entry.input, &destination, format) {
                        Ok(summary) => {
                            converted += 1;
                            BatchFileResult {
                                relative_input: entry.relative_input.clone(),
                                relative_output: Some(entry.relative_output.clone()),
                                outcome: BatchFileOutcome::Converted {
                                    frames: summary.encoded_frames,
                                    bytes: summary.output_bytes,
                                    properties: summary.properties,
                                },
                            }
                        }
                        Err(err) => {
                            failed += 1;
                            let category = category_for(&err);
                            if category == "OutputExists" {
                                existing += 1;
                            }
                            BatchFileResult {
                                relative_input: entry.relative_input.clone(),
                                relative_output: Some(entry.relative_output.clone()),
                                outcome: BatchFileOutcome::Failed {
                                    category,
                                    message: err.to_string(),
                                },
                            }
                        }
                    }
                }
            }
        };
        results.push(result);
    }

    Ok(BatchOutcome {
        results,
        summary: BatchSummary {
            converted,
            failed,
            existing,
            ignored: plan.ignored.len() as u64,
        },
    })
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
                    ignored.push(IgnoredEntry {
                        relative,
                        reason: None,
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
                                reason: None,
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
                if !is_flac_name(&entry.file_name()) || below_top {
                    ignored.push(IgnoredEntry {
                        relative,
                        reason: None,
                    });
                } else {
                    candidates.push(relative);
                }
            } else {
                // FIFOs, sockets, devices: ignored, never opened.
                ignored.push(IgnoredEntry {
                    relative,
                    reason: None,
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
            (ConvertError::OutputExists(input.clone()), "OutputExists"),
            (ConvertError::SamePath(input.clone()), "SamePath"),
        ];
        for (error, category) in cases {
            assert_eq!(category_for(&error), category);
        }
    }
}
