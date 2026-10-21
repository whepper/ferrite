# M5.i — Pass-Margin Census

Diagnostic and validation. No tuning, no behavior change, no forcing.

**Status: `M5.i COMPLETE — PASS MARGINS MEASURED AND NON-INTERFERENCE VERIFIED`**

## 1. Baseline (Phase 0)

- Branch `main`, HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (verified before and after; unchanged).
- Initial `git status --short`: 38 lines (11 modified implementation files + M5.0–M5.h reports + test-gated harness files + `cfg_44100_112000.txt` + testdata WAVs/provenance + `tools/c-enc-bench/`, `tools/enc-oracle/__pycache__/`). Index empty; nothing staged, committed, or otherwise manipulated.
- Deliverable verified absent before writing.

## 2. Source-derived definitions, formulas, units, sign conventions (Phase 1)

All values are bits (signed `i32` in code; analysis uses `i64` differences), per emitted frame, final-pass state unless noted. Positive margin = headroom before the named boundary; zero or negative would mean reached/crossed. No "near/safe" labels are applied — the source defines no near-threshold, so only raw minima are reported (§5).

| Quantity | Source | Observation point | Meaning |
|---|---|---|---|
| `iterations` | `PassTrace.iterations` (`rate.rs:1338`), last pass | S6 exit, per frame | Inner gain-adjust count (`reduce_bit_consumption` calls). **Distinct from `npass`** (outer pass count): iterations counts re-quantizations *within* the MAX_QUANT loop; npass counts outer exit-matrix loops. Crash path requires `iterations == MAX_ITERATIONS` (=5, `rate.rs:242`); QuantError at `> max`, or `== max` with `el_dyn == 0` (`rate.rs:1010-1027`). |
| `el_dyn`, `el_granted` | `qc_out.used_dyn_bits`, `qc_out.el_granted` (`rate.rs:1356,1377`) | S6 final | `M_grant = el_granted − el_dyn`: headroom under the element dyn grant. Negative would force retry/loop continuation. |
| `max_dyn` | `qc_out.max_dyn_bits` (`rate.rs:1354`) | S6 final | `M_maxdyn = max_dyn − used_dyn`: overshoot retry triggers when `used_dyn > max_dyn` (`rate.rs:1674` context, `exit_matrix` overshoot arm `:1163,1166`). |
| `sum_bits`, `total_avail` | `sum_bits = total_consumed_bits(used_dyn, static_used, 0, GLOBAL_EXT_BITS=3, ADTS_HEADER_BITS=56)` recomputed in the driver from final QC state (`rate.rs:1120-1128`, formula `(8−data%8)%8 + data + glob_hdr`); `total_avail` from `qc_out` (`rate.rs:1355`) | S6 exit-matrix formula, final values | `M_frame = total_avail − sum_bits`: over-budget (`>0` spent) forces retry (`:1156,1173`); under-budget with `decrease==1` exits. `sum_bits == total_avail` exactly is legitimate acceptance, not a violation. |
| `static_used`, `nch` | `qc_out.static_bits_used` (`rate.rs:1376`); channels per stream | S6 final / stream config | `M_cap = nch*6144 − static_used − used_dyn`: element-cap retry triggers when `el_dyn > nch*MIN_BUFSIZE_PER_EFF_CHAN − el_static` (`MIN_BUFSIZE_PER_EFF_CHAN=6144`, `rate.rs:223`). |
| `decrease`, per-pass `iterations` history | locals / `PassTrace` vector | — | **Not recorded**: `decrease` is a function-local direction flag, not emitted in any output struct; per-pass history beyond the last pass is not surfaced. Documented limitation, not a blocker (all observed frames are single-pass). |

`sum_bits` is recomputed rather than plumbed because `qc_main_frame` does not return the per-pass sum (only the decision derived from it); the recomputation uses the identical public function and constants over final values, verified equivalent by the accounting identity holding on all frames (see §4: `adts_len*8 == 56+total` plus `total == static+dyn+fill+align+3` from the S6 unit tests). `check_min_frame_demand()` is a stub returning 1 (`rate.rs:1132`): the two exit-matrix arms depending on its false branch are dead by construction (M5.h §3) — no margin applies.

