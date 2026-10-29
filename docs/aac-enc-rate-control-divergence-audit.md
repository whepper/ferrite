# Ferrite AAC encoder — rate-control and bit-reservoir divergence audit

Date: 2026-10-27. Status: **diagnostic complete; no production change; no
commit or push.**

Reference: pinned FDK AAC checkout `f14ad40a574a4132df947d15e9fb14bee91cc8a5`
(`docs/reference/fdk-aac-reference-revision.txt`). Rust path:
`Encoder` → `StreamEncoder::step_frame` → `qc_main_frame`
(`rust/src/aac_enc/encoder.rs:28`, `rust/src/aac_enc/stream_driver.rs:574`,
`rust/src/aac_enc/rate.rs:1427`).

## 1. Executive verdict

**PASS WITH FINDINGS.**

No rate-control or bit-reservoir porting defect was found in the inspected
VBR-5 path. The configuration does **not** use a conventional CBR-style
reservoir:

- `bitResMode` is `FULL`; the reservoir is initialized full and every frame's
  `updateBitres` VBR branch pins it back to its maximum.
- `BitResRedistribution` therefore yields `bitResLevelEl = 0` and
  `maxBitResBitsEl = 0`; `totalAvail == maxBitsPerFrame`.
- The VBR grant is derived from the **per-frame cap** (`maxBitsPerFrame`), not
  from `averageBits` and not from a carried reservoir balance.
- The gain loop did not execute on any of the 2,442 corpus frames: zero inner
  gain adjustments, one outer pass, no crash recovery, and large positive
  margins to every budget boundary.
- The persistent `DistributeState` fields (`peLast`, `dynBitsLast`,
  `peCorrection`, `peMin`/`peMax`) are updated, but in VBR mode their only
  products (`grantedPe`, `grantedPeCorr`) are **not consumed** by
  `AdjustThresholds`. They cannot change the coded result in this
  configuration.
- The persistent `bitResTot` is pinned; the persistent `paddingRest` is
  evolved but does not feed the VBR grant/QC path. There is no cross-frame
  rate-control feedback path that can turn one frame's size difference into a
  later allocation difference.

Frame-size and payload differences are real, but they originate upstream of
the rate-control decision. Across the 19-stream corpus, 408/2,442 frames are
byte-identical and 922/2,442 have equal frame length; every first frame-length
divergence occurs at the same frame as, or later than, the first payload
divergence. The clean stable-noise case (`m3-noise-mono`) is byte-identical
through f14 and first differs at f15, while all QC margins remain positive and
the gain loop remains inactive. That is incompatible with a carried
rate-control state being the origin.

**Scope.** This conclusion applies to the validated AAC-LC VBR-5,
mono/stereo, 44.1/48 kHz ADTS path. It does not generalize to CBR, reduced or
disabled reservoir modes, other bitrates, multichannel elements, or
uncommitted signals.

## 2. Rate-control / reservoir architecture actually active

### 2.1 VBR-5 initialization

The C reference derives the VBR QC plane in `aacenc.cpp:582-592`:

```text
averageBits = (averageBitsPerFrame + 7) & ~7
bitRes      = MIN_BUFSIZE_PER_EFF_CHAN * nChannelsEff
maxBits     = max(bitRes, averageBits)       // optional config clamp not binding here
minBits     = 0
```

At the validated points this gives:

| Operating point | `averageBits` | `maxBits` = `bitRes` | `minBits` | `bitResMode` |
|---|---:|---:|---:|---|
| 48 kHz mono | 2,392 | 6,144 | 0 | `FULL` |
| 48 kHz stereo | 4,096 | 12,288 | 0 | `FULL` |
| 44.1 kHz stereo | 4,464 | 12,288 | 0 | `FULL` |

