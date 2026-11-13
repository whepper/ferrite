//! Native loudness analysis and ReplayGain 2.0 + Sound Check tagging (L3).
//!
//! Analysis runs on source FLAC PCM through the validated `ebur128` 0.1.10
//! dependency (EBU R128, HISTOGRAM mode). Albums are source-parent
//! directories; every member is analyzed before any conversion publishes, so
//! the single-publication/no-rewrite guarantee holds by construction.
//!
//! Owner rules (L0 frozen, L2 finalized):
//!
//! - Fixed reference: **−18.0 LUFS**. Track and album gains are
//!   `−18.0 − integrated`, measured sample peaks verbatim.
//! - Easy-mode-equivalent positive-only clipping protection, applied
//!   independently to track gain (vs track peak) and album gain (vs album
//!   peak); adjusted gains never drop below `+0.00 dB`; AAC samples are never
//!   modified; `iTunNORM` derives from the clipping-adjusted track gain.
//! - `iTunNORM` uses the verified original conversion pipeline with
//!   round-half-even integer conversion (never Rust's half-away `round`).
//! - Unmeasurable tracks (silence / below the gating threshold) and analysis
//!   failures omit loudness atoms with warnings; conversion still succeeds.
//!   An album with any unanalyzable member omits album-level atoms (never a
//!   subset result presented as complete) with one album-level warning.
//! - `--no-tags`, ADTS, and dry-run skip analysis entirely (callers enforce).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ebur128::{Channel, EbuR128, Mode};

use crate::flac::FlacSource;
use crate::gapless::boxed;
use crate::progress::LoudnessOutcome;
use crate::ConvertError;

/// Fixed v1 reference level (L0 D2, frozen).
pub(crate) const REFERENCE_LUFS: f64 = -18.0;

/// The hardwired ReplayGain 2.0 reference the original workflow's `rsgain`
/// tags against. Identical to [`REFERENCE_LUFS`] in v1, so the shift term in
/// [`soundcheck_string`] is zero; both constants are kept so the pipeline
/// mirrors the verified original source line for line.
const RG2_REFERENCE_LUFS: f64 = -18.0;

/// Freeform atom names (`com.apple.iTunes` mean), in L2 ilst order.
const TRACK_GAIN_KEY: &str = "REPLAYGAIN_TRACK_GAIN";
const TRACK_PEAK_KEY: &str = "REPLAYGAIN_TRACK_PEAK";
const ALBUM_GAIN_KEY: &str = "REPLAYGAIN_ALBUM_GAIN";
const ALBUM_PEAK_KEY: &str = "REPLAYGAIN_ALBUM_PEAK";
const ITUNNORM_KEY: &str = "iTunNORM";

/// Analyzer modes for v1 (L2 §6 item 8): integrated + sample peak, HISTOGRAM.
fn analyzer_modes() -> Mode {
    Mode::I | Mode::SAMPLE_PEAK | Mode::HISTOGRAM
}

/// Precomputed loudness output for one converted file.
#[derive(Debug, Clone, Default)]
pub(crate) struct FileLoudness {
    /// Complete `----` ilst children in L2 order (track gain/peak, album
    /// gain/peak, `iTunNORM`); empty when the track is unanalyzable.
    pub atoms: Vec<Vec<u8>>,
    /// Per-file loudness warnings, appended after tag/cover warnings.
    pub warnings: Vec<String>,
}

/// Measured loudness for one analyzable track.
#[derive(Debug, Clone, Copy)]
struct TrackMeasurement {
    /// Integrated loudness in LUFS; `None` when undefined (silence / below
    /// the gating threshold).
    integrated: Option<f64>,
    /// Maximum sample peak as a linear 0…1 fraction of full scale.
    peak: f64,
}

/// Open a source for analysis: validated properties plus a fresh track
/// analyzer. `FlacSource::open` already enforces the 44.1/48 kHz mono/stereo
/// gate; anything else is unanalyzable here (conversion reports it genuinely).
fn open_track(path: &Path) -> Option<(FlacSource, EbuR128)> {
    let source = FlacSource::open(path).ok()?;
    let properties = source.properties();
    let channels = map_channels(properties.channels)?;
    let mut track = EbuR128::new(
        properties.channels,
        properties.sample_rate,
        analyzer_modes(),
    )
    .ok()?;
    track.set_channel_map(&channels).ok()?;
    Some((source, track))
}

