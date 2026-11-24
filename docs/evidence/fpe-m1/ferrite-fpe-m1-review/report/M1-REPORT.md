# TASK FPE-M1 — IMPLEMENTATION REPORT

**STATUS: DONE** (M1 complete; stopped before M2/M3 as instructed). All work at
HEAD `b188415`, branch `main`; no commits, amendments, rebases, resets,
cherry-picks, or pushes.

The complete `docs/fpe/` package (00–12, all five ADRs, `m0-verification.md`)
was read before implementing. No genuine contradictions required a stop; four
apparent tensions were resolved in favour of the rev-4 planning + the task
spec (documented in §11 below).

## 1. Exact files changed

**New — FPE module tree** (`rust/src/aac_enc/fpe/`, 1244 lines):

| File | Lines | Content |
|---|---|---|
| `mod.rs` | 135 | `FpeChannel`, `FpeScratch`, `FpeTables` (= `PsyTables`), `process_pair_reuse` entry |
| `constants.rs` | 206 | Float tuning constants (05 §3) + 4 exactness unit tests (FPE-1.1) |
| `orchestrate.rs` | 62 | Per-pair delegation to Q31 (M1); M2 mirror documented |
| `energy.rs` / `thresholds.rs` / `preecho.rs` / `output.rs` | 25/29/28/36 | Doc-only skeletons (M3–M7 scope, upstream/Q31 cross-refs); **no code** |
| `tests/mod.rs` | 495 | Differential harness: bit-identical comparators + dual-engine runner |
| `tests/differential.rs` | 228 | 12 multi-frame/vector scenario tests |

**New — fixtures:** `tools/enc-oracle/make_fpe_fixtures.py` (105 lines,
additive; `make_fixtures.py` untouched).

**Modified (additive seams only, +425/−26):**
- `rust/src/aac_enc.rs` (+3): `pub(crate) mod fpe;`
- `rust/src/aac_enc/config.rs` (+88): `PsyEngineKind`, private field,
  `with_psy_engine`/`psy_engine` (`#[doc(hidden)]`), 2 tests
- `rust/src/aac_enc/stream_driver.rs` (+262/−26): fields, `with_config`
  selection, dispatch match, `psy_tns_info` helper, `#[cfg(test)] psy_engine()`,
  3 seam tests (−26 = dispatch lines restructured into match arms; Q31 arm
  byte-identical)
- `rust/src/aac_enc/psy.rs` (+14): two `#[cfg(test)]` accessors (`tns_state()`,
  `ms_ld()`) — explicitly sanctioned by 04 §4 / 07 §3.1 / 09 FPE-2.5
- `rust/src/aac_enc/testdata.rs` (+84): 6 fixture resynthesis arms

**Unchanged:** all Q31 kernel modules (`energy.rs`, `thresholds.rs`, `q31.rs`,
`tns.rs`, `sfb.rs`, `sfb_tables.rs`, `psy_config.rs`), all downstream (`pe`,
`threshold_vbr`, `scf`, `quant`, `rate`, `bitenc`, `frame`, `adts`),
`encoder.rs` public API, all manifests/lockfile, all licensing files.

## 2. Module and dispatch architecture

Per ADR-0001: self-contained `fpe/` tree beside the flat Q31 modules (no Q31
reorganization). Internal `PsyEngineKind { Q31, Fpe }` (Copy, `Default = Q31`),
stored on `StreamEncoder`, chosen at `with_config` from the additive config
knob. **Single dispatch point** in `StreamEncoder::frontend` — a
`match self.psy_engine` (stereo + mono arms), both returning the identical
`(ChannelOutput, Option<ChannelOutput>, Option<MsOutput>)` tuple. No trait, no
dyn/generic dispatch, no Cargo feature, no plugin registry.
Allocate-selected-only state (06 §3): `psy_ch` empty when FPE selected,
`fpe_ch` empty when Q31 selected; scratch default-empty (zero allocation when
unused).

## 3. Q31/FPE pass-through implementation

`fpe::process_pair_reuse` → `orchestrate::process_pair` →
`psy::process_pair_reuse` operating on **FPE-owned** Q31-typed state
(`FpeChannel` wraps `PsyChannel`; `FpeScratch` wraps `PsyScratch`). Same
kernels, same order, same state semantics — only the state/scratch instance
differs. **No float kernel, no Q31→float→Q31 round-trip, no rounding/rescaling,
no independent state calculation.** The 05 §7 boundary is inactive (output.rs
is doc-only). D-022 preconditions, element-global shifts, TNS sync,
stale-on-short MS-Ld inherited exactly. `FpeChannel::new` mirrors
`PsyChannel::new` (init gate tested).

## 4. Differential harness design and state coverage

