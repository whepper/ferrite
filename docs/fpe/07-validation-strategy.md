# 07 — Differential Validation Strategy

Date: 2026-11-16. Primary Stage-1 acceptance is **faithful reproduction of the
reference model**, proven by three-way comparison at intermediate stages —
not by final AAC files alone. Objective quality metrics supplement, never
replace, the fidelity gates.

## 1. The three implementations

| ID | Implementation | Role |
|---|---|---|
| **C** | Upstream FDK-AAC 3.0.0, unmodified, driven by `tools/enc-oracle/enc_oracle.cpp` | algorithmic ground truth |
| **Q31** | Ferrite existing engine (`psy::process_pair_reuse`) | validated integration baseline |
| **FPE** | Ferrite new float engine (`fpe::process_pair_reuse`) | implementation under test |

Comparison relations:
- **C ↔ Q31**: already established (committed vectors + Tier-B). FPE inherits
  this baseline; it is re-confirmed, not re-derived.
- **Q31 ↔ FPE**: same-binary, same inputs, same representation lane ⇒
  differences isolate the float value arithmetic. This is the tightest,
  cheapest comparison and the primary development gate.
- **C ↔ FPE**: via oracle dumps; the ultimate fidelity gate, with tolerances
  that account for both C's truncation and the documented float-native
  divergences.

## 2. Existing infrastructure reused (verified, `01` §6)

- **`enc_oracle`** dumps every psy intermediate per frame/channel:
  `FRAME/SEQ/SHAPE/GROUPS`, `RING`, `MDCT_E`, `MDCT`, `RXX1/RXX2`,
  `PSY mdctScale/finalShift/tnsShift/nrgScaling`, `MAXSCALE`, `NRG/NRGLD`,
  `THR`, `SPREAD`, `TNSW/TNSG`, `NM1/NM1STATE`, `GROUPED GE/GT/GS/GELD/GTLD/
  GOFF/GMSNR`, `LONGOUT TLD`, `MSM/MSS/MSMLD/MSSLD`, plus S1..S8 blocks
  (`tools/enc-oracle/README.md:277-300`).
- **Committed vectors**: 47 `psy_*` frame files, `seq_*`, `mdct_*`, `ring_*`,
  `rxx_*`, `ms_*`, `qc1..qc8`, 18 `cfg_*.txt` snapshots
  (`rust/src/aac_enc/testdata/`).
- **Fixtures**: 15 synthetic WAVs (`make_fixtures.py`) + 4 committed
  music/speech WAVs (`m7-music`, `m7-speech-clean`, `m7-speech-pauses`,
  `m8-music-eroica`).
- **Tier-B driver**: `rust/tools/run-reference-validation.sh` (checkout
  validation → C build → oracle → fixtures → C outputs → preflight → 15
  tests → cfg byte-gate). `check-reference.sh`, `preflight-validation.sh`.
- **Probes**: `quant_probe`, `scf_probe`, `scf_unit_probe`, `bitcount_probe`,
  `bitenc_probe`, `tp_probe`, `s6lit_probe` for downstream stages (reused as-is;
  FPE does not change downstream).

**No production dependency on the reference**: ordinary `cargo build/test/
package` never need the C checkout; Tier-B is opt-in by name and fails loudly
if artifacts are missing (`docs/reference/using-the-fdk-aac-reference.md`).
FPE keeps this property.

## 3. Reference harness for FPE (no production deps)

- **Q31↔FPE in-binary harness** (Tier-A, self-contained): a test helper feeds
  identical `ChannelInput` (from committed `mdct_*`/`ring_*` vectors or
  generated signals) to both `psy::process_pair_reuse` and
  `fpe::process_pair_reuse`, with independently initialized but equivalent
  state, and compares outputs field-by-field. It runs **multi-frame sequences**
  and compares **output and persistent state after every frame** (§3.1). Runs
  in default CI, no C artifacts. This is the main development regression net.
