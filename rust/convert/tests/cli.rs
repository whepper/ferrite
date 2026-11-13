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
    assert!(
        stdout.starts_with(&format!("aac-convert {}\n", env!("CARGO_PKG_VERSION"))),
        "help header must carry the application name and version: {stdout}"
    );
    assert!(output.stderr.is_empty(), "help is requested output");
    assert!(
        stdout.contains("Usage: aac-convert [--format adts|m4a] [--vbr 1|2|3|4|5|6]"),
        "{stdout}"
    );
    assert!(stdout.contains("--no-tags"));
    assert!(stdout.contains("--jobs N"));
    assert!(stdout.contains("--vbr"));
    assert!(stdout.contains("VBR-6"));
    assert!(stdout.contains("16-bit"));
    assert!(stdout.contains("--format m4a"));
    assert!(stdout.contains("--cover-art embedded"));
    assert!(stdout.contains("--cover-art external"));
    assert!(stdout.contains("--cover-art both"));
}

#[test]
fn version_reports_package_version_on_stderr() {
    for flag in ["--version", "-V"] {
        let output = Command::new(binary()).arg(flag).output().unwrap();
        assert!(output.status.success(), "{flag} must succeed");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!("aac-convert {}\n", env!("CARGO_PKG_VERSION")),
            "{flag}"
        );
        assert!(
            output.stdout.is_empty(),
            "version output must never pollute stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
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
    assert!(
        !stdout.contains(&format!("aac-convert {}", env!("CARGO_PKG_VERSION"))),
        "the version banner must stay off stdout: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert_eq!(
        stderr,
        format!("aac-convert {}\n", env!("CARGO_PKG_VERSION")),
        "normal execution identifies itself on stderr"
    );
    assert!(output.is_file());
}

#[test]
fn conversion_failure_is_nonzero_and_actionable() {
    let directory = TempDir::new("cli-failure");
    let output = directory.join("out.aac");
    let input = fixture("three-channel-48k-16.flac");
    let result = Command::new(binary())
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains(&format!("error: {}:", input.display())),
        "stderr must identify the input: {stderr}"
    );
    assert!(stderr.contains("unsupported channel count 3"));
    assert!(!output.exists());
}

#[test]
fn accepts_24_bit_with_single_precision_warning() {
    let directory = TempDir::new("cli-24bit");
    let output = directory.join("out.aac");
    let result = Command::new(binary())
        .arg(fixture("stereo-48k-24.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    // ADTS carries no tags: just the startup banner and the precision notice
    // (`warning: <input>: <message>` per CLI convention).
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.starts_with(&format!("aac-convert {}\n", env!("CARGO_PKG_VERSION"))),
        "{stderr}"
    );
    let warnings: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("warning: "))
        .collect();
    assert_eq!(warnings.len(), 1, "{stderr}");
    assert!(
        stderr.contains(
            "24-bit FLAC input is truncated to 16-bit PCM (arithmetic right shift, no dithering) \
             before AAC encoding; low-order source precision is discarded"
        ),
        "{stderr}"
    );
    assert!(output.is_file());
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

#[test]
fn missing_cover_art_value_is_a_usage_error() {
    let result = Command::new(binary()).arg("--cover-art").output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--cover-art requires"));
}

#[test]
fn invalid_cover_art_value_is_a_usage_error() {
    let directory = TempDir::new("cli-bad-cover-art");
    let output = directory.join("out.m4a");
    let result = Command::new(binary())
        .arg("--cover-art")
        .arg("sidecar")
        .arg(fixture("stereo-48k-16.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("invalid cover-art mode"));
    assert!(!output.exists());
}

#[test]
fn default_vbr_is_six_and_reported() {
    let directory = TempDir::new("cli-vbr-default");
    let output = directory.join("out.aac");
    let result = Command::new(binary())
        .arg(fixture("stereo-48k-16.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert!(result.status.success());
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("VBR-6"), "{stdout}");
    assert!(stdout.contains("20000 Hz"), "{stdout}");
    assert!(output.is_file());
}

#[test]
fn explicit_vbr_selection_works_for_all_profiles() {
    for vbr in ["1", "2", "3", "4", "5", "6"] {
        for (fixture_name, mono) in [("stereo-48k-16.flac", false), ("mono-441k-16.flac", false)] {
            let _ = mono;
            let directory = TempDir::new(&format!("cli-vbr-{vbr}"));
            let output = directory.join("out.aac");
            let result = Command::new(binary())
                .arg("--vbr")
                .arg(vbr)
                .arg(fixture(fixture_name))
                .arg(&output)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "vbr {vbr} {fixture_name}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let stdout = String::from_utf8_lossy(&result.stdout);
            assert!(stdout.contains(&format!("VBR-{vbr}")), "{stdout}");
            assert!(output.is_file());
        }
    }
}

#[test]
fn explicit_vbr_equals_form_reports_profile() {
    let directory = TempDir::new("cli-vbr-eq");
    let output = directory.join("out.aac");
    let result = Command::new(binary())
        .arg("--vbr=5")
        .arg(fixture("stereo-48k-16.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("VBR-5"));
}

#[test]
fn invalid_vbr_is_a_usage_error() {
    for bad in ["0", "7", "vbr", "high"] {
        let directory = TempDir::new("cli-bad-vbr");
        let output = directory.join("out.aac");
        let result = Command::new(binary())
            .arg("--vbr")
            .arg(bad)
            .arg(fixture("stereo-48k-16.flac"))
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2), "vbr {bad}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("invalid --vbr value"),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!output.exists());
    }
    // Missing value is also a usage error.
    let result = Command::new(binary()).arg("--vbr").output().unwrap();
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn vbr_mono_and_stereo_report_nominal_bitrates() {
    // Mono VBR-1 nominal 32 kbit/s; stereo VBR-1 nominal 40 kbit/s.
    let directory = TempDir::new("cli-vbr-mono");
    let output = directory.join("out.aac");
    let result = Command::new(binary())
        .arg("--vbr")
        .arg("1")
        .arg(fixture("mono-441k-16.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert!(result.status.success());
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("VBR-1"), "{stdout}");
    assert!(stdout.contains("32000 bit/s"), "{stdout}");

    let directory = TempDir::new("cli-vbr-stereo");
    let output = directory.join("out.aac");
    let result = Command::new(binary())
        .arg("--vbr")
        .arg("1")
        .arg(fixture("stereo-48k-16.flac"))
        .arg(&output)
        .output()
        .unwrap();
    assert!(result.status.success());
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("40000 bit/s"), "{stdout}");
}
