# Phase 0 — Evidence Integrity & Acceptance-Baseline Hardening (package index)

**Checkpoint:** `eb2896923ec788727975268aca0beb27cba51760` (`whepper/ferrite`, `main`)
**Charter:** Phase 0 of the FPE roadmap (assessment
`../implementation-roadmap.md`); mission excludes all production numerical,
Q31, envelope, decision-definition, API, manifest, dependency and licensing
changes; no commits without owner authorization.

## Package contents

| Document | Role | Status |
|---|---|---|
| [P0-A-evidence-recovery-report.md](P0-A-evidence-recovery-report.md) | Evidence recovery, provenance classification, independent `B_closed` verification, reconstruction proposals | **Complete** (2026-11-21); preserved unmodified |
| [P0-A-owner-disposition-record.md](P0-A-owner-disposition-record.md) | Owner decisions D1–D3 (verbatim requirements) + implementation record; amendment D4 (P0-B authorization, 2026-11-21) | **Recorded** (2026-11-21; D4 appended same day) |
| [README.md](README.md) | This index | Current (P0-C remediation, 2026-11-23) |
| [P0-B-reproducibility-report.md](P0-B-reproducibility-report.md) | Byte baseline, aggregate assertions, per-finding retention | **Complete** (2026-11-21) |
| [P0-C-independent-audit.md](P0-C-independent-audit.md) | Independent audit of P0-A/P0-B | **Complete** (2026-11-21) |
| [P0-C-remediation-report.md](P0-C-remediation-report.md) | P0-C findings N1–N7 remediation record (this P0-C remediation package) | Complete (2026-11-23; uncommitted, pending owner review) |
| phase-0-final-report.md | Final execution report + readiness decision | Not started — pending owner disposition and finalization |

## Owner-disposition outputs (D1–D4, implemented 2026-11-21; P0-B/P0-C complete)

- **D4 (amended 2026-11-21):** P0-B authorized (Muse Spark 1.3): B1 byte
  baseline, B2 five aggregate assertions, B3 per-finding IS operands/margins/
  trace chains + Class B line vectors, deterministic ordering + explicit
  unsupported-frame handling, plus P0-B report. G0 pass-through mode
  explicitly excluded; G0 remains historical/non-rerunnable; A-01 remains
  open. FR-6 is addressed by the P0-B byte baseline (working-tree pins);
  FR-5 (`sum_abs_x=37` hard assertion) remains open (outside D4 scope).

- **D1:** `../../R2.1-B-Bclosed-derivation-reconstruction.md` —
  provenance-labelled, independently verified reconstruction; owner-accepted
  replacement for the missing original analytical report (OD-4 disposed).
- **D2:** `../../R2.1-B-G1c-absence-amendment.md` — G-1c/FR-1…FR-4 absence
  record + G-1 review attribution correction (retained review is DeepSeek's;
  Qwen final acceptance review missing); append-only amendment pointer in
  `../../R2.1-B-owner-decision.md`.
- **D3:** `../../../evidence/fpe-m1/ferrite-fpe-m1-review/` and
  `../../../evidence/fpe-m2/ferrite-fpe-m2-review/` — original M1/M2
  bundles preserved byte-for-byte (SHA-256 verified); provenance and
  review-occurrence record: `../../../evidence/fpe-m1-m2-provenance.md`.
  Fresh independent M2 review + sign-off **authorized, not yet performed**
  *(historical as of 2026-11-21; update 2026-11-24: the fresh M2 review
  has since been performed by GLM 5.3 Flash and M2 has been formally
  accepted — see Standing status below; the authorization statement
  remains accurate as history)*.

## Standing status (P0-C remediation, 2026-11-23; M2 acceptance update 2026-11-24)

- P0-A is **complete** (2026-11-21); P0-B is **complete** (2026-11-21,
  test-only + report, uncommitted); the P0-C independent audit is
  **complete** (2026-11-21).
- Overall Phase 0 assessment: **PASS WITH FINDINGS, pending owner
  disposition and finalization** (per P0-C audit §11; N1–N7 remediated in
  `P0-C-remediation-report.md`, uncommitted; final report not started).
- **M2 acceptance update (2026-11-24; history preserved):** a fresh
  independent M2 review was performed by **GLM 5.3 Flash** on
  24 November 2026
  (`../m2-independent-review-glm-2026-11-24.md`, **PASS WITH FINDINGS**)
  and the owner formally accepted M2
  (`P0-A-owner-disposition-record.md`, Amendment M2 acceptance,
  24 November 2026). **A-02 is closed** for M2 (completed, independently
  reviewed, owner-accepted); the original M2 review occurrence remains
  unconfirmed as history and the new review is a separate event. M2
  findings M2 F-1…F-4 remain non-blocking and retained. G0 remains
  historical/non-rerunnable (A-01 open).
- Nine conditional acceptances (F-07/09/10/15/17/18, F-20/22/26) remain
  **conditional**; Tier-B C parity remains **blocked**, not waived.
- G0 remains historical/non-rerunnable (A-01 open); A-02 **closed for M2**
  on 2026-11-24 (completed, independently reviewed, owner-accepted; the
  pre-2026-11-24 "partially resolved / not yet performed" wording remains
  accurate as history).
- FR-5 remains open (outside D4 scope; accepted limitation, P0-C audit
  §9.2/N7). FR-6 is implemented as working-tree test pins (P0-B B1) pending
  owner-authorized commit (P0C-N7); its remaining closure action is committing
  the Phase 0 artifacts.
- Documentation reconciliation (16→15 materials, frame-cap/PE-ATS wording,
  line cites) is partially addressed by the P0-C remediation (N1/N2/N6
  surgical corrections); the P0-B report and all assessment documents are
  otherwise preserved unmodified. No assessment conclusion has been rewritten.