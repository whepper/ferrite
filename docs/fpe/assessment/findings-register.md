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
- **Status:** Open. **Evidence:** `docs/fpe/` file inventory. **Uncertainty:**
  reviews may exist externally. **Risk:** Medium (governance). **Resolution:**
  bundle/link the M1/M2 reviews or record their absence and re-review.
  **Blocks:** G8 review completeness.

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
- **Status:** Open. **Evidence:** R2.1-B note §9 scope exclusion; no PE/ATS
  harness. **Risk:** **High** — largest gap before FPE-3.1 acceptance.
  **Resolution:** R2.2 harness (production-replay methodology extended to
  PE/ATS/S2/S4/S6). **Blocks:** FPE-3.1 acceptance; F-20/22/26 closure.

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
- **Status:** Open (architecture/planning). **Risk:** Medium — avoidable
  migration complexity. **Resolution:** explicit owner decision at M4 entry:
  FPE-side mirrors vs minimal additive baseline-gated Q31 kernel extraction.
  **Blocks:** clean M4–M7 execution (decision needed, not a hard blocker).

### A-14 — F-6 short-TNS tnsSpecShift≠0 structurally unreachable
- **Source:** this assessment; `07` §5.2; `psy.rs:1190-1213`; `f6_tns.rs`.
- **Description:** `tns_encode_channel` sets `tns_spec_shift_global` only in the
  long branch; D-022 forces CPE sequence agreement, so a short element cannot
  carry a long-channel overflow shift. The case is structurally unreachable;
  T1 scan + audit watch report 0 (pinned). The `07` §5.2 constructed-test
  requirement is defensive.
- **Status:** Open (scope clarification). **Risk:** Low. **Resolution:** owner
  decision to close F-6 via documented unreachability + retained watch (or
  construct a test-only synthetic). **Blocks:** formal R2 closure item.

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
- **Status:** Open. **Risk:** Medium (robustness before default). **Resolution:**
  explicit roadmap placement; implement via the existing `frontend` `Err(note)`
  seam (no new public API). **Blocks:** FPE-default readiness.

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
5. **A-14:** close F-6 via documented unreachability (or construct synthetic).
6. **A-20:** place `FpeError` graceful handling in the roadmap.
7. **A-01 / A-02:** recover M2 green-run + M1/M2 review records (or re-review).
8. **A-22 / A-15:** approve surgical documentation corrections + status index.
9. **OD-3 / A-07:** authorize long/real-material persistence runs for P closure.
10. **A-09:** approve perceptual-validation methodology and scope.

*No conditional acceptance is promoted by this register. F-07/09/10/15/17/18 and
F-20/22/26 remain conditional until their stated conditions are satisfied.*
