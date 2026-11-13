// M7 audit-only example: per-profile segmented-sine bandwidth probe.
// Encodes one stereo 48 kHz ADTS file per requested profile: consecutive
// 0.5 s pure-sine segments at 10k, 15k, 17k, 18k, 19k, 19.5k, 19.8k,
// 20.2k Hz (the last above the VBR-6 bandwidth). Decode-side analysis was
// done by a Python script kept outside the repository (see
// docs/aac-enc-m7-release-readiness.md).
// Usage: m7_audit_spectral <out-dir> <mode-number>...
use std::path::PathBuf;

use ferrite_aac::aac_enc::config::{EncoderConfig, TransportType, VbrMode, FRAME_LENGTH};
use ferrite_aac::aac_enc::encoder::Encoder;

const FREQS: [f64; 8] = [
    10_000.0, 15_000.0, 17_000.0, 18_000.0, 19_000.0, 19_500.0, 19_800.0, 20_200.0,
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_dir = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&out_dir).expect("mkdir");
    for arg in &args[2..] {
        // "<mode>[:rate]" — rate defaults to 48000.
        let (num, rate_str) = arg.split_once(':').unwrap_or((arg, "48000"));
        let n: u32 = num.parse().expect("mode number");
        let rate: u32 = rate_str.parse().expect("rate");
        let mode = VbrMode::from_number(n).expect("mode");
        let cfg = EncoderConfig::new_with_mode(rate, 2, FRAME_LENGTH, TransportType::Adts, mode)
            .expect("config");
        let mut enc = Encoder::new(cfg).expect("encoder");
        let mut bytes = Vec::new();
        let mut segments = Vec::new();
        let seg_len = (rate / 2) as usize;
        for (si, f) in FREQS.iter().enumerate() {
            let mut seg = Vec::with_capacity(seg_len * 2);
            for i in 0..seg_len {
                // Short fade-in/out per segment to avoid transients.
                let t = i as f64 / rate as f64;
                let fade =
                    ((i as f64 / 800.0).min(1.0)).min(((seg_len - i) as f64 / 800.0).min(1.0));
                let v = 0.5 * fade * (2.0 * std::f64::consts::PI * f * t).sin();
                let s = (v * 32767.0) as i16;
                seg.push(s);
                seg.push(s);
            }
            segments.push((si, seg.len() / 2));
            for chunk in seg.chunks(16_000) {
                for frame in enc.push(chunk).expect("push") {
                    bytes.extend_from_slice(&frame.bytes);
                }
            }
        }
        for frame in enc.flush().expect("flush") {
            bytes.extend_from_slice(&frame.bytes);
        }
        let path = out_dir.join(format!("spectral-{n}-{rate}.aac"));
        std::fs::write(&path, &bytes).expect("write");
        let mut offsets = String::new();
        let mut acc = 0usize;
        for (si, len) in &segments {
            offsets.push_str(&format!(";{si}:{acc}"));
            acc += len;
        }
        println!(
            "M7SPECTRAL\t{residue}\tfile={}",
            path.display(),
            residue = offsets
        );
    }
}
