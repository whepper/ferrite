# f32 vs f64 decoded-audio comparison

`snr_*` is full-band SNR versus the original (higher is better, descriptive only). `f64_vs_f32_snr` is the f64 decoded waveform versus the f32 decoded waveform (lag 0; both have the same encoder delay). No perceptual claim.

| Fixture | Scope | SNR f32 | SNR f64 | Δ f64−f32 | SNR C | Δ f64−C | RMS f32/f64 (LSB) | Peak f32/f64 | Bytes f32/f64/C |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| m2-sine-stereo-48k | gated | 56.911 | 56.911 | +0.000 | 56.588 | +0.322 | 10.090/10.090 | 210/210 | 3147/3147/3104 |
| m2-onset-stereo-48k | gated | 58.102 | 58.105 | +0.003 | 57.621 | +0.484 | 6.142/6.140 | 135/131 | 2359/2359/2318 |
| m2-stepdown-stereo-48k | gated | 53.648 | 53.648 | +0.000 | 53.742 | -0.095 | 10.517/10.517 | 210/210 | 2236/2236/2209 |
| m2-click-mono-48k | gated | 35.845 | 35.845 | -0.000 | 35.928 | -0.083 | 91.315/91.315 | 6932/6932 | 2680/2680/2739 |
| m2-impulse-mono-48k | gated | 6.620 | 6.620 | +0.000 | 6.595 | +0.025 | 157.304/157.304 | 7002/7002 | 306/306/308 |
| m2-edge-mono-48k | gated | 6.968 | 6.968 | +0.000 | 6.968 | +0.000 | 128.293/128.293 | 6064/6064 | 415/415/415 |
| m3-multitone-stereo-48k | gated | 45.100 | 45.100 | +0.000 | 45.374 | -0.274 | 29.410/29.410 | 722/722 | 4819/4819/4726 |
| m3-sweep-stereo-48k | gated | 43.335 | 43.335 | -0.000 | 43.807 | -0.472 | 38.526/38.527 | 216/216 | 3988/3988/4088 |
| m3-noise-mono-48k | gated | 6.965 | 6.965 | +0.000 | 6.965 | +0.000 | 2082.378/2082.378 | 8328/8328 | 10603/10603/10601 |
| m3-tonal-noise-stereo-48k | gated | 23.275 | 23.275 | +0.000 | 23.275 | -0.000 | 392.440/392.440 | 1693/1693 | 10630/10630/10628 |
| m3-quiet-sine-stereo-48k | gated | 50.758 | 50.758 | +0.000 | 50.573 | +0.185 | 0.101/0.101 | 3/3 | 4200/4200/4189 |
| m3-sine-stereo-441k | exploratory | 55.143 | 55.144 | +0.001 | 55.192 | -0.047 | 12.368/12.367 | 292/292 | 2998/3000/3014 |
| m3-onset-stereo-441k | exploratory | 58.531 | 58.531 | -0.000 | 57.265 | +1.267 | 5.830/5.830 | 80/80 | 2250/2250/2247 |
| m6-stress-stereo-48k | gated | 10.316 | 10.316 | +0.000 | 10.315 | +0.001 | 4300.704/4300.704 | 16610/16610 | 10728/10728/10728 |
| m7-music-stereo-48k | gated | 37.530 | 37.530 | -0.000 | 37.715 | -0.185 | 51.359/51.360 | 686/686 | 250926/250926/251279 |
| m7-speech-pauses-stereo-48k | gated | 40.710 | 40.710 | -0.001 | 40.720 | -0.010 | 16.806/16.808 | 236/236 | 125818/125812/125996 |
| m7-speech-clean-stereo-48k | gated | 41.126 | 41.126 | +0.000 | 41.097 | +0.029 | 13.822/13.822 | 190/190 | 127900/127903/127971 |
| m8-music-eroica-stereo-48k | exploratory | 30.930 | 30.929 | -0.000 | 30.982 | -0.053 | 126.970/126.971 | 806/806 | 234229/234228/235444 |

| Fixture | f64 vs f32 SNR | f64 vs f32 RMS | f64 vs f32 peak | 4096-win min f32/f64 | 4096-win median f32/f64 |
|---|---:|---:|---:|---:|---:|
| m2-silence-stereo-48k | inf | 0.000 | 0 | inf/inf | inf/inf |
| m2-sine-stereo-48k | inf | 0.000 | 0 | 47.30/47.30 | 63.40/63.40 |
| m2-onset-stereo-48k | 95.31 | 0.083 | 6 | 51.00/51.02 | 63.77/63.77 |
| m2-stepdown-stereo-48k | inf | 0.000 | 0 | 47.30/47.30 | 63.74/63.74 |
| m2-click-mono-48k | 98.03 | 0.069 | 1 | 28.63/28.63 | 51.71/51.71 |
| m2-impulse-mono-48k | inf | 0.000 | 0 | inf/inf | inf/inf |
| m2-edge-mono-48k | inf | 0.000 | 0 | 6.97/6.97 | inf/inf |
| m3-multitone-stereo-48k | inf | 0.000 | 0 | 38.64/38.64 | 46.58/46.58 |
| m3-sweep-stereo-48k | 87.54 | 0.233 | 4 | 39.82/39.82 | 45.35/45.35 |
| m3-noise-mono-48k | inf | 0.000 | 0 | 6.83/6.83 | 6.85/6.85 |
| m3-tonal-noise-stereo-48k | inf | 0.000 | 0 | 23.12/23.12 | 23.24/23.24 |
| m3-quiet-sine-stereo-48k | inf | 0.000 | 0 | 46.84/46.84 | 51.96/51.96 |
| m3-sine-stereo-441k | 77.60 | 0.915 | 13 | 46.01/46.01 | 63.07/62.99 |
| m3-onset-stereo-441k | 101.46 | 0.041 | 1 | 53.79/53.79 | inf/inf |
| m6-stress-stereo-48k | inf | 0.000 | 0 | 10.12/10.12 | 10.37/10.37 |
| m7-music-stereo-48k | 83.52 | 0.257 | 15 | 28.86/28.86 | 36.17/36.17 |
| m7-speech-pauses-stereo-48k | 66.82 | 0.830 | 50 | 23.22/23.22 | 41.02/41.02 |
| m7-speech-clean-stereo-48k | 87.69 | 0.065 | 4 | 13.39/13.39 | 41.64/41.64 |
| m8-music-eroica-stereo-48k | 68.73 | 1.629 | 126 | 28.88/28.88 | 30.72/30.72 |

## Aggregate (descriptive)
- gated f64-vs-f32 waveform: 9 bit-identical stream(s); finite SNR n=6, min=66.82 dB, median=87.61 dB, max=98.03 dB
- gated SNR vs original delta f64−f32 (dB): min=-0.001 median=+0.000 max=+0.003; non-positive improvements=13/15
- gated SNR vs original delta f64−C (dB): min=-0.472 median=+0.000 max=+0.484
- all measured f64-vs-f32 waveform: 9 bit-identical stream(s); finite SNR n=9, min=66.82 dB, median=87.54 dB, max=101.46 dB
- all measured SNR vs original delta f64−f32 (dB): min=-0.001 median=+0.000 max=+0.003; non-positive improvements=15/18
- all measured SNR vs original delta f64−C (dB): min=-0.472 median=-0.000 max=+1.267

## Validity
- decode failures: f32=0, f64=0
- frame-count mismatches vs C: f32=0, f64=0
- clipped samples: f32=22, f64=22
