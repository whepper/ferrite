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
//! Channel modes, channel counts, and ADTS channel configurations.
//!
//! Ports the mode table from `cpp/libAACenc/src/channel_map.cpp:139-155`
//! (`channelModeConfig`) with `CHANNEL_MODE` discriminants from
//! `cpp/libSYS/include/FDK_audio.h:232-252`, and the channel-configuration
//! mapping from `cpp/libMpegTPEnc/src/tpenc_asc.cpp:289-333`
//! (`getChannelConfig`).

/// Channel mode. Discriminants match `CHANNEL_MODE` in `FDK_audio.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ChannelMode {
    /// `MODE_INVALID`.
    Invalid = -1,
    /// `MODE_UNKNOWN`.
    Unknown = 0,
    /// `MODE_1`: center (SCE).
    Mono = 1,
    /// `MODE_2`: left + right (CPE).
    Stereo = 2,
    /// `MODE_1_2`: center, left + right (SCE, CPE).
    ThreeChannels = 3,
    /// `MODE_1_2_1`: + rear (SCE, CPE, SCE).
    FourChannels = 4,
    /// `MODE_1_2_2`: + left/right surround (SCE, CPE, CPE).
    FiveChannels = 5,
    /// `MODE_1_2_2_1`: 5.1 (SCE, CPE, CPE, LFE).
    SixChannels51 = 6,
    /// `MODE_1_2_2_2_1`: 7.1 (SCE, CPE, CPE, CPE, LFE).
    EightChannels71 = 7,
    /// `MODE_6_1`: 6.1 (SCE, CPE, CPE, SCE, LFE).
    SevenChannels61 = 11,
    /// `MODE_7_1_BACK`: 7.1 rear (SCE, CPE, CPE, CPE, LFE).
    EightChannels71Back = 12,
    /// `MODE_7_1_TOP_FRONT`: 7.1 top-front (SCE, CPE, CPE, LFE, CPE).
    EightChannels71TopFront = 14,
}

/// Static description of a channel mode: total channels, bit-reservoir
/// effective channels (LFE excluded), and bitstream element count.
/// Rows mirror `channelModeConfig` (`channel_map.cpp:139-155`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelModeInfo {
    /// Channel mode of this row.
    pub mode: ChannelMode,
    /// Total input channels (`nChannels`).
    pub channels: u32,
    /// Effective channels for bitrate math; LFE excluded (`nChannelsEff`).
    pub effective_channels: u32,
    /// Number of single/channel/LFE elements (`nElements`).
    pub elements: u32,
}

/// Full mode table in `channelModeConfig` order (`channel_map.cpp:139-155`).
/// The two extra 8-channel rows mirror the `MODE_7_1_REAR_SURROUND` (33) and
/// `MODE_7_1_FRONT_CENTER` (34) aliases, which share counts with the rows
/// below; they are intentionally not separate `ChannelMode` variants yet.
pub const CHANNEL_MODE_TABLE: [ChannelModeInfo; 10] = [
    ChannelModeInfo {
        mode: ChannelMode::Mono,
        channels: 1,
        effective_channels: 1,
        elements: 1,
    },
    ChannelModeInfo {
        mode: ChannelMode::Stereo,
        channels: 2,
        effective_channels: 2,
        elements: 1,
    },
    ChannelModeInfo {
        mode: ChannelMode::ThreeChannels,
        channels: 3,
        effective_channels: 3,
        elements: 2,
    },
    ChannelModeInfo {
        mode: ChannelMode::FourChannels,
        channels: 4,
        effective_channels: 4,
        elements: 3,
    },
    ChannelModeInfo {
        mode: ChannelMode::FiveChannels,
        channels: 5,
        effective_channels: 5,
        elements: 3,
    },
    ChannelModeInfo {
        mode: ChannelMode::SixChannels51,
        channels: 6,
        effective_channels: 5,
        elements: 4,
    },
    ChannelModeInfo {
        mode: ChannelMode::EightChannels71,
        channels: 8,
        effective_channels: 7,
        elements: 5,
    },
    ChannelModeInfo {
        mode: ChannelMode::SevenChannels61,
        channels: 7,
        effective_channels: 6,
        elements: 5,
    },
    ChannelModeInfo {
        mode: ChannelMode::EightChannels71Back,
        channels: 8,
        effective_channels: 7,
        elements: 5,
    },
    ChannelModeInfo {
        mode: ChannelMode::EightChannels71TopFront,
        channels: 8,
        effective_channels: 7,
        elements: 5,
    },
];

impl ChannelMode {
    /// Row for this mode, or `None` for `Invalid`/`Unknown` and the two
    /// unmapped 8-channel aliases (33/34).
    pub fn info(self) -> Option<ChannelModeInfo> {
        CHANNEL_MODE_TABLE
            .iter()
            .find(|row| row.mode == self)
            .copied()
    }

    /// Derive the encoder mode from a channel count: first table row whose
    /// channel count matches, mirroring `FDKaacEnc_DetermineEncoderMode`
    /// (`channel_map.cpp:157-184`) for `MODE_UNKNOWN` input.
    pub fn from_channel_count(channels: u32) -> Option<ChannelMode> {
        CHANNEL_MODE_TABLE
            .iter()
            .find(|row| row.channels == channels)
            .map(|row| row.mode)
    }

    /// Mono/stereo classification for rate tables: only `MODE_1` is mono,
    /// everything else is stereo (`FDKaacEnc_GetMonoStereoMode`,
    /// `channel_map.cpp:619-630`).
    pub fn is_mono(self) -> bool {
        self == ChannelMode::Mono
    }

