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
//! AAC-LC VBR encoder configuration (VBR-1 through VBR-6).
//!
//! Mirrors the parameter plane enforced by `FDKaacEnc_AdjustEncSettings`
//! (`cpp/libAACenc/src/aacenc_lib.cpp:544-682`) and the sanity checks in
//! `FDKaacEnc_Initialize` (`cpp/libAACenc/src/aacenc.cpp:442-514`): AAC-LC
//! only, VBR modes 1–5 from upstream FDK v3.0.0 plus Ferrite VBR-6,
//! afterburner always on, ADTS/RAW transport only.
//! The user bitrate is ignored, exactly like the C++ VBR path
//! (`aacenc_lib.cpp:618-622`).
//!
//! VBR-6 is a Ferrite extension: it inherits every VBR-5 setting except the
//! configured bandwidth (20 000 Hz instead of 19 293 Hz). It is the default
//! when no profile is explicitly selected. VBR-5 remains available unchanged
//! as the upstream-compatible baseline.

use super::channel_map::ChannelMode;
use core::fmt;

use super::adts::{sample_rate_index, AdtsError};

/// Audio object type: AAC-LC only (`aacenc_lib.cpp:557-560`).
pub const AOT_AAC_LC: u32 = 2;
/// Bitrate mode identifiers (`aacenc_lib.h`, `aacenc.h:189-197`).
/// Upstream FDK defines VBR modes 1–5; VBR-6 is a Ferrite extension.
pub const BITRATE_MODE_VBR_1: u32 = 1;
pub const BITRATE_MODE_VBR_2: u32 = 2;
pub const BITRATE_MODE_VBR_3: u32 = 3;
pub const BITRATE_MODE_VBR_4: u32 = 4;
/// Bitrate mode: VBR 5 (`aacenc_lib.cpp:561-563` in the stripped tree).
pub const BITRATE_MODE_VBR_5: u32 = 5;
/// Bitrate mode: VBR 6 (Ferrite extension, not upstream FDK).
pub const BITRATE_MODE_VBR_6: u32 = 6;
/// VBR-5 quality factor, `FL2FXCONST_DBL(0.070f)` (`qc_main.cpp:131-132`,
/// v3.0.0 `0.070f` row).
pub const VBR_5_QUAL_FACTOR: f32 = 0.070;
/// VBR-5 effective bandwidth in Hz, mono and stereo (`bandwidth.cpp:158`,
/// v3.0.0 row).
pub const VBR_5_BANDWIDTH_HZ: u32 = 19293;
/// VBR-5 nominal per-channel bitrate, mono (`aacenc.cpp:205`, v3.0.0 row).
pub const VBR_5_CHAN_BITRATE_MONO: u32 = 112_000;
/// VBR-5 nominal per-channel bitrate, stereo and up (`aacenc.cpp:205`,
/// v3.0.0 row).
pub const VBR_5_CHAN_BITRATE_STEREO: u32 = 96_000;
/// VBR-6 configured bandwidth in Hz (Ferrite extension; VBR-5 is 19 293 Hz).
pub const VBR_6_BANDWIDTH_HZ: u32 = 20_000;
/// Long frame length for AAC-LC (`aacenc_lib.cpp:608-613`).
///
/// Note: `AdjustEncSettings` also passes 960 through, but
/// `FDKaacEnc_Initialize` rejects it for non-low-delay AOTs
/// (`aacenc.cpp:496-514`), so only 1024 is constructible here.
pub const FRAME_LENGTH: usize = 1024;
/// Sample rates accepted for AAC-LC (`aacenc.cpp:452-468`).
pub const SUPPORTED_SAMPLE_RATES: [u32; 12] = [
    8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000,
];

/// Output transport. The stripped encoder supports ADTS and RAW only;
/// LATM/LOAS/ADIF are rejected (`aacenc_lib.cpp:593-607`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// Raw AAC access units (ASC out-of-band).
    Raw,
    /// ADTS framing (default, as in `aac-enc.c`).
    Adts,
}

