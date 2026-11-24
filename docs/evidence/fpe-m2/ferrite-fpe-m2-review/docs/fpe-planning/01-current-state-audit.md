# 01 — Current Ferrite Architecture Audit

Date: 2026-11-16. Audited at Ferrite HEAD `a2c1d53` (branch `main`, clean
worktree). All statements below are **verified** by direct source inspection
unless labelled *assumption*.

## 1. Repository shape

| Item | Verified fact |
|---|---|
| Repo | `github.com/whepper/ferrite`, working dir `fdk-aac/fdk-aac` |
| Crate | `rust/` → `ferrite-aac` v0.4.0, edition 2021, MSRV 1.99.1, `license = "FDK-AAC"`, `license-file = "LICENSE-FDK-AAC"` (`rust/Cargo.toml:1-14`) |
| Workspace | `rust/convert` (`aac-convert`, independently developed, MIT OR Apache-2.0) — not codec code |
| Features | **No `[features]` section exists** in `rust/Cargo.toml`; CI's `--no-default-features` build is currently a no-op distinction. There is no existing algorithm-selection mechanism. |
| Dependencies | `arrayvec`, `bitflags`, `itertools`, `num-complex`, `paste`, `zerocopy` (`rust/Cargo.toml:50-56`). No float-math or SIMD crates; SIMD is hand-written (`quant_neon.rs`, `quant_avx2.rs`). |
| CI | `.github/workflows/ci.yml`: ubuntu, Rust 1.99.1, `cargo build --locked` (default + no-default-features), self-contained **Tier-A** tests; 15 **Tier-B** tests excluded (need external C artifacts) |
| Reference tooling | `tools/enc-oracle/` (C++ oracle + probes + Python extractors), `tools/c-enc-bench/`, `tools/aac-enc-metrics/`, `rust/tools/*.sh` |
| Upstream C tree | Removed from repo at `4c69425`; recoverable at `3878c45a` (= `4c69425~1`). Pinned private-reference commit `f14ad40a574a4132df947d15e9fb14bee91cc8a5` (`docs/reference/fdk-aac-reference-revision.txt`). |
| Local reference checkout | `/private/var/folders/6h/…/T/opencode-fdk-ref/fdk-aac-reference` — **emptied** (dirs exist, files gone, `.git` incomplete). Build stamp in `~/f02f05-validation/scratch/refval-out/ref-build/reference-build-info.txt` confirms it was at the pinned revision. Must be re-provisioned (risk R-12). |

## 2. Public API and configuration

- `Encoder` (`encoder.rs:264`): owned, single-threaded, push/flush lifecycle
  with poisoning (`EncodeError`, `encoder.rs:111-138`). `Encoder::new`
  (`:283`) accepts only the **validated matrix**: mono/stereo × 44100/48000 Hz
  × ADTS, VBR-1..6. It delegates per-frame work to
  `StreamEncoder::step_frame` (`encoder.rs:389` → `stream_driver.rs:1000`).
- `EncoderConfig` (`config.rs:460`): sample rate, channels, frame length
  (1024), transport, `VbrMode` (`config.rs:199`). Default **VBR-6** (Ferrite
  extension: VBR-5 settings with 20 kHz bandwidth; `config.rs:109,196,210`).
- VBR profile table (`config.rs:339-396`): per-channel bitrates and
  bandwidths; VBR-5 bandwidth 19293 Hz; `channel_bitrate(is_mono)`
  (`config.rs:442`). VBR-5/6 share per-channel bitrates (96000 stereo /
  112000 mono).
- `PcmFeeder` (`pcm.rs`): i16 PCM buffering; flush law
  `ceil((S + 1600)/1024)` frames (verified against C, Phase 0).
