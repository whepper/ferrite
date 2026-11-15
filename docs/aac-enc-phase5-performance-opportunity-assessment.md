# Ferrite Phase 5 — Structural Performance Opportunity Assessment

- Date: 2026-11-15. Status: **investigation and assessment only; no encoder change made.**
- Question answered: where can Ferrite eliminate enough work to become meaningfully
  faster, rather than executing its existing work a little faster?
- Short answer: **one candidate clears the 5% bar** — fusing the S4 winning
  quantization directly into the QC gain loop's first pass, deleting the
  duplicated S5 quantization/bit-count stage and QC's unconditional
  re-quantization (estimated 4–7% end-to-end, medium confidence). No second
  candidate clears 5%; this is stated explicitly in §9 rather than filled
  with sub-threshold work.

## 1. Executive summary

| # | Candidate | Est. end-to-end | Confidence | Complexity | Verdict |
|---|-----------|-----------------|------------|------------|---------|
| R1 | Single-quantization S4→QC pipeline (delete dead S5 stage, seed QC pass 0 from S4 winners, early-out fallback) | **4–7%** (up to ~8% on music) | Medium | Medium-high | **Recommend for follow-up** |
| R2 | Single-traversal bit count + spectral encode | 2–3% | Medium | Medium-high | Deferred runner-up, below bar |
| R3 | Production capture/copy hygiene (`MsDecisionCapture`/`S1Cap`, PCM double clone) | ~1% | High | Low | Hygiene, not a campaign |
| R4 | Intensity-stereo decision pruning on IS-free content | ≤2% | Low-medium | Medium | Rejected (below bar) |
| R5 | Channel-level parallelism (2 threads, stereo) | Uncertain, 0–10% | Low | High | Not recommended |

The ranking criterion is credible end-to-end improvement first (§7 of the
task brief). Only R1 is recommended as a follow-up campaign. R2 is the
nearest runner-up and is explicitly below the 5% threshold.

## 2. Repository and implementation scope inspected

- Repository: `https://github.com/whepper/ferrite.git`, branch `main`.
- HEAD: `5b60c7f` ("test(aac-enc): preserve VBR-5 benchmark harness support").
- Upstream sync: `HEAD` equals `origin/main`; `git rev-list --left-right
  HEAD...@{u}` is `0 0` — both expected commits (`6025637`, `5b60c7f`)
  are present and pushed.
- Starting worktree: **clean** — `git status` reports nothing to commit,
  no untracked files.
- No encoder, test, fixture, benchmark, or workflow file was modified. No
  commit or push occurred. No profiling campaign was run.

Existing evidence read (not re-measured):

- `docs/aac-enc-performance-baseline.md` in full (§§1–39), including the
  retained-optimization history (§§11–29, aggregate checkpoint §29 at
  **+13.93%** over the pre-optimization baseline), the deferred small-gain
  candidates (§§14–28, notably the rejected per-band LRU-8 S4 cache in §26
  and the rejected SCF trial-array reuse in §28), and the SIMD record
  (§§30–39, ARM64 NEON A/B result §39).
- `docs/aac-enc-simd-performance-plan.md` (operative protocol §8A; the
  superseded 1%-gate methodology retained as history).
- `docs/aac-enc-vbr5-profile.md` (VBR-5 configuration, pipeline, C/Rust
  correspondence; corpus IS-free result used in §6/R4).
- `docs/aac-enc-m5i-pass-margin-census.md` and
  `docs/aac-enc-m5h-retry-crash-trigger-map.md` (via prior record: 0
  multi-pass / 0 crash frames in 2,442 corpus frames; `iterations` max 0/5).
- `docs/evidence/aac-enc-performance/profile-top-self.tsv` (per-workload
  symbol shares).

Implementation read directly (read-only; the findings below rest on this,
not only on historical profiles):

- `rust/src/aac_enc/stream_driver.rs` — `frontend` (795–995), `step_frame`
  pipeline: IS (1044–1070), M/S (1081–1163), form-factor/PE (1168–1253),
  S2 (1255–1325), S4 (1327–1367), **S5 (1369–1405)**, S6/QC inputs
  (1407–1458), S8/frame assembly (1544–1673).
