//! Native loudness tagging (L3) end-to-end behavior.
//!
//! Covers: exact ReplayGain/iTunNORM atoms and values, ilst ordering with
//! text/cover/`iTunSMPB`, clipping protection, silence handling, album
//! aggregation and incompleteness, retained outputs, flag semantics, and
//! failure compatibility. Fixtures are synthetic repo-owned tones (see
//! `tests/fixtures/README.md`); no EBU archive material is used.

use std::fs;
use std::path::Path;

use aac_convert::batch::{
    execute_plan_with_options, execute_plan_with_tags, plan_batch, BatchOptions,
};
use aac_convert::{convert_file_with_format_and_tags, OutputFormat};

mod common;
use common::{fixture, mp4_ilst_children, parse_mp4, TempDir};

/// One parsed `----` freeform atom.
struct Freeform {
    name: String,
    data_type: u32,
    locale: u32,
    value: String,
}

/// All `----` children of `ilst` in file order, with structure verified.
fn ilst_freeforms(bytes: &[u8]) -> Vec<Freeform> {
    let root = parse_mp4(bytes);
    let ilst = root.require("udta").require("meta").require("ilst");
    let mut out = Vec::new();
    for (kind, child) in mp4_ilst_children(bytes, ilst) {
        if &kind != b"----" {
            continue;
        }
        let mut mean = None;
        let mut name = None;
        let mut data = None;
        let mut offset = 8;
        while offset + 8 <= child.len() {
            let size = u32::from_be_bytes(child[offset..offset + 4].try_into().unwrap()) as usize;
            assert!(size >= 8 && offset + size <= child.len());
            let payload = &child[offset + 8..offset + size];
            match &child[offset + 4..offset + 8] {
                b"mean" => mean = Some(&payload[4..]),
                b"name" => name = Some(&payload[4..]),
                b"data" => data = Some(payload),
                other => panic!("unexpected ---- child {other:?}"),
            }
            offset += size;
        }
        assert_eq!(offset, child.len(), "---- children must tile exactly");
        assert_eq!(mean, Some(b"com.apple.iTunes".as_slice()));
        let data = data.expect("---- item has no data box");
        let data_type = u32::from_be_bytes(data[0..4].try_into().unwrap());
        let locale = u32::from_be_bytes(data[4..8].try_into().unwrap());
        out.push(Freeform {
            name: String::from_utf8(name.expect("---- item has no name")[..].to_vec()).unwrap(),
            data_type,
            locale,
            value: String::from_utf8(data[8..].to_vec()).unwrap(),
        });
    }
    out
}

/// `(4-byte kind, freeform name or "")` for every `ilst` child in file order.
fn ilst_layout(bytes: &[u8]) -> Vec<(String, String)> {
    let root = parse_mp4(bytes);
    let ilst = root.require("udta").require("meta").require("ilst");
    let mut layout = Vec::new();
    for (kind, child) in mp4_ilst_children(bytes, ilst) {
        let kind_name = String::from_utf8_lossy(&kind).to_string();
        if &kind == b"----" {
            let mut name = String::new();
            let mut offset = 8;
            while offset + 8 <= child.len() {
                let size =
                    u32::from_be_bytes(child[offset..offset + 4].try_into().unwrap()) as usize;
                if &child[offset + 4..offset + 8] == b"name" {
                    name = String::from_utf8(child[offset + 12..offset + size].to_vec()).unwrap();
                    break;
                }
                offset += size;
            }
            layout.push((kind_name, name));
        } else {
            layout.push((kind_name, String::new()));
        }
    }
    layout
}

fn freeform_values(bytes: &[u8]) -> Vec<(String, String)> {
    ilst_freeforms(bytes)
        .into_iter()
        .map(|item| (item.name, item.value))
        .collect()
}