- Config snapshots: `psy_config.rs:144 load_cfg` / `:221
  load_cfg_for_profile` load **18 committed `cfg_*.txt`** files via
  `include_str!` (9 per-channel bitrates × 2 rates). These are production
  data (shipped in the crate; `Cargo.toml:20-25`). They carry SFB offsets,
  active counts, lowpass, clip energy, PCM floors, spreading factors
  (MASKHI/MASKLO/MASKHIS/MASKLOS), and minSnr Ld (MINSNR), plus TNS config.
  Unsupported combinations **panic fail-closed** (`psy_config.rs:291-294`).

## 3. Per-frame data flow (production path)

`StreamEncoder::frontend` (`stream_driver.rs:795`) + `step_frame` (`:1000`).
The module header (`stream_driver.rs:1-64`) documents the stage order; the
production API wraps it (`encoder.rs`). Stage numbers S1..S8 match the oracle
dump blocks.

```
 fresh i16 PCM (1024/ch)
   │
   ▼
 [M2] BlockSwitch::process            block_switch.rs:341   (f32, per channel)
      synchronize_pair / mono         block_switch.rs:409/447
   │  seq, shape, grouping (u8,[u8;4])
   ▼
 [M3a] ChannelTransform::process      transform.rs:208      (f32 ring + MDCT)
   │  [f32;1024] → [f64;1024]  (stream_driver.rs:824-828)
   ▼
 [M3b] psy::process_pair_reuse        psy.rs:495            (HYBRID Q31 engine)
   │  ChannelOutput ×2, MsOutput      (mutates f64 spectra: TNS + scalings)
   ▼
 LiveChannel working sets             stream_driver.rs:872-966
   │  spec_q31 = map_spectrum_to_q31(spec_f64, mdct_scale)   (:929)
   │  e_stored/t_stored/spread: grouped-stride→flat (short)  (:887-921)
   │  e_ld: long=q31_of_ld_vec(float Ld) / short=ld_of(stored) (:937-947)
   │  t_ld = threshold_ld_of(t_stored)  (calc_ld_data + floor) (:950)
   ▼
 [S1pre] intensity_stereo + ms_stereo_mirror   intensity.rs / stream_driver
   │  Q31 integer decisions; PNS off (NO_NOISE_PNS)
   ▼
 [S1] form_factor + pe_calculation    pe.rs                 (Q31 Ld)
 [S2] adapt_thresholds_vbr            threshold_vbr.rs      (Q31, ATS state)
 [S4] estimate_scale_factors_channel  scf.rs (inv=2)        (Q31 spec + Ld)
 [S6] qc_main_frame (gain loop)       rate.rs               (Q31)
      └ S5 (calc_max_value_in_sfb + dyn_bit_count, bitcount.rs) runs INSIDE
        the gain loop; pass 0 is seeded from the S4 winning quantization
        (R1, HEAD a2c1d53). No standalone S5 stage remains in step_frame.
 [S8] build_raw_block + build_adts_frame  frame.rs/bitenc.rs/adts.rs
```

Exact production locations (verified): `StreamEncoder` fields
`stream_driver.rs:589-616`; `with_config` `:646-716`; `LiveChannel` struct
`:572-586`; `frontend` `:795-995`; `step_frame` `:1000-1655`. IS call `:1048`,
MS call `:1132`, PE `:1186-1211`, S2 `:1256-1301`, S4 `:1327-1367`, S6
`:1383-1438`, S8 `:1525-1654`.

### 3.1 Where psychoacoustic processing starts and ends

- **Starts**: `psy::process_pair_reuse` (`psy.rs:495`), called from
  `frontend` (`stream_driver.rs:847/857`) with `ChannelInput`
  (`psy.rs:356-366`): `&mut [f64;1024]` spectrum, post-sync `WindowSequence`,
  grouping `(u8,[u8;4])`, `is_lfe`.
- **Ends**: returned `(ChannelOutput, Option<ChannelOutput>, Option<MsOutput>)`
  (`psy.rs:247-278, 384-393`). The frontend then builds `LiveChannel`
  (Q31 working sets) — this conversion is **outside** psy but is the contract
  FPE must satisfy.
