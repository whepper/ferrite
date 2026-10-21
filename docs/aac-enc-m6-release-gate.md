# M6.5 — Repository Integration and Release Gate

Repository integration and validation milestone. No encoder, psychoacoustic, bitstream, or baseline changes.

**Verdict: `PASS WITH FINDINGS`**

The technical release gate passes: builds, packaging, API/migration tests, regression suites, oracle regeneration, and every compatibility baseline reproduce exactly. At M6.5 authoring time, two repository-level actions remained outside that milestone's allowed operations: **the M6 source line and seven assets were still untracked**, and **the m8 corpus approval contradiction was unresolved by authoritative evidence**. The subsequent C1–C8 consolidation tracked the source and assets; m8 remains provisional. This report records the M6.5 snapshot, and no post-consolidation clean-checkout build has been rerun.

## 1. Initial state

| Item | Value |
|---|---|
| HEAD | `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` |
| Branch | `main` |
| `git status --short` | 50 lines |
| Staged | empty |
| Tracked diff | 13 files, 1667 insertions(+), 412 deletions(-) |

At M6.5 time, the untracked set was broader than the seven assets: it also contained the M6 production modules (`encoder.rs`, `psy_config.rs`, `m6_migration.rs`, `stream_driver.rs`, `m5a_decode.rs`, `m5b_measure.rs`, `m5g_census.rs`, `phase1b.rs`) and all M5/M6 reports. This mattered for §7: **that snapshot's Git state could not build the encoder at all**, not merely the config snapshot. The later C1–C8 consolidation tracked these files; this report does not claim a post-consolidation clean-checkout build.

## 2. Seven-file manifest

All seven were verified in place; none was modified, regenerated, moved, staged, or committed.

| File | Purpose | Dependency / verification | SHA-256 | Licence / redistribution | Safe to track |
|---|---|---|---|---|---|
| `rust/src/aac_enc/testdata/cfg_44100_112000.txt` | Init-time PSY/TNS snapshot for mono/44.1 kHz | `include_str!` at `psy_config.rs:22`; `Encoder::new(44100, 1, …)`; 4281 B / 18 lines; hash matches M5.e `ba2a2ee8…0745` and fresh regeneration | `ba2a2ee838b4733c0c4e05b451150e6d0481bf8dd1de992ccd2ebe8215ef0745` | Generated numeric data; no third-party content | **Yes** (production build blocker) |
| `rust/src/aac_enc/testdata/m55-PROVENANCE.txt` | Attribution + conversion record for the m7 assets | Source of CC0/CC BY evidence; 4344 B | `470eca4d52fa69c5d26c9efe6e56e1ea1d2d44c725ccdbea7433002d1621c7b0` | Metadata; contains the required CC BY attribution | **Yes** |
| `rust/src/aac_enc/testdata/m7-music-stereo-48k.wav` | Licensed music fixture | `testdata.rs:442`; `M55_FIXTURES` (`stream_driver.rs:4079`); `m55`/`m5g`; 1 920 078 B | `b49e10a1d3f51b80db1d4d1a1179afa9963570528042395df20be8e2193f3086` | Wikimedia "Thoughtness" (Kian Moretz), **CC0 1.0** | **Yes** |
| `rust/src/aac_enc/testdata/m7-speech-pauses-stereo-48k.wav` | Speech-with-pauses fixture | `testdata.rs:443-445`; `M55_FIXTURES`; `m55`/`m5g`; 1 920 078 B | `67d69a64fed23fe16ab666a9e1f02262afa78934363c69999728cadd9ec1a4d3` | LibriSpeech test-clean, **CC BY 4.0** (attribution in `m55-PROVENANCE.txt`) | **Yes, with attribution** |
| `rust/src/aac_enc/testdata/m7-speech-clean-stereo-48k.wav` | Continuous-speech fixture | `testdata.rs:446-448`; `M55_FIXTURES`; `m55`/`m5g`; 1 920 078 B | `3dfb7dcb0cd9f600f9ae66eb791df6d7c740c833218df88f1de73f87353346ff` | LibriSpeech test-clean, **CC BY 4.0** (attribution in `m55-PROVENANCE.txt`) | **Yes, with attribution** |
| `rust/src/aac_enc/testdata/m8-PROVENANCE.txt` | Provenance + licence + limitations for the V3 fixture | 9740 B; cited by `testdata.rs:449-453` | `a24e0786dc919da101a30a2b0028f6a7ffdcd3055cc23f8a6b58135102d131cd` | Metadata; PD evidence and limitations | **Yes** (content unchanged; see §3) |
| `rust/src/aac_enc/testdata/m8-music-eroica-stereo-48k.wav` | V3 real-music baseline fixture | `testdata.rs:453-455`; `M55_FIXTURES` (`:4094`); `m55`/`m5g`; 1 920 044 B | `4413c8a41fe148192081c504ecfb5d69b5c7c4fc158cefff8b0dfadf8923986b` | Beethoven/Musopen, **public domain** (Musopen PD release; CC PD Mark 1.0) | **Yes for licensing**; corpus acceptance pending §3 |

