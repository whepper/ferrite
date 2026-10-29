# D-013 TNS activation-margin census

Date: 2026-10-27. Status: **diagnostic complete; no production change; no
commit or push.**

## 1. Verdict and precise scope

**Verdict: PASS WITH FINDINGS.**

The same-input TNS prediction-gain residual **cannot flip TNS activation on
any committed C-exact TNS window**. The census covers 47 committed
`rxx_*` files and 128 window/filter rows (124 HI, 4 LO):

- maximum observed gain residual: **11** gain units (×1000);
- maximum residual on a window where the exact sum branch is false:
  **1** gain unit;
- minimum decision-relevant HI margin: **54** gain units;
- minimum LO lower-boundary margin: **343**; minimum LO upper-boundary
  margin: **78,843**;
- literal triage candidates: **0**;
- policy-gate candidates: **0**;
- exact side-aware crossing candidates: **0**;
- actual activation differences from the committed C state: **0**;
- coefficient/order mismatches on C-exact RXX: **0**.

The closest committed boundary is `multitone-stereo` frame 1, window 7
(short block): Rust gain 1383, C gain 1383, threshold 1437, distance 54,
sum 1 against boundary 4 (false), inactive on both sides. Because the
comparison is strict (`gain > 1437`), a +55 change (the 54-unit distance plus
the strict step) would be required to activate it; the largest measured
residual anywhere is 11 and the largest decision-relevant residual is 1.

### Scope of the conclusion

This closes D-013 **for the committed corpus**, not for all possible inputs.
The census uses the committed FDK RXX vectors, so it isolates the gain
arithmetic from the D-012 upstream-spectrum question. It does not cover
signals, fixtures, or RXX vectors that are not committed. It also does not
exercise an LO-active case: the four committed LO windows are all inactive,
so the LO activation path has margin evidence but no active-decision case.

## 2. Activation formula and verified branch semantics

### 2.1 Units and values

- `gain_r` and `gain_c` are prediction gains multiplied by 1000, stored as
  integers. Rust computes `(gain * 1000.0).floor() as i32`
  (`rust/src/aac_enc/tns.rs:652-675`); FDK computes
  `fMultNorm(predictionGain_m, predictionGain_e, 1000, 31, 31)`
  (`cpp/libAACenc/src/aacenc_tns.cpp:814-820`, `:865-872`).
- The gain thresholds are `threshOn = [1437, 1500]` for `[HI, LO]`
  (`rust/src/aac_enc/tns.rs:173,238`; C `aacenc_tns.cpp:850,901`).
- `sumSqr` is the integer sum of squared **quantized** coefficient indices
  from index 0 through the truncated order (`tns.rs:666-674`). On C-exact RXX
  the quantized coefficients match C exactly; the census verifies this for
  every row.
- `limitOrder` is `[12, 5]` for long blocks and `[5, 0]` for short blocks
  (`tns.rs:175-176,239`). LO is disabled on short blocks.

### 2.2 Exact production conditions

HI (`tns.rs:752`; C `aacenc_tns.cpp:848-850`):

```text
active_hi = gain > 1437 || sumSqr > limitOrder[HI] / 2 + 2
```

LO (`tns.rs:764-765`; C `aacenc_tns.cpp:901-941`), evaluated only for long
blocks and only after HI activation:

```text
active_lo = (gain > 1500 && gain < 16000 * limitOrder[LO])
         || (sumSqr > 9 && sumSqr < 22 * limitOrder[LO])
```

All four comparisons are strict. Integer division is used for
`limitOrder[HI] / 2`. The LO upper gain bound (80,000 for `limitOrder[LO]=5`)
and the LO upper sum bound (110) are part of the decision and were included;
the triage's one-threshold shorthand was not sufficient for LO.

### 2.3 What the residual can and cannot affect

`predictionGain` is consumed by the TNS activation test and by FDK's
`PnsDetect` (`psy_main.cpp:1152`). PNS is disabled in this configuration
(`NO_NOISE_PNS`), and Rust does not read `TnsChannel.gain` in production.
`FDKaacEnc_TnsEncode` filters with LPC coefficients from
`CLpc_ParcorToLpc` and its normalization exponent, not with
`predictionGain` (`FDK_lpc.cpp:301-352,393-429`). D-013 is therefore an
activation-margin question, not a spectral-filter-scale question.

