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
CONTRACTORS BE LIABLE for any direct, indirect, incidental, special, exemplary,
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
//! M4-S7 spectral and scalefactor bitstream encoding (exact bits).
//!
//! Exact-bit mirror of the encode half of `cpp/libAACenc/src/bit_cnt.cpp`
//! (`FDKaacEnc_codeValues`, `FDKaacEnc_codeScalefactorDelta`, at base commit
//! `1785529`) plus the three dynamic-payload section writers of
//! `cpp/libAACenc/src/bitenc.cpp` that consume them
//! (`encodeSpectralData`, `encodeSectionData`, `encodeScaleFactorData`).
//! Huffman code tables live in [`super::bitenc_tables`] (script-extracted,
//! probe-vs-source cross-checked); lengths are shared single-source with the
//! S5 counters (`super::bitcount_tables`, `super::pe::bit_count_scf_delta`).
//!
//! ## Reference audit
//!
//! Entry points and call order:
//! - `codeValues` (`bit_cnt.cpp:725-939`): books 0..11. Per-book table
//!   selection, traversal stride, sign-bit packing and escape handling are
//!   ported verbatim; see each book arm for the cited lines. The `default`
//!   arm (books 12+) emits nothing in C; Rust refuses explicitly.
//! - `codeScalefactorDelta` (`bit_cnt.cpp:941-950`): `|delta| > 60`
//!   returns 1 in C (an error, not a bit count); Rust returns
//!   [`BitEncError::ScalefactorDeltaOutOfRange`]. Otherwise the word comes
//!   from `huff_ctabscf[delta+60]` with the shared `huff_ltabscf` length.
//! - `encodeSpectralData` (`bitenc.cpp:127-147`): per section (PNS skipped),
//!   per SFB, `codeValues(quantSpectrum + sfbOffset[sfb], width, book)`.
//! - `encodeSectionData` (`bitenc.cpp:239-281`): 4-bit book + escape-coded
//!   section lengths (`SECT_ESC_VAL_LONG/SHORT = 31/7`,
//!   `SECT_BITS_LONG/SHORT = 5/3`, `dyn_bits.h:112-117`).
//! - `encodeScaleFactorData` (`bitenc.cpp:292-369`): DPCM walks for IS
//!   (`isScale`), PNS (9-bit PCM first, then DPCM) and normal bands
//!   (zero-`maxValueInSfb` bands emit delta 0 without touching `lastValScf`).
//!   Seeding: `lastValScf = scf[firstScf]`, `lastValPns = globalGain -
//!   scf[firstScf] + 100 + 60 - 90` (`globalGainOffset = 100`,
//!   `noiseOffset = 90`, `bitenc.cpp:114-116`; `LOG_NORM_PCM = -15`,
//!   `psy_const.h:161`), `lastValIs = 0`.
//!
//! ## I/O and numeric representation
//!
//! Everything is integers: quantized lines `i16`, scalefactors plain `i32`,
//! emitted bits counted exactly. Bit order is MSB-first, identical to
//! `FDKwriteBits` (`FDK_bitstream.h:342-376`): the value's low `n` bits go
//! out high-bit-first; bytes pack MSB-first with a zero-padded tail
//! (matches the `FDKsyncCache` writer flush). C masks oversized values
//! through `BitMask[n]`; Rust fails loudly instead ([`BitEncError`]) —
//! valid inputs are unaffected (proven by the exact-hex tests).
//!
//! ## S5 counting agreement (plan Q6)
//!
//! For valid inputs each encoder emits exactly its S5 count: `code_values`
//! emits `countValues(book)` bits (`bitcount::count_values`),
//! `code_scalefactor_delta` emits `bitCountScalefactorDelta` bits
//! (`pe::bit_count_scf_delta`), and the section writers emit `huffmanBits`,
//! `sideInfoBits`, `scalefacBits + noiseNrgBits` of [`SectionData`]. The
//! agreement is proven by the S7 tests (strict equality, live-recomputed
//! S5 counters — never fixture replay). The runtime `WRITE_SEC/SCAL/
//! SPEC_ERROR` checks (`bitenc.cpp:1049-1122`) belong to the S8 element
//! assembly, which will call these writers.
//!
//! ## C divergences handled in safe Rust (all documented, all loud)
//!
//! - Out-of-range spectral values (past the book LAV), widths that are not a
//!   multiple of the book stride (C reads into the neighbouring band),
//!   `|delta| > 60` (C OOB-reads the length table / returns 1), the
//!   `codeValues` default arm (C silently emits 0 bits), and bit-buffer
//!   overflow (C has no bounds check): all are explicit [`BitEncError`]s.
//! - `encodeSpectralData` skips PNS sections like C (`:135`); IS books
//!   (14/15) also emit zero spectral bits in C (via the `codeValues`
//!   default arm), so the traversal skips them explicitly with the citation
//!   instead of routing them into the strict leaf.
//! - `encodeScaleFactorData` returns C's `1` error sentinel as `Err` (it is
//!   never a bit count); valid inputs never hit it (S6 enforces
//!   `|Δscf| <= 60`, plan Q8).
//!
//! ## Out of scope (S8+)
//!
//! Element assembly (`ChannelElementWrite` write mode: element tag, ICS, MS,
//! global-gain, pulse, TNS scaffolding), `FDKaacEnc_WriteBitstream`,
//! transport and ADTS. No S1–S6 algorithm is touched.

use super::bitcount::{
    SectionData, BLOCK_SHORT, CODE_BOOK_IS_IN_PHASE_NO, CODE_BOOK_IS_OUT_OF_PHASE_NO,
    CODE_BOOK_PNS_NO, CODE_BOOK_ZERO_NO, PNS_PCM_BITS,
};
use super::bitcount_tables::{
    HUFF_LTAB11, HUFF_LTAB1_2, HUFF_LTAB3_4, HUFF_LTAB5_6, HUFF_LTAB7_8, HUFF_LTAB9_10,
};
use super::bitenc_tables::{
    HUFF_CTAB1, HUFF_CTAB10, HUFF_CTAB11, HUFF_CTAB2, HUFF_CTAB3, HUFF_CTAB4, HUFF_CTAB5,
    HUFF_CTAB6, HUFF_CTAB7, HUFF_CTAB8, HUFF_CTAB9, HUFF_CTABSCF,
};
use super::pe::bit_count_scf_delta;
use core::fmt;

