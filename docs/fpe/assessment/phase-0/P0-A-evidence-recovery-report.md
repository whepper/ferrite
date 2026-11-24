# Phase 0 — P0-A Evidence Recovery and Derivation Verification Report

**Package:** P0-A (Evidence recovery and derivation verification)
**Responsible model:** Qwen4 Max Preview (OpenCode)
**Date:** 21 November 2026
**Repository:** `whepper/ferrite`, branch `main`
**Checkpoint / HEAD:** `eb2896923ec788727975268aca0beb27cba51760`
**Planning baseline:** the five assessment documents in `docs/fpe/assessment/`
(findings-register IDs preserved; no finding renumbered or reinterpreted).

**Authority note.** This report records evidence recovery, provenance
classification and independent verification. It does **not** modify any
production code, test, fixture, decision definition, envelope, tolerance,
`B_closed` behaviour, MS supported domain, manifest, lockfile, licensing file
or acceptance record. Nothing was committed, pushed, staged, amended,
rebased, reset, cherry-picked or cleaned. No reconstructed artifact is
presented as original evidence, and no reconstruction is treated as an
accepted replacement: owner authorization is explicitly requested (§9).
**P0-B has not been started.** Per the Phase 0 execution instruction, P0-B
requires owner review of this report first.

---

## 1. Baseline verification record (pre-package)

| Check | Result |
|---|---|
| Branch | `main`, tracking `origin/main`, no ahead/behind |
| `git rev-parse HEAD` | `eb2896923ec788727975268aca0beb27cba51760` — **identical to the published checkpoint** |
| `git rev-parse origin/main` | `eb2896923ec788727975268aca0beb27cba51760` |
| Live remote (`git ls-remote origin refs/heads/main`) | `eb2896923ec788727975268aca0beb27cba51760` — checkpoint present in local **and** remote history (it is the tip of both) |
| `git branch -a --contains eb28969…` | `main`, `remotes/origin/main` |
| Tracked modifications (`git diff`, `git diff --cached`) | **none** |
| Untracked files (`git status --porcelain -uall`) | exactly the five assessment documents: `docs/fpe/assessment/{post-checkpoint-assessment,findings-register,validation-roadmap,implementation-roadmap,next-phase-recommendation}.md` |
| `git stash list` | empty |
| `git diff --check` | clean |
| Toolchain | rustc 1.99.1 (`58cf8f9c3`), cargo 1.99.1 (`5f94df478`), `aarch64-apple-darwin` — the recorded reference platform |

**Worktrees (read-only inspection, untouched):** two pre-FPE worktrees exist
under the session temp area (`ferrite-ab-2.4.5/wt-candidate` at `02158ff`,
`wt-scalar` at `1c825c6`, both detached). Both predate the FPE commits and
contain no FPE evidence.

**Dangling git objects (`git fsck --dangling`, read-only):** 5 dangling
commits, 1 dangling tree, ~150 dangling blobs. All five commits inspected:
pre-FPE WIP/stash states (Oct 25 – Nov 14, quant/rate/convert/licensing work).
A content scan of all dangling blobs for `B_closed|G-1c|analytical|R2.1-B|
M1/M2|pass-through|green.run` returned **zero matches**. No FPE evidence
exists in unreachable git objects.

**`.DS_Store` observation (P0A-O3).** The mission and the assessment refer to
three existing `.DS_Store` files. A full filesystem scan (`find . -name
.DS_Store`, including `.git`) finds **none present** in the current tree.
Phase 0 has not removed or cleaned anything; the files were already absent
before this package started. The M1/M2/G-1b records (§5) confirm
`.DS_Store` / `docs/.DS_Store` existed as untracked files on 2026-11-17/21.
Their disappearance predates Phase 0 and is recorded as a baseline
discrepancy; no action is required or taken.

---

## 2. Search methodology

Locations searched for the missing artifacts (A1 analytical report, A2 G-1c
record, A3 M1/M2 records), with methods:

1. **Working tree:** full read of `docs/fpe/` (29 entries incl. ADRs), scoped
   `grep` for `B_closed`, `G-1c`, `analytical`, `FR-[1-6]` over `docs/` and
   `rust/`.
2. **Git history (all refs):** `git log --all --oneline` (full history,
   140 commits); `git log --all --diff-filter=D --name-only` (every deleted
   path — **no `docs/fpe/` file was ever deleted**; all deletions predate
   FPE); pickaxe `git log --all -S'G-1c'`, `-S'analytical report'`,
   `-S'B_closed'` (matches only the five FPE commits that introduce the
   referencing documents — the artifacts themselves were never committed).
3. **Unreachable objects:** `git fsck --dangling` + per-commit `git show
   --stat` + per-blob content scan (§1). `git stash list` (empty).
4. **Reflog:** `git reflog -30` — the five FPE commits were created
   2026-11-21 14:46:29–14:46:51 (squash-commit of previously untracked work,
   consistent with the Qwen review's 2026-11-18 observation that the FPE
   tree was untracked). No pre-squash FPE commit state exists in the reflog.
5. **FPE documentation and owner-review evidence:** all R2.1-B notes,
   reviews, reconciliations, owner review/decision, G-1 chain, m0 record
   read; every reference to the missing artifacts catalogued (§3–§5).
6. **Available project records (session temp, approved area
   `/private/var/folders/6h/…/T/opencode/`):** full inventory of FPE-related
   directories (`fpe-m0`, `fpe-m1`, `fpe-m1-review`, `fpe-m1-review-verify`,
   `fpe-m2-review-bundle`, `fpe-m2-review-verify`, `fpe-review`, `fpe-ref`,
   `fpe-r2-review`, `fpe31r1a-review`, `fpe31r1a3-review`, `r1-review-scratch`,
   `r21b_verify`, `m7-audit`) and top-level FPE logs/files; targeted content
   scans for `G-1c`, `B_closed`, `analytical`. **This is where the surviving
   original evidence was found** (§3.2, §5).
