# Repository Modernization Audit — Native Rust AAC Project

- Date: 2026-10-24 (UTC).
- Scope: read-only, evidence-based audit of the whole repository. No code,
  manifest, doc, or tree changes performed (this file is the only addition).
- Baseline: branch `main` @ `7ebf208d40d78adcd265e3736e93f8a8aa93e8d1`
  (L3 implementation present: 9 tracked modifications + L3/L2/prep/closeout
  docs untracked; verified identical at audit start and end).

## 1. Executive summary and overall assessment

The repository is a hybrid: a complete upstream Fraunhofer FDK AAC C/C++
tree retained as a reference oracle, plus a genuinely native Rust workspace
whose encoder (`aac`, AAC-LC VBR-5, float-oriented) and application
(`aac-convert`) are production-relevant and dependency-clean. The Rust code
does not compile, link, or invoke any C/C++: `cargo tree` shows a pure-Rust
closure, CI never touches a C toolchain, and the C tree is exercised only
through manually built oracle binaries and read-only behavioral comparison.

The central problem is identity, not engineering quality. Package names
(`aac`), descriptions ("Fraunhofer FDK AAC Codec Library in Rust"),
license identifiers (`FDK-AAC`), and the repo name (`fdk-aac`) present the
project as the FDK port it started as, while the code has become a
float-native implementation with its own architecture (own chunk loop,
state representation, and error taxonomy), a focused VBR-5 product scope,
and independent loudness/tagging work. The audit recommends an independent
umbrella identity that keeps FDK derivation and attribution explicit
without claiming affiliation or bit-exact equivalence — plus a short list
of safe cleanups. No broad restructuring is warranted: the workspace,
crate boundaries, and test organization are sound for the current scope.

Overall verdict: **modernize the identity and packaging, preserve the
engineering.** Nothing found blocks that path; the largest risks are
self-inflicted (over-broad renames, deleting oracle material, or rewriting
history).

## 2. Repository baseline and audit scope

- Branch `main`, HEAD `7ebf208d40d78adcd265e3736e93f8a8aa93e8d1`; 0 staged,
  9 tracked-modified (all L3), 24 untracked (14 pre-existing planning docs
  + L3/L2/prep/closeout reports + L3 sources/fixtures). Unchanged by this
  audit.
- Workspace root `rust/Cargo.toml`: package `aac` 0.1.0 + workspace members
  `convert`, `framework`, `ffi` (all `rust-version = "1.99.1"`,
  `license = "FDK-AAC"`, `edition = "2021"`).
- 45 docs (44 `.md` + `research/`); 8 READMEs; no repo-root README.
  L3 closeout audit (PASS WITH FINDINGS, 200 tests green) treated as
  historical evidence, re-verified where cheap (`cargo test` rerun: 200/0).
- Inspection was read-only (`read`, `grep`, `cargo tree --locked --offline`,
  `cargo test --locked --offline` writing only to ignored `target/` and OS
  temp dirs). Three parallel read-only subagents covered the encoder crate,
  the C/C++ tree, and tests/fixtures/CI/docs; their evidence is folded in
  below with independent spot-checks by the auditor.

## 3. Current architecture map

```
fdk-aac/                          ← upstream C/C++ FDK tree + Rust workspace
├── cpp/                          ← unmodified Fraunhofer reference (oracle only)
├── aac-enc.c / aac-dec.c / test-encode-decode.c + sha1/wav helpers
├── tools/enc-oracle/             ← C-encoder introspection (manual builds)
├── tools/c-enc-bench/            ← C-side benchmark driver (manual builds)
├── tests/...Benchmark/  test/    ← AOSP benchmark apps + legacy C test script
├── CMakeLists.txt / Makefile.am+configure / Android.bp / win32/ / m4/
├── NOTICE (1995–2018) / METADATA / OWNERS / ChangeLog
├── docs/ (45)                    ← milestone reports, specs, audits, research/
└── rust/                         ← Cargo workspace (production)
    ├── Cargo.toml                ← package `aac` (lib) + workspace
    ├── src/                      ← `aac` lib: aac_enc/ (31 files, VBR-5 LC),
    │                               aac_dec/ + sbr/sac/drc/arith/tp/pcm_dmx/td_limiter,
    │                               common/ (~30 files: bitstream/fft/mdct/…)
    ├── convert/                  ← `aac-convert` lib + bin (FLAC→AAC M4A/ADTS)
    │   ├── src/{lib,batch,flac,meta,m4a,gapless,loudness,adts,output}.rs
    │   ├── tests/{batch,conversion,metadata,coverart,loudness,cli,…}.rs
    │   ├── tests/fixtures/ (28 FLAC, synthetic, generate.py + README)
    │   └── spike-m4a/            ← own [workspace]; M7.5 feasibility spike
    ├── ffi/                      ← `aac_ffi` (decoder-only C ABI, 11 fns)
    ├── framework/                ← `aac_framework` (`aac_decoder` example bin)
    ├── bitstreams/input.rawpkts  ← decoder-example default input (33 KB)
    └── build.rs                   ← emits rustversion cfgs only (no cc/cmake)
```

