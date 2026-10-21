# Rust AAC-LC VBR-5 Encoder — M5.0 Readiness Audit

Audit and planning only. No fixes or features implemented.

## 1. Current revision and worktree status

- Repository root: `/Users/jeroen/Documents/VSCode/fdk-aac/fdk-aac`, branch `main`.
- HEAD: `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (`docs(research): preserve D-021 evidence and closure record`).
- Prior checkpoint referenced by earlier work: `e4258d2a5b55581f5f2b27aa6c1a369bb2503e43` (M1–M4 implementation baseline; D-021 archive records it as investigation revision).
- `git status --short` at audit time: 25 lines, matching the previously reported pre-existing state:
  - 11 modified: `docs/aac-enc-migration-spec.md`, `rust/src/aac_enc.rs`, `bitcount.rs`, `bitenc.rs`, `energy.rs`, `psy.rs`, `quant.rs`, `rate.rs`, `scf.rs`, `testdata.rs`, `tns.rs`.
  - 14 untracked: `docs/aac-enc-benchmark-plan.md`, `docs/aac-enc-first-divergence-m5.26.md`, `docs/aac-enc-performance-campaign-m5.12-m5.22.md`, `docs/aac-enc-quality-divergence-audit-m5.25.md`, `rust/src/aac_enc/phase1b.rs`, `rust/src/aac_enc/stream_driver.rs`, `rust/src/aac_enc/testdata/m55-PROVENANCE.txt`, 3× `m7-*.wav`, `m8-PROVENANCE.txt`, `m8-music-eroica-stereo-48k.wav`, `tools/c-enc-bench/`, `tools/enc-oracle/__pycache__/`.
- Target path `docs/aac-enc-m5-readiness-audit.md` did not exist before this audit; this file is the only file created by this task.

## 2. Implementation matrix (M1–M4 and unfinished work)

Plan reference: `docs/aac-enc-migration-spec.md:251-282` (M1–M8), `##9` (M1 impl), `##10` (M2 results), `##11–11.5` (M3 TNS diagnosis/closeout). There is no `##M4` section in the spec; M4 S1–S8 scope lives in `rust/src/aac_enc.rs:108-139` doc comments.

| Milestone | Status | Evidence |
|---|---|---|
| M1 config/channel/ADTS | Implemented (modified, uncommitted) | `rust/src/aac_enc/config.rs:184,208,259,264,272`, `channel_map.rs:227,244`, `adts.rs:158,170`; tests `adts.rs:212,220,228,242,265`, `config.rs:289,303,316,346`, `channel_map.rs:266,290,326,347,362` |
| M2 PCM/MDCT/block-switch | Implemented (modified, uncommitted) | `pcm.rs:196,241,261`, `block_switch.rs:287,331,399,437`, `transform.rs:179,202`; tests `pcm.rs:372,405,428,438,457,469`, `block_switch.rs:491-600`, `transform.rs:320-491` (MDCT gate `|Δ|<5e-4·peak`, only 9 vectors iterated) |
| M3 SFB/spread/pre-echo/TNS | Implemented, diagnosed, pinned (modified, uncommitted) | `sfb.rs`, `thresholds.rs:126,145,214`, `psy.rs:162,468,483`, `tns.rs:711,804,855`; `KNOWN_TNS` pins in `psy.rs`, TNS merge bit-exact test `tns.rs:920` |
| M4 S1 PE/static | Code exists, test-driver-wired only | `pe.rs:268,545`, `static_bits.rs:266,279`; tests `pe.rs:706-1049`, `static_bits.rs:319-495` |
| M4 S2 VBR adapt | Code exists, test-driver-wired only | `threshold_vbr.rs:606,552`; tests `threshold_vbr.rs:968-1654` |
| M4 S4 scf estimation | Code exists, test-driver-wired only (inv=2 afterburner only) | `scf.rs:1110,1119`; tests `scf.rs:1465-1649` |
| M4 S5 noiseless counting | Code exists, test-driver-wired only | `bitcount.rs:1237,1245,532,554`; tests `bitcount.rs:1314-1799` |
| M4 S6 QC loop | Code exists, test-driver-wired only, single-pass only | `rate.rs:373,410,481,512,1420`; 20 tests `rate.rs:1830-2506`, all committed frames single-pass (`npass=1,crash=-1`) |
| M4 S7 bit encoding | Code exists, test-driver-wired only | `bitenc.rs:303,393,642,657,699,795`; tests `bitenc.rs:835-1228` (287-vector probe transcript) |
| M4 S8 framing/ADTS | Code exists, test-driver-wired only | `frame.rs:825,949,1107,1127`; tests `frame.rs:1170-1395` |
| End-to-end assembly | **Test-only.** No production `Encoder` struct or `encode()`; `aac_enc.rs` exposes module list only (`rust/src/aac_enc.rs:141-171`, `#[cfg(feature="aac_enc")]` via `rust/src/lib.rs:101-102`). Full chain wired exclusively in untracked `rust/src/aac_enc/stream_driver.rs:468,499,608,801` (`StreamEncoder`, mono/stereo only) and isolated differential `rust/src/aac_enc/phase1b.rs:118` | `stream_driver.rs:1-57` header states test-only; `phase1b.rs:1-21` states isolated differential |
| M5–M7 (afterburner closure, rate closure, quality/perf parity) | Not started as implementation; measurement plans and divergence audits exist as untracked docs | `docs/aac-enc-benchmark-plan.md` (M5.7 plan, unexecuted), `docs/aac-enc-quality-divergence-audit-m5.25.md`, `docs/aac-enc-first-divergence-m5.26.md`, `docs/aac-enc-performance-campaign-m5.12-m5.22.md` |
| M8 public API/CLI/docs | Not started | No `Encoder` API, FFI, or CLI in repo; spec `M8` exit criteria unaddressed |

