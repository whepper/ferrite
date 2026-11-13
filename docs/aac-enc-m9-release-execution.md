# M9 — Ferrite 0.3.0 Release Execution

Release-execution record for the six-profile (VBR-1 through VBR-6) encoder
tree. This milestone performed the release preparation, version bump, release
notes, package verification, full validation, one release commit, and one
annotated tag. The commit and tag were subsequently published to `origin`
after owner approval (§15); no crates, binaries, or GitHub release were
published.

| Item | Value |
|---|---|
| Baseline revision | `a1bcbada51a50ac0fcc92e84698807890ebae5e6` (`a1bcbad`, `v0.2.0`) |
| Release commit | `f6757d5dc7452cf821d9fd71323b0706715c83ee` |
| Commit subject | `chore(release): prepare Ferrite 0.3.0` |
| Annotated tag | `v0.3.0` (tag object `f9a010d81c731a30749c8b4ea1d9bb9a4ae1c1c4`) |
| Tag target | `f6757d5dc7452cf821d9fd71323b0706715c83ee` |
| Release version | `0.3.0` (both crates) |
| Push performed | **Yes** — `main` and annotated `v0.3.0` pushed to `origin` after approval (§15) |
| GitHub release published | **No** (latest remains `Ferrite 0.2.0`) |
| crates.io publication | **No** |
| Binaries uploaded | **No** |
| Verdict | **GO WITH DOCUMENTED LIMITATIONS**; approved and published |

Document status: this report was intentionally **untracked** at the approval
gate. It records the commit it validates, so it could not be part of that
commit without self-reference. It is committed as a documentation-only
follow-up after publication. The M7 and M8 audit records and the 0.3.0 release
notes are part of the release commit.

---

## 1. Baseline revision and working-tree inventory

Baseline checks before any edit:

| Check | Result |
|---|---|
| Branch | `main`, upstream `origin/main`, `+0 −0` |
| HEAD | `a1bcbada51a50ac0fcc92e84698807890ebae5e6` — matches the M8 baseline |
| `git describe` | `v0.2.0-dirty` |
| Staged files | none |
| Modified tracked | 27 |
| Untracked | 21 |
| Local tags | `v0.1.0`, `v0.2.0`, `v3.0.0` |
| Remote tags | `v0.1.0`, `v0.2.0`, `v3.0.0` |
| GitHub releases | `Ferrite 0.2.0` (latest), `v0.2.0`, 2026-11-12 |
| Stashes | none |

The working tree was **not clean**, but it matched the state documented in
M8 §1 and §13 exactly: 27 modified tracked files (the multi-profile
implementation plus M7 audit fixes) and 20 intended untracked files plus the
M8 report itself. No divergence from the M8 baseline was found. No file was
discarded, overwritten, or silently included.

Ignored content consisted only of `rust/target/` and three `__pycache__/`
directories; none was part of the release.

Constraints re-confirmed: AAC-LC only, VBR-1 through VBR-6, VBR-6 default,
VBR-5 upstream-compatible baseline, `aac-convert` supported alongside the
Rust encoder API. No encoder algorithm change was made in M9.

## 2. Findings disposition

| Finding | Status after M9 |
|---|---|
| **M8-H1** — release commit, 0.3.0 bump, notes, package gate, tag outstanding | **CLOSED.** Release commit `f6757d5`; both crates and lockfiles at 0.3.0; `rust/docs/releases/0.3.0.md` written; strict `cargo package -p ferrite-aac` passed on a clean checkout; annotated `v0.3.0` created and verified. |
| **M8-M1** — Tier-B C-oracle parity not executed | **CARRIED, BLOCKED.** Private reference checkout still unavailable. No substitute used. Ten tests remain not-executable and are reported as BLOCKED, never as passed. See §10. |
| **M8-M2** — provenance inventory predates multi-profile work | **PARTIALLY ADDRESSED.** An addendum (§H) records the scope boundary and points to the existing, evidence-backed provenance record in `rust/docs/vbr-profiles.md`; the pinned inventory rows were not rewritten. Full regeneration remains follow-up work. |
| **M8-L1** — stale absolute `/private/var/...` fallbacks in test-only oracle defaults | **ACCEPTED (Low).** Pre-existing at `a1bcbad`; confined to `#[cfg(test)]` phase-1 modules; never used by normal builds; unset artifacts fail loudly. Recorded as M9-L2. |
| **M8-L2** — no explicit CI snapshot-manifest job | **ACCEPTED (Low).** Missing snapshots fail at compile time (`include_str!`), and `all_validated_snapshots_parse` covers all 18. Optional hardening only. |
| **M8-I1** — historical release notes describe VBR-5-only | **ACCEPTED.** Correct history; 0.1.0/0.2.0 notes not edited. |
| **M8-I2** — no perceptual/ABX claim | **ACCEPTED.** No claim added; VBR-6 described as configuration and spectral behaviour only. |
| **M8-I3** — `esds` 0/0, silence frames, VBR-3 transient observation | **ACCEPTED.** Unchanged and documented. |