Dependency direction: `aac-convert` → `aac[aac_enc]` + `ebur128` + `symphonia[flac]` + `shiguredo_mp4`; `aac_framework` → `aac` + `aac_ffi`; `aac_ffi` → `aac`. The encoder has no binary except through `aac-convert`; the decoder has FFI + example bin. No cycles.

## 4. Legacy C/C++ inventory and retention recommendation

No production Rust build path compiles, links, or executes any C/C++
(evidence: `rust/build.rs` emits only `rustversion` cfgs; workspace
build-deps are `rustversion` only; `cargo tree -p aac-convert` is pure
Rust; CI (`ci.yml`) runs only cargo commands inside `rust/`, no C
toolchain, no FFmpeg/`rsgain`).

| Component | Class | Retain? |
|---|---|---|
| `cpp/` full FDK tree (libAACdec/enc, libFDK, SBR/SAC/TP/SYS/PCMutils/ArithCoding/DRCdec) | Reference/oracle | **Yes** — unmodified upstream reference; oracle probes and parity work read against it |
| `tools/enc-oracle/` (C introspection + extractors, explicitly "not part of any build system") | Reference/oracle | **Yes** — generates parity fixtures/vectors |
| `aac-enc.c`, `aac-dec.c`, `test-encode-decode.c`, `sha1.*`, `wavreader/writer.*` | Dev/test tooling | **Yes** — manually-built parity baselines (`aac-enc` fixed VBR-5 config mirrors the Rust product scope) |
| `tools/c-enc-bench/` | Dev/test tooling | **Yes** — C-side benchmark baseline |
| `test/run-test.sh` + `ref-*.txt`, `tests/Aac*Benchmark/` | Dev tooling / Historical | **Yes** — legacy regression + AOSP tooling; inert here |
| `cpp/documentation/*.pdf`, `cpp/fuzzer/`, `win32/getopt.h`, `m4/`, `fdk-aac.{def,sym,pc.in,cmake.in}` | Historical/provenance | **Yes** — API contract, ABI surface, packaging history |
| `CMakeLists.txt`, `Makefile.am`/`configure.ac` + generated files, root + `cpp/` `Android.bp` | Historical/provenance | **Yes** — the only way to build C reference binaries; `EXTRA_DIST` already lists `rust/*` |
| `NOTICE` (root, 1995–2018), `METADATA`, `OWNERS`, `ChangeLog`, `MODULE_LICENSE_FRAUNHOFER` | Historical/provenance (NOTICE legally required) | **Yes, untouched** |
| `rust/NOTICE` (2025 variant, differs from root) | Provenance | **Yes** — keep both; record why two variants exist (roadmap CLEAN-04) |

Nothing outside `rust/` qualifies as potentially obsolete: every item is
either an oracle/tooling input or provenance. The C tree's value as an
oracle remains high (frame-by-frame introspection, table provenance,
divergence arbitration). Recommended boundary (already de-facto true):
**production = `cargo build` in `rust/`; everything C/C++ is validation
tooling or provenance, built only manually per `tools/enc-oracle/README.md`
and the M5 reports.** State this boundary once in the entry-point README
(roadmap STRUCT-03) and change nothing else.

## 5. Rust workspace and crate-boundary assessment

