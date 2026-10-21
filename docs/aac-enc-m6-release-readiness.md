# M6.3 — Release Readiness and Packaging Audit

Release-readiness milestone. No codec, bitstream, baseline, or oracle-output changes.

**Verdict: `PASS WITH FINDINGS`**

The encoder is now self-contained at runtime (configuration snapshots are compiled in), terminal failure paths are covered through a test-only seam, and every compatibility gate reproduces unchanged. The remaining release blockers are repository-hygiene actions outside this milestone's allowed operations: seven generated/licensed assets are still untracked, one of which is required to build the encoder from a clean checkout.

## 1. Worktree protection (initial state)

| Item | Value |
|---|---|
| HEAD | `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` |
| Branch | `main` |
| `git status --short` | 46 lines (pre-existing work incl. M6.1/M6.2) |
| Staged | empty |
| Tracked diff | 11 files, 1604 insertions(+), 398 deletions(-) |

`docs/aac-enc-m6-release-readiness.md` did not exist. Pre-edit hashes of files M6.3 might touch were recorded (`/tmp/m63-pre-edit-hashes.txt`):

```
d18d7329…  rust/src/aac_enc/encoder.rs        (untracked, M6.1)
13f95649…  rust/src/aac_enc/psy_config.rs     (untracked, M6.1)
ed85cb88…  rust/src/aac_enc/testdata.rs       (tracked, already modified by M6.1)
003e8354…  docs/aac-enc-m6-hardening-review.md
```

## 2. Configuration fixture provenance and packaging (Scope A)

### 2.1 What the file is

`rust/src/aac_enc/testdata/cfg_44100_112000.txt` is a **generated, committed init-time configuration snapshot**, not a build artifact and not an ordinary test vector. It supplies the long/short `PSY_CONFIGURATION` arrays and `TNSCFG` (offsets, PCM quantizers, masking thresholds, min-SNR, TNS parameters) used by `PsyTables::for_rate` in `StreamEncoder::new`. Without it, the mono/44.1 kHz encoder cell cannot be constructed.

Verified properties:

| Property | Value |
|---|---|
| Size / lines | 4281 B / 18 lines |
| SHA-256 | `ba2a2ee838b4733c0c4e05b451150e6d0481bf8dd1de992ccd2ebe8215ef0745` |
| M5.e recorded hash | `ba2a2ee8…0745` — **exact match** |
| Content | Numeric fields only (`CFG`/`TNSCFG` headers, integer arrays); no paths, host names, timestamps, or environment-dependent data |
| Diff vs `cfg_44100_96000.txt` | Only the two `MINSNR` lines differ (LONG + SHORT), exactly as M5.e documents for the 112000-vs-96000 operating point |
| Git status | `??` (untracked) |

Provenance chain (re-verified against `tools/enc-oracle/`):

1. `enc_oracle --configs` (or any oracle run) emits `CFG LONG/SHORT` + `TNSCFG` blocks for the 12 LC rates × both VBR-5 operating points.
2. `tools/enc-oracle/extract_testdata.py` parses those blocks and writes `cfg_<rate>_<pch>.txt`.
3. The repo script's `CFG_SOURCES` covers three of the four files (48000/96000, 48000/112000, 44100/96000). The fourth was produced in M5.e through the **same `parse()` + config-write code path**, with `CFG_SOURCES` restricted to the mono-44100 stem and the per-frame vector loop stubbed so committed vectors were never rewritten (M5.e §2, steps 4–6). That is why the file's hash and diff match the recorded evidence exactly.
4. The regeneration input WAV (`onset-mono-44100.orig.wav`, SHA-256 `df25ddc7…9a80c6`) is produced by the test-gated `m5b_dump_inputs`; the oracle then emits the `chBitrate=112000` dump.

**Reproducibility finding:** the current repository script alone cannot regenerate this fourth file (its `CFG_SOURCES` lacks the mono-44100 stem). The file is nevertheless fully provenance-verified by hash and content diff. Recommended follow-up: add the stem to `CFG_SOURCES` and a cfg-only extraction mode (running the full script also rewrites committed per-frame vectors).

### 2.2 Other configuration files

The other three `cfg_*.txt` files are tracked. No other production configuration file has this dependency. All four now share one packaging mechanism (§2.3).

### 2.3 Packaging mechanism (changed)

