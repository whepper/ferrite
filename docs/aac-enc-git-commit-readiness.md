# AAC Encoder M4–M6 Git Commit Readiness Audit

**Audit date:** 2026-10-21
**Repository:** `/Users/jeroen/Documents/VSCode/fdk-aac/fdk-aac`
**Scope:** inventory and commit planning for the accumulated M4/M5/M6 work. Audit only — no staging, commits, builds, tests, generators, or destructive Git operations were performed.

---

## 1. Repository baseline

| Item | Value |
|---|---|
| Branch | `main` |
| HEAD | `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` |
| Upstream | `origin/main` = `1785529e648786a11d1613440f7e7262bdea7137` |
| Divergence | `main` is **ahead 2**, behind 0 |
| Index | empty (`git diff --cached --stat` produced no output) |
| Tracked modifications | 13 files, 1667 insertions(+), 412 deletions(-) |
| Deletions | none |
| Pre-existing untracked files | 46 |
| `git status --short` | 51 lines (13 modified + 38 untracked entries; two are directories) |

The two local commits are `e4258d2` and `53481f3`; neither is on `origin/main`.

---

## 2. Existing history relevant to M4–M6

| Commit | Decoration | Content | Relevance to this audit |
|---|---|---|---|
| `1785529` | `origin/main`, `origin/HEAD` | Native Rust AAC-LC encoder foundation + psychoacoustics (M1–M3) | Base of the published branch |
| `e4258d2` | local `main~1` | `feat(aac-enc): complete native Rust encoder through frame assembly` | **M4 is already committed.** Adds encoder stages/tables (`bitcount`, `bitenc`, `frame`, `pe`, `q31`, `quant`, `rate`, `scf`, `static_bits`, tables, QC vectors). Do not recommit these files. |
| `53481f3` | local `main` (HEAD) | `docs(research): preserve D-021 evidence and closure record` | **D-021 archive is already committed.** Contains only `docs/research/d021/**`; the worktree has no changes under `docs/research/` (verified: 0 status lines). Do not recommit. |
| older | tags/branches | `99356b5` (`v3.0.0`), `8ccbe32`, `4672581`, `8b7fcaa`, `3f95d0d` | Upstream FDK AAC history; not part of M4–M6 |

**Conclusion:** M4 is not an outstanding commit. The outstanding work is the uncommitted M5/M6 layer on top of `53481f3`. The tracked modifications are M5 refinements plus M6 module registration — not a duplicate of `e4258d2`.

---

## 3. Tracked modifications (13)

All 13 are modifications; there are no deletions. "Prod" means compiled into `--features aac_enc` production builds; "Test" means required to compile or run tests.

| Path | Milestone / origin | Purpose | Prod | Test | Depends on | Disposition |
|---|---|---|---|---|---|---|
| `rust/src/aac_enc/bitenc.rs` | M5.14 | Byte-chunk `BitWriter` emission replacing the per-bit loop; test updates | yes | yes | — | C1 |
| `rust/src/aac_enc/tns.rs` | M5.12 | Fused multi-lag integer autocorrelation (`autocorr_quarter_lags`) | yes | yes | — | C1 |
| `rust/src/aac_enc/scf.rs` | M5.13 / M5.17 / M5.19 | Direct-slice `improve_scf`, scratch-band reuse, `ScfWorkspace` | yes | yes | — | C1 |
| `rust/src/aac_enc/quant.rs` | M5.16 | Quantizer state hoist/inline | yes | yes | — | C1 |
| `rust/src/aac_enc/rate.rs` | M5.18 / M5.22 | Final quant arrays moved out; `QcState.bc_scratch` | yes | yes | `bitcount` scratch | C1 |
| `rust/src/aac_enc/energy.rs` | M5.20 | Caller-owned `PsyScratch` `_into` energy helpers | yes | yes | — | C1 |
| `rust/src/aac_enc/psy.rs` | pre-M5.12 fix + M5.20 | Grouped short-block M/S Ld correction (incl. tests) and scratch reuse | yes | yes | `energy` `_into` | C1 |
| `rust/src/aac_enc/bitcount.rs` | M5.22 | `BitCountScratch` + `dyn_bit_count_reuse` | yes | yes | — | C1 |
| `rust/src/aac_enc.rs` | M6.1 (+ M5 registrations) | Declares `encoder`, `psy_config`, production `stream_driver`, and the `#[cfg(test)]` M5/M6 harness modules | yes | yes | all new modules | C2 |
| `rust/src/aac_enc/testdata.rs` | M5.5 / M5.e / M6.1 | WAV loader, `onset-mono-44100`, `stress-stereo`, `m7`/`m8` arms; config loader moved to `psy_config` and re-exported | no (`#[cfg(test)]`) | yes | `psy_config`, 4 WAVs, 4 cfg | C2 |
| `docs/aac-enc-migration-spec.md` | M5.a | Alignment-delay clarification (measured lag, 44.1 kHz still gated) | no | no | report context | C5 |
| `tools/enc-oracle/extract_testdata.py` | M6.4 | `--cfg-only`, additive cfg stems, extracted `extract_configs()` | no | no | 4 cfg snapshots for verification | C3 |
| `tools/enc-oracle/README.md` | M6.4 | Documents config-only regeneration incl. mono-44.1k procedure | no | no | `extract_testdata.py` | C3 |