Deliberately out of scope in code (asserted absent, not validated on live content): intensity stereo (`stream_driver.rs:35-36,950-970`), PNS (`usePns=0`), >2ch elements/LFE pairing, 960-granule, CBR/reservoir/LATM/RAW, retry/crash/fill paths (loud skip `stream_driver.rs:1208-1230`).

## 3. End-to-end test coverage matrix

Harness: `PcmFeeder` (`pcm.rs:196-261`) + `StreamEncoder::step_frame` (`stream_driver.rs:801`) + ADTS writer (`frame.rs:1127`). Fixture sets: `FIXTURES:15` (`stream_driver.rs:1393`), `M52_FIXTURES:15` (`:3968`), `M55_FIXTURES:4` (`:4228`), `M58_FIXTURES:19` (`:4399`).

| Area | Covered | Test(s) | Gap |
|---|---|---|---|
| Frame sequencing / buffering | Counts and order only | `pcm.rs:372,405,428,438,457`, `phase1_full_stream_parity` (`stream_driver.rs:3520`, explicitly "No parity asserts here by design") | No payload ground truth past first frames |
| Flush / termination | Frame-count law `ceil((S+1600)/1024)` + zero-pad | `pcm.rs:438,457`, `m52_all_fixtures_write_complete` (`stream_driver.rs:4316`) | Final-frame payload differs on 11/18 whole-file fixtures (M5.25) with no per-stage ground truth |
| Channel configs | Mono + stereo only (driver asserts single-element) | `m52_all_fixtures_write_complete`, `m55_streams_match_c_reference` (`stream_driver.rs:4257`, validity+accounting only, "explicitly no byte-parity") | 5.1/6.1/7.1, LFE, CCE: config accepts, nothing end-to-end |
| Sample rates | 48 kHz + 44.1 kHz end-to-end | `phase1f_onset441_f22_live_matches_real_c` (`:2691`), `phase1i_sine44_f5_tier_b` (`:3330`) | 8/16/24/32/96 kHz: SFB config parity only (`sfb.rs:609`), no end-to-end |
| VBR-5 configuration | VBR-5 afterburner-on only | Whole `stream_driver.rs` chain (`peOffset=0`, bitres full) | No other bitrate mode tested by design |
| ADTS output validity | Header asserts + re-parse per emitted frame | `check_header` (`stream_driver.rs:1557`: sync `0xFFF1`, LC, freqIdx, chanCfg, frame_length, fullness `0x7FF`), `adts.rs:212,220,228` | No in-test decode; FFmpeg attestation (18/18) claimed in M5.25 doc with no raw logs preserved |
| Skip/failure policy | Loud stop-at-first-skip invariant | `m52_emit_or_fail_is_loud` (`stream_driver.rs:4115`) | Retry/crash/PNS/IS never exercised on real content; skips abort the stream |
| Determinism | Rerun determinism | `phase1_reproducibility` (`stream_driver.rs:3920`) | Unpinned toolchain; build-env requirements not recorded |
| Benchmarks | Untimed smoke + opt-in profiling only | `m58_smoke_untimed` (`:4582`, pins 735/306/2359 B), `m58_corpus_validation` (`:4616`, timed only if `M58_MEASURE_TIMING`), `m511_profile_loop` (`:4667`, opt-in) | No in-test timing asserts; no fair C-vs-Rust ratio in tests (M5.21 `~1.25×/1.16×` coarse, non-identical boundaries, per perf doc) |

