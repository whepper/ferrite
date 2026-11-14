# Native Rust AAC-LC VBR-5 Encoder — Performance Campaign Review (M5.12–M5.22)

Audit/documentation milestone **M5.24** (2026‑10‑16). This report consolidates what the
M5.12–M5.22 performance campaign actually achieved, with every figure traced to a preserved
artifact or recomputed from raw evidence by this audit.

**Code under review**

| Item | Value |
|---|---|
| Repository | `/Users/USER/Documents/VSCode/fdk-aac/fdk-aac` |
| `HEAD` | `e4258d2a5b55581f5f2b27aa6c1a369bb2503e43` ("complete native Rust encoder through frame assembly") |
| Nature of the work | **uncommitted** (10 modified tracked files, 9 untracked paths); `HEAD` does **not** contain the harness or the campaign |
| Build profile | default `release` (opt‑level 3, LTO off, codegen‑units 16); rustc/cargo 1.99.0; macOS 27.0.1, Apple M5 (arm64) |
| Machine | Apple M5, single‑threaded encode, 1 ms `sample` profiling |
| Preservation backup | `/Users/USER/backups/fdk-aac-m5.23-worktree-20261016T193606Z` (M5.23, verified) |

**Fresh evidence digest (computed by this audit, all sources read‑only).** The 36 normalized
`M58` result lines (18 fixtures × {EncoderOnly, EndToEnd}) hash to
`5f58213f566aefe8…` in **every** preserved corpus log from the pre‑M5.16 snapshot through the
post‑M5.22 worktree — i.e. the whole campaign preserved one byte‑identical corpus output. The
18 corpus fixture files still reproduce the recorded manifest
`201b292db60d6bebb59a18891ce8d9e4ffda6f6f57b2f303f747107d87eb2998`.

---

## 1. Executive summary

1. **No campaign‑wide speedup number is defensible.** M5.16–M5.22 measured *different*
   workloads, iteration counts, and boundaries (M5.16–M5.20: whole‑process invocation of the
   `m511_profile_loop` test binary; M5.22: in‑process `wall_ns` + a comparability arm). Summing
   or compounding those per‑milestone deltas would be invalid and is deliberately not done here.
2. **Only two encoder‑core changes show a delta clearly above the harness noise:** M5.16
   (quantizer state hoist/inline, music **−2.67 %** median, 6/6 paired wins) and M5.17 (scf
   scratch‑band reuse, music **−2.42 %**, multitone **−4.19 %**, both 6/6). M5.20 (psy scratch
   reuse) is at the edge: **−0.63 %** music / **−1.56 %** multitone medians, 8/10 paired wins,
   sign‑test p≈0.11, while the same milestone's allocation measurements are unambiguous.
