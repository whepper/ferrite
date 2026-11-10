# aac-convert 0.1.0 roadmap

Standalone FLAC-to-M4A converter powered by Ferrite. Target: a practical,
dependable 0.1.0 for personal music conversion. Not a full audio suite.

Scope of this document: the `aac-convert` application only. No encoder
algorithm or public encoder API changes. No MusicPack changes. No FFmpeg
runtime dependency. No new conversion formats.

## 1. Current capabilities (verified in worktree)

All items below were read in `rust/convert/src/*`, `rust/convert/README.md`,
`rust/docs/aac-convert-integration-contract.md`, and the test suite, and
spot-checked with the debug binary (`--help`, `--version`, single-file,
dry-run, batch, rerun, and exit-code probes on 2026-11-10).

### CLI

- Flat interface, no subcommands:
  `aac-convert [--format adts|m4a] [--no-tags] <input.flac> <output>`
  and `aac-convert [--format ...] [--recursive|--no-recursive] [--dry-run]
  [--skip-existing] [--no-tags] [-v|--verbose] [-q|--quiet]
  <input-dir> <output-root>`.
- Dispatch by input file type: regular file converts one file; directory
  activates batch mode. Symlinked-file input follows to its target;
  symlinked-dir input converts the target directory.
- Batch traversal recursive by default; `--no-recursive` converts top level
  only (nested FLAC tallied as ignored); last of the two flags wins.
  Directory symlinks never followed. Symlinks to regular FLAC files convert
  at the link path. Hidden files traversed normally.
- Discovery case-insensitive `.flac`, byte-sorted relative paths,
  deterministic order. Non-FLAC entries tallied as `ignored`, never failures.
- Batch output mapping preserves relative structure and stem byte-for-byte;
  extension replaced with lowercase `.m4a` (default) or `.aac`
  (`--format adts`). Batch never emits `.mp4`/`.adts`. Single-file format
  inference: `.m4a`/`.mp4` select M4A, everything else ADTS; `--format`
  overrides in both modes.
- Preflight: duplicate destinations (exact and ASCII case-fold) abort with
  nothing converted; identical input/output roots refused; output root inside
  the input tree permitted (outputs are `.m4a`/`.aac`, never rediscovered).
- Existing outputs never overwritten. Single-file: `OutputExists` error.
  Batch default: existing destinations count as failures (`existing` tally).
  `--skip-existing`: existing destinations skipped without decoding, counted
  as `skipped`, never overwritten; publication-time races still enforced by
  the atomic hard link (skip on race under the flag, failure otherwise).
  No overwrite/force option exists. `--skip-existing` is batch-only.
- Reporting: one `ok`/`skip`/`fail` line per file on stdout in plan order,
  `error: <input>: <message>` on stderr, always a final
  `summary:`/`dry-run:` line with elapsed time. `--verbose` adds per-file
  audio detail, `tags <in>: <n> mapped, <m> unmapped`, `unmapped <key>`, and
  `ignore <path>` lines. `--quiet` prints only the summary; diagnostics stay
  on stderr. Tag/precision/cover/loudness notices are `warning: <in>: ...`
  on stderr in every mode. Single-file success prints
  `converted <in> -> <out> (<frames> frames, <bytes> bytes, <format>)`;
  encoder failures name the frame index, cause, and discarded-temp-output
  outcome (worktree improvement, see §2).
- Exit codes verified: `0` success (including all-skipped rerun and clean
  dry-run), `1` per-file failures, `2` CLI usage error, `3` fatal setup
  (missing/unreadable input, empty dir, no FLAC, duplicates, output-root
  creation failure). `--verbose`/`--quiet` on single-file input is a usage
  error (`2`); `--dry-run`/`--recursive`/`--skip-existing` on single-file
  input are usage errors.
- `--dry-run` plans, collision-checks, and reports without creating files,
  directories, decoding audio, or reading tags. Dry-run of the fixtures tree
  creates nothing and exits `0` with a `dry-run:` summary.
- `--no-tags` omits user metadata from M4A (gapless `iTunSMPB` kept); ADTS
  no-op.

### Audio conversion

- FLAC input only via Symphonia (`flac` feature): 44.1/48 kHz, mono/stereo.
  16-bit native; 24-bit accepted by truncating to the top 16 bits
  (arithmetic right shift, no rounding/dithering) with one leading per-file
  warning (ADTS and M4A, not suppressed by `--no-tags`). Other rates,
  channel counts, and bit depths rejected with named errors.
- Fixed encoder operating point: AAC-LC VBR-5, 1024-sample frames, delay
  1600, ADTS transport internally. No bitrate/quality knobs (intentional).
- M4A via `shiguredo_mp4` streaming muxer: one `soun`/`mp4a` track,
  `esds` with 2-byte ASC, timescale = sample rate, 1024 duration per sample,
  sample tables from actual frames. Raw access-unit payloads validated
  before muxing; ADTS/M4A audio payloads byte-identical by construction.