No test in `rust/` decodes output, compares decoded PCM (SNR/SDR/NMR/PE/spectral), measures bitrate distributions, or checks clipping. The `clip_cap` unit mirror (`thresholds.rs:280`) is not a clipping test.

## 4. Claims inventory

### Established (directly measured or bit-exact on equal inputs)

- Per-stage parity on equal inputs: TNS merge (`tns.rs:920`), RXX-relevant paths, S4 corpus (`scf.rs:1520`), S5 corpus (`bitcount.rs:1562`), S6 corpus (`rate.rs:2149`), S7 probe transcript + corpus payloads (`bitenc.rs:1144,1228`), S8 elements (`frame.rs:1395`), block sequences (`block_switch.rs:600`), ADTS reference headers (`adts.rs:212,220`).
- `phase1b_isolated_differential` (`phase1b.rs:118`): S4/S5/S6 bit-exact given C inputs on 3 frames (sine f5 LONG, onset f24 SHORT, stress f5 surrogate).
- Real-C capture builds are byte-identical to baseline builds (D-021 archive `evidence/v429`: base vs instrumented ADTS hash-duplicate triplets).
- D-021 disposition: SHORT-frame onset gaps (+28 B f24 / +7 B f22, −1 B sine f1, +0 multitone f1) root-caused to finite-precision MDCT arithmetic with exact downstream accounting; no decision-rule defect demonstrated. Preserved in `docs/research/d021/` (commit `53481f3a`), closure record + `verify_manifest.py` (`checked=89 failures=0` at archival).
- Frame-count law `frames=ceil((S+1600)/1024)` holds across tested fixtures (M5.2 acceptance test).

### Partially established (measured with caveats)

- End-to-end Rust encoder emits complete, re-parseable ADTS with valid headers on 15 synthetic + 4 licensed fixtures (`m52_all_fixtures_write_complete`, `m55_streams_match_c_reference`). Caveat: validity/accounting only, no byte-parity, no decode-in-test.
- Whole-file C-vs-Rust ADTS: 16/18 differ, 2 identical (silence, edge); header/frame-count parity everywhere; FFmpeg decode attested 18/18 in M5.25 doc. Caveat: no raw decode logs in repo; no quality metric attached.
- First-divergence attribution on 20 frames (M5.26 doc): TNS static, MS band-16 residue, S4 SCF/Q flips pinned to exact gates; 4 blips + sine f1 inconclusive (oracle `S6FINAL+56≠8·real_len` unfaithful region documented).
- Delay alignment: end-to-end 1600 samples established only for four named synthetic 48 kHz fixtures; decoder front trim 1744 samples measured on the tested 48 kHz path; 44.1 kHz separately gated (uncommitted spec edit `docs/aac-enc-migration-spec.md:239-248`).
- Performance: in-process medians exist (music 59/66 ms, silence 1.1/1.3 ms per perf doc) with byte-identity chain `5f58213f…` across milestones. Caveat: active-filter divergences carried as unresolved quality risks; C-vs-Rust ratio coarse only.

### Unverified (no measurement in repo)

- Perceptual competitiveness or any objective quality floor (SNR/SDR/NMR/PE/spectral vs PCM): explicitly absent everywhere (M5.25 states "no metric anywhere").
- Average bitrate / frame-size histograms vs targets or vs C.
- Clipping behavior on real content.
- Any configuration outside mono/stereo × 48/44.1 kHz × VBR-5 ADTS.
- Flush-payload correctness on final frames.
- Behavior when live PNS/IS/retry/crash/fill paths trigger.
- Release criteria themselves (undefined), CI coverage (no workflow config in repo: no `.github/workflows/`), and toolchain-pinned reproducibility.

## 5. Five highest-priority remaining risks

1. **No decoded-output quality metric exists (impact: highest).** Nothing distinguishes "numerically close" from "perceptually competitive." M5.25 documents 16/18 whole-file divergences with zero SNR/SDR/NMR attached. Any release or parity claim is currently unjudgeable. Evidence: M5.25 §"no metric anywhere"; §4-unverified above.
2. **No production encoder exists (impact: highest).** The complete chain runs only under `#[cfg(test)]` (`stream_driver.rs`, `phase1b.rs`, `testdata.rs` all test-gated per `aac_enc.rs:153-167`). There is nothing to release, fuzz, or integrate — M8 API/CLI unstarted. Evidence: §2 matrix row.
3. **Flush/termination and skip policy are unvalidated (impact: high).** Final-frame payloads differ on most whole-file fixtures; the driver stops at first skip rather than handling retry/crash/fill (`stream_driver.rs:1208-1230`, `m52_emit_or_fail_is_loud`). A real stream hitting these paths aborts. Evidence: M5.25 11/18 final-frame note; §3 matrix row.
4. **Live-only paths are asserted absent, never validated (impact: high).** PNS/IS books, multi-pass retry, crash recovery, large fills, LO-active/`merged=1` TNS have no live-content exercise. First real-world trigger is an unhandled or loud-skip event. Evidence: §2 out-of-scope list; `rate.rs` corpus all single-pass.
5. **No automated regression gate or pinned reproduction (impact: medium-high).** No CI workflow in repo, benchmarks untimed/opt-in, tool versions unrecorded, oracle-dependent vectors reproducible only via unpinned C builds. Regressions and perf drift have no tripwire. Evidence: §3 benchmark rows; `tools/enc-oracle/` (not in any build) + `tools/c-enc-bench/c-enc-bench.c` (untracked).

