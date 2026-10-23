# Current Encoder Release-Gate Addendum — ADTS Audit (2026-10-21)

**Status:** Current-state addendum. This document does not rewrite the historical M6.5 report. It records the verified state at the commit below and supersedes no historical evidence.

- Audit date: 2026-10-21
- Audit HEAD: `21bccf3baf19b64d49dc6f133d4c935ad2e50516`
- Audit branch state: `main`, three commits ahead of `origin/main`, zero behind at the audit
- MSRV policy amendment: after this audit, the workspace minimum was raised to Rust 1.99.1; see §7
- Scope: native Rust AAC-LC VBR-5 encoder, current stable Rust delivery, ADTS correctness, and release-scope documentation
- No encoder, decoder, test, fixture, oracle, manifest, or existing documentation change was made during the audit

## 1. Verdict

**ADTS audit: `PASS WITH FINDINGS`.** No ADTS framing, syntax, configuration, or decode defect was reproduced. The encoder's ADTS streams are structurally valid, decode successfully, and differ from the C reference where the payload or its resulting frame length differs.

**Encoder-only release on Rust 1.99.1: `PASS WITH FINDINGS`.** The current-stable build and test matrix is green. Release remains conditional on explicitly recording the M8 provisional-corpus finding and correcting the unsupported `544/558` claim.

**Broader 19-fixture corpus: NOT APPROVED.** M8 remains `PROVISIONAL`; the qualification evidence and owner acceptance are unresolved.

## 2. Toolchain and build matrix

Toolchain used for every fresh build and test:

```text
rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo 1.99.1 (5f94df478 2026-08-27)
```

All builds used `cargo ... --locked` through `rustup run 1.99.1`, with `CARGO_TARGET_DIR` and `TMPDIR` outside the repository.

| # | Command | Exit | Warnings | Result |
|---|---|---:|---:|---|
| 1 | `rustup run 1.99.1 cargo build --locked` | 0 | 0 | PASS |
| 2 | `rustup run 1.99.1 cargo build --locked --features aac_enc` | 0 | 8 | PASS |
| 3 | `rustup run 1.99.1 cargo build --locked --no-default-features --features aac_enc` | 0 | 9 | PASS |
| 4 | `rustup run 1.99.1 cargo build --locked --no-default-features` | 0 | 2 | PASS |
| 5 | `rustup run 1.99.1 cargo build --locked --package aac_framework --bin aac_decoder` | 0 | 0 | PASS |

The warnings are non-fatal dead-code/unused-item diagnostics in encoder configurations. They are not compatibility failures.

## 3. Test matrix

The encoder suite used the documented external prerequisites:

- `AAC_DEC_BIN=/tmp/m5a-build/aac-dec` — SHA-256 `c5d5ff8dd0b8b91eadb8e0e5df90082bde447a262b92b2b31f2793abdf83efd8`
- `M55_C_DIR` — retained four-stream M55 reference directory
- `PHASE1_ORACLE_DIR` — retained 15-dump oracle directory
- `PHASE1_C_DIR` — retained 15-stream C reference directory
- `PHASE1_OUT`, `TMPDIR`, and `CARGO_TARGET_DIR` — outside the repository

Commands:

```sh
rustup run 1.99.1 cargo test --locked --features aac_enc
rustup run 1.99.1 cargo test --locked --no-default-features --features aac_dec
rustup run 1.99.1 cargo test --locked --features aac_enc m6_api_matches_driver_bytes -- --nocapture
rustup run 1.99.1 cargo test --locked --no-default-features --features aac_dec tp_dec::pce::tests::read -- --exact
```

| Check | Exit | Fresh result |
|---|---:|---|
| Full encoder suite | 0 | **469 passed, 0 failed; 82 doctests passed** |
| Decoder suite | 0 | **256 passed, 0 failed; 81 doctests passed** |
| Migration/API equality | 0 | **1 passed; 20/20 `api-identical`** |
| D-083 named PCE regression | 0 | **1 passed** |
| PCE module | 0 | **13 passed, 0 failed** |

D-083 is closed at this HEAD: `c01cec9` adds the existing PCE element-order check in the parser and retains the negative regression test plus a conformant-PCE positive test. The formerly failing `tp_dec::pce::tests::read` test now passes.

## 4. ADTS audit

### 4.1 Method

The audit used the 19 Rust ADTS streams freshly produced by the 1.99.1 full suite, plus the 15 retained C reference streams at `$TMPDIR/opencode/fc-gate-20261021/c-aac`.