`FDKaacEnc_QCInit` copies `bitRes` into both `bitResTot` and `bitResTotMax`
(`qc_main.cpp:358-379`); `paddingRest` starts at `sampleRate`
(`aacenc.cpp:642`). The Rust initialization matches all three vectors
exactly: `s6_init_vectors_match_oracle`, backed by
`qc6init_{mono-48k,stereo-48k,stereo-44100}.txt`.

Source: `rust/src/aac_enc/rate.rs:416-466` (`derive_qc_init`),
`rust/src/aac_enc/stream_driver.rs:679` (`padding_rest = sample_rate`).

### 2.2 No reservoir carry in the VBR branch

`FDKaacEnc_QCMain` sets `isCBRAdjustment = 0` for VBR with a full reservoir
(`qc_main.cpp:798-800`) and passes `maxBitsPerFrame` — not `avgTotalBits` — to
both `BitResRedistribution` and `prepareBitDistribution`
(`qc_main.cpp:805-806,836-839`). Consequently:

```text
totalBitreservoir    = min(bitResTot, maxBits - maxBits) = 0
totalBitreservoirMax = min(bitResTotMax, maxBits - maxBits) = 0
bitResLevelEl        = 0
maxBitResBitsEl      = 0
totalAvail           = maxBits
```

`FDKaacEnc_updateBitres` has a VBR case that assigns
`bitResTot = min(maxBitsPerFrame, bitResTotMax)` (`qc_main.cpp:1251-1260`).
No bits are added or removed based on the previous frame. The Rust
`update_bitres_vbr` is the same pin (`rate.rs:1301-1303`), called at the end
of every `qc_main_frame` (`rate.rs:1761`).

The retained C S6 vectors confirm this on all 20 committed QC frames:

- `bitResTot` = 6,144 (mono) or 12,288 (stereo);
- `bitResLevelEl` = `maxBitResBitsEl` = 0;
- `totalAvail` = `maxBitsPerFrame`;
- `npass = 1`, `crash = -1` on every frame.

Evidence: `docs/evidence/aac-enc-rate-control/s6-vectors.tsv`.

### 2.3 What the grant is

`prepareBitDistribution` computes (`qc_main.cpp:621-627`):

```text
grantedDynBits = ((min(maxBits, maxBits) - globHdrBits) & ~7)
                 - globalExtBits - staticBits - elementExtBits
maxDynBits     = (maxBits & ~7)
                 - globalExtBits - staticBits - elementExtBits
```

At VBR the first `min` is the cap, so the nominal average rate is not a target.
For a 48 kHz stereo frame with 43 static bits and the fixed 56-bit ADTS
header:

```text
granted = ((12288 - 56) & ~7) - (3 + 43) = 12,186
maxDyn  = (12288 & ~7) - (3 + 43)        = 12,242
totalAvail = 12,288
```

These are the values in the committed `qc6` vectors and in the live census.
The Rust formulas are `prepare_grants` (`rate.rs:927-949`) and
`distribute_element_dyn_bits` (`rate.rs:913-925`).

### 2.4 `DistributeState`: updated but inert at VBR

`FDKaacEnc_DistributeBits` computes `grantedPe` and `grantedPeCorr` from the
previous frame's state and then updates `peLast`/`dynBitsLast`
(`adj_thr.cpp:2723-2798`). Rust mirrors this in `distribute_bits`
(`rate.rs:813-882`). The state fields are:

```text
peMin, peMax, bits2PeFactor_m/e, peCorrection_m/e, peLast, dynBitsLast
```

At VBR, `FDKaacEnc_AdjustThresholds` takes the non-CBR branch
(`adj_thr.cpp:2804-2884`) and calls only `FDKaacEnc_AdaptThresholdsVBR`. It
does **not** read `grantedPe` or `grantedPeCorr`. The `grantedPeCorr` value is
consumed only in the CBR/PE-target branch (`adj_thr.cpp:2819-2831`), which is
unreachable here. In Rust, `qc_main_frame` computes both values but only
returns them in `QcFrameOutput`; nothing downstream feeds them back into
thresholds or quantisation (`rate.rs:1467-1478`, `:1767-1768`).

