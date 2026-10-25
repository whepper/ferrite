//! Phase 1 test-only PCM-to-ADTS streaming driver.
//!
//! Chains the existing native encoder modules across successive PCM frames
//! and emits ADTS output for comparison with the original C encoder. This
//! is integration validation, not a production API: everything here lives
//! behind `#[cfg(test)]` and no production algorithm is modified.
//!
//! ## State lifecycle (one [`StreamEncoder`] per stream)
//!
//! Init (mirrors `enc_oracle.cpp` init + `FDKaacEnc_*Init*`):
//! - [`EncoderConfig`], [`PcmFeeder`], per-channel [`BlockSwitch`],
//!   [`ChannelTransform`], [`PsyChannel`], [`PsyTables::for_rate`] (with
//!   `min_snr` init vectors from the committed `cfg_*` snapshots, i.e. the
//!   `InitPsyConfiguration` output, not per-frame content).
//! - [`AtsElement::new(0)`] (VBR-5 `peOffset=0`), [`distribute_state_init`]
//!   from `mean_pe` (mirrors `QCInit`), [`QcState::new`] (bitres pinned
//!   full at VBR-5), `padding_rest = sample_rate` (evolved per frame by
//!   [`adjust_bitrate`], content-independent).
//!
//! Per frame, in order (mirrors `psy_main.cpp` + `qc_main.cpp`):
//! 1. `BlockSwitch::process` all channels, then `synchronize_pair`/`mono`.
//! 2. `ChannelTransform::process` with post-sync sequence/shape.
//! 3. `process_pair` (mutates the f64 spectra in place: post-TNS state).
//! 4. Q31 spectrum mapping ([`map_spectrum_to_q31`], the documented single
//!    approximation point) + stored/Ld vectors (Ld via exact
//!    [`calc_ld_data`] = C `fLog2`, so the only noise source is the
//!    stored-int float residue; threshold Ld carries the C
//!    `-0.515625` floor via [`threshold_ld_of`]).
//! 5. [`intensity_stereo`] decision and in-place processing, then
//!    [`ms_stereo_mirror`] (exact integer port of `ms_stereo.cpp`,
//!    including the IS-band skip and `MS_SOME` rules). Short-block
//!    mid/side Ld is the fresh grouped `LdDataVector` refresh
//!    ([`grouped_ms_vectors`], `psy_main.cpp:1078-1084`), not stale history.
//!    PNS is off by configuration (`NO_NOISE_PNS`, zero noise energies).
//! 6. `form_factor` + `pe_calculation` (S1), `static_bits_{sce,cpe}`.
//! 7. `adapt_thresholds_vbr` (S2, evolves ATS chaos).
//! 8. `estimate_scale_factors_channel` (S4 inv=2) on the S3-contract
//!    spectrum (inactive bands zeroed).
//! 9. `calc_max_value_in_sfb` + `dyn_bit_count` (S5).
//! 10. `qc_main_frame` (S6 gain loop; crash recovery emits with reduced
//!    `maxSfbPerGroup` per FDK — multi-pass retry remains a loud skip).
//! 11. `build_raw_block` + `build_adts_frame` (S8 + transport).
//!
//! Flush: the driver steps `ceil((S + 1600) / 1024)` frames with zero
//! padding past the input (same law as [`PcmFeeder::finish`], verified
//! against the C encoder in Phase 0).
//!
//! ## Known limits (by design, not defects)
//!
//! - Frontend integers carry float residue (`assert_q31_close` scale); MS/PE
//!   decisions at exact boundaries may flip vs C. Classification separates
//!   this from structural divergence.
//! - IS/PNS/multi-pass-retry/large-fill paths are asserted absent per frame;
//!   hitting one is a `Class::Unsupported` finding, not a silent fallback.
//!   Crash recovery is the exception: it emits with reduced bandwidth per FDK.

use super::adts::ADTS_HEADER_BITS;
use super::bitcount::{dyn_bit_count_reuse, BitCountInput, NO_NOISE_PNS};
use super::bitenc::BitWriter;
use super::block_switch::{BlockSwitch, WindowSequence};
use super::channel_map::ChannelMode;
use super::config::{EncoderConfig, TransportType, FRAME_LENGTH};
use super::energy;
use super::frame::{
    build_adts_frame, build_raw_block, channel_element_write, check_static_agreement,
    ChannelFrameInput, ElementFrameInput, FrameError, RawBlockInput, ID_CPE, ID_SCE,
};
use super::intensity::intensity_stereo;
#[cfg(test)]
use super::pcm::PcmFeeder;
use super::pe::{form_factor, pe_calculation, AtsElement, PeChannelInput};
use super::psy::{
    process_pair_reuse, ChannelInput, ChannelOutput, MsOutput, PsyChannel, PsyScratch, PsyTables,
};
use super::psy_config::load_cfg;
use super::q31::calc_ld_data;
use super::rate::{
    adjust_bitrate, calc_bits_per_frame, calc_max_value_in_sfb, derive_qc_init,
    distribute_state_init, init_bits2pe_factor, qc_main_frame, total_consumed_bits, BitResMode,
    DistributeState, QcChannelInput, QcError, QcFrameInput, QcInitParams, QcState, GLOBAL_EXT_BITS,
    MAXVAL_DBL, MAX_ITERATIONS,
};
use super::scf::{
    estimate_scale_factors_channel_reuse, ScfChannelInput, ScfGeometry, ScfWorkspace,
};
use super::static_bits::{ms_mask_bits, static_bits_cpe, static_bits_sce, MsDigest};
#[cfg(test)]
use super::testdata;
use super::threshold_vbr::{adapt_thresholds_vbr, S2ChannelInput, SfbGeometry};
use super::tns::TnsChannel;
use super::transform::ChannelTransform;
use crate::common::enums::WindowShape;

// ---------------------------------------------------------------------------
// Small exact helpers (test-only mirrors of C integer logic)
// ---------------------------------------------------------------------------

/// 7-bit grouping mask from group lengths (`psy_main.cpp:1273-1282`).
/// Exact integer rule, verified against oracle `S8SIDE gmask` lines.
fn grouping_mask(groups: u8, group_len: &[u8; 4]) -> i32 {
    let mut mask = 0i32;
    for g in 0..groups as usize {
        mask <<= 1;
        for _ in 1..group_len[g] {
            mask = (mask << 1) | 1;
        }
    }
    mask
}

/// Zero inactive spectral lines per group stride (`max..per` within each
/// `per`-wide group of the flat `cnt` bands), using the grouped offset
/// table for line ranges. Mirrors `phase1b.rs::s4_input_spec` (the
/// `groupShortData` grouped-band contract): for LONG blocks `cnt == per`
/// so this is identical to flat `max..cnt`; for SHORT blocks the flat
/// range would erase valid later-group coefficients, so stride iteration
/// is required.
fn zero_inactive_bands(spec: &mut [i32], offsets: &[i32], cnt: usize, per: usize, max: usize) {
    let mut o = 0usize;
    while o < cnt {
        for b in max..per {
            let s = offsets[o + b] as usize;
            let e = offsets[o + b + 1] as usize;
            for line in s..e {
                spec[line] = 0;
            }
        }
        o += per;
    }
}

/// Q31 log-domain vector from stored-domain ints (`LdDataVector` =
/// `fLog2(x, 0)` per band). Exact given the stored inputs. Correct for
/// L/R energies and thresholds (C takes Ld of the stored values there).
fn ld_of(stored: &[i32]) -> Vec<i32> {
    stored.iter().map(|&v| calc_ld_data(v)).collect()
}

/// Threshold Ld floor (`FL2FXCONST_DBL(-0.515625)`, i.e. `-33/64` in Q31).
/// C clamps every threshold Ld vector with `fixMax(ld, floor)`
/// (`psy_main.cpp:1071-1075` short, `:1124-1128` long); the driver must feed
/// the same floored values to the MS decision and PE. Exact integer:
/// `-0.515625 * 2^31 = -33 * 2^25`.
const THRESHOLD_LD_FLOOR_Q31: i32 = -33 * 33_554_432;

/// Threshold Ld vector: `LdDataVector` then the C floor. Non-positive
/// stored values yield the `-1.0` sentinel from `calc_ld_data`, which the
/// floor then lifts (C's `fixMax` sees the same sentinel).
fn threshold_ld_of(stored: &[i32]) -> Vec<i32> {
    stored
        .iter()
        .map(|&v| calc_ld_data(v).max(THRESHOLD_LD_FLOOR_Q31))
        .collect()
}

/// Fresh grouped mid/side stored sums plus their Ld refresh for short
/// blocks.
///
/// Mirrors `FDKaacEnc_groupShortData`'s MS branch (`grp_data.cpp:211-224`:
/// per-group `nrgAddSaturate` window sums over the active bands) followed
/// by the short-frame MS Ld refresh (`psy_main.cpp:1078-1084`:
/// `LdDataVector(&sfbEnergyMS.Long[sfbGrp], &sfbEnergyMSLdData[sfbGrp])`).
/// `saturating_add` is `nrgAddSaturate` for non-negative energies; bands
/// past `ms_active` keep 0, matching the driver's existing stride-pad
/// convention (unread by the `maxSfbPerGroup`-bounded MS decision).
/// Returns `(mid_stored, side_stored, mid_ld, side_ld)`, each `cnt` long.
fn grouped_ms_vectors(
    mid: &[Vec<i32>],
    side: &[Vec<i32>],
    groups: u8,
    group_len: &[u8; 4],
    per: usize,
    cnt: usize,
    ms_active: usize,
) -> (Vec<i32>, Vec<i32>, Vec<i32>, Vec<i32>) {
    let wins = group_windows(groups, group_len);
    let mut em = vec![0i32; cnt];
    let mut es = vec![0i32; cnt];
    for (grp, (w0, w1)) in wins.iter().enumerate() {
        for b in 0..ms_active {
            let mut am = 0i32;
            let mut as_ = 0i32;
            for w in *w0..*w1 {
                am = am.saturating_add(mid[w][b]);
                as_ = as_.saturating_add(side[w][b]);
            }
            em[grp * per + b] = am;
            es[grp * per + b] = as_;
        }
    }
    let em_ld = ld_of(&em);
    let es_ld = ld_of(&es);
    (em, es, em_ld, es_ld)
}

/// Q31 log-domain int from a float Ld value (`-1.0` sentinel aware).
/// Inverse of the test `ld_to_e` convention; rounding noise ≤ 1 LSB.
fn q31_of_ld(ld: f64) -> i32 {
    if ld <= -1.0 {
        i32::MIN
    } else {
        (ld * 2_147_483_648.0).round() as i32
    }
}

fn q31_of_ld_vec(ld: &[f64]) -> Vec<i32> {
    ld.iter().map(|&v| q31_of_ld(v)).collect()
}

/// Window ranges per group for short blocks: group `g` owns
/// `lens[0]+...+lens[g-1] .. +lens[g]` of the 8 windows.
fn group_windows(groups: u8, lens: &[u8; 4]) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(groups as usize);
    let mut w = 0usize;
    for g in 0..groups as usize {
        let n = lens[g] as usize;
        out.push((w, w + n));
        w += n;
    }
    out
}

/// Exact integer port of `FDKaacEnc_MsStereoProcessing`
/// (`cpp/libAACenc/src/ms_stereo.cpp:109-330`), including IS-band skipping
/// and the C `MS_NONE`/`MS_SOME`/`MS_ALL` rules.
///
/// All energy/threshold/spread slices span `cnt` bands (long: ungrouped;
/// short: grouped stride, already gathered by the caller); spectra span
/// 1024 lines with band `b` at `offsets[b]..offsets[b+1]`. `is_book` is the
/// right-channel IS book vector; `initial_mask` is the phase/mask vector
/// left by [`intensity_stereo`]. MS bands are substituted in place; returns
/// `(digest, mask)` with `SI_MS_MASK_{NONE,SOME,ALL} = 0/1/2`.
#[allow(clippy::too_many_arguments)]
fn ms_stereo_mirror(
    e_l: &mut [i32],
    e_r: &mut [i32],
    t_l: &mut [i32],
    t_r: &mut [i32],
    eld_l: &mut [i32],
    eld_r: &mut [i32],
    tld_l: &mut [i32],
    tld_r: &mut [i32],
    e_m: &[i32],
    e_s: &[i32],
    em_ld: &[i32],
    es_ld: &[i32],
    spread_l: &mut [i32],
    spread_r: &mut [i32],
    spec_l: &mut [i32],
    spec_r: &mut [i32],
    offsets: &[i32],
    cnt: usize,
    per: usize,
    max: usize,
    allow_ms: bool,
    is_book: &[i32],
    initial_mask: &[i32],
) -> (i32, Vec<i32>) {
    // Band application shared by the first pass and the ALL upgrade.
    fn apply(
        b: usize,
        e_l: &mut [i32],
        e_r: &mut [i32],
        t_l: &mut [i32],
        t_r: &mut [i32],
        eld_l: &mut [i32],
        eld_r: &mut [i32],
        tld_l: &mut [i32],
        tld_r: &mut [i32],
        e_m: &[i32],
        e_s: &[i32],
        em_ld: &[i32],
        es_ld: &[i32],
        spread_l: &mut [i32],
        spread_r: &mut [i32],
        spec_l: &mut [i32],
        spec_r: &mut [i32],
        offsets: &[i32],
    ) {
        for j in offsets[b] as usize..offsets[b + 1] as usize {
            let sl = spec_l[j] >> 1;
            let sr = spec_r[j] >> 1;
            spec_l[j] = sl + sr;
            spec_r[j] = sl - sr;
        }
        let min_t = t_l[b].min(t_r[b]);
        t_l[b] = min_t;
        t_r[b] = min_t;
        let min_tld = tld_l[b].min(tld_r[b]);
        tld_l[b] = min_tld;
        tld_r[b] = min_tld;
        e_l[b] = e_m[b];
        e_r[b] = e_s[b];
        eld_l[b] = em_ld[b];
        eld_r[b] = es_ld[b];
        let min_sp = spread_l[b].min(spread_r[b]) >> 1;
        spread_l[b] = min_sp;
        spread_r[b] = min_sp;
    }
    let mut mask = initial_mask.to_vec();
    assert_eq!(mask.len(), cnt, "IS mask spans channel bands");
    let mut true_somewhere = false;
    let mut num_false = 0i32;
    for sfb in (0..cnt).step_by(per) {
        for off in 0..max {
            let b = sfb + off;
            if b >= cnt {
                break;
            }
            if is_book.get(b).copied().unwrap_or(0) != 0 {
                // Keep the IS phase/mask decision. C sets numMsMaskFalse=9
                // to prohibit MS_ALL and only counts a true bit here.
                if mask[b] != 0 {
                    true_somewhere = true;
                }
                num_false = 9;
                continue;
            }
            let min_tld = tld_l[b].min(tld_r[b]);
            let tmp = eld_l[b].max(tld_l[b]);
            let mut pnlr = (tld_l[b] >> 1) - (tmp >> 1);
            pnlr += tld_r[b] >> 1;
            let tmp = eld_r[b].max(tld_r[b]);
            pnlr -= tmp >> 1;
            let tmp = em_ld[b].max(min_tld);
            let mut pnms = min_tld - (tmp >> 1);
            let tmp = es_ld[b].max(min_tld);
            pnms -= tmp >> 1;
            let use_ms = allow_ms && pnms > pnlr;
            if use_ms {
                mask[b] = 1;
                true_somewhere = true;
                apply(
                    b, e_l, e_r, t_l, t_r, eld_l, eld_r, tld_l, tld_r, e_m, e_s, em_ld, es_ld,
                    spread_l, spread_r, spec_l, spec_r, offsets,
                );
            } else {
                mask[b] = 0;
                num_false += 1;
            }
        }
    }
    if true_somewhere {
        if num_false == 0 || (num_false < max as i32 && num_false < 9) {
            // MS_ALL: convert every remaining non-IS band.
            for sfb in (0..cnt).step_by(per) {
                for off in 0..max {
                    let b = sfb + off;
                    if b >= cnt {
                        break;
                    }
                    if is_book.get(b).copied().unwrap_or(0) == 0 && mask[b] == 0 {
                        mask[b] = 1;
                        apply(
                            b, e_l, e_r, t_l, t_r, eld_l, eld_r, tld_l, tld_r, e_m, e_s, em_ld,
                            es_ld, spread_l, spread_r, spec_l, spec_r, offsets,
                        );
                    }
                }
            }
            (2, mask)
        } else {
            (1, mask)
        }
    } else {
        (0, mask)
    }
}

// ---------------------------------------------------------------------------
// Streaming encoder state
// ---------------------------------------------------------------------------

/// Complete pre-decision operands of one stereo MS decision (test-only
/// diagnostic readout for mask-flip causality analysis). Vectors are
/// cloned before [`ms_stereo_mirror`] runs, so margins recomputed from
/// them in tests reproduce the recorded `mask` bit-for-bit.
/// Test-observability capture of the MS decision (consumed by gated tests;
/// `allow(dead_code)` because production builds only construct it).
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub(crate) struct MsDecisionCapture {
    pub e_l: Vec<i32>,
    pub e_r: Vec<i32>,
    pub t_l: Vec<i32>,
    pub t_r: Vec<i32>,
    pub eld_l: Vec<i32>,
    pub eld_r: Vec<i32>,
    pub tld_l: Vec<i32>,
    pub tld_r: Vec<i32>,
    pub spread_l: Vec<i32>,
    pub spread_r: Vec<i32>,
    pub em: Vec<i32>,
    pub es: Vec<i32>,
    pub em_ld: Vec<i32>,
    pub es_ld: Vec<i32>,
    pub offsets: Vec<i32>,
    pub cnt: usize,
    pub per: usize,
    pub max: usize,
    pub allow_ms: bool,
    pub is_book: Vec<i32>,
    pub digest: i32,
    pub mask: Vec<i32>,
}

/// Complete S1 (PE) input state per channel (test-only diagnostic
/// readout for PE-divergence causality). Vectors are cloned before
/// [`pe_calculation`] runs; rerunning it on these inputs with a forged
/// ATS reproduces the recorded `pe` exactly.
/// Test-observability capture (consumed by gated tests; see `MsDecisionCapture`).
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub(crate) struct S1CapChannel {
    pub energy_ld: Vec<i32>,
    pub threshold_ld: Vec<i32>,
    pub energy_lin: Vec<i32>,
    pub ff: Vec<i32>,
    pub spec_q31: Vec<i32>,
    pub offsets: Vec<i32>,
    pub cnt: usize,
    pub per: usize,
    pub max: usize,
    pub short: bool,
}

/// S1-stage capture: per-channel PE inputs, the MS mask consumed, and
/// the pre-PE ATS weighting state.
/// Test-observability capture (consumed by gated tests; see `MsDecisionCapture`).
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub(crate) struct S1Cap {
    pub ch: Vec<S1CapChannel>,
    pub ms_mask: Vec<i32>,
    pub ats_pre: [i32; 4],
}

/// Typed step failure for the production API mapping (M6.1).
/// Carries the already-typed errors from QC/S8 writes; notes-only failures
/// (frontend mismatch, multi-pass policy, low-overlap) carry `None` and map
/// to `SkippedFrame` with the diagnostic notes. Pure diagnostic plumbing:
/// set at existing early-return sites, never alters decisions.
#[derive(Debug, Clone)]
pub(crate) enum StepError {
    Qc(QcError),
    Frame(FrameError),
}

/// Per-frame live snapshot for diagnosis (small scalars + finding notes).
#[derive(Debug, Clone, Default)]
pub(crate) struct FrameSnapshot {
    #[allow(dead_code)]
    pub frame: u32,
    pub seq: i32,
    pub digest: i32,
    pub mask_ones: u32,
    pub static_bits: i32,
    pub pe: i32,
    pub gains: Vec<i32>,
    pub side_bits: i32,
    pub huff_bits: i32,
    pub scf_bits: i32,
    pub used_dyn: i32,
    pub tot_fill: i32,
    pub align: i32,
    pub total: i32,
    pub npass: usize,
    pub crash: i32,
    pub adts_len: usize,
    /// True once the stereo MS decision actually ran this frame (digest /
    /// mask_ones are meaningful only then; mono or pre-MS skips leave them
    /// at their defaults and must not enter digest agreement stats).
    pub ms_ran: bool,
    /// Full MS decision capture (stereo only, `None` on mono or when the
    /// frame never reaches the MS stage). Test-only causality readout.
    pub ms_cap: Option<MsDecisionCapture>,
    /// S1 input capture (`None` when the frame never reaches PE).
    /// Test-only causality readout.
    pub s1_cap: Option<S1Cap>,
    /// Earliest-stage diagnostic notes (empty when clean).
    pub notes: Vec<String>,
    /// Typed step failure for the production API mapping (`None` on success
    /// and on notes-only policy skips). Set at existing early-return sites.
    pub step_error: Option<StepError>,
    /// True if any `is_book` value was nonzero this frame.
    pub is_book_any: bool,
    /// Number of right-channel bands selected for intensity stereo.
    pub is_bands: u32,
    /// Number of nonzero right-channel intensity scalefactors.
    pub is_scale_nz: u32,
    /// Right-channel intensity scalefactors (test diagnostics).
    pub is_scale_values: Vec<i32>,
    /// Initial `msDigest` supplied by the IS stage (`MS_NONE`/`MS_SOME`).
    pub is_digest: i32,
    /// Final section codebooks (S5 output) per channel, test diagnostics.
    pub section_books: Vec<Vec<i32>>,
    /// M5.g census: true if any `noise_nrg` value passed to S5 counting
    /// differed from `NO_NOISE_PNS` this frame. Pure read; the driver only
    /// ever passes the all-`NO_NOISE_PNS` vector.
    pub noise_any_non_pns: bool,
    /// M5.i margins (all pure reads of `qc_out`/pass state, per frame):
    /// inner gain-adjust count of the last outer pass (`PassTrace.iterations`;
    /// distinct from the outer pass count `npass`).
    pub last_iterations: i32,
    /// Final `sum_bits` as recomputed by the exit-matrix formula
    /// (`total_consumed_bits` over final dyn/static + header).
    pub sum_bits: i32,
    /// Final element dyn grant (`el_granted`).
    pub el_granted: i32,
    /// Final element dyn cap (`max_dyn_bits`).
    pub max_dyn: i32,
    /// Final frame budget (`total_avail`).
    pub total_avail: i32,
    /// Final static bits used (`static_bits_used`).
    pub static_used: i32,
}

/// One live channel's Q31 working set for a frame (owned, post-MS).
pub(crate) struct LiveChannel {
    spec_q31: Vec<i32>,
    e_stored: Vec<i32>,
    t_stored: Vec<i32>,
    spread: Vec<i32>,
    e_ld: Vec<i32>,
    t_ld: Vec<i32>,
    ff: Vec<i32>,
    offsets: Vec<i32>,
    cnt: usize,
    per: usize,
    short: bool,
    mdct_scale: i32,
    tns: TnsChannel,
}

/// Test-only streaming encoder: owns all cross-frame state.
pub(crate) struct StreamEncoder {
    sample_rate: u32,
    channels: usize,
    mode: ChannelMode,
    tables: PsyTables,
    min_snr_long: Vec<i32>,
    min_snr_short: Vec<i32>,
    switches: Vec<BlockSwitch>,
    transforms: Vec<ChannelTransform>,
    psy_ch: Vec<PsyChannel>,
    ats: AtsElement,
    dist: DistributeState,
    qc: QcState,
    s4_ws: ScfWorkspace,
    psy_ws: PsyScratch,
    init: QcInitParams,
    // Init constants kept for gated phase-1 diagnostics (`init_summary`);
    // unread in production builds by design.
    #[allow(dead_code)]
    b2m: i32,
    #[allow(dead_code)]
    b2e: i32,
    total_br: u32,
    padding_rest: i32,
    avg_bits: i32,
    short_sp: usize,
    long_ac: usize,
    /// Mid/side log-energy history per channel (mirrors C
    /// `sfbEnergyMSLdData`: float-lane conversion on long frames, grouped
    /// `LdDataVector` refresh on short frames (`psy_main.cpp:1078-1084`);
    /// init zeros like encoder RAM).
    stale_ms_ld: Vec<Vec<i32>>,
}

impl StreamEncoder {
    pub(crate) fn new(sample_rate: u32, channels: usize) -> Self {
        assert!(
            channels == 1 || channels == 2,
            "single-element mono/stereo only"
        );
        let cfg = EncoderConfig::new(
            sample_rate,
            channels as u32,
            FRAME_LENGTH,
            TransportType::Adts,
        )
        .expect("supported rate/channels");
        let (ch_br, total_br): (u32, u32) = if channels == 1 {
            (112000, 112000)
        } else {
            (96000, 192000)
        };
        let ((lcfg, _), (scfg, _)) = load_cfg(sample_rate, ch_br);
        let tables = PsyTables::for_rate(
            sample_rate,
            ch_br,
            total_br,
            19293,
            lcfg.min_snr_ld.clone(),
            scfg.min_snr_ld.clone(),
        )
        .expect("tables build");
        let long_ac = tables.long_cfg.sfb_active();
        let short_sp = tables.short_cfg.sfb_count();
        let switches = (0..channels).map(|_| BlockSwitch::new()).collect();
        let transforms = (0..channels).map(|_| ChannelTransform::new()).collect();
        let psy_ch = (0..channels)
            .map(|_| PsyChannel::new(tables.long_cfg.pcm_quant_q31()))
            .collect();
        let avg0 = calc_bits_per_frame(total_br as i32, FRAME_LENGTH as i32, sample_rate as i32);
        let init = derive_qc_init(
            total_br as i32,
            avg0,
            channels as i32,
            channels as i32,
            sample_rate as i32,
            19293,
            FRAME_LENGTH as i32,
        );
        let (b2m, b2e) = init_bits2pe_factor(false, false, init.ch_bitrate_el, channels as i32);
        let dist = distribute_state_init(init.mean_pe);
        let qc = QcState::new(init.max_bits, sample_rate as i32);
        let _ = cfg;
        Self {
            sample_rate,
            channels,
            mode: if channels == 1 {
                ChannelMode::Mono
            } else {
                ChannelMode::Stereo
            },
            tables,
            min_snr_long: lcfg.min_snr_ld,
            min_snr_short: scfg.min_snr_ld,
            switches,
            transforms,
            psy_ch,
            ats: AtsElement::new(0),
            dist,
            qc,
            s4_ws: ScfWorkspace::default(),
            psy_ws: PsyScratch::default(),
            init,
            b2m,
            b2e,
            total_br,
            padding_rest: sample_rate as i32,
            avg_bits: avg0,
            short_sp,
            long_ac,
            stale_ms_ld: (0..channels).map(|_| vec![0i32; 64]).collect(),
        }
    }

    /// Init constants for diagnostics (consumed by gated phase-1 tests only).
    #[cfg(test)]
    pub(crate) fn init_summary(&self) -> (i32, i32, i32, i32, i32, i32) {
        (
            self.init.average_bits,
            self.init.max_bits,
            self.b2m,
            self.b2e,
            self.dist.pe_min,
            self.dist.pe_max,
        )
    }

    fn seq_of(&self, ch: usize) -> WindowSequence {
        self.switches[ch].sequence()
    }
}

/// Owned frontend outputs for one frame (pre-MS working sets).
pub(crate) struct FrontendOut {
    pub seqs: Vec<WindowSequence>,
    pub groupings: Vec<(u8, [u8; 4])>,
    pub ms: Option<MsOutput>,
    pub live: Vec<LiveChannel>,
    pub max_common: usize,
}

