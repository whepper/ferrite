# Cross-frame state drift investigation

Date: 2026-10-27. Status: **diagnostic complete; no production change; no
commit or push.**

## 1. Executive conclusion

**Verdict: PASS WITH FINDINGS.** No porting defect was found in persistent
state initialization, reset, update order, history indexing, or frame-boundary
handling. The investigated cross-frame differences separate into:

1. **Block-switching numerical divergence** (known, material): the f106
   `accWindowNrg` history and the f107 window-0 attack test differ because of
   FDK's fixed-point per-sample energy truncation versus Rust's f32
   accumulation. This changes the coded window sequence at f107–f109.
2. **ATS/S2 upstream propagation** (no material consequence established):
   the f25 onset and f3 sweep ATS/S2 chaos values differ because their S1
   inputs already differ. The PE and S2 formulas are exact on C inputs. A
   test-only injection of the C state at the first consuming frame left the
   following coded frames unchanged.
3. **Pre-echo and transform state** (no material consequence established):
   pre-echo history is exact through the START/SHORT/STOP/LONG onset
   transition and differs by ≤25 Q31 LSB elsewhere in the scanned corpus;
   transform ring scheduling is exact and MDCT parity is tolerance-gated.

A key state-inventory result is that **not every mutable ATS field is
cross-frame state**. `chaosMeasureEnFac` is overwritten by the current frame's
`calcWeighting` before it is used; only `lastEnFacPatch` is read as history,
and it is exact in the studied cases. The derivative `chaosMeasureOld` **is**
persistent, but its observed differences are upstream-driven and did not
change local coded output under the counterfactual injection.

**Classification:** block switch = intentional float-native numerical
divergence; ATS/S2 drift = upstream propagation with no material consequence
established locally; pre-echo/transform = no material consequence established;
no verified porting defect.

## 2. State inventory

Only state that survives a frame boundary is listed. `TnsChannel` runtime
decisions are cleared every frame (`rust/src/aac_enc/psy.rs:903-905`) and are
not cross-frame state. `PsyScratch` and S4 workspaces are reused but reset
before use.

| State | Rust | FDK C | Init / reset | Read/update order | Scope | Cross-frame effect |
|---|---|---|---|---|---|---|
| Block-switch history | `BlockSwitch` (`block_switch.rs:276-312`), `process` `:341-399`, energy `:456-477` | `BLOCK_SWITCHING_CONTROL` (`block_switch.h:123-148`); `FDKaacEnc_BlockSwitching` (`block_switch.cpp:229-346`); `CalcWindowEnergy` `:359-409` | `new` mirrors `InitBlockSwitching`: LONG/KBD, zero energies/IIR | previous attack/energies copied at frame start; current energies then attack loop | per channel | direct: attack/sequence/grouping |
| Transform ring and MDCT overlap | `ChannelTransform` (`transform.rs:175-255`) | `PSY_STATIC.psyInputBuffer`, `mdctPers`, `lastWindowShape` (`psy_data.h:142-151`); staging `psy_main.cpp:490-627` | zeros / SINE | fresh samples staged before transform; ring rotates after transform | per channel | current spectrum; ring schedule exact, MDCT tolerance |
| Pre-echo thresholds | `PreEchoState` (`thresholds.rs:184-267`) | `PSY_STATIC.sfbThresholdnm1`, `mdctScalenm1`, `calcPreEcho` (`psy_data.h:147-149`); `psy_main.cpp:975-1006`; `pre_echo_control.cpp:117-176` | PCM long floor, scale 8, enabled | `advance_thresholds` applies control; STOP/START resets around it | per channel | direct: next-frame threshold cap |
| MS Ld history | `PsyChannel.ms_ld` (`psy.rs:151-160`); driver `stale_ms_ld` (`stream_driver.rs:599-604`) | `PSY_DATA.sfbEnergyMSLdData` (`psy_data.h:160`); `psy_main.cpp:1030-1084` | zeros | written on long frames / grouped short refresh; short decisions use fresh grouped vectors | per channel/window | primarily output/diagnostic; no causal divergence found |
| ATS weighting value | `AtsElement.chaos_measure_en_fac` (`pe.rs:210-216`) | `ATS_ELEMENT.chaosMeasureEnFac` (`adj_thr_data.h:163`) | zero | **overwritten** by `calc_weighting` (`pe.rs:438-513`; C `adj_thr.cpp:755-879`) | per channel | not read as history; value difference is not causal |
| ATS patch flag | `AtsElement.last_enfac_patch` | `ATS_ELEMENT.lastEnFacPatch` | zero | previous value selects the two-frame patch in `calc_weighting` | per channel | causal history; exact in scanned cases |
| ATS VBR chaos | `AtsElement.chaos_measure_old` | `ATS_ELEMENT.chaosMeasureOld` | `0.3` (`ATS_VBR5_CHAOS_OLD`; C `AdjThrInit`) | updated in `reduce_thresholds_vbr` (`threshold_vbr.rs:795-802`; C `adj_thr.cpp:1170-1178`) | element | direct: VBR threshold reduction |
| Rate-control / bit reservoir | `DistributeState` (`rate.rs:472-489`), `QcState` (`rate.rs:1398-1413`), `padding_rest` | `ATS_ELEMENT` distribution fields; `QC_STATE.bitResTot`/`bitResTotMax`; `QC_STATE.paddingRest` (`qc_data.h:144,280-281`) | exact VBR-5 init | after each frame's grants/size | element/stream | downstream of frame-size differences; not a root cause established here |

