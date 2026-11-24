# 06 — Encoder Integration Design

Date: 2026-11-16. Constraint: the same downstream components serve both
engines; no second quantizer/scalefactor-coder/rate-controller/bitstream;
VBR-1..6 behaviour unchanged; AAC-LC format and public interface unchanged
except an additive selection knob; Q31 remains default.

## 1. Selection mechanism

- Internal enum `PsyEngineKind { Q31, Fpe }` (Copy, Default = Q31), stored on
  `StreamEncoder` (new field, additive).
- Chosen in `StreamEncoder::with_config` (`stream_driver.rs:646-716`) from an
  additive `EncoderConfig` accessor.
- **Single dispatch point**: `StreamEncoder::frontend`
  (`stream_driver.rs:847/857`). Today:
  ```
  let (ol, or_, m) = process_pair_reuse(&mut pl[0], Some(&mut pr[0]), lin, Some(rin), &self.tables, &mut self.psy_ws);
  ```
  becomes:
  ```
  match self.psy_engine {
    PsyEngineKind::Q31 => psy::process_pair_reuse(..., &mut self.psy_ws),
    PsyEngineKind::Fpe => fpe::process_pair_reuse(&mut self.fpe_ch[0], Some(&mut self.fpe_ch[1]), lin, Some(rin), &self.tables, &mut self.fpe_ws),
  }
  ```
  (mono arm analogous). Both return the identical tuple type; the rest of
  `frontend` and `step_frame` is untouched.
- No Cargo feature (none exist; runtime enum enables same-binary three-way
  differential testing). No dyn dispatch, no trait.

## 2. Configuration changes (additive only)

- `EncoderConfig` gains a private `psy_engine: PsyEngineKind` field (default
  Q31) and a builder `with_psy_engine(kind)` (additive, mirrors
  `with_vbr_mode`, `config.rs:525-527`). Existing constructors
  (`new`, `new_with_mode`, `stereo_48k_adts`) default to Q31 — **no behaviour
  change for existing callers**.
- Public exposure timing: the knob is added internally at M2 (test-selectable).
  Public API exposure (documented, stable) happens at M8 **after** Stage-1
  validation; until then it may be `#[doc(hidden)]`/unstable. Making FPE the
  **default** is explicitly out of Stage 1 (charter §4).
- `PsyEngineKind` is exported from `config` (or `encoder`) as a small public
  enum once stabilized. No change to `VbrMode`, bandwidth, bitrate, transport,
  frame length, or the validated matrix (mono/stereo × 44.1/48k × ADTS).
- Config snapshots (`cfg_*.txt`) are shared by both engines (FPE reads the
  same `PsyTables`); no new data files, no manifest/dependency changes.

## 3. Internal data structures

- `StreamEncoder` additive fields: `psy_engine: PsyEngineKind`,
  `fpe_ch: Vec<FpeChannel>` (allocated only when FPE selected),
  `fpe_ws: FpeScratch`. Q31 fields (`psy_ch`, `psy_ws`) remain (unused when
  FPE selected, but kept so switching is a config choice, not a rebuild).
  *Memory note*: allocating both state sets doubles psy state (~few KB/ch);
  acceptable. If owner prefers, allocate only the selected engine's state
  (M2 decision, task FPE-2.3); default recommendation is allocate-selected-only
  to avoid unused memory, with the enum still allowing re-selection at
  construction.
- `FpeChannel`, `FpeScratch`, `FpePreEchoState` per `04` §4/§5.
- Shared output types unchanged (`ChannelOutput`/`GroupedOutput`/`MsOutput`).

## 4. State initialization and reset

- Init at `with_config` mirrors `PsyChannel::new` (`psy.rs:166-173`): pre-echo
  prev = long PCM floors, prev_scale = 8, enabled; TNS state/info zeroed;
  ms_ld zeros.
  - **M2 (pass-through):** `FpeChannel` holds the **Q31 state types**
    (`PreEchoState` stored-int history, `TnsChannel`) and delegates control to
    them, so history is bit-identical to Q31. No fraction conversion.
  - **M6+:** the float `FpePreEchoState` (fraction history `prev_f =
    pcm_f_long`, `pcm_quant()` accessor, `05` §6.5) replaces the delegated
    pre-echo control. TNS state stays the reused `TnsChannel` throughout.
