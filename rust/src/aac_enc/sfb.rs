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
//! Scale-factor-band configuration for AAC-LC.
//!
//! Float-native port of the LC-relevant parts of
//! `FDKaacEnc_InitPsyConfiguration`
//! (`cpp/libAACenc/src/psy_configuration.cpp:534-627`): SFB table selection
//! per sample rate, band offsets, active-band and lowpass computation,
//! clip energy, PCM quantization floors, and Bark values. SFB width tables
//! come from the script-extracted `sfb_tables.rs` (verbatim ROM data).
//!
//! Numerical conventions: Q31 constants map to `f64` by division
//! (`0x02940a10 / 2^31` etc., exact for 32-bit patterns); integer
//! constructions (offsets, lowpass lines, active counts, the `allowIS`
//! integer-division rule) use the same integer arithmetic as C and match
//! bit-exactly. Spreading factors are tabulated verbatim from oracle dumps
//! (`SPREAD_*` in `sfb_tables.rs`): the C `fixp_atan`/`fPow` approximations
//! they embed do not reproduce in float within useful tolerance (measured
//! worst factor deviation 13%), while for the VBR-5 target the factors
//! depend only on (rate, long/short) — the sole bitrate branch (<= 20000)
//! never fires. Bark values stay available for M4 `minSnr` work.
//!
//! NOT computed here: `sfbMinSnrLdData` (its consumer is M4 rate control;
//! the float port with exact exponent bookkeeping lands with M4, validated
//! in context; grouping takes min-SNR vectors as input until then).

use super::config::FRAME_LENGTH;
use super::sfb_tables::{SFB_TABLES, SPREAD_LONG, SPREAD_SHORT};
use core::fmt;

/// Long (1024-line) or short (128-line) band configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// Long/start/stop blocks: full 1024-line SFB table.
    Long,
    /// Short blocks: 128-line SFB table.
    Short,
}

/// SFB configuration errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfbError {
    /// No SFB table for this sample rate (`initSfbTable` rejects it).
    UnsupportedSampleRate(u32),
}

impl fmt::Display for SfbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SfbError::UnsupportedSampleRate(rate) => {
                write!(f, "no SFB table for sample rate: {rate}")
            }
        }
    }
}

impl std::error::Error for SfbError {}

/// Maximum scale-factor bands (long) plus terminator.
pub const MAX_OFFSETS: usize = 52;

/// Clip energy base as stored (`0x773593ff`, `psy_configuration.cpp:596`).
pub const CLIP_ENERGY_Q31: i32 = 0x773593ff;
/// PCM quantization noise density as stored (`0x00547062`,
/// `psy_configuration.cpp:329`).
pub const PCM_QUANT_NOISE_Q31: i32 = 0x00547062;
/// Threshold ratio: C_RATIO without the x16 headroom shift, i.e.
/// `0x02940a10 / 2^35 = 10^(-29/10)` up to the float-literal rounding the
/// Q31 constant was built from (`psy_configuration.h:117-118`).
#[cfg(test)]
pub const THRESHOLD_RATIO: f64 = 0x02940a10u32 as f64 / 34_359_738_368.0;
/// LFE lowpass line for long blocks (`psy_const.h:165`).
pub const LFE_LOWPASS_LINE: usize = 12;
/// Threshold-Ld floor applied to grouped/long outputs (`-33/64`,
/// `psy_main.cpp:1071-1075,1124-1128`).
pub const THRESHOLD_LD_FLOOR: f64 = -0.515625;
/// Zero-energy Ld sentinel (`fLog2(0)`, `fixpoint_math.h:876-885`).
pub const LD_ZERO: f64 = -1.0;

/// log2-domain energy: `Ld(x) = log2(x)/64`, `-1.0` for non-positive input
/// (mirrors `fLog2`, `fixpoint_math.h:876-885`).
pub fn ld_data(value: f64) -> f64 {
    if value <= 0.0 {
        LD_ZERO
    } else {
        value.log2() / 64.0
    }
}

