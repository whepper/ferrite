# FPE R2.1-B Session-Temp Artifacts (R1–R8) — Preservation and Provenance Record

**Status: P0-C remediation N5 preservation (owner-authorized). Original surviving bytes, unmodified; missing artifacts recorded explicitly, not reconstructed.**
**Date preserved:** 23 November 2026, by Muse Spark 1.3 Contributor (OpenCode) under the P0-C remediation mission (N5).
**Repository checkpoint:** `eb2896923ec788727975268aca0beb27cba51760` (`whepper/ferrite`, `main`).

## 1. What is preserved

The surviving original R2.1-B session-temp artifacts recovered by P0-A (P0-A report §3.2), copied **byte-for-byte, unmodified**:

| Preserved location | Original location (session temp) | Bytes / mtime |
|---|---|---|
| `docs/evidence/fpe-r21b/ferrite-fpe-r21b-review/is_ms.rs.orig` | `/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/is_ms.rs.orig` | 99,960 B, 2026-11-18 20:47 |
| `docs/evidence/fpe-r21b/ferrite-fpe-r21b-review/r21b-review-checks.rs` | `/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/r21b-review-checks.rs` | 6,546 B, 2026-11-18 21:12 |

Original temp copies were left untouched. No file was edited, reformatted, re-dated, or reconstructed.

## 2. Integrity verification (at preservation time)

Commands run from the preserved bundle root:

```sh
cd docs/evidence/fpe-r21b/ferrite-fpe-r21b-review && shasum -a 256 -c SHA256SUMS
diff -q <original> <preserved> && cmp <original> <preserved>
```

| Artifact | SHA-256 (P0-A pin) | Preserved SHA-256 | Checksum | Copy fidelity |
|---|---|---|---|---|
| R1 `is_ms.rs.orig` | `3e5ed6b6e43fa91c8e64bb05549b99bf2bb4b54e6e82dfc59b065a37df7f1f74` | identical | `SHA256SUMS: OK` | `diff -q` identical, `cmp` OK |
| R5 `r21b-review-checks.rs` | `20cd68a497031d510407dcd46a69ee608f09b4fbad630377cc5d8e40ade78591` | identical | `SHA256SUMS: OK` | `diff -q` identical, `cmp` OK |

Supplemental pin (P0-A §3.3, review-cycle hash chain):

```text
R1 SHA-1 16459f94d7ab1ec86c3b01fa82e0829cf85b2b8a (matches owner review Method, Qwen review §3.1 #6, DeepSeek review §7.2)
```

Bundle contents at preservation time:

```text
3e5ed6b6e43fa91c8e64bb05549b99bf2bb4b54e6e82dfc59b065a37df7f1f74  is_ms.rs.orig
20cd68a497031d510407dcd46a69ee608f09b4fbad630377cc5d8e40ade78591  r21b-review-checks.rs
```

## 3. Full R1–R8 register (preserved vs unavailable)

SHA-256 pins are the P0-A report §3.2 pins. Verification below was performed 2026-11-23 by `shasum -a 256` against the original temp paths and by filesystem inventory.

| # | Artifact (P0-A §3.2) | SHA-256 pin | Status 2026-11-23 |
|---|---|---|---|
| R1 | `T/is_ms.rs.orig` (99,960 B) | `3e5ed6b6…7f1f74` | **Preserved** (§2) |
| R2 | `T/r21b_verify/ms_bound_verify.rs` (19,607 B) | `0922e0d6…5a2a28` | **Unavailable** — source file absent; `T/r21b_verify/` is empty (volatile temp cleanup post-P0-C audit). Compiled rebuilds in `p0a-verification/` / `p0c-audit/` are derived binaries, not the original source; not preserved as originals. Pin retained from P0-A; not reconstructed. |
| R3 | `T/r21b_verify/is_margin_verify.rs` (5,644 B) | `1e58e8f6…aa954a4` | **Unavailable** — same as R2. Pin retained; not reconstructed. |
| R4 | `T/r21b_verify/final_math_verify.rs` (10,049 B) | `7a3ce183…c5313` | **Unavailable** — same as R2. Pin retained; not reconstructed. |
| R5 | `T/r21b-review-checks.rs` (6,546 B) | `20cd68a4…78591` | **Preserved** (§2) |
| R6 | `T/r21b_env_audit.log` (760,761 B) | `7115b305…8378845b` | **Unavailable** — log absent from temp (no `*env_audit*` match). Pin retained; not reconstructed. |
| R7 | `T/r21b_env_audit2.log` (771,327 B) | `c3298901…b5aee52f8` | **Unavailable** — same as R6. Pin retained; not reconstructed. |
| R8 | `T/r21b_gates.log` (11,018 B) | `5640321e…73dd4d5` | **Unavailable** — same as R6. Pin retained; not reconstructed. |

No missing artifact was reconstructed, regenerated, or invented. The loss mechanism is volatile session-temp cleanup after the P0-C audit (which verified all eight pins intact at audit time, P0-C audit §5.3). The surviving empirical leg is therefore R1 + R5 (original sources) plus the essential identities embedded in `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md` (§4–§5), the committed Qwen review §3.3, and the retained per-run bound re-validation gates (which re-validate the bound on every test run).

## 4. Handling rules

- The preserved files are **read-only evidence**. Do not edit, reformat, re-date, or "fix" their contents; any correction belongs in a dated amendment record, not in the evidence bytes.
- Re-verify integrity any time the artifacts are cited in acceptance: `shasum -a 256 -c SHA256SUMS` from the bundle root (§2).
- The preserved `.rs` files are historical snapshots, **not** production source; the production tree (`rust/src/`) is untouched by this preservation.
- Unavailable artifacts (R2–R4, R6–R8) must remain listed as unavailable until and unless the owner produces the originals; only the owner can supersede this record.

## 5. References

- `docs/fpe/assessment/phase-0/P0-A-evidence-recovery-report.md` §3.2 (artifact register, pins), §3.3 (hash chain), §6.2–6.3 (re-runs).
- `docs/fpe/assessment/phase-0/P0-A-owner-disposition-record.md` §3 ("Not preserved", follow-up decision flagged).
- `docs/fpe/assessment/phase-0/P0-C-independent-audit.md` §4.3, §5.3, §9.2/P0C-N5 (audit-time verification, preservation follow-up).
- `docs/fpe/R2.1-B-Bclosed-derivation-reconstruction.md` §5–§6 (embedded identities, reproduction).
