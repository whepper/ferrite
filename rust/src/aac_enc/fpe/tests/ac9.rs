// SPDX-License-Identifier: FDK-AAC
//
// Part of Ferrite, a Third-Party Modified Version of the
// Fraunhofer FDK AAC Codec Library for Android.
// See rust/NOTICE and rust/LICENSE-FDK-AAC for the complete license text,
// modification notice, and date.
//! R2.2 AC-9 exact-margin evidence (test-only).
//!
//! Adds a read-only evidence surface over the existing, reviewed Group D
//! machinery. It does not alter a production decision, numerical kernel,
//! threshold, tolerance, baseline pin, schema, or accepted evidence bundle.
//! Every value printed by [`super::is_ms::r22_ac9_exact_margin_evidence`] is an
//! integer operand captured from production (or an exact production-derived
//! intermediate), never a rounded display value.

use std::collections::BTreeMap;

use crate::aac_enc::fpe::tests::is_ms::{r21b_affected_frames, EngineTag, R21BHarness};
use crate::aac_enc::fpe::tests::r22_capture::replay_frame;
use crate::aac_enc::fpe::tests::s4_step3::{collect_probe_frames, verify_probe_frame};
use crate::aac_enc::fpe::tests::trace::{
    ah_clamp_margin, ah_classify_margins, chaos_floor_margin, chaos_patch_arm_margin,
    chaos_patch_hi_margin, chaos_patch_mid_margin, enfac_energy_gate_margin, pe_c1_branch_margin,
    pe_calc_admission_margin, pe_prepare_admission_margin, step3_dist_margin, step3_energy_margin,
    step3_pe_gate_margin, step3_sum_margin,
};
use crate::aac_enc::fpe::tests::{is_ms, trace};
use crate::aac_enc::intensity::{is_probe, sfb_correlation};
use crate::aac_enc::q31::{calc_ld_data, calc_ld_int, fmult, fmult_div2, shift_int};
use crate::aac_enc::rate::{GLOBAL_EXT_BITS, MAX_ITERATIONS, MIN_BUFSIZE_PER_EFF_CHAN};
use crate::aac_enc::stream_driver::{FrameSnapshot, R22S2Cap};

const MIN_LDTHRESH: i32 = 0xbe00_0000u32 as i32;
const NEG_HALF: i32 = 0xc000_0000u32 as i32;
const ONE: i32 = i32::MAX;
const ZERO: i32 = 0;
const AVG_COMP: i32 = 0x0c00_0000;
const SHORT_THR_FAC: i32 = 0x5a3d7080;
const INV_SQRT4: [i32; 8] = [
    0x7fff_ffff,
    0x7fff_ffff,
    0x6ba2_7e65,
    0x6142_4bb5,
    0x5a82_7999,
    0x5599_4845,
    0x51c8_e33c,
    0x4eb1_60d1,
];
const LIMIT: i32 = 0x0000_8000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ExactMargin {
    pub(super) name: &'static str,
    pub(super) stage: &'static str,
    pub(super) material: String,
    pub(super) frame: u32,
    pub(super) channel: Option<usize>,
    pub(super) band: usize,
    pub(super) arm: &'static str,
    pub(super) operands: String,
    pub(super) equation: &'static str,
    pub(super) threshold: &'static str,
    pub(super) value: Option<i64>,
    pub(super) decision: &'static str,
    pub(super) equality: &'static str,
    pub(super) evidence_class: &'static str,
    pub(super) probe: &'static str,
    pub(super) source: &'static str,
    pub(super) occurrence: String,
}