- **C↔FPE harness** (Tier-B, opt-in): extend the oracle comparison path so the
  FPE engine's outputs are compared against `enc_oracle` dumps. Two options
  (decided at M1, task FPE-1.3):
  1. **Rust-side**: a Tier-B test reads `<stem>.s1.txt` oracle dumps (via
     `PHASE1_ORACLE_DIR`) and compares FPE outputs computed from the same
     fixture WAVs. Mirrors existing `phase1_*` tests.
  2. **C-side instrumentation**: `enc_oracle` already exposes the needed
     intermediates; **no new C instrumentation is required** for psy stages.
     If a missing intermediate is identified (e.g. per-band float energy before
     quantization), add a dump line to a **copy** of the oracle in scratch
     (never to production; the oracle is test-only tooling). Document any such
     addition.
- The harness lives under `fpe/tests/` and `#[cfg(test)]` modules; it imports
  production FPE functions but adds no production code path.

## 3.1 Persistent-state comparison (M2 binding; multi-frame)

Output-only comparison can miss state divergence that only manifests on a later
frame. The harness therefore compares **all frame-surviving psy state after
every frame**, over **multi-frame sequences** (not isolated frames):

| State | Fields | Access strategy |
|---|---|---|
| `PreEchoState` (`thresholds.rs:184-188`) | `prev`, `prev_scale`, `enabled` | existing pub accessors `prev()/prev_scale()/is_enabled()` — no new API |
| TNS `tns_state` + `tns_info` (`tns.rs:248-267`) | `num_filters/coef_res/length/order/direction/coef/active/gain/merged` | fields are `pub`; test-only field-by-field compare (or `#[cfg(test)]` `PartialEq` helper) |
| MS-Ld history `ms_ld` (`psy.rs:160`) | `[[f64;64];8]`, incl. stale-on-short | private → **`#[cfg(test)]` accessor/snapshot** |
| Other frame-surviving psy state | none beyond the above within the psy boundary | — |

- **Test-only helpers, no production API**: private state is exposed only via
  `#[cfg(test)]` helpers (accessor/snapshot or compare fn) in the owning
  module. **No additional production API is added solely for testing.**
  Upstream `BlockSwitch`/`ChannelTransform` state is reused and identical by
  construction; downstream ATS/QC state is outside the psy boundary (shared).
- **Sequence coverage**: multi-frame runs must exercise long/short transitions,
  START/STOP sequences, pre-echo enable/reset (STOP-before/START-after,
  memorize-only frames), and stale-on-short MS-Ld history. Use corpus runs such
  as onset, stepdown, click, impulse, and sine→onset→silence; assert state
  bit-identity at **each** frame, so a divergence is localized to the frame
  that introduced it.
- **M2 vs M3+**: at M2 the comparison is **bit-identical** (`==`) for output
  and state (pass-through). From M3, state that becomes float (e.g. fraction
  pre-echo history at M6) is compared within the established envelopes, while
  reused state (TNS) and pure-integer state stay exact (`05` §9).

## 4. Intermediate-stage comparison points

| Stage | Quantities | C dump tag | Q31↔FPE | C↔FPE |
|---|---|---|---|---|
| Block switch | seq, shape, groups | `FRAME/SEQ/SHAPE/GROUPS` | exact (shared) | exact on synthetic; pinned f107 exception |
| MDCT | spectrum, exponent | `MDCT/MDCT_E` | exact (shared) | existing f32 tolerance |
| Headroom | maxScale | `MAXSCALE` | **exact** | exact |
| Rescale shifts | mdctScale/finalShift | `PSY` | **exact** | exact |
| Band energies | stored E, Ld | `NRG/NRGLD` | `q31_tol`/Ld env | `q31_tol`/Ld env |
| TNS | activation, order, coefs, gain | `TNSW/TNSG` | exact (reused) | existing TNS margins |
| TNS recompute | E, Ld, nrgScaling/tnsShift | `NRG/PSY` | exact shifts; E env | E env |
| Thresholds | stored T, Ld | `THR` | `q31_tol`/Ld env | `q31_tol`/Ld env |
| Spread energy | stored S | `SPREAD` | `q31_tol` | `q31_tol` |
| Pre-echo | nm1, m_nm1, calc | `NM1/NM1STATE` | history env | ≤ Q31 envelope |
| MS energies | mid/side stored, Ld | `MSM/MSS/MSMLD/MSSLD` | stored env; **grouped MS-Ld exact** | stored env |
| Grouping | GE/GT/GS/GELD/GTLD/GOFF/GMSNR, maxSfb | `GROUPED` | exact layout; values env | values env |
| Long output | TLD, maxSfb | `LONGOUT` | Ld env; maxSfb exact | Ld env |
| Downstream | PE, S2 thr, S4 scf, S6 bits, S8 | `S1..S8` | within propagated env | existing Tier-B gates |

