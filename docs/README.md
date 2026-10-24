# Documentation map

This index separates maintained guidance from historical records.
Follow the maintained documents; treat the archive as evidence of how
decisions were reached, not as current normative guidance.

## Maintained guidance

### Converter

- [L0 loudness decisions](aac-convert-l0-loudness-decisions.md) — frozen
  owner-approved loudness decisions D1–D7.
- [L2 loudness integration design](aac-convert-l2-loudness-integration-design.md) —
  finalized design; normative decisions are in §10.
- [G0 metadata decisions](aac-convert-g0-decisions.md) — core-9 metadata
  mapping and edge-case decisions (M1 slice).
- [S1.3 cover-art final specification](aac-convert-s1.3-cover-art-final-spec.md) —
  authoritative cover-art behavior contract (selection, warnings, order).
- [S2.1 24-bit FLAC plan](aac-convert-s2.1-24bit-plan.md) — 24-bit truncation
  acceptance policy (§§2 and 5 as implemented).
- [M8.1 batch design](aac-convert-m8.1-batch-design.md) — batch conversion
  architecture, CLI, and output-safety invariants.

### Encoder

- [Migration specification](aac-enc-migration-spec.md) — float-native
  thesis (§5).
- [M6 API contract](aac-enc-m6-api-contract.md) — production encoder API
  contract.
- [VBR-5 profile](aac-enc-vbr5-profile.md) — fixed encoder operating point.

## Archive

[archive/](archive/) holds dated plans, implementation reports, and
superseded audits: the L3 loudness evidence trail, the M9 parity/design/plan
series, S1.1/S1.2 cover-art predecessors, the M1 checklist, the M8.5
readiness audit, and the Ferrite phase-1 and modernization reports.
Historical record, not current guidance.

Other top-level `aac-enc-*` milestone, divergence, and benchmark reports —
excluding the three maintained encoder documents listed above — are
historical evidence, not normative guidance. `research/d021/` is a
read-only divergence evidence archive (see `research/d021/README.md`).

For repository layout and validation workflow, see the root
[`README.md`](../README.md).
