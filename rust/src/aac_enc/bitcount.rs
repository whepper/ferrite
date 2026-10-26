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
//! M4-S5 noiseless bit counting and sectioning (exact integers).
//!
//! Exact integer mirror of `cpp/libAACenc/src/dyn_bits.cpp` (all static
//! helpers + exported `FDKaacEnc_dynBitCount`) and the counting half of
//! `cpp/libAACenc/src/bit_cnt.cpp` (`FDKaacEnc_bitCount`,
//! `FDKaacEnc_countValues` and the six `count*` kernels), at base commit
//! `1785529`. Huffman length tables and side-info tables live in
//! [`bitcount_tables`] (script-extracted, probe-vs-source cross-checked);
//! the scf-DPCM lengths reuse [`super::pe::bit_count_scf_delta`] (no
//! duplicate table).
//!
//! ## Reference audit
//!
//! Entry point and call order (`qc_main.cpp:994-999`, `dyn_bits.cpp:617`):
//! - `dynBitCount` fills `SECTION_DATA` geometry (`blockType/sfbCnt/`
//!   `sfbPerGroup/noOfGroups = sfbCnt/sfbPerGroup/maxSfbPerGroup`, `:626-630`)
//!   with `blockType = psyOutCh->lastWindowSequence`, then runs
//!   `noiselessCounter` (spectrum + `maxValueInSfb`), `scfCount`
//!   (`scalefac` + `maxValueInSfb`), `noiseCount` (`noiseNrg`), and returns
//!   `huffman + sideInfo + scalefac + noiseNrg` (`:641-642`).
//! - `noiselessCounter` (`:343`) zeroes `noOfSections/huffmanBits/`
//!   `sideInfoBits`, early-returns on `maxSfbPerGroup == 0`, and runs per
//!   group: `buildBitLookUp` (`:128`, one section per band, `INVALID`
//!   bits, book `-1`), `gmStage0` (`:230`, cheapest book per band, with
//!   PNS/IS pre-assignment), `gmStage1` (`:259`, merge equal-book runs +
//!   side info), `gmStage2` (`:294`, greedy max-gain merges while gain >
//!   0), then the compress pass (`:405`, re-resolves the optimal book per
//!   merged section — stage 2 never updates `codeBook` — accumulates
//!   `huffmanBits` net of side info, shifts `sfbStart` by the group index,
//!   compacts into `sectionData->huffsection`).
//! - `bitCount` (`bit_cnt.cpp:543`): `bitCount[0] = 0` iff `maxVal == 0`
//!   else `INVALID`; the `countFuncTable[maxVal cl 16]` kernel fills books
//!   1..11, unreachable books stay `INVALID`.
//! - `countValues` (`:560`) prices one `codeBook` exactly (the S6
//!   crash-recovery counter; `codeValues` *encode* half is S7 — it takes a
//!   bitstream handle and is not ported).
//!
//! ## I/O and numeric representation
//!
//! Everything is integers: quantized lines `i16`, per-band maxima `u32`,
//! scalefactors plain `i32`, bit counts `i32`. `INVALID_BITCOUNT =
//! i32::MAX/4` (`bit_cnt.h:110`); sums that C forms over `INVALID`
//! entries provably stay below `i32::MAX` (`2*INVALID < MAX`) and use
//! plain `+`; `mergeBitLookUp` saturates with `min(sum, INVALID)`
//! (`dyn_bits.cpp:182`). Table entries are `u32` packed `HI/LO`
//! (books 1..10 share one ULONG: `HI_LTAB = v>>16`, `LO_LTAB = v&0xffff`)
//! or `u8` (book 11); the `+ sc` sign-bit term counts nonzero lines.
//!
//! ## Inputs S5 does not compute
//!
//! - `maxValueInSfb`: output of the S6-owned static
//!   `FDKaacEnc_calcMaxValueInSfb` (`qc_main.cpp:1219`, max abs per band
//!   over `maxSfbPerGroup` bands per group). S5 consumes it as an explicit
//!   `&[u32]` input (oracle QMAX); S6 will port the computation and can
//!   cross-check these fixtures.
//! - `noiseNrg/isBook/isScale`: explicit inputs; S1 proved
//!   `noiseNrg == NO_NOISE_PNS (i32::MIN)` and `isBook/isScale == 0`
//!   corpus-wide, so PNS/IS branches are dead on fixtures but live in
//!   code (covered by synthetic tests).
//! - `use_vcb11`: from `syntaxFlags & AC_ER_VCB11`; the stripped VBR-5
//!   build pins `syntaxFlags = 0` (`aacenc.cpp:358`), so S5 callers pass
//!   `false`. The parameter is accepted (faithful signature) but the two
//!   `find*` helpers ignore it exactly like C.
//!
//! ## Comparisons, saturation, boundaries
//!
//! `findBestBook`/`findMaxMerge` use strict `<`/`>` (first optimum wins,
//! ties keep the earlier book/position). `gmStage2` merges while gain >
//! 0 (zero gain stops). `scfCount` transmits a delta iff the next live
//! band's `|scf - last| > 60`, else `0` (bit saving); the trailing
//! no-live-band case emits `0`. `bit_count_scf_delta` debug-asserts
//! `|delta| <= 60` and panics loudly out of range (C would OOB-read the
//! table — the established safety policy, never silent UB).
//!
//! ## S5 vs S6
//!
//! S5 owns sectioning + all counting. `calcMaxValueInSfb` (gain-loop
//! gate), `reduceBitConsumption`/`crashRecovery`, grants/fill/reservoir,
//! and every bitstream write are S6/S7 and are not touched. S5 takes no
//! bit-budget input and produces no writer input beyond `SectionData`.
//!
//! ## Facts vs open points
//!
//! - [Fact] All mappings read from the cited lines; table dims from
//!   `aacEnc_rom.h:122-128,167-168`.
//! - [Fact] `fixp_abs` on widened `i16` values cannot overflow (`|q| <=
//!   32768 < i32::MAX`); the S4 envelope proof additionally guarantees no
//!   `-32768` reaches S5 on supported inputs (re-asserted by test).
//! - [Open] The C `scalefacGain == NULL` early return is unreachable in
//!   this build (QCMain always passes `qcOutCh->scf`); S5 takes `&[i32]`
//!   and does not model it.

use super::bitcount_tables::{
    HUFF_LTAB11, HUFF_LTAB1_2, HUFF_LTAB3_4, HUFF_LTAB5_6, HUFF_LTAB7_8, HUFF_LTAB9_10,
    SIDE_INFO_TAB_LONG, SIDE_INFO_TAB_SHORT,
};
use super::pe::bit_count_scf_delta;

/// `INVALID_BITCOUNT = FDK_INT_MAX/4` (`bit_cnt.h:110`).
pub const INVALID_BITCOUNT: i32 = i32::MAX / 4;
/// `CODE_BOOK_ESC_LAV = 16` (count dispatch clamp, `bit_cnt.cpp:551`).
pub const CODE_BOOK_ESC_LAV: u32 = 16;
/// `CODE_BOOK_SCF_LAV = 60` (scf-DPCM gate, `bit_cnt.h:177`).
pub const CODE_BOOK_SCF_LAV: i32 = 60;
/// `CODE_BOOK_ZERO_NO/PNS_NO/IS_*_NO` (`bit_cnt.h:116-134`).
pub const CODE_BOOK_ZERO_NO: i32 = 0;
pub const CODE_BOOK_PNS_NO: i32 = 13;
pub const CODE_BOOK_IS_OUT_OF_PHASE_NO: i32 = 14;
pub const CODE_BOOK_IS_IN_PHASE_NO: i32 = 15;
/// Block types (`psy_const.h:121-124`): LONG=0, START=1, SHORT=2, STOP=3.
pub const BLOCK_LONG: i32 = 0;
pub const BLOCK_START: i32 = 1;
pub const BLOCK_SHORT: i32 = 2;
pub const BLOCK_STOP: i32 = 3;
/// `PNS_PCM_BITS = 9` (`dyn_bits.h:117`).
pub const PNS_PCM_BITS: i32 = 9;
/// `NO_NOISE_PNS = FDK_INT_MIN` (`aacenc_pns.h:109`).
pub const NO_NOISE_PNS: i32 = i32::MIN;

/// One Huffman section (`SECTION_INFO`, `dyn_bits.h:119-124`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionInfo {
    pub code_book: i32,
    pub sfb_start: i32,
    pub sfb_cnt: i32,
    pub section_bits: i32,
}

/// Sectioning + counts (`SECTION_DATA`, `dyn_bits.h:126-139`).
///
/// `sections` holds exactly the `noOfSections` live entries in order
/// (compacted, like C's compress pass); the count is `sections.len()`
/// by construction and cannot diverge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionData {
    pub block_type: i32,
    pub no_of_groups: i32,
    pub sfb_cnt: i32,
    pub max_sfb_per_group: i32,
    pub sfb_per_group: i32,
    pub sections: Vec<SectionInfo>,
    pub side_info_bits: i32,
    pub huffman_bits: i32,
    pub scalefac_bits: i32,
    pub noise_nrg_bits: i32,
    pub first_scf: i32,
}