/// Configuration rejection reasons. Each maps to an `AACENC_INVALID_CONFIG`
/// return on the corresponding C++ path cited per variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// Not in the `aacenc.cpp:452-468` whitelist.
    UnsupportedSampleRate(u32),
    /// No channel-mode row for this count (`aacenc_lib.cpp:668-671`).
    UnsupportedChannelCount(u32),
    /// Not 1024 (`aacenc_lib.cpp:611-613`, `aacenc.cpp:496-514`).
    UnsupportedFrameLength(usize),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::UnsupportedSampleRate(rate) => {
                write!(f, "unsupported sample rate: {rate}")
            }
            ConfigError::UnsupportedChannelCount(ch) => {
                write!(f, "unsupported channel count: {ch}")
            }
            ConfigError::UnsupportedFrameLength(fl) => {
                write!(f, "unsupported frame length: {fl}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// VBR profile identifier. VBR-1 through VBR-5 are the upstream FDK AAC
/// modes (`aacenc.h:189-197`, `qc_data.h:114-124`, v3.0.0 tables);
/// VBR-6 is a Ferrite extension (VBR-5 settings with 20 kHz bandwidth).
/// Discriminants match the C enum values (`AACENC_BR_MODE_VBR_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VbrMode {
    /// Upstream FDK VBR-1 (very low bitrate).
    Vbr1 = 1,
    /// Upstream FDK VBR-2.
    Vbr2 = 2,
    /// Upstream FDK VBR-3.
    Vbr3 = 3,
    /// Upstream FDK VBR-4.
    Vbr4 = 4,
    /// Upstream FDK VBR-5 (very high bitrate baseline).
    Vbr5 = 5,
    /// Ferrite extension: VBR-5 settings with 20 000 Hz bandwidth.
    /// Default when no profile is explicitly selected.
    Vbr6 = 6,
}

impl VbrMode {
    /// All selectable profiles in order.
    pub const ALL: [VbrMode; 6] = [
        VbrMode::Vbr1,
        VbrMode::Vbr2,
        VbrMode::Vbr3,
        VbrMode::Vbr4,
        VbrMode::Vbr5,
        VbrMode::Vbr6,
    ];

    /// Profile number (1–6) as used by `--vbr` and the C enum.
    pub fn number(self) -> u32 {
        self as u32
    }

    /// Parse a profile number (1–6). Rejects anything else.
    pub fn from_number(n: u32) -> Option<Self> {
        match n {
            1 => Some(VbrMode::Vbr1),
            2 => Some(VbrMode::Vbr2),
            3 => Some(VbrMode::Vbr3),
            4 => Some(VbrMode::Vbr4),
            5 => Some(VbrMode::Vbr5),
            6 => Some(VbrMode::Vbr6),
            _ => None,
        }
    }

    /// Central profile definition (see [`VbrProfile`]).
    pub fn profile(self) -> VbrProfile {
        VbrProfile::for_mode(self)
    }
}

impl fmt::Display for VbrMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VBR-{}", self.number())
    }
}

impl core::str::FromStr for VbrMode {
    type Err = VbrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        // Accept "1".."6", "vbr1".."vbr6", "VBR-1".. etc. for CLI convenience.
        let digits = trimmed
            .strip_prefix("VBR-")
            .or_else(|| trimmed.strip_prefix("vbr-"))
            .or_else(|| trimmed.strip_prefix("VBR"))
            .or_else(|| trimmed.strip_prefix("vbr"))
            .unwrap_or(trimmed);
        match digits.parse::<u32>() {
            Ok(n) => VbrMode::from_number(n).ok_or(VbrParseError(n.to_string())),
            Err(_) => Err(VbrParseError(s.to_string())),
        }
    }
}

/// Rejection reason for an invalid VBR profile number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VbrParseError(pub String);

impl fmt::Display for VbrParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid VBR profile: {} (expected 1 through 6)", self.0)
    }
}

impl std::error::Error for VbrParseError {}

/// Profile origin: upstream FDK AAC vs Ferrite extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VbrOrigin {
    /// Parameter row from upstream Fraunhofer FDK AAC v3.0.0.
    UpstreamFdk,
    /// Ferrite-specific extension (not an upstream FDK profile).
    FerriteExtension,
}

impl fmt::Display for VbrOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VbrOrigin::UpstreamFdk => write!(f, "FDK"),
            VbrOrigin::FerriteExtension => write!(f, "Ferrite"),
        }
    }
}

/// Central VBR profile definition.
///
/// Mono and stereo values below are total nominal bitrates. In the original
/// FDK configuration table (`aacenc.cpp:199-205`) the stereo values are
/// specified per channel; the effective stereo total is twice that value.
/// This struct preserves that distinction: `stereo_per_channel_bitrate` is
/// the table entry, `stereo_total_bitrate()` derives the total.
///
/// Bandwidth is the configured maximum coding bandwidth in Hz, subject to
/// the sample-rate Nyquist limit (`min(bandwidth, sample_rate/2)`).
/// Quality factor is the internal psychoacoustic threshold-adjustment
/// parameter (`tableVbrQualFactor`, `qc_main.cpp:122-133`), not a user-facing
/// quality score or bitrate multiplier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VbrProfile {
    mode: VbrMode,
    name: &'static str,
    origin: VbrOrigin,
    mono_bitrate: u32,
    stereo_per_channel_bitrate: u32,
    bandwidth_hz: u32,
    qual_factor: f32,
    qual_q31: i32,
}

