# Phase 0 — P0-B Reproducibility & Acceptance-Baseline Hardening Report

**Package:** P0-B (test-only and documentation-only reproducibility hardening)
**Responsible model:** Muse Spark 1.3 Contributor (OpenCode)
**Date:** 21 November 2026
**Repository:** `whepper/ferrite`, branch `main`
**Checkpoint / HEAD:** `eb2896923ec788727975268aca0beb27cba51760` (= `origin/main`, 0 ahead/behind)
**Authority:** Owner decision D4 (P0-A disposition-record amendment D4, 21 November 2026):
B1 byte baseline, B2 five aggregate assertions, B3 per-finding IS operands/margins/trace
chains + Class B line vectors, B4 deterministic ordering + explicit unsupported-frame
handling, plus this report. **G0 pass-through comparison mode explicitly excluded —
not implemented; G0 remains historical/non-rerunnable and A-01 remains open.**

---

## 1. Executive summary

P0-B is complete. All authorized work is implemented in a single test-only file with no
production, manifest, lockfile, dependency, API, licensing, envelope, tolerance,
`B_closed`, domain, or decision-logic change:

- **B1 — R2.1-B byte-level baseline** (`r21b_p0b_byte_baseline_is_pinned`): per-material
  Q31/FPE ADTS byte length + FNV-1a 64 hash over the 15-material / 381-frame decision
  corpus, plus a canonical-evidence digest. Lengths pin on every platform; exact hashes
  pin on the reference platform (`macos/aarch64`), following `production_baseline.rs`.
- **B2 — five aggregate assertions** (`r21b_p0b_aggregates_are_pinned`): 30 findings,
  16 coding frames, 0 digest differences, 14 review-gated findings, 1 unsupported frame.
  Each verified against its authoritative source before pinning.
- **B3 — per-finding evidence retention** (`FindingEvidence` + `r21b_p0b_per_finding_evidence_is_complete_and_traceable`):
  IS operands, stateless decision margins, trace chains, MS operands/margins/`B_closed`,
  and Class B line vectors for all 30 findings, on the G-1 `BandRow` retention pattern.
- **B4 — determinism + unsupported handling** (`r21b_p0b_evidence_determinism_across_runs`,
  `r21b_p0b_unsupported_frame_is_explicit`): sorted deterministic ordering, canonical
  serialization, byte-identical re-run evidence, and the single unsupported frame kept
  explicit (never a pass, counted once in frames but twice in findings).

Validation: FPE **130/130 debug and release** (125 pre-existing + 5 new), Q31 production
baseline **2/2 debug and release** (unchanged), Tier-A **498/0** (493 + 5 new), public API
**6/6**, doctests **19/19**, `cargo fmt --check` clean, `git diff --check` clean, clippy
unchanged at 185 lib-test warnings with zero `is_ms` hits. No existing test regressed.

---

## 2. Exact files changed

| File | Change | Lines |
|---|---|---|
| `rust/src/aac_enc/fpe/tests/is_ms.rs` | **Only implementation file.** Added `FindingEvidence` schema + canonical serialization + `build_frame_evidences` recorder + `sorted_evidences` ordering + 5 P0-B gate tests (+ FNV helper). `analyze_frame`/`run_corpus`/`CorpusResult` extended to carry evidence alongside findings; no decision, bound, envelope, routing, or corpus predicate altered. `cargo fmt` applied. | +1230 / −9 (one signature gained `material_idx`; evidence plumbing: 2 code lines + 7 Tier-B placeholder comment lines, behavior unchanged; 1239 = 1230+9 total changed lines, previously misreported as insertions) |
| `docs/fpe/assessment/phase-0/P0-B-reproducibility-report.md` | This report (new). | new |

No other file was created, modified, or deleted. In particular: no production source
(`rust/src/aac_enc/*.rs` outside `fpe/tests/`), no `Cargo.toml`/`Cargo.lock`, no
dependency, no public API, no licensing/notice, no ADR, no owner decision or assessment
document, no preserved M1/M2 bundle, no P0-A report artifact was touched. No commit,
push, stage, amend, rebase, reset, cherry-pick, clean, or untracked-file deletion
occurred.

