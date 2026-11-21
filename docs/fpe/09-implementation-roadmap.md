# 09 — Implementation Roadmap

Date: 2026-11-16. Grounded in the audit (`01`), reference analysis (`03`),
architecture (`04`), translation spec (`05`), integration (`06`), validation
(`07`). Tasks are small, independently implementable, and cite exact files /
reference functions so a coding model needs no architectural rediscovery.
Delegation rules and report format: `11`.

## Incremental strategy (recommended)

Build the FPE **orchestration + representation lane + pass-through assembly**
first (M2), with the value lane **delegating to the existing Q31 kernels and
passing their results through unchanged** (no float conversion, no rounding).
At M2, FPE output is therefore **bit-identical to Q31** on the complete
observable output — proving the wiring and state are correct. Then M3–M6
replace each kernel group with float arithmetic **one group at a time**
(introducing the fraction value lane and boundary quantization per kernel),
each step gated by the Q31↔FPE differential (delta within envelope;
pure-integer/decision fields stay exact). This isolates any defect to the
single kernel group just changed.

## Milestone overview

| M | Objective | Key gates | Review |
|---|---|---|---|
| M0 | Audit confirmation + reference provisioning + baseline | A-2/A-3 resolved; baseline captured | owner |
| M1 | Architecture scaffolding + reference harness + fixtures | harness runs; new fixtures stable | architecture |
| M2 | FPE skeleton, selection seam, pass-through (== Q31 output) | FPE≡Q31 bit-identical (complete output); default unchanged | architecture + numerical |
| M3 | Float energy kernels | energy within envelope; shifts exact | numerical |
| M4 | Float threshold init/clip/spreading (tonality absent) | thresholds within envelope | numerical |
| M5 | Float PCM floor (absolute floor) | thresholds within envelope | numerical |
| M6 | Float pre-echo (temporal) + attack reuse | history within envelope; transitions exact | numerical |
| M7 | MS energies + grouping + TNS/LFE integration | full orchestration; grouped MS-Ld exact | numerical + integration |
| M8 | Full three-way differential validation | G1–G5, G7 | validation (independent) |
| M9 | Performance + final audit + docs | G6, G8, G9 | owner + independent |

---

## M0 — Audit confirmation and reference provisioning

**Objective**: confirm the plan's evidence base; re-provision the C reference;
capture the performance baseline. (Most audit is done in this package.)

**Files**: no production changes. Scratch: reference checkout, baseline logs.

**Tasks**:
- FPE-0.1 Re-provision the pinned reference checkout (`f14ad40a…`); run
  `rust/tools/check-reference.sh --ref-dir …`. Resolve **A-3** by diffing
  `cpp/libAACenc/src/{psy_main,band_nrg,spreading,pre_echo_control,psy_configuration}.cpp`
  against `git show 3878c45a:cpp/…`. Record result.
- FPE-0.2 Resolve **A-2**: `git show 3878c45a:cpp/libAACenc/src/psy_configuration.h`;
  record exact line anchors for THR_SHIFTBITS/PCM_QUANT_THR_SCALE/C_RATIO.
- FPE-0.3 Capture Q31 performance baseline (`08` §3.1) on available hardware;
  record psy self-time + throughput + fixture hashes.
- FPE-0.4 Build `enc_oracle` + fixtures in scratch; confirm oracle dumps
  regenerate (config-only byte-gate).

**Dependencies**: none. **Tests**: `check-reference.sh` passes; cfg byte-gate
4/4. **Acceptance**: A-2/A-3 resolved or gap documented; baseline recorded;
oracle runnable. **Risks**: R-12 (reference unavailable) — if the private
reference cannot be re-provisioned, fall back to `3878c45a:cpp/` build and
document the provenance assumption. **Review**: owner confirms evidence base.