/// S5 channel inputs. `quant_spec` (S4 inv=2 lines), `max_value_in_sfb`
/// (S6-owned QMAX, oracle-supplied), `scf` (S4 post-inversion stored
/// scalefactors); `noise_nrg/is_book/is_scale` explicit (PNS-off zeros
/// on fixtures, asserted by test).
pub struct BitCountInput<'a> {
    pub quant_spec: &'a [i16],
    pub max_value_in_sfb: &'a [u32],
    pub scf: &'a [i32],
    pub block_type: i32,
    pub sfb_cnt: usize,
    pub max_sfb_per_group: usize,
    pub sfb_per_group: usize,
    pub sfb_offsets: &'a [i32],
    pub noise_nrg: &'a [i32],
    pub is_book: &'a [i32],
    pub is_scale: &'a [i32],
    pub use_vcb11: bool,
}

fn hi_ltab(v: u32) -> i32 {
    (v >> 16) as i32
}
fn lo_ltab(v: u32) -> i32 {
    (v & 0xffff) as i32
}

/// `count1_2_3_4_5_6_7_8_9_10_11` (`bit_cnt.cpp:121-175`).
fn count_1_to_11(values: &[i16], out: &mut [i32; 12]) {
    let mut bc1_2 = 0i32;
    let mut bc3_4 = 0i32;
    let mut bc5_6 = 0i32;
    let mut bc7_8 = 0i32;
    let mut bc9_10 = 0i32;
    let mut bc11 = 0i32;
    let mut sc = 0i32;
    let mut i = 0;
    while i < values.len() {
        let t0 = values[i] as i32;
        let t1 = values[i + 1] as i32;
        let t2 = values[i + 2] as i32;
        let t3 = values[i + 3] as i32;
        bc1_2 +=
            HUFF_LTAB1_2[((t0 + 1) * 27 + (t1 + 1) * 9 + (t2 + 1) * 3 + (t3 + 1)) as usize] as i32;
        bc5_6 += HUFF_LTAB5_6[((t0 + 4) * 9 + (t1 + 4)) as usize] as i32
            + HUFF_LTAB5_6[((t2 + 4) * 9 + (t3 + 4)) as usize] as i32;
        let a0 = t0.abs();
        sc += i32::from(a0 > 0);
        let a1 = t1.abs();
        sc += i32::from(a1 > 0);
        let a2 = t2.abs();
        sc += i32::from(a2 > 0);
        let a3 = t3.abs();
        sc += i32::from(a3 > 0);
        bc3_4 += HUFF_LTAB3_4[(a0 * 27 + a1 * 9 + a2 * 3 + a3) as usize] as i32;
        bc7_8 += HUFF_LTAB7_8[(a0 * 8 + a1) as usize] as i32
            + HUFF_LTAB7_8[(a2 * 8 + a3) as usize] as i32;
        bc9_10 += HUFF_LTAB9_10[(a0 * 13 + a1) as usize] as i32
            + HUFF_LTAB9_10[(a2 * 13 + a3) as usize] as i32;
        bc11 += HUFF_LTAB11[(a0 * 17 + a1) as usize] as i32
            + HUFF_LTAB11[(a2 * 17 + a3) as usize] as i32;
        i += 4;
    }
    out[1] = hi_ltab(bc1_2 as u32);
    out[2] = lo_ltab(bc1_2 as u32);
    out[3] = hi_ltab(bc3_4 as u32) + sc;
    out[4] = lo_ltab(bc3_4 as u32) + sc;
    out[5] = hi_ltab(bc5_6 as u32);
    out[6] = lo_ltab(bc5_6 as u32);
    out[7] = hi_ltab(bc7_8 as u32) + sc;
    out[8] = lo_ltab(bc7_8 as u32) + sc;
    out[9] = hi_ltab(bc9_10 as u32) + sc;
    out[10] = lo_ltab(bc9_10 as u32) + sc;
    out[11] = bc11 + sc;
}

/// `count3_4_5_6_7_8_9_10_11` (`:187-241`).
fn count_3_to_11(values: &[i16], out: &mut [i32; 12]) {
    let mut bc3_4 = 0i32;
    let mut bc5_6 = 0i32;
    let mut bc7_8 = 0i32;
    let mut bc9_10 = 0i32;
    let mut bc11 = 0i32;
    let mut sc = 0i32;
    let mut i = 0;
    while i < values.len() {
        let t0 = values[i] as i32;
        let t1 = values[i + 1] as i32;
        let t2 = values[i + 2] as i32;
        let t3 = values[i + 3] as i32;
        bc5_6 += HUFF_LTAB5_6[((t0 + 4) * 9 + (t1 + 4)) as usize] as i32
            + HUFF_LTAB5_6[((t2 + 4) * 9 + (t3 + 4)) as usize] as i32;
        let a0 = t0.abs();
        sc += i32::from(a0 > 0);
        let a1 = t1.abs();
        sc += i32::from(a1 > 0);
        let a2 = t2.abs();
        sc += i32::from(a2 > 0);
        let a3 = t3.abs();
        sc += i32::from(a3 > 0);
        bc3_4 += HUFF_LTAB3_4[(a0 * 27 + a1 * 9 + a2 * 3 + a3) as usize] as i32;
        bc7_8 += HUFF_LTAB7_8[(a0 * 8 + a1) as usize] as i32
            + HUFF_LTAB7_8[(a2 * 8 + a3) as usize] as i32;
        bc9_10 += HUFF_LTAB9_10[(a0 * 13 + a1) as usize] as i32
            + HUFF_LTAB9_10[(a2 * 13 + a3) as usize] as i32;
        bc11 += HUFF_LTAB11[(a0 * 17 + a1) as usize] as i32
            + HUFF_LTAB11[(a2 * 17 + a3) as usize] as i32;
        i += 4;
    }
    out[1] = INVALID_BITCOUNT;
    out[2] = INVALID_BITCOUNT;
    out[3] = hi_ltab(bc3_4 as u32) + sc;
    out[4] = lo_ltab(bc3_4 as u32) + sc;
    out[5] = hi_ltab(bc5_6 as u32);
    out[6] = lo_ltab(bc5_6 as u32);
    out[7] = hi_ltab(bc7_8 as u32) + sc;
    out[8] = lo_ltab(bc7_8 as u32) + sc;
    out[9] = hi_ltab(bc9_10 as u32) + sc;
    out[10] = lo_ltab(bc9_10 as u32) + sc;
    out[11] = bc11 + sc;
}

/// `count5_6_7_8_9_10_11` (`:253-303`).
fn count_5_to_11(values: &[i16], out: &mut [i32; 12]) {
    let mut bc5_6 = 0i32;
    let mut bc7_8 = 0i32;
    let mut bc9_10 = 0i32;
    let mut bc11 = 0i32;
    let mut sc = 0i32;
    let mut i = 0;
    while i < values.len() {
        let t0 = values[i] as i32;
        let t1 = values[i + 1] as i32;
        let t2 = values[i + 2] as i32;
        let t3 = values[i + 3] as i32;
        bc5_6 += HUFF_LTAB5_6[((t0 + 4) * 9 + (t1 + 4)) as usize] as i32
            + HUFF_LTAB5_6[((t2 + 4) * 9 + (t3 + 4)) as usize] as i32;
        let a0 = t0.abs();
        sc += i32::from(a0 > 0);
        let a1 = t1.abs();
        sc += i32::from(a1 > 0);
        let a2 = t2.abs();
        sc += i32::from(a2 > 0);
        let a3 = t3.abs();
        sc += i32::from(a3 > 0);
        bc7_8 += HUFF_LTAB7_8[(a0 * 8 + a1) as usize] as i32
            + HUFF_LTAB7_8[(a2 * 8 + a3) as usize] as i32;
        bc9_10 += HUFF_LTAB9_10[(a0 * 13 + a1) as usize] as i32
            + HUFF_LTAB9_10[(a2 * 13 + a3) as usize] as i32;
        bc11 += HUFF_LTAB11[(a0 * 17 + a1) as usize] as i32
            + HUFF_LTAB11[(a2 * 17 + a3) as usize] as i32;
        i += 4;
    }
    out[1] = INVALID_BITCOUNT;
    out[2] = INVALID_BITCOUNT;
    out[3] = INVALID_BITCOUNT;
    out[4] = INVALID_BITCOUNT;
    out[5] = hi_ltab(bc5_6 as u32);
    out[6] = lo_ltab(bc5_6 as u32);
    out[7] = hi_ltab(bc7_8 as u32) + sc;
    out[8] = lo_ltab(bc7_8 as u32) + sc;
    out[9] = hi_ltab(bc9_10 as u32) + sc;
    out[10] = lo_ltab(bc9_10 as u32) + sc;
    out[11] = bc11 + sc;
}

/// `count7_8_9_10_11` (`:315-363`).
fn count_7_to_11(values: &[i16], out: &mut [i32; 12]) {
    let mut bc7_8 = 0i32;
    let mut bc9_10 = 0i32;
    let mut bc11 = 0i32;
    let mut sc = 0i32;
    let mut i = 0;
    while i < values.len() {
        let a0 = (values[i] as i32).abs();
        sc += i32::from(a0 > 0);
        let a1 = (values[i + 1] as i32).abs();
        sc += i32::from(a1 > 0);
        let a2 = (values[i + 2] as i32).abs();
        sc += i32::from(a2 > 0);
        let a3 = (values[i + 3] as i32).abs();
        sc += i32::from(a3 > 0);
        bc7_8 += HUFF_LTAB7_8[(a0 * 8 + a1) as usize] as i32
            + HUFF_LTAB7_8[(a2 * 8 + a3) as usize] as i32;
        bc9_10 += HUFF_LTAB9_10[(a0 * 13 + a1) as usize] as i32
            + HUFF_LTAB9_10[(a2 * 13 + a3) as usize] as i32;
        bc11 += HUFF_LTAB11[(a0 * 17 + a1) as usize] as i32
            + HUFF_LTAB11[(a2 * 17 + a3) as usize] as i32;
        i += 4;
    }
    out[1] = INVALID_BITCOUNT;
    out[2] = INVALID_BITCOUNT;
    out[3] = INVALID_BITCOUNT;
    out[4] = INVALID_BITCOUNT;
    out[5] = INVALID_BITCOUNT;
    out[6] = INVALID_BITCOUNT;
    out[7] = hi_ltab(bc7_8 as u32) + sc;
    out[8] = lo_ltab(bc7_8 as u32) + sc;
    out[9] = hi_ltab(bc9_10 as u32) + sc;
    out[10] = lo_ltab(bc9_10 as u32) + sc;
    out[11] = bc11 + sc;
}

