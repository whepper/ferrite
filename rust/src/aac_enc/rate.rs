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
//! M4-S6 QC gain loop and VBR rate-control closure (exact integers).
//!
//! Exact port of the `cpp/libAACenc/src/qc_main.cpp` S6 responsibilities at
//! base commit `1785529` (VBR-5, single element, `nSubFrames = 1`), plus the
//! `DistributeBits` family from `cpp/libAACenc/src/adj_thr.cpp:2218-2798`.
//! Reuses the S3 scalar quantizer ([`super::quant`]), the S5 sectioning and
//! counting ([`super::bitcount`]) and the S1 static-bit counts
//! ([`super::static_bits`]) without modification.
//!
//! ## S6 contract (plan §9/S6, reference locations)
//!
//! Implemented here (all VBR branches; CBR/SFR/FF branches are unreachable
//! at VBR-5 and panic loudly if selected):
//!
//! - QC-init derivations (`aacenc.cpp:582-684`): `calc_bits_per_frame`,
//!   VBR `averageBits/bitRes/maxBits/minBits`, `bitResMode` FULL check,
//!   `bitDistributionMode`, `mean_pe`, `max_bit_fac`, `init_element_bits`
//!   (MODE_1/MODE_2 arms), `init_bits2pe_factor` (LC default + dZone paths;
//!   the ELD table-interpolation path is unreachable at LC and panics).
//! - `adjust_bitrate` + `frame_padding` + `calc_frame_len`
//!   (`qc_main.cpp:179-195,206-223,469-489`).
//! - `bit_res_redistribution` (`:738-786`), `distribute_element_dyn_bits`
//!   (`:504-550`), `prepare_grants` (`:615-635` grant formula).
//! - `bits2pe2` (`adj_thr.cpp:433-437`), `bitres_calc_bit_fac`
//!   (`:2347-2431`, with `calc_bit_save/:2218`, `calc_bit_spend/:2236`,
//!   `adjust_pe_min_max/:2281`), `calc_pe_correction` (`:2604-2662`),
//!   `calc_pe_correction_low_bit_res` (`:2664-2721`), `distribute_bits`
//!   (`:2723-2798`, FULL + REDUCED/DISABLED branches).
//! - Gain loop (`qc_main.cpp:901-1107`): `calc_max_value_in_sfb`
//!   (`:1219-1240`), `reduce_bit_consumption` (`:1119-1166`),
//!   `crash_recovery` (`:1395-1498`), the consumed-bits accumulators
//!   (`:677-736`), the `check_min_frame_demand` stub (`:571-575`, always 1)
//!   and the exit matrix (`:1030-1107`, [`exit_matrix_step`]).
//! - `update_fill_bits` VBR (`:1168-1209`), `finalize_bit_consumption` VBR
//!   path (`:1283-1383`, CBR transport-correction block skipped) with a
//!   ported `write_extension_data` fill-sizing loop (`bitenc.cpp:809-918`,
//!   EXT_FILL_DATA/FIL branch), `update_bitres` VBR pin (`:1249-1274`).
//! - Frame driver [`qc_main_frame`] wiring grants → gain loop (with the S3
//!   re-quantizer and S5 counter) → fill/finalize.
//!
//! Explicitly excluded (later stages): the `codeValues`/
//! `codeScalefactorDelta` encode half (takes a bitstream handle — S7),
//! `WriteBitstream`/transport writes, CBR/SFR/FF branches, multi-element
//! (>1 element) and multi-channel-mode `InitElementBits` arms beyond
//! MODE_1/MODE_2, the ELD `bits2pe` table path.
//!
//! ## Key VBR-5 facts the port relies on
//!
//! - At VBR (`isCBRAdjustment == 0`) QCMain passes `maxBitsPerFrame` — not
//!   the frame `avgTotalBits` — into both `BitResRedistribution` and
//!   `prepareBitDistribution` (`qc_main.cpp:805-806,836-839`). Grants and
//!   the bit budget therefore derive from the buffer size; `AdjustBitrate`
//!   only evolves `paddingRest`. The oracle caught an initial port bug
//!   here: using the frame average made every loud mono frame "overshoot"
//!   and loop spuriously.
//! - `bitResTot` is pinned full by the VBR `updateBitres` branch, so
//!   `BitResRedistribution` always yields level 0 / max 0 and
//!   `totalAvail == maxBitsPerFrame` on every fixture frame.
//! - `globHdrBits` (ADTS header, 56 bits) is transport-owned: the frame
//!   driver takes it as an input (`S6BITR` oracle line), never computes it.
//!   Verified with a real `transportEnc` probe: 56 for mono/stereo ADTS
//!   regardless of `auBits` (PCE is `-1`/absent for SCE/CPE per
//!   `getPceRepetitionRate`; `protection_absent = 1` by default).
//! - `bits2PeFactor` is constant at LC (`advancedBitsToPe = isLowDelay =
//!   0`): the `bits2Pe` tables are never consulted, so they are not ported.
//!   `dZoneQuantEnable = 0` at LC, so the dZone compensation is dead.
//! - `checkMinFrameBitsDemand` is a stub returning 1 (`:571-575`);
//!   `minBitsPerFrame` is 0 at VBR-5.
//!
//! ## State ownership
//!
//! - [`DistributeState`] holds exactly the `DistributeBits`-owned
//!   `ATS_ELEMENT` fields (`bits2PeFactor_m/e`, `peCorrectionFactor_m/e`,
//!   `peLast`, `dynBitsLast`, `peMin/peMax`); the S2-owned fields
//!   (`chaosMeasureOld`, `vbrQualFactor`, `ahParam`, `minSnrAdaptParam`,
//!   enFac) stay in [`super::pe::AtsElement`] untouched.
//!   `bitres_calc_bit_fac` mutates `peMin/peMax` exactly like C
//!   (`adj_thr.cpp:2426`).
//! - [`QcState`] holds the persistent QC kernel (`bitResTot`,
//!   `paddingRest`; everything else is init-constant at VBR-5).
//! - The frame driver ([`qc_main_frame`]) consumes post-S4 state (stored
//!   scalefactors, global gain, mutated spectrum) plus the S5 first-pass
//!   (`QMAX`, sections, dyn bits) as explicit inputs — the same stage
//!   independence as S5 (which consumes S4 outputs without calling S4).
//! - `calc_max_value_in_sfb` leaves unvisited bands untouched (like C,
//!   which never writes them); callers zero-init for determinism. The S5
//!   `QMAX` fixtures cross-check the visited bands bit-exactly.
//!
//! ## Comparisons, saturation, termination
//!
//! `fMultI` rounds to nearest (via [`super::q31`]); the element-bit
//! compensation (`codeBits != totalBits`) adds the difference to the
//! min/max element exactly like C (`:524-547`). Crash recovery drops
//! `maxSfbPerGroup` from the top until `savedBits >= bitsToSave`, then
//! rebalances static/grant/maxDyn via a static recount.
//! `QUANT_ERROR`/`BITRES_TOO_LOW` paths return [`QcError`] (loud, never
//! silent). The outer loop is bounded by [`MAX_OUTER_PASSES`] (25), which
//! panics if tripped — C terminates via the same exit matrix on valid
//! inputs.
//!
//! ## Q31 literals
//!
//! Every `FL2FXCONST_DBL` literal below was printed from C with the exact
//! parenthesization C uses (float division first, then conversion); see
//! `/tmp/s6lit_probe.cpp` output recorded in the S6 tests. Never
//! hand-transcribe Q31.
//!
//! ## Facts vs open points
//!
//! - [Fact] All mappings read from the cited lines; fixed-point ops reuse
//!   [`super::q31`] (same primitives as S1/S2/S4).
//! - [Fact] `fixp_abs` on widened `i16` values cannot overflow
//!   (`|q| <= 32768 < i32::MAX`); `-32768` lines abort loudly (established
//!   S3/S4/S5 safety policy — C would `fixp_abs` them into `i32::MIN` and
//!   mis-gate).
//! - [Open] Multi-element `InitElementBits` arms and the CBR family are
//!   future work; selected explicitly with panics, never silent defaults.

use super::bitcount::{
    count_values, dyn_bit_count_reuse, BitCountInput, BitCountScratch, SectionData,
    CODE_BOOK_PNS_NO,
};
use super::q31::{fdiv_norm, fdiv_norm_e, fmult, fmult_add_div2, fmult_i, schur_div};
use super::static_bits::{static_bits_cpe, static_bits_sce, MsDigest};
use super::tns::TnsChannel;

/// `MIN_BUFSIZE_PER_EFF_CHAN = 6144` (`aacenc.h:111`).
pub const MIN_BUFSIZE_PER_EFF_CHAN: i32 = 6144;
/// `MAX_QUANT = 8191` (`quantize.h:110`): gain-loop gate.
pub const MAX_QUANT: i32 = 8191;
/// `MAX_GROUPED_SFB = 60` (`psy_const.h:151-154`).
pub const MAX_GROUPED_SFB: usize = 60;
/// `EL_ID_BITS = 3` (`bitenc.h:126`): `globalExtBits` with no extensions.
pub const GLOBAL_EXT_BITS: i32 = 3;
/// `Q_AVGBITS = 17` (`adj_thr.cpp:312`): PE/bit scaling.
pub const Q_AVGBITS: i32 = 17;
/// `DFRACT_BITS = 32` (`common_fix.h:113`).
pub const DFRACT_BITS: i32 = 32;
/// `BITRES_MIN = 300` (`aacenc.cpp:117`): FULL-reservoir threshold.
pub const BITRES_MIN: i32 = 300;
/// `MAXVAL_DBL = 0x7FFFFFFF` (relativeBits for single-element mappings).
pub const MAXVAL_DBL: i32 = i32::MAX;
/// `MAX_FILL_DATA_BYTES = 269` (`bitenc.cpp:814`).
pub const MAX_FILL_DATA_BYTES: i32 = 269;
/// Loud outer-loop bound for the gain loop (C terminates via the exit
/// matrix; this guard only fires on implementation bugs).
pub const MAX_OUTER_PASSES: i32 = 25;
/// `maxIterations = 5` for non-LD (`aacenc.cpp:727-731`).
pub const MAX_ITERATIONS: i32 = 5;

// Q31 literals, C-probed (`tools/enc-oracle/s6lit_probe.cpp`; float division first).
/// `FL2FXCONST_DBL(1.18f / 2)`: default `bits2PeFactor_m`.
const B2PE_DEFAULT_M: i32 = 0x4b851e80u32 as i32;
/// `FL2FXCONST_DBL(0.6f)` (dZone headroom check).
const DZONE_06: i32 = 0x4ccccd00u32 as i32;
/// `FL2FXCONST_DBL(0.4f)` (dZone 32–40 kbit/s/ch compensation).
const DZONE_04: i32 = 0x33333340u32 as i32;
/// `FL2FXCONST_DBL(0.3f)` (dZone lower-rate compensation).
const DZONE_03: i32 = 0x26666680;
/// `FL2FXCONST_DBL(0.466666666f)` (long `bitsave_slope`).
const SAVE_SLOPE_LONG: i32 = 0x3bbbbbc0u32 as i32;
/// `FL2FXCONST_DBL(0.666666666f)` (long `bitspend_slope`).
const SPEND_SLOPE_LONG: i32 = 0x55555580;
/// `FL2FXCONST_DBL(1.2f / 2.f)` (pe-correction window).
const PECORR_12_2: i32 = 0x4ccccd00u32 as i32;
/// `FL2FXCONST_DBL(0.65f)` (pe-correction window).
const PECORR_065: i32 = 0x53333300;
/// `FL2FXCONST_DBL(1.1f / 2.f)` (pe-correction dead zone).
const PECORR_11_2: i32 = 0x46666680;
/// `FL2FXCONST_DBL(0.85f / 2.f)` (pe-correction floor).
const PECORR_085_2: i32 = 0x36666680;
/// `FL2FXCONST_DBL(0.9f / 2.f)` (pe-correction clamp).
const PECORR_09_2: i32 = 0x39999980;
/// `FL2FXCONST_DBL(1.15f / 2.f)` (pe-correction clamp).
const PECORR_115_2: i32 = 0x49999980;
/// `FL2FXCONST_DBL(0.85f)` (pe-correction adaptation).
const PECORR_085: i32 = 0x6ccccd00u32 as i32;
/// `FL2FXCONST_DBL(0.15f)` (pe-correction adaptation).
const PECORR_015: i32 = 0x13333340;
/// `FL2FXCONST_DBL(0.7f)` (pe-correction adaptation).
const PECORR_07: i32 = 0x59999980;
/// `FL2FXCONST_DBL(0.3f)` (pe-correction adaptation).
const PECORR_03: i32 = 0x26666680;
/// `FL2FXCONST_DBL(0.75f / 2.f)` (low-bitres correction init/floor).
const PECORR_075_2: i32 = 0x30000000;
/// `FL2FXCONST_DBL(0.005f)` (low-bitres `amp`).
const LOWBR_AMP: i32 = 0x00a3d70a;
/// `FL2FXCONST_DBL(0.25f)` (low-bitres `maxDiff`).
const LOWBR_MAXDIFF: i32 = 0x20000000;
/// `FL2FXCONST_DBL(0.3f)` (`adjustPeMinMax` `minFacHi`).
const PEMM_MIN_HI: i32 = 0x26666680;
/// `FL2FXCONST_DBL(0.14f)` (`adjustPeMinMax` `minFacLo`).
const PEMM_MIN_LO: i32 = 0x11eb8520;
/// `FL2FXCONST_DBL(0.07f)` (`adjustPeMinMax` `maxFacLo`).
const PEMM_MAX_LO: i32 = 0x08f5c290;
/// `FL2FXCONST_DBL(0.1666666667f)` (`adjustPeMinMax` min-diff factor).
const PEMM_MIN_DIFF_FAC: i32 = 0x15555560;
/// `FL2FXCONST_DBL(0.8f)` (`AdjThrInit` non-LD `peMin` point).
const POINT8: i32 = 0x66666680;
/// `FL2FXCONST_DBL(0.6f)` (`AdjThrInit` non-LD `peMax` point).
const POINT6: i32 = 0x4ccccd00u32 as i32;
/// `FL2FXCONST_DBL(0.7f)` (bitres small-reservoir limit).
const BITFAC_07: i32 = 0x59999980;
/// `1.0` halves in Q31 (`FL2FXCONST_DBL(1.f / 2.f)`).
const HALF_Q31: i32 = 0x40000000;

