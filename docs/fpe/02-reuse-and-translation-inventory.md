# 02 — Reuse and Translation Inventory

Date: 2026-11-16. Classification per assignment §6:

- **A — Reuse unchanged**: no modification; FPE calls/consumes as-is.
- **B — Reuse with small interface adaptation**: thin wrapper, conversion, or
  additive seam; no algorithm change.
- **C — Translate fixed-point → floating-point**: new FPE implementation of
  the value arithmetic, faithful to the reference algorithm.
- **D — Replace only if strictly necessary**: none identified for Stage 1.
  Any D item requires an ADR and owner approval before implementation.

Ownership rule: a component belongs to FPE only if it is psychoacoustic model
value arithmetic. TNS, block switching, MDCT, SFB config, and all downstream
stages remain separate subsystems even where they interact with psy.

## 1. Inventory table

| # | Component | Ferrite location | Upstream counterpart | Class | Rationale / adaptation |
|---|---|---|---|---|---|
| 1 | MDCT kernel | `common/mdct.rs` | `cpp/libFDK/src/mdct.cpp:135 mdct_block` | **A** | Already a validated line-for-line float port; FPE consumes its output via `ChannelTransform`. |
| 2 | Transform staging (ring, windows, overlap) | `transform.rs:175-260` | `psy_main.cpp:552-629`, `transform.cpp:132-156` | **A** | f32, validated parity; produces the f64 spectrum FPE receives. |
| 3 | Window tables (KBD/SINE) | `common/tables/window_tables.rs` | `FDK_tools_rom.cpp` | **A** | Immutable data. |
| 4 | Block switching (attack detect, seq tables, sync) | `block_switch.rs` | `block_switch.cpp:121-560` | **A** | Already float-native f32; decisions upstream of psy; FPE receives seq/grouping. |
| 5 | SFB width tables | `sfb_tables.rs` (SFB_TABLES) | `aacEnc_rom.cpp` | **A** | Verbatim ROM extract (`@generated`). |
| 6 | SFB config selection (offsets, active, lowpass, clip, pcm, bark) | `sfb.rs:236-374` | `psy_configuration.cpp:191-259, 534-627` | **A** | Dual int/f64 already; integer constructions exact. FPE reads f64 accessors. |
| 7 | Spreading factor tables | `sfb_tables.rs` (SPREAD_LONG/SHORT) | `psy_configuration.cpp:354-405 initSpreading` output | **B** | Consume Q31 ints → f64 (`/2^31`). **Do not recompute in float** (C fixp_atan/fPow not reproducible; worst 13% deviation; `sfb.rs:111-123`). |
| 8 | minSnr Ld data | `cfg_*.txt` MINSNR via `psy_config.rs` | `psy_configuration.cpp:421-532 initMinSnr` output | **B** | Q31 Ld ints → f64 Ld (`/2^31`) where FPE needs them; grouping passes them through unchanged (they are Ld-domain data, not value arithmetic). |
| 9 | TNS config | `tns.rs:160-180 TnsConfig` | `aacenc_tns.cpp` init | **A** | From cfg snapshots / `TnsConfig::new`. |
| 10 | TNS detect (autocorr + Schur + activation) | `tns.rs` detect | `aacenc_tns.cpp TnsDetect`, `FDK_lpc.cpp` | **A** | Separate subsystem; exact integer autocorr is validated bit-exact (RXX); float Schur already. FPE calls it on the Q31 representation-lane spectrum exactly as the Q31 engine does. |
| 11 | TNS pair sync | `tns.rs synchronize_pair` | `aacenc_tns.cpp TnsSync` | **A** | Unchanged. |
| 12 | TNS encode (LPC filter on spectrum) | `tns.rs encode` | `aacenc_tns.cpp TnsEncode` | **A** | Filters the f64 spectrum in place; FPE calls at the same orchestration points. |
| 13 | TNS state types | `tns.rs TnsChannel` | `TNS_DATA`/`tnsInfo` | **A** | FPE reuses `TnsChannel` for runtime/info. |
| 14 | Lowpass zeroing + zeroSpec check | `psy.rs:398-418` | `psy_main.cpp:584-614` | **A/B** | Pure float logic; FPE re-implements identically (or shares a helper if extracted without touching Q31 behaviour — prefer copy to avoid Q31 edits). |
| 15 | Q31 spectrum mapping | `energy.rs:395-410 map_spectrum_to_q31_into` | C stored ints (implicit) | **B** | Retained in FPE as the **representation lane** input (headroom decisions + output quantization uses the same round-to-nearest convention). Not model arithmetic. |
| 16 | Per-band headroom `maxScaleSpec` | `energy.rs:130-148` | `band_nrg.cpp:111-132 CalcSfbMaxScaleSpec` | **B** | Retained integer-exact (representation decision; clz semantics). FPE reuses the function. |
| 17 | Rescale shift decisions (`minSpecShift`, `nrgShift`, `finalShift`) | `energy.rs:190-209, 440-460` | `psy_main.cpp:631-687`, `band_nrg.cpp:177-183` | **B** | Retained integer-exact so `mdctScale`/`finalShift` trajectory matches Q31/C bit-exactly (ADR-0005). Needs integer raw-energy argmax → FPE computes the integer raw sums for the decision (see #18). |
| 18 | Raw band energy sums (long, rescale stage) | `energy.rs:161-179 check_band_energy_into` | `band_nrg.cpp:140-184 CheckBandEnergyOptim` | **C** | FPE computes true float sums for **values**; the integer sum is still produced for the `maxNrg` shift decision (representation lane). Value lane = float translation. |
| 19 | TNS long energy recompute + overflow shiftBits | `energy.rs:249-313 band_energy_long_into` | `band_nrg.cpp:192-256 CalcBandEnergyOptimLong` | **C** | Float energies/Ld for values; integer `shiftBits` decision retained (representation). |
| 20 | TNS short energy recompute | `energy.rs:215-237 band_energy_short_into` | `band_nrg.cpp:264-288 CalcBandEnergyOptimShort` | **C** | Float translation (no Ld array on short path — C quirk preserved). |
| 21 | Threshold init (C_RATIO multiply) | `thresholds.rs:130-132` | `psy_main.cpp:702-703` | **C** | `T_f = E_f·C_RATIO_f` (fraction domain, `05` §6.3). |
| 22 | Clip cap | `thresholds.rs:137-145` | `psy_main.cpp:926-939` | **C** | Float cap value with identical shift/headroom branch logic. |
| 23 | Spreading max-envelope (threshold + sprEn) | `thresholds.rs:149-161` | `spreading.cpp:105-125 SpreadingMax` | **C** | Two-pass max-envelope with float factors. |
| 24 | PCM quantization floor | `thresholds.rs:167-180` | `psy_main.cpp:955-971` | **C** | Float `max(T/16, pcm×2^∓shift)`; accumulating `energyShift` logic identical. |
| 25 | Pre-echo control (temporal masking) | `thresholds.rs:184-267 PreEchoState` | `pre_echo_control.cpp:106-176` | **C** | Float history + identical scale-aligned clamp/floor/memorize and STOP/START resets. |
| 26 | Spread-energy copy + spreading | `psy.rs:1274-1275` | `psy_main.cpp:1009-1017` | **C** | Copy float energies, spread with sprEn factors. |
| 27 | MS band energies | `energy.rs:321-389 calc_ms_energies` | `band_nrg.cpp:296-361 CalcBandNrgMSOpt` | **C** | Float mid/side sums with the same net /4 convention and scale decisions; stale-on-short Ld preserved. |
| 28 | Short-block grouping value sums | `psy.rs:1285-1414 group_short_channel` | `grp_data.cpp:118-264 groupShortData` | **B/C** | Offsets/maxSfb/spectrum interleave = data movement (**B**, reuse logic). Saturating window sums: FPE may sum float per-window stored values then quantize+saturate, or reuse integer `nrgAddSaturate` on quantized stored ints (**recommended**, keeps grouped contract exact; the sum is representation-level, see `05` §6.7). |
| 29 | Output Ld vectors (grouped/long, floor −33/64) | `psy.rs:1518-1537`, `sfb.rs:184-190` | `psy_main.cpp:1058-1076, 1120-1128 LdDataVector` | **C** | Float `log2/64` with C guard/floor conventions (already the Q31 engine's float-lane convention). |
| 30 | MS-Ld history staleness | `psy.rs:160, 1019-1034` | `psy_main.cpp:1033-1035, 1078-1084` | **C** | FPE keeps `ms_ld` f64 history with identical write-on-long / stale-on-short / fresh-grouped-short semantics. |
| 31 | Output assembly (maxSfbPerGroup, nm1 dump, flags) | `psy.rs:1420-1513 finish_channel` | `psy_main.cpp:1089-1101, 1251-1295` | **B** | Same contract; FPE fills shared output types. |
| 32 | Psy output types | `psy.rs:194-278, 384-393` (`ChannelOutput`, `GroupedOutput`, `MsOutput`) | `PSY_OUT_CHANNEL`/`PSY_OUT_ELEMENT` | **A** | FPE constructs these existing types; no Q31 modification. |
| 33 | Psy tables | `psy.rs:134-148 PsyTables` | `PSY_CONFIGURATION` | **A/B** | FPE reuses `PsyTables` (SfbConfig f64 accessors + TnsConfig + minSnr). |
| 34 | Scratch buffer pattern | `psy.rs:438-462 PsyScratch` | C scratch allocs | **B** | FPE defines `FpeScratch` with the same reset discipline; encoder-owned. |
| 35 | LiveChannel construction | `stream_driver.rs:872-966` | `BuildInterface` | **A** | Unchanged; consumes FPE output identically. |
| 36 | IS/MS stereo decisions | `intensity.rs`, MS mirror in `stream_driver.rs` | `intensity.cpp`, `ms_stereo.cpp` | **A** | Downstream of psy; Q31; unchanged. |
| 37 | PE / form factor (S1) | `pe.rs` | `line_pe.cpp`, `qc_main.cpp:435-452` | **A** | Downstream; unchanged. |
| 38 | VBR threshold adaptation (S2) | `threshold_vbr.rs`, `pe.rs` ATS | `adj_thr.cpp` | **A** | Downstream rate-control family; unchanged (non-goal). |
| 39 | Scalefactor estimation (S4) | `scf.rs` | `sf_estim.cpp` | **A** | Downstream; unchanged. |
| 40 | Quantization | `quant.rs`, `quant_neon.rs`, `quant_avx2.rs` | `quantize.cpp` | **A** | Downstream; unchanged. |
| 41 | Bit counting (S5) | `bitcount.rs` | `dyn_bits.cpp` | **A** | Downstream; unchanged. |
| 42 | Rate control / QC (S6) | `rate.rs` | `qc_main.cpp`, `adj_thr.cpp` | **A** | Downstream; unchanged (non-goal). |
| 43 | Bitstream / ADTS (S8) | `bitenc.rs`, `frame.rs`, `adts.rs` | `bitenc.cpp`, `tpenc_adts.cpp` | **A** | Downstream; unchanged. |
| 44 | Encoder lifecycle / API | `encoder.rs`, `pcm.rs`, `config.rs` | `aacenc_lib.cpp` | **B** | Additive selection knob only (`06`). |
| 45 | Tonality estimation | **absent** (PNS off) | `tonality.cpp:121-219`, `chaosmeasure.cpp` | **A (absent)** | Upstream `CalculateFullTonality` is `usePns`-gated (`tonality.cpp:129`); Ferrite PNS off → no tonality. FPE adds none. The S2 "chaos measure" (`adj_thr.cpp:1054`) is a different, downstream quantity already in `threshold_vbr.rs`/`pe.rs` — not FPE scope. |
| 46 | Absolute hearing threshold | **no ATH table in FDK LC** | `psy_configuration.cpp:325-334 InitMinPCMResolution` | **n/a** | The absolute floor is `sfbPcmQuantThreshold` (PCM quantization noise, −96 dB / 1-bit). FPE translates the PCM floor (#24); it does not invent an ATH model. |
| 47 | PNS / noise detection | **absent** (PNS off) | `aacenc_pns.cpp`, `noisedet.cpp`, `pnsparam.cpp` | **A (absent)** | Out of scope; FPE adds none. |

## 2. Summary counts

- **A (reuse unchanged)**: 30 items (incl. 3 "absent, stay absent").
- **B (small adaptation)**: 9 items — mostly Q31→f64 data conversion and
  retained representation-lane helpers.
- **C (translate)**: 10 items — the psychoacoustic value kernels (#18, 19, 20,
  21, 22, 23, 24, 25, 26, 27, 29, 30 — the core FPE work).
- **D (replace)**: 0. No component requires replacement; everything is either
  reusable or translatable.

## 3. What FPE must NOT duplicate

- No second MDCT, window table set, SFB table set, or band-offset logic.
- No second TNS implementation (detect/sync/encode reused).
- No second block-switch or window-sequence decision path.
- No second quantizer, scalefactor coder, rate controller, bit counter, or
  bitstream writer.
- No re-derivation of spreading factors or minSnr in float (Stage 2 only).
- No tonality/PNS/ATH additions.

## 4. Shared vs FPE-private code

- **Shared (existing, untouched)**: output types, `PsyTables`, `SfbConfig`,
  `TnsConfig`/`TnsChannel`, `sfb_tables`, `psy_config` snapshots,
  representation-lane helpers (`map_spectrum_to_q31_into`,
  `max_scale_spec_into`, `rescale_shifts`, integer raw-sum helpers used for
  decisions), `q31.rs` primitives (used by representation lane and boundary
  quantization/Ld).
- **FPE-private (new)**: float value kernels, `FpePreEchoState`, `FpeChannel`,
  `FpeScratch`, float orchestration (`fpe::process_pair_reuse`), float
  constants derived from the Q31 tuning constants.
- **No Q31 refactor**: FPE imports existing items; it does not move, rename,
  or rewrite Q31 modules. If a shared helper proves inaccessible (privacy),
  the fix is an additive `pub(crate)` visibility change *only*, recorded in
  the task spec — never a behaviour change.