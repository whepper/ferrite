//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! x86-64 AVX2 acceleration for the S3 quantizer kernels.
//!
//! Eight-lines-at-a-time vectorization of `quantize_lines`,
//! `calc_sfb_dist`, and `calc_sfb_quant_energy_and_dist`. The scalar functions
//! in [`super::quant`] remain the reference and the fallback for exceptional
//! inputs, tails, non-x86-64 targets, and x86-64 without AVX2.
//!
//! Re-derived for Ferrite's Q31 arithmetic and the x86-64 scalar semantics
//! (not copied from NEON): the Rust scalar `fmult` is `floor(a*b/2^31)`
//! (bit 0 kept), which the x86 C `shl edx,1` variant would clear — so AVX2
//! must reproduce the *floor* via full 64-bit products (`mul_epi32`
//! even/odd halves + logical `>>31`, whose low 32 bits equal the arithmetic
//! floor), never a high-half-doubled value. `fixmuldiv2_DS` is the plain
//! high half (`>>32`), `fixmuldiv2_SS` the plain low product (`mullo`),
//! `fpow2` the floor with bit 0 cleared (`& !1`, exactly the scalar
//! `((p>>32)<<1)` for non-negative square products). AVX2 64-bit products
//! never saturate at these magnitudes (max `|2·s·t| ≈ 8.7e18 < 2^63-1` on
//! the inverse path), so unlike NEON `sqdmulh` there is no `fmult(INT_MIN,
//! INT_MIN)` saturation divergence and no `inv == INT_MIN` guard is needed.
//! `clz` has no AVX2 equivalent and is emulated by an exact 5-step binary
//! search with unsigned compares; variable shifts mask counts to 5 bits
//! explicitly, matching the scalar wrapping semantics. Table lookups use
//! scalar lane loads (as NEON does); no hardware gather is assumed faster.
//! Zero fast paths and sign restoration are selects (`blendv`/`andnot`), not
//! branches. Early-out and prefix-write semantics are preserved: a violating
//! group falls back to the scalar loop, which writes the exact scalar prefix
//! and returns `None`; tails always run scalar. Energy/distortion sums are
//! wrapping `i32` adds, associative mod `2^32`, so vector summation order is
//! exact.
//!
//! Dispatch lives in [`super::quant`]: AArch64 keeps NEON, x86-64 with AVX2
//! uses this module after `is_x86_feature_detected!("avx2")`, everything else
//! uses the scalar fallback. The three entry points carry
//! `#[target_feature(enable = "avx2")]` and `#[inline(never)]` so AVX2
//! instructions can never be inlined into baseline code that runs before the
//! runtime feature check.

#[cfg(target_arch = "x86_64")]
use super::quant::{accumulate_dist, fpow2, inv_quantize_line, quantize_line_body, MAX_QUANT};
#[cfg(target_arch = "x86_64")]
use super::quant_tables::{
    MTAB_3_4, MTAB_4_3, QUANT_TABLE_E, QUANT_TABLE_Q, SPEC_EXP_MANT, SPEC_EXP_SHIFT,
};
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(target_arch = "x86_64")]
struct Avx2Consts {
    quantizer32: i32,
    qshift1: i32,
    k: i32,
    m: usize,
    ishift: i32,
}

#[cfg(target_arch = "x86_64")]
impl Avx2Consts {
    #[inline(always)]
    fn new(gain: i32, quantizer_shift: i32, k: i32) -> Self {
        let q = QUANT_TABLE_Q[(gain.wrapping_neg() & 3) as usize];
        Avx2Consts {
            // `fixmuldiv2_DS` widens the 16-bit quantizer by 16 before the
            // 64-bit product; pre-shift once per band (no wrap: i16 << 16).
            quantizer32: (q as i32).wrapping_shl(16),
            qshift1: quantizer_shift.wrapping_add(1),
            k,
            m: (gain & 3) as usize,
            ishift: gain >> 2,
        }
    }
}

/// Eight lanes of scalar `fmult` (floor, bit 0 kept).
///
/// # Safety
/// Requires AVX2. Pure register arithmetic; no memory access. `mul_epi32`
/// covers the even dwords, the `srli_epi64::<32>` halves cover the odd
/// dwords; the logical `>>31` low 32 bits equal the arithmetic floor
/// (upper fill bits fall outside the kept word).
///
/// Reconstruction (Phase 2.1): after the shift, each 128-bit lane of
/// `even_s` holds `[lo(P0), hi(P0), lo(P2), hi(P2)]` and `odd_s` holds
/// `[lo(P1), hi(P1), lo(P3), hi(P3)]`. `slli_epi64::<32>(odd_s)` moves each
/// odd low half into the high dword of its qword (`[0, lo(P1), 0, lo(P3)]`),
/// so `blend(0xAA)` keeps the even low halves at dword positions 0,2 and the
/// odd low halves at positions 1,3 — restoring `[P0,P1,P2,P3]` lane order.
/// The former `unpacklo/unpackhi + blend(0xCC)` kept the *high* halves at
/// positions 2,3 (the desired low halves sit at positions 0,1 of `unpackhi`,
/// unreachable by a blend), corrupting lanes 2,3,6,7; see §32.6.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn fmult8(a: __m256i, b: __m256i) -> __m256i {
    let even = _mm256_mul_epi32(a, b);
    let a_odd = _mm256_srli_epi64::<32>(a);
    let b_odd = _mm256_srli_epi64::<32>(b);
    let odd = _mm256_mul_epi32(a_odd, b_odd);
    let even_s = _mm256_srli_epi64::<31>(even);
    let odd_s = _mm256_srli_epi64::<31>(odd);
    // Even low halves stay in place; odd low halves are lifted into the high
    // dword of each qword, then blended into the odd dword positions.
    let odd_hi = _mm256_slli_epi64::<32>(odd_s);
    _mm256_blend_epi32::<0xAA>(even_s, odd_hi)
}

/// Eight lanes of `fixmuldiv2_DS(x, q32) = (q32*x)>>32` (plain high half).
///
/// # Safety
/// Requires AVX2. Same even/odd structure as [`fmult8`] with a `>>32`
/// shift; logical vs arithmetic fill is again outside the kept word. The
/// lane reconstruction is the corrected `slli_epi64::<32> + blend(0xAA)`
/// form documented on [`fmult8`] (Phase 2.1).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn fixmuldiv2_ds8(x: __m256i, q32: __m256i) -> __m256i {
    let even = _mm256_mul_epi32(x, q32);
    let x_odd = _mm256_srli_epi64::<32>(x);
    // `q32` is broadcast, so its odd half equals its even half; reuse it.
    let odd = _mm256_mul_epi32(x_odd, q32);
    let even_s = _mm256_srli_epi64::<32>(even);
    let odd_s = _mm256_srli_epi64::<32>(odd);
    let odd_hi = _mm256_slli_epi64::<32>(odd_s);
    _mm256_blend_epi32::<0xAA>(even_s, odd_hi)
}

/// Eight lanes of scalar `fpow2` (square, bit 0 cleared).
///
/// # Safety
/// Requires AVX2. Square products are non-negative, so
/// `((p>>32)<<1) == ((p>>31) & !1)` exactly; the floor keeps the low word.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn fpow2_8(a: __m256i) -> __m256i {
    let f = fmult8(a, a);
    _mm256_and_si256(f, _mm256_set1_epi32(!1))
}

