# AAC-ENC source provenance inventory (evidence only)

Inventory only. This file records traceable evidence about file origins to
support a future file-level provenance audit. It performs no legal analysis,
establishes no copyright ownership, and claims no legal clearance. Labels
describe the strength and nature of the available evidence, not legal
conclusions. A lack of textual similarity does not prove independent
authorship, especially for translated code. Do not treat any row as a
licensing determination.

## A. Scope and methodology

- Repository revision: `89bb289a32773ca06b49c2ba057bbc516101dd7a`, branch
  `main`.
- Date of inventory: 2026-10-27.
- Directories and file types examined (tracked files only; `rust/target/`
  is untracked build output and was excluded):
  - `rust/src/**/*.rs` (66 files: codec `aac_enc/*`, `common/*`, roots).
  - `rust/convert/src/*.rs`, `rust/convert/tests/**/*.rs` (18 files).
  - `rust/research/**/*.rs` (1 file), `rust/build.rs` (1 file).
  - `tools/enc-oracle/*.{cpp,py}` (33 files), `tools/c-enc-bench/*.c`
    (1 file), `tools/aac-enc-metrics/*.py` (4 files),
    `rust/tools/*.sh` (7 files).
  - Per-row inventory covers these 128 code files. Bulk data
    (329 `rust/src/**/testdata/*.txt` vectors, 4 WAVs, 30
    `rust/convert/tests/fixtures/*`) is summarized as groups in §D,
    not rowed.
- Upstream references actually available: no live FDK-AAC checkout exists
  in this environment (the private reference repo is not present; the
  `cpp/` tree was removed from this repo in `4c69425`). Upstream evidence
  comes from (1) git history before the removal (`4c69425~1` still
  contains the full `cpp/` tree), (2) the pin file
  `docs/reference/fdk-aac-reference-revision.txt`
  (`f14ad40a…`, private repo, not fetched), and (3) in-repo citations
  (`cpp:` file:line anchors, generator markers, migration-spec records).
  No repositories were cloned, no sources downloaded, no network used.
- Comparison methods used: see §F. First pass: `git
  log --diff-filter=A/--follow` (introducing commit per file),
  `git ls-tree`/`git cat-file` against `4c69425~1`
  (upstream-path resolution), `grep` census of license headers,
  `cpp:` anchor census per file, generator-marker search, and two
  spot content checks. Second pass (2026-10-27): anchor resolution for
  every cited upstream path against the historical tree
  (`3878c45a` = `4c69425~1`), cited-symbol verification via `git grep`
  on that tree, cited line-range spot checks, upstream symbol-name
  searches for table files, one table-value spot check, and
  post-import change analysis. No automated translated-code (C→Rust)
  diffing was performed in either pass; classification of translated
  files rests on in-repo anchors, verified symbol/path existence, and
  history, not on line-by-line comparison.
- Limitations: the initial commit `99356b5` is squashed (221 `.rs` files
  introduced at once under the message `FDK-AAC v3.0.0 (untouched)`), so
  no per-file port history exists for those files; the upstream tree is
  history-only; content spot checks are samples, not coverage (§F);
  `cpp:` anchor style varies (`cpp/…` prefix vs bare `intensity.cpp`);
  the word “generated” in comments causes false positives (§E).

## B. Summary

- Total implementation files inventoried (rows): 128.
- Provisional labels: `EXACT_UPSTREAM_MATCH` 0, `LIKELY_UPSTREAM_ADAPTATION`
  0, `LIKELY_TRANSLATED_UPSTREAM` 53, `POSSIBLY_INDEPENDENT` 58,
  `GENERATED` 4, `MIXED_ORIGIN` 13, `UNKNOWN` 0.
- Supporting-evidence strength (for the label, not for any legal
  conclusion): `HIGH` 4, `MEDIUM` 33, `LOW` 91, `NONE` 0.
  Second pass (2026-10-27, §F) raised 1 file from `LOW` to `MEDIUM`;
  label totals are unchanged.
- Important gaps: 29 translated-port files have no per-file upstream
  anchor (LOW; down from 31 after the second pass verified `mdct.rs`
  earlier and `qmf_tables.rs` now); 6 table-data files have no recorded
  generator (§E, down from 7); bulk vectors summarized, not individually
  verified; no live upstream checkout for direct diffing.

## C. File inventory

License column: `Full+Ferrite` = complete FDK-AAC block plus Ferrite
modified-version notice; `Concise` = Ferrite/Third-Party-Modified-Version
notice only; `None` = no FDK/TPMV notice. First-commit hashes are from
`git log --diff-filter=A`.

### C1. Codec crate `rust/src` (66 rows)