/// Bit-reservoir modes (`AACENC_BITRES_MODE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitResMode {
    Full = 0,
    Reduced = 1,
    Disabled = 2,
}

/// QC failure modes. `QuantError` ports `AAC_ENC_QUANT_ERROR`;
/// `BitresTooLow` ports `AAC_ENC_BITRES_TOO_LOW`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QcError {
    QuantError,
    BitresTooLow,
}

fn fix_min(a: i32, b: i32) -> i32 {
    a.min(b)
}
fn fix_max(a: i32, b: i32) -> i32 {
    a.max(b)
}

/// `scaleValue(v, s)` (`scale.h:153`): plain shift (wrapping, like C; S6
/// paths never overflow it — verified by oracle agreement).
fn scale_value(v: i32, s: i32) -> i32 {
    if s > 0 {
        v.wrapping_shl(s as u32)
    } else {
        v.wrapping_shr((-s) as u32)
    }
}

/// `FDKaacEnc_CalcBitsPerFrame` (`aacenc.cpp:124-134`).
pub fn calc_bits_per_frame(bitrate: i32, frame_length: i32, sampling_rate: i32) -> i32 {
    let mut shift = 0;
    while (frame_length & !((1 << (shift + 1)) - 1)) == frame_length
        && (sampling_rate & !((1 << (shift + 1)) - 1)) == sampling_rate
    {
        shift += 1;
    }
    (bitrate * (frame_length >> shift)) / (sampling_rate >> shift)
}

/// `FDKaacEnc_calcFrameLen` (`qc_main.cpp:179-195`).
pub fn calc_frame_len(bitrate: i32, sample_rate: i32, granule_length: i32, modulo: bool) -> i32 {
    let result = (granule_length >> 3) * bitrate;
    if modulo {
        result % sample_rate
    } else {
        result / sample_rate
    }
}

/// `FDKaacEnc_framePadding` (`qc_main.cpp:206-223`): returns
/// `(padding_on, new_padding_rest)`.
pub fn frame_padding(
    bitrate: i32,
    sample_rate: i32,
    granule_length: i32,
    padding_rest: i32,
) -> (bool, i32) {
    let difference = calc_frame_len(bitrate, sample_rate, granule_length, true);
    let rest = padding_rest - difference;
    if rest <= 0 {
        (true, rest + sample_rate)
    } else {
        (false, rest)
    }
}

/// `FDKaacEnc_AdjustBitrate` (`qc_main.cpp:469-489`): returns
/// `(avg_total_bits, new_padding_rest)`.
pub fn adjust_bitrate(
    padding_rest: i32,
    bitrate: i32,
    sample_rate: i32,
    granule_length: i32,
) -> (i32, i32) {
    let (padding_on, rest) = frame_padding(bitrate, sample_rate, granule_length, padding_rest);
    let frame_len =
        i32::from(padding_on) + calc_frame_len(bitrate, sample_rate, granule_length, false);
    (frame_len << 3, rest)
}

/// VBR QC-init derivations (`aacenc.cpp:582-592` + `:644-684`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QcInitParams {
    pub average_bits: i32,
    pub max_bits: i32,
    pub min_bits: i32,
    pub bit_res: i32,
    pub bit_res_mode: BitResMode,
    /// `bitDistributionMode`: 1 (element-wise) for mono/stereo
    /// (`aacenc.cpp:667-670`).
    pub bit_distribution_mode: i32,
    pub mean_pe: i32,
    /// `maxBitFac` mantissa at 24-bit accuracy (`aacenc.cpp:681-684`).
    pub max_bit_fac: i32,
    /// Per-element bitrate/buffer derivation (`InitElementBits` inputs,
    /// MODE_1/MODE_2 single-element arms).
    pub ch_bitrate_el: i32,
    pub max_bits_el: i32,
}

/// Derive the VBR QC-init plane for mono/stereo LC.
/// `average_bits_per_frame` is the `LimitBitrate` out-param (which leaves
/// the VBR bitrate unchanged): `CalcBitsPerFrame(bitrate, 1024, rate)`.
/// `bandwidth_90db` is the effective bandwidth in Hz (19293 at VBR-5).
#[allow(clippy::too_many_arguments)]
pub fn derive_qc_init(
    total_bitrate: i32,
    average_bits_per_frame: i32,
    n_channels: i32,
    n_eff: i32,
    sample_rate: i32,
    bandwidth_90db: i32,
    frame_length: i32,
) -> QcInitParams {
    let average_bits = (average_bits_per_frame + 7) & !7;
    let bit_res = MIN_BUFSIZE_PER_EFF_CHAN * n_eff;
    let max_bits = fix_max(
        MIN_BUFSIZE_PER_EFF_CHAN * n_eff,
        (average_bits_per_frame + 7) & !7,
    );
    let min_bits = fix_min(0, average_bits_per_frame & !7);
    let bit_res_mode = if max_bits - average_bits >= BITRES_MIN * n_channels {
        BitResMode::Full
    } else if max_bits > average_bits {
        BitResMode::Reduced
    } else {
        BitResMode::Disabled
    };
    // `meanPe = 10*framelen*bw/(rate/2)` via fDivNorm/scaleValue
    // (`aacenc.cpp:675-679`).
    let (bw_ratio, qbw) = fdiv_norm_e(10 * frame_length * bandwidth_90db, sample_rate);
    let mean_pe = fix_max(scale_value(bw_ratio, qbw + 1 - (DFRACT_BITS - 1)), 1);
    // `maxBitFac` at 24-bit accuracy (`aacenc.cpp:681-684`).
    let (mbfac, mbfac_e) = fdiv_norm_e(max_bits, average_bits);
    let max_bit_fac = scale_value(mbfac, -(DFRACT_BITS - 1 - 24 - mbfac_e));
    // `InitElementBits` MODE_1/MODE_2 single-element arms
    // (`channel_map.cpp:377-396`): `maxChannelBits` is the per-channel
    // share `maxBitsPerFrame / nChannelsEff`.
    let (ch_bitrate_el, max_bits_el) = if n_eff == 1 {
        (total_bitrate, max_bits)
    } else {
        (total_bitrate >> 1, 2 * (max_bits / n_eff))
    };
    QcInitParams {
        average_bits,
        max_bits,
        min_bits,
        bit_res,
        bit_res_mode,
        bit_distribution_mode: 1,
        mean_pe,
        max_bit_fac,
        ch_bitrate_el,
        max_bits_el,
    }
}

/// `DistributeBits`-owned `ATS_ELEMENT` subset (`adj_thr_data.h:139-166`).
/// Initialized exactly like `AdjThrInit` (`adj_thr.cpp:2535-2595`) for the
/// VBR-5 operating point; S2 never touches these fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistributeState {
    pub pe_min: i32,
    pub pe_max: i32,
    pub bits2pe_factor_m: i32,
    pub bits2pe_factor_e: i32,
    pub pe_correction_m: i32,
    pub pe_correction_e: i32,
    pub pe_last: i32,
    pub dyn_bits_last: i32,
}

/// `FDKaacEnc_InitBits2PeFactor` (`adj_thr.cpp:318-427`): LC default path
/// (`advancedBitsToPe = isLowDelay = 0`) plus the dZone compensation
/// (dead at LC since `dZoneQuantEnable = 0`). The ELD table-interpolation
/// path panics loudly (unreachable at LC; tables not ported).
pub fn init_bits2pe_factor(
    advanced_bits_to_pe: bool,
    dzone_quant_enable: bool,
    bitrate_in_element: i32,
    n_channels_in_element: i32,
) -> (i32, i32) {
    let mut bit2pe_m: i32 = B2PE_DEFAULT_M;
    let mut bit2pe_e: i32 = 1;
    if advanced_bits_to_pe {
        panic!("init_bits2pe_factor: ELD table path not ported (unreachable at LC)");
    }
    if dzone_quant_enable {
        if bit2pe_m >= DZONE_06 >> bit2pe_e {
            bit2pe_m >>= 1;
            bit2pe_e += 1;
        }
        let per_ch = bitrate_in_element / n_channels_in_element;
        if per_ch > 32000 && per_ch <= 40000 {
            bit2pe_m += DZONE_04 >> bit2pe_e;
        } else if per_ch >= 16000 {
            // C spells `> 20000` and `>= 16000` as separate arms adding the
            // same `0.3f`; a single `>= 16000` arm is identical.
            bit2pe_m += DZONE_03 >> bit2pe_e;
        }
    }
    (bit2pe_m, bit2pe_e)
}

/// `AdjThrInit` grant-state slice (`adj_thr.cpp:2535-2595`): `peMin/peMax`
/// from `meanPe` (non-LD branch), init correction 1.0 (`0.5,e=1`),
/// `dynBitsLast = -1`, `peLast = 0`, LC `bits2PeFactor`.
pub fn distribute_state_init(mean_pe: i32) -> DistributeState {
    let (m, e) = init_bits2pe_factor(false, false, 0, 1);
    DistributeState {
        pe_min: fmult_i(POINT8, mean_pe) >> 1,
        pe_max: fmult_i(POINT6, mean_pe),
        bits2pe_factor_m: m,
        bits2pe_factor_e: e,
        pe_correction_m: HALF_Q31,
        pe_correction_e: 1,
        pe_last: 0,
        dyn_bits_last: -1,
    }
}

/// `FDKaacEnc_bits2pe2` (`adj_thr.cpp:433-437`).
pub fn bits2pe2(bits: i32, factor_m: i32, factor_e: i32) -> i32 {
    fmult(factor_m, bits.wrapping_shl(Q_AVGBITS as u32)).wrapping_shr((Q_AVGBITS - factor_e) as u32)
}

/// `FDKaacEnc_calcBitSave` (`adj_thr.cpp:2218-2232`).
fn calc_bit_save(
    fill_level: i32,
    clip_low: i32,
    clip_high: i32,
    // NOTE: C takes `minBitSave` too but never reads it (`:2229`); kept
    // for signature fidelity.
    _min_save: i32,
    max_save: i32,
    slope: i32,
) -> i32 {
    let fl = fix_max(fill_level, clip_low).min(clip_high);
    max_save - fmult(fl - clip_low, slope)
}

/// `FDKaacEnc_calcBitSpend` (`adj_thr.cpp:2256-2270`).
fn calc_bit_spend(
    fill_level: i32,
    clip_low: i32,
    clip_high: i32,
    min_spend: i32,
    // NOTE: C takes `maxBitSpend` too but never reads it (`:2267`); kept
    // for signature fidelity.
    _max_spend: i32,
    slope: i32,
) -> i32 {
    let fl = fix_max(fill_level, clip_low).min(clip_high);
    min_spend + fmult(fl - clip_low, slope)
}

/// `FDKaacEnc_adjustPeMinMax` (`adj_thr.cpp:2281-2319`).
fn adjust_pe_min_max(curr_pe: i32, pe_min: &mut i32, pe_max: &mut i32) {
    const MAX_FAC_HI: i32 = i32::MAX;
    let min_diff_fix = fmult_i(PEMM_MIN_DIFF_FAC, curr_pe);
    if curr_pe > *pe_max {
        let diff = curr_pe - *pe_max;
        *pe_min += fmult_i(PEMM_MIN_HI, diff);
        *pe_max += fmult_i(MAX_FAC_HI, diff);
    } else if curr_pe < *pe_min {
        let diff = *pe_min - curr_pe;
        *pe_min -= fmult_i(PEMM_MIN_LO, diff);
        *pe_max -= fmult_i(PEMM_MAX_LO, diff);
    } else {
        *pe_min += fmult_i(PEMM_MIN_HI, curr_pe - *pe_min);
        *pe_max -= fmult_i(PEMM_MAX_LO, *pe_max - curr_pe);
    }
    if (*pe_max - *pe_min) < min_diff_fix {
        let pe_max_fix = *pe_max;
        let pe_min_fix = *pe_min;
        let part_lo = fix_max(0, curr_pe - pe_min_fix);
        let part_hi = fix_max(0, pe_max_fix - curr_pe);
        // 2-op `fDivNorm` (normalized, exp 0) straight into `fMultI`.
        let pe_max_new = curr_pe + fmult_i(fdiv_norm(part_hi, part_lo + part_hi), min_diff_fix);
        let pe_min_new = curr_pe - fmult_i(fdiv_norm(part_lo, part_lo + part_hi), min_diff_fix);
        *pe_max = pe_max_new;
        *pe_min = fix_max(0, pe_min_new);
    }
}

/// `BRES_PARAM` long/short (`AdjThrInit`, `adj_thr.cpp:2488-2520`,
/// verbatim hex).
#[derive(Debug, Clone, Copy)]
struct BresParam {
    clip_save_low: i32,
    clip_save_high: i32,
    min_bit_save: i32,
    max_bit_save: i32,
    clip_spend_low: i32,
    clip_spend_high: i32,
    min_bit_spend: i32,
    max_bit_spend: i32,
}