Therefore a production difference in `peLast`, `dynBitsLast`,
`peCorrection`, `peMin`, or `peMax` is diagnostic, not causal, at VBR-5. It
can differ if the current frame's PE/granted bits differ, but it cannot
produce a different coded frame.

### 2.5 `paddingRest`: persistent but not in the VBR grant path

`FDKaacEnc_AdjustBitrate` evolves `paddingRest` and returns `avgTotalBits`
(`qc_main.cpp:469-489`; Rust `adjust_bitrate`, `rate.rs:379-391`). The value
is real persistent state and is initialized and evolved exactly. It is not a
VBR allocation input:

- `QCMain` passes `maxBitsPerFrame` to redistribution and grant preparation
  for VBR (`qc_main.cpp:805-806,836-839`).
- `totalAvailableBits` is seeded from that same `maxBitsPerFrame` argument,
  then reduced by the (zero) reservoir level (`qc_main.cpp:643,671`).
- `globHdrBits` is 56 for the validated ADTS path regardless of AU size
  (`S6BITR` on all 20 vectors; `transportEnc_GetStaticBits` probe recorded in
  the S6 implementation record).
- VBR `updateFillBits` uses `minBitsPerFrame = 0`; the transport-correction
  block in `FinalizeBitConsumption` is CBR-only (`qc_main.cpp:1295-1341`).

Rust still computes `padding_rest` and stores it, but discards the returned
`avgTotalBits` (`stream_driver.rs:1362-1368`). That is behavior-equivalent for
this configuration. `padding_rest` differences cannot change the frame.

### 2.6 Gain loop: emergency path, not an allocator

The QC loop adjusts gains only when a constraint is violated
(`qc_main.cpp:901-1107`; Rust `qc_main_frame`, `rate.rs:1427-1765`). The
constraints are the element grant, the absolute element cap, and the frame
budget. On every corpus frame all were non-binding:

| Margin | Minimum | Location |
|---|---:|---|
| `elementGrant - usedDyn` | **986** bits | `m3-noise-mono-48k f1` |
| `maxDyn - usedDyn` | **1,042** bits | `m3-noise-mono-48k f1` |
| `totalAvail - sumBits` | **984** bits | `m3-noise-mono-48k f1` |
| `nChannels*6144 - static - usedDyn` | **1,045** bits | `m3-noise-mono-48k f1` |
| Inner gain adjustments | **0** of 5 | all 2,442 frames |
| Outer passes | **1** | all 2,442 frames |
| Crash recovery | **none** | all 2,442 frames |

The gain loop is therefore a single-pass passthrough on this corpus. It does
not move gains, does not reduce bands, and does not re-quantise. A frame-size
difference cannot be attributed to a QC iteration that never ran.

## 3. State inventory

Only state that survives a frame boundary is listed. `FrameSnapshot` fields
are per-frame diagnostics.

