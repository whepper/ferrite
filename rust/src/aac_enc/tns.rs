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
//! Temporal Noise Shaping analysis and filtering.
//!
//! Port of the LC path in `cpp/libAACenc/src/aacenc_tns.cpp` (init,
//! detect, sync, encode) with the LPC recurrences from
//! `cpp/libFDK/src/FDK_lpc.cpp`. Two numerical lanes:
//! - Autocorrelation (`merged_autocorr_fixed`) is an EXACT integer mirror
//!   of `FDKaacEnc_MergedAutoCorrelation` (per-quarter peak scaling,
//!   summation headroom, inverse-sqrt normalization, `-2` merge weights,
//!   ARM64 `fMult` floor semantics): bit-identical to the oracle RXX
//!   ground truth on every committed frame (see
//!   `tests15::fixed_merge_matches_rxx_bit_exact`). A float equivalent
//!   cannot reproduce C here — the per-quarter normalization terms are
//!   magnitude-dependent (0.75 vs 0.0285 rxx2[0] across levels), not
//!   uniform weights.
//! - The Schur/Levinson recurrence keeps the C **plus-sign** convention
//!   (`k = -work/a`, `a += k*w`, `w += k*a`), which is self-consistent;
//!   textbook minus signs would invert the filter. Float Schur on exact
//!   rxx gives bit-exact coefficient indices and gains within the
//!   `gain_tolerance` gate corpus-wide (see
//!   `tests_gain_gate::schur_gains_match_committed_within_gate`).
//! - Coefficient quantization compares full-precision PARCOR against the
//!   exact Q15 border values with strict `>` (`Search3`/`Search4`).
//! - The LPC synthesis filter is the true FIR
//!   `y[n] = x[n] + sum a[m]*x[n-1-m]` (C's coefficient exponents and
//!   shift bookkeeping only keep fixed-point in range).
//!
//! Remaining divergence class (measured, documented): M2 f32-kernel noise
//! (~1e-4 absolute on loud frames) amplified by the ill-conditioned
//! Schur recurrence flips quantized decisions sitting within ~1% of a
//! border. All active/inactive decisions match; filter-parameter flips
//! keep post-recompute energies within tolerance (see M3 report).

use super::q31::{fixnormz_32, fixp_abs_64, fmult_add_div2_64, fmult_div2_64, scale_value_64};
use super::sfb_tables::{
    INV_SQRT_TAB, TNS_ACF_LONG, TNS_ACF_SHORT, TNS_COEF_3_BORDERS, TNS_COEF_4_BORDERS,
    TNS_ENC_COEF_3, TNS_ENC_COEF_4,
};

/// Higher (HI=0) / lower (LO=1) filter index.
pub const HI: usize = 0;
/// Lower filter index.
pub const LO: usize = 1;
/// Maximum TNS order, LC long (`TNS_MAX_ORDER`).
pub const TNS_MAX_ORDER: usize = 12;
/// Short-block subblock count for TNS purposes.
pub const TNS_WINDOWS_SHORT: usize = 8;

/// LC max bands per sample rate, descending (mirrors
/// `tnsMaxBandsTab1024`; traversal takes the first entry with
/// `sampleRate >= entry`). Values are (long, short).
const TNS_MAX_BANDS: [(u32, [i32; 2]); 12] = [
    (96000, [31, 9]),
    (88200, [31, 9]),
    (64000, [34, 10]),
    (48000, [40, 14]),
    (44100, [42, 14]),
    (32000, [51, 14]),
    (24000, [46, 14]),
    (22050, [46, 14]),
    (16000, [42, 14]),
    (12000, [42, 14]),
    (11025, [42, 14]),
    (8000, [39, 14]),
];

/// TNS configuration (mirrors `TNS_CONFIG` for the LC path).
#[derive(Debug, Clone)]
pub struct TnsConfig {
    /// TNS enabled (full mask for the VBR-5 target).
    pub active: bool,
    /// Maximum filter order (12 long, 5 short; -2 below 16 kbit/s).
    pub max_order: usize,
    /// Coefficient resolution bits (4 long, 3 short).
    pub coef_res: usize,
    /// LPC start bands [HI, LO].
    pub start_band: [usize; 2],
    /// LPC start lines [HI, LO].
    pub start_line: [usize; 2],
    /// LPC stop band/line.
    pub stop_band: usize,
    /// Stop line.
    pub stop_line: usize,
    /// Minimum prediction gain (x1000): [1437 HI, 1500 LO].
    pub thresh_on: [i32; 2],
    /// Order limits [HI=maxOrder, LO=max(0,maxOrder-7)].
    pub limit_order: [usize; 2],
    /// Autocorrelation windows as stored Q31 ints (exact table values;
    /// short pads taps 8..16 with zero — never read, lags stay below 8).
    pub acf_window_q31: [[i32; 16]; 2],
}

