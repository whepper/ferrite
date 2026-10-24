# S1.3 — Cover-Art Final Specification (Design Only)

> Maintained specification: this is the authoritative cover-art behavior contract for the shipped implementation. S1.1 and S1.2 are archived historical design records.

- Baseline: `main` plus M1/M1.1 and S2 work. No implementation is authorized
  by this document; no production code was changed to produce it.
- Authority: `docs/archive/aac-convert-s1.1-cover-art-plan.md` (architecture, atom
  layout, test strategy) and `docs/archive/aac-convert-s1.2-cover-art-decisions.md`
  (one recommendation per open decision). S1.1 and S1.2 are unchanged;
  where this spec refines or supersedes them, it says so explicitly.
- Status: **approved implementation contract** (S1.3 approval task). Policies
  §§1–2 items 1–11 are adopted as specified below; audit findings F1
  (warning order) and F2 (24-bit + cover-art regression) are closed as
  specified in §§2–3 and §5; T2/T4/T5/T6 are approved as specified in §6.
  T1 (S1.1 historical note) and T3 (`--no-tags` warning suppression) are
  recorded semantics, not open questions.

## 1. Final policy

1. **Maximum image size: 16 MiB per image**, enforced in `meta.rs` cover
   selection **before** atom serialization. `boxed()` / `udta_box_with_tags`
   checked-`u32` arithmetic remains the backstop and fails closed.
   (Approved: 16 MiB.)
2. **Source priority: standard FLAC picture metadata first; legacy
   `COVERART` only as a fallback.** Both arrive as `Visual`s from the pinned
   Symphonia API (top-level `PICTURE` blocks and `METADATA_BLOCK_PICTURE`
   with mapped `usage`; `COVERART` with `usage: None`), so this is a
   selection rule, not a parsing rule. Raw comment strings never surface
   (the parser routes picture comments to `add_visual` only), hence picture
   keys never pollute the `unmapped` count.
3. **Output: one `covr` item holding a single `data` box**, JPEG (`data`
   type 13) or PNG (`data` type 14), locale 0, raw image bytes — the layout
   the S1 spike proved accepted. No multi-`data` galleries.
4. **Candidate tiers, in order; encounter order (`revision.media.visuals`
   emission order) within each tier:**
   1. `usage == Some(FrontCover)`,
   2. other `Some(usage)` (typed non-front, e.g. `BackCover`; includes
      `Other` from picture type `0x00`),
   3. `usage == None` (`COVERART`, picture types `0x11`/`>0x14`),
   then no cover art.
5. **Validate with source fields + magic bytes; never decode images.**
   JPEG = `FF D8` prefix, PNG = 8-byte signature; `media_type` is never
   trusted. Sniffing is an O(1) prefix compare — no image crate, no new
   dependency, no full decode.
6. **Invalid, unsupported, or oversized candidates are skipped with a
   warning; selection continues** to the next candidate in tier order.
7. **Select the first valid candidate in the highest non-empty tier.**
   If none is usable, omit `covr` and continue audio conversion with a
   warning. Cover art never fails a convertible file (pre-existing
   exception unchanged: a corrupt top-level `PICTURE` block fails demuxer
   open before decode starts).
8. **Diagnostics use the existing `TagDiagnostics.warnings` model unchanged.**
   No struct-shape change: `mapped` stays text-atoms-only (0–10), `unmapped`
   untouched, cover outcomes reported as warning strings. Approved strings
   (following the lowercase `warning: <input>: <msg>` convention):
   - `missing cover`
   - `unsupported cover image format: skipped`
   - `empty cover image: skipped`
   - `cover image too large ({n} bytes, limit {m}): skipped`