| Path | Language/type | Provisional label | Upstream counterpart | Evidence | Confidence | License notice found | Notes |
|---|---|---|---|---|---|---|---|
| `rust/src/aac_enc/adts.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libMpegTPEnc/src/tpenc_adts.cpp:192-237` | First commit `1785529` (M1-M3 port checkpoint); `Full+Ferrite`; 8 `cpp:` anchors | MEDIUM | Full+Ferrite | Primary label fits; anchor count is file-level, not a line diff |
| `rust/src/aac_enc/bitcount.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/dyn_bits.cpp` | First commit `e4258d2` (complete-encoder checkpoint); `Full+Ferrite`; 24 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/bitcount_tables.rs` | Rust | GENERATED | `cpp/libAACenc/src/aacEnc_rom.cpp` (ROM data) | `Generated by tools/enc-oracle/extract_bitcount_tables.py`; first commit `e4258d2` | HIGH | Full+Ferrite (short variant) | Verbatim data extraction; see §D |
| `rust/src/aac_enc/bitenc.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/bit_cnt.cpp` | First commit `e4258d2`; `Full+Ferrite`; 20 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/bitenc_tables.rs` | Rust | GENERATED | Upstream ROM (via oracle dump) | `Generated by tools/enc-oracle/extract_bitenc_tables.py`; first commit `e4258d2` | HIGH | Full+Ferrite (short variant) | See §D |
| `rust/src/aac_enc/block_switch.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/block_switch.cpp` | First commit `1785529`; `Full+Ferrite`; 13 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/channel_map.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/channel_map.cpp:139-155` | First commit `1785529`; `Full+Ferrite`; 7 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/config.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/aacenc_lib.cpp:544-682` | First commit `1785529`; `Full+Ferrite`; 23 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/encoder.rs` | Rust | MIXED_ORIGIN | — (new production API surface) | First commit `0a4911b` (production Encoder API); `Concise`; 0 anchors; TPMV notice asserts codec integration | LOW | Concise | API surface new (error taxonomy, push/flush, poisoning); behavior parity-checked vs `aac-enc.c` (`docs/aac-enc-m6-api-contract.md:107`); delegates to ported `StreamEncoder::step_frame`; lineage is architectural, not textual |
| `rust/src/aac_enc/energy.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `psy_main.cpp:631-749` | First commit `1785529`; `Full+Ferrite`; 10 anchors (bare `*.cpp` form) | MEDIUM | Full+Ferrite | Anchor lacks `cpp/` prefix; path still resolves in history |
| `rust/src/aac_enc/frame.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/bitenc.cpp:918-1178` | First commit `e4258d2`; `Full+Ferrite`; 34 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/intensity.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `intensity.cpp:258-817` (3 named functions) | First commit `0b3b1f9` (intensity-stereo feature); body states `Port of the C reference`; 9 anchors | MEDIUM | Concise | New-file notice form, but body documents the port explicitly |
| `rust/src/aac_enc/m5a_decode.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `0a4911b` (harness); `Concise`; 0 anchors; test-only decode-check harness | LOW | Concise | No counterpart identified; not proof of independence; TPMV notice present |
| `rust/src/aac_enc/m5b_measure.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `0a4911b` (harness); `Concise`; 0 anchors | LOW | Concise | Same caveat as above |
| `rust/src/aac_enc/m5g_census.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `0a4911b` (harness); `Concise`; 0 anchors | LOW | Concise | Same caveat as above |
| `rust/src/aac_enc/m6_migration.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `0a4911b` (harness); `Concise`; 0 anchors | LOW | Concise | Same caveat as above |
| `rust/src/aac_enc/pcm.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `aacenc_lib.cpp` (PCM buffering) | First commit `1785529`; `Full+Ferrite`; 3 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/pe.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `qc_main.cpp:435-452` | First commit `e4258d2`; `Full+Ferrite`; 30 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/phase1b.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `0a4911b` (harness); `Concise`; 0 anchors | LOW | Concise | No counterpart identified; not proof of independence |
| `rust/src/aac_enc/psy.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `psy_main.cpp:552-1130` | First commit `1785529`; `Full+Ferrite`; 15 anchors; `generated` hit is incidental wording (§E) | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/psy_config.rs` | Rust | MIXED_ORIGIN | — (loader for generated snapshots) | First commit `0a4911b`; `Concise`; loads `testdata/cfg_*.txt` via `include_str!`; snapshot source documented in comments | LOW | Concise | Loader moved verbatim from `testdata.rs` (M6.1, documented in-file); `cfg_text` dispatch + `include_str!` embedding new; snapshot data oracle-derived; see §D |
| `rust/src/aac_enc/q31.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `fixmul_arm.h:174-193` | First commit `1785529`; `Full+Ferrite`; 2 anchors | MEDIUM | Full+Ferrite | Fixed-point helper ported to float context |
| `rust/src/aac_enc/quant.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/quantize.cpp` | First commit `e4258d2`; `Full+Ferrite`; 23 anchors; `generated` hit is incidental wording (§E) | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/quant_tables.rs` | Rust | GENERATED | Upstream ROM (via oracle dump) | `@generated by tools/enc-oracle/extract_quant_tables.py — do not edit`; first commit `e4258d2` | HIGH | Full+Ferrite | See §D |
| `rust/src/aac_enc/rate.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/qc_main.cpp` (also `adj_thr.cpp`, `bitenc.cpp`) | First commit `e4258d2`; `Full+Ferrite`; 60 anchors, incl. function-level derivations | MEDIUM | Full+Ferrite | Densest anchor coverage in the port |
| `rust/src/aac_enc/scf.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/sf_estim.cpp` | First commit `e4258d2`; `Full+Ferrite`; 10 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/sfb.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/psy_configuration.cpp:534-627` | First commit `1785529`; `Full+Ferrite`; 7 anchors | MEDIUM | Full+Ferrite | Selection logic; widths live in `sfb_tables.rs` |
| `rust/src/aac_enc/sfb_tables.rs` | Rust | GENERATED | `cpp/libAACenc/src/aacEnc_rom.cpp`, `aacenc_tns.cpp`, `cpp/libFDK/src/FDK_tools_rom.cpp` | `@generated by tools/enc-oracle/extract_tables.py — do not edit` (inputs named in file); first commit `1785529`; spot check: `LONG_8000` values match ROM `aacEnc_rom.cpp:736` verbatim | HIGH | Full+Ferrite | Verbatim data; selection/use in `sfb.rs`; see §D |
| `rust/src/aac_enc/static_bits.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `bitenc.cpp:918-1178` | First commit `e4258d2`; `Full+Ferrite`; 11 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/stream_driver.rs` | Rust | MIXED_ORIGIN | Multiple `cpp:` anchors (test driver) | First commit `0a4911b`; `Concise`; 24 anchors; 10 commits (most-reworked file); test-path driver | MEDIUM | Concise | Production `step_frame` (anchors incl. `groupShortData` MS branch `grp_data.cpp:211-224`, `ms_stereo.cpp:109-330`) + `#[cfg(test)]` capture/parity harnesses; header states test code modifies no production algorithm |
| `rust/src/aac_enc/testdata.rs` | Rust | MIXED_ORIGIN | `aacenc.cpp:803` (single anchor) | First commit `1785529`; `Full+Ferrite`; committed vectors + `include_bytes!` WAVs + harness | LOW | Full+Ferrite | 35 harness fns + WAV/provenance loading new; vector data oracle-derived; history spans all 4 port checkpoints; inputs partly generated (§D) |
| `rust/src/aac_enc/threshold_vbr.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `adj_thr.cpp:2869-2899` | First commit `e4258d2`; `Full+Ferrite`; 21 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/thresholds.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `psy_main.cpp:922-1019` | First commit `1785529`; `Full+Ferrite`; 10 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/tns.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libAACenc/src/aacenc_tns.cpp` | First commit `1785529`; `Full+Ferrite`; 12 anchors | MEDIUM | Full+Ferrite | — |
| `rust/src/aac_enc/transform.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libFDK/src/mdct.cpp:135` (kernel), `psy_main.cpp:490-627`, `transform.cpp:132-156` | First commit `1785529`; `Full+Ferrite`; 5 anchors; module docs describe staging and the line-for-line kernel claim; spot check: upstream `mdct_block` comment/structure consistent with the claim (fold `-D-Cr,A-Br`) | MEDIUM | Full+Ferrite | Kernel itself lives in `common/mdct.rs` |
| `rust/src/aac_enc.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — (module root) | First commit `1785529`; `Full+Ferrite`; 0 anchors; aggregates ported modules | LOW | Full+Ferrite | No per-file anchor; label rests on header + import context |
| `rust/src/lib.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — (crate root) | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat; crate root of the port |
| `rust/src/common.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — (module root) | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/aot.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Full header is the main evidence; no anchor recorded |
| `rust/src/common/autocorr2nd.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/bitbuffer.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/bitstream.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/channel_map_descr.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/channel_order.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/checked_cast.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/crc.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/dct.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/enums.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/fft.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/fft/dit_fft.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; comments state twiddle tables `was generated with a C-code` (script unrecorded, §E) | LOW | Full+Ferrite | Table-generation claim without a recorded generator |
| `rust/src/common/fft/fft_n2.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/fft/fft_tables.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; table data, no generator marker (§E) | LOW | Full+Ferrite | Candidate GENERATED pending script link; not assumed |
| `rust/src/common/hybrid.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; `generated` hit is incidental wording (§E) | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/mdct.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libFDK/src/mdct.cpp:135` (`mdct_block`) | First commit `99356b5` (squashed); `Full+Ferrite`; kernel claim recorded in `transform.rs:99-107` (line-for-line float port, same fold/state/offset); second pass verified upstream `mdct_block` at the cited line with the cited behavior; post-import changes are renames + licensing only | MEDIUM | Full+Ferrite | Strongest-evidenced `common/` file; still no line diff performed |
| `rust/src/common/mpeg_id.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/pow.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/qmf.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/qmf/qmf_fb.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/qmf/qmf_tables.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | `cpp/libFDK/src/FDK_tools_rom.cpp`, `cpp/libFDK/include/FDK_tools_rom.h` (`QMF*_MPSLDFB_PFT_TABLE_SIZE`, QMF tables) | First commit `99356b5` (squashed); `Full+Ferrite`; exact upstream symbol-name match verified in second pass; no generator marker (§E) | MEDIUM | Full+Ferrite | Symbol match is name-level; values not compared |
| `rust/src/common/quantize.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/samplerate_index.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/tables.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; 0 anchors | LOW | Full+Ferrite | Same caveat |
| `rust/src/common/tables/channel_map_descriptors.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; table data, no generator marker (§E) | LOW | Full+Ferrite | Candidate GENERATED pending script link; not assumed |
| `rust/src/common/tables/pow_4_over_3.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; table data, no generator marker (§E) | LOW | Full+Ferrite | Candidate GENERATED pending script link; not assumed |
| `rust/src/common/tables/sine_tables.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; table data, no generator marker (§E) | LOW | Full+Ferrite | Candidate GENERATED pending script link; not assumed |
| `rust/src/common/tables/window_tables.rs` | Rust | LIKELY_TRANSLATED_UPSTREAM | — | First commit `99356b5` (squashed); `Full+Ferrite`; table data, no generator marker (§E) | LOW | Full+Ferrite | Candidate GENERATED pending script link; not assumed |