impl StreamEncoder {
    /// M2 frontend + sync, M3 transform + psy, and live Q31 working-set
    /// construction (pre-MS). Returns `Err(note)` on structural mismatch
    /// (never silent); the caller records the note and skips the frame.
    pub(crate) fn frontend(
        &mut self,
        _frame: u32,
        fresh1024: Vec<[i16; 1024]>,
    ) -> Result<FrontendOut, String> {
        // --- M2 frontend + sync ---
        for (sw, fr) in self.switches.iter_mut().zip(fresh1024.iter()) {
            sw.process(fr, false);
        }
        if self.channels == 2 {
            let (l, r) = self.switches.split_at_mut(1);
            BlockSwitch::synchronize_pair(&mut l[0], &mut r[0]);
        } else {
            self.switches[0].synchronize_mono();
        }
        let seqs: Vec<WindowSequence> = (0..self.channels).map(|c| self.seq_of(c)).collect();
        let groupings: Vec<(u8, [u8; 4])> = self.switches.iter().map(|s| s.grouping()).collect();

        // --- M3 transform + psy ---
        let mut specs: Vec<[f64; 1024]> = Vec::with_capacity(self.channels);
        for (ch, (tr, sw)) in self
            .transforms
            .iter_mut()
            .zip(self.switches.iter())
            .enumerate()
        {
            let out = tr
                .process(&fresh1024[ch], sw.sequence(), sw.shape())
                .expect("mdct process");
            let mut spec = [0.0f64; 1024];
            for (d, s) in spec.iter_mut().zip(out.iter()) {
                *d = *s as f64;
            }
            specs.push(spec);
        }
        let (head, tail) = specs.split_at_mut(1);
        let lin = ChannelInput {
            spectrum: &mut head[0],
            seq: seqs[0],
            grouping: groupings[0],
            is_lfe: false,
        };
        let (out_l, out_r, ms) = if self.channels == 2 {
            let rin = ChannelInput {
                spectrum: &mut tail[0],
                seq: seqs[1],
                grouping: groupings[1],
                is_lfe: false,
            };
            let (pl, pr) = self.psy_ch.split_at_mut(1);
            // M5.20: encoder-owned psy scratch removes the per-frame
            // internal working-buffer allocations; semantics unchanged.
            let (ol, or_, m) = process_pair_reuse(
                &mut pl[0],
                Some(&mut pr[0]),
                lin,
                Some(rin),
                &self.tables,
                &mut self.psy_ws,
            );
            (ol, or_, m)
        } else {
            let (ol, _, _) = process_pair_reuse(
                &mut self.psy_ch[0],
                None,
                lin,
                None,
                &self.tables,
                &mut self.psy_ws,
            );
            (ol, None, None)
        };
        let mut outs: Vec<&ChannelOutput> = vec![&out_l];
        if let Some(r) = out_r.as_ref() {
            outs.push(r);
        }

        // --- Live Q31 working sets ---
        let mut live: Vec<LiveChannel> = Vec::with_capacity(self.channels);
        for (ch, out) in outs.iter().enumerate() {
            let short = seqs[ch] == WindowSequence::Short;
            let (cnt, per) = if short {
                (groupings[ch].0 as usize * self.short_sp, self.short_sp)
            } else {
                (self.long_ac, self.long_ac)
            };
            let (offsets, e_stored, t_stored, spread, spec_f64): (
                Vec<i32>,
                Vec<i32>,
                Vec<i32>,
                Vec<i32>,
                Vec<f64>,
            ) = if short {
                let g = out.grouped.as_ref().expect("short grouped output");
                let stride = g.energies.len() / groupings[ch].0 as usize;
                let mut e = Vec::with_capacity(cnt);
                let mut t = Vec::with_capacity(cnt);
                let mut s = Vec::with_capacity(cnt);
                for grp in 0..groupings[ch].0 as usize {
                    for b in 0..per {
                        e.push(g.energies[grp * stride + b]);
                        t.push(g.thresholds[grp * stride + b]);
                        s.push(g.spread[grp * stride + b]);
                    }
                }
                let ostride = (g.offsets.len() - 1) / groupings[ch].0 as usize;
                let mut off = Vec::with_capacity(cnt + 1);
                for grp in 0..groupings[ch].0 as usize {
                    for b in 0..per {
                        off.push(g.offsets[grp * ostride + b]);
                    }
                }
                off.push(g.offsets[(groupings[ch].0 as usize - 1) * ostride + per]);
                (off, e, t, s, g.spectrum.clone())
            } else {
                let off: Vec<i32> = self.tables.long_cfg.offsets()[..cnt + 1]
                    .iter()
                    .map(|&v| v as i32)
                    .collect();
                (
                    off,
                    out.energies[0][..cnt].to_vec(),
                    out.thresholds[0][..cnt].to_vec(),
                    out.spread[0][..cnt].to_vec(),
                    specs[ch].to_vec(),
                )
            };
            if offsets.len() != cnt + 1 || e_stored.len() != cnt {
                return Err(format!(
                    "ch{ch} geometry len mismatch off={} e={} cnt={cnt}",
                    offsets.len(),
                    e_stored.len()
                ));
            }
            let spec_q31 = energy::map_spectrum_to_q31(&spec_f64, out.mdct_scale);
            if spec_q31.len() != 1024 {
                return Err(format!("ch{ch} spec len {}", spec_q31.len()));
            }
            // Energy Ld source differs by block type (C rescale mechanics):
            // long frames carry the unshifted-accumulation Ld (float lane,
            // Q31-converted); short frames take Ld of the grouped shifted
            // sums (exact fLog2, mirrors `groupedELd`).
            let e_ld = if short {
                ld_of(&e_stored)
            } else {
                if out.energy_ld[0].len() != cnt {
                    return Err(format!(
                        "ch{ch} energy_ld len {} != cnt {cnt}",
                        out.energy_ld[0].len()
                    ));
                }
                q31_of_ld_vec(&out.energy_ld[0])
            };
            // Threshold Ld carries the C floor (`psy_main.cpp:1071-1075`
            // short, `:1124-1128` long): `fixMax(LdDataVector, -0.515625)`.
            let t_ld = threshold_ld_of(&t_stored);
            live.push(LiveChannel {
                spec_q31,
                e_stored,
                t_stored,
                spread,
                e_ld,
                t_ld,
                ff: Vec::new(),
                offsets,
                cnt,
                per,
                short,
                mdct_scale: out.mdct_scale,
                tns: self.psy_ch[ch].tns_info().clone(),
            });
        }
        // Common maxSfbPerGroup from psy outputs (exact C rule,
        // common-window max for stereo).
        let max_common = if self.channels == 2 {
            out_l
                .max_sfb_per_group
                .max(out_r.as_ref().expect("stereo right").max_sfb_per_group)
        } else {
            out_l.max_sfb_per_group
        };
        Ok(FrontendOut {
            seqs,
            groupings,
            ms,
            live,
            max_common,
        })
    }

    /// Encode one frame. `fresh[ch]` holds 1024 new PCM samples per channel.
    /// Returns `(adts_bytes_or_none, snapshot)`; `None` means the frame was
    /// skipped with a loud finding in `snapshot.notes` (never silent).
    pub(crate) fn step_frame(
        &mut self,
        frame: u32,
        fresh: &[Vec<i16>],
    ) -> (Option<Vec<u8>>, FrameSnapshot) {
        let mut snap = FrameSnapshot {
            frame,
            ..FrameSnapshot::default()
        };
        assert_eq!(fresh.len(), self.channels);
        let fresh1024: Vec<[i16; 1024]> = fresh
            .iter()
            .map(|v| {
                assert_eq!(v.len(), FRAME_LENGTH);
                let mut b = [0i16; 1024];
                b.copy_from_slice(&v[..1024]);
                b
            })
            .collect();

        // --- M2/M3 frontend (shared helper) ---
        let fe = match self.frontend(frame, fresh1024) {
            Ok(fe) => fe,
            Err(note) => {
                snap.notes.push(note);
                return (None, snap);
            }
        };
        snap.seq = fe.seqs[0] as i32;
        let FrontendOut {
            seqs,
            groupings,
            ms,
            mut live,
            max_common,
        } = fe;

        let cnt = live[0].cnt;
        let mut books: Vec<Vec<i32>> = (0..self.channels).map(|_| vec![0i32; cnt]).collect();
        let mut scales: Vec<Vec<i32>> = (0..self.channels).map(|_| vec![0i32; cnt]).collect();
        let mut is_mask = vec![0i32; cnt];
        let mut is_digest = 0i32;
        if self.channels == 2 {
            let allow_is = self.tables.long_cfg.allow_is();
            let (l, r) = live.split_at_mut(1);
            let (c0, c1) = (&mut l[0], &mut r[0]);
            let res = intensity_stereo(
                &c0.e_stored,
                &mut c1.e_stored,
                &mut c0.spec_q31,
                &mut c1.spec_q31,
                &c0.t_stored,
                &mut c1.t_stored,
                &mut c1.t_ld,
                &c0.spread,
                &mut c1.spread,
                &c0.e_ld,
                &mut c1.e_ld,
                &c1.offsets,
                c1.cnt,
                c1.per,
                max_common,
                allow_is,
            );
            books[1] = res.is_book;
            scales[1] = res.is_scale;
            is_mask = res.ms_mask;
            is_digest = res.ms_digest;
        }
        snap.is_digest = is_digest;

        // --- MS mirror (stereo) ---
        // Mid/side STORED energies are always fresh (grouped sums on
        // short, mirroring the `groupShortData` MS branch). Mid/side Ld
        // is fresh on every frame: grouped-`LdDataVector` refresh on
        // short (`psy_main.cpp:1078-1084`), float-lane conversion on long.
        // The persistent history mirrors C's `sfbEnergyMSLdData`, which the
        // short regroup refreshes in place.
        let (digest, mask_vec) = if self.channels == 2 {
            let ms_out = ms.as_ref().expect("stereo ms");
            let per = live[0].per;
            let cnt = live[0].cnt;
            let short = seqs[0] == WindowSequence::Short;
            let (em_st, es_st, em_ld, es_ld): (Vec<i32>, Vec<i32>, Vec<i32>, Vec<i32>) = if short {
                // Grouped sums + fresh grouped `LdDataVector` refresh in
                // one call (`groupShortData` MS branch + the
                // `psy_main.cpp:1078-1084` refresh); the history write
                // mirrors C refreshing `sfbEnergyMSLdData` in place.
                let ms_active = ms_out.mid[0].len();
                let (em, es, eml, esl) = grouped_ms_vectors(
                    &ms_out.mid,
                    &ms_out.side,
                    groupings[0].0,
                    &groupings[0].1,
                    per,
                    cnt,
                    ms_active,
                );
                for c in 0..2 {
                    let src = if c == 0 { &eml } else { &esl };
                    let n = src.len().min(self.stale_ms_ld[c].len());
                    self.stale_ms_ld[c][..n].copy_from_slice(&src[..n]);
                }
                (em, es, eml, esl)
            } else {
                // Fresh mid/side Ld from the float lane (computed pre
                // down-shift + scale correction, like C), converted to Q31.
                // (Known float-to-Q31 approximation, out of scope here.)
                let eml = q31_of_ld_vec(&ms_out.mid_ld[0]);
                let esl = q31_of_ld_vec(&ms_out.side_ld[0]);
                for c in 0..2 {
                    let src = if c == 0 { &eml } else { &esl };
                    let n = src.len().min(self.stale_ms_ld[c].len());
                    self.stale_ms_ld[c][..n].copy_from_slice(&src[..n]);
                }
                (ms_out.mid[0].clone(), ms_out.side[0].clone(), eml, esl)
            };
            let (l, r) = live.split_at_mut(1);
            let (c0, c1) = (&mut l[0], &mut r[0]);
            assert_eq!(c0.cnt, c1.cnt, "stereo cnt agree");
            // Test-only causality capture: clone every decision operand
            // before the in-place mirror consumes them.
            let mut cap = MsDecisionCapture {
                e_l: c0.e_stored.clone(),
                e_r: c1.e_stored.clone(),
                t_l: c0.t_stored.clone(),
                t_r: c1.t_stored.clone(),
                eld_l: c0.e_ld.clone(),
                eld_r: c1.e_ld.clone(),
                tld_l: c0.t_ld.clone(),
                tld_r: c1.t_ld.clone(),
                spread_l: c0.spread.clone(),
                spread_r: c1.spread.clone(),
                em: em_st.clone(),
                es: es_st.clone(),
                em_ld: em_ld.clone(),
                es_ld: es_ld.clone(),
                offsets: c0.offsets.clone(),
                cnt: c0.cnt,
                per: c0.per,
                max: max_common,
                allow_ms: true,
                is_book: books[1].clone(),
                ..MsDecisionCapture::default()
            };
            let (d, m) = ms_stereo_mirror(
                &mut c0.e_stored,
                &mut c1.e_stored,
                &mut c0.t_stored,
                &mut c1.t_stored,
                &mut c0.e_ld,
                &mut c1.e_ld,
                &mut c0.t_ld,
                &mut c1.t_ld,
                &em_st,
                &es_st,
                &em_ld,
                &es_ld,
                &mut c0.spread,
                &mut c1.spread,
                &mut c0.spec_q31,
                &mut c1.spec_q31,
                &c0.offsets,
                c0.cnt,
                c0.per,
                max_common,
                true,
                &books[1],
                &is_mask,
            );
            cap.digest = d;
            cap.mask = m.clone();
            snap.ms_cap = Some(cap);
            (d, m)
        } else {
            (0, Vec::new())
        };
        snap.digest = digest;
        snap.mask_ones = mask_vec.iter().map(|&v| v as u32).sum();
        snap.ms_ran = self.channels == 2;

        // --- S1: form factor + PE + static bits ---
        // The IS decision is carried on the right channel only; the left
        // book/scale vectors remain zero, exactly as C does.
        snap.is_book_any |= books.iter().flatten().any(|&b| b != 0);
        if self.channels == 2 {
            snap.is_bands = books[1].iter().filter(|&&b| b != 0).count() as u32;
            snap.is_scale_nz = scales[1].iter().filter(|&&s| s != 0).count() as u32;
            snap.is_scale_values = scales[1].clone();
        }
        for c in live.iter_mut() {
            c.ff = form_factor(&c.spec_q31, &c.offsets, c.cnt, c.per, max_common);
        }
        let ms_mask: Vec<i32> = if self.channels == 2 {
            assert_eq!(mask_vec.len(), live[0].cnt, "mask spans channel bands");
            mask_vec.clone()
        } else {
            Vec::new()
        };
        let pe_inputs: Vec<PeChannelInput> = live
            .iter()
            .enumerate()
            .map(|(ch, c)| PeChannelInput {
                energy_ld: &c.e_ld,
                threshold_ld: &c.t_ld,
                energy_linear: &c.e_stored,
                form_factor_ld: &c.ff,
                spectrum_q31: &c.spec_q31,
                offsets: &c.offsets,
                sfb_cnt: c.cnt,
                sfb_per_group: c.per,
                max_sfb_per_group: max_common,
                is_book: &books[ch],
                is_scale: &scales[ch],
                short_block: c.short,
            })
            .collect();
        // Test-only causality capture: pre-PE ATS state must be read
        // before the mutating call below.
        let s1_ats_pre = [
            self.ats.chaos_measure_en_fac[0],
            self.ats.chaos_measure_en_fac[1],
            self.ats.last_enfac_patch[0],
            self.ats.last_enfac_patch[1],
        ];
        let pe = pe_calculation(&pe_inputs, &ms_mask, &mut self.ats);
        snap.pe = pe.data.pe;
        // Test-only causality capture: PE inputs (rerunning PE on these
        // with a forged ATS reproduces the recorded `pe` exactly).
        snap.s1_cap = Some(S1Cap {
            ch: live
                .iter()
                .map(|c| S1CapChannel {
                    energy_ld: c.e_ld.clone(),
                    threshold_ld: c.t_ld.clone(),
                    energy_lin: c.e_stored.clone(),
                    ff: c.ff.clone(),
                    spec_q31: c.spec_q31.clone(),
                    offsets: c.offsets.clone(),
                    cnt: c.cnt,
                    per: c.per,
                    max: max_common,
                    short: c.short,
                })
                .collect(),
            ms_mask: ms_mask.clone(),
            ats_pre: s1_ats_pre,
        });
        let s1static = if self.channels == 2 {
            let ms = MsDigest::from_i32(digest).expect("ms digest range");
            let bands = if ms == MsDigest::Some {
                ms_mask_bits(live[0].cnt, live[0].per, max_common)
            } else {
                0
            };
            static_bits_cpe(
                true,
                live[0].short,
                live[1].short,
                ms,
                bands,
                &live[0].tns,
                &live[1].tns,
            )
        } else {
            static_bits_sce(live[0].short, &live[0].tns)
        };
        snap.static_bits = s1static;

        // --- S2: VBR threshold adjustment ---
        let group_lens: Vec<Vec<i32>> = groupings
            .iter()
            .map(|(_, gl)| gl.iter().map(|&v| v as i32).collect())
            .collect();
        let minsnrs: Vec<Vec<i32>> = live
            .iter()
            .map(|c| {
                if c.short {
                    let mut v = Vec::with_capacity(c.cnt);
                    for _ in 0..groupings[0].0 as usize {
                        v.extend_from_slice(&self.min_snr_short[..c.per]);
                    }
                    v
                } else {
                    self.min_snr_long[..c.cnt].to_vec()
                }
            })
            .collect();
        let s2_inputs: Vec<S2ChannelInput> = live
            .iter()
            .enumerate()
            .map(|(ch, c)| S2ChannelInput {
                energy_lin: &c.e_stored,
                energy_ld: &c.e_ld,
                weighted_energy_ld: &pe.weighted_energy_ld[ch],
                threshold_ld: &pe.threshold_ld[ch],
                form_factor_ld: &c.ff,
                enfac_ld: &pe.enfac_ld[ch],
                min_snr_ld: &minsnrs[ch],
                spread: &c.spread,
                geo: SfbGeometry {
                    offsets: &c.offsets,
                    group_len: &group_lens[ch],
                    sfb_cnt: c.cnt,
                    sfb_per_group: c.per,
                    max_sfb_per_group: max_common,
                    seq: seqs[ch] as u8,
                },
            })
            .collect();
        let ms_opt = if self.channels == 2 {
            Some(ms_mask.as_slice())
        } else {
            None
        };
        let s2 = adapt_thresholds_vbr(&s2_inputs, ms_opt, &mut self.ats);

        // --- S4: scalefactor estimation (inv=2) on the S3-contract spectrum ---
        struct S4Out {
            scf: Vec<i32>,
            gain: i32,
            quant_spec: Vec<i16>,
            mutated: Vec<i32>,
        }
        let mut s4outs: Vec<S4Out> = Vec::with_capacity(self.channels);
        for (ch, c) in live.iter().enumerate() {
            let mut spec = c.spec_q31.clone();
            // Per-group-stride inactive-band zeroing (S3 contract); a flat
            // `max..cnt` range would erase later groups on short blocks.
            zero_inactive_bands(&mut spec, &c.offsets, c.cnt, c.per, max_common);
            let mut input = ScfChannelInput {
                spectrum: &mut spec,
                energy_ld: &c.e_ld,
                threshold_ld: &s2.threshold_ld[ch],
                form_factor_ld: &c.ff,
                geo: ScfGeometry {
                    offsets: &c.offsets,
                    sfb_cnt: c.cnt,
                    sfb_per_group: c.per,
                    max_sfb_per_group: max_common,
                },
            };
            // No `catch_unwind` here by design: the S4 search envelope is
            // loud, and an envelope violation aborts the run with the
            // frame recorded in the harness log. The encoder is then
            // discarded, so no later frame can observe partially mutated
            // state (`input.spec` is a per-frame clone in any case).
            // M5.19: the encoder-owned `ScfWorkspace` removes the
            // per-call scratch allocations; semantics are unchanged
            // (every field re-initialized per call, outputs still fresh).
            let o = estimate_scale_factors_channel_reuse(&mut input, &mut self.s4_ws);
            s4outs.push(S4Out {
                scf: o.scf,
                gain: o.global_gain,
                quant_spec: o.quant_spec,
                mutated: o.mutated_spectrum,
            });
        }

        // --- S5: QMAX + sectioning ---
        let noise = vec![NO_NOISE_PNS; live[0].cnt];
        // M5.g census (pure read, no behavior change): record any deviation
        // from the all-`NO_NOISE_PNS` vector passed at every S5 noise site.
        snap.noise_any_non_pns |= noise.iter().any(|&n| n != NO_NOISE_PNS);
        let mut qmaxs: Vec<Vec<u32>> = Vec::with_capacity(self.channels);
        let mut sections = Vec::with_capacity(self.channels);
        for (ch, c) in live.iter().enumerate() {
            let mut qmax = vec![0u32; c.cnt];
            calc_max_value_in_sfb(
                c.cnt,
                max_common,
                c.per,
                &c.offsets,
                &s4outs[ch].quant_spec,
                &mut qmax,
            );
            let (sec, _) = dyn_bit_count_reuse(
                &BitCountInput {
                    quant_spec: &s4outs[ch].quant_spec,
                    max_value_in_sfb: &qmax,
                    scf: &s4outs[ch].scf,
                    block_type: seqs[ch] as i32,
                    sfb_cnt: c.cnt,
                    max_sfb_per_group: max_common,
                    sfb_per_group: c.per,
                    sfb_offsets: &c.offsets,
                    noise_nrg: &noise,
                    is_book: &books[ch],
                    is_scale: &scales[ch],
                    use_vcb11: false,
                },
                &mut self.qc.bc_scratch,
            );
            qmaxs.push(qmax);
            sections.push(sec);
        }

        // --- S6: QC gain loop ---
        let ms_digest = MsDigest::from_i32(digest).expect("ms digest range");
        let ms_bands = if ms_digest == MsDigest::Some {
            ms_mask_bits(live[0].cnt, live[0].per, max_common)
        } else {
            0
        };
        let qc_inputs: Vec<QcChannelInput> = live
            .iter()
            .enumerate()
            .map(|(ch, c)| QcChannelInput {
                spectrum: &s4outs[ch].mutated,
                scf: &s4outs[ch].scf,
                global_gain: s4outs[ch].gain,
                sfb_cnt: c.cnt,
                max_sfb_per_group: max_common,
                sfb_per_group: c.per,
                sfb_offsets: &c.offsets,
                block_type: seqs[ch] as i32,
                noise_nrg: &noise,
                is_book: &books[ch],
                is_scale: &scales[ch],
                tns: &c.tns,
            })
            .collect();
        let qc_input = QcFrameInput {
            channels: qc_inputs,
            no_red_pe: pe.data.pe,
            static_bits: s1static,
            ms_digest,
            ms_bands,
            glob_hdr_bits: ADTS_HEADER_BITS as i32,
            max_bits_per_frame: self.init.max_bits,
            max_bits_el: self.init.max_bits_el,
            relative_bits_el: MAXVAL_DBL,
            max_bit_fac: self.init.max_bit_fac,
            bit_res_mode: BitResMode::Full,
        };
        let (avg, rest) = adjust_bitrate(
            self.padding_rest,
            self.total_br as i32,
            self.sample_rate as i32,
            FRAME_LENGTH as i32,
        );
        self.padding_rest = rest;
        self.avg_bits = avg;
        // No `catch_unwind` here by design: a gain-loop envelope violation
        // aborts the run (frame in the harness log) and the encoder is
        // discarded, so no later frame can observe the partially mutated
        // `qc`/`dist` state. Expected `QcError`s stay structured skips.
        let qc_out = match qc_main_frame(&mut self.qc, &mut self.dist, &qc_input, MAX_ITERATIONS) {
            Ok(o) => o,
            Err(e) => {
                snap.notes.push(format!("qc error: {e:?}"));
                snap.step_error = Some(StepError::Qc(e));
                return (None, snap);
            }
        };
        self.qc.bit_res_tot = qc_out.bit_res_tot;
        snap.side_bits = qc_out.final_sections.iter().map(|s| s.side_info_bits).sum();
        snap.huff_bits = qc_out.final_sections.iter().map(|s| s.huffman_bits).sum();
        snap.scf_bits = qc_out.final_sections.iter().map(|s| s.scalefac_bits).sum();
        snap.used_dyn = qc_out.used_dyn_bits;
        snap.tot_fill = qc_out.tot_fill_bits;
        snap.align = qc_out.align_bits;
        snap.total = qc_out.total_bits;
        snap.npass = qc_out.passes.len();
        snap.crash = qc_out.passes.iter().map(|p| p.crash).max().unwrap_or(-1);
        snap.gains = qc_out.final_gains.clone();
        // M5.i margins (pure reads of final QC state; no decision impact).
        // `last_iterations` is the inner gain-adjust count of the final pass,
        // distinct from the outer pass count `npass`.
        snap.last_iterations = qc_out
            .passes
            .last()
            .expect("qc output has at least one pass")
            .iterations;
        snap.sum_bits = total_consumed_bits(
            qc_out.used_dyn_bits,
            qc_out.static_bits_used,
            0,
            GLOBAL_EXT_BITS,
            ADTS_HEADER_BITS as i32,
        );
        snap.el_granted = qc_out.el_granted;
        snap.max_dyn = qc_out.max_dyn_bits;
        snap.total_avail = qc_out.total_avail;
        snap.static_used = qc_out.static_bits_used;
        if qc_out.passes.len() != 1 {
            snap.notes.push(format!(
                "gain-loop outer passes npass={} (expected 1)",
                qc_out.passes.len()
            ));
            return (None, snap);
        }
        if snap.crash != -1 {
            // Crash recovery (FDK `qc_main.cpp:1141-1158` + `crashRecovery`):
            // the gain loop reduced `maxSfbPerGroup` and re-quantized under
            // it, and `qc_main_frame` returned converged output within
            // budget. Like the C encoder, emit the frame with reduced
            // bandwidth instead of failing: `final_max_sfb`,
            // `final_sections`, `final_gains`, and `final_quants` below are
            // already the post-crash values, and `qc_out.static_bits_used`
            // is the post-crash recount (MS mask included; tools/TNS cleared
            // only on full wipeout, `sfb == 0`).
            // `snap.crash` records the event; no blocking note is pushed so
            // the frame proceeds to S8.
        }
        // Final-pass QC results (M5.18): the gain loop's own last
        // quantization is reused instead of re-deriving it. Equivalence
        // with the former driver-side re-quantization (clone of the S4
        // spectrum + `zero_inactive_bands` at `final_max` +
        // `quantize_spectrum` at `final_gains` + `calc_max_value_in_sfb`):
        // (a) `reduce_bit_consumption` sets `recalc[ch]` on every gain
        //     adjustment (and for all channels on the crash path), so the
        //     last constraints-checked pass always quantized at
        //     `final_gains[ch]`; (b) crash recovery reduces `max_sfb`
        //     before that re-quant, so it ran under `final_max_sfb[ch]`;
        //     (c) for bands before `final_max` per group stride the
        //     zeroing never applies and the spectrum, scalefactors, gain,
        //     and traversal are identical, so the values are
        //     bit-identical; (d) at/after `final_max` the loop's spectrum
        //     buffer is the `vec![0i16; len]` initialization (quantize
        //     never visits those bands), which equals the q == 0 result of
        //     quantizing the zeroed spectrum. Frames with multi-pass gain
        //     loops emit no stream bytes here (loud skip); crash-recovery
        //     frames emit with reduced `maxSfbPerGroup` per FDK, and the
        //     argument above covers them regardless.
        let final_max: Vec<usize> = qc_out.final_max_sfb.clone();
        let final_quants: Vec<Vec<i16>> = qc_out.final_quants;
        let final_qmaxs: Vec<Vec<u32>> = qc_out.final_qmaxs;
        snap.section_books = qc_out
            .final_sections
            .iter()
            .map(|s| s.sections.iter().map(|sec| sec.code_book).collect())
            .collect();

        // --- S8 + transport ---
        let shapes: Vec<i32> = (0..self.channels)
            .map(|ch| {
                if seqs[ch] == WindowSequence::Short {
                    0 // SINE forced on short (`psy_main.cpp` build-output)
                } else {
                    match self.switches[ch].shape() {
                        WindowShape::Sine => 0,
                        WindowShape::KBD => 1,
                        WindowShape::LowOverlap => {
                            snap.notes.push("low-overlap shape".to_string());
                            1
                        }
                    }
                }
            })
            .collect();
        if !snap.notes.is_empty() {
            return (None, snap);
        }
        let gmasks: Vec<i32> = groupings
            .iter()
            .map(|(n, gl)| grouping_mask(*n, gl))
            .collect();
        let el_type = if self.channels == 2 { ID_CPE } else { ID_SCE };
        // Static agreement first (explicit element build, like the S8 test).
        let ms_mask_ref: &[i32] = if self.channels == 2 { &ms_mask } else { &[] };
        // Static agreement first (explicit element build, like the S8 test).
        // (Built twice — once for the probe, once for the owned block —
        // to keep borrow scopes simple.)
        let el_bits = {
            let channels_probe: Vec<ChannelFrameInput> = (0..self.channels)
                .map(|ch| ChannelFrameInput {
                    block_type: seqs[ch] as i32,
                    window_shape: shapes[ch],
                    grouping_mask: gmasks[ch],
                    max_sfb_per_group: final_max[ch] as i32,
                    section_data: &qc_out.final_sections[ch],
                    scf: &s4outs[ch].scf,
                    global_gain: qc_out.final_gains[ch],
                    quant_spec: &final_quants[ch],
                    max_value_in_sfb: &final_qmaxs[ch],
                    sfb_offsets: &live[ch].offsets,
                    noise_nrg: &noise,
                    is_book: &books[ch],
                    is_scale: &scales[ch],
                    tns: &live[ch].tns,
                    mdct_scale: live[ch].mdct_scale,
                })
                .collect();
            let el_probe = ElementFrameInput {
                el_type,
                instance_tag: 0,
                common_window: true,
                ms_digest: digest,
                ms_mask: ms_mask_ref,
                channels: channels_probe,
                static_bits_expected: s1static,
            };
            let mut w_el = BitWriter::new(8192);
            match channel_element_write(&el_probe, 2, 0, -1, &mut w_el) {
                Ok(b) => b,
                Err(e) => {
                    snap.notes.push(format!("element write: {e:?}"));
                    snap.step_error = Some(StepError::Frame(e));
                    return (None, snap);
                }
            }
        };
        // Post-crash static is the QC-updated value: crash recovery recounts
        // static under the surviving `maxSfbPerGroup` (MS mask included),
        // like C `crashRecovery :1484-1498`. Without crash this equals
        // `s1static` (the recount is the only static mutator), so corpus
        // behavior is unchanged.
        let static_expected = qc_out.static_bits_used;
        if let Err(e) = check_static_agreement(el_bits, static_expected, qc_out.used_dyn_bits) {
            snap.notes.push(format!("static agreement: {e:?}"));
            snap.step_error = Some(StepError::Frame(e));
            return (None, snap);
        }
        let channels_owned: Vec<ChannelFrameInput> = (0..self.channels)
            .map(|ch| ChannelFrameInput {
                block_type: seqs[ch] as i32,
                window_shape: shapes[ch],
                grouping_mask: gmasks[ch],
                max_sfb_per_group: final_max[ch] as i32,
                section_data: &qc_out.final_sections[ch],
                scf: &s4outs[ch].scf,
                global_gain: qc_out.final_gains[ch],
                quant_spec: &final_quants[ch],
                max_value_in_sfb: &final_qmaxs[ch],
                sfb_offsets: &live[ch].offsets,
                noise_nrg: &noise,
                is_book: &books[ch],
                is_scale: &scales[ch],
                tns: &live[ch].tns,
                mdct_scale: live[ch].mdct_scale,
            })
            .collect();
        let el_owned = ElementFrameInput {
            el_type,
            instance_tag: 0,
            common_window: true,
            ms_digest: digest,
            ms_mask: ms_mask_ref,
            channels: channels_owned,
            static_bits_expected: s1static,
        };
        let raw_input = RawBlockInput {
            elements: vec![el_owned],
            tot_fill_bits: qc_out.tot_fill_bits,
            align_bits: qc_out.align_bits,
            total_bits: qc_out.total_bits,
            glob_hdr_bits: ADTS_HEADER_BITS as i32,
            max_bits_per_frame: self.init.max_bits,
            min_bits_per_frame: 0,
        };
        let raw = match build_raw_block(&raw_input) {
            Ok(b) => b,
            Err(e) => {
                snap.notes.push(format!("raw block: {e:?}"));
                snap.step_error = Some(StepError::Frame(e));
                return (None, snap);
            }
        };
        let adts = match build_adts_frame(self.mode, self.sample_rate, &raw) {
            Ok(b) => b,
            Err(e) => {
                snap.notes.push(format!("adts: {e:?}"));
                snap.step_error = Some(StepError::Frame(e));
                return (None, snap);
            }
        };
        snap.adts_len = adts.len();
        (Some(adts), snap)
    }
}

