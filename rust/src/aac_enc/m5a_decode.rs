//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! M5.a decode-and-check harness (test-only; validation, no quality judgment).
//!
//! Emits ADTS through the existing `stream_driver` test path and decodes each
//! stream with the prebuilt project `aac-dec` binary (path via `AAC_DEC_BIN`
//! env var; see `docs/aac-enc-m5a-decode-check.md` for the build command).
//!
//! Exit-code caution: `aac-dec` returns 0 even when it hits a mid-stream
//! `Fill`/`Decode` failure (it prints to stderr and breaks). This harness
//! therefore requires **both** a successful exit status **and** empty stderr,
//! plus structural WAV validation. Process failures can never read as success.

#[cfg(test)]
pub(crate) mod m5a_tests {
    // Selected decoder-harness helpers (`dec_bin`, `run_decode`,
    // `parse_wav`/`WavInfo`) are `pub(crate)` for reuse by the
    // crash-recovery decode regression in `stream_driver::m52_tests`.
    // The whole module stays `#[cfg(test)]`: no production exposure.
    use super::super::stream_driver;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    struct Case {
        short: &'static str,
        stem: &'static str,
        sr: u32,
        ch: usize,
    }

    const CASES: [Case; 4] = [
        Case { short: "click-mono", stem: "m2-click-mono-48k", sr: 48000, ch: 1 },
        Case { short: "onset-stereo", stem: "m2-onset-stereo-48k", sr: 48000, ch: 2 },
        Case { short: "sine-stereo-44100", stem: "m3-sine-stereo-441k", sr: 44100, ch: 2 },
        // M5.e: restored via committed cfg_44100_112000.txt + fixture key
        // "onset-mono-44100" (mono version of the onset-stereo-44100 formula).
        Case { short: "onset-mono-44100", stem: "m3-onset-mono-441k", sr: 44100, ch: 1 },
    ];

    fn out_dir() -> PathBuf {
        std::env::temp_dir().join("m5a-decode")
    }

    pub(crate) fn dec_bin() -> PathBuf {
        std::env::var("AAC_DEC_BIN").map(PathBuf::from).expect(
            "M5.a: AAC_DEC_BIN env var must point at a prebuilt aac-dec binary; \
             build with: cmake -S . -B /tmp/m5a-build -DBUILD_PROGRAMS=ON \
             -DCMAKE_BUILD_TYPE=Release && cmake --build /tmp/m5a-build \
             --target aac-dec -j 8",
        )
    }

    /// Walk ADTS headers; return per-frame lengths. Asserts sync, sane
    /// lengths, and no trailing bytes.
    fn split_lens(bytes: &[u8], stem: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut off = 0usize;
        while off + 7 <= bytes.len() {
            assert_eq!(bytes[off], 0xFF, "{stem}: sync @ {off}");
            assert_eq!(bytes[off + 1] & 0xF0, 0xF0, "{stem}: sync tail @ {off}");
            let fl = ((bytes[off + 3] as usize & 0x03) << 11)
                | ((bytes[off + 4] as usize) << 3)
                | ((bytes[off + 5] as usize) >> 5);
            assert!(fl >= 7, "{stem}: frame len @ {off}");
            out.push(fl);
            off += fl;
        }
        assert_eq!(off, bytes.len(), "{stem}: no trailing bytes");
        out
    }

    fn u16le(b: &[u8], o: usize) -> u16 {
        u16::from_le_bytes([b[o], b[o + 1]])
    }

    fn u32le(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }

    pub(crate) struct WavInfo {
        pub(crate) ch: u16,
        pub(crate) sr: u32,
        pub(crate) bits: u16,
        pub(crate) data_len: u32,
    }

    pub(crate) fn parse_wav(bytes: &[u8], stem: &str) -> (WavInfo, usize) {
        assert!(bytes.len() >= 44, "{stem}: wav too short");
        assert_eq!(&bytes[0..4], b"RIFF", "{stem}: RIFF");
        assert_eq!(&bytes[8..12], b"WAVE", "{stem}: WAVE");
        assert_eq!(&bytes[12..16], b"fmt ", "{stem}: fmt ");
        let info = WavInfo { ch: u16le(bytes, 22), sr: u32le(bytes, 24), bits: u16le(bytes, 34), data_len: u32le(bytes, 40) };
        assert_eq!(&bytes[36..40], b"data", "{stem}: data chunk");
        assert_eq!(info.data_len as usize, bytes.len() - 44, "{stem}: data len matches file");
        (info, 44)
    }

