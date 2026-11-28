//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! Floating-point psychoacoustic engine (FPE) — self-contained module tree.
//!
//! FPE is a numerical modernization of the existing Q31 psychoacoustic
//! engine (`super::psy` and its flat kernel modules), not a new
//! psychoacoustic model: it will replace the Q31 **value arithmetic** with
//! f64 arithmetic in the Q31-fraction domain (`v_f = v_st/2^31`) while
//! preserving every FDK algorithm, state transition, tuning constant, and
//! the integer representation lane (`mdctScale`/`finalShift`/`tnsShift`/
//! `maxScale`) bit-exactly. Planning authority: `docs/fpe/` (charter `00`,
//! architecture `04`, translation spec `05`, integration `06`, validation
//! `07`, roadmap `09`; ADR-0001/0002/0003/0004/0005).
//!
//! **M2 status (roadmap `09` M2, explicit orchestration mirror) and M3
//! (FPE-3.1 + FPE-3.2):** the FPE path runs its **own explicit per-pair
//! processing sequence** ([`orchestrate`]) that mirrors the Q31 order
//! stage-for-stage and operates on **FPE-owned state** ([`FpeChannel`]) and
//! **FPE-owned scratch** ([`FpeScratch`]). **FPE-3.1** replaces the
//! rescale-stage band energy value arithmetic with native f64 in the
//! Q31-fraction domain (`energy.rs`, `05` §6.1) and **FPE-3.2** replaces
//! the TNS recompute energies (`energy.rs`, `05` §6.2, mirror-then-replace
//! over the unchanged Q31 `tns_encode_channel`/`overflow_rescale`) and
//! activates the `05` §7 fraction-to-stored boundary for those energies;
//! every other value calculation still calls the existing Q31 kernels,
//! whose stored-int / f64-Ld results pass through **unchanged** (no
//! fraction conversion, no rounding, no rescaling). The Q31 engine remains
//! authoritative and the default; FPE is independently selectable via
//! [`super::config::PsyEngineKind`] for the in-binary Q31↔FPE differential
//! harness (`tests/`). Later float kernels arrive one group at a time in
//! M3–M7, each gated by that harness.
//!
//! Upstream counterpart: `FDKaacEnc_psyMain` (`psy_main.cpp:407-1298`,
//! FDK-AAC 3.0.0), factored exactly as the Q31 engine factors it
//! (`docs/fpe/03` §2). FPE adds no tonality/PNS/ATH (absent upstream in the
//! LC path, stay absent; `docs/fpe/03` §4.2/§4.5).

mod constants;
mod energy;
mod orchestrate;
mod output;
mod preecho;
mod thresholds;

#[cfg(test)]
mod tests;

use super::psy::{ChannelInput, ChannelOutput, MsOutput, PsyTables};
use super::thresholds::PreEchoState;
use super::tns::TnsChannel;

/// FPE init-time tables: the Q31 engine's [`PsyTables`] reused unchanged
/// (`docs/fpe/04` §3, `02` #33). FPE reads the same `SfbConfig` f64
/// accessors, `TnsConfig`, and minSnr data; no second table set exists.
pub(crate) type FpeTables = PsyTables;

/// Persistent per-channel FPE state (`docs/fpe/04` §4, `06` §4).
///
/// **M2 (FPE-2.1):** the individual state fields are owned directly by FPE —
/// pre-echo history (`thresholds::PreEchoState`, stored-int), TNS
/// runtime/output (`tns::TnsChannel` pair), and the MS-Ld history
/// (`[[f64; 64]; 8]`, stale-on-short). This mirrors [`super::psy::PsyChannel`]
/// field-for-field but is a distinct, independently owned instance: the Q31
/// kernels are called with **this** state, never with the Q31 engine's
/// `PsyChannel`, so there is no accidental sharing of mutable Q31 state.
/// Initialization mirrors `PsyChannel::new` exactly, and the in-frame
/// transitions (pre-echo STOP/START resets, TNS runtime clear, MS-Ld
/// write-on-long/stale-on-short) are driven by the FPE orchestration mirror.
///
/// M2 holds the **Q31 `PreEchoState`** (stored-int history) and delegates
/// pre-echo control to it, so the history is bit-identical to Q31; the
/// fraction `FpePreEchoState` is introduced in M6 (FPE-6.1). TNS state stays
/// the reused `TnsChannel` throughout. No fraction conversion and no
/// independent floating-point state calculation (roadmap `09` M2
/// pass-through contract; risk R-19).
pub(crate) struct FpeChannel {
    /// Pre-echo temporal-masking history (stored-int in M2). Owned by FPE.
    preecho: PreEchoState,
    /// TNS runtime decisions (reused type, same semantics). Owned by FPE.
    tns_state: TnsChannel,
    /// TNS output info (reused type). Owned by FPE.
    tns_info: TnsChannel,
    /// M/S log-energy history (this channel's half: mid on ch0, side on
    /// ch1), including the stale-on-short contents. Mirrors
    /// `PsyChannel.ms_ld` / C `sfbEnergyMSLdData`. Owned by FPE.
    ms_ld: [[f64; 64]; 8],
}

impl FpeChannel {
    /// Fresh state; mirrors [`super::psy::PsyChannel::new`] exactly
    /// (pre-echo seeded with the long PCM floors, `prev_scale = 8`, enabled;
    /// TNS zeroed; MS-Ld history zeros). `pcm_q31_long` is the long PCM floor
    /// vector.
    pub(crate) fn new(pcm_q31_long: &[i32]) -> Self {
        Self {
            preecho: PreEchoState::new(pcm_q31_long),
            tns_state: TnsChannel::new(),
            tns_info: TnsChannel::new(),
            ms_ld: [[0.0; 64]; 8],
        }
    }

