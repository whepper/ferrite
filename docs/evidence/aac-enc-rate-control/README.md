# Rate-control / bit-reservoir audit evidence

Diagnostic evidence for
[`docs/aac-enc-rate-control-divergence-audit.md`](../../aac-enc-rate-control-divergence-audit.md).
No production behavior was changed, no C oracle was built, and no reference
artifact was regenerated.

## Method

1. Read the Rust VBR-5 path (`StreamEncoder` → `qc_main_frame`) and the pinned
   C reference (`aacenc.cpp`, `qc_main.cpp`, `adj_thr.cpp`) to identify the
   state that actually survives a frame boundary.
2. Replay the committed C S6 vectors through the Rust QC kernel
   (`rate::tests::s6_corpus_exact`, `s6_grant_state_evolution_onset_chain`,
   `s6_padding_chain_matches_oracle`, `s6_init_vectors_match_oracle`).
3. Run the existing 19-stream live census
   (`aac_enc::m5g_census::m5g_tests::m5g_branch_census`) and record every
   frame's pass count, gain adjustments, crash marker, grants, used bits,
   fill, total, and budget margins.
4. Compare the current Rust ADTS frame lengths/payloads with the retained C
   reference ADTS for the same 19 streams. The C files are existing
   `PHASE1_C_DIR` / `M55_C_DIR` artifacts; no new C encode was performed.
5. Extract the first payload and length divergence per stream and the
   frame-level trajectories for seven representative cases.
6. Parse the committed `qc6_*` vectors for the C grant/reservoir state and the
   onset state chain.

The scripts that produced these files were run from the repository root or
`rust/`; they are not part of the build. The table values are derived from the
committed vectors plus the retained C ADTS artifacts.

## Files

| File | Contents |
|---|---|
| `vbr5-invariants.tsv` | One row per stream: frame count, bad pass count, crash count, max gain adjustments, minimum grant/maxDyn/frame/cap margins, fill statistics, Rust byte count. |
| `frame-divergence.tsv` | One row per stream: frame counts, equal-length count, first payload-difference frame, first length-difference frame, sizes at that frame, maximum absolute length delta, and total byte delta. |
| `frame-lengths-focused.tsv` | Frame-level C/R length and live QC state for seven representative streams: onset, stable noise, speech-clean, tonal noise, sine, click, and Eroica. |
| `s6-vectors.tsv` | The 20 committed `qc6` C vectors: bitrate/padding fields, grants, available bits, element grant, PE values, reservoir level, static/used/total bits, reservoir total, pass count, crash marker. |
| `onset-state-chain.tsv` | C `S6GSTATE`/`S6GPOST` pre/post grant state for onset f22–f26. |
| `phase1-summary.txt` | Exact Phase-1 and M55 full-stream parity counts, with the artifact-dependent caveat. |
| `m5g-summary.txt` | Live 2,442-frame census global minima and invariant list. |

## Field notes

### `vbr5-invariants.tsv`

- `frames`: emitted frames, equal on C and Rust for all 19 streams.
- `npass_ne1`: frames whose outer QC pass count was not 1.
- `crash_frames`: frames with a non-`-1` crash marker.
- `max_iter`: maximum inner gain-adjust count over the stream.
- `min_grant`: minimum `elementGrant - usedDyn`.
- `min_maxdyn`: minimum `maxDyn - usedDyn`.
- `min_frame`: minimum `totalAvail - sumBits` (includes the 56-bit ADTS
  header in `sumBits`).
- `min_cap`: minimum `nChannels*6144 - static - usedDyn`.
- `fill_nonzero`, `max_fill`: frames with nonzero fill and the largest fill
  value. Only 0 and 7 occur.
- `bytes`: Rust ADTS bytes for the stream.

### `frame-divergence.tsv`

- `R_frames`, `C_frames`: parsed ADTS frame counts.
- `exact_frames`: frames whose complete ADTS bytes are identical.
- `len_equal`: frames with equal ADTS byte length.
- `first_byte_diff_frame`: first frame whose complete ADTS bytes differ;
  `-` means all compared frames are byte-identical.
- `first_len_diff_frame`: first frame whose ADTS byte length differs.
- `R_len_at_first_len_diff`, `C_len_at_first_len_diff`: lengths at that
  frame; `-1` when there is no length difference.
- `max_abs_len_delta`: maximum absolute per-frame length difference.
- `byte_delta`: total Rust minus C stream bytes.

### `frame-lengths-focused.tsv`

- `C_len`, `R_len`: ADTS bytes including the 7-byte header.
- `byte_diff`: 1 if the complete frame bytes differ.
- `seq`: Rust live window sequence (0 LONG, 1 START, 2 SHORT, 3 STOP).
- `used_dyn`, `grant`, `static`, `total`: live Rust QC values.
- `iter`, `npass`, `crash`: live gain-loop state.

### `s6-vectors.tsv`

The field names mirror the committed `S6BITR`, `S6GRANT` and `S6FINAL`
lines. `bitResLevelEl`/`maxBitResBitsEl` are `BitResRedistribution` outputs.
`bitResTot` is the post-frame reservoir total.

`qc6_onset-stereo_f24` is the stale oracle SHORT vector documented in the
D-021 archive. It is included because the Rust S6 test pins the ported formula
on the committed vector; its `totalBits` is not treated as real-C ground truth.

## Key results

- 2,442 frames; 0 frames with `npass != 1`; 0 crash frames; max gain
  adjustments 0.
- Minimum margins: grant 986, maxDyn 1,042, frame 984, cap 1,045 bits, all at
  `m3-noise-mono-48k f1`.
- Every `qc6` vector has `bitResLevelEl = maxBitResBitsEl = 0`, a pinned
  `bitResTot`, and `npass = 1`.
- 408/2,442 frames are byte-identical; 922/2,442 have equal length.
- First payload difference precedes or equals first length difference in every
  stream. No length-only divergence was found.

## Reproduction

From `rust/`:

```sh
cargo test --locked --features aac_enc --lib aac_enc::rate::tests
cargo test --locked --features aac_enc --lib m5g_branch_census -- --nocapture
cargo test --locked --features aac_enc --lib aac_enc::stream_driver::m52_tests::m52_all_fixtures_write_complete -- --nocapture
cargo test --locked --features aac_enc --lib aac_enc::stream_driver::m52_tests::m55_streams_match_c_reference -- --nocapture
```

The `m55` command needs `M55_C_DIR` for a self-contained run; otherwise it
uses the hard-coded fallback directory on this machine. The full Phase-1
comparison command is in the audit report §9.

## Limitations

- The evidence is VBR-5 only (`bitResMode = FULL`), not CBR or reduced/
  disabled reservoir modes.
- The corpus is the committed 19-stream inventory; it is not a proof for
  arbitrary signals.
- The licensed M55 fixtures have C ADTS but no S1/S4/S5 oracle dumps, so their
  exact upstream stage is not localized. Their rate-control exclusion rests on
  exact initialization, non-binding QC margins, and the source-level inertness
  of the VBR state.
- No new C capture, fixture, or reference artifact was created.