For every Rust stream, an independent parser checked:

- 12-bit syncword `0xFFF`
- MPEG ID 0 (MPEG-4)
- layer 0
- `protection_absent = 1` (7-byte header, no CRC)
- profile 1 (AAC-LC)
- sampling-frequency index (3 for 48 kHz, 4 for 44.1 kHz)
- channel configuration (1 for mono, 2 for stereo)
- private/original/home/copyright flags zero
- VBR buffer fullness `0x7FF`
- one raw data block
- 13-bit declared frame length, actual frame boundary, and exact full-file consumption
- no gaps, overlaps, truncation, or trailing bytes

All 19 streams passed. The declared stream configuration matches the fixture configuration.

### 4.2 C/Rust comparison — 15 synthetic streams, 558 frames

The C and Rust frame counts match exactly for all 15 streams.

| Metric | Count |
|---|---:|
| Frames | 558 |
| Core ADTS fields equal | **558/558** |
| First 3 header bytes equal | **558/558** |
| First 4 header bytes equal | **558/558** |
| First 5 header bytes equal | **479/558** |
| First 6 header bytes equal | **399/558** |
| Full 7-byte headers equal | **399/558** |
| Declared frame length equal | **399/558** |
| Complete ADTS frames equal | **286/558** |
| Core fields equal but frame length differs | 159 |
| Core field-value differences | **0** |

The only C/Rust header differences are the payload-dependent `frame_length` bits. There are no differences in MPEG ID, layer, protection, profile, sampling frequency, channel configuration, flags, buffer fullness, or raw-data-block count.

Complete-frame differences are payload differences, not framing defects. They are expected under the project's non-bit-identical objective. The M5.25 per-fixture `#diff` values for these 15 streams sum to 272, which implies **286 exact frames out of 558** — exactly the fresh result. Header equality and complete-frame equality are therefore consistent with the historical content-divergence table.

### 4.3 Disposition of `544/558`

**`544/558 complete frames exact` is an unsupported historical claim.** No committed or retained report defines the metric, corpus, C reference build, or comparison method that produces 544. No exact `544/558` occurrence was found in the repository, Git history, or retained reports.

The fresh measurable values are:

- **558/558** core ADTS fields equal;
- **399/558** full seven-byte headers equal;
- **286/558** complete frames equal;
- **272** differing complete frames, matching the M5.25 `#diff` sum.

No examined header-prefix or field metric yields 544. The addendum therefore records 544 as an unsupported historical figure, not as a verified gate result. It should not be repeated unless its source and metric are supplied.

### 4.4 Decode/interop evidence

- **FDK `aac-dec`:** the fresh m5a suite decoded four streams (mono-48k, stereo-48k, stereo-44.1k, mono-44.1k) with exit 0 and empty stderr. Decoded WAV hashes match the established records: `6a359695…`, `4d729444…`, `aecb7b97…`, `b8b33083…`.
- **FFmpeg 9.0.2:** all **19/19** existing census streams decoded with exit 0 and zero stderr using `ffmpeg -v error -i <stream> -f null -`.

The FFmpeg check is independent of the Rust encoder. It does not establish perceptual quality; no ABX or listening test was run.

## 5. Oracle, vector, census, and hash evidence

| Check | Result |
|---|---|
| Config-only oracle extraction | **4/4 byte-identical** to committed configs |
| Full vector extraction | **149/160 identical; 11 GSPEC-only differences; 0 missing** |
| Census | **19 streams / 2,442 rows**; `max_iter=0`; minimum `total_avail − sum_bits = 984`; 287 nonzero-fill rows all `tot_fill=7`; `npass=1`; `crash=-1`; ADTS accounting exact |
| Four ADTS hashes | `a9d7b395…` click, `702c8717…` onset, `aa906b52…` sine-441k, `c3b8b5fd…` onset-mono-441k |
| Four decoded WAV hashes | `6a359695…`, `4d729444…`, `aecb7b97…` corrected sine-441k, `b8b33083…` |

The 11 vector differences are confined to `GSPEC` lines in `psy_*` files; they are a known test-vector reproducibility limitation, not an encoder-output defect. Committed per-frame vectors were not regenerated or modified.

## 6. M8 provenance and corpus scope

