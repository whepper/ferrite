# Independent Review — D-021 TNS Order Divergence (onset-stereo f24, w4)

- **Reviewer model:** GLM-5.3-Flash (zai-coding-plan)
- **Date:** 2026-10-20
- **Scope:** TNS order divergence only (C order 2 vs live Rust order 5, onset-stereo frame 24 zero-based, window 4) and its immediate causal path. No audit of unrelated encoder subsystems.
- **Constraints honored:** no bit-identical-output demand; no unexplained-difference-as-defect reasoning; no speculative fixes; no listening tests; no new instrumentation or execution — existing evidence only.

**Provenance note:** the V4.29–V4.36 reports were located and read in full from
`/private/var/folders/.../T/opencode/v4.{29..36}-*/`, together with their raw artifacts
(`/tmp/v429-bitattr/`, `/tmp/v435-charness/`, `/tmp/d021-mdct-capture/`, live-probe logs).
Claims were additionally verified directly against the C and Rust sources where static
verification was possible. No encoder was re-run.

## Verdict

**`CLOSE TNS ISSUE — NUMERICAL DIFFERENCE IS SUFFICIENTLY EXPLAINED`**

## Evidence table

| # | Ref | Confirmed fact (verified against) | Status |
|---|-----|-----------------------------------|--------|
| 1 | V4.29 report + `/tmp/v429-bitattr/onset-stereo_f24_bitattr.txt` | Real C frame 24: `static=329, usedDyn=2084, align=0, total=2416`; TNS W4 **order 2, coef [3,1]** both channels; instrumentation bit-transparent (base==instr==V4.25 hashes) | **Measured [M]** |
| 2 | `stream_driver.rs:2752` `phase1h_d021_live_frame_reconciliation` | Live Rust f24: 337 B, total 2640, static 347, dyn 2289 — pinned in a committed test | **Measured [M]** |
| 3 | V4.29 §4–5 | +224 = +18 static +205 dyn +1 align, exactly; +18 = TNS w4 only: order 2→5 ⇒ 2×3b=6 → 5×3b=15 coef bits, +9/ch × 2ch; all other static syntax byte-identical (hand-checked per-filter sums) | **Exact accounting [A]** — 9-bit/channel math re-derived from the C bit-count formula; correct |
| 4 | `psy.rs` `KNOWN_TNS` (f24 pins) | w4 ch0/ch1: want [2, 0]/[3,1]/10227 vs got [5, 0]/[3,1,0,0,−1]/10317; w3/w5/w6/w7 orders+coefs **identical** despite differing RXX (gains differ only) | **Measured [M]** |
| 5 | V4.30 + `tns.rs` `fixed_merge_matches_rxx_bit_exact` | Live RXX ≠ committed RXX at w3–w7 (40/208 elements); merge proven **bit-exact on reference inputs** corpus-wide incl. this frame ⇒ merge not implicated | **[M]+[E]** |
| 6 | V4.31 | Live detect-input Q31 slices differ on 509/1024 lines/ch (w3–w7), max 5688 vs ~9e7 signals; w0–w2 exact zeros both sides | **Measured [M]** |
| 7 | V4.32 | Q31 conversion chain exact (ratio 4.0 on 504/510 nonzero lines; 6 exact-.5 lines = Python-vs-Rust rounding); residue present at M3 MDCT output (≤4.7e-4) | **Measured [M]** |
| 8 | V4.33 | Ring input bit-equal (2048/2048 i16 vs committed C ring), PCM mapping exact, window seq/shape/slope equal ⇒ divergence produced inside window/MDCT computation | **Measured [M]** |
| 9 | V4.34 | Sine slope table == f32(C `SineWindow128`) 0/64 differ; no persistent numeric state; coefficient effect bounded 3.1e-8 ≪ 4.7e-4 | **Measured [M]** |
| 10 | V4.35 + `/tmp/v435-charness/` | Executed real C `mdct_block` harness reproduces committed reference **0/1024** (`harness.cpp`, `c-out.txt`: NSPEC 1024); offline f64 MDCT matches live Rust within 1.7e-6–4.7e-6; stage budget fold ≤1.7e-5 / pre-twiddle ≤5.2e-6 / FFT ≤1.1e-4 (×4 post-gain) ⇒ gap 4.696e-4 = C(4.7e-4) ⊕ live(4.7e-6) | **Measured [M]** |
| 11 | spec §11 (M3, 2026-10-14) | Three-way trace: Rust rules on **C input** reproduce C decision (order 2, coef indices bit-exact, gain 10229 vs committed 10227 — inside gate); on live input → order 5, gain 10317; parcor[4] −0.171577 (C, 2.1e-3 above border) vs −0.174151 (Rust, 0.5e-3 below border) | **Recorded counterfactual; transcription only — not a preserved D-021 artifact (V4.36 names this gap)** |
| 12 | Reviewer source check | `TNS_COEF_3_BORDERS[4]`: Rust `sfb_tables.rs:247` = −0.17364501953125 = C ROM `0xe9c5e578` after C's round-to-nearest `FX_DBL2FXCONST_SGL` macro (`common_fix.h:160`) — **exact** (neighbors −0.5, 0.2225341796875 also exact). Thresholds 1437/1500, strict-`>` border search, trailing-zero order truncation, `sumSqr > limit/2+2` activation, LO long-only gating, sync rules: **structurally identical** C `aacenc_tns.cpp:766-945,1141-1173` vs Rust `tns.rs:656-800` | **Verified [E]** |

