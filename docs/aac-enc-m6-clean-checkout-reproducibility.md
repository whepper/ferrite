# M6.4 — Clean-Checkout Reproducibility

Packaging and validation-infrastructure milestone. No encoder, psychoacoustic, bitstream, baseline, or oracle-output changes.

**Verdict: `PASS WITH FINDINGS`**

Clean-checkout reproducibility is now **characterized and reproducible except for seven untracked assets that only a repository commit can fix** (staging/committing is prohibited here). The mono-44.1 kHz config regeneration is now a canonical, verified tool workflow; the remaining blockers are explicit `git add` actions and one human licensing/metadata reconciliation.

## 1. Initial state

| Item | Value |
|---|---|
| HEAD | `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` |
| Branch | `main` |
| `git status --short` | 47 lines (pre-existing M6.1–M6.3 work) |
| Staged | empty |
| Tracked diff | 11 files, 1603 insertions(+), 392 deletions(-) |

`docs/aac-enc-m6-clean-checkout-reproducibility.md` did not exist.

## 2. Production configuration snapshot

`rust/src/aac_enc/testdata/cfg_44100_112000.txt` is a **generated init-time configuration snapshot required to build the `aac_enc` feature** (mono/44.1 kHz table setup), not a build output.

| Property | Verified value |
|---|---|
| Size / lines | 4281 B / 18 lines |
| SHA-256 | `ba2a2ee838b4733c0c4e05b451150e6d0481bf8dd1de992ccd2ebe8215ef0745` |
| M5.e record | `ba2a2ee8…0745` — exact match |
| Contents | Numeric `CFG`/`TNSCFG` fields only; no paths, host data, or timestamps |
| Git status | **untracked (`??`)** |
| Siblings | `cfg_44100_96000.txt`, `cfg_48000_112000.txt`, `cfg_48000_96000.txt` — all tracked |
| Embedding | `include_str!` in `psy_config.rs:21-24`, selection match unchanged (`:28-40`) |

**Minimal release action: `git add rust/src/aac_enc/testdata/cfg_44100_112000.txt`.** No code or manifest change is needed; `cargo package` includes it from the file system, but a clean checkout cannot compile without it. Demonstrated on a scratch copy of the packaged crate with only that file removed:

```
error: couldn't read `src/aac_enc/testdata/cfg_44100_112000.txt`: No such file or directory
  --> src/aac_enc/psy_config.rs:22:32
error: could not compile `aac` (lib) due to 1 previous error
exit 101
```

The file was **not** regenerated, replaced, or staged. Its regeneration now produces a byte-identical copy (§4).

## 3. Test-only assets: provenance and distribution

All six assets are needed to compile the test suite (`include_bytes!` in `testdata.rs:442-454`) and are exercised by `m55`/`m5g` and the full 19-stream census.

| Asset | Role | Provenance / licence | SHA-256 | Distribution decision |
|---|---|---|---|---|
| `m55-PROVENANCE.txt` | Attribution + conversion record for m7 | Documents CC0 (music) and CC BY 4.0 (speech); includes required attribution | — | **Commit** (metadata; carries the CC BY attribution) |
| `m7-music-stereo-48k.wav` | Licensed music fixture | Wikimedia Commons "Thoughtness" by Kian Moretz, **CC0 1.0** (PD dedication) | `b49e10a1…` | **Commit**; attribution not required but provenance retained |
| `m7-speech-pauses-stereo-48k.wav` | Speech with pauses | LibriSpeech `test-clean` (OpenSLR 12), **CC BY 4.0**, speaker 4507/16021/0047 | `67d69a64…` | **Commit with attribution** (already in `m55-PROVENANCE.txt`) |
| `m7-speech-clean-stereo-48k.wav` | Continuous speech | LibriSpeech `test-clean`, **CC BY 4.0**, speaker 1995/1836/0004 | `3dfb7dcb…` | **Commit with attribution** |
| `m8-PROVENANCE.txt` | Provenance for the V3 real-music fixture | Musopen/Commons record; PD evidence and limitations | — | **Commit** |
| `m8-music-eroica-stereo-48k.wav` | V3 real-music baseline | Beethoven Eroica, Musopen Symphony; **public domain** (Musopen PD release; CC PD Mark 1.0), attribution not required | `4413c8a4…` | **Commit** (public domain); one metadata question for the owner, below |

Notes and required human decision:

- All m7/m8 byte hashes match their provenance records.
- **m8 status inconsistency:** the provenance header says the corpus is "PROVISIONAL … not approved, until it has passed independent review", while the same file cites `V3-MUSIC-FIXTURE-QUALIFICATION.md` with "independent review: PASS" and §"Verified" says two runs were byte-identical despite limitation 6 calling the conversion "unvalidated". Licensing is unambiguous public domain, so this is **not a redistribution blocker**; the corpus owner should reconcile the provisional header with the PASS report before release.
- **Absence behaviour verified:** removing `m7-music-stereo-48k.wav` from a scratch packaged crate produced `error: couldn't read src/aac_enc/testdata/m7-music-stereo-48k.wav: No such file or directory` at the exact `include_bytes!` line. The prerequisite error is explicit, not an opaque link failure. No test was weakened or removed; converting these to runtime loads would only defer a deterministic packaging failure and weaken the documented self-contained fixture design.
- Nothing was uploaded, published, or redistributed.