- Block switching and MDCT are **upstream of psy** in Ferrite (in C they are
  inside `FDKaacEnc_psyMain`; Ferrite factored them out — see `03` §2).

### 3.2Psy orchestration internals (verified sequence, `psy.rs`)

1. Preconditions: LFE-must-be-Long, CPE sequences must agree (`:507-532`,
   D-022 asserts active in release).
2. Per-channel config selection long/short (`:534-603`); LFE active/lowpass.
3. Lowpass zeroing + `zeroSpec` check (`:398-418, 614-618`).
4. Initial exponents: `MDCT_SCALE_LONG=12`, `MDCT_SCALE_SHORT=9`
   (`:129-131`) — measured constants, verified over 1000+ oracle frames.
5. **Integer lane**: `map_spectrum_to_q31_into` (`:634`, round-to-nearest —
   the documented *single approximation point*), `max_scale_spec_into`
   (`:644`), global `min_spec_shift` over both channels (`:642-671`).
6. Raw energies `check_band_energy_into` (`:697`), `max_nrg_for_shift`
   (`:704`), element-global `zero_elem` shortcut (`:723, 736-747`).
7. `rescale_shifts` → `(nrgShift, finalShift)` (`:749`); per-band re-shift +
   `init_threshold` (`:752-771`); representation shift of Q lane + maxScale +
   mdctScale (`:774-795`); float lane untouched (real values unchanged).
8. **TNS block** (`:799-949`, skipped for LFE elements): `tns::detect` per
   window on Q31 spectrum, `tns::synchronize_pair` (ch1←ch0), element-global
   active OR, `tns_encode_channel` (`:1121`: long pre-halving + threshold
   quartering, `tns::encode` on f64 spectrum, energy recompute
   long/short), element-global `overflow_rescale` (`:1206`).
9. **Threshold advance** `advance_thresholds` (`:1242`): clip cap →
   `spreading_max` (threshold factors) → `apply_pcm_floor` (accumulating
   `energyShift`) → pre-echo (STOP-reset-before / START-reset-after) →
   spread-energy copy + `spreading_max` (sprEn factors).
10. **MS energies** (stereo, `:994-1043`): re-map Q with final exponents,
    `calc_ms_energies` per window; `calc_ld = !short`; **stale-on-short Ld**
    semantics preserved via `PsyChannel.ms_ld` history.
11. **Grouping + outputs** `finish_channel` (`:1420`): short →
    `group_short_channel` (`:1285`, saturating regroup + offsets + maxSfb +
    spectrum interleave), fresh grouped MS Ld (`calc_ld_data`), grouped Ld
    via float `log2/64` (`grouped_ld :1518`), threshold Ld floor −33/64
    (`grouped_ld_clamped :1532`); long → maxSfb from top non-zero line with
    `min(5,active)` floor, threshold Ld.

### 3.3 How thresholds reach quantization

`LiveChannel` (`stream_driver.rs:872-966`) → `S2ChannelInput`
(`threshold_vbr.rs:244-254`: `energy_lin`, `energy_ld`, `weighted_energy_ld`,
`threshold_ld`, `form_factor_ld`, `enfac_ld`, `min_snr_ld`, `spread`, all
`&[i32]`) → S2 produces post-VBR `threshold_ld` → `ScfChannelInput`
(`scf.rs:278-284`: `spectrum: &mut [i32]`, `energy_ld`, `threshold_ld`,
`form_factor_ld`) → S4 scalefactors + `quant_spec` → `QcChannelInput`
(`rate.rs:1324-1362`) → S6 gain loop. PE (`pe.rs:340-341, 454, 558-559`)
consumes Q31 Ld vectors. **Everything downstream of psy is Q31 integer.**

## 4. State ownership