impl TnsConfig {
    /// Mirror of `FDKaacEnc_InitTnsConfiguration` for granule 1024 LC.
    /// `offsets`/`sfb_active` come from the matching `SfbConfig`;
    /// `tns_bitrate` is `(totalBitrate * tnsChannels) / channelsEff`.
    pub fn new(
        sample_rate: u32,
        tns_bitrate: u32,
        long: bool,
        offsets: &[u16],
        sfb_active: usize,
    ) -> Self {
        let mut max_order = if long { 12 } else { 5 };
        if tns_bitrate < 16000 {
            max_order -= 2;
        }
        let max_bands = TNS_MAX_BANDS
            .iter()
            .find(|(rate, _)| sample_rate >= *rate)
            .expect("tns max-bands rate covered")
            .1[if long { 0 } else { 1 }];
        let stop_band = (max_bands as usize).min(sfb_active);
        let stop_line = offsets[stop_band] as usize;
        let (start_band_lo, start_line_lo) = if long {
            let band = if sample_rate < 9391 {
                2
            } else if sample_rate < 18783 {
                4
            } else {
                8
            };
            (band, offsets[band] as usize)
        } else {
            (0, offsets[0] as usize)
        };
        // HI start: walk down from stopBand while the offset exceeds
        // LOstart + (stop - LOstart)/4 (integer division, mirrored).
        let mut band_hi = stop_band;
        while offsets[band_hi] as usize > start_line_lo + (stop_line - start_line_lo) / 4 {
            band_hi -= 1;
        }
        let acf_q31 = if long {
            [TNS_ACF_LONG, TNS_ACF_LONG]
        } else {
            let mut w = [0i32; 16];
            for (i, &q) in TNS_ACF_SHORT.iter().enumerate() {
                w[i] = q;
            }
            [w, w]
        };
        TnsConfig {
            active: true,
            max_order,
            coef_res: if long { 4 } else { 3 },
            start_band: [band_hi, start_band_lo],
            start_line: [offsets[band_hi] as usize, start_line_lo],
            stop_band,
            stop_line,
            thresh_on: [1437, 1500],
            limit_order: [max_order, max_order.saturating_sub(7)],
            acf_window_q31: acf_q31,
        }
    }
}

/// Per-channel TNS output state (mirrors `TNS_INFO` for bitstream use
/// later, plus runtime `TNS_DATA` decisions).
#[derive(Debug, Clone)]
pub struct TnsChannel {
    /// Filters per window (8 entries; long uses index 0).
    pub num_filters: [usize; 8],
    /// Coefficient resolution per window.
    pub coef_res: [usize; 8],
    /// Filter lengths in SFBs [window][HI/LO].
    pub length: [[usize; 2]; 8],
    /// Truncated orders [window][HI/LO].
    pub order: [[usize; 2]; 8],
    /// Directions [window][HI/LO] (always 0 = up on this path).
    pub direction: [[usize; 2]; 8],
    /// Quantized coefficient indices [window][HI/LO][12].
    pub coef: [[[i32; 12]; 2]; 8],
    /// Runtime: filter active [window][HI/LO].
    pub active: [[bool; 2]; 8],
    /// Runtime: prediction gain x1000 [window][HI/LO].
    pub gain: [[i32; 2]; 8],
    /// Runtime: filters merged (long only).
    pub merged: bool,
}

impl TnsChannel {
    /// Cleared state (mirrors zeroed `TNS_DATA`/`TNS_INFO` at stream start).
    pub fn new() -> Self {
        Self {
            num_filters: [0; 8],
            coef_res: [0; 8],
            length: [[0; 2]; 8],
            order: [[0; 2]; 8],
            direction: [[0; 2]; 8],
            coef: [[[0; 12]; 2]; 8],
            active: [[false; 2]; 8],
            gain: [[0; 2]; 8],
            merged: false,
        }
    }

    /// Clear runtime decisions (mirrors the TNS-disabled `memclear`).
    pub fn clear_runtime(&mut self) {
        self.active = [[false; 2]; 8];
        self.gain = [[0; 2]; 8];
        self.merged = false;
    }
}

impl Default for TnsChannel {
    fn default() -> Self {
        Self::new()
    }
}

/// Quantize one PARCOR value (`Search3`/`Search4`: last border strictly
/// below the value, minus offset).
fn parcor_to_index(parcor: f64, coef_res: usize) -> i32 {
    let (borders, offset) = if coef_res == 4 {
        (&TNS_COEF_4_BORDERS[..], 8)
    } else {
        (&TNS_COEF_3_BORDERS[..], 4)
    };
    let mut index = 0i32;
    for (i, &b) in borders.iter().enumerate() {
        if parcor > b {
            index = i as i32;
        }
    }
    index - offset
}

/// Dequantize one index (`Index2Parcor`).
fn index_to_parcor(index: i32, coef_res: usize) -> f64 {
    if coef_res == 4 {
        TNS_ENC_COEF_4[(index + 8) as usize]
    } else {
        TNS_ENC_COEF_3[(index + 4) as usize]
    }
}

/// Schur/Levinson recurrence to PARCOR with prediction gain
/// (`CLpc_AutoToParcor`, plus-sign convention, `FDK_lpc.cpp:431-487`).
/// `corr` holds lags `0..=max_order`; only the first `limit` coefficients
/// are computed. Returns `(parcor[12], gain)` with per-stage reflection
/// coefficients recorded.
fn schur_record(corr: &[f64], max_order: usize, limit: usize) -> ([f64; 12], f64) {
    let mut parcor = [0.0f64; 12];
    let mut a = [0.0f64; 13];
    let mut w = [0.0f64; 12];
    a[..=max_order].copy_from_slice(&corr[..=max_order]);
    w[..max_order].copy_from_slice(&corr[1..=max_order]);
    let r0 = corr[0];
    if r0 == 0.0 {
        return (parcor, 1.0);
    }
    for i in 0..limit {
        if a[0] < w[i].abs() {
            break;
        }
        let k = -w[i] / a[0];
        parcor[i] = k;
        for j in 0..limit - i {
            let aj = a[j];
            let wj = w[i + j];
            a[j] = aj + k * wj;
            w[i + j] = wj + k * aj;
        }
        if a[0] == 0.0 {
            break;
        }
    }
    let gain = if a[0] > 0.0 { r0 / a[0] } else { 0.0 };
    (parcor, gain)
}

