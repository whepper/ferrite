mod common;

use std::process::Command;

use aac_convert::convert_file;
use common::{fixture, TempDir};

#[test]
#[ignore = "requires ffmpeg and ffprobe on PATH"]
fn independent_decoder_accepts_the_adts_output() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("ffmpeg/ffprobe are not available; independent decoder check skipped");
        return;
    }

    let directory = TempDir::new("ffmpeg");
    let output = directory.join("out.aac");
    convert_file(&fixture("stereo-48k-16.flac"), &output).unwrap();

    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name,sample_rate,channels",
            "-of",
            "default=nw=1",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        probe.status.success(),
        "ffprobe stderr: {}",
        String::from_utf8_lossy(&probe.stderr)
    );
    let text = String::from_utf8_lossy(&probe.stdout);
    assert!(text.contains("codec_name=aac"), "{text}");
    assert!(text.contains("sample_rate=48000"), "{text}");
    assert!(text.contains("channels=2"), "{text}");

    let decode = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    assert!(
        decode.status.success(),
        "ffmpeg stderr: {}",
        String::from_utf8_lossy(&decode.stderr)
    );
    assert!(
        decode.stderr.is_empty(),
        "ffmpeg reported errors: {}",
        String::from_utf8_lossy(&decode.stderr)
    );
}

#[test]
#[ignore = "requires ffmpeg and ffprobe on PATH"]
fn independent_decoder_accepts_the_m4a_output() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("ffmpeg/ffprobe are not available; independent decoder check skipped");
        return;
    }

    let directory = TempDir::new("ffmpeg-m4a");
    for (input, name, rate, channels, frames) in [
        ("stereo-48k-16.flac", "stereo.m4a", 48_000, 2, 14),
        ("mono-441k-16.flac", "mono.m4a", 44_100, 1, 5),
    ] {
        let output = directory.join(name);
        aac_convert::convert_file_with_format(
            &fixture(input),
            &output,
            aac_convert::OutputFormat::M4a,
        )
        .unwrap();

        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_name,profile,sample_rate,channels,nb_frames",
                "-of",
                "default=nw=1",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            probe.status.success(),
            "ffprobe stderr: {}",
            String::from_utf8_lossy(&probe.stderr)
        );
        let text = String::from_utf8_lossy(&probe.stdout);
        assert!(text.contains("codec_name=aac"), "{text}");
        assert!(text.contains("profile=LC"), "{text}");
        assert!(text.contains(&format!("sample_rate={rate}")), "{text}");
        assert!(text.contains(&format!("channels={channels}")), "{text}");
        assert!(text.contains(&format!("nb_frames={frames}")), "{text}");

        let decode = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args(["-f", "null", "-"])
            .output()
            .unwrap();
        assert!(
            decode.status.success(),
            "ffmpeg stderr: {}",
            String::from_utf8_lossy(&decode.stderr)
        );
        assert!(
            decode.stderr.is_empty(),
            "ffmpeg reported errors: {}",
            String::from_utf8_lossy(&decode.stderr)
        );
    }
}