## 5. Tolerances and decision-level equivalence

Numerical envelopes (existing policy, `01` §7; FPE adopts, does not widen
without evidence + approval):

**Validation relations are distinct and must not share one tolerance blindly.**

| Relation | What is compared | Envelope |
|---|---|---|
| **C↔Q31** | validated fixed-point engine vs C oracle | existing `q31_tol` / Ld energy-domain policy (unchanged) |
| **Q31↔FPE (energies)** | physical-f64 FPE vs Q31 reference | **derived per-band envelope** `05` §9.2 (`tol_E(b)`) |
| **Q31↔FPE (thresholds, nm1, grouped thresholds, reused Q31-kernel outputs)** | Q31-arithmetic values downstream of energy | `q31_tol` retained where the propagated bound is no greater (`05` §9.3) |
| **Q31↔FPE (Ld)** | per-window/grouped/threshold Ld | energy-domain inversion using `tol_E(b)`; absolute Ld measured separately |
| **Exact representation lane** | `maxScale`, `mdctScale`, `finalShift`, `tnsShift`, `nrgScaling`, `shiftBits`, grouped offsets, `maxSfbPerGroup` | **exact** |
| **FPE arithmetic identity** | independent recomputation (gates 1–2) and decomposition term bounds (gate 3) | bit-exact for gates 1–2; term bounds for gate 3 (§5.1) |
| **Downstream decision safety** | IS/MS/PE/ATS/S2/S4/S6/frame accounting | **not covered here — required before acceptance, FPE-3.1R2** |

- **Stored Q31 ints (retained fields)**: `|got − ref| ≤ 32 + |ref|/500`
  (`q31_tol`). Retained for thresholds, `nm1`, grouped thresholds and reused
  Q31-kernel outputs.
- **Energy domain (Q31↔FPE)**: the derived per-band bound
  `tol_E(b) = max(32, Q_b + T_b + 1.5) + E_ref/500` (`05` §9.2), computed from
  the actual Q31-mapped/physical spectrum and representation state. It applies
  to long/short rescale energies, spread energy and grouped energy sums. It is
  **not** a fixed global tolerance and is not applied to unrelated fields.
- **Ld (log2/64)**: compared by energy-domain inversion with `tol_E(b)` for
  energy vectors and the retained threshold envelope for threshold Ld. The C
  `fLog2` Taylor error (~1e-9) and the platform `log2` error are **measured,
  not assumed** (`05` §2); absolute Ld differences are reported separately
  for per-window, grouped and threshold vectors. `q31_tol` is not applied
  directly to Ld values.
- **Pure integer state** (maxScale, mdctScale, finalShift, tnsShift,
  nrgScaling, shiftBits, grouped offsets, maxSfbPerGroup): **exact**.

**Sentinel/divergence classification.** Q31↔FPE low-energy differences are
classified by actual stored-energy state (`05` §9.4), not by whether Ld values
cross `−1.0`:

- **Class A (mapping-to-zero):** physical energy positive, all mapped Q31 lines
  zero, Q31 stored energy zero; FPE may store positive and resolve Ld.
- **Class B (post-shift truncation vs rounding):** mapped Q31 lines nonzero,
  Q31 rescale shift/floor yields zero, FPE rounds to positive. Grouped Ld may
  differ substantially.

