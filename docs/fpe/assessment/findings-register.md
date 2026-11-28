# FPE Findings Register — Post-Checkpoint Consolidation

**Assessment date:** 21 November 2026
**Checkpoint:** `eb2896923ec788727975268aca0beb27cba51760` (`whepper/ferrite`, `main`)
**Authority:** Independent assessment. This register consolidates existing
findings and adds new assessment findings; it does **not** change any owner
disposition. Conditional acceptances remain conditional.

## ID scheme

| Prefix | Source | Meaning |
|---|---|---|
| `F-01…F-30` | R2.1-B owner-review evidence package | Per-band IS/MS decision findings (existing IDs preserved) |
| `OD-*` | `R2.1-B-owner-decision.md` §6 | Owner-recorded outstanding technical/audit items |
| `QR-*` | `R2.1-B-independent-review-qwen.md` §13 | Qwen adversarial owner-review audit findings |
| `DN-*` | `R2.1-B-numerical-review-deepseek.md` §6 / G-1 review §I | DeepSeek numerical discrepancies (N-series) |
| `DR-*` | `R2.1-B-G1a-confirmation-deepseek.md` | DeepSeek G-1a confirmation findings (R-series) |
| `FR-*` | `R2.1-B-owner-decision.md` §6 | G-1 optional documentation findings (defined only by reference) |
| `A-*` | **This assessment** | New independent findings |
| `R-01…R-21` | `10-risk-register.md` | Project risks (status update, not restated) |
| `ASM-*` | `01` §9 / `m0-verification.md` | Assumptions A-2…A-7 |

Status vocabulary: **Open** / **Conditional** (owner-accepted with unsatisfied
conditions) / **Accepted** (owner) / **Resolved** / **Deferred** / **Blocked**
(external artifact). "Blocks" names what the finding prevents.

---

## 1. R2.1-B per-band findings (F-01…F-30)