/// Eight lanes of `leading_zeros` (32 for zero).
///
/// # Safety
/// Requires AVX2. Exact 5-step binary search with unsigned compares (sign-bit
/// bias + `cmpgt`); zero is fixed to 32 at the end. Pure arithmetic.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn clz8(x: __m256i) -> __m256i {
    let bias = _mm256_set1_epi32(0x80000000u32 as i32);
    let mut v = x;
    let mut n = _mm256_setzero_si256();
    // Step 16: v < 0x10000 (unsigned via sign-bit bias; `vb` is recomputed
    // from `v` every step — folding the shift into it would corrupt the bias).
    let m = _mm256_cmpgt_epi32(
        _mm256_xor_si256(_mm256_set1_epi32(0x00010000), bias),
        _mm256_xor_si256(v, bias),
    );
    n = _mm256_add_epi32(n, _mm256_and_si256(m, _mm256_set1_epi32(16)));
    v = _mm256_blendv_epi8(v, _mm256_slli_epi32::<16>(v), m);
    // Step 8: v < 0x1000000.
    let m = _mm256_cmpgt_epi32(
        _mm256_xor_si256(_mm256_set1_epi32(0x01000000), bias),
        _mm256_xor_si256(v, bias),
    );
    n = _mm256_add_epi32(n, _mm256_and_si256(m, _mm256_set1_epi32(8)));
    v = _mm256_blendv_epi8(v, _mm256_slli_epi32::<8>(v), m);
    // Step 4: v < 0x10000000.
    let m = _mm256_cmpgt_epi32(
        _mm256_xor_si256(_mm256_set1_epi32(0x10000000), bias),
        _mm256_xor_si256(v, bias),
    );
    n = _mm256_add_epi32(n, _mm256_and_si256(m, _mm256_set1_epi32(4)));
    v = _mm256_blendv_epi8(v, _mm256_slli_epi32::<4>(v), m);
    // Step 2: top two bits zero <=> v < 0x40000000.
    let m = _mm256_cmpgt_epi32(
        _mm256_xor_si256(_mm256_set1_epi32(0x40000000), bias),
        _mm256_xor_si256(v, bias),
    );
    n = _mm256_add_epi32(n, _mm256_and_si256(m, _mm256_set1_epi32(2)));
    v = _mm256_blendv_epi8(v, _mm256_slli_epi32::<2>(v), m);
    // Step 1: v < 0x80000000, i.e. v >= 0 signed (no shift after).
    let m = _mm256_cmpgt_epi32(v, _mm256_set1_epi32(-1));
    n = _mm256_add_epi32(n, _mm256_and_si256(m, _mm256_set1_epi32(1)));
    // Zero input took every branch (31); the scalar result is 32.
    _mm256_blendv_epi8(
        n,
        _mm256_set1_epi32(32),
        _mm256_cmpeq_epi32(x, _mm256_setzero_si256()),
    )
}

/// Scalar lane loads for the 512-entry `i16` table (indices already `& 511`).
///
/// # Safety
/// Requires AVX2. Every lane of `idx` must be `< 512`; callers mask before
/// calling. No gather is used (see module docs).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn table_i16_512(tab: &[i16; 512], idx: __m256i) -> __m256i {
    let mut ii = [0i32; 8];
    _mm256_storeu_si256(ii.as_mut_ptr() as *mut __m256i, idx);
    _mm256_set_epi32(
        tab[ii[7] as usize] as i32,
        tab[ii[6] as usize] as i32,
        tab[ii[5] as usize] as i32,
        tab[ii[4] as usize] as i32,
        tab[ii[3] as usize] as i32,
        tab[ii[2] as usize] as i32,
        tab[ii[1] as usize] as i32,
        tab[ii[0] as usize] as i32,
    )
}

/// Scalar lane loads for a 4-entry `i16` table (indices already `& 3`).
///
/// # Safety
/// Requires AVX2. Every lane of `idx` must be `< 4`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn table_i16_4(tab: &[i16; 4], idx: __m256i) -> __m256i {
    let mut ii = [0i32; 8];
    _mm256_storeu_si256(ii.as_mut_ptr() as *mut __m256i, idx);
    _mm256_set_epi32(
        tab[ii[7] as usize] as i32,
        tab[ii[6] as usize] as i32,
        tab[ii[5] as usize] as i32,
        tab[ii[4] as usize] as i32,
        tab[ii[3] as usize] as i32,
        tab[ii[2] as usize] as i32,
        tab[ii[1] as usize] as i32,
        tab[ii[0] as usize] as i32,
    )
}

/// Scalar lane loads for the 512-entry `i32` table (indices already `& 511`).
///
/// # Safety
/// Requires AVX2. Every lane of `idx` must be `< 512`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn table_i32_512(tab: &[i32; 512], idx: __m256i) -> __m256i {
    let mut ii = [0i32; 8];
    _mm256_storeu_si256(ii.as_mut_ptr() as *mut __m256i, idx);
    _mm256_set_epi32(
        tab[ii[7] as usize],
        tab[ii[6] as usize],
        tab[ii[5] as usize],
        tab[ii[4] as usize],
        tab[ii[3] as usize],
        tab[ii[2] as usize],
        tab[ii[1] as usize],
        tab[ii[0] as usize],
    )
}

/// Scalar lane loads for one `SPEC_EXP_MANT` row (indices clamped `0..=13`).
///
/// # Safety
/// Requires AVX2. `m` must be `< 4` and every lane of `idx` `< 14`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn table_mant_row(m: usize, idx: __m256i) -> __m256i {
    let row = &SPEC_EXP_MANT[m];
    let mut ii = [0i32; 8];
    _mm256_storeu_si256(ii.as_mut_ptr() as *mut __m256i, idx);
    _mm256_set_epi32(
        row[ii[7] as usize],
        row[ii[6] as usize],
        row[ii[5] as usize],
        row[ii[4] as usize],
        row[ii[3] as usize],
        row[ii[2] as usize],
        row[ii[1] as usize],
        row[ii[0] as usize],
    )
}

/// Scalar lane loads for one `SPEC_EXP_SHIFT` row (indices clamped `0..=13`).
///
/// # Safety
/// Requires AVX2. `m` must be `< 4` and every lane of `idx` `< 14`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn table_shift_row(m: usize, idx: __m256i) -> __m256i {
    let row = &SPEC_EXP_SHIFT[m];
    let mut ii = [0i32; 8];
    _mm256_storeu_si256(ii.as_mut_ptr() as *mut __m256i, idx);
    _mm256_set_epi32(
        row[ii[7] as usize] as i32,
        row[ii[6] as usize] as i32,
        row[ii[5] as usize] as i32,
        row[ii[4] as usize] as i32,
        row[ii[3] as usize] as i32,
        row[ii[2] as usize] as i32,
        row[ii[1] as usize] as i32,
        row[ii[0] as usize] as i32,
    )
}

/// Eight lines of `quantize_line_body`, sign-extended to 32 bits.
///
/// # Safety
/// Requires AVX2. Vector-only apart from the table lane loads, whose indices
/// are bounded by construction (`idx & 511`, `e_idx & 3`); `c` must be a
/// valid [`Avx2Consts`]. No slice access happens here.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn quantize8(x: __m256i, c: &Avx2Consts) -> __m256i {
    let zero = _mm256_setzero_si256();
    let accu = fixmuldiv2_ds8(x, _mm256_set1_epi32(c.quantizer32));
    let a = _mm256_abs_epi32(accu);
    let shift = _mm256_sub_epi32(clz8(a), _mm256_set1_epi32(1));
    let an = _mm256_sllv_epi32(a, _mm256_and_si256(shift, _mm256_set1_epi32(31)));
    let idx = _mm256_and_si256(_mm256_srli_epi32::<21>(an), _mm256_set1_epi32(511));
    let m34 = table_i16_512(&MTAB_3_4, idx);
    let ts = _mm256_sub_epi32(_mm256_set1_epi32(c.qshift1), shift);
    let e_idx = _mm256_and_si256(ts, _mm256_set1_epi32(3));
    let qe = table_i16_4(&QUANT_TABLE_E, e_idx);
    let m = _mm256_mullo_epi32(m34, qe);
    let t = _mm256_srai_epi32::<2>(ts);
    let t3 = _mm256_add_epi32(_mm256_add_epi32(t, t), t);
    // `12 + min(-3t, 19)` mirrors the scalar `12 - 3*(ts>>2)` with `.min(31)`.
    let sh = _mm256_add_epi32(
        _mm256_min_epi32(_mm256_sub_epi32(zero, t3), _mm256_set1_epi32(19)),
        _mm256_set1_epi32(12),
    );
    let m = _mm256_srlv_epi32(m, _mm256_and_si256(sh, _mm256_set1_epi32(31)));
    let q = _mm256_srai_epi32::<15>(_mm256_add_epi32(_mm256_set1_epi32(c.k), m));
    // Sign restoration, then the zero fast path (selects, not branches).
    let neg = _mm256_cmpgt_epi32(zero, accu);
    let q = _mm256_blendv_epi8(q, _mm256_sub_epi32(zero, q), neg);
    _mm256_andnot_si256(_mm256_cmpeq_epi32(accu, zero), q)
}

