use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::{AudioProperties, ConvertError};

/// A validated FLAC stream and its decoder.
pub(crate) struct FlacSource {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    properties: AudioProperties,
}

impl FlacSource {
    pub(crate) fn open(path: &Path) -> Result<Self, ConvertError> {
        let file =
            File::open(path).map_err(|err| ConvertError::InputOpen(path.to_path_buf(), err))?;
        let metadata = file
            .metadata()
            .map_err(|err| ConvertError::InputOpen(path.to_path_buf(), err))?;
        if !metadata.is_file() {
            return Err(ConvertError::InputNotFile(path.to_path_buf()));
        }

        let source = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("flac");
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                source,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|_| ConvertError::InputNotFlac(path.to_path_buf()))?;

        let (track_id, properties, decoder) = {
            let track = format
                .default_track(TrackType::Audio)
                .ok_or(ConvertError::MissingAudioTrack)?;
            let params = track
                .codec_params
                .as_ref()
                .and_then(|params| params.audio())
                .ok_or(ConvertError::MissingAudioProperties)?;

            let sample_rate = params
                .sample_rate
                .ok_or(ConvertError::MissingAudioProperties)?;
            let channels = params
                .channels
                .as_ref()
                .map(|channels| channels.count())
                .ok_or(ConvertError::MissingAudioProperties)?;
            let channels =
                u32::try_from(channels).map_err(|_| ConvertError::MissingAudioProperties)?;
            let bits_per_sample = params
                .bits_per_sample
                .ok_or(ConvertError::MissingAudioProperties)?;
            let properties = AudioProperties {
                sample_rate,
                channels,
                bits_per_sample,
            };
            properties.validate()?;

            let mut options = AudioDecoderOptions::default();
            options.verify = true;
            let decoder = symphonia::default::get_codecs()
                .make_audio_decoder(params, &options)
                .map_err(|_| ConvertError::InputNotFlac(path.to_path_buf()))?;

            (track.id, properties, decoder)
        };

