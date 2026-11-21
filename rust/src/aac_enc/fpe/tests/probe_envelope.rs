//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-19.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! G-1 probe-corpus energy-envelope validation (test-only).
//!
//! Applies the **unchanged, approved** R1 derived energy envelope
//! (`docs/fpe/05` §9.2, `docs/fpe/07` §5) to the six R2.1-B probe
//! materials (Qwen review §10.5 follow-up spec). This closes the G-1
//! acceptance gap: whether the numerical energy differences underlying
//! R2.1-B findings F-03…F-30 are legitimate FPE representation residue.
//!
//! ## Architecture (test-only, no production or envelope change)
//!
//! * PCM comes from the **same** generator functions the R2.1-B harness
//!   uses (`super::is_ms::{probe_sine, probe_narrow}`, `pub(super)` for
//!   this reuse) with the **identical** arguments recorded in
//!   `R2.1-B-owner-review.md` §1 — no substituted signals.
//! * Each probe runs through the **existing** `DiffStream` machinery
//!   (`tests/mod.rs`): persistent per-engine state, the shared
//!   block-switch/transform frontend, `process_pair_reuse` on both
//!   engines, and the established `EnergyAudit` (`audit.rs`) with
//!   `assert_clean` semantics intact. `DiffStream::from_pcm` only
//!   changes the PCM source; operating point, state handling, audit
//!   math, tolerances, and frame accounting are untouched.
//! * 12 probe frames + 2 zero-padded tail frames per material
//!   (`DiffStream::run_stream` accounting, `docs/fpe/07` §7.1).
//! * Per-band evidence comes from the audit's retained `BandRow`s
//!   (recorded with the exact applied terms, `audit.rs`), never from
//!   re-implemented envelope math.
//!
//! ## Scope exclusions (G-2 and later stay out)
//!
//! Attribution-label changes (G-2), ownership pointer checks (G-3), PE/ATS
//! and downstream S2/S4/S6/S8 analysis, B_closed or MS-domain changes,
//! envelope/tolerance changes of any kind, and R2.2 are all out of scope.

use super::audit::{BandRow, EnergyAudit, EnergyKind};
use super::is_ms::{probe_narrow, probe_sine};
use super::DiffStream;
use crate::aac_enc::block_switch::WindowSequence;

// ---------------------------------------------------------------------------
// Probe definitions (identical to R2.1-B `materials()`, §2 scope)
// ---------------------------------------------------------------------------

/// (display name, stereo PCM, sample rate). Arguments match the R2.1-B
/// corpus exactly; any deviation would silently substitute the signal
/// under test and is forbidden.
fn probe_materials() -> Vec<(&'static str, Vec<Vec<i16>>, u32)> {
    vec![
        (
            "probe-panned-48k",
            probe_sine(12, 48000.0, 1000.0, 10_000.0, 0.25, 0.0, Some(4)),
            48000,
        ),
        (
            "probe-outphase-48k",
            probe_sine(
                12,
                48000.0,
                1000.0,
                10_000.0,
                0.5,
                std::f64::consts::PI,
                Some(4),
            ),
            48000,
        ),
        (
            "probe-centred-48k",
            probe_sine(12, 48000.0, 1000.0, 10_000.0, 1.0, 0.0, Some(4)),
            48000,
        ),
        ("probe-narrow-48k", probe_narrow(12, 48000.0, 0.25), 48000),
        (
            "probe-quiet-48k",
            probe_sine(12, 48000.0, 1000.0, 50.0, 0.25, 0.0, Some(4)),
            48000,
        ),
        (
            "probe-panned-44100",
            probe_sine(12, 44100.0, 1000.0, 10_000.0, 0.25, 0.0, Some(4)),
            44100,
        ),
    ]
}

/// Run one probe through the differential harness with the value-audit
/// gate, then print the aggregate evidence summary.
fn run_probe(display: &'static str, pcm: Vec<Vec<i16>>, sample_rate: u32) -> DiffStream {
    let s = DiffStream::run_checked_pcm(display, pcm, sample_rate);
    println!(
        "G1-COVERAGE {display}: frames={} seqs={:?} short_frames={} preecho_disabled={} stale_ms={}",
        s.frames_compared, s.seqs_seen, s.short_frames, s.preecho_disabled_frames, s.stale_ms_reads
    );
    s.audit.print_summary(display);
    s
}

/// Frame accounting + window/transition coverage common to every probe.
fn assert_probe_coverage(s: &DiffStream, display: &str, frames: usize) {
    assert_eq!(
        s.frames_compared, frames,
        "{display}: exact frame accounting (12 probe + 2 tail)"
    );
    assert!(
        s.seen(WindowSequence::Long),
        "{display}: LONG coverage (envelope long path)"
    );
}

