/* -----------------------------------------------------------------------------
Software License for The Fraunhofer FDK AAC Codec Library for Android

© Copyright 2025 Fraunhofer-Gesellschaft zur Förderung der angewandten Forschung
e.V. All rights reserved.

 1.    INTRODUCTION
The Fraunhofer FDK AAC Codec Library for Android ("FDK AAC Codec") is software
that implements the MPEG Advanced Audio Coding ("AAC") encoding and decoding
scheme for digital audio. This FDK AAC Codec software is intended to be used on
a wide variety of Android devices.

AAC's HE-AAC and HE-AAC v2 versions are regarded as today's most efficient
general perceptual audio codecs. AAC-ELD is considered the best-performing
full-bandwidth communications codec by independent studies and is widely
deployed. AAC has been standardized by ISO and IEC as part of the MPEG
specifications.

Patent licenses for necessary patent claims for the FDK AAC Codec (including
those of Fraunhofer) may be obtained through Via Licensing
(www.vialicensing.com) or through the respective patent owners individually for
the purpose of encoding or decoding bit streams in products that are compliant
with the ISO/IEC MPEG audio standards. Please note that most manufacturers of
Android devices already license these patent claims through Via Licensing or
directly from the patent owners, and therefore FDK AAC Codec software may
already be covered under those patent licenses when it is used for those
licensed purposes only.

Commercially-licensed AAC software libraries, including floating-point versions
with enhanced sound quality, are also available from Fraunhofer. Users are
encouraged to check the Fraunhofer website for additional applications
information and documentation.

2.    COPYRIGHT LICENSE

Redistribution and use in source and binary forms, with or without modification,
are permitted without payment of copyright license fees provided that you
satisfy the following conditions:

You must retain the complete text of this software license in redistributions of
the FDK AAC Codec or your modifications thereto in source code form.

You must retain the complete text of this software license in the documentation
and/or other materials provided with redistributions of the FDK AAC Codec or
your modifications thereto in binary form. You must make available free of
charge copies of the complete source code of the FDK AAC Codec and your
modifications thereto to recipients of copies in binary form.

The name of Fraunhofer may not be used to endorse or promote products derived
from this library without prior written permission.

You may not charge copyright license fees for anyone to use, copy or distribute
the FDK AAC Codec software or your modifications thereto.

Your modified versions of the FDK AAC Codec must carry prominent notices stating
that you changed the software and the date of any change. For modified versions
of the FDK AAC Codec, the term "Fraunhofer FDK AAC Codec Library for Android"
must be replaced by the term "Third-Party Modified Version of the Fraunhofer FDK
AAC Codec Library for Android."

3.    NO PATENT LICENSE

NO EXPRESS OR IMPLIED LICENSES TO ANY PATENT CLAIMS, including without
limitation the patents of Fraunhofer, ARE GRANTED BY THIS SOFTWARE LICENSE.
Fraunhofer provides no warranty of patent non-infringement with respect to this
software.

You may use this FDK AAC Codec software or modifications thereto only for
purposes that are authorized by appropriate patent licenses.

4.    DISCLAIMER

This FDK AAC Codec software is provided by Fraunhofer on behalf of the copyright
holders and contributors "AS IS" and WITHOUT ANY EXPRESS OR IMPLIED WARRANTIES,
including but not limited to the implied warranties of merchantability and
fitness for a particular purpose. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
CONTRIBUTORS BE LIABLE for any direct, indirect, incidental, special, exemplary,
or consequential damages, including but not limited to procurement of substitute
goods or services; loss of use, data, or profits, or business interruption,
however caused and on any theory of liability, whether in contract, strict
liability, or tort (including negligence), arising in any way out of the use of
this software, even if advised of the possibility of such damage.

5.    CONTACT INFORMATION

Fraunhofer Institute for Integrated Circuits IIS
Attention: Audio and Multimedia Departments - FDK AAC LL
Am Wolfsmantel 33
91058 Erlangen, Germany

www.iis.fraunhofer.de/amm
amm-info@iis.fraunhofer.de
----------------------------------------------------------------------------- */
//! Buffered PCM framing and encoder lifecycle.
//!
//! Mirrors the `aacenc_lib.cpp` input stage (`aacEncEncode`, lines 1045-1120):
//! interleaved 16-bit PCM is deinterleaved into per-channel buffers
//! (`FDK_deinterleave`), full 1024-sample frames are drained as they complete,
//! and end-of-stream pads the tail with zeros until the encoder delay is
//! covered, then signals end-of-stream (`AACENC_ENCODE_EOF`).
//!
//! Input conventions (from `aac-enc.c` and `aacenc_lib.cpp`):
//! - Samples are 16-bit little-endian PCM in WAV channel order
//!   (`AACENC_CHANNELORDER = 1`); each `push_interleaved` slice holds an
//!   integral number of multi-channel samples (interleaved).
//! - The C API rounds a ragged push down to a channel multiple and reports
//!   how many samples it consumed (`aacenc_lib.cpp:1052-1062`). This API
//!   instead rejects ragged input with [`FeederError::UnalignedInput`]:
//!   silently dropping trailing samples is worse than an explicit error, and
//!   callers control chunking.
//! - The C `-1` flush convention (`inargs->numInSamples == -1`) is not a
//!   sample count at all: it means "no more input will arrive; pad the
//!   current partial frame with zeros and keep emitting frames until the
//!   appended zeros reach the encoder delay (1600 samples), then report
//!   end-of-stream". Here that is [`PcmFeeder::finish`], which returns every
//!   remaining frame at once.
//!
//! Delay accounting: for `S` input samples per channel the feeder emits
//! exactly `ceil((S + delay) / 1024)` frames, i.e. the smallest frame count
//! whose zero padding (`frames * 1024 - S`) reaches the 1600-sample delay.
//! Verified against the reference: 48000 stereo samples produce 49 ADTS
//! frames, 9600 mono samples produce 11. The first 1600 output samples are
//! filterbank/look-ahead priming, not signal: decoders skip `nDelay`
//! samples (`test-encode-decode.c`).

