# Phase 0 — P0-C Independent Audit of P0-A Evidence Recovery and P0-B Reproducibility Hardening

**Package:** P0-C (independent technical, numerical and evidence-integrity audit)
**Auditor:** Qwen4 Max Preview (OpenCode), acting as independent auditor — not the
P0-A author side nor the P0-B implementer (Muse Spark 1.3).
**Date:** 21 November 2026
**Repository:** `whepper/ferrite`, branch `main`
**Checkpoint / HEAD audited:** `eb2896923ec788727975268aca0beb27cba51760`
(= `origin/main`, 0 ahead / 0 behind)
**Authority:** Project owner explicitly authorized P0-C (task instruction,
21 November 2026). This audit verifies; it does not implement, repair, promote,
waive, commit or push. The only repository addition is this report.

**Classification vocabulary (per finding):** PASS / PASS WITH FINDINGS / FAIL /
NOT VERIFIED.

---

## 1. Executive assessment

Phase 0's P0-A and P0-B work is **technically sound, correctly scoped, and
accurately represented**. I independently verified — not merely reviewed — the
following:

- The `B_closed` reconstruction is **mathematically sound**. I re-derived and
  exhaustively checked every lemma and rewrite identity with a from-scratch
  checker (5,308,416 exhaustive operand pairs + 300,000 random full-width i32
  pairs, zero failures), re-ran the recovered original 28,409,561-pair scaffold
  (zero failures, exact reproduction), and re-ran the Qwen arithmetic checker
  (all owner-reported values reproduce to the unit). The reconstructed bound is
  **identical** to the implemented formula. Provenance labelling is correct: the
  original analytical report was **not** recovered and is not claimed recovered.
- The G-1c absence amendment and attribution correction are **accurate**. G-1b is
  the last recovered reconciliation; I verified every G1b correction site against
  the committed state. G-1c, FR-1…FR-4 and the Qwen final acceptance review remain
  explicitly unrecovered; no conclusions were invented.
- M1/M2 provenance is **reliable**: 64/64 and 77/77 SHA-256 checks pass, the
  preserved bundles are byte-identical to the recovered originals, M1 review
  occurrence is evidenced (REMEDIATION.md) and correctly distinguished from the
  unconfirmed M2 review occurrence; the fresh M2 review is authorized but not
  misrepresented as performed; G0 remains historical and non-rerunnable.
- The P0-B baseline is **deterministic** (verified across separate process runs),
  **test-only**, and follows the `production_baseline.rs` pattern. All five
  aggregate assertions are **real test gates** with correct values and semantics.
  Per-finding evidence is complete and integrity-pinned; unsupported-frame
  handling is explicit and correct.
- **No production numerical behaviour changed.** Q31 baselines are unchanged
  (2/2 debug and release); the only code change is the `#[cfg(test)]`-gated
  `is_ms.rs`; Tier-A grew by exactly the 5 new tests; the full-lib Tier-B failure
  set is identical to the established blocked set.
- **Repository integrity is preserved.** HEAD = checkpoint = `origin/main`; no
  commits, pushes, stages, or history operations; P0-A artifacts intact.
- Clippy is unchanged at **185 lib-test warnings with zero `is_ms` attribution**;
  no new warnings were introduced by P0-B.

**No blocking defects were found.** Seven non-blocking findings are recorded
(§9), chiefly documentation-precision items, an evidence-emission nuance, and
pending owner closure actions (commit authorization, temp-artifact preservation,
documentation reconciliation, fresh M2 review).

**Overall recommendation: PASS WITH FINDINGS — OWNER DISPOSITION REQUIRED** (§11).
Phase 1 (R2.2) is technically ready to begin once the owner disposes the findings
and authorizes scope (§12).

---

## 2. Audit methodology

Independence was maintained by re-executing and re-deriving rather than trusting
the implementation reports:

1. **Full source read** of all five assessment documents, the P0-A report, the
   owner disposition record (incl. amendment D4), the P0-B report, the phase-0
   README, the reconstruction, the absence amendment, the M1/M2 provenance
   record, the amended owner decision, and the relevant historical records
   (owner review/decision, G-1/G-1a/G-1b chain, DeepSeek G-1 review).
2. **Git forensics:** branch/HEAD/remote parity, `git diff`/`--cached`/`--numstat`,
   full untracked inventory, HEAD and `origin/main` reflogs, stash list,
   `git worktree list`, `git fsck --dangling`, pickaxe `-S'G-1c'`, file mtimes.
3. **Actual diff inspection** of the complete `is_ms.rs` change (1312-line diff)
   and the append-only owner-decision amendment; line-by-line accounting of all
   9 deleted lines.
4. **Independent mathematical verification:** a from-scratch Python checker
   (written by the auditor, not derived from the recovered scaffold) validating
   L1–L3, both rewrite identities, and the bound on exhaustive + random pairs;
   plus a hand re-derivation of the four proof steps.
5. **Re-runs of recovered original scaffolds** (`ms_bound_verify.rs`,
   `r21b-review-checks.rs`) compiled with `rustc -O` **outside the repository**
   in the approved session-temp area; SHA-256 pin verification of all eight
   recovered artifacts (R1–R8).
6. **Bundle integrity:** `shasum -a 256 -c SHA256SUMS` in both preserved bundles,
   supplemental-pin verification, and `diff -rq` against the original temp copies.
7. **Independent test execution** of every required suite (§7), plus full-lib
   no-skip to verify the Tier-B failure set, and a cross-process determinism check.
8. **Clippy** re-run with per-file attribution analysis.
9. **Independent wiki spot-check** (Outline `list_documents`, incl. archived) for
   the missing artifacts, confirming P0-A's search result.

All Cargo commands ran from `rust/` with `--locked`, toolchain rustc/cargo
**1.99.1** (`58cf8f9c3` / `5f94df478`), `aarch64-apple-darwin` — the recorded
reference platform. Audit logs and scratch checkers live in session temp
`…/T/opencode/p0c-audit/` (outside the repository). No repository content was
modified, cleaned, staged, committed or pushed; no test or implementation was
altered to make anything pass.

