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

/// `fixnorm_D` (`clz.h:193-202`): 0 for zero input, else `clz-1` over the
/// magnitude (`~v` for negatives).
pub(crate) fn fixnorm_d(v: i32) -> i32 {
    if v == 0 {
        0
    } else {
        let a = if v < 0 { !v } else { v };
        (a as u32).leading_zeros() as i32 - 1
    }
}

/// `fMultDiv2(a, b)` narrowed to `FIXP_DBL`: `(a*b)>>32` in 64-bit precision,
/// then wrapped to 32 bits (LP64 `LONG` product, `FIXP_DBL` result).
pub(crate) fn fmult_div2(a: i32, b: i32) -> i32 {
    wrap32(fmult_div2_64(a as i64, b as i64))
}

/// `fMultAddDiv2(x, a, b)` narrowed to `FIXP_DBL`: `x + ((a*b)>>32)`.
/// In-range callers (Taylor loop, sqrt) never wrap; the wrap documents the
/// LP64 `LONG` add + `FIXP_DBL` narrowing of `fixmadd.h`.
pub(crate) fn fmult_add_div2(x: i32, a: i32, b: i32) -> i32 {
    wrap32(x as i64 + fmult_div2_64(a as i64, b as i64))
}

/// `fMultNorm(f1, f2)` (`fixpoint_math.cpp`): maximum-precision multiply.
/// Returns `(mantissa, exponent)`; mirrors the `MINVAL*MINVAL` corner.
pub(crate) fn fmult_norm(f1: i32, f2: i32) -> (i32, i32) {
    if f1 == 0 || f2 == 0 {
        return (0, 0);
    }
    let norm_f1 = count_leading_bits(f1);
    let f1n = shift_int(f1, norm_f1);
    let norm_f2 = count_leading_bits(f2);
    let f2n = shift_int(f2, norm_f2);
    if f1n == i32::MIN && f2n == i32::MIN {
        (-(i32::MIN >> 1), -(norm_f1 + norm_f2 - 1))
    } else {
        (fmult(f1n, f2n), -(norm_f1 + norm_f2))
    }
}

/// `fMultI(a, b)` (`fixpoint_math.h`): fractional-times-integer with
/// round-to-nearest.
pub(crate) fn fmult_i(a: i32, b: i32) -> i32 {
    let (m, m_e) = fmult_norm(a, b);
    if m_e < 0 {
        if m_e > -32 {
            m.wrapping_shr(((-m_e) - 1) as u32)
                .wrapping_add(1)
                .wrapping_shr(1)
        } else {
            0
        }
    } else {
        scale_value_saturate(m, m_e)
    }
}

/// `schur_div(num, denum, count)` (`fixpoint_math.cpp`): restoring division
/// delivering `count`-bit accuracy. Callers guarantee `0 <= num <= denum`
/// with `denum > 0` (loud failure otherwise, as in C `FDK_ASSERT`).
pub(crate) fn schur_div(num: i32, denum: i32, count: i32) -> i32 {
    debug_assert!(num >= 0 && denum > 0 && num <= denum);
    let mut l_num = (num as i64) >> 1;
    let l_denum = (denum as i64) >> 1;
    let mut div: i64 = 0;
    let mut k = count;
    if l_num != 0 {
        loop {
            k -= 1;
            if k == 0 {
                break;
            }
            div <<= 1;
            l_num <<= 1;
            if l_num >= l_denum {
                l_num -= l_denum;
                div += 1;
            }
        }
    }
    (div << (32 - count)) as i32
}

/// `fDivNorm(num, denom)` 3-operand core (`fixpoint_math.cpp`): returns
/// `(mantissa, exponent)` with `num/denom = mantissa * 2^exponent`.
pub(crate) fn fdiv_norm_e(num: i32, denom: i32) -> (i32, i32) {
    debug_assert!(num >= 0 && denom > 0);
    if num == 0 {
        return (0, 0);
    }
    let norm_num = count_leading_bits(num);
    let l_num = shift_int(num, norm_num) >> 1;
    let mut result_e = -norm_num + 1;
    let norm_den = count_leading_bits(denom);
    let l_denum = shift_int(denom, norm_den);
    result_e -= -norm_den;
    (schur_div(l_num, l_denum, 16), result_e)
}

