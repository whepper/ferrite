# D-021 closure record

## 1. Disposition and scope

**D-021 is closed as an expected numerical difference, with no independently
demonstrated defect in the investigated path** (SHORT-frame VBR-5 attack
onsets, four fixtures, zero-based frames 24/22/1/1).

This is explicitly NOT a claim of bit-identical output, NOT a proof of
perceptual equivalence for all inputs, and NOT a finding that no numerical
difference exists — measured differences up to +28 B on attack frames are real,
fully accounted, and root-caused to finite-precision MDCT arithmetic with
C-side dominance. What was closed is the defect question: twelve reports over
V4.22–V4.36 found no evidence of any decision-rule, accounting, or mapping
defect in the Rust encoder on the investigated path. See §6 for what remains
uncertain and §8 for what would reopen the issue.

The narrower independent TNS review (GLM-5.3-Flash, `reviews/` in this
archive) closed only the TNS-order subsystem; the overall D-021 closure rests
on the full V4.36 audit, not on that review alone (§5).

## 2. Investigation timeline (V4.22–V4.36)

- **V4.22–V4.23** — replay-history and SHORT-validation baselines; fixture and
  harness groundwork.
- **V4.24** — first-divergence audit (`PARTIALLY LOCALIZED`): MDCT-after-lowpass
  match shown; S6-ratio prose later corrected (onset441 is 82–84%, not 25–35%);
  pre-MS proof gap named. Invalidated in part: oracle SHORT-frame MS staging
  later proven stale — oracle S6 totals (792/1984/728/600) reproduce on neither
  encoder and were excluded from all later ground truth.
- **V4.25** — passive real-C pre/post-MS/S1 captures with byte-identical
  instrumented builds; MS decision proven as the first mask-level mismatch
  (group-3 high bands = oracle staleness, not live divergence).
- **V4.26** — live-mask proof: 3/4 masks exact; onset f24 bands 38,39 differ by
  exactly 1 LSB of stored mid-energy, with an exact counterfactual reproducing
  the live MS margins bit-for-bit. No MS-rule defect.
- **V4.27** — live frame reconciliation: live-vs-real ADTS +28/+7/−1/+0 B;
  payload/ADTS discipline established (`total+56 == len·8` all four cases).
- **V4.28** — live localization: 372-line ch0 spectral residue beyond the
  energy envelope; +18 static remainder hypothesized as TNS side-info.
- **V4.29** — real-C S4/S5/S6/TNS capture: +224 b = static +18 (TNS w4 order
  5-vs-2, +9/+9 exact bit math) + dyn +205 (ch0 huff +189/scf +23/side −7)
  + align +1; +56 b = 0 + 60 − 4. Status: contributions reconciled.
- **V4.30** — live RXX differs 40/208 per channel (w3–w7); merge proven
  bit-exact on reference inputs ⇒ mismatch precedes RXX; Schur not implicated.
- **V4.31** — live detect-input Q31 slices differ 509/1024 per channel;
  w0–w2 trivially zero both sides; recipe boundary validated by construction.
- **V4.32** — M3 f32 outputs already differ (~1e-4); Q31 conversion proven
  exact (ratio 4.0; decimal-parse pitfall documented and avoided).
- **V4.33** — ring inputs bit-equivalent (2048/2048 vs committed C ring; PCM
  mapping exact); window selection SHORT/Sine-128 both sides ⇒ divergence
  produced inside windowing/MDCT computation.
- **V4.34** — slope table == f32(C `SineWindow128`) 0/64; persistent state is
  reference-only (no numeric state exists); coefficient effect bounded 3.1e-8
  vs 4.7e-4 observed ⇒ coefficients/state cleared, arithmetic remains.
- **V4.35** — exact-arithmetic bracketing (live ≤4.7e-6, C ≤4.7e-4 from exact);
  executed-C `mdct_block` harness reproduces the reference 0/1024; stage
  budget fold 1.7e-5 / pre-twiddle 5e-6 / FFT ~1e-4 (×4 post-gain). Verdict:
  finite-precision arithmetic explains the gap; no defect either side.
- **V4.36** — downstream audit: 17-link causal chain classified by evidence
  type; four cases reconciled; closure recommended (this record implements it).

Hypotheses invalidated along the way: oracle S6/MS as ground truth; MS side
info as static source; S5/S6 allocation or crash/fill as independent causes;
grant-history divergence (onset441 grants exact); coefficient/state causes;
any Q31-conversion defect.

## 3. Causal chain (labels: measured / exact-on-equal-inputs / supported / inferred / unresolved)

Ring input [measured equal, V4.33] → windowing/MDCT [measured residue ~1e-4,
root cause: finite precision, C-dominated, V4.35] → TNS detect inputs
[measured differ 509/1024, V4.31] → RXX [measured differ 40/208; merge exact
on equal inputs, V4.30] → TNS order w4 5-vs-2 [supported but not
counterfactually proven; robustness at w3/w5/w6/w7; rules mirror C] →
static +18 [exact bit math, V4.29] and TNS-filtered spectra [inferred
interaction: C filters in place and recomputes energies, magnitude unsplit] →
S1 MS flip at 38,39 [supported by exact counterfactual, V4.26] and S4
skip-branch flip at 38,39 [supported by code branch + exact I/O states, V4.29]
from the 1-LSB energy residue [measured; its own provenance inferred] →
S4 scf-search divergence, 8 bands per onset case [supported: inputs differ,
outputs differ; rule corpus-exact on equal inputs] → S5 section/huff/scf/side
splits [exact accounting, V4.29] → grants/loop non-binding, align by formula
[measured] → frame lengths [arithmetic]. Correlation was never upgraded to
proof: each supported link cites counterfactual, branch, accounting,
reproduction, or elimination evidence (V4.36 §2).

