# Rust AAC-LC VBR-5 Encoder — First-Divergence Stage Attribution (M5.26)

Diagnostic milestone **M5.26** (2026‑10‑16). This report identifies, for the 11 divergent
oracle frames and the 9 negative controls frozen by the M5.25 audit, the earliest **existing**
comparison gate at which the Rust production path leaves the committed C-oracle ground truth.
**No encoder behaviour was changed, no divergence was fixed, and no gate or tolerance was
altered.** All instrumentation lived in a temporary copy of the worktree outside the repository.

*Revision M5.26a (2026‑10‑17): citation and evidence-class corrections only (gate sources,
MDCT/ring tolerance status, instrumentation bounds, gate-versus-script labelling, and the
`stress‑stereo f5` sequence/geometry terms). No finding, classification, frame identity, blip
exclusion or conclusion was changed.*

**Code under audit**

| Item | Value |
|---|---|
| Repository | `/Users/jeroen/Documents/VSCode/fdk-aac/fdk-aac` (worktree, not `HEAD`) |
| `HEAD` | `e4258d2a5b55581f5f2b27aa6c1a369bb2503e43` — does **not** contain the harness |
| Rust path | `rust/src/aac_enc/**` (10 tracked-modified files + untracked `stream_driver.rs`, `phase1b.rs`) |
| C reference | `cpp/libAACenc/src/**` (production, unmodified) |
| Reference C binary | `…/T/opencode/phase0-20261015/bin/aac-enc`, md5 `850a8e6ed56641f453dc12b3c83946f5` |
| Rust build | default `release`/`test` profile, rustc/cargo 1.99.0 |
| Fixture manifest | `201b292db60d6bebb59a18891ce8d9e4ffda6f6f57b2f303f747107d87eb2998` |
| Preservation backup | `/Users/jeroen/backups/fdk-aac-m5.23-worktree-20261016T193606Z` (untouched) |

---

## 1. Executive summary

1. **The built-in controls constrain the method.** For all **eight non-blip negative control
   frames** (edge f4, noise f5, onset f22, onset f23, quiet‑sine f5, silence f5, stress f5,
   tonal‑noise f5) every ladder item that a committed reference can decide is decided in the
   encoder's favour, and the payload is byte-identical to the real C output. No control frame
   was assumed to pass: each was measured. **One caveat belongs here:** `stress‑stereo f5` has
   no committed sequence reference, so its `seq`/`geo` terms are reported **inconclusive**
   (§4.4, §8.2); its payload-identity result is independently supported and stands.
   [derived‑here, §4.4, §8]