/// `fDivNorm(num, denom)` (`fixpoint_math.cpp`): positive-operand division
/// normalized to exponent 0, with the `0x40000000,e==1` saturation edge.
pub(crate) fn fdiv_norm(num: i32, denom: i32) -> i32 {
    debug_assert!(denom >= num);
    let (mut res, e) = fdiv_norm_e(num, denom);
    if res == (1 << 30) && e == 1 {
        res = i32::MAX;
    } else {
        res = shift_int(res, e);
    }
    res
}

/// Taylor coefficients of ln(1-x) (`fixpoint_math.h:123-130`). On this
/// target (`__arm__ && __ARM_ARCH_8__`) `FDK_archdef.h` selects
/// `LDCOEFF_16BIT`, so the table holds `FL2FXCONST_SGL(-1.0/n)` widened to
/// Q31 (low 16 bits zero) — NOT the 32-bit `FL2FXCONST_DBL` values.
/// Literals verified with a C probe of the widened SGL values.
const LD_COEFF: [i32; 10] = [
    0x80000000u32 as i32,
    0xC0000000u32 as i32,
    0xD5550000u32 as i32,
    0xE0000000u32 as i32,
    0xE6660000u32 as i32,
    0xEAAB0000u32 as i32,
    0xEDB70000u32 as i32,
    0xF0000000u32 as i32,
    0xF1C70000u32 as i32,
    0xF3330000u32 as i32,
];

/// `2.0 * (1/ln 2)` tail constant of the `fLog2` Taylor evaluation
/// (`fixpoint_math.h:851-854`), C-probed Q31 literal.
const LOG2_TAIL_MUL: i32 = 0x71547653;

/// `fLog2(x_m, x_e)` (`fixpoint_math.h:807-885`): log2 with implicit output
/// exponent `LD_DATA_SHIFT` (i.e. `CalcLdData` when `x_e == 0`).
/// Non-positive input yields the `-1.0` sentinel, matching C.
pub(crate) fn calc_ld_data(x: i32) -> i32 {
    calc_ld_data_e(x, 0)
}

/// Full `fLog2` with explicit input exponent (S1 uses exponent 0 only).
fn calc_ld_data_e(x_m: i32, x_e: i32) -> i32 {
    if x_m <= 0 {
        return i32::MIN; // FL2FXCONST_DBL(-1.0f)
    }
    // Move input toward 1.0 where the Taylor approximation is accurate.
    let b_norm = count_leading_zeros(x_m) - 1;
    let mut x2 = shift_int(x_m, b_norm);
    let xe = x_e - b_norm;
    // Map to the log(1-x) domain: x2 = -(x2 + -1.0), wrapping like C.
    x2 = x2.wrapping_add(i32::MIN).wrapping_neg();
    // Taylor polynomial of ln(1-x), degree 10.
    let mut result_m: i32 = 0;
    let mut px = x2;
    for &c in LD_COEFF.iter() {
        result_m = fmult_add_div2(result_m, c, px);
        px = fmult(px, x2);
    }
    // Convert ln to log2.
    result_m = fmult_add_div2(result_m, result_m, LOG2_TAIL_MUL);
    let result_e: i32;
    if xe != 0 {
        let enorm = 32 - fixnorm_d(xe);
        result_m = shift_int(result_m, -(enorm - 1)).wrapping_add(shift_int(xe, 31 - enorm));
        result_e = enorm;
    } else {
        result_e = 1;
    }
    shift_int(result_m, result_e - 6)
}

