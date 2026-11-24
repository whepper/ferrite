# FPE M2 — Orchestration Stage Correspondence

Independent-review reference for the explicit FPE orchestration mirror.
Authority: `docs/fpe/04` §6, `09` M2 (FPE-2.2), `03` §2/§3; ADR-0001/0003/0005.
Working tree at HEAD `b188415e3c16dbf00706984858c6a568c9b705ff` (+ uncommitted
M1 seam + M2).

- **FPE mirror:** `rust/src/aac_enc/fpe/orchestrate.rs::process_pair`
  (bundled: `source/fpe/orchestrate.rs`).
- **Q31 original:** `rust/src/aac_enc/psy.rs::process_pair_reuse`
  (bundled verbatim: `q31-reference/psy-orchestration-and-helpers.rs`, and the
  complete file `source/modified-rust/psy.rs`).
- **Upstream:** `FDKaacEnc_psyMain`, `cpp/libAACenc/src/psy_main.cpp:407-1298`
  (FDK-AAC 3.0.0; tree `3878c45a:cpp`, recoverable via `git show`).

## 1. What M2 changed vs M1

M1 delegated the **whole** per-pair operation to `psy::process_pair_reuse`
operating on a wrapped `PsyChannel`/`PsyScratch`. M2 replaces that with an
**explicit FPE processing sequence** that owns the ordering, the conditional
behaviour, the element-global (joint-over-the-pair) representation decisions,
the channel-pair semantics, and the scratch reset discipline, while calling the
**existing Q31 kernels for every value calculation**. FPE state
(`FpeChannel`) and scratch (`FpeScratch`) are independently owned. No
floating-point psychoacoustic arithmetic, fraction conversion, rounding,
normalization, or rescaling is introduced (P2 pass-through; risk R-19). The
result is bit-identical to Q31 on the complete observable output **and** all
frame-surviving persistent state, after every frame of multi-frame sequences.

The only Q31-side edits are **five additive `pub(crate)` visibility bumps**
(`fn` → `pub(crate) fn`), sanctioned by `docs/fpe/04` §3 and `02` §4:
`lowpass_and_zero_check`, `tns_encode_channel`, `overflow_rescale`,
`advance_thresholds`, `finish_channel`. No behaviour, signature, or layout
change; `process_pair_reuse` and all helper bodies are byte-identical to HEAD.

## 2. Stage-by-stage correspondence

Line ranges are the Q31 `psy.rs` working-tree lines. "Q31 kernel" is the
value-processing code the FPE stage calls (all read-only / reused).

| # | FPE stage | Q31 `psy.rs` | Upstream `psy_main.cpp` | Q31 kernel(s) called by FPE |
|---|---|---|---|---|
| 0 | D-022 entry preconditions (release-active `assert!`) | 521-546 | caller contract | identical `assert!`/`assert_eq!` re-expressed in FPE |
| 1 | Per-channel long/short config selection (conf/tconf, n_win, wlen, active, lowpass; LFE-aware) | 548-626 | 552-629 | `SfbConfig` accessors (`sfb_active[_lfe]`, `lowpass_line[_lfe]`) |
| 2 | Lowpass zeroing + zeroSpec + initial exponents (12/9) | 628-642 | 584-614 | `psy::lowpass_and_zero_check`; `MDCT_SCALE_LONG/SHORT` |
| 3 | Integer lane: Q31 map of left spectrum | 644-648 | (stored ints) | `energy::map_spectrum_to_q31_into` |
| 4 | Rescale: per-band headroom + joint `min_spec_shift` (both channels) | 650-685 | 631-668 | `energy::max_scale_spec_into`, `map_spectrum_to_q31_into` (right) |
| 5 | Rescale: raw energies + joint `max_nrg` | 687-733 | `band_nrg.cpp:140-184` | `energy::check_band_energy_into`, `energy::max_nrg_for_shift` |
| 6 | `zero_elem` shortcut; else `rescale_shifts` → per-band re-shift + `init_threshold`; representation shift of Q/maxScale/mdctScale | 734-811 | 669-749 | `energy::rescale_shifts`, `energy::rescale_ld_into`, `q31::shift_int`, `thresholds::init_threshold` |
| 7 | TNS: detect per window (Q31 spectrum), pair sync ch1←ch0, element active-OR | 813-873 | 766-816 | `tns::detect`, `tns::synchronize_pair` |
| 8 | TNS: pre-encode scaling + encode (f64 spectrum) + energy recompute, per channel | 874-925 | 818-891 | `psy::tns_encode_channel` (→ `tns::encode`, `energy::band_energy_long_into`/`band_energy_short_into`, `map_spectrum_to_q31_into`, `max_scale_spec_into`) |
| 9 | TNS inactive: `clear_runtime`; element-global overflow rescale after all channels | 926-963 | 893-919 | `TnsChannel::clear_runtime`, `psy::overflow_rescale` |
| 10 | Threshold advance per channel: clip → spreading → PCM floor → pre-echo (STOP-before/START-after) → spread-energy copy+spread | 965-1006 | 922-1019 | `psy::advance_thresholds` (→ `thresholds::clip_cap`/`spreading_max`/`apply_pcm_floor`, `PreEchoState::control`/`reset_for_transition`/`set_enabled`) |
| 11 | MS energies (stereo): re-map Q at final exponents, `calc_ms_energies` per window, `calc_ld = !short`, write-on-long / stale-on-short `ms_ld` history, readout | 1008-1057 | 1021-1037 | `energy::map_spectrum_to_q31_into`, `energy::calc_ms_energies`; FPE-owned `ms_ld` |
| 12 | Grouping + output assembly per channel (short regroup / long maxSfb, Ld vectors, nm1 readout); `nrg_scaling` assignment | 1059-1129 | 1039-1130 | `output::assemble_channel` → `psy::finish_channel` (→ `psy::group_short_channel`/`grouped_ld`/`grouped_ld_clamped`, `q31::calc_ld_data`) |

