# M8 Orchestral Test Fixture — Owner Decision Record

- Preparation date: 2026-10-21
- Repository: FDK AAC to Rust
- Prepared at HEAD: `c01cec96e7a188120a4b74a2c45b0b5a57a5313f`

**Status of this record: PREPARATION ONLY — NOT AN APPROVAL.**

## 1. Decision summary

| Field | Value |
|---|---|
| Asset | `rust/src/aac_enc/testdata/m8-music-eroica-stereo-48k.wav` |
| Provenance record | `rust/src/aac_enc/testdata/m8-PROVENANCE.txt` |
| Current disposition | **`PROVISIONAL` — no approval has been granted** |
| Owner decision | **Pending owner decision** |
| Approver name / role | **Pending owner decision** |
| Decision date | **Pending owner decision** |

The provenance header remains authoritative until the owner completes the fields in §9 and the closure criteria in §10. This record does not change the header, the asset, its hashes, the fixture registrations, or any release-gate outcome.

## 2. Evidence basis

Prepared from the committed provenance record, the M6.5 release gate, the benchmark plan, the clean-checkout audit, the commit-readiness record, and the fixture/test registrations. Retained records were also inspected where cited; those files are outside the repository and are not approvals.

There is no standalone committed M8 audit report. This record reflects the read-only 2026-10-21 provenance audit and the sources in §11. External Commons and Internet Archive pages were not fetched during preparation; source and rights statements below remain claims unless they are explicitly identified as committed-file facts.

## 3. Verified from committed artifacts

These facts establish the integrity and identity of the committed bytes. They do **not** establish source rights, provenance review, or owner approval.

| Item | Verified value |
|---|---|
| WAV file size | 1,920,044 bytes |
| WAV SHA-256 | `4413c8a41fe148192081c504ecfb5d69b5c7c4fc158cefff8b0dfadf8923986b` |
| PCM data size | 1,920,000 bytes |
| PCM SHA-256 | `fe6e890c7c8f8c5ec3251e9f7971bd7b3e4f2ebb0fe6493fd496d21a0ed46cce` |
| PCM MD5 | `b35eacc217309e91744afef29ec3cb2c` |
| Format | RIFF/WAVE, PCM s16le, 48,000 Hz, 2 channels, 480,000 frames, 10.000000 s |
| Provenance file size / SHA-256 | 9,740 bytes / `a24e0786dc919da101a30a2b0028f6a7ffdcd3055cc23f8a6b58135102d131cd` |
| Committed M6.5 seven-file manifest table | WAV hash and provenance hash match (`docs/aac-enc-m6-release-gate.md` §2) |

Retained cross-checks, outside the repository and not committed:

- `$TMPDIR/opencode/v3-20261018/manifest/fixture-identities-19.json` agrees on the WAV/PCM hashes and the 480,000-frame identity. It also records `fixture_pcm_manifest_19 = 99c3c7f8…29de9` and `corpus_pcm_digest_19 = ce798562…5fc1`. The committed M6.5 report contains the seven-file manifest table, not a 19-fixture digest record.
- The retained C reference bitstream is 235,444 bytes with SHA-256 `2eb91eeaa6c864441f7064716c72aefbe3db2203b1a7d1fb6625e6b91e04fa11`.

## 4. Unverified claims and limits

The following are documented claims. They have not been independently verified in this preparation, and they are not converted into facts by repetition in the provenance file.

