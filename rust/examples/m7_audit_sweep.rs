// M7 audit-only example: profile × format sweep through the public API.
// Not part of the encoder library or CI; retained as M7 audit evidence.
// Prints machine-readable M7AUDIT_* lines: one per (profile, rate, channels,
// content) cell with ADTS validation results and frame-size statistics.

use std::path::{Path, PathBuf};

use ferrite_aac::aac_enc::config::{EncoderConfig, TransportType, VbrMode, FRAME_LENGTH};
use ferrite_aac::aac_enc::encoder::Encoder;

const CHUNK: usize = 8_000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn uniform(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn in_range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.uniform()
    }
}

/// Deterministic white-ish noise sample through a one-pole lowpass at `lp`.
fn noise(rng: &mut Rng, lp: f64, state: &mut f64) -> f64 {
    let w = rng.in_range(-1.0, 1.0);
    *state += lp * (w - *state);
    *state
}

/// Generate `seconds` of interleaved PCM at `rate` channels for `kind`.
fn pcm(kind: &str, rate: u32, channels: u32, seconds: f64) -> Vec<i16> {
    let n = (rate as f64 * seconds) as usize;
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ u64::from(rate) ^ u64::from(channels) ^ 0xA5);
    let mut out = Vec::with_capacity(n * channels as usize);
    let mut s1 = 0.0f64;
    let mut s2 = 0.0f64;
    for i in 0..n {
        let t = i as f64 / rate as f64;
        let mut l = 0.0f64;
        let mut r = 0.0f64;
        match kind {
            "speech" => {
                // Amplitude-modulated formant stack with pauses.
                let cycle = t % 1.7;
                let gate = if cycle < 0.9 { 1.0 } else { 0.0 };
                let a = 0.55 * gate * (0.7 + 0.3 * (2.0 * std::f64::consts::PI * 4.0 * t).sin());
                l = a
                    * (0.5 * (2.0 * std::f64::consts::PI * 140.0 * t).sin()
                        + 0.3 * (2.0 * std::f64::consts::PI * 1100.0 * t).sin()
                        + 0.2 * (2.0 * std::f64::consts::PI * 2600.0 * t).sin());
                r = 0.9 * l;
            }
            "sparse" => {
                // Plucked decaying impulses every ~0.4 s.
                let phase = t % 0.4;
                let decay = (-phase * 18.0).exp();
                l = 0.8 * decay * (2.0 * std::f64::consts::PI * 220.0 * t).sin()
                    + 0.25 * noise(&mut rng, 0.02, &mut s1);
                r = 0.8 * decay * (2.0 * std::f64::consts::PI * 330.0 * t).sin();
            }
            "dense" => {
                // Dense music: tonal stack + shaped noise.
                l = 0.35 * (2.0 * std::f64::consts::PI * 196.0 * t).sin()
                    + 0.25 * (2.0 * std::f64::consts::PI * 392.0 * t).sin()
                    + 0.18 * (2.0 * std::f64::consts::PI * 784.0 * t).sin()
                    + 0.22 * noise(&mut rng, 0.35, &mut s1);
                r = 0.35 * (2.0 * std::f64::consts::PI * 247.0 * t).sin()
                    + 0.25 * (2.0 * std::f64::consts::PI * 494.0 * t).sin()
                    + 0.18 * (2.0 * std::f64::consts::PI * 988.0 * t).sin()
                    + 0.22 * noise(&mut rng, 0.35, &mut s2);
            }
            "transient" => {
                // Click trains every 50 ms.
                let phase = t % 0.05;
                let click = if phase < 0.002 {
                    1.0 - phase / 0.002
                } else {
                    0.0
                };
                l = 0.9 * click + 0.1 * noise(&mut rng, 0.2, &mut s1);
                r = 0.9 * click + 0.1 * noise(&mut rng, 0.2, &mut s2);
            }
            "tonal" => {
                // Sustained chord, near-constant envelope.
                l = 0.3 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()
                    + 0.3 * (2.0 * std::f64::consts::PI * 554.37 * t).sin()
                    + 0.3 * (2.0 * std::f64::consts::PI * 659.26 * t).sin();
                r = l;
            }
            "silence" => {
                l = 0.0;
                r = 0.0;
            }
            "quietspeech" => {
                let a = 0.004;
                let gate = if (t % 1.7) < 0.9 { 1.0 } else { 0.0 };
                l = a * gate * (2.0 * std::f64::consts::PI * 300.0 * t).sin()
                    + a * 0.2 * noise(&mut rng, 0.1, &mut s1);
                r = l;
            }
            "hf" => {
                // High-frequency band-limited noise + 19.5 kHz tone (48k only).
                let hf_noise = noise(&mut rng, 0.95, &mut s1) - noise(&mut rng, 0.6, &mut s2);
                let tone = if rate == 48_000 {
                    0.3 * (2.0 * std::f64::consts::PI * 19_500.0 * t).sin()
                } else {
                    0.3 * (2.0 * std::f64::consts::PI * 19_000.0 * t).sin()
                };
                l = 0.25 * hf_noise + tone;
                r = l;
            }
            other => unreachable!("unknown content kind {other}"),
        }
        let (lc, rc) = (
            (l.clamp(-1.0, 1.0) * 32767.0) as i16,
            (r.clamp(-1.0, 1.0) * 32767.0) as i16,
        );
        out.push(lc);
        if channels == 2 {
            out.push(rc);
        }
    }
    out
}

