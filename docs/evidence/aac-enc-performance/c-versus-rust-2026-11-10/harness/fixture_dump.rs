//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! Benchmark-only fixture materializer (test-only; no production behavior).
//!
//! Writes the exact 20-workload benchmark corpus (19 committed fixtures plus
//! the derived 4x music concatenation) as 16-bit WAV files so a C encoder
//! harness and a Rust harness can consume identical PCM. Uses the production
//! `testdata::fixture_samples` generator directly; no formula is duplicated.

#[cfg(test)]
mod fixture_dump_tests {
    use super::super::stream_driver::M58_FIXTURES;
    use super::super::testdata;
    use std::path::{Path, PathBuf};

    fn write_wav(path: &Path, pcm: &[Vec<i16>], sr: u32) {
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
        std::fs::write(path, &b).expect("write fixture wav");
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    fn pcm_bytes(pcm: &[Vec<i16>]) -> Vec<u8> {
        let mut out = Vec::with_capacity(pcm[0].len() * pcm.len() * 2);
        for i in 0..pcm[0].len() {
            for ch in pcm {
                out.extend_from_slice(&ch[i].to_le_bytes());
            }
        }
        out
    }

    fn dump_one(dir: &Path, stem: &str, sr: u32, pcm: &[Vec<i16>]) {
        let ch = pcm.len();
        let n = pcm[0].len();
        let path = dir.join(format!("{stem}.wav"));
        write_wav(&path, pcm, sr);
        let bytes = pcm_bytes(pcm);
        println!(
            "DUMP\t{stem}\t{sr}\t{ch}\t{n}\t{:016x}\t{}",
            fnv1a64(&bytes),
            path.display()
        );
    }

    #[test]
    fn dump_all_benchmark_fixtures() {
        let dir = PathBuf::from(std::env::var("FIXTURE_OUT").expect("FIXTURE_OUT"));
        std::fs::create_dir_all(&dir).expect("mkdir corpus");

        for (short, stem, sr, ch) in M58_FIXTURES {
            let pcm = testdata::fixture_samples(short);
            assert_eq!(pcm.len(), ch, "{stem}: channels");
            assert_eq!(testdata::fixture_sample_rate(short), sr, "{stem}: rate");
            dump_one(&dir, stem, sr, &pcm);
        }

        let music = testdata::fixture_samples("music-stereo");
        let mut pcm = vec![Vec::new(); music.len()];
        for _ in 0..4 {
            for (dst, src) in pcm.iter_mut().zip(&music) {
                dst.extend_from_slice(src);
            }
        }
        dump_one(&dir, "m7-music-stereo-48k-x4", 48_000, &pcm);
    }
}
