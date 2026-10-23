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
//! ADTS framing for the native encoder.
//!
//! Writes the 7-byte ADTS header (no CRC) exactly as `adtsWrite_EncodeHeader`
//! does (`cpp/libMpegTPEnc/src/tpenc_adts.cpp:192-237`) for the stripped
//! configuration: MPEG-4, AAC-LC profile, no protection, one raw data block.
//! VBR frames report buffer fullness `0x7FF`: the encoder passes
//! `FDK_INT_MAX` (`aacenc.cpp:303-308`) and the transport clamps it
//! (`tpenc_lib.cpp:336-338`).

use super::channel_map::ChannelMode;
use super::config::SUPPORTED_SAMPLE_RATES;
use crate::common::samplerate_index::get_sample_rate_idx;
use core::fmt;

/// ADTS header length in bytes without CRC (`adtsWrite_GetHeaderBits`,
/// `tpenc_adts.cpp:129-153`: 56 bits, protection absent, single block).
pub const ADTS_HEADER_BYTES: usize = 7;
/// ADTS header length in bits for the configuration above.
pub const ADTS_HEADER_BITS: u32 = 56;
/// Maximum frame length encodable in the 13-bit length field, in bytes;
/// the field counts the header itself (`tpenc_adts.cpp:198,235`).
pub const ADTS_MAX_FRAME_BYTES: usize = (1 << 13) - 1;
/// Buffer-fullness value written for VBR frames (see module docs).
pub const ADTS_VBR_BUFFER_FULLNESS: u16 = 0x7FF;
/// ADTS profile field for AAC-LC: `aot - 1` (`tpenc_adts.cpp:170`).
pub const ADTS_PROFILE_AAC_LC: u8 = 1;

/// ADTS framing failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdtsError {
    /// Sample rate has no 4-bit ADTS index on the encoder path.
    UnsupportedSampleRate(u32),
    /// Channel mode has no ADTS channel configuration (0).
    InvalidChannelConfig(ChannelMode),
    /// Frame (payload + 7-byte header) exceeds the 13-bit field.
    FrameTooLarge(usize),
    /// A frame byte sequence does not satisfy the encoder's ADTS framing
    /// invariant (used by [`AdtsFrame::raw_payload`](super::encoder::AdtsFrame::raw_payload)).
    MalformedFrame(&'static str),
}

impl fmt::Display for AdtsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdtsError::UnsupportedSampleRate(rate) => {
                write!(f, "unsupported sample rate: {rate}")
            }
            AdtsError::InvalidChannelConfig(mode) => {
                write!(f, "no ADTS channel configuration for {mode:?}")
            }
            AdtsError::FrameTooLarge(len) => {
                write!(f, "ADTS frame too large: {len} bytes")
            }
            AdtsError::MalformedFrame(what) => {
                write!(f, "malformed ADTS frame: {what}")
            }
        }
    }
}

impl std::error::Error for AdtsError {}

/// 4-bit ADTS sampling-frequency index. Only encoder-supported rates are
/// accepted (notably excluding 7350 Hz, which has a table entry but is
/// rejected by `FDKaacEnc_Initialize`).
pub fn sample_rate_index(sample_rate: u32) -> Result<u8, AdtsError> {
    if !SUPPORTED_SAMPLE_RATES.contains(&sample_rate) {
        return Err(AdtsError::UnsupportedSampleRate(sample_rate));
    }
    Ok(get_sample_rate_idx(sample_rate, 4))
}

