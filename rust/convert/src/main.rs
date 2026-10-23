use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::ExitCode;

use aac_convert::{convert_file_with_format, OutputFormat};

const USAGE: &str = "\
Usage: aac-convert [--format adts|m4a] <input.flac> <output>

Convert one 16-bit FLAC file (44.1 kHz or 48 kHz, mono or stereo) to AAC-LC
using the native FDK AAC VBR-5 encoder.

Output format:
  --format m4a    Audio-only MP4/M4A container
  --format adts   ADTS AAC stream (engineering/test output)

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
    }
}

enum CliError {
    Usage(String),
    Conversion(aac_convert::ConvertError),
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

fn run() -> Result<(), CliError> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let mut positional = Vec::new();
    let mut explicit_format = None;
    let mut options_done = false;
    let mut index = 0;

    while index < arguments.len() {
        let argument = &arguments[index];
        if !options_done {
            match argument.to_str() {
                Some("-h" | "--help") => {
                    print_usage();
                    return Ok(());
                }
                Some("-V" | "--version") => {
                    println!("aac-convert {}", env!("CARGO_PKG_VERSION"));
                    return Ok(());
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

    let input = PathBuf::from(positional.remove(0));
    let output = PathBuf::from(positional.remove(0));
    let format = explicit_format.unwrap_or_else(|| OutputFormat::from_output_path(&output));

    let summary =
        convert_file_with_format(&input, &output, format).map_err(CliError::Conversion)?;
    println!(
        "converted {} -> {} ({} AAC frames, {} bytes, {})",
        input.display(),
        output.display(),
        summary.encoded_frames,
        summary.output_bytes,
        format
    );
    Ok(())
}