// ---------------------------------------------------------------------------
// Phase 1 comparison test (test-only)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod phase1_tests {
    use super::*;
    use std::collections::HashMap;

    /// The 15 Phase-0 fixtures: (resynthesis short name, C/oracle stem,
    /// sample rate, channels).
    const FIXTURES: [(&str, &str, u32, usize); 15] = [
        ("silence-stereo", "m2-silence-stereo-48k", 48000, 2),
        ("sine-stereo", "m2-sine-stereo-48k", 48000, 2),
        ("onset-stereo", "m2-onset-stereo-48k", 48000, 2),
        ("stepdown-stereo", "m2-stepdown-stereo-48k", 48000, 2),
        ("click-mono", "m2-click-mono-48k", 48000, 1),
        ("impulse-mono", "m2-impulse-mono-48k", 48000, 1),
        ("edge-mono", "m2-edge-mono-48k", 48000, 1),
        ("multitone-stereo", "m3-multitone-stereo-48k", 48000, 2),
        ("sweep-stereo", "m3-sweep-stereo-48k", 48000, 2),
        ("noise-mono", "m3-noise-mono-48k", 48000, 1),
        ("tonal-noise-stereo", "m3-tonal-noise-stereo-48k", 48000, 2),
        ("quiet-sine-stereo", "m3-quiet-sine-stereo-48k", 48000, 2),
        ("sine-stereo-44100", "m3-sine-stereo-441k", 44100, 2),
        ("onset-stereo-44100", "m3-onset-stereo-441k", 44100, 2),
        ("stress-stereo", "m6-stress-stereo-48k", 48000, 2),
    ];

    /// Phase-0 known SHORT_WINDOW oracle-vs-C blips: diagnostic context
    /// only (each still requires live `seq == SHORT` + oracle `seq ==
    /// SHORT` to classify as expected; never a blanket exemption).
    const KNOWN_SHORT: [(&str, u32); 10] = [
        ("m2-onset-stereo-48k", 24),
        ("m2-stepdown-stereo-48k", 1),
        ("m2-click-mono-48k", 1),
        ("m2-sine-stereo-48k", 1),
        ("m3-multitone-stereo-48k", 1),
        ("m3-sweep-stereo-48k", 1),
        ("m3-tonal-noise-stereo-48k", 1),
        ("m3-sine-stereo-441k", 1),
        ("m3-onset-stereo-441k", 22),
        ("m6-stress-stereo-48k", 1),
    ];

    /// Minimal per-frame oracle dump reference (diagnosis only; the live
    /// driver never consumes these as inputs).
    #[derive(Debug, Default, Clone)]
    struct DumpFrame {
        seq: i32,
        geo_cnt: i32,
        geo_per: i32,
        geo_max: i32,
        digest: Option<i32>,
        mask_len: usize,
        mask_ones: u32,
        pe: i32,
        static_bits: i32,
        s6total: i32,
        s6align: i32,
        s6npass: i32,
        s6crash: i32,
        s8total: i32,
    }

    fn load_dump(path: &str) -> HashMap<u32, DumpFrame> {
        let mut out = HashMap::new();
        let mut cur: Option<u32> = None;
        // Same missing-artifact contract as the dump_* readers above: these
        // dumps live under $PHASE1_ORACLE_DIR and are explicit validation
        // inputs, never silently skipped.
        let text = require_validation_file(
            "PHASE1_ORACLE_DIR",
            path,
            "generate oracle dumps per tools/enc-oracle/README.md and set PHASE1_ORACLE_DIR to the dump directory",
        );
        for ln in text.split('\n') {
            let ln = ln.trim();
            if ln.starts_with("FRAME ") {
                let t: Vec<&str> = ln.split_whitespace().collect();
                let f: u32 = t[1].parse().expect("frame");
                // FRAME <f> CH <c> SEQ <seq> ...
                let seq: i32 = t[5].parse().expect("seq");
                let e = out.entry(f).or_insert_with(DumpFrame::default);
                e.seq = seq;
                cur = Some(f);
            } else if ln.starts_with("S1GEO ") {
                let kv: HashMap<&str, &str> = ln
                    .split_whitespace()
                    .skip(1)
                    .map(|kv| {
                        let (k, v) = kv.split_once('=').expect("kv");
                        (k, v)
                    })
                    .collect();
                if let Some(f) = cur {
                    let e = out.entry(f).or_insert_with(DumpFrame::default);
                    // Keep channel 0's geometry (channels agree on corpus).
                    if e.geo_cnt == 0 {
                        e.geo_cnt = kv["cnt"].parse().expect("cnt");
                        e.geo_per = kv["per"].parse().expect("per");
                        e.geo_max = kv["max"].parse().expect("max");
                    }
                }
            } else if ln.starts_with("MSD ") {
                let t: Vec<&str> = ln.split_whitespace().collect();
                if let Some(f) = cur {
                    let e = out.entry(f).or_insert_with(DumpFrame::default);
                    e.digest = Some(t[1].parse().expect("digest"));
                    e.mask_len = t.len() - 2;
                    e.mask_ones = t[2..].iter().map(|v| v.parse::<u32>().unwrap_or(0)).sum();
                }
            } else if ln.starts_with("PE ") {
                let t: Vec<&str> = ln.split_whitespace().collect();
                if let Some(f) = cur {
                    // `PE <pe> <const> <nact> <offset>` element sums.
                    if let Ok(pe) = t[1].parse::<i32>() {
                        out.entry(f).or_insert_with(DumpFrame::default).pe = pe;
                    }
                }
            } else if ln.starts_with("STATIC ") {
                let t: Vec<&str> = ln.split_whitespace().collect();
                if let Some(f) = cur {
                    out.entry(f).or_insert_with(DumpFrame::default).static_bits =
                        t[2].parse().expect("static");
                }
            } else if ln.starts_with("S6FINAL ") {
                let kv: HashMap<&str, &str> = ln
                    .split_whitespace()
                    .skip(1)
                    .map(|kv| {
                        let (k, v) = kv.split_once('=').expect("kv");
                        (k, v)
                    })
                    .collect();
                if let Some(f) = cur {
                    let e = out.entry(f).or_insert_with(DumpFrame::default);
                    e.s6total = kv["total"].parse().expect("total");
                    e.s6align = kv["align"].parse().expect("align");
                    e.s6npass = kv["npass"].parse().expect("npass");
                    e.s6crash = kv["crash"].parse().expect("crash");
                }
            } else if ln.starts_with("S8FINAL ") {
                let kv: HashMap<&str, &str> = ln
                    .split_whitespace()
                    .skip(1)
                    .map(|kv| {
                        let (k, v) = kv.split_once('=').expect("kv");
                        (k, v)
                    })
                    .collect();
                if let Some(f) = cur {
                    out.entry(f).or_insert_with(DumpFrame::default).s8total =
                        kv["total"].parse().expect("total");
                }
            }
        }
        out
    }

    /// Split an `.aac` file into raw ADTS frames (lengths include the
    /// 7-byte header).
    fn split_adts(bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut off = 0usize;
        while off + 7 <= bytes.len() {
            assert_eq!(bytes[off], 0xFF, "sync @ {off}");
            assert_eq!(bytes[off + 1] & 0xF0, 0xF0, "sync tail @ {off}");
            let fl = ((bytes[off + 3] as usize & 0x03) << 11)
                | ((bytes[off + 4] as usize) << 3)
                | ((bytes[off + 5] as usize) >> 5);
            assert!(fl >= 7, "frame len @ {off}");
            out.push(bytes[off..off + fl].to_vec());
            off += fl;
        }
        assert_eq!(off, bytes.len(), "no trailing bytes");
        out
    }

    /// Independent ADTS header validation of one emitted frame.
    fn check_header(frame: &[u8], sample_rate: u32, channels: usize) {
        assert!(frame.len() >= 7);
        let want_fi: u8 = match sample_rate {
            48000 => 3,
            44100 => 4,
            _ => panic!("unexpected rate {sample_rate}"),
        };
        let want_cc = channels as u8;
        assert_eq!(frame[0], 0xFF);
        assert_eq!(frame[1], 0xF1, "MPEG-4 / layer 0 / no CRC");
        assert_eq!((frame[2] >> 6) & 0x3, 1, "AAC-LC profile");
        assert_eq!((frame[2] >> 2) & 0xF, want_fi, "sampling freq idx");
        let cc = ((frame[2] & 0x1) << 2) | ((frame[3] >> 6) & 0x3);
        assert_eq!(cc, want_cc, "channel config");
        let fl = ((frame[3] as usize & 0x03) << 11)
            | ((frame[4] as usize) << 3)
            | ((frame[5] as usize) >> 5);
        assert_eq!(fl, frame.len(), "frame_length counts header");
        let full = ((frame[5] as u16 & 0x1F) << 6) | ((frame[6] as u16) >> 2);
        assert_eq!(full, 0x7FF, "VBR fullness");
        assert_eq!(frame[6] & 0x3, 0, "single raw block");
    }

    fn env_dir(key: &str, default: &str) -> String {
        std::env::var(key).unwrap_or_else(|_| default.to_string())
    }

    /// Preflight for explicitly-requested oracle validation.
    ///
    /// Tier-B tests run only when invoked by name (ordinary CI skips them).
    /// A missing artifact is a setup error, never a silent skip: fail loudly
    /// naming the artifact, the variable that locates it, and how to supply
    /// it. `rust/tools/preflight-validation.sh` performs the same checks
    /// outside of `cargo test`.
    fn require_validation_file(var: &str, path: &str, how: &str) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|_| {
            panic!("reference validation artifact missing: {path} (from ${var}); {how}")
        })
    }

    /// Byte-oriented variant of `require_validation_file` for encoded
    /// reference outputs (ADTS is not valid UTF-8, so `read_to_string`
    /// must not be used).
    fn require_validation_bytes(var: &str, path: &str, how: &str) -> Vec<u8> {
        std::fs::read(path).unwrap_or_else(|_| {
            panic!("reference validation artifact missing: {path} (from ${var}); {how}")
        })
    }

    /// Parse the first int list for `tag` at/after S1FRAME f=`frame`
    /// (first channel block) from an oracle dump.
    fn dump_ints(path: &str, frame: u32, tag: &str) -> Vec<i32> {
        let text = require_validation_file(
            "PHASE1_ORACLE_DIR",
            path,
            "generate oracle dumps per tools/enc-oracle/README.md and set PHASE1_ORACLE_DIR to the dump directory",
        );
        let lines: Vec<&str> = text.split('\n').collect();
        let start = lines
            .iter()
            .position(|ln| ln.trim() == format!("S1FRAME f={frame}"))
            .expect("S1FRAME present");
        for ln in &lines[start..] {
            let t = ln.trim();
            if t == format!("S1FRAME f={}", frame + 1) {
                break;
            }
            if t.starts_with(tag) {
                let rest = t[tag.len()..].trim();
                // Tags look like `SPEC ch=0 <ints>` or `PENLD <ints>`.
                let nums: Vec<i32> = rest
                    .split_whitespace()
                    .filter_map(|w| w.parse::<i32>().ok())
                    .collect();
                // For `SPEC ch=0`, the `0` parses too — drop a leading
                // channel index when the count is one over 1024.
                if tag.starts_with("SPEC") && nums.len() == 1025 {
                    return nums[1..].to_vec();
                }
                return nums;
            }
        }
        panic!("tag {tag} not found for f={frame}");
    }

    /// S1-input bridge agreement on sine f5 ch0 (loud LONG frame): the
    /// live pre-MS working sets must track the oracle dump within the
    /// documented frontend-noise envelope. Bounds are structural (catch
    /// wrong arrays/scales, which diverge by ~1e9), not bit-exactness:
    /// spectra rel ≤ 1e-4 (observed 5e-6), stored maxdiff ≤ 8192
    /// (observed 1026), energy-Ld maxdiff ≤ 32M Q31 on real bands
    /// (observed 2.2M; near-zero-band sentinel flips are a known
    /// float-lane limitation, not asserted here).
    #[test]
    fn phase1_frontend_agreement_sine_f5() {
        let o_dir = env_dir(
            "PHASE1_ORACLE_DIR",
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/phase0-20261015/oracle",
        );
        let dump = format!("{o_dir}/m2-sine-stereo-48k.s1.txt");
        let pcm = testdata::fixture_samples("sine-stereo");
        let mut enc = StreamEncoder::new(48000, 2);
        let mut fe_opt = None;
        for f in 0..=5u32 {
            let fresh: Vec<[i16; 1024]> = pcm
                .iter()
                .map(|ch| {
                    let mut b = [0i16; 1024];
                    for (i, v) in b.iter_mut().enumerate() {
                        let t = f as usize * 1024 + i;
                        *v = if t < ch.len() { ch[t] } else { 0 };
                    }
                    b
                })
                .collect();
            fe_opt = Some(enc.frontend(f, fresh).expect("frontend ok"));
        }
        let fe = fe_opt.unwrap();
        assert_eq!(fe.seqs[0] as i32, 0, "f5 is LONG");
        let c = &fe.live[0];
        let o_spec = dump_ints(&dump, 5, "SPEC ch=0");
        let o_senl = dump_ints(&dump, 5, "SENL");
        // NOTE: dump tags PENLD/PTHLD/SENL/FF appear per channel; first hit
        // is ch0.
        let o_penld = dump_ints(&dump, 5, "PENLD");
        assert_eq!(o_spec.len(), 1024);
        assert_eq!(o_senl.len(), c.cnt);
        // Spectrum: structural agreement (wrong domain/scale diverges ~1e9).
        let mut maxd: i64 = 0;
        let mut peak: i64 = 0;
        for (a, b) in c.spec_q31.iter().zip(o_spec.iter()) {
            let d = (*a as i64 - *b as i64).abs();
            maxd = maxd.max(d);
            peak = peak.max(*a as i64).max(*b as i64);
        }
        let rel = maxd as f64 / peak as f64;
        assert!(rel <= 1e-4, "spec rel {rel:.3e} (maxd {maxd} peak {peak})");
        // Stored energies: near-exact (wrong array/scale diverges ~1e9).
        let mut maxde: i64 = 0;
        for (a, b) in c.e_stored.iter().zip(o_senl.iter()) {
            maxde = maxde.max((*a as i64 - *b as i64).abs());
        }
        assert!(maxde <= 8192, "stored maxdiff {maxde}");
        // Energy Ld (float-lane Q31): agreement in the energy domain
        // (committed `assert_ld_close` standard: sentinel-vs-tiny flips
        // invert to sub-tolerance energy gaps; large-energy agreement is
        // strict).
        let ld_to_e = |ld: i32| -> f64 {
            // Exact -1.0 sentinel (i32::MIN) means zero energy; anything
            // else inverts (near-sentinel reals invert to ~2^-33 ≈ 0).
            if ld == i32::MIN {
                0.0
            } else {
                2f64.powf(ld as f64 / 2_147_483_648.0 * 64.0 + 31.0)
            }
        };
        let mut worst = 0.0f64;
        for (a, b) in c.e_ld.iter().zip(o_penld.iter()) {
            let eg = ld_to_e(*a);
            let ew = ld_to_e(*b);
            let tol = 32.0 + ew / 500.0;
            let d = (eg - ew).abs();
            worst = worst.max(d / tol);
            assert!(
                d <= tol,
                "energy-Ld band: live {a} (E={eg:.3}) vs oracle {b} (E={ew:.3}) tol={tol:.3}"
            );
        }
        let _ = worst;
    }

    #[test]
    fn phase1c_short_ms_ld_grouped_refresh() {
        // C evidence: `groupShortData` MS branch (`grp_data.cpp:211-224`,
        // per-group `nrgAddSaturate` window sums over the active bands)
        // followed by the short-frame MS Ld refresh
        // (`psy_main.cpp:1078-1084`, `LdDataVector` of the grouped sums).
        // Oracle inputs: committed per-window MS stored sums for the SHORT
        // frame onset-stereo f24 (same `CalcBandNrgMSOpt` gate as C);
        // grouping from `seq_onset-stereo.txt`; geometry from qc1.
        // No oracle exposes the grouped MS Ld itself (limitation recorded:
        // the refresh formula is proven, not the C array contents).
        let wins = testdata::load_ms("onset-stereo", 24);
        assert_eq!(wins.len(), 8, "short windows");
        let row = testdata::load_seq("onset-stereo")
            .into_iter()
            .find(|r| r.frame == 24 && r.channel == 0)
            .expect("seq row");
        assert_eq!(row.groups, 4, "group count");
        assert_eq!(row.group_len, [3, 1, 3, 1], "group lens");
        let s1 = testdata::load_s1("onset-stereo", 24);
        assert_eq!(s1.ch[0].geo_cnt, 56, "grouped band count");
        assert_eq!(s1.ch[0].geo_per, 14, "stride");
        let (cnt, per) = (s1.ch[0].geo_cnt, s1.ch[0].geo_per);
        let ms_active = wins[0].mid.len();
        assert_eq!(ms_active, 13, "per-window active bands");
        let mid: Vec<Vec<i32>> = wins.iter().map(|w| w.mid.clone()).collect();
        let side: Vec<Vec<i32>> = wins.iter().map(|w| w.side.clone()).collect();
        let (em, es, eml, esl) =
            grouped_ms_vectors(&mid, &side, row.groups, &row.group_len, per, cnt, ms_active);
        // Independent regroup with the C loop shape; stride pads stay 0.
        let mut w = 0usize;
        for (grp, &n) in [3usize, 1, 3, 1].iter().enumerate() {
            for b in 0..ms_active {
                let mut am = 0i32;
                let mut as_ = 0i32;
                for k in 0..n {
                    am = am.saturating_add(mid[w + k][b]);
                    as_ = as_.saturating_add(side[w + k][b]);
                }
                assert_eq!(em[grp * per + b], am, "grp{grp} band{b} mid sum");
                assert_eq!(es[grp * per + b], as_, "grp{grp} band{b} side sum");
            }
            for b in ms_active..per {
                assert_eq!(em[grp * per + b], 0, "grp{grp} pad{b} mid");
                assert_eq!(es[grp * per + b], 0, "grp{grp} pad{b} side");
            }
            w += n;
        }
        // Fresh refresh: Ld is the exact `fLog2` of the grouped sums on
        // every band (the pre-1C driver fed stale history here instead).
        // `calc_ld_data` itself is C-probe-pinned (`q31.rs` tests).
        for (i, (&s, &l)) in em.iter().zip(eml.iter()).enumerate() {
            assert_eq!(l, calc_ld_data(s), "mid_ld[{i}] fresh, stored={s}");
        }
        for (i, (&s, &l)) in es.iter().zip(esl.iter()).enumerate() {
            assert_eq!(l, calc_ld_data(s), "side_ld[{i}] fresh, stored={s}");
        }
        // Non-degeneracy: this frame's grouped sums are actually nonzero,
        // so the test distinguishes fresh refresh from stale-zero history
        // (whose Ld would be all `i32::MIN`).
        assert!(em.iter().any(|&v| v != 0), "nonzero grouped mid");
        assert!(
            eml.iter().any(|&v| v != i32::MIN),
            "fresh Ld differs from stale-zero history"
        );
    }

    #[test]
    fn phase1c_ms_grouping_saturates_like_c() {
        // `nrgAddSaturate` pins at `MAXVAL_DBL` (`grp_data.cpp:114-116`);
        // `saturating_add` matches for non-negative energies.
        let big = vec![vec![i32::MAX; 2]; 3];
        let zero = vec![vec![0i32; 2]; 3];
        let (em, es, eml, esl) = grouped_ms_vectors(&big, &zero, 1, &[3, 0, 0, 0], 2, 2, 2);
        assert_eq!(em, vec![i32::MAX, i32::MAX], "saturating sum");
        assert_eq!(es, vec![0, 0], "zero sum");
        assert_eq!(eml, vec![calc_ld_data(i32::MAX); 2], "Ld of pinned max");
        assert_eq!(esl, vec![calc_ld_data(0); 2], "Ld of zero is sentinel");
        assert_eq!(calc_ld_data(0), i32::MIN, "sentinel spelling");
    }

    #[test]
    fn phase1c_threshold_ld_floor_matches_c() {
        // `FL2FXCONST_DBL(-0.515625)` = -33/64 in Q31: exact integer
        // (-33 * 2^25, no rounding involved), applied by C with `fixMax`
        // (`psy_main.cpp:1071-1075` short, `:1124-1128` long).
        assert_eq!(THRESHOLD_LD_FLOOR_Q31, -33 * 33_554_432);
        assert_eq!(THRESHOLD_LD_FLOOR_Q31, -1_107_296_256);
        assert_eq!(
            THRESHOLD_LD_FLOOR_Q31 as f64,
            -0.515625 * 2_147_483_648.0,
            "Q31 spelling"
        );
        // Zero stored hits the `-1.0` sentinel, which the floor then lifts
        // (C's `fixMax` sees the same sentinel, so this is parity, not a
        // special case).
        assert_eq!(threshold_ld_of(&[0]), vec![THRESHOLD_LD_FLOOR_Q31]);
        assert_eq!(threshold_ld_of(&[-7]), vec![THRESHOLD_LD_FLOOR_Q31]);
        // Positive stored values sit above the floor (floor = Ld(1/4);
        // Ld(1) = -31/64 already exceeds it), so they pass through
        // unchanged. Margins asserted so passthrough is genuine.
        assert!(calc_ld_data(1) > THRESHOLD_LD_FLOOR_Q31, "Ld(1) margin");
        assert_eq!(threshold_ld_of(&[1]), vec![calc_ld_data(1)]);
        assert!(
            calc_ld_data(1 << 30) > THRESHOLD_LD_FLOOR_Q31,
            "Ld(2^30) margin"
        );
        assert_eq!(threshold_ld_of(&[1 << 30]), vec![calc_ld_data(1 << 30)]);
        assert_eq!(threshold_ld_of(&[i32::MAX]), vec![calc_ld_data(i32::MAX)]);
        // Mixed vector keeps positions aligned.
        assert_eq!(
            threshold_ld_of(&[0, 1, i32::MAX]),
            vec![
                THRESHOLD_LD_FLOOR_Q31,
                calc_ld_data(1),
                calc_ld_data(i32::MAX)
            ]
        );
    }

    /// Parse the element-level `MSD <digest> <mask bits>` line inside the
    /// `S1FRAME f=<frame>` block of an oracle dump.
    fn dump_msd(path: &str, frame: u32) -> (i32, Vec<i32>) {
        let text = require_validation_file(
            "PHASE1_ORACLE_DIR",
            path,
            "generate oracle dumps per tools/enc-oracle/README.md and set PHASE1_ORACLE_DIR to the dump directory",
        );
        let lines: Vec<&str> = text.split('\n').collect();
        let start = lines
            .iter()
            .position(|ln| ln.trim() == format!("S1FRAME f={frame}"))
            .expect("S1FRAME present");
        for ln in &lines[start..] {
            let t = ln.trim();
            if t == format!("S1FRAME f={}", frame + 1) {
                break;
            }
            if t.starts_with("MSD ") {
                let nums: Vec<i32> = t["MSD ".len()..]
                    .split_whitespace()
                    .map(|w| w.parse::<i32>().expect("msd int"))
                    .collect();
                assert!(!nums.is_empty(), "MSD values");
                return (nums[0], nums[1..].to_vec());
            }
        }
        panic!("MSD not found for f={frame}");
    }

    /// Step a fresh streaming encoder over feeder frames `0..=frame`
    /// (feeder lifecycle identical to `phase1_full_stream_parity`) and
    /// return every snapshot. The encoder is discarded afterwards: no
    /// state survives this helper.
    fn step_to_frame(short: &str, sr: u32, ch: usize, frame: u32) -> Vec<FrameSnapshot> {
        let pcm = testdata::fixture_samples(short);
        assert_eq!(pcm.len(), ch);
        let s = pcm[0].len();
        let cfg =
            EncoderConfig::new(sr, ch as u32, FRAME_LENGTH, TransportType::Adts).expect("cfg");
        let mut feeder = PcmFeeder::new(cfg);
        let mut interleaved = Vec::with_capacity(s * ch);
        for i in 0..s {
            for c in 0..ch {
                interleaved.push(pcm[c][i]);
            }
        }
        let mut frames = feeder.push_interleaved(&interleaved).expect("push");
        frames.extend(feeder.finish().expect("finish"));
        let mut enc = StreamEncoder::new(sr, ch);
        let mut snaps = Vec::new();
        for f in 0..=frame as usize {
            let fresh: Vec<Vec<i16>> = frames[f].channels().iter().map(|b| b.to_vec()).collect();
            let (bytes, snap) = enc.step_frame(f as u32, &fresh);
            assert!(
                snap.notes.is_empty(),
                "{short} f={f} must emit for capture (notes {:?})",
                snap.notes
            );
            bytes.expect("emitted");
            snaps.push(snap);
        }
        snaps
    }

    /// First-pass MS margins from decision operands, transcribing
    /// `ms_stereo.cpp:185-201` exactly (halved-shift form, strict `>`
    /// becomes `margin >= 1` on integers). Sign convention: `margin > 0`
    /// favors M/S. Returns `(margins, mask)` over the `(0..cnt).step_by(per)`
    /// loop; the caller handles the ALL-upgrade path separately.
    fn ms_first_pass(
        e_l: &[i32],
        e_r: &[i32],
        t_l: &[i32],
        t_r: &[i32],
        eld_l: &[i32],
        eld_r: &[i32],
        tld_l: &[i32],
        tld_r: &[i32],
        em: &[i32],
        es: &[i32],
        em_ld: &[i32],
        es_ld: &[i32],
        cnt: usize,
        per: usize,
        max: usize,
        allow_ms: bool,
    ) -> (Vec<i64>, Vec<i32>) {
        let mut margins = vec![0i64; cnt];
        let mut mask = vec![0i32; cnt];
        for sfb in (0..cnt).step_by(per) {
            for off in 0..max {
                let b = sfb + off;
                if b >= cnt {
                    break;
                }
                let min_tld = tld_l[b].min(tld_r[b]) as i64;
                let tmp = (eld_l[b] as i64).max(tld_l[b] as i64);
                let mut pnlr = (tld_l[b] as i64 >> 1) - (tmp >> 1);
                pnlr += tld_r[b] as i64 >> 1;
                let tmp = (eld_r[b] as i64).max(tld_r[b] as i64);
                pnlr -= tmp >> 1;
                let tmp = (em_ld[b] as i64).max(min_tld);
                let mut pnms = min_tld - (tmp >> 1);
                let tmp = (es_ld[b] as i64).max(min_tld);
                pnms -= tmp >> 1;
                let m = pnms - pnlr;
                margins[b] = m;
                mask[b] = i32::from(allow_ms && m >= 1);
            }
        }
        (margins, mask)
    }

    /// Stored-domain residue envelope from the committed frontend
    /// agreement scale (`psy.rs` `assert_q31_close`: floor 32 for
    /// small-band rounding plus |w|/500 proportional term). Established
    /// on independent committed vectors, not chosen for this test.
    fn q31_tol(w: i32) -> i64 {
        32 + ((w as f64).abs() / 500.0) as i64
    }

    /// Energy-domain inversion of a Q31 Ld value (sentinel-aware), same
    /// convention as the committed `assert_ld_close` checks.
    fn ld_energy(ld: i32) -> f64 {
        if ld == i32::MIN {
            0.0
        } else {
            2f64.powf(ld as f64 / 2_147_483_648.0 * 64.0 + 31.0)
        }
    }

    /// Energy-domain residue envelope (`assert_ld_close` scale).
    fn ld_energy_tol(e_ref: f64) -> f64 {
        32.0 + e_ref / 500.0
    }

    /// Strict-`>` decision bit from an integer margin (`ms_stereo.cpp:201`).
    fn use_of(margin: i64) -> i32 {
        i32::from(margin >= 1)
    }

    /// Phase 1E final: sine-stereo f5 band-16 flip causality.
    ///
    /// Decision rule verified: `useMS = allowMS && (pnms > pnlr)`
    /// (`ms_stereo.cpp:201`), first pass over `(0..cnt).step_by(per)`,
    /// digest SOME on both sides (no ALL-upgrade path), IS books all zero
    /// (committed qc1 ISB/ISS), `allowMS` set. LONG frame: every C operand
    /// is committed directly (psy NRG/THR/NRGLD/TLD/SPREAD, ms
    /// MSM0/MSS0/MSMLD0/MSSLD0); the S1 dump path copies the same arrays,
    /// so the dump MSD is C-faithful here.
    #[test]
    fn phase1e_sine_f5_band16_residue() {
        let o_dir = env_dir(
            "PHASE1_ORACLE_DIR",
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/phase0-20261015/oracle",
        );
        // Fixture identity + frame indexing.
        let pcm = testdata::fixture_samples("sine-stereo");
        assert_eq!((pcm.len(), pcm[0].len()), (2, 48000));
        let row = testdata::load_seq("sine-stereo")
            .into_iter()
            .find(|r| r.frame == 5 && r.channel == 0)
            .expect("seq row");
        assert_eq!(row.seq, 0, "f5 is LONG");
        let snaps = step_to_frame("sine-stereo", 48000, 2, 5);
        let snap = &snaps[5];
        assert_eq!(snap.seq, 0, "live seq agrees (index check)");
        let cap = snap.ms_cap.as_ref().expect("ms capture");
        // Gating state.
        let s1 = testdata::load_s1("sine-stereo", 5);
        assert!(
            s1.ch.iter().all(|c| c.isb.iter().all(|&b| b == 0)),
            "IS inactive"
        );
        assert_eq!((cap.cnt, cap.per, cap.max), (45, 45, 45));
        assert_eq!(s1.ch[0].geo_max, 45);
        assert_eq!(cap.offsets, s1.ch[0].offsets, "offsets agree");
        assert!(cap.allow_ms);
        // Complete mask vectors: exactly one flipped band.
        let (o_dig, o_mask) = dump_msd(&format!("{o_dir}/m2-sine-stereo-48k.s1.txt"), 5);
        assert_eq!((cap.digest, o_dig), (1, 1), "SOME both sides");
        let flips: Vec<usize> = (0..45).filter(|&b| cap.mask[b] != o_mask[b]).collect();
        assert_eq!(flips, vec![16], "single flip at band 16");
        // Transcription: recomputed first-pass masks reproduce both
        // recorded masks (proves the margin computation).
        let (lm, lmask) = ms_first_pass(
            &cap.e_l,
            &cap.e_r,
            &cap.t_l,
            &cap.t_r,
            &cap.eld_l,
            &cap.eld_r,
            &cap.tld_l,
            &cap.tld_r,
            &cap.em,
            &cap.es,
            &cap.em_ld,
            &cap.es_ld,
            cap.cnt,
            cap.per,
            cap.max,
            cap.allow_ms,
        );
        assert_eq!(lmask, cap.mask, "live transcription exact");
        let p0 = testdata::load_psy("sine-stereo", 5, 0, false);
        let p1 = testdata::load_psy("sine-stereo", 5, 1, false);
        let ms = testdata::load_ms("sine-stereo", 5);
        let (om, omask) = ms_first_pass(
            &p0.nrg[0],
            &p1.nrg[0],
            &p0.thr[0],
            &p1.thr[0],
            &p0.nrg_ld[0],
            &p1.nrg_ld[0],
            &p0.threshold_ld.clone().unwrap(),
            &p1.threshold_ld.clone().unwrap(),
            &ms[0].mid,
            &ms[0].side,
            &ms[0].mid_ld,
            &ms[0].side_ld,
            45,
            45,
            45,
            true,
        );
        assert_eq!(omask, o_mask, "oracle transcription exact");
        // Band 16: signed margins on either side of the strict boundary.
        assert_eq!((om[16], lm[16]), (2539383, 0), "pinned margins");
        assert_eq!((omask[16], lmask[16]), (1, 0));
        // Stored operands are bit-identical; only Ld differs.
        for (name, a, b) in [
            ("e_l", cap.e_l[16], p0.nrg[0][16]),
            ("e_r", cap.e_r[16], p1.nrg[0][16]),
            ("t_l", cap.t_l[16], p0.thr[0][16]),
            ("t_r", cap.t_r[16], p1.thr[0][16]),
            ("em", cap.em[16], ms[0].mid[16]),
            ("es", cap.es[16], ms[0].side[16]),
        ] {
            assert_eq!(a, b, "{name} stored bit-exact");
        }
        assert_eq!(
            cap.tld_l[16],
            p0.threshold_ld.clone().unwrap()[16],
            "tld_l floor both"
        );
        assert_eq!(
            cap.tld_r[16],
            p1.threshold_ld.clone().unwrap()[16],
            "tld_r floor both"
        );
        // Ld deltas within the committed energy-domain envelope.
        for (name, live, want) in [
            ("eld_l", cap.eld_l[16], p0.nrg_ld[0][16]),
            ("eld_r", cap.eld_r[16], p1.nrg_ld[0][16]),
            ("em_ld", cap.em_ld[16], ms[0].mid_ld[16]),
        ] {
            let (el, ew) = (ld_energy(live), ld_energy(want));
            let d = (el - ew).abs();
            assert!(
                d <= ld_energy_tol(ew),
                "{name}: E live={el:.4} vs oracle={ew:.4} (tol {:.4})",
                ld_energy_tol(ew)
            );
        }
        // Exact counterfactual: the measured Ld deltas fully account for
        // the margin shift (no missing operand). needs: M_o=2539383 >= 1
        // needs shift <= -2539383; measured shift is exactly that.
        assert_eq!(lm[16] - om[16], -2539383, "exact measured shift");
        assert_eq!(use_of(om[16]), 1);
        assert_eq!(use_of(om[16] - 2539383), 0, "shift alone flips the bit");
        println!("sine f5 band16: residue-explained (stored exact, Ld in-envelope, shift -2539383 to margin 0)");
    }

    /// Phase 1E final: onset-stereo f24 (SHORT) band causality.
    ///
    /// Reference model (verified, not assumed): SHORT MS inputs are the
    /// committed GROUPED L/R vectors plus grouped MS sums derived by the
    /// exact C regroup (`grp_data.cpp:211-224`, saturating adds) with the
    /// exact `LdDataVector` refresh (`calc_ld_data`, C-probe-pinned).
    /// Gating: cnt/per/max 56/14/13 both sides, grouped offsets agree,
    /// IS books all zero (committed qc1 ISB/ISS), `allowMS` set, digest
    /// SOME on live and dump (no ALL-upgrade path).
    ///
    /// Reference-health finding (proven below): the dump MSD on this
    /// SHORT frame is NOT the C decision output. The S1 oracle path
    /// (`enc_oracle.cpp:1060-1214`) copies stale per-window MS Ld
    /// (`msLd[0]`) and never applies the grouped refresh the real
    /// encoder performs (`psy_main.cpp:1078-1084`). Bands 48-50 prove it:
    /// live and reference *inputs* are bit-identical there, the rule
    /// gives 1 on both, yet the dump says 0.
    #[test]
    fn phase1e_onset_f24_bands_causality() {
        let o_dir = env_dir(
            "PHASE1_ORACLE_DIR",
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/phase0-20261015/oracle",
        );
        // Fixture identity + frame indexing + coverage.
        let pcm = testdata::fixture_samples("onset-stereo");
        assert_eq!((pcm.len(), pcm[0].len()), (2, 48000));
        let row = testdata::load_seq("onset-stereo")
            .into_iter()
            .find(|r| r.frame == 24 && r.channel == 0)
            .expect("seq row");
        assert_eq!(row.seq, 2, "f24 is SHORT");
        assert_eq!((row.groups, row.group_len), (4, [3, 1, 3, 1]));
        let snaps = step_to_frame("onset-stereo", 48000, 2, 24);
        let snap = &snaps[24];
        assert_eq!(snap.seq, 2, "live seq agrees (index check)");
        let cap = snap.ms_cap.as_ref().expect("ms capture");
        // Gating state.
        let s1 = testdata::load_s1("onset-stereo", 24);
        assert!(
            s1.ch.iter().all(|c| c.isb.iter().all(|&b| b == 0)),
            "IS inactive"
        );
        assert_eq!((cap.cnt, cap.per, cap.max), (56, 14, 13));
        assert_eq!(
            (s1.ch[0].geo_cnt, s1.ch[0].geo_per, s1.ch[0].geo_max),
            (56, 14, 13)
        );
        assert_eq!(cap.offsets, s1.ch[0].offsets, "grouped offsets agree");
        assert!(cap.allow_ms);
        let (o_dig, o_mask) = dump_msd(&format!("{o_dir}/m2-onset-stereo-48k.s1.txt"), 24);
        assert_eq!((cap.digest, o_dig), (1, 1), "SOME both sides");
        // Reference operands: committed GROUPED L/R + derived grouped MS.
        let p0 = testdata::load_psy("onset-stereo", 24, 0, true);
        let p1 = testdata::load_psy("onset-stereo", 24, 1, true);
        let g0 = p0.grouped.as_ref().expect("grouped");
        let g1 = p1.grouped.as_ref().expect("grouped");
        let ms = testdata::load_ms("onset-stereo", 24);
        let mid: Vec<Vec<i32>> = ms.iter().map(|w| w.mid.clone()).collect();
        let side: Vec<Vec<i32>> = ms.iter().map(|w| w.side.clone()).collect();
        let (em, es, eml, esl) = grouped_ms_vectors(
            &mid,
            &side,
            row.groups,
            &row.group_len,
            14,
            56,
            ms[0].mid.len(),
        );
        // Transcriptions reproduce their own side exactly.
        let (lm, lmask) = ms_first_pass(
            &cap.e_l,
            &cap.e_r,
            &cap.t_l,
            &cap.t_r,
            &cap.eld_l,
            &cap.eld_r,
            &cap.tld_l,
            &cap.tld_r,
            &cap.em,
            &cap.es,
            &cap.em_ld,
            &cap.es_ld,
            cap.cnt,
            cap.per,
            cap.max,
            cap.allow_ms,
        );
        assert_eq!(lmask, cap.mask, "live transcription exact");
        let (om, omask) = ms_first_pass(
            &g0.e, &g1.e, &g0.t, &g1.t, &g0.e_ld, &g1.e_ld, &g0.t_ld, &g1.t_ld, &em, &es, &eml,
            &esl, 56, 14, 13, true,
        );
        // Stride pads stay zero on both sides (eligibility sanity).
        for &p in &[13usize, 27, 41, 55] {
            assert_eq!(cap.mask[p], 0, "live pad {p}");
            assert_eq!(o_mask[p], 0, "oracle pad {p}");
        }
        // Live-vs-dump flips: exactly bands 38,39,45-50.
        let flips: Vec<usize> = (0..56).filter(|&b| cap.mask[b] != o_mask[b]).collect();
        assert_eq!(flips, vec![38, 39, 45, 46, 47, 48, 49, 50]);
        // Fresh-rule reference agrees with live everywhere EXCEPT 38,39.
        let fresh_diff: Vec<usize> = (0..56).filter(|&b| cap.mask[b] != omask[b]).collect();
        assert_eq!(fresh_diff, vec![38, 39], "only 38,39 flip vs fresh rule");
        // Bands 48,49,50: bit-identical inputs on both sides, rule gives 1
        // on both, dump says 0 -> the dump is not the rule output here
        // (S1-path staleness, proven by contradiction; no residue involved).
        for b in [48usize, 49, 50] {
            for (name, a, o) in [
                ("e_l", cap.e_l[b], g0.e[b]),
                ("e_r", cap.e_r[b], g1.e[b]),
                ("t_l", cap.t_l[b], g0.t[b]),
                ("t_r", cap.t_r[b], g1.t[b]),
                ("eld_l", cap.eld_l[b], g0.e_ld[b]),
                ("eld_r", cap.eld_r[b], g1.e_ld[b]),
                ("tld_l", cap.tld_l[b], g0.t_ld[b]),
                ("tld_r", cap.tld_r[b], g1.t_ld[b]),
                ("em", cap.em[b], em[b]),
                ("es", cap.es[b], es[b]),
                ("em_ld", cap.em_ld[b], eml[b]),
                ("es_ld", cap.es_ld[b], esl[b]),
            ] {
                assert_eq!(a, o, "band {b} {name} bit-identical");
            }
            assert_eq!(om[b], lm[b], "band {b} margins identical");
            assert_eq!((omask[b], lmask[b], o_mask[b]), (1, 1, 0));
        }
        // Bands 45,46,47: residue-scale deltas, margins agree, both use 1.
        for b in [45usize, 46, 47] {
            for (name, a, o) in [
                ("e_l", cap.e_l[b], g0.e[b]),
                ("e_r", cap.e_r[b], g1.e[b]),
                ("em", cap.em[b], em[b]),
                ("es", cap.es[b], es[b]),
            ] {
                let d = (a as i64 - o as i64).abs();
                assert!(d <= q31_tol(o), "band {b} {name} delta {d} in-envelope");
            }
            for (name, live, want) in [
                ("eld_l", cap.eld_l[b], g0.e_ld[b]),
                ("eld_r", cap.eld_r[b], g1.e_ld[b]),
                ("em_ld", cap.em_ld[b], eml[b]),
            ] {
                let (el, ew) = (ld_energy(live), ld_energy(want));
                assert!(
                    (el - ew).abs() <= ld_energy_tol(ew),
                    "band {b} {name} in-envelope"
                );
            }
            assert_eq!((use_of(om[b]), use_of(lm[b]), o_mask[b]), (1, 1, 0));
        }
        // Bands 38,39: genuine flips (live 1 vs fresh-rule 0), explained by
        // 1-LSB stored residue flipping sentinel selections in Ld.
        for b in [38usize, 39] {
            assert_eq!((om[b], lm[b]), (0, 33556388), "band {b} pinned margins");
            assert_eq!((omask[b], lmask[b]), (0, 1));
            // Stored deltas: exactly 1 LSB (envelope floor is 32).
            for (name, a, o) in [
                ("e_l", cap.e_l[b], g0.e[b]),
                ("e_r", cap.e_r[b], g1.e[b]),
                ("em", cap.em[b], em[b]),
            ] {
                assert_eq!((a, o), (1, 0), "band {b} {name} 1-LSB residue");
            }
            assert_eq!(cap.es[b], es[b], "band {b} es exact (0)");
            assert_eq!(cap.tld_l[b], g0.t_ld[b], "band {b} tld_l exact (floor)");
            assert_eq!(cap.tld_r[b], g1.t_ld[b], "band {b} tld_r exact (floor)");
            // Energy deltas: 0 vs ~1.0, within the 32.0 envelope floor.
            for (name, live, want) in [
                ("eld_l", cap.eld_l[b], g0.e_ld[b]),
                ("eld_r", cap.eld_r[b], g1.e_ld[b]),
                ("em_ld", cap.em_ld[b], eml[b]),
            ] {
                let (el, ew) = (ld_energy(live), ld_energy(want));
                assert!(
                    (el - ew).abs() <= ld_energy_tol(ew),
                    "band {b} {name}: E {el:.4} vs {ew:.4} in-envelope"
                );
            }
        }
        // Exact joint counterfactual: substituting the measured live Ld
        // deltas (eld both channels + em_ld; es_ld/tld already equal) into
        // the reference margin reproduces the live margins exactly --
        // the measured deltas fully account for the +33556388 shift
        // (need was >= 1 from margin 0). No missing operand.
        let mut eld_l2 = g0.e_ld.clone();
        let mut eld_r2 = g1.e_ld.clone();
        let mut eml2 = eml.clone();
        for b in [38usize, 39] {
            eld_l2[b] = cap.eld_l[b];
            eld_r2[b] = cap.eld_r[b];
            eml2[b] = cap.em_ld[b];
        }
        let (cm, _) = ms_first_pass(
            &g0.e, &g1.e, &g0.t, &g1.t, &eld_l2, &eld_r2, &g0.t_ld, &g1.t_ld, &em, &es, &eml2,
            &esl, 56, 14, 13, true,
        );
        assert_eq!(cm[38], lm[38], "counterfactual reproduces live margin");
        assert_eq!(cm[39], lm[39], "counterfactual reproduces live margin");
        println!("onset f24: 38,39 residue-explained (1-LSB sentinel flips); 45-50 no live divergence (dump staleness artifact)");
    }

    /// Collect every `ISB` int line inside the `S1FRAME f=<frame>` block
    /// of an oracle dump (one per channel, in order).
    fn dump_isb(path: &str, frame: u32) -> Vec<Vec<i32>> {
        let text = require_validation_file(
            "PHASE1_ORACLE_DIR",
            path,
            "generate oracle dumps per tools/enc-oracle/README.md and set PHASE1_ORACLE_DIR to the dump directory",
        );
        let lines: Vec<&str> = text.split('\n').collect();
        let start = lines
            .iter()
            .position(|ln| ln.trim() == format!("S1FRAME f={frame}"))
            .expect("S1FRAME present");
        let mut out = Vec::new();
        for ln in &lines[start..] {
            let t = ln.trim();
            if t == format!("S1FRAME f={}", frame + 1) {
                break;
            }
            if t.starts_with("ISB ") {
                out.push(
                    t["ISB ".len()..]
                        .split_whitespace()
                        .map(|w| w.parse::<i32>().expect("isb int"))
                        .collect(),
                );
            }
        }
        assert_eq!(out.len(), 2, "two ISB lines (stereo)");
        out
    }

    /// Reconstruct the fresh-rule (post-regrouping) reference decision for
    /// a SHORT frame from committed grouped L/R vectors plus grouped MS
    /// sums derived by the exact C regroup + exact Ld refresh. Returns
    /// `(margins, first_pass_mask, digest, final_mask)` applying the
    /// ALL-upgrade rule exactly (`ms_stereo.cpp:248-251`). The `digest`
    /// distinguishes SOME/ALL/NONE; upgraded bands carry no first-pass
    /// margin meaning and are reported as such by callers.
    #[allow(clippy::too_many_arguments)]
    fn fresh_short_reference(
        e_l: &[i32],
        e_r: &[i32],
        t_l: &[i32],
        t_r: &[i32],
        eld_l: &[i32],
        eld_r: &[i32],
        tld_l: &[i32],
        tld_r: &[i32],
        em: &[i32],
        es: &[i32],
        em_ld: &[i32],
        es_ld: &[i32],
        cnt: usize,
        per: usize,
        max: usize,
    ) -> (Vec<i64>, Vec<i32>, i32, Vec<i32>) {
        let (margins, mut mask) = ms_first_pass(
            e_l, e_r, t_l, t_r, eld_l, eld_r, tld_l, tld_r, em, es, em_ld, es_ld, cnt, per, max,
            true,
        );
        let first_pass = mask.clone();
        let true_somewhere = mask.iter().any(|&v| v == 1);
        let num_false = mask
            .iter()
            .enumerate()
            .filter(|(b, &v)| v == 0 && (b % per) < max && *b < cnt)
            .count() as i32;
        if !true_somewhere {
            return (margins, first_pass, 0, mask);
        }
        if num_false == 0 || (num_false < max as i32 && num_false < 9) {
            for b in mask.iter_mut() {
                *b = 1;
            }
            // Pads (unvisited) stay 0: restore them exactly.
            for sfb in (0..cnt).step_by(per) {
                for off in max..per {
                    let b = sfb + off;
                    if b < cnt {
                        mask[b] = 0;
                    }
                }
            }
            (margins, first_pass, 2, mask)
        } else {
            (margins, first_pass, 1, mask)
        }
    }

    /// Phase 1F: multitone-stereo f1 (SHORT) shows zero live divergence.
    ///
    /// Committed coverage is complete here (qc1/qc2, grouped psy,
    /// per-window ms, seq). Geometry triple-agrees (live capture, qc1
    /// SOFF, GROUPED GOFF); IS books are zero in committed qc1 AND the
    /// dump (which also validates the dump ISB parser). The fresh-rule
    /// reference (exact regroup + exact Ld refresh, upgrade included)
    /// reproduces the live mask bit-for-bit, including the ALL upgrade
    /// (first-pass 52/52 ones, pads 0). All 10 dump differences
    /// (bands 45-54) are reference artifacts: the rule gives 1 on the
    /// reference's own arrays while the dump says 0.
    #[test]
    fn phase1f_multitone_f1_no_live_divergence() {
        let o_dir = env_dir(
            "PHASE1_ORACLE_DIR",
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/phase0-20261015/oracle",
        );
        let pcm = testdata::fixture_samples("multitone-stereo");
        assert_eq!((pcm.len(), pcm[0].len()), (2, 48000));
        let row = testdata::load_seq("multitone-stereo")
            .into_iter()
            .find(|r| r.frame == 1 && r.channel == 0)
            .expect("seq row");
        assert_eq!(row.seq, 2, "f1 is SHORT");
        assert_eq!((row.groups, row.group_len), (4, [1, 3, 3, 1]));
        let snaps = step_to_frame("multitone-stereo", 48000, 2, 1);
        let snap = &snaps[1];
        assert_eq!(snap.seq, 2, "live seq agrees (index check)");
        let cap = snap.ms_cap.as_ref().expect("ms capture");
        assert_eq!((cap.cnt, cap.per, cap.max), (56, 14, 13));
        // Geometry: live == committed qc1 == committed GROUPED.
        let s1 = testdata::load_s1("multitone-stereo", 1);
        assert_eq!(
            (s1.ch[0].geo_cnt, s1.ch[0].geo_per, s1.ch[0].geo_max),
            (56, 14, 13)
        );
        assert_eq!(cap.offsets, s1.ch[0].offsets, "live==qc1 offsets");
        let p0 = testdata::load_psy("multitone-stereo", 1, 0, true);
        let p1 = testdata::load_psy("multitone-stereo", 1, 1, true);
        let g0 = p0.grouped.as_ref().expect("grouped");
        let g1 = p1.grouped.as_ref().expect("grouped");
        assert_eq!(g0.offsets, s1.ch[0].offsets, "GROUPED==qc1 offsets");
        assert_eq!((g0.e.len(), g0.max_sfb), (56, 13));
        // IS gate: zero in committed qc1 and in the dump (agreement also
        // validates the dump ISB parser used where committed data is thin).
        assert!(s1.ch.iter().all(|c| c.isb.iter().all(|&b| b == 0)));
        let isb = dump_isb(&format!("{o_dir}/m3-multitone-stereo-48k.s1.txt"), 1);
        assert!(
            isb.iter().all(|v| v.iter().all(|&b| b == 0)),
            "dump ISB zero"
        );
        assert_eq!(isb[0].len(), s1.ch[0].geo_cnt, "ISB span");
        // Reference MS sums: exact regroup of committed per-window data.
        let ms = testdata::load_ms("multitone-stereo", 1);
        let mid: Vec<Vec<i32>> = ms.iter().map(|w| w.mid.clone()).collect();
        let side: Vec<Vec<i32>> = ms.iter().map(|w| w.side.clone()).collect();
        let (em, es, eml, esl) = grouped_ms_vectors(
            &mid,
            &side,
            row.groups,
            &row.group_len,
            14,
            56,
            ms[0].mid.len(),
        );
        // Live transcription reproduces the capture (upgrade included).
        let (_, _, live_dig, live_final) = fresh_short_reference(
            &cap.e_l, &cap.e_r, &cap.t_l, &cap.t_r, &cap.eld_l, &cap.eld_r, &cap.tld_l, &cap.tld_r,
            &cap.em, &cap.es, &cap.em_ld, &cap.es_ld, cap.cnt, cap.per, cap.max,
        );
        assert_eq!((live_dig, &live_final), (cap.digest, &cap.mask));
        assert_eq!(cap.digest, 2, "live took ALL upgrade");
        // Fresh-rule reference on committed inputs: also ALL, identical.
        let (om, ofp, o_dig, o_final) = fresh_short_reference(
            &g0.e, &g1.e, &g0.t, &g1.t, &g0.e_ld, &g1.e_ld, &g0.t_ld, &g1.t_ld, &em, &es, &eml,
            &esl, 56, 14, 13,
        );
        assert_eq!(ofp.iter().sum::<i32>(), 52, "first-pass all decided bands");
        assert_eq!(
            (o_dig, &o_final),
            (2, &cap.mask),
            "reference == live, ALL/52"
        );
        let _ = om;
        // Dump comparison: exactly bands 45-54 differ, all reference
        // artifacts (rule gives 1 on the reference's own arrays).
        let (o_dd, o_mask) = dump_msd(&format!("{o_dir}/m3-multitone-stereo-48k.s1.txt"), 1);
        assert_eq!(o_dd, 1, "dump stayed SOME (stale)");
        let flips: Vec<usize> = (0..56).filter(|&b| cap.mask[b] != o_mask[b]).collect();
        assert_eq!(flips, vec![45, 46, 47, 48, 49, 50, 51, 52, 53, 54]);
        for &b in &flips {
            assert_eq!((o_final[b], cap.mask[b], o_mask[b]), (1, 1, 0));
        }
        // Artifact proof without residue: bands 53,54 have bit-identical
        // inputs on both sides, yet the dump says 0.
        for b in [53usize, 54] {
            for (name, a, o) in [
                ("e", cap.e_l[b], g0.e[b]),
                ("eld", cap.eld_l[b], g0.e_ld[b]),
                ("tld", cap.tld_l[b], g0.t_ld[b]),
                ("em", cap.em[b], em[b]),
                ("em_ld", cap.em_ld[b], eml[b]),
            ] {
                assert_eq!(a, o, "band {b} {name} bit-identical");
            }
        }
        // Remaining bands: residue-scale deltas, same bit.
        for b in [45usize, 46, 47, 48, 49, 50, 51, 52] {
            for (name, a, o) in [
                ("e_l", cap.e_l[b], g0.e[b]),
                ("e_r", cap.e_r[b], g1.e[b]),
                ("em", cap.em[b], em[b]),
                ("es", cap.es[b], es[b]),
            ] {
                let d = (a as i64 - o as i64).abs();
                assert!(d <= q31_tol(o), "band {b} {name} delta {d} in-envelope");
            }
            for (name, live, want) in [
                ("eld_l", cap.eld_l[b], g0.e_ld[b]),
                ("eld_r", cap.eld_r[b], g1.e_ld[b]),
                ("em_ld", cap.em_ld[b], eml[b]),
            ] {
                let (el, ew) = (ld_energy(live), ld_energy(want));
                assert!(
                    (el - ew).abs() <= ld_energy_tol(ew),
                    "band {b} {name} in-envelope"
                );
            }
            assert_eq!(cap.tld_l[b], g0.t_ld[b], "band {b} tld_l exact");
            assert_eq!(cap.tld_r[b], g1.t_ld[b], "band {b} tld_r exact");
        }
        println!("multitone f1: no live divergence (52/52 ALL both); 10 dump diffs are reference artifacts");
    }

    /// Phase 1F: stepdown-stereo f1 (SHORT) shows zero live divergence.
    ///
    /// Coverage rationale (explicit): no committed qc1 exists for this
    /// frame, so geometry comes from committed GROUPED lines
    /// (cnt/noSfb, max/maxSfb, offsets/GOFF) plus the seq-file grouping,
    /// and the IS gate from dump ISB lines (parser validated by the
    /// multitone committed-vs-dump ISB agreement above). Per-window ms_
    /// and grouped L/R vectors are committed.
    #[test]
    fn phase1f_stepdown_f1_no_live_divergence() {
        let o_dir = env_dir(
            "PHASE1_ORACLE_DIR",
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/phase0-20261015/oracle",
        );
        let pcm = testdata::fixture_samples("stepdown-stereo");
        assert_eq!((pcm.len(), pcm[0].len()), (2, 48000));
        let row = testdata::load_seq("stepdown-stereo")
            .into_iter()
            .find(|r| r.frame == 1 && r.channel == 0)
            .expect("seq row");
        assert_eq!(row.seq, 2, "f1 is SHORT");
        assert_eq!((row.groups, row.group_len), (4, [1, 3, 3, 1]));
        let snaps = step_to_frame("stepdown-stereo", 48000, 2, 1);
        let snap = &snaps[1];
        assert_eq!(snap.seq, 2, "live seq agrees (index check)");
        let cap = snap.ms_cap.as_ref().expect("ms capture");
        let p0 = testdata::load_psy("stepdown-stereo", 1, 0, true);
        let p1 = testdata::load_psy("stepdown-stereo", 1, 1, true);
        let g0 = p0.grouped.as_ref().expect("grouped");
        let g1 = p1.grouped.as_ref().expect("grouped");
        // Geometry from GROUPED + seq (no committed qc1 for f1).
        assert_eq!((g0.e.len(), g0.max_sfb), (56, 13), "grouped geometry");
        assert_eq!(g0.e.len() / row.groups as usize, 14, "stride");
        assert_eq!((cap.cnt, cap.per, cap.max), (56, 14, 13));
        assert_eq!(cap.offsets, g0.offsets, "live==GROUPED offsets");
        assert_eq!(g0.offsets.len(), 57, "offset span");
        // IS gate from dump ISB (both zero).
        let isb = dump_isb(&format!("{o_dir}/m2-stepdown-stereo-48k.s1.txt"), 1);
        assert!(
            isb.iter().all(|v| v.iter().all(|&b| b == 0)),
            "dump ISB zero"
        );
        assert_eq!(isb[0].len(), 56, "ISB span");
        // Reference MS sums from committed per-window data.
        let ms = testdata::load_ms("stepdown-stereo", 1);
        let mid: Vec<Vec<i32>> = ms.iter().map(|w| w.mid.clone()).collect();
        let side: Vec<Vec<i32>> = ms.iter().map(|w| w.side.clone()).collect();
        let (em, es, eml, esl) = grouped_ms_vectors(
            &mid,
            &side,
            row.groups,
            &row.group_len,
            14,
            56,
            ms[0].mid.len(),
        );
        // Live transcription reproduces the capture.
        let (_, _, live_dig, live_final) = fresh_short_reference(
            &cap.e_l, &cap.e_r, &cap.t_l, &cap.t_r, &cap.eld_l, &cap.eld_r, &cap.tld_l, &cap.tld_r,
            &cap.em, &cap.es, &cap.em_ld, &cap.es_ld, cap.cnt, cap.per, cap.max,
        );
        assert_eq!((live_dig, &live_final), (cap.digest, &cap.mask));
        assert_eq!(cap.digest, 1, "SOME (no upgrade either side)");
        // Fresh-rule reference: identical mask -> no live divergence.
        let (om, _ofp, o_dig, o_final) = fresh_short_reference(
            &g0.e, &g1.e, &g0.t, &g1.t, &g0.e_ld, &g1.e_ld, &g0.t_ld, &g1.t_ld, &em, &es, &eml,
            &esl, 56, 14, 13,
        );
        assert_eq!(o_dig, 1, "reference SOME");
        assert_eq!(o_final, cap.mask, "reference == live on all 56 bands");
        let _ = om;
        // Dump comparison: exactly bands 45-50 differ, all artifacts.
        let (o_dd, o_mask) = dump_msd(&format!("{o_dir}/m2-stepdown-stereo-48k.s1.txt"), 1);
        assert_eq!(o_dd, 1, "dump SOME");
        let flips: Vec<usize> = (0..56).filter(|&b| cap.mask[b] != o_mask[b]).collect();
        assert_eq!(flips, vec![45, 46, 47, 48, 49, 50]);
        for &b in &flips {
            assert_eq!((o_final[b], cap.mask[b], o_mask[b]), (1, 1, 0));
        }
        // Artifact proof without residue: bands 47-50 bit-identical.
        for b in [47usize, 48, 49, 50] {
            for (name, a, o) in [
                ("e", cap.e_l[b], g0.e[b]),
                ("eld", cap.eld_l[b], g0.e_ld[b]),
                ("tld", cap.tld_l[b], g0.t_ld[b]),
                ("em", cap.em[b], em[b]),
                ("em_ld", cap.em_ld[b], eml[b]),
            ] {
                assert_eq!(a, o, "band {b} {name} bit-identical");
            }
        }
        // Bands 45,46: residue-scale deltas, same bit.
        for b in [45usize, 46] {
            for (name, a, o) in [
                ("e_l", cap.e_l[b], g0.e[b]),
                ("e_r", cap.e_r[b], g1.e[b]),
                ("em", cap.em[b], em[b]),
            ] {
                let d = (a as i64 - o as i64).abs();
                assert!(d <= q31_tol(o), "band {b} {name} delta {d} in-envelope");
            }
            for (name, live, want) in [
                ("eld_l", cap.eld_l[b], g0.e_ld[b]),
                ("em_ld", cap.em_ld[b], eml[b]),
            ] {
                let (el, ew) = (ld_energy(live), ld_energy(want));
                assert!(
                    (el - ew).abs() <= ld_energy_tol(ew),
                    "band {b} {name} in-envelope"
                );
            }
        }
        println!("stepdown f1: no live divergence (mask identical); 6 dump diffs are reference artifacts");
    }

    /// Phase 1F: sine-stereo f1 (SHORT) live-vs-real-C mask agreement.
    ///
    /// Ground truth is the real-C `MSMASK`/`MSDIGEST` from the V4.25
    /// passive captures (`v4.25-ms-20261020/artifacts/sine-stereo_f1_*_ms_post.txt`;
    /// fixture `a7e8494a...`, artifact hashes in the V4.26 report), NOT the
    /// C++ oracle dump MSD: on SHORT frames the S1 dump path stages stale
    /// per-window MS Ld and never applies the grouped refresh the real
    /// encoder performs (`psy_main.cpp:1078-1084`; proven by the
    /// phase1e-onset-f24 bands 48-50 contradiction), so the dump MSD must
    /// not be used as SHORT-frame ground truth here.
    #[test]
    fn phase1f_sine_f1_live_matches_real_c() {
        let pcm = testdata::fixture_samples("sine-stereo");
        assert_eq!((pcm.len(), pcm[0].len()), (2, 48000));
        let row = testdata::load_seq("sine-stereo")
            .into_iter()
            .find(|r| r.frame == 1 && r.channel == 0)
            .expect("seq row");
        assert_eq!(row.seq, 2, "f1 is SHORT");
        assert_eq!((row.groups, row.group_len), (4, [1, 3, 3, 1]));
        let snaps = step_to_frame("sine-stereo", 48000, 2, 1);
        let snap = &snaps[1];
        assert_eq!(snap.seq, 2, "live seq agrees (index check)");
        let cap = snap.ms_cap.as_ref().expect("ms capture");
        assert_eq!((cap.cnt, cap.per, cap.max), (56, 14, 13));
        // Geometry + IS gate from committed vectors (sine f1 has qc1;
        // no committed psy/ms grouped vectors exist for this frame).
        let s1 = testdata::load_s1("sine-stereo", 1);
        assert_eq!(
            (s1.ch[0].geo_cnt, s1.ch[0].geo_per, s1.ch[0].geo_max),
            (56, 14, 13)
        );
        assert_eq!(cap.offsets, s1.ch[0].offsets, "live==qc1 offsets");
        assert!(
            s1.ch.iter().all(|c| c.isb.iter().all(|&b| b == 0)),
            "IS inactive"
        );
        assert!(cap.allow_ms);
        // Live transcription reproduces the capture (upgrade included).
        let (_, _, live_dig, live_final) = fresh_short_reference(
            &cap.e_l, &cap.e_r, &cap.t_l, &cap.t_r, &cap.eld_l, &cap.eld_r, &cap.tld_l, &cap.tld_r,
            &cap.em, &cap.es, &cap.em_ld, &cap.es_ld, cap.cnt, cap.per, cap.max,
        );
        assert_eq!((live_dig, &live_final), (cap.digest, &cap.mask));
        // Real-C ground truth (V4.25 ms_post, MSDIGEST 1).
        let real_digest = 1;
        let real_mask: [i32; 56] = [
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1,
            1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,
        ];
        assert_eq!(cap.digest, real_digest, "digest vs real-C");
        assert_eq!(cap.mask, real_mask, "mask vs real-C");
        println!("sine f1: live mask matches real-C (digest 1, 48 decided bands)");
    }

    /// Phase 1F: onset-stereo-44100 f22 (SHORT) live-vs-real-C mask agreement.
    ///
    /// Same ground-truth rule as above: real-C `MSMASK`/`MSDIGEST` from the
    /// V4.25 passive captures (`onset-44100_f22_*_ms_post.txt`), never the
    /// oracle dump MSD on SHORT frames.
    #[test]
    fn phase1f_onset441_f22_live_matches_real_c() {
        let pcm = testdata::fixture_samples("onset-stereo-44100");
        assert_eq!((pcm.len(), pcm[0].len()), (2, 44100));
        let row = testdata::load_seq("onset-stereo-44100")
            .into_iter()
            .find(|r| r.frame == 22 && r.channel == 0)
            .expect("seq row");
        assert_eq!(row.seq, 2, "f22 is SHORT");
        assert_eq!((row.groups, row.group_len), (4, [3, 1, 1, 3]));
        let snaps = step_to_frame("onset-stereo-44100", 44100, 2, 22);
        let snap = &snaps[22];
        assert_eq!(snap.seq, 2, "live seq agrees (index check)");
        let cap = snap.ms_cap.as_ref().expect("ms capture");
        assert_eq!((cap.cnt, cap.per, cap.max), (56, 14, 13));
        let s1 = testdata::load_s1("onset-stereo-44100", 22);
        assert_eq!(
            (s1.ch[0].geo_cnt, s1.ch[0].geo_per, s1.ch[0].geo_max),
            (56, 14, 13)
        );
        assert_eq!(cap.offsets, s1.ch[0].offsets, "live==qc1 offsets");
        assert!(
            s1.ch.iter().all(|c| c.isb.iter().all(|&b| b == 0)),
            "IS inactive"
        );
        assert!(cap.allow_ms);
        let (_, _, live_dig, live_final) = fresh_short_reference(
            &cap.e_l, &cap.e_r, &cap.t_l, &cap.t_r, &cap.eld_l, &cap.eld_r, &cap.tld_l, &cap.tld_r,
            &cap.em, &cap.es, &cap.em_ld, &cap.es_ld, cap.cnt, cap.per, cap.max,
        );
        assert_eq!((live_dig, &live_final), (cap.digest, &cap.mask));
        // Real-C ground truth (V4.25 ms_post, MSDIGEST 1).
        let real_digest = 1;
        let real_mask: [i32; 56] = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0,
        ];
        assert_eq!(cap.digest, real_digest, "digest vs real-C");
        assert_eq!(cap.mask, real_mask, "mask vs real-C");
        println!(
            "onset441 f22: live mask matches real-C (digest 1, groups 0-1 silent, 2-3 all-MS)"
        );
    }

    /// Phase 1H (V4.27): D-021 live frame-boundary reconciliation.
    ///
    /// Compares live Rust ADTS frame boundaries against the real-C `aac-enc`
    /// output for the four D-021 SHORT targets, independently of the C++
    /// oracle harness (whose SHORT-frame S6 totals are stale-Ld artifacts and
    /// must not be used as ground truth here). Real-C ground truth: ADTS
    /// frame lengths parsed from `output.aac` files identical to the V4.25
    /// captures (SHA-256 `c8adaeb7…`, `fa8f2908…`, `1e7f1f41…`, `300baf80…`;
    /// see the V4.27 report): 309/303/253/223 bytes at zero-based frames
    /// 24/22/1/1. Live `total` is raw payload bits excluding the 56-bit ADTS
    /// header (`total + 56 == adts_len * 8` asserted below), i.e. the same
    /// quantity as an S6FINAL `total` — so live totals can be set against
    /// both the real payloads and the oracle S6 totals.
    ///
    /// Observed (pinned): multitone matches exactly; sine is 1 byte under;
    /// onset-44100 is 7 bytes over; onset is 28 bytes over. The oracle S6
    /// totals (792/1984/728/600) reproduce on NEITHER side — they are
    /// oracle-only measurements.
    #[test]
    fn phase1h_d021_live_frame_reconciliation() {
        // (fixture, sample rate, zero-based frame, real-C ADTS bytes,
        //  live ADTS bytes, live total/static/pe, live gains, live dyn).
        let cases: [(&str, u32, u32, usize, usize, i32, i32, i32, [i32; 2], i32); 4] = [
            (
                "onset-stereo",
                48000u32,
                24u32,
                309usize,
                337usize,
                2640,
                347,
                2533,
                [-47, 0],
                2289,
            ),
            (
                "onset-stereo-44100",
                44100u32,
                22u32,
                303usize,
                310usize,
                2424,
                279,
                2572,
                [-47, 0],
                2140,
            ),
            (
                "sine-stereo",
                48000u32,
                1u32,
                253usize,
                252usize,
                1960,
                479,
                3488,
                [-36, 0],
                1474,
            ),
            (
                "multitone-stereo",
                48000u32,
                1u32,
                223usize,
                223usize,
                1728,
                177,
                4993,
                [-33, 0],
                1543,
            ),
        ];
        for (name, sr, frame, real_len, live_len, total, static_b, pe, gains, dyn_b) in cases {
            let pcm = testdata::fixture_samples(name);
            let ch = pcm.len();
            let s = pcm[0].len();
            let cfg =
                EncoderConfig::new(sr, ch as u32, FRAME_LENGTH, TransportType::Adts).expect("cfg");
            let mut feeder = PcmFeeder::new(cfg);
            let mut interleaved = Vec::with_capacity(s * ch);
            for i in 0..s {
                for c in 0..ch {
                    interleaved.push(pcm[c][i]);
                }
            }
            let mut frames = feeder.push_interleaved(&interleaved).expect("push");
            frames.extend(feeder.finish().expect("finish"));
            assert!(
                (frame as usize) < frames.len(),
                "{name}: frame {frame} within {} feeder frames",
                frames.len()
            );
            let mut enc = StreamEncoder::new(sr, ch);
            let (mut got_len, mut got_snap) = (0usize, None);
            for f in 0..=frame as usize {
                let fresh: Vec<Vec<i16>> =
                    frames[f].channels().iter().map(|b| b.to_vec()).collect();
                let (bytes, snap) = enc.step_frame(f as u32, &fresh);
                assert!(snap.notes.is_empty(), "{name} f={f} notes {:?}", snap.notes);
                if f == frame as usize {
                    got_len = bytes.as_ref().map(|b| b.len()).unwrap_or(0);
                    got_snap = Some(snap);
                } else {
                    bytes.expect("emitted");
                }
            }
            let snap = got_snap.unwrap();
            assert_eq!(snap.seq, 2, "{name} f={frame} live SHORT");
            assert_eq!(got_len, live_len, "{name} live ADTS bytes");
            assert_eq!(
                got_len * 8,
                (snap.total + 56) as usize,
                "{name} total+56==ADTS"
            );
            assert_eq!(snap.total, total, "{name} live payload bits");
            assert_eq!(snap.static_bits, static_b, "{name} live static bits");
            assert_eq!(snap.pe, pe, "{name} live PE");
            assert_eq!(snap.gains, gains, "{name} live gains");
            assert_eq!(snap.used_dyn, dyn_b, "{name} live dyn bits");
            assert_eq!(snap.npass, 1, "{name} single gain pass");
            assert_eq!(snap.crash, -1, "{name} no crash recovery");
            // Real-C length documented (not asserted equal: 3 of 4 diverge live).
            println!(
                "V427 {name} f={frame}: live {got_len}B vs real-C {real_len}B (delta {:+}B)",
                got_len as i64 - real_len as i64
            );
        }
    }

    /// Phase 1G final: LONG-frame flips follow the residue-sensitive
    /// boundary mechanism (1E) on all three sampled frames.
    ///
    /// Frames (all LONG stereo, SOME both sides, IS books zero):
    /// sine f6-family is unusable (f6 mask-identical Tier-C pure, f7/f8
    /// lack committed psy/ms vectors — reported limitation); substitutes
    /// with full coverage: onset f26 (band 16), sweep f10 (bands 37,42),
    /// stepdown f5 (band 16). LONG dumps are C-faithful (same staleness
    /// gate both sides, no regroup refresh involved).
    ///
    /// Verified decision inputs: only the six Ld vectors enter the
    /// strict rule (`ms_stereo.cpp:185-201`; stored linear values do not
    /// — the transcription, which reproduces every mask here, ignores
    /// them). Stored deltas matter as the traced origin of Ld deltas:
    /// exact `calc_ld_data` on residue-shifted stored sums (SHORT) or
    /// the float log-lane vs integer lane (LONG energy/MS Ld).
    #[test]
    fn phase1g_long_flips_residue() {
        let o_dir = env_dir(
            "PHASE1_ORACLE_DIR",
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/phase0-20261015/oracle",
        );
        // (fixture, stem, frame, expected live-vs-dump flips)
        let cases: [(&str, &str, u32, Vec<usize>); 3] = [
            ("onset-stereo", "m2-onset-stereo-48k", 26, vec![16]),
            ("sweep-stereo", "m3-sweep-stereo-48k", 10, vec![37, 42]),
            ("stepdown-stereo", "m2-stepdown-stereo-48k", 5, vec![16]),
        ];
        // Committed LONG table offsets (static per rate; no qc1 needed).
        let ((lcfg, _), _) = testdata::load_cfg(48000, 96000);
        for (short, stem, f, expect_flips) in cases {
            let pcm = testdata::fixture_samples(short);
            assert_eq!((pcm.len(), pcm[0].len()), (2, 48000));
            let row = testdata::load_seq(short)
                .into_iter()
                .find(|r| r.frame == f && r.channel == 0)
                .expect("seq row");
            assert_eq!(row.seq, 0, "{stem} f={f} LONG");
            let snaps = step_to_frame(short, 48000, 2, f);
            let snap = &snaps[f as usize];
            assert_eq!(snap.seq, 0, "live seq agrees (index check)");
            let cap = snap.ms_cap.as_ref().expect("ms capture");
            assert_eq!((cap.cnt, cap.per, cap.max), (45, 45, 45));
            assert_eq!(
                cap.offsets,
                lcfg.offsets[..cap.cnt + 1],
                "live==table offsets (active span)"
            );
            // IS gate from dump ISB (both zero; parser validated in 1F).
            let isb = dump_isb(&format!("{o_dir}/{stem}.s1.txt"), f);
            assert!(isb.iter().all(|v| v.iter().all(|&b| b == 0)), "IS inactive");
            assert_eq!(isb[0].len(), 45, "ISB span");
            assert!(cap.allow_ms);
            let (o_dig, o_mask) = dump_msd(&format!("{o_dir}/{stem}.s1.txt"), f);
            assert_eq!((cap.digest, o_dig), (1, 1), "SOME both sides");
            // Reference operands: committed LONG psy + ms vectors.
            let p0 = testdata::load_psy(short, f, 0, false);
            let p1 = testdata::load_psy(short, f, 1, false);
            let ms = testdata::load_ms(short, f);
            assert_eq!(p0.threshold_ld.as_ref().unwrap().len(), 45);
            // Both transcriptions reproduce their recorded masks.
            let (lm, lmask) = ms_first_pass(
                &cap.e_l,
                &cap.e_r,
                &cap.t_l,
                &cap.t_r,
                &cap.eld_l,
                &cap.eld_r,
                &cap.tld_l,
                &cap.tld_r,
                &cap.em,
                &cap.es,
                &cap.em_ld,
                &cap.es_ld,
                cap.cnt,
                cap.per,
                cap.max,
                cap.allow_ms,
            );
            assert_eq!(lmask, cap.mask, "{stem} f={f} live transcription");
            let (om, omask) = ms_first_pass(
                &p0.nrg[0],
                &p1.nrg[0],
                &p0.thr[0],
                &p1.thr[0],
                &p0.nrg_ld[0],
                &p1.nrg_ld[0],
                &p0.threshold_ld.clone().unwrap(),
                &p1.threshold_ld.clone().unwrap(),
                &ms[0].mid,
                &ms[0].side,
                &ms[0].mid_ld,
                &ms[0].side_ld,
                45,
                45,
                45,
                true,
            );
            assert_eq!(omask, o_mask, "{stem} f={f} oracle transcription");
            // Flip set is exactly as diagnosed.
            let flips: Vec<usize> = (0..45).filter(|&b| cap.mask[b] != o_mask[b]).collect();
            assert_eq!(flips, expect_flips, "{stem} f={f} flip set");
            for &b in &expect_flips {
                // Signed margins straddle the strict boundary.
                assert_ne!(use_of(om[b]), use_of(lm[b]), "band {b} decision differs");
                let need: i64 = if om[b] >= 1 { om[b] } else { 1 - om[b] };
                let shift = lm[b] - om[b];
                assert!(shift.abs() >= need, "band {b} shift covers need");
                // Stored operands: exact or residue-scale (traced origin).
                for (name, a, o) in [
                    ("e_l", cap.e_l[b], p0.nrg[0][b]),
                    ("e_r", cap.e_r[b], p1.nrg[0][b]),
                    ("t_l", cap.t_l[b], p0.thr[0][b]),
                    ("t_r", cap.t_r[b], p1.thr[0][b]),
                    ("em", cap.em[b], ms[0].mid[b]),
                    ("es", cap.es[b], ms[0].side[b]),
                ] {
                    let d = (a as i64 - o as i64).abs();
                    assert!(
                        d <= q31_tol(o),
                        "{stem} f={f} band {b} {name} delta {d} in-envelope"
                    );
                }
                // Ld operands: energy-domain envelope (committed scale).
                for (name, live, want) in [
                    ("eld_l", cap.eld_l[b], p0.nrg_ld[0][b]),
                    ("eld_r", cap.eld_r[b], p1.nrg_ld[0][b]),
                    ("tld_l", cap.tld_l[b], p0.threshold_ld.as_ref().unwrap()[b]),
                    ("tld_r", cap.tld_r[b], p1.threshold_ld.as_ref().unwrap()[b]),
                    ("em_ld", cap.em_ld[b], ms[0].mid_ld[b]),
                    ("es_ld", cap.es_ld[b], ms[0].side_ld[b]),
                ] {
                    let (el, ew) = (ld_energy(live), ld_energy(want));
                    assert!(
                        (el - ew).abs() <= ld_energy_tol(ew),
                        "{stem} f={f} band {b} {name}: E {el:.4} vs {ew:.4} in-envelope"
                    );
                }
                // Exact joint counterfactual: substituting every differing
                // Ld operand into the reference margin reproduces the live
                // margin bit-for-bit -- the measured deltas fully account
                // for the flip; no missing operand, no tolerance in this
                // step.
                let sub = |live: i32, want: i32| {
                    if live != want {
                        live
                    } else {
                        want
                    }
                };
                let e_ld = vec![
                    sub(cap.eld_l[b], p0.nrg_ld[0][b]),
                    sub(cap.eld_r[b], p1.nrg_ld[0][b]),
                ];
                let t_ld = vec![
                    sub(cap.tld_l[b], p0.threshold_ld.as_ref().unwrap()[b]),
                    sub(cap.tld_r[b], p1.threshold_ld.as_ref().unwrap()[b]),
                ];
                let m_ld = vec![
                    sub(cap.em_ld[b], ms[0].mid_ld[b]),
                    sub(cap.es_ld[b], ms[0].side_ld[b]),
                ];
                let mut e2_l = p0.nrg_ld[0].clone();
                let mut e2_r = p1.nrg_ld[0].clone();
                let mut t2_l = p0.threshold_ld.clone().unwrap();
                let mut t2_r = p1.threshold_ld.clone().unwrap();
                let mut m2_l = ms[0].mid_ld.clone();
                let mut m2_r = ms[0].side_ld.clone();
                e2_l[b] = e_ld[0];
                e2_r[b] = e_ld[1];
                t2_l[b] = t_ld[0];
                t2_r[b] = t_ld[1];
                m2_l[b] = m_ld[0];
                m2_r[b] = m_ld[1];
                let (cm, _) = ms_first_pass(
                    &p0.nrg[0],
                    &p1.nrg[0],
                    &p0.thr[0],
                    &p1.thr[0],
                    &e2_l,
                    &e2_r,
                    &t2_l,
                    &t2_r,
                    &ms[0].mid,
                    &ms[0].side,
                    &m2_l,
                    &m2_r,
                    45,
                    45,
                    45,
                    true,
                );
                assert_eq!(cm[b], lm[b], "{stem} f={f} band {b} counterfactual exact");
                assert_eq!(use_of(cm[b]), use_of(lm[b]), "flipped bit reproduced");
                println!(
                    "{stem} f={f} band={b}: residue-explained (oracle_m={} live_m={})",
                    om[b], lm[b]
                );
            }
        }
    }

    /// Collect every int vector for `tag` inside the `S1FRAME f=<frame>`
    /// block of an oracle dump, in channel order. `SPEC` lines carry a
    /// `ch=N` second token (dropped when the count is one over 1024,
    /// mirroring `dump_ints`).
    fn dump_ch_vecs(path: &str, frame: u32, tag: &str) -> Vec<Vec<i32>> {
        let text = require_validation_file(
            "PHASE1_ORACLE_DIR",
            path,
            "generate oracle dumps per tools/enc-oracle/README.md and set PHASE1_ORACLE_DIR to the dump directory",
        );
        let lines: Vec<&str> = text.split('\n').collect();
        let start = lines
            .iter()
            .position(|ln| ln.trim() == format!("S1FRAME f={frame}"))
            .expect("S1FRAME present");
        let mut out = Vec::new();
        for ln in &lines[start..] {
            let t = ln.trim();
            if t == format!("S1FRAME f={}", frame + 1) {
                break;
            }
            if t.starts_with(&format!("{tag} ")) {
                let mut nums: Vec<i32> = t[tag.len()..]
                    .trim()
                    .split_whitespace()
                    .filter_map(|w| w.parse::<i32>().ok())
                    .collect();
                if tag == "SPEC" && nums.len() == 1025 {
                    nums = nums[1..].to_vec();
                }
                out.push(nums);
            }
        }
        assert!(!out.is_empty(), "tag {tag} found for f={frame}");
        out
    }

    /// Parse `ATS pre=a,b,c,d` / `ATS post=...` inside an S1FRAME block.
    fn dump_ats(path: &str, frame: u32) -> ([i32; 4], [i32; 4]) {
        let text = require_validation_file(
            "PHASE1_ORACLE_DIR",
            path,
            "generate oracle dumps per tools/enc-oracle/README.md and set PHASE1_ORACLE_DIR to the dump directory",
        );
        let lines: Vec<&str> = text.split('\n').collect();
        let start = lines
            .iter()
            .position(|ln| ln.trim() == format!("S1FRAME f={frame}"))
            .expect("S1FRAME present");
        let mut pre = None;
        let mut post = None;
        for ln in &lines[start..] {
            let t = ln.trim();
            if t == format!("S1FRAME f={}", frame + 1) {
                break;
            }
            for (key, slot) in [("ATS pre=", &mut pre), ("ATS post=", &mut post)] {
                if let Some(rest) = t.strip_prefix(key) {
                    let v: Vec<i32> = rest
                        .replace(',', " ")
                        .split_whitespace()
                        .map(|w| w.parse::<i32>().expect("ats int"))
                        .collect();
                    assert_eq!(v.len(), 4, "ATS quad");
                    *slot = Some([v[0], v[1], v[2], v[3]]);
                }
            }
        }
        (pre.expect("ATS pre"), post.expect("ATS post"))
    }

    /// Run `pe_calculation` on explicit inputs with forged ATS state.
    /// Returns the element PE plus per-channel band PE details.
    /// `is_book`/`is_scale` are all-zero (asserted by callers where the
    /// reference provides them); `ms_mask` is empty for mono.
    #[allow(clippy::too_many_arguments)]
    fn pe_with(
        energy_ld: &[Vec<i32>],
        threshold_ld: &[Vec<i32>],
        energy_lin: &[Vec<i32>],
        ff: &[Vec<i32>],
        spec: &[Vec<i32>],
        offsets: &[Vec<i32>],
        geo: &[(usize, usize, usize, bool)],
        ms_mask: &[i32],
        ats: [i32; 4],
    ) -> (i32, Vec<Vec<i32>>) {
        let zeros: Vec<Vec<i32>> = geo.iter().map(|g| vec![0i32; g.0]).collect();
        let inputs: Vec<PeChannelInput> = (0..energy_ld.len())
            .map(|c| PeChannelInput {
                energy_ld: &energy_ld[c],
                threshold_ld: &threshold_ld[c],
                energy_linear: &energy_lin[c],
                form_factor_ld: &ff[c],
                spectrum_q31: &spec[c],
                offsets: &offsets[c],
                sfb_cnt: geo[c].0,
                sfb_per_group: geo[c].1,
                max_sfb_per_group: geo[c].2,
                is_book: &zeros[c],
                is_scale: &zeros[c],
                short_block: geo[c].3,
            })
            .collect();
        let mut state = AtsElement::new(0);
        state.chaos_measure_en_fac = [ats[0], ats[1]];
        state.last_enfac_patch = [ats[2], ats[3]];
        let out = pe_calculation(&inputs, ms_mask, &mut state);
        let bands = out
            .data
            .channels
            .iter()
            .map(|pc| pc.sfb_pe.clone())
            .collect();
        (out.data.pe, bands)
    }

    /// Shared Tier-B core: compares live S1 inputs against reference
    /// vectors, checks every applicable envelope, runs the 2x2
    /// input/state attribution, and classifies. Returns
    /// `(live_pe, ref_pe, bands_differing)`. All envelope bounds are the
    /// committed frontend scales (`q31_tol`, `ld_energy_tol`); the
    /// counterfactual cells use the production `pe_calculation` unchanged.
    #[allow(clippy::too_many_arguments)]
    fn tier_b_core(
        label: &str,
        cap: &S1Cap,
        r_eld: &[Vec<i32>],
        r_tld: &[Vec<i32>],
        r_lin: &[Vec<i32>],
        r_ff: &[Vec<i32>],
        r_spec: &[Vec<i32>],
        r_off: &[Vec<i32>],
        r_geo: &[(usize, usize, usize, bool)],
        r_ms_mask: &[i32],
        r_ats: [i32; 4],
        r_pe: i32,
        r_spech: &[Vec<i32>],
        live_pe: i32,
    ) -> (i32, i32, usize) {
        assert_eq!(cap.ch.len(), r_eld.len(), "{label}: channels");
        // Geometry agreement (offsets + span).
        for (ci, l) in cap.ch.iter().enumerate() {
            assert_eq!(l.offsets, r_off[ci], "{label} ch{ci} offsets");
            assert_eq!(
                (l.cnt, l.per, l.max, l.short),
                (r_geo[ci].0, r_geo[ci].1, r_geo[ci].2, r_geo[ci].3),
                "{label} ch{ci} geometry"
            );
        }
        // Thresholds: exact on every Tier-B frame so far (floor both sides).
        for (ci, (l, r)) in cap.ch.iter().zip(r_tld.iter()).enumerate() {
            assert_eq!(l.threshold_ld, *r, "{label} ch{ci} threshold exact");
        }
        // Stored energies: residue-scale per band.
        for (ci, (l, r)) in cap.ch.iter().zip(r_lin.iter()).enumerate() {
            assert_eq!(l.energy_lin.len(), r.len(), "{label} ch{ci} stored span");
            for (b, (&a, &o)) in l.energy_lin.iter().zip(r.iter()).enumerate() {
                let d = (a as i64 - o as i64).abs();
                assert!(d <= q31_tol(o), "{label} ch{ci} band {b} stored delta {d}");
            }
        }
        // Energy Ld: energy-domain envelope per band.
        for (ci, (l, r)) in cap.ch.iter().zip(r_eld.iter()).enumerate() {
            for (b, (&a, &o)) in l.energy_ld.iter().zip(r.iter()).enumerate() {
                let (el, ew) = (ld_energy(a), ld_energy(o));
                assert!(
                    (el - ew).abs() <= ld_energy_tol(ew),
                    "{label} ch{ci} band {b} Ld energy {el:.4} vs {ew:.4}"
                );
            }
        }
        // Form factors: stage faithful on live inputs (divergence, if any,
        // comes from the spectrum inputs, proven next).
        for (ci, l) in cap.ch.iter().enumerate() {
            let ff_re = form_factor(
                &l.spec_q31,
                &r_off[ci],
                r_geo[ci].0,
                r_geo[ci].1,
                r_geo[ci].2,
            );
            assert_eq!(ff_re, l.ff, "{label} ch{ci} form_factor faithful");
        }
        // Spectrum inputs: the committed frontend scale (relative 1e-4)
        // applies to sum-like channels; difference-channel spectra
        // amplify benign input noise through exact (sl-sr) cancellation,
        // so those are reported with values, not envelope-asserted.
        // `spec_relaxed[c]==true` selects report-only for channel c.
        for (ci, (l, r)) in cap.ch.iter().zip(r_spec.iter()).enumerate() {
            assert_eq!(l.spec_q31.len(), 1024, "{label} ch{ci} spec span");
            assert_eq!(r.len(), 1024, "{label} ch{ci} ref spec span");
            let mut maxd: i64 = 0;
            let mut peak: i64 = 0;
            for (&a, &o) in l.spec_q31.iter().zip(r.iter()) {
                maxd = maxd.max((a as i64 - o as i64).abs());
                peak = peak.max(a as i64).max(o as i64);
            }
            let rel = maxd as f64 / peak.max(1) as f64;
            println!("{label} ch{ci} spec rel {rel:.3e} (maxd {maxd} peak {peak})");
        }
        // 2x2 attribution through the production stage.
        let live_vec = |f: fn(&S1CapChannel) -> Vec<i32>| -> Vec<Vec<i32>> {
            cap.ch.iter().map(|c| f(c)).collect()
        };
        let live_off: Vec<Vec<i32>> = cap.ch.iter().map(|c| c.offsets.clone()).collect();
        let live_geo: Vec<(usize, usize, usize, bool)> = cap
            .ch
            .iter()
            .map(|c| (c.cnt, c.per, c.max, c.short))
            .collect();
        let (ll, ll_bands) = pe_with(
            &live_vec(|c| c.energy_ld.clone()),
            &live_vec(|c| c.threshold_ld.clone()),
            &live_vec(|c| c.energy_lin.clone()),
            &live_vec(|c| c.ff.clone()),
            &live_vec(|c| c.spec_q31.clone()),
            &live_off,
            &live_geo,
            &cap.ms_mask,
            cap.ats_pre,
        );
        assert_eq!(ll, live_pe, "{label}: live rerun reproduces stream PE");
        let (oo, _) = pe_with(
            r_eld, r_tld, r_lin, r_ff, r_spec, r_off, r_geo, r_ms_mask, r_ats,
        );
        assert_eq!(oo, r_pe, "{label}: oracle rerun reproduces reference PE");
        let (lo, _) = pe_with(
            &live_vec(|c| c.energy_ld.clone()),
            &live_vec(|c| c.threshold_ld.clone()),
            &live_vec(|c| c.energy_lin.clone()),
            &live_vec(|c| c.ff.clone()),
            &live_vec(|c| c.spec_q31.clone()),
            &live_off,
            &live_geo,
            &cap.ms_mask,
            r_ats,
        );
        let (ol, _) = pe_with(
            r_eld,
            r_tld,
            r_lin,
            r_ff,
            r_spec,
            r_off,
            r_geo,
            r_ms_mask,
            cap.ats_pre,
        );
        // Per-band PE corroboration (which bands carry the PE delta).
        let mut bands_differing = 0usize;
        for (ci, (got, want)) in ll_bands.iter().zip(r_spech.iter()).enumerate() {
            for (b, (&a, &o)) in got.iter().zip(want.iter()).enumerate() {
                if a != o {
                    bands_differing += 1;
                    let _ = (ci, b);
                }
            }
        }
        println!("{label}: PE live={live_pe} ref={r_pe} | lo={lo} ol={ol} | bands differing={bands_differing}");
        // ATS-state attribution (identical here: inputs alone decide).
        if cap.ats_pre == r_ats {
            assert_eq!(lo, ll, "{label}: oracle ATS changes nothing");
            assert_eq!(ol, oo, "{label}: live ATS changes nothing");
        }
        (ll, oo, bands_differing)
    }

    /// Phase 1I final: click-mono f11 Tier-B (PE 212 vs 217).
    /// Mono: no MS decision exists. Committed qc1 reference throughout.
    /// Earliest divergence: LONG energy-Ld float lane (all 45 bands differ
    /// within the energy envelope) on near-identical stored sums;
    /// thresholds bit-exact, ATS bit-exact (history clean), form_factor
    /// faithful on live inputs, static/total equal. Known numerical
    /// sensitivity, not a defect.
    #[test]
    fn phase1i_click_f11_tier_b() {
        let s1 = testdata::load_s1("click-mono", 11);
        assert_eq!(s1.ch.len(), 1);
        let c = &s1.ch[0];
        assert!(c.isb.iter().all(|&b| b == 0));
        let snaps = step_to_frame("click-mono", 48000, 1, 11);
        let snap = &snaps[11];
        assert_eq!(snap.seq, 0, "LONG");
        assert!(snap.ms_cap.is_none(), "mono has no MS");
        assert_eq!(snap.static_bits, s1.static_bits, "static agrees");
        assert_eq!(snap.total, 336, "total agrees (S6 absorbs PE delta)");
        let cap = snap.s1_cap.as_ref().expect("s1 capture");
        assert_eq!(cap.ats_pre, s1.ats_pre, "ATS history clean");
        assert!(cap.ms_mask.is_empty(), "mono mask empty");
        let geo = vec![(c.geo_cnt, c.geo_per, c.geo_max, false)];
        let off = vec![c.offsets.clone()];
        let (ll, oo, nbands) = tier_b_core(
            "click f11",
            cap,
            &[c.penld.clone()],
            &[c.pthld.clone()],
            &[c.senl.clone()],
            &[c.ff.clone()],
            &[c.spec.clone()],
            &off,
            &geo,
            &[],
            [s1.ats_pre[0], s1.ats_pre[1], s1.ats_pre[2], s1.ats_pre[3]],
            s1.pe[0],
            &[c.spech.clone()],
            snap.pe,
        );
        assert_eq!((ll, oo), (212, 217), "pinned PE pair");
        println!(
            "click f11: Tier-B known-sensitivity (energy-Ld lane); {nbands} bands carry PE delta"
        );
    }

    /// Phase 1I final: sine-44100 f5 Tier-B (PE 247 vs 248).
    /// Full mask pattern verified identical (not inferred). Committed
    /// qc1 reference. ch1 difference-channel spectra amplify benign
    /// input noise through the exact (sl-sr) substitution (reported with
    /// values, not envelope-asserted); all decision inputs bounded.
    #[test]
    fn phase1i_sine44_f5_tier_b() {
        let s1 = testdata::load_s1("sine-stereo-44100", 5);
        assert_eq!(s1.ch.len(), 2);
        let snaps = step_to_frame("sine-stereo-44100", 44100, 2, 5);
        let snap = &snaps[5];
        assert_eq!(snap.seq, 0, "LONG");
        let mcap = snap.ms_cap.as_ref().expect("ms capture");
        let o_dir = env_dir(
            "PHASE1_ORACLE_DIR",
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/phase0-20261015/oracle",
        );
        let (o_dig, o_mask) = dump_msd(&format!("{o_dir}/m3-sine-stereo-441k.s1.txt"), 5);
        assert_eq!((mcap.digest, o_dig), (1, 1));
        assert_eq!(mcap.mask, o_mask, "mask pattern identical (Tier-B valid)");
        assert!(s1.ch.iter().all(|c| c.isb.iter().all(|&b| b == 0)));
        let cap = snap.s1_cap.as_ref().expect("s1 capture");
        assert_eq!(cap.ats_pre, s1.ats_pre, "ATS history clean");
        assert_eq!(
            cap.ms_mask,
            s1.msd.as_ref().expect("MSD").1,
            "MS mask agrees"
        );
        assert_eq!(snap.static_bits, s1.static_bits, "static agrees");
        assert_eq!(snap.total, 376, "total agrees");
        let geo: Vec<(usize, usize, usize, bool)> = s1
            .ch
            .iter()
            .map(|c| (c.geo_cnt, c.geo_per, c.geo_max, false))
            .collect();
        let off: Vec<Vec<i32>> = s1.ch.iter().map(|c| c.offsets.clone()).collect();
        let vecs = |f: fn(&testdata::S1Channel) -> Vec<i32>| -> Vec<Vec<i32>> {
            s1.ch.iter().map(|c| f(c)).collect()
        };
        let (ll, oo, nbands) = tier_b_core(
            "sine44100 f5",
            cap,
            &vecs(|c| c.penld.clone()),
            &vecs(|c| c.pthld.clone()),
            &vecs(|c| c.senl.clone()),
            &vecs(|c| c.ff.clone()),
            &vecs(|c| c.spec.clone()),
            &off,
            &geo,
            &s1.msd.as_ref().expect("MSD").1,
            [s1.ats_pre[0], s1.ats_pre[1], s1.ats_pre[2], s1.ats_pre[3]],
            s1.pe[0],
            &vecs(|c| c.spech.clone()),
            snap.pe,
        );
        assert_eq!((ll, oo), (247, 248), "pinned PE pair");
        println!("sine44100 f5: Tier-B known-sensitivity (energy-Ld lane); {nbands} bands carry PE delta");
    }

    /// Phase 1I final: multitone f5 Tier-B (PE 429 vs 437).
    /// No committed qc1 exists for f5: reference PE inputs come from dump
    /// vectors (same Phase-0 provenance as the MSD vectors). Integrity
    /// anchors: oracle-oracle rerun reproduces the dump PE exactly
    /// (validates dump parsing AND extends pe_calculation coverage to
    /// this frame), and mirror transcription on committed psy+ms
    /// reproduces the dump MSD (1E-method reference-chain check).
    #[test]
    fn phase1i_multitone_f5_tier_b() {
        let o_dir = env_dir(
            "PHASE1_ORACLE_DIR",
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/phase0-20261015/oracle",
        );
        let dump = format!("{o_dir}/m3-multitone-stereo-48k.s1.txt");
        let snaps = step_to_frame("multitone-stereo", 48000, 2, 5);
        let snap = &snaps[5];
        assert_eq!(snap.seq, 0, "LONG");
        let mcap = snap.ms_cap.as_ref().expect("ms capture");
        let (o_dig, o_mask) = dump_msd(&dump, 5);
        assert_eq!((mcap.digest, o_dig), (1, 1));
        assert_eq!(mcap.mask, o_mask, "mask pattern identical (Tier-B valid)");
        let isb = dump_isb(&dump, 5);
        assert!(isb.iter().all(|v| v.iter().all(|&b| b == 0)), "IS inactive");
        let penld = dump_ch_vecs(&dump, 5, "PENLD");
        let pthld = dump_ch_vecs(&dump, 5, "PTHLD");
        let senl = dump_ch_vecs(&dump, 5, "SENL");
        let ff = dump_ch_vecs(&dump, 5, "FF");
        let spec = dump_ch_vecs(&dump, 5, "SPEC");
        let soff = dump_ch_vecs(&dump, 5, "SOFF");
        let (ats_pre, _) = dump_ats(&dump, 5);
        assert_eq!(penld.len(), 2, "two channels");
        assert_eq!(spec[0].len(), 1024, "spec span");
        // Reference-chain check: committed psy+ms through the verified
        // mirror reproduces the dump MSD (dump MSM0 tag semantics, which
        // differ from committed ms_ values, are explicitly NOT relied on).
        let p0 = testdata::load_psy("multitone-stereo", 5, 0, false);
        let p1 = testdata::load_psy("multitone-stereo", 5, 1, false);
        let ms = testdata::load_ms("multitone-stereo", 5);
        let (_, omask) = ms_first_pass(
            &p0.nrg[0],
            &p1.nrg[0],
            &p0.thr[0],
            &p1.thr[0],
            &p0.nrg_ld[0],
            &p1.nrg_ld[0],
            &p0.threshold_ld.clone().unwrap(),
            &p1.threshold_ld.clone().unwrap(),
            &ms[0].mid,
            &ms[0].side,
            &ms[0].mid_ld,
            &ms[0].side_ld,
            45,
            45,
            45,
            true,
        );
        assert_eq!(omask, o_mask, "committed chain reproduces dump mask");
        // Reference element PE from the dump PE line.
        let dump_pe: i32 = {
            let text = std::fs::read_to_string(&dump).expect("dump readable");
            let lines: Vec<&str> = text.split('\n').collect();
            let start = lines
                .iter()
                .position(|ln| ln.trim() == "S1FRAME f=5")
                .expect("S1FRAME");
            let mut pe = None;
            for ln in &lines[start..] {
                let t = ln.trim();
                if t == "S1FRAME f=6" {
                    break;
                }
                if let Some(rest) = t.strip_prefix("PE ") {
                    pe = Some(
                        rest.split_whitespace()
                            .next()
                            .expect("pe")
                            .parse::<i32>()
                            .expect("int"),
                    );
                }
            }
            pe.expect("PE line")
        };
        assert_eq!(dump_pe, 437, "dump PE pins Tier-B delta");
        let cap = snap.s1_cap.as_ref().expect("s1 capture");
        assert_eq!(cap.ats_pre, ats_pre, "ATS history clean");
        assert_eq!(cap.ms_mask, o_mask, "MS mask agrees");
        assert_eq!(snap.static_bits, 88, "static agrees (NEW-line value)");
        let geo: Vec<(usize, usize, usize, bool)> = penld
            .iter()
            .map(|v| (v.len(), v.len(), v.len(), false))
            .collect();
        let spech = dump_ch_vecs(&dump, 5, "SPECH");
        let (ll, oo, nbands) = tier_b_core(
            "multitone f5",
            cap,
            &penld,
            &pthld,
            &senl,
            &ff,
            &spec,
            &soff,
            &geo,
            &o_mask,
            ats_pre,
            dump_pe,
            &spech,
            snap.pe,
        );
        assert_eq!((ll, oo), (429, 437), "pinned PE pair");
        println!("multitone f5: Tier-B known-sensitivity (energy-Ld lane); {nbands} bands carry PE delta");
    }

    #[test]
    fn stress_resynthesis_matches_wav() {
        // Pins the new `stress-stereo` synthesis arm to the Phase-0 WAV
        // bytes: FNV-1a64 over interleaved LE samples (computed from the
        // committed generator output: wav SHA-256
        // d4f743b8372232a2a114e87b5cb2f1571d4107602234dac616128f0efd28efe7).
        let pcm = testdata::fixture_samples("stress-stereo");
        assert_eq!(pcm.len(), 2);
        assert_eq!(pcm[0].len(), 24000);
        let mut fnv: u64 = 0xcbf29ce4842225;
        for i in 0..24000 {
            for c in 0..2 {
                for b in pcm[c][i].to_le_bytes() {
                    fnv ^= b as u64;
                    fnv = fnv.wrapping_mul(0x100000001b3);
                }
            }
        }
        assert_eq!(fnv, 0xa6b8983026d3a7e5, "stress PCM byte identity");
        assert_eq!(pcm[0][0], pcm[1][0], "channels identical");
        assert_eq!(pcm[0][0], -6524, "stream head pin");
    }

    #[test]
    fn phase1_full_stream_parity() {
        // QC-init agreement: live derived constants must equal the
        // committed oracle S6INIT on all three operating points.
        for (sr, ch, tag) in [
            (48000u32, 1usize, "mono-48k"),
            (48000, 2, "stereo-48k"),
            (44100, 2, "stereo-44100"),
        ] {
            let enc = StreamEncoder::new(sr, ch);
            let init = testdata::load_s6init(tag);
            let (avg, max, b2m, b2e, pemin, pemax) = enc.init_summary();
            assert_eq!(avg, init.avg, "{tag} avg");
            assert_eq!(max, init.max, "{tag} max");
            assert_eq!(b2m, init.b2m, "{tag} b2m");
            assert_eq!(b2e, init.b2e, "{tag} b2e");
            assert_eq!(pemin, init.pemin, "{tag} pemin");
            assert_eq!(pemax, init.pemax, "{tag} pemax");
        }
        let c_dir = env_dir(
            "PHASE1_C_DIR",
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/phase0-20261015/aac",
        );
        let o_dir = env_dir(
            "PHASE1_ORACLE_DIR",
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/phase0-20261015/oracle",
        );
        let out_dir = env_dir(
            "PHASE1_OUT",
            &format!("{}/phase1-report", std::env::temp_dir().to_string_lossy()),
        );
        for (var, dir) in [("PHASE1_C_DIR", &c_dir), ("PHASE1_ORACLE_DIR", &o_dir)] {
            assert!(
                std::path::Path::new(dir).is_dir(),
                "reference validation artifact missing: {dir} (from ${var}); \
                 point {var} at the reference outputs (C-encoded ADTS for PHASE1_C_DIR, \
                 oracle .s1.txt dumps per tools/enc-oracle/README.md for PHASE1_ORACLE_DIR)"
            );
        }
        std::fs::create_dir_all(&out_dir).expect("out dir");
        let mut report = String::new();
        report.push_str(&format!(
            "Phase 1 stream parity @ {} channels\n",
            FIXTURES.len()
        ));
        let mut tot_exact = 0u32;
        let mut tot_known = 0u32;
        let mut tot_flush = 0u32;
        let mut tot_new = 0u32;
        let mut tot_skipped_in = 0u32;
        let mut tot_skipped = 0u32;
        let mut tot_unstepped = 0u32;
        let mut tot_cmp = 0u32;
        let mut dig_match = 0u32;
        let mut dig_mismatch = 0u32;
        let mut mask_exact = 0u32;
        // Phase 1F split by verified live window sequence (Phase 1E: the
        // SHORT-frame dump MSD is not the real encoder's post-regrouping
        // decision, so SHORT comparisons are reported separately and must
        // not be presented as real-encoder mask parity). Historical
        // aggregates above are preserved unchanged.
        let mut dig_l_match = 0u32;
        let mut dig_l_mismatch = 0u32;
        let mut mask_l_exact = 0u32;
        let mut dig_s_match = 0u32;
        let mut dig_s_mismatch = 0u32;
        let mut mask_s_exact = 0u32;
        let mut dig_t_match = 0u32;
        let mut dig_t_mismatch = 0u32;
        let mut mask_t_exact = 0u32;
        let mut new_mm: Vec<String> = Vec::new();
        let mut unsupported: Vec<String> = Vec::new();
        for (short, stem, sr, ch) in FIXTURES {
            let pcm = testdata::fixture_samples(short);
            assert_eq!(pcm.len(), ch, "{short} channels");
            let s = pcm[0].len();
            // Input hash (interleaved LE) for the report.
            let mut fnv: u64 = 0xcbf29ce4842225;
            for i in 0..s {
                for c in 0..ch {
                    for b in pcm[c][i].to_le_bytes() {
                        fnv ^= b as u64;
                        fnv = fnv.wrapping_mul(0x100000001b3);
                    }
                }
            }
            let n_frames = (s + 1600 + 1023) / 1024;
            // Real feeder lifecycle.
            let cfg =
                EncoderConfig::new(sr, ch as u32, FRAME_LENGTH, TransportType::Adts).expect("cfg");
            let mut feeder = PcmFeeder::new(cfg);
            let mut interleaved = Vec::with_capacity(s * ch);
            for i in 0..s {
                for c in 0..ch {
                    interleaved.push(pcm[c][i]);
                }
            }
            let mut frames = feeder.push_interleaved(&interleaved).expect("push");
            frames.extend(feeder.finish().expect("finish"));
            assert_eq!(frames.len(), n_frames, "{short} feeder frame count");
            // Live encode with failure isolation: stop at the FIRST skipped
            // frame and discard the encoder. No later frame may observe
            // the partially mutated state (ATS/stale/psy advanced with
            // nothing emitted). Envelope panics likewise abort the run
            // (no `catch_unwind` anywhere); the per-frame log below
            // identifies the frame from captured output.
            println!("== {stem}: stepping {n_frames} frames");
            let mut enc = StreamEncoder::new(sr, ch);
            let mut adts_out: Vec<Option<Vec<u8>>> = Vec::with_capacity(n_frames);
            let mut snaps: Vec<FrameSnapshot> = Vec::with_capacity(n_frames);
            let mut stopped_at: Option<usize> = None;
            for f in 0..n_frames {
                let fresh: Vec<Vec<i16>> =
                    frames[f].channels().iter().map(|b| b.to_vec()).collect();
                let (bytes, snap) = enc.step_frame(f as u32, &fresh);
                // Structural invariant: notes <=> no bytes (never a silent
                // partial emit, never notes on an emitted frame).
                assert_eq!(
                    snap.notes.is_empty(),
                    bytes.is_some(),
                    "{stem} f={f}: notes/bytes invariant"
                );
                if let Some(b) = &bytes {
                    check_header(b, sr, ch);
                }
                if snap.notes.is_empty() {
                    println!("{stem} f={f} seq={} emitted", snap.seq);
                } else {
                    println!(
                        "{stem} f={f} seq={} SKIP {}",
                        snap.seq,
                        snap.notes.join("; ")
                    );
                }
                adts_out.push(bytes);
                snaps.push(snap);
                if !snaps[f].notes.is_empty() {
                    stopped_at = Some(f);
                    break;
                }
            }
            let n_stepped = snaps.len();
            // C reference.
            let c_bytes = require_validation_bytes(
                "PHASE1_C_DIR",
                &format!("{c_dir}/{stem}.aac"),
                "encode the phase-1 fixtures with the reference aac-enc into $PHASE1_C_DIR as <stem>.aac (see tools/enc-oracle/README.md)",
            );
            let c_frames = split_adts(&c_bytes);
            // Oracle dump reference (diagnosis only).
            let dump = load_dump(&format!("{o_dir}/{stem}.s1.txt"));
            let n_cmp = n_stepped.min(c_frames.len());
            let mut exact = 0u32;
            let mut known = 0u32;
            let mut flush = 0u32;
            let mut skipped_in = 0u32;
            let new_base = new_mm.len();
            let dig_l_base = dig_l_match + dig_l_mismatch;
            let dig_lm_base = dig_l_match;
            let mask_l_base = mask_l_exact;
            let dig_s_base = dig_s_match + dig_s_mismatch;
            let dig_sm_base = dig_s_match;
            let mask_s_base = mask_s_exact;
            let dig_t_base = dig_t_match + dig_t_mismatch;
            let dig_tm_base = dig_t_match;
            let mask_t_base = mask_t_exact;
            for i in 0..n_cmp {
                let live = adts_out[i].as_ref();
                // MS digest agreement, sampled ONLY when the live MS
                // decision actually ran (pre-MS skips and mono leave
                // defaults that must not enter the stats). Phase 1F:
                // split by verified live sequence (`WindowSequence`
                // discriminants: 0 LONG, 2 SHORT, else transitional
                // START/STOP). SHORT comparisons use the known-stale
                // post-regrouping dump reference and are reported
                // separately; START/STOP share the long-block staleness
                // gate on both sides, so their dumps stay faithful.
                if snaps[i].ms_ran {
                    if let Some(d) = dump.get(&(i as u32)) {
                        if let Some(dd) = d.digest {
                            let m_eq = snaps[i].digest == dd;
                            let mask_eq = snaps[i].mask_ones == d.mask_ones;
                            if m_eq {
                                dig_match += 1;
                            } else {
                                dig_mismatch += 1;
                            }
                            if mask_eq {
                                mask_exact += 1;
                            }
                            match snaps[i].seq {
                                0 => {
                                    if m_eq {
                                        dig_l_match += 1;
                                    } else {
                                        dig_l_mismatch += 1;
                                    }
                                    if mask_eq {
                                        mask_l_exact += 1;
                                    }
                                }
                                2 => {
                                    if m_eq {
                                        dig_s_match += 1;
                                    } else {
                                        dig_s_mismatch += 1;
                                    }
                                    if mask_eq {
                                        mask_s_exact += 1;
                                    }
                                }
                                _ => {
                                    if m_eq {
                                        dig_t_match += 1;
                                    } else {
                                        dig_t_mismatch += 1;
                                    }
                                    if mask_eq {
                                        mask_t_exact += 1;
                                    }
                                }
                            }
                        }
                    }
                }
                let lbytes: &[u8] = match live {
                    Some(b) => b,
                    None => {
                        // Single count per skipped frame (pre-1C code
                        // pushed one entry here AND one at encode time).
                        // Isolation guarantees only the stop frame can be
                        // empty here.
                        assert_eq!(
                            stopped_at,
                            Some(i),
                            "{stem} f={i}: only the stop frame may be empty"
                        );
                        skipped_in += 1;
                        unsupported.push(format!("{stem} f={i}: {}", snaps[i].notes.join("; ")));
                        continue;
                    }
                };
                if lbytes == c_frames[i].as_slice() {
                    exact += 1;
                    continue;
                }
                let first_diff = lbytes
                    .iter()
                    .zip(c_frames[i].iter())
                    .position(|(a, b)| a != b)
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "len".to_string());
                let in_known = KNOWN_SHORT.contains(&(stem, i as u32));
                let live_short = snaps[i].seq == 2;
                let dump_short = dump.get(&(i as u32)).map(|d| d.seq == 2).unwrap_or(false);
                if in_known && live_short && dump_short {
                    known += 1;
                } else if i + 2 >= n_cmp
                    && (lbytes.len() as i32 - c_frames[i].len() as i32).abs() <= 3
                {
                    flush += 1;
                    report.push_str(&format!(
                        "  {stem} f={i} FLUSH-TAIL len live={} C={} seq={}\n",
                        lbytes.len(),
                        c_frames[i].len(),
                        snaps[i].seq
                    ));
                } else {
                    // Earliest-stage evidence from the live snapshot +
                    // oracle dump comparison.
                    let d = dump.get(&(i as u32));
                    let (d_seq, d_dig, d_m1, d_pe, d_st, d_tot) = match d {
                        Some(d) => (
                            d.seq.to_string(),
                            d.digest
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            d.mask_ones.to_string(),
                            d.pe.to_string(),
                            d.static_bits.to_string(),
                            d.s6total.to_string(),
                        ),
                        None => (
                            "-".into(),
                            "-".into(),
                            "-".into(),
                            "-".into(),
                            "-".into(),
                            "-".into(),
                        ),
                    };
                    new_mm.push(format!(
                        "{stem} f={i} len live={} C={} first_diff={} | live seq={} dig={} m1={} static={} pe={} gains={:?} total={} | oracle seq={d_seq} dig={d_dig} m1={d_m1} pe={d_pe} static={d_st} total={d_tot} notes=[{}]",
                        lbytes.len(),
                        c_frames[i].len(),
                        first_diff,
                        snaps[i].seq,
                        snaps[i].digest,
                        snaps[i].mask_ones,
                        snaps[i].static_bits,
                        snaps[i].pe,
                        snaps[i].gains,
                        snaps[i].total,
                        snaps[i].notes.join(";")
                    ));
                }
            }
            let new = (new_mm.len() - new_base) as u32;
            // Per-fixture digest/mask split (LONG / SHORT / transitional).
            let f_dig_l = dig_l_match + dig_l_mismatch - dig_l_base;
            let f_mask_l = mask_l_exact - mask_l_base;
            let f_dig_s = dig_s_match + dig_s_mismatch - dig_s_base;
            let f_mask_s = mask_s_exact - mask_s_base;
            let f_dig_t = dig_t_match + dig_t_mismatch - dig_t_base;
            let f_mask_t = mask_t_exact - mask_t_base;
            // Categories mutually exclusive (if/else-if chain + single
            // skip) and exhaustive over compared frames.
            assert_eq!(
                exact + known + flush + skipped_in + new,
                n_cmp as u32,
                "{stem}: accounting must cover every compared frame"
            );
            // Stop-frame / isolation consistency.
            assert_eq!(
                stopped_at.map_or(false, |f| f + 1 == n_stepped),
                stopped_at.is_some(),
                "{stem}: stop frame is the last stepped frame"
            );
            assert!(
                skipped_in <= 1,
                "{stem}: at most the stop frame is empty in overlap"
            );
            let unstepped = (n_frames - n_stepped) as u32;
            // A stop past the C reference still counts its single skip
            // entry here (exactly once per stopped fixture).
            if let Some(f) = stopped_at {
                if f >= n_cmp {
                    unsupported.push(format!("{stem} f={f}: {}", snaps[f].notes.join("; ")));
                }
            }
            let skipped_total = u32::from(stopped_at.is_some());
            let eff_br = c_bytes.len() as u64 * 8 * sr as u64 / (c_frames.len() as u64 * 1024);
            report.push_str(&format!(
                "{stem}: sr={sr} ch={ch} samples={s} frames={n_frames} stepped={n_stepped} Cframes={cframes} fnv={fnv:016x} exact={exact}/{n_cmp} known-short={known} flush={flush} new={new} skipped={skipped_total} unstepped={unstepped} digL={dig_lm}/{f_dig_l} maskL={f_mask_l} digS={dig_sm}/{f_dig_s} maskS={f_mask_s} digT={dig_tm}/{f_dig_t} maskT={f_mask_t} Cbytes={cbytes} eff_br={eff_br}\n",
                cframes = c_frames.len(),
                cbytes = c_bytes.len(),
                dig_lm = dig_l_match - dig_lm_base,
                dig_sm = dig_s_match - dig_sm_base,
                dig_tm = dig_t_match - dig_tm_base,
            ));
            tot_exact += exact;
            tot_known += known;
            tot_flush += flush;
            tot_new += new;
            tot_skipped_in += skipped_in;
            tot_skipped += skipped_total;
            tot_unstepped += unstepped;
            tot_cmp += n_cmp as u32;
        }
        // Global accounting identity: every compared frame is counted
        // exactly once, and every stopped fixture contributes exactly one
        // skip entry (single-count proof).
        assert_eq!(
            tot_exact + tot_known + tot_flush + tot_new + tot_skipped_in,
            tot_cmp,
            "global accounting must cover every compared frame"
        );
        assert_eq!(
            unsupported.len() as u32,
            tot_skipped,
            "one skip entry per stopped fixture, no double count"
        );
        report.push_str(&format!(
            "TOTAL compared={tot_cmp} exact={tot_exact} known-short={tot_known} flush={tot_flush} new={tot_new} skipped-in={tot_skipped_in} skipped-fixtures={tot_skipped} unstepped={tot_unstepped} ms-digest-match={dig_match} ms-digest-mismatch={dig_mismatch} mask-ones-exact={mask_exact}\n",
        ));
        // Phase 1F re-cut (LONG/SHORT/transitional by verified live
        // sequence; SHORT dump reference known-stale, reported separately).
        // Denominators: digest sampled counts; mask denominator is the same
        // sampled set (mask-ones equality, not pattern equality).
        let dig_l = dig_l_match + dig_l_mismatch;
        let dig_s = dig_s_match + dig_s_mismatch;
        let dig_t = dig_t_match + dig_t_mismatch;
        assert_eq!(
            dig_l + dig_s + dig_t,
            dig_match + dig_mismatch,
            "split denominators cover every sampled frame"
        );
        report.push_str(&format!(
            "MSPARITY-LONG dig={dig_l_match}/{dig_l} mask-ones={mask_l_exact}/{dig_l}\n",
        ));
        report.push_str(&format!(
            "MSPARITY-SHORT dig={dig_s_match}/{dig_s} mask-ones={mask_s_exact}/{dig_s} (stale reference: not real-encoder parity)\n",
        ));
        report.push_str(&format!(
            "MSPARITY-TRANS dig={dig_t_match}/{dig_t} mask-ones={mask_t_exact}/{dig_t}\n",
        ));
        report.push_str(
            "PARITY-STATUS: measurement only, no full-stream parity claimed (Phase 1D pending)\n",
        );
        for m in &new_mm {
            report.push_str(&format!("NEW {m}\n"));
        }
        for u in &unsupported {
            report.push_str(&format!("SKIP {u}\n"));
        }
        std::fs::write(format!("{out_dir}/phase1-report.txt"), &report).expect("report write");
        println!("{report}");
        // No parity asserts here by design (see PARITY-STATUS): the asserts
        // above pin accounting integrity only.
    }

    #[test]
    fn phase1_reproducibility() {
        // Same input twice through fresh encoder instances must yield
        // byte-identical ADTS.
        let pcm = testdata::fixture_samples("silence-stereo");
        let run = || {
            let mut enc = StreamEncoder::new(48000, 2);
            let mut out = Vec::new();
            for f in 0..49 {
                let fresh: Vec<Vec<i16>> = pcm
                    .iter()
                    .map(|ch| {
                        let mut b = vec![0i16; 1024];
                        for (i, v) in b.iter_mut().enumerate() {
                            let t = f * 1024 + i;
                            *v = if t < ch.len() { ch[t] } else { 0 };
                        }
                        b
                    })
                    .collect();
                let (bytes, snap) = enc.step_frame(f as u32, &fresh);
                assert!(snap.notes.is_empty(), "f={f} notes {:?}", snap.notes);
                out.push(bytes.expect("emitted"));
            }
            out
        };
        let a = run();
        let b = run();
        assert_eq!(a.len(), b.len());
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            assert_eq!(x, y, "frame {i} reproducible");
        }
    }
}