- **Source identity.** The work, performers, project, Commons page/revision, media URL, native FLAC format, byte length, and checksums are recorded in `m8-PROVENANCE.txt:20-42`. No source FLAC was found under the retained workspace; `v3-music-20261017/src/` is empty. The claimed source SHA-256 `7a4b4d89…30f24a` cannot be recomputed locally.
- **Rights / licence.** The record states a Musopen public-domain release for the recording, a `PD-old-100-expired` composition rationale (d. 1827), Commons API values, and an Internet Archive CC Public Domain Mark 1.0. It distinguishes the recording from the composition, so it covers the specific recording **on its face**. It does not constitute a legal verification, does not state a jurisdiction-specific analysis, and does not record a rights-determination date. The committed M6.5 gate describes licensing as resolved for redistribution; that is a repository conclusion, not an external legal opinion.
- **Acquisition.** The Commons media URL and Internet Archive cross-check are documented, including claimed matching size/SHA-1/MD5. No local original file or local archival copy is retained, so acquisition and source-byte identity have not been independently reproduced.
- **Transformation.** The ffmpeg command, excerpt range, no-resampling/no-downmix policy, arithmetic-right-shift reduction, and no-dither policy are documented in `m8-PROVENANCE.txt:65-82`. The retained `out/run1/verify.json` records `reduction_mismatches: 0` and the committed PCM hash, and the implementation record reports two byte-identical runs. Those are retained verification outputs, not an independently repeatable derivation without the source FLAC.

A public-domain assertion, even if correct, would not establish the missing independent review or owner acceptance.

## 5. Missing evidence

| Missing item | Status and consequence |
|---|---|
| Authoritative corpus qualification report | `V3-MUSIC-FIXTURE-QUALIFICATION.md` at the cited scratch path, SHA-256 `22010c8380266b6c7b9c43e5e0479e5e8b06bf159fabf156f85f2e50fe1c6bab`, is absent from the repository, Git history, and the retained workspace. The `PASS` citation cannot be verified. |
| Replacement qualification review | If the report cannot be recovered, a new review of the current fixture/provenance package is required. Do not create a substitute approval record. |
| Independent review of the current provenance package | The retained implementation record says the corpus remains provisional “pending independent review of this provenance package” and ends “Stopping here for independent review.” No reviewer, role, date, scope, or outcome was found for that review. |
| Named approval authority | The repository refers to a “corpus owner” but does not name the person or role. No written acceptance exists. |
| Written owner acceptance | No decision, rationale, conditions, date, or evidence references have been recorded by an authorized owner. |

The retained implementation record’s statement that the qualification report was “independently reviewed: PASS” is a claim inside a record that explicitly stops for independent review; no review artifact for that record was found. It is not the report and does not close the package-review gap.

## 6. Review evidence that does not close the gap

The separate V3 measurement campaign report records an independent Muse audit of the **measurement report** (`PASS WITH FINDINGS`, 2026-10-19). That audit addresses measurement reproducibility and the F1–F7 corrections. It does not address M8 source identity, rights, acquisition, corpus qualification, provenance approval, or owner acceptance, and it is not evidence that M8 provenance was approved. It must not be used as a substitute for the missing qualification/package review.

## 7. Release impact

- **Production encoder compilation/runtime:** M8 does not block it. The fixture table and `M55_FIXTURES` are `#[cfg(test)]`; production builds do not read the WAV.
- **Test and research workflows:** M8 is consumed by `testdata.rs`, the M55 reference check, the 19-stream M5.g census, the 20-stream M6 migration equality test, and the M5.8 corpus validation. Removing it would break `include_bytes!` compilation unless the dependent fixture tables and tests are updated together.
- **Broader release:** The corpus must not be described as fully approved while the provenance header is `PROVISIONAL` and the evidence in §5 is missing. The M6.5 gate is `PASS WITH FINDINGS`; M8 is one of the recorded findings and remains open.
- **Proposed encoder-only release treatment (not a change to the M6.5 criteria):** If the current 19-fixture corpus is retained, the applicable release gate must explicitly record M8's `PROVISIONAL` status and the open finding. If the owner chooses rejection or replacement (option C), the dependent fixture tables, `include_bytes!` references, tests, and corpus validation must be updated coherently and the affected workflows must be revalidated. This record does not make retaining M8 a universal M6.5 release requirement.

## 8. Decision choices for the owner

**A. Keep M8 provisional pending evidence.**

Maintain the current status, keep the release-gate finding, and continue using M8 only as a provisional research/test asset. No approval is granted.

**B. Accept after the specified evidence and independent review are completed.**