/// Per-rate, per-block-kind psychoacoustic band configuration.
#[derive(Debug, Clone)]
pub struct SfbConfig {
    sample_rate: u32,
    kind: BlockKind,
    /// Transform lines covered (1024 long, 128 short).
    lines: usize,
    sfb_count: usize,
    sfb_active: usize,
    sfb_active_lfe: usize,
    lowpass_line: usize,
    lowpass_line_lfe: usize,
    /// Band start offsets, `sfb_count + 1` valid entries.
    offsets: [u16; MAX_OFFSETS],
    /// Clip energy for this block kind (short is `>>= 6` on the integer).
    clip_energy: f64,
    /// Clip energy as stored (exact integer for the threshold chain).
    clip_energy_q31: i32,
    /// PCM quantization floor per band.
    pcm_quant: Vec<f64>,
    /// PCM quantization floor per band as stored (exact integers).
    pcm_quant_q31: Vec<i32>,
    /// Bark value per band (for spreading factors).
    bark: Vec<f64>,
    /// Spreading factors as stored (exact integers): threshold high/low,
    /// spread-energy high/low.
    mask_hi_q31: Vec<i32>,
    mask_lo_q31: Vec<i32>,
    mask_hi_spr_q31: Vec<i32>,
    mask_lo_spr_q31: Vec<i32>,
    /// Spreading factors (threshold domain): high/low.
    mask_hi: Vec<f64>,
    mask_lo: Vec<f64>,
    /// Spreading factors (spread-energy domain): high/low.
    mask_hi_spr: Vec<f64>,
    mask_lo_spr: Vec<f64>,
    allow_is: bool,
    allow_ms: bool,
}

