//! Core-9 M4A text metadata (M1 slice) plus cover art (S1.3).
//!
//! Reads Vorbis comments through Symphonia's metadata log and maps the
//! G0-approved core keys to M4A text atoms plus `trkn`/`disk` tuples.
//! Everything else is counted as unmapped, never an error.
//!
//! Cover art (S1.3): at most one JPEG/PNG image from the probed FLAC
//! pictures (`revision.media.visuals`) is serialized as a `covr` ilst child.
//! Selection is deterministic (usage tier, then emission order); invalid,
//! unsupported, or oversized candidates are skipped with warnings, never
//! errors.
//!
//! Owner rules (G0 §2–§3, revised metadata policy):
//!
//! - Vorbis keys matched ASCII case-insensitively; values trimmed; the stored
//!   value is the trimmed value. Empty/whitespace-only values are treated as
//!   absent.
//! - Missing optional fields are silent. No tag slot in this converter is
//!   required, so absence never produces a warning. Warnings are reserved for
//!   malformed, invalid, unsupported or genuinely lossy metadata.
//! - Mapped text atoms are single-valued in M4A, but multiple distinct source
//!   values are preserved in source order inside that one text atom, joined
//!   with `; `. Empty values are ignored and exact duplicates are removed
//!   while retaining the first occurrence. This is a pragmatic readable
//!   mapping, not a lossless representation of the Vorbis comment structure.
//! - `tracknumber`/`discnumber` are numeric tuples and are not joined.
//!   Multiple distinct values keep the first and warn about the lossy
//!   reduction. They accept `N` or `N/M` (whitespace around `/` tolerated);
//!   totals fall back to `tracktotal`/`totaltracks` and
//!   `disctotal`/`totaldiscs`; unparseable numbers skip the atom with a
//!   warning, never a failure. Zero totals are written as `0` (unknown).
//! - `date` wins over `year` for `©day`; the atom is written at most once and
//!   stored verbatim (no reformatting). Multiple values of the selected key
//!   follow the same `; ` text-atom rule; `date` and `year` are never joined
//!   with each other.
//! - All text atoms use M4A data type UTF-8. No integer/boolean/freeform
//!   encoding in this slice.
//! - Absent artwork is silent. Invalid, unsupported, empty, or oversized
//!   artwork candidates still warn; selection never fails a conversion.

use std::collections::BTreeSet;
use std::path::Path;

use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, RawValue, StandardVisualKey, Visual};

use crate::gapless::boxed;
use crate::ConvertError;

/// One Vorbis comment value as surfaced by Symphonia.
#[derive(Debug, Clone)]
pub(crate) enum TagValue {
    /// UTF-8 text (Symphonia lossy-decodes comment bytes, so this is the
    /// normal case for Vorbis comments).
    Text(String),
    /// A non-string value (binary, integer, …). Vorbis comments are
    /// text-only, so this is defensive: the value is skipped with a warning.
    Opaque,
}

/// Approved core metadata for one file, plus reporting.
#[derive(Debug, Clone, Default)]
pub(crate) struct TagSet {
    pub(crate) title: Option<String>,
    pub(crate) artist: Option<String>,
    pub(crate) album: Option<String>,
    pub(crate) track: Option<(u16, u16)>,
    pub(crate) disc: Option<(u16, u16)>,
    pub(crate) albumartist: Option<String>,
    pub(crate) date: Option<String>,
    pub(crate) genre: Option<String>,
    pub(crate) composer: Option<String>,
    pub(crate) comment: Option<String>,
    /// Selected cover art, if any. Excluded from [`mapped_count`](TagSet::mapped_count):
    /// `mapped` remains a text-tag count (0–10).
    pub(crate) cover: Option<CoverArt>,
    /// Distinct unmapped (deferred or unknown) keys, lowercased and sorted.
    pub(crate) unmapped: Vec<String>,
    /// Per-file warnings. Warnings never fail the conversion.
    pub(crate) warnings: Vec<String>,
}

/// Vorbis keys that feed a mapped atom (lowercased). Anything else counts as
/// unmapped, including every deferred key from G0 §6.
const RECOGNIZED_KEYS: &[&str] = &[
    "title",
    "artist",
    "album",
    "tracknumber",
    "tracktotal",
    "totaltracks",
    "discnumber",
    "disctotal",
    "totaldiscs",
    "albumartist",
    "date",
    "year",
    "genre",
    "composer",
    "comment",
];

impl TagSet {
    /// Number of M4A text atoms this set will write (each `Some` text field
    /// is one). Cover art is excluded: `mapped` stays a 0–10 text-tag count.
    pub(crate) fn mapped_count(&self) -> usize {
        let mut count = 0;
        for present in [
            self.title.is_some(),
            self.artist.is_some(),
            self.album.is_some(),
            self.track.is_some(),
            self.disc.is_some(),
            self.albumartist.is_some(),
            self.date.is_some(),
            self.genre.is_some(),
            self.composer.is_some(),
            self.comment.is_some(),
        ] {
            if present {
                count += 1;
            }
        }
        count
    }

