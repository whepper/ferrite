//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! Intensity-stereo decision and processing.
//!
//! Port of the C reference:
//! - `FDKaacEnc_prepareIntensityDecision` (`intensity.cpp:258-476`)
//! - `FDKaacEnc_finalizeIntensityDecision` (`intensity.cpp:490-580`)
//! - `FDKaacEnc_IntensityStereoProcessing` (`intensity.cpp:620-817`)
//!
//! The port keeps the C integer domain: all energies, thresholds, log-domain
//! values and MDCT lines are Q31 (`FIXP_DBL`), and the arithmetic mirrors the
//! ARM64 C build through [`super::q31`] (`fMult` floor, `fMultDiv2`, `fDivNorm`,
//! `sqrtFixp`, shift/int conversion). The only upstream approximation remains
//! the existing float-to-Q31 spectrum mapping in the driver.
//!
//! PNS is disabled in the stripped reference, so the C PNS-flag clearing in
//! the processing loop has no observable Rust equivalent here. The C
//! `commonWindow` condition is satisfied by construction: `psyMain` sets it to
//! 1 (`psy_main.cpp:413`) and the Rust encoder has no unequal-window CPE path.

use super::bitcount::CODE_BOOK_IS_IN_PHASE_NO;
use super::q31::{count_leading_zeros, fdiv_norm_e, fmult, fmult_div2, shift_int, sqrt_fixp};

/// Q31 constants from the C reference. The C literals use `f` suffixes and
/// `FL2FXCONST_DBL` rounds the resulting `float`; these are the exact rounded
/// values, not decimal-double approximations.
const CORR_THRESH: i32 = 0x7999_9980; // FL2FXCONST_DBL(0.95f)
const TOTAL_ERROR_THRESH: i32 = 0x051E_B850; // FL2FXCONST_DBL(0.04f)
const LOCAL_ERROR_THRESH: i32 = 0x0147_AE14; // FL2FXCONST_DBL(0.01f)
const DIRECTION_DEVIATION_THRESH: i32 = 0x4000_0000; // 0.5f
const REGION_MIN_LOUDNESS: i32 = 0x0CCC_CCD0; // FL2FXCONST_DBL(0.1f)
const LEFT_RIGHT_RATIO_THRESH: i32 = 0x5999_9980; // FL2FXCONST_DBL(0.7f)
const REAL_SCALE_MAX: i32 = 0x3C00_0000; // 60/128
const REAL_SCALE_MIN: i32 = -0x3C00_0000; // -60/128
const ONE_POINT_FIVE_RECIP: i32 = 0x5555_5580; // FL2FXCONST_DBL(1.0f/1.5f)
const QUARTER: i32 = 0x2000_0000;
const EIGHTH: i32 = 0x1000_0000;
const IS_SCALE_HALF_LSB: i32 = 0x0040_0000; // 0.5f / 256
const THRESHOLD_LD_FLOOR: i32 = -0x4200_0000; // FL2FXCONST_DBL(-0.515625f)

const MAX_GROUPED_SFB: usize = 64;
const MIN_IS_SFBS: i32 = 6;
const LD_DATA_SHIFT: i32 = 6;
const REAL_SCALE_SF: i32 = 1;
const MAX_SFB_PER_GROUP_SF: i32 = 6;
const DFRACT_BITS: i32 = 32;

/// Test-only observation of the I9 residual calculation
/// (`intensity.rs:479-537`). The probe records the actual `lr` accumulator and
/// the *selected* residual energy (`ed` for out-of-phase, `es` for in-phase)
/// at the exact production decision point. Bands that never reach I9 are
/// recorded with a skip reason and no residual value; nothing is fabricated.
#[cfg(test)]
pub(crate) mod is_probe {
    use std::cell::RefCell;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct ResidualRec {
        pub(crate) band: usize,
        pub(crate) start: usize,
        pub(crate) end: usize,
        pub(crate) s_l: i32,
        pub(crate) s_r: i32,
        pub(crate) inv_n: i32,
        pub(crate) mdct_spec_sf: i32,
        pub(crate) lr: Option<i32>,
        pub(crate) ed: Option<i32>,
        pub(crate) es: Option<i32>,
        /// `out_of_phase` | `in_phase` | `not_reached`.
        pub(crate) branch: &'static str,
        pub(crate) selected: bool,
        /// Empty when the selected residual was positive; otherwise
        /// `ed_nonpositive`, `es_nonpositive` or a pre-I9 skip reason.
        pub(crate) skip: &'static str,
    }

