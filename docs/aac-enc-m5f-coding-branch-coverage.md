# M5.f — Targeted Coding-Branch Coverage Assessment

Read-only audit. No experiments run, no code or results modified.

**Status: `M5.f COMPLETE — NEXT VALIDATION TASK SELECTED WITH EVIDENCE`**

## 1. Initial state

- Branch `main`, HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91`.
- Initial `git status --short`: 34 lines (11 modified implementation files + M5.0/M5.a/M5.b/M5.c/M5.d/M5.e reports + `phase1b.rs`, `stream_driver.rs`, `m5a_decode.rs`, `m5b_measure.rs`, `cfg_44100_112000.txt`, 5 testdata WAV/provenance files, `tools/c-enc-bench/`, `tools/enc-oracle/__pycache__/`). Index empty.
- Deliverable `docs/aac-enc-m5f-coding-branch-coverage.md` verified absent before writing.

## 2. Sources inspected

Reports (read in full in M5.c/M5.d sessions; key sections re-verified here): M5.0 readiness audit, M5.a decode-check (§1 caveat, §2 matrix, §4 criteria), M5.b baseline (§2–§5, alignment + alias analysis), M5.c gap assessment (§5–§6, criteria quoted in M5.d §0), M5.d flush validation (§1 data flow, §3 19-stream inventory, F1–F3), M5.e mono-44100 (config provenance, CASES promotion, regression evidence). D-021 closure record (TNS causal chain only; not reopened, no new evidence sought).

Source (read-only greps, no execution): `stream_driver.rs` trigger taxonomy (`:736-760` frontend mismatch, `:1203-1223` qc/npass, `:1225-1231` crash note-emit, `:1264-1272` low-overlap, `:1317-1374` element/static/raw/adts errors, `:968,1140,1170,1298,1339` is_book zero-forcing, `:1116` NO_NOISE vector, `:425-447` snapshot fields, `:3915` no-parity-asserts note); `rate.rs` unit-forced retry/crash tests (`:2149,2296,2316,2506`); `bitcount.rs` PNS-off constant (`:211`) and corpus PNS-off assertion (`:1587`), IS-book branch (`:862-872`); `frame.rs` fill writer (`:970-1017`) and fill-sizing unit test (`:1356`), TNS round-trip test (`:1280`); `tns.rs` merged flag (`:269-293,733,777,836`); `adts.rs` sample-rate whitelist (`:130,160,242`); `pcm.rs` feeder contract (`:108-124,241-283`); `psy.rs` LFE early-out structural coverage (`:3450`).

## 3. Coding-branch coverage matrix

Categories: `OBSERVED — DIRECT` / `OBSERVED — INDIRECT` / `NOT OBSERVED` / `UNOBSERVABLE` / `NOT APPLICABLE`. "Observed" means recorded evidence of execution on the stated input, not mere code existence.

| Area | Module / function | Conditional vs lifecycle | Category | Evidence / missing evidence |
|---|---|---|---|---|
| PNS decision + signaling | `bitcount.rs:862-872` (`noise_nrg != NO_NOISE_PNS` branch), psy `usePns=0` | Conditional coding | `NOT APPLICABLE` (decision disabled by design); branch code `NOT OBSERVED` | Forced off at all call sites (`NO_NOISE_PNS`, `stream_driver.rs:1116`); corpus-level PNS-off assertion (`bitcount.rs:1587`, `"{name} f={f} ch={ci} PNS-off"`); live nonzero impossible through current path. Enabling it would be a feature change, not validation. |
| Intensity-stereo decision + signaling | `bitcount.rs:866-872` (`is_book != 0` branch) vs ordinary M/S (`ms_stereo_mirror`, `psy.rs:3362+`) | Conditional coding | Decision `NOT APPLICABLE` (suppressed); signaling branch `NOT OBSERVED` | `is_book` zero-forced at 5 call sites; header claims corpus-wide oracle proof of inactivity (`stream_driver.rs:33-36`); ordinary M/S path `OBSERVED — DIRECT` (masks, V4.26 counterfactual, S8 MS-cost tests). No end-to-end test pins zero-books on live output. |
| Gain-loop retry (`npass≠1`) | `rate.rs:1420` (`qc_main_frame`), `stream_driver.rs:1219-1223` | Coding w/ lifecycle consequence (skip) | Mechanism `OBSERVED — INDIRECT`; live firing `NOT OBSERVED` | Unit-forced convergence tests (`rate.rs:2296` tiny-budget, `:2506` reduced/disabled); 19-stream M5.d runs (thousands of frames incl. 1884 licensed) show zero occurrences — bounded search space, but absence of firing is not validation of the path. |
| Crash recovery (`crash≠-1`) | same sites + `:1225-1231` (note-emit, still loud-fails) | Coding w/ lifecycle consequence (loud fail) | Mechanism `OBSERVED — INDIRECT`; live firing `NOT OBSERVED` | Starvation-forced unit test (`rate.rs:2316-2338`); zero live occurrences in 19-stream evidence. Policy (abort-loud) never exercised end-to-end. |
| Fill extension | `frame.rs:970-1017`, `bitcount`/`rate` tot_fill accounting | Conditional coding | Mechanism `OBSERVED — INDIRECT`; live behavior `UNOBSERVABLE` | Sizing unit test (`frame.rs:1356`); snapshot records `tot_fill` (`stream_driver.rs:430`) but no harness persists or asserts it (M5.d F1); unknown whether any emitted frame carries fill. |
| Short-window grouping/transitions/state | `block_switch.rs`, `psy.rs` grouping, `transform.rs` | Conditional coding | `OBSERVED — DIRECT` (synthetic attacks); real-content per-stage truth `NOT OBSERVED` | Seq exact vs oracle, grouped spectra exact (D-021), SHORT MS-grouping tests, Tier-B attack attributions (M5.26), 15-fixture M52 + 4-fixture M5.e runs all SHORT-capable and clean. M55 licensed transitions: accounting-only (`stream_driver.rs:4257`). |
| TNS decision/filter/signaling | `tns.rs:711,804,855`, `psy.rs` TNS, `frame.rs:825+` TNS write | Conditional coding | `OBSERVED — DIRECT` | Live order flips + filter application + energy recompute (V4.29–V4.31), exact +18-bit attribution (V4.29), S8 round-trip (`frame.rs:1280`), gain-gate + Schur tests. D-021 closed; not revisited. |
| TNS `merged` flag / low-overlap shapes | `tns.rs:269-293,777,836`, `stream_driver.rs:1264-1272` | Conditional coding | `NOT OBSERVED` | Live `merged=true` assignment exists (`:777`) but no evidence of firing or non-firing end-to-end; LO-shape has a loud gate (`:1264`) with zero observations (gate exists, event absent — gate validated only as code). |
| MS decision/signaling | `ms_stereo_mirror`, `static_bits.rs:266,279`, S8 MS write | Conditional coding | `OBSERVED — DIRECT` | Per-window masks, V4.26 1-LSB counterfactual, S1/S8 MS-cost tests, digest/mask split in `phase1_full_stream_parity`. |
| SCF/quant/noiseless/bitenc/frame bodies | `scf.rs`, `quant.rs`, `bitcount.rs`, `bitenc.rs`, `frame.rs` | Conditional coding | `OBSERVED — DIRECT` | Corpus-exact tests (S4 `:1520`, S5 `:1562`, S6 `:2149`, S7 probe `:1144`/payloads `:1228`, S8 elements `:1395`), 287-vector probe transcript, CV-coverage asserts. |
| ADTS framing/transport | `frame.rs:1127`, `adts.rs` | Lifecycle-adjacent coding | `OBSERVED — DIRECT` | Per-frame re-parse in m52/m5a, header reference tests, 19-stream M5.d runs, successful third-party decode with empty stderr. |
| Flush/termination | `pcm.rs:257-283`, decoder trim | Lifecycle | Covered by M5.d (rule `tail=(1600+D)−Z`, exact on 3 fixtures) | Kept distinct from branch coverage per brief; referenced, not re-audited. |
| Decoder internals | `aac-dec.c` | Reference tool, out of scope | — | Used as validator under the M5.a exit-0 caveat; its paths are not assessed. |

Correctness vs coverage vs quality: unobserved retry/crash/fill/merged paths are **correctness/coverage** gaps (behavior on trigger unknown); IS/PNS are **coverage hygiene** (disabled paths); nothing here is a quality uncertainty (no perceptual claim depends on these branches).

## 4. What the four fixtures demonstrably establish (and what they don't)

Click, onset, sine441, onset-mono-44100 (plus the 19-stream M5.d context) establish: LONG+SHORT windows, attack transients, silence halves, both rates/channels, three flush-overshoot variants (Z=1600/2176/1980), TNS order activity, MS decisions in both states, single-pass S6 on every observed frame, complete ADTS + clean decode, cross-run determinism. The 19-stream runs additionally bound the retry/crash search: thousands of frames, zero triggers — a defined, finite negative observation.

They do not establish: PNS/IS behavior (unreachable by construction — fixture choice is irrelevant); retry/crash/fill correctness (never fired; triggering is a property of bitrate-starved or adversarial content, none present); merged-TNS or LO-shape handling (no firing evidence on any fixture); near-threshold behavior (every observed frame converges first-try — distance to the retry boundary is unmeasured); real-content transition per-stage truth (M55 accounting only). Fixture presence never implied branch triggering (IS/PNS branches exist alongside 19 clean streams); triggering never implied validation (TNS flips required bit-math + counterfactuals, D-021); decoding never implied decision correctness (M5.a gates are structural); Rust/C diffs never implied defects (D-021 finite-precision account).

## 5. Lifecycle triggers vs coding-branch coverage

M5.d's inventory is a **lifecycle** taxonomy: which skip/note/panic events fire per stream during termination-inclusive runs (answer: none in 19 streams). Coding-branch coverage asks which **conditional coding paths** execute and behave correctly. They overlap exactly where a coding path has lifecycle consequences: retry (SKIP-C), crash (NOTE-EMIT), fill (accounting only, no gate), frontend mismatch (SKIP-A). They are otherwise disjoint: IS/PNS/TNS-merged/MS-orders/LO-shapes execute (or don't) with zero lifecycle signal — 19/19 clean lifecycle runs coexist with at least six never-fired coding branches (§3). The lifecycle inventory therefore does **not** cover coding branches; treating it as coverage would be the precise overstatement this matrix is built to prevent.

## 6. Five ranked gaps

1. **Retry/crash trigger envelope + abort policy unvalidated on real content (correctness).** Paths exist, are unit-forced, but have zero live observations; first real trigger aborts the stream by current policy, which no end-to-end evidence has ever exercised. Invalidation risk: API milestone would bake in an untested abort. Evidence: §3 rows + 19-stream zero-observation bound.
2. **No end-to-end branch-execution census (infrastructure).** Per-frame `npass/crash/tot_fill`/book-presence are recorded in `FrameSnapshot` then dropped; harnesses assert only notes-emptiness. Without recording, gaps 1, 3, and 5 cannot even be quantified. Prerequisite for all branch validation; closable with a tiny deterministic test.
3. **Fill-bit end-to-end behavior unknown, no gate (correctness/accounting).** M5.d F1 carried: mechanism unit-tested, live values unrecorded, no loud gate located. A nonzero-fill stream would pass every current gate silently.
4. **TNS `merged` firing + LO-shape live handling + real-content transition per-stage truth (coverage).** Narrow: gate exists for LO, flag exists for merged, neither observed; licensed transitions accounting-only. Lower impact (TNS core deeply validated; D-021 closed).
5. **IS/PNS dead-code unreachability not test-pinned end-to-end (coverage hygiene).** Construction + oracle-corpus proof exist; no live-output assertion. Matters only if a future change enables these paths — defense in depth, lowest urgency.

## 7. Recommended next task (exactly one)

**Narrow test-only branch-execution census over the 19 M5.d streams** (observability, not a behavior change).

- **Question answered:** which conditional coding branches execute on the full current corpus, with what per-frame values — the first quantified execution record (today: zero).
- **Why first:** prerequisite for gaps 1, 3, and 5 (cannot characterize triggers, fills, or book-presence without recording them); deterministic and tiny; numerics-untouchable by construction (read-only taps on existing snapshot structs); directly informs the production-API handle-vs-abort decision with an actual behavior envelope (e.g., gain/pass margins, `tot_fill` distribution) instead of assertions.
- **Smallest scope:** persist per-frame `(frame, npass, crash, tot_fill, is_book_any, noise_book_any_non_PNS, seq, total_bits)` from the existing `FrameSnapshot` for the 19 streams; assert current policy (`npass==1`, `crash==-1`, notes empty — already asserted; **add** `tot_fill` accounting consistency + zero-book assertions); emit one census table. No PNS/IS enabling, no retry forcing, no tuning, no new fixtures, no API/CI work.
- **Acceptance:** census produced for all 19 streams; all asserts pass; ADTS SHA-256 identical to M5.a/M5.b/M5.e records (proves read-only); M5.b numbers unchanged; report records the distributions (e.g., `tot_fill==0` everywhere or not, gain ranges, any book presence).
- **Out of scope:** forcing retry/crash (adversarial, needs this census as baseline); enabling PNS/IS (feature change); per-stage transition truth on licensed content; perceptual claims.
- **Further-investigation triggers:** any nonzero book, any `npass≠1`/crash, any `tot_fill` anomaly, any hash movement → escalate under M5.c rules (defect investigation, not tuning).

## 8. Deferred tasks and why not now

- **Retry/crash forcing characterization:** needs the census baseline first and has open-ended adversarial cost; the census bounds the problem first.
- **TNS-merged/LO hunt + real-content transition truth:** narrower and lower-risk than the census payoff (TNS core already deeply evidenced); the census will incidentally record the first merged-firing evidence if any exists.
- **IS/PNS standalone pinning:** subsumed — the census includes the zero-book assertions end-to-end, closing gap 5 as a side effect.
- **Production API slice:** requires the census + policy data to decide handle-vs-abort semantics; building it on unmeasured behavior repeats the M5.d warning.
- **CI gate:** gates pinned behavior; the census creates the first pin-worthy branch-behavior record — CI follows, not precedes.
- **Corpus expansion / perceptual work:** explicitly not the default answer; no evidence calls for either.

## 9. Uncertainties and limitations

- A census records execution, not correctness: retry/crash math remains evidenced by unit-forced tests, not by live firing (which has never happened).
- This audit is confined to existing evidence; the 19-stream zero-trigger bound is finite (synthetic-heavy corpus + 4 licensed streams).
- Decoder-side paths are out of scope; `aac-dec` remains a validator under its documented caveat.
- No statement is made about configurations outside mono/stereo × 48/44.1k × VBR-5 ADTS.

## 10. Final state and non-interference confirmation

- HEAD `53481f3a`, branch `main` — unchanged throughout (verified before and after).
- Final `git status --short`: 35 lines = 34-line initial + this report; all other lines verified identical to `/tmp/m5f-status-initial.txt`; index empty (0 staged).
- No experiments run (no encoder/decoder/test/benchmark/fixture-generator invocations); no source, test, fixture, configuration, instrumentation, or report modifications — the sole filesystem effect is this new file. Pre-existing work preserved exactly.