    /// Serialize every mapped atom as a complete `ilst` child box, in G0
    /// table order with `covr` last. An empty set yields an empty vector
    /// (no container change).
    pub(crate) fn ilst_children(&self) -> Result<Vec<Vec<u8>>, ConvertError> {
        let mut children = Vec::new();
        if let Some(value) = &self.title {
            children.push(text_atom(b"\xa9nam", value)?);
        }
        if let Some(value) = &self.artist {
            children.push(text_atom(b"\xa9ART", value)?);
        }
        if let Some(value) = &self.album {
            children.push(text_atom(b"\xa9alb", value)?);
        }
        if let Some((number, total)) = self.track {
            children.push(pair_atom(b"trkn", number, total, true)?);
        }
        if let Some((number, total)) = self.disc {
            children.push(pair_atom(b"disk", number, total, false)?);
        }
        if let Some(value) = &self.albumartist {
            children.push(text_atom(b"aART", value)?);
        }
        if let Some(value) = &self.date {
            children.push(text_atom(b"\xa9day", value)?);
        }
        if let Some(value) = &self.genre {
            children.push(text_atom(b"\xa9gen", value)?);
        }
        if let Some(value) = &self.composer {
            children.push(text_atom(b"\xa9wrt", value)?);
        }
        if let Some(value) = &self.comment {
            children.push(text_atom(b"\xa9cmt", value)?);
        }
        if let Some(cover) = &self.cover {
            children.push(covr_atom(cover)?);
        }
        Ok(children)
    }
}

/// Maximum embedded image size (S1.3 approval): 16 MiB per image.
///
/// Enforced in [`select_cover`] before atom serialization, so oversized
/// images are skipped with a warning instead of reaching the serializer.
/// Everything that passes this check fits `u32` box arithmetic trivially.
pub(crate) const COVER_MAX_BYTES: usize = 16 * 1024 * 1024;

/// Image formats accepted for M4A cover art (S1.3): JPEG and PNG only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CoverFormat {
    Jpeg,
    Png,
}

impl CoverFormat {
    /// MP4 `data` box type: 13 for JPEG, 14 for PNG.
    fn data_type(self) -> u32 {
        match self {
            Self::Jpeg => 13,
            Self::Png => 14,
        }
    }

    /// File extension used for external cover-art extraction.
    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
        }
    }
}

/// Selected cover art: format plus the original image bytes, unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoverArt {
    pub(crate) format: CoverFormat,
    pub(crate) data: Vec<u8>,
}

impl CoverArt {
    /// Conventional external file name for this image format.
    pub(crate) fn file_name(&self) -> String {
        format!("cover.{}", self.format.extension())
    }
}

/// Detect JPEG/PNG by magic bytes only; never decode the image.
///
/// `media_type` is deliberately ignored: it is source-supplied and is
/// missing or wrong on some paths (`COVERART`, untyped blocks).
fn sniff_cover_format(data: &[u8]) -> Option<CoverFormat> {
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        Some(CoverFormat::Jpeg)
    } else if data.len() >= 8 && data[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        Some(CoverFormat::Png)
    } else {
        None
    }
}

/// Selection tier for one picture: front cover first, then other typed
/// pictures (including `Other`), then untyped pictures (`COVERART`,
/// unknown picture types). Lower wins.
fn cover_tier(usage: Option<StandardVisualKey>) -> u8 {
    match usage {
        Some(StandardVisualKey::FrontCover) => 0,
        Some(_) => 1,
        None => 2,
    }
}

/// Select at most one cover image from `visuals` in tier, then emission,
/// order.
///
/// Pure and total like the tag slots: never fails, only records warnings.
/// Each skipped candidate warns individually; absent artwork is silent
/// (cover art is optional). Callers with `--no-tags` skip selection entirely
/// and warn nothing.
fn select_cover(visuals: &[Visual]) -> (Option<CoverArt>, Vec<String>) {
    let mut warnings = Vec::new();
    for tier in [0u8, 1, 2] {
        for visual in visuals {
            if cover_tier(visual.usage) != tier {
                continue;
            }
            let data = &visual.data;
            if data.is_empty() {
                warnings.push("empty cover image: skipped".to_string());
                continue;
            }
            let Some(format) = sniff_cover_format(data) else {
                warnings.push("unsupported cover image format: skipped".to_string());
                continue;
            };
            if data.len() > COVER_MAX_BYTES {
                warnings.push(format!(
                    "cover image too large ({} bytes, limit {}): skipped",
                    data.len(),
                    COVER_MAX_BYTES
                ));
                continue;
            }
            return (
                Some(CoverArt {
                    format,
                    data: data.to_vec(),
                }),
                warnings,
            );
        }
    }
    (None, warnings)
}

/// One `covr` atom holding a single image `data` box (S1.3).
///
/// Layout mirrors mutagen's `__render_cover` (`data`: image-format type 13
/// or 14, locale `0`, original image bytes unchanged). No `mean`/`name`
/// wrapper. Over-cap input is rejected here as defense in depth (selection
/// enforces the cap first); oversized atoms can never reach the checked
/// `u32` arithmetic in `boxed()`.
fn covr_atom(cover: &CoverArt) -> Result<Vec<u8>, ConvertError> {
    if cover.data.len() > COVER_MAX_BYTES {
        return Err(ConvertError::Metadata("cover art exceeds size limit"));
    }
    let mut payload = Vec::with_capacity(8 + cover.data.len());
    payload.extend_from_slice(&cover.format.data_type().to_be_bytes());
    payload.extend_from_slice(&0u32.to_be_bytes()); // locale
    payload.extend_from_slice(&cover.data);
    let data =
        boxed(b"data", &payload).map_err(|_| ConvertError::Metadata("cover art too large"))?;
    boxed(b"covr", &data).map_err(|_| ConvertError::Metadata("cover art too large"))
}

