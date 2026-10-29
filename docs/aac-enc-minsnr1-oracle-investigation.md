# MINSNR1 oracle investigation

Date: 2026-10-27. Status: **diagnostic complete; no production change; no
commit or push.**

## 1. Executive summary

`MINSNR1` is not a named FDK constant, function, or bitstream field. It is the
oracle's label for `qcOutChannel->sfbMinSnrLdData` after the per-frame
`FDKaacEnc_adaptMinSnr` and `FDKaacEnc_initAvoidHoleFlag` steps. The value is a
Q31 log-domain minimum-SNR requirement used to cap the VBR threshold reduction;
it reaches quantization only indirectly, through the adjusted threshold
(`THRV`).

The Rust S2 implementation is a faithful port. With the committed C S1 energy
vectors and MS mask substituted, Rust reproduces the C `MINSNR1`, `SPREAD1`,
and `THRV` exactly on all 19 committed S2 cases
(`threshold_vbr::tests::s2_full_matches_oracle_with_injected_chaos`). On the
representative divergent frames, replaying the production S2 functions on C's
`SENL`/`PENLD` also reproduces C's `MINSNR1` exactly, except where the already
known MS band-16 decision also differs.

The production differences are therefore **upstream numerical residue**, not a
MINSNR1 porting defect:

- `MINSNR0` is exact on every studied channel.
- `SPREAD0` differs slightly but is not causal; substituting only C's energy
  vectors (with Rust `SPREAD0`/`MINSNR0`) is sufficient wherever the MS mask
  agrees.
- Three frames (`onset f25`, `sine-441 f5`, `stepdown f2`) are closed by C's
  `PENLD` alone; C's `SENL` alone is not sufficient.
- `quiet-sine f5` is sensitive to both stored linear energy and the Ld lane.
- `onset f26` and `sine f5` additionally require the C MS mask at band 16,
  which is the already-recorded MS decision residue
  (`phase1e_sine_f5_band16_residue`), not an S2 formula difference.

**Classification: intentional numerical divergence.** No verified porting
defect, intentional algorithmic difference, or harness discrepancy was found
in the MINSNR1 path. The only algorithmic difference is the documented
configuration strategy: Rust loads `MINSNR0` from committed C snapshots at the
validated VBR-5 points instead of re-deriving it for arbitrary rates; at those
points the values are exact.

Downstream, `THRV` remains inside the repository's existing energy-domain
envelope and the initial avoid-hole flags agree on all studied active bands.
`quiet-sine-stereo f5` is byte-identical in payload and sample-identical after
decoding despite seven divergent MINSNR1 bands. No coding or quality change
attributable to MINSNR1 was demonstrated. **Recommendation: no production
MINSNR1 change.**

## 2. What MINSNR1 is in FDK

`sfbMinSnrLdData` is a per-scalefactor-band minimum signal-to-noise
requirement in FDK's “Ld” domain:

```text
Ld(x) = log2(x) / 64          (Q31 fixed point)
value = Ld * 2^31             (stored i32)
inverse energy: E = 2^(value / 2^31 * 64 + 31)
```

The initial array is produced by `FDKaacEnc_initMinSnr`
(`cpp/libAACenc/src/psy_configuration.cpp:421-532`) from the channel bitrate,
sample rate, block type, and Bark/sfb geometry. Its SNR ratio is clamped to the
init formula's -25 dB/-1 dB limits before conversion; subsequent adaptation
moves the stored value between the peak/valley floors and the
`log2(0.8)/64` ceiling. The per-frame adaptation then:

1. reduces the requirement for bands whose energy is low relative to the
   frame-average (`FDKaacEnc_adaptMinSnr`,
   `cpp/libAACenc/src/adj_thr.cpp:466-533`);
2. raises/lowers it for local spectral peaks/valleys and adapts mid/side
   requirements on MS bands (`FDKaacEnc_initAvoidHoleFlag`,
   `cpp/libAACenc/src/adj_thr.cpp:539-701`);
3. caps the VBR threshold reduction with the result
   (`FDKaacEnc_reduceThresholdsVBR`,
   `cpp/libAACenc/src/adj_thr.cpp:1108-1312`).

