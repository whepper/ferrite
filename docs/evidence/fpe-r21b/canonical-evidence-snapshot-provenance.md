# R2.1-B Canonical Evidence Snapshot (RA-1) — Provenance Record

**Status: Owner-authorized RA-1 durable capture. Test-generated evidence, unmodified.**
**Date captured:** 24 November 2026, by Muse Spark (OpenCode) under owner authorization (RA-1 snapshot task).
**Source commit (Group 2 implementation):** `07c438f2541850445350e7d198f64cd1b030e039`
(`test(fpe): harden R2.1-B reproducibility evidence`).
**Snapshot generation commit (pre-snapshot HEAD):** `2e21630fa8370ecd9695d2638444ba7e09ed3d64`
(`docs(fpe): preserve phase 0 evidence bundles`).

## 1. What is captured

`canonical-evidence-snapshot.txt` contains the 30 complete R2.1-B canonical
evidence records emitted by the committed test
`r21b_p0b_canonical_evidence_is_emitted`
(`rust/src/aac_enc/fpe/tests/is_ms.rs`, gate_tests module, committed in Group 2).
Each record is one `R21B-CANONICAL <unique-id>` header plus the full
`FindingEvidence::canonical_string()` body (fixed field order, decimal
integers, bracketed decimal line vectors, `|`-joined `Debug` trace chains,
`n/a` for absent optionals), in deterministic `sorted_evidences` order
(`(material_idx, frame, band, detail)`). No record is truncated and no summary
is substituted.

## 2. Generation

Workspace directory: `<repository>/rust` (established Rust workspace directory).

Exact invocations (two independent debug executions, two independent release
executions, raw output captured outside the repository first):

```sh
cargo test --locked --lib r21b_p0b_canonical_evidence_is_emitted -- --nocapture
cargo test --locked --lib r21b_p0b_canonical_evidence_is_emitted -- --nocapture
cargo test --release --locked --lib r21b_p0b_canonical_evidence_is_emitted -- --nocapture
cargo test --release --locked --lib r21b_p0b_canonical_evidence_is_emitted -- --nocapture
```

Extraction: from each raw log, the contiguous block from the first
`R21B-CANONICAL <id>#<n>` header through the final record's `trace_f=` line
(up to, excluding, the `R21B-CANONICAL-DIGEST` line), preserving exact bytes
and ordering. Inter-record blank separators preserved; no trailing blank line.

Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, host
`aarch64-apple-darwin` (`uname Darwin arm64`) — the canonical digest
reference configuration (macOS/aarch64).

## 3. Verification results

| Run | Result | Headers | Unique IDs | Digest line |
|---|---|---|---|---|
| debug #1 | `ok. 1 passed; 0 failed; 516 filtered out` | 30 | 30 | `R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x05441ee2908755aa` |
| debug #2 | `ok. 1 passed; 0 failed; 516 filtered out` | 30 | 30 | `R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x05441ee2908755aa` |
| release #1 | `ok. 1 passed; 0 failed; 516 filtered out` | 30 | 30 | `R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x05441ee2908755aa` |
| release #2 | `ok. 1 passed; 0 failed; 516 filtered out` | 30 | 30 | `R21B-CANONICAL-DIGEST rows=30 fnv1a64=0x05441ee2908755aa` |

- All four extracted snapshots are **byte-identical** (`cmp`: debug1==debug2,
  debug1==release1, debug1==release2); ordering is deterministic across
  debug and release.
- F-01/F-02 remain separately represented:
  `R21B-CANONICAL quiet-sine-stereo:f16:b13#1` and
  `R21B-CANONICAL quiet-sine-stereo:f16:b13#2`.
- Record count: **30**. Unique evidence IDs: **30**.

## 4. Digests

- Canonical digest (computed by the committed test itself, `p0b_fnv1a64`:
  FNV-1a 64, offset basis `0xcbf29ce484222325`, prime `0x100000001b3`, over
  the sorted canonical strings joined with `\n` — the same algorithm and
  canonical input as `r21b_p0b_byte_baseline_is_pinned`): **`0x05441ee2908755aa`**,
  reproduced exactly in all four runs, matching the committed pin.
- Snapshot file SHA-256 (`sha256sum` of the exact snapshot file):

```text
0a9a95faf8cf9a7aebd580faa16f37fac6b0d1de293f031654246e6a4b6e23f5  canonical-evidence-snapshot.txt
```

Verify with: `sha256sum -c canonical-evidence-snapshot.sha256` from
`docs/evidence/fpe-r21b/`.

## 5. Handling rules and scope

- This snapshot is a **durable capture of test-generated evidence, not new
  numerical or perceptual validation**. It adds no finding, bound, envelope,
  or acceptance claim beyond preserving the Group 2 test output.
- The existing `SHA256SUMS`, original evidence files (`is_ms.rs.orig`,
  `r21b-review-checks.rs`), provenance records, and all source code are
  untouched by this snapshot. Missing R2–R4/R6–R8 artifacts remain
  unreconstructed (see `fpe-r21b-provenance.md`).
- The snapshot file is read-only evidence. Any correction belongs in a dated
  amendment record, not in the snapshot bytes.

## 6. References

- Committed test: `rust/src/aac_enc/fpe/tests/is_ms.rs`
  (`r21b_p0b_canonical_evidence_is_emitted`), Group 2 commit `07c438f`.
- Canonical pin: `0x05441ee2908755aa` (committed test assertion, reference
  platform macOS/aarch64).
- R1–R8 register: `docs/evidence/fpe-r21b-provenance.md`.