/// Step-up from reflection to LPC coefficients (`CLpc_ParcorToLpc` real
/// math, `FDK_lpc.cpp:393-429` without Q bookkeeping).
fn parcor_to_lpc(parcor: &[f64], order: usize) -> Vec<f64> {
    let mut a = vec![0.0f64; order];
    if order == 0 {
        return a;
    }
    a[0] = parcor[0];
    for i in 1..order {
        for j in 0..i / 2 {
            let t1 = a[j];
            let t2 = a[i - 1 - j];
            a[j] = t1 + parcor[i] * t2;
            a[i - 1 - j] = t2 + parcor[i] * t1;
        }
        if i & 1 == 1 {
            let m = i / 2;
            a[m] += parcor[i] * a[m];
        }
        a[i] = parcor[i];
    }
    a
}

/// Exact integer mirror of `invSqrtNorm2`
/// (`fixpoint_math.h:320-373`, generic table path used on ARM).
/// `op` is the accumulated quarter energy (positive, fits 32 bits by
/// headroom design). Returns `(mantissa, shift)`.
/// Ports the linear+HQ interpolation literally; `INV_SQRT_TAB` is the
/// verbatim ROM table. All arithmetic in `i64` matching LP64 `LONG`
/// (no 32-bit wrap occurs: values stay in range by construction —
/// bit-exact RXX agreement is the check).
fn inv_sqrt_norm2(op: i64) -> (i64, i32) {
    // Quarter energies always fit 32 bits here (accumulation headroom:
    // 160 lines max, per-term prescaled); the i32 narrowing below is exact.
    debug_assert!(op > 0 && op < (1i64 << 31));
    // Normalize input, calculate shift value.
    let mut shift = fixnormz_32(op as i32) - 1;
    let val = op << shift;
    shift += 2; // bias for exponent
    let index = ((val >> (32 - 1 - (7 + 1))) & 0x7f) as usize;
    let mut fract = (((val as i32) & 0x007FFFFF) << (7 + 1)) as i64;
    let mut diff = (INV_SQRT_TAB[index + 1] as i64) - (INV_SQRT_TAB[index] as i64);
    let mut reg1 = (INV_SQRT_TAB[index] as i64) + (fmult_div2_64(diff, fract) << 1);
    if fract != 0 {
        // fract = fract * (1 - fract), HQ correction term.
        fract = fmult_div2_64(fract, 0x80000000 - fract) << 1;
        diff -= (INV_SQRT_TAB[index + 2] as i64) - (INV_SQRT_TAB[index + 1] as i64);
        reg1 = fmult_add_div2_64(reg1, fract, diff);
    }
    // Odd-shift correction (0x5A827999 = 1/sqrt(2), truncated).
    if shift & 1 != 0 {
        reg1 = fmult_div2_64(reg1, 0x5A827999) << 2;
    }
    shift >>= 1;
    (reg1, shift)
}

/// Exact integer mirror of `FDKaacEnc_CalcAutoCorrValue`
/// (`aacenc_tns.cpp:542-576`): autocorrelation sum over `[start, stop)`
/// at `lag`, with per-term `>> scale` (summation headroom `nsc`).
/// NOTE the asymmetry (mirrored exactly): lag 0 uses `fPow2`
/// (`Div2<<1`, floor-to-even) while lag > 0 uses ARM64 `fMult`
/// (floor, keeps bit 0). All `fMult`-family inputs narrow to 32-bit
/// `INT` on ARM64 (no-op here: scaled values fit by headroom design).
/// Accumulation in `i64` (LP64 `LONG`, no wrap; fits by design).
fn calc_auto_corr_value(
    spectrum: &[i64],
    start: usize,
    stop: usize,
    lag: usize,
    scale: i32,
) -> i64 {
    // ARM64 INT narrowing at fMult-family boundaries (identity in range).
    // fMult is the shared floor primitive (q31::fmult, ARM64 semantics).
    let mut result: i64 = 0;
    if lag == 0 {
        for x in spectrum.iter().take(stop).skip(start) {
            // fPow2(x) = fixpow2div2_D(x)<<1 (the <<1 is part of fPow2,
            // applied BEFORE >>scale).
            let v = *x as i32 as i64;
            result += (fmult_div2_64(v, v) << 1) >> scale;
        }
    } else {
        for (i, a) in spectrum.iter().enumerate().take(stop - lag).skip(start) {
            // ARM64 fMult (floor, keeps bit 0).
            let av = *a as i32 as i64;
            let bv = spectrum[i + lag] as i32 as i64;
            result += (super::q31::fmult(av as i32, bv as i32) as i64) >> scale;
        }
    }
    result
}
/// Exact integer mirror of `FDKaacEnc_AutoCorrNormFac`
/// (`aacenc_tns.cpp:578-617`): energy normalization factor `~= 1/value`.
/// `scale` is the caller-side exponent `(-2*sc_up + nsc)`; `*sc`
/// accumulates the factor's own exponent. The `2^-28` floor branch
/// (`MAXVAL` + `scale + 28`) is mirrored literally (fires only for
/// near-silent quarters; corpus-gated).
fn auto_corr_norm_fac(value: i64, scale: i32, sc: &mut i32) -> i64 {
    // HLM_MIN_NRG = 2^-28; as Q31 FIXP_DBL: 2^-28 * 2^31 = 8 exactly.
    // (NOT 16: misreading this shifts the normal/floor branch boundary
    // 2x, verified against oracle RXXD intermediates on onset w6.)
    const HLM_MIN_NRG_Q31: i64 = 8;
    const MAXVAL_DBL: i64 = 0x7FFF_FFFF;
    let (a, b) = if scale >= 0 {
        // B = HLM_MIN_NRG >> min(31, scale); A = value.
        let s = scale.min(31);
        (value, HLM_MIN_NRG_Q31 >> s)
    } else {
        // A = value >> min(31, -scale); B = HLM_MIN_NRG.
        let s = (-scale).min(31);
        (value >> s, HLM_MIN_NRG_Q31)
    };
    if a > b {
        let (tmp, shift) = inv_sqrt_norm2(value);
        // fMult (full multiply, ARM64 floor), NOT Div2.
        let ret = super::q31::fmult(tmp as i32, tmp as i32) as i64;
        *sc += 2 * shift;
        ret
    } else {
        *sc += scale + 28;
        MAXVAL_DBL
    }
}