- **Organization is sound for current scope.** Three members with distinct
  jobs (encoder/decoder lib, converter app, decoder FFI, decoder example).
  DSP, psychoacoustics, quantization, bitstream, and frame assembly live in
  `aac_enc/` / `common/` with narrow `pub` surfaces (only
  `config,channel_map,encoder` are `pub`; subagent: `aac_enc.rs:94-155`).
  `aac-convert` layers batch/tag/mux concerns on the unchanged encoder
  interface. No crate split is justified now — splitting (e.g. a separate
  loudness or MP4 crate) would add publishing/versioning overhead for no
  reuse benefit (no second consumer exists).
- **Coupling notes (no action required):** `stream_driver.rs` (~5600 lines)
  concentrates orchestration plus `#[allow(dead_code)]` snapshot fields;
  `psy_config.rs:43-63` similar. Acceptable for a ported control core;
  do not chase line counts.
- **Dead weight (tiny):** `obsolete_bindings_enabled` feature has no
  consumers (roadmap CLEAN-01); `spike-m4a` is correctly isolated by its own
  `[workspace]` but lives inside `convert/` where it reads as product code
  (roadmap STRUCT-02).
- **Quality:** per-stage error enums with `From` chaining and boundary
  validation (`config.rs:186-208`, `encoder.rs:257`); recoverable vs
  terminal failures distinguished; `unsafe` confined to one helper
  (`common/checked_cast.rs:127`) plus decoder-only FFI (`ffi/`, 11
  `extern "C"` fns, 19 `#[no_mangle]`); encoder has no FFI surface.
  `TransportType::Raw` and >2ch tables exist but `Encoder` validates
  mono/stereo × 44.1/48 kHz × ADTS only — intentional product scope, not
  dead code.
- **MSRV:** all manifests declare `1.99.1`; CI pins and tests exactly that.
  (Historical note: `ebur128` itself was proven on exact 1.60 during L3-prep
  and the L3 report is explicit that workspace MSRV remains 1.99.1 by
  policy — no contradiction.)
- **Testability:** good — pure functions (`protect_gain`,
  `round_half_even`, `soundcheck_string`) carry the tricky semantics with
  unit tests; I/O and atoms are asserted on real files.

## 6. Production versus validation dependency assessment

- **Production runtime (`cargo tree -p aac-convert --edges normal`):**
  `aac` (path), `ebur128 =0.1.10` (+`bitflags1`/`dasp_*`/`smallvec`),
  `symphonia 0.6.1[flac]`, `shiguredo_mp4 =2026.5.0`. No `cc`, no FFmpeg,
  no `rsgain`, no C toolchain on the default path (corroborated: `cc`
  appears only under `ebur128`'s non-default `c-tests`; the L3-prep
  shimmed-`cc` build is historical report evidence with intact mechanism).
- **No `[dev-dependencies]` anywhere in the workspace** — integration tests
  are dependency-free (byte-level MP4 assertions). `framework`'s `hound` is
  a runtime dep of an example decoder bin, not production library surface.
- **Build deps:** `rustversion` only. **Optional features:** `default =
  ["aac_dec"]`; `aac_enc` opt-in; `obsolete_bindings_enabled` dead
  (CLEAN-01). `ffi`/`framework` are decoder-only (no `aac_enc` feature) —
  encoder reachability is lib + `aac-convert` only, matching the product.
- **Reference/validation executables:** `aac-enc`/`aac-dec` binaries,
  `enc-oracle`, `c-enc-bench` — all manual C builds outside every build
  system and CI. **Developer tools:** system `ffmpeg`/`ffprobe`/`mutagen`
  used only for fixture generation and ignored advisory tests
  (`#[ignore]` ×4, skip cleanly when absent) — never production.
- **Recommendation:** keep this separation exactly; document it once at the
  entry point (STRUCT-03). No dependency changes needed.

## 7. Encoder lineage, divergence, and naming implications

Evidence supports **"float-native port of FDK AAC with derived DSP"** —
neither from-scratch nor 1:1:

1. **Directly ported:** control logic and tables — `threshold_vbr.rs`
   ports `FDKaacEnc_AdjustThresholds/AdaptThresholdsVBR` in Q31 `i32`
   ("bit-exact given identical inputs"); Huffman/SFB/bitenc tables are
   ported literals; per-file `cpp:line` citations and FDK license headers
   throughout `aac_enc/` (e.g. `psy.rs`, `pe.rs`, `energy.rs`,
   `block_switch.rs`, `config.rs`, `pcm.rs`).