**Note on C1 numerical status:** `psy.rs` contains a **correctness fix** (short-block M/S Ld), not only performance. The established ADTS/decoded hashes in the M6.5 report were measured on the current working tree (post-fix); they must not be assumed identical to a build of `e4258d2`. The M5/M6 reports record Rust-to-Rust byte-identity across the campaign after this fix; C-oracle divergence (149/160 vectors, 11 `GSPEC`-only) is a separate, documented result.

---

## 4. Untracked Rust source (8)

All are new files. None is already committed.

| Path | Milestone | Purpose | Prod | Test | Depends on | Disposition |
|---|---|---|---|---|---|---|
| `rust/src/aac_enc/encoder.rs` | M6.1 / M6.2 | Public `Encoder`, `AdtsFrame`, typed errors, poison lifecycle, 11 unit tests + doctest | yes | yes | `stream_driver`, `config`, `frame`, `pcm`, `rate` | C2 |
| `rust/src/aac_enc/psy_config.rs` | M6.1 / M6.3 | Production config loader; four `include_str!` snapshots; 2 tests | yes | yes | 4 `cfg_*.txt` files | C2 |
| `rust/src/aac_enc/stream_driver.rs` | M5.b–M6.1 | Production `StreamEncoder`/`StepError` plus test-gated M5 harness, fixtures, census snapshots | yes | yes | all encoder stages, `psy_config`, `testdata` (test only) | C2 |
| `rust/src/aac_enc/m5a_decode.rs` | M5.a | Decode-and-check harness (`AAC_DEC_BIN`), exit/stderr/WAV validation | no | yes | `stream_driver`, `testdata` | C2 |
| `rust/src/aac_enc/m5b_measure.rs` | M5.b | Dumps the four original PCM inputs for metrics | no | yes | `testdata` | C2 |
| `rust/src/aac_enc/m5g_census.rs` | M5.g / M5.i | 19-stream branch-execution and pass-margin census | no | yes | `stream_driver` | C2 |
| `rust/src/aac_enc/m6_migration.rs` | M6.1 / M6.2 | 19+1-stream API-vs-driver byte-equality harness | no | yes | `encoder`, `stream_driver`, `testdata` | C2 |
| `rust/src/aac_enc/phase1b.rs` | Phase 1B (M5.26 era) | Oracle-fed isolated S4/S6 differential, three stereo frames | no | yes | stage modules, `testdata`, external oracle dumps | C2 |

**Why these eight plus the assets are one cluster:** `aac_enc.rs` declares both the production modules and every `#[cfg(test)]` harness module in a single diff. Committing `aac_enc.rs` without any declared module leaves the tree non-compiling; committing `encoder.rs` without `stream_driver.rs` (its `StreamEncoder`/`StepError` dependency) is impossible; and `testdata.rs` both `include_bytes!`-references the four WAVs and re-exports `psy_config`. Splitting would require partial-file staging or temporary edits, which this audit forbids and which would create broken intermediate commits.

