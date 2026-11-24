# 04 — FPE System Architecture

Date: 2026-11-16. Recommendations here are recorded as ADRs (`adr/`).
Design constraints from the charter: Q31 engine intact and default; FPE
independently selectable; no duplicated downstream functionality; no
premature abstraction; no hot-path allocations beyond the existing pattern;
no global mutable state; clear temporal-state ownership; independently
testable numerics.

## 1. Module layout (ADR-0001)

The existing Q31 engine is a set of **flat modules** (`psy.rs`, `energy.rs`,
`thresholds.rs`, `q31.rs`, …). Reorganizing them into a `psy/` directory would
refactor the Q31 engine — forbidden. Therefore FPE is added as a **new,
self-contained module tree** beside the flat Q31 modules:

```
rust/src/aac_enc/
├── psy.rs              # Q31 engine — UNCHANGED (output types shared)
├── energy.rs           # Q31 kernels + repr-lane helpers — UNCHANGED
├── thresholds.rs       # Q31 threshold chain — UNCHANGED
├── q31.rs              # Q31 primitives — UNCHANGED (used at boundary)
├── block_switch.rs     # f32 — UNCHANGED (reused)
├── transform.rs        # f32 MDCT — UNCHANGED (reused)
├── tns.rs              # hybrid TNS — UNCHANGED (reused)
├── sfb.rs / sfb_tables.rs / psy_config.rs   # config/data — UNCHANGED (reused)
├── fpe/                # NEW: floating-point psychoacoustic engine
│   ├── mod.rs          # public entry: process_pair_reuse, FpeChannel,
│   │                   #   FpeTables, FpeScratch; re-exports
│   ├── constants.rs    # float tuning constants derived from Q31 literals
│   ├── energy.rs       # float band-energy kernels (long/short/MS/recompute)
│   ├── thresholds.rs   # float clip/spreading/PCM-floor/threshold-init
│   ├── preecho.rs      # FpePreEchoState (float temporal masking)
│   ├── orchestrate.rs  # per-pair frame orchestration (mirrors psy.rs order)
│   ├── output.rs       # boundary quantization + shared-output assembly
│   └── tests/          # unit + differential tests (cfg(test))
└── stream_driver.rs    # ADDITIVE selection seam only (see 06)
```

Rationale:
- **Separation**: FPE numerics live only under `fpe/`; the Q31 modules are
  read-only dependencies. A reviewer can diff `fpe/` in isolation.
- **No premature abstraction**: no generic `PsyEngine` trait framework. The
  two engines are selected by a small enum at one call site (`06`); they
  share **data types**, not a trait hierarchy. A trait would add dyn-dispatch
  or generic monomorphization complexity for exactly two implementations and
  no third planned — rejected (ADR-0001).
- **Independent testability**: each `fpe/` kernel is a pure function over
  slices/scalars, unit-testable without the encoder; `orchestrate.rs` is
  testable via the shared output contract against committed vectors.

## 2. Dispatch and selection (ADR-0001, details in 06)

```
enum PsyEngineKind { Q31, Fpe }   // internal, Copy, default Q31
```

- Stored on `StreamEncoder` (chosen at `with_config` from an additive
  `EncoderConfig` knob; default `Q31`).
- The **single dispatch point** is `StreamEncoder::frontend`
  (`stream_driver.rs:847/857`), which today calls `psy::process_pair_reuse`.
  It becomes a `match self.psy_engine { Q31 => psy::…, Fpe => fpe::… }`.
- Both arms return the **same** `(ChannelOutput, Option<ChannelOutput>,
  Option<MsOutput>)`; everything after (LiveChannel, IS/MS, PE, S2, S4, S6,
  S8) is untouched.
- No Cargo feature (none exist today; a feature would force compile-time
  selection and prevent same-binary differential testing). Runtime enum keeps
  both engines in one binary for three-way comparison.

## 3. Shared types and data (no Q31 modification)

FPE **imports and constructs** the existing output types:

| Type | From | FPE use |
|---|---|---|
| `ChannelOutput`, `GroupedOutput`, `MsOutput` | `psy.rs:247, 194, 384` | construct identical contract |
| `ChannelInput` | `psy.rs:356` | same input (f64 spectrum, seq, grouping, is_lfe) |
| `PsyTables` | `psy.rs:134` | reuse (SfbConfig f64 accessors, TnsConfig, minSnr) |
| `SfbConfig` | `sfb.rs:194` | read f64 mask/pcm/clip/bark + integer offsets/active |
| `TnsConfig`, `TnsChannel` | `tns.rs:160, 248` | reuse; FPE calls detect/sync/encode |
| `WindowSequence` | `block_switch.rs:146` | reuse |
| repr-lane helpers | `energy.rs:130/161/190/440`, `q31.rs` | reuse for headroom/shift decisions + boundary quantization |

If a needed item is private, the only permitted Q31-side change is an additive
`pub(crate)` visibility bump (recorded in the task spec). **No behaviour,
signature, or layout change to Q31 code.**

## 4. FPE state ownership

```
struct FpeChannel {
    preecho: FpePreEchoState,   // fraction prev thresholds + prev_scale + enabled
    tns_state: TnsChannel,      // reused type, same semantics
    tns_info:  TnsChannel,      // reused type
    ms_ld: [[f64; 64]; 8],      // MS-Ld history, stale-on-short (same as Q31)
}
```

- One `FpeChannel` per channel, owned by `StreamEncoder` (parallel to
  `psy_ch`), allocated at `with_config` when FPE selected.
- **M2 (pass-through):** `FpeChannel` holds the **Q31 `PreEchoState`**
  (stored-int history) and delegates control to it, so the history is
  bit-identical to Q31; no fraction conversion (`09` M2, `06` §4).
- **M6+:** `FpePreEchoState` (fraction): `prev_f: Vec<f64>` (= `prev_st/2^31`,
  51 entries), `prev_scale: i32`, `enabled: bool`. Init mirrors
  `PreEchoState::new`: `prev_f` = long PCM floors as fraction
  (`SfbConfig::pcm_quant()`, already `/2^31`), prev_scale = 8, enabled = true.
  Transitions identical to `thresholds.rs:207-246` (STOP-before/START-after,
  memorize-pre-clamp, scale-aligned clamp/floor; `05` §6.5).
- **No shared mutable state across channels** except the element-global
  decisions computed within a call (finalShift, tnsSpecShift), exactly as the
  Q31 engine.
- TNS state is the **same `TnsChannel`** type; because TNS is reused unchanged,
  its temporal behaviour is identical by construction.
- **M2 state equivalence:** the M2 gate compares **all frame-surviving state
  after every frame** of multi-frame sequences — `PreEchoState`
  (`prev/prev_scale/enabled`), `tns_state` + `tns_info`, and `ms_ld` — not just
  outputs. Private fields are compared via **`#[cfg(test)]` helpers** (no new
  production API): `PreEchoState` has pub accessors, `TnsChannel` fields are
  pub, `ms_ld`/`tns_state` need test-only accessors (`07` §3.1, `09` M2).

## 5. Scratch and memory (ADR-0001)

```
struct FpeScratch { /* float value-lane working buffers, L+R */ }
```

- Encoder-owned (one per `StreamEncoder`), passed `&mut` into
  `fpe::process_pair_reuse`, mirroring the `PsyScratch` discipline
  (`psy.rs:420-436`): every field reset to its exact pre-first-read contents;
  buffers that escape into outputs are **not** reused.
- The representation lane still needs Q31 int buffers (q_l/q_r, maxscale,
  integer raw sums for decisions). FPE may **reuse `PsyScratch`** for those
  (it already holds them) and add float buffers in `FpeScratch`, or hold both
  in `FpeScratch`. Recommended: `FpeScratch` wraps the needed Q31 repr buffers
  plus float value buffers, so the Q31 `PsyScratch` is untouched. Decision
  finalized at M2 (task FPE-2.2).
- **No per-frame heap allocation in the kernels** beyond the existing output
  `Vec` construction (which the Q31 engine also does). No `Box<dyn>`, no
  generics over float type (f64 concrete), no global caches.

## 6. Data flow inside FPE (per pair)

