# FPE M4 — Architecture and Implementation Plan (Threshold Initialization, Clipping, Spreading)

**Date:** 28 November 2026.
**Status:** architecture and implementation blueprint — **planning only.** This
document authorizes nothing; M4 requires a separate owner authorization and
the owner decisions listed in §2.4. No production source, test, fixture,
manifest, dependency, evidence bundle, ADR, threshold, tolerance, envelope,
baseline pin, or schema was modified by this document. No file was staged,
committed, pushed, reset, rebased, stashed or cleaned; all pre-existing
working-tree modifications are preserved.
**Baseline:** `main` at HEAD `189e6a79b78a84dcc5be88eaca5ffaba24097acc`
(= `189e6a7 feat(fpe): add R2.2 capture and replay infrastructure`), with the
pre-existing uncommitted M3/R2.2 working-tree set (see §2.3).
**Governing sources:** the FPE planning package
(`00`–`12`, ADR-0001…0005), the M3 records
([`assessment/M3-owner-acceptance.md`](assessment/M3-owner-acceptance.md),
[`assessment/M3-exit-assessment.md`](assessment/M3-exit-assessment.md)), the
assessment roadmaps
([`assessment/implementation-roadmap.md`](assessment/implementation-roadmap.md)
Phase 3,
[`assessment/validation-roadmap.md`](assessment/validation-roadmap.md) N-4/N-5/N-6/N-8),
the findings registers ([`assessment/findings-register.md`](assessment/findings-register.md)
§A-13,
[`assessment/R2.2-findings-register.md`](assessment/R2.2-findings-register.md)),
and the R2.2 decision-safety design
([`assessment/R2.2-downstream-decision-safety-design.md`](assessment/R2.2-downstream-decision-safety-design.md)).

---

## 1. Executive architecture

M4 floats the **threshold front-end** of the psychoacoustic value lane: the
three genuinely float-native kernels **threshold initialization**, **clipping
(cip cap)**, and **spectral spreading** (threshold spreading and spread-energy
spreading), per `05` §6.3/§6.4. Tonality is absent and stays absent (FPE-4.3;
PNS-gated upstream, `03` §4.2).

The M4 shape follows the established M2/M3 pattern, extended by one composite
seam:

1. **FPE owns its orchestration.** The FPE path already runs its own explicit
   per-pair mirror (`fpe/orchestrate.rs`). M4 adds the one composite mirror
   the milestone requires: `fpe::thresholds::advance_thresholds` — an
   FPE-owned mirror of `psy::advance_thresholds` (`psy.rs:1256-1292`) — because
   the M4 float kernels sit **inside** that composite (clip and both
   spreadings), interleaved with the PCM floor (M5) and pre-echo control (M6)
   which remain Q31-delegated until their milestones. This is the A-13
   resolution recommended in §3.
2. **Each kernel is replaced at its own site, one stage at a time.** Threshold
   init is replaced at the rescale site (it is computed there, before the TNS
   block); clip and the two spreadings are replaced inside the advance-stage
   mirror. Every stage is a focused diff gated by the Q31↔FPE differential
   harness; no stage combines areas.
3. **Mixed delegation is explicit and per-stage.** After M4 the advance-stage
   mirror is: float clip → float threshold spreading → §7 boundary →
   Q31-delegated `apply_pcm_floor` → Q31-delegated `PreEchoState` control
   (STOP-before/START-after resets preserved) → float spread-energy copy +
   spreading → §7 boundary. M5 replaces the PCM-floor call site; M6 replaces
   the state type. The mirror's loop/branch structure is written once at the
   seam stage (M4.2) with zero value change, so later milestones swap single
   call sites.
4. **The TNS block is untouched.** `psy::tns_encode_channel`
   (`psy.rs:1135`) and `psy::overflow_rescale` remain the delegated composites
   with the M3 mirror-then-replace discipline. Their threshold mutations
   (long-window quartering `>> 2`; overflow `>> (tnsSpecShift<<1)`) continue
   to operate on FPE's **stored** threshold scratch in both engines, so M4
   adds **no float lane through the TNS composite** and no new drift surface.
   M4 introduces no threshold-lane continuity across the TNS block: the float
   threshold lane restarts from the exact fraction image of the stored
   threshold at the advance stage (§5.1.4 documents the ≤0.5-LSB cost of this
   boundary and the rejected alternative).
5. **Output assembly stays shared.** `psy::finish_channel` (`psy.rs:1434`) —
   grouping, Ld vectors, `nm1` readout — is unchanged; `fpe::output::
   assemble_channel` remains its pass-through with invariant guards. The
   `05` §7 boundary (`fraction_to_stored`, round-to-nearest, saturating) is
   the only quantization each M4 kernel adds.
6. **Representation lane untouched (ADR-0005).** `mdctScale`, `finalShift`,
   `tnsShift`/`tnsSpecShift`, `maxScale`, `nrgScaling` remain integer-exact
   and shared. The M4 value lane never feeds back into them.
7. **Q31 stays the reference and default.** No Q31 production file changes.
   `PsyEngineKind::default()` remains Q31; FPE stays opt-in.
8. **f64 only** (ADR-0002; A-4 owner decision): no f32 production kernels, no
   SIMD, no tuning changes — all mask/clip/PCM constants and spreading
   factors are consumed from the existing tables and Q31 literals via the
   exact `/2^31` images already gated by unit tests in `fpe/constants.rs`
   (M1 scaffolding activates at M4).

### 1.1 What "genuinely float-native" means for M4 (and what it does not)

For each of the three areas the **Q31 calculation dependency is removed, not
wrapped**:

- **Init:** the Q31 `fmult(E_st, C_RATIO_Q31)` floor-multiply on the stored
  energy is replaced by the exact f64 product `T_f = E_f · C_RATIO_F` on the
  M3 float energy lane (`ws.ef_l`), quantized once at the §7 boundary.
- **Clip:** the Q31 integer shift/floor clip cap is replaced by the exact
  power-of-two fraction cap `clip_f = CLIP_f · 2^(4−2m)` (branch structure and
  the uncapped case preserved).
- **Spreading:** the Q31 `fmult`-floor two-pass envelope is replaced by the
  f64 two-pass envelope with the **tabulated** f64 factors
  (`SfbConfig::mask_factors()`/`mask_factors_spr()`), never recomputed.

What remains Q31 in M4 is Q31 **by milestone design**, not by omission: the
PCM floor (M5) and the pre-echo control state (M6) — both delegated inside the
mirror — plus everything downstream of the psy boundary. The §7 boundary
round between float stages is the established FPE output contract
(ADR-0004), not a Q31 dependency.

---

## 2. M4 entry-condition assessment

### 2.1 M2 prerequisites — satisfied

M2 (bit-identical pass-through, complete output + all frame-surviving state
per frame over multi-frame sequences) was implemented, independently reviewed
(GLM 5.3 Flash, 24 November 2026, PASS WITH FINDINGS) and owner-accepted
(P0-A owner disposition record, Amendment M2 acceptance). A-02 is closed for
M2; A-01 (G0 historical non-rerunnability) remains a standing limitation and
is not an M4 entry requirement.

### 2.2 M3 acceptance and remaining limitations — satisfied with documented limitations

FPE M3 is **ACCEPTED / COMPLETE WITH DOCUMENTED LIMITATIONS** (28 November
2026). M3 delivered the float rescale band energy (FPE-3.1, owner-approved
derived envelope `05` §9.2), the float TNS energy recompute (FPE-3.2,
limitations F1–F6), and the A-4 f32-vs-f64 experiment (f64 retained). The
current-tree audit reports zero energy/value/Ld/identity failures with
recorded maxima; Q31 production baseline 2/2; `PsyEngineKind::default() ==
Q31`.

Limitations that travel into M4 unchanged (none blocks M4 entry; all must be
respected by the M4 plan):