Tier-A, in-binary, C-independent (`fpe/tests/`). `DiffStream` drives the shared
front-end (BlockSwitch + MDCT, identical by construction), feeds **identical
cloned spectra** to both engines with independently initialized equivalent
state, reusing scratch across frames. After **every frame** it compares,
**bit-identically** (`==` ints, `to_bits()` f64):
- **Every `ChannelOutput` field** (15): mdct_scale, final_shift, tns_shift,
  nrg_scaling, maxscale, energies, energy_ld, thresholds, spread, nm1, m_nm1,
  calc_pre_echo, grouped, threshold_ld, max_sfb_per_group.
- **Every `GroupedOutput` field** (11) incl. ms_energies/ms_ld; **every
  `MsOutput` field** (4) incl. stale pre-grouping Ld.
- **Mutated spectrum** (both channels).
- **All persistent state**: `PreEchoState` (prev/prev_scale/enabled via
  existing pub accessors), TNS `tns_state` + `tns_info` (all 9 pub fields),
  `ms_ld [[f64;64];8]` incl. stale-on-short — via `#[cfg(test)]` helpers only,
  **no production API added for testing**.

**Execution integrity (07 §7.1):** exact frame accounting per run; coverage
assertions prove the intended paths executed. **12 scenarios** cover long/short
transitions, START/STOP, pre-echo enable/reset (disabled/memorize frames
asserted), stale-on-short MS-Ld (asserted on onset/stepdown/mixed/44.1k),
mono/stereo, 48k/44.1k, silence/near-silence, transients, sustained/noise,
stress, all 6 new edge fixtures, all 4 real music/speech WAVs (~471 frames
each), plus committed-MDCT-vector single frames (sweep f10 long, onset f24
short, click f11 mono). The full 07 §6 corpus matrix is executed.

## 5. Test results

| Suite | Result |
|---|---|
| **New M1 tests — total** (4 constants + 12 differential + 3 seam + 2 config) | **21 passed, 0 failed** |
| — of which selected by `cargo test --lib fpe` | **20 passed** (the config test `psy_engine_defaults_to_q31_in_every_constructor` lacks `fpe` in its name and runs in the broader Tier-A suite; see §5a) |
| Tier-A lib (debug, CI skip list) | **384 passed, 0 failed, 2 ignored**, 15 filtered (was 363 pre-change; +21) |
| Production baseline (release) | **2 passed, 0 failed** (VBR-5 corpus + VBR-6 default hashes) |
| Integration `production_api` | **6 passed, 0 failed** |
| Doctests | **19 passed, 0 failed** |
| Full lib (no skips) | 388 passed, **11 failed**, 2 ignored |
| `cargo build --locked` / `--no-default-features` | both OK |
| `cargo fmt --all -- --check` | **clean** (after formatting my new code) |
| `cargo clippy --locked --all-targets` | exit 0; **zero warnings in new/edited regions** (183 pre-existing warnings elsewhere, unchanged) |
| `check-licensing.sh` | **OK** (79 src files carry TPMV notice; converter boundary intact) |
| `git diff --check` | **clean** |
| `cargo package --locked -p ferrite-aac --list` | exit 0; `NOTICE` + `LICENSE-FDK-AAC` shipped; fpe files included |

### 5a. Test-count clarification (review Finding 2)

The substring filter `cargo test --locked --lib fpe` selects a test iff its
full path contains `fpe`. It selects **20** tests:
4 `fpe::constants::tests::*` + 12 `fpe::tests::differential::*` +
3 `stream_driver::fpe_seam_tests::*` +
1 `config::tests::with_psy_engine_builder_selects_fpe_and_preserves_other_fields`.

The 21st new test,
`config::tests::psy_engine_defaults_to_q31_in_every_constructor`, does **not**
contain `fpe` in its name, so it is **not** selected by that filter; it runs in
the broader Tier-A library suite (confirmed via `cargo test --locked --lib --
--list`, and individually: 1 passed). Hence **20 (FPE filter) + 1
(constructor-default, Tier-A) = 21 total new M1 tests**. This is a reporting
clarification only — no test behaviour was changed. (An earlier draft of
`validation/COMMANDS.md` mis-stated the filter as matching "2 config" tests;
the correct figure is 1 config test in the filter, 2 config tests total.)

**Ignored (2):** `neon_kernel_microbench`, `aac_perf_bench` — opt-in
benchmarks, unchanged.

**Externally blocked (11 Tier-B — reported separately, NOT passing):** the 11
full-suite failures are exactly the M0-recorded artifact-gated set, all failing
loudly on missing C artifacts — 10 × "reference validation artifact missing"
(`$PHASE1_ORACLE_DIR`/`$PHASE1_C_DIR`/`$M55_C_DIR`) + 1 × `m5a_decode_check`
(`$AAC_DEC_BIN`). These are CI-excluded Tier-B tests blocked on reference
re-provisioning (R-12), **not Q31 regressions**. No unavailable artifact is
counted as a pass.

## 6. Q31 regression evidence

- Production baseline hashes **unchanged** (release, aarch64-apple-darwin
  reference platform): both pinned tests pass.
