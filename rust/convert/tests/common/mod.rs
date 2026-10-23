#![allow(dead_code)] // Each integration-test binary includes this shared module.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        for _ in 0..64 {
            let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "aac-convert-test-{}-{label}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("could not create test directory {}: {err}", path.display()),
            }
        }
        panic!("could not allocate a unique test directory");
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn temp_output_files(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".aac-convert-"))
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct AdtsFrameInfo {
    pub start: usize,
    pub end: usize,
    pub profile: u8,
    pub sample_rate_index: u8,
    pub channel_configuration: u8,
    pub raw_blocks: u8,
}

pub fn parse_adts(bytes: &[u8]) -> Vec<AdtsFrameInfo> {
    let mut frames = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        assert!(
            bytes.len() - offset >= 7,
            "truncated ADTS header at {offset}"
        );
        let header = &bytes[offset..offset + 7];
        assert_eq!(header[0], 0xFF, "bad ADTS syncword at {offset}");
        assert_eq!(header[1] & 0xF0, 0xF0, "bad ADTS syncword at {offset}");
        assert_eq!(header[1] & 0x08, 0, "ADTS ID is not MPEG-4 at {offset}");
        assert_eq!(header[1] & 0x01, 1, "ADTS CRC is present at {offset}");
        let length = ((usize::from(header[3] & 0x03)) << 11)
            | (usize::from(header[4]) << 3)
            | (usize::from(header[5]) >> 5);
        assert!(length >= 7, "empty ADTS frame at {offset}");
        assert!(
            offset + length <= bytes.len(),
            "ADTS frame overruns file at {offset}"
        );
        frames.push(AdtsFrameInfo {
            start: offset,
            end: offset + length,
            profile: (header[2] >> 6) & 0x03,
            sample_rate_index: (header[2] >> 2) & 0x0F,
            channel_configuration: ((header[2] & 0x01) << 2) | ((header[3] >> 6) & 0x03),
            raw_blocks: header[6] & 0x03,
        });
        offset += length;
    }
    assert_eq!(offset, bytes.len(), "trailing bytes after last ADTS frame");
    frames
}

/// Parsed MP4 box with byte ranges and recursively parsed children.
#[derive(Debug, Clone)]
pub struct Mp4BoxInfo {
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub header_len: usize,
    pub children: Vec<Mp4BoxInfo>,
}

impl Mp4BoxInfo {
    pub fn payload_start(&self) -> usize {
        self.start + self.header_len
    }

    pub fn find(&self, kind: &str) -> Option<&Mp4BoxInfo> {
        if self.kind == kind {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(kind))
    }

    pub fn require(&self, kind: &str) -> &Mp4BoxInfo {
        self.find(kind)
            .unwrap_or_else(|| panic!("missing MP4 box {kind}"))
    }
}

/// Top-level boxes; the returned root has an empty kind and spans the file.
pub fn parse_mp4(bytes: &[u8]) -> Mp4BoxInfo {
    let children = parse_mp4_boxes(bytes, 0, bytes.len());
    let end = children.last().map_or(0, |last| last.end);
    assert_eq!(end, bytes.len(), "MP4 boxes must tile the file exactly");
    Mp4BoxInfo {
        kind: String::new(),
        start: 0,
        end: bytes.len(),
        header_len: 0,
        children,
    }
}

fn parse_mp4_boxes(bytes: &[u8], start: usize, end: usize) -> Vec<Mp4BoxInfo> {
    let mut boxes = Vec::new();
    let mut offset = start;
    while offset < end {
        assert!(offset + 8 <= end, "truncated MP4 box header at {offset}");
        let size32 = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = String::from_utf8_lossy(&bytes[offset + 4..offset + 8]).to_string();
        let (size, header_len) = if size32 == 1 {
            assert!(offset + 16 <= end, "truncated large MP4 box at {offset}");
            let size =
                u64::from_be_bytes(bytes[offset + 8..offset + 16].try_into().unwrap()) as usize;
            (size, 16)
        } else if size32 == 0 {
            (end - offset, 8)
        } else {
            (size32, 8)
        };
        assert!(size >= header_len, "invalid MP4 box size for {kind}");
        assert!(
            offset + size <= end,
            "MP4 box {kind} overruns its parent at {offset}"
        );

        let payload_start = offset + header_len;
        let children = match child_payload_start(&kind, payload_start, offset + size) {
            Some(child_start) => parse_mp4_boxes(bytes, child_start, offset + size),
            None => Vec::new(),
        };
        boxes.push(Mp4BoxInfo {
            kind,
            start: offset,
            end: offset + size,
            header_len,
            children,
        });
        offset += size;
    }
    boxes
}

