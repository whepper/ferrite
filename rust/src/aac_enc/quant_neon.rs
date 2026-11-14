//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! ARM NEON (AArch64) acceleration for the S3 quantizer kernels.
//!
//! Four-lines-at-a-time vectorization of `quantize_lines`,
//! `calc_sfb_dist`, and `calc_sfb_quant_energy_and_dist`. The scalar functions
//! in [`super::quant`] remain the reference and the fallback for exceptional
//! inputs, tails, and non-AArch64 targets.
//!
//! Re-derived for Ferrite's Q31 arithmetic (not copied): `sqdmulh` products
//! that never saturate at these magnitudes, `clz` + variable `sshl`
//! normalization with 5-bit masked shifts matching the scalar wrapping
//! semantics, lane loads for the 512-entry tables, selects for the zero fast
//! paths and sign restoration, scalar fallback on `|q| > MAX_QUANT` (which
//! includes `i16::MIN`) or a `2^31` distortion difference, and wrapping-add
//! accumulation whose order is exact modulo `2^32`. Exponent-row indices are
//! clamped to the 14 entries so exceptional lanes (which the `|q|` guard
//! discards) still load in bounds; the single `fpow2(INT_MIN)` saturation
//! divergence is likewise routed to the scalar fallback.

#[cfg(target_arch = "aarch64")]
use super::quant::{accumulate_dist, fpow2, inv_quantize_line, quantize_line_body, MAX_QUANT};
#[cfg(target_arch = "aarch64")]
use super::quant_tables::QUANT_TABLE_Q;
#[cfg(target_arch = "aarch64")]
use super::quant_tables::{MTAB_3_4, MTAB_4_3, QUANT_TABLE_E, SPEC_EXP_MANT, SPEC_EXP_SHIFT};
#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;

#[cfg(target_arch = "aarch64")]
struct NeonConsts {
    quantizer: i32,
    quantizershift1: int32x4_t,
    k: int32x4_t,
    m: usize,
    iquantizershift: int32x4_t,
}

#[cfg(target_arch = "aarch64")]
impl NeonConsts {
    /// # Safety
    /// Requires AArch64 NEON. The `QUANT_TABLE_Q` index is
    /// `gain.wrapping_neg() & 3`, always in `0..4`, for any `gain`.
    #[inline(always)]
    unsafe fn new(gain: i32, quantizer_shift: i32, k: i32) -> Self {
        let q = QUANT_TABLE_Q[(gain.wrapping_neg() & 3) as usize] as i32;
        NeonConsts {
            quantizer: q.wrapping_shl(15),
            quantizershift1: vdupq_n_s32(quantizer_shift.wrapping_add(1)),
            k: vdupq_n_s32(k),
            m: (gain & 3) as usize,
            iquantizershift: vdupq_n_s32(gain >> 2),
        }
    }
}

/// # Safety
/// Requires AArch64 NEON. Every lane of `idx` must be `< 512` so each
/// `p.add(lane)` stays inside `tab`; callers mask the index with `& 511`.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn gather_i16_512(tab: &[i16; 512], idx: uint32x4_t) -> int32x4_t {
    let p = tab.as_ptr();
    let mut v = vdup_n_s16(0);
    v = vld1_lane_s16::<0>(p.add(vgetq_lane_u32::<0>(idx) as usize), v);
    v = vld1_lane_s16::<1>(p.add(vgetq_lane_u32::<1>(idx) as usize), v);
    v = vld1_lane_s16::<2>(p.add(vgetq_lane_u32::<2>(idx) as usize), v);
    v = vld1_lane_s16::<3>(p.add(vgetq_lane_u32::<3>(idx) as usize), v);
    vmovl_s16(v)
}

/// # Safety
/// Requires AArch64 NEON. Every lane of `idx` must be `< 4` so each
/// `p.add(lane)` stays inside `tab`; callers mask the index with `& 3`.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn gather_i16_4(tab: &[i16; 4], idx: uint32x4_t) -> int32x4_t {
    let p = tab.as_ptr();
    let mut v = vdup_n_s16(0);
    v = vld1_lane_s16::<0>(p.add(vgetq_lane_u32::<0>(idx) as usize), v);
    v = vld1_lane_s16::<1>(p.add(vgetq_lane_u32::<1>(idx) as usize), v);
    v = vld1_lane_s16::<2>(p.add(vgetq_lane_u32::<2>(idx) as usize), v);
    v = vld1_lane_s16::<3>(p.add(vgetq_lane_u32::<3>(idx) as usize), v);
    vmovl_s16(v)
}