```
ChannelInput (f64 spectrum, seq, grouping, is_lfe)
  │
  ├─ lowpass zero + zeroSpec                      [float, same as Q31]
  ├─ REPRESENTATION LANE (integer-exact, reused helpers):
  │    map_spectrum_to_q31 → max_scale_spec → integer raw sums
  │    → max_nrg_for_shift → rescale_shifts → (mdctScale, finalShift, maxScale)
  │
  ├─ VALUE LANE (f64, NEW; FPE fraction domain v_f = v_st/2^31, 05 §1):
  │    E_real[b] = Σ spec_f64²  (physical, per window)
  │    E_f[b] = E_real × 2^(−2m')                   [m' post-finalShift; = E_st/2^31]
  │    T_f = E_f × C_RATIO_f → clip → spread → PCM floor → pre-echo(float history)
  │    spread_energy = copy(E_f) → spread(sprEn)
  │
  ├─ TNS (reused): detect on repr-lane Q31 spectrum → sync → encode on f64
  │    spectrum → VALUE-LANE energy recompute (long/short) → overflow rescale
  │    (repr-lane shiftBits/tnsSpecShift decisions integer-exact)
  │
  ├─ MS energies (f64 fraction, /4 convention) + stale-on-short Ld history
  │
  ├─ GROUPING (short): quantize per-window values → stored ints →
  │    integer nrgAddSaturate regroup + offsets + maxSfb + spectrum interleave
  │
  └─ BOUNDARY (output.rs): stored_i32 = round(v_f·2^31) (round, clamp),
       Ld vectors (log2(v_f)/64 + C guard/floor conventions),
       assemble ChannelOutput/GroupedOutput/MsOutput (shared types)
```

> **M2 pass-through (P2):** in M2 the VALUE LANE delegates to the Q31 kernels
> and the BOUNDARY is inactive — Q31 stored ints / f64 Ld pass through
> unchanged (no fraction conversion, no rounding). The fraction value lane and
> boundary quantization activate per-kernel in M3+ (`05` §7, `09` M2,
> ADR-0004).

The representation lane and value lane run on the **same** spectrum and the
**same** shift decisions; the value lane never feeds back into representation
decisions (one-way dependency), which is what keeps `mdctScale`/`finalShift`
bit-identical to the Q31 engine (ADR-0005).

## 7. Invariants and defensive checks (summary; full in 05 §8)

- All float values finite (no NaN/Inf) — checked at the boundary in debug,
  clamped/guarded in release; a non-finite value is a loud error, never
  silently propagated.
- Energies ≥ 0; thresholds ≥ 0; stored ints within i32 range (saturating
  quantization).
- Ld sentinel `−1.0` for non-positive energy; threshold-Ld floor −33/64.
- Grouped stride layout contract preserved (`psy.rs:216-224`).
- D-022 preconditions (LFE Long, CPE seq agreement) asserted identically.

## 8. What FPE deliberately does NOT abstract

- No `trait PsychoacousticEngine` (two impls, enum dispatch suffices).
- No generic `Float`/`Fixed` numeric tower (f64 concrete; Q31 helpers reused).
- No plugin/registry/dynamic loading.
- No shared "kernel framework" between Q31 and FPE (they share **data types
  and repr-lane helpers**, not arithmetic code).
- No change to downstream input structs (`ScfChannelInput`, `QcChannelInput`,
  `S2ChannelInput`, `PeChannelInput`).

## 9. Diagram: engine separation

```
                         StreamEncoder::frontend
                                  │
                    match self.psy_engine ( PsyEngineKind )
                       ┌──────────┴──────────┐
                       ▼                      ▼
              psy::process_pair_reuse   fpe::process_pair_reuse
              (Q31 kernels, UNCHANGED)  (f64 kernels, NEW)
                       │                      │
                       └──────────┬──────────┘
                                  ▼
                 (ChannelOutput, Option<ChannelOutput>, Option<MsOutput>)
                                  │  shared contract
                                  ▼
                    LiveChannel (Q31) → IS/MS → PE → S2 → S4 → S6 → S8
                                  (ALL UNCHANGED)
```