# M9.2 — FLAC-to-AAC Integration Design

- Date: 2026-10-24 (UTC)
- Baseline: `main` at `4908778` (`test(aac-convert): harden batch resilience coverage`),
  12 commits ahead of `origin/main`, tracked tree clean at design time apart from
  pre-existing untracked `docs/aac-convert-m8.1-batch-design.md`,
  `docs/archive/aac-convert-m8.5-release-readiness.md`, and `docs/archive/aac-convert-m9.1-feature-parity-audit.md`.
- Status: **design-only proposal. No implementation authorized.**
- Authority: `docs/archive/aac-convert-m9.1-feature-parity-audit.md` (M9.1) is the primary
  evidence source. Implementation files were reinspected where cited; section
  numbers below give the verification point.
- Constraints carried forward: M7.1 D1–D10 (M4A-first, native VBR-5 only, no FFmpeg
  production dependency, no RAM disk, sequential batch, no GUI, codec/app
  separation, `i16` encoder interface), M8.1 batch semantics and exit-code contract,
  M8.5 release boundaries. VBR 1–4, encoder algorithm work, and universal-gapless
  claims are out of scope.

All recommendations in this document are **proposals**, not existing behavior and
not approved decisions. Wording is deliberate: "proposed" means designed but
unauthorized; "existing" means verified in the tree at the baseline.

## 1. Summary and scope

This design selects which useful `flac2aac` workflow features the Rust
`aac-convert` application should adopt, and specifies how, without weakening the
verified pipeline.

Proposed initial scope (owner approval required before any code):

1. **Core text metadata only** (§5): a small reviewed Vorbis→M4A table written
   through the existing `moov/udta/meta/ilst` mechanism. No freeform/BPM/
   compilation atoms, no cover art, no loudness tags in the initial slice.
2. **No other workflow expansion in the initial slice:** keep 16-bit-only input,
   CLI-only configuration, `skip` / `--skip-existing` collision policy, no
   retry-state file, no RAM disk, no loudness analysis, no signal handling,
   no parallelism, no new dependencies.

Explicitly deferred with rationale (§4, §11): freeform/BPM/compilation pass-through,
embedded or standalone cover handling beyond a spike, any loudness scanning or
gain tagging, `newer`/`retag`/`rebuild` policies, TOML configuration, persistent
retry state, 24-bit conversion, and the T-bounded-1 automated gate.

What this document does **not** do: redesign traversal, ordering, VBR-5
encoding, M4A/iTunSMPB signalling, structural verification, dry-run,
skip-existing, failure isolation, bounded-memory operation, or hard-link
publication. M9.1 verified these as already implemented; they are restated in
§2 as constraints, not redesigned.

## 2. Verified current architecture and data flow

Verified against the baseline tree (paths relative to repository root):

- **Crate shape.** `rust/convert/` is application crate `aac-convert` 0.1.0,
  edition 2021, MSRV 1.99.1, depending one-way on workspace `aac` (`aac_enc`
  feature), `symphonia` 0.6.1 (`flac` only), pinned `shiguredo_mp4 =2026.5.0`
  (`rust/convert/Cargo.toml:10-14`). No `clap`, `toml`, image, or loudness
  dependency exists today.
- **Single-file pipeline** (`rust/convert/src/lib.rs:convert_inner:336-416`):
  `output::resolve_paths` → `flac::FlacSource::open` → `properties.validate`
  → `EncoderConfig::new(rate, channels, FRAME_LENGTH, Adts)` →
  `encode_chunks` (cap `CHUNK_FRAMES = 8_000` per channel,
  `lib.rs:418-431`) → `source.verify()` (StreamInfo MD5) → encoder flush →
  `sink.finish` → `sink.publish`. Whole-file PCM and whole-file AAC are never
  buffered; the interleaved packet staging vector is bounded by the decoder's
  FLAC block size (`lib.rs:38-46`, `README.md:Memory`).
- **Input contract** (`src/flac.rs:43-132`, `src/lib.rs:251-266`):
  Symphonia probe with `hint "flac"`, `verify = true`; accepted only 16-bit,
  44.1/48 kHz, mono/stereo. `UnsupportedBitDepth/SampleRate/Channels` reject
  before decode; mid-stream spec changes → `DecodeFailed`; present-but-wrong
  MD5 → `DecodeFailed` with no publication; absent/zero MD5 accepted after
  successful decode without a verified-audio claim. `copy_to_vec_interleaved::<i16>`
  (`flac.rs:118`) is the only PCM path; no 24-bit conversion exists.
- **M4A path** (`src/m4a.rs:46-197`, `src/gapless.rs:110-225`):
  encoder still runs in ADTS mode; each `AdtsFrame::raw_payload` plus
  `EncoderConfig::audio_specific_config` (`[0x11,0x90]` stereo-48k,
  `[0x12,0x08]` mono-44.1k) feeds `shiguredo_mp4::Muxer::Mp4FileMuxer`, which
  keeps only per-sample metadata. `finish` rewrites only `moov`: appends the
  `udta > meta(hdlr md…appl) > ilst > ----(mean com.apple.iTunes, name
  iTunSMPB, data UTF-8)` item built by `GaplessAccounting::udta_box`, splices
  the 28-byte Apple-style `M4A ` ftyp, enforces `moov`-is-final. No edit list
  on the default path; `mdhd`/`mvhd`/`tkhd` keep the encoded-stream timeline.
  ADTS payload bytes and M4A `mdat` payload bytes are asserted identical
  (`tests/conversion.rs:m4a_payload_matches_adts_access_units`).
