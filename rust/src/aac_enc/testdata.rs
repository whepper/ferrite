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
// M6.1: config-snapshot loading moved to the production `psy_config` module
// (needed by `StreamEncoder::new`); re-exported here so existing test call
// sites are untouched. M6.3: snapshots are embedded at compile time, so the
// testdata path helper (for all other committed vectors) lives back here.
pub(crate) use super::psy_config::{load_cfg, parse_kv_list, CfgSnap, TnsCfgSnap};
use std::path::PathBuf;

pub(crate) fn testdata_path(file: &str) -> PathBuf {
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

/// Parse an embedded 48 kHz stereo s16le WAV with a plain 44-byte header
/// into per-channel i16 vectors. Used only by the M5.5 licensed fixtures
/// below (real speech/music assets); ordinary tests never touch the
/// network. Panics loudly on any format deviation.
fn wav_stereo_s16(bytes: &[u8]) -> Vec<Vec<i16>> {
    assert!(bytes.len() > 44, "wav too short");
    assert_eq!(&bytes[0..4], b"RIFF", "wav RIFF");
    assert_eq!(&bytes[8..12], b"WAVE", "wav WAVE");
    // Walk chunks (ffmpeg may emit a LIST chunk before data).
    let mut off = 12usize;
    let (mut fmt, mut data) = (None, None);
    while off + 8 <= bytes.len() {
        let id = &bytes[off..off + 4];
        let size = u32::from_le_bytes([
            bytes[off + 4],
            bytes[off + 5],
            bytes[off + 6],
            bytes[off + 7],
        ]) as usize;
        if id == b"fmt " {
            fmt = Some(off + 8);
        } else if id == b"data" {
            data = Some((off + 8, size));
            break;
        }
        off += 8 + size + (size % 2);
    }
    let fmt = fmt.expect("wav fmt chunk");
    let (dstart, dsize) = data.expect("wav data chunk");
    assert_eq!(
        u16::from_le_bytes([bytes[fmt], bytes[fmt + 1]]),
        1,
        "wav PCM"
    );
    assert_eq!(
        u16::from_le_bytes([bytes[fmt + 2], bytes[fmt + 3]]),
        2,
        "wav stereo"
    );
    assert_eq!(
        u32::from_le_bytes([
            bytes[fmt + 4],
            bytes[fmt + 5],
            bytes[fmt + 6],
            bytes[fmt + 7]
        ]),
        48000,
        "wav rate"
    );
    assert_eq!(
        u16::from_le_bytes([bytes[fmt + 14], bytes[fmt + 15]]),
        16,
        "wav s16"
    );
    assert_eq!(dstart + dsize, bytes.len(), "wav length");
    assert_eq!(dsize % 4, 0, "wav frame alignment");
    let (mut l, mut r) = (Vec::with_capacity(dsize / 4), Vec::with_capacity(dsize / 4));
    for fr in bytes[dstart..dstart + dsize].chunks_exact(4) {
        l.push(i16::from_le_bytes([fr[0], fr[1]]));
        r.push(i16::from_le_bytes([fr[2], fr[3]]));
    }
    vec![l, r]
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
        // M5.e mono-44.1k restore: mono version of the onset-stereo-44100
        // formula (byte-identical to that fixture's channel 0).
        "onset-mono-44100" => (0..1)
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
        // M6 stress (Phase 1 streaming coverage): full-scale deterministic
        // PRNG + tone stack, mirroring `make_fixtures.py` exactly (seed
        // 0x76F1A9B3, `(z % 32767) - 16383.0` noise, `nz * 0.9 + tones`,
        // i16 clamp via truncating-saturating `as`).
        "stress-stereo" => {
            let mut st = 0x76F1A9B3u32;
            let nz: Vec<f64> = (0..24000)
                .map(|_| (splitmix32(&mut st) % 32767) as f64 - 16383.0)
                .collect();
            (0..2)
                .map(|_| {
                    (0..24000)
                        .map(|t| {
                            let time = t as f64 / 48000.0;
                            let tones = 12000.0
                                * (2.0 * core::f64::consts::PI * 6000.0 * time).sin()
                                + 9000.0 * (2.0 * core::f64::consts::PI * 12000.0 * time).sin()
                                + 6000.0 * (2.0 * core::f64::consts::PI * 18000.0 * time).sin();
                            (nz[t] * 0.9 + tones) as i16
                        })
                        .collect()
                })
                .collect()
        }
        // M5.5 licensed fixtures (see testdata/m55-PROVENANCE.txt):
        // real speech/music recordings, embedded as deterministic WAV
        // assets (no network at test time).
        "music-stereo" => wav_stereo_s16(include_bytes!("testdata/m7-music-stereo-48k.wav")),
        "speech-pauses-stereo" => {
            wav_stereo_s16(include_bytes!("testdata/m7-speech-pauses-stereo-48k.wav"))
        }
        "speech-clean-stereo" => {
            wav_stereo_s16(include_bytes!("testdata/m7-speech-clean-stereo-48k.wav"))
        }
        // V3 lossless real-music reference (see testdata/m8-PROVENANCE.txt):
        // Musopen/Beethoven "Eroica" I excerpt, 24->16 bit by arithmetic
        // shift with no dither. Additive to the M5.5 licensed set; the m7
        // assets above are unchanged.
        "music-eroica-stereo" => {
            wav_stereo_s16(include_bytes!("testdata/m8-music-eroica-stereo-48k.wav"))
        }
        _ => panic!("unknown fixture {name}"),
    }
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

/// One committed S1 (M4-S1) channel block from `qc1_<name>_f<F>.txt`.
#[derive(Debug, Clone)]
pub(crate) struct S1Channel {
    pub(crate) geo_cnt: usize,
    pub(crate) geo_per: usize,
    pub(crate) geo_max: usize,
    pub(crate) geo_seq: u8,
    /// Exact SFB offsets consumed (`SOFF`, `cnt + 1` entries).
    pub(crate) offsets: Vec<i32>,
    /// TNS decisions consumed (`STNSW`, one per window; `active`/`gain`/
    /// `merged` are runtime state, unset here).
    pub(crate) tns: Vec<TnsWindow>,
    /// Per-window TNS coefficient resolution (`STNSR`).
    pub(crate) tns_coef_res: Vec<i32>,
    pub(crate) spec: Vec<i32>,
    pub(crate) penld: Vec<i32>,
    pub(crate) pthld: Vec<i32>,
    /// Post-decision linear energies (`SENL`, weighting input).
    pub(crate) senl: Vec<i32>,
    pub(crate) ff: Vec<i32>,
    pub(crate) enfac: Vec<i32>,
    pub(crate) wenld: Vec<i32>,
    pub(crate) wthld: Vec<i32>,
    pub(crate) spnl: Vec<i32>,
    pub(crate) spech: Vec<i32>,
    pub(crate) spco: Vec<i32>,
    pub(crate) spna: Vec<i32>,
    pub(crate) pech: [i32; 3],
    pub(crate) isb: Vec<i32>,
    pub(crate) iss: Vec<i32>,
    pub(crate) noise: Vec<i32>,
}

/// One committed S1 element frame: per-channel blocks plus (stereo) MS
/// digest/mask, element PE and the static-bit count.
#[derive(Debug, Clone)]
pub(crate) struct S1Frame {
    pub(crate) frame: u32,
    pub(crate) ch: Vec<S1Channel>,
    pub(crate) msd: Option<(i32, Vec<i32>)>,
    pub(crate) pe: [i32; 4],
    pub(crate) static_el: String,
    pub(crate) static_bits: i32,
    /// Pre-frame weighting state `[enfac0, enfac1, patch0, patch1]`.
    pub(crate) ats_pre: [i32; 4],
    /// Post-frame weighting state (same layout; validates evolution).
    pub(crate) ats_post: [i32; 4],
}

/// Load a committed S1 frame. Strict tag-order parsing: any oracle format
/// drift fails loudly here, not in numeric comparisons.
pub(crate) fn load_s1(name: &str, frame: u32) -> S1Frame {
    let text = std::fs::read_to_string(testdata_path(&format!("qc1_{name}_f{frame}.txt")))
        .expect("s1 testdata readable");
    let mut lines = text.lines();
    let head = lines.next().expect("S1FRAME head");
    assert_eq!(head, format!("S1FRAME f={frame}"));
    let mut ch_blocks: Vec<S1Channel> = Vec::new();
    let mut msd: Option<(i32, Vec<i32>)> = None;
    let mut pe = [0i32; 4];
    let mut static_el = String::new();
    let mut static_bits = 0i32;
    // Per-channel tail tags in dump order (after S1GEO/SOFF/STNSW/STNSR).
    const TAIL: [&str; 16] = [
        "SPEC", "PENLD", "PTHLD", "SENL", "FF", "ENFAC", "WENLD", "WTHLD", "SPNL", "SPECH", "SPCO",
        "SPNA", "PECH", "ISB", "ISS", "NOISE",
    ];
    let mut pending: Vec<String> = Vec::new();
    let mut ats_pre = [0i32; 4];
    let mut ats_post = [0i32; 4];
    for line in lines {
        if line.starts_with("ATS pre=") {
            let csv = line.split_once('=').expect("k=v").1.replace(',', " ");
            let vals = parse_int_list(&csv);
            assert_eq!(vals.len(), 4);
            ats_pre = [vals[0], vals[1], vals[2], vals[3]];
        } else if line.starts_with("ATS post=") {
            let csv = line.split_once('=').expect("k=v").1.replace(',', " ");
            let vals = parse_int_list(&csv);
            assert_eq!(vals.len(), 4);
            ats_post = [vals[0], vals[1], vals[2], vals[3]];
        } else if line.starts_with("MSD ") {
            let vals = parse_int_list(line.split_once(' ').expect("tag").1);
            msd = Some((vals[0], vals[1..].to_vec()));
        } else if line.starts_with("PE ") {
            let vals = parse_int_list(line.split_once(' ').expect("tag").1);
            assert_eq!(vals.len(), 4);
            pe = [vals[0], vals[1], vals[2], vals[3]];
        } else if line.starts_with("STATIC ") {
            let mut p = line.split_whitespace();
            p.next().expect("tag");
            static_el = p.next().expect("element").to_string();
            static_bits = p.next().expect("bits").parse().expect("int");
        } else {
            pending.push(line.to_string());
        }
    }
    assert!(!pending.is_empty(), "no channel blocks");
    // Channel blocks appear back-to-back in dump order; split on S1GEO.
    let mut starts = Vec::new();
    for (i, ln) in pending.iter().enumerate() {
        if ln.starts_with("S1GEO ") {
            starts.push(i);
        }
    }
    assert!(!starts.is_empty());
    for (ci, &s) in starts.iter().enumerate() {
        let e = if ci + 1 < starts.len() {
            starts[ci + 1]
        } else {
            pending.len()
        };
        let block = &pending[s..e];
        let mut bi = 0;
        assert!(block[bi].starts_with("S1GEO "), "expected S1GEO");
        let geo = |ln: &str| {
            ln.split_whitespace()
                .skip(1)
                .map(|kv| {
                    kv.split_once('=')
                        .expect("k=v")
                        .1
                        .parse::<i32>()
                        .expect("int")
                })
                .collect::<Vec<_>>()
        };
        let g = geo(&block[bi]);
        bi += 1;
        assert_eq!(g.len(), 5); // ch cnt per max seq
        let cnt = g[1] as usize;
        let nwin = if g[4] == 2 { 8 } else { 1 };
        assert!(block[bi].starts_with("SOFF "), "expected SOFF");
        let offsets = parse_int_list(block[bi].split_once(' ').expect("tag").1);
        assert_eq!(offsets.len(), cnt + 1, "SOFF length");
        bi += 1;
        let mut tns = Vec::new();
        for _ in 0..nwin {
            let wline = block[bi].as_str();
            assert!(wline.starts_with("STNSW "), "expected STNSW");
            let (head, coefs) = wline["STNSW ".len()..]
                .split_once(" coefs=")
                .expect("coefs field");
            let wkv: std::collections::HashMap<String, String> = head
                .split_whitespace()
                .map(|kv| {
                    let (k, v) = kv.split_once('=').expect("k=v field");
                    (k.to_string(), v.to_string())
                })
                .collect();
            let halves = coefs.split(" | ").collect::<Vec<_>>();
            assert_eq!(halves.len(), 2);
            let parse_coefs = |s: &str| {
                s.split(',')
                    .map(|v| v.parse::<i32>().expect("coef"))
                    .collect::<Vec<_>>()
            };
            tns.push(TnsWindow {
                window: wkv["w"].parse().expect("int"),
                num_filters: wkv["nf"].parse().expect("int"),
                order: parse_pair(&wkv["ord"]),
                length: parse_pair(&wkv["len"]),
                direction: parse_pair(&wkv["dir"]),
                coef_hi: parse_coefs(halves[0]),
                coef_lo: parse_coefs(halves[1]),
                active: [false, false],
                gain: [0, 0],
                merged: false,
            });
            bi += 1;
        }
        assert!(block[bi].starts_with("STNSR"), "expected STNSR");
        let tns_coef_res = parse_int_list(block[bi].split_once(' ').expect("tag").1);
        assert_eq!(tns_coef_res.len(), nwin, "STNSR length");
        bi += 1;
        for (ln, tag) in block[bi..].iter().zip(TAIL.iter()) {
            assert!(
                ln.starts_with(&format!("{tag} ")),
                "expected {tag}, got {}",
                &ln[..ln.len().min(12)]
            );
        }
        assert_eq!(block.len() - bi, TAIL.len(), "S1 channel tail tags");
        let vec = |ln: &str| parse_int_list(ln.split_once(' ').expect("tag").1);
        let tail = |tag_idx: usize| vec(&block[bi + tag_idx]);
        let pech_line = &block[bi + 12];
        let pech_kv: std::collections::HashMap<String, String> = pech_line
            .split_whitespace()
            .skip(1)
            .filter(|tok| tok.contains('='))
            .map(|kv| {
                let (k, v) = kv.split_once('=').expect("k=v");
                (k.to_string(), v.to_string())
            })
            .collect();
        let get3 = |k: &str| pech_kv[k].parse::<i32>().expect("int");
        // SPEC carries "ch=<c>" as its second token.
        let spec_vals = {
            let rest = block[bi].split_once(' ').expect("tag").1;
            let body = rest.split_once(' ').expect("ch tag").1;
            parse_int_list(body)
        };
        assert_eq!(spec_vals.len(), 1024, "SPEC length");
        for (tag, vals) in [
            ("PENLD", tail(1)),
            ("PTHLD", tail(2)),
            ("SENL", tail(3)),
            ("FF", tail(4)),
            ("ENFAC", tail(5)),
            ("WENLD", tail(6)),
            ("WTHLD", tail(7)),
            ("SPNL", tail(8)),
            ("SPECH", tail(9)),
            ("SPCO", tail(10)),
            ("SPNA", tail(11)),
        ] {
            assert_eq!(vals.len(), cnt, "{tag} length");
        }
        assert_eq!(tail(13).len(), cnt, "ISB length");
        assert_eq!(tail(14).len(), cnt, "ISS length");
        ch_blocks.push(S1Channel {
            geo_cnt: cnt,
            geo_per: g[2] as usize,
            geo_max: g[3] as usize,
            geo_seq: g[4] as u8,
            offsets,
            tns,
            tns_coef_res,
            spec: spec_vals,
            penld: tail(1),
            pthld: tail(2),
            senl: tail(3),
            ff: tail(4),
            enfac: tail(5),
            wenld: tail(6),
            wthld: tail(7),
            spnl: tail(8),
            spech: tail(9),
            spco: tail(10),
            spna: tail(11),
            pech: [get3("pe"), get3("const"), get3("nact")],
            isb: tail(13),
            iss: tail(14),
            noise: tail(15),
        });
    }
    if ch_blocks.len() == 2 {
        assert!(msd.is_some(), "stereo S1 frame needs MSD");
    }
    S1Frame {
        frame,
        ch: ch_blocks,
        msd,
        pe,
        static_el,
        static_bits,
        ats_pre,
        ats_post,
    }
}

/// One committed S2 (M4-S2) channel block from `qc2_<name>_f<F>.txt`.
#[derive(Debug, Clone)]
pub(crate) struct S2Channel {
    pub(crate) geo_cnt: usize,
    pub(crate) geo_per: usize,
    pub(crate) geo_max: usize,
    pub(crate) geo_seq: u8,
    /// `groupLen` (`GROUPLEN`, `MAX_NO_OF_GROUPS` = 4 entries).
    pub(crate) group_len: Vec<i32>,
    pub(crate) thr_exp: Vec<i32>,
    pub(crate) minsnr0: Vec<i32>,
    pub(crate) spread0: Vec<i32>,
    pub(crate) thrv: Vec<i32>,
    pub(crate) minsnr1: Vec<i32>,
    pub(crate) spread1: Vec<i32>,
}

/// One committed S2 element frame: chaos evolution plus per-channel
/// threshold working state. S1 inputs (weighted energies/thresholds,
/// form factors, EnFac, linear energies, MS mask, geometry) come from
/// the matching `qc1` file.
#[derive(Debug, Clone)]
pub(crate) struct S2Frame {
    pub(crate) frame: u32,
    pub(crate) ch: Vec<S2Channel>,
    pub(crate) chaos_pre: i32,
    pub(crate) chaos_post: i32,
}

/// Load a committed S2 frame. Strict tag-order parsing: any oracle format
/// drift fails loudly here, not in numeric comparisons.
pub(crate) fn load_s2(name: &str, frame: u32) -> S2Frame {
    let text = std::fs::read_to_string(testdata_path(&format!("qc2_{name}_f{frame}.txt")))
        .expect("s2 testdata readable");
    let mut lines = text.lines();
    let head = lines.next().expect("S2FRAME head");
    assert_eq!(head, format!("S2FRAME f={frame}"));
    const PER_CH: [&str; 8] = [
        "S2GEO", "GROUPLEN", "THREXP", "MINSNR0", "SPREAD0", "THRV", "MINSNR1", "SPREAD1",
    ];
    let mut chaos_pre = 0i32;
    let mut chaos_post = 0i32;
    let mut pending: Vec<String> = Vec::new();
    for line in lines {
        if line.starts_with("S2CHAOS ") {
            let kv: std::collections::HashMap<String, String> = line
                .split_whitespace()
                .skip(1)
                .map(|tok| {
                    let (k, v) = tok.split_once('=').expect("k=v");
                    (k.to_string(), v.to_string())
                })
                .collect();
            chaos_pre = kv["pre"].parse().expect("int");
            chaos_post = kv["post"].parse().expect("int");
        } else if line.starts_with("S2END") {
            // Terminator, nothing to store.
        } else {
            pending.push(line.to_string());
        }
    }
    let mut starts = Vec::new();
    for (i, ln) in pending.iter().enumerate() {
        if ln.starts_with("S2GEO ") {
            starts.push(i);
        }
    }
    assert!(!starts.is_empty(), "no S2 channel blocks");
    let mut ch_blocks: Vec<S2Channel> = Vec::new();
    for (ci, &s) in starts.iter().enumerate() {
        let e = if ci + 1 < starts.len() {
            starts[ci + 1]
        } else {
            pending.len()
        };
        let block = &pending[s..e];
        assert_eq!(block.len(), PER_CH.len(), "S2 channel block tags");
        for (ln, tag) in block.iter().zip(PER_CH.iter()) {
            assert!(
                ln.starts_with(&format!("{tag} ")),
                "expected {tag}, got {}",
                &ln[..ln.len().min(12)]
            );
        }
        let geo = |ln: &str| {
            ln.split_whitespace()
                .skip(1)
                .map(|kv| {
                    kv.split_once('=')
                        .expect("k=v")
                        .1
                        .parse::<i32>()
                        .expect("int")
                })
                .collect::<Vec<_>>()
        };
        let g = geo(&block[0]);
        assert_eq!(g.len(), 5); // ch cnt per max seq
        let cnt = g[1] as usize;
        let vec = |ln: &str| parse_int_list(ln.split_once(' ').expect("tag").1);
        let group_len = vec(&block[1]);
        assert_eq!(group_len.len(), 4, "GROUPLEN length");
        for (tag, vals) in [
            ("THREXP", vec(&block[2])),
            ("MINSNR0", vec(&block[3])),
            ("SPREAD0", vec(&block[4])),
            ("THRV", vec(&block[5])),
            ("MINSNR1", vec(&block[6])),
            ("SPREAD1", vec(&block[7])),
        ] {
            assert_eq!(vals.len(), cnt, "{tag} length");
        }
        ch_blocks.push(S2Channel {
            geo_cnt: cnt,
            geo_per: g[2] as usize,
            geo_max: g[3] as usize,
            geo_seq: g[4] as u8,
            group_len,
            thr_exp: vec(&block[2]),
            minsnr0: vec(&block[3]),
            spread0: vec(&block[4]),
            thrv: vec(&block[5]),
            minsnr1: vec(&block[6]),
            spread1: vec(&block[7]),
        });
    }
    S2Frame {
        frame,
        ch: ch_blocks,
        chaos_pre,
        chaos_post,
    }
}

/// One synthetic quantizer probe case from `qprobe_quant.txt` (produced by
/// `tools/enc-oracle/quant_probe`, which calls the unmodified reference).
#[derive(Debug, Clone)]
pub(crate) enum QProbe {
    /// Single-band quantize: `QL g=.. dz=.. n=.. spec .. q ..`.
    Quantize {
        gain: i32,
        dz: bool,
        spec: Vec<i32>,
        q: Vec<i16>,
    },
    /// Multi-band quantize with an scf ramp: `QBANDS gg=.. dz=..`.
    /// Geometry is fixed by the probe (2 groups x 2 bands,
    /// offsets [0,3,7,11,16], scf [5,15,25,35]).
    QuantizeBands {
        gain: i32,
        spec: Vec<i32>,
        q: Vec<i16>,
    },
    /// Distortion with internal re-quantize: `QD g=.. dz=.. n=..`.
    Dist {
        gain: i32,
        dz: bool,
        spec: Vec<i32>,
        q: Vec<i16>,
        dist: i32,
    },
    /// Energy/distortion with caller-chosen lines: `QI g=.. n=..`.
    InvDist {
        gain: i32,
        spec: Vec<i32>,
        q: Vec<i16>,
        en: i32,
        dist: i32,
    },
}

/// Probe band geometry (must match `quant_probe.cpp` QBANDS).
pub(crate) const QPROBE_BAND_OFF: [i32; 5] = [0, 3, 7, 11, 16];
pub(crate) const QPROBE_BAND_SCF: [i32; 4] = [5, 15, 25, 35];

/// Load every synthetic probe case. Strict: unknown tags, field drift, or
/// value overflow fails loudly here, not in numeric comparisons.
pub(crate) fn load_qprobe() -> Vec<QProbe> {
    let text = std::fs::read_to_string(testdata_path("qprobe_quant.txt")).expect("qprobe testdata");
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("QPROBE ") || line.starts_with("QTARCH") {
            continue;
        }
        if line.starts_with("QTABQ") || line.starts_with("QTABE") || line.starts_with("MTAB") {
            continue;
        }
        if line.starts_with("EXPM") || line.starts_with("EXPS") || line.starts_with("KZ ") {
            continue;
        }
        let toks: Vec<&str> = line.split_whitespace().collect();
        let kv = |t: &str| {
            t.split_once('=')
                .unwrap_or_else(|| panic!("k=v token: {t}"))
                .1
                .parse::<i32>()
                .unwrap_or_else(|_| panic!("int token: {t}"))
        };
        let section = |tag: &str| -> Vec<i32> {
            let s = toks
                .iter()
                .position(|&t| t == tag)
                .unwrap_or_else(|| panic!("{tag} in {line}"));
            toks[s + 1..]
                .iter()
                .take_while(|&&t| !t.contains('=') && t != "q" && t != "dist" && t != "en")
                .map(|t| t.parse::<i32>().unwrap_or_else(|_| panic!("int: {t}")))
                .collect()
        };
        match toks[0] {
            "QL" => {
                let gain = kv(toks[1]);
                let dz = kv(toks[2]) != 0;
                let n = kv(toks[3]) as usize;
                let spec = section("spec");
                let q: Vec<i16> = section("q").iter().map(|&v| v as i16).collect();
                assert_eq!(spec.len(), n, "QL spec length");
                assert_eq!(q.len(), n, "QL q length");
                out.push(QProbe::Quantize { gain, dz, spec, q });
            }
            "QBANDS" => {
                let gain = kv(toks[1]);
                let spec = section("spec");
                let q: Vec<i16> = section("q").iter().map(|&v| v as i16).collect();
                assert_eq!(spec.len(), 16, "QBANDS spec length");
                assert_eq!(q.len(), 16, "QBANDS q length");
                out.push(QProbe::QuantizeBands { gain, spec, q });
            }
            "QD" => {
                let gain = kv(toks[1]);
                let dz = kv(toks[2]) != 0;
                let n = kv(toks[3]) as usize;
                let spec = section("spec");
                let q: Vec<i16> = section("q").iter().map(|&v| v as i16).collect();
                let dpos = toks.iter().position(|&t| t == "dist").expect("dist tag");
                let dist = toks[dpos + 1].parse::<i32>().expect("dist int");
                assert_eq!(spec.len(), n, "QD spec length");
                assert_eq!(q.len(), n, "QD q length");
                out.push(QProbe::Dist {
                    gain,
                    dz,
                    spec,
                    q,
                    dist,
                });
            }
            "QI" => {
                let gain = kv(toks[1]);
                let n = kv(toks[2]) as usize;
                let spec = section("spec");
                let q: Vec<i16> = section("q").iter().map(|&v| v as i16).collect();
                let epos = toks.iter().position(|&t| t == "en").expect("en tag");
                let dpos = toks.iter().position(|&t| t == "dist").expect("dist tag");
                let en = toks[epos + 1].parse::<i32>().expect("en int");
                let dist = toks[dpos + 1].parse::<i32>().expect("dist int");
                assert_eq!(spec.len(), n, "QI spec length");
                assert_eq!(q.len(), n, "QI q length");
                out.push(QProbe::InvDist {
                    gain,
                    spec,
                    q,
                    en,
                    dist,
                });
            }
            _ => panic!("unknown qprobe line: {line}"),
        }
    }
    assert!(!out.is_empty(), "no probe cases loaded");
    out
}