| State | Owner | Location | Lifetime |
|---|---|---|---|
| Block-switch history (seq, attack, energies, IIR) | `BlockSwitch` per channel | `block_switch.rs:276-290` | stream |
| MDCT ring + overlap slope + last shape | `ChannelTransform` per channel | `transform.rs:175-179` | stream |
| Pre-echo history (`prev` thresholds, `prev_scale`, `enabled`) | `PsyChannel.preecho: PreEchoState` | `psy.rs:152`, `thresholds.rs:184-267` | stream |
| TNS runtime + output info | `PsyChannel.tns_state/tns_info: TnsChannel` | `psy.rs:153-154` | stream |
| MS-Ld history (stale-on-short) | `PsyChannel.ms_ld: [[f64;64];8]` | `psy.rs:160` | stream |
| ATS/VBR state (chaos, enFac, pe correction) | `AtsElement` | `pe.rs:204-288`, `stream_driver.rs:703` | stream |
| QC/distribute/bitres state | `QcState`, `DistributeState` | `rate.rs`, `stream_driver.rs:704-705` | stream |
| Psy working buffers | `PsyScratch` (encoder-owned, reused) | `psy.rs:438-462`, `stream_driver.rs:707` | stream (reset per call) |
| S4 workspace | `ScfWorkspace` | `stream_driver.rs:706` | stream |

- **Per-channel vs shared**: pre-echo, TNS, MS-Ld are per-channel; block-switch
  pair sync and TNS pair sync couple channels; `finalShift` and `tnsSpecShift`
  are **element-global** (joint over the pair, `psy.rs:636-797, 920-949`).
- **Reset/flush**: no explicit psy reset API; state is initialized at
  `StreamEncoder::with_config` and evolves per frame. Flush steps zero-padded
  frames through the same path. `PreEchoState::new` seeds `prev` with long PCM
  floors, `prev_scale = 8`, enabled (`thresholds.rs:194-202`).
- **No global mutable state**: verified by census — no `static mut`,
  `lazy_static`, `OnceLock`/`OnceCell`, or `thread_local` in production code;
  `rust/src/aac_enc` has zero top-level `static` items (tables are `const`).
  Test-only globals exist (`scf.rs:229`, `quant.rs:825-831/970`). (A-1
  resolved.)
- **Flush semantics**: `Encoder` is single-use; there is **no reset/reinit
  API**. Flush (`encoder.rs:355-374` → `pcm.rs:284-308`) zero-pads and drains
  all-zero frames through the same `step_frame` with **live, decaying
  histories** until 1600 delay samples are appended. FPE must tolerate the
  same semantics (no special flush reset).
- **Production diagnostic captures**: `MsDecisionCapture`
  (`stream_driver.rs:1109-1131`) and `S1Cap` (`:1215-1233`) clone full operand
  vectors every stereo frame in production builds (consumed only by gated
  tests). FPE output flows through the same `LiveChannel`, so these seams
  keep working unchanged; FPE must not remove or alter them (out of scope).
- **`allow_ms` call-site note**: the MS decision is passed literal `true`
  (`stream_driver.rs:1153`) rather than `SfbConfig::allow_ms()`; equivalent
  under current construction (`use_ms=true`). FPE does not change this.
- **Existing dispatch precedent**: the only algorithm-selection mechanism
  today is the quantizer SIMD backend (arch-based, internal:
  `quant.rs:396-489`, NEON/AVX2/scalar). There is no psy-engine seam; FPE
  adds one (`06`).

## 5. Numerical representation census (verified)