---

## 3. Repository integrity — PASS

| Check | Result |
|---|---|
| Branch | `main` |
| `git rev-parse HEAD` | `eb2896923ec788727975268aca0beb27cba51760` — **identical to expected baseline** |
| `git rev-parse origin/main` | `eb2896923ec788727975268aca0beb27cba51760` (0 ahead/behind) |
| HEAD reflog | last entry = the baseline commit `eb28969` (2026-11-21 14:46:51 +0100); **no commits after it** |
| `origin/main` reflog | last update = push of `eb28969`; **no pushes during P0-B/P0-C** |
| Staged changes (`git diff --cached`) | **none** |
| `git stash list` | empty |
| `git diff --check` | clean |
| Manifest/lockfile/dependency diff (`rust/Cargo.toml`, `rust/Cargo.lock`, `rust/convert/Cargo.toml`) | **empty** |
| Worktrees | two pre-FPE detached worktrees (`02158ff`, `1c825c6`), untouched (match P0-A §1) |
| `.DS_Store` | none present (confirms P0A-O3 baseline observation) |

**Exact tracked modifications (2):**

| File | Change | Nature |
|---|---|---|
| `docs/fpe/R2.1-B-owner-decision.md` | **+46 / −0** | Append-only amendment section (D1–D3 pointers); historical text byte-unchanged. Pre-existing P0-A output, not P0-B. |
| `rust/src/aac_enc/fpe/tests/is_ms.rs` | **+1230 / −9** | P0-B test-only implementation. `#[cfg(test)]`-gated (`fpe/mod.rs:44-45`). |

**Exact untracked files (all authorized P0-A/P0-B outputs):**

- `docs/fpe/assessment/{post-checkpoint-assessment,findings-register,validation-roadmap,implementation-roadmap,next-phase-recommendation}.md` (5 assessment docs)
- `docs/fpe/assessment/phase-0/{README,P0-A-evidence-recovery-report,P0-A-owner-disposition-record,P0-B-reproducibility-report}.md`
- `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md`
- `docs/fpe/R2.1-B-G1c-absence-amendment.md`
- `docs/evidence/fpe-m1-m2-provenance.md`
- `docs/evidence/fpe-m1/ferrite-fpe-m1-review/` (66 files)
- `docs/evidence/fpe-m2/ferrite-fpe-m2-review/` (78 files)

**Scope compliance:** no production source outside `fpe/tests/` was touched; no
manifest/lockfile/dependency/API/licensing/ADR change; no change outside the
authorized P0-A/P0-B scope. **P0-A artifacts were not modified or deleted:** the
preserved bundles are byte-identical to the originals and pass all checksums
(§5.3); the reconstruction/amendment/provenance documents match their D1–D3
requirements; file mtimes form a consistent chronology (assessment docs →
P0-A report → D1–D3 artifacts → disposition record incl. D4 → `is_ms.rs` →
P0-B report), with every P0-A artifact predating the P0-B implementation.

**Line-count note:** the P0-B report states "+1239 / −9"; `git diff --numstat`
reports **+1230 / −9** (1239 is the total *changed*-line count, insertions +
deletions). See P0C-N2.

---

## 4. P0-A findings

### 4.1 B_closed reconstruction (D1 / OD-4) — PASS

**Recovery status verified.** The original "accepted FPE R2.1-B analytical report
(2026-11-18)" is **not recovered**. Independent confirmation: pickaxe
`git log --all -S'G-1c'` matches only `eb28969` (the owner decision); no G-1c or
analytical-report file exists in `docs/fpe/`; my Outline wiki spot-check
(`R2.1-B`, `G-1c`, `FPE analytical report`, `Ferrite FPE`, incl. archived)
returned **zero** FPE documents — the single `B_closed` hit is the unrelated MPAK
container-design spike, exactly as P0-A §2 reported. **This audit does not claim
the original report was recovered.**

**Provenance labelling verified.** The reconstruction document
(`R2.1-B-Bclosed-derivation-reconstruction.md`) prominently states the original
was not recovered, labels itself **Reconstructed + Derived + Owner accepted
(replacement, D1)**, claims no historical provenance, and records that historical
references are amended not rewritten. The owner-decision amendment is append-only
and supersedes the §6 "Missing" status without altering historical text. The
reconstruction is correctly distinguished from original historical evidence.

**Mathematical soundness and formula identity:** verified independently — see §6.
The reconstructed bound is identical to the implemented `b_closed`
(`is_ms.rs:573-578`), to the recovered scaffold, and to every owner-review/Qwen/
DeepSeek calculation. The 28,409,561-pair verification supports the stated
soundness conclusion (§6.2). The `+3` constant is exactly accounted for (1 parity
+ 2 ceil-grouping); the doc-comment's loose "strict-`>` unit plus halving floors"
attribution is correctly flagged as harmless (the `−1` cancels in the delta).

**Hash chain verified.** Committed `is_ms.rs` SHA-1
`421df1a6609bf0785a8722b080830791daad914c`; recovered backup SHA-1
`16459f94d7ab1ec86c3b01fa82e0829cf85b2b8a` (matches owner review / Qwen §3.1 /
DeepSeek §7.2). I diffed backup vs committed: the **only** changes are 9 lines
making `probe_sine`/`probe_narrow` `pub(super)` with G-1-reuse doc comments —
**no numerical/decision/margin/bound/corpus change**. The owner-review evidence
(generated against `16459f94…`) applies unchanged to the committed harness.

### 4.2 G-1c absence amendment (D2 / A-04 / P0A-O1) — PASS

- **G-1b is the last recovered reconciliation** — verified. No G-1c document
  exists; pickaxe confirms it was never committed. The G-1b report explicitly
  leaves the analytical report outstanding and states "Qwen final review and
  overall G-1 acceptance are not performed or declared here."