### C2. Converter, research, build (20 rows)

| Path | Language/type | Provisional label | Upstream counterpart | Evidence | Confidence | License notice found | Notes |
|---|---|---|---|---|---|---|---|
| `rust/build.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `99356b5` (squashed); rustversion cfg probes; no FDK notice, no anchors | LOW | None | Build logic; no counterpart identified, not proof of independence |
| `rust/convert/src/adts.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `e2b5731` (converter feature); no FDK notice, no anchors | LOW | None | App code; same caveat |
| `rust/convert/src/batch.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `43dcd02` (batch feature); no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/src/flac.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `e2b5731`; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/src/gapless.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `e2b5731`; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/src/lib.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `e2b5731`; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/src/loudness.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `ba4f6e6` (loudness feature); no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/src/m4a.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `e2b5731`; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/src/main.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `e2b5731`; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/src/meta.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `7ebf208` (metadata feature); no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/src/output.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `e2b5731`; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/tests/batch.rs` | Rust | POSSIBLY_INDEPENDENT | — | First commit `43dcd02`; test code; `generated` hit is incidental comment (§E) | LOW | None | Same caveat |
| `rust/convert/tests/cli.rs` | Rust | POSSIBLY_INDEPENDENT | — | Test code; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/tests/common/mod.rs` | Rust | POSSIBLY_INDEPENDENT | — | Test support; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/tests/conversion.rs` | Rust | POSSIBLY_INDEPENDENT | — | Test code; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/tests/coverart.rs` | Rust | POSSIBLY_INDEPENDENT | — | Test code; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/tests/independent_decode.rs` | Rust | POSSIBLY_INDEPENDENT | — | Test code (external-decoder smoke); no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/tests/loudness.rs` | Rust | POSSIBLY_INDEPENDENT | — | Test code; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/convert/tests/metadata.rs` | Rust | POSSIBLY_INDEPENDENT | — | Test code; no FDK notice, no anchors | LOW | None | Same caveat |
| `rust/research/spike-m4a/src/main.rs` | Rust | POSSIBLY_INDEPENDENT | — | Research spike; no FDK notice, no anchors | LOW | None | Same caveat |

