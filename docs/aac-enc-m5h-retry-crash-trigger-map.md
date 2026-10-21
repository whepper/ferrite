# M5.h — Retry/Crash Trigger-Threshold Mapping

Read-only audit and test plan. No code, test, fixture, or report modifications; no executions.

**Status: `M5.h COMPLETE — TRIGGER MAP AND NEXT TEST SELECTED`**

## 1. Initial state

- Branch `main`, HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (verified before and after; unchanged).
- Initial `git status --short`: 37 lines (11 modified implementation files + M5.0–M5.g reports + `phase1b.rs`, `stream_driver.rs`, `m5a_decode.rs`, `m5b_measure.rs`, `m5g_census.rs`, `cfg_44100_112000.txt`, 5 testdata WAV/provenance files, `tools/c-enc-bench/`, `tools/enc-oracle/__pycache__/`). Index empty.
- Deliverable verified absent before writing. No staging, commits, or any other git operations.

## 2. Sources inspected

Reports (full text in-context from prior milestones; key sections re-verified): M5.f §3/§7–9, M5.g §1/§3–5/§8, M5.d §1/§3, M5.b §2–§5. Source (read-only): `rust/src/aac_enc/rate.rs` (`qc_main_frame` `:1420-1768`, `reduce_bit_consumption` `:990-1030`, `crash_recovery` `:1032-1130`, `exit_matrix_step` `:1139-1190`, `QcError` `:308-314`, constants `:221-242`, forced tests `:2239-2430`, corpus test `:2149-2170`); `rust/src/aac_enc/stream_driver.rs` policy gates (`:1219-1231` npass/crash, `:1203-1204` qc-error, `m52_emit_or_fail`).

## 3. Retry/crash control flow (traced, not assumed)

Terminology as implemented (kept distinct): a **retry** is one gain-adjust + re-quantize cycle (`reduce_bit_consumption` → `GainBumped`, `iterations += 1`); the **pass count** `npass` is the number of outer-loop `PassTrace` entries (`passes.len()`); a **crash mark** (`crash_mark`, `-1` = none) is the *surviving* `maxSfbPerGroup` after bit-saving band-drops — not a program crash; a **fallback** is the all-gains-`+1` path taken when nothing can be saved; terminal outcomes are converged-emit, `QcError` skip, or runaway panic (bug tripwire).

| Mechanism | Trigger predicate (source) | Inputs / state | On true | Continuation |
|---|---|---|---|---|
| MAX_QUANT re-quant retry | `max_all > MAX_QUANT` (=8191, `rate.rs:223`/`quant.rs:235`) after `quantize_spectrum` (`:1627-1640`) | quant spectrum, `max_sfb[ch]`, gains, scf | `ch_ok[ch]=false`, `constraints=false`, `decrease=1` | `reduce_bit_consumption` bumps gains on failing channels (`:1003-1009`), `recalc`, re-quantize |
| Element-cap retry | `el_dyn > nch*MIN_BUFSIZE_PER_EFF_CHAN − el_static` (6144/ch, `:1674`) | counted dyn bits, static bits | `constraints=false` | outer loop repeats (gain loop re-entered) |
| Exit-matrix retry | `sum_bits > total_avail` (over budget), or `used_dyn > max_dyn` overshoot even under budget, or `!done` (`:1709-1727`, cases unit-tested `:2239`) | `sum_bits`, `total_avail`, `used_dyn`/`max_dyn`, `decrease`, `iterations` | `done=false` (and/or `decrease=1`, `constraints=false` reset) | new outer pass (`npass+1`) |
| Crash path | `iterations == max_iterations` (=5, `:242`) AND `el_dyn > 0` (`:1010-1024`) | iteration count, dyn usage, `bit_res_level_el + el_granted`, `max_bits_el` | `CrashRequired{bits_to_save}`: `bits_to_save = max(el_dyn+8−(bitres+granted), el_dyn+static+8−max_bits_el)` | if `>0`: `crash_recovery` drops `max_sfb` top-down until `savedBits >= bits_to_save`, `crash_mark = surv` (`:1583-1598`); else all-gains-`+1` fallback (`:1600-1602`) |
| QuantError abort | `iterations == max` with `el_dyn == 0` (`:1011-1013`); `iterations > max` (`:1025-1027`); two earlier grant/distribute errors (`:893`, `:1284`) | same as above | `Err(QcError::QuantError)` | driver SKIP-B: skip + loud note (`stream_driver.rs:1203-1204`) |
| BitresTooLow abort | grant/distribute preconditions (`:890`, `:934`) | reservoir/grant state | `Err(QcError::BitresTooLow)` | driver SKIP-B, same path |
| Outer-guard panic | `outer_guard >= MAX_OUTER_PASSES` (=25, `:240`) | pass count | `panic!` (`:1544-1546`) | run aborts (test failure); documented as implementation-bug tripwire (`:190-192`), untestable by design |
| Driver policy: multi-pass | `passes.len() != 1` (`stream_driver.rs:1219-1223`) | `npass` | note + `return None` | stream loud-fails at `m52_emit_or_fail` |
| Driver policy: crash | `snap.crash != -1` (`:1225-1231`) | `crash_mark` | note, **still emits** | stream loud-fails ("notes on emitted frame") |
| Dead arms (stub) | `check_min_frame_demand()` always returns 1 (`:1132`; stub noted `:125`) | — | `!check_min_frame_demand()` arms (`:1173`, `:1179`) never take | No test needed; recorded as constant-folded |