/// Find retained BandRows matching (material, frame, kind, channel, w, band).
/// Frame discrimination uses the row tag prefix (`"{material} f{frame} "`),
/// since rows accumulate across the run's frames.
fn find_rows<'a>(
    audit: &'a EnergyAudit,
    material: &str,
    frame: u32,
    kind: EnergyKind,
    ch: usize,
    w: usize,
    b: usize,
) -> Vec<&'a BandRow> {
    let prefix = format!("{material} f{frame} ");
    audit
        .rows
        .iter()
        .filter(|r| {
            r.kind == kind && r.ch == ch && r.w == w && r.b == b && r.tag.starts_with(&prefix)
        })
        .collect()
}

/// Print one adjudicated row in the machine-readable G-1 table format.
fn print_band_row(material: &str, frame_tag: &str, row: &BandRow) {
    println!(
        "G1-BAND {material} {frame_tag} {:?} short={} reused={} ch{} w{} b{} got={} want={} delta={} Qb={:.6} Tb={:.6} R={:.3} sb={} s0={} width={} tol={:.6} util={:.6} supported={} reason={:?} failed={} tag={}",
        row.kind,
        row.short,
        row.reused,
        row.ch,
        row.w,
        row.b,
        row.got,
        row.want,
        row.delta,
        row.q,
        row.t,
        row.r,
        row.scale_b
            .map_or_else(|| "n/a".to_string(), |v| v.to_string()),
        row.scale0
            .map_or_else(|| "n/a".to_string(), |v| v.to_string()),
        row.width
            .map_or_else(|| "n/a".to_string(), |v| v.to_string()),
        row.tol,
        row.util,
        row.supported,
        row.unsupported_reason,
        row.failed,
        row.tag,
    );
}

// ---------------------------------------------------------------------------
// Six G-1 probe scenarios (§10.5 item 1)
// ---------------------------------------------------------------------------

#[test]
fn g1_panned_48k_envelope() {
    let (display, pcm, sr) = &probe_materials()[0];
    let s = run_probe(display, pcm.clone(), *sr);
    assert_probe_coverage(&s, display, 14);
    // Onset at frame 4 must exercise the block-switch transitions.
    assert!(s.seen(WindowSequence::Start), "{display}: START");
    assert!(s.seen(WindowSequence::Short), "{display}: SHORT");
    assert!(s.seen(WindowSequence::Stop), "{display}: STOP");
}

#[test]
fn g1_outphase_48k_envelope() {
    let (display, pcm, sr) = &probe_materials()[1];
    let s = run_probe(display, pcm.clone(), *sr);
    assert_probe_coverage(&s, display, 14);
    assert!(s.seen(WindowSequence::Start), "{display}: START");
    assert!(s.seen(WindowSequence::Short), "{display}: SHORT");
    assert!(s.seen(WindowSequence::Stop), "{display}: STOP");
}

#[test]
fn g1_centred_48k_envelope() {
    let (display, pcm, sr) = &probe_materials()[2];
    let s = run_probe(display, pcm.clone(), *sr);
    assert_probe_coverage(&s, display, 14);
    assert!(s.seen(WindowSequence::Start), "{display}: START");
    assert!(s.seen(WindowSequence::Short), "{display}: SHORT");
    assert!(s.seen(WindowSequence::Stop), "{display}: STOP");
}

#[test]
fn g1_narrow_48k_envelope() {
    let (display, pcm, sr) = &probe_materials()[3];
    let s = run_probe(display, pcm.clone(), *sr);
    assert_probe_coverage(&s, display, 14);
    // Startup transient: SHORT window(s) then steady LONG.
    assert!(s.seen(WindowSequence::Short), "{display}: SHORT");
    // Grouped-energy path must be exercised on this SHORT material.
    assert!(s.audit.grouped.count > 0, "{display}: grouped coverage");
}

#[test]
fn g1_quiet_48k_envelope() {
    let (display, pcm, sr) = &probe_materials()[4];
    let s = run_probe(display, pcm.clone(), *sr);
    assert_probe_coverage(&s, display, 14);
}

#[test]
fn g1_panned_44100_envelope() {
    let (display, pcm, sr) = &probe_materials()[5];
    let s = run_probe(display, pcm.clone(), *sr);
    assert_probe_coverage(&s, display, 14);
    assert!(s.seen(WindowSequence::Start), "{display}: START");
    assert!(s.seen(WindowSequence::Short), "{display}: SHORT");
    assert!(s.seen(WindowSequence::Stop), "{display}: STOP");
}

// ---------------------------------------------------------------------------
// Per-finding adjudication table, F-03…F-30 (§5, §8 item 5)
// ---------------------------------------------------------------------------

/// R2.1-B finding bands mapped to audit rows. LONG probes: rescale rows
/// `(Rescale, long, ch, w=0, b)`. SHORT narrow probe: grouped rows
/// `(Grouped, ch, w=group, b=offset)` with IS band = group*14 + offset
/// (per=14 stride; offsets 0..12 active, 13 is the stride pad with no
/// audit row — asserted absent as the pad-consistency check).
struct FindingBand {
    id: &'static str,
    material: &'static str,
    frame: u32,
    group: usize,
    band: usize,
    short: bool,
}