    /// ADTS/ASC channel configuration (`getChannelConfig`,
    /// `tpenc_asc.cpp:289-333`). Returns 0 for modes without a defined
    /// configuration, exactly like the C++ `default` branch.
    pub fn channel_config(self) -> u32 {
        match self {
            ChannelMode::Mono => 1,
            ChannelMode::Stereo => 2,
            ChannelMode::ThreeChannels => 3,
            ChannelMode::FourChannels => 4,
            ChannelMode::FiveChannels => 5,
            ChannelMode::SixChannels51 => 6,
            ChannelMode::EightChannels71 => 7,
            ChannelMode::SevenChannels61 => 11,
            ChannelMode::EightChannels71Back => 12,
            ChannelMode::EightChannels71TopFront => 14,
            ChannelMode::Invalid | ChannelMode::Unknown => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_table_matches_channel_map_cpp() {
        let expected = [
            (ChannelMode::Mono, 1, 1, 1),
            (ChannelMode::Stereo, 2, 2, 1),
            (ChannelMode::ThreeChannels, 3, 3, 2),
            (ChannelMode::FourChannels, 4, 4, 3),
            (ChannelMode::FiveChannels, 5, 5, 3),
            (ChannelMode::SixChannels51, 6, 5, 4),
            (ChannelMode::EightChannels71, 8, 7, 5),
            (ChannelMode::SevenChannels61, 7, 6, 5),
            (ChannelMode::EightChannels71Back, 8, 7, 5),
            (ChannelMode::EightChannels71TopFront, 8, 7, 5),
        ];
        assert_eq!(CHANNEL_MODE_TABLE.len(), expected.len());
        for (row, (mode, ch, eff, el)) in CHANNEL_MODE_TABLE.iter().zip(expected) {
            assert_eq!(row.mode, mode);
            assert_eq!(row.channels, ch);
            assert_eq!(row.effective_channels, eff);
            assert_eq!(row.elements, el);
            assert_eq!(mode.info(), Some(*row));
        }
    }

    #[test]
    fn from_channel_count_picks_first_matching_row() {
        assert_eq!(ChannelMode::from_channel_count(1), Some(ChannelMode::Mono));
        assert_eq!(
            ChannelMode::from_channel_count(2),
            Some(ChannelMode::Stereo)
        );
        assert_eq!(
            ChannelMode::from_channel_count(3),
            Some(ChannelMode::ThreeChannels)
        );
        assert_eq!(
            ChannelMode::from_channel_count(4),
            Some(ChannelMode::FourChannels)
        );
        assert_eq!(
            ChannelMode::from_channel_count(5),
            Some(ChannelMode::FiveChannels)
        );
        assert_eq!(
            ChannelMode::from_channel_count(6),
            Some(ChannelMode::SixChannels51)
        );
        assert_eq!(
            ChannelMode::from_channel_count(7),
            Some(ChannelMode::SevenChannels61)
        );
        // First 8-channel row wins, as in the C++ linear scan.
        assert_eq!(
            ChannelMode::from_channel_count(8),
            Some(ChannelMode::EightChannels71)
        );
        assert_eq!(ChannelMode::from_channel_count(0), None);
        assert_eq!(ChannelMode::from_channel_count(9), None);
    }

    #[test]
    fn channel_config_matches_get_channel_config() {
        let expected = [
            (ChannelMode::Mono, 1),
            (ChannelMode::Stereo, 2),
            (ChannelMode::ThreeChannels, 3),
            (ChannelMode::FourChannels, 4),
            (ChannelMode::FiveChannels, 5),
            (ChannelMode::SixChannels51, 6),
            (ChannelMode::EightChannels71, 7),
            (ChannelMode::SevenChannels61, 11),
            (ChannelMode::EightChannels71Back, 12),
            (ChannelMode::EightChannels71TopFront, 14),
            (ChannelMode::Unknown, 0),
            (ChannelMode::Invalid, 0),
        ];
        for (mode, cfg) in expected {
            assert_eq!(mode.channel_config(), cfg, "{mode:?}");
        }
    }

    #[test]
    fn only_mono_mode_is_mono() {
        assert!(ChannelMode::Mono.is_mono());
        for mode in [
            ChannelMode::Stereo,
            ChannelMode::ThreeChannels,
            ChannelMode::FourChannels,
            ChannelMode::FiveChannels,
            ChannelMode::SixChannels51,
            ChannelMode::EightChannels71,
        ] {
            assert!(!mode.is_mono(), "{mode:?}");
        }
    }

    #[test]
    fn discriminants_match_fdk_audio_h() {
        assert_eq!(ChannelMode::Invalid as i32, -1);
        assert_eq!(ChannelMode::Unknown as i32, 0);
        assert_eq!(ChannelMode::Mono as i32, 1);
        assert_eq!(ChannelMode::Stereo as i32, 2);
        assert_eq!(ChannelMode::ThreeChannels as i32, 3);
        assert_eq!(ChannelMode::FourChannels as i32, 4);
        assert_eq!(ChannelMode::FiveChannels as i32, 5);
        assert_eq!(ChannelMode::SixChannels51 as i32, 6);
        assert_eq!(ChannelMode::EightChannels71 as i32, 7);
        assert_eq!(ChannelMode::SevenChannels61 as i32, 11);
        assert_eq!(ChannelMode::EightChannels71Back as i32, 12);
        assert_eq!(ChannelMode::EightChannels71TopFront as i32, 14);
    }
}
