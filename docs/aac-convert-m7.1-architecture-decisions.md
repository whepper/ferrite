# M7.1 — AAC Conversion Application: Architecture and Product Decision Record

Documentation-only decision record. No application, encoder, manifest, test,
fixture, CI, or generated-file changes. No implementation is authorized by this
document.

**Status: `M7.1 COMPLETE — DECISIONS RECORDED; IMPLEMENTATION NOT AUTHORIZED`**

- Date: 2026-10-23
- Branch: `main`
- HEAD before this document: `1b4ffc7cbf90544c67fc5a7ef73e5aeea4746126`
- Tracking state before this document: 3 commits ahead of local `origin/main`,
  0 behind; tracked worktree clean; no staged changes; no Git fetch performed
- Required validation baseline: the existing M6.6 `build-and-test` CI job and
  its documented Tier A/Tier B boundary remain in force

This record is based on the preceding M7 read-only architecture audit and the
repository documents listed in §12. It records decisions; it does not replace
the encoder's existing API contract, release-gate evidence, or validation
records.

## 1. Purpose and decision summary

The next product step is a native Rust FLAC-to-AAC conversion application
around the existing encoder. The application is a separate consumer of the
`aac` codec crate; it is not a new encoder implementation and not a rewrite of
the C reference.

The confirmed product direction is:

1. The first user-facing release targets **M4A** output.
2. **ADTS is an internal engineering milestone**, used to validate FLAC
   decoding, PCM handling, frame assembly, and output finalization before the
   M4A container is added.
3. The existing `aac` codec crate remains separate. The application is a new
   workspace member, provisionally named **`aac-convert`**.
4. VBR profiles 1–4 and new encoder algorithms are out of scope for this
   project phase.
5. FFmpeg is **not** a production dependency.
6. No RAM-disk workflow is added. Processing uses bounded memory and temporary
   output files on the destination filesystem, followed by atomic replacement
   where the platform supports it.
7. Initial batch processing is **sequential**. No premature concurrency,
   worker pool, or scheduling abstraction is introduced.
8. No GUI is included in the initial release.
9. Existing encoder validation and CI requirements are preserved. Any CI
   expansion is separately scoped and must not weaken existing checks.
10. The application must not leak its dependencies into the codec crate.

## 2. Confirmed decisions

| # | Decision | Consequence |
|---|---|---|
| D1 | First user-facing release is M4A; ADTS is internal | M4A container, duration, and gapless behavior are MVP requirements; ADTS does not ship as the product format. Resolved M7.13: default M4A carries `iTunSMPB`-only signalling with the `M4A` brand (no edit list); duration fields keep the encoded timeline |
| D2 | Codec stays in `aac`; app is a new `aac-convert` workspace member | Codec remains usable independently; app dependencies stay out of `aac` |
| D3 | Use the existing native AAC-LC VBR-5 encoder | No encoder algorithm, tuning, or VBR-profile work in this phase |
| D4 | No FFmpeg production dependency | Native FLAC decode and native M4A output are required for the product path; external tools may only validate |
| D5 | No RAM-disk handling | Temp files use the destination filesystem; memory use is bounded; optional RAM disks are a user concern, not app architecture |
| D6 | Sequential batch processing initially | One file at a time; deterministic behavior and simpler failure handling; concurrency is a later decision |
| D7 | No GUI in the initial release | CLI only |
| D8 | Preserve M6.6 validation and CI boundaries | Required job and Tier B separation remain; CI expansion is separately approved |
| D9 | Application dependencies do not leak into codec | `aac` must not depend on FLAC, MP4, metadata, CLI, or app modules |
| D10 | Keep the encoder's existing `i16` PCM interface for the MVP | 24-bit handling stays in the application; no encoder PCM API expansion in this phase |

## 3. Proposed first user-facing release (MVP)

The following is the proposed first release. It is approved as product
direction; detailed behavior remains subject to the spikes and approvals in
§7 and §8.

### 3.1 Product capabilities

- Native FLAC input decoding.
- Single-file conversion and recursive directory conversion.
- AAC-LC VBR-5 encoding through the existing Rust `Encoder`.
- M4A output with correct duration and gapless playback.
- Core metadata preservation. The exact Vorbis-comment-to-M4A tag mapping must
  be documented and reviewed before metadata implementation begins.