| State | Rust | FDK C | Init / reset | Update point | Genuinely persistent? | Can it change coded output at VBR-5? | Coverage |
|---|---|---|---|---|---|---|---|
| Reservoir total | `QcState.bit_res_tot` (`rate.rs:1398-1413`) | `QC_STATE.bitResTot` (`qc_data.h:280`) | `maxBits` | end of `qc_main_frame` | yes, but pinned | **No** | `s6_corpus_exact` bitres; `update_bitres_vbr` |
| Reservoir maximum | `QcState.bit_res_tot_max` | `QC_STATE.bitResTotMax` | `maxBits` | never | constant | **No** | init vectors; `s6_corpus_exact` |
| Padding remainder | `StreamEncoder.padding_rest` (`stream_driver.rs:597`) | `QC_STATE.padding.paddingRest` | `sampleRate` | before QC, each frame | yes | **No** (not read by VBR grants) | `s6_padding_chain_matches_oracle` |
| PE/grant history | `DistributeState` (`rate.rs:472-489`) | `ATS_ELEMENT` distribution fields | `AdjThrInit` | `DistributeBits` / gain loop | yes | **No** (only feeds `grantedPe`/`Corr`, unused at VBR) | `s6_grant_state_evolution_onset_chain` |
| Per-frame grants | `QcFrameOutput.{granted_dyn_bits,max_dyn_bits,total_avail}` | `QC_OUT` grant fields | computed per frame | `prepareBitDistribution` | no | only when constraints bind; never here | `s6_corpus_exact`; live census |
| Fill / alignment | `QcFrameOutput.{tot_fill_bits,align_bits,total_bits}` | `QC_OUT` fill fields | computed per frame | `updateFillBits` / `Finalize` | no | accounting only; follows static/dyn | 2,442-frame identity census |
| Transport header | `QcFrameInput.glob_hdr_bits = 56` | `QC_STATE.globHdrBits` | transport probe | finalize | effectively constant | **No** at ADTS | `S6BITR` all 20 |
| S2/ATS chaos | `AtsElement.chaos_measure_old` | `ATS_ELEMENT.chaosMeasureOld` | `0.3` | threshold VBR | yes | **Yes, but not rate control** | separate cross-frame investigation |

The last row is deliberately separated. The ATS/S2 chaos accumulator is real
cross-frame threshold state; it was audited separately and is not part of the
rate-control/reservoir question. The rate-control-owned state above it has no
path to the threshold or quantiser at VBR-5.

## 4. Verified invariants and corpus results

### 4.1 Live production invariants

The M5.i census was regenerated over the full 19-stream inventory
(`M52_FIXTURES` + `M55_FIXTURES`) on the current tree:

```text
M5.g total frames observed: 2442
M5.i GLOBAL max_iter=0
  min_grant=986   @ m3-noise-mono-48k f=1
  min_maxdyn=1042 @ m3-noise-mono-48k f=1
  min_frame=984   @ m3-noise-mono-48k f=1
  min_cap=1045    @ m3-noise-mono-48k f=1
```

All 2,442 live frames satisfy:

- `npass == 1`;
- `crash == -1`;
- `last_iterations == 0`;
- `usedDyn < elementGrant`, `usedDyn < maxDyn`, `sumBits < totalAvail`;
- `static + usedDyn < nChannels * 6144`;
- `total = static + usedDyn + sizedFill + align + 3`;
- `adts_len * 8 = total + 56`;
- `totFill` is 0 or 7 bits; 287 frames carry 7 fill bits and no frame carries
  more.

Evidence: `docs/evidence/aac-enc-rate-control/vbr5-invariants.tsv`,
`frame-divergence.tsv`, and the M5.g/M5.i census artifact.

Peak observed frame demand:

| Path | Max `totalBits` | Max `usedDyn` | Cap | Headroom |
|---|---:|---:|---:|---:|
| mono | 5,104 (`m3-noise-mono f1`) | 5,042 | 6,144 | 984 incl. 56-bit header |
| stereo | 7,440 (`m7-music-stereo f429`) | 7,387 | 12,288 | 4,792 incl. header |

No frame approaches the cap by the QC decision; the closest boundary is the
mono noise frame at 984 bits above the frame budget.

### 4.2 Same-input exactness of the QC stage

The committed S6 oracle suite replays the exact committed inputs and compares
every grant, per-pass trace, fill, final total, and pre/post state tuple. The
set includes the known-stale D-021 SHORT vector `onset-stereo f24`, which is
used only as a formula pin, not as real-C ground truth:

- `rate::tests::s6_corpus_exact`: **20/20 frames**, bit-exact on the committed
  vectors.
- `rate::tests::s6_grant_state_evolution_onset_chain`: five consecutive
  onset frames, with `PRE(f+1) == POST(f)` state chaining, exact.