| Item | Current state |
|---|---|
| `m8-PROVENANCE.txt` | `PROVISIONAL`; SHA-256 `a24e0786dc919da101a30a2b0028f6a7ffdcd3055cc23f8a6b58135102d131cd` |
| M8 owner decision record | Committed at `21bccf3`; SHA-256 `c91902646d8c3ef77ef2b1561d45a1564de4627673689ced49e8ff94c21f661b`; explicitly **not an approval** |
| Qualification report | `V3-MUSIC-FIXTURE-QUALIFICATION.md` remains absent |
| Independent package review | Not documented |
| Owner acceptance | Not recorded; owner fields remain pending |

M8 is test/research-only: production encoder builds do not read the WAV. The current encoder-only release may proceed with the current corpus only if the release gate explicitly records M8's provisional status and open finding. M8 does **not** block production compilation or runtime, and the record does not impose a universal M6.5 requirement to retain M8. The broader 19-fixture corpus must not be described as approved.

## 7. Rust target and MSRV scope

- **Minimum supported Rust:** 1.99.1, declared by `rust-version` in the workspace manifests.
- **Tested delivery target:** Rust 1.99.1; all builds and tests above were executed with that exact toolchain.
- **Historical note:** the previous manifests declared `rust-version = "1.81.0"`. Rust 1.81 was not tested and is no longer a supported target; it is not an outstanding release requirement.

Rust 1.81 is not a supported target and must not be claimed as verified. Only Rust 1.99.1 is currently declared and tested for this release.

## 8. Outstanding findings and closure evidence

| Finding | Blocks encoder-only release? | Closure evidence needed |
|---|---|---|
| `544/558` unsupported | No functional block; release-documentation block | Identify the original metric/source and reproduce it, or remove the figure and use 558/558 core fields, 399/558 headers, 286/558 complete frames |
| M8 provisional | No production block; blocks broader corpus approval | Recover or replace the qualification report, complete independent package review, identify approver, record written acceptance |
| 11 GSPEC-only vector differences | No | Keep vectors frozen; use `--cfg-only` for config regeneration, or document a future determinism fix |
| Isolated clean-checkout build not rerun | No current build failure | Run a clean `git archive` checkout build if the release process requires it |
| Non-fatal warnings | No | Optional cleanup; no compatibility impact |

No encoder code change is recommended solely to increase byte parity with C. The objective is perceptual competitiveness, not bit-identical output, and no ABX or perceptual-equivalence claim is made here.

## 9. Release statement

For an encoder-only release targeting current stable Rust:

> **PASS WITH FINDINGS.** The native Rust AAC-LC VBR-5 encoder builds and tests successfully on Rust 1.99.1, produces structurally valid and decodable ADTS streams, preserves established hashes and census invariants, and has a freshly passing D-083 regression. The release must carry the M8 provisional-corpus finding and must not repeat the unsupported 544/558 complete-frame claim. The minimum supported Rust version is 1.99.1.

This addendum does not approve the broader 19-fixture corpus.

## 10. Evidence references

- `docs/aac-enc-m6-release-gate.md` — historical M6.5 gate, especially §§2, 3, 8
- `docs/aac-enc-quality-divergence-audit-m5.25.md` §F — C/Rust frame divergence table and header/content distinction
- `rust/src/aac_enc/adts.rs` — 7-byte ADTS header writer, MPEG-4/AAC-LC/no-CRC/VBR/one-block fields
- `rust/src/aac_enc/frame.rs:1122-1140` — complete ADTS frame construction
- `rust/src/aac_enc/stream_driver.rs:1620-1660`, `:4259-4275` — existing ADTS parsers and header checks
- `rust/src/aac_enc/m5a_decode.rs` — decode consistency checks and expected WAV hashes
- `docs/aac-enc-m8-owner-decision.md` — committed but non-approving owner decision record
- `rust/src/aac_enc/testdata/m8-PROVENANCE.txt` — provisional provenance record
- `docs/aac-enc-m6-clean-checkout-reproducibility.md` §3, §8 — test-only asset and clean-checkout history

## 11. Integrity

At audit completion:

- HEAD remains `21bccf3baf19b64d49dc6f133d4c935ad2e50516`.
- Branch remains `main`, three commits ahead of `origin/main`, zero behind.
- Index is empty; tracked worktree is clean.
- The nine pre-existing `tools/enc-oracle/__pycache__/*.pyc` files remain untracked and untouched.
- M8 provenance remains `PROVISIONAL` with SHA-256 `a24e0786…d131cd`.
- No commit, amend, rebase, reset, stash, clean, or push was performed.
- All audit builds, tests, decode outputs, and temporary scripts are outside the repository.
- The MSRV policy amendment was made after this audit; it does not change the ADTS, M8, or test evidence recorded above.