- Gapless: `iTunSMPB` (delay, padding, source-frame count) plus Apple-style
  `M4A` brands; no edit list on the default path. Verified decode paths:
  FFmpeg and macOS CoreAudio present exactly the source sample count.
  `mdhd` duration keeps the encoded-stream timeline (documented display
  consequence). VLC/GUI players unverified or approximate — documented as
  non-claims. Experimental edit-list API exists in the library only
  (`convert_file_m4a_experiment`); no CLI flag.
- Integrity: FLAC StreamInfo MD5 verified when present and nonzero;
  mismatch fails without publishing; zero/absent MD5 accepted after
  successful decode with no verification claim.
- Output validation: per-frame ADTS checks (syncword, MPEG-4, AAC-LC,
  rate index, channel config, frame length, non-empty payload, sequential
  index); gapless accounting checked (`N*L >= S+D`) before publication;
  independent-decoder tests exist but are `ignored` (require ffmpeg).

### Metadata

- M4A carries the 10 core Vorbis-comment text atoms (title, artist, album,
  trkn, disk, albumartist, date/year, genre, composer, comment), UTF-8,
  single-valued, first-non-empty wins. Date wins over year. trkn/disk accept
  `N`/`N/M` with `tracktotal`/`totaltracks`/`disctotal`/`totaldiscs`
  fallbacks. Missing/invalid values warn, never fail. Multi-valued distinct
  text values reduce to the first with one per-file warning (worktree
  addition). Unmapped keys (BPM, MusicBrainz, ISRC, label, etc.) counted
  sorted/lowercased, shown only in `--verbose`, never guessed into atoms.
- Cover: at most one JPEG/PNG `covr` (types 13/14), magic-bytes only,
  16 MiB cap, deterministic tier (front, other typed, untyped) then
  encounter order; invalid/oversized skipped with warnings; absent cover
  warns. `--no-tags` omits cover and its warnings. ADTS never carries tags
  or cover and is byte-identical with/without pictures.
- Loudness: native ReplayGain 2.0 + `iTunNORM` via `ebur128` (HISTOGRAM),
  fixed −18.0 LUFS reference, positive-only clipping protection, peaks
  verbatim, `iTunNORM` from the quantized track gain with round-half-even.
  Albums are source-parent directories; a streaming pre-pass analyzes every
  member before any publication. Silence/unmeasurable members omit loudness
  with warnings; any incomplete album omits album-level atoms with an
  album-level warning. `--no-tags`, ADTS, and dry-run skip analysis.
- Ordering guarantee: precision notice, then text-tag warnings, then cover,
  then loudness; `ilst` order text, cover, loudness after `iTunSMPB`.

### Reliability

- Per-file temp file `.aac-convert-<pid>-<seq>.tmp` in the destination
  directory; flushed, synced, atomically hard-linked to the destination.
  Link fails if the destination appeared concurrently, so winners are never
  replaced. No check-then-rename fallback: filesystems without hard-link
  support (FAT/exFAT, some network mounts) fail safely per file.
- Ordinary error paths remove the temp file; existing destinations left
  untouched; concurrent-publication test asserts exactly one winner.
  Abrupt termination (signal/kill/power) runs no cleanup by design: orphans
  keep the recognizable prefix, are never valid outputs, never block reruns,
  and are safe to delete manually once no conversion runs. They are never
  auto-deleted.
- Partial batch failures isolated per file; batch continues and exits `1`.
  Unreadable subdirectories become per-file `UnreadableEntry` failures;
  siblings still convert. Large-file behavior bounded: 8,000-frame encoder
  pushes, packet-staged PCM bounded by decoder block size, muxer holds only
  per-sample metadata; execution strictly sequential. No parallel conversion.
- Fixture corpus is synthetic and committed (`tests/fixtures/README.md`);
  no third-party audio. Corrupt/truncated/MD5-mismatch fixtures assert
  fail-closed behavior with no output published.

### Documentation

- `rust/convert/README.md` is thorough: input matrix, usage, batch rules,
  M4A structure/timing table, metadata table, cover, loudness, experimental
  edit list, dependencies, output safety, memory, tests, scope limits,
  distribution, release follow-ups.
- `rust/docs/aac-convert-integration-contract.md` (worktree, untracked) is
  the application-facing contract: precision policy, metadata policy, error
  recovery, atomic publication, gapless, limitations, acceptance checklist.
  `rust/docs/encoder-integration.md` links to it (worktree edit).
- `THIRD-PARTY-NOTICES.md` inventories the production graph (pinned
  versions, license fields, required pre-distribution actions); still needs
  human/legal sign-off before binary distribution.
- No standalone `aac-convert` release notes or changelog yet. No man page,
  no shell completion, no `inspect` command.

## 2. Current limitations and worktree notes