    thread_local! {
        static RECS: RefCell<Vec<ResidualRec>> = const { RefCell::new(Vec::new()) };
    }

    pub(crate) fn clear() {
        RECS.with(|v| v.borrow_mut().clear());
    }

    pub(crate) fn take() -> Vec<ResidualRec> {
        RECS.with(|v| std::mem::take(&mut *v.borrow_mut()))
    }

    pub(crate) fn record(rec: ResidualRec) {
        RECS.with(|v| v.borrow_mut().push(rec));
    }

    pub(crate) fn not_reached(band: usize, skip: &'static str) {
        record(ResidualRec {
            band,
            start: 0,
            end: 0,
            s_l: 0,
            s_r: 0,
            inv_n: 0,
            mdct_spec_sf: 0,
            lr: None,
            ed: None,
            es: None,
            branch: "not_reached",
            selected: false,
            skip,
        });
    }
}

/// Result of the stereo intensity decision. `is_book`/`is_scale` are the
/// right-channel vectors; the left channel remains all-zero by C design.
pub(crate) struct IntensityResult {
    pub is_book: Vec<i32>,
    pub is_scale: Vec<i32>,
    pub ms_mask: Vec<i32>,
    pub ms_digest: i32,
}

#[inline]
fn fix_abs(v: i32) -> i32 {
    if v < 0 {
        v.wrapping_neg()
    } else {
        v
    }
}

#[inline]
fn clamp(v: i32, lo: i32, hi: i32) -> i32 {
    v.max(lo).min(hi)
}

/// `GetInvInt` (`fixpoint_math.h:948`): exact rounded `1/n` in Q31.
/// The C table uses `0x7fffffff` for n=1 and `round(2^31/n)` otherwise.
#[inline]
fn inv_int(n: usize) -> i32 {
    match n {
        0 => 0,
        1 => i32::MAX,
        _ => {
            let n = n as u64;
            (((1u64 << 31) + n / 2) / n) as i32
        }
    }
}

/// `calcSfbMaxScale` (`intensity.cpp:190-207`).
fn calc_sfb_max_scale(spec: &[i32], start: usize, end: usize) -> i32 {
    let mut max = 0i32;
    for &v in &spec[start..end] {
        max = max.max(fix_abs(v));
    }
    if max == 0 {
        30
    } else {
        count_leading_zeros(max) - 1
    }
}

/// `fPow2Div2(a)` (`common_fix.h:249`): `(a*a)>>32`.
#[inline]
fn fpow2_div2(v: i32) -> i32 {
    fmult_div2(v, v)
}

/// `fAbs` for a Q31 value. The C reference uses `fAbs` on correlation and
/// shift values; `wrapping_neg` preserves its INT_MIN corner.
#[inline]
fn q31_abs(v: i32) -> i32 {
    fix_abs(v)
}

/// C `fMultDiv2`-based correlation for one SFB.
pub(crate) fn sfb_correlation(spec_l: &[i32], spec_r: &[i32], start: usize, end: usize) -> i32 {
    let width = end.saturating_sub(start);
    if width == 0 {
        return 0;
    }
    let inv_n = inv_int(width >> 1);
    if inv_n <= 0 {
        return 0;
    }

    let s_l = calc_sfb_max_scale(spec_l, start, end);
    let s_r = calc_sfb_max_scale(spec_r, start, end);
    let s = s_l.min(s_r);

    let mut ml = 0i32;
    let mut mr = 0i32;
    let mut prod_lr = 0i32;
    let mut square_l = 0i32;
    let mut square_r = 0i32;

    for j in start..end {
        ml = ml.wrapping_add(fmult_div2(shift_int(spec_l[j], s), inv_n));
        mr = mr.wrapping_add(fmult_div2(shift_int(spec_r[j], s), inv_n));
    }
    ml = fmult_div2(ml, inv_n);
    mr = fmult_div2(mr, inv_n);

    for j in start..end {
        let tmp_l = fmult_div2(shift_int(spec_l[j], s), inv_n).wrapping_sub(ml);
        let tmp_r = fmult_div2(shift_int(spec_r[j], s), inv_n).wrapping_sub(mr);
        prod_lr = prod_lr.wrapping_add(fmult_div2(tmp_l, tmp_r));
        square_l = square_l.wrapping_add(fpow2_div2(tmp_l));
        square_r = square_r.wrapping_add(fpow2_div2(tmp_r));
    }
    prod_lr = prod_lr.wrapping_shl(1);
    square_l = square_l.wrapping_shl(1);
    square_r = square_r.wrapping_shl(1);

    if square_l <= 0 || square_r <= 0 {
        return 0;
    }

    let mut s_l = (count_leading_zeros(square_l) - 1).max(0);
    let mut s_r = (count_leading_zeros(square_r) - 1).max(0);
    let s = ((s_l + s_r) >> 1) << 1;
    s_l = s_l.min(s);
    s_r = s - s_l;

    let mut tmp = fmult(shift_int(square_l, s_l), shift_int(square_r, s_r));
    if tmp <= 0 {
        return 0;
    }
    tmp = sqrt_fixp(tmp);
    if tmp <= 0 {
        return 0;
    }

    let (mut corr, mut corr_sf) = if prod_lr < 0 {
        let (m, e) = fdiv_norm_e(prod_lr.wrapping_neg(), tmp);
        (m.wrapping_neg(), e)
    } else {
        fdiv_norm_e(prod_lr, tmp)
    };
    corr_sf = clamp(
        corr_sf + ((s_l + s_r) >> 1),
        -(DFRACT_BITS - 1),
        DFRACT_BITS - 1,
    );

    if corr_sf < 0 {
        corr >>= -corr_sf;
    } else if q31_abs(corr) > (i32::MAX >> corr_sf) {
        corr = if corr < 0 { -i32::MAX } else { i32::MAX };
    } else {
        corr = corr.wrapping_shl(corr_sf as u32);
    }
    corr
}