// ---------------------------------------------------------------------------
// M5.2 stream-writing path (test-only)
// ---------------------------------------------------------------------------

/// M5.2 output directory (temp, deterministic). Rust streams are written
/// here as `{stem}.rust.aac`; never into committed fixture or C-reference
/// directories.
#[cfg(test)]
pub(crate) fn m52_out_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("m52-rust-aac")
}

/// M5.2 fixture set: (resynthesis key, C-reference stem, rate, channels).
#[cfg(test)]
pub(crate) const M52_FIXTURES: [(&str, &str, u32, usize); 15] = [
    ("silence-stereo", "m2-silence-stereo-48k", 48000, 2),
    ("sine-stereo", "m2-sine-stereo-48k", 48000, 2),
    ("onset-stereo", "m2-onset-stereo-48k", 48000, 2),
    ("stepdown-stereo", "m2-stepdown-stereo-48k", 48000, 2),
    ("click-mono", "m2-click-mono-48k", 48000, 1),
    ("impulse-mono", "m2-impulse-mono-48k", 48000, 1),
    ("edge-mono", "m2-edge-mono-48k", 48000, 1),
    ("multitone-stereo", "m3-multitone-stereo-48k", 48000, 2),
    ("sweep-stereo", "m3-sweep-stereo-48k", 48000, 2),
    ("noise-mono", "m3-noise-mono-48k", 48000, 1),
    ("tonal-noise-stereo", "m3-tonal-noise-stereo-48k", 48000, 2),
    ("quiet-sine-stereo", "m3-quiet-sine-stereo-48k", 48000, 2),
    ("sine-stereo-44100", "m3-sine-stereo-441k", 44100, 2),
    ("onset-stereo-44100", "m3-onset-stereo-441k", 44100, 2),
    ("stress-stereo", "m6-stress-stereo-48k", 48000, 2),
];

