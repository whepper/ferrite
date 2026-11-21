//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! Test-only Q31↔FPE differential audit (FPE-3.1R1, `docs/fpe/05` §9,
//! `docs/fpe/07` §5). This module is `#[cfg(test)]` only: it adds no
//! production API and never feeds back into either engine.
//!
//! Responsibilities:
//! - compute the owner-approved per-band energy envelope (`envelope.rs`);
//! - compare rescale/spread/grouped energies with **collect-all** failures;
//! - compare energy Ld in the energy domain, measuring absolute Ld deltas;
//! - retain `q31_tol` for thresholds, `nm1`, grouped thresholds and reused
//!   Q31-kernel outputs;
//! - run the three independent checks (Q31 reference, FPE arithmetic, and
//!   decomposition term-bound consistency; the latter is not claimed as a
//!   general shared-error detector);
//! - classify Class A/B sentinel divergences by actual stored state.

use super::super::constants::C_RATIO_F;
use super::envelope::{self, BandEnvelope};
use crate::aac_enc::psy::{ChannelOutput, GroupedOutput};
use crate::aac_enc::sfb::SfbConfig;

/// `q31_tol = 32 + |ref|/500` as f64 (retained for the fields documented in
/// `05` §9.3).
pub(super) fn q31_tol_f(want: i32) -> f64 {
    32.0 + (want as f64).abs() / 500.0
}

/// Energy-domain inversion of a fraction Ld value (sentinel `≤ −1.0` → 0).
pub(super) fn ld_to_e(ld: f64) -> f64 {
    if ld <= -1.0 {
        0.0
    } else {
        2f64.powf(ld * 64.0 + 31.0)
    }
}

/// Energy vector class (kept distinguishable in the report).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnergyKind {
    Rescale,
    Spread,
    Grouped,
}

impl EnergyKind {
    fn name(self) -> &'static str {
        match self {
            EnergyKind::Rescale => "rescale",
            EnergyKind::Spread => "spread",
            EnergyKind::Grouped => "grouped",
        }
    }
}

/// Ld vector class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LdClass {
    WindowEnergy,
    GroupedEnergy,
    Threshold,
}

impl LdClass {
    fn name(self) -> &'static str {
        match self {
            LdClass::WindowEnergy => "window-energy",
            LdClass::GroupedEnergy => "grouped-energy",
            LdClass::Threshold => "threshold",
        }
    }
}

/// Aggregated measured deviations for one energy vector class.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct EnergyAgg {
    pub count: usize,
    pub failures: usize,
    pub unsupported: usize,
    pub max_delta: i64,
    pub max_ref: i64,
    pub max_rel: f64,
    pub max_q: f64,
    pub max_t: f64,
    pub max_r: f64,
    pub max_tol: f64,
    pub max_util: f64,
}