| Module | Representation | Evidence |
|---|---|---|
| `block_switch.rs` | **f32 native** (IIR, energies, attack test) | `:165-183, 456-477`; module docs `:109-136` |
| `transform.rs`, `common/mdct.rs` | **f32** ring/kernel; output = C Q31 × 2^(E−31) | `transform.rs:120-125, 176, 213-218` |
| `psy.rs` spectrum lane | **f64** (real values; C stored-domain scalings tracked by `mdctScale`) | `:107-114, 356-358, 772-773` |
| `energy.rs`, `thresholds.rs` kernels | **Q31 i32 exact mirrors** (ARM64 semantics) | `energy.rs:98-118`, `thresholds.rs:98-110` |
| `q31.rs` primitives | i32/i64; `fmult`=floor(a·b/2³¹) ARM64, `fpow2_add_div2`=t+(s²>>32), `shift_int`=scaleValue, `calc_ld_data`=fLog2 Taylor, `calc_inv_ld_data`=exp2 tables | `q31.rs:98-122, 131-144, 218-224, 380-467` |
| `tns.rs` | **hybrid**: exact integer autocorrelation mirror + **float Schur** + float FIR filter | `tns.rs:98-128` |
| `sfb.rs` config | dual: integer constructions exact + **f64** mask/pcm/clip/bark | `sfb.rs:107-123, 207-227, 297-342` |
| `pe.rs`, `threshold_vbr.rs`, `scf.rs`, `quant.rs`, `rate.rs`, `bitcount.rs`, `intensity.rs`, MS mirror | **Q31 i32** | input structs §3.3; `pe.rs:340-341` |
| Ld vectors at psy output | **f64** (`log2/64`, −1.0 sentinel) | `psy.rs:1518-1537`, `sfb.rs:184-190` |
| Ld vectors downstream | **Q31 i32** (`calc_ld_data` / `q31_of_ld`) | `stream_driver.rs:145-164, 215-225` |

**Key insight**: the "Q31 engine" is already a float-spectrum + Q31-kernel
hybrid. FPE's job is to float-ify the **kernel value arithmetic**, not the
spectrum (already float) and not the downstream (must stay Q31).

## 6. Reusable infrastructure (verified)

- **MDCT/filterbank**: `common/mdct.rs` line-for-line float port of
  `mdct_block` (`cpp/libFDK/src/mdct.cpp:135`), staging verified
  (`transform.rs:100-125`). Reuse unchanged.
- **SFB tables/config**: `sfb_tables.rs` (script-extracted verbatim ROM),
  `sfb.rs` selection logic. Reuse unchanged.
- **Spreading factors / minSnr**: committed oracle data (`SPREAD_LONG/SHORT`,
  `cfg_*.txt` MINSNR). Reuse as data; convert Q31→float at consumption.
- **TNS**: `tns.rs` detect/sync/encode. Reuse unchanged (separate subsystem).
- **Scratch pattern**: `PsyScratch` (M5.20) — encoder-owned, reset-to-exact-
  initial-contents discipline (`psy.rs:420-436`). FPE mirrors this pattern.
- **Oracle + fixtures + vectors**: `tools/enc-oracle/`, 47 `psy_*` vectors,
  18 cfg snapshots, 15 fixtures + 4 WAVs (`07` §2).
- **Tier-B driver**: `rust/tools/run-reference-validation.sh` (checkout
  validation → C build → oracle → fixtures → C outputs → preflight → 15
  tests → cfg byte-gate).

## 7. Known divergences and tolerance policy (verified from docs/tests)

From `docs/aac-enc-numerical-divergence-triage.md` (2026-10-27, verdict PASS
WITH FINDINGS, no verified porting defect):

- **Block-switch knife-edge**: `m7-speech-clean` f107 attack test flips
  (C accWindowNrg 450 vs Rust 497; margin +11.1 vs −32.5) → START/SHORT/STOP
  vs LONG. Classified **intentional float-native divergence**, pinned by
  `speech_clean_f107_attack_boundary_is_float_limited`
  (`block_switch.rs:625`). The only output-changing cross-frame case.
- **TNS D-013**: closed for committed corpus (124 HI + 4 LO windows, max
  residual 11, min HI margin 54, zero activation flips).
- **MS band-16 residue**, **pre-echo ≤25 Q31** on some frames, **1-LSB
  sentinel flips**: residue-explained, tolerance-gated.