1. **Seven preserved FPE pins** (six `r21b_p0b_*` plus
   `probe_envelope::g1_probe_corpus_aggregate`) — pinned byte-aggregates of
   the FPE-3.2 divergence state. **M4 will change the values these pins
   observe** (thresholds/spread are new divergence carriers). No rebaseline
   is authorized; pin handling is owner decision **D4** (§2.4).
2. **Eleven Tier-B/environmental blockers** — blocked, not waived; Tier-B
   C parity remains M8/Stage-1 work. The `09` M4 test list's "C↔FPE Tier-B"
   entries inherit the same deferral as M3-15.
3. **AC10-F1…F8, nine R2.1-B conditionals, 187 unjudged S4 flips** —
   preserved; M4 adds its own flip set, classified separately (§6, M4.8); no
   bundling, no harmlessness claims.
4. **FpeError/A-20** — decided for M3, implementation deferred. The
   release-active `assert!` in `fraction_to_stored` stands and is reused
   unchanged by the M4 kernels.
5. **Packaging/formatting limitations** (FPE-3.2 F6, `cargo fmt --check`
   drift) — repository hygiene items attached to the uncommitted tree; see
   D3.

### 2.3 R2.2 downstream decision-safety constraints — satisfied for M3; M4 must re-run its own leg

The R2.2 framework (AC-1…AC-10: production-callsite capture/replay of
PE/ATS/S2/S4/S6/accounting, margin classification with derivations, coverage
accounting, deterministic evidence bundles, independent review) is accepted
for the M3 boundary and its infrastructure is in-tree and reusable
(`fpe/tests/{decision,r22_capture,trace,ac9,ac9_artifact,evidence,is_ms}.rs`,
`docs/evidence/fpe-r22{,-ac9}/`). Binding constraints carried into M4:

- Downstream decision safety is **required before M4 kernel acceptance**
  (the FPE-3.1R2 pattern): a threshold/spread change is a new divergence
  carrier into PE, ATS, S2, S4, S6 and frame accounting.
- Margins are observational only; every margin needs an established
  derivation (`MarginDerivationRequired`, never invented).
- Every new flip must be classified (residue-explained vs structural) before
  acceptance; unsupported/unobserved arms are recorded explicitly and never
  count as passes.
- D-6/P-termination, OP-4 recurrence, OD-3 five-frame, A-09 perceptual work
  remain outside M4; the nine R2.1-B conditionals stay conditional.
- **Uncommitted-tree caveat:** the R2.2/E2/E3 evidence, the M3 implementation
  and this plan all sit on top of an uncommitted working tree (19 modified +
   30+ untracked entries at recording). Any M4 evidence bundle must record
  the tree state it was generated from; the SG-2 clean-checkout obligation
  (E6) remains outstanding.

### 2.4 Required M4 entry owner decisions

| # | Decision | Why it blocks/frames M4 | Recommendation |
|---|---|---|---|
| **D1** | **Authorize M4** and adopt this blueprint (or an amended form) as the M4 execution basis | M3 acceptance §4 explicitly does not authorize M4; `09`/Phase 3 require separate authorization | Authorize with this document as the basis; stage gates per §6 |
| **D2** | **A-13 seam decision** (§3): FPE-side composite mirror vs minimal additive baseline-gated Q31 kernel extraction | Open finding; controls how `advance_thresholds` is migrated and whether Q31 is touched at all | **FPE-side composite mirror** (§3.2) |
| **D3** | **Baseline/commit strategy**: whether to commit the accepted M3/R2.2 work before M4 implementation starts | M4 diffs stack on a large uncommitted set; mixed diffs would compromise focused-diff review, byte-baseline attribution (A-21/OD-6), packaging (FPE-3.2 F6) and CI (rustfmt) | Authorize a commit (or series) of the accepted M3/R2.2 work plus a formatting-only cleanup before M4.1 lands; M4 stages then commit as reviewed focused diffs |
| **D4** | **Pin-successor and characterization policy for M4 divergences**: how the seven preserved pins and the AC-7-style divergence characterization are carried forward when M4 shifts the observed divergence values | The pins observe FPE-3.2 divergence state; M4 adds threshold/spread divergence; "no rebaseline" collides with "values change" unless a successor policy is set | Record an M4 divergence-characterization campaign (AC-7-successor, §6 M4.8) whose aggregates are pinned as **new M4 pins**; keep the M3 pins and their evidence as historical records (unchanged, still failing against the M4 tree, explicitly annotated), rather than rebaselining or deleting them. Owner may instead order pin retirement with a preserved evidence record |

M4 must not start implementation before D1 and D2 are issued. D3 should
precede the first implementation stage; D4 must be issued before M4.8.

### 2.5 Migration boundaries around the three composite functions

| Function | Location | M4 disposition |
|---|---|---|
| `advance_thresholds` | `psy.rs:1256-1292` (Q31 composite) | **Unchanged** in Q31. FPE **stops calling it** and calls the new FPE-owned mirror `fpe::thresholds::advance_thresholds` (same stage order, mixed float/delegated value kernels). Must change: the FPE call sites in `fpe/orchestrate.rs`. Must remain shared: nothing — this composite is fully mirrored FPE-side; its Q31 original is untouched and remains the reference |
| `tns_encode_channel` | `psy.rs:1135-1213` (Q31 composite) | **Remains shared** (delegated + mirror-then-replace, exactly the accepted M3 discipline). M4 introduces no TNS change and no float threshold lane inside it. Revisit at M7 (TNS/LFE integration) |
| `overflow_rescale` | `psy.rs:1216-1246` | **Remains shared**; shifts FPE's stored thresholds by exact powers of two — identical operation in both engines |
| `finish_channel` | `psy.rs:1434-1551` | **Remains shared** (grouping, Ld vectors, `nm1` readout). Pass-through via `fpe::output::assemble_channel` unchanged |
| `apply_pcm_floor`, `PreEchoState::*` | `thresholds.rs:167-267` | **Remain shared/delegated inside the FPE mirror** (M5/M6 float them). FPE's `FpeChannel` keeps holding the Q31 `PreEchoState`; no state-type change in M4 |

**State owned by FPE in M4:** unchanged types, FPE-owned instances —
`PreEchoState` (Q31 type, delegated; float `FpePreEchoState` only in M6),
`TnsChannel` state/info (reused), `ms_ld` history (reused f64; M7 concern),
and `FpeScratch` (M4 adds **no new persistent scratch fields**: the mirror
uses per-window fixed-size local fraction buffers; §6 stage specs).

---

## 3. A-13 seam recommendation and required owner decision

### 3.1 The finding

A-13 (`findings-register.md:312`): FPE delegates monolithic Q31 composites
(`advance_thresholds`, `tns_encode_channel`, `finish_channel`); floating
thresholds/PCM/pre-echo requires FPE-side composite mirrors because Q31
refactor is forbidden; ADR-0001's "slight duplication" understates the
growing composite duplication and drift risk (R-02). Required resolution:
explicit owner decision at M4 entry — **FPE-side mirrors vs minimal additive
baseline-gated Q31 kernel extraction**.

### 3.2 Recommendation — FPE-side composite mirror (Option A)

**Adopt the FPE-side composite mirror for the threshold-advance family.**
Concretely: a new `advance_thresholds` function in
`rust/src/aac_enc/fpe/thresholds.rs` mirroring `psy.rs:1256-1292`
stage-for-stage (per-channel factors/headroom setup; per-window clip →
spreading → PCM floor → pre-echo with STOP-before/START-after and
`set_enabled(true)` → spread-energy copy + spreading; identical `active`
band indexing, LFE handling, `energyShift` accumulation), operating on FPE
state/scratch, with per-stage delegation:

- M4 float stages: clip cap, threshold spreading, spread-energy spreading
  (and init at its own rescale site).
- Delegated Q31 stages until their milestones: `apply_pcm_floor` (M5),
  `PreEchoState::{control, reset_for_transition, set_enabled}` (M6).

**Rationale:**

