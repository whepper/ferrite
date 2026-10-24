# S1.2 — Cover-Art Policy Decisions (Recommendations, Not Approvals)

- Baseline: `main` plus M1/M1.1 and S2 work. No implementation is authorized
  by this document; no production code was changed to produce it.
- Authority: `docs/archive/aac-convert-s1.1-cover-art-plan.md` (six open decisions,
  §7) and the completed S1 spike report. This document recommends one choice
  per decision, with evidence. **Recommendations are not approvals.** S1.1
  itself is left unchanged; deltas from its proposals are flagged below.
- Note: S1.1 §8 predates S2 ("24-bit FLAC (S2, rejection unchanged)"). S2
  has since landed acceptance with a per-file precision warning. The only
  interaction is diagnostic: cover outcomes must share the `warnings`
  channel the S2 notice now also uses (see D5), and the S2 audit's
  summary-only-API finding applies equally to cover warnings (see D5).

## Verified API facts (pinned Symphonia 0.6.1, re-checked)

- `MetadataContainer { tags, visuals }`; pictures surface **only** as
  `revision.media.visuals` (`Visual { media_type, dimensions, color_mode,
  usage: Option<StandardVisualKey>, tags, data: Box<[u8]> }`).
- Raw Vorbis strings are **not** exposed for pictures: `METADATA_BLOCK_PICTURE`
  and `COVERART` comments route to `ParsedComment::Visual` →
  `builder.add_visual(visual)` and nothing else (`embedded/vorbis.rs`
  `read_vorbis_comment`). They therefore never appear as tags in
  `meta.rs`'s pairs and never pollute the `unmapped` count. Verified, not
  assumed.
- `usage` mapping (`get_visual_key_from_picture_type`): `0x03` → `FrontCover`;
  `0x00` → `Other`; `0x11` and `> 0x14` → `None`. So `usage == None` means
  exactly: legacy `COVERART` comments, or picture blocks (top-level or
  embedded) with type `0x11`/`> 0x14`.
- `COVERART` additionally arrives with `media_type` sniffed from bytes and
  `usage: None`; `METADATA_BLOCK_PICTURE` preserves the embedded block's
  usage. Malformed comment-embedded pictures are Symphonia warn-and-skip
  (non-fatal); corrupt top-level `PICTURE` blocks fail demuxer open
  (pre-existing, unchanged — audio never starts, so no "valid audio fails"
  violation).
- Current diagnostics (`rust/convert/src/lib.rs:120-128`,
  `rust/convert/src/batch.rs:237-246`, `rust/convert/src/main.rs:372`):
  `TagDiagnostics { mapped: 0–10, unmapped, warnings }`; `mapped` counts
  text atoms only (existing tests assert exact counts and the verbose
  `tags {n} mapped, {m} unmapped` line); `warnings` already carries tag
  warnings **and** the S2 precision notice; single/batch CLI prints each
  warning once per file on stderr.

## D1 — Image size cap: endorse 16 MiB, enforce in selection

**Recommend 16 MiB per image**, enforced in `meta.rs` cover selection
**before** serialization (single choke point: sniff → measure → cap-check →
construct). `boxed()`/`udta_box_with_tags` checked-`u32` arithmetic stays as
the backstop and fails closed, never as the primary enforcement.
Rationale: real-world artwork is typically < 5 MiB; 16 MiB gives headroom
for scans while bounding worst-case resident memory to ~2–3× the image
(`Visual.data` + ilst child + moov re-encode buffer, sequential per-file
processing). 8 MiB risks rejecting legitimate large scans; 32 MiB doubles
worst-case memory for no demonstrated real-world gain.
Boundary tests use synthetic byte vecs (valid magic + lengths
cap−1/cap/cap+1) — no large fixture in the repo, per S1.1 §5.1.
Over-cap images are skipped with a warning naming size and cap; audio
conversion always proceeds.

## D2 — Legacy `COVERART`: support as last-resort fallback

**Recommend supporting it**, strictly as the lowest selection tier (see D3/D6).
Evidence: Symphonia already normalizes `COVERART` into the same `Visual`
shape as every other source (`usage: None`, sniffed `media_type`), so
support costs one branch of existing iteration, not a new pipeline; and
legacy-tagged libraries (pre-`METADATA_BLOCK_PICTURE` taggers) carry art
*only* as `COVERART` — ignoring it silently drops art users expect.
The legacy risk (unauthenticated base64, no type field) is neutralized by
the standing rule to re-sniff magic bytes ourselves and never trust
`media_type`. Precedence when both exist: any eligible standard picture
(typed FrontCover first, then tier order in D6) always beats `COVERART`;
`COVERART` wins only when nothing else is eligible.
*Delta from S1.1:* S1.1 left support-vs-ignore open; this recommends
support, with the risk control made explicit.

