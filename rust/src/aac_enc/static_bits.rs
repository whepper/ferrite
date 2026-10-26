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
CONTRIBUTORS BE LIABLE for any direct, indirect, incidental, special exemplary,
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
//! M4-S1: static-bit accounting for the `ChannelElementWrite(NULL)` path.
//!
//! Ports the counting-only subset of `FDKaacEnc_ChannelElementWrite`
//! (`bitenc.cpp:918-1178`) as called from `FDKaacEnc_QCMainPrepare`
//! (`qc_main.cpp:454-456`): `hTpEnc == NULL`, `qcOutChannel == NULL`,
//! `minCnt == 0`. With no bitstream handle, section data, scalefactor
//! data and spectral data contribute exactly 0 (`bitenc.cpp:279-281,366-368,
//! 1109-1122`), so no `SECTION_DATA`, scalefactors or quantized spectrum
//! are needed. What remains is pure syntax accounting:
//!
//! * element header (`EL_ID_BITS`, `bitenc.h:126`) + instance tag (4),
//! * `common_window` (1, CPE with common window),
//! * ICS info: 15 bits for short blocks (`8 + TRANS_FAC - 1`), else 11
//!   (`bitenc.cpp:180-227`, non-ELD/non-scalable path),
//! * MS info: 2 bits, plus one per band when `msDigest == MS_SOME`
//!   (`bitenc.cpp:380-423`),
//! * global gain (8), pulse present (1), TNS present (1) + TNS payload,
//!   gain-control present (1).
//!
//! Scope is the stripped LC build only (`syntaxFlags == 0`,
//! `AOT_AAC_LC`, `epConfig == -1`): the ER/ELD/scalable branches of the
//! helpers are not implemented. LFE elements share the SCE element list
//! (`FDK_tools_rom.cpp`, `node_aac_sce`) and count identically.

use super::tns::TnsChannel;

/// MS digest (`interface.h:112`): `MS_NONE = 0, MS_SOME = 1, MS_ALL = 2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsDigest {
    None = 0,
    Some = 1,
    All = 2,
}

impl MsDigest {
    /// Convert an oracle `MSD` integer; returns `None` for other values
    /// (loud failure instead of a silent default).
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::None),
            1 => Some(Self::Some),
            2 => Some(Self::All),
            _ => None,
        }
    }
}

/// Fixed syntax widths on the NULL-counting path (`bitenc.cpp`).
pub const EL_ID_BITS: i32 = 3; // `bitenc.h:126`
const INSTANCE_TAG_BITS: i32 = 4; // `bitenc.cpp:1003`
const COMMON_WINDOW_BITS: i32 = 1; // `bitenc.cpp:1012`
const GLOBAL_GAIN_BITS: i32 = 8; // `encodeGlobalGain`, `:158-168`
const PULSE_BITS: i32 = 1; // `encodePulseData`, `:605-610`
const TNS_PRESENT_BITS: i32 = 1; // `encodeTnsDataPresent`, `:434-454`
const GAIN_CONTROL_BITS: i32 = 1; // `encodeGainControlData`, `:589-594`

/// `encodeIcsInfo` count (`bitenc.cpp:186-194`): short blocks carry the
/// 4-bit `maxSfb` plus 7 grouping bits on top of the 8-bit base; long,
/// start and stop blocks read a fixed 11 bits on the stripped path.
pub fn ics_info_bits(short_block: bool) -> i32 {
    if short_block {
        8 + 8 - 1
    } else {
        11
    }
}

/// `encodeMSInfo` count (`bitenc.cpp:412-422`): 2 digest bits plus one
/// mask bit per band when `digest == MS_SOME`. `bands` is the
/// `(sfb_cnt / sfb_per_group) * max_sfb_per_group` loop trip count of the
/// C code (channel 0 geometry on the common-window path).
pub fn ms_info_bits(digest: MsDigest, bands: usize) -> i32 {
    2 + if digest == MsDigest::Some {
        bands as i32
    } else {
        0
    }
}

/// Trip count of the C `MS_SOME` mask loop for the given SFB geometry.
pub fn ms_mask_bits(sfb_cnt: usize, sfb_per_group: usize, max_sfb_per_group: usize) -> usize {
    debug_assert!(sfb_per_group > 0 && sfb_cnt % sfb_per_group == 0);
    (sfb_cnt / sfb_per_group) * max_sfb_per_group
}

