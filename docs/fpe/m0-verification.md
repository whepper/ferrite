# M0 — Source and Reference Verification (Evidence Record)

Date: 2026-11-16. Milestone: **M0 (source-audit/verification, not
implementation)** per `09-implementation-roadmap.md`. This record provides
reproducible evidence for the source/reference claims. **No FPE kernel was
implemented; no production source, manifest, dependency, licence file, SPDX
identifier, NOTICE, or Git history was changed.**

Verification host: `aarch64-apple-darwin` (Apple Silicon), rustc 1.99.1
(`58cf8f9c3 2026-10-13`), cargo 1.99.1 (`5f94df478 2026-08-27`).

Ferrite working tree at verification: HEAD
`b188415e3c16dbf00706984858c6a568c9b705ff`, branch `main`, only `docs/fpe/`
untracked. (The planning audit `01` was recorded at the earlier HEAD `a2c1d53`;
the repo advanced since. A-2 Ferrite anchors below were re-verified against the
**current** tree `b188415`.)

Temporary artifacts (clearly identified, **not** production changes), all under
`/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/fpe-m0/`:
- `ref-src/` — FDK-AAC 3.0.0 C source re-provisioned from Ferrite history
  (`git archive 3878c45a cpp/`), used for A-2 source verification.
- `bundle-check.git/` — throwaway bare repo used only to test the backup bundle
  for the pinned commit.
- `baseline-test.log`, `full-lib-test.log`, `q31-bench.log` — captured outputs.

---

## M0.1 — Reference provisioning and tree comparison

### Commands and full hashes (reproducible)

```sh
git rev-parse HEAD
# b188415e3c16dbf00706984858c6a568c9b705ff
git rev-parse 3878c45a
# 3878c45a8b9d85956bf1b62b17897f94c41b2d6a
git rev-parse 4c69425~1
# 3878c45a8b9d85956bf1b62b17897f94c41b2d6a   (3878c45a == 4c69425~1, confirmed)
git rev-parse 3878c45a:cpp
# 97225710da1148f4b8d98512d1e04ea98bade2f3
git rev-parse 3878c45a:cpp/libAACenc/src
# c4a10a0cd10db1e9201a295b3942d527aaaba74d
git cat-file -t f14ad40a574a4132df947d15e9fb14bee91cc8a5
# fatal: git cat-file: could not get object info   (NOT in Ferrite object store)
```

### `cpp/` tree stability across Ferrite history (verified)

| Commit | `cpp/` tree hash | Note |
|---|---|---|
| `99356b5` | `f28dc242b06ca1958dbc2768c95c31a5159a986f` | "FDK-AAC v3.0.0 (untouched)" import |
| `8ccbe32` | `74327ebb4d0041f37c931ca79e8c8c8eb6ff7ebf` | "Strip encoder to AAC-LC VBR-5…" |
| `8b7fcaa` | `97225710da1148f4b8d98512d1e04ea98bade2f3` | "Strip MpegTPEnc to ADTS+RAW+ASC…" |
| `1785529` | `97225710da1148f4b8d98512d1e04ea98bade2f3` | stable |
| `e4258d2` | `97225710da1148f4b8d98512d1e04ea98bade2f3` | stable |
| `3878c45a` | `97225710da1148f4b8d98512d1e04ea98bade2f3` | stable (= `4c69425~1`) |
| `da00af1c` | `97225710da1148f4b8d98512d1e04ea98bade2f3` | stable; **parent of pinned reference** |

`git merge-base --is-ancestor e4258d2 3878c45a` → true;
`git merge-base --is-ancestor da00af1c 3878c45a` → true.
The stripped AAC-LC `cpp/` tree `97225710…` is byte-stable from `8b7fcaa`
through `da00af1c`.

### Pinned reference commit availability (exhaustive search)

The pinned reference commit is `f14ad40a574a4132df947d15e9fb14bee91cc8a5`
(parent `da00af1ca9624a213fb53c9ac52f6fec55e9b2af`), per
`docs/reference/fdk-aac-reference-revision.txt` (private reference repo, **no
remote published**).

| Source checked | Result |
|---|---|
| Ferrite object store | `git cat-file -t f14ad40a…` → **not found** |
| Temp reference checkout `…/T/opencode-fdk-ref/fdk-aac-reference/.git/objects` | **0 object files** (working tree and object store both emptied by temp cleanup); `.git/HEAD` absent |
| Backup bundle `~/backups/fdk-aac-m5.23-worktree-20261016T193606Z/metadata/fdk-aac-all-refs.bundle` (dated **Oct 16**) | unbundled into `bundle-check.git`; `cat-file -t f14ad40a…` and `da00af1c…` → **not found** (bundle heads `e4258d2`/`8b7fcaa`/`1785529`, tag `v3.0.0`=`051b330`; predates `da00af1c` Oct 24) |
| Disk search (`mdfind` object filename / `fdk-aac-reference`) | only the emptied temp checkout; no other copy |

