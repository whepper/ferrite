//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! Committed init-time encoder configuration snapshots.
//!
//! Production module (no `#[cfg(test)]`): the streaming encoder needs the
//! `InitPsyConfiguration` / `InitTnsConfiguration` dumps at table-build
//! time (`StreamEncoder::new`), so the loader lives here rather than in the
//! test-gated `testdata` module. Contents are moved verbatim from
//! `testdata.rs` (M6.1 relocation, no behavior change); `testdata` re-exports
//! them so existing test call sites are untouched.
//!
//! M6.3 packaging: the four validated snapshots are embedded at compile time
//! with `include_str!`, so installed binaries and downstream builds have no
//! runtime dependency on the crate source tree. A missing snapshot is a
//! compile-time error, never a deployment-time panic.
//!
//! Provenance: the original four snapshots were extracted from real-C
//! `enc_oracle` dumps by `tools/enc-oracle/extract_testdata.py` (M5.e
//! documents the `cfg_44100_112000.txt` addition via the same workflow).
//! The fourteen VBR-1..4 snapshots were generated the same way: by driving
//! the UNMODIFIED FDK-AAC v3.0.0 implementation (`mstorsjo/fdk-aac`, tag
//! `v3.0.0`, commit `a68a51aa45d86a43c7b228e3d45624af09dcdd8d`) through its
//! real init path (`FDKaacEnc_InitPsyConfiguration` for LONG and SHORT
//! windows plus `FDKaacEnc_InitTnsConfiguration`), then writing the dumps
//! in this exact file format. The generator re-emits the four pre-existing
//! snapshots byte-identically as a self-check before trusting new files,
//! so the new snapshots reproduce the established oracle workflow rather
//! than a parallel implementation. VBR-6 reuses the VBR-5 snapshots by
//! Ferrite definition (same per-channel bitrates); see
//! [`load_cfg_for_profile`].

const CFG_44100_20000: &str = include_str!("testdata/cfg_44100_20000.txt");
const CFG_44100_32000: &str = include_str!("testdata/cfg_44100_32000.txt");
const CFG_44100_40000: &str = include_str!("testdata/cfg_44100_40000.txt");
const CFG_44100_48000: &str = include_str!("testdata/cfg_44100_48000.txt");
const CFG_44100_56000: &str = include_str!("testdata/cfg_44100_56000.txt");
const CFG_44100_64000: &str = include_str!("testdata/cfg_44100_64000.txt");
const CFG_44100_72000: &str = include_str!("testdata/cfg_44100_72000.txt");
const CFG_44100_96000: &str = include_str!("testdata/cfg_44100_96000.txt");
const CFG_44100_112000: &str = include_str!("testdata/cfg_44100_112000.txt");
const CFG_48000_20000: &str = include_str!("testdata/cfg_48000_20000.txt");
const CFG_48000_32000: &str = include_str!("testdata/cfg_48000_32000.txt");
const CFG_48000_40000: &str = include_str!("testdata/cfg_48000_40000.txt");
const CFG_48000_48000: &str = include_str!("testdata/cfg_48000_48000.txt");
const CFG_48000_56000: &str = include_str!("testdata/cfg_48000_56000.txt");
const CFG_48000_64000: &str = include_str!("testdata/cfg_48000_64000.txt");
const CFG_48000_72000: &str = include_str!("testdata/cfg_48000_72000.txt");
const CFG_48000_96000: &str = include_str!("testdata/cfg_48000_96000.txt");
const CFG_48000_112000: &str = include_str!("testdata/cfg_48000_112000.txt");

/// Compile-time snapshot for the supported operating points. `pch` is the
/// per-channel bitrate the encoder requests: 20000 (VBR-1 stereo), 32000
/// (VBR-1 mono / VBR-2 stereo), 40000 (VBR-2 mono), 48000 (VBR-3 stereo),
/// 56000 (VBR-3 mono), 64000 (VBR-4 stereo), 72000 (VBR-4 mono), 96000
/// (VBR-5/VBR-6 stereo), 112000 (VBR-5/VBR-6 mono).
///
/// Snapshots are keyed by `(sample_rate, pch)`, so genuinely shared
/// reference data is naturally shared: VBR-1 mono and VBR-2 stereo resolve
/// to the same file (identical per-channel bitrate and bandwidth produce
/// byte-identical FDK dumps, verified during generation, not assumed).
fn cfg_text(sample_rate: u32, pch: u32) -> &'static str {
    match (sample_rate, pch) {
        (44100, 20000) => CFG_44100_20000,
        (44100, 32000) => CFG_44100_32000,
        (44100, 40000) => CFG_44100_40000,
        (44100, 48000) => CFG_44100_48000,
        (44100, 56000) => CFG_44100_56000,
        (44100, 64000) => CFG_44100_64000,
        (44100, 72000) => CFG_44100_72000,
        (44100, 96000) => CFG_44100_96000,
        (44100, 112000) => CFG_44100_112000,
        (48000, 20000) => CFG_48000_20000,
        (48000, 32000) => CFG_48000_32000,
        (48000, 40000) => CFG_48000_40000,
        (48000, 48000) => CFG_48000_48000,
        (48000, 56000) => CFG_48000_56000,
        (48000, 64000) => CFG_48000_64000,
        (48000, 72000) => CFG_48000_72000,
        (48000, 96000) => CFG_48000_96000,
        (48000, 112000) => CFG_48000_112000,
        _ => panic!(
            "no committed config snapshot for {sample_rate} Hz / {pch} bit/s \
             (supported: 44100/48000 Hz x VBR-1..6 per-channel bitrates)"
        ),
    }
}

