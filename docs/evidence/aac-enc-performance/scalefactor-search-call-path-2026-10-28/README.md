# Scalefactor-search call-path audit — 2026-10-28

Question: are the ~2,229 `calc_sfb_dist` calls per channel-frame redundant,
and is a semantics-preserving caller-level optimization supported? **Yes,
partially:** within one scalefactor-estimator pass, **13–18% of calls and
lines are exact repeats of an earlier `(band, gain)` evaluation**. A
per-band cache of the last few evaluations captures ~99% of them; a global
hash map would cost about as much as it saves. No production code was
changed.

## Environment and starting state

- Apple M5, arm64, 10 logical CPUs, 16 GiB RAM; macOS 27.0.1;
  rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1
- HEAD `c4d1262fa3ec2f4f88cbb27e49d403d6d6c06b2f`; empty index; retained
  `bitenc.rs`/`quant.rs`/`tns.rs` diffs untouched (379/55); `bitcount.rs`
  clean. Full state: [`starting-state.txt`](starting-state.txt).
- Production binary `61126376…`, 10,930,480 B, unchanged.

## Call hierarchy

All calls originate in the S4 scalefactor estimator
(`estimate_channel_with_inv_ws`, `scf.rs:1142`), which runs the initial
formula, then up to three afterburner passes and a clamp:

| Phase | Function | Site | Loop | Counts group |
|---|---|---|---|---|
| 0 | `improve_scf` | `scf.rs:432/444/458/472` | per active band; one initial + ≤2 trials (`UPCOUNT_LIMIT = 1`) | initial |
| 1 | `assimilate_single_scf` | `scf.rs:593` | descending `cur` per band transition | single |
| 2 | `assimilate_multiple_scf` | `scf.rs:735` | per `act` level, per band in region | multiple |
| 3 | `assimilate_multiple_scf2` step 1 | `scf.rs:895` | `nw` in `scf+1..=hi`, per band | scf2 coarse |
| 4 | `assimilate_multiple_scf2` step 2 | `scf.rs:985` | single `lo`, per band | scf2 fine |
| 5 | clamp | `scf.rs:1357` | per band clamped to `gmin+60` | clamp |

`assimilate_multiple_scf2` step 3 (`scf.rs:1040`) uses
`calc_sfb_quant_energy_and_dist` and does not re-quantize, so it is outside
this count. Details: [`call-sites.tsv`](call-sites.tsv).

**Measured per pass** (temporary counters, removed;
[`instrumentation-call-path.patch`](instrumentation-call-path.patch)):

| Workload | S4 passes | Calls | Calls/pass | Lines/call | Zero q | Dup calls | Dup lines |
|---|---:|---:|---:|---:|---:|---:|---:|
| music-stereo (1 s) | 942 | 209,522 | 222.4 | 14.4 | 52.5% | 18.3% | **17.8%** |
| speech-clean (1 s) | 942 | 108,932 | 115.6 | 14.2 | 47.9% | 15.6% | **13.2%** |
| onset (1 s) | 98 | 3,324 | 33.9 | 7.5 | 45.2% | 19.6% | 16.3% |
| sine (1 s) | 98 | 6,221 | 63.5 | 6.7 | 50.4% | 20.3% | 17.2% |
| noise (0.5 s) | 25 | 4,082 | 163.3 | 14.6 | 9.0% | 20.7% | 17.7% |

The estimator runs **~10 times per audio channel-frame** on music and
speech (942 passes for 94 channel-frames), which explains §24's 2,229 calls
per audio channel-frame as 222 per S4 pass × 10 passes.

Phase shares (music): 38.9% initial, 24.3% single, 14.4% multiple, 18.6%
scf2-coarse, 3.8% scf2-fine, 0% clamp. Speech is similar; onset/sine are
dominated by the initial and single passes. Raw: [`call-path-counters.log`](call-path-counters.log),
[`workload-counts.tsv`](workload-counts.tsv).

## Inputs and proven repeats

`calc_sfb_dist` reads only its arguments and module constants and writes
only its `quant` output slice. Within one estimator pass the spectrum is
**immutable** (`spec` is cloned at `scf.rs:1169`; the only `fill(0)` is the
dead-band zeroing at `scf.rs:1384`, after every search phase), so
`(band slice, gain, dzone)` fully determines both outputs. `dzone` is
`false` throughout this path; only `gain` varies.

Counters keyed on `(slice pointer, length, gain, dzone)` show exact repeats
of the same computation. Duplicate phase pairs for music (first → repeat):

```
0->1:4369  0->2:3592  0->3:11928  0->4:1817
1->1: 675  1->2:7067  1->3: 5271  1->4:2279
2->3: 567  2->4: 868
```

Most repeats are cross-phase: a gain tried by `improve_scf` is tried again
by a later assimilation pass. Within-pass repeats (1→1, 675) are small.

**Cache-size simulation** (per band, LRU over distinct gains):

