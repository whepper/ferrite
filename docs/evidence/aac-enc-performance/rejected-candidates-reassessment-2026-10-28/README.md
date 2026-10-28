# Rejected-candidate reassessment — 2026-10-28

Question: do any previously rejected performance experiments deserve to be
retained under a clearer decision framework? **Yes — one:** the
`bitcount::find_max_merge` ordered-candidate rewrite. A fresh eight-pair
remeasurement gives **6/8 positive, median +0.594%**, with every heavy
workload positive (music +0.86%, long +0.78%, Eroica +1.45%). Combined with
§23 this is 10/12 positive pairs. The audit itself changed no production
code.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM; macOS 27.0.1;
  rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index; retained
  `bitenc.rs`/`quant.rs`/`tns.rs` diffs untouched (379/55); `bitcount.rs`
  and `scf.rs` clean. Production binary `61126376…` (10,930,480 B).

## Inventory

Full table: [`inventory.tsv`](inventory.tsv). Summary:

| Candidate | Exact? | Original paired | Codegen | Reconsider? |
|---|---|---|---|---|
| `find_max_merge` ordered candidates (§23) | 20k differential + mutations + hashes | 4/4, **+0.846%**, 1/4 ≥1% | scan 12→7 insns/step, `dyn_bit_count_reuse` −612 B | **Yes** |
| SCF trial-array reuse (§16) | argument + 6 tests + hashes | 3/4, +0.602%, 0/4 ≥1% | −0xB8 function size | Possibly (separate remeasure) |
| `bit_count` `chunks_exact` (§14) | 11 tests + hashes | 1/4, +0.029% | 881→700 insns | No (neutral) |
| `accumulate_dist` shift (§22) | proof + identical codegen | 0/4, −0.279% | none | No (no mechanism) |
| LRU-8 dist cache (§26) | differential + full-pass + hashes | 0/4, −0.660% | +22.5 KB text, 15 KB state | No (net regression) |
| Energy/distortion unroll (§18) | associativity + tests | 1/4, −0.411% | 512→1164 B | No (regression) |
| TNS autocorr transpose (§20) | term-order proof + tests | 0/4, −1.563% | 1212→764 B but 2 loads/term | No (regression) |

Historical verdicts and measurements are preserved as recorded; nothing is
reclassified retroactively.

## Proposed decision framework (prospective)

The old 1% gate remains the standard. It is joined by an explicit tier for
small but consistent wins, so such results are decided by evidence rather
than by waiving the rule case by case.

- **Tier 1 — standard win (retain):** median ≥ +1.0%, ≥3/4 pairs positive,
  ≥2/4 ≥1%, no heavy-workload regression, hashes + full suite pass.
- **Tier 2 — small but consistent (retain with explicit note):** median
  ≥ +0.4% over **≥8 alternating pairs**, ≥70% of pairs positive, every
  workload >10 ms non-negative within noise, no heavy workload regressing
  more than 0.5%, and a small, differential-tested change (roughly ≤50
  production lines).
- **Tier 3 — neutral for maintainability (retain only if clearly better):**
  median within ±0.2% and the change measurably reduces code size,
  complexity, or a hazard; otherwise reject.
- **Tier 4 — reject:** negative median, inconsistent signs, or a
  heavy-workload regression outside noise.

Tier 2 is prospective only. It does not retroactively overturn §14–§26; it
applies to new measurements taken under its conditions.

## `find_max_merge` remeasurement

**Candidate:** the exact §23 patch
([`candidate-attempted.patch`](../find-max-merge-incremental-maximum-2026-10-28/candidate-attempted.patch)),
re-applied to the current tree; the rebuild reproduced the original
candidate binary SHA-256
`31a0269a037742bc3be4ac12664f928b59bcfbb8d04f6a68cfd04a83e211e7ea`.

**Correctness re-confirmed on the candidate tree:** 13 bitcount tests
(incl. the 20k-case differential and mutation checks), production baseline,
60 hashes, long hash `889c3be4ccd53dd7`.

**Method:** 8 alternating old/new pairs, 15 reps, 19 fixtures + 40 s
workload; old = production binary `61126376…`, new = candidate; all runs
recorded, none discarded.

| Pair | Order | Old | New | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 402.290 | 405.770 | −0.865% |
| 2 | new → old | 411.466 | 406.890 | +1.112% |
| 3 | old → new | 413.452 | 411.427 | +0.490% |
| 4 | new → old | 424.548 | 411.128 | +3.161% |
| 5 | old → new | 411.031 | 418.504 | −1.818% |
| 6 | new → old | 414.179 | 405.740 | +2.038% |
| 7 | old → new | 409.114 | 406.257 | +0.698% |
| 8 | new → old | 415.001 | 414.185 | +0.196% |
| **Median** | | | | **+0.594%** |

6/8 positive, 3/8 ≥1%. Heavy workloads (>10 ms median): music +0.857%,
long +0.783%, Eroica +1.448%, speech-clean +0.153%, speech-pauses +0.128%
— all non-negative. Combined with §23: **10/12 pairs positive (83%)**, and
the heavy-workload direction is consistent across both sessions.

**Limitations:** short (<3 ms) fixtures dominate the variance (pair 5's
−1.8% is one such swing); both binaries are `cfg(test)` builds with
different test-code layout (the candidate adds ~200 test lines); one machine
and two sessions.

**Assessment under the proposed framework:** Tier 2. Median +0.594% over 8
pairs, 75% positive, all heavy workloads positive, ~30 production lines,
differential-tested. The additional complexity is a documented invariant
(ordered candidate list) with a verbatim reference test retained next to it.

## Recommendation

1. **Retain `find_max_merge` in a follow-up change** by re-applying the
   existing patch, then run the full suite with artifacts and re-verify the
   60 hashes on the retained tree. This audit does not apply it (production
   code must remain unchanged here).
2. **Run one separate 8-pair experiment for the §16 trial-array reuse**
   under Tier 2 before deciding: it was 3/4 positive at +0.602% and also
   shrank the function. It must not be bundled with `find_max_merge`.
3. **Do not pursue** `bit_count` `chunks_exact` (neutral, wider edit), the
   `accumulate_dist` shift (no code change), the LRU-8 cache (net
   regression), the unroll (regression), or the TNS transpose (regression).

## Validation

Production code unchanged: the candidate patch was applied only for the
remeasurement and reversed; `bitcount.rs` has no diff, `scf.rs` has none,
HEAD and the index are unchanged, and the retained diffs remain 379/55.
The rebuilt production binary is byte-identical to the starting one
(`61126376…`). Production baseline **1 passed**; 60-row hashes unchanged
(long `889c3be4ccd53dd7`); `git diff --check` clean.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state and toolchain. |
| `inventory.tsv` | Seven rejected candidates with evidence and disposition. |
| `candidate-bitcount-tests.log`, `candidate-production-baseline.log`, `candidate-hash-verification.log` | Candidate-tree correctness. |
| `pair-{1..8}-{old,new}.log`, `paired-summary.tsv`, `paired-workloads.tsv` | Remeasurement. |
| `final-production-baseline.log`, `final-hash-verification.log` | Final-tree validation. |