- 44.1 kHz and 48 kHz sample rates.
- Mono and stereo channel layouts.
- A deliberate policy for 16-bit and 24-bit FLAC input. The preferred direction
  is conversion to the encoder's existing `i16` interface, with suitable
  dithering for 24-bit sources; the exact conversion/dither algorithm is
  unresolved.
- Mirrored output-directory structure for batch conversion.
- Collision protection and explicit handling of existing output files.
- Atomic output finalization where supported by the platform.
- A CLI with dry-run support, useful progress reporting, and meaningful exit
  codes.
- Structural output verification plus independent decoder/playback validation.

### 3.2 Proposed behavior details

| Area | Proposed MVP behavior | Status |
|---|---|---|
| Input selection | Accept one FLAC file or a directory tree | Proposed |
| Discovery | Recursive, case-insensitive `.flac` matching, deterministic ordering | Proposed |
| Output layout | Mirror the input-relative directory structure under the output root | Proposed |
| Output naming | Replace the `.flac` suffix with `.m4a` | Proposed; exact collision rules need review |
| Existing outputs | At least `skip`, `newer`, and `rebuild`; no `retag` in the MVP | Proposed |
| Failure behavior | Per-file failure recorded; run continues where safe; non-zero exit if any file failed | Proposed |
| Progress | Per-file progress and a final summary; no GUI | Proposed |
| Verification | Structural M4A checks and independent decode/playback checks | Required; implementation details unresolved |
| Metadata | Core tags only; mapping documented before implementation | Required; mapping unresolved |
| Gapless | Audio-timeline duration and trimmed encoder delay/padding via `iTunSMPB`-only signalling; container duration fields unchanged | Required; decided M7.13 (edit list rejected: FFmpeg short-input failure, CoreAudio ignores it) |
| Packaging | `aac-convert` binary plus testable library entry point | Proposed |

### 3.3 Success definition

The MVP is successful when a supported FLAC file converts to an M4A file that:

1. is recognized as AAC-LC,
2. plays in an independent decoder/player,
3. has the intended duration with no audible leading encoder delay or trailing
   padding,
4. preserves the documented core metadata,
5. is produced without FFmpeg or a C oracle in the production path,
6. does not regress the existing encoder's required tests or CI gate.

This is a product-acceptance definition, not a claim that all `flac2aac`
capabilities are implemented. **No parity claim with `flac2aac` is made until
the relevant capabilities are implemented and verified.**

## 4. Explicitly out of scope for the initial release

- VBR profiles 1–4.
- GUI or desktop packaging.
- ReplayGain or loudness normalization.
- Embedded cover-art processing.
- `retag`-only mode.
- Retry-state persistence or resume files.
- Parallel workers or concurrent encoding.
- TOML configuration files.
- Non-FLAC inputs.
- Resampling.
- More than two channels.
- Sample rates outside 44.1 kHz and 48 kHz.
- Encoder PCM API expansion to `i32`/float.
- Raw AAC, LATM/LOAS, ADIF, fragmented MP4, or MP4 video.
- Any claim of `flac2aac` parity or perceptual equivalence.

## 5. Architecture boundaries

### 5.1 Crate layout

Proposed workspace:

```text
rust/
├── Cargo.toml          # existing `aac` package + workspace root
├── src/                # existing codec; unchanged by this phase
├── framework/          # existing decoder demo; unchanged
├── ffi/                # existing decoder C ABI; unchanged
└── convert/            # NEW: package `aac-convert`
```

The application crate is provisional until its public API and CLI name are
approved. No `aac-flac`, `aac-mux`, or `aac-cli` crate is created in the MVP.

### 5.2 Responsibility split

| Responsibility | Home | Rule |
|---|---|---|
| AAC codec, configuration, PCM framing, ADTS assembly | `aac` crate | Existing behavior; no algorithm changes |
| FLAC decoding | `aac-convert` | Native app dependency; not exposed to codec |
| File discovery, path mapping, collision policy | `aac-convert` | App-only |
| Conversion orchestration and error aggregation | `aac-convert` | App-only |
| Metadata and tag mapping | `aac-convert` | App-only; mapping reviewed before implementation |
| M4A container, sample tables, gapless signaling | `aac-convert` | App-only; muxer selected by spike |
| Temporary-file and atomic-finalization policy | `aac-convert` | App-only |
| CLI, progress, exit codes | `aac-convert` | App-only |
| Structural and decoder-based output verification | `aac-convert` | App-only; external validators optional |
| C oracle and reference validation | Existing `tools/` | External tooling; never a production dependency |

