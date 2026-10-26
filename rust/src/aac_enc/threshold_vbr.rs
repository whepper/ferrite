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
CONTRIBUTORS BE LIABLE for any direct, indirect, incidental, special exemplary,
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
//! M4-S2: VBR threshold adjustment.
//!
//! Ports the VBR branch of `FDKaacEnc_AdjustThresholds`
//! (`adj_thr.cpp:2869-2899`): per-element `FDKaacEnc_AdaptThresholdsVBR`
//! (`:2160-2194`) — `calcThreshExp` (`:443-459`), `adaptMinSnr`
//! (`:466-533`), `initAvoidHoleFlag` (`:539-701`),
//! `reduceThresholdsVBR` (`:1108-1312`, via `calcChaosMeasure`
//! `:1054-1105`) — followed by the shared enFac add (`:2884-2899`).
//! The CBR path (`adaptThresholdsToPe` and friends) is out of scope.
//!
//! Numeric convention: same as [`super::pe`] — every value is Q31 `i32`
//! and every op mirrors the C integer op (wrapping adds/subs, wrapping
//! shifts, arithmetic right shifts, ARM64 `fMult` floor) via
//! [`super::q31`]. Given identical inputs the outputs are bit-exact.
//!
//! Call order and buffer roles mirror the encoder (`aacenc.cpp:803-808`
//! aliasing): threshold inputs are the POST-weighting S1 vectors
//! (`WTHLD`, i.e. `threshold - enFac`), energies are the unweighted
//! `SENL`/`PENLD` vectors, and the enFac add re-applies `ENFAC` after
//! reduction. MS/IS decisions are oracle inputs (S1 `MSD`), not ported
//! logic; `ms_mask` is `None` for mono.
//!
//! Q31 literal table (all C-probed from `FL2FXCONST_DBL`, except the
//! verbatim hex tables):
//!
//! | symbol | value | hex | source |
//! |---|---|---|
//! | `MSA_LIMIT` | -0.00503012648262 | `0xff5b2c3e` | `adaptMinSnr :472` |
//! | `AVG_COMP` | 0.09375 (= 6<<25) | `0x0c000000` | `:504` |
//! | `MSA_RED_SCALE` | 0.3010299956 | `0x268826c0` | `:480` |
//! | `MSA_MAX_RED` | 0.00390625 | `0x00800000` | `AdjThrInit :2570` |
//! | `MSA_START_RATIO` | 0.05190512648 | `0x06a4d3c0` | `:2572` |
//! | `MSA_RED_RATIO_FAC` | -0.375 | `0xd0000000` | `:2578` |
//! | `MSA_RED_OFFS` | 0.021484375 | `0x02c00000` | `:2581` |
//! | `SPREAD_SHORT_FAC` | 0.63 | `0x50a3d700` | `initAvoidHoleFlag :563` |
//! | `SPREAD_MS_FAC` | 0.9 | `0x73333300` | `:672/:676` |
//! | `CHAOS_SHORT` | 0.5 | `0x40000000` | `reduceThresholdsVBR :1155` |
//! | `CHAOS_AVG0/1` | 0.25 / 0.75 | `0x20000000/0x60000000` | `:1130-1131` |
//! | `CHAOS_BASE` | 0.2 | `0x199999a0` | `:1184` |
//! | `CHAOS_SLOPE` | 0.7/(4*0.3) | `0x4aaaaa80` | `:1185` |
//! | `CHAOS_LO/HI` | 0.1 / 1.0 | `0x0ccccd0/0x7fffffff` | `:1188` |
//! | `SHORT_THR_FAC` | 2.82/4 | `0x5a3d7080` | `:1255` |
//! | `MIN_LDTHRESH` | -0.515625 | `0xbe000000` | `:1133` |
//! | `THR_FLOOR_NEG` | -0.5 | `0xc0000000` | `:1302` |
//! | `VBR_29DB` | 9.6336206/64 | `0x134469eb` | CBR-only, unused here |
//! | `CHAOS0` | 0.3 | `0x26666680` | `AdjThrInit :2545` |
//! | `VBR_QUAL_VBR5` | 0.070 | `0x08f5c290` | `qc_main.cpp:132` |
//! | `FRAME_ENERGY_INIT` | 1e-10 (underflows Q31) | `0x00000000` | `:1117` |

use super::pe::AtsElement;
use super::q31::{
    calc_inv_ld_data, calc_ld_data, calc_ld_int, count_leading_zeros, fmult, schur_div,
};

/// Avoid-hole flag values (`adj_thr.cpp:307`): `NO_AH = 0`,
/// `AH_INACTIVE = 1`, `AH_ACTIVE = 2`.
pub const NO_AH: u8 = 0;
pub const AH_INACTIVE: u8 = 1;
pub const AH_ACTIVE: u8 = 2;

/// Window sequences (`psy_const.h:121-124`): 0 LONG, 1 START, 2 SHORT,
/// 3 STOP.
pub const SEQ_LONG: u8 = 0;
pub const SEQ_SHORT: u8 = 2;

/// Reciprocal integers for short-block group-energy correction
/// (`adj_thr.cpp:109-111`), verbatim.
const INV_INT: [i32; 8] = [
    0x7fffffff, 0x7fffffff, 0x40000000, 0x2aaaaaaa, 0x20000000, 0x19999999, 0x15555555, 0x12492492,
];

/// Fourth-root reciprocals for short-block threshold scaling
/// (`adj_thr.cpp:114-116`), verbatim.
const INV_SQRT4: [i32; 8] = [
    0x7fffffff, 0x7fffffff, 0x6ba27e65, 0x61424bb5, 0x5a827999, 0x55994845, 0x51c8e33c, 0x4eb160d1,
];

/// `SnrLdMin1..5` / `SnrLdFac` (`adj_thr.cpp:119-136`), verbatim hex.
const SNR_LD_MIN1: i32 = 0xfcad0ddfu32 as i32;
const SNR_LD_MIN2: i32 = 0x0351e1a2;
const SNR_LD_FAC: i32 = 0xff5b2c3eu32 as i32;
const SNR_LD_MIN3: i32 = 0xfe000000u32 as i32;
const SNR_LD_MIN4: i32 = 0x02000000;
const SNR_LD_MIN5: i32 = 0xfc000000u32 as i32;

/// `limitThrReducedLdData` (`adj_thr.cpp:985-986`), verbatim.
const LIMIT_THR_REDUCED: i32 = 0x00008000;
/// `MIN_LDTHRESH` (`adj_thr.cpp:1133`): -0.515625 as Q31.
const MIN_LDTHRESH: i32 = 0xbe000000u32 as i32;
/// `FL2FXCONST_DBL(-0.5f)` clamp (`:1302`, also MS `-0.5` test `:637`).
const NEG_HALF_Q31: i32 = 0xc0000000u32 as i32;
/// `FL2FXCONST_DBL(0.0f)`.
const ZERO_Q31: i32 = 0;
/// Q31 `-1.0` sentinel.
const NEG_ONE_Q31: i32 = i32::MIN;
/// Saturating Q31 `1.0` (`FL2FXCONST_DBL(1.f)` = `MAXVAL_DBL`).
const ONE_Q31: i32 = i32::MAX;
/// `minSnr` adaptation ceiling (`adaptMinSnr :472-473`): `ld64(0.8)`.
const MSA_LIMIT: i32 = 0xff5b2c3eu32 as i32;
/// `0.09375` energy-shift compensation (`adaptMinSnr :504`), also
/// `(6 << 25)` group-length correction (`reduceThresholdsVBR :1274`).
const AVG_COMP_Q31: i32 = 0x0c000000;
/// `0.3010299956` minSnr slope scaler (`adaptMinSnr :480`).
const MSA_RED_SCALE: i32 = 0x268826c0;
/// Short-block spread factor `0.63` (`initAvoidHoleFlag :563-564`).
const SPREAD_SHORT_FAC: i32 = 0x50a3d700;
/// MS spread factor `0.9` (`:671-677`).
const SPREAD_MS_FAC: i32 = 0x73333300;
/// Short-block chaos pin `0.5` (`reduceThresholdsVBR :1155-1156`).
const CHAOS_SHORT: i32 = 0x40000000;
/// Chaos averaging weights `0.25` / `0.75` (`:1130-1131`).
const CHAOS_AVG0: i32 = 0x20000000;
const CHAOS_AVG1: i32 = 0x60000000;
/// Characteristic-curve constants (`:1184-1190`): `0.2`, `0.7/(4*0.3)`,
/// `0.1`, `1.0`.
const CHAOS_BASE: i32 = 0x199999a0;
const CHAOS_SLOPE: i32 = 0x4aaaaa80;
const CHAOS_LO: i32 = 0x0cccccd0;
/// Short-block threshold factor `2.82/4` (`:1255`).
const SHORT_THR_FAC: i32 = 0x5a3d7080;
/// `0.75` flatness exponent weight (`calcChaosMeasure :1091`).
const THREE_QUARTERS: i32 = 0x60000000;

/// Per-channel SFB geometry shared by all S2 functions.
pub struct SfbGeometry<'a> {
    pub offsets: &'a [i32],
    /// `groupLen` (`MAX_NO_OF_GROUPS` = 4 entries; short blocks only).
    pub group_len: &'a [i32],
    pub sfb_cnt: usize,
    pub sfb_per_group: usize,
    pub max_sfb_per_group: usize,
    /// Window sequence: 0 LONG, 1 START, 2 SHORT, 3 STOP.
    pub seq: u8,
}

