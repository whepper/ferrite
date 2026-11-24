# FPE Next-Phase Recommendation & Readiness Decision

**Assessment date:** 21 November 2026
**Checkpoint:** `eb2896923ec788727975268aca0beb27cba51760` (`whepper/ferrite`, `main`)
**Authority:** Independent assessment recommendation; requires owner approval.
Companion: [post-checkpoint-assessment.md](post-checkpoint-assessment.md),
[findings-register.md](findings-register.md),
[validation-roadmap.md](validation-roadmap.md),
[implementation-roadmap.md](implementation-roadmap.md).

---

## 1. Readiness decision

### **CONDITIONALLY READY**

- **READY to begin** a bounded, primarily evidence-integrity and
  decision-safety-scoping phase (Phase 0 → Phase 1), **once the prerequisites in
  §4 are met**. The implementation is real, compiles, and passes all
  self-contained suites on the reference host (FPE 125/125 debug+release; Tier-A
  493/0; baseline 2/2; API 6; doctests 19; licensing/whitespace clean). Nothing
  prevents bounded, test-only/docs work from starting.
- **NOT READY** for:
  - **M3 / FPE-3.1 acceptance** — R2.2 (PE/ATS/downstream) outstanding (A-08);
    A-4/A-5/F-6 open; independent acceptance review not signed.
  - **Full R2.1-B milestone closure** — nine conditional acceptances
    (F-07/09/10/15/17/18, F-20/22/26) have **unsatisfied** conditions
    (termination; per-finding PE/ATS + perceptual evidence).
  - **Stage-1 closure** — Tier-B C parity blocked (G1/G5, A-10); M4–M9 not done.
  - **Any FPE-default change** — out of Stage 1; requires Stage-1 acceptance +
    perceptual evidence + FpeError handling (A-20).

This is not "NOT READY" globally: further *investigation* is not required to
start bounded work — the state is well understood. It is "CONDITIONALLY READY"
because specific, nameable prerequisites (mostly owner recovery actions + small
test-only hardening) must complete before the next phase is trustworthy, and
because acceptance/default remain blocked by outstanding evidence.

---

## 2. Recommended immediate next phase

### **Phase 0 — Evidence integrity & acceptance-baseline hardening**, immediately followed by **Phase 1 — R2.2 downstream decision-safety scoping/implementation**.

Phase 0 is small, cheap, and unblocks trust in the entire acceptance chain.
Phase 1 is the largest *technical* gap and the true prerequisite for FPE-3.1
acceptance. They are sequenced, not merged: Phase 0 makes the basis solid;
Phase 1 extends decision safety to where it currently stops (IS/MS → PE/ATS/
S2/S4/S6).

### Why this comes next (independent reasoning)

1. **The acceptance chain rests on missing artifacts.** The only MS bound
   (`B_closed`) cites an analytical report that is **not in the repository or
   wiki** (OD-4, Qwen R-7). The owner decision cites a **G-1c cleanup record that
   does not exist** in the package (A-04). The byte-level baseline is
   **uncommitted** (OD-6). R2.1-B aggregate counts are **printed, not asserted**
   (A-03). Per-finding IS operands came from a **removed** harness (A-05). A
   senior auditor cannot recommend building further acceptance on that footing.
   Closing these is low-effort and high-leverage.
2. **Decision safety stops exactly where perceptual risk begins.** IS/MS is
   well-validated, but PE/ATS/rate-control evolution is not (A-08). A one-LSB
   energy difference can shift PE → bit allocation → quality. The conditional
   acceptances F-20/22/26 explicitly lack this evidence. R2.2 is the necessary
   next technical step before FPE-3.1 can be accepted (`07` §5.2).
3. **It respects the owner's stated fork.** The owner decision §8 asks whether
   to prioritize recovery (Tier-B C reference + missing report) or a scoped
   downstream-evidence follow-up. This recommendation does **both in order**:
   recovery/hardening first (Phase 0), then downstream evidence (Phase 1), while
   Tier-B provisioning (A-10) proceeds in parallel as an owner action because it
   gates M8 and perceptual decode validation.
4. **It avoids premature implementation.** Jumping to M4 (float thresholds)
   before R2.2 + M3 acceptance would stack unvalidated float kernels on an
   unclosed decision-safety basis and force the A-13 seam decision under time
   pressure. The incremental strategy (`09`) exists precisely to prevent this.

---

## 3. Exact scope of Phase 0

**In scope (test-only + docs + owner recovery; no production numerical change):**

1. **Recover/link the R2.1-B analytical report** (`B_closed` derivation, OD-4)
   and the **G-1c cleanup record** (A-04); record permanent locations. If
   unrecoverable, reconstruct with explicit provenance (a new explanatory doc is
   *not* a silent substitute for the original derivation).
2. **Commit a byte-level baseline** for the R2.1-B decision corpus (OD-6/A-21),
   mirroring the `production_baseline.rs` pin pattern.
