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
//! Per-frame psychoacoustic orchestration (M3 output stage).
//!
//! Mirrors `FDKaacEnc_psyMain` from the transform output through short-block
//! grouping (`psy_main.cpp:552-1130`, LC path, PNS off): lowpass zeroing,
//! rescaling with integer headroom analysis, TNS detect/sync/encode with
//! energy recompute, threshold advance (clip, spreading, PCM floor,
//! pre-echo, spread energies), stereo M/S energies, short-block grouping,
//! and output log-domain vectors.
//!
//! Representation (see `energy` docs): the float spectrum carries C's
//! stored-domain scalings with exact powers of two, so every downstream
//! integer op mirrors C bit-nearly; absolute log-domain vectors derive
//! from true sums with no scale corrections. Persistent per-channel state
//! is pre-echo history (`thresholds::PreEchoState`) and TNS runtime/output
//! (`tns::TnsChannel` pair); `mdctScale` itself is recomputed per frame
//! from the measured MDCT exponent convention (12 for 1024-line, 9 for
//! short transforms, verified constant over 1000+ oracle frames).
//!
//! Multichannel elements beyond mono/stereo pairs are deferred to M4/M6;
//! LFE channels follow the C early-outs structurally (TNS and pre-echo
//! skipped) with unit coverage only.

use super::block_switch::WindowSequence;
use super::energy;
use super::q31::shift_int;
use super::sfb::{BlockKind, SfbConfig};
use super::thresholds::{apply_pcm_floor, clip_cap, init_threshold, spreading_max, PreEchoState};
use super::tns::{self, TnsChannel, TnsConfig};

/// Initial MDCT block exponent for 1024-line transforms (measured constant,
/// `MDCT_E` in every oracle dump).
pub const MDCT_SCALE_LONG: i32 = 12;
/// Initial MDCT block exponent for short transforms (measured constant).
pub const MDCT_SCALE_SHORT: i32 = 9;

/// Init-time tables for one sample rate and operating point.
pub struct PsyTables {
    /// Long-block SFB configuration.
    pub long_cfg: SfbConfig,
    /// Short-block SFB configuration.
    pub short_cfg: SfbConfig,
    /// Long-block TNS configuration.
    pub tns_long: TnsConfig,
    /// Short-block TNS configuration.
    pub tns_short: TnsConfig,
    /// Grouping min-SNR input, long (M4 computes these; until then vectors
    /// flow through from reference dumps in tests).
    pub min_snr_long: Vec<i32>,
    /// Grouping min-SNR input, short.
    pub min_snr_short: Vec<i32>,
}

/// Persistent per-channel psychoacoustic state.
pub struct PsyChannel {
    preecho: PreEchoState,
    tns_state: TnsChannel,
    tns_info: TnsChannel,
    /// M/S log-energy history (this channel's half: mid on ch0, side on
    /// ch1). Mirrors C's `sfbEnergyMSLdData`, which `CalcBandNrgMSOpt`
    /// leaves STALE when the element is short (`calcLdData=0`): short
    /// frames therefore read previous frames' values (or init zeros),
    /// verified on onset f24 w1..7 (all int 0).
    ms_ld: [[f64; 64]; 8],
}

impl PsyChannel {
    /// Fresh state; `pcm_q31_long` is the long PCM floor vector for the
    /// pre-echo initializer.
    pub fn new(pcm_q31_long: &[i32]) -> Self {
        Self {
            preecho: PreEchoState::new(pcm_q31_long),
            tns_state: TnsChannel::new(),
            tns_info: TnsChannel::new(),
            ms_ld: [[0.0; 64]; 8],
        }
    }

    /// TNS runtime decisions (test inspection).
    pub fn tns_state(&self) -> &TnsChannel {
        &self.tns_state
    }

    /// TNS output info (test inspection).
    pub fn tns_info(&self) -> &TnsChannel {
        &self.tns_info
    }

    /// Pre-echo previous thresholds (test inspection).
    pub fn preecho_prev(&self) -> &[i32] {
        self.preecho.prev()
    }
}

/// Grouped short-block output.
#[derive(Debug, Clone, Default)]
pub struct GroupedOutput {
    /// Grouped energies (stored ints).
    pub energies: Vec<i32>,
    /// Grouped thresholds (stored ints).
    pub thresholds: Vec<i32>,
    /// Grouped spread energies (stored ints).
    pub spread: Vec<i32>,
    /// Grouped energy log vectors.
    pub energy_ld: Vec<f64>,
    /// Grouped threshold log vectors (clamped).
    pub threshold_ld: Vec<f64>,
    /// Grouped band offsets.
    pub offsets: Vec<i32>,
    /// Grouped min-SNR vectors.
    pub min_snr: Vec<i32>,
    /// Regrouped spectrum (float, stored domain) for later stages.
    pub spectrum: Vec<f64>,
    /// Bands per group output.
    pub max_sfb_per_group: usize,
}

/// Per-channel per-frame output.
#[derive(Debug, Clone, Default)]
pub struct ChannelOutput {
    /// Final block exponent.
    pub mdct_scale: i32,
    /// Rescaling shift applied.
    pub final_shift: i32,
    /// TNS overflow shift applied.
    pub tns_shift: i32,
    /// Per-channel TNS recompute shift (long only; oracle `nrgScaling`).
    pub nrg_scaling: i32,
    /// Per-window headroom analysis.
    pub maxscale: Vec<Vec<i32>>,
    /// Stored energies per window.
    pub energies: Vec<Vec<i32>>,
    /// Absolute log energies per window.
    pub energy_ld: Vec<Vec<f64>>,
    /// Final thresholds per window (stored ints).
    pub thresholds: Vec<Vec<i32>>,
    /// Spread energies per window (stored ints).
    pub spread: Vec<Vec<i32>>,
    /// Post-frame pre-echo history (oracle NM1 comparison).
    pub nm1: Vec<i32>,
    /// Previous scale / enabled flag after the frame.
    pub m_nm1: i32,
    /// Calc flag after the frame (0/1).
    pub calc_pre_echo: i32,
    /// Short-block grouped output.
    pub grouped: Option<GroupedOutput>,
    /// Long-block threshold Ld output.
    pub threshold_ld: Option<Vec<f64>>,
    /// Long-block band count.
    pub max_sfb_per_group: usize,
}

/// Saturating energy add (`nrgAddSaturate`, `grp_data.cpp:114-116`).
fn nrg_add_saturate(a: i32, b: i32) -> i32 {
    if a >= i32::MAX - b {
        i32::MAX
    } else {
        a + b
    }
}

/// Init-time tables for one sample rate and operating point.
impl PsyTables {
    /// Build long/short configs plus TNS configs. `channel_bitrate` is
    /// per-channel nominal, `tns_bitrate` the TNS-init value
    /// (`(total * tnsChannels) / channelsEff`); `bandwidth` is the table
    /// value (19293 for VBR-5, clamped internally). `min_snr_*` flow
    /// through to grouping until M4 computes them.
    pub fn for_rate(
        sample_rate: u32,
        channel_bitrate: u32,
        tns_bitrate: u32,
        bandwidth: u32,
        min_snr_long: Vec<i32>,
        min_snr_short: Vec<i32>,
    ) -> Result<Self, super::sfb::SfbError> {
        let long_cfg = SfbConfig::new(
            sample_rate,
            BlockKind::Long,
            channel_bitrate,
            bandwidth,
            true,
            true,
        )?;
        let short_cfg = SfbConfig::new(
            sample_rate,
            BlockKind::Short,
            channel_bitrate,
            bandwidth,
            true,
            true,
        )?;
        let tns_long = TnsConfig::new(
            sample_rate,
            tns_bitrate,
            true,
            long_cfg.offsets(),
            long_cfg.sfb_active(),
        );
        let tns_short = TnsConfig::new(
            sample_rate,
            tns_bitrate,
            false,
            short_cfg.offsets(),
            short_cfg.sfb_active(),
        );
        Ok(Self {
            long_cfg,
            short_cfg,
            tns_long,
            tns_short,
            min_snr_long,
            min_snr_short,
        })
    }
}

/// One channel's frame input: M2 spectrum as f64 (converted once, exactly),
/// modified in place by TNS filtering and exact power-of-two scalings.
pub struct ChannelInput<'a> {
    /// Full-window spectrum (1024 lines; short windows use all of it).
    pub spectrum: &'a mut [f64; 1024],
    /// Decided window sequence (post block-switch sync).
    pub seq: WindowSequence,
    /// Short-block grouping from block switching: `(no_of_groups, lens)`.
    /// Only read for short sequences.
    pub grouping: (u8, [u8; 4]),
    /// LFE channel flag (structural early-outs only in M3).
    pub is_lfe: bool,
}

/// Element-level MS output (stereo only).
#[derive(Debug, Clone, Default)]
pub struct MsOutput {
    /// Mid energies per window (stored ints).
    pub mid: Vec<Vec<i32>>,
    /// Side energies per window.
    pub side: Vec<Vec<i32>>,
    /// Mid log energies.
    pub mid_ld: Vec<Vec<f64>>,
    /// Side log energies.
    pub side_ld: Vec<Vec<f64>>,
}

/// Per-frame M/S working state is held inline in `process_pair`.

/// Lowpass-zero the float spectrum per window; report zeroSpec.
fn lowpass_and_zero_check(
    spectrum: &mut [f64],
    n_windows: usize,
    window_len: usize,
    lowpass: usize,
) -> bool {
    for w in 0..n_windows {
        let base = w * window_len;
        for v in spectrum[base + lowpass..base + window_len].iter_mut() {
            *v = 0.0;
        }
    }
    for w in 0..n_windows {
        for &v in spectrum[w * window_len..w * window_len + lowpass].iter() {
            if v != 0.0 {
                return false;
            }
        }
    }
    true
}