These are expected divergences; FPE is not forced to reproduce Q31 truncation.
Counts are reported per vector and per window type with the relevant
representation parameters.

**Approval mechanism.** An envelope change is valid only under ADR-0004's
amendment mechanism: mathematically derived, independently identified error
sources, source-verified, data-dependent where appropriate, owner-approved,
and independently reviewed. The new energy envelope is the first application.

**C↔FPE (Tier-B) policy.** When C artifacts are available:
`tol_C_FPE = q31_tol + Q_b + T_b + 1.5` (triangle inequality). Tier-B remains
BLOCKED/INCOMPLETE without the reference checkout/oracle/decoder; no pass is
claimed.

#### 5.1 FPE arithmetic identity gates

The differential harness adds three independent checks (`05` §9, FPE-3.1R1):

1. **Q31 reference integrity** — test-only independent integer recomputation of
   Q31 stored band energy from the mapped spectrum (mapping, maxScale, square
   floor, wrapping accumulation, re-shift, clamp/floor, saturation). Must be
   bit-exact against the Q31 engine output; production Q31 helpers are not
   called by the recomputation.
2. **FPE arithmetic integrity** — independent recomputation of
   `clamp(round((Σx²)·2^(31−2m')))` and the §6.8 Ld formula (scale0
   correction, silent guard, unconditional `finalShift` lift, sentinel). Must
   be bit-exact against the FPE output.
3. **Decomposition term checks** — using the actual physical spectrum and its
   mapped Q31 representation, verify the measured energy difference splits
   into the actual input-mapping residue, Q31 truncation/re-shift, boundary
   rounding, and negligible f64 residue. The substantive checks are the
   independent term bounds (`|mapping| ≤ Q_b`, `|trunc| ≤ T_b + 1`,
   `|boundary| ≤ 0.5`). The reported residual is a **consistency relation**
   between the production delta and those independent terms and telescopes to
   zero when gates 1–2 pass; it is not claimed as an independent detector of
   arbitrary shared implementation errors. A term outside its derived bound is
   a genuine defect.

#### 5.2 Downstream decision safety (required before acceptance)

The energy envelope is **not** sufficient to accept FPE-3.1. The following
remain outstanding and are implemented in **FPE-3.1R2**:

- IS book/scale/digest comparison.
- MS mask/digest comparison.
- PE and ATS evolution.
- S2 VBR adaptation and avoid-hole decisions.
- S4 scalefactor decisions.
- S6 bit allocation.
- Final output/frame accounting.
- Margin-based residue-vs-structural flip classification.
- Targeted quiet short-window sentinel/PE test.
- **F-6 short-TNS overflow exponent:** construct a **short** element with
  element-global `tnsSpecShift != 0` and verify the pre-rescale `mdctScale`
  reconstruction in `fpe/tests/audit.rs` (the `let m = want.mdct_scale + f`
  helper and the short-TNS `ld_ref`/Ld gate). R1 marks this case unsupported
  if observed and does **not** claim it resolved; the current corpus does not
  exercise it.

FPE-3.1 is **not accepted** after R1 alone; the milestone remains blocked until
R2 and independent review complete.

Decision-level equivalence:

- Window sequence, shape, grouping: **exact** on the synthetic corpus;
  the documented float-native knife-edge (`m7-speech-clean` f107) is a pinned,
  classified exception inherited from Q31 — FPE must not introduce **new**
  decision flips beyond the classified set. Any new flip is a finding requiring
  triage (residue-explained vs structural), mirroring the existing phase1e
  methodology (`stream_driver.rs:2884-3013`).
- TNS activation: exact (reused TNS); coefficient/gain decisions within the
  existing D-013 margins (closed for committed corpus).
- MS digest / IS books: exact on corpus where Q31 is exact; residue-scale
  deltas triaged.
- Pre-echo enable/scale state: exact transitions; threshold history within
  envelope.

**Equivalence criterion**: FPE passes a stage iff (a) every pure-integer/
decision field matches exactly (or is a classified pinned exception), and
(b) every value field is within its envelope. A single unexplained
out-of-envelope value or new decision flip fails the stage.