### 2.4 Crossing criterion, including the triage rule

For a lower bound with strict `gain > T`, a residual-sized move of magnitude
`r` can cross only if the threshold lies between the observed gain and the
possible gain on the other side:

```text
cross_lower = (gain > T && gain - r <= T)
           || (gain <= T && gain + r > T)
```

For an upper bound with strict `gain < T`:

```text
cross_upper = (gain < T && gain + r >= T)
           || (gain >= T && gain - r < T)
```

`r` is the observed `|gain_r - gain_c|` plus a 1-unit guard for integer gain
quantization. This is side-aware: a small `|gain - T|` does **not** by itself
prove a flip if both gains are on the same side.

The triage's literal rule (`d_gain <= r_gain + 1` while the sum branch is
false) was also evaluated directly. It is a sufficient near-boundary warning,
not a flip predicate; both the literal count and the exact side-aware count
are reported. In this corpus both are zero, so the refinement does not change
the verdict.

A second check used the existing policy gate
`gain_tolerance(gain_c) = 8 + |gain_c| / 1000` (maximum 30 in this corpus) as
a conservative error envelope, with a `+1` guard. No window crosses under
that wider envelope either.

## 3. Census method and provenance

### 3.1 Inputs

- 47 committed `rust/src/aac_enc/testdata/rxx_*.txt` files; each contains the
  C `RXX1`/`RXX2` pairs per window.
- Corresponding committed `psy_*.txt` files supply the C TNS state:
  `TNSG` gains, `TNSA` active flags, `TNSO` order, and `TNSC` coefficients.
- Committed `seq_*` rows select long versus short block type.
- `SfbConfig`/`TnsConfig` are constructed exactly as the existing TNS tests
  do: sample rate, channel bitrate 96/112 kbit/s, TNS bitrate 192/112 kbit/s,
  and the committed geometry.

### 3.2 Computation

For each `rxx_*` file and each window:

1. Load `RXX2` and run `analyze_filter(..., HI)`; load `RXX1` and run
   `analyze_filter(..., LO)` only when the block is long and committed HI is
   active, matching production's LO gate.
2. Compare Rust `order` and coefficient vector against the committed C
   `psy_*` values. A mismatch increments `coefficient_mismatches`.
3. Compute `sumSqr` from the quantized coefficients and evaluate the exact
   activation formula above.
4. Compute `residual = |gain_r - gain_c|`, the lower/upper gain distances,
   the lower/upper sum-boundary distances (`d_sum_lo`/`d_sum_hi`), the sum
   branch truth value, and the line-crossing predicate.
5. Compare the recomputed activation with the committed C `active` flags.
6. Print one TSV row and update the summary extrema.

The test is
`aac_enc::tns::tests_gain_gate::d013_activation_margin_census`
(`rust/src/aac_enc/tns.rs:1281-1484`). It is self-contained: it needs only
committed repository files, no C oracle, and no external artifact.

### 3.3 Evidence

- `docs/evidence/aac-enc-tns-d013/census.tsv`: header plus 128 data rows
  (124 HI, 4 LO), with the full field dictionary in the adjacent `README.md`.
- `docs/evidence/aac-enc-tns-d013/summary.txt`: the `CENSUS_SUMMARY` line.
- `docs/evidence/aac-enc-tns-d013/README.md`: method, fields, closest cases,
  reproduction command.

No row was omitted. The report below summarizes the extrema and the cases
needed to interpret them.

## 4. Results for every committed window

### 4.1 Coverage

| Quantity | Count |
|---|---:|
| `rxx_*` files | 47 |
| HI window rows | 124 |
| LO window rows | 4 |
| HI rows with exact sum branch true | 46 |
| HI rows with C/Rust active | 48 |
| HI rows active via gain only (sum false) | 2 |
| LO rows with C/Rust active | 0 |
| Coefficient/order mismatches | 0 |

The two gain-only HI activations are `multitone-stereo` f1 w3 on both
channels: gain 1716 versus C 1715 (residual 1), threshold 1437, margin 279,
`sumSqr=4` equal to its boundary 4 (so the sum branch is false). This is the
closest **active** gain-only decision. It is still 279 units from the
threshold, far outside the 1-unit decision-relevant residual.

### 4.2 Closest HI margins