---

## 5. Test assets (7)

Hashes are current, recomputed read-only with `shasum -a 256`; all match the M6.5 manifest and provenance files.

| Path | SHA-256 | Milestone | Purpose | Used by | Licence / provenance | Disposition |
|---|---|---|---|---|---|---|
| `rust/src/aac_enc/testdata/cfg_44100_112000.txt` | `ba2a2ee838b4733c0c4e05b451150e6d0481bf8dd1de992ccd2ebe8215ef0745` | M5.e | Init-time PSY/TNS snapshot, mono/44.1 kHz | production `include_str!` in `psy_config.rs`; config tests | Generated numeric data; no third-party content | **C2 (production blocker)** |
| `rust/src/aac_enc/testdata/m55-PROVENANCE.txt` | `470eca4d52fa69c5d26c9efe6e56e1ea1d2d44c725ccdbea7433002d1621c7b0` | M5.5 | Attribution, source URLs, conversion commands, hashes | documentation; CC BY attribution record | Contains the required CC BY 4.0 attribution | **C2** |
| `rust/src/aac_enc/testdata/m7-music-stereo-48k.wav` | `b49e10a1d3f51b80db1d4d1a1179afa9963570528042395df20be8e2193f3086` | M5.5 | Licensed music fixture | `testdata.rs`; `M55_FIXTURES`; `m55`, `m5g` | Wikimedia "Thoughtness" (Kian Moretz), **CC0 1.0** | **C2** |
| `rust/src/aac_enc/testdata/m7-speech-pauses-stereo-48k.wav` | `67d69a64fed23fe16ab666a9e1f02262afa78934363c69999728cadd9ec1a4d3` | M5.5 | Speech-with-pauses fixture | `testdata.rs`; `M55_FIXTURES`; `m55`, `m5g` | LibriSpeech test-clean, **CC BY 4.0** (attribution in `m55-PROVENANCE.txt`) | **C2** |
| `rust/src/aac_enc/testdata/m7-speech-clean-stereo-48k.wav` | `3dfb7dcb0cd9f600f9ae66eb791df6d7c740c833218df88f1de73f87353346ff` | M5.5 | Continuous-speech fixture | `testdata.rs`; `M55_FIXTURES`; `m55`, `m5g` | LibriSpeech test-clean, **CC BY 4.0** (attribution in `m55-PROVENANCE.txt`) | **C2** |
| `rust/src/aac_enc/testdata/m8-PROVENANCE.txt` | `a24e0786dc919da101a30a2b0028f6a7ffdcd3055cc23f8a6b58135102d131cd` | V3 / M5.25–M5.26 | Provenance, public-domain licence evidence, conversion commands, limitations | documentation | Public domain (Musopen); **corpus status PROVISIONAL** (see §6) | **C2 with explicit finding** |
| `rust/src/aac_enc/testdata/m8-music-eroica-stereo-48k.wav` | `4413c8a41fe148192081c504ecfb5d69b5c7c4fc158cefff8b0dfadf8923986b` | V3 / M5.25–M5.26 | Beethoven "Eroica" I excerpt, 48 kHz stereo | `testdata.rs`; `M55_FIXTURES`; `m55`, `m5g` | **Public domain** (Musopen PD release / CC PD Mark 1.0); **corpus acceptance unresolved** | **C2 with explicit finding** |

The three other cfg snapshots (`cfg_48000_96000.txt`, `cfg_48000_112000.txt`, `cfg_44100_96000.txt`) are **already tracked** and must not be recommitted; only `cfg_44100_112000.txt` is new.

---

## 6. m8 provenance and approval (explicit decision)

The file was inspected and **left unchanged**. Licensing and corpus approval are distinct:

- **Licence:** public domain (recording and archive item); redistribution is safe. `AttributionRequired = false`.
- **Corpus approval:** unresolved. `m8-PROVENANCE.txt:4-5` says `Status: PROVISIONAL. The corpus is not final, and this provenance package is not approved, until it has passed independent review.`
- The same file line 53 cites `V3-MUSIC-FIXTURE-QUALIFICATION.md (independent review: PASS)` with report SHA-256 `22010c83…`, but that report is **not in the repository** and was not present in the surviving scratch tree during M6.5.
- `docs/aac-enc-benchmark-plan.md:42-49` independently says the corpus **stays provisional until the updated provenance package passes independent review**.

Therefore the audit does **not** infer approval from the licence or the unverifiable PASS citation. The correct disposition is to commit the asset with its provisional provenance intact (tests depend on it), and record the open owner question:

> Has the V3 music corpus/provenance package been formally accepted as final, and is the independent review referenced as `V3-MUSIC-FIXTURE-QUALIFICATION.md` (SHA-256 `22010c83…`) complete? If yes, where is the authoritative artifact, and may the PROVISIONAL header be replaced?

The proposed commit message for C2 should state that m8 remains provisional. No change to the header, audio, or corpus is proposed. If the owner prefers zero provisional corpus in history, the m8 WAV, provenance, and `testdata.rs` m8 arm must all be held together — which would break test compilation and is not recommended.

---

## 7. Tooling and generated files

| Path | State | Milestone | Purpose | Disposition |
|---|---|---|---|---|
| `tools/enc-oracle/extract_testdata.py` | modified | M6.4 | Config-only extraction; additive cfg stems | **C3** |
| `tools/enc-oracle/README.md` | modified | M6.4 | Documented cfg regeneration incl. mono-44.1k | **C3** |
| `tools/c-enc-bench/c-enc-bench.c` | untracked | M5.10 / M5.11 | Isolated fair C encoder benchmark driver (`BENCH wall_ns=…`); uses tracked `wavreader.h` and `cpp/libAACenc` | **C4** |
| `tools/enc-oracle/__pycache__/extract_s1..s8.cpython-315.pyc` (8) and `extract_testdata.cpython-315.pyc` (1) | untracked | generated | Python bytecode cache | **Held out** — generated, not a milestone artifact |

---

## 8. Report inventory (21 pre-existing + this audit)

All are Markdown reports; none is required to build or test. They are evidence records, not scratch logs.

| Path | Milestone | Purpose | Disposition |
|---|---|---|---|
| `docs/aac-enc-benchmark-plan.md` | M5.7 | Unexecuted Apple Silicon benchmark plan; corpus/identity definitions | C5 |
| `docs/aac-enc-m5-readiness-audit.md` | M5.0 | Pre-M5 readiness audit and gap list | C5 |
| `docs/aac-enc-m5a-decode-check.md` | M5.a | Decode-and-check report; alignment evidence | C5 |
| `docs/aac-enc-m5b-objective-quality-baseline.md` | M5.b | Objective baseline measurements | C5 |
| `docs/aac-enc-m5c-validation-gap-assessment.md` | M5.c | Validation gap assessment / next-step decision | C5 |
| `docs/aac-enc-m5d-flush-termination-validation.md` | M5.d | Flush/termination/skip-policy validation | C5 |
| `docs/aac-enc-m5e-mono44100-infrastructure.md` | M5.e | Mono/44.1 kHz infrastructure restore and validation | C5 |
| `docs/aac-enc-m5f-coding-branch-coverage.md` | M5.f | Coding-branch coverage assessment | C5 |
| `docs/aac-enc-m5g-branch-execution-census.md` | M5.g | 2,442-row branch-execution census | C5 |
| `docs/aac-enc-m5h-retry-crash-trigger-map.md` | M5.h | Retry/crash trigger-threshold mapping | C5 |
| `docs/aac-enc-m5i-pass-margin-census.md` | M5.i | Pass-margin census | C5 |
| `docs/aac-enc-m5j-api-readiness-decision.md` | M5.j | API-readiness decision (`PROCEED`) | C5 |
| `docs/aac-enc-performance-campaign-m5.12-m5.22.md` | M5.24 | Consolidated performance campaign audit | C6 |
| `docs/aac-enc-quality-divergence-audit-m5.25.md` | M5.25 | C-vs-Rust quality/divergence audit | C6 |
| `docs/aac-enc-first-divergence-m5.26.md` | M5.26 | First-divergence stage attribution | C6 |
| `docs/aac-enc-m6-api-contract.md` | M6.0 | Authoritative API lifecycle/error contract | C7 |
| `docs/aac-enc-m6-implementation.md` | M6.1 | API implementation and migration evidence | C7 |
| `docs/aac-enc-m6-hardening-review.md` | M6.2 | Defect fixes and lifecycle matrix | C7 |
| `docs/aac-enc-m6-release-readiness.md` | M6.3 | Config embedding, Panics decision, packaging | C7 |
| `docs/aac-enc-m6-clean-checkout-reproducibility.md` | M6.4 | Asset audit, oracle procedure, CI prerequisites | C7 |
| `docs/aac-enc-m6-release-gate.md` | M6.5 | Release gate, seven-file manifest, clean-state limits | C7 |
| `docs/aac-enc-git-commit-readiness.md` | This audit | Inventory and proposed commit plan | C8 |