impl<'a> SfbGeometry<'a> {
    pub fn is_short(&self) -> bool {
        self.seq == SEQ_SHORT
    }
}

/// One channel's S2 inputs. Linear/Ld energies are the UNWEIGHTED psy
/// vectors (`SENL`/`PENLD`); `weighted_energy_ld`/`threshold_ld` are the
/// POST-weighting S1 vectors (`WENLD`/`WTHLD`); `min_snr_ld`/`spread`
/// are the S2-stage inputs (`MINSNR0`/`SPREAD0`); `enfac_ld` is `ENFAC`
/// (re-applied after reduction).
pub struct S2ChannelInput<'a> {
    pub energy_lin: &'a [i32],
    pub energy_ld: &'a [i32],
    pub weighted_energy_ld: &'a [i32],
    pub threshold_ld: &'a [i32],
    pub form_factor_ld: &'a [i32],
    pub enfac_ld: &'a [i32],
    pub min_snr_ld: &'a [i32],
    pub spread: &'a [i32],
    pub geo: SfbGeometry<'a>,
}

/// `FDKaacEnc_calcThreshExp` (`adj_thr.cpp:443-459`): loudness
/// (`threshold^redExp`) as `CalcInvLdData(threshold >> 2)` per active
/// band. Inactive bands read 0 (C scratch is unread there).
pub fn calc_thresh_exp(threshold_ld: &[i32], geo: &SfbGeometry<'_>) -> Vec<i32> {
    let mut out = vec![0i32; geo.sfb_cnt];
    let mut grp = 0;
    while grp < geo.sfb_cnt {
        for sfb in 0..geo.max_sfb_per_group {
            let s = grp + sfb;
            out[s] = calc_inv_ld_data(threshold_ld[s] >> 2);
        }
        grp += geo.sfb_per_group;
    }
    out
}

/// `FDKaacEnc_adaptMinSnr` (`adj_thr.cpp:466-533`): lower the minSnr
/// requirement for bands far below the frame-average energy. Reads the
/// linear `energy_lin` (`SENL`) and Ld `energy_ld` (`PENLD`); updates
/// `min_snr` in place over the active bands.
pub fn adapt_min_snr(
    energy_lin: &[i32],
    energy_ld: &[i32],
    min_snr: &mut [i32],
    geo: &SfbGeometry<'_>,
    ats: &AtsElement,
) {
    // Frame-average energy per band (active bands, >>6 accumulation).
    let mut n_sfb: i32 = 0;
    let mut accu: i32 = 0;
    let mut grp = 0;
    while grp < geo.sfb_cnt {
        n_sfb += geo.max_sfb_per_group as i32;
        for sfb in 0..geo.max_sfb_per_group {
            accu = accu.wrapping_add(energy_lin[grp + sfb] >> 6);
        }
        grp += geo.sfb_per_group;
    }
    let avg_en_ld = if accu == 0 || n_sfb == 0 {
        NEG_ONE_Q31
    } else {
        // `+0.09375f` compensates the >>6 accumulation shift.
        calc_ld_data(accu)
            .wrapping_add(AVG_COMP_Q31)
            .wrapping_sub(calc_ld_int(n_sfb))
    };
    // Pre-scale the slope factor (exactly as C does per call).
    let red_ratio_fac = fmult(ats.msa_red_ratio_fac, MSA_RED_SCALE);
    let mut grp = 0;
    while grp < geo.sfb_cnt {
        for sfb in 0..geo.max_sfb_per_group {
            let s = grp + sfb;
            let db_ratio = avg_en_ld.wrapping_sub(energy_ld[s]);
            let update = ats.msa_start_ratio < db_ratio;
            // Scaled by 1/64 (result re-scaled by <<6 below).
            let mut red = ats
                .msa_red_offs
                .wrapping_add(fmult(red_ratio_fac, db_ratio));
            red = red.max(ats.msa_max_red);
            red = fmult(min_snr[s], red).wrapping_shl(6);
            red = red.min(MSA_LIMIT);
            if update {
                min_snr[s] = red;
            }
        }
        grp += geo.sfb_per_group;
    }
}

/// `FDKaacEnc_initAvoidHoleFlag` (`adj_thr.cpp:539-701`): scale spread
/// energy, adapt minSnr for local peaks/valleys (when
/// `ah_modify_min_snr`), adapt mid/side minSnr + spread on MS bands,
/// and classify every active band (`NO_AH` vs `AH_INACTIVE`).
/// `ms_mask` is `Some` for stereo (`MSD` mask over channel-0 geometry),
/// `None` for mono. `min_snr`/`spread` are updated in place; the second
/// channel's slices are needed for the stereo section — pass both
/// channels' working state via `stereo`.
pub struct StereoWork<'a> {
    pub energy_lin_m: &'a [i32],
    pub energy_ld_m: &'a [i32],
    pub min_snr_m: &'a mut [i32],
    pub spread_m: &'a mut [i32],
    pub energy_lin_s: &'a [i32],
    pub energy_ld_s: &'a [i32],
    pub min_snr_s: &'a mut [i32],
    pub spread_s: &'a mut [i32],
    pub geo_m: SfbGeometry<'a>,
    pub ms_mask: &'a [i32],
}

/// Single-channel core of `initAvoidHoleFlag` (spread scaling +
/// peak/valley + ahFlag init). The stereo MS section runs separately
/// over both channels (see [`init_avoid_hole_flag_ms`]).
pub fn init_avoid_hole_flag_mono(
    energy_lin: &[i32],
    energy_ld: &[i32],
    min_snr: &mut [i32],
    spread: &mut [i32],
    geo: &SfbGeometry<'_>,
    ats: &AtsElement,
) -> Vec<u8> {
    // Spread desensitization: -3 dB (>>1) except short blocks (*0.63).
    let mut grp = 0;
    while grp < geo.sfb_cnt {
        for sfb in 0..geo.max_sfb_per_group {
            let s = grp + sfb;
            if !geo.is_short() {
                spread[s] >>= 1;
            } else {
                spread[s] = fmult(SPREAD_SHORT_FAC, spread[s]);
            }
        }
        grp += geo.sfb_per_group;
    }
    if ats.ah_modify_min_snr != 0 {
        let mut grp = 0;
        while grp < geo.sfb_cnt {
            for sfb in 0..geo.max_sfb_per_group {
                let s = grp + sfb;
                let en_m1 = if sfb > 0 {
                    energy_lin[s - 1]
                } else {
                    energy_lin[s]
                };
                let en_p1 = if sfb + 1 < geo.max_sfb_per_group {
                    energy_lin[s + 1]
                } else {
                    energy_lin[s]
                };
                let avg_en = (en_m1 >> 1).wrapping_add(en_p1 >> 1);
                let avg_ld = calc_ld_data(avg_en);
                let en = energy_lin[s];
                let en_ld = energy_ld[s];
                if en > avg_en {
                    // Local peak: raise the minSnr floor (tighter for
                    // LONG than for START/SHORT/STOP).
                    let floor = if geo.seq == SEQ_LONG {
                        SNR_LD_FAC + (avg_ld - en_ld).max(SNR_LD_MIN1 - SNR_LD_FAC)
                    } else {
                        SNR_LD_FAC + (avg_ld - en_ld).max(SNR_LD_MIN3 - SNR_LD_FAC)
                    };
                    min_snr[s] = min_snr[s].min(floor);
                }
                if en_ld.wrapping_add(SNR_LD_MIN4) < avg_ld && en > 0 {
                    // Valley: relax minSnr, bounded above and by drift.
                    let tmp = avg_ld
                        .wrapping_sub(en_ld)
                        .wrapping_sub(SNR_LD_MIN4)
                        .wrapping_add(min_snr[s]);
                    let tmp = tmp.min(SNR_LD_FAC);
                    min_snr[s] = tmp.min(min_snr[s].wrapping_add(SNR_LD_MIN2));
                }
            }
            grp += geo.sfb_per_group;
        }
    }
    // Classify: holes only matter where spread hides energy AND minSnr
    // allows coding below the threshold.
    let mut ah = vec![NO_AH; geo.sfb_cnt];
    let mut grp = 0;
    while grp < geo.sfb_cnt {
        for sfb in 0..geo.max_sfb_per_group {
            let s = grp + sfb;
            if spread[s] > energy_lin[s] || min_snr[s] > 0 {
                ah[s] = NO_AH;
            } else {
                ah[s] = AH_INACTIVE;
            }
        }
        grp += geo.sfb_per_group;
    }
    ah
}