    pub(crate) fn run_decode(bin: &Path, aac: &Path, wav: &Path, stem: &str, run: u32) -> Vec<u8> {
        let out = Command::new(bin)
            .arg(aac)
            .arg(wav)
            .output()
            .unwrap_or_else(|e| panic!("{stem} run {run}: failed to spawn decoder: {e}"));
        let stderr = String::from_utf8_lossy(&out.stderr);
        // aac-dec returns 0 even on mid-stream Fill/Decode failure, so a
        // clean exit status alone is NOT success: stderr must also be empty.
        assert!(out.status.success(), "{stem} run {run}: decoder exit {out:?} stderr={stderr}");
        assert!(stderr.is_empty(), "{stem} run {run}: decoder stderr non-empty: {stderr}");
        let wav_bytes = std::fs::read(wav).unwrap_or_else(|e| panic!("{stem} run {run}: wav unreadable: {e}"));
        assert!(!wav_bytes.is_empty(), "{stem} run {run}: empty wav file");
        wav_bytes
    }

    #[test]
    fn m5a_decode_check() {
        let bin = dec_bin();
        assert!(bin.is_file(), "M5.a: AAC_DEC_BIN={bin:?} is not a file");
        let dir = out_dir();
        std::fs::create_dir_all(&dir).expect("M5.a: mkdir out_dir");
        for c in CASES {
            let rep = stream_driver::m52_write_adts_stream(c.short, c.stem, c.sr, c.ch, &dir)
                .unwrap_or_else(|e| panic!("{}: encode failed: {e}", c.stem));

            // Encode-side checks: nonempty stream, framing parses exactly.
            assert!(rep.frames > 0, "{}: no frames emitted", c.stem);
            assert!(rep.bytes > 0, "{}: empty ADTS", c.stem);
            let aac_bytes = std::fs::read(&rep.path).expect("read back aac");
            let lens = split_lens(&aac_bytes, c.stem);
            assert_eq!(lens.len(), rep.frames, "{}: parsed frame count", c.stem);
            assert_eq!(lens.iter().sum::<usize>(), rep.bytes, "{}: lens sum", c.stem);
            assert_eq!(lens, rep.frame_lens, "{}: header lens match emit record", c.stem);

            // Decode twice for determinism.
            let wav1_path = dir.join(format!("{}.dec1.wav", c.stem));
            let wav2_path = dir.join(format!("{}.dec2.wav", c.stem));
            let wav1 = run_decode(&bin, &rep.path, &wav1_path, c.stem, 1);
            let wav2 = run_decode(&bin, &rep.path, &wav2_path, c.stem, 2);
            assert_eq!(wav1, wav2, "{}: repeated decode differs", c.stem);

            // Structural WAV checks.
            let (info, _) = parse_wav(&wav1, c.stem);
            assert_eq!(info.ch as usize, c.ch, "{}: channel count", c.stem);
            assert_eq!(info.sr, c.sr, "{}: sample rate", c.stem);
            assert_eq!(info.bits, 16, "{}: bits per sample", c.stem);
            assert!(info.data_len > 0, "{}: empty data chunk", c.stem);
            assert_eq!(info.data_len as usize % (2 * c.ch), 0, "{}: frame-aligned data", c.stem);
            let data = &wav1[44..];
            assert!(data.iter().any(|&b| b != 0), "{}: all-zero decoded output", c.stem);

            // Plausibility vs encoder framing: decoded samples/ch should be
            // total granted samples minus decoder front-trim (outputDelay).
            // The exact outputDelay is decoder-internal (prior 48 kHz
            // measurement: 1744); record the implied value, bound it loosely.
            let samples_per_ch = info.data_len as usize / (2 * c.ch);
            let granted = rep.frames * 1024;
            assert!(samples_per_ch < granted, "{}: decoded exceeds granted", c.stem);
            let implied_delay = granted - samples_per_ch;
            assert!(implied_delay > 0 && implied_delay < 4096,
                "{}: implied front-trim {implied_delay} outside plausible window", c.stem);

            println!("M5.a {stem}: aac_frames={} aac_bytes={} wav_bytes={} ch={} sr={} bits={} spc={granted}->{samples_per_ch} implied_trim={implied_delay} determinism=identical",
                rep.frames, rep.bytes, wav1.len(), info.ch, info.sr, info.bits, stem = c.stem);
        }
    }
}