---

## 9. Attribution and uncertain cases

**Confirmed:**
- M4 stages/tables and QC vectors: committed at `e4258d2`; no new commit.
- D-021 evidence/closure: committed at `53481f3`; no worktree changes.
- M5.12–M5.22: the eight tracked implementation files (C1), documented in the M5.24 report.
- M5.a–M5.j: the three harness modules `m5a_decode`, `m5b_measure`, `m5g_census`, the `stream_driver` M5 sections, `cfg_44100_112000.txt`, the M5.5/M5.e fixtures, and the M5 reports (C2/C5).
- Phase 1B: `phase1b.rs`, an oracle-fed S4/S6 differential from the M5.26 era, assigned to C2 with the other test-only harnesses.
- M6.0–M6.5: `encoder.rs`, `psy_config.rs`, `m6_migration.rs`, `aac_enc.rs`/`stream_driver.rs` ungating and error plumbing, `testdata.rs` config re-export, M6 reports (C2/C7), and oracle tooling (C3).
- M5.10/M5.11: `tools/c-enc-bench/c-enc-bench.c` (C4), referenced by the M5 readiness audit.

**Medium confidence (pre-M5.12, no dedicated milestone report):**
- `psy.rs` grouped short-block M/S Ld fix and its tests; `testdata.rs` WAV loader, `stress-stereo`, `onset-mono-44100`, and the M5.5 arms. The M5.24 report explicitly says the uncommitted delta spans beyond M5.12–M5.22 and attributes these to pre-M5.16 work; code comments and the M5.7 plan corroborate. They are treated as genuine user work and assigned to C1/C2, not held.

**Uncertain/held:**
- `tools/enc-oracle/__pycache__/**` only. These are generated bytecode files, not attributable to any milestone, and must not be committed.

No other change was found whose origin cannot be confidently attributed to the user's M4–M6 line.

---

## 10. Proposed commit sequence

Eight commits. Every file below is assigned exactly once.

### C1 — `perf(aac-enc): M5 encoder-core optimizations and short-block M/S Ld fix`

**Purpose:** land the internal encoder-core work from pre-M5.12 and M5.12–M5.22 without pulling in the new API.
**Files (8):**
- `rust/src/aac_enc/tns.rs`
- `rust/src/aac_enc/scf.rs`
- `rust/src/aac_enc/bitenc.rs`
- `rust/src/aac_enc/quant.rs`
- `rust/src/aac_enc/rate.rs`
- `rust/src/aac_enc/energy.rs`
- `rust/src/aac_enc/psy.rs`
- `rust/src/aac_enc/bitcount.rs`

