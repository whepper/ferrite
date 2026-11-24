# Ferrite FPE — M2 Explicit Orchestration Mirror — Implementation Report

STATUS: DONE. M2 implemented and validated. FPE is a separately implemented,
explicit orchestration mirror with FPE-owned state, still calling the Q31 value
kernels, and **bit-identical to Q31** on complete output + all frame-surviving
state, per frame, over multi-frame sequences. No floating-point psychoacoustic
kernel introduced. Stopped at M2; M3 not started.

Host/toolchain: `aarch64-apple-darwin` (Apple Silicon, the `production_baseline`
reference platform), rustc/cargo 1.99.1. HEAD
`b188415e3c16dbf00706984858c6a568c9b705ff` (unchanged; no git history operations).

## 1. Files changed + summary

M2 edits (this milestone):
- `rust/src/aac_enc/psy.rs` — **5 additive `pub(crate)` visibility bumps only**
  (`fn`→`pub(crate) fn`): `lowpass_and_zero_check`, `tns_encode_channel`,
  `overflow_rescale`, `advance_thresholds`, `finish_channel`. No
  behaviour/signature/layout change; `process_pair_reuse` and helper bodies
  byte-identical to HEAD.
- `rust/src/aac_enc/fpe/mod.rs` — `FpeChannel` from a wrapped `PsyChannel` to
  **individual owned fields** (`preecho`, `tns_state`, `tns_info`, `ms_ld`);
  `FpeScratch` from a wrapped `PsyScratch` to **owned buffers** mirroring the
  `PsyScratch` layout. Init mirrors `PsyChannel::new`. Test-only state accessors.
- `rust/src/aac_enc/fpe/orchestrate.rs` — replaced whole-function delegation
  with the **explicit per-pair mirror** of `psy::process_pair_reuse` (686 lines),
  incl. stage-correspondence table.
- `rust/src/aac_enc/fpe/output.rs` — `assemble_channel` = pass-through to
  `psy::finish_channel` + M2 invariant guards (205 lines).
- `rust/src/aac_enc/fpe/tests/mod.rs` — state comparison uses `FpeChannel`'s
  individual accessors; added per-frame representation-lane capture (`repr_seen`).
- `rust/src/aac_enc/fpe/tests/m2_stages.rs` — NEW, 8 M2 stage/invariant tests.

Pre-existing M1 uncommitted state (left intact): `aac_enc.rs`, `config.rs`,
`stream_driver.rs`, `testdata.rs` seams; `fpe/{constants,energy,thresholds,
preecho}.rs` skeletons; `fpe/tests/differential.rs`; `docs/fpe/`;
`tools/enc-oracle/make_fpe_fixtures.py`.

Not touched: Q31 kernels (`energy.rs`, `thresholds.rs`, `q31.rs`, `tns.rs`,
`sfb.rs`), all downstream (quant/rate/scf/pe/bitenc/frame/block_switch/transform),
Cargo manifests/lockfile, NOTICE/LICENSE/SPDX.

## 2. FPE orchestration sequence ↔ Q31 stages

`fpe::orchestrate::process_pair` mirrors `psy.rs:509-1130` stage-for-stage (full
table in `docs/stage-correspondence.md` and the module doc): D-022 entry asserts →
per-channel long/short config selection → lowpass/zeroSpec + initial exponents
(12/9) → Q31 map → joint headroom/minSpecShift → raw energies + joint maxNrg →
`zero_elem` shortcut / `rescale_shifts` / per-band re-shift + `init_threshold` /
representation shift → TNS detect/sync/active-OR/`tns_encode_channel`/element-global
`overflow_rescale` → `advance_thresholds` (clip/spread/PCM/pre-echo/spread-energy) →
MS energies + stale-on-short Ld history → grouping + output assembly. FPE owns the
sequence, conditionals, and element-global decisions (`min_spec_shift`,
`final_shift`, `tns_spec_shift`, `zero_elem`, TNS active-OR); every value
calculation calls a Q31 kernel.

## 3. FPE state representation + ownership

`FpeChannel` independently owns `preecho: PreEchoState` (Q31 stored-int history in
M2), `tns_state`/`tns_info: TnsChannel`, `ms_ld: [[f64;64];8]` (stale-on-short
preserved). `FpeScratch` owns its own int buffers (same reset-to-exact-pre-first-read
discipline as `PsyScratch`). Kernels are invoked with FPE-owned state/scratch — no
sharing of mutable Q31 state; `PsyChannel`/`PsyScratch` types and instances
untouched. Init/reset semantics identical (verified by
`fresh_fpe_state_init_matches_q31` + per-frame state gate).

## 4. D-022 + representation-lane invariants

- D-022: release-active `assert!` at orchestration entry, byte-identical messages
  to Q31 (LFE-must-be-Long, CPE-seq-agreement). Exercised by 3 `should_panic` + 3
  control tests (pass in debug AND release).
- Representation-lane guards (`output.rs`, `debug_assert!`, no release behaviour
  change): `finalShift >= 0`; grouped stride layout (lengths == `groups·sfb_cnt`,
  zero stride padding); threshold-Ld floor −33/64; finite Ld; energies/thresholds
  ≥ 0. All hold across the full corpus in debug. `mdctScale`/`finalShift`/`maxScale`/
  `tnsSpecShift` remain observable and exact (per-frame harness comparison +
  dedicated representation-lane test). Float-value/boundary guards documented as
  M3+ (inactive in M2).

## 5. Value calculations still Q31

Confirmed: no Q31→float conversion, no normalization/rescaling, no
rounding/quantization boundary, no independent float state, no kernel replacement,
no table regeneration. The value lane calls `energy::*`, `thresholds::init_threshold`,
`q31::shift_int`, `tns::*`, and the 5 seam-exposed `psy` stage helpers; results pass
through unchanged (P2 / R-19).

