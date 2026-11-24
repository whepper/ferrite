# FPE Post-Checkpoint Assessment

**Assessment date:** 21 November 2026
**Assessor role:** Independent senior audio-codec architect / psychoacoustics
researcher / Rust systems engineer / technical auditor (assessment-only; no
implementation authority).
**Repository:** `whepper/ferrite`, branch `main`
**Published checkpoint:** `eb2896923ec788727975268aca0beb27cba51760`
**Verification host:** `aarch64-apple-darwin` (Apple Silicon), rustc/cargo 1.99.1
(`58cf8f9c3` / `5f94df478`) — the recorded reference platform.

**Authority note.** This document is an independent reassessment, not an owner
decision. It verifies claims against source, tests and recorded evidence; it
does not promote any conditional acceptance, waive any blocked requirement, or
authorize implementation. Where evidence is insufficient it says so. It
cross-references the existing FPE package (`docs/fpe/`) rather than restating
it. Companion documents:

- [findings-register.md](findings-register.md) — consolidated findings, status,
  evidence, blockers, resolution criteria.
- [validation-roadmap.md](validation-roadmap.md) — required validation, methods,
  evidence, acceptance gates.
- [implementation-roadmap.md](implementation-roadmap.md) — proposed phases.
- [next-phase-recommendation.md](next-phase-recommendation.md) — immediate next
  phase and readiness decision.

---

## 1. Executive summary

The checkpoint is a **real, compiling, well-tested implementation**, not a
paper plan. The FPE engine seams, FPE-owned state/orchestration, the FPE-3.1
float rescale-energy kernel, the R1 derived energy envelope, the R2.0/R2.1
IS/MS decision-safety harness and the G-1 probe-corpus adjudication are all
present in source and pass on this host. The Q31 engine remains the default and
its byte/hash baseline is unchanged. The documentation is unusually rigorous
and the independent-review chain (Qwen adversarial audit, DeepSeek numerical
reviews, G-1/G-1a/G-1b reconciliations, owner decision) is substantive and
self-correcting.

However, the project is **not at a clean milestone boundary**. The published
checkpoint sits *inside* M3: only FPE-3.1 (rescale energy) is floated; FPE-3.2
(TNS energy recompute) and FPE-3.3 (A-4 experiment) are not done; M4–M9 are not
started. FPE today is a **float-energy hybrid**, not a float-native engine: one
value kernel is f64, every other value kernel still delegates to Q31, and the
integer representation lane remains authoritative (ADR-0005). The acceptance
chain has **real evidence-integrity gaps** (a missing analytical report that is
the cited source of the only MS bound `B_closed`; a referenced-but-absent G-1c
cleanup record; an uncommitted byte-level baseline; R2.1-B aggregate counts
that are printed but not asserted) and **real validation gaps** (no PE/ATS or
downstream decision-safety evidence; no perceptual evidence of any kind; Tier-B
C parity blocked on an unavailable pinned reference).

Nine findings are **conditionally accepted** by the owner (six persistent P
findings whose termination beyond five frames is unproven; three one-LSB IS
admission/direction G findings F-20/F-22/F-26 with no per-finding PE/ATS or
perceptual evidence). These conditions are **not** satisfied by anything in the
checkpoint and must remain visible.

**Readiness decision: CONDITIONALLY READY.** A bounded, primarily
evidence-integrity and decision-safety-scoping phase can begin once the owner
confirms the recovery path for the missing records and authorizes the small
test-only pinning/baseline work. **M3 acceptance, any FPE-default change, and
Stage-1 closure are NOT READY** (Tier-B, PE/ATS and perceptual evidence
outstanding). The single most valuable next action is to **recover and commit
(or permanently link) the missing R2.1-B analytical report and G-1c record and
establish the committed byte-level baseline**, because the entire acceptance
chain rests on them. See [next-phase-recommendation.md](next-phase-recommendation.md).

---

## 2. Verified current state

### 2.1 Repository and commits (verified)

`git rev-parse HEAD` = `eb2896923ec788727975268aca0beb27cba51760`; branch
`main`; working tree clean except untracked `.DS_Store` files (left untouched).
The five published commits, inspected via `git show --stat`:

