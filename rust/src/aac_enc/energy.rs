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
//! Band energies with exact integer shift bookkeeping.
//!
//! Mirrors the rescaling stage (`psy_main.cpp:631-749`) and the TNS energy
//! recompute (`psy_main.cpp:868-912`) with two lanes (see `q31` for the
//! shared integer primitives):
//!
//! - Integer lane (exact): `maxScaleSpec` per band (leading-zero headroom of
//!   the Q31-mapped spectrum), `minSpecShift`/`nrgShift`/`finalShift`, and
//!   the `maxNrg` return.
//! - Value lane: band energies as stored-domain integers (mirroring the
//!   scaled sums bit-nearly) plus absolute log-domain energies
//!   (`log2(E)/64`, `-1.0` sentinel) computed directly from true sums —
//!   the C `LdData` scale corrections only repair its shifted-int
//!   bookkeeping, so they have no float equivalent.
//!
//! The single approximation point is the float-to-int spectrum mapping
//! (round-to-nearest of the M2 float spectrum into its Q31 domain); every
//! subsequent integer op is exact. Spectrum-derived integers therefore
//! compare with small tolerances, while pure-integer state
//! (`maxScaleSpec` where boundaries hold, shifts, `mdctScale`) must match
//! exactly.

use super::q31::{
    count_leading_bits, count_leading_zeros, fpow2_add_div2, scale_value_saturate, shift_int,
};

/// `CalcSfbMaxScaleSpec` (`band_nrg.cpp:111-132`): per-band headroom as
/// `min(30, clz(max|q|) - 1)` over Q31-mapped spectrum ints.
///
/// M5.20 `_into` form writes into caller-owned scratch: `clear` + `push`
/// yields exactly the former fresh `vec![0i32; sfb_active]` result (same
/// values, same length, same order — only the allocation is reused).
pub fn max_scale_spec_into(
    spectrum_q31: &[i32],
    offsets: &[u16],
    sfb_active: usize,
    out: &mut Vec<i32>,
) {
    out.clear();
    out.reserve(sfb_active);
    for b in 0..sfb_active {
        let mut peak: i32 = 0;
        for j in offsets[b] as usize..offsets[b + 1] as usize {
            let a = spectrum_q31[j].wrapping_abs();
            // fixp_abs(MINVAL) wraps in C; mirror with wrapping_abs.
            // (Unreachable for real spectra; kept for exactness.)
            peak = peak.max(a);
        }
        out.push((count_leading_zeros(peak) - 1).min(30));
    }
}

#[cfg(test)]
pub fn max_scale_spec(spectrum_q31: &[i32], offsets: &[u16], sfb_active: usize) -> Vec<i32> {
    let mut out = Vec::new();
    max_scale_spec_into(spectrum_q31, offsets, sfb_active, &mut out);
    out
}

/// `CheckBandEnergyOptim` (`band_nrg.cpp:140-184`): scaled per-band energy
/// sums (`spec << max(0, maxScale-4)`, halved-square accumulation,
/// final `<<1`). Returns the raw sums; the caller derives `maxNrg` with
/// [`max_nrg_for_shift`] and Ld values directly from true sums.
pub fn check_band_energy_into(
    spectrum_q31: &[i32],
    max_scale: &[i32],
    offsets: &[u16],
    sfb_active: usize,
    energies: &mut Vec<i32>,
) {
    energies.clear();
    energies.reserve(sfb_active);
    for b in 0..sfb_active {
        let scale = (max_scale[b] - 4).max(0);
        let mut tmp: i32 = 0;
        for j in offsets[b] as usize..offsets[b + 1] as usize {
            let spec = shift_int(spectrum_q31[j], scale);
            tmp = fpow2_add_div2(tmp, spec);
        }
        energies.push(tmp.wrapping_shl(1));
    }
}