Source: `R2.1-B-owner-review.md` (ledger), `R2.1-B-owner-decision.md`
(dispositions), `R2.1-B-G1-validation-report.md` (energy adjudication).
Corpus: **381 frames, 15 materials** (verified `is_ms.rs:2204`; the "16
materials" wording in the owner records is a residual error — see A-22), 30
findings, 16 coding-difference frames, 0 digest differences, 14 review-gated
P/G, 1 unsupported-domain frame. All Cause B; no Cause C; all frames
structurally ADTS-valid (decode validation is Tier-B, blocked — QR-2).

### 1.1 Owner dispositions (verified, unchanged)

| Finding(s) | Consequence | Owner disposition | Condition / limitation | Blocks |
|---|---|---|---|---|
| F-19, F-21, F-23, F-24, F-25 | G (scale-only) | **Accept** | Same IS book + phase; numerical difference explained; energy-envelope validated (G-1). Not bitstream/perceptual equivalence | — |
| F-07, F-09, F-10 | P (in-phase IS) | **Conditional** | I2 sentinel-escape; 5-frame persistence observed; **termination beyond 5 frames unproven**; perceptual neutrality not claimed | Full R2.1-B acceptance; P-cascade closure |
| F-15, F-17, F-18 | P (out-of-phase IS) | **Conditional** | Same mechanism, out-of-phase IS path (`phase 1`, `scale 4`); termination unproven | Full R2.1-B acceptance |
| F-20, F-22, F-26 | G (IS admission/direction) | **Conditional** | One-stored-LSB right-channel energy → different Ld/`real_scale`; Q31 drops band, FPE retains for IS. Mechanism trace-verified; envelope satisfied; **no per-finding PE/ATS or perceptual evidence** | Full R2.1-B acceptance; perceptual closure |
| F-01 | L (MS margin flip) | Accept as documented (Qwen §14); **outside G-1** | `B_closed` arithmetic satisfied on instance but **domain-excluded** (both-arm `es_ld=MIN`); bound not generalizable; not bound-validated | MS sentinel-domain extension |
| F-02 | L (coding consequence of F-01) | Accept as documented; **outside G-1** | `ms 0→1` on same band/frame; not independent evidence | — |
| F-03…F-06, F-08, F-11…F-14, F-16, F-27…F-30 | L (local IS flips) | Accept with G-1 evidence | Envelope-conformant Cause-B residue (G-1, 56 rows supported/pass) | — |

**G-1 acceptance boundary (owner §2.1):** PASS WITH FINDINGS for the 28-finding
energy-envelope scope only. Does **not** establish perceptual neutrality,
decoder interop, end-to-end R2.1-B acceptance, validity of the MS `B_closed`
bound for IS decisions, or resolution of F-01's sentinel domain.

### 1.2 Mechanisms (verified against source)

- **M1 — I2 sentinel escape** (`is_ms.rs:555-559`, `intensity.rs`): with
  `eR=0`, `m_b = fmult(0.7, eL)` is `0` for `eL=1` (Q31 sentinel) and `1` for
  `eL=2` (FPE escape). One stored LSB decides IS admission. Root: float rescale
  energy stores `2` where Q31 stores `1` (G-1 long rows `1→2`, util 0.0312).
- **M3 — 0-vs-1 direction drop** (F-20/22/26): stored `eR 0 vs 1` → Ld `MIN` vs
  finite → `real_scale` MAX-clamp (Q31) vs finite (FPE) → direction drop vs
  retain. Class B (G-1 §10): mapped lines nonzero (`sum_abs_x=16122`), Q31
  truncates to 0, FPE rounds to 1.
- **M4 — scale rounding** (F-19/21/23/24/25): `is_scale = ((real>>1) ±
  HALF_LSB)>>23`; one-LSB grouped-energy deltas straddle the 8.0 edge. F-24 is
  the largest *Ld-domain* delta (≈1.8 dB) but a **one-LSB stored** delta (3 vs
  2, util 0.031246) — the owner-review "not sub-LSB" phrasing was corrected by
  G-1 §9 / DeepSeek §4.5.

### 1.3 Evidence available vs missing

- **Retained/reproducible:** G-1 `G1-BAND`/`G1-WIN`/`G1-MAX`/`G1-CLASSB` rows
  (energy terms, deltas, tolerances, utilizations, support flags); structural
  gates (replay parity, trace consistency, no bound violation, no Cause C);
  exact frame accounting; pinned sentinel counts.
- **Not retained (A-05):** per-finding IS margin operands, `real_scale`/`is_scale`
  values, event chains and carry origins come from a **removed** temporary dump
  harness (`R21B-DUMP` absent from retained code; Qwen evidence class OWN).
  Qwen recomputed them to the unit from reported operands (REP), but a future
  reader cannot re-derive them from delivered tests alone. G-1 retained the
  *energy* leg, not the IS-margin leg.

---

## 2. Owner-decision outstanding items (OD)

Source: `R2.1-B-owner-decision.md` §6. These remain open; the owner decisions
do not resolve them.

| ID | Item | Status | Required resolution | Blocks |
|---|---|---|---|---|
| OD-1 | Tier-B C parity | **Blocked** | Provision pinned C reference `f14ad40a` + R2.1-B IS/MS oracle dumps + decoder; not waived | G1/G5; M8; full acceptance |
| OD-2 | Per-finding PE/ATS | **Open** (unavailable) | R2.2 harness attribution; retain as explicit limitation for F-20/22/26 | FPE-3.1 acceptance; conditional closure |
| OD-3 | P-cascade termination | **Open** (unproven) | Long/real-material runs proving termination or bounded behaviour; preserve 5-frame boundary | P conditional closure |
| OD-4 | Original analytical report | **Missing** | Recover/reconstruct the accepted `R2.1-B` analytical report (`B_closed` derivation), preserving provenance | R2 acceptance provenance |
| OD-5 | F-01/F-02 | Separately scoped | Retain unsupported sentinel-domain limitation; not G-1-validated | MS domain extension |
| OD-6 | Byte-level baseline | **Not committed** | Establish committed baseline before future change comparisons (FR-6) | Change-comparison reliability |
| OD-7 | FramePair operand cross-check | **Outstanding** | Re-run/retain the bit-exact operand cross-check (DeepSeek scaffold removed) | Evidence retention |
| OD-8 | Individual Class B line vectors | **Not retained** | Retain mapped line lists (currently only `sum_abs_x`) | Evidence retention |
| OD-9 | N-8 shared probe-argument table | **Deferred** | Documentation/test-maintenance follow-up | — (maintainability) |
| OD-10 | FR-5 / FR-6 | **Retained** | FR-5: `sum_abs_x=37` printed not hard-asserted; FR-6: no committed byte baseline | Evidence pinning |

---

## 3. Review-finding dispositions

### 3.1 Qwen adversarial audit (QR-1…QR-10, `…-independent-review-qwen.md` §13)

| ID | Severity | Finding | Status at checkpoint |
|---|---|---|---|
| QR-1 | block→G-1 | Cause B unanchored for F-03…F-30; §6 vs §4 contradiction | **Resolved by G-1** (28 findings envelope-adjudicated, retained rows) |
| QR-2 | major | "valid and decodable" = structural ADTS only; decode is Tier-B | **Acknowledged** in owner §2.1 (decoder interop not established); wording limitation retained |
| QR-3 | minor | "16 materials" (actual 15); row-count wording | **Partially resolved** — G-1 report corrected; **owner decision/review still say 16** (A-22) |
| QR-4 | minor | energy-ratio formula missing ×64 Ld factor | **Resolved** (G-1 uses unit-correct `2^(ΔLd·64/2^31)`) |
| QR-5 | major | F-19/F-21 "supported-domain bound" label on IS-excluded bands | **Resolved** (G-1 R-5 domain-label correction) |
| QR-6 | minor | per-finding operands not retained (removed dump) | **Open** (A-05); G-1 retained energy leg only |
| QR-7 | minor | analytical report absent (repo + wiki) | **Open** (OD-4) |
| QR-8 | minor | impl note §5 "capped at 32 frames" vs actual 49/49/49/32/26/32/32/32 | **Open** (A-22; verified `is_ms.rs:2205-2212`) |
| QR-9 | minor | impl note §9 PE/ATS "recorded" vs harness records none per-finding | **Open** (wording); owner review "UNAVAILABLE per finding" is accurate |
| QR-10 | minor | owner §2 M2 line cite `:1008` vs actual `:1058-1067` | **Open** (cite precision; trivial) |

### 3.2 DeepSeek G-1 numerical review (DN-1…DN-10, §I) and G-1a confirmation (DR-1…DR-8)

| ID | Finding | Status |
|---|---|---|
| DN-1 | max finding-band util is F-26 (0.031250), not F-24 | **Resolved** (G-1a N-1; `g1_maximum_rows` pins) |
| DN-2 | Class B mapped sum 16122 belongs to narrow b10, not quiet b7 (37) | **Resolved** (G-1a N-2) |
| DN-3 | long rows are `1→2` ch0 / `0→0` ch1, not "1-vs-0" | **Resolved** (G-1a N-3) |
| DN-4 | grouped Q/T/R are envelope-term sums; tol sums per-window bounds (TNS windows contribute `q31_tol`) | **Resolved** (G-1a N-4; G-1b G1b-4 wording) |
| DN-5 | corpus max 0.1182 is on tail frame f12; prescribed max 0.079051 f11 | **Resolved** (G-1a N-5; pinned) |
| DN-6 | F-19/F-21 per-window rows not retained | **Resolved** (G-1a `g1_finding_window_rows`; DR-1 direction fixed by G-1b) |
| DN-7 | "max Class A/B" label arbitrary tie-break | **Resolved as documentation-only** (counts are evidence) |
| DN-8 | shared probe-argument table | **Deferred** (OD-9; functions shared, args re-typed) |
| DN-9 | `s_b` range 18–24 not 18–28 | **Resolved** (G-1a N-9) |
| DN-10 | sentinel counts printed not asserted | **Resolved** (G-1a hard-asserts 74/0/221/3/0/3/0/0/78/0; verified `probe_envelope.rs:977-1009`) |
| DR-1…DR-8 | G-1a confirmation residuals (direction, counts, spread/rescale tol, Class A caveat, §4 wording, stale line count, N-8, 28-vs-30) | **Resolved by G-1b** except DR-7/N-8 (**Deferred**, OD-9) |

### 3.3 G-1 optional documentation findings (FR-1…FR-6)

Defined **only by reference** in `R2.1-B-owner-decision.md` §6 (no source record
in the package — see A-04).

| ID | Finding | Status |
|---|---|---|
| FR-1…FR-4 | G-1 optional documentation items | Owner: "addressed by G-1c" — **G-1c record absent** (A-04); cannot independently verify |
| FR-5 | `sum_abs_x=37` printed but not individually hard-asserted | **Open** (verified: no `37.0` assert in `probe_envelope.rs`) |
| FR-6 | no committed byte-level baseline | **Open** (OD-6) |

---

## 4. New assessment findings (A-series)

Each: source / description / status / evidence / uncertainty / risk / required
resolution / blocks.

### A-01 — M2/G0 bit-identity gate not re-runnable from the checkpoint
- **Source:** this assessment; `fe73705` commit message; `stream_driver.rs:8221`.
- **Description:** G0 (M2 pass-through bit-identity, `07` §8) is a Stage-1 gate,
  but the published tree already has the float value lane; the seam test
  explicitly states FPE-3.1 "replaces the M2 byte-identity gate". The squashed
  commit cannot be split into independently compiling M2 states. G0 evidence is
  historical only.
- **Status:** Open (evidence/provenance). **Evidence:** commit message; seam-test
  comment; absence of any pass-through-mode test. **Uncertainty:** whether a
  pre-squash M2 green run is recoverable from owner records.
- **Risk:** Medium — a Stage-1 gate rests on unretained evidence. **Resolution:**
  recover the M2 green-run record, or add a test-only pass-through comparison
  mode that re-establishes G0 on demand (owner-approved, test-only). **Blocks:**
  clean G0 sign-off at M8/M9.

### A-02 — M1/M2 independent-review sign-off records absent
- **Source:** this assessment; `09` M1/M2 review gates; `11` §5.
- **Description:** Roadmap requires architecture review (M1) and
  architecture+numerical review (M2); no retained sign-off record exists in the
  package (only M0 verification and the R2.1-B/G-1 reviews).
- **Status:** **Closed 2026-11-24** — completed, independently reviewed and owner-accepted (see dated update §8). **Evidence:** `docs/fpe/` file inventory. **Uncertainty:**
  reviews may exist externally. **Risk:** Medium (governance). **Resolution:**
  bundle/link the M1/M2 reviews or record their absence and re-review.
  **Blocks:** G8 review completeness.
- **Update 2026-11-24 (M2 acceptance; history preserved):** the original
  statement above remains accurate as history (at assessment time no retained
  M1/M2 sign-off existed). As a separate event, a **fresh independent M2
  review** was performed by **GLM 5.3 Flash on 24 November 2026**
  (`docs/fpe/assessment/m2-independent-review-glm-2026-11-24.md`, verdict
  **PASS WITH FINDINGS**, M2 F-1…F-4 non-blocking) and the owner formally
  accepted M2 (`docs/fpe/assessment/phase-0/P0-A-owner-disposition-record.md`, Amendment M2 acceptance,
  24 November 2026). The original M2 review occurrence remains unconfirmed;
  the new review does not backdate it. M1 sign-off remains unretained
  (no fresh M1 review authorized). G8 is satisfied for M2 only.

### A-03 — R2.1-B aggregate counts printed, not asserted
- **Source:** this assessment; `is_ms.rs:2364-2380` (`summarize`), gate tests.
- **Description:** The headline aggregates (30 findings / 16 coding frames / 0
  digest diffs / 14 review-gated / 1 unsupported frame) are **printed**, not
  hard-asserted. Frame count is pinned indirectly via `materials()`; structural
  gates (replay/trace/bound/Cause-C/routing) are asserted. A future numerical
  drift could change the finding population without failing any gate (provided
  bounds still hold). Contrast: G-1a *did* pin probe sentinel counts after DN-10.
- **Status:** Open. **Evidence:** `summarize` is `println!`; no
  `assert_eq!(c.findings.len(), 30)` etc. **Uncertainty:** none (verified).
- **Risk:** Medium-low — silent acceptance-basis drift. **Resolution:** test-only
  pinning of the deterministic aggregates (owner-approved). **Blocks:** reliable
  future change comparison (with OD-6).

### A-04 — G-1c cleanup record referenced but absent; FR definitions missing
- **Source:** this assessment; `R2.1-B-owner-decision.md` §2/§6.
- **Description:** Owner decision cites "the subsequent G-1c documentation
  cleanup" and FR-1…FR-6, but no G-1c document exists in the package and FR-1…FR-4
  are defined nowhere committed. The acceptance record references an unretained
  artifact (same pattern as OD-4).
- **Status:** Open. **Evidence:** `grep G-1c` → only owner-decision references;
  `ls docs/fpe` has no G-1c file. **Risk:** Low-medium (provenance).
  **Resolution:** recover/link G-1c and the FR source list. **Blocks:** complete
  provenance chain for the owner decision.

### A-05 — Per-finding IS operands/margins/trace chains not retained
- **Source:** Qwen QR-6 (OWN class); OD-7/OD-8; this assessment confirms.
- **Description:** The R2.1-B per-finding values came from a removed dump
  harness; retained tests reproduce counts/structure but not per-finding IS
  operands. G-1 retained the energy leg only. Class B mapped line vectors not
  retained (only `sum_abs_x`).
- **Status:** Open. **Evidence:** `R21B-DUMP` absent from retained code (Qwen
  grep); G-1 report §2 (DeepSeek scaffold removed). **Risk:** Medium — future
  audits must re-derive. **Resolution:** a retained per-finding dump (test-only)
  for IS margins + Class B line vectors. **Blocks:** reproducible R2 acceptance.

### A-06 — FPE-selected integration coverage narrower than planned
- **Source:** this assessment; `06` §9; `stream_driver.rs:8221-8264`.
- **Description:** The FPE-selected seam test covers **7 fixtures**, structural
  checks only (frame count, no skip notes/crash, ≥1 differing frame). `06` §9
  plans 19 fixtures + 4 WAVs with decode via `AAC_DEC_BIN`. No decode (Tier-B
  blocked), no bound on the magnitude of downstream differences, no full-corpus
  FPE encode.
- **Status:** Open (partly Tier-B-blocked). **Evidence:** test source.
  **Risk:** Medium. **Resolution:** expand FPE-selected encode coverage; decode
  leg awaits Tier-B. **Blocks:** G5 decoder gate; integration confidence.

### A-07 — Decision-safety corpus limits
- **Source:** this assessment; `is_ms.rs:2204`; owner P conditions.
- **Description:** IS/MS corpus is **stereo-only** (mono never reaches IS/MS by
  design), **VBR-5 operating point** (96k/192k, bandwidth 19293), **no real
  music**, 12-frame probes. P persistence is observed over 5 frames of
  stationary probes; termination on long/real material unproven. Other VBR modes
  change psy tables (TNS bitrate/minSnr) → different TNS activity → different
  recompute paths not exercised by the decision corpus.
- **Status:** Open. **Evidence:** `materials()`; `DiffStream::from_pcm` operating
  point. **Risk:** Medium. **Resolution:** R2.2 long/real-material runs; add
  operating points. **Blocks:** P conditional closure; general decision safety.

### A-08 — No PE/ATS/downstream decision-safety validation (R2.2)
- **Source:** `07` §5.2; OD-2; this assessment.
- **Description:** Decision safety stops at IS/MS. PE/ATS evolution, S2 VBR
  adaptation/avoid-hole, S4 scalefactors, S6 allocation, final frame accounting
  are **not validated**. The energy envelope does not bound downstream PE or
  rate-control evolution; a one-LSB energy difference can shift PE → bit
  allocation → quality. PE/ATS was diagnostic-only in R2.1-B.
- **Status:** **Resolved for the M3 boundary 28 November 2026** — R2.2
  decision-safety evidence for D-1…D-5/D-7/D-8 is owner-accepted through
  AC-9/AC-10, Group C/D and AC-7 within the M3 scope. The finding remains open
  only for the explicitly deferred conditional-closure legs (D-6/P-termination,
  OP-4, OD-3 five-frame, A-09 perceptual). See
  [M3-exit-assessment.md](M3-exit-assessment.md) §3/§4.7 and §21 below.
  **Evidence:** Group C/D acceptances, AC-7 owner decision, AC-9 owner
  acceptance, AC-10 PASS WITH FINDINGS, F-20/F-22/F-26 numerical acceptance.
  **Risk:** reduced to the deferred legs. **Resolution:** R2.2
  production-replay harness delivered and accepted for M3; deferred legs remain
  outside M3. **Blocks:** no M3 exit criterion; perceptual/full R2.1-B closure
  remains deferred.

### A-09 — No perceptual evidence
- **Source:** this assessment; constraint 5; owner F-20/22/26 conditions.
- **Description:** No listening tests, no objective quality metrics on FPE
  streams, no ABX. Numerical fidelity ≠ perceptual equivalence. The conditional
  acceptances explicitly require downstream/perceptual evidence eventually.
- **Status:** Open. **Evidence:** absence of any perceptual/metrics run for FPE.
  **Risk:** **High** for the project goal (perceptually competitive encoder) and
  for closing conditional acceptances; **not** a Stage-1 numerical-fidelity gate
  (charter keeps metrics supplementary). **Resolution:** perceptual-validation
  phase (ABX on flip material + objective metrics + Q31 regression). **Blocks:**
  conditional-acceptance closure; any future default change.

### A-10 — Tier-B C parity blocked
- **Source:** OD-1; `m0-verification.md` M0.1/M0.3; `is_ms.rs:2626`.
- **Description:** Pinned reference `f14ad40a` object unavailable locally; A-3
  verified only at the parent `da00af1c:cpp ≡ 3878c45a:cpp` (tree hash
  `97225710…`); IS/MS oracle dumps and decoder absent. Tier-B placeholder prints
  `TIERB_SKIP … BLOCKED` (cargo-counted as passed, **not** a substantive pass).
- **Status:** Blocked (external). **Risk:** High. **Resolution:** re-provision
  pinned reference from owner; generate IS/MS oracle dumps; build decoder.
  **Blocks:** G1/G5; M8; full Stage-1 acceptance.

### A-11 — Cross-architecture Ld bound not measured (A-5)
- **Source:** `05` §2/§9.7; this assessment.
- **Description:** All recorded evidence is Apple Silicon. CI runs Tier-A on
  `ubuntu-latest` (x86-64), giving *implicit* cross-arch regression coverage, but
  the **measured** max `|ld_ARM − ld_x86|` bound required by `05` §2 is not
  recorded. `log2` is implementation-defined; no correct-rounding claim.
- **Status:** Open. **Risk:** Low-medium (envelope floor 32 LSB ≫ platform log2
  noise). **Resolution:** explicit A-5 measurement on both arches. **Blocks:**
  M8 cross-platform sign-off.

### A-12 — A-4 f32-vs-f64 experiment outstanding (FPE-3.3)
- **Source:** `05` §9.7; ADR-0002; `09` M3.
- **Description:** f64 is justified analytically; the empirical corpus f32-vs-f64
  energy-sum error experiment (A-4) is not done. f64 stays the approved design
  regardless; f32 would be Stage 2.
- **Status:** Open (low priority). **Risk:** Low. **Resolution:** FPE-3.3
  measurement. **Blocks:** M3 completion (formally).

### A-13 — Composite-stage seams force duplication for M4–M7
- **Source:** this assessment; `04` §6; ADR-0001; `00` §4 escape hatch.
- **Description:** FPE delegates monolithic Q31 composites
  (`advance_thresholds`, `tns_encode_channel`, `finish_channel`). Floating
  thresholds/PCM/pre-echo requires FPE-side composite mirrors (Q31 refactor
  forbidden). ADR-0001's "slight duplication" understates the growing
  composite-stage duplication and drift risk (R-02).
- **Status:** Decided 28 November 2026 — see
  [M4.0 owner decision](M4.0-owner-decision.md) §2 (D2). Resolution:
  **FPE-side composite advance-stage mirror** for the threshold-advance
  family (M4–M7 window); `tns_encode_channel` remains delegated until the
  M7 review; `finish_channel` remains shared; Q31 kernel
  extraction/refactoring rejected. Historical status before the decision:
  Open (architecture/planning). **Risk:** Medium — avoidable
  migration complexity. **Resolution:** decided (above).  **Blocks:** none
  (decision recorded; mirror implementation is staged M4.2, not yet
  authorized).

### A-14 — F-6 short-TNS tnsSpecShift≠0 structurally unreachable
- **Source:** this assessment; `07` §5.2; `psy.rs:1190-1213`; `f6_tns.rs`.
- **Description:** `tns_encode_channel` sets `tns_spec_shift_global` only in the
  long branch; D-022 forces CPE sequence agreement, so a short element cannot
  carry a long-channel overflow shift. The case is structurally unreachable;
  T1 scan + audit watch report 0 (pinned). The `07` §5.2 constructed-test
  requirement is defensive.
- **Status:** Closed 27 November 2026 — see [F-6 owner decision](f6-owner-decision.md).
  Closure basis: documented structural unreachability (short branch cannot
  write the element-global shift; per-element-per-frame zero init; synchronized
  stereo sequences; release-active D-022 enforcement) plus retained watches;
  §5.2 construction requirement resolved as reachability-conditional (separate
  synthetic short-element test not required). Residuals F6-R2…F6-R6 retained.
  **Risk:** Low. **Resolution:** decided — owner decision to close F-6 via
  documented unreachability + retained watch, recorded in `f6-owner-decision.md`.
  **Blocks:** — (was: formal R2 closure item; leg satisfied).

### A-15 — Documentation currency / status index
- **Source:** this assessment; `README.md` header; `00` status; `09` table.
- **Description:** README still says "planning only — no production code
  changed, no engine implemented, nothing committed"; charter "owner review
  pending"; `09` milestone table presents M3 as unstarted. No single status
  index reflects the partial-M3 reality. The owner decision is the authoritative
  status record but is not linked from the README map.
- **Status:** Open (docs). **Risk:** Low. **Resolution:** owner-approved status
  refresh + index (this assessment package can serve interim). **Blocks:** — .

### A-16 — Clippy warnings in new FPE production code
- **Source:** this assessment; `cargo clippy` run.
- **Description:** 2 warnings in `fpe/orchestrate.rs` (`:457` loop index used to
  index `tns_active`; `:692` redundant deref). CI clippy is informational
  (non-gating) with known pre-existing drift; G-1's "clippy clean" claim was
  scoped to G-1-touched files (orchestrate.rs not among them).
- **Status:** Open (trivial hygiene). **Risk:** Trivial. **Resolution:** fix in a
  future approved touch. **Blocks:** — .

### A-17 — R2.0 fixtures not resynthesized in Rust
- **Source:** R2.0 note §1/§2; this assessment confirms.
- **Description:** `fpe-short-burst-quiet` / `fpe-tns-active-short` exist only in
  `tools/enc-oracle/make_fpe_fixtures.py`; `testdata.rs` resynthesis deferred.
  Quiet-short and TNS-active-short scenarios are not exercised in-binary
  (relevant to the quiet short-window sentinel/PE test, `07` §5.2, and F-6).
- **Status:** Deferred. **Risk:** Low-medium (coverage). **Resolution:** additive
  `testdata.rs` resynthesis (owner-approved). **Blocks:** quiet-short coverage.

### A-18 — MS energies still Q31; B_closed valid only for current operand set
- **Source:** this assessment; `orchestrate.rs:602-650`; `is_ms.rs:309-348`.
- **Description:** MS mid/side energies are computed from the remapped Q31
  spectrum, not float rescale energy. `B_closed` is validated for the present
  operands. M7 (float MS) introduces a new divergence source requiring
  re-derivation/re-validation of the MS bound and decision safety.
- **Status:** Open (roadmap note). **Risk:** Medium. **Resolution:** treat M7 as
  re-opening decision safety. **Blocks:** — (planning).

### A-19 — Deferred production guards / source proofs
- **Source:** R2.0/R2.1-A notes §2.
- **Description:** Production `!short_ch` TNS guard not introduced (test-only
  invariant stands in); `SyncBlockSwitching` mixed-window source proof
  unverified (D-022 retained as the Rust invariant).
- **Status:** Deferred. **Risk:** Low. **Resolution:** owner decision on whether
  to add the production guard / complete the source proof. **Blocks:** — .

### A-20 — FpeError graceful skip-with-note not implemented
- **Source:** `06` §6; `04` §7; R2.0/R2.1 notes; this assessment.
- **Description:** The only numerical guard is the release-active `assert!` in
  `fraction_to_stored`. The intended frame-level `FpeError`/skip-with-note path
  was deferred to "FPE-3.1R2" but R2.1-B scoped IS/MS only; it remains
  unimplemented. A non-finite fraction would panic the encoder in release
  (unreachable for finite input today).
- **Status:** **M3 decision satisfied 27 November 2026; implementation
  deferred.** [R2.2-group-E2-E3-owner-decision](R2.2-group-E2-E3-owner-decision.md)
  §3 records: the decision is required for M3; graceful frame-level
  implementation is not required for M3; actual implementation is deferred to
  FPE-default readiness or a separately authorised scope; `fraction_to_stored`,
  its release-active assertion, the public API and frontend error propagation
  are unchanged. **Risk:** Medium (robustness before default).
  **Resolution:** deferred implementation via the existing `frontend`
  `Err(note)` seam (no new public API), under separate authorisation.
  **Blocks:** FPE-default readiness, not M3. See §21 below.

### A-21 — R2.1-B byte-level baseline not committed (≡ FR-6 / OD-6)
- **Source:** OD-6; FR-6; this assessment.
- **Description:** The production 19-fixture Q31 baseline **is** committed
  (`production_baseline.rs` byte-length + FNV-1a pins). The R2.1-B 15-material
  Q31/FPE stream baseline is **not** committed; future change comparisons lack a
  pinned reference.
- **Status:** Open. **Risk:** Medium-low. **Resolution:** commit a byte-level
  baseline for the decision corpus. **Blocks:** reliable change comparison.

### A-22 — Residual count/wording errors in authoritative records
- **Source:** this assessment; DeepSeek §6.1; Qwen QR-3/QR-8/QR-9/QR-10.
- **Description:** (a) Owner decision §2 and owner-review Method still say "16
  materials"; harness defines **15** (verified `is_ms.rs:2204-2260`: 8 fixtures +
  silence + 6 probes; frames 301+8+72=381). "16" is correct for coding-difference
  *frames*, not materials. (b) Implementation note §5 "capped at 32 frames each"
  vs actual 49/49/49/32/26/32/32/32. (c) Impl note §9 PE/ATS "recorded" wording
  (QR-9). (d) Owner §2 M2 line cite (QR-10).