use super::config::{EncoderConfig, FRAME_LENGTH};
use core::fmt;

/// One encoder input frame: [`FRAME_LENGTH`](super::config::FRAME_LENGTH)
/// deinterleaved samples per channel, in channel order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmFrame {
    channels: Vec<[i16; FRAME_LENGTH]>,
}

impl PcmFrame {
    /// Per-channel frame data, in channel order.
    pub fn channels(&self) -> &[[i16; FRAME_LENGTH]] {
        &self.channels
    }

    /// Number of channels in this frame.
    #[cfg(test)]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

/// Feed usage errors. All are reported via `Result`, never panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeederError {
    /// Input length is not a multiple of the channel count, so it does not
    /// hold whole multi-channel samples.
    UnalignedInput {
        /// Length of the rejected slice, in samples.
        len: usize,
        /// Configured channel count.
        channels: u32,
    },
    /// Samples pushed after [`PcmFeeder::finish`], or `finish` called twice.
    /// The feeder is single-use: create a new one for the next stream.
    AlreadyFinished,
}

impl fmt::Display for FeederError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeederError::UnalignedInput { len, channels } => {
                write!(
                    f,
                    "input length {len} is not a multiple of {channels} channels"
                )
            }
            FeederError::AlreadyFinished => write!(f, "feeder already finished"),
        }
    }
}

impl std::error::Error for FeederError {}

/// Accumulates interleaved PCM and emits complete input frames.
///
/// Single-use lifecycle: any number of [`push_interleaved`](Self::push_interleaved)
/// calls, then exactly one [`finish`](Self::finish). Independent of the MDCT.
pub struct PcmFeeder {
    channels: u32,
    delay: usize,
    pending: Vec<Vec<i16>>,
    emitted_frames: u64,
    zeros_appended: u64,
    finished: bool,
}

impl PcmFeeder {
    /// Create a feeder for `config`. Buffers start empty, mirroring the
    /// cleared `inputBuffer` after `aacEncInit` (`aacenc_lib.cpp:751`).
    pub fn new(config: EncoderConfig) -> Self {
        let channels = config.channels() as usize;
        Self {
            channels: config.channels(),
            delay: config.encoder_delay(),
            pending: vec![Vec::new(); channels],
            emitted_frames: 0,
            zeros_appended: 0,
            finished: false,
        }
    }

    /// Encoder delay in samples per channel (1600 for 1024-sample frames).
    #[cfg(test)]
    pub fn delay(&self) -> usize {
        self.delay
    }

    /// Buffered samples per channel not yet emitted as a frame.
    #[cfg(test)]
    pub fn pending_per_channel(&self) -> usize {
        self.pending[0].len()
    }

    /// Frames emitted so far (by `push_interleaved` and `finish`).
    pub fn emitted_frames(&self) -> u64 {
        self.emitted_frames
    }

    /// Zero samples appended during `finish`, per channel.
    #[cfg(test)]
    pub fn zeros_appended(&self) -> u64 {
        self.zeros_appended
    }