/// Section length escape/length bits (`dyn_bits.h:112-117`).
pub const SECT_ESC_VAL_LONG: i32 = 31;
pub const SECT_ESC_VAL_SHORT: i32 = 7;
pub const SECT_BITS_LONG: i32 = 5;
pub const SECT_BITS_SHORT: i32 = 3;
/// Section codebook field width (`dyn_bits.h:115`).
pub const CODE_BOOK_BITS: i32 = 4;
/// `globalGainOffset` / `noiseOffset` (`bitenc.cpp:114-116`).
pub const GLOBAL_GAIN_OFFSET: i32 = 100;
pub const NOISE_OFFSET: i32 = 90;
/// `LOG_NORM_PCM` (`psy_const.h:161`).
pub const LOG_NORM_PCM: i32 = -15;
/// Largest absolute value each spectral book encodes without escapes
/// (`bit_cnt.h:164-177`): index by book 0..10 (book 11 is unbounded).
const BOOK_LAV: [i32; 11] = [0, 1, 1, 2, 2, 4, 4, 7, 7, 12, 12];

/// S7 encoding failures. C either masks (oversized values), reads out of
/// bounds (bad indices/widths/deltas), silently emits nothing (bad books)
/// or has no bounds check at all (bit buffer); every one of those is an
/// explicit error here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitEncError {
    /// `codeBook` has no spectral coding (leaf: anything outside 0..11,
    /// including PNS/IS which carry no spectral lines; traversal: anything
    /// outside 0..11/13/14/15).
    UnsupportedCodebook(i32),
    /// Spectral value past the book LAV (C would OOB-read the tables).
    ValueOutOfRange { book: i32, value: i32 },
    /// `|delta| > 60` (C returns 1 / would OOB-read `huff_ltabscf`).
    ScalefactorDeltaOutOfRange(i32),
    /// Width is not a multiple of the book stride (4 for books 1..10, 2
    /// for book 11; C reads into the neighbouring band).
    InvalidWidth { book: i32, width: usize },
    /// `write_bits` length above 32 (C `BitMask` has no such entry).
    InvalidLength(u8),
    /// Value does not fit in the written width (C silently masks).
    ValueTooLarge { value: u32, bits: u8 },
    /// Writer capacity exceeded (C has no bounds check).
    BufferOverflow { need_bits: usize, free_bits: usize },
    /// Section book does not fit the 4-bit field.
    SectionBookOutOfRange(i32),
    /// Negative section SFB count (C would emit a masked escape word).
    NegativeSectionLen(i32),
    /// Geometry/scalefactor index out of range with its array named.
    IndexOutOfRange(&'static str),
    /// No scalefactors to seed `lastValScf` from.
    EmptyScalefactors,
    /// First PNS PCM word does not fit 9 bits (C silently masks).
    PnsPcmOutOfRange(i32),
}

impl fmt::Display for BitEncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            BitEncError::UnsupportedCodebook(b) => {
                write!(f, "unsupported spectral codebook: {b}")
            }
            BitEncError::ValueOutOfRange { book, value } => {
                write!(f, "value {value} past LAV of book {book}")
            }
            BitEncError::ScalefactorDeltaOutOfRange(d) => {
                write!(f, "scalefactor delta out of range: {d}")
            }
            BitEncError::InvalidWidth { book, width } => {
                write!(f, "width {width} not a multiple of book {book} stride")
            }
            BitEncError::InvalidLength(n) => {
                write!(f, "bit length out of range: {n}")
            }
            BitEncError::ValueTooLarge { value, bits } => {
                write!(f, "value {value} does not fit in {bits} bits")
            }
            BitEncError::BufferOverflow {
                need_bits,
                free_bits,
            } => {
                write!(
                    f,
                    "bit buffer overflow: need {need_bits} bits, {free_bits} free"
                )
            }
            BitEncError::SectionBookOutOfRange(b) => {
                write!(f, "section book out of 4-bit range: {b}")
            }
            BitEncError::NegativeSectionLen(n) => {
                write!(f, "negative section SFB count: {n}")
            }
            BitEncError::IndexOutOfRange(what) => {
                write!(f, "index out of range: {what}")
            }
            BitEncError::EmptyScalefactors => {
                write!(f, "no scalefactors to encode")
            }
            BitEncError::PnsPcmOutOfRange(d) => {
                write!(f, "PNS PCM word out of range: {d}")
            }
        }
    }
}

impl std::error::Error for BitEncError {}

/// Minimal bounded MSB-first bit writer (the S7 `FDKwriteBits` analogue).
///
/// Bits pack MSB-first into bytes with a zero-padded tail, exactly like the
/// `FDKsyncCache` writer flush (`FDK_bitstream.h:452-462` over big-endian
/// `FDK_put`); the committed hex fixtures prove the packing bit-for-bit.
/// Unlike C, overflow is an explicit error, never a silent wrap.
#[derive(Debug, Clone, Default)]
pub struct BitWriter {
    buf: Vec<u8>,
    bits: usize,
}

impl BitWriter {
    /// Writer over a zeroed buffer of `capacity_bytes` (C requires a 2^n
    /// buffer; any capacity works here since bounds are checked).
    pub fn new(capacity_bytes: usize) -> Self {
        Self {
            buf: vec![0u8; capacity_bytes],
            bits: 0,
        }
    }

    /// `FDKwriteBits(hBs, value, numberOfBits)` (`FDK_bitstream.h:342`):
    /// the value's low `nbits` go out high-bit-first. Zero lengths are a
    /// no-op like C (`BitsInCache + 0 < CACHE_BITS` arm).
    pub fn write_bits(&mut self, value: u32, nbits: u8) -> Result<(), BitEncError> {
        if nbits > 32 {
            return Err(BitEncError::InvalidLength(nbits));
        }
        if nbits == 0 {
            return Ok(());
        }
        if nbits < 32 && value >= (1u32 << nbits) {
            return Err(BitEncError::ValueTooLarge { value, bits: nbits });
        }
        let need = nbits as usize;
        let free = self.buf.len() * 8 - self.bits;
        if need > free {
            return Err(BitEncError::BufferOverflow {
                need_bits: need,
                free_bits: free,
            });
        }
        for k in (0..nbits).rev() {
            if (value >> k) & 1 == 1 {
                self.buf[self.bits / 8] |= 1 << (7 - (self.bits % 8));
            }
            self.bits += 1;
        }
        Ok(())
    }

    /// Bits written so far (the `FDKgetValidBits` analogue).
    pub fn len_bits(&self) -> usize {
        self.bits
    }

