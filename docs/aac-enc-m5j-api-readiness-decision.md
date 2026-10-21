# M5.j — Validation Readiness and Production API Decision

Read-only audit. No code, test, fixture, report, or configuration changes; no executions.

**Decision: `PROCEED` to a bounded API-design milestone (scope and constraints in §7).**

## 1. Initial state

- Branch `main`, HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91` (verified before and after; unchanged).
- Initial `git status --short`: 39 lines (11 modified implementation files + M5.0–M5.i reports + test-gated harness files + `cfg_44100_112000.txt` + testdata WAVs/provenance + `tools/c-enc-bench/`, `tools/enc-oracle/__pycache__/`). Index empty.
- Deliverable verified absent before writing. No staging, commits, or any other git operations.

## 2. Evidence matrix (claims with repository references; unknowns marked)

### 2.1 Configuration coverage
- Validated end-to-end (encode → ADTS → third-party decode → metrics): mono/48k, stereo/48k, stereo/44.1k (M5.a §3–4, M5.b §2–4), mono/44.1k (M5.e §3–4). All four: exit 0 + empty stderr, structural WAVs, deterministic bytes.
- Constructor accepts 12 rates (`config.rs:125-127,184-205`), channel modes per table, Transports Adts/Raw (`config.rs:129-137`). Only 4 cells validated; Raw transport has zero end-to-end runs (all tests ADTS); >2ch untested live. Limits explicit — scoping input, not a defect signal.

### 2.2 Output integrity
- Per-frame ADTS re-parse in every emitting test (`m52_write_adts_stream` read-back; `check_header` sync/profile/freqIdx/chanCfg/length/fullness); M5.g asserts `adts_len*8 == 56+total` on all 2442 rows; M5.i re-verified with extended rows.
- Decoder gates (exit 0 **and** empty stderr — M5.a §1 caveat — plus WAV/ch/sr/count checks) pass on all executed decodes (M5.a 6, M5.b 9, M5.d 6, M5.e new-case 2+2).
- Determinism: encoder (ADTS hashes stable across days/runs, M5.b §4, M5.d §2, M5.e §4, M5.g §4, M5.i §6), C encoder (`cmp` clean), decoder (cross-day hash match) — all under stated local toolchains.

### 2.3 Objective baseline
- Three M5.b fixtures unchanged since M5.b (hashes re-verified in M5.d §2, M5.e §4, M5.g §4, M5.i §6; metrics JSON byte-identical on re-run). Mono-44100 added in M5.e (SNR 57.98/57.09, rust-vs-c 57.51 lag 0, bitrate 16.10 vs 16.14 kbps) without touching the three baselines.
- Rust-vs-orig ≈ C-vs-orig within 0.5 dB (3 cases) / 0.9 dB (mono44100); rust-vs-c residue 53–63 dB (trio) / 57.5 dB (mono441), consistent with the D-021 finite-precision account (no contradiction; D-021 stays closed).
- Alignment: nominal lag −1600 with documented alias analysis (M5.b §5); bitrate over original duration with header/payload split; trimmed regions excluded by construction (limitation, not a gap).

### 2.4 Lifecycle
- Feeder: single-use, channel-aligned push, `finish`-once with `AlreadyFinished` error, zero-padded flush to the 1600-sample delay, frame law `ceil((S+1600)/1024)` (`pcm.rs:108-283`; empty-input and exact-fit cases unit-tested).
- Termination: rule `tail = (1600+D) − Z` derived and exact on all 3 M5.b fixtures (M5.d §2); no unexplained loss/duplication.
- Skip/error: loud-abort everywhere (skip→`None`+note, crash→note+emit-then-fail, envelope→panic, `m52_emit_or_fail` gate); 19/19 M5.d streams + M5.g census clean. Policy is abort-loud by implementation, not by choice yet — see §3A.

### 2.5 Coding-path evidence
- Direct: MS, TNS (decision/filter/signaling), short/long switching and grouping, SCF/quant/noiseless/bitenc/frame/S8-fill-write, ADTS/transport.
- Indirect (unit-forced): retry gain-bump, crash recovery, QcError variants, exit-matrix table, fill sizing.
- Never observed live (2442-frame bound): retry, crash, QcError, nonzero fill-counter anomalies (fill *bits* observed, 287 frames × 7 bits — the mechanism runs; the *exceptional* branches don't).
- Disabled/unobservable by construction: PNS/IS (zero-forced inputs, corpus-pinned off), dead stub arms (`check_min_frame_demand()=1`), outer-guard panic tripwire.

### 2.6 Retry/crash and margins
- Forced tests prove mechanism + convergence + error outcomes at source (M5.h §4); driver-seam mapping (skip/note) unit-pinned with synthetic inputs only.
- M5.g: zero live firings (bounded search incl. 1884 licensed frames) — evidence of non-occurrence in-corpus, not of unreachability or correctness.
- M5.i: `iterations` max 0/5 on all frames; margin minima 984–1045 bits (noise-mono f1), all other streams ≥1304. No threshold approached. What absence establishes: the common path never nears the exceptional paths *in this corpus*; it does not validate the exceptional paths.

### 2.7 Reproducibility and maintenance
- Tests run via `cargo test --features aac_enc` (stable); fixtures committed or deterministic resynthesis; metrics stdlib-only; census JSON machine-checkable. Gaps: no CI workflow in repo (verified M5.c/M5.f), C tools built locally in `/tmp` with unpinned Apple clang, oracle vectors reproducible only via unpinned C builds. Design-relevant work is reproducible; release-grade pinning is not — separated in §5.

## 3. Remaining-risk classification

- **Retry/crash abort-loud policy untested on real output** — `KNOWN LIMITATION — ACCEPTABLE FOR API DESIGN` (contract can mandate Err; product-suitability of abort vs handle is a pre-*release* question, §4).
- **PNS/IS unobserved** — `NOT A CURRENT BLOCKER` (deliberately disabled; §4B).
- **Objective results ≠ universal correctness/perceptual proof** — `QUALITY UNCERTAINTY` (bounds claims, not design; §4C).
- **Raw transport / >2ch / other rates unvalidated** — `KNOWN LIMITATION — ACCEPTABLE FOR API DESIGN` (scope the API to validated cells; §7).
- **No CI / unpinned C toolchain** — `INFRASTRUCTURE GAP` (release gate, not design gate; §4E).
- **No production API exists** — the subject of the decision, not a risk class.
- Nothing in the evidence qualifies as `API BLOCKER` or `VALIDATION BLOCKER` under the brief's bar (a specific issue making the *contract* unsafe or underdetermined). Details per question below.

### A. Retry/crash without live activation
Source-level forced tests + the exact predicate map (M5.h) + full-margin census (M5.i) establish *what the branches do and when they would fire* to a degree sufficient for contract design: the API can specify single-pass expectation and define every deviation (multi-pass, crash mark, QcError) as a typed `Err`, mirroring the existing loud policy and error enums (`QcError::{QuantError,BitresTooLow}`, `FeederError`, `FrameError`). What must wait for release: whether abort-loud is the right product behavior on real content (trigger rate unknown) and any handle-and-continue semantics (implemented nowhere — a behavior change, not validation). Design the error surface now; validate the policy before release.

### B. Disabled paths
PNS/IS are suppressed by construction with corpus-pinned absence — not defects, not blockers — provided the API scope promises only implemented AAC-LC behavior (no PNS/IS emission) and documents the suppression. Enabling them later would be a feature milestone with its own validation, not a hidden API liability today.

### C. Objective results vs API work
Decoding + SNR/bitrate agreement prove the pipeline is stable, deterministic, and numerically close to the reference on the validated cells — the right foundation for wrapping behavior in an API. They prove nothing about untested cells or perception; the API milestone must therefore carry no quality or coverage claims beyond the four cells (§7 constraints).

### D. Lifecycle semantics
Sufficiently specified to define a contract: owned single-use feeder (`AlreadyFinished` on reuse), channel-aligned push (`UnalignedInput` on ragged input), padded flush with the exact frame law, encoder delay 1600 with the M5.d reconciliation rule, ADTS framing with byte-exact length accounting, loud errors for skips/notes/crashes. No ambiguity was found that would make the contract unsafe; the one soft spot (Raw transport, zero runs) is handled by scoping, not by blocking.

### E. Reproducibility
Design work needs only the repo + cargo + the committed/test-gated path — reproducible today. The `/tmp` C-tool builds and unpinned toolchains affect *reference* reproduction (C encoder/decoder/oracle), not the Rust API surface or its tests. CI is a release gate; its absence must not block design (and the M5.g/M5.i artifacts give CI its first pin-worthy content).

## 4. API-readiness boundary (design, not release)

Designable now (all implementation-supported, source-verified §2): `EncoderConfig`-driven construction with validated-rate/channel rejection (`config.rs:184-205`); owned single-threaded `Encoder` holding feeder + `StreamEncoder`-equivalent state (no globals, no shared mutability — verified: no `static mut`/`thread_local`/Rc/RefCell/Mutex in driver/config/pcm/rate); `push_interleaved`-style input with `UnalignedInput` errors; exactly-once `finish` with `AlreadyFinished` enforcement; per-frame ADTS output (`Vec<u8>` per frame, byte-exact lengths); flush semantics per the frame law; error enum wrapping the existing `FeederError`/`QcError`/`FrameError`/writer errors with abort-loud mapping (skip/note/crash → `Err`, no silent partial streams); documented encoder-delay/framing/determinism semantics; `#[cfg(feature = "aac_enc")]` placement (`lib.rs:101-102`).