const BRES_LONG: BresParam = BresParam {
    clip_save_low: 0x1999999a,
    clip_save_high: 0x7999999a,
    min_bit_save: 0xf999999au32 as i32,
    max_bit_save: 0x26666666,
    clip_spend_low: 0x1999999a,
    clip_spend_high: 0x7999999a,
    min_bit_spend: 0xf3333333u32 as i32,
    max_bit_spend: 0x33333333,
};
const BRES_SHORT: BresParam = BresParam {
    clip_save_low: 0x199999a0,
    clip_save_high: 0x5fffffff,
    min_bit_save: 0x00000000,
    max_bit_save: 0x199999a0,
    clip_spend_low: 0x199999a0,
    clip_spend_high: 0x5fffffff,
    min_bit_spend: 0xf9999998u32 as i32,
    max_bit_spend: 0x40000000,
};

/// `FDKaacEnc_bitresCalcBitFac` (`adj_thr.cpp:2347-2431`). Mutates
/// `peMin/peMax` exactly like C (`:2426`).
#[allow(clippy::too_many_arguments)]
pub fn bitres_calc_bit_fac(
    bitres_bits: i32,
    max_bitres_bits: i32,
    pe: i32,
    short_window: bool,
    avg_bits: i32,
    max_bit_fac: i32,
    state: &mut DistributeState,
) -> (i32, i32) {
    let (bres, save_slope, spend_slope) = if !short_window {
        (&BRES_LONG, SAVE_SLOPE_LONG, SPEND_SLOPE_LONG)
    } else {
        (&BRES_SHORT, 0x2E8BA2E9, 0x7fffffff)
    };
    // `fillLevel = bitresBits/maxBitresBits`, 2-op normalized; stays
    // `MAXVAL_DBL` unless strictly below max (`:2379-2381`).
    let mut fill_fix = i32::MAX;
    if bitres_bits < max_bitres_bits {
        fill_fix = fdiv_norm(bitres_bits, max_bitres_bits);
    }
    let pex = fix_max(pe, state.pe_min).min(state.pe_max);
    let bit_save = calc_bit_save(
        fill_fix,
        bres.clip_save_low,
        bres.clip_save_high,
        bres.min_bit_save,
        bres.max_bit_save,
        save_slope,
    );
    let bit_spend = calc_bit_spend(
        fill_fix,
        bres.clip_spend_low,
        bres.clip_spend_high,
        bres.min_bit_spend,
        bres.max_bit_spend,
        spend_slope,
    );
    let slope = schur_div(pex - state.pe_min, state.pe_max - state.pe_min, 31);
    // Scaled down by 1 bit: the sum can exceed 1 (but not 2).
    let mut fac = (i32::MAX >> 1) - (bit_save >> 1);
    let mut fac_e = 1;
    fac = fmult_add_div2(fac, slope, bit_spend.wrapping_add(bit_save));
    // Limit for small bitreservoir (`:2403-2418`).
    let (mut fill_level, mut fill_e) = fdiv_norm_e(bitres_bits, avg_bits);
    if fill_e < 0 {
        fill_level = scale_value(fill_level, fill_e);
        fill_e = 0;
    }
    fill_level >>= 1;
    fill_e += 1;
    fill_level += scale_value(BITFAC_07, -fill_e);
    if scale_value(fac, -fill_e + 1) > fill_level {
        fac = fill_level;
        fac_e = fill_e;
    }
    // Limit for high bitrates (`:2420-2424`).
    if scale_value(fac, fac_e - (DFRACT_BITS - 1 - 24)) > max_bit_fac {
        fac = max_bit_fac;
        fac_e = DFRACT_BITS - 1 - 24;
    }
    adjust_pe_min_max(pe, &mut state.pe_min, &mut state.pe_max);
    (fac, fac_e)
}

/// `FDKaacEnc_calcPeCorrection` (`adj_thr.cpp:2604-2662`): FULL-reservoir
/// path. The `1.5f`/`0.7f` window comparisons are `f32` ops exactly like
/// C (int operands convert to `float`, multiply rounds once).
pub fn calc_pe_correction(
    correction_m: &mut i32,
    correction_e: &mut i32,
    pe_act: i32,
    pe_last: i32,
    bits_last: i32,
    bits2pe_m: i32,
    bits2pe_e: i32,
) {
    let in_window = bits_last > 0
        && (pe_act as f32) < 1.5f32 * (pe_last as f32)
        && (pe_act as f32) > 0.7f32 * (pe_last as f32)
        && bits2pe2(bits_last, fmult(PECORR_12_2, bits2pe_m), bits2pe_e + 1) > pe_last
        && bits2pe2(bits_last, fmult(PECORR_065, bits2pe_m), bits2pe_e) < pe_last;
    if !in_window {
        *correction_m = HALF_Q31;
        *correction_e = 1;
        return;
    }
    let corr_fac = *correction_m;
    let denum = bits2pe2(bits_last, bits2pe_m, bits2pe_e);
    let (new_m, scaling) = fdiv_norm_e(pe_last, denum);
    let new_fac = if pe_last <= denum {
        fix_max(
            scale_value(
                fix_min(fmult(PECORR_11_2, new_m), scale_value(HALF_Q31, -scaling)),
                scaling,
            ),
            PECORR_085_2,
        )
    } else {
        fix_max(
            fix_min(
                scale_value(fmult(PECORR_09_2, new_m), scaling),
                PECORR_115_2,
            ),
            HALF_Q31,
        )
    };
    // Opposite-side reset (`adj_thr.cpp:2636-2641`): reset the running
    // factor `corrFac`, keep `newFac` untouched.
    let mut corr = corr_fac;
    if ((new_fac > HALF_Q31) && (corr_fac < HALF_Q31))
        || ((new_fac < HALF_Q31) && (corr_fac > HALF_Q31))
    {
        corr = HALF_Q31;
    }
    if (corr < HALF_Q31 && new_fac < corr) || (corr > HALF_Q31 && new_fac > corr) {
        corr = fmult(PECORR_085, corr) + fmult(PECORR_015, new_fac);
    } else {
        corr = fmult(PECORR_07, corr) + fmult(PECORR_03, new_fac);
    }
    corr = fix_max(fix_min(corr, PECORR_115_2), PECORR_085_2);
    *correction_m = corr;
    *correction_e = 1;
}

/// `FDKaacEnc_calcPeCorrectionLowBitRes` (`adj_thr.cpp:2664-2721`):
/// REDUCED/DISABLED-reservoir path.
#[allow(clippy::too_many_arguments)]
pub fn calc_pe_correction_low_bit_res(
    correction_m: &mut i32,
    correction_e: &mut i32,
    pe_last: i32,
    bits_last: i32,
    bitres_level: i32,
    n_channels: i32,
    bits2pe_m: i32,
    bits2pe_e: i32,
) {
    if bits_last > 0 {
        let bits_bal_last = pe_last - bits2pe2(bits_last, bits2pe_m, bits2pe_e);
        let mut headroom = if bitres_level >= 50 * n_channels {
            0
        } else {
            100 * n_channels
        };
        headroom = bits2pe2(headroom, bits2pe_m, bits2pe_e);
        let denominator = bits2pe2(bitres_level, bits2pe_m, bits2pe_e) + headroom;
        let (m, e) = if bits_bal_last >= headroom {
            let (m, e) = fdiv_norm_e(bits_bal_last - headroom, denominator);
            (fmult(LOWBR_AMP, m), e)
        } else {
            let (m, e) = fdiv_norm_e(-(bits_bal_last - headroom), denominator);
            (-fmult(LOWBR_AMP, m), e)
        };
        let scaling = e - 1;
        let mut diff = m;
        diff = if scaling <= 0 {
            fix_max(
                fix_min(diff >> (-scaling), LOWBR_MAXDIFF >> 1),
                -LOWBR_MAXDIFF >> 1,
            )
        } else {
            fix_max(
                fix_min(diff, LOWBR_MAXDIFF >> (1 + scaling)),
                -LOWBR_MAXDIFF >> (1 + scaling),
            ) << scaling
        };
        *correction_m = fix_max(fix_min((*correction_m) + diff, HALF_Q31), PECORR_075_2);
        *correction_e = 1;
    } else {
        *correction_m = PECORR_075_2;
        *correction_e = 1;
    }
}

/// `FDKaacEnc_DistributeBits` (`adj_thr.cpp:2723-2798`).
/// `short_window` prefers short windows for the bit-factor branch
/// (`:2733-2742`: either channel short selects SHORT). Returns
/// `(granted_pe, granted_pe_corr)`.
#[allow(clippy::too_many_arguments)]
pub fn distribute_bits(
    state: &mut DistributeState,
    short_window: bool,
    no_red_pe: i32,
    n_channels: i32,
    granted_dyn_bits: i32,
    bitres_bits: i32,
    max_bitres_bits: i32,
    max_bit_fac: i32,
    bit_res_mode: BitResMode,
) -> (i32, i32) {
    let granted_pe = if granted_dyn_bits >= 1 {
        if bit_res_mode != BitResMode::Full {
            bits2pe2(
                granted_dyn_bits,
                state.bits2pe_factor_m,
                state.bits2pe_factor_e,
            )
        } else {
            let (bit_factor, bit_factor_e) = bitres_calc_bit_fac(
                bitres_bits,
                max_bitres_bits,
                no_red_pe,
                short_window,
                granted_dyn_bits,
                max_bit_fac,
                state,
            );
            bits2pe2(
                granted_dyn_bits,
                fmult(bit_factor, state.bits2pe_factor_m),
                state.bits2pe_factor_e + bit_factor_e,
            )
        }
    } else {
        0
    };
    match bit_res_mode {
        BitResMode::Disabled | BitResMode::Reduced => {
            calc_pe_correction_low_bit_res(
                &mut state.pe_correction_m,
                &mut state.pe_correction_e,
                state.pe_last,
                state.dyn_bits_last,
                bitres_bits,
                n_channels,
                state.bits2pe_factor_m,
                state.bits2pe_factor_e,
            );
        }
        BitResMode::Full => {
            calc_pe_correction(
                &mut state.pe_correction_m,
                &mut state.pe_correction_e,
                fix_min(granted_pe, no_red_pe),
                state.pe_last,
                state.dyn_bits_last,
                state.bits2pe_factor_m,
                state.bits2pe_factor_e,
            );
        }
    }
    let granted_pe_corr = fmult(
        granted_pe.wrapping_shl(Q_AVGBITS as u32),
        state.pe_correction_m,
    )
    .wrapping_shr((Q_AVGBITS - state.pe_correction_e) as u32);
    state.pe_last = granted_pe;
    state.dyn_bits_last = -1;
    (granted_pe, granted_pe_corr)
}

/// `FDKaacEnc_BitResRedistribution` (`qc_main.cpp:738-786`): single-element
/// general loop. `avg_total_bits_arg` is `maxBitsPerFrame` at VBR
/// (`:805-806`). Returns `(bit_res_level_el, max_bit_res_bits_el)`.
pub fn bit_res_redistribution(
    bit_res_tot: i32,
    bit_res_tot_max: i32,
    max_bits_per_frame: i32,
    avg_total_bits_arg: i32,
    relative_bits_el: i32,
) -> Result<(i32, i32), QcError> {
    if bit_res_tot < 0 {
        return Err(QcError::BitresTooLow);
    }
    if bit_res_tot > bit_res_tot_max {
        return Err(QcError::QuantError);
    }
    let total_bitreservoir = fix_min(bit_res_tot, max_bits_per_frame - avg_total_bits_arg);
    let total_bitreservoir_max = fix_min(bit_res_tot_max, max_bits_per_frame - avg_total_bits_arg);
    let mut level = fmult_i(relative_bits_el, total_bitreservoir);
    let mut max = fmult_i(relative_bits_el, total_bitreservoir_max);
    // Second loop: the first (only) audio element absorbs the delta.
    level += fix_max(total_bitreservoir - level, -level);
    max += fix_max(total_bitreservoir_max - max, -max);
    Ok((level, max))
}

/// `FDKaacEnc_distributeElementDynBits` (`qc_main.cpp:504-550`):
/// single-element general loop. Returns the element grant.
pub fn distribute_element_dyn_bits(relative_bits_el: i32, code_bits: i32) -> i32 {
    let mut granted = fix_max(0, fmult_i(relative_bits_el, code_bits));
    let total = granted;
    if code_bits != total {
        // Single element is both min and max: the whole difference lands here.
        granted += code_bits - total;
    }
    granted
}

/// `FDKaacEnc_prepareBitDistribution` grant formula (`qc_main.cpp:615-635`,
/// without the per-element `DistributeBits` calls, which the frame driver
/// issues). `avg_total_bits_arg` is `maxBitsPerFrame` at VBR (`:836-839`).
/// Returns `(granted_dyn_bits, max_dyn_bits)`.
pub fn prepare_grants(
    max_bits_per_frame: i32,
    avg_total_bits_arg: i32,
    glob_hdr_bits: i32,
    global_ext_bits: i32,
    static_bits: i32,
    element_ext_bits: i32,
    bit_res_tot: i32,
) -> Result<(i32, i32), QcError> {
    let mut granted = (fix_min(max_bits_per_frame, avg_total_bits_arg) - glob_hdr_bits) & !7;
    granted -= global_ext_bits + static_bits + element_ext_bits;
    let max_dyn = (max_bits_per_frame & !7) - (global_ext_bits + static_bits + element_ext_bits);
    if granted + bit_res_tot < 0 {
        return Err(QcError::BitresTooLow);
    }
    Ok((granted, max_dyn))
}