7. **Outline wiki (wiki.whepper.com):** `list_documents` full-text queries
   (incl. `includeArchived: true`): `"R2.1-B"`, `"G-1c"`, `"B_closed"`,
   `"FPE analytical report"`, `"Ferrite FPE"`, `"G-1 probe"`, `"FPE M2"`,
   `"decision-safety"`. **Zero FPE-related documents** (the single
   `B_closed` hit is an unrelated MPAK container-design spike). Independently
   confirms Qwen R-7's 2026-11-18 wiki-search result.
8. **Referenced audit material:** the Qwen review's scratch checker path
   (`…/T/opencode/r21b-review-checks.rs`) — **found and recovered**; the
   DeepSeek removed-scaffold outputs — **found and recovered** (§3.2).

Search limits (disclosed): no access to other machines, private backups,
chat transcripts outside the session-temp artifacts listed above, or any
owner-private storage. If the originals exist there, only the owner can
produce them.

---

## 3. A1 — R2.1-B analytical report (`B_closed` derivation)

### 3.1 Recovery status: ORIGINAL NOT RECOVERED

The "accepted FPE R2.1-B analytical report (2026-11-18)" — cited as the
baseline/source of truth by `R2.1-B-implementation-note.md` (line 9) and
`R2.1-B-owner-review.md` (line 9), and as the derivation source of `B_closed`
by `is_ms.rs:292` — **does not exist in any searched location**. It was never
committed (pickaxe + deletion history), is not in the working tree, not in
unreachable git objects, not in the session-temp records, and not in the
wiki. Every repository mention is a *reference to* the report, never the
report itself (consistent with OD-4, QR-7, A-04-adjacent observations in the
G-1/G-1a/G-1b documents, which all explicitly leave it open).

Additional provenance gap: the report's **"accepted" status itself has no
retained acceptance record** (no owner decision predating the implementation
note approves it; the owner decision of 2026-11-21 §6 lists it as *Missing*).
The acceptance is asserted by the implementation note and owner review only.

### 3.2 Recovered artifacts (original, contemporaneous)

All paths below are in the approved session-temp area
`/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/`
(abbreviated `T/`). SHA-256 digests pin the recovered bytes.

| # | Artifact | SHA-256 | Provenance / classification |
|---|---|---|---|
| R1 | `T/is_ms.rs.orig` (99,960 B, mtime 2026-11-18 20:47) | `3e5ed6b6e43fa91c8e64bb05549b99bf2bb4b54e6e82dfc59b065a37df7f1f74` | **Original.** Byte-identity backup of the R2.1-B delivery `is_ms.rs`. SHA-1 = `16459f94d7ab1ec86c3b01fa82e0829cf85b2b8a` — **matches** the hash recorded in the owner review Method, Qwen review §3.1 #6 and DeepSeek review §7.2. Author: implementation owner (model not named in the artifact). |
| R2 | `T/r21b_verify/ms_bound_verify.rs` (19,607 B, 2026-11-18 16:27) + compiled binary | `0922e0d6a5b084f23cf589369eb46e5361e5aacd508bfa867cfa04f94c5a2a28` | **Original** verification scaffold ("R2.1-B correction-task verification scratch, out-of-repo, analysis only"). Contains the exact closed-form bound, the shift lemma, i32-vs-i64 equivalence check, boundary table, adversarial pair generators, exhaustive reduced-domain check, digest truth-table simulation. Author not stated in-file; contemporaneous with the R2.1-B review cycle; exact session attribution requires owner confirmation. **Re-run by P0-A: §6.2.** |
| R3 | `T/r21b_verify/is_margin_verify.rs` (5,644 B) | `1e58e8f6a5e2df93ed70e2dca068859dc4b0a67c41127ce6e7ff67ca5aa954a4` | **Original** IS-margin verification scaffold (same caveat on attribution). Not re-run by P0-A (IS margins are covered by the retained boundary unit tests, re-run green). |
| R4 | `T/r21b_verify/final_math_verify.rs` (10,049 B) | `7a3ce18327743e2461b70f2d4e02d7bd5c79510c036e58a1174c0863c25c5313` | **Original** final math verification scaffold (same caveat). |
| R5 | `T/r21b-review-checks.rs` (6,546 B, 2026-11-18 21:12) + binary | `20cd68a497031d510407dcd46a69ee608f09b4fbad630377cc5d8e40ade78591` | **Original.** The Qwen adversarial review's REP scratch checker, referenced by path in `R2.1-B-independent-review-qwen.md` §15. Author: Qwen4 Max Preview (stated in the review). **Re-run by P0-A: §6.3.** |
| R6 | `T/r21b_env_audit.log` (760,761 B, 2026-11-18 20:54) | `7115b305c70685642f59df7683123230e8254ee804ddbb5fb88eb3ec8378845b` | **Original** output of the DeepSeek review's temporary `r21b_env_tmp::r21b_env_audit_tmp` scaffold (R1-over-R2.1-B envelope audit; per-finding rows for F-19…F-26 etc.; test line `… ok`, `1 passed`). The scaffold code was removed from `is_ms.rs` (SHA-1 restoration verified by DeepSeek §7.2); **its output survives here**. |
| R7 | `T/r21b_env_audit2.log` (771,327 B, 2026-11-18 20:59) | `c3298901fda652b163a6e5e7216847669f4966cf9180a746258b564b5aee52f8` | **Original**, second run of the same scaffold (extended finding-data rows). |
| R8 | `T/r21b_gates.log` (11,018 B, 2026-11-18 21:01) | `5640321e22a3f04d1b7f28069872c9872dc3b8b33a28028d74893d80c73dd4d5` | **Original** R2.1-B gate run log (review-cycle contemporaneous). |