- **Status:** Open (documentation precision). **Risk:** Low, but these are the
  *authoritative* acceptance records. **Resolution:** owner-approved surgical
  corrections. **Blocks:** — (precision of the acceptance basis).

---

## 5. Risk register status update (R-01…R-21)

Reference `10-risk-register.md`; status as verified at checkpoint:

| Risk | Status | Note |
|---|---|---|
| R-01 Q31 accidental change | **Mitigated** | additive-only diffs verified; baseline 2/2 green |
| R-02 orchestration drift | **Active** | mirror verified; composite duplication grows (A-13) |
| R-03 scaling/domain mix | **Mitigated** | single fraction domain; shifts exact; identity gates |
| R-04 Ld convention | **Mitigated** | §6.8 preserved; sentinel divergence pinned |
| R-05 tuning drift | **Mitigated** | constants exact `/2^31`; tables consumed not regenerated |
| R-06 long/short/grouped | **Mitigated** | grouped layout asserts; stale-on-short covered |
| R-07 temporal state drift | **Pending M6** | pre-echo still Q31; per-frame state gate active |
| R-08 state init mismatch | **Mitigated** | `fresh_fpe_state_init_matches_q31` |
| R-09 channel-pair regression | **Mitigated** | D-022; joint shifts; LFE parity |
| R-10 quantization integration | **Mitigated (partial)** | grouped MS-Ld exact; downstream untouched |
| R-11 inadequate reference testing | **Active** | Tier-B blocked (A-10); in-binary harness strong |
| R-12 reference unavailable | **Materialized** | pinned `f14ad40a` absent (A-10) |
| R-13 performance regression | **Not measured** | G6 at M9; Q31 baseline captured |
| R-14 memory regression | **Mitigated (design)** | allocate-selected-only; FpeScratch reused; audit pending M9 |
| R-15 licensing/provenance | **Mitigated** | check-licensing OK; TPMV notices; provenance gaps A-04/OD-4 |
| R-16 numerical instability | **Mitigated (guard)** | release-active assert; FpeError pending (A-20); G4 fuzz pending M8 |
| R-17 new decision flips | **Active** | 30 findings triaged/owner-dispositioned; R2.2 pending (A-08) |
| R-18 cross-arch difference | **Active** | A-5 unmeasured (A-11); CI x86-64 implicit |
| R-19 M2 float round-trip | **Historical** | M2 pass-through verified pre-squash; not re-runnable (A-01) |
| R-20 misleading success | **Mitigated** | frame accounting; unsupported≠pass; Tier-B SKIP≠PASS |
| R-21 single-frame state divergence | **Mitigated** | per-frame multi-frame state comparison |