    /// `true` when no bit has been written.
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// Written bytes (`bits.div_ceil(8)` of them, tail zero-padded).
    pub fn used_bytes(&self) -> &[u8] {
        &self.buf[..self.bits.div_ceil(8)]
    }

    /// Lowercase hex of [`BitWriter::used_bytes`] (oracle fixture format).
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(self.used_bytes().len() * 2);
        for b in self.used_bytes() {
            s.push(char::from_digit((b >> 4) as u32, 16).expect("hex"));
            s.push(char::from_digit((b & 0xf) as u32, 16).expect("hex"));
        }
        s
    }
}

fn hi_ltab(v: u32) -> u32 {
    v >> 16
}

fn lo_ltab(v: u32) -> u32 {
    v & 0xffff
}

/// `FDKaacEnc_codeValues` (`bit_cnt.cpp:725-939`): Huffman-encode `values`
/// with `code_book`, returning the exact number of bits emitted.
///
/// Books 1..10 require `values.len()` to be a multiple of 4, book 11 a
/// multiple of 2 (the C stride; SFB geometry guarantees this corpus-wide —
/// all 1734 committed SFB widths are multiples of 4 — anything else is an
/// explicit error instead of C's neighbouring-band read).
pub fn code_values(values: &[i16], code_book: i32, w: &mut BitWriter) -> Result<u32, BitEncError> {
    match code_book {
        CODE_BOOK_ZERO_NO => Ok(0),
        1 | 2 => {
            if values.len() % 4 != 0 {
                return Err(BitEncError::InvalidWidth {
                    book: code_book,
                    width: values.len(),
                });
            }
            let (ctab, hi) = if code_book == 1 {
                (&HUFF_CTAB1[..], true)
            } else {
                (&HUFF_CTAB2[..], false)
            };
            let mut total = 0u32;
            // `bit_cnt.cpp:738-747,749-758`: index on (value + 1).
            for quad in values.chunks_exact(4) {
                let mut idx = 0usize;
                for &v in quad {
                    let t = v as i32;
                    if !(-1..=1).contains(&t) {
                        return Err(BitEncError::ValueOutOfRange {
                            book: code_book,
                            value: t,
                        });
                    }
                    idx = idx * 3 + (t + 1) as usize;
                }
                let cw = ctab[idx] as u32;
                let len_entry = HUFF_LTAB1_2[idx];
                let len = if hi {
                    hi_ltab(len_entry)
                } else {
                    lo_ltab(len_entry)
                };
                w.write_bits(cw, len as u8)?;
                total += len;
            }
            Ok(total)
        }
        3 | 4 => {
            if values.len() % 4 != 0 {
                return Err(BitEncError::InvalidWidth {
                    book: code_book,
                    width: values.len(),
                });
            }
            let (ctab, hi) = if code_book == 3 {
                (&HUFF_CTAB3[..], true)
            } else {
                (&HUFF_CTAB4[..], false)
            };
            let mut total = 0u32;
            // `bit_cnt.cpp:761-799`: sign bits pack trailing (`signLength`
            // counts nonzero lines, `sign` accumulates `(UINT)ti >> 31`).
            for quad in values.chunks_exact(4) {
                let mut sign = 0u32;
                let mut sign_len = 0u32;
                let mut idx = 0usize;
                for &v in quad {
                    let ti = v as i32;
                    let zero = if ti == 0 { 0 } else { 1 };
                    sign_len += zero;
                    sign = (sign << zero) | ((ti as u32) >> 31);
                    let a = ti.unsigned_abs();
                    if a > BOOK_LAV[code_book as usize] as u32 {
                        return Err(BitEncError::ValueOutOfRange {
                            book: code_book,
                            value: ti,
                        });
                    }
                    idx = idx * 3 + a as usize;
                }
                let cw = ctab[idx] as u32;
                let len_entry = HUFF_LTAB3_4[idx];
                let len = if hi {
                    hi_ltab(len_entry)
                } else {
                    lo_ltab(len_entry)
                };
                w.write_bits((cw << sign_len) | sign, (len + sign_len) as u8)?;
                total += len + sign_len;
            }
            Ok(total)
        }
        5 | 6 => {
            if values.len() % 4 != 0 {
                return Err(BitEncError::InvalidWidth {
                    book: code_book,
                    width: values.len(),
                });
            }
            let (ctab, hi) = if code_book == 5 {
                (&HUFF_CTAB5[..], true)
            } else {
                (&HUFF_CTAB6[..], false)
            };
            let mut total = 0u32;
            // `bit_cnt.cpp:801-829`: the second pair's codeword is shifted
            // in first (`(cw1 << len2) + cw2`), lengths added either order.
            for quad in values.chunks_exact(4) {
                let mut t = [0usize; 4];
                for (k, &v) in quad.iter().enumerate() {
                    let tv = v as i32;
                    if !(-4..=4).contains(&tv) {
                        return Err(BitEncError::ValueOutOfRange {
                            book: code_book,
                            value: tv,
                        });
                    }
                    t[k] = (tv + 4) as usize;
                }
                let len_entry_2 = HUFF_LTAB5_6[t[2] * 9 + t[3]];
                let len2 = if hi {
                    hi_ltab(len_entry_2)
                } else {
                    lo_ltab(len_entry_2)
                };
                let word = ((ctab[t[0] * 9 + t[1]] as u32) << len2) + ctab[t[2] * 9 + t[3]] as u32;
                let len_entry_1 = HUFF_LTAB5_6[t[0] * 9 + t[1]];
                let len = len2
                    + if hi {
                        hi_ltab(len_entry_1)
                    } else {
                        lo_ltab(len_entry_1)
                    };
                w.write_bits(word, len as u8)?;
                total += len;
            }
            Ok(total)
        }
        7..=10 => {
            if values.len() % 2 != 0 {
                return Err(BitEncError::InvalidWidth {
                    book: code_book,
                    width: values.len(),
                });
            }
            let lav = BOOK_LAV[code_book as usize] as u32;
            let mut total = 0u32;
            // `bit_cnt.cpp:831-901`: pair loop; the first value seeds
            // `sign` directly, the second accumulates (`sign << zero`).
            for pair in values.chunks_exact(2) {
                let t0 = pair[0] as i32;
                let mut sign = (t0 as u32) >> 31;
                let a0 = t0.unsigned_abs();
                let mut sign_len = if a0 == 0 { 0 } else { 1 };
                let t1 = pair[1] as i32;
                let zero = if t1 == 0 { 0 } else { 1 };
                sign_len += zero;
                sign = (sign << zero) | ((t1 as u32) >> 31);
                let a1 = t1.unsigned_abs();
                if a0 > lav {
                    return Err(BitEncError::ValueOutOfRange {
                        book: code_book,
                        value: t0,
                    });
                }
                if a1 > lav {
                    return Err(BitEncError::ValueOutOfRange {
                        book: code_book,
                        value: t1,
                    });
                }
                let (cw, len) = match code_book {
                    7 => (
                        HUFF_CTAB7[(a0 * 8 + a1) as usize] as u32,
                        hi_ltab(HUFF_LTAB7_8[(a0 * 8 + a1) as usize]),
                    ),
                    8 => (
                        HUFF_CTAB8[(a0 * 8 + a1) as usize] as u32,
                        lo_ltab(HUFF_LTAB7_8[(a0 * 8 + a1) as usize]),
                    ),
                    9 => (
                        HUFF_CTAB9[(a0 * 13 + a1) as usize] as u32,
                        hi_ltab(HUFF_LTAB9_10[(a0 * 13 + a1) as usize]),
                    ),
                    _ => (
                        HUFF_CTAB10[(a0 * 13 + a1) as usize] as u32,
                        lo_ltab(HUFF_LTAB9_10[(a0 * 13 + a1) as usize]),
                    ),
                };
                w.write_bits((cw << sign_len) | sign, (len + sign_len) as u8)?;
                total += len + sign_len;
            }
            Ok(total)
        }
        11 => {
            if values.len() % 2 != 0 {
                return Err(BitEncError::InvalidWidth {
                    book: code_book,
                    width: values.len(),
                });
            }
            let mut total = 0u32;
            // `bit_cnt.cpp:903-933`: base codeword on clamped magnitudes
            // plus sign bits, then one escape word per value >= 16.
            for pair in values.chunks_exact(2) {
                let t0 = pair[0] as i32;
                let mut sign = (t0 as u32) >> 31;
                let a0 = t0.unsigned_abs();
                let mut sign_len = if a0 == 0 { 0 } else { 1 };
                let t1 = pair[1] as i32;
                let zero = if t1 == 0 { 0 } else { 1 };
                sign_len += zero;
                sign = (sign << zero) | ((t1 as u32) >> 31);
                let a1 = t1.unsigned_abs();
                let t00 = a0.min(16) as usize;
                let t01 = a1.min(16) as usize;
                let cw = HUFF_CTAB11[t00 * 17 + t01] as u32;
                let len = HUFF_LTAB11[t00 * 17 + t01] as u32;
                w.write_bits((cw << sign_len) | sign, (len + sign_len) as u8)?;
                total += len + sign_len;
                // The `for (j = 0; j < 2; j++) { ...; t0 = t1; }` idiom:
                // escape words for both magnitudes, in order.
                for &a in &[a0, a1] {
                    if a >= 16 {
                        // `:924-925`: `n = 4; for (; (p >>= 1) >= 16;) n++;`
                        let mut n = 4u32;
                        let mut p = a;
                        loop {
                            p >>= 1;
                            if p < 16 {
                                break;
                            }
                            n += 1;
                        }
                        // `:926-928`.
                        let word = (((1u32 << (n - 3)) - 2) << n) | (a - (1u32 << n));
                        let esc_len = n + n - 3;
                        w.write_bits(word, esc_len as u8)?;
                        total += esc_len;
                    }
                }
            }
            Ok(total)
        }
        // Books 12+ (RESERVED/PNS/IS): C's `default` arm silently emits
        // nothing. PNS/IS carry no spectral lines at all — the section
        // traversal skips them explicitly (see `encode_spectral_data`);
        // reaching the leaf with one is a caller bug, refused loudly.
        _ => Err(BitEncError::UnsupportedCodebook(code_book)),
    }
}