/// The `maxNrg` return of `CheckBandEnergyOptim` (`band_nrg.cpp:177-183`):
/// argmax over scale-corrected Ld values (computed here in true real
/// arithmetic, which preserves the ordering of the C `fLog2` approximation
/// up to its ~1e-9 error), then `scaleValue(bandEnergy[nr],
/// max(2*((minSpecShift-4)-scale0), -31))`. NOTE the `-4`: callers pass the
/// raw global minimum and this function applies the same `-4` the C
/// callers fold into the argument (`psy_main.cpp` passes `minSpecShift -
/// 4`); missing it inflates maxNrg 256x (found via onset f24: nrgShift -2
/// vs reference 2).
pub fn max_nrg_for_shift(energies: &[i32], max_scale: &[i32], min_spec_shift: i32) -> i32 {
    let mut nr = 0usize;
    let mut max_ld = -1.0f64;
    for (b, &e) in energies.iter().enumerate() {
        let scale0 = (max_scale[b] - 4).max(0);
        let ld = if e <= 0 {
            // fLog2 sentinel for non-positive input (fixpoint_math.h).
            -1.0
        } else {
            (e as f64 / 2_147_483_648.0).log2() / 64.0 - scale0 as f64 * 2.0 / 64.0
        };
        if ld > max_ld {
            max_ld = ld;
            nr = b;
        }
    }
    let scale0 = (max_scale[nr] - 4).max(0);
    let scale = (2 * ((min_spec_shift - 4) - scale0)).max(-31);
    shift_int(energies[nr], scale)
}

/// `CalcBandEnergyOptimShort` (`band_nrg.cpp:264-288`): per-band energies
/// for short windows. Note the differences from the long path: `maxScale -
/// 3` headroom, NO final `<<1`, and a saturating output rescale by
/// `-(2*(maxScale-3)-1)`.
pub fn band_energy_short_into(
    spectrum_q31: &[i32],
    max_scale: &[i32],
    offsets: &[u16],
    sfb_active: usize,
    energies: &mut Vec<i32>,
) {
    energies.clear();
    energies.reserve(sfb_active);
    for b in 0..sfb_active {
        let leading = max_scale[b] - 3;
        let mut tmp: i32 = 0;
        for j in offsets[b] as usize..offsets[b + 1] as usize {
            let spec = shift_int(spectrum_q31[j], leading);
            tmp = fpow2_add_div2(tmp, spec);
        }
        energies.push(tmp);
    }
    for b in 0..sfb_active {
        let scale = (2 * (max_scale[b] - 3) - 1).clamp(-31, 31);
        energies[b] = scale_value_saturate(energies[b], -scale);
    }
}

/// `CalcBandEnergyOptimLong` (`band_nrg.cpp:192-256`): per-band energies
/// for the TNS path with the overflow `shiftBits` decision and the guarded
/// Ld vector, all from the PRE-shift accumulations. Returns
/// `(post-shift stored energies, shiftBits, Ld)`.
///
/// Mirror notes: the `maxNrgLd <= 0` branch test and the `while` shift
/// counter run on Q31 ints in C; the float mirror differs at most 1 LSB
/// at exact 0/multiples of 2/64 boundaries (corpus-gated, never observed).
/// `maxNrgLd` starts at 0.0 in C (line 199), so all-nonpositive Ld takes
/// the zero path.
pub fn band_energy_long_into(
    spectrum_q31: &[i32],
    max_scale: &[i32],
    offsets: &[u16],
    sfb_active: usize,
    pre: &mut Vec<i32>,
    energies: &mut Vec<i32>,
    ld: &mut Vec<f64>,
) -> i32 {
    pre.clear();
    pre.reserve(sfb_active);
    for b in 0..sfb_active {
        let leading = max_scale[b] - 4;
        let mut tmp: i32 = 0;
        if leading >= 0 {
            for j in offsets[b] as usize..offsets[b + 1] as usize {
                let spec = shift_int(spectrum_q31[j], leading);
                tmp = fpow2_add_div2(tmp, spec);
            }
        } else {
            let shift = -leading;
            for j in offsets[b] as usize..offsets[b + 1] as usize {
                let spec = shift_int(spectrum_q31[j], -shift);
                tmp = fpow2_add_div2(tmp, spec);
            }
        }
        pre.push(tmp.wrapping_shl(1));
    }
    // Guarded Ld + max (C lines 224-235).
    ld.clear();
    ld.reserve(sfb_active);
    let mut max_ld = 0.0f64;
    for b in 0..sfb_active {
        let scale_diff = (max_scale[b] - 4) as f64 * 2.0 / 64.0;
        let base = if pre[b] <= 0 { -1.0 } else { log2frac(pre[b]) };
        let v = if base >= (-0.5 + scale_diff / 2.0) {
            base - scale_diff
        } else {
            -1.0
        };
        ld.push(v);
        max_ld = max_ld.max(v);
    }
    let shift_bits = if max_ld <= 0.0 {
        0
    } else {
        let mut run = max_ld;
        let mut shift_bits = 0;
        while run > 0.0 {
            run -= 2.0 / 64.0;
            shift_bits += 1;
        }
        shift_bits
    };
    energies.clear();
    energies.reserve(sfb_active);
    for b in 0..sfb_active {
        let scale = (((max_scale[b] - 4) + shift_bits) << 1).min(31);
        energies.push(shift_int(pre[b], -scale));
        if shift_bits > 0 {
            ld[b] -= shift_bits as f64 * 2.0 / 64.0;
        }
    }
    shift_bits
}

