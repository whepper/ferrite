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
//! Test-only helpers: committed oracle vectors and fixture synthesis.
//!
//! The `testdata/` directory holds reference vectors captured with
//! `tools/enc-oracle` (window decisions for every frame/channel plus
//! selected transform-input and MDCT snapshots). Fixture PCM is
//! resynthesized here with formulae identical to
//! `tools/enc-oracle/make_fixtures.py` (same evaluation order, truncation
//! toward zero); the `synthesis_matches_oracle_input` test anchors that
//! claim against committed ring vectors.

use super::block_switch::MAX_GROUPS;
use std::path::PathBuf;

fn testdata_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/aac_enc/testdata")
        .join(file)
}

fn read_numbers(file: &str) -> Vec<String> {
    let text = std::fs::read_to_string(testdata_path(file)).expect("oracle testdata readable");
    text.split_whitespace().map(str::to_string).collect()
}

/// One committed window-decision row: frame, channel, sequence, shape,
/// group count, and four group lengths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SeqRow {
    pub(crate) frame: u32,
    pub(crate) channel: u32,
    pub(crate) seq: u8,
    pub(crate) shape: u8,
    pub(crate) groups: u8,
    pub(crate) group_len: [u8; MAX_GROUPS],
}

/// Load every window-decision row for a fixture, in file order.
pub(crate) fn load_seq(name: &str) -> Vec<SeqRow> {
    let nums = read_numbers(&format!("seq_{name}.txt"));
    assert!(nums.len() % 9 == 0, "seq file has 9 columns per row");
    nums.chunks_exact(9)
        .map(|r| {
            let n = |i: usize| r[i].parse::<u32>().expect("integer seq row");
            SeqRow {
                frame: n(0),
                channel: n(1),
                seq: n(2) as u8,
                shape: n(3) as u8,
                groups: n(4) as u8,
                group_len: [n(5) as u8, n(6) as u8, n(7) as u8, n(8) as u8],
            }
        })
        .collect()
}

/// Load one committed MDCT vector: exponent plus 1024 Q31 coefficients.
pub(crate) fn load_mdct(name: &str, frame: u32, channel: u32) -> (i32, Vec<i32>) {
    let nums = read_numbers(&format!("mdct_{name}_f{frame}_ch{channel}.txt"));
    assert_eq!(nums.len(), 1025, "exponent plus 1024 coefficients");
    let e = nums[0].parse::<i32>().expect("integer exponent");
    let coeffs = nums[1..]
        .iter()
        .map(|s| s.parse::<i32>().expect("integer coeff"))
        .collect();
    (e, coeffs)
}

/// Load one committed 2048-sample transform-input ring.
pub(crate) fn load_ring(name: &str, frame: u32, channel: u32) -> Vec<i16> {
    read_numbers(&format!("ring_{name}_f{frame}_ch{channel}.txt"))
        .iter()
        .map(|s| s.parse::<i16>().expect("integer sample"))
        .collect()
}

/// Q31 fixed-point value with block exponent to real number:
/// `real = int * 2^(E-31)`. This reading convention is confirmed by the
/// MDCT parity test, which measures a constant per-bin Rust/reference ratio
/// of 1.0 across the corpus: any convention error would show up as a
/// power-of-two deviation from 1.0.
pub(crate) fn q31_to_f64(value: i32, exponent: i32) -> f64 {
    value as f64 * 2f64.powi(exponent - 31)
}

fn sine_at(t: usize, amp: f64) -> i16 {
    sine_at_sr(t, 48000.0, 1000.0, amp)
}

fn sine_at_sr(t: usize, sample_rate: f64, freq: f64, amp: f64) -> i16 {
    // Same evaluation order as make_fixtures.py: t = i / SR, then
    // amp * sin(2 * pi * f * t); Python int() and Rust `as` both truncate
    // toward zero.
    sine_f(t, sample_rate, freq, amp) as i16
}

fn sine_f(t: usize, sample_rate: f64, freq: f64, amp: f64) -> f64 {
    let time = t as f64 / sample_rate;
    amp * (2.0 * core::f64::consts::PI * freq * time).sin()
}

fn splitmix32(state: &mut u32) -> u32 {
    // Bit-exact port of the python reference generator (all ops wrap).
    *state = state.wrapping_add(0x9E3779B9);
    let mut z = *state;
    z = (z ^ (z >> 16)).wrapping_mul(0x21F0AAAD);
    z = (z ^ (z >> 15)).wrapping_mul(0x735A2D97);
    z ^= z >> 15;
    z
}

