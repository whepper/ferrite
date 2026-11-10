//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! Production single-threaded AAC-LC VBR-5 encoder API.
//!
//! Owned, pull-style `Encoder` over the existing encoding pipeline. No
//! numerical behavior is changed here: `push`/`flush` drive the same
//! `PcmFeeder` + `StreamEncoder::step_frame` path as the test-gated driver,
//! and every loud finding still fails loudly (as a typed `Err`, with the
//! encoder poisoned — never a silent skip).
//!
//! Validated scope (M6.0 contract): mono/stereo x 44.1/48 kHz x VBR-5 ADTS.
//! Anything else is rejected at construction.
//!
//! M7.6 adds two non-behavioral accessors: [`AdtsFrame::raw_payload`] (a
//! borrowed view of the AAC access unit with the ADTS header removed) and
//! [`EncoderConfig::audio_specific_config`] (the two-byte AAC-LC
//! AudioSpecificConfig). Encoded bytes, transport selection, delay, and flush
//! behavior are unchanged.

pub use super::adts::AdtsError;
pub use super::bitenc::BitEncError;
use super::config::{ConfigError, EncoderConfig};
pub use super::frame::FrameError;
use super::pcm::{FeederError, PcmFeeder, PcmFrame};
pub use super::rate::QcError;
use super::stream_driver::{StepError, StreamEncoder};

use super::adts::{ADTS_HEADER_BYTES, ADTS_PROFILE_AAC_LC};

/// One complete ADTS frame as emitted by the encoder.
#[derive(Debug, Clone)]
pub struct AdtsFrame {
    /// 7-byte ADTS header plus raw-data block, byte-exact.
    pub bytes: Vec<u8>,
    /// 0-based position of this frame in the stream.
    pub frame_index: u32,
}

impl AdtsFrame {
    /// Borrow the raw AAC access-unit payload, excluding the 7-byte ADTS
    /// header.
    ///
    /// The returned slice starts at the first byte of the `raw_data_block`.
    /// It is a view into [`bytes`](Self::bytes); nothing is copied or
    /// re-encoded. Because `bytes` is a public field, this accessor re-checks
    /// the framing invariant before returning: MPEG-4 ADTS, layer 0, no CRC,
    /// AAC-LC profile, a declared frame length equal to the buffer length, and
    /// exactly one raw data block. A mutated or truncated frame is rejected
    /// with [`AdtsError::MalformedFrame`] instead of returning arbitrary bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrite_aac::aac_enc::config::{EncoderConfig, TransportType};
    /// use ferrite_aac::aac_enc::encoder::Encoder;
    ///
    /// let cfg = EncoderConfig::new(48000, 1, 1024, TransportType::Adts).unwrap();
    /// let mut encoder = Encoder::new(cfg).unwrap();
    /// let frames = encoder.push(&vec![0i16; 2048]).unwrap();
    /// let frame = &frames[0];
    /// let payload = frame.raw_payload().unwrap();
    /// assert_eq!(payload, &frame.bytes[7..]);
    /// assert!(!payload.is_empty());
    /// ```
    pub fn raw_payload(&self) -> Result<&[u8], AdtsError> {
        let bytes = &self.bytes;
        if bytes.len() <= ADTS_HEADER_BYTES {
            return Err(AdtsError::MalformedFrame(
                "frame contains no AAC payload after the header",
            ));
        }
        if bytes[0] != 0xFF || bytes[1] & 0xF0 != 0xF0 {
            return Err(AdtsError::MalformedFrame("bad syncword"));
        }
        if bytes[1] & 0x08 != 0 {
            return Err(AdtsError::MalformedFrame("not MPEG-4 ADTS"));
        }
        if bytes[1] & 0x06 != 0 {
            return Err(AdtsError::MalformedFrame("nonzero ADTS layer"));
        }
        if bytes[1] & 0x01 != 1 {
            return Err(AdtsError::MalformedFrame("CRC-protected ADTS frame"));
        }
        if (bytes[2] >> 6) & 0x03 != ADTS_PROFILE_AAC_LC {
            return Err(AdtsError::MalformedFrame("not AAC-LC profile"));
        }
        let declared = ((usize::from(bytes[3] & 0x03)) << 11)
            | (usize::from(bytes[4]) << 3)
            | (usize::from(bytes[5]) >> 5);
        if declared != bytes.len() {
            return Err(AdtsError::MalformedFrame(
                "frame_length field does not match buffer length",
            ));
        }
        if bytes[6] & 0x03 != 0 {
            return Err(AdtsError::MalformedFrame(
                "multiple raw data blocks in one ADTS frame",
            ));
        }
        Ok(&bytes[ADTS_HEADER_BYTES..])
    }
}