fn convert_single(name: &str, write_tags: bool) -> (Vec<u8>, aac_convert::TagDiagnostics) {
    let directory = TempDir::new("loud-single");
    let output = directory.join("out.m4a");
    let (_, diagnostics) =
        convert_file_with_format_and_tags(&fixture(name), &output, OutputFormat::M4a, write_tags)
            .unwrap();
    (fs::read(&output).unwrap(), diagnostics)
}

const ITUNNORM_TAIL: &str = "00024CA8 00024CA8 00007FFF 00007FFF 00024CA8 00024CA8";

#[test]
fn tone_single_file_writes_complete_loudness_set() {
    let (bytes, diagnostics) = convert_single("loud-tone-48k-16.flac", true);
    // No text tags in the fixture: `mapped` stays 0 and nothing warns (a
    // complete album with no clipping adjustment is silent).
    assert_eq!(diagnostics.mapped, 0);
    assert!(
        !diagnostics
            .warnings
            .iter()
            .any(|w| w.starts_with("loudness:")),
        "{:?}",
        diagnostics.warnings
    );
    // ilst order: iTunSMPB item, the encoder identification, then the five
    // loudness atoms (no text/covr).
    assert_eq!(
        ilst_layout(&bytes),
        [
            ("----".to_string(), "iTunSMPB".to_string()),
            ("\u{fffd}too".to_string(), String::new()),
            ("----".to_string(), "REPLAYGAIN_TRACK_GAIN".to_string()),
            ("----".to_string(), "REPLAYGAIN_TRACK_PEAK".to_string()),
            ("----".to_string(), "REPLAYGAIN_ALBUM_GAIN".to_string()),
            ("----".to_string(), "REPLAYGAIN_ALBUM_PEAK".to_string()),
            ("----".to_string(), "iTunNORM".to_string()),
        ]
    );
    let values = freeform_values(&bytes);
    assert_eq!(
        values,
        [
            (
                "iTunSMPB".to_string(),
                " 00000000 00000640 00000040 000000000003A980 00000000 00000000 00000000 00000000 \
                 00000000 00000000 00000000 00000000"
                    .to_string()
            ),
            ("REPLAYGAIN_TRACK_GAIN".to_string(), "0.75 dB".to_string()),
            ("REPLAYGAIN_TRACK_PEAK".to_string(), "0.125000".to_string()),
            ("REPLAYGAIN_ALBUM_GAIN".to_string(), "0.75 dB".to_string()),
            ("REPLAYGAIN_ALBUM_PEAK".to_string(), "0.125000".to_string()),
            (
                "iTunNORM".to_string(),
                format!(" 00000349 00000349 00000837 00000837 {ITUNNORM_TAIL}")
            ),
        ]
    );
    // Validated MP4 representation on every loudness atom.
    for item in ilst_freeforms(&bytes).iter().skip(1) {
        assert_eq!(item.data_type, 1, "{}", item.name);
        assert_eq!(item.locale, 0, "{}", item.name);
    }
}

#[test]
fn quiet_positive_gain_passes_through_without_adjustment() {
    let (bytes, diagnostics) = convert_single("loud-quiet-48k-16.flac", true);
    let values = freeform_values(&bytes);
    assert!(values.contains(&("REPLAYGAIN_TRACK_GAIN".to_string(), "12.35 dB".to_string())));
    assert!(values.contains(&("REPLAYGAIN_TRACK_PEAK".to_string(), "0.031250".to_string())));
    assert!(
        !diagnostics.warnings.iter().any(|w| w.contains("adjusted")),
        "{:?}",
        diagnostics.warnings
    );
}