Evidence notes:

- Every WAV hash matches its provenance file (`m55-PROVENANCE.txt` lines 24-26, 59-63; `m8-PROVENANCE.txt` lines 9-18).
- The four WAVs are embedded with `include_bytes!` in the `#[cfg(test)]` fixture table and are required by `m55_streams_match_c_reference`, `m5g_branch_census`, and the full-suite build. They do **not** affect production builds.
- The config snapshot **does** affect production: `include_str!` at compile time. Removing it fails with `couldn't read … cfg_44100_112000.txt` (exit 101).
- The two provenance files are documentation, but `m55-PROVENANCE.txt` carries the CC BY 4.0 attribution and must ship with the speech fixtures.

### Exact user action (not run)

```sh
git add \
  rust/src/aac_enc/testdata/cfg_44100_112000.txt \
  rust/src/aac_enc/testdata/m55-PROVENANCE.txt \
  rust/src/aac_enc/testdata/m7-music-stereo-48k.wav \
  rust/src/aac_enc/testdata/m7-speech-clean-stereo-48k.wav \
  rust/src/aac_enc/testdata/m7-speech-pauses-stereo-48k.wav \
  rust/src/aac_enc/testdata/m8-PROVENANCE.txt \
  rust/src/aac_enc/testdata/m8-music-eroica-stereo-48k.wav
```

The M6 source modules and reports also need to be added by the user for a buildable commit; the seven-file command alone does not make the tracked state buildable (§7).

## 3. m8 provenance reconciliation

What is provisional, exactly:

- `m8-PROVENANCE.txt:4-5`: "Status: **PROVISIONAL**. The corpus is not final, and this provenance package is not approved, until it has passed independent review."
- `m8-PROVENANCE.txt:53-54`: `Qualification: V3-MUSIC-FIXTURE-QUALIFICATION.md (independent review: PASS)`, report SHA-256 `22010c83…`.
- `docs/aac-enc-benchmark-plan.md:42-49`: the cited report lived at `…/T/v3-music-20261017/V3-MUSIC-FIXTURE-QUALIFICATION.md` (SHA-256 `22010c83…`), and "The corpus stays **provisional** until the updated provenance package passes independent review."

What the evidence shows:

- **Licensing is resolved:** public domain (Musopen PD release / CC PD Mark 1.0; attribution not required). Redistribution is safe.
- **Corpus approval is not resolved:** the cited qualification report is **not present** in the repository and **not present** in the surviving scratch tree (`$TMPDIR/opencode/v3-music-20261017/` contains only analysis inputs/outputs and a helper script; no report). Its recorded hash cannot be recomputed.
- **No corpus-owner decision exists in the repository.** The only explicit status statements are the provenance header (provisional) and the benchmark plan (provisional until review), which conflict with the unverifiable "PASS" citation.
- A technical PASS, even if genuine, would not by itself establish corpus-owner acceptance.

**Decision: leave `m8-PROVENANCE.txt` unchanged.** The smallest correction, if the owner confirms acceptance, is documentation-only: replace the PROVISIONAL header with the approval status and record the review artifact's canonical path and SHA-256 in the provenance file (and update the stale benchmark-plan sentence). No audio, vector, or provenance history was modified.

**Exact owner question:** “Has the V3 music corpus and its provenance package been formally accepted as final, and is the independent review referenced as `V3-MUSIC-FIXTURE-QUALIFICATION.md` (SHA-256 `22010c83…`) complete? If yes, where is the authoritative review artifact (the cited path is a scratch path and the file is missing), and may the PROVISIONAL header be replaced?” Until answered, m8 remains a release finding, not an approved corpus member.