/// `count9_10_11` (`:375-419`).
fn count_9_to_11(values: &[i16], out: &mut [i32; 12]) {
    let mut bc9_10 = 0i32;
    let mut bc11 = 0i32;
    let mut sc = 0i32;
    let mut i = 0;
    while i < values.len() {
        let a0 = (values[i] as i32).abs();
        sc += i32::from(a0 > 0);
        let a1 = (values[i + 1] as i32).abs();
        sc += i32::from(a1 > 0);
        let a2 = (values[i + 2] as i32).abs();
        sc += i32::from(a2 > 0);
        let a3 = (values[i + 3] as i32).abs();
        sc += i32::from(a3 > 0);
        bc9_10 += HUFF_LTAB9_10[(a0 * 13 + a1) as usize] as i32
            + HUFF_LTAB9_10[(a2 * 13 + a3) as usize] as i32;
        bc11 += HUFF_LTAB11[(a0 * 17 + a1) as usize] as i32
            + HUFF_LTAB11[(a2 * 17 + a3) as usize] as i32;
        i += 4;
    }
    out[1..9].fill(INVALID_BITCOUNT);
    out[9] = hi_ltab(bc9_10 as u32) + sc;
    out[10] = lo_ltab(bc9_10 as u32) + sc;
    out[11] = bc11 + sc;
}

/// `count11` (`:431-472`).
fn count_11_only(values: &[i16], out: &mut [i32; 12]) {
    let mut bc11 = 0i32;
    let mut sc = 0i32;
    let mut i = 0;
    while i < values.len() {
        let a0 = (values[i] as i32).abs();
        sc += i32::from(a0 > 0);
        let a1 = (values[i + 1] as i32).abs();
        sc += i32::from(a1 > 0);
        let a2 = (values[i + 2] as i32).abs();
        sc += i32::from(a2 > 0);
        let a3 = (values[i + 3] as i32).abs();
        sc += i32::from(a3 > 0);
        bc11 += HUFF_LTAB11[(a0 * 17 + a1) as usize] as i32
            + HUFF_LTAB11[(a2 * 17 + a3) as usize] as i32;
        i += 4;
    }
    out[1..11].fill(INVALID_BITCOUNT);
    out[11] = bc11 + sc;
}

/// `countEsc` (`:484-517`): book 11 with escape coding for `|v| >= 16`.
fn count_esc(values: &[i16], out: &mut [i32; 12]) {
    let mut bc11 = 0i32;
    let mut sc = 0i32;
    let mut ec = 0i32;
    let mut i = 0;
    while i < values.len() {
        let mut t0 = (values[i] as i32).abs();
        let mut t1 = (values[i + 1] as i32).abs();
        sc += i32::from(t0 > 0) + i32::from(t1 > 0);
        let t00 = t0.min(16);
        let t01 = t1.min(16);
        bc11 += HUFF_LTAB11[(t00 * 17 + t01) as usize] as i32;
        if t0 >= 16 {
            ec += 5;
            t0 >>= 1;
            while t0 >= 16 {
                ec += 2;
                t0 >>= 1;
            }
        }
        if t1 >= 16 {
            ec += 5;
            t1 >>= 1;
            while t1 >= 16 {
                ec += 2;
                t1 >>= 1;
            }
        }
        i += 2;
    }
    out[..11].fill(INVALID_BITCOUNT);
    out[11] = bc11 + sc + ec;
}

/// `FDKaacEnc_bitCount` (`bit_cnt.cpp:543-555`): per-book bit costs for
/// one band. `out` holds books 0..11 (`CODE_BOOK_ESC_NDX + 1` entries).
/// Widths are SFB-geometry multiples of 4 (loud `debug_assert`, matching
/// the C stride-4 reads without their OOB risk).
pub fn bit_count(values: &[i16], max_val: u32) -> [i32; 12] {
    debug_assert_eq!(values.len() % 4, 0);
    // Table-range precondition: `max_val` is the band maximum, so every
    // kernel indexes in range (C would OOB-read otherwise — loud panic
    // here instead, same policy as the S3 quantizer guard).
    debug_assert!(values.iter().all(|&v| (v as i32).unsigned_abs() <= max_val));
    let mut out = [INVALID_BITCOUNT; 12];
    out[0] = if max_val == 0 { 0 } else { INVALID_BITCOUNT };
    match max_val.min(CODE_BOOK_ESC_LAV) {
        0 | 1 => count_1_to_11(values, &mut out),
        2 => count_3_to_11(values, &mut out),
        3 | 4 => count_5_to_11(values, &mut out),
        5..=7 => count_7_to_11(values, &mut out),
        8..=12 => count_9_to_11(values, &mut out),
        13..=15 => count_11_only(values, &mut out),
        _ => count_esc(values, &mut out),
    }
    out
}

/// `FDKaacEnc_countValues` (`bit_cnt.cpp:560-723`): exact cost of one
/// `codeBook` over `values` (S6 crash-recovery counter).
pub fn count_values(values: &[i16], code_book: i32) -> i32 {
    debug_assert_eq!(values.len() % 4, 0);
    let mut bit_cnt = 0i32;
    let mut i = 0;
    match code_book {
        CODE_BOOK_ZERO_NO => {}
        1 => {
            while i < values.len() {
                let (t0, t1, t2, t3) = (
                    values[i] as i32,
                    values[i + 1] as i32,
                    values[i + 2] as i32,
                    values[i + 3] as i32,
                );
                bit_cnt += hi_ltab(
                    HUFF_LTAB1_2[((t0 + 1) * 27 + (t1 + 1) * 9 + (t2 + 1) * 3 + (t3 + 1)) as usize],
                );
                i += 4;
            }
        }
        2 => {
            while i < values.len() {
                let (t0, t1, t2, t3) = (
                    values[i] as i32,
                    values[i + 1] as i32,
                    values[i + 2] as i32,
                    values[i + 3] as i32,
                );
                bit_cnt += lo_ltab(
                    HUFF_LTAB1_2[((t0 + 1) * 27 + (t1 + 1) * 9 + (t2 + 1) * 3 + (t3 + 1)) as usize],
                );
                i += 4;
            }
        }
        3 => {
            while i < values.len() {
                let t0 = (values[i] as i32).abs();
                bit_cnt += i32::from(t0 > 0);
                let t1 = (values[i + 1] as i32).abs();
                bit_cnt += i32::from(t1 > 0);
                let t2 = (values[i + 2] as i32).abs();
                bit_cnt += i32::from(t2 > 0);
                let t3 = (values[i + 3] as i32).abs();
                bit_cnt += i32::from(t3 > 0);
                bit_cnt += hi_ltab(HUFF_LTAB3_4[(t0 * 27 + t1 * 9 + t2 * 3 + t3) as usize]);
                i += 4;
            }
        }
        4 => {
            while i < values.len() {
                let t0 = (values[i] as i32).abs();
                bit_cnt += i32::from(t0 > 0);
                let t1 = (values[i + 1] as i32).abs();
                bit_cnt += i32::from(t1 > 0);
                let t2 = (values[i + 2] as i32).abs();
                bit_cnt += i32::from(t2 > 0);
                let t3 = (values[i + 3] as i32).abs();
                bit_cnt += i32::from(t3 > 0);
                bit_cnt += lo_ltab(HUFF_LTAB3_4[(t0 * 27 + t1 * 9 + t2 * 3 + t3) as usize]);
                i += 4;
            }
        }
        5 => {
            while i < values.len() {
                let (t0, t1, t2, t3) = (
                    values[i] as i32,
                    values[i + 1] as i32,
                    values[i + 2] as i32,
                    values[i + 3] as i32,
                );
                bit_cnt += hi_ltab(HUFF_LTAB5_6[((t0 + 4) * 9 + (t1 + 4)) as usize])
                    + hi_ltab(HUFF_LTAB5_6[((t2 + 4) * 9 + (t3 + 4)) as usize]);
                i += 4;
            }
        }
        6 => {
            while i < values.len() {
                let (t0, t1, t2, t3) = (
                    values[i] as i32,
                    values[i + 1] as i32,
                    values[i + 2] as i32,
                    values[i + 3] as i32,
                );
                bit_cnt += lo_ltab(HUFF_LTAB5_6[((t0 + 4) * 9 + (t1 + 4)) as usize])
                    + lo_ltab(HUFF_LTAB5_6[((t2 + 4) * 9 + (t3 + 4)) as usize]);
                i += 4;
            }
        }
        7 => {
            while i < values.len() {
                let t0 = (values[i] as i32).abs();
                bit_cnt += i32::from(t0 > 0);
                let t1 = (values[i + 1] as i32).abs();
                bit_cnt += i32::from(t1 > 0);
                let t2 = (values[i + 2] as i32).abs();
                bit_cnt += i32::from(t2 > 0);
                let t3 = (values[i + 3] as i32).abs();
                bit_cnt += i32::from(t3 > 0);
                bit_cnt += hi_ltab(HUFF_LTAB7_8[(t0 * 8 + t1) as usize])
                    + hi_ltab(HUFF_LTAB7_8[(t2 * 8 + t3) as usize]);
                i += 4;
            }
        }
        8 => {
            while i < values.len() {
                let t0 = (values[i] as i32).abs();
                bit_cnt += i32::from(t0 > 0);
                let t1 = (values[i + 1] as i32).abs();
                bit_cnt += i32::from(t1 > 0);
                let t2 = (values[i + 2] as i32).abs();
                bit_cnt += i32::from(t2 > 0);
                let t3 = (values[i + 3] as i32).abs();
                bit_cnt += i32::from(t3 > 0);
                bit_cnt += lo_ltab(HUFF_LTAB7_8[(t0 * 8 + t1) as usize])
                    + lo_ltab(HUFF_LTAB7_8[(t2 * 8 + t3) as usize]);
                i += 4;
            }
        }
        9 => {
            while i < values.len() {
                let t0 = (values[i] as i32).abs();
                bit_cnt += i32::from(t0 > 0);
                let t1 = (values[i + 1] as i32).abs();
                bit_cnt += i32::from(t1 > 0);
                let t2 = (values[i + 2] as i32).abs();
                bit_cnt += i32::from(t2 > 0);
                let t3 = (values[i + 3] as i32).abs();
                bit_cnt += i32::from(t3 > 0);
                bit_cnt += hi_ltab(HUFF_LTAB9_10[(t0 * 13 + t1) as usize])
                    + hi_ltab(HUFF_LTAB9_10[(t2 * 13 + t3) as usize]);
                i += 4;
            }
        }
        10 => {
            while i < values.len() {
                let t0 = (values[i] as i32).abs();
                bit_cnt += i32::from(t0 > 0);
                let t1 = (values[i + 1] as i32).abs();
                bit_cnt += i32::from(t1 > 0);
                let t2 = (values[i + 2] as i32).abs();
                bit_cnt += i32::from(t2 > 0);
                let t3 = (values[i + 3] as i32).abs();
                bit_cnt += i32::from(t3 > 0);
                bit_cnt += lo_ltab(HUFF_LTAB9_10[(t0 * 13 + t1) as usize])
                    + lo_ltab(HUFF_LTAB9_10[(t2 * 13 + t3) as usize]);
                i += 4;
            }
        }
        11 => {
            while i < values.len() {
                let mut t0 = (values[i] as i32).abs();
                bit_cnt += i32::from(t0 > 0);
                let mut t1 = (values[i + 1] as i32).abs();
                bit_cnt += i32::from(t1 > 0);
                bit_cnt += HUFF_LTAB11[(t0.min(16) * 17 + t1.min(16)) as usize] as i32;
                if t0 >= 16 {
                    bit_cnt += 5;
                    t0 >>= 1;
                    while t0 >= 16 {
                        bit_cnt += 2;
                        t0 >>= 1;
                    }
                }
                if t1 >= 16 {
                    bit_cnt += 5;
                    t1 >>= 1;
                    while t1 >= 16 {
                        bit_cnt += 2;
                        t1 >>= 1;
                    }
                }
                i += 2;
            }
        }
        _ => {}
    }
    bit_cnt
}