- `rate::tests::s6_init_vectors_match_oracle`: 3/3 operating points.
- `rate::tests::s6_padding_chain_matches_oracle`: all 20 `S6BITR` values.
- `rate::tests::s6_fill_and_finalize_vectors`,
  `s6_reduce_and_bitres_errors`, retry/crash synthetic tests: pass.

This is the decisive local test: on equal inputs the rate-control arithmetic,
update order, fill accounting and reservoir pin reproduce FDK. The remaining
corpus differences therefore enter from the current-frame inputs, not from a
local rate-control formula.

`qc6` state table: `docs/evidence/aac-enc-rate-control/s6-vectors.tsv`.
Onset chain: `docs/evidence/aac-enc-rate-control/onset-state-chain.tsv`.

### 4.3 The onset chain

The C vectors evolve the grant state as expected, while the reservoir remains
pinned and the gain loop remains single-pass:

| Frame | pre `peLast` | pre `dynLast` | post `peLast` | post `dynLast` | granted PE | `bitResTot` |
|---:|---:|---:|---:|---:|---:|---:|
| 22 | 10,065 | 18 | 10,065 | 18 | 10,065 | 12,288 |
| 23 | 10,065 | 18 | 10,065 | 18 | 10,065 | 12,288 |
| 24 | 10,065 | 18 | 9,829 | 456 | 9,829 | 12,288 |
| 25 | 9,829 | 456 | 9,929 | 684 | 9,929 | 12,288 |
| 26 | 9,929 | 684 | 10,028 | 317 | 10,028 | 12,288 |

Note: `qc6_onset-stereo_f24` is the stale oracle SHORT vector identified in
the D-021 archive; its S6 totals are not treated as real-C ground truth. It
is included only because the Rust chain test pins the ported formula on the
committed vector. Frames 22, 23, 25 and 26 are consistent with the real C
frame sizes; f25 is the useful attribution case below.

## 5. Findings and earliest divergences

### 5.1 Stable sustained case: `m3-noise-mono-48k` f15

This is the cleanest counterexample to a carried rate-control cause:

- f0–f14 are **byte-identical** to the C reference.
- f15 is the first byte difference and the first frame-length difference:
  Rust 443 bytes, C 442 bytes, a difference of one byte (8 bits).
- The Rust snapshot at f15 is `static=29`, `usedDyn=3452`, `total=3488`,
  `iterations=0`, `npass=1`, `crash=-1`; `grant=6056`, so the dyn grant
  headroom is 2,604 bits and the frame has 2,600 bits of budget headroom.
- The phase-1 oracle comparison shows equal `static=29` and equal `PE=6809`;
  the C total is 3,480 versus Rust 3,488. The difference is in the dynamic
  bits or their alignment, not in the grant or reservoir decision.
- Because every preceding frame is byte-identical, no carried-state
  difference is observed entering f15; in any case the VBR rate-control
  fields are inert even if they differ. The first difference is therefore a
  same-frame input/decision difference, not cross-frame rate-control
  propagation.

Classification: **upstream propagation / numerical divergence**, no
rate-control defect demonstrated.

### 5.2 Transient case: `m2-onset-stereo` f24–f26

The onset transition gives a case with both a known SHORT-frame reference
caveat and a valid real-C f25 comparison:

| Frame | C len | Rust len | Rust static | Rust usedDyn | Rust total | `iter` | `npass` |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 24 | 309 | 337 | 347 | 2,289 | 2,640 | 0 | 1 |
| 25 | 119 | 119 | 200 | 687 | 896 | 0 | 1 |
| 26 | 58 | 58 | 88 | 317 | 408 | 0 | 1 |

At f25 the C S6 vector has `static=208`, `usedDyn=684`, `total=896`; Rust
production has `static=200`, `usedDyn=687`, `total=896`. The 8-bit static
difference and 3-bit dynamic difference cancel in the final total. The C
truth is reproduced exactly when the C inputs are fed to `qc_main_frame`;
the production difference is already present in the S1 static/PE inputs. The
f24 length difference is the documented D-021 SHORT-frame divergence, not a
rate-control effect; f24 also runs single-pass with zero gain adjustments.
The `qc6` f24 vector is the stale oracle SHORT staging; it is used only as a
formula pin, not as real-C ground truth.