- **Repo state differs from the task brief.** `HEAD` is `9a4bead` as
  stated, ahead of `origin/main` by 3 commits (`55f135e`, `acb34ff`,
  `9a4bead`). But the worktree has 13 modified files plus one untracked
  file, not just the two benchmark files. The extra uncommitted work is
  `aac-convert`-relevant and reviewed as part of this audit:
  - `rust/convert/README.md`, `src/lib.rs`, `src/main.rs`, `src/meta.rs`,
    `tests/{batch,cli,conversion,coverart,metadata}.rs`,
    `rust/docs/encoder-integration.md`, `rust/src/aac_enc/{encoder,rate}.rs`
    (error `Display`/`source()` chains, precision-warning wording
    "truncated … arithmetic right shift, no dithering", single-file
    `error: <input>: <message>` form, multi-value reduction warning).
  - `rust/docs/aac-convert-integration-contract.md` (untracked, new).
  - The two benchmark files named in the brief are present and were not
    touched by this task: `rust/src/aac_enc/stream_driver.rs` (modified,
    perf-harness appendix) and `rust/examples/aac_bench_wav.rs`
    (untracked). Both preserved exactly.
- Functional gaps by design or omission (not defects):
  one input path per invocation (no multiple files); single-file output is
  an explicit file path (no `--output-dir`); no output-naming templates;
  no overwrite/force mode; no single-file `--dry-run`/`--verbose` detail
  (rejected as usage errors); no `inspect`/probe command; no shell
  completion; no machine-readable (JSON) summary; single-file vs batch
  format defaults differ (ADTS vs M4A) by documented history; single-file
  requires an existing parent directory while batch creates the output root;
  hard-link requirement excludes some filesystems; no signal handler
  (orphan temp files possible); sequential only; fixed VBR-5 (no quality
  ladder); no resampling/downmixing; core-tags-only; one image only;
  album = parent directory (no cue/playlist grouping); CI runs Ubuntu only.
- Verification gaps: M4A independent-decode tests are `ignored` without
  ffmpeg; no committed large-file soak test; no Windows/macOS CI job;
  binary-release artifact flow untested (workspace-bundle model documented
  but no signed release cut yet).

## 3. CLI usability assessment

Strengths: tiny flag surface, predictable file-type dispatch, safe
defaults (no overwrite, collision preflight, dry-run, skip-existing reruns),
stable greppable output (`ok`/`skip`/`fail`/`summary:`, `warning:`/`error:`
on stderr), meaningful exit codes (verified `0/1/2/3`), deterministic
ordering, verbose/quiet modes that compose with the summary contract.

Frictions: two invocation shapes share one usage string (read twice);
single-file rejects `--verbose`/`--quiet` instead of honoring quiet;
single-file has no `--output-dir` (batch users learn a second rule);
`--format` default differs by mode (documented but surprising);
`--skip-existing` is batch-only (correct, but discoverable only via error);
no probe command, so users reach for `ffprobe`/tag editors to answer "what
is in this file"; no completion; error for three-channel input is good
(`unsupported channel count 3`) but only names counts, not the file, in
older committed code — the worktree fix (input path in single-file errors)
closes the main instance of this.

Verdict: usable and safe as-is for 0.1.0. Do not churn the surface now.
Small additions (output-dir, completion, single-file quiet) belong to 0.1.x.

## 4. Proposed CLI interface (keep flat for 0.1.0)

Do not adopt the illustrative `encode`/`inspect` subcommand sketch for
0.1.0. It would invalidate the tested usage strings, exit-code contract,
and scripts/batch parsers for no 0.1.0-required capability. If subcommands
ever come, they come after 0.1.0 with a migration note.

Freeze this for 0.1.0:

```sh
# One file (format inferred; --format overrides; never overwrites)
aac-convert input.flac output.m4a
aac-convert --format adts input.flac output.aac
aac-convert --no-tags input.flac output.m4a

# Album directory (recursive M4A batch, mirrored tree)
aac-convert album/ ./converted/

# Recursive music library with explicit ADTS output
aac-convert --format adts ./music/ ./converted-adts/

# Top level only
aac-convert --no-recursive album/ ./flat-out/

# Preview, rerun, quiet/verbose batch
aac-convert --dry-run album/ ./converted/
aac-convert --skip-existing album/ ./converted/
aac-convert --skip-existing --dry-run album/ ./converted/
aac-convert --quiet album/ ./converted/
aac-convert --verbose album/ ./converted/
aac-convert --no-tags album/ ./converted/
```

Realistic 0.1.0 examples:

```sh
# 1. One FLAC file → M4A next to it
aac-convert ~/Music/rips/track01.flac ~/Music/m4a/track01.m4a

# 2. An album directory → mirrored M4A tree
aac-convert ~/Music/rips/"Blue Album"/ ~/Music/m4a/"Blue Album"/

# 3. A recursive library → one output root
aac-convert ~/Music/rips/ ~/Music/m4a/ --skip-existing

# 4. A custom output directory (batch root is the API; single files use
#    an explicit output path)
aac-convert album/ /tmp/converted/
aac-convert track.flac /tmp/converted/track.m4a

# 5. Existing outputs: fail closed by default, skip on rerun
aac-convert album/ ./converted/        # exits 1 if anything exists
aac-convert --skip-existing album/ ./converted/  # resumes, exits 0 when clean

# 6. Verbose diagnostics (batch only)
aac-convert --verbose album/ ./converted/
```

Single-file `--verbose`/`--quiet` stay usage errors in 0.1.0 (documented).
`--dry-run` stays batch-only. No `inspect` in 0.1.0; users keep `ffprobe`,
`afinfo`, or `mutagen` for probing (README already points ffmpeg at
out-of-tree verification).