impl VbrProfile {
    /// Look up the central definition for a mode.
    pub fn for_mode(mode: VbrMode) -> Self {
        match mode {
            VbrMode::Vbr1 => Self {
                mode,
                name: "VBR-1",
                origin: VbrOrigin::UpstreamFdk,
                mono_bitrate: 32_000,
                stereo_per_channel_bitrate: 20_000,
                bandwidth_hz: 13_000,
                qual_factor: 0.150,
                // FL2FXCONST_DBL(0.150f): (float)0.15 * 2^31.
                qual_q31: 0x13333340,
            },
            VbrMode::Vbr2 => Self {
                mode,
                name: "VBR-2",
                origin: VbrOrigin::UpstreamFdk,
                mono_bitrate: 40_000,
                stereo_per_channel_bitrate: 32_000,
                bandwidth_hz: 13_000,
                qual_factor: 0.162,
                // FL2FXCONST_DBL(0.162f).
                qual_q31: 0x14bc6a80,
            },
            VbrMode::Vbr3 => Self {
                mode,
                name: "VBR-3",
                origin: VbrOrigin::UpstreamFdk,
                mono_bitrate: 56_000,
                stereo_per_channel_bitrate: 48_000,
                bandwidth_hz: 15_750,
                qual_factor: 0.176,
                // FL2FXCONST_DBL(0.176f).
                qual_q31: 0x16872b00,
            },
            VbrMode::Vbr4 => Self {
                mode,
                name: "VBR-4",
                origin: VbrOrigin::UpstreamFdk,
                mono_bitrate: 72_000,
                stereo_per_channel_bitrate: 64_000,
                bandwidth_hz: 16_500,
                qual_factor: 0.120,
                // FL2FXCONST_DBL(0.120f).
                qual_q31: 0x0f5c28f0,
            },
            VbrMode::Vbr5 => Self {
                mode,
                name: "VBR-5",
                origin: VbrOrigin::UpstreamFdk,
                mono_bitrate: 112_000,
                stereo_per_channel_bitrate: 96_000,
                bandwidth_hz: 19_293,
                qual_factor: 0.070,
                // FL2FXCONST_DBL(0.070f) = C-probed 0x08f5c290.
                qual_q31: 0x08f5c290,
            },
            VbrMode::Vbr6 => Self {
                mode,
                name: "VBR-6",
                origin: VbrOrigin::FerriteExtension,
                mono_bitrate: 112_000,
                stereo_per_channel_bitrate: 96_000,
                bandwidth_hz: 20_000,
                qual_factor: 0.070,
                // Same as VBR-5 (inherits all VBR-5 settings except bandwidth).
                qual_q31: 0x08f5c290,
            },
        }
    }

    /// Profile identifier.
    pub fn mode(self) -> VbrMode {
        self.mode
    }
    /// Profile name (`VBR-1` … `VBR-6`).
    pub fn name(self) -> &'static str {
        self.name
    }
    /// Origin (upstream FDK vs Ferrite extension).
    pub fn origin(self) -> VbrOrigin {
        self.origin
    }
    /// Mono total nominal bitrate in bit/s.
    pub fn mono_bitrate(self) -> u32 {
        self.mono_bitrate
    }
    /// Stereo per-channel nominal bitrate (FDK table entry) in bit/s.
    pub fn stereo_per_channel_bitrate(self) -> u32 {
        self.stereo_per_channel_bitrate
    }
    /// Stereo total nominal bitrate in bit/s (2 × per-channel entry).
    pub fn stereo_total_bitrate(self) -> u32 {
        self.stereo_per_channel_bitrate * 2
    }
    /// Configured bandwidth limit in Hz (before Nyquist clamping).
    pub fn bandwidth_hz(self) -> u32 {
        self.bandwidth_hz
    }
    /// VBR quality factor (float form).
    pub fn qual_factor(self) -> f32 {
        self.qual_factor
    }
    /// VBR quality factor in Q31 (`FL2FXCONST_DBL` form).
    pub fn qual_q31(self) -> i32 {
        self.qual_q31
    }

    /// Per-channel nominal bitrate for a mono (`true`) or stereo (`false`)
    /// configuration. Multichannel configurations use the stereo per-channel
    /// entry, matching `FDKaacEnc_GetVBRBitrate` (`aacenc.cpp:216-246`).
    pub fn channel_bitrate(self, is_mono: bool) -> u32 {
        if is_mono {
            self.mono_bitrate
        } else {
            self.stereo_per_channel_bitrate
        }
    }
}

/// Psychoacoustic engine selection (FPE planning ADR-0001,
/// `docs/fpe/06-encoder-integration.md` §1–§2).
///
/// [`PsyEngineKind::Q31`] is the validated bit-faithful fixed-point engine
/// and the **default**; [`PsyEngineKind::Fpe`] selects the floating-point
/// psychoacoustic engine under `aac_enc::fpe` (M3/FPE-3.1: float rescale
/// band energy; remaining value kernels still delegate to Q31). Runtime
/// enum, not a Cargo feature, so both engines live in one binary for the
/// in-binary Q31↔FPE differential harness.
///
/// **Unstable:** this knob is test-selectable from M1/M2; documented,
/// stable public exposure happens at M8 after Stage-1 validation
/// (`docs/fpe/06` §2). Making FPE the default is out of Stage 1.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PsyEngineKind {
    /// Validated Q31 fixed-point psychoacoustic engine (default).
    #[default]
    Q31,
    /// Floating-point psychoacoustic engine (FPE), independently
    /// selectable for differential testing; never default in Stage 1.
    Fpe,
}