M6.1/M6.2 loaded the snapshots at runtime from `CARGO_MANIFEST_DIR`. That made installed binaries depend on the crate source directory persisting, and made a missing file a runtime panic. M6.3 embeds all four snapshots with `include_str!` (`psy_config.rs:21-24`), selected by a compile-time match (`:28-40`). Consequences:

- No runtime filesystem, installation, or source-tree dependency.
- A missing snapshot is a **compile-time error**, not a deployment-time panic.
- `cargo package` includes the files by default (no `include`/`exclude` in `rust/Cargo.toml`); verified: the built `aac-0.1.0.crate` contains all four, and the unpacked crate builds with `--features aac_enc`.
- The file still **must be tracked in git**: `cargo package` from a clean checkout only includes tracked files, and `include_str!` requires the file at build time.

**Smallest safe release action: `git add rust/src/aac_enc/testdata/cfg_44100_112000.txt`** (not performed here; staging/committing is prohibited). Do not add it to `.gitignore`.

## 3. Constructor failure contract (Scope B)

**Decision: the missing-configuration case is a packaging/invariant failure, not a user configuration error; it is eliminated rather than typed.**

Rationale:

- `EncoderConfig` can only exist in validated form (private fields, validating constructors), so the only way configuration loading could fail was a missing/corrupt crate asset — an installation defect.
- The contract has no I/O error variant. `EncodeError::Config(ConfigError)` exists but `ConfigError` models validation (rate/channel/frame-length), not asset I/O; reusing it would distort the error model, and adding a variant would be a public API/contract revision with no user-recoverable action.
- Embedding removes the failure mode entirely: the precondition is now "the four snapshots were present at build time", enforced by the compiler. The public signature, error enum, and lifecycle contract are unchanged.
- The `# Panics` documentation added in M6.2 is therefore stale and was replaced with a statement that construction has no runtime filesystem dependency (`encoder.rs:180-184`).

Focused tests (`psy_config.rs:156-190`):

- `all_validated_snapshots_parse` — all four `(rate, pch)` pairs parse with consistent array lengths.
- `unsupported_combination_panics` — an unvalidated pair panics with `"no committed config snapshot"` (loud, no silent fallback). This path is unreachable through `Encoder::new`, which screens the matrix first.

Evidence that a missing asset now fails at build time (scratch copy of the packaged crate, file removed):

```
error: couldn't read `src/aac_enc/testdata/cfg_44100_112000.txt`: No such file or directory
  --> src/aac_enc/psy_config.rs:22:32
error: could not compile `aac` (lib) due to 1 previous error
exit 101
```

## 4. Terminal failure-path evidence (Scope C)

A `#[cfg(test)]`-only field and setter (`encoder.rs:170-174, 204, 274-292, 304-311`) inject a terminal failure at a chosen frame index **after** `step_frame` has run, so the inner encoder state advances exactly as in a real failure. The hook is absent from production builds (verified: 0 occurrences of `failure_injection`/`inject_failure` in `target/debug/libaac.rlib` strings).

New tests (`encoder.rs:635-700`):

| Test | What it proves |
|---|---|
| `push_failure_poisons_and_discards_in_call_frames` | 4-frame push fails at index 2 → `FrameFailed { frame_index: 2, completed_before: 2 }`; the two completed in-call frames are discarded; state poisoned; later `push`/`flush` return `Poisoned` |
| `flush_failure_poisons_and_emits_nothing` | 1500 samples (1 push frame + buffered tail), flush fails at index 1 → `FrameFailed { frame_index: 1, completed_before: 0 }`; no tail frames escape; state poisoned; later calls return `Poisoned` |

These exercise the real `emit_frames` failure arm for both entry points, including the `completed_before` accounting and the no-partial-output guarantee. They use zero PCM and an injected state transition; no artificial audio and no codec changes.

Residual limits: the injection constructs `EncodeFailure` directly, so it does not re-exercise classification (covered separately by `outcome_mapping_table`), and no natural encoder input on the validated corpus triggers these branches (M5.g/M5.h). The injection is test-only by construction and cannot be reached through the public API.

## 5. Commands and outcomes

All commands from `rust/` unless noted.