- `aac_bench_wav` (release, 15 reps + 1 warmup, M0 method): **all FNV-1a hashes
  match the committed VBR-6 pins** — music `d6fc9d2c59cf5c36`, speech-clean
  `7c3e409574ebc62e`, speech-pauses `6c087bfb228d3bbe`, eroica
  `dcf07392921a18b7`, music-x4 `7f3a621a091eee2f`; byte totals identical
  (256763/129335/127326/241021/1021456). The example asserts each hash;
  `WAVBENCH_DONE`, exit 0.
- **FPE-selected streams byte-identical to Q31** end-to-end
  (`fpe_selected_streams_are_byte_identical_to_q31`: 7 fixtures × both
  profiles/rates, per-frame ADTS equality through the full downstream path).
- Diff review: Q31 changes are additive-only; the Q31 dispatch arm and
  `psy_tns_info` Q31 read are byte-identical to the former code. Default
  remains Q31 (G9).

## 7. Dependency and manifest changes

**None.** `rust/Cargo.toml`, `rust/Cargo.lock`, `rust/convert/Cargo.toml`
untouched. No new dependencies, no features.

## 8. Git working-tree and untracked-file status

HEAD `b188415` (unchanged). Modified tracked: the 5 files in §1. Untracked
**new**: `rust/src/aac_enc/fpe/` (9 files),
`tools/enc-oracle/make_fpe_fixtures.py`. Untracked **pre-existing** (not
touched): `docs/fpe/`, `.DS_Store`, `docs/.DS_Store`. Generated fixture WAVs
and all logs live only in the approved temp dir, not the repo. No unrelated
working-tree file changed.

## 9. Floating-point kernel confirmation

**Confirmed: no floating-point psychoacoustic kernel was implemented.**
`constants.rs` defines fraction-domain tuning constants (scaffolding, consumed
from M3+) but applies them to no signal; `orchestrate.rs` delegates all value
processing to Q31; `energy/thresholds/preecho/output.rs` contain no code. No
Q31→normalized-float→Q31 conversion, no rounding, no rescaling, no independent
state calculation. The pass-through is bit-identical (proven per-frame on
output + spectrum + state, and end-to-end on ADTS bytes).

## 10. Remaining blockers and recommended next milestone

- **A-3 NOT resolved** (as instructed): pinned reference `f14ad40a…`
  unavailable locally; parent-tree equivalence verified at M0. **FPE-0.1**
  (direct pinned-reference diff) and **FPE-0.4** (`enc_oracle` build/fixtures)
  remain **blocked** on private-reference re-provisioning (R-12). A-3 must be
  resolved before M8.
- **Tier-B C↔FPE / C↔Q31 validation blocked** on the same reference (11 tests
  above). Oracle ring/psy/seq **vector anchoring for the 6 new fixtures** is
  likewise blocked; their PCM generation is validated (3-run byte-stability +
  sample-exact Python↔Rust cross-check), and the in-binary harness consumes
  them without C.
- **Recommended next milestone: M2** — FPE skeleton orchestration mirror
  (FPE-2.1/2.2: expose individual Q31 state fields, mirror `psy.rs:495-1116`
  order with per-stage Q31 kernel calls), pass-through assembly (FPE-2.3),
  D-022/invariant guards (FPE-2.4). The M1 harness already enforces the M2
  bit-identical output+state gate (G0). **Not started** — stopped per
  instruction.

## 11. Planning-compliance notes (apparent tensions resolved, no unilateral decisions)

1. **Knob timing:** 06 §2 says "added internally at M2", but 09 M1 FPE-1.2 and
   the task §3 place the seam at M1. Implemented at M1 with `#[doc(hidden)]`
   unstable exposure (06 §2's explicit allowance); stable public exposure
   deferred to M8.
2. **LiveChannel TNS source:** ADR-0003 says LiveChannel construction is
   "untouched", but 06 §3's separate `fpe_ch` state requires the TNS-info read
   (former `stream_driver.rs:964`) to follow the selected engine. Implemented
   as an additive engine-aware helper (`psy_tns_info`); the Q31 arm is
   identical to the former read. This is a mechanical consequence of the
   approved separate-state architecture, not a new decision — flagged for
   reviewer awareness.
3. **Skeleton depth:** 09 M1 allows "FPE skeleton may stub"; the task §4–5
   requires a delegating, bit-identical skeleton + full state harness.
   Implemented the stronger requirement (consistent with M1 acceptance); M2's
   distinctive orchestration-mirror work remains for M2.
4. **Fixture vectors:** FPE-1.4 "generate vectors via 3-run stability" — the
   C-dependent oracle-vector extraction is blocked (R-12); the C-independent
   fixture PCM generation + 3-run byte-stability + Python↔Rust cross-check
   were completed and are reported as such.

**Licensing:** FPE retains FDK-AAC; new files carry the concise TPMV notice
with FPE implementation date 2026-11-16; no SPDX/NOTICE/license-file change;
`check-licensing.sh` + `git diff --check` pass. **Q31 intact, default Q31, no
float kernels, no manifest change, no commits.**