/// One committed S3 element frame: quantizer outputs over a gain grid on
/// the matching `qc1` post-MS/IS spectrum (`SPEC`), which is exactly what
/// `QuantizeSpectrum` consumes in the aliased encoder (`aacenc.cpp:803`).
#[derive(Debug, Clone)]
pub(crate) struct S3Channel {
    pub(crate) geo_cnt: usize,
    pub(crate) geo_per: usize,
    pub(crate) geo_max: usize,
    pub(crate) geo_seq: u8,
    pub(crate) offsets: Vec<i32>,
    pub(crate) gains: Vec<i32>,
    /// Full 1024-line quantized spectra, one per grid gain (`S3Q`).
    pub(crate) qspec: Vec<Vec<i16>>,
    /// Per-band Ld distortion with internal re-quantize, one per grid
    /// gain (`S3DIST`, `cnt` entries each).
    pub(crate) dist: Vec<Vec<i32>>,
    /// Per-band `(energy, distortion)` with the committed quantized
    /// lines fed back in, one pair per grid gain (`S3EN`).
    pub(crate) en: Vec<Vec<i32>>,
    pub(crate) en_dist: Vec<Vec<i32>>,
}

/// One committed S3 element frame.
#[derive(Debug, Clone)]
pub(crate) struct S3Frame {
    pub(crate) frame: u32,
    pub(crate) ch: Vec<S3Channel>,
}

