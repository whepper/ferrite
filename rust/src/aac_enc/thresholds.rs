/* Ferrite: this file is part of a Third-Party Modified Version of the
Fraunhofer FDK AAC Codec Library for Android. Modifications and Rust
implementation copyright © 2025–2026 Jeroen Steenvoorde. Modification
notice date: 2026-10-26. See rust/NOTICE for the full license text. */
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
//! Threshold advance: clip cap, spreading, PCM floor, pre-echo control.
//!
//! Integer-exact mirror of the per-channel/window chain in
//! `psy_main.cpp:922-1019` (minus M/S) operating on Q31 stored-domain
//! integers with the shared `q31` primitives. Given identical inputs every
//! op is bit-exact (shifts, truncations, saturations all mirrored); the
//! only approximation enters through spectrum-derived inputs upstream.
//!
//! Order per band (verified against source): clip cap (`min`), spreading
//! max-envelope, PCM floor (`max` with the `>>THR_SHIFTBITS` headroom
//! removal and the per-window accumulating `energyShift`), pre-echo
//! control with STOP-before/START-after state resets, then spread-energy
//! copy + spreading.

use super::q31::{fmult, fmult_sgl, shift_int};

/// Threshold headroom shift (`psy_configuration.h:113`).
pub const THR_SHIFTBITS: i32 = 4;
/// PCM floor shift added per window (`psy_configuration.h:114`).
pub const PCM_QUANT_THR_SCALE: i32 = 16;
/// `C_RATIO` as stored (`0x02940a10`, `psy_configuration.h:117-118`).
pub const C_RATIO_Q31: i32 = 0x02940a10;
/// Pre-echo remaining factor as stored (`0x0148`,
/// `psy_configuration.cpp:592`).
pub const MIN_REMAINING_Q15: i16 = 0x0148;
/// Pre-echo increase factor (`psy_configuration.cpp:591`).
pub const MAX_INCREASE: i32 = 2;
/// `MAXVAL_DBL` stop/start reset value.
pub const MAX_Q31: i32 = i32::MAX;

/// Initial threshold from a stored-domain energy
/// (`psy_main.cpp:702-703`).
pub fn init_threshold(energy_q31: i32) -> i32 {
    fmult(energy_q31, C_RATIO_Q31)
}

/// Clip cap for one channel (`psy_main.cpp:926-939`): the stored clip
/// energy shifted by `-(2*mdctScale - THR_SHIFTBITS)`, or uncapped
/// (`MAXVAL`) beyond the short-window headroom.
pub fn clip_cap(clip_q31: i32, mdct_scale: i32, short_window: bool) -> i32 {
    let shift = 2 * mdct_scale - THR_SHIFTBITS;
    let headroom = if short_window { 6 } else { 0 };
    if shift >= -headroom {
        shift_int(clip_q31, -shift)
    } else {
        MAX_Q31
    }
}

/// Two-pass max-envelope spreading (`spreading.cpp:105-125`): forward with
/// high factors, backward with low factors, in place.
pub fn spreading_max(n: usize, mask_lo: &[i32], mask_hi: &[i32], x: &mut [i32]) {
    debug_assert!(n >= 1 && mask_lo.len() >= n && mask_hi.len() >= n && x.len() >= n);
    let mut d = x[0];
    for i in 1..n {
        d = x[i].max(fmult(mask_hi[i], d));
        x[i] = d;
    }
    d = x[n - 1];
    for i in (0..n - 1).rev() {
        d = x[i].max(fmult(mask_lo[i], d));
        x[i] = d;
    }
}

/// PCM quantization floor for one window (`psy_main.cpp:955-971`).
/// `energy_shift` is the accumulating per-channel shift, starting at
/// `2*mdctScale`; it grows by 16 per window (with the C `min(31, ...)`
/// caps applied at the same points).
pub fn apply_pcm_floor(thr: &mut [i32], pcm_q31: &[i32], energy_shift: &mut i32) {
    *energy_shift += PCM_QUANT_THR_SCALE;
    if *energy_shift >= 0 {
        *energy_shift = (*energy_shift).min(31);
        for (t, &p) in thr.iter_mut().zip(pcm_q31.iter()) {
            *t = (*t >> THR_SHIFTBITS).max(shift_int(p, -*energy_shift));
        }
    } else {
        *energy_shift = (-*energy_shift).min(31);
        for (t, &p) in thr.iter_mut().zip(pcm_q31.iter()) {
            *t = (*t >> THR_SHIFTBITS).max(shift_int(p, *energy_shift));
        }
    }
}