The C `ATS_ELEMENT` fields are at `adj_thr_data.h:153-164`; init values at
`adj_thr.cpp:2533-2595`. The Rust ownership is in `StreamEncoder`
(`stream_driver.rs:574-604`).

### Initialization and reset checks

- `BlockSwitch::new` matches `FDKaacEnc_InitBlockSwitching` for the LC path;
  the initial-state test already pins this.
- `PreEchoState::new` matches `FDKaacEnc_InitPreEchoControl` (PCM floor, scale
  8, enabled); `reset_for_transition` matches the STOP-before/START-after
  ordering in `psy_main.cpp:975-1006`.
- `AtsElement::new` sets `chaosMeasureOld = 0.3` and zeroes the S1
  `chaosMeasureEnFac`/`lastEnFacPatch`, matching `AdjThrInit` plus zeroed
  encoder RAM.
- `DistributeState`/`QcState` init is already pinned exactly against the
  committed S6/QC init vectors.

No init/reset mismatch was found.

## 3. Earliest divergence: onset f22–f26

The full consecutive trace is in
`docs/evidence/aac-enc-cross-frame-state-drift/onset-state-trace.log`. ATS
state, pre-echo state, and sequence agree through f24. The first divergence is
after f25 PE:

| Quantity | C | Rust |
|---|---:|---:|
| f25 ATS pre | 1610612736, 1610612736, 1, 1 | same |
| f25 ATS post ch0 | **407,797,760** | **410,386,432** |
| f25 ATS post ch1 | 402,653,184 | 402,653,184 |
| f25 `lastEnFacPatch` | 0, 0 | 0, 0 |
| f25 S2 chaos post | **401,997,824** | **402,063,360** |
| f26 ATS pre ch0 | **407,797,760** | **410,386,432** |
| f26 ATS post ch0 | 402,653,184 | 402,653,184 |
| f26 S2 chaos pre/post | 401,997,824 / **205,848,576** | 402,063,360 / **220,004,352** |

Frame sizes are 119 bytes at f25 and 58 bytes at f26 on both sides. The f26
S1 post-state reconverges because the current frame re-derives the weighting;
the S2 chaos accumulator remains different and is the lasting state.