2. **Derived:** DSP rewritten floating-point (migration spec §5:
   "float-native encoder, **no bit-exact fixed-point parity** … preserve
   mathematics and decisions, not bit patterns"); Q31 islands
   (`q31.rs:94-130`, ARM64 `fixmul` floor semantics) kept only where
   bit-exactness was verified valuable.
3. **Numerical divergence:** float vs fixed-point arithmetic, rounding,
   scaling — acknowledged by design with behavioral tolerances, not bit
   patterns (migration spec §5; M5.25/M5.26 divergence audits).
4. **Architectural divergence:** own chunk loop (`CHUNK_FRAMES` feeder),
   own state representation and error taxonomy (`EncodeError`,
   `EncodeFailure`, per-stage enums), own orchestration
   (`stream_driver.rs`); transport/channels restricted to the VBR-5
   product profile.
5. **Feature scope:** focused AAC-LC VBR-5 product (afterburner-on,
   mono/stereo, 44.1/48 kHz, ADTS→M4A); decoder/SBR/SAC/DRC/LOAS and other
   profiles are decoder-side or out of scope.
6. **Observable behavior:** validated by oracle comparisons and the M5
   campaign (M5.b baselines, M5.25/M5.26 divergence attribution), not by
   listening tests in this audit's scope; `cargo test` (200 convert +
   ~1000 lib unit incl. oracle vectors) is the standing gate.

Required wording (README, manifests, releases): **"float-native Rust port
derived from Fraunhofer FDK AAC — not bit-exact, not affiliated with or
endorsed by Fraunhofer."** Current strings ("Fraunhofer FDK AAC Codec
Library in Rust" as the *product* description in three manifests +
`rust/README.md` title, `license = "FDK-AAC"`, keywords `fdk`) read as
official provenance and must be demoted to attribution (roadmap NAME-02,
CLEAN-03). Do not claim from-scratch authorship: ported tables/logic and
per-file FDK headers are load-bearing provenance.

## 8. Tests, fixtures, and reproducibility

- **Scale:** ~1172 unit `#[test]` repo-wide (top: `stream_driver.rs` 31);
  136 integration tests under `rust/convert/tests/` (batch 47, conversion
  30, metadata 20, loudness 15, coverart 12, cli 10, independent_decode 2
  ignored); 4 pre-existing ignores total (ffprobe/ffmpeg-gated, skip
  cleanly). Re-ran convert suite in this audit: 200 passed, 0 failed.
- **Fixtures:** `convert/tests/fixtures/` 616 KB / 28 synthetic FLAC
  (provenance documented, `generate.py` + offline `ffmpeg`/`mutagen`,
  committed not generated-at-test-time); `src/aac_enc/testdata/` 11 MB /
  333 files (probe vectors + CC0/provisional WAVs with provenance files);
  `bitstreams/input.rawpkts` 33 KB (decoder-example default input).
  Duplication is deliberate and documented (tagged variants share audio;
  MD5/truncation fixtures share bytes) — do not "deduplicate".
- **Reproducibility:** deterministic committed inputs; `Cargo.lock`
  committed; `--locked` enforced in CI; regen procedures documented.
- **Gaps (carried, not fixed):** L3 F1 (no absolute R128 calibration test),
  F2 (no partial-rerun tag-identity test) — both explicit L2 acceptance
  criteria without CI tests; F3 (mixed-layout ruling awaits owner
  acknowledgment); F4 (24-bit loudness untested/undocumented); F5
  (`debug_assert!` invariant guard). Disposition: F1/F2 as test-only
  additions before release sign-off; F3 needs one owner sentence; F4 one
  doc sentence plus optional fixture test; F5 needs nothing.
- **Platform note (from L3 closeout §7.4):** exact-string golden tags assume
  libm last-ulp stability; watch CI on new platforms, prefer range
  assertions only if flakes appear.

## 9. Documentation and repository hygiene

