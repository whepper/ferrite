# M5.c — Encoder Validation Gap Assessment and Next-Step Decision

Audit and planning only. No experiments run, no files changed except this report.

**Completion status: `M5.c COMPLETE — NEXT TASK SELECTED WITH EVIDENCE`**

## 1. Revision and exact worktree status

- Branch `main`, HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (D-021 archive commit; unchanged throughout this audit).
- `git status --short`: 30 lines — 11 modified (`docs/aac-enc-migration-spec.md`, `rust/src/aac_enc.rs`, `bitcount.rs`, `bitenc.rs`, `energy.rs`, `psy.rs`, `quant.rs`, `rate.rs`, `scf.rs`, `testdata.rs`, `tns.rs`), 19 untracked (4 benchmark/quality/divergence docs + M5.0/M5.a/M5.b reports, `phase1b.rs`, `stream_driver.rs`, `m5a_decode.rs`, `m5b_measure.rs`, 5 testdata WAV/provenance files, `tools/c-enc-bench/`, `tools/enc-oracle/__pycache__/`). This is the 28-line M5.b-end baseline plus the 2 M5.b files — verified line-by-line against the M5.b §8 record, not by count alone.
- Unstaged diff: 11 files, +1571/−280 (the +2 over M5.b's +1569 is the `m5b_measure` registration line). Nothing staged (`git diff --cached` empty). No commits, stashes, resets, cleans, restores, amends, or pushes.

## 2. Sources inspected

- `docs/aac-enc-m5-readiness-audit.md` (M5.0, full), `docs/aac-enc-m5a-decode-check.md` (M5.a, full), `docs/aac-enc-m5b-objective-quality-baseline.md` (M5.b, full).
- Test definitions (read, not run): `rust/src/aac_enc/m5a_decode.rs` (`m5a_decode_check`, `m5a_mono44100_blocked_by_missing_cfg`, `emit_pcm_as_adts` recipe, `run_decode` stderr-empty gate); `rust/src/aac_enc/m5b_measure.rs` (`m5b_dump_inputs`); `rust/src/aac_enc/stream_driver.rs:35-56` (scope header), `:468,499,608,801` (`StreamEncoder`), `:1203-1230,1248` (skip/retry/crash/fill notes), `:950-970,1116-1298` (`is_book` zero-forcing), `:4029` (`m52_write_adts_stream`), `:4115` (`m52_emit_or_fail_is_loud`), `:4257,4316` (M5.5/M5.2 corpus tests); `rust/src/aac_enc/pcm.rs:108-233` (feeder contract, `finish`, `AlreadyFinished`), `rust/src/aac_enc/phase1b.rs:104-135` (class-5 oracle-gap reporting); `rust/src/aac_enc/testdata.rs:491` (`load_cfg` expect), `:516` of `stream_driver.rs` (mono-44100 `load_cfg(44100, 112000)` call).
- Implementation references: `aac-dec.c:26-139` (CLI/exit-code caveat already established in M5.a), `aac-enc.c:36-205` (C reference path).
- Negative checks: no `.github/workflows/`, no `.gitlab-ci.yml`, no `Jenkinsfile` (verified this session); committed `cfg_*.txt` set contains `cfg_44100_96000.txt` but no `cfg_44100_112000.txt` (per M5.a §2; not re-listed here to avoid filesystem churn — accepted from the M5.a record, which ran against this same HEAD).
- D-021 status confirmed closed (`docs/research/d021/D021-CLOSURE-RECORD.md` at commit `53481f3a`); M5.b rust-vs-c residues (53–63 dB, §3) corroborate rather than contradict the closure. No reopening evidence; none sought.

## 3. M5.a ↔ M5.b consistency check (no conflicts found)

- Frame counts 25/49/45, ADTS hashes (`a9d7b395…`, `702c8717…`, `aa906b52…`), decoded counts 23856/48432/44395, and decoded hashes (`6a359695…`, `4d729444…`, `aecb7b97…`) are identical across both reports; M5.b regenerated the Rust artifacts by re-running the M5.a harness and matched bit-for-bit (M5.b §4/§7).
- Trim arithmetic closes: nominal lag −1600 with overlaps 22256/46832/42795 satisfies 1600 + overlap = decoded N in all three cases; M5.b tail exclusion (≈1744/1744/1685) matches M5.a §5 D′ exactly.
- Bitrate figures recompute from reported bytes/durations (42.88/18.87/23.98 and 43.82/18.54/24.11 kbps); the ±2.2% agreement claim holds (worst case 2.1% on click-mono).
- Two scope deviations from M5.0's M5.b proposal, both documented in M5.b §6 (not silent): no licensed-WAV content and no per-frame size histograms. Residual sub-gap, not a conflict.
- Criterion evolution, not contradiction: M5.0's exact delay-contract match → M5.a plausibility window → M5.b nominal −1600 with the onset alias analysis. Precision increased; no report asserts anything another denies.

## 4. Validation-gap matrix

Label set: `KNOWN FAILURE` / `VALIDATION GAP` / `INFRASTRUCTURE BLOCKER` / `QUALITY UNCERTAINTY` / `OUT OF SCOPE FOR CURRENT MILESTONE` / `ADEQUATELY COVERED FOR CURRENT SCOPE`. Per instruction, untested paths are gaps, never failures.

### 4.1 Stream lifecycle

| Gap | Classification | Evidence |
|---|---|---|
| Empty-input end-to-end (ADTS emit + decode) | `VALIDATION GAP` | Feeder counts covered (`pcm.rs` empty test per M5.0 §3); no encode/decode evidence for zero-length streams. |
| Sub-frame/short-input end-to-end | `VALIDATION GAP` | Frame-count law covers short inputs as counts only; shortest M52 fixture is multi-frame; no decoded-output evidence. |
| Partial-final-frame payload + flush correctness | `VALIDATION GAP` | Counts + zero-pad covered (`pcm.rs:438,457`, `m52_all_fixtures_write_complete`); payload ground truth absent; M5.25 11/18 final-frame payload diffs unresolved; M5.b excludes trimmed tails from metrics (§5). |
| In-process state reset / back-to-back streams | `ADEQUATELY COVERED FOR CURRENT SCOPE` | Fresh-state-per-stream design; cross-run determinism demonstrated three ways (M5.b §4); no reusable-stream API exists yet, so reuse semantics are moot until the API milestone. |

### 4.2 Configuration and coverage

| Gap | Classification | Evidence |
|---|---|---|
| mono/48k, stereo/48k, stereo/44.1k cells | `ADEQUATELY COVERED FOR CURRENT SCOPE` | M5.a PASS ×3 (exit 0 + empty stderr + structural WAV + determinism); M5.b baseline (SNR/bitrate/integrity/repeatability). 44.1k trim rests on one fixture — thin but documented. |
| mono/44.1k cell | `INFRASTRUCTURE BLOCKER` | No fixture key + missing `cfg_44100_112000.txt` → `StreamEncoder::new(44100, 1)` panics at `testdata.rs:491` (M5.a §2; pinned by `m5a_mono44100_blocked_by_missing_cfg`). Not an encoder failure. |
| Unsupported-config rejection behavior (other rates/layouts) | `VALIDATION GAP` | No negative tests cited anywhere; driver asserts single-element mono/stereo but rejection paths untested. |
| Other sample rates / >2ch / other modes | `OUT OF SCOPE FOR CURRENT MILESTONE` | 4-cell VBR-5 ADTS matrix is the declared scope (M5.0 §3); expansion needs a matrix-declaration task first, not ad-hoc testing. |

### 4.3 Psychoacoustic and coding branches

| Gap | Classification | Evidence |
|---|---|---|
| PNS live path | `VALIDATION GAP` | Asserted absent per frame (`stream_driver.rs:56`, `NO_NOISE_PNS`); zero live triggers observed anywhere. |
| Intensity stereo live path | `VALIDATION GAP` | `is_book` zero-forced (`stream_driver.rs:968,1140,1170,1298`); live nonzero is a loud finding (`:35`) that has never fired. |
| Retry / crash / large-fill paths | `VALIDATION GAP` | Loud-skip design (`stream_driver.rs:1203-1230,1248`); `rate.rs` corpus all single-pass (`npass=1,crash=-1` per M5.0 §2); never exercised on real content. First trigger aborts the stream. |
| Short-window transitions, synthetic attacks | `ADEQUATELY COVERED FOR CURRENT SCOPE` | Block-seq exact (`block_switch.rs:600` per M5.0), D-021/V4.30-34 live onset probes, M5.26 Tier-B attack attributions with exact gates. |
| Transitions on real (licensed) content | `VALIDATION GAP` | M55 corpus runs assert validity/accounting only (`stream_driver.rs:4257`, "explicitly no byte-parity" per M5.0 §3). |

### 4.4 Output correctness

| Gap | Classification | Evidence |
|---|---|---|
| Decoder-failure detection (exit-0 caveat) | `ADEQUATELY COVERED FOR CURRENT SCOPE` | `run_decode` requires exit 0 **and** empty stderr; all 9 M5.b decodes clean; asserted in code, not just documented (M5.a §1/§7). |
| Framing, sequencing, counts, delay semantics (3 cells) | `ADEQUATELY COVERED FOR CURRENT SCOPE` | Header re-parse per frame, count law, nominal −1600 lag with alias analysis, trims 1744/1744/1685 (M5.a §5, M5.b §5). |
| Determinism / repeatability (3 cells) | `ADEQUATELY COVERED FOR CURRENT SCOPE` | Encoder, C encoder, and decoder all byte-reproducible across runs/days (M5.b §4). Corpus-wide determinism beyond the 3 cells unmeasured — noted, not blocking. |

### 4.5 Objective quality

| Gap | Classification | Evidence |
|---|---|---|
| Rust-vs-orig / C-vs-orig SNR + bitrate + integrity (3 cells) | `ADEQUATELY COVERED FOR CURRENT SCOPE` | Blunt-metric agreement ≤0.5 dB, bitrates ±2.2%, zero clips, repeatable (M5.b §2–§4). Baseline by design, no thresholds. |
| Rust-vs-C residue | `ADEQUATELY COVERED FOR CURRENT SCOPE` | 53–63 dB below codec error, lag-0 aligned, consistent with D-021 finite-precision account (M5.b §3). |
| Alignment method + onset trap | `ADEQUATELY COVERED FOR CURRENT SCOPE` | Documented, edge-verified (−1600 vs alias −1504 with 10021→135 collapse, M5.b §5). Reusable precedent for future metrics. |
| Perceptual competitiveness (masking/PE/NMR/listening) | `QUALITY UNCERTAINTY` | Explicitly unmeasured by design (M5.b §6); SNR must not be read as quality (M5.b §3). Not a failure — a standing uncertainty that bounds what may be claimed. |
| Real-content (speech/music) objective numbers | `VALIDATION GAP` | Licensed WAVs committed but unmeasured (M5.b §6 scope deviation); M55 runs give no signal metrics. |

### 4.6 Production readiness

| Gap | Classification | Evidence |
|---|---|---|
| No production `Encoder` API (test-gated only) | `INFRASTRUCTURE BLOCKER` | Full chain runs only under `#[cfg(test)]` (`stream_driver.rs`, `phase1b.rs`, `testdata.rs` per M5.0 §2); nothing to release, fuzz, or integrate. Blocks usability/release — not a correctness verdict on the numerics. |
| CI gate + toolchain/reproducibility pins | `INFRASTRUCTURE BLOCKER` | No workflow files exist (verified §2); benchmarks untimed/opt-in; C toolchain unpinned beyond version strings in reports. Regressions and drift have no tripwire. |
| Release criteria definition | `OUT OF SCOPE FOR CURRENT MILESTONE` | Undefined (M5.0 §4); must precede any release claim but is process, not evidence — correctly deferred, explicitly noted. |

## 5. Five highest-priority gaps

1. **Flush/termination payload + skip policy (correctness).** Could emit truncated or invalid stream tails; the driver aborts whole streams on first skip. Evidence: M5.25 11/18 final-frame diffs, loud-skip code paths, M5.b tail exclusion. Unresolved, it invalidates the API milestone (which would bake in abort semantics) and any release claim.
2. **Live-branch triggers on real content — PNS/IS/retry/crash/fill (correctness).** First trigger on real audio is an unhandled abort today. Evidence: asserted-absent paths with zero live observations; licensed corpus already committed and ready. High impact, bounded cost.
3. **Mono-44.1k infrastructure (completeness).** One matrix cell untestable for want of a committed config snapshot + fixture key. Evidence: panic location identified, recipe preserved in-harness. Cheapest task on this list; ranked third only because no failure signal attaches to it.
4. **No production API (usability/release).** Nothing shippable; production-path validation impossible until it exists. Ranked below 1–2 because API work proves usability, not correctness — but it gates fuzzing, integration, and every downstream claim.
5. **No CI/regression gate (durability).** Every result to date rests on manual runs; silent drift possible. Ranked fifth only because there is currently little concurrent development to drift against — it becomes critical the moment 1–4 produce code or config changes.

## 6. Recommended next task (exactly one)

**Bounded flush/termination + skip-policy validation** (M5.b §7 proposal, adopted).

- Scope: the three measured fixtures only. (a) Per-frame payload ground truth for final frames: Rust-vs-C frame-lens and payload-byte comparison over the last 3 frames plus decoded-tail comparison under the documented trims. (b) Trigger enumeration: run the existing 15-fixture M52 + 4-licensed M55 corpus through the current path, record every skip/note event with fixture + frame + stage, and classify each trigger. (c) Policy decision recorded per trigger class: handle vs abort-loud, with tests pinning the decision. No tuning, no API, no corpus expansion, no mono-44.1k, no perceptual metrics.
- Pass/fail criteria: (1) final-frame payloads reconciled (exact match) or bounded with a named mechanism per fixture; (2) trigger inventory complete over 19 streams with zero unexplained events; (3) every M5.b number in §2–§4 unchanged (regression floor, modulo documented alignment); (4) all decoded outputs (including final frames) pass the M5.a stderr-empty + structural gates.
- Expected artifacts: one report + minimal test-gated enumeration/reconciliation code + trigger table; all generated audio stays outside the repo.
- Further-investigation triggers: any undecodable final frame, any skip on licensed content without an identified stage cause, any M5.b-number movement, or any trigger class that cannot be assigned handle-vs-abort with a stated reason. Any of these escalates to a defect investigation; none is presumed.
- Why this first: it is the smallest task that reduces gap #1, the top correctness risk — invalid/truncated output. It precedes mono-44.1k (gap #3: smaller in absolute terms but reduces no correctness risk and carries no failure signal), the deep branch audit (gap #2: larger; the trigger enumeration inside this task is its prerequisite map — the branch audit then goes deep per trigger class on real content), and the production API (gap #4: usability, and deciding termination semantics first prevents baking abort behavior into the API). CI (gap #5) follows once this task starts producing pinned behavior worth gating.

## 7. Deferred tasks and rationale

- **Mono-44.1k restore** (commit `cfg_44100_112000.txt` + fixture key via the oracle path, promote into `CASES`): do immediately after the flush task. Deferred only because it is completeness, not correctness — the blocking mechanism is fully understood and no encoder failure is suspected.
- **Targeted PNS/IS/retry/crash/fill branch audit on real content**: follows the flush task and consumes its trigger inventory; needs the M55 licensed streams plus a per-branch forcing/observation strategy. Not folded into the flush task to keep the latter bounded (mapping triggers ≠ validating each branch's numerics).
- **Production `Encoder` API slice** (M5.0 M5.e): follows termination-semantics + branch-policy decisions so the API exposes decided behavior, not placeholders. Test driver becomes first client; numerics must not move (M5.b floor).
- **CI gate + reproducibility pins** (M5.0 M5.f): follow the flush task so the gate protects newly pinned behavior; minimal workflow (M5.a/M5.b harness + fastest parity subset) with recorded toolchain versions and input hashes.
- **Config-matrix declaration + negative tests** (M5.0 M5.d) and **release-criteria definition**: process tasks that ride along with the API milestone, not before it.
- **Perceptual/quality-metric work**: remains `QUALITY UNCERTAINTY` by design until correctness gaps 1–2 close; SNR-gated tuning is explicitly not recommended on current evidence.

## 8. Confirmation of non-interference

- No experiments, benchmarks, encodes, decodes, or test runs were performed in this audit; all findings rest on the reports, sources, and test definitions listed in §2.
- No source, test, fixture, documentation, generated-file, or configuration change of any kind was made — the sole filesystem effect of this task will be this new report file, which did not previously exist (verified absent before writing).
- No staging, commits, stashes, resets, cleans, restores, amends, or pushes. Final worktree state matches §1 exactly (30 lines, +1571/−280 unstaged, index empty).
- D-021 not reopened: no contradictory evidence encountered; M5.b residues corroborate the closure.