impl SfbConfig {
    /// Build the configuration for `sample_rate`, `kind`, per-channel
    /// `bitrate`, and `bandwidth`, mirroring `FDKaacEnc_InitPsyConfiguration`
    /// for `granuleLength = 1024` (LC only).
    pub fn new(
        sample_rate: u32,
        kind: BlockKind,
        channel_bitrate: u32,
        bandwidth: u32,
        use_is: bool,
        use_ms: bool,
    ) -> Result<Self, SfbError> {
        let table = SFB_TABLES
            .iter()
            .find(|t| t.sample_rate == sample_rate)
            .ok_or(SfbError::UnsupportedSampleRate(sample_rate))?;
        // Effective bandwidth mirrors DetermineBandWidth (bandwidth.cpp:357):
        // min(table value, sampleRate/2). Matters below ~38.6 kHz.
        let bandwidth = bandwidth.min(sample_rate / 2);
        let (widths, lines) = match kind {
            BlockKind::Long => (table.long, FRAME_LENGTH),
            BlockKind::Short => (table.short, FRAME_LENGTH / 8),
        };
        // Offsets with the C early-break + clamp (initSfbTable:245-258).
        let mut offsets = [0u16; MAX_OFFSETS];
        let mut start = 0usize;
        let mut cnt = widths.len();
        for (i, &w) in widths.iter().enumerate() {
            offsets[i] = start as u16;
            start += w as usize;
            if start >= lines {
                cnt = i + 1;
                break;
            }
        }
        cnt = cnt.min(widths.len());
        offsets[cnt] = start.min(lines) as u16;

        let lowpass_line = lowpass_line(bandwidth, lines, sample_rate);
        let lowpass_line_lfe = match kind {
            BlockKind::Long => LFE_LOWPASS_LINE,
            BlockKind::Short => 0,
        };
        let mut active = 1usize;
        for sfb in 0..cnt {
            if offsets[sfb] as usize >= lowpass_line {
                break;
            }
            active = sfb + 1;
        }
        active = active.max(1);
        let mut active_lfe = 0usize;
        for sfb in 0..cnt {
            if offsets[sfb] as usize >= lowpass_line_lfe {
                break;
            }
            active_lfe = sfb + 1;
        }
        let sfb_active = active.max(active_lfe);

        let clip_energy_q31 = match kind {
            // C shifts the stored integer (clipEnergy >>= 6).
            BlockKind::Long => CLIP_ENERGY_Q31,
            BlockKind::Short => CLIP_ENERGY_Q31 >> 6,
        };
        let clip_energy = clip_energy_q31 as f64 / 2_147_483_648.0;
        let pcm_quant_q31: Vec<i32> = (0..cnt)
            .map(|i| (offsets[i + 1] - offsets[i]) as i32 * PCM_QUANT_NOISE_Q31)
            .collect();
        let pcm_quant: Vec<f64> = pcm_quant_q31
            .iter()
            .map(|&q| q as f64 / 2_147_483_648.0)
            .collect();
        let bark: Vec<f64> = (0..cnt)
            .map(|i| {
                let v1 = bark_of_line(offsets[i] as u32, sample_rate, lines);
                let v2 = bark_of_line(offsets[i + 1] as u32, sample_rate, lines);
                ((v1 + v2) / 2.0).min(24.0)
            })
            .collect();
        let spread = match kind {
            BlockKind::Long => SPREAD_LONG.iter().find(|t| t.sample_rate == sample_rate),
            BlockKind::Short => SPREAD_SHORT.iter().find(|t| t.sample_rate == sample_rate),
        }
        .expect("spreading table follows SFB table");
        assert_eq!(spread.hi.len(), cnt, "spreading table length");
        let to_f64 = |v: &[i32]| {
            v.iter()
                .map(|&q| q as f64 / 2_147_483_648.0)
                .collect::<Vec<_>>()
        };
        let mask_hi_q31 = spread.hi.to_vec();
        let mask_lo_q31 = spread.lo.to_vec();
        let mask_hi_spr_q31 = spread.hi_spr.to_vec();
        let mask_lo_spr_q31 = spread.lo_spr.to_vec();
        let mask_hi = to_f64(spread.hi);
        let mask_lo = to_f64(spread.lo);
        let mask_hi_spr = to_f64(spread.hi_spr);
        let mask_lo_spr = to_f64(spread.lo_spr);

        // C integer division semantics (psy_configuration.cpp:565).
        let allow_is = use_is && (channel_bitrate / bandwidth.max(1) < 5);
        let allow_ms = use_ms;

        Ok(Self {
            sample_rate,
            kind,
            lines,
            sfb_count: cnt,
            sfb_active,
            sfb_active_lfe: active_lfe,
            lowpass_line,
            lowpass_line_lfe,
            offsets,
            clip_energy,
            clip_energy_q31,
            pcm_quant,
            pcm_quant_q31,
            mask_hi_q31,
            mask_lo_q31,
            mask_hi_spr_q31,
            mask_lo_spr_q31,
            mask_hi,
            mask_lo,
            mask_hi_spr,
            mask_lo_spr,
            bark,
            allow_is,
            allow_ms,
        })
    }

    /// Sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Block kind.
    pub fn kind(&self) -> BlockKind {
        self.kind
    }

    /// Transform lines covered.
    pub fn lines(&self) -> usize {
        self.lines
    }

    /// Total scale-factor bands.
    pub fn sfb_count(&self) -> usize {
        self.sfb_count
    }

    /// Active bands (up to lowpass).
    pub fn sfb_active(&self) -> usize {
        self.sfb_active
    }

    /// Active bands for LFE.
    pub fn sfb_active_lfe(&self) -> usize {
        self.sfb_active_lfe
    }

    /// Lowpass line (bins at/above are zeroed).
    pub fn lowpass_line(&self) -> usize {
        self.lowpass_line
    }

    /// LFE lowpass line.
    pub fn lowpass_line_lfe(&self) -> usize {
        self.lowpass_line_lfe
    }

    /// Band start offsets (`sfb_count + 1` entries, last = line count).
    pub fn offsets(&self) -> &[u16] {
        &self.offsets[..self.sfb_count + 1]
    }

    /// Clip energy for this block kind.
    pub fn clip_energy(&self) -> f64 {
        self.clip_energy
    }

    /// Clip energy as stored (exact integer).
    pub fn clip_energy_q31(&self) -> i32 {
        self.clip_energy_q31
    }

