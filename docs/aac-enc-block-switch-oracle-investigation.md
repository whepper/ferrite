# Block-switch oracle investigation: `m7-speech-clean-stereo-48k` f107–f109

Date: 2026-10-27. Status: **diagnostic complete; no production algorithm
change; no commit or push.**

Reported divergence: FDK selects START/SINE → SHORT/SINE → STOP/KBD at
frames 107–109; Rust selects LONG/KBD throughout. This investigation traced
the decision path, built a focused C/Rust probe, found the first causal
difference, and classified it.

**Classification: intentional float-native numerical divergence, not a
porting defect or harness discrepancy.** The Rust block-switch formulas,
tables, state order, and transition logic match the FDK C implementation. The
sequence flips because FDK's fixed-point per-sample energy truncation and the
Rust f32 real-valued energy path place the f107 window-0 attack test on
opposite sides of a knife-edge threshold.

## 1. Sources reviewed

FDK reference (pinned `f14ad40a574a4132df947d15e9fb14bee91cc8a5`,
`build/ref`):

| Function / data | Location |
|---|---|
| State (`BLOCK_SWITCHING_CONTROL`) | `cpp/libAACenc/src/block_switch.h:123-148` |
| Constants/tables (`hiPassCoeff`, `accWindowNrgFac`, `invAttackRatio`, `minAttackNrg`, `suggestedGroupingTable`, `chgWndSqLkAhd`) | `cpp/libAACenc/src/block_switch.cpp:121-227` |
| `FDKaacEnc_InitBlockSwitching` | `cpp/libAACenc/src/block_switch.cpp:168-188` |
| `FDKaacEnc_BlockSwitching` | `cpp/libAACenc/src/block_switch.cpp:229-346` |
| `FDKaacEnc_CalcWindowEnergy` | `cpp/libAACenc/src/block_switch.cpp:359-409` |
| `FDKaacEnc_SyncBlockSwitching` | `cpp/libAACenc/src/block_switch.cpp:426-560` |

Rust:

| Function / data | Location |
|---|---|
| Constants and tables | `rust/src/aac_enc/block_switch.rs:165-268` |
| State | `rust/src/aac_enc/block_switch.rs:276-312` |
| `BlockSwitch::process` | `rust/src/aac_enc/block_switch.rs:341-399` |
| `BlockSwitch::synchronize_pair` / `synchronize_mono` | `rust/src/aac_enc/block_switch.rs:409-452` |
| `BlockSwitch::calc_window_energy` | `rust/src/aac_enc/block_switch.rs:456-477` |

The operation ordering, tables, and state transitions were compared line by
line. No branch, table entry, index order, or state-reset mismatch was found.

## 2. Focused probe

The C probe calls the real `FDKaacEnc_InitBlockSwitching` and
`FDKaacEnc_BlockSwitching` on the same fixture and frame loop as
`tools/enc-oracle/enc_oracle.cpp`; it prints pre-sync state and post-sync
sequence/grouping. Both probes consume the same committed WAV
(SHA-256 `3dfb7dcb…346ff`); frame 107 channel-0 samples hash to
`f815b9e3…1111e`. The C probe reproduces the oracle:

```
FRAME 107 CH 0 SEQ 1 SHAPE 0 ...
FRAME 108 CH 0 SEQ 2 SHAPE 0 ...
FRAME 109 CH 0 SEQ 3 SHAPE 1 ...
```

The Rust probe runs the same fixture and frame loop through
`BlockSwitch::process` and `synchronize_pair`, capturing the same fields.
Both channels are sample-identical in this fixture; every value below is
identical on channel 1.

Probe source and per-frame table:
[`docs/evidence/aac-enc-block-switch/`](evidence/aac-enc-block-switch/README.md).

## 3. First causal divergence

Energies differ from frame 0 by a fraction of a percent because FDK truncates
each per-sample fixed-point product before summing while Rust accumulates the
real value; all attack flags and sequences still agree through f106. The first
decision-relevant state difference is at f106, in the history used by the f107
window-0 test:

| Quantity (Q31 energy units) | FDK C | Rust f32 |
|---|---:|---:|
| f106 final `accWindowNrg` | **450** | **497** (9.5% high) |
| f106 `windowNrgF[1][7]` (used as f107 `enM1`) | **992** | **1048** |
| f107 `windowNrgF[1][0]` (`e0`) | **6237** | **6298** |
| f107 i=0 accumulator `0.7*prev_acc + 0.3*prev_e7` | 612.6 | 662.3 |
| f107 attack threshold `0.1*e0` | 623.7 | 629.8 |
| margin (`threshold - acc_i0`) | **+11.1** | **−32.5** |
| attack flag | **1** | **0** |
| post-sync sequence | **START (1)** | **LONG (0)** |

The FDK margin is positive, so C starts a short-block sequence. The Rust
margin is negative, so Rust stays long. The same state carries the decision
forward at f108 (SHORT vs LONG) and f109 (STOP vs LONG).