/// Shared album-analyzer state across one album's members.
enum AlbumState {
    /// No member has seeded the analyzer yet.
    Absent,
    /// A live analyzer seeded from the first analyzable member's layout.
    /// Boxed: the analyzer holds histogram state far larger than the other
    /// variants.
    Live(Box<EbuR128>, (u32, u32)),
    /// A later member's layout mismatched the seed: no album value may be
    /// computed from the subset (D4).
    Poisoned,
}

/// How one member relates to the shared album aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JoinStatus {
    /// Fed (or seeded) the live aggregation.
    Joined,
    /// Layout mismatch while the aggregation was live: poisoned it. The
    /// member is still measured track-only, and is named as the cause.
    CausedPoison,
    /// Track-only while already poisoned, or no live aggregation without a
    /// measurement: a consequence, not a cause (measurement failures are
    /// reported via `measurement: None` instead).
    Detached,
}

/// Outcome of measuring one album member.
struct MemberResult {
    /// Track measurement (`None` when the file contributes nothing).
    measurement: Option<TrackMeasurement>,
    join: JoinStatus,
    /// Reporting category for batch progress.
    outcome: LoudnessOutcome,
}

/// Measure one album member: stream its PCM through a fresh track analyzer
/// and, when participating, the shared album analyzer.
///
/// The shared analyzer is seeded from the first analyzable member. A later
/// member whose rate/channel layout differs cannot join the aggregation: the
/// album state is poisoned so no album value can be computed from the subset
/// (D4), while the member keeps its own track measurement.
fn measure_member(path: &Path, album: &mut AlbumState) -> MemberResult {
    let (mut source, mut track) = match open_track(path) {
        Some(opened) => opened,
        None => {
            return MemberResult {
                measurement: None,
                join: JoinStatus::Detached,
                outcome: LoudnessOutcome::Failed,
            };
        }
    };
    let properties = source.properties();
    let spec = (properties.sample_rate, properties.channels);

    // Ensure a live analyzer when possible, poisoning on layout mismatch.
    let mut caused_poison = false;
    match album {
        AlbumState::Absent => {
            if let Some(map) = map_channels(properties.channels) {
                if let Ok(mut state) = EbuR128::new(
                    properties.channels,
                    properties.sample_rate,
                    analyzer_modes(),
                ) {
                    if state.set_channel_map(&map).is_ok() {
                        *album = AlbumState::Live(Box::new(state), spec);
                    }
                }
            }
        }
        AlbumState::Live(_, album_spec) if *album_spec != spec => {
            *album = AlbumState::Poisoned;
            caused_poison = true;
        }
        AlbumState::Live(_, _) | AlbumState::Poisoned => {}
    }
    let (mut feed, join) = match album {
        AlbumState::Live(state, album_spec) if *album_spec == spec => {
            (Some(state), JoinStatus::Joined)
        }
        AlbumState::Live(_, _) => unreachable!("layout mismatch poisons above"),
        AlbumState::Absent | AlbumState::Poisoned => (
            None,
            if caused_poison {
                JoinStatus::CausedPoison
            } else {
                JoinStatus::Detached
            },
        ),
    };

    let mut pcm = Vec::new();
    let mut failed = false;
    loop {
        let more = match source.next_pcm(&mut pcm) {
            Ok(more) => more,
            Err(_) => {
                failed = true;
                break;
            }
        };
        if !more {
            break;
        }
        if track.add_frames_i16(&pcm).is_err() {
            failed = true;
            break;
        }
        if let Some(state) = feed.as_mut() {
            if state.add_frames_i16(&pcm).is_err() {
                failed = true;
                break;
            }
        }
    }
    if failed {
        return MemberResult {
            measurement: None,
            join: JoinStatus::Detached,
            outcome: LoudnessOutcome::Failed,
        };
    }

    let integrated = match track.loudness_global() {
        Ok(value) if value.is_finite() => Some(value),
        Ok(_) => None,
        // The analyzer reports an error instead of a value when nothing
        // cleared the gating threshold (notably all-silence input).
        Err(_) => None,
    };
    let mut peak = 0.0f64;
    for channel in 0..properties.channels {
        match track.sample_peak(channel) {
            Ok(channel_peak) => peak = peak.max(channel_peak),
            Err(_) => {
                return MemberResult {
                    measurement: None,
                    join: JoinStatus::Detached,
                    outcome: LoudnessOutcome::Failed,
                };
            }
        }
    }
    if !peak.is_finite() {
        return MemberResult {
            measurement: None,
            join: JoinStatus::Detached,
            outcome: LoudnessOutcome::Failed,
        };
    }
    MemberResult {
        measurement: Some(TrackMeasurement { integrated, peak }),
        join,
        outcome: if integrated.is_some() {
            LoudnessOutcome::Analyzed
        } else {
            LoudnessOutcome::Unmeasurable
        },
    }
}