The constants are log-domain ratios, not dB or raw Q31 energies:

| Constant | Value | Meaning |
|---|---:|---|
| `SnrLdMin1` | `0xfcad0ddf` | peak floor, long blocks (`log2(0.316)/64`) |
| `SnrLdMin2` | `0x0351e1a2` | valley drift bound (`log2(3.16)/64`) |
| `SnrLdFac` | `0xff5b2c3e` | `log2(0.8)/64`; adaptation ceiling/1 dB reference |
| `SnrLdMin3` | `0xfe000000` | peak floor, non-long blocks (`log2(0.5)/64`) |
| `SnrLdMin4` | `0x02000000` | valley trigger (`log2(2)/64`) |
| `SnrLdMin5` | `0xfc000000` | MS threshold offset (`log2(0.25)/64`) |
| `minSnrLimitLD64` | `0xff5b2c3e` | `log2(0.8)/64`, adaptation floor |

`MINSNR0` is the oracle's pre-adaptation array. `MINSNR1` is the
post-adaptation/post-avoid-hole array. The names are diagnostic tags; FDK has
only the `sfbMinSnrLdData` field.

### Configuration and conditions

- `AdjThrInit` (`adj_thr.cpp:2533-2581`) sets `maxRed = 0.00390625`
  (`0x00800000`), `startRatio = 0.05190512648` (`0x06a4d3c0`),
  `redRatioFac = -0.375` (`0xd0000000`), and `redOffs = 0.021484375`
  (`0x02c00000`) for the element.
- `ahParam.modifyMinSnr` is enabled when the per-channel bitrate is at least
  20 kbit/s (`adj_thr.cpp:2559-2567`). VBR-5 (96/112 kbit/s per channel) is
  above that threshold, so peak/valley adaptation is active.
- The VBR path uses `FDKaacEnc_AdaptThresholdsVBR` (`adj_thr.cpp:2160-2194`).
  The CBR/PE-correction path's `FDKaacEnc_reduceMinSnr`
  (`adj_thr.cpp:1514-…`) is not part of the VBR-5 path.
- Initial `minSnr` depends on bitrate, sample rate, and block type. Rust uses
  committed `MINSNR` snapshots for the validated 44.1/48 kHz × 96/112 kbit/s
  points (`psy_config.rs:63,141`), which is exact there but not a general
  from-formula derivation.
- `MINSNR1` is not directly written to the bitstream. It influences
  `THRV`, and `THRV` is consumed by S4 scalefactor estimation. It is therefore
  not conditional on the afterburner; the afterburner is downstream of the
  threshold it helps shape.

## 3. Source locations and call paths

### FDK C

| Item | Location |
|---|---|
| `SnrLdMin1..5`, `SnrLdFac` | `cpp/libAACenc/src/adj_thr.cpp:119-136` |
| `FDKaacEnc_initMinSnr` | `cpp/libAACenc/src/psy_configuration.cpp:421-532` |
| `FDKaacEnc_InitPsyConfiguration` / minSnr call | `cpp/libAACenc/src/psy_configuration.cpp:534-627` |
| Long `MINSNR0` copy into `psyOutChannel` | `cpp/libAACenc/src/psy_main.cpp:1112-1114` |
| Short grouped `MINSNR0` | `cpp/libAACenc/src/psy_main.cpp:1052-1053`; `grp_data.cpp:124,176` |
| `psyOutChannel`/`qcOutChannel` minSnr alias | `cpp/libAACenc/src/aacenc.cpp:807` |
| `FDKaacEnc_adaptMinSnr` | `cpp/libAACenc/src/adj_thr.cpp:466-533` |
| `FDKaacEnc_initAvoidHoleFlag` | `cpp/libAACenc/src/adj_thr.cpp:539-701` |
| VBR calls (`adaptMinSnr`, `initAvoidHoleFlag`) | `cpp/libAACenc/src/adj_thr.cpp:2177-2183` |
| Non-VBR calls (same functions) | `cpp/libAACenc/src/adj_thr.cpp:1936-1944` |
| `FDKaacEnc_reduceThresholdsVBR` | `cpp/libAACenc/src/adj_thr.cpp:1108-1312` |
| MINSNR1 cap, short path | `cpp/libAACenc/src/adj_thr.cpp:1025-1035` |
| MINSNR1 cap, long path | `cpp/libAACenc/src/adj_thr.cpp:1289-1298` |
| CBR-only `reduceMinSnr` | `cpp/libAACenc/src/adj_thr.cpp:1514-…` |
| S2 oracle dump (`MINSNR0`/`MINSNR1`) | `tools/enc-oracle/enc_oracle.cpp:1426-1532` |