| Fixture | f | ch | w | gain_r | gain_c | residual | margin | sum / bound | sum true | active C/R |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| multitone-stereo | 1 | 0 | 7 | 1383 | 1383 | 0 | **54** | 1 / 4 | no | 0 / 0 |
| multitone-stereo | 1 | 1 | 7 | 1383 | 1383 | 0 | **54** | 1 / 4 | no | 0 / 0 |
| multitone-stereo | 1 | 0 | 1 | 1361 | 1361 | 0 | 76 | 1 / 4 | no | 0 / 0 |
| multitone-stereo | 1 | 0 | 2 | 1358 | 1358 | 0 | 79 | 1 / 4 | no | 0 / 0 |
| multitone-stereo | 1 | 0 | 4 | 1351 | 1351 | 0 | 86 | 1 / 4 | no | 0 / 0 |
| multitone-stereo | 1 | 0 | 3 | 1716 | 1715 | 1 | 279 | 4 / 4 | no | 1 / 1 |

The channel-1 rows are identical where the fixture is dual-mono. The complete
table is in `census.tsv`; the first row above is the global closest boundary.
`margin` is the distance to the boundary; activation requires crossing it
under the strict comparison, so a +1 step is still needed.

### 4.3 Maximum residual

| Fixture | f | ch | w | gain_r | gain_c | residual | margin | sum / bound | sum true | active C/R |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| onset-stereo | 25 | 0 | 0 | 22165 | 22154 | **11** | 20728 | 51 / 8 | yes | 1 / 1 |
| onset-stereo | 25 | 1 | 0 | 22165 | 22154 | **11** | 20728 | 51 / 8 | yes | 1 / 1 |
| stepdown-stereo | 2 | 0 | 0 | 18751 | 18744 | 7 | 17314 | 48 / 8 | yes | 1 / 1 |
| stepdown-stereo | 2 | 1 | 0 | 18751 | 18744 | 7 | 17314 | 48 / 8 | yes | 1 / 1 |

The maximum residual occurs only where the exact sum branch is already true,
so the gain branch is not decision-relevant there. The largest residual on a
sum-false HI window is 1 (`multitone-stereo` f5 w0, gain 1051 versus 1050,
margin 386; and f1 w3, margin 279).

### 4.4 LO rows

Only four LO rows exist, all long-block HI-active cases; none activates LO.

| Fixture | f | ch | gain_r | gain_c | residual | lower bound | lower margin | upper bound | upper margin | sum / bounds | sum true | active |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|
| onset-stereo | 25 | 0 | 1157 | 1157 | 0 | 1500 | **343** | 80,000 | 78,843 | 4 / 9..110 | no | 0 |
| onset-stereo | 25 | 1 | 1157 | 1157 | 0 | 1500 | **343** | 80,000 | 78,843 | 4 / 9..110 | no | 0 |
| stepdown-stereo | 2 | 0 | 1012 | 1012 | 0 | 1500 | 488 | 80,000 | 78,988 | 0 / 9..110 | no | 0 |
| stepdown-stereo | 2 | 1 | 1012 | 1012 | 0 | 1500 | 488 | 80,000 | 78,988 | 0 / 9..110 | no | 0 |

A LO gain would need to rise by 343 or 488 units to reach the lower bound
(one more to exceed it under the strict comparison); the upper bound is
~79,000 units away and is not the limiting side here. The maximum policy
gate is 30.

## 5. Minimum margin and maximum residual

| Quantity | Value |
|---|---:|
| Maximum observed gain residual (all rows) | **11** |
| Maximum residual on a sum-false row | **1** |
| Maximum policy gate `8 + gain/1000` | **30** |
| Minimum HI gain margin (all rows) | **54** |
| Minimum HI gain margin on a sum-false row | **54** |
| Minimum LO lower-boundary margin | **343** |
| Minimum LO upper-boundary margin | **78,843** |
| Minimum LO lower margin on a sum-false row | **343** |
| Minimum LO upper margin on a sum-false row | **78,843** |

The strongest simple bound is:

```text
54 (minimum decision-relevant margin)
  > 31 (maximum policy gate + 1 guard)
  > 12 (maximum observed residual + 1 guard)
```

So neither the measured residual nor the wider test-policy envelope can
reach the closest decision-relevant boundary.

## 6. Can the residual flip activation within the tested corpus?

**No.**

- `literal_candidates = 0`: no sum-false row satisfies
  `d_gain <= residual + 1`.
