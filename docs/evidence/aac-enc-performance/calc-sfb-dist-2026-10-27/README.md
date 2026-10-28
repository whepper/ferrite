# `calc_sfb_dist` bounds-check experiment — 2026-10-27

Follow-up to
[`docs/aac-enc-performance-baseline.md`](../../../aac-enc-performance-baseline.md)
§11. This directory contains the before/after evidence for the retained
`quant::calc_sfb_dist` bounds-check cleanup.

## Change

- `calc_sfb_dist`: one `spec.len() == quant.len()` assertion per call; the
  line loop uses `zip` instead of indexed `quant[i]`.
- `quantize_line_body` (both branches) and `inv_quantize_line`: the existing
  mantissa-table mask `& !512` is written as `& 0x1FF`. Normalization leaves
  the shifted value in `0..=0x3FF`, so this is the same index and proves the
  512-entry table bound to LLVM.
- No arithmetic, accumulation order, early-out position, table values, output
  writes, or floating-point behavior changed. No unsafe code or intrinsics.

Exact diff: [`source-diff.txt`](source-diff.txt).

## Binaries

| Binaries | SHA-256 |
|---|---|
| `ferrite_aac-before` (release test binary before change) | `f6546d90890fbe9af3b4f2876a46a965bfce5952881b09e11e0b13e105958968` |
| `ferrite_aac-after-mask` (release test binary after change) | `c58116db0d877e23a42f7a2246202bd572fa735f75ffc912b3e3c4a7f170bfca` |
| `example-before` (non-test production example, old quant.rs) | `e27fbb805b0599c883a917769b5fc27170a147fd769a5a3538565d959f255b18` |
| `example-after` (non-test production example, new quant.rs) | `033c27341325f8f46b345fd174c6f95f9a33f6f9cd2a93b25d21b0d4b1374a18` |

The old example was built from an isolated temp copy of the workspace with
`quant-before.rs` restored, using the same rustc, release profile, and
`aac_enc` feature. The worktree was not swapped or reset.

## Paired method

Both arms used 15 measured repetitions per fixture and one warm-up pass.
The old/new binaries were alternated to control drift:
`old→new`, `new→old`, `old→new`, `new→old`.

- `testbuild-*.log`: full 19-fixture + 40 s derived benchmark, quiet mode.
- `production-*.log`: four committed WAVs + 40 s derived workload through the
  non-`cfg(test)` example.

The test-build benchmark used 8,000-frame chunks and validated all 19
`production_baseline` hashes and the derived long hash. The example validated
the four committed WAV hashes and the long hash.

## Results

| Arm | Pairs >1% | Median improvement |
|---|---:|---:|
| Test-build production | **4 / 4** | **2.82%** |
| Test-build core (feeder excluded) | 4 / 4 | 2.83% |
| Non-test production example | **4 / 4** | **3.01%** |

Detailed tables: [`optimization-testbuild-ab.tsv`](optimization-testbuild-ab.tsv),
[`optimization-production-ab.tsv`](optimization-production-ab.tsv),
[`optimization-production-fixture.tsv`](optimization-production-fixture.tsv).

Assembly: [`optimization-assembly.tsv`](optimization-assembly.tsv). The
`calc_sfb_dist` function went from 212 to 196 instructions; all four
`panic_bounds_check` paths disappeared (per-line `quant[i]` and three table
checks), branches dropped from 10 to 5, and the function shrank from 0x350 to
0x310 bytes. A once-per-call length `assert_failed` path and the existing
`specExp` assertion remain outside the line loop.

Profile: [`optimization-profile-share.tsv`](optimization-profile-share.tsv).
The corpus `calc_sfb_dist` self-time share moved from 24.43% to 23.38% of
worker samples; this is directional only because sampling noise is comparable
to the shift. The end-to-end A/B is primary.

## Caveats

- Absolute timings in this directory are from an A/B session in which the
  pre-change arm ran slower than the original baseline session; use the paired
  percentages, not the absolute ms, for the experiment effect. The original
  baseline measurements remain in the parent directory.
- One machine/build (Apple M5, rustc 1.99.1, generic aarch64 release).
- The production example covers four committed WAVs, not the 15 synthetic
  fixtures; the synthetic coverage is the test-build benchmark.
- No perceptual claim is made. Hash identity and A/B throughput are the
  acceptance evidence.
- No C oracle was built and no reference artifact was regenerated.