Not recovered: the output of the DeepSeek `b_closed_random` 500,000-trial
check reported in `R2.1-B-numerical-review-deepseek.md` §7.1 was **not found**
in the recovered logs (the audit logs capture `r21b_env_audit_tmp` only).
That specific result remains **reported-only** (DeepSeek review), but the
same claim is independently re-established by P0-A's re-run of R2 with
28.4M pairs (§6.2), which is a superset in coverage.

### 3.3 The `is_ms.rs` hash chain (resolved discrepancy)

The committed `rust/src/aac_enc/fpe/tests/is_ms.rs` has SHA-1
`421df1a6609bf0785a8722b080830791daad914c`, **different** from the
review-cycle hash `16459f94…`. P0-A diffed the recovered backup R1 against
the committed file: the **only** changes are 9 lines making `probe_sine` and
`probe_narrow` `pub(super)` with doc comments "for G-1 probe-corpus reuse
(`probe_envelope`): the same generator function feeds both harnesses, so the
signals cannot diverge" (committed file mtime 2026-11-18 21:34). **No
numerical, decision, margin, bound or corpus change.** This is the documented
G-1 reuse seam; the owner-review per-finding evidence (generated against
`16459f94…`) therefore applies unchanged to the committed harness, and the
reproduced aggregates (§6.1) confirm it. Classification: **Original** backup
+ **Derived** diff analysis (independently verified by P0-A).

### 3.4 Independent mathematical verification of `B_closed` (P0-A derivation)

**Status: the bound is mathematically sound on its supported domain, and the
implemented formula is exactly the sound closed form.** This derivation was
produced by P0-A (Qwen4 Max Preview) from the production margin semantics;
it is **Reconstructed/Derived evidence, not the original report**, and is
submitted for owner authorization (§9, D1) before being treated as an
accepted replacement.

**Inputs (per band, both arms).** Six Q31 log-domain integer operands:
`eld_l, eld_r` (channel energy Ld), `tld_l, tld_r` (threshold Ld), `em, es`
(mid/side energy Ld). Supported domain (`ms_band_supported`,
`is_ms.rs:326-348`): no `i32::MIN` sentinel in any of the four energy-Ld
operands on either arm; IS-coded bands excluded on either arm. All operands
finite integers. Deltas `dEL, dER, dEm, dEs, dTL, dTR` are absolute cross-arm
deltas (the operand-domain reading documented at `is_ms.rs:300-308`).

**Margin definition (exact transcription of production `ms_stereo` first
pass, halved-shift i64 form; replay-equality gates 1–2 assert it against
production on every frame):**

```text
min_tld = min(tld_l, tld_r)
pnlr    = (tld_l>>1) − (max(eld_l,tld_l)>>1) + (tld_r>>1) − (max(eld_r,tld_r)>>1)
pnms    = min_tld − (max(em,min_tld)>>1) − (max(es,min_tld)>>1)
M       = pnms − pnlr − 1 ;   use_ms ⟺ M ≥ 0
```

**Lemmas (integer arithmetic; `>>1` = arithmetic shift = floor division):**

- **L1 (shift lemma).** `|(a>>1) − (b>>1)| ≤ ceil(|a−b|/2)` for all integers
  (incl. negatives). Verified exhaustively on [−2000,2000]² + 2,000,000
  random full-width pairs by re-run of R2 (0 failures).
- **L2 (min/max Lipschitz).** `|max(a1,b1) − max(a2,b2)| ≤ max(|a1−a2|, |b1−b2|)`;
  likewise `min`. Standard; used with `T := |Δmin_tld| ≤ max(dTL,dTR)`.
- **L3 (ceil grouping).** For non-negative integers,
  `ceil(a/2)+ceil(b/2)+ceil(c/2)+ceil(d/2) ≤ ceil((a+b+c+d)/2) + 2`
  (parity argument: LHS = `(S+k_odd)/2`, RHS = `(S+(S mod 2))/2 + 2`,
  `k_odd ≤ 4`, `k_odd ≡ S (mod 2)`).

**Derivation steps.**

1. Rewrite each `pnlr` pair: `g(eld,tld) := (tld>>1) − (max(eld,tld)>>1) =
   min(0, (tld>>1) − (eld>>1))` (case split `eld ≥ tld` / `eld < tld`; both
   give equality). Since `min(0,·)` is 1-Lipschitz,
   `|Δg| ≤ ceil(dEL/2) + ceil(dTL/2)` by L1. Hence
   `|Δpnlr| ≤ ceil(dEL/2)+ceil(dER/2)+ceil(dTL/2)+ceil(dTR/2)`.
2. Rewrite `pnms`: with `min_tld = 2·(min_tld>>1) + r`, `r ∈ {0,1}`, and the
   identity `(max(em,min_tld)>>1) − (min_tld>>1) = max(0, (em>>1)−(min_tld>>1))`
   (same case split),
   `pnms = r − max(0,(em>>1)−(min_tld>>1)) − max(0,(es>>1)−(min_tld>>1))`.
   Therefore `|Δpnms| ≤ 1 + ceil(dEm/2) + ceil(dEs/2) + 2·ceil(T/2)`.
3. The `−1` in `M` cancels in `ΔM = Δpnms − Δpnlr`. With
   `S := dEL+dER+dEm+dEs`, L3, `ceil(x/2) ≤ x`, and `2·ceil(T/2) ≤ T+1`:
   `|ΔM| ≤ ceil(S/2) + dTL + dTR + T + 4`.
4. If `T = 0`: `|ΔM| ≤ ceil(S/2) + dTL + dTR + 3`. If `T ≥ 1`:
   `T + 4 ≤ 2T + 3`. In both cases, with `T ≤ max(dTL,dTR)`:

```text
|M_q − M_f| ≤ ceil((dEL+dER+dEm+dEs)/2) + dTL + dTR + 2·max(dTL,dTR) + 3
            = B_closed
```

