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
//! Exact Q31 integer primitives mirroring the C fixed-point operations.
//!
//! Shared by the energy and threshold code so the integer mirror lives in
//! exactly one place. Semantics verified against `cpp/libFDK/include/` **as
//! built on this machine (ARM64)**, which selects arch-specific variants
//! that differ from the generic C fallbacks in LSB rounding:
//! - `fmult(a, b)`: ARM64 `fixmul_DD` (`smull; asr #31`), i.e.
//!   `floor(a*b/2^31)` narrowed to 32 bits — it KEEPS bit 0, unlike the
//!   generic `((a*b)>>32)<<1` (floor-to-even). (`fixmul_arm.h:174-193`.)
//!   Verified case: `fMult(0.5, 10)` is 5, not 4.
//! - `fmult_div2_64`/`fpow2_add_div2`: identical on all variants
//!   (`asr #32`, `fixmul.h`/`fixmadd.h`).
//! - `fmult_sgl(sgl, dbl)`: `fixmul_SD`, `(((sgl<<16)*b)>>32)<<1`; used for
//!   the 16-bit pre-echo factor (`fixmul.h:178`).
//! - `shift_int`: `scaleValue`, arithmetic `>>`, wrapping `<<`
//!   (`scale.h:153-160`).
//! - `count_leading_zeros`: `fixnormz_D` for non-negative input
//!   (`clz.h:152-160`); `count_leading_bits`: `fixnorm_D`
//!   (`clz.h:187-200`).
//!
//! Integer widths: plain `+ - << >>` ops run in `i64`, matching LP64
//! `LONG` (no 32-bit wrap); narrowing happens ONLY at `fMult`-family
//! boundaries, which take/return 32-bit `INT` on ARM64. C's headroom
//! design keeps every intermediate in 32-bit range, so the distinction
//! never fires in practice — bit-exact RXX agreement is the check.

/// Wrap an `i64` intermediate to Q31 `i32` (C two's-complement wrap).
pub(crate) fn wrap32(v: i64) -> i32 {
    (v & 0xFFFF_FFFF) as u32 as i32
}

/// `fMult(a, b)` for Q31 ints, ARM64 semantics: `floor(a*b/2^31)`
/// narrowed to 32 bits (Rust `as` wrap == C `(INT)` narrow).
pub(crate) fn fmult(a: i32, b: i32) -> i32 {
    (((a as i64) * (b as i64)) >> 31) as i32
}

/// `fMult` with a 16-bit first operand: sign-extend to Q31 (`FX_SGL2FX_DBL`)
/// then plain `fMult` (`fixmul.h:178`).
pub(crate) fn fmult_sgl(a: i16, b: i32) -> i32 {
    fmult((a as i32) << 16, b)
}

/// `fPow2AddDiv2(t, s) = t + ((s*s)>>32)` with wrap.
pub(crate) fn fpow2_add_div2(t: i32, s: i32) -> i32 {
    wrap32(t as i64 + ((s as i64) * (s as i64) >> 32))
}

/// `scaleValueSaturate` (`scale.h:171-199`): saturating shift.
pub(crate) fn scale_value_saturate(v: i32, s: i32) -> i32 {
    const MIN_PLUS_ONE: i32 = i32::MIN + 1;
    let headroom = if v >= 0 {
        (v as u32).leading_zeros() as i32
    } else {
        (!v as u32).leading_zeros() as i32
    };
    if s >= 0 {
        if headroom <= s {
            if v > 0 {
                i32::MAX
            } else {
                MIN_PLUS_ONE
            }
        } else {
            (v << s).max(MIN_PLUS_ONE)
        }
    } else {
        let k = -s;
        if (32 - headroom) <= k {
            0
        } else {
            (v >> k).max(MIN_PLUS_ONE)
        }
    }
}
/// `fMultDiv2(a, b) = (a*b)>>32` in 64-bit (LP64 `LONG`) precision.
/// Matches every arch variant (C shift, SMMUL/SMULL asm): all compute the
/// truncated high-half product identically.
pub(crate) fn fmult_div2_64(a: i64, b: i64) -> i64 {
    ((a as i128 * b as i128) >> 32) as i64
}