- **Full G-1b-vs-committed reconciliation (assigned to P0-C by the amendment):
  performed.** Every correction site matches the committed "After" state:
  - G1b-1 (§11 direction): committed G-1 report lines 314–332 read
    `Q31→FPE 58→59` (ch0) / `4→5` (ch1), F-23 `13→14`. ✓
  - G1b-2 (§13 counts): G-1 = 123 (112+11), post-G-1a = 125 (112+13);
    `is_ms` 31 = 30 substantive + 1 Tier-B placeholder; `probe_envelope` 11→13;
    cargo-passed vs substantive split present. ✓
  - G1b-3 (finding count): G-1a note §8 reads "All 28 G-1 findings (F-03…F-30)…
    F-01/F-02 remain outside this G-1 adjudication scope." ✓
  - G1b-4 (§4 grouped wording): "sums of the per-window envelope terms…" present. ✓
  - R-3 (§7 utilisation-tie wording), R-4 (§6 "arbitrary tie-break"), R-6 (no
    stale 1628/714 line-count cite): all present/absent as required. ✓
- **Attribution correction verified.** The retained independent G-1 numerical
  review is **DeepSeek's** (`R2.1-B-G1-numerical-review-deepseek.md`: "Reviewer:
  independent numerical reviewer (DeepSeek)", verdict **PASS WITH FINDINGS**,
  §H "G-1 is ready for Qwen's final independent acceptance review"). The owner
  decision §2 item 4's "Independent G-1 review by Qwen" is the historical
  misattribution; the amendment corrects it append-only. The **Qwen final
  acceptance review remains missing**; the amendment infers neither whether it
  was performed nor any of its conclusions.
- **G-1c, FR-1…FR-4 remain explicitly unrecovered; no definitions invented; no
  missing conclusions inferred.** FR-5/FR-6 remain open as documented (FR-6 now
  addressed by P0-B B1 in the working tree, commit pending — see P0C-N7). The
  reproducible-evidence vs incomplete-historical-chain distinction is preserved.

### 4.3 M1/M2 evidence (D3 / A-01 / A-02) — PASS

**Bundle integrity (independently re-verified at the preserved location):**

| Bundle | `shasum -a 256 -c` | Files | Copy fidelity |
|---|---|---|---|
| M1 | **64/64 OK, 0 failures** | 66 (64 + SHA256SUMS + MANIFEST.md) | `diff -rq` vs original temp: **byte-identical** |
| M2 | **77/77 OK, 0 failures** | 78 (77 + SHA256SUMS) | `diff -rq` vs original temp: **byte-identical** |

Supplemental pins match the provenance record exactly:
M1 MANIFEST.md `549ad37d…`, M1 SHA256SUMS `89332c96…`, M2 SHA256SUMS `785ea4b1…`.
Key report pins match: M1-REPORT.md `d720a84b…`, REMEDIATION.md `81e97292…`,
m2-implementation-report.md `7b6ce78d…`.

**M2 green-run claims cross-checked against the bundled logs themselves:**
`fpe-debug.log` 24/24, `fpe-release.log` 24/24, `production-baseline.log` 2/2,
`tier-a-debug.log` 392 passed/2 ignored, `tier-a-release.log` 388 passed/6 ignored
— all match the report's claims.

**Review-occurrence distinction verified:**
- **M1 review occurrence: CONFIRMED.** REMEDIATION.md records two findings from an
  independent M1 source review (header dates; test-count reporting) and their
  remediation with re-validation. The reviewer's sign-off document is **not
  retained**; reviewer identity unrecorded. Occurrence ≠ sign-off — correctly
  distinguished (A-02 residual).
- **M2 review occurrence: UNCONFIRMED.** `00-README.md` says "Prepared for
  independent architecture + numerical review"; no record it was performed or
  signed off exists.
- **Fresh independent M2 review + sign-off: AUTHORIZED (D3), not performed.** No
  document misrepresents it as completed; no sign-off manufactured or backdated.
- **G0 remains historical and non-rerunnable.** The seam test
  (`stream_driver.rs:8222`) documents "M3 (FPE-3.1) replaces the M2 byte-identity
  gate"; no pass-through mode exists (§5.6). A-01 remains open. Correct.

**Recovered R2.1-B artifact pins (R1–R8): all eight SHA-256 digests verified
intact** in session temp at audit time (is_ms.rs.orig, ms_bound_verify.rs,
is_margin_verify.rs, final_math_verify.rs, r21b-review-checks.rs,
r21b_env_audit.log, r21b_env_audit2.log, r21b_gates.log). Their durable
preservation was not authorized by D3 and remains a flagged follow-up (P0C-N5).

---

## 5. P0-B implementation findings

All P0-B code is in `rust/src/aac_enc/fpe/tests/is_ms.rs`, compiled test-only
(`#[cfg(test)] mod tests;` at `fpe/mod.rs:44-45`). The complete diff was inspected
line-by-line. It adds: `FindingEvidence` schema + canonical serialization,
`build_frame_evidences` recorder, `sorted_evidences` ordering, an FNV-1a helper,
and 5 gate tests; and plumbs `material_idx` + an `evidences` field through
`analyze_frame`/`FrameAnalysis`/`run_corpus`/`CorpusResult`. **No decision, bound,
envelope, routing, or corpus predicate was altered.**

### 5.1 B1 — byte-level baseline — PASS

Test `r21b_p0b_byte_baseline_is_pinned`:
- **Corpus scope:** exactly the 15 `materials()` (8 fixtures 49/49/49/32/26/32/32/32
  = 301 frames + silence 8 + 6 probes × 12 = 72 → **381 frames**), each stepped
  through the production `StreamEncoder::step_frame` path on **both** engines via
  `R21BHarness`, with per-frame `assert_adts_valid` structural validation.
- **Canonical serialization / stable hashing:** per-material Q31/FPE ADTS byte
  length + FNV-1a 64 (identical offset basis `0xcbf29ce484222325` and prime
  `0x100000001b3` as `production_baseline.rs`), plus a canonical-evidence digest
  (FNV-1a 64 over sorted canonical strings).
- **Expected-value pinning:** all 15 rows hard-coded (sr, frames, Q31/FPE length +
  hash); total frames 381 and material count 15 asserted; unexpected material
  panics. The pinned table matches the P0-B report §3 exactly.
