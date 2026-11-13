//! Existing album artwork discovery and reuse.
//!
//! Before falling back to artwork embedded in FLAC picture metadata, the batch
//! pipeline looks for a conventional image file in the source album directory.
//! The search is deterministic:
//!
//! 1. `cover.jpg`, `folder.jpg`, `front.jpg`, `album.jpg`
//! 2. the same four stems with `.jpeg`
//! 3. the same four stems with `.png`
//! 4. the same four stems with `.webp`
//!
//! Matching is ASCII case-insensitive. Candidate files are inspected in that
//! order and the first readable, supported, in-limit file wins. Missing,
//! unreadable, empty, misnamed, or oversized candidates are skipped without
//! failing the album; the search simply continues with the next candidate.
//!
//! Formats are detected by magic bytes only (never by extension or declared
//! MIME type): JPEG, PNG, and WebP. JPEG and PNG can be embedded in M4A;
//! WebP is external-publication only because the `covr` atom supports JPEG and
//! PNG only. Images are never decoded or re-encoded.
//!
//! Validation is intentionally shallow (prefix/signature match only, no image
//! decode and no added image dependency). A file with a valid prefix but
//! corrupt payload is treated as its sniffed format; this matches the
//! pre-existing embedded-artwork policy and keeps discovery dependency-free.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use crate::meta::COVER_MAX_BYTES;

/// Conventional artwork stems in priority order.
pub(crate) const COVER_STEMS: [&str; 4] = ["cover", "folder", "front", "album"];

/// Supported artwork extensions in priority order.
pub(crate) const COVER_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

/// Image format detected by magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArtworkFormat {
    /// JPEG (embeddable and publishable).
    Jpeg,
    /// PNG (embeddable and publishable).
    Png,
    /// WebP (publishable externally only).
    Webp,
}

impl ArtworkFormat {
    /// Conventional external output name for this format.
    pub(crate) fn external_file_name(self) -> &'static str {
        match self {
            Self::Jpeg => "cover.jpg",
            Self::Png => "cover.png",
            Self::Webp => "cover.webp",
        }
    }
}

/// One existing artwork file selected from an album directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExistingArtwork {
    /// Path of the selected file (joined to the searched directory).
    pub(crate) path: PathBuf,
    /// Format detected from the file's magic bytes.
    pub(crate) format: ArtworkFormat,
    /// Original image bytes, unchanged.
    pub(crate) data: Vec<u8>,
}

/// Every supported file name in deterministic priority order.
///
/// Kept public inside the crate so tests can assert the complete matrix
/// without duplicating the ordering rules.
pub(crate) fn candidate_file_names() -> Vec<String> {
    let mut names = Vec::with_capacity(COVER_STEMS.len() * COVER_EXTENSIONS.len());
    for extension in COVER_EXTENSIONS {
        for stem in COVER_STEMS {
            names.push(format!("{stem}.{extension}"));
        }
    }
    names
}

/// Find existing artwork in `directory`.
///
/// Reads the directory once, sorts entries by raw name bytes for deterministic
/// tie-breaking (for example `cover.jpg` and `COVER.JPG` on a case-sensitive
/// filesystem), then walks [`candidate_file_names`]. Returns `None` when no
/// candidate is readable, supported, and within the cover-size limit.
///
/// Files are opened and then re-validated as regular files with a bounded
/// read capped just above [`COVER_MAX_BYTES`], so a file that grows between
/// the size pre-check and the read cannot cause an unbounded allocation. A
/// candidate that is unreadable, not a regular file, unsupported, empty, or
/// too large is skipped; a lower-priority candidate may still be usable.
pub(crate) fn discover(directory: &Path) -> Option<ExistingArtwork> {
    let listing = fs::read_dir(directory).ok()?;
    let mut names: Vec<OsString> = listing
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .collect();
    names.sort_by(|left, right| left.as_encoded_bytes().cmp(right.as_encoded_bytes()));

    for candidate in candidate_file_names() {
        let candidate = OsStr::new(&candidate);
        for name in &names {
            if !name
                .as_encoded_bytes()
                .eq_ignore_ascii_case(candidate.as_encoded_bytes())
            {
                continue;
            }
            let path = directory.join(name);
            // A candidate that is unreadable, not a regular file, unsupported,
            // or too large is skipped; a lower-priority candidate may still be
            // usable, and otherwise the caller falls back to embedded artwork.
            // The size pre-check avoids opening obviously huge files; the
            // bounded read below closes the TOCTOU window where a file grows
            // between the check and the read.
            if fs::metadata(&path)
                .map(|metadata| !metadata.is_file() || metadata.len() > COVER_MAX_BYTES as u64)
                .unwrap_or(true)
            {
                continue;
            }
            let Ok(data) = read_bounded(&path) else {
                continue;
            };
            if data.is_empty() || data.len() > COVER_MAX_BYTES {
                continue;
            }
            if let Some(format) = sniff_format(&data) {
                return Some(ExistingArtwork { path, format, data });
            }
        }
    }
    None
}