/// Terminal failure of one encoding step. Every variant poisons the encoder;
/// no variant permits continuation.
#[derive(Debug, Clone)]
pub enum EncodeError {
    /// Configuration outside the validated matrix (mono/stereo x
    /// 44.1/48 kHz x ADTS). Decided here so `ConfigError` keeps its
    /// truthful meaning for constructor-level validation.
    UnsupportedConfiguration {
        sample_rate: u32,
        channels: u32,
        transport: super::config::TransportType,
    },
    /// `EncoderConfig` construction failure.
    Config(ConfigError),
    /// Input length is not a multiple of the channel count. Rejected before
    /// buffering; the encoder is NOT poisoned and the call may be retried
    /// with aligned input. The only recoverable error.
    UnalignedInput { len: usize, channels: u32 },
    /// Push or flush after successful completion. No state change.
    AlreadyFinished,
    /// A frame failed to encode. Frames completed earlier in the same call
    /// are discarded (reported via `completed_before`); the encoder is
    /// poisoned and subsequent calls return [`EncodeError::Poisoned`].
    FrameFailed {
        frame_index: u32,
        completed_before: usize,
        kind: EncodeFailure,
    },
    /// Any use after a terminal error. No output.
    Poisoned,
}

/// What went wrong when a frame failed. `BitEncError`/`AdtsError` have no
/// variants here by design: the S8 write path converts both into
/// `FrameError` (`frame.rs` `From` impls) before they reach the driver
/// boundary, so they can never surface as their own types (M6.0 contract
/// refinement, source-verified).
#[derive(Debug, Clone)]
pub enum EncodeFailure {
    /// Frame skipped with diagnostic notes (frontend mismatch, low-overlap
    /// shape, or an untyped write-path failure without a typed error).
    SkippedFrame { notes: Vec<String> },
    /// Crash-marked frame (`crash != -1` in the frame snapshot) that
    /// produced no bytes, or bytes accompanied by diagnostic notes:
    /// nothing is emitted. A crash-recovered frame with bytes and no
    /// notes/errors emits (FDK parity); only the unrecovered or
    /// note-carrying combinations reach this variant.
    CrashedFrame { notes: Vec<String> },
    /// Rate-control failure (`iterations` exhausted or bit-reservoir short).
    QcFailed(QcError),
    /// Bitstream write failure (element, static agreement, raw block, ADTS).
    FrameWrite(FrameError),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::UnsupportedConfiguration {
                sample_rate,
                channels,
                transport,
            } => {
                write!(f, "unsupported configuration: {sample_rate} Hz, {channels}ch, {transport:?} (validated: mono/stereo x 44.1/48 kHz x ADTS)")
            }
            EncodeError::Config(e) => write!(f, "invalid encoder config: {e}"),
            EncodeError::UnalignedInput { len, channels } => {
                write!(
                    f,
                    "input length {len} is not a multiple of {channels} channels"
                )
            }
            EncodeError::AlreadyFinished => write!(f, "encoder already finished"),
            EncodeError::FrameFailed {
                frame_index,
                completed_before,
                kind,
            } => {
                write!(f, "frame {frame_index} failed after {completed_before} completed this call: {kind}")
            }
            EncodeError::Poisoned => write!(f, "encoder poisoned by an earlier terminal error"),
        }
    }
}

impl std::error::Error for EncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(err) => Some(err),
            Self::FrameFailed { kind, .. } => Some(kind),
            _ => None,
        }
    }
}

impl std::fmt::Display for EncodeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeFailure::SkippedFrame { notes } => {
                write!(f, "frame skipped [{}]", notes.join("; "))
            }
            EncodeFailure::CrashedFrame { notes } => {
                write!(f, "crash-marked frame discarded [{}]", notes.join("; "))
            }
            EncodeFailure::QcFailed(e) => write!(f, "rate-control failure: {e}"),
            EncodeFailure::FrameWrite(e) => write!(f, "bitstream write failure: {e}"),
        }
    }
}

impl std::error::Error for EncodeFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::QcFailed(err) => Some(err),
            Self::FrameWrite(err) => Some(err),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Open,
    Finished,
    Poisoned,
}