Additional release-critical hygiene resolved during M9:

- `rust/docs/vbr-profiles.md` ended with an unbalanced code fence (21 fences;
  stray closing fence at line 304). Removed; the document now has 20 balanced
  fences and parses as intended.
- The two audit-only examples carried stale "delete after the audit"
  wording while being retained as evidence. Their headers now state that
  they are audit-only, retained, and not CI-wired.

## 3. Release contents

The release commit changes **55 files**: 33 modified and 22 added
(+5 291 / −218). No deletions, no file-mode changes, no binary files.

Modified (33):

- Repository/docs: `README.md`, `docs/README.md`,
  `docs/aac-enc-provenance-inventory.md`.
- Workspace metadata: `rust/Cargo.toml`, `rust/Cargo.lock`, `rust/README.md`,
  `rust/research/spike-m4a/Cargo.lock`.
- Converter: `rust/convert/Cargo.toml`, `rust/convert/README.md`,
  `rust/convert/THIRD-PARTY-NOTICES.md`,
  `rust/convert/src/{batch,lib,main}.rs`,
  `rust/convert/tests/{batch,cli}.rs`.
- Guidance: `rust/docs/aac-convert-integration-contract.md`,
  `rust/docs/encoder-integration.md`, `rust/docs/user-manual.md`.
- Encoder: `rust/src/aac_enc.rs` and
  `rust/src/aac_enc/{bitcount,config,encoder,m5a_decode,m5b_measure,m5g_census,m6_migration,pe,production_baseline,psy,psy_config,quant,sfb,stream_driver}.rs`.

Added (22):

- `docs/aac-enc-m7-release-readiness.md`
- `docs/aac-enc-m8-release-preparation.md`
- `rust/docs/releases/0.3.0.md`
- `rust/docs/vbr-profiles.md`
- `rust/examples/{aac_bench_wav,m7_audit_spectral,m7_audit_sweep}.rs`
- 14 `rust/src/aac_enc/testdata/cfg_*.txt` snapshots (§6)
- `tools/enc-oracle/dump_vbr_init_configs.cpp`

Release-critical checks on the staged set:

- All 18 snapshots are present in the commit tree.
- The four pre-existing VBR-5/VBR-6 snapshots are byte-identical to the
  baseline (no tracked snapshot is modified; `git diff --quiet HEAD --
  rust/src/aac_enc/testdata/` before commit was empty).
- No build artifacts, `__pycache__`, `.DS_Store`, temporary files, local
  environment files, or private oracle data are included.
- No license or notice file was modified; `NOTICE`, `rust/NOTICE` and
  `rust/LICENSE-FDK-AAC` retain their complete text.
- Every file required for a normal build was included: the 14 snapshots are
  `include_str!` build inputs, and the staged clean-checkout build (§8)
  proves the set is complete.

## 4. Version changes

| Reference | Before | After |
|---|---|---|
| `rust/Cargo.toml` (`ferrite-aac`) | 0.2.0 | **0.3.0** |
| `rust/convert/Cargo.toml` (`aac-convert`) | 0.2.0 | **0.3.0** |
| `ferrite-aac` path dependency in `aac-convert` | `^0.2.0` | **`^0.3.0`** |
| `rust/Cargo.lock` (both packages) | 0.2.0 | **0.3.0** |
| `rust/research/spike-m4a/Cargo.lock` | 0.2.0 | **0.3.0** |
| `rust/convert/README.md` packaging statement | `^0.2.0` | **`^0.3.0`** |
| `rust/convert/THIRD-PARTY-NOTICES.md` first-party rows | 0.2.0 | **0.3.0** |
| `rust/docs/encoder-integration.md` | 0.2.0 | **0.3.0** |
| `rust/docs/user-manual.md` version/release/tag/limits | 0.2.0, 2026-11-12 | **0.3.0, 2026-11-13** |
| `aac-convert --version` | `aac-convert 0.2.0` | **`aac-convert 0.3.0`** (derived from `CARGO_PKG_VERSION`) |
| M4A `©too` atom | `Ferrite 0.2.0` | **`Ferrite 0.3.0`** (derived from `CARGO_PKG_VERSION`) |