9. **`--no-tags` suppresses text tags and cover art together**, including
   cover selection and cover warnings (mirrors M1's "warn about nothing").
   ADTS output never carries `covr` (same gate as text tags; byte-identical
   ADTS with or without pictures). Dry runs read nothing.
10. **Document the summary-only API gap; change no signatures.** Add a
    doc note to the three diagnostics-dropping functions —
    `convert_file` ("Convert one FLAC file to one ADTS file…"),
    `convert_file_with_format` (after its "Use
    `convert_file_with_format_and_tags`…" paragraph), and
    `convert_file_m4a_experiment` — stating per-file warnings (tag issues,
    24-bit precision notice, cover-art outcomes) are not returned and
    pointing at `convert_file_with_format_and_tags`. (For `convert_file`,
    which is ADTS-only, the note covers the precision notice; tag/cover
    warnings cannot arise there.)
11. **Text-tag mapping/order, gapless `iTunSMPB`, `M4A` brands, atomic
    hard-link publication, batch semantics, exit codes, and summary shapes
    stay intact.** `covr` is appended last in `ilst_children` (after `©cmt`).

## 2. Selection and fallback algorithm (exact)

```
candidates = revision.media.visuals            # emission order, no extra I/O
for tier in [FrontCover, TypedNonFront, Untyped]:
    for v in candidates in emission order where tier(v):
        if v.data.is_empty():            warn("empty cover image: skipped"); continue
        fmt = sniff(v.data)              # JPEG -> 13, PNG -> 14, else None
        if fmt is None:                  warn("unsupported cover image format: skipped"); continue
        if v.data.len() > 16 MiB:        warn("cover image too large (...): skipped"); continue
        return Covr(fmt, v.data)         # single data box
warn("missing cover")                    # only when --no-tags is off
return None
```

`--no-tags`: skip the entire algorithm (no selection, no warnings), mirroring
M1's `unwrap_or_default` + empty-warnings arm. `mapped_count` excludes the
`covr` atom; `ilst_children` appends it after `©cmt`.

## 3. Warning conditions and diagnostic behavior

| Condition | Warning (approved) | Audio |
|---|---|---|
| No usable picture | `missing cover` | Converts |
| Unknown magic | `unsupported cover image format: skipped` | Converts |
| Empty data | `empty cover image: skipped` | Converts |
| Over 16 MiB | `cover image too large ({n} bytes, limit {m}): skipped` | Converts, tries next |
| `--no-tags` | none | Converts, no `covr` |

One warning per skipped candidate plus at most one terminal notice per file,
emitted once per converted file through the existing stderr loop (single-file
and batch share it because selection runs inside `convert_inner`). No
per-chunk emission is possible by construction. Final warning order in
`TagDiagnostics.warnings` (F1, approved): (1) 24-bit precision-loss notice,
if applicable (the existing `insert(0, …)` in `convert_inner` keeps it first);
(2) text-tag warnings in their current order; (3) cover-art candidate
warnings in selection order. Cover selection appends after tag warnings, so
the precision insert cannot reorder existing messages.

## 4. Expected production files and test coverage

- `rust/convert/src/meta.rs`: `COVER_MAX_BYTES`, `sniff_cover_format()`,
  `select_cover()`, `covr_atom()`, `TagSet.cover` (or equivalent internal
  slot), `ilst_children` position, unit tests (layout 13/14, magic matrix,
  cap boundaries via synthetic vecs, tier/encounter-order selection,
  fall-through, `covr`-last ordering).
- `rust/convert/src/lib.rs`: wire cover bytes into `extra_ilst`; warning
  strings; three doc-comment notes (§1.10); rustdoc disclosure.
- `rust/convert/src/batch.rs`, `rust/convert/src/main.rs`: no logic changes
  expected (warnings flow through existing fields/loops); verbose `tags`
  line shape unchanged.
- Tests: `tests/common/mod.rs` (`mp4_ilst_cover` helper mirroring
  `mp4_ilst_pair`); `tests/metadata.rs` (JPEG/PNG round-trip incl. payload
  bytes, negative unsupported-format case, batch per-file presence,
  `--no-tags` absence with `----`/brands intact); new `tagged-cover-*.flac`
  fixtures (KB-scale, mutagen-built, documented in `fixtures/README.md` —
  **new files only; existing fixtures must not gain pictures**, so current
  exact-vector `ilst` assertions stand unmodified).
- Docs: `rust/convert/README.md` Metadata section (priority, formats, cap,
  `--no-tags`/ADTS table entry).
- Validation: `cargo test --locked -p aac-convert` green, `cargo fmt -p
  aac-convert -- --check`, `cargo clippy` with no new lints, ignored
  ffprobe/mutagen read-backs advisory only, `rust/src/aac_enc` diff empty.

## 5. Acceptance criteria and non-goals

Accept: JPEG+PNG round-trip byte-identical through the real
`meta.rs → extra_ilst → finish` path; tier/fallback/cap matrix green at unit
level; per-file warnings asserted in single-file and batch CLI runs, in the
approved order (precision → tags → cover); a 24-bit FLAC containing cover
art converts to a valid M4A with `covr` present, audio converted, and the
precision-loss notice first (F2); `--no-tags`/ADTS/dry-run/batch/exit-code
behavior unchanged; full suite + fmt + clippy green; docs updated.
Non-goals: multi-image `covr`, GIF/BMP/WebP embedding, re-encoding or
downscaling artwork, new CLI flags, `shiguredo_mp4`/manifest/lockfile/CI
changes, encoder/decoder changes, 24-bit policy changes, new dependencies,
unrelated features.

## 6. Technical objections and owner-confirm items (S1.3 dispositions)

- **T1 (supersession recorded, not a change):** S1.1 §8 says "24-bit FLAC (S2,
  rejection unchanged)". S2 has since landed acceptance with a precision
  warning. This spec assumes the S2 behavior; S1.1 stays unchanged as the
  historical record. **Disposition: recorded, no action.**
- **T2 (`usage == Some(Other)` in tier 2):** picture type `0x00` maps to
  `Other`, hence tier 2 rather than tier 3. Defensible (it is a typed
  intent); alternative is treating `Other` as untyped. **Disposition:
  approved as specified in §1.4.**
- **T3 (`--no-tags` silences cover warnings):** accepted semantics (the flag
  means no user metadata and no metadata warnings), recorded so it is not
  mistaken for an oversight. **Disposition: recorded, no action.**
- **T4 (`mapped` excludes `covr`):** programmatic "was art embedded?" is
  observable only via read-back tests in v1; acceptable, revisit on UI need.
  **Disposition: approved as specified in §1.8.**
- **T5 (encounter order across mixed sources):** deterministic per file
  (demuxer emission order) but not authorially significant across sources;
  tiers contain the arbitrariness. Crafted fixtures pin the implemented
  order. **Disposition: approved as specified in §1.4.**
- **T6 (confirmations):** cap value (16 MiB), the four warning strings, and
  the `COVERART`-fallback risk posture. **Disposition: all approved as
  specified in §§1–3.**