**Why together:** these are mutually dependent hot-path changes across existing stages, all modifying already-committed modules; no new module registration is needed. `rate.rs` uses the new `bitcount` scratch; `psy.rs` uses the new `energy` helpers.
**Depends on:** `e4258d2` only.
**Standalone:** yes for build and stage tests. Full corpus/decode/census gates require C2 (the harness does not exist at C1).
**Recommended gates (not run):**
- `cargo build --features aac_enc`
- `cargo build --no-default-features --features aac_enc`
- `cargo test --features aac_enc`
- `cargo test --features aac_enc frame:: quant:: rate::` (S-stage/oracle tests)
- `cargo test --features aac_enc --doc`

**Concern:** `psy.rs` includes a correctness fix; the current ADTS hashes are post-fix and may differ from `e4258d2`. Do not use `e4258d2` output as the compatibility baseline. No vector or baseline file is touched.

---

### C2 — `feat(aac-enc): production encoder API with M5 validation harness and fixtures`

**Purpose:** land the minimal compilable cluster for the production `Encoder` API, the shared streaming driver, the M5 validation harnesses, and all assets they need.
**Files (17):**
- `rust/src/aac_enc.rs`
- `rust/src/aac_enc/encoder.rs`
- `rust/src/aac_enc/psy_config.rs`
- `rust/src/aac_enc/stream_driver.rs`
- `rust/src/aac_enc/m5a_decode.rs`
- `rust/src/aac_enc/m5b_measure.rs`
- `rust/src/aac_enc/m5g_census.rs`
- `rust/src/aac_enc/m6_migration.rs`
- `rust/src/aac_enc/phase1b.rs`
- `rust/src/aac_enc/testdata.rs`
- `rust/src/aac_enc/testdata/cfg_44100_112000.txt`
- `rust/src/aac_enc/testdata/m55-PROVENANCE.txt`
- `rust/src/aac_enc/testdata/m7-music-stereo-48k.wav`
- `rust/src/aac_enc/testdata/m7-speech-clean-stereo-48k.wav`
- `rust/src/aac_enc/testdata/m7-speech-pauses-stereo-48k.wav`
- `rust/src/aac_enc/testdata/m8-PROVENANCE.txt`
- `rust/src/aac_enc/testdata/m8-music-eroica-stereo-48k.wav`

**Why together:** `aac_enc.rs` registers all eight new modules in one diff; `encoder.rs` needs `stream_driver.rs`; `stream_driver.rs` needs `psy_config.rs` and (in tests) `testdata.rs`; `testdata.rs` needs the WAVs and the fourth cfg; the API's migration harness needs all of them. Splitting requires partial-file staging and produces non-compiling commits.
**Depends on:** C1 (uses `process_pair_reuse`, `dyn_bit_count_reuse`, `estimate_scale_factors_channel_reuse`, `QcState.bc_scratch`).
**Standalone:** no — requires C1.
**Recommended gates (not run; exact commands in `docs/aac-enc-m6-release-gate.md` §5):**
- all three production builds; `cargo package` + unpacked `cargo check --features aac_enc`
- out-of-crate consumer build/run + decode
- `encoder::` (11), `psy_config::` (2), doctests (82)
- `m6_api_matches_driver_bytes` (20/20)
- `m52`, `m55` (`M55_C_DIR`), `m5a` (`AAC_DEC_BIN`), `m5b_dump`, `m5g`
- `frame::` / `quant::` / `rate::`; `phase1` 19/19 with `PHASE1_ORACLE_DIR`/`PHASE1_C_DIR`
- four ADTS hashes, four decoded WAV hashes, M5.b metrics JSON, 2,442-row census
- Expect the known pre-existing `tp_dec::pce::tests::read` failure (D-083); it is not an M6 regression.

**Concerns:** binary assets total ~7.5 MB; m7 assets are CC0/CC BY (attribution included); m8 is public domain but **corpus status remains PROVISIONAL** — the commit message must say so, and the header must not be changed. The commit makes the tree test-compiling and self-contained, but a literal clean checkout of the current Git state remains impossible until this commit exists.

---

### C3 — `tools(enc-oracle): add config-only snapshot regeneration`

**Purpose:** land the M6.4 `--cfg-only` path and its documented procedure.
**Files (2):**
- `tools/enc-oracle/extract_testdata.py`
- `tools/enc-oracle/README.md`

