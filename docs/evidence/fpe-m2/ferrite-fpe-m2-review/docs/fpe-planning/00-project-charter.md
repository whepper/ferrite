# 00 — Project Charter: Ferrite Float Psychoacoustic Engine (FPE)

Date: 2026-11-16. Status: approved-for-planning charter (owner review pending).

## 1. Mission

Produce a native Rust **floating-point implementation of the existing FDK-AAC
psychoacoustic model** as implemented in Ferrite's Q31 engine, faithful to the
upstream FDK-AAC algorithms, tuning, partitions, and temporal behaviour, and
integrated behind the same encoder interface.

**Fundamental principle: change the numerical representation, not the
psychoacoustic model.**

## 2. References and authority

| Role | Source | Authority |
|---|---|---|
| Primary algorithmic reference | Upstream FDK-AAC 3.0.0 C/C++ (`cpp/` tree at git `3878c45a`, pinned reference `f14ad40a…`) | Defines the model: algorithms, constants, tables, state transitions |
| Secondary reference / integration baseline | Ferrite Q31 engine (`rust/src/aac_enc/psy.rs`, `energy.rs`, `thresholds.rs`, `q31.rs`, …) at HEAD `a2c1d53` | Defines the validated integration contract and the differential baseline |
| Validation ground truth | `tools/enc-oracle/` dumps of the unmodified C encoder + committed `testdata/` vectors | Defines observable intermediate values |

Where the C reference and Ferrite's port differ, the difference is either a
documented float-native divergence (see
`docs/aac-enc-numerical-divergence-triage.md`) or a defect. FPE inherits the
**documented** divergences; it does not inherit defects, and it does not
"fix" documented divergences (that would change the model).

## 3. Goals (Stage 1)

1. Replace Q31 fixed-point **value arithmetic** in the psychoacoustic kernels
   with floating-point equivalents (precision choice justified in `05`/ADR-0002).
2. Preserve:
   - FDK algorithms and psychoacoustic design decisions;
   - frequency partitions and scalefactor-band relationships;
   - psychoacoustic tuning parameters and model constants (as tabulated in
     Ferrite's validated data files);
   - temporal behaviour and state transitions (pre-echo history, block-switch
     history, TNS state, MS-Ld staleness semantics);
   - long-window and short-window processing, including grouping;
   - channel and channel-pair processing semantics (common window, MS energies,
     LFE early-outs);
   - the existing interaction with the rest of the encoder (same output
     contract, same downstream components).
3. Keep the Q31 engine intact, available as reference/fallback, and the
   **default**. FPE independently selectable; never default until Stage-1
   acceptance and independent review complete.
4. Document every unavoidable numerical difference introduced by the
   representation change, with measured envelopes and tolerances.
5. Deliver a three-way differential validation capability (C ↔ Q31 ↔ FPE)
   at intermediate stages, not only final bitstreams.

## 4. Explicit non-goals (Stage 1)

- Inventing a new psychoacoustic model; replacing FDK masking theory.
- Introducing Bark/ERB models where FDK does not use them (FDK's Bark-like
  `BarcLineValue` is used only to derive the **tabulated** spreading factors
  and minSnr; FPE consumes the tables, it does not re-derive them).
- Changing frequency partitions, tuning parameters, tonality estimation,
  transient detection, or temporal masking design.
- Changing AAC quantization, scalefactor algorithms, rate control, bitstream
  encoding, VBR profiles, AAC-LC format, or the encoder interface beyond an
  additive selection knob.
- Introducing HE-AAC, PNS, or intensity-stereo changes.
- Unrelated encoder optimizations; frame/stream multithreading.
- Refactoring the existing Q31 engine (beyond additive, behaviour-preserving
  seams explicitly specified in `04`/`06`, if any prove necessary).
- Making FPE the default.
- Stage-2 improvements (see §7).

## 5. Scope boundary (where FPE starts and ends)

Verified pipeline (details in `01`):

```
PCM → BlockSwitch(f32) → MDCT(f32→f64) → [PSY] → LiveChannel(Q31) →
IS/MS decisions(Q31) → PE(S1) → VBR threshold adapt(S2) → SCF(S4) →
QC gain loop(S6, includes S5 bitcount) → bitstream(S8)
```

- **Inside FPE**: the processing currently performed by
  `psy::process_pair_reuse` — lowpass/zero check, rescale value arithmetic,
  TNS orchestration calls (TNS itself reused), threshold advance (clip,
  spreading, PCM floor, pre-echo), spread-energy computation, MS energy
  computation, short-block grouping value sums, output Ld vectors.
- **Outside FPE (reused unchanged)**: block switching, MDCT/filterbank, SFB
  configuration and tables, TNS detect/sync/encode internals, channel-pair
  window synchronization, IS/MS stereo decisions, PE, S2 VBR adaptation, SCF,
  quantization, rate control, bitcount, bitstream, ADTS, encoder lifecycle.
- **Boundary contract**: FPE emits the same `ChannelOutput`/`GroupedOutput`/
  `MsOutput` data (stored-domain `i32` energies/thresholds/spread, `f64` Ld
  vectors, `mdct_scale`, grouped vectors, MS vectors) that the Q31 engine
  emits, so `StreamEncoder::frontend`'s `LiveChannel` construction and all
  downstream stages are untouched.

## 6. Success criteria (summary; full gates in `07` §8)

1. **Differential fidelity**: FPE intermediates match the C oracle within the
   established envelopes (`05` §9, `07` §5) on the full corpus; decisions
   (window sequence, grouping, TNS activation, MS digest, maxSfbPerGroup,
   pre-echo enable/scale state) match at the same level the Q31 engine
   matches, with any FPE-specific knife-edge cases documented and pinned.
2. **No regression**: all existing Tier-A tests pass unchanged; Tier-B
   reference validation passes with FPE selected where applicable; Q31
   default output byte-identical to pre-project baselines.
3. **Numerical safety**: no NaN/Inf/negative-energy/invalid-threshold can
   reach downstream (invariants + boundary checks, `05` §8).
4. **Performance**: no material regression vs Q31 baseline without owner
   approval (`08`); correctness first.
5. **Provenance/licensing**: FPE **retains the existing FDK-AAC licence** (no
   separate licence; no change to SPDX identifier, licence files, NOTICE, or
   attribution approach). Notices and attribution per `12` (the project's
   documented compliance position for translated/derived code, not a
   categorical legal conclusion); licensing hygiene check passes.
6. **Independent review**: architecture, numerical, and validation reviews
   signed off per milestone gates (`09`).

Objective audio-quality measurements may supplement but **never replace**
faithful-reproduction gates. Numerical or metric differences are not claimed
as perceptual improvements.

## 7. Stage 2 (deferred, explicitly out of Stage 1)

After Stage-1 acceptance and independent audit, separate research may explore:
improved precision/stability of energy sums, improved transient handling,
alternative numerical formulations (e.g., pure-float headroom decisions,
float-recomputed spreading factors/minSnr), float-specific SIMD, and potential
psychoacoustic improvements. Each requires its own charter; none may be
smuggled into Stage-1 tasks.

## 8. Working rules

- Planning-only until a milestone task is explicitly delegated.
- No production behaviour change without an approved task spec.
- No commits/pushes by delegated models without owner instruction; focused
  diffs, independent review, no unrelated refactoring (`11`).
- Assumptions are listed explicitly; missing evidence is named with the
  investigation required (`03` §6, `10`).