`cargo metadata` resolves `ferrite-aac 0.3.0` and `aac-convert 0.3.0`.
Definitive command:

```text
cargo metadata --format-version 1 --no-deps
aac-convert 0.3.0
ferrite-aac 0.3.0
```

No dependency version changed. Only the two package versions and the
workspace path-dependency requirement changed; the lockfile diffs are
version-line-only.

Remaining `0.2.0` strings are confined to the historical release notes
(`rust/docs/releases/0.1.0.md`, `0.2.0.md`), which correctly describe the
tagged releases and were deliberately not edited.

## 5. Release notes

`rust/docs/releases/0.3.0.md` follows the established source-only release-note
convention of 0.1.0/0.2.0. It covers:

- Multi-profile AAC-LC encoding: VBR-1 through VBR-6, VBR-6 default, VBR-5
  upstream baseline, the Ferrite 20 000 Hz VBR-6 bandwidth, profile-specific
  psychoacoustic initialisation, and the 18-snapshot coverage.
- Encoder and converter: native Rust AAC-LC, public profile-selection API,
  `aac-convert --vbr 1..6`, ADTS and M4A output, mono/stereo.
- Compatibility and migration: default output changes from VBR-5 to VBR-6,
  how to pin VBR-5, additive CLI/API surface, `©too` version movement,
  SemVer rationale.
- Validation: the matrix in §8, with the ten blocked Tier-B tests explicitly
  marked BLOCKED and not claimed as passed.
- Known limitations: Tier-B oracle unavailable, no perceptual-superiority
  claim for VBR-6, HE-AAC v1/v2, SBR and PS not implemented, not bit-exact
  with FDK, production matrix, VBR-3 transient observation, `esds` 0/0, no
  standalone `aac-convert` package.
- Source-only licensing/distribution status: no binary distributed; FDK
  license and patent review remain separate assessments.

No performance, quality, compatibility, or FDK-equivalence claim was
exaggerated. Ferrite is not described as bit-exact with FDK.

## 6. Snapshot integrity verification

All 18 embedded snapshots exist at the expected `include_str!` paths in
`rust/src/aac_enc/testdata/`. Each file has the expected 18-line layout:

`CFG LONG`, `OFFSETS`, `PCMQ`, `MASKHI`, `MASKLO`, `MASKHIS`, `MASKLOS`,
`MINSNR`, `TNSCFG` (9 long-window lines), followed by the same 9 labels for
the short window. Every `sr=` header matches the rate in the filename; LONG
uses `cnt=49` and SHORT `cnt=14`; `TNSCFG` is present in both blocks.