- `rust/src/aac_enc/scf.rs` — `improve_scf` (421–482),
  `assimilate_single_scf` (489–634), `assimilate_multiple_scf` (638–767),
  `assimilate_multiple_scf2` (771–1076), `estimate_channel_with_inv_ws`
  (1142–1413).
- `rust/src/aac_enc/quant.rs` — `quantize_spectrum` (396–489),
  `calc_sfb_dist` (535–659), `calc_sfb_quant_energy_and_dist` (661–754).
- `rust/src/aac_enc/rate.rs` — `qc_main_frame` (1441–1802), MAX_QUANT loop
  (1586–1679), per-channel recount (1680–1713), exit matrix (1742–1762).
- `rust/src/aac_enc/bitcount.rs` — `dyn_bit_count_reuse` (1267–1305),
  `build_bit_lookup`/`bit_count` (536–771), `gm_stage0–2` (856–924).
- `rust/src/aac_enc/bitenc.rs` — `code_values` (416–652),
  `encode_spectral_data` (818–850).
- `rust/src/aac_enc/intensity.rs` — early return (402–403), decision and
  processing (195–596).
- `rust/src/aac_enc/psy.rs`, `energy.rs`, `pe.rs`, `threshold_vbr.rs`,
  `encoder.rs`, `pcm.rs` — data-flow and allocation structure.

## 3. Existing performance evidence considered

Accepted prior results (not revisited):

- ARM64 NEON VBR-5 end-to-end **+14.576%** (16/16 pairs, baseline §39).
  Combined NEON kernels plus removal of the unused scalar lookup; no new
  reason to revisit was found.
- Retained scalar set at **+13.93%** aggregate (baseline §29).
- AVX2 x86-64 performance remains **unmeasured** (blocked campaign,
  `6025637`); no AVX2 claim is made here.
- Deferred small-gain work stays deferred: per-band LRU-8 cache (bit-exact,
  −0.66% median, §26), SCF trial-array reuse (+0.246% median, §28),
  `accumulate_dist` shift specialization (identical codegen, §22), TNS
  transpose (−1.56%, §20). None is reopened.

Phase 3 shares used as the quantitative frame (task brief; ARM64 M5, VBR-5):

| Area | Share |
|---|---:|
| S4 scalefactor/quantization search | 45.7% |
| Front end | 24.8% |
| Bit counting/Huffman | 10.5% |
| Memory copies and allocation | 7.2% |
| Feeder | 4.3% |
| Rate/quality control | 2.7% |
| Other | 4.9% |

Individual hotspots confirmed against `profile-top-self.tsv` and source:
`estimate_scale_factors_channel_reuse`, `calc_sfb_dist_sum_neon`,
`calc_ld_data`, `dyn_bit_count_reuse`, `encode_spectral_data`/`code_values`,
`form_factor`, `autocorr_quarter_lags`, `BlockSwitch::process`,
`intensity_stereo` (2.0–3.3% self across workloads), `dit_fft`. Gain-loop
retry rate is effectively zero on the corpus (m5h: `npass==1` on
2442/2442 frames; m5i: `iterations` max 0/5), so QC steady state is exactly
one pass.

## 4. Architectural findings

### F1. The S5 stage computes results the QC gain loop unconditionally discards

`step_frame` runs an "S5: QMAX + sectioning" stage
(`stream_driver.rs:1369–1405`): per channel, `calc_max_value_in_sfb` on
S4's `quant_spec`, then `dyn_bit_count_reuse` on the same inputs, pushing
`(qmax, sections)` into locals. The S6 QC input block (1409–1458) then
passes only spectrum/scf/gain/geometry — **not** the S5 outputs — and
`qc_main_frame` unconditionally re-quantizes (`rate.rs:1527`
`recalc=true` initially; first inner iteration always runs
`quantize_spectrum` at 1647, `calc_max_value_in_sfb` at 1661, and
`dyn_bit_count_reuse` at 1698), overwriting `sections[ch]` (1699) and
`maxes[ch]` (1661–1668). A source-level use search shows the S5 locals
`qmaxs`/`sections` are pushed and never read again. The S5 per-frame cost
— one full `calc_max_value_in_sfb` plus one full `dyn_bit_count_reuse`
per channel — is therefore **pure duplication**, independent of any
equivalence argument. (Why it exists is plausibly migration-stage
bookkeeping — S5 mirrors a C module boundary — but C performs this work
inside `QCMain`, so the Rust split executes it twice.)

