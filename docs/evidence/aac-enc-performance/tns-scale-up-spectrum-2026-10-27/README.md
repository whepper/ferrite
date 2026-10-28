# `tns::scale_up_spectrum` write-loop experiment — 2026-10-27

Follow-up to
[`docs/aac-enc-performance-baseline.md`](../../../aac-enc-performance-baseline.md)
§13 and the front-end profiling pass in
[`../front-end-2026-10-27/`](../front-end-2026-10-27/README.md).

## Change

Only the write loop in `tns::scale_up_spectrum` changed:

```rust
// before
for (d, s) in dest.iter_mut().zip(src.iter()).take(stop).skip(start) {
    *d = (*s as i64) << scale;
}

// after
let end = stop.min(dest.len()).min(src.len());
let start = start.min(end);
for (d, s) in dest[start..end].iter_mut().zip(&src[start..end]) {
    *d = (*s as i64) << scale;
}
```

The peak scan, `i64 << scale`, write order, TNS analysis, tables, thresholds,
coefficients, and state updates are unchanged. Exact diff:
[`source-diff.txt`](source-diff.txt).

## Range equivalence

The old adapter form processed `start..min(stop, dest.len(), src.len())`;
`skip(start)` after the shorter iterator simply produced an empty tail when
`start` exceeded that end. The new clamped range is exactly that set for every
range, including out-of-range values:

- `end = min(stop, dest.len(), src.len())`;
- `start = min(start, end)`.

The clamp preserves the old non-panicking behavior for out-of-range inputs
instead of tightening the private function's contract. All supported callers
are in `merged_autocorr_fixed`: `pspec` is 1024 i64; `spectrum_q31` is 1024
i64 for long windows or 128 for short windows; `start/stop` are
`start_line[LO]..stop_line` quarters, with `stop_line` from the active SFB
offset and `idx0 <= idx1 <= idx2 <= idx3 <= idx4`. Valid ranges are therefore
strictly inside both slices.

## Assembly

| Build | Standalone `scale_up_spectrum` | `merged_autocorr_fixed` | Observation |
|---|---|---|---|
| Before | 188 instructions, 0 bounds panics | 0x1164 / 1,113 instructions | write loop scalar `ldrsw/lsl/str` |
| After | no standalone symbol (inlined) | 0x1f70 / 2,012 instructions | vector `sshll.2d`/`ushl.2d` + `stp q` stores |

Before, the write loop was a scalar load/shift/store loop. After, the
production path (`merged_autocorr_fixed`, where the four calls are inlined)
contains 20 `sshll.2d`, 20 `sshll2.2d`, 56 `ushl.2d`, and 16 `abs.4s`
instructions, with vector stores. The intended codegen change is confirmed.

The inlining approximately doubles the merged symbol size (4,452 → 8,048
bytes). This is a real code-size risk, but the paired end-to-end results below
remain positive.

## Paired results

Alternating old/new runs, 15 measured reps per fixture, all 19 fixtures plus
the derived 40 s workload. Both arms used the same release binary/profile and
the same benchmark harness.

### Test build

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 445.217 ms | 450.765 ms | −1.25% |
| 2 | new → old | 455.471 ms | 446.502 ms | +1.97% |
| 3 | old → new | 456.800 ms | 451.879 ms | +1.08% |
| 4 | new → old | 469.850 ms | 462.750 ms | +1.51% |
| 5 | old → new | 486.687 ms | 467.082 ms | +4.03% |
| 6 | new → old | 483.853 ms | 474.567 ms | +1.92% |
| 7 | old → new | 477.970 ms | 463.590 ms | +3.01% |
| 8 | new → old | 471.164 ms | 461.979 ms | +1.95% |
| **Median** | | | | **+1.934%** |

7 of 8 pairs improved; pair 1 is the only negative. The paired median is the
acceptance statistic; it exceeds 1%.

### Non-`cfg(test)` production example

Five workloads (four committed WAVs + 40 s derived music), sum of per-fixture
medians, 15 reps.

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 441.467 ms | 431.631 ms | +2.23% |
| 2 | new → old | 442.328 ms | 434.914 ms | +1.68% |
| 3 | old → new | 440.412 ms | 436.070 ms | +0.99% |
| 4 | new → old | 447.676 ms | 478.328 ms | −6.85%* |
| 5 | old → new | 445.120 ms | 432.548 ms | +2.82% |
| 6 | new → old | 440.816 ms | 429.555 ms | +2.55% |
| 7 | old → new | 436.620 ms | 429.802 ms | +1.56% |
| 8 | new → old | 472.982 ms | 442.576 ms | +6.43% |
| **Median** | | | | **+1.952%** |