/// Load a committed S3 frame. Strict tag-order parsing: any oracle format
/// drift fails loudly here, not in numeric comparisons.
pub(crate) fn load_s3(name: &str, frame: u32) -> S3Frame {
    let text = std::fs::read_to_string(testdata_path(&format!("qc3_{name}_f{frame}.txt")))
        .expect("s3 testdata readable");
    let mut lines = text.lines();
    let head = lines.next().expect("S3FRAME head");
    assert_eq!(head, format!("S3FRAME f={frame}"));
    let mut ch_blocks: Vec<S3Channel> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    for line in lines {
        if line.starts_with("S3END") {
            continue;
        }
        pending.push(line.to_string());
    }
    let mut starts = Vec::new();
    for (i, ln) in pending.iter().enumerate() {
        if ln.starts_with("S3GEO ") {
            starts.push(i);
        }
    }
    assert!(!starts.is_empty(), "no S3 channel blocks");
    for (ci, &s) in starts.iter().enumerate() {
        let e = if ci + 1 < starts.len() {
            starts[ci + 1]
        } else {
            pending.len()
        };
        let block = &pending[s..e];
        assert!(block[0].starts_with("S3GEO "), "S3GEO head");
        let g: Vec<i32> = block[0]
            .split_whitespace()
            .skip(1)
            .map(|kv| {
                kv.split_once('=')
                    .expect("k=v")
                    .1
                    .parse::<i32>()
                    .expect("int")
            })
            .collect();
        assert_eq!(g.len(), 5); // ch cnt per max seq
        let cnt = g[1] as usize;
        assert!(block[1].starts_with("S3OFF "), "S3OFF second");
        let offsets = parse_int_list(block[1].split_once(' ').expect("tag").1);
        assert_eq!(offsets.len(), cnt + 1, "S3OFF length");
        let mut gains = Vec::new();
        let mut qspec = Vec::new();
        let mut dist = Vec::new();
        let mut en = Vec::new();
        let mut en_dist = Vec::new();
        let mut i = 2;
        while i < block.len() {
            let ln = &block[i];
            assert!(
                ln.starts_with("S3Q g="),
                "S3Q tag, got {}",
                &ln[..ln.len().min(12)]
            );
            let gain: i32 = ln
                .split_whitespace()
                .nth(1)
                .expect("g=")
                .split_once('=')
                .expect("k=v")
                .1
                .parse()
                .expect("gain");
            let ch_tok = ln.split_whitespace().nth(2).expect("ch=");
            assert_eq!(ch_tok.split_once('=').expect("k=v").0, "ch");
            let q: Vec<i16> = ln
                .split_once("ch=")
                .expect("ch")
                .1
                .split_whitespace()
                .skip(1)
                .map(|v| v.parse::<i32>().expect("q int") as i16)
                .collect();
            assert_eq!(q.len(), 1024, "S3Q length");
            i += 1;
            let dl = &block[i];
            assert!(dl.starts_with("S3DIST g="), "S3DIST tag");
            let dgain: i32 = dl
                .split_whitespace()
                .nth(1)
                .expect("g=")
                .split_once('=')
                .expect("k=v")
                .1
                .parse()
                .expect("gain");
            assert_eq!(dgain, gain, "S3Q/S3DIST gain agreement");
            let d: Vec<i32> = parse_int_list(
                dl.split_once("ch=")
                    .expect("ch")
                    .1
                    .split_once(' ')
                    .expect("skip")
                    .1,
            );
            assert_eq!(d.len(), cnt, "S3DIST length");
            i += 1;
            let el = &block[i];
            assert!(el.starts_with("S3EN g="), "S3EN tag");
            let egain: i32 = el
                .split_whitespace()
                .nth(1)
                .expect("g=")
                .split_once('=')
                .expect("k=v")
                .1
                .parse()
                .expect("gain");
            assert_eq!(egain, gain, "S3Q/S3EN gain agreement");
            let rest = el.split_once("ch=").expect("ch").1;
            let epart = rest.split_once(" en ").expect("en").1;
            let (e_str, d_str) = epart.split_once(" dist ").expect("dist");
            let e: Vec<i32> = parse_int_list(e_str);
            let ed: Vec<i32> = parse_int_list(d_str);
            assert_eq!(e.len(), cnt, "S3EN en length");
            assert_eq!(ed.len(), cnt, "S3EN dist length");
            i += 1;
            gains.push(gain);
            qspec.push(q);
            dist.push(d);
            en.push(e);
            en_dist.push(ed);
        }
        assert!(!gains.is_empty(), "no S3 gains");
        ch_blocks.push(S3Channel {
            geo_cnt: cnt,
            geo_per: g[2] as usize,
            geo_max: g[3] as usize,
            geo_seq: g[4] as u8,
            offsets,
            gains,
            qspec,
            dist,
            en,
            en_dist,
        });
    }
    S3Frame {
        frame,
        ch: ch_blocks,
    }
}