**Resulting bound: identical to the implemented `b_closed`
(`is_ms.rs:309-314`), to the recovered scaffold R2, and to the formula used
in every owner-review/Qwen/DeepSeek calculation. No difference; no
implication for decision behaviour.** The `+3` constant is exactly accounted
for (1 from `min_tld` parity `r`, 2 from the four-way ceil grouping); the
doc-comment's attribution ("strict-`>` unit plus halving floors") is a loose
but harmless description — the `−1` actually cancels in the delta.

**Robustness corollary (retained debug assertion, `is_ms.rs:399-407`):**
`|M_q| > B` forces `use_f = use_q` (`M_q ≥ B+1 ⟹ M_f ≥ 1`; symmetrically
`M_q ≤ −B−1 ⟹ M_f ≤ −1`). Sound.

**Assumptions register:** (i) exact margin transcription (gated by replay
equality, re-run green); (ii) finite operands — the supported-domain rule;
(iii) pure integer arithmetic (no float in the bound); (iv) deltas read as
Q31 log-domain absolute deltas. The derivation holds for *arbitrary* finite
i32 operands (it does not need the threshold floor or non-positive-energy
constraints); the domain rule is nevertheless retained unchanged because a
sentinel is not a physical log value (DeepSeek §5.3: one-sided MIN or
differing `min_tld` makes the formula vacuous, not wrong — **no domain
extension is proposed or performed**).

**Empirical note:** over 28,409,561 checked pairs the minimum slack `B−|ΔM|`
was 1 (no pair attained `B` exactly). The bound may be loose by 1 in some
regimes; **no tightening is proposed** (Phase 0 exclusion: `B_closed`
decision behaviour unchanged).

### 3.5 Reconstruction proposal (A1) — requires owner authorization

Because the original is unrecoverable from all available sources, P0-A
proposes, **for explicit owner decision (D1)**:

1. Adopt §3.4 of this report (inputs, assumptions, lemmas L1–L3, steps 1–4,
   resulting bound, robustness corollary) as a **provenance-labelled
   reconstruction** of the `B_closed` derivation, authored by Qwen4 Max
   Preview, 2026-11-21, independently verified empirically (§6.2–6.3) and
   consistent with every retained calculation (owner review, Qwen §3.3,
   DeepSeek §5.1). Classification: **Reconstructed + Derived**, pending
   **Owner accepted** status.
2. If authorized, commit it as a new document (e.g.
   `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md`) that states
   prominently: the original analytical report (2026-11-18) is missing;
   this is a reconstruction, not the original; the original's acceptance
   record is also missing; the bound formula and supported domain are
   unchanged.
3. Update referencing documents **by explicit amendment only** (OD-4 status:
   "original missing; reconstruction accepted by owner on …" — never
   silently rewriting the historical references).
4. If the owner can still produce the original (private records), the
   original supersedes the reconstruction; the reconstruction must then be
   reconciled against it and any discrepancy reported.

**No reconstruction has been committed or treated as accepted.** The
derivation in §3.4 is evidence submitted for review, nothing more.

---

## 4. A2 — G-1c cleanup record

### 4.1 Recovery status: ORIGINAL NOT RECOVERED

- `grep G-1c` over `docs/` and `rust/`: only the two owner-decision
  references (§2 item 5 "The subsequent G-1c documentation cleanup"; §6
  "FR-1 through FR-4 were addressed by G-1c") and the assessment documents.
- Never committed (pickaxe `-S'G-1c'` matches only `eb28969`, which
  introduces the owner decision itself); no deleted path; no dangling
  object; no session-temp file mentioning G-1c (targeted scan of all FPE
  temp directories and logs); no wiki document.
- **FR-1…FR-4 are defined nowhere** — not in the G-1 report, G-1a note,
  G-1a confirmation, G-1b report, DeepSeek G-1 review, or any recovered
  artifact. Only FR-5 (`sum_abs_x=37` printed, not hard-asserted) and FR-6
  (no committed byte baseline) are defined inline in the owner decision §6.

### 4.2 What can be established

- **Last retained reconciliation:** `R2.1-B-G1b-reconciliation-report.md`
  (2026-11-21, documentation-only; G1b-1…G1b-4 + R-3/R-4/R-6 corrections).
  It explicitly states: "The original analytical report remains outstanding"
  and "Qwen final review and overall G-1 acceptance are not performed or
  declared here."
- **Committed-state consistency (spot-verified):** the committed G-1 report
  contains the G-1b "After" state at the checked locations (§11 Q31→FPE
  directions `58→59`/`4→5`; §13 "125 cargo-passed" split; §4 grouped
  wording). No evidence of undocumented numerical change. A full
  G-1b-vs-committed reconciliation audit is assigned to P0-C.
- **What cleanup G-1c documented:** **unknown.** No record survives. By
  owner-decision context it was a *documentation* cleanup addressing FR-1…FR-4
  ("G-1 optional documentation findings") after G-1b and before the owner
  decision (both 2026-11-21; the g1-* verification logs in temp are timestamped
  13:09–13:40, the commits 14:46). It affected **no corpus and no numerical
  finding** per the owner decision's own framing (FR items are documentation
  findings; FR-5/FR-6 explicitly remain open). This cannot be independently
  verified beyond the retained evidence's continued reproducibility.
- **Final G-1 evidence remains reproducible: YES.** P0-A re-ran the retained
  G-1 suite (§6.1): `probe_envelope` 13/13 green, six probes × 14 frames,
  hard-asserted sentinel counts, 56 finding rows, zero failures/unsupported.
  The G-1 acceptance boundary (energy-envelope scope, 28 findings) rests on
  retained, re-runnable evidence — not on G-1c.
- **Owner-decision accuracy:** the G-1 acceptance boundary (§2.1) is
  supported by retained evidence. The claim "FR-1…FR-4 were addressed by
  G-1c" is **not independently verifiable** (no record, no definitions).
  See also P0A-O1 below.