## 3. Element-global (joint-over-the-pair) decisions owned by FPE

These are the channel-pair semantics the mirror re-expresses explicitly
(`docs/fpe/03` §4.12); they are **not** delegated:

- `min_spec_shift` — minimum headroom over **both** channels' windows (stage 4).
- `max_nrg` — maximum over both channels (stage 5), feeds `rescale_shifts`.
- `zero_elem` — element-global all-zero shortcut (`zero_l && (!stereo || zero_r)`);
  blanks maxScale and sets mdctScale 0 only when the whole element is silent (stage 6).
- `final_shift` — from `energy::rescale_shifts(max_nrg, min_spec_shift, short_elem, m_l)`;
  applied to both channels' Q lane, maxScale, and mdctScale (stage 6).
- `tns_active[8]` — OR of left/right TNS active flags over the element (stage 7).
- `tns_spec_shift` — element-global max of the per-channel long `shiftBits`,
  final **after both channels encode**, then applied via `overflow_rescale` (stages 8-9).
- `short_elem = short_l` — C `isShortWindow[0]` element-global flag for nrgShift.

## 4. State transitions / resets preserved (FPE-owned instances)

- Pre-echo: STOP-reset-**before** / START-reset-**after** control, memorize-only
  when disabled, `set_enabled(true)` after each window — inside
  `psy::advance_thresholds` acting on `FpeChannel.preecho` (stage 10).
- TNS: `clear_runtime` when globally inactive/LFE (stage 9); detect/sync/encode
  write `FpeChannel.tns_state`/`tns_info` (stages 7-8).
- MS-Ld: `calc_ld = (rseq != Short)`; long writes fresh `mid_ld`→ch0 / `side_ld`→ch1
  into `FpeChannel.ms_ld`; short leaves history **stale** and reads it out (stage 11).
- Init: `FpeChannel::new` mirrors `PsyChannel::new` (pre-echo seeded with long PCM
  floors, `prev_scale = 8`, enabled; TNS zeroed; `ms_ld` zeros).

## 5. Scratch reset discipline (FPE-owned `FpeScratch`)

`FpeScratch` mirrors the `PsyScratch` field layout and the M5.20
reset-to-exact-pre-first-read discipline, re-expressed in the mirror:
`clear`+`resize(active,0)` for `e_*`/`t_*`/`spread_*`/`ms_zeros_*`;
`clear`+`resize(active,-1.0)` for `eld_*` (placeholders read on the all-zero
path); `resize(n_win, Vec::new())` for the per-window outer vecs; `q_l`/`q_r`/
`ql_full`/`qr_full` fully overwritten by the mapper; `qwin`/`tns_pre` fully
overwritten per window inside `tns_encode_channel`. Buffers that escape into
outputs are not reused. No per-frame heap allocation beyond the existing
output-`Vec` pattern.

## 6. Representation lane (ADR-0005) — observable and exact

`mdctScale`, `finalShift`, `maxScale`, `tnsSpecShift` (and `nrgScaling`,
`shiftBits`) are produced by the reused integer-exact Q31 helpers and pass
through unchanged; the value lane never feeds back into them (one-way
dependency). The differential harness compares them exactly every frame; the
M2 guard `output::guard_channel_output` additionally `debug_assert`s
`finalShift >= 0`, the grouped stride layout, the threshold-Ld floor (−33/64),
finite Ld, and non-negative energies/thresholds.

## 7. Output assembly pass-through (M2) and M3+ boundary

`output::assemble_channel` forwards its arguments unchanged to
`psy::finish_channel` (the Q31 assembler), then runs the M2 invariant guards.
There is **no** boundary quantization in M2. The `docs/fpe/05` §7 boundary
(`stored_i32 = clamp(round(v_f·2^31))`) and FPE-owned assembly activate
per-kernel only from M3, when a specific value kernel is replaced by float
arithmetic. Grouped MS-Ld stays exact `q31::calc_ld_data` throughout.

## 8. Verification

Bit-identity of this mirror against `psy::process_pair_reuse` is gated by the
in-binary Q31↔FPE differential harness (`source/fpe/tests/`): per-frame
complete-output + mutated-spectrum + persistent-state comparison over
multi-frame sequences (long/short, START/STOP, pre-echo enable/reset,
stale-on-short MS-Ld, mono/stereo, 44.1/48 kHz, silence/transients/sustained/
noise, real music/speech, committed MDCT vectors, M1 edge fixtures), plus the
M2 stage/invariant tests (`tests/m2_stages.rs`). See `validation/` logs.
