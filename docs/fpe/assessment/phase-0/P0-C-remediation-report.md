# Phase 0 — P0-C Remediation Report (N1–N7)

**Package:** P0-C remediation (owner dispositions of P0-C findings N1–N7).
**Responsible model:** Muse Spark 1.3 Contributor (OpenCode).
**Date:** 23 November 2026.
**Repository:** `whepper/ferrite`, branch `main`.
**Checkpoint / HEAD:** `eb2896923ec788727975268aca0beb27cba51760` (= `origin/main`, 0 ahead/behind).
**Authority:** Project-owner mission "Ferrite FPE — P0-C Finding Disposition and Remediation" (N1–N7 authorized; explicit boundaries listed in §9). This remediation implements; it does not commit, push, stage, clean, delete, or begin R2.2.

**Classification vocabulary (per finding):** PASS / PASS WITH FINDINGS / FAIL / NOT VERIFIED (per P0-C audit).

---

## 1. Executive summary

All seven P0-C findings are remediated as authorized. No production numerical, Q31, decision-definition, envelope, `B_closed`, supported-domain, API, manifest, lockfile, dependency, or licensing change was made. No commit, push, stage, amend, rebase, reset, cherry-pick, clean, or untracked-file deletion occurred.

- **N1 — README status:** corrected stale Phase 0 index to P0-A complete, P0-B complete, P0-C audit complete, overall **PASS WITH FINDINGS, pending owner disposition and finalization**.
- **N2 — Report accuracy:** corrected P0-B report line-count (`+1230 / −9`; 1239 was total changed lines), deleted-line composition (2 code + 7 Tier-B placeholder comment lines), and digest distribution (`some=242 / none=109`; corrected from `s200 SOME`). All values verified against implementation and audit evidence. Historical results and conclusions unchanged.
- **N3 — Canonical evidence emission:** added retained test `r21b_p0b_canonical_evidence_is_emitted` emitting the complete canonical rows (`R21B-CANONICAL <unique-id>` + full multi-line `canonical_string()` per finding in deterministic sorted order) and re-pinning the existing digest baseline `0x0544_1ee2_9087_55aa`. Ordering assertion strengthened to the full `(material_idx, frame, band, detail)` tuple. Digest preserved; decision logic and numerical behaviour unchanged.
- **N4 — Stable identifier uniqueness:** preserved original `stable_id` labels unchanged; added deterministic `unique_id` (`{stable_id}#{ordinal}`, per-label ordinal in sorted order) plus `unique_evidence_ids()` helper and retained test `r21b_p0b_stable_identifiers_are_unique_and_deterministic` asserting 30/30 unique, F-01/F-02 disambiguated, and identical identifiers across runs. Classification and decision semantics unchanged.
- **N5 — R1–R8 artifact preservation:** preserved surviving originals R1 (`is_ms.rs.orig`) and R5 (`r21b-review-checks.rs`) byte-for-byte in `docs/evidence/fpe-r21b/ferrite-fpe-r21b-review/` (SHA-256 verified, `diff`/`cmp` identical) with `SHA256SUMS` and provenance record `docs/evidence/fpe-r21b-provenance.md`. R2–R4 and R6–R8 recorded explicitly as unavailable (volatile temp cleanup post-audit); not reconstructed.
- **N6 — B_closed citation:** corrected the stale `b_closed` doc-comment citation to reference `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md` (owner-accepted D1 replacement) with the exact `+3` accounting (1 parity + 2 ceil-grouping; `−1` cancels). Derivation and implementation unchanged.
- **N7 — Commit preparation:** complete Phase 0 artifact set inventoried for owner-authorized commits (§8); full validation run in debug and release (§7); formatting and diff checks clean. No commit or push performed.

**Overall: PASS WITH FINDINGS — OWNER DISPOSITION AND COMMIT AUTHORIZATION REQUIRED.** The accepted limitation FR-5 remains open; FR-6's remaining closure action is committing the Phase 0 artifacts.

---

## 2. Exact changes

No historical record was rewritten. P0-A/P0-B/P0-C reports are preserved except for the surgical N1/N2 corrections noted below. The only tracked-file implementation change is the test-only `is_ms.rs` (plus the pre-existing P0-A owner-decision amendment, untouched by this remediation).

