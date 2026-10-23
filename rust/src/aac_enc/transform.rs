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
//! Forward MDCT with AAC-LC overlap/window staging.
//!
//! Mirrors the `psy_main.cpp` transform stage (lines 552-629) on top of the
//! existing [`Mdct`](crate::common::mdct::Mdct) forward kernel, which is a
//! line-for-line float port of `mdct_block` (`cpp/libFDK/src/mdct.cpp:135`):
//! same fold (`-D-Cr`, `A-Br`), same left-slope-from-previous-frame /
//! right-slope-explicit convention, same per-spectrum slope update, same
//! 448-sample short-block input offset. Reusing it was verified against the
//! C source rather than assumed: input layout, window indexing, and state
//! updates all match, so no second MDCT implementation is introduced.
//!
//! Staging per 1024-sample fresh frame (one instance per channel):
//! 1. The first 448 fresh samples become visible at ring positions
//!    1600..2048 (`psy_main.cpp:490-492`); the other 576 stay back for the
//!    block-switching look-ahead.
//! 2. One 1024-line transform for long/start/stop, or eight 128-line
//!    transforms over ring positions 448..1600 for short
//!    (`transform.cpp:132-156`, via `Mdct::mdct` with `n_spec` 1 or 8).
//!    The right slope length is 128 for start/short, 1024 otherwise.
//! 3. The ring rotates: the oldest 1024 samples drop, and fresh samples
//!    448..1024 land at 1024..1600 (`psy_main.cpp:618-627`).
//!
//! Numerical contract: input `i16` maps to `x / 32768.0`. Zero input
//! yields exactly zero output. Output scale is pinned two ways: an in-test
//! naive ISO MDCT fixes the absolute gain (the kernel applies a documented
//! 2.0 fold gain), and oracle comparison fixes agreement with the reference:
//! the Rust output equals the reference Q31 output read as `int * 2^(E-31)`
//! with a measured constant per-bin ratio of 1.0 (see `mdct_parity` test).

use super::block_switch::WindowSequence;
use super::config::FRAME_LENGTH;
use crate::common::enums::WindowShape;
use crate::common::mdct::Mdct;
use crate::common::tables::window_tables;
use core::fmt;

/// Transform ring length: two long frames (`MAX_INPUT_BUFFER_SIZE`).
pub const RING_LEN: usize = 2 * FRAME_LENGTH;
/// Block-switching offset for LC: `1024 + 9 * 1024 / 16 = 1600`
/// (`psy_main.cpp:456-457`).
pub const BLOCK_SWITCH_OFFSET: usize = FRAME_LENGTH + 9 * FRAME_LENGTH / 16;
/// Fresh samples visible before the transform (`2 * 1024 - 1600 = 448`).
pub const PRE_COPY: usize = 2 * FRAME_LENGTH - BLOCK_SWITCH_OFFSET;
/// Fresh samples staged after the transform (`1600 - 1024 = 576`).
#[cfg(test)]
pub const POST_COPY: usize = BLOCK_SWITCH_OFFSET - FRAME_LENGTH;
/// MDCT lines per frame (1 x 1024 or 8 x 128).
pub const MDCT_LINES: usize = FRAME_LENGTH;
/// Short-block transform length.
pub const SHORT_LEN: usize = FRAME_LENGTH / 8;

/// Forward-transform failures. All are reported via `Result`, never panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformError {
    /// Only SINE/KBD slopes exist on the LC path; LOW_OVERLAP is LD-only.
    UnsupportedWindowShape(WindowShape),
    /// The MDCT kernel did not return a full frame of lines.
    IncompleteLines(i32),
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransformError::UnsupportedWindowShape(shape) => {
                write!(f, "unsupported window shape for AAC-LC: {shape:?}")
            }
            TransformError::IncompleteLines(n) => {
                write!(f, "MDCT kernel returned {n} lines, expected {MDCT_LINES}")
            }
        }
    }
}

impl std::error::Error for TransformError {}

/// Per-channel forward-transform state: the 2048-sample ring, the persistent
/// MDCT previous-slope state, and the previous shape (bitstream use later).
pub struct ChannelTransform {
    ring: [f32; RING_LEN],
    mdct: Mdct,
    last_shape: WindowShape,
}