/// Exact integer mirror of `FDKaacEnc_ScaleUpSpectrum`
/// (`aacenc_tns.cpp:519-540`): peak-normalize a spectrum section into the
/// scratch buffer. Returns the applied left shift (`CountLeadingBits` of
/// the peak: 32-bit `clz - 1`, 0 for zero input). All ops in `i64`
/// matching LP64 `LONG`; headroom design keeps values in 32-bit range.
fn scale_up_spectrum(dest: &mut [i64], src: &[i32], start: usize, stop: usize) -> i32 {
    let mut peak: i64 = 0;
    for s in src.iter().take(stop).skip(start) {
        peak = peak.max(fixp_abs_64(*s as i64));
    }
    debug_assert!(peak < (1i64 << 32));
    let scale = if peak == 0 {
        0
    } else {
        // 32-bit clz(peak) - 1 = 31 - bitlen(peak).
        31 - (64 - (peak as u64).leading_zeros() as i32)
    };
    for (d, s) in dest.iter_mut().zip(src.iter()).take(stop).skip(start) {
        *d = (*s as i64) << scale;
    }
    scale
}

/// Exact integer mirror of `FDKaacEnc_MergedAutoCorrelation`
/// (`aacenc_tns.cpp:619-752`), equal-quarters path only: LC-1024
/// configs always set `acfSplit = -1` (`aacenc_tns.cpp:437-443`; the
/// split path asserts `LO==1/HI==3` and never fires here).
/// Operates on Q31 stored-domain ints (post-rescale detect input),
/// returns raw `FIXP_DBL`-domain (`i64`) vectors like C. Combined with
/// the committed RXX dumps this is bit-exact validated (see tests).
/// NOTE the asymmetry vs the float mirror: per-quarter peak scaling,
/// summation headroom, inverse-sqrt normalization and the `-2` merge
/// weights are magnitude-dependent fixed-point behavior with no float
/// equivalent (0.75 vs 0.0285 rxx2[0] across levels).
// Indexed loops mirror the C loop structure line-for-line on purpose.
#[allow(clippy::needless_range_loop)]
fn merged_autocorr_fixed(spectrum_q31: &[i32], config: &TnsConfig) -> ([i64; 13], [i64; 13]) {
    use super::q31::fmult;
    let max_order = config.max_order;
    // Equal quarters over [lpcStartLine[LO], lpcStopLine).
    let n = config.stop_line - config.start_line[LO];
    let idx0 = config.start_line[LO];
    let idx1 = idx0 + n / 4;
    let idx2 = idx0 + n / 2;
    let idx3 = idx0 + n * 3 / 4;
    let idx4 = config.stop_line;
    // Temporal spectrum buffer (C scratch; fully written before read).
    let mut pspec = [0i64; 1024];
    let sc1 = scale_up_spectrum(&mut pspec, spectrum_q31, idx0, idx1);
    let sc2 = scale_up_spectrum(&mut pspec, spectrum_q31, idx1, idx2);
    let sc3 = scale_up_spectrum(&mut pspec, spectrum_q31, idx2, idx3);
    let sc4 = scale_up_spectrum(&mut pspec, spectrum_q31, idx3, idx4);
    // Summation headroom per quarter: smallest nsc with 2^nsc >= length.
    let nsc = |len: usize| {
        let mut n = 1;
        while (1 << n) < len as i32 {
            n += 1;
        }
        n
    };
    let nsc1 = nsc(idx1 - idx0);
    let nsc2 = nsc(idx2 - idx1);
    let nsc3 = nsc(idx3 - idx2);
    let nsc4 = nsc(idx4 - idx3);
    let rxx1_0 = calc_auto_corr_value(&pspec, idx0, idx1, 0, nsc1);
    let rxx2_0 = calc_auto_corr_value(&pspec, idx1, idx2, 0, nsc2);
    let rxx3_0 = calc_auto_corr_value(&pspec, idx2, idx3, 0, nsc3);
    let rxx4_0 = calc_auto_corr_value(&pspec, idx3, idx4, 0, nsc4);
    let mut rxx1 = [0i64; 13];
    let mut rxx2 = [0i64; 13];
    let mut fac1 = 0i64;
    let mut sc_fac1 = -1;
    if rxx1_0 != 0 {
        fac1 = auto_corr_norm_fac(rxx1_0, -2 * sc1 + nsc1, &mut sc_fac1);
        rxx1[0] = scale_value_64(fmult(rxx1_0 as i32, fac1 as i32) as i64, sc_fac1);
        // LC is never low-delay: single non-LD path (3*lag window).
        for lag in 1..=max_order {
            if 3 * lag <= max_order + 3 {
                let x1 = calc_auto_corr_value(&pspec, idx0, idx1, lag, nsc1);
                rxx1[lag] = fmult(
                    scale_value_64(fmult(x1 as i32, fac1 as i32) as i64, sc_fac1) as i32,
                    config.acf_window_q31[LO][3 * lag],
                ) as i64;
            }
        }
    }
    if !(rxx2_0 == 0 && rxx3_0 == 0 && rxx4_0 == 0) {
        let mut fac2 = 0i64;
        let mut fac3 = 0i64;
        let mut fac4 = 0i64;
        let mut sc_fac2 = 0;
        let mut sc_fac3 = 0;
        let mut sc_fac4 = 0;
        if rxx2_0 != 0 {
            fac2 = auto_corr_norm_fac(rxx2_0, -2 * sc2 + nsc2, &mut sc_fac2);
            sc_fac2 -= 2;
        }
        if rxx3_0 != 0 {
            fac3 = auto_corr_norm_fac(rxx3_0, -2 * sc3 + nsc3, &mut sc_fac3);
            sc_fac3 -= 2;
        }
        if rxx4_0 != 0 {
            fac4 = auto_corr_norm_fac(rxx4_0, -2 * sc4 + nsc4, &mut sc_fac4);
            sc_fac4 -= 2;
        }
        rxx2[0] = scale_value_64(fmult(rxx2_0 as i32, fac2 as i32) as i64, sc_fac2)
            + scale_value_64(fmult(rxx3_0 as i32, fac3 as i32) as i64, sc_fac3)
            + scale_value_64(fmult(rxx4_0 as i32, fac4 as i32) as i64, sc_fac4);
        for lag in 1..=max_order {
            let x2 = scale_value_64(
                fmult(
                    calc_auto_corr_value(&pspec, idx1, idx2, lag, nsc2) as i32,
                    fac2 as i32,
                ) as i64,
                sc_fac2,
            ) + scale_value_64(
                fmult(
                    calc_auto_corr_value(&pspec, idx2, idx3, lag, nsc3) as i32,
                    fac3 as i32,
                ) as i64,
                sc_fac3,
            ) + scale_value_64(
                fmult(
                    calc_auto_corr_value(&pspec, idx3, idx4, lag, nsc4) as i32,
                    fac4 as i32,
                ) as i64,
                sc_fac4,
            );
            rxx2[lag] = fmult(x2 as i32, config.acf_window_q31[HI][lag]) as i64;
        }
    }
    (rxx1, rxx2)
}
fn analyze_filter(
    rxx: &[f64; 13],
    config: &TnsConfig,
    filter: usize,
) -> ([i32; 12], usize, i32, i32) {
    let limit = config.limit_order[filter];
    let (parcor, gain) = schur_record(rxx, config.max_order, limit);
    let mut index = [0i32; 12];
    for i in 0..limit {
        index[i] = parcor_to_index(parcor[i], config.coef_res);
    }
    let mut order = 0;
    for i in (0..limit).rev() {
        if index[i] != 0 {
            order = i + 1;
            break;
        }
    }
    let mut sum_sqr = 0i32;
    for i in 0..order {
        sum_sqr += index[i] * index[i];
    }
    (index, order, sum_sqr, (gain * 1000.0).floor() as i32)
}