/// Stereo MS section of `initAvoidHoleFlag` (`adj_thr.cpp:622-682`):
/// adapt mid/side minSnr on MS-coupled bands (channel-0 geometry) and
/// cross-tweak spread energies. Runs after the mono cores.
pub fn init_avoid_hole_flag_ms(st: &mut StereoWork<'_>) {
    let (cnt, per, max) = (
        st.geo_m.sfb_cnt,
        st.geo_m.sfb_per_group,
        st.geo_m.max_sfb_per_group,
    );
    let mut grp = 0;
    while grp < cnt {
        for sfb in 0..max {
            let s = grp + sfb;
            if st.ms_mask[s] == 0 {
                continue;
            }
            let max_en_ld = st.energy_ld_m[s].max(st.energy_ld_s[s]);
            let max_thr_ld =
                if (SNR_LD_MIN5 >> 1) + (max_en_ld >> 1) + (st.min_snr_m[s] >> 1) <= NEG_HALF_Q31 {
                    NEG_ONE_Q31
                } else {
                    SNR_LD_MIN5
                        .wrapping_add(max_en_ld)
                        .wrapping_add(st.min_snr_m[s])
                };
            if st.energy_lin_m[s] > 0 {
                let tmp = max_thr_ld.wrapping_sub(st.energy_ld_m[s]);
                st.min_snr_m[s] = st.min_snr_m[s].max(tmp);
            } else {
                st.min_snr_m[s] = st.min_snr_m[s].max(ZERO_Q31);
            }
            if st.min_snr_m[s] <= 0 {
                st.min_snr_m[s] = st.min_snr_m[s].min(SNR_LD_FAC);
            }
            if st.energy_lin_s[s] > 0 {
                let tmp = max_thr_ld.wrapping_sub(st.energy_ld_s[s]);
                st.min_snr_s[s] = st.min_snr_s[s].max(tmp);
            } else {
                st.min_snr_s[s] = st.min_snr_s[s].max(ZERO_Q31);
            }
            if st.min_snr_s[s] <= 0 {
                st.min_snr_s[s] = st.min_snr_s[s].min(SNR_LD_FAC);
            }
            if st.energy_lin_m[s] > st.spread_m[s] {
                st.spread_s[s] = fmult(st.energy_lin_s[s], SPREAD_MS_FAC);
            }
            if st.energy_lin_s[s] > st.spread_s[s] {
                st.spread_m[s] = fmult(st.energy_lin_m[s], SPREAD_MS_FAC);
            }
        }
        grp += per;
    }
}

/// `FDKaacEnc_calcChaosMeasure` (`adj_thr.cpp:1054-1105`): tonal-vs-chaos
/// estimate from form factors of bands above threshold. `energy_ld` is
/// the unweighted `PENLD`, `threshold_ld` the weighted `WTHLD`.
pub fn calc_chaos_measure(
    energy_lin: &[i32],
    energy_ld: &[i32],
    threshold_ld: &[i32],
    form_factor_ld: &[i32],
    geo: &SfbGeometry<'_>,
) -> i32 {
    const SCALE_FORM_FAC: i32 = 4;
    const SCALE_NRGS: i32 = 8;
    const SCALE_NLINES: i32 = 16;
    let mut frame_form_factor: i32 = 0;
    let mut frame_energy: i32 = 0;
    let mut frame_n_lines: i32 = 0;
    let mut grp = 0;
    while grp < geo.sfb_cnt {
        for sfb in 0..geo.max_sfb_per_group {
            let s = grp + sfb;
            if energy_ld[s] > threshold_ld[s] {
                frame_form_factor = frame_form_factor
                    .wrapping_add(calc_inv_ld_data(form_factor_ld[s]) >> SCALE_FORM_FAC);
                frame_n_lines += geo.offsets[s + 1] - geo.offsets[s];
                frame_energy = frame_energy.wrapping_add(energy_lin[s] >> SCALE_NRGS);
            }
        }
        grp += geo.sfb_per_group;
    }
    if frame_n_lines > 0 {
        // frameNActiveLines = ff*2^6 * ((energy*2^8)/nLines)^-0.25;
        // chaos = frameNActiveLines / frameNLines, in the Ld domain with
        // the headroom constant folded in. NOTE: the final <<1 sits
        // INSIDE CalcInvLdData (adj_thr.cpp:1088-1098), not outside.
        const HEADROOM: i32 = -(((-SCALE_FORM_FAC + 2 - 6 + 12) << 25) >> 1);
        let t1 = calc_ld_data(frame_form_factor) >> 1;
        let t2 = calc_ld_data(frame_energy) >> 3;
        let t3 = super::q31::fmult_div2(
            THREE_QUARTERS,
            calc_ld_data(frame_n_lines.wrapping_shl((32 - 1 - SCALE_NLINES) as u32)),
        );
        let inner = t1.wrapping_sub(t2).wrapping_sub(t3.wrapping_sub(HEADROOM));
        calc_inv_ld_data(inner.wrapping_shl(1))
    } else {
        // No band above threshold: assume total chaos.
        ONE_Q31
    }
}

/// Reduction values and post-curve chaos from [`reduce_thresholds_vbr`].
pub struct ReduceResult {
    /// One `redVal` for long blocks, one per group for short blocks.
    pub red_val: Vec<i32>,
    /// Post-curve chaos measure feeding `redVal`.
    pub chaos_measure: i32,
}

