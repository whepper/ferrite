# TNS D-012 oracle investigation evidence

Diagnostic evidence for the TNS decision divergences documented in
`docs/aac-enc-tns-oracle-investigation.md`. The logs and tables here are
measurement-only; no production TNS behavior was changed.

## Method

- Fixture PCM is the committed 48 kHz corpus (`rust/src/aac_enc/testdata/`).
- FDK reference: pinned `f14ad40a574a4132df947d15e9fb14bee91cc8a5`,
  `libfdk-aac.2.0.4.dylib`.
- Committed C vectors: `rxx_*` (raw FDK RXX1/RXX2), `psy_*` (FDK TNSW/TNSG),
  `mdct_*`, and `seq_*`.
- Rust diagnostic (temporary test-only capture, removed after evidence
  collection): run the real `BlockSwitch` + `ChannelTransform` + `PsyScratch`
  path, read the Q31 spectrum entering TNS detect, compute
  `merged_autocorr_fixed` on it, and compare with committed C RXX.
- C-exact diagnostic: take the committed FDK RXX vectors, run Rust's float
  Schur / quantization / order / activation stage
  (`analyze_filter`) on them, and compare with FDK `TNSW`/`TNSG`.
- No external oracle is needed for the retained permanent test; it uses the
  committed vectors.

## Representative cases

All values are from the committed vectors and the diagnostic logs.

| Fixture | f | ch | w | block | C ord/gain/active | Rust ord/gain/active | max \|ΔRXX1\| | max \|ΔRXX2\| | C RXX → Rust float |
|---|---:|---:|---:|---|---|---|---:|---:|---|
| multitone-stereo | 1 | 0 | 6 | short | 3 / 1752 / active | 3 / 1773 / active | 4,235 | 64,757,860 | order/coefs exact; gain 1752 |
| onset-stereo | 24 | 0 | 4 | short | 2 / 10227 / active | 5 / 10317 / active | 770 | 698,176 | order/coefs exact; gain 10229 |
| onset-stereo | 25 | 0 | 0 | long | 11 / 22154 / active | 10 / 27723 / active | 3,787 | 6,805,230 | order/coefs exact; gain 22165 |
| onset-stereo | 26 | 0 | 0 | long | 0 / 1018 / inactive | 2 / 1047 / inactive | 5,186 | 22,901,335 | inactive/order 0 exact; gain 1018 |
| stepdown-stereo | 1 | 0 | 3 | short | 5 / 6007 / active | 5 / 6107 / active | 4,290 | 2,840,148 | order/coefs exact; gain 6012 |

The `C RXX → Rust float` column shows the result of running Rust's TNS
analysis on the **C-exact** RXX input. It reproduces the FDK order,
coefficients, and activation; gains differ by ≤11 and remain inside the
established `8 + |want|/1000` policy. This isolates the divergence to the
RXX/spectrum supplied to TNS, not to the TNS analysis stage.

## Local encoded sizes

| Fixture | Frame | C bytes | Rust bytes | Delta |
|---|---:|---:|---:|---:|
| multitone-stereo | 1 | 223 | 223 | 0 |
| onset-stereo | 24 | 309 | 337 | +28 |
| onset-stereo | 25 | 119 | 119 | 0 |
| onset-stereo | 26 | 58 | 58 | 0 |
| stepdown-stereo | 1 | 253 | 252 | −1 |

The onset f24 frame has an active filter with different order/coefficients;
the gain-only and inactive cases show no local size change. The onset f25
order difference happens to yield the same frame length.

## Logs

- `rust-rxx-vs-c.log`: actual Rust RXX vs committed FDK RXX and the resulting
  TNS decisions for the representative cases.
- `c-rxx-float-analysis.log`: Rust's float TNS analysis on C-exact RXX.
- `local-snr.md`: delay-aligned decoded SNR around the active-filter frames.

## Classification

The TNS port's branches, tables, state order, and syntax-facing decisions were
checked against `aacenc_tns.cpp`; no logic/state-order porting defect was
found. The final decisions diverge because the upstream f32 MDCT/Q31 spectrum
produces different RXX values, especially the merged high-frequency RXX2.
That is the project's documented float-native numerical difference, not a TNS
porting defect. With C-exact RXX, Rust reproduces the C TNS analysis.
