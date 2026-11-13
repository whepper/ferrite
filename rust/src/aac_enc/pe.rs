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
//! M4-S1: form-factor and perceptual-entropy (PE) calculation.
//!
//! Ports the `FDKaacEnc_QCMainPrepare` front half (`qc_main.cpp:435-452`):
//! `FDKaacEnc_CalcFormFactor` (`sf_estim.cpp:132-174`) and
//! `FDKaacEnc_peCalculation` (`adj_thr.cpp:908-944`, via `line_pe.cpp:116-234`
//! and `adj_thr.cpp:719-906`). Static-bit accounting lives in
//! [`super::static_bits`]; threshold adaptation (S2), quantization and the
//! afterburner (S3+) are out of scope.
//!
//! Numeric convention: every value is a Q31 `i32` in the exact fixed-point
//! domain of the C reference (`FIXP_DBL`), and every op mirrors the C
//! integer op (wrapping adds, arithmetic shifts, ARM64 `fMult` floor) via
//! [`super::q31`]. Given identical inputs the outputs are bit-exact; the
//! single upstream approximation (float-to-Q31 spectrum mapping in M2/M3)
//! stays outside this module by construction (callers pass Q31 vectors).

use super::q31::{
    calc_inv_ld_data, calc_ld_data, calc_ld_int, fdiv_norm, fmult, fmult_div2, fmult_i, sqrt_fixp,
};

/// `FORM_FAC_SHIFT` (`sf_estim.h:112`): per-line form-factor downshift.
pub const FORM_FAC_SHIFT: i32 = 6;

/// `sfbFormFactor` scale compensation (`line_pe.cpp:126-127`):
/// `(float)FORM_FAC_SHIFT/LD_DATA_SCALING` as Q31 (C-probed `0x0C000000`).
const FORM_FAC_SCALE_Q31: i32 = 0x0C000000;

/// PE line constants (`line_pe.cpp:109-113`), C-probed Q31 literals:
/// `C1 = log(8)/log(2)` and `C2 = log(2.5)/log(2)` in Ld units
/// (log2/64). `C3 = 1-C2/C1` is a DOUBLE literal (`0.5593573`, no `f`
/// suffix — distinct from `sf_estim.cpp`'s float `PE_C3` used in S4);
/// C-probed as `0x4799051F`.
const C1_LD_Q31: i32 = 0x06000000;
const C2_LD_Q31: i32 = 0x02A4D3C3;
const C3_Q31: i32 = 0x4799051F;

/// `chaosMeasureEnFac` floor (`adj_thr.cpp:822-824`): `0.1875` as Q31.
const ENFAC_FLOOR_Q31: i32 = 0x18000000;
/// EnFac patch gates (`adj_thr.cpp:826-853`): `0.78125` (arm), `0.8125`
/// (3/4-energy exponent) and `0.796875` (2/4-energy exponent) as Q31.
const PATCH_ARM_Q31: i32 = 0x64000000;
const PATCH_HI_Q31: i32 = 0x68000000;
const PATCH_MID_Q31: i32 = 0x66000000;
/// Short-element EnFac state (`adj_thr.cpp:873-875`): `0.75` as Q31.
const SHORT_ENFAC_Q31: i32 = 0x60000000;

/// Q31 `-1.0` sentinel (`FL2FXCONST_DBL(-1.0f)`): log domain for zero.
pub const LD_NEG_ONE_Q31: i32 = i32::MIN;

/// Scalefactor-DPCM Huffman lengths (`aacEnc_rom.cpp:281-292`,
/// `FDKaacEnc_huff_ltabscf[121]`, index `delta + 60`), verbatim.
///
/// `pub(crate)`: shared single-source with the M4-S7 encode half
/// (`bitenc::code_scalefactor_delta`), which must use the same lengths
/// the S1/S5 counters price (count/encode agreement, plan Q6).
pub(crate) const HUFF_LTABSCF: [u8; 121] = [
    0x12, 0x12, 0x12, 0x12, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13,
    0x13, 0x13, 0x13, 0x12, 0x13, 0x12, 0x11, 0x11, 0x10, 0x11, 0x10, 0x10, 0x10, 0x10, 0x0f, 0x0f,
    0x0e, 0x0e, 0x0e, 0x0e, 0x0e, 0x0e, 0x0d, 0x0d, 0x0c, 0x0c, 0x0c, 0x0b, 0x0c, 0x0b, 0x0a, 0x0a,
    0x0a, 0x09, 0x09, 0x08, 0x08, 0x08, 0x07, 0x06, 0x06, 0x05, 0x04, 0x03, 0x01, 0x04, 0x04, 0x05,
    0x06, 0x06, 0x07, 0x07, 0x08, 0x08, 0x09, 0x09, 0x0a, 0x0a, 0x0a, 0x0b, 0x0b, 0x0b, 0x0b, 0x0c,
    0x0c, 0x0d, 0x0d, 0x0d, 0x0e, 0x0e, 0x10, 0x0f, 0x10, 0x0f, 0x12, 0x13, 0x13, 0x13, 0x13, 0x13,
    0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13,
    0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13,
];

/// `FDKaacEnc_bitCountScalefactorDelta` (`bit_cnt.h:192-198`): Huffman
/// length of a scalefactor DPCM delta (`|delta| <= 60`, `CODE_BOOK_SCF_LAV`).
pub fn bit_count_scf_delta(delta: i32) -> i32 {
    debug_assert!((-60..=60).contains(&delta));
    HUFF_LTABSCF[(delta + 60) as usize] as i32
}

/// Per-channel PE state (`line_pe.h:112-122`).
#[derive(Debug, Clone, Default)]
pub struct PeChannelData {
    /// Relevant lines per band (`prepareSfbPe` output).
    pub sfb_n_lines: Vec<i32>,
    /// PE per band (`calcSfbPe` output).
    pub sfb_pe: Vec<i32>,
    /// Constant part per band.
    pub sfb_const_part: Vec<i32>,
    /// Active lines per band.
    pub sfb_n_active_lines: Vec<i32>,
    /// Element sums.
    pub pe: i32,
    pub const_part: i32,
    pub n_active_lines: i32,
}