## 5. Prioritized 0.1.0 scope

### Required for 0.1.0 (ship blockers)

1. Land or explicitly defer the worktree `aac-convert` deltas: error
   `Display`/`source()` chains (`encoder.rs`, `rate.rs`, `lib.rs`),
   single-file `error: <input>: <message>` form (`main.rs`), truncation
   wording, multi-value reduction warning (`meta.rs` + tests), README and
   contract linkage. They are behavior-visible and test-covered; shipping
   0.1.0 from a tree that contains them without a review decision leaves
   the contract ambiguous.
2. Freeze the CLI surface and exit codes exactly as documented; add a
   regression note that usage-string changes are breaking changes.
3. Confirm `cargo build --locked -p aac-convert` and
   `cargo test --locked -p aac-convert` green on a clean checkout at the
   release commit (Ubuntu CI plus one local macOS run); record the toolchain
   (`1.99.1`) and `Cargo.lock` hash in the release notes.
4. Publish release notes: version, included contract, supported matrix,
   known limitations (§2), distribution model (workspace source bundle),
   licensing/patent notice (no clearance claim; review required).
5. Complete the `THIRD-PARTY-NOTICES.md` human/legal review gate for binary
   distribution (FDK-AAC conditions, MPL-2.0 source availability,
   Apache/MIT/BSD attributions, no fee/endorsement, modification notices).
   No binary leaves the tree before this box is checked.
6. Re-verify the safety invariants at the release commit: no-overwrite
   (single + batch), duplicate-destination abort, same-root refusal,
   temp-file cleanup on ordinary errors, all-skipped rerun exits `0`,
   partial failure exits `1`. The test suite covers these; run it, do not
   re-specify it.

### Recommended for 0.1.x (after the release)

- Single-file `--output-dir` (derive the file name from the input stem).
- Shell completion (bash/zsh/fish) + a short man page generated from
  `--help`.
- Single-file `--quiet` (suppress the `converted` line; keep warnings).
  Keep rejecting `--verbose`/`--dry-run` on single files or define them.
- `--format` default alignment note: consider defaulting single-file
  extensionless output to M4A in a minor release with a changelog entry;
  do not change silently in a patch.
- JSON summary option for scripting (`--format json` for the summary line
  is one small, backward-compatible shape; keep human output default).
- macOS CI job (Linux already covered); Windows build check if the
  hard-link contract is kept, else document Windows as unsupported.
- Prebuilt release archives (macOS arm64/x86_64, Linux x86_64) with
  checksums, built from the workspace bundle with the pinned toolchain.

### Future possibilities (explicitly out of scope now)

Subcommand restructure (`encode`/`inspect`), multiple input files,
output-naming templates, metadata-only retag, parallel conversion,
resampling/downmixing, quality/bitrate ladder, multi-valued tags,
multi-image galleries, additional containers/codecs, stdin/stdout
streaming, watch mode, config files, signal-handler cleanup (deliberately
absent today), true-peak analysis. Each needs its own design and
compatibility review; none is required for a dependable single-purpose
0.1.0.

## 6. Post-0.1.0 possibilities (ordered)

1. Ergonomics: completion, man page, `--output-dir`, single-file quiet,
   JSON summary.
2. Distribution: versioned archives, checksums, Homebrew/cask or tarball
   flow, macOS CI, optional Windows verdict.
3. Operability: `inspect`/`probe` read-only command, richer progress
   (counts/ETA) behind `--verbose`, large-file soak test.
4. Metadata depth (only with explicit models): opt-in multi-value or
   multi-image support, explicit freeform identifier mapping with conflict
   policy — never guessed silently.
5. Performance: only from measured profiles; no 0.1.0 optimization work.

## 7. Release-readiness checklist

- [ ] Package metadata: `aac-convert 0.1.0`, `license = "FDK-AAC"`,
  description, MSRV `1.99.1` — present. Confirm no registry publish;
  document path-dependency reason (already in README).
- [ ] Versioning: 0.1.0 SemVer intent (CLI + contract frozen; usage-string
  changes breaking). Tag/commit discipline decided. [Blocker]
- [ ] README + contract: convert README + integration contract accurate at
  the release commit, including worktree wording. [Blocker]
- [ ] CLI stability: frozen surface + exit codes + output shapes. [Blocker]
- [ ] Error handling: actionable messages with input paths and frame
  context; temp discarded; no partial publish. Mostly done; land §5.1.
  [Blocker]
- [ ] Cross-platform: Ubuntu CI green; local macOS run recorded; hard-link
  requirement documented; Windows verdict documented (unsupported or
  checked). [Blocker: macOS confirmation + Windows statement]
- [ ] CI: `aac-convert` build+test job green; fmt/clippy informational
  only (known drift); decoder-backed convert tests remain manual/ignored.
- [ ] Release builds: `cargo build --locked --release -p aac-convert`
  from a clean workspace bundle; record binary hash. [Blocker for binaries]
- [ ] Licensing notices: `rust/NOTICE` + `LICENSE-FDK-AAC` shipped;
  modification notice + date intact; dependency texts retained;
  `THIRD-PARTY-NOTICES.md` signed off. [Blocker for binaries]
