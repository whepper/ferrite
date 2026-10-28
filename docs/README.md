# Documentation map

This index separates maintained guidance from historical records.
Follow the maintained documents; treat the archive as evidence of how
decisions were reached, not as current normative guidance.

## Maintained guidance

### Converter

- [L0 loudness decisions](aac-convert-l0-loudness-decisions.md) — frozen
  owner-approved loudness decisions D1–D7.
- [L2 loudness integration design](aac-convert-l2-loudness-integration-design.md) —
  finalized design; normative decisions are in §10.
- [G0 metadata decisions](aac-convert-g0-decisions.md) — core-9 metadata
  mapping and edge-case decisions (M1 slice).
- [S1.3 cover-art final specification](aac-convert-s1.3-cover-art-final-spec.md) —
  authoritative cover-art behavior contract (selection, warnings, order).
- [S2.1 24-bit FLAC plan](aac-convert-s2.1-24bit-plan.md) — 24-bit truncation
  acceptance policy (§§2 and 5 as implemented).
- [M8.1 batch design](aac-convert-m8.1-batch-design.md) — batch conversion
  architecture, CLI, and output-safety invariants.

### Encoder

- [Migration specification](aac-enc-migration-spec.md) — float-native
  thesis (§5).
- [M6 API contract](aac-enc-m6-api-contract.md) — production encoder API
  contract.
- [VBR-5 profile](aac-enc-vbr5-profile.md) — fixed encoder operating point.
- [Codec provenance inventory](aac-enc-provenance-inventory.md) —
  source-provenance inventory for the Rust codec port: upstream
  comparisons, evidence, and unresolved gaps (evidence record, not
  normative guidance).