> **M0 execution status (2026-11-16, `m0-verification.md`):** executed as a
> source/reference verification milestone. **A-2 resolved** (anchors/values
> verified). **A-3 substantially verified with a documented residual gap** —
> reference parent `da00af1c:cpp` ≡ `3878c45a:cpp` (identical tree hash
> `97225710…`); the pinned `f14ad40a` object is unavailable locally, so FPE-0.1
> direct diff and FPE-0.4 `enc_oracle` build are **blocked on private-reference
> re-provisioning** (R-12); the `3878c45a:cpp/` fallback was re-provisioned to
> scratch. **Q31 baseline captured** (FPE-0.3): Tier-A 363 pass / 11 Tier-B
> artifact-gated; production baseline hashes green; throughput recorded. A-3 is
> **not** marked resolved; confirmation carried to before M8.

## M1 — Architecture scaffolding and reference harness

**Objective**: create the `fpe/` module tree, the selection seam, and the
Q31↔FPE in-binary differential harness; add edge-case fixtures.

**Files expected to change**:
- NEW `rust/src/aac_enc/fpe/{mod,constants,energy,thresholds,preecho,orchestrate,output}.rs`
  (skeletons).
- `rust/src/aac_enc.rs` (add `pub(crate) mod fpe;`).
- `rust/src/aac_enc/config.rs` (additive `PsyEngineKind`, `with_psy_engine`,
  private field; default Q31).
- `rust/src/aac_enc/stream_driver.rs` (additive fields + dispatch match at
  `:847/857`; `with_config` selection).
- NEW `tools/enc-oracle/make_fpe_fixtures.py` (or extend `make_fixtures.py`).
- NEW `rust/src/aac_enc/fpe/tests/` harness.

**Tasks**:
- FPE-1.1 Module tree + `constants.rs` (`05` §3) with unit tests asserting
  each float constant equals `q31/2^31` exactly.
- FPE-1.2 Selection seam (`06` §1–§3): `PsyEngineKind`, config builder,
  `StreamEncoder` fields, dispatch match. Default Q31; existing constructors
  unchanged.
- FPE-1.3 Q31↔FPE in-binary harness (`07` §3): feed identical inputs to both
  engines, compare outputs field-by-field with `q31_tol`/exact rules.
- FPE-1.4 Edge-case fixtures (`07` §6): near-silence ramp, full-scale square,
  power-of-two-boundary energies, max-headroom bands, all-zero element, mixed
  silent+active pair. Generate vectors via 3-run stability validation.

**Dependencies**: M0. **Tests**: constants exact; selection seam test (default
Q31, builder works); harness compiles and runs (FPE skeleton may stub).
**Acceptance**: tree compiles; default output byte-identical to baseline;
harness operational; fixtures stable. **Risks**: R-01 (accidental Q31 change)
— seam must be additive only; review diff for any Q31 behaviour change.
**Review**: architecture review of module layout + seam.

## M2 — FPE skeleton, representation lane, pass-through (== Q31)

**Objective**: wire the full FPE orchestration with the representation lane
(reused), **delegating the value lane to the existing Q31 kernels and passing
their results through unchanged**, so FPE output is bit-identical to Q31.

**M2 pass-through contract (P2 — binding):**
- M2 introduces **no floating-point value arithmetic and no numerical
  difference**. The value kernels call the Q31 helpers, which already produce
  stored `i32` energies/thresholds/spread and f64 Ld.
- Those Q31 results pass through to the shared output types **unchanged**.
  **Do not** convert Q31 values to float and back to satisfy the FPE
  interface; **do not** apply `round(v_f·2^31)`, normalization, or rescaling.
  The `05` §7 boundary quantization is **inactive in M2**.
- The Ld vectors are produced by the **same** Q31 float-lane functions
  (`rescale_ld_into`, `grouped_ld`, `grouped_ld_clamped`) on the **same** stored
  ints — not recomputed from float — so they are bit-identical.