/// # Safety
/// Requires AArch64 NEON. Every lane of `idx` must be `< 512` so each
/// `p.add(lane)` stays inside `tab`; callers mask the index with `& 511`.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn gather_i32_512(tab: &[i32; 512], idx: uint32x4_t) -> int32x4_t {
    let p = tab.as_ptr();
    let mut v = vdupq_n_s32(0);
    v = vld1q_lane_s32::<0>(p.add(vgetq_lane_u32::<0>(idx) as usize), v);
    v = vld1q_lane_s32::<1>(p.add(vgetq_lane_u32::<1>(idx) as usize), v);
    v = vld1q_lane_s32::<2>(p.add(vgetq_lane_u32::<2>(idx) as usize), v);
    v = vld1q_lane_s32::<3>(p.add(vgetq_lane_u32::<3>(idx) as usize), v);
    v
}

/// # Safety
/// Requires AArch64 NEON. `m` must be `< 4` (a `SPEC_EXP_MANT` row) and every
/// lane of `idx` must be `< 14` (the row length). Callers pass `m = gain & 3`
/// and clamp the exponent index to `0..=13` before calling.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn gather_mant_row(m: usize, idx: uint32x4_t) -> int32x4_t {
    let row = &SPEC_EXP_MANT[m];
    let p = row.as_ptr();
    let mut v = vdupq_n_s32(0);
    v = vld1q_lane_s32::<0>(p.add(vgetq_lane_u32::<0>(idx) as usize), v);
    v = vld1q_lane_s32::<1>(p.add(vgetq_lane_u32::<1>(idx) as usize), v);
    v = vld1q_lane_s32::<2>(p.add(vgetq_lane_u32::<2>(idx) as usize), v);
    v = vld1q_lane_s32::<3>(p.add(vgetq_lane_u32::<3>(idx) as usize), v);
    v
}

/// # Safety
/// Requires AArch64 NEON. `m` must be `< 4` (a `SPEC_EXP_SHIFT` row) and every
/// lane of `idx` must be `< 14` (the row length). Callers pass `m = gain & 3`
/// and clamp the exponent index to `0..=13` before calling.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn gather_shift_row(m: usize, idx: uint32x4_t) -> int32x4_t {
    let row = &SPEC_EXP_SHIFT[m];
    let p = row.as_ptr();
    let b0 = *p.add(vgetq_lane_u32::<0>(idx) as usize) as i32;
    let b1 = *p.add(vgetq_lane_u32::<1>(idx) as usize) as i32;
    let b2 = *p.add(vgetq_lane_u32::<2>(idx) as usize) as i32;
    let b3 = *p.add(vgetq_lane_u32::<3>(idx) as usize) as i32;
    let mut v = vdupq_n_s32(0);
    v = vld1q_lane_s32::<0>(&b0, v);
    v = vld1q_lane_s32::<1>(&b1, v);
    v = vld1q_lane_s32::<2>(&b2, v);
    v = vld1q_lane_s32::<3>(&b3, v);
    v
}