| Command | Outcome |
|---|---|
| `cargo test --features aac_enc encoder::` | **11 passed**, 0 failed |
| `cargo test --features aac_enc psy_config::` | **2 passed**, 0 failed |
| `cargo test --features aac_enc --doc` | **82 passed** (incl. the `Encoder` example) |
| `cargo test --features aac_enc m6_api_matches_driver_bytes` | **1 passed**, 20/20 streams api-identical |
| `cargo test --features aac_enc m52_all_fixtures_write_complete` | 1 passed |
| `AAC_DEC_BIN=/tmp/m5a-build/aac-dec cargo test --features aac_enc m5a_decode_check` | 1 passed |
| `cargo test --features aac_enc m5b_dump_inputs` | 1 passed |
| `M55_C_DIR=$TMPDIR/m5d-c55 cargo test --features aac_enc m55_streams_match_c_reference` | 1 passed |
| `cargo test --features aac_enc m5g_branch_census` | 1 passed |
| `cargo test --features aac_enc frame::` / `quant::` / `rate::` | 8 / 9 / 20 passed |
| `PHASE1_ORACLE_DIR=… PHASE1_C_DIR=… cargo test --features aac_enc phase1` | **19 passed**, 0 failed |
| Full suite with oracle/decoder env vars | **467 passed, 1 failed** — `tp_dec::pce::tests::read` (D-083, pre-existing; see §9) |
| `cargo test --no-default-features --features aac_enc encoder::` / `psy_config::` | 11 / 2 passed |
| `cargo build --features aac_enc` | OK; 9 warning lines, 0 in M6.3 files |
| `cargo build` (default `aac_dec`) | OK |
| `cargo build --no-default-features --features aac_enc` | OK |
| `cargo package --allow-dirty -p aac` | OK (packaged build verified); `.crate` = 7,167,373 B |
| `cargo package --list --allow-dirty -p aac` | Includes all four `cfg_*.txt`, `encoder.rs`, `psy_config.rs` |
| Unpacked `.crate` + `cargo check --features aac_enc` | OK |
| Unpacked `.crate` minus `cfg_44100_112000.txt` | Fails at `include_str!` (exit 101) — clean-checkout blocker demonstrated |
| Out-of-crate consumer (`rustc --extern aac=…libaac.rlib`) | Compiles, runs, emits a stream that decodes (exit 0, empty stderr, mono 48 kHz, 304 samples for 2 delay frames) |
| `rustfmt --check` on the three touched sources | 0 diffs |
| `cargo clippy --features aac_enc --all-targets` | 0 findings in `encoder.rs`/`psy_config.rs`; 156 pre-existing warning lines remain crate-wide (not cleaned, per scope) |

## 6. Compatibility and historical baselines (unchanged)

- **ADTS hashes:** `a9d7b395…` (click-mono), `702c8717…` (onset-stereo), `aa906b52…` (sine-441k), `c3b8b5fd…` (onset-mono-441k).
- **Decoded WAV hashes:** `6a359695…`, `4d729444…`, `aecb7b97…`, `b8b33083…`; decoder exit 0 with empty stderr on all four.
- **M5.b metrics:** `/tmp/m63-metrics.json` is identical to the recorded baseline for the three historical cases; only the additive M5.e key is new.
- **Census:** 19 streams, **2,442 rows**, `max iter = 0`, minimum `avail − sum = 984` bits, `tot_fill` max 7 bits, 287 nonzero-fill frames, zero crash, zero `npass > 1`. Per-stream fill distribution matches the M5.g table (the historical §5 sentence "231 frames" remains inconsistent with its own table, which sums to 287; documentation-only, unchanged).
- **Migration equality:** all 20 established streams byte-identical through the public API.

## 7. External prerequisites and reproducibility (Scope D)

Repository-provided: oracle sources (`tools/enc-oracle/`), fixture generators (`make_fixtures.py`, `m5b_dump_inputs`), committed expected vectors, decoder/encoder reference source.

External/derived (required for the optional phase1 diagnostics):

1. A configured reference build exporting `libfdk-aac` (M5.a used `/tmp/m5a-build`, Apple clang 21.0.0 / cmake 4.4.4).
2. `enc_oracle` built with the README command from `cpp/` headers and that library.
3. Fixture WAVs: `make_fixtures.py` for the 15 standard stems, plus `cargo test --features aac_enc m5b_dump_inputs` for `onset-mono-44100`.
4. Oracle dumps (`<stem>.s1.txt`; the second CLI argument is the dump file) and C reference ADTS (`aac-enc <wav> <stem>.aac`).
5. `PHASE1_ORACLE_DIR`, `PHASE1_C_DIR`, optional `PHASE1_OUT`; `AAC_DEC_BIN` for M5.a; `M55_C_DIR` for M5.5.