/// 2^x lookup tables for `CalcInvLdData` (`fixpoint_math.cpp:208-240`),
/// verbatim literals.
const EXP2_TAB: [u32; 32] = [
    0x40000000, 0x4166C34C, 0x42D561B4, 0x444C0740, 0x45CAE0F2, 0x47521CC6, 0x48E1E9BA, 0x4A7A77D4,
    0x4C1BF829, 0x4DC69CDD, 0x4F7A9930, 0x51382182, 0x52FF6B55, 0x54D0AD5A, 0x56AC1F75, 0x5891FAC1,
    0x5A82799A, 0x5C7DD7A4, 0x5E8451D0, 0x60962665, 0x62B39509, 0x64DCDEC3, 0x6712460B, 0x69540EC9,
    0x6BA27E65, 0x6DFDDBCC, 0x70666F76, 0x72DC8374, 0x75606374, 0x77F25CCE, 0x7A92BE8B, 0x7D41D96E,
];
const EXP2W_TAB: [u32; 32] = [
    0x40000000, 0x400B1818, 0x4016321B, 0x40214E0C, 0x402C6BE9, 0x40378BB4, 0x4042AD6D, 0x404DD113,
    0x4058F6A8, 0x40641E2B, 0x406F479E, 0x407A7300, 0x4085A051, 0x4090CF92, 0x409C00C4, 0x40A733E6,
    0x40B268FA, 0x40BD9FFF, 0x40C8D8F5, 0x40D413DD, 0x40DF50B8, 0x40EA8F86, 0x40F5D046, 0x410112FA,
    0x410C57A2, 0x41179E3D, 0x4122E6CD, 0x412E3152, 0x41397DCC, 0x4144CC3B, 0x41501CA0, 0x415B6EFB,
];
const EXP2X_TAB: [u32; 32] = [
    0x40000000, 0x400058B9, 0x4000B173, 0x40010A2D, 0x400162E8, 0x4001BBA3, 0x4002145F, 0x40026D1B,
    0x4002C5D8, 0x40031E95, 0x40037752, 0x4003D011, 0x400428CF, 0x4004818E, 0x4004DA4E, 0x4005330E,
    0x40058BCE, 0x4005E48F, 0x40063D51, 0x40069613, 0x4006EED5, 0x40074798, 0x4007A05B, 0x4007F91F,
    0x400851E4, 0x4008AAA8, 0x4009036E, 0x40095C33, 0x4009B4FA, 0x400A0DC0, 0x400A6688, 0x400ABF4F,
];

/// `CalcInvLdData(x)` (`fixpoint_math.h:228-256`): inverse log-domain
/// (`2^(x*64)` in Q31) via three-level table lookup. Bit-level mirror
/// including the `-31/64` zero clamp and the `>= 31/64 or == 0` saturate.
pub(crate) fn calc_inv_ld_data(x: i32) -> i32 {
    const NEG_LIM: i32 = 0xC2000000u32 as i32; // FL2FXCONST_DBL(-31.0/64.0)
    const POS_LIM: i32 = 0x3E000000; // FL2FXCONST_DBL(31.0/64.0)
    let set_zero = if x < NEG_LIM { 0u32 } else { 1u32 };
    let set_max = (x >= POS_LIM) || (x == 0);
    let frac = ((x as i64) & 0x3FF) as i32;
    let index3 = ((x >> 10) & 0x1F) as usize;
    let index2 = ((x >> 15) & 0x1F) as usize;
    let index1 = ((x >> 20) & 0x1F) as usize;
    let shifted25 = x >> 25;
    let exp = 31.min(if x > 0 { 31 - shifted25 } else { -shifted25 });
    let lookup1 = EXP2_TAB[index1].wrapping_mul(set_zero);
    let lookup2 = EXP2W_TAB[index2];
    let lookup3 = EXP2X_TAB[index3];
    // Mixed-width `fMultDiv2(DBL, SGL)` overload (`fixpoint_math.h:252`):
    // generic `fixmuldiv2_DS(a, b) = fixmuldiv2_DD(a, FX_SGL2FX_DBL(b))`,
    // i.e. the 10-bit fraction is widened by 16 before the DD multiply.
    let lookup3f = lookup3.wrapping_add(fmult_div2(0x0016302F, frac.wrapping_shl(16)) as u32);
    let lookup12 = fmult(lookup1 as i32, lookup2 as i32) as u32;
    let lookup = fmult(lookup12 as i32, lookup3f as i32) as u32;
    // Negative `exp` is C-undefined (shift by negative) and only reachable
    // when `set_max` discards the result; the masked shift keeps this total
    // with the same observable outcome (MAXVAL).
    let mut ret = lookup.wrapping_shl(3).wrapping_shr(exp as u32) as i32;
    if set_max {
        ret = i32::MAX;
    }
    ret
}

