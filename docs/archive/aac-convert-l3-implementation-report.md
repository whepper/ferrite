# L3 Implementation Report — Native Loudness Tagging

- Date: 2026-10-24 (UTC).
- Baseline: `main` @ `7ebf208d40d78adcd265e3736e93f8a8aa93e8d1`.
- Authority: `docs/aac-convert-l0-loudness-decisions.md` (L0 D1–D7),
  `docs/aac-convert-l2-loudness-integration-design.md` (finalized D1–D5,
  §§1–9), `docs/archive/aac-convert-l3-prep-report.md` (six gates: all PASS).
- Authorization: explicit owner approval of L3 (this task).
- Scope: production implementation of native ReplayGain 2.0 + `iTunNORM`
  tagging. No VBR, encoder, packaging, or unrelated work.

## 1. Architecture and data flow

New module `rust/convert/src/loudness.rs` plus thin plumbing in `lib.rs`
and `batch.rs`. No public API, CLI, summary, exit-code, or diagnostic
category changes.

- **Analysis** (`loudness::analyze_album` / `analyze_single`): groups members
  into albums by source-parent directory (`relative_input.parent()`; root
  files form album `""`). Each member is stream-decoded once through its own
  `ebur128` instance (`I | SAMPLE_PEAK | HISTOGRAM`, validated channel map)
  while feeding a shared album instance with the same PCM (O(1) memory, one
  packet at a time). The analyzer seeds from the first analyzable member; a
  later member with a different rate/channel layout poisons the album (never
  subset-as-complete) while keeping its own track measurement.
- **Gains**: `−18.0 − integrated` per track and per album; Easy-mode-equivalent
  positive-only clipping protection applied independently (track vs track peak,
  album vs album peak), adjusted gains never below `+0.00 dB`, peaks verbatim,
  AAC samples untouched. `iTunNORM` derives from the clipping-adjusted track
  gain through the verified pipeline (2-decimal quantized input, banker's
  rounding, per-slot `[0, 0xFFFFFFFE]` clamp, exact 10-field layout).
- **Wiring**: `batch::execute_plan_with_tags` runs the pre-pass for M4A +
  tags + real runs only (skipped for ADTS, dry-run, `--no-tags`), then each
  conversion receives its precomputed `FileLoudness` (atoms + warnings)
  through the new private `convert_file_with_format_tags_and_loudness` →
  `convert_inner(..., loudness: Option<&FileLoudness>)`. Single-file
  conversion is the degenerate one-member album (`analyze_single`). Atoms
  extend `tag_ilst` after text/`covr` (`iTunSMPB → text → covr → loudness`);
  warnings append after tag/cover warnings (after the 24-bit notice).
- **Failure semantics**: analysis is infallible at the top level — any
  per-member problem (unreadable/unsupported/undecodable source, analyzer
  error, silence) yields empty atoms plus warnings, never a conversion
  failure. Album-level atoms are omitted unless every member joined with a
  finite integrated value; one album-level warning names the unanalyzable
  members and is attached to every member so the conversion pass emits it
  for each file concerned. Genuine conversion failures (e.g. `DecodeFailed`)
  are unchanged in category, reporting, and exit behavior.

## 2. Changed files