The f25 S1 max differences (channel 0) are `SPEC[183]=4,198`,
`SENL[10]=1,392`, `PENLD[37]=47,151,681`, `PTHLD[44]=0`, and
`FF[39]=12,846,591`. Replaying `pe_calculation` with C inputs yields C's
`ATS post` exactly (`407,797,760`); with production inputs it yields
production (`410,386,432`). C `PENLD` alone and C `FF` alone each cross the
same boundary and yield `407,797,760`. The state update is therefore not the
cause; the upstream energy/spectrum-derived inputs are.

Pre-echo history (`nm1`), `m_nm1`, and `calcPreEcho` are exact for every
channel/frame pair f22–f26. The sequence is exact LONG → START → SHORT → STOP
→ LONG. This excludes pre-echo reset/order and sequence state as the cause of
the ATS divergence.

## 4. Earliest persistent divergence: sweep f0–f10

The full trace is in
`docs/evidence/aac-enc-cross-frame-state-drift/sweep-state-trace.log`.
f0–f2 ATS pre/post and f0/f2 chaos are exact. f1 has a tiny, transient S2
chaos difference (C 1,033,460,135 vs Rust 1,033,476,519, 16,384 Q31), which
reconverges at f2. The first **persistent** divergence is f3:

| Quantity | C | Rust |
|---|---:|---:|
| f2 ATS post | 420,708,352, 402,653,184, 0, 0 | same |
| f3 ATS pre | 420,708,352, 402,653,184, 0, 0 | same |
| f3 ATS post ch0 | **874,971,136** | **740,753,408** |
| f3 S2 chaos post | **239,796,224** | **268,500,992** |
| f4 ATS pre ch0 | **874,971,136** | **740,753,408** |
| f10 ATS pre/post ch0 | **753,664,000 / 828,506,112** | **676,200,448 / 683,933,696** |

f3 S1 max differences (channel 0) are `SENL[9]=13,210`,
`PENLD[44]=20,221,257`, `PTHLD[12]=67,112,777`, `FF[19]=5,325,340`, and
`SPEC[29]=19,136`. `pe_calculation` replay:

```text
C ATS post         874,971,136, 402,653,184, 0, 0
Rust all inputs    740,753,408, 402,653,184, 0, 0  (= production)
C energy inputs    874,971,136, 402,653,184, 0, 0  (= C)
C PENLD only       846,594,048, 402,653,184, 0, 0
C FF only          756,219,904, 402,653,184, 0, 0
```

Both energy lanes contribute; the full C energy vector is required for the
exact C value. `lastEnFacPatch` is 0 on both sides at f3 and f10. The ATS
divergence is upstream propagation, not a state-update defect, and it persists
through f10.

The prior M5.26 audit saw the f10 ATS drift but could not trace its origin
because only f10 had been extracted. The full oracle dump contains every
frame; this trace closes that gap.

## 5. What is actually causal state?

`calcWeighting` overwrites `chaosMeasureEnFac` from the current frame's active
line count (`pe.rs:481`; C `adj_thr.cpp:816-822`). It reads the **previous**
`lastEnFacPatch` to decide whether to execute the patch. Therefore:

- `chaosMeasureEnFac` differences are recorded values, not cross-frame inputs.
  Injecting a different `chaosMeasureEnFac` alone cannot change the next
  frame's weighting.
- `lastEnFacPatch` is the actual prior-frame flag. It is 0/0 at onset f25/f26
  and sweep f3/f10; no divergence was found.
- `chaosMeasureOld` is a real S2 recurrence input. Its differences are
  upstream-driven, and the S2 formula itself is exact on C inputs
  (`threshold_vbr::tests::s2_full_matches_oracle_with_injected_chaos`).

This distinction explains why the large `chaosMeasureEnFac` values can differ
while the coded output remains identical: the large value is a diagnostic
snapshot, not a read-only history input.

## 6. Counterfactual downstream result

Test-only injection used the C post-frame state at the first frame that reads
it:

