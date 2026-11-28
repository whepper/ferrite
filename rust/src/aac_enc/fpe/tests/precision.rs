//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! FPE-3.3 A-4 experiment (test-only): f32-vs-f64 energy-sum precision.
//!
//! **Authorization:** test-only experiment implementation, execution, and
//! reporting. This module changes no production behaviour: the production
//! kernels (`super::super::energy`, `super::super::output`) are called
//! unchanged as the f64 reference arm, and every f32 variant is a local
//! transliteration confined to this file. Q31 helpers are called read-only
//! as the correctness reference. No thresholds are invented here; all
//! acceptance-relevant comparisons reuse the existing envelopes
//! (`super::envelope`, `docs/fpe/05` §9) and report utilization only.
//!
//! **Standing decision (ADR-0002, unchanged):** f64 throughout. This
//! experiment measures alternatives; it authorizes no production change.
//!
//! ## Arms (per band, identical inputs/offsets/order/scaling)
//!
//! - **F64**: production `rescale_band_energy_into` /
//!   `tns_recompute_energy_into` + `fraction_to_stored` (unchanged calls).
//! - **F32-realistic**: convert each f64 line to f32, square in f32,
//!   accumulate in f32; widen to f64 for the exact power-of-two scaling,
//!   the f64 `log2`/Ld path, and the `fraction_to_stored` boundary.
//! - **F32-accumulator diagnostic**: square in f64, add in f64, round the
//!   running accumulator to f32 after every addition (`as f32 as f64`);
//!   widen at the end. Isolates accumulator rounding from input and
//!   product narrowing. Inputs are never silently narrowed to f32 first.
//! - A pure-f32-through-boundary negative control exists only in the unit
//!   section (analytically non-viable: f32 ULP at 2^31 is 256 LSB), clearly
//!   labelled, excluded from corpus acceptance.
//!
//! No fast-math, no FMA/`mul_add`, no SIMD, fixed loop order mirroring the
//! production kernels. `log2`/Ld/guards/sentinels and the boundary are f64
//! in every arm (A-4 is the accumulation question; A-5 owns `log2`).

use super::envelope::{self, BandEnvelope};
use super::*;
use crate::aac_enc::energy as q31_energy;
use crate::aac_enc::fpe::energy as fpe_energy;
use crate::aac_enc::fpe::output as fpe_output;
use crate::aac_enc::psy::{
    self, advance_thresholds, tns_encode_channel, PsyTables, MDCT_SCALE_LONG, MDCT_SCALE_SHORT,
};
use crate::aac_enc::q31::shift_int;
use crate::aac_enc::thresholds::{init_threshold, PreEchoState};
use crate::aac_enc::tns::{self, TnsChannel};

// ---------------------------------------------------------------------------
// f32 variant kernels (test-only transliterations; production untouched)
// ---------------------------------------------------------------------------

/// F32-realistic rescale band energy for one band: f32 line conversion,
/// f32 square, f32 accumulation; f64 exact scaling afterwards.
fn rescale_f32r_band(spec: &[f64], m_pre: i32, f: i32) -> f64 {
    let mut acc = 0f32;
    for &x in spec {
        let xf = x as f32;
        acc += xf * xf;
    }
    (acc as f64) * envelope::pow2(-2 * (m_pre - f))
}

/// F32-accumulator diagnostic for one band: f64 square, f64 add, round the
/// running total to f32 after every addition. Inputs/products stay f64.
fn rescale_f32a_band(spec: &[f64], m_pre: i32, f: i32) -> f64 {
    let mut acc = 0f64;
    for &x in spec {
        acc = (acc + x * x) as f32 as f64;
    }
    acc * envelope::pow2(-2 * (m_pre - f))
}

/// Shared f64 rescale Ld + boundary, mirroring the production kernel's
/// §6.8 rescale convention (`e_raw_f` image, scale0 correction skipped on
/// the sentinel path, unconditional `finalShift` lift).
fn rescale_ld_stored(e_f: f64, scale0: i32, f: i32) -> (i32, f64) {
    let e_raw_f = e_f * envelope::pow2(2 * (scale0 - f));
    let mut v = if e_raw_f <= 0.0 {
        -1.0
    } else {
        e_raw_f.log2() / 64.0 - scale0 as f64 * 2.0 / 64.0
    };
    v += f as f64 * 2.0 / 64.0;
    (fpe_output::fraction_to_stored(e_f), v)
}

/// F1: mirror production's sync-config selection (`tconf_r`,
/// psy.rs:853-861). The right channel's window type selects the table;
/// D-022 pair agreement makes this coincide with the left channel's on
/// compliant inputs, but the selection must still be the right channel's.
fn sync_config<'a>(tables: &'a PsyTables, short_r: bool) -> &'a crate::aac_enc::tns::TnsConfig {
    if short_r {
        &tables.tns_short
    } else {
        &tables.tns_long
    }
}

/// Shared f64 TNS-long Ld + boundary, mirroring the production kernel's
/// guarded convention with identical thresholds and the unconditional
/// `−shiftBits·2/64` correction.
fn tns_long_ld_stored(e_real: f64, m: i32, leading: i32, shift_bits: i32) -> (i32, f64) {
    let scale_diff = leading as f64 * 2.0 / 64.0;
    let e_raw_f = e_real * envelope::pow2(-2 * m + 2 * leading);
    let base = if e_raw_f <= 0.0 {
        -1.0
    } else {
        e_raw_f.log2() / 64.0
    };
    let mut v = if base >= (-0.5 + scale_diff / 2.0) {
        base - scale_diff
    } else {
        -1.0
    };
    v -= shift_bits as f64 * 2.0 / 64.0;
    let e_shifted = e_real * envelope::pow2(-2 * (m + shift_bits));
    (fpe_output::fraction_to_stored(e_shifted), v)
}

// ---------------------------------------------------------------------------
// Per-band/per-kernel statistics (reported, never thresholded here)
// ---------------------------------------------------------------------------

#[derive(Default, Debug)]
struct ArmStats {
    bands: u64,
    max_abs_stored_delta: i64,
    max_loc: String,
    /// Stored-delta histogram buckets: [0, 1, 2-4, 5-16, 17-64, 65+].
    buckets: [u64; 6],
    max_rel_frac_delta: f64,
    max_abs_ld_delta: f64,
    sum_abs_stored_delta: f64,
    /// Stored delta that exceeds the approved per-band envelope (report only).
    over_envelope: u64,
    max_utilization: f64,
    guard_flips: u64,
    sentinel_flips: u64,
}

#[derive(Debug)]
struct KernelStats {
    bands: u64,
    supported: u64,
    unsupported: u64,
    unsupported_reasons: std::collections::BTreeMap<&'static str, u64>,
    class_a: u64,
    class_b: u64,
    /// Class-A/B bands where the f32 arm stores a different value than f64.
    class_flip_r: u64,
    class_flip_a: u64,
    non_finite: u64,
    saturated: u64,
    r: ArmStats,
    a: ArmStats,
    /// Minimum guard-proximity distance observed (long guard threshold or
    /// rescale zero-crossing magnitude); +inf when N/A.
    min_guard_distance: f64,
}