fn noise_vec(len: usize) -> Vec<f64> {
    let mut st = 0x12345678u32;
    (0..len)
        .map(|_| (splitmix32(&mut st) % 16001) as f64 - 8000.0)
        .collect()
}

/// Sample rate of a fixture in Hz.
pub(crate) fn fixture_sample_rate(name: &str) -> u32 {
    if name.ends_with("-44100") {
        44100
    } else {
        48000
    }
}

/// Resynthesize one oracle fixture: per-channel sample vectors.
/// Formulae (amplitudes, phases, edges) are identical to
/// `tools/enc-oracle/make_fixtures.py`.
pub(crate) fn fixture_samples(name: &str) -> Vec<Vec<i16>> {
    match name {
        "onset-stereo" => (0..2)
            .map(|_| {
                (0..48000)
                    .map(|t| if t < 24000 { 0 } else { sine_at(t, 10_000.0) })
                    .collect()
            })
            .collect(),
        "stepdown-stereo" => (0..2)
            .map(|_| {
                (0..48000)
                    .map(|t| if t < 24000 { sine_at(t, 10_000.0) } else { 0 })
                    .collect()
            })
            .collect(),
        "click-mono" => {
            vec![(0..24000)
                .map(|t| {
                    if t == 12000 {
                        30_000
                    } else {
                        sine_at(t, 8_000.0)
                    }
                })
                .collect()]
        }
        "edge-mono" => {
            vec![(0..12000)
                .map(|t| if t == 3 * 1024 + 900 { 30_000 } else { 0 })
                .collect()]
        }
        "sine-stereo" => (0..2)
            .map(|_| (0..48000).map(|t| sine_at(t, 10_000.0)).collect())
            .collect(),
        "impulse-mono" => vec![(0..9600)
            .map(|t| {
                let time = t as f64 / 48000.0;
                if (time - 0.1).abs() < 1.0 / 96000.0 {
                    30_000
                } else {
                    0
                }
            })
            .collect()],
        "silence-stereo" => vec![vec![0i16; 48000], vec![0i16; 48000]],
        "multitone-stereo" => (0..2)
            .map(|_| {
                (0..48000)
                    .map(|t| {
                        (sine_f(t, 48000.0, 200.0, 6000.0)
                            + sine_f(t, 48000.0, 2000.0, 4000.0)
                            + sine_f(t, 48000.0, 8000.0, 2000.0)) as i16
                    })
                    .collect()
            })
            .collect(),
        "sweep-stereo" => (0..2)
            .map(|_| {
                (0..48000)
                    .map(|t| {
                        let time = t as f64 / 48000.0;
                        let phase = 2.0
                            * core::f64::consts::PI
                            * (100.0 * time + (15000.0 - 100.0) * time * time / 2.0);
                        (8000.0 * phase.sin()) as i16
                    })
                    .collect()
            })
            .collect(),
        "noise-mono" => {
            vec![noise_vec(24000).into_iter().map(|v| v as i16).collect()]
        }
        "tonal-noise-stereo" => {
            let nz = noise_vec(24000);
            (0..2)
                .map(|_| {
                    (0..24000)
                        .map(|t| {
                            (sine_f(t, 48000.0, 1000.0, 8000.0) + nz[t] * 1500.0 / 8000.0) as i16
                        })
                        .collect()
                })
                .collect()
        }
        "quiet-sine-stereo" => (0..2)
            .map(|_| (0..48000).map(|t| sine_at(t, 50.0)).collect())
            .collect(),
        "sine-stereo-44100" => (0..2)
            .map(|_| {
                (0..44100)
                    .map(|t| sine_at_sr(t, 44100.0, 1000.0, 10_000.0))
                    .collect()
            })
            .collect(),
        "onset-stereo-44100" => (0..2)
            .map(|_| {
                (0..44100)
                    .map(|t| {
                        if t < 22050 {
                            0
                        } else {
                            sine_at_sr(t, 44100.0, 1000.0, 10_000.0)
                        }
                    })
                    .collect()
            })
            .collect(),
        _ => panic!("unknown fixture {name}"),
    }
}

