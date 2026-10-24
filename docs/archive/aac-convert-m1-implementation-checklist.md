# M1 — Core-9 Metadata Implementation Checklist

- Baseline: `main` at `4908778`. Authority: G0 (`docs/aac-convert-g0-decisions.md`),
  M9.2 §5, M9.3 Workstream A / milestone M1.
- Status: **checklist only. No implementation started. No spikes run.**
- Gate: G0 resolved all blocking mapping/edge ambiguity (see G0 §4). M1 may
  proceed under exactly the rules below. Any deviation needs a new owner note.

## 0. Entry conditions (verify before coding)

- [ ] `git status --porcelain=v1 -b` shows no staged/modified tracked files;
  untracked set is the pre-existing M8.1/M8.5/M9.1/M9.2/M9.3 docs plus the G0
  record and this checklist.
- [ ] G0 §§2–3 are the frozen spec: §2 table + D3 first-non-empty rule, tuple
  rule, `date`-wins, trim/empty/missing/unknown rules, §3 `--no-tags` semantics.
- [ ] S1/S2 not started; §8 deferred items untouched.

## 1. Metadata mapping and supported source comments

- [ ] Implement exactly the G0 §2 rows 1–10; no other Vorbis key produces output.
- [ ] Key matching ASCII case-insensitive; trimmed values; UTF-8 M4A text data type.
- [ ] `ilst` extension reuses the existing `gapless.rs:boxed()` checked-size
  path; `moov`-is-final enforcement, `set_len` handling, and 28-byte `M4A ` ftyp
  splice from `src/m4a.rs:118-192` are unchanged.
- [ ] Tags read after `source.verify()` succeeds, before `sink.finish`
  (`src/lib.rs:336-416`); corrupt/tagless inputs keep today's failure semantics.
- [ ] New module stays `pub(crate)` (proposed `src/meta.rs`); `M4aWriter::finish`
  gains `tags: Option<&TagSet>`; ADTS path passes nothing and writes nothing.

## 2. Missing, empty, duplicate, and multi-value tags

- [ ] Missing core key → per-file warning, conversion succeeds.
- [ ] Empty/whitespace-only → absent (trimmed value stored otherwise).
- [ ] Duplicate `date`+`year` → single `©day` (`date` wins).
- [ ] Multi-value single-valued atoms → **first non-empty wins** (D3); assert with
  multi-valued `title`/`artist`/`genre`/`composer` fixtures. `trkn`/`disk`
  exempt (tuple rule). No joining, no dedup pass, no reordering logic.

## 3. Track/disc numbers and totals

- [ ] Accept `N` and `N/M` (tolerate whitespace around `/`).
- [ ] Total from slash form, else first parseable `tracktotal`/`totaltracks`
  (`disctotal`/`totaldiscs` for disc), else `0`.
- [ ] Unparseable number → skip the atom + warning, never fail the file.
- [ ] Unit tests: `5`, `5/12`, `05/12`, missing-total + fallback, `5/x`,
  `x`, empty, `0` totals.

## 4. Date/year behavior

- [ ] `date` non-empty → `©day` = trimmed `date`; else `year` → `©day`; never both.
- [ ] Stored verbatim; no reformatting or validation of calendar shape.
- [ ] Tests: `date`+`year` (date wins), `year`-only, neither (absent + warning).

## 5. UTF-8 and special characters

- [ ] Non-UTF-8 value → skip that value + warning; file still converts.
- [ ] Pin the exact Symphonia string-error type at implementation (no new dep).
- [ ] Tests: emoji/accents/quotes/embedded separators round-trip; one bad value
  alongside good values skips only the bad one.

## 6. Unknown comments

- [ ] Every non-§2 key (including all §6 deferred keys) → ignored for output,
  counted in verbose-only `unmapped` diagnostics, never an error or failure.
- [ ] Tests seed BPM/compilation/sort/grouping/MusicBrainz/ISRC keys and assert
  absence + verbose lines + exit 0 + unchanged summary shape.

## 7. Metadata opt-out CLI semantics and invalid combinations (`--no-tags`)