- [ ] Dependency audit: locked graph reviewed (`symphonia 0.6.1`,
  `shiguredo_mp4 =2026.5.0`, `ebur128 =0.1.10`); no FFmpeg runtime;
  no unpublished-registry surprises beyond the known path dep. [Blocker]
- [ ] Reproducibility: pinned toolchain, committed lockfile, synthetic
  fixtures, no vendoring required; clean-checkout build+test logged.
- [ ] Known limitations published (matrix, truncation, tags, cover,
  loudness, sequential, filesystem, player claims). [Blocker]
- [ ] Release notes cut (notes, hashes, limitations, patent/licensing
  pointer with no clearance claim). [Blocker]

Technical blockers are the boxes marked above. Optional polish: completion,
man page, archives, JSON summary, `inspect`, parallel/progress work.

Licensing/patent note: AAC licensing and patent considerations need
appropriate review before public release or binary distribution. This
roadmap makes no legal or patent-safety claim; FDK grants no patent
license (see `THIRD-PARTY-NOTICES.md` and the codec license).

## 8. Recommended implementation milestones (priority order)

1. M1 — Contract freeze: review/land the worktree deltas (§5.1); no
   behavior change after this point except release-note fixes.
2. M2 — Clean-tree verification: `build --locked`, `test --locked -p
   aac-convert`, plus manual exit-code/safety probes (§5.6); record macOS
   result alongside Ubuntu CI.
3. M3 — Docs + notices: README/contract consistency pass; THIRD-PARTY
   sign-off; release notes draft with limitations and distribution model.
4. M4 — Release cut: version commit/tag, workspace bundle + release
   binary hashes (if binaries ship), publish notes. No code changes in M4.
5. M5 — 0.1.x ergonomics: completion, man page, `--output-dir`,
   single-file quiet, JSON summary, macOS CI.

## 9. Acceptance criteria for 0.1.0

- `cargo build --locked -p aac-convert` and
  `cargo test --locked -p aac-convert` pass on a clean checkout with
  toolchain `1.99.1`.
- Single-file: FLAC→M4A and FLAC→ADTS convert; format inference and
  `--format` override behave as documented; existing output refused;
  corrupt/unsupported inputs fail with actionable errors and publish
  nothing; no temp files remain on ordinary errors.
- Batch: recursive and `--no-recursive` mapping, byte-sorted order,
  duplicate-destination abort, same-root refusal, `ignored` tally,
  `--dry-run` creates nothing, `--skip-existing` resumes to exit `0`,
  partial failures exit `1`, setup failures exit `3`, usage errors exit
  `2`; per-file lines + `summary:` shape exactly as documented.
- Metadata: 10 core tags map as specified; multi-value reduction warns;
  unmapped keys verbose-only; one JPEG/PNG cover deterministic;
  ReplayGain 2.0 + `iTunNORM` present with album grouping and documented
  omission rules; `--no-tags` strips user atoms but keeps `iTunSMPB`;
  audio payloads identical with/without tags.
- Gapless: `iTunSMPB` + `M4A` brands on M4A; no default edit list; FFmpeg
  and CoreAudio decode to exactly the source sample count on the
  documented fixtures; no universal-playback claim.
- Safety: hard-link atomic publication; no overwrite on races; orphans only
  from abrupt termination with the documented prefix and manual-cleanup
  rule; FAT/exFAT fail safely per file.
- Docs/release: README + contract + limitations + distribution model
  accurate; release notes published; licensing/dependency gates signed;
  patent review flagged, not claimed.

## 10. 0.1.0 contract freeze (M1, 2026-11-10)

M1 record. Reviewed at `HEAD 9a4bead` (ahead of `origin/main` by 3
commits: `55f135e`, `acb34ff`, `9a4bead`). Worktree: 13 modified files,
3 untracked files, nothing staged. Full `aac-convert` suite green
(202 passed, 0 failed), encoder self-contained suite green (330 passed +
6 production-API passed), ignored ffmpeg decode tests pass (2/2) with
system ffmpeg, release build succeeds, CoreAudio/ffmpeg gapless probes
exact on macOS. No code was changed by M1; no commit, push, or reset
performed. The two benchmark items were not touched.

### 10.1 Outstanding-change inventory and classification

| Change | Category | 0.1.0 disposition |
|---|---|---|
| `convert/src/lib.rs`, `src/main.rs`, `src/meta.rs` (error `Display`/`source()` chains, `error: <input>:` form, truncation wording, multi-value warning) | 1. Production, relevant | Land as-is; reviewed §10.3 |
| `src/aac_enc/encoder.rs`, `src/aac_enc/rate.rs` (`Display`/`source()` for `EncodeError`/`EncodeFailure`/`QcError` + tests) | 1. Production, relevant | Land as-is; no algorithm change, byte-identity pins pass |
| `convert/tests/{batch,cli,conversion,coverart,metadata}.rs` (wording + input-path + reduction-warning assertions) | 1. Production, relevant | Land with the code they pin |
| `convert/README.md`, `docs/encoder-integration.md` (contract link, failure detail, multi-value/ISRC policy, scope limits) | 2. Documentation, relevant | Land as-is; matches implementation |
| `docs/aac-convert-integration-contract.md` (new, untracked) | 2. Documentation, relevant | Land as-is; matches implementation |
| `docs/aac-convert-roadmap.md` (new, untracked; §§1–9 assessment) | 2. Documentation, relevant | Keep; this section freezes the contract |
| `src/aac_enc/stream_driver.rs` (+543 perf-harness appendix) | 3. Optional benchmark/research | Keep out of production scope; never gated by unit runs (`#[ignore]` + env guard) |
| `examples/aac_bench_wav.rs` (new, untracked) | 3. Optional benchmark/research | Keep out of production scope |
| — | 4. Unrelated/deferred | None found |