Pre-existing working-tree state (preserved, not mine): tracked modification
`docs/fpe/R2.1-B-owner-decision.md` (P0-A D1–D3 append-only amendment) and untracked
P0-A artifacts (`docs/evidence/fpe-m1/`, `docs/evidence/fpe-m2/`,
`docs/evidence/fpe-m1-m2-provenance.md`, `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md`,
`docs/fpe/R2.1-B-G1c-absence-amendment.md`, `docs/fpe/assessment/`) were left untouched.

---

## 3. B1 — byte-level baseline design

**Pattern:** mirrors `rust/src/aac_enc/production_baseline.rs` (per-corpus ADTS byte
length + FNV-1a 64 hash; lengths pinned everywhere, exact hashes enforced on the
reference platform `cfg!(all(target_os = "macos", target_arch = "aarch64"))` with an
informational `eprintln!` elsewhere).

**Scope (explicit):** the R2.1-B decision corpus as defined by `materials()` in
`is_ms.rs` — 8 fixtures (49/49/49/32/26/32/32/32 = 301 frames) + silence (8) + 6 probes
(12 each = 72) = **15 materials, 381 frames** — each stepped through the production
`StreamEncoder::step_frame` path on both engines (`R21BHarness` full pairs), with
per-frame ADTS structural validation (`assert_adts_valid`). Artifacts covered: Q31 ADTS
bytes and FPE ADTS bytes per material (length + hash), material sample rate and frame
count, total frame accounting, and a canonical-evidence digest (FNV-1a 64 over the
sorted per-finding canonical strings, `R21B-EVIDENCE-DIGEST`).

**Corpus scope and expected digests (pinned):**

| Material | sr | Frames | Q31 len | Q31 hash | FPE len | FPE hash |
|---|---|---:|---:|---|---|---|
| sine-stereo | 48000 | 49 | 3147 | `0x8921_b76c_2eb4_297e` | 3147 | `0x8921_b76c_2eb4_297e` |
| onset-stereo | 48000 | 49 | 2359 | `0x2a6c_a107_a2f0_ba5d` | 2359 | `0x2a6c_a107_a2f0_ba5d` |
| stepdown-stereo | 48000 | 49 | 2236 | `0xc5e3_cf81_d7c9_051d` | 2236 | `0xc5e3_cf81_d7c9_051d` |
| multitone-stereo | 48000 | 32 | 2846 | `0x12b8_5b86_7b81_65e4` | 2846 | `0x12b8_5b86_7b81_65e4` |
| tonal-noise-stereo | 48000 | 26 | 10645 | `0x09c8_010f_bb56_535b` | 10645 | `0x09c8_010f_bb56_535b` |
| quiet-sine-stereo | 48000 | 32 | 2709 | `0xcaf0_4c85_7e5d_6686` | 2709 | `0x3aaf_db40_df87_98e6` |
| sine-stereo-44100 | 44100 | 32 | 1968 | `0x3719_4169_f53b_1234` | 1968 | `0x3719_4169_f53b_1234` |
| onset-stereo-44100 | 44100 | 32 | 1220 | `0x66dc_3820_6189_19ee` | 1220 | `0x66dc_3820_6189_19ee` |
| silence-stereo | 48000 | 8 | 120 | `0x73f8_cdbf_8e69_830d` | 120 | `0x73f8_cdbf_8e69_830d` |
| probe-panned-48k | 48000 | 12 | 686 | `0xc032_917a_b7b3_ad8f` | 691 | `0x6dbb_d0d6_0b26_ecfc` |
| probe-outphase-48k | 48000 | 12 | 672 | `0xa727_d13c_20dc_a705` | 682 | `0xa5bb_f197_1dde_9932` |
| probe-centred-48k | 48000 | 12 | 667 | `0x14d0_9c29_47d2_6449` | 667 | `0x14d0_9c29_47d2_6449` |
| probe-narrow-48k | 48000 | 12 | 1336 | `0xf9f5_6aed_5d54_7f12` | 1330 | `0x75b8_344e_ed9d_a453` |
| probe-quiet-48k | 48000 | 12 | 1060 | `0x5e44_589b_6ef2_dc74` | 1060 | `0x5e44_589b_6ef2_dc74` |
| probe-panned-44100 | 44100 | 12 | 717 | `0xb809_7967_e420_9627` | 728 | `0xc22e_8852_3a89_bfbf` |