/// Open `path` and probe only its metadata container.
///
/// Any probe failure maps to [`ConvertError::Metadata`]: callers have already
/// decoded the file successfully (or are reading metadata as an optional
/// pre-pass), so an unreadable comment block is a metadata-phase failure.
fn probe_format(
    path: &Path,
) -> Result<Box<dyn symphonia::core::formats::FormatReader>, ConvertError> {
    let file =
        std::fs::File::open(path).map_err(|_| ConvertError::Metadata("could not read tags"))?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("flac");
    symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|_| ConvertError::Metadata("could not read tags"))
}

/// Read Vorbis comments from `path` through Symphonia and build the core set.
///
/// Only the metadata log is read; no audio is decoded. Cover selection runs on
/// the same probed revision when `select_cover_art` is set; external-only
/// extraction passes `true` so cover diagnostics still surface, then drops the
/// selected image before atom serialization. `--no-tags` passes `false` and
/// skips artwork entirely.
pub(crate) fn read_tags(path: &Path, select_cover_art: bool) -> Result<TagSet, ConvertError> {
    let mut format = probe_format(path)?;

    let mut pairs: Vec<(String, TagValue)> = Vec::new();
    let mut cover: Option<CoverArt> = None;
    let mut cover_warnings: Vec<String> = Vec::new();
    let mut metadata = format.metadata();
    if let Some(revision) = metadata.skip_to_latest() {
        for tag in &revision.media.tags {
            let key = tag.raw.key.clone();
            match &tag.raw.value {
                RawValue::String(value) => {
                    pairs.push((key, TagValue::Text(value.to_string())));
                }
                RawValue::StringList(values) => {
                    for value in values.iter() {
                        pairs.push((key.clone(), TagValue::Text(value.to_string())));
                    }
                }
                _ => pairs.push((key, TagValue::Opaque)),
            }
        }
        if select_cover_art {
            let (selected, warnings) = select_cover(&revision.media.visuals);
            cover = selected;
            cover_warnings = warnings;
        }
    }
    let mut set = build_tag_set(&pairs);
    set.cover = cover;
    // Tag warnings keep their order; cover warnings follow (S1.3 F1). The
    // 24-bit precision notice, if any, is inserted ahead of both by the
    // caller (`convert_inner`).
    set.warnings.extend(cover_warnings);
    Ok(set)
}

/// Read only the selected cover image from `path`.
///
/// Used by external cover-art extraction and its batch conflict pre-scan.
/// Text tags are not materialized. A missing image returns `None` with no
/// warning; invalid or unsupported candidates retain their selection warnings.
pub(crate) fn read_cover(path: &Path) -> Result<(Option<CoverArt>, Vec<String>), ConvertError> {
    let mut format = probe_format(path)?;
    let mut metadata = format.metadata();
    let Some(revision) = metadata.skip_to_latest() else {
        return Ok((None, Vec::new()));
    };
    Ok(select_cover(&revision.media.visuals))
}

/// Build the core set from `(key, value)` pairs in encounter order.
///
/// Keys are matched ASCII case-insensitively. This is pure and total: it
/// never fails, only records warnings.
pub(crate) fn build_tag_set(pairs: &[(String, TagValue)]) -> TagSet {
    let lowered: Vec<(String, &TagValue)> = pairs
        .iter()
        .map(|(key, value)| (key.to_ascii_lowercase(), value))
        .collect();

    let mut set = TagSet::default();

    set.title = text_slot(&lowered, "title", &mut set.warnings);
    set.artist = text_slot(&lowered, "artist", &mut set.warnings);
    set.album = text_slot(&lowered, "album", &mut set.warnings);
    let (track, track_warnings) =
        pair_slot(&lowered, "tracknumber", &["tracktotal", "totaltracks"]);
    set.warnings.extend(track_warnings);
    set.track = track;
    let (disc, disc_warnings) = pair_slot(&lowered, "discnumber", &["disctotal", "totaldiscs"]);
    set.warnings.extend(disc_warnings);
    set.disc = disc;
    set.albumartist = text_slot(&lowered, "albumartist", &mut set.warnings);
    set.date = date_slot(&lowered, &mut set.warnings);
    set.genre = text_slot(&lowered, "genre", &mut set.warnings);
    set.composer = text_slot(&lowered, "composer", &mut set.warnings);
    set.comment = text_slot(&lowered, "comment", &mut set.warnings);

    let mut unmapped = BTreeSet::new();
    for (key, _) in &lowered {
        if !RECOGNIZED_KEYS.contains(&key.as_str()) {
            unmapped.insert(key.clone());
        }
    }
    set.unmapped = unmapped.into_iter().collect();
    set
}

/// Distinct non-empty trimmed text values for `key`, in source order.
///
/// Duplicate values are removed while retaining the first occurrence. Empty
/// and whitespace-only values are ignored. Opaque values are not text and are
/// reported separately by [`has_opaque`].
fn text_values(pairs: &[(String, &TagValue)], key: &str) -> Vec<String> {
    let mut values = Vec::new();
    for (candidate, value) in pairs {
        if candidate != key {
            continue;
        }
        if let TagValue::Text(text) = value {
            let trimmed = text.trim();
            if !trimmed.is_empty() && !values.iter().any(|value| value == trimmed) {
                values.push(trimmed.to_string());
            }
        }
    }
    values
}

/// True when `key` has at least one non-text value.
fn has_opaque(pairs: &[(String, &TagValue)], key: &str) -> bool {
    pairs
        .iter()
        .any(|(candidate, value)| candidate == key && matches!(value, TagValue::Opaque))
}

