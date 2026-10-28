# SCF trial-array reuse — independent Tier 2 experiment — 2026-10-28

Question: does hoisting the 60-element `trial` copy out of the
`assimilate_multiple_scf2` `nw` loop clear the prospective Tier 2 gate on
the current tree (with `find_max_merge` retained)? **No.** Eight paired
runs give **6/8 positive but a +0.246% median**, below the +0.4% Tier 2
threshold. The candidate was reverted; production is unchanged.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM; macOS 27.0.1;
  rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index.
- Baseline tree: retained `find_max_merge` (+248/−35) plus the unchanged
  `bitenc.rs`/`quant.rs`/`tns.rs` diffs (379/55); `scf.rs` clean.
- Baseline binary SHA-256
  `31a0269a037742bc3be4ac12664f928b59bcfbb8d04f6a68cfd04a83e211e7ea`.
- Candidate binary SHA-256
  `8699ecfb8d3dbc95144cca87d9ee6aef36994302bc2fdd5068d198858c4ec52a`.

## Candidate

The exact §16 patch
([`candidate-attempted.patch`](candidate-attempted.patch)), re-applied
unchanged; the resulting `git diff` is byte-identical to the saved patch.
It moves the `[i32; 60]` `trial` array and its baseline copy out of the
`nw` candidate loop; each iteration then rewrites only `start..stop`.

Equivalence: `trial` differs from `scf` only in `start..stop`; accepted
trials write the same region values back to `scf`, rejected trials leave
`scf` unchanged, and entries outside the region are never written and only
read below `cnt`. No arithmetic, ordering, comparison, or control flow
changed. The retained `bitcount.rs` implementation was untouched.

## Correctness

- `aac_enc::scf`: **6 passed**, including the exact S4 oracle tests
  (`s4_corpus_exact`, `s4_unit_probe_exact`).
- `aac_enc::bitcount`: **13 passed** (retained `find_max_merge` untouched).
- Production baseline: **1 passed**.
- All 60 hash rows unchanged; long-workload hash `889c3be4ccd53dd7`.
- Candidate tree was correct; the rejection is purely the performance gate.

## Performance

Baseline (current tree with `find_max_merge`): 409.129 / 410.330 /
411.859 ms, median **410.330 ms**.

| Pair | Order | Old | New | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 413.103 | 412.105 | +0.242% |
| 2 | new → old | 416.253 | 416.571 | −0.076% |
| 3 | old → new | 416.449 | 414.689 | +0.423% |
| 4 | new → old | 414.839 | 413.804 | +0.250% |
| 5 | old → new | 418.037 | 408.274 | +2.336% |
| 6 | new → old | 414.118 | 420.995 | −1.661% |
| 7 | old → new | 416.132 | 411.086 | +1.212% |
| 8 | new → old | 415.050 | 414.509 | +0.130% |
| **Median** | | | | **+0.246%** |

6/8 positive (75%). Heavy workloads (>10 ms) all non-negative: music
+0.425%, long +0.505%, speech-clean +0.246%, speech-pauses +0.755%,
Eroica +0.013%. Short fixtures carry the large swings (pair 5 +2.34%,
pair 6 −1.66%), consistent with sub-3 ms noise.

**Code size:** `estimate_channel_with_inv_ws` 0x3748 (14,152 B) → 0x3690
(13,968 B), −184 B; `__text` −192 B — the expected reduction, treated as a
diagnostic only.

## Decision

**Reverted.** Tier 2 requires median ≥ +0.4%; the measured median is
+0.246%. The candidate is small, exactly equivalent, and reduces redundant
work and code size, and 6/8 pairs improved with no heavy regression — but
the pre-registered magnitude threshold was not met. The criteria were not
adjusted after the result.

The original §16 measurement (+0.602%, 3/4 positive) stands as recorded;
combined with this run the effect is positive but below the Tier 2 bar.

## Final validation (reverted tree)

- Binary rebuilt to the starting SHA-256 `31a0269a…`; `scf.rs` has no diff.
- Retained `bitcount.rs` (+248/−35) and the `bitenc.rs`/`quant.rs`/`tns.rs`
  diffs (379/55) unchanged.
- Production baseline **1 passed**; 60-row hashes unchanged (long
  `889c3be4ccd53dd7`).
- Full release suite with artifacts: **338 passed, 0 failed, 5 ignored**.
- Licensing hygiene OK; `git diff --check` clean.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Hashes and environment. |
| `candidate-attempted.patch` | Exact §16 candidate (reverted). |
| `candidate-scf-tests.log`, `candidate-bitcount-tests.log`, `candidate-production-baseline.log`, `candidate-hash-verification.log` | Candidate-tree correctness. |
| `baseline-{1,2,3}.log`, `pair-{1..8}-{old,new}.log`, `paired-summary.tsv`, `paired-workloads.tsv` | Benchmarks. |
| `codegen-size.txt` | Symbol and `__text` sizes. |
| `final-production-baseline.log`, `final-hash-verification.log`, `final-full-suite-with-artifacts.log` | Final-tree validation. |