## 6. Assumptions (ASM)

| ID | Assumption | Status |
|---|---|---|
| ASM-A2 | THR_SHIFTBITS/PCM_QUANT_THR_SCALE/C_RATIO anchors | **Resolved** (M0.2) |
| ASM-A3 | pinned `f14ad40a:cpp ≡ 3878c45a:cpp` | **Substantially verified, NOT resolved** — parent tree hash identical; pinned object absent (A-10) |
| ASM-A4 | f64 energy envelope | Analytically justified; empirical experiment outstanding (A-12) |
| ASM-A5 | cross-arch log2/Ld | Outstanding (A-11) |
| ASM-A6/A7 | (per `01` §9 / `m0-verification.md`) | See M0 record; carried to M8 |

---

## 7. Blocker dependency map

```
OD-4 analytical report ─┐
A-04 G-1c record ───────┤
OD-6/A-21 byte baseline ┼─► trustworthy acceptance basis ─► R2 acceptance provenance
A-03 aggregate pinning ─┤
A-05 operand retention ─┘

A-10 Tier-B (pinned C + dumps + decoder) ─► G1 fidelity / G5 decode ─► M8 ─► Stage-1 closure
A-08 R2.2 PE/ATS/downstream ─► FPE-3.1 acceptance ─┐
A-09 perceptual evidence ─► F-20/22/26 closure ────┼─► full R2.1-B acceptance
OD-3 P termination ─► P conditional closure ───────┘
A-12 A-4 + A-11 A-5 + A-14 F-6 ─► M3 completion
  (A-12 satisfied 26 Nov 2026; A-11 Linux leg is an M8 requirement, not an M3
  blocker; A-14 closed 27 Nov 2026, see [f6-owner-decision](f6-owner-decision.md).
  M3 completion itself is not declared.)
A-13 seam decision ─► M4–M7 execution model
A-20 FpeError ─► FPE-default readiness (post Stage-1)
```