| Commit | Subject | Nature | Size |
|---|---|---|---|
| `8db94f1` | M1/M2 foundation docs, ADRs, verification record | docs only | 21 files, +4098 |
| `fe73705` | engine seams + FPE-3.1 + R1 + R2 + G-1 harness | code + tests | 23 files, +12006/−35 |
| `ba79543` | R2.0/R2.1 implementation notes + reviews | docs only | 6 files, +2674 |
| `72ad659` | G-1 probe-corpus validation + reconciliation | docs only | 5 files, +2224 |
| `eb28969` | R2.1-B owner decision + acceptance record | docs only | 1 file, +187 |

**Structural observation (finding A-01/A-02).** `fe73705` bundles M1, M2,
FPE-3.1, R1, R2.0/A/B and G-1 code into a single commit. Its message states the
final tree is interdependent and "cannot be split into independently compiling
states without reconstructing history". Consequence: the **M2 bit-identical
pass-through gate (G0) is not re-runnable from the published tree** — the value
lane is already float, and the seam test
`fpe_selected_streams_encode_cleanly_and_influence_downstream`
(`stream_driver.rs:8221`) explicitly documents that "M3 (FPE-3.1) replaces the
M2 byte-identity gate". G0 evidence is historical (pre-squash). No retained
M1/M2 independent-review sign-off record exists in the package (the roadmap
requires architecture review at M1 and architecture+numerical review at M2).
This is an evidence/provenance gap, not a correctness defect.

### 2.2 What is implemented (verified against source)

- **Selection seam** (`config.rs:466-473`, `stream_driver.rs:748-762, 957-1009`):
  `#[doc(hidden)] PsyEngineKind { Q31 (default), Fpe }`, additive
  `with_psy_engine`/`psy_engine()`; allocate-selected-only state; single dispatch
  point in `frontend` (stereo + mono arms); downstream untouched. Default Q31
  asserted in every constructor path. Matches ADR-0001 / `06`.
- **FPE module tree** (`fpe/`): `mod.rs` (`FpeChannel`/`FpeScratch`/`FpeTables`,
  `process_pair_reuse`), `orchestrate.rs` (723-line stage-for-stage mirror of
  `psy.rs:509-1130`), `energy.rs` (FPE-3.1), `output.rs` (boundary + assembly +
  guards), `constants.rs` (exact `/2^31` derivations, currently unused by
  production kernels — `#![allow(dead_code)]`), `preecho.rs`/`thresholds.rs`
  (doc-only skeletons; delegation happens in orchestrate). Matches `04`.
- **FPE-owned state** (`fpe/mod.rs:75-133`): `FpeChannel` owns `PreEchoState`
  (Q31 stored-int type in M2/M3), `TnsChannel` pair, `ms_ld [[f64;64];8]`;
  init mirrors `PsyChannel::new`; test-only inspectors. No shared mutable Q31
  state. Independence proven by canary/tag tests (`decision.rs`, `is_ms.rs`).
- **FPE-3.1 float energy** (`fpe/energy.rs:94-141`): physical f64 spectrum,
  `E_f = Σx²·2^(−2(m−f))`, rescale Ld with the §6.8 guard + unconditional
  `finalShift` lift. Boundary `fraction_to_stored` (`output.rs:137-151`):
  round-to-nearest, saturating, **release-active** finiteness `assert!`. Active
  for rescale energies only; every other value kernel delegates to Q31.
- **Shared value kernels / Q31 integration** (verified call sites in
  `orchestrate.rs`): `energy::{map_spectrum_to_q31_into, max_scale_spec_into,
  check_band_energy_into, max_nrg_for_shift, rescale_shifts, calc_ms_energies}`,
  `q31::shift_int`, `thresholds::init_threshold`, `psy::{lowpass_and_zero_check,
  tns_encode_channel, overflow_rescale, advance_thresholds, finish_channel}`,
  `tns::{detect, synchronize_pair, encode}`. Integer raw sums feed **only** the
  maxNrg/finalShift representation decision (ADR-0005 one-way dependency).
  **MS energies are still pure Q31** (computed from the remapped Q31 spectrum,
  `orchestrate.rs:602-650`), not from float rescale energy.