fn finding_bands() -> Vec<FindingBand> {
    vec![
        FindingBand {
            id: "F-03",
            material: "probe-panned-48k",
            frame: 7,
            group: 0,
            band: 5,
            short: false,
        },
        FindingBand {
            id: "F-04",
            material: "probe-panned-48k",
            frame: 7,
            group: 0,
            band: 13,
            short: false,
        },
        FindingBand {
            id: "F-05",
            material: "probe-panned-48k",
            frame: 8,
            group: 0,
            band: 13,
            short: false,
        },
        FindingBand {
            id: "F-06",
            material: "probe-panned-48k",
            frame: 9,
            group: 0,
            band: 5,
            short: false,
        },
        FindingBand {
            id: "F-07",
            material: "probe-panned-48k",
            frame: 9,
            group: 0,
            band: 13,
            short: false,
        },
        FindingBand {
            id: "F-08",
            material: "probe-panned-48k",
            frame: 10,
            group: 0,
            band: 5,
            short: false,
        },
        FindingBand {
            id: "F-09",
            material: "probe-panned-48k",
            frame: 10,
            group: 0,
            band: 13,
            short: false,
        },
        FindingBand {
            id: "F-10",
            material: "probe-panned-48k",
            frame: 11,
            group: 0,
            band: 13,
            short: false,
        },
        FindingBand {
            id: "F-11",
            material: "probe-outphase-48k",
            frame: 7,
            group: 0,
            band: 5,
            short: false,
        },
        FindingBand {
            id: "F-12",
            material: "probe-outphase-48k",
            frame: 7,
            group: 0,
            band: 13,
            short: false,
        },
        FindingBand {
            id: "F-13",
            material: "probe-outphase-48k",
            frame: 8,
            group: 0,
            band: 13,
            short: false,
        },
        FindingBand {
            id: "F-14",
            material: "probe-outphase-48k",
            frame: 9,
            group: 0,
            band: 5,
            short: false,
        },
        FindingBand {
            id: "F-15",
            material: "probe-outphase-48k",
            frame: 9,
            group: 0,
            band: 13,
            short: false,
        },
        FindingBand {
            id: "F-16",
            material: "probe-outphase-48k",
            frame: 10,
            group: 0,
            band: 5,
            short: false,
        },
        FindingBand {
            id: "F-17",
            material: "probe-outphase-48k",
            frame: 10,
            group: 0,
            band: 13,
            short: false,
        },
        FindingBand {
            id: "F-18",
            material: "probe-outphase-48k",
            frame: 11,
            group: 0,
            band: 13,
            short: false,
        },
        FindingBand {
            id: "F-19",
            material: "probe-narrow-48k",
            frame: 1,
            group: 1,
            band: 9,
            short: true,
        },
        FindingBand {
            id: "F-20",
            material: "probe-narrow-48k",
            frame: 1,
            group: 1,
            band: 10,
            short: true,
        },
        FindingBand {
            id: "F-21",
            material: "probe-narrow-48k",
            frame: 1,
            group: 2,
            band: 9,
            short: true,
        },
        FindingBand {
            id: "F-22",
            material: "probe-narrow-48k",
            frame: 1,
            group: 2,
            band: 10,
            short: true,
        },
        FindingBand {
            id: "F-23",
            material: "probe-narrow-48k",
            frame: 1,
            group: 3,
            band: 1,
            short: true,
        },
        FindingBand {
            id: "F-24",
            material: "probe-narrow-48k",
            frame: 1,
            group: 3,
            band: 2,
            short: true,
        },
        FindingBand {
            id: "F-25",
            material: "probe-narrow-48k",
            frame: 1,
            group: 3,
            band: 9,
            short: true,
        },
        FindingBand {
            id: "F-26",
            material: "probe-narrow-48k",
            frame: 1,
            group: 3,
            band: 10,
            short: true,
        },
        FindingBand {
            id: "F-27",
            material: "probe-panned-44100",
            frame: 7,
            group: 0,
            band: 5,
            short: false,
        },
        FindingBand {
            id: "F-28",
            material: "probe-panned-44100",
            frame: 8,
            group: 0,
            band: 6,
            short: false,
        },
        FindingBand {
            id: "F-29",
            material: "probe-panned-44100",
            frame: 9,
            group: 0,
            band: 15,
            short: false,
        },
        FindingBand {
            id: "F-30",
            material: "probe-panned-44100",
            frame: 10,
            group: 0,
            band: 6,
            short: false,
        },
    ]
}

