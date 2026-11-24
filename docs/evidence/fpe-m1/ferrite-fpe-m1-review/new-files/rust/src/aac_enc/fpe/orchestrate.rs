//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-11-16.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! FPE per-pair frame orchestration (mirrors the `psy.rs` order).
//!
//! Target structure (`docs/fpe/04` §6): lowpass/zeroSpec → representation
//! lane (integer-exact, reused Q31 helpers) → value lane (f64 fraction
//! domain, M3+) → TNS (reused detect/sync/encode) → threshold advance →
//! MS energies → grouping → output assembly. The value lane never feeds
//! back into representation decisions (one-way dependency, ADR-0005).
//!
//! **M1 (scaffolding):** the whole per-pair computation delegates to the
//! Q31 engine ([`crate::aac_enc::psy::process_pair_reuse`]) operating on
//! the FPE-owned Q31-typed state and scratch. This is the bit-identical
//! pass-through baseline that proves the wiring before any float kernel
//! (roadmap `09` incremental strategy; ADR-0004 G0 precursor). No float
//! conversion, rounding, normalization, or rescaling is applied, and no
//! independent psychoacoustic state is computed.
//!
//! **M2 (FPE-2.2):** this module becomes a stage-by-stage mirror of
//! `psy.rs:495-1116` (lowpass/zeroSpec, representation lane via the reused
//! `energy.rs` helpers, TNS call sites, threshold-advance call sites, MS
//! call sites, grouping, output assembly), with each value kernel still
//! calling its Q31 helper. M3–M7 then replace one kernel group at a time
//! with the float implementations in `energy`/`thresholds`/`preecho`,
//! gated by the Q31↔FPE differential harness.
//!
//! Upstream counterpart: `FDKaacEnc_psyMain` (`psy_main.cpp:407-1298`);
//! Q31 mirror: `psy.rs:495-1116` (`docs/fpe/03` §2/§3).

use super::{FpeChannel, FpeScratch, FpeTables};
use crate::aac_enc::psy::{self, ChannelInput, ChannelOutput, MsOutput};

/// Per-pair orchestration entry (called by [`super::process_pair_reuse`]).
///
/// M1 delegates the complete value lane and representation lane to the Q31
/// engine; the returned outputs and the mutated spectra are the Q31
/// results, passed through unchanged.
pub(super) fn process_pair(
    left: &mut FpeChannel,
    right: Option<&mut FpeChannel>,
    lin: ChannelInput<'_>,
    rin: Option<ChannelInput<'_>>,
    tables: &FpeTables,
    ws: &mut FpeScratch,
) -> (ChannelOutput, Option<ChannelOutput>, Option<MsOutput>) {
    // Bit-identical pass-through (roadmap `09` M2 contract, applied to the
    // M1 skeleton): same kernels, same order, same state semantics — the
    // only difference from the Q31 dispatch arm is which encoder-owned
    // state/scratch instance is used. D-022 preconditions, the element-
    // global shift decisions, TNS sync, and the stale-on-short MS-Ld
    // semantics are inherited exactly from the delegated engine.
    psy::process_pair_reuse(
        &mut left.q31,
        right.map(|r| &mut r.q31),
        lin,
        rin,
        tables,
        &mut ws.q31,
    )
}