- The mutated f64 spectrum (TNS) is produced by the same reused TNS calls.
- Boundary conversion is introduced **only** in M3+ when a specific kernel is
  replaced by float arithmetic (`05` §7, ADR-0004).
- The bit-identical acceptance criterion applies to the **complete observable
  output**: every field of `ChannelOutput`/`GroupedOutput`/`MsOutput`
  (energies, thresholds, spread, energy_ld, threshold_ld, mdct_scale,
  final_shift, tns_shift, nrg_scaling, maxscale, nm1, m_nm1, calc_pre_echo,
  grouped vectors, ms vectors, max_sfb_per_group) **and** the mutated spectrum.
- **Persistent-state equivalence (binding):** after **each** processed frame,
  **all persistent state that survives the frame and can influence subsequent
  processing** must also be bit-identical between FPE and Q31:
  - `PreEchoState` — `prev` thresholds, `prev_scale`, `enabled`
    (`thresholds.rs:184-188`);
  - TNS channel state — `tns_state` (runtime) **and** `tns_info` (output):
    `num_filters/coef_res/length/order/direction/coef/active/gain/merged`
    (`tns.rs:248-267`);
  - MS-Ld history — `ms_ld: [[f64;64];8]` (`psy.rs:160`), including the
    stale-on-short contents;
  - any other frame-surviving psy state (none beyond the above within the psy
    boundary; upstream `BlockSwitch`/`ChannelTransform` state is reused and
    identical by construction, downstream ATS/QC state is outside the psy
    boundary and shared).
- **Multi-frame sequences (binding):** the harness processes **multi-frame
  sequences, not isolated frames**, and compares output **and state after every
  frame**. Sequences must exercise long/short transitions, START/STOP
  sequences, pre-echo enable/reset behaviour (STOP-before/START-after,
  memorize-only frames), and stale-on-short MS-Ld history (e.g. onset,
  stepdown, click, impulse, sine→onset→silence runs from the corpus).
- **Test-only comparison helpers:** private state (`PreEchoState` fields,
  `PsyChannel.tns_state`/`ms_ld`) is compared via **`#[cfg(test)]` helpers**
  (test-only accessors/snapshot or field-by-field compare). `PreEchoState`
  already exposes `prev()/prev_scale()/is_enabled()`; `TnsChannel` fields are
  `pub`. **No additional production API is exposed solely for testing**
  (`07` §3.1, `04` §4).

**Files**: `fpe/orchestrate.rs`, `fpe/output.rs`, `fpe/mod.rs`
(`FpeChannel`, `FpeTables`, `FpeScratch`; in M2 `FpeChannel` holds the Q31
`PreEchoState`/`TnsChannel`), `fpe/preecho.rs` (delegates to Q31 in M2; fraction
`FpePreEchoState` introduced in M6); `#[cfg(test)]` state-comparison helpers
(FPE-2.5, no production API).

**Tasks**:
- FPE-2.1 `FpeChannel`/`FpeScratch` (`04` §4/§5); init mirrors
  `PsyChannel::new`. In M2 `FpeChannel` holds the **Q31 `PreEchoState`** and
  delegates pre-echo control to it (bit-identical history); the fraction
  `FpePreEchoState` is introduced in M6.
- FPE-2.2 Orchestration mirroring `psy.rs:495-1116` order (`04` §6): lowpass/
  zeroSpec, representation lane (reuse `energy.rs` helpers), TNS calls (reuse),
  threshold-advance call sites, MS call sites, grouping, output assembly. Value
  kernels initially call the Q31 helpers.
- FPE-2.3 Shared-output assembly as **pass-through** of Q31 results (no
  boundary quantization in M2; `05` §7 boundary is M3+); allocate-selected-only
  state (`06` §3).