- **Output safety** (`src/output.rs:17-190`): `resolve_paths` canonicalizes
  input, fails closed on any pre-existing destination (`OutputExists`) or
  identity (`SamePath`); missing parent → `OutputDirectoryMissing`. Hidden
  `.aac-convert-<pid>-<seq>.tmp` with `create_new`, flush + `sync_all`, then
  `fs::hard_link(tmp, dest)` — `AlreadyExists` maps to `OutputExists`, other
  I/O to `Publish`. No rename fallback; FAT/exFAT fail safe. `Drop` removes
  the temp file unless published. Verified by sentinel-intact and
  concurrent-publication tests.
- **Batch layer** (`src/batch.rs:314-617`, `src/main.rs:253-471`):
  input file-type dispatch (file → single-file, directory → batch);
  byte-sorted discovery (`sort_key`), recursive default with
  `--no-recursive` opt-out (last flag wins), dir symlinks never descended,
  file symlinks converted at link path, hidden entries traversed, non-FLAC
  tallied `ignored`. Mapping preserves stem byte-for-byte, emits canonical
  `.m4a`/`.aac` only. Preflight aborts on missing/unreadable/empty/no-FLAC/
  duplicate (exact + ASCII-fold)/same-root/output-root-create before any
  conversion. `execute_plan` runs sequentially, per-file failure isolation,
  dry-run creates nothing and decodes nothing, `--skip-existing` skips without
  decoding (advisory pre-check, atomic hard-link remains enforcement).
  Streams: per-file lines on stdout, `error:` diagnostics on stderr, always a
  final `summary:`/`dry-run:` line; `-v/--verbose`, `-q/--quiet` (conflict =
  usage error). Exits `0/1/2/3` per M8.1 §7.2.
- **Scope limits** (`rust/convert/README.md:Scope limits`): no metadata, cover
  art, ReplayGain, resampling, downmixing, VBR 1–4, parallel conversion; no
  universal gapless claim; ADTS is the engineering format.

## 3. Goals and non-goals

### Goals

1. Specify the smallest metadata-capable increment that preserves every §2
   guarantee (ordering, atomicity, bounded memory, exit codes, VBR-5 scope).
2. Resolve each M9.1 §11 question with evidence, options, and a testable
   recommendation, marking owner-approval gates explicitly.
3. Keep the design implementable without new runtime dependencies unless a
   spike proves one necessary.
4. Define acceptance criteria that fail loudly on overclaim (unknown tags,
   artwork, loudness, gapless).

### Non-goals (not authorized by this design)

- VBR 1–4 selection, encoder tuning, `i32`/float PCM API expansion, resampling,
  downmixing, >2 channels, additional sample rates, raw/LATM/LOAS/ADIF output.
- FFmpeg, rsgain, or any subprocess in the production path.
- RAM-disk or other working-directory indirection.
- Parallel workers, async runtime, thread pools for conversion.
- GUI, saved settings, pause/cancel, per-album tables.
- Universal gapless-playback claims; edit-list default output.
- Licensing or public-distribution clearance (M8.5 P0 items remain with the
  legal reviewer; this design claims none).

## 4. Decision table for every unresolved question

`Proposal` = recommended default in this design (still requires owner approval
marked in the last column). `Existing` = verified behavior, not a proposal.