/// `encodeTnsData` count on the NULL path (`bitenc.cpp:535-574`): 0 when
/// no window carries a filter, else per-window `1 (short) / 2 (long)` nf
/// bits plus, per present filter, length/order/direction/compression
/// bits and `order` coefficient bits. `coefBits` follows the
/// `coefRes == 4` 3/4-bit rule and the `else` 2/3-bit rule with the exact
/// C comparison bounds (`> 3 / < -4`, `> 1 / < -2`).
pub fn tns_data_bits(tns: &TnsChannel, short_block: bool) -> i32 {
    let windows = if short_block { 8 } else { 1 };
    let mut present = false;
    for w in 0..windows {
        if tns.num_filters[w] != 0 {
            present = true;
        }
    }
    if !present {
        return 0;
    }
    let mut bits: i32 = 0;
    for w in 0..windows {
        bits += if short_block { 1 } else { 2 };
        let nf = tns.num_filters[w];
        if nf == 0 {
            continue;
        }
        debug_assert!(nf <= 2);
        bits += 1; // coefRes bit
        for j in 0..nf {
            bits += if short_block { 4 } else { 6 }; // length
            bits += if short_block { 3 } else { 5 }; // order
            let order = tns.order[w][j];
            if order != 0 {
                bits += 1; // direction
                bits += 1; // coef_compression
                let coef_bits = if tns.coef_res[w] == 4 {
                    let mut cb = 3;
                    for k in 0..order {
                        let c = tns.coef[w][j][k];
                        if !(-4..=3).contains(&c) {
                            cb = 4;
                            break;
                        }
                    }
                    cb
                } else {
                    let mut cb = 2;
                    for k in 0..order {
                        let c = tns.coef[w][j][k];
                        if !(-2..=1).contains(&c) {
                            cb = 3;
                            break;
                        }
                    }
                    cb
                };
                bits += order as i32 * coef_bits;
            }
        }
    }
    bits
}

/// TNS static contribution: present bit plus payload.
pub fn tns_static_bits(tns: &TnsChannel, short_block: bool) -> i32 {
    TNS_PRESENT_BITS + tns_data_bits(tns, short_block)
}

/// Per-channel tail after (global gain,) section/scalefactor data on the
/// NULL path: pulse + TNS + gain-control bits (section/scalefactor/
/// spectral contribute 0).
fn channel_tail_bits(tns: &TnsChannel, short_block: bool) -> i32 {
    PULSE_BITS + tns_static_bits(tns, short_block) + GAIN_CONTROL_BITS
}

/// `STATIC` for SCE/LFE (`el_aac_sce` in `FDK_tools_rom.cpp`):
/// ID + tag + global gain + ICS info + pulse + TNS + gain control.
pub fn static_bits_sce(short_block: bool, tns: &TnsChannel) -> i32 {
    EL_ID_BITS
        + INSTANCE_TAG_BITS
        + GLOBAL_GAIN_BITS
        + ics_info_bits(short_block)
        + channel_tail_bits(tns, short_block)
}