3. **M5.18's −3.3…−3.7 % is real at the measured boundary but is not encoder‑core compute.**
   It removed a duplicated final‑gain re‑quantization that existed only in the Rust test driver
   (the C reference reuses the QC loop's final pass). It improves benchmark validity; it must not
   be reported as a faster encoder.
4. **Allocation work (M5.19–M5.22) did not produce a demonstrated runtime win.** M5.19 runtime
   flat (music −0.15 % pooled median, 5/10 wins); M5.22 *failed its pre‑registered timing gate*
   on both primary fixtures and is retained as allocation hygiene only. Allocation *counts* were
   measured precisely and reproducibly (M5.20: −13.2 % loop / −42.1 % psy; M5.22: −5.9 % loop with
   the in‑path delta exactly equal to the total delta).
5. **Correctness evidence is strong but narrower than it looks.** Byte‑identity is proven against
   the *immediately preceding Rust version* on 18 fixtures in both modes (chain verified across
   every milestone) plus internal ADTS re‑parse, determinism reruns, and targeted unit/probe tests
   (including a cross‑frame LFE probe with float bit‑pattern comparison). It says **nothing** about
   equivalence to the C encoder's audio quality — and the campaign ran **no** quality metric.
6. **The campaign has no measurement of C‑versus‑Rust quality or of a fair C‑versus‑Rust speed
   ratio.** The only C comparison (M5.21) is a coarse, non‑identical‑boundary end‑to‑end ratio
   (≈1.25× music, ≈1.16× speech) the milestone itself explicitly declined to call precise.
7. **Known correctness gaps remain open and untouched by this campaign**: the mono impulse
   output‑size discrepancy vs C (Rust 306 B vs C 308 B), Tier‑B float‑lane divergences, active
   TNS‑filter spectrum differences whose quality impact is explicitly "unverified", and two
   pre‑existing out‑of‑contract panics (short‑window LFE; mixed‑window CPE).
8. **Recommended next milestone: quality first** — a C‑versus‑Rust quality and divergence audit
   on the existing 18‑fixture corpus (direction **A**), before any further performance work.

---

## 2. Evidence base and how to read it

**Preserved and used by this audit** (all read‑only; nothing in the worktree or the M5.23 backup
was modified):

| Source | Content |
|---|---|
| `/Users/USER/backups/fdk-aac-m5.23-worktree-20261016T193606Z/evidence/` | full copy of the milestone evidence trees (`m516-ab`…`m522`, corpora, root reports, production binaries) |
| `…/T/opencode/m516_ab.txt`, `m517_ab.txt`, `m518_ab.txt`, `m519_ab.txt`+`m519_ab_ext.txt`, `m520_ab.txt`+`m520_ab_ext.txt` | raw interleaved A/B timing lines (epoch‑timed whole‑invocation runs) |
| `…/m521/{review_packet.md,parsed_summary_v5.txt,reviewer_corrections.md,timing_run{1,2,3}.txt,alloc_counts.txt,prof/}` | M5.21 profiling evidence, method, corrections |
| `…/m522/{gate-e-prereg.md,primary_old.txt,primary_new.txt,secondary_ab.txt,alloc_counts_m522.txt,review_packet.md,ref_m58_lines_preM522.txt,m58_lines_run{1,2}.txt,base_hashes.txt,after_hashes.txt}` | M5.22 pre‑registration, raw timings, allocation evidence, corpus logs |
| `…/m520_lfe_gap.md`, `…/m520_review_glm53.md`, `…/m522/review_packet.md` | independent‑review records |
| snapshot source trees `…/m516-ab/base`, `m517-ab/base`, `m518-ab/base`, `m519-ab/base`, `m520-ab/base`, `…/m522/base` + worktree | per‑milestone source attribution (this audit's `diff` evidence) |
| `docs/aac-enc-migration-spec.md`, `docs/aac-enc-m4-quant-ratecontrol-plan.md`, `docs/aac-enc-benchmark-plan.md` | project‑level context, known divergences, unexecuted benchmark plan |

**Verification actually performed by this audit** (scripts in `…/T/opencode/m524/`):

* `pooled_ab.py` — recomputed per‑fixture pooled OLD/NEW medians, means, paired wins, exact
  binomial p‑values from the raw A/B lines (§4).
* `analyze.py` — recomputed the M5.22 primary `wall_ns` medians, and the normalized M58 digests
  across all milestones (§6).
* `allocsamples.py` — independent recount of allocator symbol samples in the M5.19/M5.20 profiles (§5).
* `diff`/`git archive` comparison of the snapshot trees — per‑milestone file attribution (§3).
* corpus manifest reproduction: `shasum -a 256 *.aac | LC_ALL=C sort | shasum -a 256` → recorded
  manifest (§6).

**Documented‑but‑not‑verifiable here (flagged, not silently trusted)**

* **M5.12–M5.15 have no preserved reports or raw runs.** No M5.12‑era directory, log, or report
  exists in the evidence set or in `docs/`. Their source changes are partially recoverable by
  diffing `HEAD` against the pre‑M5.16 snapshot, but that range also contains earlier uncommitted
  M5 work (see §3), so per‑milestone attribution for M5.12–M5.15 is **not** established.
* **M5.15 does not appear anywhere** — not in the M5.24 task's own lead list, not in the evidence,
  and not as an unexplained fourth code delta in the pre‑M5.16 snapshot. Unresolved: numbering gap
  or a non‑source (verification) milestone whose record was not preserved.
* **`HEAD` is not the post‑M5.11 state.** `HEAD` lacks `stream_driver.rs`, `phase1b.rs` and the
  `m7-*.wav` fixtures, and `docs/aac-enc-benchmark-plan.md` describes worktree changes
  ("short‑block M/S Ld fix + tests", "stress fixture + WAV loader + M5.5 arms") that belong to
  milestones **before** M5.12. The uncommitted delta therefore spans more than M5.12–M5.22.
* The FFmpeg 18/18 decode results are **review‑packet assertions**; no raw FFmpeg log was found in
  the preserved evidence.

---

## 3. Milestone matrix (M5.12–M5.22)

Categories: **C1** demonstrated runtime improvement · **C2** allocation reduction without
demonstrated runtime improvement · **C3** benchmark/instrumentation improvement ·
**C4** profiling/investigation only · **C5** correctness/maintainability/ownership ·
**C6** unresolved/insufficient evidence.

### 3.1 Verified code scope (this audit's `diff` evidence)

| Milestone | Files changed (verified) | Changed lines | Category |
|---|---|---|---|
| M5.12 | `tns.rs` — new `autocorr_quarter_lags` fused multi‑lag integer autocorrelation (per‑lag accumulator, identical `i`‑ascending order, `i64`) | 57 (HEAD→pre‑M5.16 range) | C1/C5 (documented) |
| M5.13 | `scf.rs` — `improve_scf` trial written directly into destination band slices (removed per‑band `q`/`t` temps + copy‑back) | 15 (same range) | C2/C5 (documented) |
| M5.14 | `bitenc.rs` — byte‑chunk emission replacing the per‑bit write loop | 22 (same range) | C1/C5 (documented) |
| M5.15 | **no evidence of any change** | — | C6 |
| M5.16 | `quant.rs` (only file) | 66 | C1 |
| M5.17 | `scf.rs` (only file) | 59 | C1 |
| M5.18 | `rate.rs` (+`final_quants`/`final_qmaxs` moved out), `stream_driver.rs` (deleted driver‑side re‑quantization) | 17 + 50 | C3 (measured) |
| M5.19 | `scf.rs` (`ScfWorkspace`, `*_reuse`, `s4_ws`), `stream_driver.rs` | 221 + 11 | C2 |
| M5.20 | `energy.rs`, `psy.rs` (`PsyScratch`, `process_pair_reuse`, `*_into` helpers), `stream_driver.rs` | 174 + 353 + 28 | C2 (weak C1) |
| M5.21 | **no source change** (profiling only) | — | C4 |
| M5.22 | `bitcount.rs` (`BitCountScratch`, `dyn_bit_count_reuse`), `rate.rs` (`QcState.bc_scratch`), `stream_driver.rs`, `phase1b.rs` | 210 + 20 + 33 + 1 | C2 |

Note: pre‑M5.16 cumulative deltas that are *not* attributable to M5.12–M5.15 include
`psy.rs` (grouped M/S Ld outputs `ms_energies`/`ms_ld` + ~245 lines of tests — the documented
"short‑block M/S Ld fix"), `testdata.rs` (stress fixture, WAV loader, M5.5 arms), and
`aac_enc.rs` (module/test registration). The `tns.rs`/`scf.rs`/`bitenc.rs` deltas match the
M5.12/M5.13/M5.14 descriptions closely enough to accept those three attributions as *strongly
supported*, but not as independently reported.

### 3.2 Per‑milestone record

**M5.12 — fused TNS autocorrelation** (`tns.rs`, verified in source)
* *Hypothesis:* the per‑lag repass of `calc_auto_corr_value` costs more than one fused traversal,
  and `i64` accumulation order can be preserved exactly.
* *Result:* the fused function documents "identical per‑term ops, contributing sample sets and
  `i`‑ascending order"; used on the LC path with a provably equivalent lag bound
  (`3·lag ≤ max_order+3 ⟺ lag ≤ (max_order+3)/3`).
* *Validation/perf:* **no preserved run.** Documented as byte‑identical with no speedup claim
  recorded here. **Disposition:** retained. **Limitation:** C1 evidence missing (C6 for the
  performance claim; C5 for the simplification).
* *Bound:* the pre‑M5.16 snapshot's corpus output is byte‑identical to the current worktree's, so
  M5.12–M5.15 *as a set* provably did not change encoder output — but the pre‑M5.12 baseline was
  never captured with the M58 harness.

**M5.13 — direct‑slice scalefactor scratch** (`scf.rs`, verified in source)
* Removed per‑band `vec![0i16; w]` temps plus copy‑back in the `improve_scf` trial; the comment
  records that `improve_scf` fully overwrites both slices. Same caveats as M5.12. **Disposition:** retained.

**M5.14 — byte‑chunk BitWriter** (`bitenc.rs`, verified in source)
* Replaced one bounds‑checked bit iteration per bit with one buffer access per touched byte,
  placing identical high‑bit‑first bits at identical positions.
* *Validation/perf:* no preserved run; the M5.24 lead (profiler samples attributed to
  `code_values` fell substantially) is **not verifiable** from the preserved evidence (the
  contemporary profiles are absent; the M5.21 profile shows `code_values` at 5.01 %/3.24 %/1.09 %
  of encoder self, with no pre‑M5.14 comparison available). **Disposition:** retained (C5; the
  performance claim is C6).

**M5.15 — missing.** No source delta, report, or log. **C6.**

**M5.16 — quantizer state hoist + inline** (`quant.rs`; 66 changed lines)
* *Hypothesis:* hoisting loop‑invariant quantizer state and inlining the hot inner call removes
  call/prologue overhead without touching arithmetic.
* *Workload/boundary:* interleaved whole‑invocation A/B (`m511_profile_loop`, `M511_ITERS=300`
  music / `1500` multitone, 6 rounds each) — *not* the in‑process `wall_ns` boundary later used
  for M5.22.
* *Result (recomputed):* music 23 726 → 23 092 ms median **−2.67 %** (−2.99 % mean), 6/6 wins,
  sign‑test p = 0.031; multitone 6 106 → 5 978 ms **−2.10 %** (−1.43 % mean), 4/6 wins, p = 0.69.
* *Contradictory evidence:* the sequential (non‑interleaved) before/after files
  `m516_base_timing.txt` vs `m516_after_timing.txt` show the *opposite* direction on music
  (medians 45.89 ms → 46.03 ms) and multitone (7.86 → 8.00 ms) at `iters=600/2000`. The
  interleaved method was the adopted one, and this pair illustrates how much the sequential
  protocol drifts.
* *Equivalence:* corpus byte‑identical (36/36 rows; digests unchanged). **Disposition:** retained
  (C1) — median deltas above the interleaved noise, but the magnitude rests on the interleaved
  method alone. No aggregate/compounded claim.

**M5.17 — reused scratch bands in scalefactor estimation** (`scf.rs`; 59 lines)
* *Result (recomputed):* music **−2.42 %** (6/6, p = 0.031); multitone **−4.19 %** (6/6, p = 0.031);
  speech **−0.23 %** median / −0.86 % mean (3/4, p = 0.63 → not established). Byte‑identical.
  **Disposition:** retained (C1 for music/multitone; speech negative evidence).

**M5.18 — remove duplicated final‑gain re‑quantization** (`rate.rs` + `stream_driver.rs`)
* *What changed:* the QC loop now moves its own final‑pass `quants`/`qmaxs` out in
  `QcFrameOutput`; the driver's clone‑zero‑re‑quantize block was deleted, with a four‑point
  equivalence argument (recalc on every gain change, crash‑path `max_sfb` reduction, bit‑identical
  pre‑`final_max` bands, `vec![0i16; len]` initialization beyond `final_max`).
* *Result (recomputed):* music **−3.69 %** (6/6), multitone **−3.73 %** (5/6), speech **−3.31 %**
  (4/4). Byte‑identical.
* *Classification nuance:* the *removed* work lived in the test driver; the C reference's
  `FDKaacEnc_QCMain` hands its final spectrum to the writer
  (`qcOutCh->quantSpec` produced at `cpp/libAACenc/src/qc_main.cpp:955`, consumed at
  `cpp/libAACenc/src/bitenc.cpp:1115` — verified in source by this audit), so the duplicate pass
  existed only in the Rust harness. This is a genuine reduction of the timed region and improves
  benchmark validity, but it is **not** evidence that the encoder core became 3 % faster.
  **Disposition:** retained (C3, with measured effect).

**M5.19 — per‑encoder `ScfWorkspace`** (`scf.rs` + `stream_driver.rs`)
* *Hypothesis:* the S4 path allocated scratch per call; one workspace reused across frames and
  channels removes that.
* *Result (recomputed):* music −0.15 % pooled median (5/10 wins, p = 1.00); multitone +0.72 %
  median / −0.03 % mean (4/12); speech +0.01 % (1/4). **Runtime flat.**
* *Allocation claim:* the campaign reported "allocator samples −33 %"; this audit's independent
  recount of allocator‑symbol samples in the two preserved single‑rep profiles gives 443 → 377
  (**−14.9 %**). M5.19 preserved **no** counting‑allocator run, so the −33 % figure is **not
  verified**; only the direction (fewer allocator samples) reproduces. **Disposition:** retained
  (C2/C5 — allocation hygiene; flat runtime documented).

**M5.20 — psychoacoustic scratch reuse** (`energy.rs`, `psy.rs`, `stream_driver.rs`)
* *What changed:* `PsyScratch` + `process_pair_reuse` (caller‑owned output buffers), `_into`
  energy helpers writing into caller buffers, `finish_channel` clone → move, `psy_ws` field.
* *Allocation evidence (instrumented temp copies):* psy path 4 973 520 → 2 881 680
  (**−42.06 %**, music) and 451 680 → 251 160 (**−44.41 %**, multitone); whole loop
  660 240 → 573 080 (**−13.20 %**, music); the milestone summary reported speech psy −43.6 %,
  loop −13.4 % (raw counters for speech are not in the preserved packet). Note the review packet
  mixed denominators in one table: the psy counts are per **120 iterations** and the loop counts
  per **5 iterations**. This audit cross‑checked the per‑iteration values against the independent
  M5.21 counters of the same post‑M5.20 binary — music psy 24 014/iter, multitone psy 2 093/iter,
  music total 114 616/iter — all match exactly.
* *Timing (recomputed pooled):* music −0.63 % (8/10, p = 0.11), multitone −1.56 % (8/10, p = 0.11),
  speech −0.80 % (6/8, p = 0.29). Above the point estimates' naive noise but **not** individually
  significant at the 5 % level; the reviewer's combined 22/28 paired‑win sign test (p ≈ 0.004) is
  the strongest statement available.
* *Equivalence:* 36/36 corpus rows identical, determinism rerun identical, corpus manifest and
  18/18 FFmpeg decode, plus a **cross‑frame LFE probe** hashing every output field including float
  bit patterns: `M520LFE acc=e430d28b8518bf0f` identical in independently built old/new trees.
* *Independent review (GLM‑5.3):* RETAIN, high confidence, after five challenge classes were traced
  in code; reviewer reproduced the corpus, suites, allocation counts and timing medians
  independently. Gaps closed post‑review (LFE); two pre‑existing out‑of‑contract panics documented.
* **Disposition:** retained (C2, weak C1).

**M5.21 — profiling/investigation only** (no source change)
* 9 fresh profiles (3 fixtures × 3 reps, `/usr/bin/sample` 1 ms), parsed with a call‑tree
  reconstruction (`parse_profiles5.py`); encoder subtree = 86.8 %/80.5 %/71.4 % of worker samples
  (music/speech/multitone) after the reviewer's harness‑split correction.
* Top self‑time shares (music/speech/multitone): `quant::calc_sfb_dist` 31.1/24.1/10.3;
  `scf::estimate_channel_with_inv_ws` 9.0/8.2/9.5; `bitenc::code_values` 5.0/3.2/1.1;
  `psy::process_pair_reuse` 1.7/2.7/4.0; allocator symbols ≈0.6–1.6 %.
* *Limitations (self‑reported and corrected):* 1 ms sampling, 3 reps, 10–30 % relative spread on
  small shares; `???` bucket is unsymbolized synthesis (harness), not encoder; the harness split
  was initially mis‑parsed; "per‑run spread ≤ 3 %" was wrong (real ranges 2.8–11 %);
  the timed boundary excludes the validation walk. Conclusion adopted: sub‑1 % profile
  magnitudes are not actionable; the C1 wall‑clock hypothesis violated a ceiling argument.
* **Disposition:** C4; recommended the bit‑count scratch reuse (which became M5.22) and recorded
  the allocation‑hook undercount (4 128 → 11 634/iteration; 3.6 % → 10.2 %).

**M5.22 — bit‑count scratch reuse** (`bitcount.rs`, `rate.rs`, `stream_driver.rs`, `phase1b.rs`)
* *Pre‑registered gate (frozen before measurement):* win only if median EncoderOnly improvement
  > 1.67 ms (music) **and** > 2.54 ms (speech) — the M5.21‑measured run‑to‑run ranges; otherwise
  retain as simplification only if allocation reduction ≥ 3 % with all correctness gates green.
* *Primary result (recomputed from `primary_old/new.txt`, 8 alternating rounds, in‑process
  `wall_ns`):* music −0.052 ms (**−0.09 %**, NEW+OLD) and speech +0.412 ms (**+1.10 %**).
  **Gate FAILED on both fixtures.** Secondary arm: music −0.40 %, speech +1.54 %, direction
  inconsistent, 1/6 and 2/4 paired NEW wins.
* *Allocation evidence:* identical instrumentation on both sides (hooks on `alloc`,
  `alloc_zeroed`, `realloc`): music 114 616 → 107 893/iteration (**−5.9 %**), speech −5.8 %,
  multitone −5.4 %; in‑path delta equals total delta exactly; psy counts unchanged; the remaining
  4 911/iteration in‑path allocations are the `out_sections` return value, **intentionally
  retained**.
* *Equivalence:* 36/36 corpus rows, 18/18 file hashes, determinism, full suite 438 passed / 1
  known unrelated failure / 4 ignored (the new test is the +1).
* *Independent review (Muse 1.3 Contributor):* no correctness defect found after coverage gaps
  (PNS/IS/multi‑group shapes through a shared scratch) were addressed; verdict
  **RETAIN AS SIMPLIFICATION ONLY**.
* **Disposition:** C2 (+C5). **No runtime speedup is claimed.**

---

## 4. Verified performance results and measurement limitations

### 4.1 Pooled interleaved A/B (whole‑invocation `m511_profile_loop`; recomputed by this audit)

`iters` = profile‑loop iterations; n = paired rounds; deltas are NEW−OLD of medians (negative =
faster). These are the campaign's comparable measurements and are **not** comparable with M5.22's
in‑process boundary or with the C reference.

| Milestone | Fixture (iters) | n | OLD med (ms) | NEW med (ms) | Δmedian | Δmean | NEW wins | sign p |
|---|---|---|---|---|---|---|---|---|
| M5.16 | music (300) | 6 | 23 726 | 23 092 | **−2.67 %** | −2.99 % | 6/6 | 0.031 |
| M5.16 | multitone (1500) | 6 | 6 106 | 5 978 | −2.10 % | −1.43 % | 4/6 | 0.69 |
| M5.17 | music (300) | 6 | 22 964 | 22 409 | **−2.42 %** | −2.46 % | 6/6 | 0.031 |
| M5.17 | multitone (1500) | 6 | 5 998 | 5 746 | **−4.19 %** | −4.01 % | 6/6 | 0.031 |
| M5.17 | speech (300) | 4 | 15 544 | 15 507 | −0.23 % | −0.86 % | 3/4 | 0.63 |
| M5.18 | music (300) | 6 | 22 546 | 21 714 | −3.69 % | −3.58 % | 6/6 | 0.031 |
| M5.18 | multitone (1500) | 6 | 5 784 | 5 568 | −3.73 % | −3.17 % | 5/6 | 0.22 |
| M5.18 | speech (300) | 4 | 15 078 | 14 580 | −3.31 % | −3.38 % | 4/4 | 0.125 |
| M5.19 | music (300) | 10 | 21 080 | 21 048 | −0.15 % | −0.40 % | 5/10 | 1.00 |
| M5.19 | multitone (1500) | 12 | 5 360 | 5 398 | +0.72 % | −0.03 % | 4/12 | 0.39 |
| M5.19 | speech (300) | 4 | 14 542 | 14 543 | +0.01 % | −0.04 % | 1/4 | 0.63 |
| M5.20 | music (300) | 10 | 21 025 | 20 894 | −0.63 % | −0.75 % | 8/10 | 0.11 |
| M5.20 | multitone (1500) | 10 | 5 464 | 5 379 | −1.56 % | −0.99 % | 8/10 | 0.11 |
| M5.20 | speech (300) | 8 | 14 364 | 14 248 | −0.80 % | −0.48 % | 6/8 | 0.29 |
| M5.22 | music (300) | 6 | 20 788 | 20 872 | +0.40 % | +0.66 % | 1/6 | 0.22 |
| M5.22 | speech (300) | 4 | 14 523 | 14 299 | −1.54 % | −0.51 % | 2/4 | 1.00 |

Caveats that apply to every row: (a) each "measurement" is a whole test‑binary invocation
(process start, cargo test harness, fixture synthesis in `testdata`, PCM interleave, encode,
ADTS assembly) so the *delta* is dominated by encoding but the *absolute* numbers are not
encoder‑only; (b) the OLD binary for each milestone is the previous milestone's state, so these
are **cumulative pairwise** comparisons, not independent effects; (c) medians of 4–12 samples
cannot resolve sub‑percent effects (as M5.19/M5.20 demonstrate); (d) the same workload re‑run
later would not reproduce these absolute values (M5.16 runs are ~13 % slower than M5.20 runs of
the same fixture — thermal/OS drift across the session is larger than several claimed deltas).

### 4.2 Current state, in‑process boundary (post‑M5.22; recomputed medians over 16 measured
corpus runs, 8 per binary, `M58_MEASURE_TIMING=1`, `m58_corpus_validation`)

Boundary as implemented in `stream_driver.rs::m58_run_once` (verified in source):
**EncoderOnly** = `StreamEncoder::new` + per‑frame loop (per‑frame input `to_vec()` clones,
`step_frame`, ADTS emit) — no fixture synthesis, no feeder, no validation walk, no file write.
**EndToEnd** = PCM interleave + feeder push/finish + the EncoderOnly region + ADTS file write.
The post‑loop ADTS validation walk is never timed.

| fixture | frames | bytes | EncoderOnly ms | EndToEnd ms | EndToEnd rt× | output hash |
|---|---|---|---|---|---|---|
| `m2-silence-stereo-48k` | 49 | 735 | 1.107 | 1.315 | 761× | `b29a7020dfcbba25` |
| `m2-sine-stereo-48k` | 49 | 3147 | 2.360 | 2.521 | 397× | `6dbdf1c6dba9495d` |
| `m2-onset-stereo-48k` | 49 | 2359 | 1.760 | 1.948 | 513× | `dfb35da036ced419` |
| `m2-stepdown-stereo-48k` | 49 | 2236 | 1.731 | 1.896 | 528× | `2d83fdd5d13c1839` |
| `m2-click-mono-48k` | 25 | 2680 | 0.972 | 0.982 | 509× | `cd62de0eb0ef32a0` |
| `m2-impulse-mono-48k` | 11 | **306** | 0.189 | 0.221 | 903× | `162253a74c40bfbe` |
| `m2-edge-mono-48k` | 14 | 415 | 0.212 | 0.264 | 946× | `266d6c7463ee3c1e` |
| `m3-multitone-stereo-48k` | 49 | 4819 | 2.761 | 2.877 | 348× | `16039942f15b6549` |
| `m3-sweep-stereo-48k` | 49 | 3988 | 2.473 | 2.637 | 379× | `3e1a44ce44ff09c5` |
| `m3-noise-mono-48k` | 25 | 10603 | 1.347 | 1.398 | 358× | `8d19ed38df26235d` |
| `m3-tonal-noise-stereo-48k` | 25 | 10630 | 1.777 | 1.888 | 265× | `d8632bfc3a2d34fd` |
| `m3-quiet-sine-stereo-48k` | 49 | 4200 | 2.504 | 2.636 | 379× | `77b37435c84a3809` |
| `m3-sine-stereo-441k` | 45 | 2998 | 2.469 | 2.600 | 385× | `088f74920ee74115` |
| `m3-onset-stereo-441k` | 45 | 2250 | 1.780 | 1.937 | 516× | `aaea1112b2490f7d` |
| `m6-stress-stereo-48k` | 25 | 10728 | 1.851 | 1.932 | 259× | `a6b8983026d3a7e5` |
| `m7-music-stereo-48k` | 471 | 250 926 | **58.966** | 66.365 | 151× | `3fa9ba003cc53a99` |
| `m7-speech-pauses-stereo-48k` | 471 | 125 818 | 37.194 | 43.927 | 228× | `776d715a406fe819` |
| `m7-speech-clean-stereo-48k` | 471 | 127 900 | 37.441 | 44.368 | 225× | `1fd9ebc0adbdcae9` |

For reference, the M5.21 baseline runs of the same boundary gave run‑to‑run ranges of 1.67 ms
(2.8 %) for music and 2.54 ms (6.7 %) for speech EncoderOnly — the ranges that became the M5.22
gate. Any future performance claim must exceed these ranges on this boundary, with interleaved
A/B and a pre‑registered criterion.

### 4.3 Measurement limitations to carry forward

* **Harness overhead inside the timed region.** Per‑frame input `to_vec()` clones and the emit
  path sit inside EncoderOnly; the feeder and file write are EndToEnd‑only. Removing harness work
  must never be reported as an encoder speedup (M5.18 is the live example).
* **Noisy, short measurements.** Fixtures with 11–49 frames encode in 0.2–2.9 ms; sub‑millisecond
  resolution and OS jitter dominate. Only the 10 s m7 fixtures give usable absolute numbers, and
  even there the run‑to‑run spread is ≈3–7 %.
* **Historical allocation‑instrumentation omission.** The M5.21‑era counter hooked only `alloc`,
  undercounting by ≈2.8× (4 128 vs 11 634/iteration); with `realloc`/`alloc_zeroed` hooked the
  figures became stable and (for M5.22) exactly reconcilable with the total loop delta. Any
  pre‑M5.22 allocation share taken from that copy is unreliable.
* **Noisy profiler magnitudes.** 1 ms sampling with 3 reps gives 10–30 % relative spread on small
  shares; the encoder subtree split itself was mis‑parsed once (corrected by the reviewer).
* **C‑versus‑Rust comparison is not like‑for‑like.** The only attempt (M5.21) used the C CLI
  end‑to‑end (process start + WAV read + file write) versus an in‑process Rust time: music 55 ms
  vs 59.2 ms, speech 40 ms vs 38.2 ms, i.e. ≈1.25× / ≈1.16×, with output sizes agreeing to
  ≲0.15 % (music 251 279 vs 250 926 B; speech 127 971 vs 127 900 B). The milestone itself
  labelled this a comparability statement, not a ratio. `docs/aac-enc-benchmark-plan.md` (M5.7,
  **unexecuted**) prescribes the methodology that would make it valid.
* **A/B protocol sensitivity.** The sequential before/after pair preserved for M5.16 points the
  opposite way from its interleaved A/B; only interleaved results were adopted, and this pair is
  the evidence for why.
* **Failed gates are not retrofittable.** M5.22's threshold (>1.67 ms / >2.54 ms) was frozen
  before measurement and is restated as failed here; the retention decision used only the
  pre‑registered *fallback* condition (≥3 % allocation reduction with green gates).

---

## 5. Allocation and instrumentation findings

| Milestone | Method | Evidence | Result | Verified by this audit |
|---|---|---|---|---|
| M5.19 | allocator symbol samples in `sample` profiles (no counter) | `m519_prof_music_{old,new}.txt` (single rep) | campaign reported "−33 % allocator samples" | **No** — recount gives 443→377 samples (−14.9 %); direction only |
| M5.20 | counting global allocator in temp copies (hooks on `alloc` only) | `m520_review_glm53.md` | psy −42.06 % (music) / −44.41 % (multitone); loop −13.20 % (music) / −12.72 % (multitone); speech only from the milestone summary | **Partly** — the per‑iteration NEW values cross‑check exactly against the independently preserved M5.21 counters (24 014 / 2 093 psy, 114 616 total); this audit did not re‑run the instrumented trees |
| M5.21 | same counter, hooks still `alloc` only | `m521/alloc_counts.txt` | 114 616 allocs/iteration total; psy 24 014 (21.0 %); bit‑count 4 128 (3.6 %) → **undercount** | **Yes** as evidence, with the documented hook defect |
| M5.22 | counter with `alloc` + `alloc_zeroed` + `realloc`, identical both sides | `m522/alloc_counts_m522.txt` | 114 616 → 107 893/iter (−5.9 %) music; −5.8 % speech; −5.4 % multitone; in‑path Δ = total Δ; bit‑count path 11 634 → 4 911/iter; psy unchanged | **Yes** — arithmetic re‑checked from the raw counters |

Structural findings that survive the method changes:

* Allocation pressure was **21 % psy** and **≈10 % bit‑count path** per music iteration (the
  latter corrected upward from 3.6 %); the remaining ~70 % was never attributed per call site.
* Only the bit‑count path's per‑group `vec![...]` construction was removable without changing the
  production API shape; the `out_sections` return `Vec` (4 911/iteration) is the function's result
  and was deliberately kept.
* Allocation reduction repeatedly failed to translate into time: M5.19 removed ≈15–33 % of
  allocator samples for flat runtime, and M5.22 removed 5.9 % of allocations for a failed timing
  gate. The profile‑measured allocator time share is ≈0.6–1.6 % of encoder self, so allocation
  work is **not** by itself a lever for this encoder.
* Instrumentation lived only in temporary copies; the worktree contains no allocator hooks (the
  M5.20 and M5.22 reviewers verified this independently).

---

## 6. Validation and independent‑review evidence

**Byte‑identity chain (recomputed by this audit).** The normalized 36‑row M58 corpus digest is
`5f58213f566aefe8…` in: `m516_base_m58_full.txt` (pre‑M5.16), `m516_final`, `m517_base`,
`m517_after`, `m518_after`, `m519_{old,new,new_rerun}`, `m520_{ref,new,new_rerun}`,
`m522/{ref_m58_lines_preM522,m58_lines_run1,m58_lines_run2}` and the current worktree state.
Additionally, corpus file hashes (`base_hashes.txt` vs `after_hashes.txt`) match per milestone and
the 18 durable fixture files still reproduce the manifest `201b292d…`. So: every milestone from
the pre‑M5.16 snapshot to the post‑M5.22 worktree produced **byte‑identical** corpus output, and
reruns are deterministic.

**Per‑milestone correctness gates.** Corpus (18 fixtures × 2 modes), internal ADTS header
re‑parse inside `m58_run_once` (code verified), fixture‑level output hashes, and determinism
reruns. The full test suite went 437 passed / 1 failed / 4 ignored (M5.20 era) → 438 / 1 / 4
(M5.22, the new scratch test being the +1); the single failure is the known unrelated
`tp_dec::pce::tests::read`, reproduced independently by the M5.20 reviewer on a baseline copy
(hence pre‑existing, not a regression).

**Independent reviews.**

| Milestone | Reviewer | Scope | Verdict |
|---|---|---|---|
| M5.20 | GLM‑5.3 (`opencode-go/glm-5.3`) | 5 challenge classes (lifetimes, isolation, FP order, length‑sensitive consumers, methodology); independent rebuilds | **RETAIN, high confidence**; gaps: cross‑frame LFE (closed by probe, `M520LFE acc=e430d28b8518bf0f` bit‑identical), mixed‑window CPE panic (pre‑existing), OLD‑binary provenance by timestamp |
| M5.21 | Muse 1.3 Contributor | measurement hygiene; proposed reasons *not* to optimize C1 | Adopted: C2 first, C1 ceiling argument, 4 packet corrections |
| M5.22 | Muse 1.3 Contributor | reject‑oriented inspection of the reuse protocol, `QcState` ownership, hook completeness | **No correctness defect found**; RETAIN AS SIMPLIFICATION ONLY |

**FFmpeg decode.** Reported as 18/18 clean by both the M5.20 and M5.22 reviews; **no raw log found**
in the preserved evidence — treat as reviewer‑attested, not independently re‑verifiable here.
This audit did **not** re‑run any corpus, suite, or benchmark (M5.23/M5.24 constraints).

**Pre‑existing out‑of‑contract behaviours (identical before and after, not regressions).**
Short‑window LFE panics at `energy.rs:194` (the production driver hardcodes `is_lfe: false`);
mixed‑window CPE (`short_l ≠ short_r`) panics in the MS block.

---

## 7. What this campaign does **not** establish about encoder quality

1. **Regression hashes prove consistency with the tested Rust baseline, not equivalence to C
   FDK AAC.** Every "byte‑identical" claim in this report is a comparison against the *immediately
   preceding Rust version*. The campaign produced no C‑versus‑Rust bitstream comparison and no
   comparison at all before the pre‑M5.16 snapshot.
2. **No quality metric was measured.** No SNR/SDR, NMR, PE comparison, bitrate distribution,
   spectral comparison, or listening test was run at any point in M5.12–M5.22. Output‑size parity
   with C (≲0.15 % on two fixtures, M5.21) is a configuration check, not a quality statement.
3. **Documented divergences remain open.** The mono impulse fixture emits **306 B** in the current
   corpus (11 frames), while the Phase‑0 C baseline for the same 0.2 s mono impulse measured
   **308 B** (`docs/aac-enc-migration-spec.md` §6) — an *observed* two‑byte difference that the
   performance campaign neither explained nor touched. Separately, `docs/aac-enc-benchmark-plan.md`
   §4 carries the documented "impulse‑f5 16‑bit residual (M5.4)" note; this audit did **not**
   verify whether the two facts are the same underlying divergence. Tier‑B float‑lane divergences
   (Phase 1I) and the stale SHORT‑dump references are likewise carried, unresolved; the benchmark
   plan states explicitly that byte identity is *not* a pass condition where documented divergences
   exist.
4. **Active‑filter spectrum differences have unverified quality impact.** The M3 checkpoint
   (`docs/aac-enc-migration-spec.md` §11.5) pins 21 maxscale and 40 TNS‑window divergences as
   expected (`KNOWN_MAXSCALE`/`KNOWN_TNS`) and records that "active‑filter spectrum differences
   (f24/f25/f2) change the encoded spectrum; quality impact unverified" — and that LO‑active /
   merged TNS paths have zero corpus coverage. The performance campaign changed psychoacoustic and
   quantization code (M5.16, M5.17, M5.20) without touching that question.
5. **Two window/flush‑class behaviours remain out of contract** (short‑window LFE panic,
   mixed‑window CPE panic). They are pre‑existing and out‑of‑reach of the fixtures, but nothing in
   this campaign shows they cannot occur on real input; they are robustness risks, not quality
   evidence.
6. **A non‑changing hash is not perceptual safety.** M5.20 rewrote buffer lifetimes and FP
   operation order in the psychoacoustic path and M5.16/M5.17 altered quantizer/scf code; identical
   output hashes show those edits were *semantically inert on these 18 fixtures*, not that the
   encoder matches C, nor that the unexercised paths (LFE, mixed windows, LO‑TNS, PNS/IS shapes on
   long/short transitions) are correct.
7. **Float‑native design is intentional.** Per the migration spec §5 the port preserves mathematics
   and decisions, not bit patterns; architecture‑specific fixed‑point references do not apply. Any
   future acceptance test must be behavioural (bitrate, frame‑size histogram, NMR/PE distributions,
   SNR/SDR floors), never byte identity — and the campaign's byte‑identity gates, however useful
   for refactoring, must not be promoted to quality gates.

---

## 8. Ranked future directions

Ordering follows the project priority (quality → robustness → safe performance → production
usability). Each direction lists the evidence that justifies it, the expected value, the main
uncertainty, and the precondition for starting.

### A. Quality and correctness review (recommended next)
* **Evidence:** §7 in full — no quality metric ever measured; the impulse 306 vs 308 B divergence
  is live in the current corpus table; Tier‑B and active‑filter divergences are documented and
  unquantified; `KNOWN_MAXSCALE`/`KNOWN_TNS` pins and the M5.20 LFE/CPE panics show where the
  fixtures are blind.
* **Expected value:** highest. It converts "byte‑identical to ourselves" into an actual statement
  about the encoder's fidelity, decides whether the pinned divergences are acceptable, and removes
  the risk that future speed work is optimized on top of an undiscovered quality defect.
* **Major uncertainty:** how large the C/Rust quality gap is at all — possibly small (output sizes
  agree to ≲0.15 %), possibly material on the diverging elements.
* **Must be established first:** (i) delay‑compensated SNR/SDR or NMR/PE comparison on all 18
  fixtures against Phase‑0 C output, with the documented divergences carried as expected and
  anything else failing loudly; (ii) a decision record for each pinned divergence
  (accept / fix / quarantine); (iii) an audit of the unexercised paths (LFE, mixed windows,
  LO‑TNS, PNS/IS) to convert panics into explicit contract errors.
* **Must not** be a performance milestone, and must not use ABX listening tests.

### B. Trustworthy C‑versus‑Rust performance baseline
* **Evidence:** the only comparison is the coarse M5.21 CLI‑versus‑in‑process ratio (≈1.25×/1.16×,
  non‑identical boundaries); the C side offers no separable encoder‑only entry, the Rust side's
  encoder instance lives in a test driver; `docs/aac-enc-benchmark-plan.md` (M5.7) already specifies
  the methodology but was never executed.
* **Expected value:** high — without it, no performance claim about this encoder can be placed in
  context, and future optimization has no denominator.
* **Major uncertainty:** whether the C side can be measured at a boundary comparable to
  EncoderOnly without touching production C code (the migration spec forbids changing it and the
  project restricts instrumentation to test‑only shims).
* **Must be established first:** an agreed, documented boundary pair with the difference quantified
  (e.g. instrument a test‑only C harness that halts before file write, or state the end‑to‑end
  boundary for both and exclude I/O explicitly); warm‑up, N≥5 passes, per‑fixture medians and
  spreads, tool versions, and the known‑divergence list from direction A attached to every report.

### C. Further micro‑optimization
* **Evidence:** `calc_sfb_dist` alone is 31 %/24 %/10 % of encoder self (music/speech/multitone,
  M5.21) but no correctness‑preserving transformation has been identified; TNS autocorrelation,
  block switch, FFT and bit writing are the next cluster; the allocation lever is exhausted in
  practice (M5.19 flat, M5.22 gate failed) and remaining allocation sites are API‑shaped
  (`out_sections`, psy outputs).
* **Expected value:** potentially high in principle, currently unproven — the only candidates with
  clean mechanisms are already implemented.
* **Major uncertainty:** whether any of the hot loops admits an exactness‑preserving improvement at
  all (M5.16 already removed call overhead in `calc_sfb_dist`); past experience shows allocation
  shuffling does not move wall clock here.
* **Must be established first:** a profiling‑supported candidate with an identified mechanism, a
  *direct time‑mechanism experiment* (not an allocation‑count analogy), and a pre‑registered gate
  that exceeds the measured run‑to‑run range on the in‑process boundary with interleaved A/B.
  Do not start from "a hotspot exists".

### D. Production API and integration readiness
* **Evidence:** the encoder is driven only by untracked test code (`stream_driver.rs`, `phase1b.rs`);
  `HEAD` tracks no driver at all; there is no production entry point, no public flush/EOF API
  (the `-1` flush + `nDelay` zero‑pad live in the harness/spec, not in an API), and no error
  contract beyond panics on out‑of‑contract input.
* **Expected value:** medium‑high for adoption, but it is a build‑out, not an investigation; it also
  *enables* fair benchmarking (direction B) by separating encoder work from test scaffolding.
* **Major uncertainty:** scope — a usable API needs streaming/EOF semantics (the `-1` flush and
  `nDelay` zero‑pad), config validation, and a robustness contract for the panic cases in §7.5.
* **Must be established first:** direction A's correctness decisions (the API must not freeze
  behaviour that is still unresolved), plus an explicit contract for the out‑of‑contract paths.

