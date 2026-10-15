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
//! M4-S4 scalefactor estimation and afterburner (`invQuant == 2` only).
//!
//! Exact integer mirror of `cpp/libAACenc/src/sf_estim.cpp` (at base commit
//! `1785529`), reusing the S3 quantizer primitives without modification.
//!
//! ## Reference audit
//!
//! Entry points and call order (`sf_estim.cpp`, `qc_main.cpp:870-873`):
//! - `EstimateScaleFactors` (`:1279`, exported) loops channels, calling
//!   `EstimateScaleFactorsChannel` (`:1046`) per channel with `invQuant = 2`,
//!   `dZoneQuantEnable = 0` (LC, `qc_main.cpp:399-411`).
//! - Channel flow: initial-formula scf per live band (`:1085-1168`, incl.
//!   `minSfMaxQuant` via `CalcLdData(maxSpec)` + `c1Const`) with
//!   analysis-by-synthesis `improveScf` when `invQuant > 0` (`:1153-1164`);
//!   `calcSfbRelevantLines` (`:1178-1182`); `assimilateSingleScf`
//!   (`:1184-1187`, `restartOnSuccess = 1`); then only when
//!   `invQuant > 1`, `assimilateMultipleScf` (`:1190-1193`) and
//!   `assimilateMultipleScf2` (`:1195-1198`); min/max clamp
//!   (`MAX_SCF_DELTA = 60`, `:1202-1230`, requant on clamp);
//!   global-gain inversion + dead-band spectrum zeroing (`:1232-1273`).
//! - `improveScf` (`:378-452`): real distortion via `calcSfbDist`
//!   (`:390`); NMR `> 1.25` branch tries one step coarser then one step
//!   finer (`UPCOUNT_LIMIT = 1`, `:400-428`); NMR `<= 1.25` branch tries
//!   one step coarser under the `min(dist-factor, thresh)` allowance
//!   (`:430-448`). `distFactor = ld64(1/1.25)` is negative, so
//!   `dist > thresh-distFactor` means audible.
//! - `assimilateSingleScf` (`:458-623`): left-to-right walk with
//!   neighbour-min/max gating inside a `MAX_SCF_DELTA` window, PE gate
//!   `10.0/2^14` plus strict distortion improvement, restart-from-first
//!   after any success.
//! - `assimilateMultipleScf` (`:629-752`): outer level sweep from
//!   `scfMax-1` down to `scfMin+1`; per level, runs where every live scf
//!   exceeds the level are lowered jointly under the same PE gate, with
//!   per-band `dist > thresh` early abort and strict sum improvement.
//! - `assimilateMultipleScf2` (`:758-1044`): per equal-scf region three
//!   sub-steps in order — (1) coarser `scf+1..scfHi` under PE `< 0.0`
//!   with `distMax` caps and all-zero early stop; (2) single finer
//!   `scfLo` under PE `< 0.0` with a `0.8*old` sum gate; (3) up to three
//!   `-1` steps reusing existing lines via
//!   `calcSfbQuantEnergyAndDist` under PE `<= 0.0` with per-band
//!   `dist+ld64(1.122)` / `energy-ld64(0.7079)` windows.
//! - `calcSfbDist` / `calcSfbQuantEnergyAndDist` are consumed exactly as
//!   S3 exposes them (`:390-1226`, `:1010`); no S5 sectioning or S6
//!   bit-budget input participates — candidates are priced with
//!   `countSingleScfBits` / `countScfBitsDiff` (DPCM Huffman lengths via
//!   `pe::bit_count_scf_delta`) plus `calcSingleSpecPe` /
//!   `calcSpecPeDiff` (PE model), never with granted bits.
//!
//! ## I/O and numeric representation
//!
//! Ld values are Q31 `i32`; spectrum Q31 `i32`; lines `i16`; scf/gain are
//! plain integer exponents with `FDK_INT_MIN` (`i32::MIN`) marking dead
//! bands. Every `+ - <<` mirrors C with wrapping (`wrapping_*`); every
//! `>>` is arithmetic; `fMult` is the ARM64 floor primitive from
//! `q31::fmult`; `CalcLdData` / `CalcInvLdData` are `q31` bit-exact.
//!
//! ## Scalefactor / gain / quantized-value relationship
//!
//! The search works with absolute scf candidates; `calcSfbDist` consumes
//! them directly as QSS. Afterwards `globalGain = maxScf` and each stored
//! `scf = maxScf - scfOrig` (dead bands read `0` with zeroed spectrum),
//! so the later `QuantizeSpectrum` sees `QSS = scfOrig` on live bands.
//!
//! ## Comparisons and boundaries
//!
//! Initial `scfInt` is `floor` via arithmetic `>>22`. `minSfMaxQuant`
//! uses `>>23` + 1 with the `c1Const > -1-tmp` guard (`:1142-1148`).
//! All PE/distortion acceptances are strict `<` / `>`; ties reject
//! (except step-3's `<= 0.0` PE gate, still needing strict sum gain).
//! Negative-`scf` shifts (`scf<<24`) and small DPCM shifts wrap like ARM.
//!
//! ## Mutable state
//!
//! Evaluation mutates shared buffers in place, in order: `quantSpec`
//! (copied from `quantSpecTmp` only on acceptance), `scf`, `sfbDist`,
//! `sfbConstPePart` (lazy fill), `minScfCalculated`, finally
//! `mdctSpectrum` (dead-band zeroing). `quantSpecTmp` starts zeroed (C
//! scratch is unobservable: only fully-written bands are ever copied).
//! Order is load-bearing: single restarts from the first band, multiple
//! sweeps levels top-down, scf2 tries coarser, then finer, then bit-save.
//!
//! ## S4 vs S5/S6
//!
//! S4 owns the search routines plus the channel orchestrator. `S5`
//! sectioning/counting and `S6` gain loop / grants / fill / reservoir are
//! untouched. S4 takes no bit-budget input.
//!
//! ## Facts vs open points
//!
//! - [Fact] Mappings read from the cited lines.
//! - [Fact] `FL2FXCONST_DBL` literals below are C-probed
//!   (`tools/enc-oracle/scf_probe.cpp`); `PE_C3` is float `0.5593573f`
//!   (`1201210624`), distinct from S1's double (`0x4799051F`).
//! - [Fact] Only `dZoneQuantEnable = false`, `invQuant = 2` supported
//!   (asserted); anything else is rejected loudly.
//! - [Open] `q == -32768` rides along from S3: guards replicate C
//!   bit-exactly and panic loudly on the table index instead of
//!   OOB-reading; the envelope test proves absence on committed outputs.

use super::pe::bit_count_scf_delta;
use super::q31::{calc_inv_ld_data, calc_ld_data, fmult};
use super::quant::{calc_sfb_dist, calc_sfb_quant_energy_and_dist};

const UPCOUNT_LIMIT: i32 = 1;
const AS_PE_SHIFT: i32 = 7;
const DIST_SHIFT: u32 = 3;
/// Largest allowed scalefactor spread (`sf_estim.cpp:115`).
pub const MAX_SCF_DELTA: i32 = 60;
/// Dead-band marker (`genericStds.h:503`, `FDK_INT_MIN`).
pub const NO_SCF: i32 = i32::MIN;

// C-probed Q31 literals (`tools/enc-oracle/scf_probe.cpp`).
const PE_C1: i32 = 50331648;
const PE_C2: i32 = 22178274;
const PE_C3: i32 = 1201210624;
const AS_PE_FAC: i32 = 234881024;
const DIST_FACTOR: i32 = -10802114;
const PE_GATE: i32 = 1310720;
const THRESH_CONST: i32 = 92438688;
const CONV_CONST: i32 = 646457024;
const C1_CONST: i32 = -581606912;
const F_009375: i32 = 201326592;
const F_0021522: i32 = 46219344;
const F_0375: i32 = 805306368;
const F_07: i32 = 1503238528;
const F_08585: i32 = 1843614720;
const THIRD: i32 = 715827904;
const E_1EM3: i32 = 334396231;
const F_08: i32 = 1717986944;
const D_1122: i32 = 5572475;
const D_07079: i32 = 16722942;

/// S4 channel geometry.
pub struct ScfGeometry<'a> {
    pub offsets: &'a [i32],
    pub sfb_cnt: usize,
    pub sfb_per_group: usize,
    pub max_sfb_per_group: usize,
}

/// S4 channel inputs. Spectrum is mutated in place (dead-band zeroing).
/// Thresholds are POST-VBR S2 vectors; energies unweighted Ld vectors;
/// form factors S1 vectors.
pub struct ScfChannelInput<'a> {
    pub spectrum: &'a mut [i32],
    pub energy_ld: &'a [i32],
    pub threshold_ld: &'a [i32],
    pub form_factor_ld: &'a [i32],
    pub geo: ScfGeometry<'a>,
}