Classification: **upstream propagation** for f25; **D-021 numerical
divergence** for f24. No rate-control defect.

### 5.3 Known block-sequence case: `m7-speech-clean-stereo`

The stream already differs in payload at f0 (length 202 bytes on both sides)
and first differs in length at f5 (Rust 251 vs C 249). The known block-switch
divergence is later, at f107–f109:

| Frame | C bytes | Rust bytes | Rust usedDyn | Rust grant | `iter`/`npass` |
|---:|---:|---:|---:|---:|---|
| 107 | 276 | 267 | 1,972 | 12,130 | 0 / 1 |
| 108 | 339 | 340 | 2,616 | 12,186 | 0 / 1 |
| 109 | 268 | 304 | 2,323 | 12,186 | 0 / 1 |

The block-switch investigation already attributes this to the f106
`accWindowNrg` fixed-point/float energy difference, not to QC. The rate-control
margins at f107–f109 are enormous and the gain loop never runs. This is a
separate cross-frame *front-end* divergence, not a reservoir feedback path.

Classification: **confirmed numerical divergence**, already accounted for;
not rate control.

### 5.4 Tail and flush

No reservoir drain or tail-specific QC state exists at VBR-5. The feeder emits
`ceil((S + 1600) / 1024)` frames with zero padding past the input; the last
frames use the same `minBits=0`, fill-alignment-only rules. The M5.d flush
validation remains valid. Examples:

- `m2-click-mono` final f24: C 321 bytes, Rust 320 bytes; the first difference
  is already at f2, and the final frame is single-pass with zero gain
  adjustments.
- `m8-music-eroica-stereo` final f468/f469/f470: C−R deltas are −3/−2/−1 bytes,
  with no gain-loop or reservoir event.

Classification: **no material rate-control consequence established**; the
tail differences are examples of the same upstream boundary residue.

### 5.5 Corpus-wide first-divergence ordering

`docs/evidence/aac-enc-rate-control/frame-divergence.tsv` records, for all 19
streams:

| Quantity | Result |
|---|---:|
| Streams | 19 |
| Frames (both sides) | 2,442 |
| Byte-identical frames | 408 |
| Frames with equal length | 922 |
| Streams with at least one payload difference | 17 |
| Streams with at least one length difference | 16 |
| Cases where first length difference precedes first payload difference | **0** |

In every stream the first length difference is at the same frame as, or later
than, the first payload difference. A frame-size-only divergence on a
byte-identical payload never occurs. That is the expected signature of
downstream accounting responding to an upstream content difference, not of an
independent rate-control decision.

## 6. Why the old “reservoir propagation” hypothesis does not apply

The pre-audit triage reasoned that once frame sizes differ, `bitResTot`,
`paddingRest`, `peLast` and `dynBitsLast` “necessarily differ” and “feed later
grants.” The first half is true only as a record of computed values; the
second half is false in this configuration:

| Field | Does it differ when frame size differs? | Does it feed a later VBR decision? |
|---|---|---|
| `bitResTot` | It is always reset to `maxBits`; it does not retain the difference. | No. It is not a carry balance here. |
| `paddingRest` | It evolves from the nominal rate and sample rate, independent of content/frame size. | No. VBR grants use `maxBits`, not `avgTotalBits`. |
| `peLast` / `dynBitsLast` | They can differ if PE/granted bits differ. | They feed `grantedPe`/`grantedPeCorr`, which VBR threshold adaptation ignores. |
| `peMin` / `peMax` | They can differ through `bitresCalcBitFac`. | They feed only the unused grant PE calculation. |

So the state fields can be non-zero or different while having no output path.
This is a stronger result than “the difference is small”: the VBR-5
configuration has no feedback edge from them to the quantiser.

