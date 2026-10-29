Delay-aligned decoded comparison around the speech transient.

- Fixture: `m7-speech-clean-stereo-48k`
- Decoder: pinned FDK `aac-dec`
- Alignment: positive lag 1600 samples; `decoded[lag:]` aligns `original[0:]`
- Inputs/decoded WAVs from the retained V3 F04 metrics run
- SNR is versus the original PCM, descriptive only; it is not perceptual.

| Window start | Window (samples) | Rust SNR (dB) | C SNR (dB) |
|---|---:|---:|---:|
| f104 / 2.218667 s | 8192 | 41.638 | 40.064 |
| f106 / 2.261333 s | 1024 | 21.206 | 24.887 |
| f107 / 2.282667 s | 1024 | 33.608 | 28.956 |
| f107 / 2.282667 s | 8192 | 42.807 | 41.046 |
| f108 / 2.304000 s | 1024 | 44.535 | 37.851 |
| f109 / 2.325333 s | 1024 | 41.309 | 41.240 |
| f109 / 2.325333 s | 8192 | 41.959 | 40.785 |

Local ADTS frame sizes (bytes), C/Rust:

| Frame | C | Rust | Delta |
|---:|---:|---:|---:|
| 107 | 276 | 267 | −9 |
| 108 | 339 | 340 | +1 |
| 109 | 268 | 304 | +36 |
| 110 | 268 | 264 | −4 |

Full-file `speech-clean` size: C 127,971 B; Rust 127,900 B; −71 B (−0.055%).
Other encoder differences contribute to local size/SNR, so these values cannot
be attributed solely to the window sequence.