| ID | File(s) | Change |
|---|---|---|
| N1 | `docs/fpe/assessment/phase-0/README.md` (untracked, edited) | Status table: P0-A → **Complete** (unchanged); P0-B → **Complete** (2026-11-21, was "Not started — P0-B unauthorized"); P0-C → **Complete** (2026-11-21, was "Not started"); added `P0-C-remediation-report.md` row as Pending (this report), subsequently corrected under RA-2 to `Complete (2026-11-23; uncommitted, pending owner review)` (uncommitted); `phase-0-final-report.md` remains Not started — pending owner disposition and finalization. Header updated to "D1–D4 … P0-B/P0-C complete" with D4 scope note. Standing status rewritten to: P0-A/B complete, P0-C complete, overall **PASS WITH FINDINGS, pending owner disposition and finalization**; nine conditionals remain conditional; Tier-B C parity remains blocked; G0 historical/non-rerunnable (A-01 open); A-02 partially resolved; FR-5 open (accepted limitation); FR-6 working-tree pins pending commit; documentation reconciliation partially addressed (N1/N2/N6), conclusions otherwise unmodified. |
| N2 | `docs/fpe/assessment/phase-0/P0-B-reproducibility-report.md` (untracked, edited) | (a) Table cell: `+1239 / −9 (… evidence plumbing only)` → `+1230 / −9 (… 2 code lines + 7 Tier-B placeholder comment lines, behavior unchanged; 1239 = 1230+9 total changed lines, previously misreported as insertions)`. Verified: `git diff --numstat -- rust/src/aac_enc/fpe/tests/is_ms.rs` at P0-B time `1230 9`; deleted-line breakdown verified via `git diff -U0` (2 code lines `for m in materials()` / `analyze_frame` call + 7 Tier-B placeholder comment lines). (b) §4: `` `s200 SOME / 109 NONE` `` → `` `some=242 / none=109` `` with correction note (verified against reproduced gate output `some=242 none=109`, P0-A §6.1, P0-C §7, implementation-note §7 "242 MS_SOME frames"; re-verified 2026-11-23 via `cargo test … 'fpe::' -- --nocapture`, all gates print `some=242 none=109`). No historical result or conclusion changed. |
| N3 | `rust/src/aac_enc/fpe/tests/is_ms.rs` (tracked, test-only) | New retained test `r21b_p0b_canonical_evidence_is_emitted` (see §3). Strengthened ordering assertion in `r21b_p0b_per_finding_evidence_is_complete_and_traceable` from partial-triple check to full-tuple `assert!(key >= *p)` over `(material_idx, frame, band, detail)` (P0C-N3 tie-break). `sorted_evidences` sort function unchanged; digest procedure unchanged. |
| N4 | `rust/src/aac_enc/fpe/tests/is_ms.rs` (tracked, test-only) | `FindingEvidence::stable_id()` preserved byte-identical. Added `unique_id(&self, ordinal)` (`{stable_id}#{ordinal}`) + `unique_evidence_ids(sorted)` helper (linear scan over sorted view, no map iteration for output) + retained test `r21b_p0b_stable_identifiers_are_unique_and_deterministic` (see §4). No classification, routing, bound, envelope, or decision change. |
| N5 | `docs/evidence/fpe-r21b/ferrite-fpe-r21b-review/is_ms.rs.orig`, `…/r21b-review-checks.rs`, `…/SHA256SUMS` (new, untracked); `docs/evidence/fpe-r21b-provenance.md` (new, untracked) | Byte-for-byte copies via `cp -p` from original temp paths; `SHA256SUMS` (bare filenames) `2/2 OK`; `diff -q` + `cmp` identical for both files. Provenance record pins all eight SHA-256 values, original locations, verification commands, and explicitly lists R2–R4/R6–R8 as **Unavailable** (temp cleanup post-audit; pins retained, not reconstructed). See §5. |
| N6 | `rust/src/aac_enc/fpe/tests/is_ms.rs` (tracked, comment-only) | `b_closed` doc comment: "Candidate `B_closed` bound from the accepted R2.1-B analytical report." → "as reconstructed in `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md` (owner-accepted D1 replacement for the missing original …)". Formula, body, and `+3` value unchanged; trailing attribution corrected to "1 (min_tld parity) + 2 (four-way ceil grouping); the `−1` in `M` cancels in the delta (see reconstruction §3.2 Steps 1–4)". No derivation or implementation change. |
| N7 | This report `docs/fpe/assessment/phase-0/P0-C-remediation-report.md` (new, untracked) | Remediation record (this file). Commit set inventoried in §8; no commit/push/stage/clean performed. |

