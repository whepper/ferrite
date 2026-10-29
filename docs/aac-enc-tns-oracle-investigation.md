# TNS decision divergence investigation (D-012)

Date: 2026-10-27. Status: **diagnostic complete; no production TNS change; no
commit or push.**

## 1. Executive summary

The 40 committed TNS divergences in `KNOWN_TNS` are **not a TNS porting
defect**. Rust's integer merged-autocorrelation stage is bit-exact given the
same input (`fixed_merge_matches_rxx_bit_exact`), and running Rust's float
Schur/quantization/order/activation logic on the **C-exact RXX vectors**
reproduces the FDK order, coefficients, prediction gain (within the established
policy tolerance), and activation for representative cases.

The earliest causal difference is **upstream of TNS**: the f32 MDCT/Q31
spectrum fed to `merged_autocorr_fixed` differs from FDK's, and that difference
is amplified in the merged high-frequency autocorrelation (`RXX2`), especially
at PARCOR quantization borders. The TNS stage then makes a different but
algorithm-faithful decision.

**Classification: intentional numerical divergence** — the project's
documented float-native arithmetic versus FDK's fixed-point reference. A
secondary, bounded **intentional numerical divergence** is the float-Schur vs
fixed-point prediction-gain residual (D-013); on C-exact RXX it stays inside
the existing `8 + |want|/1000` policy and did not flip order/activation in the
cases examined.

No TNS activation flip was found in the committed corpus: all 40 pinned cases
keep the same `active` flags. The differences are order, coefficient, and/or
gain only (plus inactive-both-sides cases where no filter is transmitted).

## 2. Sources reviewed

Rust:

| Item | Location |
|---|---|
| Max-band table, config (`TNS_CONFIG` mirror) | `rust/src/aac_enc/tns.rs:137-246` |
| State (`TNS_DATA`/`TNS_INFO` mirror) | `rust/src/aac_enc/tns.rs:248-297` |
| PARCOR quantization and Schur/Levinson | `rust/src/aac_enc/tns.rs:301-382` |
| `invSqrtNorm2`, autocorrelation, normalization | `rust/src/aac_enc/tns.rs:392-651` |
| Filter analysis (order, sum, gain, activation) | `rust/src/aac_enc/tns.rs:652-705` |
| `detect` | `rust/src/aac_enc/tns.rs:707-796` |
| `synchronize_pair` | `rust/src/aac_enc/tns.rs:800-844` |
| `encode` (LPC filtering) | `rust/src/aac_enc/tns.rs:846-902` |
| TNS orchestration | `rust/src/aac_enc/psy.rs:787-870` |
| TNS syntax writing | `rust/src/aac_enc/frame.rs:631-765` |

FDK reference (pinned `f14ad40a574a4132df947d15e9fb14bee91cc8a5`):

| Item | Location |
|---|---|
| ACF windows/constants, max bands | `cpp/libAACenc/src/aacenc_tns.cpp:113-197` |
| `FDKaacEnc_InitTnsConfiguration` | `cpp/libAACenc/src/aacenc_tns.cpp:377-516` |
| `ScaleUpSpectrum`, `CalcAutoCorrValue`, `AutoCorrNormFac` | `cpp/libAACenc/src/aacenc_tns.cpp:519-617` |
| `FDKaacEnc_MergedAutoCorrelation` | `cpp/libAACenc/src/aacenc_tns.cpp:619-752` |
| `FDKaacEnc_TnsDetect` | `cpp/libAACenc/src/aacenc_tns.cpp:766-948` |
| `FDKaacEnc_TnsSync` | `cpp/libAACenc/src/aacenc_tns.cpp:961-1032` |
| `FDKaacEnc_TnsEncode` | `cpp/libAACenc/src/aacenc_tns.cpp:1051-1092` |
| `Search3`/`Search4`, `Parcor2Index`, `Index2Parcor` | `cpp/libAACenc/src/aacenc_tns.cpp:1141-1191` |
| `CLpc_AutoToParcor`, `CLpc_ParcorToLpc` | `cpp/libFDK/src/FDK_lpc.cpp:393-487` |
| TNS bitstream syntax | `cpp/libAACenc/src/bitenc.cpp:434-578` |

