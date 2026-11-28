# R2.2 Evidence Bundle (E2/E3, schema v2)

Generated under `docs/fpe/assessment/R2.2-group-E2-E3-owner-decision.md` §2 from the frozen E1 schema v2 (no redesign).

- Rows: 184781 (FRAME/BAND/CHAIN/LEAF/REPLAY/COV, total order, `R22SCHEMA\t2`).
- Files: `r22-bundle.tsv` (10844537 bytes), `r22-bundle.json` (35480048 bytes), `json/` OP×row-type partition (E-OD-8), `MANIFEST.txt`, `SHA256SUMS`.
- Provenance: manifest records HEAD, platform, per-file SHA-256 + bytes, row counts; SHA256SUMS pins every file; verification is read-only by default (E-OD-4: set `FPE_R22_GENERATE=1` to regenerate).
- Ordering: see `evidence.rs` E2/E3 total-ordering specification (F-A); REPLAY sorts purely by tag; COV sorts by (OP, scope, dimension).
- Durability: files are written with flush + file-fsync + directory-fsync; torn writes fail closed (F-B).
- Group F isolation (SG-3): recurrence dimensions are `NotGenerated`; no FixedPoint/Cycle/BoundedNotTerminated conclusions from Group E outputs.
- Register: `docs/fpe/assessment/R2.2-findings-register.md` (R22-* namespace, E-OD-5); this bundle is implementation output, not acceptance.