### F2. QC pass 0 re-quantizes the spectrum S4 just quantized, with identical parameters

S4 stores per-band winners: `scf[s]` (pre-inversion) with `quant_spec`
written band-by-band by `calc_sfb_dist` trials. After global-gain
inversion (`scf.rs:1373–1390`: searched bands get `scf = gmax − scf`;
unsearched bands get `scf = 0` with the spectrum zeroed), QC pass 0 calls
`quantize_spectrum(..., global_gain=gmax, scf, ...)` (`rate.rs:1647`),
which computes per band `qss = gmax − scf'[s]`. For searched bands
`qss = gmax − (gmax − scf[s]) = scf[s]` — the exact gain S4's winning
trial used, through the same per-line `quantize_line_body`, on the same
active-band spectrum. For unsearched bands both sides quantize zeros to
zeros (S4 never wrote them; QC sees the zeroed mutated spectrum).
`quantize_spectrum`'s only production caller is `rate.rs:1647` (other
call sites are tests), so QC pass-0 quantization (4.2% aggregate self,
5.1% on music) is, in the common case, a line-for-line repeat of S4's
stored winners. The one divergence class is the `MAX_QUANT` early-out:
`calc_sfb_dist` stops at the first `|q| > 8191` line leaving a zero tail
(`quant.rs:652–653`), while `quantize_spectrum` has no early-out and
writes full magnitudes. Per the committed production census
(`quant_bench.rs` clean-cell assertions) this never fires on the corpus,
but any fusion must preserve the QC safety net for untested content —
hence R1's fallback design (§6).

### F3. S4's speculative trials dominate its cost, but no decision-preserving cut was found

Per live band per channel, S4 performs ~2–3 `improve_scf` evaluations
plus up to ~60 finer trials per `assimilate_single_scf` visit (with
restart-on-success), a full `gmax−gmin` sweep in `assimilate_multiple_scf`,
and up to three step-3 energy evaluations — practically ~5–10 distortion
evaluations per live band, of which all but the accepted one are
discarded (~80–90% speculative). Prior work already exhausted the
decision-preserving options: exact intra-pass `(band, gain)` repeats are
only 13–18% (§25) and the bit-exact LRU-8 capture regressed (−0.66%,
§26) because lookup plus refactor overhead exceeded the saving; the
search bounds (`UPCOUNT_LIMIT`, `±MAX_SCF_DELTA`, `PE_GATE`) are
C-mandated decision logic. Any further S4 cut changes encoder decisions
and is therefore classified as a quality change, not a pure performance
opportunity. R1 does not touch the search; it reuses the search's
*output*.

### F4. Bit count and spectral encode traverse the same lines twice with the same tables

`dyn_bit_count_reuse` walks every quantized line once through the shared
`HUFF_LTAB*` length tables (all-books rows + section merge), and
`encode_spectral_data` → `code_values` walks the identical `quant_spec`
again through the same length tables plus `HUFF_CTAB*` codewords and the
`BitWriter`. Sectioning already produces the final `(code_book,
sfb_start, sfb_cnt)` geometry consumed verbatim by the encode path
(`frame.rs:787–816` emits only; no re-sectioning). One traversal could in
principle serve both, but the access patterns differ (all-books pricing
vs final-book emit), so the saving is one traversal, not the whole stage.

### F5. Frontend cost is fragmented; no single redundancy reaches 5%