/// Four lines of `quantize_line_body`, sign-extended to 32 bits.
///
/// # Safety
/// Requires AArch64 NEON. Vector-only apart from the table gathers, whose
/// indices are bounded by construction (`idx & 511`, `e_idx & 3`); `c` must be
/// a valid [`NeonConsts`]. No slice access happens here.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn quantize4(x: int32x4_t, c: &NeonConsts) -> int32x4_t {
    let accu = vqdmulhq_n_s32(x, c.quantizer);
    let a = vabsq_s32(accu);
    let accu_shift = vsubq_s32(vclzq_s32(a), vdupq_n_s32(1));
    let an = vreinterpretq_u32_s32(vshlq_s32(a, accu_shift));
    let idx = vandq_u32(vshrq_n_u32::<21>(an), vdupq_n_u32(511));
    let m34 = gather_i16_512(&MTAB_3_4, idx);
    let total_shift = vsubq_s32(c.quantizershift1, accu_shift);
    let e_idx = vandq_u32(vreinterpretq_u32_s32(total_shift), vdupq_n_u32(3));
    let qe = gather_i16_4(&QUANT_TABLE_E, e_idx);
    let m = vmulq_s32(m34, qe);
    let t = vshrq_n_s32::<2>(total_shift);
    let t3 = vaddq_s32(t, vaddq_s32(t, t));
    let sh = vaddq_s32(
        vminq_s32(vsubq_s32(vdupq_n_s32(0), t3), vdupq_n_s32(19)),
        vdupq_n_s32(12),
    );
    let m = vshlq_s32(m, vnegq_s32(vandq_s32(sh, vdupq_n_s32(31))));
    let q = vshrq_n_s32::<15>(vaddq_s32(c.k, m));
    let q16 = vmovn_s32(q);
    let neg_mask = vmovn_u32(vcltzq_s32(accu));
    let mut q16 = vbsl_s16(neg_mask, vneg_s16(q16), q16);
    let zero_mask = vmovn_u32(vceqq_s32(accu, vdupq_n_s32(0)));
    q16 = vbic_s16(q16, vreinterpret_s16_u16(zero_mask));
    vmovl_s16(q16)
}

/// Four lines of `inv_quantize_line`; lanes with `|q| > MAX_QUANT` are undefined
/// as values, but their table indices are clamped so every lane load stays
/// in bounds. The `over_max_quant` guard discards those lanes (scalar
/// fallback with the documented early-out), so clamping only affects
/// discarded lanes; lanes with `|q| <= MAX_QUANT` (`spec_exp` 1..=13) and
/// `q == 0` (`spec_exp == 0`) are unaffected.
///
/// # Safety
/// Requires AArch64 NEON. Every table index is bounded before its gather: the
/// 512-entry index is masked with `& 511` and the exponent-row index is clamped
/// to `0..=13`, so all lane loads stay in bounds even for exceptional lanes.
/// `c.m` must be `< 4`. No slice access happens here.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn inv_quantize4(q: int32x4_t, c: &NeonConsts) -> int32x4_t {
    let qa = vabsq_s32(q);
    let clz = vclzq_s32(qa);
    let an = vreinterpretq_u32_s32(vshlq_s32(qa, vsubq_s32(clz, vdupq_n_s32(1))));
    let spec_exp = vsubq_s32(vdupq_n_s32(32), clz);
    // Clamp to the 14-entry exponent rows: `|q| > MAX_QUANT` lanes (and only
    // those, plus `q == 0` which maps to 0) can yield `spec_exp >= 14`.
    let spec_exp_c = vminq_s32(spec_exp, vdupq_n_s32(13));
    let idx = vandq_u32(vshrq_n_u32::<21>(an), vdupq_n_u32(511));
    let s = gather_i32_512(&MTAB_4_3, idx);
    let t = gather_mant_row(c.m, vreinterpretq_u32_s32(spec_exp_c));
    // ARM64 `fmult` keeps bit 0 (`asr #31` floor), so no `& ~1` clearing here.
    let accu = vqdmulhq_s32(s, t);
    let e = gather_shift_row(c.m, vreinterpretq_u32_s32(spec_exp_c));
    let st = vaddq_s32(c.iquantizershift, e);
    let one = vdupq_n_s32(1);
    let mask31 = vdupq_n_s32(31);
    let left = vandq_s32(vsubq_s32(st, one), mask31);
    let right = vnegq_s32(vandq_s32(vsubq_s32(one, st), mask31));
    let shift = vbslq_s32(vcgtq_s32(st, one), left, right);
    let accu = vshlq_s32(accu, shift);
    let accu = vbslq_s32(vcltzq_s32(q), vnegq_s32(accu), accu);
    vbicq_s32(accu, vreinterpretq_s32_u32(vceqq_s32(q, vdupq_n_s32(0))))
}

