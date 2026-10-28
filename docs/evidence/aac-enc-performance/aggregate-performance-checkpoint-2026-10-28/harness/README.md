# Measured benchmark harness snapshot

This directory preserves the test-only source used to produce the aggregate
checkpoint. It is an evidence snapshot, not production code. The production
commit intentionally contains only the four retained encoder optimization files;
the harness below is entangled with unrelated quality-test instrumentation in
the worktree, so it is preserved here instead of being committed as source.

## Files

| File | SHA-256 | Role |
|---|---|---|
| `stream_driver.rs` | `002368dd89229f7241a1b6ed9731a1d99aaf4722f1e10af4d409475c075236e2` | Full measured `stream_driver.rs`, including the `#[cfg(test)] perf_tests` benchmark module, the test-only S1/S2 captures, and the phase1/m511 tests. No production path differs from the committed file except the uncommitted test-only additions. |
| `production_baseline.rs` | `3f42a13a85c7a0e81f7da3ded63e06baeae4a06caba5a3cf8a796e3183dcd7e6` | Test-only corpus byte/hash pins used by the harness and the production-baseline test. |
| `aac_enc-registration.patch` | — | Adds `#[cfg(test)] mod production_baseline;` to `rust/src/aac_enc.rs`. |
| `aac_bench_wav.rs` | `360f1cf8e8201d2801e0b90f788094b8abc37316052b2897bb96f0a077018958` | Non-`cfg(test)` example used by the earlier production-example evidence; not used for the aggregate headline. |

## Reproduce the measured test build

From the repository root after checking out the performance commit:

```sh
cp docs/evidence/aac-enc-performance/aggregate-performance-checkpoint-2026-10-28/harness/stream_driver.rs \
   rust/src/aac_enc/stream_driver.rs
cp docs/evidence/aac-enc-performance/aggregate-performance-checkpoint-2026-10-28/harness/production_baseline.rs \
   rust/src/aac_enc/production_baseline.rs
git apply docs/evidence/aac-enc-performance/aggregate-performance-checkpoint-2026-10-28/harness/aac_enc-registration.patch
cp docs/evidence/aac-enc-performance/aggregate-performance-checkpoint-2026-10-28/harness/aac_bench_wav.rs \
   rust/examples/aac_bench_wav.rs
cd rust
cargo test --release --locked --features aac_enc --lib --no-run
AAC_PERF_BENCH=1 AAC_PERF_REPS=15 AAC_PERF_LONG=1 AAC_PERF_QUIET=1 \
  target/release/deps/ferrite_aac-549f414e79686bc8 --ignored --nocapture \
  aac_enc::stream_driver::perf_tests::aac_perf_bench
```

The test binary path embeds a Cargo hash; use the `--no-run` output rather than
assuming the filename above. Build-path and compiler differences change the
binary SHA-256 and can shift absolute timings slightly; the committed logs and
`paired-summary.tsv` are the authoritative measurements. The harness snapshot
includes the same test-only captures in both measured arms, so the A/B
comparison is controlled, but it is not a production-build benchmark.
