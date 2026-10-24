# L2 — Loudness Integration Design (Design Only)

> Maintained guidance: finalized loudness design decisions are in §10. The implementation has shipped; provisional sections and the original design-only banner describe the historical design process.

- Baseline: `main` @ `7ebf208d40d78adcd265e3736e93f8a8aa93e8d1` (converter
  M1/M1.1/S2/S1.3 committed locally, not pushed).
- Authority: `docs/aac-convert-l0-loudness-decisions.md` (L0 D1–D7, frozen and
  authoritative; the amended D2 fixes the v1 reference at **−18.0 LUFS**),
  the L1 technical spike report, `docs/archive/aac-convert-m9.1-feature-parity-audit.md`,
  `docs/archive/aac-convert-m9.2-integration-design.md`,
  `docs/archive/aac-convert-m9.3-implementation-plan.md`, `docs/aac-convert-g0-decisions.md`,
  `docs/aac-convert-m7.1-architecture-decisions.md`,
  `docs/aac-convert-m8.1-batch-design.md`.
- Status: **design only. No implementation, dependency addition, experiment,
  build, or test is authorized by this document. L3 requires separate review
  and explicit approval.**
- Frozen inputs: both ReplayGain 2.0 + `iTunNORM`; fixed **−18.0 LUFS**
  (matches the audited FLAC2AAC default; the superseded −14.0 LUFS choice is
  historical only, not an active requirement); native R128 direction with
  `ebur128` 0.1.10 as the leading (unapproved) candidate; source-parent
  album; full-final-album inclusion; no v1 retag; no new CLI flag; no
  production FFmpeg/`rsgain`; no encoder changes; dry-run/ADTS/`--no-tags`
  rules per L0 D7.

Where L0 conflicts with an older report, L0 governs. The only such conflict
affecting this design is the reference level: L0 D2 as amended fixes
**−18.0 LUFS**; any −14.0 LUFS text in earlier planning (including the first
draft of this document) is superseded history.

## 1. Executive recommendation

Design a **two-stage batch pipeline with a loudness pre-pass and a single
publication**, reusing the existing per-file conversion path unchanged:

1. **Group** the byte-sorted batch plan by source-parent directory (album key
   = parent of the input-relative path; root-level files form album `""`).
   Grouping is a view only — plan order, per-file reporting order, and the
   summary shape do not change.
2. **Pre-pass (analysis only, no temps, no publication):** for each album,
   stream-decode every member's **source FLAC PCM** through R128 analyzers
   (one per-track instance plus one album instance fed the same PCM;
   sequential files, bounded memory). Retained members under
   `--skip-existing` are read from their **source FLACs for analysis only**.
   Skipped in full for dry-run, `--no-tags`, and ADTS runs.
3. **Conversion pass:** the existing
   `resolve_paths → open → validate → encode_chunks → verify() → flush →
   finish(gapless, tag_ilst) → publish` path runs unchanged per file, except
   the `tag_ilst` for M4A additionally carries precomputed loudness atoms
   (freeform `----` children appended after text atoms and `covr`).
   One publication per file; no post-publish rewrite, ever.

Recommendations with limits:

- **Analysis domain: source FLAC PCM** (not decoded retained AAC). Rationale:
  reuses the verified Symphonia path, needs no AAC decoder, stays bounded and
  deterministic, and gives retained members a read path without touching
  outputs. Limitation (stated honestly): the original FLAC2AAC workflow
  analyzes **final M4A files**, so source-domain values differ by bounded
  lossy-coding noise; §8 specifies the confirmatory differential experiment
  with measurable track-gain, album-gain, and sample-peak acceptance
  criteria. Source-domain analysis is **not** claimed to be exact behavior
  parity. If the experiment fails the criteria, L3 stops and requests owner
  review — the domain is never switched silently (owner-approved D1).
- **Gain policy (owner-approved D2, evidence-backed): measured gains at
  −18.0 LUFS, measured sample peaks verbatim, then Easy-mode-equivalent
  positive-only clipping protection.** Gains are `−18.0 − integrated`
  (track and album independently); peaks are measured values and are never
  modified. Positive gains whose `peak·10^(gain/20)` exceeds full scale are
  reduced by the excess in dB, never below `+0.00`; negative and zero gains
  pass through unadjusted. AAC sample data is never modified. True-peak
  analysis and any other clipping enhancement stay out of v1 unless
  separately approved (§5).
- **Dependency: `ebur128` 0.1.10 as the L3 vetting candidate** (§6 gate);
  `broadcast-loudness` is the named fallback. Nothing is added by this design.
- **Album-incompleteness rule (owner-approved D4):** album gain is written
  only when **every** album member is analyzable. If any member cannot be
  analyzed, valid track-level atoms are still written for the tracks whose
  measurements are available, but **album-level gain metadata is omitted for
  that album** — never a subset-computed album gain presented as complete —
  with a clear album-level warning explaining the omission (see §§5–7).

## 2. Architecture and processing sequence