/// Supported channel layouts for analysis (matches the converter gate).
fn map_channels(channels: u32) -> Option<Vec<Channel>> {
    match channels {
        1 => Some(vec![Channel::Left]),
        2 => Some(vec![Channel::Left, Channel::Right]),
        _ => None,
    }
}

/// Positive-only clipping protection (verified Easy-mode `rsgain` behavior).
///
/// Gains at or below `+0.00 dB` pass through. A positive gain whose predicted
/// peak (`peak · 10^(gain/20)`) exceeds full scale is reduced by the excess
/// in dB — equivalently to exactly `−20·log10(peak)` — and never below
/// `+0.00 dB`. Returns `(written_gain, adjusted)`.
fn protect_gain(measured_gain: f64, peak: f64) -> (f64, bool) {
    if measured_gain > 0.0 {
        let predicted = peak * 10f64.powf(measured_gain / 20.0);
        if predicted > 1.0 {
            let adjusted = measured_gain - 20.0 * predicted.log10();
            return (adjusted.max(0.0), true);
        }
    }
    (measured_gain, false)
}

/// Round-half-even for non-negative values (the original Python `round()`
/// semantics for the non-negative `iTunNORM` slot inputs).
///
/// Rust's `f64::round` rounds half away from zero and must not be used here:
/// at exactly `+30.00 dB` the `sc_2500` slot is exactly 2.5, where the two
/// semantics differ by 1 LSB.
fn round_half_even(value: f64) -> f64 {
    debug_assert!(value >= 0.0, "iTunNORM slot inputs are non-negative");
    let floor = value.floor();
    let frac = value - floor;
    if frac < 0.5 {
        floor
    } else if frac > 0.5 {
        floor + 1.0
    } else if floor % 2.0 == 0.0 {
        floor
    } else {
        floor + 1.0
    }
}

/// One clamped 32-bit `iTunNORM` slot value (never `0xFFFFFFFF`).
fn soundcheck_slot(scaled: f64) -> u64 {
    (round_half_even(scaled) as u64).min(0xFFFFFFFE)
}

/// `iTunNORM`/Sound Check string from a clipping-adjusted track gain.
///
/// Verified original pipeline: shift to the v1 reference (zero at −18.0),
/// power ratio, banker's rounding, truncation, per-slot 32-bit clamp, then
/// the leading space, ten uppercase 8-hex fields (slots 1–2 and 3–4
/// duplicated), and the fixed filler sequence in order.
pub(crate) fn soundcheck_string(gain_db: f64) -> String {
    let adjusted_gain = gain_db + (REFERENCE_LUFS - RG2_REFERENCE_LUFS);
    let ratio = 10f64.powf(-adjusted_gain / 10.0);
    let sc_1000 = soundcheck_slot(ratio * 1000.0);
    let sc_2500 = soundcheck_slot(ratio * 2500.0);
    format!(
        " {sc_1000:08X} {sc_1000:08X} {sc_2500:08X} {sc_2500:08X} \
         00024CA8 00024CA8 00007FFF 00007FFF 00024CA8 00024CA8"
    )
}

/// Rendered ReplayGain gain value (`rsgain` `"%.2f dB"`).
fn gain_string(gain_db: f64) -> String {
    format!("{gain_db:.2} dB")
}

/// Rendered peak value (`rsgain` `"%.6f"`); peaks are always verbatim.
fn peak_string(peak: f64) -> String {
    format!("{peak:.6}")
}