/// `FDKaacEnc_codeScalefactorDelta` (`bit_cnt.cpp:941-950`): encode one
/// scalefactor DPCM delta, returning the bits emitted. `|delta| > 60`
/// fails (C returns 1, never a bit count).
pub fn code_scalefactor_delta(delta: i32, w: &mut BitWriter) -> Result<u32, BitEncError> {
    if !(-60..=60).contains(&delta) {
        return Err(BitEncError::ScalefactorDeltaOutOfRange(delta));
    }
    // Length is the shared S5 counter (single table source, plan Q6).
    let len = bit_count_scf_delta(delta) as u8;
    let word = HUFF_CTABSCF[(delta + 60) as usize];
    w.write_bits(word, len)?;
    Ok(len as u32)
}

/// `encodeSectionData` (`bitenc.cpp:239-281`): section headers (4-bit book
/// + escape-coded SFB counts), returning the bits emitted.
///
/// The S8 `WRITE_SEC_ERROR` check compares this against `side_info_bits`.
pub fn encode_section_data(
    section_data: &SectionData,
    w: &mut BitWriter,
) -> Result<u32, BitEncError> {
    // `:249-261`: LONG/START/STOP (and any other type, via `default`) use
    // the long escape; only SHORT_WINDOW differs.
    let (esc, esc_bits) = if section_data.block_type == BLOCK_SHORT {
        (SECT_ESC_VAL_SHORT, SECT_BITS_SHORT)
    } else {
        (SECT_ESC_VAL_LONG, SECT_BITS_LONG)
    };
    let mut total = 0u32;
    // `:263-276`.
    for sec in section_data.sections.iter() {
        if sec.code_book < 0 || sec.code_book > 15 {
            return Err(BitEncError::SectionBookOutOfRange(sec.code_book));
        }
        if sec.sfb_cnt < 0 {
            return Err(BitEncError::NegativeSectionLen(sec.sfb_cnt));
        }
        w.write_bits(sec.code_book as u32, CODE_BOOK_BITS as u8)?;
        total += CODE_BOOK_BITS as u32;
        let mut len = sec.sfb_cnt;
        while len >= esc {
            w.write_bits(esc as u32, esc_bits as u8)?;
            total += esc_bits as u32;
            len -= esc;
        }
        w.write_bits(len as u32, esc_bits as u8)?;
        total += esc_bits as u32;
    }
    Ok(total)
}