## 12. M6.6 CI automation (2026-10-23)

M6.6 adds [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) on a
baseline of commit `0b3b1f9cad4d0b0e5990650f4782cdc3be9c6691`. This section is
a follow-up to the 2026-10-21 audit; it does not change that audit's results
or the M8 and `544/558` findings.

### Tier A — required self-contained CI

The workflow runs on pushes to `main` and pull requests targeting `main`. It
uses GitHub-hosted `ubuntu-latest`, installs Rust `1.99.1` with
`dtolnay/rust-toolchain`, uses `--locked` for Cargo, and runs from `rust/`. It
has no cache dependency, artifact upload, or secret input. At authoring time
the workflow was validated locally with `actionlint` and the commands below;
no hosted GitHub run had been triggered before the workflow was committed.

Required steps:

```sh
cargo build --locked
cargo build --locked --features aac_enc
cargo build --locked --no-default-features --features aac_enc
cargo test --locked --features aac_enc --lib -- \
  --skip m5a_decode_check \
  --skip m55_streams_match_c_reference \
  --skip phase1_frontend_agreement_sine_f5 \
  --skip phase1e_sine_f5_band16_residue \
  --skip phase1e_onset_f24_bands_causality \
  --skip phase1f_multitone_f1_no_live_divergence \
  --skip phase1f_stepdown_f1_no_live_divergence \
  --skip phase1g_long_flips_residue \
  --skip phase1i_sine44_f5_tier_b \
  --skip phase1i_multitone_f5_tier_b \
  --skip phase1_full_stream_parity \
  --skip intensity_probe_c_oracle_parity \
  --skip intensity_probe_wav_parity \
  --skip intensity_probe_adts_decodes \
  --skip m511_profile_loop
cargo test --locked --features aac_enc --doc
cargo test --locked --no-default-features --features aac_dec --lib
cargo test --locked --no-default-features --features aac_dec --doc
cargo package --locked -p aac
```

The excluded 15 tests require retained C artifacts, an external decoder, or
opt-in environment variables:

- nine Phase-1 oracle/reference tests;
- `m5a_decode_check`;
- `m55_streams_match_c_reference`;
- three external IS probe tests;
- the opt-in `m511_profile_loop`.

The other ten Phase-1 tests use committed vectors and remain in the
self-contained Tier A set. The self-contained result is **461 library tests
plus 82 encoder doctests**; the decoder result is **256 library tests plus 81
doctests**. The workflow does not hardcode these counts: any test failure
fails the job. The numbers are the local reference results from the M6.6
validation, not a gate that ignores additional tests.

### Tier B — full release validation, not automated

The complete encoder release suite remains a trusted release-environment job,
not a GitHub-hosted default. It requires:

```sh
AAC_DEC_BIN=/path/to/aac-dec
M55_C_DIR=/path/to/m55-c-aac
PHASE1_ORACLE_DIR=/path/to/phase1-oracle
PHASE1_C_DIR=/path/to/phase1-c-aac
PHASE1_OUT=/path/to/writable/output
```

With those artifacts available, the complete suite was validated locally at
**476 passed, 0 failed; 82 doctests passed**. The CI workflow deliberately
does not claim that full result: missing Tier B artifacts are a blocked
release check, not a Tier A pass. No download URL, credential, checked-in
binary, or unlicensed artifact mechanism is introduced.

### Formatting and lint

`.github/workflows/ci.yml` has a separate `lint-informational` job. It attempts
to install `rustfmt` and `clippy` for Rust 1.99.1 and runs:

```sh
cargo fmt --all -- --check
cargo clippy --locked --features aac_enc --all-targets
```

That job is explicitly non-gating. The repository has documented pre-existing
formatting drift and a non-clean Clippy baseline; CI must not turn those into
a repository-wide failure. The required Tier A job does not depend on these
checks, and no claim of complete formatting or lint coverage is made.

### Release-status boundary

Tier A proves that a clean checkout can build, test, and package the
self-contained encoder/decoder configurations. It does **not** approve the
broader M8 corpus, close the M8 provenance decision, or validate the
unsupported `544/558` historical claim. M8 remains `PROVISIONAL`, and any
release statement must retain that finding.
