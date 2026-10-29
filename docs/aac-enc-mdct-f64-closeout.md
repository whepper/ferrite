# Ferrite MDCT precision experiment — closeout

Date: 2026-10-27. Status: **closed out; no commit or push.**
Production decision: **retain f32**. The experimental f64 implementation was
removed. The report, derived evidence, and a production-baseline regression
test are retained.

## 1. Retained artifacts

| Artifact | Purpose |
|---|---|
| [`aac-enc-mdct-f64-precision-experiment.md`](aac-enc-mdct-f64-precision-experiment.md) | Full experiment report: architecture, measurements, downstream effects, quality, performance, recommendation. |
| [`evidence/aac-enc-mdct-f64/README.md`](evidence/aac-enc-mdct-f64/README.md) | Provenance and file map for the retained evidence. |
| [`evidence/aac-enc-mdct-f64/downstream-comparison.tsv`](evidence/aac-enc-mdct-f64/downstream-comparison.tsv) | Per-fixture f32/f64 frame/byte/Q31/TNS comparison. |
| [`evidence/aac-enc-mdct-f64/downstream-summary.txt`](evidence/aac-enc-mdct-f64/downstream-summary.txt) | Aggregate downstream counts. |
| [`evidence/aac-enc-mdct-f64/oracle-transform-comparison.tsv`](evidence/aac-enc-mdct-f64/oracle-transform-comparison.tsv) | Per-fixture transform comparison against FDK `enc_oracle`. |
| [`evidence/aac-enc-mdct-f64/decoded-quality-comparison.md`](evidence/aac-enc-mdct-f64/decoded-quality-comparison.md) | Decoded f32/f64/C SNR, windowed SNR, clipping. |
| [`evidence/aac-enc-mdct-f64/performance.txt`](evidence/aac-enc-mdct-f64/performance.txt) | Timing, RSS, hardware, and method. |
| [`evidence/aac-enc-mdct-f64/block-switch-finding.txt`](evidence/aac-enc-mdct-f64/block-switch-finding.txt) | Post-experiment block-switch divergence evidence. |
| `rust/src/aac_enc/production_baseline.rs` | New regression test pinning all 19 corpus ADTS byte lengths (all platforms) and FNV-1a hashes (reference platform, Apple Silicon macOS) for the default f32 path. |

The production transform remains `rust/src/aac_enc/transform.rs` plus the f32
`common/mdct.rs`, `common/dct.rs`, and `common/fft.rs` kernels. No production
encoder behavior was changed by this closeout.

Verification: the experiment report's downstream, oracle, quality, and
performance tables were checked against the retained files in
`docs/evidence/aac-enc-mdct-f64/`; the values agree. The report's raw artifact
paths remain valid for the temporary experiment directory, but are not needed
to review the conclusions.

## 2. Removed experimental code

The following were removed from the working tree after the experiment:

- `rust/src/aac_enc/transform64.rs` (f64 ring/MDCT/DCT-IV/FFT).
- `rust/src/aac_enc/m9_f64_experiment.rs` (comparison/oracle/performance harness).
- The `transform-f64-experiment` Cargo feature.
- The feature-gated transform backend selector and diagnostic capture in
  `rust/src/aac_enc/stream_driver.rs`.

These files were never committed, so removal is not a history revert and
does not appear as a deletion diff. The reviewable closeout set is the
retained report/evidence plus the regression test, with no experimental
feature or transform selector left in `Cargo.toml`, `aac_enc.rs`, or
`stream_driver.rs`. If the experimental code had been committed, its removal
would be a separate revert-style change.

Why remove rather than preserve:

- The experiment conclusively showed **no decoded-quality or FDK-agreement
  benefit** from f64; the residual is table/Q31-limited, not f32-accumulation
  limited.
- The f64 path added ~14–15% encode cost and a second transform/FFT
  implementation plus test-only capture hooks.
- Its diagnostic value is fully captured by the report and evidence; retaining
  the implementation would add maintenance cost without a planned use.

## 3. Verification and tests run

Environment: Apple M5, arm64, macOS 27.0.1, rustc/cargo 1.99.1. Reference
artifacts: pinned FDK `aac-enc`/`aac-dec` and regenerated `enc_oracle` dumps.

| Command | Result |
|---|---|
| `cargo test --locked --features aac_enc --lib` with `PHASE1_ORACLE_DIR`, `PHASE1_C_DIR`, `AAC_DEC_BIN`, `M55_C_DIR` | **335/335 pass** (includes the new baseline test) |
| `cargo test --locked --features aac_enc --lib` without external artifacts | 325 pass / **10 documented external-artifact failures** (same reference-dependent tests as before; no new failure) |
| `aac_enc::production_baseline::production_f32_corpus_baseline_is_unchanged` | pass; all 19 byte lengths match, and FNV-1a hashes match on the reference platform (Apple Silicon macOS) |
| `cargo test --locked -p aac-convert` | pass (cover art 11, loudness 15, metadata 19; ignored decode tests unchanged) |
| `cargo test --locked --features aac_enc --doc` | 19/19 pass |
| `cargo build --locked --no-default-features --features aac_enc` | pass |
| `rust/tools/check-licensing.sh` | pass; 67 `rust/src` files carry the modified-version notice |
| `rustfmt --check` on `production_baseline.rs` | clean |
| `cargo fmt --all -- --check` | still reports pre-existing diffs in unrelated files; not introduced here |

