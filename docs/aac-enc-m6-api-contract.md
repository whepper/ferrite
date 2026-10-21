# M6.0 — Production Encoder API Contract and Design

Design-only. No implementation, no behavior change, no runs.

**Status: `M6.0 COMPLETE — API CONTRACT READY FOR IMPLEMENTATION`**

## 1. Initial state and sources

- Branch `main`, HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91`; 40-line status, empty index (verified before/after; unchanged throughout).
- Sources verified (read-only; behavior as implemented, not as remembered): `pcm.rs:150-283` (feeder lifecycle/errors), `config.rs:125-205` (whitelist, `TransportType`, `EncoderConfig::new`, `ConfigError`), `stream_driver.rs:825-829` (`step_frame` signature/contract), `:1219-1231` (npass/crash policy), `:1203-1204` (qc-error skip), `rate.rs:1420-1768` (`qc_main_frame`, `QcError`, `MAX_ITERATIONS=5`), `frame.rs:825-1141` (element write, raw block, ADTS assembly), error enums `bitenc.rs:207`, `adts.rs:128`; spec M8 (`migration-spec.md:280-282`).
- Reports read: M5.j (decision + boundary), M5.i (margins/iterations), M5.g (census/policy fields), M5.e (mono44100), M5.a (decoder gates), M5.b (baselines/alignment), M5.d (flush rule `tail=(1600+D)−Z`, loud-skip policy).

## 2. Proposed public API (owned, single-threaded, pull-style, behind `aac_enc`)

```rust
// New production file: rust/src/aac_enc/encoder.rs → `pub mod encoder;`
// (gated by the existing `#[cfg(feature = "aac_enc")]` in lib.rs:101-102).

/// One complete ADTS frame as emitted by the encoder.
pub struct AdtsFrame {
    pub bytes: Vec<u8>,    // 7-byte header + raw block, byte-exact
    pub frame_index: u32,  // 0-based position in the stream
}

pub struct Encoder { /* feeder + inner encoder state + finished/poison flags + counter (all owned, no globals) */ }

impl Encoder {
    /// Accepts exactly mono/stereo × {44100, 48000} × ADTS (validated matrix).
    /// Anything else → `EncodeError::UnsupportedConfiguration` (decided here,
    /// before `EncoderConfig::new`, so `ConfigError` keeps its truthful meaning).
    pub fn new(config: EncoderConfig) -> Result<Self, EncodeError>;

    /// Borrowed interleaved i16 input (`[LRLR…]` stereo, `[M…]` mono).
    /// Returns one `AdtsFrame` per completed 1024-sample block, in order.
    /// Empty slice is a successful no-op. Unaligned length → `Err`, no state change.
    pub fn push(&mut self, samples: &[i16]) -> Result<Vec<AdtsFrame>, EncodeError>;

    /// End of input: emits the padded partial frame (if any) plus zero-padded
    /// delay frames per the verified frame law, exactly once.
    pub fn flush(&mut self) -> Result<Vec<AdtsFrame>, EncodeError>;

    /// True after successful `flush` OR after any terminal error.
    pub fn is_finished(&self) -> bool;
}

/// Terminal-everywhere error surface. No variant permits continuation except
/// where explicitly marked recoverable below.
pub enum EncodeError {
    UnsupportedConfiguration { sample_rate: u32, channels: u32, transport: TransportType }, // pre-construction scope gate
    Config(ConfigError),          // constructor validation (12-rate whitelist etc.)
    UnalignedInput { len: usize, channels: u32 },  // RECOVERABLE: rejected before buffering
    AlreadyFinished,              // push/flush after finished; no state change
    FrameFailed { frame_index: u32, completed_before: usize, kind: EncodeFailure }, // TERMINAL: in-call completed frames discarded (count reported); encoder poisoned
    Poisoned,                     // any use after a terminal error; no output
}

