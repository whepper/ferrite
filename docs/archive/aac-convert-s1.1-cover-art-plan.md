# S1.1 — Cover-Art Implementation Plan (Design Only)

- Baseline: `main` plus M1/M1.1 work (core-9 text metadata, `--no-tags`).
  No production code is changed by this document.
- Authority: G0 decision D4 (`docs/aac-convert-g0-decisions.md`), which
  chartered a 30-minute S1 spike with no product commitment; the completed S1
  spike report (feasibility evidence for the `covr` atom layout).
- Status: **plan only. No implementation started. No owner approvals granted
  by this document.** Every `Proposed` item below needs an owner note before
  it becomes a requirement; `Open decision` items must not be implemented on
  assumption.

## 0. What the spike established (and did not)

- The pinned `shiguredo_mp4 = "=2026.5.0"` has **no typed `covr` or `ilst`
  support** (registry source grep for `covr`/`ilst` is empty). Cover art must
  travel the same raw-bytes path M1 uses: complete ilst child boxes built by a
  `meta.rs` constructor, appended after the `----` item by
  `GaplessAccounting::udta_box_with_tags`, pushed as an `UnknownBox` `udta`
  into `moov.unknown_boxes`, and re-encoded by `moov.encode_to_vec()` in
  `M4aWriter::finish` (`rust/convert/src/m4a.rs:164-172`).
- Atom layout (verified against mutagen 1.48.1 `__render_cover` source):
  `covr` box → one `data` box per image, payload `u32 data-type` + `u32
  locale (0)` + raw image bytes. **Type 13 = JPEG, 14 = PNG.**
- A disposable post-hoc PoC (byte-splice into a real `aac-convert` M4A, code
  in `/tmp`, never in the repo) proved the layout is accepted: mutagen parsed
  `covr` back byte-identical for JPEG and PNG, ffprobe exposed each as an
  attached-picture video stream, audio decoded cleanly, and `moov`-final
  growth left `stco` offsets valid.
- **Caveat (constrains this plan):** the PoC modified an existing M4A
  post hoc; it did **not** exercise a production metadata constructor.
  Acceptance therefore requires end-to-end tests through the real
  `meta.rs` → `extra_ilst` → `finish` path, not byte-surgery.
- **Not tested:** large images of any size. No claim in this plan assumes
  large-image behavior; §3 proposes a cap and tests it with synthetic bytes.

## 1. Scope and architecture

### 1.1 In scope

1. **FLAC picture extraction.** Read attached pictures from the already-probed
   Symphonia metadata revision (`revision.media.visuals`, same probe
   `meta::read_tags` uses today — no extra I/O, no decoder involvement).
2. **JPEG/PNG detection and `covr` serialization.** Magic-bytes sniff
   (JPEG `FF D8`, PNG `89 50 47 0D 0A 1A 0A`) mapping to `data` types 13/14;
   a `covr_atom()` constructor in `meta.rs` mirroring `text_atom`
   (`data`: type + locale 0 + image bytes, wrapped in `boxed(b"covr", …)`).
3. **Integration through the existing `extra_ilst` path.** The selected cover
   becomes one more complete ilst child appended after text atoms in
   `TagSet::ilst_children` order (proposed position: after `©cmt`, last).
   No changes to `gapless.rs` box assembly, `m4a.rs` muxing/splicing, or
   `shiguredo_mp4` usage.
4. **Policy, CLI, diagnostics, docs, and tests** as specified in §§2–5.

### 1.2 Non-goals (explicit)

- No 24-bit FLAC support; the rejection policy is unchanged (S2 owns that).
- No new CLI flags. Cover art rides with text tags (see §4).
- No multi-image galleries, no thumbnail generation, no re-encoding or
  downscaling of images, no GIF/BMP/WebP support beyond skip-with-warning.
- No loudness, RAM-disk, parallelism, TOML, retry, JSON, or signal work.
- No dependency upgrades, no custom MP4 writer, no encoder/decoder changes.

## 2. Source metadata policy

