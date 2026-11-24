# FPE M1 — Validation Commands and Results

Environment: `aarch64-apple-darwin` (Apple Silicon, the `production_baseline`
reference platform), rustc 1.99.1 (`58cf8f9c3 2026-10-13`), cargo 1.99.1
(`5f94df478 2026-08-27`). All Cargo commands run from `rust/` with `--locked`.
Base HEAD: `b188415e3c16dbf00706984858c6a568c9b705ff` (branch `main`), unchanged.

Logs in this bundle:
- `pre-change/` — captured on the pristine tree BEFORE any M1 edit.
- `post-change/` — captured after M1 implementation (includes the historical
  `post-fmt.log` showing the formatting diffs that were then fixed, and
  `post-clippy.log` with the full pre-existing warning set).
- `post-change/fresh-verification.log` — consolidated re-run at bundle
  creation time (every command echoed with its exit code); all green.

## 1. Pre-change baseline (pristine tree)

```sh
cd rust
cargo test --release --locked --lib production_baseline -- --nocapture
# 2 passed; 0 failed; 0 ignored; 378 filtered out

cargo test --locked --lib -- \
  --skip m5a_decode_check --skip m55_streams_match_c_reference \
  --skip phase1_frontend_agreement_sine_f5 --skip phase1e_sine_f5_band16_residue \
  --skip phase1e_onset_f24_bands_causality --skip phase1f_multitone_f1_no_live_divergence \
  --skip phase1f_stepdown_f1_no_live_divergence --skip phase1g_long_flips_residue \
  --skip phase1i_sine44_f5_tier_b --skip phase1i_multitone_f5_tier_b \
  --skip phase1_full_stream_parity --skip intensity_probe_c_oracle_parity \
  --skip intensity_probe_wav_parity --skip intensity_probe_adts_decodes \
  --skip m511_profile_loop
# 363 passed; 0 failed; 2 ignored; 15 filtered out
```
(The 15-test skip list is exactly the CI Tier-B exclusion set,
`.github/workflows/ci.yml`.)

## 2. New M1 tests

```sh
cargo test --locked --lib fpe
# 20 passed; 0 failed; 381 filtered out
```

**Test-count clarification (review Finding 2).** The substring filter `fpe`
selects **20** tests, because a test is selected iff its full path contains
`fpe`:
- 4 `aac_enc::fpe::constants::tests::*`
- 12 `aac_enc::fpe::tests::differential::*`
- 3 `aac_enc::stream_driver::fpe_seam_tests::*`
- 1 `aac_enc::config::tests::with_psy_engine_builder_selects_fpe_and_preserves_other_fields`

The remaining new config test,
`aac_enc::config::tests::psy_engine_defaults_to_q31_in_every_constructor`,
does **not** contain `fpe` in its name, so it is **not** selected by the `fpe`
filter; it runs in the broader Tier-A library suite (verified present via
`cargo test --locked --lib -- --list`). Therefore:
- **20** tests selected by the FPE filter;
- **+1** constructor-default test in the broader Tier-A suite;
- **= 21** total new M1 tests.

This is a reporting clarification only; no test behaviour changed. (The
earlier draft of this file mis-stated the filter as matching "2 config" tests;
the correct figure is 1 config test in the filter, 2 config tests total.)

Test inventory (21 new tests total):
- `aac_enc::fpe::constants::tests::{float_constants_equal_q31_over_2_31_exactly,
  clip_constants_match_sfb_config_accessors,
  pcm_noise_constant_reproduces_per_band_floors_exactly,
  tabulated_mask_factors_are_fractions_and_match_stored}` (4)
- `aac_enc::fpe::tests::differential::{fresh_fpe_state_init_matches_q31,
  onset_stereo_long_short_transitions_and_stale_ms_ld,
  stepdown_stereo_offset_transition, mono_transients_click_impulse_edge,
  sustained_tonal_and_noise, silence_and_near_silence,
  stress_and_full_scale_edges, pow2_boundary_energies,
  mixed_silent_active_pair, rate_44100_sine_and_onset,
  real_music_and_speech, committed_mdct_vectors_single_frame}` (12)
- `aac_enc::stream_driver::fpe_seam_tests::{default_selection_is_q31_and_allocates_q31_state_only,
  fpe_selection_allocates_fpe_state_only,
  fpe_selected_streams_are_byte_identical_to_q31}` (3)
- `aac_enc::config::tests::with_psy_engine_builder_selects_fpe_and_preserves_other_fields`
  (1 — **selected by the `fpe` filter**)
- `aac_enc::config::tests::psy_engine_defaults_to_q31_in_every_constructor`
  (1 — **not** `fpe`-filtered; runs in the broader Tier-A suite)