### C3. Port tooling: probes, extractors, metrics, scripts (42 rows)

| Path | Language/type | Provisional label | Upstream counterpart | Evidence | Confidence | License notice found | Notes |
|---|---|---|---|---|---|---|---|
| `tools/enc-oracle/enc_oracle.cpp` | C++ | MIXED_ORIGIN | Upstream headers (oracle driver) | First commit `1785529`; project-authored driver around upstream code | MEDIUM | None checked | Self-declared: no codec code of its own; per-frame `psy_main` orchestration mirrored statement-for-statement; ~20 upstream headers included; see §D |
| `tools/enc-oracle/bitcount_probe.cpp` | C++ | MIXED_ORIGIN | Upstream ROM/behavior | First commit `e4258d2`; probe driving upstream code | MEDIUM | None checked | Reads `FDKaacEnc_huff_ltab*` ROM tables and prints values; thin dump driver |
| `tools/enc-oracle/bitenc_probe.cpp` | C++ | MIXED_ORIGIN | Upstream ROM/behavior | First commit `e4258d2`; probe driving upstream code | MEDIUM | None checked | Drives real `FDKaacEnc_codeValues`/`codeScalefactorDelta` through an FDK bitstream; cross-checked against `aacEnc_rom.cpp` text; feeds scripted table extraction |
| `tools/enc-oracle/quant_probe.cpp` | C++ | MIXED_ORIGIN | Upstream quant behavior | First commit `e4258d2`; probe driving upstream code | MEDIUM | None checked | Uses only exported upstream functions (`FDKaacEnc_QuantizeSpectrum`, `calcSfbDist`, …); static functions excluded by design |
| `tools/enc-oracle/scf_probe.cpp` | C++ | MIXED_ORIGIN | Upstream SCF behavior | First commit `e4258d2`; probe driving upstream code | MEDIUM | None checked | Prints `FL2FXCONST_DBL` values exactly as the reference build computes them (constant probe) |
| `tools/enc-oracle/scf_unit_probe.cpp` | C++ | MIXED_ORIGIN | Upstream SCF behavior | First commit `e4258d2`; probe driving upstream code | MEDIUM | None checked | `#include`s upstream `sf_estim.cpp` implementation directly (production source untouched); strongest textual upstream incorporation among the probes |
| `tools/enc-oracle/s6lit_probe.cpp` | C++ | MIXED_ORIGIN | Upstream behavior | First commit `e4258d2`; probe driving upstream code | MEDIUM | None checked | Evaluates upstream `FL2FXCONST_DBL` expressions as C evaluates them; every literal in `rate.rs` cites this probe |
| `tools/enc-oracle/tp_probe.cpp` | C++ | MIXED_ORIGIN | `libMpegTPEnc` behavior | First commit `e4258d2`; probe driving upstream code | MEDIUM | None checked | Verifies 56-bit ADTS `globHdrBits` via the real `transportEnc`; provenance of the constant, not a build dependency |
| `tools/c-enc-bench/c-enc-bench.c` | C | MIXED_ORIGIN | Upstream encoder API | First commit `5f6ac0e` (isolated C benchmark driver); driver around upstream encoder | LOW | None checked | Header: “No encoder algorithm is duplicated here”; TIMED region = upstream public-API sequence (`aacEncOpen`/SetParam/Encode/EncInfo/flush) per `aac-enc.c` |
| `tools/enc-oracle/extract_tables.py` | Python | POSSIBLY_INDEPENDENT | — (generator; see §D) | First commit `1785529`; project-authored ROM/table extractor | LOW | None checked | Tooling; no upstream counterpart; not independence proof |
| `tools/enc-oracle/extract_testdata.py` | Python | POSSIBLY_INDEPENDENT | — (generator; see §D) | First commit `1785529`; project-authored oracle-dump extractor | LOW | None checked | Same caveat |
| `tools/enc-oracle/make_fixtures.py` | Python | POSSIBLY_INDEPENDENT | — (fixture maker; see §D) | First commit `1785529`; project-authored fixture tooling | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_bitcount_tables.py` | Python | POSSIBLY_INDEPENDENT | — (generator; see §D) | First commit `e4258d2`; feeds `bitcount_tables.rs` | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_bitenc_tables.py` | Python | POSSIBLY_INDEPENDENT | — (generator; see §D) | First commit `e4258d2`; feeds `bitenc_tables.rs` | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_quant_tables.py` | Python | POSSIBLY_INDEPENDENT | — (generator; see §D) | First commit `e4258d2`; feeds `quant_tables.rs` | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_s1.py` | Python | POSSIBLY_INDEPENDENT | — (dump extractor) | First commit `e4258d2`; S1 dump extractor | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_s2.py` | Python | POSSIBLY_INDEPENDENT | — (dump extractor) | First commit `e4258d2`; S2 dump extractor | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_s3.py` | Python | POSSIBLY_INDEPENDENT | — (dump extractor) | First commit `e4258d2`; S3 dump extractor | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_s4.py` | Python | POSSIBLY_INDEPENDENT | — (dump extractor) | First commit `e4258d2`; S4 dump extractor | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_s5.py` | Python | POSSIBLY_INDEPENDENT | — (dump extractor) | First commit `e4258d2`; S5 dump extractor | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_s6.py` | Python | POSSIBLY_INDEPENDENT | — (dump extractor) | First commit `e4258d2`; S6 dump extractor | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_s7.py` | Python | POSSIBLY_INDEPENDENT | — (dump extractor) | First commit `e4258d2`; S7 dump extractor | LOW | None checked | Same caveat |
| `tools/enc-oracle/extract_s8.py` | Python | POSSIBLY_INDEPENDENT | — (dump extractor) | First commit `e4258d2`; S8 dump extractor | LOW | None checked | Same caveat |
| `tools/enc-oracle/validate_s1.py` | Python | POSSIBLY_INDEPENDENT | — (dump validator) | First commit `e4258d2` | LOW | None checked | Same caveat |
| `tools/enc-oracle/validate_s2.py` | Python | POSSIBLY_INDEPENDENT | — (dump validator) | First commit `e4258d2` | LOW | None checked | Same caveat |
| `tools/enc-oracle/validate_s3.py` | Python | POSSIBLY_INDEPENDENT | — (dump validator) | First commit `e4258d2` | LOW | None checked | Same caveat |
| `tools/enc-oracle/validate_s4.py` | Python | POSSIBLY_INDEPENDENT | — (dump validator) | First commit `e4258d2` | LOW | None checked | Same caveat |
| `tools/enc-oracle/validate_s5.py` | Python | POSSIBLY_INDEPENDENT | — (dump validator) | First commit `e4258d2` | LOW | None checked | Same caveat |
| `tools/enc-oracle/validate_s6.py` | Python | POSSIBLY_INDEPENDENT | — (dump validator) | First commit `e4258d2` | LOW | None checked | Same caveat |
| `tools/enc-oracle/validate_s7.py` | Python | POSSIBLY_INDEPENDENT | — (dump validator) | First commit `e4258d2` | LOW | None checked | Same caveat |
| `tools/enc-oracle/validate_s8.py` | Python | POSSIBLY_INDEPENDENT | — (dump validator) | First commit `e4258d2` | LOW | None checked | Same caveat |
| `tools/aac-enc-metrics/vbr5_metrics.py` | Python | POSSIBLY_INDEPENDENT | — (metrics tooling) | First commit `29e3f7c` (quality diagnostics) | LOW | None checked | Same caveat |
| `tools/aac-enc-metrics/vbr5_localize.py` | Python | POSSIBLY_INDEPENDENT | — (metrics tooling) | First commit `29e3f7c` | LOW | None checked | Same caveat |
| `tools/aac-enc-metrics/test_vbr5_metrics.py` | Python | POSSIBLY_INDEPENDENT | — (metrics tests) | First commit `29e3f7c` | LOW | None checked | Same caveat |
| `tools/aac-enc-metrics/test_vbr5_localize.py` | Python | POSSIBLY_INDEPENDENT | — (metrics tests) | First commit `29e3f7c` | LOW | None checked | Same caveat |
| `rust/tools/check-licensing.sh` | Shell | POSSIBLY_INDEPENDENT | — (repo hygiene check) | First commit `89bb289` (this HEAD); read-only text-match check | LOW | None checked | Same caveat |
| `rust/tools/check-reference.sh` | Shell | POSSIBLY_INDEPENDENT | — (checkout validator) | First commit `51904e7`; validates reference checkout vs pin file | LOW | None checked | Same caveat |
| `rust/tools/preflight-validation.sh` | Shell | POSSIBLY_INDEPENDENT | — (validation gate) | First commit `51904e7` | LOW | None checked | Same caveat |
| `rust/tools/run-reference-validation.sh` | Shell | POSSIBLY_INDEPENDENT | — (validation driver) | First commit `51904e7` | LOW | None checked | Same caveat |
| `rust/tools/check-decode-evidence.sh` | Shell | POSSIBLY_INDEPENDENT | — (CI evidence check) | First commit `ff6ad4a` | LOW | None checked | Same caveat |
| `rust/tools/test-tierb-report.sh` | Shell | POSSIBLY_INDEPENDENT | — (report tooling) | First commit `3a35ced` | LOW | None checked | Same caveat |
| `rust/tools/tierb-report.sh` | Shell | POSSIBLY_INDEPENDENT | — (report tooling) | First commit `3a35ced` | LOW | None checked | Same caveat |