| File | Profile association (per-channel bitrate) | Lines | SHA-256 |
|---|---|---:|---|
| `cfg_44100_20000.txt` | VBR-1 stereo (20k) | 18 | `1ac8c980dc9217bd15d9a557a0d65de83117c6f4f807e6b0a2d0826e86dad1f5` |
| `cfg_44100_32000.txt` | VBR-1 mono / VBR-2 stereo (32k, shared dump) | 18 | `8dae25f3934bbe8a005f32f8712bbe51e1d3f6cbe75ad788f632610273d951d5` |
| `cfg_44100_40000.txt` | VBR-2 mono (40k) | 18 | `4bf04bdabb32626c35cc871038aabebca08d1af0081212625de00cbf1e8487a8` |
| `cfg_44100_48000.txt` | VBR-3 stereo (48k) | 18 | `17c2a2f633e64cb6cae5531f00322dd234078859ca0a926d68aac705f52b7710` |
| `cfg_44100_56000.txt` | VBR-3 mono (56k) | 18 | `8d24b0cdcd740146c80653a1718fb3eab630f4aea2a276e7a04d4d2d291f8e63` |
| `cfg_44100_64000.txt` | VBR-4 stereo (64k) | 18 | `754f5d3d1adacf32e8ae5a261d7d488260d78bc1eb5e2f0a1e794e281c43a809` |
| `cfg_44100_72000.txt` | VBR-4 mono (72k) | 18 | `4bc781655b5819bf5d4fdf891833837fa6e3529cdb49ddcd26920afd33f76c2d` |
| `cfg_44100_96000.txt` | VBR-5/VBR-6 stereo (96k), tracked | 18 | `11c8de1b4663ebefd9a85c9295efabbfb249372643db5cc2b35fa5ec17d8e39e` |
| `cfg_44100_112000.txt` | VBR-5/VBR-6 mono (112k), tracked | 18 | `ba2a2ee838b4733c0c4e05b451150e6d0481bf8dd1de992ccd2ebe8215ef0745` |
| `cfg_48000_20000.txt` | VBR-1 stereo (20k) | 18 | `dfcab4081f7f5c5242cb9317514969da7b30a4f5171455de147e5818feb076d5` |
| `cfg_48000_32000.txt` | VBR-1 mono / VBR-2 stereo (32k, shared dump) | 18 | `a52bd071531f6d85793acf7d3dd4f0f021a4ae94530b4f5e9a4b5b8c46cb222f` |
| `cfg_48000_40000.txt` | VBR-2 mono (40k) | 18 | `62a429dbb305154bf1cd8193d23176e998d7717e20ee18c2375515abc7aab90b` |
| `cfg_48000_48000.txt` | VBR-3 stereo (48k) | 18 | `8b92c9644b980e55729f4bb8fb9347467a5ba715c3d61f379f5cf3a9cba48640` |
| `cfg_48000_56000.txt` | VBR-3 mono (56k) | 18 | `5a9d857e900be4a1567f549af44341744b914f07786273b04389f7784755dd24` |
| `cfg_48000_64000.txt` | VBR-4 stereo (64k) | 18 | `d5028800cb8209da6a62b68c4ad82085cceb5aa99c2e993c8b11fc2d75b6df49` |
| `cfg_48000_72000.txt` | VBR-4 mono (72k) | 18 | `2231a97d03b721b1de01a47057918f724109394635d5d29aab604c36d906f429` |
| `cfg_48000_96000.txt` | VBR-5/VBR-6 stereo (96k), tracked | 18 | `27d895f05396712d49d753cef7ce52040fbe290c98897eead3da5a1c348090b7` |
| `cfg_48000_112000.txt` | VBR-5/VBR-6 mono (112k), tracked | 18 | `8c2ef084c2989ffd15803a42f1ee9b6a5ba7e0274bfdc547fc48b3e1c418b737` |

`include_str!` coverage: `psy_config.rs` contains 18 snapshot constants plus
two non-snapshot mentions of the macro name in comments/doc text; every
snapshot constant resolves to one of the files above. `load_cfg_for_profile`
maps the profile to the per-channel bitrate and fails closed on an unknown
combination. `every_profile_resolves_to_exact_snapshot` exercises all six
profiles × two rates × mono/stereo.

Four original VBR-5/VBR-6 snapshots remain byte-identical to the baseline:

| File | Baseline git blob |
|---|---|
| `cfg_44100_96000.txt` | `0364fb1b6314931038e5b3e462f951445bac2712` |
| `cfg_44100_112000.txt` | `91b2c1198afb3bf3f53a4a705cc74a23ba99ab18` |
| `cfg_48000_96000.txt` | `20af54e6aaa75605d951b894a3d328ed0cdd5320` |
| `cfg_48000_112000.txt` | `ba551da904e9d08e88b20ccd46dee0b7edffe8af` |

**M8 hash-column erratum.** All 18 prefixes in the M8 report match the
recomputed digests. However, the abbreviated suffix fragments in M8's table
are not consistently true suffixes: 4 of 18 are trailing fragments, 4 more
appear elsewhere in the digest, and 10 do not appear verbatim (transcription
errors in the report, e.g. `…2731d` versus the actual `…273d951d5`). This is
a defect in the M8 report's abbreviated column, not a snapshot-content
discrepancy. The full digests above were recomputed from the release tree and
are authoritative; the M8 record was left unrewritten as a historical
document. Content correctness is independently pinned by the passing
`vbr_profiles_match_c_reference_init` and baseline tests.

## 7. Package verification