\*Pair 4 was a machine-wide slow run: every fixture in `production-after-4.log`
is roughly 10% slower than the same fixture in the adjacent after runs
(music 67.1 vs 60.1/60.2 ms, long 264.4 vs 243.7/239.6 ms), consistent with a
thermal/scheduler event rather than a code path. It is retained in the table.

Per-fixture production-example medians across all eight runs:

| Fixture | Before | After | Improvement | Hash |
|---|---:|---:|---:|---|
| `m7-music-stereo-48k` | 61.281 ms | 60.121 ms | 1.89% | unchanged |
| `m7-speech-clean-stereo-48k` | 39.493 ms | 38.398 ms | 2.77% | unchanged |
| `m7-speech-pauses-stereo-48k` | 39.310 ms | 38.234 ms | 2.74% | unchanged |
| `m8-music-eroica-stereo-48k` | 56.875 ms | 55.761 ms | 1.96% | unchanged |
| `m7-music-stereo-48k-x4` | 245.131 ms | 240.681 ms | 1.82% | unchanged |

## Post-change profile

`sample` profiles before/after (same benchmark harness, 1 ms interval):

| Workload | TNS self before | TNS self after | Front-end before | Front-end after |
|---|---:|---:|---:|---:|
| corpus | 6.66% | 4.56% | 26.70% | 25.66% |
| onset | 10.92% | 6.93% | 51.25% | 47.78% |
| music | 5.03% | 3.39% | 21.50% | 19.87% |
| speech | 7.50% | 5.70% | 29.57% | 28.34% |

The TNS self share fell by ~2.1 points on the corpus and ~4.0 points on onset;
no new front-end hotspot appeared. `scale_up_spectrum` no longer has a
standalone symbol because it was inlined into `merged_autocorr_fixed`; TNS
cost is measured through that symbol/category. These profile shifts are
directional and sampling-noisy; the paired benchmark is primary.

## Correctness

- TNS tests: **5 passed**.
- Block-switch tests: **8 passed**.
- Psy tests: **14 passed**.
- Quant tests: **10 passed**.
- Production-baseline test: **1 passed**.
- Full release suite with reference artifacts: **336 passed, 0 failed, 5
  ignored**.
- Full debug suite with reference artifacts: **340 passed, 0 failed, 1
  ignored**.
- Release without artifacts: **326 passed, 10 documented failures, 5 ignored**.
- Doctests: **19 passed**.
- Licensing: pass; 67 `rust/src` files carry the notice.
- All 19 fixture hashes and the derived long hash unchanged in the paired
  benchmark; all five production-example hashes unchanged.

## Binaries

| Binary | SHA-256 |
|---|---|
| release test, before | `c58116db0d877e23a42f7a2246202bd572fa735f75ffc912b3e3c4a7f170bfca` |
| release test, after | `1016eeabef40d7da86d8a86c6588fac7c4e882096f78500d60549ed64b885b4e` |
| production example, before | `033c27341325f8f46b345fd174c6f95f9a33f6f9cd2a93b25d21b0d4b1374a18` |
| production example, after | `7d39075affccc648514bf9af7adc63c49532e9430a0549679002ada8b824130c` |

## Disposition

**Retained.** The paired medians are 1.934% (test build) and 1.952%
(production example), 7/8 pairs are positive in both arms, all hashes and
correctness checks pass, and the intended vector codegen is present. The
code-size increase/inlining and the one slow outlier are recorded as
limitations. No production test expectation or reference artifact changed.

## Files

- `source-diff.txt` — sanitized source diff.
- `assembly-summary.tsv`, `asm-scale-up-before.txt`, `asm-merged-before.txt`,
  `asm-merged-after.txt` — before/after codegen.
- `testbuild-before-*.log`, `testbuild-after-*.log` — paired test-build logs.
- `production-before-*.log`, `production-after-*.log` — paired production-example logs.
- `optimization-testbuild-ab.tsv`, `optimization-production-ab.tsv`,
  `optimization-production-fixture.tsv` — summary tables.
- `baseline-before.log` — current pre-change baseline.
- `hash-verification.log` — non-quiet 1-rep run with all 20 fixture hashes
  (19 committed + derived long; production/feed/core rows).
- `profile-summary.tsv`, `*-before.sample.txt`, `*-after.sample.txt` — profile comparison.
- `*-after-bench.log` — profile-loop logs.

No C oracle, reference artifact, fixture regeneration, or perceptual test was
used. **No staging, commit, push, amend, reset, rebase, cherry-pick, or history
rewrite occurred.**