- **R1 envelope** (`tests/envelope.rs`): the approved `tol_E(b) = max(32, Q_b +
  T_b + 1.5) + E_ref/500` with an exact supported-domain detector (i64
  accumulation, i128 re-shift, `scale0 ≥ 32` rejection, panic-free across the
  i32 `maxScale` domain). Matches `05` §9.2 / ADR-0004 amendment.
- **R1 audit** (`tests/audit.rs`): collect-all failures; three independent
  identity gates (Q31 recomputation bit-exact, FPE recomputation bit-exact,
  independent Ld `<1e-15`) + decomposition term bounds; Class A/B sentinel
  classification by stored state; `assert_clean` fails on **any** unsupported
  comparison (unsupported ≠ pass, `audit.rs:1552-1629`); release-active
  finite-tolerance asserts. Matches `07` §5.1.
- **R2 decision-safety** (`tests/decision.rs` 19 tests; `tests/is_ms.rs` 31
  tests; `tests/f6_tns.rs` 2 tests): ownership/anti-reuse canaries, frame
  ledger, live dual-engine frontend harness; production-callsite IS/MS
  capture/replay/trace with I1–I10 source-derived margins and `B_closed` MS
  bound; 12 acceptance gates; honest Tier-B BLOCKED placeholder
  (`is_ms.rs:2626-2651`). Matches R2.0/R2.1-A/R2.1-B notes.
- **G-1 probe corpus** (`tests/probe_envelope.rs` 13 tests): six probes × 14
  frames, retained `G1-BAND`/`G1-WIN`/`G1-MAX`/`G1-CLASSB` rows, hard-asserted
  sentinel counts (74/0/221/3/0/3/0/0/78/0) and F-6=0, F-24/Class-B/scale-edge
  deep dives, F-19≡F-21 identity. Matches G-1 report + G-1a/G-1b.
- **Production seams are additive only** (verified diffs): `psy.rs`/
  `intensity.rs` changes are `pub(crate)` visibility bumps + `#[cfg(test)]`
  accessors; `stream_driver.rs` R2.1-B diagnostics (`is_book_values`,
  `is_mask_values`) are `#[cfg(test)]`-gated (`stream_driver.rs:1320-1324,
  551-559`). No Q31 numerical change.

### 2.3 Tests executed for this assessment (independent rerun)

All commands from `rust/`, toolchain 1.99.1, on the checkpoint tree:

| Suite | Command | Result |
|---|---|---|
| FPE debug | `cargo test --locked --lib 'fpe::'` | **125 passed, 0 failed** (124 substantive + 1 Tier-B BLOCKED placeholder) |
| FPE release | `cargo test --locked --release --lib 'fpe::'` | **125 passed, 0 failed** |
| Q31 baseline | `cargo test --locked --lib production_baseline` | **2 passed** (hashes unchanged) |
| Full lib (no skips) | `cargo test --locked --lib` | 497 passed, **11 failed**, 2 ignored |
| CI Tier-A (15 skips) | `cargo test --locked --lib -- --skip …` (ci.yml set) | **493 passed, 0 failed**, 2 ignored, 15 filtered |
| Public API | `cargo test --locked --test production_api` | **6 passed** |
| Doctests | `cargo test --locked --doc -p ferrite-aac` | **19 passed** |
| Formatting | `cargo fmt -- --check` | clean |
| Clippy | `cargo clippy --locked --lib --tests` | 185 warnings (informational CI job; 2 in `fpe/orchestrate.rs`) |
| Licensing | `bash rust/tools/check-licensing.sh` | OK (86 files carry TPMV notice) |
| Whitespace | `git diff --check` | clean |
| Package | `cargo package --locked -p ferrite-aac --list` | NOTICE + LICENSE-FDK-AAC present |

The **11 full-lib failures are all artifact-gated Tier-B tests that fail
loudly** when the C reference / oracle dumps / decoder are absent (e.g.
`m5a_decode_check` → `AAC_DEC_BIN … NotPresent`; `phase1_*` → "reference
validation artifact missing"). This is the designed behaviour (`07` §7.1,
Tier-B SKIP ≠ PASS), not a regression. The 2 ignored are opt-in
(`aac_perf_bench`, `m511_profile_loop`).

