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
//! M4-S8: channel-element assembly and complete AAC frame output.
//!
//! Exact-bit mirror of the write-mode half of
//! `FDKaacEnc_ChannelElementWrite` (`cpp/libAACenc/src/bitenc.cpp:918-1178`,
//! `hTpEnc != NULL`, `qcOutChannel != NULL`, `minCnt == 0`) plus the stripped
//! `FDKaacEnc_WriteBitstream` tail (`bitenc.cpp:1182-1322`) for the supported
//! configuration, with ADTS framing via the existing [`super::adts`] writer.
//!
//! ## S8 contract (ownership disambiguation, plan section 18.7)
//!
//! - S7 owns the dynamic-payload leaves (`codeValues`/`codeScalefactorDelta`
//!   plus `encodeSpectralData`/`encodeSectionData`/`encodeScaleFactorData`);
//!   S8 reuses them by call and hosts the runtime `WRITE_SEC/SCAL/SPEC_ERROR`
//!   agreement checks (`bitenc.cpp:1049-1122`).
//! - S1 owns the NULL-mode static-bit counts; S8 owns the corresponding write
//!   paths (element tag, ICS, MS, global-gain, pulse, TNS scaffolding) and
//!   proves `emitted == counted` on every fixture.
//! - S5 owns `SectionData` and the dynamic counts; S6 owns grants, the gain
//!   loop, fill/finalize accounting (`totalBits`/`alignBits`) and the writer
//!   preconditions (plan Q8). S8 consumes those values without re-deciding
//!   them: no psychoacoustics, quantization, bit allocation or threshold
//!   change lives here.
//! - Transport: ADTS header bits (56) are transport-owned (`tpenc_lib.cpp`,
//!   `tpenc_adts.cpp`); the header bytes themselves come from the existing
//!   [`super::adts::encode_header`]. RAW is the bare `raw_data_block()` bytes.
//!   S8 never invents a new framing.
//!
//! ## Reference audit (all at base `1785529`)
//!
//! - Element IDs (`cpp/libSYS/include/FDK_audio.h:423-431`): `ID_SCE = 0`,
//!   `ID_CPE = 1`, `ID_LFE = 3`, `ID_FIL = 6`, `ID_END = 7`.
//! - Extension types (`FDK_audio.h:470-471`): `EXT_FIL = 0`, `EXT_FILL_DATA = 1`.
//! - Element lists (`cpp/libFDK/src/FDK_tools_rom.cpp:6547-6602`):
//!   `el_aac_sce` (tag, gain, ics, section, scale, pulse, tns-present, tns,
//!   gain-present, spectral) and `el_aac_cpe`/`el_aac_cpe0`/`el_aac_cpe1`
//!   (common-window 1 shares one ICS + MS, then per-channel payloads;
//!   common-window 0 repeats ICS per channel). LFE shares the SCE list
//!   (`getBitstreamElementList` returns `node_aac_sce` for 1 channel,
//!   `node_aac_cpe` for 2, `FDK_tools_rom.cpp:7133`).
//! - `encodeIcsInfo` (`bitenc.cpp:180-227`): short blocks `8 + TRANS_FAC - 1`
//!   (15) bits, else 11 on the stripped path (`syntaxFlags == 0`,
//!   non-ELD/non-scalable). Write order: reserved(1, 0) + blockType(2) +
//!   windowShape(1, `LOL_WINDOW` maps to `KBD_WINDOW`) then long/start/stop:
//!   maxSfb(6) + predictor(1, 0); short: maxSfb(4) + groupingMask(7).
//! - `encodeGlobalGain` (`:158-168`): 8-bit
//!   `globalGain - scf[first] + 100 - 4 * (LOG_NORM_PCM - mdctScale)` with
//!   `globalGainOffset = 100`, `LOG_NORM_PCM = -15` (`psy_const.h:161`).
//! - `encodeMSInfo` (`:380-423`): digest 2 bits (`SI_MS_MASK_* = digest`,
//!   `psy_const.h:139`) plus one mask bit per band on `MS_SOME`
//!   (`(sfbCnt/sfbPerGroup) * maxSfb` trips, channel-0 geometry).
//! - `encodeTnsDataPresent` (`:434-454`, always 1 bit) and `encodeTnsData`
//!   (`:465-578`): per-window nf bits (1 short / 2 long) plus per-filter
//!   res/length/order/direction/compression/coef words; coef masking via
//!   `rmask[coefBits]` (`:525-529`); `order > 12` asserts in C.
//! - `encodePulseData` (`:605-610`, 1 bit 0) and `encodeGainControlData`
//!   (`:589-594`, 1 bit 0) are dummies on this path.
//! - `ChannelElementWrite` write mode (`:918-1178`): EL_ID (3) then the list
//!   traversal above; section/scale/spectral go through the S7 writers with
//!   the `WRITE_*_ERROR` comparisons against `SECTION_DATA` counts.
//! - `WriteBitstream` (`:1182-1322`): single-element loop (SCE/CPE/LFE only;
//!   any other `elType` is `AAC_ENC_INVALID_ELEMENTINFO_TYPE`), element
//!   extensions (none on fixtures), one global `EXT_FILL_DATA` extension
//!   sized by `writeExtensionData`, `ID_END` (3), byte alignment with
//!   `alignBits`, and the `frameBits == totalBits + globHdrBits` check
//!   (`:1317-1319`).
//! - Fill packing (`:621-713,809-902`): outer `ID_FIL` (3) + cnt (4, +8 esc)
//!   loop (`MAX_FILL_DATA_BYTES = 269`, `qc_main.cpp` counting) with the
//!   `cnt >= EXT_TYPE_BITS (4)` payload gate, inner `EXT_FILL_DATA` (4) +
//!   fill-nibble (4, 0) + `0xA5` bytes; the counting half is S6's
//!   `fill_extension_bits` (single table source, plan Q6).
//! - ADTS (`tpenc_adts.cpp:129-237`, `tpenc_lib.cpp:317-406`): 56 header bits
//!   (no CRC, single raw block), `frame_length` counts the header itself,
//!   VBR fullness `0x7FF`. The CRC list entries in the element tables are
//!   no-ops under `protection_absent` (`tpenc_adts.cpp:108-124`).
//!
//! ## Supported configuration (anything else is an explicit error)
//!
//! - `AOT_AAC_LC` (2), `syntaxFlags == 0`, `epConfig == -1`, `minCnt == 0`.
//! - Single element per frame: SCE (mono) or CPE (stereo, `common_window`
//!   0/1); LFE counts/writes like SCE with `ID_LFE`. Multi-element,
//!   CCE/DSE/PCE, ER/scalable/ELD, `useVCB11`/HCR and element extensions are
//!   refused explicitly (never silent fallback).
//! - Block types LONG/START/SHORT/STOP (0..3); window shapes SINE/KBD/LOL
//!   (LOL maps to KBD on write, like C); TNS orders `<= 12`.
//!
//! ## I/O and safety
//!
//! All inputs are integers; emitted bits are counted exactly. C divergences
//! handled loudly: out-of-range IDs/tags/shapes/blocks/gains/books/widths,
//! `order > 12`, oversized write values (C masks), buffer overflow (C has no
//! bounds check) and any `WRITE_*` count mismatch are explicit [`FrameError`]s.
//! TNS coefficient words mask low bits exactly like C (`& rmask[coefBits]`)
//! since the selector guarantees the width; the selector inputs themselves
//! (`order`, `length`, `direction`, `coefRes`) are range-checked.

use super::adts;
use super::bitcount::{SectionData, BLOCK_LONG, BLOCK_SHORT, BLOCK_START, BLOCK_STOP};
use super::bitenc::{
    encode_scale_factor_data, encode_section_data, encode_spectral_data, BitEncError, BitWriter,
};
use super::channel_map::ChannelMode;
use super::static_bits::MsDigest;
use super::tns::TnsChannel;
use core::fmt;