    /// TNS output info for the frontend `LiveChannel` seam (same contract
    /// as [`super::psy::PsyChannel::tns_info`]).
    pub(crate) fn tns_info(&self) -> &TnsChannel {
        &self.tns_info
    }

    /// Pre-echo previous thresholds (test inspection only: FPE differential
    /// harness persistent-state comparison, `docs/fpe/07` §3.1).
    #[cfg(test)]
    pub(crate) fn preecho_prev(&self) -> &[i32] {
        self.preecho.prev()
    }

    /// Pre-echo previous scale and enabled flag (test inspection only).
    #[cfg(test)]
    pub(crate) fn preecho_state(&self) -> (i32, bool) {
        (self.preecho.prev_scale(), self.preecho.is_enabled())
    }

    /// TNS runtime state (test inspection only).
    #[cfg(test)]
    pub(crate) fn tns_state(&self) -> &TnsChannel {
        &self.tns_state
    }

    /// M/S log-energy history, including stale-on-short contents (test
    /// inspection only).
    #[cfg(test)]
    pub(crate) fn ms_ld(&self) -> &[[f64; 64]; 8] {
        &self.ms_ld
    }
}

/// Encoder-owned reusable FPE working buffers (`docs/fpe/04` §5).
///
/// Mirrors the [`super::psy::PsyScratch`] discipline: one instance per
/// `StreamEncoder`, passed `&mut` into [`process_pair_reuse`], every field
/// reset to its exact pre-first-read contents by the orchestration mirror
/// and the delegated kernels; buffers that escape into outputs are not
/// reused. **M2 (FPE-2.2 decision):** FPE owns its own copy of the Q31
/// representation/value-lane int buffers (the `PsyScratch` field layout),
/// so the Q31 `PsyScratch` type and its instances are untouched and there is
/// no sharing of mutable scratch between engines. **M3 (FPE-3.1):** the
/// float value-lane buffers (`ef_l`/`ef_r`, fraction energy per window)
/// are added here; they are fully overwritten by the float kernel before
/// the boundary conversion and are never read on the all-zero shortcut. No
/// per-frame heap allocation beyond the existing output-`Vec` pattern, no
/// globals.
#[derive(Default)]
pub(crate) struct FpeScratch {
    /// Q31-mapped spectra of the integer lane (1024 ints).
    q_l: Vec<i32>,
    q_r: Vec<i32>,
    /// Post-threshold MS remap spectra (1024 ints, stereo only).
    ql_full: Vec<i32>,
    qr_full: Vec<i32>,
    /// Per-window TNS remap scratch (`tns_encode_channel`).
    qwin: Vec<i32>,
    /// `band_energy_long` pre-shift accumulations (per window).
    tns_pre: Vec<i32>,
    maxsc_l: Vec<Vec<i32>>,
    maxsc_r: Vec<Vec<i32>>,
    e_l: Vec<Vec<i32>>,
    e_r: Vec<Vec<i32>>,
    /// FPE-3.1 rescale fraction energies per window (`docs/fpe/05` §6.1);
    /// fully overwritten by the float kernel before the boundary
    /// conversion, never read on the all-zero shortcut.
    ef_l: Vec<Vec<f64>>,
    ef_r: Vec<Vec<f64>>,
    eld_l: Vec<Vec<f64>>,
    eld_r: Vec<Vec<f64>>,
    t_l: Vec<Vec<i32>>,
    t_r: Vec<Vec<i32>>,
    spread_l: Vec<Vec<i32>>,
    spread_r: Vec<Vec<i32>>,
    /// Zero MS vectors for mono elements (never written, mirroring C).
    ms_zeros_l: Vec<Vec<i32>>,
    ms_zeros_r: Vec<Vec<i32>>,
}

/// Process a mono element or one stereo pair through the FPE path.
///
/// Signature and output contract mirror [`super::psy::process_pair_reuse`]
/// exactly: both dispatch arms in `StreamEncoder::frontend` return the
/// identical `(ChannelOutput, Option<ChannelOutput>, Option<MsOutput>)`
/// tuple, and everything downstream (`LiveChannel`, IS/MS, PE, S2, S4, S6,
/// S8) is untouched (ADR-0001/0003).
///
/// **M2/M3 explicit orchestration mirror:** runs the FPE-owned per-pair
/// processing sequence ([`orchestrate::process_pair`]) over FPE-owned state
/// and scratch. **FPE-3.1 + FPE-3.2** replace the rescale band-energy and the
/// TNS recompute energy value arithmetic with f64 and activate the
/// `docs/fpe/05` §7 boundary conversion for those energies; every other
/// value calculation still calls the existing Q31 kernels, whose stored-int
/// / f64-Ld results and the mutated f64 spectrum pass through unchanged.
/// D-022 preconditions are asserted at entry, identically to the Q31 engine.
pub(crate) fn process_pair_reuse(
    left: &mut FpeChannel,
    right: Option<&mut FpeChannel>,
    lin: ChannelInput<'_>,
    rin: Option<ChannelInput<'_>>,
    tables: &FpeTables,
    ws: &mut FpeScratch,
) -> (ChannelOutput, Option<ChannelOutput>, Option<MsOutput>) {
    orchestrate::process_pair(left, right, lin, rin, tables, ws)
}