**Inventory reconciliation (verified).** R2.1-A recorded 81 FPE tests / 449
Tier-A. The checkpoint adds `is_ms` (31) + `probe_envelope` (13) = 44, giving
125 FPE / 493 Tier-A — exactly the observed counts. Per-file `#[test]` counts
match the notes (`decision.rs` 19, `is_ms.rs` 31, `probe_envelope.rs` 13,
`f6_tns.rs` 2). The recorded results are **reproduced**, not merely cited.

### 2.4 Status classification (the six required categories)

| Category | Items |
|---|---|
| **1. Designed, not implemented** | FPE-3.2 (float TNS recompute), FPE-3.3/A-4 (f32-vs-f64 experiment), M4 (threshold init/clip/spreading), M5 (PCM floor), M6 (float pre-echo), M7 (float MS/grouping/TNS-LFE integration), M8 (full three-way + public API), M9 (performance/final audit); `FpeError` graceful skip-with-note; production `!short_ch` TNS guard; Rust resynthesis of `fpe-short-burst-quiet`/`fpe-tns-active-short`; A-5 cross-arch Ld measurement; G-2/G-3 harness fixes; R2.2 (PE/ATS/S2/S4/S6 decision safety); perceptual validation; Stage-2 items |
| **2. Implemented, not independently verified** | M2/G0 bit-identity (historical; not re-runnable, A-01); R2.1-B per-finding operands/margins/trace chains (removed dump harness; OWN evidence class, A-05); DeepSeek R1-over-R2.1-B-corpus run (removed scaffold); M1/M2 review sign-off (no retained record, A-02) |
| **3. Implemented and verified** | Selection seam; FPE state/orchestration; FPE-3.1 rescale energy + boundary; R1 envelope + audit (2662-frame corpus, 0 fail/unsupported); R2.0/R2.1-A/R2.1-B IS/MS harness (381 frames, structural gates); G-1 probe adjudication (84 frames, 56 rows); Q31 baseline non-regression; licensing hygiene — all rerun green on this host |
| **4. Conditionally accepted** | F-07/F-09/F-10/F-15/F-17/F-18 (P: persistence beyond 5 frames unproven); F-20/F-22/F-26 (G: one-LSB IS admission/direction, no per-finding PE/ATS or perceptual evidence). Conditions **unsatisfied** at checkpoint |
| **5. Fully accepted** | F-19/F-21/F-23/F-24/F-25 (scale-only, same IS book/phase); G-1 as PASS WITH FINDINGS for its limited energy-envelope scope (28 findings F-03…F-30); F-01/F-02 accepted as documented local Cause-B findings with the unsupported sentinel domain retained (not bound-validated, not G-1-adjudicated) |
| **6. Deferred / out of scope** | HE-AAC/PNS/tonality/ATH (absent, stay absent); FPE default (out of Stage 1); pure-float representation lane, float-recomputed spreading/minSnr, SIMD, model changes (Stage 2, separate charter); downstream PE/S2/S4/S6 rewrites; tonality estimation |

---

## 3. Architecture assessment

The architecture is a **sound migration foundation** and several decisions are
genuinely well-judged. I challenge specific points below rather than endorse
wholesale.

### 3.1 What is sound (verified)

- **Orchestration/value-kernel separation is real and enforced.** FPE owns its
  state and scratch; Q31 modules are read-only dependencies (visibility bumps
  only); a single enum dispatch; the representation lane is one-way. The
  canary/tag/crossed-input tests (`decision.rs`, `is_ms.rs`) actively prove
  independence rather than assuming it. This satisfies constraints 3/4
  (Q31 reference baseline; independently identifiable/testable).
- **The fraction domain (`v_f = v_st/2^31`) is derived, not assumed** (`05` §1),
  and matches the existing f64 Ld lane and `SfbConfig` accessors. It eliminates
  Q-format scaffolding and avoids the rev-1 domain-mixing defect. f64 choice
  (ADR-0002) is well-argued (53-bit ≫ Q31 32-bit; denormal safety; no
  narrow/re-widen).
- **The incremental bit-exact→envelope strategy works.** M2 pass-through proved
  wiring; FPE-3.1 introduced exactly one floated kernel; the derived envelope
  isolates the difference to input-mapping + truncation + boundary terms with
  independent identity gates. This is the tightest practical differential
  surface and it localizes defects.
