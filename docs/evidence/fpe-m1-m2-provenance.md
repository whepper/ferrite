# FPE M1/M2 Evidence Bundles — Preservation and Provenance Record

**Status: Owner-authorized preservation (D3). Original evidence, unmodified.**
**Date preserved:** 21 November 2026, by Qwen4 Max Preview (OpenCode) under
owner decision D3 (`docs/fpe/assessment/phase-0/P0-A-owner-disposition-record.md`).
**Repository checkpoint:** `eb2896923ec788727975268aca0beb27cba51760`
(`whepper/ferrite`, `main`).

## 1. What is preserved

The two original independent-review bundles recovered by P0-A from volatile
session temp (P0-A report §5.1), copied **byte-for-byte, unmodified**:

| Preserved location | Original location (session temp) | Base state |
|---|---|---|
| `docs/evidence/fpe-m1/ferrite-fpe-m1-review/` | `/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/fpe-m1-review/ferrite-fpe-m1-review/` | HEAD `b188415` + uncommitted M1 working tree (2026-11-17 00:25–00:53) |
| `docs/evidence/fpe-m2/ferrite-fpe-m2-review/` | `/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/fpe-m2-review-bundle/ferrite-fpe-m2-review/` | HEAD `b188415` + uncommitted M1+M2 working tree (2026-11-17 01:37–01:46) |

Both bundles document **working-tree-only** implementations at base HEAD
`b188415e3c16dbf00706984858c6a568c9b705ff` (no commits existed at the time;
the FPE work was squash-committed later, on 2026-11-21, as `8db94f1` /
`fe73705`). The bundles are therefore **original contemporaneous evidence**
of the pre-squash M1/M2 states — the states whose green runs the published
history no longer contains (A-01).

Session-temp verification mirrors (`fpe-m1-review-verify/`,
`fpe-m2-review-verify/`) are duplicate staging copies and were **not**
preserved separately; the original temp copies were left untouched.

## 2. Integrity verification (at preservation time)

Commands run from each preserved bundle root:

```sh
cd docs/evidence/fpe-m1/ferrite-fpe-m1-review && shasum -a 256 -c SHA256SUMS
cd docs/evidence/fpe-m2/ferrite-fpe-m2-review && shasum -a 256 -c SHA256SUMS
```

| Bundle | Checksum result | File count | Copy fidelity |
|---|---|---|---|
| M1 | **64/64 OK, 0 failures** | 66 files (64 covered + `SHA256SUMS` + `MANIFEST.md`, which the M1 checksum file excludes) | `diff -rq` vs original: **byte-identical** |
| M2 | **77/77 OK, 0 failures** | 78 files (77 covered + `SHA256SUMS` itself) | `diff -rq` vs original: **byte-identical** |

Supplemental pins for files excluded from the bundles' own checksum files:

```text
M1 MANIFEST.md   549ad37db19ef2f55161f986727dc88299f7d90d0498c39fc089c6685e0e75b0
M1 SHA256SUMS    89332c96b6cfbd62ba01729598555c4537493597fbda459b73fa74170108334f
M2 SHA256SUMS    785ea4b104c3d10461bf081e7455145d87bfe9b914c80ca8ac5ca643b402470d
```

Key report pins (SHA-256, also recorded in P0-A §5.1):

```text
M1 report/M1-REPORT.md                    d720a84b1ffd88d617961ea831ce74729b2f2aff58d6cbe0e0f5ef2db0bb068a
M1 REMEDIATION.md                         81e97292e22bcc9f3fbe5dd7707fa97da75a374aa68a38c3b66369b93b8ef814
M2 report/m2-implementation-report.md     7b6ce78dbe50e499cddbe7c9c54799b2a606e3044651004ce0d432d0b97d4486
```

## 3. Contents relevant to the acceptance chain

**M1 bundle:** `report/M1-REPORT.md` (implementation report: 21 new tests;
Tier-A 384/0; baseline 2/2; `fpe_selected_streams_are_byte_identical_to_q31`
green end-to-end; no float kernels); `REMEDIATION.md` (review-finding
remediation record, §4 below); `README.md`, `MANIFEST.md`, `SHA256SUMS`;
`diff/m1-complete.diff` (git-apply-able against `b188415`) + per-file diffs;
complete new/modified file copies; line-ranged context excerpts with
provenance headers; `validation/pre-change/` (pristine-tree logs),
`validation/post-change/` (incl. `fresh-verification.log`,
`remediation-validation.log`); `validation/fixture-stability.txt` +
`fixtures-run1/` (6 fixtures, 3-run byte-stability),
`fixture-crosscheck.md` (Python↔Rust sample-exact cross-check);
`git-status.txt`.