The actual FDK file is `aacenc_tns.cpp` (not `tns.cpp`). The tables, start/stop
bands, limits, thresholds, direction, ACF windows, and syntax logic match the
Rust constants and branch order. No missing activation condition or table
entry was found.

## 3. Baseline tests

Before changes:

| Test | Result |
|---|---|
| `cargo test --locked --features aac_enc --lib aac_enc::tns` | 3 pass (before adding the new test) |
| `aac_enc::psy::tests::psy_end_to_end_matches_oracle` | pass (includes all 40 `KNOWN_TNS` cases) |
| Full suite with reference artifacts | 336/336 pass |
| Full suite without external artifacts | 326 pass / 10 documented reference-artifact failures |

The existing `psy_end_to_end_matches_oracle` test already asserts the exact
FDK reference side and the recorded Rust side for every pinned divergent
window, and asserts TNS activation equality for every case. No tolerance was
changed.

## 4. Representative divergences

| Fixture | Frame | Ch | Window | Block | FDK order/gain/active | Rust order/gain/active | Class |
|---|---:|---:|---:|---|---|---|---|
| multitone-stereo | 1 | 0 | 6 | short | 3 / 1752 / active | 3 / 1773 / active | gain-only, same filter |
| onset-stereo | 24 | 0 | 4 | short | 2 / 10227 / active | 5 / 10317 / active | active both, order/coefs differ |
| onset-stereo | 25 | 0 | 0 | long | 11 / 22154 / active | 10 / 27723 / active | active both, order/coefs differ |
| onset-stereo | 26 | 0 | 0 | long | 0 / 1018 / inactive | 2 / 1047 / inactive | inactive both, no filter transmitted |
| stepdown-stereo | 1 | 0 | 3 | short | 5 / 6007 / active | 5 / 6107 / active | gain-only, same filter |

Other pinned cases include onset-44100 f22 and sine-stereo f5/f6; they fall
into the same two classes (active order/coefs, or inactive both). The full
40-entry `KNOWN_TNS` table remains the authoritative case list. Start/stop
band configuration, filter direction (up), and length derivation matched the
reference; the observed differences were order, coefficient, and/or gain.
Coefficient compression was not independently parsed from the bitstream.

## 5. Intermediate-value comparison

The committed `rxx_*` vectors are the FDK RXX1/RXX2 values; the Rust
diagnostic captured the Q31 spectrum entering TNS detect and recomputed RXX
with `merged_autocorr_fixed`.

| Fixture | f/ch/w | max \|ΔRXX1\| | max \|ΔRXX2\| | C RXX → Rust float | Rust actual |
|---|---:|---:|---:|---|---|
| multitone-stereo | 1/0/6 | 4,235 | **64,757,860** | C exact: order 3, coefs match, gain 1752 | order 3, same coefs, gain 1773 |
| onset-stereo | 24/0/4 | 770 | **698,176** | C exact: order 2, coefs match, gain 10229 | order 5, extra coef −1, gain 10317 |
| onset-stereo | 25/0/0 | 3,787 | **6,805,230** | C exact: order 11, coefs match, gain 22165 | order 10, coefs differ, gain 27723 |
| onset-stereo | 26/0/0 | 5,186 | **22,901,335** | C exact: order 0, inactive, gain 1018 | order 2, inactive, gain 1047 |
| stepdown-stereo | 1/0/3 | 4,290 | **2,840,148** | C exact: order 5, coefs match, gain 6012 | order 5, same coefs, gain 6107 |

`RXX1` is nearly identical in every case; the divergence is concentrated in
`RXX2`, the three-quarter merged autocorrelation. This is expected from the
merge: RXX2 combines three normalized quarters and is sensitive to small
spectrum differences and cancellation, while RXX1 is a single-quarter
correlation.

The `C RXX → Rust float` column is the decisive check:

- FDK order and coefficients are reproduced exactly.
- FDK activation is reproduced exactly.
- Prediction gains differ by at most 11 in the selected active cases, which is
  inside the existing `GAIN_TOL_FLOOR=8`, `GAIN_TOL_PER_MILLE=1` policy.
- Therefore the TNS arithmetic/order/quantization/sync stage is a faithful
  port; the difference is in the RXX input.

## 6. Earliest causal divergence

TNS has no cross-frame decision state that survives a reset; its first inputs
are the stored Q31 spectrum quarters and the resulting RXX vectors.

1. The f32 MDCT + scale/Q31 path produces a spectrum slightly different from
   FDK's (already documented in the M5.25/M5.26 transform evidence).
2. `merged_autocorr_fixed` converts that spectrum to RXX. Given the same
   spectrum it is bit-exact against the committed C RXX
   (`fixed_merge_matches_rxx_bit_exact`).
3. The first differing TNS intermediate is therefore **RXX2** (and to a much
   smaller degree RXX1), before Schur, PARCOR quantization, order selection,
   or activation.
4. The float Schur then quantizes the slightly different RXX at a PARCOR
   border; this produces the order/coefficient/gain differences. For example,
   at onset f24 w4 a 0.04% RXX2 difference flips an order-5 coefficient to
   zero and changes the transmitted filter.

This is not a table, index, direction, sync, or syntax-order defect. The
TNS syntax writer consumes the resulting TNS info and emits the corresponding
order/coefficient words; the frame-size changes in §7 are downstream evidence,
not an independent cause.

## 7. Encoded-size and decoded impact

Local ADTS frame sizes:

| Fixture | Frame | C bytes | Rust bytes | Delta |
|---|---:|---:|---:|---:|
| multitone-stereo | 1 | 223 | 223 | 0 |
| onset-stereo | 24 | 309 | 337 | +28 |
| onset-stereo | 25 | 119 | 119 | 0 |
| onset-stereo | 26 | 58 | 58 | 0 |
| stepdown-stereo | 1 | 253 | 252 | −1 |

The active order/coefficient difference at onset f24 changes the TNS side
information and the filtered spectrum, producing a local +28-byte frame. The
gain-only and inactive cases do not change the transmitted filter; onset f25
has different order/coefficients but happens to land on the same frame length.
These local sizes are not by themselves a quality measure.

Delay-aligned decoded SNR around the affected frames (FDK decoder; descriptive
only, not perceptual):

| Fixture | Frame | Window | Rust SNR | C SNR |
|---|---:|---:|---:|---:|
| multitone-stereo | 1 | 1024 | 46.90 | 44.90 |
| multitone-stereo | 1 | 4096 | 46.23 | 46.45 |
| onset-stereo | 24 | 1024 | 46.96 | 46.96 |
| onset-stereo | 24 | 4096 | 52.60 | 52.51 |
| onset-stereo | 25 | 1024 | 59.95 | 59.97 |
| onset-stereo | 25 | 4096 | 62.19 | 61.46 |
| stepdown-stereo | 1 | 1024 | 53.36 | 53.25 |
| stepdown-stereo | 1 | 4096 | 58.08 | 57.79 |

There is no consistent direction or clear quality loss from the Rust TNS
decisions in these windows. Other encoder differences contribute; this table
cannot attribute the waveform difference solely to TNS.

All compared streams decode; no new clipping instability, or encoding failure
was observed. TNS activation is unchanged in the pinned corpus, so the
"TNS enabled vs disabled" class is not represented by a committed divergence
case.

## 8. Classification

| Finding | Classification |
|---|---|
| RXX1/RXX2 differ before TNS analysis | **Intentional numerical divergence** from the documented float-native f32 MDCT/Q31 path; not a TNS porting defect. |
| Float Schur vs fixed `CLpc_AutoToParcor` gain residual on identical RXX | **Intentional numerical divergence** (D-013); bounded by the existing gain policy; no selected decision flip. |
| Order/coefficient changes after RXX differences | Consequence of the upstream numerical divergence, not a separate algorithm defect. |
| Activation flips | None found in the committed corpus; no case to classify. |
| Harness/input alignment | No discrepancy: committed C vectors and the Rust diagnostic use the same fixture/frame/window; C-exact RXX reproduces C. |