/// Element IDs (`FDK_audio.h:423-431`).
pub const ID_SCE: u32 = 0;
/// Channel-pair element ID.
pub const ID_CPE: u32 = 1;
/// LFE element ID (shares the SCE list).
pub const ID_LFE: u32 = 3;
/// Fill element ID (outer fill headers).
pub const ID_FIL: u32 = 6;
/// End element ID (raw-block terminator, the S6 `globalExtBits = 3`).
pub const ID_END: u32 = 7;
/// Element-ID field width (`bitenc.h:126`, `EL_ID_BITS`).
pub const EL_ID_BITS: u8 = 3;
/// Instance-tag width (`bitenc.cpp:1003`).
pub const INSTANCE_TAG_BITS: u8 = 4;
/// Common-window flag width (`bitenc.cpp:1012`).
pub const COMMON_WINDOW_BITS: u8 = 1;
/// Global-gain field width (`encodeGlobalGain`, `:158-168`).
pub const GLOBAL_GAIN_BITS: u8 = 8;
/// Pulse/gain-control present widths (dummy single zero bits).
pub const PULSE_BITS: u8 = 1;
/// TNS-present flag width.
pub const TNS_PRESENT_BITS: u8 = 1;
/// Gain-control-present width.
pub const GAIN_CONTROL_BITS: u8 = 1;
/// `globalGainOffset` / `noiseOffset` (`bitenc.cpp:114-116`).
pub const GLOBAL_GAIN_OFFSET: i32 = 100;
/// `LOG_NORM_PCM` (`psy_const.h:161`).
pub const LOG_NORM_PCM: i32 = -15;
/// Window shapes (`psy_const.h:131-133`).
#[cfg(test)]
pub const SINE_WINDOW: i32 = 0;
/// Kaiser-Bessel derived window.
pub const KBD_WINDOW: i32 = 1;
/// Low-overlap window (maps to KBD on write, `bitenc.cpp:201`).
pub const LOL_WINDOW: i32 = 2;
/// Fill outer widths (`bitenc.cpp:815-816`).
pub const FILL_EL_COUNT_BITS: u8 = 4;
/// Fill escape-count width.
pub const FILL_EL_ESC_COUNT_BITS: u8 = 8;
/// Largest fill chunk per element (`bitenc.cpp:817`).
pub const MAX_FILL_DATA_BYTES: i32 = 269;
/// Extension-type width (`bitenc.cpp:625`).
pub const EXT_TYPE_BITS: u8 = 4;
/// Fill-nibble width (`bitenc.cpp:627`).
pub const FILL_NIBBLE_BITS: u8 = 4;
/// Fill byte for `EXT_FILL_DATA` (`bitenc.cpp:693`).
pub const FILL_DATA_BYTE: u32 = 0xA5;
/// Extension types (`FDK_audio.h:470-471`).
#[cfg(test)]
pub const EXT_FIL: u32 = 0x00;
/// Fill-data extension type (the only global extension S8 emits).
pub const EXT_FILL_DATA: u32 = 0x01;
/// Supported audio object type (stripped LC build only).
pub const AOT_AAC_LC: i32 = 2;
/// Maximum TNS order (C `FDK_ASSERT(order <= 12)`, `bitenc.cpp:495`).
pub const TNS_MAX_ORDER: usize = 12;

/// S8 failures. Every C silent/UB case (masking, OOB reads, silent no-ops,
/// unbounded buffer) is an explicit error here; valid inputs are unaffected
/// (proven by the exact-byte corpus tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// Unsupported element type (only SCE/CPE/LFE; C returns
    /// `AAC_ENC_INVALID_ELEMENTINFO_TYPE` for the rest).
    UnsupportedElement(u32),
    /// Channel count does not match the element type.
    ChannelCountMismatch { el: u32, channels: usize },
    /// More than one element per frame (multi-element is out of scope).
    MultiElementUnsupported(usize),
    /// Unsupported AOT / syntax flags / epConfig / minCnt.
    UnsupportedSyntax(&'static str),
    /// Instance tag does not fit 4 bits.
    BadInstanceTag(i32),
    /// Block type outside LONG/START/SHORT/STOP.
    BadBlockType(i32),
    /// Window shape outside SINE/KBD/LOL.
    BadWindowShape(i32),
    /// Grouping mask does not fit 7 bits (short blocks).
    BadGroupingMask(i32),
    /// `maxSfbPerGroup` does not fit its field (6 long / 4 short).
    BadMaxSfb(i32),
    /// Global-gain word does not fit 8 bits (C masks silently).
    GainOutOfRange(i32),
    /// MS digest outside NONE/SOME/ALL.
    BadMsDigest(i32),
    /// MS mask too short for the written trip count.
    MsMaskTooShort { need: usize, have: usize },
    /// TNS order above the C assert (`> 12`).
    TnsOrderOutOfRange(usize),
    /// TNS length does not fit its field (6 long / 4 short).
    TnsLengthOutOfRange(usize),
    /// TNS direction is not a single bit.
    TnsDirectionOutOfRange(i32),
    /// TNS resolution not 3/4 on an active window.
    TnsCoefResOutOfRange(usize),
    /// Too many TNS filters (`> 2`, the S1 counting bound).
    TnsTooManyFilters(usize),
    /// Underlying S7 encode failure (bad book/width/value/delta/overflow).
    BitEnc(BitEncError),
    /// `WRITE_SEC_ERROR`: emitted section bits differ from `side_info_bits`.
    SectionMismatch { emitted: u32, counted: i32 },
    /// `WRITE_SCAL_ERROR`: emitted scf bits differ from
    /// `scalefacBits + noiseNrgBits`.
    ScaleMismatch { emitted: u32, counted: i32 },
    /// `WRITE_SPEC_ERROR`: emitted spectral bits differ from `huffmanBits`.
    SpectralMismatch { emitted: u32, counted: i32 },
    /// Static scaffolding emitted a different count than S1's NULL count.
    StaticMismatch { emitted: u32, counted: i32 },
    /// Fill payload emitted a different count than S6's sized fill.
    FillMismatch { emitted: u32, counted: i32 },
    /// Raw block is not byte-aligned before ADTS framing.
    UnalignedRawBlock(usize),
    /// Total/frame-length or bounds check failed.
    BadFrameLength(&'static str),
    /// ADTS header failure (rate/sample/channel/oversize).
    Adts(adts::AdtsError),
    /// Bit-buffer overflow (C has no bounds check).
    BufferOverflow { need: usize, free: usize },
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            FrameError::UnsupportedElement(el) => write!(f, "unsupported element type: {el}"),
            FrameError::ChannelCountMismatch { el, channels } => {
                write!(f, "element {el} with {channels} channels")
            }
            FrameError::MultiElementUnsupported(n) => {
                write!(f, "multi-element frames unsupported: {n}")
            }
            FrameError::UnsupportedSyntax(what) => write!(f, "unsupported syntax: {what}"),
            FrameError::BadInstanceTag(t) => write!(f, "instance tag out of range: {t}"),
            FrameError::BadBlockType(b) => write!(f, "block type out of range: {b}"),
            FrameError::BadWindowShape(s) => write!(f, "window shape out of range: {s}"),
            FrameError::BadGroupingMask(m) => write!(f, "grouping mask out of range: {m}"),
            FrameError::BadMaxSfb(m) => write!(f, "maxSfbPerGroup out of range: {m}"),
            FrameError::GainOutOfRange(g) => write!(f, "global-gain word out of range: {g}"),
            FrameError::BadMsDigest(d) => write!(f, "MS digest out of range: {d}"),
            FrameError::MsMaskTooShort { need, have } => {
                write!(f, "MS mask too short: need {need}, have {have}")
            }
            FrameError::TnsOrderOutOfRange(o) => write!(f, "TNS order out of range: {o}"),
            FrameError::TnsLengthOutOfRange(l) => write!(f, "TNS length out of range: {l}"),
            FrameError::TnsDirectionOutOfRange(d) => {
                write!(f, "TNS direction out of range: {d}")
            }
            FrameError::TnsCoefResOutOfRange(r) => {
                write!(f, "TNS coef resolution out of range: {r}")
            }
            FrameError::TnsTooManyFilters(n) => write!(f, "too many TNS filters: {n}"),
            FrameError::BitEnc(e) => write!(f, "payload encode: {e}"),
            FrameError::SectionMismatch { emitted, counted } => {
                write!(f, "WRITE_SEC_ERROR: emitted {emitted} != counted {counted}")
            }
            FrameError::ScaleMismatch { emitted, counted } => {
                write!(
                    f,
                    "WRITE_SCAL_ERROR: emitted {emitted} != counted {counted}"
                )
            }
            FrameError::SpectralMismatch { emitted, counted } => {
                write!(
                    f,
                    "WRITE_SPEC_ERROR: emitted {emitted} != counted {counted}"
                )
            }
            FrameError::StaticMismatch { emitted, counted } => {
                write!(f, "static scaffolding {emitted} != S1 count {counted}")
            }
            FrameError::FillMismatch { emitted, counted } => {
                write!(f, "fill emitted {emitted} != S6 sized {counted}")
            }
            FrameError::UnalignedRawBlock(b) => {
                write!(f, "raw block not byte-aligned: {b} bits")
            }
            FrameError::BadFrameLength(what) => write!(f, "bad frame length: {what}"),
            FrameError::Adts(e) => write!(f, "ADTS: {e}"),
            FrameError::BufferOverflow { need, free } => {
                write!(f, "bit buffer overflow: need {need}, have {free}")
            }
        }
    }
}

