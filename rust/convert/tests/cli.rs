mod common;

use std::process::Command;

use common::{fixture, TempDir};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_aac-convert")
}

#[test]
fn help_succeeds() {
    let output = Command::new(binary()).arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: aac-convert [--format adts|m4a] <input.flac> <output>"));
    assert!(stdout.contains("16-bit"));
    assert!(stdout.contains("--format m4a"));
}

#[test]
fn missing_paths_is_a_usage_error() {
    let output = Command::new(binary()).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected exactly one input path and one output path"));
}

#[test]
fn converts_one_file() {
    let directory = TempDir::new("cli");
    let output = directory.join("out.aac");
    let result = Command::new(binary())
        .arg(fixture("stereo-48k-16.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("converted"));
    assert!(output.is_file());
}

#[test]
fn conversion_failure_is_nonzero_and_actionable() {
    let directory = TempDir::new("cli-failure");
    let output = directory.join("out.aac");
    let result = Command::new(binary())
        .arg(fixture("stereo-48k-24.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("unsupported bit depth 24"));
    assert!(!output.exists());
}

#[test]
fn m4a_extension_selects_m4a_output() {
    let directory = TempDir::new("cli-m4a-extension");
    let output = directory.join("out.m4a");
    let result = Command::new(binary())
        .arg(fixture("stereo-48k-16.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("M4A"));
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[4..8], b"ftyp");
}

#[test]
fn explicit_format_overrides_extension() {
    let directory = TempDir::new("cli-format-override");
    let output = directory.join("out.m4a");
    let result = Command::new(binary())
        .arg("--format")
        .arg("adts")
        .arg(fixture("stereo-48k-16.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(bytes[0], 0xFF);
    assert_eq!(bytes[1] & 0xF0, 0xF0);
}

#[test]
fn explicit_m4a_format_can_use_any_extension() {
    let directory = TempDir::new("cli-format-m4a");
    let output = directory.join("out.aac");
    let result = Command::new(binary())
        .arg("--format=m4a")
        .arg(fixture("stereo-48k-16.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[4..8], b"ftyp");
}

#[test]
fn invalid_format_is_a_usage_error() {
    let directory = TempDir::new("cli-bad-format");
    let output = directory.join("out.m4a");
    let result = Command::new(binary())
        .arg("--format")
        .arg("wav")
        .arg(fixture("stereo-48k-16.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("invalid output format"));
    assert!(!output.exists());
}

#[test]
fn missing_format_value_is_a_usage_error() {
    let result = Command::new(binary()).arg("--format").output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--format requires"));
}
