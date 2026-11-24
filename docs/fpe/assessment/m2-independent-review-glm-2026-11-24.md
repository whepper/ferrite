# FPE M2 — Fresh Independent Review (GLM 5.3 Flash, 2026-11-24)

**Reviewer:** GLM 5.3 Flash (independent; not the M2 implementer).
**Review date:** 24 November 2026.
**Review verdict:** **PASS WITH FINDINGS** (M2 findings F-1 through F-4 below; all non-blocking for M2).
**Authority:** Fresh independent architecture + numerical review of the M2 Explicit Orchestration Mirror, performed against the preserved original M2 evidence bundle. This is a **new review and sign-off**. It does **not** retroactively establish a historical M2 review occurrence.
**Milestone authority:** `docs/fpe/09` M2 (FPE-2.1…FPE-2.5), `04` §4/§5/§6, `05` §7/§8, `07` §3.1/§5, ADR-0001/0003/0004/0005.
**Base state reviewed:** HEAD `b188415e3c16dbf00706984858c6a568c9b705ff` + uncommitted M1+M2 working tree (2026-11-17 01:37–01:46), as preserved in `docs/evidence/fpe-m2/ferrite-fpe-m2-review/`.
**Verification host (this review):** `aarch64-apple-darwin` (Apple Silicon, the `production_baseline` exact-hash reference platform), rustc/cargo 1.99.1 — same platform class as the bundle logs.

> **Namespace note.** M2 review findings below are labelled **M2 F-1 … M2 F-4**. They are a separate namespace from the R2.1-B per-band findings `F-01…F-30`, the `FR-*`/`QR-*`/`DN-*`/`DR-*`/`OD-*` items, the assessment `A-*` series, and any other `F-N` series. No finding is reclassified by this review.

## 1. Evidence bundle reviewed and integrity check

Reviewed bundle: `docs/evidence/fpe-m2/ferrite-fpe-m2-review/` (original M2 bundle, 2026-11-17, preserved byte-for-byte under owner D3; provenance: `docs/evidence/fpe-m1-m2-provenance.md`).

Integrity re-verified by this review from the bundle root:

```sh
cd docs/evidence/fpe-m2/ferrite-fpe-m2-review && shasum -a 256 -c SHA256SUMS
```

Result: **77/77 OK, 0 failures** (SHA256SUMS covers every bundled file except itself, including `MANIFEST.txt`). Copy fidelity `diff -rq` vs the recovered original is byte-identical per the D3 preservation record. Supplemental pins recorded in the provenance record and unchanged by this review:

```text
M2 SHA256SUMS    785ea4b104c3d10461bf081e7455145d87bca8ac5ca643b402470d
M2 report/m2-implementation-report.md 7b6ce78dbe50e499cde7c9c54799b2a606e3044651004ce0d432b0b97d4486
```

Key bundle contents relied on: `00-README.md`; `report/m2-implementation-report.md`; `report/m1-context.md`; `docs/stage-correspondence.md`; `git/diff-complete.patch` (37 entries: 5 tracked-modified + 32 new); `git/diff-tracked.patch`; `git/diff-new-files.patch`; `git/head-and-branch.txt` + status snapshots; complete `source/fpe/**` and `source/modified-rust/**`; verbatim `q31-reference/**` kernel extracts; `docs/fpe-planning/**` (21 planning docs + 5 ADRs + M0); `docs/guidance/**` (date policy + licensing); `tests/list-*.txt`; `validation/*.log` with `validation/commands.txt`.

## 2. M2-era working-tree reconstruction

The published history squash (`fe73705`) cannot be split into independently compiling M1/M2 states (assessment A-01). This review therefore reconstructed the M2-era working tree from the preserved patch and source snapshots, read-only (no repository mutation):

- Base: `b188415e3c16dbf00706984858c6a568c9b705ff` (per `git/head-and-branch.txt`).
- Applied: `git/diff-complete.patch` (37 entries) against the base.
- Cross-checked the applied result against the bundled complete file copies in `source/fpe/**` (10 files) and `source/modified-rust/**` (5 files).

**Byte-for-byte reconstruction of 15 source files:** all 15 files present in the reconstructed tree matched the bundled snapshots byte-for-byte:

FPE modules/tests (10): `fpe/constants.rs`, `fpe/energy.rs`, `fpe/mod.rs`, `fpe/orchestrate.rs`, `fpe/output.rs`, `fpe/preecho.rs`, `fpe/tests/differential.rs`, `fpe/tests/m2_stages.rs`, `fpe/tests/mod.rs`, `fpe/thresholds.rs`.
Modified Rust files (5): `aac_enc.rs`, `config.rs`, `psy.rs`, `stream_driver.rs`, `testdata.rs`.