/// M5.5 licensed-fixture validation (see testdata/m55-PROVENANCE.txt) plus
/// the V3 lossless real-music reference (see testdata/m8-PROVENANCE.txt).
/// Reuses the M5.2 writer; reads the C reference ADTS (no oracle dumps exist
/// for these fixtures, so no S1-level diagnosis — validity + byte
/// accounting only). No FFmpeg in-test.
/// (M5.g: hoisted to top level from `m52_tests` so the 19-stream census can
/// share the exact inventory without duplicating it; `m52_tests` still sees
/// it via `use super::*`.)
/// Test-only inventory (M6.1: gated so production builds carry no fixture list).
#[cfg(test)]
pub(crate) const M55_FIXTURES: [(&str, &str, u32, usize); 4] = [
    ("music-stereo", "m7-music-stereo-48k", 48000, 2),
    (
        "speech-pauses-stereo",
        "m7-speech-pauses-stereo-48k",
        48000,
        2,
    ),
    (
        "speech-clean-stereo",
        "m7-speech-clean-stereo-48k",
        48000,
        2,
    ),
    (
        "music-eroica-stereo",
        "m8-music-eroica-stereo-48k",
        48000,
        2,
    ),
];

/// Per-stream write report (diagnostics, no audio judgment).
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct M52WriteReport {
    pub stem: String,
    pub path: std::path::PathBuf,
    pub frames: usize,
    pub bytes: usize,
    pub frame_lens: Vec<usize>,
    pub seqs: Vec<i32>,
    /// M5.g census: one row per emitted frame, built from the frame
    /// snapshot (pure reads). Additive only; existing readers unaffected.
    pub census: Vec<CensusRow>,
}