        Ok(Self {
            format,
            decoder,
            track_id,
            properties,
        })
    }

    pub(crate) fn properties(&self) -> AudioProperties {
        self.properties
    }

    /// Decode the next audio packet into interleaved `i16` PCM.
    ///
    /// Returns `false` at end of stream. The destination vector is replaced
    /// with exactly the samples from one decoded packet. Samples wider than
    /// 16 bits arrive left-justified from the decoder and are truncated to
    /// their top 16 bits by Symphonia's conversion; callers disclose that
    /// reduction (see `AudioProperties::validate`).
    pub(crate) fn next_pcm(&mut self, pcm: &mut Vec<i16>) -> Result<bool, ConvertError> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(false),
                Err(_) => return Err(ConvertError::DecodeFailed),
            };
            if packet.track_id != self.track_id {
                continue;
            }

            let decoded = self
                .decoder
                .decode(&packet)
                .map_err(|_| ConvertError::DecodeFailed)?;
            if decoded.spec().rate() != self.properties.sample_rate
                || decoded.spec().channels().count() != self.properties.channels as usize
            {
                return Err(ConvertError::DecodeFailed);
            }
            decoded.copy_to_vec_interleaved::<i16>(pcm);
            return Ok(true);
        }
    }

    /// Run the decoder's end-of-stream verification.
    ///
    /// FLAC carries an MD5 of the unencoded PCM. A present-but-mismatched
    /// checksum is treated as a decode failure.
    pub(crate) fn verify(&mut self) -> Result<(), ConvertError> {
        if self.decoder.finalize().verify_ok == Some(false) {
            return Err(ConvertError::DecodeFailed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn decodes_stereo_16_bit_interleaved_pcm() {
        let mut source = FlacSource::open(&fixture("stereo-48k-16.flac")).unwrap();
        assert_eq!(
            source.properties(),
            AudioProperties {
                sample_rate: 48_000,
                channels: 2,
                bits_per_sample: 16,
            }
        );

        let mut pcm = Vec::new();
        let mut all = Vec::new();
        while source.next_pcm(&mut pcm).unwrap() {
            all.extend_from_slice(&pcm);
        }
        source.verify().unwrap();

        assert_eq!(all.len(), 12_000 * 2);
        assert_eq!(
            &all[0..6],
            &[-12_000, 12_000, -11_998, 11_998, -11_996, 11_996]
        );
        // Crossing the encoder's 8,000-frame chunk boundary must not disturb
        // the canonical left/right interleaving.
        assert_eq!(
            &all[8_000 * 2..8_000 * 2 + 4],
            &[4_000, -4_000, 4_002, -4_002]
        );
    }

    #[test]
    fn accepts_24_bit_input_truncated_to_top_16_bits() {
        let mut source = FlacSource::open(&fixture("stereo-48k-24.flac")).unwrap();
        assert_eq!(
            source.properties(),
            AudioProperties {
                sample_rate: 48_000,
                channels: 2,
                bits_per_sample: 24,
            }
        );

        let mut pcm = Vec::new();
        let mut all = Vec::new();
        while source.next_pcm(&mut pcm).unwrap() {
            all.extend_from_slice(&pcm);
        }
        source.verify().unwrap();

        // Fixture signal: (i * 1000 - 500000, 500000 - i * 1000), i in 0..1000.
        // Truncation keeps the top 16 bits (arithmetic >> 8, floor): hand-computed.
        assert_eq!(all.len(), 1000 * 2);
        assert_eq!(&all[0..4], &[-1954, 1953, -1950, 1949]);
    }

    #[test]
    fn truncates_24_bit_boundary_values_exactly() {
        let mut source = FlacSource::open(&fixture("boundary-48k-24.flac")).unwrap();
        assert_eq!(source.properties().bits_per_sample, 24);

        let mut pcm = Vec::new();
        let mut all = Vec::new();
        while source.next_pcm(&mut pcm).unwrap() {
            all.extend_from_slice(&pcm);
        }
        source.verify().unwrap();

        // Hand-computed literals for the crafted 24-bit vector (see
        // fixtures/README.md): arithmetic right shift by 8, floor for
        // negatives, exact at both full-scale extremes.
        assert_eq!(
            &all[0..16],
            &[
                32767, -32768, // 0x7FFFFF, -0x800000
                -1, 0, // -1, 1
                0, 0, // silence
                0, 1, // 255, 256
                -1, -1, // -256, -255
                4660, -4661, // 0x123456, -0x123456
                32767, -32767, // 2^23 - 256, -(2^23 - 256)
                -32768, 32766, // -(2^23 - 255), 2^23 - 257
            ]
        );
        // Ramp (i * 257, -(i * 257)), i in 0..64: channel order and breadth.
        assert_eq!(all.len(), (8 + 64) * 2);
        for (i, pair) in all[16..].chunks_exact(2).enumerate() {
            let v = (i as i32) * 257;
            assert_eq!(pair[0], (v >> 8) as i16, "left ramp sample {i}");
            assert_eq!(pair[1], ((-v) >> 8) as i16, "right ramp sample {i}");
        }
    }

    #[test]
    fn accepts_mono_24_bit_input() {
        let mut source = FlacSource::open(&fixture("mono-441k-24.flac")).unwrap();
        assert_eq!(
            source.properties(),
            AudioProperties {
                sample_rate: 44_100,
                channels: 1,
                bits_per_sample: 24,
            }
        );

        let mut pcm = Vec::new();
        let mut frames = 0usize;
        while source.next_pcm(&mut pcm).unwrap() {
            frames += pcm.len();
        }
        source.verify().unwrap();
        assert_eq!(frames, 1000);
    }

    #[test]
    fn rejects_unsupported_sample_rate() {
        let err = match FlacSource::open(&fixture("mono-32k-16.flac")) {
            Ok(_) => panic!("32 kHz input was accepted"),
            Err(err) => err,
        };
        assert!(matches!(err, ConvertError::UnsupportedSampleRate(32_000)));
    }

    #[test]
    fn rejects_unsupported_channel_count() {
        let err = match FlacSource::open(&fixture("three-channel-48k-16.flac")) {
            Ok(_) => panic!("three-channel input was accepted"),
            Err(err) => err,
        };
        assert!(matches!(err, ConvertError::UnsupportedChannels(3)));
    }
}