impl std::error::Error for FrameError {}

impl From<BitEncError> for FrameError {
    fn from(e: BitEncError) -> Self {
        match e {
            BitEncError::BufferOverflow {
                need_bits,
                free_bits,
            } => FrameError::BufferOverflow {
                need: need_bits,
                free: free_bits,
            },
            other => FrameError::BitEnc(other),
        }
    }
}

impl From<adts::AdtsError> for FrameError {
    fn from(e: adts::AdtsError) -> Self {
        FrameError::Adts(e)
    }
}

/// Per-channel S8 inputs: S5/S6 dynamic state plus the psy side info the
/// write-mode `ChannelElementWrite` reads (`psyOutChannel`).
pub struct ChannelFrameInput<'a> {
    /// `SectionData.blockType` (LONG/START/SHORT/STOP); also selects the ICS
    /// and section-escape formats.
    pub block_type: i32,
    /// `psyOutChannel.windowShape` (SINE/KBD/LOL; LOL maps to KBD on write).
    pub window_shape: i32,
    /// `psyOutChannel.groupingMask` (7 bits, only read on SHORT blocks).
    pub grouping_mask: i32,
    /// `SectionData.maxSfbPerGroup` (6 bits long / 4 bits short on write).
    pub max_sfb_per_group: i32,
    /// S5 sectioning + counts (the `WRITE_*` baselines).
    pub section_data: &'a SectionData,
    /// S4 post-inversion stored scalefactors.
    pub scf: &'a [i32],
    /// S4/S6 global gain.
    pub global_gain: i32,
    /// S4 inv=2 quantized lines.
    pub quant_spec: &'a [i16],
    /// S6 QMAX gate (`maxValueInSfb`).
    pub max_value_in_sfb: &'a [u32],
    /// Exact SFB offsets (`sfbOffsets`, `sfbCnt + 1` entries).
    pub sfb_offsets: &'a [i32],
    /// S1 PNS/IS vectors (all-zero/`NO_NOISE_PNS` on corpus; live branches).
    pub noise_nrg: &'a [i32],
    /// Intensity books (corpus-zero).
    pub is_book: &'a [i32],
    /// Intensity scales (corpus-zero).
    pub is_scale: &'a [i32],
    /// `psyOutChannel.tnsInfo` decisions.
    pub tns: &'a TnsChannel,
    /// `psyOutChannel.mdctScale` (global-gain formula input).
    pub mdct_scale: i32,
}

/// Single-element S8 inputs: the `ELEMENT_INFO` + `PSY_OUT_ELEMENT` slice
/// `ChannelElementWrite` reads in write mode.
pub struct ElementFrameInput<'a> {
    /// `ID_SCE`/`ID_CPE`/`ID_LFE` (raw IDs; LFE writes `ID_LFE`).
    pub el_type: u32,
    /// `ELEMENT_INFO.instanceTag` (0 on fixtures, 4 bits).
    pub instance_tag: i32,
    /// `PSY_OUT_ELEMENT.commonWindow` (CPE only; the encoder always sets 1).
    pub common_window: bool,
    /// `toolsInfo.msDigest` as raw 0/1/2 (CPE common-window-1 only).
    pub ms_digest: i32,
    /// `toolsInfo.msMask` bits (length `sfbCnt` of channel 0).
    pub ms_mask: &'a [i32],
    /// One (SCE/LFE) or two (CPE) channels in element order.
    pub channels: Vec<ChannelFrameInput<'a>>,
    /// S1 NULL-mode static count for this element (scaffolding agreement).
    pub static_bits_expected: i32,
}

/// Stripped `WriteBitstream` inputs for the supported single-element frame.
pub struct RawBlockInput<'a> {
    /// Exactly one element (multi-element is refused explicitly).
    pub elements: Vec<ElementFrameInput<'a>>,
    /// S6 sized fill (`totFillBits` after `finalize_bit_consumption`).
    pub tot_fill_bits: i32,
    /// S6 alignment (`alignBits` after finalize).
    pub align_bits: i32,
    /// S6 `totalBits` (static + dyn + fill + align + ext + globalExt).
    pub total_bits: i32,
    /// Transport header bits (56 for ADTS SCE/CPE; 0 for RAW).
    pub glob_hdr_bits: i32,
    /// `maxBitsPerFrame` range gate (plan Q8).
    pub max_bits_per_frame: i32,
    /// `minBitsPerFrame` range gate (0 for VBR-5).
    pub min_bits_per_frame: i32,
}

fn check_supported_syntax(
    aot: i32,
    syntax_flags: u32,
    ep_config: i32,
    min_cnt: u8,
) -> Result<(), FrameError> {
    if aot != AOT_AAC_LC {
        return Err(FrameError::UnsupportedSyntax("aot != AAC-LC"));
    }
    if syntax_flags != 0 {
        return Err(FrameError::UnsupportedSyntax("syntaxFlags != 0"));
    }
    if ep_config != -1 {
        return Err(FrameError::UnsupportedSyntax("epConfig != -1"));
    }
    if min_cnt != 0 {
        return Err(FrameError::UnsupportedSyntax("minCnt != 0"));
    }
    Ok(())
}

/// `encodeIcsInfo` write path (`bitenc.cpp:180-227`, stripped `syntaxFlags == 0`).
pub fn encode_ics_info(
    block_type: i32,
    window_shape: i32,
    grouping_mask: i32,
    max_sfb_per_group: i32,
    w: &mut BitWriter,
) -> Result<u32, FrameError> {
    if !matches!(block_type, 0..=3) {
        return Err(FrameError::BadBlockType(block_type));
    }
    if !matches!(window_shape, 0..=2) {
        return Err(FrameError::BadWindowShape(window_shape));
    }
    let short = block_type == BLOCK_SHORT;
    if short {
        if !(0..(1 << 7)).contains(&grouping_mask) {
            return Err(FrameError::BadGroupingMask(grouping_mask));
        }
        if !(0..16).contains(&max_sfb_per_group) {
            return Err(FrameError::BadMaxSfb(max_sfb_per_group));
        }
    } else if !(0..64).contains(&max_sfb_per_group) {
        return Err(FrameError::BadMaxSfb(max_sfb_per_group));
    }
    // `:198-202`: reserved + blockType + mapped window shape.
    let shape = if window_shape == LOL_WINDOW {
        KBD_WINDOW
    } else {
        window_shape
    };
    w.write_bits(0, 1)?;
    w.write_bits(block_type as u32, 2)?;
    w.write_bits(shape as u32, 1)?;
    let mut total = 4u32;
    match block_type {
        BLOCK_LONG | BLOCK_START | BLOCK_STOP => {
            w.write_bits(max_sfb_per_group as u32, 6)?;
            w.write_bits(0, 1)?; // predictor absent
            total += 7;
        }
        BLOCK_SHORT => {
            w.write_bits(max_sfb_per_group as u32, 4)?;
            w.write_bits(grouping_mask as u32, 7)?;
            total += 11;
        }
        _ => unreachable!("block range checked"),
    }
    Ok(total)
}

/// `encodeMSInfo` write path (`bitenc.cpp:380-423`).
pub fn encode_ms_info(
    sfb_cnt: i32,
    sfb_per_group: i32,
    max_sfb_per_group: i32,
    ms_digest: i32,
    ms_mask: &[i32],
    w: &mut BitWriter,
) -> Result<u32, FrameError> {
    let digest = MsDigest::from_i32(ms_digest).ok_or(FrameError::BadMsDigest(ms_digest))?;
    // Digest field doubles as the SI mask value (`psy_const.h:139`).
    w.write_bits(ms_digest as u32, 2)?;
    let mut total = 2u32;
    if digest == MsDigest::Some {
        if sfb_per_group <= 0 || sfb_cnt < 0 || max_sfb_per_group < 0 {
            return Err(FrameError::BadFrameLength("MS geometry"));
        }
        if sfb_cnt % sfb_per_group != 0 {
            return Err(FrameError::BadFrameLength("MS sfbCnt % sfbPerGroup"));
        }
        let trips = ((sfb_cnt / sfb_per_group) * max_sfb_per_group) as usize;
        // C indexes `jsFlags[sfbOff + sfb]` grouped (`:400-409`), not
        // linearly: each group of `sfbPerGroup` contributes only its first
        // `maxSfb` entries (short blocks: 13 of 14). The mask slice is
        // `sfbCnt` long (oracle `MSD`); require the full span so the
        // grouped reads are in bounds (C would OOB-read otherwise).
        if (ms_mask.len() as i32) < sfb_cnt {
            return Err(FrameError::MsMaskTooShort {
                need: sfb_cnt as usize,
                have: ms_mask.len(),
            });
        }
        // `:400-409`: grouped trips over channel-0 geometry.
        let mut k = 0usize;
        let mut off = 0i32;
        while off < sfb_cnt {
            for sfb in 0..max_sfb_per_group {
                let idx = (off + sfb) as usize;
                let bit = if ms_mask[idx] != 0 { 1 } else { 0 };
                w.write_bits(bit, 1)?;
                total += 1;
                k += 1;
            }
            off += sfb_per_group;
        }
        debug_assert_eq!(k, trips);
    }
    Ok(total)
}