/// Integer-log table for `CalcLdInt` (`fixpoint_math.cpp`, `LD_INT_TAB_LEN`
/// 193 build), verbatim literals. Index `i` holds `log2(i)/64` in Q31.
const LD_INT_COEFF: [i32; 193] = [
    0x80000001u32 as i32,
    0x00000000,
    0x02000000,
    0x032B8034,
    0x04000000,
    0x04A4D3C2,
    0x052B8034,
    0x059D5DA0,
    0x06000000,
    0x06570069,
    0x06A4D3C2,
    0x06EB3A9F,
    0x072B8034,
    0x0766A009,
    0x079D5DA0,
    0x07D053F7,
    0x08000000,
    0x082CC7EE,
    0x08570069,
    0x087EF05B,
    0x08A4D3C2,
    0x08C8DDD4,
    0x08EB3A9F,
    0x090C1050,
    0x092B8034,
    0x0949A785,
    0x0966A009,
    0x0982809D,
    0x099D5DA0,
    0x09B74949,
    0x09D053F7,
    0x09E88C6B,
    0x0A000000,
    0x0A16BAD3,
    0x0A2CC7EE,
    0x0A423162,
    0x0A570069,
    0x0A6B3D79,
    0x0A7EF05B,
    0x0A92203D,
    0x0AA4D3C2,
    0x0AB7110E,
    0x0AC8DDD4,
    0x0ADA3F60,
    0x0AEB3A9F,
    0x0AFBD42B,
    0x0B0C1050,
    0x0B1BF312,
    0x0B2B8034,
    0x0B3ABB40,
    0x0B49A785,
    0x0B584822,
    0x0B66A009,
    0x0B74B1FD,
    0x0B82809D,
    0x0B900E61,
    0x0B9D5DA0,
    0x0BAA708F,
    0x0BB74949,
    0x0BC3E9CA,
    0x0BD053F7,
    0x0BDC899B,
    0x0BE88C6B,
    0x0BF45E09,
    0x0C000000,
    0x0C0B73CB,
    0x0C16BAD3,
    0x0C21D671,
    0x0C2CC7EE,
    0x0C379085,
    0x0C423162,
    0x0C4CABA8,
    0x0C570069,
    0x0C6130AF,
    0x0C6B3D79,
    0x0C7527B9,
    0x0C7EF05B,
    0x0C88983F,
    0x0C92203D,
    0x0C9B8926,
    0x0CA4D3C2,
    0x0CAE00D2,
    0x0CB7110E,
    0x0CC0052B,
    0x0CC8DDD4,
    0x0CD19BB0,
    0x0CDA3F60,
    0x0CE2C97D,
    0x0CEB3A9F,
    0x0CF39355,
    0x0CFBD42B,
    0x0D03FDA9,
    0x0D0C1050,
    0x0D140CA0,
    0x0D1BF312,
    0x0D23C41D,
    0x0D2B8034,
    0x0D3327C7,
    0x0D3ABB40,
    0x0D423B08,
    0x0D49A785,
    0x0D510118,
    0x0D584822,
    0x0D5F7CFF,
    0x0D66A009,
    0x0D6DB197,
    0x0D74B1FD,
    0x0D7BA190,
    0x0D82809D,
    0x0D894F75,
    0x0D900E61,
    0x0D96BDAD,
    0x0D9D5DA0,
    0x0DA3EE7F,
    0x0DAA708F,
    0x0DB0E412,
    0x0DB74949,
    0x0DBDA072,
    0x0DC3E9CA,
    0x0DCA258E,
    0x0DD053F7,
    0x0DD6753E,
    0x0DDC899B,
    0x0DE29143,
    0x0DE88C6B,
    0x0DEE7B47,
    0x0DF45E09,
    0x0DFA34E1,
    0x0E000000,
    0x0E05BF94,
    0x0E0B73CB,
    0x0E111CD2,
    0x0E16BAD3,
    0x0E1C4DFB,
    0x0E21D671,
    0x0E275460,
    0x0E2CC7EE,
    0x0E323143,
    0x0E379085,
    0x0E3CE5D8,
    0x0E423162,
    0x0E477346,
    0x0E4CABA8,
    0x0E51DAA8,
    0x0E570069,
    0x0E5C1D0B,
    0x0E6130AF,
    0x0E663B74,
    0x0E6B3D79,
    0x0E7036DB,
    0x0E7527B9,
    0x0E7A1030,
    0x0E7EF05B,
    0x0E83C857,
    0x0E88983F,
    0x0E8D602E,
    0x0E92203D,
    0x0E96D888,
    0x0E9B8926,
    0x0EA03232,
    0x0EA4D3C2,
    0x0EA96DF0,
    0x0EAE00D2,
    0x0EB28C7F,
    0x0EB7110E,
    0x0EBB8E96,
    0x0EC0052B,
    0x0EC474E4,
    0x0EC8DDD4,
    0x0ECD4012,
    0x0ED19BB0,
    0x0ED5F0C4,
    0x0EDA3F60,
    0x0EDE8797,
    0x0EE2C97D,
    0x0EE70525,
    0x0EEB3A9F,
    0x0EEF69FF,
    0x0EF39355,
    0x0EF7B6B4,
    0x0EFBD42B,
    0x0EFFEBCD,
    0x0F03FDA9,
    0x0F0809CF,
    0x0F0C1050,
    0x0F10113B,
    0x0F140CA0,
    0x0F18028D,
    0x0F1BF312,
    0x0F1FDE3D,
    0x0F23C41D,
    0x0F27A4C0,
    0x0F2B8034,
];