- **No reset API** (encoder is single-use, `01` §4). Flush drains zero-padded
  frames through the same path with live decaying histories — FPE inherits
  this exactly (no special flush handling).
- In-frame semantic resets (pre-echo STOP/START, TNS runtime clear when
  inactive) identical to Q31 (`05` §6.5).

## 5. Frame processing sequence

Unchanged from `01` §3. FPE occupies exactly the `process_pair_reuse` slot:
block switch → MDCT → **[Q31 psy | FPE]** → LiveChannel → IS/MS → PE → S2 →
S4 → S6 → S8. Long/short transitions, channel-pair sync, and grouping all
behave identically because FPE preserves the orchestration order, the
element-global decisions, and the output contract (`04` §6, `05` §6).

- **M2 pass-through (P2):** FPE delegates the value lane to the Q31 kernels
  and passes their stored-int / f64-Ld results through **unchanged** — no
  float conversion, rounding, normalization, or rescaling. The `05` §7
  boundary quantization activates per-kernel only in M3+. The M2 gate also
  requires **bit-identical persistent state after every frame** of multi-frame
  sequences (`PreEchoState`, TNS state, MS-Ld history; test-only helpers, no
  new production API) (`09` M2, `07` §3.1, ADR-0004).
- **Long/short**: FPE selects long/short config per channel exactly as
  `psy.rs:534-603`; initial exponents 12/9; grouping from block switch.
- **Channel pairs**: joint minSpecShift/finalShift/tnsSpecShift, TNS sync
  ch1←ch0, MS energies, stale-on-short MS-Ld — all preserved.
- **LFE**: structural early-outs (skip TNS/pre-echo) + D-022 asserts inherited.

## 6. Error handling

- FPE numerical-invariant violations (`05` §8) surface as a frame-level
  finding through the existing skip-with-note path (`frontend` returns
  `Err(note)` → `step_frame` records note, emits `None`,
  `stream_driver.rs:1021-1027`). No new panic in release for numerical noise;
  debug asserts catch bugs during development.
- Structural preconditions (D-022) use the same release-active `assert!` as
  Q31 (these are caller-contract violations, not numerical noise).
- The public `EncodeError`/`EncodeFailure` taxonomy (`encoder.rs:111-160`) is
  **unchanged**: FPE does not add error variants. A skipped FPE frame maps to
  `EncodeFailure::SkippedFrame{notes}` exactly like a Q31 structural skip.
- Poisoning/lifecycle semantics unchanged.

## 7. Interaction with quantization and rate control

- FPE output → `LiveChannel` (Q31) → S2/S4/S6 exactly as Q31. Thresholds reach
  quantization **only via S4 scalefactors** (quantize_spectrum takes no
  thresholds; `01` §3.3, subagent §3.6). PE reaches rate control as
  `no_red_pe`. Because FPE preserves the stored-domain contract within the
  established envelopes, downstream behaviour is preserved within those
  envelopes; no downstream code changes.
- VBR-1..6: unchanged. The ATS/QC/distribute state and the VBR-5 bitres-FULL
  pinning are downstream and untouched. FPE does not alter `vbr_qual_factor`,
  `pe_offset`, minSnr adaptation, or chaos evolution.

## 8. Public API implications (summary)

| Surface | Change |
|---|---|
| `Encoder::new/push/flush/is_finished/vbr_mode` | unchanged |
| `EncoderConfig` existing constructors | unchanged (default Q31) |
| `EncoderConfig::with_psy_engine` | **additive** builder (M2 internal, M8 public) |
| `PsyEngineKind` enum | **additive** public type (M8) |
| `AdtsFrame`, `EncodeError`, `EncodeFailure` | unchanged |
| AAC-LC bitstream format / ADTS | unchanged |
| Crate features / dependencies / manifest | unchanged |

## 9. Integration tests (see 07 for full strategy)

- Default-output regression: Q31 selected → byte-identical to pre-project
  baseline hashes (production_baseline test, `production_baseline.rs`).
- FPE-selected smoke: encode the 19 fixtures + 4 WAVs with FPE, decode via
  `AAC_DEC_BIN`, assert valid ADTS + decodability.
- Selection seam: construct with Q31 vs FPE, assert `psy_engine` field and
  that default is Q31; assert existing constructors unaffected.
- Three-way differential at psy boundary (`07`).