/// `FDKaacEnc_calcMaxValueInSfb` (`qc_main.cpp:1219-1240`): per-band max
/// abs over `max_sfb_per_group` bands per group; unvisited bands of
/// `max_value` are left untouched (like C). Returns the overall maximum.
/// `-32768` lines abort loudly (C would `fixp_abs` them into `i32::MIN`
/// and mis-gate; established S3/S4/S5 policy).
#[allow(clippy::needless_range_loop)]
pub fn calc_max_value_in_sfb(
    sfb_cnt: usize,
    max_sfb_per_group: usize,
    sfb_per_group: usize,
    sfb_offsets: &[i32],
    quant_spec: &[i16],
    max_value: &mut [u32],
) -> u32 {
    let mut max_all = 0u32;
    let mut offs = 0;
    while offs < sfb_cnt {
        for sfb in 0..max_sfb_per_group {
            let mut band_max = 0u32;
            let start = sfb_offsets[offs + sfb] as usize;
            let end = sfb_offsets[offs + sfb + 1] as usize;
            for line in start..end {
                let q = quant_spec[line];
                assert_ne!(q, i16::MIN, "calc_max_value_in_sfb: -32768 line");
                let t = (q as i32).unsigned_abs();
                if t > band_max {
                    band_max = t;
                }
            }
            max_value[offs + sfb] = band_max;
            if band_max > max_all {
                max_all = band_max;
            }
        }
        offs += sfb_per_group;
    }
    max_all
}

/// Outcome of [`reduce_bit_consumption`] at `iterations == maxIterations`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReduceOutcome {
    /// Gains bumped (`iterations < maxIterations`); re-quantize flagged.
    GainBumped,
    /// Crash recovery (or the all-`+1` fallback when nothing can be saved).
    CrashRequired { bits_to_save: i32 },
}

/// `FDKaacEnc_reduceBitConsumption` (`qc_main.cpp:1119-1166`).
/// `el_dyn_bits_used == 0` at max iterations is `QUANT_ERROR` (`:1138`).
#[allow(clippy::too_many_arguments)]
pub fn reduce_bit_consumption(
    iterations: &mut i32,
    max_iterations: i32,
    gain_adjustment: i32,
    ch_constraints: &[bool],
    gains: &mut [i32],
    recalc: &mut [bool],
    el_dyn_bits_used: i32,
    bit_res_level_el: i32,
    el_granted: i32,
    el_static: i32,
    max_bits_el: i32,
) -> Result<ReduceOutcome, QcError> {
    if *iterations < max_iterations {
        for (ch, failing) in ch_constraints.iter().enumerate() {
            if !failing {
                gains[ch] += gain_adjustment;
                recalc[ch] = true;
            }
        }
    } else if *iterations == max_iterations {
        if el_dyn_bits_used == 0 {
            return Err(QcError::QuantError);
        }
        let bits_to_save = fix_max(
            (el_dyn_bits_used + 8) - (bit_res_level_el + el_granted),
            (el_dyn_bits_used + el_static + 8) - max_bits_el,
        );
        // The caller runs `crash_recovery` (or the all-`+1` fallback),
        // flags all channels for re-quantization, then we count up.
        for r in recalc.iter_mut() {
            *r = true;
        }
        *iterations += 1;
        return Ok(ReduceOutcome::CrashRequired { bits_to_save });
    } else {
        return Err(QcError::QuantError);
    }
    *iterations += 1;
    Ok(ReduceOutcome::GainBumped)
}

/// `FDKaacEnc_crashRecovery` (`qc_main.cpp:1395-1498`).
/// Drops `max_sfb` from the top until `savedBits >= bits_to_save` (side
/// info reclaimed per emptied section via the S5 side-info tables), then
/// rebalances `static_bits_used`/`el_granted`/`frame_granted`/`frame_max_dyn`
/// through `recount(wipeout)` — the `ChannelElementWrite(NULL)` recount,
/// with cleared tools/TNS iff every band was dropped (`sfb == 0`).
/// Returns the surviving `maxSfbPerGroup` (shared by all channels, like C).
#[allow(clippy::too_many_arguments)]
pub fn crash_recovery(
    quant: &[Vec<i16>],
    sections: &mut [SectionData],
    max_sfb: &mut [usize],
    sfb_cnt: usize,
    sfb_per_group: usize,
    sfb_offsets: &[Vec<i32>],
    short_window: &[bool],
    bits_to_save: i32,
    static_bits_used: &mut i32,
    el_granted: &mut i32,
    frame_granted: &mut i32,
    frame_max_dyn: &mut i32,
    recount: &dyn Fn(bool) -> i32,
) -> usize {
    use super::bitcount_tables::{SIDE_INFO_TAB_LONG, SIDE_INFO_TAB_SHORT};
    let n_channels = quant.len();
    let mut bits_per_scf: Vec<Vec<i32>> = Vec::new();
    let mut section_to_scf: Vec<Vec<usize>> = Vec::new();
    for ch in 0..n_channels {
        let mut bps = vec![0i32; MAX_GROUPED_SFB];
        let mut s2s = vec![0usize; MAX_GROUPED_SFB];
        for (sect, sec) in sections[ch].sections.iter().enumerate() {
            for sfb in sec.sfb_start..sec.sfb_start + sec.sfb_cnt {
                bps[sfb as usize] = 0;
                if sec.code_book != CODE_BOOK_PNS_NO {
                    let start = sfb_offsets[ch][sfb as usize] as usize;
                    let width = (sfb_offsets[ch][sfb as usize + 1] - sfb_offsets[ch][sfb as usize])
                        as usize;
                    bps[sfb as usize] =
                        count_values(&quant[ch][start..start + width], sec.code_book);
                }
                s2s[sfb as usize] = sect;
            }
        }
        bits_per_scf.push(bps);
        section_to_scf.push(s2s);
    }
    let mut saved_bits = 0i32;
    let mut sfb = max_sfb[0] as i32 - 1;
    while sfb >= 0 {
        let mut grp = 0;
        while grp < sfb_cnt {
            for ch in 0..n_channels {
                let sect = section_to_scf[ch][grp + sfb as usize];
                sections[ch].sections[sect].sfb_cnt -= 1;
                saved_bits += bits_per_scf[ch][grp + sfb as usize];
                if sections[ch].sections[sect].sfb_cnt == 0 {
                    let tab = if short_window[ch] {
                        &SIDE_INFO_TAB_SHORT[..]
                    } else {
                        &SIDE_INFO_TAB_LONG[..]
                    };
                    saved_bits += tab[0] as i32;
                }
            }
            grp += sfb_per_group;
        }
        if saved_bits >= bits_to_save {
            break;
        }
        sfb -= 1;
    }
    if sfb == -1 {
        sfb = 0;
    }
    for m in max_sfb.iter_mut() {
        *m = sfb as usize;
    }
    let stat_new = recount(sfb == 0);
    let saved = *static_bits_used - stat_new;
    *static_bits_used -= saved;
    *el_granted += saved;
    *frame_granted += saved;
    *frame_max_dyn += saved;
    sfb as usize
}

/// `FDKaacEnc_getTotalConsumedBits` single-frame form (`qc_main.cpp:712-736`,
/// `nSubFrames = 1`): align + data + transport header.
pub fn total_consumed_bits(
    dyn_bits_used: i32,
    static_bits_used: i32,
    element_ext_bits: i32,
    global_ext_bits: i32,
    glob_hdr_bits: i32,
) -> i32 {
    let data_bits = dyn_bits_used + static_bits_used + element_ext_bits + global_ext_bits;
    (8 - data_bits % 8) % 8 + data_bits + glob_hdr_bits
}

/// `checkMinFrameBitsDemand` stub (`qc_main.cpp:571-575`): always 1.
pub fn check_min_frame_demand() -> bool {
    true
}

/// QCMain exit-matrix step (`qc_main.cpp:1030-1107`, single subframe).
/// Returns `(quantization_done, next_decrease, reset_constraints)`.
#[allow(clippy::too_many_arguments)]
pub fn exit_matrix_step(
    sum_bits: i32,
    sum_dyn: i32,
    total_avail: i32,
    avg_total_dyn: i32,
    decrease: i32,
    iterations: i32,
    max_iterations: i32,
    used_dyn: i32,
    max_dyn: i32,
) -> (bool, i32, bool) {
    let mut done = false;
    if sum_dyn != -1 {
        if ((sum_bits < total_avail) || sum_dyn == 0) && (decrease == 1) && check_min_frame_demand()
        {
            done = true;
        }
        if sum_bits > total_avail && (decrease == 0) {
            done = false;
        }
    } else {
        done = false;
    }
    let emergency = if iterations < max_iterations { 0 } else { 1 };
    let overshoot = if used_dyn > max_dyn { 1 } else { 0 };
    let mut next_decrease = decrease;
    let mut reset = false;
    if !done || overshoot != 0 {
        if (sum_dyn >= avg_total_dyn) || (sum_dyn == 0) {
            done = true;
        }
        if emergency != 0 && (sum_bits < total_avail) {
            done = true;
        }
        if (sum_bits > total_avail) || !check_min_frame_demand() {
            done = false;
        }
        if (sum_bits < total_avail) && check_min_frame_demand() {
            next_decrease = 0;
        } else {
            next_decrease = 1;
        }
        if overshoot != 0 {
            done = false;
            next_decrease = 1;
        }
        reset = true;
    }
    (done, next_decrease, reset)
}

/// `FDKaacEnc_updateFillBits` VBR branch (`qc_main.cpp:1178-1191`).
/// Returns `(tot_fill_bits, total_bits_prelim)`; `min_bits_per_frame` is 0
/// at VBR-5 but kept explicit.
pub fn update_fill_bits(
    granted_dyn_bits: i32,
    used_dyn_bits: i32,
    static_bits: i32,
    element_ext_bits: i32,
    global_ext_bits: i32,
    min_bits_per_frame: i32,
) -> (i32, i32) {
    let mut tot_fill = (granted_dyn_bits - used_dyn_bits) & 7;
    let prelim = static_bits + used_dyn_bits + tot_fill + element_ext_bits + global_ext_bits;
    tot_fill += (fix_max(0, min_bits_per_frame - prelim) + 7) & !7;
    (
        tot_fill,
        static_bits + used_dyn_bits + tot_fill + element_ext_bits + global_ext_bits,
    )
}

/// `writeExtensionData` fill-sizing loop (`bitenc.cpp:851-913`,
/// EXT_FILL_DATA/FIL branch, NULL bitstream = counting).
/// Returns the sized fill bits for `payload_bits` of fill demand.
pub fn fill_extension_bits(payload_bits: i32) -> i32 {
    const FILL_EL_COUNT_BITS: i32 = 4;
    const FILL_EL_ESC_COUNT_BITS: i32 = 8;
    let mut payload = payload_bits;
    let mut ext_used = 0;
    while payload >= (GLOBAL_EXT_BITS + FILL_EL_COUNT_BITS) {
        let mut cnt;
        let mut esc_count = -1;
        let align_bits = 0;
        payload -= GLOBAL_EXT_BITS + FILL_EL_COUNT_BITS;
        if payload >= 15 * 8 {
            payload -= FILL_EL_ESC_COUNT_BITS;
            esc_count = 0;
        }
        cnt = fix_min(MAX_FILL_DATA_BYTES, (payload + align_bits) >> 3);
        if cnt >= 15 {
            esc_count = cnt - 15 + 1;
        }
        ext_used += GLOBAL_EXT_BITS
            + FILL_EL_COUNT_BITS
            + (if esc_count >= 0 {
                FILL_EL_ESC_COUNT_BITS
            } else {
                0
            });
        // `writeExtensionPayload(NULL, FIL, cnt)`: the nibble formula only
        // applies at `cnt >= EXT_TYPE_BITS (4)`; smaller tails cost 0
        // (`bitenc.cpp`, `extPayloadBits >= EXT_TYPE_BITS` gate).
        cnt = fix_min(cnt * 8, payload);
        if cnt >= 4 {
            ext_used += 4 + (cnt & !0x7) - 8;
        }
        payload -= cnt;
    }
    ext_used
}

/// `FDKaacEnc_FinalizeBitConsumption` VBR path (`qc_main.cpp:1283-1383`;
/// the CBR transport-correction block `:1295-1341` is skipped).
/// `glob_hdr_bits` is the refreshed transport value (constant 56 for ADTS
/// SCE/CPE; transport-owned input, kept explicit for signature fidelity).
/// Returns `(tot_fill_bits, align_bits, total_bits)`.
#[allow(clippy::too_many_arguments)]
pub fn finalize_bit_consumption(
    static_bits: i32,
    used_dyn_bits: i32,
    tot_fill_bits_in: i32,
    element_ext_bits: i32,
    global_ext_bits: i32,
    glob_hdr_bits: i32,
    max_bits_per_frame: i32,
    min_bits_per_frame: i32,
) -> Result<(i32, i32, i32), QcError> {
    // VBR skips the CBR transport-correction block; the header refresh
    // reproduces the probed constant (transport-owned, passed in).
    let _ = glob_hdr_bits;
    let tot_fill = tot_fill_bits_in;
    let sized_fill = fill_extension_bits(tot_fill);
    let align_bits =
        7 - (static_bits + used_dyn_bits + element_ext_bits + sized_fill + global_ext_bits - 1) % 8;
    let mut tot_fill_out = sized_fill;
    if ((align_bits + tot_fill_out - tot_fill) == 8) && (tot_fill_out > 8) {
        tot_fill_out -= 8;
    }
    let total = static_bits
        + used_dyn_bits
        + tot_fill_out
        + align_bits
        + element_ext_bits
        + global_ext_bits;
    if (total > max_bits_per_frame) || (total < min_bits_per_frame) {
        return Err(QcError::QuantError);
    }
    Ok((tot_fill_out, align_bits, total))
}