Row count check: C1 66 + C2 20 + C3 42 = 128. Every row has exactly one
primary label.

## D. Generated files and tooling

Confirmed generation (explicit `@generated`/“Generated by” marker naming
the script and inputs):

| Output | Generator | Inputs | Provenance link |
|---|---|---|---|
| `rust/src/aac_enc/sfb_tables.rs` | `tools/enc-oracle/extract_tables.py` | `cpp/libAACenc/src/aacEnc_rom.cpp` (SFB widths, TNS coefficients), `aacenc_tns.cpp` (ACF windows), `cpp/libFDK/src/FDK_tools_rom.cpp` | Marker lines + script defaults; values spot-verified verbatim (`LONG_8000` vs ROM `aacEnc_rom.cpp:736`) |
| `rust/src/aac_enc/quant_tables.rs` | `tools/enc-oracle/extract_quant_tables.py` | Oracle dump of upstream quant tables | `@generated … do not edit` marker |
| `rust/src/aac_enc/bitcount_tables.rs` | `tools/enc-oracle/extract_bitcount_tables.py` | Oracle dump of upstream ROM bitcount data | `Generated by …` marker |
| `rust/src/aac_enc/bitenc_tables.rs` | `tools/enc-oracle/extract_bitenc_tables.py` | Oracle dump of upstream ROM bitenc data | `Generated by …` marker |