/// Process a mono element (single channel) or one stereo pair.
///
/// `left`/`right` hold persistent state; inputs carry the M2 spectra.
/// Returns `(left_output, right_output, ms_output)`. LFE elements skip
/// the TNS block (mirroring the `isLFE[0]` check); LFE channels skip
/// pre-echo control.
pub fn process_pair(
    left: &mut PsyChannel,
    mut right: Option<&mut PsyChannel>,
    lin: ChannelInput<'_>,
    rin: Option<ChannelInput<'_>>,
    tables: &PsyTables,
) -> (ChannelOutput, Option<ChannelOutput>, Option<MsOutput>) {
    let stereo = right.is_some();
    assert_eq!(stereo, rin.is_some(), "channel/state pairing");

    // Per-channel static selection.
    let short_l = lin.seq == WindowSequence::Short;
    let short_r = rin
        .as_ref()
        .map(|r| r.seq == WindowSequence::Short)
        .unwrap_or(false);
    // C uses isShortWindow[0] element-globally for nrgShift (psy_main:676);
    // CPE pairs always agree post-sync.
    let short_elem = short_l;
    let conf_l = if short_l {
        &tables.short_cfg
    } else {
        &tables.long_cfg
    };
    let conf_r = if short_r {
        &tables.short_cfg
    } else {
        &tables.long_cfg
    };
    let tconf_l = if short_l {
        &tables.tns_short
    } else {
        &tables.tns_long
    };
    let tconf_r = if short_r {
        &tables.tns_short
    } else {
        &tables.tns_long
    };
    let n_win_l = if short_l { 8 } else { 1 };
    // Mono elements have no right channel: zero windows keeps every
    // right-side loop (maxscale, energies, thresholds, MS) a no-op.
    // (Latent panic for mono silence found by survey: n_win_r was 1 with
    // empty per-window vecs.)
    let n_win_r = if stereo {
        if short_r {
            8
        } else {
            1
        }
    } else {
        0
    };
    let wlen_l = if short_l { 128 } else { 1024 };
    let wlen_r = if short_r { 128 } else { 1024 };
    let active_l = if lin.is_lfe {
        conf_l.sfb_active_lfe()
    } else {
        conf_l.sfb_active()
    };
    let lowpass_l = if lin.is_lfe {
        conf_l.lowpass_line_lfe()
    } else {
        conf_l.lowpass_line()
    };
    let (active_r, lowpass_r) = match &rin {
        Some(r) => (
            if r.is_lfe {
                conf_r.sfb_active_lfe()
            } else {
                conf_r.sfb_active()
            },
            if r.is_lfe {
                conf_r.lowpass_line_lfe()
            } else {
                conf_r.lowpass_line()
            },
        ),
        None => (0, 0),
    };
    // Copy Probe flags first (Copy types, no borrow retained), then split
    // the spectrum borrows.
    let (rseq, rlfe, rgroup) = match &rin {
        Some(r) => (r.seq, r.is_lfe, r.grouping),
        None => (WindowSequence::Long, false, (1, [1, 0, 0, 0])),
    };
    let spec_l: &mut [f64] = &mut lin.spectrum[..];
    let mut rin_opt = rin;
    let mut spec_r_opt: Option<&mut [f64]> = rin_opt.as_mut().map(|r| &mut r.spectrum[..]);

    // Lowpass + zeroSpec + initial exponents.
    let zero_l = lowpass_and_zero_check(spec_l, n_win_l, wlen_l, lowpass_l);
    let zero_r = spec_r_opt
        .as_deref_mut()
        .map(|s| lowpass_and_zero_check(s, n_win_r, wlen_r, lowpass_r));
    let mut m_l = if short_l {
        MDCT_SCALE_SHORT
    } else {
        MDCT_SCALE_LONG
    };
    let mut m_r = if short_r {
        MDCT_SCALE_SHORT
    } else {
        MDCT_SCALE_LONG
    };

    // Integer lane: Q31-mapped spectra (round-to-nearest, the single
    // approximation point; all later integer ops are exact).
    let mut q_l = energy::map_spectrum_to_q31(spec_l, m_l);

    // --- Rescale stage, joint over the element. ---
    // NOTE: maxScale/energies run unconditionally (zero spectra yield 30s
    // and zeros naturally); the all-zero shortcut below applies per C only
    // when the whole element is silent (mixed elements take the full path,
    // including the Ld offset on zero bands).
    let mut maxsc_l: Vec<Vec<i32>> = vec![vec![0; active_l]; n_win_l];
    let mut min_spec_shift = i32::MAX;
    for w in 0..n_win_l {
        maxsc_l[w] = energy::max_scale_spec(
            &q_l[w * wlen_l..(w + 1) * wlen_l],
            conf_l.offsets(),
            active_l,
        );
        for &m in maxsc_l[w].iter() {
            min_spec_shift = min_spec_shift.min(m);
        }
    }
    // Right channel contributes to the global minimum (when present).
    let mut q_r: Vec<i32> = Vec::new();
    let mut maxsc_r: Vec<Vec<i32>> = Vec::new();
    if let (Some(spec_r), Some(zero_r)) = (spec_r_opt.as_deref(), zero_r) {
        let _ = zero_r;
        q_r = energy::map_spectrum_to_q31(spec_r, m_r);
        maxsc_r = vec![vec![0; active_r]; n_win_r];
        for w in 0..n_win_r {
            maxsc_r[w] = energy::max_scale_spec(
                &q_r[w * wlen_r..(w + 1) * wlen_r],
                conf_r.offsets(),
                active_r,
            );
            for &m in maxsc_r[w].iter() {
                min_spec_shift = min_spec_shift.min(m);
            }
        }
    }

    // Energies (stored ints). Log energies derive from the stored values
    // after the re-shift below (see `stored_ld`).
    let mut e_l: Vec<Vec<i32>> = vec![vec![0; active_l]; n_win_l];
    let mut eld_l: Vec<Vec<f64>> = vec![vec![-1.0; active_l]; n_win_l];
    let mut e_r: Vec<Vec<i32>> = vec![vec![0; active_r]; n_win_r];
    let mut eld_r: Vec<Vec<f64>> = vec![vec![-1.0; active_r]; n_win_r];
    let mut max_nrg = 0i32;
    for w in 0..n_win_l {
        e_l[w] = energy::check_band_energy(
            &q_l[w * wlen_l..(w + 1) * wlen_l],
            &maxsc_l[w],
            conf_l.offsets(),
            active_l,
        );
        let m = energy::max_nrg_for_shift(&e_l[w], &maxsc_l[w], min_spec_shift);
        max_nrg = max_nrg.max(m);
    }
    if spec_r_opt.as_deref().is_some() {
        for w in 0..n_win_r {
            e_r[w] = energy::check_band_energy(
                &q_r[w * wlen_r..(w + 1) * wlen_r],
                &maxsc_r[w],
                conf_r.offsets(),
                active_r,
            );
            let m = energy::max_nrg_for_shift(&e_r[w], &maxsc_r[w], min_spec_shift);
            max_nrg = max_nrg.max(m);
        }
    }
    // All-zero shortcut is element-global (psy_main:732): blanks and
    // mdctScale 0 only when every channel is silent; otherwise the full
    // path runs (zero channels get natural zeros plus the Ld offset).
    let zero_elem = zero_l && (!stereo || zero_r == Some(true));
    let mut t_l: Vec<Vec<i32>> = vec![vec![0; active_l]; n_win_l];
    let mut t_r: Vec<Vec<i32>> = vec![vec![0; active_r]; n_win_r];
    let final_shift = if zero_elem {
        for w in 0..n_win_l {
            maxsc_l[w] = vec![0; active_l];
        }
        for w in 0..n_win_r {
            maxsc_r[w] = vec![0; active_r];
        }
        m_l = 0;
        m_r = 0;
        0
    } else {
        let (_, f) = energy::rescale_shifts(max_nrg, min_spec_shift, short_elem, m_l);
        // Apply: Ld from the RAW energies with C's exact guard/shift rules,
        // then stored E re-shift + thresholds init.
        for w in 0..n_win_l {
            eld_l[w] = energy::rescale_ld(&e_l[w], &maxsc_l[w], f);
            for b in 0..active_l {
                let mut scale = (maxsc_l[w][b] - 4).max(0);
                scale = ((scale - f) << 1).min(31);
                e_l[w][b] = shift_int(e_l[w][b], -scale);
                t_l[w][b] = init_threshold(e_l[w][b]);
            }
        }
        if stereo {
            for w in 0..n_win_r {
                eld_r[w] = energy::rescale_ld(&e_r[w], &maxsc_r[w], f);
                for b in 0..active_r {
                    let mut scale = (maxsc_r[w][b] - 4).max(0);
                    scale = ((scale - f) << 1).min(31);
                    e_r[w][b] = shift_int(e_r[w][b], -scale);
                    t_r[w][b] = init_threshold(e_r[w][b]);
                }
            }
        }
        // Representation shift (C shifts stored ints, tracked by
        // mdctScale; real values unchanged, so the float lane is untouched).
        if f != 0 {
            for v in q_l.iter_mut() {
                *v = shift_int(*v, f);
            }
            for w in 0..n_win_l {
                for m in maxsc_l[w].iter_mut() {
                    *m -= f;
                }
            }
            m_l -= f;
            if spec_r_opt.is_some() {
                for v in q_r.iter_mut() {
                    *v = shift_int(*v, f);
                }
                for w in 0..n_win_r {
                    for m in maxsc_r[w].iter_mut() {
                        *m -= f;
                    }
                }
                m_r -= f;
            }
        }
        f
    };

    // --- TNS block (skipped for LFE elements, mirroring isLFE[0]). ---
    let tns_on = tconf_l.active || tconf_r.active;
    let mut tns_active = [false; 8];
    let mut nrg_scaling = [0i32; 2];
    let mut tns_spec_shift = 0i32;
    if !lin.is_lfe && tns_on {
        // Detect per channel/window on the Q31 stored-domain spectrum
        // (exact integer merge; ratios feed float Schur).
        for w in 0..n_win_l {
            let base = w * wlen_l;
            tns::detect(
                &mut left.tns_state,
                &mut left.tns_info,
                tconf_l,
                &q_l[base..base + wlen_l],
                w,
                short_l,
                conf_l.sfb_count(),
            );
        }
        if stereo {
            let (rstate, rinfo) = right
                .as_deref_mut()
                .map(|r| (&mut r.tns_state, &mut r.tns_info))
                .expect("stereo state");
            let spec_r = spec_r_opt.as_deref().expect("stereo spectrum");
            let _ = spec_r;
            for w in 0..n_win_r {
                let base = w * wlen_r;
                tns::detect(
                    rstate,
                    rinfo,
                    tconf_r,
                    &q_r[base..base + wlen_r],
                    w,
                    short_r,
                    conf_r.sfb_count(),
                );
            }
            // Sync ch1 <- ch0 with ch1's config (psy_main:783-791).
            tns::synchronize_pair(
                rstate,
                rinfo,
                &left.tns_state,
                &left.tns_info,
                tconf_r,
                short_r,
                short_l,
            );
        }
        // Active flags OR over the element (psy_main:793-816).
        let nw = if short_l { 8 } else { 1 };
        for w in 0..nw {
            let l = left.tns_state.active[w];
            let r = right
                .as_deref()
                .map(|r| r.tns_state.active[w])
                .unwrap_or([false, false]);
            // Long windows read subblock 0; short read per-window.
            tns_active[w] = l[0] || l[1] || r[0] || r[1];
        }
        // Pre-encode scaling + encode + energy recompute, per channel.
        // (Two loops mirror C: encode/energy for all channels first so the
        // element-global tnsSpecShift is final before the overflow rescale.)
        // Left channel.
        tns_encode_channel(
            &mut left.tns_state,
            &mut left.tns_info,
            tconf_l,
            conf_l,
            spec_l,
            &mut q_l,
            &mut maxsc_l,
            &mut e_l,
            &mut eld_l,
            &mut t_l,
            &tns_active,
            short_l,
            active_l,
            &mut m_l,
            &mut nrg_scaling[0],
            &mut tns_spec_shift,
        );
        // Right channel.
        if stereo {
            let (rstate, rinfo) = right
                .as_deref_mut()
                .map(|r| (&mut r.tns_state, &mut r.tns_info))
                .expect("stereo state");
            let spec_r = spec_r_opt.as_deref_mut().expect("stereo spectrum");
            tns_encode_channel(
                rstate,
                rinfo,
                tconf_r,
                conf_r,
                spec_r,
                &mut q_r,
                &mut maxsc_r,
                &mut e_r,
                &mut eld_r,
                &mut t_r,
                &tns_active,
                short_r,
                active_r,
                &mut m_r,
                &mut nrg_scaling[1],
                &mut tns_spec_shift,
            );
        }
    } else {
        // TNS globally inactive: clear runtime decisions (psy_main:915-919).
        // tnsInfo keeps detect-written fields (none ran); info stays as-is.
        left.tns_state.clear_runtime();
        if let Some(r) = right.as_deref_mut() {
            r.tns_state.clear_runtime();
        }
    }
    // Overflow rescale AFTER all channels (element-global tnsSpecShift).
    if tns_spec_shift != 0 {
        overflow_rescale(
            spec_l,
            &mut q_l,
            &mut e_l,
            &mut eld_l,
            &mut t_l,
            &mut m_l,
            lowpass_l,
            active_l,
            tns_spec_shift,
            nrg_scaling[0],
        );
        if stereo {
            let spec_r = spec_r_opt.as_deref_mut().expect("stereo spectrum");
            overflow_rescale(
                spec_r,
                &mut q_r,
                &mut e_r,
                &mut eld_r,
                &mut t_r,
                &mut m_r,
                lowpass_r,
                active_r,
                tns_spec_shift,
                nrg_scaling[1],
            );
        }
    }

    // --- Threshold advance per channel (psy_main:922-1019 minus MS). ---
    let mut spread_l: Vec<Vec<i32>> = vec![vec![0; active_l]; n_win_l];
    advance_thresholds(
        &mut left.preecho,
        &mut t_l,
        &mut spread_l,
        &e_l,
        conf_l,
        active_l,
        m_l,
        short_l,
        lin.seq,
        lin.is_lfe,
    );
    let mut spread_r: Vec<Vec<i32>> = Vec::new();
    if stereo {
        spread_r = vec![vec![0; active_r]; n_win_r];
        let rstate = right.as_deref_mut().expect("stereo state");
        advance_thresholds(
            &mut rstate.preecho,
            &mut t_r,
            &mut spread_r,
            &e_r,
            conf_r,
            active_r,
            m_r,
            short_r,
            rseq,
            rlfe,
        );
    }

    // --- MS energies (stereo element-level, psy_main:1021-1037). ---
    let ms_out = if stereo {
        // Re-map Q with current exponents (post everything).
        let ql_full = energy::map_spectrum_to_q31(spec_l, m_l);
        let qr_full =
            energy::map_spectrum_to_q31(spec_r_opt.as_deref().expect("stereo spectrum"), m_r);
        let calc_ld = rseq != WindowSequence::Short;
        let mut mid = Vec::with_capacity(n_win_r);
        let mut side = Vec::with_capacity(n_win_r);
        for w in 0..n_win_r {
            let (mi, si, mil, sil) = energy::calc_ms_energies(
                &ql_full[w * wlen_r..(w + 1) * wlen_r],
                &qr_full[w * wlen_r..(w + 1) * wlen_r],
                &maxsc_l[w],
                &maxsc_r[w],
                conf_r.offsets(),
                active_l,
                calc_ld,
            );
            mid.push(mi);
            side.push(si);
            // C leaves msLd STALE unless calcLd: store on write, read the
            // persistent arrays for output (both short-stale and init-zero
            // mirror C exactly).
            if calc_ld {
                left.ms_ld[w][..active_l].copy_from_slice(&mil);
                let rstate = right.as_deref_mut().expect("stereo state");
                rstate.ms_ld[w][..active_l].copy_from_slice(&sil);
            }
        }
        let rstate = right.as_deref().expect("stereo state");
        let mut mid_ld = Vec::with_capacity(n_win_r);
        let mut side_ld = Vec::with_capacity(n_win_r);
        for w in 0..n_win_r {
            mid_ld.push(left.ms_ld[w][..active_l].to_vec());
            side_ld.push(rstate.ms_ld[w][..active_l].to_vec());
        }
        Some(MsOutput {
            mid,
            side,
            mid_ld,
            side_ld,
        })
    } else {
        None
    };

    // --- Grouping + outputs per channel. ---
    // MS vectors per channel for short-block regrouping (mid ch0/side ch1).
    // Mono MS buffers stay zero (never written in C either).
    let ms_zeros_l: Vec<Vec<i32>> = vec![vec![0; active_l]; n_win_l];
    let ms_zeros_r: Vec<Vec<i32>> = vec![vec![0; active_r]; n_win_r];
    let (ms_l, ms_r): (&[Vec<i32>], &[Vec<i32>]) = match &ms_out {
        Some(ms) => (&ms.mid, &ms.side),
        None => (&ms_zeros_l, &ms_zeros_r),
    };
    let mut left_out = finish_channel(
        &left.preecho,
        &t_l,
        &e_l,
        &eld_l,
        &spread_l,
        &maxsc_l,
        spec_l,
        conf_l,
        short_l,
        active_l,
        m_l,
        final_shift,
        tns_spec_shift,
        lin.grouping,
        ms_l,
        if short_l {
            &tables.min_snr_short
        } else {
            &tables.min_snr_long
        },
    );
    let mut right_out = if stereo {
        let rstate = right.as_deref_mut().expect("stereo state");
        Some(finish_channel(
            &rstate.preecho,
            &t_r,
            &e_r,
            &eld_r,
            &spread_r,
            &maxsc_r,
            spec_r_opt.as_deref().expect("stereo spectrum"),
            conf_r,
            short_r,
            active_r,
            m_r,
            final_shift,
            tns_spec_shift,
            rgroup,
            ms_r,
            if short_r {
                &tables.min_snr_short
            } else {
                &tables.min_snr_long
            },
        ))
    } else {
        None
    };
    left_out.nrg_scaling = nrg_scaling[0];
    if let Some(ref mut r) = right_out {
        r.nrg_scaling = nrg_scaling[1];
    }
    (left_out, right_out, ms_out)
}

