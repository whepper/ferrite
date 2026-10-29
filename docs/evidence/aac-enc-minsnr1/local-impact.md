# Local impact around MINSNR1-divergent frames

Descriptive measurements only; not perceptual evidence.

## Encoded sizes

ADTS frame sizes from the retained f32 production encode and the pinned FDK
`aac-enc` reference.

| Fixture | Frame | C bytes | Rust bytes | Delta |
|---|---:|---:|---:|---:|
| quiet-sine-stereo | 5 | 81 | 81 | 0 |
| onset-stereo | 25 | 119 | 119 | 0 |
| onset-stereo | 26 | 58 | 58 | 0 |
| sine-stereo | 5 | 58 | 59 | +1 |
| sine-stereo-44100 | 5 | 54 | 54 | 0 |
| stepdown-stereo | 2 | 129 | 130 | +1 |
| click-mono | 11 | 49 | 49 | 0 |
| impulse-mono | 5 | 178 | 176 | −2 |

The `quiet-sine-stereo` frame-5 payload is byte-identical despite MINSNR1
differing on seven active bands. The other ±1/2-byte changes are confounded by
the known MS/TNS/energy divergences and cannot be attributed to MINSNR1 alone.

## Decoded SNR

FDK decoder; 1600-sample positive alignment (`decoded[1600:]` aligns
`original[0:]`); SNR versus the original PCM over 1024- and 4096-sample
windows starting at the frame. `maxdiff` is the largest absolute decoded
sample difference between Rust and C.

| Fixture | Frame | Rust 1024 | C 1024 | Rust 4096 | C 4096 | maxdiff |
|---|---:|---:|---:|---:|---:|---:|
| quiet-sine-stereo | 5 | 56.23 | 56.23 | 53.59 | 53.59 | 0 |
| onset-stereo | 25 | 59.95 | 59.97 | 62.19 | 61.46 | 22 |
| onset-stereo | 26 | 62.70 | 60.23 | 63.12 | 61.55 | 22 |
| sine-stereo | 5 | 62.72 | 60.23 | 63.40 | 61.54 | 22 |
| sine-stereo-44100 | 5 | 62.57 | 62.57 | 62.18 | 62.19 | 2 |
| stepdown-stereo | 2 | 60.65 | 59.39 | 62.68 | 61.41 | 22 |

The `quiet-sine` control is sample-identical at frame 5. Elsewhere the SNR
direction is mixed and the frames also carry other pinned divergences, so the
table does not establish a MINSNR1-specific quality effect.