/// Validated encoder configuration for the AAC-LC VBR path.
///
/// Afterburner is not a field: it is unconditionally on
/// (`aacenc_lib.cpp:564-565,574`, `aacenc.cpp:716` with `invQuant=2`).
/// PNS is effectively off for every VBR profile by FDK's post-configuration
/// logic (`aacenc_lib.cpp:1173-1176`: VBR without PNS; also disabled when
/// TNS is off), even though the AAC-LC defaults initially enable it.
/// TNS, M/S and conditional intensity stereo follow the shared AAC-LC path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderConfig {
    sample_rate: u32,
    channels: u32,
    frame_length: usize,
    transport: TransportType,
    vbr_mode: VbrMode,
    /// Psychoacoustic engine selection (additive, default Q31; FPE
    /// planning `docs/fpe/06` §2). Existing constructors are unchanged.
    psy_engine: PsyEngineKind,
}

impl EncoderConfig {
    /// Validate and build a configuration with the default profile (VBR-6).
    /// Fails exactly where the C++ paths return `AACENC_INVALID_CONFIG` /
    /// `AAC_ENC_UNSUPPORTED_*`.
    pub fn new(
        sample_rate: u32,
        channels: u32,
        frame_length: usize,
        transport: TransportType,
    ) -> Result<Self, ConfigError> {
        Self::new_with_mode(
            sample_rate,
            channels,
            frame_length,
            transport,
            VbrMode::Vbr6,
        )
    }

    /// Validate and build a configuration with an explicit VBR profile.
    pub fn new_with_mode(
        sample_rate: u32,
        channels: u32,
        frame_length: usize,
        transport: TransportType,
        vbr_mode: VbrMode,
    ) -> Result<Self, ConfigError> {
        if !SUPPORTED_SAMPLE_RATES.contains(&sample_rate) {
            return Err(ConfigError::UnsupportedSampleRate(sample_rate));
        }
        if ChannelMode::from_channel_count(channels).is_none() {
            return Err(ConfigError::UnsupportedChannelCount(channels));
        }
        if frame_length != FRAME_LENGTH {
            return Err(ConfigError::UnsupportedFrameLength(frame_length));
        }
        Ok(Self {
            sample_rate,
            channels,
            frame_length,
            transport,
            vbr_mode,
            psy_engine: PsyEngineKind::default(),
        })
    }