/// Committed PSY_CONFIGURATION snapshot (long or short) for one rate and
/// per-channel bitrate.
#[derive(Debug, Clone)]
pub(crate) struct CfgSnap {
    pub(crate) sfb_cnt: usize,
    pub(crate) sfb_active: usize,
    pub(crate) sfb_active_lfe: usize,
    pub(crate) lowpass_line: usize,
    pub(crate) lowpass_line_lfe: usize,
    pub(crate) clip_energy: i32,
    pub(crate) offsets: Vec<i32>,
    pub(crate) pcm_quant: Vec<i32>,
    pub(crate) mask_hi: Vec<i32>,
    pub(crate) mask_lo: Vec<i32>,
    pub(crate) mask_hi_spr: Vec<i32>,
    pub(crate) mask_lo_spr: Vec<i32>,
    pub(crate) min_snr_ld: Vec<i32>,
}

/// Committed TNS configuration snapshot.
#[derive(Debug, Clone)]
pub(crate) struct TnsCfgSnap {
    pub(crate) active: bool,
    pub(crate) max_order: i32,
    pub(crate) coef_res: i32,
    pub(crate) start_band: [i32; 2],
    pub(crate) start_line: [i32; 2],
    pub(crate) stop_band: i32,
    pub(crate) stop_line: i32,
    pub(crate) thresh_on: [i32; 2],
    pub(crate) limit_order: [i32; 2],
}

fn parse_kv_list(line: &str) -> Vec<(String, String)> {
    // Skips the tag (and kind word, if any): keeps only k=v tokens.
    line.split_whitespace()
        .filter(|tok| tok.contains('='))
        .map(|kv| {
            let (k, v) = kv.split_once('=').expect("k=v field");
            (k.to_string(), v.to_string())
        })
        .collect()
}

/// Load committed config snapshots: ((long, long-tns), (short, short-tns)).
/// `pch` is the per-channel bitrate encoded in the file name.
pub(crate) fn load_cfg(
    sample_rate: u32,
    pch: u32,
) -> ((CfgSnap, TnsCfgSnap), (CfgSnap, TnsCfgSnap)) {
    let text = std::fs::read_to_string(testdata_path(&format!("cfg_{sample_rate}_{pch}.txt")))
        .expect("cfg testdata readable");
    let mut lines = text.lines();
    let mut block = || {
        let head = lines.next().expect("CFG head");
        assert!(head.starts_with("CFG"));
        let kv: std::collections::HashMap<String, String> =
            parse_kv_list(head).into_iter().collect();
        let get = |k: &str| kv[k].parse::<i32>().expect("int cfg field");
        let mut arrays = std::collections::HashMap::new();
        for _ in 0..7 {
            let line = lines.next().expect("cfg array");
            let (tag, rest) = line.split_once(' ').expect("tag + values");
            arrays.insert(
                tag.to_string(),
                rest.split_whitespace()
                    .map(|v| v.parse::<i32>().expect("int"))
                    .collect::<Vec<_>>(),
            );
        }
        let tns_line = lines.next().expect("TNSCFG");
        assert!(tns_line.starts_with("TNSCFG"));
        let tkv: std::collections::HashMap<String, String> =
            parse_kv_list(tns_line).into_iter().collect();
        let pair = |k: &str| {
            let (a, b) = tkv[k].split_once(',').expect("a,b pair");
            [
                a.parse::<i32>().expect("int"),
                b.parse::<i32>().expect("int"),
            ]
        };
        let snap = CfgSnap {
            sfb_cnt: get("cnt") as usize,
            sfb_active: get("active") as usize,
            sfb_active_lfe: get("activeLFE") as usize,
            lowpass_line: get("lowpass") as usize,
            lowpass_line_lfe: get("lowpassLFE") as usize,
            clip_energy: get("clipE"),
            offsets: arrays.remove("OFFSETS").expect("offsets"),
            pcm_quant: arrays.remove("PCMQ").expect("pcmq"),
            mask_hi: arrays.remove("MASKHI").expect("maskhi"),
            mask_lo: arrays.remove("MASKLO").expect("masklo"),
            mask_hi_spr: arrays.remove("MASKHIS").expect("maskhis"),
            mask_lo_spr: arrays.remove("MASKLOS").expect("masklos"),
            min_snr_ld: arrays.remove("MINSNR").expect("minsnr"),
        };
        let tns = TnsCfgSnap {
            active: tkv["active"] != "0",
            max_order: tkv["maxOrder"].parse().expect("int"),
            coef_res: tkv["coefRes"].parse().expect("int"),
            start_band: pair("startBand"),
            start_line: pair("startLine"),
            stop_band: tkv["stopBand"].parse().expect("int"),
            stop_line: tkv["stopLine"].parse().expect("int"),
            thresh_on: pair("threshOn"),
            limit_order: pair("limitOrder"),
        };
        (snap, tns)
    };
    (block(), block())
}

