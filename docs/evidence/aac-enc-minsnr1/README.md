# MINSNR1 oracle investigation evidence

Diagnostic evidence for `docs/aac-enc-minsnr1-oracle-investigation.md`.
Measurements only; no production MINSNR1, TNS, block-switch, or MDCT behavior
was changed.

## Method

- Fixture PCM: committed 48 kHz / 44.1 kHz corpus in
  `rust/src/aac_enc/testdata/`.
- C reference vectors: `qc1_<name>_f<F>.txt` (S1 inputs `SENL`/`PENLD`/`MSD`)
  and `qc2_<name>_f<F>.txt` (`MINSNR0`/`SPREAD0`/`THRV`/`MINSNR1`/`SPREAD1`),
  produced by the pinned FDK `aac-enc` oracle.
- Live path: `step_to_frame` runs the production `BlockSwitch` →
  transform → psychoacoustics → S1 → S2 chain. A test-only `S2Cap` captures the
  S2 inputs and result after the production call.
- Counterfactual replay: `adapt_min_snr` + `init_avoid_hole_flag_mono` /
  `init_avoid_hole_flag_ms` (the production S2 functions) are run with the
  captured geometry and `AtsElement::new(0)`. The test verifies that this
  reproduces the production `MINSNR1` exactly before substituting C vectors.
- Only active bands are compared (`i % sfbPerGroup < maxSfbPerGroup`);
  short-block oracle vectors contain stale/pad entries outside that range.
- “out” means outside the repository's existing energy-domain Ld envelope
  (`|ΔE| ≤ 32 + E/500`, `assert_ld_close`); “max” is the largest raw Q31 Ld
  difference. A large raw Ld difference at a tiny energy can still be inside
  the envelope.

## Input differences

All values are Q31 Ld for `MINSNR0`/`PENLD`; `SENL` is the stored linear
energy. `Δ` is Rust minus C over active bands.

| Fixture | f | ch | max ΔMINSNR0 | max ΔSENL | max ΔPENLD | max ΔSPREAD0 | MS mask Δ |
|---|---:|---:|---:|---:|---:|---:|---:|
| onset-stereo | 25 | 0 | 0 | 1,392 | 47,151,681 | 672 | 0 |
| onset-stereo | 26 | 0 | 0 | 548 | 65,657,763 | 320 | 1 (band 16) |
| sine-stereo | 5 | 0 | 0 | 698 | 101,832,433 | 320 | 1 (band 16) |
| sine-stereo-44100 | 5 | 0 | 0 | 49,420 | 67,908,528 | 24,704 | 0 |
| stepdown-stereo | 2 | 0 | 0 | 470 | 51,321,805 | 192 | 0 |
| quiet-sine-stereo | 5 | 0 | 0 | 21,850 | 14,363,723 | 9,984 | 0 |
| quiet-sine-stereo | 5 | 1 | 0 | 5 | 5,438,150 | 9,984 | 0 |
| click-mono | 11 | 0 | 0 | 256 | 67,789,194 | 256 | n/a |
| impulse-mono | 5 | 0 | 0 | 1,056 | 21,772 | 1,056 | n/a |

`MINSNR0` is exact for every studied channel. `SPREAD0` differs, but it is not
causal: substituting only the C energy vectors (with Rust `SPREAD0`, `MINSNR0`,
and mask) already reproduces C `MINSNR1` wherever the mask agrees.

## Counterfactual outcomes

Cells are `bands out / max raw |ΔMINSNR1|`. `Cenergy` = C `SENL` + C `PENLD`;
`Csenl` = only C `SENL`; `Cpenld` = only C `PENLD`; `Cenergy+mask` = C energy
plus C MS mask.