- **The envelope is mathematically grounded and conservatively guarded.** The
  supported-domain detector rejects saturation/wrap/overflow rather than
  silently applying an invalid bound; unsupported ≠ pass; tolerances are
  data-dependent and capped. This is unusually rigorous and resists the
  "widen tolerance to pass" failure mode.
- **Decision-safety replay avoids circular validation.** The IS/MS harness
  calls the *production* decision functions on cloned operands and asserts
  per-frame replay equality; margins are annotation-only and never feed
  encoding. There is no second re-implemented oracle masquerading as ground
  truth.

### 3.2 Challenges and risks

**(a) The seams are coarser than the per-kernel picture (A-13).** `04` §6
depicts per-kernel float replacement, but FPE actually delegates whole Q31
*composite stages* — `tns_encode_channel`, `advance_thresholds`,
`finish_channel`. These cannot be "partially floated": to float thresholds
(M4), PCM floor (M5) or pre-echo (M6), FPE must mirror the composite internals
with float kernels, because the Q31 composite is monolithic and Q31 refactoring
is forbidden. ADR-0001 calls the resulting duplication "slight … orchestration
structure"; that **understates** it — it is full composite-stage duplication
that grows with each floated kernel and raises orchestration-drift risk (R-02).
The charter has an escape hatch ("additive, behaviour-preserving seams … if any
prove necessary", `00` §4). The roadmap should force an explicit owner decision
at M4 entry: (i) FPE-side composite mirrors (charter-default, more duplication)
vs (ii) minimal additive, baseline-gated extraction of pure Q31 kernels from the
composites (less duplication, but a Q31 edit requiring approval). This is the
single largest source of avoidable migration complexity.

**(b) "Float-native" is not yet what the engine is (ADR-0005 tension).** The
representation lane stays integer-authoritative and the internal domain is
defined *relative to Q31 storage*. Each floated kernel adds a `·2^31` boundary
quantization point. This is correct for Stage-1 fidelity, but the long-term
objective (a genuinely float-native engine) requires a Stage-2 charter to remove
the integer scaffolding and the boundary (physical-domain energies, float-native
thresholds, downstream float or a re-quantization convention). The architecture
*permits* this (the lane is isolated and one-way) but does not *progress* toward
it automatically. The roadmap must keep Stage 1 faithful and plan Stage 2
explicitly; it must not pretend the current hybrid is the end state.

**(c) MS energies are still Q31; `B_closed` is validated only for the current
operand set (A-18).** IS decision inputs (`e_l/e_r` stored + `e_ld`) are
float-derived, but MS mid/side energies come from the remapped Q31 spectrum. The
F-01 MS flip arises via the float-derived energy-Ld operands, not MS energies.
When M7 floats MS energies, a **new divergence source** enters the MS margin and
`B_closed` must be re-derived/re-validated for the new operand set. The roadmap
must treat M7 as re-opening decision-safety, not as a pure value swap.

**(d) Error handling is a loud panic, not graceful (A-20).** The only numerical
guard is the release-active `assert!` in `fraction_to_stored`. A non-finite
fraction (unreachable for finite MDCT/PCM input today) would panic the encoder
in release. The intended `FpeError`/skip-with-note path (`06` §6) was deferred to
"FPE-3.1R2" but R2.1-B scoped IS/MS only and did **not** implement it. It remains
open and needs explicit roadmap placement before FPE is ever default.

**(e) State ownership is independent but pre-echo is still Q31-typed.** M6 will
replace the delegated `PreEchoState` with a fraction `FpePreEchoState`; until
then the temporal history is Q31 arithmetic downstream of float energy (audited
within `q31_tol`). This is fine, but the M6 state transition is the highest-risk
temporal change (R-07) and must keep the per-frame state-comparison gate.

**(f) Numerical precision/rounding is well controlled for the floated kernel.**
No FMA contraction (`mul_add`/`fast_math`/`target_feature` absent in `fpe/`),
fixed operation order, exact power-of-two scaling, round-to-nearest boundary
matching `map_spectrum_to_q31_into`. `log2` accuracy is correctly *not* claimed
correctly-rounded; the Ld bound is measured — but only on ARM64 so far (A-5/A-11).

### 3.3 Verdict on the §2B questions

| Question | Verdict |
|---|---|
| FPE orchestration vs Q31 value-kernel separation appropriate? | **Yes**, enforced and tested; but seam granularity is coarser than documented (3.2a) |
| Seams support eventual Q31-kernel replacement? | **Yes for value kernels**; composite-stage duplication is the cost (3.2a); representation-lane replacement is Stage 2 (3.2b) |
| State ownership/transitions sufficiently independent? | **Yes** (canary-proven); pre-echo float transition still pending (M6) |
| Float energy correctly isolated? | **Yes** — one kernel, one boundary, one-way lane; MS still Q31 (3.2c) |
| Precision/rounding appropriately controlled? | **Yes** for the floated kernel; cross-arch Ld bound unmeasured (A-5) |
| Architecture permits meaningful independent validation? | **Yes** — same-binary differential + production-replay; strongest aspect |
| Decisions creating unnecessary migration complexity? | **Composite seams (3.2a)** and **boundary-per-kernel anchoring (3.2b)**; both manageable with explicit decisions |

---

## 4. Implementation completeness

Per-kernel value-lane status (verified in `orchestrate.rs`):

| Kernel | Status | Path |
|---|---|---|
| Rescale band energy (long+short) | **Float (FPE-3.1)** | `fpe/energy.rs` → `fraction_to_stored` |
| Rescale energy Ld | **Float (§6.8)** | `fpe/energy.rs` |
| Representation lane (maxScale/shifts/mdctScale) | Integer-exact, reused | `energy.rs` helpers (ADR-0005) |
| Threshold init | Q31 kernel on float-derived stored energy | `thresholds::init_threshold` |
| TNS detect/sync/encode | Reused unchanged | `tns::*`, `psy::tns_encode_channel` |
| TNS energy recompute | **Q31 (FPE-3.2 pending)** | `energy::band_energy_{short,long}_into` |
| Overflow rescale | Reused | `psy::overflow_rescale` |
| Clip / spreading / PCM floor | **Q31 (M4/M5 pending)** | `psy::advance_thresholds` |
| Pre-echo temporal control | **Q31 state (M6 pending)** | `PreEchoState` via `advance_thresholds` |
| Spread energy | Q31 on float-derived energy | `advance_thresholds` |
| MS energies + Ld history | **Q31 (M7 pending)** | `energy::calc_ms_energies` |
| Grouping / output assembly | Reused Q31 assembler | `psy::finish_channel` |
| Boundary quantization | Active for rescale energy only | `fpe/output.rs` |

Milestone position: **M0 done** (verification; A-3 partial), **M1/M2 done**
(historical G0), **M3 partial** (FPE-3.1 done; 3.2/3.3 pending; R1/R2/G-1
validation done for the floated kernel), **M4–M9 not started**. The `09`
milestone-overview table still presents M3 as the next unstarted milestone;
that is stale (A-15).

---

## 5. Outstanding findings (summary)

Full detail in [findings-register.md](findings-register.md). Headlines:

- **Conditional acceptances remain conditional.** F-07/09/10/15/17/18 (P) and
  F-20/22/26 (G) are accepted *only* under explicit limitations (termination
  unproven; no per-finding PE/ATS or perceptual evidence). Nothing in the
  checkpoint satisfies those conditions. Do not promote them.
- **Evidence-integrity gaps (A-01…A-05, OD items):** missing R2.1-B analytical
  report (the cited `B_closed` derivation source); referenced-but-absent G-1c
  record (FR-1…FR-4 "addressed by G-1c" with no record); uncommitted byte-level
  baseline (FR-6); R2.1-B aggregate counts printed-not-asserted (A-03);
  per-finding operands not retained (A-05); M2/G0 not re-runnable (A-01); M1/M2
  review records absent (A-02).
- **Validation gaps (A-08…A-12):** no PE/ATS/downstream decision safety (R2.2);
  no perceptual evidence; Tier-B C parity blocked (pinned `f14ad40a` unavailable,
  IS/MS oracle dumps + decoder missing); A-5 cross-arch Ld unmeasured; A-4
  f32-vs-f64 experiment outstanding.
- **Coverage limits (A-06/A-07/A-17):** FPE-selected integration is 7 fixtures,
  structural-only (vs `06` §9's 19+4 + decode); decision-safety corpus is
  stereo-only, VBR-5 operating point, no real music, 12-frame probes; two R2.0
  fixtures exist only in Python.
- **Residual documentation-precision items (A-15/A-22):** owner decision and
  owner review still say "16 materials" (harness defines **15**, verified
  `is_ms.rs:2204`; DeepSeek §6.1 flagged this); implementation note §5 "capped at
  32 frames" vs actual 49/49/49/32/26/32/32/32 (Qwen R-8); README/charter status
  headers stale ("planning only … nothing committed").
- **F-6 clarification (A-14):** short-window `tnsSpecShift != 0` is
  **structurally unreachable** in current production (`psy.rs:1190-1213` sets it
  only in the long branch; D-022 forces CPE sequence agreement). The
  instrumented watch (0 observations, pinned) is adequate; the `07` §5.2
  constructed-test requirement can be closed by a documented unreachability
  argument (owner decision).

---

## 6. Validation gaps (summary)

Full detail in [validation-roadmap.md](validation-roadmap.md).

- **Numerical:** strong for rescale energy (R1 corpus + G-1, identity gates,
  supported-domain guards). Outstanding: A-4 (f32/f64 empirical), A-5
  (cross-arch Ld bound), per-kernel envelopes for M4–M7, threshold propagation
  bounds, cross-frame stability beyond the current corpus, F-6 closure.
- **Decision safety:** IS/MS done with documented limits. **PE/ATS/S2/S4/S6/S8
  not validated (R2.2)** — the energy envelope does not bound downstream PE or
  rate-control evolution; this is the largest gap before FPE-3.1 acceptance.
  P-cascade termination unproven beyond 5 frames; corpus lacks real music and
  other operating points.
- **Perceptual:** **none.** No listening tests, no objective metrics on FPE
  streams, no decode validation (Tier-B blocked). Numerical fidelity is a proxy,
  not perceptual equivalence (constraint 5). Perceptual evidence is the gate for
  closing F-20/22/26 conditional acceptances and for any future default change —
  not a Stage-1 numerical-fidelity gate (charter keeps objective metrics
  supplementary).
- **Engineering:** determinism controls present; cross-platform implicit via
  x86-64 CI but no measured bound; coverage gaps above; performance (G6) not
  measured for FPE (M9); state integrity gated per-frame; SIMD deferred (psy
  ≈2.4% self time).

---

## 7. Overall readiness assessment

**CONDITIONALLY READY** for a bounded next phase; **NOT READY** for M3
acceptance, FPE default, or Stage-1 closure.

Rationale:
1. The implementation is real and green; the numerical energy validation is
   trustworthy *for the floated kernel*. Further bounded work can proceed.
2. But the acceptance chain rests on artifacts that are missing or unretained
   (analytical report, G-1c record, byte baseline, per-finding operands). A
   senior auditor cannot recommend building further acceptance on that footing
   without first closing the provenance gap. This is cheap and high-value.
3. The decision-safety chain stops at IS/MS; PE/ATS and perceptual evidence are
   absent. FPE-3.1 cannot be accepted (per `07` §5.2 and the owner decision)
   until R2 completes and independent review signs off; Tier-B C parity is
   blocked and not waived.
4. The nine conditional acceptances must remain explicit until their conditions
   are satisfied; nothing here satisfies them.

**Prerequisites to begin the next phase** (owner actions + bounded test-only
work): confirm the recovery path for the analytical report and G-1c record;
authorize the committed byte-level baseline and R2.1-B aggregate pinning;
scope R2.2. **Single most valuable next action:** recover/commit (or permanently
link) the missing R2.1-B analytical report + G-1c record and establish the
committed byte-level baseline. See
[next-phase-recommendation.md](next-phase-recommendation.md).

**Model delegation:** strong reasoning model for architecture/seam decisions,
envelope derivations, perceptual-methodology design, and independent review;
faster implementation model for mechanical test pinning, fixture resynthesis,
spec-exact kernel translation, and documentation formatting. Details in the
implementation roadmap.

---

*Assessment-only. No production source, manifest, lockfile, test, fixture, ADR
or architecture decision was modified; nothing was committed or pushed; Git
history was not rewritten; untracked files were left untouched. All test runs
were read-only executions of existing suites.*