/// One M4A text atom from all distinct non-empty values of `key`.
///
/// Multiple values are joined with `; ` in source order. This is a readable
/// single-atom mapping, not a lossless model of the original Vorbis comment
/// list. A missing key is silent; an unsupported opaque value warns.
fn text_slot(
    pairs: &[(String, &TagValue)],
    key: &str,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let values = text_values(pairs, key);
    if has_opaque(pairs, key) {
        warnings.push(format!("unsupported {key} value: skipped"));
    }
    if values.is_empty() {
        None
    } else {
        Some(values.join("; "))
    }
}

/// `date`-wins-over-`year` shared slot (G0 §2 row 7), with the same
/// multi-value text policy.
///
/// The two source keys are never joined with each other: `date` is used when
/// it has any non-empty value, otherwise `year`. Absence is silent.
fn date_slot(pairs: &[(String, &TagValue)], warnings: &mut Vec<String>) -> Option<String> {
    let dates = text_values(pairs, "date");
    let years = text_values(pairs, "year");
    if has_opaque(pairs, "date") || has_opaque(pairs, "year") {
        warnings.push("unsupported date/year value: skipped".to_string());
    }
    let values = if dates.is_empty() { years } else { dates };
    if values.is_empty() {
        None
    } else {
        Some(values.join("; "))
    }
}

/// `trkn`/`disk` tuple slot: `N` or `N/M`, then fallback totals.
///
/// Numeric fields are not text-joined. Multiple distinct primary values keep
/// the first and warn because the numeric tuple cannot represent the rest.
/// Returns the tuple (or `None` when the atom must be skipped) plus warnings.
/// An unparseable number skips the whole atom, never the file.
#[allow(clippy::type_complexity)]
fn pair_slot(
    pairs: &[(String, &TagValue)],
    key: &str,
    fallback_keys: &[&str],
) -> (Option<(u16, u16)>, Vec<String>) {
    let mut warnings = Vec::new();
    let values = text_values(pairs, key);
    if has_opaque(pairs, key) {
        warnings.push(format!("unsupported {key} value: skipped"));
    }
    let Some(raw) = values.first() else {
        return (None, warnings);
    };
    if values.len() > 1 {
        warnings.push(format!(
            "multi-valued {key}: using first value '{raw}' (numeric fields are not joined)"
        ));
    }
    let raw = raw.as_str();

    let (number_part, slash_total) = match raw.split_once('/') {
        Some((number, total)) => (number.trim(), Some(total.trim())),
        None => (raw, None),
    };
    let number: u16 = match number_part.parse() {
        Ok(number) => number,
        Err(_) => {
            warnings.push(format!("invalid {key} '{raw}': skipped"));
            return (None, warnings);
        }
    };

    let total = if let Some(slash) = slash_total {
        match slash.parse::<u16>() {
            Ok(total) => total,
            Err(_) => {
                warnings.push(format!("invalid {key} '{raw}': skipped"));
                return (None, warnings);
            }
        }
    } else {
        fallback_total(pairs, fallback_keys, &mut warnings)
    };
    (Some((number, total)), warnings)
}

/// First parseable fallback total, else `0` (unknown).
///
/// A present-but-unparseable fallback warns once and falls through to the
/// next key; a wholly absent fallback is silent (totals are commonly absent).
/// Multiple distinct fallback values keep the first parseable one and warn,
/// matching the numeric no-join policy.
fn fallback_total(
    pairs: &[(String, &TagValue)],
    fallback_keys: &[&str],
    warnings: &mut Vec<String>,
) -> u16 {
    let mut warned_unparseable = false;
    for fallback in fallback_keys {
        let values = text_values(pairs, fallback);
        for value in &values {
            if let Ok(total) = value.parse::<u16>() {
                if values.len() > 1 {
                    warnings.push(format!(
                        "multi-valued {fallback}: using first parseable value '{total}' \
                         (numeric fields are not joined)"
                    ));
                }
                return total;
            }
            if !warned_unparseable {
                warnings.push(format!("invalid {fallback} '{value}': ignored"));
                warned_unparseable = true;
            }
        }
    }
    0
}

/// One M4A text atom: `name` box holding a single UTF-8 `data` box.
///
/// Layout mirrors mutagen's `__render_text` (`data`: version/flags `0/1`,
/// locale `0`, UTF-8 value) so independent readers accept the output.
fn text_atom(name: &[u8; 4], value: &str) -> Result<Vec<u8>, ConvertError> {
    let mut payload = Vec::with_capacity(8 + value.len());
    payload.extend_from_slice(&1u32.to_be_bytes()); // data type: UTF-8
    payload.extend_from_slice(&0u32.to_be_bytes()); // locale
    payload.extend_from_slice(value.as_bytes());
    let data =
        boxed(b"data", &payload).map_err(|_| ConvertError::Metadata("tag data too large"))?;
    boxed(name, &data).map_err(|_| ConvertError::Metadata("tag data too large"))
}

/// Encoder identification atom for every M4A output: `©too` (Tool) with
/// `Ferrite <version>`.
///
/// This is technical output metadata, not a copy of source tags: the value is
/// built from Cargo package metadata (`CARGO_PKG_VERSION`) and is written by
/// the muxing path for every M4A file, including `--no-tags`. Source Vorbis
/// comments cannot produce this atom (no source key maps to `©too`), so a
/// source `encoder`/`tool` tag can never overwrite it. ADTS has no metadata
/// container and never carries it.
pub(crate) fn encoder_identification_atom() -> Result<Vec<u8>, ConvertError> {
    text_atom(
        b"\xa9too",
        &format!("Ferrite {}", env!("CARGO_PKG_VERSION")),
    )
}