/// `encodeGlobalGain` write path (`bitenc.cpp:158-168`).
pub fn encode_global_gain(
    global_gain: i32,
    scf_first: i32,
    mdct_scale: i32,
    w: &mut BitWriter,
) -> Result<u32, FrameError> {
    let v = global_gain
        .wrapping_sub(scf_first)
        .wrapping_add(GLOBAL_GAIN_OFFSET)
        .wrapping_sub(4 * (LOG_NORM_PCM - mdct_scale));
    if !(0..256).contains(&v) {
        return Err(FrameError::GainOutOfRange(v));
    }
    w.write_bits(v as u32, GLOBAL_GAIN_BITS)?;
    Ok(GLOBAL_GAIN_BITS as u32)
}

/// `encodePulseData` dummy (`bitenc.cpp:605-610`): single zero bit.
pub fn encode_pulse_data(w: &mut BitWriter) -> Result<u32, FrameError> {
    w.write_bits(0, PULSE_BITS)?;
    Ok(PULSE_BITS as u32)
}

/// `encodeGainControlData` dummy (`bitenc.cpp:589-594`): single zero bit.
pub fn encode_gain_control_data(w: &mut BitWriter) -> Result<u32, FrameError> {
    w.write_bits(0, GAIN_CONTROL_BITS)?;
    Ok(GAIN_CONTROL_BITS as u32)
}

/// `encodeTnsDataPresent` (`bitenc.cpp:434-454`): single presence bit.
pub fn encode_tns_present(
    tns: &TnsChannel,
    block_type: i32,
    w: &mut BitWriter,
) -> Result<u32, FrameError> {
    if !matches!(block_type, 0..=3) {
        return Err(FrameError::BadBlockType(block_type));
    }
    let windows = if block_type == BLOCK_SHORT { 8 } else { 1 };
    let mut present = false;
    for i in 0..windows {
        if tns.num_filters[i] != 0 {
            present = true;
        }
    }
    w.write_bits(u32::from(present), TNS_PRESENT_BITS)?;
    Ok(TNS_PRESENT_BITS as u32)
}

const TNS_RMASK: [u32; 5] = [0, 1, 3, 7, 15];

/// `encodeTnsData` write path (`bitenc.cpp:465-578`).
pub fn encode_tns_data(
    tns: &TnsChannel,
    block_type: i32,
    w: &mut BitWriter,
) -> Result<u32, FrameError> {
    if !matches!(block_type, 0..=3) {
        return Err(FrameError::BadBlockType(block_type));
    }
    let short = block_type == BLOCK_SHORT;
    let windows = if short { 8 } else { 1 };
    let mut present = false;
    for i in 0..windows {
        if tns.num_filters[i] > 2 {
            return Err(FrameError::TnsTooManyFilters(tns.num_filters[i]));
        }
        if tns.num_filters[i] != 0 {
            present = true;
        }
    }
    if !present {
        return Ok(0);
    }
    let mut total = 0u32;
    for i in 0..windows {
        let nf = tns.num_filters[i];
        // `:484-486`: 1 bit short / 2 bits long.
        w.write_bits(nf as u32, if short { 1 } else { 2 })?;
        total += if short { 1 } else { 2 };
        if nf == 0 {
            continue;
        }
        let res = tns.coef_res[i];
        if res != 3 && res != 4 {
            return Err(FrameError::TnsCoefResOutOfRange(res));
        }
        // `:488-490`.
        w.write_bits(u32::from(res == 4), 1)?;
        total += 1;
        for j in 0..nf {
            let len = tns.length[i][j];
            let order = tns.order[i][j];
            let dir = tns.direction[i][j];
            if order > TNS_MAX_ORDER {
                return Err(FrameError::TnsOrderOutOfRange(order));
            }
            if short {
                if len > 15 {
                    return Err(FrameError::TnsLengthOutOfRange(len));
                }
                w.write_bits(len as u32, 4)?;
                total += 4;
                w.write_bits(order as u32, 3)?;
                total += 3;
            } else {
                if len > 63 {
                    return Err(FrameError::TnsLengthOutOfRange(len));
                }
                w.write_bits(len as u32, 6)?;
                total += 6;
                w.write_bits(order as u32, 5)?;
                total += 5;
            }
            if order != 0 {
                if dir != 0 && dir != 1 {
                    return Err(FrameError::TnsDirectionOutOfRange(dir as i32));
                }
                w.write_bits(dir as u32, 1)?;
                total += 1;
                // `:502-520`: width selector with the exact C bounds.
                let coef_bits: usize = if res == 4 {
                    let mut cb = 3usize;
                    for k in 0..order {
                        let c = tns.coef[i][j][k];
                        if !(-4..=3).contains(&c) {
                            cb = 4;
                            break;
                        }
                    }
                    cb
                } else {
                    let mut cb = 2usize;
                    for k in 0..order {
                        let c = tns.coef[i][j][k];
                        if !(-2..=1).contains(&c) {
                            cb = 3;
                            break;
                        }
                    }
                    cb
                };
                // `:521-523`: `-(coefBits - coefRes)` is 0/1 by construction.
                let compr = -((coef_bits as i32) - (res as i32));
                debug_assert!(compr == 0 || compr == 1);
                w.write_bits(compr as u32, 1)?;
                total += 1;
                for k in 0..order {
                    let c = tns.coef[i][j][k];
                    w.write_bits((c & TNS_RMASK[coef_bits] as i32) as u32, coef_bits as u8)?;
                    total += coef_bits as u32;
                }
            }
        }
    }
    Ok(total)
}

fn encode_channel_tail(ch: &ChannelFrameInput<'_>, w: &mut BitWriter) -> Result<u32, FrameError> {
    // Order on the SCE/CPE lists after scalefactor data: pulse, TNS
    // present, TNS data, gain-control present. (`FDK_tools_rom.cpp`
    // `el_aac_sce` / `el_aac_cpe0/1`.)
    let mut total = 0u32;
    total += encode_pulse_data(w)?;
    total += encode_tns_present(ch.tns, ch.block_type, w)?;
    total += encode_tns_data(ch.tns, ch.block_type, w)?;
    total += encode_gain_control_data(w)?;
    Ok(total)
}

fn encode_channel_spectral(
    ch: &ChannelFrameInput<'_>,
    w: &mut BitWriter,
) -> Result<u32, FrameError> {
    // `encodeSpectralData` (`bitenc.cpp:127-147`) via the S7 writer; the
    // `WRITE_SPEC_ERROR` agreement lives with the caller.
    let emitted = encode_spectral_data(ch.quant_spec, ch.sfb_offsets, ch.section_data, w)?;
    if emitted != ch.section_data.huffman_bits as u32 {
        return Err(FrameError::SpectralMismatch {
            emitted,
            counted: ch.section_data.huffman_bits,
        });
    }
    Ok(emitted)
}

fn encode_channel_section(
    ch: &ChannelFrameInput<'_>,
    w: &mut BitWriter,
) -> Result<u32, FrameError> {
    let emitted = encode_section_data(ch.section_data, w)?;
    if emitted != ch.section_data.side_info_bits as u32 {
        return Err(FrameError::SectionMismatch {
            emitted,
            counted: ch.section_data.side_info_bits,
        });
    }
    Ok(emitted)
}

fn encode_channel_scale(ch: &ChannelFrameInput<'_>, w: &mut BitWriter) -> Result<u32, FrameError> {
    let emitted = encode_scale_factor_data(
        ch.max_value_in_sfb,
        ch.section_data,
        ch.scf,
        ch.noise_nrg,
        ch.is_scale,
        ch.global_gain,
        w,
    )?;
    let counted = ch.section_data.scalefac_bits + ch.section_data.noise_nrg_bits;
    if emitted != counted as u32 {
        return Err(FrameError::ScaleMismatch { emitted, counted });
    }
    Ok(emitted)
}