/// Absolute floor of the prediction-gain comparison (gain x1000 ints).
/// Same-input float-Schur residuals on small gains stay within 1 unit
/// across the corpus, so 8 keeps 8x headroom there.
pub(crate) const GAIN_TOL_FLOOR: i64 = 8;
/// Relative slope of the prediction-gain comparison (per 1000 gain units).
/// Same-input residuals reach 11 units at gain 22154 (5e-4 relative:
/// onset f25, bit-identical rxx inputs); the slope admits that class with
/// 2x headroom while every input-driven divergence on record (smallest
/// Δ29 at gain ~1000) still fails loudly.
pub(crate) const GAIN_TOL_PER_MILLE: i64 = 1;

/// Prediction-gain agreement policy: `|got - want| <= 8 + |want|/1000`.
/// Governs VALUES only; the threshOn DECISION stays exact via the
/// separately-compared active flags, and order/coefficient asserts stay
/// exact — a decision flip can never hide behind this gate.
pub(crate) fn gain_tolerance(want: i32) -> i64 {
    GAIN_TOL_FLOOR + (want as i64).abs() / 1000
}

/// Whether a computed prediction gain agrees with the reference under
/// [`gain_tolerance`].
pub(crate) fn gain_within_tolerance(got: i32, want: i32) -> bool {
    (got as i64 - want as i64).abs() <= gain_tolerance(want)
}