- **Missing entry point (P2-level hygiene): no repo-root README.**
  `rust/README.md` ("The Fraunhofer FDK AAC Codec Library in Rust
  Project") is decoder-centric and stale-titled; nothing at the root
  explains what the repo is, which half is production, or where to start.
  Recommend one root README as the authoritative entry (STRUCT-03).
- **Authoritative and current:** L0, finalized L2, L3 implementation +
  prep + closeout reports (loudness thread); M6.1/M6.3/M6.5 (API contract,
  release readiness, release gate); migration spec §5 (float-native thesis).
- **Historical and valuable (retain, do not rewrite):** all M4/M5.x/M6.x
  milestone reports, M8.5, G0/M7.1/M8.1/M9.x/S1.x/S2.1 planning records,
  `research/d021/` evidence archive, ChangeLog, `cpp/documentation`.
- **Names that overstate the port:** `rust/README.md` title, three manifest
  `description`s, and `keywords = ["fdk"]` imply the product *is* FDK AAC
  (§7 wording fix covers this; code comments citing FDK files are correct
  provenance, keep them).
- **Stale/misleading:** `rust/README.md` build/test section (decoder-only
  framing, predates converter prominence); `license = "FDK-AAC"` (custom,
  non-SPDX — fine for `cargo package`, but crates.io publication would
  require `license-file` + review); two divergent NOTICE variants with no
  note explaining why (CLEAN-04); `aac-enc-m8-owner-decision.md` is
  explicitly PREPARATION ONLY (fine as labeled, do not action).
- **Duplication:** mild overlap between M6.3/M6.5/M8.5 release-readiness
  notes — acceptable as dated records; do not merge history. The future
  entry-point README should link, not duplicate.
- **No `1:1`-port claims found** in current user-facing READMEs; the
  bit-exact language in the migration spec is carefully scoped to named
  Q31 islands, not the encoder — keep that precision.

## 10. Naming candidates and recommended architecture

Availability/search-conflict status **not verified** for any candidate
(per audit rules, no registry/domain/GitHub checks performed) — owner must
verify before adopting. "Taken" notes below are known-collision risks, not
verified claims.

**Direction A — audio-engineering brand (umbrella):**
1. **sonora** — "sonorous"; short, pronounceable, product-grade umbrella.
   Risk: place names; verify.
2. **timbre** — tone-color; instantly audio. Risk: common noun, likely
   taken, noisy search.
3. **andante** — musical tempo ("moving along"); distinctive, calm.
   Risk: longer; verify.

**Direction B — codec-focused (components):**
4. **raac** — Rust AAC; terse hybrid for the encoder crate/bin. Risk:
   cryptic; verify.
5. **aac-lc** — purely descriptive; ideal long-term lib name if the generic
   `aac` must be yielded. Zero brand value, maximum clarity.
6. **aac-convert** — keep as-is (descriptive CLI; established in docs/tests).

**Direction C — broader toolkit (umbrella alternative):**
7. **soundstack** — positions a future multi-tool audio suite.
   Risk: generic; verify.
8. **waveworks** — toolkit feel, memorable. Risk: generic; verify.

**Direction D — short invented (umbrella/bin):**
9. **quaver** — eighth-note; distinctive, audio-adjacent, short.
   Risk: verify (common word in music contexts).
10. **encore** — encodes "encode" + applause; great CLI/product word.
    Risk: real commercial collisions likely; verify carefully.

**Rejected (do not use):** `cadence` (major EDA company), `audiokit`
(Apple-ecosystem association), `afterburner` (FDK-internal term; deepens
affiliation confusion), anything with `fdk-`/`fraunhofer` prefixes.

**Recommended architecture (default):** umbrella brand for repo/product
(owner picks from A/D after verification; `sonora` is the auditor's
first suggestion) + **descriptive component names retained**:
encoder lib `aac` → `aac-lc` (or `<brand>-enc`), CLI stays `aac-convert`,
FFI `aac_ffi` and framework bin unchanged. I.e., rebrand the *product*,
not the *functions*. Rationale: minimal churn where names are load-bearing
(imports, docs, tests), distinct identity where it matters (repo,
README title, manifest descriptions, releases).

## 11. Rename impact analysis

| Surface | Blast radius | Risk |
|---|---|---|
| GitHub repo (`whepper/fdk-aac`) | Remote rename + redirect; local `origin` updates; external links | Low (redirects hold), needs owner action |
| Umbrella/product name (README, docs titles) | Text-only | Very low |
| Cargo package `aac` → new lib name | `rust/Cargo.toml` [package] + [workspace] refs, path deps in 3 members, `use aac::` in ~10 files (convert ×5, ffi ×4, framework ×1, spike ×1), `crate_name` in `rust/Android.bp` | Medium — mechanical, reviewable, breaks downstream imports (none published yet at 0.1.0, so cheap now) |
| `aac-convert` package/bin | `convert/Cargo.toml`, `-p aac-convert` in CI (4 lines), test helpers, docs | Low–medium; **recommend not renaming** |
| `aac_ffi` / `aac_framework` / `aac_decoder` | FFI headers (`aacdecoder_lib.h` et al. mirror upstream C names — do not touch), Soong names, framework Cargo | Low; recommend unchanged (FFI must stay C-compatible) |
| `Cargo.lock` | Regenerates automatically | Very low (committed, `--locked` CI re-verifies) |
| `license = "FDK-AAC"` / descriptions / keywords | Manifest text + `cargo package` metadata | Very low; required for honest identity regardless |
| Docs (45 files) + fixtures + examples | Text references to names/paths | Very low; bulk-replaceable, reviewable |
| Published compatibility | All versions 0.1.0, no evidence of crates.io publication | None yet — rename is cheapest **before** any publish |

Public breaking changes exist only if the `aac` lib name changes (import
path) — acceptable pre-1.0/pre-publish, and the very reason to decide now.

## 12. Prioritized cleanup and modernization roadmap

### A. Safe, high-value cleanup

- **CLEAN-01 — Remove dead `obsolete_bindings_enabled` feature.**
  Evidence: no consumers (`rust/Cargo.toml:26-30`; subagent grep).
  Change: delete the feature line. Risk: none. Validate: `cargo build --workspace --locked`.
- **CLEAN-02 — Demote FDK product phrasing to attribution.**
  Files: `rust/Cargo.toml:8`, `rust/ffi/Cargo.toml:8`,
  `rust/framework/Cargo.toml:9`, `rust/README.md:1`, `keywords = ["fdk"]`.
  Change: descriptions → "<Brand> … derived from Fraunhofer FDK AAC";
  drop `fdk` keyword or keep one `fdk-derived` marker. Rationale: ends
  affiliation confusion; zero behavioral impact. Validate: `cargo package
  --locked -p aac` metadata diff.
- **CLEAN-03 — Make license publishable.**
  Evidence: `license = "FDK-AAC"` (non-SPDX) in 4 manifests; `cargo package`
  passes locally but crates.io requires SPDX or `license-file`.
  Change: add `license-file = "NOTICE"` (or LICENSE file) alongside review
  of the Fraunhofer terms for static-linking/distribution obligations.
  Depends on: legal skim of NOTICE § patent clause. Validate: `cargo
  package --locked` + `cargo publish --dry-run` (no actual publish).
- **CLEAN-04 — Document the two NOTICE variants.**
  Evidence: root NOTICE (1995–2018) vs `rust/NOTICE` (2025) differ with no
  note. Change: one paragraph in the entry README stating which covers
  what. Zero risk.
- **CLEAN-05 — Close L3 F4 with one sentence.**
  Add the post-truncation analysis note to `convert/README.md` Loudness
  section (F4). Zero risk.

### B. Structural modernization

- **STRUCT-01 — Root README as authoritative entry point.**
  Contents: what this repo is (native Rust AAC workspace + FDK reference
  tree), production-vs-oracle boundary, layout map (§3), build/test
  commands, doc index (entry → migration spec, M6 contract, convert
  README, L3 reports), provenance/licensing summary. Supersedes nothing;
  `rust/README.md` stays as crate-level doc (refresh its decoder-only
  framing while there).
- **STRUCT-02 — Relocate or clearly mark `spike-m4a`.**
  Evidence: `rust/convert/spike-m4a/` (own `[workspace]`, M7.5 experiment)
  sits inside the product crate directory. Change: move to
  `rust/research/spike-m4a/` (or `docs/`-adjacent archive) with its README
  intact, or add a `HISTORICAL` marker. Risk: low (excluded from workspace
  already). Validate: workspace build + convert tests.
- **STRUCT-03 — Codify the production/validation boundary.**
  One section in the root README + a pointer in `tools/enc-oracle/README.md`:
  production = `cargo` in `rust/`; C tree = oracles/provenance, manual
  builds only. No code change.
- **STRUCT-04 — Consider edition 2024.**
  Evidence: edition 2021 everywhere; MSRV 1.99.1 fully supports edition
  2024. Benefit: modern lints/idioms; cost: whole-tree `cargo fix` churn
  plus MSRV floor effectively pinned. Recommendation: defer until after
  identity work; low value on its own. (Explicitly **do not** bundle with
  renames.)
- **STRUCT-05 — `framework`/`ffi` stay as-is.**
  Decoder FFI must remain C-ABI-compatible (`aacdecoder_lib.h` mirrors
  upstream); no encoder FFI is needed. No change recommended.

### C. Naming and product identity

- **NAME-01 — Adopt umbrella brand; keep component names descriptive.**
  Default: repo/product → owner-verified pick (§10, auditor suggests
  `sonora`); lib `aac` → `aac-lc` (or `<brand>-enc`); `aac-convert`,
  `aac_ffi`, `aac_decoder` unchanged. Migration: single reviewable change
  set (§11 table); pre-publish timing makes it cheap.
- **NAME-02 — Purge affiliation-implying strings.**
  `Fraunhofer FDK AAC Codec Library in Rust` as product title →
  attribution footnote; keep FDK file headers and `cpp:` comments (they are
  provenance, not branding).
- **NAME-03 — Verify before adopting.**
  Owner checks crates.io/GitHub/domain/search collisions for the shortlist;
  record the decision (one paragraph in the root README or a dated note).
  The audit performs no reservation or verification.

### D. Deferred or avoid

- **DEFER-01 — No broad restructuring.** Workspace/crate/module boundaries
  are sound; do not split crates or move `aac_enc/` submodules without a
  demonstrated reuse or ownership need.
- **DEFER-02 — Do not touch the C/C++ tree.** No deletion, no "cleanup" of
  `cpp/`, build files, or oracle tooling; provenance value exceeds any
  tidiness gain. (If repository size ever matters, revisit with `git-lfs`
  or partial-clone guidance — not deletion.)
- **DEFER-03 — Do not merge or rewrite history docs.** The 45-doc corpus is
  a dated record; entry-point linking (§STRUCT-01) beats consolidation.
- **DEFER-04 — Do not rename FFI headers or C ABI names.** `aacdecoder_lib.h`
  et al. mirror upstream; renaming breaks C consumers for zero benefit.
- **DEFER-05 — Do not expand encoder scope under modernization cover.**
  VBR 1–4, SIMD, true-peak, retag, TOML configurability stay gated by their
  own milestones (L0 D2/D6, L2 §9).
- **DEFER-06 — Do not "fix" exact-string test goldens preemptively.**
  See §14.4; act only on observed flakes.

## 13. Outstanding L3 findings and disposition

| ID | Disposition in this roadmap |
|---|---|
| F1 (P2, no absolute calibration test) | Test-only addition; bundle with release checklist, not with renames. Owner to schedule. |
| F2 (P2, no partial-rerun identity test) | Same as F1. |
| F3 (P2, mixed-layout ruling) | Needs one owner sentence (accept as-is or amend L2); prerequisite tone for release sign-off, no code. |
| F4 (P3, 24-bit post-truncation) | CLEAN-05 (one sentence) + optional fixture test. |
| F5 (P3, `debug_assert!` guard) | No action; recorded. |

Statuses are unchanged by this audit (still open, still non-blocking).

## 14. Risks, unknowns, and claims not verified

1. **No listening/quality evaluation performed.** Lineage claims rest on
   migration-spec §5, M5/M6 reports, and code evidence — not on new
   measurements. The M5.b/M5.25/M5.26 reports are taken as historical
   evidence.
2. **Crates.io publication state unknown** (no network verification per
   audit rules). The rename-cheap-now argument assumes 0.1.0 unpublished;
   confirm before any rename lands.
3. **Name availability explicitly unverified** for all §10 candidates.
4. **Exact-string test goldens vs future platforms** (L3 closeout §7.4):
   carried as watch-item, not finding.
5. **Performance baselines absent** (L2 sets none; L3-prep recorded RSS
...[truncated 911 chars]