## 4. Oracle tooling verification

Reviewed `tools/enc-oracle/extract_testdata.py` and `README.md` (M6.4 changes). `--cfg-only` skips per-frame vectors; trailing positional arguments add config stems; the frame-extraction code is unchanged (the config loop was moved verbatim into `extract_configs()`).

Fresh run from a new scratch directory:

```sh
S="$TMPDIR/opencode/m65-oracle"
"$BLD/enc_oracle" "$TMPDIR/m5b-quality/inputs/onset-mono-44100.orig.wav" \
  "$S/oracle/onset-mono-44100.oracle.txt"
# header: sr=44100 ch=1 ... chBitrate=112000
python3 tools/enc-oracle/extract_testdata.py --cfg-only "$S/oracle" "$S/cfg4" \
  onset-mono-44100
```

| Output | SHA-256 | Matches committed |
|---|---|---|
| `cfg_44100_112000.txt` | `ba2a2ee8…0745` | **byte-identical** |
| `cfg_44100_96000.txt` | `11c8de1b…` | byte-identical |
| `cfg_48000_112000.txt` | `8c2ef084…` | byte-identical |
| `cfg_48000_96000.txt` | `27d895f0…` | byte-identical |

Full-extraction regression (same 15 dumps, fresh output directory): **149/160 files byte-identical**; the 11 differences are `psy_*` files whose **only** differing line is `GSPEC` (known pointer/inactive-region SPEC wobble). No committed vector was overwritten or regenerated; the artifact count and tags were re-verified (`differing-tags ['GSPEC']`). At M6.5 time, the repository `testdata/` status remained the same seven untracked assets after the runs.

## 5. Release-gate matrix

All commands from `rust/` unless noted; all executed in this checkout.

| # | Check | Command | Result |
|---|---|---|---|
| 1 | Production build, encoder feature | `cargo build --features aac_enc` | **Pass** |
| 1 | Default-feature build | `cargo build` | **Pass** |
| 1 | Encoder-only build | `cargo build --no-default-features --features aac_enc` | **Pass** |
| 2 | Package + unpacked build | `cargo package --allow-dirty -p aac`; unpack; `cargo check --features aac_enc` | **Pass** (4 cfg + 4 WAVs in `.crate`) |
| 3 | Out-of-crate consumer | `rustc --extern aac=…libaac.rlib`; run; `aac-dec` | **Pass** (exit 0, empty stderr) |
| 4 | Encoder API tests | `cargo test --features aac_enc encoder::` | **11 passed** |
| 4 | Config tests | `cargo test --features aac_enc psy_config::` | **2 passed** |
| 4 | Doctests | `cargo test --features aac_enc --doc` | **82 passed** (incl. `Encoder` example) |
| 5 | Migration equality | `cargo test --features aac_enc m6_api_matches_driver_bytes` | **1 passed**, 20/20 api-identical |
| 6 | `m52` | `cargo test --features aac_enc m52_all_fixtures_write_complete` | 1 passed |
| 6 | `m55` | `M55_C_DIR=$TMPDIR/m5d-c55 cargo test … m55_streams_match_c_reference` | 1 passed |
| 6 | `m5a` | `AAC_DEC_BIN=/tmp/m5a-build/aac-dec cargo test … m5a_decode_check` | 1 passed |
| 6 | `m5b_dump` | `cargo test --features aac_enc m5b_dump_inputs` | 1 passed |
| 6 | `m5g` | `cargo test --features aac_enc m5g_branch_census` | 1 passed |
| 7 | Frame / quant / rate | `cargo test --features aac_enc frame::` / `quant::` / `rate::` | 8 / 9 / 20 passed |
| 8 | `phase1` (19) | `PHASE1_ORACLE_DIR=… PHASE1_C_DIR=… cargo test … phase1` | **19 passed** |
| 9 | ADTS hashes | see §6 | **4/4 unchanged** |
| 10 | Decoded WAV hashes | see §6 | **4/4 unchanged** |
| 11 | M5.b metrics | `/tmp/m5b-metrics.py` vs recorded JSON | **historical 3 identical** |
| 12 | Census | `$TMPDIR/m5g-census/census.json` | **2 442 rows**, invariants hold |
| — | Full suite | all env vars | **467 passed, 1 failed** (D-083) |

Failure classification: `tp_dec::pce::tests::read` is the only failure; it is the documented pre-existing D-083 decoder test and reproduces without `aac_enc`. No gate was skipped or modified.