/// Element PE state (`line_pe.h:124-130`).
#[derive(Debug, Clone, Default)]
pub struct PeData {
    pub channels: Vec<PeChannelData>,
    pub pe: i32,
    pub const_part: i32,
    pub n_active_lines: i32,
    pub offset: i32,
}

/// Minimal ATS state read by `peCalculation`: `peOffset`,
/// `chaosMeasureEnFac[2]` (Q31) and `lastEnFacPatch[2]` (C `INT` boolean).
/// Fresh encoder RAM is zeroed and `AdjThrInit` leaves these exact fields
/// at zero except `peOffset` (see [`AtsElement::new`]).
///
/// The same struct also carries the S2 (VBR threshold) slice of
/// `ATS_ELEMENT` (`adj_thr_data.h:139-166`): `chaosMeasureOld`,
/// `vbrQualFactor`, `ahParam` and `minSnrAdaptParam`. `AtsElement::new`
/// initializes them to the exact `AdjThrInit` values for the stripped
/// VBR-5 configuration (single-element, `chBitrate >= 20000`), so S1-only
/// callers get correct S2 defaults for free.
#[derive(Debug, Clone)]
pub struct AtsElement {
    pub pe_offset: i32,
    pub chaos_measure_en_fac: [i32; 2],
    pub last_enfac_patch: [i32; 2],
    /// `chaosMeasureOld` (`adj_thr.cpp:2545`): starts at `0.3`, evolves in
    /// `reduceThresholdsVBR`.
    pub chaos_measure_old: i32,
    /// `vbrQualFactor` (`adj_thr.cpp:2552`): `0.070` for VBR-5
    /// (`tableVbrQualFactor`, `qc_main.cpp:131-132`).
    pub vbr_qual_factor: i32,
    /// `ahParam.modifyMinSnr` (`adj_thr.cpp:2559-2567`): TRUE at VBR-5
    /// rates (`chBitrate >= 20000`).
    pub ah_modify_min_snr: i32,
    /// `ahParam.startSfbL/S` (`adj_thr.cpp:2561-2562`): 15/3 at VBR-5.
    pub ah_start_sfb_l: i32,
    pub ah_start_sfb_s: i32,
    /// `minSnrAdaptParam` (`adj_thr.cpp:2570-2581`): `maxRed`,
    /// `startRatio`, `redRatioFac`, `redOffs` as Q31.
    pub msa_max_red: i32,
    pub msa_start_ratio: i32,
    pub msa_red_ratio_fac: i32,
    pub msa_red_offs: i32,
}

/// `peOffset` per `FDKaacEnc_AdjThrInit` (`adj_thr.cpp:2553-2556` in FDK-AAC
/// v3.0.0): 0, except when the per-channel bitrate is below 32 kbit/s, where
/// `peOffset = max(50, 100 - fMultI(0x666667, chBitrate))`.
///
/// Verified against the real `FDKaacEnc_QCInit` (which calls the real
/// `AdjThrInit`) for every VBR-1..5 operating point at 44100/48000 Hz,
/// mono/stereo: the only Ferrite operating point taking the branch is VBR-1
/// stereo (20 kbit/s per channel -> 50); all others yield 0.
pub(crate) fn vbr_pe_offset(channel_bitrate: u32) -> i32 {
    if channel_bitrate < 32000 {
        50.max(100 - super::q31::fmult_i(0x666667, channel_bitrate as i32))
    } else {
        0
    }
}

/// `AdjThrInit` shared values, C-probed Q31 literals (see the S2 literal
/// table in [`super::threshold_vbr`]). `chaosMeasureOld`, `ahParam` and
/// `minSnrAdaptParam` are identical for every VBR-1..5 operating point
/// (verified via real `FDKaacEnc_QCInit` at 44100/48000 Hz, mono/stereo);
/// only `vbrQualFactor` (per profile) and `peOffset` (VBR-1 stereo only,
/// see [`vbr_pe_offset`]) vary.
pub(crate) const ATS_VBR5_CHAOS_OLD: i32 = 0x26666680;
pub(crate) const ATS_VBR5_MSA_MAX_RED: i32 = 0x00800000;
pub(crate) const ATS_VBR5_MSA_START_RATIO: i32 = 0x06a4d3c0;
pub(crate) const ATS_VBR5_MSA_RED_RATIO_FAC: i32 = 0xd0000000u32 as i32;
pub(crate) const ATS_VBR5_MSA_RED_OFFS: i32 = 0x02c00000;

impl AtsElement {
    /// Fresh element state. `pe_offset` is explicit (no silent default):
    /// the VBR-5/VBR-6 value is 0 because `AdjThrInit` (`adj_thr.cpp:2553-2556`)
    /// only assigns a nonzero offset below 32 kbit/s per channel while
    /// VBR-5/VBR-6 run at 96/112 kbit/s per channel. The S2 slice is set to
    /// the `AdjThrInit` VBR-5 values (shared by VBR-6, which inherits all
    /// VBR-5 psychoacoustic settings except bandwidth).
    ///
    /// This constructor preserves the historical VBR-5/VBR-6 defaults;
    /// use [`AtsElement::for_vbr_mode`] for an explicit profile.
    pub fn new(pe_offset: i32) -> Self {
        Self::for_vbr_mode(crate::aac_enc::config::VbrMode::Vbr6, pe_offset)
    }

    /// Fresh element state for an explicit VBR profile. The quality factor
    /// comes from the profile table; `chaosMeasureOld`, `ahParam` and
    /// `minSnrAdaptParam` use the shared `AdjThrInit` values, verified
    /// identical for every VBR-1..5 operating point at 44100/48000 Hz
    /// (mono/stereo) via the real `FDKaacEnc_QCInit`. `pe_offset` remains
    /// an explicit argument: pass [`vbr_pe_offset`] of the per-channel
    /// bitrate (50 for VBR-1 stereo, 0 otherwise).
    pub fn for_vbr_mode(mode: crate::aac_enc::config::VbrMode, pe_offset: i32) -> Self {
        Self {
            pe_offset,
            chaos_measure_en_fac: [0, 0],
            last_enfac_patch: [0, 0],
            chaos_measure_old: ATS_VBR5_CHAOS_OLD,
            vbr_qual_factor: mode.profile().qual_q31(),
            ah_modify_min_snr: 1,
            ah_start_sfb_l: 15,
            ah_start_sfb_s: 3,
            msa_max_red: ATS_VBR5_MSA_MAX_RED,
            msa_start_ratio: ATS_VBR5_MSA_START_RATIO,
            msa_red_ratio_fac: ATS_VBR5_MSA_RED_RATIO_FAC,
            msa_red_offs: ATS_VBR5_MSA_RED_OFFS,
        }
    }
}