## Answers

**Q1 — MDCT residue as explanation.** Yes, and the upstream chain is demonstrated, not
merely plausible: ring inputs bit-equal (row 8) → measured MDCT gap fully budgeted to
finite precision, C-dominated, with the executed C pipeline reproducing the reference
exactly (row 10) → exact conversion → measured detect-input divergence → deterministic
exact merge → RXX divergence. The recorded parcor[4] values put the two sides on opposite
sides of a border whose exact value was verified against both sources (row 12), and the
direction/magnitude (3.4e-4 RXX residue → ~2.6e-3 parcor shift vs 0.5e-3 margin) is
quantitatively coherent. Weakening factor: the final-step intermediates and the
on-C-input counterfactual exist only as M3-era transcriptions (row 11), not as preserved
D-021 swap artifacts.

**Q2 — Independent TNS defect.** None found, and the check was specific: algorithm
structure, thresholds, border tables (bit-exact), search/rounding rule, order truncation,
sum-sqr accumulation, iteration/merge/sync semantics all match (rows 4, 12). The one
genuine lane difference — float Schur instead of C's `schur_div` fixed-point recursion —
is a documented design decision whose equal-input behavior is evidenced: gains within the
corpus gate (in-tree test), and bit-exact coefficient indices on equal inputs recorded at
M3 *including this exact window* (row 11). The gain tolerance gates values only;
active/order/coefficient checks stay exact, so a decision flip cannot hide behind it.
A defect would also have to explain why the same rules match C at w3/w5/w6/w7 of the very
same frame with equally-diverged inputs (row 4) — that is the signature of a marginal
input, not a broken rule.

**Q3 — Boundary crossing: demonstrated or inferred?** Demonstrated: differing detect
inputs [M]; exact deterministic merge propagates them necessarily [E]; identical rules
produce order 2 on C input and order 5 on live input, with recorded parcor[4] straddling
the verified border [recorded M3 counterfactual + live pins]. Still inferred in the
strict sense: the swap experiment is not preserved in the D-021 artifact chain
(V4.36 §4 correctly says "not causally demonstrated" at its artifact standard), and
parcor[4] cannot be re-derived statically. Assessment: the mechanism is demonstrated to
the standard achievable without new execution — every statically checkable dependency
(border constant, bit math, decision rules, recorded gains) verifies exactly. V4.36's
"[P]" label reflects artifact preservation, not a contradiction with the M3 trace.

**Q4 — +18-bit attribution.** Yes, and it is exact: 6 → 15 coefficient bits per channel
at coefRes 3, ×2 channels = +18, matching captured 329 vs pinned live 347, with every
other static syntax element byte-identical (V4.29 §5). But the accounting only prices the
difference; it is silent on which decision is correct — correctness rests on rows 5–12,
not on the bit ledger.

**Q5 — Strongest argument against closing D-021 on this TNS issue alone.** The TNS flip
is only +18 of +224 bits and one of four demonstrated residue-driven splits in this frame
(MS mask flip at 38/39 with a bit-exact counterfactual, S4 skip-branch flip, six-band
scf-search divergence). The +205 dynamic majority, the mask/skip mechanism, and sine f1's
unsplit 8-bit remainder are separate links requiring their own evidence; none of them is
discharged by explaining the TNS order. The TNS issue can close; D-021's disposition must
rest on the whole chain (as V4.36 in fact assembles it), not on this subsystem.

## Single most important reason for the verdict

Every stage of the causal chain from MDCT residue to the order flip is either directly
measured or proven exact where static verification is possible — including the border
constant, the +18-bit math, and the fact that the identical Rust decision rules reproduce
C's order-2 decision when fed C's input — so the order-5 outcome is the necessary
consequence of measured input divergence under verified-identical rules, leaving no
positive evidence of an independent TNS defect and one direct counterexample against
that hypothesis.