### `ferrite-aac`

Strict gate, run after the release commit on the clean main worktree and
again on an isolated clean checkout at `f6757d5`:

```text
cargo package --locked -p ferrite-aac
```

**PASS.** The rebuilt crate is `target/package/ferrite-aac-0.3.0.crate`
(6 487 923 bytes) with 448 file entries:

- 18 `src/aac_enc/testdata/cfg_*.txt` snapshots present;
- 4 committed WAV fixtures present;
- `Cargo.toml`, `Cargo.toml.orig`, `Cargo.lock`, `README.md` present;
- `docs/releases/0.3.0.md` present;
- `NOTICE` and `LICENSE-FDK-AAC` present (complete license text);
- no `target/`, local absolute paths, cache files, or private oracle data.

The 0.3.0 release notes cite the pre-commit `--list` count of 447 files;
the post-commit archive has 448 because the release-notes file itself did not
exist when that listing was taken. No content is missing from either run.

### `aac-convert`

```text
cargo package --locked -p aac-convert --list
```

**PASS (contents).** 57 entries, including `Cargo.toml`, `Cargo.lock`,
`README.md`, `THIRD-PARTY-NOTICES.md`, the converter sources and its test
fixtures. It does not include the codec sources or snapshots (correct: they
are a path dependency).

```text
cargo package --locked -p aac-convert
```

**BLOCKED (by design).** Cargo cannot resolve `ferrite-aac = ^0.3.0` because
`ferrite-aac` is not published to crates.io: `no matching package named
ferrite-aac found`. This matches the documented distribution model in
`rust/convert/README.md`: `aac-convert` is shipped as part of the workspace
source bundle, not as a standalone crate.

**Documented packaging gap:** the `aac-convert` crate includes
`THIRD-PARTY-NOTICES.md` but not `rust/LICENSE-FDK-AAC` or `rust/NOTICE`.
Repository policy says this must be reported, not changed unilaterally; no
manifest change was made. It is not exercised by this source-only release
because no standalone `aac-convert` package is published. See M9-M2.

### Publishing intent

No crate was published. 0.3.0 remains a source-only repository release.

## 8. Validation matrix

All commands run from `rust/` with Rust/Cargo 1.99.1 unless stated. Status
values: PASS, FAIL, BLOCKED (required external artifact absent; fails loudly),
N/A.

### Build, test, and hygiene

| # | Command | Status | Result |
|---|---|---|---|
| 1 | `cargo check --locked` | PASS | 0 errors, 20 pre-existing warnings |
| 2 | `cargo check --locked --no-default-features --features aac_enc` | PASS | 0 errors |
| 3 | `cargo build --locked --examples` | PASS | all examples compile |
| 4 | `cargo test --locked --features aac_enc --lib` | PASS with BLOCKED | 350 passed, 11 BLOCKED, 1 ignored |
| 5 | Same, CI Tier-A skip list | PASS | 346 passed, 0 failed, 1 ignored, 15 filtered |
| 6 | `cargo test --locked --features aac_enc --test production_api` | PASS | 6 passed |
| 7 | `cargo test --locked --features aac_enc --doc` | PASS | 19 passed |
| 8 | `cargo test --locked -p aac-convert` | PASS | 261 passed, 0 failed, 4 ignored |
| 9 | `cargo test --locked -p aac-convert -- --ignored` | PASS | 4 passed (`ffprobe` present) |
| 10 | `cargo fmt --all -- --check` | PASS | clean |
| 11 | `cargo clippy --locked --features aac_enc --all-targets` | PASS (informational) | 0 errors; 80 lib + 181 lib-test + 5 example + 1 manifest warnings, non-gating |
| 12 | `tools/check-licensing.sh` | PASS | 67 `rust/src` files carry the notice; NOTICE + LICENSE present; manifest declares FDK-AAC |
| 13 | `git diff --check` | PASS | clean |

The 11 BLOCKED tests in #4 are `m5a_decode_check`, nine `phase1*` tests and
`m55_streams_match_c_reference`. `m5a_decode_check` was executed separately
with the pinned decoder and passes (§9); the other ten remain BLOCKED (§10).
No test was weakened and no expected output was changed.

### Encoder-specific validation