/// Containers are parsed structurally; `stsd` and `mp4a` have fixed headers
/// before their child boxes, `meta` is a FullBox with a 4-byte version/flags
/// prefix, and `esds` is decoded separately.
fn child_payload_start(kind: &str, payload_start: usize, end: usize) -> Option<usize> {
    let offset = match kind {
        "moov" | "trak" | "mdia" | "minf" | "stbl" | "dinf" | "edts" | "udta" | "ilst" | "----" => {
            payload_start
        }
        "meta" => payload_start + 4,
        "stsd" => payload_start + 8,
        "mp4a" => payload_start + 28,
        _ => return None,
    };
    (offset <= end).then_some(offset)
}

/// Read a `TIMESTAMP`-style 32/64-bit field from `mdhd`.
pub fn mp4_mdhd(bytes: &[u8], mdhd: &Mp4BoxInfo) -> (u32, u64) {
    let payload = &bytes[mdhd.payload_start()..mdhd.end];
    let version = payload[0];
    if version == 1 {
        let timescale = u32::from_be_bytes(payload[20..24].try_into().unwrap());
        let duration = u64::from_be_bytes(payload[24..32].try_into().unwrap());
        (timescale, duration)
    } else {
        let timescale = u32::from_be_bytes(payload[12..16].try_into().unwrap());
        let duration = u32::from_be_bytes(payload[16..20].try_into().unwrap()) as u64;
        (timescale, duration)
    }
}

/// `stts` entries as `(sample_count, sample_delta)`.
pub fn mp4_stts(bytes: &[u8], stts: &Mp4BoxInfo) -> Vec<(u32, u32)> {
    let payload = &bytes[stts.payload_start()..stts.end];
    let count = u32::from_be_bytes(payload[4..8].try_into().unwrap()) as usize;
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let offset = 8 + index * 8;
        entries.push((
            u32::from_be_bytes(payload[offset..offset + 4].try_into().unwrap()),
            u32::from_be_bytes(payload[offset + 4..offset + 8].try_into().unwrap()),
        ));
    }
    entries
}

/// Per-sample sizes from `stsz`.
pub fn mp4_sample_sizes(bytes: &[u8], stsz: &Mp4BoxInfo) -> Vec<u32> {
    let payload = &bytes[stsz.payload_start()..stsz.end];
    let fixed_size = u32::from_be_bytes(payload[4..8].try_into().unwrap());
    let count = u32::from_be_bytes(payload[8..12].try_into().unwrap()) as usize;
    if fixed_size != 0 {
        return vec![fixed_size; count];
    }
    (0..count)
        .map(|index| {
            let offset = 12 + index * 4;
            u32::from_be_bytes(payload[offset..offset + 4].try_into().unwrap())
        })
        .collect()
}

fn read_descriptor_length(bytes: &[u8], position: &mut usize) -> usize {
    let first = bytes[*position];
    *position += 1;
    if first & 0x80 == 0 {
        return first as usize;
    }
    let count = (first & 0x7F) as usize;
    assert!(count > 0 && count <= 4, "unsupported descriptor length");
    let mut length = 0usize;
    for _ in 0..count {
        length = (length << 8) | bytes[*position] as usize;
        *position += 1;
    }
    length
}

/// Extract DecoderSpecificInfo (AudioSpecificConfig) from an `esds` box.
pub fn mp4_audio_specific_config(bytes: &[u8], esds: &Mp4BoxInfo) -> Vec<u8> {
    let payload = &bytes[esds.payload_start()..esds.end];
    let mut offset = 4; // FullBox version/flags.
    assert_eq!(payload[offset], 0x03, "expected ES_Descriptor");
    offset += 1;
    let es_length = read_descriptor_length(payload, &mut offset);
    let es_end = offset + es_length;

    offset += 2; // es_id
    let flags = payload[offset];
    offset += 1;
    if flags & 0x80 != 0 {
        offset += 2;
    }
    if flags & 0x40 != 0 {
        let url_length = payload[offset] as usize;
        offset += 1 + url_length;
    }
    if flags & 0x20 != 0 {
        offset += 2;
    }

    assert_eq!(payload[offset], 0x04, "expected DecoderConfigDescriptor");
    offset += 1;
    let decoder_config_length = read_descriptor_length(payload, &mut offset);
    let decoder_config_end = offset + decoder_config_length;
    assert!(decoder_config_end <= es_end);
    offset += 13; // OTI + stream type + buffer size + max/avg bitrate.

    assert_eq!(payload[offset], 0x05, "expected DecoderSpecificInfo");
    offset += 1;
    let asc_length = read_descriptor_length(payload, &mut offset);
    assert!(offset + asc_length <= decoder_config_end);
    payload[offset..offset + asc_length].to_vec()
}