Canonical-evidence digest (30 rows, reference platform): `0x0544_1ee2_9087_55aa`.

**Reading the table:** fixture/silence materials are byte-identical between engines (no
findings there); the five differing materials are exactly the finding carriers
(quiet f16 F-01/F-02; panned/outphase/narrow/44100 probe frames). Identical length with
different hash (quiet) vs different length (probes) both fail loudly on drift. Baseline
values are hard-coded; nothing regenerates them during normal test execution.

---

## 4. B2 — five aggregate assertions and authoritative sources

Test `r21b_p0b_aggregates_are_pinned` hard-asserts (any corpus drift fails the suite):

| Metric | Expected | Authoritative sources (all cross-checked) |
|---|---:|---|
| Findings | 30 | Reproduced gate output (`frames=381 … findings=30`) + owner-review ledger §1/§3 (F-01…F-30) + implementation note §7 |
| Coding frames | 16 | Reproduced gate output (`coding_frames=16`) + owner-review §3 frame table (16 rows; quiet f16 carries two findings) |
| Digest differences | 0 | Reproduced gate output (`digest_diffs=0`); digest equality explicitly not per-band equality (owner-review §3) |
| Review-gated findings | 14 | Reproduced `gate11` output (`review-gated … findings=14`) + owner decision §1/§5 register (5 accepted + 9 conditional = 14) |
| Unsupported frames | 1 | Reproduced gate output (`unsupported_frames=1`) + owner-review §4 (quiet-sine f16 b13 sentinel domain) |

Corpus scope pinned alongside: 381 frames, 365 agreeing, 12 SHORT, START+STOP covered,
`some=242 / none=109` digest distribution reproduced (corrected from `s200 SOME`;
verified against reproduced gate output `some=242 none=109`, P0-A §6.1, P0-C §7,
and implementation-note §7 "242 MS_SOME frames"). The "16 materials" wording in
owner records is the known A-22 error (corpus defines 15; 16 is the coding-frame count).

---

## 5. B3 — per-finding evidence schema

`FindingEvidence` (test-only struct; recording only, never feeds any predicate):

- **Identity:** `material`, `material_idx` (corpus order), `frame`, `band`,
  `cause`/`status`/`consequence` (static strings), `detail`, derived
  `review_gated` (`consequence >= P`), `unsupported` (`status == DomainUnsupported`).
  Stable id: `{material}:f{frame}:b{band}` (duplicate band/frame pairs, e.g. F-01/F-02,
  disambiguate by detail + sort position; both retained).
- **IS operands (both arms, `None` only for band −1 digest findings):** `e_l/e_r`,
  `e_ld_l/e_ld_r`, `t_l/t_r`, `real_scale`, `is_scale`, `is_book`, mid-mask, pre-mask.
- **Decision margins:** stateless margins recomputed exactly with the trace helpers —
  I1 (via production `sfb_correlation`), I2 dual, I3, I8 dual, I10 dual
  (`wrapping_sub` raw delta, matching production). Stateful accumulator margins
  (I4/I6/I7, I5 carry/last) are **not** re-derived out of context; they travel in the
  retained trace events that carry them (see below). Truly unavailable values
  (band −1 digests) are `None`, never invented.
- **Trace chains:** full frame event lists for both arms (`q_trace`/`f_trace`) as
  `Debug` strings in production-walk order, including `FirstDivergence` and
  `Propagated` markers — sufficient to walk any finding from inputs through the
  decision.
- **MS operands/margins:** `ms_supported` (`ms_band_supported`), `M_q`/`M_f` (exact
  integer first-pass transcription), `B_closed`, `use_q`/`use_f`, and the six absolute
  Ld deltas — `None` only where no MS band exists (digest findings).