impl ExactMargin {
    pub(super) fn render(&self) -> String {
        format!(
            "AC9-MARGIN\t{}\t{}\t{}\tf{}\tch{}\tb{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.name,
            self.stage,
            self.material,
            self.frame,
            self.channel
                .map(|c| c.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            self.band,
            self.arm,
            self.operands,
            self.equation,
            self.threshold,
            self.value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            self.decision,
            self.equality,
            self.evidence_class,
            self.probe,
            self.source,
        )
    }
}

fn push_margin_with_class(
    rows: &mut Vec<ExactMargin>,
    name: &'static str,
    stage: &'static str,
    material: &str,
    frame: u32,
    channel: impl Into<Option<usize>>,
    band: usize,
    arm: &'static str,
    operands: String,
    equation: &'static str,
    threshold: &'static str,
    value: impl Into<Option<i64>>,
    decision: &'static str,
    equality: &'static str,
    source: &'static str,
    evidence_class: &'static str,
    probe: &'static str,
) {
    let channel = channel.into();
    let value = value.into();
    let ordinal = if stage == "is" {
        rows.iter()
            .filter(|r| {
                r.stage == "is"
                    && r.material == material
                    && r.frame == frame
                    && r.band == band
                    && r.arm == arm
            })
            .count()
            + if name == "is.i3_local_extension" {
                3
            } else {
                0
            }
    } else if name == "s4.step3_pe_gate" || name == "s4.step3_sum" {
        rows.iter()
            .filter(|r| {
                (r.name == "s4.step3_pe_gate" || r.name == "s4.step3_sum")
                    && r.material == material
                    && r.frame == frame
                    && r.channel == channel
                    && r.arm == arm
            })
            .count()
    } else if matches!(
        name,
        "s4.step1_pe_gate"
            | "s4.step1_dmax"
            | "s4.step1_terminal"
            | "s4.step2_gate"
            | "s4.step2_min_scf"
            | "s4.step2_pe_gate"
            | "s4.step2_band"
            | "s4.step2_sum"
            | "s4.final_clamp"
            | "s4.all_dead"
            | "s4.gain_inversion"
    ) {
        rows.iter()
            .filter(|r| {
                r.name == name
                    && r.material == material
                    && r.frame == frame
                    && r.channel == channel
                    && r.band == band
                    && r.arm == arm
            })
            .count()
    } else if stage == "s4" {
        rows.iter()
            .filter(|r| {
                r.name == "s4.step3_dist"
                    && r.material == material
                    && r.frame == frame
                    && r.channel == channel
                    && r.band == band
                    && r.arm == arm
            })
            .count()
    } else if stage == "s6" && name == "s6.crash_step" {
        rows.iter()
            .filter(|r| {
                r.name == name
                    && r.material == material
                    && r.frame == frame
                    && r.channel == channel
                    && r.band == band
                    && r.arm == arm
            })
            .count()
    } else {
        0
    };
    let occurrence = if stage == "is" {
        if matches!(
            evidence_class,
            "Unavailable" | "Inapplicable" | "Unobserved" | "NotGenerated"
        ) {
            format!("placeholder{ordinal:04}")
        } else {
            format!("event{ordinal:04}")
        }
    } else if name == "s4.step3_pe_gate" || name == "s4.step3_sum" {
        format!("r{ordinal:04}")
    } else if matches!(
        name,
        "s4.step1_pe_gate"
            | "s4.step1_dmax"
            | "s4.step1_terminal"
            | "s4.step2_gate"
            | "s4.step2_min_scf"
            | "s4.step2_pe_gate"
            | "s4.step2_band"
            | "s4.step2_sum"
            | "s4.final_clamp"
            | "s4.all_dead"
            | "s4.gain_inversion"
    ) {
        let prefix = match name {
            "s4.step1_pe_gate" => "s1p",
            "s4.step1_dmax" => "s1d",
            "s4.step1_terminal" => "s1t",
            "s4.step2_gate" => "s2g",
            "s4.step2_min_scf" => "s2m",
            "s4.step2_pe_gate" => "s2p",
            "s4.step2_band" => "s2b",
            "s4.step2_sum" => "s2s",
            "s4.final_clamp" => "s4c",
            "s4.all_dead" => "s4a",
            "s4.gain_inversion" => "s4g",
            _ => unreachable!(),
        };
        format!("{prefix}{ordinal:04}")
    } else if stage == "s4" {
        format!("r{ordinal:04}k{:04}", ordinal % 3)
    } else if stage == "s6" && name == "s6.crash_step" {
        format!("step{ordinal:04}")
    } else {
        "summary".to_string()
    };
    let actual_channel = if stage == "is" { None } else { channel };
    let actual_value = match evidence_class {
        "Derived" | "Observed" => {
            Some(value.expect("Derived/Observed margin requires an exact value"))
        }
        "Unavailable" | "Inapplicable" | "Unobserved" | "NotGenerated" => None,
        other => panic!("unknown evidence class {other}"),
    };
    rows.push(ExactMargin {
        name,
        stage,
        material: material.to_string(),
        frame,
        channel: actual_channel,
        band,
        arm,
        operands,
        equation,
        threshold,
        value: actual_value,
        decision,
        equality,
        evidence_class,
        probe,
        occurrence,
        source,
    });
}

fn s2_derived_values(
    cap: &R22S2Cap,
    ch: usize,
    band: usize,
) -> (usize, i32, i32, i32, i32, i32, i32, bool, i32) {
    let c = &cap.ch[ch];
    let short = c.seq == 2;
    let thr_e = cap.thr_exp[ch][band];
    let (group_number, red, scaled, reduced) = if short {
        // Production's inner loop index is `sfb`, i.e. s % per. This is
        // intentionally *not* s / per. The resulting group_number is 0 for
        // every band with the current SHORT geometry (max == per == 8), a
        // production quirk retained rather than normalized away.
        let group_number = (band % c.per) / c.per;
        let gl = c.group_len[group_number] as usize;
        let red = cap.red_val[group_number];
        let scaled = fmult(thr_e, fmult(SHORT_THR_FAC, INV_SQRT4[gl])).wrapping_shl(2);
        let mut reduced = if scaled <= LIMIT.wrapping_sub(red) {
            i32::MIN
        } else if red >= ONE.wrapping_sub(scaled) {
            ZERO
        } else {
            calc_ld_data(scaled.wrapping_add(red)).wrapping_shl(2)
        };
        reduced = reduced.wrapping_add(calc_ld_int(gl as i32).wrapping_sub(AVG_COMP));
        (group_number, red, scaled, reduced)
    } else {
        let red = cap.red_val[0];
        let reduced = if red >= ONE.wrapping_sub(thr_e) {
            ZERO
        } else {
            calc_ld_data(thr_e.wrapping_add(red)).wrapping_shl(2)
        };
        (0, red, 0, reduced)
    };
    let weighted = c.weighted[band];
    let m = cap.min_snr_ld[ch][band];
    let admit = c.threshold[band] >= MIN_LDTHRESH && weighted > c.threshold[band];
    let ah_active = admit && reduced.wrapping_sub(weighted) > m && cap.ah_flag[ch][band] != 0;
    let button = ah_active && m > i32::MIN.wrapping_sub(weighted);
    (
        group_number,
        red,
        scaled,
        reduced,
        admit as i32,
        ah_active as i32,
        button as i32,
        reduced < NEG_HALF as i32,
        reduced.max(MIN_LDTHRESH),
    )
}

fn print_is_rows(
    rows: &mut Vec<ExactMargin>,
    material: &'static str,
    frame: u32,
    pair: &is_ms::FramePair,
) {
    print_is_margin_frame_into(rows, material, frame, pair);
}

pub(super) fn print_is_margin_frame(material: &'static str, frame: u32, pair: &is_ms::FramePair) {
    let mut rows = Vec::new();
    print_is_rows(&mut rows, material, frame, pair);
    println!("R21B-AC9-IS-FRAME\t{material}\tf{frame}");
    for row in &rows {
        println!("{}", row.render());
    }
}

pub(super) fn collect_is_margin_frame(
    material: &'static str,
    frame: u32,
    pair: &is_ms::FramePair,
) -> Vec<ExactMargin> {
    let mut rows = Vec::new();
    print_is_rows(&mut rows, material, frame, pair);
    rows
}

fn print_is_margin_frame_into(
    rows: &mut Vec<ExactMargin>,
    material: &'static str,
    frame: u32,
    pair: &is_ms::FramePair,
) {
    for (arm, op, tag) in [
        ("Q31", &pair.q_op, EngineTag::Q31),
        ("FPE", &pair.f_op, EngineTag::Fpe),
    ] {
        let stage = is_ms::replay_prepare(op, tag);
        is_ms::replay_finalize(&stage, op, tag);
        is_probe::clear();
        let replay = is_ms::replay_intensity(op, tag);
        let i9_records = is_probe::take();
        let i9_by_band: BTreeMap<usize, &is_probe::ResidualRec> =
            i9_records.iter().map(|r| (r.band, r)).collect();
        is_ms::validate_lockstep_replay(pair);
        is_ms::validate_ms_replay(pair);
        assert!(replay.is_book.len() == op.cnt && replay.is_scale.len() == op.cnt);
        let events =
            crate::aac_enc::fpe::tests::is_ms::build_trace_for_ac9(frame, tag, op, &stage, &replay);
        let direction = 0x4000_0000i32 >> 5;
        let total = 0x051e_b850i32 >> 6;
        let local = 0x0147_ae14i32 >> 3;
        for event in &events {
            if matches!(
                event,
                is_ms::TraceEvent::Extend { .. } | is_ms::TraceEvent::Reject { .. }
            ) {
                if let is_ms::TraceEvent::Extend { band, i3, .. }
                | is_ms::TraceEvent::Reject { band, i3, .. } = event
                {
                    push_margin_with_class(
                        rows,
                        "is.i3_local_extension",
                        "is",
                        material,
                        frame,
                        0,
                        *band,
                        arm,
                        format!(
                            "band={band};hrr_err={};local_error={local};i3={i3}",
                            stage.hrr_err[*band]
                        ),
                        "local_error-hrr_err",
                        "0 (strict <)",
                        *i3,
                        ">0 favors extension (with I4)",
                        "==0 rejects extension",
                        "intensity.rs:337 finalize_decisions",
                        "Derived",
                        "R21B-IS-LOCKSTEP",
                    );
                }
            }
            let (name, band, operands, equation, value, decision, equality, source) = match event {
                is_ms::TraceEvent::Extend { band, i3, i4 } | is_ms::TraceEvent::Reject { band, i3, i4 } => (
                    "is.i4_cumulative_extension", *band,
                    format!("band={band};overall_after margin i4={i4};local_i3={i3};total_error={total}"),
                    "total_error-overall_after", *i4, ">0 favors extension (with I3)", "==0 rejects extension", "intensity.rs:337 finalize_decisions"),
                is_ms::TraceEvent::DirectionDrop { band, i5, last, .. } => (
                    "is.i5_direction", *band,
                    format!("band={band};last={last};real_scale={};direction_thresh={direction}",stage.real_scale[*band]),
                    "direction_thresh-abs(last-real_scale)", *i5, ">0 retains direction", "==0 drops", "intensity.rs:346 finalize_decisions"),
                is_ms::TraceEvent::CarryUpdate { band, i5, .. } => (
                    "is.i5_direction", *band,
                    format!("band={band};real_scale={};direction_thresh={direction}",stage.real_scale[*band]),
                    "direction_thresh-abs(last-real_scale)", *i5, ">0 retains direction", "==0 drops", "intensity.rs:346 finalize_decisions"),
                is_ms::TraceEvent::BlockClear { start, end, i6, i7, .. } => {
                    push_margin_with_class(
                        rows,
                        "is.i6_count",
                        "is",
                        material,
                        frame,
                        0,
                        *end,
                        arm,
                        format!("start={start};end={end};count_margin_i6={i6};MIN_IS_SFBS=6"),
                        "count-6 (trace state)",
                        "0 (inclusive >=)",
                        *i6,
                        ">=0 satisfies count",
                        "==0 satisfies",
                        "intensity.rs:356 finalize_decisions",
                        "Derived",
                        "R21B-IS-LOCKSTEP",
                    );
                    (
                        "is.i7_region",
                        *end,
                        format!("start={start};end={end};i6={i6};i7={i7}"),
                        "region-region_min (trace state)",
                        *i7,
                        ">=0 satisfies loudness",
                        "==0 satisfies",
                        "intensity.rs:356 finalize_decisions",
                    )
                }
                _ => continue,
            };
            push_margin_with_class(
                rows,
                name,
                "is",
                material,
                frame,
                0,
                band,
                arm,
                operands,
                equation,
                "0 (see semantics)",
                value,
                decision,
                equality,
                source,
                "Derived",
                "R21B-IS-LOCKSTEP",
            );
        }
        for b in 0..op.cnt {
            let start = op.offsets[b] as usize;
            let end = op.offsets[b + 1] as usize;
            let corr = sfb_correlation(&op.spec_l, &op.spec_r, start, end);
            let raw = op.e_ld_l[b].wrapping_sub(op.e_ld_r[b]);
            let m_a = op.e_l[b] as i64 - fmult(0x5999_9980, op.e_r[b]) as i64;
            let m_b = fmult(0x5999_9980, op.e_l[b]) as i64 - op.e_r[b] as i64;
            let _ = (local, direction);
            let mut stateful_rows = Vec::new();
            let mut stateful_margin = |name: &'static str, reason: &'static str| {
                push_margin_with_class(
                    &mut stateful_rows,
                    name,
                    "is",
                    material,
                    frame,
                    0,
                    b,
                    arm,
                    format!(
                        "pre_mask={};status=Unavailable;reason={reason}",
                        stage.is_mask_pre[b]
                    ),
                    "stateful operand not retained per band",
                    "n/a",
                    0,
                    "Unavailable: see exact trace-event rows where the branch executed",
                    "n/a",
                    "intensity.rs:337-356 finalize_decisions",
                    "Unavailable",
                    "R21B-IS-LOCKSTEP",
                );
            };
            push_margin_with_class(
                rows,
                "is.i1_correlation",
                "is",
                material,
                frame,
                0,
                b,
                arm,
                format!("corr={corr};CORR_THRESH={}", is_ms::CORR_THRESH),
                "abs(corr)-CORR_THRESH",
                "0 (inclusive >=)",
                (corr as i64).abs() - is_ms::CORR_THRESH as i64,
                ">=0 sets the correlation mask",
                "==0 admits",
                "intensity.rs:279 prepare_decisions",
                "Derived",
                "R21B-IS-LOCKSTEP",
            );
            push_margin_with_class(
                rows,
                "is.i2_sentinel_a",
                "is",
                material,
                frame,
                0,
                b,
                arm,
                format!("e_l={};e_r={};R={}", op.e_l[b], op.e_r[b], 0x5999_9980),
                "e_l-fmult(0.7,e_r)",
                "0 (inclusive >=)",
                m_a,
                ">=0 satisfies the first anti-centre conjunct",
                "==0 satisfies",
                "intensity.rs:227 prepare_decisions",
                "Derived",
                "R21B-IS-LOCKSTEP",
            );
            push_margin_with_class(
                rows,
                "is.i2_sentinel_b",
                "is",
                material,
                frame,
                0,
                b,
                arm,
                format!("e_l={};e_r={};R={}", op.e_l[b], op.e_r[b], 0x5999_9980),
                "fmult(0.7,e_l)-e_r",
                "0 (inclusive <=)",
                m_b,
                "<=0 satisfies the second anti-centre conjunct",
                "==0 satisfies",
                "intensity.rs:228 prepare_decisions",
                "Derived",
                "R21B-IS-LOCKSTEP",
            );
            stateful_margin(
                "is.i3_local_extension",
                "local I3 is exact only on an executed extension/rejection trace event",
            );
            drop(stateful_margin);
            rows.append(&mut stateful_rows);
            let mut stateful_rows = Vec::new();
            let mut stateful_margin = |name: &'static str, reason: &'static str| {
                push_margin_with_class(
                    &mut stateful_rows,
                    name,
                    "is",
                    material,
                    frame,
                    0,
                    b,
                    arm,
                    format!(
                        "pre_mask={};status=Unavailable;reason={reason}",
                        stage.is_mask_pre[b]
                    ),
                    "stateful operand not retained per band",
                    "n/a",
                    0,
                    "Unavailable: see exact trace-event rows where the branch executed",
                    "n/a",
                    "intensity.rs:337-356 finalize_decisions",
                    "Unavailable",
                    "R21B-IS-LOCKSTEP",
                );
            };
            stateful_margin(
                "is.i5_direction",
                "carried last-scale is exact only on a direction/carry trace event",
            );
            stateful_margin(
                "is.i6_count",
                "region count is exact only at a block-clear trace event",
            );
            stateful_margin(
                "is.i7_region",
                "region loudness is exact only at a block-clear trace event",
            );
            rows.append(&mut stateful_rows);
            let i8a = op.e_l[b] as i64 - op.t_l[b] as i64;
            let i8b = fmult(0x5555_5580, op.e_r[b]) as i64 - op.t_r[b] as i64;
            push_margin_with_class(
                rows,
                "is.i8_threshold_a",
                "is",
                material,
                frame,
                0,
                b,
                arm,
                format!("e_l={};thr_l={}", op.e_l[b], op.t_l[b]),
                "e_l-thr_l",
                "0 (strict <)",
                i8a,
                "<0 is the first skip conjunct",
                "==0 does not skip via this conjunct",
                "intensity.rs:457 intensity_stereo",
                "Derived",
                "R21B-IS-LOCKSTEP",
            );
            push_margin_with_class(
                rows,
                "is.i8_threshold_b",
                "is",
                material,
                frame,
                0,
                b,
                arm,
                format!(
                    "e_r={};thr_r={};recip={}",
                    op.e_r[b], op.t_r[b], 0x5555_5580
                ),
                "fmult(2/3,e_r)-thr_r",
                "0 (strict >)",
                i8b,
                ">0 is the second skip conjunct",
                "==0 does not skip via this conjunct",
                "intensity.rs:457 intensity_stereo",
                "Derived",
                "R21B-IS-LOCKSTEP",
            );
            let visited = (b % op.per.max(1)) < op.max;
            match i9_by_band.get(&b) {
                Some(rec) if rec.lr.is_some() => {
                    let lr = rec.lr.expect("checked");
                    // Independent replay of the production residual
                    // arithmetic on the captured operands. This is evidence
                    // verification, not a second canonicalization/replay
                    // implementation.
                    let mut replay_lr = 0i32;
                    for j in rec.start..rec.end {
                        replay_lr = replay_lr.wrapping_add(fmult_div2(
                            fmult_div2(
                                shift_int(op.spec_l[j], rec.s_l),
                                shift_int(op.spec_r[j], rec.s_r),
                            ),
                            rec.inv_n,
                        ));
                    }
                    replay_lr = replay_lr.wrapping_shl(1);
                    assert_eq!(
                        replay_lr, lr,
                        "I9 lr replay {} f{} b{} {arm}",
                        material, frame, b
                    );
                    let s0 = rec.s_l.min(rec.s_r);
                    if let Some(ed) = rec.ed {
                        let mut replay_ed = 0i32;
                        for j in rec.start..rec.end {
                            let d = (shift_int(op.spec_l[j], s0) >> 1)
                                .wrapping_sub(shift_int(op.spec_r[j], s0) >> 1);
                            replay_ed = replay_ed
                                .wrapping_add(fmult_div2(d, d) >> (rec.mdct_spec_sf - 1));
                        }
                        assert_eq!(
                            replay_ed, ed,
                            "I9 ed replay {} f{} b{} {arm}",
                            material, frame, b
                        );
                    }
                    if let Some(es) = rec.es {
                        let mut replay_es = 0i32;
                        for j in rec.start..rec.end {
                            let s = (shift_int(op.spec_l[j], s0) >> 1)
                                .wrapping_add(shift_int(op.spec_r[j], s0) >> 1);
                            replay_es = replay_es
                                .wrapping_add(fmult_div2(s, s) >> (rec.mdct_spec_sf - 1));
                        }
                        assert_eq!(
                            replay_es, es,
                            "I9 es replay {} f{} b{} {arm}",
                            material, frame, b
                        );
                    }
                    let (residual, equation, operand_name) = if let Some(ed) = rec.ed {
                        (ed, "ed-0", "ed")
                    } else {
                        (rec.es.expect("es or ed"), "es-0", "es")
                    };
                    push_margin_with_class(
                        rows,
                        "is.i9_phase",
                        "is",
                        material,
                        frame,
                        0,
                        b,
                        arm,
                        format!(
                            "band={b};start={};end={};s_l={};s_r={};inv_n={};mdct_spec_sf={};lr={lr};branch={};book={};scale={}",
                            rec.start,
                            rec.end,
                            rec.s_l,
                            rec.s_r,
                            rec.inv_n,
                            rec.mdct_spec_sf,
                            rec.branch,
                            replay.is_book[b],
                            replay.is_scale[b]
                        ),
                        "lr (signed accumulator)",
                        "0 (strict < selects out-of-phase)",
                        lr as i64,
                        "<0 selects out-of-phase (ed); >=0 selects in-phase (es)",
                        "==0 selects in-phase",
                        "intensity.rs:479-488 intensity_stereo",
                        "Derived",
                        "R21B-IS-LOCKSTEP",
                    );
                    push_margin_with_class(
                        rows,
                        "is.i9_residual",
                        "is",
                        material,
                        frame,
                        0,
                        b,
                        arm,
                        format!(
                            "band={b};start={};end={};s_l={};s_r={};inv_n={};mdct_spec_sf={};branch={};selected_residual={operand_name};{operand_name}={residual};book={};scale={};selected={}",
                            rec.start,
                            rec.end,
                            rec.s_l,
                            rec.s_r,
                            rec.inv_n,
                            rec.mdct_spec_sf,
                            rec.branch,
                            replay.is_book[b],
                            replay.is_scale[b],
                            rec.selected
                        ),
                        equation,
                        "0 (strict > required)",
                        residual as i64,
                        ">0 admits the selected intensity residual; <=0 drops the band",
                        "==0 drops",
                        "intensity.rs:491-537 intensity_stereo",
                        "Derived",
                        "R21B-IS-LOCKSTEP",
                    );
                }
                Some(rec) => {
                    push_margin_with_class(
                        rows,
                        "is.i9_residual",
                        "is",
                        material,
                        frame,
                        0,
                        b,
                        arm,
                        format!("band={b};status=Inapplicable;reason={}", rec.skip),
                        "I9 not reached",
                        "n/a",
                        0,
                        "Inapplicable: I9 residual was not generated",
                        "n/a",
                        "intensity.rs:479-537 intensity_stereo",
                        "Inapplicable",
                        "R21B-IS-LOCKSTEP",
                    );
                }
                None => {
                    let (class, reason) = if visited {
                        ("Unavailable", "I9 probe did not record a visited band")
                    } else {
                        ("Inapplicable", "band outside the grouped topology")
                    };
                    push_margin_with_class(
                        rows,
                        "is.i9_residual",
                        "is",
                        material,
                        frame,
                        0,
                        b,
                        arm,
                        format!("band={b};status={class};reason={reason}"),
                        "I9 residual not generated",
                        "n/a",
                        0,
                        "no exact residual claimed",
                        "n/a",
                        "intensity.rs:479-537 intensity_stereo",
                        class,
                        "R21B-IS-LOCKSTEP",
                    );
                }
            }
            push_margin_with_class(
                rows,
                "is.i10_scale_max",
                "is",
                material,
                frame,
                0,
                b,
                arm,
                format!("raw={};REAL_SCALE_MAX={}", raw, 0x3c00_0000),
                "REAL_SCALE_MAX-raw",
                "0 (clamp when <0)",
                0x3c00_0000i64 - raw as i64,
                "<0 clamps to +60/128",
                "==0 reads the rail exactly",
                "intensity.rs:218-219 prepare_decisions",
                "Derived",
                "R21B-IS-LOCKSTEP",
            );
            push_margin_with_class(
                rows,
                "is.i10_scale_min",
                "is",
                material,
                frame,
                0,
                b,
                arm,
                format!("raw={};REAL_SCALE_MIN={}", raw, -0x3c00_0000),
                "raw-REAL_SCALE_MIN",
                "0 (clamp when <0)",
                raw as i64 + 0x3c00_0000i64,
                "<0 clamps to -60/128",
                "==0 reads the rail exactly",
                "intensity.rs:218-219 prepare_decisions",
                "Derived",
                "R21B-IS-LOCKSTEP",
            );
        }
    }
}

fn collect_s1_s2(
    caps: &BTreeMap<(usize, u32), (FrameSnapshot, FrameSnapshot)>,
    materials: &[is_ms::Material],
) -> Vec<ExactMargin> {
    let mut rows = Vec::new();
    for ((mi, frame), (q_snap, f_snap)) in caps {
        let material = materials[*mi].name;
        for (arm, snap) in [("Q31", q_snap), ("FPE", f_snap)] {
            let cap = snap.r22_cap.as_ref().expect("R22 capture");
            for (ch, c) in cap.s1.ch.iter().enumerate() {
                let bands = trace::active_bands_for_ac9(c.cnt, c.per, c.max);
                for &b in &bands {
                    let e = c.energy_ld[b];
                    let t = c.threshold_ld[b];
                    let w = cap.s1.weighted[ch][b];
                    let pt = cap.s1.thresholds[ch][b];
                    let el = c.energy_lin[b];
                    push_margin_with_class(
                        &mut rows,
                        "pe.prepare_admission",
                        "pe",
                        material,
                        *frame,
                        ch,
                        b,
                        arm,
                        format!("energy_ld={e};threshold_ld={t}"),
                        "energy_ld-threshold_ld",
                        "0 (strict >)",
                        pe_prepare_admission_margin(e, t),
                        ">0 admits a non-zero line estimate",
                        "==0 does not admit",
                        "pe.rs:353 prepare_sfb_pe",
                        "Derived",
                        "R22-S1-CAPTURE",
                    );
                    push_margin_with_class(
                        &mut rows,
                        "pe.calc_admission",
                        "pe",
                        material,
                        *frame,
                        ch,
                        b,
                        arm,
                        format!("weighted={w};threshold={pt}"),
                        "weighted-threshold",
                        "0 (strict >)",
                        pe_calc_admission_margin(w, pt),
                        ">0 contributes per-band PE",
                        "==0 contributes zero",
                        "pe.rs:406 calc_sfb_pe",
                        "Derived",
                        "R22-S1-CAPTURE",
                    );
                    if w > pt {
                        push_margin_with_class(
                            &mut rows,
                            "pe.c1_branch",
                            "pe",
                            material,
                            *frame,
                            ch,
                            b,
                            arm,
                            format!("weighted={w};threshold={pt};C1={}", 0x0600_0000),
                            "weighted-threshold-C1",
                            "0 (inclusive >=)",
                            pe_c1_branch_margin(w, pt),
                            ">=0 selects the linear branch",
                            "==0 selects linear",
                            "pe.rs:411 calc_sfb_pe",
                            "Derived",
                            "R22-S1-CAPTURE",
                        );
                    }
                    push_margin_with_class(
                        &mut rows,
                        "pe.enfac_energy_gate",
                        "pe",
                        material,
                        *frame,
                        ch,
                        b,
                        arm,
                        format!("energy_linear={el}"),
                        "energy_linear-0",
                        "0 (strict >)",
                        enfac_energy_gate_margin(el),
                        ">0 allows the patch write",
                        "==0 blocks the write",
                        "pe.rs:531 calc_weighting",
                        "Derived",
                        "R22-S1-CAPTURE",
                    );
                }
                let lines_sum: i32 = bands
                    .iter()
                    .map(|&b| cap.s1.pe_ch[ch].sfb_n_lines[b])
                    .fold(0i32, |a, b| a.wrapping_add(b));
                let total_lines = c.offsets[c.cnt];
                let chaos = cap.s1.ats_post.chaos_measure_en_fac[ch];
                if !c.short {
                    push_margin_with_class(
                        &mut rows,
                        "pe.chaos_floor",
                        "pe",
                        material,
                        *frame,
                        ch,
                        0,
                        arm,
                        format!("lines_sum={lines_sum};total_lines={total_lines}"),
                        "fdiv_norm(lines_sum,total_lines)-402653184",
                        "0 (max floor)",
                        chaos_floor_margin(lines_sum, total_lines),
                        "<0 clamps to the 0.1875 floor",
                        "==0 reads the floor exactly",
                        "pe.rs:515 calc_weighting",
                        "Derived",
                        "R22-S1-CAPTURE",
                    );
                }
                push_margin_with_class(
                    &mut rows,
                    "pe.chaos_patch_arm",
                    "pe",
                    material,
                    *frame,
                    ch,
                    0,
                    arm,
                    format!("chaos={chaos}"),
                    "chaos-1677721600",
                    "0 (strict >)",
                    chaos_patch_arm_margin(chaos),
                    ">0 arms the patch",
                    "==0 does not arm",
                    "pe.rs:516 calc_weighting",
                    "Derived",
                    "R22-S1-CAPTURE",
                );
                push_margin_with_class(
                    &mut rows,
                    "pe.chaos_patch_hi",
                    "pe",
                    material,
                    *frame,
                    ch,
                    0,
                    arm,
                    format!("chaos={chaos}"),
                    "chaos-1744830464",
                    "0 (strict >)",
                    chaos_patch_hi_margin(chaos),
                    ">0 selects the 3/4-energy exponent",
                    "==0 does not select",
                    "pe.rs:532 calc_weighting",
                    "Derived",
                    "R22-S1-CAPTURE",
                );
                push_margin_with_class(
                    &mut rows,
                    "pe.chaos_patch_mid",
                    "pe",
                    material,
                    *frame,
                    ch,
                    0,
                    arm,
                    format!("chaos={chaos}"),
                    "chaos-1711276032",
                    "0 (strict >)",
                    chaos_patch_mid_margin(chaos),
                    ">0 selects the 2/4-energy exponent",
                    "==0 does not select",
                    "pe.rs:537 calc_weighting",
                    "Derived",
                    "R22-S1-CAPTURE",
                );
                if c.short {
                    push_margin_with_class(
                        &mut rows,
                        "pe.patch_execution",
                        "pe",
                        material,
                        *frame,
                        ch,
                        0,
                        arm,
                        format!(
                            "short=true;chaos_post={};prior_patch={}",
                            cap.s1.ats_post.chaos_measure_en_fac[ch],
                            cap.s1.ats_pre.last_enfac_patch[ch]
                        ),
                        "short-element early path",
                        "n/a",
                        0,
                        "Inapplicable: short elements arm the patch without per-band execution",
                        "n/a",
                        "pe.rs:486-489 calc_weighting",
                        "Inapplicable",
                        "R22-S1-CAPTURE",
                    );
                } else {
                    let use_patch = chaos > 0x6400_0000;
                    let prior_patch = cap.s1.ats_pre.last_enfac_patch[ch] != 0;
                    let exe_patch = use_patch && prior_patch;
                    push_margin_with_class(
                        &mut rows,
                        "pe.patch_execution",
                        "pe",
                        material,
                        *frame,
                        ch,
                        0,
                        arm,
                        format!(
                            "chaos={chaos};PATCH_ARM={};use_patch={use_patch};prior_patch={prior_patch};exe_patch={exe_patch}",
                            0x6400_0000
                        ),
                        "use_patch && prior_patch != 0",
                        "1 (execute patch)",
                        i64::from(exe_patch),
                        "1 executes the flatness patch on this channel",
                        "0 does not execute",
                        "pe.rs:516-521 calc_weighting",
                        "Observed",
                        "R22-S1-CAPTURE",
                    );
                }
            }
            let exe_patch_by_ch: Vec<bool> = cap
                .s1
                .ch
                .iter()
                .enumerate()
                .map(|(ch, c)| {
                    if c.short {
                        false
                    } else {
                        cap.s1.ats_post.chaos_measure_en_fac[ch] > 0x6400_0000
                            && cap.s1.ats_pre.last_enfac_patch[ch] != 0
                    }
                })
                .collect();
            for (ch, c) in cap.s1.ch.iter().enumerate() {
                if c.short {
                    continue;
                }
                let bands = trace::active_bands_for_ac9(c.cnt, c.per, c.max);
                for &b in &bands {
                    let ms = cap.s1.ms_mask.get(b).copied().unwrap_or(0);
                    let do_patch = if ch == 1 && ms != 0 {
                        exe_patch_by_ch[0]
                    } else {
                        exe_patch_by_ch[ch]
                    };
                    if ch == 1 && ms != 0 {
                        push_margin_with_class(
                            &mut rows,
                            "pe.patch_ms_inheritance",
                            "pe",
                            material,
                            *frame,
                            ch,
                            b,
                            arm,
                            format!(
                                "ms_mask={ms};exe_patch_m={};own_exe_patch={};do_patch={do_patch}",
                                exe_patch_by_ch[0], exe_patch_by_ch[ch]
                            ),
                            "MS-coupled band follows channel 0",
                            "1 (inherited)",
                            i64::from(do_patch),
                            "1 uses channel 0's patch decision on this MS band",
                            "0 does not",
                            "pe.rs:526-530 calc_weighting",
                            "Observed",
                            "R22-S1-CAPTURE",
                        );
                    }
                    if do_patch {
                        push_margin_with_class(
                            &mut rows,
                            "pe.patch_write_gate",
                            "pe",
                            material,
                            *frame,
                            ch,
                            b,
                            arm,
                            format!(
                                "energy_linear={};enfac={};ms_mask={ms}",
                                c.energy_lin[b], cap.s1.enfac[ch][b]
                            ),
                            "energy_linear-0",
                            "0 (strict >)",
                            c.energy_lin[b] as i64,
                            ">0 writes sfbEnFacLd; ==0 blocks the write",
                            "==0 blocks",
                            "pe.rs:531-542 calc_weighting",
                            "Derived",
                            "R22-S1-CAPTURE",
                        );
                    } else {
                        push_margin_with_class(
                            &mut rows,
                            "pe.patch_write_gate",
                            "pe",
                            material,
                            *frame,
                            ch,
                            b,
                            arm,
                            format!(
                                "energy_linear={};enfac={};ms_mask={ms};do_patch=false",
                                c.energy_lin[b], cap.s1.enfac[ch][b]
                            ),
                            "patch not executed",
                            "n/a",
                            0,
                            "Inapplicable: patch execution gate did not run for this band",
                            "n/a",
                            "pe.rs:531-542 calc_weighting",
                            "Inapplicable",
                            "R22-S1-CAPTURE",
                        );
                    }
                }
            }
            for (ch, c) in cap.s2.ch.iter().enumerate() {
                let bands = trace::active_bands_for_ac9(c.cnt, c.per, c.max);
                for &b in &bands {
                    let (gn, red, scaled, reduced, admit, ah_active, button, floor_hit, clamped) =
                        s2_derived_values(&cap.s2, ch, b);
                    let w = c.weighted[b];
                    let t = c.threshold[b];
                    let m = cap.s2.min_snr_ld[ch][b];
                    let ah = cap.s2.ah_flag[ch][b];
                    let spread = cap.s2.spread[ch][b];
                    let en = c.energy_lin[b];
                    let (ms, mt) = ah_classify_margins(spread, en, m);
                    push_margin_with_class(
                        &mut rows,
                        "s2.admission_min",
                        "s2",
                        material,
                        *frame,
                        ch,
                        b,
                        arm,
                        format!("threshold={t};MIN_LDTHRESH={MIN_LDTHRESH}"),
                        "threshold-MIN_LDTHRESH",
                        "0 (inclusive >=)",
                        t as i64 - MIN_LDTHRESH as i64,
                        ">=0 passes the first admission conjunct",
                        "==0 passes",
                        "threshold_vbr.rs:847 reduce_thresholds_vbr",
                        "Derived",
                        "R22-S2-CAPTURE",
                    );
                    push_margin_with_class(
                        &mut rows,
                        "s2.admission_active",
                        "s2",
                        material,
                        *frame,
                        ch,
                        b,
                        arm,
                        format!("weighted={w};threshold={t}"),
                        "weighted-threshold",
                        "0 (strict >)",
                        w as i64 - t as i64,
                        ">0 passes the second admission conjunct",
                        "==0 does not pass",
                        "threshold_vbr.rs:847 reduce_thresholds_vbr",
                        "Derived",
                        "R22-S2-CAPTURE",
                    );
                    push_margin_with_class(
                        &mut rows,
                        "s2.ah_classify_spread",
                        "s2",
                        material,
                        *frame,
                        ch,
                        b,
                        arm,
                        format!("spread={spread};energy_linear={en}"),
                        "spread-energy_linear",
                        "0 (inclusive <=)",
                        ms,
                        ">0 forces NO_AH",
                        "==0 permits AH classification",
                        "threshold_vbr.rs:419 init_avoid_hole_flag_mono",
                        "Derived",
                        "R22-S2-CAPTURE",
                    );
                    push_margin_with_class(
                        &mut rows,
                        "s2.ah_classify_minsnr",
                        "s2",
                        material,
                        *frame,
                        ch,
                        b,
                        arm,
                        format!("min_snr={m}"),
                        "min_snr-0",
                        "0 (inclusive <=)",
                        mt,
                        ">0 forces NO_AH",
                        "==0 permits AH classification",
                        "threshold_vbr.rs:419 init_avoid_hole_flag_mono",
                        "Derived",
                        "R22-S2-CAPTURE",
                    );
                    push_margin_with_class(
                        &mut rows,
                        "s2.minsnr_drop",
                        "s2",
                        material,
                        *frame,
                        ch,
                        b,
                        arm,
                        format!(
                            "avg_en_ld={};energy_ld={};msa_start_ratio={}",
                            trace::frame_avg_en_ld_for_ac9(&c.energy_lin, c.cnt, c.per, c.max),
                            c.energy_ld[b],
                            cap.s2.ats_pre.msa_start_ratio
                        ),
                        "(avg_en_ld-energy_ld)-msa_start_ratio",
                        "0 (strict >)",
                        trace::minsnr_drop_margin_for_ac9(
                            trace::frame_avg_en_ld_for_ac9(&c.energy_lin, c.cnt, c.per, c.max),
                            c.energy_ld[b],
                            cap.s2.ats_pre.msa_start_ratio,
                        ),
                        ">0 lowers band minSnr",
                        "==0 does not update",
                        "threshold_vbr.rs:308-309 adapt_min_snr",
                        "Derived",
                        "R22-S2-CAPTURE",
                    );
                    let te = cap.s2.thr_exp[ch][b];
                    if c.seq == 2 {
                        let gl = c.group_len[gn] as usize;
                        push_margin_with_class(&mut rows, "s2.short_scaled", "s2", material, *frame, ch, b, arm,
                            format!("thr_exp={te};SHORT_THR_FAC={SHORT_THR_FAC};INV_SQRT4[gl={gl}]={};gl={gl}",INV_SQRT4[gl]),
                            "fmult(thr_exp,fmult(SHORT_THR_FAC,INV_SQRT4[gl]))<<2", "n/a (derived intermediate)", scaled as i64,
                            "scaled feeds NEG_ONE/ZERO and reduced threshold", "wrapping fmult and shift", "threshold_vbr.rs:850-853 reduce_thresholds_vbr", "Derived", "R22-S2-CAPTURE");
                        push_margin_with_class(
                            &mut rows,
                            "s2.short_negone",
                            "s2",
                            material,
                            *frame,
                            ch,
                            b,
                            arm,
                            format!("scaled={scaled};red={red};LIMIT={LIMIT}"),
                            "scaled-(LIMIT-red)",
                            "0 (inclusive <=)",
                            scaled as i64 - (LIMIT as i64 - red as i64),
                            "<=0 selects NEG_ONE",
                            "==0 selects NEG_ONE",
                            "threshold_vbr.rs:854 reduce_thresholds_vbr",
                            "Derived",
                            "R22-S2-CAPTURE",
                        );
                        push_margin_with_class(
                            &mut rows,
                            "s2.short_zero",
                            "s2",
                            material,
                            *frame,
                            ch,
                            b,
                            arm,
                            format!("red={red};scaled={scaled};ONE={ONE}"),
                            "red-(ONE-scaled)",
                            "0 (inclusive >=)",
                            red as i64 - (ONE as i64 - scaled as i64),
                            ">=0 selects ZERO",
                            "==0 selects ZERO",
                            "threshold_vbr.rs:856 reduce_thresholds_vbr",
                            "Derived",
                            "R22-S2-CAPTURE",
                        );
                    } else {
                        push_margin_with_class(
                            &mut rows,
                            "s2.long_zero",
                            "s2",
                            material,
                            *frame,
                            ch,
                            b,
                            arm,
                            format!("red={red};thr_exp={te};ONE={ONE}"),
                            "red-(ONE-thr_exp)",
                            "0 (inclusive >=)",
                            red as i64 - (ONE as i64 - te as i64),
                            ">=0 selects ZERO",
                            "==0 selects ZERO",
                            "threshold_vbr.rs:865 reduce_thresholds_vbr",
                            "Derived",
                            "R22-S2-CAPTURE",
                        );
                    }
                    push_margin_with_class(
                        &mut rows,
                        "s2.ah_clamp",
                        "s2",
                        material,
                        *frame,
                        ch,
                        b,
                        arm,
                        format!(
                            "reduced={reduced};weighted={w};min_snr={m};ah_flag={ah};admit={admit}"
                        ),
                        "(reduced-weighted)-min_snr",
                        "0 (strict >, classified/admitted only)",
                        ah_clamp_margin(reduced, w, m),
                        ">0 with admission and ah!=NO_AH engages AH_ACTIVE",
                        "==0 does not clamp",
                        "threshold_vbr.rs:870 reduce_thresholds_vbr",
                        "Derived",
                        "R22-S2-CAPTURE",
                    );
                    push_margin_with_class(&mut rows,"s2.ah_clamp_short_applicable","s2",material,*frame,ch,b,arm,
                        format!("short={};group_number={gn};admit={admit};ah_flag={ah};ah_active={ah_active}",c.seq==2),
                        "short && admit && ah!=NO_AH", "1 (applicability)", (c.seq == 2 && admit != 0 && ah != 0) as i64,
                        "1 means the SHORT clamp margin is decision-applicable", "0 is inapplicable, not a zero margin", "threshold_vbr.rs:849-870 applicability", "Derived", "R22-S2-CAPTURE");
                    if c.seq == 2 && admit != 0 && ah != 0 {
                        push_margin_with_class(&mut rows, "s2.ah_clamp_short", "s2", material, *frame, ch, b, arm,
                            format!("reduced={reduced};weighted={w};min_snr={m};ah_flag={ah};short=true;group_number={gn};admit={admit};ah_active={ah_active}"),
                            "(reduced-weighted)-min_snr", "0 (strict >)", ah_clamp_margin(reduced,w,m),
                            ">0 engages the SHORT avoid-hole clamp", "==0 does not clamp", "threshold_vbr.rs:850-870; MDEF v2 Required (D-R2)", "Derived", "R22-S2-CAPTURE");
                    }
                    push_margin_with_class(
                        &mut rows,
                        "s2.ah_button",
                        "s2",
                        material,
                        *frame,
                        ch,
                        b,
                        arm,
                        format!("min_snr={m};weighted={w};NEG_ONE={}", i32::MIN),
                        "min_snr-(NEG_ONE-weighted)",
                        "0 (strict >, active clamp only)",
                        m as i64 - (i32::MIN as i64 - w as i64),
                        ">0 takes max(min_snr+weighted,threshold)",
                        "==0 takes threshold only",
                        "threshold_vbr.rs:871 reduce_thresholds_vbr",
                        "Derived",
                        "R22-S2-CAPTURE",
                    );
                    push_margin_with_class(&mut rows, "s2.floor", "s2", material, *frame, ch, b, arm,
                        format!("reduced={reduced};NEG_HALF={NEG_HALF};MIN_LDTHRESH={MIN_LDTHRESH};clamped={clamped}"),
                        "reduced-NEG_HALF; then max(MIN_LDTHRESH)", "0 (strict <)", reduced as i64-NEG_HALF as i64,
                        "<0 clamps reduced to NEG_ONE before floor", "==0 does not take NEG_ONE", "threshold_vbr.rs:878-881 reduce_thresholds_vbr", "Derived", "R22-S2-CAPTURE");
                    // Branch flags are retained in the operand text so the raw
                    // margin cannot be mistaken for an unconditional decision.
                    let _ = (button, floor_hit);
                }
            }
            let ah_modify = cap.s2.ats_pre.ah_modify_min_snr != 0;
            for rec in &cap.s2.peak_valley {
                if !ah_modify {
                    push_margin_with_class(
                        &mut rows,
                        "s2.ah_peak_valley",
                        "s2",
                        material,
                        *frame,
                        rec.channel,
                        rec.band,
                        arm,
                        format!(
                            "band={};channel={};ah_modify_min_snr=0;status=Inapplicable",
                            rec.band, rec.channel
                        ),
                        "avoid-hole peak/valley modification disabled",
                        "n/a",
                        0,
                        "Inapplicable: ATS did not enable minSnr modification",
                        "n/a",
                        "threshold_vbr.rs:370 init_avoid_hole_flag_mono",
                        "Inapplicable",
                        "R22-S2-AH-PROBE",
                    );
                    continue;
                }
                push_margin_with_class(
                    &mut rows,
                    "s2.ah_peak",
                    "s2",
                    material,
                    *frame,
                    rec.channel,
                    rec.band,
                    arm,
                    format!(
                        "band={};channel={};seq_long={};en={};en_m1={};en_p1={};avg_en={};avg_ld={};en_ld={};min_snr_before={};peak={};peak_floor={};after_peak={}",
                        rec.band,
                        rec.channel,
                        rec.seq_long,
                        rec.en,
                        rec.en_m1,
                        rec.en_p1,
                        rec.avg_en,
                        rec.avg_ld,
                        rec.en_ld,
                        rec.min_snr_before,
                        rec.peak,
                        rec.peak_floor,
                        rec.after_peak
                    ),
                    "en-avg_en",
                    "0 (strict >)",
                    rec.en as i64 - rec.avg_en as i64,
                    ">0 selects the local-peak minSnr floor",
                    "==0 does not select",
                    "threshold_vbr.rs:389-398 init_avoid_hole_flag_mono",
                    "Derived",
                    "R22-S2-AH-PROBE",
                );
                push_margin_with_class(
                    &mut rows,
                    "s2.ah_valley",
                    "s2",
                    material,
                    *frame,
                    rec.channel,
                    rec.band,
                    arm,
                    format!(
                        "band={};channel={};en={};en_ld={};avg_ld={};SNR_LD_MIN4=33554432;min_snr_before={};valley={};valley_tmp={};after_valley={}",
                        rec.band,
                        rec.channel,
                        rec.en,
                        rec.en_ld,
                        rec.avg_ld,
                        rec.min_snr_before,
                        rec.valley,
                        rec.valley_tmp,
                        rec.after_valley
                    ),
                    "avg_ld-(en_ld+SNR_LD_MIN4)",
                    "0 (strict >, with en>0)",
                    rec.avg_ld as i64 - (rec.en_ld as i64 + 0x0200_0000),
                    ">0 with en>0 selects the valley relax",
                    "==0 does not select",
                    "threshold_vbr.rs:399-407 init_avoid_hole_flag_mono",
                    "Derived",
                    "R22-S2-AH-PROBE",
                );
            }
            for rec in &cap.s2.ms_tweaks {
                push_margin_with_class(
                    &mut rows,
                    "s2.ms_min_snr_m",
                    "s2",
                    material,
                    *frame,
                    0,
                    rec.band,
                    arm,
                    format!(
                        "band={};ms_mask={};max_en_ld={};max_thr_ld={};min_snr_m_before={};min_snr_m_after={};energy_lin_m={}",
                        rec.band,
                        rec.ms_mask,
                        rec.max_en_ld,
                        rec.max_thr_ld,
                        rec.min_snr_m_before,
                        rec.min_snr_m_after,
                        rec.energy_lin_m
                    ),
                    "max/minSnr clamp on MS band (mid)",
                    "n/a (exact transformation)",
                    rec.min_snr_m_after as i64 - rec.min_snr_m_before as i64,
                    "post-tweak mid minSnr",
                    "n/a (exact transformation)",
                    "threshold_vbr.rs:448-463 init_avoid_hole_flag_ms",
                    "Observed",
                    "R22-S2-AH-PROBE",
                );
                push_margin_with_class(
                    &mut rows,
                    "s2.ms_min_snr_s",
                    "s2",
                    material,
                    *frame,
                    1,
                    rec.band,
                    arm,
                    format!(
                        "band={};ms_mask={};max_en_ld={};max_thr_ld={};min_snr_s_before={};min_snr_s_after={};energy_lin_s={}",
                        rec.band,
                        rec.ms_mask,
                        rec.max_en_ld,
                        rec.max_thr_ld,
                        rec.min_snr_s_before,
                        rec.min_snr_s_after,
                        rec.energy_lin_s
                    ),
                    "max/minSnr clamp on MS band (side)",
                    "n/a (exact transformation)",
                    rec.min_snr_s_after as i64 - rec.min_snr_s_before as i64,
                    "post-tweak side minSnr",
                    "n/a (exact transformation)",
                    "threshold_vbr.rs:464-472 init_avoid_hole_flag_ms",
                    "Observed",
                    "R22-S2-AH-PROBE",
                );
                push_margin_with_class(
                    &mut rows,
                    "s2.ms_spread_s",
                    "s2",
                    material,
                    *frame,
                    1,
                    rec.band,
                    arm,
                    format!(
                        "band={};ms_mask={};energy_lin_m={};spread_m_before={};spread_s_before={};spread_s_mid={};spread_s_after={}",
                        rec.band,
                        rec.ms_mask,
                        rec.energy_lin_m,
                        rec.spread_m_before,
                        rec.spread_s_before,
                        rec.spread_s_mid,
                        rec.spread_s_after
                    ),
                    "energy_lin_m-spread_m_before",
                    "0 (strict >)",
                    rec.energy_lin_m as i64 - rec.spread_m_before as i64,
                    ">0 overwrites side spread with fmult(0.9, energy_lin_s)",
                    "==0 does not overwrite",
                    "threshold_vbr.rs:473-475 init_avoid_hole_flag_ms",
                    "Derived",
                    "R22-S2-AH-PROBE",
                );
                push_margin_with_class(
                    &mut rows,
                    "s2.ms_spread_m",
                    "s2",
                    material,
                    *frame,
                    0,
                    rec.band,
                    arm,
                    format!(
                        "band={};ms_mask={};energy_lin_s={};spread_s_mid={};spread_m_before={};spread_m_after={}",
                        rec.band,
                        rec.ms_mask,
                        rec.energy_lin_s,
                        rec.spread_s_mid,
                        rec.spread_m_before,
                        rec.spread_m_after
                    ),
                    "energy_lin_s-spread_s_mid",
                    "0 (strict >)",
                    rec.energy_lin_s as i64 - rec.spread_s_mid as i64,
                    ">0 overwrites mid spread with fmult(0.9, energy_lin_m)",
                    "==0 does not overwrite",
                    "threshold_vbr.rs:476-478 init_avoid_hole_flag_ms",
                    "Derived",
                    "R22-S2-AH-PROBE",
                );
            }
        }
    }
    rows.sort_by(|a, b| {
        (
            a.name,
            a.material.as_str(),
            a.frame,
            a.channel,
            a.band,
            a.arm,
        )
            .cmp(&(
                b.name,
                b.material.as_str(),
                b.frame,
                b.channel,
                b.band,
                b.arm,
            ))
    });
    rows
}

fn collect_s4(targets: &[(String, u32)]) -> Vec<ExactMargin> {
    let frames = collect_probe_frames(targets);
    let mut rows = Vec::new();
    for pf in &frames {
        verify_probe_frame(pf);
        replay_frame(
            &pf.q_cap,
            &format!("AC9 S4 Q31 {} f{}", pf.material, pf.frame),
        );
        replay_frame(
            &pf.f_cap,
            &format!("AC9 S4 FPE {} f{}", pf.material, pf.frame),
        );
        for (arm, cap) in [("Q31", &pf.q_cap), ("FPE", &pf.f_cap)] {
            let o = &cap.s6.output;
            let frame_granted = o
                .granted_dyn_bits
                .wrapping_add(cap.s6.static_bits.wrapping_sub(o.static_bits_used));
            let raw_fill = (frame_granted.wrapping_sub(o.used_dyn_bits)) & 7;
            let prelim = o
                .static_bits_used
                .wrapping_add(o.used_dyn_bits)
                .wrapping_add(raw_fill)
                .wrapping_add(GLOBAL_EXT_BITS);
            push_margin_with_class(
                &mut rows,
                "acct.fill_floor",
                "acct",
                pf.material.as_str(),
                pf.frame,
                0,
                0,
                arm,
                format!(
                    "frame_granted={frame_granted};used_dyn={};static_used={};raw_fill={raw_fill};prelim={prelim};min_bits=0",
                    o.used_dyn_bits, o.static_bits_used
                ),
                "prelim-0",
                "0 (strict < engages floor)",
                trace::fill_floor_margin(prelim, 0),
                "<0 engages the min-frame floor; ==0 reads the floor",
                "==0 reads the floor",
                "rate.rs:1664-1679 update_fill_bits",
                "Derived",
                "R22-ACCT-CAPTURE",
            );
            push_margin_with_class(
                &mut rows,
                "acct.fill_ext_gate",
                "acct",
                pf.material.as_str(),
                pf.frame,
                0,
                0,
                arm,
                format!("raw_fill={raw_fill}"),
                "raw_fill-7",
                "0 (inclusive >=)",
                trace::fill_ext_gate_margin(raw_fill),
                ">=0 writes a fill element",
                "==0 writes",
                "rate.rs:1684-1719 fill_extension_bits",
                "Derived",
                "R22-ACCT-CAPTURE",
            );
            push_margin_with_class(
                &mut rows,
                "acct.bitres_max",
                "acct",
                pf.material.as_str(),
                pf.frame,
                0,
                0,
                arm,
                format!("qc_pre={};qc_max={}", cap.s6.qc_pre, cap.s6.qc_max),
                "qc_max-qc_pre",
                "0 (inclusive <=)",
                trace::bitres_max_margin(cap.s6.qc_pre, cap.s6.qc_max),
                "<0 raises QuantError; >=0 passes",
                "==0 exactly full",
                "rate.rs:440-442 prepare_grants bitres guard",
                "Derived",
                "R22-ACCT-CAPTURE",
            );
            push_margin_with_class(
                &mut rows,
                "acct.bitres_floor",
                "acct",
                pf.material.as_str(),
                pf.frame,
                0,
                0,
                arm,
                format!("qc_pre={}", cap.s6.qc_pre),
                "qc_pre-0",
                "0 (inclusive >=)",
                trace::bitres_floor_margin(cap.s6.qc_pre),
                "<0 raises BitresTooLow; >=0 passes",
                "==0 exactly empty",
                "rate.rs:440-442 prepare_grants bitres guard",
                "Derived",
                "R22-ACCT-CAPTURE",
            );
        }
        for (arm, calls) in [("Q31", &pf.q_calls), ("FPE", &pf.f_calls)] {
            for (ch, call) in calls.iter().enumerate() {
                for region in &call.regions {
                    if !region.gate_pass {
                        continue;
                    }
                    for iter in &region.iterations {
                        for be in &iter.bands {
                            let common = (pf.material.as_str(), pf.frame, ch, be.band, arm);
                            push_margin_with_class(
                                &mut rows,
                                "s4.step3_dist",
                                "s4",
                                common.0,
                                common.1,
                                common.2,
                                common.3,
                                common.4,
                                format!("dn={};dist_prev={};D_1122=5572475", be.dn, be.dist_prev),
                                "(dist_prev+5572475)-dn",
                                "0 (inclusive pass)",
                                step3_dist_margin(be.dn, be.dist_prev),
                                ">=0 passes; <0 rejects",
                                "==0 passes",
                                "scf.rs:1693 assimilate_multiple_scf2 step 3",
                                "Derived",
                                "S4-STEP3-PROBE",
                            );
                            push_margin_with_class(
                                &mut rows,
                                "s4.step3_energy",
                                "s4",
                                common.0,
                                common.1,
                                common.2,
                                common.3,
                                common.4,
                                format!(
                                    "en_q={};energy_ld={};D_07079=16722942",
                                    be.en_q, be.energy_ld
                                ),
                                "en_q-(energy_ld-16722942)",
                                "0 (inclusive pass)",
                                step3_energy_margin(be.en_q, be.energy_ld),
                                ">=0 passes; <0 rejects",
                                "==0 passes",
                                "scf.rs:1694 assimilate_multiple_scf2 step 3",
                                "Derived",
                                "S4-STEP3-PROBE",
                            );
                        }
                        push_margin_with_class(
                            &mut rows,
                            "s4.step3_pe_gate",
                            "s4",
                            pf.material.as_str(),
                            pf.frame,
                            ch,
                            region.start,
                            arm,
                            format!(
                                "delta_new={};delta_pe_before={};d_scf={};k={}",
                                iter.delta_new, iter.delta_pe_before, iter.d_scf, iter.k
                            ),
                            "0-delta_new",
                            "0 (inclusive pass)",
                            step3_pe_gate_margin(iter.delta_new),
                            ">=0 passes; <0 rejects",
                            "==0 passes",
                            "scf.rs:1645 assimilate_multiple_scf2 step 3",
                            "Derived",
                            "S4-STEP3-PROBE",
                        );
                        if iter.pe_pass {
                            push_margin_with_class(
                                &mut rows,
                                "s4.step3_sum",
                                "s4",
                                pf.material.as_str(),
                                pf.frame,
                                ch,
                                region.start,
                                arm,
                                format!(
                                    "old_sum={};new_sum={};ok={}",
                                    iter.old_sum, iter.new_sum, iter.ok
                                ),
                                "old_sum-new_sum",
                                "0 (strict >)",
                                step3_sum_margin(iter.new_sum, iter.old_sum),
                                ">0 with ok accepts; <=0 rejects",
                                "==0 rejects",
                                "scf.rs:1701 assimilate_multiple_scf2 step 3",
                                "Derived",
                                "S4-STEP3-PROBE",
                            );
                        }
                    }
                }
                for rec in &call.step1 {
                    push_margin_with_class(
                        &mut rows,
                        "s4.step1_pe_gate",
                        "s4",
                        pf.material.as_str(),
                        pf.frame,
                        ch,
                        rec.start,
                        arm,
                        format!(
                            "start={};stop={};act={};prev={};next={};hi={};lo={};scf_start={};nw={};d_scf={};d_spec={};delta_new={};delta_pe_before={};pe_pass={}",
                            rec.start,
                            rec.stop,
                            rec.act,
                            rec.prev,
                            rec.next,
                            rec.hi,
                            rec.lo,
                            rec.scf_start,
                            rec.nw,
                            rec.d_scf,
                            rec.d_spec,
                            rec.delta_new,
                            rec.delta_pe_before,
                            rec.pe_pass
                        ),
                        "0-delta_new",
                        "0 (inclusive pass)",
                        -(rec.delta_new as i64),
                        ">=0 evaluates the trial; <0 skips the coarser trial",
                        "==0 passes",
                        "scf.rs:1243-1245 assimilate_multiple_scf2 step 1",
                        "Derived",
                        "S4-STEP1-PROBE",
                    );
                    if rec.pe_pass {
                        for be in &rec.bands {
                            push_margin_with_class(
                                &mut rows,
                                "s4.step1_dmax",
                                "s4",
                                pf.material.as_str(),
                                pf.frame,
                                ch,
                                be.band,
                                arm,
                                format!(
                                    "band={};nw={};dn={};dmax={};threshold_term={};dist_terms={};dist_base={};energy_floor={};threshold_ld={};energy_ld={};dist_before={};start={};stop={}",
                                    be.band,
                                    rec.nw,
                                    be.dn,
                                    be.dmax,
                                    be.dmax_threshold_term,
                                    be.dmax_dist_terms,
                                    be.dmax_dist_base,
                                    be.dmax_energy_floor,
                                    be.threshold_ld,
                                    be.energy_ld,
                                    be.dist_before,
                                    rec.start,
                                    rec.stop
                                ),
                                "dmax-dn",
                                "0 (inclusive pass; reject only dn>dmax)",
                                be.dmax as i64 - be.dn as i64,
                                ">=0 accepts the band; <0 rejects the whole trial",
                                "==0 accepts",
                                "scf.rs:1195-1199,1284 assimilate_multiple_scf2 step 1",
                                "Derived",
                                "S4-STEP1-PROBE",
                            );
                            if be.terminal {
                                push_margin_with_class(
                                    &mut rows,
                                    "s4.step1_terminal",
                                    "s4",
                                    pf.material.as_str(),
                                    pf.frame,
                                    ch,
                                    be.band,
                                    arm,
                                    format!(
                                        "band={};dn={};energy_ld={};start={};stop={};nw={}",
                                        be.band, be.dn, be.energy_ld, rec.start, rec.stop, rec.nw
                                    ),
                                    "dn-energy_ld",
                                    "0 (==0 terminates later trials)",
                                    be.dn as i64 - be.energy_ld as i64,
                                    "==0 sets check=false and stops the nw loop",
                                    "==0 terminates",
                                    "scf.rs:1286-1287 assimilate_multiple_scf2 step 1",
                                    "Derived",
                                    "S4-STEP1-PROBE",
                                );
                            }
                        }
                    } else {
                        push_margin_with_class(
                            &mut rows,
                            "s4.step1_dmax",
                            "s4",
                            pf.material.as_str(),
                            pf.frame,
                            ch,
                            rec.start,
                            arm,
                            format!(
                                "start={};stop={};nw={};delta_new={};status=Inapplicable",
                                rec.start, rec.stop, rec.nw, rec.delta_new
                            ),
                            "per-band dmax not evaluated",
                            "n/a",
                            0,
                            "Inapplicable: PE gate failed before band evaluation",
                            "n/a",
                            "scf.rs:1245 assimilate_multiple_scf2 step 1",
                            "Inapplicable",
                            "S4-STEP1-PROBE",
                        );
                    }
                }
                for rec in &call.step2 {
                    if rec.attempted {
                        push_margin_with_class(
                            &mut rows,
                            "s4.step2_gate",
                            "s4",
                            pf.material.as_str(),
                            pf.frame,
                            ch,
                            rec.start,
                            arm,
                            format!(
                                "start={};stop={};act={};scf_start={};lo={};hi={};act_eq={};lo_lt_act={};spread_margin={}",
                                rec.start,
                                rec.stop,
                                rec.act,
                                rec.scf_start,
                                rec.lo,
                                rec.hi,
                                rec.gate_act_eq,
                                rec.gate_lo_lt_act,
                                rec.spread_margin
                            ),
                            "MAX_SCF_DELTA-(smax-smin)",
                            "0 (inclusive >=)",
                            rec.spread_margin as i64,
                            ">=0 with act==scf_start and lo<act enables step 2",
                            "==0 enables",
                            "scf.rs:1373-1394 assimilate_multiple_scf2 step 2",
                            "Derived",
                            "S4-STEP2-PROBE",
                        );
                    } else {
                        push_margin_with_class(
                            &mut rows,
                            "s4.step2_gate",
                            "s4",
                            pf.material.as_str(),
                            pf.frame,
                            ch,
                            rec.start,
                            arm,
                            format!(
                                "start={};stop={};act={};scf_start={};lo={};hi={};act_eq={};lo_lt_act={};spread_margin={};status=Inapplicable",
                                rec.start,
                                rec.stop,
                                rec.act,
                                rec.scf_start,
                                rec.lo,
                                rec.hi,
                                rec.gate_act_eq,
                                rec.gate_lo_lt_act,
                                rec.spread_margin
                            ),
                            "step2 gate conjunction",
                            "n/a",
                            0,
                            "Inapplicable: at least one gate conjunct is false",
                            "n/a",
                            "scf.rs:1373-1394 assimilate_multiple_scf2 step 2",
                            "Inapplicable",
                            "S4-STEP2-PROBE",
                        );
                    }
                    if rec.attempted {
                        for be in &rec.bands {
                            push_margin_with_class(
                                &mut rows,
                                "s4.step2_min_scf",
                                "s4",
                                pf.material.as_str(),
                                pf.frame,
                                ch,
                                be.band,
                                arm,
                                format!(
                                    "band={};nw={};min_scf={};violation={};start={};stop={}",
                                    be.band, rec.nw, be.min_scf, be.violation, rec.start, rec.stop
                                ),
                                "nw-min_scf",
                                "0 (inclusive >=)",
                                rec.nw as i64 - be.min_scf as i64,
                                ">=0 admits the band; <0 violates the minimum",
                                "==0 admits",
                                "scf.rs:1407-1412 assimilate_multiple_scf2 step 2",
                                "Derived",
                                "S4-STEP2-PROBE",
                            );
                        }
                        if rec.min_scf_violation {
                            push_margin_with_class(
                                &mut rows,
                                "s4.step2_pe_gate",
                                "s4",
                                pf.material.as_str(),
                                pf.frame,
                                ch,
                                rec.start,
                                arm,
                                format!(
                                    "start={};stop={};nw={};status=Inapplicable;reason=min_scf_violation",
                                    rec.start, rec.stop, rec.nw
                                ),
                                "PE gate not evaluated",
                                "n/a",
                                0,
                                "Inapplicable: minimum-scalefactor violation short-circuits step 2",
                                "n/a",
                                "scf.rs:1373-1460 assimilate_multiple_scf2 step 2",
                                "Inapplicable",
                                "S4-STEP2-PROBE",
                            );
                            push_margin_with_class(
                                &mut rows,
                                "s4.step2_sum",
                                "s4",
                                pf.material.as_str(),
                                pf.frame,
                                ch,
                                rec.start,
                                arm,
                                format!(
                                    "start={};stop={};nw={};status=Inapplicable;reason=min_scf_violation",
                                    rec.start, rec.stop, rec.nw
                                ),
                                "sum gate not evaluated",
                                "n/a",
                                0,
                                "Inapplicable: minimum-scalefactor violation short-circuits step 2",
                                "n/a",
                                "scf.rs:1373-1460 assimilate_multiple_scf2 step 2",
                                "Inapplicable",
                                "S4-STEP2-PROBE",
                            );
                        } else {
                            let delta_new = rec.delta_new.expect("non-violating step2 delta");
                            push_margin_with_class(
                                &mut rows,
                                "s4.step2_pe_gate",
                                "s4",
                                pf.material.as_str(),
                                pf.frame,
                                ch,
                                rec.start,
                                arm,
                                format!(
                                    "start={};stop={};nw={};d_scf={};d_spec={};delta_new={};delta_pe_before={}",
                                    rec.start,
                                    rec.stop,
                                    rec.nw,
                                    rec.d_scf,
                                    rec.d_spec,
                                    delta_new,
                                    rec.delta_pe_before
                                ),
                                "0-delta_new",
                                "0 (inclusive pass)",
                                -(delta_new as i64),
                                ">=0 evaluates the finer trial; <0 skips it",
                                "==0 passes",
                                "scf.rs:1456-1460 assimilate_multiple_scf2 step 2",
                                "Derived",
                                "S4-STEP2-PROBE",
                            );
                            if rec.pe_pass {
                                for be in &rec.bands {
                                    if be.evaluated {
                                        push_margin_with_class(
                                            &mut rows,
                                            "s4.step2_band",
                                            "s4",
                                            pf.material.as_str(),
                                            pf.frame,
                                            ch,
                                            be.band,
                                            arm,
                                            format!(
                                                "band={};nw={};dn={};threshold_ld={};threshold_reject={};old_contrib={};new_contrib={}",
                                                be.band,
                                                rec.nw,
                                                be.dn,
                                                be.threshold_ld,
                                                be.threshold_reject,
                                                be.old_contrib,
                                                be.new_contrib
                                            ),
                                            "threshold_ld-dn",
                                            "0 (inclusive pass; reject only dn>threshold)",
                                            be.threshold_ld as i64 - be.dn as i64,
                                            ">=0 contributes; <0 rejects the whole trial",
                                            "==0 contributes",
                                            "scf.rs:1499,1505 assimilate_multiple_scf2 step 2",
                                            "Derived",
                                            "S4-STEP2-PROBE",
                                        );
                                    } else {
                                        push_margin_with_class(
                                            &mut rows,
                                            "s4.step2_band",
                                            "s4",
                                            pf.material.as_str(),
                                            pf.frame,
                                            ch,
                                            be.band,
                                            arm,
                                            format!(
                                                "band={};nw={};min_scf={};status=NotGenerated",
                                                be.band, rec.nw, be.min_scf
                                            ),
                                            "band not evaluated after an earlier rejection",
                                            "n/a",
                                            0,
                                            "NotGenerated: loop stopped before this band",
                                            "n/a",
                                            "scf.rs:1499-1503 assimilate_multiple_scf2 step 2",
                                            "NotGenerated",
                                            "S4-STEP2-PROBE",
                                        );
                                    }
                                }
                                push_margin_with_class(
                                    &mut rows,
                                    "s4.step2_sum",
                                    "s4",
                                    pf.material.as_str(),
                                    pf.frame,
                                    ch,
                                    rec.start,
                                    arm,
                                    format!(
                                        "start={};stop={};nw={};old_sum={};new_sum={};sum_limit={};broke={}",
                                        rec.start,
                                        rec.stop,
                                        rec.nw,
                                        rec.old_sum,
                                        rec.new_sum,
                                        rec.sum_limit,
                                        rec.bands.iter().any(|b| b.threshold_reject)
                                    ),
                                    "sum_limit-new_sum",
                                    "0 (strict > accept)",
                                    rec.sum_limit as i64 - rec.new_sum as i64,
                                    ">0 accepts the finer trial; <=0 rejects",
                                    "==0 rejects",
                                    "scf.rs:1515 assimilate_multiple_scf2 step 2",
                                    "Derived",
                                    "S4-STEP2-PROBE",
                                );
                            } else {
                                push_margin_with_class(
                                    &mut rows,
                                    "s4.step2_sum",
                                    "s4",
                                    pf.material.as_str(),
                                    pf.frame,
                                    ch,
                                    rec.start,
                                    arm,
                                    format!(
                                        "start={};stop={};nw={};delta_new={};status=Inapplicable",
                                        rec.start, rec.stop, rec.nw, delta_new
                                    ),
                                    "sum gate not evaluated",
                                    "n/a",
                                    0,
                                    "Inapplicable: PE gate failed before the sum gate",
                                    "n/a",
                                    "scf.rs:1460 assimilate_multiple_scf2 step 2",
                                    "Inapplicable",
                                    "S4-STEP2-PROBE",
                                );
                            }
                        }
                    }
                }
                if let Some(clamp) = &call.clamp {
                    for be in &clamp.bands {
                        if be.live {
                            push_margin_with_class(
                                &mut rows,
                                "s4.final_clamp",
                                "s4",
                                pf.material.as_str(),
                                pf.frame,
                                ch,
                                be.band,
                                arm,
                                format!(
                                    "band={};gmin={};limit={};scf_before={};clamped={}",
                                    be.band, clamp.gmin, be.limit, be.scf_before, be.clamped
                                ),
                                "(gmin+MAX_SCF_DELTA)-scf",
                                "0 (strict < clamps)",
                                be.limit as i64 - be.scf_before as i64,
                                "<0 clamps to the limit; >=0 unchanged",
                                "==0 unchanged",
                                "scf.rs:2090,2111 assimilate_multiple_scf2 final clamp",
                                "Derived",
                                "S4-FINAL-CLAMP-PROBE",
                            );
                        } else {
                            push_margin_with_class(
                                &mut rows,
                                "s4.final_clamp",
                                "s4",
                                pf.material.as_str(),
                                pf.frame,
                                ch,
                                be.band,
                                arm,
                                format!(
                                    "band={};gmin={};limit={};scf_before={};status=Inapplicable;reason=NO_SCF",
                                    be.band, clamp.gmin, be.limit, be.scf_before
                                ),
                                "clamp not applicable to dead bands",
                                "n/a",
                                0,
                                "Inapplicable: dead band carries NO_SCF",
                                "n/a",
                                "scf.rs:2090,2111 assimilate_multiple_scf2 final clamp",
                                "Inapplicable",
                                "S4-FINAL-CLAMP-PROBE",
                            );
                        }
                    }
                }
                if let Some(gain) = &call.final_gain {
                    push_margin_with_class(
                        &mut rows,
                        "s4.all_dead",
                        "s4",
                        pf.material.as_str(),
                        pf.frame,
                        ch,
                        0,
                        arm,
                        format!(
                            "gmax={};NO_SCF={};all_dead={};gain={}",
                            gain.gmax,
                            crate::aac_enc::scf::NO_SCF,
                            gain.all_dead,
                            gain.gain
                        ),
                        "gmax-NO_SCF",
                        "0 (==0 all-dead; >0 live)",
                        gain.gmax as i64 - crate::aac_enc::scf::NO_SCF as i64,
                        "==0 zeroes all bands; >0 selects gmax and inverts",
                        "==0 all-dead",
                        "scf.rs:2178-2218 global-gain inversion",
                        "Derived",
                        "S4-FINAL-GAIN-PROBE",
                    );
                    for be in &gain.bands {
                        if be.dead {
                            push_margin_with_class(
                                &mut rows,
                                "s4.gain_inversion",
                                "s4",
                                pf.material.as_str(),
                                pf.frame,
                                ch,
                                be.band,
                                arm,
                                format!(
                                    "band={};scf_before=NO_SCF;gain={};final_scf={};status=Inapplicable;reason=dead_band",
                                    be.band, gain.gain, be.scf_after
                                ),
                                "global-gain inversion not applicable to dead bands",
                                "n/a",
                                0,
                                "Inapplicable: dead band zeroed",
                                "n/a",
                                "scf.rs:2178-2218 global-gain inversion",
                                "Inapplicable",
                                "S4-FINAL-GAIN-PROBE",
                            );
                        } else {
                            push_margin_with_class(
                                &mut rows,
                                "s4.gain_inversion",
                                "s4",
                                pf.material.as_str(),
                                pf.frame,
                                ch,
                                be.band,
                                arm,
                                format!(
                                    "band={};scf_before={};gmax={};gain={};final_scf={}",
                                    be.band, be.scf_before, gain.gmax, gain.gain, be.scf_after
                                ),
                                "gmax-scf_before",
                                "n/a (exact transformation)",
                                gain.gmax as i64 - be.scf_before as i64,
                                "final per-band scalefactor after inversion",
                                "n/a (exact transformation)",
                                "scf.rs:2178-2218 global-gain inversion",
                                "Derived",
                                "S4-FINAL-GAIN-PROBE",
                            );
                        }
                    }
                }
            }
        }
    }
    rows.sort_by(|a, b| {
        (
            a.name,
            a.material.as_str(),
            a.frame,
            a.channel,
            a.band,
            a.arm,
        )
            .cmp(&(
                b.name,
                b.material.as_str(),
                b.frame,
                b.channel,
                b.band,
                b.arm,
            ))
    });
    rows
}

fn collect_s6(crash: &is_ms::Ac9CrashRecord) -> Vec<ExactMargin> {
    let c = &crash.record;
    let r = &crash.reduce;
    let term_a = (r.el_dyn_bits_used + 8) - (r.bit_res_level_el + r.el_granted);
    let term_b = (r.el_dyn_bits_used + r.el_static + 8) - r.max_bits_el;
    let mut rows = Vec::new();
    rows.push(push_margin_row("s6.crash_to_save", "s6", crash.material, crash.frame, Some(0), 0, crash.arm,
        format!("term_a={term_a};term_b={term_b};el_dyn={};bit_res={};el_granted={};el_static={};max_bits_el={}",r.el_dyn_bits_used,r.bit_res_level_el,r.el_granted,r.el_static,r.max_bits_el),
        "max(term_a,term_b)", "0 (strict > crash)", Some(r.bits_to_save.unwrap() as i64), ">0 runs crash recovery; <=0 fallback", "==0 selects fallback", "Derived", "S6-PROBE-SYNTH", "summary", "rate.rs:1336-1339,2117-2124"));
    rows.push(push_margin_row(
        "s6.reduce_branch",
        "s6",
        crash.material,
        crash.frame,
        Some(0),
        0,
        crash.arm,
        format!(
            "branch={};iterations_before={};iterations_after={};max_iterations={};el_dyn={};bits_to_save={:?};term_a={:?};term_b={:?}",
            r.branch,
            r.iterations_before,
            r.iterations_after,
            r.max_iterations,
            r.el_dyn_bits_used,
            r.bits_to_save,
            r.term_a,
            r.term_b
        ),
        "bits_to_save",
        "0 (strict > crash)",
        Some(r.bits_to_save.unwrap() as i64),
        ">0 runs crash recovery; <=0 uses fallback",
        "==0 selects fallback",
        "Derived",
        "S6-PROBE-SYNTH",
        "summary",
        "rate.rs:1300-1429 reduce_bit_consumption",
    ));
    rows.push(push_margin_row(
        "s6.crash_stop_distance",
        "s6",
        crash.material,
        crash.frame,
        Some(0),
        c.surv,
        crash.arm,
        format!(
            "saved_final={};bits_to_save={};stop={}",
            c.saved_final, c.bits_to_save_entry, c.stop
        ),
        "saved_final-bits_to_save",
        "0 (inclusive stop)",
        Some(c.saved_final as i64 - c.bits_to_save_entry as i64),
        ">=0 stops saved_met; <0 continues until exhausted",
        "==0 stops",
        "Derived",
        "S6-PROBE-SYNTH",
        "summary",
        "rate.rs:1537 crash_recovery",
    ));
    for s in &c.steps {
        rows.push(push_margin_row(
            "s6.crash_step",
            "s6",
            crash.material,
            crash.frame,
            Some(s.ch),
            s.sfb as usize,
            crash.arm,
            format!(
                "saved_before={};reclaimed={};side_reclaimed={};saved_after={}",
                s.saved_before, s.reclaimed, s.side_reclaimed, s.saved_after
            ),
            "saved_before+reclaimed+side-saved_after",
            "0 (exact chain)",
            Some(
                s.saved_before as i64 + s.reclaimed as i64 + s.side_reclaimed as i64
                    - s.saved_after as i64,
            ),
            "0 verifies saved accumulation",
            "==0 required",
            "Derived",
            "S6-PROBE-SYNTH",
            "step",
            "rate.rs:1498-1532 crash_recovery",
        ));
    }
    for frame in crate::aac_enc::rate::tests::s6_probe_corpus_for_ac9() {
        let nch = frame.n_channels as i64;
        let npass = frame.passes.len();
        for (pass_idx, pass) in frame.passes.iter().enumerate() {
            let occurrence = format!("pass{pass_idx:02}");
            let cap_margin = nch * MIN_BUFSIZE_PER_EFF_CHAN as i64
                - pass.exit_el_static as i64
                - pass.exit_el_dyn as i64;
            rows.push(push_margin_row(
                "s6.element_cap_pass",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!(
                    "pass_idx={pass_idx};nch={nch};el_dyn={};el_static={};cap={}",
                    pass.exit_el_dyn,
                    pass.exit_el_static,
                    nch * MIN_BUFSIZE_PER_EFF_CHAN as i64
                ),
                "nch*6144-el_static-el_dyn",
                "0 (inclusive >=)",
                Some(cap_margin),
                "<0 fails element constraints; >=0 passes",
                "==0 passes",
                "Derived",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:2235 element-cap loop",
            ));
            rows.push(push_margin_row(
                "s6.exit_headroom_pass",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!(
                    "pass_idx={pass_idx};total_avail={};sum_bits={}",
                    pass.exit.total_avail, pass.exit_sum_bits
                ),
                "total_avail-sum_bits",
                "0 (strict > favors done)",
                Some(pass.exit.total_avail as i64 - pass.exit_sum_bits as i64),
                ">0 under the frame budget; <=0 not under",
                "==0 is not under",
                "Derived",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:2281-2291 exit_matrix_step",
            ));
            rows.push(push_margin_row(
                "s6.multipass_entry",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!(
                    "pass_idx={pass_idx};entry_iterations={};max_iterations={}",
                    pass.entry_iterations, MAX_ITERATIONS
                ),
                "max_iterations-entry_iterations",
                "0 (strict > retries; ==0 crash boundary)",
                Some(MAX_ITERATIONS as i64 - pass.entry_iterations as i64),
                ">0 retries remain; ==0 triggers crash recovery",
                "==0 crash boundary",
                "Derived",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:1277-1300 reduce_bit_consumption",
            ));
            for mq in &pass.max_quant {
                rows.push(push_margin_row(
                    "s6.max_quant",
                    "s6",
                    frame.material,
                    frame.frame,
                    Some(mq.ch),
                    0,
                    "S6-PROBE-CORPUS",
                    format!(
                        "pass_idx={pass_idx};ch={};max_all={};MAX_QUANT={};fail={}",
                        mq.ch, mq.max_all, mq.limit, mq.fail
                    ),
                    "MAX_QUANT-max_all",
                    "0 (inclusive pass; fail only max_all>MAX_QUANT)",
                    Some(mq.limit as i64 - mq.max_all as i64),
                    ">=0 passes; <0 fails the channel constraint",
                    "==0 passes",
                    "Derived",
                    "S6-PROBE-CORPUS",
                    &occurrence,
                    "rate.rs:2196 MAX_QUANT gate",
                ));
            }
            let pass_reduces: Vec<_> = frame
                .reduces
                .iter()
                .filter(|r| r.pass_idx == pass_idx)
                .collect();
            if pass_reduces.is_empty() {
                rows.push(push_margin_row(
                    "s6.reduce_branch",
                    "s6",
                    frame.material,
                    frame.frame,
                    Some(0),
                    0,
                    "S6-PROBE-CORPUS",
                    format!("pass_idx={pass_idx};status=Inapplicable;reason=no_reduce_call"),
                    "reduce_bit_consumption not reached",
                    "n/a",
                    None,
                    "Inapplicable: pass converged without a reduce decision",
                    "n/a",
                    "Inapplicable",
                    "S6-PROBE-CORPUS",
                    &occurrence,
                    "rate.rs:2092-2157 reduce call site",
                ));
                rows.push(push_margin_row(
                    "s6.crash_error_gate",
                    "s6",
                    frame.material,
                    frame.frame,
                    Some(0),
                    0,
                    "S6-PROBE-CORPUS",
                    format!("pass_idx={pass_idx};status=Inapplicable;reason=no_reduce_call"),
                    "el_dyn==0 guard not evaluated",
                    "n/a",
                    None,
                    "Inapplicable: no reduce call in this pass",
                    "n/a",
                    "Inapplicable",
                    "S6-PROBE-CORPUS",
                    &occurrence,
                    "rate.rs:1307-1335 crash error gate",
                ));
            } else {
                for (ri, reduce) in pass_reduces.iter().enumerate() {
                    let occ = format!("pass{pass_idx:02}reduce{ri:02}");
                    let (value, equation, threshold, decision, equality) =
                        match reduce.branch {
                            "gain_bumped" => (
                                reduce.max_iterations as i64 - reduce.iterations_before as i64,
                                "max_iterations-iterations_before",
                                "0 (strict > retries)",
                                ">0 bumps gains; ==0 is the crash boundary",
                                "==0 crash boundary",
                            ),
                            "crash_required" => (
                                reduce.bits_to_save.unwrap_or(0) as i64,
                                "bits_to_save",
                                "0 (strict > crash)",
                                ">0 runs crash recovery; <=0 uses fallback",
                                "==0 fallback",
                            ),
                            "err_empty" => (
                                reduce.el_dyn_bits_used as i64,
                                "el_dyn_bits_used",
                                "0 (==0 QuantError)",
                                "==0 raises QuantError; >0 would not",
                                "==0 error",
                            ),
                            other => (
                                reduce.max_iterations as i64 - reduce.iterations_before as i64,
                                "max_iterations-iterations_before",
                                "0 (strict >)",
                                if other == "err_over" {
                                    "<=0 is the over-max error branch"
                                } else {
                                    "branch observation"
                                },
                                "n/a",
                            ),
                        };
                    rows.push(push_margin_row(
                        "s6.reduce_branch",
                        "s6",
                        frame.material,
                        frame.frame,
                        Some(0),
                        0,
                        "S6-PROBE-CORPUS",
                        format!(
                            "pass_idx={pass_idx};branch={};iterations_before={};iterations_after={};max_iterations={};el_dyn={};bit_res_level={};el_granted={};el_static={};max_bits_el={};bits_to_save={:?};term_a={:?};term_b={:?}",
                            reduce.branch,
                            reduce.iterations_before,
                            reduce.iterations_after,
                            reduce.max_iterations,
                            reduce.el_dyn_bits_used,
                            reduce.bit_res_level_el,
                            reduce.el_granted,
                            reduce.el_static,
                            reduce.max_bits_el,
                            reduce.bits_to_save,
                            reduce.term_a,
                            reduce.term_b
                        ),
                        equation,
                        threshold,
                        Some(value),
                        decision,
                        equality,
                        "Derived",
                        "S6-PROBE-CORPUS",
                        &occ,
                        "rate.rs:1300-1429 reduce_bit_consumption",
                    ));
                    if reduce.branch == "err_empty" {
                        rows.push(push_margin_row(
                            "s6.crash_error_gate",
                            "s6",
                            frame.material,
                            frame.frame,
                            Some(0),
                            0,
                            "S6-PROBE-CORPUS",
                            format!(
                                "pass_idx={pass_idx};el_dyn={};==0 raises QuantError",
                                reduce.el_dyn_bits_used
                            ),
                            "el_dyn_bits_used-0",
                            "0 (==0 QuantError)",
                            Some(reduce.el_dyn_bits_used as i64),
                            "==0 raises QuantError",
                            "==0 error",
                            "Derived",
                            "S6-PROBE-CORPUS",
                            &occ,
                            "rate.rs:1307-1335 crash error gate",
                        ));
                    }
                }
            }
            rows.push(push_margin_row(
                "s6.exit_sum_dyn",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!(
                    "pass_idx={pass_idx};sum_dyn={};sum_bits={};total_avail={}",
                    pass.exit.sum_dyn, pass.exit.sum_bits, pass.exit.total_avail
                ),
                "sum_dyn (==0 short-circuit)",
                "0 (==0 special case)",
                Some(pass.exit.sum_dyn as i64),
                "==0 takes the zero-dynamic-bit branch",
                "==0 special case",
                "Derived",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:1623-1632 exit_matrix_step",
            ));
            rows.push(push_margin_row(
                "s6.exit_decrease",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!(
                    "pass_idx={pass_idx};decrease_before={};iterations={};max_iterations={}",
                    pass.exit.decrease_before, pass.exit.iterations, pass.exit.max_iterations
                ),
                "decrease_before (==1 gate)",
                "1 (==1)",
                Some(pass.exit.decrease_before as i64),
                "==1 participates in the first done arm",
                "==1",
                "Derived",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:1623-1629 exit_matrix_step",
            ));
            rows.push(push_margin_row(
                "s6.exit_avg",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!(
                    "pass_idx={pass_idx};sum_dyn={};avg_total_dyn={}",
                    pass.exit.sum_dyn, pass.exit.avg_total_dyn
                ),
                "sum_dyn-avg_total_dyn",
                "0 (inclusive >=)",
                Some(pass.exit.sum_dyn as i64 - pass.exit.avg_total_dyn as i64),
                ">=0 favors done in the second matrix",
                "==0 favors done",
                "Derived",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:1638 exit_matrix_step",
            ));
            rows.push(push_margin_row(
                "s6.exit_emergency",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!(
                    "pass_idx={pass_idx};iterations={};max_iterations={}",
                    pass.exit.iterations, pass.exit.max_iterations
                ),
                "iterations-max_iterations",
                "0 (inclusive >= emergency)",
                Some(pass.exit.iterations as i64 - pass.exit.max_iterations as i64),
                ">=0 sets emergency",
                "==0 emergency",
                "Derived",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:1633 exit_matrix_step",
            ));
            rows.push(push_margin_row(
                "s6.exit_overshoot",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!(
                    "pass_idx={pass_idx};used_dyn={};max_dyn={}",
                    pass.exit.used_dyn, pass.exit.max_dyn
                ),
                "max_dyn-used_dyn",
                "0 (strict < overshoot)",
                Some(pass.exit.max_dyn as i64 - pass.exit.used_dyn as i64),
                "<0 sets overshoot; >=0 does not",
                "==0 does not",
                "Derived",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:1634 exit_matrix_step",
            ));
            rows.push(push_margin_row(
                "s6.exit_min_demand",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!("pass_idx={pass_idx};check_min_frame_demand=true"),
                "constant-true stub",
                "1",
                Some(1),
                "constant true; false arm is not observable",
                "n/a constant stub",
                "Observed",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:1623,1644,1647 exit_matrix_step",
            ));
            rows.push(push_margin_row(
                "s6.exit_done",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!(
                    "pass_idx={pass_idx};done={};next_decrease={};reset={};iterations={}",
                    pass.exit.done, pass.exit.next_decrease, pass.exit.reset, pass.exit.iterations
                ),
                "Observed exit-matrix outcome",
                "1 (done)",
                Some(pass.exit.done as i64),
                "1 terminates the pass loop; 0 continues",
                "n/a observed branch",
                "Observed",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:2281-2343 pass/exit record",
            ));
            rows.push(push_margin_row(
                "s6.exit_reset",
                "s6",
                frame.material,
                frame.frame,
                Some(0),
                0,
                "S6-PROBE-CORPUS",
                format!(
                    "pass_idx={pass_idx};reset={};done={}",
                    pass.exit.reset, pass.exit.done
                ),
                "Observed exit-matrix reset",
                "1 (reset)",
                Some(pass.exit.reset as i64),
                "1 resets constraints/ch_ok for the next pass",
                "n/a observed branch",
                "Observed",
                "S6-PROBE-CORPUS",
                &occurrence,
                "rate.rs:2347-2352 pass/exit record",
            ));
            if pass.exit_crash_mark >= 0 {
                rows.push(push_margin_row(
                    "s6.crash_mark_pass",
                    "s6",
                    frame.material,
                    frame.frame,
                    Some(0),
                    0,
                    "S6-PROBE-CORPUS",
                    format!(
                        "pass_idx={pass_idx};crash_mark={}",
                        pass.exit_crash_mark
                    ),
                    "Observed crash participation",
                    ">=0 (survivor maxSfb)",
                    Some(pass.exit_crash_mark as i64),
                    "crash recovery ran in this pass",
                    "n/a observed branch",
                    "Observed",
                    "S6-PROBE-CORPUS",
                    &occurrence,
                    "rate.rs:2150 crash_mark assignment",
                ));
            } else {
                rows.push(push_margin_row(
                    "s6.crash_mark_pass",
                    "s6",
                    frame.material,
                    frame.frame,
                    Some(0),
                    0,
                    "S6-PROBE-CORPUS",
                    format!(
                        "pass_idx={pass_idx};crash_mark={};status=Inapplicable",
                        pass.exit_crash_mark
                    ),
                    "crash recovery not entered",
                    "n/a",
                    None,
                    "Inapplicable: no crash recovery in this pass",
                    "n/a",
                    "Inapplicable",
                    "S6-PROBE-CORPUS",
                    &occurrence,
                    "rate.rs:2108-2155 crash decision",
                ));
            }
            if let Some(decision) = frame
                .decisions
                .iter()
                .find(|d| d.pass_idx == pass_idx)
            {
                rows.push(push_margin_row(
                    "s6.crash_decision_pass",
                    "s6",
                    frame.material,
                    frame.frame,
                    Some(0),
                    0,
                    "S6-PROBE-CORPUS",
                    format!(
                        "pass_idx={pass_idx};bits_to_save={};decision={}",
                        decision.bits_to_save, decision.decision
                    ),
                    "bits_to_save-0",
                    "0 (strict > crash)",
                    Some(decision.bits_to_save as i64),
                    ">0 runs crash recovery; <=0 uses fallback",
                    "==0 selects fallback",
                    "Derived",
                    "S6-PROBE-CORPUS",
                    &occurrence,
                    "rate.rs:2113-2124 crash decision",
                ));
            } else {
                rows.push(push_margin_row(
                    "s6.crash_decision_pass",
                    "s6",
                    frame.material,
                    frame.frame,
                    Some(0),
                    0,
                    "S6-PROBE-CORPUS",
                    format!("pass_idx={pass_idx};status=Inapplicable"),
                    "no CrashRequired decision",
                    "n/a",
                    None,
                    "Inapplicable: pass did not reach a crash/fallback decision",
                    "n/a",
                    "Inapplicable",
                    "S6-PROBE-CORPUS",
                    &occurrence,
                    "rate.rs:2108-2112 crash decision",
                ));
            }
        }
        rows.push(push_margin_row(
            "s6.later_pass",
            "s6",
            frame.material,
            frame.frame,
            Some(0),
            0,
            "S6-PROBE-CORPUS",
            format!(
                "npass={npass};done={};next_pass={npass};status=Unobserved",
                frame.frame_record.npass
            ),
            "next pass not generated",
            "n/a",
            None,
            "Unobserved: pass loop terminated before this pass",
            "n/a",
            "Unobserved",
            "S6-PROBE-CORPUS",
            &format!("pass{npass:02}"),
            "rate.rs:2064-2353 outer pass loop",
        ));
    }
    rows
}

/// Test-only AC-9 evidence entry point. All captures are owned clones and all
/// production replays assert within-arm equality before a row is printed.
fn push_margin_row(
    name: &'static str,
    stage: &'static str,
    material: &str,
    frame: u32,
    channel: Option<usize>,
    band: usize,
    arm: &'static str,
    operands: String,
    equation: &'static str,
    threshold: &'static str,
    value: Option<i64>,
    decision: &'static str,
    equality: &'static str,
    evidence_class: &'static str,
    probe: &'static str,
    occurrence: &str,
    source: &'static str,
) -> ExactMargin {
    ExactMargin {
        name,
        stage,
        material: material.to_string(),
        frame,
        channel,
        band,
        arm,
        operands,
        equation,
        threshold,
        value,
        decision,
        equality,
        evidence_class,
        probe,
        occurrence: occurrence.to_string(),
        source,
    }
}

pub(super) fn collect_ac9_evidence() -> (Vec<ExactMargin>, usize, usize) {
    let mats = is_ms::materials();
    let mut digest = Vec::new();
    let mut coding = Vec::new();
    let mut caps: BTreeMap<(usize, u32), (FrameSnapshot, FrameSnapshot)> = BTreeMap::new();
    let mut pairs: BTreeMap<(usize, u32), is_ms::FramePair> = BTreeMap::new();
    for (mi, m) in mats.iter().enumerate() {
        let mut h = R21BHarness::new(m.pcm.clone(), m.sample_rate);
        let mut persist: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
        let mut prev_geo = (0usize, 0usize);
        for f in 0..m.frames {
            let pair = h.step(f);
            let a = is_ms::analyze_frame(&pair, m.name, mi, m.sample_rate, &persist);
            if a.digest_diff {
                digest.push((mi, f));
            }
            if !a.coding_diffs.is_empty() {
                coding.push((mi, f, a.coding_diffs.clone()));
            }
            let geo = (pair.q_op.cnt, pair.q_op.per);
            if geo != prev_geo {
                persist.clear();
                prev_geo = geo;
            }
            let diffs: std::collections::HashSet<usize> = a.coding_diffs.iter().copied().collect();
            persist.retain(|b, _| diffs.contains(b));
            for &b in &diffs {
                *persist.entry(b).or_insert(0) += 1;
            }
            caps.insert((mi, f), (pair.q_snap.clone(), pair.f_snap.clone()));
            pairs.insert((mi, f), pair);
        }
    }
    let affected_keys: std::collections::BTreeSet<(usize, u32)> = digest
        .iter()
        .copied()
        .chain(coding.iter().map(|(mi, f, _)| (*mi, *f)))
        .collect();
    caps.retain(|key, _| affected_keys.contains(key));
    pairs.retain(|key, _| affected_keys.contains(key));
    assert_eq!(caps.len(), 24, "AC-9 S1/S2 affected-frame scope");
    digest.sort_unstable();
    coding.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    let mut targets: BTreeMap<(usize, u32), Vec<usize>> = BTreeMap::new();
    for (mi, f) in &digest {
        targets.entry((*mi, *f)).or_default();
    }
    for (mi, f, bands) in &coding {
        targets
            .entry((*mi, *f))
            .or_default()
            .extend(bands.iter().copied());
    }
    for v in targets.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    println!(
        "AC9-S1S2-SCOPE\tframes={}\tdigest={}\tcoding={}\tmaterial_order={:?}",
        targets.len(),
        digest.len(),
        coding.len(),
        mats.iter().map(|m| m.name).collect::<Vec<_>>()
    );
    let mut is_rows = Vec::new();
    for ((mi, f), _) in &targets {
        is_rows.extend(collect_is_margin_frame(
            mats[*mi].name,
            *f,
            &pairs[&(*mi, *f)],
        ));
    }
    let mut s1s2_rows = collect_s1_s2(&caps, &mats);
    let s1s2_frames = caps.len();
    assert_eq!(
        s1s2_frames,
        targets.len(),
        "S1/S2 scope equals affected targets"
    );
    let (_, s4_targets) = r21b_affected_frames();
    let mut s4_targets: Vec<(String, u32)> = s4_targets.into_iter().collect();
    s4_targets.sort();
    s4_targets.dedup();
    let s4_targets_vec = s4_targets.clone();
    let s4_rows = collect_s4(&s4_targets_vec);
    let crash = is_ms::capture_ac9_crash();
    let s6_rows = collect_s6(&crash);
    let mut all = is_rows;
    all.extend(s1s2_rows.drain(..));
    all.extend(s4_rows);
    all.extend(s6_rows);
    all.sort_by(|a, b| {
        (
            &a.name,
            &a.material,
            a.frame,
            a.channel,
            a.band,
            a.arm,
            &a.occurrence,
        )
            .cmp(&(
                &b.name,
                &b.material,
                b.frame,
                b.channel,
                b.band,
                b.arm,
                &b.occurrence,
            ))
    });
    let mut identities = std::collections::BTreeSet::new();
    for row in &all {
        let key = (
            row.name,
            row.material.as_str(),
            row.frame,
            row.channel,
            row.band,
            row.arm,
            row.probe,
            row.occurrence.as_str(),
        );
        assert!(
            identities.insert(key),
            "duplicate AC-9 margin identity: {}/{} f{} ch{:?} b{} {}/{}/{}",
            row.name,
            row.material,
            row.frame,
            row.channel,
            row.band,
            row.arm,
            row.probe,
            row.occurrence
        );
    }
    for row in &all {
        assert!(
            row.source.contains(".rs:"),
            "AC-9 source provenance missing for {}/{}: {}",
            row.name,
            row.material,
            row.source
        );
        assert!(
            matches!(
                row.probe,
                "R21B-IS-LOCKSTEP"
                    | "R22-S1-CAPTURE"
                    | "R22-S2-CAPTURE"
                    | "R22-S2-AH-PROBE"
                    | "R22-ACCT-CAPTURE"
                    | "S4-STEP3-PROBE"
                    | "S4-STEP1-PROBE"
                    | "S4-STEP2-PROBE"
                    | "S4-FINAL-CLAMP-PROBE"
                    | "S4-FINAL-GAIN-PROBE"
                    | "S6-PROBE-SYNTH"
                    | "S6-PROBE-CORPUS"
            ),
            "AC-9 probe identity missing for {}/{}: {}",
            row.name,
            row.material,
            row.probe
        );
    }
    for row in &all {
        println!("{}", row.render());
    }
    println!("AC9-MARGIN-COMPLETE\ts1s2_frames={}\ts4_frames={}\ts6_synth={}:f{}:{}\tpartial_numerical_accepted=true\tac9_whole_accepted=false",s1s2_frames,s4_targets.len(),crash.material,crash.frame,crash.arm);
    (all, s1s2_frames, s4_targets.len())
}

pub(super) fn print_exact_margin_evidence() {
    println!(
        "AC9-MARGIN-SCHEMA\t2\tinteger-operands-only\tpartial-numerical-accepted\tac9-whole-open"
    );
    let _ = collect_ac9_evidence();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac9_exact_margin_evidence() {
        print_exact_margin_evidence();
    }

    #[test]
    fn ac9_short_reduced_matches_existing_s2_probe_on_short_frame() {
        let caps = is_ms::capture_ac9_short_caps();
        let (q, f) = caps.expect("short capture");
        for (arm, cap) in [("Q31", &q), ("FPE", &f)] {
            let s2 = cap.r22_cap.as_ref().expect("R22 capture").s2.clone();
            for (ch, c) in s2.ch.iter().enumerate() {
                if c.seq != 2 {
                    continue;
                }
                for b in 0..c.cnt {
                    let (_, _, _, reduced, _, _, _, _, _) = s2_derived_values(&s2, ch, b);
                    let post = s2.threshold_pre_enfac[ch][b];
                    // Cross-check only bands whose reduction path actually
                    // ran. threshold_pre_enfac is post-reduction and post-enfac;
                    // the report carries the exact pre-clamp `reduced` operand.
                    let (_, _, _, _, admit, _, _, _, _) = s2_derived_values(&s2, ch, b);
                    if admit != 0
                        && c.enfac[b] == 0
                        && s2.ah_flag[ch][b] == 0
                        && post != c.threshold[b]
                    {
                        assert_eq!(reduced, post, "{arm} ch{ch} b{b}");
                    }
                }
            }
        }
    }

    #[test]
    fn ac9_s4_step1_step2_and_final_clamp_evidence() {
        let (_, coding) = r21b_affected_frames();
        let mut targets: Vec<(String, u32)> = coding;
        targets.sort();
        targets.dedup();
        assert_eq!(targets.len(), 24);
        let rows = collect_s4(&targets);
        let count = |name: &str| rows.iter().filter(|r| r.name == name).count();
        assert!(count("s4.step1_pe_gate") > 0, "step1 PE evidence");
        assert!(count("s4.step1_dmax") > 0, "step1 dmax evidence");
        assert!(count("s4.step2_gate") > 0, "step2 gate evidence");
        assert!(count("s4.step2_min_scf") > 0, "step2 minimum evidence");
        assert!(count("s4.step2_pe_gate") > 0, "step2 PE evidence");
        assert!(count("s4.step2_band") > 0, "step2 band evidence");
        assert!(count("s4.step2_sum") > 0, "step2 sum evidence");
        assert!(count("s4.final_clamp") > 0, "final clamp evidence");
        assert!(
            rows.iter()
                .any(|r| r.name == "s4.step2_gate" && r.evidence_class == "Derived"),
            "at least one attempted step2"
        );
        assert!(
            rows.iter()
                .any(|r| r.name == "s4.step2_gate" && r.evidence_class == "Inapplicable"),
            "at least one gated-off step2"
        );
        assert!(
            rows.iter()
                .any(|r| r.name == "s4.final_clamp" && r.evidence_class == "Derived"),
            "live final-clamp margin"
        );
        assert!(
            rows.iter()
                .any(|r| r.name == "s4.final_clamp" && r.evidence_class == "Inapplicable"),
            "dead-band final clamp is inapplicable"
        );
        for r in &rows {
            if matches!(r.evidence_class, "Derived" | "Observed") {
                assert!(r.value.is_some(), "{} must carry an exact value", r.name);
            } else {
                assert!(r.value.is_none(), "{} must not carry a value", r.name);
            }
        }
    }

    #[test]
    fn ac9_i9_exact_residual_evidence() {
        let (_, coding) = r21b_affected_frames();
        let materials = is_ms::materials();
        let mut observed = None;
        for (name, frame) in coding {
            let Some(material) = materials.iter().find(|m| m.name == name.as_str()) else {
                continue;
            };
            let mut harness = R21BHarness::new(material.pcm.clone(), material.sample_rate);
            let mut pair = None;
            for f in 0..=frame {
                pair = Some(harness.step(f));
            }
            let pair = pair.expect("stepped frame");
            let rows = collect_is_margin_frame(material.name, frame, &pair);
            if rows.iter().any(|r| r.name == "is.i9_phase") {
                observed = Some((material.name, frame, rows));
                break;
            }
        }
        let (material, frame, rows) = observed.expect("an affected frame with I9 residuals");
        let phase: Vec<_> = rows.iter().filter(|r| r.name == "is.i9_phase").collect();
        let residual: Vec<_> = rows
            .iter()
            .filter(|r| r.name == "is.i9_residual")
            .collect();
        assert!(!phase.is_empty(), "exact lr rows for {material} f{frame}");
        assert!(!residual.is_empty(), "exact ed/es rows");
        for row in &phase {
            assert_eq!(row.evidence_class, "Derived");
            assert!(row.value.is_some());
        }
        for row in &residual {
            assert!(matches!(
                row.evidence_class,
                "Derived" | "Inapplicable" | "Unavailable"
            ));
            if row.evidence_class != "Derived" {
                assert!(row.value.is_none(), "non-derived residual has no value");
            }
        }
        assert!(
            residual.iter().any(|r| r.evidence_class == "Derived"),
            "at least one selected I9 residual is exact"
        );
    }

    #[test]
    fn ac9_s6_per_pass_sweep_and_unreached_arms() {
        let crash = is_ms::capture_ac9_crash();
        let rows = collect_s6(&crash);
        let corpus = |name: &str| {
            rows.iter()
                .filter(|r| r.name == name && r.probe == "S6-PROBE-CORPUS")
                .count()
        };
        assert_eq!(corpus("s6.element_cap_pass"), 20);
        assert_eq!(corpus("s6.exit_headroom_pass"), 20);
        assert_eq!(corpus("s6.multipass_entry"), 20);
        assert_eq!(corpus("s6.later_pass"), 20);
        assert_eq!(corpus("s6.crash_decision_pass"), 20);
        assert_eq!(
            rows.iter()
                .filter(|r| r.name == "s6.crash_step" && r.value == Some(0))
                .count(),
            16
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r.name == "s6.crash_stop_distance" && r.value == Some(34))
                .count(),
            1
        );
        assert!(rows
            .iter()
            .filter(|r| r.name == "s6.element_cap_pass")
            .all(|r| r.evidence_class == "Derived"));
        assert!(rows
            .iter()
            .filter(|r| r.name == "s6.exit_headroom_pass")
            .all(|r| r.evidence_class == "Derived"));
        assert!(rows
            .iter()
            .filter(|r| r.name == "s6.later_pass")
            .all(|r| r.evidence_class == "Unobserved" && r.value.is_none()));
        assert!(rows
            .iter()
            .filter(|r| r.name == "s6.crash_mark_pass" && r.probe == "S6-PROBE-CORPUS")
            .all(|r| r.evidence_class == "Inapplicable" && r.value.is_none()));
        assert!(!rows
            .iter()
            .any(|r| r.name == "s6.crash_mark_pass" && r.value == Some(0)
                && r.probe == "S6-PROBE-CORPUS"));
    }