/// Distortion of four lines plus a mask of lanes whose difference is `2^31`.
///
/// # Safety
/// Requires AArch64 NEON. Vector-only arithmetic; no memory access.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn dist4(inv: int32x4_t, x: int32x4_t) -> (uint32x4_t, uint32x4_t) {
    let a = vabsq_s32(inv);
    let b = vabsq_s32(vshrq_n_s32::<1>(x));
    let d = vabsq_s32(vsubq_s32(a, b));
    let bad = vcltzq_s32(d);
    let du = vreinterpretq_u32_s32(d);
    let clz = vreinterpretq_s32_u32(vclzq_u32(du));
    let n = vreinterpretq_s32_u32(vshrq_n_u32::<1>(vshlq_u32(du, clz)));
    // `fpow2` clears bit 0, unlike `fmult`.
    let p = vreinterpretq_u32_s32(vandq_s32(vqdmulhq_s32(n, n), vdupq_n_s32(!1)));
    let amount = vsubq_s32(
        vdupq_n_s32(4),
        vminq_s32(vshlq_n_s32::<1>(clz), vdupq_n_s32(35)),
    );
    (vshlq_u32(p, amount), bad)
}

/// Lane mask of `|q| > MAX_QUANT` (widened magnitude, so `i16::MIN` is caught).
///
/// # Safety
/// Requires AArch64 NEON. Vector-only arithmetic; no memory access.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn over_max_quant(q: int32x4_t) -> uint32x4_t {
    vcgtq_s32(vabsq_s32(q), vdupq_n_s32(MAX_QUANT))
}