Verified sequence (scratch only):

```sh
S="$TMPDIR/opencode/m63-oracle"
mkdir -p "$S/wav" "$S/oracle" "$S/c"
python3 tools/enc-oracle/make_fixtures.py "$S/wav"
# plus the mono-44100 input from: (cd rust && cargo test --features aac_enc m5b_dump_inputs)
for w in "$S"/wav/*.wav; do
  stem=$(basename "$w" .wav)
  /tmp/m5a-build/enc_oracle "$w" "$S/oracle/$stem.s1.txt"
  /tmp/m5a-build/aac-enc    "$w" "$S/c/$stem.aac"
done
PHASE1_ORACLE_DIR="$S/oracle" PHASE1_C_DIR="$S/c" \
  cargo test --features aac_enc phase1     # 19 passed
```

Reused verified scratch data from M6.2 for this run (`$TMPDIR/opencode/m62-oracle`, 15 dumps + 15 C refs); no oracle output or expected value was modified.

## 8. Release blockers and next steps

1. **Track `rust/src/aac_enc/testdata/cfg_44100_112000.txt`.** Without it a clean checkout cannot build `--features aac_enc` (compile error, demonstrated). This is the one production blocker.
2. **Track the six test-only assets** (`m55-PROVENANCE.txt`, three `m7-*.wav`, `m8-PROVENANCE.txt`, `m8-music-eroica-stereo-48k.wav`). They are loaded with `include_bytes!` by `#[cfg(test)]` code, so a clean checkout cannot compile the test suite without them. They do not affect production builds.
3. **Oracle regeneration tooling:** add the mono-44100 stem to `CFG_SOURCES` and a cfg-only extraction mode so the fourth snapshot is reproducible by the canonical script; until then use the documented M5.e restricted procedure.
4. **CI:** provision `PHASE1_ORACLE_DIR`/`PHASE1_C_DIR` (and `AAC_DEC_BIN`, `M55_C_DIR`) as in §7; otherwise classify phase1 as prerequisite-gated.
5. **Crate metadata:** `cargo package` warns that `documentation`/`homepage`/`repository` are unset. Release polish, out of scope here.
6. **D-083:** `tp_dec::pce::tests::read` remains the only full-suite failure; it reproduces without `aac_enc` and is unrelated.

## 9. Test classification

- **Passed without external prerequisites:** all encoder/API/doctest/migration/M5 regression/frame/quant/rate tests (11 + 2 + 82 + 20-stream equality + 1+1+1+1+1 + 8 + 9 + 20).
- **Passed with documented prerequisites:** 19 phase1 diagnostics (oracle + C reference + decoder), full suite 467 tests.
- **Failed:** `tp_dec::pce::tests::read` only — known D-083, pre-existing, reproduces under `--no-default-features --features aac_dec`.
- **Blocked:** none in this environment. Clean-checkout encoder build is blocked *by design* until the untracked snapshot is committed.

## 10. Final state and non-interference accounting

| Item | Value |
|---|---|
| HEAD | `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (unchanged) |
| Branch | `main` |
| Staged | empty |
| Tracked diff | 11 files, 1603 insertions(+), 392 deletions(-) |
| `git status --short` | 46 pre-existing lines + this report = 47 |

The tracked-diff delta vs the initial baseline (1604/398 → 1603/392) is a **reduction**: M6.3 restored `testdata_path` to its pre-M6.1 location in `testdata.rs`, removing that hunk from the tracked diff. No pre-existing work was removed; all other status lines are identical.

Files changed by M6.3:

1. `rust/src/aac_enc/psy_config.rs` — compile-time embedding, loader selection, provenance/packaging docs, 2 tests.
2. `rust/src/aac_enc/encoder.rs` — stale panic doc removed, test-only failure seam, 2 terminal-path tests.
3. `rust/src/aac_enc/testdata.rs` — `testdata_path` restored locally; re-export narrowed.
4. `docs/aac-enc-m6-release-readiness.md` — this report.

No staging, commits, stashes, resets, restores, cleans, rebases, or pushes. No hash, baseline, fixture, oracle output, or expected result was modified. Packaging and oracle artifacts live only under `rust/target/package/` and `$TMPDIR/opencode/m6*`.