# M8.5 — `aac-convert` Release Readiness Audit and Checklist

- Date: 2026-10-24
- Baseline: `main` at `4908778` (`test(aac-convert): harden batch resilience coverage`),
  12 commits ahead of `origin/main`, worktree clean apart from the intentionally
  untracked `docs/aac-convert-m8.1-batch-design.md`.
- This is an audit deliverable, not an approval. It authorizes no distribution,
  no publication, and no implementation changes.

## 1. Executive summary and recommendation

**Recommendation: READY FOR LIMITED PRIVATE USE.**

`aac-convert` is functionally complete and well-evidenced for its documented
scope on the tested platform (macOS ARM64, with CI structured for Ubuntu):
single-file and sequential directory-batch FLAC→AAC conversion, M4A by default
with `iTunSMPB` signalling, ADTS on request, no-overwrite atomic publication,
and documented exit codes. The M8.2–M8.4 test gates pass at this HEAD.

**Public distribution — including any controlled pre-release that puts
binaries in other people's hands — is NOT READY: legal blockers remain.**
The FDK-AAC redistribution conditions have not been demonstrated as satisfied
in any shipped artifact, the MPL-2.0 source-availability workflow does not
exist yet, and AAC patent authorization is unresolved (the Via Licensing AAC
pool is active; no license is demonstrated). These are P0 legal-reviewer
items. Technical confidence is high; legal clearance is absent. Do not let
green tests be mistaken for permission to distribute.

## 2. Scope and product boundary

`aac-convert` (`rust/convert/`, crate version 0.1.0) converts native 16-bit
FLAC at 44.1/48 kHz, mono/stereo, to AAC-LC VBR-5 through the existing Rust
`aac` encoder. Delivered in M7.16–M8.4 (commits `e2b5731`, `43dcd02`,
`320720f`, `4908778`):

- Single-file conversion with extension-inferred or explicit format.
- Directory batch conversion: recursive by default, `--no-recursive` opt-out,
  deterministic byte-sorted planning/reporting, mirrored output structure,
  canonical `.m4a`/`.aac` extensions.
- `--dry-run`, `--skip-existing`, `-v/--verbose`, `-q/--quiet`.
- Sequential, bounded-memory execution; no-overwrite hard-link publication;
  per-file failure isolation; exit codes 0/1/2/3.
- M4A carries `iTunSMPB` gapless signalling with Apple-style brands, no edit
  list; ADTS payloads are byte-identical between containers.

Explicitly out of scope (unchanged): 24-bit input (rejected), resampling,
downmixing, >2 channels, other sample rates, VBR 1–4, metadata/tags, cover
art, loudness, parallel conversion, overwrite/force flags, JSON output,
multi-input syntax, GUI, universal gapless playback claims.

## 3. Evidence methodology and classification definitions

Primary sources are the repository at the baseline HEAD: implementation
(`rust/convert/src/`), tests (`rust/convert/tests/`), manifests and lockfile,
CI workflow, `THIRD-PARTY-NOTICES.md`, `NOTICE`/`rust/NOTICE`, README, and
the M7.1/M8.1 decision records. Test counts below are historical evidence
from the M8.2–M8.4 completion reports run against this exact HEAD content
(worktree matches the validated state); the full suite was not rerun for
this audit.

- **VERIFIED** — directly inspected implementation, passing automated tests,
  or recorded evidence appropriate to the claim.
- **OUTSTANDING** — not yet verified; needs a manual check, another platform,
  or a decision. Shippable only behind an explicit documented limitation.
- **BLOCKING** — must be resolved before the intended distribution can
  responsibly proceed.

No item is marked VERIFIED on README assertions alone. ABX testing is out of
scope per project decision and is not a gate.

## 4. Functional and regression readiness matrix