/// Exhaustively validate one ADTS frame header; return (total_len, payload).
fn check_adts_header(bytes: &[u8], off: usize, rate: u32, channels: u32) -> (usize, usize) {
    let b = &bytes[off..off + 7];
    assert_eq!(b[0], 0xFF, "sync high @ {off}");
    assert_eq!(b[1] & 0xF0, 0xF0, "sync low @ {off}");
    assert_eq!(b[1] & 0x08, 0x00, "MPEG-4 ID @ {off}");
    assert_eq!(b[1] & 0x06, 0x00, "layer 0 @ {off}");
    assert_eq!(b[1] & 0x01, 0x01, "protection absent @ {off}");
    assert_eq!(b[2] >> 6, 1, "profile LC @ {off}");
    let sfi = ((b[2] >> 2) & 0x0F) as u32;
    let want_sfi = match rate {
        96000 => 0,
        88200 => 1,
        64000 => 2,
        48000 => 3,
        44100 => 4,
        32000 => 5,
        24000 => 6,
        22050 => 7,
        16000 => 8,
        12000 => 9,
        11025 => 10,
        8000 => 11,
        other => unreachable!("rate {other}"),
    };
    assert_eq!(sfi, want_sfi, "sample-rate index @ {off}");
    assert_eq!(b[2] & 0x02, 0x00, "private bit @ {off}");
    let chan_cfg = ((b[2] & 0x01) << 2) | (b[3] >> 6);
    let want_cc = channels; // mono=1, stereo=2
    assert_eq!(u32::from(chan_cfg), want_cc, "channel config @ {off}");
    assert_eq!(b[3] & 0x30, 0x00, "original/copyright @ {off}");
    assert_eq!(b[3] & 0x08, 0x00, "home bit @ {off}");
    assert_eq!(b[3] & 0x04, 0x00, "copyright id @ {off}");
    let len =
        ((usize::from(b[3]) & 0x03) << 11) | (usize::from(b[4]) << 3) | (usize::from(b[5]) >> 5);
    assert!(len >= 7, "frame length @ {off}");
    let fullness = ((u32::from(b[5]) & 0x1F) << 6) | u32::from(b[6]) >> 2;
    assert_eq!(fullness, 0x7FF, "VBR fullness @ {off}");
    assert_eq!(b[6] & 0x03, 0x00, "single raw block @ {off}");
    (len, len - 7)
}

struct Cell {
    mode: VbrMode,
    rate: u32,
    channels: u32,
    kind: &'static str,
}

fn encode_cell(cell: &Cell, pcm: &[i16], out_dir: Option<&Path>, tag: &str) -> Stats {
    let cfg = EncoderConfig::new_with_mode(
        cell.rate,
        cell.channels,
        FRAME_LENGTH,
        TransportType::Adts,
        cell.mode,
    )
    .expect("config");
    let mut enc = Encoder::new(cfg).expect("encoder");
    let mut bytes = Vec::new();
    let mut frame_sizes: Vec<usize> = Vec::new();
    let mut frame_index = 0u32;
    for chunk in pcm.chunks(CHUNK * cell.channels as usize) {
        for frame in enc.push(chunk).expect("push") {
            assert_eq!(frame.frame_index, frame_index, "frame index");
            frame_index += 1;
            bytes.extend_from_slice(&frame.bytes);
        }
    }
    for frame in enc.flush().expect("flush") {
        assert_eq!(frame.frame_index, frame_index, "flush frame index");
        frame_index += 1;
        bytes.extend_from_slice(&frame.bytes);
    }
    assert!(enc.is_finished());
    // Validate every header; collect stats.
    let mut off = 0usize;
    while off < bytes.len() {
        let (len, payload) = check_adts_header(&bytes, off, cell.rate, cell.channels);
        frame_sizes.push(len);
        off += len;
        let _ = payload;
    }
    assert_eq!(off, bytes.len(), "frame walk consumed all bytes");
    if let Some(dir) = out_dir {
        let name = format!("{tag}.aac");
        std::fs::write(dir.join(name), &bytes).expect("write aac");
    }
    Stats::from(&frame_sizes, cell)
}