`crash_mark` semantics confirmed: `-1` = no recovery ran; `≥0` = surviving `maxSfbPerGroup` shared by all channels (`crash_recovery` docs `:1032-1037`). `decrease` is the gain-loop direction flag (`1` = up/constrained, `0` = down), not a retry count. `iterations` counts gain adjustments, not outer passes.

## 4. Forced-test audit (M5.f citations verified)

- `s6_retry_forced_by_tiny_budget` (`:2275`): 2000-bit buffer on noise-mono f5 forces `passes.len() > 1` (exit-matrix reset path) and converges with `total_bits ≤ 2000` plus the total-identity (`static+dyn+fill+align+GLOBAL_EXT_BITS`). Validates retry **outcome** (convergence within budget) and accounting under retry. Does not verify crash (stays −1), per-pass state transitions beyond convergence, or the driver skip that a multi-pass frame would hit live.
- `s6_crash_forced_by_starvation_budget` (`:2316`): 1200-bit buffer forces `passes.len() > 1` and `last.crash >= 0` (band-drop + static rebalance). Validates crash-path **execution and convergence**. Does not verify the dropped bands' audio consequence, the driver note-emit path, or the `+1`-fallback-vs-recovery selection on live data.
- `s6_exit_matrix_cases` (`:2239`): decision table (under/over budget, direction, overshoot retry, emergency exit) — validates **outcomes** of `exit_matrix_step`, not loop integration.
- `s6_reduce_and_bitres_errors` (`:2360+`): forces all three `QuantError` producers and both `BitresTooLow` producers — validates **error outcomes** at source. Does not verify any driver-side handling beyond the shared SKIP-B gate (unit-pinned with synthetic inputs only, `m52_emit_or_fail_is_loud`).
- `s6_corpus_exact` (`:2149`): 20 committed frames assert `npass` and per-pass `crash` (`:2108,2120`) — all single-pass, crash −1. Validates normal-path parity **including** the policy fields.
- Forcing-setup caveat: the starvation/tiny-budget tests forge `DistributeState`/budgets synthetically (M5.0-noted forging pattern); "branch executes under setup" is established, "reachable by real audio" is not.

## 5. M5.g correlation (recorded results only, not rerun)

M5.g census artifact (`$TMPDIR/m5g-census/census.json`, 19 streams / 2442 rows) records: every row `npass==1`, `crash==-1`, zero books, `adts_len*8 == 56+total` with zero violations; 231 fill rows (all exactly 7 bits); all ADTS hashes byte-identical to M5.a/M5.b/M5.e records. This confirms zero retry/crash firings in the bounded corpus (including 1884 licensed real-content frames) and proves the census fields are live and sensitive (fill variation + the sine-f42 exact-match row demonstrate resolution).

Census detection limits (from its recorded fields): it **can** detect any pass-count deviation, any crash mark, any fill, any book presence, and any accounting-identity break. It **cannot** infer: near-threshold approaches (`iterations`, the inner gain-adjust count, lives in `PassTrace` but was not recorded — a frame at `iterations=4` with `npass==1` is invisible as near-threshold); which retry sub-branch *would* fire; element-cap or emergency-exit proximity; `+1`-fallback vs recovery selection (never reached); `QcError` near-misses. Zero events is therefore evidence of non-firing in-corpus, not of unreachability and not of margin.

## 6. Trigger map