## 6. Proposed M5 sequence (small, independently verifiable)

- **M5.a — Decode-and-check harness (validity only).** Wire `aac-dec`/FFmpeg decode of every emitted ADTS from `m52_all_fixtures_write_complete` corpus into the test workspace (out-of-repo invocation, hashes + return codes recorded). Acceptance: 100% decodable, frame counts consistent, failures loud. No quality judgment yet.
- **M5.b — Delay-compensated PCM comparison baseline (metrics, no gates).** Record per-fixture SNR/SDR + avg bitrate + frame-size histogram on a small representative set (4 synthetic already aligned + 2 licensed in-repo WAVs), using the measured-lag contract from the spec edit. Acceptance: numbers published for Rust and C side-by-side; no pass/fail threshold imposed.
- **M5.c — Flush/termination + skip-policy validation.** Per-frame payload ground truth for final frames on the M5.b set; enumerate skip triggers on real content; decide handle-vs-abort per path with tests. Acceptance: final-frame reconciliation closed or explicitly bounded; skip policy documented and tested.
- **M5.d — Configuration matrix declaration + one expansion.** Declare supported matrix (mono/stereo × 48/44.1 kHz × VBR-5 ADTS) and rejection behavior for everything else with tests; add exactly one expansion candidate (e.g., 32 kHz or documented refusal). Acceptance: matrix doc + negative tests.
- **M5.e — Production API slice (cfg-gated, no FFI).** Promote `StreamEncoder` core behind `#[cfg(feature="aac_enc")]` as a minimal `Encoder` (config → frames → flush) with the test driver as its first client; no C API/CLI. Acceptance: end-to-end tests run through the production path; test-only shim deleted or thin.
- **M5.f — CI gate + reproducibility pins.** Minimal workflow running the M5.a/M5.b harness + fastest parity subset, with toolchain versions and input hashes recorded. Acceptance: green/red signal on every change to `rust/src/aac_enc/*`.

## 7. Recommended next task

**M5.a — Decode-and-check harness (validity only), before any quality metric.**

- Scope: take the exact ADTS bytes already emitted by `m52_all_fixtures_write_complete` (`stream_driver.rs:4316`, 15 fixtures) plus the 4 licensed streams from `m55_streams_match_c_reference` (`stream_driver.rs:4257`); decode each with the in-repo C decoder (`aac-dec.c`) and record return code, decoded frame/sample counts, and input/output hashes. Out-of-repo invocation; no Rust production change; no listening/ABX.
- Acceptance criteria:
  1. 100% of emitted streams decode without error, or each failure is a named, reproduced case with fixture + frame index.
  2. Decoded length matches `frames × 1024` minus the documented delay contract within the already-measured 4-fixture alignment; deviations listed per stream, not averaged away.
  3. Results recorded as a committed doc table (fixture, bytes, frames, decode rc, decoded samples, hash) plus the invocation script/command lines for rerun.
- Why this first: it is the cheapest falsifier — if any emitted stream is undecodable or mistrimmed, all downstream quality work is premature; if all decode, M5.b metrics have a sound foundation. It requires no new encoder code and respects the no-ABX constraint.

## 8. Confirmation of non-interference

- No production code, tests, fixtures, documentation, or generated files were modified by this audit.
- No staging, committing, stashing, resetting, cleaning, restoring, or amending was performed.
- The 25-line pre-existing worktree state was verified unchanged at audit start (HEAD `53481f3a`, status/diffstat identical to the archival baseline pattern).
- The sole filesystem effect of this task is the creation of this new file, `docs/aac-enc-m5-readiness-audit.md`, which did not previously exist. Temporary verification files (if any) were written outside the repository.
- The D-021 investigation was not rerun; its archive (`docs/research/d021/`, commit `53481f3a`) was read, not altered.