Per-frame comparison, frames 100–115 (channel 0; channel 1 identical):

| f | C attack | R attack | C acc | R acc | C NRGF[7] | R NRGF[7] | C prev NRGF[7] | R prev NRGF[7] | C seq | R seq | C groups | R groups |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 100 | 0 | 0 | 23486 | 23554 | 10479 | 10536 | 28720 | 28784 | 0 | 0 | 1 | 1 |
| 101 | 0 | 0 | 11578 | 11648 | 4098 | 4158 | 10479 | 10536 | 0 | 0 | 1 | 1 |
| 102 | 0 | 0 | 64342 | 64411 | 186740 | 186813 | 4098 | 4158 | 0 | 0 | 1 | 1 |
| 103 | 0 | 0 | 242308 | 242376 | 251029 | 251104 | 186740 | 186813 | 0 | 0 | 1 | 1 |
| 104 | 0 | 0 | 83144 | 83209 | 10517 | 10580 | 251029 | 251104 | 0 | 0 | 1 | 1 |
| 105 | 0 | 0 | 5212 | 5250 | 10 | 40 | 10517 | 10580 | 0 | 0 | 1 | 1 |
| 106 | 0 | 0 | 450 | 497 | 992 | 1048 | 10 | 40 | 0 | 0 | 1 | 1 |
| 107 | **1** | **0** | 61576 | 61645 | 35330 | 35393 | 992 | 1048 | **1** | **0** | 1 | 1 |
| 108 | 0 | 0 | 66626 | 66696 | 76702 | 76772 | 35330 | 35393 | **2** | **0** | **4** | **1** |
| 109 | 0 | 0 | 57784 | 57855 | 71066 | 71133 | 76702 | 76772 | **3** | **0** | 1 | 1 |
| 110 | 0 | 0 | 44152 | 44223 | 56588 | 56653 | 71066 | 71133 | 0 | 0 | 1 | 1 |
| 111 | 0 | 0 | 36100 | 36173 | 39305 | 39372 | 56588 | 56653 | 0 | 0 | 1 | 1 |
| 112 | 0 | 0 | 11118 | 11186 | 4197 | 4262 | 39305 | 39372 | 0 | 0 | 1 | 1 |
| 113 | 0 | 0 | 12646 | 12717 | 13402 | 13466 | 4197 | 4262 | 0 | 0 | 1 | 1 |
| 114 | 0 | 0 | 48790 | 48860 | 19155 | 19211 | 13402 | 13466 | 0 | 0 | 1 | 1 |
| 115 | 0 | 0 | 92988 | 93060 | 23677 | 23737 | 19155 | 19211 | 0 | 0 | 1 | 1 |

Note: `acc` is the final accumulator after the whole frame, not the i=0
accumulator. The f106 current `NRGF[7]` (992/1048) is the `prev NRGF[7]` used
at f107.

## 4. Root cause

FDK's `FDKaacEnc_CalcWindowEnergy` computes each sample's energy as

```c
temp_windowNrg += (LONG)fPow2Div2(state) >> (BLOCK_SWITCH_ENERGY_SHIFT - 1 - 2);
```

i.e. a Q31 product, halved and then shifted right by four before summing.
Rust computes the same real-number formula in f32 but does not truncate each
term to the Q31 grid. At low levels the C terms frequently truncate to zero
(e.g. f105 window-7 filtered energy: C 10 vs Rust 40 Q31 units; f106 current
window-7: 992 vs 1048). Those differences feed the recursive `accWindowNrg`
through a quiet region; by f106 C's accumulator is 450 vs Rust's 497. At f107
window 0 the attack test is a near-tie, and the accumulated difference plus
the threshold difference flips the sign.

This is a precision consequence, not a missing branch: the Rust state order,
attack loop, minimum-energy gate, frame-border spreading rule,
`chgWndSqLkAhd` table, shape mapping, and stereo grouping all match the C
source. The `f32` accumulations are deterministic and repeat identically.

## 5. Regression test

Added in `rust/src/aac_enc/block_switch.rs`:

`aac_enc::block_switch::tests::speech_clean_f107_attack_boundary_is_float_limited`

It:

- reproduces `speech-clean` frames 0–107 deterministically from the committed
  fixture;
- captures the f106 accumulator and f106 window-7 filtered energy before the
  f107 decision;
- asserts the Rust f107 attack flag is false, sequence is LONG, and the
  threshold margin is a small positive value;
- compares against the recorded FDK-side Q31 values and asserts the C-side
  margin is positive;
- checks f108/f109 remain LONG in the current float path.

It is self-contained (no external artifacts), does not assert unrelated
internals, and does not change encoder behavior. The production output
baseline remains pinned by
`aac_enc::production_baseline::production_f32_corpus_baseline_is_unchanged`.

Result: the focused test passes. The full suite result is recorded in §8.

## 6. Impact assessment