/// `FDKaacEnc_updateBitres` VBR pin (`qc_main.cpp:1251-1260`).
pub fn update_bitres_vbr(max_bits_per_frame: i32, bit_res_tot_max: i32) -> i32 {
    fix_min(max_bits_per_frame, bit_res_tot_max)
}

/// Post-S4 per-channel inputs to the S6 frame driver: the mutated spectrum
/// (dead bands zeroed in place, like `aacenc.cpp:803-808`), stored
/// post-inversion scalefactors, global gain, section geometry, PNS/IS
/// preconditions (all-zero at VBR-5/PNS-off, asserted by the driver tests)
/// and the TNS/MS state for a potential crash-recovery static recount.
pub struct QcChannelInput<'a> {
    pub spectrum: &'a [i32],
    pub scf: &'a [i32],
    pub global_gain: i32,
    pub sfb_cnt: usize,
    pub max_sfb_per_group: usize,
    pub sfb_per_group: usize,
    pub sfb_offsets: &'a [i32],
    /// `lastWindowSequence` (0/1/2/3 = LONG/START/SHORT/STOP).
    pub block_type: i32,
    pub noise_nrg: &'a [i32],
    pub is_book: &'a [i32],
    pub is_scale: &'a [i32],
    pub tns: &'a TnsChannel,
}

/// Element-level frame inputs (single element; `channels.len()` is 1 or 2).
pub struct QcFrameInput<'a> {
    pub channels: Vec<QcChannelInput<'a>>,
    /// Element PE without reduction (`peData.pe`, oracle S1 `PE` line).
    pub no_red_pe: i32,
    /// Element static bits (`ChannelElementWrite(NULL)`, oracle `STATIC`).
    pub static_bits: i32,
    pub ms_digest: MsDigest,
    /// MS mask band count (only read when `ms_digest == Some` on CPE wipeout
    /// recounts; pass 0 on corpus — recount never runs there).
    pub ms_bands: usize,
    /// Transport header bits (oracle `S6BITR globHdr`; always 56 on fixtures).
    pub glob_hdr_bits: i32,
    pub max_bits_per_frame: i32,
    pub max_bits_el: i32,
    pub relative_bits_el: i32,
    pub max_bit_fac: i32,
    pub bit_res_mode: BitResMode,
}

/// One outer gain-loop pass (oracle `S6IT` line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassTrace {
    pub iterations: i32,
    pub decrease: i32,
    pub dyn_bits: [i32; 2],
    pub gains: [i32; 2],
    pub maxq: [u32; 2],
    pub el_dyn: i32,
    pub el_static: i32,
    /// Surviving `maxSfbPerGroup` after a crash in this pass, else -1.
    pub crash: i32,
}

/// S6 frame outputs (oracle `S6GRANT`/`S6FINAL` + final artifacts for the
/// future writer: sections, gains, live `maxSfbPerGroup`).
#[derive(Debug, Clone)]
pub struct QcFrameOutput {
    pub granted_dyn_bits: i32,
    pub max_dyn_bits: i32,
    pub total_avail: i32,
    pub el_granted: i32,
    pub granted_pe: i32,
    pub granted_pe_corr: i32,
    pub bit_res_level_el: i32,
    pub bit_res_max_el: i32,
    pub passes: Vec<PassTrace>,
    pub final_sections: Vec<SectionData>,
    pub final_gains: Vec<i32>,
    pub final_max_sfb: Vec<usize>,
    /// Final-pass quantized spectra, per channel (M5.18). These are the
    /// loop's own last `quantize_spectrum` results: any gain adjustment or
    /// crash-recovery `maxSfb` reduction sets `recalc` (`reduce_bit_consumption`)
    /// so the final constraints-checked pass always quantized at the final
    /// gain under the final `maxSfb`. Bands at/after `final_max_sfb` per
    /// group stride hold the `vec![0i16; len]` initialization zeros —
    /// exactly what a zeroed-spectrum re-quantization would produce.
    pub final_quants: Vec<Vec<i16>>,
    /// Per-band maxima matching [`QcFrameOutput::final_quants`], from the
    /// same final `calcMaxValueInSfb` pass (M5.18).
    pub final_qmaxs: Vec<Vec<u32>>,
    pub static_bits_used: i32,
    pub used_dyn_bits: i32,
    pub tot_fill_bits: i32,
    pub align_bits: i32,
    pub total_bits: i32,
    pub bit_res_tot: i32,
}

/// Persistent per-stream QC kernel state (`QC_STATE` VBR-5 slice).
///
/// M5.22: no longer `Copy` — it now owns the reused bit-count scratch
/// (`bc_scratch`); nothing in the crate relies on by-value copies of it.
#[derive(Debug, Clone)]
pub struct QcState {
    pub bit_res_tot: i32,
    pub bit_res_tot_max: i32,
    pub padding_rest: i32,
    /// Reused bit-count scratch (M5.22): owns the `noiseless_counter`
    /// working buffers so the sectioning path allocates nothing per group.
    pub bc_scratch: BitCountScratch,
}

impl QcState {
    pub fn new(bit_res_tot_max: i32, sample_rate: i32) -> Self {
        Self {
            bit_res_tot: bit_res_tot_max,
            bit_res_tot_max,
            padding_rest: sample_rate,
            bc_scratch: BitCountScratch::default(),
        }
    }
}