/// Write-mode `FDKaacEnc_ChannelElementWrite` for one element
/// (`bitenc.cpp:918-1178`, `hTpEnc != NULL`, `minCnt == 0`).
///
/// Writes the exact list order from `FDK_tools_rom.cpp:6547-6602` and returns
/// the element bit demand (the `*pBitDemand` value). The CRC list entries are
/// no-ops under the supported `protection_absent` transport, like C.
pub fn channel_element_write(
    el: &ElementFrameInput<'_>,
    aot: i32,
    syntax_flags: u32,
    ep_config: i32,
    w: &mut BitWriter,
) -> Result<u32, FrameError> {
    check_supported_syntax(aot, syntax_flags, ep_config, 0)?;
    let nch = el.channels.len();
    match el.el_type {
        ID_SCE | ID_LFE if nch == 1 => {}
        ID_CPE if nch == 2 => {}
        _ => {
            if !matches!(el.el_type, ID_SCE | ID_CPE | ID_LFE) {
                return Err(FrameError::UnsupportedElement(el.el_type));
            }
            return Err(FrameError::ChannelCountMismatch {
                el: el.el_type,
                channels: nch,
            });
        }
    }
    if !(0..16).contains(&el.instance_tag) {
        return Err(FrameError::BadInstanceTag(el.instance_tag));
    }
    let start_bits = w.len_bits();
    // EL_ID (`:949-954`, skipped only under scalable/ER — never here).
    w.write_bits(el.el_type, EL_ID_BITS)?;
    if el.el_type == ID_CPE && nch != 2 {
        return Err(FrameError::ChannelCountMismatch {
            el: el.el_type,
            channels: nch,
        });
    }
    // SCE/LFE linear order (`el_aac_sce`); CPE branches on common_window
    // (`el_aac_cpe0` vs `el_aac_cpe1` via `link_sequence`).
    if el.el_type == ID_SCE || el.el_type == ID_LFE {
        let ch = &el.channels[0];
        // `element_instance_tag`.
        w.write_bits(el.instance_tag as u32, INSTANCE_TAG_BITS)?;
        // `global_gain` reads `scf[firstScf]` even when no band is coded
        // (`bitenc.cpp:307-310`); the S7 writer enforces the same seed.
        let first = ch.section_data.first_scf;
        if first < 0 || first as usize >= ch.scf.len() {
            return Err(FrameError::BadFrameLength("firstScf OOB"));
        }
        encode_global_gain(ch.global_gain, ch.scf[first as usize], ch.mdct_scale, w)?;
        encode_ics_info(
            ch.block_type,
            ch.window_shape,
            ch.grouping_mask,
            ch.max_sfb_per_group,
            w,
        )?;
        encode_channel_section(ch, w)?;
        encode_channel_scale(ch, w)?;
        encode_channel_tail(ch, w)?;
        encode_channel_spectral(ch, w)?;
    } else {
        // CPE.
        w.write_bits(el.instance_tag as u32, INSTANCE_TAG_BITS)?;
        w.write_bits(u32::from(el.common_window), COMMON_WINDOW_BITS)?;
        if el.common_window {
            let c0 = &el.channels[0];
            // Shared ICS + MS on channel-0 geometry (`el_aac_cpe1` head).
            encode_ics_info(
                c0.block_type,
                c0.window_shape,
                c0.grouping_mask,
                c0.max_sfb_per_group,
                w,
            )?;
            // MS uses channel-0 section geometry (`ChannelElementWrite`
            // passes `chSfbCnt/chSfbPerGrp/chMaxSfbPerGrp` of `ch == 0`).
            let s0 = c0.section_data;
            encode_ms_info(
                s0.sfb_cnt,
                s0.sfb_per_group,
                s0.max_sfb_per_group,
                el.ms_digest,
                el.ms_mask,
                w,
            )?;
            for ch in el.channels.iter() {
                let first = ch.section_data.first_scf;
                if first < 0 || first as usize >= ch.scf.len() {
                    return Err(FrameError::BadFrameLength("firstScf OOB"));
                }
                encode_global_gain(ch.global_gain, ch.scf[first as usize], ch.mdct_scale, w)?;
                encode_channel_section(ch, w)?;
                encode_channel_scale(ch, w)?;
                encode_channel_tail(ch, w)?;
                encode_channel_spectral(ch, w)?;
            }
        } else {
            // `el_aac_cpe0`: per-channel ICS, no MS.
            for ch in el.channels.iter() {
                let first = ch.section_data.first_scf;
                if first < 0 || first as usize >= ch.scf.len() {
                    return Err(FrameError::BadFrameLength("firstScf OOB"));
                }
                encode_global_gain(ch.global_gain, ch.scf[first as usize], ch.mdct_scale, w)?;
                encode_ics_info(
                    ch.block_type,
                    ch.window_shape,
                    ch.grouping_mask,
                    ch.max_sfb_per_group,
                    w,
                )?;
                encode_channel_section(ch, w)?;
                encode_channel_scale(ch, w)?;
                encode_channel_tail(ch, w)?;
                encode_channel_spectral(ch, w)?;
            }
        }
    }
    let emitted = (w.len_bits() - start_bits) as u32;
    Ok(emitted)
}

/// Count-mode scaffolding agreement: the write-mode static prefix must emit
/// exactly S1's NULL-mode count (element header/tag/common-window/ICS/MS/
/// global-gain/pulse/TNS/gain-control). Dynamic payloads are excluded here;
/// they are covered by the `WRITE_*` checks above.
pub fn check_static_agreement(
    emitted_element_bits: u32,
    static_expected: i32,
    dyn_bits: i32,
) -> Result<(), FrameError> {
    let static_emitted = emitted_element_bits as i32 - dyn_bits;
    if static_emitted != static_expected {
        return Err(FrameError::StaticMismatch {
            emitted: static_emitted as u32,
            counted: static_expected,
        });
    }
    Ok(())
}

/// Fill `writeExtensionData` write path for the global `EXT_FILL_DATA`
/// extension (`bitenc.cpp:851-902` with `hTpEnc != NULL`).
///
/// `payload_bits` is `QC_OUT.totFillBits` (the S6 sized value). Returns the
/// emitted fill bits, which must equal `payload_bits` on valid inputs (the
/// counting half is `rate::fill_extension_bits`, plan Q6 single source).
pub fn write_fill_extension(payload_bits: i32, w: &mut BitWriter) -> Result<u32, FrameError> {
    if payload_bits < 0 {
        return Err(FrameError::BadFrameLength("negative fill"));
    }
    if payload_bits == 0 {
        return Ok(0);
    }
    let mut payload = payload_bits;
    let mut total = 0u32;
    while payload >= (EL_ID_BITS as i32 + FILL_EL_COUNT_BITS as i32) {
        let mut cnt;
        let mut esc_count = -1i32;
        let align_bits = 0i32;
        payload -= EL_ID_BITS as i32 + FILL_EL_COUNT_BITS as i32;
        if payload >= 15 * 8 {
            payload -= FILL_EL_ESC_COUNT_BITS as i32;
            esc_count = 0;
        }
        cnt = MAX_FILL_DATA_BYTES.min((payload + align_bits) >> 3);
        if cnt >= 15 {
            esc_count = cnt - 15 + 1;
        }
        // Outer `ID_FIL` + cnt/esc (`:879-887`).
        w.write_bits(ID_FIL, EL_ID_BITS)?;
        total += EL_ID_BITS as u32;
        if esc_count >= 0 {
            w.write_bits(15, FILL_EL_COUNT_BITS)?;
            w.write_bits(esc_count as u32, FILL_EL_ESC_COUNT_BITS)?;
            total += FILL_EL_COUNT_BITS as u32 + FILL_EL_ESC_COUNT_BITS as u32;
        } else {
            if !(0..=15).contains(&cnt) {
                return Err(FrameError::BadFrameLength("fill cnt"));
            }
            w.write_bits(cnt as u32, FILL_EL_COUNT_BITS)?;
            total += FILL_EL_COUNT_BITS as u32;
        }
        cnt = (cnt * 8).min(payload);
        // `writeExtensionPayload(NULL)` gate: tails below 4 bits cost 0
        // (`bitenc.cpp:631`, S6 `fill_extension_bits` parity).
        let payload_now = cnt;
        let inner = write_fill_payload(payload_now, w)?;
        total += inner;
        payload -= payload_now;
    }
    Ok(total)
}