/// One committed S4 invQuant level: initial formula (`inv=0`), single
/// assimilation (`inv=1`), full afterburner (`inv=2`). `scf` holds the
/// POST-inversion stored values (`maxSf - orig`, dead bands `0`);
/// `qspec`/`spec` the final lines and zeroed spectrum; `dist`/`en`/
/// `en_dist` the oracle-recomputed per-band distortion/energy with
/// `QSS = gain - scf` (diagnostic, mirrors S3DIST/S3EN — the internal
/// `sfbDist` scratch is not exposed by the exported call).
#[derive(Debug, Clone)]
pub(crate) struct S4Level {
    pub(crate) inv: i32,
    pub(crate) scf: Vec<i32>,
    pub(crate) gain: i32,
    pub(crate) qspec: Vec<i16>,
    pub(crate) spec: Vec<i32>,
    pub(crate) dist: Vec<i32>,
    pub(crate) en: Vec<i32>,
    pub(crate) en_dist: Vec<i32>,
}

/// One committed S4 channel: geometry plus the three inv levels.
#[derive(Debug, Clone)]
pub(crate) struct S4Channel {
    pub(crate) geo_cnt: usize,
    pub(crate) geo_per: usize,
    pub(crate) geo_max: usize,
    pub(crate) geo_seq: u8,
    pub(crate) offsets: Vec<i32>,
    pub(crate) levels: Vec<S4Level>,
}