**M2 pass-through gate (P2):** at M2 the criterion is stricter — **bit-identical
complete observable output AND all frame-surviving persistent state, after
every frame of multi-frame sequences** (`==` on every field of
`ChannelOutput`/`GroupedOutput`/`MsOutput`, the mutated spectrum, and
`PreEchoState`/`tns_state`/`tns_info`/`ms_ld`; §3.1), because the value lane
delegates to Q31 with no float conversion or rounding (`09` M2, `05` §7,
ADR-0004). Sequences cover long/short, START/STOP, pre-echo enable/reset, and
stale-on-short MS-Ld. Envelope-based comparison applies only from M3 onward,
per replaced kernel.

## 6. Test corpus

Required coverage (assignment §11), mapped to existing + new fixtures:

| Category | Existing | New (if gap) |
|---|---|---|
| Mono / stereo | click/impulse/edge/noise mono; onset/sine/etc stereo | — |
| Sample rates | 48k, 44.1k fixtures | — (validated matrix is 44.1/48k) |
| Long windows | sine, multitone, noise | — |
| Short windows | onset, click, impulse | — |
| Window transitions | onset (START/SHORT/STOP), stepdown | — |
| Transient-rich | onset, click, impulse, stress | — |
| Sustained tonal | sine, multitone | — |
| Noise-like | noise, tonal-noise | — |
| Silence | silence-stereo | — |
| Near-silence / low-level | quiet-sine | add near-silence ramp (M1) |
| High-level | stress, edge | add full-scale square (M1) |
| Numerical edge cases | — | add: power-of-two-boundary energies, denormal-adjacent quiet bands, max-headroom bands, all-zero element, mixed silent+active pair (M1) |
| Real music/speech | m7-music, m7-speech-clean/pauses, m8-eroica | — |

New fixtures are generated by an **additive** script (extend `make_fixtures.py`
or a new `make_fpe_fixtures.py` in tools, test-only) and committed as vectors
via the established 3-run stability validation (`validate_s*.py` methodology).
No committed vector is regenerated blindly (GSPEC/SPEC nondeterminism,
`01` §7).

## 7. Test tiers

| Tier | Scope | Runs | C deps |
|---|---|---|---|
| **Unit** | each FPE kernel pure-function: energy sums, spreading envelope, PCM floor, pre-echo clamp/floor/memorize/reset, MS /4 convention, Ld conventions, boundary quantization, invariants | default CI | none |
| **Algorithm differential (Q31↔FPE)** | per-stage field comparison on committed vectors + generated signals | default CI | none |
| **Algorithm differential (C↔FPE)** | oracle-dump comparison per stage | Tier-B opt-in | reference checkout + oracle |
| **Integration** | FPE-selected full encode of corpus; ADTS validity; decode via `AAC_DEC_BIN`; selection seam; default regression | default CI (decode Tier-B) | decoder for decode tests |
| **Encoder regression** | all existing Tier-A tests pass unchanged; Q31 default byte-identical to baseline hashes | default CI | none |
| **Reference-output** | existing 15 Tier-B tests pass with Q31; FPE added to applicable comparisons | Tier-B opt-in | full reference |
| **Decoder validation** | FPE streams decode cleanly (no errors), PCM finite | Tier-B | `aac-dec` |
| **Objective quality (supplementary)** | SNR/segmental-SNR vs C decode, ebur128 loudness (`tools/aac-enc-metrics/`); report only, not a gate | optional | C outputs |

## 7.1 Execution integrity — no silent success (review §6)

A skipped frame, missing artifact, or incomplete test run **must never** produce
a misleading successful validation result:

- **Frame accounting**: every differential/integration test asserts the
  **expected frame count** was processed and compared. A frame skipped by the
  encoder (skip-with-note) is a **finding**, not a silent omission; the test
  fails or reports it explicitly. The FPE harness counts compared frames per
  fixture and asserts full coverage.