/// `encodeScaleFactorData` (`bitenc.cpp:292-369`): DPCM scalefactor payload,
/// returning the bits emitted (the S8 `WRITE_SCAL_ERROR` check compares
/// this against `scalefacBits + noiseNrgBits`).
///
/// `scf` holds the S4 post-inversion stored scalefactors, `max_value_in_sfb`
/// the S6 QMAX gate, `noise_nrg`/`is_scale` the S1 PNS/IS vectors (PNS-off
/// zeros on corpus; the branches are live code), `global_gain` the S4/S6
/// gain. All index geometry comes from `section_data`.
pub fn encode_scale_factor_data(
    max_value_in_sfb: &[u32],
    section_data: &SectionData,
    scf: &[i32],
    noise_nrg: &[i32],
    is_scale: &[i32],
    global_gain: i32,
    w: &mut BitWriter,
) -> Result<u32, BitEncError> {
    if scf.is_empty() {
        return Err(BitEncError::EmptyScalefactors);
    }
    // `:307-310` (reads `scf[firstScf]` even when no band is coded).
    let first = section_data.first_scf;
    if first < 0 || first as usize >= scf.len() {
        return Err(BitEncError::IndexOutOfRange("firstScf"));
    }
    let mut last_scf = scf[first as usize];
    // `:308-309`: `globalGain - scf[first] + 100 - 4*(-15) - 90`.
    let mut last_pns = global_gain
        .wrapping_sub(scf[first as usize])
        .wrapping_add(GLOBAL_GAIN_OFFSET)
        .wrapping_sub(4 * LOG_NORM_PCM)
        .wrapping_sub(NOISE_OFFSET);
    let mut last_is = 0i32;
    let mut pcm_flag = true;
    let mut total = 0u32;
    // `:312-363`.
    for sec in section_data.sections.iter() {
        if sec.code_book == CODE_BOOK_ZERO_NO {
            continue;
        }
        if sec.code_book == CODE_BOOK_IS_OUT_OF_PHASE_NO
            || sec.code_book == CODE_BOOK_IS_IN_PHASE_NO
        {
            // `:317-326`.
            for j in sec.sfb_start..sec.sfb_start + sec.sfb_cnt {
                if j < 0 || j as usize >= is_scale.len() {
                    return Err(BitEncError::IndexOutOfRange("isScale"));
                }
                let delta = is_scale[j as usize].wrapping_sub(last_is);
                last_is = is_scale[j as usize];
                total += code_scalefactor_delta(delta, w)?;
            }
        } else if sec.code_book == CODE_BOOK_PNS_NO {
            // `:327-343`.
            for j in sec.sfb_start..sec.sfb_start + sec.sfb_cnt {
                if j < 0 || j as usize >= noise_nrg.len() {
                    return Err(BitEncError::IndexOutOfRange("noiseNrg"));
                }
                let delta = noise_nrg[j as usize].wrapping_sub(last_pns);
                last_pns = noise_nrg[j as usize];
                if pcm_flag {
                    // `:334-337`: 9-bit PCM word.
                    let v = delta.wrapping_add(1 << (PNS_PCM_BITS - 1));
                    if !(0..(1 << PNS_PCM_BITS)).contains(&v) {
                        return Err(BitEncError::PnsPcmOutOfRange(delta));
                    }
                    w.write_bits(v as u32, PNS_PCM_BITS as u8)?;
                    total += PNS_PCM_BITS as u32;
                    pcm_flag = false;
                } else {
                    total += code_scalefactor_delta(delta, w)?;
                }
            }
        } else if (1..=12).contains(&sec.code_book) {
            // `:344-360`: normal bands. Zero-`maxValueInSfb` bands emit a
            // delta of 0 *without* touching `lastValScf`.
            for j in sec.sfb_start..sec.sfb_start + sec.sfb_cnt {
                if j < 0 || j as usize >= max_value_in_sfb.len() || j as usize >= scf.len() {
                    return Err(BitEncError::IndexOutOfRange("scf band"));
                }
                if max_value_in_sfb[j as usize] == 0 {
                    total += code_scalefactor_delta(0, w)?;
                } else {
                    let delta = last_scf.wrapping_sub(scf[j as usize]);
                    last_scf = scf[j as usize];
                    total += code_scalefactor_delta(delta, w)?;
                }
            }
        } else {
            return Err(BitEncError::UnsupportedCodebook(sec.code_book));
        }
    }
    Ok(total)
}