### 2.1 What the pinned Symphonia API exposes (verified, not assumed)

- `symphonia-core 0.6.1`: `MetadataContainer { tags, visuals }`
  (`meta.rs:705-710`); visuals are reached as `revision.media.visuals`.
  `Visual { media_type: Option<String>, dimensions, color_mode,
  usage: Option<StandardVisualKey>, tags, data: Box<[u8]> }`
  (`meta.rs:643-667`).
- `symphonia-bundle-flac 0.6.1` demuxer (`demuxer.rs:124,138-140`):
  - Top-level FLAC `PICTURE` blocks → `read_flac_picture_block` →
    `add_visual`, with `usage` mapped from the numeric picture type
    (`get_visual_key_from_picture_type`: `0x03` → `FrontCover`, unknown
    values → `None`).
  - `VorbisComment` blocks → `read_vorbis_comment`, which routes
    `METADATA_BLOCK_PICTURE` → `ParsedComment::Visual` via
    `read_flac_picture_block` (**usage preserved** from the embedded block)
    and legacy `COVERART` → `ParsedComment::Visual` with **`usage: None`**
    and `media_type` sniffed from the bytes (`parse_base64_cover_art`,
    `embedded/vorbis.rs:170-189`). Both become `builder.add_visual(visual)`.
- **Asymmetry to design around:** a malformed comment-embedded picture is
  warn-and-skip inside Symphonia (`Err(err) => warn!(…)`, never fatal),
  while a corrupt top-level `PICTURE` block propagates `?` at demuxer open
  and fails the conversion before decode (existing behavior; unchanged).

### 2.2 Proposed selection rule (needs owner approval)

`Proposed:` embed at most **one** image per file, chosen by:

1. First visual with `usage == Some(FrontCover)` whose bytes sniff as
   JPEG/PNG and fit the §3 cap.
2. Else the first visual with `usage == None` or any non-cover usage whose
   bytes sniff as JPEG/PNG and fit the cap (covers `COVERART`, untyped
   `METADATA_BLOCK_PICTURE` payloads, and unknown picture types).
3. Else no cover art; record the reason as a warning (see §3).

`Proposed:` `METADATA_BLOCK_PICTURE` and top-level `PICTURE` blocks are
first-class sources. Legacy `COVERART` is supported **only as a fallback
tier** (rule 2), never preferred over a typed front cover.
`Open decision:` whether `COVERART` should be supported at all, or ignored
as a legacy risk (its contents are unauthenticated base64 with no type
field; Symphonia sniffs it, but owner risk appetite governs).

`Open decision:` single-image-only vs. embedding all selected images as
multiple `data` boxes in one `covr` atom (the atom format allows it;
mutagen renders it). This plan proposes single-image-only for reviewability;
a gallery would need ordering and cap-per-file rules.

## 3. Input and failure policy

### 3.1 Proposed maximum size

`Proposed:` **16 MiB per image**, enforced in `meta.rs` cover selection
**before** serialization: over-cap images are skipped with a warning naming
the byte size and the cap. Rationale: peak memory holds the image ~2–3×
(`Visual.data` + ilst child + moov re-encode buffer); 16 MiB keeps worst
case under ~50 MiB per file while covering real-world artwork (typically
< 5 MiB). The existing checked `u32` arithmetic in `boxed()` and
`udta_box_with_tags` remains the backstop and fails closed.
`Open decision:` the exact cap value (candidates: 8 / 16 / 32 MiB).

### 3.2 Format and payload rules (all non-fatal to audio)

| Input | Behavior |
|---|---|
| Magic bytes JPEG / PNG | Embed as `data` type 13 / 14 |
| Magic bytes GIF, BMP, or anything else sniffable-but-unsupported | Skip image, `warning: …: unsupported cover format …` |
| Unsniffable bytes (no known magic) | Skip image, `warning: …: unsupported cover format …` |
| Empty (`data.is_empty()`) | Skip image, `warning: …: empty cover art …` |
| Over §3.1 cap | Skip image, `warning: …: cover art too large …` |
| No visuals at all | Same as a missing text slot: `warning: …: missing cover` (consistent with G0 text-tag warnings) |