/// One committed TNS window: decisions, orders, lengths, coefficients.
#[derive(Debug, Clone)]
pub(crate) struct TnsWindow {
    pub(crate) window: usize,
    pub(crate) num_filters: i32,
    pub(crate) order: [i32; 2],
    pub(crate) length: [i32; 2],
    pub(crate) direction: [i32; 2],
    pub(crate) coef_hi: Vec<i32>,
    pub(crate) coef_lo: Vec<i32>,
    pub(crate) active: [bool; 2],
    pub(crate) gain: [i32; 2],
    pub(crate) merged: bool,
}

/// Grouped short-block output vectors.
#[derive(Debug, Clone)]
pub(crate) struct GroupedData {
    pub(crate) max_sfb: usize,
    pub(crate) e: Vec<i32>,
    pub(crate) t: Vec<i32>,
    pub(crate) spread: Vec<i32>,
    pub(crate) e_ld: Vec<i32>,
    pub(crate) t_ld: Vec<i32>,
    pub(crate) offsets: Vec<i32>,
    pub(crate) min_snr: Vec<i32>,
    pub(crate) spectrum: Vec<i32>,
}

/// One committed per-frame psyche snapshot.
#[derive(Debug, Clone)]
pub(crate) struct PsyFrame {
    pub(crate) mdct_scale: i32,
    pub(crate) final_shift: i32,
    pub(crate) tns_shift: i32,
    pub(crate) nrg_scaling: i32,
    pub(crate) maxscale: Vec<i32>,
    /// Per-window vectors (single entry with key semantics for long blocks
    /// in the dump; stored here in window order).
    pub(crate) nrg: Vec<Vec<i32>>,
    pub(crate) nrg_ld: Vec<Vec<i32>>,
    pub(crate) thr: Vec<Vec<i32>>,
    pub(crate) spread: Vec<Vec<i32>>,
    pub(crate) tns: Vec<TnsWindow>,
    pub(crate) nm1: Vec<i32>,
    pub(crate) m_nm1: i32,
    pub(crate) calc_pre_echo: i32,
    pub(crate) grouped: Option<GroupedData>,
    /// Long-block threshold Ld (TLD line, stored ints).
    pub(crate) threshold_ld: Option<Vec<i32>>,
    /// Long-block band count (LONGOUT head).
    pub(crate) long_max_sfb: Option<usize>,
    pub(crate) short: bool,
}

fn parse_int_list(rest: &str) -> Vec<i32> {
    rest.split_whitespace()
        .map(|v| v.parse::<i32>().expect("int"))
        .collect()
}

fn parse_pair(rest: &str) -> [i32; 2] {
    let (a, b) = rest.split_once(',').expect("a,b");
    [a.parse().expect("int"), b.parse().expect("int")]
}

/// One committed init-time config block from configs-all.txt.
#[derive(Debug, Clone)]
pub(crate) struct AllCfg {
    pub(crate) pch: u32,
    pub(crate) sample_rate: u32,
    pub(crate) long: bool,
    pub(crate) snap: CfgSnap,
    pub(crate) tns: TnsCfgSnap,
}

