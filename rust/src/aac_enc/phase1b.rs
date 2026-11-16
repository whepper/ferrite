//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! Phase 1B isolated oracle-fed differential (test-only diagnostic).
//!
//! Feeds committed oracle S1/S2 vectors into the existing Rust S4 stage,
//! then (only where S4 is understood) oracle-derived S4 results into the
//! Rust S6 stage. Fresh state per frame; no `catch_unwind`; a failed frame
//! never donates state to another frame.
//!
//! Frames (all stereo; see rationale inline):
//! - `sine-stereo f5`: LONG_WINDOW tonal frame with previously observed
//!   streaming divergence.
//! - `onset-stereo f24`: SHORT_WINDOW stereo transient frame.
//! - `stress-stereo f5`: full-scale PRNG+tone surrogate for an S4/QC-risk
//!   frame. No streaming-failure identity survives in the worktree (Phase 1
//!   logs went to `$PHASE1_OUT`, uncommitted), and this frame is the
//!   weakest-covered committed one: absent from `S4_CASES`/`S5_CASES`, no
//!   `seq_stress-stereo.txt`, no `qc3` gain grid.
//!
//! Classifications used below: 1 = exact stage parity; 2 = divergence from a
//! known upstream input difference; 3 = missing/mismatched state input;
//! 4 = reproducible Rust-stage mismatch on identical oracle inputs;
//! 5 = inconclusive (insufficient oracle data).

use super::bitcount::{dyn_bit_count, BitCountInput};
use super::rate::{
    calc_max_value_in_sfb, qc_main_frame, BitResMode, DistributeState, QcChannelInput,
    QcFrameInput, QcState, MAX_ITERATIONS,
};
use super::scf::{estimate_scale_factors_channel, ScfChannelInput, ScfGeometry};
use super::static_bits::MsDigest;
use super::testdata;
use super::tns::TnsChannel;

/// (fixture, frame, role).
const FRAMES: [(&str, u32, &str); 3] = [
    ("sine-stereo", 5, "LONG tonal divergence frame"),
    ("onset-stereo", 24, "SHORT stereo transient frame"),
    (
        "stress-stereo",
        5,
        "S4/QC-risk surrogate (uncovered by S4/S5 corpus)",
    ),
];

fn init_tag(name: &str) -> &'static str {
    if name.contains("44100") {
        "stereo-44100"
    } else {
        "stereo-48k"
    }
}

/// S4 input spectrum: qc1 post-MS spectrum with inactive-band lines zeroed
/// (same contract as `scf::tests::s4_input_spec`; duplicated here so this
/// diagnostic does not depend on another test module's private helper).
fn s4_input_spec(spec: &[i32], offsets: &[i32], cnt: usize, per: usize, max: usize) -> Vec<i32> {
    let mut v = spec.to_vec();
    let mut o = 0;
    while o < cnt {
        for b in max..per {
            let start = offsets[o + b] as usize;
            let end = offsets[o + b + 1] as usize;
            for line in start..end {
                v[line] = 0;
            }
        }
        o += per;
    }
    v
}

fn forge_dist(init: &testdata::S6Init, gstate: &[i32]) -> DistributeState {
    assert_eq!(gstate.len(), 6);
    DistributeState {
        pe_min: gstate[4],
        pe_max: gstate[5],
        bits2pe_factor_m: init.b2m,
        bits2pe_factor_e: init.b2e,
        pe_correction_m: gstate[2],
        pe_correction_e: gstate[3],
        pe_last: gstate[0],
        dyn_bits_last: gstate[1],
    }
}

fn first_diff(a: &[i32], b: &[i32]) -> Option<(usize, i32, i32)> {
    a.iter()
        .zip(b.iter())
        .enumerate()
        .find_map(|(i, (&x, &y))| if x != y { Some((i, x, y)) } else { None })
}

fn first_diff16(a: &[i16], b: &[i16]) -> Option<(usize, i16, i16)> {
    a.iter()
        .zip(b.iter())
        .enumerate()
        .find_map(|(i, (&x, &y))| if x != y { Some((i, x, y)) } else { None })
}

