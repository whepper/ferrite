# `calc_sfb_dist` per-band LRU-8 cache — 2026-10-28

Question: does caching repeated `calc_sfb_dist` results within one S4 pass
give a reproducible end-to-end gain? **Correct, but rejected:** the cache is
bit-exact and hits 15–21% of calls, yet four paired runs show a **−0.66%
median** regression (0/4 positive). It was reverted; no production change
remains.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM; macOS 27.0.1;
  rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index; retained
  `bitenc.rs`/`quant.rs`/`tns.rs` diffs untouched; `bitcount.rs` and
  `scf.rs` clean.
- Production binary `61126376…`, 10,930,480 B; restored exactly after the
  revert.

## Design

- **Scope:** one `estimate_channel_with_inv_ws` call. `DistCache::reset(cnt)`
  clears entries and the storage pool at the start of every pass, so no
  entry can outlive its spectrum.
- **Key:** `(band index, gain, dzone, slice length)` inside the band's
  8-slot LRU. `dzone` is always false on this path but is still compared.
- **Value:** the returned distance plus the exact prefix that
  `calc_sfb_dist` wrote to `quant`.
- **Prefix write:** `quant.rs` gained a private
  `calc_sfb_dist_written() -> (i32, usize)`; the public `calc_sfb_dist`
  remains a thin wrapper. Arithmetic is unchanged; the count is the full
  band length on the normal path and the early-out prefix length otherwise.
- **Hit replay:** copy exactly `len` cached `i16`s into the destination and
  leave the tail untouched, matching the early-out behaviour for callers
  that do not pre-zero (`improve_scf`).
- **Miss:** call `calc_sfb_dist_written`, append the written prefix to a
  reusable `Vec<i16>` pool, store the entry.
- **Eviction:** least-recently-used slot (u32 stamp, refreshed on hit).

Call sites converted (the six §25 groups): `improve_scf` (4 sites),
`assimilate_single_scf`, `assimilate_multiple_scf`,
`assimilate_multiple_scf2` steps 1–2, and the clamp.

## Correctness

- Key/lifetime proof: within a pass the spectrum is immutable (cloned at
  `scf.rs:1169`; the only `fill(0)` is the post-search dead-band zeroing),
  each band has a fixed slice, and only `gain` varies. `calc_sfb_dist` reads
  only its arguments and constants and writes only the prefix, so the key
  fully determines both outputs.
- Early-outs are reachable (negative scalefactors): pinned case
  `gain = -81`, `spec = [867603899, 1400102989, 1960503771]` writes one
  line and leaves the rest untouched.
- New tests (all passing): hit replay, early-out tail preservation, LRU
  eviction after eight gains, reset/empty band, 50k randomized
  cache-vs-direct cases with fixed per-band spectra, and a **full-pass
  cache-on vs cache-off differential across every committed S4 corpus
  case and checkpoint** (`scf`, `gain`, `quant_spec`, `mutated_spectrum`).
- The pre-existing S4 oracle tests (`s4_corpus_exact`, `s4_unit_probe_exact`)
  also run with the cache enabled and pass.
- All 19 fixture hashes and the long hash `889c3be4ccd53dd7` pass on the
  candidate tree ([`hash-verification-candidate.log`](hash-verification-candidate.log)).

## Hit rate and footprint (measured)

Temporary counter build (removed; patch
[`hit-rate-instrumentation.patch`](hit-rate-instrumentation.patch)):

| Workload | Lookups | Hits | Hit rate |
|---|---:|---:|---:|
| music-stereo | 209,522 | 38,140 | 18.2% |
| speech-clean | 108,932 | 16,780 | 15.4% |
| onset | 3,324 | 651 | 19.6% |
| sine | 6,221 | 1,264 | 20.3% |
| noise | 4,082 | 845 | 20.7% |

These exactly reproduce the §25 LRU-8 simulation (38,140 / 16,780 / 651 /
1,264 / 845), confirming the implementation captures all modelled repeats.

Footprint for a 45-band pass: 7,200 B entries (45 × 8 × 20 B) + 8,192 B pool
capacity = **15,392 B** per workspace ([`cache-footprint.log`](cache-footprint.log));
a 49-band long-block pass adds ~800 B of entries. No per-lookup allocation.

## Performance — gate not met

Baseline (old binary): 414.613 / 417.536 / 417.280 ms, median **417.280 ms**.

| Pair | Order | Before | After | Improvement |
|---|---|---:|---:|---:|
| 1 | old → new | 411.529 ms | 415.330 ms | **−0.924%** |
| 2 | new → old | 412.817 ms | 413.035 ms | **−0.053%** |
| 3 | old → new | 411.828 ms | 413.463 ms | **−0.397%** |
| 4 | new → old | 411.638 ms | 416.130 ms | **−1.091%** |
| **Median** | | | | **−0.660%** |

0/4 positive. Per-workload: music −0.511%, long −0.397%, speech −1.661% /
−1.865%, Eroica +0.270%, stress −0.761%; sub-3 ms fixtures −0.1% to −6.1%.
Full table: [`paired-workloads.tsv`](paired-workloads.tsv).

A cache-bypassed build of the same refactor (`no_cache` forced in `reset`)
measured 424.6 / 427.7 ms against 410.0–419.0 ms for baseline and
416.7–419.6 ms for cache-on in an ad-hoc sequence: the extra call layer and
parameter threading cost about as much as the cache saves, and the cache
only partly recovers it. That diagnostic is noisy and not a formal paired
gate; it is reported only to explain the direction.

## Decision

**Rejected and reverted.** The candidate is correct and hits the modelled
rate, but it is slower: the per-call lookup and storage overhead plus the
extra call layer exceed the 18% hit savings on this workload. `quant.rs` and
`scf.rs` were restored byte-for-byte (`quant.rs` diff SHA-256 `ba6778bf…`,
`scf.rs` no diff), and the production binary rebuilt to its original
SHA-256.

## Final validation

- Production baseline **1 passed**; 60-row hashes unchanged (long
  `889c3be4ccd53dd7`).
- Full suite with artifacts **336 passed, 0 failed, 5 ignored**; without
  artifacts **326 passed, 10 documented artifact-dependent failures,
  5 ignored**.
- Licensing hygiene OK; `git diff --check` clean; HEAD/index unchanged.

## Limitations

- One machine, one session; the machine warmed during the run, so paired
  deltas are the meaningful signal.
- The measured binary is a `cfg(test)` build and includes the test-only
  bypass branch and new tests (~22 KB of text), which slightly changes
  layout versus the baseline binary.
- The cache key relies on the pass-level spectrum immutability guarantee;
  the tests document and enforce it, but a future caller that mutates the
  spectrum mid-pass would invalidate the cache.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state and toolchain. |
| `candidate-attempted.patch` | Reverted candidate (quant + scf). |
| `hit-rate-instrumentation.patch`, `cache-hit-rate.log` | Temporary counters and results. |
| `cache-footprint.log` | Measured cache capacity. |
| `baseline-{1,2,3}.log`, `pair-{1..4}-{old,new}.log`, `paired-summary.tsv`, `paired-workloads.tsv` | Benchmarks. |
| `diag-*.log`, `bypass-run.log` | Ad-hoc cache-on/off/bypass diagnostics. |
| `hash-verification-candidate.log`, `final-*.log` | Candidate and final-tree validation. |