- **Platform convention:** lengths/frames pinned everywhere; exact hashes enforced
  on `cfg!(all(target_os = "macos", target_arch = "aarch64"))`, informational
  `eprintln!` elsewhere — faithfully mirroring `production_baseline.rs`.
- **Failure behaviour:** any drift in length/hash/frame-count fails loudly via
  `assert_eq!`. **No automatic regeneration:** expectations are constants; no code
  path writes them during normal tests.
- **Cross-check:** the Q31 hashes for sine/onset/stepdown-stereo match the existing
  V3 production baseline pins byte-for-byte (3147/`0x8921…`, 2359/`0x2a6c…`,
  2236/`0xc5e3…`) — independent evidence the Q31 arm is unchanged.
- Evidence digest pinned `0x0544_1ee2_9087_55aa` (30 rows), reproduced (§7).

### 5.2 B2 — aggregate assertions — PASS

Test `r21b_p0b_aggregates_are_pinned` uses **real `assert_eq!` test gates** (not
informational output):

| Metric | Pinned | Authoritative source cross-check |
|---|---:|---|
| Findings | **30** | reproduced gate output + owner-review ledger §1/§3 (F-01…F-30) + impl note §7 ✓ |
| Coding frames | **16** | reproduced gate output + owner-review §3 frame table (16 frames; quiet f16 carries two findings) ✓ |
| Digest differences | **0** | reproduced gate output; digest equality ≠ per-band equality (owner-review §3) ✓ |
| Review-gated findings | **14** | reproduced `gate11` + owner decision §1/§5 (5 accepted + 9 conditional = 14) ✓ |
| Unsupported frames | **1** | reproduced gate output + owner-review §4 (quiet f16 b13 sentinel domain) ✓ |

Also pinned: frames 381, agree 365, and one-evidence-record-per-finding.
**Semantics verified:** `review_gated = consequence >= Consequence::P` with the
enum's derived `Ord` `N < L < P < G < F` → P/G/F (matches gate11). Owner-review §3
composition confirmed: 30 = 29 per-band coding findings + 1 MS-margin finding
(F-01); F-02 shares quiet f16. The "16 materials" wording in owner records is the
known A-22 error (corpus defines **15**; 16 is the coding-frame count) — verified
against `materials()`.

### 5.3 B3 — per-finding evidence — PASS WITH FINDINGS

`FindingEvidence` schema, `build_frame_evidences` recorder, `sorted_evidences`
ordering, and the completeness test were inspected:
- **Schema:** identity (material, material_idx, frame, band, cause/status/
  consequence, detail, review_gated, unsupported); IS operands both arms
  (e_l/e_r, e_ld_l/r, t_l/r, real_scale, is_scale, is_book, mid-mask, pre-mask);
  stateless margins (I1 via production `sfb_correlation`, I2 dual, I3, I8 dual,
  I10 dual with `wrapping_sub`); MS operands (ms_supported, M_q/M_f, B_closed,
  use_q/use_f, six absolute Ld deltas); full frame trace chains both arms; Class B
  line vectors (Q31 spectral lines `[spec_off..spec_end)` both channels both arms).
- **Recorder:** reads `analyze_frame` locals **after** the replay-equality gates
  (`validate_lockstep_replay`/`validate_ms_replay`, lines 2490–2491) — it records
  production-validated state, never feeds a predicate. Digest findings (band −1)
  record explicit `None`s, never invented band-0 values. Spans are bounds-checked;
  empty windows recorded as empty vectors.
- **Ordering:** `sorted_evidences` sorts by `(material_idx, frame, band, detail)` —
  corpus order, never hash-map/filesystem order.
- **Completeness test:** asserts for all 30 rows identity, flag consistency, IS
  operands + stateless margins + both traces + MS operands/bound/use + line
  vectors present (band ≥ 0), exact spec spans, canonical self-identity; prints
  `R21B-FINDING …` rows in sorted order. All 30 are per-band (0 digest diffs).
- **Traceability:** an independent reviewer can trace findings to inputs/decisions
  via the printed identity/detail rows (detail carries MS margins/bound/use and
  before/after coding state), the retained recorder (showing exactly which
  production values feed each field), the replay-equality gates (recorded values
  equal production), and the digest pin (content stability). I demonstrated this
  for F-01/F-19 via the Qwen-checker re-run (§6.3). Stateful margins (I4/I6/I7,
  I5 carry) travel in the retained trace events (`Extend`/`Reject` carry i3+i4,
  `DirectionDrop`/`CarryUpdate` carry i5, `BlockClear` carries i6+i7) — verified
  structurally; not re-derived out of context (correct).

**Findings (non-blocking):** the **full canonical evidence rows are recomputed,
completeness-asserted and digest-pinned but not emitted** by any retained test
(only identity/flag/detail rows print). The retention policy
(validation-roadmap §6) cites the G-1 `BandRow` *emission* pattern as the model;
P0-B satisfies the underlying intent (reproducible, complete, integrity-pinned
evidence) but not literal full-row emission. Also, the ordering assertion checks
(material, frame, band) monotonicity but not the `detail` tie-break (the sort
itself orders by detail and the digest pins the result). See P0C-N3. The band-−1
digest path is documented but unexercised (0 digest diffs) — a disclosed
limitation, not a gap in the 30.

### 5.4 B4 — unsupported frames — PASS

Test `r21b_p0b_unsupported_frame_is_explicit`:
- Exactly **one** unsupported finding (F-01): `quiet-sine-stereo:f16:b13`,
  `DomainUnsupported`, `ms_supported = Some(false)` — excluded from `B_closed`
  validation by the unchanged sentinel-domain rule.
- The same frame carries F-02 (`OutOfEnvelope`): **one frame, two findings** —
  counted **once** in `unsupported_band_frames` (the `run_corpus` loop breaks after
  the first DomainUnsupported finding) but **twice** in the finding population.
  Both halves pinned (`frame_peers == 2`, `unsupported_band_frames == 1`).
- **Never a pass, never silently omitted:** `route()` rejects unroutable states;
  `ms_band_supported` excludes sentinel/IS bands from validation; the finding is
  printed `R21B-UNSUPPORTED … ms_supported=0`. No sentinel value invented.