/// `fMultAddDiv2(x, a, b) = x + ((a*b)>>32)` (`fixmadd.h`).
/// Wrapping add mirrors C unsigned-long wrap on overflow (unreachable in
/// the merge: values stay in 32-bit range by headroom design).
pub(crate) fn fmult_add_div2_64(x: i64, a: i64, b: i64) -> i64 {
    x.wrapping_add(fmult_div2_64(a, b))
}

/// `scaleValue` in 64-bit (LP64 `LONG`): arithmetic `>>`, non-wrapping
/// `<<` — identical to the wrapping 32-bit `shift_int` wherever C's
/// headroom design prevents 32-bit overflow (verified by bit-exact RXX
/// agreement; any divergence would fail loudly there).
pub(crate) fn scale_value_64(v: i64, s: i32) -> i64 {
    if s >= 0 {
        v << s
    } else {
        v >> (-s as u32).min(63)
    }
}

/// `fixp_abs` for 64-bit (LP64 `fAbs`): negation WITHOUT 32-bit wrap, so
/// `abs(MINVAL)` = 2^31 (differs from 32-bit-ARM wrap; matches macOS).
pub(crate) fn fixp_abs_64(v: i64) -> i64 {
    if v < 0 {
        -v
    } else {
        v
    }
}

/// `fixnormz_D` (leading zeros) for non-negative values in 32-bit range.
/// Matches the generic C loop and ARM `clz %w` identically there
/// (both effectively count 32-bit leading zeros; merge energies always
/// fit — bit-exact RXX agreement is the check).
pub(crate) fn fixnormz_32(v: i32) -> i32 {
    (v as u32).leading_zeros() as i32
}

/// Arithmetic shift right / wrapping shift left (mirrors `scaleValue`).
pub(crate) fn shift_int(v: i32, s: i32) -> i32 {
    if s >= 0 {
        (v as u32).wrapping_shl(s as u32) as i32
    } else {
        v >> (-s as u32).min(31)
    }
}

/// `CntLeadingZeros` for non-negative ints (0 -> 32).
pub(crate) fn count_leading_zeros(v: i32) -> i32 {
    debug_assert!(v >= 0);
    (v as u32).leading_zeros() as i32
}

/// `CountLeadingBits`: 0 -> 0, else leading-sign-bits minus one.
pub(crate) fn count_leading_bits(v: i32) -> i32 {
    if v == 0 {
        0
    } else {
        let a = if v < 0 { !v } else { v };
        (a as u32).leading_zeros() as i32 - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_match_c_semantics() {
        // fMult(1.0, 1.0) in Q31: floor((2^62-2^33+1)/2^31) = 2^31-2.
        assert_eq!(fmult(0x7FFF_FFFF, 0x7FFF_FFFF), 0x7FFF_FFFE);
        assert_eq!(fmult(0, 12345), 0);
        assert_eq!(fmult(0x4000_0000, 0x4000_0000), 0x2000_0000);
        // ARM64 floor keeps bit 0 where generic-C clears it.
        assert_eq!(fmult(0x4000_0000, 10), 5);
        assert_eq!(fmult(0x4000_0000, 0x3FFF_FFFF), 0x1FFF_FFFF);
        // 16-bit factor path: 0x0148/2^15 times 2^30 = 0xA40000.
        assert_eq!(fmult_sgl(0x0148, 0x4000_0000), 0x00A4_0000);
        // fPow2AddDiv2 accumulates half-squares: (2^30)^2>>32 = 2^28.
        assert_eq!(fpow2_add_div2(0, 0x4000_0000), 0x1000_0000);
        // Shifts: arithmetic right, wrapping left.
        assert_eq!(shift_int(-8, -2), -2);
        assert_eq!(shift_int(0x4000_0000, 1), -0x8000_0000i32);
        // clz/bits.
        assert_eq!(count_leading_zeros(0), 32);
        assert_eq!(count_leading_zeros(1), 31);
        assert_eq!(count_leading_zeros(0x4000_0000), 1);
        assert_eq!(count_leading_bits(0), 0);
        assert_eq!(count_leading_bits(1), 30);
        assert_eq!(count_leading_bits(0x4000_0000), 0);
        assert_eq!(count_leading_bits(-1), 31);
        // wrap32 wraps (C two's-complement behavior).
        assert_eq!(wrap32(0x1_FFFF_FFFF), -1);
    }
}