/// Pre-echo control state per channel (mirrors `PSY_STATIC` nm1,
/// mdctScalenm1, calcPreEcho).
pub struct PreEchoState {
    prev: Vec<i32>,
    prev_scale: i32,
    enabled: bool,
}

impl PreEchoState {
    /// `InitPreEchoControl` (`pre_echo_control.cpp:106-115`): previous scale
    /// 8, previous thresholds = long PCM floors, control enabled.
    /// `num_bands` is the long `sfbCnt` (state sized `MAX_SFB = 51` in C).
    pub fn new(pcm_q31_long: &[i32]) -> Self {
        let mut prev = vec![0i32; 51];
        prev[..pcm_q31_long.len()].copy_from_slice(pcm_q31_long);
        Self {
            prev,
            prev_scale: PCM_QUANT_THR_SCALE >> 1,
            enabled: true,
        }
    }

    /// STOP-before / START-after reset (`psy_main.cpp:975-985, 996-1006`):
    /// previous thresholds to max, scale to 0, control disabled so the next
    /// frame only memorizes.
    pub fn reset_for_transition(&mut self, active: usize) {
        for v in self.prev.iter_mut().take(active) {
            *v = MAX_Q31;
        }
        self.prev_scale = 0;
        self.enabled = false;
    }

    /// `PreEchoControl` (`pre_echo_control.cpp:117-176`), exact integer
    /// mirror: `out = min(c, max(2p, 0.01c))` with scale-aligned shifts,
    /// previous thresholds memorized pre-clamp on every path.
    pub fn control(&mut self, thr: &mut [i32], mdct_scale: i32) {
        if !self.enabled {
            self.prev[..thr.len()].copy_from_slice(thr);
            self.prev_scale = mdct_scale;
            return;
        }
        if mdct_scale > self.prev_scale {
            let scaling = 2 * (mdct_scale - self.prev_scale);
            for (p, c) in self.prev.iter_mut().zip(thr.iter_mut()) {
                let cap = MAX_INCREASE.wrapping_mul(shift_int(*p, -scaling));
                let floor = fmult_sgl(MIN_REMAINING_Q15, *c);
                let tmp = *c;
                *p = tmp;
                *c = tmp.min(cap).max(floor);
            }
        } else {
            let scaling = 2 * (self.prev_scale - mdct_scale);
            for (p, c) in self.prev.iter_mut().zip(thr.iter_mut()) {
                let cap = (MAX_INCREASE >> 1) * *p;
                let floor = fmult_sgl(MIN_REMAINING_Q15, *c);
                *p = *c;
                if shift_int(*c, -(scaling + 1)) > cap {
                    *c = shift_int(cap, scaling + 1);
                }
                *c = (*c).max(floor);
            }
        }
        self.prev_scale = mdct_scale;
    }

    /// Previous thresholds (test inspection).
    pub fn prev(&self) -> &[i32] {
        &self.prev
    }

    /// Previous scale (test inspection).
    pub fn prev_scale(&self) -> i32 {
        self.prev_scale
    }

    /// Set the enabled flag (mirrors `calcPreEcho = 1` after each call).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether control is enabled (test inspection).
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_init_is_exact_mult() {
        assert_eq!(init_threshold(0), 0);
        // fMult(2^30, C_RATIO) = ((2^30 * C)>>32)<<1 = (C>>2)<<1 = C/2.
        let e = 0x4000_0000i32;
        let want = (((e as i64 * C_RATIO_Q31 as i64) >> 32) << 1) as i32;
        assert_eq!(init_threshold(e), want);
        assert_eq!(want, 0x014A_0508);
    }