/// Chunk offsets from `stco` (version 0) or `co64`.
pub fn mp4_chunk_offsets(bytes: &[u8], table: &Mp4BoxInfo) -> Vec<u64> {
    let payload = &bytes[table.payload_start()..table.end];
    let version = payload[0];
    let count = u32::from_be_bytes(payload[4..8].try_into().unwrap()) as usize;
    let mut offsets = Vec::with_capacity(count);
    for index in 0..count {
        if table.kind == "co64" || version == 1 {
            let offset = 8 + index * 8;
            offsets.push(u64::from_be_bytes(
                payload[offset..offset + 8].try_into().unwrap(),
            ));
        } else {
            let offset = 8 + index * 4;
            offsets
                .push(u32::from_be_bytes(payload[offset..offset + 4].try_into().unwrap()) as u64);
        }
    }
    offsets
}

/// Timescale and duration from `mvhd` (same layout as `mdhd`).
pub fn mp4_mvhd(bytes: &[u8], mvhd: &Mp4BoxInfo) -> (u32, u64) {
    mp4_mdhd(bytes, mvhd)
}

/// `elst` entries as `(edit_duration, media_time, media_rate)`, version-aware.
pub fn mp4_elst(bytes: &[u8], elst: &Mp4BoxInfo) -> Vec<(u64, i64, u32)> {
    let payload = &bytes[elst.payload_start()..elst.end];
    let version = payload[0];
    let count = u32::from_be_bytes(payload[4..8].try_into().unwrap()) as usize;
    let mut entries = Vec::with_capacity(count);
    let mut offset = 8;
    for _ in 0..count {
        let (duration, media_time) = if version == 1 {
            let duration = u64::from_be_bytes(payload[offset..offset + 8].try_into().unwrap());
            let media_time =
                i64::from_be_bytes(payload[offset + 8..offset + 16].try_into().unwrap());
            offset += 16;
            (duration, media_time)
        } else {
            let duration =
                u32::from_be_bytes(payload[offset..offset + 4].try_into().unwrap()) as u64;
            let media_time =
                i32::from_be_bytes(payload[offset + 4..offset + 8].try_into().unwrap()) as i64;
            offset += 8;
            (duration, media_time)
        };
        let rate = u32::from_be_bytes(payload[offset..offset + 4].try_into().unwrap());
        offset += 4;
        entries.push((duration, media_time, rate));
    }
    entries
}

/// The `iTunSMPB` value string from `moov/udta/meta/ilst/----/data`.
///
/// Verifies the freeform item structure (`mean = com.apple.iTunes`,
/// `name = iTunSMPB`, `data` type UTF-8) and returns the raw value payload.
pub fn mp4_itunsmpb(bytes: &[u8], root: &Mp4BoxInfo) -> String {
    let udta = root.require("udta");
    let meta = udta.require("meta");
    let ilst = meta.require("ilst");
    let freeform = ilst.require("----");
    let mut mean = None;
    let mut name = None;
    let mut data = None;
    for child in &freeform.children {
        let payload = &bytes[child.payload_start()..child.end];
        match child.kind.as_str() {
            "mean" => mean = Some(&payload[4..]),
            "name" => name = Some(&payload[4..]),
            "data" => data = Some(payload),
            kind => panic!("unexpected ---- child {kind}"),
        }
    }
    assert_eq!(mean, Some(b"com.apple.iTunes".as_slice()));
    assert_eq!(name, Some(b"iTunSMPB".as_slice()));
    let data = data.expect("---- item has no data box");
    assert_eq!(&data[0..4], &[0x00, 0x00, 0x00, 0x01], "data must be UTF-8");
    String::from_utf8(data[8..].to_vec()).expect("iTunSMPB value must be UTF-8")
}