### 4.3 New observation P0A-O1 — G-1 review attribution / missing final review

The owner decision §2 item 4 states: "Independent G-1 review by **Qwen**,
which returned **PASS WITH FINDINGS**." The retained independent G-1
numerical review (`R2.1-B-G1-numerical-review-deepseek.md`) is authored by
**DeepSeek** (verdict PASS WITH FINDINGS), and the entire G-1 chain
(DeepSeek G-1 review §H; G-1a confirmation §8; G-1a note §6; G-1b §4)
repeatedly defers to a **pending "Qwen final independent acceptance review"**
that is **not retained anywhere in the package**. Two possibilities:

1. A Qwen final G-1 acceptance review was performed but never retained —
   a further missing record, plausibly the **source of the FR-1…FR-6 list**
   (FR-5/FR-6's character matches a final acceptance review's retention
   findings). G-1c would then be the cleanup applying that review's
   optional documentation findings.
2. The owner decision misattributes the DeepSeek review to Qwen.

Either way the acceptance chain cites an unretained review. This observation
is **additional to** A-04 (not a renumbering); it requires owner
clarification (D2). P0-A takes no position on which possibility holds.

### 4.4 Reconstruction proposal (A2) — requires owner authorization

The G-1c record **cannot be faithfully reconstructed**: its subject matter
(the FR-1…FR-4 definitions and the cleanup actions) is unknown, and
inventing content would violate the evidence-integrity rules. Proposal
(for owner decision D2):

1. **If the owner holds the G-1c record, the FR source list, or the Qwen
   final G-1 review externally:** bundle/link them (permanent location),
   and P0-C will reconcile them against the committed documentation state.
2. **If unrecoverable:** authorize a **provenance-preserving absence
   amendment** to the owner decision (append-only, dated, owner-signed):
   record that G-1c and the FR-1…FR-4 definitions are unretained; that the
   "addressed by G-1c" claim is unverifiable; that the G-1 acceptance
   boundary stands on the retained, re-runnable G-1/G-1a/G-1b evidence
   (re-verified by P0-A §6.1 and P0-C); and resolve the P0A-O1 attribution
   (either produce the Qwen final review or correct the attribution by
   amendment). FR-5/FR-6 remain open and are routed to P0-B (FR-6 ≡ OD-6
   byte baseline; FR-5 assertion is a candidate for the P0-B retention work
   if authorized).
3. **Do not** recreate FR-1…FR-4 content by guesswork, and do not rewrite
   the historical owner decision text silently.

---

## 5. A3 — M1/M2 historical evidence

### 5.1 Recovered artifacts (original, contemporaneous, integrity-verified)

**M1 independent-review bundle** —
`T/fpe-m1-review/ferrite-fpe-m1-review/` (base HEAD `b188415`, branch `main`,
working-tree-only changes; 2026-11-17 00:25–00:53):

- `report/M1-REPORT.md` — SHA-256 `d720a84b1ffd88d617961ea831ce74729b2f2aff58d6cbe0e0f5ef2db0bb068a`.
  M1 implementation report: 21 new tests; Tier-A 384/0; baseline 2/2;
  **`fpe_selected_streams_are_byte_identical_to_q31` green** (7 fixtures ×
  profiles/rates, per-frame ADTS equality end-to-end); no float kernels;
  four planning tensions documented.
- `REMEDIATION.md` — SHA-256 `81e97292e22bcc9f3fbe5dd7707fa97da75a374aa68a38c3b66369b93b8ef814`.
  **Records two findings from an independent M1 source review** (Finding 1:
  header modification dates; Finding 2: test-count reporting) and their
  remediation, with re-validation (all green). This is retained evidence
  that an independent M1 review **occurred** and was acted upon.
- `README.md` (bundle layout, review gates), `MANIFEST.md`, `SHA256SUMS` —
  **P0-A verified: `shasum -a 256 -c SHA256SUMS` → 64/64 OK, 0 failures.**
- `diff/m1-complete.diff` (git-apply-able against `b188415`), per-file
  diffs, complete new/modified file copies, line-ranged context excerpts
  with provenance headers.
- `validation/pre-change/` (pristine-tree baseline logs),
  `validation/post-change/` (incl. `fresh-verification.log`: byte-identity
  seam test ok; FPE filter 20/20; Tier-A 384/0/2; baseline 2/2; API 6/6;
  doctests 19/19), `remediation-validation.log`.
- `validation/fixture-stability.txt` + `fixtures-run1/` (6 fixtures, 3-run
  byte-stability), `fixture-crosscheck.md` (Python↔Rust sample-exact
  cross-check), `git-status.txt`.
- Working copies: `T/fpe-m1/` (logs + `fixtures-run1/2/3`),
  `T/fpe-m1-pre-baseline-release.log`, `T/fpe-m1-pre-tierA-debug.log`;
  verification mirror `T/fpe-m1-review-verify/`.

**M2 independent-review bundle** —
`T/fpe-m2-review-bundle/ferrite-fpe-m2-review/` (base HEAD `b188415`;
2026-11-17 01:37–01:46):

- `report/m2-implementation-report.md` — SHA-256 `7b6ce78dbe50e499cddbe7c9c54799b2a606e3044651004ce0d432d0b97d4486`.
  **The M2 green-run record (G0 evidence):** FPE **bit-identical to Q31 on
  complete output + all frame-surviving state, per frame, multi-frame**;
  24/24 FPE tests debug **and** release; Tier-A 388 (release) / 392 (debug);
  baseline 2/2; seam end-to-end byte-identity 3/3; no float kernel, no
  round-trip, no rounding.
- `report/m1-context.md`, `00-README.md` (prepared **for independent
  architecture + numerical review**), `MANIFEST.txt`, `SHA256SUMS` —
  **P0-A verified: 77/77 OK, 0 failures.**