| Fixture | f | ch | prod | Cenergy | Csenl | Cpenld | Cenergy+mask | THRV out/max | ahFlag Δ |
|---|---:|---:|---|---|---|---|---|---|---:|
| onset-stereo | 25 | 0 | 1 / 163,913 | 0 / 0 | 1 / 163,913 | 0 / 88,537 | 0 / 0 | 0 / 53,132 | 0 |
| onset-stereo | 26 | 0 | 1 / 23,297,022 | 1 / 23,297,022 | 1 / 23,297,022 | 1 / 23,297,022 | 0 / 0 | 0 / 4,159,422 | 0 |
| sine-stereo | 5 | 0 | 1 / 23,297,022 | 1 / 23,297,022 | 1 / 23,297,022 | 1 / 23,297,022 | 0 / 0 | 0 / 4,125,678 | 0 |
| sine-stereo-44100 | 5 | 0 | 1 / 296,901 | 0 / 0 | 1 / 296,901 | 0 / 0 | 0 / 0 | 0 / 6,253,969 | 0 |
| stepdown-stereo | 2 | 0 | 1 / 246,409 | 0 / 0 | 1 / 246,409 | 0 / 4,804 | 0 / 0 | 0 / 0 | 0 |
| quiet-sine-stereo | 5 | 0 | 2 / 10,452,206 | 0 / 0 | 3 / 10,452,206 | 2 / 1,540,830 | 0 / 0 | 0 / 308,259 | 0 |
| quiet-sine-stereo | 5 | 1 | 5 / 1,944,944 | 0 / 0 | 4 / 1,087,523 | 5 / 1,669,435 | 0 / 0 | 0 / 557 | 0 |
| click-mono | 11 | 0 | 0 / 19,968 | 0 / 0 | 0 / 19,968 | 0 / 0 | 0 / 0 | 0 / 4,167,215 | 0 |
| impulse-mono | 5 | 0 | 0 / 12,267 | 0 / 0 | 0 / 14,909 | 0 / 12,758 | 0 / 0 | 0 / 77,236 | 0 |

Reading the table:

- `Cenergy` closes every non-mask case, including the byte-identical
  `quiet-sine` control.
- For `onset f26` and `sine f5`, C energy alone is insufficient; adding the C
  MS mask closes the gap exactly. The mask differs only at band 16, the
  already-recorded MS decision residue (`phase1e_sine_f5_band16_residue`).
- On the three `PENLD`-attributed cases, C `PENLD` alone closes the envelope
  while C `SENL` alone does not. `quiet-sine` needs both energy lanes.
- `THRV` remains inside the existing envelope and the initial avoid-hole flags
  agree on every studied active band, so no threshold/flag consequence is
  demonstrated on this set.

## Per-band evidence

Representative divergent bands (C | Rust):

- `onset-stereo f25 ch0 band 22`: `MINSNR0` equal; `SENL` 17 | 17; `PENLD`
  −901,361,351 | −901,525,264; `MINSNR1` −21,874,525 | −21,710,612.
- `onset-stereo f26 ch0 band 16`: `SENL` 0 | 0; `MS mask` 1 | 0; C side
  `PENLD` is the −1.0 sentinel while Rust is −1,107,953,108; `MINSNR1`
  −10,802,114 | −34,099,136. Adding the C mask makes the result exact.
- `quiet-sine-stereo f5 ch0 band 2`: `SENL` 2 | 3; `PENLD` −989,994,832 |
  −975,631,109; `MINSNR1` −59,851,282 | −70,303,488. A one-LSB stored-energy
  difference at a near-zero band changes the Ld average enough to move the
  adapted value.
- `stepdown-stereo f2 ch0 band 22`: `SENL` 13 | 13; `PENLD` −912,759,415 |
  −913,005,824; `MINSNR1` −22,150,828 | −21,904,419. C `PENLD` alone is
  inside tolerance, so this is purely the documented Ld float lane.

## Logs

- `minsnr1-divergence.log`: full counterfactual table and divergent-band
  details from the permanent test.
- `threshold-vbr-tests.log`: focused S2 suite output.
- `local-impact.md`: local ADTS sizes and delay-aligned decoded SNR.

## Classification

`MINSNR1` is not a distinct FDK algorithm or constant; it is the oracle label
for `qcOutChannel->sfbMinSnrLdData` after `FDKaacEnc_adaptMinSnr` and
`FDKaacEnc_initAvoidHoleFlag`. Given C's S1 energy vectors and mask, the Rust
S2 implementation reproduces C's `MINSNR1` exactly. The production differences
are therefore **intentional numerical divergences** in the upstream
stored/Ld energy lane (plus the already-known MS band-16 decision residue on
two frames), not a MINSNR1 porting defect.