#[test]
fn clip_spike_adjusts_gain_and_preserves_peak() {
    let (bytes, diagnostics) = convert_single("loud-clip-48k-16.flac", true);
    let values = freeform_values(&bytes);
    // Measured gain (+8.71 dB) would clip the 0.899994 peak; the written gain
    // is exactly −20·log10(peak). The peak itself is never rewritten.
    assert!(values.contains(&("REPLAYGAIN_TRACK_GAIN".to_string(), "0.92 dB".to_string())));
    assert!(values.contains(&("REPLAYGAIN_TRACK_PEAK".to_string(), "0.899994".to_string())));
    // iTunNORM derives from the adjusted track gain, not the measured one.
    assert!(values.contains(&(
        "iTunNORM".to_string(),
        format!(" 00000329 00000329 000007E7 000007E7 {ITUNNORM_TAIL}")
    )));
    assert!(
        diagnostics
            .warnings
            .iter()
            .any(|w| w
                == "loudness: clipping protection adjusted track gain from 8.71 dB to 0.92 dB"),
        "{:?}",
        diagnostics.warnings
    );
    assert!(
        diagnostics
            .warnings
            .iter()
            .any(|w| w
                == "loudness: clipping protection adjusted album gain from 8.71 dB to 0.92 dB"),
        "{:?}",
        diagnostics.warnings
    );
}

#[test]
fn silence_omits_atoms_and_warns_but_converts() {
    let (bytes, diagnostics) = convert_single("loud-silence-48k-16.flac", true);
    // Only the gapless item and encoder identification remain; audio still
    // converted.
    assert_eq!(
        ilst_layout(&bytes),
        [
            ("----".to_string(), "iTunSMPB".to_string()),
            ("\u{fffd}too".to_string(), String::new()),
        ]
    );
    assert!(
        diagnostics
            .warnings
            .iter()
            .any(|w| w == "loudness: no measurable signal: omitting loudness tags for this track"),
        "{:?}",
        diagnostics.warnings
    );
    assert!(
        diagnostics.warnings.iter().any(|w| w
            .contains("loudness: album is incomplete (unanalyzable: loud-silence-48k-16.flac)")),
        "{:?}",
        diagnostics.warnings
    );
}

#[test]
fn short_committed_fixtures_omit_loudness_by_construction() {
    // The 0.25 s M1/S2 fixtures never clear an R128 gating block, so they
    // carry no loudness atoms. This pins the deliberate omission (with
    // warnings) rather than weakening to it silently.
    let (bytes, diagnostics) = convert_single("stereo-48k-16.flac", true);
    assert_eq!(
        ilst_layout(&bytes),
        [
            ("----".to_string(), "iTunSMPB".to_string()),
            ("\u{fffd}too".to_string(), String::new()),
        ]
    );
    assert!(
        diagnostics
            .warnings
            .iter()
            .any(|w| w.starts_with("loudness:")),
        "{:?}",
        diagnostics.warnings
    );
}

#[test]
fn mono_lattice_reports_expected_values() {
    let (bytes, _) = convert_single("loud-mono-441k-16.flac", true);
    let values = freeform_values(&bytes);
    assert!(values.contains(&("REPLAYGAIN_TRACK_GAIN".to_string(), "3.75 dB".to_string())));
    assert!(values.contains(&("REPLAYGAIN_TRACK_PEAK".to_string(), "0.124969".to_string())));
    assert!(values.contains(&(
        "iTunNORM".to_string(),
        format!(" 000001A6 000001A6 0000041E 0000041E {ITUNNORM_TAIL}")
    )));
}