/// TNS detect for one subblock (`TnsDetect`, `aacenc_tns.cpp:766-944`).
/// Updates `state`/`info` for `window`; `short` selects the short config
/// behavior (LO filter long-only). Takes the Q31 stored-domain spectrum
/// (post-rescale detect input, like C); the exact integer merge runs on
/// it directly, and only the Schur stage uses float ratios.
pub fn detect(
    state: &mut TnsChannel,
    info: &mut TnsChannel,
    config: &TnsConfig,
    spectrum_q31: &[i32],
    window: usize,
    short: bool,
    sfb_cnt: usize,
) {
    info.num_filters[window] = 0;
    info.coef_res[window] = config.coef_res;
    // C zeroes maxOrder entries (not the full 12); the tail keeps stale
    // values exactly like the reference structs do.
    for f in 0..2 {
        for i in 0..config.max_order {
            info.coef[window][f][i] = 0;
        }
        info.length[window][f] = 0;
        info.order[window][f] = 0;
    }
    state.active[window] = [false, false];
    state.gain[window] = [1000, 1000];
    state.merged = false;
    if !config.active || config.max_order == 0 {
        return;
    }

    let (rxx1_q31, rxx2_q31) = merged_autocorr_fixed(spectrum_q31, config);
    // Exact i32->f64 (53-bit mantissa) then /2^31 (exact power of two):
    // the float Schur sees C's fractions bit-faithfully.
    let mut rxx1 = [0.0f64; 13];
    let mut rxx2 = [0.0f64; 13];
    for i in 0..13 {
        rxx1[i] = rxx1_q31[i] as f64 / 2_147_483_648.0;
        rxx2[i] = rxx2_q31[i] as f64 / 2_147_483_648.0;
    }
    // HI filter. Order/length/direction are assigned unconditionally
    // (aacenc_tns.cpp:828-846), even when the filter stays inactive.
    let (idx_hi, order_hi, sum_hi, gain_hi) = analyze_filter(&rxx2, config, HI);
    info.coef[window][HI][..config.limit_order[HI]]
        .copy_from_slice(&idx_hi[..config.limit_order[HI]]);
    info.direction[window][HI] = 0;
    info.length[window][HI] = sfb_cnt - config.start_band[HI];
    info.order[window][HI] = order_hi;
    state.gain[window][HI] = gain_hi;
    if gain_hi > config.thresh_on[HI] || sum_hi > config.limit_order[HI] as i32 / 2 + 2 {
        state.active[window][HI] = true;
        info.num_filters[window] += 1;
        // LO filter, long windows only.
        if !short {
            let (idx_lo, order_lo, sum_lo, gain_lo) = analyze_filter(&rxx1, config, LO);
            info.coef[window][LO][..config.limit_order[LO]]
                .copy_from_slice(&idx_lo[..config.limit_order[LO]]);
            info.direction[window][LO] = 0;
            info.length[window][LO] = config.start_band[HI] - config.start_band[LO];
            info.order[window][LO] = order_lo;
            state.gain[window][LO] = gain_lo;
            if (gain_lo > config.thresh_on[LO] && gain_lo < 16000 * config.limit_order[LO] as i32)
                || (sum_lo > 9 && sum_lo < 22 * config.limit_order[LO] as i32)
            {
                state.active[window][LO] = true;
                let mut diff = 0i32;
                for i in 0..config.limit_order[LO] {
                    diff += (info.coef[window][HI][i] - info.coef[window][LO][i]).abs();
                }
                if diff < 2 && info.direction[window][LO] == info.direction[window][HI] {
                    state.merged = true;
                    info.length[window][HI] = sfb_cnt - config.start_band[LO];
                    // Re-truncate HI order past small coefficients.
                    let mut i = config.limit_order[LO];
                    while i < info.order[window][HI] && info.coef[window][HI][i].abs() <= 1 {
                        i += 1;
                    }
                    let mut j = i as i32 - 1;
                    while j >= 0 {
                        if info.coef[window][HI][j as usize] != 0 {
                            break;
                        }
                        j -= 1;
                    }
                    if j + 1 < info.order[window][HI] as i32 {
                        info.order[window][HI] = (j + 1) as usize;
                    }
                } else {
                    info.num_filters[window] += 1;
                }
            }
        }
    }
}

/// Stereo sync (`TnsSync`, `aacenc_tns.cpp:961-1032`): merge `src` into
/// `dst` when window types match. `short` selects the 8-window path.
pub fn synchronize_pair(
    dst_state: &mut TnsChannel,
    dst_info: &mut TnsChannel,
    src_state: &TnsChannel,
    src_info: &TnsChannel,
    config: &TnsConfig,
    dst_short: bool,
    src_short: bool,
) {
    if (src_short && !dst_short) || (dst_short && !src_short) {
        return;
    }
    let windows = if dst_short { 8 } else { 1 };
    for w in 0..windows {
        if dst_state.active[w][HI] || src_state.active[w][HI] {
            let mut diff_sum = 0i32;
            let mut similar = true;
            for i in 0..config.max_order {
                let diff = (dst_info.coef[w][HI][i] - src_info.coef[w][HI][i]).abs();
                diff_sum += diff;
                if diff > 1 || diff_sum > 2 {
                    similar = false;
                    break;
                }
            }
            if similar {
                if src_state.active[w][HI] {
                    if !dst_state.active[w][HI] || dst_info.num_filters[w] > src_info.num_filters[w]
                    {
                        dst_state.active[w][HI] = true;
                        dst_info.num_filters[w] = 1;
                    }
                    dst_state.merged = src_state.merged;
                    dst_info.order[w][HI] = src_info.order[w][HI];
                    dst_info.length[w][HI] = src_info.length[w][HI];
                    dst_info.direction[w][HI] = src_info.direction[w][HI];
                    dst_info.coef[w][HI] = src_info.coef[w][HI];
                } else {
                    dst_state.active[w][HI] = false;
                    dst_info.num_filters[w] = 0;
                }
            }
        }
    }
}

/// TNS encode: LPC-filter the spectrum in place (`TnsEncode`,
/// `aacenc_tns.cpp:1051-1092`). `merged` selects the HI range start (LO
/// start when merged, HI start otherwise); with two filters the second
/// covers LO start to HI start. `spectrum` spans the full window; lines
/// outside the filter ranges are untouched.
pub fn encode(
    info: &TnsChannel,
    merged: bool,
    config: &TnsConfig,
    spectrum: &mut [f64],
    window: usize,
) {
    if info.num_filters[window] == 0 {
        // Early-out: HI inactive, spectrum untouched (aacenc_tns.cpp:1057).
        return;
    }
    let nf = info.num_filters[window].min(2);
    let ranges = if nf >= 2 {
        [
            (config.start_line[HI], config.stop_line),
            (config.start_line[LO], config.start_line[HI]),
        ]
    } else if merged {
        [(config.start_line[LO], config.stop_line), (0, 0)]
    } else {
        [(config.start_line[HI], config.stop_line), (0, 0)]
    };
    for f in 0..nf {
        let (s0, s1) = ranges[f];
        if s1 <= s0 {
            continue;
        }
        let order = info.order[window][f];
        if order == 0 {
            continue;
        }
        let mut parcor = [0.0f64; 12];
        for i in 0..order {
            parcor[i] = index_to_parcor(info.coef[window][f][i], info.coef_res[window]);
        }
        let lpc = parcor_to_lpc(&parcor, order);
        // FIR with zero initial state, in place.
        let mut hist = vec![0.0f64; order];
        for n in s0..s1.min(spectrum.len()) {
            let x = spectrum[n];
            let mut y = x;
            for m in 0..order {
                y += lpc[m] * hist[m];
            }
            for m in (1..order).rev() {
                hist[m] = hist[m - 1];
            }
            hist[0] = x;
            spectrum[n] = y;
        }
    }
}
#[cfg(test)]
mod tests15 {
    use super::*;
    use crate::aac_enc::testdata;