## 4. Four-case reconciliation (payload bits; ADTS = payload + 56)

- **onset-stereo f24:** live 2640 → 337 B vs real 2416 → 309 B.
  Δ +224 b = static +18 + dyn +205 + align +1. Fully reconciled.
- **onset-44100 f22:** live 2424 → 310 B vs real 2368 → 303 B.
  Δ +56 b = 0 + 60 − 4 (grants/TNS/static exact). Fully reconciled.
- **sine-stereo f1:** live 1960 → 252 B vs real 1968 → 253 B.
  Δ −8 b bounded to S5-dyn-or-fill (real split unavailable); all S1 counters
  exact (PE 3488 both, static 479, masks exact). Bounded, not fully split.
- **multitone-stereo f1:** 1728 → 223 B both sides; static/mask exact, PE Δ2.
  Fully reconciled.

## 5. Independent review (GLM-5.3-Flash TNS review)

The review (`reviews/GLM-5.3-Flash-TNS-Review.md`, found in place during
archival) covers ONLY the TNS order divergence (C order 2 vs live order 5,
f24 w4) and its immediate causal path. Verdict: `CLOSE TNS ISSUE —
NUMERICAL DIFFERENCE IS SUFFICIENTLY EXPLAINED`. It independently re-derived
the +9/channel bit math, verified the border constant against both sources
(`sfb_tables.rs:247` = C ROM `0xe9c5e578`; both confirmed present during
archival), and recorded an M3-era swap counterfactual (Rust rules on C input
→ order 2; parcor[4] straddling the border) whose provenance it flags as
transcription-only, not a preserved D-021 artifact. It explicitly warns that
the TNS issue closing does not close D-021's other links (the +205 dyn
majority, mask/skip mechanisms, sine remainder). This closure record follows
that scoping: the review corroborates, but does not substitute for, the V4.36
full-chain audit. Provenance limitation: the review file's authoring actor is
unknown to the archivist (found, not created, during archival); its figures
were spot-checked against in-repo pins (`psy.rs` `KNOWN_TNS`: want
order-2/gain-10227 vs got order-5/gain-10317) and check out.

## 6. Residual uncertainties (non-blocking, with reasons)

Per V4.36 §5, preserved verbatim in substance: (a) RXX→order lacks a preserved
swap experiment — mitigated by cross-window robustness and exact rule mirrors;
(b) TNS-filter→S4 magnitude unsplit — both candidate paths are numeric
propagation of one measured residue, and the bit account is exact either way;
(c) sine 8-bit split missing — 0.4% with exact upstream and no defect signal;
(d) within-FFT apportion and post-twiddle op path untraced — mechanism detail
below an already-quantified stage budget; (e) no LONG-block evidence;
(f) committed-fixture provenance rests on reproduction, cross-checked against
the real `aac-enc` binary at six independent boundaries; (g) direct C
TNS-detect-input capture never existed (transient stage). None is positive
evidence of an independent defect, which is the standard the keep-open options
require — hence they do not block closure, and erasing them would
misrepresent the evidence.

## 7. Acceptance rationale

Twelve reports, six independent equal-input exactness checks (MS mirror, S4
corpus, RXX merge, dctiv, MDCT parity, grant/static agreement), two executed
reproductions (C harness 0/1024; exact-arithmetic bracketing), three exact
counterfactuals/accountings (MS 38,39; TNS +18; frame totals), and zero rule
mismatches on equal inputs anywhere tested. The remaining divergences all sit
at marginal/threshold decisions with measurably differing inputs, with signs
and magnitudes consistent with residue amplification through quantization
(+224/+56/−8/0; PE signs opposing frame signs; no systematic bias). Without
new contradictory evidence or a concrete independent defect, further
TNS/D-021 probing cannot change the disposition — unexplained numerical
residue alone does not establish a defect.

## 8. Reopening criteria

Reopen D-021 only on concrete new evidence, for example: a reproducible
discrepancy on equal inputs violating a verified decision rule; proof that a
cited capture, fixture, or hash in this archive is incorrect; a source-level
mismatch in a supposedly identical rule; a downstream discrepancy that cannot
be reconciled under the documented numerical differences. These are examples,
not claims that such evidence exists.

## 9. Source revision and integrity

- Investigation revision (verified): `e4258d2a5b55581f5f2b27aa6c1a369bb2503e43`.
- Archival-time revision: `e4258d2a5b55581f5f2b27aa6c1a369bb2503e43` (unchanged).
- No production changes were made by the investigation (test-only probes, all
  removed with hash-verified restoration) or by this archival task (copies
  only; nothing staged, committed, stashed, reset, or cleaned).
- Pre-existing worktree changes (recorded in the archival working notes, left
  untouched) are not attributed to D-021.
- Reports: `reports/` in this archive. Independent review:
  `reviews/GLM-5.3-Flash-TNS-Review.md`. Manifest: `manifest.json`
  (verification procedure in `README.md`).