| Check | Status | Evidence |
|---|---|---|
| VBR-1 through VBR-6 profile tests (`vbr` filter) | PASS | 29 passed |
| C-reference initialisation parity | PASS | `vbr_profiles_match_c_reference_init`, 20 operating points |
| VBR-5 production baseline | PASS | committed byte/hash pins unchanged |
| VBR-6 default-profile baseline | PASS | `production_default_profile_vbr6_music_baseline_is_unchanged` |
| M5/M6 regression and migration | PASS | `m6_api_matches_driver_bytes` and filter set green |
| Crash recovery | PASS | 7 tests |
| ADTS structural validation | PASS | header/field/geometry assertions in lib suite and CLI outputs |
| M4A AudioSpecificConfig validation | PASS | converter suite; `ffprobe` reports AAC-LC |
| FFmpeg decoding | PASS | 5/5 sampled outputs, exit 0, empty error output |
| CoreAudio decoding | PASS | `afinfo` recognises M4A and ADTS AAC-LC; M4A trims to source length |
| FDK decoder tests | PASS | 3/3 (§9) |

### Packaging, clean checkout, and CLI

| Check | Status | Result |
|---|---|---|
| `cargo package --locked -p ferrite-aac` (clean tree) | PASS | §7 |
| Clean checkout at `f6757d5` (`git worktree`, status clean) | PASS | isolated worktree, no working-tree content copied |
| Clean-checkout builds (default, encoder-only, converter) | PASS | 3/3 |
| Clean-checkout lib suite (CI skip list) | PASS | 346 passed, 1 ignored |
| Clean-checkout lib suite (full) | PASS with BLOCKED | 350 passed, same 11 BLOCKED tests, 1 ignored |
| Clean-checkout API/doctests/converter suites | PASS | 6 / 19 / 261 passed |
| Clean-checkout strict package gate | PASS | §7 |
| `aac-convert --version` | PASS | `aac-convert 0.3.0` |
| `aac-convert --help` | PASS | documents `--vbr 1..6`, default 6, VBR-5/VBR-6 distinction |
| Default conversion (stereo 48 kHz) | PASS | VBR-6, 14 frames, 1 961 bytes M4A, 20 000 Hz bandwidth |
| Explicit VBR-5 conversion | PASS | 14 frames, 1 959 bytes M4A, 19 293 Hz |
| Additional lower-bitrate profile (VBR-2) | PASS | 14 frames, 1 676 bytes M4A, 13 000 Hz |
| Mono VBR-1 ADTS (44.1 kHz) | PASS | 5 frames, 286 bytes |
| Default ADTS | PASS | 14 frames, 1 060 bytes |
| M4A identification atom | PASS | `©too` = `Ferrite 0.3.0` |

All conversions were run both in the pre-commit working tree and again from
the clean checkout; byte counts were identical.

## 9. FDK decoder validation

The decoder was rebuilt locally from the pinned in-history source, following
the repository's documented CI procedure. No production dependency was
introduced; only the test-only `AAC_DEC_BIN` path was used.

| Item | Value |
|---|---|
| Source commit | `da00af1ca9624a213fb53c9ac52f6fec55e9b2af` (verified with `git rev-parse HEAD`) |
| Build | `cmake -DCMAKE_BUILD_TYPE=Release -DBUILD_PROGRAMS=ON -DBUILD_SHARED_LIBS=OFF`, target `aac-dec` |
| Binary SHA-256 | `520e852fa0ec9bcf963fb8dc895431729dce86fedab5e08428d118d362053371` (build-environment specific) |
| `m5a_decode_check` | PASS — exit 0, empty stderr, structural WAV checks, implied trims 1744/1685 samples |
| `crash_recovered_stream_decodes` | PASS — 42 frames, 22 035 bytes, trim 1744, determinism identical |
| `intensity_probe_adts_decodes` | PASS |
| `tools/check-decode-evidence.sh` on the three logs | PASS — `decode evidence OK: crash, m5a, adts` |

This closes the decoder portion of the M7/M8 validation gap. It does **not**
close the C-oracle portion (§10).

## 10. Tier-B oracle limitation

The private FDK-AAC reference checkout remains unavailable. `PHASE1_ORACLE_DIR`,
`PHASE1_C_DIR` and `M55_C_DIR` are unset and no reference checkout exists in
this environment. The following ten tests were therefore **not executed**:

- nine `phase1*` C-oracle parity tests
  (`phase1_frontend_agreement_sine_f5`,
  `phase1e_sine_f5_band16_residue`, `phase1e_onset_f24_bands_causality`,
  `phase1f_multitone_f1_no_live_divergence`,
  `phase1f_stepdown_f1_no_live_divergence`, `phase1g_long_flips_residue`,
  `phase1i_sine44_f5_tier_b`, `phase1i_multitone_f5_tier_b`,
  `phase1_full_stream_parity`);
- `m55_streams_match_c_reference` (full-stream C comparison).

Each fails with an explicit `reference validation artifact missing` panic
naming the variable and the required artifact; none is silently skipped. No
substitute reference implementation was used. **This release does not claim
that these tests passed, and does not claim C-oracle parity for VBR-6 or for
the full-stream comparison.**

Compensating evidence that *was* executed: 20-point C initialisation parity
against committed reference values, all VBR-5 byte/hash baselines, the VBR-6
default baseline, independent FFmpeg/CoreAudio decoding, and the pinned FDK
decoder round-trip. These compensate but do not replace Tier-B parity; they
must be re-run where the private checkout is available.

## 11. Licensing and provenance

- No license or notice file changed. `NOTICE`, `rust/NOTICE` and
  `rust/LICENSE-FDK-AAC` retain the complete FDK-AAC text, the prominent
  Third-Party Modified Version notice, and the 2026-10-26 modification date.
- `tools/check-licensing.sh`: PASS (67 codec source files).
- `ferrite-aac` 0.3.0 package includes `NOTICE` and `LICENSE-FDK-AAC`; the
  manifest declares `license = "FDK-AAC"` with `license-file`.
- `aac-convert` package includes `THIRD-PARTY-NOTICES.md`; see the documented
  gap in §7 (M9-M2). No manifest change was made.
- New Ferrite-authored files (audit examples, oracle generator, docs) carry
  no FDK codec header, consistent with the repository's new-file policy;
  upstream-derived files keep their complete headers.
- Provenance: the 14 new snapshots and their generator are recorded in
  `rust/docs/vbr-profiles.md` §“Implementation notes and verification”
  (unmodified FDK v3.0.0, `a68a51aa45d86a43c7b228e3d45624af09dcdd8d`,
  real init path, byte-identical re-emission self-check of the four
  pre-existing snapshots). The revision-pinned inventory remains pinned at
  `89bb289a32773ca06b49c2ba057bbc516101dd7a`; an addendum (§H) records the
  scope boundary without rewriting rows or inventing provenance.
- No binary is distributed. FDK binary-redistribution conditions and AAC
  patent authorization remain separate qualified assessments; this report
  makes no legal or patent-safety claim.

## 12. Annotated tag

| Item | Value |
|---|---|
| Tag name | `v0.3.0` |
| Annotation | Annotated (`git cat-file -t v0.3.0` → `tag`) |
| Tag object | `f9a010d81c731a30749c8b4ea1d9bb9a4ae1c1c4` |
| Target commit | `f6757d5dc7452cf821d9fd71323b0706715c83ee` |
| Tagger date | 2026-11-13T19:24:42+01:00 |
| `git describe --tags` | `v0.3.0` |
| Tag existed before creation | No (local and remote checks were empty) |
| Tag pushed | **Yes** — pushed to `origin` after approval (§15) |

Tag message:

```text
Ferrite 0.3.0 — six AAC-LC VBR profiles and VBR-6 default

Source-only release. VBR-1 through VBR-6 are selectable; VBR-5 remains
the upstream-compatible baseline and VBR-6 is the Ferrite default.
Adds the public profile-selection API, aac-convert --vbr support, the
profile reference, and 0.3.0 release notes.

No binary artifacts are distributed.
```

The tag points at the release commit, and `git rev-parse v0.3.0^{}` equals
`git rev-parse HEAD` at preparation time.

## 13. Outstanding findings

**Critical: none.**

**High: none open.** M8-H1 is closed.

**Medium:**

- **M9-M1 — Tier-B C-oracle parity remains BLOCKED** (carried from M8-M1).
  Ten tests were not executed; no substitute and no coverage claim. They
  remain mandatory where the private reference checkout exists.
- **M9-M2 — `aac-convert` package is not standalone-complete.** Cargo
  verification cannot resolve the unpublished `ferrite-aac` path dependency,
  and the crate does not bundle `LICENSE-FDK-AAC`/`NOTICE`. This is
  consistent with the documented source-bundle model and no crate is
  published, but a future standalone package would need an approved manifest
  and notice change.