    /// PCM quantization floor per band.
    pub fn pcm_quant(&self) -> &[f64] {
        &self.pcm_quant
    }

    /// PCM quantization floor per band as stored (exact integers).
    pub fn pcm_quant_q31(&self) -> &[i32] {
        &self.pcm_quant_q31
    }

    /// Bark value per band.
    pub fn bark(&self) -> &[f64] {
        &self.bark
    }

    /// Threshold spreading factors (high, low).
    pub fn mask_factors(&self) -> (&[f64], &[f64]) {
        (&self.mask_hi, &self.mask_lo)
    }

    /// Threshold spreading factors as stored (exact integers).
    pub fn mask_factors_q31(&self) -> (&[i32], &[i32]) {
        (&self.mask_hi_q31, &self.mask_lo_q31)
    }

    /// Spread-energy spreading factors (high, low).
    pub fn mask_factors_spr(&self) -> (&[f64], &[f64]) {
        (&self.mask_hi_spr, &self.mask_lo_spr)
    }

    /// Spread-energy spreading factors as stored (exact integers).
    pub fn mask_factors_spr_q31(&self) -> (&[i32], &[i32]) {
        (&self.mask_hi_spr_q31, &self.mask_lo_spr_q31)
    }

    /// Intensity stereo allowed.
    pub fn allow_is(&self) -> bool {
        self.allow_is
    }

    /// Mid/side stereo allowed.
    pub fn allow_ms(&self) -> bool {
        self.allow_ms
    }
}

fn lowpass_line(bandwidth: u32, lines: usize, sample_rate: u32) -> usize {
    // Exact integer arithmetic from InitPsyConfiguration: (2*bw*N)/sr.
    ((2u64 * bandwidth as u64 * lines as u64) / sample_rate as u64) as usize
}

