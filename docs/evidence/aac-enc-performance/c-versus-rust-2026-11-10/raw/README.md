# Raw benchmark evidence

All files here are captured text; no audio is stored.

## `native/`

26 logs from the native series:

- `c-stand-{1,2,3}.log`, `r-stand-{1,2,3}.log` — standalone runs before the
  pairs.
- `c-pair-{1..8}.log`, `r-pair-{1..8}.log` — eight alternating pairs
  (`pair-1` C→R, `pair-2` R→C, …).
- `c-post-{1,2}.log`, `r-post-{1,2}.log` — post-series drift checks.

Each C log has `CFG`, `WARM`, `ROW`, `IN`, and `DONE` rows; each Rust log has
`CFG`, `WARM`, `ROW`, `VAL`, and `DONE` rows. `ROW` lines are the measured
repetitions; the analysis scripts parse `init_ns + encode_ns` as the total.
Every run has 20 warm-up rows and 300 measured `ROW` rows (20 workloads × 15
repetitions).

## `portable/`

16 logs (`cp-{1..8}.log`, `rp-{1..8}.log`) from the `generic`-CPU rebuild.
Each has 20 warm-up rows and 200 measured `ROW` rows (20 × 10). Order alternates
the same way as the native pairs.

## `validation/`

- `c-validate.log` — one-repetition native C validation over all 20 workloads.
- `rust-validate.log` — one-repetition native Rust validation; all published
  byte/hash pins are asserted.
- `c-portable-validate.log`, `r-portable-validate.log` — one-repetition
  portable validation.
- `c-artifact-comparison.txt` — rebuilt C outputs versus the 19 retained
  reference C ADTS artifacts; all `SAME`.
- `c-smoke.log`, `rust-smoke.log` — initial 3-repetition stability check.
- `production-baseline.log` — published `production_f32_corpus_baseline_is_unchanged`
  regression on the exact HEAD source (1 passed).

## `profiles/`

Raw macOS `sample` captures for the music and speech-clean workloads, C and
Rust, plus the corresponding 100/120-repetition harness logs. The stage
summary and top-symbol tables are in `../profile-stages.tsv` and
`../profile-symbols.tsv`.

## `environment/`

Session start/end and thermal state, the native/portable driver consoles,
CMake and Cargo build logs, and the analysis console outputs.