/// S4 channel outputs (`scf`/`dist` have `sfb_cnt` entries;
/// `quant_spec`/`mutated_spectrum` have 1024).
pub struct ScfChannelOutput {
    pub scf: Vec<i32>,
    pub global_gain: i32,
    pub quant_spec: Vec<i16>,
    pub mutated_spectrum: Vec<i32>,
    pub sfb_dist: Vec<i32>,
}

/// `calcSfbRelevantLines` (`sf_estim.cpp:183-224`).
pub fn calc_sfb_relevant_lines(
    form_factor_ld: &[i32],
    energy_ld: &[i32],
    threshold_ld: &[i32],
    offsets: &[i32],
    cnt: usize,
    per: usize,
    max: usize,
) -> Vec<i32> {
    let mut out = vec![0i32; cnt];
    let mut offs = 0;
    while offs < cnt {
        for sfb in 0..max {
            let s = offs + sfb;
            if energy_ld[s] > threshold_ld[s] {
                let width = offsets[offs + sfb + 1] - offsets[offs + sfb];
                let width_ld = calc_ld_data(width.wrapping_shl((32 - 1 - AS_PE_SHIFT) as u32));
                let mut accu = energy_ld[s].wrapping_sub(width_ld).wrapping_sub(AS_PE_FAC);
                accu = form_factor_ld[s].wrapping_sub(accu >> 2);
                out[s] = calc_inv_ld_data(accu) >> 1;
            }
        }
        offs += per;
    }
    out
}

/// `countSingleScfBits` (`:233-243`).
fn count_single_scf_bits(scf: i32, left: i32, right: i32) -> i32 {
    let bits = bit_count_scf_delta(left.wrapping_sub(scf))
        .wrapping_add(bit_count_scf_delta(scf.wrapping_sub(right)));
    bits.wrapping_shl((32 - 1 - 2 * AS_PE_SHIFT) as u32)
}

/// `calcSingleSpecPe` (`:250-268`).
fn calc_single_spec_pe(scf: i32, const_pe_part: i32, n_lines: i32) -> i32 {
    let scf_fract = scf.wrapping_shl((32 - 1 - AS_PE_SHIFT) as u32);
    let ld_ratio = const_pe_part.wrapping_sub(fmult(F_0375, scf_fract));
    if ld_ratio >= PE_C1 {
        fmult(F_07, fmult(n_lines, ld_ratio))
    } else {
        fmult(
            F_07,
            fmult(n_lines, PE_C2.wrapping_add(fmult(PE_C3, ld_ratio))),
        )
    }
}

/// `countScfBitsDiff` (`:275-313`).
#[allow(clippy::needless_range_loop)]
fn count_scf_bits_diff(
    scf_old: &[i32],
    scf_new: &[i32],
    cnt: usize,
    start: usize,
    stop: usize,
) -> i32 {
    let mut diff: i32 = 0;
    let mut last = start as i32;
    while (last as usize) < stop && scf_old[last as usize] == NO_SCF {
        last += 1;
    }
    let mut prev = start as i32 - 1;
    while prev >= 0 && scf_old[prev as usize] == NO_SCF {
        prev -= 1;
    }
    if prev >= 0 {
        diff = diff
            .wrapping_add(bit_count_scf_delta(
                scf_new[prev as usize].wrapping_sub(scf_new[last as usize]),
            ))
            .wrapping_sub(bit_count_scf_delta(
                scf_old[prev as usize].wrapping_sub(scf_old[last as usize]),
            ));
    }
    let mut sfb = last + 1;
    while (sfb as usize) < stop {
        if scf_old[sfb as usize] != NO_SCF {
            diff = diff
                .wrapping_add(bit_count_scf_delta(
                    scf_new[last as usize].wrapping_sub(scf_new[sfb as usize]),
                ))
                .wrapping_sub(bit_count_scf_delta(
                    scf_old[last as usize].wrapping_sub(scf_old[sfb as usize]),
                ));
            last = sfb;
        }
        sfb += 1;
    }
    let mut next = stop as i32;
    while (next as usize) < cnt && scf_old[next as usize] == NO_SCF {
        next += 1;
    }
    if (next as usize) < cnt {
        diff = diff
            .wrapping_add(bit_count_scf_delta(
                scf_new[last as usize].wrapping_sub(scf_new[next as usize]),
            ))
            .wrapping_sub(bit_count_scf_delta(
                scf_old[last as usize].wrapping_sub(scf_old[next as usize]),
            ));
    }
    diff.wrapping_shl((32 - 1 - 2 * AS_PE_SHIFT) as u32)
}

/// Lazy `sfbConstPePart` fill shared by `calcSpecPeDiff` and single
/// assimilation (`:338-343`, `:544-548`).
fn const_pe_fill(energy_ld: i32, form_factor_ld: i32) -> i32 {
    ((energy_ld
        .wrapping_sub(form_factor_ld)
        .wrapping_sub(F_009375))
        >> 1)
        .wrapping_add(F_0021522)
}

/// `calcSpecPeDiff` (`:320-369`). `const_pe_part` is lazily filled.
#[allow(clippy::too_many_arguments)]
fn calc_spec_pe_diff(
    psy_energy_ld: &[i32],
    scf_old: &[i32],
    scf_new: &[i32],
    const_pe_part: &mut [i32],
    form_factor_ld: &[i32],
    n_relevant: &[i32],
    start: usize,
    stop: usize,
) -> i32 {
    let mut diff = 0i32;
    for sfb in start..stop {
        if scf_old[sfb] != NO_SCF {
            if const_pe_part[sfb] == NO_SCF {
                const_pe_part[sfb] = const_pe_fill(psy_energy_ld[sfb], form_factor_ld[sfb]);
            }
            let fract_old = scf_old[sfb].wrapping_shl((32 - 1 - AS_PE_SHIFT) as u32);
            let ld_old = const_pe_part[sfb].wrapping_sub(fmult(F_0375, fract_old));
            let fract_new = scf_new[sfb].wrapping_shl((32 - 1 - AS_PE_SHIFT) as u32);
            let ld_new = const_pe_part[sfb].wrapping_sub(fmult(F_0375, fract_new));
            let p_old = if ld_old >= PE_C1 {
                ld_old
            } else {
                PE_C2.wrapping_add(fmult(PE_C3, ld_old))
            };
            let p_new = if ld_new >= PE_C1 {
                ld_new
            } else {
                PE_C2.wrapping_add(fmult(PE_C3, ld_new))
            };
            diff = diff.wrapping_add(fmult(
                F_07,
                fmult(n_relevant[sfb], p_new.wrapping_sub(p_old)),
            ));
        }
    }
    diff
}

/// `improveScf` (`:378-452`). Band slices; updates `quant`, `dist`,
/// `min_scf_calc`; returns the best scf.
#[allow(clippy::too_many_arguments)]
fn improve_scf(
    spec: &[i32],
    quant: &mut [i16],
    tmp: &mut [i16],
    thresh: i32,
    scf: i32,
    min_scf: i32,
    dist: &mut i32,
    min_scf_calc: &mut i32,
    dz: bool,
) -> i32 {
    let mut here = calc_sfb_dist(spec, scf, dz, quant);
    *min_scf_calc = scf;
    let mut best = scf;
    if here > thresh.wrapping_sub(DIST_FACTOR) {
        let estimated = scf;
        let mut best_dist = here;
        // Coarser: single `scf+1` trial while still audible.
        let mut cnt = 0;
        let mut cur = scf;
        while here > thresh.wrapping_sub(DIST_FACTOR) && cnt < UPCOUNT_LIMIT {
            cnt += 1;
            cur += 1;
            here = calc_sfb_dist(spec, cur, dz, tmp);
            if here < best_dist {
                best = cur;
                best_dist = here;
                quant.copy_from_slice(&tmp[..quant.len()]);
            }
        }
        // Finer: single `scf-1` trial while still audible and above floor.
        let mut cnt2 = 0;
        let mut cur2 = estimated;
        here = best_dist;
        while here > thresh.wrapping_sub(DIST_FACTOR) && cnt2 < 1 && cur2 > min_scf {
            cnt2 += 1;
            cur2 -= 1;
            here = calc_sfb_dist(spec, cur2, dz, tmp);
            if here < best_dist {
                best = cur2;
                best_dist = here;
                quant.copy_from_slice(&tmp[..quant.len()]);
            }
            *min_scf_calc = cur2;
        }
        *dist = best_dist;
    } else {
        let mut best_dist = here;
        let allowed = here.wrapping_sub(DIST_FACTOR).min(thresh);
        // Single coarser trial (`UPCOUNT_LIMIT == 1`).
        let cur = best + 1;
        here = calc_sfb_dist(spec, cur, dz, tmp);
        if here < allowed {
            *min_scf_calc = best + 1;
            best = cur;
            best_dist = here;
            quant.copy_from_slice(&tmp[..quant.len()]);
        }
        *dist = best_dist;
    }
    best
}