/// `prepareIntensityDecision` (`intensity.cpp:258-476`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_decisions(
    e_l: &[i32],
    e_r: &[i32],
    e_ld_l: &[i32],
    e_ld_r: &[i32],
    spec_l: &[i32],
    spec_r: &[i32],
    offsets: &[i32],
    cnt: usize,
    per: usize,
    max: usize,
    hrr_err: &mut [i32],
    is_mask: &mut [i32],
    real_scale: &mut [i32],
    norm_loudness: &mut [i32],
) {
    // First pass: scale, loudness, and the anti-middle-panning guard.
    for grp in (0..cnt).step_by(per) {
        for off in 0..max {
            let b = grp + off;
            if b >= cnt {
                break;
            }
            let is_value = e_ld_l[b].wrapping_sub(e_ld_r[b]);
            real_scale[b] = clamp(is_value, REAL_SCALE_MIN, REAL_SCALE_MAX);

            let s_l = (count_leading_zeros(e_l[b]) - 1).max(0);
            let s_r = (count_leading_zeros(e_r[b]) - 1).max(0);
            let s = (s_l.min(s_r) >> 2) << 2;
            let sum = shift_int(e_l[b], s).wrapping_shr(1) + shift_int(e_r[b], s).wrapping_shr(1);
            norm_loudness[b] = sqrt_fixp(sqrt_fixp(sum)) >> (s >> 2);

            if e_l[b] >= fmult(LEFT_RIGHT_RATIO_THRESH, e_r[b])
                && fmult(LEFT_RIGHT_RATIO_THRESH, e_l[b]) <= e_r[b]
            {
                // Near-centre/dual-mono: prevent post-processing from
                // merging these bands (C: hrrErr = 1/8).
                hrr_err[b] = EIGHTH;
            }
        }
    }

    // Second pass: normalization and Pearson correlation.
    for grp in (0..cnt).step_by(per) {
        let mut overall_loudness = 0i32;
        for off in 0..max {
            let b = grp + off;
            if b >= cnt {
                break;
            }
            overall_loudness =
                overall_loudness.wrapping_add(norm_loudness[b] >> MAX_SFB_PER_GROUP_SF);
        }

        let (inv_overall, mut inv_overall_sf) = if overall_loudness == 0 {
            (0, 0)
        } else {
            let (m, e) = fdiv_norm_e(i32::MAX, overall_loudness);
            (m, e - MAX_SFB_PER_GROUP_SF + 1)
        };
        inv_overall_sf = clamp(inv_overall_sf, -(DFRACT_BITS - 1), DFRACT_BITS - 1);

        for off in 0..max {
            let b = grp + off;
            if b >= cnt {
                break;
            }
            let tmp = fmult_div2(
                shift_int(
                    norm_loudness[b] >> MAX_SFB_PER_GROUP_SF,
                    MAX_SFB_PER_GROUP_SF,
                ),
                inv_overall,
            );
            norm_loudness[b] = shift_int(tmp, inv_overall_sf);

            let start = offsets[b] as usize;
            let end = offsets[b + 1] as usize;
            let corr = sfb_correlation(spec_l, spec_r, start, end);

            if hrr_err[b] == EIGHTH {
                continue;
            }
            hrr_err[b] = fmult_div2(QUARTER.wrapping_sub(corr >> 2), norm_loudness[b]);
            if q31_abs(corr) >= CORR_THRESH {
                is_mask[b] = 1;
            }
        }
    }
}