/// `CalcBandNrgMSOpt` (`band_nrg.cpp:296-361`): mid/side band energies for
/// stereo, full integer mirror including the `/4` energy convention the C
/// code produces (see module docs of the threshold chain consumer).
/// `calc_ld` mirrors the long-block flag. Returns
/// `(mid, side, mid_ld_true, side_ld_true)` with Ld in absolute log
/// domain (`log2/64`, `-1.0` sentinel).
pub fn calc_ms_energies(
    spec_l: &[i32],
    spec_r: &[i32],
    max_sc_l: &[i32],
    max_sc_r: &[i32],
    offsets: &[u16],
    sfb_active: usize,
    calc_ld: bool,
) -> (Vec<i32>, Vec<i32>, Vec<f64>, Vec<f64>) {
    let mut mid = vec![0i32; sfb_active];
    let mut side = vec![0i32; sfb_active];
    let mut mid_ld = vec![-1.0f64; sfb_active];
    let mut side_ld = vec![-1.0f64; sfb_active];
    for b in 0..sfb_active {
        let min_scale = max_sc_l[b].min(max_sc_r[b]) - 4;
        let min_scale = min_scale.max(0);
        let mut n_mid: i32 = 0;
        let mut n_side: i32 = 0;
        if min_scale > 0 {
            for j in offsets[b] as usize..offsets[b + 1] as usize {
                let l = shift_int(spec_l[j], min_scale - 1);
                let r = shift_int(spec_r[j], min_scale - 1);
                n_mid = fpow2_add_div2(n_mid, l.wrapping_add(r));
                n_side = fpow2_add_div2(n_side, l.wrapping_sub(r));
            }
        } else {
            for j in offsets[b] as usize..offsets[b + 1] as usize {
                let l = shift_int(spec_l[j], -1);
                let r = shift_int(spec_r[j], -1);
                n_mid = fpow2_add_div2(n_mid, l.wrapping_add(r));
                n_side = fpow2_add_div2(n_side, l.wrapping_sub(r));
            }
        }
        // fMin(Nrg, MAXVAL>>1)<<1 with wrap.
        let cap = i32::MAX >> 1;
        mid[b] = n_mid.min(cap).wrapping_shl(1);
        side[b] = n_side.min(cap).wrapping_shl(1);
        if calc_ld {
            let min0 = max_sc_l[b].min(max_sc_r[b]);
            let scale = (0).max(2 * (min0 - 4));
            let f = |v: i32| {
                if v <= 0 {
                    -1.0
                } else {
                    (v as f64 / 2_147_483_648.0).log2() / 64.0
                }
            };
            let mut mld = f(mid[b]);
            let mut sld = f(side[b]);
            if mld != -1.0 {
                mld -= scale as f64 / 64.0;
            }
            if sld != -1.0 {
                sld -= scale as f64 / 64.0;
            }
            mid_ld[b] = mld;
            side_ld[b] = sld;
        }
        // Stored energies shift down unconditionally (psy_main energy lane
        // keeps them; M4 consumes stored ints).
        {
            let min0 = max_sc_l[b].min(max_sc_r[b]);
            let scale = (0).max(2 * (min0 - 4)).min(31);
            mid[b] = shift_int(mid[b], -scale);
            side[b] = shift_int(side[b], -scale);
        }
    }
    (mid, side, mid_ld, side_ld)
}

/// Map float spectrum (stored domain) to Q31 ints by round-to-nearest
/// with saturation. Single approximation point of the integer lane: every
/// subsequent integer op is exact, so mapped values compare with small
/// tolerances while pure-integer state must match exactly.
pub fn map_spectrum_to_q31_into(spectrum: &[f64], mdct_scale: i32, out: &mut Vec<i32>) {
    let factor = 2f64.powi(31 - mdct_scale);
    out.clear();
    out.reserve(spectrum.len());
    out.extend(
        spectrum
            .iter()
            .map(|&v| (v * factor).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32),
    );
}

