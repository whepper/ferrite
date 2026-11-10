//! Minimal production integration example for the `ferrite-aac` encoder.
//!
//! Generates three seconds of stereo 48 kHz PCM, feeds it in bounded chunks,
//! and writes the resulting ADTS stream to stdout:
//!
//! ```sh
//! cargo run --release --example aac_encode_adts > example.aac
//! ffplay example.aac
//! ```
//!
//! The example uses only the public API and writes each completed frame
//! immediately, so it never retains the encoded stream in memory.

use std::error::Error;
use std::f64::consts::PI;
use std::io::{self, Write};

use ferrite_aac::aac_enc::config::{EncoderConfig, TransportType, FRAME_LENGTH};
use ferrite_aac::aac_enc::encoder::Encoder;

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u32 = 2;
const DURATION_SECONDS: usize = 3;
/// A bounded chunk keeps peak memory flat. Buffer work is linear for any
/// channel-aligned chunk size, including a whole-file push.
const CHUNK_FRAMES: usize = 8_000;

fn pcm_chunk(start_frame: usize, frames: usize) -> Vec<i16> {
    let mut pcm = Vec::with_capacity(frames * CHANNELS as usize);
    for offset in 0..frames {
        let t = (start_frame + offset) as f64;
        let phase = 2.0 * PI * 440.0 * t / SAMPLE_RATE as f64;
        let left = (phase.sin() * 12_000.0) as i16;
        let right = (phase.sin() * 6_000.0) as i16;
        pcm.push(left);
        pcm.push(right);
    }
    pcm
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = EncoderConfig::new(SAMPLE_RATE, CHANNELS, FRAME_LENGTH, TransportType::Adts)?;
    let mut encoder = Encoder::new(config)?;

    let stdout = io::stdout();
    let mut output = stdout.lock();
    let total_frames = SAMPLE_RATE as usize * DURATION_SECONDS;
    let mut position = 0;
    let mut encoded_frames = 0u64;
    let mut encoded_bytes = 0u64;

    while position < total_frames {
        let frames = (total_frames - position).min(CHUNK_FRAMES);
        let pcm = pcm_chunk(position, frames);
        for frame in encoder.push(&pcm)? {
            output.write_all(&frame.bytes)?;
            encoded_frames += 1;
            encoded_bytes += frame.bytes.len() as u64;
        }
        position += frames;
    }

    // Flush exactly once. It emits the padded partial frame and the encoder
    // delay, after which the stream is complete and `is_finished()` is true.
    for frame in encoder.flush()? {
        output.write_all(&frame.bytes)?;
        encoded_frames += 1;
        encoded_bytes += frame.bytes.len() as u64;
    }
    output.flush()?;

    if !encoder.is_finished() {
        return Err("encoder did not finish after a successful flush".into());
    }
    eprintln!("wrote {encoded_frames} ADTS frames ({encoded_bytes} bytes) to stdout");
    Ok(())
}