**Recommended next milestone: M5.25 — C‑versus‑Rust quality and divergence audit (direction A).**
Concretely: build a test‑only comparison harness over the existing 18‑fixture corpus that decodes
both encoders' streams with FFmpeg and computes delay‑compensated SNR/SDR and per‑fixture bitrate/
frame‑size histograms; classify every divergence (impulse 306/308 B, Tier‑B, active‑filter TNS,
C/Rust size diffs) as accepted/fixable/quarantined with recorded rationale; and turn the two
out‑of‑contract panics into explicit errors or documented constraints. Exit criterion: a written
quality statement with a numeric bound per fixture, or an explicit "not established" for each
metric that cannot be defended.

---

## 9. Source evidence index

**Primary raw evidence** (`E = /Users/USER/backups/fdk-aac-m5.23-worktree-20261016T193606Z/evidence`,
`T = /private/var/folders/*/T/opencode`)

* Interleaved A/B: `T/m516_ab.txt`, `T/m517_ab.txt`, `T/m518_ab.txt`,
  `T/m519_ab.txt` + `T/m519_ab_ext.txt`, `T/m520_ab.txt` + `T/m520_ab_ext.txt`,
  `T/m522/secondary_ab.txt` (+ the matching `*_ab.sh` scripts).
* Corpus/identity: `T/m516_{base,final}_m58_full.txt`, `T/m517_{base,after}_m58_full.txt`,
  `T/m518_after_m58_full.txt`, `T/m519_{old,new,new_rerun}_m58…`, `T/m520_{ref,new,new_rerun}_m58…`,
  `T/m522/{ref_m58_lines_preM522,m58_lines_run1,m58_lines_run2,corpus_run1,corpus_final}.txt`,
  `T/m5*_{base,after,final}_hashes.txt`; durable fixtures
  `/var/folders/*/T/m52-rust-aac/m58-corpus/*.m58.aac` (manifest
  `201b292d…`).