- FPE-2.4 D-022 preconditions + invariant guards (`05` §8).
- FPE-2.5 **Test-only state-comparison helpers** (`#[cfg(test)]`, no production
  API): snapshot/compare `PreEchoState` (via existing accessors), `tns_state`
  and `tns_info` (pub fields), and `ms_ld` (test-only accessor). Extend the
  Q31↔FPE harness to run **multi-frame sequences** and assert output **and
  persistent-state** bit-identity after **every** frame, covering long/short,
  START/STOP, pre-echo enable/reset, and stale-on-short MS-Ld (`07` §3.1/§5).

**Dependencies**: M1. **Tests**: Q31↔FPE harness asserts **bit-identical
complete observable output AND persistent state after each frame** (`==` every
output field + mutated spectrum + `PreEchoState`/`tns_state`/`tns_info`/`ms_ld`)
over **multi-frame sequences** spanning the committed `psy_*` vectors + fixtures
(value lane = Q31, pass-through). Default regression. **Acceptance**: FPE≡Q31
bit-exact on the complete output **and all frame-surviving state, per frame**;
no float round-trip/rounding introduced; no new production API for testing; no
new allocation in kernels; invariants hold. **Risks**: R-02 (orchestration
order drift), R-08 (state init mismatch), R-19 (accidental float round-trip in
M2), R-21 (state divergence hidden by single-frame tests) — mitigated by the
per-frame complete-output-and-state bit-exact gate over multi-frame sequences.
**Review**: architecture + numerical review of orchestration vs `psy.rs`.

## M3 — Float energy kernels

**Objective**: replace energy value arithmetic with f64 (`05` §6.1/§6.2).

**Files**: `fpe/energy.rs`, `fpe/orchestrate.rs` (call float energy),
`fpe/output.rs` (energy Ld).

**Tasks**:
- FPE-3.1 Float rescale band energy (long+short), fraction domain (`05` §6.1):
  `E_real=Σspec_f64²`, `E_f=E_real·2^(−2m')`; boundary `E_st=round(E_f·2^31)`;
  rescale energy Ld with §6.8 conventions.
- FPE-3.2 Float TNS energy recompute (long with integer shiftBits from repr
  lane; short no-Ld quirk); overflow rescale value lane.
- FPE-3.3 A-4 experiment: measure f32-vs-f64 energy-sum error on corpus;
  record envelope; confirm f64 choice.

**Reference**: `band_nrg.cpp:140-184/192-256/264-288`; `energy.rs:161/249/215`.
**Dependencies**: M2. **Tests**: unit energy sums (hand-computed, silence,
full-scale); Q31↔FPE energy within the **owner-approved FPE-3.1R1 derived
per-band envelope** (`05` §9.2); `q31_tol` retained for thresholds, `nm1`,
grouped thresholds and reused Q31-kernel outputs; shifts/maxScale **exact**;
C↔FPE `NRG/NRGLD` (Tier-B). **Acceptance**: G2 for energy with the R1
envelope; envelope and absolute Ld differences recorded.
**Risks**: R-03 (lost scaling), R-04 (Ld convention) — cross-check §6.8 table.
**Review**: numerical review.

> **FPE-3.1R1 amendment (2026-11-17).** The original M3 wording
> “Q31↔FPE energy within `q31_tol`” is superseded for rescale/spread/grouped
> energies by the owner-approved derived envelope (`05` §9.2, ADR-0004
> amendment). `q31_tol` remains applicable to the fields listed above.
> FPE-3.1R2 downstream decision-safety validation is still required before
> FPE-3.1 acceptance.
>
> **R2.1-B amendment (2026-11-18).** IS/MS decision-safety is implemented in
> `rust/src/aac_enc/fpe/tests/is_ms.rs` (R2.1-B note,
> `docs/fpe/R2.1-B-implementation-note.md`): production-callsite IS/MS
> capture/replay, source-derived I1–I10 margins, stateful IS trace with
> carry-origin tracking, `B_closed` MS bound validation, and the N/L/P/G/F
> consequence model over the live dual-engine harness. Tier-B C parity
> remains INCOMPLETE / BLOCKED (pinned C reference unavailable).