- `git/diff-complete.patch` (tracked + new, 37 entries), `git/head-and-branch.txt`,
  status snapshots; complete `source/fpe/**` + `source/modified-rust/**`;
  verbatim `q31-reference/**` extracts; `docs/stage-correspondence.md`.
- `validation/*.log` — **P0-A verified the claimed results against the logs
  themselves:** `fpe-debug.log` 24/24, `fpe-release.log` 24/24,
  `production-baseline.log` 2/2, plus tier-a debug/release, seam, API,
  doctests, full-lib-no-skip (11 loud Tier-B failures, identical blocked
  set), fmt/clippy/licensing/package logs; `tests/list-*.txt`.
- Verification mirror `T/fpe-m2-review-verify/` (identical bundle content).

Classification: **Original** (contemporaneous records of the original
implementation/review cycle, produced at base HEAD `b188415` before the
squash commits). They are **not committed** and live in volatile session
temp; preservation requires owner-authorized bundling into the repository
or a permanent external location (D3). Author/model: the bundles name the
"implementation owner"; the implementing model is not stated in the
artifacts (the G-1 report names Muse Spark 1.3 for G-1; M1/M2 model
attribution requires owner confirmation). Independent review status: see
§5.2.

### 5.2 Review-chain status (A-02) — partially resolved, sign-offs still absent

- **M1:** an independent source review demonstrably occurred (REMEDIATION.md
  records its two findings and their fixes). **The reviewer's report/sign-off
  itself is not in the bundle** and was not found elsewhere. Reviewer
  identity/model: unrecorded in the recovered artifacts.
- **M2:** the bundle was *prepared for* independent architecture + numerical
  review; **no retained record that the M2 review was performed or signed
  off** was found in any searched location.
- Therefore A-02 remains **open** in its essential part: the roadmap-required
  M1 architecture review and M2 architecture+numerical review **sign-offs**
  are unretained. Recovery of the green-run/implementation records (A-01's
  evidence leg) does not substitute for review sign-off.

### 5.3 G0 / A-01 status after recovery

The M2 bit-identity (G0) evidence is now **recovered as original historical
records** (logs + reports + full source snapshots at `b188415` + M1/M2
working tree). It remains **not re-runnable from the published tree** (the
value lane is float since FPE-3.1; the seam test documents that FPE-3.1
replaces the M2 byte-identity gate). A-01's resolution options are unchanged:
retain/link the recovered historical evidence (now possible), and/or the
test-only pass-through comparison mode listed in P0-B B4 (owner-authorized
only). **No conclusion in this report re-establishes G0 on the current
tree.**

### 5.4 Conclusions requiring fresh independent verification

1. **M2 architecture + numerical review sign-off** (no record it occurred):
   either owner produces the record, or a fresh independent review of the
   M2 mirror is required (the recovered bundle makes this cheap: complete
   source + Q31 reference + stage correspondence are retained).
2. **M1 architecture review sign-off**: same treatment (review occurrence is
   evidenced; the sign-off document is not retained).
3. **G0 on the current tree**: only via an authorized test-only pass-through
   mode (P0-B B4), never by reinterpreting the historical evidence.

### 5.5 Provenance observations (no action taken)

- **P0A-O4 (header dates):** M1 REMEDIATION.md records an owner-issued bump
  of five per-file modification-notice dates `2026-10-26 → 2026-11-16`; the
  M2 report recommended restoring `2026-10-26` before commit. The committed
  tree carries `2026-10-26` in all five files (verified by P0-A), so the
  restoration occurred pre-commit; the decision record for the restoration
  is not retained. Licensing is a Phase 0 exclusion — recorded only.
- **P0A-O5 (scratch-table label):** the recovered Qwen checker R5 prints
  `F-20 Q clamp: is_scale=60 (want 9)` — the computed value 60 is the correct
  rounding-formula result for the MAX-clamped `real_scale`; the Q31 arm
  direction-drops that band (no IS scale is coded), so the scratch table's
  `want 9` label is a mislabel in the checker, **not** an evidence
  discrepancy. All owner-reported values checked by R5 reproduce exactly
  (§6.3). Recorded for completeness of the recovered-artifact audit.

---

## 6. Independent verification runs (P0-A, this host, checkpoint tree)

All Cargo commands from `rust/`, toolchain 1.99.1, `aarch64-apple-darwin`,
`--locked`, on the unmodified checkpoint working tree. Logs retained in
`T/p0a-verification/` (outside the repository).

### 6.1 Retained suites (Reproduced)

| Suite | Command | Result |
|---|---|---|
| FPE debug | `cargo test --locked --lib 'fpe::' -- --nocapture` | **125 passed, 0 failed** (3.06 s) |
| FPE release | `cargo test --locked --release --lib 'fpe::'` | **125 passed, 0 failed** |
| Q31 baseline debug | `cargo test --locked --lib production_baseline -- --nocapture` | **2 passed** (hashes unchanged) |
| Q31 baseline release | `cargo test --locked --release --lib production_baseline` | **2 passed** |

Reproduced aggregates (every gate prints identically):
`frames=381 agree=365 findings=30 digest_diffs=0 coding_frames=16 short=12
start=true stop=true unsupported_frames=1 some=242 none=109`;
`gate11: review-gated (P/G/F) findings=14`;
`gate05: supported-domain bands all satisfy |shift| <= B_closed;
unsupported-band frames=1`;
`TIERB_SKIP r21b_tier_b_pinned_c_parity_blocked missing_env=PINNED_C_REF
BLOCKED` (placeholder, **not** a substantive pass).
R1 full corpus: `failures: energy=0 value=0 Ld=0 identity=0 unsupported=0`;
identity checks Q31 426,648 / FPE 213,324 / FPE-Ld 216,093 / decomp 209,533,
all fail=0; `F-6 short-TNS tnsSpecShift!=0 observations: 0`.
G-1: six probes × 14 frames coverage rows reproduced (LONG/START/SHORT/STOP;
quiet probe LONG-only as documented).