## 8. Findings requiring owner decisions

1. **OD-4 / A-04:** recover/link the analytical report and G-1c record (or record
   permanent location). *Highest priority — acceptance provenance.*
2. **OD-6 / A-21 / A-03:** authorize committed byte-level baseline + R2.1-B
   aggregate pinning (test-only).
3. **A-08 / OD-2:** authorize and scope R2.2 (PE/ATS/downstream decision safety).
4. **A-13:** seam strategy for M4–M7 (FPE mirrors vs additive Q31 extraction).
5. **A-14:** ~~close F-6 via documented unreachability (or construct synthetic).~~
   Decided 27 Nov 2026 — F-6 closed via documented unreachability + retained
   watch; see [f6-owner-decision](f6-owner-decision.md).
6. **A-20:** place `FpeError` graceful handling in the roadmap.
7. **A-01 / A-02:** recover M2 green-run + M1/M2 review records (or re-review).
8. **A-22 / A-15:** approve surgical documentation corrections + status index.
9. **OD-3 / A-07:** authorize long/real-material persistence runs for P closure.
10. **A-09:** approve perceptual-validation methodology and scope.

*No conditional acceptance is promoted by this register. F-07/09/10/15/17/18 and
F-20/22/26 remain conditional until their stated conditions are satisfied.*

---

## 9. Dated update — M2 acceptance (24 November 2026)

*Appended without rewriting §§1–8 (history preserved).*

- **M2 is formally accepted** by the owner on 24 November 2026
  (`docs/fpe/assessment/phase-0/P0-A-owner-disposition-record.md`, Amendment M2
  acceptance), relying on the fresh independent M2 review by **GLM 5.3
  Flash** dated 24 November 2026
  (`docs/fpe/assessment/m2-independent-review-glm-2026-11-24.md`, verdict **PASS
  WITH FINDINGS**; all ten M2 acceptance criteria AC-01…AC-10 accepted).
- **A-02 is closed** as completed, independently reviewed and
  owner-accepted for M2 (see the dated update under A-02 above). The
  historical statement that the original M2 review occurrence was
  unconfirmed is preserved; the new review is recorded as a separate
  event and does not backdate historical sign-off. M1 sign-off remains
  unretained.
- **M2 review findings M2 F-1…F-4** (distinct namespace from R2.1-B
  `F-01…F-30`) remain **non-blocking and retained**: F-1 Tier-B
  C-reference parity blocked; F-2 provenance/reconstruction; F-3 LFE
  M3+ coverage gap; F-4 informational Clippy. No finding is resolved or
  reclassified by this update.
- **Unchanged:** the nine conditional R2.1-B acceptances remain
  conditional; Tier-B C parity remains blocked, not waived; G0 remains
  historical/non-rerunnable (A-01 open); G0/A-01 and all other standing
  limitations are unchanged. No production source, test code, manifest,
  dependency or evidence-byte change is made by this update.

---

## 10. Dated update — A-5 / N-2 owner decision (26 November 2026)

*Appended without rewriting §§1–9 (history preserved). Authority:
[owner decision](a5-n2-owner-decision.md) (26 November 2026). No finding is
resolved, closed, waived, or reclassified by this update; no envelope,
tolerance, gate, or unrelated milestone status is changed.*

- **N-2 (validation-roadmap A-5 cross-arch Ld measurement) is partially
  complete and remains open.** The retained macOS ARM64 versus macOS
  x86-64-under-Rosetta evidence (Layer A 352/352, Layer B 658/658, Layer C
  49/49; max observed absolute Ld difference zero) is accepted within its
  documented scope; the Linux x86-64 CI measurement remains open per the
  owner-decision completion protocol. No native x86-64, Linux/glibc, or
  universal cross-platform equivalence is claimed.
- **N-2 is not an M3 completion blocker.** The §7 blocker-map line placing
  "A-4 + A-5 + F-6 ─► M3 completion" is clarified on this point only: the
  A-5/N-2 Linux leg does not block M3 completion (A-4, F-6, and the rest of
  M3 completion are otherwise unchanged).
- **N-2 is an additional M8 cross-platform milestone requirement**,
  established explicitly by the owner decision — not pre-existing in the
  original normative M8 roadmap. N-2 remains open until the Linux x86-64
  comparison is executed, reviewed, and retained.
- **A-5 split:** the numerical `log2`/Ld comparison is covered by N-2 as
  above; the original A-5 performance/denormal-behaviour investigation
  remains allocated to M9.