- **Onset:** set C f25 ATS post (`407,797,760, 402,653,184, 0, 0`) and C f25
  S2 chaos post (`401,997,824`) before f26. f26–f28 emitted bytes and all
  coded fields (`seq`, static/huff/scf/dyn, total) were identical to the
  unmodified Rust run.
- **Sweep:** set C f3 ATS post (`874,971,136, 402,653,184, 0, 0`) and C f3 S2
  chaos post (`239,796,224`) before f4. f3–f10 bytes and coded fields were
  identical to the unmodified run.

This is evidence of **no local material consequence**, not a proof of
harmlessness over arbitrary horizons. The frame-size differences in these
spans already exist before the injected state is consumed, so they are
upstream output differences rather than consequences of the injected ATS
values.

The one state divergence with demonstrated coded-output consequence remains
the block-switch case: f106/f107 reset the attack decision and produce a
different sequence and local frame sizes. That is already documented in
`docs/aac-enc-block-switch-oracle-investigation.md` and pinned by
`block_switch::tests::speech_clean_f107_attack_boundary_is_float_limited`.

## 7. Downstream consequences

| Case | State change | Coded consequence |
|---|---|---|
| Speech f106/f107 | block-switch energies/accumulator | f107 START vs LONG; f108 SHORT vs LONG; f109 STOP vs LONG; local frame sizes/decoded waveform differ |
| Onset f25/f26 | ATS post, S2 chaos | f25/f26 sizes equal (119/58); injected C state leaves f26–f28 bytes/fields unchanged |
| Sweep f3–f10 | ATS/S2 chaos | sizes already differ upstream; injected C f3 state leaves f4–f10 bytes/fields unchanged |
| Pre-echo | none beyond ≤25 Q31 LSB | no decision change found |
| Transform overlap | MDCT/ring | ring schedule exact; MDCT tolerance gate; no state-order defect |

Rate-control state (`bit_res_tot`, `padding_rest`, `peLast`/`dynBitsLast`) is
downstream of emitted frame sizes. It can carry differences forward once frame
sizes differ, but this investigation did not find a rate-control state bug or
attribute any early divergence to it.

## 8. Production change policy and recommendation

**No production change is warranted.** The formulas, initialization, update
order, and resets match FDK; the observed differences are either the known
float-native precision class or upstream propagation into state whose local
coding effect is not demonstrated. Changing ATS/S2 arithmetic to force C
parity would compensate for upstream spectrum/energy differences and could
change unrelated decisions without quality evidence.

Keep the existing block-switch regression and the new cross-frame state pin.
If future quality work targets long horizons, rerun the state injection over a
longer fixture and add a frame-size/bit-reservoir trace before considering any
state-specific change.

## 9. Regression coverage and evidence

Added in `rust/src/aac_enc/stream_driver.rs`:

`aac_enc::stream_driver::phase1_tests::cross_frame_state_onset_f25_is_upstream_and_output_neutral`

It is self-contained (committed `qc1`/`qc2`/`psy` vectors and fixture PCM; no
external artifacts) and asserts:

- ATS pre/post and S2 chaos are exact through f24;
- f25 ATS post diverges (C 407,797,760 vs Rust 410,386,432), `lastEnFacPatch`
  stays 0, and S2 chaos diverges;
- f26 starts from the divergent ATS value but the S1 post-state reconverges;
- pre-echo history, scale, enabled flag, and sequence are exact through the
  START/SHORT/STOP/LONG transition;
- `pe_calculation` on C inputs reproduces C f25 ATS post and on Rust inputs
  reproduces production;
- injecting the C f25 ATS/chaos state before f26 leaves f26–f28 bytes and
  coded fields unchanged.

The block-switch boundary test remains the regression for the one case with a
demonstrated sequence consequence.

Evidence artifacts:

- `docs/evidence/aac-enc-cross-frame-state-drift/README.md`: method, state
  tables, attribution, injection results.