**Aggregate-count authority check (for P0-B B2):** each of the five counts
was verified against its authoritative source before being cited here:
30 findings / 16 coding frames / 0 digest diffs / 1 unsupported frame —
reproduced gate output **and** owner-review ledger (§1/§3) **and**
implementation note §7; 14 review-gated — reproduced `gate11` output **and**
owner decision §1/§5 register (5 accepted + 9 conditional = 14). Corpus
definition verified in source: `materials()` (`is_ms.rs:2204-2271`) =
8 fixtures (49+49+49+32+26+32+32+32 = 301 frames) + silence (8) + 6 probes
(12 each = 72) = **15 materials, 381 frames** (the "16 materials" wording in
the owner records is the known A-22 error; 16 is the coding-difference
*frame* count).

### 6.2 Recovered MS-bound scaffold re-run (Reproduced + Derived)

`rustc -O T/r21b_verify/ms_bound_verify.rs` (compiled and run **outside** the
repository; log `T/p0a-verification/ms-bound-verify.log`):

```text
shift-lemma failures: 0
i32-vs-i64 equivalence checks: 10,000,000, wraps: 0, mismatches: 0
boundary table (diff->use_ms, M): consistent=true
random full-width pairs checked: 19,605,922, failures: 0, max |dM|-B: 0
near-boundary pairs (|M|<=4): 3,271,910 checked, failures: 0
branch-crossing pairs: 5,000,000, failures: 0
reduced-domain exhaustive ordered pairs: 531,441, failures: 0
single-operand sweeps: 288, failures: 0
direction-margin semantic/bound failures: 0
digest sim: ALL-with-IS-band occurrences: 0
TOTAL bound-pair checks: 28,409,561, TOTAL failures: 0, min slack B-|dM|: 1
ALL VERIFICATIONS PASSED
```

This independently re-establishes the soundness claim the missing report was
cited for, on a coverage superset of the DeepSeek 500,000-trial check
(which itself remains reported-only, §3.2).

### 6.3 Recovered Qwen arithmetic checker re-run (Reproduced)

`rustc -O T/r21b-review-checks.rs` (outside the repository). All
owner-reported values reproduce **to the unit**: I2 sentinel boundary
(`fmult(0.7,1)=0`, `fmult(0.7,2)=1`, escape at `eL=2`); F-01 `M_q=−6670`,
`M_f=+121350`, shift 128020, `B=128022`, slack 2, use 0→1; F-19 derived
`dEL=738513`, `B=369260`, shift 369256, slack 4; all `is_scale` roundings
(F-19/23/24/25/26); F-20/F-26 direction margins (`−838110935`, `+28934097`,
`−843630081`, `+30152125`); MAX-clamp `1006632960`; energy ratios 1.2499
(0.97 dB) / 1.4991 (1.76 dB) with the corrected ×64 formula (the
owner-literal formula evaluates to 1.003492/1.006346 — known R-4/§6.4
documentation error, values correct). One scratch-table mislabel noted as
P0A-O5; no evidence discrepancy.

### 6.4 Recovered-bundle integrity (Reproduced)

- M1 bundle `shasum -a 256 -c SHA256SUMS`: **64 OK / 0 failed**.
- M2 bundle `shasum -a 256 -c SHA256SUMS`: **77 OK / 0 failed**.
- M2 green-run claims cross-checked against the bundled logs themselves
  (24/24 debug+release; baseline 2/2) — §5.1.
- `is_ms.rs.orig` SHA-1 matches all three review records; committed-file
  diff is visibility-only — §3.3.

---

## 7. Evidence classification register (Phase 0 categories)

| Item | Classification | Notes |
|---|---|---|
| R2.1-B analytical report (original, 2026-11-18) | **Missing** (not Original/Reproduced/anything — absent) | Never committed; acceptance record also absent (§3.1) |
| `B_closed` formula as implemented | **Original** (committed code) + **Derived** (P0-A proof §3.4) + **Reproduced** (28.4M-pair re-run) | Sound on supported domain; unchanged |
| P0-A derivation §3.4 | **Reconstructed + Derived**, independent review pending (P0-C), **not Owner accepted** | Replacement requires D1 authorization |
| `is_ms.rs.orig` backup + hash chain | **Original** + **Derived** diff analysis | §3.3 |
| `ms_bound_verify.rs` / `is_margin_verify.rs` / `final_math_verify.rs` | **Original** scaffolds; ms_bound **Reproduced** by P0-A | Author attribution partly unrecorded (§3.2) |
| `r21b-review-checks.rs` | **Original** (Qwen); **Reproduced** by P0-A | §6.3 |
| `r21b_env_audit*.log`, `r21b_gates.log` | **Original** outputs of removed scaffold | DeepSeek 500k-trial output not among them (§3.2) |
| DeepSeek 500,000-trial `B_closed` result | **Reported-only** (original output missing); claim re-established by §6.2 | Not cited as reproduced-from-original |
| G-1c cleanup record | **Missing** | §4 |
| FR-1…FR-4 definitions | **Missing** | §4.1 |
| Qwen final G-1 acceptance review | **Missing or misattributed** (P0A-O1) | Owner clarification required |
| G-1/G-1a/G-1b retained evidence | **Original** (committed) + **Reproduced** (P0-A rerun) | Acceptance boundary stands on this, not on G-1c |
| M1/M2 implementation reports + green-run logs + bundles | **Original**, recovered, integrity-verified; **not committed** | Preservation requires D3 |
| M1 independent review occurrence | **Original** evidence via REMEDIATION.md; sign-off document **Missing** | §5.2 |
| M2 independent review sign-off | **Missing** (no record it occurred) | Fresh verification required (§5.4) |
| M2/G0 bit-identity on current tree | **Not re-runnable** (historical only) | A-01 unchanged; optional P0-B B4 mode |
| Owner decisions of 2026-11-21 (14 findings) | **Owner accepted** (5) / **Conditional** (9) | Unchanged; no promotion |
| Nine conditional acceptances (F-07/09/10/15/17/18, F-20/22/26) | **Conditional** — conditions unsatisfied | Nothing in P0-A satisfies them |
| Tier-B C parity | **Blocked**, not waived | `TIERB_SKIP … BLOCKED` reproduced |