## 7. Demonstrated output consequences and tested scope

**Demonstrated:**

- Frame lengths and payloads differ on 17/19 streams; 16/19 have at least one
  length difference. The differences begin in the same frame as the first
  payload difference (or later).
- The maximum local length deltas in the retained comparison are 81 bytes
  (`m3-multitone`), 58 bytes (`m7-speech-clean`), 36 bytes
  (`m7-speech-pauses`), 28 bytes (`m2-onset` and `m2-click`), and 21 bytes
  (`m8-music-eroica`).
- The known block-switch sequence change remains the only demonstrated
  output-changing cross-frame state divergence.

**Not demonstrated:**

- Any case where a different rate-control state alone changes a QC decision,
  gain, section, frame size, or later allocation.
- Any case where a different reservoir level changes the coded output.
- Any perceptual consequence. No listening or ABX test was run; frame sizes,
  byte equality, and SNR are not perceptual evidence.

## 8. Is a new C-side QC capture justified?

**No, not for this question on the VBR-5 path.**

The minimum capture that would have been needed to test the old hypothesis
would be a C dump of `QC_STATE.bitResTot`, `padding.paddingRest`,
`QC_OUT.grantedDynBits/maxDynBits/usedDynBits/staticBits/totFillBits/alignBits/
totalBits`, and the `ATS_ELEMENT` grant tuple for the frames around
`speech-clean` f107 or `onset` f25. But such a capture would only show that
those values differ after the inputs differ; it could not create a VBR output
path that the source does not contain. The exact S6 vectors and the
single-pass corpus already answer the local-formula question.

A future capture would be justified only if the project expands the validated
scope to:

1. CBR / reduced / disabled reservoir modes, where `bitResTot` and
   `grantedPeCorr` are live; or
2. A production multi-element/multichannel path whose grant distribution
   differs from the single-element arm; or
3. A real retry/crash frame, where the QC loop actually changes gains or
   drops bands.

For those, the minimum useful capture is the per-frame S6 state plus the S4/S5
inputs that feed it, because otherwise a state comparison is confounded by
upstream input differences. No such capture was built here.

## 9. Validation

### 9.1 Focused tests

| Command | Result |
|---|---|
| `cargo test --locked --features aac_enc --lib aac_enc::rate::tests` | **22 passed, 0 failed** |
| `cargo test --locked --features aac_enc --lib m5g_branch_census -- --nocapture` | **1 passed**; 2,442 frames; global minima recorded |
| `cargo test --locked --features aac_enc --lib aac_enc::stream_driver::m52_tests::m52_all_fixtures_write_complete -- --nocapture` | **1 passed**; 15 streams |
| `cargo test --locked --features aac_enc --lib aac_enc::stream_driver::m52_tests::m55_streams_match_c_reference -- --nocapture` | **1 passed**; 4 streams; **artifact-dependent fallback** |
| `PHASE1_ORACLE_DIR=… PHASE1_C_DIR=… PHASE1_OUT=… cargo test --locked --features aac_enc --lib aac_enc::stream_driver::phase1_tests::phase1_full_stream_parity -- --nocapture` | **1 passed**; 558 compared; 286 exact, 7 known-short, 12 flush-tail, 253 new, 0 skipped |
| `cargo test --locked --features aac_enc --lib aac_enc::block_switch::tests::speech_clean_f107_attack_boundary_is_float_limited` | pass (existing regression) |

The `m55` result is not a self-contained pass: without `M55_C_DIR` it uses the
hard-coded fallback directory on this machine. The C files are retained
reference outputs; no new C encode was performed for this audit.

### 9.2 Full validation