Explicitly gated (not designed now): Raw transport, >2ch, non-validated rates, handle-and-continue retry policy, PNS/IS emission, thread-safety beyond owned single-threaded use (no `Sync` claim without verification), any quality/coverage claim beyond the four cells, CI/release pinning.

## 5. Decision: PROCEED

Evidence for proceeding: 4/4 cells decode + measure + reproduce deterministically; lifecycle rule exact; 2442-frame clean census with full margins; forced tests + trigger map define every exceptional branch; error/config surfaces already exist as production types; no globals or shared state constrain the design.

**Next milestone scope (bounded API design, no behavior change):** wrap the existing pipeline behind a cfg-gated owned `Encoder` (config → frames → flush) with the test driver as first client; map every loud path to typed `Err`; scope to mono/stereo × 48/44.1k × VBR-5 ADTS (reject or gate everything else as unvalidated); assert byte-identical ADTS to the M5.a/M5.b/M5.e/M5.g records for all 19+1 streams (numerics-unchanged gate); document delay/framing/determinism per M5.d/M5.b.

**Mandatory constraints:** no quantization/psychoacoustic/rate-control/tuning changes; no PNS/IS enabling; no corpus expansion; no perceptual claims; no `Sync`/threading promises; no Raw/multi-channel exposure beyond rejection; test-driver-first (production path exercised by existing tests before any new API tests).

**Validation required before release (not before design):** live retry/crash policy evidence (trigger rate on real content), CI + toolchain pinning, real-content quality numbers, broader-config coverage, release criteria definition.

## 6. Deferred work and why not blockers

- Live retry/crash forcing: adversarial, unbounded; margins (M5.i) give no evidence it is necessary; unit mechanism coverage + abort-loud contract suffice for design.
- Driver-seam test with crafted snapshots: gate fn already pinned; integration needs the same forcing as above.
- TNS-merged/LO + real-content transition truth (M5.f #4): narrow, independent, lower risk than the API path.
- IS/PNS work: feature changes, not validation.
- CI: release gate; M5.g/M5.i artifacts are its first content.
- Perceptual/corpus work: `QUALITY UNCERTAINTY` by design; never a design precondition.

## 7. Final state and non-interference confirmation

- HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91`, branch `main` — unchanged.
- Final `git status --short`: 40 lines = 39-line baseline + this report; all other lines verified identical to `/tmp/m5j-status-initial.txt`; index empty (0 staged).
- No experiments, executions, or file modifications of any kind — the sole filesystem effect is this new file. D-021 not reopened (no contradictory evidence; M5.b residues remain consistent with the closure).