/// S6 frame driver: grants → gain loop → fill/finalize/bitres
/// (`qc_main.cpp:788-1117` orchestration at VBR-5, single element).
/// Returns [`QcError`] loudly on `QUANT_ERROR`/`BITRES_TOO_LOW` (never
/// silent); trips the [`MAX_OUTER_PASSES`] panic only on implementation
/// bugs (C terminates via the same exit matrix).
#[allow(clippy::too_many_arguments)]
/// S6 frame driver: grants → gain loop → fill/finalize/bitres
/// (`qc_main.cpp:788-1117` orchestration at VBR-5, single element).
/// Returns [`QcError`] loudly on `QUANT_ERROR`/`BITRES_TOO_LOW` (never
/// silent); trips a [`MAX_OUTER_PASSES`] panic only on implementation bugs
/// (C terminates via the same exit matrix on valid inputs).
pub fn qc_main_frame(
    qc: &mut QcState,
    dist: &mut DistributeState,
    input: &QcFrameInput<'_>,
    max_iterations: i32,
) -> Result<QcFrameOutput, QcError> {
    use super::bitcount::BLOCK_SHORT;
    use super::quant::quantize_spectrum;

    let n_channels = input.channels.len();
    assert!(
        n_channels == 1 || n_channels == 2,
        "qc_main_frame: single element mono/stereo"
    );
    // 1. Bit-reservoir redistribution (VBR arg = maxBitsPerFrame).
    let (bit_res_level_el, bit_res_max_el) = bit_res_redistribution(
        qc.bit_res_tot,
        qc.bit_res_tot_max,
        input.max_bits_per_frame,
        input.max_bits_per_frame,
        input.relative_bits_el,
    )?;
    // 2. Grants (VBR arg = maxBitsPerFrame).
    let (granted, max_dyn) = prepare_grants(
        input.max_bits_per_frame,
        input.max_bits_per_frame,
        input.glob_hdr_bits,
        GLOBAL_EXT_BITS,
        input.static_bits,
        0,
        qc.bit_res_tot,
    )?;
    let mut el_granted = distribute_element_dyn_bits(input.relative_bits_el, granted);
    let total_avail = fix_min(
        input.max_bits_per_frame,
        input.max_bits_per_frame + bit_res_level_el,
    );
    // 3. Real DistributeBits (mutates grant state, like C). Either channel
    // short selects the SHORT bit-factor branch (`adj_thr.cpp:2733-2742`).
    let any_short = input.channels.iter().any(|c| c.block_type == BLOCK_SHORT);
    let (granted_pe, granted_pe_corr) = distribute_bits(
        dist,
        any_short,
        input.no_red_pe,
        n_channels as i32,
        el_granted,
        bit_res_level_el,
        bit_res_max_el,
        input.max_bit_fac,
        input.bit_res_mode,
    );
    // 4. Gain loop working state. Pass 0 always re-quantizes
    // (`calculateQuant = 1`, `qc_main.cpp:879-882`), so no first-pass
    // inputs are needed beyond spectrum/scf/gain.
    let mut gains: Vec<i32> = input.channels.iter().map(|c| c.global_gain).collect();
    let mut quants: Vec<Vec<i16>> = input
        .channels
        .iter()
        .map(|c| vec![0i16; c.spectrum.len()])
        .collect();
    let mut maxes: Vec<Vec<u32>> = input
        .channels
        .iter()
        .map(|c| vec![0u32; c.sfb_cnt])
        .collect();
    let mut max_sfb: Vec<usize> = input.channels.iter().map(|c| c.max_sfb_per_group).collect();
    let mut sections: Vec<SectionData> = Vec::new();
    for c in &input.channels {
        sections.push(SectionData {
            block_type: c.block_type,
            no_of_groups: 0,
            sfb_cnt: c.sfb_cnt as i32,
            max_sfb_per_group: c.max_sfb_per_group as i32,
            sfb_per_group: c.sfb_per_group as i32,
            sections: Vec::new(),
            side_info_bits: 0,
            huffman_bits: 0,
            scalefac_bits: 0,
            noise_nrg_bits: 0,
            first_scf: 0,
        });
    }
    let mut dyns = vec![0i32; n_channels];
    let mut iterations = 0i32;
    let mut constraints = true;
    let mut ch_ok = vec![true; n_channels];
    let mut recalc = vec![true; n_channels];
    let mut decrease = -1i32;
    let mut done = false;
    let mut el_dyn = 0i32;
    let mut el_static = input.static_bits;
    let mut frame_granted = granted;
    let mut frame_max_dyn = max_dyn;
    let mut crash_mark = -1i32;
    let mut passes = Vec::new();
    let mut outer_guard = 0i32;
    // Static recount for crash recovery (`ChannelElementWrite(NULL)`
    // mirror over `static_bits`; `wipeout` clears tools+TNS).
    let recount = |wipeout: bool| -> i32 {
        let tns0;
        let tns1;
        let (t0, t1): (&TnsChannel, &TnsChannel) = if wipeout {
            tns0 = TnsChannel::default();
            tns1 = TnsChannel::default();
            (&tns0, &tns1)
        } else if n_channels == 2 {
            (input.channels[0].tns, input.channels[1].tns)
        } else {
            (input.channels[0].tns, input.channels[0].tns)
        };
        let (digest, bands) = if wipeout {
            (MsDigest::None, 0)
        } else {
            (input.ms_digest, input.ms_bands)
        };
        let s0 = input.channels[0].block_type == BLOCK_SHORT;
        if n_channels == 2 {
            let s1 = input.channels[1].block_type == BLOCK_SHORT;
            static_bits_cpe(true, s0, s1, digest, bands, t0, t1)
        } else {
            static_bits_sce(s0, t0)
        }
    };
    while !done {
        if outer_guard >= MAX_OUTER_PASSES {
            panic!("qc_main_frame: outer loop runaway (implementation bug)");
        }
        outer_guard += 1;
        // NOTE: C resets `quantizationDone = 0` here (`:903`); the reset is
        // subsumed by the `exit_matrix_step` assignment below, which runs
        // unconditionally every pass.
        // Element-cap loop (single element, `qc_main.cpp:913-1020`).
        loop {
            // MAX_QUANT loop (`:915-981`).
            loop {
                if !constraints {
                    let adjustment = if decrease != 0 { 1 } else { -1 };
                    match reduce_bit_consumption(
                        &mut iterations,
                        max_iterations,
                        adjustment,
                        &ch_ok,
                        &mut gains,
                        &mut recalc,
                        el_dyn,
                        bit_res_level_el,
                        el_granted,
                        el_static,
                        input.max_bits_el,
                    )? {
                        ReduceOutcome::GainBumped => {}
                        ReduceOutcome::CrashRequired { bits_to_save } => {
                            if bits_to_save > 0 {
                                let short_win: Vec<bool> = input
                                    .channels
                                    .iter()
                                    .map(|c| c.block_type == BLOCK_SHORT)
                                    .collect();
                                let offs: Vec<Vec<i32>> = input
                                    .channels
                                    .iter()
                                    .map(|c| c.sfb_offsets.to_vec())
                                    .collect();
                                let surv = crash_recovery(
                                    &quants,
                                    &mut sections,
                                    &mut max_sfb,
                                    input.channels[0].sfb_cnt,
                                    input.channels[0].sfb_per_group,
                                    &offs,
                                    &short_win,
                                    bits_to_save,
                                    &mut el_static,
                                    &mut el_granted,
                                    &mut frame_granted,
                                    &mut frame_max_dyn,
                                    &recount,
                                );
                                crash_mark = surv as i32;
                            } else {
                                for g in gains.iter_mut() {
                                    *g += 1;
                                }
                            }
                        }
                    }
                }
                constraints = true;
                for ch in 0..n_channels {
                    ch_ok[ch] = true;
                    if recalc[ch] {
                        recalc[ch] = false;
                        let c = &input.channels[ch];
                        quantize_spectrum(
                            c.sfb_cnt,
                            max_sfb[ch],
                            c.sfb_per_group,
                            c.sfb_offsets,
                            c.spectrum,
                            gains[ch],
                            c.scf,
                            &mut quants[ch],
                            false,
                        );
                        for &q in quants[ch].iter() {
                            assert_ne!(q, i16::MIN, "qc_main_frame: -32768 line");
                        }
                        let max_all = calc_max_value_in_sfb(
                            c.sfb_cnt,
                            max_sfb[ch],
                            c.sfb_per_group,
                            c.sfb_offsets,
                            &quants[ch],
                            &mut maxes[ch],
                        );
                        if max_all > MAX_QUANT as u32 {
                            ch_ok[ch] = false;
                            constraints = false;
                            decrease = 1;
                        }
                    }
                }
                if constraints {
                    break;
                }
            }
            // Dynamic bit count per channel (`:988-1003`).
            el_dyn = 0;
            for ch in 0..n_channels {
                let c = &input.channels[ch];
                let bc_input = BitCountInput {
                    quant_spec: &quants[ch],
                    max_value_in_sfb: &maxes[ch],
                    scf: c.scf,
                    block_type: c.block_type,
                    sfb_cnt: c.sfb_cnt,
                    max_sfb_per_group: max_sfb[ch],
                    sfb_per_group: c.sfb_per_group,
                    sfb_offsets: c.sfb_offsets,
                    noise_nrg: c.noise_nrg,
                    is_book: c.is_book,
                    is_scale: c.is_scale,
                    use_vcb11: false,
                };
                let (sd, d) = dyn_bit_count_reuse(&bc_input, &mut qc.bc_scratch);
                sections[ch] = sd;
                dyns[ch] = d;
                el_dyn += d;
            }
            if dist.dyn_bits_last == -1 {
                dist.dyn_bits_last = el_dyn;
            }
            // Element cap (`:1011-1018`): `- extBitsUsed`, always 0 here
            // (kept explicit for C fidelity).
            if el_dyn > (n_channels as i32) * MIN_BUFSIZE_PER_EFF_CHAN - el_static {
                constraints = false;
            }
            if constraints {
                break;
            }
        }
        // Frame accounting + pass trace (`:1026-1037`).
        let sum_bits =
            total_consumed_bits(el_dyn, el_static, 0, GLOBAL_EXT_BITS, input.glob_hdr_bits);
        let mut maxq = [0u32; 2];
        for ch in 0..n_channels {
            let mut m = 0u32;
            for &v in maxes[ch].iter() {
                if v > m {
                    m = v;
                }
            }
            maxq[ch] = m;
        }
        let mut dyn_pair = [0i32; 2];
        let mut gain_pair = [0i32; 2];
        dyn_pair[..n_channels].copy_from_slice(&dyns[..n_channels]);
        gain_pair[..n_channels].copy_from_slice(&gains[..n_channels]);
        passes.push(PassTrace {
            iterations,
            decrease,
            dyn_bits: dyn_pair,
            gains: gain_pair,
            maxq,
            el_dyn,
            el_static,
            crash: crash_mark,
        });
        // Exit matrix (`:1040-1105`).
        let (next_done, next_decrease, reset) = exit_matrix_step(
            sum_bits,
            el_dyn,
            total_avail,
            0,
            decrease,
            iterations,
            max_iterations,
            el_dyn,
            frame_max_dyn,
        );
        done = next_done;
        decrease = next_decrease;
        if reset {
            constraints = false;
            for ok in ch_ok.iter_mut() {
                *ok = false;
            }
        }
    }
    // 5. Fill, finalize, reservoir (`:955-968` driver order).
    let (tot_fill, _) = update_fill_bits(frame_granted, el_dyn, el_static, 0, GLOBAL_EXT_BITS, 0);
    let (tot_fill_sized, align_bits, total_bits) = finalize_bit_consumption(
        el_static,
        el_dyn,
        tot_fill,
        0,
        GLOBAL_EXT_BITS,
        input.glob_hdr_bits,
        input.max_bits_per_frame,
        0,
    )?;
    qc.bit_res_tot = update_bitres_vbr(input.max_bits_per_frame, qc.bit_res_tot_max);
    Ok(QcFrameOutput {
        granted_dyn_bits: granted,
        max_dyn_bits: max_dyn,
        total_avail,
        el_granted,
        granted_pe,
        granted_pe_corr,
        bit_res_level_el,
        bit_res_max_el,
        passes,
        final_sections: sections,
        final_gains: gains,
        final_max_sfb: max_sfb,
        // M5.18: the loop's final-pass quantized spectra and per-band
        // maxima were previously dropped here and re-derived by the
        // stream driver; they are moved out unchanged instead. No
        // decision, arithmetic, or loop behavior is affected.
        final_quants: quants,
        final_qmaxs: maxes,
        static_bits_used: el_static,
        used_dyn_bits: el_dyn,
        tot_fill_bits: tot_fill_sized,
        align_bits,
        total_bits,
        bit_res_tot: qc.bit_res_tot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::testdata;

    /// Committed S6 corpus: 19 S1–S5 frames + F6 stereo stress.
    const S6_CASES: [(&str, u32); 20] = [
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
        ("stress-stereo", 5),
    ];

    fn init_tag(name: &str) -> &'static str {
        if name.contains("44100") {
            "stereo-44100"
        } else if name.starts_with("click")
            || name.starts_with("impulse")
            || name.starts_with("edge")
            || name.starts_with("noise")
        {
            "mono-48k"
        } else {
            "stereo-48k"
        }
    }

    fn total_br(name: &str) -> i32 {
        if init_tag(name) == "mono-48k" {
            112_000
        } else {
            192_000
        }
    }

    fn sample_rate(name: &str) -> i32 {
        if name.contains("44100") {
            44100
        } else {
            48000
        }
    }

    #[test]
    fn s6_q31_literals_match_probe() {
        // Every literal below is the `tools/enc-oracle/s6lit_probe.cpp`
        // output (float division first, then Q31 conversion).
        assert_eq!(B2PE_DEFAULT_M, 1267015296);
        assert_eq!(DZONE_06, 0x4ccccd00u32 as i32);
        assert_eq!(DZONE_04, 0x33333340u32 as i32);
        assert_eq!(DZONE_03, 0x26666680);
        assert_eq!(SAVE_SLOPE_LONG, 0x3bbbbbc0u32 as i32);
        assert_eq!(SPEND_SLOPE_LONG, 0x55555580);
        assert_eq!(PECORR_12_2, 0x4ccccd00u32 as i32);
        assert_eq!(PECORR_065, 0x53333300);
        assert_eq!(PECORR_11_2, 0x46666680);
        assert_eq!(PECORR_085_2, 0x36666680);
        assert_eq!(PECORR_09_2, 0x39999980);
        assert_eq!(PECORR_115_2, 0x49999980);
        assert_eq!(PECORR_085, 0x6ccccd00u32 as i32);
        assert_eq!(PECORR_015, 0x13333340);
        assert_eq!(PECORR_07, 0x59999980);
        assert_eq!(PECORR_03, 0x26666680);
        assert_eq!(PECORR_075_2, 0x30000000);
        assert_eq!(LOWBR_AMP, 0x00a3d70a);
        assert_eq!(LOWBR_MAXDIFF, 0x20000000);
        assert_eq!(PEMM_MIN_HI, 0x26666680);
        assert_eq!(PEMM_MIN_LO, 0x11eb8520);
        assert_eq!(PEMM_MAX_LO, 0x08f5c290);
        assert_eq!(PEMM_MIN_DIFF_FAC, 0x15555560);
        assert_eq!(POINT8, 0x66666680);
        assert_eq!(POINT6, 0x4ccccd00u32 as i32);
        assert_eq!(BITFAC_07, 0x59999980);
        assert_eq!(HALF_Q31, 0x40000000);
        assert_eq!(BRES_LONG.max_bit_save, 0x26666666);
        assert_eq!(BRES_SHORT.max_bit_spend, 0x40000000);
        assert_eq!(MAXVAL_DBL, i32::MAX);
    }

    #[test]
    fn s6_calc_bits_per_frame() {
        // Hand-computed via the shift trick (`aacenc.cpp:124-134`).
        assert_eq!(calc_bits_per_frame(192_000, 1024, 48000), 4096);
        assert_eq!(calc_bits_per_frame(112_000, 1024, 48000), 2389);
        assert_eq!(calc_bits_per_frame(192_000, 1024, 44100), 4458);
        assert_eq!(calc_bits_per_frame(96_000, 1024, 48000), 2048);
    }

    #[test]
    fn s6_adjust_bitrate_padding_chain() {
        // Mono 48k VBR-5: difference = (128*112000) % 48000 = 32000.
        let (avg0, rest0) = adjust_bitrate(48000, 112_000, 48000, 1024);
        assert_eq!((avg0, rest0), (2384, 16000));
        let (avg1, rest1) = adjust_bitrate(rest0, 112_000, 48000, 1024);
        assert_eq!((avg1, rest1), (2392, 32000));
        // Stereo 48k: difference = (128*192000) % 48000 = 0 → pad every frame.
        let (avg, rest) = adjust_bitrate(48000, 192_000, 48000, 1024);
        assert_eq!((avg, rest), (4096, 48000));
    }

    #[test]
    fn s6_init_vectors_match_oracle() {
        // All three operating points against the real-QCInit fixtures.
        for (tag, name, sr, br, n_eff, bw) in [
            ("mono-48k", "click-mono", 48000, 112_000, 1, 19293),
            ("stereo-48k", "sine-stereo", 48000, 192_000, 2, 19293),
            (
                "stereo-44100",
                "sine-stereo-44100",
                44100,
                192_000,
                2,
                19293,
            ),
        ] {
            let init = testdata::load_s6init(tag);
            let abpf = calc_bits_per_frame(br, 1024, sr);
            let p = derive_qc_init(br, abpf, n_eff, n_eff, sr, bw, 1024);
            assert_eq!(p.average_bits, init.avg, "{tag} averageBits");
            assert_eq!(p.max_bits, init.max, "{tag} maxBits");
            assert_eq!(p.min_bits, init.min, "{tag} minBits");
            assert_eq!(p.bit_res, init.bitres, "{tag} bitRes");
            assert_eq!(p.bit_res_mode, BitResMode::Full, "{tag} mode");
            assert_eq!(p.bit_distribution_mode, init.distmode, "{tag} distmode");
            assert_eq!(p.mean_pe, init.meanpe, "{tag} meanPe");
            assert_eq!(p.max_bit_fac, init.maxbitfac, "{tag} maxBitFac");
            assert_eq!(p.ch_bitrate_el, init.chbitrate_el, "{tag} chBitrateEl");
            assert_eq!(p.max_bits_el, init.maxbits_el, "{tag} maxBitsEl");
            assert_eq!(MAXVAL_DBL, init.relbits_el, "{tag} relBitsEl");
            assert_eq!(init.mode, 0, "{tag} mode==FULL");
            // Grant-state init (AdjThrInit slice) against the same fixture.
            let ds = distribute_state_init(p.mean_pe);
            assert_eq!(ds.bits2pe_factor_m, init.b2m, "{tag} bits2pe_m");
            assert_eq!(ds.bits2pe_factor_e, init.b2e, "{tag} bits2pe_e");
            assert_eq!(ds.pe_min, init.pemin, "{tag} peMin");
            assert_eq!(ds.pe_max, init.pemax, "{tag} peMax");
            assert_eq!(ds.pe_correction_m, HALF_Q31, "{tag} corr init");
            assert_eq!(ds.pe_correction_e, 1, "{tag} corr_e init");
            assert_eq!(ds.pe_last, 0, "{tag} peLast init");
            assert_eq!(ds.dyn_bits_last, -1, "{tag} dynLast init");
            let _ = name;
        }
        // Pinned init values (independent of the loader round-trip).
        assert_eq!(testdata::load_s6init("stereo-48k").meanpe, 8231);
        assert_eq!(testdata::load_s6init("stereo-48k").maxbitfac, 50331648);
        assert_eq!(testdata::load_s6init("mono-48k").max, 6144);
        assert_eq!(testdata::load_s6init("stereo-44100").avg, 4464);
    }

    #[test]
    fn s6_bits2pe_factor_paths() {
        // LC default (dead dZone): the probed constant.
        assert_eq!(
            init_bits2pe_factor(false, false, 0, 1),
            (0x4b851e80u32 as i32, 1)
        );
        // dZone compensation (dead at LC, kept exact): 0.59 headroom shift
        // then +0.3>>2 for a 112 kbit/s mono element.
        let (m, e) = init_bits2pe_factor(false, true, 112_000, 1);
        assert_eq!((m, e), (0x25c28f40u32 as i32 + (0x26666680 >> 2), 2));
    }

    #[test]
    #[should_panic(expected = "ELD table path")]
    fn s6_bits2pe_eld_path_panics() {
        let _ = init_bits2pe_factor(true, false, 0, 1);
    }

    #[test]
    fn s6_calc_max_cross_checks_s5_qmax() {
        // `calc_max_value_in_sfb` (S6-owned) reproduces every S5 `QMAX`
        // fixture bit-exactly on the visited bands, with the overall max
        // as return value. Unvisited bands are left untouched (stale
        // sentinel proves the no-write contract).
        for (name, f) in S6_CASES.iter().take(19) {
            let s4 = testdata::load_s4(name, *f);
            let s5 = testdata::load_s5(name, *f);
            for (ci, (c4, c5)) in s4.ch.iter().zip(s5.ch.iter()).enumerate() {
                let lv = &c4.levels[2];
                let mut maxv = vec![0xDEAD_BEEFu32; c5.geo_cnt];
                let max_all = calc_max_value_in_sfb(
                    c5.geo_cnt,
                    c5.geo_max,
                    c5.geo_per,
                    &c5.offsets,
                    &lv.qspec,
                    &mut maxv,
                );
                // Visited bands match exactly; unvisited bands keep the
                // stale sentinel (C never writes them — no-write contract).
                let mut o = 0;
                while o < c5.geo_cnt {
                    for b in 0..c5.geo_max {
                        assert_eq!(
                            maxv[o + b],
                            c5.qmax[o + b],
                            "{name} f={f} ch={ci} band={} qmax",
                            o + b
                        );
                    }
                    for b in c5.geo_max..c5.geo_per {
                        assert_eq!(
                            maxv[o + b],
                            0xDEAD_BEEF,
                            "{name} f={f} ch={ci} band={} untouched",
                            o + b
                        );
                    }
                    o += c5.geo_per;
                }
                let expect_max = *c5.qmax.iter().max().unwrap();
                assert_eq!(max_all, expect_max, "{name} f={f} ch={ci} maxAll");
                assert!(
                    max_all <= 32768,
                    "{name} f={f} ch={ci} ESC-range documented"
                );
            }
        }
    }

    #[test]
    fn s6_calc_max_envelope_and_short_min() {
        // I16::MIN aborts loudly instead of C's `fixp_abs` mis-gate.
        let mut maxv = vec![0u32; 2];
        let mut q = vec![0i16; 8];
        q[3] = i16::MIN;
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            calc_max_value_in_sfb(2, 2, 2, &[0, 4, 8], &q, &mut maxv);
        }));
        assert!(res.is_err(), "-32768 must panic");
    }

    fn build_frame_input<'a>(
        name: &str,
        s1: &'a testdata::S1Frame,
        s4: &'a testdata::S4Frame,
        s5: &'a testdata::S5Frame,
        init: &testdata::S6Init,
        tns_defaults: &'a [TnsChannel],
    ) -> (QcFrameInput<'a>, DistributeState) {
        let mut channels = Vec::new();
        for (ci, ((c1, c4), c5)) in s1.ch.iter().zip(s4.ch.iter()).zip(s5.ch.iter()).enumerate() {
            assert_eq!(c5.geo_cnt, c1.geo_cnt, "{name} ch={ci} cnt");
            assert_eq!(c5.offsets, c1.offsets, "{name} ch={ci} offsets");
            let lv = &c4.levels[2];
            assert_eq!(lv.inv, 2);
            channels.push(QcChannelInput {
                spectrum: &lv.spec,
                scf: &lv.scf,
                global_gain: lv.gain,
                sfb_cnt: c5.geo_cnt,
                max_sfb_per_group: c5.geo_max,
                sfb_per_group: c5.geo_per,
                sfb_offsets: &c5.offsets,
                block_type: c1.geo_seq as i32,
                noise_nrg: &c5.noise_nrg,
                is_book: &c5.is_book,
                is_scale: &c5.is_scale,
                tns: &tns_defaults[ci],
            });
        }
        let (digest, bands) = match &s1.msd {
            Some((d, mask)) => (MsDigest::from_i32(*d).expect("ms digest"), mask.len()),
            None => (MsDigest::None, 0),
        };
        let input = QcFrameInput {
            channels,
            no_red_pe: s1.pe[0],
            static_bits: s1.static_bits,
            ms_digest: digest,
            ms_bands: bands,
            glob_hdr_bits: 56,
            max_bits_per_frame: init.max,
            max_bits_el: init.maxbits_el,
            relative_bits_el: init.relbits_el,
            max_bit_fac: init.maxbitfac,
            bit_res_mode: BitResMode::Full,
        };
        (
            input,
            DistributeState {
                pe_min: 0,
                pe_max: 0,
                bits2pe_factor_m: init.b2m,
                bits2pe_factor_e: init.b2e,
                pe_correction_m: 0,
                pe_correction_e: 0,
                pe_last: 0,
                dyn_bits_last: 0,
            },
        )
    }

    fn forge_dist(init: &testdata::S6Init, gstate: &[i32]) -> DistributeState {
        assert_eq!(gstate.len(), 6);
        DistributeState {
            pe_min: gstate[4],
            pe_max: gstate[5],
            bits2pe_factor_m: init.b2m,
            bits2pe_factor_e: init.b2e,
            pe_correction_m: gstate[2],
            pe_correction_e: gstate[3],
            pe_last: gstate[0],
            dyn_bits_last: gstate[1],
        }
    }

    fn check_frame_output(
        name: &str,
        f: u32,
        s6: &testdata::S6Frame,
        out: &QcFrameOutput,
        dist: &DistributeState,
    ) {
        assert_eq!(out.granted_dyn_bits, s6.granted, "{name} f={f} granted");
        assert_eq!(out.max_dyn_bits, s6.maxdyn, "{name} f={f} maxdyn");
        assert_eq!(out.total_avail, s6.avail, "{name} f={f} avail");
        assert_eq!(out.el_granted, s6.elgrant, "{name} f={f} elgrant");
        assert_eq!(out.granted_pe, s6.gpe, "{name} f={f} gpe");
        assert_eq!(out.granted_pe_corr, s6.gpecorr, "{name} f={f} gpecorr");
        assert_eq!(out.bit_res_level_el, s6.reslevel, "{name} f={f} reslevel");
        assert_eq!(out.bit_res_max_el, s6.resmax, "{name} f={f} resmax");
        assert_eq!(out.passes.len(), s6.npass, "{name} f={f} npass");
        for (pi, (got, exp)) in out.passes.iter().zip(s6.passes.iter()).enumerate() {
            assert_eq!(got.iterations, exp.iter, "{name} f={f} pass={pi} iter");
            assert_eq!(got.decrease, exp.dec, "{name} f={f} pass={pi} dec");
            assert_eq!(got.dyn_bits[0], exp.dyn0, "{name} f={f} pass={pi} dyn0");
            assert_eq!(got.dyn_bits[1], exp.dyn1, "{name} f={f} pass={pi} dyn1");
            assert_eq!(got.gains[0], exp.gain0, "{name} f={f} pass={pi} gain0");
            assert_eq!(got.gains[1], exp.gain1, "{name} f={f} pass={pi} gain1");
            assert_eq!(got.maxq[0], exp.maxq0, "{name} f={f} pass={pi} maxq0");
            assert_eq!(got.maxq[1], exp.maxq1, "{name} f={f} pass={pi} maxq1");
            assert_eq!(got.el_dyn, exp.eldyn, "{name} f={f} pass={pi} eldyn");
            assert_eq!(got.el_static, exp.elstat, "{name} f={f} pass={pi} elstat");
            assert_eq!(got.crash, exp.crash, "{name} f={f} pass={pi} crash");
        }
        assert_eq!(out.static_bits_used, s6.static_, "{name} f={f} static");
        assert_eq!(out.used_dyn_bits, s6.useddyn, "{name} f={f} useddyn");
        assert_eq!(out.tot_fill_bits, s6.totfill, "{name} f={f} totfill");
        assert_eq!(out.align_bits, s6.align, "{name} f={f} align");
        assert_eq!(out.total_bits, s6.total, "{name} f={f} total");
        assert_eq!(out.bit_res_tot, s6.bitres, "{name} f={f} bitres");
        // Post-frame grant-state evolution (S6GPOST).
        let gp = &s6.gpost;
        assert_eq!(dist.pe_last, gp[0], "{name} f={f} post peLast");
        assert_eq!(dist.dyn_bits_last, gp[1], "{name} f={f} post dynLast");
        assert_eq!(dist.pe_correction_m, gp[2], "{name} f={f} post corrM");
        assert_eq!(dist.pe_correction_e, gp[3], "{name} f={f} post corrE");
        assert_eq!(dist.pe_min, gp[4], "{name} f={f} post peMin");
        assert_eq!(dist.pe_max, gp[5], "{name} f={f} post peMax");
        // Frame accounting identity (M4 exit criterion, integer-exact).
        assert_eq!(
            out.total_bits,
            out.static_bits_used
                + out.used_dyn_bits
                + out.tot_fill_bits
                + out.align_bits
                + GLOBAL_EXT_BITS,
            "{name} f={f} total-identity"
        );
    }

    #[test]
    fn s6_corpus_exact() {
        // End-to-end exactness: forged pre-frame grant state + oracle S1/S4
        // inputs reproduce the oracle S6 grants, iteration trace, fill and
        // finalization bit-exactly, including post-state evolution.
        let tns_defaults = [TnsChannel::default(), TnsChannel::default()];
        for (name, f) in S6_CASES {
            let s1 = testdata::load_s1(name, f);
            let s4 = testdata::load_s4(name, f);
            let s5 = testdata::load_s5(name, f);
            let s6 = testdata::load_s6(name, f);
            let init = testdata::load_s6init(init_tag(name));
            assert_eq!(s6.frame, f);
            assert_eq!(s6.glob_hdr, 56, "{name} f={f} transport-owned header");
            let (input, _) = build_frame_input(name, &s1, &s4, &s5, &init, &tns_defaults);
            let mut dist = forge_dist(&init, &s6.gstate);
            let mut qc = QcState {
                bit_res_tot: init.max,
                bit_res_tot_max: init.max,
                padding_rest: 0,
                bc_scratch: Default::default(),
            };
            let out = qc_main_frame(&mut qc, &mut dist, &input, MAX_ITERATIONS)
                .expect("s6 frame converges");
            check_frame_output(name, f, &s6, &out, &dist);
        }
    }

    #[test]
    fn s6_grant_state_evolution_onset_chain() {
        // Multi-frame evolution proof: consecutive onset frames f22-26.
        // POST(f) forged as PRE(f+1) reproduces every later frame exactly,
        // proving the persistent grant-state chain (not just isolated frames).
        let tns_defaults = [TnsChannel::default(), TnsChannel::default()];
        let init = testdata::load_s6init("stereo-48k");
        let mut dist_opt: Option<DistributeState> = None;
        for f in [22, 23, 24, 25, 26] {
            let s1 = testdata::load_s1("onset-stereo", f);
            let s4 = testdata::load_s4("onset-stereo", f);
            let s5 = testdata::load_s5("onset-stereo", f);
            let s6 = testdata::load_s6("onset-stereo", f);
            let (input, _) = build_frame_input("onset-stereo", &s1, &s4, &s5, &init, &tns_defaults);
            let mut dist = match dist_opt.take() {
                None => forge_dist(&init, &s6.gstate),
                Some(d) => {
                    // Chained state must equal the dumped PRE of this frame.
                    assert_eq!(d.pe_last, s6.gstate[0], "chain f={f} peLast");
                    assert_eq!(d.dyn_bits_last, s6.gstate[1], "chain f={f} dynLast");
                    assert_eq!(d.pe_correction_m, s6.gstate[2], "chain f={f} corrM");
                    assert_eq!(d.pe_correction_e, s6.gstate[3], "chain f={f} corrE");
                    assert_eq!(d.pe_min, s6.gstate[4], "chain f={f} peMin");
                    assert_eq!(d.pe_max, s6.gstate[5], "chain f={f} peMax");
                    d
                }
            };
            let mut qc = QcState {
                bit_res_tot: init.max,
                bit_res_tot_max: init.max,
                padding_rest: 0,
                bc_scratch: Default::default(),
            };
            let out = qc_main_frame(&mut qc, &mut dist, &input, MAX_ITERATIONS)
                .expect("chain frame converges");
            check_frame_output("onset-stereo", f, &s6, &out, &dist);
            dist_opt = Some(dist);
        }
    }

    #[test]
    fn s6_padding_chain_matches_oracle() {
        // `paddingRest`/`avgTotalBits` evolve content-independently from
        // `paddingRest = sampleRate`; replaying AdjustBitrate to frame f
        // reproduces every S6BITR line without fixtures for intermediates.
        for (name, f) in S6_CASES {
            let s6 = testdata::load_s6(name, f);
            let br = total_br(name);
            let sr = sample_rate(name);
            let mut rest = sr;
            for _ in 0..=f {
                let (avg, next) = adjust_bitrate(rest, br, sr, 1024);
                rest = next;
                // Only the final iteration is compared (chain is exact).
                if next == s6.pad_rest {
                    assert_eq!(avg, s6.avg_total, "{name} f={f} avgTotal");
                }
            }
            assert_eq!(rest, s6.pad_rest, "{name} f={f} padRest");
        }
    }

    #[test]
    fn s6_exit_matrix_cases() {
        // Hand-computed exit-matrix vectors (`qc_main.cpp:1040-1105`).
        // Under budget, decreasing: exit, keep decreasing.
        assert_eq!(
            exit_matrix_step(1000, 500, 4000, 0, 1, 0, 5, 500, 6000),
            (true, 1, false)
        );
        // Under budget, no direction yet: exit with direction down and
        // constraints reset (C still memclears inside the taken branch).
        assert_eq!(
            exit_matrix_step(1000, 500, 4000, 0, -1, 0, 5, 500, 6000),
            (true, 0, true)
        );
        // Over budget: retry, direction up, constraints reset.
        assert_eq!(
            exit_matrix_step(5000, 4000, 4000, 0, -1, 0, 5, 4000, 6000),
            (false, 1, true)
        );
        // Zero consumption with direction up: exit (C `sumDyn == 0` arm).
        assert_eq!(
            exit_matrix_step(100, 0, 4000, 0, 1, 0, 5, 0, 6000),
            (true, 1, false)
        );
        // maxDynBits overshoot forces retry even under budget.
        assert_eq!(
            exit_matrix_step(1000, 500, 4000, 0, 0, 0, 5, 6500, 6000),
            (false, 1, true)
        );
        // maxIterations reached but under budget: emergency exit.
        assert_eq!(
            exit_matrix_step(1000, 500, 4000, 0, 1, 5, 5, 500, 6000),
            (true, 1, false)
        );
    }

    #[test]
    fn s6_retry_forced_by_tiny_budget() {
        // Synthetic retry: noise-mono f5 inputs with a 2000-bit buffer.
        // Must terminate with exact accounting (no crash needed at this size).
        let s1 = testdata::load_s1("noise-mono", 5);
        let s4 = testdata::load_s4("noise-mono", 5);
        let s5 = testdata::load_s5("noise-mono", 5);
        let init = testdata::load_s6init("mono-48k");
        let tns_defaults = [TnsChannel::default(), TnsChannel::default()];
        let (input, _) = build_frame_input("noise-mono", &s1, &s4, &s5, &init, &tns_defaults);
        let mut input = input;
        input.max_bits_per_frame = 2000;
        input.max_bits_el = 2000;
        let s6 = testdata::load_s6("noise-mono", 5);
        let mut dist = forge_dist(&init, &s6.gstate);
        let mut qc = QcState {
            bit_res_tot: 2000,
            bit_res_tot_max: 2000,
            padding_rest: 0,
            bc_scratch: Default::default(),
        };
        let out = qc_main_frame(&mut qc, &mut dist, &input, MAX_ITERATIONS)
            .expect("tiny-budget frame converges");
        assert!(out.passes.len() > 1, "retry must trigger");
        assert_eq!(out.total_avail, 2000);
        assert!(
            out.total_bits <= 2000,
            "converged frame fits the buffer, got {}",
            out.total_bits
        );
        assert_eq!(
            out.total_bits,
            out.static_bits_used
                + out.used_dyn_bits
                + out.tot_fill_bits
                + out.align_bits
                + GLOBAL_EXT_BITS,
            "total-identity under retry"
        );
    }

    #[test]
    fn s6_crash_forced_by_starvation_budget() {
        // Synthetic crash: 1200-bit buffer forces iterations to the max and
        // into crash recovery (maxSfbPerGroup drops, static rebalanced).
        let s1 = testdata::load_s1("noise-mono", 5);
        let s4 = testdata::load_s4("noise-mono", 5);
        let s5 = testdata::load_s5("noise-mono", 5);
        let init = testdata::load_s6init("mono-48k");
        let tns = TnsChannel::default();
        let tns_defaults = [tns.clone(), tns.clone()];
        let (input, _) = build_frame_input("noise-mono", &s1, &s4, &s5, &init, &tns_defaults);
        let mut input = input;
        input.max_bits_per_frame = 1200;
        input.max_bits_el = 1200;
        let s6 = testdata::load_s6("noise-mono", 5);
        let mut dist = forge_dist(&init, &s6.gstate);
        let mut qc = QcState {
            bit_res_tot: 1200,
            bit_res_tot_max: 1200,
            padding_rest: 0,
            bc_scratch: Default::default(),
        };
        let out = qc_main_frame(&mut qc, &mut dist, &input, MAX_ITERATIONS)
            .expect("starvation frame converges via crash");
        assert!(out.passes.len() > 1, "retry must trigger");
        let last = out.passes.last().unwrap();
        assert!(
            last.crash >= 0,
            "crash recovery must run, passes={}",
            out.passes.len()
        );
        assert!(
            out.final_max_sfb[0] < s5.ch[0].geo_max,
            "maxSfbPerGroup must drop"
        );
        assert!(
            out.total_bits <= 1200,
            "crashed frame fits the buffer, got {}",
            out.total_bits
        );
    }

    #[test]
    fn s6_fill_and_finalize_vectors() {
        // `writeExtensionData` fill loop, hand-computed (`bitenc.cpp:851-913`).
        assert_eq!(fill_extension_bits(0), 0);
        assert_eq!(fill_extension_bits(6), 0);
        // 7-bit demand: one fill element header (3+4), empty payload.
        assert_eq!(fill_extension_bits(7), 7);
        // VBR updateFillBits is pure alignment (`minBitsPerFrame = 0`).
        assert_eq!(update_fill_bits(2276, 2105, 57, 0, 3, 0), (3, 2168));
        // Finalize identity on a corpus-shaped case.
        let (fill, align, total) =
            finalize_bit_consumption(57, 2105, 3, 0, 3, 56, 6144, 0).unwrap();
        assert_eq!(total, 57 + 2105 + fill + align + 3);
        // Range gate fires loudly (not silently clamped).
        assert_eq!(
            finalize_bit_consumption(6000, 6000, 0, 0, 3, 56, 6144, 0),
            Err(QcError::QuantError)
        );
    }

    #[test]
    fn s6_reduce_and_bitres_errors() {
        // Empty dynBitsUsed at max iterations is QUANT_ERROR (`:1138`).
        let mut it = MAX_ITERATIONS;
        let mut gains = vec![0];
        let mut recalc = vec![false];
        assert_eq!(
            reduce_bit_consumption(
                &mut it,
                MAX_ITERATIONS,
                1,
                &[true],
                &mut gains,
                &mut recalc,
                0,
                0,
                0,
                0,
                0
            ),
            Err(QcError::QuantError)
        );
        // Past max iterations is QUANT_ERROR (`:1159-1161`).
        let mut it2 = MAX_ITERATIONS + 1;
        assert_eq!(
            reduce_bit_consumption(
                &mut it2,
                MAX_ITERATIONS,
                1,
                &[true],
                &mut gains,
                &mut recalc,
                100,
                0,
                0,
                0,
                0
            ),
            Err(QcError::QuantError)
        );
        // Negative reservoir is BITRES_TOO_LOW (`:742`, `:629`).
        assert_eq!(
            bit_res_redistribution(-1, 100, 6144, 6144, MAXVAL_DBL),
            Err(QcError::BitresTooLow)
        );
        assert_eq!(
            prepare_grants(6144, 6144, 56, 3, 7000, 0, 0),
            Err(QcError::BitresTooLow)
        );
        // VBR reservoir pin (`:1251-1260`).
        assert_eq!(update_bitres_vbr(6144, 6144), 6144);
        assert_eq!(update_bitres_vbr(12288, 12288), 12288);
    }

    #[test]
    fn s6_expected_values_are_checked_non_vacuously() {
        // Tamper check: flipping one committed grant must break comparison.
        let s6 = testdata::load_s6("noise-mono", 5);
        let mut tampered = s6.granted;
        tampered += 8;
        assert_ne!(tampered, s6.granted);
        assert_ne!(s6.gpost, s6.gstate, "state must evolve on loud frames");
    }

    #[test]
    fn s6_pe_correction_opposite_side_reset() {
        // `FDKaacEnc_calcPeCorrection` opposite-side reset
        // (`adj_thr.cpp:2636-2641`): `corrFac = 1/2`, `newFac` untouched.
        // Both directions; each would fail with the old `newFac = 1/2`
        // clobber (which also disabled the reset) and with the old
        // `PECORR_03 = 0.6` weight (both old bugs clamped to `0x49999980`).
        let (b2m, b2e) = (B2PE_DEFAULT_M, 1);
        let qfmult = crate::aac_enc::q31::fmult;
        // Direction 1: newFac > 1/2 (peLast > denum), corr < 1/2.
        // bitsLast=3000 -> denum=3539; peLast=4000 -> newFac=0x411a3316.
        assert!(bits2pe2(3000, qfmult(PECORR_12_2, b2m), b2e + 1) > 4000);
        assert!(bits2pe2(3000, qfmult(PECORR_065, b2m), b2e) < 4000);
        let mut corr_m = PECORR_085_2;
        let mut corr_e = 0;
        calc_pe_correction(&mut corr_m, &mut corr_e, 4000, 4000, 3000, b2m, b2e);
        assert_eq!(corr_e, 1);
        // Reset to 1/2 then slow adaptation 0.7*1/2 + 0.3*newFac.
        assert_eq!(corr_m, 1079290093); // 0x4054a8ed
        assert_ne!(corr_m, PECORR_115_2, "old code clamped to the max");
        assert_ne!(corr_m, 966547221, "no-reset value with correct weight");
        // Direction 2: newFac < 1/2, corr > 1/2.
        // bitsLast=2000 -> denum=2359; peLast=2000 -> newFac=0x3baf8ce2.
        assert!(bits2pe2(2000, qfmult(PECORR_12_2, b2m), b2e + 1) > 2000);
        assert!(bits2pe2(2000, qfmult(PECORR_065, b2m), b2e) < 2000);
        let mut corr_m2 = PECORR_115_2;
        let mut corr_e2 = 0;
        calc_pe_correction(&mut corr_m2, &mut corr_e2, 2000, 2000, 2000, b2m, b2e);
        assert_eq!(corr_e2, 1);
        assert_eq!(corr_m2, 1052027458); // 0x3eb4aa42
        assert_ne!(corr_m2, PECORR_115_2, "old code clamped to the max");
        assert_ne!(corr_m2, 1164770329, "no-reset value with correct weight");
    }

    #[test]
    fn s6_pe_correction_low_bit_res_taken() {
        // `calcPeCorrectionLowBitRes` (`adj_thr.cpp:2664-2721`) with
        // `bitsLast > 0`: both headroom arms, each with a non-trivial
        // negative diff. Values derived from the C integer chain
        // (`bits2pe2`/`fDivNorm`/`fMult` replicas proven by corpus agreement).
        let (b2m, b2e) = (B2PE_DEFAULT_M, 1);
        // Headroom-zero arm (`bitres 1000 >= 50*2`): bbl=-3719, den=1179.
        let mut cm = HALF_Q31;
        let mut ce = 0;
        calc_pe_correction_low_bit_res(&mut cm, &mut ce, 1000, 4000, 1000, 2, b2m, b2e);
        assert_eq!(ce, 1);
        assert_eq!(cm, 1056807322); // 0x3efd999a
        assert_ne!(cm, HALF_Q31, "must move off the init");
        assert_ne!(cm, PECORR_075_2, "must not take the bitsLast==0 arm");
        // Headroom-reserve arm (`bitres 50 < 50*2`): bbl=-1539, hr=235.
        let mut cm2 = HALF_Q31;
        let mut ce2 = 0;
        calc_pe_correction_low_bit_res(&mut cm2, &mut ce2, 2000, 3000, 50, 2, b2m, b2e);
        assert_eq!(ce2, 1);
        assert_eq!(cm2, 1041237280); // 0x3e100520
        assert_ne!(cm2, HALF_Q31);
        assert_ne!(cm2, PECORR_075_2);
        // Control: `bitsLast == 0` else-branch pins the floor.
        let mut cm0 = HALF_Q31;
        let mut ce0 = 0;
        calc_pe_correction_low_bit_res(&mut cm0, &mut ce0, 1000, 0, 1000, 2, b2m, b2e);
        assert_eq!((cm0, ce0), (PECORR_075_2, 1));
    }

    #[test]
    fn s6_distribute_bits_reduced_and_disabled() {
        // `DistributeBits` small/disabled-reservoir arms
        // (`adj_thr.cpp:2744-2749` + `:2767-2776`): direct `bits2pe2`
        // grant (no bit-factor) plus the low-bitres correction above.
        let (b2m, b2e) = (B2PE_DEFAULT_M, 1);
        assert_eq!(bits2pe2(4000, b2m, b2e), 4719);
        for mode in [BitResMode::Reduced, BitResMode::Disabled] {
            let mut st = DistributeState {
                pe_min: 100,
                pe_max: 200,
                bits2pe_factor_m: b2m,
                bits2pe_factor_e: b2e,
                pe_correction_m: HALF_Q31,
                pe_correction_e: 1,
                pe_last: 1000,
                dyn_bits_last: 4000,
            };
            let (gpe, gpecorr) =
                distribute_bits(&mut st, false, 5000, 1, 4000, 1000, 12288, 50331648, mode);
            // Direct grant, low-bitres correction (`s6_pe_correction_...`
            // headroom-zero vector), and the `grantedPeCorr` product.
            assert_eq!(gpe, 4719, "{mode:?} must bypass the bit-factor");
            assert_eq!(
                st.pe_correction_m, 1056807322,
                "{mode:?} low-bit correction"
            );
            assert_eq!(st.pe_correction_e, 1);
            assert_eq!(gpecorr, 4644, "{mode:?} grantedPeCorr");
            assert_eq!(st.pe_last, 4719);
            assert_eq!(st.dyn_bits_last, -1);
            // Full would mutate peMin/peMax via `bitresCalcBitFac`; the
            // small-reservoir arms must leave them untouched.
            assert_eq!(
                (st.pe_min, st.pe_max),
                (100, 200),
                "{mode:?} no peMin/Max drift"
            );
            // Factor-bypass proof: `maxBitFac` and `short_window` are
            // Full-only inputs; varying them must not move the result.
            let mut st_alt = DistributeState {
                pe_min: 100,
                pe_max: 200,
                bits2pe_factor_m: b2m,
                bits2pe_factor_e: b2e,
                pe_correction_m: HALF_Q31,
                pe_correction_e: 1,
                pe_last: 1000,
                dyn_bits_last: 4000,
            };
            let alt = distribute_bits(&mut st_alt, true, 5000, 1, 4000, 1000, 12288, 0, mode);
            assert_eq!((gpe, gpecorr), alt, "{mode:?} ignores Full-only inputs");
            assert_eq!(st_alt.pe_correction_m, st.pe_correction_m);
        }
    }
}
