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
//! Stripped AAC-LC VBR-5 encoder configuration.
//!
//! Mirrors the parameter plane enforced by `FDKaacEnc_AdjustEncSettings`
//! (`cpp/libAACenc/src/aacenc_lib.cpp:544-682`) and the sanity checks in
//! `FDKaacEnc_Initialize` (`cpp/libAACenc/src/aacenc.cpp:442-514`): AAC-LC
//! only, VBR mode 5 only, afterburner always on, ADTS/RAW transport only.
//! The user bitrate is ignored, exactly like the C++ VBR path
//! (`aacenc_lib.cpp:618-622`).

use super::channel_map::ChannelMode;
use core::fmt;

use super::adts::{sample_rate_index, AdtsError};

/// Audio object type: AAC-LC only (`aacenc_lib.cpp:557-560`).
pub const AOT_AAC_LC: u32 = 2;
/// Bitrate mode: VBR 5 only (`aacenc_lib.cpp:561-563`).
pub const BITRATE_MODE_VBR_5: u32 = 5;
/// VBR-5 quality factor, `FL2FXCONST_DBL(0.070f)` (`qc_main.cpp:131-132`).
pub const VBR_5_QUAL_FACTOR: f32 = 0.070;
/// VBR-5 effective bandwidth in Hz, mono and stereo (`bandwidth.cpp:158`).
pub const VBR_5_BANDWIDTH_HZ: u32 = 19293;
/// VBR-5 nominal per-channel bitrate, mono (`aacenc.cpp:205`).
pub const VBR_5_CHAN_BITRATE_MONO: u32 = 112_000;
/// VBR-5 nominal per-channel bitrate, stereo and up (`aacenc.cpp:205`).
pub const VBR_5_CHAN_BITRATE_STEREO: u32 = 96_000;
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

/// Validated encoder configuration for the stripped VBR-5 path.
///
/// Afterburner is not a field: it is unconditionally on
/// (`aacenc_lib.cpp:564-565,574`, `aacenc.cpp:716` with `invQuant=2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderConfig {
    sample_rate: u32,
    channels: u32,
    frame_length: usize,
    transport: TransportType,
}

impl EncoderConfig {
    /// Validate and build a configuration. Fails exactly where the C++
    /// stripped paths return `AACENC_INVALID_CONFIG` / `AAC_ENC_UNSUPPORTED_*`.
    pub fn new(
        sample_rate: u32,
        channels: u32,
        frame_length: usize,
        transport: TransportType,
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
        })
    }

    /// Primary quality target: stereo music at 48 kHz over ADTS.
    pub fn stereo_48k_adts() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            frame_length: FRAME_LENGTH,
            transport: TransportType::Adts,
        }
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
    /// use aac::aac_enc::config::EncoderConfig;
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

    /// Nominal bitrate in bit/s: per-channel VBR-5 table entry times the
    /// effective (non-LFE) channel count (`FDKaacEnc_GetVBRBitrate`,
    /// `aacenc.cpp:216-246`). Stereo gives 192 000, mono 112 000.
    pub fn nominal_bitrate(self) -> u32 {
        let mode = self.channel_mode();
        let info = mode.info().expect("channel mode validated at construction");
        let per_channel = if mode.is_mono() {
            VBR_5_CHAN_BITRATE_MONO
        } else {
            VBR_5_CHAN_BITRATE_STEREO
        };
        per_channel * info.effective_channels
    }

    /// VBR quality factor for rate control (`qc_main.cpp:131-132`).
    pub fn vbr_qual_factor(self) -> f32 {
        VBR_5_QUAL_FACTOR
    }

    /// Effective bandwidth in Hz for VBR-5 (`bandwidth.cpp:158`).
    pub fn effective_bandwidth_hz(self) -> u32 {
        VBR_5_BANDWIDTH_HZ
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
        let cfg = EncoderConfig::stereo_48k_adts();
        assert_eq!(cfg.sample_rate(), 48000);
        assert_eq!(cfg.channels(), 2);
        assert_eq!(cfg.channel_mode(), ChannelMode::Stereo);
        assert_eq!(cfg.nominal_bitrate(), 192_000);
        assert_eq!(cfg.vbr_qual_factor(), 0.070);
        assert_eq!(cfg.effective_bandwidth_hz(), 19293);
        assert_eq!(cfg.encoder_delay(), 1600);
        assert_eq!(cfg.input_frame_samples(), 2048);
        assert_eq!(cfg.transport(), TransportType::Adts);
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
}
