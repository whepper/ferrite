# M1 context — pre-existing uncommitted state (not re-authored by M2)

The working tree at HEAD `b188415e…` already contained the **M1** implementation
(roadmap `09` M1, FPE-1.1…1.4) as uncommitted changes. M2 builds on it. This note
lets the reviewer separate M1 from M2 in `git/diff-complete.patch`.

## What M1 added (binding baseline for M2)

- **Module tree + constants (FPE-1.1):** `fpe/{mod,constants,energy,thresholds,
  preecho,orchestrate,output}.rs`. `constants.rs` derives the float tuning
  constants exactly `q31/2^31` with unit tests. `energy/thresholds/preecho/output`
  were doc-only skeletons (no float kernel).
- **Selection seam (FPE-1.2):** `config.rs` `PsyEngineKind {Q31,Fpe}` (default
  Q31), `with_psy_engine`/`psy_engine` builders (`#[doc(hidden)]`, unstable until
  M8); `stream_driver.rs` additive `psy_engine`/`fpe_ch`/`fpe_ws` fields,
  allocate-selected-only at `with_config`, single dispatch match in `frontend`,
  engine-aware `psy_tns_info` for `LiveChannel`; `aac_enc.rs` `pub(crate) mod fpe`.
- **Q31↔FPE in-binary harness (FPE-1.3):** `fpe/tests/{mod,differential}.rs` —
  multi-frame dual-engine runner with per-frame complete-output + mutated-spectrum
  + persistent-state bit-identity, scenario coverage, frame accounting.
- **Edge-case fixtures (FPE-1.4):** `testdata.rs` 6 `fpe-*` fixtures (near-silence
  ramp, full-scale square, pow2-boundary, max-headroom, all-zero, mixed-pair);
  `tools/enc-oracle/make_fpe_fixtures.py` generator.
- **Planning package:** `docs/fpe/` (charter/audit/architecture/spec/integration/
  validation/performance/roadmap/risk/guidelines/licensing + 5 ADRs + M0 record).

## M1's pass-through mechanism (what M2 replaced)

M1 `FpeChannel` **wrapped a `PsyChannel`** and `FpeScratch` **wrapped a
`PsyScratch`**; `fpe/orchestrate.rs::process_pair` **delegated the whole per-pair
operation** to `psy::process_pair_reuse`. Bit-identical, but no independent FPE
orchestration/state. M2 replaces this with the explicit mirror + individually owned
state/scratch (see `report/m2-implementation-report.md` and
`docs/stage-correspondence.md`).

## M1 edits to existing files (visible in the diff, NOT M2)

- `aac_enc.rs`, `config.rs`, `stream_driver.rs`, `testdata.rs`: the additive seam /
  fixtures / tests above.
- `psy.rs`: two `#[cfg(test)]` accessors (`tns_state`, `ms_ld`) for the harness.
- **Modification-notice date bump** `2026-10-26 → 2026-11-16` in the headers of
  `aac_enc.rs`, `config.rs`, `psy.rs`, `stream_driver.rs`, `testdata.rs`. This is an
  M1 change. It conflicts with AGENTS.md / `docs/fpe/12 §4.2` (preserve the baseline
  date `2026-10-26`; do not rewrite existing headers). M2 did **not** alter it; it is
  flagged for owner resolution before commit. The new `fpe/` files carry their own
  FPE implementation date `2026-11-16` per `12 §4.1/§4.2`.

## M2-only delta (this review's focus)

- `psy.rs`: 5 additive `pub(crate)` visibility bumps (no body change).
- `fpe/mod.rs`: individual `FpeChannel` fields + owned `FpeScratch` buffers.
- `fpe/orchestrate.rs`: explicit stage-by-stage mirror (replaces delegation).
- `fpe/output.rs`: `assemble_channel` pass-through + invariant guards.
- `fpe/tests/mod.rs`: state comparison via individual accessors + `repr_seen`.
- `fpe/tests/m2_stages.rs`: NEW, 8 M2 stage/invariant tests.
