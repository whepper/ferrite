use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use aac_convert::batch::{
    default_jobs, execute_plan_with_options_and_progress, plan_batch, BatchEvent, BatchFileOutcome,
    BatchOptions, BatchProgress,
};
use aac_convert::{
    convert_file_with_format_tags_and_cover_art, CoverArtAction, CoverArtMode, CoverArtReport,
    OutputFormat,
};

const USAGE: &str = "\
Usage: aac-convert [--format adts|m4a] [--cover-art embedded|external|both] [--no-tags]
                   <input.flac> <output>
       aac-convert [--format m4a|adts] [--recursive|--no-recursive] [--dry-run]
                   [--skip-existing] [--no-tags] [--jobs N]
                   [--cover-art embedded|external|both] [-v|--verbose] [-q|--quiet]
                   <input-dir> <output-root>

Convert 16-bit FLAC files (44.1 kHz or 48 kHz, mono or stereo) to AAC-LC
using the native Ferrite Rust AAC-LC encoder (VBR-5, derived from FDK AAC).
24-bit FLAC input is truncated to 16-bit PCM before encoding (one warning per
file); other bit depths are rejected.

A regular file input converts one file; a directory input converts every
discovered FLAC file, mirroring the input structure.

Output format:
  --format m4a    Audio-only MP4/M4A container
  --format adts   ADTS AAC stream (engineering/test output)

Single-file mode: without --format, a .m4a or .mp4 output selects M4A; every
other extension selects ADTS. Existing output files are never overwritten.

Batch (directory) mode: --format defaults to m4a and selects the output
extension (.m4a or .aac). Traversal is recursive by default; --no-recursive
converts only top-level files (last of the two flags wins). --dry-run prints
the plan without writing anything. --skip-existing skips destinations that
already exist instead of failing on them; it never overwrites anything.
--no-tags omits user metadata from M4A output (accepted for ADTS as a no-op).
--jobs N limits preparation and encoding to N concurrent files (default:
available logical CPUs; 1 is strictly sequential). -v/--verbose adds per-file
detail, including why entries were ignored; -q/--quiet prints only the final
summary (conflicts with --verbose).

Cover art (M4A with tags):
  --cover-art embedded   Embed selected artwork in the M4A (default)
  --cover-art external   Write cover.jpg/cover.png beside the audio; do not embed
  --cover-art both       Embed and write the same selected artwork
External files are never overwritten. Conflicting artwork across tracks in one
output directory is reported and skipped for that directory. --no-tags and
ADTS output never embed or extract artwork.

Without --format, a .m4a or .mp4 output selects M4A; every other extension
selects ADTS. Existing output files are never overwritten.
";

fn print_usage() {
    print!("{USAGE}");
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(message)) => {
            eprintln!("error: {message}");
            eprintln!();
            print_usage();
            ExitCode::from(2)
        }
        Err(CliError::Conversion { input, error }) => {
            eprintln!("error: {}: {error}", input.display());
            ExitCode::FAILURE
        }
        Err(CliError::Setup(message)) => {
            eprintln!("error: {message}");
            ExitCode::from(3)
        }
        Err(CliError::Partial) => ExitCode::FAILURE,
    }
}

enum CliError {
    Usage(String),
    Conversion {
        input: PathBuf,
        error: aac_convert::ConvertError,
    },
    /// Directory-mode fatal setup failure (exit 3): nothing was converted.
    Setup(String),
    /// Batch completed with per-file failures (exit 1); details already printed.
    Partial,
}

fn parse_format(value: &OsStr) -> Result<OutputFormat, CliError> {
    match value.to_str() {
        Some("adts") => Ok(OutputFormat::Adts),
        Some("m4a") => Ok(OutputFormat::M4a),
        _ => Err(CliError::Usage(format!(
            "invalid output format: {} (expected adts or m4a)",
            value.to_string_lossy()
        ))),
    }
}

fn parse_cover_art(value: &OsStr) -> Result<CoverArtMode, CliError> {
    value
        .to_str()
        .ok_or_else(|| CliError::Usage("--cover-art requires embedded, external, or both".into()))?
        .parse::<CoverArtMode>()
        .map_err(CliError::Usage)
}