| Workload | LRU-2 | LRU-4 | LRU-8 | all |
|---|---:|---:|---:|---:|
| music | 25,588 | 34,726 | 38,140 | 38,433 |
| speech | 11,236 | 15,112 | 16,780 | 16,978 |
| onset | 366 | 552 | 651 | 652 |
| sine | 673 | 1,056 | 1,264 | 1,264 |
| noise | 623 | 790 | 845 | 845 |

LRU-8 captures 99–100% of repeats; LRU-4 captures 83–94%. Distinct gains
per band per pass are small (music maximum 26, typical ~4; average
distinct/calls = 181.6/222.4).

## Cache scope across S4 passes

The 10 passes per frame mostly see different spectra: consecutive-pass
spectrum-hash equality was 1/942 for music, 0/942 for speech, 2/98 sine,
1/25 noise, and 48/98 onset (silence-heavy). A cache must therefore be
**cleared per S4 pass**; cross-pass reuse is not justified without a
content check.

## Lookup cost

Measured standalone ([`lookup-costs.txt`](lookup-costs.txt),
[`lookup-bench.rs`](lookup-bench.rs)):

- `std::collections::HashMap` entry lookup, 182 entries: **11.6–14.0 ns**
- 512-slot open-addressed table with a cheap hash: **1.3–1.4 ns**

The average `calc_sfb_dist` call is ~310 cycles (~69 ns) on music
(§21: 27.0% of music time, 209,522 calls). A duplicate call saves ~69 ns;
a std hash-map probe at 12–14 ns would consume nearly all of it. A per-band
linear scan (≤8 gains, ~1–2 ns) leaves a real margin.

## Hypothesis

**Per-pass, per-band LRU-8 cache of `(gain → dist, quant)` for
`calc_sfb_dist`.**

1. **Repeated computation:** `calc_sfb_dist(band, gain)` is re-evaluated
   for the same band and gain later in the same pass (13.2–17.8% of lines).
2. **Inputs unchanged:** the band slice is the same memory, the spectrum is
   immutable during the pass, and only `gain` varies. Same
   `(band, gain, dzone)` therefore has identical inputs.
3. **Result equivalence:** the function is pure; caching both the returned
   distance and the exact post-call `quant` bytes reproduces every output,
   including `MAX_QUANT` early-out tails, with no arithmetic changes.
4. **Expected impact:** upper bound is the duplicate share of
   `calc_sfb_dist` time: 17.8% × 27.0% ≈ **4.8%** on music, ~13–16% ×
   21.7% ≈ **3%** corpus-wide. A per-band scan (~1–2 ns) and a band copy on
   hits are small; realistic net **2–4%** on music-like content, **~2–3%**
   corpus. A global hash map should not be attempted (lookup ≈ saving).
5. **Validation:** differential cache-on/off over random bands and all 19
   fixtures comparing `dist` and `quant`; 60-row hashes + long
   `889c3be4ccd53dd7`; full suite with artifacts; four paired benchmark
   runs under the established 1% gate.

Risks: threading a cache context through six call sites; storing exact
quant bytes; workspace size (~49 bands × 8 slots × ~30 B ≈ 12 KB per pass);
and the possibility that the added loads/stores displace useful cache lines.
The cache must be dropped before the dead-band zeroing and must never
outlive the pass.

**Conditional recommendation:** the evidence supports one isolated
experiment *only* with the per-band design. If that refactor is not
acceptable, stopping is defensible: the expected net is 2–3% corpus-wide
and depends on keeping the lookup at 1–2 ns.

## Limitations

- Counters come from an instrumented build; its runtime is not used for any
  performance claim. Duplicate keys use slice addresses, valid only within
  a pass (by construction).
- One machine and five fixtures; music/speech dominate the corpus weighting.
- The phase tags lump the four `improve_scf` sites into phase 0.
- The impact estimate assumes a hit avoids the whole call; a real memo still
  scans and copies on hits.

## Validation

No production change. `quant.rs` and `scf.rs` were restored byte-for-byte
(`quant.rs` diff SHA-256 `ba6778bf…`, `scf.rs` no diff). HEAD and the index
are unchanged; retained diffs remain 379/55; `bitcount.rs` is clean.

- Production binary SHA-256 `61126376…` unchanged; production baseline
  **1 passed**.
- 60-row hash check unchanged; long hash `889c3be4ccd53dd7`.
- `git diff --check` clean.

## Files

| File | Contents |
|---|---|
| `starting-state.txt` | Git state and toolchain. |
| `call-sites.tsv` | Phase → source line → loop map. |
| `workload-counts.tsv`, `call-path-counters.log` | Per-workload counters and raw output. |
| `instrumentation-call-path.patch` | Temporary counters (removed). |
| `lookup-costs.txt`, `lookup-bench.rs` | Standalone lookup-cost measurement. |
| `production-binary-sha.txt`, `production-baseline.log`, `hash-verification-final.log` | Final validation. |