#[test]
fn tagged_cover_coexists_with_loudness_in_order() {
    let (bytes, diagnostics) = convert_single("loud-tagged-48k-16.flac", true);
    assert_eq!(diagnostics.mapped, 2);
    assert_eq!(
        ilst_layout(&bytes),
        [
            ("----".to_string(), "iTunSMPB".to_string()),
            ("\u{fffd}too".to_string(), String::new()),
            // Text atoms use Latin-1 `©` (0xA9), which lossy-converts here.
            ("\u{fffd}nam".to_string(), String::new()),
            ("\u{fffd}ART".to_string(), String::new()),
            ("covr".to_string(), String::new()),
            ("----".to_string(), "REPLAYGAIN_TRACK_GAIN".to_string()),
            ("----".to_string(), "REPLAYGAIN_TRACK_PEAK".to_string()),
            ("----".to_string(), "REPLAYGAIN_ALBUM_GAIN".to_string()),
            ("----".to_string(), "REPLAYGAIN_ALBUM_PEAK".to_string()),
            ("----".to_string(), "iTunNORM".to_string()),
        ]
    );
    let root = parse_mp4(&bytes);
    let ilst = root.require("udta").require("meta").require("ilst");
    assert_eq!(
        common::mp4_ilst_text(&bytes, ilst, b"\xa9nam"),
        vec!["Loud Title".to_string()]
    );
    // Cover bytes survive unchanged beside the new atoms.
    let (data_type, payload) = common::mp4_ilst_cover(&bytes, ilst).expect("covr present");
    assert_eq!(data_type, 13);
    assert_eq!(payload.len(), 222);
    assert_eq!(&payload[0..2], &[0xFF, 0xD8]);
}

fn batch_convert(dir: &Path, skip_existing: bool) -> aac_convert::batch::BatchOutcome {
    let out = dir.join("out");
    let plan = plan_batch(&dir.join("in"), &out, OutputFormat::M4a, true).unwrap();
    execute_plan_with_tags(&plan, OutputFormat::M4a, false, skip_existing, true).unwrap()
}

fn stage_album(files: &[&str]) -> TempDir {
    let directory = TempDir::new("loud-album");
    let input = directory.join("in");
    fs::create_dir(&input).unwrap();
    for file in files {
        fs::copy(fixture(file), input.join(file)).unwrap();
    }
    directory
}

#[test]
fn album_aggregation_shares_one_album_gain() {
    let directory = stage_album(&["loud-tone-48k-16.flac", "loud-quiet-48k-16.flac"]);
    let outcome = batch_convert(directory.path(), false);
    assert_eq!(outcome.summary.converted, 2);
    assert_eq!(outcome.summary.failed, 0);

    let a = fs::read(directory.join("out/loud-tone-48k-16.m4a")).unwrap();
    let b = fs::read(directory.join("out/loud-quiet-48k-16.m4a")).unwrap();
    let values_a = freeform_values(&a);
    let values_b = freeform_values(&b);
    // Track values differ; album values are identical across members.
    assert!(values_a.contains(&("REPLAYGAIN_TRACK_GAIN".to_string(), "0.75 dB".to_string())));
    assert!(values_b.contains(&("REPLAYGAIN_TRACK_GAIN".to_string(), "12.35 dB".to_string())));
    for values in [&values_a, &values_b] {
        assert!(values.contains(&("REPLAYGAIN_ALBUM_GAIN".to_string(), "3.47 dB".to_string())));
        assert!(values.contains(&("REPLAYGAIN_ALBUM_PEAK".to_string(), "0.125000".to_string())));
    }
    // Complete album, no adjustment: no loudness warnings on either file.
    for result in &outcome.results {
        if let aac_convert::batch::BatchFileOutcome::Converted { tags, .. } = &result.outcome {
            assert!(
                !tags.warnings.iter().any(|w| w.starts_with("loudness:")),
                "{:?}",
                tags.warnings
            );
        } else {
            panic!("expected conversion, got {:?}", result.outcome);
        }
    }
}

