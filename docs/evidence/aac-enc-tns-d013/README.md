# D-013 TNS activation-margin census evidence

Diagnostic evidence for
[`docs/aac-enc-tns-d013-activation-margin-census.md`](../../aac-enc-tns-d013-activation-margin-census.md).
No production behavior was changed.

## Method

- Enumerate every committed `rust/src/aac_enc/testdata/rxx_*.txt` vector
  (47 fixture/frame/channel files).
- Resolve the block type from the committed `seq_*` rows and build the same
  `TnsConfig` as production (`SfbConfig` geometry, channel/TNS bitrate).
- Recompute HI from `RXX2` and LO from `RXX1` with the test-accessible
  `analyze_filter` (float Schur + exact coefficient quantization).
- Compare HI/LO order and coefficient vectors against the committed C
  `psy_*` TNS vectors; all 128 window/filter rows matched exactly.
- Evaluate the exact production activation conditions:

```text
HI: gain > 1437 || sumSqr > limitOrder/2 + 2
LO: (gain > 1500 && gain < 16000*limitOrder)
    || (sumSqr > 9 && sumSqr < 22*limitOrder)
```

LO detect is evaluated only for long blocks with committed HI active, because
production does not run LO otherwise.

- Compute `residual = |gain_rust - gain_committed|`,
  `d_lo = |gain_rust - lower boundary|`, `d_hi = |gain_rust - upper boundary|`,
  and the sum-boundary distances. The crossing predicates account for strict
  comparison and the side of the boundary; distance alone is not treated as a
  flip.
- Repeat the crossing check with `gain_tolerance(gain_committed)` as a
  conservative policy-gate envelope, and with the literal triage condition
  `d_gain <= residual + 1` while the sum branch is false.

## Field dictionary (`census.tsv`)

The table has no `CENSUS` tag column; the tag was stripped from the test's
stdout rows for the committed TSV.

| Field | Meaning |
|---|---|
| `fixture`, `frame`, `ch`, `window`, `short` | Committed vector identity; `short=1` is an eight-window short block. |
| `filter` | `HI` (RXX2) or `LO` (RXX1). |
| `gain_r`, `gain_c` | Rust float-Schur gain and committed C `TNSG` gain, both integer ×1000. |
| `residual` | `abs(gain_r - gain_c)`. |
| `threshold_lo`, `threshold_hi` | Gain activation boundaries. HI uses only 1437; LO uses 1500 and 80000. `-1` means not applicable. |
| `d_lo`, `d_hi` | Absolute distance from `gain_r` to the lower/upper boundary; `-1` when no upper bound. |
| `sum` | `sumSqrCoef` from the quantized filter coefficients. |
| `bound_lo`, `bound_hi` | Sum-branch boundaries. HI has only `limitOrder/2 + 2`; LO has 9 and `22*limitOrder`. |
| `d_sum_lo`, `d_sum_hi` | Absolute distance from `sum` to the lower/upper sum boundary; `-1` when there is no upper boundary. |
| `sum_true` | The exact sum branch is true. When true, the gain branch cannot change activation. |
| `active_c`, `active_r` | Committed C activation and recomputed Rust activation. |
| `cross` | The observed residual could cross a gain boundary while the sum branch is false, under the strict comparison and side condition. |

## Key results

`summary.txt`:

```text
hi_windows=124 lo_windows=4 max_residual=11 max_residual_decision=1
max_gate=30 min_hi_margin=54 min_hi_margin_decision=54
min_lo_low_margin=343 min_lo_high_margin=78843
literal_candidates=0 gate_candidates=0 candidates=0
actual_flips=0 coefficient_mismatches=0
```

The closest HI decision-relevant window is `multitone-stereo f1 w7`:

```text
gain_r=1383 gain_c=1383 residual=0
threshold=1437 margin=54
sum=1 boundary=4 sum_true=0
active_c=0 active_r=0 cross=0
```

The maximum residual is at `onset-stereo f25 w0` (both channels):

```text
gain_r=22165 gain_c=22154 residual=11
threshold=1437 margin=20728
sum=51 boundary=8 sum_true=1
active_c=1 active_r=1 cross=0
```

Because the sum branch is true there, the gain residual is not
decision-relevant; the gain margin is also 20,728 units. The largest residual
on a sum-false HI window is 1 (`multitone-stereo f5 w0`, gain 1051 vs 1050,
margin 386), so the decision-relevant residual is far smaller than the global
maximum.

The only LO windows are `onset-stereo f25 w0` (gain 1157, lower margin 343,
upper margin 78,843) and `stepdown-stereo f2 w0` (gain 1012, lower margin
488, upper margin 78,988), both channels. All are sum-false and inactive on
both sides.

## Reproduction

From `rust/`:

```sh
cargo test --locked --features aac_enc --lib \
  aac_enc::tns::tests_gain_gate::d013_activation_margin_census -- --nocapture
```

The test prints the `CENSUS` rows to stdout. `census.tsv` and `summary.txt`
are the extracted rows and summary from that run. The test uses only committed
vectors; it needs no C oracle and no external artifacts.