The 10 artifact-dependent failures are the documented set:
`m5a_decode_check`, `phase1_frontend_agreement_sine_f5`,
`phase1_full_stream_parity`, `phase1e_*`, `phase1f_*`, `phase1g_*`,
`phase1i_*`, and `m55_streams_match_c_reference`.

The retained full-suite result proves the default f32 path still reproduces
the established corpus baseline; the new test makes that guarantee explicit
without requiring external artifacts. The later block-switch investigation
adds one boundary test and the TNS investigation adds one C-exact-RXX test;
the current suite is **339/339** with reference artifacts (329 pass / 10
documented failures without them). See
[`aac-enc-block-switch-oracle-investigation.md`](aac-enc-block-switch-oracle-investigation.md).

## 4. Proposed cleanup

No further code cleanup is proposed. The experimental implementation is gone;
the retained material is documentation, evidence, and one self-contained
regression test.

Optional housekeeping after review:

- Remove the raw temporary experiment artifacts under
  `/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/f64-mdct-experiment/`
  once the retained evidence has been reviewed. They include large oracle
  dumps and generated ADTS/WAV files and are not needed for CI.
- Do not delete `docs/evidence/aac-enc-mdct-f64/` or the experiment report.

## 5. Outstanding questions

1. **Block-switch divergence on real speech — investigated 2026-10-27.** The
   M9 full-corpus oracle comparison found six channel-frame sequence
   mismatches in `m7-speech-clean-stereo-48k`, frames 107–109. The follow-up
   [`aac-enc-block-switch-oracle-investigation.md`](aac-enc-block-switch-oracle-investigation.md)
   traced the cause to FDK's fixed-point per-sample energy truncation versus
   Rust's f32 accumulation at a knife-edge f107 attack test. Classification:
   **intentional float-native precision divergence, not a porting defect**;
   no production algorithm change. Evidence:
   `evidence/aac-enc-block-switch/`.
2. **D-012 TNS decision divergences — investigated 2026-10-27.** Forty pinned
   windows. The follow-up
   [`aac-enc-tns-oracle-investigation.md`](aac-enc-tns-oracle-investigation.md)
   shows the TNS analysis is faithful on C-exact RXX; the divergences come
   from different upstream f32-MDCT/Q31 RXX inputs, especially RXX2.
   Classification: **intentional numerical divergence, not a TNS porting
   defect**; no production TNS change. Quality impact remains bounded by
   local decoded measurements, not proven perceptually.
3. **MINSNR1 value envelope — investigated 2026-10-27.** The follow-up
   [`aac-enc-minsnr1-oracle-investigation.md`](aac-enc-minsnr1-oracle-investigation.md)
   shows the S2 adaptation is exact on C inputs; the production differences
   come from upstream `SENL`/`PENLD` residue, with the known MS band-16
   decision as a second cause on two frames. Classification: **intentional
   numerical divergence, not a MINSNR1 porting defect**; no production change.
   `THRV`/flags stayed in-envelope on the studied frames, and the
   `quiet-sine` control remained byte-identical.
4. **D-021 oracle short-window replay blips.** A subset of frozen oracle
   frames cannot be attributed downstream until the replay input is isolated.
5. **Perceptual evidence.** No PEAQ/ViSQOL/listening test exists; all quality
   statements are waveform/SNR based.
6. **Concurrent worktree changes.** Unrelated in-progress changes in
   `frame.rs`, `pe.rs`, `phase1b.rs`, `psy.rs`, `rate.rs`, `sfb.rs`, `tns.rs`,
   and `common/hybrid.rs` were left untouched during this closeout and were
   subsequently committed as `c4d1262` by that work. The block-switch
   investigation was run on that coherent tree.

## 6. Highest-value correctness finding (investigated)

The block-switch divergence on `m7-speech-clean-stereo-48k` frames 107–109 was
the highest-value open item identified here. It has now been investigated:

- FDK emits START/SINE → SHORT/SINE → STOP/KBD; Rust emits LONG/KBD.
- The first causal state difference is at f106: FDK `accWindowNrg` 450 vs
  Rust 497, and window-7 filtered energy 992 vs 1048 (Q31 units). At f107
  window 0 the FDK margin is +11.1 and the Rust margin is −32.5, so the attack
  test flips.
- The cause is FDK's fixed-point per-sample energy truncation
  (`fPow2Div2(...) >> 4`) versus Rust's f32 real accumulation, amplified by the
  recursive accumulator through a quiet region. The Rust branch/table/state
  port matches the C implementation.
- Classification: **intentional float-native precision divergence, not a
  porting defect or harness discrepancy.**
- Local decoded SNR around the event does not show a consistent loss from the
  Rust choice; both streams decode and full-band speech-clean SNR differs by
  only +0.03 dB (Rust − C). No perceptual claim is made.
- A self-contained regression test now pins the float-side f107 boundary
  (`speech_clean_f107_attack_boundary_is_float_limited`). No production
  algorithm change is recommended: emulating Q31 truncation would alter many
  low-level decisions without demonstrated quality benefit.

Full evidence and measurements:
[`aac-enc-block-switch-oracle-investigation.md`](aac-enc-block-switch-oracle-investigation.md)
and `evidence/aac-enc-block-switch/`.

If future quality work targets real-speech transients, the next controlled
step is a forced-sequence or Q31-emulated block-switch comparison with
localized perceptual metrics—not a branch patch and not more floating-point
precision.

## 7. Authorization status

No commit or push was performed. The working tree contains this closeout,
the retained report/evidence, the new regression test, and unrelated concurrent
changes that must be reviewed separately.