Generated data summarized (not rowed): 329 `rust/src/**/testdata/*.txt`
vectors (oracle S1–S8 dumps via `extract_s*.py`/`extract_testdata.py`;
4 `cfg_*.txt` snapshots loaded by `psy_config.rs` via `include_str!`;
`m55-PROVENANCE.txt`, `m8-PROVENANCE.txt` carried alongside WAVs),
4 committed WAVs loaded via `include_bytes!` in `testdata.rs`, and
`rust/convert/tests/fixtures/*.flac` produced by
`rust/convert/tests/fixtures/generate.py`. Bulk contents were not
individually verified; provenance files were confirmed present.

Suspected but unconfirmed (no recorded generator; do not assume):
`common/fft/fft_tables.rs`,
`common/tables/{sine_tables,window_tables,pow_4_over_3,channel_map_descriptors}.rs`
(table data, full FDK headers, no marker), and `common/fft/dit_fft.rs`
twiddle tables (“generated with a C-code” per comments, script
unidentified). Second pass: upstream functional equivalents exist
(`SineTable80/384/480/512/1024` in `cpp/libFDK/src/FDK_tools_rom.cpp`)
but the Rust symbol names differ (`SINE_TABLE_32/48`, `SINE_WINDOW_*`,
`FFT16_W16`, `POW_4_OVER_3` have no upstream name match) and values were
not normalized-compared (fixed-point vs float encodings).
`qmf_tables.rs` graduated to an exact upstream name match (§F) but still
has no recorded generator. Link a producing script before any GENERATED
label is applied.

Incidental `generated` wording (not generation evidence):
`aac_enc/psy.rs:1897`, `aac_enc/quant.rs:179`,
`common/hybrid.rs:156`, `convert/tests/batch.rs:453`.

## E. Unresolved inventory gaps

1. Squashed initial commit: 221 `.rs` files share introducing commit
   `99356b5`; per-file port history is unrecoverable from git. Labels
   for those files rest on headers, anchors, and the repo’s own port
   documentation (`README.md`, `docs/aac-enc-migration-spec.md`), not on
   distinct port commits.
2. Upstream tree is history-only (removed in `4c69425`; verified at
   `3878c45a` = `4c69425~1`). No live checkout was available, so no
   direct whole-file diffing was performed. All 21 distinct cited
   upstream paths resolve in history (0 missing), and named cited
   symbols (`mdct_block`, `FDKaacEnc_*`, `groupShortData`,
   `DistributeBits`, …) were confirmed present via `git grep` on that
   tree (§F) — but path/symbol resolution is not content comparison.
3. Content checks are samples, not coverage: first pass verified the
   MDCT comment/structure claim and one SFB width table; second pass
   verified 12 cited line ranges (9 land exactly on the cited
   construct, 2 are region-level) and the three `intensity.cpp`
   function definitions at their cited lines. No automated C→Rust
   translation detection was run; the method cannot reliably detect
   translated code beyond the recorded anchors. This limitation is
   stated, not waived.
4. Six table-data files lack a recorded generator (§D); treating them
   as GENERATED would require linking the producing script first.
   (`qmf_tables.rs` has an exact upstream name match but no generator.)
5. Anchor-style variance: most anchors use the `cpp/…` prefix, but some
   (notably `intensity.rs`) cite bare filenames such as `intensity.cpp`.
   The bare form was verified for `intensity.cpp` (all three cited
   functions exist at the cited lines); it was not exhaustively
   verified elsewhere.
6. The converter/tooling `POSSIBLY_INDEPENDENT` rows record absence of
   identified counterparts only; several link the FDK-derived codec at
   build time (`ferrite-aac` path dependency; probes `#include`
   upstream headers), which bounds but does not settle their status.
7. Licensing characterization of newly authored files, the
   replacement-term reading, and patent questions remain open per
   `AGENTS.md` and are out of scope for this inventory.

## F. Second-pass comparison results (2026-10-27)

Starting revision `89bb289` (branch `main`); pre-existing
quality-audit changes untouched throughout. All 66 `rust/src` codec
files were considered; the 62 converter/research/tooling rows are
unchanged. No network, no source modifications, nothing staged or
committed.

1. Methods run (all read-only, all local): (a) anchor census — every
   `*.cpp(:lines)?`/`*.h(:lines)?` citation in `rust/src` collected
   (434 raw hits; regex false positives such as `self.hf_memory` or
   license-text `s.h` excluded by inspection); (b) upstream-path
   resolution of all 21 distinct `cpp/…` paths against the historical
   tree; (c) cited-symbol verification via `git grep` on that tree for
   named functions (`FDKaacEnc_prepareIntensityDecision`,
   `FDKaacEnc_quantizeLines`, `groupShortData`, `DistributeBits`,
   `mdct_block`, `FDKaacEnc_calcMaxValueInSfb`,
   `FDKaacEnc_SyncBlockSwitching`, …); (d) 12 cited line-range spot
   checks; (e) upstream symbol-name searches for the 7 unrecorded
   table files plus one table-value spot check; (f) post-import change
   analysis (`git log`/`git show` per file).
2. Historical tree: commit `3878c45a` (`4c69425~1`), the last tree
   containing `cpp/` (`libAACenc`, `libFDK`, `libMpegTPEnc`,
   `libSYS`, `libPCMutils`, …). Starting `cpp:` anchors were treated
   as hypotheses and verified below, not assumed.