struct Stats {
    frames: usize,
    bytes: usize,
    avg_bps: f64,
    min_frame: usize,
    max_frame: usize,
    median_frame: usize,
    p95_frame: usize,
    first_frame: usize,
    last_frame: usize,
    at_limit_frames: usize,
}

impl Stats {
    fn from(sizes: &[usize], cell: &Cell) -> Self {
        let bytes: usize = sizes.iter().sum();
        let seconds = sizes.len() as f64 * 1024.0 / cell.rate as f64;
        let mut sorted = sizes.to_vec();
        sorted.sort_unstable();
        let p95 = sorted[(0.95 * (sorted.len() as f64)) as usize % sorted.len()];
        let at_limit = sizes.iter().filter(|s| **s >= 8186).count();
        Stats {
            frames: sizes.len(),
            bytes,
            avg_bps: bytes as f64 * 8.0 / seconds,
            min_frame: sorted[0],
            max_frame: sorted[sorted.len() - 1],
            median_frame: sorted[sorted.len() / 2],
            p95_frame: p95,
            first_frame: sizes[0],
            last_frame: sizes[sizes.len() - 1],
            at_limit_frames: at_limit,
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_dir = args.get(1).map(PathBuf::from);
    if let Some(dir) = &out_dir {
        std::fs::create_dir_all(dir).expect("mkdir");
    }

    let rates = [44_100u32, 48_000];
    let chans = [1u32, 2];
    let kinds = [
        "speech",
        "sparse",
        "dense",
        "transient",
        "tonal",
        "silence",
        "quietspeech",
        "hf",
    ];

    let mut cells = Vec::new();
    for mode in VbrMode::ALL {
        for rate in rates {
            for channels in chans {
                for kind in kinds {
                    cells.push(Cell {
                        mode,
                        rate,
                        channels,
                        kind,
                    });
                }
            }
        }
    }

    println!(
        "M7AUDIT_CONFIG\tcells={}\trelease={}",
        cells.len(),
        !cfg!(debug_assertions)
    );

    // Long-material stats: 12 s for distributions. Full matrix encode.
    for cell in &cells {
        let seconds = if cell.kind == "silence" { 8.0 } else { 12.0 };
        let pcm = pcm(cell.kind, cell.rate, cell.channels, seconds);
        let tag = format!(
            "{}-{}-{}ch-{}",
            cell.mode.number(),
            cell.rate,
            cell.channels,
            cell.kind
        );
        let s = encode_cell(cell, &pcm, out_dir.as_deref(), &tag);
        println!(
            "M7AUDIT_ROW\t{tag}\tframes={}\tbytes={}\tavg_bps={:.0}\tmin={}\tmed={}\tp95={}\tmax={}\tfirst={}\tlast={}\tat_limit={}",
            s.frames,
            s.bytes,
            s.avg_bps,
            s.min_frame,
            s.median_frame,
            s.p95_frame,
            s.max_frame,
            s.first_frame,
            s.last_frame,
            s.at_limit_frames,
        );
    }

    // Committed production fixtures × all profiles (stereo 48k music/speech).
    let fixtures = [
        ("m7-music-stereo-48k.wav", 48_000, 2),
        ("m7-speech-clean-stereo-48k.wav", 48_000, 2),
        ("m7-speech-pauses-stereo-48k.wav", 48_000, 2),
        ("m8-music-eroica-stereo-48k.wav", 48_000, 2),
    ];
    for (file, rate, channels) in fixtures {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/aac_enc/testdata")
            .join(file);
        let raw = std::fs::read(&path).expect("fixture");
        // Minimal WAV walk (16-bit PCM).
        let mut off = 12usize;
        let mut data = &raw[0..0];
        while off + 8 <= raw.len() {
            let id = &raw[off..off + 4];
            let size = u32::from_le_bytes([raw[off + 4], raw[off + 5], raw[off + 6], raw[off + 7]])
                as usize;
            if id == b"data" {
                data = &raw[off + 8..off + 8 + size];
                break;
            }
            off += 8 + size + (size & 1);
        }
        let pcm: Vec<i16> = data
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        for mode in VbrMode::ALL {
            let cell = Cell {
                mode,
                rate,
                channels: channels as u32,
                kind: file,
            };
            let stem = file.trim_end_matches(".wav");
            let tag = format!("{}-{}", mode.number(), stem);
            let s = encode_cell(&cell, &pcm, out_dir.as_deref(), &tag);
            println!(
                "M7AUDIT_ROW\t{tag}\tframes={}\tbytes={}\tavg_bps={:.0}\tmin={}\tmed={}\tp95={}\tmax={}\tfirst={}\tlast={}\tat_limit={}",
                s.frames,
                s.bytes,
                s.avg_bps,
                s.min_frame,
                s.median_frame,
                s.p95_frame,
                s.max_frame,
                s.first_frame,
                s.last_frame,
                s.at_limit_frames,
            );
        }
    }
    println!("M7AUDIT_DONE");
}