fn parse_jobs(value: &OsStr) -> Result<usize, CliError> {
    let text = value
        .to_str()
        .ok_or_else(|| CliError::Usage("--jobs requires a positive integer".into()))?;
    let jobs: usize = text.parse().map_err(|_| {
        CliError::Usage(format!(
            "invalid --jobs value: {text} (expected a positive integer)"
        ))
    })?;
    if jobs == 0 {
        return Err(CliError::Usage(
            "invalid --jobs value: 0 (expected a positive integer)".into(),
        ));
    }
    Ok(jobs)
}

struct Cli {
    explicit_format: Option<OutputFormat>,
    recursive: Option<bool>,
    dry_run: bool,
    skip_existing: bool,
    no_tags: bool,
    cover_art: CoverArtMode,
    jobs: Option<usize>,
    verbose: bool,
    quiet: bool,
    input: PathBuf,
    output: PathBuf,
}

fn parse_cli() -> Result<Option<Cli>, CliError> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let mut positional = Vec::new();
    let mut explicit_format = None;
    let mut recursive = None;
    let mut dry_run = false;
    let mut skip_existing = false;
    let mut no_tags = false;
    let mut cover_art = CoverArtMode::Embedded;
    let mut jobs = None;
    let mut verbose = false;
    let mut quiet = false;
    let mut options_done = false;
    let mut index = 0;

    while index < arguments.len() {
        let argument = &arguments[index];
        if !options_done {
            match argument.to_str() {
                Some("-h" | "--help") => {
                    print_usage();
                    return Ok(None);
                }
                Some("-V" | "--version") => {
                    println!("aac-convert {}", env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                Some("--") => {
                    options_done = true;
                    index += 1;
                    continue;
                }
                Some("--format") => {
                    index += 1;
                    let value = arguments
                        .get(index)
                        .ok_or_else(|| CliError::Usage("--format requires adts or m4a".into()))?;
                    explicit_format = Some(parse_format(value)?);
                    index += 1;
                    continue;
                }
                Some(value) if value.starts_with("--format=") => {
                    explicit_format = Some(parse_format(OsStr::new(&value["--format=".len()..]))?);
                    index += 1;
                    continue;
                }
                Some("--recursive") => {
                    recursive = Some(true);
                    index += 1;
                    continue;
                }
                Some("--no-recursive") => {
                    recursive = Some(false);
                    index += 1;
                    continue;
                }
                Some("--dry-run") => {
                    dry_run = true;
                    index += 1;
                    continue;
                }
                Some("--skip-existing") => {
                    skip_existing = true;
                    index += 1;
                    continue;
                }
                Some("--no-tags") => {
                    no_tags = true;
                    index += 1;
                    continue;
                }
                Some("--cover-art") => {
                    index += 1;
                    let value = arguments.get(index).ok_or_else(|| {
                        CliError::Usage("--cover-art requires embedded, external, or both".into())
                    })?;
                    cover_art = parse_cover_art(value)?;
                    index += 1;
                    continue;
                }
                Some(value) if value.starts_with("--cover-art=") => {
                    cover_art = parse_cover_art(OsStr::new(&value["--cover-art=".len()..]))?;
                    index += 1;
                    continue;
                }
                Some("--jobs") => {
                    index += 1;
                    let value = arguments.get(index).ok_or_else(|| {
                        CliError::Usage("--jobs requires a positive integer".into())
                    })?;
                    jobs = Some(parse_jobs(value)?);
                    index += 1;
                    continue;
                }
                Some(value) if value.starts_with("--jobs=") => {
                    jobs = Some(parse_jobs(OsStr::new(&value["--jobs=".len()..]))?);
                    index += 1;
                    continue;
                }
                Some("-v" | "--verbose") => {
                    verbose = true;
                    index += 1;
                    continue;
                }
                Some("-q" | "--quiet") => {
                    quiet = true;
                    index += 1;
                    continue;
                }
                Some(value) if value.starts_with('-') => {
                    return Err(CliError::Usage(format!("unknown option: {value}")));
                }
                _ => {}
            }
        }
        positional.push(argument.clone());
        index += 1;
    }

    if positional.len() != 2 {
        return Err(CliError::Usage(
            "expected exactly one input path and one output path".to_string(),
        ));
    }
    if verbose && quiet {
        return Err(CliError::Usage(
            "--verbose and --quiet conflict; choose one".to_string(),
        ));
    }

    Ok(Some(Cli {
        explicit_format,
        recursive,
        dry_run,
        skip_existing,
        no_tags,
        cover_art,
        jobs,
        verbose,
        quiet,
        input: PathBuf::from(positional.remove(0)),
        output: PathBuf::from(positional.remove(0)),
    }))
}

/// File-type dispatch for the input path.
enum InputKind {
    File,
    Directory,
}

fn dispatch_input(input: &PathBuf) -> Result<InputKind, CliError> {
    match fs::symlink_metadata(input) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(CliError::Setup(format!(
            "input path does not exist: {}",
            input.display()
        ))),
        Err(err) => Err(CliError::Setup(format!(
            "could not inspect input path: {} ({err})",
            input.display()
        ))),
        Ok(metadata) => {
            if metadata.is_symlink() {
                match input.canonicalize() {
                    Err(_) => Err(CliError::Setup(format!(
                        "input path does not exist: {}",
                        input.display()
                    ))),
                    Ok(target) => {
                        if target.is_file() {
                            Ok(InputKind::File)
                        } else if target.is_dir() {
                            Ok(InputKind::Directory)
                        } else {
                            Err(CliError::Setup(format!(
                                "input is neither a regular file nor a directory: {}",
                                input.display()
                            )))
                        }
                    }
                }
            } else if metadata.is_file() {
                Ok(InputKind::File)
            } else if metadata.is_dir() {
                Ok(InputKind::Directory)
            } else {
                Err(CliError::Setup(format!(
                    "input is neither a regular file nor a directory: {}",
                    input.display()
                )))
            }
        }
    }
}