/// `CalcLdInt(i)` (`fixpoint_math.cpp:378-392`): table lookup for
/// `1 <= i < 193`, else exactly 0.
pub(crate) fn calc_ld_int(i: i32) -> i32 {
    if i > 0 && i < 193 {
        LD_INT_COEFF[i as usize]
    } else {
        0
    }
}

/// `invSqrtNorm2(op)` (`fixpoint_math.h`, linear+HQ interpolation path):
/// `1/sqrt(op)` mantissa in `.5..1` plus output shift. Zero input yields
/// `(MAXVAL, 16)`; all arithmetic mirrors the C `FIXP_DBL` (wrapping)
/// operations with the ARM64 `fMult` floor primitive.
pub(crate) fn inv_sqrt_norm2(op: i32) -> (i32, i32) {
    use super::sfb_tables::INV_SQRT_TAB;
    if op == 0 {
        return (i32::MAX, 16);
    }
    debug_assert!(op > 0);
    let mut shift = count_leading_zeros(op) - 1;
    let val = shift_int(op, shift);
    shift += 2;
    let index = ((val >> (32 - 1 - 8)) & 0x7F) as usize;
    let mut fract = (((val as u32) & 0x007F_FFFF) << 8) as i32;
    let mut diff = INV_SQRT_TAB[index + 1].wrapping_sub(INV_SQRT_TAB[index]);
    let mut reg1 = INV_SQRT_TAB[index].wrapping_add(fmult_div2(diff, fract).wrapping_shl(1));
    if fract != 0 {
        fract = fmult_div2(fract, 0x80000000u32.wrapping_sub(fract as u32) as i32).wrapping_shl(1);
        diff = diff.wrapping_sub(INV_SQRT_TAB[index + 2].wrapping_sub(INV_SQRT_TAB[index + 1]));
        reg1 = fmult_add_div2(reg1, fract, diff);
    }
    if shift & 1 != 0 {
        reg1 = fmult_div2(reg1, 0x5A827999).wrapping_shl(2);
    }
    (reg1, shift >> 1)
}