## D3 — Multiple images: single-image `covr`, one `data` box

**Recommend embedding exactly one image** (one `data` box in `covr`).
The atom format allows several, and mutagen renders them, but multi-image
needs cross-source ordering rules, per-file cap accounting, and player
behavior that varies (several readers show only the first). Single-image
keeps selection auditable, tests small, and the atom minimal. Deterministic
selection per D6; the winner is the only `data` child. Revisit only with
evidence of real multi-image demand, as a separate proposal.

## D4 — Oversized front cover: fall through, never veto the file

**Recommend falling through** to the next eligible candidate when the
preferred image exceeds the cap. The cap is a per-image safety bound, not a
per-file veto: a file with an oversized front scan plus a valid thumbnail
still has embeddable art, and skipping everything would be the more
surprising behavior. The skip is warned (size + cap named), preserving
auditability. If no candidate remains eligible, emit no `covr` and warn —
audio conversion never fails for image reasons (corrupt top-level `PICTURE`
blocks excepted per the pre-existing asymmetry above).

## D5 — Diagnostics: no struct-shape change; warnings carry outcomes

**Recommend fitting cover outcomes into the existing model with no
`TagDiagnostics` shape change for the initial implementation:**
skip/empty/oversize/missing reasons flow through the existing per-file
`warnings` vec (same stderr `warning: <input>: <msg>` convention as tag and
24-bit warnings); `mapped` stays text-atoms-only (0–10) so every existing
count assertion and the verbose `tags …` line stand unmodified; success needs
no new signal in v1 (absence of a cover warning + presence of `covr` in
read-back tests is the assertion). A separate verbose-only `cover <in>:
embedded|skipped(<reason>)` line may mirror the `unmapped` lines later
without disturbing current assertions — deferred, not required.
Rationale against a new field now: `TagDiagnostics` is `pub` with `pub`
fields, so an additive field is a (minor) API break for exhaustive
constructors and buys little while the only consumer signal is
embedded-vs-not, already observable in tests. Revisit if a UI need emerges.
**S2 interaction:** the S2 audit found summary-only APIs (`convert_file`,
`convert_file_with_format`, `convert_file_m4a_experiment`) discard
`warnings`. Cover warnings will share that fate. **Recommend documenting
the gap now** (one-line doc note on each dropping function pointing at
`convert_file_with_format_and_tags`, in the implementation diff — no
signature changes in this task), rather than deferring a second audit
finding. `--no-tags` suppresses cover selection *and* cover warnings
(consistent with M1: the flag means "no user metadata, no metadata
warnings"); unmapped counting is unaffected (pictures were never tags).

## D6 — Untyped images never outrank typed non-front images

**Recommend strict usage tiers**, refining S1.1 §2.2's flat rule 2:
1. first `FrontCover` (any source) that sniffs JPEG/PNG and fits the cap;
2. else first **typed non-front** (`Some(usage)`, e.g. `BackCover`) in
   encounter order;
3. else first **untyped** (`usage == None`: `COVERART`, type `0x11`/`>0x14`)
   in encounter order;
4. else no `covr`, with a warning.
Rationale: a typed usage carries authorial intent ("this is the back
cover"); an untyped blob carries none, so intent wins ties. This also makes
`COVERART` strictly last-resort among eligible images, closing D2's risk
 symmetrically. Deterministic (encounter order within tiers) and cheap (a
three-pass scan over the already-in-memory `visuals` vec).
*Delta from S1.1:* S1.1's rule 2 pooled untyped and typed non-front in
encounter order; the tier split is the only substantive change, justified
above.

## Compatibility and test implications (checked)

- Existing `ilst_types` exact-vector assertions (e.g. the 10-tag round-trip
  and every `== [----]` case) stand **unmodified**: M1/M1.1 tagged fixtures
  carry no pictures, and cover tests must use *new* `tagged-cover-*.flac`
  files — do not retrofit pictures into existing fixtures.
- `covr` goes last in `ilst_children` (after `©cmt`); text-atom order and
  `mapped` semantics unchanged.
- ADTS path untouched (cover child only enters `extra_ilst` for M4A);
  dry-run reads nothing; gapless/`M4A` brands/publication/exit codes keep
  today's behavior by construction (same `UnknownBox` + moov-final growth
  the spike proved safe).
- Fixtures stay KB-scale (tiny valid JPEG/PNG, as in the spike); cap
  boundaries stay synthetic (no 16 MiB file in the repo).

## Remaining uncertainties requiring owner approval

1. All six recommendations above (none is approved here).
2. Exact warning strings (cover skip/missing notices; follow the existing
   lowercase `warning: <input>: <msg>` convention).
3. Confirmation of 16 MiB (D1) and of the `COVERART`-as-fallback risk
   posture (D2).
4. Whether the deferred verbose `cover …` line (D5) is wanted now or later.