`Proposed:` never trust the `media_type` string alone (printable-ASCII,
source-supplied, and `None`/wrong for some paths); always sniff magic bytes.
`Proposed:` never fully decode the image to embed it — magic-byte sniffing
plus the container's own acceptance (proven by the spike for JPEG/PNG) is
sufficient. No image crate, no new dependency.

Image errors never fail a valid-audio conversion, mirroring the M1
missing/invalid-tag contract. The one exception is pre-existing and
unchanged: a corrupt top-level `PICTURE` block that fails demuxer open
(§2.1 asymmetry).

## 4. CLI behavior

- **No new flags.** Cover art follows text tags exactly: written by default
  for M4A output.
- `--no-tags` suppresses **both** text atoms and `covr` (single
  `write_tags == false` gate in `convert_inner`; cover selection is skipped
  and no cover warnings are emitted, mirroring the M1 `--no-tags`
  diagnostics). Unmapped-key counting is unchanged.
- ADTS output never carries cover art (same gate as text tags; byte-identical
  ADTS with or without pictures).
- Dry-run behavior unchanged: dry runs never decode or read tags, so no
  cover is read and plans are identical with/without pictures.
- Batch safety, exit codes, gapless `iTunSMPB`, `M4A` brands, no-overwrite
  publication, and the four-count summary shape are unchanged.
- `Proposed:` extend `TagDiagnostics` minimally with cover status
  (e.g. `cover: CoverStatus { embedded: bool }`; warnings flow through the
  existing per-file `warnings` vec and stderr `warning:` lines). Verbose
  `tags …` lines keep their shape; any cover detail goes on a separate
  verbose-only line so existing assertions stand.
  `Open decision:` exact diagnostics field shape.

## 5. Tests and validation

### 5.1 Unit tests (`rust/convert/src/meta.rs`)

- `covr` layout for JPEG and PNG: child type `covr`, single `data` box,
  type 13/14, locale 0, payload bytes identical to input.
- Magic-byte matrix: JPEG, PNG accepted; GIF89a/GIF87a/BMP/random bytes
  rejected; empty input rejected; truncated magic (1-byte input) rejected.
- Size boundary: exactly-at-cap accepted, one-over-cap rejected (synthetic
  byte vecs with valid magic — no 16 MiB fixture in the repo).
- Selection: front-cover preferred over later untyped image; untyped-only
  set falls back to first sniffable image; all-unsupported set yields
  `None` + warning; over-cap front cover falls through to next candidate
  (propose) vs. stops (open — see below).
- `ilst_children` order: `covr` last, after `©cmt`; text-atom order
  unchanged.

`Open decision:` whether an over-cap front cover falls through to the next
candidate or suppresses cover art for the file.

### 5.2 Integration tests (extend `rust/convert/tests/metadata.rs`)

- New small fixtures (offline mutagen `add_picture`, same process as M1
  fixtures; document in `tests/fixtures/README.md`):
  - `tagged-cover-jpeg-48k-16.flac` (front-cover JPEG),
  - `tagged-cover-png-48k-16.flac` (front-cover PNG),
  - optionally `tagged-cover-unsupported-48k-16.flac` (GIF or BMP picture).
  Keep each fixture a few KB (tiny valid images, as in the spike).
- For each: convert → `ilst` contains `covr` last; `data` type/locale
  asserted via a `common` helper (mirror `mp4_ilst_pair`); payload bytes
  equal the fixture image; all text tags and `iTunSMPB` intact; audio
  payload equals the ADTS reference (existing pattern).
- Negative: unsupported-format fixture converts successfully with `covr`
  absent and a warning present.
- Batch test: directory run over picture fixtures asserts per-file `covr`
  presence; CLI `--no-tags` real-run test asserts `covr` absent while
  `----`/brands survive (extend the M1.1 batch test's `ilst == [----]`
  assertion pattern).
