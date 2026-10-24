# G0 — Owner Decisions for M1 Metadata Slice

> Maintained guidance: core-9 metadata mapping and edge-case decisions used by the shipped implementation. The original implementation-status notes below are historical.

- Date: 2026-10-24 (UTC)
- Baseline: `main` at `4908778`, 12 commits ahead of `origin/main`.
- Authority: `docs/archive/aac-convert-m9.1-feature-parity-audit.md` (M9.1),
  `docs/archive/aac-convert-m9.2-integration-design.md` (M9.2) §5,
  `docs/archive/aac-convert-m9.3-implementation-plan.md` (M9.3) Workstream A.
- Status: **decision record. No implementation authorized by this document.
  No spikes run. No M1 code started.**

This document records the six owner decisions verbatim as given, then binds
them to the exact M9.2 mapping table and edge rules. Wording is deliberate:
"agreed" means owner-decided here; "proposed" remains a proposal.

## 1. Recorded owner decisions

| # | Decision as given | Disposition |
|---|---|---|
| D1 | Approve the proposed core-9 M4A text metadata scope, subject to verification and explicit documentation of its mapping and edge rules before implementation. | **Recorded.** Scope is §2 below. Verification + documentation are M1 acceptance gates (§4 of the M1 checklist), not post-hoc notes. |
| D2 | Include a CLI opt-out flag for M4A metadata writing in M1. | **Recorded.** Concrete semantics fixed in §3 below (`--no-tags`). No other CLI change is approved. |
| D3 | For single-valued M4A atoms, use the first non-empty value when multiple source values are present. Document the rule and test it. | **Recorded.** Overrides M9.2 §5's "write all values in encountered order" for every single-valued text atom. Binding rule in §2. Explicit unit + integration tests required. |
| D4 | Authorize a cover-art technical spike with a 30-minute time box, limited to existing `shiguredo_mp4` API feasibility and a minimal proof of concept. No production feature implementation. | **Recorded.** Charter in §5. Spike not run. No product commitment. |
| D5 | Authorize a 24-bit FLAC technical spike with a 45-minute time box, limited to capability investigation and a precision/quality test plan. No production conversion implementation. | **Recorded.** Charter in §5. Spike not run. Current 24-bit reject policy unchanged. |
| D6 | Keep the deferred features listed in M9.2 and M9.3 unscheduled and outside M1. | **Recorded.** Deferred list restated in §6. None is an M1 prerequisite or acceptance gate. |

## 2. Agreed core-9 mapping and edge rules (D1 + D3)

Source vocabulary is Vorbis comment keys (case-insensitive), as in M9.2 §5.1.
"Single-valued" below means one M4A text value per atom after D3 resolution.
`trkn`/`disk` are tuples and keep their own rule.