## 3. Instrumentation (Phase 2)

Additive, test-gated only (`stream_driver` is `#[cfg(test)]`; `rate.rs`/`frame.rs` untouched):
- `FrameSnapshot`: 6 pure-read fields (`last_iterations`, `sum_bits`, `el_granted`, `max_dyn`, `total_avail`, `static_used`) assigned from `qc_out`/last-pass state next to the existing M5.g snapshot lines; 2 book-observer lines (M5.g) unchanged.
- `CensusRow`: same 6 fields + `used_dyn` (snapshot already had it); populated alongside existing fields.
- `m5g_census.rs`: JSON rows extended; per-stream margin-minima prints; global minima print; **no new assertions on margins by design** (all M5.g asserts preserved verbatim: npass==1, crash==-1, zero books, `adts_len*8 == 56+total`, loud skip/note policy).
- `M55_FIXTURES` was already hoisted in M5.g (no change here).

If any value had required changing encoding decisions, the milestone required stopping; none did — every operand is read after the encoding decision is final.

## 4. Inventory, artifact, schema

Exact 19-stream M5.d inventory (15 M52 + 4 M55; mono-44100 is **not** in the inventory and was not added — see §8). Artifact: `$TMPDIR/m5g-census/census.json` (overwrites the M5.g artifact at the same path by design of the extending test; M5.g reference values are preserved in the M5.g report tables and verified identical below). Schema per row: `{f, seq, npass, crash, tot_fill, total, adts_len, is_book, noise, iter, sum, used, grant, maxdyn, avail, static}`. Command: `cargo test --features aac_enc m5g_branch_census -- --nocapture` (from `rust/`), exit ok, 1 passed.

## 5. Measurements: extrema and boundary distances (Phase 3)

`iterations`: **max 0 of max 5 on all 2442 frames** — `reduce_bit_consumption` never ran; every frame satisfied MAX_QUANT and all caps on first quantization. (Distinct from `npass==1`: zero inner adjustments *and* one outer pass.)

Signed-margin minima (global; all on `m3-noise-mono-48k f=1`):

| Margin | Guards (source) | Min observed | Meaning |
|---|---|---|---|
| `M_grant = el_granted − el_dyn` | grant overuse → retry/loop | **986 bits** | smallest dyn-grant headroom |
| `M_maxdyn = max_dyn − used_dyn` | overshoot (`>max_dyn`) → retry | **1042 bits** | smallest overshoot headroom |
| `M_frame = total_avail − sum_bits` | `sum_bits > total_avail` → retry | **984 bits** | closest to over-budget retry |
| `M_cap = nch*6144 − static − dyn` | element-cap → retry | **1045 bits** | smallest cap headroom |

Per-stream minima (same order grant/maxdyn/frame/cap): silence 12168/12224/12168/12227; sine 10276/10332/10272/10335; onset 9593/9649/9592/9652; stepdown 9301/9357/9296/9360; click 1310/1366/1304/1369; impulse 4737/4793/4736/4796; edge 4717/4773/4712/4776; multitone 9264/9320/9264/9323; sweep 9812/9868/9808/9871; noise 986/1042/984/1045; tonal 8268/8324/8264/8327; quiet 10790/10846/10784/10849; sine441 10452/10508/10448/10511; onset441 9810/9866/9808/9869; stress 6426/6482/6424/6485; music 4795/4851/4792/4854; pauses 8892/8948/8888/8951; clean 7559/7615/7552/7618; eroica 7442/7498/7440/7501.

Boundary classification (source semantics only): gain-bump retry, crash entry, +1-fallback/QuantError, element-cap, emergency exit, overshoot retry, over-budget retry — **all `MEASURED — NO CROSSING OBSERVED`**. No boundary reached (no equality with a triggering predicate: `iterations` never ≥1 let alone 5; all margins strictly positive). `check_min_frame_demand` arms and outer-guard panic: `UNOBSERVABLE` (stub constant / bug tripwire, M5.h §3). Nothing was `REACHED`, `CROSSED`, or `PARTIAL`. `sum_bits == total_avail` exact equality never occurred either (closest: 984 above).