fn run() -> Result<(), CliError> {
    let cli = match parse_cli()? {
        Some(cli) => cli,
        None => return Ok(()),
    };
    match dispatch_input(&cli.input)? {
        InputKind::File => run_single_file(&cli),
        InputKind::Directory => run_batch(&cli),
    }
}

fn run_single_file(cli: &Cli) -> Result<(), CliError> {
    // Batch-only options are usage errors on a file input: they would
    // otherwise silently do nothing (recursion, dry-run) or change the fixed
    // single-file report (verbosity).
    if cli.recursive.is_some() {
        return Err(CliError::Usage(
            "--recursive/--no-recursive requires a directory input".to_string(),
        ));
    }
    if cli.dry_run {
        return Err(CliError::Usage(
            "--dry-run requires a directory input".to_string(),
        ));
    }
    if cli.skip_existing {
        return Err(CliError::Usage(
            "--skip-existing requires a directory input".to_string(),
        ));
    }
    if cli.jobs.is_some() {
        return Err(CliError::Usage(
            "--jobs requires a directory input".to_string(),
        ));
    }
    if cli.verbose || cli.quiet {
        return Err(CliError::Usage(
            "--verbose/--quiet requires a directory input".to_string(),
        ));
    }

    let format = cli
        .explicit_format
        .unwrap_or_else(|| OutputFormat::from_output_path(&cli.output));
    let (summary, diagnostics, cover_reports) = convert_file_with_format_tags_and_cover_art(
        &cli.input,
        &cli.output,
        format,
        !cli.no_tags,
        cli.cover_art,
    )
    .map_err(|error| CliError::Conversion {
        input: cli.input.clone(),
        error,
    })?;
    for warning in &diagnostics.warnings {
        eprintln!("warning: {}: {warning}", cli.input.display());
    }
    for report in &cover_reports {
        print_cover_report(report, false, true);
    }
    println!(
        "converted {} -> {} ({} AAC frames, {} bytes, {})",
        cli.input.display(),
        cli.output.display(),
        summary.encoded_frames,
        summary.output_bytes,
        format
    );
    Ok(())
}

fn channel_word(channels: u32) -> &'static str {
    match channels {
        1 => "mono",
        2 => "stereo",
        _ => "multi",
    }
}

/// Effective worker count for the batch header line.
fn jobs_label(jobs: usize) -> String {
    if jobs == 1 {
        "1 job".to_string()
    } else {
        format!("{jobs} jobs")
    }
}

/// One stderr progress line: `[  1/111] path  status`.
fn progress_line(item: &BatchProgress, status: &str) -> String {
    let width = item.total.to_string().len().max(1);
    format!(
        "[{:>width$}/{}] {}  {}",
        item.index,
        item.total,
        item.relative_input.display(),
        status,
        width = width
    )
}