fn write_fill_payload(ext_payload_bits: i32, w: &mut BitWriter) -> Result<u32, FrameError> {
    // `writeExtensionPayload` FIL branch (`bitenc.cpp:692-708`).
    if ext_payload_bits < EXT_TYPE_BITS as i32 {
        return Ok(0);
    }
    w.write_bits(EXT_FILL_DATA, EXT_TYPE_BITS)?;
    let mut total = EXT_TYPE_BITS as u32;
    // `EXT_FILL_DATA` => 0xA5 bytes, `EXT_FIL` => 0x00 (we only emit the former).
    w.write_bits(0x00, FILL_NIBBLE_BITS)?;
    total += FILL_NIBBLE_BITS as u32;
    let mut write_bits = ext_payload_bits;
    write_bits -= 8; // type + nibble
    while write_bits >= 8 {
        w.write_bits(FILL_DATA_BYTE, 8)?;
        total += 8;
        write_bits -= 8;
    }
    // Trailing sub-byte tail contributes 0 payload bits beyond the nibble
    // formula `4 + (cnt & ~7) - 8` (`:707`); the outer sizing already
    // accounts for it via `fill_extension_bits`.
    Ok(total)
}

/// Stripped `FDKaacEnc_WriteBitstream` (`bitenc.cpp:1182-1322`) for one
/// single-element frame: element + global fill + `ID_END` + alignment.
///
/// Returns the raw-data-block bits (== `total_bits` on success).
pub fn write_raw_block(input: &RawBlockInput<'_>, w: &mut BitWriter) -> Result<u32, FrameError> {
    check_supported_syntax(AOT_AAC_LC, 0, -1, 0)?;
    if input.elements.len() != 1 {
        return Err(FrameError::MultiElementUnsupported(input.elements.len()));
    }
    let el = &input.elements[0];
    let anchor = w.len_bits();
    let el_bits = channel_element_write(el, AOT_AAC_LC, 0, -1, w)?;
    // Element extensions: none on the supported path (S6 `elementExtBits = 0`).
    // Any nonzero expectation is refused rather than silently dropped.
    // (The `RawBlockInput` carries no per-element extension payloads.)
    let fill_emitted = write_fill_extension(input.tot_fill_bits, w)?;
    if fill_emitted != input.tot_fill_bits as u32 {
        return Err(FrameError::FillMismatch {
            emitted: fill_emitted,
            counted: input.tot_fill_bits,
        });
    }
    // `ID_END` (`:1299-1301`, skipped only under scalable/ER).
    w.write_bits(ID_END, EL_ID_BITS)?;
    // Byte alignment (`:1303-1310`): the `& 7` check then zero padding.
    let raw_so_far = (w.len_bits() - anchor) as u32;
    let expect_without_align = el_bits + fill_emitted + EL_ID_BITS as u32;
    if raw_so_far != expect_without_align {
        return Err(FrameError::BadFrameLength("element+fill+END tally"));
    }
    if input.align_bits < 0 || input.align_bits > 7 {
        return Err(FrameError::BadFrameLength("alignBits range"));
    }
    w.write_bits(0, input.align_bits as u8)?;
    let raw_bits = (w.len_bits() - anchor) as u32;
    if raw_bits != input.total_bits as u32 {
        return Err(FrameError::BadFrameLength("raw != S6 totalBits"));
    }
    if raw_bits % 8 != 0 {
        return Err(FrameError::UnalignedRawBlock(raw_bits as usize));
    }
    // Range gate (plan Q8 writer precondition, S6 `finalize` parity).
    if (input.total_bits > input.max_bits_per_frame)
        || (input.total_bits < input.min_bits_per_frame)
    {
        return Err(FrameError::BadFrameLength("totalBits range"));
    }
    Ok(raw_bits)
}

/// Complete raw-data-block bytes for one frame (byte-aligned).
pub fn build_raw_block(input: &RawBlockInput<'_>) -> Result<Vec<u8>, FrameError> {
    // Worst case: total + small headroom; bounded like S7 (`BitWriter` is
    // bounded, C has no check).
    if input.total_bits < 0 || input.total_bits > 8 * 8192 {
        return Err(FrameError::BadFrameLength("totalBits bounds"));
    }
    let cap = (input.total_bits as usize).div_ceil(8) + 16;
    let mut w = BitWriter::new(cap);
    let raw = write_raw_block(input, &mut w)?;
    if raw as usize != w.len_bits() {
        return Err(FrameError::BadFrameLength("writer tally"));
    }
    Ok(w.used_bytes().to_vec())
}