## 6. Non-interference and invariants (Phase 4)

- **Output hashes:** all 4 known ADTS hashes byte-identical to M5.a/M5.e records (`a9d7b395…`, `702c8717…`, `aa906b52…` reproduced from the census dir; mono-44100 `c3b8b5fd…` reproduced via the m5a re-run below). All 15 M52 byte counts and all 4 M55 byte counts match the M5.d runs exactly. All 4 M5.b input WAVs unchanged.
- **M5.g invariants:** `npass==1`, `crash==-1`, zero books, `adts_len*8 == 56+total` hold on all 2442 rows (asserted in-test, re-verified offline from the artifact with zero violations); tot_fill distribution identical to the M5.g run (287 nonzero rows, all exactly 7 bits; silence/stepdown zero).
- **Regression:** `m52_all_fixtures_write_complete`, `m5a_decode_check` (4 cases), `m5b_dump_inputs`, `m55_streams_match_c_reference` (pre-existing C dir, read-only) — all ok. Narrowest set covering the touched paths; no broad suite.
- **M5.b baselines:** untouched (no threshold, hash, alignment, or metric changes anywhere).

## 7. Interpretation

The corpus never comes close to any retry/crash boundary in the measured dimensions: zero gain adjustments (of 5 allowed) on every frame, and the tightest bit-budget margin is 984 bits of headroom on a single noise-mono frame, with all other streams ≥1304 bits. Distance-to-threshold is explicitly **not** correctness proof — it bounds how far live content stayed from the exceptional paths; the exceptional paths themselves remain validated only by the forced unit tests (M5.h §4). A small margin would not have been a defect either; as observed, no margin is small enough to implicate forcing (the minimum is three orders of magnitude above zero in units of the decision, with zero iterations to corroborate).

## 8. Limitations

- Mono-44100 (20th validated stream) is outside the 19-stream inventory and its margins are unmeasured; its path passed M5.a gates and its bytes are unchanged. Not added here to respect inventory stability (M5.i scope).
- `decrease`/per-pass history unrecorded (no output carries them); irrelevant in-corpus (single-pass everywhere) but noted for completeness.
- Margins are per-frame final-state snapshots under the live reservoir trajectory — the honest live condition, not a worst-case bound.
- The census cannot show what *would* happen past a boundary; forced tests remain the only execution evidence for the exceptional paths.

## 9. Recommended next step (exactly one)

**Defer artificial retry/crash branch forcing as unjustified on current evidence; retain the M5.i-extended census as the standing margin/behavior tripwire.** Evidence: 2442/2442 frames with `iterations==0` (of 5) and minimum headroom 984 bits across four independent boundary families — no measured boundary or small margin implicates forcing, and forcing would manufacture conditions with no live relevance (against M5.i scope). The census now fails loudly on any future `npass≠1`/`crash`/book/accounting deviation and records any future `iterations>0` in the artifact diff, so the tripwire is automatic. **Escalation (re-open forcing work) triggers:** any future run showing `iterations>0`, `crash≠-1`, a `QcError` skip, ADTS hash movement, or a margin collapse toward zero — observed events only, no invented thresholds. Acceptance for this milestone (met): margins recorded on all 19 streams, M5.g asserts pass, outputs byte-identical, no thresholds invented or accommodated.

## 10. Final state and non-interference confirmation

- HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91`, branch `main` — unchanged.
- Final `git status --short`: 39 lines = 38-line baseline + this report; all other lines verified identical; index empty (0 staged).
- No production behavior changed (all edits in `#[cfg(test)]` code or untracked test files; `rate.rs`/`frame.rs` untouched — only read via existing `pub` items), no baselines or references changed, no staging/commits. D-021 untouched.