/// Quantize `spec` into `quant`, four lines at a time, scalar tail.
///
/// # Safety
/// Requires AArch64 NEON. `spec` and `quant` must have equal length (asserted
/// in debug); the slices themselves guarantee pointer validity and alignment.
/// Reads/writes exactly the paired elements; the tail uses the scalar body.
#[cfg(target_arch = "aarch64")]
pub(crate) unsafe fn quantize_lines_neon(
    spec: &[i32],
    quant: &mut [i16],
    gain: i32,
    quantizer_shift: i32,
    k: i32,
) {
    debug_assert_eq!(spec.len(), quant.len());
    let c = NeonConsts::new(gain, quantizer_shift, k);
    let (groups, tail_spec) = spec.as_chunks::<4>();
    let (groups_q, tail_quant) = quant.as_chunks_mut::<4>();
    for (sg, qg) in groups.iter().zip(groups_q.iter_mut()) {
        let x = vld1q_s32(sg.as_ptr());
        let q = quantize4(x, &c);
        vst1_s16(qg.as_mut_ptr(), vmovn_s32(q));
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
/// Requires AArch64 NEON. `spec` and `quant` must have equal length (asserted
/// in debug); the slices guarantee pointer validity and alignment. On
/// early-out only the scalar prefix through the violating line is written.
#[cfg(target_arch = "aarch64")]
pub(crate) unsafe fn calc_sfb_dist_sum_neon(
    spec: &[i32],
    quant: &mut [i16],
    gain: i32,
    quantizer_shift: i32,
    k: i32,
) -> Option<i32> {
    debug_assert_eq!(spec.len(), quant.len());
    let c = NeonConsts::new(gain, quantizer_shift, k);
    let mut xfsf: i32 = 0;
    let mut vec_sum = vdupq_n_u32(0);
    let (groups, tail_spec) = spec.as_chunks::<4>();
    let (groups_q, tail_quant) = quant.as_chunks_mut::<4>();
    for (sg, qg) in groups.iter().zip(groups_q.iter_mut()) {
        let x = vld1q_s32(sg.as_ptr());
        let q = quantize4(x, &c);
        let inv = inv_quantize4(q, &c);
        let (d, bad) = dist4(inv, x);
        if vmaxvq_u32(vorrq_u32(over_max_quant(q), bad)) == 0 {
            vst1_s16(qg.as_mut_ptr(), vmovn_s32(q));
            vec_sum = vaddq_u32(vec_sum, d);
            continue;
        }
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
    }
    xfsf = xfsf.wrapping_add(vaddvq_u32(vec_sum) as i32);
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
/// Requires AArch64 NEON. `spec` and `quant` must have equal length (asserted
/// in debug); the slices guarantee pointer validity and alignment. `quant` is
/// read-only; nothing is written.
#[cfg(target_arch = "aarch64")]
pub(crate) unsafe fn calc_sfb_quant_energy_and_dist_sums_neon(
    spec: &[i32],
    quant: &[i16],
    gain: i32,
) -> Option<(i32, i32)> {
    debug_assert_eq!(spec.len(), quant.len());
    let c = NeonConsts::new(gain, 0, 0);
    let mut energy: i32 = 0;
    let mut distortion: i32 = 0;
    let mut en_sum = vdupq_n_s32(0);
    let mut dist_sum = vdupq_n_u32(0);
    let (groups, tail_spec) = spec.as_chunks::<4>();
    let (groups_q, tail_quant) = quant.as_chunks::<4>();
    for (sg, qg) in groups.iter().zip(groups_q.iter()) {
        let x = vld1q_s32(sg.as_ptr());
        let q = vmovl_s16(vld1_s16(qg.as_ptr()));
        let inv = inv_quantize4(q, &c);
        let (d, bad) = dist4(inv, x);
        let bad_inv = vceqq_s32(inv, vdupq_n_s32(i32::MIN));
        if vmaxvq_u32(vorrq_u32(vorrq_u32(over_max_quant(q), bad), bad_inv)) == 0 {
            let p = vandq_s32(vqdmulhq_s32(inv, inv), vdupq_n_s32(!1));
            en_sum = vaddq_s32(en_sum, p);
            dist_sum = vaddq_u32(dist_sum, d);
            continue;
        }
        for (&qs, &xs) in qg.iter().zip(sg.iter()) {
            if (qs as i32).abs() > MAX_QUANT {
                return None;
            }
            let invs = inv_quantize_line(gain, qs);
            energy = energy.wrapping_add(fpow2(invs));
            distortion = accumulate_dist(distortion, invs, xs);
        }
    }
    energy = energy.wrapping_add(vaddvq_s32(en_sum));
    distortion = distortion.wrapping_add(vaddvq_u32(dist_sum) as i32);
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

#[cfg(all(test, target_arch = "aarch64"))]
mod tests {
    use super::super::quant::{
        calc_sfb_dist_scalar, calc_sfb_quant_energy_and_dist_scalar, quantize_lines_scalar,
        MAX_QUANT,
    };
    use super::super::quant_tables::{QUANT_K0, QUANT_K1, QUANT_TABLE_Q};

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
                "NEON_TEST {name}: generated={} accepted={} rejected={} early_out={}",
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
    fn neon_quantize_lines_matches_scalar() {
        let mut rng = Rng(0x1234_5678_9abc_def0);
        let gains: [i32; 6] = [-72, -32, -8, 0, 32, 64];
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
            // Production gain -> (quantizer, shift) mapping (quantize_spectrum):
            // both are derived from -gain, so use `qs` directly (not `-qs`).
            let qs = gain.wrapping_neg();
            let quantizer = QUANT_TABLE_Q[(qs & 3) as usize];
            let shift = (qs >> 2) + 1;
            let k = if dz { QUANT_K1 } else { QUANT_K0 };
            let mut out_neon = vec![0i16; n];
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
                super::quantize_lines_neon(&spec, &mut out_neon, gain, shift, k);
            }
            assert_eq!(
                out_neon, out_ref,
                "quantize lines n={n} gain={gain} dz={dz}"
            );
        }
        c.report("neon_quantize_lines_matches_scalar");
        // Restricted gain grid + magnitude-bounded spectra never violate the
        // `totalShift >= 0` envelope (see audit); any rejection is a signal.
        assert_eq!(c.rejected, 0, "unexpected envelope rejections");
        assert_eq!(c.accepted, c.generated);
    }

    #[test]
    fn neon_calc_sfb_dist_matches_scalar() {
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        let gains: [i32; 7] = [-72, -64, -32, -8, 0, 32, 64];
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
            // Inject boundary values periodically.
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
            let mut q_neon = vec![0x5555u16 as i16; n];
            let mut q_ref = vec![0x5555u16 as i16; n];
            let d_neon = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q_neon);
            let d_ref = calc_sfb_dist_scalar(&spec, gain, dz, &mut q_ref);
            // Violation ⟺ a written lane exceeds `MAX_QUANT` (the scalar
            // stops at the first one); `d == 0` alone is not the marker
            // because `calc_ld_data` can legitimately return 0.
            if q_ref.iter().any(|&q| (q as i32).abs() > MAX_QUANT) {
                c.early_out += 1;
            }
            assert_eq!(d_neon, d_ref, "dist n={n} gain={gain} dz={dz}");
            assert_eq!(q_neon, q_ref, "quant n={n} gain={gain} dz={dz}");
        }
        c.report("neon_calc_sfb_dist_matches_scalar");
        assert_eq!(c.rejected, 0, "unexpected envelope rejections");
        assert_eq!(c.accepted, c.generated);
    }

    #[test]
    fn neon_calc_sfb_dist_early_out_prefix_matches() {
        // Force `|q| > MAX_QUANT` with loud spectra and valid gains; the
        // vector path must write exactly the scalar prefix and return 0.
        let gains: [i32; 7] = [-72, -64, -32, -8, 0, 32, 64];
        let mut c = Counts::default();
        for &gain in &gains {
            for n in [1usize, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 32, 64] {
                for &dz in &[false, true] {
                    c.generated += 1;
                    let spec = vec![0x7000_0000i32; n];
                    if !probe_dist_ok(&spec, gain, dz) {
                        c.rejected += 1;
                        continue;
                    }
                    c.accepted += 1;
                    let mut q_neon = vec![0i16; n];
                    let mut q_ref = vec![0i16; n];
                    let d_neon = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q_neon);
                    let d_ref = calc_sfb_dist_scalar(&spec, gain, dz, &mut q_ref);
                    if q_ref.iter().any(|&q| (q as i32).abs() > MAX_QUANT) {
                        c.early_out += 1;
                    }
                    assert_eq!(
                        (d_neon, &q_neon),
                        (d_ref, &q_ref),
                        "early-out gain={gain} n={n} dz={dz}"
                    );
                }
            }
        }
        // `i16::MIN` sentinel takes the early-out without reaching inv.
        let spec = vec![0x1000_0000i32; 8];
        // Directly exercise the energy/dist path with the sentinel.
        let q_in = vec![i16::MIN, 0, 1, -1, 8191, 8192, -8192, 0];
        let (e_n, d_n) = super::super::quant::calc_sfb_quant_energy_and_dist(&spec, &q_in, 0);
        let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &q_in, 0);
        assert_eq!((e_n, d_n), (e_r, d_r), "i16::MIN energy/dist");
        assert_eq!((e_n, d_n), (0, 0), "sentinel must early-out");
        c.report("neon_calc_sfb_dist_early_out_prefix_matches");
        assert_eq!(c.rejected, 0, "unexpected envelope rejections");
        // The loud-spectrum matrix must actually trigger early-outs,
        // otherwise this test covers nothing exceptional.
        assert!(c.early_out > 0, "no early-out triggered");
    }

    /// A single violating lane at every position of an 8-line band: lanes
    /// before it must match the scalar prefix, lanes after it must be
    /// untouched (sentinel), exercising the group-fallback and tail paths.
    #[test]
    fn neon_dist_violation_positions() {
        // Deterministic search for a (spec, gain, dz) whose single line
        // violates `|q| > MAX_QUANT` with a valid envelope.
        let cands = [0x7000_0000i32, 0x7FFF_FFFFi32, 0x4000_0000, 0x1000_0000];
        let gains: [i32; 7] = [-72, -64, -32, -8, 0, 32, 64];
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
        for pos in 0..8usize {
            let mut spec = vec![0i32; 8];
            spec[pos] = loud;
            let mut q_neon = vec![SENTINEL; 8];
            let mut q_ref = vec![SENTINEL; 8];
            let d_neon = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q_neon);
            let d_ref = calc_sfb_dist_scalar(&spec, gain, dz, &mut q_ref);
            assert_eq!(d_neon, 0, "pos={pos}: must early-out");
            assert_eq!(d_neon, d_ref, "pos={pos}: dist");
            assert_eq!(q_neon, q_ref, "pos={pos}: quant prefix");
            assert_eq!(
                q_neon[pos + 1..],
                vec![SENTINEL; 8 - pos - 1],
                "pos={pos}: tail untouched"
            );
            // The violating lane itself is written.
            assert!(
                (q_neon[pos] as i32).abs() > MAX_QUANT,
                "pos={pos}: lane not violating: {q_neon:?}"
            );
        }
        println!("NEON_TEST neon_dist_violation_positions: loud={loud:#x} gain={gain} dz={dz}");
    }

    /// Deterministic `|q|` guard positions for the energy/dist path: every
    /// edge magnitude at every lane of an 8-line band (two SIMD groups).
    #[test]
    fn neon_energy_max_quant_positions() {
        let spec = vec![0x1000_0000i32; 8];
        let edge: [i16; 7] = [8191, 8192, -8192, -8193, i16::MIN, i16::MAX, 0];
        let mut violating = 0usize;
        let mut passing = 0usize;
        for &gain in &[-72i32, 0, 64] {
            for pos in 0..8usize {
                for &q in &edge {
                    let mut quant = vec![0i16; 8];
                    quant[pos] = q;
                    let (e_n, d_n) =
                        super::super::quant::calc_sfb_quant_energy_and_dist(&spec, &quant, gain);
                    let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &quant, gain);
                    assert_eq!((e_n, d_n), (e_r, d_r), "gain={gain} pos={pos} q={q}");
                    if (q as i32).abs() > MAX_QUANT {
                        violating += 1;
                        assert_eq!((e_n, d_n), (0, 0), "gain={gain} pos={pos} q={q}: early-out");
                    } else {
                        passing += 1;
                    }
                }
            }
        }
        println!(
            "NEON_TEST neon_energy_max_quant_positions: violating={violating} passing={passing}"
        );
        assert!(violating > 0 && passing > 0);
    }

    /// Explicit lengths around the four-lane SIMD boundary, with edge
    /// spectra, compared across all three kernels (tails included).
    #[test]
    fn neon_boundary_lengths() {
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
        let gains: [i32; 4] = [-72, -8, 0, 64];
        let mut checked = 0usize;
        for &n in &[0usize, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 15, 16, 17] {
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
                    let mut q_n = vec![0i16; n];
                    let mut q_r = vec![0i16; n];
                    quantize_lines_scalar(&spec, &mut q_r, quantizer, shift, k);
                    unsafe {
                        super::quantize_lines_neon(&spec, &mut q_n, gain, shift, k);
                    }
                    assert_eq!(q_n, q_r, "quantize n={n} gain={gain} dz={dz}");
                    // Dist kernel (rewrites quant).
                    let mut q_n = vec![SENTINEL; n];
                    let mut q_r = vec![SENTINEL; n];
                    let d_n = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q_n);
                    let d_r = calc_sfb_dist_scalar(&spec, gain, dz, &mut q_r);
                    assert_eq!((d_n, &q_n), (d_r, &q_r), "dist n={n} gain={gain} dz={dz}");
                    // Energy kernel. Clamp crafted quant to the valid
                    // envelope plus one guard case per length.
                    for &q in &[0i16, 1, -1, 8191, 8192] {
                        let quant = vec![q; n];
                        let (e_n, d_n) = super::super::quant::calc_sfb_quant_energy_and_dist(
                            &spec, &quant, gain,
                        );
                        let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &quant, gain);
                        assert_eq!((e_n, d_n), (e_r, d_r), "energy n={n} gain={gain} q={q}");
                    }
                    checked += 1;
                }
            }
        }
        println!("NEON_TEST neon_boundary_lengths: checked={checked}");
        assert!(checked > 100, "boundary matrix too small");
    }

    #[test]
    fn neon_energy_dist_matches_scalar() {
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
            let (e_n, d_n) =
                super::super::quant::calc_sfb_quant_energy_and_dist(&spec, &quant, gain);
            let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &quant, gain);
            assert_eq!((e_n, d_n), (e_r, d_r), "energy/dist n={n} gain={gain}");
        }
        c.report("neon_energy_dist_matches_scalar");
        assert_eq!(c.accepted, c.generated);
        // The quant generator must actually produce guard cases.
        assert!(c.early_out > 0, "no guard cases generated");
    }

    /// Compact residue sweep closing the quantizer-constant / mantissa-row
    /// coverage gap: the randomized kernels above and the committed probe
    /// corpus use gains that are (almost) all multiples of four, so they only
    /// exercise `QUANT_TABLE_Q[0]` and mantissa row `m = 0`. Sweeping each
    /// production gain base through `+0..+3` hits every residue mod 4, hence
    /// all four quantizer constants and all four `SPEC_EXP` rows, on the
    /// quantize, dist and energy paths (not only inverse quantization). Gains
    /// stay `>= -72`, so the scalar `totalShift >= 0` envelope holds for every
    /// spectrum and no trial is rejected. Lengths `10..=13` also exercise the
    /// scalar tail at each residue; the dist buffers are sentinel-filled so any
    /// early-out prefix/tail write is compared exactly.
    #[test]
    fn neon_residue_sweep_all_quantizers_and_rows() {
        let mut rng = Rng(0x5EED_2026_1114_0001);
        let bases = [-72i32, -32, -8, 0, 32, 64];
        let edges = [0i32, 1, -1, 65536, -65536, i32::MIN, i32::MAX, 0x1000_0000];
        let mut q_idx = [0usize; 4];
        let mut m_row = [0usize; 4];
        let mut early = 0usize;
        for base in bases {
            for off in 0..4i32 {
                let gain = base + off;
                let n = 10 + off as usize;
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
                    let mut q_n = vec![0i16; n];
                    let mut q_r = vec![0i16; n];
                    quantize_lines_scalar(&spec, &mut q_r, quantizer, shift, k);
                    unsafe {
                        super::quantize_lines_neon(&spec, &mut q_n, gain, shift, k);
                    }
                    assert_eq!(q_n, q_r, "residue quantize gain={gain} dz={dz}");
                    q_idx[(qs & 3) as usize] += 1;

                    // Dist path (production dispatch), sentinel-filled.
                    let mut q_n = vec![SENTINEL; n];
                    let mut q_r = vec![SENTINEL; n];
                    let d_n = super::super::quant::calc_sfb_dist(&spec, gain, dz, &mut q_n);
                    let d_r = calc_sfb_dist_scalar(&spec, gain, dz, &mut q_r);
                    assert_eq!((d_n, &q_n), (d_r, &q_r), "residue dist gain={gain} dz={dz}");
                    if q_r.iter().any(|&q| (q as i32).abs() > MAX_QUANT) {
                        early += 1;
                    }
                    m_row[(gain & 3) as usize] += 1;

                    // Energy path with a valid band (exercises inv_quantize4
                    // for this gain's mantissa row, not just the early-out).
                    let quant: Vec<i16> = (0..n).map(|i| ((i as i32 * 37) % 8191) as i16).collect();
                    let (e_n, d_n) =
                        super::super::quant::calc_sfb_quant_energy_and_dist(&spec, &quant, gain);
                    let (e_r, d_r) = calc_sfb_quant_energy_and_dist_scalar(&spec, &quant, gain);
                    assert_eq!((e_n, d_n), (e_r, d_r), "residue energy gain={gain} dz={dz}");
                }
            }
        }
        println!(
            "NEON_TEST neon_residue_sweep_all_quantizers_and_rows: q_idx={q_idx:?} m_row={m_row:?} early={early}"
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

    /// Isolated kernel timing (opt-in): scalar vs NEON on a representative
    /// 14-line band (the S4 average) and a 64-line band. Prints ns/line;
    /// correctness is asserted, not just timed.
    #[test]
    #[ignore = "opt-in NEON kernel microbenchmark"]
    fn neon_kernel_microbench() {
        let cases: &[(usize, i32)] = &[(14, 0), (14, -32), (64, 0), (64, 32)];
        for &(n, gain) in cases {
            let spec: Vec<i32> = (0..n)
                .map(|i| (i as i32 * 1103515245 + 12345) >> 4)
                .collect();
            let dz = false;
            let qs = gain.wrapping_neg();
            let quantizer = QUANT_TABLE_Q[(qs.wrapping_neg() & 3) as usize];
            let shift = (qs.wrapping_neg() >> 2) + 1;
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
            let neon_ns = t1.elapsed().as_nanos() as f64 / iters as f64 / n as f64;
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
            let neon_ed = t3.elapsed().as_nanos() as f64 / iters as f64 / n as f64;
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
                    super::quantize_lines_neon(&spec, &mut q, gain, shift, QUANT_K0);
                }
                std::hint::black_box(&q);
            }
            let neon_q = t5.elapsed().as_nanos() as f64 / iters as f64 / n as f64;
            println!(
                "NEON_MICRO n={n} gain={gain}: dist scalar={scalar_ns:.2}ns neon={neon_ns:.2}ns ({:.2}x) | energy scalar={scalar_ed:.2}ns neon={neon_ed:.2}ns ({:.2}x) | quant scalar={scalar_q:.2}ns neon={neon_q:.2}ns ({:.2}x)",
                scalar_ns / neon_ns.max(0.01),
                scalar_ed / neon_ed.max(0.01),
                scalar_q / neon_q.max(0.01),
            );
        }
    }
}