### Rust

| Item | Location |
|---|---|
| `SnrLdMin*` / `MSA_*` constants | `rust/src/aac_enc/threshold_vbr.rs:176-201` |
| `adapt_min_snr` | `rust/src/aac_enc/threshold_vbr.rs:276-323` |
| Avoid-hole mono core | `rust/src/aac_enc/threshold_vbr.rs:349-428` |
| Avoid-hole MS section | `rust/src/aac_enc/threshold_vbr.rs:433-482` |
| `reduce_thresholds_vbr` | `rust/src/aac_enc/threshold_vbr.rs:556-892` |
| S2 orchestration / `AdaptResult` | `rust/src/aac_enc/threshold_vbr.rs:593-728` |
| MINSNR1 cap reads | `rust/src/aac_enc/threshold_vbr.rs:837,870-876` |
| `MINSNR0` config snapshots | `rust/src/aac_enc/psy_config.rs:63,141` |
| Short grouped `min_snr` | `rust/src/aac_enc/psy.rs:1354-1393` |
| S2 input construction | `rust/src/aac_enc/stream_driver.rs:1179-1223` |
| S2 call / test-only capture | `rust/src/aac_enc/stream_driver.rs:1224-1248` |
| S4 consumes `THRV` | `rust/src/aac_enc/stream_driver.rs:1250-1268` |
| Committed S2 vector loader | `rust/src/aac_enc/testdata.rs:1166-1290` |

The C and Rust formulas match term for term:

```text
avgEnLD64 = Ld(sum(sfbEnergy >> 6)) + 0.09375 - Ld(nSfb)
dbRatio    = avgEnLD64 - sfbEnergyLdData
red        = max(redOffs + fMult(redRatioFac, dbRatio), maxRed)
red        = min((fMult(minSnr, red)) << 6, minSnrLimit)
update     = startRatio < dbRatio
```

The peak/valley branches (`SnrLdFac + max(avgEnLd − enLd, SnrLdMin1/3 −
SnrLdFac)`, the `SnrLdMin4` valley trigger, and the `SnrLdMin2` bound) and the
stereo MS section (`SnrLdMin5 >> 1` overflow test, `fixMax`/`fixMin` clamps,
and `0.9` spread cross-tweak) also match. Constants, operation order, and
`i32` Q31 representation are identical.

One low-risk observation: C uses signed 32-bit arithmetic that can overflow in
extremal cases; Rust uses explicit `wrapping_*` in most places but plain `-` in
the peak/valley expressions. No corpus case overflows, and the exact C-input
replay is unaffected. This is a latent robustness note, not an observed
divergence.

## 4. Baseline tests and oracle evidence

Before this investigation:

| Check | Result |
|---|---|
| `cargo test --locked --features aac_enc --lib aac_enc::threshold_vbr` | 19/19 pass |
| `s2_full_matches_oracle_with_injected_chaos` | exact `MINSNR1`, `THRV`, `SPREAD1` on 19 committed S2 cases |
| Full suite with reference artifacts | 337/337 pass |
| Full suite without external artifacts | 327 pass / 10 documented reference-artifact failures |

The existing S2 test already proved the formula is exact when fed C's S1
inputs. The M5.26 audit's MINSNR1 flags were therefore a value-level open
question about the *inputs*, not evidence of a formula mismatch. The permanent
test covers all six M5.26-flagged frames (`onset f25/f26`, `sine f5`,
`sine-441 f5`, `stepdown f2`, `quiet-sine f5`) plus `click f11` and
`impulse f5` controls.

Evidence collected for this report:

- `docs/evidence/aac-enc-minsnr1/minsnr1-divergence.log`: counterfactual
  table and divergent-band details.
- `docs/evidence/aac-enc-minsnr1/threshold-vbr-tests.log`: focused S2 run.
- `docs/evidence/aac-enc-minsnr1/local-impact.md`: frame sizes and decoded
  SNR.

## 5. Numerical comparison and earliest divergence

The permanent test runs the production path through each frame, captures the
S2 inputs, and replays the production S2 functions four ways:

1. all production inputs (`prod`);
2. C `SENL` + C `PENLD`, Rust `MINSNR0`/`SPREAD0`/mask (`Cenergy`);
3. only C `SENL` (`Csenl`);
4. only C `PENLD` (`Cpenld`);
5. C energy + C mask (`Cenergy+mask`).

“Out” counts active bands outside the existing energy-domain Ld envelope;
“max” is the largest raw Q31 difference.

| Fixture | f | ch | max ΔMINSNR0 | max ΔSENL | max ΔPENLD | mask Δ | prod out/max | Cenergy out/max | Cpenld out/max | Cenergy+mask out/max |
|---|---:|---:|---:|---:|---:|---:|---|---|---|---|
| onset-stereo | 25 | 0 | 0 | 1,392 | 47,151,681 | 0 | 1 / 163,913 | 0 / 0 | 0 / 88,537 | 0 / 0 |
| onset-stereo | 26 | 0 | 0 | 548 | 65,657,763 | 1 (b16) | 1 / 23,297,022 | 1 / 23,297,022 | 1 / 23,297,022 | 0 / 0 |
| sine-stereo | 5 | 0 | 0 | 698 | 101,832,433 | 1 (b16) | 1 / 23,297,022 | 1 / 23,297,022 | 1 / 23,297,022 | 0 / 0 |
| sine-stereo-44100 | 5 | 0 | 0 | 49,420 | 67,908,528 | 0 | 1 / 296,901 | 0 / 0 | 0 / 0 | 0 / 0 |
| stepdown-stereo | 2 | 0 | 0 | 470 | 51,321,805 | 0 | 1 / 246,409 | 0 / 0 | 0 / 4,804 | 0 / 0 |
| quiet-sine-stereo | 5 | 0 | 0 | 21,850 | 14,363,723 | 0 | 2 / 10,452,206 | 0 / 0 | 2 / 1,540,830 | 0 / 0 |
| quiet-sine-stereo | 5 | 1 | 0 | 5 | 5,438,150 | 0 | 5 / 1,944,944 | 0 / 0 | 5 / 1,669,435 | 0 / 0 |
| click-mono | 11 | 0 | 0 | 256 | 67,789,194 | n/a | 0 / 19,968 | 0 / 0 | 0 / 0 | 0 / 0 |
| impulse-mono | 5 | 0 | 0 | 1,056 | 21,772 | n/a | 0 / 12,267 | 0 / 0 | 0 / 12,758 | 0 / 0 |

`Csenl` is omitted from the compact table; it is the non-causal arm for the
three `PENLD`-attributed cases (1 out for `onset f25`, 1 for `sine-441 f5`,
1 for `stepdown f2`) and contributes for `quiet-sine`.

### Earliest causal divergence, per class

1. **`PENLD`/Ld lane (onset f25, sine-441 f5, stepdown f2).**
   The stored linear energy is equal or nearly equal at the divergent band, but
   the Ld value differs. For `onset f25 ch0 band 22`, `SENL` is 17 on both
   sides while `PENLD` is −901,361,351 (C) vs −901,525,264 (Rust). Substituting
   only C `PENLD` brings `MINSNR1` inside the envelope; C `SENL` alone does not.
   This is the documented f32-MDCT/Q31 energy-Ld float lane, already visible in
   the M5.26 Tier-B tests.
2. **Stored and Ld lanes (quiet-sine f5).**
   At band 2, C `SENL` is 2 and Rust is 3, which changes the local average and
   the Ld conversion; both the linear and Ld substitutions matter. C energy
   closes the gap exactly. This is the near-zero stored-energy boundary, not an
   adaptation-formula difference. The byte-identical payload on this frame is
   the strongest control in the set.