/// One `----` freeform loudness atom (`mean com.apple.iTunes`, UTF-8 `data`
/// type 1, locale 0), matching the validated `rsgain`/mutagen layout.
fn freeform_atom(name: &str, value: &str) -> Result<Vec<u8>, ConvertError> {
    let mut payload = Vec::with_capacity(8 + value.len());
    payload.extend_from_slice(&1u32.to_be_bytes()); // data type: UTF-8
    payload.extend_from_slice(&0u32.to_be_bytes()); // locale
    payload.extend_from_slice(value.as_bytes());
    let data =
        boxed(b"data", &payload).map_err(|_| ConvertError::Metadata("loudness tag too large"))?;

    const MEAN_PREFIX: [u8; 4] = [0x00, 0x00, 0x00, 0x00];
    const MEAN_NAME: &[u8] = b"com.apple.iTunes";
    let mut mean_payload = Vec::with_capacity(MEAN_PREFIX.len() + MEAN_NAME.len());
    mean_payload.extend_from_slice(&MEAN_PREFIX);
    mean_payload.extend_from_slice(MEAN_NAME);
    let mean = boxed(b"mean", &mean_payload)
        .map_err(|_| ConvertError::Metadata("loudness tag too large"))?;

    let mut name_payload = Vec::with_capacity(MEAN_PREFIX.len() + name.len());
    name_payload.extend_from_slice(&MEAN_PREFIX);
    name_payload.extend_from_slice(name.as_bytes());
    let name = boxed(b"name", &name_payload)
        .map_err(|_| ConvertError::Metadata("loudness tag too large"))?;

    let mut inner = Vec::with_capacity(mean.len() + name.len() + data.len());
    inner.extend_from_slice(&mean);
    inner.extend_from_slice(&name);
    inner.extend_from_slice(&data);
    boxed(b"----", &inner).map_err(|_| ConvertError::Metadata("loudness tag too large"))
}

/// Loudness atoms for one analyzable track in L2 order.
///
/// `album` carries the protected album gain/peak when the album is complete,
/// `None` when album-level atoms must be omitted (D4).
fn track_atoms(
    track_gain: f64,
    track_peak: f64,
    album: Option<(f64, f64)>,
) -> Result<Vec<Vec<u8>>, ConvertError> {
    // The original reads the rendered tag string back from the file before
    // converting, so the `iTunNORM` input is the 2-decimal quantized gain.
    let rendered_gain = gain_string(track_gain);
    let quantized: f64 = rendered_gain
        .strip_suffix(" dB")
        .and_then(|stripped| stripped.parse().ok())
        .unwrap_or(track_gain);
    let mut atoms = Vec::with_capacity(5);
    atoms.push(freeform_atom(TRACK_GAIN_KEY, &rendered_gain)?);
    atoms.push(freeform_atom(TRACK_PEAK_KEY, &peak_string(track_peak))?);
    if let Some((album_gain, album_peak)) = album {
        atoms.push(freeform_atom(ALBUM_GAIN_KEY, &gain_string(album_gain))?);
        atoms.push(freeform_atom(ALBUM_PEAK_KEY, &peak_string(album_peak))?);
    }
    atoms.push(freeform_atom(ITUNNORM_KEY, &soundcheck_string(quantized))?);
    Ok(atoms)
}

/// Analyze a batch of files grouped into albums by source-parent directory.
///
/// `members` holds `(relative_input, absolute_input)` pairs in plan order;
/// the returned map is keyed by `relative_input` and holds an entry for every
/// member (empty atoms + warnings for unanalyzable ones). Album-level
/// omission warnings are attached to every member of an incomplete album so
/// the conversion pass emits them for each file concerned.
pub(crate) fn analyze_album(members: &[(PathBuf, PathBuf)]) -> HashMap<PathBuf, FileLoudness> {
    analyze_album_with_jobs(members, 1)
}