fn count_diff(a: &[i32], b: &[i32]) -> usize {
    a.iter().zip(b.iter()).filter(|(x, y)| x != y).count()
}

/// Required committed oracle files for one frame. A missing file is an
/// oracle gap (class 5 for the stages that need it), never silently
/// substituted with live-chain values.
fn oracle_gap(name: &str, frame: u32) -> Vec<String> {
    let mut gaps = Vec::new();
    for tag in ["qc1", "qc2", "qc4", "qc5", "qc6"] {
        let p = testdata::testdata_path(&format!("{tag}_{name}_f{frame}.txt"));
        if !p.exists() {
            gaps.push(format!("missing {tag}_{name}_f{frame}.txt"));
        }
    }
    gaps
}

#[test]
fn phase1b_isolated_differential() {
    let mut report = String::new();
    let mut failures: Vec<String> = Vec::new();
    // Fresh default TNS per channel, matching the `s6_corpus_exact`
    // convention. All three oracle frames have npass=1/crash=-1, so the
    // TNS-dependent crash-recount paths are inactive; the TNS choice is
    // recorded, not assumed. Oracle STNSW decisions are provenance only.
    let tns_defaults = [TnsChannel::default(), TnsChannel::default()];

    for (name, f, role) in FRAMES {
        report.push_str(&format!("FRAME {name} f={f} ({role})\n"));
        let gaps = oracle_gap(name, f);
        if !gaps.is_empty() {
            report.push_str(&format!("  class=5 oracle gap: {}\n", gaps.join(", ")));
            failures.push(format!("{name} f={f}: oracle gap"));
            continue;
        }
        let s1 = testdata::load_s1(name, f);
        let s2 = testdata::load_s2(name, f);
        let s4 = testdata::load_s4(name, f);
        let s5 = testdata::load_s5(name, f);
        let s6 = testdata::load_s6(name, f);
        let init = testdata::load_s6init(init_tag(name));
        report.push_str(&format!(
            "  state: ATS pre={:?} post={:?} GSTATE pre={:?} S6BITR avg={} padRest={} globHdr={}\n",
            s1.ats_pre, s1.ats_post, s6.gstate, s6.avg_total, s6.pad_rest, s6.glob_hdr
        ));
        report.push_str(&format!(
            "  geometry: ch={} seq={:?} cnt/per/max={}/{}/{} offsets==s4==s5: {} static={} PE={:?} MSD={:?}\n",
            s1.ch.len(),
            s1.ch.iter().map(|c| c.geo_seq).collect::<Vec<_>>(),
            s1.ch[0].geo_cnt,
            s1.ch[0].geo_per,
            s1.ch[0].geo_max,
            s4.ch.iter().zip(s5.ch.iter()).all(|(a, b)| a.geo_cnt == b.geo_cnt
                && a.offsets == b.offsets),
            s1.static_bits,
            s1.pe,
            s1.msd.as_ref().map(|(d, m)| (*d, m.len())),
        ));

        // ---- B. Isolate S4 (oracle S1/S2 in, production inv=2 path) ----
        let mut s4_exact = true;
        // Live S4 outputs, kept for the S4->S5 handoff check.
        let mut live_q: Vec<Vec<i16>> = Vec::new();
        let mut live_m: Vec<Vec<i32>> = Vec::new();
        let mut live_scf: Vec<Vec<i32>> = Vec::new();
        for (ci, ((c1, c2), c4)) in s1.ch.iter().zip(s2.ch.iter()).zip(s4.ch.iter()).enumerate() {
            assert_eq!(c4.geo_cnt, c1.geo_cnt, "{name} f={f} ch={ci} cnt");
            assert_eq!(c4.offsets, c1.offsets, "{name} f={f} ch={ci} off");
            assert_eq!(c2.geo_cnt, c1.geo_cnt, "{name} f={f} ch={ci} s2cnt");
            let lv = c4.levels.iter().find(|l| l.inv == 2).expect("inv=2 level");
            let mut spec = s4_input_spec(&c1.spec, &c1.offsets, c1.geo_cnt, c1.geo_per, c1.geo_max);
            let mut input = ScfChannelInput {
                spectrum: &mut spec,
                energy_ld: &c1.penld,
                threshold_ld: &c2.thrv,
                form_factor_ld: &c1.ff,
                geo: ScfGeometry {
                    offsets: &c1.offsets,
                    sfb_cnt: c1.geo_cnt,
                    sfb_per_group: c1.geo_per,
                    max_sfb_per_group: c1.geo_max,
                },
            };
            // No catch_unwind: an envelope panic fails this frame loudly and
            // its state object is discarded, never reused.
            let out = estimate_scale_factors_channel(&mut input);
            live_q.push(out.quant_spec.clone());
            live_m.push(out.mutated_spectrum.clone());
            live_scf.push(out.scf.clone());
            let mut notes = Vec::new();
            if out.scf != lv.scf {
                let n = count_diff(&out.scf, &lv.scf);
                let fd = first_diff(&out.scf, &lv.scf);
                notes.push(format!("SCF {n} bands diff, first {fd:?}"));
            }
            if out.global_gain != lv.gain {
                notes.push(format!("GAIN got {} want {}", out.global_gain, lv.gain));
            }
            if out.quant_spec != lv.qspec {
                let n = out
                    .quant_spec
                    .iter()
                    .zip(lv.qspec.iter())
                    .filter(|(a, b)| a != b)
                    .count();
                let fd = first_diff16(&out.quant_spec, &lv.qspec);
                notes.push(format!("Q {n}/1024 lines diff, first {fd:?}"));
            }
            if out.mutated_spectrum != lv.spec {
                let n = count_diff(&out.mutated_spectrum, &lv.spec);
                let fd = first_diff(&out.mutated_spectrum, &lv.spec);
                notes.push(format!("SPEC {n}/1024 lines diff, first {fd:?}"));
            }
            if notes.is_empty() {
                report.push_str(&format!("  S4 ch={ci}: class=1 exact (scf/gain/Q/SPEC)\n"));
            } else {
                s4_exact = false;
                report.push_str(&format!(
                    "  S4 ch={ci}: class=4 mismatch: {}\n",
                    notes.join("; ")
                ));
                failures.push(format!("{name} f={f} ch={ci} S4: {}", notes.join("; ")));
            }
        }

        // ---- S4->S5 handoff on the live S4 outputs (bit-count check) ----
        // Only meaningful where S4 is understood; otherwise stop.
        let mut s5_exact = false;
        if s4_exact {
            s5_exact = true;
            for (ci, ((c1, c5), (q, scf))) in s1
                .ch
                .iter()
                .zip(s5.ch.iter())
                .zip(live_q.iter().zip(live_scf.iter()))
                .enumerate()
            {
                let mut maxv = vec![0u32; c5.geo_cnt];
                calc_max_value_in_sfb(
                    c5.geo_cnt,
                    c5.geo_max,
                    c5.geo_per,
                    &c5.offsets,
                    q,
                    &mut maxv,
                );
                let input = BitCountInput {
                    quant_spec: q,
                    max_value_in_sfb: &maxv,
                    scf,
                    block_type: c1.geo_seq as i32,
                    sfb_cnt: c5.geo_cnt,
                    max_sfb_per_group: c5.geo_max,
                    sfb_per_group: c5.geo_per,
                    sfb_offsets: &c5.offsets,
                    noise_nrg: &c5.noise_nrg,
                    is_book: &c5.is_book,
                    is_scale: &c5.is_scale,
                    use_vcb11: false,
                };
                let (sd, dyn_used) = dyn_bit_count(&input);
                let mut notes = Vec::new();
                if sd.sections.len() != c5.sections.len() {
                    notes.push(format!(
                        "nsec got {} want {}",
                        sd.sections.len(),
                        c5.sections.len()
                    ));
                } else {
                    for (si, (got, exp)) in sd.sections.iter().zip(c5.sections.iter()).enumerate() {
                        if got.code_book != exp.book
                            || got.sfb_start != exp.start
                            || got.sfb_cnt != exp.cnt
                            || got.section_bits != exp.bits
                        {
                            notes.push(format!(
                                "sec{si} got {}/{}/{}/{} want {}/{}/{}/{}",
                                got.code_book,
                                got.sfb_start,
                                got.sfb_cnt,
                                got.section_bits,
                                exp.book,
                                exp.start,
                                exp.cnt,
                                exp.bits
                            ));
                            break;
                        }
                    }
                }
                for (k, got, want) in [
                    ("side", sd.side_info_bits, c5.side),
                    ("huff", sd.huffman_bits, c5.huff),
                    ("scf", sd.scalefac_bits, c5.scf_bits),
                    ("noise", sd.noise_nrg_bits, c5.noise_bits),
                    ("first", sd.first_scf, c5.first),
                    ("dyn", dyn_used, c5.dyn_used),
                ] {
                    if got != want {
                        notes.push(format!("{k} got {got} want {want}"));
                    }
                }
                if notes.is_empty() {
                    report.push_str(&format!(
                        "  S5 ch={ci}: class=1 exact (live-S4-fed sections+counts)\n"
                    ));
                } else {
                    s5_exact = false;
                    report.push_str(&format!(
                        "  S5 ch={ci}: class=4 mismatch: {}\n",
                        notes.join("; ")
                    ));
                    failures.push(format!("{name} f={f} ch={ci} S5: {}", notes.join("; ")));
                }
            }
        } else {
            report.push_str("  S5: skipped (S4 not understood; no substitution)\n");
        }

        // ---- C. Isolate S6 (oracle-derived S4 in, forged pre-state) ----
        // Only where S4 inputs and outputs are understood.
        if s4_exact {
            let mut channels = Vec::new();
            for (ci, ((c1, c4), c5)) in s1.ch.iter().zip(s4.ch.iter()).zip(s5.ch.iter()).enumerate()
            {
                let lv = c4.levels.iter().find(|l| l.inv == 2).expect("inv=2 level");
                channels.push(QcChannelInput {
                    spectrum: &lv.spec,
                    scf: &lv.scf,
                    global_gain: lv.gain,
                    sfb_cnt: c5.geo_cnt,
                    max_sfb_per_group: c5.geo_max,
                    sfb_per_group: c5.geo_per,
                    sfb_offsets: &c5.offsets,
                    block_type: c1.geo_seq as i32,
                    noise_nrg: &c5.noise_nrg,
                    is_book: &c5.is_book,
                    is_scale: &c5.is_scale,
                    tns: &tns_defaults[ci],
                    seed_quant: None,
                });
            }
            let digest = match &s1.msd {
                Some((d, _)) => MsDigest::from_i32(*d).expect("ms digest"),
                None => MsDigest::None,
            };
            let input = QcFrameInput {
                channels,
                no_red_pe: s1.pe[0],
                static_bits: s1.static_bits,
                ms_digest: digest,
                glob_hdr_bits: 56,
                max_bits_per_frame: init.max,
                max_bits_el: init.maxbits_el,
                relative_bits_el: init.relbits_el,
                max_bit_fac: init.maxbitfac,
                bit_res_mode: BitResMode::Full,
            };
            // Fresh state forged from S6INIT + this frame's S6GSTATE pre;
            // never chained from another frame, never reused after failure.
            let mut dist = forge_dist(&init, &s6.gstate);
            let mut qc = QcState {
                bit_res_tot: init.max,
                bit_res_tot_max: init.max,
                bc_scratch: Default::default(),
            };
            match qc_main_frame(&mut qc, &mut dist, &input, MAX_ITERATIONS) {
                Err(e) => {
                    report.push_str(&format!(
                        "  S6: class=4 Rust error {e:?} (oracle converges)\n"
                    ));
                    failures.push(format!("{name} f={f} S6 error: {e:?}"));
                }
                Ok(out) => {
                    let mut notes = Vec::new();
                    for (k, got, want) in [
                        ("granted", out.granted_dyn_bits, s6.granted),
                        ("maxdyn", out.max_dyn_bits, s6.maxdyn),
                        ("avail", out.total_avail, s6.avail),
                        ("elgrant", out.el_granted, s6.elgrant),
                        ("gpe", out.granted_pe, s6.gpe),
                        ("gpecorr", out.granted_pe_corr, s6.gpecorr),
                        ("reslevel", out.bit_res_level_el, s6.reslevel),
                        ("resmax", out.bit_res_max_el, s6.resmax),
                        ("static", out.static_bits_used, s6.static_),
                        ("useddyn", out.used_dyn_bits, s6.useddyn),
                        ("totfill", out.tot_fill_bits, s6.totfill),
                        ("align", out.align_bits, s6.align),
                        ("total", out.total_bits, s6.total),
                        ("bitres", out.bit_res_tot, s6.bitres),
                    ] {
                        if got != want {
                            notes.push(format!("{k} got {got} want {want}"));
                        }
                    }
                    if out.passes.len() != s6.npass {
                        notes.push(format!("npass got {} want {}", out.passes.len(), s6.npass));
                    } else {
                        for (pi, (got, exp)) in out.passes.iter().zip(s6.passes.iter()).enumerate()
                        {
                            if got.iterations != exp.iter
                                || got.decrease != exp.dec
                                || got.dyn_bits[0] != exp.dyn0
                                || got.dyn_bits[1] != exp.dyn1
                                || got.gains[0] != exp.gain0
                                || got.gains[1] != exp.gain1
                                || got.maxq[0] != exp.maxq0
                                || got.maxq[1] != exp.maxq1
                                || got.el_dyn != exp.eldyn
                                || got.el_static != exp.elstat
                                || got.crash != exp.crash
                            {
                                notes.push(format!(
                                    "pass{pi} got it={} dec={} dyn={:?} gain={:?} maxq={:?} el={}/{} crash={} want it={} dec={} dyn={}/{} gain={}/{} maxq={}/{} el={}/{} crash={}",
                                    got.iterations,
                                    got.decrease,
                                    got.dyn_bits,
                                    got.gains,
                                    got.maxq,
                                    got.el_dyn,
                                    got.el_static,
                                    got.crash,
                                    exp.iter,
                                    exp.dec,
                                    exp.dyn0,
                                    exp.dyn1,
                                    exp.gain0,
                                    exp.gain1,
                                    exp.maxq0,
                                    exp.maxq1,
                                    exp.eldyn,
                                    exp.elstat,
                                    exp.crash
                                ));
                            }
                        }
                    }
                    let gp = &s6.gpost;
                    for (k, got, want) in [
                        ("peLast", dist.pe_last, gp[0]),
                        ("dynLast", dist.dyn_bits_last, gp[1]),
                        ("corrM", dist.pe_correction_m, gp[2]),
                        ("corrE", dist.pe_correction_e, gp[3]),
                        ("peMin", dist.pe_min, gp[4]),
                        ("peMax", dist.pe_max, gp[5]),
                    ] {
                        if got != want {
                            notes.push(format!("post-{k} got {got} want {want}"));
                        }
                    }
                    if notes.is_empty() {
                        report.push_str(&format!(
                            "  S6: class=1 exact (grants/passes/finals/post-state; final_gains={:?} final_maxSfb={:?})\n",
                            out.final_gains, out.final_max_sfb
                        ));
                    } else {
                        report.push_str(&format!("  S6: class=4 mismatch: {}\n", notes.join("; ")));
                        failures.push(format!("{name} f={f} S6: {}", notes.join("; ")));
                    }
                    let _ = s5_exact;
                }
            }
            // Oracle gap (reported, not inferred): the S6 fixture exposes no
            // final sections; section-identity beyond pass gains/totals would
            // need the S7/S8 writer check, out of scope here.
            report.push_str("  S6 sections: class=5 gap (no S6 oracle sections; S7/S8 writer check out of scope)\n");
        } else {
            report.push_str("  S6: skipped (S4 not understood; no substitution)\n");
        }
    }
    println!("{report}");
    assert!(
        failures.is_empty(),
        "isolated mismatches:\n{}",
        failures.join("\n")
    );
}