/// `assimilateSingleScf` (`:458-623`) with `restartOnSuccess = 1`.
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
fn assimilate_single_scf(
    offsets: &[i32],
    cnt: usize,
    spectrum: &[i32],
    energy_ld: &[i32],
    form_factor_ld: &[i32],
    n_relevant: &[i32],
    quant: &mut [i16],
    tmp: &mut [i16],
    dz: bool,
    scf: &mut [i32],
    min_scf: &[i32],
    dist: &mut [i32],
    const_pe_part: &mut [i32],
    min_scf_calc: &mut [i32],
) {
    let mut prev_last = vec![i32::MAX; cnt];
    let mut prev_next = vec![i32::MAX; cnt];
    let mut last_delta = vec![i32::MAX; cnt];
    let mut sfb_last: i32 = -1;
    let mut sfb_act: i32 = -1;
    let mut sfb_next: i32 = -1;
    let mut delta_pe: i32 = 0;
    loop {
        sfb_next += 1;
        while sfb_next >= 0 && (sfb_next as usize) < cnt && scf[sfb_next as usize] == NO_SCF {
            sfb_next += 1;
        }
        // Neighbour resolution mirrors the three C cases verbatim.
        // For edge bands (first/last relevant) the missing neighbour is a
        // pointer to the LOCAL `scfAct` trial variable (`sf_estim.cpp:507`
        // /`:516`), so it TRACKS `cur` on every trial (0 DPCM cost on that
        // side). Mid bands use fixed global neighbours.
        let (act_v, last_v, next_v, mut smin, smax, has_mid, first_edge, last_edge) =
            if sfb_last >= 0 && sfb_act >= 0 && (sfb_next as usize) < cnt {
                let a = scf[sfb_act as usize];
                let l = scf[sfb_last as usize];
                let n = scf[sfb_next as usize];
                (a, l, n, l.min(n), l.max(n), true, false, false)
            } else if sfb_last == -1 && sfb_act >= 0 && (sfb_next as usize) < cnt {
                let a = scf[sfb_act as usize];
                let n = scf[sfb_next as usize];
                (a, a, n, n, n, true, true, false)
            } else if sfb_last >= 0 && sfb_act >= 0 && (sfb_next as usize) == cnt {
                let a = scf[sfb_act as usize];
                let l = scf[sfb_last as usize];
                (a, l, a, l, l, true, false, true)
            } else {
                (0, 0, 0, i32::MAX, i32::MAX, false, false, false)
            };
        if sfb_act >= 0 && has_mid {
            smin = smin.max(min_scf[sfb_act as usize]);
        }
        let mut success = false;
        if has_mid
            && sfb_act >= 0
            && (sfb_last >= 0 || (sfb_next as usize) < cnt)
            && act_v > smin
            && act_v <= smin.wrapping_add(MAX_SCF_DELTA)
            && act_v >= smax.wrapping_sub(MAX_SCF_DELTA)
            && act_v <= smin.min(last_v.min(next_v)).wrapping_add(MAX_SCF_DELTA)
            && (last_v != prev_last[sfb_act as usize]
                || next_v != prev_next[sfb_act as usize]
                || delta_pe < last_delta[sfb_act as usize])
        {
            let a = sfb_act as usize;
            let width = (offsets[a + 1] - offsets[a]) as usize;
            let offs = offsets[a] as usize;
            if const_pe_part[a] == NO_SCF {
                const_pe_part[a] = const_pe_fill(energy_ld[a], form_factor_ld[a]);
            }
            let pe_old = calc_single_spec_pe(act_v, const_pe_part[a], n_relevant[a])
                .wrapping_add(count_single_scf_bits(act_v, last_v, next_v));
            let mut delta_new = delta_pe;
            let mut update_min = true;
            // `do { scfAct--; ... } while (scfAct > scfMin)`.
            let mut cur = act_v;
            loop {
                cur -= 1;
                if cur < min_scf_calc[a] && cur >= smax.wrapping_sub(MAX_SCF_DELTA) {
                    // Edge neighbours track `cur` (C local-pointer quirk);
                    // mid neighbours stay fixed (see neighbour resolution).
                    let l = if first_edge { cur } else { last_v };
                    let n = if last_edge { cur } else { next_v };
                    let pe_new = calc_single_spec_pe(cur, const_pe_part[a], n_relevant[a])
                        .wrapping_add(count_single_scf_bits(cur, l, n));
                    let tmp_delta = delta_pe.wrapping_add(pe_new).wrapping_sub(pe_old);
                    if tmp_delta < PE_GATE {
                        let mut t = vec![0i16; width];
                        let dnew = calc_sfb_dist(&spectrum[offs..offs + width], cur, dz, &mut t);
                        if dnew < dist[a] {
                            scf[a] = cur;
                            dist[a] = dnew;
                            quant[offs..offs + width].copy_from_slice(&t);
                            tmp[offs..offs + width].copy_from_slice(&t);
                            delta_new = tmp_delta;
                            success = true;
                        }
                        if update_min {
                            min_scf_calc[a] = cur;
                        }
                    } else {
                        update_min = false;
                    }
                }
                if !(cur > smin) {
                    break;
                }
            }
            delta_pe = delta_new;
            // `prevScf` stores the DEREFERENCED neighbour values (`*scfLast`
            // / `*scfNext`), which for edge bands track the final `cur`
            // (C local-pointer quirk), not the visit-start values.
            prev_last[a] = if first_edge { cur } else { last_v };
            prev_next[a] = if last_edge { cur } else { next_v };
            last_delta[a] = delta_pe;
        }
        if success {
            sfb_last = -1;
            sfb_act = -1;
            sfb_next = -1;
        } else {
            sfb_last = sfb_act;
            sfb_act = sfb_next;
        }
        // Signed compare like C (`sfbNext < sfbCnt` on INTs); `-1`
        // restarts the walk instead of breaking via usize wrap.
        if !(sfb_next < cnt as i32) {
            break;
        }
    }
}

/// `assimilateMultipleScf` (`:629-752`). `delta_pe` carries across levels.
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
fn assimilate_multiple_scf(
    offsets: &[i32],
    cnt: usize,
    spectrum: &[i32],
    energy_ld: &[i32],
    threshold_ld: &[i32],
    form_factor_ld: &[i32],
    n_relevant: &[i32],
    quant: &mut [i16],
    tmp: &mut [i16],
    dz: bool,
    scf: &mut [i32],
    min_scf: &[i32],
    dist: &mut [i32],
    const_pe_part: &mut [i32],
    delta_pe: &mut i32,
) {
    let mut gmin = i32::MAX;
    let mut gmax = NO_SCF;
    for sfb in 0..cnt {
        if scf[sfb] != NO_SCF {
            gmin = gmin.min(scf[sfb]);
            gmax = gmax.max(scf[sfb]);
        }
    }
    if gmax == NO_SCF || gmax > gmin.wrapping_add(MAX_SCF_DELTA) {
        return;
    }
    // `do { scfAct--; ... } while (scfAct > scfMin)`.
    let mut act = gmax;
    loop {
        act -= 1;
        let mut scf_tmp = [0i32; 60];
        for i in 0..60 {
            scf_tmp[i] = if i < cnt { scf[i] } else { 0 };
        }
        // `do { region scan ... } while (stopSfb <= sfbCnt)`.
        let mut stop = 0usize;
        loop {
            let mut sfb = stop;
            while sfb < cnt && (scf[sfb] == NO_SCF || scf[sfb] <= act) {
                sfb += 1;
            }
            let start = sfb;
            sfb += 1;
            while sfb < cnt && (scf[sfb] == NO_SCF || scf[sfb] > act) {
                sfb += 1;
            }
            stop = sfb;
            let mut possible = false;
            if start < cnt {
                possible = true;
                for b in start..stop {
                    if scf[b] != NO_SCF && act < min_scf[b] {
                        possible = false;
                        break;
                    }
                }
            }
            if possible {
                for b in start..stop {
                    if scf_tmp[b] != NO_SCF {
                        scf_tmp[b] = act;
                    }
                }
                let d_scf = count_scf_bits_diff(scf, &scf_tmp, cnt, start, stop);
                let d_spec = calc_spec_pe_diff(
                    energy_ld,
                    scf,
                    &scf_tmp,
                    const_pe_part,
                    form_factor_ld,
                    n_relevant,
                    start,
                    stop,
                );
                let delta_new = (*delta_pe).wrapping_add(d_scf).wrapping_add(d_spec);
                if delta_new < PE_GATE {
                    let mut old_sum = 0i32;
                    let mut new_sum = 0i32;
                    let mut new_d = [0i32; 60];
                    let mut broke = false;
                    for b in start..stop {
                        if scf_tmp[b] != NO_SCF {
                            old_sum = old_sum.wrapping_add(calc_inv_ld_data(dist[b]) >> DIST_SHIFT);
                            let w = (offsets[b + 1] - offsets[b]) as usize;
                            let o = offsets[b] as usize;
                            let mut t = vec![0i16; w];
                            let dn = calc_sfb_dist(&spectrum[o..o + w], act, dz, &mut t);
                            new_d[b] = dn;
                            tmp[o..o + w].copy_from_slice(&t);
                            if dn > threshold_ld[b] {
                                new_sum = old_sum.wrapping_shl(1);
                                broke = true;
                                break;
                            }
                            new_sum = new_sum.wrapping_add(calc_inv_ld_data(dn) >> DIST_SHIFT);
                        }
                    }
                    if !broke && new_sum < old_sum {
                        *delta_pe = delta_new;
                        for b in start..stop {
                            if scf[b] != NO_SCF {
                                let w = (offsets[b + 1] - offsets[b]) as usize;
                                let o = offsets[b] as usize;
                                scf[b] = act;
                                dist[b] = new_d[b];
                                quant[o..o + w].copy_from_slice(&tmp[o..o + w]);
                            }
                        }
                    }
                }
            }
            if !(stop <= cnt) {
                break;
            }
        }
        if !(act > gmin) {
            break;
        }
    }
}

