# MDCT f32/f64 experiment evidence

Measurement-only evidence for
[`docs/aac-enc-mdct-f64-precision-experiment.md`](../../aac-enc-mdct-f64-precision-experiment.md).
These files are retained so the experiment's conclusions can be reviewed
without the multi-hundred-megabyte oracle dumps and generated ADTS/WAV
artifacts. They are evidence, not normative guidance. The f64 implementation
and its feature/test harness were removed at closeout; only this evidence,
the experiment report, and the production f32 regression test remain.

## Files

| File | Contents |
|---|---|
| `downstream-comparison.tsv` | Per-fixture f32/f64 encode comparison: frames, bytes, equal frame lengths, Q31 difference counts, TNS differences. |
| `downstream-summary.txt` | Aggregate frame/byte/Q31/psy/decision counts. |
| `oracle-transform-comparison.tsv` | Per-fixture f32 and f64 MDCT comparison against the FDK `enc_oracle` dumps. |
| `decoded-quality-comparison.md` | Full-band and 4096-window SNR, f64-vs-f32 decoded waveform difference, C comparison, clipping. |
| `performance.txt` | Encoder-only timing and RSS, hardware/toolchain, and method. |
| `block-switch-finding.txt` | Post-experiment finding: C START/SHORT/STOP vs Rust LONG on `m7-speech-clean-stereo-48k` frames 107–109. |

## Provenance

- Date: 2026-10-27.
- Base commit at experiment start: `0abf11b`.
- Worktree HEAD during the final full-suite runs:
  `e2ab63026e9dd5feb96c61ac908e18f9e85e7520` plus unrelated concurrent
  working-tree changes.
- Toolchain: `rustc`/`cargo` 1.99.1, Apple M5, macOS 27.0.1.
- Reference: pinned FDK AAC source `f14ad40a574a4132df947d15e9fb14bee91cc8a5`;
  `aac-enc` SHA-256 `013e8acf2571486a008c0ece538489e36d7e796b84a6f927418e66fb2d15e5f5`;
  `aac-dec` SHA-256 `f361ba89d372cb1b1a9175e673e6969ac6fc632428202cab02a31b3e3a21c0b8`.
- Corpus: the repository's 19-fixture V3 corpus (`tools/aac-enc-metrics/fixtures-v3.json`).
- Commands and the full evidence-directory map are in §4 and §12 of the
  experiment report.

The generated ADTS streams, decoded WAVs, and full oracle dumps remain in
the temporary experiment directory
`/private/var/folders/*/T/opencode/f64-mdct-experiment/`
and are not committed. The retained tables above are derived directly from
those artifacts.

## Key recorded result

The f64 transform is arithmetically more accurate against an ideal MDCT
(1.199e-7 vs 3.757e-7 worst relative), but not against FDK in absolute/RMS
terms (worst absolute 1.035e-2 vs 1.034e-2; RMS 1.675e-4 vs 1.674e-4).
Decoded quality changed by at most ±0.003 dB full-band SNR. The production
recommendation is to retain f32.