### 5.3 Dependency rule

Dependency direction is one-way:

```text
aac-convert  ──depends on──►  aac (feature `aac_enc`)
aac          ──must not depend on──►  aac-convert or any app/CLI/container crate
```

The application may depend on a FLAC decoder, M4A muxer, metadata library, CLI
parser, and temporary-file helper. None of those may be added to `aac` as part
of the conversion MVP.

### 5.4 Processing model

- PCM is read and encoded in bounded chunks, not by buffering an entire file.
- A starting chunk target of approximately **8,000 sample frames per channel**
  is reasonable and must be verified for performance and output stability.
- The app must not repeatedly drain a growing front-buffered vector; the
  existing encoder feeder is suitable for bounded pushes, not whole-file
  pushes.
- Output is written to a temporary file on the destination filesystem. On
  success, the app attempts an atomic replacement of the final path. On
  failure, the temporary file is removed and the existing destination is left
  untouched.
- Where atomic replacement is not supported or cannot be guaranteed, the
  fallback behavior must be explicit and documented before release.
- Initial batch processing is sequential. No worker pool, async runtime, or
  parallel scheduler is introduced.

### 5.5 Data flow

```text
FLAC file
  → discovery / output mapping
  → native FLAC decoder
  → bounded PCM chunks
  → 16-bit conversion / dithering policy (24-bit sources)
  → aac::aac_enc::Encoder (existing i16 interface)
  → raw AAC frames + stream information
  → ADTS writer (internal milestones M7.3–M7.4)
     or
     M4A muxer + gapless + metadata (M7.5+)
  → temporary output file
  → structural verification
  → atomic replacement
  → optional independent decoder/playback validation
```

ADTS is deliberately shown as an internal branch. It proves the input and
codec integration before the M4A container is introduced.

## 6. Milestone outline

| Milestone | Scope | Exit criterion |
|---|---|---|
| **M7.1** | This architecture/product decision record | Decisions, proposed MVP scope, unresolved items, and milestones recorded; implementation not authorized |
| **M7.2** | Encoder public-API hygiene, after reviewing compatibility implications | Public surface reviewed/narrowed only where approved; existing tests, byte-identity gates, and out-of-crate use remain valid; no algorithm changes |
| **M7.3** | Single-file FLAC-to-ADTS engineering milestone | One supported FLAC file converts to a structurally valid ADTS stream; independent decoder check passes; no C or FFmpeg in the normal path |
| **M7.4** | Directory processing and CLI behavior | Recursive input, mirrored output, collision handling, existing-output policy, dry-run, progress, and exit codes work sequentially; no M4A yet |
| **M7.5** | M4A feasibility spike, then M4A MVP only if the spike succeeds | Audio-only AAC muxing, ASC, sample tables, duration, gapless behavior, and independent playback pass the §8 criteria; otherwise stop and report |

**M7.2 scope boundary:** public-API hygiene only. The existing encoder internal
orchestration refactor (for example, renaming or splitting `stream_driver`) is
out of scope unless a concrete blocker is demonstrated and separately approved.

**M7.3/M7.4 scope boundary:** these are engineering milestones, not the
user-facing release. They validate the application boundary and CLI before the
container work. ADTS output is not the MVP product format.

**M7.5 scope boundary:** no M4A MVP commitment until the feasibility spike
passes. A failed spike is a valid outcome; it must be reported with the
evidence and the next option, not hidden by selecting a muxer prematurely.

## 7. Unresolved decisions and required evidence

These items are intentionally unresolved. No library or mechanism is selected
by this record.