Q31-side change confirmed as **5 additive `pub(crate)` visibility bumps only** (`fn` → `pub(crate) fn`: `lowpass_and_zero_check`, `tns_encode_channel`, `overflow_rescale`, `advance_thresholds`, `finish_channel`); `process_pair_reuse` and all helper bodies byte-identical to HEAD. Q31 value kernels (`energy.rs`, `thresholds.rs`, `q31.rs`, `tns.rs`, `sfb.rs`) untouched; downstream (quant/rate/scf/pe/bitenc/frame/block_switch/transform), manifests, lockfile, NOTICE/LICENSE/SPDX untouched.

Limitation: this is a **reconstructive** verification (finding M2 F-2), not a contemporaneous checkout. It supports the M2 acceptance because checksums, patch/file consistency, execution logs and test lists converge, but it does not convert the bundle into live git history.

## 3. Independent reproduction — 24/24 FPE tests in debug and release

This review independently reproduced the M2 FPE suite from the reconstructed tree and cross-checked the bundled execution logs (`validation/fpe-debug.log`, `validation/fpe-release.log`):

- `cargo test --locked --lib fpe::` (debug): **24 passed / 0 failed / 0 ignored**.
- `cargo test --release --locked --lib fpe::` (release): **24 passed / 0 failed / 0 ignored**.

Composition (matches `tests/list-fpe-release.txt`): 4 `fpe::constants` tests + 12 `fpe::tests::differential` tests + 8 `fpe::tests::m2_stages` tests. The bundled logs record `running 24 tests` with `test result: ok. 24 passed` in both profiles (debug 1.65 s, release 0.08 s in the bundle run; exact timing is host-dependent and not part of the gate).

Supporting validation reproduced/cross-checked from bundle logs (not re-counted as passing where blocked): Tier-A lib 388 passed / 0 failed / 6 ignored (release) and 392 / 0 / 2 ignored (debug) with 15 CI skips; production baseline 2/2 (exact FNV-1a + byte totals); production API 6/6; doctests 19/19; FPE seam end-to-end byte-identity 3/3 (debug + release); `cargo fmt --check` clean; Clippy 0 errors (2 informational notes, M2 F-4); default + no-default-features builds OK; `check-licensing.sh` OK; `git diff --check` clean. The 11 Tier-B tests fail loudly on missing C-reference/decoder artifacts (`PHASE1_ORACLE_DIR`/`PHASE1_C_DIR`/`M55_C_DIR`/`AAC_DEC_BIN`) — **BLOCKED, not passing** (M2 F-1); identical to the M0/M1 blocked set.

## 4. Architectural findings

- **Separate orchestration (FPE-2.2).** M2 replaces the M1 whole-function delegation with an explicit per-pair FPE processing sequence (`fpe/orchestrate.rs::process_pair`, 686 lines) that stage-for-stage mirrors `psy.rs::process_pair_reuse` (`psy.rs:509–1130`). Stage correspondence verified against `docs/stage-correspondence.md` stages 0–12: D-022 entry asserts → per-channel long/short config → lowpass/zeroSpec + initial exponents (12/9) → Q31 map → joint headroom/`min_spec_shift` → raw energies + joint `max_nrg` → `zero_elem` shortcut / `rescale_shifts` / re-shift + `init_threshold` / representation shift → TNS detect/sync/active-OR/`tns_encode_channel`/element-global `overflow_rescale` → `advance_thresholds` (clip/spread/PCM/pre-echo/spread-energy) → MS energies + stale-on-short Ld history → grouping + output assembly. Element-global decisions (`min_spec_shift`, `final_shift`, `tns_spec_shift`, `zero_elem`, TNS active-OR) and scratch reset discipline are owned by FPE.
- **FPE-owned state and scratch (FPE-2.1).** `FpeChannel` owns individual fields (`preecho: PreEchoState`, `tns_state`/`tns_info: TnsChannel`, `ms_ld: [[f64;64];8]`); `FpeScratch` owns its int buffers mirroring the `PsyScratch` layout and reset discipline. Init mirrors `PsyChannel::new` (verified by `fresh_fpe_state_init_matches_q31`). No shared mutable Q31 state; `PsyChannel`/`PsyScratch` types and instances untouched. Private state inspected only via `#[cfg(test)]` helpers (FPE-2.5); **no new production API for testing**.
- **Q31 value-kernel reuse; no float psychoacoustic kernel (P2).** Every value calculation calls an existing Q31 kernel (`energy::*`, `thresholds::init_threshold`, `q31::shift_int`, `tns::*`, the 5 seam-exposed `psy` stage helpers). Results pass through unchanged. Confirmed: no Q31→float conversion, no normalization/rescaling, no rounding/quantization boundary, no independent float state, no table regeneration. Boundary conversion (`05` §7) is inactive in M2 by design and activates per-kernel only from M3+.