#[test]
fn incomplete_album_keeps_track_atoms_and_omits_album_atoms() {
    let directory = stage_album(&["loud-tone-48k-16.flac", "loud-silence-48k-16.flac"]);
    let outcome = batch_convert(directory.path(), false);
    assert_eq!(outcome.summary.converted, 2);

    let tone = fs::read(directory.join("out/loud-tone-48k-16.m4a")).unwrap();
    // Track-level atoms present; album-level atoms omitted (never a subset
    // result presented as complete).
    assert_eq!(
        ilst_layout(&tone),
        [
            ("----".to_string(), "iTunSMPB".to_string()),
            ("\u{fffd}too".to_string(), String::new()),
            ("----".to_string(), "REPLAYGAIN_TRACK_GAIN".to_string()),
            ("----".to_string(), "REPLAYGAIN_TRACK_PEAK".to_string()),
            ("----".to_string(), "iTunNORM".to_string()),
        ]
    );
    let silence = fs::read(directory.join("out/loud-silence-48k-16.m4a")).unwrap();
    assert_eq!(
        ilst_layout(&silence),
        [
            ("----".to_string(), "iTunSMPB".to_string()),
            ("\u{fffd}too".to_string(), String::new()),
        ]
    );
    // The album-level omission names the silent member on each file concerned.
    for result in &outcome.results {
        if let aac_convert::batch::BatchFileOutcome::Converted { tags, .. } = &result.outcome {
            assert!(
                tags.warnings.iter().any(|w| w.contains(
                    "loudness: album is incomplete (unanalyzable: loud-silence-48k-16.flac)"
                )),
                "{:?}",
                tags.warnings
            );
        }
    }
}

#[test]
fn batch_groups_albums_by_parent_directory() {
    // Root-level and subdirectory members form separate one-member albums:
    // each album gain equals its own track gain (no cross-directory merging).
    let directory = stage_album(&["loud-tone-48k-16.flac"]);
    let sub = directory.join("in/sub");
    fs::create_dir(&sub).unwrap();
    fs::copy(
        fixture("loud-quiet-48k-16.flac"),
        sub.join("loud-quiet-48k-16.flac"),
    )
    .unwrap();
    let outcome = batch_convert(directory.path(), false);
    assert_eq!(outcome.summary.converted, 2);

    for (file, gain) in [
        ("out/loud-tone-48k-16.m4a", "0.75 dB"),
        ("out/sub/loud-quiet-48k-16.m4a", "12.35 dB"),
    ] {
        let bytes = fs::read(directory.join(file)).unwrap();
        let values = freeform_values(&bytes);
        let track = values
            .iter()
            .find(|(name, _)| name == "REPLAYGAIN_TRACK_GAIN")
            .unwrap()
            .1
            .clone();
        let album = values
            .iter()
            .find(|(name, _)| name == "REPLAYGAIN_ALBUM_GAIN")
            .unwrap()
            .1
            .clone();
        assert_eq!(track, gain, "{file}");
        assert_eq!(album, gain, "{file}");
    }
}

#[test]
fn parallel_album_analysis_matches_sequential_output() {
    // Two one-member albums with measurable loudness run through the
    // concurrent preparation stage (jobs = 4) and must match sequential
    // analysis byte for byte.
    let directory = stage_album(&["loud-tone-48k-16.flac"]);
    let sub = directory.join("in/sub");
    fs::create_dir(&sub).unwrap();
    fs::copy(
        fixture("loud-quiet-48k-16.flac"),
        sub.join("loud-quiet-48k-16.flac"),
    )
    .unwrap();

    let input = directory.join("in");
    let sequential_out = directory.join("sequential");
    let parallel_out = directory.join("parallel");
    for (output, jobs) in [(&sequential_out, 1), (&parallel_out, 4)] {
        let plan = plan_batch(&input, output, OutputFormat::M4a, true).unwrap();
        let execution = execute_plan_with_options(
            &plan,
            OutputFormat::M4a,
            &BatchOptions {
                jobs,
                ..BatchOptions::default()
            },
        )
        .unwrap();
        assert_eq!(execution.outcome.summary.converted, 2, "jobs={jobs}");
        assert_eq!(execution.outcome.summary.failed, 0, "jobs={jobs}");
    }

    for (relative, expected_gain) in [
        ("loud-tone-48k-16.m4a", "0.75 dB"),
        ("sub/loud-quiet-48k-16.m4a", "12.35 dB"),
    ] {
        let sequential = fs::read(sequential_out.join(relative)).unwrap();
        let parallel = fs::read(parallel_out.join(relative)).unwrap();
        assert_eq!(sequential, parallel, "{relative}");

        let values = freeform_values(&parallel);
        let track = values
            .iter()
            .find(|(name, _)| name == "REPLAYGAIN_TRACK_GAIN")
            .unwrap();
        let album = values
            .iter()
            .find(|(name, _)| name == "REPLAYGAIN_ALBUM_GAIN")
            .unwrap();
        assert_eq!(track.1, expected_gain, "{relative}");
        assert_eq!(album.1, expected_gain, "{relative}");
    }
}

