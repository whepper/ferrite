//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! M6.1 migration equality harness (test-only).
//!
//! First client of the production [`Encoder`](super::encoder::Encoder):
//! runs the exact 19-stream M5.d inventory through both the existing
//! test-gated `m52_write_adts_stream` path and the new public API with
//! identical inputs, and asserts byte-identical ADTS output.
//!
//! This harness does not replace the test driver — the driver keeps its
//! snapshot observability (M5.g/M5.i census) intact. Equality here proves
//! the API is a byte-exact wrapper, not a reimplementation.

#[cfg(test)]
mod m6_tests {
    use super::super::config::{EncoderConfig, TransportType, FRAME_LENGTH};
    use super::super::encoder::Encoder;
    use super::super::stream_driver::{m52_write_adts_stream, M52_FIXTURES, M55_FIXTURES};
    use super::super::testdata;

    // The 19-stream M5.d inventory, plus the fourth validated M5.a/M5.e
    // configuration (mono/44.1k), which is outside the census inventory
    // but inside the byte-identity gate. Local list on purpose: the shared
    // M52/M55 inventories must not be expanded by this harness.
    const EXTRA: [(&str, &str, u32, usize); 1] =
        [("onset-mono-44100", "m3-onset-mono-441k", 44100, 1)];
    #[test]
    fn m6_api_matches_driver_bytes() {
        let dir = std::env::temp_dir().join("m6-migration");
        std::fs::create_dir_all(&dir).expect("M6.1: mkdir migration dir");
        for (short, stem, sr, ch) in M52_FIXTURES
            .iter()
            .chain(M55_FIXTURES.iter())
            .chain(EXTRA.iter())
            .copied()
        {
            // Existing test-gated path (reference).
            let rep = m52_write_adts_stream(short, stem, sr, ch, &dir)
                .unwrap_or_else(|e| panic!("{stem}: driver path failed: {e}"));
            let driver_bytes = std::fs::read(&rep.path).expect("M6.1: read driver ADTS");
            // New public API path, identical input in a single push.
            let pcm = testdata::fixture_samples(short);
            assert_eq!(pcm.len(), ch, "{stem}: channels");
            let s = pcm[0].len();
            let mut interleaved = Vec::with_capacity(s * ch);
            for (i, _) in pcm[0].iter().enumerate() {
                interleaved.extend(pcm.iter().map(|chan| chan[i]));
            }
            let cfg = EncoderConfig::new(sr, ch as u32, FRAME_LENGTH, TransportType::Adts)
                .unwrap_or_else(|e| panic!("{stem}: config rejected: {e:?}"));
            let mut enc = Encoder::new(cfg).unwrap_or_else(|e| panic!("{stem}: API refused: {e}"));
            let mut api_bytes = Vec::with_capacity(driver_bytes.len());
            let mut idx = 0u32;
            for f in enc
                .push(&interleaved)
                .unwrap_or_else(|e| panic!("{stem}: API push failed: {e}"))
            {
                assert_eq!(f.frame_index, idx, "{stem}: frame order");
                idx += 1;
                api_bytes.extend_from_slice(&f.bytes);
            }
            assert!(!enc.is_finished(), "{stem}: push must not finish");
            for f in enc
                .flush()
                .unwrap_or_else(|e| panic!("{stem}: API flush failed: {e}"))
            {
                assert_eq!(f.frame_index, idx, "{stem}: frame order");
                idx += 1;
                api_bytes.extend_from_slice(&f.bytes);
            }
            assert!(enc.is_finished(), "{stem}: finished after flush");
            assert_eq!(idx as usize, rep.frames, "{stem}: frame count");
            assert_eq!(
                api_bytes, driver_bytes,
                "{stem}: API output must be byte-identical to the test-gated driver"
            );
            println!(
                "M6.1 {stem}: frames={} bytes={} api-identical",
                rep.frames,
                driver_bytes.len()
            );
        }
    }
}