* In‑process timing: `T/m522/primary_old.txt`, `T/m522/primary_new.txt`, `T/m522/primary_ab.sh`,
  `T/m521/timing_run{1,2,3}.txt`.
* Pre‑registration: `T/m522/gate-e-prereg.md`.
* Allocation: `T/m521/alloc_counts.txt`, `T/m522/alloc_counts_m522.txt`, instrumented temp trees
  `T/m520-alloc/{old,new}(-target)`, `T/m522/alloc-new(-target)`.
* Profiling: `T/m521/prof/prof_*_{r1,r2,r3}.txt`, `T/m521/parse_profiles5.py`,
  `T/m521/parsed_summary_v5.txt`, `T/m519_prof_music_{old,new}.txt`,
  `T/m520_prof_{old,new}_rep{1,2,3}.txt`.
* Reviews: `T/m520_review_glm53.md`, `T/m521/review_packet.md`, `T/m521/reviewer_corrections.md`,
  `T/m522/review_packet.md`; LFE probe `T/m520_lfe_gap.md`; diff `T/m522/m522.diff`.
* Provenance: `T/m521/binary-provenance.txt`, `T/m521/fixture-input-hashes.txt`,
  `T/m521/corpus-manifest.txt`, `T/m522/provenance.txt`.
* Preserved production binaries: `E/production-binaries/` (pre‑M5.21, pre‑M5.22 `19d09d10…`,
  post‑M5.22 `5a3c62cb…`).

