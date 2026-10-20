# D-021 evidence archive

Permanent, read-only archive of the D-021 SHORT-frame bit-count divergence
investigation (Rust VBR-5 AAC-LC vs real C FDK-AAC, revision `e4258d2a…`).
Created 2026-10-20. Uncommitted by design — left for review, never staged.

## Layout

- `reports/` — the 15 investigation reports V4.22–V4.36, byte-exact copies.
  Invalidated intermediate reports are kept deliberately (V4.24's S6 prose,
  oracle-derived claims) so rejected hypotheses stay understandable.
- `evidence/v425/` — real-C target-frame captures (pre/post-MS spectra,
  energies, thresholds, element PE/static) and the four byte-identical ADTS
  outputs all later byte counts reproduce.
- `evidence/v426/`, `v427/`, `v428/` — live-mask, frame-reconciliation, and
  spectral-residue probe logs plus hash records.
- `evidence/v429/` — real-C S4/S5/S6/TNS captures for the two onset targets,
  live probe output, analysis scripts, the C patch applier, base/instrumented
  ADTS pairs, and build logs.
- `evidence/v430/` … `evidence/v434/` — live RXX, detect-input, M3/Q31, ring,
  and slope-table probe logs plus comparison scripts.
- `evidence/v435/` — the isolated C `mdct_block` harness source, exact-arithmetic
  and stage-split scripts, and harness/stage dumps.
- `evidence/review/` — third-party reviewer verification scripts (author and
  model unverified; see manifest caveats).
- `reviews/GLM-5.3-Flash-TNS-Review.md` — the independent TNS review, found in
  place during archival and preserved untouched (see manifest provenance note).
- `fixtures/` — the four Phase-0 input WAVs (not stored elsewhere in the repo).
- `manifest.json` — machine-readable inventory (paths, sizes, hashes, types,
  provenance, caveats, missing-artifact entries).
- `D021-CLOSURE-RECORD.md` — disposition, timeline, causal chain, four-case
  reconciliation, review summary, uncertainties, acceptance rationale, reopening
  criteria.

## Verifying integrity

Each `manifest.json` entry under `files` carries `original_sha256` (hash at the
source before copying) and `archived_sha256` (hash after copying). At archival
time all entries matched. A matching archive hash proves the archived bytes
match the inspected source bytes; it does not independently prove the source
artifact was correct or complete — provenance is recorded per file
(`copied-and-hash-verified`, `provenance-partial`, or `unavailable`), never
inferred from the hash alone. Likewise, preserving a report plus its output
dumps is not claimed as reproducibility where build inputs are absent (see
reproducibility notes below).

Run the read-only verifier from the repo root (stdlib only, modifies nothing):

```sh
python3 docs/research/d021/verify_manifest.py
```

or the equivalent manual check:

```sh
python3 -c "
import json, hashlib
m = json.load(open('docs/research/d021/manifest.json'))
ok = bad = 0
for e in m['files']:
    h = hashlib.sha256(open('docs/research/d021/' + e['archive_path'], 'rb').read()).hexdigest()
    if h == e['archived_sha256']: ok += 1
    else: bad += 1; print('MISMATCH:', e['archive_path'])
print(f'{ok} ok, {bad} mismatched')
"
```

## Independent review verification record

`reviews/GLM-5.3-Flash-TNS-Review.md` (8581 bytes,
SHA-256 `85ca48971581e3d81aa997da9d3b0e5226d8527c8b4d5337bc23fa9ab1975a8c`)
was found in place during archival and preserved untouched — not written,
regenerated, or normalized by the archival task. Content check at archival
time: 12-row evidence table, answers Q1–Q5, verdict
(`CLOSE TNS ISSUE — NUMERICAL DIFFERENCE IS SUFFICIENTLY EXPLAINED`), and
provenance caveat all present. Border-constant and KNOWN_TNS figures were
spot-checked against `rust/src/aac_enc/sfb_tables.rs:247`,
`cpp/libAACenc/src/aacEnc_rom.cpp:828`, and the `psy.rs` `KNOWN_TNS` pins
and check out. Comparison against the original GLM conversation was not
possible (that conversation is not available to the archivist); the file is
therefore recorded as content-complete per its required elements, with raw
model-output provenance unverified. Its scope is the focused TNS order
divergence, not the whole D-021 investigation.