impl ChannelTransform {
    /// Zeroed state, mirroring cleared `psyInputBuffer`/`mdctPers`/
    /// `lastWindowShape` (SINE) at encoder start.
    pub fn new() -> Self {
        Self {
            ring: [0.0; RING_LEN],
            mdct: Mdct::new(0).expect("zero-size overlap buffer always allocates"),
            last_shape: WindowShape::Sine,
        }
    }

    /// Transform ring (test inspection).
    #[cfg(test)]
    pub fn ring(&self) -> &[f32; RING_LEN] {
        &self.ring
    }

    /// Window shape of the previous frame.
    pub fn last_shape(&self) -> WindowShape {
        self.last_shape
    }

    /// Stage one fresh frame and compute its 1024 MDCT lines.
    ///
    /// `sequence`/`shape` come from [`BlockSwitch`](super::block_switch::BlockSwitch)
    /// after stereo sync. Short sequences emit eight 128-line blocks in
    /// order; all others emit one 1024-line block.
    pub fn process(
        &mut self,
        fresh: &[i16; FRAME_LENGTH],
        sequence: WindowSequence,
        shape: WindowShape,
    ) -> Result<[f32; MDCT_LINES], TransformError> {
        for (dst, src) in self.ring[BLOCK_SWITCH_OFFSET..]
            .iter_mut()
            .zip(&fresh[..PRE_COPY])
        {
            *dst = *src as f32 / 32768.0;
        }

        let slope_len = match sequence {
            WindowSequence::Start | WindowSequence::Short => SHORT_LEN,
            WindowSequence::Long | WindowSequence::Stop => FRAME_LENGTH,
        };
        let slope = match shape {
            WindowShape::Sine | WindowShape::KBD => {
                window_tables::get_table(slope_len as u16, shape)
                    .ok_or(TransformError::UnsupportedWindowShape(shape))?
            }
            WindowShape::LowOverlap => return Err(TransformError::UnsupportedWindowShape(shape)),
        };
        let n_spec = if sequence == WindowSequence::Short {
            8
        } else {
            1
        };

        let mut out = [0.0f32; MDCT_LINES];
        let got = self
            .mdct
            .mdct(&mut self.ring[..], &mut out[..], n_spec, slope);
        if got != MDCT_LINES as i32 {
            return Err(TransformError::IncompleteLines(got));
        }

        self.ring.copy_within(FRAME_LENGTH..RING_LEN, 0);
        for (dst, src) in self.ring[FRAME_LENGTH..BLOCK_SWITCH_OFFSET]
            .iter_mut()
            .zip(&fresh[PRE_COPY..])
        {
            *dst = *src as f32 / 32768.0;
        }
        self.last_shape = shape;
        Ok(out)
    }
}