1. **Charter and ADR constraints decide it.** The charter's non-goals forbid
   refactoring the Q31 engine beyond additive, behaviour-preserving seams
   "explicitly specified in `04`/`06`, if any prove necessary"; `11` §1
   makes Q31 read-only with escalation on any perceived need. Kernel
   extraction (Option B) modifies Q31 production code — a new shared generic
   stage framework would also couple the engines' arithmetic, which
   ADR-0001 explicitly rejected ("the two engines' arithmetic differs;
   sharing a framework would couple them").
2. **The M3 precedent already chose this shape.** FPE-3.2 used
   mirror-then-replace over `tns_encode_channel` rather than TNS kernel
   extraction, and the orchestration itself is already an explicit mirror.
   The advance-stage mirror is the same decision applied one composite
   earlier in the chain.
3. **Drift risk (R-02) is bounded by the existing gates.** The mirror is
   introduced at stage M4.2 with **zero value change** and gated by
   byte-identity against the pre-mirror FPE state over the full differential
   corpus; from then on the per-frame complete-output + persistent-state
   differential localizes any divergence to the last swapped stage. The
   duplicated logic is ~40 lines of loop/branch orchestration calling the
   same kernels — not arithmetic.
4. **Option B has no numerical upside.** The two engines' stage arithmetic
   differs in type (i32 vs f64); extraction cannot share the value code, only
   the loop skeleton. The cost (Q31 production diff, re-review of the Q31
   engine, baseline re-verification) buys no accuracy and violates the
   Q31-intactness gates.

**Scope of the decision (bind it to the M4–M7 window):**

- `advance_thresholds` family: **FPE-side mirror** (M4; the mirror's PCM-floor
  and pre-echo call sites are the M5/M6 swap points).
- `tns_encode_channel`: **stays shared/delegated** with mirror-then-replace
  through M4–M6; the M7 TNS/LFE integration review revisits it with fresh
  evidence (the A-13 decision should say so explicitly rather than leaving it
  implied).
- `finish_channel`: **stays shared** — it is representation-lane/output
  assembly operating on stored ints (per `05` §6.7 quantize-then-regroup and
  §6.8 Ld conventions); no float value arithmetic belongs in it through M7.
- **Sub-decision recorded with it (no separate owner decision needed, but
  documented):** the M4 float threshold lane does **not** continue through
  the TNS block; it restarts at the advance stage from the exact fraction
  image of the stored threshold. The alternative (carry `T_f` through the TNS
  block by mirroring its two exact power-of-two threshold shifts) was
  rejected for M4: it duplicates composite mutation behaviour for a ≤0.5-LSB
  gain and adds a drift surface inside a delegated composite (§5.1.4).
- **Sub-decision recorded with it:** spread-energy input is the exact
  fraction image of the current **stored** energies (`e_st as f64 / 2^31`),
  not a re-derived float energy. The post-TNS authoritative energy is the
  M3-accepted stored value; re-deriving float energies would require
  extending the accepted FPE-3.2 kernel for a sub-LSB gain (§5.3.4). M7 may
  revisit both sub-decisions when TNS integration is reworked.

### 3.3 Required decision text (owner)

> **A-13 disposition:** For M4–M7, adopt FPE-side composite mirrors for the
> threshold-advance family; `tns_encode_channel` remains delegated with
> mirror-then-replace until the M7 review; `finish_channel` remains shared.
> Q31 kernel extraction is rejected. ADR-0001 is amended accordingly
> (§9.1, after authorization). The M4 sub-decisions (no float threshold lane
> through the TNS block; spread-energy input from the stored-energy fraction
> image) are recorded with this disposition.

---

## 4. Function-level migration map

All paths relative to `rust/src/aac_enc/`. "Q31" files are read-only for M4.

| # | Function / symbol | File:line (current) | M4 action |
|---|---|---|---|
| 1 | `init_threshold` (Q31) | `thresholds.rs:130` | **Unchanged.** FPE stops calling it |
| 2 | `init_threshold_f` (NEW, FPE-4.1a) | `fpe/thresholds.rs` | `T_f = E_f · C_RATIO_F`; `T_st = fraction_to_stored(T_f)`. Consumes `ws.ef_*` (M3 float energies) |
| 3 | `clip_cap` (Q31) | `thresholds.rs:137` | **Unchanged.** FPE stops calling it |
| 4 | `clip_cap_f` (NEW, FPE-4.1b) | `fpe/thresholds.rs` | `clipNrgShift = 2m−4`; headroom 6 short/0 long; `clip_f = CLIP_f·2^(4−2m)` (exact pow2) or `MAXVAL_F` (uncapped branch, unreachable bind); `T_f = min(T_f, clip_f)` |
| 5 | `spreading_max` (Q31) | `thresholds.rs:149` | **Unchanged.** FPE stops calling it |
| 6 | `spreading_max_f` (NEW, FPE-4.2) | `fpe/thresholds.rs` | Two-pass max envelope, f64 multiplies, **tabulated** factors; forward `mask_hi[i]`, backward `mask_lo[i]` |
| 7 | `advance_thresholds` (Q31 composite) | `psy.rs:1256-1292` | **Unchanged** (reference engine) |
| 8 | `fpe::thresholds::advance_thresholds` (NEW mirror, seam stage) | `fpe/thresholds.rs` | FPE-owned composite mirror; mixed float/delegated stages per §1 item 3; called from `fpe/orchestrate.rs` at both channel sites |
| 9 | `apply_pcm_floor` (Q31) | `thresholds.rs:167` | **Delegated inside the mirror** (M5 floats); `energyShift` accumulation lives in the mirror |
| 10 | `PreEchoState::{new, control, reset_for_transition, set_enabled}` | `thresholds.rs:176-267` | **Delegated inside the mirror** (M6 floats state); `FpeChannel.preecho` keeps the Q31 type |
| 11 | `tns_encode_channel` | `psy.rs:1135-1213` | **Shared, unchanged** (M3 discipline); mutates FPE's stored thresholds exactly as Q31's |
| 12 | `overflow_rescale` | `psy.rs:1216-1246` | **Shared, unchanged** |
| 13 | `finish_channel` / `group_short_channel` / `grouped_ld(_clamped)` | `psy.rs:1296-1551` | **Shared, unchanged** (outputs change transitively via inputs) |
| 14 | `fpe::output::fraction_to_stored` | `fpe/output.rs` | **Reused unchanged** for the M4 boundaries (init, post-spreading, spread-energy) |
| 15 | `fpe::output::assemble_channel` / `guard_channel_output` | `fpe/output.rs` | **Unchanged** (guards already cover threshold non-negativity, Ld floor/finiteness) |
| 16 | `fpe::constants::{C_RATIO_F, CLIP_F_LONG, CLIP_F_SHORT, MAXVAL_F, …}` | `fpe/constants.rs` | **Consumed** (M1 scaffolding activates); exactness tests already gate them |
| 17 | `SfbConfig::{mask_factors, mask_factors_spr, clip_energy, pcm_quant}` | `sfb.rs:422-462` | **Consumed read-only** (tabulated f64 accessors; byte-match vs stored already tested in `constants.rs`) |
| 18 | FPE orchestration call sites | `fpe/orchestrate.rs` | Init sites (2) swap to `init_threshold_f`; advance sites (2) swap to the FPE mirror. Nothing else in the orchestration changes |
| 19 | `FpeChannel` / `FpeScratch` | `fpe/mod.rs` | **No type change, no new persistent scratch fields** (the mirror uses per-window local fraction buffers sized to the config's `sfb_count` bound — `PreEchoState` is sized 51 per C `MAX_SFB`, and the `ms_ld` history uses 64; a fixed `[f64; 64]` covers all supported configs) |
| 20 | Downstream (`pe.rs`, `threshold_vbr.rs`, `scf.rs`, `rate.rs`, `stream_driver.rs`) | — | **No production change.** Decision-safety instrumented test-only via the existing capture/replay infra |

Test-side extensions (see §6 per stage): `fpe/tests/{audit, differential,
envelope, probe_envelope, m2_stages, decision, r22_capture, trace, ac9,
ac9_artifact, evidence}.rs` — comparators, capture rows, bundle families and
classification for `THR`/`SPREAD`/`NM1`/`TLD`/downstream stages; all
additive, test-only.

---

## 5. Numerical specifications

Conventions: **fraction domain** `v_f = v_st/2^31` (`05` §1.3); **stored
domain** in i32 LSBs; `m` = current block exponent (`mdctScale`); tolerances
`q31_tol(w) = 32 + |w|/500`; energy envelope `tol_E(b)` per `05` §9.2
(owner-approved, FPE-3.1R1). `fmult(a,b) = floor(a·b/2^31)` (ARM64
`fixmul_DD`, `q31.rs:131`). All M4 kernels use only `+`, `·`, `min`, `max`
and exact power-of-two scalings — **no transcendental functions** (threshold
Ld stays in the shared `finish_channel`), so cross-architecture determinism
holds by IEEE-754 construction (subject to the same f64 caveats already
accepted in M3; the N-2 architecture layering applies unchanged).

### 5.1 Threshold initialization

| Aspect | Specification |
|---|---|
| **Current Q31 calculation** | `init_threshold(E_st) = fmult(E_st, C_RATIO_Q31)` (`thresholds.rs:130`), computed per band at the rescale site (`psy.rs:772/782`; FPE M3 state: `fpe/orchestrate.rs` rescale loops call it on the boundary-quantized stored energy). Long TNS-active windows subsequently quarter the stored threshold (`>> 2`, `tns_encode_channel`); overflow rescale shifts it (`>> (tnsSpecShift<<1)`). Both are exact integer ops operating identically on FPE's stored scratch |
| **Input representation / units** | Q31: stored band energy `E_st` (i32, fraction-of-full-scale). FPE: fraction band energy `E_f` from the M3 float lane (`ws.ef_l/ef_r`), post-rescale `E_f = E_real·2^(−2(m−finalShift))`; post-TNS windows re-derive from the stored image (see 5.1.4) |
| **Output representation / units** | Stored threshold `T_st` (i32); consumed by the TNS block, the advance stage, `finish_channel` (threshold Ld, grouping) and the downstream Ld chain |
| **Intermediate arithmetic / rounding** | Q31: 64-bit product, `>>31` **floor**, i32 narrow. FPE: single f64 product `T_f = E_f·C_RATIO_F` (both operands exact `/2^31` images; product correctly rounded, ~2⁻⁵³ relative — negligible); boundary `T_st = clamp(round(T_f·2^31))` (round-half-away-from-zero, `fraction_to_stored`) |
| **Saturation / clipping / boundaries** | None at init itself: `E_f ≤ MAXVAL_F` ⇒ `T_f ≤ C_RATIO_F ≈ 0.0201 ≪ 1`; the §7 clamp is structurally unreachable for thresholds but retained. Non-negativity preserved (energies ≥ 0) |
| **State dependencies** | None — pure per-band function of energy. No frame-to-frame state |
| **Frame-to-frame transitions** | None direct; the delegated pre-echo state (M6 scope) memorizes the thresholds that leave the advance chain, so init deltas persist into history within envelope (§6 M4.9 measures boundedness) |
| **C reference behaviour** | `psy_main.cpp:702-703` `(pSfbThreshold)[sfb] = fMult((pSfbEnergy)[sfb], C_RATIO)` — identical fMult floor semantics (verified, tree `3878c45a`) |
| **Proposed f64 implementation** | `T_f[b] = E_f[b] * C_RATIO_F;` `T_st[b] = fraction_to_stored(T_f[b]);` at the two rescale-loop sites (left/right), all-zero shortcut path unchanged (thresholds stay zero) |
| **Expected numerical differences** | `\|ΔT_st\| ≤ C_RATIO·ΔE_st + 1.5` LSB: energy input difference propagated attenuated by `C_RATIO ≈ 0.0201` (the M3 accepted `tol_E(b)`, signed), plus Q31 floor (<1) and boundary round (≤0.5). The **relative terms align**: `C_RATIO·(E_ref/500) = T_ref/500`, so the retained `q31_tol` stays the incumbent gate with the N-5 propagated-bound audit (`audit.rs` already computes `prop = ΔE·C_RATIO_F + 1` and counts widenings — extend it through the new chain). Direction: the kernel-intrinsic terms (fmult floor removal, round-vs-floor) make FPE ≥ Q31; the propagated energy term is signed and either direction is possible |
| **Downstream consumers / decision sensitivity** | Highest of the three areas. `T_st` → `threshold_ld` (`finish_channel`, `-33/64` floor) → i32 Ld in the driver → PE band terms (`pe.rs:341/385/406`: `energy_ld > threshold_ld` admission + ratio magnitudes) → `thr_exp` → ATS/VBR reduction (`threshold_vbr.rs:699/753`, chaos measure state) → S2/S4 scalefactor estimation → S6 bit allocation → quantization/bitstream. Knife-edge flips at PE admission and quantization boundaries are expected and must be margin-classified (M4.8) |

#### 5.1.4 Float-lane boundary at the advance stage (design decision, §3.3)

The stored threshold is the surviving lane through the TNS block (identical
exact integer mutations in both engines). The float clip/spreading stage
restarts from `T_f = T_st as f64 / 2^31` — an exact image, costing only the
init boundary round already counted above (≤0.5 LSB). The alternative —
carrying full-precision `T_f` through the TNS block by mirroring its two
exact power-of-two threshold shifts — is rejected for M4 (§3.2 item 4).

### 5.2 Clipping (clip cap)

| Aspect | Specification |
|---|---|
| **Current Q31 calculation** | `clip_cap(clip_st, m, short)` (`thresholds.rs:137`): `shift = 2m − 4` (`THR_SHIFTBITS = 4`); headroom 6 (short) / 0 (long); `shift ≥ 0` or `≥ −headroom` → `shift_int(clip_st, −shift)` (arithmetic `>>` floor / wrapping `<<`); else `MAX_Q31` (uncapped). Per window: `t = min(t, clip)` over `active` bands (`psy.rs:1270-1273`) |
| **Input representation / units** | Stored clip energy from `SfbConfig::clip_energy_q31()` (`0x773593ff` long; short consumes `>>6`, `psy_configuration.cpp:595-607`); `m` = `mdctScale` (typically 12 long / 9 short, reduced by `finalShift` and TNS shifts); `short` flag |
| **Output representation / units** | Fraction cap `clip_f`; capped fraction threshold |
| **Intermediate arithmetic / rounding** | Q31: integer shift — right shift **floors** (<1 LSB loss); left shift exact (wrapping unreachable in-range). FPE: `clip_f = CLIP_f·2^(4−2m)` — exact power-of-two scaling both directions; both in-range shift branches are the same exact float scaling, so the branch only matters for the uncapped case |
| **Saturation / clipping / boundaries** | The left-shift branch cannot reach `MAXVAL_F`: short headroom limits negative shifts to ≥ −6 and `CLIP_F_SHORT·2^6 ≈ 0.93 < 1`; long never left-shifts (headroom 0 ⇒ any negative shift is uncapped). Uncapped branch: `MAXVAL_F`; the `min` can never bind since `T_f ≤ C_RATIO_F ≪ MAXVAL_F` — preserve the branch structurally (document unreachability, mirroring the F-6 pattern of structural notes) |
| **State dependencies / frame transitions** | None — per-channel/per-window pure function of `m` and the config |
| **C reference behaviour** | `psy_main.cpp:926-939` — identical branch structure, `fixMin` per band (verified) |
| **Proposed f64 implementation** | Compute `clip_f` once per channel before the window loop (mirroring the Q31 composite), `T_f = min(T_f, clip_f)` per band per window inside the mirror |
| **Expected numerical differences** | Only bands actually at the cap differ; there `clip_q31` floors (<1 LSB) so FPE caps are ≤1 LSB higher ⇒ FPE threshold can exceed Q31 by <1 LSB on capped bands. min on exact images is exact; no accumulation |
| **Downstream consumers / decision sensitivity** | Same threshold chain as §5.1. Clip-capped bands are the loudest bands — exactly where PE ratios and quantization decisions concentrate; cap differences are absolute-LSB-scale and expected to classify as residue |

### 5.3 Spreading (threshold spreading and spread-energy spreading)

| Aspect | Specification |
|---|---|
| **Current Q31 calculation** | `spreading_max(n, mask_lo, mask_hi, x)` (`thresholds.rs:149`): forward `d = max(x[i], fmult(mask_hi[i], d))` for `i in 1..n`; backward `d = max(x[i], fmult(mask_lo[i], d))` for `i in (0..n−1).rev()`; in place. Called per window on thresholds (`mask_factors_q31`) and, after a copy from the energies, on spread energies (`mask_factors_spr_q31`) (`psy.rs:1276/1288-1289`) |
| **Input representation / units** | Q31: stored thresholds / energies; stored Q31 factor tables. FPE: fraction thresholds (post clip, exact stored image per §5.1.4) / fraction energies (exact stored image, §3.3 sub-decision); **tabulated** f64 factors `SfbConfig::mask_factors()` / `mask_factors_spr()` (exact `/2^31` images, unit-gated in `constants.rs`) |
| **Output representation / units** | Spread fraction thresholds / spread fraction energies → §7 boundary → stored |
| **Intermediate arithmetic / rounding** | Q31: per-step `fmult` **floor** (<1 LSB each). FPE: exact f64 products; two-pass envelope identical structure. No rounding inside the kernel; contractive (all factors ∈ [0,1)) |
| **Saturation / clipping / boundaries** | None inside the kernel (max-envelope of non-negative values bounded by the max input ≤ `MAXVAL_F`); boundary round only |
| **State dependencies / frame transitions** | None — per-window pure function. Spread energies feed the avoid-hole chain (no psy state) |
| **C reference behaviour** | `spreading.cpp:105-125 FDKaacEnc_SpreadingMax` (verified: same two-pass, `maskHighFactor` forward / `maskLowFactor` backward); factors from `initSpreading` (`psy_configuration.cpp:354-405`): slopes MASKHIGH 1.5, MASKLOW 3.0 dB/Bark (threshold); spread-energy long hi 2.0 (1.5 low-rate), lo 3.0; short 1.5/2.0; edge factors 0. Spread-energy site: `psy_main.cpp:1009-1017` (memcpy + SpreadingMax with sprEn factors) |
| **Proposed f64 implementation** | `spreading_max_f(n, mask_lo_f, mask_hi_f, &mut x_f)` — identical loop structure, f64 multiply. Threshold pass consumes post-clip `T_f`; spread pass consumes `S_f[b] = e_st[b] as f64 / 2^31` (copy per band) and writes `fraction_to_stored` per band |
| **Expected numerical differences** | (a) Input deltas propagate attenuated (envelope is a max of contractive products — Lipschitz in inputs with attenuation `Π` path factors; signed). (b) Q31 floor error per recursive step <1 LSB, accumulating only along **consecutive recursive-win chains**, decaying by the subsequent factors; with tabulated factors ≈ `10^(−1.5/10) ≈ 0.71` (hi) and `10^(−3/10) ≈ 0.50` (lo) per Bark step, the chain sum is expected to be a few LSB — **to be derived per band from the actual tables, not assumed** (N-4-M4, §6 M4.7). Direction: the kernel-intrinsic floor-removal term makes FPE ≥ Q31 along recursive chains; the input-propagation term is signed. Spread-energy deltas = M3 energy deltas (already within `tol_E`) + the same chain terms |
| **Downstream consumers / decision sensitivity** | Thresholds: the full §5.1 chain. Spread energies: `initAvoidHoleFlag` (`threshold_vbr.rs:325-363` — spread scaled (`>>=1` etc.) against weighted energies; MS spread factor 0.9; short 0.63) → `ah_flag` → minSnr modification and S4 avoid-hole decisions → scalefactors → quantization. Avoid-hole flag flips are binary decisions on ratio comparisons — small spread deltas can flip them; they are prime margin-classification targets (M4.8) |

#### 5.3.4 Spread-energy input provenance (design decision, §3.3)

`S_f` starts from the **stored** energies' exact fraction image, not from
retained float energies: after TNS-active windows the authoritative energy
is the M3-accepted stored value (`tns_recompute_energy_into` writes the §7
boundary image; `overflow_rescale` then applies an exact integer shift).
Re-deriving float energies would require extending the accepted FPE-3.2
kernel for a sub-LSB gain — rejected for M4; M7 (TNS integration) may
revisit. Consequence: spread-energy comparison uses the derived energy
envelope extended by the spreading chain terms (§6 M4.7).

### 5.4 Propagation into PE, ATS, rate control, quantization and coding

Threshold and spread deltas propagate along two proven chains:

1. **Threshold → Ld → PE → ATS → S2 → S4 → S6 → bitstream.** PE band terms
   admit when `energy_ld > threshold_ld` and scale with the Ld ratio;
   thresholds also set `thr_exp`, which drives the VBR reduction curves and
   the chaos-measure state (frame-surviving — a flipped reduction path
   persists across frames). Scalefactor estimation and the quantization
   loop consume the reduced thresholds; rate control consumes PE.
2. **Spread → avoid-hole → minSnr → S4.** Binary `ah_flag` decisions with
   direct scalefactor consequences.

Consequences for M4 validation: (i) per-band value envelopes are necessary
but **not sufficient** — the M4 acceptance requires the decision-safety leg
(M4.8) mirroring the FPE-3.1R2/AC-9 pattern: production-callsite replay
equality, margin classification of every new flip, explicit unobserved-arm
records; (ii) frame-surviving downstream state (ATS chaos measure, bit
reservoir) makes flips **cascade** — the R2.2 chain-attribution machinery
(`trace.rs`) is reused; (iii) no perceptual claim is made or implied —
perceptual validation is Phase 5 / A-09 territory.

---

## 6. Staged implementation plan

Stage numbering `M4.<n>`; FPE-x.y task IDs follow `09`/`11` §2 issuance
template. Every stage: focused diff, Q31 untouched, default Q31 unchanged,
no new allocation in kernels, all Tier-A green except the documented set,
`check-licensing.sh` + `git diff --check` clean. Each stage's differential
run covers the full corpus: the committed `psy_*` vectors (47), 18 config
snapshots, 15 synthetic fixtures + 4 music/speech WAVs, and the G-1 probe
corpus (27 fixtures / 2,757 frames) via `DiffStream::from_pcm`, comparing
complete output **and** persistent state after every frame.

| Stage | Task | Production changes | Test changes | Numerical comparison | Decision-safety checks | Acceptance criteria | Depends on / rollback |
|---|---|---|---|---|---|---|---|
| **M4.0** | Governance entry | None | None | None | None | D1 (M4 authorization), D2 (A-13), D3 (baseline/commit), D4 (pin-successor policy) issued by owner | — |
| **M4.1** | **FPE-4.1a — float threshold init** | `fpe/thresholds.rs`: `init_threshold_f`; `fpe/orchestrate.rs`: both rescale-loop sites call it on `ws.ef_*`, then `fraction_to_stored` | Unit: `C_RATIO_F` exactness consumed; hand-computed init (zero, small, large, exact-LSB, boundary round-half cases); all-zero path unchanged. Differential: `THR` band comparator extended (float-init provenance), maxima/utilisation recorded | `THR` within retained `q31_tol` with the N-5 propagated-bound audit (`prop = ΔE·C_RATIO_F+1` extended through init; widenings counted, target 0); threshold-Ld deltas measured | None yet (value deltas expected; full campaign at M4.8) | THR/NM1 within gates; shifts/representation fields exact; measured maxima recorded; no new unclassified flips vs M3 baseline (deltas attributed to init) | M4.0. Rollback: revert single diff (restores Q31 `init_threshold` calls) |
| **M4.2** | **A-13 seam — FPE advance-stage mirror, zero value change** | `fpe/thresholds.rs`: `advance_thresholds` mirror (all value stages still delegate to the same Q31 kernels: `clip_cap`, `spreading_max`, `apply_pcm_floor`, `PreEchoState`); `fpe/orchestrate.rs`: both call sites swap `psy::advance_thresholds` → mirror | Test-pinned pre-mirror FPE baseline (complete per-frame outputs captured over the corpus at M4.1); mirror-mode bit-identity test | **Byte-identical** to the M4.1 FPE state on the full corpus (complete output + persistent state, per frame) | None (no value change) | Bit-identity gate green; structural review vs `psy.rs:1256-1292` (stage order, `active` indexing, LFE, `energyShift`, STOP/START discipline, `set_enabled`); D2 recorded | M4.1. **Structural rollback boundary**: reverting restores composite delegation |
| **M4.3** | **FPE-4.1b — float clip cap** | `fpe/thresholds.rs`: `clip_cap_f`; mirror's clip stage swaps to it (fraction restart per §5.1.4) | Unit: branch matrix (shift ≥0; short negative to −6; beyond headroom; long negative → uncapped; `m` at extremes); hand-computed caps; unreachable-`MAXVAL_F`-bind documented | `THR` deltas at capped bands ≤1 LSB + input propagation; corpus maxima at capped bands recorded | None yet | Same gates as M4.1; delta attribution isolated to clip | M4.2. Rollback: revert diff (Q31 `clip_cap` delegation restored) |
| **M4.4** | **FPE-4.2a — float threshold spreading** | `fpe/thresholds.rs`: `spreading_max_f`; mirror's threshold-spreading stage swaps; consumes `conf.mask_factors()` (tabulated) | Unit: hand-computed two-pass envelope (mirror `thresholds.rs:295-308` pattern in float); edge-factor/band-boundary cases; factor byte-match vs tables (already gated in `constants.rs` — keep active) | `THR` within the working gate; per-band chain-term measurements recorded (input to M4.7); grouped thresholds within `q31_tol` | None yet | Same gates; delta attribution isolated to threshold spreading; no factor re-derivation (R-05) | M4.3. Rollback: revert diff |
| **M4.5** | **FPE-4.2b — float spread energy** | `fpe/thresholds.rs`: spread-energy stage in the mirror: copy `e_st → fraction image`, `spreading_max_f` with `conf.mask_factors_spr()`, boundary round | Unit: copy/envelope/boundary round-trip; zero-energy; saturation case | `SPREAD` within the derived energy envelope (`tol_E` propagation per `05` §9.3) + chain terms (measured); grouped spread likewise | None yet | Same gates; delta attribution isolated to spread-energy | M4.4. Rollback: revert diff |
| **M4.6** | **FPE-4.3 — tonality absence + module docs** | `fpe/thresholds.rs` module docs updated from skeleton to implemented state; assert (test-only) that no tonality symbol/branch exists in the FPE tree | Doc/test only | — | — | Absence documented and asserted; `fpe/thresholds.rs` skeleton docs replaced | M4.5 (docs reflect final state) |
| **M4.7** | **N-4/N-5 — derived threshold/spread envelope + owner approval** | None (test/evidence only) | Envelope derivation implemented in the audit comparators: per-band `tol_THR(b)` = propagated init bound `C_RATIO·tol_E(b) + 1.5` + clip term (≤1, capped bands) + spreading chain term (derived from the actual tabulated factor chains, data-dependent per band) + retained floor/relative terms `max(32, …) + T_ref/500`; `tol_SPR(b)` = `tol_E(b)` + chain term + boundary term; widenings vs `q31_tol` reported explicitly (§9.3 rule) | Full-corpus re-audit under the derived envelope; maxima and utilisation recorded; unsupported/mapping-saturation arms flagged per the §9.2 discipline | Margin derivations for the envelope terms recorded (`MarginDerivationRequired` never left standing) | Owner-approved derived envelope (ADR-0004 amendment, §9.2); zero over-envelope on corpus; every widening justified or the stage fails | M4.1–M4.5 (data); can start measurement after M4.1. Rollback: envelope reverts to retained `q31_tol` (larger) — a validated state |
| **M4.8** | **R2.2-style downstream decision safety** | None (test-only via existing capture/replay infra) | `decision.rs`/`r22_capture.rs`/`trace.rs`/`ac9*.rs` extended for the M4 carrier: PE, ATS (incl. chaos-measure state), S2/S4 (incl. `ah_flag` flips from spread), S6, frame accounting; OP matrix reused; margin classification per flip; **AC-7-successor divergence characterization** (agree counts, per-family counts, pinned M4 aggregates per D4); findings routed to the registers | Downstream fields within their propagated envelopes; replay equality 100% on covered OPs | This **is** the decision-safety stage | All new flips classified (residue vs structural) with derived margins; no unclassified flip; unobserved arms explicit; M4 pin set recorded per D4; independent review of the classification (AC-10 pattern) | M4.4+M4.5 (carrier complete); M4.7 (envelope basis). Rollback: n/a (evidence; findings feed owner disposition) |
| **M4.9** | **Multi-frame state, boundary coverage, determinism, perf spot-check** | None (test only) | (a) N-6-analog: ≥471-frame runs per material; per-frame envelope + state boundedness (pre-echo feedback via delegated `PreEchoState`: `nm1` within envelope, `m_nm1`/`calc_pre_echo` exact, transitions exact; ATS chaos state bounded); (b) N-8 completion: full boundary/saturation unit matrix across all three kernels (silence, full-scale, power-of-two, negative clipNrgShift branches, edge factors, LFE, START/STOP/short sequences, mono/stereo, 44.1/48k); (c) determinism: double-run byte-identity; macOS ARM64 primary + x86-64-under-Rosetta layer (N-2 pattern); (d) performance spot-check: psy self-time vs M3 baseline recorded (non-gating; G6 is M9) | Per-frame boundedness confirmed; no state divergence outside envelope at any horizon | Flips across horizons already covered by M4.8 chains | Bounded multi-frame behaviour recorded; transitions exact; determinism layers green; perf delta recorded | M4.5; parallel with M4.7/M4.8 |
| **M4.10** | **Numerical review + owner acceptance** | None | Register updates (findings register, R22 register, assessment roadmaps Phase 3 exit, `09` M4 dated amendment per §9.4) | Consolidated M4 evidence record (maxima, envelopes, flip classification, pins per D4) | Independent review sign-off (AC-10 pattern; non-implementer where practical) | Owner acceptance record: M4 accepted with documented limitations (or blocked with findings); M5 not authorized by M4 acceptance | All above |

**Dependency graph:** M4.0 → M4.1 → M4.2 → {M4.3 → M4.4 → M4.5} → M4.6;
M4.7 measurement starts after M4.1 and completes after M4.5; M4.8 after
M4.5+M4.7; M4.9 parallel after M4.5; M4.10 last. Every stage is an
independently revertable focused diff; M4.2 is the structural rollback
boundary; the value stages behind it revert independently.

---

## 7. Validation and acceptance criteria

### 7.1 Q31↔FPE numerical comparisons

- **Fields:** per-window `THR`, grouped thresholds, `SPREAD`, grouped
  spread, `NM1` (pre-echo history), threshold Ld (long + grouped, via the
  shared `finish_channel`), and — exact — all representation fields
  (`mdctScale`, `finalShift`, `tnsShift`, `maxScale`, `nrgScaling`,
  grouped offsets, `maxSfbPerGroup`, grouping layout) and `m_nm1` /
  `calc_pre_echo` flags.
- **Envelopes:** working gate per stage = retained `q31_tol` with the N-5
  propagated-bound audit; formal gate = the M4.7 owner-approved derived
  per-band envelope (`tol_THR`, `tol_SPR`); spread energy additionally
  covered by the `tol_E` propagation rule. Ld compared by energy-domain
  inversion + absolute deltas reported separately (never `q31_tol` on Ld
  directly). No envelope is widened to make a stage pass; a widening is a
  finding.
- **Corpus:** the full differential corpus (§6 preamble) including stereo
  pairs, mono, LFE fixtures, long/short/START/STOP sequences and the probe
  corpus; per-frame comparison of complete output + persistent state.
- **Maxima and utilisation recorded** for every stage (the
  "envelope and absolute Ld differences recorded" acceptance element).

### 7.2 Boundary and saturation cases (N-8)

Per kernel: hand-computed unit vectors covering silence (all-zero element
shortcut), full-scale, exact power-of-two energies/thresholds, the boundary
round-half cases, the clip branch matrix (right-shift, short left-shift to
−6, beyond-headroom uncapped, long negative), spreading edge factors and
single-band windows, mapping-saturation arms flagged per the §9.2
unsupported-domain discipline.

### 7.3 Multi-frame state behaviour (N-6-analog)

Long runs (≥471 frames per material; chained ≥1,884) asserting per-frame
envelopes **and** state: the delegated `PreEchoState` history (within
envelope; enable/scale transitions exact), TNS state (reused, exact), MS-Ld
history (untouched in M4 — exact), and the downstream ATS chaos-measure
persistence (via M4.8 chains). Onset/stepdown/impulse/sine→onset→silence
sequences per the established corpus.

### 7.4 Stereo and supported channel configurations

Mono SCE, stereo CPE (identical spectra and state-pair discipline), LFE
(clip/spreading/PCM run; pre-echo skipped — mirrored), mixed silent+active
pairs, 44.1/48 kHz, long/short/START/STOP; the validated matrix unchanged.

### 7.5 C-reference comparisons

Tier-B C parity remains **blocked, not waived** (A-10; M8/Stage-1). M4
documents C behaviour from the verified source
(`psy_main.cpp:702-703/926-939/1009-1017`, `spreading.cpp:105-125`,
tree `3878c45a` — re-verified for this document) and validates against the
Q31 reference engine in-binary. The `09` M4 "C↔FPE Tier-B" test entries
inherit the M3-15 deferral; no Tier-B pass is claimed at M4.

### 7.6 Downstream decision changes (M4.8)

Production-callsite capture/replay over the reused OP matrix; PE/ATS/S2/S4/S6/
accounting replay equality 100% on covered arms; **every new flip
margin-classified** with derived margins (residue-explained vs structural);
avoid-hole (`ah_flag`) flips explicitly enumerated (spread is a new binary
decision carrier); unobserved arms recorded, never counted as passes;
divergence characterization + M4 pin set per D4. The 187 unjudged M3 S4 flips
remain unjudged and separate.

### 7.7 Determinism and architectures

f64 arithmetic identical by construction (no transcendentals in the M4
kernels); double-run byte-identity asserted; ARM64 primary + x86-64-under-
Rosetta layer (N-2 precedent); Linux x86-64 remains an M8 requirement;
evidence bundles deterministic per the E2/E3 discipline with tree state
recorded (SG-2 clean-checkout stays with E6).

### 7.8 Performance

Non-gating at M4 (G6 binds M9): psy self-time spot-check recorded per stage
to catch gross regressions; expected cost profile neutral (per-band f64
multiplies replacing i64 multiplies of the same count; no SIMD; no
allocation).

### 7.9 Perceptual validation — explicitly not M4

No perceptual equivalence or acceptability is claimed from numerical
similarity. M4 flips are recorded and classified; perceptual adjudication
(ABX on flip material, real-material listening, objective metrics) is
Phase 5 / A-09 with Tier-B decoder artifacts, required before any default
change and before the R2.1-B conditionals can close.

### 7.10 M4 acceptance gates (summary)

M4 is accepted iff: (a) FPE-4.1/4.2/4.3 implemented per `05` §6.3/§6.4 with
tabulated factors; (b) `THR`/`SPREAD`/`NM1` within the owner-approved M4
derived envelope (M4.7) with maxima recorded; (c) all pure-integer/decision
psy fields exact; (d) decision-safety campaign (M4.8) complete: no
unclassified new flip, replay equality on covered arms, M4 pin set recorded
per D4; (e) multi-frame boundedness (M4.9) recorded; (f) Tier-A green except
the documented set (seven preserved M3 pins handled per D4; eleven
Tier-B/environmental blockers unchanged); (g) Q31 untouched, default Q31,
licensing/hygiene clean; (h) independent numerical review signed; (i) owner
acceptance record issued. M4 acceptance does **not** authorize M5, any
default change, or any Stage-2 item.

---

## 8. Risks and deferred work

| Risk | Exposure | Mitigation (stage) |
|---|---|---|
| R-02 orchestration drift (mirror vs `psy.rs` composite) | M4.2 | Seam-first mirror with zero value change + byte-identity gate + structural review; per-frame differential thereafter |
| R-05 tuning drift (recomputed factors/constants) | M4.4/M4.5 | Consume `SfbConfig` f64 accessors and `constants.rs` literals only; factor byte-match unit gates stay active; `03` §4.3 13% recompute deviation documented |
| R-04 Ld convention error | transitive | Threshold Ld stays in shared `finish_channel`; `-33/64` floor and guard assertions unchanged |
| Accidental extra quantization (R-19-pattern) | all value stages | Exactly one §7 boundary per kernel output; audited by the decomposition/identity gates extended to thresholds (arithmetic-identity gate 2 pattern) |
| Envelope widening (N-5) | M4.1/M4.7 | Propagated-bound audit with explicit widening counter (existing `audit.rs` mechanism); derived envelope per ADR-0004 |
| Decision flips cascading through ATS/chaos state | M4.8 | Chain attribution + margin classification; multi-frame persistence checks (M4.9) |
| Pin-baseline churn | M4.8 | D4 successor policy issued before M4.8; no silent rebaseline |
| Pre-echo feedback amplification across frames | M4.9 | Boundedness measured on long runs (delegated control is contractive in the floor term); escalation if unbounded drift observed |
| Uncommitted-tree provenance | all | D3 baseline/commit decision; bundle tree-state recording |
| Tier-B unavailability | §7.5 | Blocked-not-waived status preserved; in-binary differential is the gate |

**Deferred work (not M4):** PCM floor (M5), pre-echo float state (M6),
MS/grouping/TNS-LFE integration (M7), Tier-B C parity + decoder (M8/Stage-1),
performance G6 + denormal/A-5 (M9), Linux x86-64 N-2 leg (M8), perceptual
validation A-09 (Phase 5), D-6/OP-4/OD-3 conditional-closure programme,
FpeError production implementation (FPE-default readiness), float threshold
lane through TNS + float-energy spread input (revisit at M7), spreading
factor float re-derivation and any pure-float headroom (Stage 2), SIMD.

---

## 9. Explicit M4 authorization boundaries

### 9.1 ADR amendments identified (proposed only — not made)

1. **ADR-0001 (engine separation):** record the A-13 disposition — the
   threshold-advance composite joins the orchestration as an FPE-owned
   mirror; `tns_encode_channel` delegated until the M7 review;
   `finish_channel` shared; Q31 kernel extraction rejected. Update the
   "slight duplication" consequence to name the composite mirrors
   explicitly.
2. **ADR-0004 (differential validation):** amendment recording the M4
   derived threshold/spread envelope (`tol_THR`/`tol_SPR`) as a new
   application of the FPE-3.1R1 mechanism (M4.7, owner-approved).
3. **ADR-0002/0003/0005:** no amendment required. ADR-0005's one-way
   dependency is restated as binding on M4 (value lane never feeds the
   representation lane).

Documentation amendments after authorization: `09` M4 section (dated
amendment linking this blueprint, the stage IDs and the A-13 decision),
`05` §9.x application record, `07` §5 tolerance table row, assessment
registers (dated entries at M4 entry and acceptance). **None of these is
made by this document.**

### 9.2 What M4 (as planned here) does and does not authorize

**M4 implementation, once D1/D2 are issued, comprises only:** new float
kernels in `fpe/thresholds.rs`; the FPE advance-stage mirror; the two
orchestration call-site swaps; additive test-only extensions under
`fpe/tests/`; documentation per D-approved amendments. All other
production files are read-only.

**M4 does not authorize:** any Q31 production change; any change to
`tns_encode_channel`/`overflow_rescale`/`finish_channel`; any downstream
encoder subsystem change (PE/ATS/S2/S4/S6/bitstream) beyond test-only
capture; any default-engine change or FPE-default promotion (separate
acceptance required); f32 production kernels; SIMD; Stage-2 items
(float-recomputed factors, pure-float headroom, model changes); any change
to psychoacoustic algorithms, tuning constants, thresholds, spreading
factors or tables; any tolerance/envelope change without the ADR-0004
amendment mechanism; any rebaseline of the seven preserved pins or the 187
S4 flips outside D4; any perceptual-equivalence claim; Tier-B pass claims;
any ADR/normative-document change without explicit amendment approval;
any commit, stage, push, reset, rebase or clean beyond what D3 explicitly
orders. FPE M4 output never becomes the default engine without separate
acceptance.

---

## Appendix A — Evidence index

- Q31 kernels: `rust/src/aac_enc/thresholds.rs` (`init_threshold`:130,
  `clip_cap`:137, `spreading_max`:149, `apply_pcm_floor`:167,
  `PreEchoState`:176-267); composite `rust/src/aac_enc/psy.rs`
  (`tns_encode_channel`:1135, `overflow_rescale`:1216,
  `advance_thresholds`:1256, `group_short_channel`:1296,
  `finish_channel`:1434); primitives `rust/src/aac_enc/q31.rs`
  (`fmult`:131).
- FPE: `rust/src/aac_enc/fpe/{mod,orchestrate,output,constants,thresholds,
  preecho,energy}.rs`.
- C reference (verified from git tree `3878c45a`):
  `cpp/libAACenc/src/psy_main.cpp:702-703/926-939/953-1019`,
  `cpp/libAACenc/src/spreading.cpp:105-125`,
  `cpp/libAACenc/src/psy_configuration.cpp:354-405/595-607`.
- Tables/accessors: `rust/src/aac_enc/sfb.rs:422-462`;
  `rust/src/aac_enc/sfb_tables.rs`.
- Downstream: `rust/src/aac_enc/pe.rs:341-407`;
  `rust/src/aac_enc/threshold_vbr.rs:325-363/699/753`;
  `rust/src/aac_enc/scf.rs`; `rust/src/aac_enc/rate.rs`;
  `rust/src/aac_enc/stream_driver.rs`.
- Test infrastructure: `rust/src/aac_enc/fpe/tests/{mod,audit,differential,
  envelope,probe_envelope,m2_stages,decision,r22_capture,trace,ac9,
  ac9_artifact,evidence,is_ms,precision}.rs`; evidence bundles
  `docs/evidence/fpe-r22{,-ac9}/`.
- Governance: `docs/fpe/00`–`12`; ADR-0001…0005;
  `docs/fpe/assessment/{M3-owner-acceptance,M3-exit-assessment,
  findings-register,implementation-roadmap,validation-roadmap,
  R2.2-downstream-decision-safety-design,R2.2-findings-register}.md`.

## Appendix B — Task issuance skeletons (first two stages, per `11` §2)

```
TASK FPE-4.1a — Float threshold initialization
MILESTONE: M4 (stage M4.1)
SCOPE: replace the Q31 init at the FPE rescale sites with the f64 fraction
  product; one §7 boundary per band.
FILES TO CHANGE: rust/src/aac_enc/fpe/thresholds.rs (NEW init_threshold_f),
  rust/src/aac_enc/fpe/orchestrate.rs (two rescale-loop sites).
FILES READ-ONLY: thresholds.rs (Q31), psy.rs, energy.rs, sfb.rs, sfb_tables.rs,
  q31.rs, everything downstream.
REFERENCE:
  - upstream: psy_main.cpp:702-703 (fMult(E, C_RATIO))
  - Q31 mirror: thresholds.rs:130 init_threshold
  - spec: 05 §6.3 (T_f = E_f·C_RATIO_f; boundary round); §7; §9.3/N-5
EXPECTED CHANGE: T_f = ws.ef_*[w][b] * C_RATIO_F;
  t_*[w][b] = fraction_to_stored(T_f). All-zero path unchanged.
NON-GOALS: no clip/spreading (M4.3-4.5); no PCM floor (M5); no pre-echo (M6);
  no Q31 edits; no float lane through TNS; no factor re-derivation.
REQUIRED TESTS: unit (hand-computed, zero, exact-LSB, round-half); differential
  THR (extended provenance comparator); N-5 widened counter stays 0-or-reported.
ACCEPTANCE: THR/NM1 within retained q31_tol (working gate); representation
  fields exact; maxima recorded; Tier-A green except documented set.
INVARIANTS: finite, non-negative thresholds; 05 §8 guards unchanged.
REPORT: 11 §4. ESCALATE IF: envelope exceeded; new unclassified flip; any
  apparent Q31-change need.

TASK FPE-4.2-Seam — FPE advance-stage mirror (zero value change)
MILESTONE: M4 (stage M4.2; executes owner decision D2)
SCOPE: add fpe::thresholds::advance_thresholds mirroring psy.rs:1256-1292
  stage-for-stage over FPE state/scratch, all value stages still delegating to
  the same Q31 kernels; swap the two orchestration call sites.
FILES TO CHANGE: rust/src/aac_enc/fpe/thresholds.rs, fpe/orchestrate.rs.
FILES READ-ONLY: psy.rs, thresholds.rs (Q31) — the mirror CALLS them.
EXPECTED CHANGE: none observable. Byte-identical to the M4.1 FPE state.
ACCEPTANCE: byte-identity over the full corpus (complete output + persistent
  state, per frame) against the M4.1 test-pinned baseline; structural review
  vs the Q31 composite.
ESCALATE IF: any observable difference; any branch/order ambiguity vs
  psy.rs:1256-1292.
```

**STOP — M4 requires owner decisions D1–D4 before implementation. This
document is the blueprint, not an authorization.**

---

## Dated addendum — M4.0 owner decisions issued (28 November 2026)

*Appended without rewriting §§1–9 or the appendices above (history
preserved). Authority: [M4.0 owner decision](assessment/M4.0-owner-decision.md)
(28 November 2026).*

The §2.4 decisions are no longer pending: **D1–D4 were issued by the owner
on 28 November 2026** and are recorded in the M4.0 owner decision. In
brief:

- **D1:** M4.0 and **M4.1 are authorized**; authorization for M4.2 and later
  stages is **staged** (explicit per-stage authorization required). M4.1
  implementation is **blocked** until the baseline checkpoint (D3) is
  established and verified.
- **D2:** the §3 recommendation is adopted — **FPE-side composite
  advance-stage mirror**; `tns_encode_channel` delegated until the M7
  review; `finish_channel` shared; **no Q31 kernel extraction/refactoring
  authorized**. The §3.3 sub-decisions are recorded with it.
- **D3:** a safe, reproducible baseline checkpoint of the accepted M3/R2.2
  state is **required before implementation**;
  [`M4-baseline-checkpoint-plan.md`](M4-baseline-checkpoint-plan.md)
  specifies the procedure; its Git operations **await explicit owner
  approval** and were not executed. The baseline is **not yet established**.
- **D4:** historical baseline pins are immutable; any successor baseline
  requires explicit owner approval; no silent rebaselining.

The §2.4 table, §3.3 decision text and §9 authorization boundaries above
are superseded **prospectively only** where this addendum states so; all
historical text stands as written. Implementation of §6 stages M4.2+ and
the §9.1 ADR amendments remain unauthorized.