- **Tier-B SKIP ≠ PASS**: the existing driver already classifies PASS/SKIP/FAIL
  and treats any SKIP as INCOMPLETE (exit 2), never counting it as passed
  (`rust/tools/run-reference-validation.sh`, `tierb-report.sh`). FPE Tier-B
  tests inherit this: a missing oracle/decoder artifact fails loudly
  (`docs/reference/using-the-fdk-aac-reference.md`), never skips silently.
- **Coverage matrix is a gate**: the §6 corpus matrix must be **fully
  executed**; the validation report lists every category × fixture with its
  result. An unexecuted cell is INCOMPLETE, not PASS.
- **Failure reporting**: each test reports per-field max delta vs envelope,
  exact-field confirmations, and any decision flips with triage classification.
  Aggregate "all passed" is only emitted when every selected test ran to
  completion and every coverage cell is accounted for.
- **Default-regression guard**: the Q31-default byte-identical baseline check
  (`production_baseline.rs` hashes) runs independently of FPE selection, so an
  FPE-only test path cannot mask a Q31 regression.

## 8. Stage-1 acceptance gates (formal)

FPE is Stage-1 complete iff **all** hold:

0. **G0 M2 pass-through**: the M2 skeleton produced bit-identical complete
   observable output **and all frame-surviving persistent state, after every
   frame of multi-frame sequences** vs Q31 (no float round-trip/rounding),
   establishing the wiring baseline before any float kernel (`09` M2, §3.1,
   §5 M2 gate).
1. **G1 Fidelity (C↔FPE)**: every §4 stage passes §5 equivalence on the full
   §6 corpus (existing + new fixtures), with any exceptions classified,
   pinned, and owner-approved. No unexplained out-of-envelope value. Coverage
   matrix fully executed; no silent skips (§7.1).
2. **G2 Baseline (Q31↔FPE)**: differences confined to the float value
   arithmetic; pure-integer/decision fields exact; measured envelopes
   recorded (A-4 resolved).
3. **G3 No regression**: all existing Tier-A tests pass; the 15 Tier-B tests
   pass with Q31; Q31-default output byte-identical to pre-project baselines
   (`production_baseline.rs` hashes); `cargo package --locked -p ferrite-aac`
   succeeds.
4. **G4 Numerical safety**: invariant tests pass; no NaN/Inf/invalid threshold
   reaches downstream on the corpus (including edge fixtures); fuzz/property
   test over random spectra confirms boundary guards.
5. **G5 Decoder**: FPE corpus streams decode without error.
6. **G6 Performance**: no material regression vs Q31 baseline without owner
   approval — decided on the **fixed-sample 95% two-sided CI** of the median
   paired % difference (predeclared `N`, single analysis; `08` §3.2): G6 PASS
   iff CI upper bound ≤ +2% (an interval entirely below −2% is a measured
   improvement and also passes); INCONCLUSIVE if the CI straddles +2% (`08`
   §3–§4).
7. **G7 Licensing/provenance**: `rust/tools/check-licensing.sh` passes; `git
   diff --check` clean; notices per `12`.
8. **G8 Review**: architecture, numerical, and validation independent reviews
   signed off (`09` gates); risk register (`10`) reviewed.
9. **G9 Default unchanged**: FPE is selectable but **not default**; default
   remains Q31.

Objective metrics are reported alongside G0–G9 but are **not** acceptance
criteria. No claim that numerical/metric differences are perceptual
improvements.

## 9. Validation risks and mitigations (cross-ref 10)

- Reference checkout unavailable (R-12): re-provision at M0/M1; Q31↔FPE
  in-binary harness keeps development unblocked meanwhile.
- Oracle nondeterminism (GSPEC/SPEC): compare only stable fields; config-only
  byte-gate; 3-run stability for any new vector.
- Tolerance masking a real defect: envelopes are inherited from the validated
  Q31 policy, not chosen to make FPE pass; any widening needs evidence +
  approval; decision fields stay exact.
- Cross-arch `log2` accuracy: ARM64 reference; x86-64 supplementary; the
  absolute/cross-platform Ld bound is **measured and recorded** (`05` §2), not
  assumed correct-rounding; covered by the Ld envelope.