The frontend (24.8%) splits across ~7 symbols at 2–4% each
(`autocorr_quarter_lags` 3.6%, `BlockSwitch::process` 3.0%,
`form_factor` 2.7%, `dit_fft` 2.7%, `scale_up_spectrum` 2.5%,
`calc_ld_data` 3.1% spread over all Ld conversions, `intensity_stereo`
2.0–3.3%). The spectrum→Q31 mapping runs up to 4× per frame per channel
(psy map, TNS-active remap, stereo MS remap, driver live-set map), but the
maps operate on *different* spectra (pre/post-TNS-filter, mid/side), so
only conditional (TNS-inactive) reuse exists — worth ~1–2%, with
correctness-sensitive plumbing. `intensity_stereo`'s cost is almost
entirely its per-band correlation walks on content where IS never fires
(the 19-stream corpus is IS-free); skipping decisions is
quality-adjacent, not decision-preserving. Block-switch IIR carries state
across all eight windows (no exact parallel split; §21.3). TNS lag
computation already uses a fused single traversal; its transpose was
rejected (§20).

### F6. Remaining copies are real but small; the allocator is not the lever

Per-frame copies confirmed: PCM cloned twice (`encoder.rs:388` then
`stream_driver.rs:1010–1018`), per-channel spectrum/Q31/band-vector
`to_vec`/`clone` chains in the live-set build (873–966), S4's
`spec.to_vec()` + fresh `quant`/`scf`/`dist` (`scf.rs:1169–1175`), QC's
fresh `quants`/`maxes`/`sections` (`rate.rs:1495–1527`).
Test-causality captures `MsDecisionCapture` (1109–1131) and `S1Cap`
(1215–1233) are built **unconditionally in production** (only `S2Cap` is
`cfg(test)`-gated at 1302) — each clones ~6–15 vectors per frame for
snapshots the production path discards. Prior evidence (§6.4 of the
baseline: allocation reductions without runtime wins) plus the copy
volume involved (tens of KB per frame against ~1024-line compute loops)
bounds this whole area at ~1–2% even if fully addressed.

### F7. Parallelism has no 5%-credible case for this workload

Channel-independent sections exist (block switch, transform, live-set
build, form-factor, S4 per channel, per-channel quantize/max/count) but
the pipeline is joint at every decision point that matters: common
window sync, joint `process_pair_reuse`, intensity stereo, M/S mirror,
joint PE/ATS, joint S2, element-level QC exit matrix, single CPE header.
Only stereo (2 channels) is in the validated matrix, so the parallel
fraction caps near half the frame while per-frame granularity (~470
frames over the corpus, sub-millisecond each) puts spawn/sync overhead
against fine-grained memory-bound loops. Shared workspaces (`s4_ws`,
`qc.bc_scratch`) would need privatization. No measurement supports a
net win; per the brief, threads are not assumed to help throughput.

## 5. Ranked opportunity table

| # | Candidate | Current cost (what is eliminated) | Est. end-to-end gain | Confidence | Complexity | Correctness / quality risk |
|---|-----------|-----------------------------------|----------------------|------------|------------|----------------------------|
| R1 | **Single-quantization S4→QC pipeline** (§6) | S5 `calc_max` + `dyn_bit_count` per ch per frame (dead, F1) **plus** QC pass-0 `quantize_spectrum` + `calc_max` + `dyn_bit_count` per ch per frame (redundant, F2) | **4–7% corpus, up to ~8% music** | Medium | Medium-high | Medium: MAX_QUANT edge needs fallback; decisions otherwise identical |
| R2 | Single-traversal bit count + encode | One of two full quantized-spectrum traversals (`code_values` 3.4% + half of count work) | 2–3% | Medium | Medium-high | Medium: bit-exact emit must be proved; section geometry already shared |
| R3 | Production capture/copy hygiene | `MsDecisionCapture`/`S1Cap` clones, PCM double clone, probe double-write | ~1% | High | Low | Negligible (gate behind `cfg(test)` / reuse buffers) |
| R4 | Intensity-stereo decision pruning | Correlation walks on IS-free content (2.0–3.3%) | ≤2% | Low-medium | Medium | Quality-adjacent: any skip risks decision change |
| R5 | Channel-level parallelism | Serial L/R execution of F7 sections | Uncertain (0–10%) | Low | High | Medium-high: determinism, workspace privatization, threading runtime |

## 6. Detailed analysis

### R1. Single-quantization S4→QC pipeline (recommended)