/// One committed S4 element frame.
#[derive(Debug, Clone)]
pub(crate) struct S4Frame {
    pub(crate) frame: u32,
    pub(crate) ch: Vec<S4Channel>,
}

/// Load a committed S4 frame. Strict tag-order parsing: any oracle format
/// drift fails loudly here, not in numeric comparisons.
pub(crate) fn load_s4(name: &str, frame: u32) -> S4Frame {
    let text = std::fs::read_to_string(testdata_path(&format!("qc4_{name}_f{frame}.txt")))
        .expect("s4 testdata readable");
    let mut lines = text.lines();
    let head = lines.next().expect("S4FRAME head");
    assert_eq!(head, format!("S4FRAME f={frame}"));
    let mut pending: Vec<String> = Vec::new();
    for line in lines {
        if line.starts_with("S4END") {
            continue;
        }
        pending.push(line.to_string());
    }
    // Channel geometry blocks come first (S4GEO/S4OFF per channel).
    let mut geo: Vec<(Vec<i32>, Vec<i32>)> = Vec::new();
    let mut i = 0;
    while i < pending.len() && pending[i].starts_with("S4GEO ") {
        let g: Vec<i32> = pending[i]
            .split_whitespace()
            .skip(1)
            .map(|kv| {
                kv.split_once('=')
                    .expect("k=v")
                    .1
                    .parse::<i32>()
                    .expect("int")
            })
            .collect();
        assert_eq!(g.len(), 5); // ch cnt per max seq
        assert!(pending[i + 1].starts_with("S4OFF "), "S4OFF second");
        let offsets = parse_int_list(pending[i + 1].split_once(' ').expect("tag").1);
        assert_eq!(offsets.len(), g[1] as usize + 1, "S4OFF length");
        geo.push((g, offsets));
        i += 2;
    }
    assert!(!geo.is_empty(), "no S4 channel blocks");
    let nch = geo.len();
    let mut levels: Vec<Vec<S4Level>> = vec![Vec::new(); nch];
    for inv in [0, 1, 2] {
        for ch in 0..nch {
            let cnt = geo[ch].0[1] as usize;
            let ln = &pending[i];
            assert!(
                ln.starts_with(&format!("S4SCF inv={inv} ")),
                "S4SCF tag, got {}",
                &ln[..ln.len().min(16)]
            );
            // `S4SCF inv=<v> ch=<c> <cnt ints>`.
            let scf: Vec<i32> = ln
                .split_once(&format!("ch={ch}"))
                .expect("ch")
                .1
                .split_whitespace()
                .map(|v| v.parse::<i32>().expect("scf int"))
                .collect();
            assert_eq!(scf.len(), cnt, "S4SCF length");
            i += 1;
            let gl = &pending[i];
            assert!(gl.starts_with(&format!("S4GAIN inv={inv} ")), "S4GAIN tag");
            let gain: i32 = gl
                .split_whitespace()
                .nth(3)
                .expect("gain")
                .parse()
                .expect("gain int");
            i += 1;
            let ql = &pending[i];
            assert!(ql.starts_with(&format!("S4Q inv={inv} ")), "S4Q tag");
            let q: Vec<i16> = ql
                .split_once(&format!("ch={ch}"))
                .expect("ch")
                .1
                .split_whitespace()
                .map(|v| v.parse::<i32>().expect("q int") as i16)
                .collect();
            assert_eq!(q.len(), 1024, "S4Q length");
            i += 1;
            let sl = &pending[i];
            assert!(sl.starts_with(&format!("S4SPEC inv={inv} ")), "S4SPEC tag");
            let spec: Vec<i32> = sl
                .split_once(&format!("ch={ch}"))
                .expect("ch")
                .1
                .split_whitespace()
                .map(|v| v.parse::<i32>().expect("spec int"))
                .collect();
            assert_eq!(spec.len(), 1024, "S4SPEC length");
            i += 1;
            let dl = &pending[i];
            assert!(dl.starts_with(&format!("S4DIST inv={inv} ")), "S4DIST tag");
            let d: Vec<i32> =
                parse_int_list(dl.split_once(&format!("ch={ch}")).expect("ch").1.trim());
            assert_eq!(d.len(), cnt, "S4DIST length");
            i += 1;
            let el = &pending[i];
            assert!(el.starts_with(&format!("S4EN inv={inv} ")), "S4EN tag");
            let rest = el.split_once(&format!("ch={ch}")).expect("ch").1;
            let epart = rest.split_once(" en ").expect("en").1;
            let (e_str, d_str) = epart.split_once(" dist ").expect("dist");
            let e: Vec<i32> = parse_int_list(e_str);
            let ed: Vec<i32> = parse_int_list(d_str);
            assert_eq!(e.len(), cnt, "S4EN en length");
            assert_eq!(ed.len(), cnt, "S4EN dist length");
            i += 1;
            levels[ch].push(S4Level {
                inv,
                scf,
                gain,
                qspec: q,
                spec,
                dist: d,
                en: e,
                en_dist: ed,
            });
        }
    }
    assert_eq!(i, pending.len(), "trailing S4 lines");
    let ch_blocks = geo
        .into_iter()
        .zip(levels.into_iter())
        .map(|((g, offsets), lv)| S4Channel {
            geo_cnt: g[1] as usize,
            geo_per: g[2] as usize,
            geo_max: g[3] as usize,
            geo_seq: g[4] as u8,
            offsets,
            levels: lv,
        })
        .collect();
    S4Frame {
        frame,
        ch: ch_blocks,
    }
}

/// One committed S5 section: codebook + start/count (SFB units) +
/// section bits (Huffman + side info, like C `sectionBits`).
#[derive(Debug, Clone)]
pub(crate) struct S5Section {
    pub(crate) book: i32,
    pub(crate) start: i32,
    pub(crate) cnt: i32,
    pub(crate) bits: i32,
}

/// One committed S5 channel: geometry, QMAX (S6-owned
/// `calcMaxValueInSfb` output consumed as S5 input), the PNS-off
/// precondition vectors, and the sectioning + counts result.
#[derive(Debug, Clone)]
pub(crate) struct S5Channel {
    pub(crate) geo_cnt: usize,
    pub(crate) geo_per: usize,
    pub(crate) geo_max: usize,
    pub(crate) geo_seq: u8,
    pub(crate) offsets: Vec<i32>,
    pub(crate) qmax: Vec<u32>,
    pub(crate) is_book: Vec<i32>,
    pub(crate) is_scale: Vec<i32>,
    pub(crate) noise_nrg: Vec<i32>,
    pub(crate) sections: Vec<S5Section>,
    pub(crate) block: i32,
    pub(crate) side: i32,
    pub(crate) huff: i32,
    pub(crate) scf_bits: i32,
    pub(crate) noise_bits: i32,
    pub(crate) first: i32,
    pub(crate) dyn_used: i32,
}