- **Status notes (unchanged substance):** A-11 remains open pending the Linux
  measurement (macOS bounded evidence now retained); R-18 remains Active;
  ASM-A5 remains outstanding for the Linux leg. All other findings, gates,
  and limitations are unchanged.

---

## 11. Dated update — A-4 / FPE-3.3 owner acceptance (26 November 2026)

*Appended without rewriting §§1–10 (history preserved). Authority:
[owner decision](a4-owner-decision.md) (26 November 2026). No finding is
resolved, closed, waived, or reclassified by this update beyond what the
owner decision records; no envelope, tolerance, gate, or unrelated
milestone status is changed.*

- **A-12 (A-4 f32-vs-f64 experiment, FPE-3.3) is recorded and accepted with
  documented limitations.** The corrected A-4 run (27 fixtures, 2757
  frames; 233,910 unique window-band cells; 467,820 combined kernel
  observations; maxima 338 stored LSB rescale / 55 TNS-long-active with
  utilization ≤ 0.031; zero over-envelope, guard, sentinel, or Class B
  flip events) is accepted as the reproducible numerical precision
  experiment covering FPE-3.1 rescale-energy and FPE-3.2 TNS-recompute
  accumulation. f64 is retained as the approved Stage-1 implementation
  (ADR-0002 unchanged); f32 results are diagnostic for possible Stage-2
  optimization only. A-4 is no longer awaiting independent review (Mistral
  4 re-review: primary numbers reproduced; two informational report
  findings corrected) or owner acceptance.
- **The §7 blocker-map line "A-12 A-4 + A-11 A-5 + A-14 F-6 ─► M3
  completion" is clarified on the A-4 leg only:** the A-4
  evidence/acceptance leg is satisfied. M3 completion itself is not
  declared — F-6 closure, the N-2 Linux leg, the `FpeError` decision, R2.2
  decision safety (A-08/OD-2), and the perceptual conditions (A-09) remain
  as currently recorded.
- **A-4 numerical evidence is distinct from R2.2 decision-safety
  evidence.** This acceptance establishes no downstream decision outcome,
  no perceptual equivalence, and no universal cross-architecture
  equivalence. The seven existing FPE-suite failures remain separately
  tracked and are not resolved or accepted by the A-4 decision.

---

## 12. Dated update — FPE-3.2 owner acceptance (27 November 2026)

*Appended without rewriting §§1–11 (history preserved). Authority:
[owner decision](fpe-3.2-owner-decision.md) (27 November 2026). No finding
is resolved, closed, waived, or reclassified by this update beyond what the
owner decision records; no envelope, tolerance, gate, or unrelated
milestone status is changed.*

- **FPE-3.2 (float TNS energy recompute, long + short, incl. FPE
  orchestration integration) is recorded and accepted with documented
  limitations.** The independent Mistral 4 review (26 November 2026)
  verdict was PASS WITH FINDINGS with no correctness defect identified;
  the six findings travel with the acceptance: F1 (nonzero
  `shiftBits`/`tnsSpecShift` production behaviour unobserved on-corpus,
  synthetic coverage only); F2 (cross-arch `log2`/Ld validation
  incomplete, A-5/N-2 Linux leg open); F3 (stale `audit.rs` documentation
  labels, future maintenance); F4 (seven baseline-pin failures remain
  unrebaselined; IS/MS changes and downstream bitstream differences
  subject to R2.2 owner disposition); F5 (frame-level `FpeError` taxonomy
  deferred, A-20); F6 (Cargo packaging verification outstanding for a
  clean committed state; licensing verification passed).
- **The §7 blocker-map is clarified on the FPE-3.2 leg only:** the
  FPE-3.2 implementation/acceptance leg is satisfied. M3 completion
  itself is not declared — F-6 closure (A-14), the `FpeError` decision
  (A-20), the N-2 Linux leg (A-11), R2.2 decision safety (A-08/OD-2), and
  the perceptual conditions (A-09) remain as currently recorded.
- **FPE-3.2 correctness acceptance is distinct from R2.2
  decision-safety evidence.** This acceptance establishes no downstream
  decision outcome, no perceptual equivalence, and no universal
  cross-architecture equivalence. The seven existing FPE-suite failures
  (A-21) remain separately tracked and are not resolved, rebaselined, or
  accepted by the FPE-3.2 decision.

---

## 13. Dated update — M3 boundary + E2/E3 authorization (27 November 2026)

*Appended without rewriting §§1–12 (history preserved). Authority:
[owner decision](R2.2-group-E2-E3-owner-decision.md) (27 November 2026). No
finding is resolved, closed, waived, or reclassified by this update beyond
what the owner decision records; no envelope, tolerance, gate, or unrelated
milestone status is changed.*

- **Separate R2.2 register (OD-R22-2 / E-OD-5):**
  [R2.2-findings-register.md](R2.2-findings-register.md) (R22-* namespace).
  R2.2 findings are not merged into the R2.1-B namespace; this link history
  is append-only and never rewritten.
- **M3 boundary set:** M3 requires R2.2 downstream decision-safety
  acceptance, F-20/F-22/F-26 per-finding PE/ATS evidence for FPE-3.1
  acceptance, independent review + owner acceptance of the R2.2 evidence,
  the FpeError/A-20 decision, and the existing FPE-3.1/3.2/3.3 milestone
  requirements. D-6 recurrence + P-termination, the OP-4 recurrence campaign,
  the retained five-frame OD-3 boundary evidence, and A-09 perceptual
  evidence are deferred to later conditional closure. The nine R2.1-B
  conditionals remain conditional. The M3 scope conflict between
  validation-roadmap §5 and the broader R2.2 design completion language is
  resolved as recorded in the owner decision; historical design requirements
  are not rewritten.
- **E2/E3 authorized** on frozen E1 schema v2 (no redesign); F-20/F-22/F-26
  per-finding slice authorized from Group F; D-6 remains unauthorized.
  FpeError/A-20 decision required for M3, implementation deferred. Seven
  pins + 187 S4 flips preserved without rebaseline. Group C AC-2 sufficiency
  for M3 confirmed; A-09 not an M3 prerequisite; N-2 stays M8; A-5
  performance/denormal stays M9; Tier-B/A-10 stays blocked; A-13 stays
  M4-entry; M4 not authorized.

---

## 14. Dated update — Group E2/E3 owner acceptance (27 November 2026)

*Appended without rewriting §§1–13 (history preserved). Authority:
[owner decision](R2.2-group-E2-E3-owner-decision.md) (27 November 2026,
dated acceptance; Mistral 4 independent review PASS WITH FINDINGS). No
finding is resolved, closed, waived, or reclassified by this update beyond
what the owner decision records; no envelope, tolerance, gate, or unrelated
milestone status is changed.*

- **Group E2/E3 implementation and evidence accepted with documented
  limitations** (findings F-1…F-5 with dispositions in the owner decision;
  full detail in [R2.2-findings-register.md](R2.2-findings-register.md)
  R22-E23-01…09). The six F-20/F-22/F-26 per-finding replay rows are
  accepted as generated evidence; the R2.1-B conditionals themselves remain
  conditional.
- **F-4 (SG-2 clean-checkout proof) stays outstanding** as the E6 obligation
  under design §11.2 — not waived, not declared satisfied, E6 not
  authorized by this decision.
- M3 is not declared complete. D-6, P-termination, OD-3 five-frame, A-09,
  M4, FpeError implementation, rebaselines, and threshold/tolerance changes
  remain unauthorized. All other findings, gates, and limitations unchanged.

---

## 15. Dated update — F-20/F-22/F-26 numerical acceptance (27 November 2026)

*Appended without rewriting §§1–14 (history preserved). Authority:
[explicit owner decision](R2.2-F20-F22-F26-numerical-owner-acceptance.md)
(27 November 2026). No numerical contract or unrelated milestone status is
changed.*