| Area | Status | Strongest evidence and limits |
|---|---|---|
| Single-file CLI compatibility | VERIFIED | `tests/cli.rs` (9 tests) green at this HEAD; success line, `--format` override forms, usage texts, 0/1/2 codes intact. One deliberate M8.2 change: missing input now exits 3 per M8.1 F2 (reported in M8.2 closeout). |
| Traversal, mapping, ordering | VERIFIED | `tests/batch.rs` flat/nested/non-recursive/extension-case/hidden-file tests; byte-sorted plan and reporting asserted. |
| Preflight and duplicate detection | VERIFIED | Exact + ASCII case-fold duplicate abort tested at unit level always; FS-level integration runs on case-sensitive filesystems (skips honestly on APFS — see §5). Same-root refusal tested. |
| Existing-output behavior (±`--skip-existing`) | VERIFIED | Sentinel-intact tests both modes; all-skipped exits 0; mixed skip/convert/fail tallies and exit 1 tested; default rerun still fails (exit 1). |
| Publication races, atomic no-overwrite | VERIFIED | Three barrier-started race tests (unit, M4A, skip-classification): exactly one winner, loser `OutputExists`/skip, winner bytes intact, no orphans. |
| Temp cleanup on ordinary failures | VERIFIED | Every conversion test asserts empty temp scans; truncated/MD5/24-bit/permission-failure paths leave no destination and no `.aac-convert-*.tmp`. |
| Stale-temp behavior | VERIFIED | New M8.4 test: pre-placed orphan coexists, run succeeds, orphan untouched for manual removal. Abrupt-termination cleanup itself is untestable here — documented limitation, not a defect. |
| M4A structure, `iTunSMPB`, no edit list | VERIFIED | Box/sample-table/`mdhd`/`esds`/brand/`iTunSMPB`-value assertions per fixture; `edts` absence asserted; `mdat` == ADTS access units. Decoder-trim evidence (FFmpeg 9.0.2, CoreAudio, macOS 27.0.1) is historical per README, rerun as ignored smoke tests in M8.2–M8.3 gates. |
| ADTS structure | VERIFIED | `parse_adts` tiling/profile/rate/channel assertions on batch and single outputs. |
| Sample-count/alignment validation | VERIFIED | Structural + independent-decoder spot checks (historical FFmpeg runs); no ABX per project decision. |
| Encoder API/migration byte-identity | VERIFIED | Tier A suite green with the exact 15-test skip list (467 passed); batch-vs-single byte-identity tests prove the converter reuses the pipeline. Perceptual competitiveness remains the encoder objective, not a bit-identity claim. |
| Tier A + decoder/doctest gates | VERIFIED | Historical at this HEAD: Tier A 467/15-skipped; encoder doctests 84; decoder lib 256 + doc 81; convert doctests 0. Skip list byte-identical to `.github/workflows/ci.yml`. |
| Builds, fmt, lint, deps | VERIFIED | Debug + release builds green; convert files fmt/clippy-clean (remaining `aac_enc` drift is pre-existing and informational per CI); `cargo tree` matches the notices inventory; `Cargo.lock` untouched by M8 work. |

## 5. Operational safety and platform matrix

| Area | Status | Notes |
|---|---|---|
| Atomic no-overwrite publication (hard link) | VERIFIED (macOS ARM64/APFS; CI targets Ubuntu) | Same-filesystem by construction (temp in dest dir); `AlreadyExists`↔`OutputExists` distinguished by kind; race-tested. |
| Existing-destination preservation under race | VERIFIED | See §4 race row. |
| Temp cleanup, ordinary errors | VERIFIED | See §4 cleanup row. |
| Orphan temps after abrupt termination | OUTSTANDING (documented) | No handler by design; orphans possible after any fatal signal/kill/power loss (README corrected in M8.4). Manual-removal rule documented. Not a defect; do not over-claim. |
| Disk-full / write/finalize failures | OUTSTANDING (partially covered) | Paths verified by inspection (all pre-publish; Drop cleanup); creation-failure proven by read-only-dir test. True disk-full NOT RUN — no controlled environment; do not fill host disks. |
| M4A-finalization fault injection | OUTSTANDING | No deterministic trigger without a filesystem-abstraction refactor (explicitly out of scope). Structural mitigation recorded in M8.4. |
| Unsupported hard links (FAT/exFAT/network) | OUTSTANDING | Safe-failure path by inspection only; no such volume available for testing. Do not claim support. |
| Unix symlink/permission behavior | VERIFIED (macOS) | Gated tests green (dir links never descended, file links accepted, broken links fail per-file, read-only dirs fail per-file). |
| Windows behavior | OUTSTANDING | NOT RUN — no Windows host; CI is Ubuntu-only. Drop-before-link ordering and naming are Windows-safe by inspection only. Never weaken rules to accommodate; report as untested. |
| Case-sensitive/insensitive collisions | VERIFIED (logic) / platform-dependent (FS) | Unit fold-detection always runs; FS integration runs on case-sensitive filesystems and skips honestly on APFS. |
| Forced termination demonstration | OUTSTANDING | Cannot be timed deterministically against ms-long conversions; documented, not claimed. |