| # | Question (M9.1 §11) | Evidence | Options considered | Proposed default and rationale | Compat / complexity / testing | Owner approval? |
|---|---|---|---|---|---|---|
| Q1 | Core tag set: which Vorbis keys constitute MVP? | M7.1 §3.1/§3.3 requires *core* preservation with a reviewed table (U4); M7.1 §8.6 floor is title/artist/album/track/disc; `flac2aac metadata.py:TAG_MAPPING` treats ~20 atoms as core; Rust writes none (`m4a.rs`, `gapless.rs`) | (a) Full 20-atom port. (b) Minimal 5-atom floor (title/artist/album/track/disc). (c) Proposed core-9 + albumartist/date/genre/composer/comment (§5). (d) No metadata. | **Proposal (c): core-9 table in §5.** Covers real-library usability (albumartist for compilations, date for catalog, genre/composer/comment for search) while staying inside plain-text atoms with no tuple/integer/boolean special cases beyond track/disc. (a) overclaims freeform-adjacent semantics; (b) drops fields users sort on; (d) leaves M7.1 MVP unmet. | Compat: additive `ilst` children; old outputs remain valid. Complexity: one new small module + `finish` parameter; no encoder change. Testing: per-tag round-trip + unknown-tag + tuple-edge tests (§10). | **Yes — mapping review is M7.1 U4 gate.** No code before sign-off. |
| Q2 | Freeform/BPM/compilation scope | `flac2aac TAG_FREEFORM/INTEGER/BOOL_MAPPING`: MusicBrainz IDs, ISRC, label, catalog, barcode, `tmpo`, `cpil`, sort fields; M7.1 §7 notes "core text tags *or* any freeform tags" as open, cover out | (a) Port all freeform/integer/boolean. (b) Proposed: defer all; document as unsupported (§5). (c) Port `tmpo`/`cpil` only. | **Proposal (b): defer.** Integer (`tmpo` float→int rounding), boolean (`cpil` truthy set), and freeform namespace (`----:com.apple.iTunes:<name>`) each need value-semantics review and player-acceptance evidence that does not exist. Shipping text core first bounds the review. | Compat: deferred atoms absent, not wrong. Complexity avoided: no int/bool/freeform encoders now. Testing: assert absence + document list. | **Yes (bundled with Q1 review).** |
| Q3 | Cover-art policy | M7.1 §4 out of scope; `flac2aac CoverManager`: 16-name search, fallback extract, resize/LANCZOS, JPEG quality, verbatim-copy fast path; Rust: no `covr`, no cover files; `M4aWriter` owns `TempOutput` + `unknown_boxes` push path (`m4a.rs:161-165`) | (a) Embedded `covr` in `ilst`. (b) Standalone cover copy per album. (c) Proposed: no artwork in initial slice; run a spike against the existing MP4 API first. | **Proposal (c).** Rationale: `covr` layout (data-type JPEG/PNG, size bounds, multi-image behavior) and standalone ownership (search order, fallback naming, resize policy) are both unreviewed, and artwork bytes threaten the bounded-memory story (§6). A spike must prove `shiguredo_mp4` can carry `covr` without breaking §2 playback/duration evidence before any commitment. | Compat: no container change now. Complexity: spike-only (no prod dep). Testing: spike acceptance in §10, not product tests. | **Yes — artwork needs a separate decision after the spike.** |
| Q4 | Loudness policy | M7.1 §4 out of scope; M9.1 F6: `flac2aac` writes ReplayGain 2.0 + `iTunNORM` via rsgain subprocess; Rust has only gapless `iTunSMPB` (`gapless.rs`), which is **not** loudness | (a) Native R128 analysis + tags. (b) Tag-only passthrough. (c) Proposed: no loudness analysis, normalization, gain changes, or tag writing in initial scope. | **Proposal (c).** Provisional preference and M7.1 §4 agree: loudness is post-MVP. (a) needs algorithm + dependency review; (b) still needs value-semantics + album-scope design. Document that `iTunSMPB ≠ iTunNORM` to prevent conflation. | Compat: files play at encoded gain; no silent level shift. Complexity: zero. Testing: assert no `REPLAYGAIN_*`/`iTunNORM` atoms. | **Yes — any loudness work needs a new decision.** |
| Q5 | `newer` rebuild semantics | M8.1 §10 Q7 deferred; Rust: existing = failure, `--skip-existing` = no-decode skip (`batch.rs:519-601`); `flac2aac scanner.should_process`: mtime-compare `newer` | (a) mtime-only `newer`. (b) mtime + size/hash. (c) Proposed: defer `newer`; keep `skip` + `--skip-existing`. | **Proposal (c).** No audit evidence of a recurring `newer` workflow that `skip-existing` reruns do not already cover idempotently; (a) inherits clock-skew/false-negative hazards, (b) adds hashing cost and policy surface. | Compat: reruns stay explicit (exit 1 without flag, exit 0 all-skipped with flag). Complexity avoided. Testing: existing skip matrix unchanged. | **Yes to adopt `newer` later; no approval needed to defer.** |
| Q6 | `retag` / `rebuild` | `flac2aac`: `retag` refreshes metadata/loudness without re-encode, `rebuild` re-encodes and replaces; Rust: no replace by design (`output.rs:publish` hard-link, no rename fallback) | (a) Port both. (b) Proposed: defer both. | **Proposal (b).** Both contradict the no-replace guarantee or require its redesign; `retag` is meaningless with no tag layer, `rebuild` needs a replace semantic the project deliberately refuses. | Compat: no destructive flag added. | **Yes to adopt later; no approval needed to defer.** |
| Q7 | TOML vs CLI-only | M7.1 U7, M8.1 §10 Q7 deferred; Rust: hand parser, no TOML dep (`main.rs:97-201`, `Cargo.toml`); `flac2aac config.py:load_config` 5 sections + unknown-key errors + bounds | (a) Versioned TOML. (b) Proposed: CLI-only for initial slice. | **Proposal (b).** Provisional preference holds: no recurring multi-profile workflow is demonstrated that CLI flags + shell cannot express; TOML adds schema, versioning, path-canonicalization, and error-surface work. | Compat: CLI contract additive-only going forward. Complexity avoided. Testing: CLI matrix unchanged. | **Yes to adopt TOML later; no approval needed to defer.** |
| Q8 | Persistent retry state | `flac2aac run_state.py`: `.flac2aac-state.json` + `failed_sources` + `--retry-failed`; Rust: stateless re-plan, `--skip-existing` + exit codes cover reruns | (a) Port state file. (b) Proposed: defer. | **Proposal (b).** No compelling need demonstrated: batch reruns are already deterministic and idempotent; a state file adds location, concurrency, corruption-recovery, and privacy surface. | Compat: filesystem remains source of truth. | **Yes to adopt later; no approval needed to defer.** |
| Q9 | 24-bit policy | M7.1 U5/D10: deliberate policy required, no encoder API expansion; Rust rejects `bits_per_sample != 16` before decode (`lib.rs:259-263`); `stereo-48k-24.flac` fixture pins reject; Symphonia 24-bit decode path unverified in-tree | (a) Keep reject. (b) Scale + TPDF dither to `i16`. (c) Silent truncation (forbidden). | **Proposal (a) for initial slice; (b) as a separately scoped research spike.** Rationale: (c) is incorrect by M7.1; (b) needs verified decoder output semantics (Symphonia 24-bit → what integer width?), dither/noise proof, no-clipping/overflow tests, and 24-bit fixtures — none exist. Do not assume `copy_to_vec_interleaved::<i16>` semantics for 24-bit sources. | Compat: 24-bit stays a clear error naming the policy. Complexity avoided now. Testing: existing reject tests stand; spike defines new fixtures/gates. | **Yes — any conversion policy needs owner + quality review.** |
| Q10 | T-bounded-1 ceiling | M8.1 §9.1 T-bounded-1 specified (flat peak RSS, linear time) but no automated gate committed; M8.5 P2 lists it outstanding; `CHUNK_FRAMES=8000` + streaming muxer bound per-file heap (`lib.rs`, `m4a.rs`) | (a) Adopt M8.1's `<25%` RSS-growth gate now. (b) Proposed: measurement-first baseline, gate as follow-up. | **Proposal (b).** The `<25%` figure is a generous anti-regression heuristic, not a measured ceiling for this tree; enshrining it without a 30-min-equivalent baseline risks a flaky or vacuous gate. Record the procedure in §10 and gate after one measured run. | Compat: none. Testing: manual procedure now, automated gate later. | **No approval needed; engineering follow-up.** |
| Q11 | RAM disk / working directory | M7.1 D5, M9.1 §8: destination-filesystem temp + streaming muxer sufficient; `flac2aac work_dir` motives (multi-rewrite NAS round-trips, half-finished album hiding) do not apply to the single-pass pipeline | (a) Add `work_dir`. (b) Proposed: do not add. | **Proposal (b).** No measured benefit; cross-filesystem staging would break the same-filesystem hard-link guarantee. Revisit only with benchmarks after a multi-rewrite feature lands. | Compat: atomicity preserved. | **No approval needed to defer; approval required to add.** |
| Q12 | Preflight / machine-readable summary / signal handling | M8.1 §7 fatal-setup subset exists; full `preflight.py` capacity/album/tool layer absent; no JSON summary; Ctrl-C = OS default (M8.1 §6.4) | (a) Port full preflight + JSON + finish-current-file. (b) Proposed: defer as separable operability items. | **Proposal (b)**, ordered only if later wanted: capacity check → JSON summary → finish-current-file. Each is additive and independently testable; none blocks metadata. | Compat: exit-code contract stays additive. | **No approval needed to defer; each item needs scoping if revived.** |

