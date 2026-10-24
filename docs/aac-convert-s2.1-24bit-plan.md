# S2.1 — 24-bit FLAC Acceptance Plan (Design Only)

> Maintained policy: the 24-bit truncation behavior described in §§2 and 5 is implemented. The plan framing and open questions below record the earlier planning stage.

- Baseline: `main` plus M1/M1.1 work. No implementation is authorized by
  this document.
- Authority: the S2 spike report (decoder/encoder capabilities, candidate
  strategies, test outline). This plan adopts the spike's recommended
  strategy — converter-only truncation, no rounding, no dithering, no
  encoder-API change — and turns it into reviewable steps.
- Status: **plan only. Every `Proposed` item needs an owner note before it
  becomes a requirement.** `Open decision` items must not be implemented on
  assumption.

## 0. Spike facts reused (not re-proven here)

- Symphonia 0.6.1 FLAC decoder outputs `AudioBuffer<i32>` with samples
  **left-justified** (`sample << (32 - bps)`; `symphonia-bundle-flac`
  `decoder.rs:239-241`). 24-bit `v` is held as `v << 8`, exactly.
- Bit depth is reliable: mandatory STREAMINFO field
  (`with_bits_per_sample`, `decoder.rs:114`; read in
  `rust/convert/src/flac.rs:63-70`). MD5 validation runs inside the decoder
  on the i32 buffer with the true bps (`decoder.rs:233`) — unaffected by any
  converter-side reduction.
- Current `next_pcm` uses plain `FromSample` (`buf.rs:469-476`, no dither);
  `i32 → i16` is `(s >> 16) as i16` (`conv.rs:527`): arithmetic shift,
  truncation (floor for negatives), no clamp needed. The existing path
  therefore already reduces left-justified 24-bit samples to their top 16
  bits correctly; the only gate is `AudioProperties::validate`
  (`rust/convert/src/lib.rs:283-296`), which deliberately rejects
  `bits_per_sample != 16`.
- Encoder input is `&[i16]` only (`Encoder::push`, `encoder.rs:292`) scaled
  exactly to Q31 (`(a as i32) << 16`, `q31.rs:133-134`). No wider/float path
  exists; none is proposed.

## 1. Proposed behavior

- 16-bit FLAC converts exactly as today. No change to its path, output, or
  diagnostics.
- `Proposed:` accept 24-bit FLAC through the **unchanged** decode → truncate
  → encode path. The product disclosure is explicit: output PCM fed to the
  encoder is 16-bit; the low 8 bits of source precision are discarded.
- `Proposed:` emit one warning per affected input file:
  `warning: <input>: 24-bit input converted to 16-bit PCM; low-order precision discarded`
  (exact wording: Open decision §6.1). Once per file **including batch
  conversion** — routed through the existing per-file `TagDiagnostics.warnings`
  → stderr loops (`main.rs:304-306` single-file, `main.rs:392-394` batch),
  never per PCM chunk (conversion runs `convert_inner` once per file).
  Dry runs emit no warning (nothing is decoded or converted, consistent with
  tag warnings).
- Bit depths other than 16/24 (8, 32, and anything else Symphonia reports)
  stay rejected with `UnsupportedBitDepth`, unless separately approved.
- Sample-rate (44.1/48 kHz), channel (mono/stereo), output-format, gapless
  `iTunSMPB`, `M4A` brands, no-overwrite publication, exit codes, and summary
  shapes are unchanged. `--no-tags`, dry-run, and skip-existing semantics
  are orthogonal and unchanged.

## 2. Conversion arithmetic (frozen facts + test obligations)

- Representation: 24-bit sample `v ∈ [-2²³, 2²³)` arrives as i32 `v << 8`.
- Reduction: `(v << 8) >> 16 == v >> 8` (arithmetic), i.e. the top 16 bits;
  identical to what `next_pcm` already computes. No new arithmetic code.
- Boundary obligations the tests must pin (all derivable, none measured
  end-to-end yet):
  - Full-scale positive `0x7FFFFF` → `0x7FFF` exactly; full-scale negative
    `-0x800000` → i32 `i32::MIN` → `-0x8000` exactly. No clipping possible:
    the shift always fits.
  - Negative samples: floor semantics (`-1₂₄ → -1₁₆`); assert, don't assume
    symmetry.
  - Error bound: `|e| ≤ 1` LSB₁₆ per sample, mean bias ≈ −0.5 LSB
    (≈ −102 dBFS DC). Document the bound in code and README; never present
    it as a listening-test result.
- `Proposed:` no rounding, no dithering in this scope (per S2 evidence:
  AAC coding noise dominates; added complexity buys nothing audible).
- Do not claim any measured end-to-end AAC impact — §4 defines the
  experiment that would establish it; until run, impact statements stay in
  the proven PCM-error-bound form only.

## 3. Implementation sequence (small, reviewable steps)

**Step 1 — Gate + disclosure plumbing.**
- `rust/convert/src/lib.rs`: relax `AudioProperties::validate` to accept
  `16 | 24` (keep rejecting everything else with `UnsupportedBitDepth`);
  rewrite the stale "reject rather than silently truncate" comment into the
  approved disclosure. Extend `convert_inner` (`lib.rs:390`) to push the
  §1 warning into the returned `TagDiagnostics.warnings` when
  `properties.bits_per_sample == 24` (verified location: diagnostics are
  already threaded through `convert_file_with_format_and_tags`,
  `execute_plan_with_tags`, and both `main.rs` warning loops — one new
  push site covers single-file and batch).