/// Encode a 7-byte ADTS header for `raw_data_bytes` of payload.
///
/// `frame_length` field = payload + header (`tpenc_adts.cpp:235`); fails when
/// that exceeds [`ADTS_MAX_FRAME_BYTES`], mirroring the 13-bit assert at
/// `tpenc_adts.cpp:198`.
pub fn encode_header(
    channel_mode: ChannelMode,
    sample_rate: u32,
    raw_data_bytes: usize,
) -> Result<[u8; ADTS_HEADER_BYTES], AdtsError> {
    let freq_idx = sample_rate_index(sample_rate)?;
    let chan_cfg = channel_mode.channel_config();
    if chan_cfg == 0 || chan_cfg > 7 {
        return Err(AdtsError::InvalidChannelConfig(channel_mode));
    }
    let frame_len = raw_data_bytes
        .checked_add(ADTS_HEADER_BYTES)
        .ok_or(AdtsError::FrameTooLarge(usize::MAX))?;
    if frame_len > ADTS_MAX_FRAME_BYTES {
        return Err(AdtsError::FrameTooLarge(frame_len));
    }
    let frame_len = frame_len as u16;
    let fullness = ADTS_VBR_BUFFER_FULLNESS;

    let mut out = [0u8; ADTS_HEADER_BYTES];
    out[0] = 0xFF;
    // Sync tail (4b) | MPEG-4 ID (1b) | layer 0 (2b) | protection absent (1b).
    out[1] = 0xF1;
    // Profile (2b) | frequency index (4b) | private 0 (1b) | config high bit.
    out[2] = (ADTS_PROFILE_AAC_LC << 6) | (freq_idx << 2) | ((chan_cfg >> 2) as u8);
    // Config low bits (2b) | original/home/copyright bits 0 (4b) | len bits 12:11.
    out[3] = (((chan_cfg & 0x3) as u8) << 6) | (((frame_len >> 11) & 0x3) as u8);
    // Length bits 10:3.
    out[4] = ((frame_len >> 3) & 0xFF) as u8;
    // Length bits 2:0 | fullness bits 10:6.
    out[5] = (((frame_len & 0x7) << 5) | (fullness >> 6)) as u8;
    // Fullness bits 5:0 | raw-data-block count (2b, always 0 = single block).
    out[6] = ((fullness & 0x3F) << 2) as u8;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::channel_map::ChannelMode;

    #[test]
    fn stereo_48k_header_matches_reference_encoder() {
        // First frame of the C reference build encoding 1 s of stereo 48 kHz
        // silence at VBR-5 ADTS: 7-byte header + 8 payload bytes = 15.
        let hdr = encode_header(ChannelMode::Stereo, 48000, 8).unwrap();
        assert_eq!(hdr, [0xFF, 0xF1, 0x4C, 0x80, 0x01, 0xFF, 0xFC]);
    }

    #[test]
    fn mono_48k_header_matches_reference_encoder() {
        // First frame of the C reference build encoding the mono impulse
        // fixture: 7-byte header + 6 payload bytes = 13.
        let hdr = encode_header(ChannelMode::Mono, 48000, 6).unwrap();
        assert_eq!(hdr, [0xFF, 0xF1, 0x4C, 0x40, 0x01, 0xBF, 0xFC]);
    }

    #[test]
    fn header_fields_round_trip() {
        let hdr = encode_header(ChannelMode::Stereo, 44100, 100).unwrap();
        assert_eq!(hdr[0], 0xFF);
        assert_eq!(hdr[1], 0xF1);
        // Profile 1 (LC), frequency index 4 (44100 Hz).
        assert_eq!(hdr[2], 0x50);
        // Frame length 107 = 0x6B: high bits in hdr[3], mid byte 0x0D.
        assert_eq!(hdr[3] & 0x03, 0x00);
        assert_eq!(hdr[4], 0x0D);
        assert_eq!(hdr[5], ((0x6B & 0x7) << 5) | 0x1F);
        assert_eq!(hdr[6], 0xFC);
    }

    #[test]
    fn sample_rate_index_covers_encoder_whitelist() {
        let expected = [
            (96000, 0),
            (88200, 1),
            (64000, 2),
            (48000, 3),
            (44100, 4),
            (8000, 11),
        ];
        for (rate, idx) in expected {
            assert_eq!(sample_rate_index(rate), Ok(idx), "{rate}");
        }
        assert_eq!(
            sample_rate_index(7350),
            Err(AdtsError::UnsupportedSampleRate(7350))
        );
        assert_eq!(
            sample_rate_index(12345),
            Err(AdtsError::UnsupportedSampleRate(12345))
        );
    }

    #[test]
    fn rejects_bad_modes_and_oversize_frames() {
        assert_eq!(
            encode_header(ChannelMode::Unknown, 48000, 8),
            Err(AdtsError::InvalidChannelConfig(ChannelMode::Unknown))
        );
        // Channel configurations above 3 bits are PCE-signalled, never ADTS.
        assert_eq!(
            encode_header(ChannelMode::SevenChannels61, 48000, 8),
            Err(AdtsError::InvalidChannelConfig(
                ChannelMode::SevenChannels61
            ))
        );
        let max_payload = ADTS_MAX_FRAME_BYTES - ADTS_HEADER_BYTES;
        assert!(encode_header(ChannelMode::Stereo, 48000, max_payload).is_ok());
        assert_eq!(
            encode_header(ChannelMode::Stereo, 48000, max_payload + 1),
            Err(AdtsError::FrameTooLarge(ADTS_MAX_FRAME_BYTES + 1))
        );
    }

    #[test]
    fn malformed_frame_error_is_descriptive() {
        assert_eq!(
            AdtsError::MalformedFrame("bad syncword").to_string(),
            "malformed ADTS frame: bad syncword"
        );
    }
}