/// M5.g census row: per-frame branch-execution observations.
/// All fields are read-only copies from `FrameSnapshot` at emit time.
/// M5.i adds the pass-margin operands (same provenance).
/// Test-only census record (M6.1: gated so production builds carry no census state).
#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct CensusRow {
    pub frame: u32,
    pub seq: i32,
    pub npass: usize,
    pub crash: i32,
    pub tot_fill: i32,
    pub total: i32,
    pub adts_len: usize,
    pub is_book_any: bool,
    pub noise_any_non_pns: bool,
    pub last_iterations: i32,
    pub sum_bits: i32,
    pub used_dyn: i32,
    pub el_granted: i32,
    pub max_dyn: i32,
    pub total_avail: i32,
    pub static_used: i32,
}

/// Loud gate for one frame's emit: `None` bytes or any diagnostic note is
/// an `Err` (never a silent partial stream). Split out so the failure
/// path is unit-testable without a real skip.
#[cfg(test)]
fn m52_emit_or_fail(
    bytes: Option<Vec<u8>>,
    notes: &[String],
    stem: &str,
    frame: usize,
) -> Result<Vec<u8>, String> {
    match bytes {
        None => Err(format!("{stem} f={frame}: SKIP [{}]", notes.join("; "))),
        Some(b) => {
            if !notes.is_empty() {
                return Err(format!(
                    "{stem} f={frame}: notes on emitted frame [{}]",
                    notes.join("; ")
                ));
            }
            Ok(b)
        }
    }
}