/// TNS pre-encode scaling, encode, and energy recompute for one channel
/// (mirrors `psy_main.cpp:818-891`).
#[allow(clippy::too_many_arguments)]
fn tns_encode_channel(
    state: &mut TnsChannel,
    info: &mut TnsChannel,
    tconf: &TnsConfig,
    conf: &SfbConfig,
    spectrum: &mut [f64],
    q: &mut Vec<i32>,
    maxsc: &mut [Vec<i32>],
    e_stored: &mut [Vec<i32>],
    eld_true: &mut [Vec<f64>],
    thr: &mut [Vec<i32>],
    tns_active: &[bool; 8],
    short_ch: bool,
    active: usize,
    mdct_scale: &mut i32,
    nrg_scaling_out: &mut i32,
    tns_spec_shift_global: &mut i32,
) {
    use super::q31::shift_int;
    let n_win = if short_ch { 8 } else { 1 };
    let wlen = if short_ch { 128 } else { 1024 };
    // Pre-encode scaling for long blocks (psy_main:818-839): stored-int
    // halving with mdctScale tracking plus threshold quartering. The float
    // lane carries real values and is untouched (TNS ratios and the linear
    // filter are scale-invariant; post-filter Q remap uses the tracked
    // exponent, matching C's halved-domain values exactly).
    if tns_active[0] && !short_ch {
        for v in q[..conf.lowpass_line()].iter_mut() {
            *v = shift_int(*v, -1);
        }
        for t in thr[0][..active].iter_mut() {
            *t = shift_int(*t, -2);
        }
        *mdct_scale += 1;
    }
    for w in 0..n_win {
        let base = w * wlen;
        tns::encode(
            info,
            state.merged,
            tconf,
            &mut spectrum[base..base + wlen],
            w,
        );
        if tns_active[w] {
            // Recompute headroom on the filtered spectrum (psy_main:860-863).
            let qwin = energy::map_spectrum_to_q31(&spectrum[base..base + wlen], *mdct_scale);
            maxsc[w] = energy::max_scale_spec(&qwin, conf.offsets(), active);
            q[base..base + wlen].copy_from_slice(&qwin);
            if short_ch {
                e_stored[w] = energy::band_energy_short(&qwin, &maxsc[w], conf.offsets(), active);
                // Short Ld stays at rescale values (C quirk: Short path
                // takes no Ld array, mirrored).
            } else {
                let (e_new, k, ld_new) =
                    energy::band_energy_long(&qwin, &maxsc[w], conf.offsets(), active);
                e_stored[w] = e_new;
                eld_true[w] = ld_new;
                *nrg_scaling_out = k;
                *tns_spec_shift_global = (*tns_spec_shift_global).max(k);
            }
        }
    }
}

/// TNS overflow rescale for one channel (`psy_main.cpp:893-912`).
#[allow(clippy::too_many_arguments)]
fn overflow_rescale(
    spectrum: &mut [f64],
    q: &mut Vec<i32>,
    e_stored: &mut [Vec<i32>],
    eld: &mut [Vec<f64>],
    thr: &mut [Vec<i32>],
    mdct_scale: &mut i32,
    lowpass: usize,
    active: usize,
    tns_spec_shift: i32,
    nrg_scaling: i32,
) {
    use super::q31::shift_int;
    // Representation shift only (stored ints + mdctScale; real values and
    // the float lane are untouched). The `spectrum` slice is kept for
    // signature symmetry with the other stages (grouping reads it later).
    let _ = &mut spectrum[..lowpass];
    for v in q[..lowpass].iter_mut() {
        *v = shift_int(*v, -tns_spec_shift);
    }
    let scale = (tns_spec_shift - nrg_scaling) << 1;
    for b in 0..active {
        e_stored[0][b] = shift_int(e_stored[0][b], -scale);
        thr[0][b] = shift_int(thr[0][b], -(tns_spec_shift << 1));
        // Unconditional Ld correction (mirrors line 903, no zero guard).
        eld[0][b] -= scale as f64 / 64.0;
    }
    *mdct_scale += tns_spec_shift;
}

