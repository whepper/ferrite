# FPE-3.3 / A-4 Experiment Report — f32-vs-f64 Energy-Sum Precision

Date: 2026-11-26 (experiment); remediated per review findings F1–F7 on
2026-11-26 (see §14). Model: Muse Spark 1.3 (implementation contributor).
Authorization: test-only experiment implementation, execution, and reporting
(task brief §§1–9), plus bounded test-only remediation (§§F1–F7). This
report documents results; it does not close A-4, amend ADR-0002, or
authorize any production change. All owner decisions are flagged in §11.

## 1. Baseline and inventory (reconciled)

- Branch `main`, HEAD `189e6a79b78a84dcc5be88eaca5ffaba24097acc` (verified;
  matches the expected historical HEAD).
- Current `git status --short` at this run: **15 modified + 12 untracked**
  (enumerated in §2; this task's 2 new files + 1 line vs pre-existing
  FPE-3.2 + R2.2 work, preserved untouched).
- Toolchain/platform: `rustc 1.99.1`, `cargo 1.99.1`, `aarch64` (Apple
  Silicon, ARM64 primary target). x86-64 cross-arch measurement is A-5 and
  was not performed here; all numbers below are ARM64.

## 2. Files added or modified (F5 inventory — this task vs pre-existing)

This task's files (test-only, §14 remediation included):

- NEW (untracked): `rust/src/aac_enc/fpe/tests/precision.rs`
  (test-only). License header per repo convention (Ferrite
  Third-Party Modified Version notice, 2026-11-26).