    #[test]
    fn clip_cap_mirrors_shift_branches() {
        // mdctScale 9, long: shift = 18-4 = 14.
        assert_eq!(clip_cap(0x7735_93ff, 9, false), 0x7735_93ff >> 14);
        // Short window tolerates 6 extra bits of negative shift.
        assert_eq!(clip_cap(0x7735_93ff >> 6, 0, true), (0x7735_93ff >> 6) << 4);
        // Beyond headroom: uncapped.
        assert_eq!(clip_cap(12345, -10, false), MAX_Q31);
        assert_eq!(clip_cap(12345, -10, true), MAX_Q31);
    }

    #[test]
    fn spreading_max_envelope_is_exact() {
        // Hand-computed two-pass envelope with 0.5 factors under ARM64
        // floor semantics: fMult(0.5, 50) = 50>>1 = 25 (the generic-C
        // `((x>>2)<<1)` would give 24; verified against the reference
        // build, which selects the ARM64 variant).
        let half = 0x4000_0000;
        let mut x = vec![100, 10, 10];
        spreading_max(3, &[half, half, 0], &[0, half, half], &mut x);
        assert_eq!(x, vec![100, 50, 25]);
        // Edge factors of 0 keep end bands (boundaries per initSpreading).
        let mut y = vec![5, 100, 5];
        spreading_max(3, &[half, half, 0], &[0, half, half], &mut y);
        assert_eq!(y, vec![50, 100, 50]);
    }

    #[test]
    fn pcm_floor_shift_accumulates_per_window() {
        // energyShift starts 0 (*not* mdctScale-derived here: unit test of
        // the accumulation); window 0 uses >>16, window 1 hits the 31 cap.
        let pcm = vec![0x7735_93ff, 0x7735_93ff];
        let mut thr = vec![0, 0];
        let mut shift = 0;
        apply_pcm_floor(&mut thr, &pcm, &mut shift);
        assert_eq!(shift, 16);
        // 0x773593ff>>16 = 30517.
        assert_eq!(thr, vec![30517, 30517]);
        apply_pcm_floor(&mut thr, &pcm, &mut shift);
        assert_eq!(shift, 31);
        // Thresholds now hold 30517: max(30517>>4, pcm>>31) = max(1907, 0).
        assert_eq!(thr, vec![1907, 1907]);
    }

    #[test]
    fn pre_echo_bypass_clamp_floor_memorize() {
        // Bypass: memorize only.
        let mut st = PreEchoState::new(&[7, 7]);
        st.enabled = false;
        let mut thr = vec![100, 200];
        st.control(&mut thr, 9);
        assert_eq!(thr, vec![100, 200]);
        assert_eq!(&st.prev()[..2], &[100, 200]);
        assert_eq!(st.prev_scale(), 9);

        // Clamp: current far above twice previous.
        let mut st = PreEchoState::new(&[10, 10]);
        let mut thr = vec![1000, 1000];
        st.control(&mut thr, 8);
        // mdctScale 8 == prev_scale 8: else-branch, scaling 0:
        // cap = 1*10 = 10; (1000>>1)=500 > 10 -> c = 10<<1 = 20; floor tiny.
        assert_eq!(thr, vec![20, 20]);
        // Memorized pre-clamp values.
        assert_eq!(&st.prev()[..2], &[1000, 1000]);

        // Floor: previous far below current -> 1% of current holds.
        let mut st = PreEchoState::new(&[0, 0]);
        let mut thr = vec![100000, 100000];
        st.control(&mut thr, 8);
        let floor = fmult_sgl(MIN_REMAINING_Q15, 100000);
        assert_eq!(thr, vec![floor, floor]);
        assert!(floor > 900 && floor < 1100);

        // Normal: current below twice previous passes through.
        let mut st = PreEchoState::new(&[1000, 1000]);
        let mut thr = vec![1500, 1500];
        st.control(&mut thr, 8);
        assert_eq!(thr, vec![1500, 1500]);
    }

    #[test]
    fn pre_echo_reset_disables_next_frame() {
        let mut st = PreEchoState::new(&[7, 7]);
        st.reset_for_transition(2);
        assert!(!st.is_enabled());
        assert_eq!(st.prev_scale(), 0);
        assert_eq!(&st.prev()[..2], &[MAX_Q31, MAX_Q31]);
    }
}