**Why together:** documentation and implementation are a single usability change; the README examples exercise the new flags.
**Depends on:** C2 for the fourth snapshot in the documented output set (the tool itself is independent).
**Standalone:** yes after C2.
**Recommended gates (not run):**
- `--cfg-only` with the three built-in stems and again with `onset-mono-44100`; compare all four hashes to the committed files.
- Full extraction into a fresh directory: expect 149/160 byte-identical, 11 `GSPEC`-only differences. **Do not overwrite the committed vectors.**

**Concern:** the full-extraction non-determinism is documented, not accepted; config-only is the supported path.

---

### C4 — `tools(bench): add isolated C encoder benchmark driver`

**Purpose:** preserve the M5.10/M5.11 fair C benchmark harness used by the performance campaign.
**Files (1):**
- `tools/c-enc-bench/c-enc-bench.c`

**Why separate:** a C tool, not required by the Rust build or tests; committing it with C1 would mix languages and unrelated gate requirements.
**Depends on:** nothing (uses tracked `wavreader.h` and `cpp/libAACenc/include/aacenc_lib.h`).
**Standalone:** yes.
**Recommended gate (not run):** compile against the in-repo `libfdk-aac` and smoke-run on one fixture; check the `BENCH wall_ns=…` output. No automated test exists.
**Concern:** no explicit SPDX/licence header; it is project-authored code under the repository licence. Optional follow-up: add a header, but do not block the commit.

---

### C5 — `docs(aac-enc): M5 validation reports and API-readiness decision`

**Purpose:** archive the M5.0–M5.j evidence trail in milestone order, including the alignment-spec clarification.
**Files (13):**
- `docs/aac-enc-migration-spec.md` (tracked modification)
- `docs/aac-enc-benchmark-plan.md`
- `docs/aac-enc-m5-readiness-audit.md`
- `docs/aac-enc-m5a-decode-check.md`
- `docs/aac-enc-m5b-objective-quality-baseline.md`
- `docs/aac-enc-m5c-validation-gap-assessment.md`
- `docs/aac-enc-m5d-flush-termination-validation.md`
- `docs/aac-enc-m5e-mono44100-infrastructure.md`
- `docs/aac-enc-m5f-coding-branch-coverage.md`
- `docs/aac-enc-m5g-branch-execution-census.md`
- `docs/aac-enc-m5h-retry-crash-trigger-map.md`
- `docs/aac-enc-m5i-pass-margin-census.md`
- `docs/aac-enc-m5j-api-readiness-decision.md`

**Why together:** these reports reference the C2 harness/assets and the M5.12–M5.22 work; landing them after the code keeps the evidence interpretable. The migration-spec edit is the M5.a alignment clarification.
**Depends on:** C1 and C2 (content references them).
**Standalone:** documentation only; no build impact.
**Recommended gate (not run):** verify internal links/paths and that the recorded commands still match the committed tree. Do not re-run the measurements for this commit.

---

### C6 — `docs(aac-enc): M5 performance campaign and C-vs-Rust divergence audits`

**Purpose:** archive the M5.24–M5.26 diagnostic record.
**Files (3):**
- `docs/aac-enc-performance-campaign-m5.12-m5.22.md`
- `docs/aac-enc-quality-divergence-audit-m5.25.md`
- `docs/aac-enc-first-divergence-m5.26.md`

**Why together:** the campaign audit and the two divergence audits form one diagnostic chain; the campaign report documents C1.
**Depends on:** C1 and C2.
**Standalone:** documentation only.
**Recommended gate (not run):** confirm the cited commands/paths exist; no code execution.

---

### C7 — `docs(aac-enc): M6 API contract, hardening, and release-gate evidence`

**Purpose:** archive the M6.0–M6.5 contract, implementation, hardening, readiness, reproducibility, and release-gate record.
**Files (6):**
- `docs/aac-enc-m6-api-contract.md`
- `docs/aac-enc-m6-implementation.md`
- `docs/aac-enc-m6-hardening-review.md`
- `docs/aac-enc-m6-release-readiness.md`
- `docs/aac-enc-m6-clean-checkout-reproducibility.md`
- `docs/aac-enc-m6-release-gate.md`