## 6. Differential results (per-frame output + state)

24/24 FPE tests pass in debug and release. The harness feeds identical spectra to
both engines over multi-frame sequences and asserts bit-identity every frame of:
every `ChannelOutput`/`GroupedOutput`/`MsOutput` field, mutated spectra,
`PreEchoState` (prev/prev_scale/enabled), `tns_state`+`tns_info`, `ms_ld` (incl.
stale-on-short), and all repr shifts — exact integer + `to_bits()` f64. Coverage:
long/short, START/STOP, pre-echo enable/disable/reset, stale-on-short MS-Ld,
mono/stereo, 44.1/48 kHz, silence/near-silence/transients/sustained/noise, 4 real
music/speech fixtures (~471 frames each), committed MDCT vectors, all M1 edge
fixtures. Exact frame accounting enforced. Max delta = 0 on every field.

## 7. Full validation (passed / failed / ignored / blocked separated)

- FPE tests: 24 passed / 0 failed (debug + release).
- Tier-A lib (CI skip list): 388 passed / 0 failed / 6 ignored (release);
  392 / 0 / 2 ignored (debug); 15 filtered out.
- Production baseline: 2 passed / 0 failed (exact hashes + byte totals pinned).
- Production API integration: 6 passed / 0 failed.
- Doctests: 19 passed / 0 failed.
- FPE seam (end-to-end byte-identity): 3 passed / 0 failed (debug + release).
- Formatting (`cargo fmt --check`): clean.
- Clippy (`--all-targets`): 0 errors; 2 informational warnings in
  `orchestrate.rs:420/655` identical to pre-existing Q31 baseline patterns
  (`psy.rs:865` needless_range_loop, `psy.rs:1099` needless_option_as_deref);
  mirror kept token-faithful.
- No-default-features build: OK (default build OK too).
- Licensing (`check-licensing.sh`): OK (80 files carry notice; converter boundary intact).
- `git diff --check`: clean.
- `cargo package --list`: includes LICENSE-FDK-AAC, NOTICE, all FPE files.

Ignored (not Tier-B): 2 perf benches (`neon_kernel_microbench`, `aac_perf_bench`)
+ 4 bitbuffer/bitstream manual checks (release-ignored; run+pass in debug).

BLOCKED (Tier-B, not counted as passing): 11 tests fail loudly on missing C
artifacts — `m5a_decode_check` (`AAC_DEC_BIN`), `m55_streams_match_c_reference`
(`M55_C_DIR`), and 9 `phase1*` (`PHASE1_ORACLE_DIR`/`PHASE1_C_DIR`). Identical to
the M0/M1 blocked set; the CI skip list excludes 15 total (these 11 + 4
decoder/opt-in-gated). A-3 remains unresolved (pinned `f14ad40a` not
re-provisioned); no claim otherwise.

## 8. Q31 regression evidence + baseline hashes

Q31 remains the default (`default_selection_is_q31_and_allocates_q31_state_only`;
allocate-selected-only). `production_f32_corpus_baseline_is_unchanged` passed → all
19 fixtures byte-identical (exact FNV-1a + byte lengths enforced on this reference
platform), e.g. onset-stereo `0x2a6ca107a2f0ba5d`/2359 B, music-stereo
`0x151e37b967d52678`/250926 B, silence-stereo `0x7713057f645eae80`/735 B (full set
in `production_baseline.rs`). VBR-6 default music pin `0xd6fc9d2c59cf5c36`/256763 B
passed. FPE-selected streams byte-identical to Q31 end-to-end
(`fpe_selected_streams_are_byte_identical_to_q31`).

## 9. Dependency / manifest / licensing status

No dependency, Cargo manifest, or lockfile change. No SPDX/NOTICE/LICENSE-file
change; FPE retains FDK-AAC TPMV, new files carry the concise notice.

FINDING FOR OWNER (M1-inherited, not altered by M2): M1's uncommitted diff bumped
the modification-notice date in 5 existing headers `2026-10-26 → 2026-11-16`, which
conflicts with AGENTS.md / `docs/fpe/12 §4.2` ("preserve baseline date 2026-10-26;
do not rewrite existing headers"). M2 preserved the M1 working-tree state and did
not change attribution; recommend the owner restore `2026-10-26` (or explicitly
approve the bump) before commit. See `docs/guidance/AGENTS.md` and
`docs/guidance/12-licensing-provenance.md`.

## 10. Git status / HEAD / untracked

HEAD `b188415e…` (unchanged). Modified: `aac_enc.rs`, `config.rs`, `psy.rs`,
`stream_driver.rs`, `testdata.rs`. Untracked: `docs/fpe/`, `rust/src/aac_enc/fpe/`,
`tools/enc-oracle/make_fpe_fixtures.py`, plus macOS `.DS_Store` artifacts. No
commit/push/amend/rebase/reset/cherry-pick/stash performed.

## 11. Remaining limitations + next milestone

- Tier-B C↔FPE ground truth blocked on reference re-provisioning (R-12 / A-3);
  Q31↔FPE in-binary harness is the active gate.
- M2 value lane is Q31 pass-through; fraction value lane + boundary quantization
  activate per-kernel from M3.
- The 2 Clippy informational notes mirror pre-existing Q31 patterns (left for
  mirror faithfulness).
- Recommended next: M3 (float energy kernels, FPE-3.1/3.2/3.3) — replace rescale +
  TNS energy value arithmetic with f64 fraction-domain sums, keeping the
  representation lane integer-exact, gated by the same differential harness
  (energy within `q31_tol`, shifts/maxScale exact). Not started.