- **F-20/F-22/F-26 numerical per-finding evidence is ACCEPTED for M3.**
  The authorized numerical requirement is satisfied: per-finding
  admission/direction, per-band IS book/scale, pre/post ATS, element-bundled PE,
  per-band Ld/`threshold_ld`, and Q31/FPE within-arm replay equality. The six
  Group E replay rows remain the accepted generated evidence foundation and
  were not regenerated or modified.
- **The nine R2.1-B conditional acceptances remain conditional** pending their
  perceptual requirements. Numerical evidence acceptance is not perceptual
  equivalence; A-09 remains outside M3. D-6, P-termination and broader Group F
  work remain deferred. No M4 authorization.
- **OD-2's numerical per-finding leg is satisfied; OD-2 is not wholly closed.**
  The earlier “Open (unavailable)” wording describes the checkpoint state;
  the generated F-20/F-22/F-26 numerical evidence now exists and is accepted.
  Conditional closure still requires the outstanding perceptual evidence.
- AC-9 and M3 completion are not declared by this update. All other findings,
  gates, pins, and limitations remain unchanged.

---

## 16. Dated update — AC-9 derivation evidence (27 November 2026)

*Appended without rewriting §§1–15 (history preserved). Factual evidence status
only; no owner acceptance is recorded by this update.*

- [R2.2-AC-9-margin-derivation-report](R2.2-AC-9-margin-derivation-report.md)
  records exact test-only derivations for `s2.ah_clamp_short`, all four S4
  step-3 margins, and S6 crash stop-distance from existing captured operands.
  It also inventories the broader MDEF/trace coverage gaps and corrections.
- D-R2 and AC-9 remain open pending owner disposition. The frozen schema v2,
  accepted Group E bundle, production numerical kernels, thresholds,
  tolerances, and baseline pins are unchanged. No AC-9 owner acceptance or M3
  completion is claimed.

---

## 17. Dated update — AC-9 partial numerical acceptance (27 November 2026)

*Appended without rewriting §§1–16 (history preserved). Authority:
[explicit owner decision](R2.2-AC-9-partial-owner-acceptance.md)
(27 November 2026). No historical decision or acceptance criterion is rewritten.*

- **AC-9 numerical evidence is ACCEPTED WITH DOCUMENTED LIMITATIONS**, limited
  to the S2 short-clamp derivation, S4 step-3 distance/energy/PE/sum margins,
  the S6 crash derivation, and IS/MS I1–I10 reporting over the 24 affected
  frames.
- **AC-9 as a whole is not accepted or complete.** D-R2 owner disposition,
  frozen-schema-v2 evidence integration, broader omitted decision-family
  coverage, collector/applicability remediation, and every other gap in the
  AC-9 report remain outstanding.
- No production numerical behaviour, threshold, tolerance, baseline pin,
  schema-v2 contract, accepted Group E bundle, or M3-completion status changes.
- **Supplemental implementation:** `docs/evidence/fpe-r22-ac9/` now retains
  exact AC-9 rows under separate `AC9SCHEMA 1`, with TSV/JSON, Group E row and
  bundle hash links, deterministic env-gated generation, and default
  read-only verification. Test-only collector remediation is recorded in the
  AC-9 report §10. D-R2 and AC-9 completion remain owner-pending.

---

## 18. Dated update — D-R2 decision and remaining AC-9 families (27 November 2026)

*Appended without rewriting §§1–17 (history preserved). Authority:
[explicit owner decision](R2.2-AC-9-D-R2-owner-decision.md) (27 November 2026)
and [AC-9 report §11](R2.2-AC-9-margin-derivation-report.md).*

- **D-R2 is dispositioned ACCEPT WITH DOCUMENTED LIMITATIONS — option 1.** The
  `s2.ah_clamp_short` numerical derivation is accepted; MDEF v2's existing
  `Required` marker is retained; no registry/schema redesign is authorized or
  required; the decision is scoped to the existing derivation, not blanket
  acceptance of all Required margins across all operating conditions.
- **Remaining AC-9 families advanced with decision-ready evidence:** S4 steps
  1 and 2, the distinct S4 final clamp, exact I9 `lr`/selected `ed`/`es`
  residuals, and the S6 per-pass/unreached-arm sweep. The supplement now has
  113,263 rows and 1,176 Group E links; exact TSV/JSON hashes and read-only
  verification pass; Group E remains 36/36.
- **AC-9 is not ready for final owner acceptance.** Broader §3.4 decision
  families, step-1 non-evaluated-band representation, frozen-schema-v2 numeric
  fields, collector remediation, corpus crash arms, MS/M7 downstream, and
  AC-10 review remain open. No M3 completion or M4 authorization is claimed.
- No production numerical behaviour, threshold, tolerance, baseline pin,
  schema-v2 contract, accepted Group E bundle, or coding decision changes.

---

## 19. Dated update — AC-9 M3 remainder (28 November 2026)

*Appended without rewriting §§1–18 (history preserved). Implementation status
only; no owner acceptance is recorded.*

- **M3 scope matrix recorded:**
  [R2.2-AC-9-M3-scope-matrix](R2.2-AC-9-M3-scope-matrix.md) classifies every
  remaining AC-9 §3.4 family against the M3 boundary. AC-9 §3.4 is treated as
  an inventory, not a requirement list.
- **M3-mandatory remainder implemented:** S1 patch execution/channel-1
  inheritance; S2 avoid-hole peak/valley and MS minSnr/spread cross-tweaks; S4
  all-dead/gain inversion; S6 MAX_QUANT, reduce/crash-error, full exit
  predicates and named accounting margins. True S1 patch-write/inheritance
  arms remain `Unobserved` on corpus; the I9 unselected residual is not
  generated by production.
- **Supplement:** 134,617 rows and 1,176 Group E links; exact hashes and
  read-only verification pass; Group E remains 36/36.
- **Later-phase / deferred:** S2 internal chaos curve/clamps, S6 internal
  fill-sizing/padding, frozen-schema-v2 numeric fields, MS/M7 downstream,
  D-6/P-termination/OP-4/OD-3/A-09/M4.
- **AC-9 is ready for the single AC-10 independent review**, not
  owner-accepted; M3 is not declared complete. No production numerical
  behaviour, threshold, tolerance, baseline pin, schema, or Group E hash
  changed.

---

## 20. Dated update — AC-10 review and AC-9 owner acceptance (28 November 2026)

*Appended without rewriting §§1–19 (history preserved). Authority:
[AC-10 independent review](R2.2-AC-10-independent-review.md) (outcome **PASS
WITH FINDINGS**) and [AC-9 owner decision](R2.2-AC-9-owner-acceptance.md)
(**ACCEPT WITH DOCUMENTED LIMITATIONS**).*

- **AC-10 outcome is PASS WITH FINDINGS; no M3 acceptance blocker was
  identified.** AC-10 found AC-9 suitable for owner disposition and signed the
  evidence implementation; the owner retains final authority.
- **AC-9 is owner-accepted with documented limitations** within the M3
  boundary established by
  [R2.2-AC-9-M3-scope-matrix](R2.2-AC-9-M3-scope-matrix.md). The accepted
  evidence is the AC-9 M3 evidence described in that matrix and retained in
  the separate `AC9SCHEMA 1` artifact.
- **AC10-F1…F8 remain visible with their original severity and
  classification; none is closed, waived, or reclassified.** All are severity
  **minor**:

| ID | Classification (AC-10) | Evidence reference |
|---|---|---|
| AC10-F1 | Non-blocking limitation (documentation precision only; link integrity verified) | `group-e-links.tsv`; `ac9_artifact.rs:646-737` |
| AC10-F2 | Non-blocking limitation (honest `Unobserved` arm; AC-9 permits explicit status) | `ac9-margins.tsv`; `pe.rs:516-548`; matrix §2.1 |
| AC10-F3 | Non-blocking limitation | `ac9-margins.tsv`; `rate.rs` `s6_probe`; report §4.3/§11.5 |
| AC10-F4 | Non-blocking limitation (genuinely unobservable) | `intensity.rs:551-585`; `ac9-margins.tsv` |
| AC10-F5 | Later-phase recommendation (or non-blocking limitation; D-5 named margins are covered) | matrix §2.4; report §12.5 |
| AC10-F6 | Later-phase recommendation (owner-scoped collector remediation; no AC-9 row depends on it) | report §5, §10.2; `trace.rs:747-768` |
| AC10-F7 | Later-phase recommendation | report §11.2/§11.7; `ac9.rs` step-2 `NotGenerated` handling |
| AC10-F8 | Later-phase recommendation (owner schema decision; OD-3 freeze respected) | `ac9_artifact.rs` README/asserts; bundle header `R22SCHEMA\t2` |