/// Detect JPEG, PNG, or WebP by magic bytes.
fn sniff_format(data: &[u8]) -> Option<ArtworkFormat> {
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        Some(ArtworkFormat::Jpeg)
    } else if data.len() >= 8 && data[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        Some(ArtworkFormat::Png)
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some(ArtworkFormat::Webp)
    } else {
        None
    }
}

/// Read at most [`COVER_MAX_BYTES`] + 1 bytes.
///
/// The extra byte distinguishes "exactly at the limit" from "over the
/// limit" without allocating the whole file when it grows concurrently.
/// Callers treat over-limit reads as skipped candidates. The post-open
/// `is_file` check closes part of the TOCTOU window where a path is
/// replaced between the pre-check and the open; a path replaced by a FIFO
/// or socket after the pre-check can still block on open, which is an
/// accepted residual risk for media directories (documented in the batch
/// module).
fn read_bounded(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;

    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let mut bounded = file.take(COVER_MAX_BYTES as u64 + 1);
    let mut data = Vec::new();
    bounded.read_to_end(&mut data)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "aac-convert-artwork-unit-{}-{label}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn bytes(format: ArtworkFormat) -> Vec<u8> {
        match format {
            ArtworkFormat::Jpeg => vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10],
            ArtworkFormat::Png => {
                let mut data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
                data.extend_from_slice(b"payload");
                data
            }
            ArtworkFormat::Webp => {
                let mut data = b"RIFF".to_vec();
                data.extend_from_slice(&16u32.to_le_bytes());
                data.extend_from_slice(b"WEBPVP8 payload");
                data
            }
        }
    }

    fn write(directory: &Path, name: &str, format: ArtworkFormat) -> PathBuf {
        let path = directory.join(name);
        fs::write(&path, bytes(format)).unwrap();
        path
    }

    #[test]
    fn candidate_order_matches_the_specification() {
        assert_eq!(
            candidate_file_names(),
            vec![
                "cover.jpg",
                "folder.jpg",
                "front.jpg",
                "album.jpg",
                "cover.jpeg",
                "folder.jpeg",
                "front.jpeg",
                "album.jpeg",
                "cover.png",
                "folder.png",
                "front.png",
                "album.png",
                "cover.webp",
                "folder.webp",
                "front.webp",
                "album.webp",
            ]
        );
    }

    #[test]
    fn every_supported_name_is_discovered() {
        for candidate in candidate_file_names() {
            let directory = TempDir::new("name-matrix");
            let format = if candidate.ends_with(".png") {
                ArtworkFormat::Png
            } else if candidate.ends_with(".webp") {
                ArtworkFormat::Webp
            } else {
                ArtworkFormat::Jpeg
            };
            let path = write(&directory.path, &candidate, format);
            let found = discover(&directory.path);
            assert_eq!(
                found.as_ref().map(|found| found.path.as_path()),
                Some(path.as_path()),
                "{candidate}"
            );
            assert_eq!(found.unwrap().format, format, "{candidate}");
        }
    }

    #[test]
    fn matching_is_ascii_case_insensitive() {
        for name in [
            "COVER.JPG",
            "Folder.JpEg",
            "FRONT.PNG",
            "aLbUm.WeBp",
            "folder.jpeg",
        ] {
            let directory = TempDir::new("case");
            let format = if name.to_ascii_lowercase().ends_with(".png") {
                ArtworkFormat::Png
            } else if name.to_ascii_lowercase().ends_with(".webp") {
                ArtworkFormat::Webp
            } else {
                ArtworkFormat::Jpeg
            };
            let path = write(&directory.path, name, format);
            assert_eq!(
                discover(&directory.path).map(|found| found.path),
                Some(path),
                "{name}"
            );
        }
    }

    #[test]
    fn priority_is_deterministic_when_every_candidate_exists() {
        let directory = TempDir::new("priority");
        let mut written = Vec::new();
        for candidate in candidate_file_names() {
            let format = if candidate.ends_with(".png") {
                ArtworkFormat::Png
            } else if candidate.ends_with(".webp") {
                ArtworkFormat::Webp
            } else {
                ArtworkFormat::Jpeg
            };
            written.push((
                candidate.clone(),
                write(&directory.path, &candidate, format),
            ));
        }
        for (candidate, path) in written {
            assert_eq!(
                discover(&directory.path).map(|found| found.path),
                Some(path),
                "expected {candidate} to win the remaining priority"
            );
            fs::remove_file(directory.path.join(&candidate)).unwrap();
        }
        assert!(discover(&directory.path).is_none());
    }

    #[test]
    fn case_variants_resolve_to_one_deterministic_name() {
        let directory = TempDir::new("case-tie");
        let upper = write(&directory.path, "COVER.JPG", ArtworkFormat::Jpeg);
        let lower = write(&directory.path, "cover.jpg", ArtworkFormat::Jpeg);
        let found = discover(&directory.path).expect("one case variant must be found");
        assert!(
            found.path == upper || found.path == lower,
            "unexpected path {}",
            found.path.display()
        );
    }

    #[test]
    fn invalid_and_oversized_candidates_fall_through() {
        let directory = TempDir::new("fallthrough");
        fs::write(directory.path.join("cover.jpg"), b"not an image").unwrap();
        let path = write(&directory.path, "folder.png", ArtworkFormat::Png);
        assert_eq!(
            discover(&directory.path).map(|found| found.path),
            Some(path)
        );

        let directory = TempDir::new("oversized");
        fs::write(directory.path.join("cover.jpg"), b"not an image").unwrap();
        // A directory named like a candidate is not a regular file.
        fs::create_dir(directory.path.join("folder.jpg")).unwrap();
        let path = write(&directory.path, "front.jpeg", ArtworkFormat::Jpeg);
        assert_eq!(
            discover(&directory.path).map(|found| found.path),
            Some(path)
        );
    }

    #[test]
    fn oversized_candidate_is_skipped_before_reading() {
        let directory = TempDir::new("oversized-cover");
        let huge = fs::File::create(directory.path.join("cover.jpg")).unwrap();
        huge.set_len((COVER_MAX_BYTES + 1) as u64).unwrap();
        let fallback = write(&directory.path, "folder.jpg", ArtworkFormat::Jpeg);
        assert_eq!(
            discover(&directory.path).map(|found| found.path),
            Some(fallback)
        );
    }

    #[test]
    fn missing_artwork_returns_none() {
        let directory = TempDir::new("missing");
        assert!(discover(&directory.path).is_none());
        assert!(discover(Path::new("/definitely/not/a/real/album")).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_candidate_falls_through() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TempDir::new("unreadable");
        let locked = write(&directory.path, "cover.jpg", ArtworkFormat::Jpeg);
        let fallback = write(&directory.path, "folder.jpg", ArtworkFormat::Jpeg);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&locked).is_ok() {
            // Running as root or on a filesystem that ignores permissions.
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
            return;
        }
        let found = discover(&directory.path).map(|found| found.path);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(found, Some(fallback));
    }

    #[test]
    fn format_sniffing_never_trusts_extensions() {
        let directory = TempDir::new("sniff");
        // A PNG payload under a `.jpg` name is classified by its bytes.
        fs::write(directory.path.join("cover.jpg"), bytes(ArtworkFormat::Png)).unwrap();
        assert_eq!(
            discover(&directory.path).map(|found| found.format),
            Some(ArtworkFormat::Png)
        );
        assert_eq!(ArtworkFormat::Jpeg.external_file_name(), "cover.jpg");
        assert_eq!(ArtworkFormat::Png.external_file_name(), "cover.png");
        assert_eq!(ArtworkFormat::Webp.external_file_name(), "cover.webp");
    }

    #[test]
    fn empty_candidate_is_skipped() {
        let directory = TempDir::new("empty");
        fs::write(directory.path.join("cover.jpg"), b"").unwrap();
        let path = write(&directory.path, "front.jpg", ArtworkFormat::Jpeg);
        assert_eq!(
            discover(&directory.path).map(|found| found.path),
            Some(path)
        );
    }
}