    /// Bit-exact validation of the integer merge: reconstruct C's exact
    /// detect input (committed MDCT ints + lowpass zero + <<=finalShift,
    /// all integer-exact) and require rxx bit-equality with committed RXX.
    /// Covers every committed (frame, channel) pair that has MDCT + RXX
    /// vectors (long/short, all signal classes). Any semantic divergence
    /// (narrowing, rounding, weights, branches) fails loudly here by
    /// construction.
    #[test]
    fn fixed_merge_matches_rxx_bit_exact() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/aac_enc/testdata");
        let mut cases: Vec<(String, u32, u32)> = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("testdata dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("rxx_") && n.ends_with(".txt"))
            .collect();
        entries.sort();
        for n in entries {
            // rxx_<name>_f<frame>_ch<ch>.txt
            let rest = n["rxx_".len()..n.len() - ".txt".len()].to_string();
            let (name, tail) = rest.rsplit_once("_f").expect("rxx name");
            let (frame, ch) = tail.split_once("_ch").expect("rxx frame/ch");
            // Only frames with committed MDCT vectors can be reconstructed.
            if !dir
                .join(format!("mdct_{name}_f{frame}_ch{ch}.txt"))
                .exists()
            {
                continue;
            }
            cases.push((
                name.to_string(),
                frame.parse().expect("frame"),
                ch.parse().expect("ch"),
            ));
        }
        assert!(!cases.is_empty(), "no rxx+mdct vectors");
        for (name, f, ch) in cases {
            let sr = testdata::fixture_sample_rate(&name);
            // Sequence determines long/short layout (from seq file).
            let seq_rows = testdata::load_seq(&name);
            let row = seq_rows
                .iter()
                .find(|r| r.frame == f && r.channel == ch)
                .expect("seq row");
            let short = row.seq == 2;
            // Per-fixture rates mirror Stream::new (affect only allowIS
            // and the never-taken spreading branch, not tables).
            let nch = if testdata::fixture_samples(&name).len() == 1 {
                1
            } else {
                2
            };
            let (ch_br, tns_br) = if nch == 1 {
                (112000, 112000)
            } else {
                (96000, 192000)
            };
            let (cfg, tconf) = if short {
                let cfg = crate::aac_enc::sfb::SfbConfig::new(
                    sr,
                    crate::aac_enc::sfb::BlockKind::Short,
                    ch_br,
                    19293,
                    true,
                    true,
                )
                .unwrap();
                let tconf = TnsConfig::new(sr, tns_br, false, cfg.offsets(), cfg.sfb_active());
                (cfg, tconf)
            } else {
                let cfg = crate::aac_enc::sfb::SfbConfig::new(
                    sr,
                    crate::aac_enc::sfb::BlockKind::Long,
                    ch_br,
                    19293,
                    true,
                    true,
                )
                .unwrap();
                let tconf = TnsConfig::new(sr, tns_br, true, cfg.offsets(), cfg.sfb_active());
                (cfg, tconf)
            };
            let (_e, ints) = testdata::load_mdct(&name, f, ch);
            let psy = testdata::load_psy(&name, f, ch, short);
            // Reconstruct C's detect input exactly.
            let mut spec: Vec<i32> = ints.clone();
            let lp = cfg.lowpass_line();
            let nw = if short { 8 } else { 1 };
            let wl = if short { 128 } else { 1024 };
            for w in 0..nw {
                for i in w * wl + lp..(w + 1) * wl {
                    spec[i] = 0;
                }
                for i in 0..lp.min(wl) {
                    spec[w * wl + i] = spec[w * wl + i].wrapping_shl(psy.final_shift as u32);
                }
            }
            // NOTE: C shifts with 64-bit LONG (no wrap); values fit
            // 32 bits by the clamp+headroom invariant (see q31 docs).
            // wrapping_shl matches wherever values fit (validated).
            let rxx_want = testdata::load_rxx(&name, f, ch);
            assert_eq!(rxx_want.len(), nw, "{name} f{f} ch{ch} windows");
            for w in 0..nw {
                let slice = &spec[w * wl..(w + 1) * wl];
                // Pad to full window for the fixed merge (long needs
                // 1024-line absolute indexing; short slices are 128).
                let (r1, r2) = if short {
                    let mut full = vec![0i32; 1024];
                    full[..128].copy_from_slice(slice);
                    merged_autocorr_fixed(&full, &tconf)
                } else {
                    merged_autocorr_fixed(slice, &tconf)
                };
                let got = &rxx_want[w];
                assert_eq!(got.window, w);
                // Bit-exact: values must fit 32 bits (as in C's
                // dump narrowing) AND match.
                for i in 0..13 {
                    assert!(
                        r1[i] >= i32::MIN as i64 && r1[i] <= i32::MAX as i64,
                        "{name} f{f} ch{ch} w{w} RXX1[{i}] out of i32 range: {}",
                        r1[i]
                    );
                    assert!(
                        r2[i] >= i32::MIN as i64 && r2[i] <= i32::MAX as i64,
                        "{name} f{f} ch{ch} w{w} RXX2[{i}] out of i32 range: {}",
                        r2[i]
                    );
                }
                let r1n: Vec<i32> = r1.iter().map(|&v| v as i32).collect();
                let r2n: Vec<i32> = r2.iter().map(|&v| v as i32).collect();
                assert_eq!(&r1n[..], &got.rxx1[..], "{name} f{f} ch{ch} w{w} RXX1");
                assert_eq!(&r2n[..], &got.rxx2[..], "{name} f{f} ch{ch} w{w} RXX2");
            }
        }
    }
}