/// `FDKaacEnc_CalcFormFactorChannel` + `FDKaacEnc_CalcFormFactor`
/// (`sf_estim.cpp:132-174`): per-band `sum(sqrt(|spec|)) >> 6` accumulated
/// with wrap, then `CalcLdData`. `spectrum_q31` is the post-TNS rescaled
/// spectrum; `offsets` has at least `sfb_cnt + 1` entries. Bands past
/// `max_sfb_per_group` read the `-1.0` sentinel (debugging aid in C).
pub fn form_factor(
    spectrum_q31: &[i32],
    offsets: &[i32],
    sfb_cnt: usize,
    sfb_per_group: usize,
    max_sfb_per_group: usize,
) -> Vec<i32> {
    debug_assert!(sfb_per_group > 0 && max_sfb_per_group <= sfb_per_group);
    debug_assert!(offsets.len() > sfb_cnt);
    let mut out = vec![0i32; sfb_cnt];
    let mut grp = 0;
    while grp < sfb_cnt {
        for sfb in 0..max_sfb_per_group {
            let mut acc: i32 = 0;
            let start = offsets[grp + sfb] as usize;
            let stop = offsets[grp + sfb + 1] as usize;
            for &q in &spectrum_q31[start..stop] {
                acc = acc.wrapping_add(sqrt_fixp(q.wrapping_abs()) >> FORM_FAC_SHIFT);
            }
            out[grp + sfb] = calc_ld_data(acc);
        }
        for sfb in max_sfb_per_group..sfb_per_group {
            out[grp + sfb] = LD_NEG_ONE_Q31;
        }
        grp += sfb_per_group;
    }
    out
}

/// `FDKaacEnc_prepareSfbPe` (`line_pe.cpp:116-150`): estimated relevant
/// line count per band from energy/threshold/form-factor Ld vectors.
/// Bands at/below threshold read 0; the estimate clamps to the band width
/// (same `fMin` as C, guarding `CalcInvLdData` overshoot).
pub fn prepare_sfb_pe(
    energy_ld: &[i32],
    threshold_ld: &[i32],
    form_factor_ld: &[i32],
    offsets: &[i32],
    sfb_cnt: usize,
    sfb_per_group: usize,
    max_sfb_per_group: usize,
) -> Vec<i32> {
    let mut n_lines = vec![0i32; sfb_cnt];
    let mut grp = 0;
    while grp < sfb_cnt {
        for sfb in 0..max_sfb_per_group {
            let s = grp + sfb;
            if energy_ld[s] > threshold_ld[s] {
                let width = offsets[s + 1] - offsets[s];
                let avg = ((energy_ld[s].wrapping_neg() >> 1) + (calc_ld_int(width) >> 1)) >> 1;
                let est = calc_inv_ld_data(
                    form_factor_ld[s]
                        .wrapping_add(FORM_FAC_SCALE_Q31)
                        .wrapping_add(avg),
                );
                n_lines[s] = width.min(est);
            } else {
                n_lines[s] = 0;
            }
        }
        grp += sfb_per_group;
    }
    n_lines
}

/// `FDKaacEnc_calcSfbPe` (`line_pe.cpp:161-234`): PE, constant part and
/// active-line sums from weighted energies and thresholds.
///
/// Intensity bands (`is_book != 0` with energy at/below threshold) are an
/// observably dead branch in C: the scalefactor-cost provision written at
/// `line_pe.cpp:209-217` is unconditionally overwritten by the zeroed
/// temporaries at `:218-220`, so such bands contribute exactly 0 here
/// (their cost is counted later in S5 `scfCount`). This port replicates
/// the observable zeros; `is_scale` is accepted (and asserted in tests)
/// but likewise contributes nothing.
#[allow(clippy::too_many_arguments)]
pub fn calc_sfb_pe(
    n_lines: &[i32],
    weighted_energy_ld: &[i32],
    threshold_ld: &[i32],
    sfb_cnt: usize,
    sfb_per_group: usize,
    max_sfb_per_group: usize,
    is_book: &[i32],
    is_scale: &[i32],
) -> PeChannelData {
    const FACTOR_SHIFT: u32 = 6 + 16 + 1; // LD_DATA_SHIFT + PE_CONSTPART_SHIFT + 1
    let mut sfb_pe = vec![0i32; sfb_cnt];
    let mut sfb_const = vec![0i32; sfb_cnt];
    let mut sfb_nact = vec![0i32; sfb_cnt];
    let mut pe: i32 = 0;
    let mut const_part: i32 = 0;
    let mut n_active: i32 = 0;
    let mut grp = 0;
    while grp < sfb_cnt {
        for sfb in 0..max_sfb_per_group {
            let s = grp + sfb;
            let mut tmp_pe: i32 = 0;
            let mut tmp_const: i32 = 0;
            let mut tmp_nact: i32 = 0;
            if weighted_energy_ld[s] > threshold_ld[s] {
                let ratio = weighted_energy_ld[s].wrapping_sub(threshold_ld[s]);
                let n = n_lines[s];
                // `nLines << 23` in 32-bit INT (wraps like C on overflow).
                let factor = n.wrapping_shl(FACTOR_SHIFT);
                if ratio >= C1_LD_Q31 {
                    tmp_pe = fmult_div2(ratio, factor);
                    tmp_const = fmult_div2(weighted_energy_ld[s], factor);
                    tmp_nact = n;
                } else {
                    tmp_pe = fmult_div2(C2_LD_Q31.wrapping_add(fmult(C3_Q31, ratio)), factor);
                    tmp_const = fmult_div2(
                        C2_LD_Q31.wrapping_add(fmult(C3_Q31, weighted_energy_ld[s])),
                        factor,
                    );
                    tmp_nact = fmult_i(C3_Q31, n);
                }
            } else if is_book[s] != 0 {
                // Dead-store branch (see doc comment): C writes the IS
                // DPCM cost here, then overwrites it with zeros below.
                // `is_scale` is deliberately read so the caller contract
                // stays visible; it cannot affect the output.
                let _ = is_scale[s];
            }
            sfb_pe[s] = tmp_pe;
            sfb_const[s] = tmp_const;
            sfb_nact[s] = tmp_nact;
            pe = pe.wrapping_add(tmp_pe);
            const_part = const_part.wrapping_add(tmp_const);
            n_active = n_active.wrapping_add(tmp_nact);
        }
        grp += sfb_per_group;
    }
    PeChannelData {
        sfb_n_lines: n_lines.to_vec(),
        sfb_pe,
        sfb_const_part: sfb_const,
        sfb_n_active_lines: sfb_nact,
        pe: pe >> 16, // PE_CONSTPART_SHIFT == FRACT_BITS == 16
        const_part: const_part >> 16,
        n_active_lines: n_active,
    }
}