/// Load every init-time config block (12 rates x long/short x both VBR-5
/// operating points).
pub(crate) fn load_configs_all() -> Vec<AllCfg> {
    let text = std::fs::read_to_string(testdata_path("configs-all.txt")).expect("configs testdata");
    let mut out = Vec::new();
    let mut pch = 0u32;
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("# pch=") {
            pch = rest.parse().expect("pch");
        } else if line.starts_with("CFG ") {
            let sr: u32 = line
                .split_whitespace()
                .nth(2)
                .expect("sr field")
                .split('=')
                .nth(1)
                .expect("val")
                .parse()
                .expect("int");
            let long_head = line.to_string();
            let mut arrays = std::collections::HashMap::new();
            for _ in 0..7 {
                let l = lines.next().expect("cfg array");
                let (tag, rest) = l.split_once(' ').expect("tag");
                arrays.insert(
                    tag.to_string(),
                    rest.split_whitespace()
                        .map(|v| v.parse::<i32>().expect("int"))
                        .collect::<Vec<_>>(),
                );
            }
            let short_head = lines.next().expect("CFG SHORT");
            assert!(short_head.starts_with("CFG SHORT "));
            let mut short_arrays = std::collections::HashMap::new();
            for _ in 0..7 {
                let l = lines.next().expect("cfg array");
                let (tag, rest) = l.split_once(' ').expect("tag");
                short_arrays.insert(
                    tag.to_string(),
                    rest.split_whitespace()
                        .map(|v| v.parse::<i32>().expect("int"))
                        .collect::<Vec<_>>(),
                );
            }
            let tns_long_line = lines.next().expect("TNSCFG LONG");
            let tns_short_line = lines.next().expect("TNSCFG SHORT");
            let make_snap = |head: &str, arrs: &std::collections::HashMap<String, Vec<i32>>| {
                let get = |k: &str| {
                    head.split_whitespace()
                        .find(|t| t.starts_with(&format!("{k}=")))
                        .unwrap_or_else(|| panic!("field {k}"))
                        .split('=')
                        .nth(1)
                        .expect("val")
                        .parse::<i32>()
                        .expect("int")
                };
                CfgSnap {
                    sfb_cnt: get("cnt") as usize,
                    sfb_active: get("active") as usize,
                    sfb_active_lfe: get("activeLFE") as usize,
                    lowpass_line: get("lowpass") as usize,
                    lowpass_line_lfe: get("lowpassLFE") as usize,
                    clip_energy: get("clipE"),
                    offsets: arrs["OFFSETS"].clone(),
                    pcm_quant: arrs["PCMQ"].clone(),
                    mask_hi: arrs["MASKHI"].clone(),
                    mask_lo: arrs["MASKLO"].clone(),
                    mask_hi_spr: arrs["MASKHIS"].clone(),
                    mask_lo_spr: arrs["MASKLOS"].clone(),
                    min_snr_ld: arrs["MINSNR"].clone(),
                }
            };
            let make_tns = |tns_line: &str| {
                assert!(tns_line.starts_with("TNSCFG "));
                let tkv: std::collections::HashMap<String, String> =
                    parse_kv_list(tns_line).into_iter().collect();
                let pair = |k: &str| {
                    let (a, b) = tkv[k].split_once(',').expect("a,b");
                    [
                        a.parse::<i32>().expect("int"),
                        b.parse::<i32>().expect("int"),
                    ]
                };
                TnsCfgSnap {
                    active: tkv["active"] != "0",
                    max_order: tkv["maxOrder"].parse().expect("int"),
                    coef_res: tkv["coefRes"].parse().expect("int"),
                    start_band: pair("startBand"),
                    start_line: pair("startLine"),
                    stop_band: tkv["stopBand"].parse().expect("int"),
                    stop_line: tkv["stopLine"].parse().expect("int"),
                    thresh_on: pair("threshOn"),
                    limit_order: pair("limitOrder"),
                }
            };
            out.push(AllCfg {
                pch,
                sample_rate: sr,
                long: true,
                snap: make_snap(&long_head, &arrays),
                tns: make_tns(tns_long_line),
            });
            out.push(AllCfg {
                pch,
                sample_rate: sr,
                long: false,
                snap: make_snap(short_head, &short_arrays),
                tns: make_tns(tns_short_line),
            });
        } else {
            panic!("unexpected line {line}");
        }
    }
    out
}

/// One committed mid/side window: energies and Ld vectors.
#[derive(Debug, Clone)]
pub(crate) struct MsWindow {
    pub(crate) mid: Vec<i32>,
    pub(crate) side: Vec<i32>,
    pub(crate) mid_ld: Vec<i32>,
    pub(crate) side_ld: Vec<i32>,
}