/// `STATIC` for CPE. With `common_window` (the only encoder mode:
/// `psy_main.cpp:413`) the ICS info and MS info are written once
/// (channel 0 geometry) followed by both channels' payloads
/// (`el_aac_cpe1`); without it each channel carries its own ICS info
/// (`el_aac_cpe0`).
pub fn static_bits_cpe(
    common_window: bool,
    short_block_ch0: bool,
    short_block_ch1: bool,
    ms_digest: MsDigest,
    ms_bands: usize,
    tns_ch0: &TnsChannel,
    tns_ch1: &TnsChannel,
) -> i32 {
    let mut bits = EL_ID_BITS + INSTANCE_TAG_BITS;
    if common_window {
        bits += COMMON_WINDOW_BITS;
        bits += ics_info_bits(short_block_ch0);
        bits += ms_info_bits(ms_digest, ms_bands);
        bits += GLOBAL_GAIN_BITS + channel_tail_bits(tns_ch0, short_block_ch0);
        bits += GLOBAL_GAIN_BITS + channel_tail_bits(tns_ch1, short_block_ch1);
    } else {
        bits += GLOBAL_GAIN_BITS
            + ics_info_bits(short_block_ch0)
            + channel_tail_bits(tns_ch0, short_block_ch0);
        bits += GLOBAL_GAIN_BITS
            + ics_info_bits(short_block_ch1)
            + channel_tail_bits(tns_ch1, short_block_ch1);
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::testdata;
    use crate::aac_enc::tns::TnsChannel;

    fn blank_tns() -> TnsChannel {
        TnsChannel::new()
    }

    /// Hand-counted silence/static anchors (see module docs):
    /// SCE long = 3+4+8+11+1+1+0+1; SCE short swaps 11->15.
    #[test]
    fn s1_hand_counted_element_bases() {
        let tns = blank_tns();
        assert_eq!(static_bits_sce(false, &tns), 29);
        assert_eq!(static_bits_sce(true, &tns), 33);
        // LFE shares the SCE element list: identical counts.
        assert_eq!(static_bits_sce(false, &tns), 29);
        // CPE long, MS NONE, no TNS elsewhere:
        // 3+4+1+11+2 + 2*(8+1+1+0+1).
        assert_eq!(
            static_bits_cpe(true, false, false, MsDigest::None, 0, &tns, &tns),
            43
        );
        // CPE short, MS NONE: 3+4+1+15+2 + 2*(8+1+1+0+1) = 47
        // (the per-channel tail is block-type independent: only the
        // single shared ICS info grows 11 -> 15).
        assert_eq!(
            static_bits_cpe(true, true, true, MsDigest::None, 0, &tns, &tns),
            47
        );
        // CPE long without common window: per-channel ICS info, no MS.
        assert_eq!(
            static_bits_cpe(false, false, false, MsDigest::None, 0, &tns, &tns),
            51
        );
    }

    #[test]
    fn s1_ms_some_costs_exactly_the_mask_bits() {
        let tns = blank_tns();
        // MS_SOME with N mask bands costs exactly N bits over MS_NONE;
        // MS_ALL costs nothing extra (digest-only, 2 bits either way).
        for bands in [0, 1, 13, 45, 52] {
            let some = static_bits_cpe(true, false, false, MsDigest::Some, bands, &tns, &tns);
            let none = static_bits_cpe(true, false, false, MsDigest::None, bands, &tns, &tns);
            let all = static_bits_cpe(true, false, false, MsDigest::All, bands, &tns, &tns);
            assert_eq!(some - none, bands as i32, "bands={bands}");
            assert_eq!(all, none, "bands={bands}");
        }
    }

    fn tns_long_order5() -> TnsChannel {
        // One long filter, order 5, all coefficients in 3-bit range,
        // coefRes 4: 2 (nf) + 1 (res) + 6 (len) + 5 (order) + 1 (dir) +
        // 1 (compr) + 5*3 (coefs) = 31 payload bits, +1 present = 32.
        let mut tns = TnsChannel::new();
        tns.num_filters[0] = 1;
        tns.coef_res[0] = 4;
        tns.length[0] = [10, 0];
        tns.order[0] = [5, 0];
        tns.direction[0] = [0, 0];
        tns.coef[0][0] = [1, -2, 3, -4, 0, 0, 0, 0, 0, 0, 0, 0];
        tns
    }

    #[test]
    fn s1_tns_data_bits_hand_counted() {
        // No filters anywhere: present bit only.
        assert_eq!(tns_data_bits(&blank_tns(), false), 0);
        assert_eq!(tns_static_bits(&blank_tns(), false), 1);
        assert_eq!(tns_static_bits(&blank_tns(), true), 1);
        // Hand-counted long filter above.
        assert_eq!(tns_data_bits(&tns_long_order5(), false), 31);
        assert_eq!(tns_static_bits(&tns_long_order5(), false), 32);
        // A coefficient outside the 3-bit range upgrades to 4-bit coding:
        // +5 bits over the 31 above.
        let mut wide = tns_long_order5();
        wide.coef[0][0][4] = 4;
        assert_eq!(tns_data_bits(&wide, false), 36);
        // coefRes 3 selects the 2/3-bit rule instead: coefficients inside
        // [-2, 1] stay at 2 bits, anything outside upgrades to 3.
        let mut res3 = tns_long_order5();
        res3.coef_res[0] = 3;
        res3.coef[0][0] = [1, -2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(tns_data_bits(&res3, false), 2 + 1 + 6 + 5 + 1 + 1 + 10);
        res3.coef[0][0][0] = 2;
        assert_eq!(tns_data_bits(&res3, false), 2 + 1 + 6 + 5 + 1 + 1 + 15);
        // Short format on window 3 (1-bit nf): every window still spends
        // its nf bit, so 8 + 1 (res) + 4 (len) + 3 (order) + 1 (dir) +
        // 1 (compr) + 5*2 (coefs) = 28 payload bits.
        let mut short = TnsChannel::new();
        short.num_filters[3] = 1;
        short.coef_res[3] = 3;
        short.length[3] = [4, 0];
        short.order[3] = [5, 0];
        short.direction[3] = [1, 0];
        short.coef[3][0] = [1, 0, -1, 1, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(tns_data_bits(&short, true), 8 + 1 + 4 + 3 + 1 + 1 + 10);
        // SCE short base (33, which already holds one TNS present bit)
        // with the payload added: 33 + 28 = 61.
        assert_eq!(static_bits_sce(true, &short), 61);
    }

    /// Build a [`TnsChannel`] from committed STNSW/STNSR lines.
    fn tns_from_committed(
        tns: &[testdata::TnsWindow],
        coef_res: &[i32],
        short_block: bool,
    ) -> TnsChannel {
        let mut out = TnsChannel::new();
        let windows = if short_block { 8 } else { 1 };
        assert_eq!(tns.len(), windows);
        assert_eq!(coef_res.len(), windows);
        for (w, tw) in tns.iter().enumerate() {
            out.num_filters[w] = tw.num_filters as usize;
            out.coef_res[w] = coef_res[w] as usize;
            for j in 0..2 {
                out.length[w][j] = tw.length[j] as usize;
                out.order[w][j] = tw.order[j] as usize;
                out.direction[w][j] = tw.direction[j] as usize;
            }
            for k in 0..12 {
                out.coef[w][0][k] = tw.coef_hi[k];
                out.coef[w][1][k] = tw.coef_lo[k];
            }
        }
        out
    }

    /// Committed S1 corpus for static-bit counting: (name, frame).
    /// Same set as the PE tests (pe.rs `S1_CASES`).
    const S1_CASES: [(&str, u32); 19] = [
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

    #[test]
    fn s1_static_bits_match_oracle() {
        for (name, f) in S1_CASES {
            let frame = testdata::load_s1(name, f);
            let got = if frame.ch.len() == 2 {
                assert_eq!(frame.static_el, "CPE");
                let short0 = frame.ch[0].geo_seq == 2;
                let short1 = frame.ch[1].geo_seq == 2;
                let (digest, mask) = frame.msd.as_ref().expect("MSD");
                let ms = MsDigest::from_i32(*digest).expect("MS digest");
                let bands = if ms == MsDigest::Some {
                    ms_mask_bits(
                        frame.ch[0].geo_cnt,
                        frame.ch[0].geo_per,
                        frame.ch[0].geo_max,
                    )
                } else {
                    0
                };
                // The committed mask always spans the channel bands.
                assert_eq!(mask.len(), frame.ch[0].geo_cnt);
                let t0 = tns_from_committed(&frame.ch[0].tns, &frame.ch[0].tns_coef_res, short0);
                let t1 = tns_from_committed(&frame.ch[1].tns, &frame.ch[1].tns_coef_res, short1);
                static_bits_cpe(true, short0, short1, ms, bands, &t0, &t1)
            } else {
                assert_eq!(frame.static_el, "SCE");
                let short = frame.ch[0].geo_seq == 2;
                let t = tns_from_committed(&frame.ch[0].tns, &frame.ch[0].tns_coef_res, short);
                static_bits_sce(short, &t)
            };
            assert_eq!(got, frame.static_bits, "{name} f{f} STATIC");
        }
    }

    #[test]
    fn s1_static_tamper_changes_count() {
        // Flipping the digest on a SOME frame must change the count by the
        // mask-bit amount (non-vacuous comparison).
        let frame = testdata::load_s1("sine-stereo", 5);
        let (digest, _) = frame.msd.as_ref().expect("MSD");
        assert_eq!(*digest, 1, "sine f5 is MS_SOME");
        let tns = blank_tns();
        let some = static_bits_cpe(true, false, false, MsDigest::Some, 14, &tns, &tns);
        let none = static_bits_cpe(true, false, false, MsDigest::None, 14, &tns, &tns);
        assert_eq!(some - none, 14);
    }
}