#[test]
fn g1_finding_band_table() {
    // Adjudicate all 28 independent probe findings (F-03…F-30; F-01/F-02
    // are R1-gated fixture material, F-02 is a consequence of F-01).
    // Every finding band must be supported and within the envelope;
    // any failure or unsupported comparison fails loudly here (G-1 gate).
    let mut audited = 0usize;
    for mat in probe_materials() {
        let s = DiffStream::run_checked_pcm(mat.0, mat.1, mat.2);
        let all = finding_bands();
        let mine: Vec<&FindingBand> = all.iter().filter(|fb| fb.material == mat.0).collect();
        for fb in mine {
            let frame_tag = format!("{} f{}", mat.0, fb.frame);
            if fb.short {
                // Grouped row for IS band = group*14 + offset.
                let rows = find_rows(
                    &s.audit,
                    mat.0,
                    fb.frame,
                    EnergyKind::Grouped,
                    0,
                    fb.group,
                    fb.band,
                );
                assert_eq!(
                    rows.len(),
                    1,
                    "{} {}: exactly one grouped ch0 row",
                    fb.id,
                    frame_tag
                );
                let rows1 = find_rows(
                    &s.audit,
                    mat.0,
                    fb.frame,
                    EnergyKind::Grouped,
                    1,
                    fb.group,
                    fb.band,
                );
                assert_eq!(
                    rows1.len(),
                    1,
                    "{} {}: exactly one grouped ch1 row",
                    fb.id,
                    frame_tag
                );
                for row in rows.iter().chain(rows1.iter()) {
                    assert!(
                        row.supported,
                        "{} {}: grouped band must be supported ({:?})",
                        fb.id, frame_tag, row.unsupported_reason
                    );
                    assert!(
                        !row.failed,
                        "{} {}: grouped band must pass (Δ={} tol={:.3} util={:.4} Q={:.3} T={:.3})",
                        fb.id, frame_tag, row.delta, row.tol, row.util, row.q, row.t
                    );
                    print_band_row(fb.id, &frame_tag, row);
                    audited += 1;
                }
                // Stride-pad bands (offset 13) have no audit rows: the
                // IS stride pads are unaudited and unread downstream.
                let pads = find_rows(
                    &s.audit,
                    mat.0,
                    fb.frame,
                    EnergyKind::Grouped,
                    0,
                    fb.group,
                    13,
                );
                assert!(
                    pads.is_empty(),
                    "{} {}: stride pad must have no grouped row",
                    fb.id,
                    frame_tag
                );
            } else {
                // Long rescale rows, both channels, window 0.
                for ch in 0..2usize {
                    let rows = find_rows(
                        &s.audit,
                        mat.0,
                        fb.frame,
                        EnergyKind::Rescale,
                        ch,
                        0,
                        fb.band,
                    );
                    assert_eq!(
                        rows.len(),
                        1,
                        "{} {} ch{ch}: exactly one long-rescale row",
                        fb.id,
                        frame_tag
                    );
                    let row = rows[0];
                    assert!(
                        row.supported,
                        "{} {} ch{ch}: band must be supported ({:?})",
                        fb.id, frame_tag, row.unsupported_reason
                    );
                    assert!(
                        !row.failed,
                        "{} {} ch{ch}: band must pass (Δ={} tol={:.3} util={:.4} Q={:.3} T={:.3} s_b={:?})",
                        fb.id, frame_tag, row.delta, row.tol, row.util, row.q, row.t, row.scale_b
                    );
                    print_band_row(fb.id, &frame_tag, row);
                    audited += 1;
                }
            }
        }
    }
    assert_eq!(audited, 56, "28 findings × 2 channels");
}

// ---------------------------------------------------------------------------
// Priority adjudications (§5): F-24, Class B transitions, scale edges
// ---------------------------------------------------------------------------