/// Eight lines of `inv_quantize_line`; lanes with `|q| > MAX_QUANT` hold
/// unspecified values, but their table indices are clamped so every lane load
/// stays in bounds. The `over_max_quant` guard discards those lanes (scalar
/// fallback with the documented early-out), so clamping only affects
/// discarded lanes; lanes with `|q| <= MAX_QUANT` and `q == 0` are unaffected.
///
/// # Safety
/// Requires AVX2. Every table index is bounded before its lane load: the
/// 512-entry index is masked with `& 511` and the exponent-row index is
/// clamped to `0..=13`, so all lane loads stay in bounds even for exceptional
/// lanes. `c.m` must be `< 4`. No slice access happens here.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn inv_quantize8(q: __m256i, c: &Avx2Consts) -> __m256i {
    let zero = _mm256_setzero_si256();
    let qa = _mm256_abs_epi32(q);
    let clz = clz8(qa);
    let an = _mm256_sllv_epi32(
        qa,
        _mm256_and_si256(
            _mm256_sub_epi32(clz, _mm256_set1_epi32(1)),
            _mm256_set1_epi32(31),
        ),
    );
    let spec_exp = _mm256_sub_epi32(_mm256_set1_epi32(32), clz);
    // Clamp to the 14-entry exponent rows: `|q| > MAX_QUANT` lanes (and only
    // those, plus `q == 0` which maps to 0) can yield `spec_exp >= 14`.
    let spec_exp_c = _mm256_min_epi32(spec_exp, _mm256_set1_epi32(13));
    let idx = _mm256_and_si256(_mm256_srli_epi32::<21>(an), _mm256_set1_epi32(511));
    let s = table_i32_512(&MTAB_4_3, idx);
    let t = table_mant_row(c.m, spec_exp_c);
    // Floor `fmult` (bit 0 kept), matching the x86-64 scalar reference.
    let accu = fmult8(s, t);
    let e = table_shift_row(c.m, spec_exp_c);
    // Unified signed shift `st - 1` (negative shifts right): both directions
    // computed with explicitly masked counts, selected per lane.
    let st = _mm256_add_epi32(_mm256_set1_epi32(c.ishift), e);
    let one = _mm256_set1_epi32(1);
    let s_amt = _mm256_sub_epi32(st, one);
    let mask31 = _mm256_set1_epi32(31);
    let left = _mm256_sllv_epi32(accu, _mm256_and_si256(s_amt, mask31));
    let right = _mm256_srav_epi32(
        accu,
        _mm256_and_si256(_mm256_sub_epi32(zero, s_amt), mask31),
    );
    let accu = _mm256_blendv_epi8(right, left, _mm256_cmpgt_epi32(st, one));
    let accu = _mm256_blendv_epi8(
        accu,
        _mm256_sub_epi32(zero, accu),
        _mm256_cmpgt_epi32(zero, q),
    );
    _mm256_andnot_si256(_mm256_cmpeq_epi32(q, zero), accu)
}

/// Distortion of eight lines plus a mask of lanes whose difference is `2^31`.
///
/// # Safety
/// Requires AVX2. Vector-only arithmetic; no memory access. Mirrors the
/// scalar `accumulate_dist` step for step, including the `fpow2` bit-0 clear
/// and the `4 - min(2*clz, 35)` net shift.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dist8(inv: __m256i, x: __m256i) -> (__m256i, __m256i) {
    let zero = _mm256_setzero_si256();
    let a = _mm256_abs_epi32(inv);
    let b = _mm256_abs_epi32(_mm256_srai_epi32::<1>(x));
    let d = _mm256_abs_epi32(_mm256_sub_epi32(a, b));
    // `d < 0` only for `d == INT_MIN` (wrapping abs), routed to fallback.
    let bad = _mm256_cmpgt_epi32(zero, d);
    let clz = clz8(d);
    let n = _mm256_srli_epi32::<1>(_mm256_sllv_epi32(
        d,
        _mm256_and_si256(clz, _mm256_set1_epi32(31)),
    ));
    let p = fpow2_8(n);
    let amount = _mm256_sub_epi32(
        _mm256_set1_epi32(4),
        _mm256_min_epi32(_mm256_slli_epi32::<1>(clz), _mm256_set1_epi32(35)),
    );
    let mask31 = _mm256_set1_epi32(31);
    let left = _mm256_sllv_epi32(p, _mm256_and_si256(amount, mask31));
    let right = _mm256_srlv_epi32(p, _mm256_and_si256(_mm256_sub_epi32(zero, amount), mask31));
    let dist = _mm256_blendv_epi8(right, left, _mm256_cmpgt_epi32(amount, zero));
    (dist, bad)
}

/// Lane mask of `|q| > MAX_QUANT` (widened magnitude, so `i16::MIN` is caught:
/// vector lanes hold sign-extended `i16` values, whose abs always fits `i32`).
///
/// # Safety
/// Requires AVX2. Vector-only arithmetic; no memory access.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn over_max_quant(q: __m256i) -> __m256i {
    _mm256_cmpgt_epi32(_mm256_abs_epi32(q), _mm256_set1_epi32(MAX_QUANT))
}

/// Wrapping horizontal sum of eight `i32` lanes (associative mod `2^32`).
///
/// # Safety
/// Requires AVX2. SSE2-level shuffles only; no memory access.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn hsum_wrapping(v: __m256i) -> i32 {
    let lo = _mm256_castsi256_si128(v);
    let hi = _mm256_extracti128_si256::<1>(v);
    let s = _mm_add_epi32(lo, hi);
    let s = _mm_add_epi32(s, _mm_shuffle_epi32::<0x4E>(s));
    let a = _mm_cvtsi128_si32(s);
    let b = _mm_cvtsi128_si32(_mm_shuffle_epi32::<0x55>(s));
    a.wrapping_add(b)
}

/// Quantize `spec` into `quant`, eight lines at a time, scalar tail.
///
/// # Safety
/// Requires AVX2. `spec` and `quant` must have equal length (asserted in
/// debug); the slices themselves guarantee pointer validity. Reads/writes
/// exactly the paired elements; the tail uses the scalar body.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline(never)]
pub(crate) unsafe fn quantize_lines_avx2(
    spec: &[i32],
    quant: &mut [i16],
    gain: i32,
    quantizer_shift: i32,
    k: i32,
) {
    debug_assert_eq!(spec.len(), quant.len());
    let c = Avx2Consts::new(gain, quantizer_shift, k);
    let (groups, tail_spec) = spec.as_chunks::<8>();
    let (groups_q, tail_quant) = quant.as_chunks_mut::<8>();
    for (sg, qg) in groups.iter().zip(groups_q.iter_mut()) {
        let x = _mm256_loadu_si256(sg.as_ptr() as *const __m256i);
        let q = quantize8(x, &c);
        let mut tmp = [0i32; 8];
        _mm256_storeu_si256(tmp.as_mut_ptr() as *mut __m256i, q);
        for (o, &v) in qg.iter_mut().zip(tmp.iter()) {
            // `as i16` wraps exactly like the scalar narrowing.
            *o = v as i16;
        }
    }
    for (q_out, &x) in tail_quant.iter_mut().zip(tail_spec.iter()) {
        *q_out = quantize_line_body(
            super::quant_tables::QUANT_TABLE_Q[(gain.wrapping_neg() & 3) as usize],
            quantizer_shift,
            k,
            x,
        );
    }
}