/// Load committed element-level MS vectors for one frame (stereo only).
pub(crate) fn load_ms(name: &str, frame: u32) -> Vec<MsWindow> {
    let text = std::fs::read_to_string(testdata_path(&format!("ms_{name}_f{frame}.txt")))
        .expect("ms testdata readable");
    let mut wins = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let (tag, rest) = line.split_once(' ').expect("tag");
        assert!(tag.starts_with("MSM"));
        let w: usize = tag["MSM".len()..].parse().expect("window");
        let mid = parse_int_list(rest);
        let side_line = lines.next().expect("MSS");
        assert!(side_line.starts_with("MSS"));
        let side = parse_int_list(side_line.split_once(' ').expect("tag").1);
        let mid_ld_line = lines.next().expect("MSMLD");
        assert!(mid_ld_line.starts_with("MSMLD"));
        let mid_ld = parse_int_list(mid_ld_line.split_once(' ').expect("tag").1);
        let side_ld_line = lines.next().expect("MSSLD");
        assert!(side_ld_line.starts_with("MSSLD"));
        let side_ld = parse_int_list(side_ld_line.split_once(' ').expect("tag").1);
        assert_eq!(w, wins.len(), "windows in order");
        wins.push(MsWindow {
            mid,
            side,
            mid_ld,
            side_ld,
        });
    }
    wins
}

/// One committed autocorrelation window pair (raw Q31 ints, 13 lags).
#[derive(Debug, Clone)]
pub(crate) struct RxxWindow {
    pub(crate) window: usize,
    pub(crate) rxx1: Vec<i32>,
    pub(crate) rxx2: Vec<i32>,
}

/// Load committed ground-truth autocorrelation pairs for one frame/channel
/// (oracle replica of `FDKaacEnc_MergedAutoCorrelation`, same detect input).
pub(crate) fn load_rxx(name: &str, frame: u32, channel: u32) -> Vec<RxxWindow> {
    fn parse_rxx_line(line: &str, want_tag: &str) -> (usize, Vec<i32>) {
        let (tag, rest) = line.split_once(' ').expect("tag");
        assert_eq!(tag, want_tag);
        let mut toks = rest.split_whitespace();
        let w: usize = toks
            .next()
            .expect("w=k")
            .split('=')
            .nth(1)
            .expect("w value")
            .parse()
            .expect("window");
        let vals: Vec<i32> = toks.map(|v| v.parse().expect("int")).collect();
        assert_eq!(vals.len(), 13, "rxx lags");
        (w, vals)
    }
    let text = std::fs::read_to_string(testdata_path(&format!(
        "rxx_{name}_f{frame}_ch{channel}.txt"
    )))
    .expect("rxx testdata readable");
    let mut wins = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let (w, rxx1) = parse_rxx_line(line, "RXX1");
        let rxx2_line = lines.next().expect("RXX2");
        let (w2, rxx2) = parse_rxx_line(rxx2_line, "RXX2");
        assert_eq!(w, w2, "rxx pair window");
        assert_eq!(w, wins.len(), "windows in order");
        wins.push(RxxWindow {
            window: w,
            rxx1,
            rxx2,
        });
    }
    wins
}