## 6. Compatibility and historical baselines (unchanged)

- **ADTS:** `a9d7b395…` (click-mono), `702c8717…` (onset-stereo), `aa906b52…` (sine-441k), `c3b8b5fd…` (onset-mono-441k).
- **Decoded WAV:** `6a359695…`, `4d729444…`, `aecb7b97…`, `b8b33083…`; decoder exit 0 + empty stderr.
- **M5.b metrics:** identical for `click-mono`, `onset-stereo`, `sine-stereo-44100`; only the additive M5.e key is new.
- **Census:** 19 streams, 2 442 rows, max iter 0, minimum `avail − sum` = 984 bits, fill max 7 bits, 287 nonzero-fill frames, zero crash, zero `npass > 1`.

## 7. Clean-checkout verification

Method: scratch copies only; the user's worktree was not cleaned, reset, or checked out. Because the whole M6 line was uncommitted at M6.5 time, “present Git state” and “post-integration state” were distinct.

| Scenario | Construction | Command | Observed |
|---|---|---|---|
| **A. Present Git state (M6.5)** | `git ls-files` export of the then-current worktree content (1 101 files; tracked files only) | `cargo check --features aac_enc` | **Blocked** — `error[E0583]: file not found for module encoder` and `psy_config`; exit 101. The tracked state at M6.5 lacked the M6 source modules. |
| **B. Post-integration, assets supplied** | `cargo package` tarball (contains all crate files incl. 4 cfg + 4 WAVs) unpacked in scratch | `cargo check --features aac_enc`; `cargo test --no-run --features aac_enc`; `cargo test … encoder::`; `… psy_config::` | **Pass** — build OK, full test suite compiles, 11 + 2 tests pass |
| **C. Post-integration, cfg missing** | B with `cfg_44100_112000.txt` removed | `cargo check --features aac_enc` | **Blocked** — `couldn't read … cfg_44100_112000.txt`; exit 101 |
| **D. Post-integration, test WAV missing** | B with `m7-music-stereo-48k.wav` removed | `cargo test --no-run --features aac_enc` | **Blocked** — `couldn't read … m7-music-stereo-48k.wav`; exit 101 |

**Conclusion:** at M6.5 time, clean-checkout reproducibility was **not** demonstrated for the then-current Git state, because neither the M6 source nor the seven assets were tracked. It **was** demonstrated for a post-integration tree containing the tracked M6 sources and the seven assets (scenario B). The production-build dependency (C) and test-asset dependency (D) were verified separately, with explicit prerequisite errors. The later C1–C8 consolidation tracked those files, but no post-consolidation clean-checkout build has been rerun; this report does not claim one.

## 8. Release blockers and next actions

1. **Release commits (historical):** at M6.5 time this required user authorization to add the seven assets (§2) **and** the untracked M6 source modules/reports; scenario A described that then-current state. The subsequent C1–C8 consolidation has since tracked them. No staging or committing was performed as part of M6.5, and no post-consolidation clean-checkout build is recorded here.
2. **m8 owner decision (§3):** confirm corpus acceptance and the authoritative review artifact, or keep m8 provisional and treat it as a release finding. Licensing alone does not resolve approval.
3. **Do not regenerate committed per-frame vectors:** full extraction is not byte-identical (11 `GSPEC`-only diffs). Use `--cfg-only` for snapshots.
4. **D-083:** known pre-existing decoder test failure; out of scope.
5. **Optional CI:** no pipeline exists; provisioning for `AAC_DEC_BIN`, `M55_C_DIR`, `PHASE1_ORACLE_DIR`, `PHASE1_C_DIR` is verified but not codified.

## 9. Final state and non-interference accounting

| Item | Value |
|---|---|
| HEAD | `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (unchanged) |
| Branch | `main` |
| Staged | empty |
| Tracked diff | 13 files, 1667 insertions(+), 412 deletions(-) — unchanged |
| `git status --short` | 50 pre-existing lines + this report = 51 |

Files changed by M6.5: **only** `docs/aac-enc-m6-release-gate.md`. No Rust source, tooling, test asset, hash, baseline, vector, or provenance file was modified. All generated data stayed under `$TMPDIR/opencode/m65-*` and `rust/target/package/`. Nothing was staged, committed, stashed, reset, restored, cleaned, rebased, or pushed.