/// F-24 mandatory priority: narrow f1 group 3 / band 2 (audit grouped
/// coordinates), the largest energy delta in the R2.1-B ledger. Prints the
/// grouped row with summed Q_b/T_b/tol/utilisation plus every contributing
/// short-rescale window row with its own s_b and envelope terms, and the
/// sentinel classification for the band.
#[test]
fn g1_f24_priority_adjudication() {
    let s = DiffStream::run_checked_pcm("probe-narrow-48k", probe_narrow(12, 48000.0, 0.25), 48000);
    // Grouped row, both channels.
    for ch in 0..2usize {
        let rows = find_rows(
            &s.audit,
            "probe-narrow-48k",
            1,
            EnergyKind::Grouped,
            ch,
            3,
            2,
        );
        assert_eq!(rows.len(), 1, "F-24 ch{ch}: grouped row");
        let row = rows[0];
        println!(
            "G1-F24 grouped ch{ch}: got={} want={} delta={} Qsum={:.6} Tsum={:.6} Rsum={:.3} tol={:.6} util={:.6} supported={} failed={}",
            row.got, row.want, row.delta, row.q, row.t, row.r,
            row.tol, row.util, row.supported, row.failed
        );
        assert!(row.supported, "F-24 ch{ch}: supported");
        assert!(!row.failed, "F-24 ch{ch}: within envelope");
    }
    // Contributing short-rescale window rows for band 2 (window span comes
    // from the grouped row tag, e.g. g3[w6..8]).
    let mut windows = 0usize;
    for ch in 0..2usize {
        for w in 0..8usize {
            for row in find_rows(
                &s.audit,
                "probe-narrow-48k",
                1,
                EnergyKind::Rescale,
                ch,
                w,
                2,
            ) {
                println!(
                    "G1-F24 window ch{ch} w{w}: got={} want={} delta={} Qb={:.6} Tb={:.6} sb={} s0={} tol={:.6} util={:.6} supported={} failed={} sentinel_sum_abs_x={:?}",
                    row.got, row.want, row.delta, row.q, row.t,
                    row.scale_b.map_or_else(|| "n/a".to_string(), |v| v.to_string()),
                    row.scale0.map_or_else(|| "n/a".to_string(), |v| v.to_string()),
                    row.tol, row.util, row.supported, row.failed, row.sum_abs_x
                );
                assert!(row.supported, "F-24 ch{ch} w{w}: supported");
                assert!(!row.failed, "F-24 ch{ch} w{w}: within envelope");
                windows += 1;
            }
        }
    }
    assert!(windows > 0, "F-24: contributing window rows present");
    // Sentinel classification for the band (from the audit's retained cases).
    let mut sentinel_hits = 0usize;
    for c in s
        .audit
        .sentinels
        .grouped_b_cases
        .iter()
        .chain(s.audit.sentinels.grouped_a_cases.iter())
    {
        if c.tag.starts_with("probe-narrow-48k f1") && c.w == 3 && c.b == 2 {
            println!(
                "G1-F24 sentinel ch{}: class={} E FPE/Q31={}/{} Ld={:.9}/{:.9}",
                c.ch,
                if s.audit
                    .sentinels
                    .grouped_b_cases
                    .iter()
                    .any(|x| { x.tag == c.tag && x.ch == c.ch && x.w == c.w && x.b == c.b })
                {
                    "B"
                } else {
                    "A"
                },
                c.got_e,
                c.want_e,
                c.got_ld,
                c.want_ld
            );
            sentinel_hits += 1;
        }
    }
    println!("G1-F24 sentinel cases at g3/b2: {sentinel_hits}");
}

/// F-20/F-22/F-26 Class B priority: stored 0-vs-1 transitions with Ld MIN
/// on the Q31 side. Confirms actual stored state, mapped-line state,
/// the grouped bound, and the Ld consequence — stored units and Ld
/// units kept distinct throughout.
#[test]
fn g1_class_b_zero_one_transitions() {
    let s = DiffStream::run_checked_pcm("probe-narrow-48k", probe_narrow(12, 48000.0, 0.25), 48000);
    // IS bands 24/38/52 (group offsets 10/10/10) sit on per-window eR
    // stored 0 (Q31) vs 1 (FPE). The audit's rescale rows carry the
    // stored values and the mapped sum_abs_x for the Class B check.
    for (label, grp, off) in [("F-20", 1usize, 10usize), ("F-22", 2, 10), ("F-26", 3, 10)] {
        for ch in 0..2usize {
            // Contributing short-rescale window rows for this grouped band.
            let mut saw = 0usize;
            for w in 0..8usize {
                for row in find_rows(
                    &s.audit,
                    "probe-narrow-48k",
                    1,
                    EnergyKind::Rescale,
                    ch,
                    w,
                    off,
                ) {
                    // Prints all eight windows at band offset 10 under this
                    // finding label; it does not filter by the grouped row's
                    // recorded window span.
                    saw += 1;
                    println!(
                        "G1-CLASSB {label} ch{ch} w{w}: got={} want={} sum_abs_x={:?} sb={} tol={:.3} util={:.4} supported={} failed={}",
                        row.got,
                        row.want,
                        row.sum_abs_x,
                        row.scale_b.map_or_else(|| "n/a".to_string(), |v| v.to_string()),
                        row.tol,
                        row.util,
                        row.supported,
                        row.failed
                    );
                    assert!(row.supported, "{label} ch{ch} w{w}: supported");
                    assert!(!row.failed, "{label} ch{ch} w{w}: within envelope");
                }
            }
            assert!(saw > 0, "{label} ch{ch}: window rows present");
            let grows = find_rows(
                &s.audit,
                "probe-narrow-48k",
                1,
                EnergyKind::Grouped,
                ch,
                grp,
                off,
            );
            assert_eq!(grows.len(), 1, "{label} ch{ch}: grouped row");
            assert!(grows[0].supported, "{label} ch{ch}: grouped supported");
            assert!(!grows[0].failed, "{label} ch{ch}: grouped within envelope");
            println!(
                "G1-CLASSB {label} ch{ch} grouped: got={} want={} delta={} tol={:.3} util={:.4}",
                grows[0].got, grows[0].want, grows[0].delta, grows[0].tol, grows[0].util
            );
        }
        let _ = grp;
    }
}