**Conclusion:** the pinned commit object `f14ad40a` is **not available locally**
and cannot be fetched (private repo, no remote). The FDK-AAC 3.0.0 C source was
re-provisioned from the authoritative Ferrite-history copy:

```sh
git archive 3878c45a cpp/ | tar -x -C <temp>/fpe-m0/ref-src
# ref-src/cpp/libAACenc/src/psy_configuration.h present
```

This re-provisioned tree is the **stripped AAC-LC** FDK-AAC 3.0.0 `cpp/` tree
`97225710…` (post `8ccbe32`/`8b7fcaa`), identical to `da00af1c:cpp`. It is a
**proxy** for the pinned reference, not the pinned commit object itself (see
M0.3).

---

## M0.2 — A-2 resolved: `psy_configuration.h` anchors and verified values

Verified against the re-provisioned reference source
(`3878c45a:cpp`, tree `97225710…`) and the current Ferrite tree (`b188415`).

| Constant | Reference file:line | Reference value | Ferrite location | Ferrite value | Match |
|---|---|---|---|---|---|
| `THR_SHIFTBITS` | `cpp/libAACenc/src/psy_configuration.h:113` | `4` | `rust/src/aac_enc/thresholds.rs:115` | `4` | ✓ |
| `PCM_QUANT_THR_SCALE` | `psy_configuration.h:114` | `16` | `thresholds.rs:117` | `16` | ✓ |
| `C_RATIO` | `psy_configuration.h:117-118` | `(FIXP_DBL)0x02940a10` | `thresholds.rs:119` | `0x02940a10` | ✓ |
| `PCM_QUANT_NOISE` | `psy_configuration.cpp:329` | `(FIXP_DBL)0x00547062` | `sfb.rs:168` | `0x00547062` | ✓ |
| `maxAllowedIncreaseFactor` | `psy_configuration.cpp:591` | `2` | `thresholds.rs:124` | `2` | ✓ |
| `minRemainingThresholdFactor` | `psy_configuration.cpp:592` | `(FIXP_SGL)0x0148` | `thresholds.rs:122` | `0x0148` | ✓ |
| `clipEnergy` | `psy_configuration.cpp:595-596` | `(FIXP_DBL)0x773593ff` | `sfb.rs:165` | `0x773593ff` | ✓ |
| `TRANS_FAC` | `psy_const.h:109` | `8` | `block_switch.rs` `SWITCH_WINDOWS` | `8` | ✓ |
| `NORM_PCM` | `psy_const.h:159` | `PCM_LEVEL/32768.0f` | `transform.rs:218,251` | `/32768.0` | ✓ |
| `LFE_LOWPASS_LINE` | `psy_const.h:165` | `12` | `sfb.rs:175` | `12` | ✓ |

Reference comment confirms the `C_RATIO` derivation used in `05` §3/§6.3:
`psy_configuration.h:118` — `FL2FXCONST_DBL(0.001258925f) << THR_SHIFTBITS;
pow(10.0f, -(29.0f/10.0f))`, i.e. `C_RATIO = 10^(−29/10) << 4`
(`0x02940a10/2^31 ≈ 0.020143 = 16·10^(−29/10)`). `clipEnergy` comment
(`psy_configuration.cpp:596`): `FL2FXCONST_DBL(1.0e9*NORM_PCM_ENERGY)`.
`PCM_QUANT_NOISE` comment (`:327`): `FDKpow(10,-20/10)*ABS_LOW*NORM_PCM_ENERGY*
FDKpow(2,PCM_QUANT_THR_SCALE)`.

Ferrite's own source comments cite these exact reference anchors
(`thresholds.rs:114,116,118,120-121,123`), now independently confirmed against
the actual reference source. **No differences found. A-2 RESOLVED.**

---

## M0.3 — A-3 equivalence conclusion (verified parent tree; residual gap)

**Claim under test** (`01` §9 A-3, `03` §6): pinned reference `f14ad40a:cpp/`
≡ `3878c45a:cpp/`.

**Directly verified (reproducible):**
- The pinned reference's **parent** commit `da00af1c…` (per the pin file) **is a
  Ferrite commit** ("docs: improve Ferrite repository navigation",
  2026-10-24 23:43:30 +0200), an ancestor of `3878c45a`.
- `git rev-parse da00af1c:cpp` = `97225710da1148f4b8d98512d1e04ea98bade2f3`
  = `3878c45a:cpp` (**identical tree hash**).