impl EnergyAgg {
    #[allow(clippy::too_many_arguments)]
    fn record(&mut self, delta: i64, want: i32, q: f64, t: f64, r: f64, tol: f64, failed: bool) {
        self.count += 1;
        if failed {
            self.failures += 1;
        }
        self.max_delta = self.max_delta.max(delta);
        self.max_ref = self.max_ref.max((want as i64).abs());
        if want != 0 {
            self.max_rel = self.max_rel.max(delta as f64 / (want as f64).abs());
        }
        self.max_q = self.max_q.max(q);
        self.max_t = self.max_t.max(t);
        self.max_r = self.max_r.max(r);
        self.max_tol = self.max_tol.max(tol);
        if tol > 0.0 {
            self.max_util = self.max_util.max(delta as f64 / tol);
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct EnergyFailure {
    pub tag: String,
    pub kind: EnergyKind,
    pub short: bool,
    pub reused: bool,
    pub ch: usize,
    pub w: usize,
    pub b: usize,
    pub got: i32,
    pub want: i32,
    pub delta: i64,
    pub rel: f64,
    pub q: f64,
    pub t: f64,
    pub r: f64,
    pub tol: f64,
    pub util: f64,
}

/// Retained per-band envelope row (G-1 evidence retention, `docs/fpe/07`
/// §7.1). Recording only: populated for every compared band (supported
/// or not, passing or failing) with the exact terms the audit applied.
/// It never feeds `assert_clean`, aggregation, or any decision.
#[derive(Debug, Clone)]
pub(super) struct BandRow {
    pub tag: String,
    pub kind: EnergyKind,
    pub short: bool,
    pub reused: bool,
    pub ch: usize,
    /// Window index, or group index for grouped rows.
    pub w: usize,
    pub b: usize,
    pub got: i32,
    pub want: i32,
    pub delta: i64,
    pub q: f64,
    pub t: f64,
    pub r: f64,
    pub tol: f64,
    pub util: f64,
    pub failed: bool,
    pub supported: bool,
    pub unsupported_reason: Option<&'static str>,
    /// Single-band envelope representation state (`None` where no single
    /// `BandEnvelope` exists: TNS-reused and grouped-sum rows).
    pub scale_b: Option<i32>,
    pub scale0: Option<i32>,
    pub width: Option<usize>,
    pub sum_abs_x: Option<f64>,
}

#[derive(Debug, Clone)]
pub(super) struct ValueFailure {
    pub tag: String,
    pub field: &'static str,
    pub index: usize,
    pub got: i32,
    pub want: i32,
    pub tol: f64,
}

#[derive(Debug, Clone, Default)]
pub(super) struct LdAgg {
    pub count: usize,
    pub failures: usize,
    pub max_abs: f64,
    pub max_abs_tag: String,
    pub max_abs_got: f64,
    pub max_abs_want: f64,
}

impl LdAgg {
    fn record(&mut self, abs: f64, got: f64, want: f64, tag: String, failed: bool) {
        self.count += 1;
        if failed {
            self.failures += 1;
        }
        if abs > self.max_abs {
            self.max_abs = abs;
            self.max_abs_tag = tag;
            self.max_abs_got = got;
            self.max_abs_want = want;
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct LdFailure {
    pub tag: String,
    pub class: LdClass,
    pub got: f64,
    pub want: f64,
    pub e_got: f64,
    pub e_want: f64,
    pub tol: f64,
    pub abs_delta: f64,
}

#[derive(Debug, Clone, Default)]
pub(super) struct LdAudit {
    pub window_energy: LdAgg,
    pub grouped_energy: LdAgg,
    pub threshold: LdAgg,
    pub failures: Vec<LdFailure>,
}

impl LdAudit {
    fn agg_mut(&mut self, class: LdClass) -> &mut LdAgg {
        match class {
            LdClass::WindowEnergy => &mut self.window_energy,
            LdClass::GroupedEnergy => &mut self.grouped_energy,
            LdClass::Threshold => &mut self.threshold,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct GateAudit {
    pub checked: usize,
    pub failures: Vec<String>,
    pub max_residual: f64,
}

impl GateAudit {
    fn check(&mut self, ok: bool, residual: f64, message: impl FnOnce() -> String) {
        self.checked += 1;
        self.max_residual = self.max_residual.max(residual.abs());
        if !ok {
            self.failures.push(message());
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct DecompAudit {
    pub checked: usize,
    pub failures: Vec<String>,
    pub max_residual: f64,
    pub max_mapping: f64,
    pub max_trunc: f64,
    pub max_boundary: f64,
}

impl DecompAudit {
    fn check(
        &mut self,
        ok: bool,
        residual: f64,
        mapping: f64,
        trunc: f64,
        boundary: f64,
        message: impl FnOnce() -> String,
    ) {
        self.checked += 1;
        self.max_residual = self.max_residual.max(residual.abs());
        self.max_mapping = self.max_mapping.max(mapping.abs());
        self.max_trunc = self.max_trunc.max(trunc.abs());
        self.max_boundary = self.max_boundary.max(boundary.abs());
        if !ok {
            self.failures.push(message());
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct IdentityAudit {
    pub q31: GateAudit,
    pub fpe: GateAudit,
    pub fpe_ld: GateAudit,
    pub decomp: DecompAudit,
}

impl IdentityAudit {
    fn failure_count(&self) -> usize {
        self.q31.failures.len()
            + self.fpe.failures.len()
            + self.fpe_ld.failures.len()
            + self.decomp.failures.len()
    }
}

#[derive(Debug, Clone)]
pub(super) struct SentinelCase {
    pub tag: String,
    pub ch: usize,
    pub w: usize,
    pub b: usize,
    pub got_e: i32,
    pub want_e: i32,
    pub got_ld: f64,
    pub want_ld: f64,
    pub maxscale: i32,
    pub scale0: i32,
    pub scale_b: i32,
    pub width: usize,
    pub sum_abs_x: f64,
}

impl SentinelCase {
    fn energy_delta(&self) -> i64 {
        (self.got_e as i64 - self.want_e as i64).abs()
    }
    fn ld_delta(&self) -> f64 {
        (self.got_ld - self.want_ld).abs()
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct SentinelAudit {
    pub long_a: usize,
    pub short_a: usize,
    pub long_b: usize,
    pub short_b: usize,
    pub long_spread_a: usize,
    pub short_spread_a: usize,
    pub long_spread_b: usize,
    pub short_spread_b: usize,
    pub grouped_b: usize,
    pub grouped_a: usize,
    pub a_cases: Vec<SentinelCase>,
    pub b_cases: Vec<SentinelCase>,
    pub grouped_b_cases: Vec<SentinelCase>,
    pub grouped_a_cases: Vec<SentinelCase>,
}

/// Per-channel frame context needed by the envelope/identity gates.
///
/// `x` is the **pre-rescale** spectrum (before TNS filtering); the engines'
/// mutated copies are not used for envelope derivation.
pub(super) struct ChannelEnergyCtx<'a> {
    pub tag: String,
    pub ch: usize,
    pub short: bool,
    pub active: usize,
    pub grouping: (u8, [u8; 4]),
    pub x: &'a [f64],
    pub tns_active: [bool; 8],
    pub conf: &'a SfbConfig,
}

/// Complete audit accumulator (test-only).
#[derive(Debug, Clone, Default)]
pub(super) struct EnergyAudit {
    pub long_rescale: EnergyAgg,
    pub short_rescale: EnergyAgg,
    pub long_spread: EnergyAgg,
    pub short_spread: EnergyAgg,
    pub grouped: EnergyAgg,
    /// Retained per-band envelope rows (G-1 evidence; recording only,
    /// no aggregation or gate semantics).
    pub rows: Vec<BandRow>,
    pub failures: Vec<EnergyFailure>,
    pub value_failures: Vec<ValueFailure>,
    pub unsupported: Vec<String>,
    pub ld: LdAudit,
    pub identity: IdentityAudit,
    pub sentinels: SentinelAudit,
    /// Threshold comparisons where `C_RATIO·tol_E + 1 > q31_tol`; the
    /// larger derived bound is used and reported explicitly (`05` §9.3).
    pub threshold_prop_widened: usize,
    pub max_threshold_prop_excess: f64,
    /// Thresholds that would exceed `q31_tol` alone; the propagated bound
    /// covers them (reported, not hidden).
    pub threshold_q31_failures: usize,
    /// F-6 (R1a): short elements observed with element-global
    /// `tnsSpecShift != 0`. R1 does not validate the pre-rescale exponent
    /// reconstruction for this case; it must be covered in R2.
    pub short_tns_spec_shift_seen: usize,
}

struct EnergySample {
    kind: EnergyKind,
    short: bool,
    reused: bool,
    ch: usize,
    w: usize,
    b: usize,
    got: i32,
    want: i32,
    q: f64,
    t: f64,
    r: f64,
    tol: f64,
    unsupported: bool,
    unsupported_reason: Option<&'static str>,
    /// Single-band representation state for the retained row (`None`
    /// where no single `BandEnvelope` exists: TNS-reused and
    /// grouped-sum rows).
    scale_b: Option<i32>,
    scale0: Option<i32>,
    width: Option<usize>,
    sum_abs_x: Option<f64>,
    /// Contributing window span for grouped rows (`Some((first, one_past))`).
    group_windows: Option<(usize, usize)>,
}

impl EnergyAudit {
    fn agg_mut(&mut self, kind: EnergyKind, short: bool) -> &mut EnergyAgg {
        match (kind, short) {
            (EnergyKind::Rescale, false) => &mut self.long_rescale,
            (EnergyKind::Rescale, true) => &mut self.short_rescale,
            (EnergyKind::Spread, false) => &mut self.long_spread,
            (EnergyKind::Spread, true) => &mut self.short_spread,
            (EnergyKind::Grouped, _) => &mut self.grouped,
        }
    }

    fn record_energy(&mut self, ctx: &ChannelEnergyCtx<'_>, s: EnergySample) {
        // Grouped rows carry their contributing window span in the tag
        // (G-1 evidence: window→group mapping without re-derivation).
        // Tags are diagnostic strings; nothing parses them.
        let tag = if s.kind == EnergyKind::Grouped {
            match s.group_windows {
                Some((w0, w1)) => {
                    format!(
                        "{} ch{} g{}[w{}..{}] b{}",
                        ctx.tag, ctx.ch, s.w, w0, w1, s.b
                    )
                }
                None => format!("{} ch{} g{} b{}", ctx.tag, ctx.ch, s.w, s.b),
            }
        } else {
            format!("{} ch{} w{} b{}", ctx.tag, ctx.ch, s.w, s.b)
        };
        if s.unsupported {
            self.unsupported.push(format!(
                "{tag} {}: unsupported envelope ({})",
                s.kind.name(),
                s.unsupported_reason.unwrap_or("unspecified")
            ));
            self.agg_mut(s.kind, s.short).unsupported += 1;
            // Retain the row (unsupported, never counted as passing) so
            // G-1 can adjudicate every comparison explicitly.
            self.rows.push(BandRow {
                tag,
                kind: s.kind,
                short: s.short,
                reused: s.reused,
                ch: s.ch,
                w: s.w,
                b: s.b,
                got: s.got,
                want: s.want,
                delta: (s.got as i64 - s.want as i64).abs(),
                q: s.q,
                t: s.t,
                r: s.r,
                tol: s.tol,
                util: f64::NAN,
                failed: false,
                supported: false,
                unsupported_reason: s.unsupported_reason,
                scale_b: s.scale_b,
                scale0: s.scale0,
                width: s.width,
                sum_abs_x: s.sum_abs_x,
            });
            return;
        }
        // R2.0 R1a3 hardening (audit-only): a supported comparison must
        // carry a finite tolerance. Release-active on purpose: an infinite
        // tolerance would make `delta > tol` false and silently pass.
        assert!(
            s.tol.is_finite(),
            "{tag} {}: non-finite tolerance {t} must be classified unsupported, not compared",
            s.kind.name(),
            t = s.tol
        );
        let tol = s.tol.min(envelope::MAX_FINITE_ENERGY_TOL);
        let delta = (s.got as i64 - s.want as i64).abs();
        let rel = if s.want != 0 {
            delta as f64 / (s.want as f64).abs()
        } else {
            f64::NAN
        };
        let util = if tol > 0.0 {
            delta as f64 / tol
        } else {
            f64::INFINITY
        };
        let failed = delta as f64 > tol;
        self.agg_mut(s.kind, s.short)
            .record(delta, s.want, s.q, s.t, s.r, tol, failed);
        if failed {
            self.failures.push(EnergyFailure {
                tag: tag.clone(),
                kind: s.kind,
                short: s.short,
                reused: s.reused,
                ch: s.ch,
                w: s.w,
                b: s.b,
                got: s.got,
                want: s.want,
                delta,
                rel,
                q: s.q,
                t: s.t,
                r: s.r,
                tol,
                util,
            });
        }
        self.rows.push(BandRow {
            tag,
            kind: s.kind,
            short: s.short,
            reused: s.reused,
            ch: s.ch,
            w: s.w,
            b: s.b,
            got: s.got,
            want: s.want,
            delta,
            q: s.q,
            t: s.t,
            r: s.r,
            tol,
            util,
            failed,
            supported: true,
            unsupported_reason: None,
            scale_b: s.scale_b,
            scale0: s.scale0,
            width: s.width,
            sum_abs_x: s.sum_abs_x,
        });
    }

    fn record_value(
        &mut self,
        ctx: &ChannelEnergyCtx<'_>,
        field: &'static str,
        index: usize,
        got: i32,
        want: i32,
    ) {
        self.record_value_tol(ctx, field, index, got, want, q31_tol_f(want));
    }

    fn record_value_tol(
        &mut self,
        ctx: &ChannelEnergyCtx<'_>,
        field: &'static str,
        index: usize,
        got: i32,
        want: i32,
        tol: f64,
    ) {
        // R2.0 hardening: value tolerances are finite by construction
        // (`32 + |ref|/500`); fail loudly if that ever changes.
        assert!(
            tol.is_finite(),
            "{} ch{} {field}[{index}]: non-finite value tolerance",
            ctx.tag,
            ctx.ch
        );
        if (got as i64 - want as i64).abs() as f64 > tol {
            self.value_failures.push(ValueFailure {
                tag: format!("{} ch{}", ctx.tag, ctx.ch),
                field,
                index,
                got,
                want,
                tol,
            });
        }
    }

    fn compare_ld(
        &mut self,
        ctx: &ChannelEnergyCtx<'_>,
        class: LdClass,
        index_tag: String,
        got: f64,
        want: f64,
        tol: f64,
    ) {
        assert!(
            tol.is_finite(),
            "{} ch{} {index_tag}: non-finite Ld tolerance",
            ctx.tag,
            ctx.ch
        );
        let e_got = ld_to_e(got);
        let e_want = ld_to_e(want);
        let abs = (got - want).abs();
        let failed = (e_got - e_want).abs() > tol;
        let tag = format!("{} ch{} {index_tag}", ctx.tag, ctx.ch);
        self.ld
            .agg_mut(class)
            .record(abs, got, want, tag.clone(), failed);
        if failed {
            self.ld.failures.push(LdFailure {
                tag,
                class,
                got,
                want,
                e_got,
                e_want,
                tol,
                abs_delta: abs,
            });
        }
    }

    /// Run the full energy/Ld/value audit for one channel against its Q31
    /// reference. Exact representation fields are compared separately.
    #[allow(clippy::needless_range_loop)]
    pub fn compare_channel(
        &mut self,
        ctx: &ChannelEnergyCtx<'_>,
        got: &ChannelOutput,
        want: &ChannelOutput,
    ) {
        let short = ctx.short;
        let n_win = if short { 8 } else { 1 };
        let wlen = if short { 128 } else { 1024 };
        let active = ctx.active;
        let offsets = ctx.conf.offsets();
        let f = want.final_shift;
        if short && want.tns_shift != 0 {
            // F-6: the pre-rescale exponent reconstruction below is not
            // guaranteed for short-TNS with an element-global tnsSpecShift.
            // Record the condition; the short-TNS Ld gate is marked
            // unsupported rather than claimed as passing. R2 must test it.
            self.short_tns_spec_shift_seen += 1;
        }

        // --- Retained q31_tol fields (nm1; thresholds in the band loop) ---
        for b in 0..active {
            self.record_value(ctx, "nm1", b, got.nm1[b], want.nm1[b]);
        }
        if let (Some(g), Some(r)) = (&got.grouped, &want.grouped) {
            let sfb_cnt = ctx.conf.sfb_count();
            let no_sfb = ctx.grouping.0 as usize * sfb_cnt;
            for i in 0..no_sfb {
                self.record_value(
                    ctx,
                    "grouped-threshold",
                    i,
                    g.thresholds[i],
                    r.thresholds[i],
                );
            }
        }

        // Long threshold Ld (grouped threshold Ld is handled below).
        if let (Some(g), Some(r)) = (&got.threshold_ld, &want.threshold_ld) {
            for b in 0..active {
                let e_want = ld_to_e(r[b]);
                self.compare_ld(
                    ctx,
                    LdClass::Threshold,
                    format!("threshold-ld b{b}"),
                    g[b],
                    r[b],
                    32.0 + e_want / 500.0,
                );
            }
        }

        // All-zero shortcut: C blanks the energies/Ld and sets
        // mdctScale/finalShift to zero. Exact fields are compared elsewhere;
        // there is no float rescale energy to audit.
        let zero_element = want.mdct_scale == 0
            && want.final_shift == 0
            && want.energies.iter().all(|w| w.iter().all(|&e| e == 0))
            && got.energies.iter().all(|w| w.iter().all(|&e| e == 0));
        if zero_element {
            return;
        }

        let m = want.mdct_scale + f;
        // F-6 (R1a, tracked for R2): this reconstruction is valid for
        // non-TNS windows. For short-TNS windows it may be wrong when the
        // element-global tnsSpecShift != 0 (overflow rescale adjusts
        // mdct_scale). See docs/fpe/07 §5.2. R1 does not claim the case is
        // resolved; the short-TNS Ld gate is marked unsupported if observed.
        let mut mapped = Vec::with_capacity(n_win);
        for w in 0..n_win {
            mapped.push(envelope::map_window(
                &ctx.x[w * wlen..(w + 1) * wlen],
                offsets,
                active,
                m,
            ));
        }

        let mut window_bounds: Vec<Vec<f64>> = Vec::with_capacity(n_win);
        let mut window_envs: Vec<Vec<Option<BandEnvelope>>> = Vec::with_capacity(n_win);
        let mut window_supported: Vec<Vec<bool>> = Vec::with_capacity(n_win);

        for w in 0..n_win {
            let tns = ctx.tns_active[w];
            let mw = &mapped[w];
            let maxsc_pre: Vec<i32> = if tns {
                mw.max_scale.clone()
            } else {
                want.maxscale[w].iter().map(|&v| v + f).collect()
            };
            let mut bounds = Vec::with_capacity(active);
            let mut envs: Vec<Option<BandEnvelope>> = Vec::with_capacity(active);
            let mut supported_flags: Vec<bool> = Vec::with_capacity(active);

            for b in 0..active {
                let want_e = want.energies[w][b];
                let got_e = got.energies[w][b];
                let phys = band_energy(&ctx.x[w * wlen..(w + 1) * wlen], offsets, b);
                let short_tns_shift = short && tns && want.tns_shift != 0;
                // The rescale maxScale is the pre-TNS value even when TNS
                // later overwrites the output maxScale.
                let env = envelope::band_envelope(
                    &mw.x_q31,
                    offsets,
                    b,
                    f,
                    mw.max_scale[b],
                    mw.mapping_saturated,
                );
                // Rescale-energy reference for the §6.8 Ld: for short TNS
                // the Ld is the rescale value even though the stored energy
                // was recomputed; reconstruct it independently from the
                // pre-TNS mapped band. For non-TNS this is the reference
                // stored energy itself. (F-6: the short-TNS exponent
                // reconstruction is separately tracked for R2.)
                let ld_ref = if short && tns {
                    envelope::q31_stored_energy(&mw.x_q31, offsets, b, mw.max_scale[b], f)
                } else {
                    want_e
                };

                // Thresholds: retained q31_tol, but never weaker than the
                // propagated bound C_RATIO·tol_E + 1 (fmult floor). For TNS
                // windows the threshold is pre-halved and the rescale `m`
                // is not recoverable here; q31_tol is retained (conservative).
                let want_t = want.thresholds[w][b];
                let got_t = got.thresholds[w][b];
                let q_tol = q31_tol_f(want_t);
                // Propagated threshold bound from the ACTUAL measured energy
                // difference (not the worst-case envelope):
                // |Δfmult(E,C)| <= |ΔE|·C_RATIO + 1. For TNS windows the
                // threshold is pre-halved and the output energy is the Q31
                // recompute; q31_tol is retained there.
                let energy_delta = (got_e as i64 - want_e as i64).unsigned_abs() as f64;
                let prop = energy_delta * C_RATIO_F + 1.0;
                if prop > q_tol {
                    self.threshold_prop_widened += 1;
                    self.max_threshold_prop_excess =
                        self.max_threshold_prop_excess.max(prop - q_tol);
                }
                let tol_t = if tns { q_tol } else { q_tol.max(prop) };
                if (got_t as i64 - want_t as i64).abs() as f64 > q_tol {
                    self.threshold_q31_failures += 1;
                }
                self.record_value_tol(ctx, "threshold", w * active + b, got_t, want_t, tol_t);

                if tns {
                    // Reused Q31-kernel output (FPE-3.2 not implemented):
                    // retain q31_tol for the energy value.
                    let tol = q31_tol_f(want_e);
                    self.record_energy(
                        ctx,
                        EnergySample {
                            kind: EnergyKind::Rescale,
                            short,
                            reused: true,
                            ch: ctx.ch,
                            w,
                            b,
                            got: got_e,
                            want: want_e,
                            q: 0.0,
                            t: 0.0,
                            r: 0.0,
                            tol,
                            unsupported: false,
                            unsupported_reason: None,
                            // Reused-kernel row: no single envelope applies.
                            scale_b: None,
                            scale0: None,
                            width: None,
                            sum_abs_x: None,
                            group_windows: None,
                        },
                    );
                    bounds.push(tol);
                    // Short TNS keeps the rescale Ld; retain the envelope for
                    // the Ld comparison. Long TNS overwrites Ld too.
                    envs.push(if short { Some(env) } else { None });
                    supported_flags.push(true);
                } else {
                    let tol = env.tolerance(want_e);
                    let reason = env.unsupported_reason(got_e, want_e);
                    let supported = reason.is_none();
                    self.record_energy(
                        ctx,
                        EnergySample {
                            kind: EnergyKind::Rescale,
                            short,
                            reused: false,
                            ch: ctx.ch,
                            w,
                            b,
                            got: got_e,
                            want: want_e,
                            q: env.q,
                            t: env.t,
                            r: env.r,
                            tol,
                            unsupported: !supported,
                            unsupported_reason: reason,
                            scale_b: Some(env.scale_b),
                            scale0: Some(env.scale0),
                            width: Some(env.width),
                            sum_abs_x: Some(env.sum_abs_x),
                            group_windows: None,
                        },
                    );
                    bounds.push(tol);
                    envs.push(Some(env));
                    supported_flags.push(supported);

                    // --- Sentinel classes (actual stored state) ----------
                    if phys > 0.0 && env.sum_abs_x == 0.0 && want_e == 0 {
                        if short {
                            self.sentinels.short_a += 1;
                        } else {
                            self.sentinels.long_a += 1;
                        }
                        self.sentinels.a_cases.push(SentinelCase {
                            tag: ctx.tag.clone(),
                            ch: ctx.ch,
                            w,
                            b,
                            got_e,
                            want_e,
                            got_ld: got.energy_ld[w][b],
                            want_ld: want.energy_ld[w][b],
                            maxscale: mw.max_scale[b],
                            scale0: env.scale0,
                            scale_b: env.scale_b,
                            width: env.width,
                            sum_abs_x: env.sum_abs_x,
                        });
                    }
                    if env.sum_abs_x > 0.0 && want_e == 0 && got_e > 0 {
                        if short {
                            self.sentinels.short_b += 1;
                        } else {
                            self.sentinels.long_b += 1;
                        }
                        self.sentinels.b_cases.push(SentinelCase {
                            tag: ctx.tag.clone(),
                            ch: ctx.ch,
                            w,
                            b,
                            got_e,
                            want_e,
                            got_ld: got.energy_ld[w][b],
                            want_ld: want.energy_ld[w][b],
                            maxscale: mw.max_scale[b],
                            scale0: env.scale0,
                            scale_b: env.scale_b,
                            width: env.width,
                            sum_abs_x: env.sum_abs_x,
                        });
                    }

                    // --- Identity gates (non-TNS rescale energy) ---------
                    let q31_ind =
                        envelope::q31_stored_energy(&mw.x_q31, offsets, b, maxsc_pre[b], f);
                    self.identity
                        .q31
                        .check(mw.max_scale[b] == maxsc_pre[b], 0.0, || {
                            format!(
                                "{} ch{} w{} b{}: independent maxScale {} != output pre-shift {}",
                                ctx.tag, ctx.ch, w, b, mw.max_scale[b], maxsc_pre[b]
                            )
                        });
                    self.identity.q31.check(q31_ind == want_e, 0.0, || {
                        format!(
                            "{} ch{} w{} b{}: independent Q31 energy {q31_ind} != reference {want_e}",
                            ctx.tag, ctx.ch, w, b
                        )
                    });

                    let fpe_ind = fpe_stored_energy(phys, m, f);
                    self.identity.fpe.check(fpe_ind == got_e, 0.0, || {
                        format!(
                            "{} ch{} w{} b{}: independent FPE energy {fpe_ind} != output {got_e}",
                            ctx.tag, ctx.ch, w, b
                        )
                    });

                    // --- Decomposition term checks -----------------------
                    // The term bounds below are the substantive checks:
                    // |mapping| <= Q_b, |trunc| <= T_b + 1, |boundary| <= 0.5.
                    // `residual` is a consistency relation between the
                    // PRODUCTION delta (got_e - want_e) and those independent
                    // terms; when gates 1/2 pass it telescopes to zero. It is
                    // not claimed as an independent detector of arbitrary
                    // shared implementation errors.
                    if !env.shift_clamped {
                        let sum_x2_stored = phys * envelope::pow2(31 - 2 * (m - f));
                        let sum_q2_stored: f64 = mw.x_q31
                            [offsets[b] as usize..offsets[b + 1] as usize]
                            .iter()
                            .map(|&v| (v as f64) * (v as f64))
                            .sum::<f64>()
                            * envelope::pow2(2 * f - 31);
                        let delta_measured = got_e as f64 - want_e as f64;
                        let delta_mapping = sum_x2_stored - sum_q2_stored;
                        let delta_trunc = sum_q2_stored - q31_ind as f64;
                        let delta_boundary = fpe_ind as f64 - sum_x2_stored;
                        let residual =
                            delta_measured - (delta_mapping + delta_trunc + delta_boundary);
                        let ok = residual.abs() < 1e-6
                            && delta_mapping.abs() <= env.q + 1e-9
                            && delta_trunc.abs() <= env.t + 1.0 + 1e-9
                            && delta_boundary.abs() <= 0.5 + 1e-9;
                        self.identity.decomp.check(
                            ok,
                            residual,
                            delta_mapping,
                            delta_trunc,
                            delta_boundary,
                            || {
                                format!(
                                    "{} ch{} w{} b{}: decomposition term mismatch \
                                     residual {residual} (measured {delta_measured}, \
                                     map {delta_mapping}, trunc {delta_trunc}, \
                                     boundary {delta_boundary}; Q={} T={})",
                                    ctx.tag, ctx.ch, w, b, env.q, env.t
                                )
                            },
                        );
                    }
                }

                // Gate 2 Ld: FPE rescale Ld for long non-TNS and all short
                // windows (short TNS keeps the rescale Ld). Long TNS is the
                // reused Q31 recompute Ld and is excluded.
                if short || !tns {
                    if short_tns_shift {
                        self.unsupported.push(format!(
                            "{} ch{} w{} b{} Ld: F-6 short-TNS tnsSpecShift != 0 \
                             (pre-rescale exponent not validated in R1)",
                            ctx.tag, ctx.ch, w, b
                        ));
                    } else if let Some(reason) = env.unsupported_reason(ld_ref, ld_ref) {
                        if tns {
                            self.unsupported.push(format!(
                                "{} ch{} w{} b{} Ld: unsupported envelope ({reason})",
                                ctx.tag, ctx.ch, w, b
                            ));
                        }
                    } else {
                        let scale0 = (mw.max_scale[b] - 4).max(0);
                        let ld_ind = independent_ld(phys, m, scale0, f);
                        let ld_out = got.energy_ld[w][b];
                        self.identity.fpe_ld.check(
                            (ld_ind - ld_out).abs() < 1e-15,
                            ld_ind - ld_out,
                            || {
                                format!(
                                    "{} ch{} w{} b{}: independent Ld {ld_ind} != output {ld_out}",
                                    ctx.tag, ctx.ch, w, b
                                )
                            },
                        );
                    }
                }
            }

            // --- Spread energy: propagate the per-band bound ------------
            let (hi, lo) = ctx.conf.mask_factors_spr();
            let mut spread_bound = bounds.clone();
            for i in 1..active {
                spread_bound[i] = spread_bound[i].max(hi[i] * spread_bound[i - 1] + 1.0);
            }
            for i in (0..active.saturating_sub(1)).rev() {
                spread_bound[i] = spread_bound[i].max(lo[i] * spread_bound[i + 1] + 1.0);
            }
            for b in 0..active {
                let want_s = want.spread[w][b];
                let got_s = got.spread[w][b];
                let (q, t, r, rep) = match envs[b] {
                    Some(env) if !tns => (
                        env.q,
                        env.t,
                        env.r,
                        Some((env.scale_b, env.scale0, env.width, env.sum_abs_x)),
                    ),
                    _ => (0.0, 0.0, 0.0, None),
                };
                let (scale_b, scale0, width, sum_abs_x) = match rep {
                    Some((sb, s0, wd, sx)) => (Some(sb), Some(s0), Some(wd), Some(sx)),
                    None => (None, None, None, None),
                };
                let spread_reason = if supported_flags.iter().any(|&s| !s) {
                    Some("one or more input bands unsupported")
                } else {
                    None
                };
                self.record_energy(
                    ctx,
                    EnergySample {
                        kind: EnergyKind::Spread,
                        short,
                        reused: tns,
                        ch: ctx.ch,
                        w,
                        b,
                        got: got_s,
                        want: want_s,
                        q,
                        t,
                        r,
                        tol: spread_bound[b],
                        unsupported: spread_reason.is_some(),
                        unsupported_reason: spread_reason,
                        scale_b,
                        scale0,
                        width,
                        sum_abs_x,
                        group_windows: None,
                    },
                );
                if want_s == 0 && got_s > 0 {
                    if let Some(env) = envs[b] {
                        if !tns {
                            if env.sum_abs_x == 0.0
                                && band_energy(&ctx.x[w * wlen..], offsets, b) > 0.0
                            {
                                if short {
                                    self.sentinels.short_spread_a += 1;
                                } else {
                                    self.sentinels.long_spread_a += 1;
                                }
                            } else if env.sum_abs_x > 0.0 {
                                if short {
                                    self.sentinels.short_spread_b += 1;
                                } else {
                                    self.sentinels.long_spread_b += 1;
                                }
                            }
                        }
                    }
                }
            }

            // --- Per-window energy Ld -----------------------------------
            for b in 0..active {
                let got_ld = got.energy_ld[w][b];
                let want_ld = want.energy_ld[w][b];
                let short_tns_shift = short && tns && want.tns_shift != 0;
                let ld_ref = if short && tns {
                    envelope::q31_stored_energy(&mw.x_q31, offsets, b, mw.max_scale[b], f)
                } else {
                    want.energies[w][b]
                };
                if tns && !short {
                    // Long TNS overwrote Ld with the Q31 recompute Ld.
                    let e_want = ld_to_e(want_ld);
                    self.compare_ld(
                        ctx,
                        LdClass::WindowEnergy,
                        format!("w{w} b{b} (tns-reused)"),
                        got_ld,
                        want_ld,
                        32.0 + e_want / 500.0,
                    );
                } else {
                    let env = envs[b].expect("non-long-TNS band has an envelope");
                    if short_tns_shift {
                        self.unsupported.push(format!(
                            "{} ch{} w{} b{} Ld: F-6 short-TNS tnsSpecShift != 0 \
                             (pre-rescale exponent not validated in R1)",
                            ctx.tag, ctx.ch, w, b
                        ));
                    } else if let Some(reason) = env.unsupported_reason(ld_ref, ld_ref) {
                        if tns {
                            self.unsupported.push(format!(
                                "{} ch{} w{} b{} Ld: unsupported envelope ({reason})",
                                ctx.tag, ctx.ch, w, b
                            ));
                        }
                    } else {
                        self.compare_ld(
                            ctx,
                            LdClass::WindowEnergy,
                            format!("w{w} b{b}"),
                            got_ld,
                            want_ld,
                            env.tolerance(ld_ref),
                        );
                    }
                }
            }

            window_bounds.push(bounds);
            window_envs.push(envs);
            window_supported.push(supported_flags);
        }

        // --- Grouped energies and grouped Ld (short only) ---------------
        if short {
            if let (Some(g), Some(r)) = (&got.grouped, &want.grouped) {
                self.compare_grouped(
                    ctx,
                    g,
                    r,
                    &window_bounds,
                    &window_envs,
                    &window_supported,
                    active,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
    fn compare_grouped(
        &mut self,
        ctx: &ChannelEnergyCtx<'_>,
        got: &GroupedOutput,
        want: &GroupedOutput,
        window_bounds: &[Vec<f64>],
        window_envs: &[Vec<Option<BandEnvelope>>],
        window_supported: &[Vec<bool>],
        active: usize,
    ) {
        let sfb_cnt = ctx.conf.sfb_count();
        let (no_groups, lens) = ctx.grouping;
        let wlen = 128usize;
        let mut wnd = 0usize;
        for grp in 0..no_groups as usize {
            let gl = lens[grp] as usize;
            for b in 0..active {
                let gi = grp * sfb_cnt + b;
                let want_g = want.energies[gi];
                let got_g = got.energies[gi];
                let mut bound = 0.0;
                let mut q = 0.0;
                let mut t = 0.0;
                let mut r = 0.0;
                let mut reused = false;
                let mut unsupported_reason: Option<&'static str> =
                    if want_g == i32::MAX || got_g == i32::MAX {
                        Some("grouped nrgAddSaturate saturation")
                    } else {
                        None
                    };
                for j in 0..gl {
                    let w = wnd + j;
                    bound += window_bounds[w][b];
                    if !window_supported[w][b] {
                        unsupported_reason.get_or_insert("one or more grouped windows unsupported");
                    }
                    match window_envs[w][b] {
                        Some(env) => {
                            q += env.q;
                            t += env.t;
                            r += env.r;
                        }
                        None => reused = true,
                    }
                }
                self.record_energy(
                    ctx,
                    EnergySample {
                        kind: EnergyKind::Grouped,
                        short: true,
                        reused,
                        ch: ctx.ch,
                        w: grp,
                        b,
                        got: got_g,
                        want: want_g,
                        q,
                        t,
                        r,
                        tol: bound,
                        unsupported: unsupported_reason.is_some(),
                        unsupported_reason,
                        // Grouped sums have no single-band envelope state;
                        // contributing window rows carry their own terms.
                        scale_b: None,
                        scale0: None,
                        width: None,
                        sum_abs_x: None,
                        group_windows: Some((wnd, wnd + gl)),
                    },
                );

                if want_g == 0 && got_g > 0 {
                    // Classify by the actual mapped/physical state, not by
                    // Ld crossing: all-mapped-zero => Class A; any mapped
                    // line nonzero => Class B.
                    let mut all_x_zero = true;
                    let mut any_x_nonzero = false;
                    let mut any_phys = false;
                    for j in 0..gl {
                        let w = wnd + j;
                        if let Some(env) = window_envs[w][b] {
                            all_x_zero &= env.sum_abs_x == 0.0;
                            any_x_nonzero |= env.sum_abs_x > 0.0;
                        }
                        any_phys |=
                            band_energy(&ctx.x[w * wlen..(w + 1) * wlen], ctx.conf.offsets(), b)
                                > 0.0;
                    }
                    let case = SentinelCase {
                        tag: ctx.tag.clone(),
                        ch: ctx.ch,
                        w: grp,
                        b,
                        got_e: got_g,
                        want_e: want_g,
                        got_ld: got.energy_ld[gi],
                        want_ld: want.energy_ld[gi],
                        maxscale: 0,
                        scale0: 0,
                        scale_b: 0,
                        width: gl,
                        sum_abs_x: 0.0,
                    };
                    if all_x_zero && any_phys {
                        self.sentinels.grouped_a += 1;
                        self.sentinels.grouped_a_cases.push(case);
                    } else if any_x_nonzero {
                        self.sentinels.grouped_b += 1;
                        self.sentinels.grouped_b_cases.push(case);
                    }
                }

                // Grouped energy Ld: energy-domain inversion with the summed
                // per-window envelope (no extra grouped relative term). If a
                // grouped operand/window is unsupported, the bound is invalid;
                // the energy record already reports the unsupported state.
                if unsupported_reason.is_none() {
                    self.compare_ld(
                        ctx,
                        LdClass::GroupedEnergy,
                        format!("g{grp} b{b}"),
                        got.energy_ld[gi],
                        want.energy_ld[gi],
                        bound,
                    );
                }
                // Grouped threshold Ld retains the threshold envelope.
                let e_want = ld_to_e(want.threshold_ld[gi]);
                self.compare_ld(
                    ctx,
                    LdClass::Threshold,
                    format!("grouped-threshold g{grp} b{b}"),
                    got.threshold_ld[gi],
                    want.threshold_ld[gi],
                    32.0 + e_want / 500.0,
                );
            }
            wnd += gl;
        }
    }

    /// Merge another audit (full-corpus aggregation).
    pub fn merge(&mut self, other: &EnergyAudit) {
        merge_agg(&mut self.long_rescale, &other.long_rescale);
        merge_agg(&mut self.short_rescale, &other.short_rescale);
        merge_agg(&mut self.long_spread, &other.long_spread);
        merge_agg(&mut self.short_spread, &other.short_spread);
        merge_agg(&mut self.grouped, &other.grouped);
        self.rows.extend(other.rows.iter().cloned());
        self.failures.extend(other.failures.iter().cloned());
        self.value_failures
            .extend(other.value_failures.iter().cloned());
        self.unsupported.extend(other.unsupported.iter().cloned());
        merge_ld(&mut self.ld.window_energy, &other.ld.window_energy);
        merge_ld(&mut self.ld.grouped_energy, &other.ld.grouped_energy);
        merge_ld(&mut self.ld.threshold, &other.ld.threshold);
        self.ld.failures.extend(other.ld.failures.iter().cloned());
        merge_gate(&mut self.identity.q31, &other.identity.q31);
        merge_gate(&mut self.identity.fpe, &other.identity.fpe);
        merge_gate(&mut self.identity.fpe_ld, &other.identity.fpe_ld);
        self.identity.decomp.checked += other.identity.decomp.checked;
        self.identity.decomp.max_residual = self
            .identity
            .decomp
            .max_residual
            .max(other.identity.decomp.max_residual);
        self.identity.decomp.max_mapping = self
            .identity
            .decomp
            .max_mapping
            .max(other.identity.decomp.max_mapping);
        self.identity.decomp.max_trunc = self
            .identity
            .decomp
            .max_trunc
            .max(other.identity.decomp.max_trunc);
        self.identity.decomp.max_boundary = self
            .identity
            .decomp
            .max_boundary
            .max(other.identity.decomp.max_boundary);
        self.identity
            .decomp
            .failures
            .extend(other.identity.decomp.failures.iter().cloned());
        self.sentinels.long_a += other.sentinels.long_a;
        self.sentinels.short_a += other.sentinels.short_a;
        self.sentinels.long_b += other.sentinels.long_b;
        self.sentinels.short_b += other.sentinels.short_b;
        self.sentinels.long_spread_a += other.sentinels.long_spread_a;
        self.sentinels.short_spread_a += other.sentinels.short_spread_a;
        self.sentinels.long_spread_b += other.sentinels.long_spread_b;
        self.sentinels.short_spread_b += other.sentinels.short_spread_b;
        self.sentinels.grouped_b += other.sentinels.grouped_b;
        self.sentinels.grouped_a += other.sentinels.grouped_a;
        self.sentinels
            .a_cases
            .extend(other.sentinels.a_cases.iter().cloned());
        self.sentinels
            .b_cases
            .extend(other.sentinels.b_cases.iter().cloned());
        self.sentinels
            .grouped_b_cases
            .extend(other.sentinels.grouped_b_cases.iter().cloned());
        self.sentinels
            .grouped_a_cases
            .extend(other.sentinels.grouped_a_cases.iter().cloned());
        self.threshold_prop_widened += other.threshold_prop_widened;
        self.max_threshold_prop_excess = self
            .max_threshold_prop_excess
            .max(other.max_threshold_prop_excess);
        self.threshold_q31_failures += other.threshold_q31_failures;
        self.short_tns_spec_shift_seen += other.short_tns_spec_shift_seen;
    }

    /// Print the term-separated report (used by the full-corpus test).
    pub fn print_summary(&self, tag: &str) {
        println!("FPE-3.1R1 energy audit — {tag}");
        print_agg("long rescale", &self.long_rescale);
        print_agg("short rescale", &self.short_rescale);
        print_agg("long spread", &self.long_spread);
        print_agg("short spread", &self.short_spread);
        print_agg("grouped energy", &self.grouped);
        println!(
            "  unsupported comparisons: {} (energy), {} (total details)",
            self.long_rescale.unsupported
                + self.short_rescale.unsupported
                + self.long_spread.unsupported
                + self.short_spread.unsupported
                + self.grouped.unsupported,
            self.unsupported.len()
        );
        println!(
            "  Ld window-energy: n={} fail={} max|Δ|={:.9} at {} (got {:.9}, want {:.9})",
            self.ld.window_energy.count,
            self.ld.window_energy.failures,
            self.ld.window_energy.max_abs,
            self.ld.window_energy.max_abs_tag,
            self.ld.window_energy.max_abs_got,
            self.ld.window_energy.max_abs_want
        );
        println!(
            "  Ld grouped-energy: n={} fail={} max|Δ|={:.9} at {} (got {:.9}, want {:.9})",
            self.ld.grouped_energy.count,
            self.ld.grouped_energy.failures,
            self.ld.grouped_energy.max_abs,
            self.ld.grouped_energy.max_abs_tag,
            self.ld.grouped_energy.max_abs_got,
            self.ld.grouped_energy.max_abs_want
        );
        println!(
            "  Ld threshold: n={} fail={} max|Δ|={:.9} at {}",
            self.ld.threshold.count,
            self.ld.threshold.failures,
            self.ld.threshold.max_abs,
            self.ld.threshold.max_abs_tag
        );
        println!(
            "  sentinel classes: A(long {}, short {}) B(long {}, short {}) \
             grouped-A {} grouped-B {}; spread A(long {}, short {}) B(long {}, short {})",
            self.sentinels.long_a,
            self.sentinels.short_a,
            self.sentinels.long_b,
            self.sentinels.short_b,
            self.sentinels.grouped_a,
            self.sentinels.grouped_b,
            self.sentinels.long_spread_a,
            self.sentinels.short_spread_a,
            self.sentinels.long_spread_b,
            self.sentinels.short_spread_b
        );
        if let Some(c) = self
            .sentinels
            .a_cases
            .iter()
            .max_by_key(|c| c.energy_delta())
        {
            println!(
                "  max Class A: {} ch{} w{} b{} maxScale={} s0={} s={} width={} \
                 Σ|X|={:.1} E FPE/Q31={}/{} Ld FPE/Q31={:.9}/{:.9} (ΔLd {:.9})",
                c.tag,
                c.ch,
                c.w,
                c.b,
                c.maxscale,
                c.scale0,
                c.scale_b,
                c.width,
                c.sum_abs_x,
                c.got_e,
                c.want_e,
                c.got_ld,
                c.want_ld,
                c.ld_delta()
            );
        }
        if let Some(c) = self
            .sentinels
            .b_cases
            .iter()
            .max_by_key(|c| c.energy_delta())
        {
            println!(
                "  max Class B: {} ch{} w{} b{} maxScale={} s0={} s={} width={} \
                 Σ|X|={:.1} E FPE/Q31={}/{} Ld FPE/Q31={:.9}/{:.9} (ΔLd {:.9})",
                c.tag,
                c.ch,
                c.w,
                c.b,
                c.maxscale,
                c.scale0,
                c.scale_b,
                c.width,
                c.sum_abs_x,
                c.got_e,
                c.want_e,
                c.got_ld,
                c.want_ld,
                c.ld_delta()
            );
        }
        if let Some(c) = self
            .sentinels
            .grouped_a_cases
            .iter()
            .max_by_key(|c| c.energy_delta())
        {
            println!(
                "  max grouped Class A: {} ch{} g{} b{} E FPE/Q31={}/{} \
                 Ld FPE/Q31={:.9}/{:.9} (ΔLd {:.9})",
                c.tag,
                c.ch,
                c.w,
                c.b,
                c.got_e,
                c.want_e,
                c.got_ld,
                c.want_ld,
                c.ld_delta()
            );
        }
        if let Some(c) = self
            .sentinels
            .grouped_b_cases
            .iter()
            .max_by_key(|c| c.energy_delta())
        {
            println!(
                "  max grouped Class B: {} ch{} g{} b{} E FPE/Q31={}/{} \
                 Ld FPE/Q31={:.9}/{:.9} (ΔLd {:.9})",
                c.tag,
                c.ch,
                c.w,
                c.b,
                c.got_e,
                c.want_e,
                c.got_ld,
                c.want_ld,
                c.ld_delta()
            );
        }
        println!(
            "  threshold propagated-bound widenings: {} (max excess over q31_tol {:.6}); \
             thresholds outside q31_tol alone: {}",
            self.threshold_prop_widened,
            self.max_threshold_prop_excess,
            self.threshold_q31_failures
        );
        println!(
            "  F-6 short-TNS tnsSpecShift!=0 observations (R1 unvalidated): {}",
            self.short_tns_spec_shift_seen
        );
        println!(
            "  identity Q31: checked={} fail={} max_resid={:.3e}",
            self.identity.q31.checked,
            self.identity.q31.failures.len(),
            self.identity.q31.max_residual
        );
        println!(
            "  identity FPE: checked={} fail={} max_resid={:.3e}",
            self.identity.fpe.checked,
            self.identity.fpe.failures.len(),
            self.identity.fpe.max_residual
        );
        println!(
            "  identity FPE Ld: checked={} fail={} max_resid={:.3e}",
            self.identity.fpe_ld.checked,
            self.identity.fpe_ld.failures.len(),
            self.identity.fpe_ld.max_residual
        );
        println!(
            "  decomposition term checks: checked={} fail={} max_resid={:.3e} \
             max|map|={:.6} max|trunc|={:.6} max|boundary|={:.6}",
            self.identity.decomp.checked,
            self.identity.decomp.failures.len(),
            self.identity.decomp.max_residual,
            self.identity.decomp.max_mapping,
            self.identity.decomp.max_trunc,
            self.identity.decomp.max_boundary
        );
        println!(
            "  failures: energy={} value={} Ld={} identity={} unsupported={}",
            self.failures.len(),
            self.value_failures.len(),
            self.ld.failures.len(),
            self.identity.failure_count(),
            self.unsupported.len()
        );
    }

    /// Fail loudly after collecting every failure for the stream/corpus.
    pub fn assert_clean(&self, tag: &str) {
        let identity_failures = self.identity.failure_count();
        if self.failures.is_empty()
            && self.value_failures.is_empty()
            && self.ld.failures.is_empty()
            && identity_failures == 0
            && self.unsupported.is_empty()
        {
            return;
        }
        let mut msg = format!(
            "{tag}: FPE-3.1R1 audit failed: {} energy, {} value, {} Ld, \
             {} identity, {} unsupported comparisons\n",
            self.failures.len(),
            self.value_failures.len(),
            self.ld.failures.len(),
            identity_failures,
            self.unsupported.len()
        );
        for f in self.failures.iter().take(25) {
            msg.push_str(&format!(
                "  ENERGY {} {} {} {} ch{} w{} b{}: got {} want {} Δ{} \
                 rel {:.6e} tol {:.3} util {:.3} (Q {:.3} T {:.3} R {:.3})\n",
                f.tag,
                f.kind.name(),
                if f.short { "short" } else { "long" },
                if f.reused { "reused" } else { "value" },
                f.ch,
                f.w,
                f.b,
                f.got,
                f.want,
                f.delta,
                f.rel,
                f.tol,
                f.util,
                f.q,
                f.t,
                f.r
            ));
        }
        for f in self.value_failures.iter().take(10) {
            msg.push_str(&format!(
                "  VALUE {} {}[{}]: got {} want {} tol {:.3}\n",
                f.tag, f.field, f.index, f.got, f.want, f.tol
            ));
        }
        for f in self.ld.failures.iter().take(10) {
            msg.push_str(&format!(
                "  LD {} {}: got {:.9} want {:.9} (E {:.3} vs {:.3}, \
                 tol {:.3}, |ΔLd| {:.9})\n",
                f.tag,
                f.class.name(),
                f.got,
                f.want,
                f.e_got,
                f.e_want,
                f.tol,
                f.abs_delta
            ));
        }
        for f in self.unsupported.iter().take(10) {
            msg.push_str(&format!("  UNSUPPORTED {f}\n"));
        }
        for f in self.identity.q31.failures.iter().take(10) {
            msg.push_str(&format!("  ID-Q31 {f}\n"));
        }
        for f in self.identity.fpe.failures.iter().take(10) {
            msg.push_str(&format!("  ID-FPE {f}\n"));
        }
        for f in self.identity.fpe_ld.failures.iter().take(10) {
            msg.push_str(&format!("  ID-FPE-LD {f}\n"));
        }
        for f in self.identity.decomp.failures.iter().take(10) {
            msg.push_str(&format!("  ID-DECOMP {f}\n"));
        }
        panic!("{msg}");
    }
}

fn merge_agg(dst: &mut EnergyAgg, src: &EnergyAgg) {
    dst.count += src.count;
    dst.failures += src.failures;
    dst.unsupported += src.unsupported;
    dst.max_delta = dst.max_delta.max(src.max_delta);
    dst.max_ref = dst.max_ref.max(src.max_ref);
    dst.max_rel = dst.max_rel.max(src.max_rel);
    dst.max_q = dst.max_q.max(src.max_q);
    dst.max_t = dst.max_t.max(src.max_t);
    dst.max_r = dst.max_r.max(src.max_r);
    dst.max_tol = dst.max_tol.max(src.max_tol);
    dst.max_util = dst.max_util.max(src.max_util);
}

fn merge_ld(dst: &mut LdAgg, src: &LdAgg) {
    dst.count += src.count;
    dst.failures += src.failures;
    if src.max_abs > dst.max_abs {
        dst.max_abs = src.max_abs;
        dst.max_abs_tag = src.max_abs_tag.clone();
        dst.max_abs_got = src.max_abs_got;
        dst.max_abs_want = src.max_abs_want;
    }
}

fn merge_gate(dst: &mut GateAudit, src: &GateAudit) {
    dst.checked += src.checked;
    dst.max_residual = dst.max_residual.max(src.max_residual);
    dst.failures.extend(src.failures.iter().cloned());
}

fn print_agg(name: &str, a: &EnergyAgg) {
    println!(
        "  {name}: n={} fail={} unsupported={} max|Δ|={} max_ref={} \
         max_rel={:.6e} maxQ={:.6} maxT={:.6} maxR={:.3} max_tol={:.3} \
         max_util={:.4}",
        a.count,
        a.failures,
        a.unsupported,
        a.max_delta,
        a.max_ref,
        a.max_rel,
        a.max_q,
        a.max_t,
        a.max_r,
        a.max_tol,
        a.max_util
    );
}

pub(super) fn band_energy(x: &[f64], offsets: &[u16], b: usize) -> f64 {
    x[offsets[b] as usize..offsets[b + 1] as usize]
        .iter()
        .map(|v| v * v)
        .sum()
}

/// Independent FPE stored energy: `clamp(round(Σx²·2^(31−2m')))`.
pub(super) fn fpe_stored_energy(sum_x2: f64, m: i32, f: i32) -> i32 {
    let v = (sum_x2 * envelope::pow2(31 - 2 * (m - f))).round();
    v.clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

/// Independent `05` §6.8 rescale Ld formula.
pub(super) fn independent_ld(sum_x2: f64, m: i32, scale0: i32, f: i32) -> f64 {
    let e_raw_f = sum_x2 * envelope::pow2(-2 * m + 2 * scale0);
    let mut ld = if e_raw_f <= 0.0 {
        -1.0
    } else {
        e_raw_f.log2() / 64.0 - scale0 as f64 * 2.0 / 64.0
    };
    ld += f as f64 * 2.0 / 64.0;
    ld
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::sfb::{BlockKind, SfbConfig};

    fn ctx<'a>(conf: &'a SfbConfig) -> ChannelEnergyCtx<'a> {
        ChannelEnergyCtx {
            tag: "audit-test".to_string(),
            ch: 0,
            short: false,
            active: 1,
            grouping: (1, [1, 0, 0, 0]),
            x: &[],
            tns_active: [false; 8],
            conf,
        }
    }

    fn sample(unsupported: bool) -> EnergySample {
        EnergySample {
            kind: EnergyKind::Rescale,
            short: false,
            reused: false,
            ch: 0,
            w: 0,
            b: 0,
            got: 100,
            want: 100,
            q: 1.0,
            t: 1.0,
            r: 1.5,
            tol: 32.0,
            unsupported,
            unsupported_reason: unsupported.then_some("test accumulation wrap"),
            scale_b: Some(0),
            scale0: Some(0),
            width: Some(4),
            sum_abs_x: Some(100.0),
            group_windows: None,
        }
    }

    #[test]
    fn supported_comparison_records_and_passes() {
        let conf = SfbConfig::new(48000, BlockKind::Long, 96000, 19293, true, true).unwrap();
        let ctx = ctx(&conf);
        let mut audit = EnergyAudit::default();
        audit.record_energy(&ctx, sample(false));
        audit.assert_clean("supported");
        assert_eq!(audit.long_rescale.count, 1);
        assert_eq!(audit.long_rescale.unsupported, 0);
    }

    #[test]
    #[should_panic(expected = "unsupported")]
    fn unsupported_comparison_cannot_count_as_passing() {
        let conf = SfbConfig::new(48000, BlockKind::Long, 96000, 19293, true, true).unwrap();
        let ctx = ctx(&conf);
        let mut audit = EnergyAudit::default();
        audit.record_energy(&ctx, sample(true));
        assert_eq!(audit.long_rescale.count, 0, "unsupported not counted");
        assert_eq!(audit.long_rescale.unsupported, 1);
        assert_eq!(audit.unsupported.len(), 1);
        audit.assert_clean("unsupported");
    }
}