| # | Unresolved item | Required evidence/decision |
|---|---|---|
| U1 | M4A muxer/library selection | Spike must verify audio-only AAC muxing, ASC/`esds`, sample tables, duration, metadata interaction, and maintenance/license suitability |
| U2 | Gapless signaling and validation | **Resolved M7.13: `iTunSMPB`-only with the `M4A` brand; no edit list.** M7.8–M7.11 evidence: exact source counts and alignment on FFmpeg 9.0.2 and macOS CoreAudio (macOS 27.0.1) across 20 input lengths; `elst` rejected (FFmpeg decodes ≤448-frame streams empty, CoreAudio ignores it); `mdhd`/`mvhd`/`tkhd` deliberately unchanged (rewriting them breaks FFmpeg tail trimming). Remaining: GUI player verification, VLC approximation documented as a non-blocking limitation |
| U3 | FLAC decoder/library selection | Verify 16/24-bit decoding, metadata access, sample-rate/channel reporting, licensing, maintenance, and platform support |
| U4 | Metadata/tag library and exact mapping | Produce a reviewed Vorbis-comment-to-M4A table before implementation; document unsupported tags rather than claiming arbitrary preservation |
| U5 | Exact 24-bit → `i16` conversion/dither policy | Choose TPDF or another documented method; verify no clipping/overflow; do not expand the encoder PCM API in this phase |
| U6 | Final `aac-convert` public API surface | Keep the library entry point minimal and testable; do not expose internal modules prematurely |
| U7 | Final CLI name and command syntax | Decide binary name, flags, overwrite-policy names, progress format, and exit-code mapping before M7.4 |
| U8 | Licensing and AAC patent considerations | Review FDK-AAC license obligations for the combined binary, third-party dependency licenses, and patent notice requirements |
| U9 | Optional post-MVP capabilities | Loudness, cover art, retag, retry state, parallel workers, TOML, and non-FLAC inputs require separate decisions and must not shape the MVP architecture |

Additional unresolved details:

- Whether metadata is written before or after M4A finalization, and which
  library owns tag writing.
- Whether atomic replacement can be guaranteed on every intended platform, and
  what the documented fallback is.
- Which independent decoder/player combination is the release baseline
  (FFmpeg/`aac-dec`/Apple Music/VLC) and which are advisory.
- Whether the `aac` codec needs a small `AdtsFrame::payload()` or
  `StreamInfo`/ASC API for M4A; decide from the spike, not speculatively.
- Exact supported-file diagnostics: reject or warn on 24-bit, unusual channel
  layouts, zero-length files, and malformed metadata.
- Whether the first M4A MVP includes only core text tags or also any freeform
  tags; cover art remains out of scope.

## 8. M4A feasibility spike acceptance criteria

The spike must be completed and recorded before the M4A MVP is scheduled. It
must verify at least:

1. **Audio-only AAC muxing.** A mono and a stereo AAC-LC VBR-5 stream can be
   muxed without a video track. Video-first muxers must not be assumed to work
   for audio-only output.
2. **Codec configuration.** The `esds`/AudioSpecificConfig matches AAC-LC,
   the selected sample rate, and the channel configuration.
3. **Sample tables.** Sample durations, sizes, chunk offsets, and decode
   timing are internally consistent; duration is correct.
4. **Gapless behavior.** Encoder delay (1600 samples at 1024-frame AAC-LC) and
   final zero padding are represented so a compliant player/decoder produces
   the intended sample count. Resolved M7.13 as `iTunSMPB`-only (no `edts`/`elst`,
   durations unchanged); the spike criterion is met on the FFmpeg and CoreAudio
   decode paths, with GUI player verification still outstanding.
5. **Independent validation.** The file decodes with at least one independent
   decoder and plays in at least one independent player on the primary
   platform (macOS ARM). FFmpeg/C tools may be used only as validators, not as
   production dependencies.
6. **Metadata floor.** At minimum, title/artist/album/track/disc tags can be
   written without breaking playback or duration; the full mapping remains a
   separate reviewed deliverable.
7. **Failure criteria.** If any of the above fails, the spike records the
   failure, the candidate muxer, and the evidence. The M4A MVP is not started
   until a viable option is approved.

## 9. Compatibility and validation constraints

- **Preserve M6.6.** The existing required `build-and-test` job, its 15-test
  skip boundary, and its Tier A/Tier B distinction remain unchanged.
- **No C/FFmpeg in normal builds.** The codec and app must build and run
  without the C oracle, C encoder/decoder, FFmpeg, or rsgain.