/// `encodeSpectralData` (`bitenc.cpp:127-147`): Huffman spectral payload,
/// returning the bits emitted (the S8 `WRITE_SPEC_ERROR` check compares
/// this against `huffmanBits`).
///
/// PNS sections carry no spectral lines and are skipped like C (`:135`).
/// IS books (14/15) likewise emit zero spectral bits in C (the
/// `codeValues` default arm is a silent no-op); the traversal skips them
/// explicitly with this citation rather than routing valid books into the
/// strict leaf, which only accepts 0..11.
pub fn encode_spectral_data(
    quant_spec: &[i16],
    sfb_offsets: &[i32],
    section_data: &SectionData,
    w: &mut BitWriter,
) -> Result<u32, BitEncError> {
    let mut total = 0u32;
    // `:134-145`.
    for sec in section_data.sections.iter() {
        if sec.code_book == CODE_BOOK_PNS_NO
            || sec.code_book == CODE_BOOK_IS_OUT_OF_PHASE_NO
            || sec.code_book == CODE_BOOK_IS_IN_PHASE_NO
        {
            continue;
        }
        if sec.code_book < 0 || sec.code_book > 11 {
            return Err(BitEncError::UnsupportedCodebook(sec.code_book));
        }
        for sfb in sec.sfb_start..sec.sfb_start + sec.sfb_cnt {
            if sfb < 0 || (sfb + 1) as usize >= sfb_offsets.len() {
                return Err(BitEncError::IndexOutOfRange("sfbOffsets"));
            }
            let start = sfb_offsets[sfb as usize];
            let end = sfb_offsets[(sfb + 1) as usize];
            if start < 0 || end < start || end as usize > quant_spec.len() {
                return Err(BitEncError::IndexOutOfRange("quantSpec band"));
            }
            // `:140-142`.
            total += code_values(&quant_spec[start as usize..end as usize], sec.code_book, w)?;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::bitcount::count_values;

    #[test]
    fn s7_constants_match_reference() {
        // `dyn_bits.h:112-117`, `bitenc.cpp:114-116`, `psy_const.h:161`.
        assert_eq!(
            (
                SECT_ESC_VAL_LONG,
                SECT_ESC_VAL_SHORT,
                CODE_BOOK_BITS,
                SECT_BITS_LONG,
                SECT_BITS_SHORT
            ),
            (31, 7, 4, 5, 3)
        );
        assert_eq!(
            (GLOBAL_GAIN_OFFSET, NOISE_OFFSET, LOG_NORM_PCM),
            (100, 90, -15)
        );
        // `bit_cnt.h:164-177` LAVs; `PNS_PCM_BITS` shared with S5.
        assert_eq!(BOOK_LAV, [0, 1, 1, 2, 2, 4, 4, 7, 7, 12, 12]);
        assert_eq!(PNS_PCM_BITS, 9);
        // Code-table shapes pin the ROM dims (`aacEnc_rom.h:129-140`).
        assert_eq!(HUFF_CTAB1.len(), 81);
        assert_eq!(HUFF_CTAB2.len(), 81);
        assert_eq!(HUFF_CTAB3.len(), 81);
        assert_eq!(HUFF_CTAB4.len(), 81);
        assert_eq!(HUFF_CTAB5.len(), 81);
        assert_eq!(HUFF_CTAB6.len(), 81);
        assert_eq!(HUFF_CTAB7.len(), 64);
        assert_eq!(HUFF_CTAB8.len(), 64);
        assert_eq!(HUFF_CTAB9.len(), 169);
        assert_eq!(HUFF_CTAB10.len(), 169);
        assert_eq!(HUFF_CTAB11.len(), 21 * 17);
        assert_eq!(HUFF_CTABSCF.len(), 121);
        // Spot values (first entries + scf center + ESC corner).
        assert_eq!(HUFF_CTAB1[40], 0x0000);
        assert_eq!(HUFF_CTABSCF[60], 0x00000000);
        assert_eq!(bit_count_scf_delta(0), 1);
        assert_eq!(HUFF_LTAB11[16 * 17 + 16], 5);
    }

    #[test]
    fn s7_writer_packs_msb_first() {
        // Hand-computed packing (MSB-first, zero-padded tail).
        let mut w = BitWriter::new(4);
        w.write_bits(0b101, 3).unwrap();
        w.write_bits(0x00, 1).unwrap();
        w.write_bits(0xFF, 8).unwrap();
        assert_eq!(w.len_bits(), 12);
        assert_eq!(w.used_bytes(), &[0b1010_1111, 0b1111_0000]);
        assert_eq!(w.to_hex(), "aff0");
        // Zero-length write is a C-parity no-op.
        w.write_bits(0xFFFF_FFFF, 0).unwrap();
        assert_eq!(w.len_bits(), 12);
        // 32-bit straddle across the FDK 32-bit cache-flush boundary.
        let mut w2 = BitWriter::new(8);
        w2.write_bits(0xFFFF_FFFF, 32).unwrap();
        w2.write_bits(0x1, 1).unwrap();
        assert_eq!(w2.len_bits(), 33);
        assert_eq!(w2.used_bytes(), &[0xFF, 0xFF, 0xFF, 0xFF, 0x80]);
    }

    #[test]
    fn s7_writer_rejects_bad_inputs_and_overflow() {
        let mut w = BitWriter::new(1);
        assert_eq!(w.write_bits(0, 33), Err(BitEncError::InvalidLength(33)));
        assert_eq!(
            w.write_bits(0b1000, 3),
            Err(BitEncError::ValueTooLarge { value: 8, bits: 3 })
        );
        w.write_bits(0xAB, 8).unwrap();
        assert_eq!(
            w.write_bits(0, 1),
            Err(BitEncError::BufferOverflow {
                need_bits: 1,
                free_bits: 0
            })
        );
        // Zero-capacity writer overflows immediately.
        let mut w0 = BitWriter::new(0);
        assert!(w0.write_bits(0, 1).is_err());
    }

    #[test]
    fn s7_code_values_rejects_invalid_books_widths_values() {
        let mut w = BitWriter::new(64);
        // C default arm (12+) emits nothing; Rust refuses.
        for bad in [12, 13, 14, 15, -1, 16, 99] {
            assert_eq!(
                code_values(&[0, 0, 0, 0], bad, &mut w),
                Err(BitEncError::UnsupportedCodebook(bad)),
                "book {bad}"
            );
        }
        assert_eq!(w.len_bits(), 0, "no bits on refusal");
        // Width precondition (C would read the neighbouring band).
        assert_eq!(
            code_values(&[0, 0, 0], 1, &mut w),
            Err(BitEncError::InvalidWidth { book: 1, width: 3 })
        );
        assert_eq!(
            code_values(&[0, 0, 0], 11, &mut w),
            Err(BitEncError::InvalidWidth { book: 11, width: 3 })
        );
        // LAV precondition (C would OOB-read the tables).
        assert_eq!(
            code_values(&[2, 0, 0, 0], 1, &mut w),
            Err(BitEncError::ValueOutOfRange { book: 1, value: 2 })
        );
        assert_eq!(
            code_values(&[0, 13], 9, &mut w),
            Err(BitEncError::ValueOutOfRange { book: 9, value: 13 })
        );
        // Scalefactor twin: |delta| > 60 (C returns 1).
        assert_eq!(
            code_scalefactor_delta(61, &mut w),
            Err(BitEncError::ScalefactorDeltaOutOfRange(61))
        );
        assert_eq!(
            code_scalefactor_delta(-61, &mut w),
            Err(BitEncError::ScalefactorDeltaOutOfRange(-61))
        );
        assert_eq!(w.len_bits(), 0, "no bits on refusal");
    }

    #[test]
    fn s7_emitted_bits_match_s5_counters_on_probe_grid() {
        // Live S5-counter agreement on hand-picked valid vectors
        // (full-grid proof runs over the committed probe transcript).
        let cases: &[(&[i16], i32)] = &[
            (&[0, 0, 0, 0], 1),
            (&[1, -1, 1, -1], 2),
            (&[2, -2, 0, 1], 3),
            (&[-2, 2, -1, 0], 4),
            (&[4, -4, 0, 4], 5),
            (&[-3, 2, -1, 0], 6),
            (&[7, -7, 0, 1], 7),
            (&[0, -1, 5, -7], 8),
            (&[12, -12, 0, 3], 9),
            (&[-5, 0, 12, 12], 10),
            (&[16, -17, 0, 31], 11),
            (&[100, -200, 300, -400], 11),
        ];
        for (vals, book) in cases {
            let mut w = BitWriter::new(64);
            let emitted = code_values(vals, *book, &mut w).unwrap();
            assert_eq!(
                emitted,
                count_values(vals, *book) as u32,
                "book {book} vals {vals:?}"
            );
            assert_eq!(emitted as usize, w.len_bits(), "writer tally");
        }
        for d in [-60, -59, -1, 0, 1, 59, 60] {
            let mut w = BitWriter::new(8);
            let emitted = code_scalefactor_delta(d, &mut w).unwrap();
            assert_eq!(emitted, bit_count_scf_delta(d) as u32, "delta {d}");
            assert_eq!(emitted as usize, w.len_bits(), "writer tally");
        }
    }

    #[test]
    fn s7_section_headers_match_side_info_rule() {
        // Single-section escape folding: cnt 31 on LONG writes esc+0
        // (`while (sectLen >= 31)`), cnt 7 on SHORT likewise.
        let mk = |book: i32, cnt: i32, block: i32| SectionData {
            block_type: block,
            no_of_groups: 1,
            sfb_cnt: cnt,
            max_sfb_per_group: cnt,
            sfb_per_group: cnt,
            sections: vec![crate::aac_enc::bitcount::SectionInfo {
                code_book: book,
                sfb_start: 0,
                sfb_cnt: cnt,
                section_bits: 0,
            }],
            side_info_bits: 0,
            huffman_bits: 0,
            scalefac_bits: 0,
            noise_nrg_bits: 0,
            first_scf: 0,
        };
        let mut w = BitWriter::new(16);
        let b = encode_section_data(&mk(3, 31, 0), &mut w).unwrap();
        assert_eq!(b, 4 + 5 + 5, "book + esc(31) + rest(0)");
        assert_eq!(w.len_bits(), b as usize);
        let mut w = BitWriter::new(16);
        let b = encode_section_data(&mk(11, 7, 2), &mut w).unwrap();
        assert_eq!(b, 4 + 3 + 3, "book + esc(7) + rest(0)");
        // Invalid section geometry fails loudly (C masks/emits).
        let mut w = BitWriter::new(16);
        assert_eq!(
            encode_section_data(&mk(16, 2, 0), &mut w),
            Err(BitEncError::SectionBookOutOfRange(16))
        );
        assert_eq!(
            encode_section_data(&mk(3, -2, 0), &mut w),
            Err(BitEncError::NegativeSectionLen(-2))
        );
    }

    #[test]
    fn s7_pns_and_is_walks_use_primitives_exactly() {
        // Synthetic single-section PNS: 9-bit PCM head + DPCM tail with
        // exact shared-table lengths (C walk has no skip logic here).
        let sections = SectionData {
            block_type: 0,
            no_of_groups: 1,
            sfb_cnt: 3,
            max_sfb_per_group: 3,
            sfb_per_group: 3,
            sections: vec![crate::aac_enc::bitcount::SectionInfo {
                code_book: CODE_BOOK_PNS_NO,
                sfb_start: 0,
                sfb_cnt: 3,
                section_bits: 0,
            }],
            side_info_bits: 0,
            huffman_bits: 0,
            scalefac_bits: 0,
            noise_nrg_bits: 0,
            first_scf: 0,
        };
        // lastPns seed: gain 100, scf[0] = 60 -> 100 - 60 + 70 = 110.
        // noise = [110, 112, 109]: PCM(110 - 110 + 256 = 256) then
        // DPCM(2), DPCM(-3).
        let mut w = BitWriter::new(16);
        let b = encode_scale_factor_data(
            &[1, 1, 1],
            &sections,
            &[60, 61, 62],
            &[110, 112, 109],
            &[0, 0, 0],
            100,
            &mut w,
        )
        .unwrap();
        let expect = 9 + bit_count_scf_delta(2) + bit_count_scf_delta(-3);
        assert_eq!(b, expect as u32);
        assert_eq!(w.len_bits(), b as usize);
        // PCM word out of 9-bit range fails (C masks silently).
        let mut w = BitWriter::new(16);
        assert_eq!(
            encode_scale_factor_data(
                &[1],
                &SectionData {
                    sections: vec![crate::aac_enc::bitcount::SectionInfo {
                        code_book: CODE_BOOK_PNS_NO,
                        sfb_start: 0,
                        sfb_cnt: 1,
                        section_bits: 0,
                    }],
                    block_type: 0,
                    no_of_groups: 1,
                    sfb_cnt: 1,
                    max_sfb_per_group: 1,
                    sfb_per_group: 1,
                    side_info_bits: 0,
                    huffman_bits: 0,
                    scalefac_bits: 0,
                    noise_nrg_bits: 0,
                    first_scf: 0,
                },
                &[0],
                &[10000],
                &[0],
                0,
                &mut w,
            ),
            Err(BitEncError::PnsPcmOutOfRange(10000 - 70))
        );
        // IS: plain isScale DPCM chain from lastValIs = 0.
        let mut w = BitWriter::new(16);
        let b = encode_scale_factor_data(
            &[1, 1],
            &SectionData {
                sections: vec![crate::aac_enc::bitcount::SectionInfo {
                    code_book: CODE_BOOK_IS_IN_PHASE_NO,
                    sfb_start: 0,
                    sfb_cnt: 2,
                    section_bits: 0,
                }],
                block_type: 0,
                no_of_groups: 1,
                sfb_cnt: 2,
                max_sfb_per_group: 2,
                sfb_per_group: 2,
                side_info_bits: 0,
                huffman_bits: 0,
                scalefac_bits: 0,
                noise_nrg_bits: 0,
                first_scf: 0,
            },
            &[0, 0],
            &[0, 0],
            &[3, -2],
            0,
            &mut w,
        )
        .unwrap();
        assert_eq!(b, (bit_count_scf_delta(3) + bit_count_scf_delta(-5)) as u32);
        // Spectral traversal skips PNS/IS with zero bits (C parity: the
        // `codeValues` default arm is a silent no-op for books 13-15).
        let mut w = BitWriter::new(16);
        let b = encode_spectral_data(&[5, -5, 7, 7], &[0, 2, 4], &sections, &mut w).unwrap();
        assert_eq!(b, 0);
        assert!(w.is_empty());
    }

    #[test]
    fn s7_probe_transcript_matches_c_bit_exact() {
        // Every committed probe vector through the real `codeValues` /
        // `codeScalefactorDelta`: exact bits + exact hex (MSB-first bytes).
        // `rc=1` lines pin the C `|delta| > 60` error (zero bits emitted);
        // invalid-book CV lines pin C's silent default arm, which Rust
        // refuses explicitly instead.
        let cases = crate::aac_enc::testdata::load_s7probe();
        let mut n_cv = 0;
        let mut n_cs = 0;
        for c in &cases {
            match c {
                crate::aac_enc::testdata::S7Probe::CodeValues {
                    book,
                    vals,
                    bits,
                    hex,
                } => {
                    n_cv += 1;
                    let mut w = BitWriter::new(64);
                    if *book == 12 || *book == 14 || *book == -1 {
                        assert_eq!(*bits, 0, "C default arm emits nothing");
                        assert!(hex.is_empty());
                        assert_eq!(
                            code_values(vals, *book, &mut w),
                            Err(BitEncError::UnsupportedCodebook(*book)),
                            "book {book}"
                        );
                        continue;
                    }
                    let emitted = code_values(vals, *book, &mut w)
                        .unwrap_or_else(|e| panic!("book {book} vals {vals:?}: {e}"));
                    assert_eq!(emitted, *bits, "book {book} vals {vals:?}");
                    assert_eq!(&w.to_hex(), hex, "book {book} vals {vals:?}");
                    // Independent S5-counter agreement on every valid
                    // transcript vector (never fixture replay).
                    // `countValues` strides 4 for books 7..10, so
                    // length-2 pair vectors are checked doubled: pairs are
                    // independent in `countValues` (`bit_cnt.cpp:642-726`,
                    // per-pair terms) and in `codeValues`, so the doubled
                    // count is exactly twice the pair cost.
                    let counted = if vals.len() % 4 == 0 {
                        count_values(vals, *book) as u32
                    } else {
                        let mut doubled = vals.to_vec();
                        doubled.extend_from_slice(vals);
                        (count_values(&doubled, *book) / 2) as u32
                    };
                    assert_eq!(emitted, counted, "countValues agreement book {book}");
                }
                crate::aac_enc::testdata::S7Probe::CodeDelta {
                    delta,
                    rc,
                    bits,
                    hex,
                } => {
                    n_cs += 1;
                    let mut w = BitWriter::new(8);
                    if *rc == 1 {
                        assert_eq!(*bits, 0);
                        assert!(hex.is_empty());
                        assert_eq!(
                            code_scalefactor_delta(*delta, &mut w),
                            Err(BitEncError::ScalefactorDeltaOutOfRange(*delta)),
                            "delta {delta}"
                        );
                        continue;
                    }
                    let emitted = code_scalefactor_delta(*delta, &mut w)
                        .unwrap_or_else(|e| panic!("delta {delta}: {e}"));
                    assert_eq!(emitted, *bits, "delta {delta}");
                    assert_eq!(&w.to_hex(), hex, "delta {delta}");
                    assert_eq!(
                        emitted,
                        bit_count_scf_delta(*delta) as u32,
                        "counter agreement delta {delta}"
                    );
                }
            }
        }
        assert!(n_cv > 150, "CV coverage: {n_cv}");
        assert_eq!(n_cs, 123, "full -61..=61 CS sweep: {n_cs}");
    }

    #[test]
    fn s7_corpus_payloads_match_c_and_s5() {
        // End-to-end S7 over the committed corpus (20 element-frames, 36
        // channels): S4 inv=2 lines/scf/gain + S5 sections/geometry drive
        // the three payload writers; outputs must equal the oracle hex
        // (real C encode functions) bit-for-bit AND the live-recomputed
        // S5 counters (the WRITE_SEC/SCAL/SPEC agreement, strict).
        const S7_CASES: [(&str, u32); 20] = [
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
        for (name, frame) in S7_CASES {
            let tag = format!("{name} f={frame}");
            let s4 = crate::aac_enc::testdata::load_s4(name, frame);
            let s5 = crate::aac_enc::testdata::load_s5(name, frame);
            let s7 = crate::aac_enc::testdata::load_s7(name, frame);
            assert_eq!(s4.ch.len(), s5.ch.len(), "{tag} channels");
            assert_eq!(s4.ch.len(), s7.ch.len(), "{tag} channels");
            for ((c4, c5), c7) in s4.ch.iter().zip(s5.ch.iter()).zip(s7.ch.iter()) {
                let level = c4
                    .levels
                    .iter()
                    .find(|l| l.inv == 2)
                    .unwrap_or_else(|| panic!("{tag} inv=2 level"));
                // Live S5 recount on the same inputs (input-validity
                // proof; never fixture replay for the agreement below).
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
                assert_eq!(sections.huffman_bits, c5.huff, "{tag} huff");
                assert_eq!(sections.side_info_bits, c5.side, "{tag} side");
                assert_eq!(sections.scalefac_bits, c5.scf_bits, "{tag} scf");
                assert_eq!(sections.noise_nrg_bits, c5.noise_bits, "{tag} noise");
                assert_eq!(sections.first_scf, c5.first, "{tag} first");
                // Spectral payload (S8 WRITE_SPEC agreement).
                let mut w = BitWriter::new(2048);
                let spec = encode_spectral_data(&level.qspec, &c5.offsets, &sections, &mut w)
                    .unwrap_or_else(|e| panic!("{tag} spectral: {e}"));
                assert_eq!(spec, sections.huffman_bits as u32, "{tag} SPEC count");
                assert_eq!(spec, c7.spec_bits, "{tag} SPEC C bits");
                assert_eq!(&w.to_hex(), &c7.spec_hex, "{tag} SPEC C hex");
                // Scalefactor payload (S8 WRITE_SCAL agreement).
                let mut w = BitWriter::new(2048);
                let scf = encode_scale_factor_data(
                    &c5.qmax,
                    &sections,
                    &level.scf,
                    &c5.noise_nrg,
                    &c5.is_scale,
                    level.gain,
                    &mut w,
                )
                .unwrap_or_else(|e| panic!("{tag} scf: {e}"));
                assert_eq!(
                    scf,
                    (sections.scalefac_bits + sections.noise_nrg_bits) as u32,
                    "{tag} SCF count"
                );
                assert_eq!(scf, c7.scf_bits, "{tag} SCF C bits");
                assert_eq!(&w.to_hex(), &c7.scf_hex, "{tag} SCF C hex");
                // Section headers (S8 WRITE_SEC agreement).
                let mut w = BitWriter::new(2048);
                let sech = encode_section_data(&sections, &mut w)
                    .unwrap_or_else(|e| panic!("{tag} section: {e}"));
                assert_eq!(sech, sections.side_info_bits as u32, "{tag} SEC count");
                assert_eq!(sech, c7.sech_bits, "{tag} SEC C bits");
                assert_eq!(&w.to_hex(), &c7.sech_hex, "{tag} SEC C hex");
            }
        }
    }
}