impl KernelStats {
    fn new() -> Self {
        Self {
            bands: 0,
            supported: 0,
            unsupported: 0,
            unsupported_reasons: std::collections::BTreeMap::new(),
            class_a: 0,
            class_b: 0,
            class_flip_r: 0,
            class_flip_a: 0,
            non_finite: 0,
            saturated: 0,
            r: ArmStats::default(),
            a: ArmStats::default(),
            min_guard_distance: f64::INFINITY,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record_arm(
    s: &mut ArmStats,
    stored_f64: i32,
    stored_fx: i32,
    e_f64: f64,
    e_fx: f64,
    ld_f64: f64,
    ld_fx: f64,
    ld_f64_sentinel: bool,
    ld_fx_sentinel: bool,
    ld_f64_guarded_out: bool,
    ld_fx_guarded_out: bool,
    tol: f64,
    flip_log: &mut Vec<String>,
    flip_tag: &str,
) {
    s.bands += 1;
    let d_stored = (stored_fx as i64 - stored_f64 as i64).abs();
    if d_stored > s.max_abs_stored_delta {
        s.max_abs_stored_delta = d_stored;
        s.max_loc = flip_tag.to_string();
    }
    let bucket = match d_stored {
        0 => 0,
        1 => 1,
        2..=4 => 2,
        5..=16 => 3,
        17..=64 => 4,
        _ => 5,
    };
    s.buckets[bucket] += 1;
    s.sum_abs_stored_delta += d_stored as f64;
    if e_f64 != 0.0 {
        let rel = ((e_fx - e_f64) / e_f64).abs();
        if rel.is_finite() {
            s.max_rel_frac_delta = s.max_rel_frac_delta.max(rel);
        }
    } else if e_fx != 0.0 {
        s.max_rel_frac_delta = f64::INFINITY;
    }
    let d_ld = (ld_fx - ld_f64).abs();
    if d_ld.is_finite() {
        s.max_abs_ld_delta = s.max_abs_ld_delta.max(d_ld);
    }
    // Over-envelope, guard-flip and sentinel-flip identities are logged for
    // the report. Ordinary nonzero stored deltas are aggregated in the
    // maxima/means above and are NOT logged individually (they would bury
    // the discontinuous events this log exists to surface).
    if tol.is_finite() && (d_stored as f64) > tol {
        s.over_envelope += 1;
        if flip_log.len() < 300 {
            flip_log.push(format!("OVER-ENV {flip_tag} delta={d_stored} tol={tol:.2}"));
        }
    }
    if tol > 0.0 && tol.is_finite() {
        s.max_utilization = s.max_utilization.max(d_stored as f64 / tol);
    }
    if ld_f64_guarded_out != ld_fx_guarded_out {
        s.guard_flips += 1;
        if flip_log.len() < 300 {
            flip_log.push(format!("GUARD-FLIP {flip_tag}"));
        }
    }
    if ld_f64_sentinel != ld_fx_sentinel {
        s.sentinel_flips += 1;
        if flip_log.len() < 300 {
            flip_log.push(format!("SENTINEL-FLIP {flip_tag}"));
        }
    }
}

/// Classify Q31↔F64 low-energy divergence by stored state (`05` §9.4):
/// Class A = mapped lines all zero; Class B = mapped nonzero, Q31 stores 0.
fn classify(x_q31_band: &[i32], e_q31: i32, e_f64: i32) -> Option<&'static str> {
    if e_q31 == 0 && e_f64 != 0 {
        if x_q31_band.iter().all(|&v| v == 0) {
            Some("A")
        } else {
            Some("B")
        }
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Driver: front-end + mirrored rescale/TNS lanes, per-arm kernel comparison
// ---------------------------------------------------------------------------

struct ChannelDrive {
    switch: BlockSwitch,
    transform: ChannelTransform,
    tns_state: TnsChannel,
    tns_info: TnsChannel,
    /// Per-arm pre-echo history for the downstream propagation probe, carried
    /// across frames exactly like production state.
    pre_q31: PreEchoState,
    pre_f64: PreEchoState,
    pre_f32r: PreEchoState,
    pre_f32a: PreEchoState,
}

struct CorpusStats {
    frames: u64,
    long_frames: u64,
    short_frames: u64,
    tns_active_frames: u64,
    short_tns_active_frames: u64,
    rescale: KernelStats,
    tns_long_active: KernelStats,
    tns_long_inactive: KernelStats,
    tns_short_active: KernelStats,
    tns_short_inactive: KernelStats,
    /// Downstream propagation probe maxima (arm vs Q31 arm, arm vs f64 arm).
    max_thr_delta_r_q31: i64,
    max_thr_delta_a_q31: i64,
    max_thr_delta_r_64: i64,
    max_thr_delta_a_64: i64,
    max_spread_delta_r_64: i64,
    max_spread_delta_a_64: i64,
    max_preecho_prev_delta_r_64: i64,
    max_preecho_prev_delta_a_64: i64,
    /// Flip/outlier identities (bounded).
    flip_log: Vec<String>,
    /// F1: stereo sync configuration accounting and old-vs-corrected
    /// outcome divergences.
    sync_long_pairs: u64,
    sync_short_pairs: u64,
    sync_divergences: u64,
    sync_divergence_log: Vec<String>,
    /// F7: observed nonzero-shift coverage (active-long rows only).
    max_shiftbits: i32,
    nonzero_shift_windows: u64,
}

impl CorpusStats {
    fn new() -> Self {
        Self {
            frames: 0,
            long_frames: 0,
            short_frames: 0,
            tns_active_frames: 0,
            short_tns_active_frames: 0,
            rescale: KernelStats::new(),
            tns_long_active: KernelStats::new(),
            tns_long_inactive: KernelStats::new(),
            tns_short_active: KernelStats::new(),
            tns_short_inactive: KernelStats::new(),
            max_thr_delta_r_q31: 0,
            max_thr_delta_a_q31: 0,
            max_thr_delta_r_64: 0,
            max_thr_delta_a_64: 0,
            max_spread_delta_r_64: 0,
            max_spread_delta_a_64: 0,
            max_preecho_prev_delta_r_64: 0,
            max_preecho_prev_delta_a_64: 0,
            flip_log: Vec::new(),
            sync_long_pairs: 0,
            sync_short_pairs: 0,
            sync_divergences: 0,
            sync_divergence_log: Vec::new(),
            max_shiftbits: 0,
            nonzero_shift_windows: 0,
        }
    }
}

/// Drive one fixture through front-end + mirrored lanes, comparing the three
/// arms on identical inputs. Mirrors `psy.rs:548-963` stage order for the
/// rescale lane and the TNS block (detect per channel, pair sync, element
/// active-OR, per-channel `tns_encode_channel`); stereo joint decisions
/// (`min_spec_shift`, `max_nrg`, `finalShift`) are mirrored element-globally.
fn drive_fixture(name: &str, stats: &mut CorpusStats) {
    let fixture = testdata::fixture_samples(name);
    let channels = fixture.len();
    let sr = testdata::fixture_sample_rate(name);
    assert!(!fixture.is_empty());

    // Operating point identical to DiffStream::from_pcm (VBR-5 bandwidth).
    let (ch_br, tns_br) = if channels == 1 {
        (112000, 112000)
    } else {
        (96000, 192000)
    };
    let ((lcfg, _), (scfg, _)) = testdata::load_cfg(sr, ch_br);
    let tables = PsyTables::for_rate(sr, ch_br, tns_br, 19293, lcfg.min_snr_ld, scfg.min_snr_ld)
        .expect("tables build");

    let mut ch: Vec<ChannelDrive> = (0..channels)
        .map(|_| ChannelDrive {
            switch: BlockSwitch::new(),
            transform: ChannelTransform::new(),
            tns_state: TnsChannel::new(),
            tns_info: TnsChannel::new(),
            pre_q31: PreEchoState::new(tables.long_cfg.pcm_quant_q31()),
            pre_f64: PreEchoState::new(tables.long_cfg.pcm_quant_q31()),
            pre_f32r: PreEchoState::new(tables.long_cfg.pcm_quant_q31()),
            pre_f32a: PreEchoState::new(tables.long_cfg.pcm_quant_q31()),
        })
        .collect();

    let n_frames = fixture[0].len().div_ceil(1024) + 2;
    for frame in 0..n_frames {
        // --- Front-end (identical construction to DiffStream::step). ---
        let fresh: Vec<[i16; 1024]> = (0..channels)
            .map(|c| {
                let mut block = [0i16; 1024];
                for (i, v) in block.iter_mut().enumerate() {
                    let t = frame as usize * 1024 + i;
                    *v = if t < fixture[c].len() {
                        fixture[c][t]
                    } else {
                        0
                    };
                }
                block
            })
            .collect();
        for (d, fr) in ch.iter_mut().zip(fresh.iter()) {
            d.switch.process(fr, false);
        }
        let stereo = channels == 2;
        if stereo {
            let (l, r) = ch.split_at_mut(1);
            BlockSwitch::synchronize_pair(&mut l[0].switch, &mut r[0].switch);
        } else {
            ch[0].switch.synchronize_mono();
        }
        let mut specs: Vec<[f64; 1024]> = Vec::with_capacity(channels);
        let mut seqs = Vec::with_capacity(channels);
        for d in ch.iter_mut() {
            let idx = specs.len();
            let out = d
                .transform
                .process(&fresh[idx], d.switch.sequence(), d.switch.shape())
                .expect("mdct");
            let mut spec = [0.0f64; 1024];
            for (t, s) in spec.iter_mut().zip(out.iter()) {
                *t = *s as f64;
            }
            specs.push(spec);
            seqs.push(d.switch.sequence());
        }
        if stereo {
            assert_eq!(seqs[0], seqs[1], "{name} f{frame}: D-022 pair sync");
        }

        // --- Per-channel config selection (psy.rs:548-602). ---
        let short: Vec<bool> = seqs.iter().map(|&s| s == WindowSequence::Short).collect();
        let short_elem = short[0];
        let n_win: Vec<usize> = short.iter().map(|&s| if s { 8 } else { 1 }).collect();
        let wlen: Vec<usize> = short.iter().map(|&s| if s { 128 } else { 1024 }).collect();

        let confs: Vec<&crate::aac_enc::sfb::SfbConfig> = short
            .iter()
            .map(|&s| {
                if s {
                    &tables.short_cfg
                } else {
                    &tables.long_cfg
                }
            })
            .collect();
        let active: Vec<usize> = confs.iter().map(|c| c.sfb_active()).collect();
        let lowpass: Vec<usize> = confs.iter().map(|c| c.lowpass_line()).collect();
        let mut m: Vec<i32> = short
            .iter()
            .map(|&s| if s { MDCT_SCALE_SHORT } else { MDCT_SCALE_LONG })
            .collect();

        // --- Lowpass + zero check. ---
        let mut zero = vec![false; channels];
        for c in 0..channels {
            zero[c] = psy::lowpass_and_zero_check(&mut specs[c], n_win[c], wlen[c], lowpass[c]);
        }

        // --- Integer lane: map + maxScale + joint minimum. ---
        let mut q: Vec<Vec<i32>> = vec![Vec::new(); channels];
        let mut maxsc: Vec<Vec<Vec<i32>>> = vec![Vec::new(); channels];
        for c in 0..channels {
            q31_energy::map_spectrum_to_q31_into(&specs[c], m[c], &mut q[c]);
            maxsc[c].resize(n_win[c], Vec::new());
            for w in 0..n_win[c] {
                q31_energy::max_scale_spec_into(
                    &q[c][w * wlen[c]..(w + 1) * wlen[c]],
                    confs[c].offsets(),
                    active[c],
                    &mut maxsc[c][w],
                );
            }
        }
        let mut min_spec_shift = i32::MAX;
        for c in 0..channels {
            for w in 0..n_win[c] {
                for &v in maxsc[c][w].iter() {
                    min_spec_shift = min_spec_shift.min(v);
                }
            }
        }

        // --- Raw energies + joint maxNrg. ---
        let mut e_raw: Vec<Vec<Vec<i32>>> = vec![Vec::new(); channels];
        for c in 0..channels {
            e_raw[c].resize(n_win[c], Vec::new());
            for w in 0..n_win[c] {
                e_raw[c][w].clear();
                e_raw[c][w].resize(active[c], 0);
                q31_energy::check_band_energy_into(
                    &q[c][w * wlen[c]..(w + 1) * wlen[c]],
                    &maxsc[c][w],
                    confs[c].offsets(),
                    active[c],
                    &mut e_raw[c][w],
                );
            }
        }
        let mut max_nrg = 0i32;
        for c in 0..channels {
            for w in 0..n_win[c] {
                let v = q31_energy::max_nrg_for_shift(&e_raw[c][w], &maxsc[c][w], min_spec_shift);
                max_nrg = max_nrg.max(v);
            }
        }

        stats.frames += 1;
        if short_elem {
            stats.short_frames += 1;
        } else {
            stats.long_frames += 1;
        }

        let zero_elem = zero.iter().all(|&z| z);
        if zero_elem {
            // F4: production still runs advance_thresholds on the all-zero
            // path (psy.rs:974-985, unconditional) with blanked (zero)
            // thresholds and energies at m = 0. That call advances
            // PreEchoState (memorize zeros; STOP/START resets apply), so the
            // probe mirrors it per arm to preserve state progression: stale
            // history here would mismatch post-silence frames against
            // production threading. All arms receive identical zeros, so
            // inter-arm deltas on these frames are 0 by construction.
            for c in 0..channels {
                let thr0: Vec<Vec<i32>> = (0..n_win[c]).map(|_| vec![0i32; active[c]]).collect();
                let en0: Vec<Vec<i32>> = (0..n_win[c]).map(|_| vec![0i32; active[c]]).collect();
                let sp0: Vec<Vec<i32>> = (0..n_win[c]).map(|_| vec![0i32; active[c]]).collect();
                let d = &mut ch[c];
                for pre in [
                    &mut d.pre_q31,
                    &mut d.pre_f64,
                    &mut d.pre_f32r,
                    &mut d.pre_f32a,
                ] {
                    let mut thr = thr0.clone();
                    let mut sp = sp0.clone();
                    advance_thresholds(
                        pre, &mut thr, &mut sp, &en0, confs[c], active[c], 0, short[c], seqs[c],
                        false,
                    );
                }
            }
            // Silence coverage is trivially exact (all arms produce zeros);
            // no kernel rows to measure.
            continue;
        }

        let (_, f) = q31_energy::rescale_shifts(max_nrg, min_spec_shift, short_elem, m[0]);
        // Pre-shift representation state: the production FPE kernels consume
        // pre-rescale `m`, `finalShift`, and pre-shift `maxScale`.
        let m_pre = m.clone();
        let maxsc_pre: Vec<Vec<Vec<i32>>> = maxsc.clone();

        // Q31 reference stored energies + Ld (psy.rs:766-784).
        let mut e_q: Vec<Vec<Vec<i32>>> = vec![Vec::new(); channels];
        for c in 0..channels {
            e_q[c].resize(n_win[c], Vec::new());
            for w in 0..n_win[c] {
                e_q[c].push(Vec::new());
                for b in 0..active[c] {
                    let mut scale = (maxsc[c][w][b] - 4).max(0);
                    scale = ((scale - f) << 1).min(31);
                    e_q[c][w].push(shift_int(e_raw[c][w][b], -scale));
                }
            }
        }

        // Retained per-band arm stored rescale values for the downstream
        // propagation probe (post-overflow shift applied there).
        // arm_store[c][w][b] = (f64, f32r, f32a) stored energies.
        let mut arm_store: Vec<Vec<Vec<(i32, i32, i32)>>> = vec![Vec::new(); channels];

        // --- Rescale experiment: F64 production vs f32 variants. ---
        for c in 0..channels {
            arm_store[c].resize(n_win[c], Vec::new());
            for w in 0..n_win[c] {
                let win = &specs[c][w * wlen[c]..(w + 1) * wlen[c]];
                let mut ef64 = Vec::new();
                let mut ld64 = Vec::new();
                fpe_energy::rescale_band_energy_into(
                    win,
                    confs[c].offsets(),
                    active[c],
                    m_pre[c],
                    f,
                    &maxsc_pre[c][w],
                    &mut ef64,
                    &mut ld64,
                );
                for b in 0..active[c] {
                    let band =
                        &win[confs[c].offsets()[b] as usize..confs[c].offsets()[b + 1] as usize];
                    let e_f64 = ef64[b];
                    let e_f32r = rescale_f32r_band(band, m_pre[c], f);
                    let e_f32a = rescale_f32a_band(band, m_pre[c], f);
                    let scale0 = (maxsc_pre[c][w][b] - 4).max(0);
                    let (st64, ld64v) = rescale_ld_stored(e_f64, scale0, f);
                    let (str_, ldr) = rescale_ld_stored(e_f32r, scale0, f);
                    let (sta, lda) = rescale_ld_stored(e_f32a, scale0, f);
                    // F3: no self-check of fraction_to_stored is asserted
                    // here. st64 IS fraction_to_stored(e_f64) by
                    // construction; re-asserting it against itself would
                    // prove nothing. Boundary correctness is covered by the
                    // output.rs unit tests (round-to-nearest, saturation,
                    // NaN rejection). The independent checks on this path
                    // are the Ld replication assert below (local formula vs
                    // the production kernel's own Ld output) and the
                    // Q31↔F64 fidelity gate further below (separately
                    // computed Q31 reference under the approved envelope).
                    assert!(
                        (ld64v - ld64[b]).abs() == 0.0,
                        "A-4 harness Ld replication drift"
                    );
                    arm_store[c][w].push((st64, str_, sta));
                    // Independent envelope from the mapped spectrum.
                    let (x_q31, sat) = envelope::map_q31(win, m_pre[c]);
                    let env: BandEnvelope = envelope::band_envelope(
                        &x_q31,
                        confs[c].offsets(),
                        b,
                        f,
                        maxsc_pre[c][w][b],
                        sat,
                    );
                    let tol = env
                        .tolerance(e_q[c][w][b])
                        .min(envelope::MAX_FINITE_ENERGY_TOL);
                    let tag = format!("{name} f{frame} ch{c} w{w} b{b} rescale");
                    let st = &mut stats.rescale;
                    st.bands += 1;
                    if !e_f64.is_finite() || !e_f32r.is_finite() || !e_f32a.is_finite() {
                        st.non_finite += 1;
                        continue;
                    }
                    // F6: saturation is exact here. fraction_to_stored
                    // saturates iff round(e_f·2^31) exceeds i32::MAX, i.e.
                    // e_f·2^31 > MAX−0.5 (2147483646.5 is exactly
                    // representable; round-half-away still lands on MAX, a
                    // saturating clamp to the same value). The half-LSB
                    // sliver [MAX−1, MAX−0.5] rounds exactly to MAX and is
                    // not saturation.
                    if e_f64 * envelope::pow2(31) > i32::MAX as f64 - 0.5 {
                        st.saturated += 1;
                    }
                    match env.unsupported_reason(st64, e_q[c][w][b]) {
                        Some(reason) => {
                            st.unsupported += 1;
                            *st.unsupported_reasons.entry(reason).or_insert(0) += 1;
                        }
                        None => st.supported += 1,
                    }
                    // Q31↔F64 fidelity gate: the mirrored lane must reproduce
                    // the audited production relationship (delta in envelope).
                    let d_q64 = (st64 as i64 - e_q[c][w][b] as i64).abs();
                    assert!(
                        env.supported(st64, e_q[c][w][b]) && (d_q64 as f64) <= tol,
                        "{tag}: Q31↔F64 fidelity break (driver lane drift): \
                         delta={d_q64} tol={tol:.2}"
                    );
                    if let Some(cls) = classify(
                        &x_q31[confs[c].offsets()[b] as usize..confs[c].offsets()[b + 1] as usize],
                        e_q[c][w][b],
                        st64,
                    ) {
                        match cls {
                            "A" => st.class_a += 1,
                            _ => st.class_b += 1,
                        }
                        if str_ != st64 {
                            st.class_flip_r += 1;
                        }
                        if sta != st64 {
                            st.class_flip_a += 1;
                        }
                    }
                    // Rescale has no threshold guard (correction applies
                    // unless sentinel); proximity measured as |E_raw_f|.
                    let e_raw_f = e_f64 * envelope::pow2(2 * (scale0 - f));
                    if e_raw_f > 0.0 {
                        st.min_guard_distance = st.min_guard_distance.min(e_raw_f.abs());
                    }
                    let sent64 = e_f64 == 0.0;
                    let sentr = e_f32r == 0.0;
                    let senta = e_f32a == 0.0;
                    let (r, a, log) = (&mut st.r, &mut st.a, &mut stats.flip_log);
                    record_arm(
                        r, st64, str_, e_f64, e_f32r, ld64[b], ldr, sent64, sentr, false, false,
                        tol, log, &tag,
                    );
                    record_arm(
                        a, st64, sta, e_f64, e_f32a, ld64[b], lda, sent64, senta, false, false,
                        tol, log, &tag,
                    );
                }
            }
        }

        // --- Apply representation shift (psy.rs:788-809). ---
        if f != 0 {
            for c in 0..channels {
                for v in q[c].iter_mut() {
                    *v = shift_int(*v, f);
                }
                for w in 0..n_win[c] {
                    for ms in maxsc[c][w].iter_mut() {
                        *ms -= f;
                    }
                }
                m[c] -= f;
            }
        }

        // --- TNS block mirror (psy.rs:813-933). ---
        let tns_on = {
            let tcs: Vec<&crate::aac_enc::tns::TnsConfig> = short
                .iter()
                .map(|&s| {
                    if s {
                        &tables.tns_short
                    } else {
                        &tables.tns_long
                    }
                })
                .collect();
            tcs.iter().any(|t| t.active)
        };
        let mut qwin: Vec<i32> = Vec::new();
        let mut tns_pre: Vec<i32> = Vec::new();
        let mut e_tns: Vec<Vec<Vec<i32>>> = vec![Vec::new(); channels];
        let mut eld_tns: Vec<Vec<Vec<f64>>> = vec![Vec::new(); channels];
        let mut thr_tns: Vec<Vec<Vec<i32>>> = vec![Vec::new(); channels];
        for c in 0..channels {
            e_tns[c].resize(n_win[c], Vec::new());
            eld_tns[c].resize(n_win[c], Vec::new());
            thr_tns[c].resize(n_win[c], Vec::new());
            for w in 0..n_win[c] {
                e_tns[c][w].resize(active[c], 0);
                eld_tns[c][w].resize(active[c], -1.0);
                thr_tns[c][w].resize(active[c], 0);
                for b in 0..active[c] {
                    thr_tns[c][w][b] = init_threshold(e_q[c][w][b]);
                }
            }
        }
        let mut tns_active = [false; 8];
        let mut nrg_scaling = [0i32; 2];
        let mut tns_spec_shift = 0i32;
        if tns_on {
            for c in 0..channels {
                let tconf = if short[c] {
                    &tables.tns_short
                } else {
                    &tables.tns_long
                };
                for w in 0..n_win[c] {
                    let base = w * wlen[c];
                    let d = &mut ch[c];
                    tns::detect(
                        &mut d.tns_state,
                        &mut d.tns_info,
                        tconf,
                        &q[c][base..base + wlen[c]],
                        w,
                        short[c],
                        confs[c].sfb_count(),
                    );
                }
            }
            if stereo {
                let (l, r) = ch.split_at_mut(1);
                // F1: production passes the RIGHT channel's config
                // (`tconf_r`, psy.rs:853-861). The D-022 pair agreement
                // asserted above implies short[0] == short[1]; select by the
                // right channel to mirror production exactly: long stereo
                // pairs use tns_long (max_order 12), short pairs tns_short.
                let sync_conf = sync_config(&tables, short[1]);
                // Old-vs-corrected quantification: run the previous
                // (short-always) selection on cloned state and compare
                // sync outcomes, isolating the configuration effect.
                let mut old_state = r[0].tns_state.clone();
                let mut old_info = r[0].tns_info.clone();
                tns::synchronize_pair(
                    &mut old_state,
                    &mut old_info,
                    &l[0].tns_state,
                    &l[0].tns_info,
                    &tables.tns_short,
                    short[1],
                    short[0],
                );
                tns::synchronize_pair(
                    &mut r[0].tns_state,
                    &mut r[0].tns_info,
                    &l[0].tns_state,
                    &l[0].tns_info,
                    sync_conf,
                    short[1],
                    short[0],
                );
                if short[1] {
                    stats.sync_short_pairs += 1;
                } else {
                    stats.sync_long_pairs += 1;
                }
                if old_state.active != r[0].tns_state.active
                    || old_state.merged != r[0].tns_state.merged
                    || old_info.num_filters != r[0].tns_info.num_filters
                    || old_info.order != r[0].tns_info.order
                    || old_info.length != r[0].tns_info.length
                    || old_info.direction != r[0].tns_info.direction
                    || old_info.coef != r[0].tns_info.coef
                {
                    stats.sync_divergences += 1;
                    if stats.sync_divergence_log.len() < 40 {
                        stats
                            .sync_divergence_log
                            .push(format!("{name} f{frame} (short={})", short[1]));
                    }
                }
            }
            let nw = if short_elem { 8 } else { 1 };
            for w in 0..nw {
                let mut any = false;
                for c in 0..channels {
                    any = any || ch[c].tns_state.active[w][0] || ch[c].tns_state.active[w][1];
                }
                tns_active[w] = any;
            }
            if tns_active.iter().any(|&a| a) {
                stats.tns_active_frames += 1;
                if short_elem {
                    stats.short_tns_active_frames += 1;
                }
            }
            for c in 0..channels {
                let tconf = if short[c] {
                    &tables.tns_short
                } else {
                    &tables.tns_long
                };
                let d = &mut ch[c];
                tns_encode_channel(
                    &mut d.tns_state,
                    &mut d.tns_info,
                    tconf,
                    confs[c],
                    &mut specs[c],
                    &mut q[c],
                    &mut maxsc[c],
                    &mut e_tns[c],
                    &mut eld_tns[c],
                    &mut thr_tns[c],
                    &tns_active,
                    short[c],
                    active[c],
                    &mut m[c],
                    &mut nrg_scaling[c],
                    &mut tns_spec_shift,
                    &mut qwin,
                    &mut tns_pre,
                );
            }
        } else {
            for d in ch.iter_mut() {
                d.tns_state.clear_runtime();
            }
        }

        // --- TNS recompute experiment. ---
        for c in 0..channels {
            for w in 0..n_win[c] {
                let win_len = wlen[c];
                let win = &specs[c][w * win_len..(w + 1) * win_len];
                let is_active = tns_active[w];
                // Production recompute inputs for active windows are read
                // back from the real encode call (filtered spectrum +
                // recomputed headroom + integer shiftBits + post-pre-halving
                // exponent). Inactive windows exercise the same math on the
                // unfiltered spectrum with shiftBits = 0 and an independently
                // computed Q31 reference (production skips recompute there;
                // rows are labelled inactive).
                // F7 SCOPE: the inactive-window extension covers
                // shiftBits = 0 ONLY. Nonzero shiftBits change both the
                // stored scaling (2^(−2·shiftBits)) and the Ld correction
                // (−shiftBits·2/64); inactive rows must not be read as
                // covering them. Nonzero-shift coverage comes exclusively
                // from the TNS-long-active rows (observed range reported).
                let (e_q31w, m_use, maxsc_use, shift_use) = if is_active {
                    (
                        e_tns[c][w].clone(),
                        m[c],
                        maxsc[c][w].clone(),
                        if short[c] { 0 } else { nrg_scaling[c] },
                    )
                } else {
                    let (x_q31, _) = envelope::map_q31(win, m[c]);
                    let msc = envelope::max_scale_q31(&x_q31, confs[c].offsets(), active[c]);
                    let eq: Vec<i32> = if short[c] {
                        let mut e = Vec::new();
                        q31_energy::band_energy_short_into(
                            &x_q31,
                            &msc,
                            confs[c].offsets(),
                            active[c],
                            &mut e,
                        );
                        e
                    } else {
                        let mut pre = Vec::new();
                        let mut e = Vec::new();
                        let mut l = Vec::new();
                        q31_energy::band_energy_long_into(
                            &x_q31,
                            &msc,
                            confs[c].offsets(),
                            active[c],
                            &mut pre,
                            &mut e,
                            &mut l,
                        );
                        e
                    };
                    (eq, m[c], msc, 0)
                };
                let tag = || {
                    format!(
                        "{name} f{frame} ch{c} w{w} tns-{}-{}",
                        if short[c] { "short" } else { "long" },
                        if is_active { "active" } else { "inactive" }
                    )
                };
                if short[c] {
                    // Short: energy-only, caller Ld untouched.
                    let mut e64 = vec![0i32; active[c]];
                    let mut ld_keep = vec![-2.5f64; active[c]];
                    fpe_energy::tns_recompute_energy_into(
                        win,
                        confs[c].offsets(),
                        active[c],
                        m_use,
                        &maxsc_use,
                        0,
                        true,
                        &mut e64,
                        &mut ld_keep,
                    );
                    assert_eq!(
                        ld_keep,
                        vec![-2.5f64; active[c]],
                        "short recompute must leave Ld untouched"
                    );
                    let st = if is_active {
                        &mut stats.tns_short_active
                    } else {
                        &mut stats.tns_short_inactive
                    };
                    for b in 0..active[c] {
                        let band = &win
                            [confs[c].offsets()[b] as usize..confs[c].offsets()[b + 1] as usize];
                        let e_real_64: f64 = band.iter().map(|&x| x * x).sum();
                        let mut ar = 0f32;
                        for &x in band {
                            let xf = x as f32;
                            ar += xf * xf;
                        }
                        let e_real_r = ar as f64;
                        let mut aa = 0f64;
                        for &x in band {
                            aa = (aa + x * x) as f32 as f64;
                        }
                        let e_f64 = e_real_64 * envelope::pow2(-2 * m_use);
                        let e_fr = e_real_r * envelope::pow2(-2 * m_use);
                        let e_fa = aa * envelope::pow2(-2 * m_use);
                        let st64 = fpe_output::fraction_to_stored(e_f64);
                        assert_eq!(st64, e64[b], "harness short replication drift");
                        let str_ = fpe_output::fraction_to_stored(e_fr);
                        let sta = fpe_output::fraction_to_stored(e_fa);
                        let (x_q31, sat) = envelope::map_q31(win, m_use);
                        // Short envelope per the TNS unit-test convention
                        // (energy.rs tns_tol, short branch).
                        let n = (confs[c].offsets()[b + 1] - confs[c].offsets()[b]) as f64;
                        let sum_abs: f64 = x_q31
                            [confs[c].offsets()[b] as usize..confs[c].offsets()[b + 1] as usize]
                            .iter()
                            .map(|&v| (v as f64).abs())
                            .sum();
                        let s = (2 * (maxsc_use[b] - 3) - 1).clamp(-31, 31);
                        let qb = sum_abs * envelope::pow2(-31) + n * envelope::pow2(-33);
                        let tb = 2.0 * n * envelope::pow2(-s);
                        let tol = (32.0f64.max(qb + tb + 1.5) + (e_q31w[b] as f64).abs() / 500.0)
                            .min(envelope::MAX_FINITE_ENERGY_TOL);
                        st.bands += 1;
                        if !e_f64.is_finite() || !e_fr.is_finite() || !e_fa.is_finite() {
                            st.non_finite += 1;
                            continue;
                        }
                        // F6: saturation is exact here. fraction_to_stored
                        // saturates iff round(e_f·2^31) exceeds i32::MAX, i.e.
                        // e_f·2^31 > MAX−0.5 (2147483646.5 is exactly
                        // representable; round-half-away still lands on MAX, a
                        // saturating clamp to the same value). The half-LSB
                        // sliver [MAX−1, MAX−0.5] rounds exactly to MAX and is
                        // not saturation.
                        if e_f64 * envelope::pow2(31) > i32::MAX as f64 - 0.5 {
                            st.saturated += 1;
                        }
                        if sat {
                            st.unsupported += 1;
                            *st.unsupported_reasons
                                .entry("Q31 input mapping saturated")
                                .or_insert(0) += 1;
                        } else {
                            st.supported += 1;
                        }
                        let d_q64 = (st64 as i64 - e_q31w[b] as i64).abs();
                        assert!(
                            !sat && (d_q64 as f64) <= tol,
                            "{} b{b}: short Q31↔F64 fidelity break",
                            tag()
                        );
                        if let Some(cls) = classify(
                            &x_q31[confs[c].offsets()[b] as usize
                                ..confs[c].offsets()[b + 1] as usize],
                            e_q31w[b],
                            st64,
                        ) {
                            match cls {
                                "A" => st.class_a += 1,
                                _ => st.class_b += 1,
                            }
                            if str_ != st64 {
                                st.class_flip_r += 1;
                            }
                            if sta != st64 {
                                st.class_flip_a += 1;
                            }
                        }
                        let tagb = format!("{} b{b}", tag());
                        let (r, a, log) = (&mut st.r, &mut st.a, &mut stats.flip_log);
                        record_arm(
                            r, st64, str_, e_f64, e_fr, -1.0, -1.0, true, true, false, false, tol,
                            log, &tagb,
                        );
                        record_arm(
                            a, st64, sta, e_f64, e_fa, -1.0, -1.0, true, true, false, false, tol,
                            log, &tagb,
                        );
                    }
                } else {
                    // Long: production kernel writes stored + Ld in place.
                    let mut e64 = vec![0i32; active[c]];
                    let mut ld64 = vec![-1.0f64; active[c]];
                    fpe_energy::tns_recompute_energy_into(
                        win,
                        confs[c].offsets(),
                        active[c],
                        m_use,
                        &maxsc_use,
                        shift_use,
                        false,
                        &mut e64,
                        &mut ld64,
                    );
                    let st = if is_active {
                        // F7: record the observed nonzero-shift range. Only
                        // the active-long rows cover shiftBits != 0; the
                        // inactive-window extension is shiftBits = 0 only.
                        stats.max_shiftbits = stats.max_shiftbits.max(shift_use);
                        if shift_use != 0 {
                            stats.nonzero_shift_windows += 1;
                        }
                        &mut stats.tns_long_active
                    } else {
                        &mut stats.tns_long_inactive
                    };
                    for b in 0..active[c] {
                        let band = &win
                            [confs[c].offsets()[b] as usize..confs[c].offsets()[b + 1] as usize];
                        let e_real_64: f64 = band.iter().map(|&x| x * x).sum();
                        let mut ar = 0f32;
                        for &x in band {
                            let xf = x as f32;
                            ar += xf * xf;
                        }
                        let e_real_r = ar as f64;
                        let mut aa = 0f64;
                        for &x in band {
                            aa = (aa + x * x) as f32 as f64;
                        }
                        let leading = maxsc_use[b] - 4;
                        let (st64, ld64v) =
                            tns_long_ld_stored(e_real_64, m_use, leading, shift_use);
                        assert_eq!(st64, e64[b], "harness TNS-long replication drift");
                        assert!(
                            (ld64v - ld64[b]).abs() == 0.0,
                            "harness TNS-long Ld replication drift"
                        );
                        let (str_, ldr) = tns_long_ld_stored(e_real_r, m_use, leading, shift_use);
                        let (sta, lda) = tns_long_ld_stored(aa, m_use, leading, shift_use);
                        let (x_q31, sat) = envelope::map_q31(win, m_use);
                        // Long TNS envelope (energy.rs tns_tol, long branch).
                        let n = (confs[c].offsets()[b + 1] - confs[c].offsets()[b]) as f64;
                        let sum_abs: f64 = x_q31
                            [confs[c].offsets()[b] as usize..confs[c].offsets()[b + 1] as usize]
                            .iter()
                            .map(|&v| (v as f64).abs())
                            .sum();
                        let s = (2 * (leading + shift_use)).min(31);
                        let qb = sum_abs * envelope::pow2(-31 - 2 * shift_use)
                            + n * envelope::pow2(-33 - 2 * shift_use);
                        let tb = 2.0 * n * envelope::pow2(-s);
                        let tol = (32.0f64.max(qb + tb + 1.5) + (e_q31w[b] as f64).abs() / 500.0)
                            .min(envelope::MAX_FINITE_ENERGY_TOL);
                        st.bands += 1;
                        if !e_real_64.is_finite() || !e_real_r.is_finite() || !aa.is_finite() {
                            st.non_finite += 1;
                            continue;
                        }
                        // F6: exact saturation boundary (see rescale site):
                        // the rounded value stored is round of this product.
                        if e_real_64 * envelope::pow2(31 - 2 * (m_use + shift_use))
                            > i32::MAX as f64 - 0.5
                        {
                            st.saturated += 1;
                        }
                        if sat {
                            st.unsupported += 1;
                            *st.unsupported_reasons
                                .entry("Q31 input mapping saturated")
                                .or_insert(0) += 1;
                        } else {
                            st.supported += 1;
                        }
                        let d_q64 = (st64 as i64 - e_q31w[b] as i64).abs();
                        assert!(
                            !sat && (d_q64 as f64) <= tol,
                            "{} b{b}: TNS-long Q31↔F64 fidelity break \
                             (active={is_active})",
                            tag()
                        );
                        if let Some(cls) = classify(
                            &x_q31[confs[c].offsets()[b] as usize
                                ..confs[c].offsets()[b + 1] as usize],
                            e_q31w[b],
                            st64,
                        ) {
                            match cls {
                                "A" => st.class_a += 1,
                                _ => st.class_b += 1,
                            }
                            if str_ != st64 {
                                st.class_flip_r += 1;
                            }
                            if sta != st64 {
                                st.class_flip_a += 1;
                            }
                        }
                        // Guard proximity: distance of the f64 base to the
                        // guard threshold (finite active bands only).
                        let scale_diff = leading as f64 * 2.0 / 64.0;
                        let e_raw_f = e_real_64 * envelope::pow2(-2 * m_use + 2 * leading);
                        if e_raw_f > 0.0 {
                            let base = e_raw_f.log2() / 64.0;
                            let dist = (base - (-0.5 + scale_diff / 2.0)).abs();
                            st.min_guard_distance = st.min_guard_distance.min(dist);
                        }
                        let sent64 = e_real_64 == 0.0;
                        let sentr = e_real_r == 0.0;
                        let senta = aa == 0.0;
                        let g64 = ld64[b] == -1.0 && e_real_64 != 0.0;
                        let gr = ldr == -1.0 && e_real_r != 0.0;
                        let ga = lda == -1.0 && aa != 0.0;
                        let tagb = format!("{} b{b}", tag());
                        let (r, a, log) = (&mut st.r, &mut st.a, &mut stats.flip_log);
                        record_arm(
                            r, st64, str_, e_real_64, e_real_r, ld64[b], ldr, sent64, sentr, g64,
                            gr, tol, log, &tagb,
                        );
                        record_arm(
                            a, st64, sta, e_real_64, aa, ld64[b], lda, sent64, senta, g64, ga, tol,
                            log, &tagb,
                        );
                    }
                }
            }
        }

        // --- Overflow mirror + downstream sensitivity probe.
        // This probe is NOT a production mirror. Production threads
        // post-TNS/overflow e_l into advance_thresholds (psy.rs:974-1006):
        // the TNS-recomputed energies feed spreading (which copies
        // energies) and the resulting thresholds feed pre-echo. The probe
        // instead passes each arm's RESCALE-basis stored vector uniformly,
        // measuring downstream sensitivity to rescale-energy perturbations
        // only. Threshold init threading IS mirrored (init → pre-halving
        // >>=2 where production applies it → overflow >>=2·shift, all with
        // the same shift_int production uses), so the Q31 arm follows
        // production's threshold construction on the rescale basis. No
        // decision-safety claim follows from these numbers (R2.2 owns
        // decision safety). Do not expand this into a full downstream
        // production mirror.
        let m_final = if tns_spec_shift != 0 {
            let mut mf = m.clone();
            for c in 0..channels {
                let escale = (tns_spec_shift - nrg_scaling[c]) << 1;
                for w in 0..n_win[c] {
                    for b in 0..active[c] {
                        let (s64, sr, sa) = arm_store[c][w][b];
                        arm_store[c][w][b] = (
                            shift_int(s64, -escale),
                            shift_int(sr, -escale),
                            shift_int(sa, -escale),
                        );
                    }
                }
                mf[c] += tns_spec_shift;
            }
            mf
        } else {
            m.clone()
        };
        for c in 0..channels {
            // F2: threshold threading mirrors production on the rescale
            // basis: init at rescale, pre-halving >>=2 exactly where
            // production applies it inside tns_encode_channel
            // (tns_active[0] && !short, psy.rs:1165-1173), then overflow
            // >>=2·shift. Applied uniformly to all four arms.
            let hscale = if tns_on && tns_active[0] && !short[c] {
                2
            } else {
                0
            };
            let tscale = (tns_spec_shift << 1) + hscale;
            let mut thr_arms: [Vec<Vec<i32>>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
            for w in 0..n_win[c] {
                let mut tq = Vec::with_capacity(active[c]);
                let mut t64 = Vec::with_capacity(active[c]);
                let mut tr = Vec::with_capacity(active[c]);
                let mut ta = Vec::with_capacity(active[c]);
                for b in 0..active[c] {
                    let (s64, sr, sa) = arm_store[c][w][b];
                    tq.push(shift_int(init_threshold(e_q[c][w][b]), -tscale));
                    t64.push(shift_int(init_threshold(s64), -tscale));
                    tr.push(shift_int(init_threshold(sr), -tscale));
                    ta.push(shift_int(init_threshold(sa), -tscale));
                }
                // e_q[c][w] is the rescale stored vector (tns_encode_channel
                // wrote the recomputed values into e_tns, never into e_q).
                // All four arms are rescale-basis by construction (see the
                // probe header); production's post-TNS e_l is deliberately
                // not replicated here.
                thr_arms[0].push(tq);
                thr_arms[1].push(t64);
                thr_arms[2].push(tr);
                thr_arms[3].push(ta);
            }
            let mut spread_arms: [Vec<Vec<i32>>; 4] =
                [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
            for w in 0..n_win[c] {
                for s in spread_arms.iter_mut() {
                    s.push(vec![0i32; active[c]]);
                }
            }
            // Energies input to advance_thresholds: production passes the
            // post-TNS/overflow e_l (spread copies it). The probe passes
            // each arm's rescale stored vector instead — the uniform
            // rescale basis for sensitivity measurement (see header),
            // with the same overflow shift applied to all four arms.
            let mut en_arms: [Vec<Vec<i32>>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
            // F2: uniform overflow handling. arm_store vectors were already
            // overflow-shifted above; e_q is rescale-basis without that
            // shift, so apply the same escale here (no-op when
            // tns_spec_shift == 0, as on the current corpus).
            let escale_en = (tns_spec_shift - nrg_scaling[c]) << 1;
            for w in 0..n_win[c] {
                let mut eq = Vec::with_capacity(active[c]);
                let mut e64 = Vec::with_capacity(active[c]);
                let mut er = Vec::with_capacity(active[c]);
                let mut ea = Vec::with_capacity(active[c]);
                for b in 0..active[c] {
                    let (s64, sr, sa) = arm_store[c][w][b];
                    eq.push(shift_int(e_q[c][w][b], -escale_en));
                    e64.push(s64);
                    er.push(sr);
                    ea.push(sa);
                }
                en_arms[0].push(eq);
                en_arms[1].push(e64);
                en_arms[2].push(er);
                en_arms[3].push(ea);
            }
            let seq = seqs[c];
            {
                let d = &mut ch[c];
                advance_thresholds(
                    &mut d.pre_q31,
                    &mut thr_arms[0],
                    &mut spread_arms[0],
                    &en_arms[0],
                    confs[c],
                    active[c],
                    m_final[c],
                    short[c],
                    seq,
                    false,
                );
                advance_thresholds(
                    &mut d.pre_f64,
                    &mut thr_arms[1],
                    &mut spread_arms[1],
                    &en_arms[1],
                    confs[c],
                    active[c],
                    m_final[c],
                    short[c],
                    seq,
                    false,
                );
                advance_thresholds(
                    &mut d.pre_f32r,
                    &mut thr_arms[2],
                    &mut spread_arms[2],
                    &en_arms[2],
                    confs[c],
                    active[c],
                    m_final[c],
                    short[c],
                    seq,
                    false,
                );
                advance_thresholds(
                    &mut d.pre_f32a,
                    &mut thr_arms[3],
                    &mut spread_arms[3],
                    &en_arms[3],
                    confs[c],
                    active[c],
                    m_final[c],
                    short[c],
                    seq,
                    false,
                );
            }
            for w in 0..n_win[c] {
                for b in 0..active[c] {
                    let d_r_q31 = (thr_arms[2][w][b] as i64 - thr_arms[0][w][b] as i64).abs();
                    let d_a_q31 = (thr_arms[3][w][b] as i64 - thr_arms[0][w][b] as i64).abs();
                    let d_r_64 = (thr_arms[2][w][b] as i64 - thr_arms[1][w][b] as i64).abs();
                    let d_a_64 = (thr_arms[3][w][b] as i64 - thr_arms[1][w][b] as i64).abs();
                    stats.max_thr_delta_r_q31 = stats.max_thr_delta_r_q31.max(d_r_q31);
                    stats.max_thr_delta_a_q31 = stats.max_thr_delta_a_q31.max(d_a_q31);
                    stats.max_thr_delta_r_64 = stats.max_thr_delta_r_64.max(d_r_64);
                    stats.max_thr_delta_a_64 = stats.max_thr_delta_a_64.max(d_a_64);
                    let ds_r = (spread_arms[2][w][b] as i64 - spread_arms[1][w][b] as i64).abs();
                    let ds_a = (spread_arms[3][w][b] as i64 - spread_arms[1][w][b] as i64).abs();
                    stats.max_spread_delta_r_64 = stats.max_spread_delta_r_64.max(ds_r);
                    stats.max_spread_delta_a_64 = stats.max_spread_delta_a_64.max(ds_a);
                }
            }
            let d = &ch[c];
            for b in 0..active[c] {
                let dp_r = (d.pre_f32r.prev()[b] as i64 - d.pre_f64.prev()[b] as i64).abs();
                let dp_a = (d.pre_f32a.prev()[b] as i64 - d.pre_f64.prev()[b] as i64).abs();
                stats.max_preecho_prev_delta_r_64 = stats.max_preecho_prev_delta_r_64.max(dp_r);
                stats.max_preecho_prev_delta_a_64 = stats.max_preecho_prev_delta_a_64.max(dp_a);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

fn report_kernel(tag: &str, k: &KernelStats, ld_na: bool) {
    println!("--- {tag} ---");
    println!(
        "bands={} supported={} unsupported={} non_finite={} saturated={} classA={} classB={} classFlipR={} classFlipA={}",
        k.bands, k.supported, k.unsupported, k.non_finite, k.saturated, k.class_a,
        k.class_b, k.class_flip_r, k.class_flip_a
    );
    for (reason, n) in k.unsupported_reasons.iter() {
        println!("  unsupported: {reason}: {n}");
    }
    let ld_r = if ld_na {
        "N/A(short energy-only)".to_string()
    } else {
        format!("{:.3e}", k.r.max_abs_ld_delta)
    };
    let ld_a = if ld_na {
        "N/A(short energy-only)".to_string()
    } else {
        format!("{:.3e}", k.a.max_abs_ld_delta)
    };
    println!(
        "  f32r-vs-f64: maxStoredLSB={} @{} buckets[0,1,2-4,5-16,17-64,65+]={:?} meanStoredLSB={:.4} maxRelFrac={:.3e} maxAbsLd={} overEnv={} maxUtil={:.4} guardFlips={} sentFlips={}",
        k.r.max_abs_stored_delta,
        k.r.max_loc,
        k.r.buckets,
        if k.r.bands > 0 {
            k.r.sum_abs_stored_delta / k.r.bands as f64
        } else {
            0.0
        },
        k.r.max_rel_frac_delta,
        ld_r,
        k.r.over_envelope,
        k.r.max_utilization,
        k.r.guard_flips,
        k.r.sentinel_flips
    );
    println!(
        "  f32a-vs-f64: maxStoredLSB={} @{} buckets[0,1,2-4,5-16,17-64,65+]={:?} meanStoredLSB={:.4} maxRelFrac={:.3e} maxAbsLd={} overEnv={} maxUtil={:.4} guardFlips={} sentFlips={}",
        k.a.max_abs_stored_delta,
        k.a.max_loc,
        k.a.buckets,
        if k.a.bands > 0 {
            k.a.sum_abs_stored_delta / k.a.bands as f64
        } else {
            0.0
        },
        k.a.max_rel_frac_delta,
        ld_a,
        k.a.over_envelope,
        k.a.max_utilization,
        k.a.guard_flips,
        k.a.sentinel_flips
    );
    println!("  minGuardDistance={:.3e}", k.min_guard_distance);
}

fn report_corpus(stats: &CorpusStats) {
    println!("=== A-4 corpus ===");
    println!(
        "frames={} long={} short={} tnsActive={} shortTnsActive={}",
        stats.frames,
        stats.long_frames,
        stats.short_frames,
        stats.tns_active_frames,
        stats.short_tns_active_frames
    );
    report_kernel("rescale", &stats.rescale, false);
    report_kernel("tns-long-active", &stats.tns_long_active, false);
    report_kernel("tns-long-inactive", &stats.tns_long_inactive, false);
    report_kernel("tns-short-active", &stats.tns_short_active, true);
    report_kernel("tns-short-inactive", &stats.tns_short_inactive, true);
    println!(
        "downstream maxThr: r-vs-q31={} a-vs-q31={} r-vs-64={} a-vs-64={}",
        stats.max_thr_delta_r_q31,
        stats.max_thr_delta_a_q31,
        stats.max_thr_delta_r_64,
        stats.max_thr_delta_a_64
    );
    println!(
        "downstream maxSpread(vs64): r={} a={}  maxPreEchoPrev(vs64): r={} a={}",
        stats.max_spread_delta_r_64,
        stats.max_spread_delta_a_64,
        stats.max_preecho_prev_delta_r_64,
        stats.max_preecho_prev_delta_a_64
    );
    println!("flip/outlier log entries: {}", stats.flip_log.len());
    for line in stats.flip_log.iter().take(60) {
        println!("  {line}");
    }
    println!(
        "F1 sync: longPairs={} shortPairs={} oldVsCorrectedDivergences={}",
        stats.sync_long_pairs, stats.sync_short_pairs, stats.sync_divergences
    );
    for line in stats.sync_divergence_log.iter() {
        println!("  sync-divergence: {line}");
    }
    println!(
        "F7 active-long shiftBits: max={} nonzeroWindows={}",
        stats.max_shiftbits, stats.nonzero_shift_windows
    );
}

// ---------------------------------------------------------------------------
// Unit tests: arm semantics on synthetic spectra
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f1_sync_config_selection_matches_production() {
        // The driver must pass the right channel's config (tconf_r):
        // long pairs → tns_long, short pairs → tns_short. Mono has no
        // sync call at all (code path unchanged by the F1 fix).
        let ((lcfg, _), (scfg, _)) = testdata::load_cfg(48000, 96000);
        let tables = PsyTables::for_rate(
            48000,
            96000,
            192000,
            19293,
            lcfg.min_snr_ld,
            scfg.min_snr_ld,
        )
        .expect("tables build");
        assert!(
            std::ptr::eq(sync_config(&tables, false), &tables.tns_long),
            "long stereo sync must use tns_long"
        );
        assert!(
            std::ptr::eq(sync_config(&tables, true), &tables.tns_short),
            "short stereo sync must use tns_short"
        );
        assert!(
            tables.tns_long.max_order > tables.tns_short.max_order,
            "test setup: configs must differ in similarity-loop length"
        );
    }

    #[test]
    fn f1_sync_config_changes_merge_decisions() {
        // Coefficients identical over the short prefix but differing
        // beyond it: short-config sync merges (loop too short to see the
        // difference), long-config sync declines. Proves the F1
        // configuration choice is outcome-relevant, not cosmetic.
        let ((lcfg, _), (scfg, _)) = testdata::load_cfg(48000, 96000);
        let tables = PsyTables::for_rate(
            48000,
            96000,
            192000,
            19293,
            lcfg.min_snr_ld,
            scfg.min_snr_ld,
        )
        .expect("tables build");
        let k = tables.tns_short.max_order;
        assert!(k < tables.tns_long.max_order);
        let mut run = |cfg: &crate::aac_enc::tns::TnsConfig| {
            let mut dst_s = TnsChannel::new();
            let mut dst_i = TnsChannel::new();
            let mut src_s = TnsChannel::new();
            let mut src_i = TnsChannel::new();
            src_s.active[0][tns::HI] = true;
            src_i.num_filters[0] = 1;
            for i in 0..12 {
                dst_i.coef[0][tns::HI][i] = 3;
                src_i.coef[0][tns::HI][i] = if i < k { 3 } else { 9 };
            }
            tns::synchronize_pair(&mut dst_s, &mut dst_i, &src_s, &src_i, cfg, false, false);
            (dst_s.active[0][tns::HI], dst_s.merged)
        };
        let (short_active, _) = run(&tables.tns_short);
        let (long_active, _) = run(&tables.tns_long);
        assert!(short_active, "short config merges on identical prefix");
        assert!(
            !long_active,
            "long config sees the beyond-prefix difference and declines"
        );
    }

    #[test]
    fn arms_agree_on_trivial_inputs() {
        // Zeros: all arms exact, sentinel paths identical.
        let spec = [0.0f64; 16];
        let offsets = [0u16, 8, 16];
        for b in 0..2 {
            let band = &spec[offsets[b] as usize..offsets[b + 1] as usize];
            assert_eq!(rescale_f32r_band(band, 12, 3), 0.0);
            assert_eq!(rescale_f32a_band(band, 12, 3), 0.0);
            let (s, ld) = rescale_ld_stored(0.0, 2, 3);
            assert_eq!(s, 0);
            assert_eq!(ld, -1.0 + 3.0 * 2.0 / 64.0);
        }
        // Powers of two within f32 range: f32 arithmetic is exact, so the
        // realistic arm must match f64 bit-exactly here.
        let spec2 = [0.5, -0.25, 0.125, -0.0625];
        let e64: f64 = spec2.iter().map(|&x| x * x).sum::<f64>() * envelope::pow2(0);
        let er = rescale_f32r_band(&spec2, 0, 0);
        let ea = rescale_f32a_band(&spec2, 0, 0);
        assert_eq!(er, e64, "f32r exact on powers of two");
        assert_eq!(ea, e64, "f32a exact on powers of two");
    }

    #[test]
    fn realistic_arm_narrows_inputs() {
        // Mechanism check with x = sqrt(2) (irrational: not f32-representable).
        // The realistic arm must consume the narrowed input value, while the
        // diagnostic arm rounds only the accumulator. Both are verified
        // against their defining expressions rather than against each other.
        let x = 2f64.sqrt();
        assert_ne!((x as f32) as f64, x, "test setup: value must narrow in f32");
        let band = [x];
        let er = rescale_f32r_band(&band, 0, 0);
        let ea = rescale_f32a_band(&band, 0, 0);
        let xf = x as f32;
        let expect_r = (xf * xf) as f64 * envelope::pow2(0);
        let expect_a = ((x * x) as f32) as f64 * envelope::pow2(0);
        assert_eq!(
            er, expect_r,
            "realistic arm narrows input then squares in f32"
        );
        assert_eq!(
            ea, expect_a,
            "diagnostic arm squares in f64, rounds accumulator"
        );
        assert_ne!(er, x * x, "input narrowing changes the result");
    }

    #[test]
    fn accumulator_diagnostic_isolates_rounding() {
        // Many small addends into a large total: f32 accumulation absorbs
        // small values once the running total exceeds 2^24·ulp spacing.
        // 2^10 ones then small values: diagnostic must lose the tail.
        let mut band = vec![1.0f64; 64];
        band.extend(std::iter::repeat(1.0 / 65536.0).take(64));
        let e64: f64 = band.iter().map(|&x| x * x).sum();
        let ea = rescale_f32a_band(&band, 0, 0);
        let er = rescale_f32r_band(&band, 0, 0);
        // f64 reference resolves everything.
        assert!((e64 - (64.0 + 64.0 / 65536.0f64.powi(2))).abs() < 1e-9);
        // Both f32 variants lose precision here (absorption); the point is
        // they are measured, not that they match.
        println!("absorption: e64={e64:.10} er={er:.10} ea={ea:.10}");
        assert!(ea <= e64, "accumulator rounding can only lose addends");
    }

    #[test]
    fn pure_f32_boundary_is_nonviable_negative_control() {
        // Full-scale band at zero headroom: E_f = 4.0 → E_st = 2^33 clamps.
        // A smaller loud case: E_f just below 1.0 needs sub-256-LSB
        // resolution that f32 cannot provide at 2^31 scale.
        let e_f = (i32::MAX as f64 - 100.0) / 2_147_483_648.0;
        let st64 = fpe_output::fraction_to_stored(e_f);
        // Pure-f32 boundary: round in f32 domain.
        let st32 = ((e_f as f32) * 2_147_483_648.0f32).round() as i64;
        let delta = (st32 - st64 as i64).abs();
        println!("negative control: st64={st64} st32={st32} delta={delta}");
        // f32 ULP at 2^31 magnitude is 256; deltas up to ~128 are expected
        // from boundary coarseness alone. This control is excluded from any
        // candidate acceptance: it demonstrates WHY the boundary stays f64.
        assert_eq!(st64, i32::MAX - 100);
        assert!(
            delta <= 256,
            "control behaves as analyzed (f32 spacing ≤256 LSB here)"
        );
    }

    #[test]
    fn sub_lsb_class_a_detected() {
        // Sub-LSB spectrum at m = 12: maps to all-zero Q31 (Class A setup
        // from energy.rs tests); f64 resolves positive energy.
        let spec = [
            1.0 / 4_194_304.0,
            -1.0 / 4_194_304.0,
            0.5 / 4_194_304.0,
            0.0,
        ];
        let (x_q31, _) = envelope::map_q31(&spec, 12);
        assert!(x_q31.iter().all(|&v| v == 0));
        let e_f64: f64 = spec.iter().map(|&x| x * x).sum();
        assert!(e_f64 > 0.0);
        let er = rescale_f32r_band(&spec, 12, 12);
        let ea = rescale_f32a_band(&spec, 12, 12);
        // m′ = 0 so E_f = E_real; both variants must resolve positive energy
        // (values are exactly representable in f32, so both are exact).
        assert_eq!(er, e_f64);
        assert_eq!(ea, e_f64);
        assert_eq!(classify(&x_q31, 0, 1), Some("A"));
    }

    // -----------------------------------------------------------------------
    // Corpus experiment (authorized corpus execution for A-4 only)
    // -----------------------------------------------------------------------

    const FIXTURES: &[&str] = &[
        "silence-stereo",
        "fpe-allzero-stereo",
        "quiet-sine-stereo",
        "fpe-near-silence-ramp-stereo",
        "sine-stereo",
        "sine-stereo-44100",
        "multitone-stereo",
        "tonal-noise-stereo",
        "noise-mono",
        "sweep-stereo",
        "music-stereo",
        "speech-clean-stereo",
        "speech-pauses-stereo",
        "music-eroica-stereo",
        "onset-stereo",
        "onset-stereo-44100",
        "onset-mono-44100",
        "stepdown-stereo",
        "click-mono",
        "edge-mono",
        "impulse-mono",
        "stress-stereo",
        "fpe-fullscale-square-stereo",
        "fpe-pow2-boundary-stereo",
        "fpe-max-headroom-stereo",
        "fpe-mixed-pair-stereo",
        "fpe-tns-active-short-stereo",
    ];

    #[test]
    fn a4_precision_corpus() {
        let mut stats = CorpusStats::new();
        for name in FIXTURES {
            let before = stats.frames;
            drive_fixture(name, &mut stats);
            println!("{name}: frames={}", stats.frames - before);
        }
        report_corpus(&stats);

        // Coverage gates (unexecuted cells fail, never pass silently).
        assert!(stats.frames > 0, "no frames executed");
        assert!(stats.long_frames > 0, "no long-window frames");
        assert!(stats.short_frames > 0, "no short-window frames");
        assert!(
            stats.tns_active_frames > 0,
            "no TNS-active frames exercised"
        );
        assert!(
            stats.short_tns_active_frames > 0,
            "no short-window TNS-active frames (fpe-tns-active-short-stereo)"
        );
        assert!(stats.rescale.bands > 0, "no rescale bands");
        assert!(stats.tns_long_active.bands > 0, "no TNS-long-active bands");
        assert!(
            stats.tns_short_active.bands > 0 || stats.tns_short_inactive.bands > 0,
            "no short recompute bands"
        );
        // Class coverage: the full corpus must surface Class B
        // (truncation vs rounding). Class A (mapping-to-zero) is covered
        // deterministically by `sub_lsb_class_a_detected` above and the
        // audited energy.rs Class A tests; real-corpus mining yielded none
        // on this fixture set (reported, not a pass — see limitations).
        assert!(
            stats.rescale.class_b + stats.tns_long_active.class_b + stats.tns_long_inactive.class_b
                > 0,
            "no Class B bands mined"
        );
        println!(
            "Class A mined on corpus: rescale={} tnsLongA={} tnsLongI={} \
             (0 is reported; synthetic coverage in unit tests)",
            stats.rescale.class_a, stats.tns_long_active.class_a, stats.tns_long_inactive.class_a
        );
        // Structural invariants (not acceptance thresholds): finite
        // everywhere, no silent skips.
        assert_eq!(stats.rescale.non_finite, 0, "non-finite rescale values");
        assert_eq!(
            stats.tns_long_active.non_finite + stats.tns_long_inactive.non_finite,
            0,
            "non-finite TNS-long values"
        );
        assert_eq!(
            stats.tns_short_active.non_finite + stats.tns_short_inactive.non_finite,
            0,
            "non-finite TNS-short values"
        );
        // No invented pass/fail on deltas: results are reported above for
        // owner review. This test passes iff the experiment ran completely
        // and its structural invariants hold.
    }
}