- **C oracle stays external.** Reference comparisons remain Tier B validation,
  not a hidden requirement for ordinary tests.
- **No encoder algorithm changes.** M7.2 may change visibility/API surface only
  after a compatibility review; quantization, psychoacoustics, rate control,
  bitstream, and ADTS behavior must not change.
- **No CI weakening.** Any future app CI job is additional. It must not remove
  or relax existing required checks.
- **No ABX requirement.** Validation is structural, decoder-based, and
  behavioral. Perceptual competitiveness is the encoder objective, not a
  bit-identical or ABX claim.
- **No parity claim.** `flac2aac` parity is assessed only after the relevant
  capabilities are implemented and tested.
- **Independent validation is not production dependency.** External decoders
  and players are allowed in test/release validation only.

## 10. Risks and open questions

| Risk | Impact | Mitigation |
|---|---|---|
| M4A muxer maturity | Container bugs, duration/gapless failures | Spike first; prefer a maintained library or a small auditable muxer; test audio-only explicitly |
| Gapless inconsistency across players | Audible delay/padding or wrong duration | `iTunSMPB`-only + `M4A` brand decided M7.13; exact on FFmpeg 9.0.2 and CoreAudio decode paths. Residual: VLC approximate (non-blocking per product decision), GUI players unverified, container-reported duration stays on the encoded timeline |
| 24-bit conversion quality | Quantization noise or clipping | Define dither/rounding and test with 24-bit fixtures; do not silently truncate |
| FLAC/metadata library licensing | Distribution constraints | Review licenses before selection; record the decision |
| AAC patent/licensing obligations | Distribution risk | Obtain review before publishing binaries; preserve notices |
| Atomic replacement differences | Partial or clobbered outputs | Use destination-filesystem temp files; document platform fallback; test failure paths |
| Bounded-memory implementation drift | Large-file memory spikes or O(n²) behavior | Fixed chunk target (~8,000 frames/channel as a starting point); long-file test |
| Metadata overclaim | User trust and data loss | Publish the exact mapping; document unsupported tags |
| Scope creep into loudness/cover art/GUI | MVP delay | Keep §4 out-of-scope list enforced; separate milestones |

## 11. What this record does not authorize

This document does **not** authorize:

- adding the `aac-convert` crate or any dependency;
- changing `aac` public modules or encoder code;
- selecting a FLAC, M4A, metadata, or CLI library;
- implementing M4A, gapless, metadata, or tag mapping;
- changing CI workflows or branch protection;
- changing the encoder's `i16` PCM contract;
- claiming M4A readiness, `flac2aac` parity, or release readiness.

Implementation begins only after M7.1 is reviewed and the first implementation
milestone (M7.2) is separately approved.

## 12. References

- `docs/aac-enc-migration-spec.md` — encoder migration and C/Rust architecture.
- `docs/aac-enc-vbr5-profile.md` — current AAC-LC VBR-5 profile and implementation status.
- `docs/aac-enc-m6-api-contract.md` — production `Encoder` API and lifecycle contract.
- `docs/aac-enc-m6-hardening-review.md` — API boundary findings and test coverage gaps.
- `docs/aac-enc-m6-release-readiness.md` — packaging and runtime-independence decisions.
- `docs/aac-enc-m6-release-gate.md` — historical release gate and validation evidence.
- `docs/aac-enc-release-gate-addendum-20261021.md` — current ADTS audit, MSRV policy, and M6.6 CI section.
- `docs/aac-enc-m8-owner-decision.md` — M8 fixture remains provisional; not an approval.
- `docs/research/d021/D021-CLOSURE-RECORD.md` — retained MDCT/bitstream research closure.
- `rust/src/aac_enc/encoder.rs` — public encoder API.
- `rust/src/aac_enc/pcm.rs` — PCM feeder, delay, and flush framing.
- `rust/src/aac_enc/config.rs` — validated configuration matrix.
- `.github/workflows/ci.yml` — current required and informational CI jobs.
- `tools/enc-oracle/README.md` — oracle/extraction workflow and reproducibility limits.
- `https://github.com/whepper/flac2aac` — reference product inspected for capability inventory; not a code-reuse mandate and not a parity claim.
