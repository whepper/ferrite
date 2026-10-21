# M5.d — Flush, Termination and Skip-Policy Validation

Validation only. No encoder changes, no tuning, no API work, no listening tests.

**Verdict: `PASS WITH FINDINGS`**

- All five M5.c acceptance criteria are met as written; no criterion fails or is blocked.
- Findings (none blocking): F1 — no per-frame loud gate exists for the large-fill class (unobservable as a trigger; values pass through); F2 — mono-44.1k remains blocked (prior status, unchanged); F3 — the M55 default C-reference path holds only 1 of 4 files (worked around with a fresh, fully documented directory).

## 0. Baseline and provenance (Phase 0)

- Branch `main`, HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (recorded before and after; unchanged).
- Initial `git status --short`: 31 lines (11 modified implementation files + M5.0/M5.a/M5.b/M5.c reports + `phase1b.rs`, `stream_driver.rs`, `m5a_decode.rs`, `m5b_measure.rs`, 5 testdata WAV/provenance files, `tools/c-enc-bench/`, `tools/enc-oracle/__pycache__/`). Unstaged diff 11 files +1571/−280; index empty. Nothing staged, committed, stashed, reset, cleaned, restored, or amended at any point.
- M5.c recommendation verified (`docs/aac-enc-m5c-validation-gap-assessment.md` §6): bounded flush/termination + skip-policy validation on the three measured fixtures, criteria (1) final-frame reconciliation, (2) trigger inventory over 19 streams with zero unexplained events, (3) M5.b numbers unchanged, (4) M5.a gates on all decoded outputs including final frames.
- Deliverable `docs/aac-enc-m5d-flush-termination-validation.md` verified absent before writing.
- **Terminology correction applied throughout:** M5.c specifies a trigger inventory over **19 streams** (15 M52 + 4 M55) — it does not define "19 triggers", and there is no 19-item trigger list in M5.c. The M5.d brief's "19 defined triggers / 19-trigger inventory / target count" phrasing misstates M5.c. Per the brief's own stop-and-document rule, no 19-trigger list was invented. What M5.c defines is applied exactly: enumerate trigger-class events over the 19 streams using the pipeline's own code-defined loud-finding taxonomy (§4), and require zero unexplained observed events. Trigger *classes* come from code (§1.5); the *streams* come from M5.c.

## 1. Lifecycle data flow (Phase 1; all paths read, not modified)

- **Input:** `PcmFeeder::push_interleaved` (`rust/src/aac_enc/pcm.rs:241-255`) buffers channel-aligned i16; ragged input is rejected (`FeederError::UnalignedInput`, `:245-250`), never truncated. Single-use lifecycle: any number of pushes, exactly one `finish`; reuse is `AlreadyFinished` (`:159-161,261-264`).
- **End-of-input / flush:** `PcmFeeder::finish` (`pcm.rs:257-283`) emits the padded partial frame (if any) plus all-zero frames until appended zeros reach the 1600-sample encoder delay (`:115-120,267`), mirroring the C `-1` flush loop (`aacenc_lib.cpp:1091-1115`, cited at `:259`). Zero padding is **appended at the end** — nothing is prepended. Frame law: `F = ceil((S+1600)/1024)` (`:118-120`), verified against the reference (48000 stereo → 49, 9600 mono → 11, `:121`).
- **Per-frame encode:** `StreamEncoder::step_frame` (`rust/src/aac_enc/stream_driver.rs:801-826`) runs the 11-stage chain documented in the module header (`:28-56`): block-switch + sync → MDCT transform → psy/TNS → MS mirror → S1/static → S2 adapt → S4 scf → S5 bitcount → S6 QC (single-pass expected) → S8 + ADTS. Returns `(Option<ADTS>, FrameSnapshot)`; `None` = skipped with a loud note, never silent (`:798-800`).
- **Delay/overlap accounting (implementation, not harness assumption):** encoder delay is 1600 samples/ch (`pcm.rs:213`, `config.encoder_delay()`); "the first 1600 output samples are filterbank/look-ahead priming" (`pcm.rs:122-124`). Overlap state lives in the block-switch/MDCT frontend (`BlockSwitch::process`, `ChannelTransform::process`); SHORT/long transition decisions are exact vs oracle (`block_switch.rs:600` per M5.0). No skip-sample metadata exists anywhere in the Rust path or the ADTS bitstream — ADTS carries no delay signaling.
- **Termination/decoded-count relation:** ADTS has no end marker; termination = stream end. `aac-dec` consumes every ADTS frame, then trims `outputDelay × channels` samples before writing the WAV (`aac-dec.c:117-129`); pre-trim (granted) counts are therefore **derived** (`F·1024`, decoder consumed all frames cleanly), not independently observed — no claim beyond that is made. Post-trim counts are measured WAV facts.
- **Harness assumptions (kept explicit):** frame-count law (`m52` test asserts `(S+1600+1023)/1024` per fixture); M5.a plausibility window `0 < D′ < 4096`; M5.b nominal lag −1600 with alias analysis. None were changed for this milestone.