3. Stronger evidence: 1 file raised `LOW`→`MEDIUM` (`qmf_tables.rs`,
   exact upstream name match). All 21 distinct cited upstream paths
   resolve (0 missing). Named cited symbols confirmed present in the
   historical tree. Of 12 line-range anchors checked, 9 land exactly
   on the cited construct (e.g. `intensity.cpp:258` =
   `FDKaacEnc_prepareIntensityDecision`; `psy_const.h:121` =
   `LONG_WINDOW = 0`; `psy_const.h:151` = `MAX_GROUPED_SFB`;
   `fixpoint_math.h:948` = `GetInvInt`; `grp_data.cpp:211` = MS-branch
   comment; `channel_map.cpp:139` = `channelModeConfig[]`;
   `psy_configuration.cpp:534` = `FDKaacEnc_InitPsyConfiguration`;
   `intensity.cpp:490/614` = the other two cited functions) and 2 are
   region-level (`psy_main.cpp:552`, `adj_thr.cpp:2869`). Post-import
   history: sampled `common/` files changed after `99356b5` only in
   the `ferrite-aac` rename (`9f3a6c9`, e.g. one doctest path) and the
   licensing notice (`e3aeb7f`, +4 lines); port content dates to the
   squashed import.
4. Label/confidence changes: exactly 1 (`qmf_tables.rs` LOW→MEDIUM;
   label unchanged). Established: one verbatim table-value match
   (`LONG_8000` vs `aacEnc_rom.cpp:736`); two exact upstream
   symbol-name matches (`QMF*_MPSLDFB_PFT_TABLE_SIZE`,
   `channelModeConfig`, the latter already cited in-file); function-
   level translated relationships with verified anchors (`intensity.*`
   ×3, MDCT kernel, `rate`/`frame`/`pe`/`quant` families — hypotheses
   with anchor evidence, not line diffs). No `EXACT_UPSTREAM_MATCH`
   (whole-file) was established for any file.
5. Unresolved: `sine_tables`, `window_tables`, `fft_tables`,
   `pow_4_over_3`, `channel_map_descriptors` (upstream functional
   equivalents exist but names differ and values uncompared);
   `dit_fft.rs` twiddle generator; the 8 new-harness/API files and
   `testdata.rs` (no counterparts found by these methods; labels
   retained, uncertainty retained); bare-form anchors outside
   `intensity.cpp`; the squashed per-file history (permanent).
6. Limitations: automated C→Rust comparison cannot reliably detect
   translation; matching identifiers/table names are evidence of a
   relationship, not of extent; a table-value match does not establish
   the origin of surrounding code; confidence was not inflated from
   coincident weak indicators — 29 anchored-but-unverified files
   remain LOW.
## G. Targeted review of mixed-origin and harness files (2026-10-27)

Manual file-by-file review of the 13 `MIXED_ORIGIN` rows (§C) and the
8 harness/API files named as having no known upstream counterpart
(§F item 5). The 8 are `encoder.rs`, `stream_driver.rs`,
`psy_config.rs` (also MIXED — both angles covered below),
`m5a_decode.rs`, `m5b_measure.rs`, `m5g_census.rs`,
`m6_migration.rs`, and `phase1b.rs`. No label or confidence level
changed; findings below refine the evidence and the remaining
uncertainty. Nothing here establishes authorship, derivative status,
distribution rights, or patent position.

### G1. Mixed-origin codec files

- `rust/src/aac_enc/encoder.rs` (858 lines; commits `0a4911b`,
  `6cc14d7`, `20c252d`, `9f3a6c9`, `f5681f9`, `e3aeb7f`).
  Upstream-derived: behavioral parity target — the API contract
  (`docs/aac-enc-m6-api-contract.md:107`) specifies the encoder chain
  plus `aac-enc.c` parity, and the implementation delegates frame
  processing to the ported `StreamEncoder::step_frame`; no direct
  textual mapping exists (0 `cpp:` anchors). Other: the public API
  itself is new — `Encoder::new/push/flush/is_finished`,
  `AdtsFrame`, `EncodeError`/`EncodeFailure` taxonomy, poison-on-
  failure semantics, doctests. Positive evidence for newness is the
  M6 feature/refactor history; absence of anchors is not independence
  proof. Retained: `MIXED_ORIGIN`, LOW.
- `rust/src/aac_enc/stream_driver.rs` (6027 lines; 10 commits, the
  most-reworked file). Upstream-derived: the production
  `StreamEncoder::step_frame` pipeline — grouping
  (`psy_main.cpp:1071-1084`), `FDKaacEnc_groupShortData` MS branch
  (`grp_data.cpp:211-224`, verified at the cited lines), MS processing
  (`ms_stereo.cpp:109-330`), threshold Ld C floor
  (`psy_main.cpp:1071-1075`) — 24 anchors total. Other: ~half the
  file is `#[cfg(test)]` capture/parity infrastructure (`S1Cap`,
  `FrameSnapshot`, `phase1_tests`, `m52_tests`); the module header
  states test code modifies no production algorithm. The test
  scaffolding has no upstream counterpart identified. Retained:
  `MIXED_ORIGIN`, MEDIUM.
- `rust/src/aac_enc/psy_config.rs` (192 lines; `0a4911b`,
  `e3aeb7f`). Upstream-derived: the snapshot *data* — long/short
  `PSY_CONFIGURATION` + `TNSCFG` dumps produced by
  `tools/enc-oracle/extract_testdata.py` from real-C `enc_oracle`
  output (provenance stated in-file). Other: the loader itself is
  new — moved verbatim from `testdata.rs` in the documented M6.1
  relocation, plus new `cfg_text` dispatch and the M6.3
  `include_str!` embedding design. Retained: `MIXED_ORIGIN`, LOW.
- `rust/src/aac_enc/testdata.rs` (2462 lines, 35 fns; history spans
  all four port checkpoints `1785529`→`0a4911b`). Upstream-derived:
  committed oracle vectors and the single behavioral anchor
  (`aacenc.cpp:803`); the 4 WAVs are external fixtures with their own
  provenance files (`m55-PROVENANCE.txt`, `m8-PROVENANCE.txt`), not
  upstream FDK material. Other: the 35-function harness (loading,
  resynthesis, table access) is project-authored. Retained:
  `MIXED_ORIGIN`, LOW.

### G2. Mixed-origin tooling