Tracked working-tree diff at report time (includes pre-existing P0-A amendment + P0-B + this remediation; see §8 for separation):

```text
46      0       docs/fpe/R2.1-B-owner-decision.md   (pre-existing P0-A D1–D3 append-only amendment; untouched)
1400    11      rust/src/aac_enc/fpe/tests/is_ms.rs  (P0-B 1230/9 + remediation N3/N4/N6 test-only additions)
```

`cargo fmt` applied; `git diff --check` clean. Full `is_ms.rs` change remains test-only (`#[cfg(test)]`-gated; no production source, manifest, lockfile, dependency, API, licensing, ADR, or decision-logic change).

---

## 3. N3 — canonical evidence emission (provenance and generation)

**Design constraints honored:** existing digest baseline preserved (`0x0544_1ee2_9087_55aa`); deterministic serialization unchanged (`canonical_string()` byte-identical); all required operands/margins/traces/line vectors included; unsupported-frame representation preserved; emitted artifact reproduces the pinned digest; no decision logic or numerical behaviour changed.

- **Provenance:** each emitted row is `FindingEvidence::canonical_string()` over values recorded from production-validated harness state (`build_frame_evidences` reads `analyze_frame` locals after the replay-equality gates; it never feeds a predicate). Included per row: identity (`id`, cause/status/consequence, gated/unsupported flags, `detail`); IS operands both arms (`e/eld/t/real_scale/is_scale/is_book/mid/pre`); stateless margins (I1, I2 dual, I3, I8 dual, I10 dual with `wrapping_sub`); MS operands/margins/bound/use plus six absolute Ld deltas; full frame trace chains both arms (`Debug` strings, walk order, incl. `FirstDivergence`/`Propagated`); Class B line vectors (`spec_off..spec_end` Q31 lines, both channels both arms). Unsupported frame F-01 emitted as `unsupported=1`, `DomainUnsupported`, `ms_supported=0` (never a pass). The band-−1 digest path records explicit `None`s (this corpus has 0 digest diffs, so the path is documented but unexercised — disclosed limitation, unchanged).
- **Generation procedure (exact):** from `rust/`, `cargo test --locked --lib r21b_p0b_canonical_evidence_is_emitted -- --nocapture` prints one `R21B-CANONICAL <unique-id>` header plus the full multi-line canonical row per finding in deterministic sorted order, then `R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x0544_1ee2_9087_55aa`. The test re-hashes with the identical procedure as `r21b_p0b_byte_baseline_is_pinned` (`sorted_evidences → canonical_string → lexicographic sort → join("\n") → FNV-1a 64`) and hard-asserts the preserved baseline on the reference platform (`macos/aarch64`), informational `eprintln!` elsewhere (per `production_baseline.rs` convention).
- **Verification:** `R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x05441ee2908755aa` reproduced in debug and release; cross-process byte-baseline output byte-identical across two separate runs (`diff` clean); `cargo test --locked --lib r21b_p0b -- --nocapture` 7/7 green (5 pre-existing + 2 new).

---

## 4. N4 — stable identifier uniqueness

- **Original labels preserved:** `stable_id()` (`{material}:f{frame}:b{band}`) is byte-identical to P0-B; it remains the `id=` field of the canonical string and the key of all historical prints (`R21B-FINDING`, `R21B-UNSUPPORTED`, `R21B-BASELINE` context).
- **Unique identifiers:** `unique_id()` appends the 1-based per-label occurrence ordinal in deterministic sorted order (`sorted_evidences`: `(material_idx, frame, band, detail)`), e.g. `quiet-sine-stereo:f16:b13#1` (F-01, `DomainUnsupported`) vs `quiet-sine-stereo:f16:b13#2` (F-02, `OutOfEnvelope`). The ordinal derives only from the sorted position and the retained `detail` tie-break; no classification or decision semantic is affected.
- **Retained test:** `r21b_p0b_stable_identifiers_are_unique_and_deterministic` asserts: 30 rows, 30 distinct unique ids (`BTreeSet`); each unique id preserves its stable label as prefix; exactly one colliding stable label exists (`quiet-sine-stereo:f16:b13`, count 2) with two distinct unique ids; `run_corpus()` executed twice yields byte-identical unique-id vectors, equal to the cached-corpus vector (`R21B-UNIQUE-IDS rows=30 unique=30`). Verified in debug and release, and across separate process runs via the determinism gate.