## 5. Output, mutated-spectrum and frame-surviving-state equivalence

The harness feeds identical spectra to both engines over multi-frame sequences with independently initialized equivalent state and asserts bit-identity **every frame** (`==` on integers, `to_bits()` on f64; max delta 0 on every field):

- **Complete observable output:** every field of `ChannelOutput`/`GroupedOutput`/`MsOutput` (energies, thresholds, spread, `energy_ld`, `threshold_ld`, `mdct_scale`, `final_shift`, `tns_shift`, `nrg_scaling`, `maxscale`, `nm1`, `m_nm1`, `calc_pre_echo`, grouped vectors, MS vectors, `max_sfb_per_group`), including grouped stride layout and threshold-Ld floor.
- **Mutated spectra:** TNS filtering + exact scalings (each engine mutates its own copy of the identical input spectrum).
- **Frame-surviving state after every frame:** `PreEchoState` (`prev`/`prev_scale`/`enabled`), TNS `tns_state` + `tns_info` (all pub fields), `ms_ld` `[[f64;64];8]` **including stale-on-short contents**, plus per-frame representation shifts (`mdct_scale`/`final_shift`/`tns_shift`) captured in `repr_seen`.
- **Execution integrity:** exact frame accounting asserted on every run (`frames_compared`); coverage accounting asserts the intended paths (long/short transitions, START/STOP, pre-echo disable/reset, stale-on-short reads) actually executed.

D-022 preconditions verified: release-active `assert!` at orchestration entry with byte-identical messages (LFE-must-be-Long; CPE-sequence-agreement), exercised by 3 `should_panic` + 3 control tests passing in debug **and** release. Representation-lane invariants verified: `finalShift >= 0` every frame, non-zero rescale shift exercised, both window types present, START/STOP present, `mdctScale` observable; M2 invariant guards in `output.rs` are `debug_assert!` (no release behaviour change) except the release-active D-022 `assert!`.

## 6. Twelve multi-frame scenarios

All scenarios run the full fixture plus two zero-padded tail frames with exact accounting. “Multi-frame” means complete output + mutated spectrum + persistent state compared after **every** frame of the sequence.

| ID | Scenario (test) | Fixtures / frames | Coverage asserted |
|---|---|---|---|
| S-01 | Onset long/short transitions + stale MS-Ld (`onset_stereo_long_short_transitions_and_stale_ms_ld`) | `onset-stereo` (START f23, EIGHT_SHORT f24 grouping [3,1,3,1], STOP f25 + tails) | Long + Start + Short + Stop; short ≥ 1; pre-echo disabled ≥ 1; stale MS reads ≥ 1 |
| S-02 | Stepdown offset transition (`stepdown_stereo_offset_transition`) | `stepdown-stereo` (START f0, SHORT f1 grouping [1,3,3,1], STOP f2 + decay) | Start + Short + Stop; stale MS reads ≥ 1 |
| S-03 | Click transient mono (`mono_transients_click_impulse_edge` on `click-mono`) | `click-mono` (committed seq: short f1) | Long + short; mono element path (no right/MS) |
| S-04 | Impulse transient mono (same test on `impulse-mono`) | `impulse-mono` | Long; isolated full-scale impulse |
| S-05 | Edge cross-border attack mono (same test on `edge-mono`) | `edge-mono` (short f3/f4) | Long + short; cross-border attack repeat |
| S-06 | Sustained tonal + noise (`sustained_tonal_and_noise`) | `sine-stereo`, `multitone-stereo`, `sweep-stereo`, `noise-mono`, `tonal-noise-stereo` | Long-dominated corpus; initial-frame SHORT blips are shared front-end decisions |
| S-07 | Silence + near-silence (`silence_and_near_silence`) | `silence-stereo`, `fpe-allzero-stereo` (all-long, zeroSpec path, −1.0 Ld placeholders), `quiet-sine-stereo`, `fpe-near-silence-ramp-stereo` | All-long for all-zero; PCM-floor-dominated low-level content |
| S-08 | Stress + full-scale edges (`stress_and_full_scale_edges`) | `stress-stereo`, `fpe-fullscale-square-stereo` (zero mid, full side), `fpe-max-headroom-stereo` | Transient/short coverage; clip/saturation/overflow-rescale/joint-shift pressure |
| S-09 | Power-of-two boundary energies (`pow2_boundary_energies`) | `fpe-pow2-boundary-stereo` (2⁰…2¹⁴ segments) | Long; clz headroom + rescale knife edges |
| S-10 | Mixed silent/active pair (`mixed_silent_active_pair`) | `fpe-mixed-pair-stereo` (ch0 onset at 0.5 s, ch1 silent) | Start + Short + Stop; asymmetric joint shifts; one-sided MS; stale MS reads ≥ 1 |
| S-11 | Second sample rate (`rate_44100_sine_and_onset`) | `sine-stereo-44100`, `onset-stereo-44100` (different SFB widths) | Long; Start + Short + Stop; stale MS reads ≥ 1 |
| S-12 | Real music + speech (`real_music_and_speech`) | `music-stereo`, `speech-clean-stereo`, `speech-pauses-stereo`, `music-eroica-stereo` (~471 frames each, licensed 10 s recordings) | Long multi-frame runs with live decaying histories; shared front-end knife edge (speech-clean f107) identical by construction |