/// Scale-edge reconfirmation (F-19/F-21, F-23, F-25): the envelope
/// adjudication for the rounded-scale bands, including the F-21/F-22 ≡
/// F-19/F-20 identity check from retained rows (not from prose).
#[test]
fn g1_scale_edge_identity() {
    let s = DiffStream::run_checked_pcm("probe-narrow-48k", probe_narrow(12, 48000.0, 0.25), 48000);
    // F-19 (g1/b9) vs F-21 (g2/b9): identical energies required.
    for ch in 0..2usize {
        let r19 = find_rows(
            &s.audit,
            "probe-narrow-48k",
            1,
            EnergyKind::Grouped,
            ch,
            1,
            9,
        );
        let r21 = find_rows(
            &s.audit,
            "probe-narrow-48k",
            1,
            EnergyKind::Grouped,
            ch,
            2,
            9,
        );
        assert_eq!((r19.len(), r21.len()), (1, 1));
        assert_eq!(
            (r19[0].got, r19[0].want, r19[0].tol.to_bits()),
            (r21[0].got, r21[0].want, r21[0].tol.to_bits()),
            "F-19 ≡ F-21 grouped energies and bounds, ch{ch}"
        );
        let r20 = find_rows(
            &s.audit,
            "probe-narrow-48k",
            1,
            EnergyKind::Grouped,
            ch,
            1,
            10,
        );
        let r22 = find_rows(
            &s.audit,
            "probe-narrow-48k",
            1,
            EnergyKind::Grouped,
            ch,
            2,
            10,
        );
        assert_eq!((r20.len(), r22.len()), (1, 1));
        assert_eq!(
            (r20[0].got, r20[0].want, r20[0].tol.to_bits()),
            (r22[0].got, r22[0].want, r22[0].tol.to_bits()),
            "F-20 ≡ F-22 grouped energies and bounds, ch{ch}"
        );
        for (label, row) in [
            ("F-19", r19[0]),
            ("F-21", r21[0]),
            (
                "F-23",
                find_rows(
                    &s.audit,
                    "probe-narrow-48k",
                    1,
                    EnergyKind::Grouped,
                    ch,
                    3,
                    1,
                )[0],
            ),
            (
                "F-25",
                find_rows(
                    &s.audit,
                    "probe-narrow-48k",
                    1,
                    EnergyKind::Grouped,
                    ch,
                    3,
                    9,
                )[0],
            ),
        ] {
            assert!(row.supported, "{label} ch{ch}: supported");
            assert!(!row.failed, "{label} ch{ch}: within envelope");
            println!(
                "G1-SCALE {label} ch{ch}: got={} want={} delta={} tol={:.3} util={:.4} Q={:.3} T={:.3}",
                row.got, row.want, row.delta, row.tol, row.util, row.q, row.t
            );
        }
    }
}

/// Retained per-window rescale rows for the narrow-probe finding bands
/// (N-6 evidence retention): bands 1 (F-23) and 9 (F-19/F-21/F-25),
/// every window, both channels. Prints `G1-WIN` rows with the exact
/// applied terms and asserts the F-19 (w1) vs F-21 (w4) row identity.
#[test]
fn g1_finding_window_rows() {
    let s = DiffStream::run_checked_pcm("probe-narrow-48k", probe_narrow(12, 48000.0, 0.25), 48000);
    for ch in 0..2usize {
        for b in [1usize, 9] {
            for w in 0..8usize {
                let rows = find_rows(
                    &s.audit,
                    "probe-narrow-48k",
                    1,
                    EnergyKind::Rescale,
                    ch,
                    w,
                    b,
                );
                assert_eq!(rows.len(), 1, "band {b} ch{ch} w{w}: one row");
                let row = rows[0];
                assert!(row.supported, "band {b} ch{ch} w{w}: supported");
                assert!(!row.failed, "band {b} ch{ch} w{w}: within envelope");
                println!(
                    "G1-WIN narrow f1 ch{ch} w{w} b{b} got={} want={} delta={} Qb={:.6} Tb={:.6} sb={} s0={} width={:?} sum_abs_x={:?} tol={:.6} util={:.6} supported={} failed={}",
                    row.got,
                    row.want,
                    row.delta,
                    row.q,
                    row.t,
                    row.scale_b
                        .map_or_else(|| "n/a".to_string(), |v| v.to_string()),
                    row.scale0
                        .map_or_else(|| "n/a".to_string(), |v| v.to_string()),
                    row.width,
                    row.sum_abs_x,
                    row.tol,
                    row.util,
                    row.supported,
                    row.failed
                );
            }
        }
        // F-19 (group g1, contributing window w1) vs F-21 (group g2,
        // contributing window w4): row-for-row identity on band 9.
        let r1 = find_rows(
            &s.audit,
            "probe-narrow-48k",
            1,
            EnergyKind::Rescale,
            ch,
            1,
            9,
        )[0];
        let r2 = find_rows(
            &s.audit,
            "probe-narrow-48k",
            1,
            EnergyKind::Rescale,
            ch,
            4,
            9,
        )[0];
        assert_eq!(
            (
                r1.got,
                r1.want,
                r1.delta,
                r1.tol.to_bits(),
                r1.q.to_bits(),
                r1.t.to_bits(),
                r1.scale_b,
                r1.scale0
            ),
            (
                r2.got,
                r2.want,
                r2.delta,
                r2.tol.to_bits(),
                r2.q.to_bits(),
                r2.t.to_bits(),
                r2.scale_b,
                r2.scale0
            ),
            "F-19 w1 ≡ F-21 w4 ch{ch}"
        );
    }
}