/// Threshold advance for one channel's windows (`psy_main.cpp:922-1019`
/// minus MS): per-window clip cap, spreading, PCM floor with the
/// accumulating shift, pre-echo with STOP-before/START-after resets, and
/// spread-energy copy plus spreading. `active` is the channel's active
/// band count (LFE-aware; differs from `conf.sfb_active()` for LFE).
#[allow(clippy::too_many_arguments)]
fn advance_thresholds(
    preecho: &mut PreEchoState,
    thr: &mut [Vec<i32>],
    spread: &mut [Vec<i32>],
    energies: &[Vec<i32>],
    conf: &SfbConfig,
    active: usize,
    mdct_scale: i32,
    short_window: bool,
    seq: WindowSequence,
    is_lfe: bool,
) {
    let (mask_hi, mask_lo) = conf.mask_factors_q31();
    let (spr_hi, spr_lo) = conf.mask_factors_spr_q31();
    let clip = clip_cap(conf.clip_energy_q31(), mdct_scale, short_window);
    let mut energy_shift = 2 * mdct_scale;
    for w in 0..thr.len() {
        for t in thr[w][..active].iter_mut() {
            *t = (*t).min(clip);
        }
        spreading_max(active, mask_lo, mask_hi, &mut thr[w]);
        apply_pcm_floor(&mut thr[w], conf.pcm_quant_q31(), &mut energy_shift);
        if !is_lfe {
            if seq == WindowSequence::Stop {
                preecho.reset_for_transition(active);
            }
            preecho.control(&mut thr[w], mdct_scale);
            preecho.set_enabled(true);
            if seq == WindowSequence::Start {
                preecho.reset_for_transition(active);
            }
        }
        spread[w][..active].copy_from_slice(&energies[w][..active]);
        spreading_max(active, spr_lo, spr_hi, &mut spread[w]);
    }
}

/// Group one short channel (`groupShortData`, `grp_data.cpp:118-264`).
/// `ms` holds this channel's regrouped-input MS vectors (mid for ch0, side
/// for ch1; zeros for mono). `min_snr` flows through per group (M4 input
/// until computed). Returns grouped (energies, thresholds, spread, ms,
/// minsnr, offsets, max_sfb, regrouped spectrum).
#[allow(clippy::too_many_arguments)]
fn group_short_channel(
    spectrum: &[f64],
    thr: &[Vec<i32>],
    en: &[Vec<i32>],
    ms: &[Vec<i32>],
    spread: &[Vec<i32>],
    min_snr: &[i32],
    conf: &SfbConfig,
    no_of_groups: usize,
    group_len: &[u8; 4],
) -> (
    Vec<i32>,
    Vec<i32>,
    Vec<i32>,
    Vec<i32>,
    Vec<i32>,
    Vec<i32>,
    usize,
    Vec<f64>,
) {
    let offsets: Vec<usize> = conf.offsets().iter().map(|&o| o as usize).collect();
    let active = conf.sfb_active();
    let sfb_cnt = conf.sfb_count();
    // maxSfbPerGroup from top non-zero band across windows.
    let mut highest = 0usize;
    for wnd in 0..8 {
        let mut s = active as i32;
        while s > highest as i32 {
            s -= 1;
            let su = s as usize;
            let mut found = false;
            for line in (offsets[su]..offsets[su + 1]).rev() {
                if spectrum[wnd * 128 + line] != 0.0 {
                    found = true;
                    break;
                }
            }
            if found {
                highest = su;
                break;
            }
        }
    }
    let max_sfb = highest + 1;
    let no_sfb = no_of_groups * sfb_cnt;
    // Grouped offsets.
    let mut grouped_off = vec![0i32; no_sfb + 1];
    {
        let mut i = 0;
        let mut offset = 0i32;
        for grp in 0..no_of_groups {
            let gl = group_len[grp] as usize;
            for sfb in 0..active + 1 {
                grouped_off[i] = offset + offsets[sfb] as i32 * gl as i32;
                i += 1;
            }
            i += sfb_cnt - (active + 1);
            offset += gl as i32 * 128;
        }
        grouped_off[i] = 1024;
    }
    // Generic saturating regroup over int arrays.
    let regroup = |src: &[Vec<i32>]| {
        let mut out = vec![0i32; no_sfb];
        let mut wnd = 0usize;
        let mut i = 0usize;
        for grp in 0..no_of_groups {
            let gl = group_len[grp] as usize;
            for sfb in 0..active {
                let mut acc = src[wnd][sfb];
                for j in 1..gl {
                    acc = nrg_add_saturate(acc, src[wnd + j][sfb]);
                }
                out[i] = acc;
                i += 1;
            }
            i += sfb_cnt - active;
            wnd += gl;
        }
        out
    };
    let grouped_min_snr = {
        let mut out = vec![0i32; no_sfb];
        let mut i = 0usize;
        for grp in 0..no_of_groups {
            for sfb in 0..active {
                out[i] = min_snr[sfb];
                i += 1;
            }
            i += sfb_cnt - active;
            let _ = grp;
        }
        out
    };
    // Regrouped spectrum interleave.
    let mut grouped_spec = vec![0.0f64; 1024];
    {
        let mut wnd = 0usize;
        let mut i = 0usize;
        for grp in 0..no_of_groups {
            let gl = group_len[grp] as usize;
            for sfb in 0..active {
                let width = offsets[sfb + 1] - offsets[sfb];
                for j in 0..gl {
                    let base = (wnd + j) * 128 + offsets[sfb];
                    for k in 0..width {
                        grouped_spec[i] = spectrum[base + k];
                        i += 1;
                    }
                }
            }
            i += gl * (offsets[sfb_cnt] - offsets[active]);
            wnd += gl;
        }
    }
    (
        regroup(en),
        regroup(thr),
        regroup(spread),
        regroup(ms),
        grouped_min_snr,
        grouped_off,
        max_sfb,
        grouped_spec,
    )
}

/// Assemble one channel's output: grouping (short) or long outputs with
/// log-domain vectors. `ms` carries this channel's MS vectors (mid for
/// ch0, side for ch1; zeros for mono) for short-block regrouping.
#[allow(clippy::too_many_arguments)]
fn finish_channel(
    preecho: &PreEchoState,
    thr: &[Vec<i32>],
    en: &[Vec<i32>],
    eld_true: &[Vec<f64>],
    spread: &[Vec<i32>],
    maxsc: &[Vec<i32>],
    spectrum: &[f64],
    conf: &SfbConfig,
    short_ch: bool,
    active: usize,
    mdct_scale: i32,
    final_shift: i32,
    tns_shift: i32,
    grouping: (u8, [u8; 4]),
    ms: &[Vec<i32>],
    min_snr_in: &[i32],
) -> ChannelOutput {
    let mut out = ChannelOutput {
        mdct_scale,
        final_shift,
        tns_shift,
        maxscale: maxsc.to_vec(),
        energies: en.to_vec(),
        energy_ld: eld_true.to_vec(),
        thresholds: thr.to_vec(),
        spread: spread.to_vec(),
        nm1: preecho.prev()[..active].to_vec(),
        m_nm1: preecho.prev_scale(),
        calc_pre_echo: i32::from(preecho.is_enabled()),
        max_sfb_per_group: 0,
        ..Default::default()
    };
    if short_ch {
        let (no_groups, lens) = grouping;
        let (ge, gt, gs, gms, gminsnr, goff, max_sfb, gspec) = group_short_channel(
            spectrum,
            thr,
            en,
            ms,
            spread,
            min_snr_in,
            conf,
            no_groups as usize,
            &lens,
        );
        let _ = gms;
        let grouped = GroupedOutput {
            energies: ge.clone(),
            thresholds: gt.clone(),
            spread: gs.clone(),
            energy_ld: grouped_ld(&ge),
            threshold_ld: grouped_ld_clamped(&gt),
            offsets: goff,
            min_snr: gminsnr,
            spectrum: gspec,
            max_sfb_per_group: max_sfb,
        };
        out.grouped = Some(grouped);
        out.max_sfb_per_group = max_sfb;
    } else {
        // Long maxSfbPerGroup from top non-zero line (psy_main:1092-1101).
        let offsets: Vec<usize> = conf.offsets().iter().map(|&o| o as usize).collect();
        let mut max_sfb = 0usize;
        for sfb in (0..active).rev() {
            let mut found = false;
            for line in (offsets[sfb]..offsets[sfb + 1]).rev() {
                if spectrum[line] != 0.0 {
                    found = true;
                    break;
                }
            }
            if found {
                max_sfb = sfb + 1;
                break;
            }
        }
        // floor at min(5, active)... C: max(fixMin(5,active), found).
        max_sfb = max_sfb.max(5.min(active));
        out.max_sfb_per_group = max_sfb;
        out.threshold_ld = Some(grouped_ld_clamped(&thr[0]));
    }
    out
}

/// Log-domain of stored ints with the grouped-output floor rule.
/// NOTE: grouped/longout Ld in C comes from `LdDataVector` (fLog2 Taylor);
/// the float `log2/64` used here differs by ~1e-9, covered by tolerance.
fn grouped_ld(stored: &[i32]) -> Vec<f64> {
    stored
        .iter()
        .map(|&v| {
            if v <= 0 {
                super::sfb::ld_data(0.0)
            } else {
                (v as f64 / 2_147_483_648.0).log2() / 64.0
            }
        })
        .collect()
}