/// Owned single-threaded AAC-LC VBR-5 encoder.
///
/// Feed PCM with [`push`](Self::push) until the input is exhausted, then call
/// [`flush`](Self::flush) exactly once to emit the padded tail; the stream is
/// complete only after a successful `flush`.
///
/// No internal synchronization is provided: use one encoder per stream and
/// serialize `&mut self` access. Moving a finished or open encoder between
/// threads is safe because it owns only plain data, but concurrent encoding
/// through one encoder is not supported.
///
/// # Examples
///
/// Encode one second of mono 48 kHz silence and collect the ADTS stream:
///
/// ```
/// use ferrite_aac::aac_enc::config::{EncoderConfig, TransportType};
/// use ferrite_aac::aac_enc::encoder::Encoder;
///
/// let cfg = EncoderConfig::new(48000, 1, 1024, TransportType::Adts).unwrap();
/// let mut encoder = Encoder::new(cfg).unwrap();
/// let mut stream = Vec::new();
/// for frame in encoder.push(&vec![0i16; 48000]).unwrap() {
///     stream.extend_from_slice(&frame.bytes);
/// }
/// for frame in encoder.flush().unwrap() {
///     stream.extend_from_slice(&frame.bytes);
/// }
/// assert!(encoder.is_finished());
/// assert_eq!(&stream[..2], &[0xFF, 0xF1], "ADTS syncword");
/// ```
pub struct Encoder {
    feeder: PcmFeeder,
    inner: StreamEncoder,
    state: Lifecycle,
    frame_index: u32,
    /// Test-only deterministic terminal-failure injection, compiled out of
    /// production builds: fail the frame with this index.
    #[cfg(test)]
    failure_injection: Option<(u32, EncodeFailure)>,
}

impl Encoder {
    /// Build an encoder for the validated matrix only.
    ///
    /// Configuration snapshots are embedded at compile time, so construction
    /// has no runtime filesystem, installation, or source-tree dependency.
    pub fn new(config: EncoderConfig) -> Result<Self, EncodeError> {
        let (sr, ch, tp) = (config.sample_rate(), config.channels(), config.transport());
        if !((sr == 44100 || sr == 48000)
            && (ch == 1 || ch == 2)
            && tp == super::config::TransportType::Adts)
        {
            return Err(EncodeError::UnsupportedConfiguration {
                sample_rate: sr,
                channels: ch,
                transport: tp,
            });
        }
        // Re-validate through the constructor (infallible given the screen
        // above, but the constructor owns the truth about validity).
        let cfg =
            EncoderConfig::new(sr, ch, config.frame_length(), tp).map_err(EncodeError::Config)?;
        Ok(Self {
            feeder: PcmFeeder::new(cfg),
            inner: StreamEncoder::new(sr, ch as usize),
            state: Lifecycle::Open,
            frame_index: 0,
            #[cfg(test)]
            failure_injection: None,
        })
    }

    /// Feed interleaved i16 PCM (`[LRLR..]` stereo, `[M..]` mono).
    /// Returns one [`AdtsFrame`] per completed 1024-sample block, in order.
    /// Empty input is a successful no-op. Unaligned input is rejected before
    /// buffering (recoverable). Any encoding failure poisons the encoder.
    ///
    /// Bounded chunks are still recommended for peak memory: the feeder
    /// copies each push into per-channel buffers, and a whole-file push holds
    /// the source, that copy, and every returned frame at once. Buffer work
    /// is linear for any push size; a chunk target around 8,000 sample frames
    /// per channel keeps peak memory small.
    pub fn push(&mut self, samples: &[i16]) -> Result<Vec<AdtsFrame>, EncodeError> {
        match self.state {
            Lifecycle::Open => {}
            Lifecycle::Finished => return Err(EncodeError::AlreadyFinished),
            Lifecycle::Poisoned => return Err(EncodeError::Poisoned),
        }
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        let frames = match self.feeder.push_interleaved(samples) {
            Ok(frames) => frames,
            Err(FeederError::UnalignedInput { len, channels }) => {
                return Err(EncodeError::UnalignedInput { len, channels })
            }
            // Unreachable: state is Open, so the feeder cannot be finished.
            // Fail closed (poison) rather than inventing a recovery mapping.
            Err(FeederError::AlreadyFinished) => {
                self.state = Lifecycle::Poisoned;
                return Err(EncodeError::Poisoned);
            }
        };
        self.emit_frames(&frames)
    }