- **Preserved conditions:** unobserved production arms remain unobserved;
  synthetic evidence is not production-corpus observation; later-phase
  recommendations remain deferred and do not expand M3; D-R2 remains accepted
  with its documented limitation; MDEF v2 and frozen E1 schema v2 are
  unchanged; the accepted Group E bundle remains unchanged at 36/36; no
  rebaseline or replacement pins are authorized; no production numerical
  behaviour, threshold, tolerance or coding decision is changed; no
  perceptual-equivalence claim is made.
- **This accepts AC-9 only.** It does not declare M3 complete or authorize M4.
  A-08 is not closed by this update for any non-AC-9 R2.2/M3 exit criteria;
  the separate M3 exit-criteria assessment remains the authority for M3
  completion. All nine R2.1-B conditional acceptances remain conditional, and
  the deferred items (D-6/P-termination, OP-4, OD-3 five-frame, A-09, M4,
  MS/M7) are unchanged.
- Full AC-9 finding reconciliation is recorded in
  [R2.2-findings-register.md](R2.2-findings-register.md) R22-AC9-13…15.

---

## 21. Dated update — M3 exit assessment (28 November 2026)

*Appended without rewriting §§1–20 (history preserved). Assessment and status
reconciliation only; no owner acceptance, milestone declaration, production,
test, numerical-contract, threshold, tolerance, envelope, baseline pin, schema,
or evidence change is made by this update.*

- **M3 exit assessment recorded:**
  [M3-exit-assessment.md](M3-exit-assessment.md). It reconciles the
  authoritative M3 boundary (`09` §M3; `07` §5.2/§8; this register §13;
  [R2.2-group-E2-E3-owner-decision](R2.2-group-E2-E3-owner-decision.md) §1)
  against the accepted evidence and concludes **no technical M3 blocker
  remains**. Recommendation: **READY FOR M3 OWNER ACCEPTANCE**; the only
  remaining step is the owner's formal M3 completion declaration (which also
  settles the formal FPE-3.1 component acceptance).
- **A-08 reconciled:** resolved for the M3 boundary (D-1…D-5/D-7/D-8 accepted
  through AC-9/AC-10, Group C/D and AC-7); the deferred conditional-closure
  legs (D-6/P-termination, OP-4, OD-3 five-frame, A-09) remain open outside M3.
- **A-20 reconciled:** the M3-required decision is recorded (implementation not
  required for M3; deferred to FPE-default readiness or separately authorised
  scope; release-active `assert!` stands). The implementation remains open for
  FPE-default readiness, not M3.
- **Unchanged by this update:** F-20/F-22/F-26 numerical acceptance and their
  still-conditional perceptual conditions; the nine R2.1-B conditional
  acceptances; AC10-F1…F8 with their AC-10 severity/classification; D-R2 and
  MDEF v2; frozen E1 schema v2 and the accepted Group E bundle; A-21/OD-6,
  A-22/A-15, A-11/N-2 (M8), A-5 performance (M9), A-10 Tier-B (blocked, not
  waived), A-13 (M4 entry), SG-2/E6, and the seven pins/187 S4 flips. No
  rebaseline or replacement pin is authorised.
- **M3 completion is not declared; M4 is not authorised.** The exit assessment
  is the record of readiness, not the owner's declaration.

---

## 22. Dated update — M3 owner acceptance (28 November 2026)

*Appended without rewriting §§1–21 (history preserved). Authority:
[explicit owner decision](M3-owner-acceptance.md) (28 November 2026). This
closes the M3 milestone only; it does not close, waive or reclassify any
finding beyond the statuses already recorded, and does not authorise M4.*

- **FPE M3 is ACCEPTED / COMPLETE WITH DOCUMENTED LIMITATIONS.** The owner
  formally accepts FPE-3.1 (float rescale energy), FPE-3.2 (float TNS
  recompute), FPE-3.3/A-4, and the R2.2 decision-safety work within the M3
  boundary, on the basis of
  [M3-exit-assessment.md](M3-exit-assessment.md), the authoritative `09`/`07`
  criteria, the component owner decisions, AC-9 and AC-10. All authoritative
  M3 exit criteria are satisfied; no technical M3 blocker remains.
- **F-6** and the **FpeError/A-20 M3 decision** remain accepted/recorded as
  before ([f6-owner-decision](f6-owner-decision.md);
  [R2.2-group-E2-E3-owner-decision](R2.2-group-E2-E3-owner-decision.md) §3).
- **Preserved without change:** the seven pin failures and 187 unjudged S4
  flips (no rebaseline); the 11 Tier-B/environmental blockers (blocked, not
  waived); AC10-F1…F8; the nine R2.1-B conditional acceptances; A-08's
  deferred conditional-closure legs; A-20 implementation deferral; A-09,
  A-10, A-11/N-2 (M8), A-13 (M4 entry), A-21/OD-6, A-22/A-15, D-R2 and
  R22-E23-08/E6; frozen E1 schema v2, MDEF v2, the accepted Group E bundle
  (36/36) and AC9SCHEMA 1 (5/5); no perceptual-equivalence claim.
- **Known test/packaging/formatting exceptions remain open:** the 18 library
  failures are exactly the 7 preserved pins + 11 Tier-B/environmental
  blockers; clean-state `cargo package` verification (FPE-3.2 F6) and the
  pre-existing `cargo fmt --check` drift remain for the commit/CI pass. None
  is silently treated as resolved.
- **M4 is not authorised.** M4 requires a separate authorisation and planning
  decision. No production, test, evidence, schema or baseline-pin change is
  made by this update.

## 23. Dated update — M4.0 owner decisions D1–D4 (28 November 2026)

*Appended without rewriting any earlier section (history preserved).
Authority: [M4.0 owner decision](M4.0-owner-decision.md) (28 November 2026).*

- **D1:** M4.0 and M4.1 are authorized; M4.2 and later stages require
  explicit staged authorization. M4.1 implementation is blocked until the
  baseline checkpoint (D3) is established and verified.
- **D2 / A-13:** decided — FPE-side composite advance-stage mirror;
  `tns_encode_channel` delegated until M7; `finish_channel` shared; no Q31
  kernel extraction/refactoring authorized. A-13's status line (§5 above)
  is updated with a dated link; the historical text is preserved.
- **D3:** a safe, reproducible baseline checkpoint of the accepted M3/R2.2
  state is required before any M4 implementation. Investigation confirmed:
  HEAD is `189e6a7` (the historical checkpoint); the entire accepted M3/R2.2
  state is uncommitted working-tree content (19 modified + 31 untracked
  entries) with **no commit and no off-machine copy** (`main` is ahead 1 of
  `origin/main`); current-tree validation matches the accepted M3 state
  exactly (production baseline 2/2; `fpe::` 289/7 documented pins; full
  suite 671/18/2 documented set; evidence bundles 36/36 and 5/5 OK). The
  checkpoint procedure ([`../M4-baseline-checkpoint-plan.md`](../M4-baseline-checkpoint-plan.md))
  is specified and **awaits explicit owner approval of its Git operations**;
  the baseline is not yet established.
- **D4:** historical baseline pins are immutable; any successor baseline
  requires explicit owner approval; no silent rebaselining. The seven
  preserved pins, the 187 unjudged S4 flips, the frozen schema/MDEF and the
  accepted evidence bundles are untouched.
- No finding other than A-13 is resolved, closed, waived or reclassified by
  this update. No production, test, evidence, schema, ADR or pin change
  was made. M4.2+ remains unauthorized.
