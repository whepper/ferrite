// Benchmark-only corpus runner for the production Rust `Encoder` API.
//
// This is a temporary measurement harness, not part of the encoder library.
// It consumes the same canonical WAV corpus as the C reference harness, uses
// the production `Encoder::new`/`push`/`flush` path, and prints one row per
// measurement so complete-corpus per-repetition sums can be formed.
//
// Timing boundary:
//   TIMED   = Encoder::new + all push/flush calls, including production
//             buffering, deinterleave, per-frame work, and ADTS assembly.
//   UNTIMED = WAV load/parse, process startup, output Vec allocation,
//             drop, ADTS validation, hashing, output file write.
//
// Usage:
//   bench_corpus manifest.tsv outdir reps warmup chunk_frames
//
// Manifest columns: stem<TAB>wav_path<TAB>expected_bytes<TAB>expected_hash_hex

use std::path::{Path, PathBuf};
use std::time::Instant;

use ferrite_aac::aac_enc::config::{EncoderConfig, TransportType, FRAME_LENGTH};
use ferrite_aac::aac_enc::encoder::Encoder;

const ENCODER_DELAY: usize = 1_600;

struct Fixture {
    stem: String,
    sample_rate: u32,
    channels: usize,
    pcm: Vec<i16>,
    samples: usize,
    expected_bytes: usize,
    expected_hash: u64,
}