/// One `trkn`/`disk` number-pair atom.
///
/// Value layout mirrors mutagen (`__render_pair` / `__render_pair_no_trailing`):
/// `trkn` carries 8 bytes `00 00 N N M M 00 00`, `disk` carries 6 bytes
/// `00 00 N N M M`, both under an IMPLICIT-type (`0`) `data` box.
fn pair_atom(
    name: &[u8; 4],
    number: u16,
    total: u16,
    trailing_pad: bool,
) -> Result<Vec<u8>, ConvertError> {
    let mut value = Vec::with_capacity(8);
    value.extend_from_slice(&0u16.to_be_bytes());
    value.extend_from_slice(&number.to_be_bytes());
    value.extend_from_slice(&total.to_be_bytes());
    if trailing_pad {
        value.extend_from_slice(&0u16.to_be_bytes());
    }
    let mut payload = Vec::with_capacity(8 + value.len());
    payload.extend_from_slice(&0u32.to_be_bytes()); // version/flags: IMPLICIT
    payload.extend_from_slice(&0u32.to_be_bytes()); // locale
    payload.extend_from_slice(&value);
    let data =
        boxed(b"data", &payload).map_err(|_| ConvertError::Metadata("tag data too large"))?;
    boxed(name, &data).map_err(|_| ConvertError::Metadata("tag data too large"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(entries: &[(&str, &str)]) -> Vec<(String, TagValue)> {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), TagValue::Text(value.to_string())))
            .collect()
    }

    #[test]
    fn multi_value_text_atoms_join_in_source_order() {
        let set = build_tag_set(&pairs(&[
            ("TITLE", "  "),
            ("title", "First Title"),
            ("Title", "Second Title"),
            ("ARTIST", "Artist B"),
            ("artist", "Artist A"),
            ("ARTIST", "Artist B"),
            ("GENRE", "Genre One"),
            ("genre", "Genre Two"),
            ("COMPOSER", "Composer One"),
            ("composer", "Composer Two"),
        ]));
        assert_eq!(set.title.as_deref(), Some("First Title; Second Title"));
        assert_eq!(set.artist.as_deref(), Some("Artist B; Artist A"));
        assert_eq!(set.genre.as_deref(), Some("Genre One; Genre Two"));
        assert_eq!(set.composer.as_deref(), Some("Composer One; Composer Two"));
        assert!(
            set.warnings.is_empty(),
            "preserving multiple text values must not warn: {:?}",
            set.warnings
        );
    }

    #[test]
    fn multi_value_text_drops_empty_and_repeated_values() {
        let set = build_tag_set(&pairs(&[
            ("artist", ""),
            ("artist", "  Artist A  "),
            ("artist", "   "),
            ("artist", "Artist A"),
            ("artist", "Artist B"),
            ("artist", "Artist A"),
        ]));
        assert_eq!(set.artist.as_deref(), Some("Artist A; Artist B"));
        assert!(set.warnings.is_empty(), "{:?}", set.warnings);
    }

    #[test]
    fn numeric_fields_do_not_join_and_warn_about_reduction() {
        let set = build_tag_set(&pairs(&[
            ("tracknumber", "7"),
            ("tracknumber", "8"),
            ("tracktotal", "9"),
            ("discnumber", "1"),
            ("discnumber", "2"),
            ("disctotal", "3"),
        ]));
        assert_eq!(set.track, Some((7, 9)));
        assert_eq!(set.disc, Some((1, 3)));
        assert!(
            set.warnings
                .iter()
                .any(|w| w.contains("multi-valued tracknumber") && w.contains("'7'")),
            "{:?}",
            set.warnings
        );
        assert!(
            set.warnings
                .iter()
                .any(|w| w.contains("multi-valued discnumber") && w.contains("'1'")),
            "{:?}",
            set.warnings
        );
    }

    #[test]
    fn opaque_values_mixed_with_text_warn_but_keep_text() {
        let mut entries = pairs(&[("artist", "Artist A"), ("artist", "Artist B")]);
        entries.push(("artist".to_string(), TagValue::Opaque));
        let set = build_tag_set(&entries);
        assert_eq!(set.artist.as_deref(), Some("Artist A; Artist B"));
        assert!(
            set.warnings
                .iter()
                .any(|w| w.contains("unsupported artist")),
            "{:?}",
            set.warnings
        );
    }

    #[test]
    fn duplicate_text_values_do_not_trigger_a_warning() {
        let set = build_tag_set(&pairs(&[
            ("artist", "Same Artist"),
            ("ARTIST", " Same Artist "),
            ("albumartist", "Same Album Artist"),
            ("ALBUMARTIST", "Same Album Artist"),
        ]));
        assert_eq!(set.artist.as_deref(), Some("Same Artist"));
        assert_eq!(set.albumartist.as_deref(), Some("Same Album Artist"));
        assert!(set.warnings.is_empty(), "{:?}", set.warnings);
    }

    #[test]
    fn missing_optional_fields_are_silent() {
        let set = build_tag_set(&[]);
        assert_eq!(set.mapped_count(), 0);
        assert!(set.ilst_children().unwrap().is_empty());
        assert!(
            set.warnings.is_empty(),
            "missing optional tags must not warn: {:?}",
            set.warnings
        );
    }

    #[test]
    fn whitespace_only_values_count_as_absent_without_warning() {
        let set = build_tag_set(&pairs(&[("title", "   "), ("genre", "\t")]));
        assert_eq!(set.title, None);
        assert_eq!(set.genre, None);
        assert!(set.warnings.is_empty(), "{:?}", set.warnings);
    }

    #[test]
    fn unicode_values_trim_and_round_trip_as_utf8() {
        // Multi-byte values trim on character boundaries and serialize as the
        // trimmed UTF-8 bytes; first-non-empty still applies across entries.
        let set = build_tag_set(&pairs(&[
            ("title", "   "),
            ("title", "  café 🎵  "),
            ("comment", "\tcrème brûlée\t"),
        ]));
        assert_eq!(set.title.as_deref(), Some("café 🎵"));
        assert_eq!(set.comment.as_deref(), Some("crème brûlée"));

        let children = set.ilst_children().unwrap();
        assert_eq!(children.len(), 2);
        // G0 table order: title before comment.
        assert_eq!(&children[0][4..8], b"\xa9nam");
        assert_eq!(&children[1][4..8], b"\xa9cmt");
        for (child, expected) in [&children[0], &children[1]]
            .into_iter()
            .zip(["café 🎵", "crème brûlée"])
        {
            // data box: size + `data`, type UTF-8 (1), locale 0, value bytes.
            assert_eq!(&child[12..16], b"data");
            assert_eq!(&child[16..20], &[0, 0, 0, 1]);
            assert_eq!(&child[20..24], &[0, 0, 0, 0]);
            assert_eq!(&child[24..], expected.as_bytes());
        }
    }

    #[test]
    fn date_wins_over_year_and_never_duplicates() {
        let both = build_tag_set(&pairs(&[("year", "1999"), ("date", "2024-03-07")]));
        assert_eq!(both.date.as_deref(), Some("2024-03-07"));

        let year_only = build_tag_set(&pairs(&[("YEAR", "2001")]));
        assert_eq!(year_only.date.as_deref(), Some("2001"));

        let neither = build_tag_set(&[]);
        assert_eq!(neither.date, None);
        assert!(neither.warnings.is_empty(), "{:?}", neither.warnings);
    }

    #[test]
    fn multiple_dates_join_but_date_and_year_never_mix() {
        let dates = build_tag_set(&pairs(&[
            ("date", "2024-03-07"),
            ("date", "2023-01-01"),
            ("year", "1999"),
        ]));
        assert_eq!(dates.date.as_deref(), Some("2024-03-07; 2023-01-01"));
        assert!(dates.warnings.is_empty(), "{:?}", dates.warnings);

        let years = build_tag_set(&pairs(&[("year", "1999"), ("year", "2000")]));
        assert_eq!(years.date.as_deref(), Some("1999; 2000"));
        assert!(years.warnings.is_empty(), "{:?}", years.warnings);
    }

    #[test]
    fn opaque_values_warn_as_unsupported() {
        let set = build_tag_set(&[("title".to_string(), TagValue::Opaque)]);
        assert_eq!(set.title, None);
        assert!(set.warnings.iter().any(|w| w.contains("unsupported title")));
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn track_disc_tuple_grammar() {
        // (number input, fallback pairs, expected tuple, expected warning fragment)
        let cases: &[(&str, &[(&str, &str)], Option<(u16, u16)>, Option<&str>)] = &[
            ("5", &[], Some((5, 0)), None),
            ("5/12", &[], Some((5, 12)), None),
            ("05/12", &[], Some((5, 12)), None),
            (" 5 / 12 ", &[], Some((5, 12)), None),
            ("7", &[("tracktotal", "9")], Some((7, 9)), None),
            ("7", &[("totaltracks", "4")], Some((7, 4)), None),
            (
                "7",
                &[("tracktotal", "x"), ("totaltracks", "4")],
                Some((7, 4)),
                Some("tracktotal"),
            ),
            ("0/0", &[], Some((0, 0)), None),
            ("65535/65535", &[], Some((65535, 65535)), None),
            ("5/x", &[], None, Some("invalid tracknumber")),
            ("x", &[], None, Some("invalid tracknumber")),
            ("", &[], None, None),
            ("/12", &[], None, Some("invalid tracknumber")),
            ("70000", &[], None, Some("invalid tracknumber")),
            ("-1", &[], None, Some("invalid tracknumber")),
            ("1/2/3", &[], None, Some("invalid tracknumber")),
        ];
        for (input, fallbacks, expected, warns) in cases {
            let mut entries = vec![("tracknumber", *input)];
            entries.extend_from_slice(fallbacks);
            let set = build_tag_set(&pairs(&entries));
            assert_eq!(set.track, *expected, "input={input:?}");
            match warns {
                Some(fragment) => assert!(
                    set.warnings.iter().any(|w| w.contains(fragment)),
                    "input={input:?} warnings={:?}",
                    set.warnings
                ),
                None => assert!(
                    !set.warnings.iter().any(|w| w.contains("tracknumber")
                        || w.contains("tracktotal")
                        || w.contains("totaltracks")),
                    "input={input:?} warnings={:?}",
                    set.warnings
                ),
            }
        }
    }

    #[test]
    fn disc_tuple_and_fallbacks() {
        let set = build_tag_set(&pairs(&[("discnumber", "2"), ("disctotal", "3")]));
        assert_eq!(set.disc, Some((2, 3)));

        let slash = build_tag_set(&pairs(&[("DISCNUMBER", "1/2")]));
        assert_eq!(slash.disc, Some((1, 2)));

        let bad = build_tag_set(&pairs(&[("discnumber", "3/x")]));
        assert_eq!(bad.disc, None);
        assert!(bad.warnings.iter().any(|w| w.contains("discnumber")));
    }

    #[test]
    fn unmapped_keys_are_sorted_distinct_and_lowercased() {
        let set = build_tag_set(&pairs(&[
            ("title", "T"),
            ("BPM", "120"),
            ("bpm", "121"),
            ("compilation", "1"),
            ("MusicBrainz_TrackId", "x"),
            ("GROUPING", "g"),
        ]));
        assert_eq!(set.title.as_deref(), Some("T"));
        assert_eq!(
            set.unmapped,
            vec!["bpm", "compilation", "grouping", "musicbrainz_trackid"]
        );
    }

    #[test]
    fn ilst_children_follow_g0_order_and_mutagen_layout() {
        let set = build_tag_set(&pairs(&[
            ("comment", "C"),
            ("title", "T"),
            ("tracknumber", "5/12"),
            ("discnumber", "1/2"),
        ]));
        assert_eq!(set.mapped_count(), 4);
        let children = set.ilst_children().unwrap();
        assert_eq!(children.len(), 4);

        // Order: title, track, disc, comment (G0 table order).
        assert_eq!(&children[0][4..8], b"\xa9nam");
        assert_eq!(&children[1][4..8], b"trkn");
        assert_eq!(&children[2][4..8], b"disk");
        assert_eq!(&children[3][4..8], b"\xa9cmt");

        // Text data box: type UTF-8 (1), locale 0, then the value.
        let title_data = &children[0][8..];
        assert_eq!(&title_data[4..8], b"data");
        assert_eq!(&title_data[8..12], &[0, 0, 0, 1]);
        assert_eq!(&title_data[12..16], &[0, 0, 0, 0]);
        assert_eq!(&title_data[16..], b"T");

        // trkn value: 00 00 N N M M 00 00 (mutagen __render_pair).
        let trkn_data = &children[1][8..];
        assert_eq!(&trkn_data[8..12], &[0, 0, 0, 0]);
        assert_eq!(&trkn_data[16..24], &[0, 0, 0, 5, 0, 12, 0, 0]);

        // disk value: 00 00 N N M M (mutagen __render_pair_no_trailing).
        let disk_data = &children[2][8..];
        assert_eq!(&disk_data[16..22], &[0, 0, 0, 1, 0, 2]);
    }

    // ------------------------------------------------------------------
    // Cover art (S1.3).
    // ------------------------------------------------------------------

    const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00];
    const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
    const GIF_MAGIC: &[u8] = b"GIF89a\x01\x00";

    fn visual(usage: Option<StandardVisualKey>, data: &[u8]) -> Visual {
        Visual {
            media_type: None,
            dimensions: None,
            color_mode: None,
            usage,
            tags: Vec::new(),
            data: data.into(),
        }
    }

    fn front(data: &[u8]) -> Visual {
        visual(Some(StandardVisualKey::FrontCover), data)
    }

    fn back(data: &[u8]) -> Visual {
        visual(Some(StandardVisualKey::BackCover), data)
    }

    fn other(data: &[u8]) -> Visual {
        visual(Some(StandardVisualKey::Other), data)
    }

    fn untyped(data: &[u8]) -> Visual {
        visual(None, data)
    }

    #[test]
    fn sniff_cover_format_matrix() {
        assert_eq!(sniff_cover_format(JPEG_MAGIC), Some(CoverFormat::Jpeg));
        assert_eq!(sniff_cover_format(PNG_MAGIC), Some(CoverFormat::Png));
        // Short prefixes that cannot be valid images are rejected.
        assert_eq!(sniff_cover_format(&[0xFF]), None);
        assert_eq!(sniff_cover_format(&PNG_MAGIC[..7]), None);
        // Anything else — GIF, BMP, random bytes, empty — is unsupported.
        assert_eq!(sniff_cover_format(GIF_MAGIC), None);
        assert_eq!(sniff_cover_format(b"BM\x00\x01\x02\x03"), None);
        assert_eq!(sniff_cover_format(b"not an image"), None);
        assert_eq!(sniff_cover_format(&[]), None);
    }

    #[test]
    fn cover_media_type_is_never_trusted() {
        // A lying media type must not override magic bytes in either direction.
        let mut lying_jpeg = visual(Some(StandardVisualKey::FrontCover), JPEG_MAGIC);
        lying_jpeg.media_type = Some("image/gif".to_string());
        let (cover, warnings) = select_cover(&[lying_jpeg]);
        let cover = cover.expect("magic bytes decide, not media_type");
        assert_eq!(cover.format, CoverFormat::Jpeg);
        assert!(warnings.is_empty(), "{warnings:?}");

        let mut lying_gif = visual(Some(StandardVisualKey::FrontCover), GIF_MAGIC);
        lying_gif.media_type = Some("image/jpeg".to_string());
        let (cover, warnings) = select_cover(&[lying_gif]);
        assert_eq!(cover, None);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("unsupported cover image format")),
            "{warnings:?}"
        );
    }

    #[test]
    fn covr_atom_layout_matches_mutagen() {
        for (format, expected_type) in [(CoverFormat::Jpeg, 13u32), (CoverFormat::Png, 14u32)] {
            let cover = CoverArt {
                format,
                data: b"fake-image-bytes".to_vec(),
            };
            let child = covr_atom(&cover).unwrap();
            assert_eq!(&child[4..8], b"covr");
            // data box: type, locale 0, then the original bytes unchanged.
            assert_eq!(&child[12..16], b"data");
            assert_eq!(&child[16..20], &expected_type.to_be_bytes());
            assert_eq!(&child[20..24], &[0, 0, 0, 0]);
            assert_eq!(&child[24..], b"fake-image-bytes");
        }
    }

    #[test]
    fn cover_cap_boundary_is_exact() {
        let mut at_cap = JPEG_MAGIC.to_vec();
        at_cap.resize(COVER_MAX_BYTES, 0);
        let (cover, warnings) = select_cover(&[front(&at_cap)]);
        assert!(cover.is_some(), "{warnings:?}");
        assert!(warnings.is_empty(), "{warnings:?}");
        covr_atom(&cover.unwrap()).unwrap();

        let mut over_cap = JPEG_MAGIC.to_vec();
        over_cap.resize(COVER_MAX_BYTES + 1, 0);
        let (cover, warnings) = select_cover(&[front(&over_cap)]);
        assert_eq!(cover, None);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("cover image too large")
                    && w.contains(&over_cap.len().to_string())),
            "{warnings:?}"
        );

        // Defense in depth: the constructor rejects over-cap input even if
        // selection is bypassed (no huge serialization is attempted).
        let huge = CoverArt {
            format: CoverFormat::Jpeg,
            data: over_cap,
        };
        assert!(covr_atom(&huge).is_err());
    }

    #[test]
    fn cover_front_beats_typed_beats_untyped() {
        let (cover, _) = select_cover(&[back(JPEG_MAGIC), front(PNG_MAGIC), untyped(JPEG_MAGIC)]);
        let cover = cover.unwrap();
        assert_eq!(cover.format, CoverFormat::Png);
        assert_eq!(cover.data, PNG_MAGIC);
    }

    #[test]
    fn cover_other_counts_as_typed_non_front() {
        let (cover, _) = select_cover(&[untyped(JPEG_MAGIC), other(PNG_MAGIC)]);
        let cover = cover.unwrap();
        assert_eq!(cover.format, CoverFormat::Png);
        assert_eq!(cover.data, PNG_MAGIC);
    }

    #[test]
    fn cover_encounter_order_wins_within_a_tier() {
        let (cover, _) = select_cover(&[front(PNG_MAGIC), front(JPEG_MAGIC)]);
        let cover = cover.unwrap();
        assert_eq!(cover.format, CoverFormat::Png);
    }

    #[test]
    fn cover_standard_picture_beats_coverart_equivalent() {
        // A COVERART comment surfaces as an untyped visual; a typed front
        // cover wins even when the untyped image comes first.
        let (cover, _) = select_cover(&[untyped(JPEG_MAGIC), front(PNG_MAGIC)]);
        let cover = cover.unwrap();
        assert_eq!(cover.format, CoverFormat::Png);
    }

    #[test]
    fn cover_untyped_only_falls_back() {
        let (cover, warnings) = select_cover(&[untyped(JPEG_MAGIC)]);
        let cover = cover.unwrap();
        assert_eq!(cover.data, JPEG_MAGIC);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn cover_skips_invalid_candidates_and_continues() {
        // Front GIF (unsupported) must not block the eligible back JPEG.
        let (cover, warnings) = select_cover(&[front(GIF_MAGIC), back(JPEG_MAGIC)]);
        let cover = cover.unwrap();
        assert_eq!(cover.data, JPEG_MAGIC);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("unsupported cover image format")),
            "{warnings:?}"
        );
    }

    #[test]
    fn cover_invalid_standard_falls_back_to_legacy_untyped() {
        // An ineligible FrontCover (unsupported format) must not block
        // fallback to a valid legacy-shaped untyped candidate (tier 2), and
        // the skip warning is retained in selection order.
        let (cover, warnings) = select_cover(&[front(GIF_MAGIC), untyped(JPEG_MAGIC)]);
        let cover = cover.unwrap();
        assert_eq!(cover.format, CoverFormat::Jpeg);
        assert_eq!(cover.data, JPEG_MAGIC);
        assert_eq!(warnings, ["unsupported cover image format: skipped"]);
    }

    #[test]
    fn cover_all_ineligible_warns_per_candidate_without_missing_warning() {
        let (cover, warnings) = select_cover(&[front(GIF_MAGIC), back(&[]), untyped(b"junk")]);
        assert_eq!(cover, None);
        assert_eq!(
            warnings,
            [
                "unsupported cover image format: skipped",
                "empty cover image: skipped",
                "unsupported cover image format: skipped",
            ]
        );
    }

    #[test]
    fn absent_cover_is_silent() {
        let (cover, warnings) = select_cover(&[]);
        assert_eq!(cover, None);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn ilst_children_appends_covr_last_and_mapped_excludes_cover() {
        let mut set = build_tag_set(&pairs(&[("title", "T")]));
        set.cover = Some(CoverArt {
            format: CoverFormat::Png,
            data: PNG_MAGIC.to_vec(),
        });
        assert_eq!(set.mapped_count(), 1);
        let children = set.ilst_children().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(&children[0][4..8], b"\xa9nam");
        assert_eq!(&children[1][4..8], b"covr");
    }
}