/// Bark value of one transform line (`BarcLineValue`, LC paths only:
/// `center = line * Fs / (2 * N)`, `bark = 13.3*atan(0.00076*c) +
/// 3.5*atan((4/3e-4)*c)^2`; `psy_configuration.cpp:270-317`).
///
/// Kept for M4 `minSnr` work (which consumes Bark widths); spreading
/// factors themselves are tabulated (see below) because the C
/// `fixp_atan`/`fPow` approximations they embed do not reproduce in float
/// within useful tolerance.
fn bark_of_line(line: u32, sample_rate: u32, lines: usize) -> f64 {
    let center = line as f64 * sample_rate as f64 / (2 * lines) as f64;
    let x1 = center * (4.0 / 3.0) * 1e-4;
    let x2 = center * 0.00076;
    13.3 * x2.atan() + 3.5 * x1.atan().powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::testdata;

    fn cfg_long_48k() -> SfbConfig {
        SfbConfig::new(48000, BlockKind::Long, 96000, 19293, true, true).unwrap()
    }

    #[test]
    fn table_selection_and_offsets_match_init_sfb_table() {
        // All 12 rates: counts from the ROM headers, offsets cumulative with
        // the early-break clamp, last offset == line count.
        for rate in [
            8000u32, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000,
        ] {
            for kind in [BlockKind::Long, BlockKind::Short] {
                let cfg = SfbConfig::new(rate, kind, 96000, 19293, true, true).unwrap();
                let off = cfg.offsets();
                assert_eq!(off[0], 0, "{rate} {kind:?}");
                assert!(*off.last().unwrap() as usize <= cfg.lines());
                for w in off.windows(2) {
                    assert!(w[0] < w[1], "{rate} {kind:?} offsets not increasing");
                }
            }
        }
        assert!(matches!(
            SfbConfig::new(12345, BlockKind::Long, 96000, 19293, true, true),
            Err(SfbError::UnsupportedSampleRate(12345))
        ));
    }

    #[test]
    fn oracle_config_parity_48k() {
        // Exact integer agreement on counts, lines, offsets, clip energy
        // and PCM floors; spreading factors within float tolerance.
        let ((long_cfg, long_tns), (short_cfg, _)) = testdata::load_cfg(48000, 96000);
        let cfg = cfg_long_48k();
        assert_eq!(cfg.sfb_count(), long_cfg.sfb_cnt);
        assert_eq!(cfg.sfb_active(), long_cfg.sfb_active);
        assert_eq!(cfg.sfb_active_lfe(), long_cfg.sfb_active_lfe);
        assert_eq!(cfg.lowpass_line(), long_cfg.lowpass_line);
        assert_eq!(cfg.lowpass_line_lfe(), long_cfg.lowpass_line_lfe);
        assert_eq!(
            cfg.offsets().iter().map(|&v| v as i32).collect::<Vec<_>>(),
            long_cfg.offsets
        );
        assert_eq!(cfg.clip_energy_q31(), long_cfg.clip_energy);
        for (got, want) in cfg.pcm_quant_q31().iter().zip(long_cfg.pcm_quant.iter()) {
            assert_eq!(got, want);
        }
        assert_eq!(long_tns.active, true);
        assert_eq!(long_tns.max_order, 12);
        assert_eq!(long_tns.coef_res, 4);
        assert_eq!(long_tns.stop_band, 40);
        assert_eq!(long_tns.stop_line, 672);

        let short = SfbConfig::new(48000, BlockKind::Short, 96000, 19293, true, true).unwrap();
        assert_eq!(short.sfb_count(), short_cfg.sfb_cnt);
        assert_eq!(short.sfb_active(), short_cfg.sfb_active);
        assert_eq!(short.lowpass_line(), short_cfg.lowpass_line);
        assert_eq!(
            short
                .offsets()
                .iter()
                .map(|&v| v as i32)
                .collect::<Vec<_>>(),
            short_cfg.offsets
        );
        // Short clip energy is the stored integer >>= 6.
        assert_eq!(short.clip_energy_q31(), short_cfg.clip_energy);

        // Spreading factors: exact stored integers (tabulated because the C
        // fixp_atan/fPow approximations do not reproduce in float; the VBR-5
        // target never takes the <=20000 bit/s branch, so factors depend
        // only on (rate, kind)).
        let (hi_q, lo_q) = cfg.mask_factors_q31();
        let (his_q, los_q) = cfg.mask_factors_spr_q31();
        assert_eq!(hi_q, long_cfg.mask_hi.as_slice());
        assert_eq!(lo_q, long_cfg.mask_lo.as_slice());
        assert_eq!(his_q, long_cfg.mask_hi_spr.as_slice());
        assert_eq!(los_q, long_cfg.mask_lo_spr.as_slice());
        let (shi_q, slo_q) = short.mask_factors_q31();
        assert_eq!(shi_q, short_cfg.mask_hi.as_slice());
        assert_eq!(slo_q, short_cfg.mask_lo.as_slice());
        // allowIS: 96000/19293 = 4 (int) < 5; allowMS passthrough.
        assert!(cfg.allow_is());
        assert!(cfg.allow_ms());
    }

    #[test]
    fn oracle_config_parity_441k_and_mono() {
        let ((long_cfg, long_tns), (short_cfg, _)) = testdata::load_cfg(44100, 96000);
        let cfg = SfbConfig::new(44100, BlockKind::Long, 96000, 19293, true, true).unwrap();
        assert_eq!(cfg.sfb_count(), long_cfg.sfb_cnt);
        assert_eq!(cfg.sfb_active(), long_cfg.sfb_active);
        assert_eq!(cfg.lowpass_line(), long_cfg.lowpass_line);
        assert_eq!(
            cfg.offsets().iter().map(|&v| v as i32).collect::<Vec<_>>(),
            long_cfg.offsets
        );
        assert_eq!(long_tns.stop_band, 42);
        let short = SfbConfig::new(44100, BlockKind::Short, 96000, 19293, true, true).unwrap();
        assert_eq!(short.sfb_count(), short_cfg.sfb_cnt);
        assert_eq!(
            short
                .offsets()
                .iter()
                .map(|&v| v as i32)
                .collect::<Vec<_>>(),
            short_cfg.offsets
        );

        // Mono (112000 bit/s per channel): same tables, different rate class.
        let ((mono_cfg, mono_tns), _) = testdata::load_cfg(48000, 112000);
        let mono = SfbConfig::new(48000, BlockKind::Long, 112000, 19293, true, true).unwrap();
        assert_eq!(mono.sfb_count(), mono_cfg.sfb_cnt);
        assert_eq!(
            mono.offsets().iter().map(|&v| v as i32).collect::<Vec<_>>(),
            mono_cfg.offsets
        );
        assert_eq!(mono_tns.max_order, 12);
        // Mono: 112000/19293 = 5 (int) which is NOT < 5: intensity off.
        assert!(!mono.allow_is());
        assert!(mono.allow_ms());
    }

    #[test]
    fn all_rates_config_parity() {
        // Every init-time config (12 rates x long/short x both VBR-5
        // operating points): exact integer agreement on counts, lines,
        // offsets, clip, PCM floors, and all four spreading-factor sets.
        // This also covers the bandwidth clamp (min(19293, sr/2)) below
        // ~38.6 kHz and the mono allowIS=false rule across rates.
        let cfgs = testdata::load_configs_all();
        assert_eq!(cfgs.len(), 48);
        for c in &cfgs {
            let kind = if c.long {
                BlockKind::Long
            } else {
                BlockKind::Short
            };
            let cfg = SfbConfig::new(c.sample_rate, kind, c.pch, 19293, true, true).unwrap();
            assert_eq!(
                cfg.sfb_count(),
                c.snap.sfb_cnt,
                "{} {} {}",
                c.sample_rate,
                c.pch,
                c.long
            );
            assert_eq!(cfg.sfb_active(), c.snap.sfb_active);
            assert_eq!(cfg.sfb_active_lfe(), c.snap.sfb_active_lfe);
            assert_eq!(cfg.lowpass_line(), c.snap.lowpass_line);
            assert_eq!(cfg.lowpass_line_lfe(), c.snap.lowpass_line_lfe);
            assert_eq!(
                cfg.offsets().iter().map(|&v| v as i32).collect::<Vec<_>>(),
                c.snap.offsets
            );
            assert_eq!(cfg.clip_energy_q31(), c.snap.clip_energy);
            assert_eq!(cfg.pcm_quant_q31(), c.snap.pcm_quant.as_slice());
            let (hi, lo) = cfg.mask_factors_q31();
            let (his, los) = cfg.mask_factors_spr_q31();
            assert_eq!(hi, c.snap.mask_hi.as_slice());
            assert_eq!(lo, c.snap.mask_lo.as_slice());
            assert_eq!(his, c.snap.mask_hi_spr.as_slice());
            assert_eq!(los, c.snap.mask_lo_spr.as_slice());
            // allowIS rule with integer division on the EFFECTIVE bandwidth.
            let bw = 19293.min(c.sample_rate / 2);
            assert_eq!(
                cfg.allow_is(),
                c.pch / bw.max(1) < 5,
                "{} {}",
                c.sample_rate,
                c.pch
            );
            assert!(cfg.allow_ms());
        }
    }

    #[test]
    fn active_band_semantics() {
        // 48k long: lowpass 823 falls inside band 44 (offsets 800..832);
        // sfbActive is the first band starting at/above the line.
        let cfg = cfg_long_48k();
        assert_eq!(cfg.lowpass_line(), 823);
        let off = cfg.offsets();
        assert!(off[cfg.sfb_active()] >= 823);
        assert!(off[cfg.sfb_active() - 1] < 823);
        // Threshold ratio sanity: 10^(-2.9).
        assert!((THRESHOLD_RATIO - 10f64.powf(-2.9)).abs() < 2e-9);
        // ld_data conventions: log2/64 with -1.0 sentinel.
        assert_eq!(ld_data(0.0), -1.0);
        assert_eq!(ld_data(-3.0), -1.0);
        assert!((ld_data(1.0) - 0.0).abs() < 1e-15);
        assert!((ld_data(2.0) - 1.0 / 64.0).abs() < 1e-15);
        assert_eq!(THRESHOLD_LD_FLOOR, -33.0 / 64.0);
    }
}