/// One committed S5 element frame.
#[derive(Debug, Clone)]
pub(crate) struct S5Frame {
    pub(crate) frame: u32,
    pub(crate) ch: Vec<S5Channel>,
}

/// Load a committed S5 frame. Strict tag-order parsing: any oracle format
/// drift fails loudly here, not in numeric comparisons.
pub(crate) fn load_s5(name: &str, frame: u32) -> S5Frame {
    let text = std::fs::read_to_string(testdata_path(&format!("qc5_{name}_f{frame}.txt")))
        .expect("s5 testdata readable");
    let mut lines = text.lines();
    let head = lines.next().expect("S5FRAME head");
    assert_eq!(head, format!("S5FRAME f={frame}"));
    let mut pending: Vec<String> = Vec::new();
    for line in lines {
        if line.starts_with("S5END") {
            continue;
        }
        pending.push(line.to_string());
    }
    // Channel geometry blocks come first (S5GEO/S5OFF per channel).
    let mut geo: Vec<(Vec<i32>, Vec<i32>)> = Vec::new();
    let mut i = 0;
    while i < pending.len() && pending[i].starts_with("S5GEO ") {
        let g: Vec<i32> = pending[i]
            .split_whitespace()
            .skip(1)
            .map(|kv| {
                kv.split_once('=')
                    .expect("k=v")
                    .1
                    .parse::<i32>()
                    .expect("int")
            })
            .collect();
        assert_eq!(g.len(), 5); // ch cnt per max seq
        assert!(pending[i + 1].starts_with("S5OFF "), "S5OFF second");
        let offsets = parse_int_list(pending[i + 1].split_once(' ').expect("tag").1);
        assert_eq!(offsets.len(), g[1] as usize + 1, "S5OFF length");
        geo.push((g, offsets));
        i += 2;
    }
    assert!(!geo.is_empty(), "no S5 channel blocks");
    let nch = geo.len();
    let mut ch_blocks: Vec<S5Channel> = Vec::new();
    for ch in 0..nch {
        let cnt = geo[ch].0[1] as usize;
        let get_vals = |line: &str, tag: &str| -> Vec<i32> {
            assert!(
                line.starts_with(&format!("{tag} ch={ch}")),
                "{tag} tag, got {}",
                &line[..line.len().min(16)]
            );
            line.split_once(&format!("ch={ch}"))
                .expect("ch")
                .1
                .split_whitespace()
                .map(|v| v.parse::<i32>().expect("int"))
                .collect()
        };
        let qmax_line = &pending[i];
        assert!(
            qmax_line.starts_with(&format!("S5QMAX ch={ch}")),
            "S5QMAX tag"
        );
        let qmax: Vec<u32> = qmax_line
            .split_once(&format!("ch={ch}"))
            .expect("ch")
            .1
            .split_whitespace()
            .map(|v| v.parse::<u32>().expect("qmax int"))
            .collect();
        assert_eq!(qmax.len(), cnt, "S5QMAX length");
        for &q in &qmax {
            assert!(q <= 32768, "S5QMAX range");
        }
        i += 1;
        let is_book = get_vals(&pending[i], "S5ISB");
        assert_eq!(is_book.len(), cnt, "S5ISB length");
        i += 1;
        let is_scale = get_vals(&pending[i], "S5ISS");
        assert_eq!(is_scale.len(), cnt, "S5ISS length");
        i += 1;
        let noise_nrg = get_vals(&pending[i], "S5NOISE");
        assert_eq!(noise_nrg.len(), cnt, "S5NOISE length");
        i += 1;
        let sect_line = &pending[i];
        assert!(
            sect_line.starts_with(&format!("S5SECT ch={ch}")),
            "S5SECT tag"
        );
        let nsec: usize = sect_line
            .split("nsec=")
            .nth(1)
            .expect("nsec")
            .parse()
            .expect("nsec int");
        i += 1;
        let mut sections = Vec::new();
        for s in 0..nsec {
            let sl = &pending[i];
            assert!(sl.starts_with(&format!("S5SEC ch={ch}")), "S5SEC tag");
            let kv: Vec<(&str, i32)> = sl
                .split_whitespace()
                .skip(2)
                .map(|kv| {
                    let (k, v) = kv.split_once('=').expect("k=v");
                    (k, v.parse::<i32>().expect("sec int"))
                })
                .collect();
            assert_eq!(kv.len(), 5, "S5SEC fields");
            assert_eq!(kv[0].0, "i");
            assert_eq!(kv[0].1, s as i32, "S5SEC order");
            assert_eq!(kv[1].0, "book");
            assert_eq!(kv[2].0, "start");
            assert_eq!(kv[3].0, "cnt");
            assert_eq!(kv[4].0, "bits");
            sections.push(S5Section {
                book: kv[1].1,
                start: kv[2].1,
                cnt: kv[3].1,
                bits: kv[4].1,
            });
            i += 1;
        }
        let bl = &pending[i];
        assert!(bl.starts_with(&format!("S5BITS ch={ch}")), "S5BITS tag");
        let bv: Vec<i32> = bl
            .split_whitespace()
            .skip(2)
            .map(|kv| {
                kv.split_once('=')
                    .expect("k=v")
                    .1
                    .parse::<i32>()
                    .expect("bits int")
            })
            .collect();
        // block nsec side huff scf noise first dyn
        assert_eq!(bv.len(), 8, "S5BITS fields");
        assert_eq!(bv[1] as usize, nsec, "S5BITS nsec agreement");
        i += 1;
        ch_blocks.push(S5Channel {
            geo_cnt: cnt,
            geo_per: geo[ch].0[2] as usize,
            geo_max: geo[ch].0[3] as usize,
            geo_seq: geo[ch].0[4] as u8,
            offsets: geo[ch].1.clone(),
            qmax,
            is_book,
            is_scale,
            noise_nrg,
            sections,
            block: bv[0],
            side: bv[2],
            huff: bv[3],
            scf_bits: bv[4],
            noise_bits: bv[5],
            first: bv[6],
            dyn_used: bv[7],
        });
    }
    assert_eq!(i, pending.len(), "trailing S5 lines");
    S5Frame {
        frame,
        ch: ch_blocks,
    }
}

/// One committed S6 gain-loop pass (oracle `S6IT` line).
#[derive(Debug, Clone)]
pub(crate) struct S6Pass {
    pub(crate) iter: i32,
    pub(crate) dec: i32,
    pub(crate) dyn0: i32,
    pub(crate) dyn1: i32,
    pub(crate) gain0: i32,
    pub(crate) gain1: i32,
    pub(crate) maxq0: u32,
    pub(crate) maxq1: u32,
    pub(crate) eldyn: i32,
    pub(crate) elstat: i32,
    pub(crate) crash: i32,
}

/// One committed S6 element frame: bitrate/grant accounting, the per-pass
/// iteration trace and the fill/finalization result.
#[derive(Debug, Clone)]
pub(crate) struct S6Frame {
    pub(crate) frame: u32,
    pub(crate) avg_total: i32,
    pub(crate) glob_hdr: i32,
    pub(crate) pad_rest: i32,
    pub(crate) granted: i32,
    pub(crate) maxdyn: i32,
    pub(crate) avail: i32,
    pub(crate) elgrant: i32,
    pub(crate) gpe: i32,
    pub(crate) gpecorr: i32,
    pub(crate) reslevel: i32,
    pub(crate) resmax: i32,
    /// Pre-frame ATS grant state `(peLast, dynBitsLast, corrM, corrE,
    /// peMin, peMax)` for exact single-frame forging.
    pub(crate) gstate: Vec<i32>,
    pub(crate) passes: Vec<S6Pass>,
    pub(crate) static_: i32,
    pub(crate) useddyn: i32,
    pub(crate) totfill: i32,
    pub(crate) align: i32,
    pub(crate) total: i32,
    pub(crate) fgranted: i32,
    pub(crate) bitres: i32,
    pub(crate) fcrash: i32,
    pub(crate) npass: usize,
    /// Post-frame ATS grant state (same 6-tuple; evolution proof:
    /// PRE(f+1) == POST(f) on consecutive frames).
    pub(crate) gpost: Vec<i32>,
}