## 5. Proposed metadata mapping table, including unsupported or ambiguous tags

This section is a **proposal for review (M7.1 U4)**. It is not implemented and
not approved. Field names below use `flac2aac`'s Vorbis keys as the reference
vocabulary (M9.1 §3.2) so the reviewer can compare directly; the final column
states the proposal.

### 5.1 Proposed core table (initial slice)

Conventions (proposed):

- Vorbis keys matched **case-insensitively**; values are UTF-8 strings as
  decoded from FLAC comments. First non-empty value wins per atom
  (mirrors `flac2aac` `written`-set behavior for `date`→`year`).
- `tracknumber`/`discnumber` accept `N` or `N/M`; total falls back to
  `tracktotal`/`totaltracks` and `disctotal`/`totaldiscs` respectively when the
  slash form is absent; unparseable values are skipped with a per-file warning,
  never a failure. Zero totals are written as `0` (unknown), matching
  `flac2aac` `_lookup_total` semantics.
- `date` (full `YYYY[-MM-DD]`) takes precedence over `year` for `©day`;
  both map to the same atom and must not duplicate it.
- All text atoms written as M4A data type UTF-8. No integer/boolean/freeform
  encoding in this slice.

| # | Vorbis key(s) (case-insensitive) | M4A atom | Type in this proposal | Notes |
|---|---|---|---|---|
| 1 | `title` | `©nam` | text | Required floor (M7.1 §8.6). |
| 2 | `artist` | `©ART` | text | Required floor. |
| 3 | `album` | `©alb` | text | Required floor. |
| 4 | `tracknumber` (+ fallback `tracktotal`, `totaltracks`) | `trkn` | track tuple `(N, M)` | Required floor; see tuple rule above. |
| 5 | `discnumber` (+ fallback `disctotal`, `totaldiscs`) | `disk` | disc tuple `(N, M)` | Required with track for multi-disc correctness. |
| 6 | `albumartist` | `aART` | text | Needed for compilation grouping; absence must not fail. |
| 7 | `date`, then `year` | `©day` | text | `date` wins; `year` only if `date` absent. |
| 8 | `genre` | `©gen` | text | Free-text genre; no genre-ID mapping in this slice. |
| 9 | `composer` | `©wrt` | text | Single text atom; multi-value joined per §5.3. |
| 10 | `comment` | `©cmt` | text | Verbatim; no sanitization beyond UTF-8 validity. |

### 5.2 Explicitly deferred or unsupported in the initial slice

Documented, not silently dropped (M7.1 metadata-overclaim mitigation):