Current verified pipeline (unchanged): `output::resolve_paths` →
`flac::FlacSource::open` → `properties.validate` → `EncoderConfig::new` →
bounded `encode_chunks` (`CHUNK_FRAMES = 8_000`) → `source.verify()` (MD5) →
encoder flush → `sink.finish(gapless, tag_ilst)` (`moov`-is-final rewrite:
`iTunSMPB` `----` item + `extra_ilst` children, `M4A` brand splice) →
`TempOutput::publish` (hard-link, no-replace, `Drop` cleanup). Tags are read
after `verify()`, before `finish`; ADTS never reads tags; the 24-bit notice
is `insert(0, …)` ahead of tag then cover warnings; `--no-tags` M4A arm
reads nothing tag-related and warns nothing.

Loudness integration (batch layer only; single-file path unchanged):

```text
plan_batch (unchanged: discovery, byte-sort, mapping, preflight)
  → group entries by album key (parent of relative_input)
  → IF m4a AND NOT dry_run AND write_tags:
       FOR each album (any order; results keyed, reporting stays plan-ordered):
         FOR each member entry (converted + retained-skipped):
           stream-decode source FLAC PCM in CHUNK_FRAMES-style pushes
           feed per-track EbuR128 instance AND album EbuR128 instance
           record track integrated LUFS + track sample peak
         album_integrated = album instance loudness_global (HISTOGRAM mode)
         album_peak = max(track sample peaks)
         apply positive-only clipping protection (D2): measured gains >
           +0.00 with peak·10^(gain/20) > 1.0 reduced by the excess,
           track vs track peak and album vs album peak independently
         IF every member analyzable: render per-member atoms
           (track gain/peak, album gain/peak, iTunNORM) + warnings
         ELSE (D4): track-level atoms for measured tracks only;
           album-level gain atoms omitted + album-level warning
     ELSE: no analysis, no reads beyond planning
  → conversion pass (existing execute_plan order):
       convert_inner gains ONE private input: precomputed loudness children
       (empty vec when inapplicable); public signatures unchanged
       tag_ilst order: iTunSMPB item → text atoms → covr → loudness atoms
       finish → publish (single publication; no second phase)
```

Single-file conversion (`convert_file_with_format_and_tags`) is the
degenerate one-member album: analyze the single source, then convert. No
behavior change for ADTS (analysis skipped), dry-run (planning only), or
`--no-tags` (gapless-only `ilst`).

Why pre-pass before any publication: album gain is unknowable until all
members are analyzed, and the no-replace guarantee forbids post-publish
rewrites (M9.2 §9, M9.3 §§8/10). The pre-pass reads only (no temps, no
outputs), so dry-run/advisory-check and atomicity properties hold by
construction. Peak extra memory: two analyzer instances alive per file
during its analysis (track + album), sequential files — O(1) beyond the
existing per-file peak.

## 3. Decision table