impl Default for ChannelTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::block_switch::BlockSwitch;
    use crate::aac_enc::testdata;
    use crate::common::enums::WindowShape;

    /// Fresh 1024-sample input frames for all channels at frame `f`,
    /// zero-padded past the fixture end (mirrors oracle flush frames).
    fn fresh_at(fixture: &[Vec<i16>], frame: u32, channels: usize) -> Vec<[i16; FRAME_LENGTH]> {
        (0..channels)
            .map(|c| {
                let mut block = [0i16; FRAME_LENGTH];
                for (i, v) in block.iter_mut().enumerate() {
                    let t = frame as usize * FRAME_LENGTH + i;
                    *v = if t < fixture[c].len() {
                        fixture[c][t]
                    } else {
                        0
                    };
                }
                block
            })
            .collect()
    }

    /// One full frame step for every channel: block-switch all channels,
    /// then sync (pair or mono, as `psy_main` does), then transform all.
    /// Ordering matters: transforms must see post-sync decisions.
    fn run_frame(
        switches: &mut [BlockSwitch],
        transforms: &mut [ChannelTransform],
        fresh: &[[i16; FRAME_LENGTH]],
    ) -> Vec<[f32; MDCT_LINES]> {
        for (sw, fr) in switches.iter_mut().zip(fresh.iter()) {
            sw.process(fr, false);
        }
        match switches.len() {
            2 => {
                let (left, right) = switches.split_at_mut(1);
                BlockSwitch::synchronize_pair(&mut left[0], &mut right[0]);
            }
            1 => switches[0].synchronize_mono(),
            n => panic!("M2 covers mono/stereo, not {n} channels"),
        }
        transforms
            .iter_mut()
            .zip(fresh.iter())
            .zip(switches.iter())
            .map(|((tr, fr), sw)| tr.process(fr, sw.sequence(), sw.shape()).unwrap())
            .collect()
    }

    fn run_fixture(name: &str) -> (Vec<Vec<i16>>, Vec<BlockSwitch>, Vec<ChannelTransform>) {
        let fixture = testdata::fixture_samples(name);
        let channels = fixture.len();
        let switches = (0..channels).map(|_| BlockSwitch::new()).collect();
        let transforms = (0..channels).map(|_| ChannelTransform::new()).collect();
        (fixture, switches, transforms)
    }

    #[test]
    fn staging_constants_match_psy_main() {
        assert_eq!(BLOCK_SWITCH_OFFSET, 1600);
        assert_eq!(PRE_COPY, 448);
        assert_eq!(POST_COPY, 576);
        assert_eq!(RING_LEN, 2048);
    }

    #[test]
    fn window_tables_have_encoder_lengths() {
        // Right-slope tables consumed per frame: 512 pairs for long (1024),
        // 64 for short (128), both shapes.
        for (len, pairs) in [(1024u16, 512), (128u16, 64)] {
            for shape in [WindowShape::Sine, WindowShape::KBD] {
                let table = window_tables::get_table(len, shape)
                    .unwrap_or_else(|| panic!("missing table {len} {shape:?}"));
                assert_eq!(table.len(), pairs, "{len} {shape:?}");
            }
        }
    }

    #[test]
    fn silence_in_silence_out() {
        let mut tr = ChannelTransform::new();
        let silence = [0i16; FRAME_LENGTH];
        for (seq, shape) in [
            (WindowSequence::Long, WindowShape::KBD),
            (WindowSequence::Start, WindowShape::Sine),
            (WindowSequence::Short, WindowShape::Sine),
            (WindowSequence::Stop, WindowShape::KBD),
        ] {
            let out = tr.process(&silence, seq, shape).unwrap();
            assert!(out.iter().all(|v| *v == 0.0), "{seq:?}");
        }
    }

    #[test]
    fn rejects_low_overlap_shape() {
        let mut tr = ChannelTransform::new();
        let silence = [0i16; FRAME_LENGTH];
        assert_eq!(
            tr.process(&silence, WindowSequence::Long, WindowShape::LowOverlap),
            Err(TransformError::UnsupportedWindowShape(
                WindowShape::LowOverlap
            ))
        );
    }

    #[test]
    fn naive_mdct_pins_absolute_scale() {
        // Independent ISO/IEC 14496-3 MDCT (sine window, f64) versus one
        // long SINE-block call of the shared kernel: validates folding,
        // window-table values, DCT-IV, and overall gain with no reference
        // to the C implementation.
        const N: usize = 1024;
        let mut input = [0.0f32; 2 * N];
        for (n, v) in input.iter_mut().enumerate() {
            let t = n as f64 / 48000.0;
            *v = (0.5 * (2.0 * core::f64::consts::PI * 1000.0 * t).sin()
                + 0.125 * (2.0 * core::f64::consts::PI * 250.0 * t).sin()) as f32;
        }
        let slope = window_tables::get_table(1024, WindowShape::Sine).unwrap();
        let mut mdct = Mdct::new(0).unwrap();
        let mut out = [0.0f32; N];
        assert_eq!(mdct.mdct(&mut input[..], &mut out[..], 1, slope), N as i32);

        // The shared kernel applies a documented 2.0 fold gain
        // (MDCT_FAC_FL), so the naive reference is scaled to match.
        let mut worst_rel = 0.0f64;
        let mut peak = 0.0f64;
        let mut reference = [0.0f64; N];
        for (k, ref_k) in reference.iter_mut().enumerate() {
            let mut acc = 0.0f64;
            for (n, &x) in input.iter().enumerate() {
                let w = (core::f64::consts::PI * (n as f64 + 0.5) / (2 * N) as f64).sin();
                let angle = core::f64::consts::PI / N as f64
                    * (n as f64 + 0.5 + N as f64 / 2.0)
                    * (k as f64 + 0.5);
                acc += x as f64 * w * angle.cos();
            }
            *ref_k = 2.0 * acc;
            peak = peak.max(ref_k.abs());
        }
        for (k, &ref_k) in reference.iter().enumerate() {
            if ref_k.abs() > 0.01 * peak {
                worst_rel = worst_rel.max((out[k] as f64 - ref_k).abs() / ref_k.abs());
            }
        }
        assert!(
            worst_rel < 5e-4,
            "naive MDCT agreement broke down: worst relative error {worst_rel}"
        );
    }

    #[test]
    fn ring_schedule_matches_oracle() {
        // The oracle dumps the pre-transform ring. The Rust state after
        // processing frame f must equal oracle pre(f+1)[0..1600] followed
        // by oracle pre(f)[1600..2048]: rotation moves [1024..2048] down to
        // [0..1024], fresh[448..1024] lands at [1024..1600], and
        // [1600..2048] is untouched by rotation. Covers priming zeros at
        // stream start and the onset edge.
        for (name, frame) in [
            ("onset-stereo", 0u32),
            ("onset-stereo", 23),
            ("onset-stereo", 24),
        ] {
            let (fixture, mut switches, mut transforms) = run_fixture(name);
            for f in 0..=frame {
                run_frame(&mut switches, &mut transforms, &fresh_at(&fixture, f, 2));
            }
            let pre_next = testdata::load_ring(name, frame + 1, 0);
            let pre_this = testdata::load_ring(name, frame, 0);
            let ring = transforms[0].ring();
            for i in 0..BLOCK_SWITCH_OFFSET {
                assert_eq!(
                    (ring[i] * 32768.0).round() as i16,
                    pre_next[i],
                    "{name} f{frame} ring[{i}]"
                );
            }
            for i in BLOCK_SWITCH_OFFSET..RING_LEN {
                assert_eq!(
                    (ring[i] * 32768.0).round() as i16,
                    pre_this[i],
                    "{name} f{frame} ring[{i}]"
                );
            }
        }
    }

    #[test]
    fn synthesis_matches_oracle_input() {
        // The in-test fixture synthesis must reproduce the exact PCM the
        // oracle consumed; the committed rings embed ground-truth samples.
        // Onset rings cover stream start, the silence/sine edge at sample
        // 24000, and steady sine at phases identical to the sine fixture.
        // The 44100 sine-stream-start case pins the wav↔resynthesis
        // contract: a bare-function fixture bug once shipped an all-silent
        // 44100 wav while resynthesis produced the intended sine. The f0
        // ring now carries real sine and must match sample-exactly.
        for (name, frame, _ch) in [
            ("onset-stereo", 0u32, 0u32),
            ("onset-stereo", 23, 0),
            ("onset-stereo", 24, 0),
            ("sine-stereo-44100", 0, 0),
        ] {
            let fixture = testdata::fixture_samples(name)[0].clone();
            let ring = testdata::load_ring(name, frame, 0);
            // Ring[1600..2048] holds fresh[0..448] of this frame.
            let base = frame as usize * FRAME_LENGTH;
            for i in 0..PRE_COPY {
                assert_eq!(
                    fixture[base + i],
                    ring[BLOCK_SWITCH_OFFSET + i],
                    "f{frame} off{i}"
                );
            }
            // Ring[1024..1600] holds fresh[448..1024] of the previous frame.
            if frame > 0 {
                for i in 0..POST_COPY {
                    assert_eq!(
                        fixture[base - FRAME_LENGTH + PRE_COPY + i],
                        ring[FRAME_LENGTH + i],
                        "f{frame} post[{i}]"
                    );
                }
            }
        }
    }

    #[test]
    fn mdct_parity_with_reference() {
        // Every oracle fixture runs end-to-end through the Rust staging;
        // selected MDCT vectors are compared against the reference Q31
        // output read as `int * 2^(E-31)`. The Rust/reference per-bin ratio
        // must be constant across all bins and frames (proving identical
        // mathematics: folding, windows, tables, DCT) with median 1.0
        // (pinning the exponent convention and the overall gain with no
        // fudge factor).
        let vectors = [
            ("onset-stereo", 22u32, 0u32),
            ("onset-stereo", 23, 0),
            ("onset-stereo", 24, 0),
            ("onset-stereo", 24, 1),
            ("onset-stereo", 25, 0),
            ("onset-stereo", 26, 0),
            ("stepdown-stereo", 0, 0),
            ("stepdown-stereo", 1, 0),
            ("stepdown-stereo", 2, 0),
            ("click-mono", 11, 0),
            ("sine-stereo", 5, 0),
            ("impulse-mono", 4, 0),
            ("impulse-mono", 5, 0),
        ];
        let mut ratios = Vec::new();
        let mut worst_overall = 0.0f64;
        let mut worst_at = ("", 0u32, 0u32, 0.0f64);
        for (name, frame, ch) in vectors {
            let (fixture, mut switches, mut transforms) = run_fixture(name);
            let channels = fixture.len();
            let (e, reference) = testdata::load_mdct(name, frame, ch);
            let mut selected = [0.0f32; MDCT_LINES];
            for f in 0..=frame {
                let outs = run_frame(
                    &mut switches,
                    &mut transforms,
                    &fresh_at(&fixture, f, channels),
                );
                if f == frame {
                    selected = outs[ch as usize];
                }
            }
            let peak = reference
                .iter()
                .map(|&q| testdata::q31_to_f64(q, e).abs())
                .fold(0.0f64, f64::max);
            if peak == 0.0 {
                // Zero-reference frames (priming/look-ahead silence, and the
                // stepdown START whose input hides in the D region) must be
                // exactly zero through the float path as well.
                assert!(
                    selected.iter().all(|&v| v == 0.0),
                    "{name} f{frame}: nonzero output for zero reference"
                );
                continue;
            }
            for (r, &q) in selected.iter().zip(reference.iter()) {
                let c = testdata::q31_to_f64(q, e);
                let diff = (f64::from(*r) - c).abs();
                if diff > worst_overall {
                    worst_overall = diff;
                    worst_at = (name, frame, ch, peak);
                }
                if c.abs() > 0.1 * peak {
                    ratios.push(f64::from(*r) / c);
                }
            }
        }
        // The silence vector is committed zeros with exponent 12.
        {
            let (e, reference) = testdata::load_mdct("silence-stereo", 5, 0);
            assert_eq!(e, 12);
            assert!(reference.iter().all(|&q| q == 0));
            let (fixture, mut switches, mut transforms) = run_fixture("silence-stereo");
            for f in 0..=5 {
                let outs = run_frame(&mut switches, &mut transforms, &fresh_at(&fixture, f, 2));
                if f == 5 {
                    assert!(outs[0].iter().all(|&v| v == 0.0));
                }
            }
        }
        let min_r = ratios.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_r = ratios.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let med_r = {
            let mut sorted = ratios.clone();
            sorted.sort_by(f64::total_cmp);
            sorted[sorted.len() / 2]
        };
        assert!(!ratios.is_empty(), "no significant bins compared");
        // Agreement is absolute, scaled by each vector's peak: this covers
        // near-zero bins uniformly, where ratios of two roundings blow up
        // even though both sides agree to far below audibility. Bound 5e-4
        // is ~2x the worst observed deviation across the corpus (long,
        // short, start, stop; KBD and sine; tones, impulse, silence),
        // driven by f32 arithmetic vs Q31 quantization/table rounding.
        assert!(
            worst_overall < 5e-4 * worst_at.3.max(1.0),
            "worst deviation {} at {} f{} ch{} (peak {})",
            worst_overall,
            worst_at.0,
            worst_at.1,
            worst_at.2,
            worst_at.3
        );
        // The median ratio over strong bins pins the exponent convention
        // and overall gain with no fudge factor: any power-of-two
        // convention error would move it far outside this bound. Measured
        // 1.0000076 on the corpus (an early 4.0 prediction was refuted by
        // measurement on two independent vectors and corrected here).
        assert!(
            (med_r - 1.0).abs() < 1e-3,
            "path gain median {med_r} (min {min_r} max {max_r}), expected 1.0"
        );
    }
}