**Low:**

- **M9-L1 — M8 hash-column errata.** Abbreviated suffixes in the M8 table
  are unreliable; full verified digests are in §6. Historical record left
  unrewritten.
- **M9-L2 — test-only absolute `/private/var/...` oracle fallbacks** remain
  in `#[cfg(test)]` phase-1 modules (M8-L1). Pre-existing at the baseline,
  never used in normal builds, fail loudly when artifacts are absent.
  Accepted; no production impact.
- **M9-L3 — no explicit CI snapshot-manifest job** (M8-L2). Compilation and
  `all_validated_snapshots_parse` already fail closed. Optional hardening.

**Informational:**

- **M9-I1 — provenance inventory remains revision-pinned** at the 2026-10-27
  revision; §H addendum records scope only. Regeneration is follow-up work.
- **M9-I2 — `esds` declares 0/0 bitrates**, silence produces minimal frames,
  and the VBR-3 synthetic-transient observation remains unexplained at the
  perceptual level. All documented; no change.
- **M9-I3 — Cargo manifest warning** about `license-file` with a non-standard
  license identifier is pre-existing and informational; the FDK-AAC
  packaging arrangement is deliberate and was not changed.
- **M9-I4 — no perceptual, ABX, or listening claim** is made for any profile;
  VBR-6 is validated as configuration and decoded spectral behaviour only.
- **M9-I5 — audit examples are retained as evidence**, not CI-wired; their
  headers now say so.

## 14. Release go/no-go recommendation

**GO WITH DOCUMENTED LIMITATIONS** for remote publication of the prepared
release commit and annotated tag, subject to explicit owner approval.

Rationale:

- The release commit is created, cleanly scoped, and fully validated on an
  isolated clean checkout.
- All 18 snapshots are included, correctly mapped, structurally valid, and
  content-pinned by passing reference tests; the four original snapshots are
  byte-identical.
- Version metadata is consistently 0.3.0 in manifests, lockfiles, CLI output,
  container metadata, and maintained docs.
- The `ferrite-aac` package passes the strict gate and contains the required
  sources, fixtures, snapshots, license, and notice.
- Ordinary builds and tests pass without private artifacts; the FDK decoder
  round-trip is green.
- Ten Tier-B C-oracle tests remain BLOCKED and are disclosed as such. No
  coverage, bit-exactness, or perceptual claim depends on them.

Conditions before remote publication:

1. Owner approval of this report.
2. Push `main` (one commit) and the annotated `v0.3.0` tag.
3. Do not publish crates, binaries, or a GitHub release without separate
   instruction and the already-flagged licensing/patent review.
4. Re-run the Tier-B suite in an environment that holds the private reference
   checkout, and record the result without changing this release's claims.

### Mandatory stopping point (at approval time)

At the approval gate, the release commit and annotated tag existed **locally
only**. No push, tag push, GitHub release, crate publication, or binary upload
had occurred. Work stopped there pending explicit owner approval; §15 records
the subsequent approved push.

---

## 15. Post-approval remote publication (2026-11-13)

The owner approved pushing `main` and the annotated tag after reviewing this
report. The following refs were pushed to `origin`
(`https://github.com/whepper/ferrite.git`) without force:

| Ref | Resolved remote hash |
|---|---|
| `refs/heads/main` | `f6757d5dc7452cf821d9fd71323b0706715c83ee` |
| `refs/tags/v0.3.0` | `f9a010d81c731a30749c8b4ea1d9bb9a4ae1c1c4` (annotated tag object) |
| `refs/tags/v0.3.0^{}` | `f6757d5dc7452cf821d9fd71323b0706715c83ee` (peeled commit) |

Push results: `a1bcbad..f6757d5  main -> main` (fast-forward) and
`* [new tag]  v0.3.0 -> v0.3.0`.

Not performed, per the approval scope: no force-push, no history rewrite, no
crates.io publication, no GitHub release (latest remains `Ferrite 0.2.0`), and
no binary upload. At publication time, local `main` was level with
`origin/main`; the documentation-only follow-up commit that records this
report makes local `main` one commit ahead until it is pushed.

This section is the authoritative record of the publication step; the
approval-time narrative above is preserved as history.