- `rust/convert/src/flac.rs` tests: invert `rejects_24_bit_input` into an
  acceptance test (open succeeds, `properties.bits_per_sample == 24`,
  full decode + `verify()` green on `stereo-48k-24.flac`).
- No `m4a.rs`/`gapless.rs`/encoder/decoder changes.

**Step 2 — Failure-path test retargeting.**
- Tests currently using `stereo-48k-24.flac` as a *generic* failure fixture
  must move to still-unsupported inputs (e.g. `mono-32k-16.flac`,
  `three-channel-48k-16.flac`) so failure-path coverage is preserved:
  `tests/conversion.rs:80-81`, `tests/batch.rs:367,783,914,1123,1311`,
  `tests/cli.rs:56`, plus the `batch.rs:976` category-table entry (keep the
  `UnsupportedBitDepth(24)` unit case — other depths still produce it).
- Exact expectation changes belong to the implementation diff, not this
  plan; the rule is: no test may assert 24-bit failure after this change.

**Step 3 — New deterministic fixtures (small, generated, committed).**
- Via the existing `generate.py` 24-bit path (already supports
  `setsampwidth(3)`): a **matched-content pair** — the same synthetic
  signal rendered at 16-bit and 24-bit — plus boundary vectors (full-scale
  sine, −90 dBFS tone, silence, DC offset, per-channel impulse) at
  44.1/48 kHz × mono/stereo within the supported lattice. Each file stays
  KB-scale (12k frames precedent). Document provenance in
  `tests/fixtures/README.md` (extend the `generate.py` section; note the
  existing `stereo-48k-24.flac` row flips from "rejected" to "accepted with
  warning").
- `Proposed:` no 8/32-bit fixtures unless owners want explicit rejection
  tests for them; `validate()` logic for those depths is covered by the
  retargeted tests in Step 2.

**Step 4 — Conversion/mapping tests.**
- PCM-level (exact, pre-encode): decode the matched 24-bit fixture through
  the `FlacSource` path; assert per-sample `|out − (ref >> 8)| == 0`
  against the 16-bit mate where content matches, full-scale exactness,
  channel order, DC preservation within bound, silence stays silence.
- End-to-end: 24-bit converts successfully to ADTS and M4A; gapless,
  tags, brands, and publication behavior identical in shape to 16-bit runs;
  warning asserted once on stderr in single-file and batch CLI tests;
  dry-run asserts no warning and no output.
- Docs: `rust/convert/README.md` (scope-limits + a 24-bit disclosure
  subsection), `USAGE`/help text only if the disclosure belongs there
  (`Proposed:` README + rustdoc, no `--help` change — no flags change).

**Step 5 — Validation runs (§4).** Full suite green, fmt, clippy (no new
  lints), encoder gates with a provably empty `rust/src/aac_enc` diff.

## 4. Validation

Required (all must pass):
- `cargo test --locked -p aac-convert` from `rust/`: full suite green,
  including inverted 24-bit acceptance tests, retargeted failure-path
  tests, new PCM-boundary tests, batch/`--no-tags`/dry-run/ADTS+M4A warning
  tests, and unchanged 16-bit behavior tests.
- `cargo fmt -p aac-convert -- --check` clean; `cargo clippy --locked
  -p aac-convert --all-targets` introduces no new lints.
- FLAC source MD5 validation still enforced (`verify()` path; corrupt-24-bit
  fixture or `md5-mismatch` analog must still fail without publishing).
- **Objective encoded-output comparison:** convert the matched 16-bit and
  24-bit mates; (a) assert PCM-before-encode difference is exactly the
  bounded truncation from §2; (b) compare AAC-decoded outputs differentially
  — the 16-vs-24 residual must be small against the AAC-vs-source residual
  control, separating converter-induced error from ordinary lossy coding
  error. ADTS byte-identity between the two runs is *not* required (lossy
  encoder need not be bit-stable across LSB differences) — state this in
  the test comments.
- Existing independent-decoder checks where available (same `#[ignore]` +
  tool-gate pattern as M1); existing encoder regression gates apply
  unchanged, with an explicit empty-diff assertion on `rust/src/aac_enc`.
- No ABX or listening tests required.

## 5. Acceptance criteria and non-goals

Acceptance: 24-bit converts with exactly one documented warning per file;
16-bit behavior byte-identical in procedure and output; non-16/24 depths
still rejected with `UnsupportedBitDepth`; full suite + fmt + clippy green;
no encoder/decoder internal changes; no new dependencies; docs updated.
Non-goals: wider encoder input API; rounding or dithering; resampling or
downmixing; VBR 1–4; cover art (S1.1, untouched); new dependencies;
unrelated refactoring.

## 6. Unresolved questions

1. Exact warning wording (proposed in §1; needs owner sign-off).
2. Whether 8/32-bit rejection deserves dedicated fixtures or continued
   unit-level coverage only.
3. **Evidence gap (explicit):** the arithmetic (§2) is established by code
   inspection; the *end-to-end perceptual/encoded-output impact* is
   unmeasured until §4's comparison experiment actually runs. No approval
   should cite a measured impact that does not exist yet.
4. Whether the truncation disclosure belongs in `--help` text in addition
   to README/rustdoc (proposed: no).
5. Interaction with any future S1.1 cover-art diagnostics shape (independent
   workstreams; both extend `TagDiagnostics.warnings` today, no conflict).