/// Committed S6 QC-init vectors (oracle `S6INIT`, from the real QCInit path).
#[derive(Debug, Clone)]
pub(crate) struct S6Init {
    pub(crate) avg: i32,
    pub(crate) max: i32,
    pub(crate) min: i32,
    pub(crate) bitres: i32,
    pub(crate) mode: i32,
    pub(crate) distmode: i32,
    pub(crate) meanpe: i32,
    pub(crate) maxbitfac: i32,
    pub(crate) chbitrate_el: i32,
    pub(crate) maxbits_el: i32,
    pub(crate) relbits_el: i32,
    pub(crate) b2m: i32,
    pub(crate) b2e: i32,
    pub(crate) pemin: i32,
    pub(crate) pemax: i32,
}

fn parse_kv_ints(line: &str) -> Vec<i32> {
    line.split_whitespace()
        .skip(1)
        .map(|kv| {
            kv.split_once('=')
                .expect("k=v")
                .1
                .parse::<i32>()
                .expect("int")
        })
        .collect()
}

/// Load a committed S6 frame. Strict tag-order parsing.
pub(crate) fn load_s6(name: &str, frame: u32) -> S6Frame {
    let text = std::fs::read_to_string(testdata_path(&format!("qc6_{name}_f{frame}.txt")))
        .expect("s6 testdata readable");
    let mut lines = text.lines();
    let head = lines.next().expect("S6FRAME head");
    assert_eq!(head, format!("S6FRAME f={frame}"));
    let bitr = parse_kv_ints(lines.next().expect("S6BITR"));
    assert_eq!(bitr.len(), 3, "S6BITR fields");
    let grant = parse_kv_ints(lines.next().expect("S6GRANT"));
    assert_eq!(grant.len(), 8, "S6GRANT fields");
    let gstate_line = lines.next().expect("S6GSTATE");
    assert!(gstate_line.starts_with("S6GSTATE "), "S6GSTATE tag");
    let gstate: Vec<i32> = gstate_line
        .split_whitespace()
        .skip(1)
        .map(|v| v.parse::<i32>().expect("gstate int"))
        .collect();
    assert_eq!(gstate.len(), 6, "S6GSTATE fields");
    let mut passes = Vec::new();
    for line in lines.by_ref() {
        if line.starts_with("S6IT ") {
            let v = parse_kv_ints(line);
            assert_eq!(v.len(), 12, "S6IT fields");
            let idx = passes.len() as i32;
            assert_eq!(v[0], idx, "S6IT pass order");
            passes.push(S6Pass {
                iter: v[1],
                dec: v[2],
                dyn0: v[3],
                dyn1: v[4],
                gain0: v[5],
                gain1: v[6],
                maxq0: v[7] as u32,
                maxq1: v[8] as u32,
                eldyn: v[9],
                elstat: v[10],
                crash: v[11],
            });
        } else if line.starts_with("S6FINAL ") {
            let b = parse_kv_ints(line);
            assert_eq!(b.len(), 9, "S6FINAL fields");
            assert_eq!(b[8] as usize, passes.len(), "S6FINAL npass agreement");
            let gpost_line = lines.next().expect("S6GPOST");
            assert!(gpost_line.starts_with("S6GPOST "), "S6GPOST tag");
            let gpost: Vec<i32> = gpost_line
                .split_whitespace()
                .skip(1)
                .map(|v| v.parse::<i32>().expect("gpost int"))
                .collect();
            assert_eq!(gpost.len(), 6, "S6GPOST fields");
            let end = lines.next().expect("S6END");
            assert!(end.starts_with("S6END"), "S6END tag");
            assert!(lines.next().is_none(), "trailing S6 lines");
            return S6Frame {
                frame,
                avg_total: bitr[0],
                glob_hdr: bitr[1],
                pad_rest: bitr[2],
                granted: grant[0],
                maxdyn: grant[1],
                avail: grant[2],
                elgrant: grant[3],
                gpe: grant[4],
                gpecorr: grant[5],
                reslevel: grant[6],
                resmax: grant[7],
                gstate,
                passes,
                static_: b[0],
                useddyn: b[1],
                totfill: b[2],
                align: b[3],
                total: b[4],
                fgranted: b[5],
                bitres: b[6],
                fcrash: b[7],
                npass: b[8] as usize,
                gpost,
            };
        } else {
            panic!("unexpected S6 line: {}", &line[..line.len().min(16)]);
        }
    }
    panic!("S6FINAL missing");
}

/// Load a committed S6 init fixture (`mono-48k`, `stereo-48k`,
/// `stereo-44100`).
pub(crate) fn load_s6init(tag: &str) -> S6Init {
    let text = std::fs::read_to_string(testdata_path(&format!("qc6init_{tag}.txt")))
        .expect("s6init testdata readable");
    let line = text.lines().next().expect("S6INIT line");
    assert!(line.starts_with("S6INIT "), "S6INIT tag");
    let mut v: Vec<i32> = Vec::new();
    let mut b2m = 0i32;
    let mut b2e = 0i32;
    for kv in line.split_whitespace().skip(1) {
        let (k, val) = kv.split_once('=').expect("k=v");
        if k == "bits2pe" {
            let (m, e) = val.split_once(',').expect("bits2pe m,e");
            b2m = m.parse::<i32>().expect("b2m int");
            b2e = e.parse::<i32>().expect("b2e int");
        } else {
            v.push(val.parse::<i32>().expect("int"));
        }
    }
    assert_eq!(v.len(), 13, "S6INIT fields");
    S6Init {
        avg: v[0],
        max: v[1],
        min: v[2],
        bitres: v[3],
        mode: v[4],
        distmode: v[5],
        meanpe: v[6],
        maxbitfac: v[7],
        chbitrate_el: v[8],
        maxbits_el: v[9],
        relbits_el: v[10],
        b2m,
        b2e,
        pemin: v[11],
        pemax: v[12],
    }
}

/// One synthetic S7 encode-probe vector from `s7probe.txt` (produced by
/// `tools/enc-oracle/bitenc_probe`, which calls the unmodified reference
/// `FDKaacEnc_codeValues` / `FDKaacEnc_codeScalefactorDelta` through a real
/// FDK writer bitstream).
#[derive(Debug, Clone)]
pub(crate) enum S7Probe {
    /// Spectral encode: `CV book=.. n=.. vals=.. bits=.. hex=..`.
    CodeValues {
        book: i32,
        vals: Vec<i16>,
        bits: u32,
        hex: String,
    },
    /// Scalefactor-delta encode: `CS delta=.. rc=.. bits=.. hex=..`
    /// (`rc=1` means the C error return: `|delta| > 60`, zero bits).
    CodeDelta {
        delta: i32,
        rc: i32,
        bits: u32,
        hex: String,
    },
}