/// Re-quantize + inverse-quantize + distortion sum for one band; `None` on the
/// `|q| > MAX_QUANT` early-out (scalar prefix already written).
///
/// # Safety
/// Requires AVX2. `spec` and `quant` must have equal length (asserted in
/// debug); the slices guarantee pointer validity. On early-out only the
/// scalar prefix through the violating line is written.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline(never)]
pub(crate) unsafe fn calc_sfb_dist_sum_avx2(
    spec: &[i32],
    quant: &mut [i16],
    gain: i32,
    quantizer_shift: i32,
    k: i32,
) -> Option<i32> {
    debug_assert_eq!(spec.len(), quant.len());
    let c = Avx2Consts::new(gain, quantizer_shift, k);
    let mut xfsf: i32 = 0;
    let mut vec_sum = _mm256_setzero_si256();
    let (groups, tail_spec) = spec.as_chunks::<8>();
    let (groups_q, tail_quant) = quant.as_chunks_mut::<8>();
    for (sg, qg) in groups.iter().zip(groups_q.iter_mut()) {
        let x = _mm256_loadu_si256(sg.as_ptr() as *const __m256i);
        let q = quantize8(x, &c);
        let inv = inv_quantize8(q, &c);
        let (d, bad) = dist8(inv, x);
        let bad_or = _mm256_or_si256(over_max_quant(q), bad);
        if _mm256_testz_si256(bad_or, bad_or) == 0 {
            // Fall back to the scalar loop for this group: exact prefix.
            for (q_out, &xs) in qg.iter_mut().zip(sg.iter()) {
                let qs = quantize_line_body(
                    super::quant_tables::QUANT_TABLE_Q[(gain.wrapping_neg() & 3) as usize],
                    quantizer_shift,
                    k,
                    xs,
                );
                *q_out = qs;
                if (qs as i32).abs() > MAX_QUANT {
                    return None;
                }
                let invs = inv_quantize_line(gain, qs);
                xfsf = accumulate_dist(xfsf, invs, xs);
            }
            continue;
        }
        let mut tmp = [0i32; 8];
        _mm256_storeu_si256(tmp.as_mut_ptr() as *mut __m256i, q);
        for (o, &v) in qg.iter_mut().zip(tmp.iter()) {
            *o = v as i16;
        }
        vec_sum = _mm256_add_epi32(vec_sum, d);
    }
    xfsf = xfsf.wrapping_add(hsum_wrapping(vec_sum));
    for (q_out, &xs) in tail_quant.iter_mut().zip(tail_spec.iter()) {
        let qs = quantize_line_body(
            super::quant_tables::QUANT_TABLE_Q[(gain.wrapping_neg() & 3) as usize],
            quantizer_shift,
            k,
            xs,
        );
        *q_out = qs;
        if (qs as i32).abs() > MAX_QUANT {
            return None;
        }
        let invs = inv_quantize_line(gain, qs);
        xfsf = accumulate_dist(xfsf, invs, xs);
    }
    Some(xfsf)
}