---

## 5. N5 — R1–R8 preservation and checksums

Preservation performed 2026-11-23 via `cp -p` (original temp files untouched). Verification from the bundle root (`shasum -a 256 -c SHA256SUMS`), plus `diff -q` and `cmp` against the originals.

| # | Artifact | SHA-256 pin (P0-A §3.2) | Status |
|---|---|---|---|
| R1 | `is_ms.rs.orig` (99,960 B, 2026-11-18 20:47) | `3e5ed6b6e43fa91c8e64bb05549b99bf2bb4b54e6e82dfc59b065a37df7f1f74` | **Preserved** — `SHA256SUMS: OK`, `diff -q` identical, `cmp` OK. SHA-1 `16459f94d7ab1ec86c3b01fa82e0829cf85b2b8a` matches owner review / Qwen §3.1 / DeepSeek §7.2. |
| R2 | `r21b_verify/ms_bound_verify.rs` (19,607 B) | `0922e0d6a5b084f23cf589369eb46e5361e5aacd508bfa867cfa04f94c5a2a28` | **Unavailable** — source absent; `T/r21b_verify/` empty (volatile temp cleanup after P0-C audit, which verified all pins intact). Rebuild binaries in `p0a-verification/`/`p0c-audit/` are derived, not originals. Pin retained; not reconstructed. |
| R3 | `r21b_verify/is_margin_verify.rs` (5,644 B) | `1e58e8f6a5e2df93ed70e2dca068859dc4b0a67c41127ce6e7ff67ca5aa954a4` | **Unavailable** — same as R2. Pin retained; not reconstructed. |
| R4 | `r21b_verify/final_math_verify.rs` (10,049 B) | `7a3ce18327743e2461b70f2d4e02d7bd5c79510c036e58a1174c0863c25c5313` | **Unavailable** — same as R2. Pin retained; not reconstructed. |
| R5 | `r21b-review-checks.rs` (6,546 B, 2026-11-18 21:12) | `20cd68a497031d510407dcd46a69ee608f09b4fbad630377cc5d8e40ade78591` | **Preserved** — `SHA256SUMS: OK`, `diff -q` identical, `cmp` OK. |
| R6 | `r21b_env_audit.log` (760,761 B) | `7115b305c70685642f59df7683123230e8254ee804ddbb5fb88eb3ec8378845b` | **Unavailable** — log absent (no `*env_audit*` match in temp). Pin retained; not reconstructed. |
| R7 | `r21b_env_audit2.log` (771,327 B) | `c3298901fda652b163a6e5e7216847669f496cf9180a746258b564b5aee52f8` | **Unavailable** — same as R6. Pin retained; not reconstructed. |
| R8 | `r21b_gates.log` (11,018 B) | `5640321e22a3f04d1b7f28069872c9872dc3b8b33a28028d74893d80c73dd4d5` | **Unavailable** — same as R6. Pin retained; not reconstructed. |

Preserved bundle `SHA256SUMS` (bare filenames, verified from the bundle root):

```text
3e5ed6b6e43fa91c8e64bb05549b99bf2bb4b54e6e82dfc59b065a37df7f1f74  is_ms.rs.orig
20cd68a497031d510407dcd46a69ee608f09b4fbad630377cc5d8e40ade78591  r21b-review-checks.rs
```

```
is_ms.rs.orig: OK
r21b-review-checks.rs: OK
```

M1/M2 bundles untouched and re-verified at report time: M1 **64/64 OK**, M2 **77/77 OK** (from each bundle root). Full provenance: `docs/evidence/fpe-r21b-provenance.md`.

---

## 6. Validation — exact commands and results

All from `rust/`, `--locked`, toolchain rustc/cargo **1.99.1** (`58cf8f9c3` / `5f94df478`), `aarch64-apple-darwin` (reference platform).