- Reproduced: `R21B-UNSUPPORTED quiet-sine-stereo:f16:b13 status=DomainUnsupported
  ms_supported=0 peers_in_frame=2`. ✓

### 5.5 B5 — determinism — PASS

Independently assessed (not accepted from the report):
- **Collection ordering:** `run_corpus` iterates materials × frames × in-analysis
  band order (deterministic); `sorted_evidences` makes dump/hash order explicit.
  The only `HashMap`s (`persist`, `div_first`) affect counters/minimums via
  `.get()`/`.values().min()` — never output order; the `HashSet` at line 3008 only
  increments per-band counters (commutative). No filesystem iteration participates.
- **Serialization stability:** `canonical_string` uses fixed field order, decimal
  integers, bracketed decimal vectors, `|`-joined `Debug` traces, `n/a` for absent
  optionals. **No f64 enters the canonical form** — verified: all four `detail`
  format sites use integers/enum-`Debug` only; spectra are i32 Q31 lines; trace
  events carry integer/enum fields. No map iteration, no pointer formatting.
- **Stable identifiers / optional values:** `stable_id = {material}:f{frame}:b{band}`;
  `None` → `n/a` (explicit, never invented). Note: stable_id is not unique for
  F-01/F-02 (same band/frame) — disambiguated by detail + sort position as
  documented (P0C-N4).
- **Repeated execution:** the in-test determinism gate re-runs the full corpus twice
  (bypassing the `OnceLock` cache) and asserts byte-identical canonical strings.
  **I additionally verified cross-process determinism:** two separate
  `cargo test … r21b_p0b_byte_baseline_is_pinned -- --nocapture` runs produced
  **byte-identical** `R21B-BASELINE` rows and `R21B-EVIDENCE-DIGEST` (`diff` clean).

### 5.6 B6 — scope compliance — PASS

- **G0 pass-through comparison mode NOT implemented:** search
  `pass_through|pass-through|passthrough` in `is_ms.rs` → **no hits**. G0 remains
  historical/non-rerunnable; A-01 open.
- **No production numerical or decision behaviour changed:** only the test file +
  docs changed; `#[cfg(test)]` gate; Q31 baseline 2/2 unchanged; Tier-A +5 exactly.
- **No conditional acceptance promoted:** the nine conditionals remain conditional
  in all records; no acceptance criterion changed.
- **No unrelated refactoring:** the diff is evidence plumbing + 5 tests + FNV
  helper. The 9 deleted lines are fully accounted for: 2 evidence-plumbing lines
  (`for m in materials()` → enumerate; `analyze_frame` call gaining `material_idx`)
  + **7 comment lines** in the Tier-B placeholder (behavior unchanged — see
  P0C-N2). No manifest/lockfile/dependency/API/licensing/ADR change; no R2.2/M3/M4/
  G-2/G-3/float-kernel work begun.

---

## 6. Mathematical verification (independent)

### 6.1 Hand re-derivation + from-scratch exhaustive checker

I re-derived the four proof steps and confirmed the logic:
- **Step 1:** `g(eld,tld) = (tld>>1) − (max(eld,tld)>>1) = min(0,(tld>>1)−(eld>>1))`
  by case split; `min(0,·)` 1-Lipschitz + L1 → `|Δpnlr| ≤ Σ ceil(d/2)`. ✓
- **Step 2:** `min_tld = 2(min_tld>>1)+r`, `r∈{0,1}`;
  `pnms = r − max(0,(em>>1)−(mt>>1)) − max(0,(es>>1)−(mt>>1))`;
  `|Δpnms| ≤ 1 + ceil(dEm/2)+ceil(dEs/2)+2·ceil(T/2)`, `T≤max(dTL,dTR)` (L2). ✓
- **Step 3:** the `−1` cancels in `ΔM`; L3 + `ceil(x/2)≤x` + `2ceil(T/2)≤T+1` →
  `|ΔM| ≤ ceil(S/2)+dTL+dTR+T+4`. ✓
- **Step 4:** `T=0` gives `+3` exactly (2ceil(0)=0 saves 1); `T≥1`: `T+4≤2T+3`;
  with `T≤max(dTL,dTR)` → `B_closed`. The `+3` = 1 (parity) + 2 (L3 grouping). ✓
- **Robustness corollary:** `|M_q|>B ⟹ use_f=use_q` (M_q≥B+1 ⟹ M_f≥1; symmetric). ✓

A from-scratch Python checker (auditor-written, independent of the scaffold;
Python `>>` is arithmetic/floor shift matching Rust) confirmed:

| Check | Domain | Failures |
|---|---|---:|
| L1 shift lemma | [−64,64]² exhaustive + 500,000 random full-width i32 | 0 |
| L2 min/max Lipschitz | [−8,8]⁴ exhaustive (×2 for min/max) | 0 |
| L3 ceil grouping | [0,12]⁴ exhaustive | 0 |
| I1 g-identity | [−32,32]² exhaustive | 0 |
| I2 pnms-rewrite identity | [−16,16]³ exhaustive | 0 |
| **B_closed bound** | **5,308,416 exhaustive ordered pairs** (odd-heavy landmark grid) | **0** (min slack 1, no tight pair) |
| B_closed bound | 300,000 random full-width i32 pairs | 0 |

### 6.2 Recovered original scaffold re-run (Reproduced)

`rustc -O …/r21b_verify/ms_bound_verify.rs` (SHA-256 `0922e0d6…` verified), run
outside the repository. **Exact reproduction of P0-A §6.2:**

```text
shift-lemma failures: 0
i32-vs-i64 equivalence checks: 10000000, wraps: 0, mismatches: 0
boundary table (diff->use_ms, M): consistent=true
random full-width pairs checked: 19605922, failures: 0, max |dM|-B: 0
near-boundary pairs (|M|<=4): 3271910, failures: 0
branch-crossing pairs: 5000000, failures: 0
reduced-domain exhaustive ordered pairs: 531441, failures: 0
single-operand sweeps: 288, failures: 0
direction-margin semantic/bound failures: 0
TOTAL bound-pair checks: 28409561, TOTAL failures: 0, min slack B-|dM|: 1
ALL VERIFICATIONS PASSED
```