/// Threshold Ld with the `-33/64` floor (`psy_main.cpp:1071-1075`).
fn grouped_ld_clamped(stored: &[i32]) -> Vec<f64> {
    grouped_ld(stored)
        .into_iter()
        .map(|v| v.max(super::sfb::THRESHOLD_LD_FLOOR))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::block_switch::BlockSwitch;
    use crate::aac_enc::testdata;
    use crate::aac_enc::transform::ChannelTransform;

    /// Integer stored-domain comparison: absolute floor plus relative
    /// margin for float-spectrum rounding through the integer sums.
    /// Floor 32 covers the worst observed small-band rounding
    /// (quiet-sine f5 band 11: dE=21 on E=775 from +-1 LSB Q-mapping
    /// rounding on ~10-LSB lines); the two terms sum as independent
    /// sources (rounding-sum floor + proportional M2/mult error).
    fn assert_q31_close(got: &[i32], want: &[i32], what: &str) {
        assert_eq!(got.len(), want.len(), "{what}: length");
        for (i, (&g, &w)) in got.iter().zip(want.iter()).enumerate() {
            let d = (g as i64 - w as i64).abs();
            let tol = 32 + ((w as f64).abs() / 500.0) as i64;
            assert!(d <= tol, "{what}[{i}]: got {g} want {w} (tol {tol})");
        }
    }

    /// Absolute comparison for log-domain vectors (Q31 ints as f64).
    /// Tolerance propagates the stored-energy rounding bound through
    /// log2/64 (`dLd = dE/(E*ln2*64)`) plus margin for C's `fLog2` Taylor
    /// error (~1e-9): tight for large energies, honest for tiny ones where
    /// a few LSBs of Q-mapping rounding dominate.
    /// Log-domain comparison via energy-domain inversion. Rationale: Ld
    /// has genuine discontinuities (the `-1.0` sentinel at E=0: E=0 vs E=1
    /// differ by 0.5 in Ld; the long-recompute guard clamp), so no
    /// absolute Ld tolerance is both tight and sound. Inverting both
    /// sides to implied energies (`sentinel -> 0`, else `2^(Ld*64+31)`)
    /// and applying the stored-energy bound (`16 + E/500`) accepts exactly
    /// the flips that rounding-level E agreement forces, and fails loudly
    /// on real formula divergences (which invert to huge E gaps).
    /// Sentinel/clamp behavior itself is gated by the energy tests plus
    /// the discrete TNS/threshold comparisons.
    fn ld_to_e(ld: f64) -> f64 {
        if ld <= -1.0 {
            0.0
        } else {
            2f64.powf(ld * 64.0 + 31.0)
        }
    }

    fn assert_ld_close(got: &[f64], want: &[i32], what: &str) {
        assert_eq!(got.len(), want.len(), "{what}: length");
        for (i, (&g, &w)) in got.iter().zip(want.iter()).enumerate() {
            let e_got = ld_to_e(g);
            let e_want = ld_to_e(w as f64 / 2_147_483_648.0);
            // Same energy bound as assert_q31_close (floor + relative).
            let tol = 32.0 + e_want / 500.0;
            assert!(
                (e_got - e_want).abs() <= tol,
                "{what}[{i}]: Ld got {g} want {} (E {e_got} vs {e_want}, tol {tol})",
                w as f64 / 2_147_483_648.0
            );
        }
    }

    /// `assert_ld_close` over a gathered subset: `got[k]` corresponds to
    /// flat index `idx[k]` in `want` (pad masking for grouped vectors).
    fn assert_ld_indexed(got: &[f64], want: &[i32], idx: &[usize], what: &str) {
        assert_eq!(got.len(), idx.len(), "{what}: gathered length");
        for (k, &fi) in idx.iter().enumerate() {
            let w = want[fi];
            let e_got = ld_to_e(got[k]);
            let e_want = ld_to_e(w as f64 / 2_147_483_648.0);
            // Same energy bound as assert_q31_close (floor 32).
            let tol = 32.0 + e_want / 500.0;
            assert!(
                (e_got - e_want).abs() <= tol,
                "{what}[{fi}]: Ld got {} want {} (E {e_got} vs {e_want}, tol {tol})",
                got[k],
                w as f64 / 2_147_483_648.0
            );
        }
    }

    struct Stream {
        switches: Vec<BlockSwitch>,
        transforms: Vec<ChannelTransform>,
        psy: Vec<PsyChannel>,
        fixture: Vec<Vec<i16>>,
        tables: PsyTables,
        channels: usize,
    }

    impl Stream {
        fn new(name: &str) -> Self {
            let fixture = testdata::fixture_samples(name);
            let channels = fixture.len();
            let sr = testdata::fixture_sample_rate(name);
            let (ch_br, tns_br) = if channels == 1 {
                (112000, 112000)
            } else {
                (96000, 192000)
            };
            let ((lcfg, _), (scfg, _)) = testdata::load_cfg(sr, ch_br);
            let tables =
                PsyTables::for_rate(sr, ch_br, tns_br, 19293, lcfg.min_snr_ld, scfg.min_snr_ld)
                    .expect("tables build");
            let switches = (0..channels).map(|_| BlockSwitch::new()).collect();
            let transforms = (0..channels).map(|_| ChannelTransform::new()).collect();
            let psy = (0..channels)
                .map(|_| PsyChannel::new(tables.long_cfg.pcm_quant_q31()))
                .collect();
            Self {
                switches,
                transforms,
                psy,
                fixture,
                tables,
                channels,
            }
        }

        fn fresh_at(&self, frame: u32) -> Vec<[i16; 1024]> {
            (0..self.channels)
                .map(|c| {
                    let mut block = [0i16; 1024];
                    for (i, v) in block.iter_mut().enumerate() {
                        let t = frame as usize * 1024 + i;
                        *v = if t < self.fixture[c].len() {
                            self.fixture[c][t]
                        } else {
                            0
                        };
                    }
                    block
                })
                .collect()
        }

        /// Run M2 frontend + M3 psy for frame `frame` (call frames in
        /// order; state evolves across calls).
        fn step(
            &mut self,
            frame: u32,
        ) -> (
            Vec<WindowSequence>,
            Vec<(u8, [u8; 4])>,
            (ChannelOutput, Option<ChannelOutput>, Option<MsOutput>),
        ) {
            let fresh = self.fresh_at(frame);
            for (sw, fr) in self.switches.iter_mut().zip(fresh.iter()) {
                sw.process(fr, false);
            }
            if self.channels == 2 {
                let (l, r) = self.switches.split_at_mut(1);
                BlockSwitch::synchronize_pair(&mut l[0], &mut r[0]);
            } else {
                self.switches[0].synchronize_mono();
            }
            let mut specs: Vec<[f64; 1024]> = Vec::with_capacity(self.channels);
            let mut seqs = Vec::with_capacity(self.channels);
            let mut groupings = Vec::with_capacity(self.channels);
            for (ch, (tr, sw)) in self
                .transforms
                .iter_mut()
                .zip(self.switches.iter())
                .enumerate()
            {
                let out = tr
                    .process(&fresh[ch], sw.sequence(), sw.shape())
                    .expect("mdct");
                let mut spec = [0.0f64; 1024];
                for (d, s) in spec.iter_mut().zip(out.iter()) {
                    *d = *s as f64;
                }
                specs.push(spec);
                seqs.push(sw.sequence());
                groupings.push(sw.grouping());
            }
            let (head, tail) = specs.split_at_mut(1);
            let lin = ChannelInput {
                spectrum: &mut head[0],
                seq: seqs[0],
                grouping: groupings[0],
                is_lfe: false,
            };
            if self.channels == 2 {
                let rin = ChannelInput {
                    spectrum: &mut tail[0],
                    seq: seqs[1],
                    grouping: groupings[1],
                    is_lfe: false,
                };
                let (pl, pr) = self.psy.split_at_mut(1);
                let out = process_pair(&mut pl[0], Some(&mut pr[0]), lin, Some(rin), &self.tables);
                (seqs, groupings, out)
            } else {
                let out = process_pair(&mut self.psy[0], None, lin, None, &self.tables);
                (seqs, groupings, out)
            }
        }
    }

    fn compare_channel(
        name: &str,
        frame: u32,
        ch: usize,
        out: &ChannelOutput,
        want: &testdata::PsyFrame,
        short: bool,
        grouping: (u8, [u8; 4]),
    ) {
        let tag = format!("{name} f{frame} ch{ch}");
        assert_eq!(out.mdct_scale, want.mdct_scale, "{tag} mdct_scale");
        assert_eq!(out.final_shift, want.final_shift, "{tag} final_shift");
        assert_eq!(out.tns_shift, want.tns_shift, "{tag} tns_shift");
        assert_eq!(out.nrg_scaling, want.nrg_scaling, "{tag} nrg_scaling");
        assert_eq!(
            out.maxscale.len(),
            if short { 8 } else { 1 },
            "{tag} maxscale windows"
        );
        // Only window 0's maxScale is committed; per-window energies gate
        // the rest (a wrong maxScale shifts energies by 4x+, far outside
        // tolerance). maxScaleSpec is pure integer analysis: exactness is
        // expected, except for the KNOWN_MAXSCALE clz-boundary flips below
        // (spec §11.2), which pin recorded Rust values at flipped bands and
        // committed values everywhere — either side changing fails loudly.
        if let Some(e) = find_maxscale_entry(name, frame, ch) {
            assert_eq!(
                out.maxscale[0].len(),
                want.maxscale.len(),
                "{tag} maxscale len"
            );
            for (b, (&g, &w)) in out.maxscale[0].iter().zip(want.maxscale.iter()).enumerate() {
                match e.flipped.iter().find(|&&(i, _, _)| i == b) {
                    Some(&(_, gv, wv)) => {
                        assert_eq!(
                            g, gv,
                            "{tag} maxscale[{b}] recorded divergence ({})",
                            e.cause
                        );
                        assert_eq!(w, wv, "{tag} maxscale[{b}] reference changed ({})", e.cause);
                    }
                    None => assert_eq!(g, w, "{tag} maxscale[{b}]"),
                }
            }
        } else {
            assert_eq!(&out.maxscale[0], &want.maxscale, "{tag} maxscale w0");
        }
        assert_eq!(out.energies.len(), want.nrg.len(), "{tag} energy windows");
        for (w, (got, want_w)) in out.energies.iter().zip(want.nrg.iter()).enumerate() {
            assert_q31_close(got, want_w, &format!("{tag} energy w{w}"));
        }
        for (w, (got, want_w)) in out.energy_ld.iter().zip(want.nrg_ld.iter()).enumerate() {
            assert_ld_close(got, want_w, &format!("{tag} energy_ld w{w}"));
        }
        for (w, (got, want_w)) in out.thresholds.iter().zip(want.thr.iter()).enumerate() {
            assert_q31_close(got, want_w, &format!("{tag} threshold w{w}"));
        }
        for (w, (got, want_w)) in out.spread.iter().zip(want.spread.iter()).enumerate() {
            assert_q31_close(got, want_w, &format!("{tag} spread w{w}"));
        }
        assert_q31_close(&out.nm1, &want.nm1, &format!("{tag} nm1"));
        assert_eq!(out.m_nm1, want.m_nm1, "{tag} m_nm1");
        assert_eq!(out.calc_pre_echo, want.calc_pre_echo, "{tag} calc_pre_echo");
        if short {
            let g = out.grouped.as_ref().expect("grouped output");
            let wg = want.grouped.as_ref().expect("grouped vectors");
            assert_eq!(g.max_sfb_per_group, wg.max_sfb, "{tag} max_sfb");
            // Pad entries (flat indices past `active` in each
            // group-stride of `sfb_cnt`) are uninitialized C union memory
            // (`grp_data.cpp:192,207`: `i += sfbCnt - sfb` skips without
            // writing) and never consumed downstream (reads stop at
            // maxSfbPerGroup). Ours are 0; mask pads out of every
            // per-group comparison. Verified: f24 GT[13]=675 is f23's
            // stale Long[13], GE[55]=16 predates f23.
            let no_groups = grouping.0 as usize;
            let active = out.energies[0].len();
            let sfb_cnt = g.energies.len() / no_groups;
            assert_eq!(wg.e.len(), no_groups * sfb_cnt, "{tag} grouped span");
            let mut meaningful: Vec<usize> = Vec::new();
            for grp in 0..no_groups {
                for b in 0..active {
                    meaningful.push(grp * sfb_cnt + b);
                }
            }
            let gather = |v: &[i32]| meaningful.iter().map(|&i| v[i]).collect::<Vec<_>>();
            assert_q31_close(
                &gather(&g.energies),
                &gather(&wg.e),
                &format!("{tag} grouped E"),
            );
            assert_q31_close(
                &gather(&g.thresholds),
                &gather(&wg.t),
                &format!("{tag} grouped T"),
            );
            assert_q31_close(
                &gather(&g.spread),
                &gather(&wg.spread),
                &format!("{tag} grouped S"),
            );
            let gather_f = |v: &[f64]| meaningful.iter().map(|&i| v[i]).collect::<Vec<_>>();
            assert_ld_indexed(
                &gather_f(&g.energy_ld),
                &wg.e_ld,
                &meaningful,
                &format!("{tag} grouped ELd"),
            );
            assert_ld_indexed(
                &gather_f(&g.threshold_ld),
                &wg.t_ld,
                &meaningful,
                &format!("{tag} grouped TLd"),
            );
            assert_eq!(g.offsets, wg.offsets, "{tag} grouped offsets");
            assert_eq!(
                gather(&g.min_snr),
                gather(&wg.min_snr),
                "{tag} grouped minsnr"
            );
            // Regrouped spectrum: our floats track real values while the
            // oracle dumps stored ints; convert with the final block
            // exponent (peak-relative like the M2 MDCT gate). Skip regions
            // past the active bands are scratch garbage in C (same pad
            // story as above); compare per-group meaningful prefixes from
            // the pad-free grouped offsets.
            assert_eq!(g.spectrum.len(), wg.spectrum.len());
            let to_real = 2f64.powi(out.mdct_scale - 31);
            let (no_groups, lens) = grouping;
            let per_group = active + 1;
            let mut base = 0usize;
            let mut checked = 0usize;
            let mut peak = 1.0f64;
            for grp in 0..no_groups as usize {
                let gl = lens[grp] as usize;
                let span =
                    (g.offsets[grp * per_group + active] - g.offsets[grp * per_group]) as usize;
                for (k, (&got, &w)) in g.spectrum[base..base + span]
                    .iter()
                    .zip(wg.spectrum[base..base + span].iter())
                    .enumerate()
                {
                    let want_r = w as f64 * to_real;
                    peak = peak.max(want_r.abs());
                    let _ = k;
                    assert!(
                        (got - want_r).abs() <= 1e-3 * peak.max(1.0),
                        "{tag} gspec g{grp}[{k}]: got {got} want {want_r} (q31 {w})"
                    );
                    checked += 1;
                }
                base += gl * 128;
                let _ = gl;
            }
            assert!(checked > 0, "{tag} gspec empty");
        } else {
            let tld = out.threshold_ld.as_ref().expect("long threshold Ld");
            assert_ld_close(
                tld,
                &want.threshold_ld.as_ref().expect("TLD"),
                &format!("{tag} TLD"),
            );
            assert_eq!(
                out.max_sfb_per_group,
                want.long_max_sfb.expect("long maxSfb"),
                "{tag} max_sfb"
            );
        }
    }

    /// Known input-driven divergences (spec §11-11.3). Generated from a
    /// got-side harvest (temporary test, since removed) cross-checked
    /// against committed vectors at generation time; both sides are
    /// re-checked at runtime below, so Rust behavior changes AND reference
    /// edits fail loudly. Regenerate by re-adding the harvest walk if M2
    /// changes. Entries exist ONLY for windows/bands proven divergent;
    /// everything else takes the strict path (a new divergence class —
    /// e.g. nf/len/dir/coefLO/active flips, which never occur in corpus —
    /// fails strict loudly instead of landing here silently).

    struct KnownMaxscaleDivergence {
        fixture: &'static str,
        frame: u32,
        ch: usize,
        cause: &'static str,
        /// (band, recorded Rust maxscale, committed maxscale).
        flipped: &'static [(usize, i32, i32)],
    }

    const KNOWN_MAXSCALE: &[KnownMaxscaleDivergence] = &[
        KnownMaxscaleDivergence {
            fixture: "click-mono",
            frame: 11,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (17, 18, 17),
                (20, 18, 19),
                (25, 20, 19),
                (27, 20, 19),
                (35, 21, 19),
                (37, 20, 19),
                (41, 17, 18),
                (43, 21, 20),
                (44, 18, 17),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "multitone-stereo",
            frame: 5,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[(34, 16, 17)],
        },
        KnownMaxscaleDivergence {
            fixture: "multitone-stereo",
            frame: 5,
            ch: 1,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[(34, 16, 17)],
        },
        KnownMaxscaleDivergence {
            fixture: "onset-stereo",
            frame: 25,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[(36, 18, 19), (37, 21, 20), (39, 20, 19), (43, 20, 19)],
        },
        KnownMaxscaleDivergence {
            fixture: "onset-stereo",
            frame: 25,
            ch: 1,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[(36, 18, 19), (37, 21, 20), (39, 20, 19), (43, 20, 19)],
        },
        KnownMaxscaleDivergence {
            fixture: "onset-stereo",
            frame: 26,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (2, 17, 16),
                (22, 19, 18),
                (26, 20, 19),
                (27, 20, 19),
                (28, 17, 16),
                (29, 20, 19),
                (32, 20, 19),
                (33, 18, 17),
                (34, 20, 19),
                (35, 19, 20),
                (36, 18, 17),
                (37, 20, 19),
                (42, 21, 19),
                (43, 20, 19),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "onset-stereo",
            frame: 26,
            ch: 1,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (2, 17, 16),
                (22, 19, 18),
                (26, 20, 19),
                (27, 20, 19),
                (28, 17, 16),
                (29, 20, 19),
                (32, 20, 19),
                (33, 18, 17),
                (34, 20, 19),
                (35, 19, 20),
                (36, 18, 17),
                (37, 20, 19),
                (42, 21, 19),
                (43, 20, 19),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "quiet-sine-stereo",
            frame: 5,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[(40, 14, 13)],
        },
        KnownMaxscaleDivergence {
            fixture: "quiet-sine-stereo",
            frame: 5,
            ch: 1,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[(40, 14, 13)],
        },
        KnownMaxscaleDivergence {
            fixture: "sine-stereo",
            frame: 5,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (0, 18, 17),
                (2, 17, 16),
                (15, 16, 17),
                (22, 19, 18),
                (24, 18, 17),
                (26, 20, 19),
                (27, 20, 19),
                (28, 17, 16),
                (31, 18, 19),
                (32, 21, 19),
                (33, 18, 17),
                (34, 21, 20),
                (36, 18, 17),
                (37, 21, 20),
                (39, 20, 19),
                (40, 21, 20),
                (42, 21, 19),
                (43, 21, 19),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "sine-stereo",
            frame: 5,
            ch: 1,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (0, 18, 17),
                (2, 17, 16),
                (15, 16, 17),
                (22, 19, 18),
                (24, 18, 17),
                (26, 20, 19),
                (27, 20, 19),
                (28, 17, 16),
                (31, 18, 19),
                (32, 21, 19),
                (33, 18, 17),
                (34, 21, 20),
                (36, 18, 17),
                (37, 21, 20),
                (39, 20, 19),
                (40, 21, 20),
                (42, 21, 19),
                (43, 21, 19),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "sine-stereo",
            frame: 6,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (2, 17, 16),
                (20, 19, 18),
                (21, 19, 18),
                (25, 20, 19),
                (26, 20, 19),
                (27, 20, 19),
                (29, 21, 20),
                (34, 21, 20),
                (35, 21, 19),
                (37, 21, 20),
                (38, 19, 20),
                (40, 21, 20),
                (42, 21, 20),
                (43, 21, 20),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "sine-stereo",
            frame: 6,
            ch: 1,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (2, 17, 16),
                (20, 19, 18),
                (21, 19, 18),
                (25, 20, 19),
                (26, 20, 19),
                (27, 20, 19),
                (29, 21, 20),
                (34, 21, 20),
                (35, 21, 19),
                (37, 21, 20),
                (38, 19, 20),
                (40, 21, 20),
                (42, 21, 20),
                (43, 21, 20),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "sine-stereo-44100",
            frame: 5,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (5, 16, 15),
                (18, 17, 18),
                (23, 20, 19),
                (26, 18, 19),
                (33, 17, 18),
                (36, 18, 17),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "sine-stereo-44100",
            frame: 5,
            ch: 1,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (5, 16, 15),
                (18, 17, 18),
                (23, 20, 19),
                (26, 18, 19),
                (33, 17, 18),
                (36, 18, 17),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "stepdown-stereo",
            frame: 2,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (25, 18, 19),
                (37, 21, 20),
                (41, 18, 19),
                (42, 20, 19),
                (43, 20, 19),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "stepdown-stereo",
            frame: 2,
            ch: 1,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (25, 18, 19),
                (37, 21, 20),
                (41, 18, 19),
                (42, 20, 19),
                (43, 20, 19),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "stepdown-stereo",
            frame: 5,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (0, 18, 17),
                (2, 17, 16),
                (15, 16, 17),
                (22, 19, 18),
                (24, 18, 17),
                (26, 20, 19),
                (27, 20, 19),
                (28, 17, 16),
                (31, 18, 19),
                (32, 21, 19),
                (33, 18, 17),
                (34, 21, 20),
                (36, 18, 17),
                (37, 21, 20),
                (39, 20, 19),
                (40, 21, 20),
                (42, 21, 19),
                (43, 21, 19),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "stepdown-stereo",
            frame: 5,
            ch: 1,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (0, 18, 17),
                (2, 17, 16),
                (15, 16, 17),
                (22, 19, 18),
                (24, 18, 17),
                (26, 20, 19),
                (27, 20, 19),
                (28, 17, 16),
                (31, 18, 19),
                (32, 21, 19),
                (33, 18, 17),
                (34, 21, 20),
                (36, 18, 17),
                (37, 21, 20),
                (39, 20, 19),
                (40, 21, 20),
                (42, 21, 19),
                (43, 21, 19),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "sweep-stereo",
            frame: 10,
            ch: 0,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (13, 17, 16),
                (16, 15, 16),
                (27, 17, 18),
                (28, 18, 17),
                (29, 18, 17),
                (32, 18, 17),
                (34, 18, 17),
                (44, 17, 18),
            ],
        },
        KnownMaxscaleDivergence {
            fixture: "sweep-stereo",
            frame: 10,
            ch: 1,
            cause: "input-driven clz-boundary peak flips, energetically neutral (spec §11.2)",
            flipped: &[
                (13, 17, 16),
                (16, 15, 16),
                (27, 17, 18),
                (28, 18, 17),
                (29, 18, 17),
                (32, 18, 17),
                (34, 18, 17),
                (44, 17, 18),
            ],
        },
    ];

    struct KnownTnsDivergence {
        fixture: &'static str,
        frame: u32,
        ch: usize,
        window: usize,
        cause: &'static str,
        want_ord: [i32; 2],
        want_coef_hi: [i32; 12],
        want_gain: [i32; 2],
        want_active: [bool; 2],
        got_ord: [usize; 2],
        got_coef_hi: [i32; 12],
        got_gain: [i32; 2],
    }

    const KNOWN_TNS: &[KnownTnsDivergence] = &[
        KnownTnsDivergence {
            fixture: "multitone-stereo",
            frame: 1,
            ch: 0,
            window: 6,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [3, 0],
            want_coef_hi: [0, 2, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1752, 1000],
            want_active: [true, false],
            got_ord: [3, 0],
            got_coef_hi: [0, 2, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1773, 1000],
        },
        KnownTnsDivergence {
            fixture: "multitone-stereo",
            frame: 1,
            ch: 1,
            window: 6,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [3, 0],
            want_coef_hi: [0, 2, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1752, 1000],
            want_active: [true, false],
            got_ord: [3, 0],
            got_coef_hi: [0, 2, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1773, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo",
            frame: 24,
            ch: 0,
            window: 4,
            cause: "input-driven parcor border flip, filter active both sides (spec §11)",
            want_ord: [2, 0],
            want_coef_hi: [3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [10227, 1000],
            want_active: [true, false],
            got_ord: [5, 0],
            got_coef_hi: [3, 1, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [10317, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo",
            frame: 24,
            ch: 0,
            window: 6,
            cause: "input-driven parcor border flip, filter active both sides (spec §11)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3866, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3910, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo",
            frame: 24,
            ch: 0,
            window: 7,
            cause: "input-driven parcor border flip, filter active both sides (spec §11)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3609, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3678, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo",
            frame: 24,
            ch: 1,
            window: 4,
            cause: "input-driven parcor border flip, filter active both sides (spec §11)",
            want_ord: [2, 0],
            want_coef_hi: [3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [10227, 1000],
            want_active: [true, false],
            got_ord: [5, 0],
            got_coef_hi: [3, 1, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [10317, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo",
            frame: 24,
            ch: 1,
            window: 6,
            cause: "input-driven parcor border flip, filter active both sides (spec §11)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3866, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3910, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo",
            frame: 24,
            ch: 1,
            window: 7,
            cause: "input-driven parcor border flip, filter active both sides (spec §11)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3609, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3678, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo",
            frame: 25,
            ch: 0,
            window: 0,
            cause: "input-driven parcor border flips, filter active both sides (spec §11.1)",
            want_ord: [11, 4],
            want_coef_hi: [6, 1, -2, 2, -1, 1, 0, -1, 1, -1, 1, 0],
            want_gain: [22154, 1157],
            want_active: [true, false],
            got_ord: [10, 4],
            got_coef_hi: [6, 2, -3, 2, -1, 0, 1, -1, 1, -1, 0, 0],
            got_gain: [27723, 1157],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo",
            frame: 25,
            ch: 1,
            window: 0,
            cause: "input-driven parcor border flips, filter active both sides (spec §11.1)",
            want_ord: [11, 4],
            want_coef_hi: [6, 1, -2, 2, -1, 1, 0, -1, 1, -1, 1, 0],
            want_gain: [22154, 1157],
            want_active: [true, false],
            got_ord: [10, 4],
            got_coef_hi: [6, 2, -3, 2, -1, 0, 1, -1, 1, -1, 0, 0],
            got_gain: [27723, 1157],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo",
            frame: 26,
            ch: 0,
            window: 0,
            cause: "input-driven knife-edge, filter inactive both sides (spec §11.1)",
            want_ord: [0, 0],
            want_coef_hi: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1018, 1000],
            want_active: [false, false],
            got_ord: [2, 0],
            got_coef_hi: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1047, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo",
            frame: 26,
            ch: 1,
            window: 0,
            cause: "input-driven knife-edge, filter inactive both sides (spec §11.1)",
            want_ord: [0, 0],
            want_coef_hi: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1018, 1000],
            want_active: [false, false],
            got_ord: [2, 0],
            got_coef_hi: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1047, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo-44100",
            frame: 22,
            ch: 0,
            window: 6,
            cause: "input-driven gain shift, same filter (spec §11.3)",
            want_ord: [5, 0],
            want_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [4637, 1000],
            want_active: [true, false],
            got_ord: [5, 0],
            got_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [4708, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo-44100",
            frame: 22,
            ch: 0,
            window: 7,
            cause: "input-driven gain shift, same filter (spec §11.3)",
            want_ord: [5, 0],
            want_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [4885, 1000],
            want_active: [true, false],
            got_ord: [5, 0],
            got_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [5072, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo-44100",
            frame: 22,
            ch: 1,
            window: 6,
            cause: "input-driven gain shift, same filter (spec §11.3)",
            want_ord: [5, 0],
            want_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [4637, 1000],
            want_active: [true, false],
            got_ord: [5, 0],
            got_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [4708, 1000],
        },
        KnownTnsDivergence {
            fixture: "onset-stereo-44100",
            frame: 22,
            ch: 1,
            window: 7,
            cause: "input-driven gain shift, same filter (spec §11.3)",
            want_ord: [5, 0],
            want_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [4885, 1000],
            want_active: [true, false],
            got_ord: [5, 0],
            got_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [5072, 1000],
        },
        KnownTnsDivergence {
            fixture: "sine-stereo",
            frame: 5,
            ch: 0,
            window: 0,
            cause: "input-driven knife-edge, filter inactive both sides (spec §11)",
            want_ord: [0, 0],
            want_coef_hi: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1018, 1000],
            want_active: [false, false],
            got_ord: [2, 0],
            got_coef_hi: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1049, 1000],
        },
        KnownTnsDivergence {
            fixture: "sine-stereo",
            frame: 5,
            ch: 1,
            window: 0,
            cause: "input-driven knife-edge, filter inactive both sides (spec §11)",
            want_ord: [0, 0],
            want_coef_hi: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1018, 1000],
            want_active: [false, false],
            got_ord: [2, 0],
            got_coef_hi: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1049, 1000],
        },
        KnownTnsDivergence {
            fixture: "sine-stereo",
            frame: 6,
            ch: 0,
            window: 0,
            cause: "input-driven knife-edge, filter inactive both sides (spec §11)",
            want_ord: [0, 0],
            want_coef_hi: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1015, 1000],
            want_active: [false, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1024, 1000],
        },
        KnownTnsDivergence {
            fixture: "sine-stereo",
            frame: 6,
            ch: 1,
            window: 0,
            cause: "input-driven knife-edge, filter inactive both sides (spec §11)",
            want_ord: [0, 0],
            want_coef_hi: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1015, 1000],
            want_active: [false, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1024, 1000],
        },
        KnownTnsDivergence {
            fixture: "sine-stereo-44100",
            frame: 5,
            ch: 0,
            window: 0,
            cause: "input-driven knife-edge, filter inactive both sides (spec §11.3)",
            want_ord: [0, 0],
            want_coef_hi: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1023, 1000],
            want_active: [false, false],
            got_ord: [5, 0],
            got_coef_hi: [0, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1039, 1000],
        },
        KnownTnsDivergence {
            fixture: "sine-stereo-44100",
            frame: 5,
            ch: 1,
            window: 0,
            cause: "input-driven knife-edge, filter inactive both sides (spec §11.3)",
            want_ord: [0, 0],
            want_coef_hi: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1023, 1000],
            want_active: [false, false],
            got_ord: [5, 0],
            got_coef_hi: [0, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1039, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 0,
            window: 1,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3820, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3861, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 0,
            window: 2,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3607, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3674, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 0,
            window: 3,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [5, 0],
            want_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [6007, 1000],
            want_active: [true, false],
            got_ord: [5, 0],
            got_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [6107, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 0,
            window: 4,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3825, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3871, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 0,
            window: 5,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3608, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3675, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 0,
            window: 6,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [5, 0],
            want_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [5904, 1000],
            want_active: [true, false],
            got_ord: [5, 0],
            got_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [5861, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 0,
            window: 7,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3819, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3863, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 1,
            window: 1,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3820, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3861, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 1,
            window: 2,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3607, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3674, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 1,
            window: 3,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [5, 0],
            want_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [6007, 1000],
            want_active: [true, false],
            got_ord: [5, 0],
            got_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [6107, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 1,
            window: 4,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3825, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3871, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 1,
            window: 5,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3608, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3675, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 1,
            window: 6,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [5, 0],
            want_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [5904, 1000],
            want_active: [true, false],
            got_ord: [5, 0],
            got_coef_hi: [0, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [5861, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 1,
            ch: 1,
            window: 7,
            cause: "input-driven gain shift, same filter (spec §11.1)",
            want_ord: [4, 0],
            want_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [3819, 1000],
            want_active: [true, false],
            got_ord: [4, 0],
            got_coef_hi: [0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [3863, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 2,
            ch: 0,
            window: 0,
            cause: "input-driven parcor border flips, filter active both sides (spec §11.1)",
            want_ord: [10, 0],
            want_coef_hi: [6, 1, -2, 2, -1, 0, 0, 0, 1, -1, 0, 0],
            want_gain: [18744, 1012],
            want_active: [true, false],
            got_ord: [11, 0],
            got_coef_hi: [6, 2, -2, 2, -1, 0, 0, -1, 1, -1, 1, 0],
            got_gain: [27781, 1012],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 2,
            ch: 1,
            window: 0,
            cause: "input-driven parcor border flips, filter active both sides (spec §11.1)",
            want_ord: [10, 0],
            want_coef_hi: [6, 1, -2, 2, -1, 0, 0, 0, 1, -1, 0, 0],
            want_gain: [18744, 1012],
            want_active: [true, false],
            got_ord: [11, 0],
            got_coef_hi: [6, 2, -2, 2, -1, 0, 0, -1, 1, -1, 1, 0],
            got_gain: [27781, 1012],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 5,
            ch: 0,
            window: 0,
            cause: "input-driven knife-edge, filter inactive both sides (spec §11.1)",
            want_ord: [0, 0],
            want_coef_hi: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1018, 1000],
            want_active: [false, false],
            got_ord: [2, 0],
            got_coef_hi: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1049, 1000],
        },
        KnownTnsDivergence {
            fixture: "stepdown-stereo",
            frame: 5,
            ch: 1,
            window: 0,
            cause: "input-driven knife-edge, filter inactive both sides (spec §11.1)",
            want_ord: [0, 0],
            want_coef_hi: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            want_gain: [1018, 1000],
            want_active: [false, false],
            got_ord: [2, 0],
            got_coef_hi: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            got_gain: [1049, 1000],
        },
    ];

    /// Look up a known maxscale divergence (linear scan; 21 entries).
    fn find_maxscale_entry(
        fixture: &str,
        frame: u32,
        ch: usize,
    ) -> Option<&'static KnownMaxscaleDivergence> {
        KNOWN_MAXSCALE
            .iter()
            .find(|e| e.fixture == fixture && e.frame == frame && e.ch == ch)
    }

    /// Look up a known TNS divergence (linear scan; 40 entries).
    fn find_tns_entry(
        fixture: &str,
        frame: u32,
        ch: usize,
        window: usize,
    ) -> Option<&'static KnownTnsDivergence> {
        KNOWN_TNS
            .iter()
            .find(|e| e.fixture == fixture && e.frame == frame && e.ch == ch && e.window == window)
    }

    /// Compare TNS state+info for one channel against committed windows.
    /// Every field is discrete except prediction gains (see below):
    /// exact agreement required.
    fn compare_tns(
        name: &str,
        frame: u32,
        ch: usize,
        state: &TnsChannel,
        info: &TnsChannel,
        want: &[testdata::TnsWindow],
        short: bool,
    ) {
        let tag = format!("{name} f{frame} ch{ch} tns");
        assert_eq!(want.len(), if short { 8 } else { 1 }, "{tag} window count");
        for tw in want.iter() {
            let w = tw.window;
            assert_eq!(
                info.num_filters[w], tw.num_filters as usize,
                "{tag} w{w} nf"
            );
            assert_eq!(
                info.length[w],
                [tw.length[0] as usize, tw.length[1] as usize],
                "{tag} w{w} len"
            );
            assert_eq!(
                info.direction[w],
                [tw.direction[0] as usize, tw.direction[1] as usize],
                "{tag} w{w} dir"
            );
            assert_eq!(&info.coef[w][1][..], &tw.coef_lo[..], "{tag} w{w} coef LO");
            // Active flags always agree (no corpus case flips activation);
            // strict here keeps any future flip loud.
            assert_eq!(state.active[w], tw.active, "{tag} w{w} active");
            if let Some(e) = find_tns_entry(name, frame, ch, w) {
                // Known input-driven divergence (spec §11): pin the
                // committed reference side AND the recorded Rust outcome.
                // Either side changing — including the filter going
                // inactive on either side — fails loudly.
                assert_eq!(
                    tw.order, e.want_ord,
                    "{tag} w{w} reference ord ({})",
                    e.cause
                );
                assert_eq!(
                    &tw.coef_hi[..],
                    &e.want_coef_hi[..],
                    "{tag} w{w} reference coef HI ({})",
                    e.cause
                );
                assert_eq!(
                    tw.gain, e.want_gain,
                    "{tag} w{w} reference gain ({})",
                    e.cause
                );
                assert_eq!(
                    tw.active, e.want_active,
                    "{tag} w{w} reference active ({})",
                    e.cause
                );
                assert_eq!(
                    info.order[w], e.got_ord,
                    "{tag} w{w} recorded ord ({})",
                    e.cause
                );
                assert_eq!(
                    &info.coef[w][0][..],
                    &e.got_coef_hi[..],
                    "{tag} w{w} recorded coef HI ({})",
                    e.cause
                );
                assert_eq!(
                    state.gain[w], e.got_gain,
                    "{tag} w{w} recorded gain ({})",
                    e.cause
                );
            } else {
                assert_eq!(
                    info.order[w],
                    [tw.order[0] as usize, tw.order[1] as usize],
                    "{tag} w{w} ord"
                );
                assert_eq!(&info.coef[w][0][..], &tw.coef_hi[..], "{tag} w{w} coef HI");
                // Prediction gains: VALUES take the evidence-based
                // `gain_within_tolerance` gate (floor 8 + 1/1000 relative:
                // admits proven same-input float-Schur residuals up to Δ11
                // while every input-driven divergence on record, smallest
                // Δ29, still fails). The threshOn DECISION stays exact via
                // the active flags above; orders/coefficients stay exact —
                // no decision flip can hide behind the gain value gate.
                for f in 0..2 {
                    assert!(
                        tns::gain_within_tolerance(state.gain[w][f], tw.gain[f]),
                        "{tag} w{w} gain[{f}]: got {} want {} (tol {})",
                        state.gain[w][f],
                        tw.gain[f],
                        tns::gain_tolerance(tw.gain[f])
                    );
                }
            }
        }
        // Merged flag is per-channel (repeated on every committed line).
        if let Some(first) = want.first() {
            assert_eq!(state.merged, first.merged, "{tag} merged");
        }
    }

    /// Compare element-level MS vectors (stereo only).
    fn compare_ms(name: &str, frame: u32, got: &MsOutput, want: &[testdata::MsWindow]) {
        let tag = format!("{name} f{frame} ms");
        assert_eq!(got.mid.len(), want.len(), "{tag} windows");
        for (w, wm) in want.iter().enumerate() {
            assert_q31_close(&got.mid[w], &wm.mid, &format!("{tag} mid w{w}"));
            assert_q31_close(&got.side[w], &wm.side, &format!("{tag} side w{w}"));
            assert_ld_close(&got.mid_ld[w], &wm.mid_ld, &format!("{tag} mid_ld w{w}"));
            assert_ld_close(&got.side_ld[w], &wm.side_ld, &format!("{tag} side_ld w{w}"));
        }
    }

    const CASES: &[(&str, &[u32])] = &[
        ("onset-stereo", &[22, 23, 24, 25, 26]),
        ("stepdown-stereo", &[0, 1, 2, 5]),
        ("click-mono", &[11]),
        ("edge-mono", &[3, 4, 5]),
        ("sine-stereo", &[5, 6]),
        ("impulse-mono", &[4, 5]),
        ("silence-stereo", &[5]),
        ("multitone-stereo", &[1, 5]),
        ("sweep-stereo", &[10]),
        ("noise-mono", &[5]),
        ("tonal-noise-stereo", &[5]),
        ("quiet-sine-stereo", &[5]),
        ("sine-stereo-44100", &[5]),
        ("onset-stereo-44100", &[21, 22]),
    ];

    #[test]
    fn psy_end_to_end_matches_oracle() {
        for (name, frames) in CASES {
            let mut stream = Stream::new(name);
            let seq_rows = testdata::load_seq(name);
            let max_f = frames.iter().max().expect("frames");
            for f in 0..=*max_f {
                let (seqs, groupings, (left, right_opt, ms_opt)) = stream.step(f);
                // Window decisions must match exactly on every frame (also
                // extends the M2 decision gate to the new fixtures).
                for (ch, seq) in seqs.iter().enumerate() {
                    let row = seq_rows
                        .iter()
                        .find(|r| r.frame == f && r.channel as usize == ch)
                        .expect("seq row");
                    assert_eq!(*seq as u8, row.seq, "{name} f{f} ch{ch} seq");
                }
                if !frames.contains(&f) {
                    continue;
                }
                let short0 = seqs[0] == WindowSequence::Short;
                let want0 = testdata::load_psy(name, f, 0, short0);
                compare_channel(name, f, 0, &left, &want0, short0, groupings[0]);
                compare_tns(
                    name,
                    f,
                    0,
                    &stream.psy[0].tns_state,
                    &stream.psy[0].tns_info,
                    &want0.tns,
                    short0,
                );
                if stream.channels == 2 {
                    let short1 = seqs[1] == WindowSequence::Short;
                    let right = right_opt.as_ref().expect("right output");
                    let want1 = testdata::load_psy(name, f, 1, short1);
                    compare_channel(name, f, 1, right, &want1, short1, groupings[1]);
                    compare_tns(
                        name,
                        f,
                        1,
                        &stream.psy[1].tns_state,
                        &stream.psy[1].tns_info,
                        &want1.tns,
                        short1,
                    );
                    let ms = ms_opt.expect("stereo ms");
                    compare_ms(name, f, &ms, &testdata::load_ms(name, f));
                }
            }
        }
    }

    #[test]
    fn lfe_path_skips_tns_and_preecho() {
        // Structural coverage of the LFE early-outs (no LFE fixtures exist):
        // TNS stays fully inactive and pre-echo history stays at its
        // initializer while energies still compute.
        let ((lcfg, _), _) = testdata::load_cfg(48000, 96000);
        let tables = PsyTables::for_rate(
            48000,
            96000,
            192000,
            19293,
            lcfg.min_snr_ld.clone(),
            lcfg.min_snr_ld.clone(),
        )
        .expect("tables");
        let mut psy = PsyChannel::new(tables.long_cfg.pcm_quant_q31());
        let mut spec = [1000.0f64; 1024];
        let mut input = ChannelInput {
            spectrum: &mut spec,
            seq: WindowSequence::Long,
            grouping: (1, [1, 0, 0, 0]),
            is_lfe: true,
        };
        let (out, right, ms) = process_pair(&mut psy, None, input, None, &tables);
        assert!(right.is_none() && ms.is_none());
        for w in 0..8 {
            assert_eq!(psy.tns_state.active[w], [false, false]);
            assert_eq!(psy.tns_state.gain[w], [0, 0]);
        }
        assert!(!out.energies.iter().all(|w| w.iter().all(|&e| e == 0)));
        // Pre-echo untouched: history still the initializer.
        assert_eq!(
            &psy.preecho_prev()[..tables.long_cfg.sfb_active()],
            &tables.long_cfg.pcm_quant_q31()[..tables.long_cfg.sfb_active()]
        );
    }
}