- **Class B line vectors:** Q31 spectral line slices for the band window
  `[spec_off..spec_end)` on both channels and both arms (`q_spec_l/q_spec_r`,
  `f_spec_l/f_spec_r`), following the G-1 `BandRow` retention pattern (retained rows
  with exact applied terms, incl. `sum_abs_x`-style mapped-line state — here the raw
  lines themselves, not just the scalar). Window bounds-checked; empty windows recorded
  as empty vectors, never omitted silently.

Test `r21b_p0b_per_finding_evidence_is_complete_and_traceable` asserts for all 30 rows:
identity present, flags consistent, IS operands + stateless margins + both traces +
MS operands/bound/use + line vectors present for every band ≥ 0, spec spans exact,
canonical serialization self-identical, and prints `R21B-FINDING …` rows in sorted order.
All 30 findings are per-band (digest diffs are 0, so the explicit `None` digest path is
documented but unexercised — recorded here as a limitation, not a gap in the 30).

---

## 6. Deterministic ordering and serialization

- **Ordering:** `sorted_evidences` sorts by `(material_idx, frame, band, detail)` —
  corpus order, never hash-map or filesystem order. The test asserts the sorted view is
  monotonic. (`run_corpus` itself iterates materials × frames × in-analysis band order,
  already deterministic; the sorted view makes the dump/hash order explicit.)
- **Serialization:** `FindingEvidence::canonical_string` uses fixed field order,
  decimal integers, bracketed decimal vectors, `|`-joined `Debug` trace strings, and
  `n/a` for absent optionals. No map iteration, no pointer formatting, no `f64`
  decimal formatting enters the canonical form (spectra are `i32` Q31 lines; the one
  float-adjacent scalar `sum_abs_x` is intentionally not part of the canonical string —
  raw lines are retained instead, preserving exact bit patterns by construction).
  Integer formatting is platform-independent; ADTS hashing uses the same FNV-1a 64 as
  `production_baseline.rs`.
- **Map discipline:** the only `HashMap`s in the path (`persist`, `div_first`) affect
  counters/minimums, never output order; evidence output derives solely from sorted
  vectors. No filesystem iteration participates.

---

## 7. Unsupported-frame representation

Test `r21b_p0b_unsupported_frame_is_explicit`:

- Exactly **one** unsupported finding (F-01): `quiet-sine-stereo:f16:b13`,
  `DomainUnsupported`, `ms_supported = Some(false)` — excluded from `B_closed`
  validation by the unchanged sentinel-domain rule (sentinel is not a log value).
- The same frame carries its coding consequence (F-02, `OutOfEnvelope`): one frame with
  two findings — counted **once** in `unsupported_band_frames` but **twice** in the
  finding population. The test pins both halves of that accounting.
- Unsupported is never a pass and never silently omitted: `route()` already rejects
  unroutable states, and the finding is printed as `R21B-UNSUPPORTED … ms_supported=0`.
- No sentinel value is invented: the domain flag plus retained MS deltas/bound carry
  the evidence; the report states where the sentinel lives (MS energy-Ld domain rule).

---

## 8. Tests executed — exact commands and results

All from `rust/`, toolchain rustc/cargo 1.99.1, `aarch64-apple-darwin`, `--locked`.
Pre-change reference (before implementation): FPE 125/125 debug, `is_ms` 31/31.

| Command | Result |
|---|---|
| `cargo test --locked --lib 'fpe::'` | **130 passed, 0 failed** (125 pre-existing + 5 new P0-B) |
| `cargo test --locked --release --lib 'fpe::'` | **130 passed, 0 failed** |
| `cargo test --locked --lib production_baseline` | **2 passed, 0 failed** (Q31 hashes unchanged) |
| `cargo test --locked --release --lib production_baseline` | **2 passed, 0 failed** |
| `cargo test --locked --lib -- --skip m5a_decode_check --skip m55_streams_match_c_reference --skip phase1_frontend_agreement_sine_f5 --skip phase1e_sine_f5_band16_residue --skip phase1e_onset_f24_bands_causality --skip phase1f_multitone_f1_no_live_divergence --skip phase1f_stepdown_f1_no_live_divergence --skip phase1g_long_flips_residue --skip phase1i_sine44_f5_tier_b --skip phase1i_multitone_f5_tier_b --skip phase1_full_stream_parity --skip intensity_probe_c_oracle_parity --skip intensity_probe_wav_parity --skip intensity_probe_adts_decodes --skip m511_profile_loop` (CI Tier-A skip set) | **498 passed, 0 failed**, 2 ignored, 15 filtered |
| `cargo test --locked --test production_api` | **6 passed, 0 failed** |
| `cargo test --locked --doc -p ferrite-aac` | **19 passed, 0 failed** |
| `cargo fmt -- --check` | clean (after `cargo fmt`) |
| `git diff --check` | clean |
| `cargo clippy --locked --lib --tests` | 185 warnings — identical count to the pre-change checkpoint inventory; zero warnings reference `is_ms` (no new lints) |