/// `finalizeIntensityDecision` (`intensity.cpp:490-580`).
pub(crate) fn finalize_decisions(
    hrr_err: &[i32],
    is_mask: &mut [i32],
    real_scale: &[i32],
    norm_loudness: &[i32],
    cnt: usize,
    per: usize,
    max: usize,
) {
    let local_error = LOCAL_ERROR_THRESH >> 3;
    let total_error = TOTAL_ERROR_THRESH >> MAX_SFB_PER_GROUP_SF;
    let region_min = REGION_MIN_LOUDNESS >> MAX_SFB_PER_GROUP_SF;
    let direction_thresh = DIRECTION_DEVIATION_THRESH >> (REAL_SCALE_SF + LD_DATA_SHIFT - 2);

    let mut is_scale_last = 0i32;
    let mut is_start_value_found = false;

    for grp in (0..cnt).step_by(per) {
        let mut start_is_sfb = 0usize;
        let mut in_is_block = false;
        let mut current_is_sfb_count = 0i32;
        let mut overall_hrr_error = 0i32;
        let mut is_region_loudness = 0i32;

        for off in 0..max {
            let b = grp + off;
            if b >= cnt {
                break;
            }

            if is_mask[b] == 1 {
                if current_is_sfb_count == 0 {
                    start_is_sfb = b;
                }
                if !is_start_value_found {
                    is_scale_last = real_scale[b];
                    is_start_value_found = true;
                }
                in_is_block = true;
                current_is_sfb_count += 1;
                overall_hrr_error =
                    overall_hrr_error.wrapping_add(hrr_err[b] >> (MAX_SFB_PER_GROUP_SF - 3));
                is_region_loudness =
                    is_region_loudness.wrapping_add(norm_loudness[b] >> MAX_SFB_PER_GROUP_SF);
            } else if in_is_block {
                overall_hrr_error =
                    overall_hrr_error.wrapping_add(hrr_err[b] >> (MAX_SFB_PER_GROUP_SF - 3));
                is_region_loudness =
                    is_region_loudness.wrapping_add(norm_loudness[b] >> MAX_SFB_PER_GROUP_SF);

                if hrr_err[b] < local_error && overall_hrr_error < total_error {
                    current_is_sfb_count += 1;
                    is_mask[b] = 1;
                } else {
                    in_is_block = false;
                }
            }

            if in_is_block {
                if q31_abs(is_scale_last.wrapping_sub(real_scale[b])) < direction_thresh {
                    is_scale_last = real_scale[b];
                } else {
                    is_mask[b] = 0;
                    in_is_block = false;
                    current_is_sfb_count -= 1;
                }
            }

            if current_is_sfb_count > 0 && (!in_is_block || off + 1 == max) {
                if current_is_sfb_count < MIN_IS_SFBS || is_region_loudness < region_min {
                    for j in start_is_sfb..=b {
                        is_mask[j] = 0;
                    }
                    is_scale_last = 0;
                    is_start_value_found = false;
                    for j in 0..start_is_sfb {
                        if is_mask[j] != 0 {
                            is_scale_last = real_scale[j];
                            is_start_value_found = true;
                        }
                    }
                }
                current_is_sfb_count = 0;
                overall_hrr_error = 0;
                is_region_loudness = 0;
            }
        }
    }
}