/// One channel's weighting input for [`calc_weighting`].
pub struct WeightingInput<'a> {
    pub n_lines: &'a [i32],
    pub energy_linear: &'a [i32],
    pub energy_ld: &'a [i32],
    pub offsets: &'a [i32],
    pub sfb_cnt: usize,
    pub sfb_per_group: usize,
    pub max_sfb_per_group: usize,
    pub short_block: bool,
}

/// `FDKaacEnc_calcWeighting` (`adj_thr.cpp:755-879`): spectral-flatness
/// energy factor (`sfbEnFacLd`) per band. The single call site
/// (`peCalculation`, `:919`) passes `usePatchTool = 1`, so the tool is
/// unconditionally enabled here.
///
/// Short elements take the early path (`enFac = 0.75`, patch armed);
/// long elements compute the flatness patch only across two consecutive
/// frames (`usePatch && lastEnFacPatch`) with MS-coupled bands of channel
/// 1 following channel 0's decision (`exePatchM`). State evolves in
/// `ats` exactly like the C `ATS_ELEMENT`.
pub fn calc_weighting(
    channels: &[WeightingInput<'_>],
    ms_mask: &[i32],
    ats: &mut AtsElement,
) -> Vec<Vec<i32>> {
    let nch = channels.len();
    debug_assert!(nch == 1 || nch == 2);
    if nch == 2 {
        debug_assert!(ms_mask.len() >= channels[0].sfb_cnt);
    }
    let mut enfac: Vec<Vec<i32>> = channels.iter().map(|ch| vec![0i32; ch.sfb_cnt]).collect();
    let no_short = channels.iter().all(|ch| !ch.short_block);
    let mut exe_patch_m: i32 = 0;
    for (ci, ch) in channels.iter().enumerate() {
        if !no_short {
            ats.chaos_measure_en_fac[ci] = SHORT_ENFAC_Q31;
            ats.last_enfac_patch[ci] = 1;
            continue;
        }
        let mut lines_sum: i32 = 0;
        let mut nrg_total: i32 = 0;
        let mut sum12: i32 = 0;
        let mut sum14: i32 = 0;
        let mut sum34: i32 = 0;
        let mut grp = 0;
        while grp < ch.sfb_cnt {
            for sfb in 0..ch.max_sfb_per_group {
                let s = grp + sfb;
                let f12 = calc_inv_ld_data(ch.energy_ld[s] >> 1);
                let f14 = calc_inv_ld_data(ch.energy_ld[s] >> 2);
                lines_sum = lines_sum.wrapping_add(ch.n_lines[s]);
                nrg_total = nrg_total.wrapping_add(ch.energy_linear[s] >> 6);
                sum12 = sum12.wrapping_add(f12 >> 6);
                sum14 = sum14.wrapping_add(f14 >> 6);
                sum34 = sum34.wrapping_add(fmult(f14, f12) >> 6);
            }
            grp += ch.sfb_per_group;
        }
        let nrg_total_ld = calc_ld_data(nrg_total);
        let fac14 = calc_ld_data(sum14).wrapping_sub(nrg_total_ld);
        let fac12 = calc_ld_data(sum12).wrapping_sub(nrg_total_ld);
        let fac34 = calc_ld_data(sum34).wrapping_sub(nrg_total_ld);
        let total_lines = ch.offsets[ch.sfb_cnt];
        ats.chaos_measure_en_fac[ci] = ENFAC_FLOOR_Q31.max(fdiv_norm(lines_sum, total_lines));
        let use_patch = ats.chaos_measure_en_fac[ci] > PATCH_ARM_Q31;
        let exe_patch = if use_patch && ats.last_enfac_patch[ci] != 0 {
            1
        } else {
            0
        };
        let mut grp = 0;
        while grp < ch.sfb_cnt {
            for sfb in 0..ch.max_sfb_per_group {
                let s = grp + sfb;
                let do_patch = if ci == 1 && ms_mask[s] != 0 {
                    exe_patch_m
                } else {
                    exe_patch
                };
                if do_patch != 0 && ch.energy_linear[s] > 0 {
                    let e = if ats.chaos_measure_en_fac[ci] > PATCH_HI_Q31 {
                        (fac14
                            .wrapping_add(ch.energy_ld[s])
                            .wrapping_add(ch.energy_ld[s] >> 1))
                            >> 1
                    } else if ats.chaos_measure_en_fac[ci] > PATCH_MID_Q31 {
                        (fac12.wrapping_add(ch.energy_ld[s])) >> 1
                    } else {
                        (fac34.wrapping_add(ch.energy_ld[s] >> 1)) >> 1
                    };
                    enfac[ci][s] = e.min(0);
                }
            }
            grp += ch.sfb_per_group;
        }
        ats.last_enfac_patch[ci] = if use_patch { 1 } else { 0 };
        exe_patch_m = exe_patch;
    }
    enfac
}

/// One channel's input for [`pe_calculation`]: post-decision vectors in
/// the exact Q31 representation the C code consumes (spectrum enters only
/// via the precomputed `form_factor_ld`, computed upstream with
/// `form_factor`).
pub struct PeChannelInput<'a> {
    pub energy_ld: &'a [i32],
    pub threshold_ld: &'a [i32],
    pub energy_linear: &'a [i32],
    pub form_factor_ld: &'a [i32],
    pub offsets: &'a [i32],
    pub sfb_cnt: usize,
    pub sfb_per_group: usize,
    pub max_sfb_per_group: usize,
    pub is_book: &'a [i32],
    pub is_scale: &'a [i32],
    pub short_block: bool,
}

