# Block-switch oracle investigation evidence

Diagnostic evidence for the `m7-speech-clean-stereo-48k` frames 107–109
sequence divergence. The investigation is written up in
[`docs/aac-enc-block-switch-oracle-investigation.md`](../../aac-enc-block-switch-oracle-investigation.md).
This directory contains the per-frame table, the focused C probe source, and
the filtered C/Rust diagnostic logs for frames 100–115.

## Fixture and method

- Fixture: `rust/src/aac_enc/testdata/m7-speech-clean-stereo-48k.wav`
  (48 kHz, stereo, 16-bit, dual-mono speech), SHA-256
  `3dfb7dcb0cd9f600f9ae66eb791df6d7c740c833218df88f1de73f87353346ff`.
  Frame 107 channel-0 samples (2048 bytes) SHA-256
  `f815b9e3446b1d4cdce8f15a31c47c380cb3a8a383ea60d0afd35b03fd61111e`.
- Reference: pinned FDK source
  `f14ad40a574a4132df947d15e9fb14bee91cc8a5`, Release library
  `libfdk-aac.2.0.4.dylib`.
- C probe: calls the real `FDKaacEnc_InitBlockSwitching` and
  `FDKaacEnc_BlockSwitching` frame by frame, prints the pre-sync state and
  post-sync sequence/grouping. It reproduces the oracle `FRAME` lines:
  f107 START/SINE, f108 SHORT/SINE, f109 STOP/KBD.
- Rust probe: the same fixture and frame loop through `BlockSwitch::process`
  and `synchronize_pair`, with a temporary test-only dump of the same fields.
- The probe source is retained as `block_switch_probe.cpp`. It is not part of
  the Rust build and is not committed as a production tool.
- `c-frames-100-115.log` and `rust-frames-100-115.log` are the filtered
  diagnostic outputs behind `frames-100-115.md`.
- `local-decoded-snr.md` records the localized decoded SNR and local ADTS
  frame sizes.
- Both channels are sample-identical in this fixture; every value in the table
  is identical on channel 1.

Build/run command used for the C probe:

```sh
c++ -std=c++11 -O2 \
  -I <ref>/cpp/libAACenc/src -I <ref>/cpp/libFDK/include \
  -I <ref>/cpp/libSYS/include -I <ref>/cpp/libMpegTPEnc/include -I <ref> \
  block_switch_probe.cpp <ref>/wavreader.c \
  -L <ref-build> -lfdk-aac -Wl,-rpath,<ref-build> \
  -o block_switch_probe
./block_switch_probe m7-speech-clean-stereo-48k.wav > c-block-switch.log
```

## Decision boundary at f107

All values below are Q31 energy units. `NRGF[7]` is the current frame's
filtered window 7; `prev NRGF[7]` is the previous frame's value used as
`enM1` at i=0. `acc` is the accumulator **after** the full frame attack loop,
not the i=0 accumulator.

| Quantity | FDK C | Rust f32 |
|---|---:|---:|
| previous `acc` (after f106) | 450 | 497 |
| previous `NRGF[7]` (f106) | 992 | 1048 |
| f107 `NRGF[0]` (`e0`) | 6237 | 6298 |
| i=0 accumulator `0.7*prev_acc + 0.3*prev_e7` | 612.6 | 662.3 |
| attack threshold `0.1*e0` | 623.7 | 629.8 |
| margin `threshold - acc_i0` | **+11.1** | **-32.5** |
| attack flag | **1** | **0** |
| post-sync sequence | **START (1)** | **LONG (0)** |

The C-side margin is positive, so C starts a short-block sequence. The Rust
margin is negative, so Rust remains long. At f108/f109 the same state carries
the C sequence forward (SHORT then STOP), while Rust remains long.

## Per-frame table

`frames-100-115.md` lists C/Rust attack flags, final accumulator, current and
previous filtered window-7 energy, post-sync sequence, and groups. The first
attack/sequence difference is f107; all prior attack flags and sequences agree.

## Interpretation

The port's formulas, tables, state order, and transition logic match the C
functions. The difference is caused by FDK's per-sample fixed-point energy
truncation (`fPow2Div2(...) >> 4` in `FDKaacEnc_CalcWindowEnergy`) versus
Rust's real-valued f32 energy accumulation, amplified by the recursive
`accWindowNrg` history at a low-energy-to-transient boundary. This is a
documented float-native precision divergence, not a branch/table/state-order
porting defect. It is also not a harness or input-alignment discrepancy: the
neighbouring frames agree, and the C probe reproduces the oracle sequence.