- `gate_candidates = 0`: no row crosses under the policy gate plus guard.
- `candidates = 0`: no row crosses under the exact side-aware predicate.
- `actual_flips = 0`: Rust activation matches the committed C activation on
  every row.
- `coefficient_mismatches = 0`: the coefficients and `sumSqr` inputs are
  exact, so the result is not confounded by a coefficient-border difference.

D-013 is therefore closed for the committed corpus. This does not mean the
residual is zero; it means the corpus' nearest decision is more than 50 gain
units away from the boundary, while the largest measured residual is 11 and
the largest decision-relevant residual is 1.

## 7. Counterfactual results

No counterfactual replay was required: no committed window entered the
activation-margin envelope. A replay that substituted the committed C gain
would reproduce the committed activation trivially, because the coefficients
and `sumSqr` inputs are already exact and the observed active flags already
match. Such a replay would not isolate a new variable or change a decision.

No production switch, runtime hook, or feature flag was added. The census
test is test-only.

## 8. Tests run and actual outcomes

| Command / test | Result |
|---|---|
| `cargo test --locked --features aac_enc --lib aac_enc::tns::tests_gain_gate::d013_activation_margin_census -- --nocapture` | **pass**; prints 128 rows and the summary |
| `cargo test --locked --features aac_enc --lib aac_enc::tns` | **5/5 pass** (4 existing + census) |
| `cargo test --locked --features aac_enc --lib` with reference artifacts | **340/340 pass** |
| `cargo test --locked --features aac_enc --lib` without env | **330 pass / 10 documented artifact-dependent failures** |
| `cargo test --locked --features aac_enc --lib aac_enc::production_baseline::production_f32_corpus_baseline_is_unchanged` | pass |
| `cargo test --locked --features aac_enc --doc` | **19/19 pass** |
| `rust/tools/check-licensing.sh` | pass; 67 `rust/src` files carry the notice |
| `rustfmt --edition 2021 --check src/aac_enc/tns.rs` | clean |
| scoped `git diff --check` | pass |

The 10 no-env failures are the same documented artifact-dependent set as
before; the census adds one self-contained pass (329→330 without artifacts;
339→340 with artifacts). The `m55` fallback caveat from the triage still
applies: one artifact-dependent test passes locally because its hard-coded
fallback directory exists.

## 9. Limitations and unresolved evidence gaps

- **Committed corpus only.** The conclusion applies to the 47 `rxx_*` files
  and 128 window/filter rows. It does not generalize to untested signals,
  rates, or RXX vectors.
- **No LO-active case.** All four LO rows are inactive. The census proves the
  LO margins are large on these rows, but it does not exercise an active LO
  decision or an LO filter merge.
- **C-exact RXX only.** The census isolates D-013's same-input gain residual.
  It does not address D-012's different production RXX inputs, which are a
  separate upstream numerical divergence.
- **Gain quantization.** `gain_c` is the committed C `TNSG` integer (×1000),
  and `gain_r` is Rust's floor. The residual therefore includes C and Rust
  quantization/rounding, not only floating-point recurrence error. The
  +1 guard covers this for the crossing check.
- **No PNS.** `predictionGain` also feeds FDK PNS detection, but PNS is
  disabled (`NO_NOISE_PNS`), so this census does not bound a PNS-enabled
  configuration.
- **No perceptual claim.** This is a discrete-decision census, not a quality
  or listening result.
- **No new C oracle.** The census uses committed vectors only. It does not
  re-run the C encoder or replace any reference artifact.

## 10. Files added or modified

| File | Change |
|---|---|
| `rust/src/aac_enc/tns.rs` | Added the test-only `d013_activation_margin_census`; no production behavior changed. |
| `docs/aac-enc-tns-d013-activation-margin-census.md` | This report. |
| `docs/evidence/aac-enc-tns-d013/README.md` | Method, field dictionary, closest cases, reproduction. |
| `docs/evidence/aac-enc-tns-d013/census.tsv` | 128-row committed-vector census. |
| `docs/evidence/aac-enc-tns-d013/summary.txt` | Census summary line. |
| `docs/README.md` | Index entry. |
| `docs/aac-enc-numerical-divergence-triage.md` | D-013 status updated to closed by census; pre-census ranking retained as history. |

No production encoder behavior was changed. No commit or push was performed.