| # | Vorbis key(s), case-insensitive | M4A atom | Agreed behavior |
|---|---|---|---|
| 1 | `title` | `©nam` | Text, UTF-8. First non-empty value wins (D3). Missing/empty → absent + warning. |
| 2 | `artist` | `©ART` | Same first-non-empty rule. |
| 3 | `album` | `©alb` | Same first-non-empty rule. |
| 4 | `tracknumber`, fallback `tracktotal` then `totaltracks` | `trkn` | Tuple `(N, M)`. Accept `N` or `N/M` (whitespace tolerated around the slash per M9.3 tuple grammar). Total `M` from slash form, else first parseable fallback key, else `0` (unknown). Unparseable `N` → skip the atom with a per-file warning, never fail. |
| 5 | `discnumber`, fallback `disctotal` then `totaldiscs` | `disk` | Same tuple rule as track. |
| 6 | `albumartist` | `aART` | Text. First non-empty wins. Absence is not a failure. |
| 7 | `date`, then `year` | `©day` | Text. `date` wins when non-empty after trimming; `year` used only if `date` is absent/empty/whitespace. Written at most once — never duplicated. Stored verbatim (no date reformatting in M1). |
| 8 | `genre` | `©gen` | Text. First non-empty wins. Free text; no genre-ID mapping. |
| 9 | `composer` | `©wrt` | Text. First non-empty wins (D3 supersedes M9.2's "joined" note for this atom). |
| 10 | `comment` | `©cmt` | Text. First non-empty wins. Verbatim modulo UTF-8 validity; no other sanitization. |

Conventions (all agreed, carried from M9.2 §5 with the D3 amendment):

- Key matching is ASCII case-insensitive (`TITLE` = `title`).
- Value trimming: leading/trailing whitespace trimmed for the emptiness test;
  the stored value is the trimmed value. Empty/whitespace-only → treated as absent.
- Missing core keys → per-file warning only; conversion succeeds.
- Duplicate atoms from `date`+`year` → written once (`date` wins).
- Non-UTF-8 comment payloads: skip that value with a warning, never fail the
  file. Exact Symphonia string-error type to be pinned at M1 implementation
  (no new dependency); the skip-with-warning behavior itself is agreed.
- Unknown and deferred comments: ignored for output, counted in a verbose-only
  `unmapped` diagnostic, never an error. Deferred set is §6 (unchanged from
  M9.2 §5.2).
- All text atoms written as M4A data type UTF-8. No integer/boolean/freeform
  encoding in M1.
- **D3 scope note:** D3 applies to every single-valued text atom above
  (1–3, 6–10). It does not alter the `trkn`/`disk` tuple rule: `N` comes from
  the number field, `M` from the slash total or fallback keys.

## 3. CLI opt-out semantics (D2)

Agreed flag: `--no-tags`.

- Effect: M4A outputs carry no user metadata atoms (only the existing
  gapless `iTunSMPB` item and `M4A ` brands). ADTS outputs are unaffected
  (they never carry tags, with or without the flag).
- Applicability: valid for single-file M4A and directory-batch M4A inputs.
  With an ADTS output (`--format adts` or ADTS-inferred extension) the flag is
  accepted and is a no-op (not an error), so scripts can pass it unconditionally.
- Combinations: orthogonal to `--format`, `--recursive`/`--no-recursive`,
  `--dry-run`, `--skip-existing`, `-v`/`--verbose`, `-q`/`--quiet`, `--`.
  No new conflict is introduced; the existing `-v`+`-q` usage error is unchanged.
  Batch-only/single-file guards from the current CLI are unchanged: the flag is
  valid in both modes (unlike `--dry-run`/`--skip-existing`, which stay
  directory-only usage errors on file inputs).
- Spelling forms: `--no-tags` only. No `--no-tags=` value form, no short form.
  Unknown spellings (`--no-tag`, `--notags`) remain usage errors (exit 2) under
  the existing unknown-option rule.
- Dry-run: `--dry-run --no-tags` plans exactly as `--dry-run` today (no tag
  reads, no directories created, no audio decoded); the plan is identical with
  or without the flag.
- Reporting: no summary-shape change. With the flag, verbose `tags` lines
  report `0 mapped` (unmapped counting continues verbatim).
- Exit codes: unchanged (`0/1/2/3`). Tag warnings never change the exit code;
  with the flag there are no tag warnings at all.

## 4. Ambiguity review — no blocking ambiguity remains

Checked against M9.2 §5 before declaring M1 authorized:

| Area | M9.2 state | G0 resolution | Blocking? |
|---|---|---|---|
| Core set (which 10 rows) | Proposed table §5.1 | **Agreed as §2** (D1). Includes `albumartist/date/genre/composer/comment` beyond the M7.1 §8.6 floor. | No |
| Multi-value text | "Write all in source order" | **Superseded by D3:** first non-empty wins for all single-valued atoms; documented + tested. | No |
| `composer` "joined" note | Contradicted the general rule | **Resolved:** D3 governs; no joining. | No |
| Track/disc grammar | `N`/`N/M` + fallbacks + skip-with-warning | **Agreed as §2.** Slash whitespace tolerance is the only added precision and matches the M9.3 grammar. | No |
| `date` vs `year` | `date` wins, written once | **Agreed.** No reformatting. | No |
| Empty/whitespace, missing keys | Absent + warning-only | **Agreed.** | No |
| Non-UTF-8 | Skip value + warning | **Agreed** (error-type pinning is M1 detail, not a rule gap). | No |
| Unknown/deferred keys | Ignore + verbose count | **Agreed** (§6 list frozen for M1). | No |
| Opt-out flag | Open (M9.2 O1) | **Agreed as §3** (`--no-tags`, semantics above). | No |
| Write path/timing | After `verify()`, before `finish`; ADTS untouched | **Agreed** for M1 (checklist pins tests). Exact module/error-variant names are M1 implementation detail. | No |

**Conclusion: no blocking mapping or edge-rule ambiguity remains. M1 is
authorized subject to the checklist in `docs/archive/aac-convert-m1-implementation-checklist.md`.**

## 5. Authorized spike charters (D4, D5) — not run

- **S1 cover-art (30-minute box).** Limited to: whether pinned
  `shiguredo_mp4 =2026.5.0` models `covr` explicitly or only via `UnknownBox`;
  minimal JPEG + PNG prototypes against both committed audio fixtures;
  `ffprobe` + zero-stderr full FFmpeg decode + trim-invariance + peak-RSS delta
  observations. Throwaway harness only, outside `rust/convert/tests/`, not
  committed unless a follow-up proposal says otherwise. Ends with
  proceed-to-standalone-first / proceed-to-embedded / stay-with-no-artwork plus
  reasons. No product module, CLI, filesystem copy, or README promise.
- **S2 24-bit (45-minute box).** Limited to: observed Symphonia yield for the
  committed `stereo-48k-24.flac` (type/range/spec); candidate scale-to-`i16`
  law + dither candidate (or justified no-dither rounding); no-clipping/overflow
  and determinism analysis; distance vs truncated baseline; fixture/test list
  that *would* be needed. Ends with keep-reject or convert recommendation.
  No encoder API change, no production policy change; `UnsupportedBitDepth(24)`
  stands.

Neither spike has been run under this record.

## 6. Deferred set (D6) — unscheduled and outside M1

Freeform tags, BPM, compilation and sort metadata; loudness scanning,
normalization, and gain metadata (`REPLAYGAIN_*`, `iTunNORM`); `newer`/`retag`/
`rebuild` policies; TOML configuration; persistent retry state; RAM-disk
support; parallelism; JSON output and signal-handling changes. None is an M1
prerequisite or acceptance gate. Each revival needs its own scoping and approval.

---

*End of G0 decisions. M1 proceeds under the companion checklist; S1/S2 proceed
only within the charters above and have not been run.*