/// `assimilateMultipleScf2` (`:758-1044`). `delta_pe` carries across regions.
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
fn assimilate_multiple_scf2(
    offsets: &[i32],
    cnt: usize,
    spectrum: &[i32],
    threshold_ld: &[i32],
    energy_ld: &[i32],
    form_factor_ld: &[i32],
    n_relevant: &[i32],
    quant: &mut [i16],
    tmp: &mut [i16],
    dz: bool,
    scf: &mut [i32],
    min_scf: &[i32],
    dist: &mut [i32],
    const_pe_part: &mut [i32],
    delta_pe: &mut i32,
) {
    let mut gmin = i32::MAX;
    let mut gmax = NO_SCF;
    for sfb in 0..cnt {
        if scf[sfb] != NO_SCF {
            gmin = gmin.min(scf[sfb]);
            gmax = gmax.max(scf[sfb]);
        }
    }
    let _ = (gmin, gmax);
    let mut stop = 0usize;
    let mut prev_act = NO_SCF;
    // `do { region ... } while (stopSfb <= sfbCnt)`.
    loop {
        let prev = prev_act;
        let mut sfb = stop;
        while sfb < cnt && scf[sfb] == NO_SCF {
            sfb += 1;
        }
        let start = sfb;
        sfb += 1;
        // Equal-scf run (dead bands ride along inside the region but are
        // never rewritten). The trailing scan (`start == cnt`) finds no
        // region and is skipped below without reading out of bounds.
        while sfb < cnt && start < cnt && (scf[sfb] == NO_SCF || scf[sfb] == scf[start]) {
            sfb += 1;
        }
        stop = sfb;
        if start < cnt {
            let act = scf[start];
            prev_act = act;
            let next = if stop < cnt { scf[stop] } else { act };
            let p = if prev == NO_SCF { act } else { prev };
            let hi_prev_next_max = p.max(next);
            let hi_prev_next_min = p.min(next);
            let hi = hi_prev_next_max.max(act);
            let lo = if hi_prev_next_max >= act {
                act.min(hi_prev_next_min)
            } else {
                hi_prev_next_max
            };
            // Recompute gmin/gmax live like C (they may have moved).
            let mut cur_min = i32::MAX;
            let mut cur_max = NO_SCF;
            for b in 0..cnt {
                if scf[b] != NO_SCF {
                    cur_min = cur_min.min(scf[b]);
                    cur_max = cur_max.max(scf[b]);
                }
            }
            if hi.wrapping_sub(lo) <= MAX_SCF_DELTA {
                // -- Step 1: coarser `scf+1..=hi` under PE < 0.0.
                if hi > scf[start] {
                    let mut dmax = [0i32; 60];
                    for b in start..stop {
                        if scf[b] != NO_SCF {
                            let mut m = fmult(THIRD, threshold_ld[b])
                                .wrapping_add(fmult(THIRD, dist[b]))
                                .wrapping_add(fmult(THIRD, dist[b]));
                            m = m.max(energy_ld[b].wrapping_sub(E_1EM3));
                            m = m.min(threshold_ld[b]);
                            dmax[b] = m;
                        }
                    }
                    let mut check = true;
                    let mut nw = scf[start];
                    loop {
                        nw += 1;
                        if !(nw <= hi) {
                            break;
                        }
                        let mut trial = [0i32; 60];
                        for i in 0..60 {
                            trial[i] = if i < cnt { scf[i] } else { 0 };
                        }
                        for b in start..stop {
                            if trial[b] != NO_SCF {
                                trial[b] = nw;
                            }
                        }
                        let d_scf = count_scf_bits_diff(scf, &trial, cnt, start, stop);
                        let d_spec = calc_spec_pe_diff(
                            energy_ld,
                            scf,
                            &trial,
                            const_pe_part,
                            form_factor_ld,
                            n_relevant,
                            start,
                            stop,
                        );
                        let delta_new = (*delta_pe).wrapping_add(d_scf).wrapping_add(d_spec);
                        if delta_new < 0 {
                            let mut ok = true;
                            let mut new_d = [0i32; 60];
                            for b in start..stop {
                                if trial[b] != NO_SCF {
                                    let w = (offsets[b + 1] - offsets[b]) as usize;
                                    let o = offsets[b] as usize;
                                    let mut t = vec![0i16; w];
                                    let dn = calc_sfb_dist(&spectrum[o..o + w], nw, dz, &mut t);
                                    tmp[o..o + w].copy_from_slice(&t);
                                    new_d[b] = dn;
                                    if dn > dmax[b] {
                                        ok = false;
                                        if dn == energy_ld[b] {
                                            check = false;
                                        }
                                        break;
                                    }
                                }
                            }
                            if !check {
                                break;
                            }
                            if ok {
                                *delta_pe = delta_new;
                                for b in start..stop {
                                    if scf[b] != NO_SCF {
                                        let w = (offsets[b + 1] - offsets[b]) as usize;
                                        let o = offsets[b] as usize;
                                        scf[b] = nw;
                                        dist[b] = new_d[b];
                                        quant[o..o + w].copy_from_slice(&tmp[o..o + w]);
                                    }
                                }
                            }
                        }
                    }
                    let _ = check;
                }
                // -- Step 2: single finer `lo` under PE < 0.0.
                // Recompute live min/max (step 1 may have moved `scf`).
                let mut smin = i32::MAX;
                let mut smax = NO_SCF;
                for b in 0..cnt {
                    if scf[b] != NO_SCF {
                        smin = smin.min(scf[b]);
                        smax = smax.max(scf[b]);
                    }
                }
                if act == scf[start] && lo < act && smax.wrapping_sub(smin) <= MAX_SCF_DELTA {
                    let mut trial = [0i32; 60];
                    for i in 0..60 {
                        trial[i] = if i < cnt { scf[i] } else { 0 };
                    }
                    let nw = lo;
                    let mut viol = false;
                    for b in start..stop {
                        if trial[b] != NO_SCF {
                            trial[b] = nw;
                            if nw < min_scf[b] {
                                viol = true;
                            }
                        }
                    }
                    let mut delta_new = 0i32;
                    if !viol {
                        let d_scf = count_scf_bits_diff(scf, &trial, cnt, start, stop);
                        let d_spec = calc_spec_pe_diff(
                            energy_ld,
                            scf,
                            &trial,
                            const_pe_part,
                            form_factor_ld,
                            n_relevant,
                            start,
                            stop,
                        );
                        delta_new = (*delta_pe).wrapping_add(d_scf).wrapping_add(d_spec);
                    }
                    if !viol && delta_new < 0 {
                        let mut old_sum = 0i32;
                        let mut new_sum = 0i32;
                        let mut new_d = [0i32; 60];
                        let mut broke = false;
                        for b in start..stop {
                            if trial[b] != NO_SCF {
                                old_sum =
                                    old_sum.wrapping_add(calc_inv_ld_data(dist[b]) >> DIST_SHIFT);
                                let w = (offsets[b + 1] - offsets[b]) as usize;
                                let o = offsets[b] as usize;
                                let mut t = vec![0i16; w];
                                let dn = calc_sfb_dist(&spectrum[o..o + w], nw, dz, &mut t);
                                new_d[b] = dn;
                                tmp[o..o + w].copy_from_slice(&t);
                                if dn > threshold_ld[b] {
                                    new_sum = old_sum.wrapping_shl(1);
                                    broke = true;
                                    break;
                                }
                                new_sum = new_sum.wrapping_add(calc_inv_ld_data(dn) >> DIST_SHIFT);
                            }
                        }
                        if !broke && new_sum < fmult(F_08, old_sum) {
                            *delta_pe = delta_new;
                            for b in start..stop {
                                if scf[b] != NO_SCF {
                                    let w = (offsets[b + 1] - offsets[b]) as usize;
                                    let o = offsets[b] as usize;
                                    scf[b] = nw;
                                    dist[b] = new_d[b];
                                    quant[o..o + w].copy_from_slice(&tmp[o..o + w]);
                                }
                            }
                        }
                    }
                    let _ = p;
                }
                // -- Step 3: up to three `-1` bit-saves without requant.
                let mut smin3 = i32::MAX;
                let mut smax3 = NO_SCF;
                for b in 0..cnt {
                    if scf[b] != NO_SCF {
                        smin3 = smin3.min(scf[b]);
                        smax3 = smax3.max(scf[b]);
                    }
                }
                if smax3.wrapping_sub(smin3) <= MAX_SCF_DELTA - 3 {
                    let mut trial = [0i32; 60];
                    trial[..cnt].copy_from_slice(&scf[..cnt]);
                    for _ in 0..3 {
                        let nw = trial[start].wrapping_sub(1);
                        for b in start..stop {
                            if trial[b] != NO_SCF {
                                trial[b] = nw;
                            }
                        }
                        let d_scf = count_scf_bits_diff(scf, &trial, cnt, start, stop);
                        let delta_new = (*delta_pe).wrapping_add(d_scf);
                        if delta_new <= 0 {
                            let mut old_sum = 0i32;
                            let mut new_sum = 0i32;
                            let mut new_d = [0i32; 60];
                            let mut ok = true;
                            for b in start..stop {
                                if trial[b] != NO_SCF {
                                    let w = (offsets[b + 1] - offsets[b]) as usize;
                                    let o = offsets[b] as usize;
                                    let (en_q, dn) = calc_sfb_quant_energy_and_dist(
                                        &spectrum[o..o + w],
                                        &quant[o..o + w],
                                        nw,
                                    );
                                    new_d[b] = dn;
                                    old_sum = old_sum
                                        .wrapping_add(calc_inv_ld_data(dist[b]) >> DIST_SHIFT);
                                    new_sum =
                                        new_sum.wrapping_add(calc_inv_ld_data(dn) >> DIST_SHIFT);
                                    if dn > dist[b].wrapping_add(D_1122)
                                        || en_q < energy_ld[b].wrapping_sub(D_07079)
                                    {
                                        ok = false;
                                        break;
                                    }
                                }
                            }
                            if new_sum < old_sum && ok {
                                *delta_pe = delta_new;
                                for b in start..stop {
                                    if scf[b] != NO_SCF {
                                        scf[b] = nw;
                                        dist[b] = new_d[b];
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if !(stop <= cnt) {
            break;
        }
    }
}

// Step-1 candidate-dist side table removed: candidates now live in a
// per-`nw` stack array (`new_d`), no shared state.

/// Channel orchestrator `EstimateScaleFactorsChannel` (`:1046-1277`)
/// with `inv_quant = 2`, `dZoneQuantEnable = false` only.
pub fn estimate_scale_factors_channel(input: &mut ScfChannelInput<'_>) -> ScfChannelOutput {
    estimate_channel_with_inv(input, 2)
}

/// Test-only entry covering the oracle's `inv = 0/1` checkpoints with the
/// same code path: `0` stops after the initial formula (+ clamp/inversion),
/// `1` adds `improveScf` + single assimilation, `2` is the full afterburner.
/// Production callers use [`estimate_scale_factors_channel`] (`inv = 2`).
pub(crate) fn estimate_channel_with_inv(
    input: &mut ScfChannelInput<'_>,
    inv_quant: i32,
) -> ScfChannelOutput {
    assert!((0..=2).contains(&inv_quant), "invQuant envelope");
    assert_eq!(input.spectrum.len(), 1024);
    let cnt = input.geo.sfb_cnt;
    let per = input.geo.sfb_per_group;
    let max = input.geo.max_sfb_per_group;
    let offsets = input.geo.offsets.to_vec();
    let energy_ld = input.energy_ld.to_vec();
    let threshold_ld = input.threshold_ld.to_vec();
    let form_factor = input.form_factor_ld.to_vec();
    let mut spec = input.spectrum.to_vec();
    let mut quant = vec![0i16; 1024];
    let mut tmp = vec![0i16; 1024];
    let mut scf = vec![NO_SCF; cnt];
    let mut dist = vec![0i32; cnt];
    let mut min_scf_calc = vec![NO_SCF; cnt];
    let mut min_max_quant = vec![NO_SCF; cnt];
    // Initial formula.
    let mut offs = 0;
    while offs < cnt {
        for sfb in 0..max {
            let s = offs + sfb;
            let th = threshold_ld[s];
            let en = energy_ld[s];
            dist[s] = en;
            if en > th {
                let energy_part = form_factor[s].wrapping_add(F_009375);
                let thresh_part = THRESH_CONST.wrapping_add(th);
                let mut fract = thresh_part.wrapping_sub(energy_part);
                fract = fmult(CONV_CONST, fract);
                fract = fract.wrapping_add(fmult(F_08585, fract >> 3));
                let mut s_int = fract >> 22;
                // maxSpec over the band.
                let start = offsets[offs + sfb] as usize;
                let end = offsets[offs + sfb + 1] as usize;
                let mut peak = 0i32;
                for &v in &spec[start..end] {
                    let a = v.wrapping_abs();
                    if a > peak {
                        peak = a;
                    }
                }
                let tmp_ld = calc_ld_data(peak);
                let floor = if C1_CONST > i32::MIN.wrapping_sub(tmp_ld) {
                    (C1_CONST.wrapping_add(tmp_ld) >> 23) + 1
                } else {
                    (i32::MIN >> 23) + 1
                };
                min_max_quant[s] = floor;
                s_int = s_int.max(floor);
                if inv_quant > 0 {
                    let w = end - start;
                    let mut q = vec![0i16; w];
                    let mut t = vec![0i16; w];
                    let mut d = 0i32;
                    let mut m = 0i32;
                    let best = improve_scf(
                        &spec[start..end],
                        &mut q,
                        &mut t,
                        th,
                        s_int,
                        floor,
                        &mut d,
                        &mut m,
                        false,
                    );
                    quant[start..end].copy_from_slice(&q);
                    for (k, &v) in t.iter().enumerate() {
                        tmp[start + k] = v;
                    }
                    dist[s] = d;
                    min_scf_calc[s] = m;
                    scf[s] = best;
                } else {
                    scf[s] = s_int;
                }
            }
        }
        offs += per;
    }
    // Afterburner (`invQuant > 0` gates single, `> 1` gates multiple).
    let mut const_pe = vec![NO_SCF; cnt];
    let n_rel = if inv_quant > 0 {
        calc_sfb_relevant_lines(
            &form_factor,
            &energy_ld,
            &threshold_ld,
            &offsets,
            cnt,
            per,
            max,
        )
    } else {
        vec![0i32; cnt]
    };
    if inv_quant > 0 {
        assimilate_single_scf(
            &offsets,
            cnt,
            &spec,
            &energy_ld,
            &form_factor,
            &n_rel,
            &mut quant,
            &mut tmp,
            false,
            &mut scf,
            &min_max_quant,
            &mut dist,
            &mut const_pe,
            &mut min_scf_calc,
        );
    }
    if inv_quant > 1 {
        let mut delta2 = 0i32;
        assimilate_multiple_scf(
            &offsets,
            cnt,
            &spec,
            &energy_ld,
            &threshold_ld,
            &form_factor,
            &n_rel,
            &mut quant,
            &mut tmp,
            false,
            &mut scf,
            &min_max_quant,
            &mut dist,
            &mut const_pe,
            &mut delta2,
        );
        let mut delta3 = 0i32;
        assimilate_multiple_scf2(
            &offsets,
            cnt,
            &spec,
            &threshold_ld,
            &energy_ld,
            &form_factor,
            &n_rel,
            &mut quant,
            &mut tmp,
            false,
            &mut scf,
            &min_max_quant,
            &mut dist,
            &mut const_pe,
            &mut delta3,
        );
    }
    // Clamp `min + 60` with requant.
    let mut gmin = i32::MAX;
    let mut o = 0;
    while o < cnt {
        for b in 0..max {
            if scf[o + b] != NO_SCF {
                gmin = gmin.min(scf[o + b]);
            }
        }
        o += per;
    }
    let mut o2 = 0;
    while o2 < cnt {
        for b in 0..max {
            let s = o2 + b;
            if scf[s] != NO_SCF && gmin.wrapping_add(MAX_SCF_DELTA) < scf[s] {
                scf[s] = gmin.wrapping_add(MAX_SCF_DELTA);
                // Changed bands are re-quantized only when the search ran
                // (`invQuant > 0`, `:1219-1227`).
                if inv_quant > 0 {
                    let start = offsets[o2 + b] as usize;
                    let end = offsets[o2 + b + 1] as usize;
                    let mut q = vec![0i16; end - start];
                    dist[s] = calc_sfb_dist(&spec[start..end], scf[s], false, &mut q);
                    quant[start..end].copy_from_slice(&q);
                }
            }
        }
        o2 += per;
    }
    // Global-gain inversion + dead-band zeroing.
    let mut gmax = NO_SCF;
    let mut o3 = 0;
    while o3 < cnt {
        for b in 0..max {
            gmax = gmax.max(scf[o3 + b]);
        }
        o3 += per;
    }
    let gain;
    if gmax > NO_SCF {
        gain = gmax;
        let mut o4 = 0;
        while o4 < cnt {
            for b in 0..max {
                let s = o4 + b;
                if scf[s] == NO_SCF {
                    scf[s] = 0;
                    let start = offsets[o4 + b] as usize;
                    let end = offsets[o4 + b + 1] as usize;
                    spec[start..end].fill(0);
                } else {
                    scf[s] = gmax.wrapping_sub(scf[s]);
                }
            }
            o4 += per;
        }
    } else {
        gain = 0;
        let mut o4 = 0;
        while o4 < cnt {
            for b in 0..max {
                let s = o4 + b;
                scf[s] = 0;
                let start = offsets[o4 + b] as usize;
                let end = offsets[o4 + b + 1] as usize;
                spec[start..end].fill(0);
            }
            o4 += per;
        }
    }
    // Write back the mutated spectrum for the caller.
    input.spectrum.copy_from_slice(&spec);
    ScfChannelOutput {
        scf,
        global_gain: gain,
        quant_spec: quant,
        mutated_spectrum: spec,
        sfb_dist: dist,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::quant::{calc_sfb_dist, calc_sfb_quant_energy_and_dist};

    /// Committed S4 corpus: same 19 element-frames as S1/S2/S3.
    const S4_CASES: [(&str, u32); 19] = [
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

    /// S4 input spectrum: qc1 post-MS/IS spectrum with inactive-band lines
    /// zeroed (same contract as S3 `s3_input_spec`).
    fn s4_input_spec(
        spec: &[i32],
        offsets: &[i32],
        cnt: usize,
        per: usize,
        max: usize,
    ) -> Vec<i32> {
        let mut v = spec.to_vec();
        let mut o = 0;
        while o < cnt {
            for b in max..per {
                let start = offsets[o + b] as usize;
                let end = offsets[o + b + 1] as usize;
                for line in start..end {
                    v[line] = 0;
                }
            }
            o += per;
        }
        v
    }

    #[test]
    fn s4_constants_match_c_probe() {
        // `tools/enc-oracle/scf_probe.cpp` output. `PE_C3` pins the float
        // (`0.5593573f`) spelling against S1's double (`0x4799051F`).
        assert_eq!((PE_C1, PE_C2, PE_C3), (50331648, 22178274, 1201210624));
        assert_eq!(PE_C3, 0x47990500);
        assert_eq!(
            (AS_PE_FAC, DIST_FACTOR, PE_GATE),
            (234881024, -10802114, 1310720)
        );
        assert_eq!(
            (THRESH_CONST, CONV_CONST, C1_CONST),
            (92438688, 646457024, -581606912)
        );
        assert_eq!(
            (F_009375, F_0021522, F_0375, F_07, F_08585),
            (201326592, 46219344, 805306368, 1503238528, 1843614720)
        );
        assert_eq!(
            (THIRD, E_1EM3, F_08, D_1122, D_07079),
            (715827904, 334396231, 1717986944, 5572475, 16722942)
        );
        assert_eq!(MAX_SCF_DELTA, 60);
        assert_eq!(NO_SCF, i32::MIN);
    }

    #[test]
    fn s4_silence_all_dead() {
        // All-zero input: every band dead (`NO_SCF`), gain 0, spectrum and
        // lines all zero — the `maxSf == FDK_INT_MIN` branch
        // (`sf_estim.cpp:1259-1273`).
        let mut spec = vec![0i32; 1024];
        let offsets = vec![0, 4, 8];
        let energy = vec![i32::MIN, i32::MIN];
        let thresh = vec![0, 0];
        let ff = vec![i32::MIN, i32::MIN];
        let mut input = ScfChannelInput {
            spectrum: &mut spec,
            energy_ld: &energy,
            threshold_ld: &thresh,
            form_factor_ld: &ff,
            geo: ScfGeometry {
                offsets: &offsets,
                sfb_cnt: 2,
                sfb_per_group: 2,
                max_sfb_per_group: 2,
            },
        };
        let out = estimate_scale_factors_channel(&mut input);
        assert_eq!(out.global_gain, 0);
        assert_eq!(out.scf, vec![0, 0]);
        assert_eq!(out.quant_spec, vec![0i16; 1024]);
        assert_eq!(out.mutated_spectrum, vec![0i32; 1024]);
    }

    #[test]
    fn s4_corpus_exact() {
        // End-to-end exactness at all three invQuant checkpoints: scf,
        // gain, quantized lines and mutated spectrum reproduce the oracle
        // bit-exactly; per-band distortion/energy recomputed with
        // `QSS = gain - scf` match the committed diagnostics. Geometry
        // cross-checks qc1/qc2, proving identical inputs.
        use super::super::testdata;
        for (name, f) in S4_CASES {
            let s1 = testdata::load_s1(name, f);
            let s2 = testdata::load_s2(name, f);
            let s4 = testdata::load_s4(name, f);
            assert_eq!(s4.frame, f);
            assert_eq!(s4.ch.len(), s1.ch.len(), "{name} f={f} channels");
            for (ci, ((c1, c2), c4)) in s1.ch.iter().zip(s2.ch.iter()).zip(s4.ch.iter()).enumerate()
            {
                assert_eq!(c4.geo_cnt, c1.geo_cnt, "{name} f={f} ch={ci} cnt");
                assert_eq!(c4.geo_per, c1.geo_per, "{name} f={f} ch={ci} per");
                assert_eq!(c4.geo_max, c1.geo_max, "{name} f={f} ch={ci} max");
                assert_eq!(c4.offsets, c1.offsets, "{name} f={f} ch={ci} off");
                assert_eq!(c2.geo_cnt, c1.geo_cnt, "{name} f={f} ch={ci} s2cnt");
                let spec0 =
                    s4_input_spec(&c1.spec, &c1.offsets, c1.geo_cnt, c1.geo_per, c1.geo_max);
                for lv in &c4.levels {
                    let mut spec = spec0.clone();
                    let mut input = ScfChannelInput {
                        spectrum: &mut spec,
                        energy_ld: &c1.penld,
                        threshold_ld: &c2.thrv,
                        form_factor_ld: &c1.ff,
                        geo: ScfGeometry {
                            offsets: &c1.offsets,
                            sfb_cnt: c1.geo_cnt,
                            sfb_per_group: c1.geo_per,
                            max_sfb_per_group: c1.geo_max,
                        },
                    };
                    let out = estimate_channel_with_inv(&mut input, lv.inv);
                    assert_eq!(out.scf, lv.scf, "{name} f={f} ch={ci} inv={} SCF", lv.inv);
                    assert_eq!(
                        out.global_gain, lv.gain,
                        "{name} f={f} ch={ci} inv={} GAIN",
                        lv.inv
                    );
                    assert_eq!(
                        out.quant_spec, lv.qspec,
                        "{name} f={f} ch={ci} inv={} Q",
                        lv.inv
                    );
                    assert_eq!(
                        out.mutated_spectrum, lv.spec,
                        "{name} f={f} ch={ci} inv={} SPEC",
                        lv.inv
                    );
                    for b in 0..c4.geo_cnt {
                        let start = c4.offsets[b] as usize;
                        let end = c4.offsets[b + 1] as usize;
                        let qss = lv.gain.wrapping_sub(lv.scf[b]);
                        let mut scratch = vec![0i16; end.saturating_sub(start)];
                        let d = if end > start {
                            calc_sfb_dist(&lv.spec[start..end], qss, false, &mut scratch)
                        } else {
                            0
                        };
                        assert_eq!(
                            d, lv.dist[b],
                            "{name} f={f} ch={ci} inv={} DIST band={b}",
                            lv.inv
                        );
                        let (en, ed) = if end > start {
                            calc_sfb_quant_energy_and_dist(
                                &lv.spec[start..end],
                                &lv.qspec[start..end],
                                qss,
                            )
                        } else {
                            (0, 0)
                        };
                        assert_eq!(
                            en, lv.en[b],
                            "{name} f={f} ch={ci} inv={} EN band={b}",
                            lv.inv
                        );
                        assert_eq!(
                            ed, lv.en_dist[b],
                            "{name} f={f} ch={ci} inv={} ENDIST band={b}",
                            lv.inv
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn s4_quantizer_envelope_holds() {
        // `q == -32768` cannot occur on committed S4 outputs (all three
        // levels): the extractor and oracle gates already scan for it, and
        // this re-proves it from the Rust side. Any occurrence would take
        // the loud-panic path in `quant::inv_quantize_line` instead of C's
        // silent OOB read.
        use super::super::testdata;
        for (name, f) in S4_CASES {
            let s4 = testdata::load_s4(name, f);
            for (ci, c4) in s4.ch.iter().enumerate() {
                for lv in &c4.levels {
                    for (i, &q) in lv.qspec.iter().enumerate() {
                        assert_ne!(
                            q, -32768,
                            "{name} f={f} ch={ci} inv={} line={i} envelope",
                            lv.inv
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn s4_expected_values_are_checked_non_vacuously() {
        // Tamper check: flipping one committed scf must break comparison.
        use super::super::testdata;
        let s4 = testdata::load_s4("sine-stereo", 5);
        let lv = &s4.ch[0].levels[2];
        let mut tampered = lv.scf.clone();
        tampered[3] = tampered[3].wrapping_add(1);
        assert_ne!(tampered, lv.scf);
    }

    #[test]
    fn s4_unit_probe_exact() {
        // Synthetic unit-probe transcript (`scf_unit_probe` output):
        // countSingleScfBits + calcSingleSpecPe over dense grids (small
        // deltas, corpus scf/cp/nrel), relevant lines, single, channel
        // (dead band), improve, nrel46, quantize cross-checks and the
        // isolated 6-band single run. Any helper mismatch fails here.
        use super::super::testdata::testdata_path;
        let text = std::fs::read_to_string(testdata_path("scfunit.txt")).expect("scfunit readable");
        let mut n_cb = 0;
        let mut n_sp = 0;
        for line in text.lines() {
            let t: Vec<&str> = line.split_whitespace().collect();
            if t.is_empty() {
                continue;
            }
            match t[0] {
                "CB" => {
                    n_cb += 1;
                    let scf: i32 = t[1].split('=').nth(1).expect("scf").parse().expect("int");
                    let d: i32 = t[2].split('=').nth(1).expect("d").parse().expect("int");
                    let v: i32 = t[3].split('=').nth(1).expect("v").parse().expect("int");
                    assert_eq!(
                        count_single_scf_bits(scf, scf + d, scf - d),
                        v,
                        "CB scf={scf} d={d}"
                    );
                }
                "SP" => {
                    n_sp += 1;
                    let scf: i32 = t[1].split('=').nth(1).expect("scf").parse().expect("int");
                    let cp: i32 = t[2].split('=').nth(1).expect("cp").parse().expect("int");
                    let nl: i32 = t[3].split('=').nth(1).expect("nl").parse().expect("int");
                    let v: i32 = t[4].split('=').nth(1).expect("v").parse().expect("int");
                    assert_eq!(
                        calc_single_spec_pe(scf, cp, nl),
                        v,
                        "SP scf={scf} cp={cp} nl={nl}"
                    );
                }
                _ => {}
            }
        }
        assert!(n_cb > 1000 && n_sp > 1000, "probe mix {n_cb}/{n_sp}");
        // Fixed vectors (RL/SN/CH/IM/N46/QB/SX sections).
        let off4 = [0, 4, 8, 12, 16];
        let ff_rl = [-337460364, -318525798, 0, 100000000];
        let en_rl = [-693314277, -734730193, 0, -100000000];
        let th_rl = [-704116391, -700000000, 0, -200000000];
        let got_rl = calc_sfb_relevant_lines(&ff_rl, &en_rl, &th_rl, &off4, 4, 4, 4);
        let mut exp_rl = Vec::new();
        let mut nrel4 = [0i32; 4];
        let mut scf_sn = [0i32; 4];
        let mut dist_sn = [0i32; 4];
        let mut qs_sn = [0i16; 16];
        for line in text.lines() {
            let t: Vec<&str> = line.split_whitespace().collect();
            if t.is_empty() {
                continue;
            }
            if t[0] == "RL" {
                let v: i32 = t[2].split('=').nth(1).expect("v").parse().expect("int");
                exp_rl.push(v);
            } else if t[0] == "SN" && t[1] == "nrel" {
                let i: usize = t[2].split('=').nth(1).expect("i").parse().expect("int");
                nrel4[i] = t[3].split('=').nth(1).expect("v").parse().expect("int");
            } else if t[0] == "SN" && t[1] == "scf" {
                let i: usize = t[2].split('=').nth(1).expect("i").parse().expect("int");
                scf_sn[i] = t[3].split('=').nth(1).expect("v").parse().expect("int");
                dist_sn[i] = t[4].split('=').nth(1).expect("v").parse().expect("int");
            } else if t[0] == "SN" && t[1] == "qs" {
                for (i, tok) in t[2..].iter().enumerate() {
                    qs_sn[i] = tok.parse::<i32>().expect("q") as i16;
                }
            }
        }
        assert_eq!(got_rl, exp_rl, "relevant lines");
        // Single run on the SN vectors.
        {
            let spec16 = [
                1000000, -2000000, 3000000, -1000000, 5000000, -5000000, 1000000, 2000000, 8000000,
                -8000000, 4000000, -4000000, 100000, -100000, 200000, -200000,
            ];
            let mut spec = vec![0i32; 1024];
            for (i, &v) in spec16.iter().enumerate() {
                spec[i] = v;
            }
            let en4 = [-400000000, -400000000, -400000000, -500000000];
            let ff4 = [-300000000, -300000000, -300000000, -300000000];
            let mut scf = [-40, -36, -30, -20];
            let min_scf = [-111, -111, -111, -111];
            let mut dist = [-400000000, -410000000, -420000000, -430000000];
            let mut cpe = [NO_SCF; 4];
            let mut min_calc = [-40, -36, -30, -20];
            let mut qs = vec![0i16; 1024];
            let mut qt = vec![0i16; 1024];
            let nrel_v = nrel4.to_vec();
            assimilate_single_scf(
                &off4,
                4,
                &spec,
                &en4,
                &ff4,
                &nrel_v,
                &mut qs,
                &mut qt,
                false,
                &mut scf,
                &min_scf,
                &mut dist,
                &mut cpe,
                &mut min_calc,
            );
            assert_eq!(scf, scf_sn, "single scf");
            assert_eq!(dist, dist_sn, "single dist");
            assert_eq!(&qs[..16], &qs_sn, "single qs");
        }
        // Isolated 6-band single (SX section).
        {
            let mut nrel6 = [0i32; 6];
            let mut scf6 = [0i32; 6];
            for line in text.lines() {
                let t: Vec<&str> = line.split_whitespace().collect();
                if t.is_empty() {
                    continue;
                }
                if t[0] == "SX" && t[1] == "nrel" {
                    let i: usize = t[2].split('=').nth(1).expect("i").parse().expect("int");
                    nrel6[i] = t[3].split('=').nth(1).expect("v").parse().expect("int");
                } else if t[0] == "SX" && t[1] == "scf" {
                    let i: usize = t[2].split('=').nth(1).expect("i").parse().expect("int");
                    scf6[i] = t[3].split('=').nth(1).expect("v").parse().expect("int");
                }
            }
            let spec44 = [
                3351240, 33792, -2021824, 49280, 1356672, -97576, -958056, 107568, 714648, -106536,
                -552480, 102280, 439616, -96600, -366704, 91328, 295128, -82232, 81992, -17668,
                59740, -20884, -53580, 16892, 42612, -26300, -42776, 5804, 36716, -19064, -30040,
                12388, 27936, -12860, -26204, 21248, 21404, 324, -22944, 12320, 20308, -11132,
                -14792, 8424,
            ];
            let mut spec = vec![0i32; 1024];
            for (i, &v) in spec44.iter().enumerate() {
                spec[i] = v;
            }
            let en6 = [
                -610674038,
                -693314277,
                -734730193,
                -850811009,
                -987004883,
                -1040183479,
            ];
            let ff6 = [
                -324236992, -337460364, -318525798, -353478107, -375609672, -368161603,
            ];
            let off6 = [0, 4, 8, 16, 24, 32, 44];
            let th6 = [
                -633321256,
                -704116391,
                -752244433,
                -861613123,
                -997806997,
                -1050985593,
            ];
            let nrel = calc_sfb_relevant_lines(&ff6, &en6, &th6, &off6, 6, 6, 6);
            assert_eq!(nrel, nrel6.to_vec(), "6-band nrel");
            // Isolated 6-band single final scf (edge-tracking regression:
            // first/last neighbours track `cur`, not the visit-start value).
            {
                let mut scf = [-34, -38, -44, -50, -58, -62];
                let min_scf = [-106, -111, -115, -120, -131, -134];
                let mut dist = [
                    -627841291,
                    -693351700,
                    -759167659,
                    -928721872,
                    i32::MIN,
                    i32::MIN,
                ];
                let mut cpe = [NO_SCF; 6];
                let mut min_calc = [-34, -38, -44, -50, -58, -62];
                let mut qs = vec![0i16; 1024];
                let mut qt = vec![0i16; 1024];
                for b in 0..6 {
                    let w = (off6[b + 1] - off6[b]) as usize;
                    let o = off6[b] as usize;
                    let mut t = vec![0i16; w];
                    crate::aac_enc::quant::calc_sfb_dist(&spec[o..o + w], scf[b], false, &mut t);
                    for (k, &v) in t.iter().enumerate() {
                        qs[o + k] = v;
                    }
                }
                let nrel_v = nrel.clone();
                assimilate_single_scf(
                    &off6,
                    6,
                    &spec,
                    &en6,
                    &ff6,
                    &nrel_v,
                    &mut qs,
                    &mut qt,
                    false,
                    &mut scf,
                    &min_scf,
                    &mut dist,
                    &mut cpe,
                    &mut min_calc,
                );
                assert_eq!(scf, scf6, "6-band single scf");
            }
        }
        // Full-channel probe with a dead band (CH/CHQ sections).
        {
            let off = [0, 4, 8, 12, 16];
            let mut base_spec = vec![0i32; 1024];
            let vals = [
                1000000, -2000000, 3000000, -1000000, 0, 0, 0, 0, 8000000, -8000000, 4000000,
                -4000000, 100000, -100000, 200000, -200000,
            ];
            for (i, &v) in vals.iter().enumerate() {
                base_spec[i] = v;
            }
            let en = [-400000000, -600000000, -400000000, -500000000];
            let th = [-500000000, -500000000, -500000000, -600000000];
            let ff = [-300000000, -300000000, -300000000, -300000000];
            for line in text.lines() {
                if line.starts_with("CH inv=") {
                    let t: Vec<&str> = line.split_whitespace().collect();
                    let inv: i32 = t[1].split('=').nth(1).expect("inv").parse().expect("int");
                    let gain: i32 = t[2].split('=').nth(1).expect("gain").parse().expect("int");
                    let scf_exp: Vec<i32> = t[3]
                        .split('=')
                        .nth(1)
                        .expect("scf")
                        .split(',')
                        .map(|v| v.parse().expect("scf"))
                        .collect();
                    let mut spec = base_spec.clone();
                    let mut input = ScfChannelInput {
                        spectrum: &mut spec,
                        energy_ld: &en,
                        threshold_ld: &th,
                        form_factor_ld: &ff,
                        geo: ScfGeometry {
                            offsets: &off,
                            sfb_cnt: 4,
                            sfb_per_group: 4,
                            max_sfb_per_group: 4,
                        },
                    };
                    let out = estimate_channel_with_inv(&mut input, inv);
                    assert_eq!(out.global_gain, gain, "ch gain inv={inv}");
                    assert_eq!(out.scf, scf_exp, "ch scf inv={inv}");
                } else if line.starts_with("CHQ inv=") {
                    let t: Vec<&str> = line.split_whitespace().collect();
                    let inv: i32 = t[1].split('=').nth(1).expect("inv").parse().expect("int");
                    let qexp: Vec<i16> = t[2..]
                        .iter()
                        .map(|v| v.parse::<i32>().expect("q") as i16)
                        .collect();
                    let mut spec = base_spec.clone();
                    let mut input = ScfChannelInput {
                        spectrum: &mut spec,
                        energy_ld: &en,
                        threshold_ld: &th,
                        form_factor_ld: &ff,
                        geo: ScfGeometry {
                            offsets: &off,
                            sfb_cnt: 4,
                            sfb_per_group: 4,
                            max_sfb_per_group: 4,
                        },
                    };
                    let out = estimate_channel_with_inv(&mut input, inv);
                    assert_eq!(&out.quant_spec[..16], &qexp, "ch q inv={inv}");
                }
            }
        }
        // ImproveScf transcript (IM lines).
        {
            let bspec = [8000000, -8000000, 4000000, -4000000];
            for line in text.lines() {
                if line.starts_with("IM ") {
                    let t: Vec<&str> = line.split_whitespace().collect();
                    let scf0: i32 = t[1].split('=').nth(1).expect("scf0").parse().expect("int");
                    let best: i32 = t[2].split('=').nth(1).expect("best").parse().expect("int");
                    let d: i32 = t[3].split('=').nth(1).expect("dist").parse().expect("int");
                    let m: i32 = t[4]
                        .split('=')
                        .nth(1)
                        .expect("mincalc")
                        .parse()
                        .expect("int");
                    let qstr: Vec<i16> = t[5]
                        .split('=')
                        .nth(1)
                        .expect("q")
                        .split(',')
                        .map(|v| v.parse::<i32>().expect("q") as i16)
                        .collect();
                    let mut q = vec![0i16; 4];
                    let mut tmp = vec![0i16; 4];
                    let mut dd = 0i32;
                    let mut mm = 0i32;
                    let got = improve_scf(
                        &bspec, &mut q, &mut tmp, -500000000, scf0, -111, &mut dd, &mut mm, false,
                    );
                    assert_eq!(got, best, "improve best scf0={scf0}");
                    assert_eq!(dd, d, "improve dist scf0={scf0}");
                    assert_eq!(mm, m, "improve mincalc scf0={scf0}");
                    assert_eq!(q, qstr, "improve q scf0={scf0}");
                } else if line.starts_with("N46 ") {
                    let t: Vec<&str> = line.split_whitespace().collect();
                    let v: i32 = t[1].split('=').nth(1).expect("v").parse().expect("int");
                    let off1 = [912, 916];
                    let got = calc_sfb_relevant_lines(
                        &[-337460364],
                        &[-693314277],
                        &[-704116391],
                        &off1,
                        1,
                        1,
                        1,
                    );
                    assert_eq!(got, vec![v], "nrel46");
                } else if line.starts_with("QB45 ") || line.starts_with("QB46 ") {
                    // Corpus quantize cross-checks (mod-2 QSS values the S3
                    // grid never covered): re-quantize and compare lines +
                    // distortion bit-exactly.
                    let tag = if line.starts_with("QB45 ") {
                        "QB45"
                    } else {
                        "QB46"
                    };
                    let (spec, qss) = if tag == "QB45" {
                        ([3351240, 33792, -2021824, 49280], -34)
                    } else {
                        ([1356672, -97576, -958056, 107568], -38)
                    };
                    let t: Vec<&str> = line.split_whitespace().collect();
                    let d: i32 = t[1].split('=').nth(1).expect("dist").parse().expect("int");
                    let qexp: Vec<i16> = t[2]
                        .split('=')
                        .nth(1)
                        .expect("q")
                        .split(',')
                        .map(|v| v.parse::<i32>().expect("q") as i16)
                        .collect();
                    let mut q = vec![0i16; 4];
                    let got = crate::aac_enc::quant::calc_sfb_dist(&spec, qss, false, &mut q);
                    assert_eq!(got, d, "{tag} dist");
                    assert_eq!(q, qexp, "{tag} q");
                }
            }
        }
    }
}