/// Analyze a batch with bounded cross-album concurrency.
///
/// Albums are independent aggregations, so they run on separate workers.
/// Members of one album deliberately stay sequential: they share one
/// `EbuR128` aggregation whose block boundaries depend on member order, so
/// parallelising inside an album would change loudness semantics (and would
/// require either redundant decoding or cross-worker analyzer state).
pub(crate) fn analyze_album_with_jobs(
    members: &[(PathBuf, PathBuf)],
    jobs: usize,
) -> HashMap<PathBuf, FileLoudness> {
    analyze_album_with_jobs_and_progress(members, jobs, |_, _| {})
}

/// Analyze a batch with bounded cross-album concurrency and per-file progress.
///
/// Behaves exactly like [`analyze_album_with_jobs`], and additionally invokes
/// `on_file(relative_input, outcome)` on the calling thread after each member's
/// PCM analysis finishes. Albums remain independent workers and members of one
/// album stay sequential; completion events may arrive in album-completion
/// order rather than plan order.
pub(crate) fn analyze_album_with_jobs_and_progress<F>(
    members: &[(PathBuf, PathBuf)],
    jobs: usize,
    mut on_file: F,
) -> HashMap<PathBuf, FileLoudness>
where
    F: FnMut(&Path, LoudnessOutcome),
{
    // Group member indices by album key (parent of the relative path;
    // root-level files form album `""`), preserving plan order.
    let mut albums: Vec<(&Path, Vec<usize>)> = Vec::new();
    for (index, (relative, _)) in members.iter().enumerate() {
        let key = relative.parent().unwrap_or_else(|| Path::new(""));
        match albums.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, indices)) => indices.push(index),
            None => albums.push((key, vec![index])),
        }
    }

    let measured = crate::parallel::map_indexed_with_progress(
        albums.len(),
        jobs,
        |album, sender| {
            let (_, indices) = &albums[album];
            analyze_one_album(members, indices, sender)
        },
        |(relative, outcome)| on_file(&relative, outcome),
    );
    let mut out = HashMap::with_capacity(members.len());
    // Merge in album order so the map contents are deterministic even though
    // albums completed out of order.
    for album in measured {
        out.extend(album);
    }
    out
}

/// Analyze a single source file as a degenerate one-member album.
pub(crate) fn analyze_single(path: &Path) -> FileLoudness {
    let relative = path
        .file_name()
        .map_or_else(|| PathBuf::from(path), PathBuf::from);
    let map = analyze_album(&[(relative.clone(), path.to_path_buf())]);
    map.get(&relative).cloned().unwrap_or_default()
}