## M4 — Float threshold init, clip, spreading (tonality absent)

**Objective**: float threshold init + clip + spreading (`05` §6.3/§6.4).
**Tonality is not implemented** (PNS-gated upstream; `03` §4.2) — task asserts
absence.

**Files**: `fpe/thresholds.rs`, `fpe/orchestrate.rs`.

**Tasks**:
- FPE-4.1 Float threshold init (`T_f=E_f·C_RATIO_f`, fraction domain) + clip
  (branch logic, `05` §6.3).
- FPE-4.2 Float two-pass spreading (threshold factors) + spread-energy
  copy/spread (sprEn factors), consuming tabulated f64 factors.
- FPE-4.3 Assert no tonality symbol added; document absence in module docs.

**Reference**: `psy_main.cpp:702-703/926-939/1009-1017`; `spreading.cpp:105-125`;
`thresholds.rs:130/137/149`. **Dependencies**: M3. **Tests**: unit init/clip/
spreading (hand-computed envelope, edge factors 0); Q31↔FPE `THR`/`SPREAD`
within envelope; C↔FPE Tier-B. **Acceptance**: G2 for thresholds/spread.
**Risks**: R-05 (tuning drift — must use tabulated factors, not recompute),
R-04. **Review**: numerical.

## M5 — Float PCM floor (absolute hearing floor)

**Objective**: float PCM quantization floor (`05` §6.4). Document that FDK has
no ATH table; the PCM floor is the absolute floor (`03` §4.5).

**Files**: `fpe/thresholds.rs`, `fpe/orchestrate.rs`.

**Tasks**:
- FPE-5.1 Float PCM floor (fraction domain, `05` §6.4) with accumulating
  `energyShift` (start 2m, +16/win, cap 31, sign branches),
  `max(T_f/16, pcm_f·2^∓shift)` where `pcm_f = SfbConfig::pcm_quant()`.

**Reference**: `psy_main.cpp:955-971`; `psy_configuration.cpp:325-334`;
`thresholds.rs:167`. **Dependencies**: M4. **Tests**: unit floor accumulation
(per-window shift, cap, sign branch); Q31↔FPE `THR` within envelope; C↔FPE.
**Acceptance**: G2 for PCM floor. **Risks**: R-03 (shift accumulation), R-06
(short-window per-window accumulation). **Review**: numerical.

## M6 — Float pre-echo (temporal masking); attack detection reused

**Objective**: float pre-echo control + history (`05` §6.5). Attack detection
(block switching) is reused unchanged (`03` §4.8).

**Files**: `fpe/preecho.rs`, `fpe/orchestrate.rs`, `fpe/output.rs` (nm1 readout).

**Tasks**:
- FPE-6.1 Introduce the fraction `FpePreEchoState` (`prev_f`, `05` §6.5),
  **replacing the M2 delegated Q31 `PreEchoState`**; float clamp/floor/memorize,
  scale alignment, both branches. Init `prev_f = pcm_quant()` long (fraction).
- FPE-6.2 STOP-before/START-after resets; init; enabled flag transitions.
- FPE-6.3 nm1/m_nm1/calc_pre_echo readout (quantize history).

**Reference**: `pre_echo_control.cpp:106-176`; `psy_main.cpp:975-1006`;
`thresholds.rs:184-267`. **Dependencies**: M5. **Tests**: unit bypass/clamp/
floor/memorize/reset (mirror `thresholds.rs:328-370`); cross-frame onset/
stepdown differential `NM1/NM1STATE`; history within envelope; transitions
exact. **Acceptance**: G2 for temporal; no new decision flips. **Risks**: R-07
(temporal state drift), R-06. **Review**: numerical.

## M7 — MS energies, grouping, TNS/LFE integration

**Objective**: complete the orchestration: float MS energies, grouping,
TNS integration, LFE early-outs, stale-on-short MS-Ld.