**Why together:** one coherent M6 narrative; splitting would separate the release gate from the evidence it aggregates.
**Depends on:** C2 and C3.
**Standalone:** documentation only.
**Recommended gate (not run):** confirm that the M6.5 report's claims still match the committed tree, especially the seven-file manifest and the clean-checkout limitation. Do not "fix" the m8 status.

---

### C8 — `docs(aac-enc): add git commit-readiness inventory`

**Purpose:** commit this audit as the final documentation-only change.
**Files (1):**
- `docs/aac-enc-git-commit-readiness.md`

**Why separate:** it describes the commit plan itself; it should not be mixed into any milestone commit.
**Depends on:** all previous commits.
**Standalone:** documentation only.

---

## 11. Dependency order

```
C1 ──► C2 ──► C3 ──► C7
        │
        ├────► C5
        └────► C6

C4  (independent; can be committed any time after the baseline)
C8  (last)
```

C2 is the critical cluster. C1 must precede it; C3/C5/C6/C7 must follow it. C4 is independent.

---

## 12. Files held out

| Path | Reason | Recommended action |
|---|---|---|
| `tools/enc-oracle/__pycache__/extract_s1..s8.cpython-315.pyc` (8) | Generated Python bytecode; not a milestone artifact; already regenerated by tool use | Do not commit. Optionally delete or add `__pycache__/` to `.gitignore` in a separate housekeeping change (not proposed here). |
| `tools/enc-oracle/__pycache__/extract_testdata.cpython-315.pyc` (1) | Same | Same |

The m8 corpus is **not** held: its licensing is safe and tests require it. It is committed with the PROVISIONAL provenance intact and the owner question recorded. This is deliberately not an approval.

---

## 13. Reconciliation

| Commit | Files | Modified | Untracked | Notes |
|---|---:|---:|---:|---|
| C1 | 8 | 8 | 0 | Encoder-core work |
| C2 | 17 | 2 | 15 | API + M5 harness + 7 assets |
| C3 | 2 | 2 | 0 | Oracle tooling |
| C4 | 1 | 0 | 1 | C benchmark |
| C5 | 13 | 1 | 12 | M5 reports + migration spec |
| C6 | 3 | 0 | 3 | M5 diagnostics |
| C7 | 6 | 0 | 6 | M6 reports |
| C8 | 1 | 0 | 1 | This audit |
| **Assigned** | **51** | **13** | **38** | |
| Held out | 9 | 0 | 9 | `__pycache__` |
| **Total** | **60** | **13** | **47** | 46 pre-existing untracked + this report |

Checks:
- Every one of the 13 tracked modifications appears exactly once (8 in C1, 2 in C2, 2 in C3, 1 in C5).
- Every pre-existing untracked file appears exactly once (15 in C2, 1 in C4, 12 in C5, 3 in C6, 6 in C7) or is held (9).
- No file appears in two commits.
- Already-committed M4 and D-021 files are excluded.
- The three tracked cfg snapshots are excluded; only the fourth is added.

---

## 14. Non-interference confirmation

- **No staging:** `git add`/interactive staging was not run; `git diff --cached` remains empty.
- **No commits:** HEAD is unchanged at `53481f3a`.
- **No destructive operations:** no reset, restore, clean, stash, rebase, cherry-pick, amend, or branch switch.
- **No tests/builds/generators:** no `cargo` build or test, no CMake build, no oracle extraction, no fixture regeneration.
- **Read-only inspection:** `git status`, `git diff`, `git log`, `git show`, `git ls-files`, `git rev-parse`, `git rev-list`, `shasum`, `ls`, `grep`, `sed`, and `find` only.
- **Only file created:** `docs/aac-enc-git-commit-readiness.md`.
- **Pre-existing work preserved:** all 13 modified and 46 pre-existing untracked files remain in place.

The proposed staging commands are intentionally omitted from execution. The user should review the plan and authorize each commit explicitly.