## Temporary-path mapping index

Original report text is preserved verbatim (nothing was rewritten). To locate
the corresponding archived files, map temporary prefixes as follows:

| prefix cited in reports | archived equivalent or disposition |
|---|---|
| `…/T/opencode/phase0-20261015/wavs/*.wav` (4 fixtures) | `fixtures/` (exact copies) |
| `…/T/opencode/v4.{22..36}-*/V*.md` (phase reports) | `reports/` (exact copies) |
| `…/T/opencode/v4.25-ms-20261020/artifacts/` (24 target captures + 4 AAC) | `evidence/v425/` (exact copies) |
| `…/T/opencode/v4.2[678]-*/ *.log`, hash records | `evidence/v426/`, `v427/`, `v428/` |
| `…/T/opencode/v4.29-bitattr-20261020/` (captures, logs, scripts, AACs) | `evidence/v429/` (target frames + logs + scripts + AACs) |
| `…/T/opencode/v4.3[01234]-*/` (probe logs, analyses) | `evidence/v430/`–`v434/` |
| `…/T/opencode/v4.36-audit-20261020/` (audit report) | `reports/V4.36-…` |
| `/tmp/v429-bitattr/` (94 per-frame captures) | `evidence/v429/` holds f24 + f22; remainder not archived (reproducible via `evidence/v429/apply_v429_capture.py` + pristine `git archive` source) |
| `/tmp/v435-charness/` (harness, scripts, dumps) | `evidence/v435/` (harness source, scripts, all three dumps) |
| `/tmp/v425-out-instr/`, `/tmp/d021-mdct-capture/`, `/tmp/d021-oracle-output/`, `/tmp/*-build-*`, `/tmp/v425-out-rerun/`, `/tmp/*_all_blocktypes.txt` | not archived (bulk sweeps, build trees, superseded oracle output); committed test vectors in `rust/src/aac_enc/testdata/` (git-versioned) supersede capture dirs; byte-identity covered by archived AAC hashes |
| `/private/var/folders/…/T/opencode/{d021,v429,v425}-scratch/` | not archived (full source copies); reproducible from revision `e4258d2a…` + archived patch applier |

## Reproducibility notes (what reproduction actually requires)

- Fixture WAVs: archived (`fixtures/`); hashes in `evidence/v426/fixture-hashes.txt`.
- Real-C outputs: reproducible by building `aac-enc` at revision `e4258d2a…`
  (`-DBUILD_PROGRAMS=ON`, AppleClang 21 / CMake; exact flags in
  `evidence/v429/build-*.log`) and encoding the archived fixtures; expected
  hashes are in the V4.25/V4.29 reports.
- Real-C captures: pristine `git archive HEAD` + `evidence/v429/apply_v429_capture.py`
  + `-DV429_BITATTR` build per `evidence/v429/build-instr2.log`.
- Live captures: `cargo test --features aac_enc <probe-test>` at the same
  revision — but probe code was temporary and is NOT archived (removed
  bit-for-bit after each use); only probe *outputs* are preserved, so live
  captures are auditable, not re-runnable from this archive. Do not describe
  them as reproducible.
- C harness: `evidence/v435/harness.cpp` compiles against pristine FDK sources
  at the same revision (`c++ -O2 … -Icpp/libFDK/include -Icpp/libSYS/include`;
  full command reconstructible from V4.35 evidence); committed ring fixture
  (`rust/src/aac_enc/testdata/ring_onset-stereo_f24_ch0.txt`, git-versioned)
  is its input.
- Unpinned elements: exact AppleClang/CMake/Python3 patch versions were not
  recorded; behavior verified only through byte-identical outputs, which are
  preserved.

Entries under `missing` were deliberately not archived (bulk sweep dirs,
unlocatable review text); each names its last-known path and reason. The
committed C-struct test vectors (`rust/src/aac_enc/testdata/`, 326 files at
this revision) are referenced, not duplicated — they are versioned by git.

## What this archive does not contain

Production-code changes (none were made by the investigation), temporary probe
code (removed bit-for-bit after each use; restoration hashes live in the phase
logs), full per-frame sweep captures (target frames only; reproduction recipes
in `evidence/v429/apply_v429_capture.py` and the phase reports), or listening
test material (none was ever run).