    /// Whether `finish` has been called.
    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Feed interleaved samples. Returns the complete frames this chunk
    /// completes, in order; an empty vector means the chunk did not fill a
    /// frame. A zero-length slice is a no-op returning no frames.
    pub fn push_interleaved(&mut self, samples: &[i16]) -> Result<Vec<PcmFrame>, FeederError> {
        if self.finished {
            return Err(FeederError::AlreadyFinished);
        }
        if samples.len() % self.channels as usize != 0 {
            return Err(FeederError::UnalignedInput {
                len: samples.len(),
                channels: self.channels,
            });
        }
        for (i, sample) in samples.iter().enumerate() {
            self.pending[i % self.channels as usize].push(*sample);
        }
        Ok(self.drain_full_frames())
    }

    /// End of stream: emit the padded partial frame (if any samples are
    /// pending) plus all-zero frames until the appended zeros reach the
    /// encoder delay, mirroring the `-1` flush loop
    /// (`aacenc_lib.cpp:1091-1115`). Marks the feeder finished.
    pub fn finish(&mut self) -> Result<Vec<PcmFrame>, FeederError> {
        if self.finished {
            return Err(FeederError::AlreadyFinished);
        }
        self.finished = true;
        let mut frames = Vec::new();
        while self.zeros_appended < self.delay as u64 {
            // All channels always hold equal pending lengths (pushes are
            // channel-aligned, drains uniform), so the zero count is shared.
            let take = self.pending[0].len().min(FRAME_LENGTH);
            let mut frame = Vec::with_capacity(self.channels as usize);
            for pending in self.pending.iter_mut() {
                let mut block = [0i16; FRAME_LENGTH];
                block[..take].copy_from_slice(&pending[..take]);
                pending.drain(..take);
                frame.push(block);
            }
            self.zeros_appended += (FRAME_LENGTH - take) as u64;
            self.emitted_frames += 1;
            frames.push(PcmFrame { channels: frame });
        }
        Ok(frames)
    }