#[test]
fn retained_outputs_stay_byte_identical_and_complete_the_album() {
    let directory = stage_album(&["loud-tone-48k-16.flac", "loud-quiet-48k-16.flac"]);
    let first = batch_convert(directory.path(), false);
    assert_eq!(first.summary.converted, 2);
    let before_a = fs::read(directory.join("out/loud-tone-48k-16.m4a")).unwrap();
    let before_b = fs::read(directory.join("out/loud-quiet-48k-16.m4a")).unwrap();

    // Second run retains both outputs; analysis reads the retained sources
    // without modifying the retained outputs.
    let second = batch_convert(directory.path(), true);
    assert_eq!(second.summary.skipped, 2);
    assert_eq!(second.summary.converted, 0);
    assert_eq!(second.summary.failed, 0);
    assert_eq!(
        fs::read(directory.join("out/loud-tone-48k-16.m4a")).unwrap(),
        before_a
    );
    assert_eq!(
        fs::read(directory.join("out/loud-quiet-48k-16.m4a")).unwrap(),
        before_b
    );
}

#[test]
fn missing_retained_source_marks_the_album_incomplete() {
    let directory = stage_album(&["loud-tone-48k-16.flac", "loud-quiet-48k-16.flac"]);
    // Simulate a retained member whose source vanished after planning: the
    // destination exists (so it is skipped, never decoded), but its source
    // cannot be analyzed.
    let out = directory.join("out");
    fs::create_dir(&out).unwrap();
    fs::copy(
        fixture("stereo-48k-16.flac"),
        directory.join("in/loud-quiet-48k-16.flac"),
    )
    .unwrap();
    let plan = plan_batch(&directory.join("in"), &out, OutputFormat::M4a, true).unwrap();
    fs::copy(
        fixture("stereo-48k-16.flac"),
        out.join("loud-quiet-48k-16.m4a"),
    )
    .unwrap();
    fs::remove_file(directory.join("in/loud-quiet-48k-16.flac")).unwrap();

    let outcome = execute_plan_with_tags(&plan, OutputFormat::M4a, false, true, true).unwrap();
    assert_eq!(outcome.summary.converted, 1);
    assert_eq!(outcome.summary.skipped, 1);

    let tone = fs::read(directory.join("out/loud-tone-48k-16.m4a")).unwrap();
    let names: Vec<String> = ilst_freeforms(&tone)
        .into_iter()
        .map(|item| item.name)
        .collect();
    assert!(
        !names.contains(&"REPLAYGAIN_ALBUM_GAIN".to_string()),
        "{names:?}"
    );
    assert!(
        !names.contains(&"REPLAYGAIN_ALBUM_PEAK".to_string()),
        "{names:?}"
    );
    assert!(names.contains(&"REPLAYGAIN_TRACK_GAIN".to_string()));
}