Coverage arithmetic verified: 19,605,922 + 3,271,910 + 5,000,000 + 531,441 + 288
= **28,409,561**. The scaffold's `margins()` matches the harness
`ms_first_pass_margins` (shift-before-subtract i64) and the documented production
form; its equivalence to production is gated by the retained replay-equality
gates 1–2 (re-run green, §7) and the scaffold's own 10M-sample i32-vs-i64 check
(zero wraps). The sampling domain (`eld/em/es ∈ [MIN,0]`, `tld ∈ [−33·2^25,0]`)
is an arithmetic superset of the supported domain. **The 28.4M-pair verification
supports the stated soundness conclusion.** Minimum slack 1 (no pair attains B)
is correctly noted; no tightening proposed (Phase 0 exclusion). The DeepSeek
500,000-trial result remains reported-only (original output not recovered) and is
re-established on a coverage superset — correctly represented.

### 6.3 Qwen arithmetic checker re-run (Reproduced)

`rustc -O …/r21b-review-checks.rs` (SHA-256 `20cd68a4…` verified). **All
owner-reported values reproduce to the unit:** I2 sentinel boundary
(`fmult(0.7,1)=0`, `fmult(0.7,2)=1`); F-01 `M_q=−6670`, `M_f=+121350`, shift
128020, `B=128022`, slack 2, use 0→1; F-19 `dEL=738513`, `B=369260`, shift 369256,
slack 4; all `is_scale` roundings (F-19/23/24/25/26); direction margins
(`−838110935`, `+28934097`, `−843630081`, `+30152125`); MAX-clamp `1006632960`;
energy ratios 1.2499 (0.97 dB) / 1.4991 (1.76 dB) with the corrected ×64 formula.
Known documentation-only residuals confirmed unchanged: the owner-literal ratio
formula evaluates to 1.003492/1.006346 (R-4); the scratch-table `want 9` mislabel
(P0A-O5 — computed 60 is the correct MAX-clamp formula result; Q31 direction-drops
that band). Neither affects any verified claim.

**Conclusion:** the reconstruction is mathematically sound on its supported
domain, provenance-correct, and identical to the implemented formula.

---

## 7. Independent test execution — PASS

All from `rust/`, `--locked`, toolchain 1.99.1, `aarch64-apple-darwin`. Exact
commands and outcomes (logs in `…/T/opencode/p0c-audit/`):

| # | Command | Result |
|---|---|---|
| 1 | `cargo test --locked --lib 'fpe::' -- --nocapture` | **130 passed, 0 failed** (125 pre-existing + 5 new), 385 filtered |
| 2 | `cargo test --locked --release --lib 'fpe::'` | **130 passed, 0 failed** |
| 3 | `cargo test --locked --lib production_baseline -- --nocapture` | **2 passed, 0 failed** (Q31 hashes unchanged) |
| 4 | `cargo test --locked --release --lib production_baseline` | **2 passed, 0 failed** |
| 5 | `cargo test --locked --lib -- --skip …` (15 CI Tier-A skips, ci.yml set) | **498 passed, 0 failed**, 2 ignored, 15 filtered |
| 6 | `cargo test --locked --test production_api` | **6 passed, 0 failed** |
| 7 | `cargo test --locked --doc -p ferrite-aac` | **19 passed, 0 failed** |
| 8 | `cargo fmt -- --check` | clean (exit 0) |
| 9 | `git diff --check` | clean (exit 0) |
| 10 | `cargo test --locked --lib` (full, no skips) | **502 passed, 11 failed, 2 ignored** |
| 11 | `cargo test --locked --lib r21b_p0b_byte_baseline_is_pinned -- --nocapture` ×2 | both pass; **cross-process output byte-identical** |
| 12 | `bash tools/check-licensing.sh` | OK (86 rust/src files carry TPMV notice; converter boundary intact) |

**Reproduced gate output (identical to P0-A §6.1 and the authoritative records):**
`frames=381 agree=365 findings=30 digest_diffs=0 coding_frames=16 short=12
start=true stop=true unsupported_frames=1 some=242 none=109`;
`gate11: review-gated (P/G/F) findings=14`;
`gate05: supported-domain bands all satisfy |shift| <= B_closed;
unsupported-band frames=1`; `R21B-EVIDENCE-DIGEST rows=30
fnv1a64=0x05441ee2908755aa`; `R21B-P0B determinism: 30 evidence rows identical`;
30 `R21B-FINDING` rows; `R21B-UNSUPPORTED quiet-sine-stereo:f16:b13 …
peers_in_frame=2`. Test breakdown: `is_ms` **36** (31 + 5 new), `probe_envelope`
**13**, **56** `G1-BAND` rows. The 2 Tier-A ignored are opt-in benchmarks
(`neon_kernel_microbench`, `aac_perf_bench`).

**Tier-B failure set (full-lib no-skip): identical to the established blocked
set — not waived, not reclassified.** The 11 failures are exactly the
artifact-gated Tier-B tests, each failing loudly on a missing artifact:
`m5a_decode_check` (AAC_DEC_BIN NotPresent), `m55_streams_match_c_reference`,
`phase1_frontend_agreement_sine_f5`, `phase1_full_stream_parity`,
`phase1e_onset_f24_bands_causality`, `phase1e_sine_f5_band16_residue`,
`phase1f_multitone_f1_no_live_divergence`, `phase1f_stepdown_f1_no_live_divergence`,
`phase1g_long_flips_residue`, `phase1i_multitone_f5_tier_b`,
`phase1i_sine44_f5_tier_b` (reference validation artifact missing). The Tier-B
placeholder `r21b_tier_b_pinned_c_parity_blocked` still prints `TIERB_SKIP …
BLOCKED` (cargo-counted as passed, **not** a substantive pass). Full-lib grew from
the assessment's 497/11/2 to **502/11/2** — exactly the 5 new tests; no regression.