Single-frame supplements (not counted in the twelve): committed-oracle MDCT vectors (`committed_mdct_vectors_single_frame`: sweep-stereo f10 long, onset-stereo f24 short grouping [3,1,3,1], click-mono f11) and D-022/LFE/representation-lane stage tests (§8/AC-09), including the LFE structural early-out parity (`fpe_lfe_path_skips_tns_and_preecho`) and the representation-lane observability run over `onset-stereo`.

## 7. M2 acceptance criteria — results (10/10 satisfied)

Source: `docs/fpe/09` M2 (contract, files, tasks, tests, acceptance, risks) as planned; verified against the reconstructed tree, stage correspondence, harness comparators and logs.

| # | Criterion | Result |
|---|---|---|
| AC-01 | Separate explicit orchestration mirror implements the full FPE processing sequence and owns order/conditionals/element-global decisions (FPE-2.2; stages 0–12 per `stage-correspondence.md`) | **PASS** — token-faithful stage mirror verified |
| AC-02 | FPE-owned state/scratch with init mirroring Q31 and no shared mutable Q31 state; test-only comparison helpers only, no new production API (FPE-2.1/FPE-2.5) | **PASS** — `fresh_fpe_state_init_matches_q31` + per-frame state gate |
| AC-03 | Value lane delegates to existing Q31 kernels for every value calculation; no floating-point psychoacoustic kernel introduced | **PASS** — call-site audit + `energy.rs`/`thresholds.rs` skeletons unused by M2 value lane |
| AC-04 | No float round-trip/rounding; `05` §7 boundary quantization inactive in M2 (P2/R-19) | **PASS** — pass-through confirmed; no conversion in diff |
| AC-05 | Complete observable output bit-identical (every `ChannelOutput`/`GroupedOutput`/`MsOutput` field) | **PASS** — `assert_channel_output_eq`/`assert_grouped_output_eq`/`assert_ms_output_eq` every frame |
| AC-06 | Mutated spectra bit-identical (TNS filtering + exact scalings) | **PASS** — per-channel `to_bits()` comparison every frame |
| AC-07 | All frame-surviving state bit-identical after every frame (`PreEchoState`, `tns_state`/`tns_info`, `ms_ld` incl. stale-on-short) | **PASS** — `assert_psy_state_eq` every frame |
| AC-08 | Equivalence checked per frame across multi-frame sequences covering long/short, START/STOP, pre-echo enable/reset, stale-on-short MS-Ld (§6 S-01…S-12) | **PASS** — 12 scenarios with asserted coverage + exact frame accounting |
| AC-09 | Representation lane exact and observable (`mdctScale`/`finalShift`/`tnsShift`/`maxScale`); `finalShift >= 0`; D-022 entry asserts (release-active) + M2 invariant guards; no new production API; no new kernel allocation | **PASS** — per-frame repr comparison + `representation_lane_exact_and_observable_across_transitions` + 3+3 D-022 tests |
| AC-10 | Q31 regression preserved: Q31 default unchanged, production hashes/byte totals unchanged, Tier-A green, no manifest/dependency/licensing change | **PASS** — default-selection + allocate-selected-only tests; baseline 2/2; Tier-A per §3; `check-licensing.sh` + `git diff --check` + package list OK |

## 8. Findings F-1 through F-4 (all non-blocking for M2)