// ---------------------------------------------------------------------------
// Aggregate coverage and sentinel report (§4, §8 items 4/6)
// ---------------------------------------------------------------------------

#[test]
fn g1_probe_corpus_aggregate() {
    // Aggregate all six probes: every energy/Ld class exercised, both
    // identity gates checked, Class A/B sentinels classified, zero
    // unsupported comparisons (unsupported ≠ pass; assert_clean already
    // enforced per scenario, the merge re-checks the union).
    let mut total = EnergyAudit::default();
    let mut frames = 0usize;
    for (display, pcm, sr) in probe_materials() {
        let s = DiffStream::run_pcm(display, pcm, sr);
        frames += s.frames_compared;
        total.merge(&s.audit);
    }
    assert_eq!(frames, 6 * 14, "aggregate frame accounting (6 × 14)");
    total.assert_clean("probe corpus aggregate");
    total.print_summary("probe corpus aggregate");
    assert!(total.long_rescale.count > 0, "long rescale coverage");
    assert!(total.short_rescale.count > 0, "short rescale coverage");
    assert!(total.long_spread.count > 0, "long spread coverage");
    assert!(total.short_spread.count > 0, "short spread coverage");
    assert!(total.grouped.count > 0, "grouped coverage");
    assert!(total.ld.window_energy.count > 0, "per-window Ld coverage");
    assert!(total.ld.grouped_energy.count > 0, "grouped Ld coverage");
    assert!(total.ld.threshold.count > 0, "threshold Ld coverage");
    assert!(total.identity.q31.checked > 0, "Q31 identity coverage");
    assert!(total.identity.fpe.checked > 0, "FPE identity coverage");
    assert!(total.identity.decomp.checked > 0, "decomposition coverage");
    assert!(
        total.sentinels.long_b + total.sentinels.short_b + total.sentinels.grouped_b > 0,
        "Class B exercised on the probe corpus"
    );
    assert!(
        total.sentinels.long_a + total.sentinels.short_a > 0,
        "Class A exercised on the probe corpus"
    );
    // Exact sentinel-class counts (deterministic corpus; pinning turns
    // any numerical drift into a loud failure rather than silent drift).
    assert_eq!(total.sentinels.long_a, 74, "Class A long count");
    assert_eq!(total.sentinels.short_a, 0, "Class A short count");
    assert_eq!(total.sentinels.long_b, 221, "Class B long count");
    assert_eq!(total.sentinels.short_b, 3, "Class B short count");
    assert_eq!(total.sentinels.grouped_a, 0, "Class grouped-A count");
    assert_eq!(total.sentinels.grouped_b, 3, "Class grouped-B count");
    assert_eq!(
        total.sentinels.long_spread_a, 0,
        "spread Class A long count"
    );
    assert_eq!(
        total.sentinels.short_spread_a, 0,
        "spread Class A short count"
    );
    assert_eq!(
        total.sentinels.long_spread_b, 78,
        "spread Class B long count"
    );
    assert_eq!(
        total.sentinels.short_spread_b, 0,
        "spread Class B short count"
    );
    assert_eq!(
        total.short_tns_spec_shift_seen, 0,
        "F-6 short-TNS tnsSpecShift must be unobserved on the probe corpus"
    );
    println!(
        "G1-AGGREGATE sentinel A(long {}, short {}) B(long {}, short {}) grouped(A {}, B {}) F-6 tnsSpecShift observations: {}",
        total.sentinels.long_a,
        total.sentinels.short_a,
        total.sentinels.long_b,
        total.sentinels.short_b,
        total.sentinels.grouped_a,
        total.sentinels.grouped_b,
        total.short_tns_spec_shift_seen
    );
    println!(
        "G1-AGGREGATE maxima: max_util(long {:.4}, short {:.4}, grouped {:.4}) max|ΔLd|(window {:.6}, grouped {:.6}, threshold {:.6})",
        total.long_rescale.max_util,
        total.short_rescale.max_util,
        total.grouped.max_util,
        total.ld.window_energy.max_abs,
        total.ld.grouped_energy.max_abs,
        total.ld.threshold.max_abs
    );
}