    /// End of input: emit the padded partial frame (if any) plus zero-padded
    /// delay frames, exactly once. Marks the encoder finished.
    pub fn flush(&mut self) -> Result<Vec<AdtsFrame>, EncodeError> {
        match self.state {
            Lifecycle::Open => {}
            Lifecycle::Finished => return Err(EncodeError::AlreadyFinished),
            Lifecycle::Poisoned => return Err(EncodeError::Poisoned),
        }
        let frames = match self.feeder.finish() {
            Ok(frames) => frames,
            // Unreachable: state is Open, so the feeder is unfinished. Fail
            // closed: poison so `is_finished` stays truthful for a terminal
            // error instead of reporting an untouched encoder.
            Err(_) => {
                self.state = Lifecycle::Poisoned;
                return Err(EncodeError::Poisoned);
            }
        };
        let out = self.emit_frames(&frames)?;
        self.state = Lifecycle::Finished;
        Ok(out)
    }

    /// True after successful `flush` or after any terminal error.
    pub fn is_finished(&self) -> bool {
        self.state != Lifecycle::Open
    }

    /// Drive one [`PcmFrame`] batch through `step_frame`, mapping the
    /// (bytes, snapshot) outcome to the fail-closed contract. In-call
    /// completed frames are discarded on failure (reported via
    /// `completed_before`); the encoder is poisoned.
    fn emit_frames(&mut self, frames: &[PcmFrame]) -> Result<Vec<AdtsFrame>, EncodeError> {
        let mut out = Vec::with_capacity(frames.len());
        for fr in frames {
            let fresh: Vec<Vec<i16>> = fr.channels().iter().map(|b| b.to_vec()).collect();
            let (bytes, snap) = self.inner.step_frame(self.frame_index, &fresh);
            let idx = self.frame_index;
            self.frame_index += 1;
            // Test-only failure injection: `step_frame` has already run, so
            // the inner state advanced exactly as in a real terminal failure.
            #[cfg(test)]
            if let Some((at, kind)) = self.failure_injection.clone() {
                if idx == at {
                    self.state = Lifecycle::Poisoned;
                    return Err(EncodeError::FrameFailed {
                        frame_index: idx,
                        completed_before: out.len(),
                        kind,
                    });
                }
            }
            match map_outcome(idx, bytes, snap.step_error, snap.crash, snap.notes) {
                Ok(None) => {}
                Ok(Some(frame)) => out.push(frame),
                Err(kind) => {
                    self.state = Lifecycle::Poisoned;
                    return Err(EncodeError::FrameFailed {
                        frame_index: idx,
                        completed_before: out.len(),
                        kind,
                    });
                }
            }
        }
        Ok(out)
    }

    /// Test-only seam: fail the frame with index `at` after `step_frame` has
    /// run. Compiled out of production builds; never reachable through the
    /// public API.
    #[cfg(test)]
    fn inject_failure(&mut self, at: u32, kind: EncodeFailure) {
        self.failure_injection = Some((at, kind));
    }
}