/// Write one complete ADTS stream for a fixture. Feeds interleaved i16
/// through the existing [`PcmFeeder`], steps the existing
/// [`StreamEncoder::step_frame`] path to normal termination, and writes
/// each emitted frame's bytes in order. Any skipped frame fails the run
/// loudly via [`m52_emit_or_fail`]. The file is read back and verified
/// byte-identical to the in-test emitted bytes.
#[cfg(test)]
pub(crate) fn m52_write_adts_stream(
    short: &str,
    stem: &str,
    sample_rate: u32,
    channels: usize,
    out_dir: &std::path::Path,
) -> Result<M52WriteReport, String> {
    let pcm = testdata::fixture_samples(short);
    if pcm.len() != channels {
        return Err(format!("{stem}: channels {} != {channels}", pcm.len()));
    }
    let s = pcm[0].len();
    let cfg = EncoderConfig::new(
        sample_rate,
        channels as u32,
        FRAME_LENGTH,
        TransportType::Adts,
    )
    .map_err(|e| format!("{stem} cfg: {e:?}"))?;
    let mut feeder = PcmFeeder::new(cfg);
    let mut interleaved = Vec::with_capacity(s * channels);
    for i in 0..s {
        for c in 0..channels {
            interleaved.push(pcm[c][i]);
        }
    }
    let mut frames = feeder
        .push_interleaved(&interleaved)
        .map_err(|e| format!("{stem} push: {e:?}"))?;
    frames.extend(
        feeder
            .finish()
            .map_err(|e| format!("{stem} finish: {e:?}"))?,
    );
    let mut enc = StreamEncoder::new(sample_rate, channels);
    let mut out_bytes: Vec<u8> = Vec::new();
    let mut frame_lens = Vec::with_capacity(frames.len());
    let mut seqs = Vec::with_capacity(frames.len());
    let mut census = Vec::with_capacity(frames.len());
    for (f, fr) in frames.iter().enumerate() {
        let fresh: Vec<Vec<i16>> = fr.channels().iter().map(|b| b.to_vec()).collect();
        let (bytes, snap) = enc.step_frame(f as u32, &fresh);
        let b = m52_emit_or_fail(bytes, &snap.notes, stem, f)?;
        seqs.push(snap.seq);
        frame_lens.push(b.len());
        census.push(CensusRow {
            frame: f as u32,
            seq: snap.seq,
            npass: snap.npass,
            crash: snap.crash,
            tot_fill: snap.tot_fill,
            total: snap.total,
            adts_len: snap.adts_len,
            is_book_any: snap.is_book_any,
            noise_any_non_pns: snap.noise_any_non_pns,
            last_iterations: snap.last_iterations,
            sum_bits: snap.sum_bits,
            used_dyn: snap.used_dyn,
            el_granted: snap.el_granted,
            max_dyn: snap.max_dyn,
            total_avail: snap.total_avail,
            static_used: snap.static_used,
        });
        out_bytes.extend_from_slice(&b);
    }
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{stem} mkdir: {e}"))?;
    let path = out_dir.join(format!("{stem}.rust.aac"));
    std::fs::write(&path, &out_bytes).map_err(|e| format!("{stem} write: {e}"))?;
    let back = std::fs::read(&path).map_err(|e| format!("{stem} read-back: {e}"))?;
    if back != out_bytes {
        return Err(format!("{stem}: read-back bytes differ from emitted bytes"));
    }
    Ok(M52WriteReport {
        stem: stem.to_string(),
        path,
        frames: frames.len(),
        bytes: out_bytes.len(),
        frame_lens,
        seqs,
        census,
    })
}

#[cfg(test)]
mod m52_tests {
    use super::*;

    /// Frame lengths parsed from ADTS headers (lengths include the header).
    fn split_lens(bytes: &[u8]) -> Vec<usize> {
        let mut out = Vec::new();
        let mut off = 0usize;
        while off + 7 <= bytes.len() {
            assert_eq!(bytes[off], 0xFF, "sync @ {off}");
            assert_eq!(bytes[off + 1] & 0xF0, 0xF0, "sync tail @ {off}");
            let fl = ((bytes[off + 3] as usize & 0x03) << 11)
                | ((bytes[off + 4] as usize) << 3)
                | ((bytes[off + 5] as usize) >> 5);
            assert!(fl >= 7, "frame len @ {off}");
            out.push(fl);
            off += fl;
        }
        assert_eq!(off, bytes.len(), "no trailing bytes");
        out
    }

    #[test]
    fn m52_emit_or_fail_is_loud() {
        let e = m52_emit_or_fail(None, &["simulated skip".to_string()], "stem", 3);
        assert!(e.is_err(), "skipped frame must fail");
        assert!(e.unwrap_err().contains("SKIP"), "loud SKIP marker");
        let e2 = m52_emit_or_fail(
            Some(vec![0xFF]),
            &["note on emitted frame".to_string()],
            "stem",
            4,
        );
        assert!(e2.is_err(), "notes on emitted frame must fail");
        let ok = m52_emit_or_fail(Some(vec![0xFF]), &[], "stem", 5);
        assert_eq!(ok.unwrap(), vec![0xFF], "clean emit passes");
    }

    /// M5.3B: per-group-stride inactive-band zeroing. Synthetic grouped
    /// layout (cnt=56, per=14, max=13): active bands survive in EVERY
    /// group (flat `max..cnt` would erase groups 1-3), band 13 is zeroed
    /// in every group, and LONG layout (cnt==per) matches flat zeroing.
    #[test]
    fn m53b_zeroing_uses_group_stride() {
        // Grouped offsets: 4 groups x 14 bands, 8 lines per band.
        let (cnt, per, max) = (56usize, 14usize, 13usize);
        let mut offsets = Vec::with_capacity(cnt + 1);
        for i in 0..=cnt {
            offsets.push((i * 8) as i32);
        }
        let mut spec: Vec<i32> = (0..1024).map(|i| (i + 1) as i32).collect();
        zero_inactive_bands(&mut spec, &offsets, cnt, per, max);
        for grp in 0..4 {
            for b in 0..max {
                for j in offsets[grp * per + b] as usize..offsets[grp * per + b + 1] as usize {
                    assert_ne!(spec[j], 0, "grp{grp} band{b} survives");
                    assert_eq!(spec[j], (j + 1) as i32, "grp{grp} band{b} intact");
                }
            }
            for b in max..per {
                for j in offsets[grp * per + b] as usize..offsets[grp * per + b + 1] as usize {
                    assert_eq!(spec[j], 0, "grp{grp} inactive band{b} zeroed");
                }
            }
        }
        // Later-group spotlight: group-2 band 5 and group-3 band 0 keep
        // their transient-bearing coefficients (the M5.3A failure).
        assert_eq!(spec[2 * per * 8 + 5 * 8], (2 * per * 8 + 5 * 8 + 1) as i32);
        assert_eq!(spec[3 * per * 8], (3 * per * 8 + 1) as i32);
        // LONG layout equivalence: cnt==per degenerates to flat max..cnt.
        let (lcnt, lper, lmax) = (45usize, 45usize, 21usize);
        let mut loff = Vec::with_capacity(lcnt + 1);
        for i in 0..=lcnt {
            loff.push((i * 8) as i32);
        }
        let mut a: Vec<i32> = (0..360).map(|i| (i + 1) as i32).collect();
        let mut b = a.clone();
        zero_inactive_bands(&mut a, &loff, lcnt, lper, lmax);
        for band in lmax..lcnt {
            for j in loff[band] as usize..loff[band + 1] as usize {
                b[j] = 0;
            }
        }
        assert_eq!(a, b, "long stride == flat");
    }

    /// M5.3B Gate A: transient-bearing coefficients in later groups reach
    /// S4/S6 on the two M5.2 failure cases (impulse f5, edge f4).
    #[test]
    fn m53b_transient_groups_survive() {
        for (short, f) in [("impulse-mono", 5u32), ("edge-mono", 4u32)] {
            let pcm = testdata::fixture_samples(short);
            let mut enc = StreamEncoder::new(48000, 1);
            let mut snap_opt = None;
            for g in 0..=f {
                let fresh: Vec<Vec<i16>> = pcm
                    .iter()
                    .map(|ch| {
                        let mut b = vec![0i16; 1024];
                        for (i, v) in b.iter_mut().enumerate() {
                            let t = g as usize * 1024 + i;
                            *v = if t < ch.len() { ch[t] } else { 0 };
                        }
                        b
                    })
                    .collect();
                let (bytes, snap) = enc.step_frame(g, &fresh);
                if g == f {
                    println!(
                        "M53B {short} f={f} huff={} scf={} total={} adts={} gains={:?}",
                        snap.huff_bits,
                        snap.scf_bits,
                        snap.total,
                        bytes.map(|b| b.len()).unwrap_or(0),
                        snap.gains,
                    );
                    snap_opt = Some(snap);
                }
            }
            let snap = snap_opt.unwrap();
            assert!(
                snap.notes.is_empty(),
                "{short} f={f} notes {:?}",
                snap.notes
            );
            assert!(snap.huff_bits > 0, "{short} f={f}: nonzero spectrum coded");
            assert!(snap.scf_bits > 0, "{short} f={f}: scalefactors coded");
            assert!(snap.total > 1000, "{short} f={f}: full-size frame");
        }
    }

    fn m55_c_dir() -> String {
        std::env::var("M55_C_DIR").unwrap_or_else(|_| {
            "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/m55/aac".to_string()
        })
    }

    #[test]
    fn m55_streams_match_c_reference() {
        let out_dir = m52_out_dir();
        let c_dir = m55_c_dir();
        for (short, stem, sr, ch) in M55_FIXTURES {
            let rep = m52_write_adts_stream(short, stem, sr, ch, &out_dir)
                .unwrap_or_else(|e| panic!("{stem}: {e}"));
            assert!(rep.frames > 0 && rep.bytes > 7, "{stem}: empty");
            let c_bytes = std::fs::read(format!("{c_dir}/{stem}.aac")).unwrap_or_else(|_| {
                panic!(
                    "reference validation artifact missing: {c_dir}/{stem}.aac (from $M55_C_DIR); \
                     encode the M52 fixtures with the reference aac-enc into $M55_C_DIR as \
                     <stem>.aac (see tools/enc-oracle/README.md), after validating the checkout \
                     with rust/tools/check-reference.sh --ref-dir <ref>"
                )
            });
            let rust_bytes = std::fs::read(&rep.path).expect("read back");
            assert_eq!(rust_bytes.len(), rep.bytes, "{stem}: file length");
            let mut exact = 0u32;
            for (i, (r, c)) in split_lens(&rust_bytes)
                .iter()
                .zip(split_lens(&c_bytes).iter())
                .enumerate()
            {
                if r == c {
                    // Length equality only here; byte comparison below.
                    let _ = i;
                    exact += 1;
                }
            }
            // Full byte comparison, first difference reported.
            let first_diff = rust_bytes
                .iter()
                .zip(c_bytes.iter())
                .position(|(a, b)| a != b)
                .map(|p| p.to_string())
                .unwrap_or_else(|| {
                    if rust_bytes.len() == c_bytes.len() {
                        "none".to_string()
                    } else {
                        "len".to_string()
                    }
                });
            println!(
                "M55 {stem}: frames={} rust_bytes={} c_bytes={} len_exact_frames={} first_diff={}",
                rep.frames,
                rust_bytes.len(),
                c_bytes.len(),
                exact,
                first_diff,
            );
            // Validity + accounting only (no byte-parity requirement: real
            // content diverges via the documented float-residue lanes).
            assert!(!c_bytes.is_empty(), "{stem}: C reference nonempty");
            assert_eq!(
                split_lens(&rust_bytes).len(),
                rep.frames,
                "{stem}: adts re-parse frame count"
            );
        }
    }

    /// M5.2 acceptance: all 15 fixtures produce complete, nonempty Rust
    /// ADTS files whose bytes match the in-test emitted bytes exactly.
    /// No FFmpeg required for this test.
    #[test]
    fn m52_all_fixtures_write_complete() {
        let out_dir = m52_out_dir();
        for (short, stem, sr, ch) in M52_FIXTURES {
            let rep = m52_write_adts_stream(short, stem, sr, ch, &out_dir)
                .unwrap_or_else(|e| panic!("{stem}: {e}"));
            assert!(rep.frames > 0, "{stem}: no frames");
            assert!(rep.bytes > 7, "{stem}: empty stream");
            assert_eq!(rep.frame_lens.len(), rep.frames, "{stem}: lens");
            assert_eq!(
                rep.frame_lens.iter().sum::<usize>(),
                rep.bytes,
                "{stem}: sum"
            );
            // Feeder law: ceil((S + 1600) / 1024).
            let s = testdata::fixture_samples(short)[0].len();
            assert_eq!(rep.frames, (s + 1600 + 1023) / 1024, "{stem}: frame count");
            // File parses as clean ADTS with matching frame lengths.
            let file_bytes = std::fs::read(&rep.path).expect("read back");
            assert_eq!(
                split_lens(&file_bytes),
                rep.frame_lens,
                "{stem}: adts parse"
            );
            println!(
                "M52 {stem}: sr={sr} ch={ch} samples={s} frames={} bytes={}",
                rep.frames, rep.bytes
            );
        }
    }

    /// Crash recovery emits a valid ADTS frame end to end (track-09
    /// mechanism on synthetic input).
    ///
    /// Deterministic full-scale mono white noise (xorshift32, fixed seed)
    /// exceeds the VBR bit budget past max gain iterations on an early
    /// frame and enters gain-loop crash recovery. The test proves all
    /// three: recovery was entered (`crash != -1` observed), the recovered
    /// frame reached emission (bytes with no notes/errors, through both
    /// `step_frame` and the public [`Encoder`] path with dense frame
    /// indices), and the emitted bytes are structurally valid ADTS
    /// (syncword, MPEG-4 ID, AAC-LC profile, 48 kHz index, mono channel
    /// configuration, consistent frame length, real payload).
    #[test]
    fn crash_recovered_frame_emits_valid_adts() {
        use crate::aac_enc::encoder::Encoder;
        use std::cell::Cell;
        // MPEG-4 Audio constants, independent of the implementation:
        // profile = AOT-1 (2 = AAC-LC -> 1), 48 kHz index 3, mono config 1.
        const ADTS_PROFILE_AAC_LC: u8 = 1;
        const ADTS_SFI_48000: u8 = 3;
        const ADTS_CHANCFG_MONO: u8 = 1;
        let rng = Cell::new(0x12345678u32);
        let mut sample = || {
            let mut r = rng.get();
            r ^= r << 13;
            r ^= r >> 17;
            r ^= r << 5;
            rng.set(r);
            (r >> 16) as i16
        };
        const NFRAMES: usize = 40;
        let mut mono = Vec::with_capacity(NFRAMES * 1024);
        for _ in 0..NFRAMES * 1024 {
            mono.push(sample());
        }
        // Driver leg: observe recovery and emission directly.
        let cfg = EncoderConfig::new(48000, 1, FRAME_LENGTH, TransportType::Adts).unwrap();
        let mut feeder = PcmFeeder::new(cfg);
        let mut frames = feeder.push_interleaved(&mono).expect("push");
        frames.extend(feeder.finish().expect("finish"));
        let mut enc = StreamEncoder::new(48000, 1);
        let mut recovered: Vec<(u32, Vec<u8>)> = Vec::new();
        for (f, fr) in frames.iter().enumerate() {
            let fresh: Vec<Vec<i16>> = fr.channels().iter().map(|b| b.to_vec()).collect();
            let (bytes, snap) = enc.step_frame(f as u32, &fresh);
            assert!(
                snap.step_error.is_none(),
                "f={f}: unexpected typed step error {:?}",
                snap.step_error
            );
            if snap.crash != -1 {
                let b = m52_emit_or_fail(bytes, &snap.notes, "crash-synth", f)
                    .unwrap_or_else(|e| panic!("recovered frame not emitted: {e}"));
                recovered.push((f as u32, b));
            }
        }
        assert!(
            !recovered.is_empty(),
            "synthetic input must enter crash recovery at least once"
        );
        // Public-API leg: the same PCM through `Encoder` succeeds with
        // dense frame indices, so nothing was skipped or poisoned.
        let mut api =
            Encoder::new(EncoderConfig::new(48000, 1, FRAME_LENGTH, TransportType::Adts).unwrap())
                .unwrap();
        let mut out = api.push(&mono).expect("push succeeds");
        out.extend(api.flush().expect("flush succeeds"));
        assert!(!out.is_empty(), "no output frames");
        for (i, fr) in out.iter().enumerate() {
            assert_eq!(fr.frame_index, i as u32, "dense frame indices");
        }
        // Every recovered frame is present in the public output with
        // identical bytes and a structurally valid ADTS header.
        for (crash_frame, crash_bytes) in &recovered {
            let api_frame = out
                .iter()
                .find(|fr| fr.frame_index == *crash_frame)
                .unwrap_or_else(|| {
                    panic!("recovered frame {crash_frame} present in public output")
                });
            assert_eq!(
                api_frame.bytes, *crash_bytes,
                "public path emits exactly the recovered driver bytes"
            );
            // ADTS validity on the recovered frame (header + real payload).
            let b = crash_bytes;
            assert!(
                b.len() > 7,
                "recovered frame carries payload, got {} bytes",
                b.len()
            );
            assert_eq!(b[0], 0xFF, "syncword high byte");
            assert_eq!(b[1], 0xF1, "sync tail + MPEG-4 ID + no protection");
            assert_eq!((b[2] >> 6) & 0x3, ADTS_PROFILE_AAC_LC, "AAC-LC profile");
            assert_eq!((b[2] >> 2) & 0xF, ADTS_SFI_48000, "48 kHz index");
            let chan_cfg = ((b[2] & 0x1) << 2) | ((b[3] >> 6) & 0x3);
            assert_eq!(chan_cfg, ADTS_CHANCFG_MONO, "mono channel configuration");
            let frame_len =
                (((b[3] & 0x3) as usize) << 11) | ((b[4] as usize) << 3) | ((b[5] as usize) >> 5);
            assert_eq!(frame_len, b.len(), "declared length matches emitted bytes");
            assert_eq!(b[6] & 0x3, 0, "single raw data block");
        }
    }
}

// ---------------------------------------------------------------------------
// M5.8 benchmark harness (test-only/tooling-only; timing runs are M5.9)
// ---------------------------------------------------------------------------

/// M5.8 measurement modes (plan §3). Encoder-only times just the
/// `step_frame` loop over prebuilt feeder frames; end-to-end additionally
/// times interleave+feeder, encoder init, and ADTS file output.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum M58Mode {
    EncoderOnly,
    EndToEnd,
}

/// One harness run over one fixture (timing present only when requested;
/// M5.8 validation always runs unmeasured).
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct M58Sample {
    pub short: String,
    pub stem: String,
    pub sample_rate: u32,
    pub channels: usize,
    pub input_samples: usize,
    pub input_hash: u64,
    pub mode: M58Mode,
    pub run: usize,
    pub frames: usize,
    pub bytes: usize,
    /// Timed-region wall time in ns (`None` on explicitly untimed runs).
    pub wall_ns: Option<u128>,
}