- `tools/enc-oracle/enc_oracle.cpp` (3076 lines; `1785529`,
  `e4258d2`). Upstream-derived: ~20 upstream headers included and
  driven (real `InitPsyConfiguration`/`InitTnsConfiguration`/
  `InitPreEchoControl`); the file header self-declares that the
  per-frame `psy_main` orchestration is “mirrored
  statement-for-statement (integer ops are exact)”. Other: dump
  printers (`print_ints`, `dump_config`, …), `main` CLI, transcript
  formats — project scaffolding with no upstream counterpart.
  Retained: `MIXED_ORIGIN`, MEDIUM.
- Table-dump probes (`bitcount_probe.cpp` 50 lines,
  `bitenc_probe.cpp` 224 lines): upstream-derived — direct reads of
  `FDKaacEnc_huff_ltab*` ROM tables and the real
  `FDKaacEnc_codeValues`/`codeScalefactorDelta` through an FDK
  bitstream (`bitenc_probe` header notes cross-checking against
  `aacEnc_rom.cpp` text and feeding scripted table extraction).
  Other: `main`/printing loops. Retained: `MIXED_ORIGIN`, MEDIUM.
- Function probes (`quant_probe.cpp` 260 lines,
  `scf_unit_probe.cpp` 274 lines, `scf_probe.cpp` 34 lines):
  upstream-derived — calls into upstream quant/SFC behavior
  (`quant_probe` deliberately uses only exported functions, statics
  excluded); `scf_unit_probe.cpp` `#include`s the upstream
  `sf_estim.cpp` implementation file directly (production source
  untouched) — the strongest textual upstream incorporation among
  the probes. Other: transcript printing, CLI scaffolding. Retained:
  `MIXED_ORIGIN`, MEDIUM.
- Literal/transport probes (`s6lit_probe.cpp` 52 lines,
  `tp_probe.cpp` 55 lines): upstream-derived — `s6lit_probe`
  evaluates upstream `FL2FXCONST_DBL` expressions exactly as C does
  (every literal in `rate.rs` cites it); `tp_probe` verifies the
  56-bit ADTS `globHdrBits` via the real `transportEnc` and is
  recorded as the constant’s provenance. Other: minimal `main`
  wrappers. Retained: `MIXED_ORIGIN`, MEDIUM.
- `tools/c-enc-bench/c-enc-bench.c` (222 lines; single commit
  `5f6ac0e`). Upstream-derived: the TIMED region is the upstream
  public-API call sequence (`aacEncOpen`/SetParam/Encode/EncInfo/
  flush) following `aac-enc.c`’s parameter order. Other: timing,
  WAV I/O, reporting, and validation scaffolding; the header states
  “No encoder algorithm is duplicated here.” The call-sequence
  derivation keeps the file mixed rather than independent.
  Retained: `MIXED_ORIGIN`, LOW.

### G3. Harness/API files with no identified counterpart

`encoder.rs`, `stream_driver.rs`, `psy_config.rs` are covered in G1
(harness angle: `encoder.rs` is first client-tested by
`m6_migration.rs`; `stream_driver.rs` hosts the `#[cfg(test)]`
harnesses; `psy_config.rs` serves both production embedding and test
gating — dual use recorded, not double-counted). For the remaining
five, repository evidence was searched (headers, symbols, anchors,
history, `docs/aac-enc-m*.md`): no upstream function, file, or table
counterpart was identified for any of them. Each is project-authored
validation scaffolding around the ported pipeline (hence each carries
the concise Ferrite/TPMV notice); that integration is not an
independence claim.

- `m5a_decode.rs` (167 lines): decode-and-check harness; emits ADTS
  through the `stream_driver` test path and shells to the prebuilt
  external `aac-dec` binary (`AAC_DEC_BIN`). Later fix commit
  `f56bdbe` (crash-recovered streams). No upstream symbols.
- `m5b_measure.rs` (64 lines): input-dump helper; resynthesizes
  fixtures as WAVs under `$TMPDIR` for joint consumption by the C
  reference, the decoder, and offline metrics. No upstream symbols.
- `m5g_census.rs` (198 lines): branch-execution census over the
  19-stream inventory; asserts M5.f policy criteria and writes a
  JSON artifact. No upstream symbols.
- `m6_migration.rs` (86 lines): migration-equality harness; first
  client of the production `Encoder`, asserting byte-identical ADTS
  against the test-gated path. No upstream symbols.
- `phase1b.rs` (481 lines): oracle-fed differential diagnostic;
  feeds committed S1/S2 vectors into Rust S4/S6 stages with fresh
  per-frame state. No upstream symbols.

All five retained: `POSSIBLY_INDEPENDENT`, LOW — absence of a match
is recorded as a method outcome, not as authorship evidence.

### G4. Questions left explicitly unresolved

1. Whether the Rust ports preserve protectable expression from any
   particular upstream function (e.g. `mdct_block`,
   `FDKaacEnc_prepareIntensityDecision`, `groupShortData`): anchors
   and symbol presence are established; expression-level comparison
   was not performed.
2. Whether any unrecorded table (`sine_tables`, `window_tables`,
   `fft_tables`, `pow_4_over_3`, `channel_map_descriptors`,
   `dit_fft.rs` twiddles) was copied, transformed, independently
   recreated, or generated — values were not normalized-compared.
3. Whether `encoder.rs`’s API/error design or the harness files’
   orchestration contain independently copyrightable expression:
   no basis in this inventory for that determination either way.
4. Whether any file’s origin can be fixed despite the squashed
   `99356b5` import: per-file port history is unrecoverable from
   git; only in-file and doc evidence can narrow it.
5. Whether `scf_unit_probe.cpp`’s direct `#include` of upstream
   `sf_estim.cpp`, or `enc_oracle.cpp`’s mirrored orchestration
   block, has consequences beyond provenance documentation: out of
   scope for this inventory; see `AGENTS.md` open questions.