/// Fail-closed mapping from one `step_frame` outcome to the API contract.
/// Pure function (no encoder state): unit-testable without forcing the
/// encoder. Ordering is deliberate — a typed error always wins; a crash
/// marker without bytes discards; notes-with-bytes discards; a clean frame
/// (including a crash-recovered frame that emitted bytes with no notes)
/// emits.
fn map_outcome(
    frame_index: u32,
    bytes: Option<Vec<u8>>,
    step_error: Option<StepError>,
    crash: i32,
    notes: Vec<String>,
) -> Result<Option<AdtsFrame>, EncodeFailure> {
    match (bytes, step_error, crash, notes.is_empty()) {
        (_, Some(StepError::Qc(e)), _, _) => Err(EncodeFailure::QcFailed(e)),
        (_, Some(StepError::Frame(e)), _, _) => Err(EncodeFailure::FrameWrite(e)),
        // Crash-marked frame with no bytes: discarded (crash recovery that
        // could not produce output; the driver now emits recoverable crash
        // frames instead of returning None for them).
        (None, None, c, _) if c != -1 => Err(EncodeFailure::CrashedFrame { notes }),
        (None, None, _, _) => Err(EncodeFailure::SkippedFrame { notes }),
        // Defensive: notes on a frame that produced bytes are disqualifying.
        (Some(_), None, _, false) => Err(EncodeFailure::CrashedFrame { notes }),
        (Some(b), None, _, true) => Ok(Some(AdtsFrame {
            bytes: b,
            frame_index,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::config::TransportType;

    fn mono48() -> EncoderConfig {
        EncoderConfig::new(48000, 1, 1024, TransportType::Adts).unwrap()
    }

    fn stereo48() -> EncoderConfig {
        EncoderConfig::new(48000, 2, 1024, TransportType::Adts).unwrap()
    }

    /// Structural ADTS check: 12-bit syncword and a frame_length field equal
    /// to the emitted size (independent of the encoder's accounting).
    fn assert_adts(frame: &AdtsFrame) {
        let b = &frame.bytes;
        assert!(
            b.len() >= 7,
            "frame {} shorter than ADTS header",
            frame.frame_index
        );
        assert_eq!(b[0], 0xFF, "syncword high byte");
        assert_eq!(b[1] & 0xF0, 0xF0, "syncword low nibble");
        let len =
            (((b[3] & 0x03) as usize) << 11) | ((b[4] as usize) << 3) | ((b[5] as usize) >> 5);
        assert_eq!(len, b.len(), "ADTS frame_length must match bytes");
    }

    // M6.0 lifecycle 1+3+5: valid input, empty input, flush with buffered input.
    #[test]
    fn empty_and_partial_flush_behaves() {
        let mut enc = Encoder::new(mono48()).unwrap();
        assert!(!enc.is_finished());
        assert!(enc.push(&[]).unwrap().is_empty());
        // 2000 samples = 1 full frame + 976 buffered.
        let out = enc.push(&vec![0i16; 2000]).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].frame_index, 0);
        assert_adts(&out[0]);
        let tail = enc.flush().unwrap();
        // 976 pending + delay pad: ceil((2000 + 1600 - 1024) / 1024) = 3 frames.
        assert_eq!(tail.len(), 3);
        for (i, f) in tail.iter().enumerate() {
            assert_eq!(f.frame_index, 1 + i as u32);
            assert_adts(f);
        }
        assert!(enc.is_finished());
    }

    // M6.0 lifecycle 3: flush with no input at all still yields a valid,
    // complete ADTS stream (delay frames only).
    #[test]
    fn empty_input_flush_emits_valid_stream() {
        let mut enc = Encoder::new(mono48()).unwrap();
        let tail = enc.flush().unwrap();
        assert_eq!(tail.len(), 2, "ceil(1600 / 1024) delay frames");
        for (i, f) in tail.iter().enumerate() {
            assert_eq!(f.frame_index, i as u32);
            assert_adts(f);
        }
        assert!(enc.is_finished());
    }

    // M6.0 lifecycle 2: input split at arbitrary valid boundaries.
    #[test]
    fn split_input_matches_single_push() {
        let pcm = vec![1000i16; 5000];
        let mut a = Encoder::new(stereo48()).unwrap();
        let mut joined = Vec::new();
        let mut idx = 0u32;
        for chunk in pcm.chunks(333 * 2) {
            for f in a.push(chunk).unwrap() {
                assert_eq!(f.frame_index, idx, "chunked push order");
                idx += 1;
                joined.extend_from_slice(&f.bytes);
            }
        }
        for f in a.flush().unwrap() {
            assert_eq!(f.frame_index, idx, "flush order after chunked pushes");
            idx += 1;
            joined.extend_from_slice(&f.bytes);
        }
        let mut b = Encoder::new(stereo48()).unwrap();
        let mut whole = Vec::new();
        for f in b.push(&pcm).unwrap() {
            whole.extend_from_slice(&f.bytes);
        }
        for f in b.flush().unwrap() {
            whole.extend_from_slice(&f.bytes);
        }
        assert_eq!(joined, whole, "split/blue push must be byte-identical");
    }

    // M6.0 lifecycle 6+7: repeated flush, input after finish.
    #[test]
    fn double_flush_and_post_finish_push_fail() {
        let mut enc = Encoder::new(mono48()).unwrap();
        assert!(!enc.flush().unwrap().is_empty());
        assert!(enc.is_finished());
        assert!(matches!(enc.flush(), Err(EncodeError::AlreadyFinished)));
        assert!(matches!(
            enc.push(&[0i16; 4]),
            Err(EncodeError::AlreadyFinished)
        ));
        // Empty input after completion is also rejected: the lifecycle check
        // precedes the empty-input no-op.
        assert!(matches!(enc.push(&[]), Err(EncodeError::AlreadyFinished)));
        assert!(enc.is_finished());
    }

    // M6.0 lifecycle 8: invalid length rejected before buffering (recoverable).
    #[test]
    fn unaligned_input_rejected_and_recoverable() {
        let mut enc = Encoder::new(stereo48()).unwrap();
        assert!(matches!(
            enc.push(&[0i16; 3]),
            Err(EncodeError::UnalignedInput {
                len: 3,
                channels: 2
            })
        ));
        assert!(!enc.is_finished());
        // The rejected samples were not buffered and no state advanced: the
        // post-error stream is byte-identical to a fresh encoder fed the same
        // aligned input.
        let aligned = vec![0i16; 2048];
        let out = enc.push(&aligned).unwrap();
        assert_eq!(out.len(), 1);
        let mut got = Vec::new();
        for f in &out {
            got.extend_from_slice(&f.bytes);
        }
        for f in enc.flush().unwrap() {
            got.extend_from_slice(&f.bytes);
        }
        let mut fresh = Encoder::new(stereo48()).unwrap();
        let mut want = Vec::new();
        for f in fresh.push(&aligned).unwrap() {
            want.extend_from_slice(&f.bytes);
        }
        for f in fresh.flush().unwrap() {
            want.extend_from_slice(&f.bytes);
        }
        assert_eq!(got, want, "rejected push must leave no buffered state");
    }

    // M6.0 config scope: validated matrix accepted, everything else refused.
    #[test]
    fn config_scope_gates() {
        for (sr, ch) in [(44100, 1), (44100, 2), (48000, 1), (48000, 2)] {
            assert!(
                Encoder::new(EncoderConfig::new(sr, ch, 1024, TransportType::Adts).unwrap())
                    .is_ok(),
                "validated cell {sr}/{ch}"
            );
        }
        for (sr, ch) in [(32000, 1), (48000, 6), (8000, 2)] {
            let cfg = EncoderConfig::new(sr, ch, 1024, TransportType::Adts);
            let err = match cfg {
                Ok(c) => match Encoder::new(c) {
                    Ok(_) => panic!("out-of-scope cell {sr}/{ch} unexpectedly accepted"),
                    Err(e) => e,
                },
                Err(e) => EncodeError::Config(e),
            };
            assert!(
                matches!(
                    err,
                    EncodeError::UnsupportedConfiguration { .. } | EncodeError::Config(_)
                ),
                "out-of-scope cell {sr}/{ch} refused, got {err:?}"
            );
        }
        // Raw transport passes the constructor but is outside the API scope.
        let raw = EncoderConfig::new(48000, 2, 1024, TransportType::Raw).unwrap();
        assert!(matches!(
            Encoder::new(raw),
            Err(EncodeError::UnsupportedConfiguration { .. })
        ));
        // Bad frame length fails at the constructor level.
        let bad = EncoderConfig::new(48000, 2, 512, TransportType::Adts);
        assert!(matches!(bad, Err(ConfigError::UnsupportedFrameLength(512))));
    }

    // M6.0 lifecycle 9+10 + crash policy: mapping table unit coverage.
    // Synthetic outcomes only (no encoder forcing); proves every arm maps.
    #[test]
    fn outcome_mapping_table() {
        // Clean emit.
        let ok = map_outcome(7, Some(vec![0xFF]), None, -1, Vec::new()).unwrap();
        let f = ok.expect("emits");
        assert_eq!(f.frame_index, 7);
        // Crash-marked frame without bytes: discarded. (Bytes with a crash
        // marker plus notes are still discarded below; bytes with a crash
        // marker and no notes — a recovered crash frame — emit, per FDK.)
        for bytes in [None, Some(vec![0xFF])] {
            let err = map_outcome(
                3,
                bytes,
                None,
                0,
                vec!["gain-loop crash recovery (maxSfb reduced) crash=0".to_string()],
            );
            assert!(
                matches!(err, Err(EncodeFailure::CrashedFrame { .. })),
                "crash marker with notes must map to CrashedFrame, got {err:?}"
            );
        }
        // Crash-recovered frame: bytes, no error, no notes — emits.
        let ok = map_outcome(7303, Some(vec![0xFF]), None, 45, Vec::new()).unwrap();
        assert_eq!(ok.expect("crash-recovered frame emits").frame_index, 7303);
        // Notes + bytes without a crash marker are still disqualifying.
        let err = map_outcome(
            3,
            Some(vec![0xFF]),
            None,
            -1,
            vec!["unexpected note".to_string()],
        );
        assert!(
            matches!(err, Err(EncodeFailure::CrashedFrame { .. })),
            "notes with bytes must never emit, got {err:?}"
        );
        // Typed QC error.
        let err = map_outcome(
            0,
            None,
            Some(StepError::Qc(QcError::QuantError)),
            -1,
            vec!["qc error".to_string()],
        );
        assert!(
            matches!(err, Err(EncodeFailure::QcFailed(_))),
            "qc, got {err:?}"
        );
        // Typed frame error.
        let err = map_outcome(
            0,
            None,
            Some(StepError::Frame(FrameError::BadFrameLength("x"))),
            -1,
            vec!["raw block".to_string()],
        );
        assert!(
            matches!(err, Err(EncodeFailure::FrameWrite(_))),
            "frame, got {err:?}"
        );
        // Notes-only skip without a crash marker (frontend mismatch,
        // low-overlap).
        let err = map_outcome(5, None, None, -1, vec!["geometry len".to_string()]);
        assert!(
            matches!(err, Err(EncodeFailure::SkippedFrame { .. })),
            "skip, got {err:?}"
        );
        // Hypothetical bytes + typed error (no current path produces this):
        // fail closed, never emit.
        let err = map_outcome(
            0,
            Some(vec![0xFF]),
            Some(StepError::Qc(QcError::BitresTooLow)),
            -1,
            Vec::new(),
        );
        assert!(err.is_err(), "bytes+error must never emit");
    }

    // Frame indices count every attempted frame in order.
    #[test]
    fn frame_indices_are_sequential() {
        let mut enc = Encoder::new(mono48()).unwrap();
        let mut idx = 0u32;
        for f in enc.push(&vec![0i16; 5000]).unwrap() {
            assert_eq!(f.frame_index, idx);
            idx += 1;
        }
        for f in enc.flush().unwrap() {
            assert_eq!(f.frame_index, idx);
            idx += 1;
        }
        assert_eq!(idx, 7, "5000 samples + delay pad at 48k mono");
    }

    #[test]
    fn frame_failure_display_and_source_preserve_cause() {
        use std::error::Error as _;

        let qc = EncodeError::FrameFailed {
            frame_index: 42,
            completed_before: 1,
            kind: EncodeFailure::QcFailed(QcError::BitresTooLow),
        };
        let text = qc.to_string();
        assert!(text.contains("frame 42"), "{text}");
        assert!(text.contains("bit reservoir is too small"), "{text}");
        let kind = qc.source().expect("frame error source");
        assert!(kind.to_string().contains("rate-control failure"), "{kind}");
        assert!(
            kind.source()
                .expect("QC error source")
                .to_string()
                .contains("bit reservoir"),
            "QC source must be preserved"
        );

        let frame = EncodeError::FrameFailed {
            frame_index: 7,
            completed_before: 0,
            kind: EncodeFailure::FrameWrite(FrameError::BadFrameLength("test")),
        };
        assert!(
            frame.to_string().contains("bad frame length: test"),
            "{frame}"
        );
    }

    #[test]
    fn raw_payload_borrows_bytes_after_adts_header() {
        let mut enc = Encoder::new(stereo48()).unwrap();
        let frames = enc.push(&vec![0i16; 4096]).unwrap();
        assert_eq!(frames.len(), 2);

        let mut payload_total = 0;
        let mut adts_total = 0;
        for frame in &frames {
            let payload = frame.raw_payload().unwrap();
            assert_eq!(payload, &frame.bytes[ADTS_HEADER_BYTES..]);
            assert_eq!(
                payload.as_ptr(),
                frame.bytes[ADTS_HEADER_BYTES..].as_ptr(),
                "payload must be a borrowed view, not a copy"
            );
            assert!(!payload.is_empty());
            payload_total += payload.len();
            adts_total += frame.bytes.len();
        }
        assert_eq!(payload_total + ADTS_HEADER_BYTES * frames.len(), adts_total);
    }

    #[test]
    fn raw_payload_rejects_malformed_frames() {
        let mut enc = Encoder::new(mono48()).unwrap();
        let valid = enc.push(&vec![0i16; 2048]).unwrap().remove(0);

        let mut header_only = valid.clone();
        header_only.bytes.truncate(ADTS_HEADER_BYTES);
        assert!(matches!(
            header_only.raw_payload(),
            Err(AdtsError::MalformedFrame(_))
        ));

        let mut truncated = valid.clone();
        truncated.bytes.pop();
        assert!(matches!(
            truncated.raw_payload(),
            Err(AdtsError::MalformedFrame(_))
        ));

        let mut bad_sync = valid.clone();
        bad_sync.bytes[0] = 0;
        assert!(matches!(
            bad_sync.raw_payload(),
            Err(AdtsError::MalformedFrame("bad syncword"))
        ));

        let mut crc = valid.clone();
        crc.bytes[1] &= !0x01;
        assert!(matches!(
            crc.raw_payload(),
            Err(AdtsError::MalformedFrame("CRC-protected ADTS frame"))
        ));

        let mut profile = valid.clone();
        profile.bytes[2] &= 0x3F;
        assert!(matches!(
            profile.raw_payload(),
            Err(AdtsError::MalformedFrame("not AAC-LC profile"))
        ));

        let mut blocks = valid.clone();
        blocks.bytes[6] |= 0x01;
        assert!(matches!(
            blocks.raw_payload(),
            Err(AdtsError::MalformedFrame(_))
        ));

        assert!(valid.raw_payload().is_ok());
    }

    // M6.0 lifecycle 9+10 at the encoder level: the terminal state rejects all
    // further use and never resumes. The failure->poison transition itself is
    // one unconditional assignment in `emit_frames`; no deterministic live
    // trigger exists (M5.g/M5.h prove crash/retry never fire on the corpus),
    // so the state is entered here directly rather than via a production hook.
    #[test]
    fn poisoned_encoder_rejects_all_operations() {
        let mut enc = Encoder::new(stereo48()).unwrap();
        enc.state = Lifecycle::Poisoned;
        assert!(enc.is_finished(), "poisoned counts as finished");
        assert!(matches!(enc.push(&[0i16; 4]), Err(EncodeError::Poisoned)));
        assert!(matches!(enc.flush(), Err(EncodeError::Poisoned)));
        assert!(enc.is_finished(), "failed calls must not clear poisoning");
    }

    // M6.3 Scope C: push-time terminal failure through the real `emit_frames`
    // path, via the test-only injection seam (compiled out of production).
    #[test]
    fn push_failure_poisons_and_discards_in_call_frames() {
        let mut enc = Encoder::new(mono48()).unwrap();
        // 4096 samples = 4 complete frames in one call; fail the third.
        enc.inject_failure(
            2,
            EncodeFailure::CrashedFrame {
                notes: vec!["injected".to_string()],
            },
        );
        match enc.push(&vec![0i16; 4096]) {
            Err(EncodeError::FrameFailed {
                frame_index,
                completed_before,
                kind,
            }) => {
                assert_eq!(frame_index, 2);
                assert_eq!(
                    completed_before, 2,
                    "frames completed earlier in the call are counted"
                );
                assert!(matches!(kind, EncodeFailure::CrashedFrame { .. }));
            }
            other => panic!("expected FrameFailed, got {other:?}"),
        }
        assert!(enc.is_finished(), "terminal failure poisons");
        assert!(
            matches!(enc.push(&[0i16; 4]), Err(EncodeError::Poisoned)),
            "later pushes must not resume encoding"
        );
        assert!(matches!(enc.flush(), Err(EncodeError::Poisoned)));
    }

    // M6.3 Scope C: flush-time terminal failure, same real path as push.
    #[test]
    fn flush_failure_poisons_and_emits_nothing() {
        let mut enc = Encoder::new(mono48()).unwrap();
        // 1500 samples: one frame now, 476 buffered; the first flush frame is
        // stream frame 1.
        assert_eq!(enc.push(&vec![0i16; 1500]).unwrap().len(), 1);
        enc.inject_failure(1, EncodeFailure::QcFailed(QcError::BitresTooLow));
        match enc.flush() {
            Err(EncodeError::FrameFailed {
                frame_index,
                completed_before,
                kind,
            }) => {
                assert_eq!(frame_index, 1);
                assert_eq!(completed_before, 0);
                assert!(matches!(kind, EncodeFailure::QcFailed(_)));
            }
            other => panic!("expected FrameFailed, got {other:?}"),
        }
        assert!(enc.is_finished(), "flush failure poisons");
        assert!(matches!(enc.push(&[0i16; 4]), Err(EncodeError::Poisoned)));
        assert!(matches!(enc.flush(), Err(EncodeError::Poisoned)));
    }
}