### 10.2 Roadmap-versus-implementation reconciliation

Roadmap §5 required items:

| Item | Status | Evidence |
|---|---|---|
| Worktree deltas reviewed/landed-or-deferred | COMPLETE | §10.1 + §10.3; recommend landing categories 1–2 unchanged |
| CLI surface and exit codes frozen as documented | COMPLETE | Usage strings, `0/1/2/3` codes, `ok/skip/fail` + `summary:`/`dry-run:` shapes all probed against the debug binary and pinned by `cli.rs`/`batch.rs` |
| Clean build + test (Ubuntu CI + local macOS) | COMPLETE | `cargo test --locked -p aac-convert` green here; CI job covers Ubuntu; toolchain `1.99.1`, lockfile committed |
| Release notes | INCOMPLETE | None exist; draft in M3 |
| THIRD-PARTY sign-off | BLOCKED | Needs human/legal review; no binary leaves the tree before it |
| Safety invariants re-verified | COMPLETE | No-overwrite, duplicate-abort, same-root refusal, temp cleanup, skip-rerun `0`, partial-fail `1` — all suite-pinned and spot-probed |

CLI contract (single-file, recursive batch, format inference/override,
naming, no-overwrite, dry-run, quiet/verbose, exit codes, partial
failures, error text) matches `convert/README.md` on every probed point:
single-file `--verbose`/`--quiet`/`--dry-run`/`--recursive`/`--skip-existing`
are usage errors (`2`); batch-only flags behave as documented; existing
outputs fail closed by default and skip cleanly under `--skip-existing`.

Audio contract (AAC-LC VBR-5, 44.1/48 kHz, mono/stereo, `i16`, 24-bit
truncation by arithmetic shift with leading warning, ADTS validation,
`iTunSMPB` + `M4A` brands, ReplayGain/`iTunNORM` at −18.0 LUFS with
parent-directory albums, `UnalignedInput` recoverable / `FrameFailed`
terminal-poison, MD5-when-present) matches code, tests, and probes:
ffmpeg decodes M4A to exactly the source sample count (48 000 B for the
12 000-frame stereo fixture); `afinfo` reports
`audio 12000 valid frames + 1600 priming + 736 remainder`.

Metadata contract (10 core atoms, first-non-empty wins, date-wins,
`N`/`N+M` tuples with fallbacks, verbose-only unmapped, one JPEG/PNG
cover ≤ 16 MiB by magic bytes, deterministic tier order, tag/payload
independence) matches code and tests. No model expansion for 0.1.0.

### 10.3 Production-change review findings

- Error messages keep their causes: `EncoderConfig`/`Encoder`/`QcFailed`/
  `FrameWrite` now render the underlying error instead of `{:?}` or a bare
  label; the discarded-temp-output statement is part of the message.
- `source()` chains work: `EncodeError → EncodeFailure → QcError/FrameError`
  and `ConvertError::Encoder → EncodeError`, each pinned by a unit test.
- CLI errors identify the input: single-file failures print
  `error: <input>: <message>` (previously the path was missing); batch
  lines already carried the relative input. Pinned by `cli.rs`.
- Multi-value warnings are deterministic and useful: fixed key order
  (`title, artist, album, albumartist, genre, composer, comment`), trimmed
  comparison, identical repeats silent, one line per file. Pinned by
  `meta.rs` unit tests and `metadata.rs`.
- 24-bit wording is accurate: "truncated to 16-bit PCM (arithmetic right
  shift, no dithering)" matches `FlacSource::next_pcm` (Symphonia
  left-justified samples truncated to the top 16 bits). Pinned across
  `conversion.rs`, `coverart.rs`, `batch.rs`, `cli.rs`.
- Output safety and atomic publication unchanged: `output.rs` untouched;
  hard-link publish, temp cleanup, and the concurrent-winner test are
  intact and passing.
- No encoded-output change: conversion/coverart/loudness/metadata suites
  pass unmodified in expectations (only warning-string assertions
  updated); encoder byte-identity pins (`production_api`, lib suite) pass.
- No encoder algorithm change: `encoder.rs`/`rate.rs` diffs are
  `Display`/`Error` impls plus one test; DSP, control flow, and feeder
  paths untouched.

No code correction was required: every delta is either a reviewed
improvement to land or test/doc alignment. M1 changes no behavior.

### 10.4 Product decisions (frozen)