- `git rev-parse da00af1c:cpp/libAACenc/src` =
  `c4a10a0cd10db1e9201a295b3942d527aaaba74d` = `3878c45a:cpp/libAACenc/src`
  (**identical**).
- The `cpp/` tree `97225710…` is stable across `8b7fcaa…da00af1c` (M0.1 table).

**Residual gap (cannot be closed locally):**
- The pinned commit object `f14ad40a` itself is **unavailable** (M0.1 search).
  Its subject ("Reference baseline: legacy FDK AAC implementation and oracle
  tooling") indicates it adds oracle tooling atop `da00af1c`; the pin file
  scopes it to the legacy `cpp/` tree + root helpers + `tools/enc-oracle` +
  `tools/c-enc-bench` + root `NOTICE`. Because the object is absent, it cannot
  be **directly** confirmed that `f14ad40a` left `cpp/` byte-identical to its
  parent `da00af1c`.

**Conclusion:** A-3 is **substantially verified but NOT fully resolved**. The
`cpp/` tree equivalence is directly established between the reference's
available parent (`da00af1c`) and `3878c45a` (identical content-addressed tree
hashes), and the tree is provably stable across the relevant history. The
equivalence is therefore strongly supported; the only unverified step is that
the final reference-baseline commit `f14ad40a` did not modify `cpp/`. **A-3 is
not marked resolved.** Implication: C↔FPE Tier-B ground truth uses
`3878c45a:cpp` (≡ `da00af1c:cpp`) as the reference proxy; before final M8
reliance, confirm `f14ad40a:cpp` unchanged by re-provisioning the pinned commit
from the private reference owner (risk R-12). Reproducible commands above.

---

## M0.4 — Q31 baseline (no Q31 change)

### Git / toolchain / build

- HEAD `b188415e3c16dbf00706984858c6a568c9b705ff`, branch `main`; working tree
  clean except untracked `docs/fpe/` (and macOS `.DS_Store` artifacts).
- rustc 1.99.1 (`58cf8f9c3 2026-10-13`), cargo 1.99.1, host
  `aarch64-apple-darwin` (the reference platform for `production_baseline`).
- Build: `cargo build --release --locked` (release profile, default features;
  no `[features]` section exists). Build fingerprints consistent with current
  source (no recompile needed on the cached release tree).

### Corpus and fixture hashes (SHA-256)

```
b49e10a1d3f51b80db1d4d1a1179afa9963570528042395df20be8e2193f3086  m7-music-stereo-48k.wav
3dfb7dcb0cd9f600f9ae66eb791df6d7c740c833218df88f1de73f87353346ff  m7-speech-clean-stereo-48k.wav
67d69a64fed23fe16ab666a9e1f02262afa78934363c69999728cadd9ec1a4d3  m7-speech-pauses-stereo-48k.wav
4413c8a41fe148192081c504ecfb5d69b5c7c4fc158cefff8b0dfadf8923986b  m8-music-eroica-stereo-48k.wav
```
(`m7-speech-clean` SHA matches the hash cited in
`docs/aac-enc-block-switch-oracle-investigation.md`.) Committed vectors: 343
`.txt`, 47 `psy_*`, 18 `cfg_*` snapshots under `rust/src/aac_enc/testdata/`.

### Commands and test results

```sh
cd rust
cargo test --release --locked --lib production_baseline -- --nocapture
# 2 passed; 0 failed; 378 filtered out
#   production_f32_corpus_baseline_is_unchanged ... ok
#   production_default_profile_vbr6_music_baseline_is_unchanged ... ok
cargo test --release --locked --lib
# 363 passed; 11 failed; 6 ignored
```

**The 11 failures are exclusively Tier-B reference-validation tests** failing
loudly on missing C artifacts (10 × "reference validation artifact missing"
from `$PHASE1_ORACLE_DIR`/`$M55_C_DIR`; 1 × `m5a_decode_check` needing
`$AAC_DEC_BIN`). These are the CI-excluded Tier-B set; they are **not Q31
regressions** — they are blocked on reference provisioning (M0.1/M0.3, R-12).
All **363 self-contained Tier-A tests pass**, including the byte/hash-pinned
production baseline. Q31 default output is unchanged.

### Performance baseline (approved harness, Q31-only)

```sh
cd rust
AAC_WAV_REPS=15 AAC_WAV_WARMUP=1 cargo run --release --locked --example aac_bench_wav
# WAVBENCH_DONE; all FNV-1a hashes match committed VBR-6 pins
```

Q31, VBR-6 (default), release, `aarch64-apple-darwin`, 15 reps + 1 warmup,
4 music/speech fixtures + 4× long fixture. Median per fixture (encode time,
RTF, realtime×, FNV-1a):

| Fixture | Audio | Bytes | Frames | Encode (median) | RTF | Realtime× | FNV-1a |
|---|---|---|---|---|---|---|---|
| m7-music-stereo-48k | 10 s | 256763 | 471 | 39.88 ms | 0.003990 | 250.7 | `d6fc9d2c59cf5c36` |
| m7-speech-clean-stereo-48k | 10 s | 129335 | 471 | 27.49 ms | 0.002751 | 363.5 | `7c3e409574ebc62e` |
| m7-speech-pauses-stereo-48k | 10 s | 127326 | 471 | 27.45 ms | 0.002747 | 364.1 | `6c087bfb228d3bbe` |
| m8-music-eroica-stereo-48k | 10 s | 241021 | 471 | 35.67 ms | 0.003569 | 280.2 | `dcf07392921a18b7` |
| m7-music-stereo-48k-x4 | 40 s | 1021456 | 1877 | 159.30 ms | 0.003983 | 251.1 | `7f3a621a091eee2f` |

### Environmental limitations / non-reproducible aspects

- This is a **Q31-only preliminary baseline** (M0/M1 capture per `08` §6), not
  the paired Q31↔FPE confirmatory benchmark (that is M9, requires FPE). The
  fixed-sample paired design, pilot variability, serial-dependence assessment,
  and CI of `08` §3 apply to the **paired** comparison; they are not meaningful
  for a single-arm capture and were not run here.
- Single machine, no controlled thermal chamber; run-to-run spread observed
  (e.g. one music-x4 rep ~2× slower, a thermal/background outlier). The
  harness median is reported; the predeclared outlier policy (`08` §3.4) governs
  the later confirmatory run.
- Tier-B reference tests and the C↔Q31 oracle comparison are **blocked** until
  the reference is re-provisioned (M0.1/M0.3).

---

## M0.5 — A-2…A-7 disposition

| ID | Item | Disposition | Justification / named milestone |
|---|---|---|---|
| A-2 | `psy_configuration.h` anchors + values | **Resolved by M0** | M0.2: anchors/values verified against reference source and Ferrite; no differences |
| A-3 | Pinned-reference `cpp/` equivalence | **Not fully resolved — residual gap documented; confirmation deferred** | M0.3: parent `da00af1c:cpp` ≡ `3878c45a:cpp` directly verified (identical tree hashes); `f14ad40a` object unavailable locally. Confirm `f14ad40a:cpp` unchanged via private-reference re-provisioning **before M8** Tier-B ground-truth reliance (R-12). Not marked resolved |
| A-4 | f32-vs-f64 energy-sum error envelope | **Deferred to M3** | Named in `09` FPE-3.3; measured experiment, not needed to close M0 |
| A-5 | x86-64 performance / denormal behaviour | **Deferred to M9** | Named in `08`/`09`; ARM64 is the Stage-1 reference target; x86 supplementary for the cross-platform claim |
| A-6 | C `mdctSpectrum_e` derivation (why 12/9) | **Not required for Stage 1** | Ferrite uses measured constants `MDCT_SCALE_LONG=12`/`MDCT_SCALE_SHORT=9` (`psy.rs:129-131`), validated over 1000+ oracle frames; FPE reuses them unchanged. No re-derivation needed |
| A-7 | `fixp_atan`/`fPow` internals | **Not required for Stage 1** | Spreading factors/minSnr are consumed as committed tables (`sfb_tables.rs`, `cfg_*.txt`); float re-derivation is explicitly Stage 2 (`03` §4.3, `00` §7). Not needed for the faithful translation |

(A-1, global-mutable-state census, was resolved during the planning audit:
zero production globals, `01` §4.)

---

## Scope integrity (M0)

- **No FPE kernel implemented; M1 not started.**
- **No production Rust source modified**; no Cargo manifest/lockfile/dependency
  change; no licence file/SPDX/NOTICE change; FDK-AAC licence decision
  preserved; Q31 engine and its default selection preserved.
- **No Git history operation** (no commit/amend/rebase/reset/cherry-pick/push).
  All git commands used were read-only (`rev-parse`, `cat-file`, `ls-tree`,
  `log`, `merge-base --is-ancestor`, `archive`).
- Temporary reference checkout (`ref-src/`), throwaway bundle-check repo, and
  test/bench logs are confined to the approved temp dir and are **not**
  production changes. The unrelated working-tree state was not modified.
- Documentation/verification artifacts added only under `docs/fpe/`.

**M0 status:** source claims (A-2) supported by reproducible evidence;
reference equivalence (A-3) verified at the available parent commit with a
clearly documented residual gap (pinned object unavailable). Q31 baseline
captured and green (Tier-A). M0 passes for the verifiable claims; A-3's residual
step is explicitly carried forward (not prematurely resolved).