/// [`pe_calculation`] outputs: PE state plus the weighted vectors that
/// feed S2/S4 (`sfbWeightedEnergyLdData`, post-weighting thresholds,
/// `sfbEnFacLd`).
pub struct PeResult {
    pub data: PeData,
    pub weighted_energy_ld: Vec<Vec<i32>>,
    pub threshold_ld: Vec<Vec<i32>>,
    pub enfac_ld: Vec<Vec<i32>>,
}

/// `FDKaacEnc_peCalculation` (`adj_thr.cpp:908-944`): prepare, weight,
/// apply (`weighted = energy - enFac`, `threshold -= enFac` over the
/// active bands) and sum PE per channel and element.
pub fn pe_calculation(
    channels: &[PeChannelInput<'_>],
    ms_mask: &[i32],
    ats: &mut AtsElement,
) -> PeResult {
    let nch = channels.len();
    debug_assert!(nch == 1 || nch == 2);
    // prepareSfbPe per channel.
    let mut n_lines: Vec<Vec<i32>> = Vec::with_capacity(nch);
    for ch in channels {
        n_lines.push(prepare_sfb_pe(
            ch.energy_ld,
            ch.threshold_ld,
            ch.form_factor_ld,
            ch.offsets,
            ch.sfb_cnt,
            ch.sfb_per_group,
            ch.max_sfb_per_group,
        ));
    }
    // Weighting factors (evolves ats).
    let w_inputs: Vec<WeightingInput<'_>> = channels
        .iter()
        .zip(n_lines.iter())
        .map(|(ch, nl)| WeightingInput {
            n_lines: nl,
            energy_linear: ch.energy_linear,
            energy_ld: ch.energy_ld,
            offsets: ch.offsets,
            sfb_cnt: ch.sfb_cnt,
            sfb_per_group: ch.sfb_per_group,
            max_sfb_per_group: ch.max_sfb_per_group,
            short_block: ch.short_block,
        })
        .collect();
    let enfac = calc_weighting(&w_inputs, ms_mask, ats);
    // Apply weighting over the active bands, mirroring the two C buffer
    // roles exactly (`adj_thr.cpp:925-939`):
    // - `sfbWeightedEnergyLdData` is write-only scratch (memset in the
    //   oracle, stale in the encoder, unread outside the active bands by
    //   every downstream stage whose loops are all bounded by
    //   maxSfbPerGroup: S2 AdaptThresholdsVBR, S4 scf search,
    //   crashRecovery via sectionData). Zero-init + active-only writes.
    // - `sfbThresholdLdData` is mutated in place: full input clone with
    //   active-only subtracts.
    let mut weighted: Vec<Vec<i32>> = Vec::with_capacity(nch);
    let mut thresholds: Vec<Vec<i32>> = Vec::with_capacity(nch);
    for (ci, ch) in channels.iter().enumerate() {
        let mut w = vec![0i32; ch.sfb_cnt];
        let mut t = ch.threshold_ld.to_vec();
        let mut grp = 0;
        while grp < ch.sfb_cnt {
            for sfb in 0..ch.max_sfb_per_group {
                let s = grp + sfb;
                w[s] = ch.energy_ld[s].wrapping_sub(enfac[ci][s]);
                t[s] = ch.threshold_ld[s].wrapping_sub(enfac[ci][s]);
            }
            grp += ch.sfb_per_group;
        }
        weighted.push(w);
        thresholds.push(t);
    }
    // Sum PE.
    let mut per_channel = Vec::with_capacity(nch);
    let mut pe_sum: i32 = ats.pe_offset;
    let mut const_sum: i32 = 0;
    let mut nact_sum: i32 = 0;
    for (ci, ch) in channels.iter().enumerate() {
        let pc = calc_sfb_pe(
            &n_lines[ci],
            &weighted[ci],
            &thresholds[ci],
            ch.sfb_cnt,
            ch.sfb_per_group,
            ch.max_sfb_per_group,
            ch.is_book,
            ch.is_scale,
        );
        pe_sum = pe_sum.wrapping_add(pc.pe);
        const_sum = const_sum.wrapping_add(pc.const_part);
        nact_sum = nact_sum.wrapping_add(pc.n_active_lines);
        per_channel.push(pc);
    }
    PeResult {
        data: PeData {
            channels: per_channel,
            pe: pe_sum,
            const_part: const_sum,
            n_active_lines: nact_sum,
            offset: ats.pe_offset,
        },
        weighted_energy_ld: weighted,
        threshold_ld: thresholds,
        enfac_ld: enfac,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::bitcount::NO_NOISE_PNS;
    use crate::aac_enc::testdata;

    /// Committed S1 corpus: (testdata short name, frame). Covers silence,
    /// tones, transients, noise, sweeps, mono/stereo, long/short/start/
    /// stop, 48/44.1 kHz, MS NONE/SOME/ALL, TNS active/inactive, and the
    /// EnFac patch armed/unarmed.
    const S1_CASES: [(&str, u32); 19] = [
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

    fn ats_from_committed(frame: &testdata::S1Frame) -> AtsElement {
        let mut ats = AtsElement::new(0);
        ats.chaos_measure_en_fac = [frame.ats_pre[0], frame.ats_pre[1]];
        ats.last_enfac_patch = [frame.ats_pre[2], frame.ats_pre[3]];
        ats
    }

    fn pe_inputs<'a>(frame: &'a testdata::S1Frame) -> (Vec<PeChannelInput<'a>>, Vec<i32>) {
        let mut inputs = Vec::new();
        for ch in &frame.ch {
            inputs.push(PeChannelInput {
                energy_ld: &ch.penld,
                threshold_ld: &ch.pthld,
                energy_linear: &ch.senl,
                form_factor_ld: &ch.ff,
                offsets: &ch.offsets,
                sfb_cnt: ch.geo_cnt,
                sfb_per_group: ch.geo_per,
                max_sfb_per_group: ch.geo_max,
                is_book: &ch.isb,
                is_scale: &ch.iss,
                short_block: ch.geo_seq == 2,
            });
        }
        let ms_mask: Vec<i32> = match &frame.msd {
            Some((_, mask)) => mask.clone(),
            None => Vec::new(),
        };
        (inputs, ms_mask)
    }

    #[test]
    fn s1_form_factor_matches_oracle() {
        for (name, f) in S1_CASES {
            let frame = testdata::load_s1(name, f);
            for (c, ch) in frame.ch.iter().enumerate() {
                let got = form_factor(&ch.spec, &ch.offsets, ch.geo_cnt, ch.geo_per, ch.geo_max);
                assert_eq!(got, ch.ff, "{name} f{f} ch{c} form factor");
            }
        }
    }

    #[test]
    fn s1_pe_matches_oracle_with_injected_state() {
        for (name, f) in S1_CASES {
            let frame = testdata::load_s1(name, f);
            let (inputs, ms_mask) = pe_inputs(&frame);
            let mut ats = ats_from_committed(&frame);
            let result = pe_calculation(&inputs, &ms_mask, &mut ats);
            // Weighted vectors and EnFac.
            for (c, ch) in frame.ch.iter().enumerate() {
                assert_eq!(result.enfac_ld[c], ch.enfac, "{name} f{f} ch{c} enfac");
                assert_eq!(
                    result.weighted_energy_ld[c], ch.wenld,
                    "{name} f{f} ch{c} weighted energies"
                );
                assert_eq!(
                    result.threshold_ld[c], ch.wthld,
                    "{name} f{f} ch{c} weighted thresholds"
                );
                // Per-band PE details.
                let pc = &result.data.channels[c];
                assert_eq!(pc.sfb_n_lines, ch.spnl, "{name} f{f} ch{c} nlines");
                assert_eq!(pc.sfb_pe, ch.spech, "{name} f{f} ch{c} sfbPe");
                assert_eq!(pc.sfb_const_part, ch.spco, "{name} f{f} ch{c} constPart");
                assert_eq!(pc.sfb_n_active_lines, ch.spna, "{name} f{f} ch{c} nActive");
                assert_eq!(
                    [pc.pe, pc.const_part, pc.n_active_lines],
                    ch.pech,
                    "{name} f{f} ch{c} channel sums"
                );
            }
            // Element sums.
            assert_eq!(
                [
                    result.data.pe,
                    result.data.const_part,
                    result.data.n_active_lines,
                    result.data.offset
                ],
                frame.pe,
                "{name} f{f} element PE"
            );
            // ATS evolution matches the committed post-state.
            assert_eq!(
                [
                    ats.chaos_measure_en_fac[0],
                    ats.chaos_measure_en_fac[1],
                    ats.last_enfac_patch[0],
                    ats.last_enfac_patch[1]
                ],
                frame.ats_post,
                "{name} f{f} ATS evolution"
            );
        }
    }

    #[test]
    fn s1_geo_and_inputs_are_self_consistent() {
        // Loader-level invariants: committed geometry matches vector
        // lengths, offsets span the spectrum, and the MS mask covers the
        // channel bands. Guards extractor/loader drift (the S1 analogue of
        // the M3 synthesis anchor).
        for (name, f) in S1_CASES {
            let frame = testdata::load_s1(name, f);
            for (c, ch) in frame.ch.iter().enumerate() {
                assert_eq!(ch.offsets.len(), ch.geo_cnt + 1);
                assert!(ch.geo_max <= ch.geo_per);
                assert!(*ch.offsets.last().expect("offsets") <= 1024);
                assert_eq!(ch.isb.len(), ch.geo_cnt);
                assert_eq!(ch.iss.len(), ch.geo_cnt);
                // PNS-off parity: the encoder is configured with PNS
                // disabled, so every committed S1 NOISE entry must equal
                // the `NO_NOISE_PNS` sentinel the S5/bit-count path
                // consumes (`bitcount.rs`, `stream_driver.rs`). Length is
                // `sfbActive` in the oracle dump (not `geo_cnt`), so only
                // the sentinel value is pinned here, non-vacuously.
                assert!(!ch.noise.is_empty(), "{name} f{f} ch{c} noise non-empty");
                for &n in &ch.noise {
                    assert_eq!(n, NO_NOISE_PNS, "{name} f{f} ch{c} PNS-off");
                }
                // TNS windows match the block type.
                let nwin = if ch.geo_seq == 2 { 8 } else { 1 };
                assert_eq!(ch.tns.len(), nwin, "{name} f{f} ch{c} tns windows");
                assert_eq!(ch.tns_coef_res.len(), nwin);
            }
            if frame.ch.len() == 2 {
                let (digest, mask) = frame.msd.as_ref().expect("MSD");
                assert!((0..=2).contains(digest));
                assert_eq!(mask.len(), frame.ch[0].geo_cnt);
            } else {
                assert!(frame.msd.is_none());
            }
        }
    }

    #[test]
    fn s1_calc_sfb_pe_covers_both_ld_ratio_branches() {
        // Branch-coverage proof grounded in reference data: find bands on
        // each side of C1 = log(8)/log(2) = 3.0 (Ld 0x06000000) and assert
        // the committed per-band PE values match on both sides.
        let mut above = 0;
        let mut below = 0;
        for (name, f) in S1_CASES {
            let frame = testdata::load_s1(name, f);
            for ch in &frame.ch {
                for s in 0..ch.geo_cnt {
                    if ch.wenld[s] <= ch.wthld[s] {
                        continue;
                    }
                    let ratio = ch.wenld[s].wrapping_sub(ch.wthld[s]);
                    if ratio >= 0x06000000 {
                        above += 1;
                    } else {
                        below += 1;
                    }
                }
            }
        }
        assert!(above > 0, "no C1-branch band in corpus");
        assert!(below > 0, "no C2/C3-branch band in corpus");
    }

    #[test]
    fn s1_pe_constants_match_reference_literals() {
        // `line_pe.cpp:109-113` use a DOUBLE `0.5593573` for C3 (no `f`
        // suffix), unlike `sf_estim.cpp`'s float `PE_C3`. Using the float
        // value (0x47990500) shifts fMult products by up to 31 LSB and
        // breaks bit-exactness (found via sine f1 ch0 band 50: SPCO off
        // by one with the float constant). C1/C2 are double divisions by
        // `64.0f` and are exact either way; values C-probed.
        assert_eq!(C1_LD_Q31, 0x06000000);
        assert_eq!(C2_LD_Q31, 0x02A4D3C3);
        assert_eq!(C3_Q31, 0x4799051F);
    }

    #[test]
    fn s1_scf_dpcm_cost_table() {
        assert_eq!(HUFF_LTABSCF.len(), 121);
        assert_eq!(bit_count_scf_delta(0), 0x01);
        assert_eq!(bit_count_scf_delta(-60), 0x12);
        assert_eq!(bit_count_scf_delta(60), 0x13);
        // The table is NOT symmetric around 0 (verified against the ROM:
        // index 59 is 0x03, index 61 is 0x04).
        assert_eq!(bit_count_scf_delta(1), 0x04);
        assert_eq!(bit_count_scf_delta(-1), 0x03);
    }

    #[test]
    fn s1_form_factor_zero_band_and_tail_sentinel() {
        // All-zero spectrum: every band reads CalcLdData(0) = -1.0, and
        // bands past maxSfbPerGroup read the explicit sentinel.
        let spec = vec![0i32; 1024];
        let offsets = vec![0, 4, 8];
        let got = form_factor(&spec, &offsets, 2, 2, 1);
        assert_eq!(got, vec![i32::MIN, i32::MIN]);
    }

    #[test]
    fn s1_prepare_clamps_estimate_to_band_width() {
        // A form factor at the 31/64 saturation bound drives
        // CalcInvLdData to MAXVAL; the estimate must clamp to the band
        // width (fMin branch). (0x7FFFFFFF would wrap on adding the scale
        // compensation, so the bound itself is the right probe.)
        let energy = vec![0x10000000];
        let threshold = vec![0x00000000];
        let form = vec![0x3E000000];
        let n = prepare_sfb_pe(&energy, &threshold, &form, &[0, 4], 1, 1, 1);
        assert_eq!(n, vec![4]);
        // At/below threshold reads exactly 0.
        let n0 = prepare_sfb_pe(&threshold, &energy, &form, &[0, 4], 1, 1, 1);
        assert_eq!(n0, vec![0]);
    }

    #[test]
    fn s1_intensity_branch_contributes_nothing() {
        // The isBook branch is a dead store in C (line_pe.cpp:209-220):
        // nonzero isBook/isScale with energy at/below threshold must still
        // yield all-zero PE, and varying isScale must not leak anywhere.
        let energy = vec![0x10000000, 0x10000000];
        let threshold = vec![0x20000000, 0x20000000];
        for is_scale in [vec![0, 0], vec![50, -50], vec![i32::MAX, i32::MIN]] {
            let pc = calc_sfb_pe(&[3, 5], &energy, &threshold, 2, 2, 2, &[1, 1], &is_scale);
            assert_eq!(pc.sfb_pe, vec![0, 0]);
            assert_eq!(pc.sfb_const_part, vec![0, 0]);
            assert_eq!(pc.sfb_n_active_lines, vec![0, 0]);
            assert_eq!([pc.pe, pc.const_part, pc.n_active_lines], [0, 0, 0]);
        }
    }

    #[test]
    fn s1_weighting_short_element_arms_patch_state() {
        // Short elements skip the flatness computation: enFac stays zero
        // while chaosMeasureEnFac reads 0.75 and lastEnFacPatch goes TRUE,
        // arming the *next* long frame (adj_thr.cpp:871-876).
        let ch = WeightingInput {
            n_lines: &[10, 20],
            energy_linear: &[1000, 2000],
            energy_ld: &[0x10000000, 0x10000000],
            offsets: &[0, 4, 8],
            sfb_cnt: 2,
            sfb_per_group: 2,
            max_sfb_per_group: 2,
            short_block: true,
        };
        let mut ats = AtsElement::new(0);
        let enfac = calc_weighting(&[ch], &[], &mut ats);
        assert_eq!(enfac, vec![vec![0, 0]]);
        assert_eq!(ats.chaos_measure_en_fac, [0x60000000, 0]);
        assert_eq!(ats.last_enfac_patch, [1, 0]);
    }

    #[test]
    fn s1_weighting_silence_stays_unpatched() {
        // All-zero long input: chaosMeasure hits the 0.1875 floor, the
        // patch gate stays shut, and lastEnFacPatch records FALSE.
        let ch = WeightingInput {
            n_lines: &[0, 0],
            energy_linear: &[0, 0],
            energy_ld: &[i32::MIN, i32::MIN],
            offsets: &[0, 4, 8],
            sfb_cnt: 2,
            sfb_per_group: 2,
            max_sfb_per_group: 2,
            short_block: false,
        };
        let mut ats = AtsElement::new(0);
        let enfac = calc_weighting(&[ch], &[], &mut ats);
        assert_eq!(enfac, vec![vec![0, 0]]);
        assert_eq!(ats.chaos_measure_en_fac, [0x18000000, 0]);
        assert_eq!(ats.last_enfac_patch, [0, 0]);
    }

    #[test]
    fn s1_weighting_patch_gate_needs_prior_frame() {
        // Same loud input with the patch armed (lastEnFacPatch = 1) writes
        // non-positive enFac; disarmed it writes zeros. This is the exact
        // two-frame arming of adj_thr.cpp:826-828.
        static NL: [i32; 2] = [900, 900];
        static EL: [i32; 2] = [0x20000000, 0x20000000];
        static ED: [i32; 2] = [0x10000000, 0x10000000];
        static OFF: [i32; 3] = [0, 1024, 2048];
        let mk = || WeightingInput {
            n_lines: &NL,
            energy_linear: &EL,
            energy_ld: &ED,
            offsets: &OFF,
            sfb_cnt: 2,
            sfb_per_group: 2,
            max_sfb_per_group: 2,
            short_block: false,
        };
        let mut armed = AtsElement::new(0);
        armed.last_enfac_patch = [1, 0];
        let got_armed = calc_weighting(&[mk()], &[], &mut armed);
        assert!(
            got_armed[0].iter().any(|&e| e != 0),
            "armed patch must write enFac"
        );
        assert!(got_armed[0].iter().all(|&e| e <= 0), "enFac capped at 0");
        let mut disarmed = AtsElement::new(0);
        let got_disarmed = calc_weighting(&[mk()], &[], &mut disarmed);
        assert_eq!(got_disarmed, vec![vec![0, 0]]);
        // Tamper check: the two outcomes must differ (non-vacuous gate).
        assert_ne!(got_armed, got_disarmed);
    }

    #[test]
    fn s1_ms_steering_drives_ch1_patch() {
        // Channel 1 follows channel 0's patch decision on MS-coupled bands
        // (adj_thr.cpp:834-840): with msMask set, ch1 patches iff ch0 did,
        // even when ch1's own flatness would not arm the patch.
        fn channels<'a>() -> [WeightingInput<'a>; 2] {
            // Static storage keeps the borrow simple; values mirror the
            // armed-patch unit test above (ch0) and a flat ch1.
            static NL0: [i32; 2] = [900, 900];
            static EL0: [i32; 2] = [0x20000000, 0x20000000];
            static ED0: [i32; 2] = [0x10000000, 0x10000000];
            static OFF: [i32; 3] = [0, 1024, 2048];
            static NL1: [i32; 2] = [1, 1];
            static EL1: [i32; 2] = [0x1000, 0x1000];
            static ED1: [i32; 2] = [0x08000000, 0x08000000];
            [
                WeightingInput {
                    n_lines: &NL0,
                    energy_linear: &EL0,
                    energy_ld: &ED0,
                    offsets: &OFF,
                    sfb_cnt: 2,
                    sfb_per_group: 2,
                    max_sfb_per_group: 2,
                    short_block: false,
                },
                WeightingInput {
                    n_lines: &NL1,
                    energy_linear: &EL1,
                    energy_ld: &ED1,
                    offsets: &OFF,
                    sfb_cnt: 2,
                    sfb_per_group: 2,
                    max_sfb_per_group: 2,
                    short_block: false,
                },
            ]
        }
        let run = |mask: &[i32]| {
            let mut ats = AtsElement::new(0);
            ats.last_enfac_patch = [1, 1];
            calc_weighting(&channels(), mask, &mut ats)
        };
        let masked = run(&[1, 1]);
        let unmasked = run(&[0, 0]);
        // ch0 (loud, armed) patches in both runs.
        assert!(masked[0].iter().any(|&e| e != 0));
        assert!(unmasked[0].iter().any(|&e| e != 0));
        // ch1 (flat, would not arm itself) patches only via MS steering.
        assert!(masked[1].iter().any(|&e| e != 0));
        assert_eq!(unmasked[1], vec![0, 0]);
    }

    #[test]
    fn s1_pe_offset_is_added_verbatim() {
        // ATS peOffset seeds the element PE (adj_thr.cpp:890); VBR-5 runs
        // with 0, but the plumbing must carry any value exactly.
        let ch = PeChannelInput {
            energy_ld: &[i32::MIN],
            threshold_ld: &[0],
            energy_linear: &[0],
            form_factor_ld: &[i32::MIN],
            offsets: &[0, 4],
            sfb_cnt: 1,
            sfb_per_group: 1,
            max_sfb_per_group: 1,
            is_book: &[0],
            is_scale: &[0],
            short_block: false,
        };
        let mut ats = AtsElement::new(7);
        let result = pe_calculation(&[ch], &[], &mut ats);
        assert_eq!(result.data.pe, 7);
        assert_eq!(result.data.offset, 7);
    }

    #[test]
    fn s1_expected_values_are_checked_non_vacuously() {
        // Mutating one committed value must break its comparison: proves
        // the corpus asserts above can actually fail.
        let frame = testdata::load_s1("sine-stereo", 5);
        let (inputs, ms_mask) = pe_inputs(&frame);
        let mut ats = ats_from_committed(&frame);
        let result = pe_calculation(&inputs, &ms_mask, &mut ats);
        assert_eq!(result.data.pe, frame.pe[0]);
        let mut tampered = frame.pe;
        tampered[0] = tampered[0].wrapping_add(1);
        assert_ne!(result.data.pe, tampered[0]);
        let mut tampered_ff = frame.ch[0].ff.clone();
        tampered_ff[10] = tampered_ff[10].wrapping_add(1);
        let ff = form_factor(
            &frame.ch[0].spec,
            &frame.ch[0].offsets,
            frame.ch[0].geo_cnt,
            frame.ch[0].geo_per,
            frame.ch[0].geo_max,
        );
        assert_eq!(ff, frame.ch[0].ff);
        assert_ne!(ff, tampered_ff);
    }

    #[test]
    fn vbr_pe_offset_matches_c_reference() {
        // FDK-AAC v3.0.0 `FDKaacEnc_AdjThrInit` branch (`adj_thr.cpp:2553-2556`),
        // verified via the real `FDKaacEnc_QCInit` for every VBR-1..5
        // operating point at 44100/48000 Hz, mono/stereo: the only
        // Ferrite operating point below 32 kbit/s per channel is VBR-1
        // stereo (20 kbit/s -> 50); everything else is 0.
        assert_eq!(vbr_pe_offset(20000), 50);
        for pch in [32000, 40000, 48000, 56000, 64000, 72000, 96000, 112000] {
            assert_eq!(vbr_pe_offset(pch), 0, "pch={pch}");
        }
        // Boundary: the C branch is `chBitrate < 32000`.
        assert_eq!(vbr_pe_offset(31999), 50);
        assert_eq!(vbr_pe_offset(32000), 0);
        // The shared Q31 step is the exact C op (`fMultI` round-to-nearest);
        // 0x666667 * 20000 = 62.5 -> 63, so max(50, 100-63) = 50.
        assert_eq!(crate::aac_enc::q31::fmult_i(0x666667, 20000), 63);
    }
}