    /// Drain every pending complete frame, in order.
    fn drain_full_frames(&mut self) -> Vec<PcmFrame> {
        let mut frames = Vec::new();
        while self.pending[0].len() >= FRAME_LENGTH {
            let mut frame = Vec::with_capacity(self.channels as usize);
            for pending in self.pending.iter_mut() {
                let mut block = [0i16; FRAME_LENGTH];
                block.copy_from_slice(&pending[..FRAME_LENGTH]);
                pending.drain(..FRAME_LENGTH);
                frame.push(block);
            }
            self.emitted_frames += 1;
            frames.push(PcmFrame { channels: frame });
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::config::TransportType;

    fn stereo_48k() -> EncoderConfig {
        EncoderConfig::stereo_48k_adts()
    }

    fn mono_48k() -> EncoderConfig {
        EncoderConfig::new(48000, 1, FRAME_LENGTH, TransportType::Adts).unwrap()
    }

    /// Deterministic pseudo-signal with no periodicity the tests depend on.
    fn pattern(t: usize, ch: usize) -> i16 {
        (((t.wrapping_mul(1103515245).wrapping_add(12345 + ch * 7919)) >> 16) & 0x7FFF) as i16
            - 0x1000
    }

    fn interleave(channels: usize, per_ch: usize, f: impl Fn(usize, usize) -> i16) -> Vec<i16> {
        let mut out = Vec::with_capacity(channels * per_ch);
        for t in 0..per_ch {
            for c in 0..channels {
                out.push(f(t, c));
            }
        }
        out
    }

    /// Feed `total` samples per channel in the given per-channel chunk
    /// sizes (cycled, scaled by the channel count so every push holds
    /// whole multi-channel samples), then finish. Returns all frames in
    /// order.
    fn feed_chunks(
        config: EncoderConfig,
        total: usize,
        chunks: &[usize],
    ) -> (PcmFeeder, Vec<PcmFrame>) {
        assert!(!chunks.is_empty() && chunks.iter().all(|&c| c > 0));
        let channels = config.channels() as usize;
        let input = interleave(channels, total, pattern);
        let mut feeder = PcmFeeder::new(config);
        let mut frames = Vec::new();
        let mut ci = 0;
        let mut pos = 0;
        while pos < input.len() {
            let take = (chunks[ci % chunks.len()] * channels).min(input.len() - pos);
            assert!(take > 0, "chunking stalled");
            frames.extend(feeder.push_interleaved(&input[pos..pos + take]).unwrap());
            pos += take;
            ci += 1;
        }
        frames.extend(feeder.finish().unwrap());
        (feeder, frames)
    }

    /// Flatten emitted frames back to per-channel sample vectors.
    fn flatten(frames: &[PcmFrame], channels: usize) -> Vec<Vec<i16>> {
        let mut out = vec![Vec::new(); channels];
        for frame in frames {
            assert_eq!(frame.channel_count(), channels);
            for (c, block) in frame.channels().iter().enumerate() {
                out[c].extend_from_slice(block);
            }
        }
        out
    }

    #[test]
    fn irregular_chunks_preserve_samples_and_order() {
        for chunks in [
            &[1usize][..],
            &[3, 7, 2000][..],
            &[1023, 1025][..],
            &[5000][..],
        ] {
            let (feeder, frames) = feed_chunks(stereo_48k(), 5000, chunks);
            let flat = flatten(&frames, 2);
            for (c, chan) in flat.iter().enumerate() {
                assert_eq!(chan.len(), frames.len() * FRAME_LENGTH);
                for (t, &v) in chan.iter().take(5000).enumerate() {
                    assert_eq!(v, pattern(t, c), "chunks {chunks:?} ch {c} t {t}");
                }
                for (i, &v) in chan.iter().skip(5000).enumerate() {
                    assert_eq!(
                        v,
                        0,
                        "tail not zero-padded at {} (chunks {chunks:?})",
                        5000 + i
                    );
                }
            }
            // Minimal padding: zeros in [delay, delay + 1024).
            let zeros = (flat[0].len() - 5000) as u64;
            assert!(zeros >= feeder.delay() as u64, "chunks {chunks:?}");
            assert!(zeros < feeder.delay() as u64 + FRAME_LENGTH as u64);
            assert_eq!(feeder.zeros_appended(), zeros);
            assert!(feeder.is_finished());
        }
    }

    #[test]
    fn frame_count_matches_reference_formula() {
        // Total frames = ceil((S + delay) / 1024) for every tested length,
        // channel count, and chunking, including empty input.
        for channels in [1u32, 2, 6] {
            let config =
                EncoderConfig::new(48000, channels, FRAME_LENGTH, TransportType::Adts).unwrap();
            for total in [0, 1, 511, 1023, 1024, 1025, 2048, 5000, 9600, 47104, 48000] {
                for chunks in [&[7usize][..], &[4096][..]] {
                    let (feeder, frames) = feed_chunks(config, total, chunks);
                    let delay = feeder.delay();
                    let expected = total.checked_add(delay).unwrap().div_ceil(FRAME_LENGTH);
                    assert_eq!(
                        frames.len(),
                        expected,
                        "ch {channels} total {total} chunks {chunks:?}"
                    );
                    assert_eq!(feeder.emitted_frames(), expected as u64);
                }
            }
        }
    }

    #[test]
    fn emitted_counts_match_observed_reference_runs() {
        // M1 baseline: aac-enc produced 49 ADTS frames for 48000 stereo
        // samples and 11 for 9600 mono samples (delay 1600 in both cases).
        let (_, stereo) = feed_chunks(stereo_48k(), 48000, &[2048]);
        assert_eq!(stereo.len(), 49);
        let (_, mono) = feed_chunks(mono_48k(), 9600, &[100]);
        assert_eq!(mono.len(), 11);
    }

    #[test]
    fn exact_frame_input_still_flushes_delay() {
        // 2048 stereo samples fill exactly 2 frames during push; finish must
        // add ceil(1600/1024) = 2 all-zero frames.
        let mut feeder = PcmFeeder::new(stereo_48k());
        let input = interleave(2, 2048, pattern);
        let frames = feeder.push_interleaved(&input).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(feeder.pending_per_channel(), 0);
        let tail = feeder.finish().unwrap();
        assert_eq!(tail.len(), 2);
        for frame in &tail {
            for block in frame.channels() {
                assert_eq!(block, &[0i16; FRAME_LENGTH]);
            }
        }
        assert_eq!(feeder.emitted_frames(), 4);
    }

    #[test]
    fn empty_stream_emits_delay_frames() {
        let mut feeder = PcmFeeder::new(mono_48k());
        assert_eq!(feeder.push_interleaved(&[]).unwrap().len(), 0);
        let frames = feeder.finish().unwrap();
        assert_eq!(frames.len(), 1600_usize.div_ceil(FRAME_LENGTH));
        assert_eq!(
            feeder.zeros_appended(),
            frames.len() as u64 * FRAME_LENGTH as u64
        );
    }

    #[test]
    fn invalid_usage_is_rejected() {
        let mut feeder = PcmFeeder::new(stereo_48k());
        assert_eq!(
            feeder.push_interleaved(&[1, 2, 3]),
            Err(FeederError::UnalignedInput {
                len: 3,
                channels: 2
            })
        );
        // Rejected push leaves no residue.
        assert_eq!(feeder.pending_per_channel(), 0);
        feeder.finish().unwrap();
        assert_eq!(
            feeder.push_interleaved(&[0, 0]),
            Err(FeederError::AlreadyFinished)
        );
        assert_eq!(feeder.finish(), Err(FeederError::AlreadyFinished));
    }
}