| Mechanism | Trigger condition | Relevant inputs/state | Existing evidence | M5.g live observation | Outcome | Validation gap | Safe testability | Recommended test |
|---|---|---|---|---|---|---|---|---|
| MAX_QUANT re-quant retry | `max_all > 8191` post-quant | spectrum, `max_sfb[ch]`, gains, scf | corpus (never fires: all pass first-try) | Not observed (2442/2442 first-try) | gain bump + re-quantize, `iterations+1` | Firing threshold on real audio unknown | Yes — unit | None new (covered by corpus non-firing + tiny-budget forcing) |
| Element-cap retry | `el_dyn > nch*6144 − el_static` | counted dyn/static bits | unit paths via forced tests | Not observed | outer repeat | Same as above | Yes — unit | None new |
| Exit-matrix retry (new pass) | over-budget / overshoot / `!done` | `sum_bits`, `total_avail`, `used_dyn/max_dyn`, `decrease`, `iterations` | `s6_exit_matrix_cases` (table) + tiny-budget (integration) | Not observed (`npass==1` everywhere) | `npass+1`, constraints reset | Live firing unseen; proximity unmeasured | Yes — unit + census extension | **Margin census (§7)** |
| Crash recovery | `iterations==5`, `el_dyn>0`, `bits_to_save>0` | budgets, grants, reservoir | starvation test (execution+convergence) | Not observed | band-drop, `crash_mark=surv≥0`, note-emit | Live firing unseen; driver note-emit path never runs on real output | Yes — unit; live forcing adversarial | Margin census first; forcing only if margins demand |
| +1 fallback | same, with `bits_to_save≤0` | same | code path, no dedicated forced test | Not observed | all gains `+1`, `recalc` all | Selection vs recovery on live data unknown | Yes — unit (synthetic) | Defer unless margins implicate |
| QuantError / BitresTooLow | iterations-exceeded/empty-dyn; grant preconditions | budgets, `el_dyn` | forced error tests (outcomes at source) | Not observed (no error in corpus) | driver SKIP-B (gate unit-pinned only) | Driver mapping never runs on real QC output | Partial — gate fn covered; integration needs forcing | Margin census (distance-to-error); no new error tests |
| Outer-guard panic | `outer_guard≥25` | pass count | none (tripwire) | Not observed | panic / test failure | None (by design) | No (would panic) | None |
| Driver multi-pass skip | `passes.len()≠1` | `npass` | gate fn synthetic test only | Not observed | skip + loud fail | Never executed with real QC output | Needs real multi-pass frame (forcing) | Covered by margin census as detector |
| Driver crash note-emit | `crash≠-1` | `crash_mark` | gate fn synthetic test only | Not observed | emit + loud fail | Same as above | Same as above | Same as above |
| Dead stub arms | `!check_min_frame_demand()` | constant `1` | stub doc | Unobservable (constant) | never taken | None | N/A | None |

## 7. Recommended next validation task (exactly one)

**Pass-margin census extension (read-only observability, no behavior change).** Record per frame, on the same 19 streams via the existing M5.g harness pattern: `iterations` (inner gain-adjust count, already in `PassTrace`), `el_dyn` vs `el_granted`/`max_dyn` margins, and `sum_bits` vs `total_avail` margin — all already computed inside `qc_main_frame`/`step_frame`, requiring only additive snapshot fields like M5.g. Report the minima/distributions; assert nothing new except output-hash stability (ADTS byte-identical to M5.a–M5.g records) and the existing M5.g asserts unchanged.

- Precise question: how close does any of the 2442 corpus frames come to a retry, crash, error, or emergency-exit threshold?
- Why first: it is the only item that shrinks the top residual risk (unmeasured proximity) deterministically and cheaply; it reuses proven instrumentation; it cannot perturb numerics; and its output (margin minima per class) is the evidence that decides whether any forcing work is justified at all.
- Assertions: census completes on all 19 streams; all M5.g asserts still pass; ADTS hashes unchanged; margin table published. No invented thresholds (report-only, M5.b precedent).
- Further investigation triggers: any frame with `iterations ≥ 3` (of max 5), any `el_dyn` within 10% of its grant/cap, any nonzero `bits_to_save` pressure signal — these reported minima (not preset gates) motivate a targeted forcing task; persistent clean margins close the urgency.
- Out of scope: forcing retry/crash, tuning, PNS/IS, perceptual work, API, CI, new corpus.

## 8. Deferred tasks and rationale

- **Live retry/crash forcing:** adversarial and open-ended; forbidden-ish to manufacture ("do not manufacture artificial input conditions"); only justified if §7 margins show proximity. The forced unit tests already cover mechanism + convergence.
- **Driver-seam test with crafted snapshots:** marginal value — the gate function is already unit-pinned; the untested part is *reaching* it with real QC output, which only forcing or margins address.
- **New QcError tests:** source outcomes already forced-tested; integration shares the forcing problem above.
- **Merged-TNS/LO + real-content transition truth (M5.f #4):** lower rank, independent track; untouched by this task.
- **API slice / CI:** need policy + margin data first; sequencing unchanged from M5.c/M5.f.

## 9. Uncertainties and limitations

- Thresholds are source-derived (8191 / 6144 / 5 / 25 / stub-1); C-fidelity of the ported predicates is assumed from the M4 corpus work, not re-verified here.
- Forced-test setups are synthetic (forged budgets/states); execution-under-setup ≠ live reachability, in both directions.
- `crash_mark ≥ 0` values were never observed, so the surviving-`maxSfb` semantics rest on code reading + the starvation test's assertion, not on diverse samples.
- No statement is made about uninspected rate-control helpers beyond their role in the traced paths.

## 10. Final state and non-interference confirmation

- HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91`, branch `main` — unchanged.
- Final `git status --short`: 38 lines = 37-line baseline + this report; all other lines verified identical to `/tmp/m5h-status-initial.txt`; index empty (0 staged).
- No tests or experiments were executed (no `cargo`, encoder, decoder, benchmark, or fixture-generator invocations in this session); no source, test, fixture, configuration, instrumentation, or report file was modified — the sole filesystem effect is this new file. D-021 not touched.