    /// Primary quality target: stereo music at 48 kHz over ADTS (VBR-6).
    pub fn stereo_48k_adts() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            frame_length: FRAME_LENGTH,
            transport: TransportType::Adts,
            vbr_mode: VbrMode::Vbr6,
            psy_engine: PsyEngineKind::default(),
        }
    }

    /// Builder: return a copy with an explicit VBR profile.
    pub fn with_vbr_mode(self, vbr_mode: VbrMode) -> Self {
        Self { vbr_mode, ..self }
    }

    /// Builder: return a copy with an explicit psychoacoustic engine
    /// (additive, mirrors [`EncoderConfig::with_vbr_mode`]; FPE planning
    /// `docs/fpe/06` §2). Default remains Q31; FPE is independently
    /// selectable for differential testing.
    ///
    /// **Unstable:** test-selectable from M1/M2; documented, stable public
    /// exposure happens at M8 after Stage-1 validation.
    #[doc(hidden)]
    pub fn with_psy_engine(self, psy_engine: PsyEngineKind) -> Self {
        Self { psy_engine, ..self }
    }

    /// Selected psychoacoustic engine (default Q31).
    ///
    /// **Unstable:** see [`EncoderConfig::with_psy_engine`].
    #[doc(hidden)]
    pub fn psy_engine(self) -> PsyEngineKind {
        self.psy_engine
    }

    /// Selected VBR profile identifier.
    pub fn vbr_mode(self) -> VbrMode {
        self.vbr_mode
    }

    /// Central profile definition for the selected mode.
    pub fn profile(self) -> VbrProfile {
        self.vbr_mode.profile()
    }

    /// Input sample rate in Hz.
    pub fn sample_rate(self) -> u32 {
        self.sample_rate
    }

    /// Input channel count.
    pub fn channels(self) -> u32 {
        self.channels
    }

    /// Frame length in samples per channel (always 1024).
    pub fn frame_length(self) -> usize {
        self.frame_length
    }

    /// Output transport.
    pub fn transport(self) -> TransportType {
        self.transport
    }

    /// Channel mode derived from the channel count
    /// (`FDKaacEnc_DetermineEncoderMode`, `channel_map.cpp:157-184`).
    pub fn channel_mode(self) -> ChannelMode {
        ChannelMode::from_channel_count(self.channels)
            .expect("channel count validated at construction")
    }

    /// Two-byte AudioSpecificConfig for this AAC-LC configuration.
    ///
    /// The bytes are `audioObjectType = 2` (AAC-LC), the sampling-frequency
    /// index from the encoder's authoritative rate table, the channel
    /// configuration from [`ChannelMode::channel_config`], and a
    /// `GASpecificConfig` with `frameLengthFlag = 0` (1024-sample frames),
    /// `dependsOnCoreCoder = 0`, and `extensionFlag = 0` — exactly the
    /// configuration this encoder implements.
    ///
    /// This is an out-of-band configuration for raw AAC access units; it is
    /// not the ADTS header. ADTS transport remains unchanged.
    ///
    /// # Errors
    ///
    /// [`AdtsError::UnsupportedSampleRate`] or
    /// [`AdtsError::InvalidChannelConfig`] if the configuration cannot be
    /// expressed in the two-byte form. [`EncoderConfig::new`] already rejects
    /// such values, so a successfully constructed configuration returns
    /// `Ok`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrite_aac::aac_enc::config::EncoderConfig;
    ///
    /// let stereo_48k = EncoderConfig::stereo_48k_adts();
    /// assert_eq!(stereo_48k.audio_specific_config().unwrap(), [0x11, 0x90]);
    /// ```
    pub fn audio_specific_config(self) -> Result<[u8; 2], AdtsError> {
        let freq_idx = sample_rate_index(self.sample_rate)?;
        let channel_config = self.channel_mode().channel_config();
        if channel_config == 0 || channel_config > 15 {
            return Err(AdtsError::InvalidChannelConfig(self.channel_mode()));
        }
        let aot = AOT_AAC_LC as u8;
        Ok([
            (aot << 3) | (freq_idx >> 1),
            ((freq_idx & 1) << 7) | ((channel_config as u8) << 3),
        ])
    }

    /// Nominal bitrate in bit/s: per-channel profile table entry times the
    /// effective (non-LFE) channel count (`FDKaacEnc_GetVBRBitrate`,
    /// `aacenc.cpp:216-246`). Stereo total is twice the per-channel entry
    /// (e.g. VBR-5/VBR-6 stereo 2 × 96 000 = 192 000; mono 112 000).
    pub fn nominal_bitrate(self) -> u32 {
        let mode = self.channel_mode();
        let info = mode.info().expect("channel mode validated at construction");
        let per_channel = self.profile().channel_bitrate(mode.is_mono());
        per_channel * info.effective_channels
    }

    /// Per-channel nominal bitrate in bit/s (FDK table entry).
    pub fn channel_bitrate(self) -> u32 {
        self.profile()
            .channel_bitrate(self.channel_mode().is_mono())
    }

    /// VBR quality factor for rate control (`qc_main.cpp:122-133`,
    /// `tableVbrQualFactor` v3.0.0 rows).
    pub fn vbr_qual_factor(self) -> f32 {
        self.profile().qual_factor()
    }

    /// VBR quality factor in Q31 (`FL2FXCONST_DBL` form).
    pub fn vbr_qual_q31(self) -> i32 {
        self.profile().qual_q31()
    }

    /// Configured bandwidth in Hz (profile table value, before Nyquist clamp).
    pub fn configured_bandwidth_hz(self) -> u32 {
        self.profile().bandwidth_hz()
    }

    /// Effective bandwidth in Hz: profile bandwidth limited by the
    /// sample-rate Nyquist limit (`min(bandwidth, sample_rate/2)`), mirroring
    /// `SfbConfig` / `DetermineBandWidth` (`bandwidth.cpp:357`).
    /// At 44.1/48 kHz the clamp is inactive for all six profiles.
    pub fn effective_bandwidth_hz(self) -> u32 {
        self.profile().bandwidth_hz().min(self.sample_rate / 2)
    }

    /// Encoder delay in samples per channel: MDCT plus block-switching
    /// look-ahead, `DELAY_AAC(fl) = fl + 4*(fl/8) + (fl/8)/2`
    /// (`aacenc_lib.cpp:132-137`), i.e. 1024 + 512 + 64 = 1600.
    /// Callers must compensate this many samples for gapless output.
    pub fn encoder_delay(self) -> usize {
        let short = self.frame_length / 8;
        self.frame_length + 4 * short + short / 2
    }

    /// PCM samples per input frame across all channels
    /// (`nSamplesToRead = frameLength * nChannels`, `aacenc_lib.cpp:812`).
    pub fn input_frame_samples(self) -> usize {
        self.frame_length * self.channels as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_target_derivations() {
        // Default is VBR-6 (Ferrite): same bitrate/qual as VBR-5, 20 kHz bandwidth.
        let cfg = EncoderConfig::stereo_48k_adts();
        assert_eq!(cfg.vbr_mode(), VbrMode::Vbr6);
        assert_eq!(cfg.sample_rate(), 48000);
        assert_eq!(cfg.channels(), 2);
        assert_eq!(cfg.channel_mode(), ChannelMode::Stereo);
        assert_eq!(cfg.nominal_bitrate(), 192_000);
        assert_eq!(cfg.vbr_qual_factor(), 0.070);
        assert_eq!(cfg.effective_bandwidth_hz(), 20_000);
        assert_eq!(cfg.configured_bandwidth_hz(), 20_000);
        assert_eq!(cfg.encoder_delay(), 1600);
        assert_eq!(cfg.input_frame_samples(), 2048);
        assert_eq!(cfg.transport(), TransportType::Adts);
        // Explicit VBR-5 baseline is unchanged.
        let vbr5 = EncoderConfig::stereo_48k_adts().with_vbr_mode(VbrMode::Vbr5);
        assert_eq!(vbr5.nominal_bitrate(), 192_000);
        assert_eq!(vbr5.vbr_qual_factor(), 0.070);
        assert_eq!(vbr5.effective_bandwidth_hz(), 19293);
        // `new` also defaults to VBR-6.
        let fresh = EncoderConfig::new(48000, 2, 1024, TransportType::Adts).unwrap();
        assert_eq!(fresh.vbr_mode(), VbrMode::Vbr6);
        assert_eq!(fresh.effective_bandwidth_hz(), 20_000);
    }

    #[test]
    fn nominal_bitrate_uses_effective_channels() {
        let mono = EncoderConfig::new(44100, 1, 1024, TransportType::Raw).unwrap();
        assert_eq!(mono.nominal_bitrate(), 112_000);
        // 5.1: LFE excluded from the effective count (5 * 96000).
        let five_one = EncoderConfig::new(48000, 6, 1024, TransportType::Adts).unwrap();
        assert_eq!(five_one.channel_mode(), ChannelMode::SixChannels51);
        assert_eq!(five_one.nominal_bitrate(), 480_000);
        // 6.1: 6 effective channels.
        let six_one = EncoderConfig::new(48000, 7, 1024, TransportType::Adts).unwrap();
        assert_eq!(six_one.nominal_bitrate(), 576_000);
        // VBR-1 stereo total is 2 × 20 000 = 40 000; mono total is 32 000.
        let vbr1_stereo =
            EncoderConfig::new_with_mode(48000, 2, 1024, TransportType::Adts, VbrMode::Vbr1)
                .unwrap();
        assert_eq!(vbr1_stereo.channel_bitrate(), 20_000);
        assert_eq!(vbr1_stereo.nominal_bitrate(), 40_000);
        let vbr1_mono =
            EncoderConfig::new_with_mode(48000, 1, 1024, TransportType::Adts, VbrMode::Vbr1)
                .unwrap();
        assert_eq!(vbr1_mono.channel_bitrate(), 32_000);
        assert_eq!(vbr1_mono.nominal_bitrate(), 32_000);
    }

    #[test]
    fn rejects_out_of_plane_parameters() {
        assert_eq!(
            EncoderConfig::new(12345, 2, 1024, TransportType::Adts),
            Err(ConfigError::UnsupportedSampleRate(12345))
        );
        // 7350 Hz is a valid ADTS index but not an encoder rate.
        assert_eq!(
            EncoderConfig::new(7350, 2, 1024, TransportType::Adts),
            Err(ConfigError::UnsupportedSampleRate(7350))
        );
        assert_eq!(
            EncoderConfig::new(48000, 0, 1024, TransportType::Adts),
            Err(ConfigError::UnsupportedChannelCount(0))
        );
        assert_eq!(
            EncoderConfig::new(48000, 9, 1024, TransportType::Adts),
            Err(ConfigError::UnsupportedChannelCount(9))
        );
        // 960 passes AdjustEncSettings but Initialize rejects it for LC.
        assert_eq!(
            EncoderConfig::new(48000, 2, 960, TransportType::Adts),
            Err(ConfigError::UnsupportedFrameLength(960))
        );
        assert_eq!(
            EncoderConfig::new(48000, 2, 512, TransportType::Adts),
            Err(ConfigError::UnsupportedFrameLength(512))
        );
    }

    #[test]
    fn accepts_full_sample_rate_whitelist() {
        for rate in SUPPORTED_SAMPLE_RATES {
            let cfg = EncoderConfig::new(rate, 2, FRAME_LENGTH, TransportType::Adts).unwrap();
            assert_eq!(cfg.sample_rate(), rate);
        }
    }

    #[test]
    fn error_display_is_informative() {
        assert_eq!(
            ConfigError::UnsupportedSampleRate(11).to_string(),
            "unsupported sample rate: 11"
        );
        assert_eq!(
            ConfigError::UnsupportedChannelCount(9).to_string(),
            "unsupported channel count: 9"
        );
        assert_eq!(
            ConfigError::UnsupportedFrameLength(960).to_string(),
            "unsupported frame length: 960"
        );
    }

    #[test]
    fn audio_specific_config_matches_m75_cases() {
        let stereo_48k = EncoderConfig::new(48000, 2, 1024, TransportType::Adts).unwrap();
        assert_eq!(stereo_48k.audio_specific_config(), Ok([0x11, 0x90]));

        let mono_441k = EncoderConfig::new(44100, 1, 1024, TransportType::Adts).unwrap();
        assert_eq!(mono_441k.audio_specific_config(), Ok([0x12, 0x08]));

        let mono_48k = EncoderConfig::new(48000, 1, 1024, TransportType::Adts).unwrap();
        assert_eq!(mono_48k.audio_specific_config(), Ok([0x11, 0x88]));

        let stereo_441k = EncoderConfig::new(44100, 2, 1024, TransportType::Adts).unwrap();
        assert_eq!(stereo_441k.audio_specific_config(), Ok([0x12, 0x10]));
    }

    #[test]
    fn audio_specific_config_covers_rate_table() {
        // (rate, expected 2-byte ASC for mono); sfi 0..=11, channel config 1.
        let expected = [
            (96000, [0x10, 0x08]),
            (88200, [0x10, 0x88]),
            (64000, [0x11, 0x08]),
            (48000, [0x11, 0x88]),
            (44100, [0x12, 0x08]),
            (32000, [0x12, 0x88]),
            (24000, [0x13, 0x08]),
            (22050, [0x13, 0x88]),
            (16000, [0x14, 0x08]),
            (12000, [0x14, 0x88]),
            (11025, [0x15, 0x08]),
            (8000, [0x15, 0x88]),
        ];
        for (rate, asc) in expected {
            let cfg = EncoderConfig::new(rate, 1, 1024, TransportType::Adts).unwrap();
            assert_eq!(cfg.audio_specific_config(), Ok(asc), "{rate} Hz");
        }
    }

    #[test]
    fn audio_specific_config_covers_channel_configs() {
        for channels in 1..=8u32 {
            let cfg = EncoderConfig::new(48000, channels, 1024, TransportType::Adts).unwrap();
            let asc = cfg.audio_specific_config().unwrap();
            assert_eq!(asc[0], 0x11, "{channels} channels: AOT/SFI");
            assert_eq!(
                asc[1] & 0x78,
                ((cfg.channel_mode().channel_config() as u8) << 3) & 0x78,
                "{channels} channels: channel configuration"
            );
        }
    }

    #[test]
    fn vbr_profiles_match_upstream_table() {
        // (mode, mono total, stereo per-ch, stereo total, bandwidth, qual, origin)
        let cases = [
            (
                VbrMode::Vbr1,
                32_000,
                20_000,
                40_000,
                13_000,
                0.150,
                VbrOrigin::UpstreamFdk,
            ),
            (
                VbrMode::Vbr2,
                40_000,
                32_000,
                64_000,
                13_000,
                0.162,
                VbrOrigin::UpstreamFdk,
            ),
            (
                VbrMode::Vbr3,
                56_000,
                48_000,
                96_000,
                15_750,
                0.176,
                VbrOrigin::UpstreamFdk,
            ),
            (
                VbrMode::Vbr4,
                72_000,
                64_000,
                128_000,
                16_500,
                0.120,
                VbrOrigin::UpstreamFdk,
            ),
            (
                VbrMode::Vbr5,
                112_000,
                96_000,
                192_000,
                19_293,
                0.070,
                VbrOrigin::UpstreamFdk,
            ),
            (
                VbrMode::Vbr6,
                112_000,
                96_000,
                192_000,
                20_000,
                0.070,
                VbrOrigin::FerriteExtension,
            ),
        ];
        for (mode, mono, per_ch, stereo_total, bw, qual, origin) in cases {
            let p = mode.profile();
            assert_eq!(p.mode(), mode, "{mode}");
            assert_eq!(p.name(), format!("VBR-{}", mode.number()).as_str());
            assert_eq!(p.origin(), origin, "{mode}");
            assert_eq!(p.mono_bitrate(), mono, "{mode}");
            assert_eq!(p.stereo_per_channel_bitrate(), per_ch, "{mode}");
            assert_eq!(p.stereo_total_bitrate(), stereo_total, "{mode}");
            assert_eq!(
                p.stereo_total_bitrate(),
                per_ch * 2,
                "{mode} total = 2x per-ch"
            );
            assert_eq!(p.bandwidth_hz(), bw, "{mode}");
            assert!((p.qual_factor() - qual).abs() < 1e-6, "{mode} qual");
            assert_eq!(p.channel_bitrate(true), mono, "{mode} mono per-ch");
            assert_eq!(p.channel_bitrate(false), per_ch, "{mode} stereo per-ch");
            assert_eq!(mode.number(), p.mode() as u32);
        }
    }

    #[test]
    fn vbr6_inherits_vbr5_except_bandwidth() {
        let vbr5 = VbrMode::Vbr5.profile();
        let vbr6 = VbrMode::Vbr6.profile();
        assert_eq!(vbr6.mono_bitrate(), vbr5.mono_bitrate());
        assert_eq!(
            vbr6.stereo_per_channel_bitrate(),
            vbr5.stereo_per_channel_bitrate()
        );
        assert_eq!(vbr6.stereo_total_bitrate(), vbr5.stereo_total_bitrate());
        assert_eq!(vbr6.qual_factor(), vbr5.qual_factor());
        assert_eq!(vbr6.qual_q31(), vbr5.qual_q31());
        assert_eq!(vbr6.origin(), VbrOrigin::FerriteExtension);
        assert_eq!(vbr5.origin(), VbrOrigin::UpstreamFdk);
        assert_eq!(vbr5.bandwidth_hz(), 19_293);
        assert_eq!(vbr6.bandwidth_hz(), 20_000);
        assert_ne!(vbr6.bandwidth_hz(), vbr5.bandwidth_hz());
    }

    #[test]
    fn vbr_mode_numbers_and_parsing() {
        assert_eq!(VbrMode::Vbr1.number(), 1);
        assert_eq!(VbrMode::Vbr6.number(), 6);
        assert_eq!(VbrMode::from_number(1), Some(VbrMode::Vbr1));
        assert_eq!(VbrMode::from_number(6), Some(VbrMode::Vbr6));
        assert_eq!(VbrMode::from_number(0), None);
        assert_eq!(VbrMode::from_number(7), None);
        use core::str::FromStr as _;
        assert_eq!(VbrMode::from_str("1").unwrap(), VbrMode::Vbr1);
        assert_eq!(VbrMode::from_str("6").unwrap(), VbrMode::Vbr6);
        assert_eq!(VbrMode::from_str("VBR-5").unwrap(), VbrMode::Vbr5);
        assert_eq!(VbrMode::from_str("vbr2").unwrap(), VbrMode::Vbr2);
        assert!(VbrMode::from_str("0").is_err());
        assert!(VbrMode::from_str("7").is_err());
        assert!(VbrMode::from_str("vbr").is_err());
        assert_eq!(VbrMode::Vbr6.to_string(), "VBR-6");
        assert_eq!(VbrOrigin::UpstreamFdk.to_string(), "FDK");
        assert_eq!(VbrOrigin::FerriteExtension.to_string(), "Ferrite");
    }

    #[test]
    fn effective_bandwidth_respects_sample_rate() {
        // At 44.1/48 kHz the Nyquist clamp is inactive for all profiles.
        for mode in VbrMode::ALL {
            for rate in [44100, 48000] {
                let cfg =
                    EncoderConfig::new_with_mode(rate, 2, FRAME_LENGTH, TransportType::Adts, mode)
                        .unwrap();
                assert_eq!(
                    cfg.effective_bandwidth_hz(),
                    mode.profile().bandwidth_hz(),
                    "{mode} at {rate}"
                );
            }
        }
        // At low rates the Nyquist limit binds (e.g. 8 kHz -> 4 kHz cap).
        let low =
            EncoderConfig::new_with_mode(8000, 2, FRAME_LENGTH, TransportType::Adts, VbrMode::Vbr6)
                .unwrap();
        assert_eq!(low.effective_bandwidth_hz(), 4000);
        let low5 =
            EncoderConfig::new_with_mode(8000, 2, FRAME_LENGTH, TransportType::Adts, VbrMode::Vbr5)
                .unwrap();
        assert_eq!(low5.effective_bandwidth_hz(), 4000);
    }

    #[test]
    fn explicit_profile_selection_and_defaults() {
        for mode in VbrMode::ALL {
            let cfg =
                EncoderConfig::new_with_mode(48000, 2, FRAME_LENGTH, TransportType::Adts, mode)
                    .unwrap();
            assert_eq!(cfg.vbr_mode(), mode);
            assert_eq!(cfg.profile().mode(), mode);
            assert_eq!(cfg.vbr_qual_factor(), mode.profile().qual_factor());
            assert_eq!(cfg.vbr_qual_q31(), mode.profile().qual_q31());
            assert_eq!(
                cfg.channel_bitrate(),
                mode.profile().stereo_per_channel_bitrate()
            );
            // Mono uses the mono entry.
            let mono =
                EncoderConfig::new_with_mode(48000, 1, FRAME_LENGTH, TransportType::Adts, mode)
                    .unwrap();
            assert_eq!(mono.channel_bitrate(), mode.profile().mono_bitrate());
            assert_eq!(mono.nominal_bitrate(), mode.profile().mono_bitrate());
        }
    }

    #[test]
    fn psy_engine_defaults_to_q31_in_every_constructor() {
        // FPE selection seam (docs/fpe/06 §2, roadmap M1 FPE-1.2): the
        // additive knob defaults to Q31 everywhere; existing constructors
        // are behaviourally unchanged.
        assert_eq!(PsyEngineKind::default(), PsyEngineKind::Q31);
        let new = EncoderConfig::new(48000, 2, FRAME_LENGTH, TransportType::Adts).unwrap();
        assert_eq!(new.psy_engine(), PsyEngineKind::Q31);
        for mode in VbrMode::ALL {
            let cfg =
                EncoderConfig::new_with_mode(48000, 2, FRAME_LENGTH, TransportType::Adts, mode)
                    .unwrap();
            assert_eq!(cfg.psy_engine(), PsyEngineKind::Q31, "{mode}");
        }
        assert_eq!(
            EncoderConfig::stereo_48k_adts().psy_engine(),
            PsyEngineKind::Q31
        );
    }

    #[test]
    fn with_psy_engine_builder_selects_fpe_and_preserves_other_fields() {
        let base = EncoderConfig::stereo_48k_adts().with_vbr_mode(VbrMode::Vbr5);
        let fpe = base.with_psy_engine(PsyEngineKind::Fpe);
        assert_eq!(fpe.psy_engine(), PsyEngineKind::Fpe);
        // Every other field is preserved (additive builder).
        assert_eq!(fpe.sample_rate(), base.sample_rate());
        assert_eq!(fpe.channels(), base.channels());
        assert_eq!(fpe.frame_length(), base.frame_length());
        assert_eq!(fpe.transport(), base.transport());
        assert_eq!(fpe.vbr_mode(), base.vbr_mode());
        assert_eq!(fpe.nominal_bitrate(), base.nominal_bitrate());
        assert_eq!(fpe.effective_bandwidth_hz(), base.effective_bandwidth_hz());
        // The base copy is untouched (Copy builder semantics).
        assert_eq!(base.psy_engine(), PsyEngineKind::Q31);
        // Selecting Q31 explicitly round-trips to the base config.
        assert_eq!(fpe.with_psy_engine(PsyEngineKind::Q31), base);
    }
}