**Current cost.** Per frame per channel, the encoder performs the
quantize→max→count triple **twice**: once in S5 on S4's `quant_spec`
(`stream_driver.rs:1376–1404`), once in QC pass 0 on re-quantized lines
(`rate.rs:1644–1702`). Steady state is exactly one QC pass (retry rate
zero on 2,442 corpus frames), so the triple's steady-state cost is
`quantize_spectrum` (4.2% aggregate / 5.1% music) + `dyn_bit_count_reuse`
(3.3%) + `bit_count` (2.7%, the per-band rows inside the count) +
`calc_max_value_in_sfb` (sub-1%, inside rate/other). S5's half is dead by
construction (F1); QC's re-quantize half is redundant by parameter
identity (F2).

**Proposed change (work eliminated, not accelerated).** Delete the S5
stage. Seed QC pass 0 from S4 outputs: `quants[ch] =
s4outs[ch].quant_spec` (move, no copy), `maxes[ch]` from one kept
`calc_max_value_in_sfb`, `sections[ch]` from one kept
`dyn_bit_count_reuse` — i.e. the surviving triple *is* QC pass 0, with
`recalc[ch]` starting false. Carry a per-band early-out flag out of S4's
winning trials (new output bit, set iff a winning `calc_sfb_dist`
returned via the `MAX_QUANT` path): if set, fall back to today's
re-quantize path for that channel, preserving the MAX_QUANT gain-bump
safety net exactly. Retry/crash paths are untouched — they re-quantize
from the (retained) spectrum exactly as today, and `crash_recovery`
reads identical `quants` content.

**Estimated gain (Amdahl).** Eliminated work ≈ all of `quantize_spectrum`
(~4.2%, since ~100% of its production calls are pass 0) + roughly half
of the count/max work (~(3.3 + 2.7)/2 + ~0.5 ≈ 3.5%). Gross ≈ 7% corpus
(≈ 8% music, where quantize is 5.1%); net **4–7%** after seeding/copy
effects and I-cache change. Derivation is from observed self-time shares
of the exact eliminated calls — an estimate, not a measurement.

**Confidence: medium.** The deadness half (F1) is certain from source;
the redundancy half (F2) rests on the `qss` identity plus spectrum
identity argued above, which needs a differential proof (including
synthetic huge-magnitude spectra forcing the early-out path, short-block
grouping, mono, and 44.1 kHz) rather than being taken on trust.

**Complexity: medium-high.** Touches S4 output shape (flag), deletes a
pipeline stage, and reworks QC pass-0 initialization while keeping every
later QC path (retry, crash, exit matrix, fill/finalize) intact.

**Correctness risk: medium; quality risk: none by design.** With the
fallback, decisions and bitstream are identical by construction on all
inputs; without it, the MAX_QUANT edge would diverge on adversarial
content (never on the corpus). The campaign must prove bit-identity,
not merely hash-stability on easy fixtures.

**Validation.** (1) Differential test: seeded-QC vs fresh-QC over the
full S4 corpus plus forced-early-out synthetics — bit-identical
`final_quants`/`final_qmaxs`/`final_sections`/gains. (2) All 19 fixture
hashes + long hash unchanged. (3) Full suite + licensing check. (4) Paired
alternating A/B per the §8A conventions (K=16, production builds) with
the pre-declared gate; S5-only deletion (below) as a staged checkpoint.

**Staged subset.** Deleting S5 alone (keeping QC untouched) is low-risk,
low-complexity, high-confidence — but saves only S5's half (~2–3%) and
does not clear the bar. It is the natural first step *inside* an R1
campaign, not a standalone campaign.

### R2. Single-traversal bit count + spectral encode (deferred runner-up)