/// `getSideInfoBits` (`dyn_bits.cpp:112-125`): 5 bits under HCR with
/// book 11+, else the section-width table entry. S5 always passes
/// `use_hcr = false` (`useVCB11 = 0` in this build).
fn get_side_info_bits(section: &SectionInfo, side_info_tab: &[i16], use_hcr: bool) -> i32 {
    if use_hcr && (section.code_book == 11 || section.code_book >= 16) {
        5
    } else {
        side_info_tab[section.sfb_cnt as usize] as i32
    }
}

/// `buildBitLookUp` (`dyn_bits.cpp:128-144`): one section per band.
fn build_bit_lookup(
    quant_spec: &[i16],
    max_sfb: usize,
    sfb_offsets: &[i32],
    sfb_max: &[u32],
    bit_lookup: &mut [[i32; 12]],
    sections: &mut [SectionInfo],
) {
    for i in 0..max_sfb {
        sections[i] = SectionInfo {
            code_book: -1,
            sfb_start: i as i32,
            sfb_cnt: 1,
            section_bits: INVALID_BITCOUNT,
        };
        let width = (sfb_offsets[i + 1] - sfb_offsets[i]) as usize;
        let start = sfb_offsets[i] as usize;
        let mut out = [0i32; 12];
        let bc = bit_count(&quant_spec[start..start + width], sfb_max[i]);
        out.copy_from_slice(&bc);
        bit_lookup[i] = out;
    }
}

/// `findBestBook` (`dyn_bits.cpp:147-160`): cheapest book (first wins
/// ties); `use_vcb11` ignored exactly like C.
#[allow(clippy::needless_range_loop)]
fn find_best_book(bc: &[i32; 12], _use_vcb11: bool) -> (i32, i32) {
    let mut min_bits = INVALID_BITCOUNT;
    let mut book = 0;
    for j in 0..12 {
        if bc[j] < min_bits {
            min_bits = bc[j];
            book = j as i32;
        }
    }
    (min_bits, book)
}

/// `findMinMergeBits` (`dyn_bits.cpp:162-174`): cheapest joint book cost
/// over the summed lookup rows.
#[allow(clippy::needless_range_loop)]
fn find_min_merge_bits(bc1: &[i32; 12], bc2: &[i32; 12], _use_vcb11: bool) -> i32 {
    let mut min_bits = INVALID_BITCOUNT;
    for j in 0..12 {
        min_bits = min_bits.min(bc1[j] + bc2[j]);
    }
    min_bits
}

/// `mergeBitLookUp` (`dyn_bits.cpp:176-184`): row sum saturated at
/// `INVALID_BITCOUNT` (`FDK_ASSERT(INVALID == 0x1FFFFFFF)` holds:
/// `i32::MAX/4 == 0x1FFFFFFF`).
fn merge_bit_lookup(bc1: &mut [i32; 12], bc2: &[i32; 12]) {
    for j in 0..12 {
        bc1[j] = (bc1[j] + bc2[j]).min(INVALID_BITCOUNT);
    }
}

/// `findMaxMerge` (`dyn_bits.cpp:186-200`): first maximum gain position
/// (strict `>`, initial gain 0 at index 0 — a non-positive best breaks
/// the stage-2 loop in the caller).
fn find_max_merge(merge_gain: &[i32], sections: &[SectionInfo], max_sfb: usize) -> (i32, usize) {
    let mut max_gain = 0;
    let mut max_ndx = 0;
    // Faithful loop: `for (i = 0; i + cnt < maxSfb; i += cnt)`.
    let mut i = 0;
    while i + (sections[i].sfb_cnt as usize) < max_sfb {
        if merge_gain[i] > max_gain {
            max_gain = merge_gain[i];
            max_ndx = i;
        }
        i += sections[i].sfb_cnt as usize;
    }
    (max_gain, max_ndx)
}

/// `CalcMergeGain` (`dyn_bits.cpp:202-227`).
fn calc_merge_gain(
    sections: &[SectionInfo],
    bit_lookup: &[[i32; 12]],
    side_info_tab: &[i16],
    ndx1: usize,
    ndx2: usize,
    use_vcb11: bool,
) -> i32 {
    let use_hcr = use_vcb11;
    let merge_bits = side_info_tab[(sections[ndx1].sfb_cnt + sections[ndx2].sfb_cnt) as usize]
        as i32
        + find_min_merge_bits(&bit_lookup[ndx1], &bit_lookup[ndx2], use_vcb11);
    let split_bits = sections[ndx1].section_bits + sections[ndx2].section_bits;
    let mut gain = split_bits - merge_bits;
    let b1 = sections[ndx1].code_book;
    let b2 = sections[ndx2].code_book;
    if b1 == CODE_BOOK_PNS_NO
        || b2 == CODE_BOOK_PNS_NO
        || b1 == CODE_BOOK_IS_OUT_OF_PHASE_NO
        || b2 == CODE_BOOK_IS_OUT_OF_PHASE_NO
        || b1 == CODE_BOOK_IS_IN_PHASE_NO
        || b2 == CODE_BOOK_IS_IN_PHASE_NO
    {
        gain = -1;
    }
    let _ = use_hcr;
    gain
}

/// `gmStage0` (`dyn_bits.cpp:230-253`): cheapest book per band, with
/// PNS/IS pre-assignment (`useVCB11` hardcoded 0).
fn gm_stage0(
    sections: &mut [SectionInfo],
    bit_lookup: &[[i32; 12]],
    max_sfb: usize,
    noise_nrg: &[i32],
    is_book: &[i32],
) {
    for i in 0..max_sfb {
        if sections[i].section_bits == INVALID_BITCOUNT {
            if noise_nrg[i] != NO_NOISE_PNS {
                sections[i].code_book = CODE_BOOK_PNS_NO;
                sections[i].section_bits = 0;
            } else if is_book[i] != 0 {
                sections[i].code_book = is_book[i];
                sections[i].section_bits = 0;
            } else {
                let (bits, book) = find_best_book(&bit_lookup[i], false);
                sections[i].section_bits = bits;
                sections[i].code_book = book;
            }
        }
    }
}