/// Report external cover-art outcomes on stderr.
///
/// Conflicts and failures are diagnostics and always print. A newly written
/// file prints once per output directory in normal mode; an untouched existing
/// file prints only in verbose mode.
fn print_cover_report(report: &CoverArtReport, quiet: bool, verbose: bool) {
    let output_root = report.directory == Path::new(".");
    let path = match &report.file_name {
        Some(file_name) if output_root => PathBuf::from(file_name),
        Some(file_name) => report.directory.join(file_name),
        None if output_root => PathBuf::from("output root"),
        None => report.directory.clone(),
    };
    let directory = if output_root {
        "output root".to_string()
    } else {
        report.directory.display().to_string()
    };
    match &report.action {
        CoverArtAction::Written => {
            if !quiet {
                eprintln!("cover {}: written", path.display());
            }
        }
        CoverArtAction::Existing => {
            if !quiet && verbose {
                eprintln!("cover {}: already exists; left unchanged", path.display());
            }
        }
        CoverArtAction::Conflicted => {
            eprintln!(
                "warning: external cover art not written for {directory}: \
                 source tracks contain different cover images"
            );
        }
        CoverArtAction::Failed { message } => {
            eprintln!(
                "warning: external cover art not written for {}: {message}",
                path.display()
            );
        }
    }
}