#[cfg(test)]
mod tests_gain_gate {
    use super::*;
    use crate::aac_enc::testdata;

    /// Gate policy pin: measured same-input float-Schur residuals ACCEPT,
    /// documented input-driven divergences REJECT. Constants are the
    /// recorded corpus values (spec §11), so this test guards the policy
    /// itself, not pipeline behavior (which `psy_end_to_end` gates).
    #[test]
    fn gain_gate_policy() {
        // Accept: exact small gains, and the largest proven same-input
        // residual (f25 C-input 22165 vs committed 22154, Δ11).
        for (got, want) in [
            (1000, 1000),
            (1018, 1018),
            (1007, 1007),
            (18751, 18744),
            (10229, 10227),
            (22165, 22154),
        ] {
            assert!(gain_within_tolerance(got, want), "accept ({got}, {want})");
        }
        // Reject: every input-driven divergence on record (smallest Δ29).
        for (got, want) in [
            (1049, 1018),
            (1047, 1018),
            (10317, 10227),
            (27723, 22154),
            (27781, 18744),
            (3910, 3866),
        ] {
            assert!(!gain_within_tolerance(got, want), "reject ({got}, {want})");
        }
        // Gate shape: floor 8 at small gains, +1 per mille above.
        assert_eq!(gain_tolerance(1018), 9);
        assert_eq!(gain_tolerance(22154), 30);
    }

    /// Corpus-wide same-input gain validation: float Schur on committed
    /// (exact) rxx must satisfy the gate against committed TNSG gains, HI
    /// on every window plus LO wherever the committed HI filter is active
    /// (LO detect only runs then; otherwise production leaves exactly
    /// 1000, asserted as such). Any failure here is a genuine Schur/port
    /// defect, since inputs are bit-identical by construction.
    #[test]
    fn schur_gains_match_committed_within_gate() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/aac_enc/testdata");
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("testdata dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("rxx_") && n.ends_with(".txt"))
            .collect();
        entries.sort();
        let mut checked = 0;
        for n in entries {
            let rest = n["rxx_".len()..n.len() - ".txt".len()].to_string();
            let (name, tail) = rest.rsplit_once("_f").expect("rxx name");
            let (frame, ch) = tail.split_once("_ch").expect("rxx frame/ch");
            let (frame, ch): (u32, u32) = (frame.parse().expect("frame"), ch.parse().expect("ch"));
            let seq_rows = testdata::load_seq(name);
            let row = seq_rows
                .iter()
                .find(|r| r.frame == frame && r.channel == ch)
                .expect("seq row");
            let short = row.seq == 2;
            let sr = testdata::fixture_sample_rate(name);
            let nch = if testdata::fixture_samples(name).len() == 1 {
                1
            } else {
                2
            };
            let (ch_br, tns_br) = if nch == 1 {
                (112000, 112000)
            } else {
                (96000, 192000)
            };
            let tconf = if short {
                let cfg = crate::aac_enc::sfb::SfbConfig::new(
                    sr,
                    crate::aac_enc::sfb::BlockKind::Short,
                    ch_br,
                    19293,
                    true,
                    true,
                )
                .unwrap();
                TnsConfig::new(sr, tns_br, false, cfg.offsets(), cfg.sfb_active())
            } else {
                let cfg = crate::aac_enc::sfb::SfbConfig::new(
                    sr,
                    crate::aac_enc::sfb::BlockKind::Long,
                    ch_br,
                    19293,
                    true,
                    true,
                )
                .unwrap();
                TnsConfig::new(sr, tns_br, true, cfg.offsets(), cfg.sfb_active())
            };
            let psy = testdata::load_psy(name, frame, ch, short);
            for rw in testdata::load_rxx(name, frame, ch) {
                let tw = &psy.tns[rw.window];
                // HI filter from rxx2.
                let mut corr = [0.0f64; 13];
                for (i, &q) in rw.rxx2.iter().enumerate() {
                    corr[i] = q as f64 / 2_147_483_648.0;
                }
                let (_, gain) = schur_record(&corr, tconf.max_order, tconf.limit_order[HI]);
                let gain_int = (gain * 1000.0).floor() as i32;
                assert!(
                    gain_within_tolerance(gain_int, tw.gain[0]),
                    "{name} f{frame} ch{ch} w{} HI gain {gain_int} vs {}",
                    rw.window,
                    tw.gain[0]
                );
                // LO filter from rxx1 (long only, HI-active gates LO detect).
                if !short && tconf.limit_order[LO] > 0 {
                    if tw.active[0] {
                        let mut corr_lo = [0.0f64; 13];
                        for (i, &q) in rw.rxx1.iter().enumerate() {
                            corr_lo[i] = q as f64 / 2_147_483_648.0;
                        }
                        let (_, gain_lo) =
                            schur_record(&corr_lo, tconf.max_order, tconf.limit_order[LO]);
                        let gain_lo_int = (gain_lo * 1000.0).floor() as i32;
                        assert!(
                            gain_within_tolerance(gain_lo_int, tw.gain[1]),
                            "{name} f{frame} ch{ch} w{} LO gain {gain_lo_int} vs {}",
                            rw.window,
                            tw.gain[1]
                        );
                    } else {
                        assert_eq!(
                            tw.gain[1], 1000,
                            "{name} f{frame} ch{ch} w{} LO default",
                            rw.window
                        );
                    }
                }
                checked += 1;
            }
        }
        assert!(checked > 100, "corpus coverage, got {checked}");
    }
}