3. **Hard-assert R2.1-B aggregate counts** (30 findings / 16 coding frames / 0
   digest diffs / 14 review-gated / 1 unsupported frame) in retained tests
   (A-03), mirroring the G-1a sentinel-count pinning.
4. **Retain per-finding IS operands/margins/trace chains + Class B line vectors**
   via a retained dump (A-05/OD-7/OD-8), extending the G-1 `BandRow` pattern.
5. **Recover M2 green-run + M1/M2 review records** or record absence and re-review
   (A-01/A-02); optionally add a test-only pass-through comparison mode to make
   G0 re-runnable.
6. **Surgical documentation corrections + status index** (A-15/A-22): "16
   materials" → 15; frame-cap wording; PE/ATS wording; line cite; stale
   README/charter/roadmap status headers; link the owner decision + this
   assessment package from the README map.

**Explicit exclusions (Phase 0 must NOT):**

- Modify production Rust numerics, Q31 behaviour, the R1 envelope/tolerances,
  `B_closed`, the MS supported domain, or any decision definition.
- Change manifests, lockfiles, dependencies, public API, licensing, or header
  dates.
- Start R2.2 implementation, M3.2/M4+, G-2/G-3, or any float kernel.
- Promote any conditional acceptance; waive Tier-B; claim perceptual neutrality.
- Commit/push/rewrite history without owner instruction; remove untracked files.

---

## 4. Required prerequisites (before Phase 0/1 begin)

| # | Prerequisite | Owner action |
|---|---|---|
| 1 | Confirm recovery path for the analytical report + G-1c record (locate externally or authorize provenance-preserving reconstruction) | Owner |
| 2 | Authorize the committed byte-level baseline + R2.1-B aggregate pinning + operand retention (test-only) | Owner |
| 3 | Authorize surgical doc corrections + status index | Owner |
| 4 | Scope R2.2 (Phase 1): PE/ATS/S2/S4/S6 production-replay harness, per-finding attribution, P-termination runs, operating points | Owner + Strong |
| 5 | Initiate Tier-B provisioning (pinned `f14ad40a` C reference + IS/MS oracle dumps + decoder) in parallel — gates M8/P-5 | Owner |
| 6 | Decide the A-13 seam strategy *in principle* (needed at M4 entry, not Phase 0) | Owner + Strong |

---

## 5. Suggested implementation & review model

- **Phase 0:** Owner performs record recovery/provisioning. A **fast
  implementation model** does the mechanical, spec-bound work (aggregate
  pinning, byte baseline, retained-dump wiring, doc corrections) under a focused
  task spec (`11` §2 template). A **strong reasoning model** reviews any
  reconstructed derivation and the retention/pinning design. Independent review
  of the hardened evidence basis before it is cited in acceptance.
- **Phase 1 (R2.2):** A **strong reasoning model** designs the production-replay
  harness extension, derives downstream margins, and triages flips (residue vs
  structural). A **fast implementation model** wires mechanical replay calls and
  resynthesizes fixtures. **Independent numerical + decision review** (not the
  implementer) signs the gate; owner dispositions any new findings.
- **Throughout:** Q31 read-only; default unchanged; focused diffs; no
  self-approval of gates; no commits without owner instruction; licensing +
  `git diff --check` before reporting (`11`).

---

## 6. Single most valuable next action

> **Recover and commit (or permanently link) the missing R2.1-B analytical
> report (`B_closed` derivation) and the G-1c cleanup record, and establish the
> committed byte-level baseline for the decision corpus.**

This is the highest-leverage action because the **entire acceptance chain rests
on these artifacts**: `B_closed` is the only MS decision bound and its derivation
source is absent; the owner decision cites a cleanup record that does not exist;
and future change comparisons have no pinned baseline. Closing this provenance
gap is cheap, requires no numerical risk, and converts the acceptance basis from
"asserted with missing references" to "reproducible and committed". It is an
owner recovery action plus a small test-only hardening task — exactly the kind
of bounded work that can start immediately under conditional readiness.

The immediately following technical action is to **scope and build R2.2
(PE/ATS/downstream decision safety)**, the largest validation gap blocking
FPE-3.1 acceptance and the F-20/22/26 conditional closures.

---

## 7. What this recommendation deliberately does not do

- It does **not** authorize M4+ or any new float kernel before R2.2 + M3
  acceptance.
- It does **not** promote the nine conditional acceptances or waive Tier-B.
- It does **not** claim perceptual neutrality (no perceptual evidence exists).
- It does **not** treat the current float-energy hybrid as a float-native engine;
  the genuinely float-native objective remains a Stage-2 charter.
- It does **not** modify any production source, test, fixture, ADR, manifest or
  licensing arrangement — this is an assessment recommendation only.

*Readiness: **CONDITIONALLY READY** for Phase 0 → Phase 1; **NOT READY** for M3
acceptance, R2.1-B closure, Stage-1 closure, or FPE default.*