/// Load committed config snapshots: ((long, long-tns), (short, short-tns)).
/// `pch` is the per-channel bitrate selecting the embedded snapshot.
/// Committed PSY_CONFIGURATION snapshot (long or short) for one rate and
/// per-channel bitrate. Fields are consumed by gated tests and table setup;
/// `allow(dead_code)` because production builds read only a subset.
#[allow(dead_code)]
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

/// Committed TNS configuration snapshot (same consumption note as `CfgSnap`).
#[allow(dead_code)]
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

pub(crate) fn parse_kv_list(line: &str) -> Vec<(String, String)> {
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
/// `pch` is the per-channel bitrate selecting the embedded snapshot.
/// Every per-channel bitrate used by VBR-1..6 at 44100/48000 Hz has an
/// exact snapshot; anything else panics fail-closed (no silent fallback
/// to another operating point).
pub(crate) fn load_cfg(
    sample_rate: u32,
    pch: u32,
) -> ((CfgSnap, TnsCfgSnap), (CfgSnap, TnsCfgSnap)) {
    let text = cfg_text(sample_rate, pch);
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

/// Load `minSnr` snapshots for a VBR profile.
///
/// Snapshots are keyed by `(sample_rate, per-channel bitrate)`, so every
/// VBR-1..5 operating point at 44100/48000 Hz resolves to its own exact
/// FDK-AAC v3.0.0 reference data. VBR-5 and VBR-6 share per-channel bitrates
/// (96 000 stereo, 112 000 mono), so VBR-6 reuses the VBR-5 snapshots by
/// Ferrite definition (same `minSnr` input bitrate; the bandwidth difference
/// is carried by the SFB/low-pass/active-band path, and the resulting
/// VBR-6 output is pinned by the existing regression baselines).
/// VBR-1 mono and VBR-2 stereo share pch=32000 at the same rate and
/// bandwidth; their FDK dumps are byte-identical, so one file serves both
/// (verified during generation, not assumed).
pub(crate) fn load_cfg_for_profile(
    sample_rate: u32,
    profile: crate::aac_enc::config::VbrProfile,
    is_mono: bool,
) -> ((CfgSnap, TnsCfgSnap), (CfgSnap, TnsCfgSnap)) {
    let pch = profile.channel_bitrate(is_mono);
    load_cfg(sample_rate, pch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::config::VbrMode;

    /// Every embedded snapshot parses with structurally consistent arrays.
    /// This is the clean-checkout guard: a missing or truncated asset fails
    /// the build (include_str!) or this test, never a runtime deployment.
    #[test]
    fn all_validated_snapshots_parse() {
        for (sr, pch) in [
            (44100, 20000),
            (44100, 32000),
            (44100, 40000),
            (44100, 48000),
            (44100, 56000),
            (44100, 64000),
            (44100, 72000),
            (44100, 96000),
            (44100, 112000),
            (48000, 20000),
            (48000, 32000),
            (48000, 40000),
            (48000, 48000),
            (48000, 56000),
            (48000, 64000),
            (48000, 72000),
            (48000, 96000),
            (48000, 112000),
        ] {
            let ((long, long_tns), (short, short_tns)) = load_cfg(sr, pch);
            assert!(long.sfb_cnt > 0, "{sr}/{pch} long sfb_cnt");
            assert!(short.sfb_cnt > 0, "{sr}/{pch} short sfb_cnt");
            assert_eq!(long.offsets.len(), long.sfb_cnt + 1, "{sr}/{pch} offsets");
            assert_eq!(short.offsets.len(), short.sfb_cnt + 1, "{sr}/{pch} offsets");
            assert!(!long.min_snr_ld.is_empty(), "{sr}/{pch} long minsnr");
            assert!(!short.min_snr_ld.is_empty(), "{sr}/{pch} short minsnr");
            assert!(long_tns.stop_band >= 0 && short_tns.stop_band >= 0);
        }
    }

    /// Every VBR-1..6 operating point at 44100/48000 Hz resolves to an exact
    /// snapshot. VBR-6 shares VBR-5's per-channel bitrates by
    /// Ferrite definition, so it resolves to the same files.
    #[test]
    fn every_profile_resolves_to_exact_snapshot() {
        for mode in VbrMode::ALL {
            for sr in [44100, 48000] {
                for is_mono in [true, false] {
                    let profile = mode.profile();
                    let ((long, _), (short, _)) = load_cfg_for_profile(sr, profile, is_mono);
                    assert!(!long.min_snr_ld.is_empty());
                    assert!(!short.min_snr_ld.is_empty());
                }
            }
        }
    }

    /// Unvalidated combinations have no embedded table and fail loudly rather
    /// than silently falling back to another operating point.
    #[test]
    #[should_panic(expected = "no committed config snapshot")]
    fn unsupported_combination_panics() {
        let _ = load_cfg(32000, 96000);
    }
}