## 6. Distribution and packaging assessment — OUTSTANDING with a clear path

- **Not standalone-publishable (VERIFIED):** `aac-convert` depends on the workspace-local unpublished `aac` crate (`path = ".."`); crates.io carries only `aac 0.0.0`, which does not satisfy `^0.1.0`. `cargo package`/`publish` for the converter is unsupported by design; publishing `aac` is out of scope. The `spike-m4a` crate is explicitly workspace-excluded (`publish = false`) and does not affect this.
- **Workspace-source-bundle model (VERIFIED as documented):** `cargo build --locked --release -p aac-convert` + `cargo test --locked -p aac-convert` with Rust 1.99.1 against the committed `rust/Cargo.lock`. Toolchain is consistent across CI, `rust-version` fields, and README. No vendoring; no tracked build artifacts (`git ls-files` shows no `target/` outputs).
- **No installable artifacts exist (VERIFIED):** no release archives, checksums, provenance metadata, or platform labels are produced. Installation instructions are build-from-source only and reproducible on the tested toolchain.
- **Platform evidence (OUTSTANDING):** meaningful end-to-end evidence exists for macOS ARM64 (local) with CI structured for Ubuntu x86_64; neither constitutes a multi-platform release claim.
- **Recommendation:** keep the bundle model for the next step. Before any binary leaves the project, complete §7 P0 items, then produce a versioned source bundle + build SBOM (see §8) with SHA-256 checksums. Do not create registry packages or CI release workflows in the readiness phase.

## 7. Legal and licensing assessment — BLOCKING until reviewed

This is a separate gate from technical correctness. Nothing below is legal
advice or clearance. All items are BLOCKING for public distribution
(including controlled pre-releases that distribute binaries) and
OUTSTANDING-but-tolerable for strictly private local builds.