/// `gmStage1` (`dyn_bits.cpp:259-288`): merge equal-book runs, add side
/// info, leave the `sfbStart` breadcrumb for the stage-2 prev search.
fn gm_stage1(
    sections: &mut [SectionInfo],
    bit_lookup: &mut [[i32; 12]],
    max_sfb: usize,
    side_info_tab: &[i16],
    use_vcb11: bool,
) {
    let use_hcr = use_vcb11;
    let mut merge_start = 0;
    loop {
        let mut merge_end = merge_start + 1;
        while merge_end < max_sfb
            && sections[merge_start].code_book == sections[merge_end].code_book
        {
            sections[merge_start].sfb_cnt += 1;
            sections[merge_start].section_bits += sections[merge_end].section_bits;
            // Merge row merge_end into row merge_start (row copied out
            // first: the rows are disjoint, same values as C's dual
            // borrow).
            let row = bit_lookup[merge_end];
            merge_bit_lookup(&mut bit_lookup[merge_start], &row);
            merge_end += 1;
        }
        let bits = get_side_info_bits(&sections[merge_start], side_info_tab, use_hcr);
        sections[merge_start].section_bits += bits;
        sections[merge_end - 1].sfb_start = sections[merge_start].sfb_start;
        merge_start = merge_end;
        if !(merge_start < max_sfb) {
            break;
        }
    }
}

/// `gmStage2` (`dyn_bits.cpp:294-340`): greedy max-gain merges.
fn gm_stage2(
    sections: &mut [SectionInfo],
    merge_gain: &mut [i32],
    bit_lookup: &mut [[i32; 12]],
    max_sfb: usize,
    side_info_tab: &[i16],
    use_vcb11: bool,
) {
    // Faithful init: `for (i = 0; i + cnt < maxSfb; i += cnt)`.
    let mut i = 0;
    while i + (sections[i].sfb_cnt as usize) < max_sfb {
        let nxt = i + sections[i].sfb_cnt as usize;
        merge_gain[i] = calc_merge_gain(sections, bit_lookup, side_info_tab, i, nxt, use_vcb11);
        i = nxt;
    }
    loop {
        let (max_gain, max_ndx) = find_max_merge(merge_gain, sections, max_sfb);
        if max_gain <= 0 {
            break;
        }
        let max_next = max_ndx + sections[max_ndx].sfb_cnt as usize;
        sections[max_ndx].sfb_cnt += sections[max_next].sfb_cnt;
        sections[max_ndx].section_bits += sections[max_next].section_bits - max_gain;
        let row = bit_lookup[max_next];
        merge_bit_lookup(&mut bit_lookup[max_ndx], &row);
        if max_ndx != 0 {
            let max_last = sections[max_ndx - 1].sfb_start as usize;
            merge_gain[max_last] = calc_merge_gain(
                sections,
                bit_lookup,
                side_info_tab,
                max_last,
                max_ndx,
                use_vcb11,
            );
        }
        let max_next2 = max_ndx + sections[max_ndx].sfb_cnt as usize;
        sections[max_next2 - 1].sfb_start = sections[max_ndx].sfb_start;
        if max_next2 < max_sfb {
            merge_gain[max_ndx] = calc_merge_gain(
                sections,
                bit_lookup,
                side_info_tab,
                max_ndx,
                max_next2,
                use_vcb11,
            );
        }
    }
}

/// Reusable scratch for [`dyn_bit_count`] (M5.22).
///
/// Owned by persistent encoder state (`QcState`) and passed down to
/// [`noiseless_counter`]. The three buffers replace the per-group `vec![]`
/// allocations of `BITCNTR_STATE`-equivalent working memory; they are
/// resized to `max_sfb_per_group.max(1)` at the top of every group
/// iteration and hold no information across groups, calls, channels, or
/// encoder instances.
///
/// Initialization semantics (audited against `build_bit_lookup` /
/// `gm_stage0..2` / the compress pass):
/// * `bit_lookup` — every row later read is rewritten in full by
///   `build_bit_lookup` (`out.copy_from_slice`), so only the length is
///   restored.
/// * `work` — every `SectionInfo` later read is rewritten in full by
///   `build_bit_lookup`, so only the length is restored (the resize
///   placeholder matches the former `vec![]` initializer anyway).
/// * `merge_gain` — only section-start indices are written before the
///   first `find_max_merge` walk, so the former fresh-allocation zeros are
///   restored explicitly (`fill(0)`) for all potentially observed
///   elements.
#[derive(Debug, Default, Clone)]
pub struct BitCountScratch {
    bit_lookup: Vec<[i32; 12]>,
    merge_gain: Vec<i32>,
    work: Vec<SectionInfo>,
}

/// `noiselessCounter` (`dyn_bits.cpp:343-430`): per-group sectioning.
/// Returns `(sections, side_info_bits, huffman_bits)`.
#[allow(clippy::too_many_arguments)]
fn noiseless_counter(
    quant_spec: &[i16],
    max_value_in_sfb: &[u32],
    sfb_offsets: &[i32],
    block_type: i32,
    sfb_cnt: usize,
    max_sfb_per_group: usize,
    sfb_per_group: usize,
    noise_nrg: &[i32],
    is_book: &[i32],
    use_vcb11: bool,
    scratch: &mut BitCountScratch,
) -> (Vec<SectionInfo>, i32, i32) {
    let side_info_tab: &[i16] = match block_type {
        BLOCK_SHORT => &SIDE_INFO_TAB_SHORT,
        BLOCK_LONG | BLOCK_START | BLOCK_STOP => &SIDE_INFO_TAB_LONG,
        _ => &SIDE_INFO_TAB_LONG,
    };
    let use_hcr = use_vcb11;
    let mut out_sections = Vec::new();
    let mut side_bits = 0i32;
    let mut huff_bits = 0i32;
    if max_sfb_per_group == 0 {
        return (out_sections, side_bits, huff_bits);
    }
    let mut grp = 0;
    while grp < sfb_cnt {
        // Per-group scratch (C reuses the BITCNTR_STATE buffers). M5.22:
        // caller-owned and reused across groups/calls — the lengths are
        // restored to `max_sfb_per_group.max(1)` here, `bit_lookup`/`work`
        // are then fully rewritten for every index read later, and
        // `merge_gain`'s observable zeros are restored explicitly (it is
        // only partially written before its first read).
        let n = max_sfb_per_group.max(1);
        scratch.bit_lookup.resize(n, [0i32; 12]);
        scratch.merge_gain.resize(n, 0);
        scratch.merge_gain.fill(0);
        scratch.work.resize(
            n,
            SectionInfo {
                code_book: -1,
                sfb_start: 0,
                sfb_cnt: 0,
                section_bits: 0,
            },
        );
        let bit_lookup = &mut scratch.bit_lookup[..];
        let merge_gain = &mut scratch.merge_gain[..];
        let work = &mut scratch.work[..];
        let off = &sfb_offsets[grp..];
        build_bit_lookup(
            quant_spec,
            max_sfb_per_group,
            off,
            &max_value_in_sfb[grp..],
            bit_lookup,
            work,
        );
        gm_stage0(
            work,
            bit_lookup,
            max_sfb_per_group,
            &noise_nrg[grp..],
            &is_book[grp..],
        );
        gm_stage1(
            work,
            bit_lookup,
            max_sfb_per_group,
            side_info_tab,
            use_vcb11,
        );
        gm_stage2(
            work,
            merge_gain,
            bit_lookup,
            max_sfb_per_group,
            side_info_tab,
            use_vcb11,
        );
        // Compress pass (re-resolves books; stage 2 never updates codeBook).
        let mut i = 0;
        while i < max_sfb_per_group {
            let cnt = work[i].sfb_cnt as usize;
            if work[i].code_book == CODE_BOOK_PNS_NO
                || work[i].code_book == CODE_BOOK_IS_OUT_OF_PHASE_NO
                || work[i].code_book == CODE_BOOK_IS_IN_PHASE_NO
            {
                work[i].section_bits = 0;
            } else {
                let (_min, book) = find_best_book(&bit_lookup[i], use_vcb11);
                work[i].code_book = book;
                huff_bits +=
                    work[i].section_bits - get_side_info_bits(&work[i], side_info_tab, use_hcr);
            }
            work[i].sfb_start += grp as i32;
            side_bits += get_side_info_bits(&work[i], side_info_tab, use_hcr);
            out_sections.push(work[i]);
            i += cnt;
        }
        grp += sfb_per_group;
    }
    (out_sections, side_bits, huff_bits)
}