- Keep the flat CLI for 0.1.0. No `encode`/`inspect` subcommands, no
  multiple inputs, no output templates, no parallel conversion, no
  resampling, no quality modes, no metadata-model expansion.
- Keep the single-file/batch format-default asymmetry (extension-inferred
  with historical ADTS default vs batch M4A default). Intentional,
  documented in README and usage text, pinned by tests. Do not symmetrize.
- Multi-value reduction warning covers the seven single-text keys.
  `tracknumber`/`discnumber` tuples and the shared `date`/`year` slot keep
  existing first-wins slot rules without a reduction warning; the
  integration contract §4 ("recognized text key") describes exactly this.
- Degenerate inputs convert: no minimum-duration gate. A 480-sample
  silence fixture converts to valid output (3 frames) with missing-tag,
  missing-cover, and loudness-omission warnings. Silence always omits
  loudness atoms with warnings; album-level atoms are omitted whenever any
  member is unanalyzable — never a subset presented as complete.
- Platform evidence stands as: Ubuntu CI (build + full suite) and local
  macOS (build, suite, ffmpeg + `afinfo` gapless probes). Windows is not
  claimed. FAT/exFAT and hard-link-less filesystems fail safely per file.

### 10.5 Frozen 0.1.0 contract (by reference)

Normative detail lives in `../convert/README.md` (usage, batch rules,
M4A structure, metadata/cover/loudness policies, safety, scope limits)
and `aac-convert-integration-contract.md` (§§2–9). Frozen summary:

- Purpose: dependable personal FLAC-to-M4A conversion powered by Ferrite;
  not a suite, not a tagger, not a player.
- Guaranteed: input matrix (FLAC, 44.1/48 kHz, mono/stereo, 16-bit;
  24-bit truncated with warning); fixed AAC-LC VBR-5 operating point;
  format inference/override; byte-sorted deterministic batch plans;
  relative-structure output mapping (`.m4a`/`.aac`); duplicate-destination
  abort; same-root refusal; never-overwrite with atomic hard-link
  publication; temp cleanup on ordinary errors; per-file isolation with
  exit `1` on partial failure; `0/1/2/3` exit codes; `ok/skip/fail` +
  `summary:`/`dry-run:` output shapes; first-value single-atom metadata
  with warnings; deterministic single-image cover selection; ReplayGain
  2.0 + `iTunNORM` with documented omission rules; `iTunSMPB` + `M4A`
  brands with no default edit list; tags never altering audio payloads.
- Best-effort: loudness analysis on hostile inputs (warns, never fails);
  tag reads on malformed comment blocks (warn or `Metadata` error only
  after successful decode).
- Unsupported: resampling, downmixing, other codecs/containers, quality
  ladder, multi-value/multi-image output, identifier mapping, parallel
  conversion, signal-handler cleanup (orphan temp files removed manually).
- Player-dependent: trimmed-playback presentation (exact on verified
  FFmpeg/CoreAudio paths only); container-duration displays follow the
  encoded-stream timeline; no universal-gapless or perceptual-equivalence
  claim of any kind.

### 10.6 Acceptance criteria and status

| Criterion | Status |
|---|---|
| Release-mode workspace build (`--locked -p aac-convert`) | Demonstrated (M1) |
| Full automated suite green | Demonstrated: 202 passed, 0 failed (M1) |
| Independent M4A decode validation | Demonstrated: `independent_decode` 2/2 with system ffmpeg; `afinfo` trim exact (M1, macOS) |
| Production output identity pins pass | Demonstrated: `production_api` 6/6, lib suite 330 passed (M1) |
| Batch conversion tests pass | Demonstrated: `batch.rs` 47/47 (M1) |
| Metadata/artwork tests pass | Demonstrated: `metadata` 19, `coverart` 11, `loudness` 15 (M1) |
| Gapless signalling validated | Demonstrated: sample-exact ffmpeg + `afinfo` probes (M1) |
| Error handling and exit codes tested | Demonstrated: suite-pinned + binary probes (`0/1/2/3`) (M1) |
| No-overwrite/atomic-publication verified | Demonstrated: concurrent-winner + rerun probes (M1) |
| Licensing/third-party notices present | Present, sign-off BLOCKED (human/legal) |
| README + contracts agree with behavior | Agree after landing §10.1 categories 1–2 (reviewed M1) |
| Release notes + versioning prepared | INCOMPLETE (M3) |

### 10.7 Remaining release blockers (not contract issues)

1. Land §10.1 categories 1–2 (reviewed, unchanged) or explicitly defer.
2. `THIRD-PARTY-NOTICES.md` human/legal sign-off before any binary leaves
   the tree; AAC patent authorization stays the integrator's review item.
3. Cut release notes (version, contract pointer, matrix, limitations,
   distribution model, licensing/patent notice with no clearance claim).
4. Record release-commit verification (build hash, test log, `Cargo.lock`).

### Verdict: CONTRACT FROZEN

The 0.1.0 product contract is frozen as §10.5 (by reference, with §10.4
decisions). The release itself is not ready: blockers §10.7 items 2–4
remain. Recommended next milestone: M2 clean-tree verification at the
release commit, then M3 docs + notices.

## 11. M3 documentation, notices, and distribution readiness (2026-11-10)