3. **MS mask (onset f26, sine f5).**
   At band 16 the C MS mask is 1 and Rust is 0. The already-recorded MS test
   `phase1e_sine_f5_band16_residue` shows the stored operands are bit-identical
   and the Ld residue shifts the strict margin by exactly −2,539,383 (C margin
   2,539,383; Rust 0), flipping the bit. Once the C mask is supplied,
   `Cenergy+mask` reproduces C `MINSNR1` exactly. MINSNR1 is downstream of the
   MS decision here, not its cause.
4. **`MINSNR0`.** Exact on every active band of every studied frame. The
   committed config snapshot strategy is not a source of the divergence at
   these operating points.
5. **`SPREAD0`.** Differs slightly (up to 24,704 Q31), but is not causal:
   `Cenergy` with Rust `SPREAD0` is exact wherever the mask agrees.

## 6. Downstream impact and limitations

### `THRV` and avoid-hole flags

`MINSNR1` is consumed by `reduceThresholdsVBR` when deciding whether a reduced
threshold would create a hole. In the studied frames:

- `THRV` is inside the existing energy-domain envelope for every active band
  (`THRV out = 0`); raw Q31 differences up to ~6.25 M occur at very low-energy
  bands but invert to sub-tolerance energy differences.
- The initial avoid-hole flags agree on every active band (`ahFlag Δ = 0`).
  Post-reduction `AH_ACTIVE` transitions are not separately dumped by the C
  oracle, so this is not a complete flag comparison.
- `quiet-sine-stereo f5`, the byte-identical control, has seven divergent
  MINSNR1 bands but identical `THRV`, flags, payload, and decoded samples.

### Encoded size

| Fixture | Frame | C bytes | Rust bytes | Delta |
|---|---:|---:|---:|---:|
| quiet-sine-stereo | 5 | 81 | 81 | 0 |
| onset-stereo | 25 | 119 | 119 | 0 |
| onset-stereo | 26 | 58 | 58 | 0 |
| sine-stereo | 5 | 58 | 59 | +1 |
| sine-stereo-44100 | 5 | 54 | 54 | 0 |
| stepdown-stereo | 2 | 129 | 130 | +1 |
| click-mono | 11 | 49 | 49 | 0 |
| impulse-mono | 5 | 178 | 176 | −2 |

The ±1/2-byte changes are confounded by the known MS/TNS/energy divergences;
they cannot be attributed to MINSNR1 alone. The quiet-sine frame is the clean
no-op.

### Decoded output

Delay-aligned (1600 samples) decoded SNR around the affected frames is mixed:
`quiet-sine f5` is sample-identical (max diff 0); `onset f26` and `sine f5`
show Rust SNR above C at the 1024-sample window, while `stepdown f2` is below
at 1024 and above at 4096. These frames also carry TNS/MS differences. The
measurements are descriptive waveform comparisons, not perceptual evidence.
Full numbers: `docs/evidence/aac-enc-minsnr1/local-impact.md`.

### Limitations

- FDK does not dump the intermediate array after `adaptMinSnr` but before
  `initAvoidHoleFlag`; the composite S2 stage is proven exact, not each
  sub-stage separately on production inputs.
- The C oracle does not dump post-reduction `ahFlag`; only initial flags are
  compared.
- The test covers the six M5.26-flagged frames plus two controls, not the full
  corpus.
- No PEAQ/ViSQOL/listening test exists. Successful decoding and local SNR do
  not establish perceptual equivalence.
- Rust's `MINSNR0` is a committed snapshot at four operating points, not a
  general `FDKaacEnc_initMinSnr` derivation; other rates/bitrates remain out of
  scope.
- The signed-overflow robustness note in §3 is not exercised by the corpus.

## 7. Regression coverage

Added in `rust/src/aac_enc/stream_driver.rs`:

`aac_enc::stream_driver::phase1_tests::minsnr1_divergence_is_upstream_of_s2`

It runs the production path for eight frames, captures S2 inputs, and asserts:

- the replay reproduces the production `MINSNR1` exactly;
- C inputs reproduce C `MINSNR1` exactly;
- `MINSNR0` is exact;
- C energy alone reproduces C `MINSNR1` when the mask agrees;
- C energy plus the C mask is required for the two MS band-16 cases;
- C `PENLD` alone closes the three Ld-lane cases while C `SENL` alone does not;
- `THRV` stays inside the documented envelope and initial avoid-hole flags
  agree;
- at least one real divergence and one in-envelope control are present.

The test uses only committed `qc1`/`qc2` vectors and fixture PCM; no external
oracle artifacts are required. Existing `s2_full_matches_oracle_with_injected_chaos`
continues to pin the exact S2 formula on all 19 committed cases.

Tests after the change:

| Command | Result |
|---|---|
| `cargo test --locked --features aac_enc --lib aac_enc::threshold_vbr` | 19/19 pass |
| `cargo test --locked --features aac_enc --lib aac_enc::stream_driver::phase1_tests::minsnr1_divergence_is_upstream_of_s2` | pass |
| Full suite with reference artifacts | **338/338 pass** |
| Full suite without external artifacts | 328 pass / 10 documented reference-artifact failures |
| `cargo test --locked -p aac-convert` | pass |
| `cargo test --locked --features aac_enc --doc` | 19/19 pass |
| `rustfmt --check` | changed regions clean; only the two pre-existing unrelated formatting diffs remain |
| `rust/tools/check-licensing.sh` | pass |

## 8. Recommendation and remaining uncertainties

**Do not change production MINSNR1 behavior.** The adaptation and avoid-hole
formulas are faithful; the observed differences are upstream f32-MDCT/Q31
stored/Ld energy residue, plus the already-known MS band-16 decision residue on
two frames. A MINSNR1-only patch would compensate for those upstream
differences and could change unrelated decisions without a demonstrated
quality benefit.

Recommended actions:

1. Keep the new attribution test and the exact C-input S2 test.
2. Record the MINSNR1 value envelope as an **intentional numerical
   divergence**, not an open porting defect.
3. If closer MINSNR1 parity is ever required, improve the upstream stored/Ld
   energy accuracy (fixed-point/Q31 MDCT emulation or a more faithful energy
   lane), then remeasure. Do not special-case the S2 adaptation.
4. Treat the `quiet-sine f5` control as the regression canary: a future change
   that makes its payload differ should be investigated, but matching C
   MINSNR1 alone is not evidence of better quality.

Remaining uncertainties are listed in §6. The highest-value follow-up, if
needed, is a C-side dump of the post-`adaptMinSnr`/pre-avoid-hole array and the
post-reduction `ahFlag`, which would split the composite S2 exactness into its
two sub-stages.

## 9. Files changed

| File | Change |
|---|---|
| `rust/src/aac_enc/stream_driver.rs` | Test-only S2 capture and the new MINSNR1 attribution regression test. No production behavior changed. |
| `docs/aac-enc-minsnr1-oracle-investigation.md` | This report. |
| `docs/evidence/aac-enc-minsnr1/README.md` | Method, tables, classification. |
| `docs/evidence/aac-enc-minsnr1/minsnr1-divergence.log` | Counterfactual and per-band diagnostic log. |
| `docs/evidence/aac-enc-minsnr1/threshold-vbr-tests.log` | Focused S2 test output. |
| `docs/evidence/aac-enc-minsnr1/local-impact.md` | Local sizes and decoded SNR. |
| `docs/README.md` | Index entry. |
| `docs/aac-enc-mdct-f64-closeout.md` | MINSNR1 open-item status update. |

No production MINSNR1, TNS, block-switch, MDCT, or other encoder behavior was
changed. Unrelated concurrent changes in the worktree were left untouched:
`docs/aac-enc-m5b-objective-quality-baseline.md`,
`docs/aac-enc-quality-divergence-audit-m5.25.md`, `rust/src/aac_enc.rs`,
`rust/src/aac_enc/block_switch.rs`, `rust/src/aac_enc/tns.rs`, the existing
f64/block-switch/TNS reports and evidence, `rust/src/aac_enc/production_baseline.rs`,
and `tools/aac-enc-metrics/challenge/listening-review-worksheet.md`. No commit
or push was performed.
