use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use aac_convert::batch::{execute_plan, plan_batch, BatchFileOutcome};
use aac_convert::{convert_file_with_format, OutputFormat};

const USAGE: &str = "\
Usage: aac-convert [--format adts|m4a] <input.flac> <output>
       aac-convert [--format m4a|adts] [--recursive|--no-recursive] [--dry-run]
                   [-v|--verbose] [-q|--quiet] <input-dir> <output-root>

Convert 16-bit FLAC files (44.1 kHz or 48 kHz, mono or stereo) to AAC-LC
using the native FDK AAC VBR-5 encoder.

A regular file input converts one file; a directory input converts every
discovered FLAC file sequentially, mirroring the input structure.

Output format:
  --format m4a    Audio-only MP4/M4A container
  --format adts   ADTS AAC stream (engineering/test output)

Single-file mode: without --format, a .m4a or .mp4 output selects M4A; every
other extension selects ADTS. Existing output files are never overwritten.

Batch (directory) mode: --format defaults to m4a and selects the output
extension (.m4a or .aac). Traversal is recursive by default; --no-recursive
converts only top-level files (last of the two flags wins). --dry-run prints
the plan without writing anything. -v/--verbose adds per-file detail;
-q/--quiet prints only the final summary (conflicts with --verbose).

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
        Err(CliError::Conversion(err)) => {
            eprintln!("error: {err}");
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
    Conversion(aac_convert::ConvertError),
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

struct Cli {
    explicit_format: Option<OutputFormat>,
    recursive: Option<bool>,
    dry_run: bool,
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
    if cli.verbose || cli.quiet {
        return Err(CliError::Usage(
            "--verbose/--quiet requires a directory input".to_string(),
        ));
    }

    let format = cli
        .explicit_format
        .unwrap_or_else(|| OutputFormat::from_output_path(&cli.output));
    let summary =
        convert_file_with_format(&cli.input, &cli.output, format).map_err(CliError::Conversion)?;
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

fn run_batch(cli: &Cli) -> Result<(), CliError> {
    let format = cli.explicit_format.unwrap_or(OutputFormat::M4a);
    let recursive = cli.recursive.unwrap_or(true);
    let plan = plan_batch(&cli.input, &cli.output, format, recursive)
        .map_err(|err| CliError::Setup(err.to_string()))?;

    let start = std::time::Instant::now();
    let outcome =
        execute_plan(&plan, format, cli.dry_run).map_err(|err| CliError::Setup(err.to_string()))?;
    let elapsed = start.elapsed();

    let mode = if recursive { "recursive" } else { "top-level" };
    if !cli.quiet {
        if cli.dry_run {
            println!(
                "batch: {} files, format {format}, {mode}, dry-run",
                plan.entries.len()
            );
        } else {
            println!(
                "batch: {} files, format {format}, {mode}",
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
        }
    }

    for result in &outcome.results {
        match &result.outcome {
            BatchFileOutcome::Converted {
                frames,
                bytes,
                properties,
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
            BatchFileOutcome::Failed { category, message } => {
                if !cli.quiet {
                    println!("fail {}: {category}", result.relative_input.display());
                }
                eprintln!("error: {}: {message}", result.relative_input.display());
            }
        }
    }

    let summary = &outcome.summary;
    if cli.dry_run {
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