- **Rate control**: VBR-5 bitres FULL pinned; 2442 corpus frames single-pass,
  zero gain adjustments; no output-changing propagation.

Tolerance policy (verified in code):

| Domain | Envelope | Location |
|---|---|---|
| Stored Q31 ints | `32 + |w|/500` | `psy.rs:1552-1558` (`assert_q31_close`), `stream_driver.rs:2860-2862` (`q31_tol`) |
| Energy domain | `32 + E/500` | `stream_driver.rs:2875-2877` (`ld_energy_tol`) |
| Ld (log2/64) | ~1e-9 (fLog2 Taylor vs true log2) | `psy.rs:1516-1517`, `energy.rs:412-414` |
| Pure integer state (maxScale at boundaries, shifts, mdctScale) | **exact** | `energy.rs:113-118` |
| Window seq/grouping on synthetic fixtures | **exact** | `block_switch.rs:130-136` |

Oracle nondeterminism: `GSPEC`/`SPEC` inactive-band scratch wobble (11/160
files on full re-extraction); **config-only regeneration is byte-gated**
(`tools/enc-oracle/README.md:225-244`).

## 8. Performance baseline (verified, `docs/aac-enc-performance-baseline.md`)

Apple M5 arm64, release, production `Encoder` API (§12, 2026-10-27):

- 19 fixtures + derived 40 s: **460.5 ms median, ~198.6× realtime** (RTF
  0.005036). 19-fixture aggregate alone: 220.4 ms / 233.5×.
- Front-end self time: **26.70% corpus / 51.25% onset / 21.50% music /
  29.57% speech**.
- Hotspots (corpus self %): `tns::autocorr_quarter_lags` 3.39,
  `pe::form_factor` 3.18, `BlockSwitch::process` 2.93, `fft::dit_fft` 2.49,
  `tns::scale_up_spectrum` 2.48, `intensity_stereo` 2.41,
  **`psy::process_pair_reuse` 2.38**, `energy::map_spectrum_to_q31` 0.70.
- TNS stage total 6.66% corpus / 10.92% onset; transform ~3.91% corpus.
- Quantization dominates: 49.9% aggregate self (`calc_sfb_dist` 24.4%).
- Retained optimization set: +13.93% vs reconstructed pre-optimization
  baseline (§29). NEON quant kernels retained; AVX2 campaign blocked
  (`docs/aac-enc-phase5-performance-opportunity-assessment.md`, git log).

**Implication for FPE**: the psy kernel orchestration is ~2.4% of encoder
self time; the float value lane touches a subset of that. Performance risk is
low but must be measured (`08`), not assumed.

## 9. Open evidence gaps (named, not guessed)

| ID | Gap | Investigation required |
|---|---|---|
| A-1 | ~~Global-mutable-state census~~ **RESOLVED**: zero production globals (see §4) | done at audit |
| A-2 | Exact upstream `psy_configuration.h` constant line anchors (THR_SHIFTBITS/PCM_QUANT_THR_SCALE/C_RATIO) | **RESOLVED by M0** (`m0-verification.md` M0.2): anchors `psy_configuration.h:113/114/117-118` confirmed against reference source; values match Ferrite |
| A-3 | Whether the pinned reference tree `f14ad40a` is byte-identical to `3878c45a:cpp/` | **M0: substantially verified, residual gap** (`m0-verification.md` M0.3): reference parent `da00af1c:cpp` ≡ `3878c45a:cpp` (identical tree hash `97225710…`); pinned `f14ad40a` object unavailable locally (temp checkout emptied, not in Ferrite objects, Oct-16 bundle predates it, no remote). Confirm before M8; **not marked resolved** |
| A-4 | f64 vs f32 energy-sum error measurement on real corpus | M3 experiment (`05` §2, `07` §6) |
| A-5 | x86-64 performance/denormal behaviour | M9 on available x86 hardware or CI; ARM64 is the primary validated target |