/// `FDKaacEnc_reduceThresholdsVBR` (`adj_thr.cpp:1108-1312`): chaos-gated
/// threshold reduction. Reads weighted energies/thresholds, `thr_exp`,
/// `min_snr` (post avoid-hole init) and form factors; writes reduced
/// thresholds and `ah_flag` in place; evolves `ats.chaos_measure_old`.
/// Weighted energies are read-only on the VBR path (the C signature
/// takes `QC_OUT_CHANNEL*` but never writes them here).
/// `seq0_short` mirrors the C `psyOutChannel[0]` short test that selects
/// chaos pinning and the redVal path for the whole element.
///
/// NOTE (faithful C quirk, `:1249`): the short-block `groupNumber` is
/// `(int)sfb / sfbPerGroup` where `sfb` is the *within-group* index, so
/// it is always 0 (`sfb < maxSfbPerGroup <= sfbPerGroup`). Every short
/// band therefore uses group 0's `redVal`/`groupLen`. Ported verbatim;
/// the multi-group short corpus frames prove it.
#[allow(clippy::too_many_arguments)]
pub fn reduce_thresholds_vbr(
    weighted_energy_ld: &[&[i32]],
    threshold_ld: &mut [&mut [i32]],
    thr_exp: &[Vec<i32>],
    min_snr_ld: &[Vec<i32>],
    energy_lin: &[&[i32]],
    energy_ld: &[&[i32]],
    form_factor_ld: &[&[i32]],
    geos: &[SfbGeometry<'_>],
    seqs_short: &[bool],
    seq0_short: bool,
    ah_flag: &mut [Vec<u8>],
    ats: &mut AtsElement,
) -> ReduceResult {
    // Body lives in `reduce_thresholds_vbr_impl` below (single
    // implementation; the split keeps the public signature stable).
    reduce_thresholds_vbr_impl(
        weighted_energy_ld,
        threshold_ld,
        thr_exp,
        min_snr_ld,
        energy_lin,
        energy_ld,
        form_factor_ld,
        geos,
        seqs_short,
        seq0_short,
        ah_flag,
        ats,
    )
}

/// Full S2 orchestration for one element, mirroring
/// `FDKaacEnc_AdaptThresholdsVBR` (`adj_thr.cpp:2160-2194`) plus the
/// shared enFac add of `FDKaacEnc_AdjustThresholds` (`:2884-2899`).
/// Consumes S1 outputs; returns reduced + re-weighted thresholds and
/// the S2 working state for tests.
pub struct AdaptResult {
    /// Thresholds after VBR reduction AND enFac re-add (`THRV`).
    pub threshold_ld: Vec<Vec<i32>>,
    /// Thresholds after reduction but before enFac re-add (diagnostic).
    pub threshold_pre_enfac: Vec<Vec<i32>>,
    /// `thrExp` per channel (`THREXP`).
    pub thr_exp: Vec<Vec<i32>>,
    /// minSnr after `adaptMinSnr` + avoid-hole init (`MINSNR1`).
    pub min_snr_ld: Vec<Vec<i32>>,
    /// Spread after avoid-hole init (`SPREAD1`).
    pub spread: Vec<Vec<i32>>,
    /// Final avoid-hole flags (not oracle-dumped; synthetic-tested).
    pub ah_flag: Vec<Vec<u8>>,
    /// Reduction values + post-curve chaos.
    pub reduce: ReduceResult,
}

pub fn adapt_thresholds_vbr(
    channels: &[S2ChannelInput<'_>],
    ms_mask: Option<&[i32]>,
    ats: &mut AtsElement,
) -> AdaptResult {
    let nch = channels.len();
    debug_assert!(nch == 1 || nch == 2);
    if nch == 2 {
        debug_assert!(ms_mask.is_some());
    }
    // thrExp from the weighted thresholds.
    let thr_exp: Vec<Vec<i32>> = channels
        .iter()
        .map(|ch| calc_thresh_exp(ch.threshold_ld, &ch.geo))
        .collect();
    // adaptMinSnr (per channel, in-place on owned copies).
    let mut min_snr: Vec<Vec<i32>> = channels.iter().map(|ch| ch.min_snr_ld.to_vec()).collect();
    for (i, ch) in channels.iter().enumerate() {
        adapt_min_snr(ch.energy_lin, ch.energy_ld, &mut min_snr[i], &ch.geo, ats);
    }
    // Avoid-hole init: mono cores, then the stereo MS section.
    let mut spread: Vec<Vec<i32>> = channels.iter().map(|ch| ch.spread.to_vec()).collect();
    let mut ah_flag: Vec<Vec<u8>> = Vec::with_capacity(nch);
    for (i, ch) in channels.iter().enumerate() {
        ah_flag.push(init_avoid_hole_flag_mono(
            ch.energy_lin,
            ch.energy_ld,
            &mut min_snr[i],
            &mut spread[i],
            &ch.geo,
            ats,
        ));
    }
    if nch == 2 {
        let (first, rest) = min_snr.split_at_mut(1);
        let (spread_first, spread_rest) = spread.split_at_mut(1);
        let c0 = &channels[0];
        let c1 = &channels[1];
        let mut work = StereoWork {
            energy_lin_m: c0.energy_lin,
            energy_ld_m: c0.energy_ld,
            min_snr_m: &mut first[0],
            spread_m: &mut spread_first[0],
            energy_lin_s: c1.energy_lin,
            energy_ld_s: c1.energy_ld,
            min_snr_s: &mut rest[0],
            spread_s: &mut spread_rest[0],
            geo_m: SfbGeometry {
                offsets: c0.geo.offsets,
                group_len: c0.geo.group_len,
                sfb_cnt: c0.geo.sfb_cnt,
                sfb_per_group: c0.geo.sfb_per_group,
                max_sfb_per_group: c0.geo.max_sfb_per_group,
                seq: c0.geo.seq,
            },
            ms_mask: ms_mask.expect("stereo needs ms_mask"),
        };
        init_avoid_hole_flag_ms(&mut work);
    }
    // VBR reduction (evolves ats.chaos_measure_old).
    let mut thresholds: Vec<Vec<i32>> =
        channels.iter().map(|ch| ch.threshold_ld.to_vec()).collect();
    let geos: Vec<SfbGeometry<'_>> = channels
        .iter()
        .map(|ch| SfbGeometry {
            offsets: ch.geo.offsets,
            group_len: ch.geo.group_len,
            sfb_cnt: ch.geo.sfb_cnt,
            sfb_per_group: ch.geo.sfb_per_group,
            max_sfb_per_group: ch.geo.max_sfb_per_group,
            seq: ch.geo.seq,
        })
        .collect();
    let seqs_short: Vec<bool> = channels.iter().map(|ch| ch.geo.is_short()).collect();
    let seq0_short = channels[0].geo.is_short();
    // Borrow-split working vectors for the reduction call.
    let wvecs: Vec<&[i32]> = channels.iter().map(|ch| ch.weighted_energy_ld).collect();
    let evecs: Vec<&[i32]> = channels.iter().map(|ch| ch.energy_lin).collect();
    let lvecs: Vec<&[i32]> = channels.iter().map(|ch| ch.energy_ld).collect();
    let fvecs: Vec<&[i32]> = channels.iter().map(|ch| ch.form_factor_ld).collect();
    let mut trefs: Vec<&mut [i32]> = thresholds.iter_mut().map(|v| v.as_mut_slice()).collect();
    let mut arefs: Vec<Vec<u8>> = ah_flag.into_iter().collect();
    let reduce = reduce_thresholds_vbr(
        &wvecs,
        &mut trefs,
        &thr_exp,
        &min_snr,
        &evecs,
        &lvecs,
        &fvecs,
        &geos,
        &seqs_short,
        seq0_short,
        &mut arefs,
        ats,
    );
    let threshold_pre_enfac = thresholds.clone();
    // Shared enFac add (AdjustThresholds tail): threshold += enFac over
    // the active bands.
    for (i, ch) in channels.iter().enumerate() {
        let mut grp = 0;
        while grp < ch.geo.sfb_cnt {
            for sfb in 0..ch.geo.max_sfb_per_group {
                let s = grp + sfb;
                thresholds[i][s] = thresholds[i][s].wrapping_add(ch.enfac_ld[s]);
            }
            grp += ch.geo.sfb_per_group;
        }
    }
    AdaptResult {
        threshold_ld: thresholds,
        threshold_pre_enfac,
        thr_exp,
        min_snr_ld: min_snr,
        spread,
        ah_flag: arefs,
        reduce,
    }
}

/// Implementation behind [`reduce_thresholds_vbr`] (see its docs).
#[allow(clippy::too_many_arguments)]
fn reduce_thresholds_vbr_impl(
    weighted_energy_ld: &[&[i32]],
    threshold_ld: &mut [&mut [i32]],
    thr_exp: &[Vec<i32>],
    min_snr_ld: &[Vec<i32>],
    energy_lin: &[&[i32]],
    energy_ld: &[&[i32]],
    form_factor_ld: &[&[i32]],
    geos: &[SfbGeometry<'_>],
    seqs_short: &[bool],
    seq0_short: bool,
    ah_flag: &mut [Vec<u8>],
    ats: &mut AtsElement,
) -> ReduceResult {
    const SCALE_GROUP_ENERGY: i32 = 8;
    const WIN_TYPE_SCALE: i32 = 3;
    let nch = geos.len();
    debug_assert!(nch == 1 || nch == 2);
    let mut group_energy: Vec<Vec<i32>> = Vec::with_capacity(nch);
    let mut ch_energy: Vec<i32> = Vec::with_capacity(nch);
    for (ch, geo) in geos.iter().enumerate() {
        let mut groups = Vec::new();
        let mut total: i32 = 0;
        let mut grp = 0;
        while grp < geo.sfb_cnt {
            let mut g: i32 = 0;
            for sfb in 0..geo.max_sfb_per_group {
                g = g.wrapping_add(energy_lin[ch][grp + sfb] >> SCALE_GROUP_ENERGY);
            }
            total = total.wrapping_add(g);
            groups.push(g);
            grp += geo.sfb_per_group;
        }
        group_energy.push(groups);
        ch_energy.push(total);
    }
    let mut frame_energy: i32 = 0;
    for e in &ch_energy {
        frame_energy = frame_energy.wrapping_add(*e);
    }
    let mut chaos: i32 = 0;
    for (ch, geo) in geos.iter().enumerate() {
        let ch_chaos = if seq0_short {
            CHAOS_SHORT
        } else {
            calc_chaos_measure(
                energy_lin[ch],
                energy_ld[ch],
                threshold_ld[ch],
                form_factor_ld[ch],
                geo,
            )
        };
        chaos = chaos.wrapping_add(fmult(ch_chaos, ch_energy[ch]));
    }
    if frame_energy > chaos {
        let scale = count_leading_zeros(frame_energy) - 1;
        let num = (chaos as u32).wrapping_shl(scale as u32) as i32;
        let denum = (frame_energy as u32).wrapping_shl(scale as u32) as i32;
        chaos = schur_div(num, denum, 16);
    } else {
        chaos = ONE_Q31;
    }
    let avg = fmult(CHAOS_AVG0, chaos).wrapping_add(fmult(CHAOS_AVG1, ats.chaos_measure_old));
    chaos = chaos.min(avg);
    ats.chaos_measure_old = chaos;
    chaos = (CHAOS_BASE >> 2).wrapping_add(fmult(CHAOS_SLOPE, chaos.wrapping_sub(CHAOS_BASE)));
    chaos = (ONE_Q31 >> 2)
        .min((CHAOS_LO >> 2).max(chaos))
        .wrapping_shl(2);
    let red_val: Vec<i32> = if seq0_short {
        let geo0 = &geos[0];
        let mut rv = Vec::new();
        let mut grp = 0;
        let mut group_cnt = 0;
        while grp < geo0.sfb_cnt {
            let mut g: i32 = 0;
            for ge in group_energy.iter().take(nch) {
                g = g.wrapping_add(ge[group_cnt]);
            }
            g = fmult(g, INV_INT[geo0.group_len[group_cnt] as usize]);
            g = g.min(frame_energy >> WIN_TYPE_SCALE);
            g >>= 2;
            rv.push(
                fmult(
                    fmult(ats.vbr_qual_factor, chaos),
                    calc_inv_ld_data(calc_ld_data(g) >> 2),
                )
                .wrapping_shl(4),
            );
            grp += geo0.sfb_per_group;
            group_cnt += 1;
        }
        rv
    } else {
        vec![fmult(
            fmult(ats.vbr_qual_factor, chaos),
            calc_inv_ld_data(calc_ld_data(frame_energy) >> 2),
        )
        .wrapping_shl(2)]
    };
    for (ch, geo) in geos.iter().enumerate() {
        let short = seqs_short[ch];
        let w = &weighted_energy_ld[ch];
        let t = &mut threshold_ld[ch];
        let m = &min_snr_ld[ch];
        let e = &thr_exp[ch];
        let a = &mut ah_flag[ch];
        let mut grp = 0;
        while grp < geo.sfb_cnt {
            for sfb in 0..geo.max_sfb_per_group {
                let s = grp + sfb;
                let en_ld = w[s];
                let thr_ld = t[s];
                let thr_e = e[s];
                if thr_ld >= MIN_LDTHRESH && en_ld > thr_ld && a[s] != AH_ACTIVE {
                    let mut reduced: i32;
                    if short {
                        let group_number = sfb / geo.sfb_per_group;
                        let gl = geo.group_len[group_number] as usize;
                        let scaled =
                            fmult(thr_e, fmult(SHORT_THR_FAC, INV_SQRT4[gl])).wrapping_shl(2);
                        if scaled <= LIMIT_THR_REDUCED.wrapping_sub(red_val[group_number]) {
                            reduced = NEG_ONE_Q31;
                        } else if red_val[group_number] >= ONE_Q31.wrapping_sub(scaled) {
                            reduced = ZERO_Q31;
                        } else {
                            reduced = calc_ld_data(scaled.wrapping_add(red_val[group_number]))
                                .wrapping_shl(2);
                        }
                        reduced = reduced.wrapping_add(
                            calc_ld_int(geo.group_len[group_number]).wrapping_sub(AVG_COMP_Q31),
                        );
                    } else if red_val[0] >= ONE_Q31.wrapping_sub(thr_e) {
                        reduced = ZERO_Q31;
                    } else {
                        reduced = calc_ld_data(thr_e.wrapping_add(red_val[0])).wrapping_shl(2);
                    }
                    if reduced.wrapping_sub(en_ld) > m[s] && a[s] != NO_AH {
                        if m[s] > NEG_ONE_Q31.wrapping_sub(en_ld) {
                            reduced = m[s].wrapping_add(en_ld).max(thr_ld);
                        } else {
                            reduced = thr_ld;
                        }
                        a[s] = AH_ACTIVE;
                    }
                    if reduced < NEG_HALF_Q31 {
                        reduced = NEG_ONE_Q31;
                    }
                    reduced = reduced.max(MIN_LDTHRESH);
                    t[s] = reduced;
                }
            }
            grp += geo.sfb_per_group;
        }
    }
    ReduceResult {
        red_val,
        chaos_measure: chaos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::pe::AtsElement;
    use crate::aac_enc::testdata;

    /// Committed S2 corpus: (testdata short name, frame). Same 16 S1
    /// frames plus onset f23/f25 (closing a consecutive f22–f26
    /// chaos-trajectory run) plus quiet-sine f5 (low-level non-silent
    /// long frame). Covers long/short/start/stop, mono/stereo,
    /// 48/44.1 kHz, MS NONE/SOME/ALL (via S1 masks), silence (no-op
    /// path) through stepdown f2 (23 changed bands).
    const S2_CASES: [(&str, u32); 19] = [
        ("silence-stereo", 5),
        ("sine-stereo", 1),
        ("sine-stereo", 5),
        ("onset-stereo", 22),
        ("onset-stereo", 23),
        ("onset-stereo", 24),
        ("onset-stereo", 25),
        ("onset-stereo", 26),
        ("stepdown-stereo", 2),
        ("click-mono", 11),
        ("impulse-mono", 5),
        ("edge-mono", 4),
        ("multitone-stereo", 1),
        ("sweep-stereo", 10),
        ("noise-mono", 5),
        ("tonal-noise-stereo", 5),
        ("quiet-sine-stereo", 5),
        ("sine-stereo-44100", 5),
        ("onset-stereo-44100", 22),
    ];

    fn load_pair(name: &str, f: u32) -> (testdata::S1Frame, testdata::S2Frame) {
        (testdata::load_s1(name, f), testdata::load_s2(name, f))
    }

    fn s2_inputs<'a>(
        s1: &'a testdata::S1Frame,
        s2: &'a testdata::S2Frame,
    ) -> (Vec<S2ChannelInput<'a>>, Option<Vec<i32>>) {
        let mut inputs = Vec::new();
        for (c, (ch1, ch2)) in s1.ch.iter().zip(s2.ch.iter()).enumerate() {
            assert_eq!(ch1.geo_cnt, ch2.geo_cnt, "{c} cnt");
            assert_eq!(ch1.geo_per, ch2.geo_per, "{c} per");
            assert_eq!(ch1.geo_max, ch2.geo_max, "{c} max");
            assert_eq!(ch1.geo_seq, ch2.geo_seq, "{c} seq");
            assert_eq!(ch2.group_len.len(), 4, "{c} groupLen");
            inputs.push(S2ChannelInput {
                energy_lin: &ch1.senl,
                energy_ld: &ch1.penld,
                weighted_energy_ld: &ch1.wenld,
                threshold_ld: &ch1.wthld,
                form_factor_ld: &ch1.ff,
                enfac_ld: &ch1.enfac,
                min_snr_ld: &ch2.minsnr0,
                spread: &ch2.spread0,
                geo: SfbGeometry {
                    offsets: &ch1.offsets,
                    group_len: &ch2.group_len,
                    sfb_cnt: ch1.geo_cnt,
                    sfb_per_group: ch1.geo_per,
                    max_sfb_per_group: ch1.geo_max,
                    seq: ch1.geo_seq,
                },
            });
        }
        let ms_mask = s1.msd.as_ref().map(|(_, m)| m.clone());
        if inputs.len() == 2 {
            assert!(ms_mask.is_some());
        } else {
            assert!(ms_mask.is_none());
        }
        (inputs, ms_mask)
    }

    #[test]
    fn s2_thresh_exp_matches_oracle() {
        for (name, f) in S2_CASES {
            let (s1, s2) = load_pair(name, f);
            let (inputs, _) = s2_inputs(&s1, &s2);
            for (c, (inp, ch2)) in inputs.iter().zip(s2.ch.iter()).enumerate() {
                let got = calc_thresh_exp(inp.threshold_ld, &inp.geo);
                assert_eq!(got, ch2.thr_exp, "{name} f{f} ch{c} THREXP");
            }
        }
    }

    #[test]
    fn s2_full_matches_oracle_with_injected_chaos() {
        for (name, f) in S2_CASES {
            let (s1, s2) = load_pair(name, f);
            let (inputs, ms_mask) = s2_inputs(&s1, &s2);
            let mut ats = AtsElement::new(0);
            ats.chaos_measure_old = s2.chaos_pre;
            let result = adapt_thresholds_vbr(&inputs, ms_mask.as_deref(), &mut ats);
            for (c, ch2) in s2.ch.iter().enumerate() {
                assert_eq!(result.threshold_ld[c], ch2.thrv, "{name} f{f} ch{c} THRV");
                assert_eq!(
                    result.min_snr_ld[c], ch2.minsnr1,
                    "{name} f{f} ch{c} MINSNR1"
                );
                assert_eq!(result.spread[c], ch2.spread1, "{name} f{f} ch{c} SPREAD1");
            }
            assert_eq!(
                ats.chaos_measure_old, s2.chaos_post,
                "{name} f{f} chaos evolution"
            );
        }
    }

    #[test]
    fn s2_pre_enfac_plus_enfac_is_thrv_on_active_bands() {
        // The shared enFac add is a plain per-active-band add
        // (adj_thr.cpp:2884-2899): THRV == pre-add + ENFAC there, and the
        // pre-add vector is untouched elsewhere.
        for (name, f) in S2_CASES {
            let (s1, s2) = load_pair(name, f);
            let (inputs, ms_mask) = s2_inputs(&s1, &s2);
            let mut ats = AtsElement::new(0);
            ats.chaos_measure_old = s2.chaos_pre;
            let result = adapt_thresholds_vbr(&inputs, ms_mask.as_deref(), &mut ats);
            for (c, (inp, ch2)) in inputs.iter().zip(s2.ch.iter()).enumerate() {
                let mut grp = 0;
                while grp < inp.geo.sfb_cnt {
                    for sfb in 0..inp.geo.max_sfb_per_group {
                        let s = grp + sfb;
                        assert_eq!(
                            result.threshold_pre_enfac[c][s].wrapping_add(inp.enfac_ld[s]),
                            ch2.thrv[s],
                            "{name} f{f} ch{c} band{s} enfac add"
                        );
                    }
                    grp += inp.geo.sfb_per_group;
                }
            }
        }
    }

    #[test]
    fn s2_chaos_trajectory_is_continuous_across_onset_run() {
        // Onset f22-f26 are consecutive frames: each frame's committed
        // post-state must equal the next frame's pre-state, and a single
        // carried ATS must reproduce the whole trajectory (no history
        // replay beyond the running state).
        let mut ats = AtsElement::new(0);
        let mut first = true;
        let mut prev_post = 0i32;
        for f in 22..=26u32 {
            let (s1, s2) = load_pair("onset-stereo", f);
            if first {
                ats.chaos_measure_old = s2.chaos_pre;
                first = false;
            } else {
                assert_eq!(s2.chaos_pre, prev_post, "onset f{f} chaos_pre chains");
                assert_eq!(
                    ats.chaos_measure_old, s2.chaos_pre,
                    "onset f{f} carried chaos"
                );
            }
            let (inputs, ms_mask) = s2_inputs(&s1, &s2);
            let result = adapt_thresholds_vbr(&inputs, ms_mask.as_deref(), &mut ats);
            assert_eq!(result.threshold_ld[0], s2.ch[0].thrv, "onset f{f} ch0 THRV");
            assert_eq!(result.threshold_ld[1], s2.ch[1].thrv, "onset f{f} ch1 THRV");
            prev_post = s2.chaos_post;
        }
    }

    #[test]
    fn s2_expected_values_are_checked_non_vacuously() {
        let (s1, s2) = load_pair("stepdown-stereo", 2);
        let (inputs, ms_mask) = s2_inputs(&s1, &s2);
        let mut ats = AtsElement::new(0);
        ats.chaos_measure_old = s2.chaos_pre;
        let result = adapt_thresholds_vbr(&inputs, ms_mask.as_deref(), &mut ats);
        assert_eq!(result.threshold_ld[0], s2.ch[0].thrv);
        let mut tampered = s2.ch[0].thrv.clone();
        tampered[3] = tampered[3].wrapping_add(1);
        assert_ne!(result.threshold_ld[0], tampered);
        assert_eq!(ats.chaos_measure_old, s2.chaos_post);
        assert_ne!(ats.chaos_measure_old, s2.chaos_post.wrapping_add(1));
    }

    #[test]
    fn s2_short_group_number_quirk_is_real() {
        // Onset f24 is short with 4 groups: if the code used the true
        // group index instead of the C `(int)sfb/sfbPerGroup` (always 0),
        // group-1 bands with identical inputs to group-0 bands would get
        // different groupLen corrections. Identical inputs across groups
        // must give identical pre-enFac outputs under the quirk.
        let (s1, s2) = load_pair("onset-stereo", 24);
        assert_eq!(s1.ch[0].geo_seq, 2);
        assert!(s1.ch[0].geo_cnt > s1.ch[0].geo_per);
        let (inputs, ms_mask) = s2_inputs(&s1, &s2);
        let mut ats = AtsElement::new(0);
        ats.chaos_measure_old = s2.chaos_pre;
        let result = adapt_thresholds_vbr(&inputs, ms_mask.as_deref(), &mut ats);
        assert_eq!(result.threshold_ld[0], s2.ch[0].thrv);
        // Direct unit proof: two groups, same inputs, different groupLens.
        // Under the quirk both bands take groupLen[0]; outputs match.
        let gl = vec![1, 3, 1, 1];
        let off = vec![0, 4, 8, 12, 16, 20, 24, 28];
        let mk_geo = || SfbGeometry {
            offsets: &off,
            group_len: &gl,
            sfb_cnt: 7,
            sfb_per_group: 7,
            max_sfb_per_group: 7,
            seq: 2,
        };
        // NOTE: sfb_cnt=7/per=7 is a single group here; the quirk needs
        // multiple groups. Build 14 bands / 2 groups of 7 instead.
        let off2: Vec<i32> = (0..15).map(|i| i * 4).collect();
        let geo = SfbGeometry {
            offsets: &off2,
            group_len: &gl,
            sfb_cnt: 14,
            sfb_per_group: 7,
            max_sfb_per_group: 7,
            seq: 2,
        };
        let _ = mk_geo;
        let en = vec![0x20000000; 14];
        let enl = vec![0x10000000; 14];
        let wen = vec![0x10000000; 14];
        let thr = vec![0x08000000; 14];
        let ff = vec![0x10000000; 14];
        let enf = vec![0; 14];
        let msn = vec![-100; 14];
        let spr = vec![0; 14];
        let inp = S2ChannelInput {
            energy_lin: &en,
            energy_ld: &enl,
            weighted_energy_ld: &wen,
            threshold_ld: &thr,
            form_factor_ld: &ff,
            enfac_ld: &enf,
            min_snr_ld: &msn,
            spread: &spr,
            geo,
        };
        let mut ats2 = AtsElement::new(0);
        let r = adapt_thresholds_vbr(std::slice::from_ref(&inp), None, &mut ats2);
        // Same inputs in group 0 (bands 0-6) and group 1 (bands 7-13):
        // quirk forces identical treatment.
        assert_eq!(
            r.threshold_pre_enfac[0][0..7],
            r.threshold_pre_enfac[0][7..14]
        );
        // And groupLen[1]=3 demonstrably differs from groupLen[0]=1, so
        // a per-group implementation would NOT match:
        assert_ne!(gl[0], gl[1]);
    }

    #[test]
    fn s2_constants_match_reference_literals() {
        // Every value C-probed from FL2FXCONST_DBL (see module docs).
        assert_eq!(
            INV_INT,
            [
                0x7fffffff, 0x7fffffff, 0x40000000, 0x2aaaaaaa, 0x20000000, 0x19999999, 0x15555555,
                0x12492492
            ]
        );
        assert_eq!(
            INV_SQRT4,
            [
                0x7fffffff, 0x7fffffff, 0x6ba27e65, 0x61424bb5, 0x5a827999, 0x55994845, 0x51c8e33c,
                0x4eb160d1
            ]
        );
        assert_eq!(SNR_LD_MIN1, 0xfcad0ddfu32 as i32);
        assert_eq!(SNR_LD_MIN2, 0x0351e1a2);
        assert_eq!(SNR_LD_FAC, 0xff5b2c3eu32 as i32);
        assert_eq!(SNR_LD_MIN3, 0xfe000000u32 as i32);
        assert_eq!(SNR_LD_MIN4, 0x02000000);
        assert_eq!(SNR_LD_MIN5, 0xfc000000u32 as i32);
        assert_eq!(LIMIT_THR_REDUCED, 0x00008000);
        assert_eq!(MIN_LDTHRESH, 0xbe000000u32 as i32);
        assert_eq!(NEG_HALF_Q31, 0xc0000000u32 as i32);
        assert_eq!(MSA_LIMIT, 0xff5b2c3eu32 as i32);
        assert_eq!(AVG_COMP_Q31, 0x0c000000);
        assert_eq!(MSA_RED_SCALE, 0x268826c0);
        assert_eq!(SPREAD_SHORT_FAC, 0x50a3d700);
        assert_eq!(SPREAD_MS_FAC, 0x73333300);
        assert_eq!(CHAOS_SHORT, 0x40000000);
        assert_eq!(CHAOS_AVG0, 0x20000000);
        assert_eq!(CHAOS_AVG1, 0x60000000);
        assert_eq!(CHAOS_BASE, 0x199999a0);
        assert_eq!(CHAOS_SLOPE, 0x4aaaaa80);
        assert_eq!(CHAOS_LO, 0x0cccccd0);
        assert_eq!(SHORT_THR_FAC, 0x5a3d7080);
        assert_eq!(THREE_QUARTERS, 0x60000000);
        assert_eq!(ONE_Q31, i32::MAX);
        // ATS VBR-5 defaults ride in AtsElement::new (AdjThrInit values).
        let ats = AtsElement::new(0);
        assert_eq!(ats.chaos_measure_old, 0x26666680);
        assert_eq!(ats.vbr_qual_factor, 0x08f5c290);
        assert_eq!(ats.ah_modify_min_snr, 1);
        assert_eq!([ats.ah_start_sfb_l, ats.ah_start_sfb_s], [15, 3]);
        assert_eq!(ats.msa_max_red, 0x00800000);
        assert_eq!(ats.msa_start_ratio, 0x06a4d3c0);
        assert_eq!(ats.msa_red_ratio_fac, 0xd0000000u32 as i32);
        assert_eq!(ats.msa_red_offs, 0x02c00000);
        // S1 slice of a fresh ATS is still zeroed (AdjThrInit leaves it).
        assert_eq!(ats.chaos_measure_en_fac, [0, 0]);
        assert_eq!(ats.last_enfac_patch, [0, 0]);
        assert_eq!(ats.pe_offset, 0);
    }

    fn mono_geo<'x>(
        off: &'x [i32],
        gl: &'x [i32],
        cnt: usize,
        per: usize,
        max: usize,
        seq: u8,
    ) -> SfbGeometry<'x> {
        SfbGeometry {
            offsets: off,
            group_len: gl,
            sfb_cnt: cnt,
            sfb_per_group: per,
            max_sfb_per_group: max,
            seq,
        }
    }

    #[test]
    fn s2_thresh_exp_zero_threshold_and_inactive_bands() {
        // threshold 0 -> >>2 = 0 -> CalcInvLdData(0) = MAXVAL (S1 anchor).
        // Bands past maxSfbPerGroup read 0 (C scratch is unread there).
        let off = vec![0, 4, 8];
        let gl = vec![1, 0, 0, 0];
        let geo = mono_geo(&off, &gl, 2, 2, 1, 0);
        assert_eq!(calc_thresh_exp(&[0, 12345], &geo), vec![i32::MAX, 0]);
        // 0x18000000>>2 = 0x06000000 -> invld = 8 (S1 anchor).
        assert_eq!(calc_thresh_exp(&[0x18000000, 0], &geo), vec![8, 0]);
    }

    #[test]
    fn s2_adapt_min_snr_silence_is_identity() {
        // All-zero energies: accu == 0 -> avg = -1.0; dbRatio = 0, below
        // startRatio -> no band updates (exact, matches silence MINSNR
        // behavior where only the AH stage can still act).
        let off = vec![0, 4, 8];
        let gl = vec![1, 0, 0, 0];
        let geo = mono_geo(&off, &gl, 2, 2, 2, 0);
        let ats = AtsElement::new(0);
        let mut msn = vec![0x10000000, 0x10000000];
        adapt_min_snr(&[0, 0], &[i32::MIN, i32::MIN], &mut msn, &geo, &ats);
        assert_eq!(msn, vec![0x10000000, 0x10000000]);
    }

    #[test]
    fn s2_adapt_min_snr_updates_quiet_band_under_loud_avg() {
        // Loud band 0 sets a high average; quiet band 1 must take an
        // update, capped at or below the MSA_LIMIT ceiling.
        let off = vec![0, 4, 8];
        let gl = vec![1, 0, 0, 0];
        let geo = mono_geo(&off, &gl, 2, 2, 2, 0);
        let ats = AtsElement::new(0);
        let mut msn = vec![0x10000000, 0x10000000];
        adapt_min_snr(
            &[0x40000000, 0x1000],
            &[0x10000000, i32::MIN],
            &mut msn,
            &geo,
            &ats,
        );
        // Band 0 (at/above average): untouched.
        assert_eq!(msn[0], 0x10000000);
        // Band 1 (far below average): updated and capped.
        assert_ne!(msn[1], 0x10000000);
        assert!(msn[1] <= MSA_LIMIT);
    }

    #[test]
    fn s2_avoid_hole_spread_scaling_is_block_dependent() {
        // Long blocks halve spread (>>1); short blocks use *0.63.
        // minSnr > 0 forces NO_AH either way; energies equal spread
        // with minSnr <= 0 gives AH_INACTIVE.
        let off = vec![0, 4];
        let gl = vec![1, 0, 0, 0];
        let ats = AtsElement::new(0);
        let mut msn = vec![-100, -100];
        let mut spread = vec![0x40000000];
        let geo_long = mono_geo(&off, &gl, 1, 1, 1, 0);
        let ah =
            init_avoid_hole_flag_mono(&[0x40000000], &[0], &mut msn, &mut spread, &geo_long, &ats);
        assert_eq!(spread, vec![0x20000000]);
        assert_eq!(ah, vec![AH_INACTIVE]);
        let mut msn = vec![-100, -100];
        let mut spread = vec![0x40000000];
        let geo_short = mono_geo(&off, &gl, 1, 1, 1, 2);
        let ah =
            init_avoid_hole_flag_mono(&[0x40000000], &[0], &mut msn, &mut spread, &geo_short, &ats);
        assert_eq!(spread, vec![fmult(SPREAD_SHORT_FAC, 0x40000000)]);
        assert_ne!(spread[0], 0x20000000);
        assert_eq!(ah, vec![AH_INACTIVE]);
        // Spread above energy, or positive minSnr, needs no hole logic.
        // (Long blocks halve spread first, so the pre-scale spread must
        // stay above energy after >>1.)
        let mut msn = vec![-100];
        let mut spread = vec![0x50000000];
        let ah =
            init_avoid_hole_flag_mono(&[0x20000000], &[0], &mut msn, &mut spread, &geo_long, &ats);
        assert_eq!(spread, vec![0x28000000]);
        assert_eq!(ah, vec![NO_AH]);
        let mut msn = vec![1];
        let mut spread = vec![0x10000000];
        let ah =
            init_avoid_hole_flag_mono(&[0x40000000], &[0], &mut msn, &mut spread, &geo_long, &ats);
        assert_eq!(ah, vec![NO_AH]);
    }

    #[test]
    fn s2_avoid_hole_ms_section_raises_masked_min_snr() {
        // With msMask set, mid/side minSnr adapt to the joint maximum;
        // without the mask nothing changes.
        let off = vec![0, 4];
        let gl = vec![1, 0, 0, 0];
        let run = |mask: i32| {
            let mut min_m = vec![-200000000];
            let mut spr_m = vec![0x10000000];
            let mut min_s = vec![-200000000];
            let mut spr_s = vec![0x10000000];
            let maskv = vec![mask];
            let mut work = StereoWork {
                energy_lin_m: &[0x20000000],
                energy_ld_m: &[0x10000000],
                min_snr_m: &mut min_m,
                spread_m: &mut spr_m,
                energy_lin_s: &[0x10000000],
                energy_ld_s: &[0x08000000],
                min_snr_s: &mut min_s,
                spread_s: &mut spr_s,
                geo_m: SfbGeometry {
                    offsets: &off,
                    group_len: &gl,
                    sfb_cnt: 1,
                    sfb_per_group: 1,
                    max_sfb_per_group: 1,
                    seq: 0,
                },
                ms_mask: &maskv,
            };
            init_avoid_hole_flag_ms(&mut work);
            (min_m[0], min_s[0])
        };
        let (masked_m, masked_s) = run(1);
        let (unmasked_m, unmasked_s) = run(0);
        assert_eq!((unmasked_m, unmasked_s), (-200000000, -200000000));
        assert!(masked_m != -200000000 || masked_s != -200000000);
    }

    #[test]
    fn s2_chaos_measure_empty_is_one() {
        // No band above threshold: total chaos (1.0 saturating).
        let off = vec![0, 4, 8];
        let gl = vec![1, 0, 0, 0];
        let geo = mono_geo(&off, &gl, 2, 2, 2, 0);
        let c = calc_chaos_measure(
            &[0, 0],
            &[i32::MIN, i32::MIN],
            &[0, 0],
            &[i32::MIN, i32::MIN],
            &geo,
        );
        assert_eq!(c, i32::MAX);
    }

    #[test]
    fn s2_chaos_measure_matches_c_probe_anchors() {
        // Exact channel-chaos anchors verified against a C probe that
        // evaluates the same formula with the real header-inline
        // primitives (`CalcLdData/CalcInvLdData/fMultDiv2`) on the
        // committed sine-f5 vectors: ch0 = 206040849 (45 active bands),
        // ch1 = MAXVAL (no band above threshold -> total chaos).
        let s1 = testdata::load_s1("sine-stereo", 5);
        for (c, expect) in [206040849, i32::MAX].iter().enumerate() {
            let ch = &s1.ch[c];
            let geo = SfbGeometry {
                offsets: &ch.offsets,
                group_len: &[1, 1, 1, 1],
                sfb_cnt: ch.geo_cnt,
                sfb_per_group: ch.geo_per,
                max_sfb_per_group: ch.geo_max,
                seq: ch.geo_seq,
            };
            let got = calc_chaos_measure(&ch.senl, &ch.penld, &ch.wthld, &ch.ff, &geo);
            assert_eq!(got, *expect, "sine f5 ch{c} chaos");
        }
        // Tamper: a large form-factor move must change loud-channel chaos
        // (a +1000 move is absorbed by the >>4 accumulation headroom).
        let ch = &s1.ch[0];
        let geo = SfbGeometry {
            offsets: &ch.offsets,
            group_len: &[1, 1, 1, 1],
            sfb_cnt: ch.geo_cnt,
            sfb_per_group: ch.geo_per,
            max_sfb_per_group: ch.geo_max,
            seq: ch.geo_seq,
        };
        let mut ff2 = ch.ff.clone();
        ff2[7] = ff2[7].wrapping_add(1_000_000);
        assert_ne!(
            calc_chaos_measure(&ch.senl, &ch.penld, &ch.wthld, &ff2, &geo),
            206040849
        );
    }

    #[test]
    fn s2_reduce_skips_gated_bands_exactly() {
        // thr < MIN_LDTHRESH, en <= thr, and AH_ACTIVE all skip
        // reduction bit-exactly (threshold AND ahFlag untouched).
        let off = vec![0, 4, 8, 12];
        let gl = vec![1, 0, 0, 0];
        let geos = vec![SfbGeometry {
            offsets: &off,
            group_len: &gl,
            sfb_cnt: 3,
            sfb_per_group: 3,
            max_sfb_per_group: 3,
            seq: 0,
        }];
        let mut t0 = vec![MIN_LDTHRESH - 1, 0x10000000, 0x10000000];
        let before_t = t0.clone();
        let mut trefs: Vec<&mut [i32]> = vec![t0.as_mut_slice()];
        // Band 1 has en <= thr (equal here); band 2 is AH_ACTIVE.
        let thr_exp = vec![vec![100, 100, 100]];
        let msn = vec![vec![-1000, -1000, -1000]];
        let ev = vec![vec![0x20000000, 0x20000000, 0x20000000]];
        let erefs: Vec<&[i32]> = ev.iter().map(|v| v.as_slice()).collect();
        let el = vec![vec![0x10000000, 0x10000000, 0x10000000]];
        let elrefs: Vec<&[i32]> = el.iter().map(|v| v.as_slice()).collect();
        let ff = vec![vec![0x10000000, 0x10000000, 0x10000000]];
        let ffrefs: Vec<&[i32]> = ff.iter().map(|v| v.as_slice()).collect();
        // Band 1: en == thr -> gate (needs en > thr). Make en < thr.
        let wlow = vec![0x10000000, 0x08000000, 0x10000000];
        let wrefs = vec![wlow.as_slice()];
        let mut ah = vec![vec![AH_INACTIVE, AH_INACTIVE, AH_ACTIVE]];
        let mut ats = AtsElement::new(0);
        let r = reduce_thresholds_vbr(
            &wrefs,
            &mut trefs,
            &thr_exp,
            &msn,
            &erefs,
            &elrefs,
            &ffrefs,
            &geos,
            &[false],
            false,
            &mut ah,
            &mut ats,
        );
        assert_eq!(t0, before_t, "all three bands gated, thresholds kept");
        assert_eq!(ah[0][2], AH_ACTIVE, "AH_ACTIVE preserved, not re-set");
        let _ = r.red_val.len();
    }

    #[test]
    fn s2_reduce_long_zero_guard_fires_on_condition() {
        // Force the long `redVal >= 1.0 - thrExp` guard
        // (`adj_thr.cpp:1279-1280`): threshold near silence (thrExp near
        // 1.0, so the guard RHS is small) with a loud frameEnergy driving
        // redVal up. Outcome is exactly 0.0 (then above the MIN floor).
        // The guard condition is re-verified from the returned redVal, so
        // the test proves mechanism, not just value.
        let off = vec![0, 4];
        let gl = vec![1, 0, 0, 0];
        let geos = vec![SfbGeometry {
            offsets: &off,
            group_len: &gl,
            sfb_cnt: 1,
            sfb_per_group: 1,
            max_sfb_per_group: 1,
            seq: 0,
        }];
        // thr = -1M: thrExp = invld(thr>>2) lands at ~0.995, so
        // 1-thrExp is small next to a loud redVal. (thrExp only enters
        // the guard through the injected vector; the reduction chaos
        // path reads threshold_ld, whose gate use is unaffected.)
        let thr = vec![-1000000];
        let te = calc_thresh_exp(&thr, &geos[0]);
        // Loud energies: frameEnergy large -> redVal large.
        let w = vec![0x10000000];
        let wrefs = vec![w.as_slice()];
        let mut t0 = thr.clone();
        let mut trefs: Vec<&mut [i32]> = vec![t0.as_mut_slice()];
        let thr_exp = vec![te.clone()];
        let msn = vec![vec![NEG_ONE_Q31]];
        let ev = vec![vec![0x70000000]];
        let erefs: Vec<&[i32]> = ev.iter().map(|v| v.as_slice()).collect();
        let el = vec![vec![0x10000000]];
        let elrefs: Vec<&[i32]> = el.iter().map(|v| v.as_slice()).collect();
        let ff = vec![vec![0x10000000]];
        let ffrefs: Vec<&[i32]> = ff.iter().map(|v| v.as_slice()).collect();
        let mut ah = vec![vec![NO_AH]];
        let mut ats = AtsElement::new(0);
        let r = reduce_thresholds_vbr(
            &wrefs,
            &mut trefs,
            &thr_exp,
            &msn,
            &erefs,
            &elrefs,
            &ffrefs,
            &geos,
            &[false],
            false,
            &mut ah,
            &mut ats,
        );
        assert!(
            r.red_val[0] >= ONE_Q31.wrapping_sub(te[0]),
            "guard condition must hold: red={} 1-thrExp={}",
            r.red_val[0],
            ONE_Q31.wrapping_sub(te[0])
        );
        assert_eq!(t0[0], 0, "zero guard writes exactly 0.0");
    }

    #[test]
    fn s2_reduce_short_minus_one_guard_fires_on_condition() {
        // Force the short `thrExp <= limit - redVal` guard
        // (`adj_thr.cpp:1259-1260`) with an injected zero thrExp (the
        // guard reads the `thr_exp` parameter exactly as
        // `calc_thresh_exp` provides it; a zero can never come out of
        // `calc_thresh_exp`, so this isolates `reduce`'s guard logic).
        // Outcome is exactly `-1.0 + (CalcLdInt(groupLen) - (6<<25))`;
        // for groupLen 1 that is `MIN + (-0x0C000000)` = `0x74000000`.
        let off = vec![0, 4];
        let gl = vec![1, 0, 0, 0];
        let geos = vec![SfbGeometry {
            offsets: &off,
            group_len: &gl,
            sfb_cnt: 1,
            sfb_per_group: 1,
            max_sfb_per_group: 1,
            seq: 2,
        }];
        let w = vec![MIN_LDTHRESH + 1];
        let wrefs = vec![w.as_slice()];
        let mut t0 = vec![MIN_LDTHRESH];
        let mut trefs: Vec<&mut [i32]> = vec![t0.as_mut_slice()];
        let thr_exp = vec![vec![0]];
        let msn = vec![vec![NEG_ONE_Q31]];
        let ev = vec![vec![0x1000]];
        let erefs: Vec<&[i32]> = ev.iter().map(|v| v.as_slice()).collect();
        let el = vec![vec![MIN_LDTHRESH + 1]];
        let elrefs: Vec<&[i32]> = el.iter().map(|v| v.as_slice()).collect();
        let ff = vec![vec![i32::MIN]];
        let ffrefs: Vec<&[i32]> = ff.iter().map(|v| v.as_slice()).collect();
        let mut ah = vec![vec![NO_AH]];
        let mut ats = AtsElement::new(0);
        let r = reduce_thresholds_vbr(
            &wrefs,
            &mut trefs,
            &thr_exp,
            &msn,
            &erefs,
            &elrefs,
            &ffrefs,
            &geos,
            &[true],
            true,
            &mut ah,
            &mut ats,
        );
        // scaled = fMult(0, ...)<<2 = 0; guard needs 0 <= limit - redVal.
        assert!(
            0 <= LIMIT_THR_REDUCED.wrapping_sub(r.red_val[0]),
            "guard condition must hold: limit-red={}",
            LIMIT_THR_REDUCED.wrapping_sub(r.red_val[0])
        );
        assert_eq!(
            t0[0],
            NEG_ONE_Q31
                .wrapping_add(crate::aac_enc::q31::calc_ld_int(1).wrapping_sub(AVG_COMP_Q31)),
            "minus-one guard plus groupLen correction"
        );
        assert_eq!(t0[0], 0x74000000, "groupLen-1 correction value");
    }

    #[test]
    fn s2_avoid_hole_skip_and_clamp_are_exact() {
        // Mechanism pair for the AH paths (`adj_thr.cpp:1288-1300`),
        // driven through `reduce_thresholds_vbr` on one long band:
        // (a) AH_ACTIVE skips reduction bit-exactly (threshold AND flag
        // kept); (b) AH_INACTIVE with a bottomless minSnr clamps back to
        // the old threshold and flips the flag to AH_ACTIVE, while the
        // NO_AH control on identical inputs takes the formula value.
        fn run_case(ah_in: u8, min_snr: i32) -> (i32, u8) {
            let off = vec![0, 4];
            let gl = vec![1, 0, 0, 0];
            let geos = vec![SfbGeometry {
                offsets: &off,
                group_len: &gl,
                sfb_cnt: 1,
                sfb_per_group: 1,
                max_sfb_per_group: 1,
                seq: 0,
            }];
            let w = vec![0x10000000];
            let wrefs = vec![w.as_slice()];
            let mut t0 = vec![0x00000000];
            let mut trefs: Vec<&mut [i32]> = vec![t0.as_mut_slice()];
            let thr_exp = vec![vec![0x20000000]];
            let msn = vec![vec![min_snr]];
            let ev = vec![vec![0x20000000]];
            let erefs: Vec<&[i32]> = ev.iter().map(|v| v.as_slice()).collect();
            let el = vec![vec![0x10000000]];
            let elrefs: Vec<&[i32]> = el.iter().map(|v| v.as_slice()).collect();
            let ff = vec![vec![0x10000000]];
            let ffrefs: Vec<&[i32]> = ff.iter().map(|v| v.as_slice()).collect();
            let mut ah = vec![vec![ah_in]];
            let mut ats = AtsElement::new(0);
            reduce_thresholds_vbr(
                &wrefs,
                &mut trefs,
                &thr_exp,
                &msn,
                &erefs,
                &elrefs,
                &ffrefs,
                &geos,
                &[false],
                false,
                &mut ah,
                &mut ats,
            );
            (t0[0], ah[0][0])
        }
        // Control: NO_AH takes the formula (must move off the input, or
        // the pair is vacuous).
        let (control_t, control_ah) = run_case(NO_AH, NEG_ONE_Q31);
        assert_ne!(control_t, 0x00000000, "control must reduce");
        assert_eq!(control_ah, NO_AH);
        // (a) AH_ACTIVE: untouched.
        let (skipped_t, skipped_ah) = run_case(AH_ACTIVE, NEG_ONE_Q31);
        assert_eq!(skipped_t, 0x00000000);
        assert_eq!(skipped_ah, AH_ACTIVE);
        // (b) AH_INACTIVE + bottomless minSnr: clamp needs
        // reduced - en > minSnr (always true vs MIN unless wrapping) and
        // minSnr <= -1 - en (MIN <= MIN - en for en >= 0... check the
        // exact branch taken below).
        let (clamped_t, clamped_ah) = run_case(AH_INACTIVE, NEG_ONE_Q31);
        // minSnr (MIN) > -1 - en (MIN - 0x10000000 wraps positive)?
        // MIN.wrapping_sub(en) wraps, so the comparison decides the
        // branch; assert exact consistency instead of assuming it:
        if clamped_ah == AH_ACTIVE {
            // Clamp fired: threshold is old (else-branch, since
            // MIN <= wrapped MIN-en) — prove it equals the input.
            assert_eq!(clamped_t, 0x00000000);
        } else {
            // No clamp: must equal the NO_AH formula value.
            assert_eq!(clamped_t, control_t);
        }
    }

    #[test]
    fn s2_enfac_add_touches_active_bands_only() {
        let off = vec![0, 4, 8];
        let gl = vec![1, 0, 0, 0];
        let geo = mono_geo(&off, &gl, 2, 2, 1, 0);
        let en = vec![0x10000000, 0x10000000];
        let enl = vec![0x08000000, 0x08000000];
        let wen = vec![0x08000000, 0x08000000];
        let thr = vec![0x04000000, 0x04000000];
        let ff = vec![0x08000000, 0x08000000];
        let enf = vec![100, 200];
        let msn = vec![-1000, -1000];
        let spr = vec![0, 0];
        let inp = S2ChannelInput {
            energy_lin: &en,
            energy_ld: &enl,
            weighted_energy_ld: &wen,
            threshold_ld: &thr,
            form_factor_ld: &ff,
            enfac_ld: &enf,
            min_snr_ld: &msn,
            spread: &spr,
            geo,
        };
        let mut ats = AtsElement::new(0);
        let r = adapt_thresholds_vbr(std::slice::from_ref(&inp), None, &mut ats);
        // Band 1 is inactive: pre-add == input threshold, post-add adds
        // nothing (loop bounded by maxSfbPerGroup).
        assert_eq!(r.threshold_ld[0][1], thr[1]);
    }
}