pub enum EncodeFailure { SkippedFrame { notes: Vec<String> }, CrashedFrame, QcFailed(QcError), FrameWrite(FrameError), BitWrite(BitEncError), Adts(AdtsError) }
```

No `Send`/`Sync`/threading/callback/async promises (verified: no globals or shared mutability in driver/config/pcm/rate — instance-owned state throughout; `Sync` specifically unverified and not offered). No other sample rates, layouts, profiles, or transports promised.

## 3. State-transition table (all semantics source-verified, M5.d/M5.j)

| # | Operation | Result |
|---|---|---|
| 1 | Valid input pre-flush | `Ok(frames)` for each completed 1024-sample block; remainder buffered (`pending_per_channel` internal; not exposed) |
| 2 | Input split at arbitrary valid (channel-aligned) boundaries | `Ok`; byte-identical stream to single-push (M6.1 must test split-equality) |
| 3 | Empty input (immediate `flush`, or empty `push`) | Success: `push(&[])` → `Ok(vec![])`; `flush` with no input emits the delay frames (currently 2 zero-informed frames) — a valid, decodable stream, not an error |
| 4 | Partial final input | Buffered; `flush` pads with zeros per the frame law (M5.d §2) |
| 5 | Flush with buffered input | Emits partial + delay frames, sets finished, returns them |
| 6 | Repeated `flush` | `Err(AlreadyFinished)`, no output, state unchanged |
| 7 | Input after successful flush | `Err(AlreadyFinished)`, no output, no state change |
| 8 | Unaligned sample count | `Err(UnalignedInput)`, rejected before buffering (`pcm.rs:245` checks first) — **the only recoverable error**; correct re-push works |
| 9 | Encoding/frame-emission failure (skip, crash, Qc/Frame/BitEnc/Adts error) | `Err(FrameFailed{..})`, in-call completed frames discarded with the count reported, encoder poisoned (`finished=true`); S8-write failures are terminal because frontend state already advanced past the frame — retry would double-advance and corrupt the stream |
| 10 | Reuse after terminal error | `Err(Poisoned)`, no output, no state change |

Crash policy decision (implementation gate, documented): on `crash != -1` the frame bytes are **discarded, not emitted** — a deliberate, explicit delta from the test driver (which emits-then-fails-loudly, `stream_driver.rs:1225-1231`). Rationale: emitting bytes alongside an error enables accidental duplicate/corrupt streams and violates "never emit after error". Numerics are unaffected on all validated corpora (crash never fires there — M5.g census proves it; the M6.1 hash gate enforces it).

## 4. Ownership rules

- Input **borrowed** (`&[i16]`): the feeder copies into `pending` regardless, so borrowing avoids transfer ceremony; caller retains its buffer. Format is interleaved little-endian i16 in WAV channel order (existing `PcmFeeder` contract, `pcm.rs:102-110`).
- Output **returned per call** (`Vec<AdtsFrame>`, in order); nothing accumulated internally — pull-style, no unbounded internal growth, matches the m52 collection pattern.
- ADTS headers **emitted by the encoder** (`build_adts_frame` per frame, `frame.rs:1127`; verified 7-byte headers in every artifact).
- Pre-error output: discarded with the count reported in `FrameFailed.completed_before` (consistent with `m52_emit_or_fail` shape; stream is terminal anyway).
- Duplicates impossible by construction: success advances exactly once; failure poisons (retry errors); finished rejects. No path emits the same frame twice.

## 5. Error taxonomy and scope

Fixed by implementation (documented, not configurable): AAC-LC, VBR-5, afterburner on, `peOffset=0`, full bit reservoir (`stream_driver` chain + `aac-enc.c` parity). From `EncoderConfig`: sample rate, channel count/mode, transport (validated subset only), frame length (must be 1024). Everything else is internal.

- Recoverable: `UnalignedInput` only (rejected pre-buffering).
- Terminal (poison; `is_finished()` true; further use → `Poisoned`): `FrameFailed` (all kinds), `AlreadyFinished` (state-preserving: finished stays finished), construction failures (no object).
- No skipped frames, ever: `None`/notes from `step_frame` map to `FrameFailed::SkippedFrame`, never to silent continuation — the M5.j evidence-backed policy, now contractual.
- No handle-and-continue: retry/fallback inside `qc_main_frame` is unchanged C-ported behavior; at the API boundary, any deviation from single-pass-clean is an error, not a recovery. (Validating a future handle-policy is release work, explicitly out of M6.1.)
- Context preserved: `FrameFailed` carries frame index, completed count, notes, and the source error (`QcError`/`FrameError`/etc. via `EncodeFailure`); `Display`+`std::error::Error` impls required (existing enums already implement `Display`).

## 6. Compatibility acceptance criteria (M6.1 gates)

1. Byte-identical ADTS for all 4 validated configs on established inputs (hashes: M5.a `a9d7b395…`/`702c8717…`/`aa906b52…`, M5.e `c3b8b5fd…`).
2. M5.a gates pass on API-produced streams (exit 0 + empty stderr + structure/ch/sr/counts).
3. M5.b numbers unchanged (re-run metrics; JSON-identical).
4. 19-stream census + M5.i margins consistent (re-run `m5g_branch_census`: same rows + same global minima).
5. Lifecycle table (§3) tested case-by-case, including split-boundary byte-equality, empty-input stream validity, double-flush/input-after-flush errors, unaligned-input recovery, and poison-on-error.
6. Test-driver migration without output change: the driver keeps running on internals (census/observability must keep working), while a migration harness asserts API output == driver output byte-for-byte on all 19 streams — **equality harness, not replacement** (replacing the driver would destroy the snapshot observability M5.g/M5.i depend on).

## 7. Ordered M6.1 implementation plan (bounded; not executed here)

1. **Relocate, don't rewrite:** move `StreamEncoder` + `FrameSnapshot` (+ census fields) and its pure-read observer lines into new production `rust/src/aac_enc/encoder.rs` as `pub(crate)`; leave all diagnostics, oracles, and survey tests in the test-gated driver module operating on the moved types. Pure move first, verified by `git diff` review + step 5 gates. (Fallback if the move proves tangled: un-gate `stream_driver` with test fns still individually gated — decide in M6.1 under the same hash gates.)
2. **Add the public surface** (`Encoder`, `AdtsFrame`, `EncodeError`, `EncodeFailure` per §2) in `encoder.rs`; `pub mod encoder;` (no new feature gate — inherits `aac_enc`).
3. **Wire policy:** `push` = feeder-drain + step per frame (collect or fail-loud); `flush` = finish + steps, exactly-once flag; `failed` poison flag on any error path; crash path discards bytes (gate §3).
4. **Unit tests:** all 10 lifecycle rows (incl. split-equality, empty-input validity, double-flush, post-finish input, unaligned-then-recover, poison-after-forced-error via synthetic gate inputs following the `m52_emit_or_fail_is_loud` pattern); error-variant coverage for every `EncodeError` arm.
5. **Compatibility gates** (§6, in order): byte-identity → decode gates → baselines → census+margins → migration-harness equality.
6. **Test-only stays test-only:** census test/fields remain; verify the production build exposes only the §2 surface (review `pub` items; no `#[cfg(test)]` leakage into the contract).
7. **Exclusions:** Raw/multi-channel/other rates (reject per §2), PNS/IS, tuning, threading, CLI/FFI, CI, perceptual work.

## 8. Assumptions, blockers, deferred capabilities

- Assumptions (source-verified, not guessed): single-pass-clean on validated corpora (M5.g census); crash-never-fires there; delay 1600 + trim rule (M5.a/M5.b/M5.d); no globals/shared state.
- No blockers: every contract behavior maps to existing, evidenced semantics (the one deliberate delta — crash discard-vs-emit — is specified as a gate, not an assumption).
- Deferred: Raw transport validation, >2ch/other-rate expansion, handle-and-continue policy design, PNS/IS features, `Sync`/threading, CLI/FFI, CI pinning, perceptual/corpus work, release criteria. None blocks design; several gate release (per M5.j §5).

## 9. Final state and non-interference confirmation

- HEAD `53481f3af2e5b3fc78ce02ca3fc0af8aaf3b4e91`, branch `main` — unchanged.
- Final `git status --short`: 41 lines = 40-line baseline + this report; all other lines verified identical; index empty (0 staged).
- No source, test, fixture, configuration, report, or generated-file changes of any kind; no executions, builds, or benchmarks — the sole filesystem effect is this new file. D-021 untouched.