- [Performance baseline and profiling audit](aac-enc-performance-baseline.md) —
  release-throughput baseline on the committed corpus (≈229× realtime for
  the 19 fixtures, ≈194× including a 40 s derived music workload) and
  `sample`-based hotspot attribution: the S4 scalefactor/quantization search
  is ~50% of encoder self time and `quant::calc_sfb_dist` alone ~24%; the
  bit path is ~10%, the psychoacoustic/transform front-end ~26%, and rate
  control ~2%. The follow-up `calc_sfb_dist` bounds-check experiment was
  retained: paired medians **2.82%** (test build) and **3.01%** (non-test
  production example), with all fixture hashes unchanged. The 2026-10-27
  front-end pass measured 26.70% aggregate / 51.25% onset self time and
  recommended a direct-slice write loop in `tns::scale_up_spectrum`
  (2.48% corpus / 5.50% onset; isolated 7.8× loop shape). That experiment was
  retained: paired medians **1.93%** (test build) and **1.95%** (non-test
  production example), 7/8 positive pairs, all hashes unchanged; the TNS
  self share fell from 6.66% to 4.56% on the corpus. The 2026-10-28 bit-path
  re-profile found `code_values`/`dyn_bit_count_reuse`/`bit_count` at
  3.76%/2.98%/2.77% corpus self; a safe `bit_count` chunks cleanup was tested
  but failed the 1% gate (median +0.03%) and was reverted. The subsequent
  `code_values` investigation found the measured cost in the inlined
  `BitWriter::write_bits` byte loop. An 8-byte-window writer change was
  retained: paired medians **2.10%** (test build, 8/8 pairs ≥1%) and
  **2.07%** (non-test production example), all hashes unchanged;
  `code_values` self fell from 4.18% to 2.19% on the corpus and 4.97% to
  3.13% on music. The 2026-10-28 scalefactor-search investigation localized
  the hot cost to a 60-element per-candidate trial copy in
  `assimilate_multiple_scf2`. Hoisting it was exactly equivalent but measured
  only **+0.60%** median (3/4 pairs positive, 0/4 ≥1%), so it was reverted;
  a branchless min/max follow-up had no measurable headroom. No
  scalefactor-search change is retained. The subsequent `quantize_spectrum`
  investigation unified the duplicated positive/negative magnitude paths in
  the shared `quantize_line_body` helper. That change was retained: paired
  medians **7.38%** (test build, 8/8 pairs ≥1%) and **7.59%** (production
  example), all hashes unchanged; `quantize_spectrum` self fell from 4.62% to
  2.06% and `calc_sfb_dist` from 24.62% to 22.18% on the corpus. A subsequent
  `calc_sfb_quant_energy_and_dist` two-line unroll was exactly equivalent but
  regressed **−0.41%** median (1/4 pairs positive), so it was reverted. No
  change from that investigation is retained. A `pe::form_factor`
  investigation then found the cost concentrated in the inlined fixed-point
  square root per spectral line; the only plausible micro-transforms were
  each far below the 1% gate, so no change was made. A
  `tns::autocorr_quarter_lags` lag-outer transpose was exactly equivalent
  and reduced code/branches, but added a second spectral load per term and
  regressed **−1.56%** median (0/4 pairs positive), so it was reverted. No
  change from that investigation is retained. A follow-up optimized-DWARF
  source-line profile established a working dSYM workflow (no production
  change) and identified the next target: the always-left `shift_int` in
  `quant::accumulate_dist` (4.80% corpus attribution), ahead of
  `bitcount::find_max_merge` (3.07%). `BlockSwitch`’s IIR is serial across
  the full frame and is not recommended. That `shift_int` candidate was then
  proven exact and implemented: it produced instruction-identical code
  (LLVM had already emitted the specialized `lsl`) and measured **−0.28%**
  median over four pairs, so it was reverted. The 4.80% line was the
  **second** (genuinely sign-dependent) shift, correcting §21’s call-site
  attribution. `bitcount::find_max_merge` remains the next target. That
  candidate was then implemented as a compacted ordered candidate list
  (24 candidates and ~17 merges per call, ~19 scan steps per merge). It was
  proven equivalent by a 20k-case differential test and removed the
  loop-carried `sections[i].sfb_cnt` walk, but measured **+0.85%** median
  (4/4 pairs positive) — below the 1% gate — so it was reverted. The
  residual cost is merge bookkeeping (`calc_merge_gain` plus an ordered
  `memmove`), not the scan. Next step: resolve the unattributed
  `calc_sfb_dist` `quant.rs:0` region (7.86%) before another optimization.
  That region is now resolved: it is a DWARF line-table gap over 26
  instructions — hoisted quantizer setup, the `q == 0` return and zero
  store, and the `|q|` negation in inlined `inv_quantize_line`. A temporary
  counter build measured 52.5% zero-quantized lines for music, so both
  per-line paths are live; §21's per-PC split was invalid because `sample`
  merges PCs, though the line total reproduces (7.45–8.20%). No removable
  work was found; optimization on this region stops. A follow-up call-path
  audit showed the estimator runs ~10 times per channel-frame (222
  `calc_sfb_dist` calls per pass) and that **13–18% of calls and lines are
  exact intra-pass repeats** of the same `(band, gain)`. A per-band LRU-8
  captures 99% of them; `std::HashMap` lookup (~12–14 ns) would cancel the
  ~69 ns per-call saving, while a per-band scan (~1–2 ns) leaves a
  plausible 2–3% corpus gain. One isolated per-band-cache experiment is
  the supported next step; no code was changed in the audit. That candidate
  was implemented and proven bit-exact (including `MAX_QUANT` prefix
  writes), with measured hit rates of 15–21% and a 15 KB workspace. It
  still regressed **−0.66%** median over four pairs: the per-call lookup
  and storage plus the extra call layer outweighed the 18% hit savings, so
  it was reverted. A follow-up reassessment of every rejected candidate then
  remeasured `find_max_merge` over eight additional pairs: **6/8 positive,
  median +0.594%**, with every heavy workload positive — 10/12 positive
  pairs across both sessions. It was **retained** under a new prospective
  "small but consistent" tier (median ≥0.4%, ≥70% of ≥8 pairs positive, no
  heavy regression), while the neutral or negative candidates (bit-count
  loops, shift specialization, LRU cache, unroll, TNS transpose) remain
  rejected. The second Tier 2 candidate, scalefactor-search trial-array
  reuse, was then re-tested on that tree: 6/8 pairs improved and
  `estimate_channel` shrank 184 B, but the median (**+0.246%**) missed the
  +0.4% Tier 2 bar, so it was reverted. An aggregate checkpoint then measured
  the complete retained set against a rebuilt pre-optimization baseline:
  **+13.932%** median over eight alternating pairs, 8/8 positive, all 20
  workloads improved (heavy workloads +14.265% median), and all 60 output
  hashes unchanged.

## Archive

[archive/](archive/) holds dated plans, implementation reports, and
superseded audits: the L3 loudness evidence trail, the M9 parity/design/plan
series, S1.1/S1.2 cover-art predecessors, the M1 checklist, the M8.5
readiness audit, and the Ferrite phase-1 and modernization reports.
Historical record, not current guidance.

Other top-level `aac-enc-*` milestone, divergence, and benchmark reports —
excluding the three maintained encoder documents listed above — are
historical evidence, not normative guidance. `research/d021/` is a
read-only divergence evidence archive (see `research/d021/README.md`).

For repository layout and validation workflow, see the root
[`README.md`](../README.md).