No **verified porting defect** and no **intentional algorithmic difference**
were found. The TNS algorithm and syntax-facing decisions correspond to FDK.

## 9. Regression coverage

Added in `rust/src/aac_enc/tns.rs`:

`aac_enc::tns::tests15::c_exact_rxx_reproduces_c_tns_analysis`

For the representative cases above, it feeds the **committed FDK RXX** to
Rust's `analyze_filter`, then asserts against the committed FDK TNS vectors:

- exact filter order,
- exact coefficient vector,
- prediction gain within `gain_within_tolerance`, and
- exact activation result.

It uses only committed `rxx_*`/`psy_*` vectors and runs without external
artifacts. Existing `psy_end_to_end_matches_oracle` continues to pin the actual
Rust/C divergences and activation equality.

Tests run after the change:

| Command | Result |
|---|---|
| `cargo test --locked --features aac_enc --lib aac_enc::tns` | 4/4 pass |
| `cargo test --locked --features aac_enc --lib aac_enc::psy::tests::psy_end_to_end_matches_oracle` | pass |
| Full suite with reference artifacts | **337/337 pass** |
| Full suite without external artifacts | 327 pass / 10 documented reference-artifact failures |
| `cargo test --locked -p aac-convert` | pass |
| `cargo test --locked --features aac_enc --doc` | 19/19 pass |
| `rustfmt --check` on changed Rust files | clean |
| `rust/tools/check-licensing.sh` | pass |

## 10. Impact assessment and recommendation

**Do not change production TNS behavior to force FDK parity.** The TNS port is
faithful; the divergent decisions are a boundary consequence of the upstream
f32 MDCT/Q31 spectrum. A TNS-only patch would compensate for an upstream
difference and could worsen other decisions.

Recommended actions:

1. Keep the new C-exact-RXX test and the existing `KNOWN_TNS`/`psy_end_to_end`
   coverage.
2. Record D-012 as an **intentional numerical divergence**, not an open
   porting defect. The remaining uncertainty is quality, not causality.
3. If closer FDK TNS parity is ever required, address the upstream spectrum
   accuracy (fixed-point/Q31 MDCT emulation or higher-precision coefficient
   tables), not the TNS branch. The f64 MDCT experiment already showed that
   more float precision alone does not close the FDK gap.
4. If quality work targets TNS-sensitive transients/music, use a controlled
   fixed-point-emulation comparison and perceptual metrics; do not infer
   superiority from matching TNS flags.

Remaining uncertainties:

- No perceptual metric/listening test was available; SNR is descriptive.
- Exact TNS side-info bits were not independently parsed; local frame-size and
  decoded SNR provide indirect evidence.
- The committed corpus/`KNOWN_TNS` set was examined; a full-corpus search for
  activation flips was not performed.
- D-013 gain residual is value-level and bounded; its interaction with other
  rate-control decisions is not exhaustively measured.

## 11. Files changed

| File | Change |
|---|---|
| `rust/src/aac_enc/tns.rs` | Added the C-exact-RXX TNS analysis regression test. No production TNS behavior changed. |
| `docs/aac-enc-tns-oracle-investigation.md` | This report. |
| `docs/evidence/aac-enc-tns/README.md` | Method, representative-case table, classification. |
| `docs/evidence/aac-enc-tns/rust-rxx-vs-c.log` | Actual Rust vs C RXX diagnostic |
| `docs/evidence/aac-enc-tns/c-rxx-float-analysis.log` | C-exact RXX through Rust's float TNS analysis |
| `docs/evidence/aac-enc-tns/local-snr.md` | Delay-aligned local SNR and frame sizes |
| `docs/README.md` | Index entry for this report |

Untouched: block switching, MDCT precision, MINSNR1, TNS production behavior,
and all unrelated concurrent work. The removed f64 experiment remains removed.
No commit or push was performed.