/// `scfCount` (`dyn_bits.cpp:467-586`): scalefactor DPCM costs with the
/// zero-`maxValueInSfb` skip optimization. Returns `(scalefac_bits,
/// first_scf)`.
fn scf_count(
    scf: &[i32],
    max_value_in_sfb: &[u32],
    sections: &[SectionInfo],
    is_scale: &[i32],
) -> (i32, i32) {
    let mut bits = 0i32;
    let mut first_scf = 0i32;
    let mut last_val_scf = 0i32;
    let mut delta_scf = 0i32;
    let mut found: i32;
    let mut scf_skip_counter = 0i32;
    let mut last_val_is = 0i32;
    // Pre-pass: firstScf = first non-zero-book section start (C :485-491).
    for sec in sections.iter() {
        if sec.code_book != CODE_BOOK_ZERO_NO {
            first_scf = sec.sfb_start;
            last_val_scf = scf[first_scf as usize];
            break;
        }
    }
    for (si, sec) in sections.iter().enumerate() {
        if sec.code_book == CODE_BOOK_IS_OUT_OF_PHASE_NO
            || sec.code_book == CODE_BOOK_IS_IN_PHASE_NO
        {
            for j in sec.sfb_start..sec.sfb_start + sec.sfb_cnt {
                let delta_is = is_scale[j as usize].wrapping_sub(last_val_is);
                last_val_is = is_scale[j as usize];
                bits += bit_count_scf_delta(delta_is);
            }
        } else if sec.code_book != CODE_BOOK_ZERO_NO && sec.code_book != CODE_BOOK_PNS_NO {
            let tmp = sec.sfb_start + sec.sfb_cnt;
            for j in sec.sfb_start..tmp {
                if max_value_in_sfb[j as usize] == 0 {
                    found = 0;
                    if scf_skip_counter == 0 {
                        if j == tmp - 1 {
                            found = 0;
                        } else {
                            let mut k = j + 1;
                            while k < tmp {
                                if max_value_in_sfb[k as usize] != 0 {
                                    found = 1;
                                    if scf[k as usize].wrapping_sub(last_val_scf).abs()
                                        <= CODE_BOOK_SCF_LAV
                                    {
                                        delta_scf = 0;
                                    } else {
                                        delta_scf = last_val_scf.wrapping_sub(scf[j as usize]);
                                        last_val_scf = scf[j as usize];
                                        scf_skip_counter = 0;
                                    }
                                    break;
                                }
                                scf_skip_counter += 1;
                                k += 1;
                            }
                        }
                        let mut m = si + 1;
                        while m < sections.len() && found == 0 {
                            if sections[m].code_book != CODE_BOOK_ZERO_NO
                                && sections[m].code_book != CODE_BOOK_PNS_NO
                            {
                                let end = sections[m].sfb_start + sections[m].sfb_cnt;
                                let mut n = sections[m].sfb_start;
                                while n < end {
                                    if max_value_in_sfb[n as usize] != 0 {
                                        found = 1;
                                        if scf[n as usize].wrapping_sub(last_val_scf).abs()
                                            <= CODE_BOOK_SCF_LAV
                                        {
                                            delta_scf = 0;
                                        } else {
                                            delta_scf = last_val_scf.wrapping_sub(scf[j as usize]);
                                            last_val_scf = scf[j as usize];
                                            scf_skip_counter = 0;
                                        }
                                        break;
                                    }
                                    scf_skip_counter += 1;
                                    n += 1;
                                }
                            }
                            m += 1;
                        }
                        if found == 0 {
                            delta_scf = 0;
                            scf_skip_counter = 0;
                        }
                    } else {
                        delta_scf = 0;
                        scf_skip_counter -= 1;
                    }
                } else {
                    delta_scf = last_val_scf.wrapping_sub(scf[j as usize]);
                    last_val_scf = scf[j as usize];
                }
                bits += bit_count_scf_delta(delta_scf);
            }
        }
    }
    (bits, first_scf)
}

/// `noiseCount` (`dyn_bits.cpp:589-615`): PNS energy costs (`9` bits for
/// the first band, DPCM after). No-op at PNS-off (corpus-wide).
fn noise_count(sections: &[SectionInfo], noise_nrg: &[i32]) -> i32 {
    let mut bits = 0i32;
    let mut pcm_flag = true;
    let mut last_val_pns = 0i32;
    for sec in sections.iter() {
        if sec.code_book == CODE_BOOK_PNS_NO {
            for j in sec.sfb_start..sec.sfb_start + sec.sfb_cnt {
                if pcm_flag {
                    bits += PNS_PCM_BITS;
                    last_val_pns = noise_nrg[j as usize];
                    pcm_flag = false;
                } else {
                    let delta = noise_nrg[j as usize].wrapping_sub(last_val_pns);
                    last_val_pns = noise_nrg[j as usize];
                    bits += bit_count_scf_delta(delta);
                }
            }
        }
    }
    bits
}

/// `FDKaacEnc_dynBitCount` (`dyn_bits.cpp:617-643`): full per-channel
/// noiseless count. Returns `(SectionData, dyn_bits_used)`.
/// `dynBitCount` (`dyn_bits.cpp:437-465`) entry point. Convenience form
/// that allocates a fresh [`BitCountScratch`] per call; hot-path callers
/// use [`dyn_bit_count_reuse`].
#[cfg(test)]
pub fn dyn_bit_count(input: &BitCountInput<'_>) -> (SectionData, i32) {
    let mut scratch = BitCountScratch::default();
    dyn_bit_count_reuse(input, &mut scratch)
}