| Item | Status | Evidence / action |
|---|---|---|
| FDK-AAC binary-redistribution conditions (retain license text, free source availability, no license fees, no endorsement, modification notices) | BLOCKING (public) | Texts present (`NOTICE`, `rust/NOTICE`, file headers) but no shipped artifact demonstrates the conditions as satisfied. Action: legal reviewer to define the artifact bundle (notices + source offer + modification log) before any distribution. |
| FDK grants no patent license | BLOCKING (public) | Stated in the NOTICE text itself. Action: resolve AAC patent authorization for the target use/territories (see next row). |
| AAC patent authorization | BLOCKING (public) | External corroboration: Via Licensing Alliance operates an active AAC patent pool covering AAC-LC with per-unit royalties (https://www.via-la.com/licensing-programs/aac; 2026 licensee announcements). This establishes *where* authorization is sought, not that it is held. Action: legal reviewer to determine coverage/licensing need for the intended distribution. |
| MPL-2.0 (Symphonia, statically linked) | BLOCKING (public) | Inventory records MPL-2.0 with bundled `LICENSE` texts (`THIRD-PARTY-NOTICES.md`). No source-availability workflow exists. Action: legal reviewer to confirm notice + covered-source availability plan. |
| Apache-2.0/MIT/BSD/Zlib attributions (`shiguredo_mp4` + transitive set) | BLOCKING (public, low effort) | Inventory complete with versions; bundled texts confirmed where noted. Action: assemble attributions into the artifact; confirm whether build-time-only texts must also ship. |
| Non-SPDX `FDK-AAC` license identifier | OUTSTANDING | Flagged in-repo; must be clarified before any registry publication (which is in any case out of scope). |
| M8 orchestral fixture provenance | OUTSTANDING (non-blocking for converter) | `docs/aac-enc-m8-owner-decision.md` is explicitly PREPARATION ONLY, owner decision pending. Irrelevant to converter distribution unless that asset ships; do not ship it. |

If clearance cannot be established from this evidence, engage an appropriately
qualified legal professional. Do not distribute on the strength of passing tests.

## 8. Prioritized blockers and follow-up actions

| # | Priority | Class | Item | Evidence / reason | Action | Owner | Pre-public? |
|---|---|---|---|---|---|---|---|
| 1 | P0 | BLOCKING | FDK-AAC redistribution compliance demonstrated in-artifact | Texts exist; no artifact demonstrates conditions | Define + assemble notice/source/modification bundle | Legal reviewer | Yes |
| 2 | P0 | BLOCKING | AAC patent authorization resolved | FDK grants none; Via pool active, no license shown | Determine need + obtain coverage for use/territories | Legal reviewer | Yes |
| 3 | P0 | BLOCKING | MPL-2.0 source-availability workflow | Static linking; no workflow exists | Confirm notices + covered-source plan | Legal reviewer | Yes |
| 4 | P0 | BLOCKING | Attribution bundle assembled | Inventory complete, not assembled into artifact | Bundle Apache/MIT/BSD/Zlib texts; resolve build-time-only question | Release maintainer + legal | Yes |
| 5 | P1 | OUTSTANDING | CI executed on the M8 commits | CI is push/PR-triggered; M8 commits are unpushed, so CI has not run on them (local runs replicate its commands) | Push → record CI results | Release maintainer | Before any release claim |
| 6 | P1 | OUTSTANDING | Windows verification or explicit non-support | No host, no CI job; never weaken rules | Test on Windows or document Windows as unsupported | Platform tester | Before claiming it |
| 7 | P1 | OUTSTANDING | FAT/exFAT/network + true disk-full evidence | No volumes/env available | Test on real media/quota FS or document as untested | Platform tester | Before claiming it |
| 8 | P1 | OUTSTANDING | Artifact metadata (checksums, SBOM, platform labels) | Nothing produced yet | Add SHA-256 + source SBOM at bundle time | Release maintainer | With first artifact |
| 9 | P2 | OUTSTANDING | M4A-finish fault coverage; forced-kill demo | Needs refactor / non-deterministic | Keep documented; revisit only with a proven need | Engineering | Defer |
| 10 | P2 | OUTSTANDING | Version/provenance polish (0.1.0, M8 fixture pending) | Cosmetic vs blockers | Tag bundle; keep provisional asset out of scope | Release maintainer | Defer |

## 9. Minimum release checklist

- [ ] P0 legal items 1–4 signed off by a qualified reviewer; artifact bundle defined.
- [ ] CI green on the exact release commit (push first; item 5).
- [ ] `cargo test --locked -p aac-convert` + Tier A (exact 15-skip) + doctests green at release HEAD.
- [ ] Release + debug builds green; `cargo tree` matches notices inventory.
- [ ] Platforms listed explicitly with evidence level (tested / structured / untested).
- [ ] SHA-256 checksums + source SBOM published with the bundle.
- [ ] Provisional M8 fixture and spike crates excluded from the bundle.
- [ ] README limitations (VLC approximation, GUI players unverified, no universal gapless claim) intact.

## 10. Explicit limitations and assumptions

- Evidence is macOS ARM64 local runs plus a Ubuntu-structured CI that has not executed on the unpushed M8 commits; CI results on release HEAD are still to be recorded.
- Decoder-trim evidence (FFmpeg/CoreAudio versions) is historical; rerun as ignored smoke tests most recently in M8.2–M8.3 gates.
- VLC approximates (skips a frame, ignores `iTunSMPB`); GUI players unverified — do not claim universal gapless playback.
- No ABX or perceptual claim; VBR-5 quality is the encoder objective, out of scope here.
- Assumes the pinned toolchain (1.99.1) and crates.io availability at bundle time; assumes no network-isolated build requirement (no vendoring).

## 11. Source references

- Commits: `e2b5731` (converter), `3748779` (M7.1 record), `63179a9` (CI gates), `43dcd02` (M8.2 batch), `320720f` (M8.3 skip), `4908778` (M8.4 hardening).
- Code: `rust/convert/src/{main,batch,output,m4a,flac,adts,gapless,lib}.rs`; tests `rust/convert/tests/{batch,cli,conversion,independent_decode}.rs`, `tests/common/mod.rs`, `tests/fixtures/generate.py`.
- Config: `rust/convert/Cargo.toml`, `rust/Cargo.toml` (workspace members convert/framework/ffi), `rust/Cargo.lock`, `.github/workflows/ci.yml`, `rust/convert/spike-m4a/Cargo.toml` (`publish = false`, non-member).
- Docs: `rust/convert/README.md`, `rust/convert/THIRD-PARTY-NOTICES.md`, `NOTICE`, `rust/NOTICE`, `docs/aac-convert-m7.1-architecture-decisions.md`, `docs/aac-convert-m8.1-batch-design.md` (untracked), `docs/aac-enc-m8-owner-decision.md` (provisional), `docs/aac-enc-release-gate-addendum-20261021.md` (Tier A baseline).
- External (legal venue only, not clearance): Via Licensing Alliance AAC program, https://www.via-la.com/licensing-programs/aac (active pool covering AAC-LC; fee structures at https://www.via-la.com/aac-license-fees-structures).
