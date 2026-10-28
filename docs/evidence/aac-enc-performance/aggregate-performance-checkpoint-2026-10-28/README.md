# Aggregate performance checkpoint — current retained tree vs pre-optimization baseline — 2026-10-28

Question: what is the combined end-to-end effect of the five retained encoder
optimizations, measured against the pre-optimization source on the current
machine/toolchain? **+13.93% median over eight alternating pairs**, with 8/8
pairs positive, all 20 workloads positive, and every output hash unchanged.
The comparison is valid: both arms were rebuilt from source in the same
isolated worktree with the same compiler, profile, features, and harness.

## 1. Baseline definition and reconstruction

HEAD is `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; the index was empty. The
retained optimizations are uncommitted worktree changes, so the baseline was
reconstructed in a detached worktree at HEAD, then the current non-performance
changes were copied in. The four retained performance patches were kept out of
the baseline:

| File | Baseline source | Retained optimization(s) removed |
|---|---|---|
| `rust/src/aac_enc/quant.rs` | HEAD | §11 `calc_sfb_dist` bounds-check cleanup; §17 `quantize_line_body` shared magnitude path and `inv_quantize_line` mask |
| `rust/src/aac_enc/bitenc.rs` | HEAD | §15 `BitWriter::write_bits` 8-byte window |
| `rust/src/aac_enc/bitcount.rs` | HEAD | §23/§27 ordered-candidate `find_max_merge` and its tests |
| `rust/src/aac_enc/tns.rs` | current file minus the §13 hunk | §13 `scale_up_spectrum` direct-slice loop only; the later TNS test additions remain |

The non-performance worktree changes are present in both arms: the
`aac_enc.rs` test-module registration, the `block_switch.rs`/`psy.rs` test
additions, and the `stream_driver.rs` test-only S1/S2 captures plus benchmark
harness. A recursive source diff between the two built trees is clean.

Both arms were built in the same detached worktree with the same target
directory:

```sh
cargo test --release --locked --features aac_enc --lib --no-run
```

Baseline binary: `54eb92771609c1d85e40eeaf0dd23ee200ae61bc22935840b951acad45deae8e`.
Current isolated rebuild:
`261b747a8e289f960e3bfc596b518ac8a3401451eec979c406ecd113faeb55dd`.
The repository build is
`31a0269a037742bc3be4ac12664f928b59bcfbb8d04f6a68cfd04a83e211e7ea`.
The isolated rebuild has identical source and `__text` size to the repository
build; the hash differs only because the test binary embeds its absolute
source path, shifting constant addresses. The retained-optimization functions
(`gm_stage2`, `estimate_channel_with_inv_ws`) have identical sizes; 2,893 of
2,903 text symbols match exactly, and 10 test/auxiliary symbols (plus the
encoder helper `dyn_bit_count`, 8 B) differ by 4–8 bytes from path-dependent
constant materialization. The benchmark deliberately uses the two same-path
binaries (`54eb9277…` and `261b747a…`).

Code size: baseline 10,926,960 B (`__text` 1,831,036 B); current 10,936,768 B
(`__text` 1,837,692 B); +9,808 B total / +6,656 B text. Diagnostic only.

A two-pair build-path control (repository build vs isolated current rebuild)
measured isolated-vs-repository **+1.168%** and **−1.335%** (median −0.084%),
i.e. no systematic difference beyond the ±1.3% run noise. The checkpoint
therefore reflects the repository production binary.

## 2. Method

- 19 committed fixtures plus the derived 40 s `m7-music-stereo-48k-x4`
  concatenation; 91.45 s of audio; 15 synthetic fixtures and four committed
  WAVs.
- Production `Encoder` API: AAC-LC VBR-5, ADTS, 1024-sample frames,
  mono/stereo, 44.1/48 kHz, 8,000-sample chunks per channel.
- One untimed warm-up pass per process; 15 measured repetitions per fixture
  and mode; a fresh encoder per repetition.
- Three standalone baseline runs before the paired series, eight alternating
  old/new pairs, then two post-series baseline runs to check drift.
- The headline is the harness `AACPERF_AGG production` value: the median of
  complete-corpus per-repetition sums. Pair improvement is
  `(old - new) / old × 100`.
- Timing is `Encoder::new` (reported separately) plus `push`/`flush`. Fixture
  synthesis/decoding, interleaving, ADTS validation, hashing, and disk I/O are
  outside; copying emitted bytes into a preallocated output buffer is inside.
- Every benchmark run validates the committed byte totals and FNV-1a hashes.
  Separate `REPS=1` runs produced 60-row hash logs for baseline and current.

## 3. Aggregate results

### 3.1 Paired aggregates

| Pair | Order | Old (ms) | New (ms) | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 481.343 | 414.401 | **+13.907%** |
| 2 | new → old | 484.223 | 414.398 | **+14.420%** |
| 3 | old → new | 485.920 | 417.086 | **+14.166%** |
| 4 | new → old | 480.635 | 416.092 | **+13.429%** |
| 5 | old → new | 481.039 | 416.439 | **+13.429%** |
| 6 | new → old | 481.882 | 413.769 | **+14.135%** |
| 7 | old → new | 480.663 | 413.581 | **+13.956%** |
| 8 | new → old | 479.291 | 420.122 | **+12.345%** |
| **Median** | | **481.191** | **415.247** | **+13.932%** |

8/8 pairs positive; pair improvements 12.345–14.420%, interquartile range
13.429–14.151%. Old-arm runs span 479.291–485.920 ms, new-arm runs
413.581–420.122 ms. The 13.932% headline is the median of the eight pair
improvements; the ratio of the two arm medians is 13.70%.

### 3.2 Standalone baseline drift check

| Run | When | Production total | Realtime × | RTF |
|---|---|---:|---:|---:|
| baseline 1 | before pairs | 474.308 ms | 192.8× | 0.005187 |
| baseline 2 | before pairs | 480.119 ms | 190.5× | 0.005250 |
| baseline 3 | before pairs | 482.067 ms | 189.7× | 0.005271 |
| post-baseline 1 | after pairs | 482.161 ms | 189.7× | 0.005272 |
| post-baseline 2 | after pairs | 481.947 ms | 189.8× | 0.005270 |

Five-run median **481.947 ms** (190.0×, RTF 0.005262); range 474.308–482.161
ms (1.66%). The initial run was the coolest; the later runs stabilized around
482 ms. Pairing and alternating order are therefore the primary estimate.

The original §4 four-run baseline was 468.2–481.9 ms (median 470.6 ms). The
fresh reconstructed baseline median (481.9 ms) lies inside that historical
range and is 2.4% slower than the historical median, consistent with the
documented session drift. The fresh paired baseline is used for the headline;
the historical figure is a cross-check, not a second measurement of the same
session. A naive cross-session comparison of the current 415.247 ms to the
historical 470.6 ms would give 11.8%, but it mixes sessions; the paired
13.93% is the checkpoint estimate.

At the paired medians, the current encoder is **415.247 ms / 220.2× / RTF
0.004541** versus **481.191 ms / 190.0× / RTF 0.005262** for the baseline.

### 3.3 Per-workload results

| Workload | Old median (ms) | New median (ms) | Improvement |
|---|---:|---:|---:|
| `m2-click-mono-48k` | 0.990 | 0.911 | **+7.983%** |
| `m2-edge-mono-48k` | 0.216 | 0.199 | **+7.622%** |
| `m2-impulse-mono-48k` | 0.184 | 0.172 | **+6.868%** |
| `m2-onset-stereo-48k` | 1.957 | 1.852 | **+5.361%** |
| `m2-silence-stereo-48k` | 1.213 | 1.130 | **+6.854%** |
| `m2-sine-stereo-48k` | 2.615 | 2.494 | **+4.621%** |
| `m2-stepdown-stereo-48k` | 1.933 | 1.823 | **+5.694%** |
| `m3-multitone-stereo-48k` | 2.988 | 2.819 | **+5.635%** |
| `m3-noise-mono-48k` | 1.373 | 1.159 | **+15.557%** |
| `m3-onset-stereo-441k` | 1.961 | 1.814 | **+7.478%** |
| `m3-quiet-sine-stereo-48k` | 2.757 | 2.515 | **+8.798%** |
| `m3-sine-stereo-441k` | 2.726 | 2.536 | **+6.949%** |
| `m3-sweep-stereo-48k` | 2.744 | 2.580 | **+5.964%** |
| `m3-tonal-noise-stereo-48k` | 1.908 | 1.667 | **+12.626%** |
| `m6-stress-stereo-48k` | 1.992 | 1.731 | **+13.078%** |
| `m7-music-stereo-48k` | 62.923 | 53.800 | **+14.499%** |
| `m7-music-stereo-48k-x4` | 250.954 | 215.155 | **+14.265%** |
| `m7-speech-clean-stereo-48k` | 40.401 | 35.415 | **+12.341%** |
| `m7-speech-pauses-stereo-48k` | 40.342 | 35.466 | **+12.087%** |
| `m8-music-eroica-stereo-48k` | 58.701 | 49.664 | **+15.394%** |

All 20 workloads improved. The five heavy workloads (>10 ms) have a median
gain of **+14.265%** (music +14.499%, long +14.265%, speech
+12.341%/+12.087%, Eroica +15.394%). The 15 short fixtures have a median gain
of **+6.949%**; their absolute times are 0.2–3.0 ms, so their pair-to-pair
ranges are wider and they are not used as standalone evidence of a meaningful
gain. Full per-pair ranges are in `paired-workloads.tsv`.

### 3.4 Interaction check

The arithmetic sum of the individually measured medians is
2.82 + 1.93 + 2.10 + 7.38 + 0.594 ≈ **14.82%**, versus the measured combined
**13.93%**. The difference is consistent with interaction, code-layout, and
session effects; the individual percentages are not additive and were not
summed to produce the checkpoint.

## 4. Correctness and hashes

| Check | Baseline | Current |
|---|---|---|
| Production baseline test | **1 passed** | **1 passed** |
| Focused tests | `quant` 10, `bitenc` 9, `bitcount` 11, `tns` 5 | §28 retained tree: full suite 338 passed |
| 60 benchmark hash rows | 60/60 match committed bytes/hash | 60/60 match |
| Long-workload hash | `889c3be4ccd53dd7` | `889c3be4ccd53dd7` |
| Explicit baseline/current row comparison | **60/60 identical** (`hash-comparison.tsv`) | same |

The baseline `bitcount` count is 11 because the retained candidate's two
differential tests are absent there; the current tree has 13 `bitcount` tests.
Matching hashes demonstrate bit-exact output on this finite corpus; they do not
establish universal perceptual equivalence.

## 5. Decision, limitations, and recommendation

**Verdict: valid paired comparison.** The baseline is not the original binary
from the pre-optimization session, but it is a deterministic rebuild of the
same source state (HEAD plus the non-performance worktree changes), and both
arms were built and measured under the same conditions. The fresh baseline
median also falls inside the historical baseline range.

Limitations:

- One machine, one toolchain, one session; single-threaded process with no CPU
  pinning, so P/E-core placement and thermal drift remain the largest
  variability source. Pairing controls the drift but does not eliminate it.
- Test-build harness (`cfg(test)`) as in the established benchmark; the
  documented production-example bias is 0.25–0.86% in the same direction, so
  these numbers are slightly conservative.
- Short fixtures are sub-3 ms and dominated by fixed overhead; their
  percentages are recorded but not treated as the primary result.
- Binary hashes are build-path-dependent for the test binary; source and text
  symbol sizes, not SHA-256 alone, establish current-build identity.
- The baseline includes later non-performance test/instrumentation changes so
  that both arms share the current harness and corpus; no production behavior
  differs between the arms other than the five retained optimizations.
- The exact measured test-only harness, including the shared S1/S2 captures,
  is preserved under [`harness/`](harness/) rather than committed as production
  source because it is entangled with unrelated quality-test work. Applying
  that snapshot reproduces the measured test build; see
  [`harness/README.md`](harness/README.md).

**Recommendation: stop incremental performance work for now.** The retained
set delivers a large, consistent end-to-end gain with bit-exact output. The
remaining dominant cost is the S4 scalefactor/quantization search, but the
recent local candidates there and in the bit path were neutral or below the
pre-registered gates. Further work should require a new structural target and
the existing Tier 1/Tier 2 evidence rules, not another micro-optimization
sweep. Use this checkpoint as the regression reference.

## 6. Files

| File | Contents |
|---|---|
| `starting-state.txt`, `git-status-start.txt` | HEAD, branch, status, retained and non-performance diffs, environment. |
| `build-config.txt` | Baseline definition, binaries, toolchain, build/benchmark commands, corpus hashes, timing/repetition rules. |
| `perf-quant.patch`, `perf-bitenc.patch`, `perf-bitcount.patch`, `perf-tns.patch` | Exact retained diffs reversed to reconstruct the baseline. |
| `baseline-{1,2,3}.log`, `post-baseline-{1,2}.log` | Standalone baseline runs. |
| `pair-{1..8}-{old,new}.log`, `paired-summary.tsv` | Eight alternating pairs and aggregate summary. |
| `paired-workloads.tsv`, `workloads.tsv` | Per-workload medians, per-pair ranges, corpus metadata. |
| `baseline-hash-verification.log`, `current-rebuild-hash-verification.log`, `hash-comparison.tsv` | 60-row hash verification and explicit comparison. |
| `baseline-production-baseline.log`, `current-rebuild-production-baseline.log`, `baseline-focused-tests.log` | Correctness checks. |
| `current-rebuild-sha.txt`, `current-build-code-identity.txt` | Isolated current binary hash and code-identity diagnostics. |
| `control-pair-{1,2}-{repo,isolated}.log`, `control-summary.tsv` | Repository-vs-isolated-build control. |
| `final-full-suite-with-artifacts.log`, `final-hash-verification.log` | Final repository-tree suite and 60-row hash verification. |
| `final-licensing.txt`, `final-state.txt` | Final licensing check and Git/binary state. |
| `harness/` | Test-only measured harness snapshot: full `stream_driver.rs`, `production_baseline.rs`, module-registration patch, production example, and reproduction instructions. |
| `summary.txt` | Raw summary statistics. |