### Trigger taxonomy (code-defined; M5.c's "trigger classes")

| Class | Code source | Pipeline behavior |
|---|---|---|
| SKIP-A frontend structural mismatch (geometry/spec/energy_ld length) | `stream_driver.rs:736-760` → `Err(note)` → `:824-826` | frame skipped (`None`), run loud-fails at `m52_emit_or_fail` |
| SKIP-B QC error | `:1203-1204` | frame skipped |
| SKIP-C multi-pass gain loop (`npass≠1`) | `:1219-1223` | frame skipped |
| NOTE-EMIT crash recovery (`crash≠-1`) | `:1225-1231` (note, **emits**) → `m52_emit_or_fail` `:4012` | bytes emitted but run loud-fails on "notes on emitted frame" |
| SKIP-D low-overlap window shape | `:1264-1272` | frame skipped |
| SKIP-E element-write / static-agreement / raw-block / ADTS errors | `:1317-1374` | frame skipped |
| PANIC envelope violations (S4 search, gain loop) | `:1098-1103,1196-1199` (no `catch_unwind` by design) | run aborts (test failure), never a silent skip |
| CONSTR-IS / CONSTR-PNS (nonzero books) | zero-forced at call sites (`:968,1140,1170,1298,1339`; `NO_NOISE_PNS`, `:36,1116`); IS inactive proven corpus-wide by oracle dumps (header `:33-34`) | cannot fire through this path; absence follows from construction |
| FILL large-fill | **no per-frame loud gate located in the driver** (tot_fill recorded `:1212`, passed through `:1356`); S8 fill-sizing covered only by unit test (`frame.rs:1356` per M5.0) | **unobservable as a trigger with current instrumentation (finding F1)** |

## 2. Per-fixture validation (Phase 2; 3 M5.b fixtures, no new inputs)

Method note: the first inline-loop attempt at C-encode/decode crossed two output filenames (shell-quoting bug); the size check caught it immediately, the files were deleted, and the run was redone cleanly via the auditable script `/tmp/m5d-run.sh`. Only the clean run is reported below.

| Fixture | S (in/ch) | Zeros appended Z | F (frames) | Granted G=F·1024 | N decoded | D′=G−N | Priming | Overlap (orig covered) | Orig tail unrepresented |
|---|---|---|---|---|---|---|---|---|---|
| click-mono 48k | 24000 | 1600 | 25 | 25600 | 23856 | 1744 | 1600 | [0:22256] | [22256:24000] = 1744 |
| onset-stereo 48k | 48000 | 2176 | 49 | 50176 | 48432 | 1744 | 1600 | [0:46832] | [46832:48000] = 1168 |
| sine-44100 stereo | 44100 | 1980 | 45 | 46080 | 44395 | 1685 | 1600 | [0:42795] | [42795:44100] = 1305 |

Reconciliation rule derived from the implementation (feeder code + delay constants + decoder trim), verified exact on all three rows: **unrepresented orig tail = (1600 + D) − Z**, where 1600 = encoder delay (visible output priming), D = decoder outputDelay trim (1744 @48k, 1685 @44.1k), Z = flush zeros appended (≥1600, overshoots to the frame boundary). Check: click 3344−1600=1744 ✓; onset 3344−2176=1168 ✓; sine 3285−1980=1305 ✓. No unexplained sample loss or duplication: every granted sample is accounted for as priming, represented content, decoder trim, or delay-line remainder still unflushed at stream end (larger Z → smaller tail loss; exact-fit click loses exactly D′). Rust and C paths share identical F, G, N per fixture (C ADTS re-encoded fresh: hashes `269a47aa…`, `c8adaeb7…`, `54ef50de…`, all matching M5.b records).