The sequence changes temporal resolution around a speech transient:

- C uses a START/SINE frame, eight 128-sample short windows, then STOP/KBD.
- Rust uses three 1024-sample long/KBD frames. This changes overlap, short
  grouping, pre-echo handling, and the TNS/quantization inputs for the region.
- The affected frame sizes (ADTS bytes, C/R) are:
  f107 276/267, f108 339/340, f109 268/304, f110 268/264.
  The full `speech-clean` stream is 127,971 B (C) vs 127,900 B (Rust), −71 B
  (−0.055%); the local differences are larger than the file total suggests.

Delay-aligned decoded comparison around the event (positive lag 1600,
FDK decoder; SNR versus the source; higher is descriptive only):

| Window start | Window | Rust SNR | C SNR |
|---|---:|---:|---:|
| f106 (2.261333 s) | 1024 samples | 21.21 | 24.89 |
| f107 (2.282667 s) | 1024 | 33.61 | 28.96 |
| f108 (2.304000 s) | 1024 | 44.54 | 37.85 |
| f109 (2.325333 s) | 1024 | 41.31 | 41.24 |
| f104 (2.218667 s) | 8192 | 41.64 | 40.06 |
| f107 (2.282667 s) | 8192 | 42.81 | 41.05 |
| f109 (2.325333 s) | 8192 | 41.96 | 40.79 |

These measurements do **not** show a consistent quality loss from Rust's
long-block choice; some windows favor Rust, one favors C. The differences are
small and SNR is not perceptual. This is not evidence that Rust sounds better
or worse. It is also not a controlled forced-sequence comparison: other
encoder differences contribute, so the local SNR cannot be attributed solely
to the window sequence.

No clipping change or decode failure was observed; both streams decode
independently. The sequence divergence is a valid AAC-LC alternative, not a
bitstream-validity defect.

## 7. Recommendation

**Do not change the production block-switch algorithm to force FDK parity.**
The port is logically faithful; the divergence is the documented float-native
precision class. Emulating Q31 truncation would be a broad behavior change
across many low-level frames, and the local decoded evidence does not
demonstrate a quality benefit. It would also need a separate controlled
sequence-forcing experiment and perceptual evidence.

Keep the focused regression test and evidence. If future quality work targets
real-speech transients, compare a Q31-emulated block switch (or a forced
START/SHORT/STOP sequence) against the current float path with localized
metrics and listening evidence before considering a production change.

## 8. Tests run

| Command / test | Result |
|---|---|
| `cargo test --locked --features aac_enc --lib aac_enc::block_switch::tests::speech_clean_f107_attack_boundary_is_float_limited` | pass |
| `cargo test --locked --features aac_enc --lib` with reference artifacts (`PHASE1_ORACLE_DIR`, `PHASE1_C_DIR`, `AAC_DEC_BIN`, `M55_C_DIR`) | **336/336 pass** (335 + new boundary test) |
| `cargo test --locked --features aac_enc --lib` without external artifacts | 326 pass / 10 documented reference-artifact failures |
| `cargo test --locked -p aac-convert` | pass |
| `cargo test --locked --features aac_enc --doc` | 19/19 |
| `rustfmt --check` on changed Rust files | clean |
| `rust/tools/check-licensing.sh` | pass |

The production baseline test remains green; no encoder behavior changed.

## 9. Files changed

| File | Change |
|---|---|
| `rust/src/aac_enc/block_switch.rs` | Added the f107 boundary regression test; qualified the module parity wording. No production decision change. |
| `docs/aac-enc-block-switch-oracle-investigation.md` | This report. |
| `docs/evidence/aac-enc-block-switch/README.md` | Probe method, C/R margin values, interpretation. |
| `docs/evidence/aac-enc-block-switch/frames-100-115.md` | Per-frame C/R diagnostic table. |
| `docs/evidence/aac-enc-block-switch/block_switch_probe.cpp` | Focused reference probe source (not part of the Rust build). |
| `docs/README.md` | Index entry for this report. |

Unrelated working-tree changes (concurrent encoder/bookkeeping work and the
pre-existing documentation edits) were not touched, reverted, staged, or
committed. The f64 experiment remains removed.

No commit or push was performed.

## 10. Classification summary

| Question | Answer |
|---|---|
| Input alignment/initialization? | No; frames 100–106 and 110–115 agree, and the C probe reproduces the oracle. |
| Harness discrepancy? | No; same WAV, frame loop, and real `FDKaacEnc_BlockSwitching`. |
| Logic/table/state-order porting defect? | No evidence; all branches and tables match. |
| First divergent state? | f106 `accWindowNrg` and window-7 filtered history; first decision flip at f107 window 0. |
| Cause? | FDK Q31 per-sample energy truncation vs Rust f32 real accumulation. |
| Classification? | **Intentional float-native precision divergence.** |
| Fix justified? | **No, not on current evidence.** Keep the diagnostic test; revisit only with controlled localized quality evidence. |