- All tests deterministic; no network; no new dependencies.

### 5.3 Independent validation (optional-tool pattern)

- Reuse the existing `#[ignore]` + `ffprobe -version` gate pattern
  (`ffprobe_reads_core_tags` precedent): when ffprobe is present, assert the
  cover surfaces as an attached-picture stream and the file decodes; when
  mutagen is present (offline dev check, never a dependency), assert
  `MP4['covr']` round-trips. Document both as advisory: required suite must
  pass without them.
- **Required vs. optional boundary:** `cargo test --locked -p aac-convert`
  green (including new unit + integration tests) is required; ignored
  ffprobe/mutagen checks are advisory evidence only.
- Encoder gates unchanged: no encoder/decoder internals are touched, so the
  existing tier-A gate and `cargo package` checks apply as-is; confirm no
  `rust/src/aac_enc` diff in the final status.

## 6. Implementation sequence and acceptance criteria

Small, reviewable steps; each step ends with a green suite:

1. **`meta.rs` selection + constructor + unit tests.**
   Add: `COVER_MAX_BYTES` const, `sniff_cover_format()`, `select_cover()`,
   `covr_atom()`, `TagSet.cover` field + `ilst_children` position +
   `mapped_count` treatment (`Proposed:` cover counts separately from the
   10 text atoms, not inside `mapped`). Files: `rust/convert/src/meta.rs`.
2. **Pipeline wiring + diagnostics.** Extend `read_tags`/`TagSet` to carry
   the selected cover (visuals come from the same revision already read),
   thread bytes through `convert_inner` into `extra_ilst`, extend
   `TagDiagnostics` per §4. Files: `rust/convert/src/meta.rs`,
   `rust/convert/src/lib.rs`, `rust/convert/src/batch.rs`,
   `rust/convert/src/main.rs` (verbose line only). No `m4a.rs`/`gapless.rs`
   logic changes (doc-comment touch-ups only if they mention text atoms).
3. **Fixtures + integration tests + docs.** Files:
   `rust/convert/tests/fixtures/tagged-cover-*.flac` (new),
   `rust/convert/tests/common/mod.rs` (`mp4_ilst_cover` helper),
   `rust/convert/tests/metadata.rs`, `rust/convert/tests/fixtures/README.md`,
   `rust/convert/README.md` (Metadata section: source priority, formats,
   cap, `--no-tags`/ADTS behavior), `docs/archive/aac-convert-m9.2-integration-design.md`
   only if it normatively lists deferred items (check first; prefer no-op).
4. **Validation.** Required: `cargo test --locked -p aac-convert` (full
   suite green), `cargo fmt -p aac-convert -- --check`, `cargo clippy
   --locked -p aac-convert --all-targets` (no new lints). Advisory:
   ignored ffprobe/mutagen read-back tests where tools exist.

Acceptance: all §5 required tests pass; text-tag and gapless assertions
unchanged and green; `--no-tags`/ADTS/dry-run/batch/exit-code behavior
unchanged; final `git status` shows only the files listed above; nothing
staged, committed, or pushed without owner instruction.

## 7. Open decisions (do not implement without owner notes)

1. Exact maximum image size (§3.1: proposed 16 MiB).
2. Legacy `COVERART` support vs. ignore (§2.2).
3. Single-image-only vs. multi-`data` `covr` (§2.2).
4. Over-cap front cover: fall through vs. suppress file cover (§5.1).
5. `TagDiagnostics` cover-status field shape (§4).
6. Whether `COVERART`-sourced or untyped images may ever outrank a typed
   non-front usage (e.g. back cover) — proposed order says no; confirm.

## 8. Explicitly out of this plan

24-bit FLAC (S2, rejection unchanged); re-encoding/downscaling artwork;
format allowlist beyond JPEG/PNG; multi-image UX; any CLI flag; any change
to `shiguredo_mp4` version, manifests, lockfiles, CI, or encoder/decoder
internals; any unrelated deferred feature.