    #[test]
    fn ac9_s1_s2_avoid_hole_and_s4_gain_families() {
        let (_, coding) = r21b_affected_frames();
        let materials = is_ms::materials();
        let (name, frame) = coding
            .iter()
            .find(|(m, _)| {
                m == "multitone-stereo"
                    && materials.iter().any(|candidate| candidate.name == m.as_str())
            })
            .or_else(|| {
                coding
                    .iter()
                    .find(|(m, _)| materials.iter().any(|candidate| candidate.name == m.as_str()))
            })
            .expect("affected material")
            .clone();
        let mi = materials
            .iter()
            .position(|m| m.name == name.as_str())
            .expect("material index");
        let material = &materials[mi];
        let mut harness = R21BHarness::new(material.pcm.clone(), material.sample_rate);
        let mut pair = None;
        for f in 0..=frame {
            pair = Some(harness.step(f));
        }
        let pair = pair.expect("stepped frame");
        let mut caps = BTreeMap::new();
        caps.insert(
            (mi, frame),
            (pair.q_snap.clone(), pair.f_snap.clone()),
        );
        let s1s2 = collect_s1_s2(&caps, &materials);
        let cap = pair.q_snap.r22_cap.as_ref().expect("Q31 R22 capture");
        let ms_bands: std::collections::BTreeSet<usize> =
            cap.s2.ms_tweaks.iter().map(|r| r.band).collect();
        for rec in &cap.s2.peak_valley {
            if !ms_bands.contains(&rec.band) {
                assert_eq!(
                    rec.after_valley, cap.s2.min_snr_ld[rec.channel][rec.band],
                    "peak/valley probe matches captured post-init minSnr"
                );
            }
        }
        for rec in &cap.s2.ms_tweaks {
            assert_eq!(
                rec.min_snr_m_after, cap.s2.min_snr_ld[0][rec.band],
                "MS mid probe matches captured post-init minSnr"
            );
            assert_eq!(
                rec.min_snr_s_after, cap.s2.min_snr_ld[1][rec.band],
                "MS side probe matches captured post-init minSnr"
            );
        }
        for name in [
            "pe.patch_execution",
            "s2.ah_peak",
            "s2.ah_valley",
            "s2.ms_min_snr_m",
            "s2.ms_min_snr_s",
            "s2.ms_spread_s",
            "s2.ms_spread_m",
        ] {
            assert!(
                s1s2.iter().any(|r| r.name == name)
                    || (name.starts_with("s2.ah_")
                        && s1s2.iter().any(|r| r.name == "s2.ah_peak_valley")),
                "{name} evidence present"
            );
        }
        if s1s2
            .iter()
            .any(|r| r.name == "pe.patch_execution" && r.evidence_class == "Observed")
        {
            assert!(
                s1s2.iter().any(|r| r.name == "pe.patch_write_gate"),
                "patch write gate evidence present for long channels"
            );
        }
        assert!(
            s1s2
                .iter()
                .any(|r| r.name.starts_with("pe.patch_")),
            "patch family evidence present"
        );
        if s1s2.iter().any(|r| r.name.starts_with("s2.ms_")) {
            assert!(
                s1s2
                    .iter()
                    .any(|r| r.name == "s2.ms_spread_s" && r.evidence_class == "Derived"),
                "MS spread cross-tweak margin derived"
            );
        }

        let s4 = collect_s4(&[(name.clone(), frame)]);
        assert!(s4.iter().any(|r| r.name == "s4.all_dead"));
        assert!(s4.iter().any(|r| r.name == "s4.gain_inversion"));
        for name in [
            "acct.fill_floor",
            "acct.fill_ext_gate",
            "acct.bitres_max",
            "acct.bitres_floor",
        ] {
            assert!(s4.iter().any(|r| r.name == name), "{name} evidence present");
        }
        if s4
            .iter()
            .any(|r| r.name == "s4.gain_inversion" && r.evidence_class == "Inapplicable")
        {
            assert!(
                s4.iter().any(|r| r.name == "s4.gain_inversion" && r.value.is_none()),
                "dead-band gain inversion has no numeric margin"
            );
        }

        let crash = is_ms::capture_ac9_crash();
        let s6 = collect_s6(&crash);
        assert!(
            s6.iter().any(|r| r.name == "s6.max_quant" && r.value.is_some()),
            "MAX_QUANT gate evidence"
        );
        assert!(
            s6.iter().any(|r| r.name == "s6.reduce_branch"),
            "reduce branch evidence"
        );
        assert!(
            s6.iter().any(|r| r.name == "s6.reduce_branch"
                && r.evidence_class == "Derived"
                && r.probe == "S6-PROBE-SYNTH"),
            "synthetic crash reduce branch is exact"
        );
        for name in [
            "s6.exit_sum_dyn",
            "s6.exit_decrease",
            "s6.exit_avg",
            "s6.exit_emergency",
            "s6.exit_overshoot",
            "s6.exit_min_demand",
        ] {
            assert!(s6.iter().any(|r| r.name == name), "{name} evidence present");
        }
    }

    #[test]
    fn ac9_s4_probe_operands_are_already_available() {
        let targets = vec![("probe-narrow-48k".to_string(), 1u32)];
        let frames = collect_probe_frames(&targets);
        assert!(!frames.is_empty());
        let mut bands = 0;
        let mut iters = 0;
        for pf in &frames {
            verify_probe_frame(pf);
            for calls in [&pf.q_calls, &pf.f_calls] {
                for call in calls {
                    for r in &call.regions {
                        if r.gate_pass {
                            for i in &r.iterations {
                                iters += 1;
                                bands += i.bands.len();
                            }
                        }
                    }
                }
            }
        }
        assert!(iters > 0 && bands > 0);
    }
}
