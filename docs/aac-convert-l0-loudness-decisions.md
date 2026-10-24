# L0 — Loudness Decisions for `aac-convert` (Owner-Approved)

> Maintained guidance: owner-approved loudness decisions D1–D7. The implementation has shipped; the original scope notes below record the historical decision process.

- Date: 2026-10-24 (UTC)
- Baseline: `main` @ `7ebf208d40d78adcd265e3736e93f8a8aa93e8d1` (converter
  milestone M1/M1.1/S2/S1.3 committed locally, not pushed).
- Authority: `docs/archive/aac-convert-m9.1-feature-parity-audit.md` (parity evidence),
  `docs/archive/aac-convert-m9.2-integration-design.md` Q4/§6.2 (deferral + hook),
  `docs/archive/aac-convert-m9.3-implementation-plan.md` §8/D (deferred bucket),
  `docs/aac-convert-g0-decisions.md` §6 (deferred set),
  `docs/aac-convert-m7.1-architecture-decisions.md` §4/U9,
  `docs/aac-convert-m8.1-batch-design.md`, and the read-only FLAC2AAC
  Loudness Parity Audit.
- Status: **owner-approved product decisions D1–D7 below. No implementation,
  spike, library selection, or dependency change is authorized by this
  document. L1/L2 remain separately scoped.**
- Constraints in force: no FFmpeg or `rsgain` as required production
  dependencies; no encoder algorithm changes; no VBR 1–4; no ABX testing;
  no silent change to output overwrite/replacement guarantees; normal CI
  tests must not require external loudness tools.

G0, M9.1, M9.2, and M9.3 are historical/planning records and are unchanged
by this document. Where this record advances a previously deferred item,
it says so explicitly; it does not rewrite those documents.

## Approved decisions

### D1 — Output metadata (approved)

Target **both ReplayGain 2.0 metadata and `iTunNORM`** for loudness parity.

Exact ReplayGain tag spellings, MP4 atom representation and ordering, and
the evidence-backed ReplayGain-to-`iTunNORM` conversion formula are **not**
decided here. They remain subject to investigation during L1/L2. No formula,
tag set, or atom layout is specified or implied by this approval.

### D2 — Loudness target (approved, amended; see history)

The fixed reference for the first Rust release is **−18.0 LUFS**.

This matches the audited FLAC2AAC `reference_loudness` default of
−18.0 LUFS, keeping a consistent reference convention for ReplayGain 2.0
and iTunNORM.

ReplayGain tags retain standard −18 LUFS reference semantics.
iTunNORM/Sound Check generation must follow the evidence-backed FLAC2AAC
conversion convention using the approved −18.0 LUFS reference. The exact
conversion implementation and output vectors remain subject to independent
validation during L2; no formula is specified or changed by this approval.

Do not add target configurability in the first release. TOML remains out of
scope. Do not invent target bounds, steps, or CLI options.

### D3 — Analysis (approved direction; selection deferred)

Native EBU R128-compatible loudness analysis is the intended direction.

Do not select a library or algorithm yet. L1 must assess candidates,
licensing, MSRV, determinism, and resource cost before any selection.

Tag-passthrough and the existing ReplayGain reuse fast path are not part of
the first-release scope.

### D4 — Album grouping (approved)

The **source parent directory defines an album**. Files in the same source
directory share album-level loudness. Disc subdirectories are separate
albums, matching the audited FLAC2AAC grouping behavior.

Metadata-based grouping remains deferred.

### D5 — Partial reruns (approved rule; mechanism deferred)

Album analysis must include **all album members, including outputs retained
because of `--skip-existing`**.

The necessary analysis-read path and analysis domain remain to be designed
in L1/L2. An analysis read is not a conversion and must not silently alter
existing skip semantics beyond the explicitly approved loudness requirement
stated here.

### D6 — Retry (approved)

Metadata-only retry/retag is **not required** in the first release. Retry may
require a full run and re-encoding.

Preserve the current no-replace guarantee. Any future retag feature requires
a separate atomic-replacement safety design and approval.

### D7 — CLI and diagnostics (approved)

- Do not introduce a new flag for loudness in the first release.
- `--no-tags` suppresses loudness tags and loudness warnings, consistent
  with the existing metadata opt-out contract.
- ADTS output never receives loudness metadata.
- Dry-run must not decode audio or perform loudness analysis.
- Recoverable loudness-analysis failures produce clear warnings and allow
  audio conversion to succeed without loudness tags.
- Genuine conversion/input failures must not be disguised as optional
  metadata failures.
- Preserve existing exit codes and summary shape. Do not change public API
  signatures as part of L0.

## Still deferred to L1/L2 (do not resolve speculatively)

- Native R128 implementation/library selection and dependency
  licensing/MSRV.
- Exact ReplayGain tag keys, MP4 atom representation, and atom ordering.
- The evidence-backed ReplayGain-to-`iTunNORM` formula.
- Source FLAC PCM versus decoded AAC analysis domain.
- Peak measurement semantics, including sample peak versus true peak.
- Retained-output analysis strategy and memory/performance costs.
- Detailed failure taxonomy and warning wording.

## Change history

- 2026-10-24: L0 proposal drafted (seven areas D1–D7) from the read-only
  loudness parity audit; proposed fixed default −18.0 LUFS tracking the
  audited FLAC2AAC default.
- 2026-10-24: Owner review **amended D2** from the proposed −18.0 LUFS to
  the owner-selected **−14.0 LUFS**, recorded at the time as the approved
  fixed default with its explicit divergence from FLAC2AAC.
- 2026-10-24: Owner revised D2 from −14.0 LUFS to **−18.0 LUFS** as the
  approved fixed reference, matching the audited FLAC2AAC
  `reference_loudness` default. All other decisions unchanged.