## 4. Canonical mono-44.1 kHz oracle regeneration

### Tool change (small, isolated)

`tools/enc-oracle/extract_testdata.py` gained:

- `--cfg-only` — skip all per-frame vector writes and emit config snapshots only.
- Optional trailing `cfg-stem` arguments — additive config sources (e.g. `onset-mono-44100`).
- The config-writing loop was extracted verbatim into `extract_configs()`; the per-frame extraction code is untouched (confirmed by `git diff`).

`tools/enc-oracle/README.md` now documents the workflow (new "Config-only regeneration" section, no absolute machine paths added; uses `$BLD`/`$TMPDIR`).

### Verified procedure

```sh
# 1. Mono 44.1 kHz input (test-gated generator, deterministic)
(cd rust && cargo test --features aac_enc m5b_dump_inputs)
# -> $TMPDIR/m5b-quality/inputs/onset-mono-44100.orig.wav
#    SHA-256 df25ddc788ae280b21dce150caf47fb9f2781361d3016cf29aa91be4cf9a80c6
#    (matches M5.e)

# 2. Oracle dump for that input
"$BLD/enc_oracle" "$TMPDIR/m5b-quality/inputs/onset-mono-44100.orig.wav" \
  "$BLD/oracle/onset-mono-44100.oracle.txt"
# header: sr=44100 ch=1 ... chBitrate=112000

# 3. Config-only extraction
python3 tools/enc-oracle/extract_testdata.py --cfg-only \
  "$BLD/oracle" "$BLD/cfg" onset-mono-44100
```

Verification (scratch output, no repo writes):

| Regenerated | SHA-256 | Matches committed |
|---|---|---|
| `cfg_44100_112000.txt` | `ba2a2ee8…0745` | **identical** (`diff` empty) |
| `cfg_44100_96000.txt` | `11c8de1b…` | identical |
| `cfg_48000_112000.txt` | `8c2ef084…` | identical |
| `cfg_48000_96000.txt` | `27d895f0…` | identical |

### Full-extraction regression (tool refactor safety)