| Command | Result |
|---|---|
| Full library suite with reference artifacts (`PHASE1_ORACLE_DIR`, `PHASE1_C_DIR`, `AAC_DEC_BIN`, `M55_C_DIR`) | **340 passed, 0 failed** |
| Full library suite without environment variables | **330 passed, 10 documented artifact-dependent failures** |
| `cargo test --locked --features aac_enc --doc` | **19 passed, 0 failed** |
| `rust/tools/check-licensing.sh` | pass; 67 `rust/src` files carry the modified-version notice |
| `rustfmt --edition 2021 --check src/aac_enc/rate.rs` | clean |
| `rustfmt --edition 2021 --check src/aac_enc/stream_driver.rs` | pre-existing formatting differences at unrelated lines; not reformatted per guardrail |
| `git diff --check` on changed text files | pass |

The 10 no-env failures are the same documented `m5a`/`phase1e`/`phase1f`/
`phase1g`/`phase1i` artifact-dependent set as before. They are not counted as
self-contained passes. The `stream_driver.rs` formatting check failure is in
pre-existing worktree changes; this audit did not edit Rust files.

## 10. Files added or modified

| File | Change |
|---|---|
| `docs/aac-enc-rate-control-divergence-audit.md` | This report. |
| `docs/evidence/aac-enc-rate-control/README.md` | Method, field dictionary, reproduction, limitations. |
| `docs/evidence/aac-enc-rate-control/vbr5-invariants.tsv` | Per-stream 2,442-frame invariant/margin summary. |
| `docs/evidence/aac-enc-rate-control/frame-divergence.tsv` | Per-stream first payload/length divergence and totals. |
| `docs/evidence/aac-enc-rate-control/frame-lengths-focused.tsv` | Frame-level C/R lengths and QC state for seven representative streams. |
| `docs/evidence/aac-enc-rate-control/s6-vectors.tsv` | 20 committed S6 vectors: grants, fill, totals, reservoir pin. |
| `docs/evidence/aac-enc-rate-control/onset-state-chain.tsv` | C pre/post grant-state chain for onset f22–f26. |
| `docs/evidence/aac-enc-rate-control/phase1-summary.txt` | Exact Phase-1/M55 comparison counts and fallback caveat. |
| `docs/evidence/aac-enc-rate-control/m5g-summary.txt` | Live 2,442-frame census global minima and invariants. |
| `docs/README.md` | Index entry for this audit. |
| `docs/aac-enc-numerical-divergence-triage.md` | Rate-control/bit-reservoir status updated from “insufficient” to closed for VBR-5, with pre-audit history retained. |

No production or test Rust code was changed by this audit. No C oracle,
fixture, or reference artifact was regenerated or replaced. No commit or push
was performed.

## 11. Limitations and unresolved evidence gaps

- **VBR-5 only.** The result is specific to `bitResMode = FULL` and the
  validated mono/stereo 44.1/48 kHz ADTS path. CBR and reduced/disabled
  reservoir modes have different feedback paths and are out of scope.
- **Corpus scope.** 19 streams / 2,442 frames, all committed. This is not a
  proof for arbitrary signals; it is a bounded empirical audit plus source
  proof for the specific VBR path.
- **No production C QC capture.** The production `DistributeState` values are
  not compared frame-by-frame with C. That is not needed for the defect
  question because the state has no output path at VBR-5; it would be needed
  to diagnose diagnostic-value differences in another mode.
- **M55 stage attribution.** The licensed fixtures have C ADTS outputs but no
  S1/S4/S5 oracle dumps, so the exact upstream stage of their first payload
  difference is not localized. The rate-control exclusion for these streams
  rests on the exact initial state, the absence of constraint crossings, and
  the source-level inertness of the rate state.
- **D-021 short-frame reference.** `qc6_onset-stereo_f24` is the stale oracle
  SHORT vector; it is not used as real-C ground truth. The real C ADTS frame
  length is used only for the frame-size comparison.
- **No perceptual claim.** No listening, ABX, or perceptual metric was run.
  Byte equality and frame-size equality are not quality evidence.
- **No new C oracle.** All C comparisons use retained artifacts and committed
  vectors. A future mode expansion would need a new, narrowly scoped capture;
  it was not built here.