| # | Decision | Evidence | Recommendation | Unresolved / limits | Owner approval? |
|---|---|---|---|---|---|
| A | Analysis domain: source FLAC PCM vs decoded retained AAC (owner-approved D1) | Original workflow analyzes **final M4As** (`process_album(m4a_files)`, `rsgain easy <album_dir>`; L1 source fetch). Rust has a verified Symphonia FLAC path and **no** AAC decoder; L0 D5 requires retained-member inclusion; constraints forbid new decode machinery without review | **Source FLAC PCM** (reused decode path, same chunk loop; retained members read from retained *sources*). The difference from final-M4A analysis is documented as a design difference, **not** exact behavior parity | Conditional gate before L3 (§8 E-domain): matched fixtures, measurable track-gain, album-gain, and sample-peak difference criteria. If the experiment fails, stop and request owner review — never switch domains silently | **Approved (conditional)**: domain selected; experiment + criteria must pass before L3 |
| B | Gain and clipping at −18 LUFS (owner-approved D2) | Verified `rsgain` source (`scan.cpp::calculate_loudness`, `tag.cpp` formats): Easy-mode positive-only (`-c p`) protection exactly as invoked by the original workflow (`rsgain easy <album_dir>`, no flags); measured `gain = target − integrated`, measured sample peaks; track and album gains each reduced only when `peak·10^(gain/20) > max_peak` (max = full scale), by `20·log10(new_peak/max)`, capped at the gain itself (adjusted ≥ +0.00 in mode p); peaks never modified; silence → gain `0.00 dB`, peak `0.000000`. L0 D2 fixed −18; D7–D8: audio untouched, recoverable issues warn | **Evidence-backed parity policy: measured gains and peaks as observed; positive-only clipping protection applied independently to track gain (vs track peak) and album gain (vs album peak); adjusted gains written; `iTunNORM` derived from the adjusted track gain.** `gain_track = −18.0 − integrated_track`, `gain_album = −18.0 − integrated_album`; measured peaks, clipping-adjusted gains, and encoded audio are three distinct things. True-peak analysis and any other clipping enhancement stay out of v1 unless separately approved. Silence/unmeasurable → omit atoms + warning (§7; deliberate difference from `rsgain`, which writes `0.00 dB` / `0.000000`) | Rounding: Python `round()` (banker's) in the audited `iTunNORM` path vs Rust/C++ half-away — exact-half float cases may differ by 1 LSB; L3 pins one half-way vector and records any divergence as a finding. Peak rendering `"%.6f"` and gain `"{:.2f} dB"` verified in `rsgain` `tag.cpp` | **Approved**: evidence-backed policy above replaces the unconditional no-protection rule |
| C | Dependency `ebur128` 0.1.10 | L1: MIT, rust-version 1.60, pure-Rust default (`cc` optional/build-only), `i16` streaming, both peak modes, `loudness_global_multiple` + `seed_frames_*`, 3341/3342 pass claim, Send+Sync; bus factor small; true-peak version-unstable | **Vet (not adopt) `ebur128 =0.1.10` via the §6 gate.** C API/`cargo-c` not used; default features only unless the gate says otherwise | README pass claim is single-source until the gate re-verifies; `dasp_*`/`smallvec`/`bitflags-1` transitive audit, MSRV build, HISTOGRAM accuracy all pending. Fallback: `broadcast-loudness` re-evaluation | Gate execution is L3-prep engineering; **any manifest addition needs the L3 approval itself** |
| D | RG/`iTunNORM` representation (owner-approved D3) | L1 + §4 source verification: 4 uppercase freeform keys, `"±X.XX dB"` gain strings, `MP4FreeForm` UTF-8, `iTunNORM` 10-field layout/slots/clamps/fillers, gain-parse rules, 4-casing read keys, −30…0 bounds, both-enabled/reuse-off defaults | Retain the four RG fields (track gain/peak, album gain/peak) + the source-derived `iTunNORM` formula as the candidate spec in §4, labeled **provisional until independently verified vectors pass**; deterministic atom ordering coexisting with text/cover/`iTunSMPB`; no compatibility claim until vectors + output checks pass | Data-box type for loudness `data` children (candidate: type 1 UTF-8 mirroring `text_atom`/iTunSMPB — unverified); peak string format beyond gains (gain parsing verified in source; peak *rendering* must be observed, not assumed); atom order candidate in §4 | **Approved**: candidate scope retained; formula/serialization stay provisional |
| E | Partially analyzable albums (owner-approved D4) | L0 D4 (parent = album, discs separate) + D5 (all members incl. retained); M8.1 per-file isolation + byte-sorted reporting; `batch.rs` advisory-skip + hard-link enforcement; L0 recoverable-warn rule | Group by parent rel-path bytes; analyze converted + retained sources; conversion continues where the existing failure contract allows; valid track-level atoms written for measured tracks; **if any member cannot be analyzed, album-level gain metadata is omitted for that album** (never a subset result presented as complete) with a clear album-level warning; retained outputs never rewritten; single publication | Missing/invalid retained member ⇒ album incomplete ⇒ album atoms omitted (track atoms for measured members still written); the pre-pass analysis read is not a conversion and adds no summary line | **Approved**: omit-with-warning (strict), not subset-with-warning |
| F | Failure/CLI compatibility (owner-approved D5) | L0 D7–D9 + G0 §3 + M9.2 §8/M9.3 §9 + S1.3 warning order (precision → tags → cover) | §7 matrix: preserve exits, summary shape, public API signatures; prefer existing warning/reporting mechanisms for per-track and album-level diagnostics; append loudness after cover warnings | No new stable failure category unless source inspection demonstrates it is necessary; any API/summary expansion is a separate owner-approval request, not part of this design | **Approved**: contract preserved; expansion needs its own approval |
| G | Test/benchmark sufficiency | L1 §5 vectors; S2.1 differential precedent; M8.5 gates | §8 matrix: required deterministic CI + advisory interop; analytical estimates now, measured baselines in L3-prep | Thresholds (e.g. overhead budgets) are set from the first measured baseline, not invented here | Engineering (budgets recorded, not gated, until one baseline exists) |

## 4. Provisional metadata and formula specification

All of §4 is **provisional evidence, not proof of compatibility**. The source
behavior below was verified by direct read of the original
`whepper/flac2aac` `loudness.py` and `config.py` (fetched raw 2026-10-24; key
excerpts quoted). L3 must still independently validate every byte it writes.

### 4.1 Verified source behavior (original workflow)

- Reference: `_RG2_REFERENCE_LUFS = -18.0`; `LoudnessConfig` defaults
  `enable_replaygain=true`, `enable_itunes_soundcheck=true`,
  `reference_loudness=-18.0`, `reuse_existing_replaygain=false`, with
  validation bounds `REFERENCE_LUFS_MIN/MAX = −30.0/0.0`.
- ReplayGain tags are produced by `rsgain easy <album_dir>` (timeout
  `max(120, n·60)s); every file is then verified to carry track gain
  (`_has_replaygain`), else the album raises `LoudnessError`.
  Docstring: “rsgain targets −18 LUFS (ReplayGain 2.0 standard).
  `reference_loudness` only shifts the iTunNORM value, not these tags.”
- Read keys (`_REPLAYGAIN_KEYS`, 4 casings for older files):
  `----:com.apple.iTunes:REPLAYGAIN_TRACK_GAIN`,
  `----:com.apple.iTunes:replaygain_track_gain`,
  `REPLAYGAIN_TRACK_GAIN`, `replaygain_track_gain`.
- Reuse fast path (`_SOURCE_REPLAYGAIN_ATOMS`, L0-out-of-scope for v1 but
  recorded for completeness): lowercase Vorbis
  `replaygain_{track_gain,track_peak,album_gain,album_peak}` →
  uppercase freeform `----:com.apple.iTunes:REPLAYGAIN_{TRACK_GAIN,
  TRACK_PEAK,ALBUM_GAIN,ALBUM_PEAK}` as UTF-8 `MP4FreeForm`; requires every
  source to carry at least `replaygain_track_gain`, else full `rsgain` pass.
- `iTunNORM` is derived **per track from the track gain only**
  (`_add_itunes_soundcheck` reads each file’s RG track gain, converts, writes
  `----:com.apple.iTunes:iTunNORM` as UTF-8 `MP4FreeForm`); a file with no
  usable track gain is a per-file failure entry, and the album raises if any
  file fails.
- Gain string format (parse side, verified): `"±X.XX dB"` (example `"-7.23
  dB"` in source); parsing strips `dB`/whitespace and `float()`s the rest,
  warning on failure. Peak *rendering* format is **not** established by this
  source read and must be observed from real tagged files (see §4.3).

### 4.2 Candidate ReplayGain mapping for v1 (PROVISIONAL)

| RG concept | Candidate M4A `----` name (`com.apple.iTunes` mean) | Candidate value |
|---|---|---|
| Track gain | `REPLAYGAIN_TRACK_GAIN` | `"±X.XX dB"` (2 decimals, UTF-8 freeform) |
| Track peak | `REPLAYGAIN_TRACK_PEAK` | peak rendering **to be observed** (linear 0…1 string expected; format pinned by byte inspection, never assumed) |
| Album gain | `REPLAYGAIN_ALBUM_GAIN` | `"±X.XX dB"` |
| Album peak | `REPLAYGAIN_ALBUM_PEAK` | same observation requirement as track peak |
| SoundCheck | `iTunNORM` | §4.3 formula string |

### 4.3 Candidate `iTunNORM` conversion (PROVISIONAL)

Verified source (`_replaygain_to_soundcheck(gain_db,
reference_loudness=-18.0)`), quoted exactly:

```python
adjusted_gain = gain_db + (reference_loudness - _RG2_REFERENCE_LUFS)
ratio = 10 ** (-adjusted_gain / 10.0)
sc_1000 = max(0, min(int(round(ratio * 1000)), 0xFFFFFFFE))
sc_2500 = max(0, min(int(round(ratio * 2500)), 0xFFFFFFFE))
return (
    f" {sc_1000:08X} {sc_1000:08X}"
    f" {sc_2500:08X} {sc_2500:08X}"
    f" 00024CA8 00024CA8 00007FFF 00007FFF 00024CA8 00024CA8"
)
```

Verified layout facts: leading space; ten uppercase 8-hex fields separated
by single spaces; slots 1–2 duplicated (`sc_1000`), slots 3–4 duplicated
(`sc_2500`); slots 5–10 are the fixed filler sequence
`00024CA8 00024CA8 00007FFF 00007FFF 00024CA8 00024CA8`; per-slot 32-bit
clamp at `0xFFFFFFFE`. Docstring claims this matches Mp3tag/foobar2000 —
that claim is **unverified** and L3 must test it rather than trust it.

Reference semantics (do not conflate the two formats): **ReplayGain tags and
`iTunNORM` do not contain equivalent values.** RG tags encode the measured
gain against the fixed −18 LUFS reference. `iTunNORM` encodes a *scaled
acoustic-power ratio* derived from the gain plus the selected reference
(`adjusted_gain`). At the approved v1 reference of −18.0 LUFS,
`adjusted_gain == gain_db` (shift is zero); the shift term exists only so the
same function can target other references. The v1 implementation always
passes the approved −18.0 reference — no configurability (L0 D2). The
`gain_db` input is the clipping-adjusted track gain (§5): `iTunNORM` reflects
exactly what is written to the RG track-gain tag, matching the original
workflow (which reads back the written file tags).

Formatting, rounding, clamping (verified source behavior; L3 pins identical
behavior in Rust): banker's rounding is Python `round()` (round-half-even)
on the scaled float, then C-style truncation toward zero by `int()`; Rust
must reproduce the *validated* mapping, not blindly copy Python semantics —
any half-way or float-precision divergence found during validation is a
finding that L3 records and resolves with owner-visible vectors. Missing
measurements are never formatted (atoms omitted, §7); invalid/non-finite
inputs are never serialized.

### 4.4 Worked vectors (derived from the verified formula; L3 must confirm by
running them — not claimed as measured)

With the approved reference (−18.0), `adjusted_gain = gain_db`:

- Zero gain: `gain_db = 0.00` → `adjusted = 0.0` → `ratio = 1.0` →
  `sc_1000 = 1000` (`000003E8`), `sc_2500 = 2500` (`000009C4`).
  Full string: `" 000003E8 000003E8 000009C4 000009C4 00024CA8 00024CA8
  00007FFF 00007FFF 00024CA8 00024CA8"`.
- Nonzero gain (source’s own example magnitude): `gain_db = −7.23` →
  `adjusted = −7.23` → `ratio = 10^0.723 ≈ 5.28445` →
  `sc_1000 = 5284` (`000014A4`), `sc_2500 = 13211` (`0000339B`).
  (`iTunNORM` always derives from the clipping-adjusted track gain; for
  unadjusted gains the adjusted value equals the measured gain.)
  (Arithmetic rechecked by the L3 vector test itself; the test computes from
  the formula, never copies this text.)
- Clamp boundaries: extreme positive `adjusted_gain` → `ratio → 0` →
  `00000000` slots; extreme negative `adjusted_gain` → `ratio` huge →
  `FFFFFFFE` slots (never `FFFFFFFF`).
- Silence: no blocks clear the −70 LUFS gate → integrated undefined → omit
  all loudness atoms for that track + warning (never `±inf dB`, never an
  audio failure).
- Invalid/non-finite: omit atoms + warning; audio converts.

Serialization: each loudness item is a complete `----` child
(`mean com.apple.iTunes` + `name <KEY>` + one UTF-8 `data` box, candidate
data type 1 mirroring `text_atom`/iTunSMPB — **to be confirmed by byte
inspection of real `rsgain`-tagged M4As**), built with the shared `boxed()`
checked-size path (fail closed on overflow, same as text/cover). `ilst`
order (deterministic): existing `----/iTunSMPB` item first, then text atoms
in G0 table order, then `covr` (S1.3: last), then loudness atoms in the
fixed order track-gain, track-peak, album-gain, album-peak, `iTunNORM`.
`mdat`, sample tables, `mdhd`, brands, and `moov`-is-final enforcement are
untouched; audio payloads are byte-identical with or without loudness.

## 5. Analysis-domain and clipping-policy recommendations

**Domain: source FLAC PCM.** Effects: track/album integrated values and
sample peaks are measured on the lossless source, so retained members are
handled by reading retained *sources* (no output inspection, no AAC decoder,
no new dependency); CPU is one extra streaming pass over the same chunk loop
(§8 measures it); determinism follows from fixed decode + fixed analyzer
version; memory stays at the existing per-file peak plus two small analyzer
states. The cost is honesty: this is **not** exact behavior parity with the
original workflow (final-M4A analysis), and the two can differ by bounded
lossy-coding noise. The §8 E-domain experiment quantifies that difference
before L3 code; if it exceeds the S2.1-style control bound, L3 stops and the
owner chooses (no silent domain switch).

**Clipping: Easy-mode-equivalent positive-only protection (verified parity
behavior).** For each track: `measured_gain = −18.0 − integrated_track`;
if `measured_gain > 0.0` and `track_peak · 10^(measured_gain/20)` exceeds
full scale (1.0), the written track gain is reduced by
`20·log10(new_peak)` — equivalently, to exactly `−20·log10(track_peak)` —
and never below `+0.00` (the mode-`p` cap, which binds only if a peak
exceeds 1.0). Negative and zero gains pass through unadjusted. The same rule
applies independently to the album gain against the album peak, and the one
resulting album adjustment (if any) is applied uniformly to every track's
album-gain atom. Measured peaks are recorded verbatim and never modified;
AAC sample data is never modified — only the gain numbers written to tags
change. `iTunNORM` is derived from the clipping-adjusted track gain (the
original workflow reads back the written file tags, so adjustment flows
through automatically; v1 threads the adjusted value directly).
Silence (no blocks clear the −70 LUFS gate): v1 omits loudness atoms with a
warning and never synthesizes a gain — a deliberate, documented difference
from `rsgain`, which writes `0.00 dB` / `0.000000` for silent tracks.
Gain strings keep the verified `"{:.2f} dB"` format (so `+` is
representable); peak strings the verified `"%.6f"` format; no value is
clamped except inside the `iTunNORM` slot arithmetic verified in §4.3.
Whenever an adjustment applies, v1 emits a per-file warning naming the
measured and written gains (mirroring `rsgain`'s own `tclip`/`aclip`
reporting); the adjustment never changes the exit code.

**Album incompleteness (owner-approved D4).** Conversion continues wherever
the existing failure contract allows it. Valid track-level atoms are written
for every track whose measurements are available. If any album member cannot
be analyzed — corrupt/undecodable source, silence/undefined integrated value
on a member, invalid measurement, or a retained member whose source cannot
be read — **album-level gain atoms are omitted for that whole album** rather
than publishing a subset-computed album gain, with a clear album-level
warning explaining that album loudness metadata was omitted because analysis
was incomplete. A subset result is never presented as a complete album
result. Genuine source/conversion failures remain genuine failures (§7).

**Retained outputs under `--skip-existing`.** A retained member participates
in album completeness through its *source*: if the retained member's source
FLAC reads and analyzes successfully, the album is complete with respect to
that member and its retained output is left byte-identical (never rewritten).
If the source is missing or unreadable, that member counts as unanalyzable:
track atoms follow the per-track rules above for converted members, album
atoms are omitted for the album, and the album-level warning names the
missing member. The analysis read is not a conversion and adds no summary
line; skip reporting itself is unchanged.

## 6. Dependency acceptance checklist (`ebur128` 0.1.10; L3-prep only, not run here)

1. Pin `ebur128 =0.1.10` (exact, mirroring `shiguredo_mp4` practice);
   record crates.io checksum + docs.rs source identity + GitHub tag in the
   L3 note.
2. `cargo tree -p aac-convert -e normal` (and with candidate features):
   enumerate `bitflags/dasp_*/smallvec` (+ optional `cc`) and all transitive
   crates; confirm no build-script C requirement on the default path.
3. License audit of the full closure against `THIRD-PARTY-NOTICES.md`
   practice (MIT + Apache/MIT/BSD/Zlib inventory; flag anything else).
4. MSRV build at repo toolchain 1.99.1 (candidate declares 1.60 — confirm,
   don’t assume) incl. `cargo build --locked` clean.
5. C-toolchain independence: default-feature build with no `cc` on PATH.
6. EBU vectors: run the crate’s `reference-tests` feature set plus L3’s
   independent §8 calibration vectors; record pass/fail per Tech 3341/3342
   case — any README-claim mismatch is a finding, not a waiver.
7. HISTOGRAM accuracy: integrated agreement (HISTOGRAM vs. full-history)
   within a pre-declared tolerance on long synthetic + album fixtures;
   bounded-RSS confirmation on the 30-min equivalent.
8. API semantics: streaming `i16` interleaved at 44.1/48 kHz mono/stereo,
   channel map defaults, `I | SAMPLE_PEAK [+ HISTOGRAM]` sufficiency,
   `loudness_global_multiple`/energy-merge album semantics,
   silence/non-finite behavior, determinism across repeats.
9. Fallback trigger: if any of 1–8 fails, evaluate `broadcast-loudness`
   against the same checklist (explicit partial-3341 matrix + `broadcast-common`
   transitive audit) before any alternative proposal. No third candidate
   without a new spike note.

## 7. Failure / CLI compatibility matrix

Warning order in `TagDiagnostics.warnings` (extends S1.3 F1): 24-bit precision
notice (existing `insert(0, …)`) → text-tag warnings → cover warnings →
**loudness warnings** (per-track warnings in plan order, including
clipping-adjustment notices, then at most one
album-level notice per album, emitted on the conversion-pass stderr loop for
each file concerned). `--no-tags` suppresses loudness analysis, atoms, and
warnings entirely (unmapped counting unaffected — pictures were never tags
and loudness reads no tags).

| Situation | Analysis? | Atoms? | Warning / outcome | Exit / summary |
|---|---|---|---|---|
| `--no-tags` (single + batch) | No (skipped like tags/cover) | None (gapless-only `ilst`) | None | Unchanged (0/1/2/3) |
| ADTS output (any flags) | No | None; ADTS byte-identical ± loudness sources | None | Unchanged |
| Dry-run (any format) | No (never decodes/analyzes) | n/a (nothing written) | None; plan identical ± loudness | Unchanged (`dry-run:` shape) |
| Recoverable analysis failure (per track) | Attempted | Track atoms omitted for that track; album-level gain omitted for its album (D4) | Per-file `warning: <in>: …` + album-level omission warning | Audio converts; exit unchanged |
| Silence / undefined integrated | Attempted | Atoms omitted for that track; album-level gain omitted for its album (a silent member makes the album incomplete; all-silent album → no album atoms) | Per-file/album warning | Audio converts |
| Invalid/non-finite measurement | Attempted | Omitted | Warning | Audio converts |
| Clipping adjustment applied | Done (analysis ok) | Clipping-adjusted gains written; peaks verbatim; `iTunNORM` from adjusted track gain | Per-file warning naming measured vs written gain (exit unchanged) | Audio converts |
| Corrupt/undecodable source | n/a (existing decode path fails first) | None | Existing `DecodeFailed`, no destination/orphan | Per-file `Failed`, exit 1 (never disguised as metadata) |
| Album aggregation failure | Partial | No album atoms for that album (valid track atoms as computed) | Album-level warning naming the album + cause | Siblings unaffected |
| MP4 tag-write failure (`boxed`/overflow/mux) | Done | None published | Per-file `Failed` (existing `Metadata` family; a new stable category only if source inspection demonstrates necessity, with separate approval) | Exit 1, no destination/orphan |
| Existing output, no flag | Skipped conversion; source analysis-read still runs for album completeness | Retained output untouched (never rewritten) | Existing `OutputExists` failure accounting unchanged | Exit 1 contribution unchanged |
| Existing output + `--skip-existing` | Skipped conversion; source analysis-read runs | Retained output byte-identical; converted members carry track atoms; album atoms only if every member (incl. retained) analyzable, else omitted per D4 | Skip reported as today; analysis reads are not conversions and add no summary line | Skips never mask unrelated failures |
| Interrupted operation / fatal signal | Partial | Whatever completed stays; no partial outputs (unchanged `Drop` cleanup; orphans only via kill/power-loss, safe to delete) | No new handler in v1 | OS default (unchanged) |

Public API signatures stay unchanged (`execute_plan[_with_tags]`,
`convert_file*`): loudness travels on a **private** thread-through
(precomputed children into the existing `extra_ilst` path). Any new stable
category string or summary change requires owner approval first — the design
assumes none.

## 8. Test and performance plans with acceptance criteria

Required CI (deterministic, `cargo test --locked -p aac-convert`, no external
tools; every test asserts empty temp scans + sentinel preservation where
applicable):

- T-cal (R128 calibration/gating): 1 kHz −23 LUFS stereo (±0.1 LU), silence
  gate (omit path), 44.1/48 kHz × mono/stereo lattice. Fail on any
  nondeterminism (repeat-run inequality).
- T-peak: sample-peak exactness on full-scale/DC/impulse vectors; dBFS =
  20·log10(pin) cross-checked; true-peak code path absent (compile-gated
  `Mode` assertion: only `I | SAMPLE_PEAK [+ HISTOGRAM]` enabled in v1).
- T-atoms: exact `----` byte serialization for the §4 candidate keys
  (mean/name/data-type/locale/payload), `ilst` order assertion
  (`iTunSMPB` → text → `covr` → loudness), `mdat` byte-equality ± loudness.
- T-norm: §4.4 vectors (zero/nonzero/clamp/silence-omission/invalid-omission).
- T-clip (deterministic clipping vectors, mode-`p` semantics): negative gain
  (−3.00 dB, peak 1.0) and zero gain (peak 0.9) pass through unadjusted;
  positive gain without clip (+5.00 dB, peak 0.5, new peak ≈ 0.8891)
  unadjusted; positive gain with clip (+5.00 dB, peak 0.9, new peak ≈ 1.6005)
  adjusted to `+0.92 dB` with a per-file warning; album gain adjusted
  independently against the album peak (+6.00 dB vs 0.7 → `+3.10 dB`);
  capped adjustment never drives a gain below `+0.00`; peaks verbatim
  (`"%.6f"`); `iTunNORM` recomputed from the adjusted gain.
- T-album: 2–3-track album + disc-subdirectory separation; full run vs.
  partial rerun (one retained) → **identical track + album tags**;
  missing/invalid member → track atoms for measured tracks, album atoms
  omitted with the D4 omission warning; failed-conversion member excluded
  the same way, siblings green.
- T-flags: `--no-tags` (gapless-only, no loudness warnings), ADTS
  byte-identity, dry-run plan identity + zero reads.
- T-fail: recoverable (warn + audio OK, no atoms) vs. fatal (existing
  `DecodeFailed`/write-failure paths, exit 1, no destination/orphan).
- T-safe: existing-output sentinel intact both modes; concurrent-publication
  race still exactly-one-winner; no-overwrite + hard-link-only preserved;
  gapless `iTunSMPB` value unchanged for same audio; `edts` absent on the
  default path; `M4A` brands intact.
- T-reg: full existing suite green unmodified in behavior; Tier-A encoder
  gate + `rust/src` empty-diff retained.

Advisory (`#[ignore]` + tool gate, never blocking): EBU 3341/3342 reference
cross-checks where licensed vectors exist; `ffprobe`/mutagen read-back of
`REPLAYGAIN_*`/`iTunNORM`; full-decode zero-stderr; optional out-of-tree
read-only inspection of `rsgain`-tagged files (never a production
subprocess) to validate §4 bytes.

Performance experiments (L3-prep runs these; L2 claims no numbers):
E-domain — controlled source-FLAC-versus-decoded-M4A differential on matched
fixtures (same album converted once; album analyzed both from source FLAC
PCM and from out-of-tree-decoded final M4As, validator tooling only):
acceptance requires \|Δtrack_gain\| ≤ 0.5 dB per track, \|Δalbum_gain\| ≤
0.5 dB, and \|Δsample_peak\| ≤ 0.5 dBFS-equivalent linear tolerance stated in
the experiment note, each small against the AAC-vs-source residual control
(S2.1 differential pattern). If any criterion fails, stop and request owner
review — never switch domains silently.
30-min-equivalent stereo + 3-track album wall-clock overhead vs.
no-loudness baseline; peak RSS (`/usr/bin/time -v` + allocator counters);
HISTOGRAM vs. full-history accuracy delta; retained-output analysis-read
cost under `--skip-existing`. **No other thresholds are set here** — the first
measured baseline becomes the recorded budget; analytical expectation:
O(samples) time, O(blocks) or O(1) state, peak ≈ single-file peak.

## 9. Risks, mitigations, milestones, L3 readiness gate, and no-go conditions

### 9.1 Risks and mitigations

| Risk | Impact | Mitigation in this design | Milestone |
|---|---|---|---|
| Domain divergence (source vs final) surprises | Album gains differ from original workflow beyond noise | §5 + §8 E-domain experiment with control; documented difference either way | L3-prep experiment, then code |
| Positive gains clip on peak-ignoring players | Audible clipping | Verified positive-only protection written to tags; peaks always written; player-side protection documented | L3 T-clip + README note |
| `moov` growth breaks `moov`-is-final | Publish failure on loudness-tagged files | Reuse checked `boxed()` + `set_len` + tiling tests (M1/S1.3 pattern); 5 small atoms/file bounded | L3 T-atoms/T-safe |
| Warning/failure misclassification | CI breakage or silent data loss | §7 matrix; unmeasurable = warning; undecodable = existing failure paths; frozen exits | L3 T-fail |
| Scope creep (configurability, retag, true peak, metadata grouping) | Schedule + container risk | L0 D2/D3/D4/D6 enforced; each revival needs its own note | All L3 reviews |
| Vetting failure late (license/MSRV/vectors) | Wasted implementation | §6 gate runs **before** any production diff | L3-prep gate |
| Formula/bytes mismatch vs real `rsgain` files | Incompatible tags despite passing unit tests | §4 provisional status + advisory byte-inspection; mismatch is a finding, not a waiver | L3-prep validation |
| L0/L1 tension (e.g. “reuse” expectations) | Assumption smuggling | §3 records each follow-L0 callout; deviations need owner notes | L3 entry review |

Milestones (sequential; each gated): L3-prep (experiment + vetting gate +
byte-validation, artifacts only) → L3a (analysis + atom rendering +
unit/formula vectors) → L3b (album orchestration + partial-rerun + flags
matrix) → L3c (bench + docs + full-gate green). No encoder/transport/VBR/
packaging changes in any.

L3 (any production diff, manifest change included) cannot start until **all**
of the following readiness-gate items are complete and recorded:

1. Dependency/license/MSRV/no-C-toolchain vetting for the exact `ebur128`
   version is complete (§6 items 1–5), with evidence recorded.
2. Applicable EBU Tech 3341/3342 vectors independently validated, with any
   gaps explicitly documented (§6 item 6, §8 T-cal/advisory).
3. Histogram-mode accuracy and bounded-memory behavior demonstrated (§6
   item 7, §8 bench).
4. The source-FLAC-versus-decoded-M4A differential experiment passes the
   §8 acceptance criteria (track gain, album gain, sample peak).
5. ReplayGain serialization and `iTunNORM` formula vectors pass independent
   verification (§4.4, §8 T-atoms/T-norm plus advisory byte-inspection).
6. Album failure, retained-output, and no-replace safety tests fully
   specified (§8 T-album/T-safe; §7 matrix).
7. The owner explicitly approves L3.

If any gate item fails, stop and report the evidence and required design
changes. Do not weaken approved L0 requirements or change the analysis
domain silently.

Explicit no-go conditions (restated as stop rules): the §8 E-domain
experiment unrun or failed without an owner-accepted divergence note; the §6
vetting gate unrun or failed without the named fallback evaluation; the §4
formula/atom bytes lacking independent validation vectors; any §7 row
requiring a new flag, new summary field, new exit code, or public-signature
change without owner approval; or any L0 frozen item needing
reinterpretation without an owner amendment.

## 10. Finalized decisions and remaining owner approvals

Owner-approved in this finalization (follow L0 + this design; do not
reinterpret without a new owner note):

- D1 — Source FLAC PCM as the planned analysis domain, conditional on the
  §8 differential experiment passing its track-gain, album-gain, and
  sample-peak criteria; failure stops L3 and returns to the owner.
- D2 — Measured loudness → gains at fixed −18.0 LUFS; measured sample peaks
  recorded verbatim; verified Easy-mode-equivalent positive-only clipping
  protection applied independently to track and album gains (adjusted ≥
  +0.00, `iTunNORM` from the adjusted track gain); AAC samples never
  modified; peaks, clipping-adjusted gains, and audio kept distinct;
  true-peak and any other clipping enhancement out of v1 unless separately
  approved.
- D3 — Four RG fields (track/album gain/peak) + source-derived `iTunNORM`
  formula retained as the provisional candidate; deterministic ordering with
  text/cover/`iTunSMPB`; no compatibility claim until vectors + output
  checks pass.
- D4 — Partially analyzable albums: conversion continues per contract;
  valid track atoms written; album-level gain omitted with a clear
  album-level warning when any member is unanalyzable (including via
  retained outputs); never a subset result presented as complete.
- D5 — Exits, summary shape, and public API signatures preserved; existing
  warning/reporting mechanisms preferred; no new stable category unless
  source inspection demonstrates necessity; any API/summary expansion is a
  separate approval request.

Remaining owner approvals (nothing further is decided here):

1. L3 readiness gate (§9): all seven items evidenced, then explicit L3
   approval. L3 remains unauthorized until then.
2. Any deviation forced by gate evidence (domain, formula, category,
   expansion) — each needs its own owner note before L3 uses it.
