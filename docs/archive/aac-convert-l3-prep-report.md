# L3-prep Evidence Report — Loudness Readiness Gates

- Date: 2026-10-24 (UTC). Initial research-only version earlier the same day;
  executed validation added the same day; the two remaining gaps were closed
  in follow-up passes (`iTunNORM` real-file bytes, then official EBU vectors
  against an owner-provisioned v5.0 archive).
- Baseline: `main` @ `7ebf208d40d78adcd265e3736e93f8a8aa93e8d1`.
- Authority: `docs/aac-convert-l0-loudness-decisions.md` (L0 D1–D7),
  `docs/aac-convert-l2-loudness-integration-design.md` finalized design (§6
  vetting gate, §7 matrix, §8 tests, §9 L3 gate with 7 items).
- Scope: the six evidence gates from L2 §9 (owner approval itself excluded).
  This task authorized isolated research, temporary dependency fetching,
  builds, and validation experiments. No production source, test, manifest,
  lockfile, CI, or other documentation changes. All experiments ran outside
  the repository (`/tmp/loudprep`); the repository itself was never
  modified.
- Environment: macOS arm64; `rustc 1.99.1`, `cargo 1.99.1` (plus installed
  1.85 and 1.60 toolchains); `rsgain 3.8` (libebur128 1.2.6,
  pre-existing system install — used as a validator only, never modified);
  `ffmpeg`/`ffprobe` 9.0.2; `python3` 3.15 (stdlib only, plus the
  pre-existing system `mutagen 1.48.1` used read-only as the reference MP4
  writer — nothing installed); original `flac2aac` source read-only at
  `/Users/USER/Downloads/Python/flac2aac/` (user's own tree, never modified).
- Method per gate: `PASS` (required evidence and acceptance criteria actually
  satisfied), `FAIL` (executed evidence contradicts a requirement), or
  `NOT RUN` (a required step was not executed). A gate is never marked
  `PASS` on a plan or an unverified claim alone.

## Gate 1 — `ebur128` dependency, license, MSRV, native-toolchain vetting

**Status: PASS** (exact-1.60 library build executed; dev-dependency test
compilation out of scope as noted below).

Executed evidence (isolated `/tmp/loudprep/msrv160/ebur128check`, crate
`ebur128 =0.1.10`, vendored sources, `--locked --offline`):

- Package identity/version: `cargo metadata` resolves `ebur128 0.1.10`,
  MIT, `rust-version = "1.60"`, edition 2018; vendored `Cargo.toml`
  confirms `version = "0.1.10"`, `rust-version = "1.60"`, `license = "MIT"`;
  `LICENSE` file present (covers libebur128 Kokemüller + ebur128 Dröge
  notices).
- Transitive normal-dependency closure (`cargo tree -e normal`, executed):
  `bitflags 1.3.2` (MIT/Apache-2.0), `smallvec 1.16.4` (MIT/Apache-2.0),
  `dasp_frame 0.11 → dasp_sample 0.11` (MIT OR Apache-2.0). No copyleft; all
  within `THIRD-PARTY-NOTICES.md` practice. (`cc 1.0` is optional and absent
  from the normal graph.)
- Exact-MSRV library build (executed): genuine `rustc 1.60.0
  (7737e0b5c 2022-04-04)` toolchain installed via rustup; clean
  `cargo +1.60 build --locked --offline` succeeds, producing
  `libebur128-*.rlib` + `.rmeta`. Resolved dependency versions under 1.60:
  identical (`bitflags 1.3.2`, `smallvec 1.16.4`, `dasp 0.11.0`).
- No-C-toolchain on the default path (executed twice): full clean rebuild
  with `cc`/`gcc`/`clang` on `PATH` replaced by failing shims and the real
  linker passed by absolute path produced zero shim invocations and a
  successful build; `build.rs` additionally verified by source read to invoke
  `cc` only under `cfg(feature = "c-tests")` (default build script emits
  nothing — verified empty `output` file).
- Compatible-toolchain builds (executed): the full isolated harness builds
  clean on repo toolchain 1.99.1 and on installed 1.85.

Scope note: dev-dependencies (`criterion`, `hound`, `quickcheck`,
`ebur128-c 0.1.1`, …) were not compiled under 1.60 — upstream's test
harness is not part of the production dependency closure and needs no
vetting for L3. No manifest or lockfile in the repository was touched;
all fetching occurred in `/tmp`.

## Gate 2 — EBU Tech 3341/3342 vector validation

**Status: PASS** (official v5.0 archive obtained; unmodified upstream suite
30/30; v1-scope values recorded with margins).

Archive identity and provenance (verified by execution, original untouched):

- File: `/Users/USER/Downloads/ebu-loudness-test-setv05.zip`
  (91,631,421 bytes), SHA-256
  `9cc500b4df83f7c21855c74dce795ef5209a752bf884253ae57d0ce512efb062`.
- Content: 71 entries / ~314 MB uncompressed — 70 audio files + `readme.txt`
  identifying "EBU Loudness test set - v05 - 30 March 2016" (v05 adds the
  500–2000 Hz reference listening signal; v02+ signals match the August 2011
  Tech 3341/3342 with the −10 LU relative gate). Filenames map 1:1 to the
  signals in Tech 3341/3342 (bit depth suffixed; seq-3341-6 notes document
  the 5ch PCM vs 6ch WAVEEX variants; v01-era files suffixed `v02`/`2011`).
- Terms (`readme.txt`): "provided 'as is' with no warranty. The material may
  only be used for technical testing purposes." — covers this validation use;
  no redistribution: fixtures live only in `/tmp/loudprep/ebu/ref/` (66
  suite-referenced files extracted; ZIP never modified) and nothing was
  committed to the repository.

Execution (isolated `/tmp/loudprep/eburun`, repo toolchain `rustc/cargo
1.99.1`, resolved `ebur128 =0.1.10`, `hound 3.5.1`, `float_eq 1.0.1`):

- Ran the **unmodified** upstream `tests/reference_tests.rs`
  (byte-identical copy, `diff`-verified) via `cargo test --test
  reference_tests`: **30 passed, 0 failed** (two consecutive runs, 6.5 s
  each). Reference WAVs supplied as symlinks under `tests/reference_files/`
  (the suite's `prepare_file!` location); no suite, threshold, or fixture
  change of any kind.
- Values-dump companion (same dependency, same file-reading semantics as the
  suite macros — 24-bit as `i32 << 8` — whole-file feed, release build):

| Vector | Expected | Actual queue | Actual HISTOGRAM | Tol | Result |
|---|---|---|---|---|---|
| seq-3341-1 global | −23.0 | −22.9536 | −22.9500 | ±0.1 | PASS |
| seq-3341-2 global | −33.0 | −32.9599 | −32.9500 | ±0.1 | PASS |
| seq-3341-3-v02 global | −23.0 | −23.0141 | −22.9707 | ±0.1 | PASS |
| seq-3341-4-v02 global | −23.0 | −23.0141 | −22.9707 | ±0.1 | PASS |
| seq-3341-5-v02 global | −23.0 | −22.9790 | −22.9357 | ±0.1 | PASS |
| seq-3341-7 global | −23.0 | −22.9862 | −22.9870 | ±0.1 | PASS |
| seq-3341-2011-8 global | −23.0 | −22.9978 | −22.9986 | ±0.1 | PASS |
| seq-3342-1 LRA | 10.0 | 10.0011 | 10.0000 | ±1.0 | PASS |
| seq-3342-2 LRA | 5.0 | 4.9994 | 5.0000 | ±1.0 | PASS |
| seq-3342-3 LRA | 20.0 | 19.9951 | 20.0000 | ±1.0 | PASS |
| seq-3342-4 LRA | 15.0 | 14.9993 | 15.0000 | ±1.0 | PASS |
| seq-3341-7 LRA | 5.0 | 4.9748 | 5.0000 | ±1.0 | PASS |
| seq-3341-2011-8 LRA | 15.0 | 14.9932 | 15.0000 | ±1.0 | PASS |

Coverage and explicit scope boundaries:

- The 30/30 run covers: global ±0.1 (queue + HISTOGRAM) incl. M/S ladders and
  the seq-3341-9…14 momentary/short-term programs; LRA ±1.0 (queue +
  HISTOGRAM); 9 true-peak cases (−0.4/+0.2 dB window). Zero skips, zero
  unsupported cases — every suite test executed.
- Gate verdict rests on the v1-applicable vectors (integrated mono/stereo +
  LRA above, worst |Δ| = 0.064 LU against a ±0.1 LUFS tolerance — margin
  ≈ 36%; LRA worst |Δ| = 0.025 LU against ±1.0). Executed but
  out-of-v1-scope (v1 is mono/stereo, no true-peak path): surround
  seq-3341-6 5ch/6ch both −23.0172 (queue) / −23.0500 (HISTOGRAM) vs −23.0 —
  inside the suite's own ±0.1 but recorded as informational only; the 9
  true-peak suite tests passed but are likewise informational for v1.
- Synthetic structure-vectors from the prior passes are retained as
  supplemental only and are superseded by the official results above.

Remaining work: none for this gate. The prior Cloudflare/login blockers are
resolved by the owner-provisioned archive.

## Gate 3 — HISTOGRAM accuracy and bounded-memory behavior

**Status: PASS** (measured; tolerances are upstream's own, explicitly cited
— no threshold invented).

- Accuracy (executed): queue-vs-HISTOGRAM agreement — tone Δ = 0.044 LU,
  30-min tone Δ = 0.021 LU, LRA dual-level Δ = 0.007 LU, album Δ = 0.044 LU.
  All within the ±0.1 LUFS (global) / ±1.0 LU (LRA) tolerances upstream
  asserts for the same comparison in its own reference tests.
- Bounded memory (executed, `/usr/bin/time -l`, streaming harness holding
  one 100 ms block): 345.6 MB / 30-min stereo input → peak RSS **2.54 MB**
  (queue) / **2.33 MB** (HISTOGRAM); 3.8 MB / 20-s input → 2.34 MB.
  Duration up ~90×, RSS up ~8% — input-size-independent, O(1) analyzer
  state. Throughput ≈1200× realtime (1.5 s for 30 min).
- L2 stated no numeric HISTOGRAM tolerance of its own; the PASS uses
  upstream's asserted tolerances as the bar, recorded here explicitly.

## Gate 4 — Source-FLAC versus decoded-AAC differential experiment

**Status: PASS** (executed; one L2 wording gap documented and resolved
without inventing a number — see below).

Method (per L2 §8, unchanged): two-track 48 kHz stereo album (5 s tones at
different levels, ffmpeg-generated FLAC) converted with the unmodified repo
`aac-convert 0.1.0` binary (built `--locked --offline` from the clean tree)
to M4A; source FLACs and final M4As each decoded with ffmpeg (validator
only) to s16 PCM and analyzed with the same isolated analyzer/settings.

Results:

| Comparison | Source domain | Final domain | \|Δ\| | L2 criterion |
|---|---|---|---|---|
| Track A integrated | −27.7775 LUFS | −27.7797 LUFS | **0.0022 dB** | ≤ 0.5 dB ✓ |
| Track B integrated | −39.3707 LUFS | −39.3776 LUFS | **0.0069 dB** | ≤ 0.5 dB ✓ |
| Album integrated | −30.4967 LUFS | −30.4993 LUFS | **0.0026 dB** | ≤ 0.5 dB ✓ |
| Track A peak | 0.044189 | 0.044464 | 0.000275 lin (0.054 dB) | see note |
| Track B peak | 0.011047 | 0.011139 | 0.000092 lin (0.072 dB) | see note |

Residual control (independent script): AAC-vs-source PCM max |diff| 47 LSB
(−56.9 dBFS) / RMS 1.7 LSB (track A), 18 LSB / 1.1 LSB (track B) — ordinary
lossy-coding residual, orders of magnitude above the 0.002–0.007 dB
loudness deltas, so the domain effect is negligible against codec noise.

Peak-criterion note: L2 §8 states the peak tolerance only qualitatively
("small against the residual control") with no numeric bound. Measured peak
deltas (≤0.0003 linear / ≤0.08 dB) sit well below the sample-level residual
floor (max 47 LSB ≈ 0.0014 linear), satisfying that reading as written; no
number was invented, and any future stricter bound remains an owner
decision. Committed 0.25 s fixtures yield `-inf` integrated in **both**
domains (consistent omission behavior — itself a finding: sub-block tracks
cannot carry track gain in either domain, so the synthetic 5 s album was
required for track-level comparison).

## Gate 5 — ReplayGain serialization and `iTunNORM` formula validation

**Status: PASS** (formula math, reference-writer serialization, and
original-code real-file bytes all validated; the future Rust implementation
itself is L3 work and remains unvalidated by definition — see boundary note).

Executed evidence (all in `/tmp/loudprep`, disposable copies only):

- Exact pipeline re-confirmed by direct read of the original
  `LoudnessProcessor` (`loudness.py` lines 211–325): `_add_itunes_soundcheck`
  reads back the **written file's** RG track-gain string (first hit of the 4
  `_REPLAYGAIN_KEYS` casings), `_get_replaygain_value` strips `dB`/whitespace
  and `float()`s it (so the effective input resolution is the 2-decimal
  rendered string), then `_replaygain_to_soundcheck(gain_db,
  reference_loudness=-18.0)` computes `adjusted = gain + (ref − (−18.0))`,
  `ratio = 10**(-adjusted/10)`, `int(round(ratio·1000))` /
  `int(round(ratio·2500))` (Python banker's `round`, then truncating `int`),
  clamps each slot to `[0, 0xFFFFFFFE]`, and formats `" {A:08X} {A:08X}
  {B:08X} {B:08X} 00024CA8 00024CA8 00007FFF 00007FFF 00024CA8 00024CA8"`
  (leading space, ten uppercase 8-hex fields, duplicated slots, fixed filler
  order). Write is `m4a[key] = [MP4FreeForm(s.encode('utf-8'))]; m4a.save()`
  with `key = '----:com.apple.iTunes:iTunNORM'` — the standard mutagen path.
- Formula cross-check (independent stdlib-only harness vs the **real original
  function imported read-only**, called unbound — it uses no instance state):
  all 10 vectors match plus a dense sweep of every 0.01 dB in [−40, +40]
  (8001 cases) with **0 mismatches**. Vectors: `0.00` → `000003E8`/`000009C4`;
  `−7.23` → `000014A4`/`0000339B` (this check caught and fixed the L2 draft
  transcription error `13210`); `+9.78` → `00000069`/`00000107`; `+0.18`
  (real clipping-adjusted case) → `000003BF`/`0000095F`; `+30.00` →
  `...0001`/`...0002`; `−30.00` → `000F4240`/`002625A0`; `−70.00` → all
  `FFFFFFFE` (never `FFFFFFFF`); `+100.00` → all `00000000`.
- Rounding pinned with direct evidence: exact-half sweep of the 0.01 dB grid
  over [−100, +100] finds exactly 3 float-exact halves — `+30.00` sc_2500
  (`2.5`: banker's 2 vs Rust half-away 3, executed both) and two negative
  cases (−96.54/−95.01 sc slots ≈ 1.1e13/3.2e12) that clamp to `FFFFFFFE`
  under **both** semantics (identical output). So `+30.00 dB → sc_2500 == 2`
  is the single reachable divergence and the pinned L3 vector: L3 must
  implement round-half-even. Neighbors `+29.99` (→ 3) and float-level
  `round(2.5)=2`/`round(3.5)=4` vs Rust `2.5f64.round()=3` recorded.
- **Real-file `iTunNORM` bytes (executed):** the real original
  `_add_itunes_soundcheck` (stub instance carrying only
  `reference_loudness=-18.0`; no Config, no subprocess, no rsgain run) was
  executed against 5 disposable M4A copies in `/tmp/loudprep/itn/` — 2 kept
  their real rsgain-written gains (`+9.78`, `+0.18` clipping-adjusted), 3
  carried constructed rsgain-format gains (`−7.23`, `0.00`, `+30.00` written
  via the same mutagen `MP4FreeForm` mechanism, verified `data` type 1).
  All 5 resulting payloads match the independently computed expectations
  **byte-for-byte** (5/5). Atom structure from `ilst` dumps: `----` with
  `mean=com.apple.iTunes`, `name=iTunNORM` (exact case), `data` **type=1,
  locale=0**, UTF-8 payload with leading space and the exact 10-field
  layout/constants. Pre-existing atoms preserved (`iTunSMPB` byte-identical
  to the pre-write dump; RG tags intact). `ffprobe` read-back:
  `TAG:iTunNORM= 00000001 00000001 00000002 ...` (leading space preserved).
- **Real-file RG bytes (prior pass, unchanged):** `rsgain easy` on disposable
  copies — FLAC Vorbis tags and M4A `----` atoms parsed byte-by-byte:
  `mean=com.apple.iTunes`, uppercase names, `data` type=1 locale=0,
  `"9.78 dB"`/`"12.50 dB"`/`"0.044464"` payloads, clipping-adjusted album
  gain `+0.18 dB` (measured +21.25/+10.07 dB, peak 0.979309 — exactly
  `−20·log10(peak)`), peak-verbatim, `iTunSMPB` preserved. Clipping vectors
  re-verified: negatives/zero unadjusted; +5.00/0.5 unadjusted; +5.00/0.9 →
  `+0.92`; +6.00/0.7 → `+3.10`; cap never below `+0.00`.

Boundary (explicit, not a gap in this gate): what ran is the original
implementation's own code path, so reference-output compatibility holds by
construction for the `iTunNORM` step; the full `process_album` end-to-end
(rsgain subprocess + verify + convert) was not re-run, and the future Rust
implementation does not exist yet — its T-atoms/T-norm vectors are L3 work
against the pinned spec above.

## Gate 6 — Album failure, retained-output, and no-replace test specification

**Status: PASS** (specification deliverable; test execution itself is L3).

Unchanged from the prior report and re-confirmed against the finalized L2
(incl. the D4 omit-with-warning rule and the D5 no-new-category rule):
album-incompleteness semantics (§5), retained-output interaction rules
(read retained sources; missing source ⇒ album incomplete), full §7
failure/CLI matrix (now 15 rows incl. clipping-adjustment), T-album/T-safe/
T-fail acceptance tests (§8), and the single-publication/no-rewrite
architecture (§2). No new flag, summary field, exit code, or public-signature
change is specified. The tests themselves are unrun — that is L3 work, not
a gap in this gate.

## Blockers requiring an owner decision

1. **No L3-prep evidence blockers remain.** All six gates are evidenced
   except the standing L2 §9 item 7: explicit owner approval of L3 itself.
2. **No design blockers.** The clipping policy is verified end-to-end
   against real `rsgain` output; the `iTunNORM` pipeline is verified
   byte-for-byte against the original implementation's own output
   (including the +30.00 dB rounding pin); the official EBU suite passes
   30/30; the differential experiment passes with margin; MSRV 1.60
   compiles; HISTOGRAM/memory demonstrated; no L0 decision needs
   reinterpretation.
3. L3 implementation remains unauthorized regardless: L2 §9 requires all
   seven gate items evidenced plus explicit L3 approval.

## Final gate status

| Gate | Status | One-line basis |
|---|---|---|
| 1. `ebur128` dep/license/MSRV/no-C-toolchain | PASS | Exact-1.60 lib build (`rlib` produced), MIT closure, zero C-compiler invocations, 1.85 build green |
| 2. EBU Tech 3341/3342 vectors | PASS | Official v5.0 archive (SHA-256 recorded); unmodified suite 30/30; v1-scope worst \|Δ\| 0.064 vs ±0.1 tol |
| 3. HISTOGRAM accuracy + bounded memory | PASS | Queue↔HISTOGRAM Δ ≤ 0.05 LU/LRA; 345 MB input → ≤ 2.6 MB RSS |
| 4. Source-vs-decoded-AAC differential | PASS | Track Δ ≤ 0.0069 dB, album Δ 0.0026 dB (≤ 0.5); peaks vs residual floor |
| 5. RG/`iTunNORM` serialization | PASS | Harness == original fn (8001/8001); 5/5 real-file atoms byte-exact incl. clip-adjusted + half-way pin; Rust impl itself is L3 work |
| 6. Album/retained/no-replace spec | PASS | Spec complete in finalized L2; execution is L3 work |

All six evidence gates hold a final status; the only remaining L3-readiness
item is the owner's explicit L3 approval (L2 §9 item 7).