struct Run {
    init_ns: u128,
    encode_ns: u128,
    frames: usize,
    bytes: usize,
    hash: u64,
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn le16(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([bytes[off], bytes[off + 1]])
}

fn le32(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

fn read_wav(path: &Path) -> Result<(u32, usize, Vec<i16>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(format!("{}: not a RIFF/WAVE file", path.display()));
    }
    let mut off = 12usize;
    let mut channels = 0usize;
    let mut sample_rate = 0u32;
    let mut bits = 0u16;
    let mut audio_format = 0u16;
    let mut data_off = 0usize;
    let mut data_len = 0usize;
    while off + 8 <= bytes.len() {
        let id = &bytes[off..off + 4];
        let size = le32(&bytes, off + 4) as usize;
        let body = off + 8;
        if body + size > bytes.len() {
            break;
        }
        if id == b"fmt " {
            if size < 16 {
                return Err(format!("{}: short fmt chunk", path.display()));
            }
            audio_format = le16(&bytes, body);
            channels = le16(&bytes, body + 2) as usize;
            sample_rate = le32(&bytes, body + 4);
            bits = le16(&bytes, body + 14);
        } else if id == b"data" {
            data_off = body;
            data_len = size;
        }
        off = body + size + (size & 1);
    }
    if audio_format != 1 || bits != 16 || (channels != 1 && channels != 2) {
        return Err(format!(
            "{}: unsupported format={audio_format} bits={bits} channels={channels}",
            path.display()
        ));
    }
    if data_len == 0 || data_off + data_len > bytes.len() {
        return Err(format!("{}: missing data chunk", path.display()));
    }
    if data_len % (channels * 2) != 0 {
        return Err(format!("{}: ragged PCM data", path.display()));
    }
    let pcm = bytes[data_off..data_off + data_len]
        .chunks_exact(2)
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect();
    Ok((sample_rate, channels, pcm))
}

fn check_adts(bytes: &[u8], want_frames: usize, label: &str) {
    let mut off = 0usize;
    let mut frames = 0usize;
    while off + 7 <= bytes.len() {
        assert_eq!(bytes[off], 0xFF, "{label}: sync high @ {off}");
        assert_eq!(bytes[off + 1] & 0xF0, 0xF0, "{label}: sync low @ {off}");
        let fl = ((bytes[off + 3] as usize & 0x03) << 11)
            | ((bytes[off + 4] as usize) << 3)
            | ((bytes[off + 5] as usize) >> 5);
        assert!(fl >= 7, "{label}: bad frame length @ {off}");
        assert!(off + fl <= bytes.len(), "{label}: truncated frame @ {off}");
        off += fl;
        frames += 1;
    }
    assert_eq!(off, bytes.len(), "{label}: trailing bytes");
    assert_eq!(frames, want_frames, "{label}: frame count");
}

fn encode_once(fixture: &Fixture, out: &mut Vec<u8>, chunk_frames: usize) -> Run {
    let t0 = Instant::now();
    let cfg = EncoderConfig::new(
        fixture.sample_rate,
        fixture.channels as u32,
        FRAME_LENGTH,
        TransportType::Adts,
    )
    .expect("benchmark config");
    let mut enc = Encoder::new(cfg).expect("Encoder::new");
    let init_ns = t0.elapsed().as_nanos();
    out.clear();
    let t1 = Instant::now();
    let chunk = chunk_frames * fixture.channels;
    let mut frames = 0usize;
    for chunk in fixture.pcm.chunks(chunk) {
        for frame in enc.push(chunk).expect("Encoder::push") {
            frames += 1;
            out.extend_from_slice(&frame.bytes);
        }
    }
    for frame in enc.flush().expect("Encoder::flush") {
        frames += 1;
        out.extend_from_slice(&frame.bytes);
    }
    let encode_ns = t1.elapsed().as_nanos();
    drop(enc);
    let want_frames = (fixture.samples + ENCODER_DELAY).div_ceil(FRAME_LENGTH);
    check_adts(out, want_frames, &fixture.stem);
    assert_eq!(out.len(), fixture.expected_bytes, "{}: byte total", fixture.stem);
    let hash = fnv1a64(out);
    assert_eq!(hash, fixture.expected_hash, "{}: ADTS hash", fixture.stem);
    Run {
        init_ns,
        encode_ns,
        frames,
        bytes: out.len(),
        hash,
    }
}

fn read_manifest(path: &Path) -> Vec<Fixture> {
    let text = std::fs::read_to_string(path).expect("manifest");
    let mut fixtures = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), 4, "manifest columns: {line}");
        let (sr, ch, pcm) = read_wav(Path::new(cols[1])).expect("wav");
        let samples = pcm.len() / ch;
        fixtures.push(Fixture {
            stem: cols[0].to_string(),
            sample_rate: sr,
            channels: ch,
            pcm,
            samples,
            expected_bytes: cols[2].parse().expect("expected bytes"),
            expected_hash: u64::from_str_radix(cols[3], 16).expect("expected hash"),
        });
    }
    fixtures
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 6, "usage: {} manifest outdir reps warmup chunk_frames", args[0]);
    let manifest = PathBuf::from(&args[1]);
    let outdir = PathBuf::from(&args[2]);
    let reps: usize = args[3].parse().expect("reps");
    let warmup: usize = args[4].parse().expect("warmup");
    let chunk_frames: usize = args[5].parse().expect("chunk_frames");
    assert!(reps >= 1 && chunk_frames >= 1);

    let fixtures = read_manifest(&manifest);
    println!(
        "CFG\tprofile={}\tarch={}\tos={}\treps={reps}\twarmup={warmup}\tchunk_frames={chunk_frames}\tfixtures={}",
        if cfg!(debug_assertions) { "debug" } else { "release" },
        std::env::consts::ARCH,
        std::env::consts::OS,
        fixtures.len()
    );

    let mut outputs: Vec<Vec<u8>> = fixtures
        .iter()
        .map(|f| Vec::with_capacity(f.expected_bytes + 8192))
        .collect();

    for _ in 0..warmup {
        for (i, f) in fixtures.iter().enumerate() {
            let run = encode_once(f, &mut outputs[i], chunk_frames);
            println!(
                "WARM\t{}\t0\t{}\t{}\t{}\t{}\t{}",
                f.stem, run.init_ns, run.encode_ns, run.frames, run.bytes, run.hash
            );
        }
    }

    for rep in 0..reps {
        for (i, f) in fixtures.iter().enumerate() {
            let run = encode_once(f, &mut outputs[i], chunk_frames);
            println!(
                "ROW\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:016x}",
                f.stem,
                rep,
                f.sample_rate,
                f.channels,
                f.samples,
                run.frames,
                run.bytes,
                run.init_ns,
                run.encode_ns,
                run.hash
            );
            if rep == 0 {
                let in_hash = fnv1a64(
                    &f.pcm
                        .iter()
                        .flat_map(|s| s.to_le_bytes())
                        .collect::<Vec<u8>>(),
                );
                println!(
                    "CFG\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:016x}",
                    f.stem,
                    f.sample_rate,
                    f.channels,
                    f.samples,
                    run.frames,
                    run.bytes,
                    run.init_ns + run.encode_ns,
                    in_hash
                );
            }
        }
    }

    for (i, f) in fixtures.iter().enumerate() {
        let path = outdir.join(format!("{}.rust.aac", f.stem));
        std::fs::write(&path, &outputs[i]).expect("write output");
        println!(
            "VAL\t{}\tok\t{}\t{}\t{:016x}",
            f.stem,
            outputs[i].len() / 1,
            f.samples,
            fnv1a64(&outputs[i])
        );
    }
    println!("DONE\t{}", fixtures.len());
}