---

## 8. Unresolved discrepancies and observations

| ID | Item | Status |
|---|---|---|
| OD-4 | Original analytical report missing | Confirmed unrecoverable from available sources; reconstruction proposed (§3.5), **awaiting owner** |
| A-04 | G-1c record + FR-1…FR-4 definitions missing | Confirmed; absence-amendment proposal (§4.4), **awaiting owner** |
| P0A-O1 | Owner decision attributes G-1 review to Qwen; retained review is DeepSeek's; Qwen final G-1 review unretained | **Awaiting owner clarification** (§4.3) |
| A-02 | M1/M2 review sign-offs | M1 review occurrence evidenced; sign-offs absent; M2 review unconfirmed (§5.2) — **awaiting owner** |
| A-01 | G0 not re-runnable | Historical evidence recovered; status unchanged (§5.3) |
| P0A-O3 | Three `.DS_Store` files absent from current tree (predates Phase 0) | Recorded; no action (§1) |
| P0A-O4 | Header-date bump→restoration decision record unretained | Recorded; licensing excluded from Phase 0 (§5.5) |
| P0A-O5 | Qwen scratch checker `want 9` mislabel | Recorded; no evidence impact (§5.5) |
| A-22 | "16 materials" vs 15; frame-cap wording; PE/ATS wording; line cites | Confirmed against source (`materials()` = 15/381); corrections remain **documentation-reconciliation work after P0-C**, owner-approved |

No numerical discrepancy was found anywhere: the bound, the margins, the
aggregates, the corpus definition and the hash chain are mutually consistent
across original records, recovered artifacts, committed code and fresh runs.

---

## 9. Owner decisions required (P0-A gate)

**P0-B must not start until the owner has reviewed this report and given
explicit dispositions.** Required decisions:

- **D1 — `B_closed` derivation (OD-4).** Either (a) produce the original
  analytical report / its permanent location, or (b) authorize the
  provenance-labelled reconstruction (§3.4–3.5) as the accepted replacement
  (committed as a new reconstruction document + append-only amendments to
  referencing records), or (c) leave OD-4 explicitly unresolved with a
  recorded owner disposition. P0-A recommends (b) if (a) is impossible: the
  derivation is independently verified and matches every retained use.
- **D2 — G-1c / FR list / P0A-O1.** Either (a) produce the G-1c record, the
  FR-1…FR-4 source list, and/or the Qwen final G-1 review, or (b) authorize
  the provenance-preserving absence amendment (§4.4) and correct/clarify the
  G-1 review attribution by append-only amendment.
- **D3 — M1/M2 records (A-01/A-02).** Authorize preservation of the recovered
  bundles (commit into the repository under an evidence directory, or
  permanent external link — owner's choice), and either produce the M1/M2
  review sign-offs or authorize fresh independent review (M2 mandatory if no
  record exists; M1 sign-off likewise). Note: committing recovered artifacts
  is a working-tree change reserved for an authorized package (P0-B or a
  separately scoped step); P0-A has copied nothing into the repository.
- **D4 — P0-B authorization.** Confirm scope for P0-B (responsible model:
  Muse Spark 1.3): byte-level R2.1-B baseline (B1), five aggregate assertions
  (B2), retained per-finding IS operands/margins/trace chains + Class B line
  vectors (B3), and explicitly decide which B4 items are authorized (G0
  pass-through mode; reproducible evidence dumps; deterministic ordering;
  unsupported-frame handling). P0-A's verification runs provide the
  pre-change reference results.
- **D5 — Observations.** Note P0A-O3/O4/O5 (no action proposed; licensing and
  `.DS_Store` handling are excluded from Phase 0).

---

## 10. Compliance and git integrity (post-package record)

- Files created by P0-A in the repository: **this report only**
  (`docs/fpe/assessment/phase-0/P0-A-evidence-recovery-report.md`,
  untracked, alongside the five untracked assessment documents).
- No production source, test, fixture, ADR, manifest, lockfile, licensing
  file, header, or acceptance record was modified. No envelope, tolerance,
  decision definition, `B_closed` behaviour or MS domain was changed. No
  conditional acceptance was promoted. Tier-B was not waived. No perceptual
  claim is made.
- No commit, push, stage, amend, rebase, reset, cherry-pick, stash or clean.
  Untracked files (including the five assessment documents) left untouched.
  The two pre-FPE worktrees were not touched.
- All Cargo invocations were read-only test executions (`--locked`); the
  recovered scratch verifiers were compiled and run **outside** the
  repository; verification logs live in `T/p0a-verification/`.
- Post-package state (exact, verified after writing this report):
  `git rev-parse HEAD` → `eb2896923ec788727975268aca0beb27cba51760`
  (unchanged); `git diff --stat` → empty; `git diff --cached --stat` →
  empty; `git diff --check` → clean. `git status --porcelain -uall`:

  ```text
  ?? docs/fpe/assessment/findings-register.md
  ?? docs/fpe/assessment/implementation-roadmap.md
  ?? docs/fpe/assessment/next-phase-recommendation.md
  ?? docs/fpe/assessment/phase-0/P0-A-evidence-recovery-report.md
  ?? docs/fpe/assessment/post-checkpoint-assessment.md
  ?? docs/fpe/assessment/validation-roadmap.md
  ```

  No unexpected changes: the only delta vs the pre-package record (§1) is
  this report.

**STOP — P0-A complete; awaiting owner review and decisions D1–D5 before
P0-B.**