Final-3-frames rust-vs-C (ADTS parse; `/tmp/m5d-frames.py`):

| Fixture | f(n−3) lens R/C, payload = | f(n−2) lens R/C, payload = | f(n−1) lens R/C, payload = |
|---|---|---|---|
| click (f22–24) | 55/56 differ | 49/49 differ | 320/321 differ |
| onset (f46–48) | 51/51 differ | 220/216 differ | 219/219 differ |
| sine (f43–44… last 2 shown; f42 46/46 **equal**) | 46/46 **equal** | 230/236 differ | 201/203 differ |

Frame counts equal everywhere (25/49/45 both encoders); byte-equal frames overall: 2/25, 26/49, 17/45 (residue-class divergence, D-021 account — expected, not a failure). Final-frame payloads differ on all three, bounded as follows: the same finite-precision MDCT residue that M5.b measures as 53–63 dB rust-vs-c full-stream SNR flips SCF/Huffman decisions on these tail frames; decoded-tail comparison (last 4096 samples/ch, lag-0, apples-to-apples since both decoders trim identically) bounds the audible-relevant effect — err RMS 0.46/4.14/1.63 LSB, max abs err 1/23/14 LSB, SNR 81.8/64.6/72.7 dB. One exact final-region match exists (sine f42, payload-identical), confirming the comparison is sensitive enough to register equality where it occurs. Per-frame stage attribution for the differing tails (S4 vs S5 split) is unavailable — no committed per-stage ground truth exists for final frames (known M5.0 gap) — so the mechanism is named at the residue class level, not the stage level. That is the documented bound, not a gap in the argument.

Decoder integrity (6 fresh decodes, established gates): exit 0 **and** empty stderr on all 6; all WAVs `RIFF/WAVE/fmt /data`, `data_len == file−44`, frame-aligned, ch/sr as configured (1/2 ch, 48/44.1 kHz), 16-bit; counts plausible per the table above. No warnings, truncation, or anomalies.

Determinism: Rust ADTS re-encoded today hashes bit-identical to M5.a/M5.b records (`a9d7b395…`, `702c8717…`, `aa906b52…`); all 6 decoded WAVs hash-identical to M5.b records (`6a359695…`, `4d729444…`, `aecb7b97…` rust; `194c3b3c…`, `25946466…`, `44c06ba6…` C). C encoder re-ran byte-identical. M5.b metrics re-run (`/tmp/m5b-metrics.py`) reproduces `/tmp/m5b-metrics.json` **byte-for-byte** — every M5.b §2–§4 number stands unchanged at full precision, not within a tolerance.

## 3. Trigger inventory over 19 streams (Phase 3; M5.c as written)

Streams: 15 M52 resynthesis fixtures + 4 M55 licensed fixtures (`m7-music/speech-clean/speech-pauses-48k`, `m8-music-eroica-48k` WAVs, committed in-repo). Runs executed for this milestone:

- `cargo test --features aac_enc m52_all_fixtures_write_complete` (from `rust/`): **ok** — all 15 emit complete ADTS, zero notes/skips (any trigger fails loudly via `m52_emit_or_fail`). Spot frame law: 9600→11, 12000→14, 24000→25, 48000/44100→49/45, all `ceil((S+1600)/1024)` ✓. Bytes reproduce M5.a/M5.b/M5.25 records where overlapping (silence 735, impulse 306, edge 415, onset 2359, click 2680, sine441 2998).
- Licensed C references generated fresh with the established `aac-enc` procedure into `$TMPDIR/m5d-c55/` (hashes recorded in the run log), because the M55 default external dir holds only 1 of 4 files (**finding F3**); `M55_C_DIR` pointed at the fresh dir. This changes nothing about trigger evidence (the C bytes are used only for accounting/first_diff prints).
- `M55_C_DIR=… cargo test --features aac_enc m55_streams_match_c_reference`: **ok** — 4/4 emit, zero notes/skips; frames 471 = 471 rust-vs-C on all four; `first_diff` at byte 19/64/21/20 (first-payload residue class, expected); `len_exact_frames` 145/161/124/93.