| File | Purpose |
|---|---|
| `rust/convert/Cargo.toml` | Add `ebur128 = "=0.1.10"` (pinned, with vetting comment) |
| `rust/Cargo.lock` | Resolve `ebur128 0.1.10` (+ `bitflags 1`/`dasp_*`/`smallvec`; checksum matches the vetted crate) |
| `rust/convert/src/loudness.rs` | **New.** Analysis, album grouping, clipping, `iTunNORM`, atom rendering, album/incomplete logic, 13 unit tests |
| `rust/convert/src/lib.rs` | `convert_inner` takes `Option<&FileLoudness>`; public wrappers compute the degenerate one-member pass for M4A+tags; new `pub(crate)` batch entry point; debug assertions guard the M4A+tags-only invariant |
| `rust/convert/src/batch.rs` | Pre-pass over all planned sources in `execute_plan_with_tags` (M4A + real run + tags only); per-entry loudness lookup at conversion; doc updates |
| `rust/convert/tests/loudness.rs` | **New.** 15 end-to-end tests (see §6) |
| `rust/convert/tests/fixtures/generate.py` | 5 s sine/silence/clip-spike/mono synthesis (`sine_frames`, `clip_frames`) |
| `rust/convert/tests/fixtures/loud-*.flac` (6) | **New.** Repo-owned synthetic fixtures (no third-party audio); `loud-tagged` carries `title`/`artist` + front JPEG copied from the existing repo-owned tagged fixture via offline mutagen |
| `rust/convert/tests/fixtures/README.md` | Fixture table + regeneration/tagging procedure |
| `rust/convert/tests/coverart.rs`, `metadata.rs` | Exact-warning assertions extended with the specified loudness warnings (order pinned); `cover` substring assertions now exclude the `loudness:`-prefixed warnings that name fixture files |
| `rust/convert/README.md` | `## Loudness` section; text-atom bullet rescoped (freeform loudness atoms documented) |

L0 and L2 were not edited. No staging, commits, or pushes occurred.

## 3. Dependency and toolchain validation

- `ebur128 =0.1.10` added as the only new dependency; production graph is
  `ebur128 → bitflags 1 / dasp_frame+DASM 0.11 / smallvec` (MIT/Apache, no
  copyleft, no build-script C requirement on the default path).
- No-C-compiler production check (executed): `cargo clean -p aac-convert
  -p ebur128`, then `cargo build -p aac-convert --locked` with
  `cc`/`gcc`/`clang` replaced by failing shims on `PATH` (real linker via
  `RUSTFLAGS -C linker=...`): zero shim invocations, build succeeds.
- MSRV: the crate declares `rust-version = "1.99.1"` and the full suite below
  ran on exactly `rustc 1.99.1` — the MSRV run and the validation run are
  the same. (`ebur128` itself was proven to compile on exact 1.60 during
  L3-prep; the workspace MSRV remains 1.99.1 by project policy.)

## 4. Clipping and `iTunNORM` implementation

- `protect_gain` implements the verified behavior exactly (including the
  `+0.00 dB` cap via `.max(0.0)`); T-clip vectors pinned:
  `(−3.00, 1.0)→−3.00`, `(0.00, 0.9)→0.00`, `(+5.00, 0.5)→+5.00`,
  `(+5.00, 0.9)→0.915149811213503` (rendered `0.92 dB`),
  `(+6.00, 0.7)→3.098039199714864` (rendered `3.10 dB`).
- `round_half_even` (never `f64::round`); pinned `+30.00 dB → sc_2500 == 2`
  with the `+29.99 → 3` neighbor; clamps (`−70 → FFFFFFFE`, never
  `FFFFFFFF`; `+100 → 0`); exact layout string asserted byte-for-byte.
- `iTunNORM` input is the rendered 2-decimal gain parsed back, mirroring the
  original read-back pipeline; per-file warnings name measured vs written
  gains (e.g. `adjusted track gain from 8.71 dB to 0.92 dB`).

## 5. Album, retained, and incomplete behavior

- Grouping by source-parent bytes; per-file reporting order and summary
  shape unchanged (`mapped` stays a text-only 0–10 count).
- Complete albums: identical album gain/peak atoms on all members
  (measured: tone+quiet → both carry `3.47 dB` / `0.125000`); no loudness
  warnings when nothing adjusts and everything measures.
- Incomplete albums (silent member, unreadable retained source, layout
  mismatch — poison names the mismatching member in plan order): track atoms
  + `iTunNORM` still written for measurable members; album atoms omitted;
  per-file + album-level warnings emitted for each converted member.
- Retained outputs (`--skip-existing`): analyzed through readable sources,
  outputs byte-identical across reruns; missing sources mark the album
  incomplete. `--no-tags`/ADTS/dry-run skip analysis entirely (dry-run never
  decodes — verified with an undecodable fixture planning cleanly).