/// `FDKaacEnc_IntensityStereoProcessing` (`intensity.cpp:620-817`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn intensity_stereo(
    e_l: &[i32],
    e_r: &mut [i32],
    spec_l: &mut [i32],
    spec_r: &mut [i32],
    threshold_l: &[i32],
    threshold_r: &mut [i32],
    threshold_ld_r: &mut [i32],
    spread_l: &[i32],
    spread_r: &mut [i32],
    e_ld_l: &[i32],
    e_ld_r: &mut [i32],
    offsets: &[i32],
    cnt: usize,
    per: usize,
    max: usize,
    allow_is: bool,
) -> IntensityResult {
    let mut is_book = vec![0i32; cnt];
    let mut is_scale = vec![0i32; cnt];
    let mut ms_mask = vec![0i32; cnt];
    let mut ms_digest = 0i32;

    if !allow_is || cnt == 0 {
        return IntensityResult {
            is_book,
            is_scale,
            ms_mask,
            ms_digest,
        };
    }

    assert!(per > 0 && max <= per && offsets.len() > cnt);
    assert!(cnt <= MAX_GROUPED_SFB, "grouped SFB bound");

    let mut hrr_err = vec![0i32; cnt];
    let mut norm_loudness = vec![0i32; cnt];
    let mut real_scale = vec![0i32; cnt];
    let mut is_mask = vec![0i32; cnt];

    prepare_decisions(
        e_l,
        e_r,
        e_ld_l,
        e_ld_r,
        spec_l,
        spec_r,
        offsets,
        cnt,
        per,
        max,
        &mut hrr_err,
        &mut is_mask,
        &mut real_scale,
        &mut norm_loudness,
    );
    finalize_decisions(
        &hrr_err,
        &mut is_mask,
        &real_scale,
        &norm_loudness,
        cnt,
        per,
        max,
    );

    for grp in (0..cnt).step_by(per) {
        for off in 0..max {
            let b = grp + off;
            if b >= cnt {
                break;
            }
            ms_mask[b] = 0;
            if is_mask[b] == 0 {
                #[cfg(test)]
                is_probe::not_reached(b, "mask_zero");
                continue;
            }

            // C threshold guard (`intensity.cpp:671-673`).
            if e_l[b] < threshold_l[b] && fmult(ONE_POINT_FIVE_RECIP, e_r[b]) > threshold_r[b] {
                #[cfg(test)]
                is_probe::not_reached(b, "i8_guard");
                continue;
            }

            let start = offsets[b] as usize;
            let end = offsets[b + 1] as usize;
            if end <= start {
                #[cfg(test)]
                is_probe::not_reached(b, "geometry");
                continue;
            }

            let mut mdct_spec_sf = 6i32;
            if end - start > (1usize << mdct_spec_sf) {
                mdct_spec_sf += 1;
            }
            let inv_n = inv_int((end - start) >> 1);
            if inv_n <= 0 {
                #[cfg(test)]
                is_probe::not_reached(b, "inv_n");
                continue;
            }

            let s_l = calc_sfb_max_scale(spec_l, start, end);
            let s_r = calc_sfb_max_scale(spec_r, start, end);

            let mut lr = 0i32;
            for j in start..end {
                lr = lr.wrapping_add(fmult_div2(
                    fmult_div2(shift_int(spec_l[j], s_l), shift_int(spec_r[j], s_r)),
                    inv_n,
                ));
            }
            lr = lr.wrapping_shl(1);

            if lr < 0 {
                // Out-of-phase intensity stereo.
                let s0 = s_l.min(s_r);
                let mut ed = 0i32;
                for j in start..end {
                    let d =
                        (shift_int(spec_l[j], s0) >> 1).wrapping_sub(shift_int(spec_r[j], s0) >> 1);
                    ed = ed.wrapping_add(fmult_div2(d, d) >> (mdct_spec_sf - 1));
                }
                #[cfg(test)]
                is_probe::record(is_probe::ResidualRec {
                    band: b,
                    start,
                    end,
                    s_l,
                    s_r,
                    inv_n,
                    mdct_spec_sf,
                    lr: Some(lr),
                    ed: Some(ed),
                    es: None,
                    branch: "out_of_phase",
                    selected: ed > 0,
                    skip: if ed > 0 { "" } else { "ed_nonpositive" },
                });
                if ed <= 0 {
                    continue;
                }
                ms_mask[b] = 1;
                let (mut tmp, s1) = fdiv_norm_e(e_l[b], ed);
                let mut s2 = s1 + 2 * s0 - 2 - mdct_spec_sf;
                if (s2 & 1) != 0 {
                    tmp >>= 1;
                    s2 += 1;
                }
                s2 = (s2 >> 1) + 1;
                s2 = clamp(s2, -(DFRACT_BITS - 1), DFRACT_BITS - 1);
                let scale = sqrt_fixp(tmp);
                if s2 < 0 {
                    let sh = -s2;
                    for j in start..end {
                        spec_l[j] = fmult_div2(spec_l[j], scale)
                            .wrapping_sub(fmult_div2(spec_r[j], scale))
                            >> sh;
                        spec_r[j] = 0;
                    }
                } else {
                    for j in start..end {
                        spec_l[j] = (fmult_div2(spec_l[j], scale)
                            .wrapping_sub(fmult_div2(spec_r[j], scale)))
                        .wrapping_shl(s2 as u32);
                        spec_r[j] = 0;
                    }
                }
            } else {
                // In-phase intensity stereo.
                let s0 = s_l.min(s_r);
                let mut es = 0i32;
                for j in start..end {
                    let s =
                        (shift_int(spec_l[j], s0) >> 1).wrapping_add(shift_int(spec_r[j], s0) >> 1);
                    es = es.wrapping_add(fmult_div2(s, s) >> (mdct_spec_sf - 1));
                }
                #[cfg(test)]
                is_probe::record(is_probe::ResidualRec {
                    band: b,
                    start,
                    end,
                    s_l,
                    s_r,
                    inv_n,
                    mdct_spec_sf,
                    lr: Some(lr),
                    ed: None,
                    es: Some(es),
                    branch: "in_phase",
                    selected: es > 0,
                    skip: if es > 0 { "" } else { "es_nonpositive" },
                });
                if es <= 0 {
                    continue;
                }
                ms_mask[b] = 0;
                let (mut tmp, s1) = fdiv_norm_e(e_l[b], es);
                let mut s2 = s1 + 2 * s0 - 2 - mdct_spec_sf;
                if (s2 & 1) != 0 {
                    tmp >>= 1;
                    s2 += 1;
                }
                s2 = (s2 >> 1) + 1;
                s2 = clamp(s2, -(DFRACT_BITS - 1), DFRACT_BITS - 1);
                let scale = sqrt_fixp(tmp);
                if s2 < 0 {
                    let sh = -s2;
                    for j in start..end {
                        spec_l[j] = fmult_div2(spec_l[j], scale)
                            .wrapping_add(fmult_div2(spec_r[j], scale))
                            >> sh;
                        spec_r[j] = 0;
                    }
                } else {
                    for j in start..end {
                        spec_l[j] = (fmult_div2(spec_l[j], scale)
                            .wrapping_add(fmult_div2(spec_r[j], scale)))
                        .wrapping_shl(s2 as u32);
                        spec_r[j] = 0;
                    }
                }
            }

            is_book[b] = CODE_BOOK_IS_IN_PHASE_NO;
            let scaled = if real_scale[b] < 0 {
                ((real_scale[b] >> 1).wrapping_sub(IS_SCALE_HALF_LSB)
                    >> (DFRACT_BITS - 1 - REAL_SCALE_SF - LD_DATA_SHIFT - 1))
                    .wrapping_add(1)
            } else {
                (real_scale[b] >> 1).wrapping_add(IS_SCALE_HALF_LSB)
                    >> (DFRACT_BITS - 1 - REAL_SCALE_SF - LD_DATA_SHIFT - 1)
            };
            is_scale[b] = scaled;

            e_r[b] = 0;
            e_ld_r[b] = i32::MIN;
            threshold_r[b] = 0;
            threshold_ld_r[b] = THRESHOLD_LD_FLOOR;
            spread_r[b] = 0;
            ms_digest = 1; // MS_SOME
        }
    }

    // C leaves the unused scale/spread inputs untouched; the caller only
    // consumes the selected right-channel entries and clears the rest via
    // the normal zero-book path.
    let _ = (spread_l, e_ld_l);
    IntensityResult {
        is_book,
        is_scale,
        ms_mask,
        ms_digest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv_int_matches_reference_table_edges() {
        assert_eq!(inv_int(0), 0);
        assert_eq!(inv_int(1), 0x7FFF_FFFF);
        assert_eq!(inv_int(2), 0x4000_0000);
        assert_eq!(inv_int(3), 0x2AAA_AAAB);
        assert_eq!(inv_int(48), 0x02AA_AAAB);
    }

    #[test]
    fn max_scale_matches_clz_rule() {
        assert_eq!(calc_sfb_max_scale(&[0, 0], 0, 2), 30);
        assert_eq!(calc_sfb_max_scale(&[0x4000_0000], 0, 1), 0);
        assert_eq!(calc_sfb_max_scale(&[0x7FFF_FFFF], 0, 1), 0);
        assert_eq!(calc_sfb_max_scale(&[0x3FFF_FFFF], 0, 1), 1);
    }
}