| Vorbis key(s) | M4A target in `flac2aac` | Proposal |
|---|---|---|
| `titlesort`, `artistsort`, `albumsort`, `albumartistsort`, `composersort` (`sonm`/`soar`/`soal`/`soaa`/`soco`) | sort atoms | **Deferred.** Needs sort-display semantics review; absence does not affect playback. |
| `bpm` (`tmpo`) | integer atom | **Deferred (unsupported).** Float→int rounding and empty-field semantics unreviewed. |
| `compilation` (`cpil`) | boolean atom | **Deferred (unsupported).** Truthy-set (`1/true/yes/on`) semantics unreviewed. |
| `grouping` (`©grp`), `lyrics` (`©lyr`), `copyright` (`cprt`), `encoder` (`©too`) | text atoms | **Deferred.** Low MVP value; `encoder` tag in particular must not claim encoder identity without review. |
| `musicbrainz_*`, `isrc`, `label`, `catalognumber`, `barcode`, `encoded_by`, `encoding`, `media`, `asin`, `ean` (freeform `----:com.apple.iTunes:<name>`) | freeform atoms | **Deferred (unsupported).** Namespace, multi-value, and player-acceptance review outstanding (Q2). |
| Any other Vorbis key | — | **Unsupported in this slice:** ignored for output, counted in a per-file `unmapped_tags` diagnostic (see §5.3), never an error. |

Ambiguous inputs (proposed handling):

- Duplicate atoms from `date`+`year`: write once (`date` wins).
- Multi-valued text keys: write all values in encountered order (no dedup, no
  reordering); player display order is then source order.
- Empty/whitespace-only values: treated as absent.
- Non-UTF-8 comment payloads: Symphonia already surfaces valid strings; any
  conversion failure for a single value skips that value with a warning, never
  fails the file. (Exact Symphonia string-error type to be pinned by the
  implementation spike; no new dependency.)
- Missing core keys: warning only (mirrors `flac2aac validate_album` spirit
  without album grouping); conversion still succeeds.

### 5.3 Proposed write path and diagnostics