/// Retained maximum-utilisation rows (N-1/N-5 verification): the overall
/// corpus maximum, the prescribed-frame (non-tail) maximum, and the
/// finding-band maximum. All three must be supported and passing; their
/// exact terms are printed as `G1-MAX` evidence.
#[test]
fn g1_maximum_rows() {
    fn tag_frame(row: &BandRow) -> u32 {
        let t = row.tag.split_whitespace().nth(1).expect("frame token");
        assert!(t.starts_with('f'), "frame token {}", row.tag);
        t[1..].parse().expect("frame number")
    }
    fn tag_material(row: &BandRow) -> &str {
        row.tag.split_whitespace().next().expect("material token")
    }
    let mut total = EnergyAudit::default();
    for (display, pcm, sr) in probe_materials() {
        let s = DiffStream::run_checked_pcm(display, pcm, sr);
        total.merge(&s.audit);
    }
    total.assert_clean("probe corpus maxima");
    fn max_util_row<'a>(rows: &[&'a BandRow]) -> &'a BandRow {
        let best = rows
            .iter()
            .max_by(|a, b| a.util.partial_cmp(&b.util).expect("finite util"))
            .expect("nonempty");
        best
    }
    let supported: Vec<&BandRow> = total.rows.iter().filter(|r| r.supported).collect();
    assert!(!supported.is_empty(), "rows present");
    // Overall maximum (tail frames included).
    let overall = max_util_row(&supported);
    println!(
        "G1-MAX overall {} frame={} kind={:?} ch{} w{} b{} got={} want={} delta={} Qb={:.6} Tb={:.6} tol={:.6} util={:.6}",
        tag_material(overall),
        tag_frame(overall),
        overall.kind,
        overall.ch,
        overall.w,
        overall.b,
        overall.got,
        overall.want,
        overall.delta,
        overall.q,
        overall.t,
        overall.tol,
        overall.util
    );
    assert_eq!(tag_material(overall), "probe-quiet-48k");
    assert_eq!(tag_frame(overall), 12);
    assert_eq!((overall.ch, overall.w, overall.b), (0, 0, 15));
    assert_eq!(overall.delta, 15);
    assert!((overall.util - 0.118164).abs() < 1e-6, "overall max util");
    // The rescale row at the same band shares the maximum: the spread
    // bound propagates the rescale bound unchanged at this band, so the
    // "long rescale" attribution in the report is exact.
    let orow = find_rows(&total, "probe-quiet-48k", 12, EnergyKind::Rescale, 0, 0, 15);
    assert_eq!(orow.len(), 1, "overall-max rescale row present");
    assert!(
        (orow[0].util - overall.util).abs() < 1e-12,
        "rescale row shares the overall max util"
    );
    // Prescribed-frame maximum (12 signal frames; tail f12/f13 excluded).
    let prescribed: Vec<&BandRow> = supported
        .iter()
        .filter(|r| tag_frame(r) < 12)
        .copied()
        .collect();
    let pmax = max_util_row(&prescribed);
    println!(
        "G1-MAX prescribed {} frame={} kind={:?} ch{} w{} b{} got={} want={} delta={} Qb={:.6} Tb={:.6} tol={:.6} util={:.6}",
        tag_material(pmax),
        tag_frame(pmax),
        pmax.kind,
        pmax.ch,
        pmax.w,
        pmax.b,
        pmax.got,
        pmax.want,
        pmax.delta,
        pmax.q,
        pmax.t,
        pmax.tol,
        pmax.util
    );
    assert_eq!(tag_material(pmax), "probe-quiet-48k");
    assert_eq!(tag_frame(pmax), 11);
    assert_eq!((pmax.ch, pmax.w, pmax.b), (0, 0, 33));
    assert_eq!(pmax.delta, 4);
    assert!((pmax.util - 0.079051).abs() < 1e-6, "prescribed max util");
    let prow = find_rows(&total, "probe-quiet-48k", 11, EnergyKind::Rescale, 0, 0, 33);
    assert_eq!(prow.len(), 1, "prescribed-max rescale row present");
    assert!(
        (prow[0].util - pmax.util).abs() < 1e-12,
        "rescale row shares the prescribed max util"
    );
    // Finding-band maximum over the 28 adjudicated bands.
    let mut fmax: Option<&BandRow> = None;
    for fb in finding_bands() {
        let kind = if fb.short {
            EnergyKind::Grouped
        } else {
            EnergyKind::Rescale
        };
        for ch in 0..2usize {
            let w = if fb.short { fb.group } else { 0 };
            for row in find_rows(&total, fb.material, fb.frame, kind, ch, w, fb.band) {
                let better = match fmax {
                    None => true,
                    Some(m) => row.util > m.util,
                };
                if better {
                    fmax = Some(row);
                }
            }
        }
    }
    let fmax = fmax.expect("finding rows present");
    println!(
        "G1-MAX finding {} frame={} kind={:?} ch{} w{} b{} got={} want={} delta={} tol={:.6} util={:.6}",
        tag_material(fmax),
        tag_frame(fmax),
        fmax.kind,
        fmax.ch,
        fmax.w,
        fmax.b,
        fmax.got,
        fmax.want,
        fmax.delta,
        fmax.tol,
        fmax.util
    );
    assert_eq!(tag_material(fmax), "probe-narrow-48k");
    assert_eq!(tag_frame(fmax), 1);
    assert_eq!((fmax.ch, fmax.w, fmax.b), (1, 3, 10));
    assert_eq!(fmax.delta, 1);
    assert!(
        (fmax.util - 0.03125).abs() < 1e-12,
        "finding max util is F-26"
    );
}