- [ ] Flag spelling `--no-tags` only; valid in single-file and batch modes.
- [ ] Effect: M4A omits user atoms (gapless `iTunSMPB` + brands stay); ADTS
  unaffected (accepted as no-op, never an error).
- [ ] Orthogonal to `--format`, `--recursive`/`--no-recursive`, `--dry-run`,
  `--skip-existing`, `-v`/`-q`, `--`; dry-run plans identically with/without it.
- [ ] No new conflicts; `-v`+`-q` stays the only conflict (exit 2); unknown
  spellings stay exit 2. Existing batch-only guards unchanged.
- [ ] Tests: single-file M4A with/without flag; batch M4A with/without flag;
  ADTS + flag no-op; `--dry-run --no-tags` plan equality; invalid spellings exit 2.

## 8. M4A metadata read-back tests

- [ ] Structure (both committed audio fixtures via generated tagged FLAC copies;
  never edit committed fixtures): `ilst` = exactly mapped children + `----`
  `iTunSMPB`; `edts` absent; `M4A ` brands; `mdhd` = `N*1024`.
- [ ] `mdat` byte-identical to the untagged conversion of the same audio
  (tags must not alter AAC payloads).
- [ ] Ignored `independent_decode`-pattern round-trip: `ffprobe` shows approved
  atoms; full `ffmpeg -v error -i … -f null -` clean with zero stderr.

## 9. Regression checks

- [ ] **ADTS output:** tagged sources → ADTS has no `ilst`, bytes identical to
  untagged ADTS conversion.
- [ ] **Gapless signaling:** `iTunSMPB` value unchanged for same audio;
  delay 1600 / padding / source-count accounting untouched; `edts` still absent
  on the default path.
- [ ] **Batch behavior:** traversal/mapping/preflight/duplicate/same-root,
  per-file `ok`/`fail`/`skip`/`plan` lines, `summary:`/`dry-run:` shapes
  (no new counter), `--skip-existing`/`--dry-run`/`-v`/`-q` semantics, exits
  `0/1/2/3` — full `tests/cli.rs` + `tests/batch.rs` green.
- [ ] **No-overwrite publication:** sentinel-intact, concurrent-publication,
  temp-orphan scans (`temp_output_files(...).is_empty()`), hard-link only, no
  rename fallback — full `tests/conversion.rs` + `src/*` unit tests green.
- [ ] **Encoder gates:** Tier A suite with the 15 documented skips, doctests,
  and `cargo package -p aac` check per M8.5 §9 / `ci.yml` (run on the
  implementation branch).

## 10. Validation commands (run from `rust/`)

- [ ] `cargo build --locked -p aac-convert`
- [ ] `cargo test --locked -p aac-convert`
- [ ] `cargo test --locked -p aac-convert --test independent_decode -- --ignored --nocapture` (needs `ffmpeg`/`ffprobe`; advisory)
- [ ] `git status --porcelain=v1 -b` + `git diff --stat` review before any
  correctness claim; only expected files touched; no `Cargo.toml`/`Cargo.lock`
  drift unless a stop-and-rescope note was written (no new dependency expected).

## 11. Documentation and error taxonomy (define at implementation)

- [ ] `README.md` + crate docs updated: mapped table, deferred list, ADTS/gapless
  unchanged notes, `--no-tags` semantics. No distribution/clearance claims.
- [ ] New `ConvertError::Metadata`-family variant + `Display` + `source` +
  `category_for` entry (proposed `"Metadata"`); per-file `Failed`, `failed`
  tally, stderr detail, siblings continue; warnings never affect exit code.
- [ ] `THIRD-PARTY-NOTICES.md` touched only if (unexpectedly) a dependency is
  added — otherwise left alone; any addition forces stop-and-rescope first.

## 12. Done criteria

M1 is done when §§1–11 boxes are checked, T-tags-1…5 (M9.3 §11) pass, the
commands in §10 are green, and the diff review shows only: new `src/meta.rs`,
`src/m4a.rs` + `src/lib.rs` thread-through, `src/batch.rs` category entry,
`tests/metadata.rs` + `common` helper, and docs. Anything else is scope creep —
stop and re-scope.
