# M8 — Release Preparation and Reproducibility

Release-engineering milestone for the six-profile (VBR-1 through VBR-6) encoder
tree. No encoder algorithm, psychoacoustic-tuning, profile-parameter, or
bitstream change was made in this milestone: no `.rs` production logic was
edited, and the pinned byte/hash baselines all still pass unmodified. No
commit, push, tag, or publication was performed.

**Verdict: `GO WITH DOCUMENTED LIMITATIONS`**

The encoder is technically validated for release; publishing additionally
requires the mechanical release commit (exact file list in §13), a 0.3.0
version bump with new release notes (§9), and a tag. Tier-B C-oracle parity
(9 phase-1 tests + M5.2 stream comparison) remains environmentally blocked
(private reference checkout unavailable) with compensating evidence in place;
the FDK-decoder gap from M7 is closed (3/3 decoder-backed tests pass against
a locally built pinned decoder). No perceptual claim is made. §12 lists every
remaining finding; §14 states the exact conditions.

M7 closed as `PASS WITH FINDINGS`. This report resolves or explicitly accepts
each of M7 F-1 through F-9 (§3).

---

## 1. Exact revision and working-tree state

| Item | Value |
|---|---|
| HEAD | `a1bcbada51a50ac0fcc92e84698807890ebae5e6` |
| `git describe` | `v0.2.0-dirty` |
| Branch | `main`, up to date with `origin/main` (`## main...origin/main`) |
| Staged | empty (nothing staged) |
| Tags present | `v0.1.0`, `v0.2.0` (Ferrite releases), `v3.0.0` (FDK-AAC v3.0.0 untouched) |
| Toolchain | rustc/cargo 1.99.1 (MSRV, per manifests) |
| Platform validated | macOS 27.0.1 arm64; cmake 4.4.4, Apple `c++` |
| Recent commits | `a1bcbad` chore(release): prepare Ferrite 0.2.0; `67705b3` short-block M/S regroup fix; `9eb407d` converter encoder identification; `647c495` parallel batch |

Working tree at audit end: **27 modified tracked files, 0 staged, 20 untracked
files** (full lists in §13). HEAD itself is unchanged by this milestone
(`git rev-parse HEAD` still `a1bcbad` at report time; no commits created).

Modified (27): `README.md`, `docs/README.md`, `rust/README.md`,
`rust/convert/README.md`, `rust/convert/src/{batch,lib,main}.rs`,
`rust/convert/tests/{batch,cli}.rs`,
`rust/docs/{aac-convert-integration-contract,encoder-integration,user-manual}.md`,
`rust/src/aac_enc.rs`, `rust/src/aac_enc/{bitcount,config,encoder,m5a_decode,m5b_measure,m5g_census,m6_migration,pe,production_baseline,psy,psy_config,quant,sfb,stream_driver}.rs`.

Untracked (20): `docs/aac-enc-m7-release-readiness.md` (M7 audit record),
`rust/docs/vbr-profiles.md` (authoritative profile reference),
`rust/examples/{aac_bench_wav,m7_audit_sweep,m7_audit_spectral}.rs`,
14 `rust/src/aac_enc/testdata/cfg_{44100,48000}_{20000,32000,40000,48000,56000,64000,72000}.txt`
snapshots, `tools/enc-oracle/dump_vbr_init_configs.cpp` (oracle generator).

No user changes were discarded: this milestone made **zero** edits to tracked
or untracked files outside of writing this report. (`git diff --check` clean;
`cargo fmt --all -- --check` clean.)

Baseline documents read: `docs/aac-enc-m7-release-readiness.md` (full),
`rust/docs/vbr-profiles.md` (full), root `README.md`, `rust/README.md`,
`rust/convert/README.md`, `rust/docs/releases/0.2.0.md`,
`docs/aac-enc-vbr5-profile.md` (audit scope), `rust/Cargo.toml`,
`rust/convert/Cargo.toml`, `.github/workflows/{ci,reference-validation,oracle-repeatability-diagnostic}.yml`.

## 2. Intended release version and rationale

**Intended next release: `0.3.0`.** No version was changed in this milestone.

Rationale:

- Both workspace crates declare `0.2.0` (`rust/Cargo.toml`,
  `rust/convert/Cargo.toml`; `Cargo.lock` agrees; converter path-dependency
  requires `^0.2.0`). The tagged `v0.2.0` release notes describe a
  **fixed-VBR-5-only** encoder. The working tree instead ships six VBR
  profiles, a new VBR-6 default, new public API (`VbrMode`, `VbrProfile`,
  `new_with_mode`/`with_vbr_mode`), and a new CLI surface (`--vbr 1..6`).
- The repository's own precedent (`rust/docs/releases/0.2.0.md` §3) treats
  `0.2.0` as a SemVer minor release for additive converter features with no
  registry stability contract ("this repository does not publish a registry
  contract, and this release makes no new API-stability or performance
  guarantee"). A patch (`0.2.x`) would be wrong for new functionality; a
  major bump is unwarranted pre-1.0 with no stability contract. The default
  output bytestream does change (VBR-5 → VBR-6: e.g. 1959 → 1961 bytes on the
  stereo fixture), which is within 0.x minor scope and is pinned by an
  explicit regression test (`production_default_profile_vbr6_music_baseline_is_unchanged`).
- No `0.3.0` string exists anywhere in the tree yet; no `CHANGELOG` file
  exists (release history lives in `rust/docs/releases/`). The bump, the
  `0.3.0.md` notes, and the `v0.3.0` tag are **release-time actions** for the
  release commit, following the established `docs(release): prepare …`
  workflow (commits `1f22103`, `a1bcbad`). M8 performs none of them by
  milestone constraint.

## 3. Resolution status of M7 F-1 through F-9

| M7 finding | Severity (M7) | M8 status |
|---|---|---|
| F-1: 14 `include_str!` snapshots untracked; clean checkout cannot compile | High (process) | **Content-complete, build-verified; commit pending (release procedure).** All 14 files present in tree with verified provenance/format (§4); 4 VBR-5 files byte-unchanged; simulated-commit worktree builds lib + `aac-convert` and passes snapshot/profile tests (§5). Strict `cargo package` still refuses the dirty tree — expected; exact `git add` list in §13. |
| F-2: FDK-decoder round-trip + phase-1 oracle parity + M5.2 C-stream comparison unexecutable | Medium | **Partially closed.** Decoder part CLOSED: `aac-dec` built from the pinned in-history source (`da00af1…`, CI procedure) and all 3 decoder-backed tests pass (§6). Oracle part (9 phase-1 + M5.2) remains BLOCKED: private reference checkout unavailable, no token provisioned. Compensating evidence: 20-point C-init parity green, all byte/hash pins green. |
| F-3: stale VBR-5 benchmark pins in `aac_bench_wav.rs` | Medium (fixed in M7) | **Verified still fixed.** Example pins the VBR-6 default streams; comment states the profile; VBR-5 totals retained as contrast. Example compiles; new VBR-6 regression test passes. |
| F-4: `releases/0.2.0.md` VBR-5-only wording vs new tree; multi-profile work needs its own notes | Informational | **Carried → M8-H1.** Historical notes correctly left untouched. The 0.3.0 notes do not exist yet (required content listed in §9). |
| F-5: `Encoder::vbr_mode()` comment cleanup | Low (fixed in M7) | **Verified still fixed** (factual comment; behaviour unchanged). |
| F-6: `docs/README.md` VBR-5 wording + profile-reference link | Low (fixed in M7) | **Verified still fixed** (links `rust/docs/vbr-profiles.md` as authoritative). |
| F-7: rustfmt violations in 5 test modules | Low (fixed in M7) | **Verified still clean**: `cargo fmt --all -- --check` exits 0. |
| F-8: synthetic VBR-3 transient cell below VBR-2 | Informational | **Carried, unchanged.** No algorithm change in M8; pins green. Deferred to future quality evaluation; no claim made. |
| F-9: `esds` declares `maxBitrate = avgBitrate = 0` | Informational | **Carried, unchanged.** Honest "unknown" encoding, documented; players ignore it. |

## 4. Required source/snapshot/fixture tracking audit

### 4.1 `include_str!` snapshots (18 required, 18 present)

`rust/src/aac_enc/psy_config.rs` embeds one snapshot per `(sample_rate,
per-channel-bitrate)` operating point. Required set vs M7: the 4 tracked
VBR-5/VBR-6 files **plus** the 14 VBR-1..4 files.

| File | Tracked? | Profile association (per-channel bitrate) | sr header | Lines | SHA-256 |
|---|---|---|---|---|---|
| `cfg_44100_20000.txt` | ready-to-track | VBR-1 stereo (20k) | 44100 | 18 | `1ac8c980…ad1f5` |
| `cfg_44100_32000.txt` | ready-to-track | VBR-1 mono / VBR-2 stereo (32k, byte-identical FDK dumps, one file) | 44100 | 18 | `8dae25f3…2731d` |
| `cfg_44100_40000.txt` | ready-to-track | VBR-2 mono (40k) | 44100 | 18 | `4bf04bda…e8487a` |
| `cfg_44100_48000.txt` | ready-to-track | VBR-3 stereo (48k) | 44100 | 18 | `17c2a2f6…52b7710` |
| `cfg_44100_56000.txt` | ready-to-track | VBR-3 mono (56k) | 44100 | 18 | `8d24b0cd…2d291f8` |
| `cfg_44100_64000.txt` | ready-to-track | VBR-4 stereo (64k) | 44100 | 18 | `754f5d3d…43a809` |
| `cfg_44100_72000.txt` | ready-to-track | VBR-4 mono (72k) | 44100 | 18 | `4bc78165…3f76c2d` |
| `cfg_44100_96000.txt` | TRACKED, unchanged | VBR-5/VBR-6 stereo (96k) | 44100 | 18 | `11c8de1b…5ec17d8` |
| `cfg_44100_112000.txt` | TRACKED, unchanged | VBR-5/VBR-6 mono (112k) | 44100 | 18 | `ba2a2ee8…4e0745` |
| `cfg_48000_20000.txt` | ready-to-track | VBR-1 stereo (20k) | 48000 | 18 | `dfcab408…81feb076` |
| `cfg_48000_32000.txt` | ready-to-track | VBR-1 mono / VBR-2 stereo (32k) | 48000 | 18 | `a52bd071…8cb46c22` |
| `cfg_48000_40000.txt` | ready-to-track | VBR-2 mono (40k) | 48000 | 18 | `62a429db…1c237551` |
| `cfg_48000_48000.txt` | ready-to-track | VBR-3 stereo (48k) | 48000 | 18 | `8b92c964…a9cba486` |
| `cfg_48000_56000.txt` | ready-to-track | VBR-3 mono (56k) | 48000 | 18 | `5a9d857e…778455dd` |
| `cfg_48000_64000.txt` | ready-to-track | VBR-4 stereo (64k) | 48000 | 18 | `d5028800…c8b11fcdf` |
| `cfg_48000_72000.txt` | ready-to-track | VBR-4 mono (72k) | 48000 | 18 | `2231a97d…636d906f` |
| `cfg_48000_96000.txt` | TRACKED, unchanged | VBR-5/VBR-6 stereo (96k) | 48000 | 18 | `27d895f0…a5c348090` |
| `cfg_48000_112000.txt` | TRACKED, unchanged | VBR-5/VBR-6 mono (112k) | 48000 | 18 | `8c2ef084…b418b737` |

(Full hashes in the audit log; prefixes above. `git diff`/`git status`
confirm zero modifications to the 4 tracked files.)

Verified per file: exact `testdata/cfg_<sr>_<pch>.txt` path matching the
`include_str!` literal; `sr=` header matches the filename rate; 18-line
layout (`CFG LONG` + 7 arrays + `TNSCFG` + `CFG SHORT` + 7 arrays +
`TNSCFG`); profile association via `VbrProfile::channel_bitrate`
(mono/stereo table in `config.rs:331-400` cross-checked against
`vbr-profiles.md` §"Authoritative profile reference table": mono
32/40/56/72/112/112, stereo-per-channel 20/32/48/64/96/96, bandwidths
13000/13000/15750/16500/19293/20000 — all match). Provenance: generated by
driving UNMODIFIED FDK-AAC v3.0.0 (`mstorsjo/fdk-aac` tag `v3.0.0`, commit
`a68a51aa…`) through the real init path; the generator (`tools/enc-oracle/
dump_vbr_init_configs.cpp`, untracked, to be committed) re-emits the 4
pre-existing files byte-identically as a self-check. The parse-guard test
`all_validated_snapshots_parse` and the resolution test
`every_profile_resolves_to_exact_snapshot` (all 6 modes × 2 rates ×
mono/stereo) both pass.

### 4.2 Other `include_*` dependencies

- `include_bytes!` WAV fixtures (`m7-music-stereo-48k.wav`,
  `m7-speech-pauses-stereo-48k.wav`, `m7-speech-clean-stereo-48k.wav`,
  `m8-music-eroica-stereo-48k.wav`): all 4 **tracked** (`git ls-files`
  confirms); `testdata.rs` fixture keys resolve.
- New untracked non-snapshot files and their intended disposition:
  `rust/docs/vbr-profiles.md` (required doc), `rust/examples/aac_bench_wav.rs`
  (bench example with corrected VBR-6 pins), `rust/examples/m7_audit_{sweep,
  spectral}.rs` (audit-only harnesses; compile, not wired into CI),
  `tools/enc-oracle/dump_vbr_init_configs.cpp` (oracle provenance tooling),
  `docs/aac-enc-m7-release-readiness.md` (M7 audit record). All are intended
  for version control at release-commit time (audit harnesses explicitly
  marked "delete after the audit" in their headers; committing them preserves
  the evidence trail — retention vs deletion is the owner's call).

### 4.3 What must be committed

At release-commit time: `git add` the 27 modified files (§1) and the 20
untracked files above (at minimum the 14 snapshots + `vbr-profiles.md` +
`dump_vbr_init_configs.cpp` + the bench example). Nothing in the required set
is generated, downloaded, or license-ambiguous.

## 5. Clean-checkout build results

The critical acceptance test was run with an **isolated temporary worktree**,
never the working tree:

1. `git worktree add --detach /tmp/ferrite-m8-clean a1bcbad` (clean HEAD).
   Clean-HEAD lib build: PASS (4-snapshot VBR-5 code, as expected).
2. Overlaid exactly the working-tree content (`git diff HEAD --name-only` +
   `git ls-files --others --exclude-standard`, 47 paths) onto the clean
   worktree — i.e. the byte-exact intended commit content, still uncommitted.
   - `cargo build -p ferrite-aac --features aac_enc`: **PASS**.
   - `cargo build -p aac-convert`: **PASS**.
   - `cargo test … psy_config` (3 snapshot/profile-resolution tests): **PASS**.
   - VBR filter set (29 tests incl. C-init parity + VBR-6 baseline): **PASS**.
   - `cargo test … --test production_api`: **6 passed**.
3. Failure-mode proof: deleting one snapshot from the simulation fails the
   build at compile time (`error: couldn't read
   src/aac_enc/testdata/cfg_48000_20000.txt`), confirming the M6.3
   fail-closed mechanism — a missing file can never become a runtime or
   deployment surprise.
4. `cargo package -p ferrite-aac --list` on the simulation: **refused** (37
   changed paths, dirty tree — Cargo policy, not a content problem). With
   `--allow-dirty` (read-only listing): all **18** `cfg_*.txt` files are in
   the package list. The strict gate must be re-run on the clean release
   checkout after the commit (release-time action, §12 M8-H1).

No claim is made beyond this: a *true* clean-checkout proof requires the
release commit itself. §13 gives the exact file list that commit must contain.

## 6. External oracle and decoder tooling status

Environment: `PHASE1_ORACLE_DIR`, `PHASE1_C_DIR`, `M55_C_DIR`, `AAC_DEC_BIN`
all **unset**. No reference checkout present. `fdkaac` on PATH is the encoder
frontend only. `ffmpeg`/`ffprobe`/`afinfo` available.

| Artifact | Needed for | Status in this environment |
|---|---|---|
| Ordinary `cargo build` / `cargo test` (Tier-A) | production builds, default suite | **Not needed.** Verified: Tier-A green with all four vars unset (§7). |
| `PHASE1_ORACLE_DIR` + `PHASE1_C_DIR` (9 phase-1 tests) | optional reference-validation tests only | **BLOCKED.** Private repo `whepper/fdk-aac-reference` not present; pin file notes it is "not yet a published remote dependency"; owner token not provisioned. Setup documented (`using-the-fdk-aac-reference.md`, `check-reference.sh`, `enc-oracle/README.md`, `run-reference-validation.sh --validate`). No substitution attempted (a public-tree stand-in would fail `check-reference.sh` and violate the no-substitution rule). |
| `M55_C_DIR` (`m55_streams_match_c_reference`) | optional full-stream C comparison | **BLOCKED**, same reason. |
| `AAC_DEC_BIN` (`m5a_decode_check` + 2 decode-backed tests) | optional decoder round-trip tests | **RESOLVED in M8.** Built per the repo's own documented procedure (CI `decode-validation` job + `docs/aac-enc-m5a-decode-check.md`): `git worktree add --detach … da00af1…` (in-history FDK-derived tree), verified `git rev-parse HEAD == da00af1ca9624a213fb53c9ac52f6fec55e9b2af`, `cmake -DBUILD_PROGRAMS=ON … --target aac-dec`. Binary SHA-256 `0f9cdb4a…66d75b1f`. **No production dependency introduced** (test-only `AAC_DEC_BIN`; no vendoring; the real FDK `aac-dec.c`, not a substitute). |

Decoder results (release-relevant, all with locally built `aac-dec`):

- `m5a_decode::m5a_tests::m5a_decode_check`: **PASS** (exit 0 + empty stderr +
  structural WAV checks; implied trims 1744/1685 samples as documented).
- `m52_tests::crash_recovered_stream_decodes`: **PASS**.
- `is_integration_tests::intensity_probe_adts_decodes`: **PASS**.
- `rust/tools/check-decode-evidence.sh` on the three logs: **OK**.

This closes the FDK-decoder third of M7 F-2. The 10 remaining Tier-B tests
(9 phase-1 + M5.2) fail loudly with `reference validation artifact missing`
— by design, never silently — and are reported as BLOCKED, not passed (§7).
Release eligibility: a source-only release may ship with these documented as
not-executed (0.2.0 precedent with 11), but must not claim their coverage;
they remain mandatory in the next environment holding the private checkout.

## 7. Complete test and validation matrix

All commands run from `rust/` with toolchain 1.99.1. "BLOCKED" = missing
external artifact (fails loudly); nothing is skipped silently.

| # | Command | Result |
|---|---|---|
| 1 | `cargo test --locked --features aac_enc --lib` (default env) | 350 passed, **11 BLOCKED** (`m5a_decode_check`, `m55_streams_match_c_reference`, 9 × `phase1*`), 1 ignored (opt-in bench). All 11 fail with `reference validation artifact missing`. |
| 2 | Same lib suite, CI Tier-A skip list (15 skips) | **346 passed**, 0 failed, 1 ignored |
| 3 | Lib suite with `AAC_DEC_BIN`=locally built `aac-dec` (§6): the 3 decoder-backed tests | **3/3 PASS** (`m5a_decode_check`, `crash_recovered_stream_decodes`, `intensity_probe_adts_decodes`) + `check-decode-evidence.sh` OK |
| 4 | `cargo test --locked --features aac_enc --test production_api` | **6 passed** |
| 5 | `cargo test --locked --features aac_enc --doc` | **19 passed** |
| 6 | `cargo test --locked -p aac-convert` | **261 passed** (80 lib + 62 batch + 18 cli + 30 conversion + 34 coverart + 16 loudness + 21 metadata), 0 failed, 2 ignored |
| 7 | `cargo test --locked -p aac-convert -- --ignored` (ffprobe/decoder-gated) | **4 passed** (coverart, metadata, 2 × independent_decode), needs `ffprobe` — present |
| 8 | Encoder-specific: `vbr_profiles_match_c_reference_init` (20 operating points) | **PASS** |
| 9 | Encoder-specific: VBR-5 baselines + `production_default_profile_vbr6_music_baseline_is_unchanged` | **PASS** |
| 10 | Encoder-specific: M5/M6 regression + migration (`m6_`, `m5*` non-oracle) | **PASS** (11/11 in filter set) |
| 11 | Crash recovery (7 tests) | **PASS** |
| 12 | CLI audit (release build): default VBR-6, `--vbr 1..6`, `--vbr=N`, aliases, invalid `7`→exit 2, M4A+ADTS mono/stereo | **PASS** (byte-verified diag lines; `ffprobe`/`ffmpeg` decode M4A AAC-LC + ADTS AAC-LC clean) |
| 13 | `cargo check` (default + `--no-default-features --features aac_enc`) | **PASS** (20 pre-existing warnings, 0 errors) |
| 14 | `cargo fmt --all -- --check` | **PASS** (clean) |
| 15 | `cargo clippy --locked --features aac_enc --all-targets` | informational: 211 warnings, **0 errors** (upstream-mirroring style lints; CI non-gating) |
| 16 | `tools/check-licensing.sh` | **PASS** (67 files) |
| 17 | `git diff --check` | **PASS** (clean) |
| 18 | `cargo package -p ferrite-aac --list` (strict) | **BLOCKED by dirty tree** (expected pre-commit); `--allow-dirty` listing confirms all 18 snapshots packaged |
| 19 | Bitrate/spectral behaviour | **Carried from M7** (216-stream/111k-frame sweep; no algorithm change since; pins in #8–#9 green). NOT re-run in M8 (audit harnesses retained untracked, not CI-wired). |
| 20 | `cargo build --examples` (incl. untracked audit harnesses) | **PASS** (2 minor warnings in `m7_audit_sweep`, audit-only, untracked) |

Not green, not hidden: row 1's 11 BLOCKED (10 after §6's decoder build) and
row 18's dirty-tree refusal. Everything else passes.

## 8. Licensing and provenance audit

Consistent across all layers:

- Root `NOTICE` preserves the upstream FDK-AAC license text verbatim; no
  header was removed, shortened, or reformatted (`git diff` shows zero
  removed Fraunhofer/FDK/SPDX/Ferrite header lines across the 27 modified
  files).
- `rust/NOTICE` carries the prominent Third-Party Modified Version notice
  (product, upstream identification, non-affiliation, © 2025–2026 Jeroen
  Steenvoorde, modification date 2026-10-26) plus the complete license text.
- `rust/LICENSE-FDK-AAC` present; manifests declare `license = "FDK-AAC"`
  (`ferrite-aac` additionally `license-file = "LICENSE-FDK-AAC"` so the
  source package carries the text; `aac-convert` relies on the workspace
  bundle model documented in `rust/convert/THIRD-PARTY-NOTICES.md`).
- All 67 `rust/src/**/*.rs` files carry the modified-version notice
  (`check-licensing.sh` PASS). New non-codec files (examples, oracle `.cpp`,
  docs) correctly carry **no** FDK header per the new-file policy (standalone
  utilities, not codec files); the oracle generator header states
  test-only provenance from UNMODIFIED v3.0.0.
- FDK-derived sources remain attributed as upstream; VBR-6 is identified
  everywhere (code `VbrOrigin::FerriteExtension`, docs, CLI help) as a
  Ferrite extension — never as upstream FDK. The 14 new snapshots' provenance
  (unmodified-v3.0.0 init path + byte-identical re-emission self-check) is
  recorded in `vbr-profiles.md`, `psy_config.rs`, and the generator header.
- READMEs: licensing section lives in the root README (canonical);
  `rust/README.md` defers to it; no legal conclusions drawn here — the
  inventory (`docs/aac-enc-provenance-inventory.md`) is explicitly
  evidence-only and revision-pinned at 2026-10-27/pre-multi-profile (see
  M8-M2). No binary is distributed, so binary-redistribution terms are not
  exercised; patent authorization remains a separate qualified assessment.

## 9. Version and release metadata audit

| Reference | Value | Consistent? |
|---|---|---|
| `rust/Cargo.toml` (`ferrite-aac`) | 0.2.0 | yes |
| `rust/convert/Cargo.toml` (`aac-convert` + path-dep `^0.2.0`) | 0.2.0 | yes |
| `rust/Cargo.lock` (both crates) | 0.2.0 | yes |
| `aac-convert --version` (release build) | `aac-convert 0.2.0` | yes (derives from `CARGO_PKG_VERSION`) |
| M4A `©too` atom | `Ferrite 0.2.0` | yes |
| `rust/docs/user-manual.md` header / `encoder-integration.md` | 0.2.0 | **stale-by-content**: both describe six-profile behaviour while labelled 0.2.0 (whose notes are VBR-5-only). Release-time update to 0.3.0 (M8-H1), not an M8 edit. |
| `rust/docs/releases/0.1.0.md`, `0.2.0.md` | VBR-5-only wording | correct as historical record; must not be edited (M7 F-4) |
| `rust/docs/releases/0.3.0.md` | — | **missing** (M8-H1) |

Required 0.3.0 notes content (to be written at release time): six AAC-LC VBR
profiles; VBR-6 default; VBR-5 upstream-compatible baseline; Ferrite-specific
20 kHz VBR-6 bandwidth; `--vbr 1..6` selection; API additions (`VbrMode`,
profiles); validation performed (Tier-A green, decoder round-trip green,
Tier-B oracle disclosure); known limitations. Must explicitly state:
HE-AAC v1/v2 **not implemented**, SBR **not implemented**, Parametric Stereo
**not implemented**, and **no perceptual-superiority claim** for VBR-6. Must
not present unavailable FDK-decoder validation as completed (it is now
completed for the 3 decoder tests — say exactly that — while C-oracle parity
remains not-executed here).

## 10. Documentation changes

M8 made **no documentation edits** (no stale or misleading statement was
found in maintained docs; rewrites avoided per the milestone brief). State of
the tree's uncommitted documentation (all verified against implementation):

- `rust/docs/vbr-profiles.md` (new): authoritative six-profile reference.
  Table values verified equal to `VbrProfile::for_mode`; provenance section
  documents the oracle workflow; selection examples use valid syntax
  (`new_with_mode`, `with_vbr_mode` signatures confirmed); CLI examples match
  the verified parser (`--vbr 1..6`, `--vbr=N`, aliases, exit-2 errors).
- Modified docs (`README.md`, `docs/README.md`, `rust/README.md`,
  `rust/convert/README.md`, `encoder-integration.md`, `user-manual.md`,
  `aac-convert-integration-contract.md`): six-profile updates, all accurate —
  VBR-6 default stated everywhere; VBR-5 kept as upstream baseline; AAC-LC
  only; no HE-AAC/SBR/PS; afterburner-on/PNS-off documented; nominal-vs-
  measured bitrate wording correct ("nominal", muxer declares 0/0);
  VBR-6 described as configuration/bandwidth, with the "not a guaranteed
  cutoff / no listening claim" caveats intact. The `convert/README.md:86`
  parenthetical ("VBR-6 is the Ferrite default (VBR-5 with 20 kHz
  bandwidth)") reads correctly in full context — not the VBR-5-default
  misreading a grep fragment suggests.
- All README-referenced doc paths resolve (checked all 9); API doc examples
  compile (19 doctests pass); build instructions (`cd rust` first, MSRV
  1.99.1) verified from the simulated clean checkout; reference tooling is
  fenced off as manual-only in the root README and `using-the-fdk-aac-
  reference.md`; licensing is one click from the root README.
- Release-time doc actions (not M8 edits): version labels → 0.3.0, new
  `releases/0.3.0.md` (§9), tag.

## 11. CI/reproducibility findings

- Standard CI (`ci.yml`, push/PR): default + encoder-only builds, Tier-A
  self-contained lib tests (15 Tier-B skips), public-API integration tests,
  doctests, `cargo package`, `aac-convert` build+test, and the
  **decode-validation job** (builds `aac-dec` from pinned public commit
  `da00af1…` with SHA verification — no secret, runs per-PR). Format/Clippy
  are informational non-gating. Manual workflows (`reference-validation.yml`,
  `oracle-repeatability-diagnostic.yml`) are `workflow_dispatch`-only and
  never required for normal development — the Tier-B/private-checkout
  distinction is explicit everywhere (`using-the-fdk-aac-reference.md`,
  `preflight-validation.sh`, test panic messages).
- A developer cloning the repo builds and runs the ordinary suite with no
  private artifacts: verified (Tier-A 346 + 6 API + 19 doc + 261 converter
  tests green, env vars unset; only Tier-B tests demand artifacts, loudly).
- Snapshot coverage in CI: missing-tracked-snapshot fails compilation
  (fail-closed, demonstrated §5); `all_validated_snapshots_parse` guards all
  18; `cargo package` additionally refuses dirty trees. No new CI job is
  required; an explicit snapshot-manifest check would be redundant but
  harmless (M8-L2).

## 12. Remaining findings by severity

**Critical: none.** No clean-checkout breakage in the intended content, no
licensing defect, no corrupted output, no packaging defect.

**High:**

- **M8-H1 — release commit, 0.3.0 bump, notes, and tag outstanding.**
  Mechanical but mandatory before any publish: commit the §13 file set;
  bump both crates to 0.3.0 (+ lockfile + path-dep); write
  `rust/docs/releases/0.3.0.md` (§9 content); update version labels in
  `user-manual.md`/`encoder-integration.md`; re-run `cargo package
  --locked -p ferrite-aac --list` on the clean checkout; tag `v0.3.0`.
  Fully specified; no unknowns.

**Medium:**

- **M8-M1 — Tier-B C-oracle parity not executed here** (9 phase-1 tests +
  `m55_streams_match_c_reference` BLOCKED; private checkout + token
  unavailable). Compensating evidence: 20-point `QCInit`/init parity green,
  all byte/hash pins green, FDK-decoder round-trip green, two independent
  decoders per M7 with no algorithm change since. Shippable source-only with
  explicit disclosure (0.2.0 precedent); must not claim this coverage; rerun
  where the private checkout exists.
- **M8-M2 — provenance inventory predates the multi-profile work**
  (revision-pinned 2026-10-27 by design). New-artifact provenance is carried
  in `vbr-profiles.md`/`psy_config.rs`/generator header. Regenerating the
  inventory is follow-up work, not a release blocker.

**Low:**

- **M8-L1** — stale absolute `/private/var/…` fallback paths in test-only
  oracle env defaults (fail loudly by design; cosmetic).
- **M8-L2** — no explicit CI snapshot-manifest job (compilation + parse test
  already fail-closed; optional hardening only).

**Informational (accepted limitations, not defects):**

- M8-I1: `releases/0.1.0.md`/`0.2.0.md` describe VBR-5-only — correct history.
- M8-I2: no perceptual/listening/ABX claim for any profile (VBR-6 bandwidth
  validated as configuration + decoded-spectral behaviour only, per M7 §8).
- M8-I3: `esds` 0/0 bitrates (honest "unknown"); silence → minimal frames;
  VBR-3 synthetic-transient observation (M7 F-8); snapshot coverage is
  production rates (44.1/48 kHz) with fail-closed panic elsewhere — all
  unchanged and documented.

## 13. Exact files changed

Uncommitted at report time (no staged entries; HEAD still `a1bcbad`).
Release commit must contain at minimum the 14 snapshots; the full intended
set is:

Modified tracked (27): `README.md`, `docs/README.md`, `rust/README.md`,
`rust/convert/README.md`, `rust/convert/src/batch.rs`,
`rust/convert/src/lib.rs`, `rust/convert/src/main.rs`,
`rust/convert/tests/batch.rs`, `rust/convert/tests/cli.rs`,
`rust/docs/aac-convert-integration-contract.md`,
`rust/docs/encoder-integration.md`, `rust/docs/user-manual.md`,
`rust/src/aac_enc.rs`, `rust/src/aac_enc/bitcount.rs`,
`rust/src/aac_enc/config.rs`, `rust/src/aac_enc/encoder.rs`,
`rust/src/aac_enc/m5a_decode.rs`, `rust/src/aac_enc/m5b_measure.rs`,
`rust/src/aac_enc/m5g_census.rs`, `rust/src/aac_enc/m6_migration.rs`,
`rust/src/aac_enc/pe.rs`, `rust/src/aac_enc/production_baseline.rs`,
`rust/src/aac_enc/psy.rs`, `rust/src/aac_enc/psy_config.rs`,
`rust/src/aac_enc/quant.rs`, `rust/src/aac_enc/sfb.rs`,
`rust/src/aac_enc/stream_driver.rs`.

Untracked, intended for version control (20): `docs/aac-enc-m7-release-
readiness.md`, `rust/docs/vbr-profiles.md`,
`rust/examples/aac_bench_wav.rs`, `rust/examples/m7_audit_spectral.rs`,
`rust/examples/m7_audit_sweep.rs`, the 14 `cfg_*.txt` snapshots listed in
§4.1, `tools/enc-oracle/dump_vbr_init_configs.cpp`. (Plus this report,
`docs/aac-enc-m8-release-preparation.md`.)

M8 itself edited no file except this report.

## 14. Final release go/no-go recommendation

**GO WITH DOCUMENTED LIMITATIONS**, conditional on the release procedure in
M8-H1 being executed as specified (commit §13 set → 0.3.0 bump + notes →
clean-checkout `cargo package` re-run → tag; no binary publication without
the separate licensing/patent review already flagged in the release-notes
template).

The encoder is technically validated: six profiles encode valid,
independently decodable AAC-LC; defaults agree on VBR-6 everywhere; all
ordinary tests pass without private artifacts; licensing and attribution are
correct and complete; documentation matches the implementation; the M7 FDK-
decoder gap is closed with a pinned, verifiable, test-only decoder build.
What remains is process, not substance: a fully specified commit/version/tag
sequence, honest disclosure of the environmentally blocked Tier-B oracle
tests, and the standing rule that no perceptual claim may be attached to
VBR-6. A technically validated encoder is not yet a published release — after
M8-H1, it will be ready to become one.