**Repository** (all read‑only for this report): `docs/aac-enc-migration-spec.md` (§5, §11.4–11.5),
`docs/aac-enc-m4-quant-ratecontrol-plan.md`, `docs/aac-enc-benchmark-plan.md` (M5.7, unexecuted),
`rust/src/aac_enc/{stream_driver.rs,framing/…}` as cited inline.

**This audit's derived artifacts**: `T/m524/analyze.py`, `T/m524/pooled_ab.py`,
`T/m524/allocsamples.py` (plus their printed outputs, captured in the session log).

---

## 10. Open questions and discrepancies (recorded, not resolved)

1. **M5.15 is missing** from the campaign record; no source delta is attributable to it.
2. **M5.12–M5.15 have no preserved reports or runs**, and their source scope cannot be isolated
   from earlier uncommitted M5 work (the pre‑M5.16 snapshot already contains the M5.4/M5.5/M5.7‑era
   psy/testdata/harness changes).
3. **M5.14's profiler‑sample claim** ("`code_values` samples fell substantially") is unverifiable —
   no pre‑M5.14 profile is preserved.
4. **M5.19's "−33 % allocator samples"** is not reproducible from the preserved single‑rep
   profiles (−14.9 % by this audit's recount); M5.19 has no counting‑allocator run at all.
5. **M5.16's sequential timing files contradict its interleaved A/B**; only the latter was adopted
   (protocol sensitivity, not a code discrepancy).
6. **M5.21's bit‑count allocation share (3.6 %)** was later corrected to 10.2 % because the counter
   missed `realloc`/`alloc_zeroed`; the M5.22 gate's *mechanism expectation* was therefore based on
   an undercount, while its *success criterion* was correctly based on measured run‑to‑run ranges.
7. **FFmpeg decode validation** is reviewer‑attested only; no raw log is preserved.
8. **No quality‑relevant artifact exists anywhere in the campaign evidence** — the single largest
   gap in this body of work.
