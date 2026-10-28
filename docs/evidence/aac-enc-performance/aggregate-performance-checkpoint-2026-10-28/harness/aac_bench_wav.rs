// Benchmark-only example: production `Encoder` path on committed WAV fixtures.
//
// This binary is not part of the encoder library and does not change any
// production behavior. It exists so the public API can be timed in a normal
// `cargo run --release --example` build (no `cfg(test)` harness, no test-only
// failure-injection branch). It reads the four committed 16-bit PCM WAVs,
// encodes them through `Encoder::push`/`flush`, validates the committed byte
// totals and FNV-1a hashes, and prints per-repetition and median timings.
//
// Usage:
//   cargo run --release --locked --features aac_enc --example aac_bench_wav
// Env:
//   AAC_WAV_REPS   measured repetitions (default 15)
//   AAC_WAV_WARMUP warm-up passes (default 1)

use std::path::{Path, PathBuf};
use std::time::Instant;

use ferrite_aac::aac_enc::config::{EncoderConfig, TransportType, FRAME_LENGTH};
use ferrite_aac::aac_enc::encoder::Encoder;

const CHUNK_FRAMES: usize = 8_000;
const ENCODER_DELAY: usize = 1_600;

#[derive(Clone)]
struct Fixture {
    name: &'static str,
    sample_rate: u32,
    channels: usize,
    pcm: Vec<i16>,
    samples: usize,
    expected_bytes: Option<usize>,
    expected_hash: Option<u64>,
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

fn fixture(
    name: &'static str,
    file: &str,
    expected_bytes: Option<usize>,
    expected_hash: Option<u64>,
) -> Result<Fixture, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/aac_enc/testdata")
        .join(file);
    let (sample_rate, channels, pcm) = read_wav(&path)?;
    let samples = pcm.len() / channels;
    Ok(Fixture {
        name,
        sample_rate,
        channels,
        pcm,
        samples,
        expected_bytes,
        expected_hash,
    })
}

fn repeat_fixture(mut base: Fixture, factor: usize) -> Fixture {
    let mut pcm = Vec::with_capacity(base.pcm.len() * factor);
    for _ in 0..factor {
        pcm.extend_from_slice(&base.pcm);
    }
    base.name = "m7-music-stereo-48k-x4";
    base.pcm = pcm;
    base.samples *= factor;
    base.expected_bytes = None;
    base.expected_hash = None;
    base
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

fn encode_once(fixture: &Fixture) -> Run {
    let cfg = EncoderConfig::new(
        fixture.sample_rate,
        fixture.channels as u32,
        FRAME_LENGTH,
        TransportType::Adts,
    )
    .expect("benchmark config");
    let t0 = Instant::now();
    let mut enc = Encoder::new(cfg).expect("Encoder::new");
    let init_ns = t0.elapsed().as_nanos();
    let mut out = Vec::with_capacity(
        fixture
            .expected_bytes
            .unwrap_or(fixture.pcm.len() / 2 + 8192),
    );
    let chunk = CHUNK_FRAMES * fixture.channels;
    let t1 = Instant::now();
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
    let want_frames = (fixture.samples + ENCODER_DELAY).div_ceil(FRAME_LENGTH);
    check_adts(&out, want_frames, fixture.name);
    let hash = fnv1a64(&out);
    if let Some(want) = fixture.expected_bytes {
        assert_eq!(out.len(), want, "{}: byte total", fixture.name);
    }
    if let Some(want) = fixture.expected_hash {
        assert_eq!(hash, want, "{}: ADTS hash", fixture.name);
    }
    Run {
        init_ns,
        encode_ns,
        frames,
        bytes: out.len(),
        hash,
    }
}

fn median(mut values: Vec<u128>) -> f64 {
    values.sort_unstable();
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2] as f64
    } else {
        (values[n / 2 - 1] as f64 + values[n / 2] as f64) / 2.0
    }
}