- `onset-state-trace.log`, `onset-injection.log`, `sweep-state-trace.log`,
  `sweep-injection.log`, `corpus-state-scan.log`.

## 10. Tests run

| Command / test | Result |
|---|---|
| New cross-frame state test | pass |
| `aac_enc::block_switch::tests::speech_clean_f107_attack_boundary_is_float_limited` | existing pass |
| Full suite with reference artifacts | **339/339 pass** |
| Full suite without external artifacts | 329 pass / 10 documented reference-artifact failures |
| `cargo test --locked --features aac_enc --doc` | 19/19 pass |
| `cargo test --locked -p aac-convert` | pass |
| `rustfmt --check` on changed files | changed regions clean; whole-file `stream_driver.rs` still has the two pre-existing unrelated formatting diffs |
| `rust/tools/check-licensing.sh` | pass |

## 11. Limitations

- The counterfactual injection covers onset f26–f28 and sweep f3–f10 only; it
  does not prove that a state difference cannot matter later.
- The full C oracle dump was used for the sweep prefix; no new C encoder probe
  was built. The dump’s ATS/chaos values are the reference harness’s state,
  and the f0–f2 equality establishes that the prefix is aligned.
- Pre-echo was scanned on 19 committed S1 cases; small Q31 residue (≤25 LSB)
  is within the existing stored-energy envelope but is not proof for every
  possible signal.
- No perceptual metric or listening test exists. Byte equality, frame sizes,
  and SNR are not perceptual evidence.
- `ms_ld`/`stale_ms_ld` history is not fully traced into every short-block
  decision; the investigation only establishes that it is not the onset/sweep
  cause.
- Rate-control/bit-reservoir state was inventoried but not exhaustively
  replayed; it remains downstream of frame-size differences.

## 12. Files changed and worktree state

| File | Change |
|---|---|
| `rust/src/aac_enc/stream_driver.rs` | Added the permanent cross-frame state test; no production behavior changed. |
| `rust/src/aac_enc/psy.rs` | Added a test-only pre-echo state accessor. No production behavior changed. |
| `docs/aac-enc-cross-frame-state-drift-investigation.md` | This report. |
| `docs/evidence/aac-enc-cross-frame-state-drift/README.md` | Method, tables, attribution, injection results. |
| `docs/evidence/aac-enc-cross-frame-state-drift/*.log` | Diagnostic logs listed above. |
| `docs/README.md` | Index entry. |

Unrelated worktree changes were left untouched: the prior
f64/block-switch/TNS/MINSNR1 reports and evidence,
`docs/aac-enc-m5b-objective-quality-baseline.md`,
`docs/aac-enc-quality-divergence-audit-m5.25.md`, `rust/src/aac_enc.rs`,
`rust/src/aac_enc/block_switch.rs`, `rust/src/aac_enc/tns.rs`,
`rust/src/aac_enc/production_baseline.rs`, and
`tools/aac-enc-metrics/challenge/listening-review-worksheet.md`.

No production ATS, S2, block-switch, TNS, MDCT, or rate-control behavior was
changed. No commit or push was performed.

## 13. Classification summary

| Question | Answer |
|---|---|
| Initialization/reset defect? | No; init and reset paths match, and onset pre-echo/state is exact. |
| Update-order/history-index defect? | No; PE/S2 replay exact on C inputs; `lastEnFacPatch` exact. |
| Earliest block-switch state difference? | f106 `accWindowNrg` / f106 window-7 history; first decision at f107. |
| Earliest onset state difference? | f25 ATS post / S2 chaos post; f26 S1 post reconverges. |
| Earliest sweep persistent state difference? | f3 ATS post / S2 chaos post; f1 S2 chaos transient reconverges at f2. |
| Cause of ATS/S2 differences? | Upstream S1 energy/form-factor differences; S2 updates exact on C inputs. |
| Material downstream consequence? | Block switch: yes. ATS/S2: no local consequence demonstrated by injection. |
| Production change? | **No.** |