/// Run the two-stage measurement for one album: per-track analysis feeding a
/// shared album analyzer, then gain/clipping/atom rendering.
///
/// Returns one entry per member in plan order. The caller merges albums; this
/// function never touches shared state, so albums can run concurrently.
fn analyze_one_album(
    members: &[(PathBuf, PathBuf)],
    indices: &[usize],
    progress: &dyn Fn((PathBuf, LoudnessOutcome)),
) -> Vec<(PathBuf, FileLoudness)> {
    // Stage 1: measure every member, feeding the shared album analyzer.
    // The analyzer is seeded from the first analyzable member; a later
    // member with a different rate/channel layout cannot join the
    // aggregation, which makes the album incomplete (never subset-as-complete).
    // A silent member (undefined integrated value) likewise makes the album
    // incomplete (L2 §7).
    let mut album = AlbumState::Absent;
    let mut album_complete = true;
    let mut album_peak = 0.0f64;
    let mut measurements: Vec<Option<TrackMeasurement>> = Vec::with_capacity(indices.len());
    let mut incomplete_members: Vec<PathBuf> = Vec::new();
    for &index in indices {
        let (relative, absolute) = &members[index];
        let result = measure_member(absolute, &mut album);
        // Progress observers are best-effort: a dropped receiver (cancelled
        // batch observer) must not stop analysis.
        progress((relative.clone(), result.outcome));
        // A member that poisoned the aggregation (layout mismatch) is
        // incomplete for album purposes even when its own track measurement
        // stands. Post-poison members measured track-only are consequences,
        // not causes, and are not named.
        let mut incomplete = result.join == JoinStatus::CausedPoison;
        match result.measurement {
            Some(measured) if measured.integrated.is_some() => {
                album_peak = album_peak.max(measured.peak);
            }
            // Unmeasurable (silence / below the gating threshold) or
            // unanalyzable: track atoms omitted and the album is incomplete.
            _ => incomplete = true,
        }
        if incomplete {
            album_complete = false;
            incomplete_members.push(relative.clone());
        }
        measurements.push(result.measurement);
    }

    // Stage 2: album aggregation (complete only when every member joined
    // the shared analyzer with a finite integrated value).
    let album_integrated = match &mut album {
        AlbumState::Live(state, _) => state.loudness_global().ok(),
        AlbumState::Absent | AlbumState::Poisoned => None,
    }
    .filter(|value| value.is_finite());
    let album_complete = album_complete && album_integrated.is_some();

    // One album-level notice for the whole album (D4); attached to every
    // member so the conversion pass emits it for each file concerned.
    let album_warning = if album_complete {
        None
    } else if incomplete_members.is_empty() {
        Some(
            "loudness: album analysis produced no finite integrated value: \
             omitting album-level loudness tags"
                .to_string(),
        )
    } else {
        let names = incomplete_members
            .iter()
            .map(|relative| relative.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "loudness: album is incomplete (unanalyzable: {names}): \
             omitting album-level loudness tags"
        ))
    };

    let (album_gain, album_gain_warned) = match (album_complete, album_integrated) {
        (true, Some(integrated)) => {
            let measured = REFERENCE_LUFS - integrated;
            let (written, adjusted) = protect_gain(measured, album_peak);
            let warning = adjusted.then(|| {
                format!(
                    "loudness: clipping protection adjusted album gain from {measured:.2} dB \
                     to {written:.2} dB"
                )
            });
            (Some((written, album_peak)), warning)
        }
        _ => (None, None),
    };

    let mut measured = Vec::with_capacity(indices.len());
    for (position, &index) in indices.iter().enumerate() {
        let (relative, _) = &members[index];
        let mut loudness = FileLoudness::default();
        match measurements[position] {
            Some(measured) => match measured.integrated {
                Some(integrated) => {
                    let track_measured = REFERENCE_LUFS - integrated;
                    let (track_written, track_adjusted) =
                        protect_gain(track_measured, measured.peak);
                    if track_adjusted {
                        loudness.warnings.push(format!(
                            "loudness: clipping protection adjusted track gain from \
                             {track_measured:.2} dB to {track_written:.2} dB"
                        ));
                    }
                    if let Some(warning) = &album_gain_warned {
                        loudness.warnings.push(warning.clone());
                    }
                    match track_atoms(track_written, measured.peak, album_gain) {
                        Ok(atoms) => loudness.atoms = atoms,
                        Err(_) => {
                            // Atom serialization is fail-closed and total on
                            // realistic inputs; on overflow the track degrades
                            // to unanalyzable rather than failing conversion.
                            loudness.atoms.clear();
                            loudness.warnings.push(
                                "loudness: tag serialization failed: \
                                 omitting loudness tags for this track"
                                    .to_string(),
                            );
                        }
                    }
                }
                None => {
                    loudness.warnings.push(
                        "loudness: no measurable signal: \
                         omitting loudness tags for this track"
                            .to_string(),
                    );
                }
            },
            None => {
                loudness.warnings.push(
                    "loudness: analysis failed: omitting loudness tags for this track".to_string(),
                );
            }
        }
        if let Some(warning) = &album_warning {
            loudness.warnings.push(warning.clone());
        }
        measured.push((relative.clone(), loudness));
    }
    measured
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-9;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= EPSILON,
            "actual={actual} expected={expected}"
        );
    }

    #[test]
    fn negative_and_zero_gains_pass_through_unadjusted() {
        for (measured, peak) in [(-3.00, 1.0), (0.00, 0.9), (-13.39, 0.98)] {
            assert_eq!(protect_gain(measured, peak), (measured, false));
        }
    }

    #[test]
    fn positive_gain_without_clip_risk_passes_through() {
        // +5.00 dB against peak 0.5: predicted peak ≈ 0.8891 < 1.0.
        let (written, adjusted) = protect_gain(5.00, 0.5);
        assert!(!adjusted);
        assert_close(written, 5.00);
    }

    #[test]
    fn positive_gain_with_clip_risk_is_reduced_to_full_scale() {
        // T-clip vectors from the verified original behavior (L2 §8). The raw
        // adjusted gains are exact full-scale solutions; the rendered strings
        // carry the 2-decimal values the original workflow writes.
        let (written, adjusted) = protect_gain(5.00, 0.9);
        assert!(adjusted);
        assert_close(written, 0.915149811213503);
        assert_eq!(gain_string(written), "0.92 dB");
        // Exactly −20·log10(peak): only the gain number changes, never audio.
        assert_close(written, -20.0 * 0.9f64.log10());

        let (album_written, album_adjusted) = protect_gain(6.00, 0.7);
        assert!(album_adjusted);
        assert_close(album_written, 3.098039199714864);
        assert_eq!(gain_string(album_written), "3.10 dB");
    }

    #[test]
    fn adjustment_never_drives_gain_below_zero() {
        // A peak above full scale (unreachable from i16 PCM, reachable only
        // through the unit under test) binds the mode-p cap at +0.00 dB.
        let (written, adjusted) = protect_gain(5.00, 1.5);
        assert!(adjusted);
        assert_eq!(written, 0.0);
        assert!(written >= 0.0);
    }

    #[test]
    fn boundary_gain_at_zero_is_not_positive() {
        assert_eq!(protect_gain(0.0, 1.0), (0.0, false));
        assert_eq!(
            protect_gain(f64::MIN_POSITIVE, 1.0),
            (f64::MIN_POSITIVE, false)
        );
    }

    #[test]
    fn round_half_even_matches_bankers_semantics() {
        for (input, expected) in [
            (2.5, 2.0),
            (3.5, 4.0),
            (0.5, 0.0),
            (1.5, 2.0),
            (13210.5, 13210.0),
            (13211.5, 13212.0),
            (2.4, 2.0),
            (2.6, 3.0),
            (0.0, 0.0),
        ] {
            assert_eq!(round_half_even(input), expected, "input={input}");
        }
        // Differs from Rust's half-away `round()` exactly at halves.
        assert_eq!(2.5f64.round(), 3.0);
        assert_eq!(round_half_even(2.5), 2.0);
    }

    #[test]
    fn soundcheck_vectors_match_the_verified_formula() {
        // (gain dB, expected sc_1000, expected sc_2500); L2 §4.4 vectors.
        for (gain, sc_1000, sc_2500) in [
            (0.00, 1000, 2500),
            (-7.23, 5284, 13211),
            (9.78, 0x69, 0x107),
            (0.18, 0x3BF, 0x95F),
        ] {
            let rendered = soundcheck_string(gain);
            let fields: Vec<&str> = rendered.split_whitespace().collect();
            assert_eq!(fields.len(), 10, "{gain}: {rendered}");
            assert_eq!(u64::from_str_radix(fields[0], 16).unwrap(), sc_1000);
            assert_eq!(u64::from_str_radix(fields[2], 16).unwrap(), sc_2500);
        }
    }

    #[test]
    fn soundcheck_half_way_pin_is_bankers_not_half_away() {
        // Exactly +30.00 dB: ratio·2500 is exactly 2.5 in binary64.
        assert!((10f64.powf(-30.0 / 10.0) * 2500.0 - 2.5).abs() == 0.0);
        let rendered = soundcheck_string(30.00);
        let fields: Vec<&str> = rendered.split_whitespace().collect();
        assert_eq!(fields[2], "00000002", "{rendered}");
        // Neighbor below the boundary rounds the other way.
        let below = soundcheck_string(29.99);
        let below_fields: Vec<&str> = below.split_whitespace().collect();
        assert_eq!(below_fields[2], "00000003", "{below}");
        // A half-away implementation would emit 00000003 here.
        assert_ne!(fields[2], "00000003");
    }

    #[test]
    fn soundcheck_clamps_at_both_boundaries() {
        let low = soundcheck_string(-70.00);
        assert!(
            low.starts_with(" FFFFFFFE FFFFFFFE FFFFFFFE FFFFFFFE"),
            "{low}"
        );
        assert!(!low.contains("FFFFFFFF"), "{low}");
        let high = soundcheck_string(100.00);
        assert!(
            high.starts_with(" 00000000 00000000 00000000 00000000"),
            "{high}"
        );
    }

    #[test]
    fn soundcheck_layout_is_exact() {
        let rendered = soundcheck_string(-7.23);
        assert_eq!(
            rendered,
            " 000014A4 000014A4 0000339B 0000339B \
             00024CA8 00024CA8 00007FFF 00007FFF 00024CA8 00024CA8"
        );
        // Leading space, single separators, uppercase, ten fields.
        assert!(rendered.starts_with(' '));
        assert!(!rendered.starts_with("  "));
        assert_eq!(rendered.split(' ').count(), 11);
        assert!(rendered.chars().all(|c| !c.is_lowercase()));
        // Slots 1–2 and 3–4 are duplicated.
        let fields: Vec<&str> = rendered.split_whitespace().collect();
        assert_eq!(fields[0], fields[1]);
        assert_eq!(fields[2], fields[3]);
        assert_eq!(
            fields[4..],
            ["00024CA8", "00024CA8", "00007FFF", "00007FFF", "00024CA8", "00024CA8"]
        );
    }

    #[test]
    fn gain_and_peak_rendering_match_rsgain() {
        assert_eq!(gain_string(9.78), "9.78 dB");
        assert_eq!(gain_string(-7.23), "-7.23 dB");
        assert_eq!(gain_string(0.00), "0.00 dB");
        assert_eq!(gain_string(0.18), "0.18 dB");
        assert_eq!(gain_string(12.5), "12.50 dB");
        assert_eq!(peak_string(0.044006), "0.044006");
        assert_eq!(peak_string(0.0), "0.000000");
        assert_eq!(peak_string(1.0), "1.000000");
    }

    #[test]
    fn track_atoms_follow_l2_order_and_keys() {
        let atoms = track_atoms(9.78, 0.044006, Some((12.50, 0.044464))).unwrap();
        assert_eq!(atoms.len(), 5);
        let names: Vec<String> = atoms
            .iter()
            .map(|atom| freeform_name(atom).expect("test helper parses test output"))
            .collect();
        assert_eq!(
            names,
            [
                TRACK_GAIN_KEY,
                TRACK_PEAK_KEY,
                ALBUM_GAIN_KEY,
                ALBUM_PEAK_KEY,
                ITUNNORM_KEY
            ]
        );
        assert_eq!(freeform_value(&atoms[0]).unwrap(), "9.78 dB".to_string());
        // iTunNORM derives from the rendered (quantized) track gain.
        let expected = soundcheck_string(9.78);
        assert_eq!(freeform_value(&atoms[4]).unwrap(), expected);
    }

    #[test]
    fn track_atoms_omit_album_level_when_incomplete() {
        let atoms = track_atoms(9.78, 0.044006, None).unwrap();
        assert_eq!(atoms.len(), 3);
        let names: Vec<String> = atoms
            .iter()
            .map(|atom| freeform_name(atom).expect("test helper parses test output"))
            .collect();
        assert_eq!(names, [TRACK_GAIN_KEY, TRACK_PEAK_KEY, ITUNNORM_KEY]);
    }

    /// Parse the `name` payload of a `----` atom produced above.
    fn freeform_name(atom: &[u8]) -> Option<String> {
        let mut offset = 8;
        while offset + 8 <= atom.len() {
            let size = u32::from_be_bytes(atom[offset..offset + 4].try_into().ok()?) as usize;
            let kind = &atom[offset + 4..offset + 8];
            if kind == b"name" {
                return String::from_utf8(atom[offset + 12..offset + size].to_vec()).ok();
            }
            offset += size;
        }
        None
    }

    /// Parse the UTF-8 `data` payload of a `----` atom produced above.
    fn freeform_value(atom: &[u8]) -> Option<String> {
        let mut offset = 8;
        while offset + 8 <= atom.len() {
            let size = u32::from_be_bytes(atom[offset..offset + 4].try_into().ok()?) as usize;
            let kind = &atom[offset + 4..offset + 8];
            if kind == b"data" {
                assert_eq!(&atom[offset + 8..offset + 12], &[0, 0, 0, 1]);
                assert_eq!(&atom[offset + 12..offset + 16], &[0, 0, 0, 0]);
                return String::from_utf8(atom[offset + 16..offset + size].to_vec()).ok();
            }
            offset += size;
        }
        None
    }
}