Classification per class: SKIP-A/B/C/D/E — **not observed** on any of 19 streams (a single occurrence fails the run); NOTE-EMIT crash — **not observed**; PANIC envelope — **not observed** (no abort); CONSTR-IS/PNS — **not observed, and unobservable as live events** (construction-zeroed; absence follows from construction + oracle-corpus proof, stated as a limitation not a verification); FILL — **unobservable** (finding F1: no per-frame gate; S8 unit-test coverage only). Zero observed events, zero unexplained — the M5.c criterion is satisfied as written; no trigger was manufactured and the stream count was not altered.

## 4. Acceptance criteria (M5.c, unrelaxed)

1. **Frame/sample reconciliation — `PASS`.** All three fixtures reconcile exactly to the implementation-derived rule `tail = (1600+D)−Z`; final-frame payload differences bounded (decoded-tail SNR 65–82 dB, max 23 LSB) under the named D-021 residue mechanism; counts exact.
2. **Decoder integrity — `PASS`.** 6/6 fresh decodes meet the M5.a gates (exit 0, empty stderr, structure, ch/sr, plausible counts).
3. **Trigger inventory — `PASS`.** 19/19 streams enumerated (15 M52 + 4 M55); taxonomy in §1; zero observed, zero unexplained. Corrections/disclosures: "19 triggers" corrected to "19 streams" (§0); FILL class unobservable (F1); mono-44100 cell still blocked (F2, prior status).
4. **Baseline stability — `PASS`.** M5.b metrics JSON byte-identical on re-run; all hashes reproduced; no tolerances invoked.
5. **Determinism — `PASS`.** Encoder, C encoder, and decoder all byte-reproducible across runs/days (three independent replications in this milestone alone).

## 5. Residual uncertainties (no unsupported causal claims)

- Per-frame stage attribution for final-frame payload diffs (S4-vs-S5 split) remains unavailable; the bound rests on decoded-tail error, which is the audible-relevant quantity but does not localize the decision flip. Localizing it would require temporary per-stage probes of the kind D-021 used — explicitly out of scope.
- FILL-class behavior has no per-frame loud gate; current evidence is unit-test-level only.
- Trimmed regions (front priming, delay-line remainder tails) are excluded from error metrics by construction; flush correctness *of the bitstream* is validated (counts, framing, decodability), not sample-exact tail recovery (impossible by design — the delay line cannot fully flush).
- Only 3 short synthetic fixtures plus accounting-level licensed runs; real-content signal metrics remain a `VALIDATION GAP` (M5.c), untouched by this milestone.

## 6. Recommended next step (supported by results)

**Mono-44.1k infrastructure restore** — the smallest task that closes a standing gap: commit `cfg_44100_112000.txt` via the established oracle path, add the fixture key, promote the derived-mono recipe (`m5a_decode.rs:emit_pcm_as_adts`) into `CASES`. Pass criterion: the 4th cell meets the M5.a gates and reconciles under the §2 rule with D=1685. Why this before the branch-trigger deep audit: it is fully specified, touches no numerics, and completes the matrix the flush rule must then cover; the branch audit (PNS/IS/retry/crash/fill on real content, consuming the §3 inventory as its map) follows as the larger correctness task, then the production API slice (which must not bake in abort semantics — now documented as abort-loud everywhere, §1), then CI.

## 7. Final worktree and change confirmation

- HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91`, branch `main` — unchanged from §0.
- Final `git status --short`: 32 lines = the 31-line §0 baseline + this report. Unstaged diff still 11 files (implementation work untouched); index empty — nothing staged, committed, stashed, reset, cleaned, restored, amended, or pushed.
- Files changed by this milestone: exactly one — `docs/aac-enc-m5d-flush-termination-validation.md` (verified absent in §0). No production code, test, fixture, doc, or config file modified; `m5a_decode.rs`/`m5b_measure.rs` untouched; `aac-enc.c`/`aac-dec.c` untouched. Test runs wrote only to `$TMPDIR/m52-rust-aac/`, `$TMPDIR/m5a-decode/`, `$TMPDIR/m5b-quality/`, `$TMPDIR/m5d/`, `$TMPDIR/m5d-c55/`; analysis scripts live in `/tmp/m5d-run.sh`, `/tmp/m5d-frames.py`, `/tmp/m5b-metrics.py`; build trees in `/tmp/m5a-build/`. No repo artifact added, removed, or overwritten. The M55 default external C dir was read (1 file) but never written.