**No discrepancy with Muse's report** on any count or outcome. (Two report
wording/line-count imprecisions are recorded as P0C-N2; they do not affect any
test result.)

---

## 8. Clippy assessment — PASS

`cargo clippy --locked --lib --tests` (exit 0):
- **lib test: 185 warnings** (63 duplicates) — **identical to Muse's claim and to
  the pre-change checkpoint inventory** recorded independently in the assessment.
- **lib: 85 warnings; manifest: 1 warning** — the manifest warning
  ("only one of `license` or `license-file` is necessary") is **pre-existing**: the
  manifest is unchanged, and the dual declaration is the intentional packaging
  arrangement documented in AGENTS.md.
- **Zero warnings reference `is_ms`** (grep of the full clippy log: 0 hits) —
  Muse's "zero warnings attributed to `is_ms.rs`" is **verified**.
- The 2 `fpe/orchestrate.rs` warnings (`:457` loop index, `:692` redundant deref)
  are the known pre-existing **A-16** pair, unchanged.

**Existing vs new:** the only code file P0-B modified is `is_ms.rs` (zero
warnings); every warning location is in an unmodified file; the total count is
unchanged from the pre-change inventory. **No new warnings were introduced by
P0-B.** All 185 are existing repository warnings. CI clippy is informational/
non-gating. **No warning cleanup was performed** (P0-C did not expand scope).

---

## 9. Deviations, limitations, and findings

### 9.1 Blocking findings

**None.** No defect was found that blocks Phase 0 acceptance.

### 9.2 Non-blocking findings (owner disposition required)

| ID | Finding | Classification | Disposition suggested |
|---|---|---|---|
| **P0C-N1** | phase-0 `README.md` status table is stale: it lists P0-B as "Not started — P0-B unauthorized" and the standing status says "P0-B has not been started", while amendment D4 authorizes P0-B and a completed P0-B report exists. The README itself schedules status-link reconciliation **after P0-C**. | PASS WITH FINDINGS | Reconcile README status links in the post-P0-C documentation pass (already scheduled). |
| **P0C-N2** | P0-B report precision: (a) "+1239 / −9" vs actual **+1230 / −9** (1239 = total changed lines); (b) "−9 … evidence plumbing only" omits that **7 of 9** deleted lines are Tier-B placeholder *comment* lines (behavior unchanged); (c) §4 "`s200 SOME / 109 NONE`" — actual reproduced distribution is **some=242** none=109 (matches impl note §7 "242 MS_SOME frames"). | PASS WITH FINDINGS | Surgical report corrections in the post-P0-C documentation pass. No functional impact. |
| **P0C-N3** | Full canonical evidence rows (IS operands/margins/traces/line vectors) are recomputed, completeness-asserted and digest-pinned but **not emitted** by any retained test; only identity/flag/detail rows print. The retention policy (validation-roadmap §6) cites G-1 `BandRow` *emission* as the model. Traceability is satisfied (detail rows + retained recorder + replay gates + digest), but literal full-row inspection needs a trivial authorized test-only print. Also the ordering assertion checks the (material,frame,band) triple but not the `detail` tie-break (the sort orders by detail; the digest pins the result). | PASS WITH FINDINGS | Owner to decide whether to authorize a test-only canonical-row emission (and/or strengthen the ordering assert) to fully match the BandRow emission model. |
| **P0C-N4** | `stable_id = {material}:f{frame}:b{band}` is **not unique** for F-01/F-02 (both `quiet-sine-stereo:f16:b13`); disambiguated by `detail` + sort position as documented; both records retained and covered by the digest. | PASS WITH FINDINGS | Acceptable as documented; optionally add a per-finding ordinal to the id in a future authorized touch. |
| **P0C-N5** | Recovered R2.1-B session-temp artifacts (R1–R8) remain **volatile**; durable preservation was not authorized by D3 (M1/M2 only). All eight SHA-256 pins verified intact at audit time and two scaffolds re-ran successfully, but temp cleanup would leave the reconstruction's empirical leg reported-only (essential identities are embedded in the reconstruction + committed Qwen review §3.3, and retained gates re-validate the bound every run). | PASS WITH FINDINGS | Follow-up owner decision (already flagged in disposition record §3): authorize durable preservation of R1–R8. |
| **P0C-N6** | `b_closed` doc comment (`is_ms.rs:556-572`) still cites "the accepted R2.1-B analytical report" (the missing original) and gives the loose `+3` attribution. The reconstruction §4.1 documents both; the amendment-only policy preserves the historical reference. | PASS WITH FINDINGS | Reconcile the code-comment citation in the post-P0-C documentation pass (amendment, not silent rewrite). |
| **P0C-N7** | FR-6 ≡ OD-6 ≡ A-21 (byte baseline) is **implemented as working-tree test pins but uncommitted**; commits require separate owner authorization (D4 boundary). FR-5 (`sum_abs_x=37` hard assertion) remains **open** (outside D4 scope). Phase 0's exit criterion "baseline committed / owner confirms provenance chain complete" is therefore **pending owner action**. | PASS WITH FINDINGS | Owner to authorize the commit of Phase 0 artifacts and decide FR-5. |

### 9.3 Disclosed limitations (unchanged by P0-B, confirmed)

- Band-−1 digest-evidence path covered by code but unexercised (0 digest diffs).
- Exact ADTS/evidence hashes pin on the reference platform per the
  `production_baseline.rs` convention; other targets enforce lengths/frames.
- I4/I6/I7 and I5-carry margins retained via trace events, not standalone scalars.
- G-1 `BandRow` energy rows referenced, not duplicated (energy leg owned by
  `probe_envelope.rs`).
- Nine conditional acceptances remain conditional; Tier-B C parity remains
  blocked; no perceptual evidence claimed.

---

## 10. Acceptance criteria assessment

