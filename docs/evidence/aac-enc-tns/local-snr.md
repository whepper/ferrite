Delay-aligned decoded comparison around active TNS-divergence frames.

- Decoder: pinned FDK `aac-dec`
- Alignment: positive lag 1600 samples; `decoded[lag:]` aligns `original[0:]`
- SNR versus the original PCM, descriptive only (not perceptual)
- Inputs/decoded WAVs from the retained V3 F04 metrics run

| Fixture | Frame | Window | Rust SNR (dB) | C SNR (dB) |
|---|---:|---:|---:|---:|
| m3-multitone-stereo-48k | 1 | 1024 | 46.90 | 44.90 |
| m3-multitone-stereo-48k | 1 | 4096 | 46.23 | 46.45 |
| m2-onset-stereo-48k | 24 | 1024 | 46.96 | 46.96 |
| m2-onset-stereo-48k | 24 | 4096 | 52.60 | 52.51 |
| m2-onset-stereo-48k | 25 | 1024 | 59.95 | 59.97 |
| m2-onset-stereo-48k | 25 | 4096 | 62.19 | 61.46 |
| m2-stepdown-stereo-48k | 1 | 1024 | 53.36 | 53.25 |
| m2-stepdown-stereo-48k | 1 | 4096 | 58.08 | 57.79 |

These local differences do not show a consistent direction or a clear
quality loss from the Rust TNS decisions. Other coding differences contribute;
the table cannot attribute the waveform difference solely to TNS.