## 6. Test commands, results, and coverage

- `cargo test -p aac-convert` (rustc 1.99.1): **200 passed, 0 failed**
  (lib 68 incl. 13 new loudness unit tests; batch 47; cli 10; conversion 30;
  coverart 11; loudness 15 new; metadata 19; 4 pre-existing ignores).
- `cargo fmt -p aac-convert -- --check`: clean (workspace-wide `--all`
  shows only pre-existing `aac`-crate diffs from a newer rustfmt; untouched).
- `cargo clippy -p aac-convert --all-targets`: no warnings in new/changed
  code (remaining convert warnings are pre-existing lints from the newer
  toolchain; verified unrelated to this diff).
- `cargo build --workspace --locked`: succeeds.
- New `tests/loudness.rs` (15 tests): complete loudness set + exact order;
  quiet no-adjustment; clip adjustment with verbatim peak and adjusted
  `iTunNORM`; silence omission + warnings; short-fixture omission pin;
  mono lattice (`3.75 dB` / `0.124969`); tagged+cover coexistence and order
  (`iTunSMPB → ©nam/©ART → covr → 5 loudness`); two-track album sharing;
  incomplete album (silence) omission + naming; parent-directory grouping;
  retained byte-identity + skip tallies; missing retained source poisoning;
  `--no-tags`/ADTS/dry-run skips; genuine `DecodeFailed` preserved with no
  output published; mixed-layout album omission + naming.

## 7. Real-file validation

- Same-domain parity vs real `rsgain 3.8` on disposable fixture copies:
  tone `+0.75 dB` / `0.125000` exact; clip `+0.92 dB` adjusted / `0.899994`
  exact; quiet `+12.35 dB` vs rsgain `+12.34 dB` (0.01 dB HISTOGRAM-vs-queue
  mode delta, within the ±0.1 EBU tolerance; documented, not hidden).
- Byte-level check (isolated script, 7 converter-produced M4As incl. a
  two-track album): every `iTunNORM` payload equals the independently
  recomputed expectation from the file's own written gain string; all
  `----` atoms carry `mean=com.apple.iTunes`, `data` type 1, locale 0.
  **7/7 ALL-MATCH.**
- Pre-existing `iTunSMPB` preserved byte-identical; cover bytes unchanged;
  AAC payloads untouched by construction (analysis is read-only; tags append
  to `ilst` only).

## 8. Compatibility confirmation

- Public API (`convert_file*`, `execute_plan*`), CLI flags, summary shape,
  exit codes, `TagDiagnostics` shape, no-replace/hard-link publication, and
  warning mechanisms: unchanged — all pre-existing suites pass unmodified
  except exact-warning assertions that now pin the specified loudness
  warnings in the specified order.
- No new CLI flag, no new summary field, no new failure category, no encoder
  or container change beyond appended `ilst` children.

## 9. Limitations

- Mixed rate/channel layouts within one source directory cannot share an
  album value (single R128 state per album): album atoms omitted with a
  naming warning. Edge-case ruling consistent with L2 §7, documented here
  and pinned by test; no L2 change required.
- Short (< ~0.4 s gating-block) inputs are unmeasurable by construction and
  carry no loudness atoms (with warnings), including all pre-existing
  fixtures — pinned by test.
- The `+30.00 dB` rounding pin is theoretical (absurd gain) but enforced by
  unit test to protect the semantics.
- The full `aac` workspace suite was not run (converter scope only); the
  workspace builds clean and no `aac`-crate code was touched.

## 10. Final state

- HEAD `7ebf208` unchanged; 0 staged; 0 tracked modifications beyond the
  table in §2; all 14 pre-existing untracked planning docs intact; no
  experimental artifacts or restricted fixtures in the repo.
- This report (`docs/archive/aac-convert-l3-implementation-report.md`) is the only
  docs change besides the L3 code/tests/fixtures/README listed above.