/// Complete ADTS frame: 7-byte header plus raw-data-block bytes.
///
/// `channel_mode`/`sample_rate` select the header (`adts::encode_header`,
/// VBR fullness `0x7FF`); `raw` must already be byte-aligned. The header
/// `frame_length` counts itself, so `adts_len == raw.len() + 7`.
pub fn build_adts_frame(
    channel_mode: ChannelMode,
    sample_rate: u32,
    raw: &[u8],
) -> Result<Vec<u8>, FrameError> {
    if raw.is_empty() {
        return Err(FrameError::BadFrameLength("empty raw block"));
    }
    let hdr = adts::encode_header(channel_mode, sample_rate, raw.len())?;
    let mut out = Vec::with_capacity(hdr.len() + raw.len());
    out.extend_from_slice(&hdr);
    out.extend_from_slice(raw);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::bitcount::{SectionData, BLOCK_LONG, BLOCK_SHORT};
    use crate::aac_enc::bitenc::BitWriter;
    use crate::aac_enc::tns::TnsChannel;

    fn blank_tns() -> TnsChannel {
        TnsChannel::new()
    }

    fn empty_section(block: i32) -> SectionData {
        SectionData {
            block_type: block,
            no_of_groups: 1,
            sfb_cnt: 1,
            max_sfb_per_group: 1,
            sfb_per_group: 1,
            sections: Vec::new(),
            side_info_bits: 0,
            huffman_bits: 0,
            scalefac_bits: 0,
            noise_nrg_bits: 0,
            first_scf: 0,
        }
    }

    #[test]
    fn s8_constants_match_reference() {
        assert_eq!((ID_SCE, ID_CPE, ID_LFE, ID_FIL, ID_END), (0, 1, 3, 6, 7));
        assert_eq!(EL_ID_BITS, 3);
        assert_eq!((EXT_FIL, EXT_FILL_DATA), (0x00, 0x01));
        assert_eq!((GLOBAL_GAIN_OFFSET, LOG_NORM_PCM), (100, -15));
        assert_eq!((SINE_WINDOW, KBD_WINDOW, LOL_WINDOW), (0, 1, 2));
        assert_eq!(adts::ADTS_HEADER_BITS, 56);
    }

    #[test]
    fn s8_ics_info_widths_and_values() {
        // Long: 1+2+1+6+1 = 11 bits.
        let mut w = BitWriter::new(8);
        let b = encode_ics_info(BLOCK_LONG, SINE_WINDOW, 0, 21, &mut w).unwrap();
        assert_eq!(b, 11);
        assert_eq!(w.len_bits(), 11);
        // Reserved 0, block 0, shape 0, maxSfb 21, predictor 0.
        // Bits: 0 | 00 | 0 | 010101 | 0 = 00000101010 (+5 pad) = 0x05 0x40.
        assert_eq!(w.used_bytes(), &[0x05, 0x40]);
        // Short: 1+2+1+4+7 = 15 bits.
        let mut w = BitWriter::new(8);
        let b = encode_ics_info(BLOCK_SHORT, KBD_WINDOW, 0b1010101, 13, &mut w).unwrap();
        assert_eq!(b, 15);
        // LOL maps to KBD on write.
        let mut a = BitWriter::new(8);
        let mut b_ = BitWriter::new(8);
        encode_ics_info(BLOCK_LONG, LOL_WINDOW, 0, 21, &mut a).unwrap();
        encode_ics_info(BLOCK_LONG, KBD_WINDOW, 0, 21, &mut b_).unwrap();
        assert_eq!(a.used_bytes(), b_.used_bytes());
        // Rejections.
        let mut w = BitWriter::new(8);
        assert_eq!(
            encode_ics_info(4, 0, 0, 1, &mut w),
            Err(FrameError::BadBlockType(4))
        );
        assert_eq!(
            encode_ics_info(0, 5, 0, 1, &mut w),
            Err(FrameError::BadWindowShape(5))
        );
        assert_eq!(
            encode_ics_info(BLOCK_SHORT, 0, 128, 1, &mut w),
            Err(FrameError::BadGroupingMask(128))
        );
        assert_eq!(
            encode_ics_info(BLOCK_SHORT, 0, 0, 16, &mut w),
            Err(FrameError::BadMaxSfb(16))
        );
    }

    #[test]
    fn s8_global_gain_formula() {
        // gain 100, scfFirst 60, mdctScale 0: 100-60+100-4*(-15-0)=200.
        let mut w = BitWriter::new(8);
        let b = encode_global_gain(100, 60, 0, &mut w).unwrap();
        assert_eq!(b, 8);
        assert_eq!(w.used_bytes(), &[200]);
        // mdctScale shifts by 4 per unit.
        let mut a = BitWriter::new(8);
        let mut b_ = BitWriter::new(8);
        encode_global_gain(100, 60, 1, &mut a).unwrap();
        encode_global_gain(100, 60, 0, &mut b_).unwrap();
        assert_eq!(a.used_bytes()[0], b_.used_bytes()[0].wrapping_add(4));
        // Out of 8-bit range fails (C masks).
        let mut w = BitWriter::new(8);
        assert_eq!(
            encode_global_gain(1000, 0, 0, &mut w),
            Err(FrameError::GainOutOfRange(1160))
        );
    }

    #[test]
    fn s8_ms_some_costs_mask_bits() {
        let mut a = BitWriter::new(16);
        let mut b_ = BitWriter::new(16);
        let mask = vec![1, 0, 1, 0];
        let am = encode_ms_info(4, 4, 4, 1, &mask, &mut a).unwrap();
        let bm = encode_ms_info(4, 4, 4, 0, &mask, &mut b_).unwrap();
        assert_eq!(am - bm, 4);
        assert_eq!(bm, 2);
        // Short mask fails loudly (C would OOB-read jsFlags).
        let mut w = BitWriter::new(16);
        assert_eq!(
            encode_ms_info(4, 4, 4, 1, &[1, 0], &mut w),
            Err(FrameError::MsMaskTooShort { need: 4, have: 2 })
        );
        assert_eq!(
            encode_ms_info(1, 1, 1, 9, &[], &mut w),
            Err(FrameError::BadMsDigest(9))
        );
        // Grouped indexing (short blocks: 13 of 14 per group): the 14th,
        // 28th, ... entries are never read (`bitenc.cpp:400-409`).
        // cnt=28, per=14, max=13: trips 26, mask span 28.
        let mut g1 = BitWriter::new(32);
        let mut g2 = BitWriter::new(32);
        let mut base = vec![0i32; 28];
        for i in 0..28 {
            base[i] = (i % 2) as i32;
        }
        let mut skipped = base.clone();
        // Flip only the skipped positions (13, 27): grouped writes must not change.
        skipped[13] = 1 - skipped[13];
        skipped[27] = 1 - skipped[27];
        let b1 = encode_ms_info(28, 14, 13, 1, &base, &mut g1).unwrap();
        let b2 = encode_ms_info(28, 14, 13, 1, &skipped, &mut g2).unwrap();
        assert_eq!(b1, 2 + 26);
        assert_eq!(b2, b1);
        assert_eq!(g1.used_bytes(), g2.used_bytes());
    }

    #[test]
    fn s8_tns_round_trip_matches_static_count() {
        // Silence TNS: present 0 + no payload.
        let tns = blank_tns();
        let mut w = BitWriter::new(8);
        assert_eq!(encode_tns_present(&tns, BLOCK_LONG, &mut w).unwrap(), 1);
        assert_eq!(encode_tns_data(&tns, BLOCK_LONG, &mut w).unwrap(), 0);
        assert_eq!(w.len_bits(), 1);
        // Order > 12 is refused (C asserts).
        let mut bad = blank_tns();
        bad.num_filters[0] = 1;
        bad.coef_res[0] = 4;
        bad.order[0] = [13, 0];
        let mut w = BitWriter::new(32);
        w.write_bits(0, 0).unwrap();
        assert_eq!(
            encode_tns_data(&bad, BLOCK_LONG, &mut w),
            Err(FrameError::TnsOrderOutOfRange(13))
        );
    }

    #[test]
    fn s8_static_agreement_accepts_post_crash_static() {
        // Track-09 frame-7303 regression numbers (stereo `MS_SOME`,
        // `maxSfbPerGroup` 47 -> 45): the S8 write emits 11982 element bits
        // with 11844 dynamic bits, i.e. 138 static bits. The pre-crash S1
        // count (140, MS mask under 47 bands) must fail; the post-crash
        // recount (138, mask under 45 bands) must pass. This pins the
        // exact discrepancy the crash-recovery recount repair resolves.
        assert_eq!(
            check_static_agreement(11982, 140, 11844),
            Err(FrameError::StaticMismatch {
                emitted: 138,
                counted: 140
            })
        );
        assert!(check_static_agreement(11982, 138, 11844).is_ok());
    }

    #[test]
    fn s8_rejects_unsupported_syntax_and_elements() {
        let ch = ChannelFrameInput {
            block_type: BLOCK_LONG,
            window_shape: 0,
            grouping_mask: 0,
            max_sfb_per_group: 1,
            section_data: &empty_section(BLOCK_LONG),
            scf: &[0],
            global_gain: 100,
            quant_spec: &[0],
            max_value_in_sfb: &[0],
            sfb_offsets: &[0, 1],
            noise_nrg: &[0],
            is_book: &[0],
            is_scale: &[0],
            tns: &blank_tns(),
            mdct_scale: 0,
        };
        // This dance avoids lifetime unification for the two shapes below.
        let el_sce = ElementFrameInput {
            el_type: 99,
            instance_tag: 0,
            common_window: true,
            ms_digest: 0,
            ms_mask: &[],
            channels: Vec::new(),
            static_bits_expected: 0,
        };
        let mut w = BitWriter::new(64);
        assert_eq!(
            channel_element_write(&el_sce, AOT_AAC_LC, 0, -1, &mut w),
            Err(FrameError::UnsupportedElement(99))
        );
        assert_eq!(
            channel_element_write(
                &ElementFrameInput {
                    el_type: ID_SCE,
                    instance_tag: 0,
                    common_window: true,
                    ms_digest: 0,
                    ms_mask: &[],
                    channels: Vec::new(),
                    static_bits_expected: 0,
                },
                AOT_AAC_LC,
                1,
                -1,
                &mut w
            ),
            Err(FrameError::UnsupportedSyntax("syntaxFlags != 0"))
        );
        let _ = ch;
    }

    #[test]
    fn s8_fill_sizing_matches_s6_counter() {
        // VBR-5 corpus fills are 0 or 7; the writer must agree with S6's
        // `fill_extension_bits` exactly there (and on small probes).
        for payload in [0, 7] {
            let mut w = BitWriter::new(16);
            let emitted = write_fill_extension(payload, &mut w).unwrap();
            let counted = crate::aac_enc::rate::fill_extension_bits(payload);
            assert_eq!(emitted, counted as u32, "payload {payload}");
            assert_eq!(emitted as usize, w.len_bits());
        }
        assert_eq!(write_fill_extension(0, &mut BitWriter::new(16)).unwrap(), 0);
    }

    fn tns_from_s1(
        tns: &[crate::aac_enc::testdata::TnsWindow],
        coef_res: &[i32],
        short_block: bool,
    ) -> TnsChannel {
        let mut out = TnsChannel::new();
        let windows = if short_block { 8 } else { 1 };
        assert_eq!(tns.len(), windows);
        assert_eq!(coef_res.len(), windows);
        for (w, tw) in tns.iter().enumerate() {
            out.num_filters[w] = tw.num_filters as usize;
            out.coef_res[w] = coef_res[w] as usize;
            for j in 0..2 {
                out.length[w][j] = tw.length[j] as usize;
                out.order[w][j] = tw.order[j] as usize;
                out.direction[w][j] = tw.direction[j] as usize;
            }
            for k in 0..12 {
                out.coef[w][0][k] = tw.coef_hi[k];
                out.coef[w][1][k] = tw.coef_lo[k];
            }
        }
        out
    }

    #[test]
    fn s8_corpus_elements_match_c_bit_exact() {
        // End-to-end S8 over the committed corpus (20 element-frames, 36
        // channels): S1 TNS/geometry/MS/static + S4 inv=2 lines/scf/gain +
        // S5 sections/geometry + S6 fill/align/total + S8 side info drive
        // the write-mode element assembly; outputs must equal the oracle
        // S8ELEM/S8RAW hex bit-for-bit AND the live S1/S5/S6 counts
        // (the WRITE_SEC/SCAL/SPEC + static + fill + total agreements).
        const S8_CASES: [(&str, u32); 20] = [
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
        use crate::aac_enc::bitcount::{dyn_bit_count, BitCountInput};
        for (name, frame) in S8_CASES {
            let tag = format!("{name} f={frame}");
            let s1 = crate::aac_enc::testdata::load_s1(name, frame);
            let s4 = crate::aac_enc::testdata::load_s4(name, frame);
            let s5 = crate::aac_enc::testdata::load_s5(name, frame);
            let s6 = crate::aac_enc::testdata::load_s6(name, frame);
            let s8 = crate::aac_enc::testdata::load_s8(name, frame);
            assert_eq!(s1.ch.len(), s5.ch.len(), "{tag} channels");
            assert_eq!(s1.ch.len(), s4.ch.len(), "{tag} channels");
            assert_eq!(s1.ch.len(), s8.sides.len(), "{tag} channels");
            // S8FINAL must reproduce S6FINAL (same fill/align/total source).
            assert_eq!(s8.sstatic, s6.static_, "{tag} static");
            assert_eq!(s8.useddyn, s6.useddyn, "{tag} useddyn");
            assert_eq!(s8.totfill, s6.totfill, "{tag} totfill");
            assert_eq!(s8.align, s6.align, "{tag} align");
            assert_eq!(s8.total, s6.total, "{tag} total");
            // Build live sections (never fixture replay for the agreement).
            let mut live_sections = Vec::new();
            for ((c4, c5), c1) in s4.ch.iter().zip(s5.ch.iter()).zip(s1.ch.iter()) {
                let level = c4
                    .levels
                    .iter()
                    .find(|l| l.inv == 2)
                    .unwrap_or_else(|| panic!("{tag} inv=2 level"));
                assert_eq!(c1.geo_cnt, c5.geo_cnt, "{tag} cnt");
                assert_eq!(c1.geo_per, c5.geo_per, "{tag} per");
                assert_eq!(c1.geo_max, c5.geo_max, "{tag} max");
                assert_eq!(c1.offsets, c5.offsets, "{tag} offsets");
                let input = BitCountInput {
                    quant_spec: &level.qspec,
                    max_value_in_sfb: &c5.qmax,
                    scf: &level.scf,
                    block_type: c5.block,
                    sfb_cnt: c5.geo_cnt,
                    max_sfb_per_group: c5.geo_max,
                    sfb_per_group: c5.geo_per,
                    sfb_offsets: &c5.offsets,
                    noise_nrg: &c5.noise_nrg,
                    is_book: &c5.is_book,
                    is_scale: &c5.is_scale,
                    use_vcb11: false,
                };
                let (sections, _) = dyn_bit_count(&input);
                assert_eq!(sections.side_info_bits, c5.side, "{tag} side");
                assert_eq!(sections.huffman_bits, c5.huff, "{tag} huff");
                assert_eq!(sections.scalefac_bits, c5.scf_bits, "{tag} scf");
                assert_eq!(sections.noise_nrg_bits, c5.noise_bits, "{tag} noise");
                assert_eq!(sections.first_scf, c5.first, "{tag} first");
                live_sections.push((sections, level));
            }
            // Assemble element inputs with S8 side info + S1 TNS/MS.
            let is_stereo = s1.ch.len() == 2;
            let (ms_digest, ms_mask_vec) = if is_stereo {
                let (d, m) = s1.msd.as_ref().expect("MSD");
                (*d, m.clone())
            } else {
                (0, Vec::new())
            };
            // TNS channels must be held alive across the element build.
            let tns_held: Vec<TnsChannel> = s1
                .ch
                .iter()
                .map(|c1| {
                    let short = c1.geo_seq == 2;
                    tns_from_s1(&c1.tns, &c1.tns_coef_res, short)
                })
                .collect();
            // Live SectionData must outlive the channel inputs.
            let sections_held: Vec<SectionData> =
                live_sections.into_iter().map(|(s, _)| s).collect();
            let levels_held: Vec<(Vec<i16>, Vec<i32>, i32)> = s4
                .ch
                .iter()
                .map(|c4| {
                    let l = c4.levels.iter().find(|l| l.inv == 2).unwrap();
                    (l.qspec.clone(), l.scf.clone(), l.gain)
                })
                .collect();
            let mut ch_inputs = Vec::new();
            for (idx, c5) in s5.ch.iter().enumerate() {
                let side = &s8.sides[idx];
                ch_inputs.push(ChannelFrameInput {
                    block_type: c5.block,
                    window_shape: side.wshape,
                    grouping_mask: side.gmask,
                    max_sfb_per_group: c5.geo_max as i32,
                    section_data: &sections_held[idx],
                    scf: &levels_held[idx].1,
                    global_gain: levels_held[idx].2,
                    quant_spec: &levels_held[idx].0,
                    max_value_in_sfb: &c5.qmax,
                    sfb_offsets: &c5.offsets,
                    noise_nrg: &c5.noise_nrg,
                    is_book: &c5.is_book,
                    is_scale: &c5.is_scale,
                    tns: &tns_held[idx],
                    mdct_scale: side.mdct,
                });
            }
            let el_type = if is_stereo { ID_CPE } else { ID_SCE };
            let el = ElementFrameInput {
                el_type,
                instance_tag: 0,
                common_window: true,
                ms_digest,
                ms_mask: &ms_mask_vec,
                channels: ch_inputs,
                static_bits_expected: s1.static_bits,
            };
            // Element assembly (level 1: side info + level 2: payloads).
            let mut w_el = BitWriter::new(8192);
            let el_bits = channel_element_write(&el, AOT_AAC_LC, 0, -1, &mut w_el)
                .unwrap_or_else(|e| panic!("{tag} element: {e}"));
            assert_eq!(el_bits, s8.elem_bits, "{tag} S8ELEM bits");
            assert_eq!(&w_el.to_hex(), &s8.elem_hex, "{tag} S8ELEM hex");
            // Static-scaffolding agreement (emitted minus dyn == S1 count).
            check_static_agreement(el_bits, s1.static_bits, s6.useddyn)
                .unwrap_or_else(|e| panic!("{tag} static: {e}"));
            // Complete raw block (level 3: access unit).
            let max_bits = if is_stereo { 12288 } else { 6144 };
            let raw_input = RawBlockInput {
                elements: vec![ElementFrameInput {
                    el_type,
                    instance_tag: 0,
                    common_window: true,
                    ms_digest,
                    ms_mask: &ms_mask_vec,
                    channels: {
                        let mut v = Vec::new();
                        for (idx, c5) in s5.ch.iter().enumerate() {
                            let side = &s8.sides[idx];
                            v.push(ChannelFrameInput {
                                block_type: c5.block,
                                window_shape: side.wshape,
                                grouping_mask: side.gmask,
                                max_sfb_per_group: c5.geo_max as i32,
                                section_data: &sections_held[idx],
                                scf: &levels_held[idx].1,
                                global_gain: levels_held[idx].2,
                                quant_spec: &levels_held[idx].0,
                                max_value_in_sfb: &c5.qmax,
                                sfb_offsets: &c5.offsets,
                                noise_nrg: &c5.noise_nrg,
                                is_book: &c5.is_book,
                                is_scale: &c5.is_scale,
                                tns: &tns_held[idx],
                                mdct_scale: side.mdct,
                            });
                        }
                        v
                    },
                    static_bits_expected: s1.static_bits,
                }],
                tot_fill_bits: s6.totfill,
                align_bits: s6.align,
                total_bits: s6.total,
                glob_hdr_bits: s6.glob_hdr,
                max_bits_per_frame: max_bits,
                min_bits_per_frame: 0,
            };
            let raw_bytes =
                build_raw_block(&raw_input).unwrap_or_else(|e| panic!("{tag} raw: {e}"));
            assert_eq!(
                (raw_bytes.len() * 8) as u32,
                s8.raw_bits,
                "{tag} S8RAW bits"
            );
            let mut hex = String::with_capacity(raw_bytes.len() * 2);
            for b in &raw_bytes {
                hex.push(char::from_digit((b >> 4) as u32, 16).unwrap());
                hex.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
            }
            assert_eq!(&hex, &s8.raw_hex, "{tag} S8RAW hex");
            // ADTS framing (level 4): header + raw, length + fullness.
            let mode = if is_stereo {
                ChannelMode::Stereo
            } else {
                ChannelMode::Mono
            };
            let sr = if name.contains("44100") { 44100 } else { 48000 };
            let adts = build_adts_frame(mode, sr, &raw_bytes)
                .unwrap_or_else(|e| panic!("{tag} adts: {e}"));
            assert_eq!(adts.len(), raw_bytes.len() + 7, "{tag} ADTS len");
            assert_eq!(
                (adts.len() * 8) as i32,
                s6.total + s6.glob_hdr,
                "{tag} ADTS bits"
            );
        }
    }
}