fn main() {
    let reps: usize = std::env::var("AAC_WAV_REPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(15)
        .max(1);
    let warmups: usize = std::env::var("AAC_WAV_WARMUP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let mut fixtures = vec![
        fixture(
            "m7-music-stereo-48k",
            "m7-music-stereo-48k.wav",
            Some(250_926),
            Some(0x151e_37b9_67d5_2678),
        )
        .expect("music fixture"),
        fixture(
            "m7-speech-clean-stereo-48k",
            "m7-speech-clean-stereo-48k.wav",
            Some(127_900),
            Some(0xea31_83de_57cb_f39f),
        )
        .expect("speech-clean fixture"),
        fixture(
            "m7-speech-pauses-stereo-48k",
            "m7-speech-pauses-stereo-48k.wav",
            Some(125_818),
            Some(0xe647_71cc_5687_e56b),
        )
        .expect("speech-pauses fixture"),
        fixture(
            "m8-music-eroica-stereo-48k",
            "m8-music-eroica-stereo-48k.wav",
            Some(234_229),
            Some(0x3c40_aaae_9307_8019),
        )
        .expect("eroica fixture"),
    ];
    let long = repeat_fixture(fixtures[0].clone(), 4);
    fixtures.push(long);

    println!(
        "WAVBENCH_CONFIG\tprofile={}\tarch={}\tos={}\treps={reps}\twarmups={warmups}\tfixtures={}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        std::env::consts::ARCH,
        std::env::consts::OS,
        fixtures.len()
    );

    for _ in 0..warmups {
        for f in &fixtures {
            let _ = encode_once(f);
        }
    }

    let mut rows: Vec<(usize, usize, Run)> = Vec::new();
    let mut derived: Option<(usize, u64)> = None;
    for rep in 0..reps {
        for (index, f) in fixtures.iter().enumerate() {
            let run = encode_once(f);
            if f.expected_bytes.is_none() {
                match derived {
                    None => derived = Some((run.bytes, run.hash)),
                    Some((bytes, hash)) => {
                        assert_eq!(run.bytes, bytes, "{}: derived byte total", f.name);
                        assert_eq!(run.hash, hash, "{}: derived hash", f.name);
                    }
                }
            }
            println!(
                "WAVBENCH_ROW\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:016x}",
                f.name,
                rep,
                f.sample_rate,
                f.channels,
                f.samples,
                run.frames,
                run.bytes,
                run.init_ns,
                run.encode_ns,
                run.init_ns + run.encode_ns,
                run.hash
            );
            rows.push((index, rep, run));
        }
    }

    for (index, f) in fixtures.iter().enumerate() {
        let selected: Vec<&Run> = rows
            .iter()
            .filter(|(i, _, _)| *i == index)
            .map(|(_, _, r)| r)
            .collect();
        let init = selected.iter().map(|r| r.init_ns).collect::<Vec<_>>();
        let encode = selected.iter().map(|r| r.encode_ns).collect::<Vec<_>>();
        let total = selected
            .iter()
            .map(|r| r.init_ns + r.encode_ns)
            .collect::<Vec<_>>();
        let median_total = median(total);
        let audio_s = f.samples as f64 / f.sample_rate as f64;
        let rtf = median_total / 1e9 / audio_s;
        println!(
            "WAVBENCH_MEDIAN\t{}\t{}\t{}\t{}\t{:.6}\t{}\t{}\t{:.0}\t{:.0}\t{:.0}\t{:.6}\t{:.1}\t{:.0}\t{:016x}",
            f.name,
            f.sample_rate,
            f.channels,
            f.samples,
            audio_s,
            selected.first().map(|r| r.bytes).unwrap_or(0),
            selected.first().map(|r| r.frames).unwrap_or(0),
            median(init),
            median(encode),
            median_total,
            rtf,
            if rtf > 0.0 { 1.0 / rtf } else { f64::INFINITY },
            f.samples as f64 / (median_total / 1e9),
            selected.first().map(|r| r.hash).unwrap_or(0)
        );
    }
    println!("WAVBENCH_DONE");
}