Approve only after the qualification evidence is recovered or recreated, the current provenance package receives documented independent review, the approval authority is identified, and written acceptance is recorded. This is the only choice that would change the disposition to approved.

**C. Reject or replace M8.**

Reject or replace the fixture only after evaluating test-corpus impact and updating the dependent fixture tables, `include_bytes!` call sites, reference data, and tests. This is not a documentation-only change.

**Owner selection: Pending owner decision.**

## 9. Required owner fields

| Field | Value |
|---|---|
| Approver name / role | **Pending owner decision** |
| Decision (A / B / C) | **Pending owner decision** |
| Rationale | **Pending owner decision** |
| Date | **Pending owner decision** |
| Conditions | **Pending owner decision** |
| Evidence references | **Pending owner decision** (initial references in §11) |
| Scope of acceptance | **Pending owner decision** |
| Signature / record location | **Pending owner decision** |

## 10. Closure criteria

1. The original `V3-MUSIC-FIXTURE-QUALIFICATION.md` is recovered at a stable, documented canonical location — in the repository or an identified canonical archive — and its canonical path/reference and matching SHA-256 (`22010c83…`) are recorded in the provenance package as part of acceptance (M6.5 §3). If it cannot be recovered, a replacement qualification review of the current fixture and provenance package must be completed and retained in the same manner, with its own canonical reference and SHA-256. Absence of the original report does not waive this criterion.
2. The current provenance package receives documented independent review, including reviewer identity/role, date, scope, and outcome.
3. The approval authority is identified by name or role.
4. Written acceptance is recorded with the fields in §9.
5. Relevant documentation and release-gate references are updated **after** acceptance; `m8-PROVENANCE.txt` is not changed before then.
6. Validation is rerun and recorded for the affected test and release-gate workflows.

Optional reproducibility improvements: commit the 19-fixture manifest/digests and retain verifiable source or archival evidence (for example, a retained source FLAC or independently checkable Commons/Internet Archive hash evidence).

## 11. Evidence references

- `rust/src/aac_enc/testdata/m8-PROVENANCE.txt` — status, source, licence, excerpt, conversion, limitations, corpus status.
- `rust/src/aac_enc/testdata/m8-music-eroica-stereo-48k.wav` — committed fixture.
- `docs/aac-enc-m6-release-gate.md` §2–3, §8 — seven-file manifest table; m8 reconciliation and owner question.
- `docs/aac-enc-benchmark-plan.md` §1 — V3 fixture description and provisional-corpus statement.
- `docs/aac-enc-m6-clean-checkout-reproducibility.md` §3, §8 — test-only asset status and m8 decision.
- `docs/aac-enc-git-commit-readiness.md` §6 — m8 licensing/approval distinction.
- `rust/src/aac_enc/testdata.rs:449-455`; `rust/src/aac_enc/stream_driver.rs:4079-4099`, `:4393-4450`, `:4535-4570`; `rust/src/aac_enc/m5g_census.rs:24-45`; `rust/src/aac_enc/m6_migration.rs:1-80` — fixture registration and consuming tests.
- Retained, not committed: `$TMPDIR/opencode/v3-20261018/V3-FIXTURE-IMPLEMENTATION.md`; `$TMPDIR/opencode/v3-20261018/manifest/fixture-identities-19.json`; `$TMPDIR/opencode/v3-music-20261017/out/run1/verify.json`; `$TMPDIR/opencode/v3-measure-20261019/V3-MEASUREMENT-REPORT-CORRECTED.md`.
- Documented external sources (not fetched during preparation): the Commons file page/media URL in `m8-PROVENANCE.txt:26-32`; Internet Archive item `MusopenCollectionAsFlac`.

## 12. Non-interference confirmation

- M8 provenance status remains `PROVISIONAL`.
- No code, test, fixture byte, manifest, baseline, provenance file, or release-gate outcome was modified.
- No owner, reviewer, approval, or missing artifact was invented.
- No staging, commit, amend, rebase, reset, stash, clean, or push was performed.