/// Load every S7 probe vector. Strict: unknown tags, field drift, or value
/// overflow fails loudly here, not in numeric comparisons.
pub(crate) fn load_s7probe() -> Vec<S7Probe> {
    let text = std::fs::read_to_string(testdata_path("s7probe.txt")).expect("s7probe testdata");
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("S7TABLES")
            || line.starts_with("CTAB")
            || line.starts_with("S7VECTORS")
        {
            continue;
        }
        let toks: Vec<&str> = line.split_whitespace().collect();
        fn kval<'a>(toks: &[&'a str], i: usize) -> &'a str {
            toks[i]
                .split_once('=')
                .unwrap_or_else(|| panic!("k=v token: {}", toks[i]))
                .1
        }
        match toks[0] {
            "CV" => {
                assert_eq!(toks.len(), 6, "CV fields: {line}");
                let book = kval(&toks, 1).parse::<i32>().expect("book int");
                let n = kval(&toks, 2).parse::<usize>().expect("n int");
                let vals: Vec<i16> = kval(&toks, 3)
                    .split(',')
                    .map(|v| v.parse::<i32>().expect("val int") as i16)
                    .collect();
                assert_eq!(vals.len(), n, "CV vals length");
                let bits = kval(&toks, 4).parse::<u32>().expect("bits int");
                let hex = kval(&toks, 5).to_string();
                assert_eq!(hex.len(), ((bits as usize + 7) / 8) * 2, "CV hex length");
                out.push(S7Probe::CodeValues {
                    book,
                    vals,
                    bits,
                    hex,
                });
            }
            "CS" => {
                assert_eq!(toks.len(), 5, "CS fields: {line}");
                let delta = kval(&toks, 1).parse::<i32>().expect("delta int");
                let rc = kval(&toks, 2).parse::<i32>().expect("rc int");
                let bits = kval(&toks, 3).parse::<u32>().expect("bits int");
                let hex = kval(&toks, 4).to_string();
                assert_eq!(hex.len(), ((bits as usize + 7) / 8) * 2, "CS hex length");
                out.push(S7Probe::CodeDelta {
                    delta,
                    rc,
                    bits,
                    hex,
                });
            }
            _ => panic!("unknown s7probe line: {line}"),
        }
    }
    assert!(!out.is_empty(), "no S7 probe cases loaded");
    out
}

/// One committed S7 channel payload: emitted bits + MSB-first hex for the
/// spectral (`S7SPEC`), scalefactor (`S7SCF`) and section-header
/// (`S7SECH`) writers from the real reference encode functions.
#[derive(Debug, Clone)]
pub(crate) struct S7Channel {
    pub(crate) spec_bits: u32,
    pub(crate) spec_hex: String,
    pub(crate) scf_bits: u32,
    pub(crate) scf_hex: String,
    pub(crate) sech_bits: u32,
    pub(crate) sech_hex: String,
}

/// One committed S7 element frame.
#[derive(Debug, Clone)]
pub(crate) struct S7Frame {
    pub(crate) frame: u32,
    pub(crate) ch: Vec<S7Channel>,
}

/// Load a committed S7 frame. Strict tag-order parsing: any oracle format
/// drift fails loudly here, not in numeric comparisons.
pub(crate) fn load_s7(name: &str, frame: u32) -> S7Frame {
    let text = std::fs::read_to_string(testdata_path(&format!("qc7_{name}_f{frame}.txt")))
        .expect("s7 testdata readable");
    let mut lines = text.lines();
    let head = lines.next().expect("S7FRAME head");
    assert_eq!(head, format!("S7FRAME f={frame}"));
    let mut ch: Vec<S7Channel> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    for line in lines {
        if line.starts_with("S7END") {
            continue;
        }
        pending.push(line.to_string());
    }
    assert_eq!(pending.len() % 3, 0, "S7 triplets");
    fn parts(line: &str, tag: &str, ch_idx: usize) -> (u32, String) {
        let toks: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(toks.len(), 4, "S7 fields: {line}");
        assert_eq!(toks[0], tag, "S7 tag: {line}");
        assert_eq!(toks[1], format!("ch={ch_idx}"), "S7 order: {line}");
        let bits = toks[2]
            .split_once('=')
            .expect("bits k=v")
            .1
            .parse::<u32>()
            .expect("bits int");
        let hex = toks[3].split_once('=').expect("hex k=v").1.to_string();
        assert_eq!(hex.len(), ((bits as usize + 7) / 8) * 2, "S7 hex length");
        (bits, hex)
    }
    for (c, triplet) in pending.chunks_exact(3).enumerate() {
        let (spec_bits, spec_hex) = parts(&triplet[0], "S7SPEC", c);
        let (scf_bits, scf_hex) = parts(&triplet[1], "S7SCF", c);
        let (sech_bits, sech_hex) = parts(&triplet[2], "S7SECH", c);
        ch.push(S7Channel {
            spec_bits,
            spec_hex,
            scf_bits,
            scf_hex,
            sech_bits,
            sech_hex,
        });
    }
    S7Frame { frame, ch }
}

/// One committed S8 side-info channel (`S8SIDE`).
#[derive(Debug, Clone)]
pub(crate) struct S8Side {
    pub(crate) wshape: i32,
    pub(crate) gmask: i32,
    pub(crate) mdct: i32,
}

/// One committed S8 element frame: side info plus element/raw bit-exact
/// payloads and the fill/align/total cross-check.
#[derive(Debug, Clone)]
pub(crate) struct S8Frame {
    pub(crate) frame: u32,
    pub(crate) sides: Vec<S8Side>,
    pub(crate) elem_bits: u32,
    pub(crate) elem_hex: String,
    pub(crate) raw_bits: u32,
    pub(crate) raw_hex: String,
    pub(crate) sstatic: i32,
    pub(crate) useddyn: i32,
    pub(crate) totfill: i32,
    pub(crate) align: i32,
    pub(crate) total: i32,
}

/// Load a committed S8 frame. Strict tag-order parsing: any oracle format
/// drift fails loudly here, not in numeric comparisons.
pub(crate) fn load_s8(name: &str, frame: u32) -> S8Frame {
    let text = std::fs::read_to_string(testdata_path(&format!("qc8_{name}_f{frame}.txt")))
        .expect("s8 testdata readable");
    let mut lines = text.lines();
    let head = lines.next().expect("S8FRAME head");
    assert_eq!(head, format!("S8FRAME f={frame}"));
    let mut pending: Vec<String> = Vec::new();
    for line in lines {
        if line.starts_with("S8END") {
            continue;
        }
        pending.push(line.to_string());
    }
    // sides (one per channel) + S8ELEM + S8RAW + S8FINAL.
    assert!(pending.len() >= 4, "S8 block");
    let nch = pending.len() - 3;
    let mut sides = Vec::new();
    for (c, ln) in pending[..nch].iter().enumerate() {
        assert!(ln.starts_with("S8SIDE "), "S8SIDE tag: {ln}");
        assert!(ln.contains(&format!("ch={c} ")), "S8SIDE order: {ln}");
        let kv: std::collections::HashMap<String, String> = ln
            .split_whitespace()
            .skip(1)
            .filter(|tok| tok.contains('='))
            .map(|kv| {
                let (k, v) = kv.split_once('=').expect("k=v");
                (k.to_string(), v.to_string())
            })
            .collect();
        sides.push(S8Side {
            wshape: kv["wshape"].parse().expect("int"),
            gmask: kv["gmask"].parse().expect("int"),
            mdct: kv["mdct"].parse().expect("int"),
        });
    }
    let elem_line = &pending[nch];
    let raw_line = &pending[nch + 1];
    let final_line = &pending[nch + 2];
    assert!(elem_line.starts_with("S8ELEM "), "S8ELEM tag");
    assert!(raw_line.starts_with("S8RAW "), "S8RAW tag");
    assert!(final_line.starts_with("S8FINAL "), "S8FINAL tag");
    fn bits_hex(line: &str, tag: &str) -> (u32, String) {
        let toks: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(toks.len(), 3, "S8 bits/hex fields: {line}");
        assert_eq!(toks[0], tag, "S8 tag: {line}");
        let bits = toks[1]
            .split_once('=')
            .expect("bits")
            .1
            .parse::<u32>()
            .expect("bits int");
        let hex = toks[2].split_once('=').expect("hex").1.to_string();
        assert_eq!(hex.len(), ((bits as usize + 7) / 8) * 2, "S8 hex length");
        (bits, hex)
    }
    let (elem_bits, elem_hex) = bits_hex(elem_line, "S8ELEM");
    let (raw_bits, raw_hex) = bits_hex(raw_line, "S8RAW");
    let fkv: std::collections::HashMap<String, String> = final_line
        .split_whitespace()
        .skip(1)
        .map(|kv| {
            let (k, v) = kv.split_once('=').expect("k=v");
            (k.to_string(), v.to_string())
        })
        .collect();
    S8Frame {
        frame,
        sides,
        elem_bits,
        elem_hex,
        raw_bits,
        raw_hex,
        sstatic: fkv["static"].parse().expect("int"),
        useddyn: fkv["useddyn"].parse().expect("int"),
        totfill: fkv["totfill"].parse().expect("int"),
        align: fkv["align"].parse().expect("int"),
        total: fkv["total"].parse().expect("int"),
    }
}