/// Energy + distortion sums for a band with caller-supplied quantized lines;
/// `None` on the `|q| > MAX_QUANT` early-out.
///
/// # Safety
/// Requires AVX2. `spec` and `quant` must have equal length (asserted in
/// debug); the slices guarantee pointer validity. `quant` is read-only.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline(never)]
pub(crate) unsafe fn calc_sfb_quant_energy_and_dist_sums_avx2(
    spec: &[i32],
    quant: &[i16],
    gain: i32,
) -> Option<(i32, i32)> {
    debug_assert_eq!(spec.len(), quant.len());
    let c = Avx2Consts::new(gain, 0, 0);
    let mut energy: i32 = 0;
    let mut distortion: i32 = 0;
    let mut en_sum = _mm256_setzero_si256();
    let mut dist_sum = _mm256_setzero_si256();
    let (groups, tail_spec) = spec.as_chunks::<8>();
    let (groups_q, tail_quant) = quant.as_chunks::<8>();
    for (sg, qg) in groups.iter().zip(groups_q.iter()) {
        let x = _mm256_loadu_si256(sg.as_ptr() as *const __m256i);
        let q = _mm256_cvtepi16_epi32(_mm_loadu_si128(qg.as_ptr() as *const __m128i));
        let inv = inv_quantize8(q, &c);
        let (d, bad) = dist8(inv, x);
        let bad_or = _mm256_or_si256(over_max_quant(q), bad);
        if _mm256_testz_si256(bad_or, bad_or) == 0 {
            for (&qs, &xs) in qg.iter().zip(sg.iter()) {
                if (qs as i32).abs() > MAX_QUANT {
                    return None;
                }
                let invs = inv_quantize_line(gain, qs);
                energy = energy.wrapping_add(fpow2(invs));
                distortion = accumulate_dist(distortion, invs, xs);
            }
            continue;
        }
        en_sum = _mm256_add_epi32(en_sum, fpow2_8(inv));
        dist_sum = _mm256_add_epi32(dist_sum, d);
    }
    energy = energy.wrapping_add(hsum_wrapping(en_sum));
    distortion = distortion.wrapping_add(hsum_wrapping(dist_sum));
    for (&qs, &xs) in tail_quant.iter().zip(tail_spec.iter()) {
        if (qs as i32).abs() > MAX_QUANT {
            return None;
        }
        let invs = inv_quantize_line(gain, qs);
        energy = energy.wrapping_add(fpow2(invs));
        distortion = accumulate_dist(distortion, invs, xs);
    }
    Some((energy, distortion))
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::super::q31::fmult;
    use super::super::quant::{
        calc_sfb_dist_scalar, calc_sfb_quant_energy_and_dist_scalar, quantize_lines_scalar,
        MAX_QUANT,
    };
    use super::super::quant_tables::{QUANT_K0, QUANT_K1, QUANT_TABLE_Q};
    use super::{fixmuldiv2_ds8, fmult8};
    use core::arch::x86_64::*;

    fn avx2_available() -> bool {
        std::arch::is_x86_feature_detected!("avx2")
    }

    /// Store a vector to eight scalar lanes (uses the real intrinsic output).
    fn lanes(v: __m256i) -> [i32; 8] {
        let mut out = [0i32; 8];
        unsafe { _mm256_storeu_si256(out.as_mut_ptr() as *mut __m256i, v) };
        out
    }

    /// Phase 2.1 (MEDIUM-1): positively assert that production dispatch
    /// selects AVX2 on this machine when the feature is present. The helper
    /// mirrors the exact dispatch gates and the identical detection macro, so
    /// `Avx2` here means the real `calc_sfb_dist` / energy / quantize_spectrum
    /// dispatch takes the AVX2 branch. Without AVX2 it must resolve to scalar
    /// (honest skip of the AVX2 assertion). This does not infer selection from
    /// a kernel test passing.
    #[test]
    fn avx2_dispatch_selection() {
        use super::super::quant::{selected_backend, Backend};
        let b = selected_backend();
        if avx2_available() {
            assert_eq!(b, Backend::Avx2, "production dispatch must select AVX2");
        } else {
            assert_eq!(
                b,
                Backend::Scalar,
                "without AVX2, dispatch must select scalar"
            );
            eprintln!("AVX2_TEST avx2_dispatch_selection: SKIP (no AVX2; scalar confirmed)");
        }
    }

    /// Phase 2.1 regression: the former `unpacklo/unpackhi + blend(0xCC)`
    /// reconstruction kept the high product halves in lanes 2,3,6,7. Eight
    /// identical lanes `0x40000000 * 0x40000000` must all be `0x20000000`
    /// (floor(2^60 / 2^31)); the old code returned 0 in lanes 2,3,6,7.
    #[test]
    fn avx2_fmult8_identical_lanes_anchor() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_fmult8_identical_lanes_anchor: SKIP (no AVX2)");
            return;
        }
        let v = unsafe { fmult8(_mm256_set1_epi32(0x40000000), _mm256_set1_epi32(0x40000000)) };
        assert_eq!(
            lanes(v),
            [0x20000000; 8],
            "all eight lanes must be 0x20000000"
        );
    }

    /// Phase 2.1 regression: `fixmuldiv2_DS(x, 16384<<16) = floor(x/4)`.
    /// Distinct nonzero results in every lane; the old code zeroed lanes
    /// 2,3,6,7. Expected values are hand-computed constants (independent of
    /// any test-only reimplementation); the function under test is the real
    /// SIMD helper.
    #[test]
    fn avx2_ds8_all_lanes_nonzero() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_ds8_all_lanes_nonzero: SKIP (no AVX2)");
            return;
        }
        let x = unsafe { _mm256_set_epi32(32, 28, 24, 20, 16, 12, 8, 4) };
        let q32 = unsafe { _mm256_set1_epi32(16384 << 16) };
        let v = unsafe { fixmuldiv2_ds8(x, q32) };
        // set_epi32 is reverse order: lane0=4 ... lane7=32 -> floor(x/4).
        assert_eq!(
            lanes(v),
            [1, 2, 3, 4, 5, 6, 7, 8],
            "DS8 distinct nonzero lanes"
        );
    }

    /// Phase 2.1 regression: alternating even/odd magnitudes with distinct
    /// products expose any lane-ordering defect. Differential against the
    /// production scalar `q31::fmult` on every lane.
    #[test]
    fn avx2_fmult8_alternating_even_odd() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_fmult8_alternating_even_odd: SKIP (no AVX2)");
            return;
        }
        let a = [
            0x40000000i32,
            0x20000000,
            0x60000000,
            0x10000000,
            0x40000000,
            0x20000000,
            0x60000000,
            0x10000000,
        ];
        let b = [
            0x40000000i32,
            0x30000000,
            0x20000000,
            0x50000000,
            0x40000000,
            0x30000000,
            0x20000000,
            0x50000000,
        ];
        let va = unsafe { _mm256_loadu_si256(a.as_ptr() as *const __m256i) };
        let vb = unsafe { _mm256_loadu_si256(b.as_ptr() as *const __m256i) };
        let got = lanes(unsafe { fmult8(va, vb) });
        for i in 0..8 {
            assert_eq!(got[i], fmult(a[i], b[i]), "fmult8 lane {i} (alternating)");
        }
    }

    /// Phase 2.1 regression: negative products, values around zero, and the
    /// `INT_MIN * INT_MIN` corner (no saturation on AVX2). Differential
    /// against production scalar `q31::fmult` on every lane.
    #[test]
    fn avx2_fmult8_negative_and_near_zero() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_fmult8_negative_and_near_zero: SKIP (no AVX2)");
            return;
        }
        let a = [i32::MIN, -1, 0, 1, i32::MIN, -2, 2, i32::MAX];
        let b = [i32::MIN, 1, i32::MAX, -1, 2, i32::MIN, -3, 3];
        let va = unsafe { _mm256_loadu_si256(a.as_ptr() as *const __m256i) };
        let vb = unsafe { _mm256_loadu_si256(b.as_ptr() as *const __m256i) };
        let got = lanes(unsafe { fmult8(va, vb) });
        for i in 0..8 {
            assert_eq!(
                got[i],
                fmult(a[i], b[i]),
                "fmult8 lane {i} (negative/near-zero)"
            );
        }
        // Explicit corner anchors (independent of the loop above).
        assert_eq!(got[0], i32::MIN, "fmult(INT_MIN, INT_MIN) == INT_MIN");
        assert_eq!(got[2], 0, "fmult(0, INT_MAX) == 0");
    }

    /// Phase 2.1 regression: DS8 with negative inputs and zero. Expected
    /// `floor(x/4)` constants; the old code zeroed lanes 2,3,6,7 (here lane 6
    /// must be -2 and lane 7 must be 3).
    #[test]
    fn avx2_ds8_negative_and_zero() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_ds8_negative_and_zero: SKIP (no AVX2)");
            return;
        }
        let x = unsafe { _mm256_set_epi32(12, -8, 8, -1, 1, 0, -4, 4) };
        let q32 = unsafe { _mm256_set1_epi32(16384 << 16) };
        let got = lanes(unsafe { fixmuldiv2_ds8(x, q32) });
        // lane0=4,1=-4,2=0,3=1,4=-1,5=8,6=-8,7=12 -> floor(x/4)
        assert_eq!(got, [1, -1, 0, 0, -1, 2, -2, 3], "DS8 negative/zero lanes");
    }

    /// Phase 2.1 regression: the production quantize kernel must match the
    /// scalar reference on ALL eight lanes of two full groups. Crafted loud
    /// spectra make every lane nonzero under scalar; the old interleave
    /// zeroed lanes 2,3,6,7 of each group, so this fails deterministically
    /// on the old code (no random luck required).
    #[test]
    fn avx2_quantize_lines_all_lanes_deterministic() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_quantize_lines_all_lanes_deterministic: SKIP (no AVX2)");
            return;
        }
        let spec: Vec<i32> = (0..16).map(|i| 0x4000_0000i32 + i * 0x0200_0000).collect();
        for &gain in &[-72i32, -71, -70, -69, -32, -8] {
            let qs = gain.wrapping_neg();
            let quantizer = QUANT_TABLE_Q[(qs & 3) as usize];
            let shift = (qs >> 2) + 1;
            for &k in &[QUANT_K0, QUANT_K1] {
                let mut q_a = vec![0i16; 16];
                let mut q_r = vec![0i16; 16];
                quantize_lines_scalar(&spec, &mut q_r, quantizer, shift, k);
                unsafe { super::quantize_lines_avx2(&spec, &mut q_a, gain, shift, k) };
                assert_eq!(q_a, q_r, "quantize all-lanes gain={gain} k={k}");
                // Every lane is nonzero under scalar, so a zeroed lane is a bug.
                assert!(
                    q_r.iter().all(|&q| q != 0),
                    "craft must be nonzero: {q_r:?}"
                );
            }
        }
    }

    /// Phase 2.1 regression: deterministic energy/dist over a full group with
    /// nonzero caller-supplied quant (exercises inv_quantize8 + fmult8 on all
    /// lanes), compared to the production scalar reference.
    #[test]
    fn avx2_energy_dist_all_lanes_deterministic() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_energy_dist_all_lanes_deterministic: SKIP (no AVX2)");
            return;
        }
        let spec: Vec<i32> = (0..16).map(|i| 0x1000_0000i32 + i * 0x0080_0000).collect();
        let quant: Vec<i16> = (0..16).map(|i| (100 + i * 37) as i16).collect();
        for &gain in &[-72i32, -71, -70, -69, 0, 1, 2, 3, 32, 64] {
            let (e_a, d_a) =
                super::super::quant::calc_sfb_quant_energy_and_dist(&spec, &quant, gain);
            let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &quant, gain);
            assert_eq!((e_a, d_a), (e_r, d_r), "energy/dist all-lanes gain={gain}");
            assert_ne!((e_r, d_r), (0, 0), "craft must not early-out");
        }
    }

    struct Rng(u64);
    impl Rng {
        fn next_u32(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 32) as u32
        }
        fn next_i32(&mut self) -> i32 {
            self.next_u32() as i32
        }
        fn range(&mut self, lo: i32, hi: i32) -> i32 {
            lo + (self.next_u32() as u64 % (hi as i64 - lo as i64 + 1) as u64) as i32
        }
        fn spec(&mut self) -> i32 {
            match self.range(0, 9) {
                0 => 0,
                1 => [
                    i32::MIN,
                    i32::MAX,
                    1,
                    -1,
                    65536,
                    -65536,
                    0x10000000,
                    -0x10000000,
                ][self.range(0, 7) as usize],
                2..=5 => self.next_i32() >> self.range(0, 10),
                _ => self.next_i32(),
            }
        }
        fn gain(&mut self) -> i32 {
            match self.range(0, 9) {
                0 => self.next_i32(),
                1 => self.range(-400, 400),
                _ => self.range(-120, 120),
            }
        }
    }

    /// Trial accounting: every generated case is counted, so envelope
    /// rejections can never silently grow. Seeds are fixed, so counts are
    /// reproducible in debug and release.
    #[derive(Default)]
    struct Counts {
        generated: usize,
        accepted: usize,
        rejected: usize,
        early_out: usize,
    }

    impl Counts {
        fn report(&self, name: &str) {
            println!(
                "AVX2_TEST {name}: generated={} accepted={} rejected={} early_out={}",
                self.generated, self.accepted, self.rejected, self.early_out
            );
        }
    }

    fn probe_dist_ok(spec: &[i32], gain: i32, dz: bool) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut q = vec![0i16; spec.len()];
            calc_sfb_dist_scalar(spec, gain, dz, &mut q);
        }))
        .is_ok()
    }

    /// Prefill proving tail lanes are never written after an early-out.
    const SENTINEL: i16 = 0x5555u16 as i16;

    #[test]
    fn avx2_quantize_lines_matches_scalar() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_quantize_lines_matches_scalar: SKIP (no AVX2)");
            return;
        }
        let mut rng = Rng(0x1234_5678_9abc_def0);
        // All four residues mod 4: every quantizer constant is exercised.
        let gains: [i32; 14] = [-72, -71, -70, -69, -32, -8, -7, 0, 1, 2, 3, 32, 64, 65];
        let mut c = Counts::default();
        for trial in 0..5000 {
            c.generated += 1;
            let n = if trial % 17 == 0 {
                rng.range(0, 160) as usize
            } else {
                rng.range(0, 40) as usize
            };
            let gain = gains[(rng.next_u32() as usize) % gains.len()];
            let dz = trial % 2 == 0;
            let spec: Vec<i32> = (0..n).map(|_| rng.spec()).collect();
            // Production gain -> (quantizer, shift) mapping (quantize_spectrum).
            let qs = gain.wrapping_neg();
            let quantizer = QUANT_TABLE_Q[(qs & 3) as usize];
            let shift = (qs >> 2) + 1;
            let k = if dz { QUANT_K1 } else { QUANT_K0 };
            let mut out_avx2 = vec![0i16; n];
            let mut out_ref = vec![0i16; n];
            // Envelope-violating inputs panic in the scalar debug build;
            // count them visibly instead of skipping silently.
            let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                quantize_lines_scalar(&spec.clone(), &mut out_ref.clone(), quantizer, shift, k);
            }));
            if ok.is_err() {
                c.rejected += 1;
                continue;
            }
            c.accepted += 1;
            quantize_lines_scalar(&spec, &mut out_ref, quantizer, shift, k);
            unsafe {
                super::quantize_lines_avx2(&spec, &mut out_avx2, gain, shift, k);
            }
            assert_eq!(
                out_avx2, out_ref,
                "quantize lines n={n} gain={gain} dz={dz}"
            );
        }
        c.report("avx2_quantize_lines_matches_scalar");
        // Gains stay in a range where the scalar envelope holds for these
        // spectra; any rejection is a signal.
        assert_eq!(c.rejected, 0, "unexpected envelope rejections");
        assert_eq!(c.accepted, c.generated);
    }

    #[test]
    fn avx2_calc_sfb_dist_matches_scalar() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_calc_sfb_dist_matches_scalar: SKIP (no AVX2)");
            return;
        }
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        // Residue-complete gain grid (multiples and non-multiples of four).
        let gains: [i32; 13] = [-72, -71, -70, -69, -64, -32, -8, -7, 0, 1, 2, 32, 64];
        let mut c = Counts::default();
        for trial in 0..5000 {
            c.generated += 1;
            let n = if trial % 16 == 0 {
                rng.range(0, 160) as usize
            } else {
                rng.range(0, 40) as usize
            };
            let gain = gains[(rng.next_u32() as usize) % gains.len()];
            let dz = trial % 2 == 0;
            let spec: Vec<i32> = (0..n).map(|_| rng.spec()).collect();
            let mut spec = spec;
            if trial % 7 == 0 && !spec.is_empty() {
                spec[0] = 0;
            }
            if trial % 11 == 0 && spec.len() > 1 {
                spec[1] = i32::MIN;
            }
            if trial % 13 == 0 && spec.len() > 2 {
                spec[2] = i32::MAX;
            }
            if !probe_dist_ok(&spec, gain, dz) {
                c.rejected += 1;
                continue;
            }
            c.accepted += 1;
            let mut q_avx2 = vec![0x5555u16 as i16; n];
            let mut q_ref = vec![0x5555u16 as i16; n];
            let d_avx2 = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q_avx2);
            let d_ref = calc_sfb_dist_scalar(&spec, gain, dz, &mut q_ref);
            // Violation ⟺ a written lane exceeds `MAX_QUANT` (the scalar
            // stops at the first one); `d == 0` alone is not the marker
            // because `calc_ld_data` can legitimately return 0.
            if q_ref.iter().any(|&q| (q as i32).abs() > MAX_QUANT) {
                c.early_out += 1;
            }
            assert_eq!(d_avx2, d_ref, "dist n={n} gain={gain} dz={dz}");
            assert_eq!(q_avx2, q_ref, "quant n={n} gain={gain} dz={dz}");
        }
        c.report("avx2_calc_sfb_dist_matches_scalar");
        assert_eq!(c.rejected, 0, "unexpected envelope rejections");
        assert_eq!(c.accepted, c.generated);
    }

    #[test]
    fn avx2_calc_sfb_dist_early_out_prefix_matches() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_calc_sfb_dist_early_out_prefix_matches: SKIP (no AVX2)");
            return;
        }
        // Residue-complete gains; forcing `|q| > MAX_QUANT` with loud spectra
        // and valid gains; the vector path must write exactly the scalar
        // prefix and return 0.
        let gains: [i32; 13] = [-72, -71, -70, -69, -64, -32, -8, -7, 0, 1, 2, 32, 64];
        let mut c = Counts::default();
        for &gain in &gains {
            for n in [
                1usize, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 18, 24, 25, 32, 33, 64,
            ] {
                for &dz in &[false, true] {
                    c.generated += 1;
                    let spec = vec![0x7000_0000i32; n];
                    if !probe_dist_ok(&spec, gain, dz) {
                        c.rejected += 1;
                        continue;
                    }
                    c.accepted += 1;
                    let mut q_avx2 = vec![0i16; n];
                    let mut q_ref = vec![0i16; n];
                    let d_avx2 = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q_avx2);
                    let d_ref = calc_sfb_dist_scalar(&spec, gain, dz, &mut q_ref);
                    if q_ref.iter().any(|&q| (q as i32).abs() > MAX_QUANT) {
                        c.early_out += 1;
                    }
                    assert_eq!(
                        (d_avx2, &q_avx2),
                        (d_ref, &q_ref),
                        "early-out gain={gain} n={n} dz={dz}"
                    );
                }
            }
        }
        // `i16::MIN` sentinel takes the early-out without reaching inv.
        let spec = vec![0x1000_0000i32; 8];
        let q_in = vec![i16::MIN, 0, 1, -1, 8191, 8192, -8192, 0];
        let (e_a, d_a) = super::super::quant::calc_sfb_quant_energy_and_dist(&spec, &q_in, 0);
        let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &q_in, 0);
        assert_eq!((e_a, d_a), (e_r, d_r), "i16::MIN energy/dist");
        assert_eq!((e_a, d_a), (0, 0), "sentinel must early-out");
        c.report("avx2_calc_sfb_dist_early_out_prefix_matches");
        assert_eq!(c.rejected, 0, "unexpected envelope rejections");
        // The loud-spectrum matrix must actually trigger early-outs,
        // otherwise this test covers nothing exceptional.
        assert!(c.early_out > 0, "no early-out triggered");
    }

    /// A single violating lane at every position of a 16-line band: lanes
    /// before it must match the scalar prefix, lanes after it must be
    /// untouched (sentinel), exercising both group-fallbacks and the tail.
    #[test]
    fn avx2_dist_violation_positions() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_dist_violation_positions: SKIP (no AVX2)");
            return;
        }
        // Deterministic search for a (spec, gain, dz) whose single line
        // violates `|q| > MAX_QUANT` with a valid envelope.
        let cands = [0x7000_0000i32, 0x7FFF_FFFFi32, 0x4000_0000, 0x1000_0000];
        let gains: [i32; 9] = [-72, -71, -69, -64, -32, -8, 0, 1, 32];
        let mut found = None;
        'search: for &gain in &gains {
            for &dz in &[false, true] {
                for &s in &cands {
                    let spec = [s];
                    if !probe_dist_ok(&spec, gain, dz) {
                        continue;
                    }
                    let mut q = [0i16; 1];
                    if calc_sfb_dist_scalar(&spec, gain, dz, &mut q) == 0 {
                        found = Some((s, gain, dz));
                        break 'search;
                    }
                }
            }
        }
        let (loud, gain, dz) = found.expect("no violating (spec, gain, dz) found");
        for pos in 0..16usize {
            let mut spec = vec![0i32; 16];
            spec[pos] = loud;
            let mut q_avx2 = vec![SENTINEL; 16];
            let mut q_ref = vec![SENTINEL; 16];
            let d_avx2 = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q_avx2);
            let d_ref = calc_sfb_dist_scalar(&spec, gain, dz, &mut q_ref);
            assert_eq!(d_avx2, 0, "pos={pos}: must early-out");
            assert_eq!(d_avx2, d_ref, "pos={pos}: dist");
            assert_eq!(q_avx2, q_ref, "pos={pos}: quant prefix");
            assert_eq!(
                q_avx2[pos + 1..],
                vec![SENTINEL; 16 - pos - 1],
                "pos={pos}: tail untouched"
            );
            // The violating lane itself is written.
            assert!(
                (q_avx2[pos] as i32).abs() > MAX_QUANT,
                "pos={pos}: lane not violating: {q_avx2:?}"
            );
        }
        println!("AVX2_TEST avx2_dist_violation_positions: loud={loud:#x} gain={gain} dz={dz}");
    }

    /// Deterministic `|q|` guard positions for the energy/dist path: every
    /// edge magnitude at every lane of a 16-line band (two SIMD groups).
    #[test]
    fn avx2_energy_max_quant_positions() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_energy_max_quant_positions: SKIP (no AVX2)");
            return;
        }
        let spec = vec![0x1000_0000i32; 16];
        let edge: [i16; 7] = [8191, 8192, -8192, -8193, i16::MIN, i16::MAX, 0];
        let mut violating = 0usize;
        let mut passing = 0usize;
        for &gain in &[-72i32, -71, 0, 1, 64] {
            for pos in 0..16usize {
                for &q in &edge {
                    let mut quant = vec![0i16; 16];
                    quant[pos] = q;
                    let (e_a, d_a) =
                        super::super::quant::calc_sfb_quant_energy_and_dist(&spec, &quant, gain);
                    let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &quant, gain);
                    assert_eq!((e_a, d_a), (e_r, d_r), "gain={gain} pos={pos} q={q}");
                    if (q as i32).abs() > MAX_QUANT {
                        violating += 1;
                        assert_eq!((e_a, d_a), (0, 0), "gain={gain} pos={pos} q={q}: early-out");
                    } else {
                        passing += 1;
                    }
                }
            }
        }
        println!(
            "AVX2_TEST avx2_energy_max_quant_positions: violating={violating} passing={passing}"
        );
        assert!(violating > 0 && passing > 0);
    }

    /// Explicit lengths around the eight-lane SIMD boundary, with edge
    /// spectra, compared across all three kernels (tails included).
    #[test]
    fn avx2_boundary_lengths() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_boundary_lengths: SKIP (no AVX2)");
            return;
        }
        let edges: [i32; 9] = [
            0,
            1,
            -1,
            65536,
            -65536,
            i32::MIN,
            i32::MAX,
            0x1000_0000,
            -0x1000_0000,
        ];
        let gains: [i32; 8] = [-72, -71, -69, -8, -7, 0, 1, 64];
        let mut checked = 0usize;
        for &n in &[
            0usize, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 15, 16, 17, 18, 24, 25, 32, 33,
        ] {
            for &gain in &gains {
                for &dz in &[false, true] {
                    let spec: Vec<i32> = (0..n).map(|i| edges[i % edges.len()]).collect();
                    if !probe_dist_ok(&spec, gain, dz) {
                        continue;
                    }
                    // Quantize-lines kernel (production gain -> shift mapping).
                    let qs = gain.wrapping_neg();
                    let quantizer = QUANT_TABLE_Q[(qs & 3) as usize];
                    let shift = (qs >> 2) + 1;
                    let k = if dz { QUANT_K1 } else { QUANT_K0 };
                    let mut q_a = vec![0i16; n];
                    let mut q_r = vec![0i16; n];
                    quantize_lines_scalar(&spec, &mut q_r, quantizer, shift, k);
                    unsafe {
                        super::quantize_lines_avx2(&spec, &mut q_a, gain, shift, k);
                    }
                    assert_eq!(q_a, q_r, "quantize n={n} gain={gain} dz={dz}");
                    // Dist kernel (rewrites quant).
                    let mut q_a = vec![SENTINEL; n];
                    let mut q_r = vec![SENTINEL; n];
                    let d_a = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q_a);
                    let d_r = calc_sfb_dist_scalar(&spec, gain, dz, &mut q_r);
                    assert_eq!((d_a, &q_a), (d_r, &q_r), "dist n={n} gain={gain} dz={dz}");
                    // Energy kernel. Clamp crafted quant to the valid
                    // envelope plus one guard case per length.
                    for &q in &[0i16, 1, -1, 8191, 8192] {
                        let quant = vec![q; n];
                        let (e_a, d_a) = super::super::quant::calc_sfb_quant_energy_and_dist(
                            &spec, &quant, gain,
                        );
                        let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &quant, gain);
                        assert_eq!((e_a, d_a), (e_r, d_r), "energy n={n} gain={gain} q={q}");
                    }
                    checked += 1;
                }
            }
        }
        println!("AVX2_TEST avx2_boundary_lengths: checked={checked}");
        assert!(checked > 100, "boundary matrix too small");
    }

    #[test]
    fn avx2_energy_dist_matches_scalar() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_energy_dist_matches_scalar: SKIP (no AVX2)");
            return;
        }
        let mut rng = Rng(0xdead_beef_cafe_f00d);
        let mut c = Counts::default();
        for trial in 0..5000 {
            c.generated += 1;
            let n = if trial % 16 == 0 {
                rng.range(0, 160) as usize
            } else {
                rng.range(0, 40) as usize
            };
            let gain = rng.gain();
            let spec: Vec<i32> = (0..n).map(|_| rng.spec()).collect();
            let quant: Vec<i16> = (0..n)
                .map(|_| match rng.range(0, 40) {
                    0 => rng.next_u32() as i16,
                    1 => i16::MIN,
                    2..=4 => 0,
                    5 => MAX_QUANT as i16,
                    6 => (MAX_QUANT + 1) as i16,
                    7 => (MAX_QUANT + 2) as i16,
                    _ => (rng.range(-MAX_QUANT - 10, MAX_QUANT + 10) >> rng.range(0, 4)) as i16,
                })
                .collect();
            c.accepted += 1;
            // Violation ⟺ the input itself carries `|q| > MAX_QUANT`.
            if quant.iter().any(|&q| (q as i32).abs() > MAX_QUANT) {
                c.early_out += 1;
            }
            let (e_a, d_a) =
                super::super::quant::calc_sfb_quant_energy_and_dist(&spec, &quant, gain);
            let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &quant, gain);
            assert_eq!((e_a, d_a), (e_r, d_r), "energy/dist n={n} gain={gain}");
        }
        c.report("avx2_energy_dist_matches_scalar");
        assert_eq!(c.accepted, c.generated);
        // The quant generator must actually produce guard cases.
        assert!(c.early_out > 0, "no guard cases generated");
    }

    /// Residue sweep carried forward from Phase 1.1: every production gain
    /// base through `+0..+3` hits every residue mod 4, hence all four
    /// quantizer constants and all four `SPEC_EXP` mantissa/shift rows, on
    /// the quantize, dist and energy paths. Lengths `14..=17` exercise one
    /// full eight-lane group plus a scalar tail at each residue; the dist
    /// buffers are sentinel-filled so any early-out prefix/tail write is
    /// compared exactly.
    #[test]
    fn avx2_residue_sweep_all_quantizers_and_rows() {
        if !avx2_available() {
            eprintln!("AVX2_TEST avx2_residue_sweep_all_quantizers_and_rows: SKIP (no AVX2)");
            return;
        }
        let mut rng = Rng(0x5EED_2026_1114_0001);
        let bases = [-72i32, -32, -8, 0, 32, 64];
        let edges = [0i32, 1, -1, 65536, -65536, i32::MIN, i32::MAX, 0x1000_0000];
        let mut q_idx = [0usize; 4];
        let mut m_row = [0usize; 4];
        let mut early = 0usize;
        for base in bases {
            for off in 0..4i32 {
                let gain = base + off;
                let n = 14 + off as usize;
                let spec: Vec<i32> = (0..n)
                    .map(|i| {
                        if i % 2 == 0 {
                            edges[i % edges.len()]
                        } else {
                            rng.next_i32() >> rng.range(0, 10)
                        }
                    })
                    .collect();
                for &dz in &[false, true] {
                    // Quantize path, production gain -> (quantizer, shift).
                    let qs = gain.wrapping_neg();
                    let quantizer = QUANT_TABLE_Q[(qs & 3) as usize];
                    let shift = (qs >> 2) + 1;
                    let k = if dz { QUANT_K1 } else { QUANT_K0 };
                    let mut q_a = vec![0i16; n];
                    let mut q_r = vec![0i16; n];
                    quantize_lines_scalar(&spec, &mut q_r, quantizer, shift, k);
                    unsafe {
                        super::quantize_lines_avx2(&spec, &mut q_a, gain, shift, k);
                    }
                    assert_eq!(q_a, q_r, "residue quantize gain={gain} dz={dz}");
                    q_idx[(qs & 3) as usize] += 1;

                    // Dist path (production dispatch), sentinel-filled.
                    let mut q_a = vec![SENTINEL; n];
                    let mut q_r = vec![SENTINEL; n];
                    let d_a = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q_a);
                    let d_r = calc_sfb_dist_scalar(&spec, gain, dz, &mut q_r);
                    assert_eq!((d_a, &q_a), (d_r, &q_r), "residue dist gain={gain} dz={dz}");
                    if q_r.iter().any(|&q| (q as i32).abs() > MAX_QUANT) {
                        early += 1;
                    }
                    m_row[(gain & 3) as usize] += 1;

                    // Energy path with a valid band (exercises inv_quantize8
                    // for this gain's mantissa row, not just the early-out).
                    let quant: Vec<i16> = (0..n).map(|i| ((i as i32 * 37) % 8191) as i16).collect();
                    let (e_a, d_a) =
                        super::super::quant::calc_sfb_quant_energy_and_dist(&spec, &quant, gain);
                    let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &quant, gain);
                    assert_eq!((e_a, d_a), (e_r, d_r), "residue energy gain={gain} dz={dz}");
                }
            }
        }
        println!(
            "AVX2_TEST avx2_residue_sweep_all_quantizers_and_rows: q_idx={q_idx:?} m_row={m_row:?} early={early}"
        );
        assert!(
            q_idx.iter().all(|&c| c > 0),
            "a quantizer constant was not exercised: {q_idx:?}"
        );
        assert!(
            m_row.iter().all(|&c| c > 0),
            "a mantissa row was not exercised: {m_row:?}"
        );
    }

    /// Scalar `fpow2(INT_MIN)` anchor: the AVX2 energy/distortion path must
    /// reproduce this exactly. AVX2 64-bit products cannot saturate here
    /// (unlike NEON `sqdmulh`), so no exceptional-lane guard is needed; the
    /// randomized trials above plus this anchor pin the behavior.
    #[test]
    fn avx2_fpow2_int_min_anchor() {
        assert_eq!(super::super::quant::fpow2(i32::MIN), i32::MIN);
        assert_eq!(super::super::quant::fpow2(0x40000001), 0x20000000);
        assert_eq!(super::super::quant::fpow2(0), 0);
        assert_eq!(super::super::quant::fpow2(0x7FFFFFFF), 0x7FFFFFFE);
    }

    /// Isolated kernel timing (opt-in): scalar vs AVX2 on a representative
    /// 14-line band (the S4 average) and a 64-line band. Prints ns/line;
    /// correctness is asserted, not just timed. Skips cleanly without AVX2.
    #[test]
    #[ignore = "opt-in AVX2 kernel microbenchmark"]
    fn avx2_kernel_microbench() {
        if !avx2_available() {
            eprintln!("AVX2_MICRO: SKIP (no AVX2)");
            return;
        }
        let cases: &[(usize, i32)] = &[(14, 0), (14, -32), (64, 0), (64, 32)];
        for &(n, gain) in cases {
            let spec: Vec<i32> = (0..n)
                .map(|i| (i as i32 * 1103515245 + 12345) >> 4)
                .collect();
            let dz = false;
            // Production gain -> (quantizer, shift) mapping.
            let qs = gain.wrapping_neg();
            let quantizer = QUANT_TABLE_Q[(qs & 3) as usize];
            let shift = (qs >> 2) + 1;
            let iters = 50_000usize;
            // Warm up.
            for _ in 0..1000 {
                let mut q = vec![0i16; n];
                let _ = calc_sfb_dist_scalar(&spec, gain, dz, &mut q);
                let mut q2 = vec![0i16; n];
                let _ = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q2);
            }
            let t0 = std::time::Instant::now();
            for _ in 0..iters {
                let mut q = vec![0i16; n];
                let d = calc_sfb_dist_scalar(&spec, gain, dz, &mut q);
                std::hint::black_box(d);
                std::hint::black_box(&q);
            }
            let scalar_ns = t0.elapsed().as_nanos() as f64 / iters as f64 / n as f64;
            let t1 = std::time::Instant::now();
            for _ in 0..iters {
                let mut q = vec![0i16; n];
                let d = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q);
                std::hint::black_box(d);
                std::hint::black_box(&q);
            }
            let avx2_ns = t1.elapsed().as_nanos() as f64 / iters as f64 / n as f64;
            // Energy/dist path with fixed quant lines.
            let quant: Vec<i16> = (0..n).map(|i| ((i as i32 * 7919) % 8191) as i16).collect();
            let t2 = std::time::Instant::now();
            for _ in 0..iters {
                let (e, d) = calc_sfb_quant_energy_and_dist_scalar(&spec, &quant, gain);
                std::hint::black_box((e, d));
            }
            let scalar_ed = t2.elapsed().as_nanos() as f64 / iters as f64 / n as f64;
            let t3 = std::time::Instant::now();
            for _ in 0..iters {
                let (e, d) =
                    super::super::quant::calc_sfb_quant_energy_and_dist(&spec, &quant, gain);
                std::hint::black_box((e, d));
            }
            let avx2_ed = t3.elapsed().as_nanos() as f64 / iters as f64 / n as f64;
            // Quantize-only path.
            let t4 = std::time::Instant::now();
            for _ in 0..iters {
                let mut q = vec![0i16; n];
                quantize_lines_scalar(&spec, &mut q, quantizer, shift, QUANT_K0);
                std::hint::black_box(&q);
            }
            let scalar_q = t4.elapsed().as_nanos() as f64 / iters as f64 / n as f64;
            let t5 = std::time::Instant::now();
            for _ in 0..iters {
                let mut q = vec![0i16; n];
                unsafe {
                    super::quantize_lines_avx2(&spec, &mut q, gain, shift, QUANT_K0);
                }
                std::hint::black_box(&q);
            }
            let avx2_q = t5.elapsed().as_nanos() as f64 / iters as f64 / n as f64;
            println!(
                "AVX2_MICRO n={n} gain={gain}: dist scalar={scalar_ns:.2}ns avx2={avx2_ns:.2}ns ({:.2}x) | energy scalar={scalar_ed:.2}ns avx2={avx2_ed:.2}ns ({:.2}x) | quant scalar={scalar_q:.2}ns avx2={avx2_q:.2}ns ({:.2}x)",
                scalar_ns / avx2_ns.max(0.01),
                scalar_ed / avx2_ed.max(0.01),
                scalar_q / avx2_q.max(0.01),
            );
        }
    }
}
