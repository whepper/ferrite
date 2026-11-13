//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! M5.b input-dump helper (test-only; measurement support, no quality judgment).
//!
//! Writes the three M5.b original-PCM fixtures as 16-bit WAV files under
//! `$TMPDIR/m5b-quality/inputs/` so the C reference encoder (`aac-enc`),
//! the decoder (`aac-dec`), and the offline metrics script can consume the
//! exact same bytes the Rust encoder path uses. Deterministic resynthesis;
//! byte-identical output on every run.

#[cfg(test)]
mod m5b_tests {
    use super::super::testdata;
    use std::path::PathBuf;

    const INPUTS: [(&str, u32, usize); 4] = [
        ("click-mono", 48000, 1),
        ("onset-stereo", 48000, 2),
        ("sine-stereo-44100", 44100, 2),
        ("onset-mono-44100", 44100, 1),
    ];

    fn write_wav(path: &std::path::Path, pcm: &[Vec<i16>], sr: u32) {
        let ch = pcm.len();
        let n = pcm[0].len();
        let data_len = (n * ch * 2) as u32;
        let mut b = Vec::with_capacity(44 + data_len as usize);
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&(ch as u16).to_le_bytes());
        b.extend_from_slice(&sr.to_le_bytes());
        b.extend_from_slice(&(sr * ch as u32 * 2).to_le_bytes());
        b.extend_from_slice(&(ch as u16 * 2).to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..n {
            for c in 0..ch {
                b.extend_from_slice(&pcm[c][i].to_le_bytes());
            }
        }
        std::fs::write(path, &b).expect("write input wav");
    }

    #[test]
    fn m5b_dump_inputs() {
        let dir: PathBuf = std::env::temp_dir().join("m5b-quality").join("inputs");
        std::fs::create_dir_all(&dir).expect("mkdir inputs");
        for (short, sr, ch) in INPUTS {
            let pcm = testdata::fixture_samples(short);
            assert_eq!(pcm.len(), ch, "{short}: channels");
            assert_eq!(testdata::fixture_sample_rate(short), sr, "{short}: rate");
            let path = dir.join(format!("{short}.orig.wav"));
            write_wav(&path, &pcm, sr);
            let peak = pcm
                .iter()
                .flat_map(|v| v.iter())
                .map(|s| s.unsigned_abs() as u32)
                .max()
                .unwrap_or(0);
            println!(
                "M5.b input {short}: samples/ch={} ch={ch} sr={sr} peak={peak}",
                pcm[0].len()
            );
        }
    }
}