**M2 bundle:** `report/m2-implementation-report.md` (**the M2 green-run
record**: FPE bit-identical to Q31 on complete output + all frame-surviving
state, per frame, multi-frame; 24/24 debug **and** release; Tier-A 388
release / 392 debug; baseline 2/2; seam byte-identity 3/3; no float kernel,
no round-trip); `report/m1-context.md`; `00-README.md`, `MANIFEST.txt`,
`SHA256SUMS`; `git/diff-complete.patch` (37 entries) + tracked/new-file
patches + HEAD/status snapshots; complete `source/fpe/**` and
`source/modified-rust/**`; verbatim `q31-reference/**` kernel extracts;
`docs/stage-correspondence.md`; `validation/*.log` (execution logs — P0-A
cross-checked the claimed results against the logs themselves: fpe-debug
24/24, fpe-release 24/24, production-baseline 2/2, tier-a, seam, API,
doctests, full-lib-no-skip with the 11 loud Tier-B failures, fmt/clippy/
licensing/package); `tests/list-*.txt`.

## 4. Review-occurrence distinction (owner-confirmed framing)

- **M1 — independent review occurrence: CONFIRMED.** `REMEDIATION.md`
  records two findings from an independent M1 source review (Finding 1:
  per-file modification-notice dates; Finding 2: test-count reporting) and
  their remediation with full re-validation. **The reviewer's sign-off
  document itself is not retained** in the bundle or anywhere searched;
  reviewer identity/model is unrecorded in the artifacts. Occurrence is
  evidenced; sign-off remains unretained (A-02 residual).
- **M2 — independent review occurrence: UNCONFIRMED.** The bundle was
  *prepared for* independent architecture + numerical review
  (`00-README.md`); no record that the review was performed or signed off
  was found in any searched location.
- **Fresh independent M2 review and sign-off: AUTHORIZED by owner (D3),
  not yet performed.** It is a separately scoped work item (not P0-B). The
  preserved bundle provides everything needed (complete source, Q31
  reference extracts, stage correspondence, diffs, logs).
- **No sign-off documents were manufactured, backdated, or reconstructed.**
  This record states occurrence status only.

## 5. Gate status after preservation

- **G0 (M2 pass-through bit-identity) remains historical and
  non-rerunnable from the published tree** (the value lane is float since
  FPE-3.1; the seam test documents that FPE-3.1 replaces the M2 byte-identity
  gate). Preservation strengthens the historical evidence leg; it does not
  re-establish G0 on the current tree. **A-01 remains open.** A test-only
  pass-through comparison mode remains an *unauthorized* P0-B B4 option.
- **A-02** is partially resolved: M1/M2 implementation and green-run records
  are now durably preserved; M1 review occurrence is evidenced; M2 review
  occurrence is unconfirmed and a fresh M2 review is authorized (§4).
- Context note (P0A-O4): the M1 remediation bumped five per-file
  modification-notice dates to `2026-11-16` (recorded as owner-issued); the
  M2 report recommended restoring `2026-10-26` before commit; the committed
  tree carries `2026-10-26` in all five files. The restoration decision
  record is unretained. Licensing is a Phase 0 exclusion; no header or
  licensing change is made by this preservation (bundle copies retain their
  original headers verbatim).

## 6. Handling rules

- The preserved bundles are **read-only evidence**. Do not edit, reformat,
  re-date, or "fix" their contents (including the header-date state they
  snapshot); any correction belongs in a dated amendment record, not in the
  evidence bytes.
- Re-verify integrity any time the bundles are cited in acceptance:
  `shasum -a 256 -c SHA256SUMS` from each bundle root (§2).
- The source copies inside the bundles are historical snapshots, **not**
  production source; the production tree (`rust/src/`) is untouched by this
  preservation.

## 7. References

- `docs/fpe/assessment/phase-0/P0-A-evidence-recovery-report.md` §5
  (recovery), §6.4 (integrity verification).
- `docs/fpe/assessment/phase-0/P0-A-owner-disposition-record.md` (D3).
- `docs/fpe/assessment/findings-register.md` A-01/A-02 (status context).

---

## 8. Dated clarification — fresh M2 review and formal M2 acceptance (24 November 2026)

*Appended without altering §§1–7 (history preserved). The preserved bundle
bytes, checksums and review-occurrence distinctions above are unchanged.*

- A **fresh independent M2 review** was performed by **GLM 5.3 Flash** on
  24 November 2026 and durably recorded in
  `docs/fpe/assessment/m2-independent-review-glm-2026-11-24.md` (verdict
  **PASS WITH FINDINGS**; M2 F-1…F-4 non-blocking; all ten M2 acceptance
  criteria satisfied).
- The project owner **formally accepted M2** on 24 November 2026
  (`docs/fpe/assessment/phase-0/P0-A-owner-disposition-record.md`,
  Amendment M2 acceptance).
- The original M2 review occurrence remains **unconfirmed as history**
  (§4); the GLM review is a **separate event** and does not backdate or
  manufacture historical sign-off.
- G0 remains historical and non-rerunnable (A-01 open). This clarification
  makes no production, test, manifest or evidence-byte change.