/// [`dyn_bit_count`] with caller-owned scratch reuse (M5.22): identical
/// integer arithmetic, comparison order, tie-breaking, loop order and
/// early exits; the scratch only removes the per-group heap allocations.
pub fn dyn_bit_count_reuse(
    input: &BitCountInput<'_>,
    scratch: &mut BitCountScratch,
) -> (SectionData, i32) {
    let no_of_groups = (input.sfb_cnt / input.sfb_per_group) as i32;
    let (sections, side_info_bits, huffman_bits) = noiseless_counter(
        input.quant_spec,
        input.max_value_in_sfb,
        input.sfb_offsets,
        input.block_type,
        input.sfb_cnt,
        input.max_sfb_per_group,
        input.sfb_per_group,
        input.noise_nrg,
        input.is_book,
        input.use_vcb11,
        scratch,
    );
    let (scalefac_bits, first_scf) =
        scf_count(input.scf, input.max_value_in_sfb, &sections, input.is_scale);
    let noise_nrg_bits = noise_count(&sections, input.noise_nrg);
    let dyn_used = huffman_bits + side_info_bits + scalefac_bits + noise_nrg_bits;
    (
        SectionData {
            block_type: input.block_type,
            no_of_groups,
            sfb_cnt: input.sfb_cnt as i32,
            max_sfb_per_group: input.max_sfb_per_group as i32,
            sfb_per_group: input.sfb_per_group as i32,
            sections,
            side_info_bits,
            huffman_bits,
            scalefac_bits,
            noise_nrg_bits,
            first_scf,
        },
        dyn_used,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::pe::bit_count_scf_delta;

    /// Committed S5 corpus: same 19 element-frames as S1/S2/S3/S4.
    const S5_CASES: [(&str, u32); 19] = [
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

    #[test]
    fn s5_constants_match_reference() {
        // `bit_cnt.h:110`, `bit_cnt.h:177`, `dyn_bits.h:112-117`,
        // `psy_const.h:121-124`, `aacenc_pns.h:109`, `dyn_bits.h:117`.
        assert_eq!(INVALID_BITCOUNT, 0x1FFF_FFFF);
        assert_eq!(INVALID_BITCOUNT, i32::MAX / 4);
        assert_eq!(CODE_BOOK_ESC_LAV, 16);
        assert_eq!(CODE_BOOK_SCF_LAV, 60);
        assert_eq!(
            (
                CODE_BOOK_ZERO_NO,
                CODE_BOOK_PNS_NO,
                CODE_BOOK_IS_OUT_OF_PHASE_NO,
                CODE_BOOK_IS_IN_PHASE_NO
            ),
            (0, 13, 14, 15)
        );
        assert_eq!(
            (BLOCK_LONG, BLOCK_START, BLOCK_SHORT, BLOCK_STOP),
            (0, 1, 2, 3)
        );
        assert_eq!(PNS_PCM_BITS, 9);
        assert_eq!(NO_NOISE_PNS, i32::MIN);
        // Table shapes pin the ROM dims (`aacEnc_rom.h:122-128,167-168`).
        assert_eq!(HUFF_LTAB1_2.len(), 81);
        assert_eq!(HUFF_LTAB3_4.len(), 81);
        assert_eq!(HUFF_LTAB5_6.len(), 81);
        assert_eq!(HUFF_LTAB7_8.len(), 64);
        assert_eq!(HUFF_LTAB9_10.len(), 169);
        assert_eq!(HUFF_LTAB11.len(), 289);
        assert_eq!(SIDE_INFO_TAB_LONG.len(), 52);
        assert_eq!(SIDE_INFO_TAB_SHORT.len(), 16);
        // Spot values (first entries of each table + side-info steps).
        assert_eq!(HUFF_LTAB1_2[0], 0x000b0009);
        assert_eq!(HUFF_LTAB3_4[0], 0x00010004);
        assert_eq!(HUFF_LTAB5_6[40], HUFF_LTAB5_6[4 * 9 + 4]);
        assert_eq!(SIDE_INFO_TAB_LONG[0], 9);
        assert_eq!(SIDE_INFO_TAB_LONG[31], 0x0e);
        assert_eq!(SIDE_INFO_TAB_SHORT[0], 7);
        assert_eq!(SIDE_INFO_TAB_SHORT[7], 0x0a);
        assert_eq!(SIDE_INFO_TAB_SHORT[14], 0x0d);
    }

    #[test]
    fn s5_bit_count_dispatch_and_kernels() {
        // maxVal 0: book 0 costs 0, every other book INVALID.
        let zero = bit_count(&[0i16; 4], 0);
        assert_eq!(zero[0], 0);
        for b in 1..12 {
            assert_eq!(zero[b], bit_count(&[0i16; 4], 1)[b]);
        }
        // maxVal 2 routes to the 3_4 kernel (books 1,2 INVALID).
        let v = [2i16, -2, 1, 0];
        let bc = bit_count(&v, 2);
        assert_eq!(bc[0], INVALID_BITCOUNT);
        assert_eq!(bc[1], INVALID_BITCOUNT);
        assert_eq!(bc[2], INVALID_BITCOUNT);
        assert_eq!(bc[3], 10 + 3); // HI(ltab[2,2,1,0])=10 + sc 3
                                   // maxVal 16 routes to ESC (only book 11 valid). Note the
                                   // dispatch clamps at 16, so the true maximum (20) is passed.
        let big = bit_count(&[20i16, -20, 0, 0], 20);
        for b in 0..11 {
            assert_eq!(big[b], INVALID_BITCOUNT, "esc blanks {b}");
        }
        // ltab11[16][16]=5 + sc 2 + ec (5+5) for the live pair, plus
        // ltab11[0][0]=4 for the zero pair = 21.
        assert_eq!(big[11], 21);
        // Single quad [1,0,0,0], maxVal 1: every book cost pinned
        // (independently computed from the ROM tables, see S5 record).
        let q = bit_count(&[1i16, 0, 0, 0], 1);
        assert_eq!(q, [INVALID_BITCOUNT, 5, 4, 5, 5, 5, 8, 5, 10, 5, 12, 10]);
    }

    #[test]
    fn s5_count_values_matches_bit_count() {
        // `countValues` (single book) agrees with the `bitCount` row for
        // every book on crafted vectors incl. ESC and zero runs.
        let cases: &[(&[i16], u32)] = &[
            (&[0, 0, 0, 0], 0),
            (&[1, -1, 1, -1], 1),
            (&[2, -2, 1, 0], 2),
            (&[4, -4, 0, 0], 4),
            (&[7, 7, -7, 0], 7),
            (&[12, -12, 0, 0], 12),
            (&[13, 0, 0, 0], 13),
            (&[16, 16, -16, 0], 16),
            (&[20, -20, 0, 0], 20),
            (&[100, -200, 300, -400], 400),
        ];
        for (v, max) in cases {
            let bc = bit_count(v, *max);
            for book in 0..12 {
                if bc[book] != INVALID_BITCOUNT {
                    assert_eq!(
                        count_values(v, book as i32),
                        bc[book],
                        "book {book} v={v:?}"
                    );
                }
            }
        }
        // Pinned literals (same independent table computation as above).
        assert_eq!(count_values(&[1, -1, 1, -1], 1), 11);
        assert_eq!(count_values(&[1, -1, 1, -1], 2), 8);
        assert_eq!(count_values(&[2, -2, 1, 0], 3), 13);
        assert_eq!(count_values(&[4, -4, 0, 0], 5), 14);
        assert_eq!(count_values(&[0, 0, 0, 0], 0), 0);
        assert_eq!(count_values(&[20, -20, 0, 0], 11), 21);
        // Large ESC pairs: ltab11[16][16]=5 + sc 2 per pair, plus escape
        // lengths (5 for the first doubling past 16, 2 per further bit).
        // (100,200): 5+2 + (5+4) + (5+6) = 27; (300,400): 5+2 + (5+8) +
        // (5+8) = 33; total 60.
        assert_eq!(count_values(&[100, -200, 300, -400], 11), 60);
    }

    #[test]
    fn s5_all_zero_band_sections() {
        // Single all-zero band: one book-0 section, side info only.
        // sideInfoTabLong[1] = 9; scf/noise loops skip book 0.
        let input = BitCountInput {
            quant_spec: &[0i16; 4],
            max_value_in_sfb: &[0],
            scf: &[7],
            block_type: BLOCK_LONG,
            sfb_cnt: 1,
            max_sfb_per_group: 1,
            sfb_per_group: 1,
            sfb_offsets: &[0, 4],
            noise_nrg: &[NO_NOISE_PNS],
            is_book: &[0],
            is_scale: &[0],
            use_vcb11: false,
        };
        let (sd, dyn_used) = dyn_bit_count(&input);
        assert_eq!(sd.block_type, BLOCK_LONG);
        assert_eq!(sd.no_of_groups, 1);
        assert_eq!(
            sd.sections,
            vec![SectionInfo {
                code_book: 0,
                sfb_start: 0,
                sfb_cnt: 1,
                section_bits: 9,
            }]
        );
        assert_eq!(
            (
                sd.side_info_bits,
                sd.huffman_bits,
                sd.scalefac_bits,
                sd.noise_nrg_bits
            ),
            (9, 0, 0, 0)
        );
        assert_eq!(sd.first_scf, 0);
        assert_eq!(dyn_used, 9);
    }

    #[test]
    fn s5_synthetic_pns_section() {
        // One PNS band: stage0 pre-assigns book 13 with 0 section bits;
        // noiseCount charges the 9-bit PCM header; scfCount skips PNS.
        let input = BitCountInput {
            quant_spec: &[0i16; 4],
            max_value_in_sfb: &[0],
            scf: &[7],
            block_type: BLOCK_LONG,
            sfb_cnt: 1,
            max_sfb_per_group: 1,
            sfb_per_group: 1,
            sfb_offsets: &[0, 4],
            noise_nrg: &[5],
            is_book: &[0],
            is_scale: &[0],
            use_vcb11: false,
        };
        let (sd, dyn_used) = dyn_bit_count(&input);
        assert_eq!(
            sd.sections,
            vec![SectionInfo {
                code_book: CODE_BOOK_PNS_NO,
                sfb_start: 0,
                sfb_cnt: 1,
                section_bits: 0,
            }]
        );
        assert_eq!((sd.scalefac_bits, sd.noise_nrg_bits), (0, 9));
        assert_eq!(dyn_used, sd.side_info_bits + 9);
    }

    #[test]
    fn s5_synthetic_is_section() {
        // One IS band: stage0 pre-assigns the IS book; scfCount prices
        // isScale DPCM deltas through the same scf table.
        let input = BitCountInput {
            quant_spec: &[0i16; 4],
            max_value_in_sfb: &[0],
            scf: &[7],
            block_type: BLOCK_LONG,
            sfb_cnt: 1,
            max_sfb_per_group: 1,
            sfb_per_group: 1,
            sfb_offsets: &[0, 4],
            noise_nrg: &[NO_NOISE_PNS],
            is_book: &[CODE_BOOK_IS_IN_PHASE_NO],
            is_scale: &[3],
            use_vcb11: false,
        };
        let (sd, dyn_used) = dyn_bit_count(&input);
        assert_eq!(sd.sections[0].code_book, CODE_BOOK_IS_IN_PHASE_NO);
        assert_eq!(sd.sections[0].section_bits, 0);
        assert_eq!(sd.scalefac_bits, bit_count_scf_delta(3));
        assert_eq!(sd.noise_nrg_bits, 0);
        assert_eq!(dyn_used, sd.side_info_bits + sd.scalefac_bits);
    }

    #[test]
    fn s5_scf_skip_forward_saves_bits() {
        // Two bands, first silent: band 0 (zeros) merges with the live
        // band 1 in stage 2 (side-info saving beats joint cost), so the
        // silent band sits INSIDE a live section and the scfCount
        // skip-forward path fires: with |scf[1]-scf[0]| <= 60 the silent
        // band codes a zero delta (cost 1) instead of transmitting.
        let input = BitCountInput {
            quant_spec: &[0i16, 0, 0, 0, 1, 0, 0, 0],
            max_value_in_sfb: &[0, 1],
            scf: &[10, 20],
            block_type: BLOCK_LONG,
            sfb_cnt: 2,
            max_sfb_per_group: 2,
            sfb_per_group: 2,
            sfb_offsets: &[0, 4, 8],
            noise_nrg: &[NO_NOISE_PNS, NO_NOISE_PNS],
            is_book: &[0, 0],
            is_scale: &[0, 0],
            use_vcb11: false,
        };
        let (sd, _) = dyn_bit_count(&input);
        assert_eq!(sd.sections.len(), 1);
        assert_eq!(sd.first_scf, 0);
        // Silent band: delta 0 word (cost 1); live band: 10-20 = -10.
        assert_eq!(
            sd.scalefac_bits,
            bit_count_scf_delta(0) + bit_count_scf_delta(-10)
        );
        assert_eq!(bit_count_scf_delta(0), 1);
    }

    #[test]
    fn s5_corpus_exact() {
        // End-to-end exactness: S4 inv=2 lines + stored scf plus oracle
        // QMAX/geometry/PNS-off vectors reproduce the oracle sectioning
        // (book/start/cnt/bits per section) and all four counts plus the
        // total bit-exactly. Geometry cross-checks qc1, proving identical
        // inputs; PNS-off preconditions are asserted (oracle-supplied, not
        // assumed).
        use super::super::testdata;
        for (name, f) in S5_CASES {
            let s1 = testdata::load_s1(name, f);
            let s4 = testdata::load_s4(name, f);
            let s5 = testdata::load_s5(name, f);
            assert_eq!(s5.frame, f);
            assert_eq!(s5.ch.len(), s1.ch.len(), "{name} f={f} channels");
            for (ci, ((c1, c4), c5)) in s1.ch.iter().zip(s4.ch.iter()).zip(s5.ch.iter()).enumerate()
            {
                assert_eq!(c5.geo_cnt, c1.geo_cnt, "{name} f={f} ch={ci} cnt");
                assert_eq!(c5.geo_per, c1.geo_per, "{name} f={f} ch={ci} per");
                assert_eq!(c5.geo_max, c1.geo_max, "{name} f={f} ch={ci} max");
                assert_eq!(c5.offsets, c1.offsets, "{name} f={f} ch={ci} off");
                assert_eq!(
                    c5.geo_seq as i32, c1.geo_seq as i32,
                    "{name} f={f} ch={ci} seq"
                );
                for &n in &c5.noise_nrg {
                    assert_eq!(n, NO_NOISE_PNS, "{name} f={f} ch={ci} PNS-off");
                }
                assert!(
                    c5.is_book.iter().all(|&b| b == 0),
                    "{name} f={f} ch={ci} isBook-zero"
                );
                assert!(
                    c5.is_scale.iter().all(|&s| s == 0),
                    "{name} f={f} ch={ci} isScale-zero"
                );
                let lv = &c4.levels[2];
                assert_eq!(lv.inv, 2);
                let input = BitCountInput {
                    quant_spec: &lv.qspec,
                    max_value_in_sfb: &c5.qmax,
                    scf: &lv.scf,
                    block_type: c5.geo_seq as i32,
                    sfb_cnt: c5.geo_cnt,
                    max_sfb_per_group: c5.geo_max,
                    sfb_per_group: c5.geo_per,
                    sfb_offsets: &c5.offsets,
                    noise_nrg: &c5.noise_nrg,
                    is_book: &c5.is_book,
                    is_scale: &c5.is_scale,
                    use_vcb11: false,
                };
                let (sd, dyn_used) = dyn_bit_count(&input);
                assert_eq!(sd.block_type, c5.block, "{name} f={f} ch={ci} block");
                assert_eq!(
                    sd.sections.len(),
                    c5.sections.len(),
                    "{name} f={f} ch={ci} nsec"
                );
                for (si, (got, exp)) in sd.sections.iter().zip(c5.sections.iter()).enumerate() {
                    assert_eq!(
                        got.code_book, exp.book,
                        "{name} f={f} ch={ci} sec={si} book"
                    );
                    assert_eq!(
                        got.sfb_start, exp.start,
                        "{name} f={f} ch={ci} sec={si} start"
                    );
                    assert_eq!(got.sfb_cnt, exp.cnt, "{name} f={f} ch={ci} sec={si} cnt");
                    assert_eq!(
                        got.section_bits, exp.bits,
                        "{name} f={f} ch={ci} sec={si} bits"
                    );
                }
                assert_eq!(sd.side_info_bits, c5.side, "{name} f={f} ch={ci} side");
                assert_eq!(sd.huffman_bits, c5.huff, "{name} f={f} ch={ci} huff");
                assert_eq!(sd.scalefac_bits, c5.scf_bits, "{name} f={f} ch={ci} scf");
                assert_eq!(
                    sd.noise_nrg_bits, c5.noise_bits,
                    "{name} f={f} ch={ci} noise"
                );
                assert_eq!(sd.first_scf, c5.first, "{name} f={f} ch={ci} first");
                assert_eq!(dyn_used, c5.dyn_used, "{name} f={f} ch={ci} dyn");
                assert_eq!(
                    dyn_used,
                    sd.huffman_bits + sd.side_info_bits + sd.scalefac_bits + sd.noise_nrg_bits,
                    "{name} f={f} ch={ci} dyn-sum"
                );
            }
        }
    }

    #[test]
    fn s5_scratch_reuse_is_bit_exact() {
        // M5.22: one shared `BitCountScratch` across every S5 case must
        // return exactly the same `SectionData` (and dynamic-bit total) as
        // the per-call fresh-scratch wrapper — in the corpus order and in
        // reverse order, so the reused buffers shrink and grow across
        // long/short block shapes while carrying state from the previous
        // case.
        use super::super::testdata;
        let mut shared = BitCountScratch::default();
        for pass in 0..2 {
            let mut cases: Vec<(&str, u32)> = S5_CASES.to_vec();
            if pass == 1 {
                cases.reverse();
            }
            for (name, f) in cases {
                let s4 = testdata::load_s4(name, f);
                let s5 = testdata::load_s5(name, f);
                for (ci, (c4, c5)) in s4.ch.iter().zip(s5.ch.iter()).enumerate() {
                    let lv = &c4.levels[2];
                    assert_eq!(lv.inv, 2, "{name} f={f} ch={ci} inv");
                    let input = BitCountInput {
                        quant_spec: &lv.qspec,
                        max_value_in_sfb: &c5.qmax,
                        scf: &lv.scf,
                        block_type: c5.geo_seq as i32,
                        sfb_cnt: c5.geo_cnt,
                        max_sfb_per_group: c5.geo_max,
                        sfb_per_group: c5.geo_per,
                        sfb_offsets: &c5.offsets,
                        noise_nrg: &c5.noise_nrg,
                        is_book: &c5.is_book,
                        is_scale: &c5.is_scale,
                        use_vcb11: false,
                    };
                    let (fresh_sd, fresh_bits) = dyn_bit_count(&input);
                    let (reuse_sd, reuse_bits) = dyn_bit_count_reuse(&input, &mut shared);
                    assert_eq!(
                        fresh_bits, reuse_bits,
                        "{name} f={f} ch={ci} pass={pass} dyn bits"
                    );
                    assert_eq!(
                        fresh_sd, reuse_sd,
                        "{name} f={f} ch={ci} pass={pass} section data"
                    );
                }
            }
        }
        // Synthetic shapes the committed corpus does not exercise with a
        // reused scratch (reviewer-identified gap): PNS and IS sections
        // (the `gm_stage0` pre-assignment and the `calc_merge_gain` −1
        // veto), and a two-group short block so the per-group reset path
        // runs repeatedly on the same buffers after long-block shapes.
        let pns = BitCountInput {
            quant_spec: &[0i16; 4],
            max_value_in_sfb: &[0],
            scf: &[7],
            block_type: BLOCK_LONG,
            sfb_cnt: 1,
            max_sfb_per_group: 1,
            sfb_per_group: 1,
            sfb_offsets: &[0, 4],
            noise_nrg: &[5],
            is_book: &[0],
            is_scale: &[0],
            use_vcb11: false,
        };
        let is_sec = BitCountInput {
            quant_spec: &[0i16; 4],
            max_value_in_sfb: &[0],
            scf: &[7],
            block_type: BLOCK_LONG,
            sfb_cnt: 1,
            max_sfb_per_group: 1,
            sfb_per_group: 1,
            sfb_offsets: &[0, 4],
            noise_nrg: &[NO_NOISE_PNS],
            is_book: &[CODE_BOOK_IS_IN_PHASE_NO],
            is_scale: &[3],
            use_vcb11: false,
        };
        let short_2grp = BitCountInput {
            quant_spec: &[3i16; 32],
            max_value_in_sfb: &[3u32; 8],
            scf: &[7i32; 8],
            block_type: BLOCK_SHORT,
            sfb_cnt: 8,
            max_sfb_per_group: 3,
            sfb_per_group: 4,
            sfb_offsets: &[0, 4, 8, 12, 16, 20, 24, 28, 32],
            noise_nrg: &[NO_NOISE_PNS; 8],
            is_book: &[0i32; 8],
            is_scale: &[0i32; 8],
            use_vcb11: false,
        };
        let all_zeroes = BitCountInput {
            quant_spec: &[0i16; 4],
            max_value_in_sfb: &[0],
            scf: &[0],
            block_type: BLOCK_LONG,
            sfb_cnt: 1,
            max_sfb_per_group: 0,
            sfb_per_group: 1,
            sfb_offsets: &[0, 4],
            noise_nrg: &[NO_NOISE_PNS],
            is_book: &[0],
            is_scale: &[0],
            use_vcb11: false,
        };
        for (tag, input) in [
            ("pns-short-pns", &pns),
            ("is", &is_sec),
            ("short-2grp", &short_2grp),
            ("all-zeroes", &all_zeroes),
            ("short-2grp-again", &short_2grp),
            ("pns-again", &pns),
        ] {
            let (fresh_sd, fresh_bits) = dyn_bit_count(input);
            let (reuse_sd, reuse_bits) = dyn_bit_count_reuse(input, &mut shared);
            assert_eq!(fresh_bits, reuse_bits, "{tag} synthetic dyn bits");
            assert_eq!(fresh_sd, reuse_sd, "{tag} synthetic section data");
        }
    }

    #[test]
    fn s5_quantizer_envelope_holds() {
        // `-32768` cannot occur on S5 inputs (S4-proven; re-asserted here
        // because `fixp_abs` widening is the only S5-side assumption), and
        // every QMAX fits the `u16`-ish envelope the loader enforces.
        use super::super::testdata;
        for (name, f) in S5_CASES {
            let s4 = testdata::load_s4(name, f);
            let s5 = testdata::load_s5(name, f);
            for (ci, (c4, c5)) in s4.ch.iter().zip(s5.ch.iter()).enumerate() {
                let lv = &c4.levels[2];
                for (i, &q) in lv.qspec.iter().enumerate() {
                    assert_ne!(q, -32768, "{name} f={f} ch={ci} line={i} envelope");
                }
                for (b, &q) in c5.qmax.iter().enumerate() {
                    assert!(q <= 32768, "{name} f={f} ch={ci} band={b} qmax");
                }
            }
        }
    }

    #[test]
    fn s5_expected_values_are_checked_non_vacuously() {
        // Tamper check: flipping one committed section book must break
        // comparison.
        use super::super::testdata;
        let s5 = testdata::load_s5("onset-stereo", 24);
        let mut tampered: Vec<i32> = s5.ch[0].sections.iter().map(|s| s.book).collect();
        tampered[4] = (tampered[4] + 1) % 12;
        let orig: Vec<i32> = s5.ch[0].sections.iter().map(|s| s.book).collect();
        assert_ne!(tampered, orig);
    }
}