fn run_batch(cli: &Cli) -> Result<(), CliError> {
    let format = cli.explicit_format.unwrap_or(OutputFormat::M4a);
    let recursive = cli.recursive.unwrap_or(true);
    let jobs = cli.jobs.unwrap_or_else(default_jobs);
    let plan = plan_batch(&cli.input, &cli.output, format, recursive)
        .map_err(|err| CliError::Setup(err.to_string()))?;

    let options = BatchOptions {
        dry_run: cli.dry_run,
        skip_existing: cli.skip_existing,
        write_tags: !cli.no_tags,
        cover_art: cli.cover_art,
        jobs,
    };

    let start = std::time::Instant::now();
    let quiet = cli.quiet;
    let dry_run = cli.dry_run;
    let execution = execute_plan_with_options_and_progress(&plan, format, &options, |event| {
        // Dry runs report the plan on stdout and must not pretend to
        // encode; failures keep their detailed stderr diagnostic.
        if dry_run {
            if let BatchEvent::Failed { item, message, .. } = &event {
                eprintln!("error: {}: {message}", item.relative_input.display());
            }
            return;
        }
        match event {
            BatchEvent::Preparing { total, cover_art } => {
                if !quiet {
                    let detail = if cover_art {
                        "loudness and artwork"
                    } else {
                        "loudness"
                    };
                    let noun = if total == 1 { "file" } else { "files" };
                    eprintln!("preparing {total} {noun} ({detail})...");
                }
            }
            BatchEvent::Started(item) => {
                if !quiet {
                    eprintln!("{}", progress_line(&item, "encoding..."));
                }
            }
            BatchEvent::Converted { item, tags } => {
                if !quiet {
                    eprintln!("{}", progress_line(&item, "done"));
                }
                // Warnings surface as soon as the file finishes, not after
                // the whole batch.
                for warning in &tags.warnings {
                    eprintln!("warning: {}: {warning}", item.relative_input.display());
                }
            }
            BatchEvent::Skipped(item) => {
                if !quiet {
                    eprintln!("{}", progress_line(&item, "skipped (already exists)"));
                }
            }
            BatchEvent::Failed {
                item,
                category,
                message,
            } => {
                if !quiet {
                    eprintln!("{}", progress_line(&item, &format!("failed ({category})")));
                }
                eprintln!("error: {}: {message}", item.relative_input.display());
            }
            BatchEvent::Planned(item) => {
                if !quiet {
                    eprintln!("{}", progress_line(&item, "would convert"));
                }
            }
        }
    })
    .map_err(|err| CliError::Setup(err.to_string()))?;
    let elapsed = start.elapsed();
    let outcome = &execution.outcome;

    let mode = if recursive { "recursive" } else { "top-level" };
    let skip_suffix = if cli.skip_existing {
        ", skip-existing"
    } else {
        ""
    };
    if !cli.quiet {
        let workers = jobs_label(jobs);
        if cli.dry_run {
            println!(
                "batch: {} files, format {format}, {mode}{skip_suffix}, dry-run, {workers}",
                plan.entries.len()
            );
        } else {
            println!(
                "batch: {} files, format {format}, {mode}{skip_suffix}, {workers}",
                plan.entries.len()
            );
        }
        if cli.verbose {
            for ignored in &plan.ignored {
                match ignored.reason {
                    Some(reason) => println!("ignore {} ({reason})", ignored.relative.display()),
                    None => println!("ignore {}", ignored.relative.display()),
                }
            }
            // Tag diagnostics mirror the `ignore` lines: verbose-only, in
            // processing order, after the ignore tally. Dry runs never read
            // tags, so they report nothing here.
            if !cli.dry_run {
                for result in &outcome.results {
                    if let BatchFileOutcome::Converted { tags, .. } = &result.outcome {
                        println!(
                            "tags {}: {} mapped, {} unmapped",
                            result.relative_input.display(),
                            tags.mapped,
                            tags.unmapped.len()
                        );
                        for key in &tags.unmapped {
                            println!("unmapped {}: {key}", result.relative_input.display());
                        }
                    }
                }
            }
        }
    }

    for report in &execution.cover_art {
        print_cover_report(report, cli.quiet, cli.verbose);
    }

    for result in &outcome.results {
        match &result.outcome {
            BatchFileOutcome::Converted {
                frames,
                bytes,
                properties,
                ..
            } => {
                if !cli.quiet {
                    let relative_output = result
                        .relative_output
                        .as_ref()
                        .expect("conversions always have a destination");
                    if cli.verbose {
                        println!(
                            "ok {} -> {} ({frames} frames, {bytes} bytes, {} Hz, {}, {}-bit)",
                            result.relative_input.display(),
                            relative_output.display(),
                            properties.sample_rate,
                            channel_word(properties.channels),
                            properties.bits_per_sample
                        );
                    } else {
                        println!(
                            "ok {} -> {} ({frames} frames, {bytes} bytes)",
                            result.relative_input.display(),
                            relative_output.display()
                        );
                    }
                }
            }
            BatchFileOutcome::Planned => {
                if !cli.quiet {
                    let relative_output = result
                        .relative_output
                        .as_ref()
                        .expect("planned items always have a destination");
                    println!(
                        "plan {} -> {}",
                        result.relative_input.display(),
                        relative_output.display()
                    );
                }
            }
            BatchFileOutcome::Skipped => {
                // Skips are informational, not diagnostics: stdout in
                // normal/verbose mode, nothing on stderr, nothing in quiet
                // mode beyond the summary.
                if !cli.quiet {
                    let relative_output = result
                        .relative_output
                        .as_ref()
                        .expect("skipped items always have a destination");
                    println!(
                        "skip {} -> {} (exists)",
                        result.relative_input.display(),
                        relative_output.display()
                    );
                }
            }
            BatchFileOutcome::Failed { category, .. } => {
                if !cli.quiet {
                    println!("fail {}: {category}", result.relative_input.display());
                }
            }
        }
    }

    let summary = &outcome.summary;
    // The summary shape is a pure function of the flags: without
    // `--skip-existing` it is byte-identical to the M8.2 four-count line, so
    // existing parsers keep working; with the flag it gains a trailing
    // skipped count.
    if cli.skip_existing {
        if cli.dry_run {
            println!(
                "dry-run: {} converted, {} failed, {} existing, {} ignored, {} skipped ({:.2}s)",
                summary.converted,
                summary.failed,
                summary.existing,
                summary.ignored,
                summary.skipped,
                elapsed.as_secs_f64()
            );
        } else {
            println!(
                "summary: {} converted, {} failed, {} existing, {} ignored, {} skipped ({:.2}s)",
                summary.converted,
                summary.failed,
                summary.existing,
                summary.ignored,
                summary.skipped,
                elapsed.as_secs_f64()
            );
        }
    } else if cli.dry_run {
        println!(
            "dry-run: {} converted, {} failed, {} existing, {} ignored ({:.2}s)",
            summary.converted,
            summary.failed,
            summary.existing,
            summary.ignored,
            elapsed.as_secs_f64()
        );
    } else {
        println!(
            "summary: {} converted, {} failed, {} existing, {} ignored ({:.2}s)",
            summary.converted,
            summary.failed,
            summary.existing,
            summary.ignored,
            elapsed.as_secs_f64()
        );
    }

    if summary.failed > 0 {
        return Err(CliError::Partial);
    }
    Ok(())
}