- NEW (untracked): `docs/fpe/FPE-3.3-A-4-experiment-report.md` (this file).
- MODIFIED (1 line of the file's diff): `rust/src/aac_enc/fpe/tests/mod.rs`
  — added `mod precision;` in alphabetical order.

Pre-existing work, preserved untouched (not this task):

- Modified (14 files fully pre-existing, plus `tests/mod.rs` which
  additionally carries this task's one line): `docs/fpe/assessment/R2.2-
  downstream-decision-safety-design.md`,
  `docs/fpe/assessment/{findings-register,implementation-roadmap,
  validation-roadmap}.md`, `rust/src/aac_enc/fpe/{energy,mod,
  orchestrate,output}.rs`, `rust/src/aac_enc/fpe/tests/{decision,is_ms,
  r22_capture}.rs`, `rust/src/aac_enc/{rate,scf,testdata}.rs`, plus the
  pre-existing `mod evidence/s4_step3/trace/xarch_log2` hunks inside
  `tests/mod.rs`.
- Untracked (10 files): `docs/fpe/assessment/R2.2-group-{C-owner-acceptance,
  D-implementation-report,D-owner-acceptance,D-owner-decision,
  E1-owner-decision}.md`, `docs/fpe/assessment/a5-n2-owner-decision.md`,
  `rust/src/aac_enc/fpe/tests/{evidence,s4_step3,trace,xarch_log2}.rs`.
- No production file touched by this task. `git diff` on all production
  paths shows only pre-existing working-tree changes.

## 3. Implementation

Three arms, identical inputs/offsets/order/scaling, no fast-math/FMA/SIMD,
f64 `log2`/Ld/guards/sentinels/boundary in every arm:

- **F64**: unchanged production calls (`rescale_band_energy_into`,
  `tns_recompute_energy_into`, `fraction_to_stored`).
- **F32-realistic** (`rescale_f32r_band` + inline TNS equivalents): each f64
  line → f32, square in f32, accumulate in f32, widen to f64 for the exact
  power-of-two scaling and all downstream steps.
- **F32-accumulator diagnostic**: square in f64, add in f64, round the
  running accumulator to f32 after every addition (`as f32 as f64`).
  Mechanistically verified: `realistic_arm_narrows_inputs` asserts each arm
  equals its defining expression on `sqrt(2)`.
- Negative control (pure-f32-through-boundary) exists only as a unit test:
  at 2^31 magnitude f32 ULP is 256 LSB, so it is non-viable by construction
  and excluded from corpus acceptance.

Driver (`drive_fixture`): per-frame BlockSwitch + ChannelTransform
(identical construction to `DiffStream`), joint element-global rescale-lane
mirror (`map/maxScale → check/maxNrg → rescale_shifts → Ld/re-shift`,
`psy.rs:644-811`), representation shift, real `tns::detect` + pair sync
(F1: right-channel config via `sync_config`, mirroring `tconf_r`,
`psy.rs:853-861`) + real `psy::tns_encode_channel` on test-owned state,
then per-band arm comparison. TNS-active windows use production read-back
inputs (filtered spectrum, recomputed headroom, integer `shiftBits`,
post-pre-halving `m`); TNS-inactive windows exercise the same math on the
unfiltered spectrum with `shiftBits = 0` and an independent Q31 reference
(labelled inactive; production skips recompute there — documented
methodological choice; F7 scope: inactive rows cover `shiftBits = 0`
only, never nonzero shifts).

Fidelity gates (asserted, not assumed): the local Ld replication is
bit-exact vs the production kernels' own Ld outputs, and Q31↔F64 deltas sit
inside the approved per-band envelopes on all 467,820 combined kernel
observations (233,910 unique window-band cells × 2 kernels: rescale +
TNS recompute; see §4 for the counting convention)
(rescale `tol_E(b)`; TNS adapted N-4 forms). Any driver-lane drift would
fail loudly instead of contaminating the f32-vs-f64 comparison. (F3: no
self-check of `fraction_to_stored` is asserted — re-asserting a value
against its own construction proves nothing; boundary correctness rests on
the `output.rs` unit tests plus the independent Q31 fidelity gate.)

Downstream sensitivity probe — NOT a production mirror (F2): per-arm
`init_threshold` + real Q31 `advance_thresholds` with per-arm `PreEchoState`
carried across frames; threshold init threading mirrored (init →
pre-halving `>>=2` where production applies it → overflow `>>=2·shift`, all
with the same `shift_int` production uses). The probe passes each arm's
RESCALE-basis stored vector uniformly: production threads post-TNS/overflow
`e_l` into `advance_thresholds` (recomputed energies feed spreading, which
copies energies, and the resulting thresholds feed pre-echo), which the
probe deliberately does not replicate. Results are sensitivity observations
only; no decision-safety claim follows (R2.2 owns decision safety).
Zero-element frames run the probe's `advance_thresholds` per arm with
zeros at m = 0 (F4: preserves `PreEchoState` progression exactly as
production's unconditional call does; inter-arm deltas there are 0).

## 4. Coverage achieved (27 fixtures, 2757 frames; corrected run §14)

Long 2662 / short 95; TNS-active frames 216 (short-TNS-active 43, via
fpe-tns-active-short-stereo); mono + stereo; silence, all-zero, quiet-sine,
near-silence ramp, tonal, multitone, noise, sweep, real music/speech
(eroica), onsets/stepdown/clicks/edges/impulses, stress, full-scale square,
pow2-boundary, max-headroom, mixed-pair; 44.1 kHz + 48 kHz. Multi-frame
sequences throughout (fixture + 2 zero-tail frames, exact frame accounting).
Zero-element frames now advance the probe state (F4) instead of skipping it.

Bands compared: rescale 233,910; TNS-long-active 15,240;
TNS-long-inactive 199,534; TNS-short-active 3,107; TNS-short-inactive
16,029. Counting convention (R1): the four TNS rows sum to a TNS total of
233,910 — one TNS observation per window-band cell, matching the 233,910
rescale observations cell-for-cell. Combined kernel observations =
233,910 + 233,910 = 467,820 over 233,910 unique window-band cells (each
cell is observed once by the rescale kernel and once by the TNS recompute
kernel). Supported-domain: 100% supported, 0 unsupported, 0 non-finite,
0 mapping-saturated. Saturation counter 0 under the exact F6 boundary
(finalShift keeps energies in range; boundary saturation remains
unit-covered in `output.rs` tests). Sync accounting (F1): 2349 long stereo
pairs (tns_long), 89 short stereo pairs (tns_short); mono fixtures exercise
no sync call (unchanged path).

Unexecuted / uncovered cells (reported, not passed): real-corpus Class A
mining yielded 0 (deterministic Class A coverage via
`sub_lsb_class_a_detected` unit test + audited `energy.rs` Class A tests);
MS/grouping are not A-4 surfaces (Q31, per A-18); A-5 cross-arch not run;
nonzero shiftBits unobserved on-corpus (F7: max observed shiftBits 0 over
15,240 active-long bands — the experiment covers shiftBits = 0 only).

## 5. Numerical results (f32-variant vs f64; Q31↔F64 fidelity held everywhere)

| Kernel | Arm | maxStoredLSB @location | buckets [0,1,2-4,5-16,17-64,65+] | meanLSB | maxRelFrac | maxAbsLd | overEnv | maxUtil | guard/sent flips |
|---|---|---|---|---|---|---|---|---|---|
| rescale | f32r | 338 @eroica f261 ch1 w0 b18 | [185963,18564,12231,9944,5834,1374] | 2.01 | 5.48e-7 | 1.24e-8 | 0 | 0.031 | 0/0 |
| rescale | f32a | 338 @same | [187035,18652,11935,9678,5467,1143] | 1.84 | 5.12e-7 | 1.15e-8 | 0 | 0.031 | 0/0 |
| tns-long-act | f32r | 55 @sweep f36 ch0 w0 b34 | [14128,518,254,245,95,0] | 0.40 | 3.80e-7 | 8.56e-9 | 0 | 0.020 | 0/0 |
| tns-long-act | f32a | 55 @same | [14199,481,246,219,95,0] | 0.37 | 3.74e-7 | 8.44e-9 | 0 | 0.020 | 0/0 |
| tns-long-inact | f32r | 338 @eroica f261 ch1 w0 b18 | [155553,16885,11244,9153,5448,1251] | 2.16 | 5.48e-7 | 1.24e-8 | 0 | 0.025 | 0/0 |
| tns-long-inact | f32a | 338 @same | [156526,16969,10966,8907,5144,1022] | 1.97 | 5.12e-7 | 1.15e-8 | 0 | 0.025 | 0/0 |
| tns-short-act | f32r | 33 @music f7 ch1 w5 b0 | [2850,98,89,63,7,0] | 0.33 | 2.69e-7 | N/A | 0 | 0.0004 | 0/0 |
| tns-short-act | f32a | 33 @same | [2855,99,85,67,1,0] | 0.30 | 2.20e-7 | N/A | 0 | 0.0004 | 0/0 |
| tns-short-inact | f32r | 53 @music f113 ch1 w7 b0 | [14379,837,433,324,56,0] | 0.39 | 3.65e-7 | N/A | 0 | 0.026 | 0/0 |
| tns-short-inact | f32a | 33 @multitone f1 ch0 w7 b0 | [14433,808,458,300,30,0] | 0.33 | 3.65e-7 | N/A | 0 | 0.026 | 0/0 |

(Corrected run: 467,820 combined kernel observations over 233,910 unique
window-band cells. Rescale rows are bit-identical to the
pre-remediation run — rescale is upstream of TNS sync, as expected. Active/
inactive long rows shifted by one window: 15,150→15,240 active,
199,624→199,534 inactive; see §14.)

~79% of bands bit-exact; ~99.4% within 64 LSB. The 338-LSB maxima sit in
loud eroica bands where the envelope is ~10k LSB (E/500 term) — utilization
≤ 0.031 everywhere (i.e. ≥32× margin to the approved bound). Max relative
fraction error 5.5e-7 matches the `n·2^-24` expectation for wide loud bands.
Max |ΔLd| 1.24e-8 — an order below the recorded 1e-6 clean-band observation.
Flip/outlier log: 0 entries (no over-envelope, guard, or sentinel events on
467,820 combined kernel observations). Minimum guard distances: rescale zero-crossing 3.0e-7 (small
bands exist near zero — yet zero sentinel flips, since both arms hit exact
0.0 identically); TNS-long guard 8.2e-2 (active) / 2.5e-2 (inactive).

Class B mined: rescale 6796, tns-long-act 388, tns-long-inact 6054,
short-act 132, short-inact 258 — with **zero** Class B stored-value flips in
either f32 arm (classFlipR/A = 0 everywhere): every Q31-truncation-vs-FPE
divergence is reproduced identically by the f32 variants.

## 6. Guard/sentinel and Class A/B inventory

Guard flips 0, sentinel flips 0, class flips 0 — across all kernels and both
f32 arms. The f32 variants never change a stored-energy outcome where Q31
and f64 disagree, nor any Ld guard/sentinel branch outcome.

## 7. Downstream numerical propagation (sensitivity observations only)

maxThr: f32r-vs-Q31 32, f32a-vs-Q31 32, f32r-vs-f64 8, f32a-vs-f64 8 (LSB);
maxSpread vs f64: 338/338 (inherits the stored-energy max); maxPreEchoPrev
vs f64: 1/1 LSB. No downstream decision changes were assessed — per the
brief, any decision-level effect is R2.2 business. CORRECTION (F2): the
prior revision of this report stated that TNS-recomputed energies do not
feed thresholds in production. That is wrong: production passes
post-TNS/overflow `e_l` into `advance_thresholds`, so recomputed energies
feed spreading (spread copies energies) and pre-echo processing. The probe
passes rescale-basis vectors uniformly instead (see §3) — a sensitivity
probe, not a production mirror. These numbers are sensitivity observations,
not safety evidence. (Reminder: `05` §9.6 and the R2.2 limitations remain
in force — no decision-safety or perceptual inference follows from error
bounds.)

## 8. Regression results and known failures

- New: 8/8 precision tests pass in debug and release (`cargo test precision`
  and `cargo test precision --release`; includes 2 F1 sync-config tests +
  corpus experiment + 5 arm-semantics units).
- Existing: `cargo test --lib aac_enc::fpe::energy` 24/24 pass;
  `cargo test --lib fpe` 276 passed / 7 failed —
  the 7 failures are pre-existing working-tree issues, none attributable to
  this task: 6 in `is_ms.rs` R2.2 evidence tests
  (`r21b_p0b_aggregates_are_pinned`,
  `r21b_p0b_byte_baseline_is_pinned`,
  `r21b_p0b_canonical_evidence_is_emitted`,
  `r21b_p0b_per_finding_evidence_is_complete_and_traceable`,
  `r21b_p0b_stable_identifiers_are_unique_and_deterministic`,
  `r21b_p0b_unsupported_frame_is_explicit`;
  byte-baseline pins moved by in-progress R2.2/FPE-3.2 changes, e.g. FPE
  ADTS length 3140 vs pinned 3147 — cf. open item A-21) and 1 in
  `probe_envelope::g1_probe_corpus_aggregate`
  (pinned grouped-B count 3 vs 47 — pins recorded against HEAD production
  outputs, moved by the pre-existing FPE-3.2 TNS-recompute change). All
  failing tests live in files with pre-existing working-tree modifications
  or depend on pre-existing production-output changes; this task's diff adds
  one `mod` line and one new test-only file and cannot affect them.
  A pristine HEAD baseline run was not possible without stashing (forbidden);
  failure attribution is by file/change ownership instead.
- Hygiene: `rustfmt --check` clean on the new file (`xarch_log2.rs` fmt
  drift is pre-existing, untouched); `git diff --check`
  clean; `check-licensing.sh` passes; `cargo clippy` reports warnings in
  `precision.rs` (e.g. `needless_range_loop`, `unnecessary_cast`,
  `needless_lifetimes`) which are non-blocking and consistent with existing
  crate-wide lint patterns (cf. findings-register A-16; CI clippy is
  informational, non-gating); production `cargo build` unaffected (test-only
  module). (Corrected per the Mistral 4 re-review, 26 November 2026: the
  prior "no new clippy warnings" claim was inaccurate.)

## 9. Limitations and inconclusive aspects

- Single-arch (ARM64) measurement; A-5 cross-arch remains open — f32 FTZ/
  denormal behaviour on other targets is unmeasured (relevant because the
  f32 denormal range ~1e-38 is reachable by quiet bands per `05` §2).
- Real-corpus Class A mining yielded 0; synthetic Class A coverage only.
- No saturation events on-corpus (finalShift keeps energies in range).
- No per-band histogram persisted beyond the 6-bucket summary; raw corpus
  dumps were intentionally not produced (no evidence-bundle generation).
- No performance measurement (forbidden); the ~2.4%-self-time stage context
  stands as the only performance statement.
- Stereo TNS sync uses the agreed-window config (fixtures comply with
  D-022); mixed-window sync behaviour is not an A-4 surface.
- F7: shiftBits coverage is 0-only everywhere on-corpus (active-long max
  observed shiftBits 0 over 15,240 bands; inactive rows cover shiftBits = 0
  by construction). Nonzero-shift behaviour is unmeasured — a corpus gap,
  reported, not passed.

## 14. Review-findings remediation record (F1–F7, 2026-11-26)

The independent review verdict (PASS WITH FINDINGS; primary numerical
results reproduced exactly) stands; the following bounded corrections were
applied to `precision.rs` (plus this report). No production code, ADR-0002,
governance records, or unrelated work was touched.

- **F1 — TNS sync config.** The driver passed `&tables.tns_short`
  unconditionally to `tns::synchronize_pair`; production passes the right
  channel's `tconf_r`. Fixed via `sync_config()` (right-channel selection),
  proven by `f1_sync_config_selection_matches_production` (ptr-identity
  long→tns_long / short→tns_short) and
  `f1_sync_config_changes_merge_decisions` (crafted beyond-prefix coef
  difference: short-config merges, long-config declines). Mono has no sync
  call (path unchanged). Old-vs-corrected impact on the corpus: 2349 long
  stereo pairs now use tns_long, 89 short pairs tns_short (unchanged
  behaviour); **7 long-stereo frames produced different sync outcomes**
  (all music fixtures, all long windows; identities logged). Consequences:
  TNS-active frames 215→216 (+1 window moved to the active rows:
  tns-long-active 15,150→15,240 bands, inactive 199,624→199,534); right-
  channel filtered spectra differ from the old run exactly on those 7
  frames. All rescale statistics are bit-identical to the original run
  (rescale is upstream of sync). All f32-vs-f64 numerical conclusions are
  unchanged: identical maxima/locations (338/55/33/53 LSB), identical
  utilization bounds, zero flips.
- **F2 — probe semantics.** Report §7 corrected (see above): recomputed
  energies DO feed thresholds in production (post-TNS/overflow `e_l` into
  `advance_thresholds`; spread copies energies, thresholds feed pre-echo).
  The probe is explicitly a sensitivity probe on the uniform rescale basis,
  not a production mirror; threshold init threading additionally mirrors the
  pre-halving `>>=2` (production `psy.rs:1165-1173`). This round removed the
  contradictory PRE-TNS NOTE block and made overflow handling uniform: the
  Q31 energies input now receives the same `escale` shift as the three float
  arms (no-op on this corpus where `tns_spec_shift` is always 0; corpus
  downstream maxima unchanged: maxThr 32/32/8/8, maxSpread 338/338,
  maxPreEchoPrev 1/1).
- **F3 — tautology removed.** The `st64 == fraction_to_stored(e_f64)`
  assert (same construction on both sides) was deleted and replaced with a
  comment explaining why; the meaningful Ld-vs-kernel assert and the Q31
  fidelity gate remain.
- **F4 — zero-element state.** The driver now runs `advance_thresholds` per
  arm with zeros at m = 0 on all-zero frames, mirroring production's
  unconditional call, so probe `PreEchoState` progression matches
  production threading (inter-arm deltas 0 there by construction).
- **F5 — inventory.** §1/§2/§13 reconciled to this run's `git status
  --short` snapshot: 15 M + 12 ?? total (this task: `precision.rs` +
  this report + 1 line in `tests/mod.rs`; pre-existing: 14 fully
  modified + mixed `tests/mod.rs` hunks, 10 untracked including
  `a5-n2-owner-decision.md` and `xarch_log2.rs` omitted from the prior
  count). No historical counts repeated.
- **F6 — saturation boundary.** Counters now use the exact boundary
  `e_f·2^31 > MAX−0.5` (previously `>= MAX`, conservative by half an LSB).
  Numerically immaterial on-corpus (still 0); definitionally exact.
- **F7 — inactive scope.** Code comment + §4 state the limitation: inactive
  rows cover shiftBits = 0 only; measured active-long shiftBits range is
  {0} (max 0, 0 nonzero windows), so nonzero-shift behaviour is an open
  corpus gap, not a covered case.

No findings remain open within the F1–F7 scope. A-4 closure and M3
completion remain explicitly out of scope (owner decisions).

## 10. Empirical confirmation assessment

The data confirm the standing f64 decision on all three sub-questions:
(a) rescale accumulation — f32 variants stay ≥32× inside the approved
envelope with zero discontinuous events; (b) TNS-long recompute (active and
inactive) — same, with even smaller absolute deltas; (c) TNS-short —
deltas ≤53 LSB against tolerances ≥32 LSB with utilization ≤0.026. The
f32-accumulator diagnostic tracks the realistic arm within ~10%, showing
input narrowing is a minor contributor vs accumulation rounding at these
magnitudes — but both are dwarfed by the Q31 mapping/truncation terms the
envelope already budgets. No measurement gives a reason to change the
production width; two structural facts (exact i32 boundary in f64 vs 256-LSB
f32 spacing at 2^31; f64 immunity to the reachable f32 denormal range on
sentinel-sensitive quiet bands) remain decisive independently of the
measured deltas.

## 11. Decisions reserved for the owner

1. Accept this report as the A-4 empirical record (or request more coverage).
2. Confirm f64 for rescale + TNS recompute (single decision or per-kernel).
3. Set any future acceptance thresholds (stored-LSB, |ΔLd|, flip counts) —
   none were invented here; §5 numbers are reports, not gates.
4. ADR-0002 amendment policy for any future Stage-2 f32 proposal.
5. Whether the 0-Class-A-mined cell needs corpus follow-up or stands as
   synthetically covered. A-4 closure itself is explicitly out of scope for
   this task (no amendment, no M3-complete declaration).

## 12. Production / ADR-0002 / Group C-D / evidence status

Unchanged: production calculations (energy, orchestration, Q31, thresholds,
spreading, PCM, pre-echo, MS), ADR-0002 f64 decision, Group C and D
dispositions, D-R2/D-R3, Q-1, R2.2 evidence gaps, E1 schema v2 freeze and
E3 conditions. No FPE-4.1/M4, A-5, F-6, FpeError, Group F, E2, or E3 work.

## 13. Final working-tree state

Deltas vs the §1 baseline: this task's authorized test-only work
(`mod precision;` in `tests/mod.rs`; new untracked
`rust/src/aac_enc/fpe/tests/precision.rs`; new untracked `docs/fpe/FPE-3.3-
A-4-experiment-report.md`, §2) plus all pre-existing modifications (14 files
fully pre-existing + the pre-existing hunks in `tests/mod.rs`) and untracked
files (10 others), preserved byte-identically except for this task's
authorized edits (verified via `git status --short`: 15 M + 12 ?? total).
No commits, staging, or history changes.