Filter arithmetic: 4 + 12 + 3 + 1 = **20** (`fpe` filter); 20 + 1 = **21** total.

## 3. Q31 regression gates

```sh
cargo test --release --locked --lib production_baseline -- --nocapture
# 2 passed; 0 failed  (VBR-5 corpus + VBR-6 default ADTS byte-length and
#  FNV-1a hash pins, enforced on aarch64-apple-darwin)

AAC_WAV_REPS=15 AAC_WAV_WARMUP=1 cargo run --release --locked --example aac_bench_wav
# WAVBENCH_DONE; exit 0. The example asserts every fixture hash against the
# committed VBR-6 pins (examples/aac_bench_wav.rs:208,345). Measured hashes
# identical to the M0 baseline (docs/fpe/m0-verification.md M0.4):
#   m7-music-stereo-48k        d6fc9d2c59cf5c36  (256763 bytes, 471 frames)
#   m7-speech-clean-stereo-48k 7c3e409574ebc62e  (129335 bytes, 471 frames)
#   m7-speech-pauses-stereo-48k 6c087bfb228d3bbe (127326 bytes, 471 frames)
#   m8-music-eroica-stereo-48k dcf07392921a18b7  (241021 bytes, 471 frames)
#   m7-music-stereo-48k-x4     7f3a621a091eee2f  (1021456 bytes, 1877 frames)
```

## 4. Full suites, builds, hygiene

```sh
cargo test --locked --lib -- <15 CI skips as above>
# 384 passed; 0 failed; 2 ignored; 15 filtered out   (363 pre-change + 21 new)

cargo test --locked --lib          # no skips — Tier-B accounting
# 388 passed; 11 failed; 2 ignored
# The 11 failures are EXACTLY the M0-recorded externally-blocked Tier-B set,
# all failing loudly on missing C artifacts (NOT Q31 regressions):
#   10 x "reference validation artifact missing" ($PHASE1_ORACLE_DIR /
#        $PHASE1_C_DIR / $M55_C_DIR): m55_streams_match_c_reference,
#        phase1_frontend_agreement_sine_f5, phase1_full_stream_parity,
#        phase1e_onset_f24_bands_causality, phase1e_sine_f5_band16_residue,
#        phase1f_multitone_f1_no_live_divergence,
#        phase1f_stepdown_f1_no_live_divergence, phase1g_long_flips_residue,
#        phase1i_multitone_f5_tier_b, phase1i_sine44_f5_tier_b
#    1 x m5a_decode_check ($AAC_DEC_BIN not present)
# Ignored (2, opt-in benchmarks, unchanged): quant_neon::tests::neon_kernel_microbench,
#   stream_driver::perf_tests::aac_perf_bench

cargo test --locked --test production_api     # 6 passed; 0 failed
cargo test --locked --doc                     # 19 passed; 0 failed
cargo build --locked                          # OK
cargo build --locked --no-default-features    # OK
cargo fmt --all -- --check                    # clean (after formatting new code)
cargo clippy --locked --all-targets           # exit 0; 183 pre-existing warnings,
                                              # ZERO in new/edited regions
bash rust/tools/check-licensing.sh            # OK (79 rust/src files carry TPMV notice)
git diff --check                              # clean
cargo package --locked -p ferrite-aac --list --allow-dirty
# exit 0; NOTICE + LICENSE-FDK-AAC shipped; all 9 fpe/ source files included
```

## 5. Fixtures (FPE-1.4)

```sh
python3 tools/enc-oracle/make_fpe_fixtures.py <dir-run1>   # x3 separate dirs
shasum -a 256 <dir-runN>/*.wav                              # identical across runs
```
Result: all 6 fixtures STABLE (see `fixture-stability.txt`).
Python↔Rust resynthesis cross-check: see `fixture-crosscheck.md`
(sample-exact, temporary harness, removed after verification).

## 6. Blocked / not run (reported explicitly, never counted as passing)

- C↔Q31 and C↔FPE Tier-B oracle comparisons: blocked on private-reference
  re-provisioning (pinned `f14ad40a…` unavailable; A-3 NOT resolved; R-12).
  FPE-0.1 / FPE-0.4 remain blocked. The M1 in-binary harness is C-independent.
- Oracle ring/psy/seq vector extraction for the 6 new fixtures: blocked (same
  reason). Fixture PCM generation + stability + cross-check completed instead.
- Decoder-backed tests (`AAC_DEC_BIN`): blocked (no decoder artifact).
- M9 paired performance protocol: not applicable at M1 (single-arm Q31 bench
  only, for hash regression evidence).