| ID | Finding | Severity / status | Limitation |
|---|---|---|---|
| M2 F-1 | Tier-B C-reference parity blocked | **Informational-blocked (external); NON-BLOCKING for M2 Q31-equivalence** | Pinned C reference `f14ad40a` unavailable locally; 11 Tier-B tests BLOCKED (loud failure, not counted as passing); A-3 unresolved; identical to the M0/M1 blocked set. M2 acceptance is Q31-equivalence only and does not require C parity. G1/G5, M8 and Stage-1 closure remain blocked. |
| M2 F-2 | Provenance/reconstruction limitation | **Known limitation; NON-BLOCKING** | The M2-era tree is reconstructed from the preserved patch + source snapshots (37-entry `diff-complete.patch` + 15 byte-identical files, §2), not from live splittable history. Convergence of 77/77 checksums, patch/file consistency, 24/24 reproduction and log accounting supports acceptance, but the reconstruction must be cited as reconstructive. Original M2 review occurrence remains unconfirmed as history; this review is a separate event. |
| M2 F-3 | LFE numerical/coverage differential tracked as M3+ gap | **Coverage gap; NON-BLOCKING for M2** | LFE is exercised only via the mono-Long structural early-out parity path (`fpe_lfe_path_skips_tns_and_preecho`: TNS fully inactive, pre-echo history at initializer, energies still computed; output/spectrum/state parity holds on the tested path). No multi-frame LFE decaying-history sequence exists. Any future float-kernel interaction with LFE-adjacent paths must extend LFE coverage; tracked as an M3+ coverage item. Not an M2 failure. |
| M2 F-4 | Informational Clippy findings | **Informational; NON-BLOCKING** | 2 notes in `fpe/orchestrate.rs:420/655` (`needless_range_loop`, `needless_option_as_deref`) are token-faithful mirrors of pre-existing Q31 patterns (`psy.rs:865/1099`); kept for mirror faithfulness. Broader pre-existing warnings are unchanged. CI Clippy is informational/non-gating; G-1 “clippy clean” was scoped to G-1-touched files. Fix in a future approved touch. |

No finding reclassifies, widens or narrows any R2.1-B disposition, envelope, tolerance, bound, supported domain or decision definition. The nine conditional R2.1-B acceptances (F-07/09/10/15/17/18, F-20/22/26) are untouched. Tier-B remains blocked. G0 remains historical/non-rerunnable (A-01 open).

## 9. Reviewer conclusion

M2 satisfies its acceptance criteria (AC-01…AC-10, §7). The evidence — preserved bundle with 77/77 checksum verification, byte-for-byte reconstruction of the 15 source files, independent reproduction of 24/24 FPE tests in debug and release with per-frame complete-output, mutated-spectrum and frame-surviving-state bit-identity across the twelve multi-frame scenarios — supports M2 acceptance as a **Q31-equivalent explicit orchestration foundation**. Verdict: **PASS WITH FINDINGS** (M2 F-1…F-4 non-blocking, retained as stated).

## 10. Scope and limitations (binding on any citation)

M2 is an accepted **Q31-equivalent explicit orchestration mirror**: FPE owns its orchestration state; orchestration is separately implemented; the intended Q31 value kernels are reused; complete output equivalence is established against Q31; relevant frame-surviving state is compared bit-identically; equivalence is checked per frame across multi-frame sequences; preserved evidence and independently reproduced tests support the claims.

M2 is **NOT**: a float-native psychoacoustic engine; evidence of perceptual equivalence; evidence of C-reference Tier-B parity; authorization to switch the FPE implementation to default; authorization to begin R2.2. The float value lane, boundary quantization, R2.2 downstream decision-safety, perceptual validation, Tier-B parity, performance acceptance and any default change are all out of scope and remain governed by their own gates and owner decisions.

## 11. References

- Evidence bundle: `docs/evidence/fpe-m2/ferrite-fpe-m2-review/` (`00-README.md`, `report/m2-implementation-report.md`, `report/m1-context.md`, `docs/stage-correspondence.md`, `git/*`, `source/**`, `q31-reference/**`, `docs/fpe-planning/**`, `docs/guidance/**`, `tests/*`, `validation/*`).
- Provenance: `docs/evidence/fpe-m1-m2-provenance.md`.
- Planning authority: `docs/fpe/09` M2; `04` §4/§6; `05` §7/§8; `06` §3–§5; `07` §3.1/§5/§8 (G0); ADR-0001/0003/0004/0005.
- Standing limitations: `docs/fpe/assessment/findings-register.md` (A-01/A-02, R-19/R-21, OD-1); assessment roadmaps (G0 historical, Tier-B blocked, nine conditionals conditional).

*Record created 2026-11-24 to durably preserve the GLM 5.3 Flash fresh independent M2 review. Original M2 bundle bytes and historical review-occurrence statements are unchanged by this record.*