#[test]
fn mixed_layout_album_keeps_track_tags_and_omits_album_tags() {
    // A 48 kHz stereo member cannot join the aggregation seeded by the
    // plan-first 44.1 kHz mono member (byte order decides the seed): the
    // album is incomplete (never a subset result), but both track-level sets
    // are still written and the warning names the mismatching member.
    let directory = stage_album(&["loud-tone-48k-16.flac", "loud-mono-441k-16.flac"]);
    let outcome = batch_convert(directory.path(), false);
    assert_eq!(outcome.summary.converted, 2);

    for (file, gain) in [
        ("out/loud-tone-48k-16.m4a", "0.75 dB"),
        ("out/loud-mono-441k-16.m4a", "3.75 dB"),
    ] {
        let bytes = fs::read(directory.join(file)).unwrap();
        let names: Vec<String> = ilst_freeforms(&bytes)
            .into_iter()
            .map(|item| item.name)
            .collect();
        assert!(
            names.contains(&"REPLAYGAIN_TRACK_GAIN".to_string()),
            "{file}"
        );
        assert!(names.contains(&"iTunNORM".to_string()), "{file}");
        assert!(
            !names.contains(&"REPLAYGAIN_ALBUM_GAIN".to_string()),
            "{names:?}"
        );
        let gain_value = freeform_values(&bytes)
            .into_iter()
            .find(|(name, _)| name == "REPLAYGAIN_TRACK_GAIN")
            .unwrap()
            .1;
        assert_eq!(gain_value, gain, "{file}");
    }
    for result in &outcome.results {
        if let aac_convert::batch::BatchFileOutcome::Converted { tags, .. } = &result.outcome {
            assert!(
                tags.warnings.iter().any(|w| w.contains(
                    "loudness: album is incomplete (unanalyzable: loud-tone-48k-16.flac)"
                )),
                "{:?}",
                tags.warnings
            );
        }
    }
}

#[test]
fn no_tags_adts_and_dry_run_skip_analysis() {
    // `--no-tags`: gapless signalling plus encoder identification only, no
    // warnings at all.
    let directory = TempDir::new("loud-no-tags");
    let output = directory.join("out.m4a");
    let (_, diagnostics) = convert_file_with_format_and_tags(
        &fixture("loud-tone-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        false,
    )
    .unwrap();
    assert!(
        diagnostics.warnings.is_empty(),
        "{:?}",
        diagnostics.warnings
    );
    assert_eq!(diagnostics.mapped, 0);
    let bytes = fs::read(&output).unwrap();
    assert_eq!(
        ilst_layout(&bytes),
        [
            ("----".to_string(), "iTunSMPB".to_string()),
            ("\u{fffd}too".to_string(), String::new()),
        ]
    );

    // ADTS carries no tags regardless of the flag.
    let adts = directory.join("out.aac");
    convert_file_with_format_and_tags(
        &fixture("loud-tone-48k-16.flac"),
        &adts,
        OutputFormat::Adts,
        true,
    )
    .unwrap();
    let adts_bytes = fs::read(&adts).unwrap();
    assert!(!adts_bytes.windows(4).any(|w| w == b"ilst"));

    // Dry runs never decode: even a truncated input plans without failure.
    let batch_dir = TempDir::new("loud-dry-run");
    let input = batch_dir.join("in");
    fs::create_dir(&input).unwrap();
    fs::copy(
        fixture("truncated-stereo-48k-16.flac"),
        input.join("t.flac"),
    )
    .unwrap();
    let plan = plan_batch(&input, &batch_dir.join("out"), OutputFormat::M4a, true).unwrap();
    let outcome = execute_plan_with_tags(&plan, OutputFormat::M4a, true, false, true).unwrap();
    assert_eq!(outcome.summary.converted, 1);
    assert_eq!(outcome.summary.failed, 0);
}

#[test]
fn genuine_decode_failures_keep_their_category() {
    // Loudness analysis cannot disguise a corrupt input as a metadata issue:
    // the conversion still fails with DecodeFailed and publishes nothing.
    let directory = TempDir::new("loud-fail");
    let output = directory.join("out.m4a");
    let err = convert_file_with_format_and_tags(
        &fixture("truncated-stereo-48k-16.flac"),
        &output,
        OutputFormat::M4a,
        true,
    )
    .unwrap_err();
    assert!(
        matches!(err, aac_convert::ConvertError::DecodeFailed),
        "{err:?}"
    );
    assert!(!output.exists());
}