M3 record. Working basis: RC `a1cb226` (parent `9a4bead`), evidence
commit `93042a8`, verification report
`releases/0.1.0-verification.md` (verdict: VERIFIED WITH LIMITATIONS).
M3 adds documents only; no code, dependency, lockfile, or license-text
changes. No commit, tag, push, or binary publication.

### 11.1 Documentation audit findings

- Product identity is consistent across root README, `rust/README.md`,
  `convert/README.md`, `encoder-integration.md`, the integration
  contract, and this roadmap: standalone native Rust AAC-LC VBR-5
  encoder + FLAC-to-M4A converter, `ferrite-aac 0.1.0` /
  `aac-convert 0.1.0`, MSRV 1.99.1, no MusicPack integration.
- Installation/usage, input/output matrix, batch rules, metadata/cover
  behavior, and error handling agree between `convert/README.md` and the
  frozen contract; the M2 probes reproduced every documented behavior.
- Technical claims are bounded by implementation and M2 evidence:
  fixed VBR-5, 44.1/48 kHz mono/stereo, `i16` with documented 24-bit
  truncation, `iTunSMPB` qualified to verified decode paths, ReplayGain
  2.0 at −18.0 LUFS with documented omission rules, no universal-playback
  or perceptual-equivalence claims.
- Two documentation-level observations, no code action: (a) mixed
  rate/channel layouts in one album directory receive track-level
  loudness only (album atoms omitted with a warning) — implemented,
  tested, and now stated in the release notes; the L3 closeout's pending
  owner acknowledgment (F3) is folded into human sign-off item 6 below;
  (b) the pre-existing `v3.0.0` git tag points at a legacy upstream
  commit and is unrelated to Ferrite 0.1.0 — no 0.1.0 tag is created in
  M3.
- No contradictions requiring correction were found; no existing document
  was edited in M3.

### 11.2 New documents

- `releases/0.1.0.md` — user-facing draft release notes (DRAFT, no date,
  no binaries, no clearance claim), covering identity, capabilities,
  formats, CLI examples, metadata/artwork, qualified gapless behavior,
  performance evidence vs guarantees, limitations, build instructions,
  and licensing status.
- `releases/0.1.0-third-party-audit.md` — dependency/license inventory
  from the committed lockfile and RC graph, AAC patent issue checklist,
  and the human sign-off table (all PENDING).

### 11.3 Platform support matrix (evidence-based)

| Platform | Status | Basis |
|---|---|---|
| macOS ARM64 | Verified on M2 | Release + debug builds, full suites, ffmpeg + `afinfo` probes, macOS 27.0.1 |
| macOS x86_64 | Buildable, not tested | No hardware evidence; no claim |
| Linux x86_64 | Covered by CI configuration | `ubuntu-latest` jobs (build, suites, decode validation); not executed in M2 |
| Linux ARM64 | Buildable, not tested | No evidence; no claim |
| Windows x86_64 | Not currently supported | Hard-link publication contract unvalidated; no CI; no verdict |

No platform support is claimed from Rust target availability alone.

### 11.4 Proposed release artifacts (plan only — nothing built or published)

1. Workspace source bundle (corresponding sources, `Cargo.lock`,
   `NOTICE`, `LICENSE-FDK-AAC`, third-party notices) — the only supported
   model today.
2. macOS ARM64 binary + SHA-256, with version output and build provenance
   recorded — after approvals.
3. Linux x86_64 binary + SHA-256 — after CI validation of the release commit.
4. macOS x86_64, Linux ARM64, Windows binaries — only if validated; no
   commitment now.
5. Third-party notices and license texts alongside every artifact.

Reproducibility note: builds are `--locked` with a pinned toolchain, but
no reproducible-bit-identical claim is made; the release should document
the exact build environment instead.

### 11.5 Release-readiness checklist

| Item | State |
|---|---|
| Product documentation (README, contract, integration guide) | COMPLETE |
| Release notes draft (`releases/0.1.0.md`) | COMPLETE (DRAFT, unapproved) |
| Package metadata (0.1.0, FDK-AAC, MSRV 1.99.1) | COMPLETE |
| Dependency inventory (`0.1.0-third-party-audit.md` §§1–4) | COMPLETE (technical findings) |
| Third-party notices shippable | PENDING (human review, audit §7 item 3) |
| Human licensing sign-off | BLOCKED (all audit §7 rows PENDING) |
| AAC patent review | BLOCKED (audit §6 checklist open) |
| Platform support defined | COMPLETE (§11.3) |
| CI status (Ubuntu config green by design) | PENDING (release-commit run outstanding) |
| Release artifacts | PENDING (plan only, §11.4) |
| Checksums | PENDING (with artifacts) |
| Final release verification | PENDING (post-approval rebuild + test log) |
| Git tag and publication approval | BLOCKED (requires all legal rows first) |

Technical work is complete; every remaining item is an approval or an
approval-gated action.

### Verdict: M3 COMPLETE WITH BLOCKERS

M3's documentation and audit deliverables are done. The blockers are
human/legal approvals (audit §7) and the approval-gated release actions
above — not further technical work known at this time.