- **Where.** New small module (proposed name `src/meta.rs`, `pub(crate)`)
  exposing `read_flac_comments(&Path) -> TagSet` and `ilst_children(&TagSet)
  -> Vec<UnknownBox-child-bytes>`. `M4aWriter::finish` gains a
  `tags: Option<&TagSet>` parameter (default path passes `Some` for M4A,
  `None` preserves today's ADTS behavior of writing nothing). The existing
  `udta > meta > ilst` builder in `gapless.rs:udta_box` is extended to append
  one `data`-carrying child per mapped atom alongside the existing `----`
  `iTunSMPB` item — same `boxed()` helper, same checked-size arithmetic, same
  `moov`-is-final enforcement. `mdat`, sample tables, `mdhd` duration, and
  ftyp-brand behavior are unchanged.
- **When.** Tags are read after `source.verify()` succeeds and before
  `sink.finish`, so corrupt/tagless inputs keep today's failure semantics and
  tag errors can never resurrect a failed decode. Tag-read I/O errors map to a
  new `ConvertError::Metadata(&'static str)`-family variant (proposed; exact
  name at implementation) counted under the existing per-file `Failed` tally
  and `category_for` string, preserving the exit-code contract.
- **Diagnostics.** Batch `ok` lines unchanged. Proposed additions, all behind
  `--verbose` except the summary: per-file `tags <rel>: <n> mapped, <m>
  unmapped` verbose line; `unmapped <rel>: <key>` verbose lines mirroring
  today's `ignore` lines; summary gains no new counter in the initial slice
  (avoids breaking parsers; unmapped counts are verbose-only).
- **ADTS.** No tags are written on the ADTS path (engineering format unchanged).

## 6. Artwork and loudness handling

### 6.1 Artwork (proposal: spike first, no product commitment)

- **Investigation required before commitment** (provisional preference).
  The existing MP4 API plausibly carries artwork: `finish` already appends
  arbitrary children to `moov.unknown_boxes` and re-encodes `moov`
  (`m4a.rs:161-169`); a `covr` data box inside the same `ilst` would reuse the
  `boxed()` path. What is **not** established: whether `shiguredo_mp4`
  2026.5.0 models `covr` explicitly or only as `UnknownBox`, accepted image
  types/sizes, multi-image behavior, CoreAudio/FFmpeg acceptance of Rust-built
  `covr`, and peak-memory behavior for large embedded images (a 10 MB cover
  buffered per file breaks the §2 memory story unless streamed or bounded).
- **Proposed spike acceptance (no product code):** build one M4A with a small
  JPEG `covr` and one with PNG via `UnknownBox`, verify `ffprobe` codec +
  full `ffmpeg -v error -i … -f null -` decode with zero stderr, verify
  `afinfo`/`afconvert` trim behavior unchanged from §2 (delay 1600 honored,
  duration fields untouched), and record peak RSS delta. Failure is a valid
  outcome: the spike may recommend staying with no artwork.
- **If the spike passes, proposed order:** standalone cover-file copy first
  (filesystem copy alongside outputs; no container change, no decode-risk),
  embedded `covr` second. Standalone needs its own policy spike: search order,
  fallback naming, no-resize default (verbatim copy to avoid generation loss),
  existing-output interaction (`skip` vs `--skip-existing` applies to covers
  too), and non-image-file handling. Neither phase is in the initial slice.

### 6.2 Loudness (proposal: none in initial scope)

- No ReplayGain 2.0 analysis, no `rsgain` or substitute subprocess, no
  `REPLAYGAIN_*` tags, no `iTunNORM` (SoundCheck) tag, no reference-loudness
  configuration, no `reuse_existing_replaygain` fast path, no album-scoped
  second pass in the initial slice.
- Rationale: M7.1 §4 places loudness post-MVP; the provisional preference
  forbids automatic scanning/normalization/gain changes initially; a native
  R128 implementation or a tag-only passthrough both need algorithm, library,
  licensing, and audibility review that does not exist. `iTunSMPB` gapless
  signalling stays exactly as verified; documentation must state that it is
  not a loudness tag.
- Future design hook (not proposed now): if loudness is ever adopted, it
  requires album grouping (`_group_by_parent` analog), full-final-album
  analysis including retained outputs (the `flac2aac` partial-rerun
  correctness property), and a stated stance on whether tags or audio levels
  change. That design does not exist here.

## 7. Supported input formats and bit-depth policy

- **Proposed initial policy (no change):** 16-bit FLAC, 44.1/48 kHz,
  mono/stereo only. Reject 24-bit with `UnsupportedBitDepth(24)`, other rates
  with `UnsupportedSampleRate`, 3+ channels with `UnsupportedChannels`, no
  resampling, no downmixing, no upmixing (`flac.rs:validate`,
  `lib.rs:251-266`). Existing fixtures (`stereo-48k-24.flac`,
  `mono-32k-16.flac`, `three-channel-48k-16.flac`) and reject tests stand.
- **Why no 24-bit conversion now.** The encoder's public PCM interface is
  `i16` (M7.1 D10; `encoder.rs:push(&[i16])`); M9.1 verified that 24-bit
  rejection happens before any PCM conversion, so no Symphonia 24-bit decode
  semantics are pinned in-tree. A conversion policy must establish from
  verified capabilities: what integer width Symphonia yields for 24-bit FLAC
  in this configuration, the exact scale-to-`i16` law, the dither algorithm
  (e.g., TPDF with stated bit depth and shaping or explicit no-dither rounding
  with justification), no-clipping/overflow proof, and fixtures covering
  full-scale positive/negative, silence, and fade material. None of this
  exists; assuming `copy_to_vec_interleaved::<i16>` handles 24-bit correctly
  would be the exact silent-truncation error M7.1 forbids.
- **Proposed research spike (separate from initial slice):** decode the
  committed `stereo-48k-24.flac` through Symphonia in a throwaway harness,
  record yielded sample type/range, prototype scale + TPDF, measure objective
  distance vs truncated baseline, and return a keep-reject-or-convert
  recommendation with fixtures. No production API change is proposed here.

## 8. CLI/configuration behavior and compatibility

- **Proposed: CLI-only stays.** No `config.toml`, no `--config`, no `--workers`,
  `--log-level`, `--existing-mode`, `--no-verify-output`, `--retry-failed`
  equivalents in the initial slice. Rationale per Q7: no recurring workflow in
  evidence justifies schema/versioning cost; M8.1 deferred TOML without
  dissent.
- **Compatibility rules for any future addition (binding on this design's
  successors):**
  1. Existing flags keep their meaning; `--format`, `--recursive/--no-recursive`
     (last wins), `--dry-run`, `--skip-existing`, `-v/--quiet` (conflict =
     usage error 2), `--`, `-h/--help`, `-V/--version` are frozen
     (`main.rs:97-201`).
  2. New output-affecting flags default to current behavior (metadata on by
     default once adopted needs an explicit `--no-tags`-style opt-out design;
     that opt-out is **not** proposed now — caller must decide it with Q1).
  3. Exit codes stay additive: `0` success, `1` per-file failure, `2` usage,
     `3` fatal setup. Tag warnings never change the exit code; tag errors are
     per-file failures (`1`).
  4. Batch-only flags remain usage errors on single-file inputs (current
     `run_single_file` guard), and vice versa.
- **What would justify TOML later:** three or more recurring named profiles
  (e.g., portable vs archival tag sets) requested with evidence, or a fleet
  deployment needing versioned fleet-wide defaults. The trigger is demonstrated
  recurrence, not symmetry with `flac2aac`.

## 9. Output collision, rebuild, and failure semantics

- **Unchanged foundation.** Preflight duplicate detection (exact + ASCII-fold),
  `SameRoot` refusal, per-file `OutputExists` vs `--skip-existing` skip,
  advisory pre-check with atomic hard-link enforcement, `Drop` cleanup, sentinel
  preservation, and race semantics all stand (`batch.rs:find_duplicate`,
  `plan_batch`, `execute_plan`; `output.rs:publish`). No `--force`, no rename
  fallback, no destructive replace.
- **Proposed tag-error semantics (initial slice):** tag read/encode failures
  are per-file `Failed` with a new stable `category_for` string (e.g.,
  `Metadata`), counted in `failed` (not `existing`, not `ignored`), details on
  stderr, siblings continue, no destination and no temp orphan left behind.
  Dry-run never reads tags (mirrors today's no-decode rule); a file whose
  destination exists dry-runs as `OutputExists`/`Skipped` exactly as today.
- **Deferred rebuild policies.** `newer` (mtime or mtime+hash), `retag`, and
  `rebuild` remain deferred per Q5–Q6. If revived, `retag` must be specified
  against the tag layer designed in §5 (meaningless before it), and any
  `rebuild` must confront the no-replace guarantee head-on with a new
  atomic-replace design and its own approval — it is not a flag rename.
- **Cover/loudness interaction (for future designs, not now):** any adopted
  cover or loudness pass must state whether it runs before publication (inside
  the temp file, preferred — preserves atomicity) or rewrites published
  outputs (forbidden without a new safety case).

## 10. Testing and acceptance criteria

Conventions: reuse `tests/common/mod.rs` (`TempDir`, `temp_output_files`,
`parse_adts`, `parse_mp4` + new `mp4_ilst` helper). Every test asserts empty
temp scans and sentinel preservation on collision paths, as today.

### 10.1 Initial-slice acceptance (metadata core-9)

- **T-tags-1 (unit).** Tuple parser: `5`, `5/12`, `05/12`, missing total with
  `tracktotal` fallback, unparseable (`x`, `5/x`, empty) → skip-with-warning;
  `date` beats `year`; case-insensitive keys; whitespace-only = absent.
- **T-tags-2 (structure).** Convert both committed fixtures to M4A with a
  synthetic tagged FLAC copy (test harness writes Vorbis comments via a
  fixture generator, not by editing committed fixtures): `moov/udta/meta/ilst`
  contains exactly the mapped children + the existing `----/iTunSMPB` item;
  `edts` absent; `ftyp` brands `M4A `; `mdhd` = `N*1024`; `mdat` byte-identical
  to the untagged conversion of the same audio (tags must not alter AAC
  payloads — the `m4a_payload_matches_adts_access_units` invariant extended).
- **T-tags-3 (round-trip).** `ffprobe -show_entries format_tags` (or mutagen
  read in test harness — implementation choice, out-of-tree validator only)
  shows title/artist/album/track/disc values; full `ffmpeg -v error -i … -f
  null -` decode clean with zero stderr (ignored-test pattern, as
  `independent_decode.rs`).
- **T-tags-4 (negative).** Unknown/freeform/BPM/compilation keys present in
  source → absent in output, verbose `unmapped` lines present, exit 0, summary
  shape unchanged (no new counter). Corrupt comment block → per-file `Failed`
  with `Metadata` category, exit 1, no destination, no orphan.
- **T-tags-5 (regression).** Full existing suite green unmodified in behavior:
  `cargo test --locked -p aac-convert`, Tier A + doctests per M8.5 §9,
  byte-identity batch-vs-single checks, race tests, MD5 mismatch/truncated
  tests. ADTS outputs contain no tags.
- **T-artspike-1 (spike, not product).** JPEG + PNG `covr` prototypes decode
  clean under FFmpeg with zero stderr, `afinfo` trim unchanged, RSS delta
  recorded; recommendation to proceed or not with reasons.
- **T-24spike-1 (spike, not product).** Symphonia 24-bit yield recorded,
  scale+TPDF prototype measured, no-clipping proof, recommendation with
  fixtures.

### 10.2 Explicitly not gated now

- No `newer`/`retag`/`rebuild`, TOML, retry-state, loudness, cover-product,
  JSON-summary, signal-handling, or T-bounded-1 automated gates. Manual
  T-bounded-1 procedure: convert a 30-minute-equivalent synthetic FLAC,
  record peak RSS and wall time per file, confirm flat growth and linear time;
  automate only after one measured baseline is recorded.

## 11. Phased implementation outline

Phases are sequential; each needs its owner gate before the next begins. No
phase changes the encoder, transport, delay, or VBR-5 scope.

- **Phase 0 — Decisions (this design + owner review).** Approve, amend, or
  reject Q1–Q12. Required output: signed core table (§5), explicit artwork /
  loudness / 24-bit / TOML / retry-state dispositions. No code.
- **Phase 1 — Initial slice (metadata core-9 only).** New `src/meta.rs`
  (read + `ilst` children), `M4aWriter::finish` tag parameter, `ConvertError`
  tag variant + `category_for` entry, verbose diagnostics, T-tags-1–5. No new
  runtime dependency (prove it; if Symphonia comment access or `shiguredo_mp4`
  atom modelling forces one, stop and re-scope). No CLI flag changes except
  possibly a documented `--no-tags` opt-out **only if** Q1 review demands it.
- **Phase 2 — Spikes (parallelizable, no product code).** Artwork `covr`
  spike (T-artspike-1) and 24-bit conversion spike (T-24spike-1). Each returns
  a proceed/defer recommendation with measurements; neither lands product
  behavior by itself.
- **Phase 3 — Operability (only if evidence demands).** In order:
  machine-readable summary opt-in, persistent retry-state design, preflight
  capacity/album-warning report, finish-current-file signal behavior. Each
  scoped, tested, and exit-code-compatible independently.
- **Explicitly not scheduled:** VBR 1–4, FFmpeg/subprocess production use,
  RAM-disk `work_dir`, parallel conversion, GUI, `newer`/`retag`/`rebuild`,
  TOML, loudness, cover product work, universal-gapless claims.

## 12. Risks, open questions, and owner decisions

| # | Risk / question | Impact if unresolved | Proposed handling | Owner decision required |
|---|---|---|---|---|
| R1 | Metadata scope creep (freeform/BPM/sort/cover bundled into "tags") | Unbounded review, container risk | Enforce §5 split: core-9 now, all else deferred + documented | **Yes — §5 table sign-off (M7.1 U4).** |
| R2 | Artwork assumed shippable without a spike | Broken players, memory regression | Spike-first per §6.1; no commitment | **Yes — after spike.** |
| R3 | Loudness conflated with gapless (`iTunSMPB` vs `iTunNORM`) | Wrong tags, unexpected level shifts | §6.2: no loudness; doc the distinction | **Yes — any loudness work.** |
| R4 | 24-bit "just scale it" assumption | Clipping, noise, silent data loss | Q9: reject now, research separately | **Yes — conversion policy.** |
| R5 | `moov` growth breaking the `moov`-is-final invariant | Publish failure on tagged files | §5.3 reuses checked `boxed()` sizing + `set_len`; T-tags-2 asserts tiling | Engineering (covered by tests). |
| R6 | Tag warnings promoted to failures (or vice versa) by mistake | CI breakage or silent data loss | Rule: unknown/missing = warning, undecodable comment block = per-file failure; exit-code contract frozen | Engineering (assert in T-tags-4). |
| R7 | TOML/retry/`newer` reintroduced by symmetry argument | Scope sprawl | Q7/Q8/Q5: recurrence-evidence trigger only | **Yes — each revival.** |
| R8 | VBR-5 / validation drift via "small" encoder-adjacent changes | Release-gate breach | No encoder, transport, delay, or flush change in any phase; M8.5 §9 gates re-run | Engineering (CI). |
| O1 | Should `--no-tags` exist? | Flag surface | Only if Q1 review demands an opt-out; otherwise tags are the documented default | **Owner, with Q1.** |
| O2 | Multi-value text join vs first-only? | Display order | Proposed: all values in source order (§5) | **Owner, with Q1.** |
| O3 | Cover search-order/fallback if standalone ever adopted? | User surprise | Deferred to artwork decision | **Owner, later.** |

## 13. Traceability to M9.1 findings

| M9.1 item | Disposition in this design |
|---|---|
| F4 core tags (`missing`, MVP-required) | §5 core-9 proposal; Q1 owner gate; Phase 1. |
| F5 freeform/BPM/compilation/sort (`decision`) | §5.2 deferred + documented; Q2 owner gate. |
| F6–F7 loudness + reuse fast path (`decision`) | §6.2 none in initial scope; Q4 owner gate. |
| F8–F9 cover embedded/standalone (`decision`) | §6.1 spike-first, no product commitment; Q3 owner gate; Phase 2. |
| F10 work_dir/RAM disk (`unnecessary`) | §4 Q11, §11 Phase list: do not add; benchmark trigger only. |
| F13 TOML (`decision`) | §8, Q7: CLI-only; recurrence trigger. |
| F15 `newer`/`retag`/`rebuild` (`partial`) | §9, Q5–Q6: keep `skip` + `--skip-existing`; defer rest. |
| F16 verification (`partial`) | Unchanged; T-tags-3 extends independent-decode pattern. No `verify_output` port. |
| F17 retry state (`decision`) | Q8: defer; filesystem stays source of truth. |
| F18 album grouping (`decision`) | Needed only if loudness/cover adopted; not designed here. |
| F19 metadata warnings (`decision`) | §5 verbose warnings; no album-table port. |
| F20–F22 progress/logging/preflight (`partial`/`decision`) | Unchanged CLI contract; operability deferred per Q12. |
| F23/F25/F27/F28 traversal/safety (`supported`) | Restated as constraints (§2); no redesign. |
| F26 symlink semantics (`partial`) | No change; two-links-convert-independently stands. |
| F29 naming (`partial`) | No `.mp4`/`.adts` batch extensions; tags do not alter naming. |
| F30 exit codes (`partial`) | Frozen `0/1/2/3`; tag errors = `1`, warnings = `0`. |
| F31 timeout (`decision`) | No subprocess timeout to port; bounded-decode argument belongs to a future spike, not this design. |
| F32–F33 input/integrity (`supported`) | §7 keeps 16-bit-only + MD5; 24-bit research separated (Q9). |
| F1–F3/F24 VBR/FFmpeg/rsgain/GUI (`supported` scope / `unnecessary`) | Non-goals (§3); no production subprocesses. |
| M9.1 §8 temp storage | Adopted: no working directory (§4 Q11). |
| M9.1 §9 missing tests | §10 converts tag/artwork/24-bit gaps into named criteria; non-tag gaps stay manual until baselined. |
| M9.1 §10 P0–P7 | P0→Q1/§5/Phase 0–1; P2→Q9/Phase 2; P3–P7→Q5–Q8/Q12/Phase 3 or deferred. VBR 1–4/FFmpeg/RAM-disk exclusions honored. |
| M9.1 §11 Q1–Q10 | Answered in §4 Q1–Q12 (Q11–Q12 added for RAM-disk/operability completeness). |

No gapless-playback universality is claimed: default M4A carries
`iTunSMPB`-only signalling with Apple brands, verified on FFmpeg 9.0.2 and
macOS CoreAudio decode paths with VLC/GUI limitations as documented in the
crate README. No licensing or distribution clearance is claimed; M8.5 P0 items
remain with the legal reviewer.

---

*End of M9.2 design. No implementation authorized. Next step is owner review of
§4–§5 decisions (Q1–Q4, Q9) before any Phase 1 work is scoped.*