| # | Criterion | Classification | Basis |
|---|---|---|---|
| 1 | P0-A evidence recovery correctly represented | **PASS** | §4.1–4.3; original not recovered (independently confirmed); no invented conclusions |
| 2 | B_closed reconstruction mathematically sound and provenance-correct | **PASS** | §6; independent derivation + 5.6M pairs + 28.4M scaffold re-run + Qwen checker; identical formula; correct labels |
| 3 | G-1c historical gaps accurately represented | **PASS** | §4.2; full G-1b reconciliation verified; attribution corrected; gaps explicit |
| 4 | M1/M2 provenance reliable | **PASS** | §4.3; 64/64 + 77/77, byte-identical, occurrence distinction correct, G0 historical |
| 5 | P0-B baseline deterministic and correctly scoped | **PASS** | §5.1, §5.5; cross-process determinism verified; test-only; pattern-faithful |
| 6 | All five aggregate assertions valid | **PASS** | §5.2; real gates; 30/16/0/14/1; sources + semantics verified |
| 7 | Per-finding evidence complete and traceable | **PASS WITH FINDINGS** | §5.3; complete + integrity-pinned + traceable; emission nuance P0C-N3/N4 |
| 8 | Unsupported-frame handling correct | **PASS** | §5.4; explicit, never a pass, counted once/twice correctly |
| 9 | Existing Q31 baselines unchanged | **PASS** | §7; 2/2 debug+release; V3 pins untouched; R21B Q31 hashes match V3 |
| 10 | No production numerical behaviour changed | **PASS** | §3, §5.6, §7; cfg(test) only; Tier-A +5 exactly; Tier-B set identical |
| 11 | No unauthorized scope expansion | **PASS** | §5.6; no G0 pass-through/R2.2/G-2/G-3; no promotion; no manifest/API change |
| 12 | Repository integrity preserved | **PASS** | §3; HEAD=checkpoint=origin/main; no commits/pushes/stages; P0-A artifacts intact |

---

## 11. Overall recommendation

> ## PASS WITH FINDINGS — OWNER DISPOSITION REQUIRED

**Rationale.** All twelve acceptance criteria are satisfied (criterion 7 with
non-blocking findings). The P0-A evidence recovery is correctly represented, the
`B_closed` reconstruction is mathematically sound and provenance-correct, the G-1c
gaps are accurately recorded, M1/M2 provenance is reliable, and the P0-B baseline
is deterministic, correctly scoped, and gated by valid aggregate assertions. No
production numerical behaviour changed; repository integrity is preserved; clippy
is unchanged with zero `is_ms` attribution. **No blocking defect exists.**

The recommendation is "PASS WITH FINDINGS" rather than unconditional "PASS —
PHASE 0 ACCEPTED" because (a) seven non-blocking findings (§9.2) require owner
disposition, and (b) Phase 0's own exit criterion — *every acceptance-cited
artifact committed or permanently linked; owner confirms provenance chain
complete* — is **not yet met**: the artifacts are working-tree-only pending
owner-authorized commit (P0C-N7), the R2.1-B temp artifacts await a preservation
decision (P0C-N5), documentation reconciliation is scheduled after P0-C
(P0C-N1/N2/N6), and the fresh M2 review is authorized but not performed. These are
owner closure actions, not technical defects. The audit passes the work; the owner
must dispose the findings and authorize the remaining closure steps.

**Required owner dispositions:**
1. Disposition P0C-N1…N7 (accept / schedule corrections / authorize emission).
2. Authorize the **commit** of Phase 0 artifacts (baseline, pins, evidence,
   reconstruction, amendments, provenance, this audit) — separately authorized per
   D4 boundary.
3. Decide durable preservation of R2.1-B temp artifacts R1–R8 (P0C-N5).
4. Authorize the post-P0-C documentation reconciliation (A-15/A-22 + P0C-N1/N2/N6).
5. Schedule the authorized fresh independent M2 review + sign-off (A-02).
6. Decide FR-5 (P0C-N7).

---

## 12. Phase 1 (R2.2) readiness assessment

**Technically READY TO BEGIN**, conditional on owner authorization and disposition.

- The Phase 0 dependency for Phase 1 — *retention/pinning discipline* — is now
  **established and independently verified**: deterministic per-finding evidence,
  hard-asserted aggregates, a byte-level baseline, and explicit unsupported-frame
  handling. The production-replay harness foundation is sound and green.
- **No Phase 0 technical blocker remains** for R2.2. The decision-safety chain can
  be extended from IS/MS to PE/ATS/S2/S4/S6 on this verified basis.
- **Prerequisites (owner actions, per next-phase-recommendation §4):** dispose the
  P0-C findings; authorize R2.2 scope (prerequisite 4); ideally **commit the Phase
  0 artifacts first** so R2.2 builds on a committed, pinned basis (P0C-N7). Tier-B
  provisioning (A-10) and the fresh M2 review (A-02) proceed in parallel as owner
  items and are **not** R2.2 entry blockers per the roadmap.
- **Not ready / unchanged:** M3/FPE-3.1 acceptance, full R2.1-B closure, Stage-1
  closure, and any FPE-default change remain blocked by outstanding evidence
  (R2.2 itself, perceptual evidence, Tier-B, conditional-acceptance conditions).
  R2.2 is the necessary next technical step toward FPE-3.1 acceptance.

**This audit does not authorize or begin R2.2.** It stops for project-owner review.

---

## 13. Stop-condition compliance

- This report is the **only** repository addition
  (`docs/fpe/assessment/phase-0/P0-C-independent-audit.md`, untracked).
- No production source, test, fixture, manifest, lockfile, dependency, ADR,
  acceptance criterion, or numerical behaviour was modified. No conditional
  acceptance promoted; no Tier-B waiver; no G0/G-2/G-3/R2.2 implementation.
- No commit, push, stage, amend, rebase, reset, cherry-pick, clean, or untracked-file
  deletion. Git history not rewritten.
- Working-tree state verified after writing this report (§3 inventory plus this
  file); HEAD remains `eb2896923ec788727975268aca0beb27cba51760` = `origin/main`.
- All scratch checkers and logs reside outside the repository in session temp
  `…/T/opencode/p0c-audit/`.

**STOP — P0-C audit complete. Awaiting project-owner review and disposition.
R2.2 and any subsequent implementation phase are not begun.**