Running the unchanged full mode over all 15 oracle dumps produced **149/160 files byte-identical** to the committed testdata. The 11 differences are `psy_*` files whose **only** differing line is `GSPEC`, carrying the known non-deterministic pointer/uninitialized-value entries in inactive spectrum regions (same class as the README's documented SPEC wobble). No committed vector was overwritten; full re-extraction is **not** a byte-reproducibility gate, and the committed vectors must not be regenerated casually. The cfg-only path is fully deterministic.

## 5. CI / test prerequisite matrix

No CI pipeline exists in the repository (`.github/`, `.gitlab-ci.yml`, `azure-pipelines.yml`, `Makefile` absent); none was added. All rows below were executed in this environment.

| Test group | Required env/assets | Preparation / source | Verified | External dependency / licensing |
|---|---|---|---|---|
| API, lifecycle, doctests, migration equality | none | `cargo test --features aac_enc …` | **Yes** (11 + 2 + 82 + 20 streams) | none |
| `m52_all_fixtures_write_complete`, `m5b_dump_inputs` | none (synthetic fixtures generated in-test) | same | **Yes** | none |
| `m5a_decode_check` | `AAC_DEC_BIN` | Build reference decoder from repo source: `cmake -S . -B <dir> -DBUILD_PROGRAMS=ON -DCMAKE_BUILD_TYPE=Release && cmake --build <dir> --target aac-dec` | **Yes** (`/tmp/m5a-build/aac-dec`) | External C++ build of repository source; no licensed content |
| `m55_streams_match_c_reference` | `M55_C_DIR` + the four m7/m8 WAVs | Generate C refs with `aac-enc <wav> <stem>.aac` (M5.d procedure) | **Yes** (`$TMPDIR/m5d-c55`) | Licensed fixtures (CC0 / CC BY / PD); C refs derived from repo encoder |
| `m5g_branch_census` | four m7/m8 WAVs (19-stream inventory) | none beyond assets | **Yes** (2442 rows) | Same licensed fixtures |
| phase1 (8 tests) | `PHASE1_ORACLE_DIR` | `make_fixtures.py` + `enc_oracle` dumps; mono input via `m5b_dump_inputs` | **Yes** (16 dumps in scratch) | Oracle binary built from repo source + `libfdk-aac` build |
| `phase1_full_stream_parity` | `PHASE1_C_DIR` (+ optional `PHASE1_OUT`) | `aac-enc` C references for all 15 stems | **Yes** (15 refs) | Same reference build |
| `m511_profile_loop` | `M511_PROFILE` (opt-in), `M511_FIXTURE`, `M511_ITERS` | Returns immediately unless opted in | Not run (by design) | none |
| `m58` timing | `M58_MEASURE_TIMING` (optional diagnostic) | — | Not run (optional) | none |
| Config regeneration | oracle dump + mono input WAV | §4 | **Yes** (4/4 hashes) | Reference build as above |
| Full suite | union of the above | — | **Yes**: 467 passed, 1 known failure (D-083) | — |

Repository-provided: fixtures generators, oracle sources, expected vectors, decoder/encoder C sources.
Externally built (from repository source, no third-party licensing): `aac-dec`, `aac-enc`, `enc_oracle`, `libfdk-aac`.
Licensed test data: the m7/m8 WAVs (see §3) — redistributable, but they must be tracked.

## 6. Regression results

| Command (from `rust/`) | Result |
|---|---|
| `cargo build --features aac_enc` / `cargo build` / `cargo build --no-default-features --features aac_enc` | all OK (0 new warnings in touched areas) |
| `cargo test --features aac_enc encoder::` / `psy_config::` | 11 / 2 passed |
| `cargo test --features aac_enc --doc` | 82 passed |
| `cargo test --features aac_enc m6_api_matches_driver_bytes` | 1 passed, 20/20 api-identical |
| `m52_all_fixtures_write_complete`, `m5b_dump_inputs` | 1 / 1 passed |
| `AAC_DEC_BIN=… m5a_decode_check` | 1 passed |
| `M55_C_DIR=… m55_streams_match_c_reference` | 1 passed |
| `m5g_branch_census` | 1 passed |
| `frame::` / `quant::` / `rate::` | 8 / 9 / 20 passed |
| `PHASE1_ORACLE_DIR=… PHASE1_C_DIR=… phase1` | **19 passed** |
| Full suite (all env vars) | **467 passed, 1 failed** — `tp_dec::pce::tests::read` (D-083, pre-existing) |
| `cargo package --allow-dirty -p aac` | OK; `.crate` contains 4 config snapshots; unpacked `cargo check --features aac_enc` OK |
| Out-of-crate consumer (`rustc --extern aac=…`) | compiles, runs, decodes (exit 0, empty stderr) |

## 7. Compatibility and historical baselines (unchanged)

- **ADTS:** `a9d7b395…` (click-mono), `702c8717…` (onset-stereo), `aa906b52…` (sine-441k), `c3b8b5fd…` (onset-mono-441k).
- **Decoded WAV:** `6a359695…`, `4d729444…`, `aecb7b97…`, `b8b33083…`; decoder exit 0 + empty stderr.
- **M5.b metrics:** historical three cases identical to the recorded JSON; only the additive M5.e key is new.
- **Census:** 19 streams, **2,442 rows**, max iter 0, minimum `avail − sum` = 984 bits, fill max 7 bits, 287 nonzero-fill frames, zero crash, zero `npass > 1`.
- **D-083:** `tp_dec::pce::tests::read` is the only failure; it reproduces without `aac_enc` and is unrelated to this milestone.

## 8. Remaining release blockers and human decisions

1. **Track seven files** (one production, six test-only): `cfg_44100_112000.txt`, `m55-PROVENANCE.txt`, three `m7-*.wav`, `m8-PROVENANCE.txt`, `m8-music-eroica-stereo-48k.wav`. Until then, a clean checkout cannot build `aac_enc` or compile the test suite. No staging/commit was performed.
2. **Reconcile the m8 provenance header** ("PROVISIONAL … not approved") with the cited independent-review PASS before release. Licensing (public domain) is settled; the approval wording is a corpus-owner decision.
3. **Do not regenerate committed per-frame vectors** from a fresh oracle run: 11 `psy_*` files carry non-deterministic `GSPEC` fields. Config-only regeneration is the supported path.
4. **CI provisioning (if adopted):** `AAC_DEC_BIN`, `M55_C_DIR`, `PHASE1_ORACLE_DIR`, `PHASE1_C_DIR` per §5; no CI configuration exists today, so none was added.
5. **D-083** remains the known decoder test failure.

## 9. Final state and non-interference accounting

| Item | Value |
|---|---|
| HEAD | `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (unchanged) |
| Branch | `main` |
| Staged | empty |
| Tracked diff | 13 files, 1667 insertions(+), 412 deletions(-) |
| `git status --short` | 47 pre-existing lines + 2 M6.4 tool files + this report = 50 |

The tracked-diff growth (11 → 13 files, +64 insertions, +20 deletions) is exactly the two M6.4 oracle-infrastructure files; every pre-existing status line is unchanged (verified against `/tmp/m64-status-initial.txt`). The pre-existing untracked `tools/enc-oracle/__pycache__/` directory was rewritten by Python bytecode validation; it remains untracked and was already in the baseline.

Files changed by M6.4:

1. `tools/enc-oracle/extract_testdata.py` — `--cfg-only`, additive cfg stems, verbatim helper extraction.
2. `tools/enc-oracle/README.md` — config-only regeneration procedure (no machine-specific paths added).
3. `docs/aac-enc-m6-clean-checkout-reproducibility.md` — this report.

No Rust source, hash, baseline, fixture, oracle output, or expected result was modified. Generated configs/dumps live only under `$TMPDIR/opencode/m64-*`; package artifacts only under `rust/target/package/`. Nothing was staged, committed, stashed, reset, restored, cleaned, rebased, or pushed.