**Files**: `fpe/energy.rs` (MS), `fpe/orchestrate.rs`, `fpe/output.rs`.

**Tasks**:
- FPE-7.1 Float MS energies (/4 convention, `05` §6.6); calc_ld long-fresh/
  short-stale; ms_ld history.
- FPE-7.2 Grouping: quantize per-window → integer regroup (reuse logic);
  grouped offsets/maxSfb/spectrum interleave; **grouped MS-Ld exact
  `calc_ld_data`**.
- FPE-7.3 TNS orchestration integration (detect on repr Q31, encode on f64,
  sync, active OR, overflow); LFE early-outs + D-022.
- FPE-7.4 Long/short output assembly (maxSfb floor, threshold Ld floor).

**Reference**: `band_nrg.cpp:296-361`; `grp_data.cpp:118-264`;
`psy_main.cpp:766-920/1039-1130`; `energy.rs:321`; `psy.rs:1285/1420`.
**Dependencies**: M6. **Tests**: unit MS /4 convention; grouped layout contract;
Q31↔FPE full-output differential (all fields); C↔FPE `MSM/GROUPED/LONGOUT`;
LFE + mixed-pair edge fixtures. **Acceptance**: full orchestration; G2 for
MS/grouping; grouped MS-Ld exact. **Risks**: R-09 (channel-pair regression),
R-06 (grouped stride), R-10 (MS-Ld exactness). **Review**: numerical +
integration.

## M8 — Full differential validation

**Objective**: prove G1–G5, G7 on the full corpus; independent validation review.

**Files**: `fpe/tests/` (Tier-B comparisons), testdata (new vectors), docs.

**Tasks**:
- FPE-8.1 C↔FPE Tier-B comparisons for every §4 stage on full §6 corpus.
- FPE-8.2 Triage any out-of-envelope/new-flip findings (residue vs structural),
  pin classified exceptions, owner approval.
- FPE-8.3 Integration: FPE-selected encode of corpus; decode via `AAC_DEC_BIN`;
  selection seam; default regression (G3).
- FPE-8.4 Numerical-safety property/fuzz test over random spectra (G4).
- FPE-8.5 Licensing hygiene (`check-licensing.sh`) + `git diff --check` (G7).
- FPE-8.6 Public API exposure of `with_psy_engine`/`PsyEngineKind` (stable,
  documented; default still Q31).

**Dependencies**: M7, M0 reference. **Tests**: all tiers (`07` §7).
**Acceptance**: G1–G5, G7 pass. **Risks**: R-11 (inadequate reference testing),
R-12. **Review**: **independent validation review** (not the implementer).

## M9 — Performance evaluation and final audit

**Objective**: G6, G8, G9; finalize docs; owner acceptance.

**Files**: docs (`08` results), risk register update, no production change
unless a budget finding is approved.

**Tasks**:
- FPE-9.1 A/B performance (`08` §3.2), attribution, allocation audit.
- FPE-9.2 Final audit: Q31 unchanged (diff review), default Q31 (G9), no
  downstream change, no manifest/dependency change.
- FPE-9.3 Update risk register (`10`) with measured outcomes; close resolved
  risks.
- FPE-9.4 Independent architecture + numerical + validation sign-off (G8);
  owner acceptance.

**Dependencies**: M8. **Acceptance**: G6 within budget (or owner-approved
finding); G8/G9. **Risks**: R-13 (performance), R-14 (memory). **Review**:
owner + independent.

## Cross-milestone rules

- Every task: focused diff, no unrelated refactoring, no Q31 behaviour change,
  no default change, no commit/push without owner instruction (`11`).
- Every milestone ends with its review gate before the next begins.
- A task that needs an architectural decision **stops and escalates** — it does
  not invent one (`11` §3).
- Stage-2 items (pure-float headroom, float-recomputed factors, SIMD, model
  changes) are never started in M0–M9.