pub fn map_spectrum_to_q31(spectrum: &[f64], mdct_scale: i32) -> Vec<i32> {
    let mut out = Vec::new();
    map_spectrum_to_q31_into(spectrum, mdct_scale, &mut out);
    out
}

/// Q31-fraction log2: `log2(E/2^31)/64`, the float equivalent of C's
/// `fLog2` up to its ~1e-9 Taylor error (gated by tests). Callers apply
/// C's exact guard/correction rules around it; see `rescale_ld_into`.
fn log2frac(e: i32) -> f64 {
    (e as f64 / 2_147_483_648.0).log2() / 64.0
}

/// Rescale-stage Ld (`CheckBandEnergyOptim` + `psy_main.cpp:379-394`):
/// from the RAW (pre-shift) energies with the `!= -1.0` guard on the
/// scale correction, then the UNCONDITIONAL ldShift (it also lifts the
/// sentinel: silent bands read `-1.0 + final*2/64`, verified on sine f5).
pub fn rescale_ld_into(e_raw: &[i32], maxscale: &[i32], final_shift: i32, out: &mut Vec<f64>) {
    out.clear();
    out.reserve(e_raw.len());
    for (&e, &ms) in e_raw.iter().zip(maxscale.iter()) {
        let mut ld = if e <= 0 {
            -1.0
        } else {
            log2frac(e) - (ms - 4).max(0) as f64 * 2.0 / 64.0
        };
        ld += final_shift as f64 * 2.0 / 64.0;
        out.push(ld);
    }
}

/// (`psy_main.cpp:669-687`). `is_short_element` mirrors the C
/// `isShortWindow[0]` element-global flag. Returns `(nrgShift, finalShift)`.
/// `max_nrg` is the `CheckBandEnergyOptim` return (0 when all zero).
pub fn rescale_shifts(
    max_nrg: i32,
    min_spec_shift: i32,
    is_short_element: bool,
    mdct_scale_ch0: i32,
) -> (i32, i32) {
    const MAX_SHIFT_DBL: i32 = 62;
    let mut nrg_shift = MAX_SHIFT_DBL;
    if max_nrg != 0 {
        nrg_shift = (count_leading_bits(max_nrg) >> 1) + (min_spec_shift - 4);
    }
    if is_short_element {
        nrg_shift -= 1;
    }
    let mut final_shift = min_spec_shift.min(nrg_shift);
    if final_shift > mdct_scale_ch0 + 3 {
        final_shift = mdct_scale_ch0 + 3;
    }
    debug_assert!(final_shift >= 0, "C asserts finalShift >= 0");
    (nrg_shift, final_shift)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::sfb::ld_data;

    #[test]
    fn max_scale_spec_matches_clz_rule() {
        // Single band [0..4): peak 2^20 -> clz 11 -> min(30, 10) = 10.
        let spec = vec![0, 1 << 20, -(1 << 20), 5];
        assert_eq!(max_scale_spec(&spec, &[0, 4], 1), vec![10]);
        // All-zero band saturates at 30.
        assert_eq!(max_scale_spec(&[0, 0], &[0, 2], 1), vec![30]);
        // Full-scale peak leaves no headroom.
        assert_eq!(max_scale_spec(&[0x4000_0000], &[0, 1], 1), vec![0]);
    }

    #[test]
    fn rescale_shifts_mirror_psy_main() {
        // clb(0x200000)=9 -> 9>>1=4; 4+(12-4)=12; final=min(12,12)=12.
        let (nrg, fin) = rescale_shifts(0x0020_0000, 12, false, 9);
        assert_eq!((nrg, fin), (12, 12));
        // Short elements lose one bit: final=min(12,11)=11.
        let (nrg_s, fin_s) = rescale_shifts(0x0020_0000, 12, true, 9);
        assert_eq!((nrg_s, fin_s), (11, 11));
        // Clamp to mdctScale+3.
        let (_, fin_c) = rescale_shifts(0x0020_0000, 20, false, 9);
        assert_eq!(fin_c, 12);
        // All-zero energies: nrgShift stays huge, minSpecShift wins.
        let (_, fin_z) = rescale_shifts(0, 7, false, 9);
        assert_eq!(fin_z, 7);
    }

    #[test]
    fn ld_data_conventions() {
        assert_eq!(ld_data(0.0), -1.0);
        assert!((ld_data(1024.0) - 10.0 / 64.0).abs() < 1e-15);
    }
}