**Cost.** `code_values` 3.4% + roughly half the count-path table work;
the two traversals share inputs (`quant_spec`), band geometry
(`sfb_offsets`), and the `HUFF_LTAB*` tables, differing only in that
count prices all books while encode emits the final book's codewords.
**Change.** Emit `(codeword, length)` during the final-book pass, or
replay cached per-quad decisions at encode time; the count traversal
remains the decision authority. **Gain 2–3%** (consistent with baseline
§6.3's 1–3%), i.e. below threshold — hence deferred, to be revisited
only after R1 shrinks the quant stages and the bit path becomes
relatively larger. **Confidence medium; complexity medium-high**
(`BitWriter` interleaving with section/scf payloads must stay bit-exact).
Validation: S5/S7/S8 bit-exact vectors + frame hex tests + corpus hashes.

### R3–R5 (not campaigns)

- **R3** is certain (~1%) and cheap: `cfg(test)`-gate the two production
  captures, remove the `encoder.rs:388` PCM clone (feed slices
  downstream), reuse the probe `BitWriter`. Recommend as hygiene alongside
  any campaign, never as the campaign itself.
- **R4** caps at the decision-walk cost (2.0–3.3%, and only the IS-free
  fraction is even theoretically skippable); any skip rule risks changing
  stereo decisions on content the corpus does not cover. Rejected.
- **R5** has no credible floor: joint stages stay serial, granularity is
  sub-millisecond, and the validated matrix is 2-channel. Not recommended;
  a prototype could only be justified after all single-threaded structural
  work is exhausted.

## 7. Rejected or deferred opportunities

| Candidate | Reason |
|---|---|
| S4 search-space cuts (fewer trials, narrower bounds, cross-frame memoization) | Changes encoder decisions → quality change, out of scope; spectra differ across passes (1/942 repeats, §25) |
| Per-band LRU-8 S4 cache | Already tried, bit-exact, −0.66% median (§26); lookup + refactor overhead exceeds saving |
| SCF trial-array reuse, `find_max_merge` follow-ups, shift specialization, energy/dist unroll, TNS transpose | Already measured at <1% or negative (§§18, 20, 22, 23, 28); reopening them is explicitly out of scope |
| Frontend micro-fusions (Q31 remap dedup, Ld caching, MS-energy reuse) | Each 1–2%; maps operate on different spectra (F5); a bundle of them is the "collection of tiny opts" the brief forbids |
| Allocator pools / buffer reuse everywhere | Prior evidence shows allocation reduction without runtime wins; remaining copies bound the area at ~1–2% (F6) |
| AVX2 work | Blocked/unmeasured by project decision (`6025637`); no claim possible |
| Build flags (`target-cpu`, LTO) | Changes the deployment contract, not a code improvement; not evaluated |
| New profiling infrastructure / benchmark redesign | Out of scope; existing harnesses suffice for R1 validation |

## 8. Recommended next campaign (if justified)

**R1, staged:** (a) delete S5, keep QC fresh, paired A/B — expect ~2–3%,
proving the dead-stage removal cleanly; (b) seed QC pass 0 from S4 with
the early-out fallback, differential-proof first (including forced
early-out synthetics), then paired A/B — expect a further ~2–4%.
Accept only on bit-identity (seeded vs fresh QC outputs) plus unchanged
corpus hashes plus a paired production-build A/B clearing the bar. Do
not bundle R2/R3 into the acceptance comparison; R3 hygiene may land
separately as it cannot perturb decisions.

## 9. Uncertainties and measurement limitations

- All gain figures are **estimates from observed self-time shares**
  (Amdahl), not measurements; the task brief's Phase 3 shares are taken as
  given (ARM64 M5, VBR-5) and the same distribution is not assumed for
  other CPUs, bitrates, or VBR modes.
- The F2 identity argument is source-verified but not yet differentially
  proved; the estimate assumes the proof succeeds with the fallback in
  place.
- `sample`-based self shares conflate inlined callees and carry scheduling
  noise; the R1 validation must rest on the paired A/B, not on profile
  arithmetic.
- No cross-machine, cross-compiler, or `target-cpu`-tuned claim is made;
  AVX2 remains unmeasured.

## 10. Final recommendation

**The S4→QC single-quantization pipeline (R1) is the one structural
opportunity with credible ≥5% potential (estimated 4–7%).** It eliminates
an entire duplicated quantize→max→count triple per channel per frame —
a whole pipeline stage plus a redundant first pass — rather than
executing the same passes faster. No second candidate clears the bar;
R2 (2–3%) is the documented runner-up. If R1's differential proof fails
(i.e. the identity does not hold with the fallback), the honest outcome
is that no structural ≥5% campaign exists in the current architecture,
and the recommendation collapses to hygiene (R3) plus revisiting R2.