2. **The oracle "blip" reference defect is independently reproduced and bounded.** Comparing
   `qc6 S6FINAL total + 56` with the real C frame length reproduces **exactly the four** frames
   of our set that the M4 record calls SHORT_WINDOW replay blips (D‑021): `onset‑stereo f24`,
   `onset‑stereo‑44100 f22`, `sine‑stereo f1`, `multitone‑stereo f1`. For those four frames the
   oracle S1→S8 replay is **not** a faithful reference for the real encoder, so the S4/S5/S6/S8
   comparison is reported **inconclusive**, and the value deviations that occur there
   (`SENL`, `THRV`, `SPREAD1`, and the set's only `SPEC` failure — at `onset f24`) are
   attributed to the same reference defect — they coincide exactly with the blip set and occur
   on no non‑blip frame. [derived‑here, §4.3, §8.3]

3. **No value-level (S1/S2) tolerance is exceeded by any non-blip frame, on the counters that
   can be compared.** Across the 8 non-blip positives and 8 non-blip controls, the M3 tolerance
   definitions (`|Δ| ≤ 32 + |w|/500` stored; `32.0 + e_want/500.0` energy-domain for Ld) hold
   for `SENL`, `PTHLD`, `PENLD` (LONG), `SPEC` (sum-like channels), `THRV` and `SPREAD1` on
   **every** frame. These are this milestone's **script** comparisons against committed vectors
   (§4.4), except on the few frames where the repository asserts the same quantity. **One
   exception is flagged: `MINSNR1`**, which exceeds the Ld envelope on 6 of the 16 non-blip
   frames (onset f25/f26, sine f5, sine‑441 f5, stepdown f2, and the byte-identical control
   `quiet‑sine‑stereo f5`; gaps 1.7× to 190× the tolerance); it is therefore **not usable as an
   attribution term** and is listed as an unresolved cause (§11). [derived‑here, §8, §10]

4. **The earliest failing exact comparisons are all of the documented numeric class or of
   already-pinned divergence classes**, and each is traced to a specific stage (the evidence
   class behind each term is per §4.4):
   * **TNS decision (M3 psychoacoustics)** — `onset f25` and `stepdown f2` fail the `STATIC`
     gate (200 vs 208 and 208 vs 200 bits) because the **pinned D‑012 active-filter TNS
     divergence** at `w0` changes the TNS side-info cost; `onset f24` (blip) fails `STATIC`
     347 vs 329 for its pinned `w4/w6/w7` active-filter pins. Consistent control: the
     inactive/knife‑edge pins (`onset f26`, `sine f5`, `sine‑441 f5`) leave `STATIC`
     **identical**, exactly as `filter inactive both sides` predicts; the gain-shift pins are
     mixed (`onset‑441 f22` identical, `multitone f1` different — the latter is a blip frame,
     §8.3). [derived‑here + recorded §11]
   * **MS decision** — `sine f5` fails the `MSD` gate at exactly one band (16), reproducing
     the already-recorded `phase1e_sine_f5_band16_residue` result (`stored` operands bit‑equal,
     Ld residue shifting the strict `>` margin by exactly −2 539 383). [derived‑here + recorded]
   * **S4 quantizer / scalefactors** — `click f11` (`S4SCF` 2/45 bands ±1), `impulse f5`
     (`S4SCF` 3/52, then `S4Q`/`S5`/`S6`), `sine‑441 f5` (`S4Q` 1/1024 lines) are the first
     frames whose **exact** decision comparison fails with no earlier `STATIC`/`MSD`/`ATS`
     failure. The oracle-fed differential (`phase1b_isolated_differential`, passing) proves the
     Rust S4/S5/S6 stages are **bit-exact given C's inputs**, so these flips are produced by the
     in-tolerance upstream residue, not by a stage defect. [derived‑here + recorded]
   * **Cross-frame adaptive state** — `onset f26` (ATS weighting state entering the frame
     differs 0.63 %) and `sweep f10` (11 %) fail at `ATS pre`, i.e. the divergence entered at
     an earlier frame. For `onset f26` the chain back to the `f25` TNS divergence is
     hypothesised (§10.3); for `sweep f10` the origin is **inconclusive** because frames 0–9 of
     that fixture carry no oracle frame. [derived‑here, §8, §10.4]

5. **The MDCT and transform-input gates are tolerance/identity checks with narrow coverage and
   are *not* bit-exact for the MDCT.** `mdct_parity_with_reference` compares against the Q31
   reference with `|Δ| < 5e-4·peak` per bin **and** a median path-gain ratio inside `1 ± 1e-3`
   (`transform.rs:491-605`); only the ring test is exact-`i16` (`transform.rs:414-448`,
   `:451-488`). Of the 11 divergent frames, 7 lie inside the MDCT gate and it is green for them;
   `sweep‑stereo f10` carries `mdct_*` files the gate does not iterate, and 3 have no MDCT
   vector at all — those are marked inconclusive (§8.5). [derived‑here, §8.5]

6. **`sine–stereo f1` is the one frame that cannot be attributed at all**: it has no
   `mdct_*`/`ring_*`/`psy_*`/`rxx_*`/`ms_*` reference, and its `qc1`–`qc8` oracle replay is a
   blip. Its earliest reliable comparison is therefore the payload itself, and even that is a
   script comparison rather than a repository gate. The exact reference data needed is named in
   §10.4. [derived‑here]

7. **No evidence of instrumentation-induced perturbation was found — within stated limits.**
   The instrumented temporary path emits byte‑identical ADTS for all 15 synthetic fixtures
   versus the hash‑pinned M5.22 reference outputs (15/15 SHA‑256 equal), and its test suite
   matches an independent uninstrumented build of the same tree except for the single added dump
   test (443 vs 442 passing, same single unrelated failure). This bounds perturbation for the
   covered fixtures and paths only; §5.1 lists what it does **not** cover — the `m7-*` fixtures
   were not re-encoded under instrumentation, unexercised encoder paths (retry/crash, live
   PNS/IS, out-of-contract inputs) are outside both builds, and the dump writer itself is new
   code whose correctness is not independently validated. [derived‑here, §5.1]

8. **Recommendation: the next milestone is V3, the quality baseline** (delay-compensated
   SNR/SDR + bitrate/frame-size distributions, spec §6). M5.26 shows the divergences are
   value-in-tolerance and decision-level; the remaining question is their measured magnitude,
   not their location. §12 states the required reference data for the two still-open items.

---

## 2. Scope and completion criteria

**In scope (exactly the frozen M5.25 set):** 11 divergent frames with committed C ground truth
(click f11; impulse f5; onset f24/f25/f26; onset‑441 f22; sine f1/f5; sine‑441 f5; stepdown f2;
sweep f10) plus the 9 negative controls (edge f4; multitone f1; noise f5; onset f22/f23;
quiet‑sine f5; silence f5; stress f5; tonal‑noise f5). Fixture/vector identities are the ones
the audit recorded; no frame was substituted.

**Completion criteria (from the milestone brief) and status**

| # | Criterion | Status |
|---|---|---|
| 1 | All 20 cases accounted for | **met** — §8, 20 rows |
| 2 | Every first-divergence claim supported by an existing gate + traceable evidence | **met with the evidence class stated** — each claim cites either a repository gate (`prod`/`replay`) or a script comparison whose threshold is taken from an existing gate; §4.1 names the gates, §4.4 maps them to the §8 verdicts, §14 indexes the sources |
| 3 | Controls not assumed to pass | **met** — each measured; 8/8 non-blip controls show no deciding comparison failing (§8.2), and `stress‑stereo f5`'s undecidable `seq`/`geo` terms are marked inconclusive rather than passed |
| 4 | Instrumented output matches uninstrumented output | **met within the stated limits** — 15/15 synthetic fixtures byte-identical, suite parity with an independent uninstrumented build; §5.1 states the coverage and dump-correctness limits |
| 5 | No existing project file other than the new report changed | **met** — §13 |
| 6 | Nothing staged or committed | **met** — §13 |
| 7 | Insufficient evidence reported as such with the missing data named | **met** — §10.4, §11 |

---

## 3. Stage map of the Rust production path

Derived from source (line references verified against the worktree). The Rust "production path"
is the module chain driven by the test-only integration driver `stream_driver.rs` (there is no
production API yet, D‑080); `StreamEncoder::step_frame` (`stream_driver.rs:801`) is the frame
loop.

Pipeline stages are labelled **P0…P11** so they cannot be confused with the oracle dump stages
**S1…S8** (`qc1`…`qc8`) used throughout the rest of this report. The right-hand note gives the
oracle stage each pipeline stage is compared at.

| # | Stage | Rust | C counterpart | oracle stage |
|---|---|---|---|---|
| P0 | PCM feeder / framing (`ceil((S+1600)/1024)`) | `pcm.rs:99-122,261`; `stream_driver.rs:47-49` | `aacenc_lib.cpp:759,1091-1114` | – |
| P1 | Block switching + pair sync + grouping | `block_switch.rs`; `stream_driver.rs:614-624` | `FDKaacEnc_BlockSwitching`/`SyncBlockSwitching` `block_switch.cpp:229,426` | `seq_*` |
| P2 | MDCT / windowing (rescaled spectrum) | `transform.rs`; `stream_driver.rs:627-642` | `FDKaacEnc_Transform_Real` | `mdct_*`, `ring_*` |
| P3 | Psy per channel: maxscale, stored energies, Ld energies, thresholds, spread, nm1, TNS detect/apply/encode, pre-echo | `psy.rs::process_pair_reuse` | `psy_main.cpp` psy chain | `psy_*`, `rxx_*` |
| P4 | Grouped (short) outputs; MS/IS decision | `psy.rs::group_short_channel` (`psy.rs:1231`), `MsOutput` (`psy.rs:364`) | `grp_data.cpp`, `ms_stereo.cpp` | `ms_*` |
| P5 | Q31 spectrum mapping (**the documented single approximation point**), stored/Ld working sets, MS mirror | `stream_driver.rs:742`, `:845-949`; `energy.rs:152-159` | `band_nrg.cpp:111-184`, `ms_stereo.cpp` | S1 (`qc1`) |
| P6 | Form factor, PE weighting, element PE + static bits | `stream_driver.rs:951-1029`; `pe.rs` | `CalcFormFactor`, `peCalculation`, `ChannelElementWrite(NULL)` | S1 (`qc1`) |
| P7 | VBR threshold adaptation (THREXP/THRV/MINSNR/SPREAD, chaos) | `stream_driver.rs:1031-1077`; `threshold_vbr.rs` | `FDKaacEnc_AdjustThresholds` VBR branch | S2 (`qc2`) |
| P8 | Scalefactor estimation (inv=2) + quantizer | `stream_driver.rs:1079-1119`; `scf.rs`, `quant.rs` | `FDKaacEnc_EstimateScaleFactors`, `QuantizeSpectrum` | S3 (`qc3`), S4 (`qc4`) |
| P9 | QMAX + sectioning + Huffman/scf/noise counting | `stream_driver.rs:1121-1154`; `bitcount.rs` | `calcMaxValueInSfb`, `dynBitCount` | S5 (`qc5`) |
| P10 | QC gain loop / VBR rate control / grants / fill | `stream_driver.rs:1156-1260`; `rate.rs` | `QCInit`/`AdjustBitrate`/`DistributeBits`/`qc_main.cpp` | S6 (`qc6`) |
| P11 | Payload writers + element/raw assembly + ADTS | `frame.rs`, `bitenc.rs`, `adts.rs` | `bitenc.cpp`, transport | S7 (`qc7`), S8 (`qc8`) |

The **frame-entry adaptive state** is `ATS[chaos_measure_en_fac, last_enfac_patch]` (P6/P7) and
the `DistributeState` six-tuple (P10); both are produced by the previous frame, so a
single-frame vector cannot distinguish per-frame noise from trajectory drift.

---

## 4. Existing gates and their intended comparison boundaries

Only gates that already exist in the repository are used. Each gate is an *existing assertion
or tolerance*; none was changed, relaxed, or invented for this milestone.

### 4.1 Gate definitions, and what kind of evidence each one is

Three kinds of comparison appear in this report and are **not** interchangeable:

* **`prod`** — an existing repository assertion that drives the **production** path on fixture
  PCM and compares the result against committed C evidence. Only these support a verdict of the
  form "this existing gate passes/fails on this frame".
* **`replay`** — an existing repository assertion that feeds **committed C vectors** into a Rust
  stage and requires the stage output to reproduce the C stage output. It proves that a stage is
  exact *given identical inputs*; it never compares production values.
* **`script`** — a comparison performed by this milestone's temporary scripts against committed
  C vectors, using a threshold taken from an existing gate or from the M3 tolerance helpers. It
  is evidence, **not** an existing gate. §4.4 states exactly which §8 verdicts are of this kind.

| Gate | Kind | Definition | Enforcing assertion (source) |
|---|---|---|---|
| G‑seq | prod | window sequence, exact, on every frame of each `CASES` prefix | `psy.rs:2959-2964` |
| G‑geo | prod | geometry: `SOFF` offsets and `cnt/per/max`, exact; grouped-offset agreement | `stream_driver.rs:2862-2867`; `:2003`; `:2113` |
| G‑maxscale | prod | window-0 maxscale exact except the recorded `KNOWN_MAXSCALE` clz-boundary flips | `psy.rs:1706-1727`; pins `psy.rs:1872` |
| G-psy-E | prod | per-window stored energies, `32 + |w|/500` | `psy.rs:1728-1731` (helper `psy.rs:1496-1503`) |
| G-psy-ELd | prod | per-window energy Ld, energy-domain `32 + e/500` | `psy.rs:1732-1734` (helper `psy.rs:1520-1541`) |
| G-psy-T | prod | per-window stored thresholds, `32 + |w|/500` | `psy.rs:1735-1737` |
| G-psy-S | prod | per-window spread, `32 + |w|/500` | `psy.rs:1738-1740` |
| G-nm1 | prod | `nm1` q31; `m_nm1`/`calc_pre_echo` exact | `psy.rs:1741-1743` |
| G-grouped | prod | grouped **offsets** (`psy.rs:1794`) and **`min_snr`** (`psy.rs:1795-1799`) **exact**; grouped E/T/S and Ld **tolerance-gated** (q31 / energy-domain, as G-psy-E / G-psy-ELd); **regrouped** spectrum tolerance `1e-3 · max(1.0, peak)` (`psy.rs:1825-1826`) | `psy.rs:1744-1840` |
| G-TNS | prod | `active`/`order`/`length`/`direction`/coefs exact; gain `|Δ| ≤ 8 + |want|/1000` (`GAIN_TOL_FLOOR = 8`); 40 recorded `KNOWN_TNS` decisions | `psy.rs:2816-2918`; `tns.rs:700-720`; pins `psy.rs:2227` |
| G-MS | prod | mid/side stored (q31) + Ld (energy domain) per window | `psy.rs:2921-2930` |
| G-mdct | prod | MDCT vs the Q31 reference: `|Δ| < 5e-4·peak` per bin **and** median path-gain ratio `|1 − r| < 1e-3` — a **tolerance** gate, not bit-exact | `transform.rs:491-605` |
| G-ring | prod | transform-input ring, exact `i16` | `transform.rs:414-448`; `:451-488` |
| G-S1-geo | prod | production geometry/offsets vs `qc1` | `stream_driver.rs:2862-2867`; `:2003`; `:2113` |
| G-S1-PTHLD | prod | production threshold Ld vs `qc1 PTHLD`, **exact** | `stream_driver.rs:2871` |
| G-S1-SENL | prod | production stored energy vs `qc1 SENL`, `32 + |w|/500` | `stream_driver.rs:2878` (sine f5 inline: `≤ 8192` absolute, `:1680`) |
| G-S1-PENLD | prod | production energy Ld vs `qc1 PENLD`, energy-domain `32 + e/500` | `stream_driver.rs:2886` (sine f5 inline: `:1698-1704`) |
| G-S1-FF | prod | form factor re-derived from the live spectrum, exact | `stream_driver.rs:2895` |
| G-S1-SPEC | prod | relative spectrum agreement; asserted (`≤ 1e-4`) **only for `sine‑stereo f5 ch0`**; report-only (printed, not asserted) inside `tier_b_core` | `stream_driver.rs:1674`; report-only `:2905-2912` |
| G-S1-ATS | prod | pre-frame weighting state, exact | `stream_driver.rs:2976`; `:3123` |
| G-S1-STATIC | prod | static element bits, exact | `stream_driver.rs:2973`; `:3125` |
| G-S1-MSD | prod | MS digest + full mask, exact (LONG); SHORT reference documented stale | sine‑stereo f5 gate: `stream_driver.rs:2006-2009` (`flips == [16]`); separate sine‑44100 f5 Tier‑B assertion: `stream_driver.rs:3018-3019`; staleness recorded in `phase1e_onset_f24_bands_causality` (`stream_driver.rs:2069-2086`) |
| G-S4 | replay | `S4SCF`/`S4GAIN`/`S4Q`/`S4SPEC` + per-band DIST/EN exact given C S1/S2 | `scf.rs:1520` (`s4_corpus_exact`); `scf.rs:1649` (`s4_unit_probe_exact`) |
| G-S5 | replay | sections + four counts + `dyn` exact given C S4 | `bitcount.rs:1562` (`s5_corpus_exact`) |
| G-S6 | replay | grants/pass trace/finals/post-state exact given C S1/S4/S5 | `rate.rs:2149` (`s6_corpus_exact`) |
| G-S7 | replay | payload-section hex + counts exact given C S4/S5 | `bitenc.rs:1228` (`s7_corpus_payloads_match_c_and_s5`) |
| G-S8 | replay | element/raw hex exact given C S1/S4/S5/S6 | `frame.rs:1395` (`s8_corpus_elements_match_c_bit_exact`) |
| G-stage-equiv | replay | isolated S4/S5/S6 differential given C inputs | `phase1b.rs:119-481` (`phase1b_isolated_differential`) |

Two gates in the earlier draft have been **removed** because they do not exist:

* there is **no gate for `THRV`/`MINSNR1`/`SPREAD1` on production values** — the S2 stage has
  synthetic unit tests and the replay gates above, but no production-vs-`qc2` assertion;
* there is **no gate for per-frame payload byte identity**. `m55_streams_match_c_reference`
  (`stream_driver.rs:3878`) covers only the three `m7-*` fixtures and explicitly requires
  *validity and accounting only* ("no byte-parity requirement", `stream_driver.rs:3922-3923`),
  and `phase1_full_stream_parity` states "No parity asserts here by design"
  (`stream_driver.rs:3564-3565`). Per-frame payload identity in §8 is therefore a **script**
  comparison (§4.4).

### 4.2 Coverage of the 20 frozen frames (committed reference families)

`y` = committed vector present for that channel; `-` = absent.

| frame | kind | ch | seq | mdct | ring | psy/rxx | ms | qc1..qc8 |
|---|---|---|---|---|---|---|---|---|
| click‑mono f11 | positive | 1 | y | ch0 | – | ch0 | – | yyyyyyyy |
| impulse‑mono f5 | positive | 1 | y | ch0 | – | ch0 | – | yyyyyyyy |
| onset‑stereo f24 | positive | 2 | y | ch0,ch1 | ch0 | both | y | yyyyyyyy |
| onset‑stereo f25 | positive | 2 | y | ch0 | ch0 | both | y | yyyyyyyy |
| onset‑stereo f26 | positive | 2 | y | ch0 | – | both | y | yyyyyyyy |
| onset‑44100 f22 | positive | 2 | y | – | ch0 | both | y | yyyyyyyy |
| sine‑stereo f1 | positive | 2 | y | – | – | **–** | – | yyyyyyyy |
| sine‑stereo f5 | positive | 2 | y | ch0 | – | both | y | yyyyyyyy |
| sine‑44100 f5 | positive | 2 | y | – | – | both | y | yyyyyyyy |
| stepdown‑stereo f2 | positive | 2 | y | ch0 | – | both | y | yyyyyyyy |
| sweep‑stereo f10 | positive | 2 | y | ch0,ch1 | – | both | y | yyyyyyyy |
| edge‑mono f4 | control | 1 | y | – | – | ch0 | – | yyyyyyyy |
| multitone‑stereo f1 | control | 2 | y | – | – | both | y | yyyyyyyy |
| noise‑mono f5 | control | 1 | y | – | – | ch0 | – | yyyyyyyy |
| onset‑stereo f22 | control | 2 | y | ch0 | – | both | y | yyyyyyyy |
| onset‑stereo f23 | control | 2 | y | ch0 | ch0 | both | y | yyyyyyyy |
| quiet‑sine‑stereo f5 | control | 2 | y | – | – | both | y | yyyyyyyy |
| silence‑stereo f5 | control | 2 | y | ch0 | – | both | y | yyyyyyyy |
| stress‑stereo f5 | control | 2 | **–** | – | – | **–** | – | yy‑yyyyy |
| tonal‑noise‑stereo f5 | control | 2 | y | – | – | both | y | yyyyyyyy |

Consequences used below: **19/20** frames are inside the `seq_*` (frame-level) gate — every
fixture except `stress‑stereo`, for which no `seq_stress-stereo.txt` is committed; **18/20** are
inside the M2/M3 psy gate's `CASES` (`psy.rs:2932-2947`); **11/20** have a committed `mdct_*`
vector, but only **9** of those are inside the `mdct_parity_with_reference` gate's vector list
(`silence‑stereo f5` and `sweep‑stereo f10` carry files that the gate does not iterate);
**4/20** have a committed `ring_*` vector, of which only **2** (`onset‑stereo f23`, `f24`) are
inside the ring gate. `sine‑stereo f1` and `stress‑stereo f5` have **no** per-stage M2/M3
reference at all, and `stress‑stereo f5` additionally has no `qc3` (S3) vector.

### 4.3 Reference-health limits already on record (not new findings)

1. **Oracle SHORT_WINDOW blips (D‑021).** Ten content frames where the oracle's replay frame
   length disagrees with the real `aac-enc`. This milestone re-derives the criterion
   (`S6FINAL total + 56 ≠ 8·real_frame_len`) and confirms exactly four of our frames:
   `onset f24`, `onset‑44100 f22`, `sine f1`, `multitone f1`. For these the oracle S1→S8 chain
   is not a faithful reference.
2. **SHORT-frame dump MSD staleness** — recorded in the doc comment of
   `stream_driver.rs::tests::phase1e_onset_f24_bands_causality` (`stream_driver.rs:2069-2086`):
   the S1 oracle path copies stale per-window MS Ld, so the dumped mask is not the rule output
   for SHORT frames → the `MSD` comparison is **inconclusive** on SHORT stereo frames.
3. **Difference-channel spectra** are report-only in the S1 comparison because exact
   `(l−r)` substitution amplifies benign input noise (`stream_driver.rs:2897-2901`).

### 4.4 Which counter is an existing gate, and which is a script comparison

For every ladder item used in §8–§9, this table states the existing repository gate (if any),
the frames it actually covers, and whether the per-frame verdict in §8 is gate-backed or
script-backed. Where the "existing gate" column is empty, the verdicts are this milestone's own
script comparisons against committed C vectors — **evidence, not gates**.

| Ladder item | Existing gate (kind) | Frames it covers | Verdict source in §8 |
|---|---|---|---|
| `seq` | G‑seq (prod) | 19/20 — all but `stress‑stereo f5` | gate for 19; **inconclusive** for `stress f5` |
| `geo` | G‑geo / G‑S1-geo (prod) | `tier_b_core` frames (sine f5, click f11, sine‑441 f5) and the two phase1e offset asserts (sine f5, onset f24) | gate for those; **script** elsewhere; **inconclusive** for `stress f5` (no committed sequence/geometry reference) |
| `ATS pre`, `STATIC` | G‑S1-ATS / G‑S1-STATIC (prod) | `phase1i_*_tier_b` (click f11, sine‑441 f5, multitone f5 — outside scope) | gate for those; **script** elsewhere |
| `MSD` | G‑S1-MSD (prod) | asserted for `sine f5` (`phase1e_sine_f5_band16_residue`); `onset f24` documents SHORT staleness | gate for `sine f5`; **script** elsewhere; **inconclusive** on SHORT |
| `SENL`, `PTHLD`, `PENLD`, `FF` | G‑S1-SENL / G‑S1-PTHLD / G‑S1-PENLD / G‑S1-FF (prod) | `tier_b_core` frames (sine f5, click f11, sine‑441 f5); sine‑f5 inline variants | gate for those; **script** elsewhere |
| `SPEC` | G‑S1-SPEC (prod) | asserted **only** for `sine f5 ch0`; report-only in `tier_b_core` | gate for `sine f5 ch0`; **script** elsewhere |
| `THRV`, `MINSNR1`, `SPREAD1` | **none** | — | **script** (thresholds taken from the M3 gates) |
| `S4SCF`, `S4GAIN`, `S4Q`, `S4SPEC` | G‑S4 (replay, oracle-fed) | 19/20 replay inputs | **script** for production values |
| `S5QMAX`, `S5SEC`, `S5BITS` | G‑S5 (replay) | 19/20 | **script** |
| `S6GRANT`, `S6IT`, `S6FINAL` | G‑S6 (replay) | 20/20 | **script** |
| `PAYLOAD` | **none** (`m55_streams_match_c_reference` needs no parity and covers only `m7-*`; `phase1_full_stream_parity` has no parity asserts) | — | **script** |

The `replay` gates in this table are what license the *classification* claim used in §7 item 4
and §9 — "the failing stage is downstream of the residue and is exact given identical inputs" —
because they show the stage itself reproduces C bit-exactly when fed C's vectors.

---

## 5. Instrumentation method

* **Location.** A full copy of the worktree at
  `…/T/opencode/m526/work` (repository untouched), with instrumentation in **one** file:
  `rust/src/aac_enc/stream_driver.rs` (**+407 / −1** lines vs
  sha256 `b499fb3c1d94236e5b05536e85b3acb2006c5f7105c43624b4947d0c242d2f4c`; instrumented
  sha256 `966b6d79891013c4711d22b162560d2440926136e323ac6dce01b41bea65980d`).
* **What was added.** (a) A `StageDump` struct of read-only clones; (b) a `stage` field on the
  existing `FrameSnapshot`; (c) unconditional capture statements placed *after* each stage's
  values are final (same pattern as the pre-existing `S1Cap`/`MsDecisionCapture`, which are
  already captured unconditionally in production runs); (d) a test-only `m526_dump` module that
  writes one text dump per target frame plus the emitted ADTS for each fixture.
* **Behaviour neutrality by construction.** No arithmetic, operation order, branch or encoding
  decision was added or reordered: every capture is a `clone()` of an already-computed value;
  the only new control flow is inside the new test module, which is not on the encoder path.
  This is a *by-construction* argument; §5.1 states the empirical evidence for it and its limits.
* **Capture points** (all mirror the oracle tag semantics): S1 post-MS `spec_q31`, `e_ld`,
  `t_ld`, `e_stored`, `ff`, `tns`, offsets/geometry, ATS pre/post, PE data + per-band PE,
  MS digest/mask, static bits; S2 chaos pre/post and `THRV`/`THREXP`/`MINSNR0/1`/`SPREAD0/1`;
  S4 `scf`/`gain`/`quant_spec`/`mutated`; S5 `qmax`/sections/`(side,huff,scf,noise,first,dyn)`;
  S6 gstate-pre, grants, pass trace, finals, `final_max_sfb`. The frame payload is the emitted
  ADTS bytes.

### 5.1 Behaviour-preservation check (required)

```sh
T=/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode
# instrumented temp copy
cd "$T/m526/work/rust" && M526_OUT="$T/m526/out" \
  cargo test --features aac_enc m526_dump_cases -- --nocapture
# byte-compare against the hash-pinned M5.22 reference outputs
for each of the 15 synthetic fixtures: sha256(out/rust_<short>.aac) == sha256(.../m52-rust-aac/m58-corpus/<stem>.m58.aac)
# independent uninstrumented copy of the same tree
rsync -a --exclude rust/target --exclude .git <repo>/ "$T/m526/base/"
cd "$T/m526/base/rust" && cargo test --features aac_enc
```

| Check | Result |
|---|---|
| Instrumented vs pinned M5.22 Rust reference outputs (15 synthetic fixtures) | **15/15 SHA‑256 identical** |
| Instrumented suite | 443 passed / 1 failed / 0 ignored |
| Independent uninstrumented suite (same tree) | 442 passed / 1 failed / 0 ignored |
| Delta | exactly the one added `m526_dump_cases` test |
| Single failure, both builds | `tp_dec::pce::tests::read` — the known, unrelated, deliberately untouched failure (D‑083) |

**Bounded conclusion.** *No evidence of perturbation was found* on the fixtures and paths these
checks cover: the encoder output is byte-identical to the uninstrumented reference on all 15
synthetic fixtures, and no existing test that passed before the instrumentation fails after it.
That is the strongest statement the checks support. It is **not** a general proof of
equivalence. The limits are:

* **Coverage of the byte-identity check.** The 15 fixtures that contain the 20 target frames were
  re-encoded and compared; the three `m7-*` real-audio fixtures were **not** re-encoded under
  instrumentation in this milestone (the base copy's corpus test was run for suite parity only),
  so byte identity is not established for them.
* **Coverage of the dump.** Only the 20 target frames were dumped; every other frame's captured
  values were never written or inspected, so a capture placed at the wrong point could exist
  undetected outside those frames.
* **Dump correctness is not independently validated.** The dump writer is new code; nothing
  re-derives a dumped value from an independent source. A defect in the *writer* would corrupt
  the reported stage values without changing any encoder output, and the byte-identity and suite
  checks cannot detect it. The dump's internal consistency with `qc1`–`qc6` conventions is the
  only assurance, and §11 item 6 records one place where those conventions still need care.
* **Unexercised encoder paths are outside both builds.** Retry/crash recovery, live PNS/IS
  books, and out-of-contract inputs never occur on the corpus (D‑014, D‑040, D‑082), so none of
  the above says anything about them under instrumentation.

---

## 6. Evidence classification for this report

* **Measured** — produced by running code in this milestone (the stage dumps, the payload
  comparisons, the blip criterion, the suite/byte-identity checks).
* **Derived** — arithmetic or set operations over measured values (gate verdicts, magnitudes).
* **Recorded** — taken from existing project documents/vectors (pins, oracle semantics,
  documented divergences); cited, not re-derived.
* **Hypothesis** — a causal chain that is plausible but not proven by a gate.
* **Missing evidence** — a required reference that does not exist; the chain stops and is
  reported `inconclusive`.

---

## 7. Attribution rules applied

1. **Two ladders, reported separately.**
   * **Class A (decision/structural, exact).** `seq`, `geo`, `ATS pre`, `STATIC`, `MSD` (LONG
     stereo), `S4SCF`, `S4GAIN`, `S4Q`, `S5QMAX`, `S5SEC`, `S5BITS`, `S6GRANT`, `S6IT`,
     `S6FINAL`, `PAYLOAD`. The exactness expectation for these integer fields is the M4 plan's
     comparison policy (`docs/aac-enc-m4-quant-ratecontrol-plan.md` §7.3); the plan
     **defers Ld/Q31 values to the existing M3 gates** and does not itself define any tolerance.
   * **Class B (value, documented tolerance).** `SENL`, `PENLD`, `PTHLD`, `THRV`, `MINSNR1`,
     `SPREAD1`, `SPEC` (sum-like channels). Their thresholds are the M3 helpers
     (`psy.rs:1496-1503`, `:1520-1541`), not a new policy.
2. **Every verdict states which kind of evidence it rests on** — an existing `prod`/`replay`
   gate, or a `script` comparison applying an existing threshold (§4.1, §4.4). A `script`
   counter is reported as evidence, never as "the gate passes".
3. **Applicability is stated per frame**: a stage with no committed reference, or with a
   reference already shown unfaithful, is reported `inconclusive` (never "pass").
4. **A downstream difference that merely follows from an earlier one is labelled as such**,
   using the oracle-fed differential (`phase1b_isolated_differential`) and the oracle-fed
   S4/S5/S6/S7/S8 replay gates as the proof that the *stage* is exact given identical inputs.
5. **A tolerance violation that occurs on a byte-identical control falsifies that term** for
   attribution purposes; the term is flagged rather than used (this is how `MINSNR1` and the
   blip-frame value deviations were identified).

---

## 8. Per-frame results

Notation: `IDENT` = bit-identical; `IN` = inside the documented tolerance, not identical;
`OUT` = comparison fails; `INCONC` = reference unavailable/unfaithful; `n/a` = does not apply.
Magnitudes are the measured values. Each row states the threshold used; per §4.4, only the
frames listed there have a repository gate behind a verdict — the remaining verdicts are this
milestone's script comparisons against committed C vectors and are labelled as such where they
decide an outcome.

### 8.1 Non-blip divergent frames (8)

**8.1.1 `click‑mono f11` (positive) — earliest exact-gate violation: `S4SCF`.**
Class B all in-gate: `SENL` differs in 3/45 bands, largest Δ 256 LSB at band 10 (tol 1.37e6);
`PENLD` differs in all 45 bands, largest inversion gap 1.13e3 energy units at band 10
(E = 6.86e8, tol 1.37e6); `PTHLD` **bit-identical**; `SPEC` 822/832 active
lines differ, `rel` 5.08e‑6 (gate 1e‑4); `THRV` (0.046), `MINSNR1` (2.56e5 vs tol 1.24e6) and
`SPREAD1` in-gate. Class A: `seq`,
`geo`, `ATS pre` (`[402653184,0,0,0]`), `STATIC` (29) all IDENT; then **`S4SCF` 2/45 bands differ
(band 4: got 1 want 0)**, `S4Q` 2/1024 lines differ (index 43: 363 vs 362); `S4GAIN`,
`S5QMAX`, `S5SEC`, `S5BITS`, `S6GRANT`, `S6IT`, `S6FINAL` all IDENT; `PAYLOAD` differs (42 vs
42 B, first differing byte 15). Attribution: **S4 scalefactor/quantizer flip produced by
in-tolerance S1 residue** (the recorded D‑011 maxscale pin and the Phase‑1I "energy‑Ld float
lane" note for this frame agree). Confidence: high for the stage; the audit's C1–C6 class is
**C2** (bitstream-only impact).

**8.1.2 `impulse‑mono f5` (positive) — earliest exact-gate violation: `S4SCF`.**
Class B all in-gate — `SENL` largest Δ 1 056 LSB at band 33 (tol 2.03e5); `PTHLD` differs in
14/52 bands, largest inversion gap 5.0 energy units at band 51 (tol 237); `THRV` 4.95 vs 234;
`MINSNR1` 2.42e5 vs 3.42e6; `SPEC` rel 3.68e‑5 — with `PENLD` marked `INCONC` because the C
grouped‑Ld dump uses a different pad convention on SHORT frames (§11 item 6). Class A:
`seq`=SHORT, `geo` (56/14/13), `ATS pre`, `STATIC` (76) IDENT; **`S4SCF` 3/52 bands differ
(index 31: 19 vs 18)**; then `S4Q` 10/1024, `S5QMAX` 2/56, `S5SEC` 9 vs 9 sections (section 6:
book 7/48/1/42 vs 9/48/1/54), `S5BITS` (huff 1118 vs 1130), `S6IT`/`S6FINAL` (dyn 1272 vs
1282), `PAYLOAD` 169 vs 171 B (first differing byte 10). Attribution: same class as 8.1.1;
this is the §4.A attack frame of the audit — its 2‑byte difference is **the documented numeric
class, not a new class**. Confidence: high for the stage (direct measurement + the passing
oracle-fed S4/S5/S6/S8 gates).

**8.1.3 `onset‑stereo f25` (positive) — earliest exact-gate violation: `STATIC`.**
Class A: `seq`=STOP, `geo`, `ATS pre` (`[1610612736,1610612736,1,1]`), `MSD` (digest 1/1,
mask identical) IDENT; **`STATIC` 200 vs 208 bits**; `S4SCF` 1/45, `S5BITS` scf 61 vs 58,
`S6GRANT`/`S6IT`/`S6FINAL` differ; `PAYLOAD` 112 vs 112 B (first difference byte 21). Class B:
`SENL`/`PTHLD`/`SPEC`/`THRV`/`SPREAD1` IN; `MINSNR1` flagged (11/45, band 22 E 1.3714e9 vs
1.3667e9, tol 2.73e6). Attribution: **TNS decision divergence** — `KNOWN_TNS` pins
`onset‑stereo f25 ch0/ch1 w0` as *"input-driven parcor border flips, filter active both
sides"*; an active filter with different coefficients changes the element's static side-info
cost, which is exactly the observed `STATIC`/payload difference. **Recorded** pin + measured
consequence. Confidence: high.

**8.1.4 `onset‑stereo f26` (positive) — earliest exact-gate violation: `ATS pre`.**
Class A: `seq`=LONG, `geo`, `STATIC` (88) IDENT, `S4SCF`/`S4GAIN`/`S4Q`/`S5*`/`S6*` all IDENT;
**`ATS pre` differs: `[410386432, 402653184, 0, 0]` vs `[407797760, 402653184, 0, 0]`**
(0.63 %); `MSD` mask differs in **1 band**; `PAYLOAD` differs (51 vs 51 B, byte 4). Class B:
IN except the flagged `MINSNR1` (2/45, band 16 E 1.0617e9 vs 1.7180e9). Attribution: the
divergence is **not produced inside frame 26** (every in-frame stage is IDENT); it enters with
the previous frame's weighting state. `KNOWN_TNS` pins `f26 w0` as *"knife‑edge, filter
inactive both sides"* (spectrum unchanged, consistent with `STATIC` and S4/S5/S6 all IDENT),
so the TNS pin does not explain the ATS drift; the hypothesised chain is the `f25`
active-filter divergence propagating through PE → `chaosMeasureEnFac` (§10.3). Confidence:
high for the location (state entering the frame); medium for the causal chain.

**8.1.5 `sine‑stereo f5` (positive) — earliest exact-gate violation: `MSD`.**
Class A: `seq`=LONG, `geo`, `ATS pre`, `STATIC` (88) IDENT; **`MSD` mask differs in exactly one
band (band 16)**; `S4Q` 1/1024 differs; `S5SEC`/`S5BITS`/`S6IT`/`S6FINAL` differ; `PAYLOAD`
52 vs 51 B (byte 4). Class B: IN except the flagged `MINSNR1`. Attribution: **MS decision
flip** — the already-recorded `phase1e_sine_f5_band16_residue` test proves the stored operands
at band 16 are bit-identical, the Ld operands are inside the energy envelope, and the measured
residue shifts the strict `>` margin by exactly −2 539 383, flipping the bit. **Recorded** +
measured. Confidence: high.

**8.1.6 `sine‑44100 f5` (positive) — earliest exact-gate violation: `S4Q`.**
Class A: `seq`, `geo`, `ATS pre`, `STATIC` (90), `MSD` (mask identical) all IDENT; **`S4Q`
1/1024 lines differ (index 22: 0 vs −1)**; `S5QMAX` 1/47 (index 5: 0 vs 1), `S5SEC`, `S5BITS`
(huff 201 vs 205), `S6IT`/`S6FINAL`; `PAYLOAD` 47 vs 47 B (byte 18). Class B: IN except
`MINSNR1` flagged (1/47). Attribution: **quantizer 1‑LSB flip** from in-tolerance S1 residue
(1 band crosses the dead-zone boundary); the S4 stage is shown exact given C inputs by the
replay gate `s4_corpus_exact` (§4.1 G‑S4). Confidence: high.

**8.1.7 `stepdown‑stereo f2` (positive) — earliest exact-gate violation: `STATIC`.**
Class A: `seq`=STOP, `geo`, `ATS pre`, `MSD` (mask identical), `S4SCF`/`S4GAIN`/`S4Q`/`S5*`
IDENT; **`STATIC` 208 vs 200 bits**; `S6GRANT`/`S6IT`/`S6FINAL` differ; `PAYLOAD` 123 vs 122 B
(byte 24). Class B: IN except `MINSNR1` flagged. Attribution: **TNS decision divergence** —
`KNOWN_TNS` pins `stepdown‑stereo f2 ch0/ch1 w0` as *"input-driven parcor border flips, filter
active both sides"*; identical mechanism to 8.1.3. Confidence: high.

**8.1.8 `sweep‑stereo f10` (positive) — earliest exact-gate violation: `ATS pre`.**
Class A: `seq`, `geo`, `STATIC` (88) IDENT; **`ATS pre` differs `[676200448, 402653184, 0, 0]`
vs `[753664000, 402653184, 0, 0]` (11 %)**; `MSD` mask differs 2 bands; `S4SCF` 5/45, `S4Q`
7/1024, `S5*`, `S6*`; `PAYLOAD` 86 vs 84 B (byte 7). Class B: **all in-gate** (no `MINSNR1`
deviation). Attribution: the divergence is in the **adaptive state entering the frame**; the
per-frame pipeline is otherwise consistent with the same numeric class. The origin frame is
**inconclusive** — `sweep‑stereo` has a committed `psy_*`/`mdct_*`/`ms_*` reference only for f10
(§4.2), so no oracle frame exists for f0–f9 where the drift must have started.

### 8.2 Non-blip control frames (8) — no deciding comparison fails

| frame | seq/geo | ATS pre | STATIC | MSD | S4SCF/GAIN/Q | S5* | S6* | PAYLOAD | value terms |
|---|---|---|---|---|---|---|---|---|---|
| edge‑mono f4 | IDENT | IDENT | IDENT 66 | n/a | IDENT | IDENT | IDENT | IDENT 172 B | all IN (`PENLD` INCONC: SHORT) |
| noise‑mono f5 | IDENT | IDENT | IDENT 29 | n/a | IDENT | IDENT | IDENT | IDENT 426 B | all IN |
| onset‑stereo f22 | IDENT | IDENT | IDENT 43 | IDENT | IDENT | IDENT | IDENT | IDENT 8 B | all IN |
| onset‑stereo f23 | IDENT | IDENT | IDENT 43 | IDENT | IDENT | IDENT | IDENT | IDENT 8 B | all IN |
| quiet‑sine‑stereo f5 | IDENT | IDENT | IDENT 88 | IDENT | IDENT | IDENT | IDENT | IDENT 74 B | `MINSNR1` **OUT** (flagged) |
| silence‑stereo f5 | IDENT | IDENT | IDENT 43 | IDENT | IDENT | IDENT | IDENT | IDENT 8 B | all IN |
| stress‑stereo f5 | **INCONC** (no committed sequence/geometry reference) | IDENT | IDENT 43 | IDENT | IDENT | IDENT | IDENT | IDENT 436 B | all IN |
| tonal‑noise‑stereo f5 | IDENT | IDENT | IDENT 43 | IDENT | IDENT | IDENT | IDENT | IDENT 439 B | all IN |

All entries in the first eight columns except `stress‑stereo f5`'s `seq`/`geo` are `script`
comparisons against committed `qc1`/`qc4`/`qc5`/`qc6` vectors and the real C output, except the
few cells that the repository gates directly (§4.4); the `PAYLOAD` column is a script comparison
for every frame. `stress‑stereo f5`'s `seq` and geometry terms are **inconclusive for missing
reference data** — no `seq_stress-stereo.txt` is committed and the psy `CASES` do not include the
fixture, so there is no sequence or geometry reference to compare against. Its remaining
entries and, in particular, its **payload-identity result (`436 B` vs the real C frame) are
independently supported and stand unchanged**.

Two calibration results are visible here and are used to falsify gates, not to excuse
divergences: `quiet‑sine‑stereo f5` carries a **recorded `KNOWN_MAXSCALE` pin** (clz-boundary
flips) *and* the `MINSNR1` out-of-envelope value, yet emits a **byte-identical** payload — so
neither a maxscale pin nor a `MINSNR1` deviation is by itself a decision divergence. The same
frame's `SPEC` for the **difference channel** is out of the 1e‑4 structural band (report-only
by the existing rule) while the sum-like channel is inside it.

### 8.3 Blip frames (4) — S4/S5/S6/S8 comparison inconclusive

| frame | kind | blip evidence | Class A (reliable part) | Class B | recorded divergence | outcome |
|---|---|---|---|---|---|---|
| onset‑stereo f24 | positive | `S6FINAL 792+56 = 848` bits vs **real 2 472** bits | `seq`/`geo`/`ATS pre` IDENT; `STATIC` 347 vs 329; S4–S6 **INCONC**; `MSD` INCONC (SHORT stale) | `SENL`/`THRV`/`SPREAD1` OUT **and the only `SPEC` failure in the whole set (`rel` 2.81e‑4)** — all blip-coincident | `KNOWN_TNS` w4/w6/w7 *"parcor border flip, filter active both sides"* | **TNS decision divergence (pinned)**; downstream attribution inconclusive |
| onset‑44100 f22 | positive | `2040` vs **real 2 424** bits | `seq`/`geo`/`ATS pre`/`STATIC` (279) IDENT; S4–S6 INCONC | `SENL`/`THRV`/`SPREAD1` OUT — blip-coincident | `KNOWN_TNS` w6/w7 *"gain shift, same filter"* | **TNS gain shift (pinned)**; downstream inconclusive |
| sine‑stereo f1 | positive | `784` vs **real 2 024** bits | `seq`/`geo`/`ATS pre`/`STATIC` (479) IDENT; S4–S6 INCONC | `SENL`/`THRV`/`SPREAD1` OUT — blip-coincident | none (no `psy_*` reference for f1) | **inconclusive** — no M2/M3 reference and a blip replay (§10.4) |
| multitone‑stereo f1 | control | `656` vs **real 1 784** bits | `STATIC` 177 vs 229; S4–S6 INCONC; `PAYLOAD` **IDENT** 216 B | `SENL`/`THRV`/`SPREAD1` OUT — blip-coincident | `KNOWN_TNS` w6 *"gain shift, same filter"* | control holds (payload identical); blip reference limits downstream comparison |

The blip set derived here is **exactly** the D‑021 set intersected with our scope, which is
independent confirmation of both the criterion and the D‑021 classification. The "Class A/B"
cells follow the evidence-class rules of §4.4: `seq`/`geo`/`ATS pre`/`STATIC` are `prod` gates on
the frames §4.4 names, the value cells are script comparisons, `MSD` is a `prod` gate that is
inconclusive on SHORT stereo, and the S4–S6 cells are script comparisons against a reference
that the blip makes unreliable.

### 8.4 Measured Class-B magnitudes (worst case per vector, non-blip frames)

`dmax` = largest energy-domain deviation; `tol` = the M3 threshold at that band. Those
thresholds are existing gates on the M3 path; applying them to the production values of all 20
frames is this milestone's **script** comparison (§4.4), except on the frames where a repository
gate asserts the same quantity.

| frame | SENL (LSB / tol) | PENLD (dmax @ band / tol) | PTHLD | THRV | MINSNR1 | SPEC `rel` |
|---|---|---|---|---|---|---|
| click‑mono f11 | 256 @10 / 1.37e6 | 1.13e3 @10 / 1.37e6 | 0 | 0.046 | 2.56e5 / 1.24e6 | 5.08e‑6 |
| impulse‑mono f5 | 1 056 @33 / 2.03e5 | INCONC (SHORT) | 5.0 @51 / 237 | 4.95 / 234 | 2.42e5 / 3.42e6 | 3.68e‑5 |
| onset‑stereo f25 | 1 392 @10 / 3.02e5 | 1.04e3 @10 / 3.02e5 | 0 | 1.12 / 39.9 | **4.64e6 / 2.73e6** | 9.74e‑6 |
| onset‑stereo f26 | 548 @10 / 2.15e6 | 554 @10 / 2.15e6 | 0 | 0.072 | **6.56e8 / 3.44e6** | 5.04e‑6 |
| sine‑stereo f5 | 698 @10 / 2.15e6 | 665 @10 / 2.15e6 | 0 | 0.072 | **6.56e8 / 3.44e6** | 5.03e‑6 |
| sine‑44100 f5 | 49 420 @10 / 1.81e6 | 4.95e4 @10 / 1.81e6 | 0 | 0.079 | **4.96e6 / 1.61e6** | 2.83e‑5 |
| stepdown‑stereo f2 | 470 @10 / 4.66e5 | 462 @10 / 4.66e5 | 0 | 0 | **6.93e6 / 2.72e6** | 5.42e‑6 |
| sweep‑stereo f10 | 19 688 @19 / 2.19e6 | 4.95e4 @19 / 2.19e6 | 0 | 0.05 | 2.99e6 / 3.27e6 | 1.81e‑5 |
| edge‑mono f4 (ctrl) | 5 965 @53 / 7.77e4 | INCONC (SHORT) | 1 @46 / 33.5 | 0.99 | 1.20e5 / 3.05e6 | 7.48e‑5 |
| noise‑mono f5 (ctrl) | 23 104 @30 / 1.55e6 | 2.32e4 @30 / 1.55e6 | 12 @17 / 647 | 97.1 | 4.49e4 / 3.29e6 | 3.37e‑5 |
| onset‑stereo f22 (ctrl) | 0 | 0 | 0 | 0 | 0 | 0 (spectrum all-zero) |
| onset‑stereo f23 (ctrl) | 0 | 0 | 0 | 0 | 0 | 0 (spectrum all-zero) |
| quiet‑sine f5 (ctrl) | 21 850 @10 / 3.45e6 | 2.17e4 @10 / 3.45e6 | 25 @10 / 4.37e3 | 29.3 | **1.21e8 / 1.25e6** | 9.57e‑6 |
| silence‑stereo f5 (ctrl) | 0 | 0 | 0 | 0 | 0 | 0 (spectrum all-zero) |
| stress‑stereo f5 (ctrl) | 25 758 @34 / 1.16e6 | 1.01e5 @26 / 2.16e6 | 1 @16 / 37.8 | 33.6 / 639 | 1.79e4 / 1.84e6 | 1.91e‑5 |
| tonal‑noise f5 (ctrl) | 386 @10 / 1.37e6 | 1.41e3 @10 / 1.37e6 | 0 | 0.043 | 2.01e5 / 3.38e6 | 5.17e‑6 |

Every threshold is met except the bold `MINSNR1` entries (and the `onset f24` blip entry, §8.3).
The single bold entry inside a **control** row (`quiet‑sine‑stereo f5`) is precisely why
`MINSNR1` is flagged rather than used as an attribution term. Four frames need no numeric
statement because their sum-like spectrum is exactly zero. `SPEC` is compared on sum-like
channels only; the difference-channel values are carried as report-only (`rel` 1.5e‑2 … 2.4e‑1
at the same frames), following the report-only rule of §4.3 item 3.

### 8.5 MDCT and transform-input comparisons for the divergent frames

The two transform groups have different strengths and different coverage:
`mdct_parity_with_reference` (`transform.rs:491-605`) is a **tolerance** gate
(`|Δ| < 5e-4·peak` per bin **and** median path-gain ratio inside `1 ± 1e-3`), whereas
`ring_schedule_matches_oracle` (`transform.rs:414-448`) and `synthesis_matches_oracle_input`
(`transform.rs:451-488`) are **exact-`i16`** identity checks. All three are `prod` gates. The
suite run recorded in §5.1 is green, so every frame the gates iterate passes; neither gate
prints a per-frame magnitude on success, so none is reported. These gates read the M2 dumps,
not the S1→S8 replay, so the blip defect does not affect them.

| divergent frame | `mdct_*` committed | inside MDCT gate | MDCT result | `ring_*` committed | inside ring gate | ring result |
|---|---|---|---|---|---|---|
| click‑mono f11 | ch0 | yes | passes | – | – | no reference |
| impulse‑mono f5 | ch0 | yes | passes | – | – | no reference |
| onset‑stereo f24 | ch0, ch1 | yes (ch0, ch1) | passes | ch0 | yes | **exact i16** |
| onset‑stereo f25 | ch0 | yes | passes | ch0 | **no** | inconclusive (file not iterated) |
| onset‑stereo f26 | ch0 | yes | passes | – | – | no reference |
| onset‑44100 f22 | – | – | no reference | ch0 | **no** | inconclusive (file not iterated) |
| sine‑stereo f1 | – | – | no reference | – | – | no reference |
| sine‑stereo f5 | ch0 | yes | passes | – | – | no reference |
| sine‑44100 f5 | – | – | no reference | – | – | no reference |
| stepdown‑stereo f2 | ch0 | yes | passes | – | – | no reference |
| sweep‑stereo f10 | ch0, ch1 | **no** | inconclusive (files not iterated) | – | – | no reference |

Summary: **7 of the 11** divergent frames are inside the MDCT tolerance gate and pass it; one of
those (`onset f24`) is also inside the exact ring gate and passes it bit-exactly. **Three**
frames have no MDCT vector (`onset‑44100 f22`, `sine f1`, `sine‑44100 f5`), **one** carries MDCT
files the gate does not iterate (`sweep f10`), and **two** carry `ring_*` files the ring gate
does not cover (`onset f25`, `onset‑44100 f22`) — those are reported **inconclusive**, not as
passes. No divergent frame shows an MDCT or ring *failure*.

---

## 9. Earliest gate-leaving stage — consolidated

| # | frame | kind | earliest **Class B** (value) departure | earliest **Class A** (exact) departure | attributed stage | C1–C6 |
|---|---|---|---|---|---|---|
| 1 | click‑mono f11 | positive | none in-gate | `S4SCF` (2/45, ±1) | S4 scalefactors from in-tolerance S1 residue | C2/C3 |
| 2 | impulse‑mono f5 | positive | none in-gate (`PENLD` INCONC) | `S4SCF` (3/52) | S4 scalefactors (same class) | C2/C3 |
| 3 | onset‑stereo f24 | positive | blip-coincident (INCONC) | `STATIC` 347 vs 329 | TNS decision, `w4/w6/w7` (pinned) | C2 |
| 4 | onset‑stereo f25 | positive | `MINSNR1` (flagged) | `STATIC` 200 vs 208 | TNS decision, `w0` (pinned) | C2 |
| 5 | onset‑stereo f26 | positive | `MINSNR1` (flagged) | `ATS pre` (0.63 %) + `MSD` 1 band | adaptive state entering the frame | C2/C6 |
| 6 | onset‑44100 f22 | positive | blip-coincident (INCONC) | `STATIC`/`ATS pre` IDENT; S4–S8 INCONC | TNS gain shift, `w6/w7` (pinned) | C2/C6 |
| 7 | sine‑stereo f1 | positive | blip-coincident (INCONC) | `STATIC`/`ATS pre` IDENT; S4–S8 INCONC | **inconclusive** | C6 |
| 8 | sine‑stereo f5 | positive | `MINSNR1` (flagged) | `MSD` band 16 | MS decision (recorded band‑16 flip) | C2 |
| 9 | sine‑44100 f5 | positive | `MINSNR1` (flagged) | `S4Q` 1/1024 | S4 quantizer 1‑LSB | C2/C3 |
| 10 | stepdown‑stereo f2 | positive | `MINSNR1` (flagged) | `STATIC` 208 vs 200 | TNS decision, `w0` (pinned) | C2 |
| 11 | sweep‑stereo f10 | positive | none in-gate | `ATS pre` (11 %) + `MSD` 2 bands | adaptive state; origin inconclusive | C6 |
| 12 | edge‑mono f4 | control | none | **none** | in-gate (identical payload) | C3 |
| 13 | multitone‑stereo f1 | control | blip-coincident (INCONC) | INCONC (blip); `STATIC` differs | TNS gain shift (pinned), harmless | C3 |
| 14 | noise‑mono f5 | control | none | **none** | in-gate | C3 |
| 15 | onset‑stereo f22 | control | none | **none** | in-gate | C3 |
| 16 | onset‑stereo f23 | control | none | **none** | in-gate | C3 |
| 17 | quiet‑sine‑stereo f5 | control | `MINSNR1` (flagged) | **none** | in-gate | C3 |
| 18 | silence‑stereo f5 | control | none | **none** | in-gate | C3 |
| 19 | stress‑stereo f5 | control | none | seq/geo **INCONC** (no reference); none else | payload identical; seq/geo undecided | C3 (+C6) |
| 20 | tonal‑noise‑stereo f5 | control | none | **none** | in-gate | C3 |

**Reading of the table.** "Class A" and "Class B" here are the *ladder* labels of §7; the
evidence class behind each cell is per §4.4 — for the exact integer decisions (S4/S5/S6) and for
`PAYLOAD` these are script comparisons against committed vectors, while `seq`/`MSD`/`ATS`/`STATIC`
and part of the S1 value ladder are backed by repository gates on the frames §4.4 names. For
every frame whose reference chain is faithful, the **value** ladder (S1/S2) stays inside the M3
tolerance definitions — the divergence is never a value-threshold violation. The first *exact*
comparison that fails is always one of: a pinned TNS decision (`STATIC`), the MS decision mask,
an S4 scalefactor/quantizer flip, or the adaptive state entering the frame. In the first three
cases the failing stage is *downstream* of the residue and is shown exact given identical inputs
by the oracle-fed replay gates; in the last case the failing quantity is not produced inside the
frame at all. `stress‑stereo f5` is the one control for which a ladder term cannot be decided at
all, and it is marked inconclusive rather than "pass" — its payload identity is unaffected.

---

## 10. Measured, derived, hypothesised, missing

### 10.1 Measured (new, this milestone)
* Per-frame production vectors for the 20 frames (`…/m526/out/stage_<name>_f<F>.txt`).
* Emitted ADTS for the 15 synthetic fixtures (`…/m526/out/rust_<name>.aac`).
* Real-C vs Rust payload comparison per target frame — a **script** comparison, not a
  repository gate (`…/m526/compare.py`, `analyse2.py`; §4.4).
* The blip criterion across the 20 frames and its agreement with D‑021 (§4.3).
* Suite parity and 15/15 byte identity of the instrumented build, with the limits of §5.1.

### 10.2 Derived
* Every verdict and magnitude in §8–§9, together with its evidence class — repository `prod`
  gate, oracle-fed `replay` gate, or milestone `script` comparison (§4.4).
* The MDCT/ring gate outcomes in §8.5, read from the green suite run of §5.1 (the gates do not
  print per-frame magnitudes on success).
* The observation that all Class‑B deviations at `SENL`/`THRV`/`SPREAD1`/`PENLD` coincide with
  the blip set (no non-blip frame shows one), which attributes them to the reference defect.
* The observation that `STATIC` differs **iff** the frame has a pinned **active-filter** TNS
  decision on a non-blip frame, and is **identical** for every pinned inactive/knife-edge
  decision; the two gain-shift pins are mixed (`onset‑441 f22` identical, `multitone f1`
  different but a blip frame with an identical payload).

### 10.3 Hypothesised
* `onset f26`: `f25` TNS (active filter) → `f25` PE → `chaosMeasureEnFac` update → `f26`
  `ATS pre` drift (0.63 %). Consistent with the measured state difference, but the
  intermediate update is not directly instrumented.

### 10.4 Missing evidence (chains stopped, reported `inconclusive`)
| Case | Missing reference | What would close it |
|---|---|---|
| `sine‑stereo f1` | no `psy_*`/`rxx_*`/`mdct_*`/`ring_*`/`ms_*` for f1, **and** its `qc1`–`qc8` replay is a blip | an oracle (non-blip) dump for `sine‑stereo f1` — i.e. the M2/M3 vector plus a faithful S1–S8 replay; the real encoder's own frame-1 side info would also do |
| `sweep‑stereo f10` origin | no oracle frame for `sweep‑stereo f0…f9` | a state snapshot (`ATS` weighting state, `DistributeState`) at the `f9→f10` boundary of the **real** encoder |
| `onset f24`, `onset‑44100 f22`, `multitone f1` downstream | oracle S1→S8 replay is a blip (D‑021, root cause still unisolated) | isolating the upstream short-block replay input that makes the oracle disagree with `aac-enc` |
| `MINSNR1` on 6/16 non-blip frames | no gate that distinguishes value from decision for `minSnr` | an oracle-fed replay of the `adaptMinSnr`/avoid-hole-init step for a control frame (e.g. `quiet‑sine f5`), or a recorded `ahFlag` dump |
| `PENLD` on SHORT frames | the C grouped-Ld dump uses a different pad/staleness convention | a `groupedELd` dump with the active-band/stride contract stated, or the M3 `psy_*` `nrg_ld` vectors used as the reference instead (already gated, §4.1 G‑psy-ELd) |

---

## 11. Ranked unresolved causes (for future investigation)

1. **`MINSNR1` out of the documented Ld envelope on 6/16 non-blip frames — including a
   byte-identical control.** Gaps run 1.7× to 190× the tolerance (up to 6.56e8 implied-energy
   units against a 3.44e6 tolerance); the pattern (Rust producing values at/below the
   `−10802119` floor where C produces raised values, and vice versa) points at the
   adapt-minSnr / avoid-hole-init step, but Rust's S4 does not consume `min_snr` at all and the
   control's payload is identical, so no decision impact is demonstrated. **Highest-value open
   item** because it is the only value-threshold term that fails anywhere in the set. Evidence
   class: measured + hypothesis.
2. **Blip root cause (D‑021).** Four of our frames (and ten corpus-wide) have an oracle replay
   that is not the real encoder. Until that is isolated, ~1/5 of the frozen frame set cannot be
   attributed downstream, and the short-block region is exactly where the audit expects the
   interesting behaviour. Evidence class: recorded + re-derived criterion.
3. **Cross-frame adaptive-state drift** (`onset f26` 0.63 %, `sweep f10` 11 %). Per-frame
   attribution is fundamentally limited for single-frame vectors; a state-trace over a frame
   *range* is needed. Evidence class: measured (state difference) + hypothesis (origin).
4. **Whether an active-filter TNS change ever moves a *coding* decision beyond the static bit
   count.** Measured here: `STATIC` moves for every pinned active-filter case. Not measured:
   whether `scf`/codebook/gain choices would have differed had the filter matched. Evidence
   class: measured consequence, open question.
5. **`S4SPEC` (mutated spectrum) exceeds the Q31 tolerance on a byte-identical control**
   (`noise‑mono f5`, max Δ 8144 vs tol 5990). Value-level only, no decision impact on that
   frame; recorded so it is not mistaken for a defect later. Evidence class: measured.
6. **`PENLD`/`PTHLD` conventions on SHORT frames** (pads written as 0 by C and as `INT_MIN`/
   values by Rust; the C grouped-Ld dump not equal to `Ld(SENL)` on MS-active bands). A
   documentation/reference-shape issue, not an encoder divergence. Evidence class: measured.

---

## 12. Recommendation for the next milestone (one)

**V3 — quality baseline on the 18-fixture corpus** (spec §6 metric list, deferred by the M4
plan §10 exit criterion 5). Rationale, stated in the audit's own terms: M5.26 was the gate
that decided whether a quality measurement is meaningful, and it found that **no value
threshold is exceeded** — the differences are within-tolerance frontend residue feeding a small
number of discrete decisions (TNS active filters, MS mask bands, S4 scalefactors/quantizers),
i.e. the documented numeric class. What remains unknown is the **magnitude** of those decisions,
and only a metric can bound it.

Scope for that milestone: delay-compensated (nDelay 1600) SNR/SDR of decoded C vs decoded Rust
output against the source PCM, average bitrate, and frame-size histograms per fixture and per
side; report deltas, never an audibility claim; no ABX. Pre-register the expectation before
measuring (C vs Rust deltas at or below the C encoder's own run-to-run/numeric residue on the
same fixture would support "no measurable impact on this corpus"), and record the raw decoder
logs (closing audit gap G4).

Do **not** fold the §11 items into that milestone: item 1 needs an oracle-fed probe, item 2 an
oracle-replay investigation, item 3 a state trace. They are separate, smaller milestones.

---

## 13. What this milestone does *not* establish

1. Nothing about audio quality or perceptual equivalence; no metric was computed.
2. Nothing about standards conformance of either stream (no independent parser check).
3. No claim that any observed difference is a defect — the report deliberately states where a
   difference follows from an in-tolerance upstream residue, and where the reference itself is
   unfaithful.
4. No claim about out-of-contract behaviour (panics, PNS/IS, retries) — not exercised here.
5. No performance statement.

---

## 14. Source evidence index

**Repository (read-only in this milestone)**

* Gates and tolerances: `rust/src/aac_enc/psy.rs:1496-1560,1681-1850,1863-1872,2212-2227,
  2816-2930,2932-2947,2950-3000`; `energy.rs:120-159,205-224`; `tns.rs:700-720,936`;
  `transform.rs:414-448,451-488,491-605`; `scf.rs:1418-1650`; `bitcount.rs:1291-1650`;
  `rate.rs:1776-1950,2149`; `bitenc.rs:1144,1228`; `frame.rs:1395`;
  `stream_driver.rs:801,951-1119,1121-1260,1632-1707,1862-1891,1947-1970,1971-2067,2069-2160,
  2585-2720,2794-2953,2964-3053,3172-3570,3878,3922-3923,3937,4038-4130`.
* Oracle semantics: `tools/enc-oracle/README.md:137-169` (output format), `:170-202` (S1),
  `:203-233` (S2), `:234-271` (S3), `:285-324` (S4), `:325-363` (S5), `:376-426` (S6),
  `:479-507` (S7), `:541-572` (S8); `tools/enc-oracle/enc_oracle.cpp:1060-1418` (S1 stage and
  its `S1FRAME` dump block).
* Committed C ground truth: `rust/src/aac_enc/testdata/qc1..qc8_<name>_f<F>.txt`,
  `psy_*`, `rxx_*`, `mdct_*`, `ring_*`, `ms_*`, `seq_*`.
* Prior findings cited: `docs/aac-enc-quality-divergence-audit-m5.25.md` (§3.5 D‑030, §4.A,
  §4.B, §6.1 G2/G3/G7, §7.1, §8 V1/V3/V6); `docs/aac-enc-m4-quant-ratecontrol-plan.md`
  §7.3 (comparison policy — defers Ld/Q31 values to the existing M3 gates), §8, §17.3–§17.5;
  `docs/aac-enc-migration-spec.md` §5, §6, §11–§11.5.

**Preserved runtime evidence**

* C outputs: `…/T/opencode/phase0-20261015/aac/*.aac`; oracle dumps
  `…/T/opencode/phase0-20261015/oracle/*.s1.txt`; reference binary md5
  `850a8e6ed56641f453dc12b3c83946f5`.
* Pinned Rust reference outputs: `/var/folders/…/T/m52-rust-aac/m58-corpus/*.m58.aac`
  (hashes also in `…/T/opencode/m522/{base,after}_hashes.txt`).
* Preservation backup: `/Users/jeroen/backups/fdk-aac-m5.23-worktree-20261016T193606Z`.

**This milestone's temporary artefacts (outside the repository)**

* Instrumented copy `…/T/opencode/m526/work` (only `stream_driver.rs` differs, +407/−1;
  instrumented sha256 `966b6d79…`, repository sha256 `b499fb3c…`).
* Uninstrumented control copy `…/T/opencode/m526/base`.
* Stage dumps + emitted ADTS `…/T/opencode/m526/out/` (35 files).
* Scripts: `…/T/opencode/m526/{compare.py,analyse2.py,summary.py,coverage.py,extract_pins.py,
  magnitudes.py,specdump.py,blipnum.py,diag_align.py}`, results in `analysis.txt`, `report.txt`,
  `analysis.json`.
* Reproduction: `M526_OUT=…/m526/out cargo test --features aac_enc m526_dump_cases -- --nocapture`
  in the instrumented copy, then `python3 compare.py` and `python3 analyse2.py`.

---

## 15. Final status of this milestone

* **Deliverable:** this file only. No encoder, test, fixture, oracle-vector, harness,
  reference-C or existing-report file was modified; nothing was staged or committed; the M5.23
  backup was not written to. All instrumentation, scripts, dumps and outputs live outside the
  repository under `…/T/opencode/m526/`.
* All 20 specified cases are accounted for: 8 non-blip positives each with a named earliest
  failing exact comparison and measured magnitude; 3 blip positives, of which 2 (`onset f24`,
  `onset‑44100 f22`) are attributed to their pinned TNS decisions and 1 (`sine‑stereo f1`) is
  carried as fully inconclusive with the missing reference named; 8 non-blip controls with no
  failing deciding comparison and a byte-identical payload — of which `stress‑stereo f5` is
  inconclusive on `seq`/`geo` for missing reference data, with its payload identity preserved;
  1 blip control (`multitone‑stereo f1`) with an identical payload.
* Behaviour preservation: **no evidence of perturbation** on the covered fixtures and paths
  (15/15 byte-identical outputs; suite parity with an independent uninstrumented build), with
  the coverage and dump-correctness limits stated in §5.1.
* Citation/evidence-class revision (M5.26a, 2026‑10‑17): the gate table now names the enforcing
  assertions and the gate kind, per-frame payload identity is labelled a script comparison
  rather than a repository gate, the S1 tolerance provenance is corrected to the M3 helpers,
  the MDCT gate is reported as a tolerance gate with per-frame coverage, the instrumentation
  conclusion is bounded, and `stress‑stereo f5`'s sequence/geometry terms are marked
  inconclusive. No finding, classification, frame identity, blip exclusion or conclusion was
  changed.
* Plain-language summary: **where the Rust encoder and the C reference differ, the Rust
  frontend's slightly different numbers stay inside every tolerance the project defined; what
  changes the payload is a handful of discrete decisions taken just past those tolerances — a
  TNS filter turning on or off, a mid/side bit flipping, a scalefactor or quantizer line
  moving by one — plus, on two frames, adaptive state carried in from an earlier frame that
  has no oracle reference. Of the **eight** non-blip frames that do *not* differ, **seven** are
  bit-identical on every deciding comparison from window sequence to payload, while the eighth —
  `stress‑stereo f5` — has its payload identity established but its sequence/geometry comparison
  remains **inconclusive** for missing reference data. What has still never been measured is how
  large any of this is in audio terms.**