/// Load one committed psyche frame. `short` selects the windowed-vector
/// layout (numbered NRG0..7 tags) vs the long layout (bare tags).
pub(crate) fn load_psy(name: &str, frame: u32, channel: u32, short: bool) -> PsyFrame {
    let text = std::fs::read_to_string(testdata_path(&format!(
        "psy_{name}_f{frame}_ch{channel}.txt"
    )))
    .expect("psy testdata readable");
    let mut lines = text.lines();
    let head = lines.next().expect("PSY head");
    let kv: std::collections::HashMap<String, String> = parse_kv_list(head).into_iter().collect();
    let maxscale = parse_int_list(
        lines
            .next()
            .expect("MAXSCALE")
            .split_once(' ')
            .expect("tag")
            .1,
    );
    let mut nrg = Vec::new();
    let mut nrg_ld = Vec::new();
    let mut thr = Vec::new();
    let mut spread = Vec::new();
    let windows = if short { 8 } else { 1 };
    // File order groups by kind (all NRGw, then all NRGLDw, ...), matching
    // the extractor; short tags carry the window suffix, long tags are bare.
    for (tag, store) in [
        ("NRG", &mut nrg),
        ("NRGLD", &mut nrg_ld),
        ("THR", &mut thr),
        ("SPREAD", &mut spread),
    ] {
        for _ in 0..windows {
            let line = lines.next().expect("psy vector");
            let (t, rest) = line.split_once(' ').expect("tag + values");
            assert!(
                t == tag || (short && t.starts_with(tag)),
                "expected {tag}, got {t}"
            );
            store.push(parse_int_list(rest));
        }
    }
    let mut tns = Vec::new();
    for _ in 0..windows {
        let wline = lines.next().expect("TNSW");
        assert!(wline.starts_with("TNSW "));
        let gline = lines.next().expect("TNSG");
        assert!(gline.starts_with("TNSG "));
        let (head, coefs) = wline["TNSW ".len()..]
            .split_once(" coefs=")
            .expect("coefs field");
        let wkv: std::collections::HashMap<String, String> = head
            .split_whitespace()
            .map(|kv| {
                let (k, v) = kv.split_once('=').expect("k=v field");
                (k.to_string(), v.to_string())
            })
            .collect();
        let gkv: std::collections::HashMap<String, String> =
            parse_kv_list(gline).into_iter().collect();
        let halves = coefs.split(" | ").collect::<Vec<_>>();
        assert_eq!(halves.len(), 2);
        let parse_coefs = |s: &str| {
            s.split(',')
                .map(|v| v.parse::<i32>().expect("coef"))
                .collect::<Vec<_>>()
        };
        let act = parse_pair(&gkv["active"]);
        tns.push(TnsWindow {
            window: wkv["w"].parse().expect("int"),
            num_filters: wkv["nf"].parse().expect("int"),
            order: parse_pair(&wkv["ord"]),
            length: parse_pair(&wkv["len"]),
            direction: parse_pair(&wkv["dir"]),
            coef_hi: parse_coefs(halves[0]),
            coef_lo: parse_coefs(halves[1]),
            active: [act[0] != 0, act[1] != 0],
            gain: parse_pair(&gkv["gain"]),
            merged: gkv["merged"] != "0",
        });
    }
    let nm1_line = lines.next().expect("NM1");
    assert!(nm1_line.starts_with("NM1 "));
    let nm1 = parse_int_list(nm1_line.split_once(' ').expect("tag").1);
    let st_line = lines.next().expect("NM1STATE");
    let stkv: std::collections::HashMap<String, String> =
        parse_kv_list(st_line).into_iter().collect();
    let mut grouped = None;
    let mut threshold_ld = None;
    let mut long_max_sfb = None;
    if short {
        let ghead = lines.next().expect("GROUPED head");
        assert!(ghead.starts_with("GROUPED "));
        let gkv: std::collections::HashMap<String, String> =
            parse_kv_list(ghead).into_iter().collect();
        let mut g = GroupedData {
            max_sfb: gkv["maxSfb"].parse().expect("int"),
            e: vec![],
            t: vec![],
            spread: vec![],
            e_ld: vec![],
            t_ld: vec![],
            offsets: vec![],
            min_snr: vec![],
            spectrum: vec![],
        };
        for tag in ["GE", "GT", "GS", "GELD", "GTLD", "GOFF", "GMSNR", "GSPEC"] {
            let line = lines.next().expect("grouped vector");
            assert!(line.starts_with(tag), "expected {tag}");
            let vals = parse_int_list(line.split_once(' ').expect("tag").1);
            match tag {
                "GE" => g.e = vals,
                "GT" => g.t = vals,
                "GS" => g.spread = vals,
                "GELD" => g.e_ld = vals,
                "GTLD" => g.t_ld = vals,
                "GOFF" => g.offsets = vals,
                "GMSNR" => g.min_snr = vals,
                "GSPEC" => g.spectrum = vals,
                _ => unreachable!(),
            }
        }
        grouped = Some(g);
    } else {
        let lhead = lines.next().expect("LONGOUT head");
        assert!(lhead.starts_with("LONGOUT "));
        let tld_line = lines.next().expect("TLD");
        assert!(tld_line.starts_with("TLD "));
        threshold_ld = Some(parse_int_list(tld_line.split_once(' ').expect("tag").1));
        let lkv: std::collections::HashMap<String, String> =
            parse_kv_list(lhead).into_iter().collect();
        long_max_sfb = Some(lkv["maxSfb"].parse().expect("int"));
    }
    PsyFrame {
        mdct_scale: kv["mdctScale"].parse().expect("int"),
        final_shift: kv["finalShift"].parse().expect("int"),
        tns_shift: kv["tnsShift"].parse().expect("int"),
        nrg_scaling: kv["nrgScaling"].parse().expect("int"),
        maxscale,
        nrg,
        nrg_ld,
        thr,
        spread,
        tns,
        nm1,
        m_nm1: stkv["m1"].parse().expect("int"),
        calc_pre_echo: stkv["calc"].parse().expect("int"),
        grouped,
        threshold_ld,
        long_max_sfb,
        short,
    }
}