| # | Command | Result |
|---|---|---|
| 1 | `cargo test --locked --lib 'fpe::'` | **132 passed, 0 failed** (125 pre-existing + 5 P0-B + 2 P0-C remediation), 385 filtered |
| 2 | `cargo test --locked --release --lib 'fpe::'` | **132 passed, 0 failed** |
| 3 | `cargo test --locked --lib production_baseline` | **2 passed, 0 failed** (Q31 hashes unchanged) |
| 4 | `cargo test --locked --release --lib production_baseline` | **2 passed, 0 failed** |
| 5 | `cargo test --locked --lib -- --skip m5a_decode_check --skip m55_streams_match_c_reference --skip phase1_frontend_agreement_sine_f5 --skip phase1e_sine_f5_band16_residue --skip phase1e_onset_f24_bands_causality --skip phase1f_multitone_f1_no_live_divergence --skip phase1f_stepdown_f1_no_live_divergence --skip phase1g_long_flips_residue --skip phase1i_sine44_f5_tier_b --skip phase1i_multitone_f5_tier_b --skip phase1_full_stream_parity --skip intensity_probe_c_oracle_parity --skip intensity_probe_wav_parity --skip intensity_probe_adts_decodes --skip m511_profile_loop` (CI Tier-A skip set) | **500 passed, 0 failed**, 2 ignored, 15 filtered (498 + 2 new; no regression) |
| 6 | `cargo test --locked --test production_api` | **6 passed, 0 failed** |
| 7 | `cargo test --locked --doc -p ferrite-aac` | **19 passed, 0 failed** |
| 8 | `cargo fmt -- --check` (from `rust/`) | clean (exit 0) |
| 9 | `git diff --check` (repo root) | clean (exit 0) |
| 10 | `cargo test --locked --lib` (full, no skips) | **504 passed, 11 failed, 2 ignored** — the 11 failures are exactly the established artifact-gated Tier-B blocked set (no regression; +2 vs P0-C audit's 502 = the 2 new tests) |
| 11 | `cargo test --locked --lib r21b_p0b_byte_baseline_is_pinned -- --nocapture` ×2 (cross-process) | both pass; `R21B-BASELINE` rows + `R21B-EVIDENCE-DIGEST` byte-identical (`diff` clean) |
| 12 | `cargo test --locked --lib r21b_p0b_canonical_evidence_is_emitted -- --nocapture` | pass; `R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x05441ee2908755aa` (preserved baseline) |
| 13 | `cargo test --locked --lib r21b_p0b_stable_identifiers -- --nocapture` | pass; `R21B-UNIQUE-IDS rows=30 unique=30` |
| 14 | `bash rust/tools/check-licensing.sh` (from repo root) | OK (86 `rust/src` files carry TPMV notice; converter boundary intact) |
| 15 | `cargo clippy --locked --lib --tests` | 185 lib-test warnings (identical to pre-change inventory); **zero warnings reference `is_ms`**; manifest warning pre-existing (intentional dual license declaration per AGENTS.md) |

Reproduced gate output (identical to P0-A §6.1 / P0-C §7, plus new N3/N4 lines):

```text
frames=381 agree=365 findings=30 digest_diffs=0 coding_frames=16 short=12
start=true stop=true unsupported_frames=1 some=242 none=109
gate11: review-gated (P/G/F) findings=14
gate05: supported-domain bands all satisfy |shift| <= B_closed; unsupported-band frames=1
R21B-EVIDENCE-DIGEST rows=30 fnv1a64=0x05441ee2908755aa
R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x05441ee2908755aa
R21B-P0B determinism: 30 evidence rows identical
R21B-UNIQUE-IDS rows=30 unique=30
30 R21B-FINDING rows; 30 R21B-CANONICAL rows; R21B-UNSUPPORTED quiet-sine-stereo:f16:b13 … peers_in_frame=2
```

Five aggregate assertions verified: **30 findings / 16 coding frames / 0 digest diffs / 14 review-gated / 1 unsupported frame** (`r21b_p0b_aggregates_are_pinned` green in debug and release). Q31 baseline unchanged (2/2 debug and release). Full `is_ms` suite: **38 tests** (36 + 2 new). No test was suppressed and no expectation was weakened.

---

## 7. Unresolved items and deviations

None of the following blocks Phase 0 acceptance beyond owner action; all are recorded explicitly per the mission's accepted limitations and boundaries.

- **FR-5 — accepted limitation (no implementation authorized).** The `sum_abs_x` Class B example pin remains a documented limitation: the exact sum is printed but not individually hard-asserted. No additional implementation was performed in this remediation.
- **FR-6 — implemented; closure is the commit.** FR-6 (byte baseline) is implemented as working-tree test pins (P0-B B1 + N3 emission); its remaining closure action is owner-authorized commit of the Phase 0 artifacts (N7).
- **R2–R4 / R6–R8 unavailable.** Recorded explicitly in §5 and in `docs/evidence/fpe-r21b-provenance.md`; pins retained from P0-A; nothing reconstructed. The surviving leg (R1+R5 + embedded identities in the reconstruction + committed Qwen review §3.3 + per-run bound re-validation) is unchanged from the audit's assessment.
- **Fresh independent M2 review + sign-off:** authorized (D3), not yet performed. Not part of this remediation.
- **Nine conditional acceptances:** remain conditional; Tier-B C parity remains blocked, not waived; no promotion performed.
- **G0:** remains historical/non-rerunnable (A-01 open); G0 pass-through mode was not implemented (verified: no `pass_through|pass-through|passthrough` hits beyond this report's exclusion note).
- **Documentation reconciliation residual (line references, addressed):** the reconstruction's former line-number cites have been replaced with stable function references (`fn b_closed`, `fn ms_first_pass_margins`, `fn ms_band_supported`, `fn validate_ms_band`); the cited formulas and semantics are unchanged. This correction is limited to the line-reference issue and does not claim any broader documentation reconciliation is complete.
- **Deviations from the mission spec:** none. G0/R2.2/M3/M4/G-2/G-3/float-kernel work not begun; no production numerical, Q31, decision-definition, envelope, `B_closed`, supported-domain, FPE-default, manifest, lockfile, or dependency change; no conditional-acceptance promotion.

---

## 8. Git status and commit preparation (no commit performed)

HEAD is unchanged and equals the checkpoint and `origin/main`; no commit, push, stage, amend, rebase, reset, cherry-pick, clean, or untracked-file deletion occurred during this remediation.

```text
HEAD:                 eb2896923ec788727975268aca0beb27cba51760
origin/main:          eb2896923ec788727975268aca0beb27cba51760 (0 ahead/behind)
Tracked modifications (2):
 M docs/fpe/R2.1-B-owner-decision.md        (46/0; pre-existing P0-A append-only amendment; untouched)
 M rust/src/aac_enc/fpe/tests/is_ms.rs      (1400/11; P0-B 1230/9 + N3/N4/N6 test-only additions)
Untracked Phase 0 set (prepared for owner-authorized commits, complete inventory in `git status --porcelain -uall`):
  docs/fpe/assessment/phase-0/{README.md (N1-corrected), P0-A-evidence-recovery-report.md (preserved),
    P0-A-owner-disposition-record.md (preserved, incl. D4), P0-B-reproducibility-report.md (N2-corrected),
    P0-C-independent-audit.md (preserved), P0-C-remediation-report.md (this report)}
  docs/fpe/{R2.1-B-Bclosed-derivation-reconstruction.md, R2.1-B-G1c-absence-amendment.md} (D1/D2; preserved)
  docs/evidence/{fpe-m1-m2-provenance.md, fpe-m1/…, fpe-m2/…} (D3; preserved, checksums re-verified)
  docs/evidence/{fpe-r21b-provenance.md, fpe-r21b/ferrite-fpe-r21b-review/{is_ms.rs.orig, r21b-review-checks.rs, SHA256SUMS}} (N5; new)
Checks: `git diff --check` clean; `cargo fmt -- --check` clean (from rust/); `git diff --cached` empty (nothing staged).
```

**Prepared commit scope for owner authorization** (exact set above; staging and commit messaging remain owner-authorized actions — not performed here). Suggested separation, if the owner prefers atomic review: (1) N1/N2 documentation corrections + this report; (2) N3/N4/N6 test-only implementation; (3) N5 evidence bundle + provenance. All three are working-tree-ready and validated.

---

**STOP — P0-C remediation complete. Awaiting project-owner review, disposition of any residual items, and explicit commit authorization. R2.2 and any subsequent implementation phase are not begun.**