Determinism evidence: `r21b_p0b_evidence_determinism_across_runs` re-runs the full
corpus twice (bypassing the `OnceLock` cache) and asserts the 30 canonical strings are
byte-identical; `R21B-EVIDENCE-DIGEST … 0x0544_1ee2_9087_55aa` pins the digest. No test
was suppressed and no expectation was weakened to pass: the only new expectations are
the five authorized aggregates plus the new byte baseline.

---

## 9. Existing limitations (unchanged by P0-B)

- The band-−1 digest-evidence path (explicit `None`s) is covered by code but
  unexercised — this corpus has 0 digest differences. A future digest-flip corpus would
  exercise it without code changes.
- Exact ADTS/evidence hashes pin on the reference platform per the
  `production_baseline.rs` convention; other targets enforce lengths/frames and report.
- I4/I6/I7 and I5-carry margins are retained via trace events, not as standalone
  recomputed scalars (stateful accumulators are only meaningful in walk order).
- G-1 `BandRow` energy rows are referenced, not duplicated: P0-B retains the
  decision-side (IS/MS) leg; the energy leg remains owned by `probe_envelope.rs`.
- FR-5 (`sum_abs_x=37` hard assertion) remains open — outside the D4 scope.
- The nine conditional acceptances remain conditional; Tier-B C parity remains blocked;
  no perceptual evidence is claimed.

---

## 10. Confirmations

- **Production numerical invariance:** no production source was modified. The complete
  diff touches one test file (`rust/src/aac_enc/fpe/tests/is_ms.rs`, compiled
  test-only) plus this report. Q31 baseline 2/2 unchanged debug and release; FPE 130/130
  with the same 125 pre-existing passes; Tier-A +5 exactly (the new tests); API and
  doctests unchanged. No envelope, tolerance, `B_closed`, supported-domain, IS/MS
  definition, classification, acceptance, API, manifest, lockfile, dependency,
  licensing, or architecture/ADR item was altered, and no M3/M4/R2.2/G-2/G-3/float-kernel
  work was begun. No general refactoring was performed.
- **G0 pass-through mode was NOT implemented.** Verified by search
  (`pass_through|pass-through|passthrough` → no hits in `is_ms.rs`). G0 remains
  historical and non-rerunnable; A-01 remains open.
- **No commit or push occurred.** No stage/amend/rebase/reset/cherry-pick/clean; no
  untracked files deleted; P0-A artifacts preserved; `git diff --check` clean.
  Final state: HEAD `eb2896923ec788727975268aca0beb27cba51760` = `origin/main`;
  tracked modifications: `docs/fpe/R2.1-B-owner-decision.md` (pre-existing P0-A
  amendment) + `rust/src/aac_enc/fpe/tests/is_ms.rs` (this package).

---

## 11. Unresolved issues and deviations

None. No stop condition triggered: all five aggregates matched their authoritative
sources on first verification; the baseline required no production or decision change;
all required operands, margins, trace chains, and line vectors were available from
retained harness state; no decision definition was ambiguous; Q31 output is unchanged;
scope stayed within test-only + this report with no manifest/dependency/API edits.
Deviations from the mission spec: none — G0 excluded as instructed, FR-5 left open as
scoped (D4), and the evidence-digest pin additionally covers the canonical
serializations (a B1-compatible strengthening, still test-only).

---

**Handover: READY FOR INDEPENDENT P0-C AUDIT.** P0-B is complete as authorized. STOP —
P0-C audit and any R2.2 work await explicit owner authorization.