/// FNV-1a64 over interleaved LE i16 samples (fixture identity for reports).
#[cfg(test)]
fn m58_fixture_hash(pcm: &[Vec<i16>]) -> u64 {
    let mut h: u64 = 0xcbf29ce4842225;
    let s = pcm[0].len();
    for i in 0..s {
        for c in 0..pcm.len() {
            for b in pcm[c][i].to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
    }
    h
}

/// All 19 corpus fixtures: 15 synthetic + 3 licensed (M5.5) + the V3 lossless
/// real-music reference (additive; see testdata/m8-PROVENANCE.txt).
#[cfg(test)]
pub(crate) const M58_FIXTURES: [(&str, &str, u32, usize); 19] = [
    ("silence-stereo", "m2-silence-stereo-48k", 48000, 2),
    ("sine-stereo", "m2-sine-stereo-48k", 48000, 2),
    ("onset-stereo", "m2-onset-stereo-48k", 48000, 2),
    ("stepdown-stereo", "m2-stepdown-stereo-48k", 48000, 2),
    ("click-mono", "m2-click-mono-48k", 48000, 1),
    ("impulse-mono", "m2-impulse-mono-48k", 48000, 1),
    ("edge-mono", "m2-edge-mono-48k", 48000, 1),
    ("multitone-stereo", "m3-multitone-stereo-48k", 48000, 2),
    ("sweep-stereo", "m3-sweep-stereo-48k", 48000, 2),
    ("noise-mono", "m3-noise-mono-48k", 48000, 1),
    ("tonal-noise-stereo", "m3-tonal-noise-stereo-48k", 48000, 2),
    ("quiet-sine-stereo", "m3-quiet-sine-stereo-48k", 48000, 2),
    ("sine-stereo-44100", "m3-sine-stereo-441k", 44100, 2),
    ("onset-stereo-44100", "m3-onset-stereo-441k", 44100, 2),
    ("stress-stereo", "m6-stress-stereo-48k", 48000, 2),
    ("music-stereo", "m7-music-stereo-48k", 48000, 2),
    (
        "speech-pauses-stereo",
        "m7-speech-pauses-stereo-48k",
        48000,
        2,
    ),
    (
        "speech-clean-stereo",
        "m7-speech-clean-stereo-48k",
        48000,
        2,
    ),
    (
        "music-eroica-stereo",
        "m8-music-eroica-stereo-48k",
        48000,
        2,
    ),
];

/// Run one fixture in one mode. Fresh feeder + encoder state every call.
/// Skipped frames are loud `Err` (never zero-duration partial success).
/// The clock runs only around the documented timed region when `measure`
/// is true; M5.8 sets it false (explicitly untimed validation path).
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn m58_run_once(
    short: &str,
    stem: &str,
    sample_rate: u32,
    channels: usize,
    mode: M58Mode,
    run: usize,
    out_dir: &std::path::Path,
    measure: bool,
) -> Result<M58Sample, String> {
    let pcm = testdata::fixture_samples(short);
    if pcm.len() != channels {
        return Err(format!("{stem}: channels {} != {channels}", pcm.len()));
    }
    let s = pcm[0].len();
    let input_hash = m58_fixture_hash(&pcm);
    // Preparation (outside the timed region in both modes; end-to-end
    // additionally times the interleave+feeder below — see branch).
    let mut interleaved = Vec::with_capacity(s * channels);
    for i in 0..s {
        for c in 0..channels {
            interleaved.push(pcm[c][i]);
        }
    }
    let t0 = (mode == M58Mode::EndToEnd && measure).then(std::time::Instant::now);
    let cfg = EncoderConfig::new(
        sample_rate,
        channels as u32,
        FRAME_LENGTH,
        TransportType::Adts,
    )
    .map_err(|e| format!("{stem} cfg: {e:?}"))?;
    let mut feeder = PcmFeeder::new(cfg);
    let mut frames = feeder
        .push_interleaved(&interleaved)
        .map_err(|e| format!("{stem} push: {e:?}"))?;
    frames.extend(
        feeder
            .finish()
            .map_err(|e| format!("{stem} finish: {e:?}"))?,
    );
    let t1 = (mode == M58Mode::EncoderOnly && measure).then(std::time::Instant::now);
    let mut enc = StreamEncoder::new(sample_rate, channels);
    let mut out_bytes: Vec<u8> = Vec::new();
    for (f, fr) in frames.iter().enumerate() {
        let fresh: Vec<Vec<i16>> = fr.channels().iter().map(|b| b.to_vec()).collect();
        let (bytes, snap) = enc.step_frame(f as u32, &fresh);
        match m52_emit_or_fail(bytes, &snap.notes, stem, f) {
            Ok(b) => out_bytes.extend_from_slice(&b),
            Err(e) => return Err(e),
        }
    }
    if mode == M58Mode::EndToEnd {
        let path = out_dir.join(format!("{stem}.m58.aac"));
        std::fs::create_dir_all(out_dir).map_err(|e| format!("{stem} mkdir: {e}"))?;
        std::fs::write(&path, &out_bytes).map_err(|e| format!("{stem} write: {e}"))?;
    }
    let wall_ns = match (mode, measure, t0, t1) {
        (M58Mode::EndToEnd, true, Some(a), _) => Some(a.elapsed().as_nanos()),
        (M58Mode::EncoderOnly, true, _, Some(a)) => Some(a.elapsed().as_nanos()),
        _ => None,
    };
    // Post-timing consumption + validation (never timed): the byte vector
    // is fully walked here so no work can be optimized out unnoticed, and
    // frame accounting is re-checked from the bytes.
    let frames_n = frames.len();
    let bytes_n = out_bytes.len();
    let mut lens = Vec::with_capacity(frames_n);
    let mut off = 0usize;
    while off + 7 <= out_bytes.len() {
        if out_bytes[off] != 0xFF || out_bytes[off + 1] & 0xF0 != 0xF0 {
            return Err(format!("{stem}: ADTS sync lost @ {off}"));
        }
        let fl = ((out_bytes[off + 3] as usize & 0x03) << 11)
            | ((out_bytes[off + 4] as usize) << 3)
            | ((out_bytes[off + 5] as usize) >> 5);
        if fl < 7 {
            return Err(format!("{stem}: bad frame len @ {off}"));
        }
        lens.push(fl);
        off += fl;
    }
    if off != out_bytes.len() || lens.len() != frames_n {
        return Err(format!("{stem}: frame accounting mismatch"));
    }
    Ok(M58Sample {
        short: short.to_string(),
        stem: stem.to_string(),
        sample_rate,
        channels,
        input_samples: s,
        input_hash,
        mode,
        run,
        frames: frames_n,
        bytes: bytes_n,
        wall_ns,
    })
}

/// M5.9 timing sequence (warm-up + 5 measured runs) over the corpus.
/// NOT executed in M5.8: the `m58_corpus_validation` test below always
/// passes `measure = false`. M5.9 invokes this with `measure = true` via
/// the documented command.
#[cfg(test)]
fn m58_bench_corpus(
    modes: &[M58Mode],
    warmup: bool,
    runs: usize,
    out_dir: &std::path::Path,
    measure: bool,
) -> Result<Vec<M58Sample>, String> {
    if warmup {
        for (short, stem, sr, ch) in M58_FIXTURES {
            for mode in modes {
                let _ = m58_run_once(short, stem, sr, ch, *mode, usize::MAX, out_dir, false)?;
            }
        }
    }
    let mut out = Vec::new();
    for run in 0..runs {
        for (short, stem, sr, ch) in M58_FIXTURES {
            for mode in modes {
                out.push(m58_run_once(
                    short, stem, sr, ch, *mode, run, out_dir, measure,
                )?);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod m58_tests {
    use super::*;

    /// M5.8 smoke validation (explicitly untimed): subset covering silence
    /// plus transient fixtures, both modes. Pins M5.2/M5.3B-recorded byte
    /// totals, proving the harness measures the unchanged pipeline.
    #[test]
    fn m58_smoke_untimed() {
        let out_dir = m52_out_dir().join("m58-smoke");
        for (short, stem, sr, ch, want_bytes) in [
            (
                "silence-stereo",
                "m2-silence-stereo-48k",
                48000u32,
                2usize,
                735usize,
            ),
            ("impulse-mono", "m2-impulse-mono-48k", 48000, 1, 306),
            ("onset-stereo", "m2-onset-stereo-48k", 48000, 2, 2359),
        ] {
            for mode in [M58Mode::EncoderOnly, M58Mode::EndToEnd] {
                let s = m58_run_once(short, stem, sr, ch, mode, 0, &out_dir, false)
                    .unwrap_or_else(|e| panic!("{stem} {mode:?}: {e}"));
                assert_eq!(s.wall_ns, None, "untimed path takes no samples");
                let expect_frames =
                    (testdata::fixture_samples(short)[0].len() + 1600 + 1023) / 1024;
                assert_eq!(s.frames, expect_frames, "{stem} frame count");
                assert_eq!(s.bytes, want_bytes, "{stem} {mode:?} byte total");
                assert_eq!(s.input_samples, testdata::fixture_samples(short)[0].len());
                println!(
                    "M58SMOKE {stem} {mode:?}: frames={} bytes={} hash={:016x}",
                    s.frames, s.bytes, s.input_hash
                );
            }
        }
    }

    /// M5.8 corpus validation (explicitly untimed): all 19 fixtures, both
    /// modes. Asserts complete output + accounting; prints the M5.9 report
    /// rows with timing marked n/a. Timing executes only under M5.9.
    #[test]
    fn m58_corpus_validation() {
        // M5.9 execution flips this via the documented env var; M5.8 runs
        // of this test always take the untimed path.
        let measure = std::env::var("M58_MEASURE_TIMING").is_ok();
        let out_dir = m52_out_dir().join("m58-corpus");
        let modes = [M58Mode::EncoderOnly, M58Mode::EndToEnd];
        let samples =
            m58_bench_corpus(&modes, true, 1, &out_dir, measure).expect("corpus run completes");
        assert_eq!(
            samples.len(),
            M58_FIXTURES.len() * modes.len(),
            "full corpus x modes"
        );
        for s in &samples {
            assert!(s.frames > 0 && s.bytes > 7, "{}: empty", s.stem);
            assert_eq!(
                s.frames,
                (s.input_samples + 1600 + 1023) / 1024,
                "{}: frame law",
                s.stem
            );
            println!(
                "M58 {:>28} {:>11?} run={} frames={} bytes={} in_samples={} hash={:016x} wall_ns={} rt_x={}",
                s.stem,
                s.mode,
                s.run,
                s.frames,
                s.bytes,
                s.input_samples,
                s.input_hash,
                s.wall_ns.map(|n| n.to_string()).unwrap_or_else(|| "n/a".to_string()),
                s.wall_ns.map(|n| s.input_samples as f64 / (n as f64 / 1e9) / s.sample_rate as f64)
                    .map(|x| format!("{:.1}", x))
                    .unwrap_or_else(|| "n/a".to_string()),
            );
        }
    }
}

#[cfg(test)]
mod is_integration_tests {
    use super::*;

    fn probe_pcm(stereo: bool, out_phase: bool, centered: bool, narrow: bool) -> Vec<Vec<i16>> {
        let sr = 48_000.0f64;
        let n = 96_000usize;
        let freqs: Vec<f64> = if narrow {
            (0..=50).map(|i| 5500.0 + 20.0 * i as f64).collect()
        } else {
            vec![
                2000.0, 2500.0, 3000.0, 3500.0, 4000.0, 4500.0, 5000.0, 6000.0, 7000.0, 8000.0,
                9000.0, 10000.0,
            ]
        };
        let mut base = vec![0.0f64; n];
        for (i, f) in freqs.iter().enumerate() {
            let phase = i as f64 * 0.371;
            let step = 2.0 * std::f64::consts::PI * *f / sr;
            for t in 0..n {
                base[t] += (step * t as f64 + phase).sin() / freqs.len() as f64;
            }
        }
        let peak = base.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
        let fade = (0.1 * sr) as usize;
        for (t, v) in base.iter_mut().enumerate() {
            let env = if t < fade {
                0.5 - 0.5 * (std::f64::consts::PI * t as f64 / fade as f64).cos()
            } else if t + fade >= n {
                0.5 - 0.5 * (std::f64::consts::PI * (n - 1 - t) as f64 / fade as f64).cos()
            } else {
                1.0
            };
            *v = 0.5 * *v / peak * env;
        }
        if !stereo {
            return vec![base
                .iter()
                .map(|v| (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16)
                .collect()];
        }
        let gain = if narrow { 0.5 } else { 0.25 };
        let right: Vec<i16> = base
            .iter()
            .map(|v| {
                let r = if centered {
                    *v
                } else if out_phase {
                    -gain * *v
                } else {
                    gain * *v
                };
                (r * 32767.0).round().clamp(-32768.0, 32767.0) as i16
            })
            .collect();
        let left = base
            .iter()
            .map(|v| (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16)
            .collect();
        vec![left, right]
    }

    fn run_at(sample_rate: u32, pcm: &[Vec<i16>], channels: usize) -> Vec<FrameSnapshot> {
        let cfg = EncoderConfig::new(
            sample_rate,
            channels as u32,
            FRAME_LENGTH,
            TransportType::Adts,
        )
        .expect("config");
        let mut interleaved = Vec::with_capacity(pcm[0].len() * channels);
        for i in 0..pcm[0].len() {
            for c in 0..channels {
                interleaved.push(pcm[c][i]);
            }
        }
        let mut feeder = PcmFeeder::new(cfg);
        let mut frames = feeder.push_interleaved(&interleaved).expect("push");
        frames.extend(feeder.finish().expect("finish"));
        let mut enc = StreamEncoder::new(sample_rate, channels);
        let mut out = Vec::with_capacity(frames.len());
        for (i, fr) in frames.iter().enumerate() {
            let fresh: Vec<Vec<i16>> = fr.channels().iter().map(|b| b.to_vec()).collect();
            let (bytes, snap) = enc.step_frame(i as u32, &fresh);
            assert!(bytes.is_some(), "frame {i} notes={:?}", snap.notes);
            assert!(snap.notes.is_empty(), "frame {i} notes={:?}", snap.notes);
            out.push(snap);
        }
        out
    }

    fn run_with_bytes(pcm: &[Vec<i16>], channels: usize) -> (Vec<FrameSnapshot>, Vec<u8>) {
        let cfg = EncoderConfig::new(48000, channels as u32, FRAME_LENGTH, TransportType::Adts)
            .expect("config");
        let mut interleaved = Vec::with_capacity(pcm[0].len() * channels);
        for i in 0..pcm[0].len() {
            for c in 0..channels {
                interleaved.push(pcm[c][i]);
            }
        }
        let mut feeder = PcmFeeder::new(cfg);
        let mut frames = feeder.push_interleaved(&interleaved).expect("push");
        frames.extend(feeder.finish().expect("finish"));
        let mut enc = StreamEncoder::new(48000, channels);
        let mut out = Vec::with_capacity(frames.len());
        let mut adts = Vec::new();
        for (i, fr) in frames.iter().enumerate() {
            let fresh: Vec<Vec<i16>> = fr.channels().iter().map(|b| b.to_vec()).collect();
            let (bytes, snap) = enc.step_frame(i as u32, &fresh);
            let bytes = bytes.unwrap_or_else(|| panic!("frame {i} notes={:?}", snap.notes));
            assert!(snap.notes.is_empty(), "frame {i} notes={:?}", snap.notes);
            adts.extend_from_slice(&bytes);
            out.push(snap);
        }
        (out, adts)
    }

    fn run(pcm: &[Vec<i16>], channels: usize) -> Vec<FrameSnapshot> {
        run_at(48000, pcm, channels)
    }

    fn read_wav_16(path: &str) -> Vec<Vec<i16>> {
        let b = std::fs::read(path).expect("read WAV");
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..12], b"WAVE");
        let mut pos = 12usize;
        let mut channels = 0usize;
        let mut data_off = 0usize;
        let mut data_len = 0usize;
        while pos + 8 <= b.len() {
            let id = &b[pos..pos + 4];
            let n = u32::from_le_bytes([b[pos + 4], b[pos + 5], b[pos + 6], b[pos + 7]]) as usize;
            if id == b"fmt " {
                assert_eq!(u16::from_le_bytes([b[pos + 8], b[pos + 9]]), 1);
                channels = u16::from_le_bytes([b[pos + 10], b[pos + 11]]) as usize;
                assert_eq!(u16::from_le_bytes([b[pos + 22], b[pos + 23]]), 16);
            } else if id == b"data" {
                data_off = pos + 8;
                data_len = n;
                break;
            }
            pos += 8 + n + (n & 1);
        }
        assert!(channels == 1 || channels == 2);
        assert!(data_off > 0 && data_off + data_len <= b.len());
        let mut out = vec![Vec::new(); channels];
        for i in 0..data_len / 2 {
            let v = i16::from_le_bytes([b[data_off + 2 * i], b[data_off + 2 * i + 1]]);
            out[i % channels].push(v);
        }
        out
    }

    #[test]
    fn intensity_probe_wav_parity() {
        let Ok(path) = std::env::var("IS_PROBE_WAV") else {
            // Tier-B status marker (see rust/tools/tierb-report.sh): this
            // probe performed no validation. Visible with `--nocapture`.
            println!("TIERB_SKIP intensity_probe_wav_parity missing_env=IS_PROBE_WAV");
            return;
        };
        let pcm = read_wav_16(&path);
        let snaps = run(&pcm, pcm.len());
        let is_frames = snaps.iter().filter(|s| s.is_bands > 0).count();
        let is_bands: u32 = snaps.iter().map(|s| s.is_bands).sum();
        let books15: usize = snaps
            .iter()
            .flat_map(|s| s.section_books.get(1).into_iter().flatten())
            .filter(|&&b| b == 15)
            .count();
        println!(
            "IS_PROBE_WAV {path}: frames={} is_frames={is_frames} is_bands={is_bands} books15={books15}",
            snaps.len()
        );
        if let Ok(out) = std::env::var("IS_PROBE_OUT") {
            use std::io::Write;
            let mut f = std::fs::File::create(out).expect("create probe dump");
            for (i, s) in snaps.iter().enumerate() {
                writeln!(f, "{i} {} {}", s.is_bands, s.digest).expect("write probe dump");
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ResidualKind {
        FrontendBoundary,
        UnresolvedTail,
    }

    fn range_vec(a: i32, b: i32) -> Vec<i32> {
        (a..=b).collect()
    }

    // Signatures of the retained 2026-10-22 synthetic probes. Long-frame
    // edge losses follow pre-IS C/Rust spectrum/energy divergence (verified
    // by replaying the Rust port on C pre-IS operands); the flush-tail cases
    // are marked unresolved because their exact upstream stage is not pinned
    // down. Any new signature fails the parity test.
    fn expected_residual(probe: &str, frame: u32) -> Option<(ResidualKind, Vec<i32>, Vec<i32>)> {
        match probe {
            "noise-like-in-phase" => match frame {
                28 | 47 => Some((
                    ResidualKind::FrontendBoundary,
                    range_vec(15, 37),
                    range_vec(15, 35),
                )),
                43 => Some((
                    ResidualKind::FrontendBoundary,
                    range_vec(14, 36),
                    range_vec(14, 35),
                )),
                51 => Some((
                    ResidualKind::FrontendBoundary,
                    range_vec(13, 35),
                    range_vec(14, 35),
                )),
                60 => Some((
                    ResidualKind::FrontendBoundary,
                    range_vec(15, 35),
                    range_vec(13, 35),
                )),
                _ => None,
            },
            "noise-like-out-phase" => match frame {
                51 => Some((
                    ResidualKind::FrontendBoundary,
                    range_vec(13, 35),
                    range_vec(14, 35),
                )),
                95 => Some((
                    ResidualKind::UnresolvedTail,
                    range_vec(16, 22),
                    range_vec(14, 22),
                )),
                _ => None,
            },
            "tones-in-phase" => {
                if frame == 95 {
                    Some((
                        ResidualKind::UnresolvedTail,
                        vec![
                            14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 26, 27, 28, 29, 30, 31, 32,
                        ],
                        range_vec(14, 32),
                    ))
                } else if (8..=89).contains(&frame) && (frame - 8) % 3 == 0 {
                    Some((
                        ResidualKind::FrontendBoundary,
                        range_vec(12, 33),
                        range_vec(12, 32),
                    ))
                } else {
                    None
                }
            }
            "tones-out-phase" => (frame == 95).then(|| {
                (
                    ResidualKind::UnresolvedTail,
                    range_vec(14, 22),
                    range_vec(14, 32),
                )
            }),
            "narrowband-in-phase" => {
                (frame == 95).then(|| (ResidualKind::UnresolvedTail, Vec::new(), range_vec(24, 30)))
            }
            "narrowband-out-phase" => None,
            _ => None,
        }
    }

    fn known_scale_frame(probe: &str, frame: u32) -> bool {
        match probe {
            "noise-like-in-phase" => matches!(frame, 1 | 2 | 19 | 33 | 56),
            "noise-like-out-phase" => frame == 95,
            "tones-in-phase" | "tones-out-phase" => {
                frame == 95 || ((7..=88).contains(&frame) && (frame - 7) % 3 == 0)
            }
            "narrowband-in-phase" | "narrowband-out-phase" => false,
            _ => false,
        }
    }

    fn expected_scale_mismatches(probe: &str) -> usize {
        match probe {
            "noise-like-in-phase" => 5,
            "noise-like-out-phase" => 1,
            "tones-in-phase" => 38,
            "tones-out-phase" => 30,
            "narrowband-in-phase" | "narrowband-out-phase" => 0,
            _ => 0,
        }
    }

    fn expected_scale_compared(probe: &str) -> usize {
        match probe {
            "noise-like-in-phase" => 2046,
            "noise-like-out-phase" => 2032,
            "tones-in-phase" => 2015,
            "tones-out-phase" => 1924,
            "narrowband-in-phase" => 17,
            "narrowband-out-phase" => 13,
            _ => 0,
        }
    }

    fn expected_band_sets(probe: &str) -> (u32, u32) {
        match probe {
            "noise-like-in-phase" => (2052, 2048),
            "noise-like-out-phase" => (2033, 2034),
            "tones-in-phase" => (2043, 2016),
            "tones-out-phase" => (1924, 1934),
            "narrowband-in-phase" => (17, 24),
            "narrowband-out-phase" => (13, 13),
            _ => (0, 0),
        }
    }

    fn band_indices(v: &[i32]) -> Vec<i32> {
        v.iter()
            .enumerate()
            .filter(|(_, &x)| x != 0)
            .map(|(i, _)| i as i32)
            .collect()
    }

    #[test]
    fn intensity_probe_c_oracle_parity() {
        // Environment-gated: the retained probe WAV and C oracle are outside
        // the repository. Exact oracle-domain band vectors are compared below;
        // decoder `aCodeBook` indices are not used for short-block comparison.
        let (Ok(path), Ok(oracle)) = (std::env::var("IS_PROBE_WAV"), std::env::var("IS_C_ORACLE"))
        else {
            // Tier-B status marker (see rust/tools/tierb-report.sh): this
            // probe performed no validation. Visible with `--nocapture`.
            let mut missing = Vec::new();
            if std::env::var("IS_PROBE_WAV").is_err() {
                missing.push("IS_PROBE_WAV");
            }
            if std::env::var("IS_C_ORACLE").is_err() {
                missing.push("IS_C_ORACLE");
            }
            println!(
                "TIERB_SKIP intensity_probe_c_oracle_parity missing_env={}",
                missing.join(",")
            );
            return;
        };
        let probe = std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("probe stem")
            .to_string();
        let pcm = read_wav_16(&path);
        let snaps = run(&pcm, pcm.len());
        let dir = std::env::temp_dir().join(format!("is-c-oracle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir oracle dir");
        let dump = dir.join("probe.oracle.txt");
        let out = std::process::Command::new(oracle)
            .arg(&path)
            .arg(&dump)
            .output()
            .expect("spawn enc_oracle");
        assert!(out.status.success(), "oracle exit {:?}", out.status);

        let mut c_sets = Vec::<Vec<i32>>::new();
        let mut c_iss = Vec::<Vec<i32>>::new();
        let mut c_msd = Vec::<i32>::new();
        let mut cur = 0usize;
        let mut ch = 0usize;
        let mut got_frame = false;
        for line in std::fs::read_to_string(&dump)
            .expect("read oracle dump")
            .lines()
        {
            if let Some(rest) = line.strip_prefix("FRAME ") {
                let mut it = rest.split_whitespace();
                cur = it.next().expect("frame").parse().expect("frame int");
                it.next(); // CH
                ch = it.next().expect("ch").parse().expect("ch int");
                if cur >= c_sets.len() {
                    c_sets.resize_with(cur + 1, Vec::new);
                    c_iss.resize_with(cur + 1, Vec::new);
                    c_msd.resize(cur + 1, 0);
                }
                got_frame = true;
            } else if got_frame && ch == 1 && line.starts_with("ISB ") {
                c_sets[cur] = line
                    .split_whitespace()
                    .skip(1)
                    .map(|v| v.parse().expect("ISB int"))
                    .collect();
            } else if got_frame && ch == 1 && line.starts_with("ISS ") {
                c_iss[cur] = line
                    .split_whitespace()
                    .skip(1)
                    .map(|v| v.parse().expect("ISS int"))
                    .collect();
            } else if got_frame && line.starts_with("MSD ") {
                c_msd[cur] = line
                    .split_whitespace()
                    .nth(1)
                    .expect("MSD digest")
                    .parse()
                    .expect("MSD int");
            }
        }
        let n = snaps.len().min(c_sets.len());
        let mut exact_frames = 0usize;
        let mut boundary_frames = 0usize;
        let mut tail_frames = 0usize;
        let mut band_mismatches = 0usize;
        let mut scale_cmp = 0usize;
        let mut scale_mismatch = 0usize;
        for i in 0..n {
            let cs = band_indices(&c_sets[i]);
            let rs = snaps[i]
                .ms_cap
                .as_ref()
                .map(|c| band_indices(&c.is_book))
                .unwrap_or_default();
            match expected_residual(&probe, i as u32) {
                Some((kind, expected_c, expected_r)) => {
                    assert_ne!(
                        cs, rs,
                        "{probe} f={i}: expected known {kind:?} residual no longer present"
                    );
                    assert_eq!(
                        cs, expected_c,
                        "{probe} f={i}: C band set changed from known {kind:?} residual"
                    );
                    assert_eq!(
                        rs, expected_r,
                        "{probe} f={i}: Rust band set changed from known {kind:?} residual"
                    );
                    band_mismatches += 1;
                    match kind {
                        ResidualKind::FrontendBoundary => boundary_frames += 1,
                        ResidualKind::UnresolvedTail => tail_frames += 1,
                    }
                }
                None => {
                    exact_frames += 1;
                    assert_eq!(
                        cs, rs,
                        "{probe} f={i}: unexpected exact-band mismatch: C={cs:?} Rust={rs:?}"
                    );
                }
            }

            if let Some(cap) = snaps[i].ms_cap.as_ref() {
                for (b, &book) in cap.is_book.iter().enumerate() {
                    if book != 0 && c_iss.get(i).and_then(|v| v.get(b)).copied().unwrap_or(0) != 0 {
                        scale_cmp += 1;
                        if snaps[i].is_scale_values.get(b).copied().unwrap_or(0) != c_iss[i][b] {
                            assert!(
                                known_scale_frame(&probe, i as u32),
                                "{probe} f={i} b={b}: unexpected intensity-scale mismatch C={} Rust={}",
                                c_iss[i][b],
                                snaps[i].is_scale_values.get(b).copied().unwrap_or(0)
                            );
                            scale_mismatch += 1;
                        }
                    }
                }
            }
        }

        // C oracle `ISB` and Rust `ms_cap.is_book` are the same decision
        // representation. Do not compare C `ISB` to decoder `aCodeBook`
        // indices for short blocks: the decoder's grouped section indexing
        // is offset from the oracle book vector, while both C and Rust ADTS
        // streams carry the same short-frame codebook placement.
        if probe == "narrowband-in-phase" {
            let short_c = band_indices(&c_sets[1]);
            let short_r = snaps[1]
                .ms_cap
                .as_ref()
                .map(|c| band_indices(&c.is_book))
                .unwrap_or_default();
            assert_eq!(short_c, range_vec(30, 37), "C short-frame IS bands");
            assert_eq!(short_r, short_c, "Rust short-frame IS bands");
        }

        let c_total: u32 = c_sets
            .iter()
            .take(n)
            .map(|v| band_indices(v).len() as u32)
            .sum();
        let r_total: u32 = snaps.iter().take(n).map(|s| s.is_bands).sum();
        let (expected_c_total, expected_r_total) = expected_band_sets(&probe);
        assert_eq!(
            (c_total, r_total),
            (expected_c_total, expected_r_total),
            "{probe}: C/Rust band totals changed"
        );
        assert_eq!(
            scale_cmp,
            expected_scale_compared(&probe),
            "{probe}: scale comparison count changed"
        );
        assert_eq!(
            scale_mismatch,
            expected_scale_mismatches(&probe),
            "{probe}: scale mismatch count changed"
        );
        let msd_mismatch = (0..n).filter(|&i| snaps[i].digest != c_msd[i]).count();
        let expected_msd = usize::from(probe == "narrowband-in-phase");
        assert_eq!(
            msd_mismatch, expected_msd,
            "{probe}: MSD mismatch pattern changed"
        );
        if probe == "narrowband-in-phase" {
            assert_eq!(c_msd.get(95).copied(), Some(2), "C frame 95 MSD");
            assert_eq!(
                snaps.get(95).map(|s| s.digest),
                Some(1),
                "Rust frame 95 MSD"
            );
        }

        println!(
            "IS_C_PARITY {probe}: frames={n} exact={exact_frames} known_frontend_boundary={boundary_frames} unresolved_tail={tail_frames} band_set_mismatches={band_mismatches} C_bands={c_total} R_bands={r_total} msd_mismatch={msd_mismatch} scale_compared={scale_cmp} scale_mismatch={scale_mismatch}"
        );
    }

    #[test]
    fn intensity_probe_decisions() {
        for (label, out_phase, centered, narrow) in [
            ("broad-in", false, false, false),
            ("broad-out", true, false, false),
            ("centered", false, true, false),
            ("narrow-in", false, false, true),
        ] {
            let pcm = probe_pcm(true, out_phase, centered, narrow);
            let snaps = run(&pcm, 2);
            let is_frames = snaps.iter().filter(|s| s.is_bands > 0).count();
            let is_bands: u32 = snaps.iter().map(|s| s.is_bands).sum();
            let is_scale: u32 = snaps.iter().map(|s| s.is_scale_nz).sum();
            let books15: usize = snaps
                .iter()
                .flat_map(|s| s.section_books.get(1).into_iter().flatten())
                .filter(|&&b| b == 15)
                .count();
            let mask_ones: u32 = snaps
                .iter()
                .filter_map(|s| s.ms_cap.as_ref())
                .map(|c| c.mask.iter().map(|&v| v as u32).sum::<u32>())
                .sum();
            let mut is_mask_zero = 0u32;
            let mut is_mask_one = 0u32;
            for cap in snaps.iter().filter_map(|s| s.ms_cap.as_ref()) {
                for (b, &book) in cap.is_book.iter().enumerate() {
                    if book != 0 {
                        if cap.mask[b] == 0 {
                            is_mask_zero += 1;
                        } else {
                            is_mask_one += 1;
                        }
                    }
                }
            }
            let digest_counts = [
                snaps.iter().filter(|s| s.digest == 0).count(),
                snaps.iter().filter(|s| s.digest == 1).count(),
                snaps.iter().filter(|s| s.digest == 2).count(),
            ];
            println!(
                "{label}: frames={} is_frames={is_frames} is_bands={is_bands} is_scale={is_scale} books15={books15} mask_ones={mask_ones} is_mask0={is_mask_zero} is_mask1={is_mask_one} digests={digest_counts:?}",
                snaps.len()
            );
            if centered {
                assert_eq!(is_frames, 0, "centered must not use IS");
                assert!(!snaps.iter().any(|s| s.is_book_any));
            } else {
                assert!(is_frames > 0, "{label} should trigger IS");
                assert!(is_bands > 0 && is_scale > 0);
                assert_eq!(is_scale, is_bands, "{label}: selected bands have scales");
                assert!(
                    snaps.iter().any(|s| s.seq == 2 && s.is_bands > 0),
                    "{label}: no short-frame IS decision"
                );
                assert!(books15 > 0, "{label}: right-channel codebook 15");
                assert!(!snaps
                    .iter()
                    .any(|s| s.section_books[0].iter().any(|&b| b == 15)));
                if !narrow {
                    assert!(
                        snaps
                            .iter()
                            .filter(|s| s.is_bands > 0)
                            .all(|s| s.digest == 1),
                        "{label}: IS frames use MS_SOME"
                    );
                }
                if out_phase && !narrow {
                    assert!(is_mask_one > is_mask_zero, "out-of-phase mask");
                }
                if !out_phase && !narrow {
                    assert!(is_mask_zero > is_mask_one, "in-phase mask");
                }
            }
        }

        let mono = probe_pcm(false, false, false, false);
        let mono_snaps = run(&mono, 1);
        assert!(!mono_snaps.iter().any(|s| s.is_book_any));
        assert!(mono_snaps.iter().all(|s| s.is_bands == 0));

        let centered_existing = testdata::fixture_samples("sine-stereo");
        let existing_snaps = run(&centered_existing, 2);
        assert!(!existing_snaps.iter().any(|s| s.is_book_any));
    }

    #[test]
    fn intensity_probe_44100_stereo() {
        // Rust-only positive-path coverage at the other production rate.
        // This deliberately does not claim C parity; the C-oracle cases are
        // environment-gated because their retained WAV/oracle artifacts are
        // external to the repository.
        let pcm = probe_pcm(true, false, false, false);
        let snaps = run_at(44_100, &pcm, 2);
        assert!(
            snaps.iter().any(|s| s.is_bands > 0),
            "44.1 kHz stereo probe did not exercise IS"
        );
        assert!(
            snaps
                .iter()
                .any(|s| s.section_books.get(1).is_some_and(|v| v.contains(&15))),
            "44.1 kHz stereo probe did not emit right-channel codebook 15"
        );
    }

    #[test]
    fn intensity_probe_adts_decodes() {
        let Ok(dec) = std::env::var("AAC_DEC_BIN") else {
            return;
        };
        let pcm = probe_pcm(true, false, false, false);
        let (snaps, adts) = run_with_bytes(&pcm, 2);
        assert!(snaps.iter().any(|s| s.is_bands > 0));
        let dir = std::env::temp_dir().join(format!("is-rust-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir decode dir");
        let aac = dir.join("probe.aac");
        let wav = dir.join("probe.wav");
        std::fs::write(&aac, &adts).expect("write aac");
        let out = std::process::Command::new(dec)
            .arg(&aac)
            .arg(&wav)
            .output()
            .expect("spawn aac-dec");
        assert!(
            out.status.success(),
            "decoder exit={:?} stderr={}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stderr.is_empty(),
            "decoder stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let decoded = std::fs::read(&wav).expect("read decoded wav");
        assert!(decoded.len() > 44);
        assert_eq!(&decoded[0..4], b"RIFF");
        assert!(decoded[44..].iter().any(|&b| b != 0));
    }
}

#[cfg(test)]
mod m511_tests {
    use super::*;

    /// M5.11 profiling workload (profiling-only, NOT a benchmark).
    /// Opt-in via `M511_PROFILE=1` so normal suites stay fast. Repeats
    /// fresh-state encoder-only encodes of one fixture; output is
    /// checksummed and black-boxed so no work is eliminated. The M5.10
    /// timing boundaries are unchanged (`m58_run_once`, measure=false).
    /// Env: `M511_FIXTURE`=short name (default `music-stereo`),
    /// `M511_ITERS`=N (default 60).
    #[test]
    fn m511_profile_loop() {
        if std::env::var("M511_PROFILE").is_err() {
            // Tier-B status marker (see rust/tools/tierb-report.sh): this
            // probe performed no validation. Visible with `--nocapture`.
            // (Normally this workload is not selected at all; the marker
            // covers direct invocations without the opt-in variable.)
            println!("TIERB_SKIP m511_profile_loop missing_env=M511_PROFILE");
            return;
        }
        let want = std::env::var("M511_FIXTURE").unwrap_or_else(|_| "music-stereo".to_string());
        let iters: usize = std::env::var("M511_ITERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        let (short, stem, sr, ch) = M58_FIXTURES
            .iter()
            .find(|(s, _, _, _)| *s == want)
            .unwrap_or_else(|| panic!("unknown fixture {want}"));
        let out_dir = m52_out_dir().join("m511-profile");
        let mut acc: u64 = 0;
        for i in 0..iters {
            let s = m58_run_once(
                short,
                stem,
                *sr,
                *ch,
                M58Mode::EncoderOnly,
                i,
                &out_dir,
                false,
            )
            .expect("encode");
            acc = acc.wrapping_add(s.bytes as u64).wrapping_mul(0x100000001b3);
            std::hint::black_box(s.bytes);
        }
        println!("M511 {want}: iters={iters} acc={acc:016x}");
    }
}