/// `sqrtFixp(op)` (`fixpoint_math.h:377-383`): fixed-point square root via
/// `invSqrtNorm2`, mirroring the shifts/narrowing literally.
pub(crate) fn sqrt_fixp(op: i32) -> i32 {
    let (tmp_inv, tmp_exp) = inv_sqrt_norm2(op);
    debug_assert!(tmp_exp > 0);
    fmult_div2(shift_int(op, tmp_exp - 1), tmp_inv).wrapping_shl(2)
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

    /// S1 fixmath kernels against C-probed anchors
    /// (`tools/enc-oracle` probe, `fixpoint_math.h`). Every value below is
    /// the exact output of the reference build, not a recomputation.
    #[test]
    fn s1_kernels_match_c_anchors() {
        // fLog2 (Taylor approximation: NOT the ideal log; the port must
        // reproduce the approximation bit-exactly).
        assert_eq!(calc_ld_data(0x4000_0000), -33550519);
        assert_eq!(calc_ld_data(0x7FFF_FFFF), -1);
        assert_eq!(calc_ld_data(1), -1040183479);
        assert_eq!(calc_ld_data(2), -1006629047);
        assert_eq!(calc_ld_data(0x12345678), -94414177);
        assert_eq!(calc_ld_data(0), i32::MIN);
        assert_eq!(calc_ld_data(-5), i32::MIN);
        assert_eq!(calc_ld_data(0x08000000), -134213815);
        // sqrtFixp (Newton via invSqrtNorm2).
        assert_eq!(sqrt_fixp(0), 0);
        assert_eq!(sqrt_fixp(1), 46340);
        assert_eq!(sqrt_fixp(0x4000_0000), 1518500248);
        assert_eq!(sqrt_fixp(0x7FFF_FFFF), 2147483644);
        assert_eq!(sqrt_fixp(0x04000000), 379625060);
        assert_eq!(sqrt_fixp(1000000), 46340948);
        // CalcInvLdData incl. clamps (0 -> MAXVAL, below -31/64 -> 0,
        // at/above 31/64 -> MAXVAL).
        assert_eq!(calc_inv_ld_data(0), i32::MAX);
        assert_eq!(calc_inv_ld_data(i32::MIN), 0);
        assert_eq!(calc_inv_ld_data(0x06000000), 8);
        assert_eq!(calc_inv_ld_data(0x3E000000), i32::MAX);
        assert_eq!(calc_inv_ld_data(0x3DFFFFFF), 2147483584);
        assert_eq!(calc_inv_ld_data(-1), 2147483584);
        assert_eq!(calc_inv_ld_data(0x0C000000), 64);
        // General table path (sine f1 weighting inputs).
        assert_eq!(calc_inv_ld_data(-260045870), 9975993);
        assert_eq!(calc_inv_ld_data(-520091740), 46342);
        assert_eq!(calc_inv_ld_data(-1040183479), 1);
        // CalcLdInt table (index 1 is log2(1) = 0; out of range reads 0).
        assert_eq!(LD_INT_COEFF.len(), 193);
        assert_eq!(calc_ld_int(0), 0);
        assert_eq!(calc_ld_int(1), 0);
        assert_eq!(calc_ld_int(2), 0x02000000);
        assert_eq!(calc_ld_int(192), 0x0F2B8034);
        assert_eq!(calc_ld_int(193), 0);
        assert_eq!(calc_ld_int(-7), 0);
        // Table monotonicity (log is monotone; guards transcription typos).
        for i in 2..LD_INT_COEFF.len() {
            assert!(
                LD_INT_COEFF[i] >= LD_INT_COEFF[i - 1],
                "ldIntCoeff not monotone at {i}"
            );
        }
        // fMultI rounding (0.5*3 rounds to 2; C3*100 truncates to 56).
        assert_eq!(fmult_i(0x40000000, 3), 2);
        assert_eq!(fmult_i(0x47990500, 100), 56);
        assert_eq!(fmult_i(i32::MIN, 5), -5);
        assert_eq!(fmult_i(0, 99), 0);
        // fDivNorm incl. the zero fast path and the 1/1 saturation edge.
        assert_eq!(fdiv_norm(0, 100), 0);
        assert_eq!(fdiv_norm(900, 1024), 1887436800);
        assert_eq!(fdiv_norm(1, 1), i32::MAX);
        assert_eq!(fdiv_norm(512, 2048), 0x20000000);
        // Narrowed Div2/AddDiv2 (Taylor/sqrt building blocks).
        // fMultDiv2(0.5, 0.5) = 0.25/2 = 0.125 (the Div2 halves once more).
        assert_eq!(fmult_div2(0x40000000, 0x40000000), 0x10000000);
        assert_eq!(
            fmult_add_div2(100, 0x40000000, 0x40000000),
            100 + 0x10000000
        );
        // fixnorm-D (0 -> 0, else clz-1 over magnitude; note ~(-1) == 0
        // gives 32-1 == 31, matching C).
        assert_eq!(fixnorm_d(0), 0);
        assert_eq!(fixnorm_d(1), 30);
        assert_eq!(fixnorm_d(-1), 31);
        assert_eq!(fixnorm_d(0x40000000), 0);
    }
}
